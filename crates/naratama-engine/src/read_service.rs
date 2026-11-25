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

    /// Tournaments contested in a season (domestic leagues/cups + international club cups).
    pub fn season_tournaments(
        conn: &Connection,
        season_id: SeasonId,
    ) -> NaratamaResult<Vec<(i64, String, i32)>> {
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
        let standings = if groups.is_empty() {
            Self::standings_from_fixtures(&league_fixtures, &names)
        } else {
            Vec::new()
        };
        let bracket = Self::bracket_rounds(&fixtures);

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
        Ok(Some(TeamDetail {
            team,
            stadium,
            league_name,
            squad_size,
            average_power,
            squad_value,
            top_players: squad.into_iter().take(5).collect(),
            recent_results,
            next_fixtures,
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
