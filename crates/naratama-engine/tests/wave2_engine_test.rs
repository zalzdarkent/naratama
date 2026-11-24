use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use chrono::NaiveDate;
use naratama_db::reference_repo::ReferenceRepo;
use naratama_db::season_repo::SeasonRepo;
use naratama_db::session::SessionManager;
use naratama_engine::{
    FixtureCalendarService, GenerateCareerRequest, InternationalCalendarService, SeasonGenerator,
};
use naratama_model::{tournament_ids, CountryId, MatchStatus, Team, TeamId};
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
fn generate_england_career_validates_zero_errors_and_respects_fifa_blackouts() {
    let dir = tempdir().unwrap();
    let mgr = SessionManager::new(dir.path(), template_db_path());
    mgr.create_slot_from_seed(1).unwrap();
    let conn = mgr.begin_session(1).unwrap();

    let ref_repo = ReferenceRepo::new(&conn);
    // England (country_id = 44 in superkickoff_2025.db)
    let england = CountryId(44);
    let leagues = ref_repo.leagues_for_country(england).unwrap();
    assert!(!leagues.is_empty(), "England leagues must exist");

    let primary_tour = ref_repo
        .tournament_by_id(leagues[0].tournament_id)
        .unwrap()
        .unwrap();
    let mut other_leagues = Vec::new();
    for l in leagues.iter().skip(1) {
        if let Some(t) = ref_repo.tournament_by_id(l.tournament_id).unwrap() {
            other_leagues.push(t);
        }
    }
    let cup = ref_repo.national_cup(england).unwrap();
    let internationals = ref_repo.international_cups().unwrap();
    let div1_teams = ref_repo
        .teams_for_division(primary_tour.tournament_id)
        .unwrap();
    let user_team_id = div1_teams[0].team_id;

    let mut rng = StdRng::seed_from_u64(2025);
    let plan = SeasonGenerator::generate_career(
        &conn,
        &GenerateCareerRequest {
            country_id: england,
            league: primary_tour.clone(),
            cup: cup.clone(),
            internationals,
            other_leagues,
            start_year: 2025,
            user_team_id: Some(user_team_id),
            include_preseason_friendlies: true,
        },
        &mut rng,
    )
    .unwrap();

    assert_eq!(plan.international_windows.len(), 3);
    assert!(plan.match_count > 300);

    // 1. Validate the entire season (leagues, UCL/UEL/UECL, cups, global calendar).
    let report = FixtureCalendarService::validate_season(&conn, plan.season.season_id).unwrap();
    assert!(
        report.is_valid(),
        "generated season has validation errors:\n{}",
        report.describe()
    );
    assert_eq!(report.error_count(), 0);

    // 2. Verify zero club matches fall inside mandatory FIFA windows.
    let blackout: HashSet<NaiveDate> =
        InternationalCalendarService::mandatory_unavailable_dates(&conn, plan.season.season_id)
            .unwrap()
            .into_iter()
            .collect();
    let season_repo = SeasonRepo::new(&conn);
    let all_matches = season_repo
        .matches_for_season(plan.season.season_id)
        .unwrap();
    assert!(!all_matches.is_empty());

    for m in &all_matches {
        if m.status == MatchStatus::Cancelled
            || m.tournament_id == tournament_ids::PRE_SEASON_FRIENDLY
        {
            continue;
        }
        let d = m.date.expect("scheduled match must have date");
        assert!(
            !blackout.contains(&d),
            "club match {} (tournament {}) landed on FIFA blackout date {d}",
            m.match_id,
            m.tournament_id
        );
    }

    // 3. Test manual postpone_match preserves match_id & original_date, and passes validator.
    let target_match = all_matches
        .iter()
        .find(|m| m.tournament_id == primary_tour.tournament_id && m.round == 1)
        .expect("round 1 league match");
    let midweek_date = target_match.date.unwrap() + chrono::Duration::days(3);
    let postponed = FixtureCalendarService::postpone_match(
        &conn,
        target_match.match_id,
        "Waterlogged pitch",
        Some(midweek_date),
        NaiveDate::from_ymd_opt(2025, 8, 9).unwrap(),
    )
    .unwrap();
    assert!(postponed.success);
    assert_eq!(postponed.match_id, target_match.match_id);
    assert_ne!(postponed.new_date, target_match.date);

    let reloaded = season_repo
        .match_by_id(target_match.match_id)
        .unwrap()
        .unwrap();
    assert_eq!(reloaded.original_date, target_match.date);

    let post_report =
        FixtureCalendarService::validate_season(&conn, plan.season.season_id).unwrap();
    assert_eq!(
        post_report.error_count(),
        0,
        "post-postpone errors:\n{}",
        post_report.describe()
    );

    // 4. Bug #9 regression check: UCL standings must ignore phase > 0 knockout rows.
    let ucl_matches = season_repo
        .matches_for_tournament(plan.season.season_id, tournament_ids::CHAMPIONS_LEAGUE)
        .unwrap();
    if let Some(first_ucl) = ucl_matches.first() {
        let h = first_ucl.home_team_id;
        let a = first_ucl.away_team_id;
        let mut cache: HashMap<TeamId, Team> = HashMap::new();
        for tid in [h, a] {
            if let Some(tm) = ref_repo.team_by_id(tid).unwrap() {
                cache.insert(tid, tm);
            }
        }
        // Mark one phase-0 match finished (1-0) and insert a synthetic phase-1 knockout match (5-0).
        conn.execute(
            "UPDATE matches SET status = 2, home_goals = 1, away_goals = 0, winner_id = ?1 WHERE match_id = ?2",
            params![h.get(), first_ucl.match_id.get()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO matches (tournament_id, season_id, home_team_id, away_team_id, stadium_id, round, phase, group_id, status, leg, date, home_goals, away_goals, winner_id)
             VALUES (?1, ?2, ?3, ?4, 1, 1, 1, NULL, 2, 1, '2026-03-10', 5, 0, ?3)",
            params![
                tournament_ids::CHAMPIONS_LEAGUE.get(),
                plan.season.season_id.get(),
                h.get(),
                a.get()
            ],
        )
        .unwrap();
        let table = SeasonGenerator::standings(
            &conn,
            plan.season.season_id,
            tournament_ids::CHAMPIONS_LEAGUE,
            None,
            &cache,
        )
        .unwrap();
        let h_row = table.iter().find(|r| r.team_id == h).unwrap();
        assert_eq!(
            h_row.goals_for, 1,
            "knockout goals leaked into league phase standings"
        );
    }
}

#[test]
fn cup_progression_schedules_next_round_after_completion() {
    let dir = tempdir().unwrap();
    let mgr = SessionManager::new(dir.path(), template_db_path());
    mgr.create_slot_from_seed(2).unwrap();
    let conn = mgr.begin_session(2).unwrap();

    let ref_repo = ReferenceRepo::new(&conn);
    let indonesia = CountryId(94);
    let leagues = ref_repo.leagues_for_country(indonesia).unwrap();
    let primary_tour = ref_repo
        .tournament_by_id(leagues[0].tournament_id)
        .unwrap()
        .unwrap();
    let cup = ref_repo
        .national_cup(indonesia)
        .unwrap()
        .expect("Indonesia cup");

    let mut rng = StdRng::seed_from_u64(77);
    let plan = SeasonGenerator::generate_career(
        &conn,
        &GenerateCareerRequest {
            country_id: indonesia,
            league: primary_tour,
            cup: Some(cup.clone()),
            internationals: Vec::new(),
            other_leagues: Vec::new(),
            start_year: 2025,
            user_team_id: None,
            include_preseason_friendlies: false,
        },
        &mut rng,
    )
    .unwrap();

    // Mark every phase-1 cup fixture finished with home team winning.
    conn.execute(
        "UPDATE matches
         SET status = 2, home_goals = 2, away_goals = 0, winner_id = home_team_id
         WHERE season_id = ?1 AND tournament_id = ?2 AND phase = 1",
        params![plan.season.season_id.get(), cup.tournament_id.get()],
    )
    .unwrap();

    let cache: HashMap<TeamId, Team> = ref_repo
        .teams_for_country(indonesia)
        .unwrap()
        .into_iter()
        .map(|t| (t.team_id, t))
        .collect();
    let advanced = SeasonGenerator::maybe_schedule_next_cup_round(
        &conn,
        plan.season.season_id,
        cup.tournament_id,
        &cache,
        NaiveDate::from_ymd_opt(2025, 9, 1).unwrap(),
    )
    .unwrap();
    assert!(advanced, "expected phase 2 of domestic cup to be scheduled");

    let cup_matches = SeasonRepo::new(&conn)
        .matches_for_tournament(plan.season.season_id, cup.tournament_id)
        .unwrap();
    assert!(cup_matches.iter().any(|m| m.phase == 2));

    let report = FixtureCalendarService::validate_season(&conn, plan.season.season_id).unwrap();
    assert_eq!(report.error_count(), 0, "{}", report.describe());
}
