use std::collections::{HashMap, HashSet};

use chrono::{Duration, NaiveDate};
use naratama_db::club_repo::{ClubRepo, MatchPersistPayload, MatchPlayerInsert, RedCardBanInput};
use naratama_db::finance_repo::{FinanceRepo, NewLedgerEntry};
use naratama_db::intl_repo::IntlRepo;
use naratama_db::market_repo::MarketRepo;
use naratama_db::reference_repo::ReferenceRepo;
use naratama_db::{db_err, SeasonRepo};
use naratama_model::{
    CompetitionType, FixtureMatch, LedgerCategory, LedgerPeriod, MatchId, MatchStatus,
    NaratamaResult, Player, PlayerId, SeasonId, Stadium, StaffRole, Team, TeamId, TournamentOrg,
};
use naratama_sim::{FinanceCalc, Formation, MatchEngine, MatchEventType, MatchInput, MatchResult};
use rand::Rng;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::finance_service::{BroadcastService, FinanceService, SponsorshipService};
use crate::intl_service::InternationalSimulationService;
use crate::national_team_service::{NationalTeamSelectionService, NationalTeamService};
use crate::season_generator::SeasonGenerator;
use crate::squad_service::SquadTacticsService;
use crate::transfer_service::TransferMarketService;

/// Everything the engine needs to simulate one fixture, built the same way for live and batch play.
pub struct PreparedFixture {
    pub input: MatchInput,
    pub banned: HashSet<PlayerId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchdayOutcome {
    pub match_id: MatchId,
    pub home_team_id: TeamId,
    pub away_team_id: TeamId,
    pub home_goals: i32,
    pub away_goals: i32,
    pub winner_id: Option<TeamId>,
}

/// Orchestrates match simulation, competition-scoped suspensions, injuries, cup progression, and 18-step housekeeping.
pub struct MatchdayService;

impl MatchdayService {
    /// Builds the simulation input for one fixture; shared by the live match and the batch matchday.
    #[allow(clippy::too_many_arguments)]
    fn prepare_fixture(
        conn: &Connection,
        fixture: &FixtureMatch,
        comp_type: CompetitionType,
        home_team: Option<&Team>,
        away_team: Option<&Team>,
        stadium: Option<&Stadium>,
        squads: &HashMap<TeamId, Vec<Player>>,
        user_team_id: TeamId,
        date: NaiveDate,
    ) -> NaratamaResult<PreparedFixture> {
        let home_power = home_team.map_or(65, |t| t.power);
        let away_power = away_team.map_or(65, |t| t.power);
        let (cap_gen, cap_stand, cap_ter, cap_box) =
            stadium.map_or((15_000, 10_000, 5_000, 500), |s| {
                (
                    s.capacity_general,
                    s.capacity_stand,
                    s.capacity_terrace,
                    s.capacity_boxes,
                )
            });

        let mut banned = ClubRepo::banned_players_for_competition(
            conn,
            fixture.season_id,
            comp_type,
            fixture.home_team_id,
            fixture.away_team_id,
        )?;
        banned.extend(IntlRepo::active_duty_player_ids_on_date(conn, date)?);

        let home_squad = squads
            .get(&fixture.home_team_id)
            .cloned()
            .unwrap_or_default();
        let away_squad = squads
            .get(&fixture.away_team_id)
            .cloned()
            .unwrap_or_default();
        let involves_user =
            fixture.home_team_id == user_team_id || fixture.away_team_id == user_team_id;

        let (home_form_str, home_lineup_ids) = if involves_user {
            ClubRepo::lineup_and_formation(conn, fixture.home_team_id)?
        } else {
            ("4-4-2".to_string(), Vec::new())
        };
        let (away_form_str, away_lineup_ids) = if involves_user {
            ClubRepo::lineup_and_formation(conn, fixture.away_team_id)?
        } else {
            ("4-4-2".to_string(), Vec::new())
        };

        let (home_tac_bonus, home_inj_red) =
            Self::user_staff_bonuses(conn, fixture.home_team_id, user_team_id)?;
        let (away_tac_bonus, away_inj_red) =
            Self::user_staff_bonuses(conn, fixture.away_team_id, user_team_id)?;

        Ok(PreparedFixture {
            banned,
            input: MatchInput {
                home_squad,
                away_squad,
                home_team_id: fixture.home_team_id,
                away_team_id: fixture.away_team_id,
                capacity_general: cap_gen,
                capacity_stand: cap_stand,
                capacity_terrace: cap_ter,
                capacity_boxes: cap_box,
                ticket_price_general: stadium
                    .map_or(Stadium::DEFAULT_GENERAL_PRICE, |s| s.ticket_price_general),
                ticket_price_stand: stadium
                    .map_or(Stadium::DEFAULT_STAND_PRICE, |s| s.ticket_price_stand),
                ticket_price_terrace: stadium
                    .map_or(Stadium::DEFAULT_TERRACE_PRICE, |s| s.ticket_price_terrace),
                ticket_price_boxes: stadium
                    .map_or(Stadium::DEFAULT_BOXES_PRICE, |s| s.ticket_price_boxes),
                home_power,
                away_power,
                home_formation: Formation::parse(Some(&home_form_str)),
                away_formation: Formation::parse(Some(&away_form_str)),
                home_lineup_ids,
                away_lineup_ids,
                home_tactical_bonus: home_tac_bonus,
                away_tactical_bonus: away_tac_bonus,
                home_injury_reduction: home_inj_red.min(0.60),
                away_injury_reduction: away_inj_red.min(0.60),
                neutral_venue: fixture.neutral_venue,
                extra_time: fixture.extra_time,
                ..MatchInput::default()
            },
        })
    }

