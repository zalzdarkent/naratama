use naratama_model::{
    tournament_ids, CompetitionConfig, CompetitionFormat, CompetitionType, Tournament,
    TournamentId, TournamentOrg,
};

/// Resolves a `Tournament` row into its centralised `CompetitionConfig` (24 known profiles + fallback).
#[must_use]
pub fn config_for_tournament(
    tournament: &Tournament,
    season_start_year: Option<i32>,
) -> CompetitionConfig {
    let tid = tournament.tournament_id;
    match tid.get() {
        x if x == tournament_ids::CHAMPIONS_LEAGUE.get()
            || x == tournament_ids::EUROPA_LEAGUE.get() =>
        {
            league_phase_profile(tournament, season_start_year, 8, 4, 4)
        }
        x if x == tournament_ids::CONFERENCE_LEAGUE.get() => {
            league_phase_profile(tournament, season_start_year, 6, 3, 3)
        }
        x if x == tournament_ids::ACL_ELITE.get()
            || x == tournament_ids::ACL_TWO.get()
            || x == tournament_ids::AFC_CHALLENGE_LEAGUE.get()
            || x == tournament_ids::CONCACAF_CHAMPIONS_CUP.get()
            || x == tournament_ids::LIBERTADORES.get()
            || x == tournament_ids::SUDAMERICANA.get()
            || x == tournament_ids::CAF_CHAMPIONS_LEAGUE.get() =>
        {
            continental_group_profile(tournament, season_start_year, 2)
        }
        x if x == tournament_ids::CLUB_WORLD_CUP.get() => {
            intl_group_profile(tournament, season_start_year, 16, 0, 4, false, 1, true)
        }
        x if x == tournament_ids::UEFA_SUPER_CUP.get()
            || x == tournament_ids::AFC_SUPER_CUP.get() =>
        {
            super_cup_profile(tournament, season_start_year)
        }
        x if x == tournament_ids::INTERCONTINENTAL_CUP.get() => CompetitionConfig {
            competition_type: CompetitionType::International,
            format: CompetitionFormat::Knockout,
            participant_count: Some(8),
            knockout_legs: 1,
            final_single_match: true,
            neutral_final: true,
            extra_time: true,
            penalties: true,
            use_byes: true,
            ..base_config(tournament, season_start_year)
        },
        x if x == tournament_ids::WORLD_CUP.get() => {
            let (field, groups) = if season_start_year.is_some_and(|y| y >= 2026) {
                (48, 12)
            } else {
                (32, 8)
            };
            CompetitionConfig {
                field_size: Some(field),
                ..intl_group_profile(
                    tournament,
                    season_start_year,
                    field,
                    groups,
                    4,
                    false,
                    1,
                    true,
                )
            }
        }
        x if x == tournament_ids::EURO_CUP.get() || x == tournament_ids::ASIAN_CUP.get() => {
            CompetitionConfig {
                field_size: Some(24),
                ..intl_group_profile(tournament, season_start_year, 24, 6, 4, false, 1, true)
            }
        }
        x if x == tournament_ids::COPA_AMERICA.get() || x == tournament_ids::GOLD_CUP.get() => {
            intl_group_profile(tournament, season_start_year, 16, 0, 4, false, 1, true)
        }
        x if x == tournament_ids::AFRICA_CUP.get() => {
            intl_group_profile(tournament, season_start_year, 24, 0, 4, false, 1, true)
        }
        x if x == tournament_ids::INTL_FRIENDLY.get() => CompetitionConfig {
            competition_type: CompetitionType::Friendly,
            format: CompetitionFormat::SingleMatch,
            final_single_match: true,
            knockout_legs: 1,
            extra_time: false,
            penalties: false,
            ..base_config(tournament, season_start_year)
        },
        x if x == tournament_ids::NATIONS_LEAGUE.get() => CompetitionConfig {
            field_size: Some(16),
            double_round_robin: true,
            promotion_slots: 4,
            relegation_slots: 4,
            ..intl_group_profile(tournament, season_start_year, 16, 0, 4, true, 1, true)
        },
        x if x == tournament_ids::ASEAN_CHAMPIONSHIP.get() => CompetitionConfig {
            field_size: Some(10),
            ..intl_group_profile(tournament, season_start_year, 10, 2, 5, false, 2, false)
        },
        x if x == tournament_ids::FIFA_ASEAN_CUP.get() => CompetitionConfig {
            field_size: Some(16),
            ..intl_group_profile(tournament, season_start_year, 16, 4, 4, false, 1, true)
        },
        _ => fallback_config(tournament, season_start_year),
    }
}

/// Returns a synthetic fallback config when a tournament row does not exist in DB.
#[must_use]
pub fn synthetic_fallback_config(
    tournament_id: TournamentId,
    season_start_year: Option<i32>,
) -> CompetitionConfig {
    let synthetic = Tournament {
        tournament_id,
        name: format!("Tournament {tournament_id}"),
        organization: TournamentOrg::International,
        division_id: 1,
        slots: 16,
        promotion_slots: 0,
        relegation_spots: 0,
        points: 3.0,
        prize_teams: 0,
        prize_top_scorer: 0,
        prize_mvp: 0,
        play_days: "6,7".to_string(),
        rounds: 1,
        logo_base64: None,
    };
    config_for_tournament(&synthetic, season_start_year)
}

#[must_use]
pub fn parse_weekdays(raw: &str) -> Vec<u32> {
    let mut days: Vec<u32> = raw
        .split(',')
        .filter_map(|tok| tok.trim().parse::<u32>().ok())
        .filter(|&d| (1..=7).contains(&d))
        .collect();
    days.sort_unstable();
    days.dedup();
    days
}

