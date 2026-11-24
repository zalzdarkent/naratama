use std::path::PathBuf;

use chrono::NaiveDate;
use naratama_db::club_repo::{ClubRepo, MatchPersistPayload, MatchPlayerInsert, RedCardBanInput};
use naratama_db::reference_repo::ReferenceRepo;
use naratama_db::season_repo::NewFixtureRow;
use naratama_db::session::SessionManager;
use naratama_db::SeasonRepo;
use naratama_model::{
    CompetitionType, MatchStatus, PlayerId, SeasonId, StadiumId, TeamId, TournamentId,
};
use naratama_sim::{Lineup, MatchEngine, MatchInput};
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

fn insert_test_match(
    conn: &Connection,
    season_id: SeasonId,
    tournament_id: TournamentId,
    home: TeamId,
    away: TeamId,
    round: i32,
    date: NaiveDate,
) -> naratama_model::MatchId {
    let repo = SeasonRepo::new(conn);
    repo.insert_matches(&[NewFixtureRow {
        tournament_id,
        season_id,
        home_team_id: home,
        away_team_id: away,
        stadium_id: StadiumId(1),
        round,
        phase: 0,
        group_id: None,
        leg: 1,
        date,
        neutral_venue: false,
        extra_time: false,
        away_goals_rule: false,
    }])
    .unwrap();
    repo.matches_for_tournament(season_id, tournament_id)
        .unwrap()
        .last()
        .unwrap()
        .match_id
}

