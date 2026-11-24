use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::BuildHasher;

use chrono::{Datelike, Duration, NaiveDate};
use naratama_db::reference_repo::ReferenceRepo;
use naratama_db::season_repo::{RescheduleUpdate, SeasonRepo};
use naratama_model::{
    CompetitionConfig, CompetitionFormat, CompetitionType, FixtureMatch, MatchId, MatchStatus,
    NaratamaError, NaratamaResult, SeasonId, StadiumId, TeamId, TournamentId,
};
use naratama_sim::{
    config_for_tournament, synthetic_fallback_config, FixtureValidationReport, FixtureValidator,
    FixtureViolation,
};
use rusqlite::Connection;

use crate::international_calendar::InternationalCalendarService;

#[derive(Debug, Clone)]
pub struct FixtureRescheduleResult {
    pub success: bool,
    pub match_id: MatchId,
    pub old_date: Option<NaiveDate>,
    pub new_date: Option<NaiveDate>,
    pub reason: String,
    pub message: Option<String>,
}

/// Central calendar & conflict-resolution service owning fixture date mutations.
pub struct FixtureCalendarService;

impl FixtureCalendarService {
    pub const PREFERRED_REST_DAYS: i64 = 3;
    pub const DEFAULT_MIN_REST_DAYS: i64 = 2;

    pub fn resolve_season_conflicts(
        conn: &Connection,
        season_id: SeasonId,
        min_rest_days: i64,
        record_changes: bool,
        audit_date: NaiveDate,
    ) -> NaratamaResult<usize> {
        let mut total_moved = 0usize;
        for _ in 0..8 {
            let moved = Self::resolve_conflicts_pass(
                conn,
                season_id,
                min_rest_days,
                record_changes,
                audit_date,
            )?;
            let reordered = Self::enforce_round_order_pass(
                conn,
                season_id,
                min_rest_days,
                record_changes,
                audit_date,
            )?;
            total_moved += moved + reordered;
            if moved == 0 && reordered == 0 {
                break;
            }
        }
        Ok(total_moved)
    }

    pub fn resolve_config_for_tournament(
        conn: &Connection,
        tournament_id: TournamentId,
        season_start_year: Option<i32>,
    ) -> NaratamaResult<CompetitionConfig> {
        let season_repo = SeasonRepo::new(conn);
        if let Some(saved) =
            season_repo.load_competition_config(tournament_id, season_start_year)?
        {
            return Ok(saved);
        }
        if let Some(tour) = ReferenceRepo::new(conn).tournament_by_id(tournament_id)? {
            return Ok(config_for_tournament(&tour, season_start_year));
        }
        Ok(synthetic_fallback_config(tournament_id, season_start_year))
    }

