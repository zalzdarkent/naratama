use std::collections::HashSet;

use chrono::NaiveDate;
use naratama_model::{
    Confederation, Country, CountryId, InternationalWindow, MatchId, MatchStatus, Money,
    NaratamaResult, Player, PlayerId, SeasonId, StadiumId, TournamentId,
};
use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};
use serde::{Deserialize, Serialize};

use crate::club_repo::ClubRepo;
use crate::{db_err, parse_sqlite_date};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CountryMatchRow {
    pub match_id: MatchId,
    pub tournament_id: TournamentId,
    pub season_id: SeasonId,
    pub referee_id: Option<i64>,
    pub stadium_id: StadiumId,
    pub home_country_id: CountryId,
    pub away_country_id: CountryId,
    pub home_goals: Option<i32>,
    pub away_goals: Option<i32>,
    pub winner_id: Option<CountryId>,
    pub phase: i32,
    pub round: i32,
    pub status: MatchStatus,
    pub leg: i32,
    pub date: Option<NaiveDate>,
    pub penalty_home_goals: Option<i32>,
    pub penalty_away_goals: Option<i32>,
    pub group_id: Option<i32>,
    pub window_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewCountryMatchRow {
    pub tournament_id: TournamentId,
    pub season_id: SeasonId,
    pub stadium_id: StadiumId,
    pub home_country_id: CountryId,
    pub away_country_id: CountryId,
    pub phase: i32,
    pub round: i32,
    pub leg: i32,
    pub date: NaiveDate,
    pub group_id: Option<i32>,
    pub window_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct InternationalDutyRow {
    pub duty_id: i64,
    pub season_id: SeasonId,
    pub country_id: CountryId,
    pub player_id: PlayerId,
    pub window_id: String,
    pub release_date: NaiveDate,
    pub return_date: NaiveDate,
    pub squad_number: i32,
    pub is_captain: bool,
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct CountryMatchPlayerInsert {
    pub player_id: PlayerId,
    pub country_id: CountryId,
    pub starter: bool,
    pub minutes: i32,
    pub points: i32,
    pub rating: i32,
    pub mvp: bool,
    pub goals: i32,
    pub yellow_cards: i32,
    pub red_cards: i32,
    pub assists: i32,
    pub fatigue_drop: i32,
}

#[derive(Debug, Clone)]
pub struct CountryMatchPersistPayload {
    pub match_id: MatchId,
    pub season_id: SeasonId,
    pub tournament_id: TournamentId,
    pub home_country_id: CountryId,
    pub away_country_id: CountryId,
    pub home_goals: i32,
    pub away_goals: i32,
    pub winner_id: Option<CountryId>,
    pub penalty_home_goals: Option<i32>,
    pub penalty_away_goals: Option<i32>,
    pub attendance_total: i32,
    pub revenue: Money,
    pub is_competitive: bool,
    pub player_rows: Vec<CountryMatchPlayerInsert>,
    pub injury_return_dates: Vec<(PlayerId, NaiveDate)>,
    pub new_home_points: f64,
    pub new_away_points: f64,
}

/// Repository for national teams, eligibility/cap-ties, duties/call-ups, international matches, and FIFA rankings.
pub struct IntlRepo;

impl IntlRepo {
    pub fn fifa_rankings(
        conn: &Connection,
        confederation: Option<Confederation>,
    ) -> NaratamaResult<Vec<Country>> {
        let mut out = Vec::new();
        if let Some(conf) = confederation {
            let mut stmt = conn
                .prepare(
                    "SELECT country_id, name, COALESCE(iso2, ''), COALESCE(power, 50),
                            COALESCE(confederation, 0), COALESCE(points, 0.0), previous_rank
                     FROM countries
                     WHERE confederation = ?1
                     ORDER BY COALESCE(points, 0.0) DESC, power DESC, name ASC",
                )
                .map_err(db_err)?;
            let rows = stmt
                .query_map(params![conf as i64], map_country_row)
                .map_err(db_err)?;
            for r in rows {
                out.push(r.map_err(db_err)?);
            }
        } else {
            let mut stmt = conn
                .prepare(
                    "SELECT country_id, name, COALESCE(iso2, ''), COALESCE(power, 50),
                            COALESCE(confederation, 0), COALESCE(points, 0.0), previous_rank
                     FROM countries
                     ORDER BY COALESCE(points, 0.0) DESC, power DESC, name ASC",
                )
                .map_err(db_err)?;
            let rows = stmt.query_map([], map_country_row).map_err(db_err)?;
            for r in rows {
                out.push(r.map_err(db_err)?);
            }
        }
        Ok(out)
    }

    pub fn country_by_id(conn: &Connection, cid: CountryId) -> NaratamaResult<Option<Country>> {
        conn.query_row(
            "SELECT country_id, name, COALESCE(iso2, ''), COALESCE(power, 50),
                    COALESCE(confederation, 0), COALESCE(points, 0.0), previous_rank
             FROM countries WHERE country_id = ?1",
            params![cid.0],
            map_country_row,
        )
        .optional()
        .map_err(db_err)
    }

    /// Snapshots current FIFA World Rankings ordering into `countries.previous_rank`.
    pub fn snapshot_world_rankings(conn: &mut Connection) -> NaratamaResult<usize> {
        let mut stmt = conn
            .prepare("SELECT country_id FROM countries ORDER BY COALESCE(points, 0.0) DESC, power DESC, name ASC")
            .map_err(db_err)?;
        let ids: Vec<i64> = stmt
            .query_map([], |r| r.get(0))
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        drop(stmt);

        let tx = conn.transaction().map_err(db_err)?;
        {
            let mut upd = tx
                .prepare("UPDATE countries SET previous_rank = ?1 WHERE country_id = ?2")
                .map_err(db_err)?;
            for (idx, cid) in ids.iter().enumerate() {
                let rank = i32::try_from(idx + 1).unwrap_or(999);
                upd.execute(params![rank, cid]).map_err(db_err)?;
            }
        }
        tx.commit().map_err(db_err)?;
        Ok(ids.len())
    }

    /// Recomputes every nation's `countries.power = ROUND(AVG(top 11 active primary players.power))`.
    pub fn refresh_all_country_powers(conn: &Connection) -> NaratamaResult<usize> {
        conn.execute(
            "UPDATE countries
             SET power = COALESCE((
                 SELECT CAST(ROUND(AVG(sub.power)) AS INTEGER)
                 FROM (
                     SELECT p.power FROM players p
                     WHERE p.country_id = countries.country_id AND COALESCE(p.status_id, 1) = 1
                     ORDER BY p.power DESC
                     LIMIT 11
                 ) sub
             ), power)
             WHERE EXISTS (
                 SELECT 1 FROM players pl
                 WHERE pl.country_id = countries.country_id AND COALESCE(pl.status_id, 1) = 1
             )",
            [],
        )
        .map_err(db_err)
    }

    /// Resolves all national teams `player_id` may represent (cap-tied locks + lazy competitive lock + dual nationality).
    /// Club residency NEVER grants national eligibility.
    pub fn eligible_country_ids_for_player(
        conn: &Connection,
        player_id: PlayerId,
    ) -> NaratamaResult<Vec<CountryId>> {
        // 1. Existing locked row
        let locked: Option<i64> = conn
            .query_row(
                "SELECT country_id FROM player_national_eligibility
                 WHERE player_id = ?1 AND COALESCE(is_locked, 0) = 1
                 LIMIT 1",
                params![player_id.0],
                |r| r.get(0),
            )
            .optional()
            .map_err(db_err)?;
        if let Some(cid) = locked {
            return Ok(vec![CountryId(cid)]);
        }

        // 2. Lazy cap-tie from prior competitive international appearance
        let comp_cid: Option<i64> = conn
            .query_row(
                "SELECT mpc.country_id
                 FROM match_players_countries mpc
                 JOIN country_matches cm ON cm.match_id = mpc.match_id
                 WHERE mpc.player_id = ?1 AND cm.tournament_id != 900 AND COALESCE(mpc.minutes, 0) > 0
                 LIMIT 1",
                params![player_id.0],
                |r| r.get(0),
            )
            .optional()
            .map_err(db_err)?;
        if let Some(cid) = comp_cid {
            let tx = conn.unchecked_transaction().map_err(db_err)?;
            Self::lock_allegiance_tx(&tx, player_id, CountryId(cid))?;
            tx.commit().map_err(db_err)?;
            return Ok(vec![CountryId(cid)]);
        }

        // 3. Primary nationality + unlocked secondary eligibilities
        let mut ids = Vec::new();
        if let Some(primary) = conn
            .query_row(
                "SELECT country_id FROM players WHERE player_id = ?1",
                params![player_id.0],
                |r| r.get::<_, Option<i64>>(0),
            )
            .optional()
            .map_err(db_err)?
            .flatten()
        {
            ids.push(CountryId(primary));
        }

        let mut stmt = conn
            .prepare("SELECT country_id FROM player_national_eligibility WHERE player_id = ?1")
            .map_err(db_err)?;
        let extras: Vec<CountryId> = stmt
            .query_map(params![player_id.0], |r| Ok(CountryId(r.get(0)?)))
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        for c in extras {
            if !ids.contains(&c) {
                ids.push(c);
            }
        }
        Ok(ids)
    }

    pub fn lock_allegiance_tx(
        tx: &Transaction<'_>,
        player_id: PlayerId,
        country_id: CountryId,
    ) -> NaratamaResult<()> {
        tx.execute(
            "DELETE FROM player_national_eligibility WHERE player_id = ?1 AND country_id != ?2",
            params![player_id.0, country_id.0],
        )
        .map_err(db_err)?;
        tx.execute(
            "INSERT OR REPLACE INTO player_national_eligibility (
                player_id, country_id, is_primary, is_locked, notes
             ) VALUES (?1, ?2, 1, 1, 'Cap-tied in official competition')",
            params![player_id.0, country_id.0],
        )
        .map_err(db_err)?;
        Ok(())
    }

    pub fn eligible_candidates_for_country(
        conn: &Connection,
        country_id: CountryId,
        limit: usize,
    ) -> NaratamaResult<Vec<Player>> {
        let lim = i64::try_from(limit).unwrap_or(120);
        let mut stmt = conn
            .prepare(
                "SELECT DISTINCT p.*
                 FROM players p
                 LEFT JOIN player_national_eligibility pne ON pne.player_id = p.player_id
                 WHERE COALESCE(p.status_id, 1) = 1
                   AND (
                       (pne.is_locked = 1 AND pne.country_id = ?1)
                       OR (
                           NOT EXISTS (
                               SELECT 1 FROM player_national_eligibility lck
                               WHERE lck.player_id = p.player_id AND COALESCE(lck.is_locked, 0) = 1
                           )
                           AND (p.country_id = ?1 OR pne.country_id = ?1)
                       )
                   )
                 ORDER BY p.power DESC, p.fitness DESC, p.player_id ASC
                 LIMIT ?2",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![country_id.0, lim], ClubRepo::player_from_row)
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        Ok(rows)
    }

    pub fn can_callup_player(
        conn: &Connection,
        player_id: PlayerId,
        release_start: NaiveDate,
        return_date: NaiveDate,
    ) -> NaratamaResult<bool> {
        let start_str = release_start.format("%Y-%m-%d").to_string();
        let end_str = return_date.format("%Y-%m-%d").to_string();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM international_duty
                 WHERE player_id = ?1 AND status IN ('active', 'pending_release')
                   AND NOT (return_date < ?2 OR release_date > ?3)",
                params![player_id.0, start_str, end_str],
                |r| r.get(0),
            )
            .map_err(db_err)?;
        Ok(count == 0)
    }

    pub fn active_duty_player_ids_on_date(
        conn: &Connection,
        date: NaiveDate,
    ) -> NaratamaResult<HashSet<PlayerId>> {
        let date_str = date.format("%Y-%m-%d").to_string();
        let mut stmt = conn
            .prepare(
                "SELECT DISTINCT player_id FROM international_duty
                 WHERE status = 'active' AND release_date <= ?1 AND return_date >= ?1",
            )
            .map_err(db_err)?;
        let ids = stmt
            .query_map(params![date_str], |r| Ok(PlayerId(r.get(0)?)))
            .map_err(db_err)?
            .collect::<Result<HashSet<_>, _>>()
            .map_err(db_err)?;
        Ok(ids)
    }

    pub fn duties_for_window(
        conn: &Connection,
        season_id: SeasonId,
        country_id: CountryId,
        window_id: &str,
    ) -> NaratamaResult<Vec<InternationalDutyRow>> {
        let mut stmt = conn
            .prepare(
                "SELECT duty_id, season_id, country_id, player_id, window_id,
                        release_date, return_date, COALESCE(squad_number, 0),
                        COALESCE(is_captain, 0), COALESCE(status, 'active')
                 FROM international_duty
                 WHERE season_id = ?1 AND country_id = ?2 AND window_id = ?3
                 ORDER BY squad_number ASC, duty_id ASC",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![season_id.0, country_id.0, window_id], |r| {
                Ok(InternationalDutyRow {
                    duty_id: r.get(0)?,
                    season_id: SeasonId(r.get(1)?),
                    country_id: CountryId(r.get(2)?),
                    player_id: PlayerId(r.get(3)?),
                    window_id: r.get(4)?,
                    release_date: parse_sqlite_date(r.get(5)?).unwrap_or(NaiveDate::MIN),
                    return_date: parse_sqlite_date(r.get(6)?).unwrap_or(NaiveDate::MAX),
                    squad_number: r.get(7)?,
                    is_captain: r.get::<_, i32>(8)? != 0,
                    status: r.get(9)?,
                })
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        Ok(rows)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn save_callups_and_duties(
        conn: &mut Connection,
        season_id: SeasonId,
        country_id: CountryId,
        window: &InternationalWindow,
        squad: &[Player],
        pending_user_players: &HashSet<PlayerId>,
        captain_id: PlayerId,
        numbers: &[(PlayerId, i32)],
    ) -> NaratamaResult<()> {
        let start_str = window.release_start_date.format("%Y-%m-%d").to_string();
        let end_str = window.return_date.format("%Y-%m-%d").to_string();
        let tx = conn.transaction().map_err(db_err)?;

        tx.execute(
            "DELETE FROM international_duty WHERE season_id = ?1 AND country_id = ?2 AND window_id = ?3",
            params![season_id.0, country_id.0, window.window_id],
        )
        .map_err(db_err)?;
        tx.execute(
            "DELETE FROM country_callups WHERE season_id = ?1 AND country_id = ?2",
            params![season_id.0, country_id.0],
        )
        .map_err(db_err)?;

        for p in squad {
            let is_pending = pending_user_players.contains(&p.player_id);
            let status = if is_pending {
                "pending_release"
            } else {
                "active"
            };
            let num = numbers
                .iter()
                .find(|(id, _)| *id == p.player_id)
                .map_or(12, |(_, n)| *n);
            tx.execute(
                "INSERT INTO international_duty (
                    season_id, country_id, player_id, window_id,
                    release_date, return_date, squad_number, is_captain, status
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    season_id.0,
                    country_id.0,
                    p.player_id.0,
                    window.window_id,
                    start_str,
                    end_str,
                    num,
                    i32::from(p.player_id == captain_id),
                    status,
                ],
            )
            .map_err(db_err)?;

            if !is_pending {
                tx.execute(
                    "INSERT OR IGNORE INTO country_callups (country_id, player_id, season_id)
                     VALUES (?1, ?2, ?3)",
                    params![country_id.0, p.player_id.0, season_id.0],
                )
                .map_err(db_err)?;
            }
        }
        tx.commit().map_err(db_err)?;
        Ok(())
    }

    pub fn release_concluded_duties(conn: &Connection, date: NaiveDate) -> NaratamaResult<usize> {
        let date_str = date.format("%Y-%m-%d").to_string();
        conn.execute(
            "UPDATE international_duty SET status = 'returned'
             WHERE status IN ('active', 'pending_release') AND return_date < ?1",
            params![date_str],
        )
        .map_err(db_err)
    }

    pub fn insert_country_matches(
        conn: &Connection,
        matches: &[NewCountryMatchRow],
    ) -> NaratamaResult<usize> {
        if matches.is_empty() {
            return Ok(0);
        }
        let tx = conn.unchecked_transaction().map_err(db_err)?;
        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO country_matches (
                        tournament_id, season_id, stadium_id, home_country_id, away_country_id,
                        phase, round, status, leg, date, group_id, window_id
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                )
                .map_err(db_err)?;
            for m in matches {
                let d = m.date.format("%Y-%m-%d").to_string();
                stmt.execute(params![
                    m.tournament_id.0,
                    m.season_id.0,
                    m.stadium_id.0,
                    m.home_country_id.0,
                    m.away_country_id.0,
                    m.phase,
                    m.round,
                    MatchStatus::Scheduled as i64,
                    m.leg,
                    d,
                    m.group_id,
                    m.window_id,
                ])
                .map_err(db_err)?;
            }
        }
        tx.commit().map_err(db_err)?;
        Ok(matches.len())
    }

    pub fn country_matches_for_tournament(
        conn: &Connection,
        season_id: SeasonId,
        tournament_id: TournamentId,
    ) -> NaratamaResult<Vec<CountryMatchRow>> {
        let mut stmt = conn
            .prepare(
                "SELECT match_id, tournament_id, season_id, referee_id, stadium_id,
                        home_country_id, away_country_id, home_goals, away_goals, winner_id,
                        phase, round, status, leg, date, penalty_home_goals, penalty_away_goals,
                        group_id, window_id
                 FROM country_matches
                 WHERE season_id = ?1 AND tournament_id = ?2
                 ORDER BY phase ASC, round ASC, date ASC, match_id ASC",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![season_id.0, tournament_id.0], map_country_match_row)
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        Ok(rows)
    }

    pub fn pending_country_matches_through_date(
        conn: &Connection,
        season_id: SeasonId,
        date: NaiveDate,
    ) -> NaratamaResult<Vec<CountryMatchRow>> {
        let date_str = date.format("%Y-%m-%d").to_string();
        let mut stmt = conn
            .prepare(
                "SELECT match_id, tournament_id, season_id, referee_id, stadium_id,
                        home_country_id, away_country_id, home_goals, away_goals, winner_id,
                        phase, round, status, leg, date, penalty_home_goals, penalty_away_goals,
                        group_id, window_id
                 FROM country_matches
                 WHERE season_id = ?1 AND status = ?2 AND date IS NOT NULL AND date <= ?3
                 ORDER BY date ASC, match_id ASC",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(
                params![season_id.0, MatchStatus::Scheduled as i64, date_str],
                map_country_match_row,
            )
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        Ok(rows)
    }

    pub fn callup_players_for_country(
        conn: &Connection,
        season_id: SeasonId,
        country_id: CountryId,
    ) -> NaratamaResult<Vec<Player>> {
        let mut stmt = conn
            .prepare(
                "SELECT p.*
                 FROM country_callups cc
                 JOIN players p ON p.player_id = cc.player_id
                 WHERE cc.season_id = ?1 AND cc.country_id = ?2 AND COALESCE(p.status_id, 1) = 1
                 ORDER BY p.power DESC, p.player_id ASC",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(
                params![season_id.0, country_id.0],
                ClubRepo::player_from_row,
            )
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        Ok(rows)
    }

    /// Saves completed international match, stats, fatigue, cap-tie locks, injuries (`match.date` based!), and Elo points atomically.
    #[allow(clippy::too_many_lines)]
    pub fn save_country_match_result(
        conn: &mut Connection,
        payload: &CountryMatchPersistPayload,
    ) -> NaratamaResult<()> {
        let tx = conn.transaction().map_err(db_err)?;

        let updated = tx
            .execute(
                "UPDATE country_matches SET
                    home_goals = ?1,
                    away_goals = ?2,
                    winner_id = ?3,
                    penalty_home_goals = ?4,
                    penalty_away_goals = ?5,
                    attendance_general = ?6,
                    revenue = ?7,
                    status = ?8
                 WHERE match_id = ?9 AND status != ?8",
                params![
                    payload.home_goals,
                    payload.away_goals,
                    payload.winner_id.map(|w| w.0),
                    payload.penalty_home_goals,
                    payload.penalty_away_goals,
                    payload.attendance_total,
                    payload.revenue,
                    MatchStatus::Finished as i64,
                    payload.match_id.0,
                ],
            )
            .map_err(db_err)?;

        if updated == 0 {
            return Ok(());
        }

        for pr in &payload.player_rows {
            tx.execute(
                "INSERT OR REPLACE INTO match_players_countries (
                    player_id, match_id, country_id, starter, minutes,
                    points, rating, mvp, goals, yellow_cards, red_cards, assists
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    pr.player_id.0,
                    payload.match_id.0,
                    pr.country_id.0,
                    i32::from(pr.starter),
                    pr.minutes,
                    pr.points,
                    pr.rating,
                    i32::from(pr.mvp),
                    pr.goals,
                    pr.yellow_cards,
                    pr.red_cards,
                    pr.assists,
                ],
            )
            .map_err(db_err)?;

            tx.execute(
                "INSERT INTO player_season_stats_countries (
                    player_id, country_id, season_id, matches_played, minutes, rating,
                    starts, yellow_cards, red_cards, mvps, goals, assists
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                 ON CONFLICT(player_id, country_id, season_id) DO UPDATE SET
                    matches_played = matches_played + excluded.matches_played,
                    minutes = minutes + excluded.minutes,
                    rating = rating + excluded.rating,
                    starts = starts + excluded.starts,
                    yellow_cards = yellow_cards + excluded.yellow_cards,
                    red_cards = red_cards + excluded.red_cards,
                    mvps = mvps + excluded.mvps,
                    goals = goals + excluded.goals,
                    assists = assists + excluded.assists",
                params![
                    pr.player_id.0,
                    pr.country_id.0,
                    payload.season_id.0,
                    i32::from(pr.minutes > 0),
                    pr.minutes,
                    pr.rating,
                    i32::from(pr.starter),
                    pr.yellow_cards,
                    pr.red_cards,
                    i32::from(pr.mvp),
                    pr.goals,
                    pr.assists,
                ],
            )
            .map_err(db_err)?;

            if pr.fatigue_drop > 0 {
                tx.execute(
                    "UPDATE players SET fitness = MAX(30, fitness - ?1) WHERE player_id = ?2",
                    params![pr.fatigue_drop, pr.player_id.0],
                )
                .map_err(db_err)?;
            }

            if payload.is_competitive && pr.minutes > 0 {
                Self::lock_allegiance_tx(&tx, pr.player_id, pr.country_id)?;
            }
        }

        for &(pid, ret_date) in &payload.injury_return_dates {
            let ret_str = ret_date.format("%Y-%m-%d").to_string();
            tx.execute(
                "UPDATE players SET
                    fitness = 40,
                    injury_return_date = CASE
                        WHEN injury_return_date IS NULL OR injury_return_date < ?1 THEN ?1
                        ELSE injury_return_date
                    END
                 WHERE player_id = ?2",
                params![ret_str, pid.0],
            )
            .map_err(db_err)?;
        }

        tx.execute(
            "UPDATE countries SET points = ?1 WHERE country_id = ?2",
            params![payload.new_home_points, payload.home_country_id.0],
        )
        .map_err(db_err)?;
        tx.execute(
            "UPDATE countries SET points = ?1 WHERE country_id = ?2",
            params![payload.new_away_points, payload.away_country_id.0],
        )
        .map_err(db_err)?;

        tx.commit().map_err(db_err)?;
        Ok(())
    }
}

fn map_country_row(row: &Row<'_>) -> rusqlite::Result<Country> {
    Ok(Country {
        country_id: CountryId(row.get(0)?),
        name: row.get(1)?,
        iso2: row.get(2)?,
        power: row.get(3)?,
        confederation: Confederation::from_code(row.get(4)?),
        points: row.get(5)?,
        previous_rank: row.get(6)?,
    })
}

fn map_country_match_row(row: &Row<'_>) -> rusqlite::Result<CountryMatchRow> {
    Ok(CountryMatchRow {
        match_id: MatchId(row.get(0)?),
        tournament_id: TournamentId(row.get(1)?),
        season_id: SeasonId(row.get(2)?),
        referee_id: row.get(3)?,
        stadium_id: StadiumId(row.get::<_, Option<i64>>(4)?.unwrap_or(1)),
        home_country_id: CountryId(row.get(5)?),
        away_country_id: CountryId(row.get(6)?),
        home_goals: row.get(7)?,
        away_goals: row.get(8)?,
        winner_id: row.get::<_, Option<i64>>(9)?.map(CountryId),
        phase: row.get::<_, Option<i32>>(10)?.unwrap_or(0),
        round: row.get::<_, Option<i32>>(11)?.unwrap_or(1),
        status: MatchStatus::from_code(row.get::<_, Option<i64>>(12)?.unwrap_or(1)),
        leg: row.get::<_, Option<i32>>(13)?.unwrap_or(1),
        date: parse_sqlite_date(row.get(14)?),
        penalty_home_goals: row.get(15)?,
        penalty_away_goals: row.get(16)?,
        group_id: row.get(17)?,
        window_id: row.get(18)?,
    })
}
