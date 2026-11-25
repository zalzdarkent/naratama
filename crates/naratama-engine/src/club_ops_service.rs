use chrono::{Datelike, NaiveDate};
use naratama_db::db_err;
use naratama_db::finance_repo::{FinanceRepo, NewLedgerEntry};
use naratama_model::{
    LedgerCategory, LedgerPeriod, Money, NaratamaError, NaratamaResult, PlayerId, SeasonId, TeamId,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

/// Result of renewing a player contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractRenewal {
    pub player_id: PlayerId,
    pub new_salary: Money,
    pub signing_bonus: Money,
    pub new_end_date: String,
}

/// A stadium or stand upgrade summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StadiumExpansion {
    pub added_seats: i32,
    pub cost: Money,
    pub new_capacity: i32,
}

/// Sponsorship slot on offer for an empty slot of the user's club.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SponsorOffer {
    pub slot_type: String,
    pub sponsor_id: i64,
    pub sponsor_name: String,
    pub upfront: Money,
    pub weekly: Money,
    pub target_wins: i64,
    pub target_bonus: Money,
}

struct RenewRow {
    player_salary: Money,
    contract_id: Option<i64>,
    contract_salary: Option<Money>,
    old_end: Option<String>,
    budget: Money,
}

struct StadiumRow {
    budget: Money,
    stadium_id: i64,
    general: i32,
    stand: i32,
    terrace: i32,
    boxes: i32,
    max_gen: i32,
    max_stand: i32,
    max_ter: i32,
    max_box: i32,
}

/// Club operations the manager controls directly: contracts, stadium, tickets, sponsorship.
pub struct ClubOpsService;

