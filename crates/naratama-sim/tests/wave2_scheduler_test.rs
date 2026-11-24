use std::collections::HashMap;

use chrono::NaiveDate;
use naratama_model::{
    tournament_ids, CompetitionFormat, CountryId, FixtureMatch, GroupTable, MatchId, MatchStatus,
    SeasonId, StadiumId, TeamId, Tournament, TournamentId, TournamentOrg,
};
use naratama_sim::{
    compute_standings, config_for_tournament, snake_groups, FixtureValidator, KnockoutScheduler,
    LeaguePhaseScheduler, RoundRobinScheduler,
};

fn make_league_matches(
    tid: TournamentId,
    sid: SeasonId,
    slots: &[naratama_sim::FixtureSlot],
) -> Vec<FixtureMatch> {
    let base = NaiveDate::from_ymd_opt(2025, 8, 9).unwrap();
    slots
        .iter()
        .enumerate()
        .map(|(i, s)| FixtureMatch {
            match_id: MatchId(i as i64 + 1),
            tournament_id: tid,
            season_id: sid,
            round: s.round,
            phase: 1,
            group_id: s.group_id,
            status: MatchStatus::Scheduled,
            home_team_id: s.home,
            away_team_id: s.away,
            stadium_id: StadiumId(s.home.get()),
            date: Some(base + chrono::Duration::days(7 * i64::from(s.round - 1))),
            home_goals: None,
            away_goals: None,
            winner_id: None,
            penalty_home_goals: None,
            penalty_away_goals: None,
            leg: s.leg,
            original_date: None,
            reschedule_reason: None,
            neutral_venue: false,
            extra_time: false,
            away_goals_rule: false,
        })
        .collect()
}

#[test]
fn round_robin_even_and_odd_leagues_pass_validator() {
    let tid = TournamentId(1);
    let sid = SeasonId(1);
    for count in [8_i64, 10, 12, 18, 20, 7, 15] {
        let clubs: Vec<TeamId> = (1..=count).map(TeamId).collect();
        let slots = RoundRobinScheduler::schedule(&clubs, true, true, None);
        let matches = make_league_matches(tid, sid, &slots);
        let report = FixtureValidator::validate_league(tid, sid, &matches, &clubs, true, "league");
        assert!(
            report.is_valid(),
            "league size {count} failed validation:\n{}",
            report.describe()
        );
        assert_eq!(report.error_count(), 0);
    }
}

#[test]
fn ucl_and_uecl_league_phase_36_teams_pass_validator() {
    let clubs: Vec<TeamId> = (1..=36).map(TeamId).collect();
    let assoc: HashMap<TeamId, CountryId> = clubs
        .iter()
        .enumerate()
        .map(|(i, &id)| (id, CountryId((i % 9) as i64 + 1)))
        .collect();

    // UCL: 36 teams, 8 opponents (4H / 4A)
    let ucl = LeaguePhaseScheduler::generate(&clubs, 8, Some(4), Some(&assoc));
    let ucl_matches =
        make_league_matches(tournament_ids::CHAMPIONS_LEAGUE, SeasonId(1), &ucl.fixtures);
    let ucl_report = FixtureValidator::validate_league_phase(
        tournament_ids::CHAMPIONS_LEAGUE,
        SeasonId(1),
        &ucl_matches,
        &clubs,
        8,
        4,
        4,
    );
    assert!(
        ucl_report.is_valid(),
        "UCL league phase failed:\n{}",
        ucl_report.describe()
    );

    // UECL: 36 teams, 6 opponents (3H / 3A)
    let conf = LeaguePhaseScheduler::generate(&clubs, 6, Some(3), Some(&assoc));
    let conf_matches = make_league_matches(
        tournament_ids::CONFERENCE_LEAGUE,
        SeasonId(1),
        &conf.fixtures,
    );
    let conf_report = FixtureValidator::validate_league_phase(
        tournament_ids::CONFERENCE_LEAGUE,
        SeasonId(1),
        &conf_matches,
        &clubs,
        6,
        3,
        3,
    );
    assert!(
        conf_report.is_valid(),
        "UECL league phase failed:\n{}",
        conf_report.describe()
    );
}

