use naratama_db::finance_repo::FinanceRepo;
use naratama_db::logo_service::{seed_default_club_logos, seed_default_league_logos};
use naratama_db::reference_repo::ReferenceRepo;
use naratama_db::session::SessionManager;
use naratama_model::{CountryId, FfpEnforcement};
use rusqlite::params;
use std::path::PathBuf;
use tempfile::TempDir;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .to_path_buf()
}

fn seed_db_path() -> PathBuf {
    workspace_root().join("assets/data/superkickoff_2025.db")
}

#[test]
#[allow(clippy::too_many_lines)]
fn wave0_session_and_reference_queries_work() {
    let root = workspace_root();
    let seed = seed_db_path();
    assert!(seed.exists(), "Seed DB missing at {}", seed.display());

    let temp = TempDir::new().expect("tempdir");
    let mgr = SessionManager::new(temp.path(), &seed);

    // 1. All 5 slots start empty.
    let initial_slots = mgr.get_all_slots_info().expect("initial slots");
    assert_eq!(initial_slots.len(), 5);
    assert!(initial_slots.iter().all(|s| s.is_empty));

    // 2. Create slot 1 from seed & verify base counts + migrations.
    let slot1_path = mgr.create_slot_from_seed(1).expect("create slot 1");
    assert!(slot1_path.exists());

    let conn = mgr.begin_session(1).expect("begin session");

    let countries: i64 = conn
        .query_row("SELECT COUNT(*) FROM countries", [], |r| r.get(0))
        .expect("countries count");
    let teams: i64 = conn
        .query_row("SELECT COUNT(*) FROM teams", [], |r| r.get(0))
        .expect("teams count");
    let players: i64 = conn
        .query_row("SELECT COUNT(*) FROM players", [], |r| r.get(0))
        .expect("players count");
    let tournaments: i64 = conn
        .query_row("SELECT COUNT(*) FROM tournaments", [], |r| r.get(0))
        .expect("tournaments count");

    assert_eq!(countries, 254);
    assert_eq!(teams, 2014);
    assert_eq!(players, 55624);
    assert!(tournaments >= 228);

    // Verify ReferenceRepo queries (Indonesia = CountryId(94)).
    let repo = ReferenceRepo::new(&conn);
    let playable = repo.playable_countries().expect("playable countries");
    assert!(!playable.is_empty());

    let id_leagues = repo
        .leagues_for_country(CountryId(94))
        .expect("Indonesia leagues");
    assert!(
        !id_leagues.is_empty(),
        "Expected playable leagues for Indonesia (94)"
    );

    let first_league = &id_leagues[0];
    let div_teams = repo
        .teams_for_division(first_league.tournament_id)
        .expect("teams for division");
    assert!(!div_teams.is_empty());

    let first_team = &div_teams[0];
    assert!(first_team.budget >= 500_000);
    let stadium = repo
        .stadium_by_id(first_team.stadium_id)
        .expect("stadium")
        .expect("stadium exists");
    assert!(stadium.capacity_total() > 0);

    // Verify LogoService seeding on slot session (clear one team & one tournament logo to test backfill).
    conn.execute(
        "UPDATE teams SET logo_base64 = NULL WHERE team_id = ?1",
        params![first_team.team_id.get()],
    )
    .expect("clear one team logo");
    conn.execute(
        "UPDATE tournaments SET logo_base64 = NULL WHERE tournament_id IN (SELECT tournament_id FROM tournaments WHERE logo_base64 IS NOT NULL LIMIT 1)",
        [],
    )
    .expect("clear one tournament logo");
    let club_logos = root.join("assets/data/default_club_logos.json");
    let league_logos = root.join("assets/data/default_league_logos.json");
    let seeded_clubs = seed_default_club_logos(&conn, &club_logos).expect("seed club logos");
    let seeded_leagues =
        seed_default_league_logos(&conn, &league_logos).expect("seed league logos");
    assert!(seeded_clubs >= 1);
    assert!(seeded_leagues >= 1);

    // Verify team power refresh against team_players.
    let refreshed = repo.refresh_all_team_powers().expect("refresh team powers");
    assert!(refreshed > 0);

    // 3. Test `.session` rollback when save == false.
    conn.execute(
        "INSERT INTO local_game_state (
            id, manager_name, manager_country_id, user_team_id,
            current_season_id, current_date, game_mode, ffp_enforcement,
            created_at, updated_at
        ) VALUES (1, 'Coach Rollback', 94, ?1, 1, '2025-07-01', 'career', 'off', '2025-07-01', '2025-07-01')",
        params![first_team.team_id.get()],
    )
    .expect("insert state");
    mgr.finish_session(1, conn, false)
        .expect("rollback session");

    let slots_after_rollback = mgr.get_all_slots_info().expect("slots after rollback");
    assert!(slots_after_rollback[0].is_empty);

    // 4. Test `.session` commit when save == true.
    let conn2 = mgr.begin_session(1).expect("begin session 2");
    conn2
        .execute(
            "INSERT INTO local_game_state (
                id, manager_name, manager_country_id, user_team_id,
                current_season_id, current_date, game_mode, ffp_enforcement,
                created_at, updated_at
            ) VALUES (1, 'Coach Persisted', 94, ?1, 1, '2025-07-01', 'career', 'off', '2025-07-01', '2025-07-01')",
            params![first_team.team_id.get()],
        )
        .expect("insert persisted state");
    mgr.finish_session(1, conn2, true).expect("commit session");

    let slots_after_save = mgr.get_all_slots_info().expect("slots after save");
    assert!(!slots_after_save[0].is_empty);
    assert_eq!(
        slots_after_save[0].manager_name.as_deref(),
        Some("Coach Persisted")
    );

    // 5. Delete slot 1.
    mgr.delete_slot(1).expect("delete slot 1");
    let slots_after_delete = mgr.get_all_slots_info().expect("slots after delete");
    assert!(slots_after_delete[0].is_empty);
}

