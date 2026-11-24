use std::collections::HashSet;

use chrono::NaiveDate;
use naratama_model::{
    CoachId, CompetitionConfig, CompetitionFormat, CompetitionType, CountryId, FixtureMatch,
    InternationalWindow, InternationalWindowType, MatchId, MatchStatus, NaratamaResult, Season,
    SeasonId, StadiumId, Team, TeamId, TournamentId,
};
use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::{db_err, parse_sqlite_date};

#[derive(Debug, Clone)]
pub struct NewFixtureRow {
    pub tournament_id: TournamentId,
    pub season_id: SeasonId,
    pub home_team_id: TeamId,
    pub away_team_id: TeamId,
    pub stadium_id: StadiumId,
    pub round: i32,
    pub phase: i32,
    pub group_id: Option<i32>,
    pub leg: i32,
    pub date: NaiveDate,
    pub neutral_venue: bool,
    pub extra_time: bool,
    pub away_goals_rule: bool,
}

#[derive(Debug, Clone)]
pub struct RescheduleUpdate {
    pub match_id: MatchId,
    pub old_date: Option<NaiveDate>,
    pub new_date: NaiveDate,
    pub original_date: NaiveDate,
    pub reason: String,
}

pub struct SeasonRepo<'a> {
    conn: &'a Connection,
}

impl<'a> SeasonRepo<'a> {
    #[must_use]
    pub const fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn insert_season(&self, start_year: i32) -> NaratamaResult<Season> {
        let name = format!("Season {start_year}/{:02}", (start_year + 1) % 100);
        let end_year = start_year + 1;
        self.conn
            .execute(
                "INSERT INTO seasons (name, start_year, end_year) VALUES (?1, ?2, ?3)",
                params![name, start_year, end_year],
            )
            .map_err(db_err)?;
        let id = SeasonId(self.conn.last_insert_rowid());
        Ok(Season {
            season_id: id,
            name,
            start_year,
            end_year,
        })
    }

    pub fn season_years(&self, season_id: SeasonId) -> NaratamaResult<Option<(i32, i32)>> {
        self.conn
            .query_row(
                "SELECT start_year, end_year FROM seasons WHERE season_id = ?1",
                params![season_id.get()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(db_err)
    }

    pub fn link_season_tournament(
        &self,
        season_id: SeasonId,
        tournament_id: TournamentId,
    ) -> NaratamaResult<()> {
        let linked = self.is_tournament_linked(season_id, tournament_id)?;
        if !linked {
            self.conn
                .execute(
                    "INSERT INTO season_tournaments (season_id, tournament_id, status) VALUES (?1, ?2, 1)",
                    params![season_id.get(), tournament_id.get()],
                )
                .map_err(db_err)?;
        }
        Ok(())
    }

    pub fn is_tournament_linked(
        &self,
        season_id: SeasonId,
        tournament_id: TournamentId,
    ) -> NaratamaResult<bool> {
        let count: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM season_tournaments WHERE season_id = ?1 AND tournament_id = ?2",
                params![season_id.get(), tournament_id.get()],
                |row| row.get(0),
            )
            .map_err(db_err)?;
        Ok(count > 0)
    }

    pub fn registered_season_team_ids(
        &self,
        season_id: SeasonId,
    ) -> NaratamaResult<HashSet<TeamId>> {
        let mut stmt = self
            .conn
            .prepare("SELECT team_id FROM season_teams WHERE season_id = ?1")
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![season_id.get()], |row| Ok(TeamId(row.get(0)?)))
            .map_err(db_err)?;
        rows.collect::<Result<HashSet<_>, _>>().map_err(db_err)
    }