    fn user_staff_bonuses(
        conn: &Connection,
        team_id: TeamId,
        user_team_id: TeamId,
    ) -> NaratamaResult<(f64, f64)> {
        if team_id != user_team_id {
            return Ok((0.0, 0.0));
        }
        let mut tac_bonus = 0.0;
        let fac = ClubRepo::facilities(conn, user_team_id)?;
        let mut inj_red = f64::from((fac.medical_level - 1).max(0)) * 0.08;
        for st in ClubRepo::staff_for_team(conn, user_team_id)? {
            match st.role {
                StaffRole::TacticalAnalyst => tac_bonus += f64::from(st.stars) * 0.015,
                StaffRole::HeadPhysio => inj_red += f64::from(st.stars) * 0.05,
                StaffRole::HeadScout => {}
            }
        }
        Ok((tac_bonus, inj_red))
    }

    /// Prepares the user's next fixture on `date` for an interactive live match.
    pub fn prepare_live_match(
        conn: &Connection,
        season_id: SeasonId,
        date: NaiveDate,
        user_team_id: TeamId,
    ) -> NaratamaResult<Option<(FixtureMatch, PreparedFixture)>> {
        let Some(fixture) = SeasonRepo::new(conn)
            .matches_on_date(season_id, date)?
            .into_iter()
            .find(|m| {
                m.status == MatchStatus::Scheduled
                    && (m.home_team_id == user_team_id || m.away_team_id == user_team_id)
            })
        else {
            return Ok(None);
        };

        let org = load_tournament_orgs(conn)?
            .get(&fixture.tournament_id.0)
            .copied()
            .unwrap_or(TournamentOrg::League);
        let comp_type = match org {
            TournamentOrg::League => CompetitionType::League,
            TournamentOrg::NationalCup => CompetitionType::DomesticCup,
            TournamentOrg::International => CompetitionType::Continental,
            TournamentOrg::Friendly => CompetitionType::Friendly,
        };
        let ref_repo = ReferenceRepo::new(conn);
        let home_team = ref_repo.team_by_id(fixture.home_team_id)?;
        let away_team = ref_repo.team_by_id(fixture.away_team_id)?;
        let stadium = ref_repo.stadium_by_id(fixture.stadium_id)?;
        let mut involved = HashSet::new();
        involved.insert(fixture.home_team_id);
        involved.insert(fixture.away_team_id);
        let squads = load_squads_for_teams(conn, &involved)?;
        let prepared = Self::prepare_fixture(
            conn,
            &fixture,
            comp_type,
            home_team.as_ref(),
            away_team.as_ref(),
            stadium.as_ref(),
            &squads,
            user_team_id,
            date,
        )?;
        Ok(Some((fixture, prepared)))
    }
    /// Simulates all scheduled matches on `date` (and earlier postponed/missed fixtures if requested),
    /// awards home matchday finances via `FinanceRepo::record_tx`, updates competition-scoped bans and injuries,
    /// and advances knockout cup rounds when a phase completes.
    #[allow(clippy::too_many_lines, clippy::too_many_arguments)]
    pub fn simulate_matchday(
        conn: &mut Connection,
        season_id: SeasonId,
        date: NaiveDate,
        skip_match_id: Option<MatchId>,
        record_user_events: bool,
        user_team_id: TeamId,
        rng: &mut impl Rng,
        preset_result: Option<&(MatchId, MatchResult)>,
    ) -> NaratamaResult<Vec<MatchdayOutcome>> {
        let fixtures = SeasonRepo::new(conn).matches_on_date(season_id, date)?;
        let pending: Vec<FixtureMatch> = fixtures
            .into_iter()
            .filter(|m| m.status == MatchStatus::Scheduled && Some(m.match_id) != skip_match_id)
            .collect();
        if pending.is_empty() {
            let _ = InternationalSimulationService::simulate_pending_matches_through_date(
                conn, season_id, date, rng,
            )?;
            return Ok(Vec::new());
        }

        // Preload teams, stadiums, squads, and tournament metadata to keep whole-world matchdays fast.
        let ref_repo = ReferenceRepo::new(conn);
        let teams_map: HashMap<TeamId, Team> = ref_repo
            .all_teams()?
            .into_iter()
            .map(|t| (t.team_id, t))
            .collect();
        let stadiums_map: HashMap<i64, Stadium> = load_all_stadiums(conn)?;
        let org_map: HashMap<i64, TournamentOrg> = load_tournament_orgs(conn)?;

        let mut involved_teams: HashSet<TeamId> = HashSet::new();
        let mut touched_tournaments: HashSet<i64> = HashSet::new();
        for m in &pending {
            involved_teams.insert(m.home_team_id);
            involved_teams.insert(m.away_team_id);
            touched_tournaments.insert(m.tournament_id.0);
        }

        let squads = load_squads_for_teams(conn, &involved_teams)?;
        let mut outcomes = Vec::with_capacity(pending.len());

        for fixture in pending {
            let org = org_map
                .get(&fixture.tournament_id.0)
                .copied()
                .unwrap_or(TournamentOrg::League);
            let comp_type = match org {
                TournamentOrg::League => CompetitionType::League,
                TournamentOrg::NationalCup => CompetitionType::DomesticCup,
                TournamentOrg::International => CompetitionType::Continental,
                TournamentOrg::Friendly => CompetitionType::Friendly,
            };

            let involves_user =
                fixture.home_team_id == user_team_id || fixture.away_team_id == user_team_id;
            let prepared = Self::prepare_fixture(
                conn,
                &fixture,
                comp_type,
                teams_map.get(&fixture.home_team_id),
                teams_map.get(&fixture.away_team_id),
                stadiums_map.get(&fixture.stadium_id.0),
                &squads,
                user_team_id,
                date,
            )?;
            let banned = prepared.banned;
            let input = prepared.input;

            let record_events = record_user_events && involves_user;
            let sim: MatchResult = match preset_result {
                Some((match_id, result)) if *match_id == fixture.match_id => result.clone(),
                _ => MatchEngine::simulate(rng, &input, record_events, &banned),
            };

            // Knockout tiebreaker if level after 90'/120' in Cup/Continental knockout
            let mut winner_id = match sim.home_goals.cmp(&sim.away_goals) {
                std::cmp::Ordering::Greater => Some(fixture.home_team_id),
                std::cmp::Ordering::Less => Some(fixture.away_team_id),
                std::cmp::Ordering::Equal => None,
            };
            let mut pen_home = None;
            let mut pen_away = None;
            if winner_id.is_none() && fixture.phase > 0 {
                let shootout = MatchEngine::penalty_shootout(rng, &input);
                pen_home = Some(shootout.home_score);
                pen_away = Some(shootout.away_score);
                winner_id = Some(if shootout.home_score >= shootout.away_score {
                    fixture.home_team_id
                } else {
                    fixture.away_team_id
                });
            }

            let match_date = fixture.date.unwrap_or(date);
            let mut red_card_bans = Vec::new();
            let mut injury_return_dates = Vec::new();
            let mut seen_bans = HashSet::new();
            let mut seen_inj = HashSet::new();

            for ev in &sim.events {
                if let Some(pid) = ev.player_id {
                    if ev.event_type == MatchEventType::RedCard && seen_bans.insert(pid) {
                        let is_second_yellow = ev
                            .detail
                            .as_deref()
                            .is_some_and(|d| d.contains("second") || d.contains("2nd"));
                        red_card_bans.push(RedCardBanInput {
                            player_id: pid,
                            matches_banned: if is_second_yellow { 1 } else { 2 },
                        });
                    } else if ev.event_type == MatchEventType::Injury && seen_inj.insert(pid) {
                        let days = rng.gen_range(1..=14);
                        injury_return_dates.push((pid, match_date + Duration::days(days)));
                    }
                }
            }
            // If `record_events` was false, derive red cards directly from `sim.player_rows`.
            for pr in &sim.player_rows {
                if pr.red_cards > 0 && seen_bans.insert(pr.player_id) {
                    let is_second_yellow = pr.yellow_cards >= 2;
                    red_card_bans.push(RedCardBanInput {
                        player_id: pr.player_id,
                        matches_banned: if is_second_yellow { 1 } else { 2 },
                    });
                }
            }

            let player_rows: Vec<MatchPlayerInsert> = sim
                .player_rows
                .iter()
                .map(|r| MatchPlayerInsert {
                    player_id: r.player_id,
                    team_id: r.team_id,
                    starter: r.starter,
                    minutes: r.minutes,
                    goals: r.goals,
                    assists: r.assists,
                    yellow_cards: r.yellow_cards,
                    red_cards: r.red_cards,
                    mvp: r.mvp,
                    rating: r.rating,
                    points: r.points,
                })
                .collect();

            let total_att = sim.attendance_general
                + sim.attendance_stand
                + sim.attendance_terrace
                + sim.attendance_boxes;
            let incidents_json: Vec<String> = if record_events {
                sim.events
                    .iter()
                    .filter_map(|e| serde_json::to_string(e).ok())
                    .collect()
            } else {
                Vec::new()
            };

            ClubRepo::save_match_result(
                conn,
                &MatchPersistPayload {
                    match_id: fixture.match_id,
                    season_id,
                    tournament_id: fixture.tournament_id,
                    competition_type: comp_type,
                    home_team_id: fixture.home_team_id,
                    away_team_id: fixture.away_team_id,
                    home_goals: sim.home_goals,
                    away_goals: sim.away_goals,
                    winner_id,
                    penalty_home_goals: pen_home,
                    penalty_away_goals: pen_away,
                    attendance_total: total_att,
                    ticket_revenue: sim.revenue,
                    match_date,
                    player_rows,
                    incidents_json,
                    red_card_bans,
                    injury_return_dates,
                    team_stats: naratama_db::club_repo::MatchTeamStats {
                        home_shots: sim.home_shots,
                        away_shots: sim.away_shots,
                        home_shots_on_target: sim.home_shots_on_target,
                        away_shots_on_target: sim.away_shots_on_target,
                        home_saves: sim.home_saves,
                        away_saves: sim.away_saves,
                        home_possession: sim.home_possession,
                    },
                },
            )?;

            // Home club matchday finance breakdown
            if !fixture.neutral_venue && comp_type != CompetitionType::Friendly {
                let bd = FinanceCalc::matchday(sim.revenue, total_att);
                let mut fin_entries = Vec::new();
                let ticket_gross = bd.gross_tickets + bd.hospitality;
                if ticket_gross > 0 {
                    fin_entries.push(NewLedgerEntry {
                        team_id: fixture.home_team_id,
                        season_id,
                        entry_date: match_date,
                        period: LedgerPeriod::Matchday,
                        category: LedgerCategory::MatchdayTickets,
                        subcategory: "matchday_tickets",
                        amount: ticket_gross,
                        ref_type: "match",
                        ref_id: Some(fixture.match_id.0),
                        description: "Matchday ticket & hospitality income",
                    });
                }
                if bd.merchandise > 0 {
                    fin_entries.push(NewLedgerEntry {
                        team_id: fixture.home_team_id,
                        season_id,
                        entry_date: match_date,
                        period: LedgerPeriod::Matchday,
                        category: LedgerCategory::MatchdayMerch,
                        subcategory: "matchday_merch",
                        amount: bd.merchandise,
                        ref_type: "match",
                        ref_id: Some(fixture.match_id.0),
                        description: "Matchday stadium merchandise income",
                    });
                }
                if bd.operating_cost > 0 {
                    fin_entries.push(NewLedgerEntry {
                        team_id: fixture.home_team_id,
                        season_id,
                        entry_date: match_date,
                        period: LedgerPeriod::Matchday,
                        category: LedgerCategory::MatchdayOps,
                        subcategory: "matchday_ops",
                        amount: -bd.operating_cost,
                        ref_type: "match",
                        ref_id: Some(fixture.match_id.0),
                        description: "Matchday stadium operating costs",
                    });
                }
                FinanceRepo::record_batch(conn, &fin_entries)?;
            }

            outcomes.push(MatchdayOutcome {
                match_id: fixture.match_id,
                home_team_id: fixture.home_team_id,
                away_team_id: fixture.away_team_id,
                home_goals: sim.home_goals,
                away_goals: sim.away_goals,
                winner_id,
            });
        }

        // Check knockout progression for any tournament played today
        for tid in touched_tournaments {
            let org = org_map.get(&tid).copied().unwrap_or(TournamentOrg::League);
            if org == TournamentOrg::NationalCup || org == TournamentOrg::International {
                let _ = SeasonGenerator::maybe_schedule_next_cup_round(
                    conn,
                    season_id,
                    naratama_model::TournamentId(tid),
                    &teams_map,
                    date,
                )?;
            }
        }

        // Simulate scheduled international matches through today and advance any international knockouts
        let _ = InternationalSimulationService::simulate_pending_matches_through_date(
            conn, season_id, date, rng,
        )?;

        Ok(outcomes)
    }

