use std::collections::HashSet;

use naratama_model::{
    tournament_ids, Confederation, CountryId, FfpEnforcement, MarketStatus, Player, PlayerId,
    PlayerStatus, PreferredFoot, SquadStatus, TeamId, TournamentId, TournamentOrg,
};
use naratama_sim::{
    build_tactical_dossier, calculate_fifa_elo, match_importance, CompetitionKind,
    CompetitionProfile, CounterTactic, EconomyScale, FfpAssessmentInput, FfpLevel, FifaEloInput,
    FinanceCalc, Formation, Lineup, MatchEngine, MatchEventType, MatchInput, MatchOutlook,
    PlayerValuationInput, PlayerValuationService, PrizeEngine, PsrInput, TacticalStyle,
    Vulnerability,
};
use rand::rngs::StdRng;
use rand::SeedableRng;

fn make_player(id: i64, position_id: i32, power: i32, fitness: i32) -> Player {
    Player {
        player_id: PlayerId(id),
        name: format!("Player {id}"),
        age: 24,
        position_id,
        preferred_foot: PreferredFoot::Right,
        power,
        potential: (power + 5).min(99),
        fitness,
        value: 1_000_000,
        salary: 10_000,
        country_id: CountryId(94),
        squad_status: SquadStatus::FirstTeam,
        market_status: MarketStatus::Active,
        status: PlayerStatus::Active,
        is_captain: false,
        shirt_number: (id % 99) as i32 + 1,
        stars: 3,
        skill_a: 0,
        skill_b: 0,
        height: 180,
        championships: None,
        injury_return_date: None,
    }
}

fn make_squad(base_id: i64, power: i32) -> Vec<Player> {
    let positions = [
        1, 1, // 2 GK
        2, 2, 3, 4, 4, 4, // 6 DEF
        5, 6, 7, 7, 8, 8, // 6 MID
        9, 10, 10, 11, // 4 ATT
    ];
    positions
        .iter()
        .enumerate()
        .map(|(i, &pos)| make_player(base_id + i as i64, pos, power, 95))
        .collect()
}

#[test]
fn match_engine_is_deterministic_with_seeded_rng() {
    let input = MatchInput {
        home_squad: make_squad(100, 76),
        away_squad: make_squad(200, 74),
        home_team_id: TeamId(1),
        away_team_id: TeamId(2),
        home_power: 76,
        away_power: 74,
        ..MatchInput::default()
    };
    let banned = HashSet::new();

    let mut rng_a = StdRng::seed_from_u64(42);
    let mut rng_b = StdRng::seed_from_u64(42);
    let res_a = MatchEngine::simulate(&mut rng_a, &input, true, &banned);
    let res_b = MatchEngine::simulate(&mut rng_b, &input, true, &banned);

    assert_eq!(res_a.home_goals, res_b.home_goals);
    assert_eq!(res_a.away_goals, res_b.away_goals);
    assert_eq!(res_a.events, res_b.events);
    assert_eq!(res_a.player_rows, res_b.player_rows);
    assert_eq!(res_a.home_possession, res_b.home_possession);
}

