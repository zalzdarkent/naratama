use std::collections::{HashMap, HashSet};

use chrono::{Datelike, NaiveDate};
use naratama_db::club_repo::{ClubRepo, OffseasonStateRow};
use naratama_db::db_err;
use naratama_db::finance_repo::{FinanceRepo, NewLedgerEntry};
use naratama_db::market_repo::MarketRepo;
use naratama_db::reference_repo::ReferenceRepo;
use naratama_db::SeasonRepo;
use naratama_model::{
    AwardType, Confederation, CountryId, LedgerCategory, LedgerPeriod, Money, NaratamaError,
    NaratamaResult, OffseasonStep, PlayerId, Season, SeasonId, StandingRow, Team, TeamId,
    Tournament, TournamentId,
};
use naratama_sim::{compute_standings, PlayerValuationInput, PlayerValuationService};
use rand::Rng;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::finance_service::{PrizeDistributionService, SponsorshipService};
use crate::season_generator::{CareerPlan, GenerateCareerRequest, SeasonGenerator};
use crate::squad_service::SquadTacticsService;
use crate::transfer_service::TransferMarketService;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PreseasonCamp {
    TacticalRetreat,
    FitnessBootcamp,
    YouthShowcase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BoardAmbition {
    Consolidate,
    TopHalf,
    TitleChallenge,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromotionSwap {
    pub team_id: TeamId,
    pub old_division: i32,
    pub new_division: i32,
}

/// Manages the 7-step offseason transition (`0..=6`), Gala Awards, Promotion/Relegation, aging, and next-season launch.
pub struct OffseasonService;

impl OffseasonService {
    /// Step 0: closes season prizes, Gala awards, Promotion/Relegation, sponsorship bonuses, and board objectives.
    #[allow(clippy::too_many_lines)]
    pub fn begin_offseason(
        conn: &mut Connection,
        season: &Season,
        user_team_id: TeamId,
        manager_name: &str,
        date: NaiveDate,
    ) -> NaratamaResult<OffseasonStateRow> {
        let mut lines = Vec::new();
        let titles = PrizeDistributionService::award_all_season_prizes(
            conn,
            season.season_id,
            user_team_id,
            date,
        )?;

        let ref_repo = ReferenceRepo::new(conn);
        let user_team = ref_repo.team_by_id(user_team_id)?.ok_or_else(|| {
            NaratamaError::NotFound(format!("User club {} not found", user_team_id.0))
        })?;
        let user_div_before = user_team.division;

        let leagues = load_season_leagues(conn, season.season_id)?;
        let all_teams: HashMap<TeamId, Team> = ref_repo
            .all_teams()?
            .into_iter()
            .map(|t| (t.team_id, t))
            .collect();
        let team_names: HashMap<TeamId, String> = all_teams
            .iter()
            .map(|(&id, t)| (id, t.name.clone()))
            .collect();

        let mut user_rank = 0_i32;
        let mut user_league_id = TournamentId(1);
        let mut champion_id = user_team_id;

        for league in &leagues {
            let matches = SeasonRepo::new(conn)
                .matches_for_tournament(season.season_id, league.tournament_id)?;
            let league_matches: Vec<_> = matches.into_iter().filter(|m| m.phase == 0).collect();
            let mut clubs = Vec::new();
            for m in &league_matches {
                if !clubs.contains(&m.home_team_id) {
                    clubs.push(m.home_team_id);
                }
                if !clubs.contains(&m.away_team_id) {
                    clubs.push(m.away_team_id);
                }
            }
            if clubs.is_empty() {
                continue;
            }
            let table = compute_standings(&clubs, &team_names, &league_matches);
            if let Some(pos) = table.iter().position(|r| r.team_id == user_team_id) {
                user_rank = i32::try_from(pos + 1).unwrap_or(1);
                user_league_id = league.tournament_id;
                if let Some(first) = table.first() {
                    champion_id = first.team_id;
                    lines.push(format!(
                        "Finished #{} in {} ({} pts). Champion: {}",
                        user_rank, league.name, table[pos].points, first.team_name
                    ));
                }
            }
        }
        if !titles.is_empty() {
            let mut tvec: Vec<String> = titles.iter().cloned().collect();
            tvec.sort();
            lines.push(format!("Trophies won: {}", tvec.join(", ")));
        }

        // Complete finance season + Gala Awards (while match_players rows still exist!)
        FinanceRepo::complete_season(conn, season.season_id, date)?;
        let awards = ClubRepo::record_season_awards(
            conn,
            season.season_id,
            season.end_year,
            user_league_id,
            user_team_id,
            champion_id,
            manager_name,
        )?;
        for (atype, winner) in awards {
            if matches!(
                atype,
                AwardType::TopScorer
                    | AwardType::Mvp
                    | AwardType::YoungPlayer
                    | AwardType::BallonDor
            ) {
                lines.push(format!("{atype:?}: {winner}"));
            }
        }

        // Country promotion & relegation (reuse ReferenceRepo!)
        let country_leagues: Vec<Tournament> = {
            let rr = ReferenceRepo::new(conn);
            rr.leagues_for_country(user_team.country_id)?
                .into_iter()
                .filter_map(|l| rr.tournament_by_id(l.tournament_id).ok().flatten())
                .collect()
        };
        let swaps = Self::apply_country_promotion_relegation(
            conn,
            season.season_id,
            user_team.country_id,
            &country_leagues,
            &team_names,
        )?;
        if let Some(swap) = swaps.iter().find(|s| s.team_id == user_team_id) {
            if swap.new_division < user_div_before {
                lines.push(format!("PROMOTED to Division {}!", swap.new_division));
            } else if swap.new_division > user_div_before {
                lines.push(format!("RELEGATED to Division {}.", swap.new_division));
            }
        }

        // Sponsorship bonuses + board objective
        let sponsor_bonuses = SponsorshipService::evaluate_season_targets(
            conn,
            user_team_id,
            season.season_id,
            user_rank,
            date,
        )?;
        for (sname, bonus) in sponsor_bonuses {
            lines.push(format!("Sponsor bonus from {sname}: ${bonus}"));
        }
        settle_board_objective(conn, season, user_team_id, user_rank, date, &mut lines)?;

        let state = OffseasonStateRow {
            season_id: season.season_id,
            step: OffseasonStep::Summary,
            report: lines.join("\n"),
            window_weeks: 0,
            camp: None,
            ambition: None,
        };
        ClubRepo::save_offseason_state(conn, &state)?;
        Ok(state)
    }

    /// Bug #12 fix: computes `swap_count` cleanly against lower table OR falls back to `table.len()` when lower div was unsimulated.
    #[allow(clippy::too_many_lines)]
    pub fn apply_country_promotion_relegation(
        conn: &mut Connection,
        season_id: SeasonId,
        country_id: CountryId,
        leagues: &[Tournament],
        team_names: &HashMap<TeamId, String>,
    ) -> NaratamaResult<Vec<PromotionSwap>> {
        let mut sorted = leagues.to_vec();
        sorted.sort_by_key(|t| t.division_id);
        if sorted.is_empty() {
            return Ok(Vec::new());
        }

        let repo = SeasonRepo::new(conn);
        let mut tables: HashMap<i32, Vec<StandingRow>> = HashMap::new();
        for tour in &sorted {
            let fixtures = repo.matches_for_tournament(season_id, tour.tournament_id)?;
            let league_matches: Vec<_> = fixtures.into_iter().filter(|m| m.phase == 0).collect();
            let mut clubs = Vec::new();
            for m in &league_matches {
                if !clubs.contains(&m.home_team_id) {
                    clubs.push(m.home_team_id);
                }
                if !clubs.contains(&m.away_team_id) {
                    clubs.push(m.away_team_id);
                }
            }
            if !clubs.is_empty() {
                tables.insert(
                    tour.division_id,
                    compute_standings(&clubs, team_names, &league_matches),
                );
            }
        }

        let mut swaps = Vec::new();
        for upper in &sorted {
            let div = upper.division_id;
            let Some(upper_table) = tables.get(&div) else {
                continue;
            };
            let Some(lower) = sorted.iter().find(|t| t.division_id == div + 1) else {
                continue;
            };
            let spots = if upper.relegation_spots > 0 {
                upper.relegation_spots
            } else if lower.promotion_slots > 0 {
                lower.promotion_slots
            } else {
                3
            };
            let desired = usize::try_from(spots.max(0)).unwrap_or(0);

            if let Some(lower_table) = tables.get(&(div + 1)).filter(|t| !t.is_empty()) {
                let swap_count = desired.min(upper_table.len()).min(lower_table.len());
                for row in upper_table.iter().skip(upper_table.len() - swap_count) {
                    swaps.push(PromotionSwap {
                        team_id: row.team_id,
                        old_division: div,
                        new_division: div + 1,
                    });
                }
                for row in lower_table.iter().take(swap_count) {
                    swaps.push(PromotionSwap {
                        team_id: row.team_id,
                        old_division: div + 1,
                        new_division: div,
                    });
                }
            } else {
                // Bug #12 fix: fall back using `upper_table.len()` instead of `min(..., 0)`
                let mut lower_stmt = conn
                    .prepare(
                        "SELECT team_id FROM teams
                         WHERE country_id = ?1 AND division = ?2
                         ORDER BY power DESC, team_id ASC",
                    )
                    .map_err(db_err)?;
                let lower_ids: Vec<TeamId> = lower_stmt
                    .query_map(params![country_id.0, div + 1], |r| Ok(TeamId(r.get(0)?)))
                    .map_err(db_err)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(db_err)?;
                drop(lower_stmt);
                let swap_count = desired.min(upper_table.len()).min(lower_ids.len());
                for row in upper_table.iter().skip(upper_table.len() - swap_count) {
                    swaps.push(PromotionSwap {
                        team_id: row.team_id,
                        old_division: div,
                        new_division: div + 1,
                    });
                }
                for &tid in lower_ids.iter().take(swap_count) {
                    swaps.push(PromotionSwap {
                        team_id: tid,
                        old_division: div + 1,
                        new_division: div,
                    });
                }
            }
        }

        if !swaps.is_empty() {
            let tx = conn.transaction().map_err(db_err)?;
            for s in &swaps {
                tx.execute(
                    "UPDATE teams SET division = ?1 WHERE team_id = ?2",
                    params![s.new_division, s.team_id.0],
                )
                .map_err(db_err)?;
            }
            tx.commit().map_err(db_err)?;
        }
        Ok(swaps)
    }

    /// Step 1 -> 2: recall finished loans, age players + retire veterans, replenish AI academies, roll over AI contracts.
    pub fn enter_contracts(
        conn: &mut Connection,
        season: &Season,
        user_team_id: TeamId,
        date: NaiveDate,
        rng: &mut impl Rng,
    ) -> NaratamaResult<OffseasonStateRow> {
        let _recalled = MarketRepo::recall_finished_loans(conn, season.end_year)?;

        let tx = conn.transaction().map_err(db_err)?;
        tx.execute("UPDATE players SET age = age + 1 WHERE status_id = 1", [])
            .map_err(db_err)?;
        tx.execute(
            "UPDATE players SET status_id = 2
             WHERE status_id = 1 AND (age >= 41 OR (age >= 37 AND power < 55))",
            [],
        )
        .map_err(db_err)?;
        tx.execute(
            "DELETE FROM team_players WHERE player_id IN (SELECT player_id FROM players WHERE status_id = 2)",
            [],
        )
        .map_err(db_err)?;
        tx.execute(
            "UPDATE contracts SET status = 0 WHERE player_id IN (SELECT player_id FROM players WHERE status_id = 2)",
            [],
        )
        .map_err(db_err)?;
        // Roll over expiring AI contracts automatically
        let new_end_year = (season.end_year + 2).to_string();
        tx.execute(
            "UPDATE contracts SET end_date = ?1 || substr(end_date, 5)
             WHERE status = 1 AND team_id != ?2 AND end_date IS NOT NULL
               AND CAST(substr(end_date, 1, 4) AS INTEGER) <= ?3",
            params![new_end_year, user_team_id.0, season.end_year],
        )
        .map_err(db_err)?;
        tx.commit().map_err(db_err)?;

        let replenished = SquadTacticsService::replenish_ai_academies(
            conn,
            user_team_id,
            season.season_id,
            date,
            rng,
        )?;

        let state = OffseasonStateRow {
            season_id: season.season_id,
            step: OffseasonStep::Contracts,
            report: format!("Season aging & AI contract rollover complete ({replenished} academy regens promoted)."),
            window_weeks: 0,
            camp: None,
            ambition: None,
        };
        ClubRepo::save_offseason_state(conn, &state)?;
        Ok(state)
    }

    /// Step 2 -> 3: performance boost, player development/decline, MV recalculation, explicit transaction archive (`Bug #19`).
    pub fn enter_development(
        conn: &mut Connection,
        season: &Season,
        user_team_id: TeamId,
    ) -> NaratamaResult<OffseasonStateRow> {
        // Develop young & prime players, age-decline veterans, then recalculate MVs
        let tx = conn.transaction().map_err(db_err)?;
        tx.execute(
            "UPDATE players
             SET power = power + MIN(potential - power, 1 + (player_id * 7 + age * 13) % 3)
             WHERE status_id = 1 AND power < potential AND age <= 23",
            [],
        )
        .map_err(db_err)?;
        tx.execute(
            "UPDATE players
             SET power = power + MIN(potential - power, (player_id * 7 + age * 13) % 2)
             WHERE status_id = 1 AND power < potential AND age BETWEEN 24 AND 27",
            [],
        )
        .map_err(db_err)?;
        tx.execute(
            "UPDATE players
             SET power = MAX(40, power - CASE
                 WHEN age >= 34 THEN 1 + (player_id * 5 + age * 11) % 2
                 ELSE (player_id * 5 + age * 11) % 3 END)
             WHERE status_id = 1 AND age >= 31",
            [],
        )
        .map_err(db_err)?;

        // Update market values for developed / declined players
        let mut mv_stmt = tx
            .prepare(
                "SELECT player_id, power, potential, age, position_id, value
                 FROM players WHERE status_id = 1 AND ((power < potential AND age <= 27) OR age >= 31)",
            )
            .map_err(db_err)?;
        let rows: Vec<(i64, i32, i32, i32, i32, i64)> = mv_stmt
            .query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        drop(mv_stmt);

        let val_svc = PlayerValuationService::default();
        for (pid, pwr, pot, age, pos, cur_val) in rows {
            let target = val_svc.calculate_target_market_value(PlayerValuationInput {
                power: pwr,
                potential: Some(pot),
                age,
                position_id: pos,
                confederation: Confederation::Uefa,
                team_power: 72,
                contract_months_remaining: 24,
                ..PlayerValuationInput::default()
            });
            let next_val = PlayerValuationService::round_valuation_cleanly(
                val_svc.smooth_value_update(cur_val, target, None),
            );
            tx.execute(
                "UPDATE players SET value = ?1 WHERE player_id = ?2",
                params![next_val, pid],
            )
            .map_err(db_err)?;
        }

        // Refresh all season club overalls inside tx
        let mut st_stmt = tx
            .prepare("SELECT DISTINCT team_id FROM season_teams WHERE season_id = ?1")
            .map_err(db_err)?;
        let club_ids: Vec<TeamId> = st_stmt
            .query_map(params![season.season_id.0], |r| Ok(TeamId(r.get(0)?)))
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        drop(st_stmt);
        for tid in club_ids {
            ClubRepo::refresh_team_power_tx(&tx, tid)?;
        }
        let _ = user_team_id;
        tx.commit().map_err(db_err)?;

        // Bug #19 fix: archive stats & delete transient match_players/match_incidents explicitly in 1 transaction
        ClubRepo::archive_season_stats(conn, season.season_id)?;

        let state = OffseasonStateRow {
            season_id: season.season_id,
            step: OffseasonStep::Development,
            report:
                "Player development, market valuation refresh, and season stats archive completed."
                    .into(),
            window_weeks: 0,
            camp: None,
            ambition: None,
        };
        ClubRepo::save_offseason_state(conn, &state)?;
        Ok(state)
    }

    /// Step 3: enter open transfer window phase.
    pub fn enter_transfers(
        conn: &mut Connection,
        season: &Season,
        user_team_id: TeamId,
        date: NaiveDate,
        rng: &mut impl Rng,
    ) -> NaratamaResult<OffseasonStateRow> {
        let moves =
            TransferMarketService::run_market_ai(conn, season.season_id, user_team_id, date, rng)?;
        let state = OffseasonStateRow {
            season_id: season.season_id,
            step: OffseasonStep::Transfers,
            report: format!("Summer transfer window active ({moves} early AI deals)."),
            window_weeks: 1,
            camp: None,
            ambition: None,
        };
        ClubRepo::save_offseason_state(conn, &state)?;
        Ok(state)
    }

    /// Step 4: apply user pre-season training camp focus.
    pub fn apply_preseason_camp(
        conn: &mut Connection,
        season: &Season,
        user_team_id: TeamId,
        camp: PreseasonCamp,
    ) -> NaratamaResult<OffseasonStateRow> {
        let camp_name = match camp {
            PreseasonCamp::TacticalRetreat => "TacticalRetreat",
            PreseasonCamp::FitnessBootcamp => "FitnessBootcamp",
            PreseasonCamp::YouthShowcase => "YouthShowcase",
        };
        let tx = conn.transaction().map_err(db_err)?;
        match camp {
            PreseasonCamp::FitnessBootcamp => {
                tx.execute(
                    "UPDATE players SET fitness = 100
                     WHERE status_id = 1 AND player_id IN (SELECT player_id FROM team_players WHERE team_id = ?1)",
                    params![user_team_id.0],
                )
                .map_err(db_err)?;
            }
            PreseasonCamp::TacticalRetreat => {
                tx.execute(
                    "UPDATE players SET fitness = MIN(100, fitness + 15)
                     WHERE status_id = 1 AND player_id IN (SELECT player_id FROM team_players WHERE team_id = ?1)",
                    params![user_team_id.0],
                )
                .map_err(db_err)?;
            }
            PreseasonCamp::YouthShowcase => {
                let mut stmt = tx
                    .prepare(
                        "SELECT p.player_id FROM players p
                         JOIN team_players tp ON tp.player_id = p.player_id
                         WHERE tp.team_id = ?1 AND p.status_id = 1 AND p.age <= 21 AND p.power < p.potential
                         ORDER BY p.potential DESC LIMIT 3",
                    )
                    .map_err(db_err)?;
                let pids: Vec<i64> = stmt
                    .query_map(params![user_team_id.0], |r| r.get(0))
                    .map_err(db_err)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(db_err)?;
                drop(stmt);
                for pid in pids {
                    tx.execute(
                        "UPDATE players SET power = MIN(potential, power + 1) WHERE player_id = ?1",
                        params![pid],
                    )
                    .map_err(db_err)?;
                }
                ClubRepo::refresh_team_power_tx(&tx, user_team_id)?;
            }
        }
        tx.commit().map_err(db_err)?;

        let state = OffseasonStateRow {
            season_id: season.season_id,
            step: OffseasonStep::Preseason,
            report: format!("Pre-season camp completed: {camp_name}"),
            window_weeks: 1,
            camp: Some(camp_name.to_string()),
            ambition: None,
        };
        ClubRepo::save_offseason_state(conn, &state)?;
        Ok(state)
    }

    /// Step 5: agree next-season board ambition & grant budget upfront.
    pub fn choose_board_ambition(
        conn: &mut Connection,
        season: &Season,
        user_team_id: TeamId,
        ambition: BoardAmbition,
        date: NaiveDate,
    ) -> NaratamaResult<OffseasonStateRow> {
        let next_start_year = season.start_year + 1;
        let team = ReferenceRepo::new(conn)
            .team_by_id(user_team_id)?
            .ok_or_else(|| NaratamaError::NotFound("User team not found".into()))?;
        let base = (team.budget / 20).max(300_000);
        let (target_rank, advance, bonus, label) = match ambition {
            BoardAmbition::Consolidate => (14, base / 2, base, "Consolidate"),
            BoardAmbition::TopHalf => (8, base, base * 2, "TopHalf"),
            BoardAmbition::TitleChallenge => (3, (base * 3) / 2, base * 3, "TitleChallenge"),
        };

        let tx = conn.transaction().map_err(db_err)?;
        tx.execute(
            "INSERT INTO board_objectives (start_year, team_id, target_rank, bonus, status)
             VALUES (?1, ?2, ?3, ?4, 'pending')
             ON CONFLICT(start_year, team_id) DO UPDATE SET
                target_rank = excluded.target_rank,
                bonus = excluded.bonus,
                status = 'pending'",
            params![next_start_year, user_team_id.0, target_rank, bonus],
        )
        .map_err(db_err)?;
        FinanceRepo::record_tx(
            &tx,
            &NewLedgerEntry {
                team_id: user_team_id,
                season_id: season.season_id,
                entry_date: date,
                period: LedgerPeriod::Board,
                category: LedgerCategory::BoardFunding,
                subcategory: "board_grant",
                amount: advance,
                ref_type: "board_grant",
                ref_id: Some(season.season_id.0),
                description: "Pre-season board management grant",
            },
        )?;
        tx.commit().map_err(db_err)?;

        let state = OffseasonStateRow {
            season_id: season.season_id,
            step: OffseasonStep::Board,
            report: format!("Board target agreed ({label}): top {target_rank} finish, upfront grant ${advance}."),
            window_weeks: 1,
            camp: None,
            ambition: Some(label.to_string()),
        };
        ClubRepo::save_offseason_state(conn, &state)?;
        Ok(state)
    }

    /// Step 6 -> Done: generates next season fixtures across all country leagues, cup, and continental tournaments.
    pub fn start_next_season(
        conn: &mut Connection,
        completed_season: &Season,
        user_team_id: TeamId,
        rng: &mut impl Rng,
    ) -> NaratamaResult<CareerPlan> {
        let ref_repo = ReferenceRepo::new(conn);
        let user_team = ref_repo
            .team_by_id(user_team_id)?
            .ok_or_else(|| NaratamaError::NotFound("User team not found".into()))?;
        let domestic_rows = ref_repo.leagues_for_country(user_team.country_id)?;
        let all_leagues: Vec<Tournament> = domestic_rows
            .iter()
            .filter_map(|l| ref_repo.tournament_by_id(l.tournament_id).ok().flatten())
            .collect();
        let next_league = all_leagues
            .iter()
            .find(|t| t.division_id == user_team.division)
            .or_else(|| all_leagues.first())
            .cloned()
            .ok_or_else(|| {
                NaratamaError::NotFound("No domestic league found for user division".into())
            })?;
        let cup = ref_repo.national_cup(user_team.country_id)?;
        let internationals = ref_repo.international_cups()?;

        let next_year = completed_season.start_year + 1;
        let plan = SeasonGenerator::generate_career(
            conn,
            &GenerateCareerRequest {
                country_id: user_team.country_id,
                league: next_league.clone(),
                cup,
                internationals,
                other_leagues: all_leagues
                    .into_iter()
                    .filter(|t| t.tournament_id != next_league.tournament_id)
                    .collect(),
                start_year: next_year,
                user_team_id: Some(user_team_id),
                include_preseason_friendlies: true,
            },
            rng,
        )?;

        let season_start = NaiveDate::from_ymd_opt(next_year, 7, 1).unwrap_or(NaiveDate::MIN);
        MarketRepo::ensure_windows_for_season(conn, plan.season.season_id, next_year)?;
        let _ = TransferMarketService::process_due_scheduled_transfers(
            conn,
            plan.season.season_id,
            season_start,
        )?;

        conn.execute("DELETE FROM offseason_state", [])
            .map_err(db_err)?;

        let _ = PlayerId(0);
        let _ = HashSet::<i64>::new();
        Ok(plan)
    }
}

fn load_season_leagues(conn: &Connection, season_id: SeasonId) -> NaratamaResult<Vec<Tournament>> {
    let mut stmt = conn
        .prepare(
            "SELECT st.tournament_id
             FROM season_tournaments st
             JOIN tournaments tu ON tu.tournament_id = st.tournament_id
             WHERE st.season_id = ?1 AND tu.organization = 1
             ORDER BY tu.division_id ASC",
        )
        .map_err(db_err)?;
    let ids: Vec<TournamentId> = stmt
        .query_map(params![season_id.0], |r| Ok(TournamentId(r.get(0)?)))
        .map_err(db_err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_err)?;
    drop(stmt);
    let ref_repo = ReferenceRepo::new(conn);
    Ok(ids
        .into_iter()
        .filter_map(|id| ref_repo.tournament_by_id(id).ok().flatten())
        .collect())
}

fn settle_board_objective(
    conn: &mut Connection,
    season: &Season,
    user_team_id: TeamId,
    user_rank: i32,
    date: NaiveDate,
    lines: &mut Vec<String>,
) -> NaratamaResult<()> {
    if user_rank <= 0 {
        return Ok(());
    }
    let row: Option<(i32, Money)> = conn
        .query_row(
            "SELECT target_rank, bonus FROM board_objectives
             WHERE start_year = ?1 AND team_id = ?2 AND status = 'pending'",
            params![season.start_year, user_team_id.0],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(db_err)?;
    let Some((target, bonus)) = row else {
        return Ok(());
    };

    let met = user_rank <= target;
    let tx = conn.transaction().map_err(db_err)?;
    tx.execute(
        "UPDATE board_objectives SET status = ?1 WHERE start_year = ?2 AND team_id = ?3",
        params![
            if met { "met" } else { "failed" },
            season.start_year,
            user_team_id.0
        ],
    )
    .map_err(db_err)?;
    if met && bonus > 0 {
        FinanceRepo::record_tx(
            &tx,
            &NewLedgerEntry {
                team_id: user_team_id,
                season_id: season.season_id,
                entry_date: date,
                period: LedgerPeriod::Annual,
                category: LedgerCategory::BoardFunding,
                subcategory: "board_objective",
                amount: bonus,
                ref_type: "board_objective",
                ref_id: Some(season.season_id.0),
                description: "Board league objective bonus",
            },
        )?;
        lines.push(format!(
            "Board objective MET (top {target}): bonus ${bonus}."
        ));
    } else {
        lines.push(format!(
            "Board objective missed (required top {target}, finished #{user_rank})."
        ));
    }
    tx.commit().map_err(db_err)?;
    let _ = date.year();
    Ok(())
}
