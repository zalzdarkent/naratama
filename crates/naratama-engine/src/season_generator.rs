use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{Datelike, Duration, NaiveDate};
use naratama_db::reference_repo::ReferenceRepo;
use naratama_db::season_repo::{NewFixtureRow, SeasonRepo};
use naratama_model::{
    tournament_ids, CompetitionConfig, CompetitionFormat, CountryId, FixtureMatch, GroupTable,
    InternationalWindow, MatchStatus, NaratamaError, NaratamaResult, Season, SeasonId, StandingRow,
    Team, TeamId, Tournament, TournamentId,
};
use naratama_sim::{
    compute_standings, config_for_tournament, snake_groups, KnockoutScheduler,
    LeaguePhaseScheduler, MatchEngine, RoundRobinScheduler,
};
use rand::seq::SliceRandom;
use rand::Rng;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::fixture_calendar::{find_available_date, FixtureCalendarService};
use crate::international_calendar::InternationalCalendarService;

#[derive(Debug, Clone)]
pub struct GenerateCareerRequest {
    pub country_id: CountryId,
    pub league: Tournament,
    pub cup: Option<Tournament>,
    pub internationals: Vec<Tournament>,
    pub other_leagues: Vec<Tournament>,
    pub start_year: i32,
    pub user_team_id: Option<TeamId>,
    pub include_preseason_friendlies: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CareerPlan {
    pub season: Season,
    pub league_tournament_id: TournamentId,
    pub cup_tournament_id: Option<TournamentId>,
    pub league_rounds: i32,
    pub cup_rounds: usize,
    pub team_count: usize,
    pub match_count: usize,
    pub international_tournament_ids: Vec<TournamentId>,
    pub international_windows: Vec<InternationalWindow>,
}

pub struct SeasonGenerator;

impl SeasonGenerator {
    pub const DEFAULT_START_YEAR: i32 = 2025;

    #[allow(clippy::too_many_lines)]
    pub fn generate_career(
        conn: &Connection,
        req: &GenerateCareerRequest,
        rng: &mut impl Rng,
    ) -> NaratamaResult<CareerPlan> {
        let start_date = default_kickoff(req.start_year)?;
        let season_repo = SeasonRepo::new(conn);
        let ref_repo = ReferenceRepo::new(conn);

        let season = season_repo.insert_season(req.start_year)?;
        let windows = InternationalCalendarService::generate_windows_for_season(
            conn,
            season.season_id,
            req.start_year,
        )?;

        let mut all_domestic_leagues = vec![req.league.clone()];
        for other in &req.other_leagues {
            if other.tournament_id != req.league.tournament_id {
                all_domestic_leagues.push(other.clone());
            }
        }

        for tour in &all_domestic_leagues {
            season_repo.link_season_tournament(season.season_id, tour.tournament_id)?;
            persist_default_config(conn, tour, req.start_year)?;
        }
        if let Some(cup) = &req.cup {
            season_repo.link_season_tournament(season.season_id, cup.tournament_id)?;
            persist_default_config(conn, cup, req.start_year)?;
        }
        for tour in &req.internationals {
            season_repo.link_season_tournament(season.season_id, tour.tournament_id)?;
            persist_default_config(conn, tour, req.start_year)?;
        }

        let mut all_teams: HashMap<TeamId, Team> = HashMap::new();
        let mut domestic_by_tour: HashMap<TournamentId, Vec<Team>> = HashMap::new();
        for lg in &all_domestic_leagues {
            let mut div_teams = ref_repo.teams_for_division(lg.tournament_id)?;
            let max_teams = if lg.slots > 0 { lg.slots as usize } else { 24 };
            if div_teams.len() > max_teams {
                div_teams.sort_by(|a, b| {
                    b.power
                        .cmp(&a.power)
                        .then_with(|| a.team_id.cmp(&b.team_id))
                });
                div_teams.truncate(max_teams);
            }
            for tm in &div_teams {
                all_teams.insert(tm.team_id, tm.clone());
            }
            domestic_by_tour.insert(lg.tournament_id, div_teams);
        }

        let cup_teams = ref_repo.teams_for_country(req.country_id)?;
        for tm in &cup_teams {
            all_teams.insert(tm.team_id, tm.clone());
        }

        let mut sorted_internationals = req.internationals.clone();
        sorted_internationals.sort_by_key(|t| t.tournament_id);

        let mut intl_entrants: BTreeMap<TournamentId, Vec<TeamId>> = BTreeMap::new();
        let mut continental_invites: HashSet<TeamId> = HashSet::new();
        let mut qualifiers_cache: HashMap<(CountryId, Option<i64>), Vec<Team>> = HashMap::new();
        let mut feeder_cache: HashMap<(i64, TournamentId), Option<(TeamId, TeamId)>> =
            HashMap::new();

        for tour in &sorted_internationals {
            let is_super_or_global = is_super_or_global_cup(tour.tournament_id);
            let exclude = if is_super_or_global {
                HashSet::new()
            } else {
                continental_invites.clone()
            };
            let invited = select_international_teams(
                conn,
                tour,
                &exclude,
                Some(season.season_id),
                &mut qualifiers_cache,
                &mut feeder_cache,
                rng,
            )?;
            if invited.len() < 2 {
                continue;
            }
            let by_id: HashMap<TeamId, Team> =
                invited.iter().map(|tm| (tm.team_id, tm.clone())).collect();
            let cfg = FixtureCalendarService::resolve_config_for_tournament(
                conn,
                tour.tournament_id,
                Some(req.start_year),
            )?;
            let entrants = if matches!(
                cfg.format,
                CompetitionFormat::LeaguePhase | CompetitionFormat::GroupStage
            ) {
                seed_ids_by_power(&invited)
            } else {
                seed_power_of_two(&invited)
            };
            if !is_super_or_global {
                continental_invites.extend(entrants.iter().copied());
            }
            for &id in &entrants {
                if let Some(tm) = by_id.get(&id) {
                    all_teams.insert(id, tm.clone());
                }
            }
            intl_entrants.insert(tour.tournament_id, entrants);
        }

        let team_refs: Vec<&Team> = all_teams.values().collect();
        season_repo.register_season_teams(season.season_id, &team_refs)?;

        let mut match_rows: Vec<NewFixtureRow> = Vec::new();
        let mut user_league_rounds = 0i32;

        for lg in &all_domestic_leagues {
            let div_teams = domestic_by_tour
                .get(&lg.tournament_id)
                .map_or(&[][..], Vec::as_slice);
            let team_count = div_teams.len();
            let cfg = FixtureCalendarService::resolve_config_for_tournament(
                conn,
                lg.tournament_id,
                Some(req.start_year),
            )?;
            let double_round = cfg.double_round_robin;
            let rounds_per_leg = if team_count.is_multiple_of(2) {
                team_count.saturating_sub(1)
            } else {
                team_count
            };
            let total_rounds = if team_count >= 2 {
                (rounds_per_leg * if double_round { 2 } else { 1 }) as i32
            } else {
                0
            };
            if lg.tournament_id == req.league.tournament_id {
                user_league_rounds = total_rounds;
            }
            if team_count >= 2 {
                let ids: Vec<TeamId> = div_teams.iter().map(|t| t.team_id).collect();
                let slots = RoundRobinScheduler::schedule(&ids, double_round, true, None);
                for slot in slots {
                    if let (Some(home), Some(away)) =
                        (all_teams.get(&slot.home), all_teams.get(&slot.away))
                    {
                        match_rows.push(build_fixture_row(
                            season.season_id,
                            lg.tournament_id,
                            home,
                            away,
                            slot.round,
                            1,
                            slot.leg,
                            round_date(start_date, slot.round, &cfg)?,
                            None,
                            false,
                            false,
                            false,
                        ));
                    }
                }
            }
        }

        let mut cup_phases = 0usize;
        if let Some(cup) = &req.cup {
            if cup_teams.len() >= 4 {
                let cup_cfg = FixtureCalendarService::resolve_config_for_tournament(
                    conn,
                    cup.tournament_id,
                    Some(req.start_year),
                )?;
                let seeded = seed_ids_by_power(&cup_teams);
                let plan = KnockoutScheduler::plan_opening_round(
                    &seeded,
                    cup_cfg.use_byes,
                    cup_cfg.trim_to_power_of_two,
                    cup_cfg.field_size.map(|v| v.max(0) as usize),
                );
                cup_phases = plan.rounds_to_final();
                let date = cup_date(start_date, 1, user_league_rounds);
                schedule_cup_round_inner(
                    conn,
                    season.season_id,
                    cup.tournament_id,
                    &seeded,
                    1,
                    date,
                    &all_teams,
                    Some(&plan.pairs),
                    &plan.byes,
                    Some(req.start_year),
                )?;
            }
        }

        let mut intl_index = 0usize;
        let mut super_cup_index = 0i64;
        for (tid, entrants) in &intl_entrants {
            let is_super =
                *tid == tournament_ids::UEFA_SUPER_CUP || *tid == tournament_ids::AFC_SUPER_CUP;
            let start = if is_super {
                let s = start_date - Duration::days(4 + super_cup_index * 3);
                super_cup_index += 1;
                s
            } else {
                let s = international_date(start_date, intl_index);
                intl_index += 1;
                s
            };
            schedule_continental_opening(
                conn,
                season.season_id,
                *tid,
                entrants,
                start,
                &all_teams,
                req.start_year,
            )?;
        }

        let inserted_league_count = season_repo.insert_matches(&match_rows)?;

        if req.include_preseason_friendlies {
            if let Some(uid) = req.user_team_id {
                Self::schedule_pre_season_friendlies(
                    conn,
                    season.season_id,
                    req.start_year,
                    uid,
                    &all_teams,
                    rng,
                )?;
            }
        }

        // Bug #17 fix: always resolve season conflicts AFTER all fixtures (including friendlies) are inserted.
        FixtureCalendarService::resolve_season_conflicts(
            conn,
            season.season_id,
            FixtureCalendarService::DEFAULT_MIN_REST_DAYS,
            true,
            start_date,
        )?;
        season_repo.repair_self_matches(season.season_id)?;

        Ok(CareerPlan {
            season,
            league_tournament_id: req.league.tournament_id,
            cup_tournament_id: req.cup.as_ref().map(|c| c.tournament_id),
            league_rounds: user_league_rounds,
            cup_rounds: cup_phases,
            team_count: all_teams.len(),
            match_count: inserted_league_count,
            international_tournament_ids: intl_entrants.keys().copied().collect(),
            international_windows: windows,
        })
    }

