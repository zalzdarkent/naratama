use std::path::PathBuf;

use chrono::NaiveDate;
use naratama_db::reference_repo::ReferenceRepo;
use naratama_db::session::SessionManager;
use naratama_engine::{
    ClubLifeNews, DesktopFacade, GenerateCareerRequest, InboxService, ManagerOfferService,
    ManagerOfferStatus, MatchdayService, NewsEngine, SeasonGenerator,
};
use naratama_model::{CountryId, MatchStatus};
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
fn wave5_news_club_life_manager_offers_inbox_and_dashboard_snapshot() {
    let dir = tempdir().unwrap();
    let mgr = SessionManager::new(dir.path(), template_db_path());
    mgr.create_slot_from_seed(1).unwrap();
    let mut conn = mgr.begin_session(1).unwrap();
    let mut rng = StdRng::seed_from_u64(5_555);

    let england = CountryId(44);
    let ref_repo = ReferenceRepo::new(&conn);
    let leagues = ref_repo.leagues_for_country(england).unwrap();
    let league = ref_repo
        .tournament_by_id(leagues[0].tournament_id)
        .unwrap()
        .unwrap();
    let teams = ref_repo.teams_for_division(league.tournament_id).unwrap();
    let user_team = teams.last().unwrap().clone(); // Pick an underdog so manager offers find stronger clubs!

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

    let day1 = NaiveDate::from_ymd_opt(2025, 8, 1).unwrap();
    let created = NewsEngine::preview(
        &conn,
        plan.season.season_id,
        day1,
        league.tournament_id,
        "europe",
    )
    .unwrap();
    assert_eq!(created, 2);
    // Idempotency: running preview twice inserts 0 duplicates
    let dup = NewsEngine::preview(
        &conn,
        plan.season.season_id,
        day1,
        league.tournament_id,
        "europe",
    )
    .unwrap();
    assert_eq!(dup, 0);

    // Simulate 4 matchdays so form, fan mood, and matchday news populate
    for _ in 0..4 {
        let next_date_str: Option<String> = conn
            .query_row(
                "SELECT MIN(date) FROM matches WHERE season_id = ?1 AND status = ?2 AND date IS NOT NULL",
                params![plan.season.season_id.0, MatchStatus::Scheduled as i64],
                |r| r.get(0),
            )
            .unwrap();
        let Some(ds) = next_date_str else { break };
        let date = NaiveDate::parse_from_str(&ds, "%Y-%m-%d").unwrap();
        MatchdayService::simulate_matchday(
            &mut conn,
            plan.season.season_id,
            date,
            None,
            false,
            user_team.team_id,
            &mut rng,
        )
        .unwrap();
        let _ = NewsEngine::on_matchday(
            &conn,
            plan.season.season_id,
            date,
            league.tournament_id,
            "europe",
        )
        .unwrap();
        let _ = ClubLifeNews::on_matchday(
            &conn,
            plan.season.season_id,
            date,
            user_team.team_id,
            &user_team.name,
        )
        .unwrap();
    }

    let mood = ClubLifeNews::fan_mood(&conn, plan.season.season_id, user_team.team_id, 6).unwrap();
    assert!((0..=100).contains(&mood));
    let feed = NewsEngine::feed(&conn, None, 20).unwrap();
    assert!(!feed.is_empty());

    // Manager offer + Inbox
    let offer = ManagerOfferService::maybe_generate_offer(
        &conn,
        plan.season.season_id,
        user_team.team_id,
        day1,
        &mut rng,
    )
    .unwrap()
    .expect("Underdog club manager must be able to receive a higher-club offer");
    assert!(offer.prestige_stars() >= 1);
    assert!(InboxService::unread_count(&conn).unwrap() >= 1);

    let next_status = ManagerOfferService::respond_to_offer(&conn, offer.offer_id, true).unwrap();
    assert_eq!(next_status, ManagerOfferStatus::Accepted);
    assert!(ManagerOfferService::accepted_move(&conn).unwrap().is_some());
    InboxService::mark_all_read(&conn).unwrap();
    assert_eq!(InboxService::unread_count(&conn).unwrap(), 0);

    // Desktop Dashboard Snapshot
    let snap = DesktopFacade::dashboard_snapshot(
        &conn,
        plan.season.season_id,
        user_team.team_id,
        league.tournament_id,
    )
    .unwrap();
    assert_eq!(snap.club.team_id, user_team.team_id);
    assert!(!snap.standings_preview.is_empty());
    assert!(snap.next_match.is_some());
    assert!(snap.weekly_payroll > 0);
}