fn base_config(tournament: &Tournament, season_start_year: Option<i32>) -> CompetitionConfig {
    CompetitionConfig {
        tournament_id: tournament.tournament_id,
        name: tournament.name.clone(),
        competition_type: CompetitionType::League,
        format: CompetitionFormat::DoubleRoundRobin,
        season_start_year,
        participant_count: None,
        double_round_robin: true,
        matchday_interval_days: 7,
        min_rest_days: 2,
        group_size: 4,
        group_count: 0,
        group_double_round_robin: true,
        group_qualifiers_per_group: 2,
        league_phase_opponents: 0,
        league_phase_home_matches: 0,
        league_phase_away_matches: 0,
        league_phase_qualifiers: 0,
        knockout_legs: 1,
        final_single_match: true,
        neutral_final: false,
        extra_time: true,
        penalties: true,
        away_goals: false,
        use_byes: true,
        trim_to_power_of_two: false,
        field_size: None,
        promotion_slots: 0,
        relegation_slots: 0,
        unavailable_dates: Vec::new(),
        allowed_weekdays: Vec::new(),
    }
}

fn fallback_config(tournament: &Tournament, season_start_year: Option<i32>) -> CompetitionConfig {
    match tournament.organization {
        TournamentOrg::League => CompetitionConfig {
            competition_type: CompetitionType::League,
            format: CompetitionFormat::DoubleRoundRobin,
            double_round_robin: true,
            allowed_weekdays: parse_weekdays(&tournament.play_days),
            promotion_slots: tournament.promotion_slots,
            relegation_slots: tournament.relegation_spots,
            ..base_config(tournament, season_start_year)
        },
        TournamentOrg::NationalCup => CompetitionConfig {
            competition_type: CompetitionType::DomesticCup,
            format: CompetitionFormat::Knockout,
            knockout_legs: if tournament.rounds == 2 { 2 } else { 1 },
            final_single_match: true,
            neutral_final: true,
            trim_to_power_of_two: true,
            ..base_config(tournament, season_start_year)
        },
        TournamentOrg::International => {
            if tournament.slots <= 2 {
                let count = if tournament.slots > 0 {
                    tournament.slots
                } else {
                    2
                };
                CompetitionConfig {
                    competition_type: CompetitionType::SuperCup,
                    format: CompetitionFormat::SingleMatch,
                    participant_count: Some(count),
                    field_size: Some(count),
                    final_single_match: true,
                    neutral_final: true,
                    knockout_legs: 1,
                    ..base_config(tournament, season_start_year)
                }
            } else if tournament.slots <= 8 {
                CompetitionConfig {
                    competition_type: CompetitionType::International,
                    format: CompetitionFormat::Knockout,
                    participant_count: Some(tournament.slots),
                    field_size: Some(tournament.slots),
                    knockout_legs: 1,
                    final_single_match: true,
                    neutral_final: true,
                    use_byes: true,
                    ..base_config(tournament, season_start_year)
                }
            } else {
                continental_group_profile(tournament, season_start_year, 2)
            }
        }
        TournamentOrg::Friendly => CompetitionConfig {
            competition_type: CompetitionType::Friendly,
            format: CompetitionFormat::SingleMatch,
            final_single_match: true,
            knockout_legs: 1,
            extra_time: false,
            penalties: false,
            ..base_config(tournament, season_start_year)
        },
    }
}

fn league_phase_profile(
    tournament: &Tournament,
    season_start_year: Option<i32>,
    opponents: i32,
    home: i32,
    away: i32,
) -> CompetitionConfig {
    CompetitionConfig {
        competition_type: CompetitionType::Continental,
        format: CompetitionFormat::LeaguePhase,
        participant_count: Some(36),
        field_size: Some(36),
        league_phase_opponents: opponents,
        league_phase_home_matches: home,
        league_phase_away_matches: away,
        league_phase_qualifiers: 16,
        knockout_legs: 2,
        final_single_match: true,
        neutral_final: true,
        ..base_config(tournament, season_start_year)
    }
}

fn continental_group_profile(
    tournament: &Tournament,
    season_start_year: Option<i32>,
    knockout_legs: i32,
) -> CompetitionConfig {
    CompetitionConfig {
        competition_type: CompetitionType::Continental,
        format: CompetitionFormat::GroupStage,
        participant_count: Some(tournament.slots),
        field_size: Some(tournament.slots),
        group_size: 4,
        group_double_round_robin: true,
        knockout_legs,
        final_single_match: true,
        neutral_final: true,
        use_byes: true,
        ..base_config(tournament, season_start_year)
    }
}

fn super_cup_profile(tournament: &Tournament, season_start_year: Option<i32>) -> CompetitionConfig {
    CompetitionConfig {
        competition_type: CompetitionType::SuperCup,
        format: CompetitionFormat::SingleMatch,
        participant_count: Some(2),
        field_size: Some(2),
        final_single_match: true,
        neutral_final: true,
        knockout_legs: 1,
        ..base_config(tournament, season_start_year)
    }
}

#[allow(clippy::too_many_arguments)]
fn intl_group_profile(
    tournament: &Tournament,
    season_start_year: Option<i32>,
    participants: i32,
    group_count: i32,
    group_size: i32,
    group_double_rr: bool,
    knockout_legs: i32,
    final_single_match: bool,
) -> CompetitionConfig {
    CompetitionConfig {
        competition_type: CompetitionType::International,
        format: CompetitionFormat::GroupStage,
        participant_count: Some(participants),
        field_size: Some(participants),
        group_count,
        group_size,
        group_double_round_robin: group_double_rr,
        knockout_legs,
        final_single_match,
        neutral_final: final_single_match,
        use_byes: true,
        ..base_config(tournament, season_start_year)
    }
}