    pub fn schedule_pre_season_friendlies(
        conn: &Connection,
        season_id: SeasonId,
        start_year: i32,
        user_team_id: TeamId,
        all_teams: &HashMap<TeamId, Team>,
        rng: &mut impl Rng,
    ) -> NaratamaResult<usize> {
        let Some(user_team) = all_teams.get(&user_team_id) else {
            return Ok(0);
        };
        let mut candidates: Vec<&Team> = all_teams
            .values()
            .filter(|t| t.team_id != user_team_id)
            .collect();
        if candidates.len() < 3 {
            return Ok(0);
        }
        candidates.sort_by_key(|t| t.team_id);
        candidates.shuffle(rng);

        let start = default_kickoff(start_year)?;
        let dates = [
            start - Duration::days(17),
            start - Duration::days(10),
            start - Duration::days(3),
        ];
        let mut rows = Vec::with_capacity(3);
        for (i, date) in dates.into_iter().enumerate() {
            let opp = candidates[i];
            let (home, away) = if i % 2 == 0 {
                (user_team, opp)
            } else {
                (opp, user_team)
            };
            rows.push(build_fixture_row(
                season_id,
                tournament_ids::PRE_SEASON_FRIENDLY,
                home,
                away,
                i as i32 + 1,
                0,
                1,
                date,
                None,
                false,
                false,
                false,
            ));
        }
        SeasonRepo::new(conn).insert_matches(&rows)
    }

    pub fn schedule_group_stage(
        conn: &Connection,
        season_id: SeasonId,
        tournament_id: TournamentId,
        entrants: &[TeamId],
        start: NaiveDate,
        all_teams: &HashMap<TeamId, Team>,
        config: &CompetitionConfig,
    ) -> NaratamaResult<usize> {
        let seeded_teams: Vec<Team> = entrants
            .iter()
            .filter_map(|id| all_teams.get(id).cloned())
            .collect();
        let sorted = seed_ids_by_power(&seeded_teams);
        let groups = snake_groups(
            &sorted,
            config.group_size.max(2) as usize,
            config.group_count.max(0) as usize,
        );
        if groups.is_empty() {
            return Ok(0);
        }

        let mut rows = Vec::new();
        for (g, group_ids) in groups.iter().enumerate() {
            let slots = RoundRobinScheduler::schedule(
                group_ids,
                config.group_double_round_robin,
                true,
                None,
            );
            for slot in slots {
                if let (Some(home), Some(away)) =
                    (all_teams.get(&slot.home), all_teams.get(&slot.away))
                {
                    rows.push(build_fixture_row(
                        season_id,
                        tournament_id,
                        home,
                        away,
                        slot.round,
                        0,
                        slot.leg,
                        round_date(start, slot.round, config)?,
                        Some(g as i32 + 1),
                        false,
                        false,
                        false,
                    ));
                }
            }
        }
        SeasonRepo::new(conn).insert_matches(&rows)?;
        Ok(groups.len())
    }

