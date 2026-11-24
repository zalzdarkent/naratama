use chrono::{Datelike, Duration, NaiveDate};
use naratama_db::club_repo::ClubRepo;
use naratama_db::db_err;
use naratama_db::finance_repo::{FinanceRepo, NewLedgerEntry};
use naratama_db::market_repo::{MarketRepo, ScoutReportRow, ScoutRow, TransferOfferRow};
use naratama_model::{
    Confederation, LedgerCategory, LedgerPeriod, Money, NaratamaError, NaratamaResult, Player,
    PlayerId, PreferredFoot, SeasonId, TeamId, TransferOfferStatus,
};
use naratama_sim::{FinanceCalc, PlayerValuationInput, PlayerValuationService};
use rand::seq::SliceRandom;
use rand::Rng;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum OfferResponseAction {
    Accept,
    Reject,
    Counter(Money),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferExecutionOutcome {
    pub completed_immediately: bool,
    pub scheduled_date: Option<NaiveDate>,
    pub offer_id: i64,
    pub message: String,
}

/// Manages transfer offers, AI market activity, scheduled transfers, loans, and contract renewals.
pub struct TransferMarketService;

impl TransferMarketService {
    /// Submits a permanent transfer or loan offer; validates Bug #1 (seller squad > 19) upfront.
    #[allow(clippy::too_many_arguments)]
    pub fn make_transfer_offer(
        conn: &Connection,
        player_id: PlayerId,
        seller_team_id: TeamId,
        buyer_team_id: TeamId,
        offer_amount: Money,
        is_loan: bool,
        loan_seasons: i32,
        date: NaiveDate,
    ) -> NaratamaResult<i64> {
        if seller_team_id == buyer_team_id {
            return Err(NaratamaError::Validation(
                "Cannot make a transfer offer to the same club".into(),
            ));
        }
        if seller_team_id.0 > 0 && ClubRepo::squad_size(conn, seller_team_id)? <= 19 {
            return Err(NaratamaError::Validation(
                "Selling club has only 19 players left and refuses all exits".into(),
            ));
        }
        if ClubRepo::squad_size(conn, buyer_team_id)? >= 30 {
            return Err(NaratamaError::Validation(
                "Buying club has reached maximum 30-player squad capacity".into(),
            ));
        }
        let player_value: Money = conn
            .query_row(
                "SELECT COALESCE(value, 100000) FROM players WHERE player_id = ?1",
                params![player_id.0],
                |r| r.get(0),
            )
            .optional()
            .map_err(db_err)?
            .unwrap_or(100_000);

        MarketRepo::create_offer(
            conn,
            player_id,
            seller_team_id,
            buyer_team_id,
            offer_amount,
            player_value,
            is_loan,
            loan_seasons,
            date,
            None,
            TransferOfferStatus::Pending,
        )
    }

    #[allow(clippy::too_many_lines)]
    pub fn respond_to_offer(
        conn: &mut Connection,
        season_id: SeasonId,
        offer_id: i64,
        action: OfferResponseAction,
        date: NaiveDate,
    ) -> NaratamaResult<TransferExecutionOutcome> {
        let offer = load_offer_by_id(conn, offer_id)?;
        match action {
            OfferResponseAction::Reject => {
                MarketRepo::update_offer_status(
                    conn,
                    offer_id,
                    TransferOfferStatus::Rejected,
                    date,
                    None,
                    Some("Rejected by club"),
                )?;
                Ok(TransferExecutionOutcome {
                    completed_immediately: false,
                    scheduled_date: None,
                    offer_id,
                    message: "Offer rejected".into(),
                })
            }
            OfferResponseAction::Counter(amount) => {
                MarketRepo::update_offer_status(
                    conn,
                    offer_id,
                    TransferOfferStatus::Countered,
                    date,
                    Some(amount),
                    None,
                )?;
                Ok(TransferExecutionOutcome {
                    completed_immediately: false,
                    scheduled_date: None,
                    offer_id,
                    message: "Counter-offer submitted".into(),
                })
            }
            OfferResponseAction::Accept => {
                if offer.is_loan {
                    MarketRepo::complete_loan(
                        conn,
                        season_id,
                        offer.player_id,
                        offer.from_team_id,
                        offer.to_team_id,
                        offer.offer_amount,
                        offer.loan_seasons,
                        date,
                    )?;
                    MarketRepo::update_offer_status(
                        conn,
                        offer_id,
                        TransferOfferStatus::Completed,
                        date,
                        None,
                        None,
                    )?;
                    return Ok(TransferExecutionOutcome {
                        completed_immediately: true,
                        scheduled_date: None,
                        offer_id,
                        message: "Loan agreement completed".into(),
                    });
                }

                let window_open =
                    MarketRepo::active_window_for_team(conn, season_id, offer.to_team_id, date)?
                        .is_some();

                if window_open {
                    let agent_fee = FinanceCalc::agent_fee(offer.offer_amount);
                    MarketRepo::complete_transfer(
                        conn,
                        season_id,
                        offer.player_id,
                        offer.from_team_id,
                        offer.to_team_id,
                        offer.offer_amount,
                        agent_fee,
                        offer.proposed_salary,
                        offer.proposed_duration_years.unwrap_or(3),
                        date,
                        false,
                    )?;
                    MarketRepo::update_offer_status(
                        conn,
                        offer_id,
                        TransferOfferStatus::Completed,
                        date,
                        None,
                        None,
                    )?;
                    Ok(TransferExecutionOutcome {
                        completed_immediately: true,
                        scheduled_date: None,
                        offer_id,
                        message: "Transfer completed".into(),
                    })
                } else {
                    let next_start = MarketRepo::next_window_start_for_team(
                        conn,
                        season_id,
                        offer.to_team_id,
                        date,
                    )?;
                    let eff_str = next_start.format("%Y-%m-%d").to_string();
                    conn.execute(
                        "UPDATE transfer_offers
                         SET status = ?1, effective_date = ?2, eligible_date = ?2, agreement_date = ?3
                         WHERE offer_id = ?4",
                        params![
                            TransferOfferStatus::AwaitingWindow as i64,
                            eff_str,
                            date.format("%Y-%m-%d").to_string(),
                            offer_id
                        ],
                    )
                    .map_err(db_err)?;
                    Ok(TransferExecutionOutcome {
                        completed_immediately: false,
                        scheduled_date: Some(next_start),
                        offer_id,
                        message: format!("Transfer scheduled for registration on {eff_str}"),
                    })
                }
            }
        }
    }

    pub fn process_due_scheduled_transfers(
        conn: &mut Connection,
        season_id: SeasonId,
        date: NaiveDate,
    ) -> NaratamaResult<Vec<(i64, bool)>> {
        let due = MarketRepo::due_scheduled_offers(conn, date)?;
        let mut outcomes = Vec::with_capacity(due.len());
        for offer in due {
            let res = if offer.is_loan {
                MarketRepo::complete_loan(
                    conn,
                    season_id,
                    offer.player_id,
                    offer.from_team_id,
                    offer.to_team_id,
                    offer.offer_amount,
                    offer.loan_seasons,
                    date,
                )
            } else {
                let agent_fee = FinanceCalc::agent_fee(offer.offer_amount);
                MarketRepo::complete_transfer(
                    conn,
                    season_id,
                    offer.player_id,
                    offer.from_team_id,
                    offer.to_team_id,
                    offer.offer_amount,
                    agent_fee,
                    offer.proposed_salary,
                    offer.proposed_duration_years.unwrap_or(3),
                    date,
                    false,
                )
            };
            match res {
                Ok(()) => {
                    MarketRepo::update_offer_status(
                        conn,
                        offer.offer_id,
                        TransferOfferStatus::Completed,
                        date,
                        None,
                        None,
                    )?;
                    outcomes.push((offer.offer_id, true));
                }
                Err(err) => {
                    MarketRepo::update_offer_status(
                        conn,
                        offer.offer_id,
                        TransferOfferStatus::Failed,
                        date,
                        None,
                        Some(&err.to_string()),
                    )?;
                    outcomes.push((offer.offer_id, false));
                }
            }
        }
        Ok(outcomes)
    }

    /// Weekly AI market activity: up to 4 competing bidders, respects transfer windows, Agent fees, and 19..=30 squad bounds.
    #[allow(clippy::too_many_lines)]
    pub fn run_market_ai(
        conn: &mut Connection,
        season_id: SeasonId,
        user_team_id: TeamId,
        date: NaiveDate,
        rng: &mut impl Rng,
    ) -> NaratamaResult<usize> {
        let active_user_window =
            MarketRepo::active_window_for_team(conn, season_id, user_team_id, date)?;
        let is_deadline = active_user_window
            .as_ref()
            .is_some_and(|w| w.is_deadline_day(date));
        let tick_chance = if is_deadline { 0.85 } else { 0.35 };
        if rng.gen::<f64>() > tick_chance {
            return Ok(0);
        }

        let mut club_stmt = conn
            .prepare(
                "SELECT tm.team_id, COALESCE(tm.power, 60), COALESCE(tm.budget, 0)
                 FROM season_teams st
                 JOIN teams tm ON tm.team_id = st.team_id
                 WHERE st.season_id = ?1 AND tm.team_id != ?2",
            )
            .map_err(db_err)?;
        let mut clubs: Vec<(TeamId, i32, Money)> = club_stmt
            .query_map(params![season_id.0, user_team_id.0], |r| {
                Ok((TeamId(r.get(0)?), r.get(1)?, r.get(2)?))
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        drop(club_stmt);
        if clubs.len() < 2 {
            return Ok(0);
        }

        let mut listed_stmt = conn
            .prepare(
                "SELECT p.*, tp.team_id
                 FROM players p
                 JOIN team_players tp ON tp.player_id = p.player_id
                 WHERE COALESCE(p.status_id, 1) = 1
                   AND COALESCE(p.market_status, 1) = 2
                   AND tp.team_id != ?1
                 ORDER BY p.power DESC
                 LIMIT 12",
            )
            .map_err(db_err)?;
        let listed: Vec<(Player, TeamId)> = listed_stmt
            .query_map(params![user_team_id.0], |r| {
                Ok((ClubRepo::player_from_row(r)?, TeamId(r.get("team_id")?)))
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        drop(listed_stmt);

        let mut completed_or_scheduled = 0_usize;
        for (player, seller_id) in listed {
            if rng.gen::<f64>() > 0.25 {
                continue;
            }
            let min_budget = player.value + FinanceCalc::agent_fee(player.value);
            clubs.shuffle(rng);
            let Some(&(buyer_id, _, buyer_budget)) = clubs.iter().find(|&&(cid, c_pwr, bgt)| {
                cid != seller_id && bgt >= min_budget && c_pwr >= player.power - 15
            }) else {
                continue;
            };
            #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
            let offer_amount =
                ((player.value as f64) * (1.0 + rng.gen::<f64>() * 0.12)).round() as Money;
            let agent_fee = FinanceCalc::agent_fee(offer_amount);
            if offer_amount + agent_fee > buyer_budget {
                continue;
            }

            let buyer_open =
                MarketRepo::active_window_for_team(conn, season_id, buyer_id, date)?.is_some();
            if buyer_open {
                if MarketRepo::complete_transfer(
                    conn,
                    season_id,
                    player.player_id,
                    seller_id,
                    buyer_id,
                    offer_amount,
                    agent_fee,
                    None,
                    3,
                    date,
                    false,
                )
                .is_ok()
                {
                    completed_or_scheduled += 1;
                    break;
                }
            } else {
                let next_start =
                    MarketRepo::next_window_start_for_team(conn, season_id, buyer_id, date)?;
                MarketRepo::create_offer(
                    conn,
                    player.player_id,
                    seller_id,
                    buyer_id,
                    offer_amount,
                    player.value,
                    false,
                    1,
                    date,
                    Some(next_start),
                    TransferOfferStatus::AwaitingWindow,
                )?;
                completed_or_scheduled += 1;
                break;
            }
        }
        Ok(completed_or_scheduled)
    }
}

/// Scouting network: hire scouts, deploy to confederation regions (Bug #3 fix), collect reports, and sign wonderkids (Bug #10 fix).
pub struct ScoutingService;

impl ScoutingService {
    #[must_use]
    pub const fn hiring_cost_for_stars(stars: i32) -> Money {
        match stars {
            1 => 150_000,
            2 => 400_000,
            3 => 900_000,
            4 => 1_800_000,
            _ => 3_500_000,
        }
    }

    #[must_use]
    pub const fn weekly_salary_for_stars(stars: i32) -> Money {
        match stars {
            1 => 4_000,
            2 => 10_000,
            3 => 22_000,
            4 => 45_000,
            _ => 85_000,
        }
    }

    pub fn hire_scout(
        conn: &mut Connection,
        season_id: SeasonId,
        team_id: TeamId,
        name: &str,
        stars: i32,
        region: &str,
        date: NaiveDate,
    ) -> NaratamaResult<i64> {
        let clamped_stars = stars.clamp(1, 5);
        let cost = Self::hiring_cost_for_stars(clamped_stars);
        let salary = Self::weekly_salary_for_stars(clamped_stars);
        let budget: i64 = conn
            .query_row(
                "SELECT COALESCE(budget, 0) FROM teams WHERE team_id = ?1",
                params![team_id.0],
                |r| r.get(0),
            )
            .map_err(db_err)?;
        if budget < cost {
            return Err(NaratamaError::Validation(
                "Insufficient club budget to hire scout".into(),
            ));
        }

        let tx = conn.transaction().map_err(db_err)?;
        FinanceRepo::record_tx(
            &tx,
            &NewLedgerEntry {
                team_id,
                season_id,
                entry_date: date,
                period: LedgerPeriod::Annual,
                category: LedgerCategory::StaffWages,
                subcategory: "scout_hire_fee",
                amount: -cost,
                ref_type: "scout_hire",
                ref_id: Some(team_id.0),
                description: "Scout recruitment fee",
            },
        )?;
        tx.execute(
            "INSERT INTO scouts (team_id, name, stars, region, salary, status)
             VALUES (?1, ?2, ?3, ?4, ?5, 'idle')",
            params![team_id.0, name, clamped_stars, region, salary],
        )
        .map_err(db_err)?;
        let sid = tx.last_insert_rowid();
        tx.commit().map_err(db_err)?;
        Ok(sid)
    }

    pub fn deploy_scout(
        conn: &Connection,
        scout_id: i64,
        region: &str,
        date: NaiveDate,
    ) -> NaratamaResult<NaiveDate> {
        let return_date = date + Duration::days(21);
        let ret_str = return_date.format("%Y-%m-%d").to_string();
        conn.execute(
            "UPDATE scouts SET status = 'scouting', region = ?1, return_date = ?2 WHERE scout_id = ?3",
            params![region, ret_str, scout_id],
        )
        .map_err(db_err)?;
        Ok(return_date)
    }

    /// Bug #3 fix: picks authentic `country_id` from `MarketRepo::country_ids_for_region`.
    pub fn collect_due_reports(
        conn: &mut Connection,
        team_id: TeamId,
        date: NaiveDate,
        rng: &mut impl Rng,
    ) -> NaratamaResult<Vec<ScoutReportRow>> {
        let date_str = date.format("%Y-%m-%d").to_string();
        let scouts: Vec<ScoutRow> = MarketRepo::hired_scouts(conn, team_id)?
            .into_iter()
            .filter(|s| s.status == "scouting" && s.return_date.is_some_and(|r| r <= date))
            .collect();
        if scouts.is_empty() {
            return Ok(Vec::new());
        }

        let val_svc = PlayerValuationService::default();
        let mut generated = Vec::new();
        for scout in scouts {
            let country_pool = MarketRepo::country_ids_for_region(conn, &scout.region)?;
            let count = if scout.stars >= 4 { 3 } else { 2 };
            let tx = conn.transaction().map_err(db_err)?;
            for idx in 0..count {
                let country_id = country_pool[rng.gen_range(0..country_pool.len())];
                let age = rng.gen_range(16..=20);
                let pos = rng.gen_range(1..=11);
                let power = (50 + scout.stars * 3 + rng.gen_range(0..=10)).clamp(48, 78);
                let pot =
                    (power + 10 + scout.stars * 2 + rng.gen_range(0..=10)).clamp(power + 6, 96);
                let value = val_svc.calculate_target_market_value(PlayerValuationInput {
                    power,
                    potential: Some(pot),
                    age,
                    position_id: pos,
                    confederation: Confederation::Uefa,
                    team_power: 72,
                    contract_months_remaining: 36,
                    ..PlayerValuationInput::default()
                });
                let pname = format!("Scouted Prospect {}{idx}", date.ordinal());
                tx.execute(
                    "INSERT INTO scout_reports (
                        team_id, scout_id, scout_name, region, date,
                        player_name, age, position_id, power, potential, value, country_id, signed
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 0)",
                    params![
                        team_id.0,
                        scout.scout_id,
                        scout.name,
                        scout.region,
                        date_str,
                        pname,
                        age,
                        pos,
                        power,
                        pot,
                        value,
                        country_id.0,
                    ],
                )
                .map_err(db_err)?;
                let report_id = tx.last_insert_rowid();
                generated.push(ScoutReportRow {
                    report_id,
                    team_id,
                    scout_id: scout.scout_id,
                    scout_name: scout.name.clone(),
                    region: scout.region.clone(),
                    date: Some(date),
                    player_name: pname,
                    age,
                    position_id: pos,
                    power,
                    potential: pot,
                    value,
                    country_id,
                    signed: false,
                });
            }
            tx.execute(
                "UPDATE scouts SET status = 'idle', return_date = NULL, last_report = ?1 WHERE scout_id = ?2",
                params![date_str, scout.scout_id],
            )
            .map_err(db_err)?;
            tx.commit().map_err(db_err)?;
        }
        Ok(generated)
    }

    pub fn sign_wonderkid(
        conn: &mut Connection,
        season_id: SeasonId,
        report: &ScoutReportRow,
        date: NaiveDate,
    ) -> NaratamaResult<PlayerId> {
        let salary = 2_000 + Money::from(report.power - 45).max(0) * 200;
        let fee = (report.value / 4).max(50_000);
        MarketRepo::sign_new_prospect_to_club(
            conn,
            season_id,
            report.team_id,
            &report.player_name,
            report.age,
            report.position_id,
            report.power,
            report.potential,
            if report.potential >= 86 { 5 } else { 4 },
            PreferredFoot::Right,
            179,
            report.country_id,
            report.value,
            salary,
            fee,
            date,
            Some(report.report_id),
            None,
        )
    }
}

fn load_offer_by_id(conn: &Connection, offer_id: i64) -> NaratamaResult<TransferOfferRow> {
    conn.query_row(
        "SELECT o.offer_id, o.player_id, COALESCE(p.name, 'Player'),
                o.from_team_id, COALESCE(ft.name, 'Club'),
                o.to_team_id, COALESCE(tt.name, 'Club'),
                o.offer_amount, o.player_value, o.status,
                o.counter_amount, o.proposed_salary, o.proposed_duration_years,
                COALESCE(o.is_loan, 0), COALESCE(o.loan_seasons, 1)
         FROM transfer_offers o
         LEFT JOIN players p ON p.player_id = o.player_id
         LEFT JOIN teams ft ON ft.team_id = o.from_team_id
         LEFT JOIN teams tt ON tt.team_id = o.to_team_id
         WHERE o.offer_id = ?1",
        params![offer_id],
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
                effective_date: None,
            })
        },
    )
    .optional()
    .map_err(db_err)?
    .ok_or_else(|| NaratamaError::NotFound(format!("Transfer offer {offer_id} not found")))
}
