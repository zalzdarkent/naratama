use std::path::PathBuf;

use chrono::NaiveDate;
use naratama_db::reference_repo::ReferenceRepo;
use naratama_db::session::SessionManager;
use naratama_engine::{GenerateCareerRequest, MatchdayService, ReadService, SeasonGenerator};
use naratama_model::{CountryId, MatchStatus, SeasonId};
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
fn wave9_read_service_exposes_schedule_bracket_stats_and_history() {
    let dir = tempdir().unwrap();
    let mgr = SessionManager::new(dir.path(), template_db_path());
    mgr.create_slot_from_seed(1).unwrap();
    let mut conn = mgr.begin_session(1).unwrap();
    let mut rng = StdRng::seed_from_u64(9_191);

    let england = CountryId(44);
    let ref_repo = ReferenceRepo::new(&conn);
    let leagues = ref_repo.leagues_for_country(england).unwrap();
    let league = ref_repo
        .tournament_by_id(leagues[0].tournament_id)
        .unwrap()
        .unwrap();
    let cup = ref_repo.national_cup(england).unwrap();
    let teams = ref_repo.teams_for_division(league.tournament_id).unwrap();
    let user_team = teams[0].clone();

    let plan = SeasonGenerator::generate_career(
        &conn,
        &GenerateCareerRequest {
            country_id: england,
            league: league.clone(),
            cup: cup.clone(),
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

    // 1. Schedule + standings for the user league.
    let overview =
        ReadService::competition_overview(&conn, season_id, league.tournament_id).unwrap();
    assert!(!overview.fixtures.is_empty(), "fixtures must be listed");
    assert_eq!(
        overview.standings.len(),
        teams.len(),
        "league standings must cover the whole division"
    );
    assert_eq!(overview.standings[0].played, 0);
    assert!(
        overview
            .fixtures
            .iter()
            .all(|f| !f.home_name.starts_with("Club #")),
        "fixture rows must resolve club names"
    );

    // 2. Cup bracket appears once knockout rounds exist.
    if let Some(cup_tour) = cup.as_ref() {
        let cup_overview =
            ReadService::competition_overview(&conn, season_id, cup_tour.tournament_id).unwrap();
        assert!(!cup_overview.fixtures.is_empty(), "cup fixtures must exist");
    }

    // 3. Play the first matchday and read the match statistics back.
    let next_date_str: Option<String> = conn
        .query_row(
            "SELECT MIN(date) FROM matches WHERE season_id = ?1 AND status = ?2 AND date IS NOT NULL",
            params![season_id.0, MatchStatus::Scheduled as i64],
            |r| r.get(0),
        )
        .unwrap();
    let date = NaiveDate::parse_from_str(next_date_str.as_deref().unwrap(), "%Y-%m-%d").unwrap();
    MatchdayService::simulate_matchday(
        &mut conn,
        season_id,
        date,
        None,
        true,
        user_team.team_id,
        &mut rng,
        None,
    )
    .unwrap();

    let played_match: i64 = conn
        .query_row(
            "SELECT match_id FROM matches
             WHERE season_id = ?1 AND status = ?2 AND (home_team_id = ?3 OR away_team_id = ?3)
             ORDER BY match_id LIMIT 1",
            params![
                season_id.0,
                MatchStatus::Finished as i64,
                user_team.team_id.0
            ],
            |r| r.get(0),
        )
        .unwrap();
    let stats = ReadService::match_statistics(&conn, naratama_model::MatchId(played_match))
        .unwrap()
        .expect("match statistics must exist for a played match");
    assert!(!stats.players.is_empty(), "player rows must be stored");
    assert!(
        stats.home_shots + stats.away_shots > 0,
        "team shot statistics must be persisted"
    );
    assert!(stats.home_possession > 0);
    assert!(
        stats.players.iter().any(|p| p.rating > 0 && p.minutes > 0),
        "player ratings must be stored"
    );

    // 4. Competition leaders are populated for the league.
    let overview =
        ReadService::competition_overview(&conn, season_id, league.tournament_id).unwrap();
    assert!(
        !overview.top_scorers.is_empty(),
        "top scorers must be available after a matchday"
    );

    // 5. Season history: honours for a played season stay empty-safe, awards readable.
    let honours = ReadService::season_honours(&conn, season_id).unwrap();
    assert!(honours.len() <= 64, "honours list stays bounded");
    let awards = ReadService::season_awards(&conn, season_id).unwrap();
    assert!(awards.len() <= 64);

    // 6. Team + player detail and the tactical dossier.
    let detail = ReadService::team_detail(&conn, user_team.team_id)
        .unwrap()
        .expect("team detail");
    assert_ne!(detail.team.name, "");
    assert!(detail.squad_size > 0);
    assert!(detail.stadium.is_some());
    assert!(!detail.recent_results.is_empty());

    let player = ReadService::player_detail(&conn, detail.top_players[0].player_id)
        .unwrap()
        .expect("player detail");
    assert_eq!(player.player.player_id, detail.top_players[0].player_id);

    let (opponent_name, dossier) = ReadService::tactical_dossier(&conn, user_team.team_id, 3)
        .unwrap()
        .expect("dossier");
    assert_ne!(opponent_name, "");
    assert!(
        !dossier.counter_tactics.is_empty(),
        "dossier must advise counters"
    );

    // 7. Seasons listing feeds the history selector.
    let seasons = ReadService::seasons(&conn).unwrap();
    assert!(seasons.iter().any(|s| s.season_id == SeasonId(season_id.0)));
}