    /// Runs the 18-step Weekly Housekeeping loop on `current_date` (no wall-clock dates anywhere).
    #[allow(clippy::too_many_lines)]
    pub fn weekly_housekeeping(
        conn: &mut Connection,
        season_id: SeasonId,
        user_team_id: TeamId,
        current_date: NaiveDate,
        rng: &mut impl Rng,
    ) -> NaratamaResult<()> {
        // 1. Legacy untyped suspension tick + International duty release/callups
        ClubRepo::tick_untyped_suspensions(conn, season_id)?;
        let released = NationalTeamSelectionService::release_concluded_duties(conn, current_date)?;
        if released > 0 {
            let _ = NationalTeamService::snapshot_world_rankings(conn)?;
        }
        let _ = NationalTeamSelectionService::ensure_international_callups(
            conn,
            season_id,
            Some(user_team_id),
            current_date,
        )?;

        // 2. Clear recovered injuries whose return date has passed + weekly squad fitness recovery
        ClubRepo::clear_healed_injuries(conn, current_date)?;
        conn.execute(
            "UPDATE players SET fitness = CASE
                WHEN fitness <= 40 THEN MIN(45, fitness + 2)
                ELSE MIN(100, fitness + 6)
             END
             WHERE COALESCE(status_id, 1) = 1
               AND injury_return_date IS NULL
               AND player_id IN (
                   SELECT tp.player_id
                   FROM team_players tp
                   JOIN season_teams st ON st.team_id = tp.team_id
                   WHERE st.season_id = ?1
               )",
            params![season_id.0],
        )
        .map_err(db_err)?;

        // 3. Medical Centre + Head Physio extra fitness bonus for user club
        let fac = ClubRepo::facilities(conn, user_team_id)?;
        let physio_stars: i32 = ClubRepo::staff_for_team(conn, user_team_id)?
            .into_iter()
            .find(|s| s.role == StaffRole::HeadPhysio)
            .map_or(0, |s| s.stars);
        let extra_rec = (fac.medical_level - 1) * 2 + (physio_stars + 1) / 2;
        if extra_rec > 0 {
            conn.execute(
                "UPDATE players SET fitness = MIN(100, fitness + ?1)
                 WHERE COALESCE(status_id, 1) = 1 AND injury_return_date IS NULL
                   AND player_id IN (SELECT player_id FROM team_players WHERE team_id = ?2)",
                params![extra_rec, user_team_id.0],
            )
            .map_err(db_err)?;
        }

        // 4. Weekly salaries for all season clubs
        FinanceService::pay_weekly_player_salaries(conn, season_id, current_date)?;

        // 5. Sponsorship weekly payouts (ensuring 3 slots exist first)
        SponsorshipService::ensure_season_deals(conn, season_id, user_team_id, current_date, rng)?;
        SponsorshipService::pay_weekly_all(conn, season_id, current_date)?;

        // 6. User scout wages + coaching/medical staff wages
        let scout_wages: i64 = MarketRepo::hired_scouts(conn, user_team_id)?
            .into_iter()
            .map(|s| s.salary)
            .sum();
        let staff_wages = ClubRepo::total_weekly_staff_wages(conn, user_team_id)?;
        let total_staff_out = scout_wages + staff_wages;
        if total_staff_out > 0 {
            FinanceRepo::record(
                conn,
                &NewLedgerEntry {
                    team_id: user_team_id,
                    season_id,
                    entry_date: current_date,
                    period: LedgerPeriod::Weekly,
                    category: LedgerCategory::StaffWages,
                    subcategory: "staff_wages",
                    amount: -total_staff_out,
                    ref_type: "staff_wages",
                    ref_id: Some(season_id.0),
                    description: "Weekly scouting & professional staff wages",
                },
            )?;
        }

        // 7. Broadcast due installment
        if let Some((start_year, end_year)) = SeasonRepo::new(conn).season_years(season_id)? {
            let bc = BroadcastService::pay_due_installments(
                conn,
                season_id,
                start_year,
                end_year,
                user_team_id,
                current_date,
            )?;
            if bc.paid {
                let _ = ClubRepo::insert_message(
                    conn,
                    "Broadcast installment received",
                    &format!("Broadcast revenue received: ${}", bc.amount),
                    current_date,
                    "League",
                    Some(user_team_id),
                    None,
                );
            }

            // Ensure country transfer windows exist for this season
            MarketRepo::ensure_windows_for_season(conn, season_id, start_year)?;
        }

        // 8. Emergency board injection (max 1 per club per season)
        FinanceService::maybe_emergency_board_injection(
            conn,
            season_id,
            user_team_id,
            current_date,
        )?;

        // 9. Youth Academy quarterly intake
        let graduates =
            SquadTacticsService::graduate_youth_if_due(conn, user_team_id, current_date, rng)?;
        if !graduates.is_empty() {
            let _ = ClubRepo::insert_message(
                conn,
                &format!("Youth Academy Intake: {} new prospects", graduates.len()),
                "New youth prospects are ready for review in your academy.",
                current_date,
                "Youth Academy",
                Some(user_team_id),
                None,
            );
        }

        // 10. Process due scheduled transfers
        let _ =
            TransferMarketService::process_due_scheduled_transfers(conn, season_id, current_date)?;

        // 11. Persist Bug #18 `last_housekeeping_date`
        let mut cstate = ClubRepo::career_state(conn, season_id)?;
        cstate.last_housekeeping_date = Some(current_date);
        ClubRepo::save_career_state(conn, &cstate)?;

        // 12. AI Transfer Market & AI Weekly Training
        let _ =
            TransferMarketService::run_market_ai(conn, season_id, user_team_id, current_date, rng)?;
        let _ =
            SquadTacticsService::run_ai_training(conn, season_id, user_team_id, current_date, rng)?;

        Ok(())
    }
}

fn load_all_stadiums(conn: &Connection) -> NaratamaResult<HashMap<i64, Stadium>> {
    let mut stmt = conn
        .prepare(
            "SELECT stadium_id, name, city, country_id,
                    COALESCE(capacity_general, 15000), COALESCE(capacity_stand, 10000),
                    COALESCE(capacity_terrace, 5000), COALESCE(capacity_boxes, 500),
                    COALESCE(max_general, 30000), COALESCE(max_stand, 20000),
                    COALESCE(max_terrace, 10000), COALESCE(max_boxes, 1000),
                    COALESCE(ticket_price_general, 14), COALESCE(ticket_price_stand, 38),
                    COALESCE(ticket_price_terrace, 21), COALESCE(ticket_price_boxes, 120)
             FROM stadiums",
        )
        .map_err(db_err)?;
    let map = stmt
        .query_map([], |r| {
            let id: i64 = r.get(0)?;
            Ok((
                id,
                Stadium {
                    stadium_id: naratama_model::StadiumId(id),
                    name: r.get(1)?,
                    city: r.get(2)?,
                    country_id: naratama_model::CountryId(r.get::<_, Option<i64>>(3)?.unwrap_or(1)),
                    capacity_general: r.get(4)?,
                    capacity_stand: r.get(5)?,
                    capacity_terrace: r.get(6)?,
                    capacity_boxes: r.get(7)?,
                    max_general: r.get(8)?,
                    max_stand: r.get(9)?,
                    max_terrace: r.get(10)?,
                    max_boxes: r.get(11)?,
                    ticket_price_general: r.get(12)?,
                    ticket_price_stand: r.get(13)?,
                    ticket_price_terrace: r.get(14)?,
                    ticket_price_boxes: r.get(15)?,
                },
            ))
        })
        .map_err(db_err)?
        .collect::<Result<HashMap<_, _>, _>>()
        .map_err(db_err)?;
    Ok(map)
}

fn load_tournament_orgs(conn: &Connection) -> NaratamaResult<HashMap<i64, TournamentOrg>> {
    let mut stmt = conn
        .prepare("SELECT tournament_id, COALESCE(organization, 1) FROM tournaments")
        .map_err(db_err)?;
    let map = stmt
        .query_map([], |r| Ok((r.get(0)?, TournamentOrg::from_code(r.get(1)?))))
        .map_err(db_err)?
        .collect::<Result<HashMap<_, _>, _>>()
        .map_err(db_err)?;
    Ok(map)
}

fn load_squads_for_teams(
    conn: &Connection,
    team_ids: &HashSet<TeamId>,
) -> NaratamaResult<HashMap<TeamId, Vec<Player>>> {
    if team_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let mut by_team: HashMap<TeamId, Vec<Player>> = HashMap::new();
    let ids: Vec<i64> = team_ids.iter().map(|t| t.0).collect();
    for chunk in ids.chunks(400) {
        let placeholders = vec!["?"; chunk.len()].join(",");
        let sql = format!(
            "SELECT tp.team_id, p.*
             FROM team_players tp
             JOIN players p ON p.player_id = tp.player_id
             WHERE tp.team_id IN ({placeholders}) AND COALESCE(p.status_id, 1) = 1
             ORDER BY tp.team_id ASC, p.power DESC"
        );
        let mut stmt = conn.prepare(&sql).map_err(db_err)?;
        let params: Vec<&dyn rusqlite::ToSql> =
            chunk.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
        let mut rows = stmt.query(params.as_slice()).map_err(db_err)?;
        while let Some(row) = rows.next().map_err(db_err)? {
            let tid = TeamId(row.get(0).map_err(db_err)?);
            let p = ClubRepo::player_from_row(row).map_err(db_err)?;
            by_team.entry(tid).or_default().push(p);
        }
    }
    Ok(by_team)
}
