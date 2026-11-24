use naratama_model::{tournament_ids, TournamentId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FifaEloInput {
    pub home_points: f64,
    pub away_points: f64,
    pub tournament_id: TournamentId,
    pub phase: i32,
    pub home_goals: i32,
    pub away_goals: i32,
    pub has_penalties: bool,
    pub home_won_penalties: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FifaEloOutput {
    pub new_home_points: f64,
    pub new_away_points: f64,
    pub delta_home: f64,
    pub delta_away: f64,
}

/// FIFA SUM importance weight (`I`) for an international match.
#[must_use]
pub const fn match_importance(tournament_id: TournamentId, phase: i32) -> f64 {
    if tournament_id.get() == tournament_ids::INTL_FRIENDLY.get() {
        10.0
    } else if tournament_id.get() == tournament_ids::NATIONS_LEAGUE.get() {
        15.0
    } else if tournament_id.get() == tournament_ids::WORLD_CUP.get() {
        if phase >= 3 {
            60.0
        } else {
            50.0
        }
    } else if tournament_id.get() == tournament_ids::ASIAN_CUP.get()
        || tournament_id.get() == tournament_ids::EURO_CUP.get()
    {
        if phase >= 2 {
            40.0
        } else {
            35.0
        }
    } else if tournament_id.get() == tournament_ids::FIFA_ASEAN_CUP.get() {
        if phase >= 2 {
            25.0
        } else {
            20.0
        }
    } else {
        25.0
    }
}

/// Computes updated FIFA World Ranking points using the official SUM Elo model,
/// including penalty-shootout scoring (`0.75` / `0.50`) and final-tournament
/// knockout loss protection.
#[must_use]
pub fn calculate_fifa_elo(input: FifaEloInput) -> FifaEloOutput {
    let weight = match_importance(input.tournament_id, input.phase);
    let dr = input.home_points - input.away_points;
    let expected_home = 1.0 / (10.0_f64.powf(-dr / 600.0) + 1.0);
    let expected_away = 1.0 - expected_home;

    let (actual_home, actual_away) = if input.has_penalties {
        if input.home_won_penalties {
            (0.75, 0.50)
        } else {
            (0.50, 0.75)
        }
    } else if input.home_goals > input.away_goals {
        (1.0, 0.0)
    } else if input.away_goals > input.home_goals {
        (0.0, 1.0)
    } else {
        (0.5, 0.5)
    };

    let mut delta_home = weight * (actual_home - expected_home);
    let mut delta_away = weight * (actual_away - expected_away);

    let tid = input.tournament_id.get();
    let is_final_knockout = input.phase > 0
        && (tid == tournament_ids::WORLD_CUP.get()
            || tid == tournament_ids::ASIAN_CUP.get()
            || tid == tournament_ids::EURO_CUP.get()
            || tid == tournament_ids::FIFA_ASEAN_CUP.get());
    if is_final_knockout {
        if delta_home < 0.0 {
            delta_home = 0.0;
        }
        if delta_away < 0.0 {
            delta_away = 0.0;
        }
    }

    let new_home = round_2dp(input.home_points + delta_home).max(0.0);
    let new_away = round_2dp(input.away_points + delta_away).max(0.0);

    FifaEloOutput {
        new_home_points: new_home,
        new_away_points: new_away,
        delta_home: round_2dp(delta_home),
        delta_away: round_2dp(delta_away),
    }
}

fn round_2dp(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}