    #[allow(clippy::too_many_lines)]
    pub fn postpone_match(
        conn: &Connection,
        match_id: MatchId,
        reason: &str,
        explicit_date: Option<NaiveDate>,
        audit_date: NaiveDate,
    ) -> NaratamaResult<FixtureRescheduleResult> {
        let season_repo = SeasonRepo::new(conn);
        let Some(m) = season_repo.match_by_id(match_id)? else {
            return Ok(FixtureRescheduleResult {
                success: false,
                match_id,
                old_date: None,
                new_date: None,
                reason: reason.to_string(),
                message: Some("Fixture not found.".to_string()),
            });
        };
        if m.status == MatchStatus::Finished {
            return Ok(FixtureRescheduleResult {
                success: false,
                match_id,
                old_date: m.date,
                new_date: None,
                reason: reason.to_string(),
                message: Some("Completed fixtures cannot be postponed.".to_string()),
            });
        }

        let (season_start, season_end) = Self::season_bounds(conn, m.season_id)?;
        let mandatory =
            InternationalCalendarService::mandatory_unavailable_dates(conn, m.season_id)?;
        let mut config =
            Self::resolve_config_for_tournament(conn, m.tournament_id, Some(season_start.year()))?;
        combine_unavailable(&mut config, &mandatory);

        let all_matches = season_repo.matches_for_season(m.season_id)?;
        let (club_dates, stadium_dates) = build_occupancy_maps(&all_matches, Some(match_id));
        let rest_days = Self::DEFAULT_MIN_REST_DAYS.max(i64::from(config.min_rest_days));
        let preferred = explicit_date
            .or_else(|| m.date.map(|d| d + Duration::days(7)))
            .unwrap_or(season_start);
        let default_midweek = [2_u32, 3, 4];
        let weekdays = if config.allowed_weekdays.is_empty() {
            default_midweek.as_slice()
        } else {
            config.allowed_weekdays.as_slice()
        };

        let target = if let Some(req) = explicit_date {
            if date_is_available(
                req,
                m.home_team_id,
                m.away_team_id,
                &club_dates,
                rest_days,
                season_start,
                season_end,
                &config.unavailable_dates,
                m.stadium_id,
                &stadium_dates,
            ) {
                Some(req)
            } else {
                find_available_date(
                    req,
                    m.home_team_id,
                    m.away_team_id,
                    &club_dates,
                    season_start,
                    season_end,
                    rest_days,
                    Self::PREFERRED_REST_DAYS,
                    &config.unavailable_dates,
                    m.stadium_id,
                    &stadium_dates,
                    true,
                    Some(weekdays),
                )
            }
        } else {
            find_available_date(
                preferred,
                m.home_team_id,
                m.away_team_id,
                &club_dates,
                season_start,
                season_end,
                rest_days,
                Self::PREFERRED_REST_DAYS,
                &config.unavailable_dates,
                m.stadium_id,
                &stadium_dates,
                true,
                Some(weekdays),
            )
        };

        let Some(new_date) = target else {
            return Ok(FixtureRescheduleResult {
                success: false,
                match_id,
                old_date: m.date,
                new_date: None,
                reason: reason.to_string(),
                message: Some("No valid alternative date was found.".to_string()),
            });
        };

        let update = RescheduleUpdate {
            match_id,
            old_date: m.date,
            new_date,
            original_date: m.original_date.or(m.date).unwrap_or(new_date),
            reason: reason.to_string(),
        };
        season_repo.apply_reschedules(&[update], &[], audit_date)?;

        Ok(FixtureRescheduleResult {
            success: true,
            match_id,
            old_date: m.date,
            new_date: Some(new_date),
            reason: reason.to_string(),
            message: None,
        })
    }

