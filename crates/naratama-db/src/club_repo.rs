use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;
use naratama_model::{
    AwardType, ClubFacilities, CompetitionType, CountryId, MarketStatus, MatchId, MatchStatus,
    Money, NaratamaResult, OffseasonStep, Player, PlayerId, PlayerStatus, PreferredFoot, SeasonId,
    SquadStatus, StaffRole, TeamId, TournamentId,
};
use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};
use serde::{Deserialize, Serialize};

use crate::{db_err, parse_sqlite_date};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchPlayerInsert {
    pub player_id: PlayerId,
    pub team_id: TeamId,
    pub starter: bool,
    pub minutes: i32,
    pub goals: i32,
    pub assists: i32,
    pub yellow_cards: i32,
    pub red_cards: i32,
    pub mvp: bool,
    pub rating: i32,
    pub points: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedCardBanInput {
    pub player_id: PlayerId,
    pub matches_banned: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchPersistPayload {
    pub match_id: MatchId,
    pub season_id: SeasonId,
    pub tournament_id: TournamentId,
    pub competition_type: CompetitionType,
    pub home_team_id: TeamId,
    pub away_team_id: TeamId,
    pub home_goals: i32,
    pub away_goals: i32,
    pub winner_id: Option<TeamId>,
    pub penalty_home_goals: Option<i32>,
    pub penalty_away_goals: Option<i32>,
    pub attendance_total: i32,
    pub ticket_revenue: Money,
    pub match_date: NaiveDate,
    pub player_rows: Vec<MatchPlayerInsert>,
    pub incidents_json: Vec<String>,
    pub red_card_bans: Vec<RedCardBanInput>,
    pub injury_return_dates: Vec<(PlayerId, NaiveDate)>,
    pub team_stats: MatchTeamStats,
}

/// Persisted team-level match statistics.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchTeamStats {
    pub home_shots: i32,
    pub away_shots: i32,
    pub home_shots_on_target: i32,
    pub away_shots_on_target: i32,
    pub home_saves: i32,
    pub away_saves: i32,
    pub home_possession: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClubStaffRow {
    pub staff_id: i64,
    pub team_id: TeamId,
    pub role: StaffRole,
    pub name: String,
    pub stars: i32,
    pub salary: Money,
    pub specialty: String,
    pub hired_date: Option<NaiveDate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingStateRow {
    pub team_id: TeamId,
    pub season_id: SeasonId,
    pub last_match_count: i32,
    pub sessions: i32,
    pub done_focuses: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CareerStateRow {
    pub season_id: SeasonId,
    pub orientation_completed: bool,
    pub pre_season_date: Option<NaiveDate>,
    pub last_housekeeping_date: Option<NaiveDate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OffseasonStateRow {
    pub season_id: SeasonId,
    pub step: OffseasonStep,
    pub report: String,
    pub window_weeks: i32,
    pub camp: Option<String>,
    pub ambition: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YouthProspectRow {
    pub prospect_id: i64,
    pub team_id: TeamId,
    pub name: String,
    pub age: i32,
    pub position_id: i32,
    pub power: i32,
    pub potential: i32,
    pub stars: i32,
    pub profile: String,
    pub height: i32,
    pub country_id: CountryId,
    pub created_date: Option<NaiveDate>,
    pub signed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpiringContractRow {
    pub player: Player,
    pub end_date: String,
    pub new_salary: Money,
    pub signing_bonus: Money,
}

/// Transactional club/match/training/facility/suspension/offseason repository.
pub struct ClubRepo;

impl ClubRepo {
    pub fn player_from_row(row: &Row<'_>) -> rusqlite::Result<Player> {
        let foot_raw: Option<String> = row.get("profile")?;
        let status_code: i64 = row.get::<_, Option<i64>>("status_id")?.unwrap_or(1);
        let squad_code: i64 = row.get::<_, Option<i64>>("squad_status")?.unwrap_or(0);
        let market_code: i64 = row.get::<_, Option<i64>>("market_status")?.unwrap_or(1);
        let ret_str: Option<String> = row.get("injury_return_date")?;

        Ok(Player {
            player_id: PlayerId(row.get("player_id")?),
            name: row.get("name")?,
            age: row.get::<_, Option<i32>>("age")?.unwrap_or(22),
            position_id: row.get::<_, Option<i32>>("position_id")?.unwrap_or(8),
            preferred_foot: foot_raw
                .as_deref()
                .map_or(PreferredFoot::Right, PreferredFoot::from_db),
            power: row.get::<_, Option<i32>>("power")?.unwrap_or(60),
            potential: row.get::<_, Option<i32>>("potential")?.unwrap_or(65),
            fitness: row.get::<_, Option<i32>>("fitness")?.unwrap_or(100),
            value: row.get::<_, Option<i64>>("value")?.unwrap_or(100_000),
            salary: row.get::<_, Option<i64>>("salary")?.unwrap_or(5_000),
            country_id: CountryId(row.get::<_, Option<i64>>("country_id")?.unwrap_or(1)),
            squad_status: match squad_code {
                1 => SquadStatus::FirstTeam,
                2 => SquadStatus::LoanListed,
                _ => SquadStatus::Reserve,
            },
            market_status: match market_code {
                2 => MarketStatus::ForSale,
                3 => MarketStatus::OnLoan,
                _ => MarketStatus::Active,
            },
            status: match status_code {
                2 => PlayerStatus::Retired,
                3 => PlayerStatus::Dead,
                _ => PlayerStatus::Active,
            },
            is_captain: row.get::<_, Option<i32>>("is_captain")?.unwrap_or(0) != 0,
            shirt_number: row.get::<_, Option<i32>>("shirt_number")?.unwrap_or(10),
            stars: row.get::<_, Option<i32>>("stars")?.unwrap_or(3),
            skill_a: row.get::<_, Option<i32>>("skill_a")?.unwrap_or(0),
            skill_b: row.get::<_, Option<i32>>("skill_b")?.unwrap_or(0),
            height: row.get::<_, Option<i32>>("height")?.unwrap_or(178),
            championships: row.get("championships")?,
            injury_return_date: parse_sqlite_date(ret_str),
        })
    }

    pub fn squad_for_team(conn: &Connection, team_id: TeamId) -> NaratamaResult<Vec<Player>> {
        let mut stmt = conn
            .prepare(
                "SELECT p.*
                 FROM players p
                 JOIN team_players tp ON tp.player_id = p.player_id
                 WHERE tp.team_id = ?1 AND COALESCE(p.status_id, 1) = 1
                 ORDER BY p.power DESC, p.player_id ASC",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![team_id.0], Self::player_from_row)
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        Ok(rows)
    }

    pub fn squad_size(conn: &Connection, team_id: TeamId) -> NaratamaResult<i64> {
        conn.query_row(
            "SELECT COUNT(*) FROM team_players tp
             JOIN players p ON p.player_id = tp.player_id
             WHERE tp.team_id = ?1 AND COALESCE(p.status_id, 1) = 1",
            params![team_id.0],
            |r| r.get(0),
        )
        .map_err(db_err)
    }

    pub fn lineup_and_formation(
        conn: &Connection,
        team_id: TeamId,
    ) -> NaratamaResult<(String, Vec<PlayerId>)> {
        let formation: String = conn
            .query_row(
                "SELECT formation FROM team_tactics WHERE team_id = ?1",
                params![team_id.0],
                |r| r.get(0),
            )
            .optional()
            .map_err(db_err)?
            .unwrap_or_else(|| "4-4-2".to_string());

        let mut stmt = conn
            .prepare("SELECT player_id FROM team_lineups WHERE team_id = ?1 ORDER BY slot ASC")
            .map_err(db_err)?;
        let ids = stmt
            .query_map(params![team_id.0], |r| Ok(PlayerId(r.get(0)?)))
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        Ok((formation, ids))
    }

    pub fn save_lineup_and_formation(
        conn: &mut Connection,
        team_id: TeamId,
        formation: &str,
        starter_ids: &[PlayerId],
    ) -> NaratamaResult<()> {
        let tx = conn.transaction().map_err(db_err)?;
        tx.execute(
            "INSERT INTO team_tactics (team_id, formation) VALUES (?1, ?2)
             ON CONFLICT(team_id) DO UPDATE SET formation = excluded.formation",
            params![team_id.0, formation],
        )
        .map_err(db_err)?;
        tx.execute(
            "DELETE FROM team_lineups WHERE team_id = ?1",
            params![team_id.0],
        )
        .map_err(db_err)?;
        for (slot, pid) in starter_ids.iter().enumerate() {
            let slot_i32 = i32::try_from(slot).unwrap_or(0);
            tx.execute(
                "INSERT INTO team_lineups (team_id, slot, player_id) VALUES (?1, ?2, ?3)",
                params![team_id.0, slot_i32, pid.0],
            )
            .map_err(db_err)?;
        }
        tx.commit().map_err(db_err)?;
        Ok(())
    }

    pub fn refresh_team_power_tx(tx: &Transaction<'_>, team_id: TeamId) -> NaratamaResult<i32> {
        let power: Option<f64> = tx
            .query_row(
                "SELECT AVG(sub.power) FROM (
                    SELECT p.power FROM players p
                    JOIN team_players tp ON tp.player_id = p.player_id
                    WHERE tp.team_id = ?1 AND COALESCE(p.status_id, 1) = 1
                    ORDER BY p.power DESC
                    LIMIT 16
                ) sub",
                params![team_id.0],
                |r| r.get(0),
            )
            .map_err(db_err)?;
        #[allow(clippy::cast_possible_truncation)]
        let rounded = power.map_or(50, |v| v.round().clamp(30.0, 99.0) as i32);
        tx.execute(
            "UPDATE teams SET power = ?1 WHERE team_id = ?2",
            params![rounded, team_id.0],
        )
        .map_err(db_err)?;
        Ok(rounded)
    }

    pub fn refresh_team_power(conn: &mut Connection, team_id: TeamId) -> NaratamaResult<i32> {
        let tx = conn.transaction().map_err(db_err)?;
        let power = Self::refresh_team_power_tx(&tx, team_id)?;
        tx.commit().map_err(db_err)?;
        Ok(power)
    }

    /// Active bans strictly scoped to `competition_type` (plus `'all'` if present) in `season_id`.
    pub fn banned_players_for_competition(
        conn: &Connection,
        season_id: SeasonId,
        competition_type: CompetitionType,
        home_team_id: TeamId,
        away_team_id: TeamId,
    ) -> NaratamaResult<HashSet<PlayerId>> {
        let comp = competition_type.as_str();
        let mut stmt = conn
            .prepare(
                "SELECT s.player_id
                 FROM suspensions s
                 JOIN team_players tp ON tp.player_id = s.player_id
                 WHERE s.matches_remaining > 0
                   AND tp.team_id IN (?1, ?2)
                   AND (s.season_id IS NULL OR s.season_id = ?3)
                   AND (
                       s.competition_type = ?4
                       OR s.competition_type = 'all'
                       OR (s.competition_type IS NULL AND ?4 = 'league')
                   )",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(
                params![home_team_id.0, away_team_id.0, season_id.0, comp],
                |r| Ok(PlayerId(r.get(0)?)),
            )
            .map_err(db_err)?
            .collect::<Result<HashSet<_>, _>>()
            .map_err(db_err)?;
        Ok(rows)
    }

    /// Queries all active suspensions for a team (used for UI / testing inspection).
    pub fn active_suspensions_for_team(
        conn: &Connection,
        season_id: SeasonId,
        team_id: TeamId,
    ) -> NaratamaResult<Vec<(PlayerId, String, i32)>> {
        let mut stmt = conn
            .prepare(
                "SELECT s.player_id, COALESCE(s.competition_type, 'league'), s.matches_remaining
                 FROM suspensions s
                 JOIN team_players tp ON tp.player_id = s.player_id
                 WHERE tp.team_id = ?1
                   AND s.matches_remaining > 0
                   AND (s.season_id IS NULL OR s.season_id = ?2)
                 ORDER BY s.player_id ASC, s.competition_type ASC",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![team_id.0, season_id.0], |r| {
                Ok((PlayerId(r.get(0)?), r.get(1)?, r.get(2)?))
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        Ok(rows)
    }

    /// Decrements bans ONLY for `competition_type` across `home_team_id` and `away_team_id`.
    pub fn tick_suspensions_for_match_tx(
        tx: &Transaction<'_>,
        season_id: SeasonId,
        competition_type: CompetitionType,
        home_team_id: TeamId,
        away_team_id: TeamId,
    ) -> NaratamaResult<HashSet<PlayerId>> {
        let comp = competition_type.as_str();
        let mut stmt = tx
            .prepare(
                "SELECT s.suspension_id, s.player_id, s.matches_remaining
                 FROM suspensions s
                 JOIN team_players tp ON tp.player_id = s.player_id
                 WHERE s.matches_remaining > 0
                   AND tp.team_id IN (?1, ?2)
                   AND (s.season_id IS NULL OR s.season_id = ?3)
                   AND (s.competition_type = ?4 OR (s.competition_type IS NULL AND ?4 = 'league'))
                 ORDER BY s.suspension_id ASC",
            )
            .map_err(db_err)?;
        let rows: Vec<(i64, i64, i32)> = stmt
            .query_map(
                params![home_team_id.0, away_team_id.0, season_id.0, comp],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        drop(stmt);

        let mut by_player: HashMap<i64, Vec<(i64, i32)>> = HashMap::new();
        for (susp_id, player_id, rem) in rows {
            by_player.entry(player_id).or_default().push((susp_id, rem));
        }

        let mut still_out = HashSet::new();
        for (player_id, player_rows) in by_player {
            let primary_id = player_rows[0].0;
            let total_rem: i32 = player_rows.iter().map(|(_, rem)| (*rem).max(1)).sum();
            let next_rem = total_rem - 1;
            if next_rem > 0 {
                tx.execute(
                    "UPDATE suspensions
                     SET matches_remaining = ?1, competition_type = ?2, season_id = ?3
                     WHERE suspension_id = ?4",
                    params![next_rem, comp, season_id.0, primary_id],
                )
                .map_err(db_err)?;
                still_out.insert(PlayerId(player_id));
            } else {
                tx.execute(
                    "DELETE FROM suspensions WHERE suspension_id = ?1",
                    params![primary_id],
                )
                .map_err(db_err)?;
            }
            for &(dup_id, _) in player_rows.iter().skip(1) {
                tx.execute(
                    "DELETE FROM suspensions WHERE suspension_id = ?1",
                    params![dup_id],
                )
                .map_err(db_err)?;
            }
        }
        Ok(still_out)
    }

    /// Inserts or stacks red-card suspensions strictly scoped to `competition_type` + `season_id`.
    pub fn apply_red_card_bans_tx(
        tx: &Transaction<'_>,
        season_id: SeasonId,
        competition_type: CompetitionType,
        bans: &[RedCardBanInput],
    ) -> NaratamaResult<()> {
        let comp = competition_type.as_str();
        for ban in bans {
            if ban.matches_banned <= 0 {
                continue;
            }
            let mut stmt = tx
                .prepare(
                    "SELECT suspension_id, matches_remaining
                     FROM suspensions
                     WHERE player_id = ?1 AND matches_remaining > 0
                       AND (season_id IS NULL OR season_id = ?2)
                       AND (competition_type = ?3 OR (competition_type IS NULL AND ?3 = 'league'))
                     ORDER BY suspension_id ASC",
                )
                .map_err(db_err)?;
            let existing: Vec<(i64, i32)> = stmt
                .query_map(params![ban.player_id.0, season_id.0, comp], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })
                .map_err(db_err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(db_err)?;
            drop(stmt);

            if let Some(&(primary_id, _)) = existing.first() {
                let total_rem: i32 =
                    existing.iter().map(|(_, r)| (*r).max(0)).sum::<i32>() + ban.matches_banned;
                tx.execute(
                    "UPDATE suspensions
                     SET matches_remaining = ?1, competition_type = ?2, season_id = ?3
                     WHERE suspension_id = ?4",
                    params![total_rem, comp, season_id.0, primary_id],
                )
                .map_err(db_err)?;
                for &(dup_id, _) in existing.iter().skip(1) {
                    tx.execute(
                        "DELETE FROM suspensions WHERE suspension_id = ?1",
                        params![dup_id],
                    )
                    .map_err(db_err)?;
                }
            } else {
                tx.execute(
                    "INSERT INTO suspensions (player_id, type, matches_remaining, season_id, competition_type)
                     VALUES (?1, 2, ?2, ?3, ?4)",
                    params![ban.player_id.0, ban.matches_banned, season_id.0, comp],
                )
                .map_err(db_err)?;
            }
        }
        Ok(())
    }

    /// Weekly housekeeping suspension tick: ONLY ticks legacy rows where `competition_type IS NULL`
    /// so competition-scoped bans (`league`, `nationalCup`, `continental`, `friendly`) are NEVER touched by weekly ticks.
    pub fn tick_untyped_suspensions(
        conn: &mut Connection,
        season_id: SeasonId,
    ) -> NaratamaResult<()> {
        let tx = conn.transaction().map_err(db_err)?;
        tx.execute(
            "UPDATE suspensions
             SET matches_remaining = matches_remaining - 1
             WHERE competition_type IS NULL AND matches_remaining > 0
               AND (season_id IS NULL OR season_id = ?1)",
            params![season_id.0],
        )
        .map_err(db_err)?;
        tx.execute(
            "DELETE FROM suspensions WHERE competition_type IS NULL AND matches_remaining <= 0",
            [],
        )
        .map_err(db_err)?;
        tx.commit().map_err(db_err)?;
        Ok(())
    }

    /// Saves a completed match, player stats, incidents, ticks prior bans, applies new red cards and injuries atomically.
    #[allow(clippy::too_many_lines)]
    pub fn save_match_result(
        conn: &mut Connection,
        payload: &MatchPersistPayload,
    ) -> NaratamaResult<()> {
        let tx = conn.transaction().map_err(db_err)?;

        tx.execute(
            "UPDATE matches SET
                home_goals = ?1,
                away_goals = ?2,
                winner_id = ?3,
                penalty_home_goals = ?4,
                penalty_away_goals = ?5,
                attendance_general = ?6,
                revenue = ?7,
                status = ?8,
                home_shots = ?10,
                away_shots = ?11,
                home_shots_on_target = ?12,
                away_shots_on_target = ?13,
                home_saves = ?14,
                away_saves = ?15,
                home_possession = ?16
             WHERE match_id = ?9",
            params![
                payload.home_goals,
                payload.away_goals,
                payload.winner_id.map(|w| w.0),
                payload.penalty_home_goals,
                payload.penalty_away_goals,
                payload.attendance_total,
                payload.ticket_revenue,
                MatchStatus::Finished as i64,
                payload.match_id.0,
                payload.team_stats.home_shots,
                payload.team_stats.away_shots,
                payload.team_stats.home_shots_on_target,
                payload.team_stats.away_shots_on_target,
                payload.team_stats.home_saves,
                payload.team_stats.away_saves,
                payload.team_stats.home_possession,
            ],
        )
        .map_err(db_err)?;

        if !payload.player_rows.is_empty() {
            let mut mp_stmt = tx
                .prepare(
                    "INSERT OR REPLACE INTO match_players (
                        player_id, match_id, team_id, starter, minutes,
                        points, rating, mvp, goals, yellow_cards, red_cards, assists
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                )
                .map_err(db_err)?;
            for pr in &payload.player_rows {
                mp_stmt
                    .execute(params![
                        pr.player_id.0,
                        payload.match_id.0,
                        pr.team_id.0,
                        i32::from(pr.starter),
                        pr.minutes,
                        pr.points,
                        pr.rating,
                        i32::from(pr.mvp),
                        pr.goals,
                        pr.yellow_cards,
                        pr.red_cards,
                        pr.assists,
                    ])
                    .map_err(db_err)?;
            }
        }

        if !payload.incidents_json.is_empty() {
            let mut inc_stmt = tx
                .prepare("INSERT INTO match_incidents (match_id, data) VALUES (?1, ?2)")
                .map_err(db_err)?;
            for inc in &payload.incidents_json {
                inc_stmt
                    .execute(params![payload.match_id.0, inc])
                    .map_err(db_err)?;
            }
        }

        // Decrement existing bans strictly in this competition BEFORE adding this match's new red cards.
        Self::tick_suspensions_for_match_tx(
            &tx,
            payload.season_id,
            payload.competition_type,
            payload.home_team_id,
            payload.away_team_id,
        )?;
        Self::apply_red_card_bans_tx(
            &tx,
            payload.season_id,
            payload.competition_type,
            &payload.red_card_bans,
        )?;

        for &(player_id, ret_date) in &payload.injury_return_dates {
            let ret_str = ret_date.format("%Y-%m-%d").to_string();
            tx.execute(
                "UPDATE players SET
                    fitness = 40,
                    injury_return_date = CASE
                        WHEN injury_return_date IS NULL OR injury_return_date < ?1 THEN ?1
                        ELSE injury_return_date
                    END
                 WHERE player_id = ?2",
                params![ret_str, player_id.0],
            )
            .map_err(db_err)?;
        }

        tx.commit().map_err(db_err)?;
        Ok(())
    }

    pub fn clear_healed_injuries(conn: &mut Connection, date: NaiveDate) -> NaratamaResult<usize> {
        let date_str = date.format("%Y-%m-%d").to_string();
        conn.execute(
            "UPDATE players SET injury_return_date = NULL
             WHERE injury_return_date IS NOT NULL AND injury_return_date <= ?1",
            params![date_str],
        )
        .map_err(db_err)
    }

    pub fn facilities(conn: &Connection, team_id: TeamId) -> NaratamaResult<ClubFacilities> {
        conn.execute(
            "INSERT OR IGNORE INTO team_facilities (team_id, training_level, medical_level)
             VALUES (?1, 1, 1)",
            params![team_id.0],
        )
        .map_err(db_err)?;
        conn.query_row(
            "SELECT team_id, training_level, medical_level FROM team_facilities WHERE team_id = ?1",
            params![team_id.0],
            |r| {
                Ok(ClubFacilities {
                    team_id: TeamId(r.get(0)?),
                    training_level: r.get(1)?,
                    medical_level: r.get(2)?,
                })
            },
        )
        .map_err(db_err)
    }

    pub fn staff_for_team(conn: &Connection, team_id: TeamId) -> NaratamaResult<Vec<ClubStaffRow>> {
        let mut stmt = conn
            .prepare(
                "SELECT staff_id, team_id, role, name, COALESCE(stars, 1),
                        COALESCE(salary, 5000), COALESCE(specialty, ''), hired_date
                 FROM team_staff WHERE team_id = ?1",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![team_id.0], |r| {
                let role_str: String = r.get(2)?;
                let role = match role_str.as_str() {
                    "tactical_analyst" => StaffRole::TacticalAnalyst,
                    "head_scout" => StaffRole::HeadScout,
                    _ => StaffRole::HeadPhysio,
                };
                Ok(ClubStaffRow {
                    staff_id: r.get(0)?,
                    team_id: TeamId(r.get(1)?),
                    role,
                    name: r.get(3)?,
                    stars: r.get(4)?,
                    salary: r.get(5)?,
                    specialty: r.get(6)?,
                    hired_date: parse_sqlite_date(r.get(7)?),
                })
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        Ok(rows)
    }

    pub fn total_weekly_staff_wages(conn: &Connection, team_id: TeamId) -> NaratamaResult<Money> {
        let total: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(salary), 0) FROM team_staff WHERE team_id = ?1",
                params![team_id.0],
                |r| r.get(0),
            )
            .map_err(db_err)?;
        Ok(total)
    }

    pub fn training_state(
        conn: &Connection,
        team_id: TeamId,
        season_id: SeasonId,
    ) -> NaratamaResult<TrainingStateRow> {
        let row = conn
            .query_row(
                "SELECT COALESCE(last_match_count, -1), COALESCE(sessions, 0), COALESCE(done_focuses, '')
                 FROM team_training WHERE team_id = ?1 AND season_id = ?2",
                params![team_id.0, season_id.0],
                |r| {
                    let raw: String = r.get(2)?;
                    let done = raw
                        .split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(ToString::to_string)
                        .collect();
                    Ok(TrainingStateRow {
                        team_id,
                        season_id,
                        last_match_count: r.get(0)?,
                        sessions: r.get(1)?,
                        done_focuses: done,
                    })
                },
            )
            .optional()
            .map_err(db_err)?;
        Ok(row.unwrap_or(TrainingStateRow {
            team_id,
            season_id,
            last_match_count: -1,
            sessions: 0,
            done_focuses: Vec::new(),
        }))
    }

    pub fn finished_match_count_for_team(
        conn: &Connection,
        season_id: SeasonId,
        team_id: TeamId,
    ) -> NaratamaResult<i32> {
        conn.query_row(
            "SELECT COUNT(*) FROM matches
             WHERE season_id = ?1 AND status = ?2
               AND (home_team_id = ?3 OR away_team_id = ?3)",
            params![season_id.0, MatchStatus::Finished as i64, team_id.0],
            |r| r.get(0),
        )
        .map_err(db_err)
    }

    pub fn career_state(conn: &Connection, season_id: SeasonId) -> NaratamaResult<CareerStateRow> {
        let row = conn
            .query_row(
                "SELECT season_id, COALESCE(orientation_completed, 0), pre_season_date, last_housekeeping_date
                 FROM career_state WHERE season_id = ?1",
                params![season_id.0],
                |r| {
                    Ok(CareerStateRow {
                        season_id: SeasonId(r.get(0)?),
                        orientation_completed: r.get::<_, i32>(1)? != 0,
                        pre_season_date: parse_sqlite_date(r.get(2)?),
                        last_housekeeping_date: parse_sqlite_date(r.get(3)?),
                    })
                },
            )
            .optional()
            .map_err(db_err)?;
        Ok(row.unwrap_or(CareerStateRow {
            season_id,
            orientation_completed: false,
            pre_season_date: None,
            last_housekeeping_date: None,
        }))
    }

    pub fn save_career_state(conn: &Connection, state: &CareerStateRow) -> NaratamaResult<()> {
        let pre = state
            .pre_season_date
            .map(|d| d.format("%Y-%m-%d").to_string());
        let hk = state
            .last_housekeeping_date
            .map(|d| d.format("%Y-%m-%d").to_string());
        conn.execute(
            "INSERT INTO career_state (season_id, orientation_completed, pre_season_date, last_housekeeping_date)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(season_id) DO UPDATE SET
                orientation_completed = excluded.orientation_completed,
                pre_season_date = COALESCE(excluded.pre_season_date, career_state.pre_season_date),
                last_housekeeping_date = COALESCE(excluded.last_housekeeping_date, career_state.last_housekeeping_date)",
            params![
                state.season_id.0,
                i32::from(state.orientation_completed),
                pre,
                hk
            ],
        )
        .map_err(db_err)?;
        Ok(())
    }

    pub fn offseason_state(conn: &Connection) -> NaratamaResult<Option<OffseasonStateRow>> {
        conn.query_row(
            "SELECT season_id, COALESCE(step, 0), COALESCE(report, ''),
                    COALESCE(window_weeks, 0), camp, ambition
             FROM offseason_state WHERE id = 1",
            [],
            |r| {
                let step_raw: i64 = r.get(1)?;
                let step = match step_raw {
                    1 => OffseasonStep::Contracts,
                    2 => OffseasonStep::Development,
                    3 => OffseasonStep::Transfers,
                    4 => OffseasonStep::Preseason,
                    5 => OffseasonStep::Board,
                    6 => OffseasonStep::NewSeason,
                    _ => OffseasonStep::Summary,
                };
                Ok(OffseasonStateRow {
                    season_id: SeasonId(r.get(0)?),
                    step,
                    report: r.get(2)?,
                    window_weeks: r.get(3)?,
                    camp: r.get(4)?,
                    ambition: r.get(5)?,
                })
            },
        )
        .optional()
        .map_err(db_err)
    }

    pub fn save_offseason_state(
        conn: &Connection,
        state: &OffseasonStateRow,
    ) -> NaratamaResult<()> {
        conn.execute(
            "INSERT INTO offseason_state (id, season_id, step, report, window_weeks, camp, ambition)
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
                season_id = excluded.season_id,
                step = excluded.step,
                report = excluded.report,
                window_weeks = excluded.window_weeks,
                camp = COALESCE(excluded.camp, offseason_state.camp),
                ambition = COALESCE(excluded.ambition, offseason_state.ambition)",
            params![
                state.season_id.0,
                state.step as i64,
                state.report,
                state.window_weeks,
                state.camp,
                state.ambition,
            ],
        )
        .map_err(db_err)?;
        Ok(())
    }

    pub fn insert_message(
        conn: &Connection,
        title: &str,
        body: &str,
        date: NaiveDate,
        sender: &str,
        team_id: Option<TeamId>,
        transfer_offer_id: Option<i64>,
    ) -> NaratamaResult<i64> {
        let date_str = date.format("%Y-%m-%d").to_string();
        conn.execute(
            "INSERT INTO messages (title, body, date, sender, is_read, team_id, transfer_offer_id)
             VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6)",
            params![
                title,
                body,
                date_str,
                sender,
                team_id.map(|t| t.0),
                transfer_offer_id
            ],
        )
        .map_err(db_err)?;
        Ok(conn.last_insert_rowid())
    }

    /// Bug #19 fix: explicitly archives `match_players` -> `player_season_stats` & removes raw rows in one transaction.
    pub fn archive_season_stats(conn: &mut Connection, season_id: SeasonId) -> NaratamaResult<()> {
        let tx = conn.transaction().map_err(db_err)?;
        tx.execute(
            "INSERT OR REPLACE INTO player_season_stats (
                player_id, team_id, season_id, matches_played, minutes, rating,
                starts, yellow_cards, red_cards, mvps, goals, assists
            )
            SELECT
                mp.player_id,
                mp.team_id,
                m.season_id,
                COUNT(CASE WHEN mp.minutes > 0 THEN 1 END),
                COALESCE(SUM(mp.minutes), 0),
                COALESCE(SUM(mp.rating), 0),
                COALESCE(SUM(CASE WHEN mp.starter = 1 THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(mp.yellow_cards), 0),
                COALESCE(SUM(mp.red_cards), 0),
                COALESCE(SUM(CASE WHEN mp.mvp = 1 THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(mp.goals), 0),
                COALESCE(SUM(mp.assists), 0)
            FROM match_players mp
            JOIN matches m ON m.match_id = mp.match_id
            WHERE m.season_id = ?1
            GROUP BY mp.player_id, mp.team_id",
            params![season_id.0],
        )
        .map_err(db_err)?;

        tx.execute(
            "DELETE FROM match_players WHERE match_id IN (SELECT match_id FROM matches WHERE season_id = ?1)",
            params![season_id.0],
        )
        .map_err(db_err)?;
        tx.execute(
            "DELETE FROM match_incidents WHERE match_id IN (SELECT match_id FROM matches WHERE season_id = ?1)",
            params![season_id.0],
        )
        .map_err(db_err)?;
        tx.execute(
            "UPDATE season_tournaments SET status = 0 WHERE season_id = ?1 AND status = 1",
            params![season_id.0],
        )
        .map_err(db_err)?;
        tx.commit().map_err(db_err)?;
        Ok(())
    }

    /// Inserts season player awards (`TopScorer`, `Mvp`, `YoungPlayer`, `GoldenGlove`, `BallonDor`, `ManagerOfTheYear`).
    /// Bug #8 fix: uses `manager_name` only if `champion_team_id == user_team_id`.
    #[allow(clippy::too_many_lines)]
    pub fn record_season_awards(
        conn: &mut Connection,
        season_id: SeasonId,
        year: i32,
        league_tournament_id: TournamentId,
        user_team_id: TeamId,
        champion_team_id: TeamId,
        manager_name: &str,
    ) -> NaratamaResult<Vec<(AwardType, String)>> {
        const MIN_APPS: i32 = 8;
        let tx = conn.transaction().map_err(db_err)?;
        tx.execute(
            "DELETE FROM player_awards WHERE season_id = ?1",
            params![season_id.0],
        )
        .map_err(db_err)?;

        let mut recorded = Vec::new();

        // 1. Top Scorer
        if let Some((pid, tid, pos, goals, avg_rat, pname)) = tx
            .query_row(
                "SELECT mp.player_id, mp.team_id, COALESCE(p.position_id, 9),
                        COALESCE(SUM(mp.goals), 0) AS goals,
                        COALESCE(AVG(mp.rating), 65.0) AS avg_rat,
                        p.name
                 FROM match_players mp
                 JOIN matches m ON m.match_id = mp.match_id
                 JOIN players p ON p.player_id = mp.player_id
                 WHERE m.season_id = ?1 AND m.tournament_id = ?2 AND m.status = ?3
                 GROUP BY mp.player_id, mp.team_id
                 HAVING SUM(mp.goals) > 0 AND COUNT(DISTINCT mp.match_id) >= ?4
                 ORDER BY goals DESC, SUM(mp.assists) DESC, avg_rat DESC
                 LIMIT 1",
                params![
                    season_id.0,
                    league_tournament_id.0,
                    MatchStatus::Finished as i64,
                    MIN_APPS
                ],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, i32>(2)?,
                        r.get::<_, i32>(3)?,
                        r.get::<_, f64>(4)?,
                        r.get::<_, String>(5)?,
                    ))
                },
            )
            .optional()
            .map_err(db_err)?
        {
            let rat = if avg_rat > 10.0 {
                avg_rat / 10.0
            } else {
                avg_rat
            };
            let sub = format!("{goals} Goals - Rating {rat:.1}");
            tx.execute(
                "INSERT INTO player_awards (player_id, team_id, type, year, season_id, goals, rating, stat_subtitle, position_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![pid, tid, AwardType::TopScorer as i64, year, season_id.0, goals, rat, sub, pos],
            )
            .map_err(db_err)?;
            recorded.push((AwardType::TopScorer, pname));
        }

        // 2. MVP
        if let Some((pid, tid, pos, goals, mvps, avg_rat, pname)) = tx
            .query_row(
                "SELECT mp.player_id, mp.team_id, COALESCE(p.position_id, 8),
                        COALESCE(SUM(mp.goals), 0), COALESCE(SUM(mp.mvp), 0) AS mvps,
                        COALESCE(AVG(mp.rating), 65.0) AS avg_rat, p.name
                 FROM match_players mp
                 JOIN matches m ON m.match_id = mp.match_id
                 JOIN players p ON p.player_id = mp.player_id
                 WHERE m.season_id = ?1 AND m.tournament_id = ?2 AND m.status = ?3 AND mp.minutes > 0
                 GROUP BY mp.player_id, mp.team_id
                 HAVING COUNT(DISTINCT mp.match_id) >= ?4
                 ORDER BY mvps DESC, avg_rat DESC, (SUM(mp.goals) + SUM(mp.assists)) DESC
                 LIMIT 1",
                params![
                    season_id.0,
                    league_tournament_id.0,
                    MatchStatus::Finished as i64,
                    MIN_APPS
                ],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, i32>(2)?,
                        r.get::<_, i32>(3)?,
                        r.get::<_, i32>(4)?,
                        r.get::<_, f64>(5)?,
                        r.get::<_, String>(6)?,
                    ))
                },
            )
            .optional()
            .map_err(db_err)?
        {
            let rat = if avg_rat > 10.0 { avg_rat / 10.0 } else { avg_rat };
            let sub = format!("{mvps}x MVP - Rating {rat:.1}");
            tx.execute(
                "INSERT INTO player_awards (player_id, team_id, type, year, season_id, goals, rating, stat_subtitle, position_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![pid, tid, AwardType::Mvp as i64, year, season_id.0, goals, rat, sub, pos],
            )
            .map_err(db_err)?;
            recorded.push((AwardType::Mvp, pname));
        }

        // 3. Young Player (age <= 21)
        if let Some((pid, tid, pos, goals, avg_rat, pname)) = tx
            .query_row(
                "SELECT mp.player_id, mp.team_id, COALESCE(p.position_id, 8),
                        COALESCE(SUM(mp.goals), 0), COALESCE(AVG(mp.rating), 65.0) AS avg_rat, p.name
                 FROM match_players mp
                 JOIN matches m ON m.match_id = mp.match_id
                 JOIN players p ON p.player_id = mp.player_id
                 WHERE m.season_id = ?1 AND m.tournament_id = ?2 AND m.status = ?3 AND p.age <= 21
                 GROUP BY mp.player_id, mp.team_id
                 HAVING COUNT(DISTINCT mp.match_id) >= ?4
                 ORDER BY avg_rat DESC, (SUM(mp.goals) + SUM(mp.assists)) DESC
                 LIMIT 1",
                params![
                    season_id.0,
                    league_tournament_id.0,
                    MatchStatus::Finished as i64,
                    MIN_APPS
                ],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, i32>(2)?,
                        r.get::<_, i32>(3)?,
                        r.get::<_, f64>(4)?,
                        r.get::<_, String>(5)?,
                    ))
                },
            )
            .optional()
            .map_err(db_err)?
        {
            let rat = if avg_rat > 10.0 { avg_rat / 10.0 } else { avg_rat };
            let sub = format!("U21 - Rating {rat:.1}");
            tx.execute(
                "INSERT INTO player_awards (player_id, team_id, type, year, season_id, goals, rating, stat_subtitle, position_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![pid, tid, AwardType::YoungPlayer as i64, year, season_id.0, goals, rat, sub, pos],
            )
            .map_err(db_err)?;
            recorded.push((AwardType::YoungPlayer, pname));
        }

        // 4. Golden Glove (best GK by average rating)
        if let Some((pid, tid, avg_rat, pname)) = tx
            .query_row(
                "SELECT mp.player_id, mp.team_id, COALESCE(AVG(mp.rating), 65.0) AS avg_rat, p.name
                 FROM match_players mp
                 JOIN matches m ON m.match_id = mp.match_id
                 JOIN players p ON p.player_id = mp.player_id
                 WHERE m.season_id = ?1 AND m.tournament_id = ?2 AND m.status = ?3
                   AND p.position_id = 1 AND mp.minutes > 0
                 GROUP BY mp.player_id, mp.team_id
                 HAVING COUNT(DISTINCT mp.match_id) >= ?4
                 ORDER BY avg_rat DESC
                 LIMIT 1",
                params![
                    season_id.0,
                    league_tournament_id.0,
                    MatchStatus::Finished as i64,
                    MIN_APPS
                ],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, f64>(2)?,
                        r.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()
            .map_err(db_err)?
        {
            let rat = if avg_rat > 10.0 {
                avg_rat / 10.0
            } else {
                avg_rat
            };
            let sub = format!("GK - Rating {rat:.1}");
            tx.execute(
                "INSERT INTO player_awards (player_id, team_id, type, year, season_id, goals, rating, stat_subtitle, position_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, ?7, 1)",
                params![pid, tid, AwardType::GoldenGlove as i64, year, season_id.0, rat, sub],
            )
            .map_err(db_err)?;
            recorded.push((AwardType::GoldenGlove, pname));
        }

        // 5. Ballon d'Or (highest combined power & rating across all active seasons)
        if let Some((pid, tid, pos, goals, avg_rat, pname)) = tx
            .query_row(
                "SELECT mp.player_id, mp.team_id, COALESCE(p.position_id, 9),
                        COALESCE(SUM(mp.goals), 0), COALESCE(AVG(mp.rating), 65.0) AS avg_rat, p.name
                 FROM match_players mp
                 JOIN matches m ON m.match_id = mp.match_id
                 JOIN players p ON p.player_id = mp.player_id
                 WHERE m.season_id = ?1 AND m.status = ?2 AND mp.minutes > 0
                 GROUP BY mp.player_id, mp.team_id
                 HAVING COUNT(DISTINCT mp.match_id) >= ?3
                 ORDER BY (p.power * 0.6 + avg_rat * 0.4 + SUM(mp.goals)) DESC
                 LIMIT 1",
                params![season_id.0, MatchStatus::Finished as i64, MIN_APPS],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, i32>(2)?,
                        r.get::<_, i32>(3)?,
                        r.get::<_, f64>(4)?,
                        r.get::<_, String>(5)?,
                    ))
                },
            )
            .optional()
            .map_err(db_err)?
        {
            let rat = if avg_rat > 10.0 { avg_rat / 10.0 } else { avg_rat };
            let sub = format!("World Best - Rating {rat:.1}");
            tx.execute(
                "INSERT INTO player_awards (player_id, team_id, type, year, season_id, goals, rating, stat_subtitle, position_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![pid, tid, AwardType::BallonDor as i64, year, season_id.0, goals, rat, sub, pos],
            )
            .map_err(db_err)?;
            recorded.push((AwardType::BallonDor, pname));
        }

        // 6. Manager of the Year (Bug #8 fix: use user's manager_name ONLY if champion == user_team_id)
        let winner_coach_name: String = if champion_team_id == user_team_id {
            manager_name.to_string()
        } else {
            tx.query_row(
                "SELECT COALESCE(c.name, 'Head Coach')
                 FROM teams t
                 LEFT JOIN coaches c ON c.coach_id = t.coach_id
                 WHERE t.team_id = ?1",
                params![champion_team_id.0],
                |r| r.get(0),
            )
            .optional()
            .map_err(db_err)?
            .unwrap_or_else(|| "Head Coach".to_string())
        };
        let coach_sub = format!("Champion Manager - {winner_coach_name}");
        tx.execute(
            "INSERT INTO player_awards (player_id, team_id, type, year, season_id, goals, rating, stat_subtitle, position_id)
             VALUES (0, ?1, ?2, ?3, ?4, 0, 9.0, ?5, 0)",
            params![
                champion_team_id.0,
                AwardType::ManagerOfTheYear as i64,
                year,
                season_id.0,
                coach_sub,
            ],
        )
        .map_err(db_err)?;
        recorded.push((AwardType::ManagerOfTheYear, winner_coach_name));

        tx.commit().map_err(db_err)?;
        Ok(recorded)
    }
}
