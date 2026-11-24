use naratama_model::{Player, PlayerId, PlayerStatus};
use serde::{Deserialize, Serialize};

use crate::formation::Formation;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TacticalStyle {
    HighPressWingOverload,
    MidfieldDominanceWingBackSurge,
    FluidPlaymakingBoxInfiltration,
    DeepDefensiveBlockRapidCounters,
    DirectPlayBalancedStructure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PredictedMentality {
    AggressiveAttacking,
    Possession,
    DynamicCombination,
    DefensiveCounter,
    Balanced,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Vulnerability {
    WeakGoalkeeper {
        player_id: PlayerId,
        player_name: String,
        power: i32,
    },
    WeakDefender {
        player_id: PlayerId,
        player_name: String,
        power: i32,
    },
    FatiguedSquad {
        tired_count: usize,
    },
    SolidDefenseFallback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CounterTactic {
    ExploitWideSpacesBehindWingBacks,
    HoldDeeperDefensiveLineAgainstWingers,
    PressInFinalThird,
    TargetWeakestDefensiveFlank,
    ShortCombinationPassingThroughPivot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MatchOutlook {
    ToughHighTempo,
    BalancedMidfieldBattle,
    FavorableFullPoints,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DangerPlayer {
    pub player_id: PlayerId,
    pub name: String,
    pub position_id: i32,
    pub power: i32,
}

/// Structured pre-match tactical report (localized at the UI layer per ADR-003).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TacticalDossier {
    pub formation: String,
    pub tactical_style: TacticalStyle,
    pub predicted_mentality: PredictedMentality,
    pub danger_men: Vec<DangerPlayer>,
    pub vulnerabilities: Vec<Vulnerability>,
    pub counter_tactics: Vec<CounterTactic>,
    pub match_outlook: MatchOutlook,
    pub confidence_rating: i32,
}

/// Builds a deterministic pre-match tactical dossier from opponent strength,
/// formation, squad, and the user's tactical analyst star rating (`0..=5`).
#[must_use]
pub fn build_tactical_dossier(
    opponent_power: i32,
    formation: Formation,
    squad: &[Player],
    analyst_stars: i32,
) -> TacticalDossier {
    let mut active: Vec<&Player> = squad
        .iter()
        .filter(|p| p.status == PlayerStatus::Active)
        .collect();
    active.sort_by_key(|a| std::cmp::Reverse(a.power));

    let danger_men = active
        .iter()
        .take(2)
        .map(|p| DangerPlayer {
            player_id: p.player_id,
            name: p.name.clone(),
            position_id: p.position_id,
            power: p.power,
        })
        .collect();

    let (tactical_style, predicted_mentality) = match formation.name {
        "4-3-3" => (
            TacticalStyle::HighPressWingOverload,
            PredictedMentality::AggressiveAttacking,
        ),
        "3-5-2" => (
            TacticalStyle::MidfieldDominanceWingBackSurge,
            PredictedMentality::Possession,
        ),
        "4-2-3-1" => (
            TacticalStyle::FluidPlaymakingBoxInfiltration,
            PredictedMentality::DynamicCombination,
        ),
        "5-3-2" | "5-4-1" => (
            TacticalStyle::DeepDefensiveBlockRapidCounters,
            PredictedMentality::DefensiveCounter,
        ),
        _ => (
            TacticalStyle::DirectPlayBalancedStructure,
            PredictedMentality::Balanced,
        ),
    };

    let vulnerabilities = detect_vulnerabilities(opponent_power, &active);

    let mut counter_tactics = vec![match formation.name {
        "3-5-2" => CounterTactic::ExploitWideSpacesBehindWingBacks,
        "4-3-3" => CounterTactic::HoldDeeperDefensiveLineAgainstWingers,
        _ => CounterTactic::PressInFinalThird,
    }];
    if analyst_stars >= 3 {
        counter_tactics.push(CounterTactic::TargetWeakestDefensiveFlank);
    }
    if analyst_stars >= 4 {
        counter_tactics.push(CounterTactic::ShortCombinationPassingThroughPivot);
    }

    let confidence_rating = if analyst_stars <= 0 {
        35
    } else {
        (45 + analyst_stars * 11).min(98)
    };

    let match_outlook = if opponent_power > 82 {
        MatchOutlook::ToughHighTempo
    } else if opponent_power >= 70 {
        MatchOutlook::BalancedMidfieldBattle
    } else {
        MatchOutlook::FavorableFullPoints
    };

    TacticalDossier {
        formation: formation.name.to_string(),
        tactical_style,
        predicted_mentality,
        danger_men,
        vulnerabilities,
        counter_tactics,
        match_outlook,
        confidence_rating,
    }
}

fn detect_vulnerabilities(opponent_power: i32, active: &[&Player]) -> Vec<Vulnerability> {
    let mut vulnerabilities = Vec::new();
    if !active.is_empty() {
        let gk = active
            .iter()
            .copied()
            .find(|p| p.position_id == 1)
            .or_else(|| active.last().copied());
        if let Some(gk) = gk {
            if gk.power < opponent_power - 3 {
                vulnerabilities.push(Vulnerability::WeakGoalkeeper {
                    player_id: gk.player_id,
                    player_name: gk.name.clone(),
                    power: gk.power,
                });
            }
        }

        let mut defenders: Vec<&Player> = active
            .iter()
            .copied()
            .filter(|p| (2..=4).contains(&p.position_id))
            .collect();
        if defenders.len() >= 4 {
            defenders.sort_by_key(|p| p.power);
            let weakest = defenders[0];
            vulnerabilities.push(Vulnerability::WeakDefender {
                player_id: weakest.player_id,
                player_name: weakest.name.clone(),
                power: weakest.power,
            });
        }

        let tired_count = active.iter().filter(|p| p.fitness < 80).count();
        if tired_count >= 3 {
            vulnerabilities.push(Vulnerability::FatiguedSquad { tired_count });
        }
    }
    if vulnerabilities.is_empty() {
        vulnerabilities.push(Vulnerability::SolidDefenseFallback);
    }
    vulnerabilities
}