    #[allow(clippy::too_many_lines)]
    pub fn validate_season(
        conn: &Connection,
        season_id: SeasonId,
    ) -> NaratamaResult<FixtureValidationReport> {
        let season_repo = SeasonRepo::new(conn);
        let fixtures = season_repo.matches_for_season(season_id)?;
        let start_year = season_repo.season_years(season_id)?.map(|(s, _)| s);

        let mut by_tournament: BTreeMap<TournamentId, Vec<FixtureMatch>> = BTreeMap::new();
        for f in &fixtures {
            by_tournament
                .entry(f.tournament_id)
                .or_default()
                .push(f.clone());
        }

        let mut violations: Vec<FixtureViolation> = Vec::new();
        let mut rest_by_comp: HashMap<TournamentId, i64> = HashMap::new();

        for (tid, tour_fixtures) in by_tournament {
            let Some(tour) = ReferenceRepo::new(conn).tournament_by_id(tid)? else {
                continue;
            };
            let config = Self::resolve_config_for_tournament(conn, tour.tournament_id, start_year)?;
            rest_by_comp.insert(tid, i64::from(config.min_rest_days));

            match config.format {
                CompetitionFormat::LeaguePhase => {
                    let phase0: Vec<FixtureMatch> = tour_fixtures
                        .iter()
                        .filter(|m| m.phase == 0)
                        .cloned()
                        .collect();
                    if !phase0.is_empty() {
                        let clubs = unique_clubs_from_fixtures(&phase0);
                        violations.extend(
                            FixtureValidator::validate_league_phase(
                                tid,
                                season_id,
                                &phase0,
                                &clubs,
                                config.league_phase_opponents.max(0) as usize,
                                config.league_phase_home_matches.max(0) as usize,
                                config.league_phase_away_matches.max(0) as usize,
                            )
                            .violations,
                        );
                    }
                    let ko: Vec<FixtureMatch> = tour_fixtures
                        .iter()
                        .filter(|m| m.phase > 0 && m.group_id.is_none())
                        .cloned()
                        .collect();
                    if !ko.is_empty() {
                        violations.extend(
                            FixtureValidator::validate_knockout(
                                tid,
                                season_id,
                                &ko,
                                Some(config.knockout_legs.max(1) as usize),
                                config.final_single_match,
                                config.use_byes,
                            )
                            .violations,
                        );
                    }
                }
                _ if config.is_league_like() => {
                    let clubs = unique_clubs_from_fixtures(&tour_fixtures);
                    violations.extend(
                        FixtureValidator::validate_league(
                            tid,
                            season_id,
                            &tour_fixtures,
                            &clubs,
                            config.double_round_robin,
                            "league",
                        )
                        .violations,
                    );
                }
                CompetitionFormat::GroupStage => {
                    let mut groups: BTreeMap<i32, Vec<FixtureMatch>> = BTreeMap::new();
                    for f in &tour_fixtures {
                        if let Some(gid) = f.group_id {
                            groups.entry(gid).or_default().push(f.clone());
                        }
                    }
                    for (gid, g_fixtures) in groups {
                        let clubs = unique_clubs_from_fixtures(&g_fixtures);
                        violations.extend(
                            FixtureValidator::validate_league(
                                tid,
                                season_id,
                                &g_fixtures,
                                &clubs,
                                config.group_double_round_robin,
                                &format!("group_{gid}"),
                            )
                            .violations,
                        );
                    }
                    let ko: Vec<FixtureMatch> = tour_fixtures
                        .iter()
                        .filter(|m| m.group_id.is_none())
                        .cloned()
                        .collect();
                    if !ko.is_empty() {
                        violations.extend(
                            FixtureValidator::validate_knockout(
                                tid,
                                season_id,
                                &ko,
                                Some(config.knockout_legs.max(1) as usize),
                                config.final_single_match,
                                config.use_byes,
                            )
                            .violations,
                        );
                    }
                }
                _ if config.competition_type == CompetitionType::Friendly => {
                    // Friendlies are standalone exhibition matches, not elimination brackets;
                    // calendar overlap, rest, and valid-date checks still run below.
                }
                _ => {
                    let ko: Vec<FixtureMatch> = tour_fixtures
                        .iter()
                        .filter(|m| m.group_id.is_none())
                        .cloned()
                        .collect();
                    if !ko.is_empty() {
                        violations.extend(
                            FixtureValidator::validate_knockout(
                                tid,
                                season_id,
                                &ko,
                                Some(config.knockout_legs.max(1) as usize),
                                config.final_single_match,
                                config.use_byes,
                            )
                            .violations,
                        );
                    }
                }
            }
        }

        violations.extend(
            FixtureValidator::validate_calendar(
                &fixtures,
                Self::DEFAULT_MIN_REST_DAYS,
                &rest_by_comp,
            )
            .violations,
        );
        Ok(FixtureValidationReport { violations })
    }