    pub fn register_season_teams(
        &self,
        season_id: SeasonId,
        teams: &[&Team],
    ) -> NaratamaResult<()> {
        let existing = self.registered_season_team_ids(season_id)?;
        let tx = self.conn.unchecked_transaction().map_err(db_err)?;
        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO season_teams (team_id, season_id, coach_id) VALUES (?1, ?2, ?3)",
                )
                .map_err(db_err)?;
            for tm in teams {
                if !existing.contains(&tm.team_id) {
                    stmt.execute(params![
                        tm.team_id.get(),
                        season_id.get(),
                        tm.coach_id.get()
                    ])
                    .map_err(db_err)?;
                }
            }
        }
        tx.commit().map_err(db_err)?;
        Ok(())
    }

    pub fn persist_competition_config(&self, cfg: &CompetitionConfig) -> NaratamaResult<()> {
        let unavailable_json = serde_json::to_string(
            &cfg.unavailable_dates
                .iter()
                .map(|d| d.format("%Y-%m-%d").to_string())
                .collect::<Vec<_>>(),
        )?;
        let weekdays_json = serde_json::to_string(&cfg.allowed_weekdays)?;

        self.conn
            .execute(
                "INSERT OR REPLACE INTO competition_configs (
                    tournament_id, season_start_year, name, type, format,
                    participant_count, double_round_robin, matchday_interval_days, min_rest_days,
                    group_size, group_count, group_double_round_robin, group_qualifiers_per_group,
                    league_phase_opponents, league_phase_home_matches, league_phase_away_matches,
                    league_phase_qualifiers, knockout_legs, final_single_match, neutral_final,
                    extra_time, penalties, away_goals, use_byes, trim_to_power_of_two,
                    field_size, promotion_slots, relegation_slots, unavailable_dates, allowed_weekdays
                 ) VALUES (
                    ?1, ?2, ?3, ?4, ?5,
                    ?6, ?7, ?8, ?9,
                    ?10, ?11, ?12, ?13,
                    ?14, ?15, ?16,
                    ?17, ?18, ?19, ?20,
                    ?21, ?22, ?23, ?24, ?25,
                    ?26, ?27, ?28, ?29, ?30
                 )",
                params![
                    cfg.tournament_id.get(),
                    cfg.season_start_year.unwrap_or(0),
                    cfg.name,
                    cfg.competition_type.as_str(),
                    cfg.format.as_str(),
                    cfg.participant_count,
                    i32::from(cfg.double_round_robin),
                    cfg.matchday_interval_days,
                    cfg.min_rest_days,
                    cfg.group_size,
                    cfg.group_count,
                    i32::from(cfg.group_double_round_robin),
                    cfg.group_qualifiers_per_group,
                    cfg.league_phase_opponents,
                    cfg.league_phase_home_matches,
                    cfg.league_phase_away_matches,
                    cfg.league_phase_qualifiers,
                    cfg.knockout_legs,
                    i32::from(cfg.final_single_match),
                    i32::from(cfg.neutral_final),
                    i32::from(cfg.extra_time),
                    i32::from(cfg.penalties),
                    i32::from(cfg.away_goals),
                    i32::from(cfg.use_byes),
                    i32::from(cfg.trim_to_power_of_two),
                    cfg.field_size,
                    cfg.promotion_slots,
                    cfg.relegation_slots,
                    unavailable_json,
                    weekdays_json,
                ],
            )
            .map_err(db_err)?;
        Ok(())
    }

    pub fn load_competition_config(
        &self,
        tournament_id: TournamentId,
        season_start_year: Option<i32>,
    ) -> NaratamaResult<Option<CompetitionConfig>> {
        let sql = if season_start_year.is_some() {
            "SELECT tournament_id, season_start_year, name, type, format,
                    participant_count, double_round_robin, matchday_interval_days, min_rest_days,
                    group_size, group_count, group_double_round_robin, group_qualifiers_per_group,
                    league_phase_opponents, league_phase_home_matches, league_phase_away_matches,
                    league_phase_qualifiers, knockout_legs, final_single_match, neutral_final,
                    extra_time, penalties, away_goals, use_byes, trim_to_power_of_two,
                    field_size, promotion_slots, relegation_slots, unavailable_dates, allowed_weekdays
             FROM competition_configs
             WHERE tournament_id = ?1 AND season_start_year IN (?2, 0)
             ORDER BY CASE WHEN season_start_year = ?2 THEN 0 ELSE 1 END
             LIMIT 1"
        } else {
            "SELECT tournament_id, season_start_year, name, type, format,
                    participant_count, double_round_robin, matchday_interval_days, min_rest_days,
                    group_size, group_count, group_double_round_robin, group_qualifiers_per_group,
                    league_phase_opponents, league_phase_home_matches, league_phase_away_matches,
                    league_phase_qualifiers, knockout_legs, final_single_match, neutral_final,
                    extra_time, penalties, away_goals, use_byes, trim_to_power_of_two,
                    field_size, promotion_slots, relegation_slots, unavailable_dates, allowed_weekdays
             FROM competition_configs
             WHERE tournament_id = ?1 AND season_start_year = 0
             LIMIT 1"
        };

        let mut stmt = self.conn.prepare(sql).map_err(db_err)?;
        let res = if let Some(year) = season_start_year {
            stmt.query_row(params![tournament_id.get(), year], map_competition_config)
                .optional()
                .map_err(db_err)?
        } else {
            stmt.query_row(params![tournament_id.get()], map_competition_config)
                .optional()
                .map_err(db_err)?
        };
        Ok(res)
    }

    pub fn upsert_international_windows(
        &self,
        windows: &[InternationalWindow],
    ) -> NaratamaResult<()> {
        let tx = self.conn.unchecked_transaction().map_err(db_err)?;
        {
            let mut stmt = tx
                .prepare(
                    "INSERT OR REPLACE INTO international_windows (
                        window_id, season_id, name, window_type,
                        start_date, end_date, release_start_date, return_date,
                        is_mandatory, max_matches, club_fixtures_allowed
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                )
                .map_err(db_err)?;
            for w in windows {
                stmt.execute(params![
                    w.window_id,
                    w.season_id.get(),
                    w.name,
                    w.window_type.as_str(),
                    w.start_date.format("%Y-%m-%d").to_string(),
                    w.end_date.format("%Y-%m-%d").to_string(),
                    w.release_start_date.format("%Y-%m-%d").to_string(),
                    w.return_date.format("%Y-%m-%d").to_string(),
                    i32::from(w.is_mandatory_release),
                    w.max_matches,
                    i32::from(w.club_fixtures_allowed),
                ])
                .map_err(db_err)?;
            }
        }
        tx.commit().map_err(db_err)?;
        Ok(())
    }

    pub fn windows_for_season(
        &self,
        season_id: SeasonId,
    ) -> NaratamaResult<Vec<InternationalWindow>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT window_id, season_id, name, window_type,
                        start_date, end_date, release_start_date, return_date,
                        is_mandatory, max_matches, club_fixtures_allowed
                 FROM international_windows
                 WHERE season_id = ?1
                 ORDER BY start_date ASC",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![season_id.get()], |row| {
                let start = parse_sqlite_date(row.get(4)?).unwrap_or_default();
                let end = parse_sqlite_date(row.get(5)?).unwrap_or_default();
                let rel = parse_sqlite_date(row.get(6)?).unwrap_or_default();
                let ret = parse_sqlite_date(row.get(7)?).unwrap_or_default();
                let wtype: String = row.get(3)?;
                Ok(InternationalWindow {
                    window_id: row.get(0)?,
                    season_id: SeasonId(row.get(1)?),
                    name: row.get(2)?,
                    window_type: InternationalWindowType::from_db_str(&wtype),
                    start_date: start,
                    end_date: end,
                    release_start_date: rel,
                    return_date: ret,
                    is_mandatory_release: row.get::<_, i64>(8)? != 0,
                    max_matches: row.get(9)?,
                    club_fixtures_allowed: row.get::<_, i64>(10)? != 0,
                })
            })
            .map_err(db_err)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(db_err)
    }

    pub fn insert_matches(&self, rows: &[NewFixtureRow]) -> NaratamaResult<usize> {
        if rows.is_empty() {
            return Ok(0);
        }
        let tx = self.conn.unchecked_transaction().map_err(db_err)?;
        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO matches (
                        tournament_id, season_id, home_team_id, away_team_id, stadium_id,
                        round, phase, group_id, status, leg, date,
                        home_goals, away_goals, winner_id,
                        neutral_venue, extra_time, away_goals_rule
                     ) VALUES (
                        ?1, ?2, ?3, ?4, ?5,
                        ?6, ?7, ?8, 1, ?9, ?10,
                        NULL, NULL, NULL,
                        ?11, ?12, ?13
                     )",
                )
                .map_err(db_err)?;
            for r in rows {
                stmt.execute(params![
                    r.tournament_id.get(),
                    r.season_id.get(),
                    r.home_team_id.get(),
                    r.away_team_id.get(),
                    r.stadium_id.get(),
                    r.round,
                    r.phase,
                    r.group_id,
                    r.leg,
                    r.date.format("%Y-%m-%d").to_string(),
                    i32::from(r.neutral_venue),
                    i32::from(r.extra_time),
                    i32::from(r.away_goals_rule),
                ])
                .map_err(db_err)?;
            }
        }
        tx.commit().map_err(db_err)?;
        Ok(rows.len())
    }

    pub fn matches_for_season(&self, season_id: SeasonId) -> NaratamaResult<Vec<FixtureMatch>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT match_id, tournament_id, season_id, round, phase, group_id, status,
                        home_team_id, away_team_id, stadium_id, date,
                        home_goals, away_goals, winner_id, penalty_home_goals, penalty_away_goals,
                        leg, original_date, reschedule_reason,
                        COALESCE(neutral_venue, 0), COALESCE(extra_time, 0), COALESCE(away_goals_rule, 0)
                 FROM matches
                 WHERE season_id = ?1
                 ORDER BY date ASC, match_id ASC",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![season_id.get()], map_fixture_match)
            .map_err(db_err)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(db_err)
    }

    pub fn matches_on_date(
        &self,
        season_id: SeasonId,
        date: NaiveDate,
    ) -> NaratamaResult<Vec<FixtureMatch>> {
        let date_str = date.format("%Y-%m-%d").to_string();
        let mut stmt = self
            .conn
            .prepare(
                "SELECT match_id, tournament_id, season_id, round, phase, group_id, status,
                        home_team_id, away_team_id, stadium_id, date,
                        home_goals, away_goals, winner_id, penalty_home_goals, penalty_away_goals,
                        leg, original_date, reschedule_reason,
                        COALESCE(neutral_venue, 0), COALESCE(extra_time, 0), COALESCE(away_goals_rule, 0)
                 FROM matches
                 WHERE season_id = ?1 AND date = ?2
                 ORDER BY tournament_id ASC, match_id ASC",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![season_id.get(), date_str], map_fixture_match)
            .map_err(db_err)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(db_err)
    }

    pub fn matches_for_tournament(
        &self,
        season_id: SeasonId,
        tournament_id: TournamentId,
    ) -> NaratamaResult<Vec<FixtureMatch>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT match_id, tournament_id, season_id, round, phase, group_id, status,
                        home_team_id, away_team_id, stadium_id, date,
                        home_goals, away_goals, winner_id, penalty_home_goals, penalty_away_goals,
                        leg, original_date, reschedule_reason,
                        COALESCE(neutral_venue, 0), COALESCE(extra_time, 0), COALESCE(away_goals_rule, 0)
                 FROM matches
                 WHERE season_id = ?1 AND tournament_id = ?2
                 ORDER BY phase ASC, round ASC, leg ASC, match_id ASC",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(
                params![season_id.get(), tournament_id.get()],
                map_fixture_match,
            )
            .map_err(db_err)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(db_err)
    }

    pub fn match_by_id(&self, match_id: MatchId) -> NaratamaResult<Option<FixtureMatch>> {
        self.conn
            .query_row(
                "SELECT match_id, tournament_id, season_id, round, phase, group_id, status,
                        home_team_id, away_team_id, stadium_id, date,
                        home_goals, away_goals, winner_id, penalty_home_goals, penalty_away_goals,
                        leg, original_date, reschedule_reason,
                        COALESCE(neutral_venue, 0), COALESCE(extra_time, 0), COALESCE(away_goals_rule, 0)
                 FROM matches
                 WHERE match_id = ?1",
                params![match_id.get()],
                map_fixture_match,
            )
            .optional()
            .map_err(db_err)
    }

    pub fn apply_reschedules(
        &self,
        updates: &[RescheduleUpdate],
        unresolved_logs: &[RescheduleUpdate],
        created_at: NaiveDate,
    ) -> NaratamaResult<()> {
        if updates.is_empty() && unresolved_logs.is_empty() {
            return Ok(());
        }
        let created = created_at.format("%Y-%m-%d").to_string();
        let tx = self.conn.unchecked_transaction().map_err(db_err)?;
        {
            let mut upd = tx
                .prepare(
                    "UPDATE matches
                     SET date = ?1, original_date = ?2, reschedule_reason = ?3, status = 1
                     WHERE match_id = ?4",
                )
                .map_err(db_err)?;
            let mut ins = tx
                .prepare(
                    "INSERT INTO fixture_reschedules (match_id, old_date, new_date, reason, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                )
                .map_err(db_err)?;

            for u in updates {
                let new_iso = u.new_date.format("%Y-%m-%d").to_string();
                let orig_iso = u.original_date.format("%Y-%m-%d").to_string();
                let old_iso = u.old_date.map(|d| d.format("%Y-%m-%d").to_string());
                upd.execute(params![new_iso, orig_iso, u.reason, u.match_id.get()])
                    .map_err(db_err)?;
                ins.execute(params![
                    u.match_id.get(),
                    old_iso,
                    new_iso,
                    u.reason,
                    created
                ])
                .map_err(db_err)?;
            }
            for u in unresolved_logs {
                let new_iso = u.new_date.format("%Y-%m-%d").to_string();
                let old_iso = u.old_date.map(|d| d.format("%Y-%m-%d").to_string());
                ins.execute(params![
                    u.match_id.get(),
                    old_iso,
                    new_iso,
                    u.reason,
                    created
                ])
                .map_err(db_err)?;
            }
        }
        tx.commit().map_err(db_err)?;
        Ok(())
    }

    pub fn record_knockout_byes(
        &self,
        season_id: SeasonId,
        tournament_id: TournamentId,
        phase: i32,
        byes: &[TeamId],
    ) -> NaratamaResult<()> {
        if byes.is_empty() {
            return Ok(());
        }
        let tx = self.conn.unchecked_transaction().map_err(db_err)?;
        {
            let mut stmt = tx
                .prepare(
                    "INSERT OR REPLACE INTO knockout_byes (season_id, tournament_id, phase, team_id)
                     VALUES (?1, ?2, ?3, ?4)",
                )
                .map_err(db_err)?;
            for &tid in byes {
                stmt.execute(params![
                    season_id.get(),
                    tournament_id.get(),
                    phase,
                    tid.get()
                ])
                .map_err(db_err)?;
            }
        }
        tx.commit().map_err(db_err)?;
        Ok(())
    }

    pub fn byes_for_phase(
        &self,
        season_id: SeasonId,
        tournament_id: TournamentId,
        phase: i32,
    ) -> NaratamaResult<Vec<TeamId>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT team_id FROM knockout_byes
                 WHERE season_id = ?1 AND tournament_id = ?2 AND phase = ?3",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(
                params![season_id.get(), tournament_id.get(), phase],
                |row| Ok(TeamId(row.get(0)?)),
            )
            .map_err(db_err)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(db_err)
    }

    pub fn international_slots(
        &self,
        tournament_id: TournamentId,
    ) -> NaratamaResult<Vec<(CountryId, i32)>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT country_id, COALESCE(slots, 0)
                 FROM international_slots
                 WHERE tournament_id = ?1 AND COALESCE(slots, 0) > 0
                 ORDER BY country_id ASC",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![tournament_id.get()], |row| {
                Ok((CountryId(row.get(0)?), row.get(1)?))
            })
            .map_err(db_err)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(db_err)
    }

    pub fn division1_teams_by_power(&self, country_id: CountryId) -> NaratamaResult<Vec<Team>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT team_id, name, COALESCE(abbreviation, ''), COALESCE(country_id, 0),
                        COALESCE(power, 50), COALESCE(value, 0), COALESCE(budget, 500000),
                        COALESCE(division, 0), COALESCE(stadium_id, 0), COALESCE(coach_id, 0),
                        COALESCE(founded_year, 1900), logo_base64
                 FROM teams
                 WHERE country_id = ?1 AND division = 1
                 ORDER BY power DESC, team_id ASC",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![country_id.get()], map_team)
            .map_err(db_err)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(db_err)
    }

    pub fn latest_div1_league_standings_order(
        &self,
        country_id: CountryId,
        before_season_id: SeasonId,
    ) -> NaratamaResult<Vec<TeamId>> {
        let prev: Option<(i64, i64)> = self
            .conn
            .query_row(
                "SELECT st.season_id, tu.tournament_id
                 FROM season_tournaments st
                 JOIN tournaments tu ON tu.tournament_id = st.tournament_id
                 JOIN tournament_countries tc ON tc.tournament_id = tu.tournament_id
                 WHERE tu.organization = 1 AND tu.division_id = 1
                   AND tc.country_id = ?1 AND st.season_id < ?2
                 ORDER BY st.season_id DESC
                 LIMIT 1",
                params![country_id.get(), before_season_id.get()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(db_err)?;

        let Some((sid, tid)) = prev else {
            return Ok(Vec::new());
        };
        let mut stmt = self
            .conn
            .prepare(
                "SELECT team_id, SUM(pts) AS points,
                        SUM(gf) - SUM(ga) AS gd,
                        SUM(gf) AS goals_for
                 FROM (
                     SELECT home_team_id AS team_id,
                            CASE WHEN home_goals > away_goals THEN 3 WHEN home_goals = away_goals THEN 1 ELSE 0 END AS pts,
                            home_goals AS gf, away_goals AS ga
                     FROM matches WHERE season_id = ?1 AND tournament_id = ?2 AND status = 2
                     UNION ALL
                     SELECT away_team_id AS team_id,
                            CASE WHEN away_goals > home_goals THEN 3 WHEN away_goals = home_goals THEN 1 ELSE 0 END AS pts,
                            away_goals AS gf, home_goals AS ga
                     FROM matches WHERE season_id = ?1 AND tournament_id = ?2 AND status = 2
                 )
                 GROUP BY team_id
                 ORDER BY points DESC, gd DESC, goals_for DESC",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![sid, tid], |row| Ok(TeamId(row.get(0)?)))
            .map_err(db_err)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(db_err)
    }

    pub fn last_final_teams(
        &self,
        tournament_id: TournamentId,
        before_season_id: SeasonId,
    ) -> NaratamaResult<Option<(TeamId, TeamId)>> {
        let row: Option<(i64, i64, i64)> = self
            .conn
            .query_row(
                "SELECT final.home_team_id, final.away_team_id, final.winner_id
                 FROM matches final
                 WHERE final.tournament_id = ?1 AND final.season_id < ?2
                   AND final.phase = (
                       SELECT MAX(m.phase) FROM matches m
                       WHERE m.season_id = final.season_id AND m.tournament_id = final.tournament_id
                   )
                   AND final.winner_id IS NOT NULL AND final.winner_id != 0
                   AND NOT EXISTS (
                       SELECT 1 FROM matches pending
                       WHERE pending.season_id = final.season_id
                         AND pending.tournament_id = final.tournament_id
                         AND pending.status != 2
                   )
                 ORDER BY final.season_id DESC, final.leg DESC, final.match_id DESC
                 LIMIT 1",
                params![tournament_id.get(), before_season_id.get()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .map_err(db_err)?;
        Ok(row.map(|(home, away, winner)| {
            let runner_up = if winner == home { away } else { home };
            (TeamId(winner), TeamId(runner_up))
        }))
    }

    pub fn repair_self_matches(&self, season_id: SeasonId) -> NaratamaResult<usize> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT match_id, tournament_id, COALESCE(phase, 1), home_team_id
                 FROM matches
                 WHERE season_id = ?1 AND home_team_id = away_team_id",
            )
            .map_err(db_err)?;
        let self_matches: Vec<(i64, i64, i64, i64)> = stmt
            .query_map(params![season_id.get()], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;

        let mut repaired = 0usize;
        for (match_id, tournament_id, phase, team_id) in self_matches {
            let mut busy_stmt = self
                .conn
                .prepare(
                    "SELECT home_team_id FROM matches WHERE season_id = ?1 AND tournament_id = ?2 AND phase = ?3 AND match_id != ?4
                     UNION
                     SELECT away_team_id FROM matches WHERE season_id = ?1 AND tournament_id = ?2 AND phase = ?3 AND match_id != ?4",
                )
                .map_err(db_err)?;
            let mut busy: HashSet<i64> = busy_stmt
                .query_map(
                    params![season_id.get(), tournament_id, phase, match_id],
                    |r| r.get(0),
                )
                .map_err(db_err)?
                .collect::<Result<HashSet<_>, _>>()
                .map_err(db_err)?;
            busy.insert(team_id);

            let mut cand_stmt = self
                .conn
                .prepare(
                    "SELECT tm.team_id
                     FROM season_teams st
                     JOIN teams tm ON tm.team_id = st.team_id
                     WHERE st.season_id = ?1
                     ORDER BY tm.power DESC",
                )
                .map_err(db_err)?;
            let candidate = cand_stmt
                .query_map(params![season_id.get()], |r| r.get::<_, i64>(0))
                .map_err(db_err)?
                .filter_map(Result::ok)
                .find(|id| !busy.contains(id));

            if let Some(opp_id) = candidate {
                self.conn
                    .execute(
                        "UPDATE matches SET away_team_id = ?1 WHERE match_id = ?2",
                        params![opp_id, match_id],
                    )
                    .map_err(db_err)?;
            } else {
                self.conn
                    .execute("DELETE FROM matches WHERE match_id = ?1", params![match_id])
                    .map_err(db_err)?;
            }
            repaired += 1;
        }
        Ok(repaired)
    }

    pub fn remaining_matches(
        &self,
        season_id: SeasonId,
        tournament_id: TournamentId,
    ) -> NaratamaResult<i64> {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM matches WHERE season_id = ?1 AND tournament_id = ?2 AND status = 1",
                params![season_id.get(), tournament_id.get()],
                |row| row.get(0),
            )
            .map_err(db_err)
    }

    pub fn max_phase_and_knockout_count(
        &self,
        season_id: SeasonId,
        tournament_id: TournamentId,
    ) -> NaratamaResult<(Option<i32>, i64)> {
        let max_phase: Option<i32> = self
            .conn
            .query_row(
                "SELECT MAX(phase) FROM matches WHERE season_id = ?1 AND tournament_id = ?2",
                params![season_id.get(), tournament_id.get()],
                |row| row.get(0),
            )
            .map_err(db_err)?;
        let ko_count: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM matches
                 WHERE season_id = ?1 AND tournament_id = ?2 AND group_id IS NULL AND phase > 0",
                params![season_id.get(), tournament_id.get()],
                |row| row.get(0),
            )
            .map_err(db_err)?;
        Ok((max_phase, ko_count))
    }

    pub fn latest_phase_winners(
        &self,
        season_id: SeasonId,
        tournament_id: TournamentId,
    ) -> NaratamaResult<Vec<TeamId>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT winner_id FROM matches m
                 WHERE m.season_id = ?1 AND m.tournament_id = ?2
                   AND m.phase = (SELECT MAX(phase) FROM matches WHERE season_id = ?1 AND tournament_id = ?2)
                   AND (m.leg = 2 OR m.leg = (
                       SELECT MAX(m2.leg) FROM matches m2
                       WHERE m2.season_id = m.season_id AND m2.tournament_id = m.tournament_id AND m2.phase = m.phase
                   ))
                   AND m.status = 2 AND m.winner_id IS NOT NULL AND m.winner_id != 0
                 ORDER BY m.match_id ASC",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![season_id.get(), tournament_id.get()], |row| {
                Ok(TeamId(row.get(0)?))
            })
            .map_err(db_err)?;
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        for row in rows {
            let wid = row.map_err(db_err)?;
            if seen.insert(wid) {
                out.push(wid);
            }
        }
        Ok(out)
    }

    pub fn last_tournament_date(
        &self,
        season_id: SeasonId,
        tournament_id: TournamentId,
    ) -> NaratamaResult<Option<NaiveDate>> {
        let raw: Option<String> = self
            .conn
            .query_row(
                "SELECT MAX(date) FROM matches WHERE season_id = ?1 AND tournament_id = ?2",
                params![season_id.get(), tournament_id.get()],
                |row| row.get(0),
            )
            .map_err(db_err)?;
        Ok(parse_sqlite_date(raw))
    }

    pub fn earliest_scheduled_date(
        &self,
        season_id: SeasonId,
    ) -> NaratamaResult<Option<NaiveDate>> {
        let raw: Option<String> = self
            .conn
            .query_row(
                "SELECT MIN(date) FROM matches WHERE season_id = ?1 AND status = 1",
                params![season_id.get()],
                |row| row.get(0),
            )
            .map_err(db_err)?;
        Ok(parse_sqlite_date(raw))
    }

    pub fn continental_playing_team_ids(
        &self,
        season_id: SeasonId,
    ) -> NaratamaResult<HashSet<TeamId>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT DISTINCT home_team_id FROM matches
                 WHERE season_id = ?1 AND tournament_id IN (49, 51, 135, 99, 164, 1650, 52, 53, 50, 100)
                 UNION
                 SELECT DISTINCT away_team_id FROM matches
                 WHERE season_id = ?1 AND tournament_id IN (49, 51, 135, 99, 164, 1650, 52, 53, 50, 100)",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![season_id.get()], |r| Ok(TeamId(r.get(0)?)))
            .map_err(db_err)?;
        rows.collect::<Result<HashSet<_>, _>>().map_err(db_err)
    }
}