#[test]
fn match_engine_realistic_averages_and_neutral_venue_symmetry() {
    let equal_input = MatchInput {
        home_squad: make_squad(100, 75),
        away_squad: make_squad(200, 75),
        home_team_id: TeamId(1),
        away_team_id: TeamId(2),
        home_power: 75,
        away_power: 75,
        neutral_venue: true,
        ..MatchInput::default()
    };
    let banned = HashSet::new();
    let mut rng = StdRng::seed_from_u64(1337);
    let mut total_goals = 0_i32;
    let mut home_wins = 0_i32;
    let mut away_wins = 0_i32;

    for _ in 0..300 {
        let res = MatchEngine::simulate(&mut rng, &equal_input, false, &banned);
        total_goals += res.home_goals + res.away_goals;
        assert_eq!(res.home_possession, 50);
        if res.home_goals > res.away_goals {
            home_wins += 1;
        } else if res.away_goals > res.home_goals {
            away_wins += 1;
        }
    }
    let avg_goals = f64::from(total_goals) / 300.0;
    assert!(
        (1.8..=3.8).contains(&avg_goals),
        "unexpected average goals: {avg_goals}"
    );
    assert!(
        (home_wins - away_wins).abs() < 45,
        "neutral venue should be balanced: home={home_wins}, away={away_wins}"
    );

    // Dominant team (88 vs 60) must win the vast majority of matches.
    let mismatch = MatchInput {
        home_squad: make_squad(300, 88),
        away_squad: make_squad(400, 60),
        home_team_id: TeamId(3),
        away_team_id: TeamId(4),
        home_power: 88,
        away_power: 60,
        ..MatchInput::default()
    };
    let mut fav_wins = 0;
    for _ in 0..100 {
        let res = MatchEngine::simulate(&mut rng, &mismatch, false, &banned);
        if res.home_goals > res.away_goals {
            fav_wins += 1;
        }
    }
    assert!(fav_wins >= 75, "dominant side won only {fav_wins}/100");
}

#[test]
fn bug7_home_substitute_receives_home_outcome_points() {
    let input = MatchInput {
        home_squad: make_squad(100, 90),
        away_squad: make_squad(200, 55),
        home_team_id: TeamId(10),
        away_team_id: TeamId(20),
        home_power: 90,
        away_power: 55,
        ..MatchInput::default()
    };
    let banned = HashSet::new();
    let mut rng = StdRng::seed_from_u64(99);

    for _ in 0..30 {
        let res = MatchEngine::simulate(&mut rng, &input, true, &banned);
        if res.home_goals <= res.away_goals {
            continue;
        }
        for row in &res.player_rows {
            if row.team_id == TeamId(10) && !row.starter {
                // Winning side outcome = 3, so points must be 3 + 2 * goals (never 0 + 2 * goals).
                assert_eq!(row.points, 3 + row.goals * 2);
                return;
            }
        }
    }
    panic!("expected at least one home win with a home substitute");
}

#[test]
fn bug16_penalty_shootout_always_decides_cleanly_and_handles_early_termination() {
    let input = MatchInput {
        home_squad: make_squad(100, 75),
        away_squad: make_squad(200, 75),
        home_team_id: TeamId(1),
        away_team_id: TeamId(2),
        ..MatchInput::default()
    };
    let mut rng = StdRng::seed_from_u64(2026);
    for _ in 0..200 {
        let pen = MatchEngine::penalty_shootout(&mut rng, &input);
        assert_ne!(pen.home_score, pen.away_score);
        assert!(pen
            .events
            .iter()
            .all(|e| e.event_type == MatchEventType::ShootoutKick));
        let home_kicks = pen
            .events
            .iter()
            .filter(|e| e.side == naratama_sim::MatchSide::Home)
            .count() as i32;
        let away_kicks = pen
            .events
            .iter()
            .filter(|e| e.side == naratama_sim::MatchSide::Away)
            .count() as i32;
        if home_kicks <= 5 && away_kicks <= 5 {
            // Early termination must only happen when trailing team cannot catch up.
            let home_rem = 5 - home_kicks;
            let away_rem = 5 - away_kicks;
            assert!(
                pen.home_score > pen.away_score + away_rem
                    || pen.away_score > pen.home_score + home_rem
            );
        } else {
            // Sudden death always finishes with equal kicks taken.
            assert_eq!(home_kicks, away_kicks);
        }
    }
}

