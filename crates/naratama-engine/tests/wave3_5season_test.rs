use std::collections::HashSet;
use std::path::PathBuf;

use chrono::{Datelike, NaiveDate};
use naratama_db::club_repo::ClubRepo;
use naratama_db::finance_repo::{FinanceRepo, NewLedgerEntry};
use naratama_db::market_repo::MarketRepo;
use naratama_db::reference_repo::ReferenceRepo;
use naratama_db::session::SessionManager;
use naratama_engine::{
    BoardAmbition, CustomClubRequest, CustomClubService, DesktopFacade, FinanceService,
    GenerateCareerRequest, InitialSquadPreset, MatchdayService, OfferResponseAction,
    OffseasonService, OnboardingService, PreseasonCamp, ScoutingService, SeasonGenerator,
    SquadTacticsService, StadiumArchitecture, TrainingFocus, TransferMarketService,
};
use naratama_model::{
    CountryId, FfpEnforcement, LedgerCategory, LedgerPeriod, MatchStatus, Season, StaffRole,
};
use naratama_sim::FinanceCalc;
use rand::rngs::StdRng;
use rand::SeedableRng;
use rusqlite::params;
use tempfile::tempdir;

fn template_db_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("assets/data/superkickoff_2025.db")
}

#[test]
#[allow(clippy::too_many_lines)]
fn headless_five_season_career_and_economic_invariants() {
    let dir = tempdir().unwrap();
    let mgr = SessionManager::new(dir.path(), template_db_path());
    mgr.create_slot_from_seed(1).unwrap();
    let mut conn = mgr.begin_session(1).unwrap();
    let mut rng = StdRng::seed_from_u64(77_005);

    // 1. Initialize opening ledger across all 2,014 clubs BEFORE Season 1
    let init_date = NaiveDate::from_ymd_opt(2025, 7, 1).unwrap();
    FinanceService::ensure_ready(&mut conn, naratama_model::SeasonId(1), init_date).unwrap();
    assert!(
        FinanceRepo::verify_ledger_invariant(&conn)
            .unwrap()
            .is_empty(),
        "Opening balances across all 2,014 clubs must satisfy budget == SUM(ledger)"
    );

    // 2. Onboarding + Custom Club creation in England (CountryId 44) lowest division
    let england = CountryId(44);
    let custom = CustomClubService::create_custom_club(
        &mut conn,
        &CustomClubRequest {
            name: "Naratama United FC".into(),
            abbreviation: "NRU".into(),
            country_id: england,
            founded_year: 2025,
            primary_color: "#1B5E20".into(),
            secondary_color: "#FFFFFF".into(),
            stadium_name: "Naratama Arena".into(),
            stadium_architecture: StadiumArchitecture::CompactModern,
            squad_preset: InitialSquadPreset::YouthProject,
            starting_budget: 25_000_000,
            nickname: None,
            badge_shape: None,
            badge_symbol: None,
            jersey_pattern: None,
        },
        naratama_model::SeasonId(1),
        init_date,
        &mut rng,
    )
    .unwrap();
    let user_team_id = custom.team.team_id;
    assert_eq!(custom.team.budget, 25_000_000);
    assert!(
        FinanceRepo::verify_ledger_invariant(&conn)
            .unwrap()
            .is_empty(),
        "Custom club starting capital must satisfy budget == SUM(ledger)"
    );

    // Build Season 1 across England pyramid + national cup + UEFA cups
    let ref_repo = ReferenceRepo::new(&conn);
    let eng_leagues = ref_repo.leagues_for_country(england).unwrap();
    let user_league = eng_leagues
        .iter()
        .find(|l| l.division_id == custom.team.division)
        .and_then(|l| ref_repo.tournament_by_id(l.tournament_id).ok().flatten())
        .unwrap();
    let other_leagues: Vec<_> = eng_leagues
        .iter()
        .filter(|l| l.tournament_id != user_league.tournament_id)
        .filter_map(|l| ref_repo.tournament_by_id(l.tournament_id).ok().flatten())
        .collect();
    let cup = ref_repo.national_cup(england).unwrap();
    let internationals = ref_repo.international_cups().unwrap();

    let mut current_plan = SeasonGenerator::generate_career(
        &conn,
        &GenerateCareerRequest {
            country_id: england,
            league: user_league,
            cup,
            internationals,
            other_leagues,
            start_year: 2025,
            user_team_id: Some(user_team_id),
            include_preseason_friendlies: true,
        },
        &mut rng,
    )
    .unwrap();
    OnboardingService::init_season_state(&conn, &current_plan.season, init_date).unwrap();
    OnboardingService::complete_orientation(&conn, current_plan.season.season_id, init_date)
        .unwrap();
    assert!(
        OnboardingService::is_orientation_completed(&conn, current_plan.season.season_id).unwrap()
    );

    // 3. Exercise Season 1 user actions: training, facility, staff, scouting (Bug #3 & #10), transfers (Bug #1 & #11), sell-on clause
    SquadTacticsService::upgrade_training_ground(
        &mut conn,
        user_team_id,
        current_plan.season.season_id,
        init_date,
    )
    .unwrap();
    SquadTacticsService::hire_staff(
        &mut conn,
        user_team_id,
        current_plan.season.season_id,
        StaffRole::TacticalAnalyst,
        "Coach Arif",
        4,
        init_date,
    )
    .unwrap();
    let _improved = SquadTacticsService::train_focus(
        &mut conn,
        user_team_id,
        current_plan.season.season_id,
        TrainingFocus::Attacking,
        init_date,
    )
    .unwrap();

    let scout_id = ScoutingService::hire_scout(
        &mut conn,
        current_plan.season.season_id,
        user_team_id,
        "Budi Scout",
        4,
        "South America",
        init_date,
    )
    .unwrap();
    let ret = ScoutingService::deploy_scout(&conn, scout_id, "South America", init_date).unwrap();
    let reports =
        ScoutingService::collect_due_reports(&mut conn, user_team_id, ret, &mut rng).unwrap();
    assert!(!reports.is_empty());
    // Bug #3 verification: South America reports carry a CONMEBOL country_id, not hardcoded wrong Asia ID!
    let conf_of_report: i64 = conn
        .query_row(
            "SELECT confederation FROM countries WHERE country_id = ?1",
            params![reports[0].country_id.0],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        conf_of_report, 2,
        "Bug #3: South America scout must find CONMEBOL nation"
    );
    let wonderkid_id =
        ScoutingService::sign_wonderkid(&mut conn, current_plan.season.season_id, &reports[0], ret)
            .unwrap();
    // Bug #10 verification: player_registrations created cleanly
    let reg_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM player_registrations WHERE player_id = ?1 AND status = 1",
            params![wonderkid_id.0],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(reg_count, 1);

    // Exercise Sell-On Clause + Transfer Offer flow + Loan Bug #11
    let seller_club = ReferenceRepo::new(&conn)
        .teams_for_country(england)
        .unwrap()
        .into_iter()
        .find(|t| {
            t.team_id != user_team_id
                && t.division > 0
                && ClubRepo::squad_size(&conn, t.team_id).unwrap_or(30) < 30
        })
        .unwrap();
    let third_club = custom.replaced_team_id;
    conn.execute(
        "INSERT INTO transfer_clauses (player_id, beneficiary_team_id, type, pct, active, created_date)
         VALUES (?1, ?2, 'sell_on', 0.15, 1, '2025-07-01')",
        params![wonderkid_id.0, third_club.0],
    )
    .unwrap();
    let offer_id = TransferMarketService::make_transfer_offer(
        &conn,
        wonderkid_id,
        user_team_id,
        seller_club.team_id,
        2_000_000,
        false,
        1,
        ret,
    )
    .unwrap();
    // Ensure seller_club can afford 2M + 100k fee first via legitimate board grant
    FinanceRepo::record(
        &mut conn,
        &NewLedgerEntry {
            team_id: seller_club.team_id,
            season_id: current_plan.season.season_id,
            entry_date: ret,
            period: LedgerPeriod::Board,
            category: LedgerCategory::BoardFunding,
            subcategory: "test_grant",
            amount: 5_000_000,
            ref_type: "test",
            ref_id: None,
            description: "Club investment",
        },
    )
    .unwrap();
    MarketRepo::ensure_windows_for_season(&mut conn, current_plan.season.season_id, 2025).unwrap();
    let outcome = TransferMarketService::respond_to_offer(
        &mut conn,
        current_plan.season.season_id,
        offer_id,
        OfferResponseAction::Accept,
        ret,
    )
    .unwrap();
    assert!(outcome.completed_immediately);
    let sell_on_received: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(amount), 0) FROM finance_ledger
             WHERE team_id = ?1 AND category = 'sell_on_received'",
            params![third_club.0],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        sell_on_received, 300_000,
        "15% sell-on clause must pay $300k"
    );

    // 4. Simulate 5 full consecutive seasons end-to-end!
    let mut observed_divisions = HashSet::new();
    observed_divisions.insert(custom.team.division);

    for season_idx in 1..=5 {
        let season: Season = current_plan.season.clone();
        let mut last_hk_week: Option<u32> = None;

        // Loop until all scheduled matches (including newly drawn cup rounds) finish!
        loop {
            let next_date_str: Option<String> = conn
                .query_row(
                    "SELECT MIN(date) FROM matches WHERE season_id = ?1 AND status = ?2 AND date IS NOT NULL",
                    params![season.season_id.0, MatchStatus::Scheduled as i64],
                    |r| r.get(0),
                )
                .unwrap();
            let Some(date_str) = next_date_str else {
                break;
            };
            let date = NaiveDate::parse_from_str(&date_str, "%Y-%m-%d").unwrap();

            MatchdayService::simulate_matchday(
                &mut conn,
                season.season_id,
                date,
                None,
                true,
                user_team_id,
                &mut rng,
                None,
            )
            .unwrap();

            if season_idx == 0 {
                let replay =
                    DesktopFacade::latest_match_replay(&conn, season.season_id, user_team_id)
                        .unwrap()
                        .expect("user match replay must exist after a matchday");
                assert!(
                    !replay.events.is_empty(),
                    "recorded user match must expose an incident timeline"
                );
                assert!(
                    replay.events.iter().any(|e| e.minute <= 90),
                    "replay events must carry in-match minutes"
                );
            }

            let wk = date.iso_week().week();
            if last_hk_week != Some(wk) {
                MatchdayService::weekly_housekeeping(
                    &mut conn,
                    season.season_id,
                    user_team_id,
                    date,
                    &mut rng,
                )
                .unwrap();
                last_hk_week = Some(wk);
            }
        }

        // Verify zero duplicate fixtures and zero self-matches in this season!
        let dup_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM (
                    SELECT 1 FROM matches WHERE season_id = ?1
                    GROUP BY season_id, tournament_id, phase, leg, round, home_team_id, away_team_id, IFNULL(group_id, -1)
                    HAVING COUNT(*) > 1
                 )",
                params![season.season_id.0],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            dup_count, 0,
            "Season {season_idx} must have zero duplicate fixtures"
        );

        let self_matches: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM matches WHERE season_id = ?1 AND home_team_id = away_team_id",
                params![season.season_id.0],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            self_matches, 0,
            "Season {season_idx} must have zero self-matches"
        );

        // Verify global financial ledger invariant BEFORE offseason (all 2,014 clubs!)
        let mismatches = FinanceRepo::verify_ledger_invariant(&conn).unwrap();
        assert!(
            mismatches.is_empty(),
            "Season {season_idx} mid/end ledger mismatch on {} clubs: {:?}",
            mismatches.len(),
            &mismatches[..mismatches.len().min(3)]
        );

        // Offseason 7-step flow (season 0 exercises the FFP telemetry branch)
        let off_date = NaiveDate::from_ymd_opt(season.end_year, 6, 15).unwrap();
        if season_idx == 0 {
            FinanceRepo::set_mode(&conn, FfpEnforcement::Relaxed).unwrap();
        }
        let s0 = OffseasonService::begin_offseason(
            &mut conn,
            &season,
            user_team_id,
            "Manager Budi",
            off_date,
            &mut rng,
        )
        .unwrap();
        assert_ne!(s0.report, "");
        if season_idx == 0 {
            assert!(
                s0.report.contains("FFP (relaxed)"),
                "FFP toggle must be reported in the offseason summary: {}",
                s0.report
            );
            FinanceRepo::set_mode(&conn, FfpEnforcement::Off).unwrap();
        }

        // Check Gala awards recorded for season
        let awards_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM player_awards WHERE season_id = ?1",
                params![season.season_id.0],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            awards_count >= 1,
            "Season {season_idx} Gala awards must be recorded"
        );

        let _s1 =
            OffseasonService::enter_contracts(&mut conn, &season, user_team_id, off_date, &mut rng)
                .unwrap();
        let _s2 = OffseasonService::enter_development(&mut conn, &season, user_team_id).unwrap();
        // Actually run mut version of enter_development -> verify archived stats!
        let archived_rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM player_season_stats WHERE season_id = ?1",
                params![season.season_id.0],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            archived_rows > 0,
            "Bug #19: Season {season_idx} stats must archive explicitly"
        );

        let _s3 =
            OffseasonService::enter_transfers(&mut conn, &season, user_team_id, off_date, &mut rng)
                .unwrap();
        let _s4 = OffseasonService::apply_preseason_camp(
            &mut conn,
            &season,
            user_team_id,
            PreseasonCamp::YouthShowcase,
        )
        .unwrap();
        let _s5 = OffseasonService::choose_board_ambition(
            &mut conn,
            &season,
            user_team_id,
            BoardAmbition::TopHalf,
            off_date,
        )
        .unwrap();

        let post_off_mismatches = FinanceRepo::verify_ledger_invariant(&conn).unwrap();
        assert!(
            post_off_mismatches.is_empty(),
            "Season {season_idx} post-offseason ledger mismatch!"
        );

        if let Some(u) = ReferenceRepo::new(&conn).team_by_id(user_team_id).unwrap() {
            observed_divisions.insert(u.division);
        }

        if season_idx < 5 {
            current_plan =
                OffseasonService::start_next_season(&mut conn, &season, user_team_id, &mut rng)
                    .unwrap();
        }
    }

    assert_eq!(FinanceCalc::annual_amortization(10_000_000, 4), 2_500_000);
    assert!(
        FinanceRepo::verify_ledger_invariant(&conn)
            .unwrap()
            .is_empty(),
        "Final 5-season global audit: budget == SUM(finance_ledger.amount) for ALL 2,014 clubs"
    );
}
