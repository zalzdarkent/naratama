use std::collections::HashMap;

use naratama_model::{tournament_ids, Confederation, Money, TeamId, TournamentId, TournamentOrg};
use serde::{Deserialize, Serialize};

use crate::finance_calc::EconomyScale;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CompetitionKind {
    League,
    DomesticCup,
    Continental,
    Intercontinental,
    SuperCup,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompetitionProfile {
    pub kind: CompetitionKind,
    pub tier: i32,
    pub entrants: usize,
    pub country_power: i32,
    pub confederation: Confederation,
    pub slots: i32,
    pub legacy_prize_teams: Money,
}

impl Default for CompetitionProfile {
    fn default() -> Self {
        Self {
            kind: CompetitionKind::League,
            tier: 1,
            entrants: 16,
            country_power: 60,
            confederation: Confederation::None,
            slots: 1,
            legacy_prize_teams: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrizeAward {
    pub team_id: TeamId,
    pub amount: Money,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompetitionPrizeResult {
    pub tournament_id: TournamentId,
    pub kind: CompetitionKind,
    pub prize_pool: Money,
    pub awards: Vec<PrizeAward>,
}

impl CompetitionPrizeResult {
    #[must_use]
    pub fn total_distributed(&self) -> Money {
        self.awards.iter().map(|a| a.amount).sum()
    }
}

#[derive(Debug, Clone)]
pub struct PrizeRule {
    pub base_pool: Money,
    pub participation_per_club: Money,
    pub win_bonus: Money,
    pub draw_bonus: Money,
    pub position_weights: Vec<i64>,
    pub qualification_rounds: HashMap<i32, f64>,
    pub champion_bonus: Money,
    pub runner_up_bonus: Money,
}

#[derive(Debug, Clone, Default)]
pub struct PrizeEngine {
    pub config_overrides: HashMap<TournamentId, PrizeRule>,
}

impl PrizeEngine {
    pub const MIN_PRIZE_POOL: Money = 100_000;

    #[must_use]
    pub fn default_rule(kind: CompetitionKind) -> PrizeRule {
        match kind {
            CompetitionKind::League => PrizeRule {
                base_pool: 15_000_000,
                participation_per_club: 0,
                win_bonus: 0,
                draw_bonus: 0,
                position_weights: vec![
                    40, 30, 24, 20, 17, 15, 13, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 2, 2, 1,
                ],
                qualification_rounds: HashMap::new(),
                champion_bonus: 0,
                runner_up_bonus: 0,
            },
            CompetitionKind::DomesticCup => PrizeRule {
                base_pool: 20_000_000,
                participation_per_club: 150_000,
                win_bonus: 0,
                draw_bonus: 0,
                position_weights: Vec::new(),
                qualification_rounds: HashMap::from([
                    (1, 0.06),
                    (2, 0.09),
                    (3, 0.13),
                    (4, 0.20),
                    (5, 0.30),
                ]),
                champion_bonus: 2_500_000,
                runner_up_bonus: 1_250_000,
            },
            CompetitionKind::Continental => PrizeRule {
                base_pool: 55_000_000,
                participation_per_club: 1_000_000,
                win_bonus: 625_000,
                draw_bonus: 225_000,
                position_weights: Vec::new(),
                qualification_rounds: HashMap::from([
                    (1, 0.08),
                    (2, 0.12),
                    (3, 0.17),
                    (4, 0.24),
                    (5, 0.34),
                ]),
                champion_bonus: 10_000_000,
                runner_up_bonus: 5_000_000,
            },
            CompetitionKind::Intercontinental => PrizeRule {
                base_pool: 100_000_000,
                participation_per_club: 1_500_000,
                win_bonus: 1_000_000,
                draw_bonus: 375_000,
                position_weights: Vec::new(),
                qualification_rounds: HashMap::from([
                    (1, 0.10),
                    (2, 0.16),
                    (3, 0.22),
                    (4, 0.30),
                    (5, 0.40),
                ]),
                champion_bonus: 15_000_000,
                runner_up_bonus: 7_500_000,
            },
            CompetitionKind::SuperCup => PrizeRule {
                base_pool: 7_500_000,
                participation_per_club: 625_000,
                win_bonus: 0,
                draw_bonus: 0,
                position_weights: Vec::new(),
                qualification_rounds: HashMap::new(),
                champion_bonus: 3_750_000,
                runner_up_bonus: 500_000,
            },
        }
    }

    #[must_use]
    pub const fn kind_of(org: TournamentOrg, entrants: usize) -> CompetitionKind {
        match org {
            TournamentOrg::League | TournamentOrg::Friendly => CompetitionKind::League,
            TournamentOrg::NationalCup => CompetitionKind::DomesticCup,
            TournamentOrg::International => {
                if entrants <= 2 {
                    CompetitionKind::SuperCup
                } else if entrants > 32 {
                    CompetitionKind::Intercontinental
                } else {
                    CompetitionKind::Continental
                }
            }
        }
    }

    /// Continental tier override matching the spec (secondary/tertiary cups earn less).
    #[must_use]
    pub const fn continental_tier_override(tournament_id: TournamentId) -> i32 {
        match tournament_id.get() {
            x if x == tournament_ids::EUROPA_LEAGUE.get()
                || x == tournament_ids::SUDAMERICANA.get()
                || x == tournament_ids::ACL_TWO.get() =>
            {
                2
            }
            x if x == tournament_ids::CONFERENCE_LEAGUE.get()
                || x == tournament_ids::AFC_CHALLENGE_LEAGUE.get() =>
            {
                3
            }
            _ => 1,
        }
    }

    #[must_use]
    pub fn rule_for(&self, tournament_id: TournamentId, kind: CompetitionKind) -> PrizeRule {
        self.config_overrides
            .get(&tournament_id)
            .cloned()
            .unwrap_or_else(|| Self::default_rule(kind))
    }

    #[must_use]
    pub fn prize_pool(&self, profile: CompetitionProfile, rule: &PrizeRule) -> Money {
        let entrants_factor = 0.75 + (profile.entrants.max(2) as f64).ln() / 4.0;
        let scale = EconomyScale::national(profile.country_power)
            * EconomyScale::confederation(profile.confederation)
            * EconomyScale::tier(profile.tier)
            * entrants_factor;
        let scaled_base = (rule.base_pool as f64 * scale).round() as i64;
        let pool = if profile.legacy_prize_teams > 0 {
            scaled_base.max(i64::midpoint(scaled_base, profile.legacy_prize_teams))
        } else {
            scaled_base
        };
        pool.max(Self::MIN_PRIZE_POOL)
    }

    #[must_use]
    pub fn league_prizes(
        &self,
        tournament_id: TournamentId,
        profile: CompetitionProfile,
        ranked_team_ids: &[TeamId],
    ) -> CompetitionPrizeResult {
        let rule = self.rule_for(tournament_id, CompetitionKind::League);
        let pool = self.prize_pool(profile, &rule);
        let total_weight: i64 = rule
            .position_weights
            .iter()
            .take(ranked_team_ids.len())
            .sum();
        let safe_weight = total_weight.max(1);

        let awards = ranked_team_ids
            .iter()
            .enumerate()
            .map(|(i, &team_id)| {
                let weight = rule.position_weights.get(i).copied().unwrap_or(1);
                let amount = (pool as f64 * weight as f64 / safe_weight as f64).round() as i64;
                PrizeAward {
                    team_id,
                    amount,
                    reason: format!("League position {}", i + 1),
                }
            })
            .collect();

        CompetitionPrizeResult {
            tournament_id,
            kind: CompetitionKind::League,
            prize_pool: pool,
            awards,
        }
    }

    #[must_use]
    pub fn cup_prizes(&self, input: &CupPrizeInput<'_>) -> CompetitionPrizeResult {
        let rule = self.rule_for(input.tournament_id, input.kind);
        let pool = self.prize_pool(input.profile, &rule);
        let mut awards = Vec::new();

        for &(team_id, round) in input.reached_round_by_team {
            let mut amount = rule.participation_per_club;
            if let Some(&round_weight) = rule.qualification_rounds.get(&round) {
                amount += (pool as f64 * round_weight).round() as i64;
            }
            if Some(team_id) == input.winner_team_id {
                amount += rule.champion_bonus;
            } else if Some(team_id) == input.runner_up_team_id {
                amount += rule.runner_up_bonus;
            }
            if amount > 0 {
                awards.push(PrizeAward {
                    team_id,
                    amount,
                    reason: format!("Reached round {round}"),
                });
            }
        }

        if rule.win_bonus > 0 {
            for &(team_id, wins) in input.wins_by_team {
                let amount = rule.win_bonus * i64::from(wins);
                if amount > 0 {
                    add_to_awards(&mut awards, team_id, amount, format!("{wins} wins"));
                }
            }
        }
        if rule.draw_bonus > 0 {
            for &(team_id, draws) in input.draws_by_team {
                let amount = rule.draw_bonus * i64::from(draws);
                if amount > 0 {
                    add_to_awards(&mut awards, team_id, amount, format!("{draws} draws"));
                }
            }
        }

        CompetitionPrizeResult {
            tournament_id: input.tournament_id,
            kind: input.kind,
            prize_pool: pool,
            awards,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CupPrizeInput<'a> {
    pub tournament_id: TournamentId,
    pub kind: CompetitionKind,
    pub profile: CompetitionProfile,
    pub reached_round_by_team: &'a [(TeamId, i32)],
    pub winner_team_id: Option<TeamId>,
    pub runner_up_team_id: Option<TeamId>,
    pub wins_by_team: &'a [(TeamId, i32)],
    pub draws_by_team: &'a [(TeamId, i32)],
}

fn add_to_awards(awards: &mut Vec<PrizeAward>, team_id: TeamId, amount: Money, reason: String) {
    if let Some(existing) = awards.iter_mut().find(|a| a.team_id == team_id) {
        existing.amount += amount;
        existing.reason = format!("{}; {reason}", existing.reason);
    } else {
        awards.push(PrizeAward {
            team_id,
            amount,
            reason,
        });
    }
}
