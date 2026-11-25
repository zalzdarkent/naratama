use std::collections::{BTreeMap, HashMap};

use naratama_db::db_err;
use naratama_db::intl_repo::IntlRepo;
use naratama_model::{
    Confederation, MatchId, MatchStatus, NaratamaError, NaratamaResult, Player, PlayerId, Season,
    SeasonId, Stadium, StandingRow, Team, TeamId, TournamentId, TournamentOrg,
};
use naratama_sim::{build_tactical_dossier, compute_standings, TacticalDossier};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::manager_service::{ManagerOfferRow, ManagerOfferService};

/// One fixture row with resolved club names, shared by the schedule, bracket and match sheets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureRow {
    pub match_id: MatchId,
    pub date: Option<String>,
    pub phase: i32,
    pub round: i32,
    pub leg: i32,
    pub group_id: Option<i32>,
    pub status: i32,
    pub home_team_id: TeamId,
    pub away_team_id: TeamId,
    pub home_name: String,
    pub away_name: String,
    pub home_goals: Option<i32>,
    pub away_goals: Option<i32>,
    pub penalty_home_goals: Option<i32>,
    pub penalty_away_goals: Option<i32>,
    pub winner_id: Option<TeamId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BracketRound {
    pub phase: i32,
    pub name: String,
    pub fixtures: Vec<FixtureRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupTable {
    pub group_id: i32,
    pub rows: Vec<StandingRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderRow {
    pub player_id: PlayerId,
    pub player_name: String,
    pub team_id: TeamId,
    pub team_name: String,
    pub value: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompetitionOverview {
    pub tournament_id: TournamentId,
    pub name: String,
    pub organization: i32,
    pub standings: Vec<StandingRow>,
    pub groups: Vec<GroupTable>,
    pub fixtures: Vec<FixtureRow>,
    pub bracket: Vec<BracketRound>,
    pub top_scorers: Vec<LeaderRow>,
    pub top_assists: Vec<LeaderRow>,
    pub top_rated: Vec<LeaderRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchPlayerStat {
    pub player_id: PlayerId,
    pub player_name: String,
    pub team_id: TeamId,
    pub position_id: i32,
    pub starter: bool,
    pub minutes: i32,
    pub goals: i32,
    pub assists: i32,
    pub yellow_cards: i32,
    pub red_cards: i32,
    pub mvp: bool,
    pub rating: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchStatistics {
    pub match_id: MatchId,
    pub date: Option<String>,
    pub tournament_name: String,
    pub home_team_id: TeamId,
    pub away_team_id: TeamId,
    pub home_name: String,
    pub away_name: String,
    pub home_goals: i32,
    pub away_goals: i32,
    pub penalty_home_goals: Option<i32>,
    pub penalty_away_goals: Option<i32>,
    pub home_shots: i32,
    pub away_shots: i32,
    pub home_shots_on_target: i32,
    pub away_shots_on_target: i32,
    pub home_saves: i32,
    pub away_saves: i32,
    pub home_possession: i32,
    pub attendance: i32,
    pub revenue: i64,
    pub players: Vec<MatchPlayerStat>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HonourRow {
    pub tournament_id: TournamentId,
    pub tournament_name: String,
    pub is_international: bool,
    pub is_national_team: bool,
    pub winner_name: String,
    pub winner_id: Option<i64>,
    pub runner_up_name: Option<String>,
    pub runner_up_id: Option<i64>,
    pub third_place_name: Option<String>,
    pub score_line: Option<String>,
    pub notes: Option<String>,
    /// Season the honour belongs to; differs from the requested season for quadrennial events.
    pub edition_season_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeasonAwardRow {
    pub award_type: i32,
    pub player_id: Option<PlayerId>,
    pub player_name: Option<String>,
    pub team_id: Option<TeamId>,
    pub team_name: Option<String>,
    pub year: i32,
    pub goals: Option<i32>,
    pub rating: Option<f64>,
    pub subtitle: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamDetail {
    pub team: Team,
    pub stadium: Option<Stadium>,
    pub league_name: String,
    pub squad_size: i32,
    pub average_power: i32,
    pub squad_value: i64,
    pub top_players: Vec<Player>,
    pub recent_results: Vec<FixtureRow>,
    pub next_fixtures: Vec<FixtureRow>,
    pub squad: Vec<Player>,
    pub weekly_payroll: i64,
    pub trophies: Vec<TeamTrophy>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamTrophy {
    pub season_id: i64,
    pub tournament_name: String,
    pub score_line: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractRow {
    pub end_date: Option<String>,
    pub salary: i64,
    pub goal_bonus: Option<i64>,
    pub assist_bonus: Option<i64>,
    pub mvp_bonus: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeasonStatRow {
    pub season_id: SeasonId,
    pub matches_played: i32,
    pub starts: i32,
    pub minutes: i32,
    pub rating: i32,
    pub goals: i32,
    pub assists: i32,
    pub yellow_cards: i32,
    pub red_cards: i32,
    pub mvps: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerDetail {
    pub player: Player,
    pub club_name: Option<String>,
    pub contract: Option<ContractRow>,
    pub season_stats: Vec<SeasonStatRow>,
    pub recent_ratings: Vec<(String, i32, bool)>,
    pub national_caps: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoutInfo {
    pub scout_id: i64,
    pub name: String,
    pub stars: i32,
    pub region: String,
    pub salary: i64,
    pub status: String,
    pub return_date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoutReportInfo {
    pub report_id: i64,
    pub scout_name: String,
    pub region: String,
    pub date: String,
    pub player_name: String,
    pub age: i32,
    pub position_id: i32,
    pub power: i32,
    pub potential: i32,
    pub value: i64,
    pub country_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcademyProspect {
    pub prospect_id: i64,
    pub name: String,
    pub age: i32,
    pub position_id: i32,
    pub power: i32,
    pub potential: i32,
    pub stars: i32,
    pub profile: String,
    pub height: i32,
    pub country_id: i64,
    pub created_date: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SquadStatRow {
    pub player_id: PlayerId,
    pub player_name: String,
    pub position_id: i32,
    pub age: i32,
    pub power: i32,
    pub matches_played: i32,
    pub starts: i32,
    pub minutes: i32,
    pub goals: i32,
    pub assists: i32,
    pub yellow_cards: i32,
    pub red_cards: i32,
    pub mvps: i32,
    pub rating: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledTransferInfo {
    pub offer_id: i64,
    pub player_name: String,
    pub from_team_name: String,
    pub to_team_name: String,
    pub offer_amount: i64,
    pub is_loan: bool,
    pub loan_seasons: i32,
    pub effective_date: String,
    pub status: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DutyRequestInfo {
    pub duty_id: i64,
    pub player_name: String,
    pub country_name: String,
    pub window_id: String,
    pub release_date: String,
    pub return_date: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NationalSquadPlayer {
    pub player_id: PlayerId,
    pub player_name: String,
    pub position_id: i32,
    pub power: i32,
    pub age: i32,
    pub caps: i32,
    pub shirt_number: i32,
    pub is_captain: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfferInfo {
    pub offer_id: i64,
    pub player_id: PlayerId,
    pub player_name: String,
    pub from_team_id: TeamId,
    pub from_team_name: String,
    pub offer_amount: i64,
    pub player_value: i64,
    pub is_loan: bool,
    pub loan_seasons: i32,
    pub created_date: String,
    pub counter_amount: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NationalLeader {
    pub player_id: PlayerId,
    pub player_name: String,
    pub country_name: String,
    pub caps: i32,
    pub goals: i32,
    pub assists: i32,
    pub power: i32,
}

/// Read-only aggregates for the desktop data screens (standings, schedule, bracket, stats, history).
pub struct ReadService;

impl ReadService {
    pub fn season(conn: &Connection, season_id: SeasonId) -> NaratamaResult<Option<Season>> {
        let years: Option<(i32, i32)> =
            naratama_db::SeasonRepo::new(conn).season_years(season_id)?;
        Ok(years.map(|(start_year, end_year)| Season {
            season_id,
            name: format!("Season {start_year}/{:02}", end_year % 100),
            start_year,
            end_year,
        }))
    }

    /// All seasons played so far, newest first (for the history selector).
    pub fn seasons(conn: &Connection) -> NaratamaResult<Vec<Season>> {
        let mut stmt = conn
            .prepare("SELECT season_id, start_year, end_year FROM seasons ORDER BY season_id DESC")
            .map_err(db_err)?;
        let rows = stmt
            .query_map([], |r| {
                let season_id = SeasonId(r.get::<_, i64>(0)?);
                let start_year: i32 = r.get(1)?;
                let end_year: i32 = r.get(2)?;
                Ok(Season {
                    season_id,
                    name: format!("Season {start_year}/{:02}", end_year % 100),
                    start_year,
                    end_year,
                })
            })
            .map_err(db_err)?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    fn team_names(conn: &Connection) -> NaratamaResult<HashMap<TeamId, String>> {
        let mut stmt = conn
            .prepare("SELECT team_id, name FROM teams")
            .map_err(db_err)?;
        let rows = stmt
            .query_map([], |r| {
                Ok((TeamId(r.get::<_, i64>(0)?), r.get::<_, String>(1)?))
            })
            .map_err(db_err)?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    /// Club tournaments contested in a season (domestic leagues, domestic cup, and continental club cups).
    /// Excludes national team tournament IDs (57, 94..98, 900..906) and empty tournaments without fixtures.
    pub fn season_tournaments(
        conn: &Connection,
        season_id: SeasonId,
    ) -> NaratamaResult<Vec<(i64, String, i32)>> {
        let mut stmt = conn
            .prepare(
                "SELECT DISTINCT tu.tournament_id, tu.name, COALESCE(tu.organization, 1)
                 FROM tournaments tu
                 WHERE tu.tournament_id IN (
                    SELECT DISTINCT tournament_id FROM matches WHERE season_id = ?1
                 )
                   AND tu.tournament_id NOT IN (57, 94, 95, 96, 97, 98, 900, 901, 902, 903, 904, 905, 906)
                 ORDER BY
                    CASE COALESCE(tu.organization, 1)
                        WHEN 1 THEN 1
                        WHEN 3 THEN 2
                        WHEN 2 THEN 3
                        ELSE 4
                    END ASC,
                    COALESCE(tu.division_id, 99) ASC,
                    tu.tournament_id ASC",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![season_id.0], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get::<_, i32>(2)?))
            })
            .map_err(db_err)?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    pub fn fixtures_for_tournament(
        conn: &Connection,
        season_id: SeasonId,
        tournament_id: TournamentId,
    ) -> NaratamaResult<Vec<FixtureRow>> {
        let names = Self::team_names(conn)?;
        let mut stmt = conn
            .prepare(
                "SELECT match_id, date, phase, round, leg, group_id, status,
                        home_team_id, away_team_id, home_goals, away_goals,
                        penalty_home_goals, penalty_away_goals, winner_id
                 FROM matches
                 WHERE season_id = ?1 AND tournament_id = ?2
                 ORDER BY COALESCE(date, '9999-12-31') ASC, phase ASC, round ASC, match_id ASC",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![season_id.0, tournament_id.0], |r| {
                let home_team_id = TeamId(r.get::<_, i64>(7)?);
                let away_team_id = TeamId(r.get::<_, i64>(8)?);
                Ok(FixtureRow {
                    match_id: MatchId(r.get(0)?),
                    date: r.get(1)?,
                    phase: r.get(2)?,
                    round: r.get(3)?,
                    leg: r.get(4)?,
                    group_id: r.get(5)?,
                    status: r.get(6)?,
                    home_team_id,
                    away_team_id,
                    home_name: names
                        .get(&home_team_id)
                        .cloned()
                        .unwrap_or_else(|| format!("Club #{}", home_team_id.0)),
                    away_name: names
                        .get(&away_team_id)
                        .cloned()
                        .unwrap_or_else(|| format!("Club #{}", away_team_id.0)),
                    home_goals: r.get(9)?,
                    away_goals: r.get(10)?,
                    penalty_home_goals: r.get(11)?,
                    penalty_away_goals: r.get(12)?,
                    winner_id: r.get::<_, Option<i64>>(13)?.map(TeamId),
                })
            })
            .map_err(db_err)?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    fn standings_from_fixtures(
        fixtures: &[FixtureRow],
        names: &HashMap<TeamId, String>,
    ) -> Vec<StandingRow> {
        let mut teams: Vec<TeamId> = Vec::new();
        for f in fixtures {
            if !teams.contains(&f.home_team_id) {
                teams.push(f.home_team_id);
            }
            if !teams.contains(&f.away_team_id) {
                teams.push(f.away_team_id);
            }
        }
        if teams.is_empty() {
            return Vec::new();
        }
        let team_names_map: HashMap<TeamId, String> = names.clone();
        let rows: Vec<naratama_model::FixtureMatch> = fixtures
            .iter()
            .map(|f| naratama_model::FixtureMatch {
                match_id: f.match_id,
                tournament_id: TournamentId(0),
                season_id: SeasonId(0),
                round: f.round,
                phase: f.phase,
                group_id: f.group_id,
                status: MatchStatus::from_code(i64::from(f.status)),
                home_team_id: f.home_team_id,
                away_team_id: f.away_team_id,
                stadium_id: naratama_model::StadiumId(0),
                date: None,
                home_goals: f.home_goals,
                away_goals: f.away_goals,
                winner_id: f.winner_id,
                penalty_home_goals: f.penalty_home_goals,
                penalty_away_goals: f.penalty_away_goals,
                leg: f.leg,
                original_date: None,
                reschedule_reason: None,
                neutral_venue: false,
                extra_time: false,
                away_goals_rule: false,
            })
            .collect();
        compute_standings(&teams, &team_names_map, &rows)
    }

    pub fn competition_overview(
        conn: &Connection,
        season_id: SeasonId,
        tournament_id: TournamentId,
    ) -> NaratamaResult<CompetitionOverview> {
        let (name, organization) = conn
            .query_row(
                "SELECT name, COALESCE(organization, 1) FROM tournaments WHERE tournament_id = ?1",
                params![tournament_id.0],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, i32>(1)?)),
            )
            .optional()
            .map_err(db_err)?
            .ok_or_else(|| {
                NaratamaError::NotFound(format!("Tournament {} not found", tournament_id.0))
            })?;
        let fixtures = Self::fixtures_for_tournament(conn, season_id, tournament_id)?;
        let names = Self::team_names(conn)?;

        let groups = Self::group_tables(&fixtures, &names);
        let league_fixtures: Vec<FixtureRow> = fixtures
            .iter()
            .filter(|f| f.group_id.is_none() && f.phase <= 1)
            .cloned()
            .collect();
        let standings = if groups.is_empty() && organization != 3 {
            Self::standings_from_fixtures(&league_fixtures, &names)
        } else {
            Vec::new()
        };
        let bracket = if organization == 1 {
            Vec::new()
        } else {
            Self::bracket_rounds(&fixtures)
        };

        Ok(CompetitionOverview {
            tournament_id,
            name,
            organization,
            standings,
            groups,
            fixtures,
            bracket,
            top_scorers: Self::leaders(conn, season_id, tournament_id, LeaderKind::Goals)?,
            top_assists: Self::leaders(conn, season_id, tournament_id, LeaderKind::Assists)?,
            top_rated: Self::leaders(conn, season_id, tournament_id, LeaderKind::Rating)?,
        })
    }

    pub fn match_statistics(
        conn: &Connection,
        match_id: MatchId,
    ) -> NaratamaResult<Option<MatchStatistics>> {
        let Some(raw) = load_match_stats_row(conn, match_id)? else {
            return Ok(None);
        };
        let names = Self::team_names(conn)?;
        let home_name = names
            .get(&TeamId(raw.home_team_id))
            .cloned()
            .unwrap_or_else(|| format!("Club #{}", raw.home_team_id));
        let away_name = names
            .get(&TeamId(raw.away_team_id))
            .cloned()
            .unwrap_or_else(|| format!("Club #{}", raw.away_team_id));
        let tournament_name: String = conn
            .query_row(
                "SELECT COALESCE(t.name, 'Match') FROM matches m
                 LEFT JOIN tournaments t ON t.tournament_id = m.tournament_id
                 WHERE m.match_id = ?1",
                params![match_id.0],
                |r| r.get(0),
            )
            .unwrap_or_else(|_| "Match".to_string());
        let attendance: i32 = conn
            .query_row(
                "SELECT COALESCE(SUM(attendance_general + attendance_stand + attendance_terrace + attendance_boxes), 0)
                 FROM matches WHERE match_id = ?1",
                params![match_id.0],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let players = match_player_stats(conn, match_id)?;

        Ok(Some(MatchStatistics {
            match_id,
            date: raw.date,
            tournament_name,
            home_team_id: TeamId(raw.home_team_id),
            away_team_id: TeamId(raw.away_team_id),
            home_name,
            away_name,
            home_goals: raw.home_goals.unwrap_or(0),
            away_goals: raw.away_goals.unwrap_or(0),
            penalty_home_goals: raw.penalty_home_goals,
            penalty_away_goals: raw.penalty_away_goals,
            home_shots: raw.home_shots,
            away_shots: raw.away_shots,
            home_shots_on_target: raw.home_shots_on_target,
            away_shots_on_target: raw.away_shots_on_target,
            home_saves: raw.home_saves,
            away_saves: raw.away_saves,
            home_possession: raw.home_possession,
            attendance,
            revenue: raw.revenue,
            players,
        }))
    }

    fn group_tables(fixtures: &[FixtureRow], names: &HashMap<TeamId, String>) -> Vec<GroupTable> {
        let mut by_group: BTreeMap<i32, Vec<FixtureRow>> = BTreeMap::new();
        for f in fixtures.iter().filter(|f| f.group_id.is_some()) {
            by_group
                .entry(f.group_id.unwrap_or(1))
                .or_default()
                .push(f.clone());
        }
        by_group
            .into_iter()
            .map(|(group_id, rows)| GroupTable {
                group_id,
                rows: Self::standings_from_fixtures(&rows, names),
            })
            .collect()
    }

    fn bracket_rounds(fixtures: &[FixtureRow]) -> Vec<BracketRound> {
        let mut by_phase: BTreeMap<i32, Vec<FixtureRow>> = BTreeMap::new();
        for f in fixtures.iter().filter(|f| f.phase >= 1) {
            by_phase.entry(f.phase).or_default().push(f.clone());
        }
        by_phase
            .into_iter()
            .map(|(phase, fixtures)| BracketRound {
                phase,
                name: knockout_name(phase, fixtures.len()),
                fixtures,
            })
            .collect()
    }

    fn leaders(
        conn: &Connection,
        season_id: SeasonId,
        tournament_id: TournamentId,
        kind: LeaderKind,
    ) -> NaratamaResult<Vec<LeaderRow>> {
        let sql = match kind {
            LeaderKind::Goals => {
                "SELECT mp.player_id, COALESCE(p.name, 'Unknown'), mp.team_id,
                        COALESCE(t.name, ''), SUM(COALESCE(mp.goals, 0)) AS v
                 FROM match_players mp
                 JOIN matches m ON m.match_id = mp.match_id
                 LEFT JOIN players p ON p.player_id = mp.player_id
                 LEFT JOIN teams t ON t.team_id = mp.team_id
                 WHERE m.season_id = ?1 AND m.tournament_id = ?2 AND m.status = ?3
                 GROUP BY mp.player_id, mp.team_id
                 HAVING v > 0 ORDER BY v DESC LIMIT 20"
            }
            LeaderKind::Assists => {
                "SELECT mp.player_id, COALESCE(p.name, 'Unknown'), mp.team_id,
                        COALESCE(t.name, ''), SUM(COALESCE(mp.assists, 0)) AS v
                 FROM match_players mp
                 JOIN matches m ON m.match_id = mp.match_id
                 LEFT JOIN players p ON p.player_id = mp.player_id
                 LEFT JOIN teams t ON t.team_id = mp.team_id
                 WHERE m.season_id = ?1 AND m.tournament_id = ?2 AND m.status = ?3
                 GROUP BY mp.player_id, mp.team_id
                 HAVING v > 0 ORDER BY v DESC LIMIT 20"
            }
            LeaderKind::Rating => {
                "SELECT mp.player_id, COALESCE(p.name, 'Unknown'), mp.team_id,
                        COALESCE(t.name, ''), CAST(ROUND(AVG(mp.rating)) AS INTEGER) AS v
                 FROM match_players mp
                 JOIN matches m ON m.match_id = mp.match_id
                 LEFT JOIN players p ON p.player_id = mp.player_id
                 LEFT JOIN teams t ON t.team_id = mp.team_id
                 WHERE m.season_id = ?1 AND m.tournament_id = ?2 AND m.status = ?3
                   AND COALESCE(mp.minutes, 0) > 0
                 GROUP BY mp.player_id, mp.team_id
                 HAVING COUNT(*) >= 3 ORDER BY v DESC LIMIT 20"
            }
        };
        let mut stmt = conn.prepare(sql).map_err(db_err)?;
        let rows = stmt
            .query_map(
                params![season_id.0, tournament_id.0, MatchStatus::Finished as i64],
                |r| {
                    Ok(LeaderRow {
                        player_id: PlayerId(r.get(0)?),
                        player_name: r.get(1)?,
                        team_id: TeamId(r.get(2)?),
                        team_name: r.get(3)?,
                        value: r.get::<_, i64>(4)? as i32,
                    })
                },
            )
            .map_err(db_err)?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    /// Roll of honour for a season: league champions, cup finals and national-team podiums.
    pub fn season_honours(
        conn: &Connection,
        season_id: SeasonId,
    ) -> NaratamaResult<Vec<HonourRow>> {
        let names = Self::team_names(conn)?;
        let mut stmt = conn
            .prepare(
                "SELECT DISTINCT tu.tournament_id, tu.name, COALESCE(tu.organization, 1)
                 FROM tournaments tu
                 WHERE tu.tournament_id IN (
                    SELECT tournament_id FROM season_tournaments WHERE season_id = ?1
                    UNION
                    SELECT DISTINCT tournament_id FROM matches WHERE season_id = ?1
                 )
                 ORDER BY COALESCE(tu.organization, 1) ASC, tu.tournament_id ASC",
            )
            .map_err(db_err)?;
        let comps: Vec<(TournamentId, String, i32)> = stmt
            .query_map(params![season_id.0], |r| {
                Ok((TournamentId(r.get(0)?), r.get(1)?, r.get(2)?))
            })
            .map_err(db_err)?
            .filter_map(Result::ok)
            .collect();

        let mut honours = Vec::new();
        for (tournament_id, tournament_name, organization) in comps {
            let is_international = organization == 3 || organization == 4;
            let is_national_team = organization == 4;
            if is_national_team {
                if let Some(row) =
                    Self::national_honour(conn, season_id, tournament_id, &tournament_name)?
                {
                    honours.push(row);
                }
                continue;
            }
            let fixtures = Self::fixtures_for_tournament(conn, season_id, tournament_id)?;
            if fixtures.is_empty() {
                continue;
            }
            let final_phase = fixtures.iter().map(|f| f.phase).max().unwrap_or(0);
            let honour = if final_phase <= 1 {
                Self::league_honour(
                    &fixtures,
                    &names,
                    tournament_id,
                    &tournament_name,
                    is_international,
                    season_id,
                )
            } else {
                Self::cup_honour(
                    &fixtures,
                    final_phase,
                    tournament_id,
                    &tournament_name,
                    is_international,
                    season_id,
                )
            };
            if let Some(honour) = honour {
                honours.push(honour);
            }
        }
        Ok(honours)
    }

    fn league_honour(
        fixtures: &[FixtureRow],
        names: &HashMap<TeamId, String>,
        tournament_id: TournamentId,
        tournament_name: &str,
        is_international: bool,
        season_id: SeasonId,
    ) -> Option<HonourRow> {
        let league_fixtures: Vec<FixtureRow> = fixtures
            .iter()
            .filter(|f| f.group_id.is_none())
            .cloned()
            .collect();
        let table = Self::standings_from_fixtures(&league_fixtures, names);
        let champ = table.first()?;
        let runner = table.get(1);
        Some(HonourRow {
            tournament_id,
            tournament_name: tournament_name.to_string(),
            is_international,
            is_national_team: false,
            winner_name: champ.team_name.clone(),
            winner_id: Some(champ.team_id.0),
            runner_up_name: runner.map(|r| r.team_name.clone()),
            runner_up_id: runner.map(|r| r.team_id.0),
            third_place_name: table.get(2).map(|r| r.team_name.clone()),
            score_line: None,
            notes: Some(format!(
                "{} Pts ({}-{}-{})",
                champ.points, champ.wins, champ.draws, champ.losses
            )),
            edition_season_id: season_id.0,
        })
    }

    fn cup_honour(
        fixtures: &[FixtureRow],
        final_phase: i32,
        tournament_id: TournamentId,
        tournament_name: &str,
        is_international: bool,
        season_id: SeasonId,
    ) -> Option<HonourRow> {
        let mut last_round: Vec<&FixtureRow> =
            fixtures.iter().filter(|f| f.phase == final_phase).collect();
        last_round.sort_by_key(|f| (f.leg, f.match_id.0));
        let fin = last_round.last()?;
        let winner_id = fin.winner_id?;
        let (champ_name, runner_name, runner_id) = if winner_id == fin.home_team_id {
            (&fin.home_name, &fin.away_name, fin.away_team_id)
        } else {
            (&fin.away_name, &fin.home_name, fin.home_team_id)
        };
        let score = match (fin.home_goals, fin.away_goals) {
            (Some(h), Some(a)) => {
                let mut line = format!("{h} - {a}");
                if let (Some(ph), Some(pa)) = (fin.penalty_home_goals, fin.penalty_away_goals) {
                    if ph > 0 || pa > 0 {
                        let _ =
                            std::fmt::Write::write_fmt(&mut line, format_args!(" ({ph}-{pa} pen)"));
                    }
                }
                Some(line)
            }
            _ => None,
        };
        Some(HonourRow {
            tournament_id,
            tournament_name: tournament_name.to_string(),
            is_international,
            is_national_team: false,
            winner_name: champ_name.clone(),
            winner_id: Some(winner_id.0),
            runner_up_name: Some(runner_name.clone()),
            runner_up_id: Some(runner_id.0),
            third_place_name: None,
            score_line: score,
            notes: None,
            edition_season_id: season_id.0,
        })
    }

    fn national_honour(
        conn: &Connection,
        season_id: SeasonId,
        tournament_id: TournamentId,
        tournament_name: &str,
    ) -> NaratamaResult<Option<HonourRow>> {
        let mut edition = season_id.0;
        let mut podium = crate::InternationalTournamentService::get_tournament_podium(
            conn,
            tournament_id,
            season_id,
        )?;
        if podium.is_none() {
            let past: Option<i64> = conn
                .query_row(
                    "SELECT season_id FROM country_matches
                     WHERE tournament_id = ?1 AND season_id <= ?2 AND status = ?3 AND phase > 0
                     ORDER BY season_id DESC LIMIT 1",
                    params![tournament_id.0, season_id.0, MatchStatus::Finished as i64],
                    |r| r.get(0),
                )
                .optional()
                .map_err(db_err)?;
            if let Some(past_season) = past {
                edition = past_season;
                podium = crate::InternationalTournamentService::get_tournament_podium(
                    conn,
                    tournament_id,
                    SeasonId(past_season),
                )?;
            }
        }
        let Some(podium) = podium else {
            return Ok(None);
        };
        let country_name = |id: naratama_model::CountryId| -> NaratamaResult<String> {
            Ok(IntlRepo::country_by_id(conn, id)?
                .map_or_else(|| format!("Nation #{}", id.0), |c| c.name))
        };
        Ok(Some(HonourRow {
            tournament_id,
            tournament_name: tournament_name.to_string(),
            is_international: true,
            is_national_team: true,
            winner_name: country_name(podium.champion_country_id)?,
            winner_id: Some(podium.champion_country_id.0),
            runner_up_name: Some(country_name(podium.runner_up_country_id)?),
            runner_up_id: Some(podium.runner_up_country_id.0),
            third_place_name: match podium.third_place_country_id {
                Some(id) => Some(country_name(id)?),
                None => None,
            },
            score_line: None,
            notes: (edition != season_id.0).then(|| format!("Edition season {edition}")),
            edition_season_id: edition,
        }))
    }

    pub fn season_awards(
        conn: &Connection,
        season_id: SeasonId,
    ) -> NaratamaResult<Vec<SeasonAwardRow>> {
        let mut stmt = conn
            .prepare(
                "SELECT COALESCE(pa.type, 0), pa.player_id, p.name, pa.team_id, t.name,
                        COALESCE(pa.year, 0), pa.goals, pa.rating, pa.stat_subtitle
                 FROM player_awards pa
                 LEFT JOIN players p ON p.player_id = pa.player_id
                 LEFT JOIN teams t ON t.team_id = pa.team_id
                 WHERE pa.season_id = ?1
                 ORDER BY pa.type ASC, pa.year ASC",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![season_id.0], |r| {
                Ok(SeasonAwardRow {
                    award_type: r.get(0)?,
                    player_id: r.get::<_, Option<i64>>(1)?.map(PlayerId),
                    player_name: r.get(2)?,
                    team_id: r.get::<_, Option<i64>>(3)?.map(TeamId),
                    team_name: r.get(4)?,
                    year: r.get(5)?,
                    goals: r.get(6)?,
                    rating: r.get(7)?,
                    subtitle: r.get(8)?,
                })
            })
            .map_err(db_err)?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    pub fn team_detail(conn: &Connection, team_id: TeamId) -> NaratamaResult<Option<TeamDetail>> {
        let Some(team) =
            naratama_db::reference_repo::ReferenceRepo::new(conn).team_by_id(team_id)?
        else {
            return Ok(None);
        };
        let stadium =
            naratama_db::reference_repo::ReferenceRepo::new(conn).stadium_by_id(team.stadium_id)?;
        let league_name: String = conn
            .query_row(
                "SELECT COALESCE(name, 'Unknown league') FROM tournaments
                 WHERE COALESCE(organization, 1) = 1 AND country_id = ?1 AND division_id = ?2
                 ORDER BY tournament_id ASC LIMIT 1",
                params![team.country_id.0, team.division],
                |r| r.get(0),
            )
            .unwrap_or_else(|_| "Unknown league".to_string());

        let squad: Vec<Player> = naratama_db::club_repo::ClubRepo::squad_for_team(conn, team_id)?;
        let mut squad = squad;
        squad.sort_by_key(|p| std::cmp::Reverse(p.power));

        let squad_size = i32::try_from(squad.len()).unwrap_or(0);
        let average_power = if squad.is_empty() {
            0
        } else {
            squad.iter().map(|p| p.power).sum::<i32>() / squad_size.max(1)
        };
        let squad_value: i64 = squad.iter().map(|p| p.value).sum();

        let recent_results = Self::fixtures_for_team(conn, team_id, true, 5)?;
        let next_fixtures = Self::fixtures_for_team(conn, team_id, false, 5)?;
        let weekly_payroll: i64 = squad.iter().map(|p| p.salary).sum();
        let trophies = Self::club_trophy_rows(conn, team_id)?;
        Ok(Some(TeamDetail {
            team,
            stadium,
            league_name,
            squad_size,
            average_power,
            squad_value,
            top_players: squad.iter().take(5).cloned().collect(),
            recent_results,
            next_fixtures,
            squad,
            weekly_payroll,
            trophies,
        }))
    }

    fn fixtures_for_team(
        conn: &Connection,
        team_id: TeamId,
        finished: bool,
        limit: i64,
    ) -> NaratamaResult<Vec<FixtureRow>> {
        let names = Self::team_names(conn)?;
        let status_clause = if finished {
            format!("m.status = {}", MatchStatus::Finished as i64)
        } else {
            format!("m.status = {}", MatchStatus::Scheduled as i64)
        };
        let order = if finished { "DESC" } else { "ASC" };
        let sql = format!(
            "SELECT m.match_id, m.date, m.phase, m.round, m.leg, m.group_id, m.status,
                    m.home_team_id, m.away_team_id, m.home_goals, m.away_goals,
                    m.penalty_home_goals, m.penalty_away_goals, m.winner_id
             FROM matches m
             WHERE {status_clause} AND (m.home_team_id = ?1 OR m.away_team_id = ?1)
             ORDER BY COALESCE(m.date, '9999-12-31') {order}, m.match_id {order}
             LIMIT ?2"
        );
        let mut stmt = conn.prepare(&sql).map_err(db_err)?;
        let rows = stmt
            .query_map(params![team_id.0, limit], |r| {
                let home_team_id = TeamId(r.get::<_, i64>(7)?);
                let away_team_id = TeamId(r.get::<_, i64>(8)?);
                Ok(FixtureRow {
                    match_id: MatchId(r.get(0)?),
                    date: r.get(1)?,
                    phase: r.get(2)?,
                    round: r.get(3)?,
                    leg: r.get(4)?,
                    group_id: r.get(5)?,
                    status: r.get(6)?,
                    home_team_id,
                    away_team_id,
                    home_name: names
                        .get(&home_team_id)
                        .cloned()
                        .unwrap_or_else(|| format!("Club #{}", home_team_id.0)),
                    away_name: names
                        .get(&away_team_id)
                        .cloned()
                        .unwrap_or_else(|| format!("Club #{}", away_team_id.0)),
                    home_goals: r.get(9)?,
                    away_goals: r.get(10)?,
                    penalty_home_goals: r.get(11)?,
                    penalty_away_goals: r.get(12)?,
                    winner_id: r.get::<_, Option<i64>>(13)?.map(TeamId),
                })
            })
            .map_err(db_err)?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    pub fn player_detail(
        conn: &Connection,
        player_id: PlayerId,
    ) -> NaratamaResult<Option<PlayerDetail>> {
        let Some(player) = load_player(conn, player_id)? else {
            return Ok(None);
        };
        let club_name: Option<String> = conn
            .query_row(
                "SELECT t.name FROM players p
                 LEFT JOIN team_players tp ON tp.player_id = p.player_id
                 LEFT JOIN teams t ON t.team_id = tp.team_id
                 WHERE p.player_id = ?1",
                params![player_id.0],
                |r| r.get(0),
            )
            .optional()
            .map_err(db_err)?
            .flatten();
        let contract: Option<ContractRow> = conn
            .query_row(
                "SELECT end_date, COALESCE(salary, 0), goal_bonus, assist_bonus, mvp_bonus
                 FROM contracts WHERE player_id = ?1 AND COALESCE(status, 1) = 1
                 ORDER BY end_date DESC LIMIT 1",
                params![player_id.0],
                |r| {
                    Ok(ContractRow {
                        end_date: r.get(0)?,
                        salary: r.get(1)?,
                        goal_bonus: r.get(2)?,
                        assist_bonus: r.get(3)?,
                        mvp_bonus: r.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(db_err)?;

        let mut stmt = conn
            .prepare(
                "SELECT season_id, matches_played, starts, minutes, rating, goals, assists,
                        yellow_cards, red_cards, mvps
                 FROM player_season_stats WHERE player_id = ?1
                 ORDER BY season_id DESC",
            )
            .map_err(db_err)?;
        let season_stats: Vec<SeasonStatRow> = stmt
            .query_map(params![player_id.0], |r| {
                Ok(SeasonStatRow {
                    season_id: SeasonId(r.get::<_, Option<i64>>(0)?.unwrap_or(0)),
                    matches_played: r.get::<_, Option<i32>>(1)?.unwrap_or(0),
                    starts: r.get::<_, Option<i32>>(2)?.unwrap_or(0),
                    minutes: r.get::<_, Option<i32>>(3)?.unwrap_or(0),
                    rating: r.get::<_, Option<i32>>(4)?.unwrap_or(0),
                    goals: r.get::<_, Option<i32>>(5)?.unwrap_or(0),
                    assists: r.get::<_, Option<i32>>(6)?.unwrap_or(0),
                    yellow_cards: r.get::<_, Option<i32>>(7)?.unwrap_or(0),
                    red_cards: r.get::<_, Option<i32>>(8)?.unwrap_or(0),
                    mvps: r.get::<_, Option<i32>>(9)?.unwrap_or(0),
                })
            })
            .map_err(db_err)?
            .filter_map(Result::ok)
            .collect();

        let mut stmt = conn
            .prepare(
                "SELECT COALESCE(m.date, ''), COALESCE(mp.rating, 0), COALESCE(mp.mvp, 0)
                 FROM match_players mp JOIN matches m ON m.match_id = mp.match_id
                 WHERE mp.player_id = ?1 AND COALESCE(mp.minutes, 0) > 0
                 ORDER BY COALESCE(m.date, '') DESC LIMIT 5",
            )
            .map_err(db_err)?;
        let recent_ratings: Vec<(String, i32, bool)> = stmt
            .query_map(params![player_id.0], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get::<_, i32>(2)? != 0))
            })
            .map_err(db_err)?
            .filter_map(Result::ok)
            .collect();

        let national_caps: i32 = conn
            .query_row(
                "SELECT COALESCE(national_caps, 0) FROM players WHERE player_id = ?1",
                params![player_id.0],
                |r| r.get(0),
            )
            .unwrap_or(0);

        Ok(Some(PlayerDetail {
            player,
            club_name,
            contract,
            season_stats,
            recent_ratings,
            national_caps,
        }))
    }

    /// Pre-match tactical dossier for the opponent of `team_id`.
    pub fn tactical_dossier(
        conn: &Connection,
        team_id: TeamId,
        analyst_stars: i32,
    ) -> NaratamaResult<Option<(String, TacticalDossier)>> {
        let Some(team) =
            naratama_db::reference_repo::ReferenceRepo::new(conn).team_by_id(team_id)?
        else {
            return Ok(None);
        };
        let squad = naratama_db::club_repo::ClubRepo::squad_for_team(conn, team_id)?;
        let (formation, _) = naratama_db::club_repo::ClubRepo::lineup_and_formation(conn, team_id)?;
        let dossier = build_tactical_dossier(
            team.power,
            naratama_sim::Formation::parse(Some(&formation)),
            &squad,
            analyst_stars,
        );
        Ok(Some((team.name, dossier)))
    }

    /// Tournament podium list for the international hub.
    pub fn international_podiums(
        conn: &Connection,
        season_id: SeasonId,
    ) -> NaratamaResult<Vec<HonourRow>> {
        let mut rows = Vec::new();
        for (tournament_id, name) in INTERNATIONAL_TOURNAMENTS {
            if let Some(row) =
                Self::national_honour(conn, season_id, TournamentId(tournament_id), name)?
            {
                rows.push(row);
            }
        }
        Ok(rows)
    }

    /// Club trophies across every played season (roll of honour for the club hub).
    pub fn club_trophies(
        conn: &Connection,
        team_id: TeamId,
    ) -> NaratamaResult<Vec<(i64, String, String)>> {
        let mut stmt = conn
            .prepare(
                "SELECT season_id, COALESCE(championships, '') FROM career_history
                 WHERE team_id = ?1 ORDER BY season_id DESC",
            )
            .map_err(db_err)?;
        let rows: Vec<(i64, String)> = stmt
            .query_map(params![team_id.0], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(db_err)?
            .filter_map(Result::ok)
            .collect();
        Ok(rows
            .into_iter()
            .filter(|(_, titles)| !titles.trim().is_empty())
            .map(|(season_id, titles)| (season_id, titles.clone(), titles))
            .collect())
    }

    /// Hired scouts of the user's club (status, region, return date).
    pub fn scouts(conn: &Connection, team_id: TeamId) -> NaratamaResult<Vec<ScoutInfo>> {
        let mut stmt = conn
            .prepare(
                "SELECT scout_id, name, stars, COALESCE(region, ''), COALESCE(salary, 0),
                        COALESCE(status, 'idle'), return_date
                 FROM scouts WHERE team_id = ?1 ORDER BY scout_id ASC",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![team_id.0], |r| {
                Ok(ScoutInfo {
                    scout_id: r.get(0)?,
                    name: r.get(1)?,
                    stars: r.get(2)?,
                    region: r.get(3)?,
                    salary: r.get(4)?,
                    status: r.get(5)?,
                    return_date: r.get(6)?,
                })
            })
            .map_err(db_err)?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    /// Unsigned scouting reports waiting on a decision.
    pub fn scout_reports(
        conn: &Connection,
        team_id: TeamId,
    ) -> NaratamaResult<Vec<ScoutReportInfo>> {
        let mut stmt = conn
            .prepare(
                "SELECT report_id, scout_name, region, date, player_name, age, position_id,
                        power, potential, COALESCE(value, 0), country_id
                 FROM scout_reports WHERE team_id = ?1 AND COALESCE(signed, 0) = 0
                 ORDER BY potential DESC, report_id DESC",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![team_id.0], |r| {
                Ok(ScoutReportInfo {
                    report_id: r.get(0)?,
                    scout_name: r.get(1)?,
                    region: r.get(2)?,
                    date: r.get(3)?,
                    player_name: r.get(4)?,
                    age: r.get(5)?,
                    position_id: r.get(6)?,
                    power: r.get(7)?,
                    potential: r.get(8)?,
                    value: r.get(9)?,
                    country_id: r.get(10)?,
                })
            })
            .map_err(db_err)?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    /// Academy intake waiting to be signed to a youth contract.
    pub fn academy_prospects(
        conn: &Connection,
        team_id: TeamId,
    ) -> NaratamaResult<Vec<AcademyProspect>> {
        let mut stmt = conn
            .prepare(
                "SELECT prospect_id, name, age, position_id, power, potential, stars,
                        profile, height, country_id, created_date
                 FROM academy_prospects
                 WHERE team_id = ?1 AND COALESCE(signed, 0) = 0
                 ORDER BY potential DESC, prospect_id DESC",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![team_id.0], |r| {
                Ok(AcademyProspect {
                    prospect_id: r.get(0)?,
                    name: r.get(1)?,
                    age: r.get(2)?,
                    position_id: r.get(3)?,
                    power: r.get(4)?,
                    potential: r.get(5)?,
                    stars: r.get(6)?,
                    profile: r.get(7)?,
                    height: r.get(8)?,
                    country_id: r.get(9)?,
                    created_date: r.get(10)?,
                })
            })
            .map_err(db_err)?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    /// Transfer offers awaiting the user's decision (incoming bids for own players).
    pub fn incoming_offers(conn: &Connection, team_id: TeamId) -> NaratamaResult<Vec<OfferInfo>> {
        let mut stmt = conn
            .prepare(
                "SELECT o.offer_id, o.player_id, COALESCE(p.name, 'Unknown'), o.from_team_id,
                        COALESCE(ft.name, ''), o.offer_amount, o.player_value, o.status,
                        o.is_loan, o.loan_seasons, o.created_date, o.counter_amount
                 FROM transfer_offers o
                 LEFT JOIN players p ON p.player_id = o.player_id
                 LEFT JOIN teams ft ON ft.team_id = o.from_team_id
                 WHERE o.to_team_id = ?1 AND o.status = 1
                 ORDER BY o.offer_id DESC",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![team_id.0], |r| {
                Ok(OfferInfo {
                    offer_id: r.get(0)?,
                    player_id: PlayerId(r.get(1)?),
                    player_name: r.get(2)?,
                    from_team_id: TeamId(r.get(3)?),
                    from_team_name: r.get(4)?,
                    offer_amount: r.get(5)?,
                    player_value: r.get(6)?,
                    is_loan: r.get::<_, i32>(8)? != 0,
                    loan_seasons: r.get(9)?,
                    created_date: r.get(10)?,
                    counter_amount: r.get(11)?,
                })
            })
            .map_err(db_err)?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    pub fn scout_report_row(
        conn: &Connection,
        report_id: i64,
    ) -> NaratamaResult<Option<naratama_db::market_repo::ScoutReportRow>> {
        let row = conn
            .query_row(
                "SELECT report_id, team_id, scout_id, scout_name, region, date, player_name,
                        age, position_id, power, potential, COALESCE(value, 0), country_id,
                        COALESCE(signed, 0)
                 FROM scout_reports WHERE report_id = ?1",
                params![report_id],
                |r| {
                    Ok(naratama_db::market_repo::ScoutReportRow {
                        report_id: r.get(0)?,
                        team_id: TeamId(r.get(1)?),
                        scout_id: r.get(2)?,
                        scout_name: r.get(3)?,
                        region: r.get(4)?,
                        date: parse_date(r.get(5)?),
                        player_name: r.get(6)?,
                        age: r.get(7)?,
                        position_id: r.get(8)?,
                        power: r.get(9)?,
                        potential: r.get(10)?,
                        value: r.get(11)?,
                        country_id: naratama_model::CountryId(r.get(12)?),
                        signed: r.get::<_, i32>(13)? != 0,
                    })
                },
            )
            .optional()
            .map_err(db_err)?;
        Ok(row)
    }

    pub fn youth_prospect_row(
        conn: &Connection,
        prospect_id: i64,
    ) -> NaratamaResult<Option<naratama_db::club_repo::YouthProspectRow>> {
        let row = conn
            .query_row(
                "SELECT prospect_id, team_id, name, age, position_id, power, potential, stars,
                        profile, height, country_id, created_date, COALESCE(signed, 0)
                 FROM academy_prospects WHERE prospect_id = ?1",
                params![prospect_id],
                |r| {
                    Ok(naratama_db::club_repo::YouthProspectRow {
                        prospect_id: r.get(0)?,
                        team_id: TeamId(r.get(1)?),
                        name: r.get(2)?,
                        age: r.get(3)?,
                        position_id: r.get(4)?,
                        power: r.get(5)?,
                        potential: r.get(6)?,
                        stars: r.get(7)?,
                        profile: r.get(8)?,
                        height: r.get(9)?,
                        country_id: naratama_model::CountryId(r.get(10)?),
                        created_date: parse_date(r.get(11)?),
                        signed: r.get::<_, i32>(12)? != 0,
                    })
                },
            )
            .optional()
            .map_err(db_err)?;
        Ok(row)
    }

    /// Pending international release requests involving the club's players.
    pub fn duty_requests(
        conn: &Connection,
        team_id: TeamId,
        season_id: SeasonId,
    ) -> NaratamaResult<Vec<DutyRequestInfo>> {
        let mut stmt = conn
            .prepare(
                "SELECT d.duty_id, COALESCE(p.name, 'Player'), COALESCE(c.name, 'Nation'),
                        d.window_id, d.release_date, d.return_date
                 FROM international_duty d
                 LEFT JOIN players p ON p.player_id = d.player_id
                 LEFT JOIN countries c ON c.country_id = d.country_id
                 JOIN team_players tp ON tp.player_id = d.player_id
                 WHERE tp.team_id = ?1 AND d.season_id = ?2
                   AND d.status = 'pending_release'
                 ORDER BY d.release_date ASC",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![team_id.0, season_id.0], |r| {
                Ok(DutyRequestInfo {
                    duty_id: r.get(0)?,
                    player_name: r.get(1)?,
                    country_name: r.get(2)?,
                    window_id: r.get(3)?,
                    release_date: r.get(4)?,
                    return_date: r.get(5)?,
                })
            })
            .map_err(db_err)?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    /// The nation's currently called-up squad for the season (23 names by default).
    pub fn national_squad(
        conn: &Connection,
        country_id: naratama_model::CountryId,
        season_id: SeasonId,
    ) -> NaratamaResult<Vec<NationalSquadPlayer>> {
        let mut stmt = conn
            .prepare(
                "SELECT d.player_id, COALESCE(p.name, 'Player'), COALESCE(p.position_id, 8),
                        COALESCE(p.power, 60), COALESCE(p.age, 24), COALESCE(p.national_caps, 0),
                        COALESCE(d.squad_number, 0), COALESCE(d.is_captain, 0),
                        COALESCE(p.club_id, 0)
                 FROM international_duty d
                 LEFT JOIN players p ON p.player_id = d.player_id
                 WHERE d.country_id = ?1 AND d.season_id = ?2 AND d.status = 'active'
                 ORDER BY COALESCE(d.squad_number, 99) ASC",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![country_id.0, season_id.0], |r| {
                Ok(NationalSquadPlayer {
                    player_id: PlayerId(r.get(0)?),
                    player_name: r.get(1)?,
                    position_id: r.get(2)?,
                    power: r.get(3)?,
                    age: r.get(4)?,
                    caps: r.get(5)?,
                    shirt_number: r.get(6)?,
                    is_captain: r.get::<_, i32>(7)? != 0,
                })
            })
            .map_err(db_err)?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    /// Countries whose squad has already been called up this season.
    pub fn called_up_countries(
        conn: &Connection,
        season_id: SeasonId,
    ) -> NaratamaResult<Vec<(i64, String)>> {
        let mut stmt = conn
            .prepare(
                "SELECT DISTINCT c.country_id, c.name
                 FROM international_duty d
                 JOIN countries c ON c.country_id = d.country_id
                 WHERE d.season_id = ?1 AND d.status = 'active'
                 ORDER BY c.name ASC",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![season_id.0], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(db_err)?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    /// Per-player season statistics for the squad statistics tab.
    pub fn squad_statistics(
        conn: &Connection,
        team_id: TeamId,
        season_id: SeasonId,
    ) -> NaratamaResult<Vec<SquadStatRow>> {
        let mut stmt = conn
            .prepare(
                "SELECT p.player_id, COALESCE(p.name, 'Unknown'), COALESCE(p.position_id, 8),
                        COALESCE(p.age, 24), COALESCE(p.power, 60),
                        COALESCE(ps.matches_played, 0), COALESCE(ps.starts, 0),
                        COALESCE(ps.minutes, 0), COALESCE(ps.goals, 0), COALESCE(ps.assists, 0),
                        COALESCE(ps.yellow_cards, 0), COALESCE(ps.red_cards, 0),
                        COALESCE(ps.mvps, 0), COALESCE(ps.rating, 0)
                 FROM team_players tp
                 JOIN players p ON p.player_id = tp.player_id
                 LEFT JOIN player_season_stats ps
                        ON ps.player_id = p.player_id AND ps.season_id = ?2
                 WHERE tp.team_id = ?1 AND COALESCE(p.status_id, 1) = 1
                 ORDER BY COALESCE(ps.goals, 0) DESC, COALESCE(ps.rating, 0) DESC, p.power DESC",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![team_id.0, season_id.0], |r| {
                Ok(SquadStatRow {
                    player_id: PlayerId(r.get(0)?),
                    player_name: r.get(1)?,
                    position_id: r.get(2)?,
                    age: r.get(3)?,
                    power: r.get(4)?,
                    matches_played: r.get(5)?,
                    starts: r.get(6)?,
                    minutes: r.get(7)?,
                    goals: r.get(8)?,
                    assists: r.get(9)?,
                    yellow_cards: r.get(10)?,
                    red_cards: r.get(11)?,
                    mvps: r.get(12)?,
                    rating: r.get(13)?,
                })
            })
            .map_err(db_err)?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    /// Transfer deals already agreed but waiting for the next window.
    pub fn scheduled_transfers(
        conn: &Connection,
        team_id: TeamId,
    ) -> NaratamaResult<Vec<ScheduledTransferInfo>> {
        let mut stmt = conn
            .prepare(
                "SELECT o.offer_id, COALESCE(p.name, 'Player'), COALESCE(ft.name, ''),
                        COALESCE(tt.name, ''), o.offer_amount, COALESCE(o.is_loan, 0),
                        COALESCE(o.loan_seasons, 1), COALESCE(o.effective_date, ''),
                        o.status
                 FROM transfer_offers o
                 LEFT JOIN players p ON p.player_id = o.player_id
                 LEFT JOIN teams ft ON ft.team_id = o.from_team_id
                 LEFT JOIN teams tt ON tt.team_id = o.to_team_id
                 WHERE (o.from_team_id = ?1 OR o.to_team_id = ?1)
                   AND o.status IN (2, 6, 9)
                 ORDER BY COALESCE(o.effective_date, '') ASC, o.offer_id ASC",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![team_id.0], |r| {
                Ok(ScheduledTransferInfo {
                    offer_id: r.get(0)?,
                    player_name: r.get(1)?,
                    from_team_name: r.get(2)?,
                    to_team_name: r.get(3)?,
                    offer_amount: r.get(4)?,
                    is_loan: r.get::<_, i32>(5)? != 0,
                    loan_seasons: r.get(6)?,
                    effective_date: r.get(7)?,
                    status: r.get(8)?,
                })
            })
            .map_err(db_err)?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    pub fn pending_manager_offer(conn: &Connection) -> NaratamaResult<Option<ManagerOfferRow>> {
        ManagerOfferService::pending_offer(conn)
    }

    /// Honours won by one club across every played season (from the roll of honour).
    pub fn club_trophy_rows(conn: &Connection, team_id: TeamId) -> NaratamaResult<Vec<TeamTrophy>> {
        let seasons = Self::seasons(conn)?;
        let mut trophies = Vec::new();
        for season in seasons {
            for honour in Self::season_honours(conn, season.season_id)? {
                if honour.winner_id == Some(team_id.0) {
                    trophies.push(TeamTrophy {
                        season_id: honour.edition_season_id,
                        tournament_name: honour.tournament_name,
                        score_line: honour.score_line,
                    });
                }
            }
        }
        Ok(trophies)
    }

    /// Logo data URIs for the requested clubs (used by tables that only carry names).
    pub fn logo_map(conn: &Connection, team_ids: &[i64]) -> NaratamaResult<Vec<(i64, String)>> {
        if team_ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut stmt = conn
            .prepare("SELECT team_id, logo_base64 FROM teams WHERE team_id = ?1")
            .map_err(db_err)?;
        let mut logos = Vec::new();
        for team_id in team_ids {
            let row: Option<Option<String>> = stmt
                .query_row(params![team_id], |r| r.get(1))
                .optional()
                .map_err(db_err)?;
            if let Some(Some(raw)) = row {
                if !raw.trim().is_empty() {
                    logos.push((*team_id, raw));
                }
            }
        }
        Ok(logos)
    }

    /// National-team leaders for the season: goals, assists and caps.
    pub fn national_leaders(
        conn: &Connection,
        season_id: SeasonId,
        limit: usize,
    ) -> NaratamaResult<Vec<NationalLeader>> {
        let limit = i64::try_from(limit).unwrap_or(20);
        let mut stmt = conn
            .prepare(
                "SELECT p.player_id, COALESCE(p.name, 'Player'),
                        COALESCE(c.name, ''),
                        COALESCE(st.matches_played, 0),
                        COALESCE(st.goals, 0),
                        COALESCE(st.assists, 0),
                        COALESCE(p.power, 60)
                 FROM player_season_stats_countries st
                 JOIN players p ON p.player_id = st.player_id
                 LEFT JOIN countries c ON c.country_id = st.country_id
                 WHERE st.season_id = ?1
                 ORDER BY COALESCE(st.goals, 0) DESC, COALESCE(st.assists, 0) DESC,
                          COALESCE(st.matches_played, 0) DESC
                 LIMIT ?2",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![season_id.0, limit], |r| {
                Ok(NationalLeader {
                    player_id: PlayerId(r.get(0)?),
                    player_name: r.get(1)?,
                    country_name: r.get(2)?,
                    caps: r.get(3)?,
                    goals: r.get(4)?,
                    assists: r.get(5)?,
                    power: r.get(6)?,
                })
            })
            .map_err(db_err)?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    pub fn confederation_for_country(
        conn: &Connection,
        country_id: naratama_model::CountryId,
    ) -> NaratamaResult<Confederation> {
        Ok(IntlRepo::country_by_id(conn, country_id)?
            .map_or(Confederation::None, |c| c.confederation))
    }
}

#[derive(Debug, Clone, Copy)]
enum LeaderKind {
    Goals,
    Assists,
    Rating,
}

struct RawMatchStats {
    date: Option<String>,
    home_team_id: i64,
    away_team_id: i64,
    home_goals: Option<i32>,
    away_goals: Option<i32>,
    penalty_home_goals: Option<i32>,
    penalty_away_goals: Option<i32>,
    home_shots: i32,
    away_shots: i32,
    home_shots_on_target: i32,
    away_shots_on_target: i32,
    home_saves: i32,
    away_saves: i32,
    home_possession: i32,
    revenue: i64,
}

fn load_match_stats_row(
    conn: &Connection,
    match_id: MatchId,
) -> NaratamaResult<Option<RawMatchStats>> {
    let row = conn
        .query_row(
            "SELECT m.date, m.home_team_id, m.away_team_id,
                    m.home_goals, m.away_goals, m.penalty_home_goals, m.penalty_away_goals,
                    COALESCE(m.home_shots, 0), COALESCE(m.away_shots, 0),
                    COALESCE(m.home_shots_on_target, 0), COALESCE(m.away_shots_on_target, 0),
                    COALESCE(m.home_saves, 0), COALESCE(m.away_saves, 0),
                    COALESCE(m.home_possession, 0), COALESCE(m.revenue, 0)
             FROM matches m WHERE m.match_id = ?1",
            params![match_id.0],
            |r| {
                Ok(RawMatchStats {
                    date: r.get(0)?,
                    home_team_id: r.get(1)?,
                    away_team_id: r.get(2)?,
                    home_goals: r.get(3)?,
                    away_goals: r.get(4)?,
                    penalty_home_goals: r.get(5)?,
                    penalty_away_goals: r.get(6)?,
                    home_shots: r.get(7)?,
                    away_shots: r.get(8)?,
                    home_shots_on_target: r.get(9)?,
                    away_shots_on_target: r.get(10)?,
                    home_saves: r.get(11)?,
                    away_saves: r.get(12)?,
                    home_possession: r.get(13)?,
                    revenue: r.get(14)?,
                })
            },
        )
        .optional()
        .map_err(db_err)?;
    Ok(row)
}

fn match_player_stats(
    conn: &Connection,
    match_id: MatchId,
) -> NaratamaResult<Vec<MatchPlayerStat>> {
    let mut stmt = conn
        .prepare(
            "SELECT mp.player_id, p.name, mp.team_id, COALESCE(p.position_id, 0), mp.starter,
                    mp.minutes, mp.goals, mp.assists, mp.yellow_cards, mp.red_cards, mp.mvp, mp.rating
             FROM match_players mp
             LEFT JOIN players p ON p.player_id = mp.player_id
             WHERE mp.match_id = ?1
             ORDER BY mp.team_id ASC, mp.starter DESC, mp.rating DESC",
        )
        .map_err(db_err)?;
    let rows = stmt
        .query_map(params![match_id.0], |r| {
            Ok(MatchPlayerStat {
                player_id: PlayerId(r.get(0)?),
                player_name: r
                    .get::<_, Option<String>>(1)?
                    .unwrap_or_else(|| "Unknown".to_string()),
                team_id: TeamId(r.get(2)?),
                position_id: r.get(3)?,
                starter: r.get::<_, Option<i32>>(4)?.unwrap_or(0) != 0,
                minutes: r.get::<_, Option<i32>>(5)?.unwrap_or(0),
                goals: r.get::<_, Option<i32>>(6)?.unwrap_or(0),
                assists: r.get::<_, Option<i32>>(7)?.unwrap_or(0),
                yellow_cards: r.get::<_, Option<i32>>(8)?.unwrap_or(0),
                red_cards: r.get::<_, Option<i32>>(9)?.unwrap_or(0),
                mvp: r.get::<_, Option<i32>>(10)?.unwrap_or(0) != 0,
                rating: r.get::<_, Option<i32>>(11)?.unwrap_or(0),
            })
        })
        .map_err(db_err)?;
    Ok(rows.filter_map(Result::ok).collect())
}

fn parse_date(raw: Option<String>) -> Option<chrono::NaiveDate> {
    raw.and_then(|s| chrono::NaiveDate::parse_from_str(&s[..s.len().min(10)], "%Y-%m-%d").ok())
}

fn load_player(conn: &Connection, player_id: PlayerId) -> NaratamaResult<Option<Player>> {
    let mut stmt = conn
        .prepare("SELECT p.* FROM players p WHERE p.player_id = ?1")
        .map_err(db_err)?;
    let player = stmt
        .query_row(
            params![player_id.0],
            naratama_db::club_repo::ClubRepo::player_from_row,
        )
        .optional()
        .map_err(db_err)?;
    Ok(player)
}

fn knockout_name(phase: i32, matches: usize) -> String {
    if matches == 1 {
        return "Final".to_string();
    }
    if matches == 2 {
        return "Semi-final".to_string();
    }
    match phase {
        1 => "Round of 32".to_string(),
        2 => "Round of 16".to_string(),
        3 => "Quarter-final".to_string(),
        _ => format!("Round {phase}"),
    }
}

const INTERNATIONAL_TOURNAMENTS: [(i64, &str); 9] = [
    (57, "FIFA World Cup"),
    (98, "AFC Asian Cup"),
    (95, "UEFA EURO"),
    (94, "Copa América"),
    (97, "Africa Cup of Nations"),
    (96, "CONCACAF Gold Cup"),
    (903, "FIFA ASEAN Cup"),
    (902, "ASEAN Championship"),
    (901, "UEFA Nations League"),
];

/// Maps a database organization code to its tournament kind.
#[must_use]
pub const fn tournament_org(organization: i32) -> TournamentOrg {
    match organization {
        2 | 3 => TournamentOrg::NationalCup,
        4 => TournamentOrg::International,
        _ => TournamentOrg::League,
    }
}