    #[allow(clippy::too_many_lines)]
    fn resolve_conflicts_pass(
        conn: &Connection,
        season_id: SeasonId,
        min_rest_days: i64,
        record_changes: bool,
        audit_date: NaiveDate,
    ) -> NaratamaResult<usize> {
        let season_repo = SeasonRepo::new(conn);
        let mut ordered: Vec<FixtureMatch> = season_repo
            .matches_for_season(season_id)?
            .into_iter()
            .filter(|m| m.status != MatchStatus::Cancelled && m.date.is_some())
            .collect();
        if ordered.is_empty() {
            return Ok(0);
        }

        ordered.sort_by_key(|m| (m.date, m.tournament_id, m.phase, m.round, m.match_id));

        let (season_start, season_end) = Self::season_bounds(conn, season_id)?;
        let mandatory = InternationalCalendarService::mandatory_unavailable_dates(conn, season_id)?;

        let mut configs: HashMap<TournamentId, CompetitionConfig> = HashMap::new();
        let mut club_dates: HashMap<TeamId, Vec<NaiveDate>> = HashMap::new();
        let mut stadium_dates: HashMap<StadiumId, Vec<NaiveDate>> = HashMap::new();
        let mut round_max_dates: HashMap<(TournamentId, i32, i32, i32), BTreeMap<i32, NaiveDate>> =
            HashMap::new();

        for m in &ordered {
            if m.status == MatchStatus::Finished {
                if let Some(d) = m.date {
                    club_dates.entry(m.home_team_id).or_default().push(d);
                    club_dates.entry(m.away_team_id).or_default().push(d);
                    if m.stadium_id.get() > 0 {
                        stadium_dates.entry(m.stadium_id).or_default().push(d);
                    }
                    let entry = round_max_dates
                        .entry(scope_key(m))
                        .or_default()
                        .entry(m.round)
                        .or_insert(d);
                    if d > *entry {
                        *entry = d;
                    }
                }
            }
        }

        let mut updates = Vec::new();
        let mut unresolved = Vec::new();
        let mut moved = 0usize;
        let midweek = [2_u32, 3, 4];

        for m in &ordered {
            if m.status == MatchStatus::Finished {
                continue;
            }
            let Some(parsed) = m.date else { continue };

            if let Entry::Vacant(entry) = configs.entry(m.tournament_id) {
                let mut cfg = Self::resolve_config_for_tournament(
                    conn,
                    m.tournament_id,
                    Some(season_start.year()),
                )?;
                combine_unavailable(&mut cfg, &mandatory);
                entry.insert(cfg);
            }
            let cfg = &configs[&m.tournament_id];
            let rest_days = min_rest_days.max(i64::from(cfg.min_rest_days));
            let unavailable = cfg.unavailable_dates.contains(&parsed);
            let clash = has_club_clash(&club_dates, m.home_team_id, m.away_team_id, parsed, 0);
            let rest_issue = has_club_clash(
                &club_dates,
                m.home_team_id,
                m.away_team_id,
                parsed,
                rest_days,
            );
            let stadium_issue = has_stadium_clash(&stadium_dates, m.stadium_id, parsed);

            let r_key = scope_key(m);
            let min_allowed = round_max_dates
                .get(&r_key)
                .and_then(|map| map.range(..m.round).map(|(_, &d)| d).max());
            let round_order_issue = min_allowed.is_some_and(|min_d| parsed < min_d);

            if !clash && !rest_issue && !stadium_issue && !unavailable && !round_order_issue {
                record_occupancy(
                    &mut club_dates,
                    &mut stadium_dates,
                    &mut round_max_dates,
                    m,
                    parsed,
                );
                continue;
            }

            let search_start = min_allowed.map_or(parsed, |min_d| parsed.max(min_d));
            let cup_like = m.group_id.is_none() && m.phase > 0;
            let allowed_weekdays = if !cfg.allowed_weekdays.is_empty() {
                Some(cfg.allowed_weekdays.as_slice())
            } else if cup_like {
                Some(midweek.as_slice())
            } else {
                None
            };

            let candidate = find_available_date(
                search_start,
                m.home_team_id,
                m.away_team_id,
                &club_dates,
                season_start,
                season_end,
                rest_days,
                Self::PREFERRED_REST_DAYS,
                &cfg.unavailable_dates,
                m.stadium_id,
                &stadium_dates,
                true,
                allowed_weekdays,
            )
            .or_else(|| {
                find_available_date_at_or_before(
                    search_start,
                    m.home_team_id,
                    m.away_team_id,
                    &club_dates,
                    min_allowed.map_or(season_start, |min_d| season_start.max(min_d)),
                    season_end,
                    rest_days,
                    &cfg.unavailable_dates,
                    m.stadium_id,
                    &stadium_dates,
                    allowed_weekdays,
                )
            });

            let Some(new_date) = candidate else {
                unresolved.push(RescheduleUpdate {
                    match_id: m.match_id,
                    old_date: Some(parsed),
                    new_date: parsed,
                    original_date: m.original_date.unwrap_or(parsed),
                    reason: "Unresolved calendar conflict: no valid alternative date.".to_string(),
                });
                record_occupancy(
                    &mut club_dates,
                    &mut stadium_dates,
                    &mut round_max_dates,
                    m,
                    parsed,
                );
                continue;
            };

            if record_changes {
                updates.push(RescheduleUpdate {
                    match_id: m.match_id,
                    old_date: Some(parsed),
                    new_date,
                    original_date: m.original_date.unwrap_or(parsed),
                    reason: "Calendar conflict / rest-period rule".to_string(),
                });
            }
            record_occupancy(
                &mut club_dates,
                &mut stadium_dates,
                &mut round_max_dates,
                m,
                new_date,
            );
            moved += 1;
        }

        if record_changes {
            season_repo.apply_reschedules(&updates, &unresolved, audit_date)?;
        }
        Ok(moved)
    }

