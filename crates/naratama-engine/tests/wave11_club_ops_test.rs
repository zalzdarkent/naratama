use std::path::PathBuf;

use chrono::NaiveDate;
use naratama_db::club_repo::ClubRepo;
use naratama_db::finance_repo::FinanceRepo;
use naratama_db::reference_repo::ReferenceRepo;
use naratama_db::session::SessionManager;
use naratama_engine::{ClubOpsService, GenerateCareerRequest, SeasonGenerator};
use naratama_model::CountryId;
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
fn wave11_contract_renewal_stadium_tickets_and_sponsorship() {
    let dir = tempdir().unwrap();
    let mgr = SessionManager::new(dir.path(), template_db_path());
    mgr.create_slot_from_seed(1).unwrap();
    let mut conn = mgr.begin_session(1).unwrap();
    let mut rng = StdRng::seed_from_u64(11_111);

    let england = CountryId(44);
    let ref_repo = ReferenceRepo::new(&conn);
    let leagues = ref_repo.leagues_for_country(england).unwrap();
    let league = ref_repo
        .tournament_by_id(leagues[0].tournament_id)
        .unwrap()
        .unwrap();
    let teams = ref_repo.teams_for_division(league.tournament_id).unwrap();
    let user_team = teams[0].clone();
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
    let date = NaiveDate::from_ymd_opt(2025, 7, 10).unwrap();
    let year = ClubOpsService::current_year(&conn, season_id).unwrap();
    assert_eq!(year, 2025);

    // 1. Contract renewal: salary +15%, bonus = 2 months, end date +2 seasons.
    conn.execute(
        "UPDATE teams SET budget = 50000000 WHERE team_id = ?1",
        params![user_team.team_id.0],
    )
    .unwrap();
    let player = ClubRepo::squad_for_team(&conn, user_team.team_id)
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    conn.execute(
        "INSERT INTO contracts (player_id, team_id, start_date, end_date, status, salary)
         VALUES (?1, ?2, '2024-07-01', '2026-06-30', 1, ?3)",
        params![player.player_id.0, user_team.team_id.0, player.salary],
    )
    .unwrap();
    let old_end: String = conn
        .query_row(
            "SELECT end_date FROM contracts WHERE player_id = ?1 AND status = 1
             ORDER BY end_date DESC LIMIT 1",
            params![player.player_id.0],
            |r| r.get(0),
        )
        .unwrap();
    let expected_end = format!(
        "{}{}",
        old_end[0..4].parse::<i32>().unwrap().max(year) + 2,
        &old_end[4..old_end.len().min(10)]
    );
    let renewed =
        ClubOpsService::renew_contract(&mut conn, user_team.team_id, player.player_id, year, date)
            .unwrap();
    assert!(renewed.new_salary > player.salary);
    assert_eq!(renewed.signing_bonus, renewed.new_salary * 2);
    assert_eq!(renewed.new_end_date, expected_end);
    let stored_salary: i64 = conn
        .query_row(
            "SELECT salary FROM players WHERE player_id = ?1",
            params![player.player_id.0],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(stored_salary, renewed.new_salary);

    // 2. Stadium expansion: capacity grows, ledger records the cost.
    let before = ReferenceRepo::new(&conn)
        .stadium_by_id(user_team.stadium_id)
        .unwrap()
        .unwrap();
    let expansion =
        ClubOpsService::expand_stadium(&mut conn, user_team.team_id, season_id, date).unwrap();
    assert!(expansion.added_seats > 0);
    let after = ReferenceRepo::new(&conn)
        .stadium_by_id(user_team.stadium_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        after.capacity_total(),
        before.capacity_total() + expansion.added_seats
    );
    let ledger_costs: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM finance_ledger
             WHERE team_id = ?1 AND ref_type = 'stadium_expansion'",
            params![user_team.team_id.0],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(ledger_costs, 1);

    // 3. Ticket pricing.
    ClubOpsService::set_ticket_price(&conn, user_team.team_id, "general", 25).unwrap();
    let priced = ReferenceRepo::new(&conn)
        .stadium_by_id(user_team.stadium_id)
        .unwrap()
        .unwrap();
    assert_eq!(priced.ticket_price_general, 25);
    assert!(ClubOpsService::set_ticket_price(&conn, user_team.team_id, "vip", 10).is_err());

    // 4. Sponsorship: offers for empty slots, sign one, terminate it again.
    conn.execute(
        "DELETE FROM sponsorships WHERE team_id = ?1 AND season_id = ?2",
        params![user_team.team_id.0, season_id.0],
    )
    .unwrap();
    let offers = ClubOpsService::sponsor_offers(&conn, user_team.team_id, season_id, 74).unwrap();
    assert_eq!(offers.len(), 3, "three slots must be offered");
    assert!(offers[0].upfront > 0 && offers[0].weekly > 0);
    let budget_before: i64 = conn
        .query_row(
            "SELECT budget FROM teams WHERE team_id = ?1",
            params![user_team.team_id.0],
            |r| r.get(0),
        )
        .unwrap();
    let sponsorship_id =
        ClubOpsService::sign_sponsorship(&mut conn, user_team.team_id, season_id, &offers[0], date)
            .unwrap();
    let budget_after: i64 = conn
        .query_row(
            "SELECT budget FROM teams WHERE team_id = ?1",
            params![user_team.team_id.0],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(budget_after, budget_before + offers[0].upfront);
    let active =
        FinanceRepo::active_sponsorships_for_team(&conn, user_team.team_id, season_id).unwrap();
    assert_eq!(active.len(), 1);
    assert!(ClubOpsService::sign_sponsorship(
        &mut conn,
        user_team.team_id,
        season_id,
        &offers[0],
        date
    )
    .is_err());
    ClubOpsService::terminate_sponsorship(&conn, user_team.team_id, sponsorship_id).unwrap();
    assert!(
        FinanceRepo::active_sponsorships_for_team(&conn, user_team.team_id, season_id)
            .unwrap()
            .is_empty()
    );
    let reopened = ClubOpsService::sponsor_offers(&conn, user_team.team_id, season_id, 74).unwrap();
    assert_eq!(reopened.len(), 3);
}
