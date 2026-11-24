use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;

use chrono::NaiveDate;
use naratama_model::{
    FixtureMatch, MatchId, MatchStatus, SeasonId, StadiumId, TeamId, TournamentId,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FixtureViolationSeverity {
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FixtureViolation {
    pub code: String,
    pub message: String,
    pub competition_id: TournamentId,
    pub season_id: SeasonId,
    pub match_id: Option<MatchId>,
    pub club_id: Option<TeamId>,
    pub round: Option<i32>,
    pub phase: Option<i32>,
    pub severity: FixtureViolationSeverity,
}

impl FixtureViolation {
    #[must_use]
    pub const fn is_error(&self) -> bool {
        matches!(self.severity, FixtureViolationSeverity::Error)
    }
}

impl fmt::Display for FixtureViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sev = if self.is_error() { "ERROR" } else { "WARNING" };
        write!(
            f,
            "[{sev}] competition={} season={} ",
            self.competition_id, self.season_id
        )?;
        if let Some(m) = self.match_id {
            write!(f, "fixture={m} ")?;
        }
        if let Some(c) = self.club_id {
            write!(f, "club={c} ")?;
        }
        if let Some(r) = self.round {
            write!(f, "round={r} ")?;
        }
        if let Some(p) = self.phase {
            write!(f, "phase={p} ")?;
        }
        write!(f, "{}: {}", self.code, self.message)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FixtureValidationReport {
    pub violations: Vec<FixtureViolation>,
}

impl FixtureValidationReport {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.violations.iter().all(|v| !v.is_error())
    }

    #[must_use]
    pub fn error_count(&self) -> usize {
        self.violations.iter().filter(|v| v.is_error()).count()
    }

    #[must_use]
    pub fn warning_count(&self) -> usize {
        self.violations.len() - self.error_count()
    }

    #[must_use]
    pub fn describe(&self) -> String {
        if self.violations.is_empty() {
            return "Fixture list is valid.".to_string();
        }
        let mut lines = vec![format!(
            "{} error(s), {} warning(s):",
            self.error_count(),
            self.warning_count()
        )];
        for v in &self.violations {
            lines.push(format!(" - {v}"));
        }
        lines.join("\n")
    }
}

pub struct FixtureValidator;

impl FixtureValidator {
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn validate_league(
        competition_id: TournamentId,
        season_id: SeasonId,
        fixtures: &[FixtureMatch],
        club_ids: &[TeamId],
        double_round: bool,
        code_prefix: &str,
    ) -> FixtureValidationReport {
        let mut out = Vec::new();
        let mut clubs = unique_sorted_clubs(club_ids);
        clubs.sort_unstable();
        let valid: HashSet<TeamId> = clubs.iter().copied().collect();
        let n = clubs.len();
        let expected_meetings = if double_round { 2 } else { 1 };
        let expected_fixtures = if double_round {
            n * n.saturating_sub(1)
        } else {
            n * n.saturating_sub(1) / 2
        };
        let single_cycle_rounds = if n.is_multiple_of(2) {
            n.saturating_sub(1)
        } else {
            n
        };
        let expected_rounds = if double_round {
            single_cycle_rounds * 2
        } else {
            single_cycle_rounds
        };

        if fixtures.len() != expected_fixtures {
            out.push(simple_err(
                format!("{code_prefix}_fixture_count"),
                format!(
                    "Expected {expected_fixtures} fixtures for {n} clubs, found {}.",
                    fixtures.len()
                ),
                competition_id,
                season_id,
            ));
        }

        let mut pair_counts: HashMap<(TeamId, TeamId), usize> = HashMap::new();
        let mut home_counts: HashMap<(TeamId, TeamId), usize> = HashMap::new();
        let mut club_homes: HashMap<TeamId, usize> = clubs.iter().map(|&id| (id, 0)).collect();
        let mut club_aways: HashMap<TeamId, usize> = clubs.iter().map(|&id| (id, 0)).collect();
        let mut by_round: BTreeMap<i32, Vec<&FixtureMatch>> = BTreeMap::new();
        let mut identities = HashSet::new();

        for m in fixtures {
            if m.home_team_id == m.away_team_id {
                out.push(match_err(
                    "league_self_fixture",
                    "A club was scheduled against itself.",
                    competition_id,
                    season_id,
                    m,
                ));
            }
            if !valid.contains(&m.home_team_id) || !valid.contains(&m.away_team_id) {
                out.push(match_err(
                    "league_invalid_club",
                    "Fixture references a club that is not registered in the league.",
                    competition_id,
                    season_id,
                    m,
                ));
            }
            let identity = (m.phase, m.leg, m.round, m.home_team_id, m.away_team_id);
            if !identities.insert(identity) {
                out.push(match_err(
                    "league_duplicate_fixture",
                    "The same fixture appears more than once.",
                    competition_id,
                    season_id,
                    m,
                ));
            }
            *pair_counts
                .entry(pair_key(m.home_team_id, m.away_team_id))
                .or_insert(0) += 1;
            *home_counts
                .entry((m.home_team_id, m.away_team_id))
                .or_insert(0) += 1;
            *club_homes.entry(m.home_team_id).or_insert(0) += 1;
            *club_aways.entry(m.away_team_id).or_insert(0) += 1;
            by_round.entry(m.round).or_default().push(m);
        }

        for (&(a, b), &count) in &pair_counts {
            if count != expected_meetings {
                out.push(simple_err(
                    format!("{code_prefix}_pair_coverage"),
                    format!(
                        "Pairing {a}-{b} occurs {count} time(s); expected {expected_meetings}."
                    ),
                    competition_id,
                    season_id,
                ));
            }
            if double_round {
                let ab = home_counts.get(&(a, b)).copied().unwrap_or(0);
                let ba = home_counts.get(&(b, a)).copied().unwrap_or(0);
                if ab != 1 || ba != 1 {
                    let mut v = simple_err(
                        format!("{code_prefix}_home_away_reversal"),
                        format!(
                            "Pairing {a}/{b} must have one home fixture each; found {a} home={ab}, {b} home={ba}."
                        ),
                        competition_id,
                        season_id,
                    );
                    v.club_id = Some(a);
                    out.push(v);
                }
            }
        }

        if n >= 2 {
            for i in 0..n {
                for j in (i + 1)..n {
                    let key = pair_key(clubs[i], clubs[j]);
                    if !pair_counts.contains_key(&key) {
                        let mut v = simple_err(
                            format!("{code_prefix}_missing_pair"),
                            format!("Clubs {} and {} never meet.", clubs[i], clubs[j]),
                            competition_id,
                            season_id,
                        );
                        v.club_id = Some(clubs[i]);
                        out.push(v);
                    }
                }
            }
        }

        let expected_home = if double_round {
            n.saturating_sub(1)
        } else {
            n.saturating_sub(1) / 2 + usize::from(n.saturating_sub(1) % 2 == 1)
        };
        let expected_away = if double_round {
            n.saturating_sub(1)
        } else {
            n.saturating_sub(1) / 2
        };

        for &club in &clubs {
            let homes = club_homes.get(&club).copied().unwrap_or(0);
            let aways = club_aways.get(&club).copied().unwrap_or(0);
            if homes + aways != expected_meetings * n.saturating_sub(1) {
                let mut v = simple_err(
                    format!("{code_prefix}_appearance_count"),
                    format!(
                        "Club {club} plays {} matches; expected {}.",
                        homes + aways,
                        expected_meetings * n.saturating_sub(1)
                    ),
                    competition_id,
                    season_id,
                );
                v.club_id = Some(club);
                out.push(v);
            }
            if double_round && (homes != expected_home || aways != expected_away) {
                let mut v = simple_err(
                    format!("{code_prefix}_home_away_totals"),
                    format!(
                        "Club {club} has {homes} home / {aways} away; expected {expected_home} / {expected_away}."
                    ),
                    competition_id,
                    season_id,
                );
                v.club_id = Some(club);
                out.push(v);
            }
            if !double_round && homes.abs_diff(aways) > 1 {
                let mut v = simple_err(
                    format!("{code_prefix}_home_away_balance"),
                    format!(
                        "Club {club} has {homes} home / {aways} away; single round-robin may differ by at most one."
                    ),
                    competition_id,
                    season_id,
                );
                v.club_id = Some(club);
                out.push(v);
            }
        }

        if by_round.len() != expected_rounds {
            out.push(simple_err(
                format!("{code_prefix}_round_count"),
                format!(
                    "Expected {expected_rounds} rounds, found {}.",
                    by_round.len()
                ),
                competition_id,
                season_id,
            ));
        }
        for (round, round_fixtures) in &by_round {
            let mut playing = HashSet::new();
            for m in round_fixtures {
                if !playing.insert(m.home_team_id) {
                    out.push(match_err(
                        &format!("{code_prefix}_double_booked"),
                        &format!("Club {} appears twice in round {round}.", m.home_team_id),
                        competition_id,
                        season_id,
                        m,
                    ));
                }
                if !playing.insert(m.away_team_id) {
                    out.push(match_err(
                        &format!("{code_prefix}_double_booked"),
                        &format!("Club {} appears twice in round {round}.", m.away_team_id),
                        competition_id,
                        season_id,
                        m,
                    ));
                }
            }
        }

        FixtureValidationReport { violations: out }
    }

    #[must_use]
    pub fn validate_league_phase(
        competition_id: TournamentId,
        season_id: SeasonId,
        fixtures: &[FixtureMatch],
        club_ids: &[TeamId],
        opponents_per_club: usize,
        home_matches: usize,
        away_matches: usize,
    ) -> FixtureValidationReport {
        let mut out = Vec::new();
        let clubs = unique_sorted_clubs(club_ids);
        let valid: HashSet<TeamId> = clubs.iter().copied().collect();
        let mut opponents: HashMap<TeamId, HashSet<TeamId>> =
            clubs.iter().map(|&id| (id, HashSet::new())).collect();
        let mut homes: HashMap<TeamId, usize> = clubs.iter().map(|&id| (id, 0)).collect();
        let mut aways: HashMap<TeamId, usize> = clubs.iter().map(|&id| (id, 0)).collect();
        let mut by_round: BTreeMap<i32, Vec<&FixtureMatch>> = BTreeMap::new();
        let mut pairs = HashSet::new();

        let expected_fixtures = clubs.len() * opponents_per_club / 2;
        if fixtures.len() != expected_fixtures {
            out.push(simple_err(
                "league_phase_fixture_count".to_string(),
                format!(
                    "Expected {expected_fixtures} fixtures, found {}.",
                    fixtures.len()
                ),
                competition_id,
                season_id,
            ));
        }

        for m in fixtures {
            if !valid.contains(&m.home_team_id) || !valid.contains(&m.away_team_id) {
                out.push(match_err(
                    "league_phase_invalid_club",
                    "Fixture references a club outside the league phase.",
                    competition_id,
                    season_id,
                    m,
                ));
                continue;
            }
            if m.home_team_id == m.away_team_id {
                out.push(match_err(
                    "league_phase_self_fixture",
                    "A club was scheduled against itself.",
                    competition_id,
                    season_id,
                    m,
                ));
            }
            if !pairs.insert(pair_key(m.home_team_id, m.away_team_id)) {
                out.push(match_err(
                    "league_phase_repeated_opponent",
                    "The same pair meets more than once in the league phase.",
                    competition_id,
                    season_id,
                    m,
                ));
            }
            if let Some(set) = opponents.get_mut(&m.home_team_id) {
                set.insert(m.away_team_id);
            }
            if let Some(set) = opponents.get_mut(&m.away_team_id) {
                set.insert(m.home_team_id);
            }
            *homes.entry(m.home_team_id).or_insert(0) += 1;
            *aways.entry(m.away_team_id).or_insert(0) += 1;
            by_round.entry(m.round).or_default().push(m);
        }

        for &club in &clubs {
            let opp_count = opponents.get(&club).map_or(0, HashSet::len);
            if opp_count != opponents_per_club {
                let mut v = simple_err(
                    "league_phase_opponent_count".to_string(),
                    format!("Club {club} has {opp_count} distinct opponents; expected {opponents_per_club}."),
                    competition_id,
                    season_id,
                );
                v.club_id = Some(club);
                out.push(v);
            }
            let h = homes.get(&club).copied().unwrap_or(0);
            let a = aways.get(&club).copied().unwrap_or(0);
            if h != home_matches || a != away_matches {
                let mut v = simple_err(
                    "league_phase_home_away".to_string(),
                    format!("Club {club} has {h} home / {a} away; expected {home_matches} / {away_matches}."),
                    competition_id,
                    season_id,
                );
                v.club_id = Some(club);
                out.push(v);
            }
        }

        for (round, round_fixtures) in &by_round {
            let mut playing = HashSet::new();
            for m in round_fixtures {
                if !playing.insert(m.home_team_id) || !playing.insert(m.away_team_id) {
                    out.push(match_err(
                        "league_phase_double_booked",
                        &format!("A club appears twice in matchday {round}."),
                        competition_id,
                        season_id,
                        m,
                    ));
                }
            }
        }

        FixtureValidationReport { violations: out }
    }

    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn validate_knockout(
        competition_id: TournamentId,
        season_id: SeasonId,
        fixtures: &[FixtureMatch],
        expected_legs: Option<usize>,
        final_single_match: bool,
        allow_byes: bool,
    ) -> FixtureValidationReport {
        let mut out = Vec::new();
        let mut by_phase: BTreeMap<i32, Vec<&FixtureMatch>> = BTreeMap::new();
        for m in fixtures {
            by_phase.entry(m.phase).or_default().push(m);
        }
        let last_phase = by_phase.keys().next_back().copied().unwrap_or(0);

        for (&phase, phase_fixtures) in &by_phase {
            let mut ties: BTreeMap<(TeamId, TeamId), Vec<&FixtureMatch>> = BTreeMap::new();
            for &m in phase_fixtures {
                ties.entry(pair_key(m.home_team_id, m.away_team_id))
                    .or_default()
                    .push(m);
            }

            let mut playing = HashSet::new();
            for tie in ties.values() {
                let reference = tie[0];
                for club in [reference.home_team_id, reference.away_team_id] {
                    if !playing.insert(club) {
                        out.push(match_err(
                            "knockout_duplicate_club",
                            &format!("Club {club} appears in two ties in phase {phase}."),
                            competition_id,
                            season_id,
                            reference,
                        ));
                    }
                }
            }

            let is_final = final_single_match && phase == last_phase && ties.len() == 1;
            let expected = if is_final {
                1
            } else {
                expected_legs.unwrap_or(if phase_fixtures.len() > ties.len() {
                    2
                } else {
                    1
                })
            };

            for ((a, b), mut legs) in ties {
                legs.sort_by_key(|m| m.leg);
                if legs.len() != expected {
                    let mut v = simple_err(
                        "knockout_leg_count".to_string(),
                        format!(
                            "Tie {a}-{b} has {} leg(s); expected {expected}.{}",
                            legs.len(),
                            if allow_byes {
                                ""
                            } else {
                                " Byes are disabled."
                            }
                        ),
                        competition_id,
                        season_id,
                    );
                    v.round = Some(phase);
                    v.phase = Some(phase);
                    out.push(v);
                }
                if legs.len() == 2 {
                    let first = legs[0];
                    let second = legs[1];
                    if first.home_team_id != second.away_team_id
                        || first.away_team_id != second.home_team_id
                    {
                        out.push(match_err(
                            "knockout_no_reversal",
                            "The second leg does not reverse the first-leg venues.",
                            competition_id,
                            season_id,
                            second,
                        ));
                    }
                }
                for m in legs {
                    if m.status == MatchStatus::Finished {
                        match m.winner_id {
                            None => out.push(match_err(
                                "knockout_missing_winner",
                                "Completed knockout fixture has no winner.",
                                competition_id,
                                season_id,
                                m,
                            )),
                            Some(w) if w.get() == 0 => out.push(match_err(
                                "knockout_missing_winner",
                                "Completed knockout fixture has no winner.",
                                competition_id,
                                season_id,
                                m,
                            )),
                            Some(w) if w != m.home_team_id && w != m.away_team_id => {
                                out.push(match_err(
                                    "knockout_invalid_winner",
                                    "Winner is not one of the two clubs.",
                                    competition_id,
                                    season_id,
                                    m,
                                ));
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        FixtureValidationReport { violations: out }
    }

    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn validate_calendar(
        fixtures: &[FixtureMatch],
        min_rest_days: i64,
        min_rest_days_by_competition: &HashMap<TournamentId, i64>,
    ) -> FixtureValidationReport {
        type IdentityKey = (
            SeasonId,
            TournamentId,
            i32,
            i32,
            i32,
            TeamId,
            TeamId,
            Option<i32>,
        );
        type RoundDatesMap<'a> = HashMap<
            (TournamentId, i32, i32, i32),
            BTreeMap<i32, Vec<(NaiveDate, &'a FixtureMatch)>>,
        >;

        let mut out = Vec::new();
        let mut match_ids = HashSet::new();
        let mut by_club: HashMap<TeamId, Vec<&FixtureMatch>> = HashMap::new();
        let mut by_stadium: HashMap<StadiumId, Vec<&FixtureMatch>> = HashMap::new();
        let mut identities: HashMap<IdentityKey, MatchId> = HashMap::new();
        let mut dates_by_round: RoundDatesMap<'_> = HashMap::new();
        let mut legs_by_tie: HashMap<(TournamentId, i32, TeamId, TeamId), Vec<&FixtureMatch>> =
            HashMap::new();

        for m in fixtures {
            let cid = m.tournament_id;
            let sid = m.season_id;
            if !match_ids.insert(m.match_id) {
                out.push(match_err(
                    "calendar_duplicate_id",
                    "Duplicate fixture id.",
                    cid,
                    sid,
                    m,
                ));
            }
            if m.home_team_id.get() == 0 || m.away_team_id.get() == 0 {
                out.push(match_err(
                    "calendar_invalid_club",
                    "Fixture has an invalid club reference.",
                    cid,
                    sid,
                    m,
                ));
            }
            let ident = (
                sid,
                cid,
                m.phase,
                m.leg,
                m.round,
                m.home_team_id,
                m.away_team_id,
                m.group_id,
            );
            if let Some(prev_id) = identities.insert(ident, m.match_id) {
                out.push(match_err(
                    "calendar_duplicate_fixture",
                    &format!(
                        "The same fixture identity appears twice (fixtures {prev_id} and {}).",
                        m.match_id
                    ),
                    cid,
                    sid,
                    m,
                ));
            }
            if m.status == MatchStatus::Cancelled {
                continue;
            }
            let Some(date) = m.date else {
                out.push(match_err(
                    "calendar_invalid_date",
                    "Fixture has no valid calendar date.",
                    cid,
                    sid,
                    m,
                ));
                continue;
            };
            by_club.entry(m.home_team_id).or_default().push(m);
            by_club.entry(m.away_team_id).or_default().push(m);
            if m.stadium_id.get() > 0 {
                by_stadium.entry(m.stadium_id).or_default().push(m);
            }
            dates_by_round
                .entry((cid, m.phase, m.group_id.unwrap_or(-1), m.leg))
                .or_default()
                .entry(m.round)
                .or_default()
                .push((date, m));
            if m.phase > 0 && m.group_id.is_none() {
                let (pa, pb) = pair_key(m.home_team_id, m.away_team_id);
                legs_by_tie
                    .entry((cid, m.phase, pa, pb))
                    .or_default()
                    .push(m);
            }
        }

        for (club_id, mut list) in by_club {
            list.sort_by_key(|m| (m.date, m.match_id));
            for pair in list.windows(2) {
                let (Some(prev), Some(curr)) = (pair[0].date, pair[1].date) else {
                    continue;
                };
                let days = (curr - prev).num_days();
                let req = min_rest_days.max(
                    min_rest_days_by_competition
                        .get(&pair[1].tournament_id)
                        .copied()
                        .unwrap_or(min_rest_days),
                );
                if days == 0 {
                    out.push(match_err(
                        "calendar_overlap",
                        &format!("Club {club_id} has two fixtures on {curr}."),
                        pair[1].tournament_id,
                        pair[1].season_id,
                        pair[1],
                    ));
                } else if days < req {
                    out.push(FixtureViolation {
                        code: "calendar_rest".to_string(),
                        message: format!(
                            "Club {club_id} has only {days} day(s) between fixtures {prev} and {curr}; minimum is {req}."
                        ),
                        competition_id: pair[1].tournament_id,
                        season_id: pair[1].season_id,
                        match_id: Some(pair[1].match_id),
                        club_id: Some(club_id),
                        round: None,
                        phase: None,
                        severity: FixtureViolationSeverity::Warning,
                    });
                }
            }
        }

        for (stadium_id, mut list) in by_stadium {
            list.sort_by_key(|m| (m.date, m.match_id));
            for pair in list.windows(2) {
                if pair[0].date.is_some() && pair[0].date == pair[1].date {
                    out.push(match_err(
                        "calendar_stadium_overlap",
                        &format!(
                            "Stadium {stadium_id} hosts multiple fixtures on {}.",
                            pair[1].date.map_or_else(String::new, |d| d.to_string())
                        ),
                        pair[1].tournament_id,
                        pair[1].season_id,
                        pair[1],
                    ));
                }
            }
        }

        for rounds in dates_by_round.values() {
            let mut prev_max: Option<NaiveDate> = None;
            for entries in rounds.values() {
                let cur_min = entries.iter().map(|(d, _)| *d).min();
                let cur_max = entries.iter().map(|(d, _)| *d).max();
                if let (Some(p_max), Some(c_min)) = (prev_max, cur_min) {
                    if c_min < p_max {
                        if let Some((_, m)) = entries.iter().min_by_key(|(d, _)| *d) {
                            out.push(match_err(
                                "calendar_round_order",
                                &format!(
                                    "Round {} starts before previous round is complete in this phase.",
                                    m.round
                                ),
                                m.tournament_id,
                                m.season_id,
                                m,
                            ));
                        }
                    }
                }
                if let Some(c_max) = cur_max {
                    prev_max = Some(prev_max.map_or(c_max, |p| p.max(c_max)));
                }
            }
        }

        for mut legs in legs_by_tie.into_values() {
            legs.sort_by_key(|m| m.leg);
            if legs.len() > 1 {
                if let (Some(d1), Some(d2)) = (legs[0].date, legs[legs.len() - 1].date) {
                    if d2 <= d1 {
                        let last = legs[legs.len() - 1];
                        out.push(match_err(
                            "calendar_knockout_leg_order",
                            "The return leg must be scheduled after the first leg.",
                            last.tournament_id,
                            last.season_id,
                            last,
                        ));
                    }
                }
            }
        }

        FixtureValidationReport { violations: out }
    }
}

const fn pair_key(a: TeamId, b: TeamId) -> (TeamId, TeamId) {
    if a.get() < b.get() {
        (a, b)
    } else {
        (b, a)
    }
}

fn unique_sorted_clubs(ids: &[TeamId]) -> Vec<TeamId> {
    let mut seen = HashSet::with_capacity(ids.len());
    ids.iter().copied().filter(|id| seen.insert(*id)).collect()
}

fn simple_err(
    code: String,
    message: String,
    competition_id: TournamentId,
    season_id: SeasonId,
) -> FixtureViolation {
    FixtureViolation {
        code,
        message,
        competition_id,
        season_id,
        match_id: None,
        club_id: None,
        round: None,
        phase: None,
        severity: FixtureViolationSeverity::Error,
    }
}

fn match_err(
    code: &str,
    message: &str,
    competition_id: TournamentId,
    season_id: SeasonId,
    m: &FixtureMatch,
) -> FixtureViolation {
    FixtureViolation {
        code: code.to_string(),
        message: message.to_string(),
        competition_id,
        season_id,
        match_id: Some(m.match_id),
        club_id: Some(m.home_team_id),
        round: Some(m.round),
        phase: Some(m.phase),
        severity: FixtureViolationSeverity::Error,
    }
}