impl ClubOpsService {
    /// Extends a contract by two seasons: +15% salary, signing bonus = 2 months of the new salary.
    #[allow(clippy::too_many_lines)]
    pub fn renew_contract(
        conn: &mut Connection,
        team_id: TeamId,
        player_id: PlayerId,
        current_year: i32,
        date: NaiveDate,
    ) -> NaratamaResult<ContractRenewal> {
        let row = conn
            .query_row(
                "SELECT COALESCE(p.salary, 0), c.contract_id, c.salary, c.end_date,
                        COALESCE(tm.budget, 0)
                 FROM players p
                 JOIN team_players tp ON tp.player_id = p.player_id
                 JOIN teams tm ON tm.team_id = tp.team_id
                 LEFT JOIN contracts c ON c.player_id = p.player_id AND c.status = 1
                 WHERE p.player_id = ?1 AND tp.team_id = ?2
                 ORDER BY c.end_date DESC LIMIT 1",
                params![player_id.0, team_id.0],
                |r| {
                    Ok(RenewRow {
                        player_salary: r.get(0)?,
                        contract_id: r.get(1)?,
                        contract_salary: r.get(2)?,
                        old_end: r.get(3)?,
                        budget: r.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(db_err)?;
        let Some(RenewRow {
            player_salary,
            contract_id,
            contract_salary,
            old_end,
            budget,
        }) = row
        else {
            return Err(NaratamaError::Validation(
                "This player is not in your squad.".into(),
            ));
        };

        let old_salary = player_salary.max(contract_salary.unwrap_or(0));
        let new_salary = (old_salary + 1).max(((old_salary as f64) * 1.15).round() as i64);
        let signing_bonus = new_salary * 2;
        if budget < signing_bonus {
            return Err(NaratamaError::Validation(format!(
                "Not enough budget: the signing bonus is {signing_bonus}."
            )));
        }

        let old_end = old_end.unwrap_or_default();
        let old_end_year = old_end
            .get(0..4)
            .and_then(|s| s.parse::<i32>().ok())
            .unwrap_or(0);
        let base_year = old_end_year.max(current_year);
        let suffix = if old_end.len() >= 10 {
            old_end[4..10].to_string()
        } else {
            "-06-30".to_string()
        };
        let new_end = format!("{}{}", base_year + 2, suffix);

        let tx = conn.transaction().map_err(db_err)?;
        if let Some(cid) = contract_id {
            tx.execute(
                "UPDATE contracts SET salary = ?1, end_date = ?2, status = 1 WHERE contract_id = ?3",
                params![new_salary, new_end, cid],
            )
            .map_err(db_err)?;
        } else {
            tx.execute(
                "INSERT INTO contracts (player_id, team_id, start_date, end_date, status, salary)
                 VALUES (?1, ?2, ?3, ?4, 1, ?5)",
                params![
                    player_id.0,
                    team_id.0,
                    format!("{current_year}-07-01"),
                    new_end,
                    new_salary
                ],
            )
            .map_err(db_err)?;
        }
        tx.execute(
            "UPDATE players SET salary = ?1 WHERE player_id = ?2",
            params![new_salary, player_id.0],
        )
        .map_err(db_err)?;
        FinanceRepo::record_tx(
            &tx,
            &NewLedgerEntry {
                team_id,
                season_id: SeasonId(0),
                entry_date: date,
                period: LedgerPeriod::Annual,
                category: LedgerCategory::BonusPayouts,
                subcategory: "contract_signing_bonus",
                amount: -signing_bonus,
                ref_type: "contract_signing_bonus",
                ref_id: Some(player_id.0),
                description: "Contract signing bonus",
            },
        )?;
        tx.commit().map_err(db_err)?;

        Ok(ContractRenewal {
            player_id,
            new_salary,
            signing_bonus,
            new_end_date: new_end,
        })
    }

    fn next_tier(current: i32, max: i32) -> i32 {
        if current >= max {
            return current;
        }
        let step = if current / 10 < 100 {
            100
        } else {
            current / 10
        };
        (current + step).min(max)
    }

    /// Adds roughly 10% to every stand up to its maximum, charging 1.2k per new seat.
    pub fn expand_stadium(
        conn: &mut Connection,
        team_id: TeamId,
        season_id: SeasonId,
        date: NaiveDate,
    ) -> NaratamaResult<StadiumExpansion> {
        let row = conn
            .query_row(
                "SELECT COALESCE(t.budget, 0), t.stadium_id,
                        s.capacity_general, s.capacity_stand, s.capacity_terrace, s.capacity_boxes,
                        COALESCE(s.max_general, s.capacity_general),
                        COALESCE(s.max_stand, s.capacity_stand),
                        COALESCE(s.max_terrace, s.capacity_terrace),
                        COALESCE(s.max_boxes, s.capacity_boxes)
                 FROM teams t JOIN stadiums s ON s.stadium_id = t.stadium_id
                 WHERE t.team_id = ?1 LIMIT 1",
                params![team_id.0],
                |r| {
                    Ok(StadiumRow {
                        budget: r.get(0)?,
                        stadium_id: r.get(1)?,
                        general: r.get(2)?,
                        stand: r.get(3)?,
                        terrace: r.get(4)?,
                        boxes: r.get(5)?,
                        max_gen: r.get(6)?,
                        max_stand: r.get(7)?,
                        max_ter: r.get(8)?,
                        max_box: r.get(9)?,
                    })
                },
            )
            .optional()
            .map_err(db_err)?;
        let Some(StadiumRow {
            budget,
            stadium_id,
            general,
            stand,
            terrace,
            boxes,
            max_gen,
            max_stand,
            max_ter,
            max_box,
        }) = row
        else {
            return Err(NaratamaError::NotFound("Club or stadium not found.".into()));
        };

        let new_general = Self::next_tier(general, max_gen);
        let new_stand = Self::next_tier(stand, max_stand);
        let new_terrace = Self::next_tier(terrace, max_ter);
        let new_boxes = Self::next_tier(boxes, max_box);
        let added = (new_general - general)
            + (new_stand - stand)
            + (new_terrace - terrace)
            + (new_boxes - boxes);
        if added <= 0 {
            return Err(NaratamaError::Validation(
                "Stadium is already at maximum capacity.".into(),
            ));
        }
        let cost = Money::from(added) * 1_200;
        if budget < cost {
            return Err(NaratamaError::Validation(format!(
                "Not enough budget: the upgrade costs {cost}."
            )));
        }

        let tx = conn.transaction().map_err(db_err)?;
        tx.execute(
            "UPDATE stadiums SET capacity_general = ?1, capacity_stand = ?2,
                    capacity_terrace = ?3, capacity_boxes = ?4
             WHERE stadium_id = ?5",
            params![new_general, new_stand, new_terrace, new_boxes, stadium_id],
        )
        .map_err(db_err)?;
        FinanceRepo::record_tx(
            &tx,
            &NewLedgerEntry {
                team_id,
                season_id,
                entry_date: date,
                period: LedgerPeriod::Annual,
                category: LedgerCategory::FacilityUpkeep,
                subcategory: "stadium_expansion",
                amount: -cost,
                ref_type: "stadium_expansion",
                ref_id: None,
                description: "Stadium expansion",
            },
        )?;
        tx.commit().map_err(db_err)?;

        Ok(StadiumExpansion {
            added_seats: added,
            cost,
            new_capacity: new_general + new_stand + new_terrace + new_boxes,
        })
    }

    /// Sets one stand's ticket price (`general`, `stand`, `terrace`, `boxes`).
    pub fn set_ticket_price(
        conn: &Connection,
        team_id: TeamId,
        seat_type: &str,
        price: i32,
    ) -> NaratamaResult<()> {
        if price < 0 {
            return Err(NaratamaError::Validation(
                "Ticket price cannot be negative.".into(),
            ));
        }
        let column = match seat_type {
            "general" => "ticket_price_general",
            "stand" => "ticket_price_stand",
            "terrace" => "ticket_price_terrace",
            "boxes" => "ticket_price_boxes",
            _ => {
                return Err(NaratamaError::Validation("Unknown seat type.".into()));
            }
        };
        let sql = format!(
            "UPDATE stadiums SET {column} = ?1
             WHERE stadium_id = (SELECT stadium_id FROM teams WHERE team_id = ?2)"
        );
        let updated = conn
            .execute(&sql, params![price, team_id.0])
            .map_err(db_err)?;
        if updated == 0 {
            return Err(NaratamaError::NotFound("Club or stadium not found.".into()));
        }
        Ok(())
    }

    /// Sponsorship offers for every still-empty slot of the user's club.
    ///
    /// Mirrors the Flutter rules: 3 slots (`main_shirt`, `kit_sleeve`, `stadium_partner`),
    /// an upfront fee scaled by club power, a weekly payment, and a win-count bonus target.
    pub fn sponsor_offers(
        conn: &Connection,
        team_id: TeamId,
        season_id: SeasonId,
        club_power: i32,
    ) -> NaratamaResult<Vec<SponsorOffer>> {
        let filled: Vec<String> =
            FinanceRepo::active_sponsorships_for_team(conn, team_id, season_id)?
                .into_iter()
                .map(|s| s.deal_type)
                .collect();
        let slots = ["main_shirt", "kit_sleeve", "stadium_partner"];
        if slots.iter().all(|s| filled.iter().any(|f| f == s)) {
            return Ok(Vec::new());
        }

        let mut stmt = conn
            .prepare(
                "SELECT sponsor_id, COALESCE(name, 'Sponsor') FROM sponsors ORDER BY sponsor_id",
            )
            .map_err(db_err)?;
        let catalog: Vec<(i64, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(db_err)?
            .filter_map(Result::ok)
            .collect();
        if catalog.is_empty() {
            return Ok(Vec::new());
        }

        let scale = f64::from(club_power.saturating_sub(40)) / 10.0;
        let scale = scale.max(1.0);
        let target_wins = if club_power >= 75 {
            3
        } else if club_power >= 65 {
            6
        } else {
            10
        };
        let mut offers = Vec::new();
        for (idx, slot) in slots.iter().enumerate() {
            if filled.iter().any(|f| f == slot) {
                continue;
            }
            let base = match *slot {
                "main_shirt" => 1_200_000.0,
                "kit_sleeve" => 800_000.0,
                _ => 500_000.0,
            };
            let upfront = (base * scale).round() as Money;
            let weekly = ((upfront as f64) / 38.0 * 0.8).round() as Money;
            let target_bonus = ((upfront as f64) * 0.75).round() as Money;
            let (sponsor_id, sponsor_name) = catalog
                .get((team_id.0 as usize + idx * 7 + season_id.0 as usize) % catalog.len())
                .cloned()
                .unwrap_or_else(|| (catalog[0].0, catalog[0].1.clone()));
            offers.push(SponsorOffer {
                slot_type: (*slot).to_string(),
                sponsor_id,
                sponsor_name,
                upfront,
                weekly,
                target_wins,
                target_bonus,
            });
        }
        Ok(offers)
    }

    /// Signs an offered sponsorship; the ledger entry carries the upfront fee into the budget.
    pub fn sign_sponsorship(
        conn: &mut Connection,
        team_id: TeamId,
        season_id: SeasonId,
        offer: &SponsorOffer,
        date: NaiveDate,
    ) -> NaratamaResult<i64> {
        let existing: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sponsorships
                 WHERE team_id = ?1 AND season_id = ?2 AND type = ?3 AND COALESCE(status, 1) = 1",
                params![team_id.0, season_id.0, offer.slot_type],
                |r| r.get(0),
            )
            .map_err(db_err)?;
        if existing > 0 {
            return Err(NaratamaError::Validation(format!(
                "Slot {} is already sponsored",
                offer.slot_type
            )));
        }
        let tx = conn.transaction().map_err(db_err)?;
        tx.execute(
            "INSERT INTO sponsorships (
                type, team_id, sponsor_id, tournament_id, season_id, is_priority,
                is_unique_prize, base_reward, target, progress, success, status, extra_data
             ) VALUES (?1, ?2, ?3, NULL, ?4, 1, 0, ?5, ?6, 0, 0, 1, ?7)",
            params![
                offer.slot_type,
                team_id.0,
                offer.sponsor_id,
                season_id.0,
                offer.weekly,
                offer.target_wins,
                format!("{{\"target_bonus\":{}}}", offer.target_bonus)
            ],
        )
        .map_err(db_err)?;
        let sponsorship_id = tx.last_insert_rowid();
        FinanceRepo::record_tx(
            &tx,
            &NewLedgerEntry {
                team_id,
                season_id,
                entry_date: date,
                period: LedgerPeriod::Annual,
                category: LedgerCategory::SponsorshipUpfront,
                subcategory: "sponsorship_upfront",
                amount: offer.upfront,
                ref_type: "sponsorship",
                ref_id: Some(sponsorship_id),
                description: "Sponsorship signing fee",
            },
        )?;
        tx.commit().map_err(db_err)?;
        Ok(sponsorship_id)
    }

    /// Terminates a sponsorship deal (status 0) so the slot can be re-signed.
    pub fn terminate_sponsorship(
        conn: &Connection,
        team_id: TeamId,
        sponsorship_id: i64,
    ) -> NaratamaResult<()> {
        let updated = conn
            .execute(
                "UPDATE sponsorships SET status = 0 WHERE sponsorship_id = ?1 AND team_id = ?2",
                params![sponsorship_id, team_id.0],
            )
            .map_err(db_err)?;
        if updated == 0 {
            return Err(NaratamaError::NotFound(format!(
                "Sponsorship {sponsorship_id} not found"
            )));
        }
        Ok(())
    }

    /// Remaining suspension count per player for the club (all competitions).
    pub fn suspensions(conn: &Connection, team_id: TeamId) -> NaratamaResult<Vec<(PlayerId, i64)>> {
        let mut stmt = conn
            .prepare(
                "SELECT s.player_id, SUM(COALESCE(s.matches_remaining, 0)) AS remaining
                 FROM suspensions s
                 JOIN team_players tp ON tp.player_id = s.player_id
                 WHERE tp.team_id = ?1 AND COALESCE(s.matches_remaining, 0) > 0
                 GROUP BY s.player_id",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![team_id.0], |r| Ok((PlayerId(r.get(0)?), r.get(1)?)))
            .map_err(db_err)?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    /// Current season year for contract extensions (`start_year` of the active season).
    pub fn current_year(conn: &Connection, season_id: SeasonId) -> NaratamaResult<i32> {
        let year: Option<i32> = conn
            .query_row(
                "SELECT start_year FROM seasons WHERE season_id = ?1",
                params![season_id.0],
                |r| r.get(0),
            )
            .optional()
            .map_err(db_err)?;
        Ok(year.unwrap_or_else(|| NaiveDate::from_ymd_opt(2025, 7, 1).map_or(2025, |d| d.year())))
    }
}