#[test]
fn adr004_extra_time_runs_when_level_at_90() {
    let input = MatchInput {
        home_squad: make_squad(100, 75),
        away_squad: make_squad(200, 75),
        home_team_id: TeamId(1),
        away_team_id: TeamId(2),
        home_power: 75,
        away_power: 75,
        neutral_venue: true,
        extra_time: true,
        ..MatchInput::default()
    };
    let banned = HashSet::new();
    let mut rng = StdRng::seed_from_u64(7);
    let mut saw_extra_time = false;
    for _ in 0..50 {
        let res = MatchEngine::simulate(&mut rng, &input, true, &banned);
        if res.went_to_extra_time {
            saw_extra_time = true;
            assert!(res.player_rows.iter().any(|r| r.minutes == 120));
            break;
        }
    }
    assert!(
        saw_extra_time,
        "expected at least one draw triggering extra time"
    );
}

#[test]
fn lineup_selection_and_tactical_dossier_work() {
    let mut squad = make_squad(1, 75);
    // Low fitness GK stays eligible (unlike outfielder); when both GKs are tired, the fitter one starts.
    squad[0].fitness = 20;
    squad[1].fitness = 10;
    squad[15].fitness = 20;
    let banned = HashSet::from([squad[2].player_id]);

    let lineup = Lineup::best(&squad, Formation::F433, 35, &banned, &[]);
    assert_eq!(lineup.starters.len(), 11);
    assert!(!lineup.unavailable_ids.contains(&squad[0].player_id));
    assert!(lineup
        .starters
        .iter()
        .any(|p| p.player_id == squad[0].player_id));
    assert!(lineup.unavailable_ids.contains(&squad[15].player_id));
    assert!(lineup.unavailable_ids.contains(&squad[2].player_id));
    assert!(lineup.substitutes.len() <= 7);

    // Tactical dossier checks
    squad[0].power = 65; // weak GK vs opponent_power 84
    squad[3].fitness = 70;
    squad[4].fitness = 72;
    squad[5].fitness = 68;
    let dossier = build_tactical_dossier(84, Formation::F433, &squad, 4);
    assert_eq!(dossier.tactical_style, TacticalStyle::HighPressWingOverload);
    assert_eq!(dossier.match_outlook, MatchOutlook::ToughHighTempo);
    assert_eq!(dossier.confidence_rating, 89);
    assert_eq!(dossier.counter_tactics.len(), 3);
    assert!(dossier
        .counter_tactics
        .contains(&CounterTactic::ShortCombinationPassingThroughPivot));
    assert!(dossier
        .vulnerabilities
        .iter()
        .any(|v| matches!(v, Vulnerability::WeakGoalkeeper { .. })));
    assert!(dossier
        .vulnerabilities
        .iter()
        .any(|v| matches!(v, Vulnerability::FatiguedSquad { .. })));
}

#[test]
fn finance_calc_broadcast_and_ffp_invariants() {
    for clubs in [1_usize, 8, 12, 18, 20, 24] {
        let pool = FinanceCalc::broadcast_pool(85, Confederation::Uefa, 1, clubs);
        let dist = FinanceCalc::broadcast_distribution(pool, clubs);
        assert_eq!(dist.total_distributed(), pool);
        assert_eq!(dist.shares.len(), clubs);
        if clubs > 1 {
            assert!(dist.shares[0] >= dist.shares[clubs - 1]);
        }
    }

    let md = FinanceCalc::matchday(1_000_000, 40_000);
    assert_eq!(md.hospitality, 200_000);
    assert_eq!(md.merchandise, 120_000);
    assert_eq!(md.operating_cost, 100_000);
    assert_eq!(md.net, 1_220_000);

    assert_eq!(FinanceCalc::agent_fee(0), 0);
    assert_eq!(FinanceCalc::agent_fee(500_000), 50_000);
    assert_eq!(FinanceCalc::agent_fee(10_000_000), 500_000);
    assert_eq!(
        FinanceCalc::sell_on_amount(20_000_000, 10_000_000, 0.15),
        1_500_000
    );
    assert_eq!(FinanceCalc::book_value(30_000_000, 3, 365), 20_000_000);

    let psr = FinanceCalc::psr_result(PsrInput {
        ledger_net: -25_000_000,
        board_funding: 5_000_000,
        facility_capex: 2_000_000,
        transfer_fees_paid: 10_000_000,
        amortization: 4_000_000,
        ..PsrInput::default()
    });
    assert_eq!(psr, -22_000_000);

    let base_ffp = FfpAssessmentInput {
        annual_player_wages: 55_000_000,
        annual_staff_wages: 5_000_000,
        annual_amortization: 20_000_000,
        agent_fees: 5_000_000,
        football_revenue: 80_000_000,
        psr_result: -22_000_000,
        psr_revenue: 80_000_000,
        enforcement: FfpEnforcement::Off,
    };
    let off = FinanceCalc::assess_ffp(base_ffp);
    assert_eq!(off.level, FfpLevel::HardBlock);
    assert!(!off.transfer_blocked);

    let strict = FinanceCalc::assess_ffp(FfpAssessmentInput {
        enforcement: FfpEnforcement::Strict,
        ..base_ffp
    });
    assert!(strict.transfer_blocked);
}