    #[allow(clippy::too_many_lines)]
    fn enforce_round_order_pass(
        conn: &Connection,
        season_id: SeasonId,
        min_rest_days: i64,
        record_changes: bool,
        audit_date: NaiveDate,
    ) -> NaratamaResult<usize> {
        let season_repo = SeasonRepo::new(conn);
        let mut all_matches: Vec<FixtureMatch> = season_repo
            .matches_for_season(season_id)?
            .into_iter()
            .filter(|m| m.status != MatchStatus::Cancelled && m.date.is_some())
            .collect();
        if all_matches.is_empty() {
            return Ok(0);
        }

        let (season_start, season_end) = Self::season_bounds(conn, season_id)?;
        let mandatory = InternationalCalendarService::mandatory_unavailable_dates(conn, season_id)?;
        let mut configs: HashMap<TournamentId, CompetitionConfig> = HashMap::new();
        let midweek = [2_u32, 3, 4];

        let mut by_scope: ScopeRoundsMap = BTreeMap::new();
        for (idx, m) in all_matches.iter().enumerate() {
            by_scope
                .entry(scope_key(m))
                .or_default()
                .entry(m.round)
                .or_default()
                .push(idx);
        }

        let mut updates = Vec::new();
        let mut moved = 0usize;

        for rounds in by_scope.values() {
            let mut prev_round_max: Option<NaiveDate> = None;
            let mut prev_indices: Option<&Vec<usize>> = None;

            for indices in rounds.values() {
                let mut round_max = indices.iter().filter_map(|&i| all_matches[i].date).max();
                let current_min = indices.iter().filter_map(|&i| all_matches[i].date).min();

                // Pull offending previous-round fixtures earlier if later round starts before previous finishes.
                if let (Some(p_max), Some(c_min), Some(p_idxs)) =
                    (prev_round_max, current_min, prev_indices)
                {
                    if c_min < p_max {
                        let mut offenders: Vec<usize> = p_idxs
                            .iter()
                            .copied()
                            .filter(|&i| {
                                all_matches[i].status != MatchStatus::Finished
                                    && all_matches[i].date.is_some_and(|d| d > c_min)
                            })
                            .collect();
                        offenders.sort_by(|&a, &b| all_matches[b].date.cmp(&all_matches[a].date));

                        let mut pulled = false;
                        for idx in offenders {
                            let m = all_matches[idx].clone();
                            let Some(old_date) = m.date else { continue };
                            let cfg = get_or_load_config(
                                conn,
                                &mut configs,
                                m.tournament_id,
                                season_start.year(),
                                &mandatory,
                            )?;
                            let (club_dates, stadium_dates) =
                                build_occupancy_maps(&all_matches, Some(m.match_id));
                            let cup_like = m.group_id.is_none() && m.phase > 0;
                            let weekdays = if !cfg.allowed_weekdays.is_empty() {
                                Some(cfg.allowed_weekdays.as_slice())
                            } else if cup_like {
                                Some(midweek.as_slice())
                            } else {
                                None
                            };
                            if let Some(new_date) = find_available_date_at_or_before(
                                c_min,
                                m.home_team_id,
                                m.away_team_id,
                                &club_dates,
                                season_start,
                                season_end,
                                min_rest_days.max(i64::from(cfg.min_rest_days)),
                                &cfg.unavailable_dates,
                                m.stadium_id,
                                &stadium_dates,
                                weekdays,
                            ) {
                                all_matches[idx].date = Some(new_date);
                                if record_changes {
                                    updates.push(RescheduleUpdate {
                                        match_id: m.match_id,
                                        old_date: Some(old_date),
                                        new_date,
                                        original_date: m.original_date.unwrap_or(old_date),
                                        reason: "Round order enforced".to_string(),
                                    });
                                }
                                pulled = true;
                                moved += 1;
                            }
                        }
                        if pulled {
                            prev_round_max =
                                p_idxs.iter().filter_map(|&i| all_matches[i].date).max();
                        }
                    }
                }

                if let Some(p_max) = prev_round_max {
                    for &idx in indices {
                        let m = all_matches[idx].clone();
                        let Some(old_date) = m.date else { continue };
                        if old_date >= p_max || m.status == MatchStatus::Finished {
                            continue;
                        }
                        let cfg = get_or_load_config(
                            conn,
                            &mut configs,
                            m.tournament_id,
                            season_start.year(),
                            &mandatory,
                        )?;
                        let (club_dates, stadium_dates) =
                            build_occupancy_maps(&all_matches, Some(m.match_id));
                        let cup_like = m.group_id.is_none() && m.phase > 0;
                        let weekdays = if !cfg.allowed_weekdays.is_empty() {
                            Some(cfg.allowed_weekdays.as_slice())
                        } else if cup_like {
                            Some(midweek.as_slice())
                        } else {
                            None
                        };
                        if let Some(new_date) = find_available_date(
                            p_max,
                            m.home_team_id,
                            m.away_team_id,
                            &club_dates,
                            season_start,
                            season_end,
                            min_rest_days.max(i64::from(cfg.min_rest_days)),
                            Self::PREFERRED_REST_DAYS,
                            &cfg.unavailable_dates,
                            m.stadium_id,
                            &stadium_dates,
                            true,
                            weekdays,
                        ) {
                            all_matches[idx].date = Some(new_date);
                            if round_max.is_none_or(|rm| new_date > rm) {
                                round_max = Some(new_date);
                            }
                            if record_changes {
                                updates.push(RescheduleUpdate {
                                    match_id: m.match_id,
                                    old_date: Some(old_date),
                                    new_date,
                                    original_date: m.original_date.unwrap_or(old_date),
                                    reason: "Round order enforced".to_string(),
                                });
                            }
                            moved += 1;
                        }
                    }
                }

                prev_round_max = round_max;
                prev_indices = Some(indices);
            }
        }

        if record_changes && !updates.is_empty() {
            season_repo.apply_reschedules(&updates, &[], audit_date)?;
        }
        Ok(moved)
    }