fn map_team(row: &Row<'_>) -> rusqlite::Result<Team> {
    Ok(Team {
        team_id: TeamId(row.get(0)?),
        name: row.get(1)?,
        abbreviation: row.get(2)?,
        country_id: CountryId(row.get(3)?),
        power: row.get(4)?,
        value: row.get(5)?,
        budget: row.get(6)?,
        division: row.get(7)?,
        stadium_id: StadiumId(row.get(8)?),
        coach_id: CoachId(row.get(9)?),
        founded_year: row.get(10)?,
        logo_base64: row.get(11)?,
    })
}

fn map_fixture_match(row: &Row<'_>) -> rusqlite::Result<FixtureMatch> {
    Ok(FixtureMatch {
        match_id: MatchId(row.get(0)?),
        tournament_id: TournamentId(row.get(1)?),
        season_id: SeasonId(row.get(2)?),
        round: row.get(3)?,
        phase: row.get(4)?,
        group_id: row.get(5)?,
        status: MatchStatus::from_code(row.get(6)?),
        home_team_id: TeamId(row.get(7)?),
        away_team_id: TeamId(row.get(8)?),
        stadium_id: StadiumId(row.get(9)?),
        date: parse_sqlite_date(row.get(10)?),
        home_goals: row.get(11)?,
        away_goals: row.get(12)?,
        winner_id: row
            .get::<_, Option<i64>>(13)?
            .filter(|&v| v > 0)
            .map(TeamId),
        penalty_home_goals: row.get(14)?,
        penalty_away_goals: row.get(15)?,
        leg: row.get::<_, Option<i32>>(16)?.unwrap_or(1),
        original_date: parse_sqlite_date(row.get(17)?),
        reschedule_reason: row.get(18)?,
        neutral_venue: row.get::<_, i64>(19)? != 0,
        extra_time: row.get::<_, i64>(20)? != 0,
        away_goals_rule: row.get::<_, i64>(21)? != 0,
    })
}

