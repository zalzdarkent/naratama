use std::path::PathBuf;

use chrono::NaiveDate;
use naratama_db::reference_repo::ReferenceRepo;
use naratama_db::session::SessionManager;
use naratama_engine::{
    build_crest_data_uri, BadgeShape, BadgeSymbol, ClubOpsService, CustomClubRequest,
    CustomClubService, GenerateCareerRequest, InitialSquadPreset, MatchdayService, ReadService,
    SeasonGenerator, StadiumArchitecture,
};
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
fn wave13_crest_generation_reads_and_squad_statistics() {
    // 1. Crest data URIs are deterministic, valid base64 and honour every shape/symbol.
    let base_req = CustomClubRequest {
        name: "Nusantara FC".into(),
        abbreviation: "NFC".into(),
        country_id: CountryId(94),
        founded_year: 2025,
        primary_color: "#E5A93C".into(),
        secondary_color: "#070D18".into(),
        stadium_name: "Gelanggang".into(),
        stadium_architecture: StadiumArchitecture::CompactModern,
        squad_preset: InitialSquadPreset::Competitive,
        starting_budget: 10_000_000,
        nickname: Some("Garuda".into()),
        badge_shape: Some(BadgeShape::Shield),
        badge_symbol: Some(BadgeSymbol::Soccer),
        jersey_pattern: None,
    };
    let crest = build_crest_data_uri(&base_req);
    assert!(crest.starts_with("data:image/svg+xml;base64,"));
    assert_eq!(crest, build_crest_data_uri(&base_req));
    let mut variants = Vec::new();
    for (shape, symbol) in [
        (BadgeShape::Shield, BadgeSymbol::Soccer),
        (BadgeShape::Circle, BadgeSymbol::Star),
        (BadgeShape::Diamond, BadgeSymbol::Crown),
        (BadgeShape::Classic, BadgeSymbol::Eagle),
        (BadgeShape::Shield, BadgeSymbol::Flame),
    ] {
        let mut req = base_req.clone();
        req.badge_shape = Some(shape);
        req.badge_symbol = Some(symbol);
        variants.push(build_crest_data_uri(&req));
    }
    variants.dedup();
    assert!(variants.len() >= 4, "each badge combo must differ");
    let mut bad_color = base_req.clone();
    bad_color.primary_color = "not-a-color".into();
    assert!(build_crest_data_uri(&bad_color).starts_with("data:image/svg+xml;base64,"));

    // 2. A custom club stores its generated crest.
    let dir = tempdir().unwrap();
    let mgr = SessionManager::new(dir.path(), template_db_path());
    mgr.create_slot_from_seed(1).unwrap();
    let mut conn = mgr.begin_session(1).unwrap();
    let mut rng = StdRng::seed_from_u64(13_131);
    let date = NaiveDate::from_ymd_opt(2025, 7, 1).unwrap();
    let created = CustomClubService::create_custom_club(
        &mut conn,
        &base_req,
        naratama_model::SeasonId(1),
        date,
        &mut rng,
    )
    .unwrap();
    assert!(created
        .team
        .logo_base64
        .as_deref()
        .unwrap_or("")
        .starts_with("data:image/svg+xml;base64,"));
    let stored: Option<String> = conn
        .query_row(
            "SELECT logo_base64 FROM teams WHERE team_id = ?1",
            params![created.team.team_id.0],
            |r| r.get(0),
        )
        .unwrap();
    assert!(stored.unwrap_or_default().starts_with("data:image"));

    // 3. Career for the read surfaces.
    let ref_repo = ReferenceRepo::new(&conn);
    let england = CountryId(44);
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

    // Squad statistics: pre-season rows exist with zeroed numbers (LEFT JOIN keeps every player).
    let stats = ReadService::squad_statistics(&conn, user_team.team_id, season_id).unwrap();
    assert!(!stats.is_empty());
    assert!(stats.iter().all(|s| s.matches_played >= 0));

    // Play a matchday so rating/statistics data exists, then re-read.
    let next_date: Option<String> = conn
        .query_row(
            "SELECT MIN(date) FROM matches WHERE season_id = ?1 AND status = 1 AND date IS NOT NULL",
            params![season_id.0],
            |r| r.get(0),
        )
        .unwrap();
    let match_date = NaiveDate::parse_from_str(next_date.as_deref().unwrap(), "%Y-%m-%d").unwrap();
    MatchdayService::simulate_matchday(
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
    let match_stats: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM match_players mp JOIN matches m ON m.match_id = mp.match_id
             WHERE m.season_id = ?1 AND mp.team_id = ?2",
            params![season_id.0, user_team.team_id.0],
            |r| r.get(0),
        )
        .unwrap();
    assert!(match_stats > 0);

    // 4. Logo map + national leaders.
    let logos = ReadService::logo_map(&conn, &[user_team.team_id.0, 999_999_999]).unwrap();
    assert_eq!(logos.len(), 1, "only clubs with a crest are returned");
    let leaders = ReadService::national_leaders(&conn, 10).unwrap();
    assert!(leaders.len() <= 10);
    assert!(leaders.iter().all(|l| l.caps >= 0));

    // 5. Scheduled transfers list stays queryable (empty before any agreed deal).
    assert!(ReadService::scheduled_transfers(&conn, user_team.team_id)
        .unwrap()
        .is_empty());

    // 6. Team detail carries the full squad, payroll and trophies.
    let detail = ReadService::team_detail(&conn, user_team.team_id)
        .unwrap()
        .expect("team detail");
    assert_eq!(
        detail.squad.len(),
        usize::try_from(detail.squad_size).unwrap()
    );
    assert!(detail.weekly_payroll > 0);
    assert!(detail.trophies.len() <= 64);

    // 7. Club ops still works on the crest-bearing custom club.
    conn.execute(
        "UPDATE teams SET budget = 20000000 WHERE team_id = ?1",
        params![created.team.team_id.0],
    )
    .unwrap();
    let expansion =
        ClubOpsService::expand_stadium(&mut conn, created.team.team_id, season_id, date).unwrap();
    assert!(expansion.added_seats > 0);
}