    fn season_bounds(
        conn: &Connection,
        season_id: SeasonId,
    ) -> NaratamaResult<(NaiveDate, NaiveDate)> {
        let years = SeasonRepo::new(conn).season_years(season_id)?;
        let (start_year, end_year) = years.unwrap_or((2025, 2026));
        let start = NaiveDate::from_ymd_opt(start_year, 7, 1)
            .ok_or_else(|| NaratamaError::InvalidState("invalid season start".to_string()))?;
        let end = NaiveDate::from_ymd_opt(end_year, 6, 30)
            .ok_or_else(|| NaratamaError::InvalidState("invalid season end".to_string()))?;
        Ok((start, end))
    }
}

#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn find_available_date<S: BuildHasher, T: BuildHasher>(
    preferred: NaiveDate,
    home_team_id: TeamId,
    away_team_id: TeamId,
    club_dates: &HashMap<TeamId, Vec<NaiveDate>, S>,
    season_start: NaiveDate,
    season_end: NaiveDate,
    min_rest_days: i64,
    preferred_rest_days: i64,
    unavailable_dates: &[NaiveDate],
    stadium_id: StadiumId,
    stadium_dates: &HashMap<StadiumId, Vec<NaiveDate>, T>,
    forward_only: bool,
    allowed_weekdays: Option<&[u32]>,
) -> Option<NaiveDate> {
    let unavailable: HashSet<NaiveDate> = unavailable_dates.iter().copied().collect();
    let is_ok = |date: NaiveDate, rest: i64, enforce_weekdays: bool| -> bool {
        if date < season_start || date > season_end || unavailable.contains(&date) {
            return false;
        }
        if enforce_weekdays {
            if let Some(days) = allowed_weekdays {
                let wd = date.weekday().number_from_monday();
                if !days.contains(&wd) {
                    return false;
                }
            }
        }
        !has_club_clash(club_dates, home_team_id, away_team_id, date, rest)
            && !has_stadium_clash(stadium_dates, stadium_id, date)
    };

    for pass in 0..3 {
        let rest = if pass == 0 {
            min_rest_days.max(preferred_rest_days)
        } else {
            min_rest_days
        };
        let enforce_weekdays = pass < 2;
        for offset in 0..=180_i64 {
            let forward = preferred + Duration::days(offset);
            if is_ok(forward, rest, enforce_weekdays) {
                return Some(forward);
            }
            if forward_only || offset == 0 {
                continue;
            }
            let backward = preferred - Duration::days(offset);
            if is_ok(backward, rest, enforce_weekdays) {
                return Some(backward);
            }
        }
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn find_available_date_at_or_before(
    upper_bound: NaiveDate,
    home_team_id: TeamId,
    away_team_id: TeamId,
    club_dates: &HashMap<TeamId, Vec<NaiveDate>>,
    season_start: NaiveDate,
    season_end: NaiveDate,
    min_rest_days: i64,
    unavailable_dates: &[NaiveDate],
    stadium_id: StadiumId,
    stadium_dates: &HashMap<StadiumId, Vec<NaiveDate>>,
    allowed_weekdays: Option<&[u32]>,
) -> Option<NaiveDate> {
    let unavailable: HashSet<NaiveDate> = unavailable_dates.iter().copied().collect();
    for pass in 0..3 {
        let rest = if pass == 0 {
            min_rest_days.max(FixtureCalendarService::PREFERRED_REST_DAYS)
        } else {
            min_rest_days
        };
        let enforce_weekdays = pass < 2;
        for offset in 0..=180_i64 {
            let date = upper_bound - Duration::days(offset);
            if date < season_start {
                break;
            }
            if date > season_end || unavailable.contains(&date) {
                continue;
            }
            if enforce_weekdays {
                if let Some(days) = allowed_weekdays {
                    let wd = date.weekday().number_from_monday();
                    if !days.contains(&wd) {
                        continue;
                    }
                }
            }
            if !has_club_clash(club_dates, home_team_id, away_team_id, date, rest)
                && !has_stadium_clash(stadium_dates, stadium_id, date)
            {
                return Some(date);
            }
        }
    }
    None
}

fn get_or_load_config<'a>(
    conn: &Connection,
    cache: &'a mut HashMap<TournamentId, CompetitionConfig>,
    tournament_id: TournamentId,
    start_year: i32,
    mandatory: &[NaiveDate],
) -> NaratamaResult<&'a CompetitionConfig> {
    if let Entry::Vacant(entry) = cache.entry(tournament_id) {
        let mut cfg = FixtureCalendarService::resolve_config_for_tournament(
            conn,
            tournament_id,
            Some(start_year),
        )?;
        combine_unavailable(&mut cfg, mandatory);
        entry.insert(cfg);
    }
    Ok(&cache[&tournament_id])
}