fn map_competition_config(row: &Row<'_>) -> rusqlite::Result<CompetitionConfig> {
    let start_year_raw: i32 = row.get(1)?;
    let comp_type_raw: String = row.get(3)?;
    let format_raw: String = row.get(4)?;
    let unavailable_raw: String = row.get(28)?;
    let weekdays_raw: String = row.get(29)?;

    let unavailable_dates: Vec<NaiveDate> = serde_json::from_str::<Vec<String>>(&unavailable_raw)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|s| parse_sqlite_date(Some(s)))
        .collect();
    let allowed_weekdays: Vec<u32> = serde_json::from_str(&weekdays_raw).unwrap_or_default();

    Ok(CompetitionConfig {
        tournament_id: TournamentId(row.get(0)?),
        name: row.get(2)?,
        competition_type: CompetitionType::from_db_str(&comp_type_raw),
        format: CompetitionFormat::from_db_str(&format_raw),
        season_start_year: (start_year_raw != 0).then_some(start_year_raw),
        participant_count: row.get(5)?,
        double_round_robin: row.get::<_, i64>(6)? != 0,
        matchday_interval_days: row.get(7)?,
        min_rest_days: row.get(8)?,
        group_size: row.get(9)?,
        group_count: row.get(10)?,
        group_double_round_robin: row.get::<_, i64>(11)? != 0,
        group_qualifiers_per_group: row.get(12)?,
        league_phase_opponents: row.get(13)?,
        league_phase_home_matches: row.get(14)?,
        league_phase_away_matches: row.get(15)?,
        league_phase_qualifiers: row.get(16)?,
        knockout_legs: row.get(17)?,
        final_single_match: row.get::<_, i64>(18)? != 0,
        neutral_final: row.get::<_, i64>(19)? != 0,
        extra_time: row.get::<_, i64>(20)? != 0,
        penalties: row.get::<_, i64>(21)? != 0,
        away_goals: row.get::<_, i64>(22)? != 0,
        use_byes: row.get::<_, i64>(23)? != 0,
        trim_to_power_of_two: row.get::<_, i64>(24)? != 0,
        field_size: row.get(25)?,
        promotion_slots: row.get(26)?,
        relegation_slots: row.get(27)?,
        unavailable_dates,
        allowed_weekdays,
    })
}
