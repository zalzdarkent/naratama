use naratama_model::{Confederation, Money};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PlayerValuationConfig {
    pub min_valuation: Money,
    pub max_valuation: Money,
    pub routine_max_growth_rate: f64,
}

impl Default for PlayerValuationConfig {
    fn default() -> Self {
        Self {
            min_valuation: 15_000,
            max_valuation: 250_000_000,
            routine_max_growth_rate: 0.15,
        }
    }
}

impl PlayerValuationConfig {
    /// 19 OVR anchor points for Hermite cubic smoothstep interpolation.
    pub const OVR_ANCHORS: [(i32, i64); 19] = [
        (40, 20_000),
        (45, 50_000),
        (50, 100_000),
        (55, 250_000),
        (60, 500_000),
        (65, 1_200_000),
        (70, 3_200_000),
        (73, 5_500_000),
        (76, 10_000_000),
        (79, 18_000_000),
        (82, 30_000_000),
        (84, 42_000_000),
        (86, 60_000_000),
        (88, 85_000_000),
        (90, 115_000_000),
        (92, 145_000_000),
        (94, 180_000_000),
        (96, 210_000_000),
        (99, 240_000_000),
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PlayerValuationInput {
    pub power: i32,
    pub potential: Option<i32>,
    pub age: i32,
    pub position_id: i32,
    pub confederation: Confederation,
    pub team_power: i32,
    pub appearances: i32,
    pub avg_rating: Option<f64>,
    pub goals: i32,
    pub assists: i32,
    pub clean_sheets: i32,
    pub contract_months_remaining: i32,
}

impl Default for PlayerValuationInput {
    fn default() -> Self {
        Self {
            power: 70,
            potential: None,
            age: 24,
            position_id: 7,
            confederation: Confederation::Uefa,
            team_power: 75,
            appearances: 0,
            avg_rating: None,
            goals: 0,
            assists: 0,
            clean_sheets: 0,
            contract_months_remaining: 24,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PlayerValuationService {
    pub config: PlayerValuationConfig,
}

impl PlayerValuationService {
    #[must_use]
    pub const fn new(config: PlayerValuationConfig) -> Self {
        Self { config }
    }

    #[must_use]
    pub fn calculate_base_value(&self, ovr: i32) -> f64 {
        let clamped = ovr.clamp(40, 99);
        for pair in PlayerValuationConfig::OVR_ANCHORS.windows(2) {
            let (x0, y0) = pair[0];
            let (x1, y1) = pair[1];
            if clamped >= x0 && clamped <= x1 {
                let t = f64::from(clamped - x0) / f64::from(x1 - x0);
                let smooth_t = t * t * (3.0 - 2.0 * t);
                return y0 as f64 + smooth_t * (y1 - y0) as f64;
            }
        }
        PlayerValuationConfig::OVR_ANCHORS[PlayerValuationConfig::OVR_ANCHORS.len() - 1].1 as f64
    }

    #[must_use]
    pub const fn calculate_age_multiplier(&self, age: i32) -> f64 {
        if age <= 17 {
            0.85
        } else if age <= 20 {
            1.15
        } else if age <= 24 {
            1.25
        } else if age <= 28 {
            1.05
        } else if age <= 30 {
            0.90
        } else if age <= 32 {
            0.72
        } else if age <= 34 {
            0.50
        } else if age <= 36 {
            0.35
        } else {
            0.25
        }
    }

    #[must_use]
    pub fn calculate_potential_multiplier(&self, age: i32, power: i32, potential: i32) -> f64 {
        if age >= 25 || potential <= power {
            return 1.0;
        }
        let gap = f64::from(potential - power);
        let age_weight = (f64::from(25 - age) / 9.0).max(0.0);
        let premium = (gap * 0.025 * age_weight).min(0.35);
        1.0 + premium
    }

    #[must_use]
    pub const fn calculate_position_multiplier(&self, position_id: i32) -> f64 {
        match position_id {
            1 => 0.88,
            2 | 3 => 0.93,
            4 => 0.94,
            5 => 0.96,
            6 => 1.03,
            9 => 1.05,
            10 => 1.08,
            11 => 1.06,
            _ => 1.00,
        }
    }

    #[must_use]
    pub fn calculate_reputation_multiplier(
        &self,
        confederation: Confederation,
        team_power: i32,
    ) -> f64 {
        let conf_mult = match confederation {
            Confederation::Uefa => 1.10,
            Confederation::Concacaf | Confederation::Conmebol => 1.02,
            _ => 0.88,
        };
        let team_mult = 0.85 + f64::from(team_power.clamp(50, 90) - 50) / 40.0 * 0.30;
        (conf_mult * team_mult).clamp(0.75, 1.25)
    }

    #[must_use]
    pub fn calculate_performance_multiplier(
        &self,
        appearances: i32,
        avg_rating: Option<f64>,
        goals: i32,
        assists: i32,
        clean_sheets: i32,
        position_id: i32,
    ) -> f64 {
        let Some(rating) = avg_rating else {
            return 1.0;
        };
        if appearances < 5 || rating <= 0.0 {
            return 1.0;
        }
        let rating_delta = (rating - 6.8).clamp(-1.0, 1.2);
        let mut modifier = 1.0 + rating_delta * 0.08;

        if (6..=11).contains(&position_id) {
            let ga_per_app = f64::from(goals + assists) / f64::from(appearances);
            if ga_per_app >= 0.5 {
                modifier += 0.04;
            }
            if ga_per_app >= 0.8 {
                modifier += 0.03;
            }
        } else if position_id <= 3 {
            let cs_ratio = f64::from(clean_sheets) / f64::from(appearances);
            if cs_ratio >= 0.40 {
                modifier += 0.04;
            }
        }
        modifier.clamp(0.85, 1.15)
    }

    #[must_use]
    pub const fn calculate_contract_multiplier(&self, months_remaining: i32) -> f64 {
        if months_remaining <= 6 {
            0.90
        } else if months_remaining <= 12 {
            0.95
        } else if months_remaining <= 48 {
            1.00
        } else {
            1.03
        }
    }

    #[must_use]
    pub fn calculate_target_market_value(&self, input: PlayerValuationInput) -> Money {
        let eff_potential = input.potential.unwrap_or(input.power);
        let raw = self.calculate_base_value(input.power)
            * self.calculate_age_multiplier(input.age)
            * self.calculate_potential_multiplier(input.age, input.power, eff_potential)
            * self.calculate_position_multiplier(input.position_id)
            * self.calculate_reputation_multiplier(input.confederation, input.team_power)
            * self.calculate_performance_multiplier(
                input.appearances,
                input.avg_rating,
                input.goals,
                input.assists,
                input.clean_sheets,
                input.position_id,
            )
            * self.calculate_contract_multiplier(input.contract_months_remaining);

        let clamped =
            (raw.round() as i64).clamp(self.config.min_valuation, self.config.max_valuation);
        Self::round_valuation_cleanly(clamped)
    }

    #[must_use]
    pub fn smooth_value_update(
        &self,
        current_value: Money,
        target_value: Money,
        max_percentage_step: Option<f64>,
    ) -> Money {
        if current_value <= 0 {
            return target_value;
        }
        let max_step = max_percentage_step.unwrap_or(self.config.routine_max_growth_rate);
        let diff = target_value - current_value;
        let max_allowed = (current_value as f64 * max_step).round() as i64;

        if diff.abs() <= max_allowed {
            return target_value;
        }
        if diff > 0 {
            Self::round_valuation_cleanly(current_value + max_allowed)
        } else {
            Self::round_valuation_cleanly(
                (current_value - max_allowed).max(self.config.min_valuation),
            )
        }
    }

    #[must_use]
    pub fn calculate_asking_price(
        &self,
        market_value: Money,
        contract_months_remaining: i32,
        is_transfer_listed: bool,
        is_untouchable: bool,
        club_financial_multiplier: f64,
    ) -> Money {
        let mut markup = 1.0;
        if contract_months_remaining <= 6 {
            markup *= 0.65;
        } else if contract_months_remaining <= 12 {
            markup *= 0.85;
        } else if contract_months_remaining >= 36 {
            markup *= 1.20;
        }

        if is_transfer_listed {
            markup *= 0.85;
        } else if is_untouchable {
            markup *= 1.35;
        }

        markup *= club_financial_multiplier.clamp(0.80, 1.25);
        let asking = (market_value as f64 * markup.clamp(0.55, 1.80)).round() as i64;
        Self::round_valuation_cleanly(asking)
    }

    #[must_use]
    pub fn calculate_negotiated_fee(
        &self,
        market_value: Money,
        asking_price: Money,
        initial_offer: Money,
        buyer_leverage: f64,
    ) -> Money {
        let lev = buyer_leverage.clamp(0.0, 1.0);
        let compromise = initial_offer as f64 * (1.0 - lev) + asking_price as f64 * lev;
        let lo = (market_value as f64 * 0.70).round() as i64;
        let hi = (market_value as f64 * 1.50).round() as i64;
        Self::round_valuation_cleanly((compromise.round() as i64).clamp(lo, hi))
    }

    #[must_use]
    pub fn calculate_loan_fee(
        &self,
        market_value: Money,
        loan_seasons: i32,
        loan_rate: f64,
    ) -> Money {
        let fee = (market_value as f64 * loan_rate * f64::from(loan_seasons)).round() as i64;
        Self::round_valuation_cleanly(fee.max(10_000))
    }

    #[must_use]
    pub fn round_valuation_cleanly(val: Money) -> Money {
        let step = if val >= 10_000_000 {
            500_000.0
        } else if val >= 1_000_000 {
            100_000.0
        } else if val >= 100_000 {
            10_000.0
        } else {
            5_000.0
        };
        ((val as f64 / step).round() as i64) * (step as i64)
    }
}