type ScopeRoundsMap = BTreeMap<(TournamentId, i32, i32, i32), BTreeMap<i32, Vec<usize>>>;

fn combine_unavailable(cfg: &mut CompetitionConfig, mandatory: &[NaiveDate]) {
    if mandatory.is_empty() {
        return;
    }
    let mut set: HashSet<NaiveDate> = cfg.unavailable_dates.iter().copied().collect();
    set.extend(mandatory.iter().copied());
    let mut combined: Vec<NaiveDate> = set.into_iter().collect();
    combined.sort_unstable();
    cfg.unavailable_dates = combined;
}

const fn scope_key(m: &FixtureMatch) -> (TournamentId, i32, i32, i32) {
    (
        m.tournament_id,
        m.phase,
        match m.group_id {
            Some(g) => g,
            None => -1,
        },
        m.leg,
    )
}

fn record_occupancy(
    club_dates: &mut HashMap<TeamId, Vec<NaiveDate>>,
    stadium_dates: &mut HashMap<StadiumId, Vec<NaiveDate>>,
    round_max_dates: &mut HashMap<(TournamentId, i32, i32, i32), BTreeMap<i32, NaiveDate>>,
    m: &FixtureMatch,
    date: NaiveDate,
) {
    club_dates.entry(m.home_team_id).or_default().push(date);
    club_dates.entry(m.away_team_id).or_default().push(date);
    if m.stadium_id.get() > 0 {
        stadium_dates.entry(m.stadium_id).or_default().push(date);
    }
    let entry = round_max_dates
        .entry(scope_key(m))
        .or_default()
        .entry(m.round)
        .or_insert(date);
    if date > *entry {
        *entry = date;
    }
}

