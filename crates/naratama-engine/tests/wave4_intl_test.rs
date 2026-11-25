use std::collections::HashSet;
use std::path::PathBuf;

use chrono::NaiveDate;
use naratama_db::club_repo::ClubRepo;
use naratama_db::intl_repo::IntlRepo;
use naratama_db::reference_repo::ReferenceRepo;
use naratama_db::session::SessionManager;
use naratama_db::SeasonRepo;
use naratama_engine::{
    InternationalCalendarService, InternationalSimulationService, InternationalTournamentService,
    NationalTeamSelectionService, NationalTeamService,
};
use naratama_model::{tournament_ids, CountryId, PositionGroup};
use rand::rngs::StdRng;
use rand::SeedableRng;
use rusqlite::{params, Connection};
use tempfile::tempdir;

fn template_db_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("assets/data/superkickoff_2025.db")
}

fn setup_session() -> (tempfile::TempDir, Connection) {
    let dir = tempdir().unwrap();
    let mgr = SessionManager::new(dir.path(), template_db_path());
    mgr.create_slot_from_seed(1).unwrap();
    let conn = mgr.begin_session(1).unwrap();
    (dir, conn)
}

#[test]
#[allow(clippy::too_many_lines)]
fn wave4_four_year_tournament_cycle_and_podiums() {
    let (_dir, mut conn) = setup_session();
    let mut rng = StdRng::seed_from_u64(4_004);

    // 1. Seeded FIFA World Rankings check (2024 dataset)
    let rankings = NationalTeamService::get_fifa_rankings(&conn, None).unwrap();
    assert_eq!(rankings.len(), 254);
    assert!(
        rankings[0].points > 1800.0,
        "Top ranked FIFA nation must be seeded above 1800 pts"
    );

    let argentina_before = IntlRepo::country_by_id(&conn, CountryId(11))
        .unwrap()
        .unwrap()
        .points;
    assert!((argentina_before - 1889.02).abs() < 0.01);

    // 2. Simulate full 4-year cycle: start_years 2024 (cycle=1), 2025 (cycle=2 WC), 2026 (cycle=3 ASEAN), 2027 (cycle=0 EURO+Asian)
    let years = [2024, 2025, 2026, 2027];
    let england = CountryId(44);
    let indonesia = CountryId(94);

    for year in years {
        let season = SeasonRepo::new(&conn).insert_season(year).unwrap();
        let windows = InternationalCalendarService::generate_windows_for_season(
            &conn,
            season.season_id,
            year,
        )
        .unwrap();
        let user_c = if year == 2026 {
            Some(indonesia)
        } else {
            Some(england)
        };

        let scheduled = InternationalTournamentService::schedule_cycle_for_season(
            &conn,
            season.season_id,
            year,
            user_c,
            &windows,
            &mut rng,
        )
        .unwrap();
        assert!(
            scheduled > 0,
            "Cycle year {} must schedule fixtures",
            year + 1
        );

        // Verify no nation plays two matches on the same date in group stage
        let same_day_dups: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM (
                    SELECT date, country_id, COUNT(*) AS cnt FROM (
                        SELECT date, home_country_id AS country_id FROM country_matches WHERE season_id = ?1
                        UNION ALL
                        SELECT date, away_country_id AS country_id FROM country_matches WHERE season_id = ?1
                    )
                    GROUP BY date, country_id
                    HAVING cnt > 1
                 )",
                params![season.season_id.0],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            same_day_dups, 0,
            "No country may play twice on the same date (season {year})"
        );

        let summer_end = NaiveDate::from_ymd_opt(year + 1, 8, 31).unwrap();
        let simmed = InternationalSimulationService::simulate_pending_matches_through_date(
            &mut conn,
            season.season_id,
            summer_end,
            &mut rng,
        )
        .unwrap();
        assert!(simmed > 0);

        let cycle = (year + 1) % 4;
        if cycle == 1 {
            let nl = InternationalTournamentService::get_tournament_podium(
                &conn,
                tournament_ids::NATIONS_LEAGUE,
                season.season_id,
            )
            .unwrap();
            assert!(nl.is_some(), "2025 Nations League must reach Final podium");
        } else if cycle == 2 {
            let wc = InternationalTournamentService::get_tournament_podium(
                &conn,
                tournament_ids::WORLD_CUP,
                season.season_id,
            )
            .unwrap()
            .expect("2026 World Cup must crown champion, runner-up & 3rd place");
            assert_ne!(wc.champion_country_id, wc.runner_up_country_id);
            assert!(wc.third_place_country_id.is_some());
        } else if cycle == 3 {
            let asean = InternationalTournamentService::get_tournament_podium(
                &conn,
                tournament_ids::FIFA_ASEAN_CUP,
                season.season_id,
            )
            .unwrap();
            assert!(asean.is_some(), "2027 FIFA ASEAN Cup must complete");
        } else {
            let euro = InternationalTournamentService::get_tournament_podium(
                &conn,
                tournament_ids::EURO_CUP,
                season.season_id,
            )
            .unwrap();
            let asian = InternationalTournamentService::get_tournament_podium(
                &conn,
                tournament_ids::ASIAN_CUP,
                season.season_id,
            )
            .unwrap();
            assert!(euro.is_some(), "2028 EURO must reach Final podium");
            assert!(asian.is_some(), "2028 Asian Cup must reach Final podium");
            for (tid, label) in [
                (tournament_ids::COPA_AMERICA, "Copa América"),
                (tournament_ids::AFRICA_CUP, "AFCON"),
                (tournament_ids::GOLD_CUP, "Gold Cup"),
            ] {
                let podium = InternationalTournamentService::get_tournament_podium(
                    &conn,
                    tid,
                    season.season_id,
                )
                .unwrap();
                assert!(podium.is_some(), "2028 {label} must reach Final podium");
            }
        }
    }

    let argentina_after = IntlRepo::country_by_id(&conn, CountryId(11))
        .unwrap()
        .unwrap()
        .points;
    assert!(
        (argentina_after - argentina_before).abs() > 0.01,
        "FIFA Elo SUM points must update across matches"
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn wave4_eligibility_cap_tie_squad_selection_and_duty_locks() {
    let (_dir, mut conn) = setup_session();
    let mut rng = StdRng::seed_from_u64(99);

    let england = CountryId(44);
    let ireland = CountryId(108);
    let spain = CountryId(64);
    let season = SeasonRepo::new(&conn).insert_season(2025).unwrap();
    let windows =
        InternationalCalendarService::generate_windows_for_season(&conn, season.season_id, 2025)
            .unwrap();
    let oct = &windows[0];

    // Pick a top England club team for user_team_id
    let user_club = ReferenceRepo::new(&conn)
        .teams_for_country(england)
        .unwrap()
        .into_iter()
        .find(|t| t.division == 1)
        .unwrap();
    let club_squad = ClubRepo::squad_for_team(&conn, user_club.team_id).unwrap();

    // Verify club residency never grants national eligibility: find non-English player at English club
    if let Some(foreigner) = club_squad.iter().find(|p| p.country_id != england) {
        let elig = NationalTeamService::eligible_country_ids_for_player(&conn, foreigner.player_id)
            .unwrap();
        assert!(
            !elig.contains(&england),
            "Club residency in England must NOT grant England eligibility"
        );
    }

    // Add dual nationality (England primary + Ireland secondary) to a club player
    let dual_pid = club_squad[0].player_id;
    conn.execute(
        "INSERT OR REPLACE INTO player_national_eligibility (player_id, country_id, is_primary, is_locked, notes)
         VALUES (?1, ?2, 0, 0, 'Heritage')",
        params![dual_pid.0, ireland.0],
    )
    .unwrap();
    let dual_elig = NationalTeamService::eligible_country_ids_for_player(&conn, dual_pid).unwrap();
    assert!(dual_elig.contains(&ireland));

    // Balanced 23-man selection check (quota >= 3 GK, 7 DEF, 6 MID, 4 ATT)
    let sel = NationalTeamSelectionService::select_squad_for_country(
        &mut conn,
        spain,
        season.season_id,
        oct,
        23,
        Some(user_club.team_id),
    )
    .unwrap();
    assert_eq!(sel.players.len(), 23);
    assert!(
        sel.players
            .iter()
            .filter(|p| PositionGroup::from_position_id(p.position_id) == PositionGroup::Goalkeeper)
            .count()
            >= 3
    );
    assert!(
        sel.players
            .iter()
            .filter(|p| PositionGroup::from_position_id(p.position_id) == PositionGroup::Defender)
            .count()
            >= 7
    );
    assert!(
        sel.players
            .iter()
            .filter(|p| PositionGroup::from_position_id(p.position_id) == PositionGroup::Midfielder)
            .count()
            >= 6
    );
    assert!(
        sel.players
            .iter()
            .filter(|p| PositionGroup::from_position_id(p.position_id) == PositionGroup::Attacker)
            .count()
            >= 4
    );
    let captain = sel
        .players
        .iter()
        .find(|p| p.player_id == sel.captain_id)
        .unwrap();
    assert_ne!(
        PositionGroup::from_position_id(captain.position_id),
        PositionGroup::Goalkeeper
    );
    let gk_ids: HashSet<_> = sel
        .players
        .iter()
        .filter(|p| PositionGroup::from_position_id(p.position_id) == PositionGroup::Goalkeeper)
        .map(|p| p.player_id)
        .collect();
    assert!(
        sel.squad_numbers
            .iter()
            .any(|(pid, num)| *num == 1 && gk_ids.contains(pid)),
        "Shirt #1 must be assigned to a goalkeeper"
    );

    // Friendly window sets `pending_release` for user-club England players
    InternationalTournamentService::schedule_friendlies_for_window(
        &conn,
        season.season_id,
        oct,
        Some(england),
        &mut rng,
    )
    .unwrap();
    let eng_sel = NationalTeamSelectionService::select_squad_for_country(
        &mut conn,
        england,
        season.season_id,
        oct,
        23,
        Some(user_club.team_id),
    )
    .unwrap();
    let _ = eng_sel;
    let duties =
        IntlRepo::duties_for_window(&conn, season.season_id, england, &oct.window_id).unwrap();
    if let Some(pending_duty) = duties.iter().find(|d| d.status == "pending_release") {
        NationalTeamSelectionService::respond_to_friendly_callup(
            &mut conn,
            pending_duty.duty_id,
            true,
        )
        .unwrap();
        let on_duty = IntlRepo::active_duty_player_ids_on_date(&conn, oct.start_date).unwrap();
        assert!(on_duty.contains(&pending_duty.player_id));
    }

    // Return duty cleanly after window closes
    let after_win = oct.return_date + chrono::Duration::days(1);
    NationalTeamSelectionService::release_concluded_duties(&conn, after_win).unwrap();
    let on_duty_after = IntlRepo::active_duty_player_ids_on_date(&conn, after_win).unwrap();
    assert!(on_duty_after.is_empty());

    // Cap-tie lock: simulate a competitive match (`EURO_CUP`) involving Spain, verify participants are permanently locked!
    let sum = &windows[2];
    InternationalTournamentService::schedule_continental_championship(
        &conn,
        tournament_ids::EURO_CUP,
        naratama_model::Confederation::Uefa,
        season.season_id,
        sum,
        24,
        6,
        Some(spain),
        &mut rng,
    )
    .unwrap();
    let euro_matches =
        IntlRepo::country_matches_for_tournament(&conn, season.season_id, tournament_ids::EURO_CUP)
            .unwrap();
    let spain_match = euro_matches
        .iter()
        .find(|m| m.home_country_id == spain || m.away_country_id == spain)
        .unwrap()
        .clone();
    InternationalSimulationService::simulate_country_match(&mut conn, &spain_match, &mut rng)
        .unwrap();

    let locked_player: i64 = conn
        .query_row(
            "SELECT mpc.player_id FROM match_players_countries mpc
             WHERE mpc.match_id = ?1 AND mpc.minutes > 0 AND mpc.player_id > 0 LIMIT 1",
            params![spain_match.match_id.0],
            |r| r.get(0),
        )
        .unwrap();
    let locked_flag: i64 = conn
        .query_row(
            "SELECT COALESCE(is_locked, 0) FROM player_national_eligibility WHERE player_id = ?1",
            params![locked_player],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        locked_flag, 1,
        "Competitive cap must permanently cap-tie player"
    );
}
