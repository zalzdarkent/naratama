use std::path::PathBuf;

use chrono::NaiveDate;
use naratama_db::club_repo::ClubRepo;
use naratama_db::market_repo::MarketRepo;
use naratama_db::reference_repo::ReferenceRepo;
use naratama_db::session::SessionManager;
use naratama_engine::{
    GenerateCareerRequest, ManagerOfferService, MatchdayService, OfferResponseAction, ReadService,
    ScoutingService, SeasonGenerator, SquadTacticsService, TransferMarketService,
};
use naratama_model::{CountryId, MatchStatus, TransferOfferStatus};
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
fn wave10_scouting_academy_offers_and_managerial_move_actions() {
    let dir = tempdir().unwrap();
    let mgr = SessionManager::new(dir.path(), template_db_path());
    mgr.create_slot_from_seed(1).unwrap();
    let mut conn = mgr.begin_session(1).unwrap();
    let mut rng = StdRng::seed_from_u64(10_101);

    let england = CountryId(44);
    let ref_repo = ReferenceRepo::new(&conn);
    let leagues = ref_repo.leagues_for_country(england).unwrap();
    let league = ref_repo
        .tournament_by_id(leagues[0].tournament_id)
        .unwrap()
        .unwrap();
    let teams = ref_repo.teams_for_division(league.tournament_id).unwrap();
    let user_team = teams[0].clone();
    let rival = teams[1].clone();

    let plan = SeasonGenerator::generate_career(
        &conn,
        &GenerateCareerRequest {
            country_id: england,
            league: league.clone(),
            cup: None,
            internationals: Vec::new(),
            other_leagues: Vec::new(),
            start_year: 2025,
            user_team_id: Some(user_team.team_id),
            include_preseason_friendlies: false,
        },
        &mut rng,
    )
    .unwrap();
    let season_id = plan.season.season_id;
    let date = NaiveDate::from_ymd_opt(2025, 7, 15).unwrap();

    // 1. Scouting: hire, deploy, collect after the trip and sign the wonderkid.
    let scout_id = ScoutingService::hire_scout(
        &mut conn,
        season_id,
        user_team.team_id,
        "Scout Uji",
        4,
        "europe",
        date,
    )
    .unwrap();
    assert!(scout_id > 0);
    let hired = ReadService::scouts(&conn, user_team.team_id).unwrap();
    assert_eq!(hired.len(), 1);
    let returns = ScoutingService::deploy_scout(&conn, scout_id, "europe", date).unwrap();
    let reports = ScoutingService::collect_due_reports(
        &mut conn,
        user_team.team_id,
        returns + chrono::Duration::days(1),
        &mut rng,
    )
    .unwrap();
    assert!(!reports.is_empty(), "a completed trip must yield a report");
    let listed = ReadService::scout_reports(&conn, user_team.team_id).unwrap();
    assert_eq!(listed.len(), reports.len());
    // Free a squad slot so the signing fits under the 30-player cap.
    let released = ClubRepo::squad_for_team(&conn, user_team.team_id).unwrap();
    let spare = released.last().unwrap().player_id;
    conn.execute(
        "DELETE FROM team_players WHERE team_id = ?1 AND player_id = ?2",
        params![user_team.team_id.0, spare.0],
    )
    .unwrap();
    let squad_before = ClubRepo::squad_size(&conn, user_team.team_id).unwrap();
    let row = ReadService::scout_report_row(&conn, listed[0].report_id)
        .unwrap()
        .unwrap();
    ScoutingService::sign_wonderkid(&mut conn, season_id, &row, date).unwrap();
    let squad_after = ClubRepo::squad_size(&conn, user_team.team_id).unwrap();
    assert_eq!(squad_after, squad_before + 1);
    let remaining = ReadService::scout_reports(&conn, user_team.team_id).unwrap();
    assert!(
        remaining.iter().all(|r| r.report_id != listed[0].report_id),
        "the signed report must leave the open list"
    );

    // 2. Academy intake: graduate, list and promote a prospect.
    let prospects = SquadTacticsService::graduate_youth_if_due(
        &mut conn,
        user_team.team_id,
        date + chrono::Duration::days(200),
        &mut rng,
    )
    .unwrap();
    assert!(
        !prospects.is_empty(),
        "academy intake must produce prospects"
    );
    let listed = ReadService::academy_prospects(&conn, user_team.team_id).unwrap();
    assert_eq!(listed.len(), prospects.len());
    let released = ClubRepo::squad_for_team(&conn, user_team.team_id).unwrap();
    let spare = released.last().unwrap().player_id;
    conn.execute(
        "DELETE FROM team_players WHERE team_id = ?1 AND player_id = ?2",
        params![user_team.team_id.0, spare.0],
    )
    .unwrap();
    let squad_before = ClubRepo::squad_size(&conn, user_team.team_id).unwrap();
    let prospect = ReadService::youth_prospect_row(&conn, listed[0].prospect_id)
        .unwrap()
        .unwrap();
    SquadTacticsService::sign_academy_prospect(&mut conn, season_id, &prospect, date).unwrap();
    assert_eq!(
        ClubRepo::squad_size(&conn, user_team.team_id).unwrap(),
        squad_before + 1
    );

    // 3. Incoming offers: a rival bid for a user player is listed and can be rejected.
    let player_id = ClubRepo::squad_for_team(&conn, user_team.team_id)
        .unwrap()
        .last()
        .unwrap()
        .player_id;
    let offer_id = MarketRepo::create_offer(
        &conn,
        player_id,
        rival.team_id,
        user_team.team_id,
        5_000_000,
        4_000_000,
        false,
        1,
        date,
        None,
        TransferOfferStatus::Pending,
    )
    .unwrap();
    let offers = ReadService::incoming_offers(&conn, user_team.team_id).unwrap();
    assert_eq!(offers.len(), 1);
    assert_eq!(offers[0].offer_id, offer_id);
    assert_eq!(offers[0].player_id, player_id);
    let outcome = TransferMarketService::respond_to_offer(
        &mut conn,
        season_id,
        offer_id,
        OfferResponseAction::Reject,
        date,
    )
    .unwrap();
    assert!(!outcome.completed_immediately);
    assert!(ReadService::incoming_offers(&conn, user_team.team_id)
        .unwrap()
        .is_empty());

    // 4. Managerial offer: pending -> accepted, ready for the season re-anchor.
    conn.execute(
        "INSERT INTO manager_offers
            (team_id, season_id, date, status, club_name, league_name, target,
             contract_years, budget, power, from_power)
         VALUES (?1, ?2, '2025-06-15', 0, ?3, ?4, 'Top half', 3, 25000000, ?5, ?6)",
        params![
            rival.team_id.0,
            season_id.0,
            rival.name,
            league.name,
            rival.power,
            user_team.power
        ],
    )
    .unwrap();
    let pending = ReadService::pending_manager_offer(&conn)
        .unwrap()
        .expect("pending offer");
    assert_eq!(pending.team_id, rival.team_id);
    assert!(ManagerOfferService::accepted_move(&conn).unwrap().is_none());
    ManagerOfferService::respond_to_offer(&conn, pending.offer_id, true).unwrap();
    let accepted = ManagerOfferService::accepted_move(&conn)
        .unwrap()
        .expect("accepted move");
    assert_eq!(accepted.offer_id, pending.offer_id);
    assert_eq!(accepted.team_id, rival.team_id);
    ManagerOfferService::complete_accepted_move(&conn, accepted.offer_id).unwrap();
    assert!(ManagerOfferService::accepted_move(&conn).unwrap().is_none());

    // 5. Simulation still healthy after all of the above.
    let next_date_str: Option<String> = conn
        .query_row(
            "SELECT MIN(date) FROM matches WHERE season_id = ?1 AND status = ?2 AND date IS NOT NULL",
            params![season_id.0, MatchStatus::Scheduled as i64],
            |r| r.get(0),
        )
        .unwrap();
    let match_date =
        NaiveDate::parse_from_str(next_date_str.as_deref().unwrap(), "%Y-%m-%d").unwrap();
    let outcomes = MatchdayService::simulate_matchday(
        &mut conn,
        season_id,
        match_date,
        None,
        true,
        user_team.team_id,
        &mut rng,
        None,
    )
    .unwrap();
    assert!(!outcomes.is_empty());
}