#[test]
#[allow(clippy::too_many_lines, clippy::similar_names)]
fn twelve_competition_scoped_suspension_scenarios() {
    let (_dir, mut conn) = setup_session();
    let ref_repo = ReferenceRepo::new(&conn);
    let teams = ref_repo.all_teams().unwrap();
    let t_a = teams[0].team_id;
    let t_b = teams[1].team_id;
    let t_c = teams[2].team_id;
    let p_a1 = ClubRepo::squad_for_team(&conn, t_a).unwrap()[0].player_id;
    let p_a2 = ClubRepo::squad_for_team(&conn, t_a).unwrap()[1].player_id;
    let p_c1 = ClubRepo::squad_for_team(&conn, t_c).unwrap()[0].player_id;

    let season = SeasonRepo::new(&conn).insert_season(2025).unwrap();
    let s1 = season.season_id;
    let d = NaiveDate::from_ymd_opt(2025, 9, 1).unwrap();

    // Scenario 1: League direct red (2-match ban) suspends ONLY in League, eligible in Cup/Continental/Friendly
    let m1 = insert_test_match(&conn, s1, TournamentId(1), t_a, t_b, 1, d);
    ClubRepo::save_match_result(
        &mut conn,
        &MatchPersistPayload {
            match_id: m1,
            season_id: s1,
            tournament_id: TournamentId(1),
            competition_type: CompetitionType::League,
            home_team_id: t_a,
            away_team_id: t_b,
            home_goals: 1,
            away_goals: 0,
            winner_id: Some(t_a),
            penalty_home_goals: None,
            penalty_away_goals: None,
            attendance_total: 20_000,
            ticket_revenue: 300_000,
            match_date: d,
            player_rows: Vec::new(),
            incidents_json: Vec::new(),
            red_card_bans: vec![RedCardBanInput {
                player_id: p_a1,
                matches_banned: 2,
            }],
            injury_return_dates: Vec::new(),
        },
    )
    .unwrap();
    assert!(
        ClubRepo::banned_players_for_competition(&conn, s1, CompetitionType::League, t_a, t_b)
            .unwrap()
            .contains(&p_a1),
        "Scenario 1: banned in League"
    );
    assert!(
        !ClubRepo::banned_players_for_competition(
            &conn,
            s1,
            CompetitionType::DomesticCup,
            t_a,
            t_b
        )
        .unwrap()
        .contains(&p_a1),
        "Scenario 1: eligible in DomesticCup"
    );
    assert!(
        !ClubRepo::banned_players_for_competition(
            &conn,
            s1,
            CompetitionType::Continental,
            t_a,
            t_b
        )
        .unwrap()
        .contains(&p_a1),
        "Scenario 1: eligible in Continental"
    );
    assert!(
        !ClubRepo::banned_players_for_competition(&conn, s1, CompetitionType::Friendly, t_a, t_b)
            .unwrap()
            .contains(&p_a1),
        "Scenario 1: eligible in Friendly"
    );

    // Scenario 2: DomesticCup red card (p_a2) suspends ONLY in DomesticCup, NOT League
    let m2 = insert_test_match(&conn, s1, TournamentId(2), t_a, t_b, 1, d);
    ClubRepo::save_match_result(
        &mut conn,
        &MatchPersistPayload {
            match_id: m2,
            season_id: s1,
            tournament_id: TournamentId(2),
            competition_type: CompetitionType::DomesticCup,
            home_team_id: t_a,
            away_team_id: t_b,
            home_goals: 2,
            away_goals: 1,
            winner_id: Some(t_a),
            penalty_home_goals: None,
            penalty_away_goals: None,
            attendance_total: 15_000,
            ticket_revenue: 200_000,
            match_date: d,
            player_rows: Vec::new(),
            incidents_json: Vec::new(),
            red_card_bans: vec![RedCardBanInput {
                player_id: p_a2,
                matches_banned: 1,
            }],
            injury_return_dates: Vec::new(),
        },
    )
    .unwrap();
    assert!(
        ClubRepo::banned_players_for_competition(&conn, s1, CompetitionType::DomesticCup, t_a, t_b)
            .unwrap()
            .contains(&p_a2),
        "Scenario 2: p_a2 banned in DomesticCup"
    );
    assert!(
        !ClubRepo::banned_players_for_competition(&conn, s1, CompetitionType::League, t_a, t_b)
            .unwrap()
            .contains(&p_a2),
        "Scenario 2: p_a2 eligible in League"
    );

    // Scenario 3: Continental red card (p_c1) isolated to Continental
    let m3 = insert_test_match(&conn, s1, TournamentId(49), t_c, t_b, 1, d);
    ClubRepo::save_match_result(
        &mut conn,
        &MatchPersistPayload {
            match_id: m3,
            season_id: s1,
            tournament_id: TournamentId(49),
            competition_type: CompetitionType::Continental,
            home_team_id: t_c,
            away_team_id: t_b,
            home_goals: 0,
            away_goals: 0,
            winner_id: None,
            penalty_home_goals: None,
            penalty_away_goals: None,
            attendance_total: 40_000,
            ticket_revenue: 900_000,
            match_date: d,
            player_rows: Vec::new(),
            incidents_json: Vec::new(),
            red_card_bans: vec![RedCardBanInput {
                player_id: p_c1,
                matches_banned: 2,
            }],
            injury_return_dates: Vec::new(),
        },
    )
    .unwrap();
    assert!(
        ClubRepo::banned_players_for_competition(&conn, s1, CompetitionType::Continental, t_c, t_b)
            .unwrap()
            .contains(&p_c1),
        "Scenario 3: p_c1 banned in Continental"
    );
    assert!(
        !ClubRepo::banned_players_for_competition(&conn, s1, CompetitionType::League, t_c, t_b)
            .unwrap()
            .contains(&p_c1),
        "Scenario 3: p_c1 eligible in League"
    );

    // Scenario 4: Friendly red card stays in Friendly only (does not pollute League)
    let m4 = insert_test_match(&conn, s1, TournamentId(9999), t_b, t_c, 1, d);
    let p_b1 = ClubRepo::squad_for_team(&conn, t_b).unwrap()[0].player_id;
    ClubRepo::save_match_result(
        &mut conn,
        &MatchPersistPayload {
            match_id: m4,
            season_id: s1,
            tournament_id: TournamentId(9999),
            competition_type: CompetitionType::Friendly,
            home_team_id: t_b,
            away_team_id: t_c,
            home_goals: 1,
            away_goals: 1,
            winner_id: None,
            penalty_home_goals: None,
            penalty_away_goals: None,
            attendance_total: 5_000,
            ticket_revenue: 50_000,
            match_date: d,
            player_rows: Vec::new(),
            incidents_json: Vec::new(),
            red_card_bans: vec![RedCardBanInput {
                player_id: p_b1,
                matches_banned: 1,
            }],
            injury_return_dates: Vec::new(),
        },
    )
    .unwrap();
    assert!(
        !ClubRepo::banned_players_for_competition(&conn, s1, CompetitionType::League, t_b, t_c)
            .unwrap()
            .contains(&p_b1),
        "Scenario 4: friendly ban must not bleed into League"
    );

    // Scenario 5 & 7: Playing 1 League match for t_a vs t_b ticks p_a1 from 2 -> 1, leaves t_c's Continental ban & t_a's Cup ban untouched!
    let m5 = insert_test_match(&conn, s1, TournamentId(1), t_a, t_b, 2, d);
    ClubRepo::save_match_result(
        &mut conn,
        &MatchPersistPayload {
            match_id: m5,
            season_id: s1,
            tournament_id: TournamentId(1),
            competition_type: CompetitionType::League,
            home_team_id: t_a,
            away_team_id: t_b,
            home_goals: 2,
            away_goals: 0,
            winner_id: Some(t_a),
            penalty_home_goals: None,
            penalty_away_goals: None,
            attendance_total: 22_000,
            ticket_revenue: 320_000,
            match_date: d,
            player_rows: Vec::new(),
            incidents_json: Vec::new(),
            red_card_bans: Vec::new(),
            injury_return_dates: Vec::new(),
        },
    )
    .unwrap();
    assert!(
        ClubRepo::banned_players_for_competition(&conn, s1, CompetitionType::League, t_a, t_b)
            .unwrap()
            .contains(&p_a1),
        "Scenario 5: p_a1 still has 1 League match remaining after serving 1 of 2"
    );
    assert!(
        ClubRepo::banned_players_for_competition(&conn, s1, CompetitionType::DomesticCup, t_a, t_b)
            .unwrap()
            .contains(&p_a2),
        "Scenario 7: p_a2 Cup ban untouched by League fixture"
    );
    assert!(
        ClubRepo::banned_players_for_competition(&conn, s1, CompetitionType::Continental, t_c, t_b)
            .unwrap()
            .contains(&p_c1),
        "Scenario 7: p_c1 Continental ban untouched by t_a vs t_b League fixture"
    );

    // Scenario 8: Weekly untyped tick does NOT touch typed League/Cup/Continental bans, only NULL legacy rows
    conn.execute(
        "INSERT INTO suspensions (player_id, type, matches_remaining, season_id, competition_type)
         VALUES (?1, 2, 1, ?2, NULL)",
        params![p_a2.0, s1.0],
    )
    .unwrap();
    ClubRepo::tick_untyped_suspensions(&mut conn, s1).unwrap();
    assert!(
        ClubRepo::banned_players_for_competition(&conn, s1, CompetitionType::League, t_a, t_b)
            .unwrap()
            .contains(&p_a1),
        "Scenario 8: typed League ban untouched by weekly tick"
    );
    let untyped_left: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM suspensions WHERE competition_type IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        untyped_left, 0,
        "Scenario 8: legacy NULL row ticked down & cleared"
    );

    // Scenario 6 & 12: Multi-competition ban + double stacking in League on p_a1
    let m6 = insert_test_match(&conn, s1, TournamentId(2), t_a, t_b, 2, d);
    ClubRepo::save_match_result(
        &mut conn,
        &MatchPersistPayload {
            match_id: m6,
            season_id: s1,
            tournament_id: TournamentId(2),
            competition_type: CompetitionType::DomesticCup,
            home_team_id: t_a,
            away_team_id: t_b,
            home_goals: 1,
            away_goals: 1,
            winner_id: None,
            penalty_home_goals: None,
            penalty_away_goals: None,
            attendance_total: 18_000,
            ticket_revenue: 240_000,
            match_date: d,
            player_rows: Vec::new(),
            incidents_json: Vec::new(),
            red_card_bans: vec![RedCardBanInput {
                player_id: p_a1,
                matches_banned: 1,
            }],
            injury_return_dates: Vec::new(),
        },
    )
    .unwrap();
    // Now p_a1 has 1 in League AND 1 in DomesticCup simultaneously, while p_a2's 1-match Cup ban just expired!
    assert!(
        !ClubRepo::banned_players_for_competition(
            &conn,
            s1,
            CompetitionType::DomesticCup,
            t_a,
            t_b
        )
        .unwrap()
        .contains(&p_a2),
        "Scenario 5: p_a2 1-match Cup ban expired after 1 Cup match"
    );
    assert!(
        ClubRepo::banned_players_for_competition(&conn, s1, CompetitionType::DomesticCup, t_a, t_b)
            .unwrap()
            .contains(&p_a1),
        "Scenario 12: p_a1 banned in DomesticCup AND League independently"
    );

    // Play 1 more League match with ANOTHER red card on p_a1 -> stacks! (1 -> 0, then +2 => 2!)
    let m7 = insert_test_match(&conn, s1, TournamentId(1), t_a, t_c, 3, d);
    ClubRepo::save_match_result(
        &mut conn,
        &MatchPersistPayload {
            match_id: m7,
            season_id: s1,
            tournament_id: TournamentId(1),
            competition_type: CompetitionType::League,
            home_team_id: t_a,
            away_team_id: t_c,
            home_goals: 1,
            away_goals: 2,
            winner_id: Some(t_c),
            penalty_home_goals: None,
            penalty_away_goals: None,
            attendance_total: 25_000,
            ticket_revenue: 350_000,
            match_date: d,
            player_rows: Vec::new(),
            incidents_json: Vec::new(),
            red_card_bans: vec![RedCardBanInput {
                player_id: p_a1,
                matches_banned: 2,
            }],
            injury_return_dates: Vec::new(),
        },
    )
    .unwrap();
    let susp_a1 = ClubRepo::active_suspensions_for_team(&conn, s1, t_a).unwrap();
    let league_rem = susp_a1
        .iter()
        .find(|(pid, c, _)| *pid == p_a1 && c == "league")
        .map(|(_, _, rem)| *rem);
    assert_eq!(
        league_rem,
        Some(2),
        "Scenario 6: double ban stacks accurately"
    );

    // Scenario 9: Season-scoped isolation: Season 2 queries do not see Season 1 bans
    let s2 = SeasonRepo::new(&conn)
        .insert_season(2026)
        .unwrap()
        .season_id;
    assert!(
        !ClubRepo::banned_players_for_competition(&conn, s2, CompetitionType::League, t_a, t_b)
            .unwrap()
            .contains(&p_a1),
        "Scenario 9: Season 1 ban does not apply in Season 2 when season_id differs"
    );

    // Scenario 10: Legacy NULL competition_type acts as 'league'
    let p_leg = ClubRepo::squad_for_team(&conn, t_b).unwrap()[1].player_id;
    conn.execute(
        "INSERT INTO suspensions (player_id, type, matches_remaining, season_id, competition_type)
         VALUES (?1, 2, 1, ?2, NULL)",
        params![p_leg.0, s2.0],
    )
    .unwrap();
    assert!(
        ClubRepo::banned_players_for_competition(&conn, s2, CompetitionType::League, t_b, t_c)
            .unwrap()
            .contains(&p_leg),
        "Scenario 10: NULL competition_type matches League"
    );
    assert!(
        !ClubRepo::banned_players_for_competition(
            &conn,
            s2,
            CompetitionType::DomesticCup,
            t_b,
            t_c
        )
        .unwrap()
        .contains(&p_leg),
        "Scenario 10: NULL competition_type does NOT match DomesticCup"
    );

    // Scenario 11: MatchEngine simulation automatically excludes banned player from played minutes
    let home_sq = ClubRepo::squad_for_team(&conn, t_a).unwrap();
    let away_sq = ClubRepo::squad_for_team(&conn, t_b).unwrap();
    let banned =
        ClubRepo::banned_players_for_competition(&conn, s1, CompetitionType::League, t_a, t_b)
            .unwrap();
    assert!(banned.contains(&p_a1));
    let mut rng = StdRng::seed_from_u64(42);
    let sim = MatchEngine::simulate(
        &mut rng,
        &MatchInput {
            home_squad: home_sq.clone(),
            away_squad: away_sq.clone(),
            home_team_id: t_a,
            away_team_id: t_b,
            ..MatchInput::default()
        },
        false,
        &banned,
    );
    assert!(
        !sim.player_rows.iter().any(|r| r.player_id == p_a1),
        "Scenario 11: banned player must not appear in simulated match_players"
    );
    let _ = Lineup::best(&home_sq, naratama_sim::Formation::F442, 70, &banned, &[]);
    let _ = MatchPlayerInsert {
        player_id: PlayerId(0),
        team_id: TeamId(0),
        starter: false,
        minutes: 0,
        goals: 0,
        assists: 0,
        yellow_cards: 0,
        red_cards: 0,
        mvp: false,
        rating: 0,
        points: 0,
    };
    let _ = MatchStatus::Scheduled;
}