    pub fn schedule_league_phase(
        conn: &Connection,
        season_id: SeasonId,
        tournament_id: TournamentId,
        entrants: &[TeamId],
        start: NaiveDate,
        all_teams: &HashMap<TeamId, Team>,
        config: &CompetitionConfig,
    ) -> NaratamaResult<usize> {
        let seeded_teams: Vec<Team> = entrants
            .iter()
            .filter_map(|id| all_teams.get(id).cloned())
            .collect();
        let seeded = seed_ids_by_power(&seeded_teams);
        if seeded.len() < 2 {
            return Ok(0);
        }
        let assoc: HashMap<TeamId, CountryId> = seeded
            .iter()
            .filter_map(|&id| all_teams.get(&id).map(|t| (id, t.country_id)))
            .collect();
        let plan = LeaguePhaseScheduler::generate(
            &seeded,
            config.league_phase_opponents.max(0) as usize,
            Some(config.league_phase_home_matches.max(0) as usize),
            Some(&assoc),
        );

        let mut rows = Vec::with_capacity(plan.fixtures.len());
        for slot in plan.fixtures {
            if slot.home == slot.away {
                continue;
            }
            if let (Some(home), Some(away)) = (all_teams.get(&slot.home), all_teams.get(&slot.away))
            {
                rows.push(build_fixture_row(
                    season_id,
                    tournament_id,
                    home,
                    away,
                    slot.round,
                    0,
                    1,
                    round_date(start, slot.round, config)?,
                    None,
                    false,
                    false,
                    false,
                ));
            }
        }
        SeasonRepo::new(conn).insert_matches(&rows)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn schedule_cup_round(
        conn: &Connection,
        season_id: SeasonId,
        cup_tournament_id: TournamentId,
        survivors: &[TeamId],
        phase: i32,
        date: NaiveDate,
        all_teams: &HashMap<TeamId, Team>,
        pairs: Option<&[(TeamId, TeamId)]>,
        byes: &[TeamId],
    ) -> NaratamaResult<usize> {
        let start_year = SeasonRepo::new(conn)
            .season_years(season_id)?
            .map(|(s, _)| s);
        schedule_cup_round_inner(
            conn,
            season_id,
            cup_tournament_id,
            survivors,
            phase,
            date,
            all_teams,
            pairs,
            byes,
            start_year,
        )
    }

    #[allow(clippy::too_many_lines)]
    pub fn maybe_schedule_next_cup_round(
        conn: &Connection,
        season_id: SeasonId,
        cup_tournament_id: TournamentId,
        team_cache: &HashMap<TeamId, Team>,
        audit_date: NaiveDate,
    ) -> NaratamaResult<bool> {
        let season_repo = SeasonRepo::new(conn);
        if season_repo.remaining_matches(season_id, cup_tournament_id)? != 0 {
            return Ok(false);
        }

        let start_year = season_repo.season_years(season_id)?.map(|(s, _)| s);
        let cfg = FixtureCalendarService::resolve_config_for_tournament(
            conn,
            cup_tournament_id,
            start_year,
        )?;

        let (max_phase, ko_count) =
            season_repo.max_phase_and_knockout_count(season_id, cup_tournament_id)?;

        if cfg.format == CompetitionFormat::GroupStage && ko_count == 0 {
            let groups = Self::group_tables(conn, season_id, cup_tournament_id, team_cache)?;
            if !groups.is_empty() {
                let plan = KnockoutScheduler::plan_group_knockout(
                    &groups,
                    cfg.group_qualifiers_per_group.max(1) as usize,
                );
                if plan.is_empty() {
                    return Ok(false);
                }
                let mut qualifiers = Vec::new();
                for &(a, b) in &plan.pairs {
                    qualifiers.push(a);
                    qualifiers.push(b);
                }
                qualifiers.extend(plan.byes.iter().copied());
                let next_date = next_cup_date(conn, season_id, cup_tournament_id, &qualifiers)?;
                let scheduled = Self::schedule_cup_round(
                    conn,
                    season_id,
                    cup_tournament_id,
                    &qualifiers,
                    1,
                    next_date,
                    team_cache,
                    Some(&plan.pairs),
                    &plan.byes,
                )?;
                FixtureCalendarService::resolve_season_conflicts(
                    conn,
                    season_id,
                    FixtureCalendarService::DEFAULT_MIN_REST_DAYS,
                    true,
                    audit_date,
                )?;
                return Ok(scheduled > 0);
            }
        }

        if cfg.format == CompetitionFormat::LeaguePhase && ko_count == 0 {
            let table = Self::standings(conn, season_id, cup_tournament_id, None, team_cache)?;
            let qualifiers: Vec<TeamId> = table
                .into_iter()
                .take(cfg.league_phase_qualifiers.max(0) as usize)
                .map(|r| r.team_id)
                .collect();
            if qualifiers.len() < 2 {
                return Ok(false);
            }
            let next_date = next_cup_date(conn, season_id, cup_tournament_id, &qualifiers)?;
            let scheduled = Self::schedule_cup_round(
                conn,
                season_id,
                cup_tournament_id,
                &qualifiers,
                1,
                next_date,
                team_cache,
                None,
                &[],
            )?;
            FixtureCalendarService::resolve_season_conflicts(
                conn,
                season_id,
                FixtureCalendarService::DEFAULT_MIN_REST_DAYS,
                true,
                audit_date,
            )?;
            return Ok(scheduled > 0);
        }

        let Some(current_phase) = max_phase else {
            return Ok(false);
        };
        let winners = season_repo.latest_phase_winners(season_id, cup_tournament_id)?;
        let byes = season_repo.byes_for_phase(season_id, cup_tournament_id, current_phase)?;
        let mut survivors: Vec<TeamId> = Vec::with_capacity(byes.len() + winners.len());
        survivors.extend(byes.iter().copied());
        survivors.extend(winners);
        if !byes.is_empty() {
            survivors.sort_by(|a, b| {
                let pa = team_cache.get(a).map_or(0, |t| t.power);
                let pb = team_cache.get(b).map_or(0, |t| t.power);
                pb.cmp(&pa).then_with(|| a.cmp(b))
            });
        }
        if survivors.len() < 2 {
            return Ok(false);
        }

        let date = next_cup_date(conn, season_id, cup_tournament_id, &survivors)?;
        let next_pairs: Vec<(TeamId, TeamId)> = survivors
            .as_chunks::<2>()
            .0
            .iter()
            .filter_map(|&[a, b]| (a != b).then_some((a, b)))
            .collect();
        let explicit = (byes.is_empty() && next_pairs.len() * 2 == survivors.len())
            .then_some(next_pairs.as_slice());

        let scheduled = Self::schedule_cup_round(
            conn,
            season_id,
            cup_tournament_id,
            &survivors,
            current_phase + 1,
            date,
            team_cache,
            explicit,
            &[],
        )?;
        FixtureCalendarService::resolve_season_conflicts(
            conn,
            season_id,
            FixtureCalendarService::DEFAULT_MIN_REST_DAYS,
            true,
            audit_date,
        )?;
        Ok(scheduled > 0)
    }

    /// Computes standings for a competition/group.
    /// Fixes Bug #9: restricts `LeaguePhase` and `GroupStage` (when `group_id` is None) to `phase == 0`.
    pub fn standings(
        conn: &Connection,
        season_id: SeasonId,
        tournament_id: TournamentId,
        group_id: Option<i32>,
        team_cache: &HashMap<TeamId, Team>,
    ) -> NaratamaResult<Vec<StandingRow>> {
        let season_repo = SeasonRepo::new(conn);
        let start_year = season_repo.season_years(season_id)?.map(|(s, _)| s);
        let cfg =
            FixtureCalendarService::resolve_config_for_tournament(conn, tournament_id, start_year)?;
        let all_matches = season_repo.matches_for_tournament(season_id, tournament_id)?;

        let filtered: Vec<FixtureMatch> = all_matches
            .into_iter()
            .filter(|m| match group_id {
                Some(gid) => m.group_id == Some(gid),
                None => {
                    if matches!(
                        cfg.format,
                        CompetitionFormat::LeaguePhase | CompetitionFormat::GroupStage
                    ) {
                        m.phase == 0
                    } else {
                        true
                    }
                }
            })
            .collect();

        let mut clubs = Vec::new();
        let mut seen = HashSet::new();
        let mut names = HashMap::new();
        for m in &filtered {
            for id in [m.home_team_id, m.away_team_id] {
                if seen.insert(id) {
                    clubs.push(id);
                    if let Some(tm) = team_cache.get(&id) {
                        names.insert(id, tm.name.clone());
                    }
                }
            }
        }
        Ok(compute_standings(&clubs, &names, &filtered))
    }

    pub fn group_tables(
        conn: &Connection,
        season_id: SeasonId,
        tournament_id: TournamentId,
        team_cache: &HashMap<TeamId, Team>,
    ) -> NaratamaResult<Vec<GroupTable>> {
        let matches = SeasonRepo::new(conn).matches_for_tournament(season_id, tournament_id)?;
        let mut by_group: BTreeMap<i32, Vec<FixtureMatch>> = BTreeMap::new();
        for m in matches {
            if let Some(gid) = m.group_id {
                by_group.entry(gid).or_default().push(m);
            }
        }

        let mut tables = Vec::with_capacity(by_group.len());
        for (group_id, g_matches) in by_group {
            let mut clubs = Vec::new();
            let mut seen = HashSet::new();
            let mut names = HashMap::new();
            for m in &g_matches {
                for id in [m.home_team_id, m.away_team_id] {
                    if seen.insert(id) {
                        clubs.push(id);
                        if let Some(tm) = team_cache.get(&id) {
                            names.insert(id, tm.name.clone());
                        }
                    }
                }
            }
            tables.push(GroupTable {
                group_id,
                rows: compute_standings(&clubs, &names, &g_matches),
            });
        }
        Ok(tables)
    }

    pub fn add_international_cups(
        conn: &Connection,
        season: &Season,
        internationals: &[Tournament],
        team_cache: &mut HashMap<TeamId, Team>,
        rng: &mut impl Rng,
    ) -> NaratamaResult<usize> {
        let season_repo = SeasonRepo::new(conn);
        let start_date = default_kickoff(season.start_year)?;
        let mut continental_invites = season_repo.continental_playing_team_ids(season.season_id)?;
        let mut qualifiers_cache = HashMap::new();
        let mut feeder_cache = HashMap::new();
        let mut added = 0usize;
        let mut super_cup_index = 0i64;

        for tour in internationals {
            if season_repo.is_tournament_linked(season.season_id, tour.tournament_id)? {
                continue;
            }
            let is_super_or_global = is_super_or_global_cup(tour.tournament_id);
            let exclude = if is_super_or_global {
                HashSet::new()
            } else {
                continental_invites.clone()
            };
            let invited = select_international_teams(
                conn,
                tour,
                &exclude,
                Some(season.season_id),
                &mut qualifiers_cache,
                &mut feeder_cache,
                rng,
            )?;
            if invited.len() < 2 {
                continue;
            }
            persist_default_config(conn, tour, season.start_year)?;
            let cfg = FixtureCalendarService::resolve_config_for_tournament(
                conn,
                tour.tournament_id,
                Some(season.start_year),
            )?;
            let entrants = if matches!(
                cfg.format,
                CompetitionFormat::LeaguePhase | CompetitionFormat::GroupStage
            ) {
                seed_ids_by_power(&invited)
            } else {
                seed_power_of_two(&invited)
            };
            if !is_super_or_global {
                continental_invites.extend(entrants.iter().copied());
            }

            let mut to_register = Vec::new();
            for tm in &invited {
                if entrants.contains(&tm.team_id) {
                    team_cache.insert(tm.team_id, tm.clone());
                }
            }
            for &id in &entrants {
                if let Some(tm) = team_cache.get(&id) {
                    to_register.push(tm);
                }
            }
            season_repo.register_season_teams(season.season_id, &to_register)?;
            season_repo.link_season_tournament(season.season_id, tour.tournament_id)?;

            let base = season_repo
                .earliest_scheduled_date(season.season_id)?
                .unwrap_or(start_date);
            let is_super = tour.tournament_id == tournament_ids::UEFA_SUPER_CUP
                || tour.tournament_id == tournament_ids::AFC_SUPER_CUP;
            let date = if is_super {
                let s = start_date - Duration::days(4 + super_cup_index * 3);
                super_cup_index += 1;
                s
            } else {
                next_thursday(base + Duration::days(7 * (added as i64 * 6 + 1)))
            };
            schedule_continental_opening(
                conn,
                season.season_id,
                tour.tournament_id,
                &entrants,
                date,
                team_cache,
                season.start_year,
            )?;
            added += 1;
        }

        if added > 0 {
            FixtureCalendarService::resolve_season_conflicts(
                conn,
                season.season_id,
                FixtureCalendarService::DEFAULT_MIN_REST_DAYS,
                true,
                start_date,
            )?;
            season_repo.repair_self_matches(season.season_id)?;
        }
        Ok(added)
    }

    pub fn add_domestic_leagues(
        conn: &Connection,
        season: &Season,
        leagues: &[Tournament],
        team_cache: &mut HashMap<TeamId, Team>,
    ) -> NaratamaResult<usize> {
        let season_repo = SeasonRepo::new(conn);
        let ref_repo = ReferenceRepo::new(conn);
        let start_date = default_kickoff(season.start_year)?;
        let mut match_rows = Vec::new();
        let mut added = 0usize;

        for tour in leagues {
            if season_repo.is_tournament_linked(season.season_id, tour.tournament_id)? {
                continue;
            }
            let mut div_teams = ref_repo.teams_for_division(tour.tournament_id)?;
            let max_teams = if tour.slots > 0 {
                tour.slots as usize
            } else {
                24
            };
            if div_teams.len() > max_teams {
                div_teams.sort_by(|a, b| {
                    b.power
                        .cmp(&a.power)
                        .then_with(|| a.team_id.cmp(&b.team_id))
                });
                div_teams.truncate(max_teams);
            }
            if div_teams.len() < 2 {
                continue;
            }
            for tm in &div_teams {
                team_cache.insert(tm.team_id, tm.clone());
            }
            let refs: Vec<&Team> = div_teams.iter().collect();
            season_repo.register_season_teams(season.season_id, &refs)?;
            season_repo.link_season_tournament(season.season_id, tour.tournament_id)?;
            persist_default_config(conn, tour, season.start_year)?;
            let cfg = FixtureCalendarService::resolve_config_for_tournament(
                conn,
                tour.tournament_id,
                Some(season.start_year),
            )?;
            let ids: Vec<TeamId> = div_teams.iter().map(|t| t.team_id).collect();
            let slots = RoundRobinScheduler::schedule(&ids, cfg.double_round_robin, true, None);
            for slot in slots {
                if let (Some(home), Some(away)) =
                    (team_cache.get(&slot.home), team_cache.get(&slot.away))
                {
                    match_rows.push(build_fixture_row(
                        season.season_id,
                        tour.tournament_id,
                        home,
                        away,
                        slot.round,
                        1,
                        slot.leg,
                        round_date(start_date, slot.round, &cfg)?,
                        None,
                        false,
                        false,
                        false,
                    ));
                }
            }
            added += 1;
        }

        if !match_rows.is_empty() {
            season_repo.insert_matches(&match_rows)?;
            FixtureCalendarService::resolve_season_conflicts(
                conn,
                season.season_id,
                FixtureCalendarService::DEFAULT_MIN_REST_DAYS,
                true,
                start_date,
            )?;
            season_repo.repair_self_matches(season.season_id)?;
        }
        Ok(added)
    }
}

#[allow(clippy::too_many_arguments)]
fn schedule_cup_round_inner(
    conn: &Connection,
    season_id: SeasonId,
    cup_tournament_id: TournamentId,
    survivors: &[TeamId],
    phase: i32,
    date: NaiveDate,
    all_teams: &HashMap<TeamId, Team>,
    pairs: Option<&[(TeamId, TeamId)]>,
    byes: &[TeamId],
    season_start_year: Option<i32>,
) -> NaratamaResult<usize> {
    let season_repo = SeasonRepo::new(conn);
    let cfg = FixtureCalendarService::resolve_config_for_tournament(
        conn,
        cup_tournament_id,
        season_start_year,
    )?;
    let seeded_teams: Vec<Team> = survivors
        .iter()
        .filter_map(|id| all_teams.get(id).cloned())
        .collect();
    let seeded = seed_ids_by_power(&seeded_teams);

    let mut advancing_byes = byes.to_vec();
    let match_pairs = if let Some(explicit) = pairs {
        explicit.to_vec()
    } else {
        let plan = KnockoutScheduler::pair_next_round(&seeded);
        advancing_byes.extend(plan.byes);
        plan.pairs
    };

    let paired: HashSet<TeamId> = match_pairs.iter().flat_map(|&(a, b)| [a, b]).collect();
    let mut seen_byes = HashSet::new();
    advancing_byes.retain(|id| !paired.contains(id) && seen_byes.insert(*id));

    let is_final = match_pairs.len() == 1 && advancing_byes.is_empty() && seeded.len() == 2;
    let legs = if is_final && cfg.final_single_match {
        1
    } else {
        cfg.knockout_legs
    };

    let mut rows = Vec::new();
    for &(a, b) in &match_pairs {
        if a == b {
            continue;
        }
        let (Some(home), Some(away)) = (all_teams.get(&a), all_teams.get(&b)) else {
            continue;
        };
        let neutral = is_final && cfg.neutral_final;
        rows.push(build_fixture_row(
            season_id,
            cup_tournament_id,
            home,
            away,
            phase,
            phase,
            1,
            date,
            None,
            neutral,
            cfg.extra_time,
            cfg.away_goals,
        ));
        if legs == 2 {
            rows.push(build_fixture_row(
                season_id,
                cup_tournament_id,
                away,
                home,
                phase,
                phase,
                2,
                date + Duration::days(i64::from(cfg.matchday_interval_days)),
                None,
                false,
                cfg.extra_time,
                cfg.away_goals,
            ));
        }
    }

    season_repo.record_knockout_byes(season_id, cup_tournament_id, phase, &advancing_byes)?;
    season_repo.insert_matches(&rows)
}

fn schedule_continental_opening(
    conn: &Connection,
    season_id: SeasonId,
    tournament_id: TournamentId,
    entrants: &[TeamId],
    start: NaiveDate,
    all_teams: &HashMap<TeamId, Team>,
    season_start_year: i32,
) -> NaratamaResult<()> {
    let cfg = FixtureCalendarService::resolve_config_for_tournament(
        conn,
        tournament_id,
        Some(season_start_year),
    )?;
    if cfg.format == CompetitionFormat::LeaguePhase {
        SeasonGenerator::schedule_league_phase(
            conn,
            season_id,
            tournament_id,
            entrants,
            start,
            all_teams,
            &cfg,
        )?;
        return Ok(());
    }
    if cfg.format == CompetitionFormat::GroupStage
        && entrants.len() >= (cfg.group_size.max(2) as usize) * 2
    {
        SeasonGenerator::schedule_group_stage(
            conn,
            season_id,
            tournament_id,
            entrants,
            start,
            all_teams,
            &cfg,
        )?;
        return Ok(());
    }
    let seeded_teams: Vec<Team> = entrants
        .iter()
        .filter_map(|id| all_teams.get(id).cloned())
        .collect();
    let seeded = seed_ids_by_power(&seeded_teams);
    let plan = KnockoutScheduler::plan_opening_round(
        &seeded,
        cfg.use_byes,
        cfg.trim_to_power_of_two,
        cfg.field_size.map(|v| v.max(0) as usize),
    );
    schedule_cup_round_inner(
        conn,
        season_id,
        tournament_id,
        &seeded,
        1,
        start,
        all_teams,
        Some(&plan.pairs),
        &plan.byes,
        Some(season_start_year),
    )?;
    Ok(())
}

fn select_international_teams(
    conn: &Connection,
    tour: &Tournament,
    exclude: &HashSet<TeamId>,
    before_season_id: Option<SeasonId>,
    qualifiers_cache: &mut HashMap<(CountryId, Option<i64>), Vec<Team>>,
    feeder_cache: &mut HashMap<(i64, TournamentId), Option<(TeamId, TeamId)>>,
    rng: &mut impl Rng,
) -> NaratamaResult<Vec<Team>> {
    if tour.tournament_id == tournament_ids::INTERCONTINENTAL_CUP {
        return select_intercontinental_teams(
            conn,
            before_season_id,
            qualifiers_cache,
            feeder_cache,
            rng,
        );
    }
    if let Some((cups, size)) = champions_feed(tour.tournament_id) {
        return select_champions_of(
            conn,
            cups,
            size,
            before_season_id,
            qualifiers_cache,
            feeder_cache,
            rng,
        );
    }

    let slots = SeasonRepo::new(conn).international_slots(tour.tournament_id)?;
    let mut teams = Vec::new();
    let mut added = HashSet::new();
    for (country_id, count) in slots {
        if count <= 0 {
            continue;
        }
        let clubs = continental_qualifiers(conn, country_id, before_season_id, qualifiers_cache)?;
        for tm in clubs
            .into_iter()
            .filter(|tm| !exclude.contains(&tm.team_id) && added.insert(tm.team_id))
            .take(count as usize)
        {
            teams.push(tm);
        }
    }
    Ok(teams)
}

fn select_intercontinental_teams(
    conn: &Connection,
    before_season_id: Option<SeasonId>,
    qualifiers_cache: &mut HashMap<(CountryId, Option<i64>), Vec<Team>>,
    feeder_cache: &mut HashMap<(i64, TournamentId), Option<(TeamId, TeamId)>>,
    rng: &mut impl Rng,
) -> NaratamaResult<Vec<Team>> {
    const REGIONS: [TournamentId; 5] = [
        tournament_ids::CHAMPIONS_LEAGUE,
        tournament_ids::LIBERTADORES,
        tournament_ids::ACL_ELITE,
        tournament_ids::CAF_CHAMPIONS_LEAGUE,
        tournament_ids::CONCACAF_CHAMPIONS_CUP,
    ];
    let ref_repo = ReferenceRepo::new(conn);
    let mut teams = Vec::new();
    let mut selected = HashSet::new();
    if let Some(sid) = before_season_id {
        for cup_id in REGIONS {
            if let Some((champ, runner)) =
                feeder_finals(conn, cup_id, sid, qualifiers_cache, feeder_cache, rng)?
            {
                for finalist in [champ, runner] {
                    if !selected.insert(finalist) {
                        continue;
                    }
                    if let Some(tm) = ref_repo.team_by_id(finalist)? {
                        teams.push(tm);
                        break;
                    }
                }
            }
        }
    }
    Ok(teams)
}

fn select_champions_of(
    conn: &Connection,
    cups: &[TournamentId],
    size: usize,
    before_season_id: Option<SeasonId>,
    qualifiers_cache: &mut HashMap<(CountryId, Option<i64>), Vec<Team>>,
    feeder_cache: &mut HashMap<(i64, TournamentId), Option<(TeamId, TeamId)>>,
    rng: &mut impl Rng,
) -> NaratamaResult<Vec<Team>> {
    let mut champions = Vec::new();
    let mut runners_up = Vec::new();
    if let Some(sid) = before_season_id {
        for &cup_id in cups {
            if let Some((champ, runner)) =
                feeder_finals(conn, cup_id, sid, qualifiers_cache, feeder_cache, rng)?
            {
                champions.push(champ);
                runners_up.push(runner);
            }
        }
    }
    let mut ids = Vec::new();
    let mut selected = HashSet::new();
    for id in champions.into_iter().chain(runners_up) {
        if ids.len() >= size {
            break;
        }
        if selected.insert(id) {
            ids.push(id);
        }
    }
    let ref_repo = ReferenceRepo::new(conn);
    let mut teams = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(tm) = ref_repo.team_by_id(id)? {
            teams.push(tm);
        }
    }
    Ok(teams)
}

fn feeder_finals(
    conn: &Connection,
    tournament_id: TournamentId,
    before_season_id: SeasonId,
    qualifiers_cache: &mut HashMap<(CountryId, Option<i64>), Vec<Team>>,
    feeder_cache: &mut HashMap<(i64, TournamentId), Option<(TeamId, TeamId)>>,
    rng: &mut impl Rng,
) -> NaratamaResult<Option<(TeamId, TeamId)>> {
    if let Some(played) = SeasonRepo::new(conn).last_final_teams(tournament_id, before_season_id)? {
        return Ok(Some(played));
    }
    let key = (before_season_id.get(), tournament_id);
    if let Some(&cached) = feeder_cache.get(&key) {
        return Ok(cached);
    }
    let sim = simulate_virtual_feeder_final(
        conn,
        tournament_id,
        before_season_id,
        qualifiers_cache,
        rng,
    )?;
    feeder_cache.insert(key, sim);
    Ok(sim)
}

fn simulate_virtual_feeder_final(
    conn: &Connection,
    tournament_id: TournamentId,
    before_season_id: SeasonId,
    qualifiers_cache: &mut HashMap<(CountryId, Option<i64>), Vec<Team>>,
    rng: &mut impl Rng,
) -> NaratamaResult<Option<(TeamId, TeamId)>> {
    let Some(tour) = ReferenceRepo::new(conn).tournament_by_id(tournament_id)? else {
        return Ok(None);
    };
    let season_repo = SeasonRepo::new(conn);
    let start_year = season_repo.season_years(before_season_id)?.map(|(s, _)| s);
    let cfg =
        FixtureCalendarService::resolve_config_for_tournament(conn, tournament_id, start_year)?;
    let field_limit = cfg
        .field_size
        .or(cfg.participant_count)
        .unwrap_or(tour.slots)
        .max(2) as usize;

    let slots = season_repo.international_slots(tournament_id)?;
    let mut eligible: HashMap<TeamId, Team> = HashMap::new();
    for (country_id, count) in slots {
        if count <= 0 {
            continue;
        }
        let qualifiers =
            continental_qualifiers(conn, country_id, Some(before_season_id), qualifiers_cache)?;
        for tm in qualifiers.into_iter().take(count as usize) {
            eligible.entry(tm.team_id).or_insert(tm);
        }
    }
    if eligible.len() < 2 {
        return Ok(None);
    }

    let mut entrants: Vec<Team> = eligible.into_values().collect();
    entrants.sort_by(|a, b| {
        b.power
            .cmp(&a.power)
            .then_with(|| a.team_id.cmp(&b.team_id))
    });
    entrants.truncate(field_limit);
    entrants.shuffle(rng);

    let by_id: HashMap<TeamId, Team> = entrants.iter().map(|tm| (tm.team_id, tm.clone())).collect();
    let ids: Vec<TeamId> = entrants.iter().map(|tm| tm.team_id).collect();
    let opening = KnockoutScheduler::plan_opening_round(&ids, true, false, None);
    let mut survivors = opening.byes;
    for (home, away) in opening.pairs {
        survivors.push(virtual_cup_winner(home, away, &by_id, rng));
    }
    while survivors.len() > 2 {
        let next = KnockoutScheduler::pair_next_round(&survivors);
        let mut next_survivors = next.byes;
        for (home, away) in next.pairs {
            next_survivors.push(virtual_cup_winner(home, away, &by_id, rng));
        }
        survivors = next_survivors;
    }
    if survivors.len() != 2 {
        return Ok(None);
    }
    let champion = virtual_cup_winner(survivors[0], survivors[1], &by_id, rng);
    let runner_up = if champion == survivors[0] {
        survivors[1]
    } else {
        survivors[0]
    };
    Ok(Some((champion, runner_up)))
}

fn virtual_cup_winner(
    home: TeamId,
    away: TeamId,
    teams: &HashMap<TeamId, Team>,
    rng: &mut impl Rng,
) -> TeamId {
    let hp = teams.get(&home).map_or(50, |t| t.power).max(1);
    let ap = teams.get(&away).map_or(50, |t| t.power).max(1);
    let (hg, ag) = MatchEngine::quick_simulate(rng, hp, ap, true);
    match hg.cmp(&ag) {
        std::cmp::Ordering::Greater => home,
        std::cmp::Ordering::Less => away,
        std::cmp::Ordering::Equal => {
            let p_home = f64::from(hp) / f64::from(hp + ap);
            if rng.gen::<f64>() < p_home {
                home
            } else {
                away
            }
        }
    }
}

fn continental_qualifiers(
    conn: &Connection,
    country_id: CountryId,
    before_season_id: Option<SeasonId>,
    cache: &mut HashMap<(CountryId, Option<i64>), Vec<Team>>,
) -> NaratamaResult<Vec<Team>> {
    let key = (country_id, before_season_id.map(SeasonId::get));
    if let Some(cached) = cache.get(&key) {
        return Ok(cached.clone());
    }
    let season_repo = SeasonRepo::new(conn);
    let clubs = season_repo.division1_teams_by_power(country_id)?;
    let Some(before_sid) = before_season_id else {
        cache.insert(key, clubs.clone());
        return Ok(clubs);
    };
    let table_order = season_repo.latest_div1_league_standings_order(country_id, before_sid)?;
    if table_order.is_empty() {
        cache.insert(key, clubs.clone());
        return Ok(clubs);
    }
    let mut by_id: HashMap<TeamId, Team> = clubs.into_iter().map(|t| (t.team_id, t)).collect();
    let mut ordered = Vec::with_capacity(by_id.len());
    for id in table_order {
        if let Some(tm) = by_id.remove(&id) {
            ordered.push(tm);
        }
    }
    let mut remaining: Vec<Team> = by_id.into_values().collect();
    remaining.sort_by(|a, b| {
        b.power
            .cmp(&a.power)
            .then_with(|| a.team_id.cmp(&b.team_id))
    });
    ordered.extend(remaining);
    cache.insert(key, ordered.clone());
    Ok(ordered)
}

fn persist_default_config(
    conn: &Connection,
    tour: &Tournament,
    start_year: i32,
) -> NaratamaResult<()> {
    let repo = SeasonRepo::new(conn);
    if repo
        .load_competition_config(tour.tournament_id, Some(start_year))?
        .is_some()
    {
        return Ok(());
    }
    let cfg = config_for_tournament(tour, Some(start_year));
    repo.persist_competition_config(&cfg)
}

fn next_cup_date(
    conn: &Connection,
    season_id: SeasonId,
    tournament_id: TournamentId,
    team_ids: &[TeamId],
) -> NaratamaResult<NaiveDate> {
    let season_repo = SeasonRepo::new(conn);
    let start_year = season_repo
        .season_years(season_id)?
        .map_or(SeasonGenerator::DEFAULT_START_YEAR, |(s, _)| s);
    let season_start = NaiveDate::from_ymd_opt(start_year, 7, 1)
        .ok_or_else(|| NaratamaError::InvalidState("invalid season start".to_string()))?;
    let season_end = NaiveDate::from_ymd_opt(start_year + 1, 6, 30)
        .ok_or_else(|| NaratamaError::InvalidState("invalid season end".to_string()))?;
    let base = season_repo
        .last_tournament_date(season_id, tournament_id)?
        .unwrap_or_else(|| NaiveDate::from_ymd_opt(start_year, 8, 9).unwrap_or(season_start));
    let mandatory = InternationalCalendarService::mandatory_unavailable_dates(conn, season_id)?;
    let matches = season_repo.matches_for_season(season_id)?;
    let mut club_dates: HashMap<TeamId, Vec<NaiveDate>> = HashMap::new();
    for m in matches {
        if m.status == MatchStatus::Cancelled {
            continue;
        }
        if let Some(d) = m.date {
            club_dates.entry(m.home_team_id).or_default().push(d);
            club_dates.entry(m.away_team_id).or_default().push(d);
        }
    }

    let target_set: HashSet<TeamId> = team_ids.iter().copied().collect();
    let mut candidate = base + Duration::days(7);
    for _ in 0..180 {
        let wd = candidate.weekday().number_from_monday();
        let midweek_ok = (2..=4).contains(&wd);
        let not_blackout = !mandatory.contains(&candidate);
        let no_clash = !target_set.iter().any(|tid| {
            club_dates
                .get(tid)
                .is_some_and(|ds| ds.iter().any(|&d| (d - candidate).num_days().abs() < 2))
        });
        if midweek_ok && not_blackout && no_clash && candidate <= season_end {
            return Ok(candidate);
        }
        candidate += Duration::days(1);
    }
    let dummy = TeamId(0);
    Ok(find_available_date(
        base + Duration::days(7),
        dummy,
        dummy,
        &club_dates,
        season_start,
        season_end,
        2,
        3,
        &mandatory,
        naratama_model::StadiumId(0),
        &HashMap::new(),
        true,
        Some(&[2, 3, 4]),
    )
    .unwrap_or(base + Duration::days(7)))
}

#[allow(clippy::too_many_arguments)]
fn build_fixture_row(
    season_id: SeasonId,
    tournament_id: TournamentId,
    home: &Team,
    away: &Team,
    round: i32,
    phase: i32,
    leg: i32,
    date: NaiveDate,
    group_id: Option<i32>,
    neutral_venue: bool,
    extra_time: bool,
    away_goals_rule: bool,
) -> NewFixtureRow {
    NewFixtureRow {
        tournament_id,
        season_id,
        home_team_id: home.team_id,
        away_team_id: away.team_id,
        stadium_id: home.stadium_id,
        round,
        phase,
        group_id,
        leg,
        date,
        neutral_venue,
        extra_time,
        away_goals_rule,
    }
}

fn default_kickoff(start_year: i32) -> NaratamaResult<NaiveDate> {
    NaiveDate::from_ymd_opt(start_year, 8, 9)
        .ok_or_else(|| NaratamaError::InvalidState(format!("invalid kickoff year {start_year}")))
}

fn round_date(start: NaiveDate, round: i32, cfg: &CompetitionConfig) -> NaratamaResult<NaiveDate> {
    let preferred =
        start + Duration::days(i64::from(cfg.matchday_interval_days) * i64::from(round - 1));
    if cfg.allowed_weekdays.is_empty() {
        return Ok(preferred);
    }
    for offset in 0..7_i64 {
        let candidate = preferred + Duration::days(offset);
        let wd = candidate.weekday().number_from_monday();
        if cfg.allowed_weekdays.contains(&wd) {
            return Ok(candidate);
        }
    }
    Err(NaratamaError::InvalidState(format!(
        "Competition {} has no valid allowed weekdays",
        cfg.tournament_id
    )))
}

fn cup_date(start_date: NaiveDate, phase: i32, league_rounds: i32) -> NaiveDate {
    let spacing = (league_rounds / 8).max(2);
    let week_offset = 2 + (phase - 1) * spacing;
    start_date + Duration::days(7 * i64::from(week_offset) - 3)
}

fn international_date(start_date: NaiveDate, index: usize) -> NaiveDate {
    let day_offset = if index.is_multiple_of(2) {
        -4_i64
    } else {
        -2_i64
    };
    let week_offset = 3_i64 + ((index as i64 / 2) % 4) * 2;
    let mut date = start_date + Duration::days(7 * week_offset + day_offset);
    if let (Some(oct_start), Some(oct_end)) = (
        NaiveDate::from_ymd_opt(start_date.year(), 10, 5),
        NaiveDate::from_ymd_opt(start_date.year(), 10, 15),
    ) {
        while date >= oct_start && date <= oct_end {
            date += Duration::days(7);
        }
    }
    date
}

fn next_thursday(from: NaiveDate) -> NaiveDate {
    let mut d = from;
    while d.weekday().number_from_monday() != 4 {
        d += Duration::days(1);
    }
    d
}

fn seed_ids_by_power(teams: &[Team]) -> Vec<TeamId> {
    let mut unique: HashMap<TeamId, &Team> = HashMap::with_capacity(teams.len());
    for tm in teams {
        unique.entry(tm.team_id).or_insert(tm);
    }
    let mut sorted: Vec<&Team> = unique.into_values().collect();
    sorted.sort_by(|a, b| {
        b.power
            .cmp(&a.power)
            .then_with(|| a.team_id.cmp(&b.team_id))
    });
    sorted.into_iter().map(|t| t.team_id).collect()
}

fn seed_power_of_two(teams: &[Team]) -> Vec<TeamId> {
    let sorted = seed_ids_by_power(teams);
    let mut bracket = 1usize;
    while bracket * 2 <= sorted.len() {
        bracket *= 2;
    }
    sorted.into_iter().take(bracket).collect()
}

const fn is_super_or_global_cup(tid: TournamentId) -> bool {
    let id = tid.get();
    id == tournament_ids::UEFA_SUPER_CUP.get()
        || id == tournament_ids::AFC_SUPER_CUP.get()
        || id == tournament_ids::INTERCONTINENTAL_CUP.get()
        || id == tournament_ids::CLUB_WORLD_CUP.get()
}

fn champions_feed(tid: TournamentId) -> Option<(&'static [TournamentId], usize)> {
    const UEFA_SC: [TournamentId; 2] = [
        tournament_ids::CHAMPIONS_LEAGUE,
        tournament_ids::EUROPA_LEAGUE,
    ];
    const AFC_SC: [TournamentId; 2] = [tournament_ids::ACL_ELITE, tournament_ids::ACL_TWO];
    const CWC: [TournamentId; 10] = [
        tournament_ids::CHAMPIONS_LEAGUE,
        tournament_ids::LIBERTADORES,
        tournament_ids::ACL_ELITE,
        tournament_ids::CONCACAF_CHAMPIONS_CUP,
        tournament_ids::CAF_CHAMPIONS_LEAGUE,
        tournament_ids::EUROPA_LEAGUE,
        tournament_ids::SUDAMERICANA,
        tournament_ids::ACL_TWO,
        tournament_ids::CONFERENCE_LEAGUE,
        tournament_ids::AFC_CHALLENGE_LEAGUE,
    ];
    match tid.get() {
        x if x == tournament_ids::UEFA_SUPER_CUP.get() => Some((&UEFA_SC, 2)),
        x if x == tournament_ids::AFC_SUPER_CUP.get() => Some((&AFC_SC, 2)),
        x if x == tournament_ids::CLUB_WORLD_CUP.get() => Some((&CWC, 16)),
        _ => None,
    }
}