#[test]
fn prize_engine_and_valuation_and_elo_parity() {
    // PrizeEngine
    let engine = PrizeEngine::default();
    assert_eq!(
        PrizeEngine::kind_of(TournamentOrg::International, 2),
        CompetitionKind::SuperCup
    );
    assert_eq!(
        PrizeEngine::continental_tier_override(tournament_ids::EUROPA_LEAGUE),
        2
    );
    assert_eq!(
        PrizeEngine::continental_tier_override(tournament_ids::CONFERENCE_LEAGUE),
        3
    );

    let teams: Vec<TeamId> = (1..=20).map(TeamId).collect();
    let res = engine.league_prizes(
        TournamentId(1),
        CompetitionProfile {
            kind: CompetitionKind::League,
            tier: 1,
            entrants: 20,
            country_power: 85,
            confederation: Confederation::Uefa,
            ..CompetitionProfile::default()
        },
        &teams,
    );
    assert_eq!(res.awards.len(), 20);
    assert!(res.awards[0].amount > res.awards[19].amount);
    assert!((EconomyScale::confederation(Confederation::Conmebol) - 0.95).abs() < 1e-9);
    assert!((EconomyScale::confederation(Confederation::Caf) - 0.90).abs() < 1e-9);

    // PlayerValuationService
    let val = PlayerValuationService::default();
    assert!((val.calculate_base_value(70) - 3_200_000.0).abs() < 1e-6);
    assert!((val.calculate_base_value(99) - 240_000_000.0).abs() < 1e-6);

    let mv = val.calculate_target_market_value(PlayerValuationInput {
        power: 95,
        potential: Some(98),
        age: 22,
        position_id: 10,
        confederation: Confederation::Uefa,
        team_power: 90,
        appearances: 30,
        avg_rating: Some(8.2),
        goals: 28,
        assists: 10,
        clean_sheets: 0,
        contract_months_remaining: 48,
    });
    assert!((15_000..=250_000_000).contains(&mv));
    let smoothed = val.smooth_value_update(10_000_000, 50_000_000, None);
    assert_eq!(smoothed, 11_500_000);

    // FIFA Elo SUM + knockout protection
    assert!((match_importance(tournament_ids::WORLD_CUP, 3) - 60.0).abs() < 1e-9);
    let ko_loss = calculate_fifa_elo(FifaEloInput {
        home_points: 1800.0,
        away_points: 1500.0,
        tournament_id: tournament_ids::WORLD_CUP,
        phase: 3,
        home_goals: 0,
        away_goals: 1,
        has_penalties: false,
        home_won_penalties: false,
    });
    assert!(
        (ko_loss.delta_home - 0.0).abs() < 1e-9,
        "FIFA knockout protection must prevent point loss"
    );
    assert!(ko_loss.delta_away > 40.0);
    assert!((ko_loss.new_home_points - 1800.0).abs() < 1e-9);
}