#[test]
fn wave6_save_export_import_roundtrip_and_ffp_mode() {
    let temp = TempDir::new().expect("tempdir");
    let mgr = SessionManager::new(temp.path(), seed_db_path());
    mgr.create_slot_from_seed(1).expect("create slot 1");

    let conn = mgr.begin_session(1).expect("begin session");
    conn.execute(
        "INSERT INTO local_game_state (
            id, manager_name, manager_country_id, user_team_id,
            current_season_id, current_date, game_mode, ffp_enforcement,
            created_at, updated_at
        ) VALUES (1, 'Coach Archive', 94, 1, 1, '2025-07-01', 'career', 'off', '2025-07-01', '2025-07-01')",
        [],
    )
    .expect("insert state");
    mgr.finish_session(1, conn, true).expect("commit session");

    let original = mgr.get_all_slots_info().expect("slots")[0].clone();

    // Export, wipe, and import back into the same slot.
    let archive = mgr.export_slot(1).expect("export slot");
    assert!(archive.exists(), "archive must be written");
    assert_eq!(mgr.list_exports().expect("list exports").len(), 1);

    mgr.delete_slot(1).expect("delete slot");
    assert!(mgr.get_all_slots_info().expect("slots")[0].is_empty);

    mgr.import_slot(1, &archive).expect("import slot");
    let restored = mgr.get_all_slots_info().expect("slots")[0].clone();
    assert_eq!(restored.manager_name, original.manager_name);
    assert_eq!(restored.game_date, original.game_date);

    // Archives outside saves/exports are rejected.
    let outside = temp.path().join("rogue.zip");
    std::fs::copy(&archive, &outside).expect("copy archive outside exports");
    assert!(mgr.import_slot(2, &outside).is_err());
    assert!(mgr.get_all_slots_info().expect("slots")[1].is_empty);

    // FFP mode is persisted for the career.
    let conn = mgr.begin_session(1).expect("begin session");
    FinanceRepo::set_mode(&conn, FfpEnforcement::Strict).expect("set ffp mode");
    assert_eq!(
        FinanceRepo::settings(&conn).expect("settings").mode,
        FfpEnforcement::Strict
    );
    mgr.finish_session(1, conn, true).expect("commit");
    let conn = mgr.begin_session(1).expect("begin session 2");
    assert_eq!(
        FinanceRepo::settings(&conn).expect("settings").mode,
        FfpEnforcement::Strict
    );
}
