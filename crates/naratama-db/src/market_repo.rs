use chrono::{Datelike, Duration, NaiveDate};
use naratama_model::{
    Confederation, CountryId, LedgerCategory, LedgerPeriod, MarketStatus, Money, NaratamaError,
    NaratamaResult, PlayerId, PlayerStatus, PreferredFoot, SeasonId, SquadStatus, TeamId,
    TransferOfferStatus,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::club_repo::ClubRepo;
use crate::finance_repo::{FinanceRepo, NewLedgerEntry};
use crate::{db_err, parse_sqlite_date};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferWindowRow {
    pub window_id: i64,
    pub season_id: SeasonId,
    pub country_id: CountryId,
    pub tournament_id: i64,
    pub window_type: String,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub is_open: bool,
    pub allow_free_agents_outside: bool,
}

impl TransferWindowRow {
    #[must_use]
    pub fn contains_date(&self, date: NaiveDate) -> bool {
        date >= self.start_date && date <= self.end_date
    }

    #[must_use]
    pub fn is_deadline_day(&self, date: NaiveDate) -> bool {
        date == self.end_date
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferOfferRow {
    pub offer_id: i64,
    pub player_id: PlayerId,
    pub player_name: String,
    pub from_team_id: TeamId,
    pub from_team_name: String,
    pub to_team_id: TeamId,
    pub to_team_name: String,
    pub offer_amount: Money,
    pub player_value: Money,
    pub status: i64,
    pub counter_amount: Option<Money>,
    pub proposed_salary: Option<Money>,
    pub proposed_duration_years: Option<i32>,
    pub is_loan: bool,
    pub loan_seasons: i32,
    pub effective_date: Option<NaiveDate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoutRow {
    pub scout_id: i64,
    pub team_id: TeamId,
    pub name: String,
    pub stars: i32,
    pub region: String,
    pub salary: Money,
    pub status: String,
    pub return_date: Option<NaiveDate>,
    pub last_report: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoutReportRow {
    pub report_id: i64,
    pub team_id: TeamId,
    pub scout_id: i64,
    pub scout_name: String,
    pub region: String,
    pub date: Option<NaiveDate>,
    pub player_name: String,
    pub age: i32,
    pub position_id: i32,
    pub power: i32,
    pub potential: i32,
    pub value: Money,
    pub country_id: CountryId,
    pub signed: bool,
}

/// Transactional transfer windows, offers, permanent transfers, loans, sell-on clauses, and scouting repository.
pub struct MarketRepo;

impl MarketRepo {
    pub fn ensure_windows_for_season(
        conn: &mut Connection,
        season_id: SeasonId,
        start_year: i32,
    ) -> NaratamaResult<()> {
        let mut stmt = conn
            .prepare("SELECT country_id, COALESCE(confederation, 3) FROM countries")
            .map_err(db_err)?;
        let countries: Vec<(i64, i64)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        drop(stmt);

        let tx = conn.transaction().map_err(db_err)?;
        for (country_id, conf_code) in countries {
            let (s_start, s_end, w_start, w_end) = match Confederation::from_code(conf_code) {
                Confederation::Conmebol => (
                    format!("{start_year}-06-15"),
                    format!("{start_year}-09-02"),
                    format!("{}-01-10", start_year + 1),
                    format!("{}-03-07", start_year + 1),
                ),
                Confederation::Afc => (
                    format!("{start_year}-06-20"),
                    format!("{start_year}-09-10"),
                    format!("{}-01-05", start_year + 1),
                    format!("{}-02-15", start_year + 1),
                ),
                _ => (
                    format!("{start_year}-07-01"),
                    format!("{start_year}-09-01"),
                    format!("{}-01-01", start_year + 1),
                    format!("{}-02-03", start_year + 1),
                ),
            };
            tx.execute(
                "INSERT OR IGNORE INTO transfer_windows (
                    season_id, country_id, tournament_id, window_type, start_date, end_date, is_open, allow_free_agents_outside
                 ) VALUES (?1, ?2, 0, 'summer', ?3, ?4, 0, 1)",
                params![season_id.0, country_id, s_start, s_end],
            )
            .map_err(db_err)?;
            tx.execute(
                "INSERT OR IGNORE INTO transfer_windows (
                    season_id, country_id, tournament_id, window_type, start_date, end_date, is_open, allow_free_agents_outside
                 ) VALUES (?1, ?2, 0, 'winter', ?3, ?4, 0, 1)",
                params![season_id.0, country_id, w_start, w_end],
            )
            .map_err(db_err)?;
        }
        tx.commit().map_err(db_err)?;
        Ok(())
    }

    pub fn active_window_for_team(
        conn: &Connection,
        season_id: SeasonId,
        team_id: TeamId,
        date: NaiveDate,
    ) -> NaratamaResult<Option<TransferWindowRow>> {
        let country_id: i64 = conn
            .query_row(
                "SELECT country_id FROM teams WHERE team_id = ?1",
                params![team_id.0],
                |r| r.get(0),
            )
            .optional()
            .map_err(db_err)?
            .unwrap_or(1);
        let date_str = date.format("%Y-%m-%d").to_string();
        conn.query_row(
            "SELECT window_id, season_id, country_id, COALESCE(tournament_id, 0),
                    window_type, start_date, end_date, COALESCE(is_open, 0),
                    COALESCE(allow_free_agents_outside, 1)
             FROM transfer_windows
             WHERE season_id = ?1 AND country_id = ?2 AND start_date <= ?3 AND end_date >= ?3
             ORDER BY start_date ASC
             LIMIT 1",
            params![season_id.0, country_id, date_str],
            |r| {
                let s = parse_sqlite_date(Some(r.get::<_, String>(5)?)).unwrap_or(NaiveDate::MIN);
                let e = parse_sqlite_date(Some(r.get::<_, String>(6)?)).unwrap_or(NaiveDate::MAX);
                Ok(TransferWindowRow {
                    window_id: r.get(0)?,
                    season_id: SeasonId(r.get(1)?),
                    country_id: CountryId(r.get(2)?),
                    tournament_id: r.get(3)?,
                    window_type: r.get(4)?,
                    start_date: s,
                    end_date: e,
                    is_open: true,
                    allow_free_agents_outside: r.get::<_, i32>(8)? != 0,
                })
            },
        )
        .optional()
        .map_err(db_err)
    }

    pub fn next_window_start_for_team(
        conn: &Connection,
        season_id: SeasonId,
        team_id: TeamId,
        after_date: NaiveDate,
    ) -> NaratamaResult<NaiveDate> {
        let country_id: i64 = conn
            .query_row(
                "SELECT country_id FROM teams WHERE team_id = ?1",
                params![team_id.0],
                |r| r.get(0),
            )
            .optional()
            .map_err(db_err)?
            .unwrap_or(1);
        let date_str = after_date.format("%Y-%m-%d").to_string();
        let next: Option<String> = conn
            .query_row(
                "SELECT start_date FROM transfer_windows
                 WHERE season_id = ?1 AND country_id = ?2 AND start_date > ?3
                 ORDER BY start_date ASC LIMIT 1",
                params![season_id.0, country_id, date_str],
                |r| r.get(0),
            )
            .optional()
            .map_err(db_err)?;
        Ok(parse_sqlite_date(next).unwrap_or_else(|| {
            NaiveDate::from_ymd_opt(after_date.year() + 1, 7, 1)
                .unwrap_or(after_date + Duration::days(90))
        }))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_offer(
        conn: &Connection,
        player_id: PlayerId,
        from_team_id: TeamId,
        to_team_id: TeamId,
        offer_amount: Money,
        player_value: Money,
        is_loan: bool,
        loan_seasons: i32,
        date: NaiveDate,
        effective_date: Option<NaiveDate>,
        status: TransferOfferStatus,
    ) -> NaratamaResult<i64> {
        let d = date.format("%Y-%m-%d").to_string();
        let eff = effective_date.map(|v| v.format("%Y-%m-%d").to_string());
        conn.execute(
            "INSERT INTO transfer_offers (
                player_id, from_team_id, to_team_id, offer_amount, player_value,
                status, created_date, updated_date, is_loan, loan_seasons,
                agreement_date, effective_date, eligible_date, transfer_type
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, ?8, ?9, ?7, ?10, ?10, ?11)",
            params![
                player_id.0,
                from_team_id.0,
                to_team_id.0,
                offer_amount,
                player_value,
                status as i64,
                d,
                i32::from(is_loan),
                loan_seasons,
                eff,
                if is_loan { "loan" } else { "permanent" },
            ],
        )
        .map_err(db_err)?;
        Ok(conn.last_insert_rowid())
    }

    pub fn update_offer_status(
        conn: &Connection,
        offer_id: i64,
        status: TransferOfferStatus,
        date: NaiveDate,
        counter_amount: Option<Money>,
        cancellation_reason: Option<&str>,
    ) -> NaratamaResult<()> {
        let d = date.format("%Y-%m-%d").to_string();
        conn.execute(
            "UPDATE transfer_offers SET
                status = ?1,
                updated_date = ?2,
                counter_amount = COALESCE(?3, counter_amount),
                cancellation_reason = COALESCE(?4, cancellation_reason)
             WHERE offer_id = ?5",
            params![
                status as i64,
                d,
                counter_amount,
                cancellation_reason,
                offer_id
            ],
        )
        .map_err(db_err)?;
        Ok(())
    }

    /// Atomic permanent transfer with Bug #1 fix (explicit `seller_team_id` / `buyer_team_id` + seller squad > 19 check),
    /// FFP registration amortization, agent fee, and active 15% sell-on clause payout.
    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    pub fn complete_transfer(
        conn: &mut Connection,
        season_id: SeasonId,
        player_id: PlayerId,
        seller_team_id: TeamId,
        buyer_team_id: TeamId,
        fee: Money,
        agent_fee: Money,
        proposed_salary: Option<Money>,
        contract_years: i32,
        date: NaiveDate,
        allow_small_seller_squad: bool,
    ) -> NaratamaResult<()> {
        if seller_team_id == buyer_team_id {
            return Err(NaratamaError::Validation(
                "Buyer and seller clubs must be distinct".into(),
            ));
        }

        let tx = conn.transaction().map_err(db_err)?;

        // Check active status and current club
        let status_id: Option<i64> = tx
            .query_row(
                "SELECT status_id FROM players WHERE player_id = ?1",
                params![player_id.0],
                |r| r.get(0),
            )
            .optional()
            .map_err(db_err)?;
        if status_id != Some(PlayerStatus::Active as i64) {
            return Err(NaratamaError::InvalidState(
                "Player is no longer active".into(),
            ));
        }

        if seller_team_id.0 > 0 && !allow_small_seller_squad {
            let seller_count: i64 = tx
                .query_row(
                    "SELECT COUNT(*) FROM team_players tp
                     JOIN players p ON p.player_id = tp.player_id
                     WHERE tp.team_id = ?1 AND COALESCE(p.status_id, 1) = 1",
                    params![seller_team_id.0],
                    |r| r.get(0),
                )
                .map_err(db_err)?;
            if seller_count <= 19 {
                return Err(NaratamaError::Validation(
                    "Selling club has reached the minimum squad limit of 19 players".into(),
                ));
            }
        }

        let buyer_count: i64 = tx
            .query_row(
                "SELECT COUNT(*) FROM team_players tp
                 JOIN players p ON p.player_id = tp.player_id
                 WHERE tp.team_id = ?1 AND COALESCE(p.status_id, 1) = 1",
                params![buyer_team_id.0],
                |r| r.get(0),
            )
            .map_err(db_err)?;
        if buyer_count >= 30 {
            return Err(NaratamaError::Validation(
                "Buying club squad is full (30 players)".into(),
            ));
        }

        let buyer_budget: i64 = tx
            .query_row(
                "SELECT COALESCE(budget, 0) FROM teams WHERE team_id = ?1",
                params![buyer_team_id.0],
                |r| r.get(0),
            )
            .optional()
            .map_err(db_err)?
            .unwrap_or(0);
        let total_cost = fee + agent_fee;
        if buyer_budget < total_cost {
            return Err(NaratamaError::Validation(
                "Buying club lacks budget for fee + agent commission".into(),
            ));
        }

        // Move ownership
        let updated_tp = tx
            .execute(
                "UPDATE team_players SET team_id = ?1 WHERE player_id = ?2",
                params![buyer_team_id.0, player_id.0],
            )
            .map_err(db_err)?;
        if updated_tp == 0 {
            tx.execute(
                "INSERT INTO team_players (team_id, player_id) VALUES (?1, ?2)",
                params![buyer_team_id.0, player_id.0],
            )
            .map_err(db_err)?;
        }

        tx.execute(
            "DELETE FROM team_lineups WHERE player_id = ?1 AND team_id = ?2",
            params![player_id.0, seller_team_id.0],
        )
        .map_err(db_err)?;

        tx.execute(
            "UPDATE players SET
                market_status = ?1,
                squad_status = ?2,
                is_captain = 0,
                salary = COALESCE(?3, salary)
             WHERE player_id = ?4",
            params![
                MarketStatus::Active as i64,
                SquadStatus::Reserve as i64,
                proposed_salary,
                player_id.0
            ],
        )
        .map_err(db_err)?;

        let date_str = date.format("%Y-%m-%d").to_string();
        let years = contract_years.clamp(1, 5);
        let end_date_str = NaiveDate::from_ymd_opt(date.year() + years, 6, 30)
            .unwrap_or(date)
            .format("%Y-%m-%d")
            .to_string();
        let effective_salary: i64 = proposed_salary.unwrap_or_else(|| {
            tx.query_row(
                "SELECT COALESCE(salary, 5000) FROM players WHERE player_id = ?1",
                params![player_id.0],
                |r| r.get(0),
            )
            .unwrap_or(5000)
        });
        tx.execute(
            "UPDATE contracts SET status = 0 WHERE player_id = ?1 AND status = 1",
            params![player_id.0],
        )
        .map_err(db_err)?;
        tx.execute(
            "INSERT INTO contracts (player_id, team_id, start_date, end_date, status, salary)
             VALUES (?1, ?2, ?3, ?4, 1, ?5)",
            params![
                player_id.0,
                buyer_team_id.0,
                date_str,
                end_date_str,
                effective_salary
            ],
        )
        .map_err(db_err)?;

        // Deactivate old registration and record amortized registration for FFP
        tx.execute(
            "UPDATE player_registrations SET status = 0 WHERE player_id = ?1 AND status = 1",
            params![player_id.0],
        )
        .map_err(db_err)?;
        let amort = (fee + agent_fee) / i64::from(years);
        tx.execute(
            "INSERT INTO player_registrations (
                player_id, team_id, from_team_id, transfer_fee, agent_fee,
                sign_on_bonus, start_date, end_date, annual_amortization, status
             ) VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, ?7, ?8, 1)",
            params![
                player_id.0,
                buyer_team_id.0,
                seller_team_id.0,
                fee,
                agent_fee,
                date_str,
                end_date_str,
                amort,
            ],
        )
        .map_err(db_err)?;

        tx.execute(
            "INSERT INTO player_transfers (player_id, from_team_id, to_team_id, type, value, date)
             VALUES (?1, ?2, ?3, 'permanent', ?4, ?5)",
            params![
                player_id.0,
                seller_team_id.0,
                buyer_team_id.0,
                fee,
                date_str
            ],
        )
        .map_err(db_err)?;

        // Ledger writes
        if fee > 0 {
            FinanceRepo::record_tx(
                &tx,
                &NewLedgerEntry {
                    team_id: buyer_team_id,
                    season_id,
                    entry_date: date,
                    period: LedgerPeriod::Transfer,
                    category: LedgerCategory::TransferFees,
                    subcategory: "transfer_fee",
                    amount: -fee,
                    ref_type: "player_transfer",
                    ref_id: Some(player_id.0),
                    description: "Transfer fee paid",
                },
            )?;
        }
        if agent_fee > 0 {
            FinanceRepo::record_tx(
                &tx,
                &NewLedgerEntry {
                    team_id: buyer_team_id,
                    season_id,
                    entry_date: date,
                    period: LedgerPeriod::Transfer,
                    category: LedgerCategory::AgentFees,
                    subcategory: "agent_fee",
                    amount: -agent_fee,
                    ref_type: "player_transfer",
                    ref_id: Some(player_id.0),
                    description: "Agent fee paid",
                },
            )?;
        }
        if seller_team_id.0 > 0 && fee > 0 {
            FinanceRepo::record_tx(
                &tx,
                &NewLedgerEntry {
                    team_id: seller_team_id,
                    season_id,
                    entry_date: date,
                    period: LedgerPeriod::Transfer,
                    category: LedgerCategory::PlayerSales,
                    subcategory: "transfer_sale",
                    amount: fee,
                    ref_type: "player_transfer",
                    ref_id: Some(player_id.0),
                    description: "Player transfer fee received",
                },
            )?;

            // Active sell-on clause settlement (atomic debit from seller, credit to beneficiary)
            let mut clause_stmt = tx
                .prepare(
                    "SELECT clause_id, beneficiary_team_id, pct
                     FROM transfer_clauses
                     WHERE player_id = ?1 AND type = 'sell_on' AND active = 1",
                )
                .map_err(db_err)?;
            let clauses: Vec<(i64, i64, f64)> = clause_stmt
                .query_map(params![player_id.0], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?))
                })
                .map_err(db_err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(db_err)?;
            drop(clause_stmt);

            for (clause_id, beneficiary_raw, pct) in clauses {
                #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
                let cut = ((fee as f64) * pct).round() as i64;
                let beneficiary = TeamId(beneficiary_raw);
                if cut > 0 && beneficiary != seller_team_id {
                    FinanceRepo::record_tx(
                        &tx,
                        &NewLedgerEntry {
                            team_id: seller_team_id,
                            season_id,
                            entry_date: date,
                            period: LedgerPeriod::Transfer,
                            category: LedgerCategory::SellOnPaid,
                            subcategory: "sell_on_clause",
                            amount: -cut,
                            ref_type: "transfer_clause",
                            ref_id: Some(clause_id),
                            description: "Sell-on clause paid",
                        },
                    )?;
                    FinanceRepo::record_tx(
                        &tx,
                        &NewLedgerEntry {
                            team_id: beneficiary,
                            season_id,
                            entry_date: date,
                            period: LedgerPeriod::Transfer,
                            category: LedgerCategory::SellOnReceived,
                            subcategory: "sell_on_clause",
                            amount: cut,
                            ref_type: "transfer_clause",
                            ref_id: Some(clause_id),
                            description: "Sell-on clause income",
                        },
                    )?;
                }
                tx.execute(
                    "UPDATE transfer_clauses SET active = 0, amount = ?1, settled_date = ?2
                     WHERE clause_id = ?3",
                    params![cut, date_str, clause_id],
                )
                .map_err(db_err)?;
            }
        }

        if seller_team_id.0 > 0 {
            ClubRepo::refresh_team_power_tx(&tx, seller_team_id)?;
        }
        ClubRepo::refresh_team_power_tx(&tx, buyer_team_id)?;
        tx.commit().map_err(db_err)?;
        Ok(())
    }

    /// Bug #11 fix: validates borrower budget >= fee AND borrower squad < 30 inside transaction.
    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    pub fn complete_loan(
        conn: &mut Connection,
        season_id: SeasonId,
        player_id: PlayerId,
        parent_team_id: TeamId,
        borrower_team_id: TeamId,
        loan_fee: Money,
        loan_seasons: i32,
        date: NaiveDate,
    ) -> NaratamaResult<()> {
        if parent_team_id == borrower_team_id {
            return Err(NaratamaError::Validation(
                "Loan parent and borrower teams cannot be identical".into(),
            ));
        }

        let tx = conn.transaction().map_err(db_err)?;
        let borrower_count: i64 = tx
            .query_row(
                "SELECT COUNT(*) FROM team_players tp
                 JOIN players p ON p.player_id = tp.player_id
                 WHERE tp.team_id = ?1 AND COALESCE(p.status_id, 1) = 1",
                params![borrower_team_id.0],
                |r| r.get(0),
            )
            .map_err(db_err)?;
        if borrower_count >= 30 {
            return Err(NaratamaError::Validation(
                "Borrowing club squad is full (30 players)".into(),
            ));
        }

        let parent_count: i64 = tx
            .query_row(
                "SELECT COUNT(*) FROM team_players tp
                 JOIN players p ON p.player_id = tp.player_id
                 WHERE tp.team_id = ?1 AND COALESCE(p.status_id, 1) = 1",
                params![parent_team_id.0],
                |r| r.get(0),
            )
            .map_err(db_err)?;
        if parent_count <= 19 {
            return Err(NaratamaError::Validation(
                "Parent club cannot loan below 19 active players".into(),
            ));
        }

        let borrower_budget: i64 = tx
            .query_row(
                "SELECT COALESCE(budget, 0) FROM teams WHERE team_id = ?1",
                params![borrower_team_id.0],
                |r| r.get(0),
            )
            .optional()
            .map_err(db_err)?
            .unwrap_or(0);
        if borrower_budget < loan_fee {
            return Err(NaratamaError::Validation(
                "Borrowing club budget insufficient for loan fee".into(),
            ));
        }

        let seasons = loan_seasons.clamp(1, 2);
        let date_from = date.format("%Y-%m-%d").to_string();
        let date_to = NaiveDate::from_ymd_opt(date.year() + seasons, 6, 30)
            .unwrap_or(date)
            .format("%Y-%m-%d")
            .to_string();

        tx.execute(
            "INSERT INTO team_loans (player_id, from_team_id, to_team_id, date_from, date_to, months, notified)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)",
            params![
                player_id.0,
                parent_team_id.0,
                borrower_team_id.0,
                date_from,
                date_to,
                seasons * 12
            ],
        )
        .map_err(db_err)?;

        tx.execute(
            "UPDATE team_players SET team_id = ?1 WHERE player_id = ?2",
            params![borrower_team_id.0, player_id.0],
        )
        .map_err(db_err)?;
        tx.execute(
            "DELETE FROM team_lineups WHERE player_id = ?1 AND team_id = ?2",
            params![player_id.0, parent_team_id.0],
        )
        .map_err(db_err)?;
        tx.execute(
            "UPDATE players SET market_status = ?1, squad_status = ?2 WHERE player_id = ?3",
            params![
                MarketStatus::OnLoan as i64,
                SquadStatus::Reserve as i64,
                player_id.0
            ],
        )
        .map_err(db_err)?;

        if loan_fee > 0 {
            FinanceRepo::record_tx(
                &tx,
                &NewLedgerEntry {
                    team_id: borrower_team_id,
                    season_id,
                    entry_date: date,
                    period: LedgerPeriod::Transfer,
                    category: LedgerCategory::LoanFeePaid,
                    subcategory: "loan_fee",
                    amount: -loan_fee,
                    ref_type: "loan",
                    ref_id: Some(player_id.0),
                    description: "Loan fee paid",
                },
            )?;
            FinanceRepo::record_tx(
                &tx,
                &NewLedgerEntry {
                    team_id: parent_team_id,
                    season_id,
                    entry_date: date,
                    period: LedgerPeriod::Transfer,
                    category: LedgerCategory::LoanFeeReceived,
                    subcategory: "loan_fee",
                    amount: loan_fee,
                    ref_type: "loan",
                    ref_id: Some(player_id.0),
                    description: "Loan fee received",
                },
            )?;
        }

        ClubRepo::refresh_team_power_tx(&tx, parent_team_id)?;
        ClubRepo::refresh_team_power_tx(&tx, borrower_team_id)?;
        tx.commit().map_err(db_err)?;
        Ok(())
    }

    pub fn recall_finished_loans(
        conn: &mut Connection,
        season_end_year: i32,
    ) -> NaratamaResult<Vec<(PlayerId, TeamId)>> {
        let mut stmt = conn
            .prepare(
                "SELECT loan_id, player_id, from_team_id
                 FROM team_loans
                 WHERE CAST(substr(date_to, 1, 4) AS INTEGER) <= ?1",
            )
            .map_err(db_err)?;
        let rows: Vec<(i64, i64, i64)> = stmt
            .query_map(params![season_end_year], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        drop(stmt);

        if rows.is_empty() {
            return Ok(Vec::new());
        }

        let tx = conn.transaction().map_err(db_err)?;
        let mut recalled = Vec::with_capacity(rows.len());
        for (loan_id, player_raw, parent_raw) in rows {
            tx.execute(
                "UPDATE team_players SET team_id = ?1 WHERE player_id = ?2",
                params![parent_raw, player_raw],
            )
            .map_err(db_err)?;
            tx.execute(
                "UPDATE players SET market_status = ?1, squad_status = ?2 WHERE player_id = ?3",
                params![
                    MarketStatus::Active as i64,
                    SquadStatus::Reserve as i64,
                    player_raw
                ],
            )
            .map_err(db_err)?;
            tx.execute(
                "DELETE FROM team_loans WHERE loan_id = ?1",
                params![loan_id],
            )
            .map_err(db_err)?;
            recalled.push((PlayerId(player_raw), TeamId(parent_raw)));
        }
        tx.commit().map_err(db_err)?;
        Ok(recalled)
    }

    pub fn due_scheduled_offers(
        conn: &Connection,
        date: NaiveDate,
    ) -> NaratamaResult<Vec<TransferOfferRow>> {
        let d = date.format("%Y-%m-%d").to_string();
        let mut stmt = conn
            .prepare(
                "SELECT o.offer_id, o.player_id, COALESCE(p.name, 'Player'),
                        o.from_team_id, COALESCE(ft.name, 'Club'),
                        o.to_team_id, COALESCE(tt.name, 'Club'),
                        o.offer_amount, o.player_value, o.status,
                        o.counter_amount, o.proposed_salary, o.proposed_duration_years,
                        COALESCE(o.is_loan, 0), COALESCE(o.loan_seasons, 1), o.effective_date
                 FROM transfer_offers o
                 LEFT JOIN players p ON p.player_id = o.player_id
                 LEFT JOIN teams ft ON ft.team_id = o.from_team_id
                 LEFT JOIN teams tt ON tt.team_id = o.to_team_id
                 WHERE o.status = ?1 AND o.effective_date IS NOT NULL AND o.effective_date <= ?2
                 ORDER BY o.effective_date ASC, o.offer_id ASC",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(
                params![TransferOfferStatus::AwaitingWindow as i64, d],
                |r| {
                    Ok(TransferOfferRow {
                        offer_id: r.get(0)?,
                        player_id: PlayerId(r.get(1)?),
                        player_name: r.get(2)?,
                        from_team_id: TeamId(r.get(3)?),
                        from_team_name: r.get(4)?,
                        to_team_id: TeamId(r.get(5)?),
                        to_team_name: r.get(6)?,
                        offer_amount: r.get(7)?,
                        player_value: r.get(8)?,
                        status: r.get(9)?,
                        counter_amount: r.get(10)?,
                        proposed_salary: r.get(11)?,
                        proposed_duration_years: r.get(12)?,
                        is_loan: r.get::<_, i32>(13)? != 0,
                        loan_seasons: r.get(14)?,
                        effective_date: parse_sqlite_date(r.get(15)?),
                    })
                },
            )
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        Ok(rows)
    }

    pub fn hired_scouts(conn: &Connection, team_id: TeamId) -> NaratamaResult<Vec<ScoutRow>> {
        let mut stmt = conn
            .prepare(
                "SELECT scout_id, team_id, name, COALESCE(stars, 2), COALESCE(region, 'Europe'),
                        COALESCE(salary, 5000), COALESCE(status, 'idle'), return_date, last_report
                 FROM scouts WHERE team_id = ?1 ORDER BY stars DESC, scout_id ASC",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![team_id.0], |r| {
                Ok(ScoutRow {
                    scout_id: r.get(0)?,
                    team_id: TeamId(r.get(1)?),
                    name: r.get(2)?,
                    stars: r.get(3)?,
                    region: r.get(4)?,
                    salary: r.get(5)?,
                    status: r.get(6)?,
                    return_date: parse_sqlite_date(r.get(7)?),
                    last_report: r.get(8)?,
                })
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        Ok(rows)
    }

    /// Bug #3 fix: looks up country IDs dynamically by confederation code instead of invalid hardcoded IDs.
    pub fn country_ids_for_region(
        conn: &Connection,
        region: &str,
    ) -> NaratamaResult<Vec<CountryId>> {
        let conf_code = match region {
            "South America" => Confederation::Conmebol as i64,
            "Africa" => Confederation::Caf as i64,
            "Asia" => Confederation::Afc as i64,
            "North America" => Confederation::Concacaf as i64,
            _ => Confederation::Uefa as i64,
        };
        let mut stmt = conn
            .prepare(
                "SELECT country_id FROM countries WHERE confederation = ?1 ORDER BY power DESC",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![conf_code], |r| Ok(CountryId(r.get(0)?)))
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        Ok(if rows.is_empty() {
            vec![CountryId(1)]
        } else {
            rows
        })
    }

    /// Bug #10 fix: signs wonderkid or academy prospect with < 30 squad limit check, contract, and `player_registration` row.
    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    pub fn sign_new_prospect_to_club(
        conn: &mut Connection,
        season_id: SeasonId,
        team_id: TeamId,
        name: &str,
        age: i32,
        position_id: i32,
        power: i32,
        potential: i32,
        stars: i32,
        foot: PreferredFoot,
        height: i32,
        country_id: CountryId,
        value: Money,
        salary: Money,
        signing_fee: Money,
        date: NaiveDate,
        scout_report_id: Option<i64>,
        academy_prospect_id: Option<i64>,
    ) -> NaratamaResult<PlayerId> {
        let tx = conn.transaction().map_err(db_err)?;
        let squad_count: i64 = tx
            .query_row(
                "SELECT COUNT(*) FROM team_players tp
                 JOIN players p ON p.player_id = tp.player_id
                 WHERE tp.team_id = ?1 AND COALESCE(p.status_id, 1) = 1",
                params![team_id.0],
                |r| r.get(0),
            )
            .map_err(db_err)?;
        if squad_count >= 30 {
            return Err(NaratamaError::Validation(
                "Club squad is full (maximum 30 players)".into(),
            ));
        }

        let budget: i64 = tx
            .query_row(
                "SELECT COALESCE(budget, 0) FROM teams WHERE team_id = ?1",
                params![team_id.0],
                |r| r.get(0),
            )
            .optional()
            .map_err(db_err)?
            .unwrap_or(0);
        if signing_fee > 0 && budget < signing_fee {
            return Err(NaratamaError::Validation(
                "Club lacks budget for prospect signing fee".into(),
            ));
        }

        let date_str = date.format("%Y-%m-%d").to_string();
        let end_date_str = NaiveDate::from_ymd_opt(date.year() + 4, 6, 30)
            .unwrap_or(date)
            .format("%Y-%m-%d")
            .to_string();
        let foot_code = foot.to_db();
        tx.execute(
            "INSERT INTO players (
                name, age, height, stars, value, salary, profile, experience, fitness,
                status_id, power, potential, market_status, country_id, position_id,
                squad_status, is_captain, shirt_number, points, skill_a, skill_b
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 10, 100, 1, ?8, ?9, 1, ?10, ?11, 0, 0, 35, 0, 0, 0)",
            params![
                name,
                age,
                height,
                stars,
                value,
                salary,
                foot_code,
                power,
                potential,
                country_id.0,
                position_id,
            ],
        )
        .map_err(db_err)?;
        let player_id = PlayerId(tx.last_insert_rowid());

        tx.execute(
            "INSERT INTO team_players (team_id, player_id) VALUES (?1, ?2)",
            params![team_id.0, player_id.0],
        )
        .map_err(db_err)?;
        tx.execute(
            "INSERT INTO contracts (player_id, team_id, start_date, end_date, status, salary)
             VALUES (?1, ?2, ?3, ?4, 1, ?5)",
            params![player_id.0, team_id.0, date_str, end_date_str, salary],
        )
        .map_err(db_err)?;
        tx.execute(
            "INSERT INTO player_registrations (
                player_id, team_id, from_team_id, transfer_fee, agent_fee, sign_on_bonus,
                start_date, end_date, annual_amortization, status
             ) VALUES (?1, ?2, 0, 0, 0, ?3, ?4, ?5, ?6, 1)",
            params![
                player_id.0,
                team_id.0,
                signing_fee,
                date_str,
                end_date_str,
                signing_fee / 4,
            ],
        )
        .map_err(db_err)?;

        if signing_fee > 0 {
            FinanceRepo::record_tx(
                &tx,
                &NewLedgerEntry {
                    team_id,
                    season_id,
                    entry_date: date,
                    period: LedgerPeriod::Transfer,
                    category: LedgerCategory::TransferFees,
                    subcategory: "prospect_signing",
                    amount: -signing_fee,
                    ref_type: "prospect_signing",
                    ref_id: Some(player_id.0),
                    description: "Youth or scouted prospect signing fee",
                },
            )?;
        }

        if let Some(rid) = scout_report_id {
            tx.execute(
                "UPDATE scout_reports SET signed = 1 WHERE report_id = ?1",
                params![rid],
            )
            .map_err(db_err)?;
        }
        if let Some(pid) = academy_prospect_id {
            tx.execute(
                "UPDATE academy_prospects SET signed = 1 WHERE prospect_id = ?1",
                params![pid],
            )
            .map_err(db_err)?;
        }

        ClubRepo::refresh_team_power_tx(&tx, team_id)?;
        tx.commit().map_err(db_err)?;
        Ok(player_id)
    }
}
