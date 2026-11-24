use chrono::NaiveDate;
use naratama_db::club_repo::ClubRepo;
use naratama_db::finance_repo::{FinanceRepo, NewLedgerEntry};
use naratama_db::{db_err, SeasonRepo};
use naratama_model::{
    FfpEnforcement, LedgerCategory, LedgerPeriod, MatchStatus, Money, NaratamaResult, SeasonId,
    TeamId, TournamentId, TournamentOrg,
};
use naratama_sim::{
    compute_standings, CompetitionKind, CompetitionProfile, CupPrizeInput, EconomyScale,
    FfpAssessment, FfpAssessmentInput, FinanceCalc, PrizeEngine,
};
use rand::Rng;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Finance, broadcast, sponsorship, and prize distribution orchestration.
pub struct FinanceService;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BroadcastPaymentResult {
    pub paid: bool,
    pub installment: Option<String>,
    pub amount: Money,
}

impl FinanceService {
    /// Seeds opening ledger balances once so `teams.budget == SUM(finance_ledger.amount)` holds across all clubs.
    pub fn ensure_ready(
        conn: &mut Connection,
        season_id: SeasonId,
        opening_date: NaiveDate,
    ) -> NaratamaResult<()> {
        FinanceRepo::ensure_ready(conn, season_id, opening_date)
    }

    /// Pays weekly player wages for all clubs registered in `season_id`.
    pub fn pay_weekly_player_salaries(
        conn: &mut Connection,
        season_id: SeasonId,
        date: NaiveDate,
    ) -> NaratamaResult<usize> {
        let mut stmt = conn
            .prepare(
                "SELECT tp.team_id, COALESCE(SUM(p.salary), 0) AS payroll
                 FROM team_players tp
                 JOIN players p ON p.player_id = tp.player_id
                 JOIN season_teams st ON st.team_id = tp.team_id
                 WHERE st.season_id = ?1 AND COALESCE(p.status_id, 1) = 1
                 GROUP BY tp.team_id",
            )
            .map_err(db_err)?;
        let rows: Vec<(TeamId, Money)> = stmt
            .query_map(params![season_id.0], |r| Ok((TeamId(r.get(0)?), r.get(1)?)))
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        drop(stmt);

        let entries: Vec<NewLedgerEntry<'_>> = rows
            .iter()
            .filter(|(_, payroll)| *payroll > 0)
            .map(|&(team_id, payroll)| NewLedgerEntry {
                team_id,
                season_id,
                entry_date: date,
                period: LedgerPeriod::Weekly,
                category: LedgerCategory::PlayerWages,
                subcategory: "player_wages",
                amount: -payroll,
                ref_type: "weekly_wages",
                ref_id: Some(season_id.0),
                description: "Weekly squad wages",
            })
            .collect();
        let paid_clubs = entries.len();
        FinanceRepo::record_batch(conn, &entries)?;
        Ok(paid_clubs)
    }

    /// Evaluates board emergency injections (max 1 per club per season).
    pub fn maybe_emergency_board_injection(
        conn: &mut Connection,
        season_id: SeasonId,
        user_team_id: TeamId,
        date: NaiveDate,
    ) -> NaratamaResult<Vec<(TeamId, Money)>> {
        let injected = FinanceRepo::emergency_injections(conn, season_id, date)?;
        for &(team_id, amount) in &injected {
            if team_id == user_team_id {
                let _ = ClubRepo::insert_message(
                    conn,
                    "Board emergency injection",
                    &format!("The board injected ${amount} to cover club cash deficit."),
                    date,
                    "Board of Directors",
                    Some(user_team_id),
                    None,
                );
            }
        }
        Ok(injected)
    }