#[test]
fn snake_groups_and_knockout_plans_work() {
    let clubs: Vec<TeamId> = (1..=16).map(TeamId).collect();
    let groups = snake_groups(&clubs, 4, 4);
    assert_eq!(groups.len(), 4);
    assert!(groups.iter().all(|g| g.len() == 4));

    let names: HashMap<TeamId, String> =
        clubs.iter().map(|&id| (id, format!("Club {id}"))).collect();
    let group_tables: Vec<GroupTable> = groups
        .iter()
        .enumerate()
        .map(|(idx, g)| GroupTable {
            group_id: idx as i32 + 1,
            rows: compute_standings(g, &names, &[]),
        })
        .collect();
    let ko = KnockoutScheduler::plan_group_knockout(&group_tables, 2);
    assert_eq!(ko.pairs.len(), 4);
    assert_eq!(ko.byes, [] as [TeamId; 0]);

    // Opening round with byes for 5 teams -> bracket 8, 3 byes, 1 match
    let five: Vec<TeamId> = (1..=5).map(TeamId).collect();
    let plan = KnockoutScheduler::plan_opening_round(&five, true, false, None);
    assert_eq!(plan.bracket_size, 8);
    assert_eq!(plan.byes.len(), 3);
    assert_eq!(plan.pairs.len(), 1);
}

#[test]
fn competition_config_profiles_and_standings_tiebreakers() {
    let mk_tour = |id: TournamentId, org: TournamentOrg, slots: i32| Tournament {
        tournament_id: id,
        name: "Test".to_string(),
        organization: org,
        division_id: 1,
        slots,
        promotion_slots: 0,
        relegation_spots: 0,
        points: 1.0,
        prize_teams: 0,
        prize_top_scorer: 0,
        prize_mvp: 0,
        play_days: "6,7".to_string(),
        rounds: 1,
        logo_base64: None,
    };

    let ucl = config_for_tournament(
        &mk_tour(
            tournament_ids::CHAMPIONS_LEAGUE,
            TournamentOrg::International,
            36,
        ),
        Some(2025),
    );
    assert_eq!(ucl.format, CompetitionFormat::LeaguePhase);
    assert_eq!(ucl.league_phase_opponents, 8);
    assert_eq!(ucl.league_phase_qualifiers, 16);

    let wc26 = config_for_tournament(
        &mk_tour(tournament_ids::WORLD_CUP, TournamentOrg::International, 48),
        Some(2026),
    );
    assert_eq!(wc26.participant_count, Some(48));
    assert_eq!(wc26.group_count, 12);

    let asean = config_for_tournament(
        &mk_tour(
            tournament_ids::ASEAN_CHAMPIONSHIP,
            TournamentOrg::International,
            10,
        ),
        Some(2025),
    );
    assert_eq!(asean.knockout_legs, 2);
    assert!(!asean.final_single_match);

    // Standings tiebreaker: points -> GD -> GF -> alphabetical name
    let t1 = TeamId(1);
    let t2 = TeamId(2);
    let t3 = TeamId(3);
    let names = HashMap::from([
        (t1, "Beta FC".to_string()),
        (t2, "Alpha FC".to_string()),
        (t3, "Gamma FC".to_string()),
    ]);
    let m = FixtureMatch {
        match_id: MatchId(1),
        tournament_id: TournamentId(1),
        season_id: SeasonId(1),
        round: 1,
        phase: 1,
        group_id: None,
        status: MatchStatus::Finished,
        home_team_id: t1,
        away_team_id: t2,
        stadium_id: StadiumId(1),
        date: NaiveDate::from_ymd_opt(2025, 8, 9),
        home_goals: Some(1),
        away_goals: Some(1),
        winner_id: None,
        penalty_home_goals: None,
        penalty_away_goals: None,
        leg: 1,
        original_date: None,
        reschedule_reason: None,
        neutral_venue: false,
        extra_time: false,
        away_goals_rule: false,
    };
    let table = compute_standings(&[t1, t2, t3], &names, &[m]);
    // t1 & t2 tied on 1 pt, 0 GD, 1 GF -> Alpha FC sorts before Beta FC; t3 has 0 pts.
    assert_eq!(table[0].team_id, t2);
    assert_eq!(table[1].team_id, t1);
    assert_eq!(table[2].team_id, t3);
}
