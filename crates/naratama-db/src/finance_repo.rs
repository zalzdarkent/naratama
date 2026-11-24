use chrono::NaiveDate;
use naratama_model::{
    FfpEnforcement, LedgerCategory, LedgerPeriod, Money, NaratamaResult, SeasonId, TeamId,
    TournamentId,
};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};

use crate::{db_err, parse_sqlite_date};

/// Input payload to append an immutable entry to `finance_ledger` and atomically adjust `teams.budget`.
#[derive(Debug, Clone)]
pub struct NewLedgerEntry<'a> {
    pub team_id: TeamId,
    pub season_id: SeasonId,
    pub entry_date: NaiveDate,
    pub period: LedgerPeriod,
    pub category: LedgerCategory,
    pub subcategory: &'a str,
    pub amount: Money,
    pub ref_type: &'a str,
    pub ref_id: Option<i64>,
    pub description: &'a str,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinanceClubState {
    pub team_id: TeamId,
    pub consecutive_breaches: i32,
    pub distress: bool,
    pub embargo_until_season: Option<i32>,
    pub last_injection_season: Option<i32>,
    pub last_ffp_level: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinanceSettingsRow {
    pub mode: FfpEnforcement,
    pub completed_seasons: i32,
    pub initialized: bool,
    pub opening_balance_date: Option<NaiveDate>,
    pub mode_locked: bool,
    pub last_completed_season: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SponsorshipRow {
    pub sponsorship_id: i64,
    pub deal_type: String,
    pub team_id: TeamId,
    pub sponsor_id: i64,
    pub sponsor_name: String,
    pub tournament_id: Option<TournamentId>,
    pub season_id: SeasonId,
    pub is_priority: bool,
    pub is_unique_prize: bool,
    pub base_reward: Money,
    pub target: i64,
    pub progress: i64,
    pub success: bool,
    pub status: i32,
    pub extra_data: Option<String>,
}

/// Unified transactional finance & sponsorship ledger repository (`finance_ledger` + `teams.budget`).
pub struct FinanceRepo;

impl FinanceRepo {
    /// Appends one immutable ledger row and updates `teams.budget` in the caller's transaction.
    /// This is the single write gate enforcing `teams.budget == SUM(finance_ledger.amount)`.
    pub fn record_tx(tx: &Transaction<'_>, entry: &NewLedgerEntry<'_>) -> NaratamaResult<i64> {
        let date_str = entry.entry_date.format("%Y-%m-%d").to_string();
        tx.execute(
            "INSERT INTO finance_ledger (
                team_id, season_id, entry_date, period, category,
                subcategory, amount, ref_type, ref_id, description, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                entry.team_id.0,
                entry.season_id.0,
                date_str,
                entry.period.as_str(),
                entry.category.as_str(),
                entry.subcategory,
                entry.amount,
                entry.ref_type,
                entry.ref_id,
                entry.description,
                date_str,
            ],
        )
        .map_err(db_err)?;
        let ledger_id = tx.last_insert_rowid();

        tx.execute(
            "UPDATE teams SET budget = COALESCE(budget, 0) + ?1 WHERE team_id = ?2",
            params![entry.amount, entry.team_id.0],
        )
        .map_err(db_err)?;

        Ok(ledger_id)
    }

    pub fn record(conn: &mut Connection, entry: &NewLedgerEntry<'_>) -> NaratamaResult<i64> {
        let tx = conn.transaction().map_err(db_err)?;
        let id = Self::record_tx(&tx, entry)?;
        tx.commit().map_err(db_err)?;
        Ok(id)
    }

    pub fn record_batch(
        conn: &mut Connection,
        entries: &[NewLedgerEntry<'_>],
    ) -> NaratamaResult<()> {
        if entries.is_empty() {
            return Ok(());
        }
        let tx = conn.transaction().map_err(db_err)?;
        for entry in entries {
            Self::record_tx(&tx, entry)?;
        }
        tx.commit().map_err(db_err)?;
        Ok(())
    }

    /// Seeds initial `opening_balance` entries for every club once so
    /// `teams.budget == SUM(finance_ledger.amount)` holds across all 2,014 clubs from Day 0.
    pub fn ensure_ready(
        conn: &mut Connection,
        season_id: SeasonId,
        opening_date: NaiveDate,
    ) -> NaratamaResult<()> {
        let already: Option<i64> = conn
            .query_row(
                "SELECT initialized FROM finance_settings WHERE id = 1",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(db_err)?;
        if already == Some(1) {
            return Ok(());
        }

        let existing_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM finance_ledger WHERE ref_type = 'opening_balance'",
                [],
                |r| r.get(0),
            )
            .map_err(db_err)?;

        let date_str = opening_date.format("%Y-%m-%d").to_string();
        let tx = conn.transaction().map_err(db_err)?;
        if existing_count == 0 {
            tx.execute(
                "INSERT INTO finance_ledger (
                    team_id, season_id, entry_date, period, category,
                    subcategory, amount, ref_type, ref_id, description, created_at
                )
                SELECT team_id, ?1, ?2, 'board', 'board_funding',
                       'opening_balance', COALESCE(budget, 0), 'opening_balance',
                       NULL, 'Opening club treasury balance', ?2
                FROM teams",
                params![season_id.0, date_str],
            )
            .map_err(db_err)?;
        }
        tx.execute(
            "INSERT INTO finance_settings (
                id, mode, completed_seasons, initialized, opening_balance_date, updated_at, mode_locked
            ) VALUES (1, 'off', 0, 1, ?1, ?1, 0)
            ON CONFLICT(id) DO UPDATE SET
                initialized = 1,
                opening_balance_date = COALESCE(finance_settings.opening_balance_date, excluded.opening_balance_date),
                updated_at = excluded.updated_at",
            params![date_str],
        )
        .map_err(db_err)?;
        tx.commit().map_err(db_err)?;
        Ok(())
    }

    /// Reconciles a club's opening balance entry after custom-club creation before season start.
    pub fn set_initial_club_balance_tx(
        tx: &Transaction<'_>,
        team_id: TeamId,
        season_id: SeasonId,
        amount: Money,
        date: NaiveDate,
    ) -> NaratamaResult<()> {
        tx.execute(
            "DELETE FROM finance_ledger WHERE team_id = ?1",
            params![team_id.0],
        )
        .map_err(db_err)?;
        tx.execute(
            "UPDATE teams SET budget = 0 WHERE team_id = ?1",
            params![team_id.0],
        )
        .map_err(db_err)?;
        Self::record_tx(
            tx,
            &NewLedgerEntry {
                team_id,
                season_id,
                entry_date: date,
                period: LedgerPeriod::Board,
                category: LedgerCategory::BoardFunding,
                subcategory: "opening_balance",
                amount,
                ref_type: "opening_balance",
                ref_id: None,
                description: "Custom club starting capital",
            },
        )?;
        Ok(())
    }

    /// Bug #2 fix: uses `ON CONFLICT DO UPDATE` rather than `INSERT OR REPLACE` so partial updates never clear other columns.
    #[allow(clippy::too_many_arguments)]
    pub fn upsert_club_state_tx(
        tx: &Transaction<'_>,
        team_id: TeamId,
        consecutive_breaches: Option<i32>,
        distress: Option<bool>,
        embargo_until_season: Option<i32>,
        last_injection_season: Option<i32>,
        last_ffp_level: Option<&str>,
        updated_at: NaiveDate,
    ) -> NaratamaResult<()> {
        let date_str = updated_at.format("%Y-%m-%d").to_string();
        let distress_val = distress.map(i32::from);
        tx.execute(
            "INSERT INTO finance_club_state (
                team_id, consecutive_breaches, distress, embargo_until_season,
                last_injection_season, last_ffp_level, updated_at
            ) VALUES (?1, COALESCE(?2, 0), COALESCE(?3, 0), COALESCE(?4, 0), ?5, ?6, ?7)
            ON CONFLICT(team_id) DO UPDATE SET
                consecutive_breaches = COALESCE(?2, finance_club_state.consecutive_breaches),
                distress = COALESCE(?3, finance_club_state.distress),
                embargo_until_season = COALESCE(?4, finance_club_state.embargo_until_season),
                last_injection_season = COALESCE(?5, finance_club_state.last_injection_season),
                last_ffp_level = COALESCE(?6, finance_club_state.last_ffp_level),
                updated_at = ?7",
            params![
                team_id.0,
                consecutive_breaches,
                distress_val,
                embargo_until_season,
                last_injection_season,
                last_ffp_level,
                date_str,
            ],
        )
        .map_err(db_err)?;
        Ok(())
    }

    /// Gives every registered club in the red one emergency board injection per season.
    pub fn emergency_injections(
        conn: &mut Connection,
        season_id: SeasonId,
        date: NaiveDate,
    ) -> NaratamaResult<Vec<(TeamId, Money)>> {
        let mut stmt = conn
            .prepare(
                "SELECT t.team_id, t.budget
                 FROM teams t
                 JOIN season_teams st ON st.team_id = t.team_id
                 LEFT JOIN finance_club_state fcs ON fcs.team_id = t.team_id
                 WHERE st.season_id = ?1
                   AND COALESCE(t.budget, 0) < 0
                   AND COALESCE(fcs.last_injection_season, -1) != ?1
                 GROUP BY t.team_id",
            )
            .map_err(db_err)?;
        let rows: Vec<(TeamId, i64)> = stmt
            .query_map(params![season_id.0], |r| Ok((TeamId(r.get(0)?), r.get(1)?)))
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        drop(stmt);

        let mut injected = Vec::with_capacity(rows.len());
        let tx = conn.transaction().map_err(db_err)?;
        for (team_id, budget) in rows {
            let bailout = -budget;
            Self::record_tx(
                &tx,
                &NewLedgerEntry {
                    team_id,
                    season_id,
                    entry_date: date,
                    period: LedgerPeriod::Board,
                    category: LedgerCategory::BoardFunding,
                    subcategory: "emergency_injection",
                    amount: bailout,
                    ref_type: "emergency_injection",
                    ref_id: Some(season_id.0),
                    description: "Emergency board deficit injection",
                },
            )?;
            Self::upsert_club_state_tx(
                &tx,
                team_id,
                None,
                Some(true),
                None,
                Some(i32::try_from(season_id.0).unwrap_or(i32::MAX)),
                None,
                date,
            )?;
            injected.push((team_id, bailout));
        }

        // Flag any club that is still negative (already had its injection this season).
        let mut neg_stmt = tx
            .prepare(
                "SELECT t.team_id
                 FROM teams t
                 JOIN season_teams st ON st.team_id = t.team_id
                 WHERE st.season_id = ?1 AND COALESCE(t.budget, 0) < 0
                 GROUP BY t.team_id",
            )
            .map_err(db_err)?;
        let still_neg: Vec<TeamId> = neg_stmt
            .query_map(params![season_id.0], |r| Ok(TeamId(r.get(0)?)))
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        drop(neg_stmt);

        for team_id in still_neg {
            Self::upsert_club_state_tx(&tx, team_id, None, Some(true), None, None, None, date)?;
        }
        tx.commit().map_err(db_err)?;
        Ok(injected)
    }

    pub fn club_state(conn: &Connection, team_id: TeamId) -> NaratamaResult<FinanceClubState> {
        let row = conn
            .query_row(
                "SELECT consecutive_breaches, distress, embargo_until_season,
                        last_injection_season, last_ffp_level
                 FROM finance_club_state WHERE team_id = ?1",
                params![team_id.0],
                |r| {
                    Ok(FinanceClubState {
                        team_id,
                        consecutive_breaches: r.get::<_, Option<i32>>(0)?.unwrap_or(0),
                        distress: r.get::<_, Option<i32>>(1)?.unwrap_or(0) != 0,
                        embargo_until_season: r.get(2)?,
                        last_injection_season: r.get(3)?,
                        last_ffp_level: r.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(db_err)?;
        Ok(row.unwrap_or(FinanceClubState {
            team_id,
            consecutive_breaches: 0,
            distress: false,
            embargo_until_season: None,
            last_injection_season: None,
            last_ffp_level: None,
        }))
    }

    pub fn settings(conn: &Connection) -> NaratamaResult<FinanceSettingsRow> {
        let row = conn
            .query_row(
                "SELECT COALESCE(mode, 'off'), COALESCE(completed_seasons, 0),
                        COALESCE(initialized, 0), opening_balance_date,
                        COALESCE(mode_locked, 0), last_completed_season
                 FROM finance_settings WHERE id = 1",
                [],
                |r| {
                    let mode_str: String = r.get(0)?;
                    let mode = match mode_str.as_str() {
                        "relaxed" => FfpEnforcement::Relaxed,
                        "standard" => FfpEnforcement::Standard,
                        "strict" => FfpEnforcement::Strict,
                        _ => FfpEnforcement::Off,
                    };
                    Ok(FinanceSettingsRow {
                        mode,
                        completed_seasons: r.get(1)?,
                        initialized: r.get::<_, i32>(2)? != 0,
                        opening_balance_date: parse_sqlite_date(r.get(3)?),
                        mode_locked: r.get::<_, i32>(4)? != 0,
                        last_completed_season: r.get(5)?,
                    })
                },
            )
            .optional()
            .map_err(db_err)?;
        Ok(row.unwrap_or(FinanceSettingsRow {
            mode: FfpEnforcement::Off,
            completed_seasons: 0,
            initialized: false,
            opening_balance_date: None,
            mode_locked: false,
            last_completed_season: None,
        }))
    }

    pub fn complete_season(
        conn: &mut Connection,
        season_id: SeasonId,
        date: NaiveDate,
    ) -> NaratamaResult<()> {
        let date_str = date.format("%Y-%m-%d").to_string();
        conn.execute(
            "INSERT INTO finance_settings (id, mode, completed_seasons, initialized, updated_at, last_completed_season)
             VALUES (1, 'off', 1, 1, ?1, ?2)
             ON CONFLICT(id) DO UPDATE SET
                completed_seasons = CASE
                    WHEN COALESCE(finance_settings.last_completed_season, -1) = ?2 THEN finance_settings.completed_seasons
                    ELSE COALESCE(finance_settings.completed_seasons, 0) + 1
                END,
                last_completed_season = ?2,
                updated_at = ?1",
            params![date_str, season_id.0],
        )
        .map_err(db_err)?;
        Ok(())
    }

    pub fn award_competition_prize(
        conn: &mut Connection,
        season_id: SeasonId,
        tournament_id: TournamentId,
        team_id: TeamId,
        amount: Money,
        reason: &str,
        date: NaiveDate,
    ) -> NaratamaResult<bool> {
        if amount <= 0 {
            return Ok(false);
        }
        let date_str = date.format("%Y-%m-%d").to_string();
        let tx = conn.transaction().map_err(db_err)?;
        let inserted = tx
            .execute(
                "INSERT OR IGNORE INTO competition_prize_awards (
                    season_id, tournament_id, team_id, amount, reason, created_date
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    season_id.0,
                    tournament_id.0,
                    team_id.0,
                    amount,
                    reason,
                    date_str
                ],
            )
            .map_err(db_err)?;
        if inserted > 0 {
            Self::record_tx(
                &tx,
                &NewLedgerEntry {
                    team_id,
                    season_id,
                    entry_date: date,
                    period: LedgerPeriod::Annual,
                    category: LedgerCategory::PrizeMoney,
                    subcategory: "competition_prize",
                    amount,
                    ref_type: "competition_prize",
                    ref_id: Some(tournament_id.0),
                    description: reason,
                },
            )?;
        }
        tx.commit().map_err(db_err)?;
        Ok(inserted > 0)
    }

    pub fn has_ledger_ref(
        conn: &Connection,
        team_id: TeamId,
        season_id: SeasonId,
        ref_type: &str,
        subcategory: &str,
    ) -> NaratamaResult<bool> {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM finance_ledger
                 WHERE team_id = ?1 AND season_id = ?2 AND ref_type = ?3 AND subcategory = ?4",
                params![team_id.0, season_id.0, ref_type, subcategory],
                |r| r.get(0),
            )
            .map_err(db_err)?;
        Ok(count > 0)
    }

    pub fn team_season_cash_summary(
        conn: &Connection,
        team_id: TeamId,
        season_id: SeasonId,
    ) -> NaratamaResult<Vec<(String, i64)>> {
        let mut stmt = conn
            .prepare(
                "SELECT COALESCE(category, 'other'), COALESCE(SUM(amount), 0)
                 FROM finance_ledger
                 WHERE team_id = ?1 AND season_id = ?2
                   AND COALESCE(ref_type, '') != 'opening_balance'
                 GROUP BY category",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![team_id.0, season_id.0], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        Ok(rows)
    }

    pub fn season_wage_and_amortization(
        conn: &Connection,
        team_id: TeamId,
        season_id: SeasonId,
    ) -> NaratamaResult<(Money, Money)> {
        let wages: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(-amount), 0) FROM finance_ledger
                 WHERE team_id = ?1 AND season_id = ?2 AND category = 'player_wages'",
                params![team_id.0, season_id.0],
                |r| r.get(0),
            )
            .map_err(db_err)?;
        let amort: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(annual_amortization), 0) FROM player_registrations
                 WHERE team_id = ?1 AND status = 1",
                params![team_id.0],
                |r| r.get(0),
            )
            .map_err(db_err)?;
        Ok((wages, amort))
    }

    /// Audits `teams.budget == COALESCE(SUM(finance_ledger.amount), 0)` for every club in DB.
    /// Returns any mismatched `(team_id, budget, ledger_sum)` triples. Empty = 100% healthy.
    pub fn verify_ledger_invariant(
        conn: &Connection,
    ) -> NaratamaResult<Vec<(TeamId, Money, Money)>> {
        let mut stmt = conn
            .prepare(
                "SELECT t.team_id,
                        COALESCE(t.budget, 0) AS budget,
                        COALESCE((SELECT SUM(fl.amount) FROM finance_ledger fl WHERE fl.team_id = t.team_id), 0) AS ledger_sum
                 FROM teams t
                 WHERE COALESCE(t.budget, 0) != COALESCE((SELECT SUM(fl.amount) FROM finance_ledger fl WHERE fl.team_id = t.team_id), 0)",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map([], |r| Ok((TeamId(r.get(0)?), r.get(1)?, r.get(2)?)))
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        Ok(rows)
    }

    pub fn active_sponsorships_for_team(
        conn: &Connection,
        team_id: TeamId,
        season_id: SeasonId,
    ) -> NaratamaResult<Vec<SponsorshipRow>> {
        let mut stmt = conn
            .prepare(
                "SELECT s.sponsorship_id, s.type, s.team_id, s.sponsor_id,
                        COALESCE(sp.name, 'Sponsor') AS sponsor_name,
                        s.tournament_id, s.season_id,
                        COALESCE(s.is_priority, 0), COALESCE(s.is_unique_prize, 0),
                        COALESCE(s.base_reward, 0), COALESCE(s.target, 0),
                        COALESCE(s.progress, 0), COALESCE(s.success, 0),
                        COALESCE(s.status, 1), s.extra_data
                 FROM sponsorships s
                 LEFT JOIN sponsors sp ON sp.sponsor_id = s.sponsor_id
                 WHERE s.team_id = ?1 AND s.season_id = ?2 AND COALESCE(s.status, 1) = 1
                 ORDER BY s.sponsorship_id ASC",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![team_id.0, season_id.0], |r| {
                Ok(SponsorshipRow {
                    sponsorship_id: r.get(0)?,
                    deal_type: r.get(1)?,
                    team_id: TeamId(r.get(2)?),
                    sponsor_id: r.get(3)?,
                    sponsor_name: r.get(4)?,
                    tournament_id: r.get::<_, Option<i64>>(5)?.map(TournamentId),
                    season_id: SeasonId(r.get(6)?),
                    is_priority: r.get::<_, i32>(7)? != 0,
                    is_unique_prize: r.get::<_, i32>(8)? != 0,
                    base_reward: r.get(9)?,
                    target: r.get(10)?,
                    progress: r.get(11)?,
                    success: r.get::<_, i32>(12)? != 0,
                    status: r.get(13)?,
                    extra_data: r.get(14)?,
                })
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        Ok(rows)
    }

    pub fn sponsors_catalog(conn: &Connection) -> NaratamaResult<Vec<(i64, String, i32)>> {
        let mut stmt = conn
            .prepare("SELECT sponsor_id, name, COALESCE(level, 1) FROM sponsors ORDER BY level DESC, sponsor_id ASC")
            .map_err(db_err)?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        Ok(rows)
    }
}