    /// Assesses PSR / Squad Cost Ratio telemetry for a club in `season_id`.
    pub fn assess_club_psr(
        conn: &Connection,
        team_id: TeamId,
        season_id: SeasonId,
        mode: FfpEnforcement,
    ) -> NaratamaResult<FfpAssessment> {
        let cash_rows = FinanceRepo::team_season_cash_summary(conn, team_id, season_id)?;
        let mut football_rev: Money = 0;
        let mut staff_wages: Money = 0;
        let mut agent_fees: Money = 0;
        let mut other_ops: Money = 0;
        for (cat, amt) in cash_rows {
            match cat.as_str() {
                "matchday_tickets"
                | "matchday_merch"
                | "broadcast"
                | "sponsorship_upfront"
                | "sponsorship_weekly"
                | "sponsorship_bonus"
                | "prize_money" => football_rev += amt,
                "staff_wages" if amt < 0 => staff_wages += -amt,
                "agent_fees" if amt < 0 => agent_fees += -amt,
                "academy_upkeep" | "facility_upkeep" | "matchday_ops" if amt < 0 => {
                    other_ops += -amt;
                }
                _ => {}
            }
        }
        let (wages, amort) = FinanceRepo::season_wage_and_amortization(conn, team_id, season_id)?;
        let psr_result = football_rev - (wages + amort + staff_wages + agent_fees + other_ops);
        Ok(FinanceCalc::assess_ffp(FfpAssessmentInput {
            enforcement: mode,
            football_revenue: football_rev,
            annual_player_wages: wages,
            annual_staff_wages: staff_wages,
            annual_amortization: amort,
            agent_fees,
            psr_result,
            psr_revenue: football_rev,
        }))
    }
}

/// Broadcast pool distribution in 3 installments (`kickoff` 40%, `midseason` 30%, `final` 30%).
pub struct BroadcastService;