fn build_occupancy_maps(
    fixtures: &[FixtureMatch],
    excluding_match_id: Option<MatchId>,
) -> (
    HashMap<TeamId, Vec<NaiveDate>>,
    HashMap<StadiumId, Vec<NaiveDate>>,
) {
    let mut clubs: HashMap<TeamId, Vec<NaiveDate>> = HashMap::new();
    let mut stadiums: HashMap<StadiumId, Vec<NaiveDate>> = HashMap::new();
    for m in fixtures {
        if excluding_match_id == Some(m.match_id) || m.status == MatchStatus::Cancelled {
            continue;
        }
        let Some(d) = m.date else { continue };
        clubs.entry(m.home_team_id).or_default().push(d);
        clubs.entry(m.away_team_id).or_default().push(d);
        if m.stadium_id.get() > 0 {
            stadiums.entry(m.stadium_id).or_default().push(d);
        }
    }
    (clubs, stadiums)
}

fn has_club_clash<S: BuildHasher>(
    club_dates: &HashMap<TeamId, Vec<NaiveDate>, S>,
    home: TeamId,
    away: TeamId,
    date: NaiveDate,
    min_rest_days: i64,
) -> bool {
    for club in [home, away] {
        if let Some(dates) = club_dates.get(&club) {
            for &existing in dates {
                let gap = (existing - date).num_days().abs();
                if (min_rest_days == 0 && gap == 0) || (min_rest_days > 0 && gap < min_rest_days) {
                    return true;
                }
            }
        }
    }
    false
}

fn has_stadium_clash<S: BuildHasher>(
    stadium_dates: &HashMap<StadiumId, Vec<NaiveDate>, S>,
    stadium_id: StadiumId,
    date: NaiveDate,
) -> bool {
    if stadium_id.get() <= 0 {
        return false;
    }
    stadium_dates
        .get(&stadium_id)
        .is_some_and(|dates| dates.contains(&date))
}

#[allow(clippy::too_many_arguments)]
fn date_is_available(
    date: NaiveDate,
    home: TeamId,
    away: TeamId,
    club_dates: &HashMap<TeamId, Vec<NaiveDate>>,
    min_rest_days: i64,
    season_start: NaiveDate,
    season_end: NaiveDate,
    unavailable_dates: &[NaiveDate],
    stadium_id: StadiumId,
    stadium_dates: &HashMap<StadiumId, Vec<NaiveDate>>,
) -> bool {
    if date < season_start || date > season_end || unavailable_dates.contains(&date) {
        return false;
    }
    !has_club_clash(club_dates, home, away, date, min_rest_days)
        && !has_stadium_clash(stadium_dates, stadium_id, date)
}

fn unique_clubs_from_fixtures(fixtures: &[FixtureMatch]) -> Vec<TeamId> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for m in fixtures {
        if seen.insert(m.home_team_id) {
            out.push(m.home_team_id);
        }
        if seen.insert(m.away_team_id) {
            out.push(m.away_team_id);
        }
    }
    out
}
