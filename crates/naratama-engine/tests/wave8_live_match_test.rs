use std::path::PathBuf;

use chrono::NaiveDate;
use naratama_db::reference_repo::ReferenceRepo;
use naratama_db::session::SessionManager;
use naratama_engine::{GenerateCareerRequest, MatchdayService, SeasonGenerator};
use naratama_model::{CountryId, MatchStatus};
use naratama_sim::{LiveMatch, MatchMentality, MatchSide};
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
fn wave8_live_match_half_time_adjustments_commit_and_finish_matchday() {
    let dir = tempdir().unwrap();
    let mgr = SessionManager::new(dir.path(), template_db_path());
    mgr.create_slot_from_seed(1).unwrap();
    let mut conn = mgr.begin_session(1).unwrap();
    let mut rng = StdRng::seed_from_u64(8_888);

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

    let next_date_str: Option<String> = conn
        .query_row(
            "SELECT MIN(date) FROM matches WHERE season_id = ?1 AND status = ?2 AND date IS NOT NULL",
            params![plan.season.season_id.0, MatchStatus::Scheduled as i64],
            |r| r.get(0),
        )
        .unwrap();
    let date = NaiveDate::parse_from_str(
        next_date_str.as_deref().expect("a scheduled matchday"),
        "%Y-%m-%d",
    )
    .unwrap();

    // 1. Prepare the user's fixture and play it interactively to the break.
    let (fixture, prepared) =
        MatchdayService::prepare_live_match(&conn, plan.season.season_id, date, user_team.team_id)
            .unwrap()
            .expect("user fixture must be live-ready");
    assert_eq!(fixture.status, MatchStatus::Scheduled);
    let user_side = if fixture.home_team_id == user_team.team_id {
        MatchSide::Home
    } else {
        MatchSide::Away
    };

    let mut live = LiveMatch::from_input(&prepared.input, true, &prepared.banned);
    assert_eq!(live.advance_to(&mut rng, 45), 45);
    let on_pitch = live.on_pitch(user_side);
    let bench = live.bench(user_side);
    assert!(!bench.is_empty(), "bench must offer substitutes");
    let outgoing = on_pitch
        .iter()
        .find(|p| p.position_id != 1)
        .map_or(on_pitch[0].player_id, |p| p.player_id);
    live.adjust(
        &mut rng,
        user_side,
        &[(outgoing, bench[0].player_id)],
        Some(MatchMentality::Attacking),
    )
    .expect("half-time adjustment must be accepted");
    assert_eq!(live.advance_to(&mut rng, 90), 90);
    let half_time_goals = live.score();
    assert!(half_time_goals.0 >= 0 && half_time_goals.1 >= 0);
    let live_result = live.finish(&mut rng);
    assert_eq!(live_result.home_goals, half_time_goals.0);

    // 2. Commit it through the normal matchday path and finish the rest of the day.
    let outcomes = MatchdayService::simulate_matchday(
        &mut conn,
        plan.season.season_id,
        date,
        None,
        true,
        user_team.team_id,
        &mut rng,
        Some(&(fixture.match_id, live_result.clone())),
    )
    .unwrap();
    assert!(!outcomes.is_empty(), "rest of the matchday must sim");
    let user_outcome = outcomes
        .iter()
        .find(|o| o.match_id == fixture.match_id)
        .expect("live result must be persisted as an outcome");
    assert_eq!(user_outcome.home_goals, live_result.home_goals);
    assert_eq!(user_outcome.away_goals, live_result.away_goals);

    let stored: (i64, i64, i64) = conn
        .query_row(
            "SELECT COALESCE(home_goals, -1), COALESCE(away_goals, -1), status
             FROM matches WHERE match_id = ?1",
            params![fixture.match_id.0],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(stored.0, i64::from(live_result.home_goals));
    assert_eq!(stored.1, i64::from(live_result.away_goals));
    assert_eq!(stored.2, MatchStatus::Finished as i64);

    // The substitution is stored in the match timeline for the replay viewer.
    let incident_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM match_incidents WHERE match_id = ?1",
            params![fixture.match_id.0],
            |r| r.get(0),
        )
        .unwrap();
    assert!(incident_count > 0, "live match timeline must be stored");

    // No fixture of that date is left scheduled.
    let remaining: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM matches WHERE season_id = ?1 AND date = ?2 AND status = ?3",
            params![
                plan.season.season_id.0,
                date.format("%Y-%m-%d").to_string(),
                MatchStatus::Scheduled as i64
            ],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(remaining, 0);
}