impl BroadcastService {
    #[allow(clippy::too_many_lines)]
    pub fn pay_due_installments(
        conn: &mut Connection,
        season_id: SeasonId,
        start_year: i32,
        end_year: i32,
        user_team_id: TeamId,
        date: NaiveDate,
    ) -> NaratamaResult<BroadcastPaymentResult> {
        let Some(kickoff) = NaiveDate::from_ymd_opt(start_year, 8, 1) else {
            return Ok(BroadcastPaymentResult {
                paid: false,
                installment: None,
                amount: 0,
            });
        };
        let midseason = NaiveDate::from_ymd_opt(end_year, 1, 1).unwrap_or(kickoff);
        let final_day = NaiveDate::from_ymd_opt(end_year, 5, 15).unwrap_or(midseason);

        let phases = [
            ("broadcast_kickoff", "kickoff", 0.40_f64, kickoff),
            ("broadcast_midseason", "midseason", 0.30_f64, midseason),
            ("broadcast_final", "final", 0.30_f64, final_day),
        ];

        // Find primary league info for registered clubs
        let mut league_stmt = conn
            .prepare(
                "SELECT DISTINCT tu.tournament_id, tu.division_id,
                        COALESCE(c.power, 60), COALESCE(c.confederation, 3)
                 FROM season_tournaments st
                 JOIN tournaments tu ON tu.tournament_id = st.tournament_id
                 LEFT JOIN tournament_countries tc ON tc.tournament_id = tu.tournament_id
                 LEFT JOIN countries c ON c.country_id = tc.country_id
                 WHERE st.season_id = ?1 AND tu.organization = 1",
            )
            .map_err(db_err)?;
        let leagues: Vec<(i64, i32, i32, i64)> = league_stmt
            .query_map(params![season_id.0], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        drop(league_stmt);

        let mut user_paid_amount = 0;
        let mut user_paid_label = None;

        for (tag, label, pct, due_date) in phases {
            if date < due_date {
                continue;
            }
            // Idempotent check against user club (or first league club if user already retired)
            if FinanceRepo::has_ledger_ref(conn, user_team_id, season_id, "broadcast_pool", tag)? {
                continue;
            }

            let mut entries = Vec::new();
            for &(tid, div_id, c_power, conf_code) in &leagues {
                let mut club_stmt = conn
                    .prepare(
                        "SELECT DISTINCT st.team_id
                         FROM season_teams st
                         JOIN teams tm ON tm.team_id = st.team_id
                         JOIN tournament_countries tc ON tc.country_id = tm.country_id
                         WHERE st.season_id = ?1 AND tc.tournament_id = ?2 AND tm.division = ?3
                         ORDER BY tm.power DESC, st.team_id ASC",
                    )
                    .map_err(db_err)?;
                let teams: Vec<TeamId> = club_stmt
                    .query_map(params![season_id.0, tid, div_id], |r| Ok(TeamId(r.get(0)?)))
                    .map_err(db_err)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(db_err)?;
                drop(club_stmt);

                if teams.is_empty() {
                    continue;
                }
                let pool = FinanceCalc::broadcast_pool(
                    c_power,
                    naratama_model::Confederation::from_code(conf_code),
                    div_id,
                    teams.len(),
                );
                let dist = FinanceCalc::broadcast_distribution(pool, teams.len());
                for (team_id, share) in teams.into_iter().zip(dist.shares) {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
                    let part = ((share as f64) * pct).round() as i64;
                    if part <= 0 {
                        continue;
                    }
                    if FinanceRepo::has_ledger_ref(conn, team_id, season_id, "broadcast_pool", tag)?
                    {
                        continue;
                    }
                    if team_id == user_team_id {
                        user_paid_amount += part;
                        user_paid_label = Some(label.to_string());
                    }
                    entries.push(NewLedgerEntry {
                        team_id,
                        season_id,
                        entry_date: date,
                        period: LedgerPeriod::Annual,
                        category: LedgerCategory::Broadcast,
                        subcategory: tag,
                        amount: part,
                        ref_type: "broadcast_pool",
                        ref_id: Some(tid),
                        description: "Domestic TV broadcast installment",
                    });
                }
            }
            FinanceRepo::record_batch(conn, &entries)?;
            if user_paid_amount > 0 {
                return Ok(BroadcastPaymentResult {
                    paid: true,
                    installment: user_paid_label,
                    amount: user_paid_amount,
                });
            }
        }

        Ok(BroadcastPaymentResult {
            paid: false,
            installment: None,
            amount: 0,
        })
    }
}

/// Manages 3-slot club sponsorship deals (Main Shirt, Kit Sleeve, Stadium Partner).
pub struct SponsorshipService;

impl SponsorshipService {
    pub fn ensure_season_deals(
        conn: &mut Connection,
        season_id: SeasonId,
        user_team_id: TeamId,
        date: NaiveDate,
        rng: &mut impl Rng,
    ) -> NaratamaResult<()> {
        let catalog = FinanceRepo::sponsors_catalog(conn)?;
        if catalog.is_empty() {
            return Ok(());
        }

        let mut stmt = conn
            .prepare(
                "SELECT DISTINCT st.team_id, COALESCE(tm.power, 60), COALESCE(c.power, 60)
                 FROM season_teams st
                 JOIN teams tm ON tm.team_id = st.team_id
                 LEFT JOIN countries c ON c.country_id = tm.country_id
                 WHERE st.season_id = ?1",
            )
            .map_err(db_err)?;
        let clubs: Vec<(TeamId, i32, i32)> = stmt
            .query_map(params![season_id.0], |r| {
                Ok((TeamId(r.get(0)?), r.get(1)?, r.get(2)?))
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        drop(stmt);

        let slot_types = ["main_shirt", "kit_sleeve", "stadium_partner"];
        let mut ledger_entries = Vec::new();
        let tx = conn.transaction().map_err(db_err)?;

        for (team_id, team_power, country_power) in clubs {
            let existing: i64 = tx
                .query_row(
                    "SELECT COUNT(*) FROM sponsorships WHERE team_id = ?1 AND season_id = ?2 AND COALESCE(status, 1) = 1",
                    params![team_id.0, season_id.0],
                    |r| r.get(0),
                )
                .map_err(db_err)?;
            if existing >= 3 {
                continue;
            }
            // User club starts with slot 1 (Main) signed or auto-signed in season orientation; AI signs all 3 slots.
            let target_slots = if team_id == user_team_id { 1 } else { 3 };
            let existing_usize = usize::try_from(existing).unwrap_or(0);
            for (slot_idx, &slot_type) in slot_types
                .iter()
                .enumerate()
                .take(target_slots)
                .skip(existing_usize)
            {
                let (sponsor_id, _, _) = &catalog[rng.gen_range(0..catalog.len())];
                let scale = EconomyScale::national(country_power);
                #[allow(clippy::cast_possible_truncation)]
                let weekly = (((i64::from(team_power.max(40)) - 35).max(5) * 1_800) as f64
                    * scale
                    * if slot_idx == 0 { 1.0 } else { 0.45 })
                .round() as i64;
                let upfront = weekly * 4;
                let target_wins = if slot_idx == 0 { 14 } else { 10 };

                tx.execute(
                    "INSERT INTO sponsorships (
                        type, team_id, sponsor_id, tournament_id, season_id,
                        is_priority, is_unique_prize, base_reward, target, progress, success, status
                     ) VALUES (?1, ?2, ?3, NULL, ?4, ?5, 0, ?6, ?7, 0, 0, 1)",
                    params![
                        slot_type,
                        team_id.0,
                        sponsor_id,
                        season_id.0,
                        i32::from(slot_idx == 0),
                        weekly,
                        target_wins,
                    ],
                )
                .map_err(db_err)?;
                let sid = tx.last_insert_rowid();
                ledger_entries.push((team_id, sid, upfront));
            }
        }

        for (team_id, sid, upfront) in ledger_entries {
            FinanceRepo::record_tx(
                &tx,
                &NewLedgerEntry {
                    team_id,
                    season_id,
                    entry_date: date,
                    period: LedgerPeriod::Annual,
                    category: LedgerCategory::SponsorshipUpfront,
                    subcategory: "sponsorship_upfront",
                    amount: upfront,
                    ref_type: "sponsorship",
                    ref_id: Some(sid),
                    description: "Season sponsorship signing fee",
                },
            )?;
        }
        tx.commit().map_err(db_err)?;
        Ok(())
    }

    pub fn pay_weekly_all(
        conn: &mut Connection,
        season_id: SeasonId,
        date: NaiveDate,
    ) -> NaratamaResult<usize> {
        let mut stmt = conn
            .prepare(
                "SELECT s.team_id, s.sponsorship_id, COALESCE(s.base_reward, 0)
                 FROM sponsorships s
                 WHERE s.season_id = ?1 AND COALESCE(s.status, 1) = 1 AND COALESCE(s.base_reward, 0) > 0",
            )
            .map_err(db_err)?;
        let rows: Vec<(TeamId, i64, Money)> = stmt
            .query_map(params![season_id.0], |r| {
                Ok((TeamId(r.get(0)?), r.get(1)?, r.get(2)?))
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        drop(stmt);

        let entries: Vec<NewLedgerEntry<'_>> = rows
            .iter()
            .map(|&(team_id, sid, base)| NewLedgerEntry {
                team_id,
                season_id,
                entry_date: date,
                period: LedgerPeriod::Weekly,
                category: LedgerCategory::SponsorshipWeekly,
                subcategory: "sponsorship_weekly",
                amount: base,
                ref_type: "sponsorship",
                ref_id: Some(sid),
                description: "Weekly sponsorship payout",
            })
            .collect();
        let count = entries.len();
        FinanceRepo::record_batch(conn, &entries)?;
        Ok(count)
    }

    pub fn evaluate_season_targets(
        conn: &mut Connection,
        team_id: TeamId,
        season_id: SeasonId,
        final_league_rank: i32,
        date: NaiveDate,
    ) -> NaratamaResult<Vec<(String, Money)>> {
        let wins: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM matches
                 WHERE season_id = ?1 AND status = ?2 AND winner_id = ?3",
                params![season_id.0, MatchStatus::Finished as i64, team_id.0],
                |r| r.get(0),
            )
            .map_err(db_err)?;

        let deals = FinanceRepo::active_sponsorships_for_team(conn, team_id, season_id)?;
        let mut bonuses = Vec::new();
        let tx = conn.transaction().map_err(db_err)?;
        for deal in deals {
            let met = wins >= deal.target || (final_league_rank > 0 && final_league_rank <= 6);
            if met && !deal.success {
                let bonus = (deal.base_reward * 8).max(150_000);
                tx.execute(
                    "UPDATE sponsorships SET progress = ?1, success = 1, status = 0 WHERE sponsorship_id = ?2",
                    params![wins, deal.sponsorship_id],
                )
                .map_err(db_err)?;
                FinanceRepo::record_tx(
                    &tx,
                    &NewLedgerEntry {
                        team_id,
                        season_id,
                        entry_date: date,
                        period: LedgerPeriod::Annual,
                        category: LedgerCategory::SponsorshipBonus,
                        subcategory: "sponsorship_bonus",
                        amount: bonus,
                        ref_type: "sponsorship",
                        ref_id: Some(deal.sponsorship_id),
                        description: "Sponsorship objective bonus",
                    },
                )?;
                bonuses.push((deal.sponsor_name, bonus));
            } else {
                tx.execute(
                    "UPDATE sponsorships SET progress = ?1, status = 0 WHERE sponsorship_id = ?2",
                    params![wins, deal.sponsorship_id],
                )
                .map_err(db_err)?;
            }
        }
        tx.commit().map_err(db_err)?;
        Ok(bonuses)
    }
}

/// End-of-season competition prize pool settlement across leagues and knockout cups.
pub struct PrizeDistributionService;

impl PrizeDistributionService {
    #[allow(clippy::too_many_lines)]
    pub fn award_all_season_prizes(
        conn: &mut Connection,
        season_id: SeasonId,
        user_team_id: TeamId,
        date: NaiveDate,
    ) -> NaratamaResult<HashSet<String>> {
        let mut won_user_titles = HashSet::new();
        let engine = PrizeEngine::default();

        let mut stmt = conn
            .prepare(
                "SELECT tu.tournament_id, tu.name, tu.organization, tu.division_id,
                        COALESCE(tu.prize_teams, 0)
                 FROM season_tournaments st
                 JOIN tournaments tu ON tu.tournament_id = st.tournament_id
                 WHERE st.season_id = ?1 AND st.status = 1",
            )
            .map_err(db_err)?;
        let tournaments: Vec<(i64, String, i64, i32, i64)> = stmt
            .query_map(params![season_id.0], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        drop(stmt);

        let all_teams = {
            let mut ts = conn
                .prepare("SELECT team_id, name FROM teams")
                .map_err(db_err)?;
            let map: HashMap<TeamId, String> = ts
                .query_map([], |r| Ok((TeamId(r.get(0)?), r.get(1)?)))
                .map_err(db_err)?
                .collect::<Result<HashMap<_, _>, _>>()
                .map_err(db_err)?;
            map
        };

        for (tid_raw, name, org_code, div_id, legacy_prize) in tournaments {
            let tid = TournamentId(tid_raw);
            let org = TournamentOrg::from_code(org_code);

            let (country_power, conf_code): (i32, i64) = conn
                .query_row(
                    "SELECT COALESCE(c.power, 60), COALESCE(c.confederation, 3)
                     FROM tournament_countries tc
                     JOIN countries c ON c.country_id = tc.country_id
                     WHERE tc.tournament_id = ?1 LIMIT 1",
                    params![tid_raw],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .map_err(db_err)?
                .unwrap_or((60, 3));
            let confed = naratama_model::Confederation::from_code(conf_code);

            if org == TournamentOrg::League {
                let fixtures = SeasonRepo::new(conn).matches_for_tournament(season_id, tid)?;
                let league_matches: Vec<_> =
                    fixtures.into_iter().filter(|m| m.phase <= 1).collect();
                let mut club_set: Vec<TeamId> = Vec::new();
                for m in &league_matches {
                    if !club_set.contains(&m.home_team_id) {
                        club_set.push(m.home_team_id);
                    }
                    if !club_set.contains(&m.away_team_id) {
                        club_set.push(m.away_team_id);
                    }
                }
                if club_set.is_empty() {
                    continue;
                }
                let table = compute_standings(&club_set, &all_teams, &league_matches);
                let ranked: Vec<TeamId> = table.iter().map(|r| r.team_id).collect();
                if ranked.first().copied() == Some(user_team_id) {
                    won_user_titles.insert(name.clone());
                }
                let profile = CompetitionProfile {
                    kind: CompetitionKind::League,
                    tier: div_id.max(1),
                    entrants: ranked.len(),
                    country_power,
                    confederation: confed,
                    slots: 1,
                    legacy_prize_teams: legacy_prize,
                };
                let res = engine.league_prizes(tid, profile, &ranked);
                for award in res.awards {
                    FinanceRepo::award_competition_prize(
                        conn,
                        season_id,
                        tid,
                        award.team_id,
                        award.amount,
                        &award.reason,
                        date,
                    )?;
                }
            } else if org == TournamentOrg::NationalCup || org == TournamentOrg::International {
                let fixtures = SeasonRepo::new(conn).matches_for_tournament(season_id, tid)?;
                if fixtures.is_empty() {
                    continue;
                }
                let mut max_phase_by_team: HashMap<TeamId, i32> = HashMap::new();
                let mut wins_by_team: HashMap<TeamId, i32> = HashMap::new();
                let mut draws_by_team: HashMap<TeamId, i32> = HashMap::new();
                for m in &fixtures {
                    max_phase_by_team
                        .entry(m.home_team_id)
                        .and_modify(|p| *p = (*p).max(m.phase))
                        .or_insert(m.phase);
                    max_phase_by_team
                        .entry(m.away_team_id)
                        .and_modify(|p| *p = (*p).max(m.phase))
                        .or_insert(m.phase);
                    if m.status == MatchStatus::Finished {
                        if let (Some(hg), Some(ag)) = (m.home_goals, m.away_goals) {
                            match hg.cmp(&ag) {
                                std::cmp::Ordering::Greater => {
                                    *wins_by_team.entry(m.home_team_id).or_default() += 1;
                                }
                                std::cmp::Ordering::Less => {
                                    *wins_by_team.entry(m.away_team_id).or_default() += 1;
                                }
                                std::cmp::Ordering::Equal => {
                                    *draws_by_team.entry(m.home_team_id).or_default() += 1;
                                    *draws_by_team.entry(m.away_team_id).or_default() += 1;
                                }
                            }
                        }
                    }
                }
                let winner_opt = SeasonRepo::new(conn)
                    .last_final_teams(tid, season_id)?
                    .map(|(winner, runner)| (Some(winner), Some(runner)));
                let (winner_id, runner_id) = winner_opt.unwrap_or((None, None));
                if winner_id == Some(user_team_id) {
                    won_user_titles.insert(name.clone());
                }
                let kind = if org == TournamentOrg::NationalCup {
                    CompetitionKind::DomesticCup
                } else {
                    CompetitionKind::Continental
                };
                let profile = CompetitionProfile {
                    kind,
                    tier: 1,
                    entrants: max_phase_by_team.len().max(2),
                    country_power,
                    confederation: confed,
                    slots: 1,
                    legacy_prize_teams: legacy_prize,
                };
                let reached: Vec<(TeamId, i32)> = max_phase_by_team.into_iter().collect();
                let wins_vec: Vec<(TeamId, i32)> = wins_by_team.into_iter().collect();
                let draws_vec: Vec<(TeamId, i32)> = draws_by_team.into_iter().collect();
                let cup_input = CupPrizeInput {
                    tournament_id: tid,
                    kind,
                    profile,
                    reached_round_by_team: &reached,
                    winner_team_id: winner_id,
                    runner_up_team_id: runner_id,
                    wins_by_team: &wins_vec,
                    draws_by_team: &draws_vec,
                };
                let res = engine.cup_prizes(&cup_input);
                for award in res.awards {
                    FinanceRepo::award_competition_prize(
                        conn,
                        season_id,
                        tid,
                        award.team_id,
                        award.amount,
                        &award.reason,
                        date,
                    )?;
                }
            }
        }

        // Persist user's trophy record
        for title in &won_user_titles {
            conn.execute(
                "INSERT INTO local_season_trophies (team_id, season_id, tournament_id, trophy_name)
                 VALUES (?1, ?2, 0, ?3)",
                params![user_team_id.0, season_id.0, title],
            )
            .map_err(db_err)?;
        }

        Ok(won_user_titles)
    }
}
