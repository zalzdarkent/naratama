use naratama_model::{Confederation, FfpEnforcement, Money};
use serde::{Deserialize, Serialize};

/// Scales prize money and broadcast pools by country strength, confederation wealth, and tier.
pub struct EconomyScale;

impl EconomyScale {
    /// National economy multiplier: `0.35` (power 0) up to `2.10` (power 100).
    #[must_use]
    pub fn national(country_power: i32) -> f64 {
        0.35 + f64::from(country_power.clamp(0, 100)) * 0.0175
    }

    /// Confederation wealth multiplier keyed by `Confederation` (fixes Bug #13 label swap).
    #[must_use]
    pub const fn confederation(conf: Confederation) -> f64 {
        match conf {
            Confederation::Concacaf | Confederation::Afc => 0.85,
            Confederation::Conmebol => 0.95,
            Confederation::Uefa => 1.15,
            Confederation::Caf => 0.90,
            Confederation::Ofc => 0.80,
            Confederation::None => 1.00,
        }
    }

    /// Tier multiplier: every division below the top is worth less.
    #[must_use]
    pub fn tier(division: i32) -> f64 {
        let step = (division - 1).clamp(0, 5);
        1.0 / (1.0 + f64::from(step) * 0.35)
    }
}

/// Finance policy constants shared across matchday, broadcast, transfer, and FFP calculations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FinanceRules {
    pub strict: bool,
}

impl FinanceRules {
    pub const STANDARD: Self = Self { strict: false };
    pub const STRICT: Self = Self { strict: true };

    pub const WEEKS_PER_SEASON: i64 = 52;
    pub const RELAXED_SEASONS: i32 = 2;
    pub const HOME_LEAGUE_MATCHES: i32 = 19;

    pub const HOSPITALITY_RATE: f64 = 0.20;
    pub const MERCH_PER_FAN: Money = 3;
    pub const MATCHDAY_OPS_RATE: f64 = 0.10;
    pub const MIN_ATTENDANCE_SHARE: f64 = 0.25;
    pub const MAX_ATTENDANCE_SHARE: f64 = 1.00;
    pub const MIN_TICKET_PRICE_MULTIPLE: f64 = 0.50;
    pub const MAX_TICKET_PRICE_MULTIPLE: f64 = 2.50;

    pub const BROADCAST_BASE_POOL: Money = 150_000_000;
    pub const BROADCAST_EQUAL_SHARE: f64 = 0.70;
    pub const MIN_BROADCAST_SHARE: Money = 100_000;

    pub const SPONSOR_REVENUE_SCALE: f64 = 1.50;

    pub const MAX_PLAYER_WAGE_SHARE: f64 = 0.25;
    pub const CONTRACT_BONUS_CAP: f64 = 0.20;

    pub const AGENT_FEE_RATE: f64 = 0.05;
    pub const AGENT_FEE_MINIMUM: Money = 50_000;
    pub const SELL_ON_DEFAULT_PCT: f64 = 0.15;
    pub const SELL_ON_MIN_PCT: f64 = 0.05;
    pub const SELL_ON_MAX_PCT: f64 = 0.25;
    pub const RELEASE_CLAUSE_MIN_MULTIPLE: f64 = 1.50;
    pub const RELEASE_CLAUSE_MAX_MULTIPLE: f64 = 2.50;
    pub const MIN_WAGE_RESERVE_WEEKS: i64 = 4;
    pub const FREE_AGENT_SIGN_ON_WEEKS: i64 = 4;

    #[must_use]
    pub const fn for_enforcement(mode: FfpEnforcement) -> Self {
        match mode {
            FfpEnforcement::Strict => Self::STRICT,
            FfpEnforcement::Off | FfpEnforcement::Relaxed | FfpEnforcement::Standard => {
                Self::STANDARD
            }
        }
    }

    #[must_use]
    pub const fn scr_warning(self) -> f64 {
        if self.strict {
            0.70
        } else {
            0.75
        }
    }

    #[must_use]
    pub const fn scr_breach(self) -> f64 {
        if self.strict {
            0.75
        } else {
            0.85
        }
    }

    #[must_use]
    pub const fn scr_hard_block(self) -> f64 {
        if self.strict {
            0.90
        } else {
            1.00
        }
    }

    #[must_use]
    pub const fn psr_warning_fraction(self) -> f64 {
        0.80
    }

    #[must_use]
    pub const fn psr_min_allowed_loss(self) -> Money {
        if self.strict {
            1_000_000
        } else {
            2_000_000
        }
    }

    #[must_use]
    pub const fn psr_revenue_fraction(self) -> f64 {
        if self.strict {
            0.05
        } else {
            0.10
        }
    }

    #[must_use]
    pub fn psr_allowed_loss(self, football_revenue: Money) -> Money {
        let revenue_based = (football_revenue as f64 * self.psr_revenue_fraction()).round() as i64;
        revenue_based.max(self.psr_min_allowed_loss())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchdayBreakdown {
    pub gross_tickets: Money,
    pub hospitality: Money,
    pub merchandise: Money,
    pub operating_cost: Money,
    pub net: Money,
}

impl MatchdayBreakdown {
    #[must_use]
    pub const fn total_income(self) -> Money {
        self.gross_tickets + self.hospitality + self.merchandise
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BroadcastDistribution {
    pub pool: Money,
    pub equal_share_total: Money,
    pub merit_share_total: Money,
    pub shares: Vec<Money>,
}

impl BroadcastDistribution {
    #[must_use]
    pub fn total_distributed(&self) -> Money {
        self.shares.iter().sum()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FfpLevel {
    Compliant,
    Warning,
    Breach,
    HardBlock,
}

impl FfpLevel {
    #[must_use]
    pub const fn is_at_least_breach(self) -> bool {
        matches!(self, Self::Breach | Self::HardBlock)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum FfpReason {
    WithinRules,
    HighSquadCostRatio { ratio: f64, limit: f64 },
    PsrLossNearLimit,
    PsrLossExceeded { allowed_loss: Money },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FfpAssessment {
    pub squad_cost_ratio: f64,
    pub squad_cost: Money,
    pub football_revenue: Money,
    pub psr_result: Money,
    pub allowed_loss: Money,
    pub level: FfpLevel,
    pub transfer_blocked: bool,
    pub reasons: Vec<FfpReason>,
}

impl FfpAssessment {
    #[must_use]
    pub const fn is_compliant(&self) -> bool {
        matches!(self.level, FfpLevel::Compliant)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FfpAssessmentInput {
    pub annual_player_wages: Money,
    pub annual_staff_wages: Money,
    pub annual_amortization: Money,
    pub agent_fees: Money,
    pub football_revenue: Money,
    pub psr_result: Money,
    pub psr_revenue: Money,
    pub enforcement: FfpEnforcement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PsrInput {
    pub ledger_net: Money,
    pub board_funding: Money,
    pub facility_capex: Money,
    pub transfer_fees_paid: Money,
    pub amortization: Money,
    pub book_value_of_sales: Money,
    pub book_value_write_off: Money,
}

pub struct FinanceCalc;

impl FinanceCalc {
    #[must_use]
    pub fn attractiveness(
        home_power: i32,
        away_power: i32,
        form: f64,
        competition_prestige: f64,
    ) -> f64 {
        let raw = 0.30
            + 0.006 * f64::from(home_power.clamp(0, 100))
            + 0.20 * form.clamp(0.0, 1.0)
            + 0.10 * competition_prestige.clamp(0.0, 1.0)
            + 0.05 * (f64::from(away_power.clamp(0, 100)) / 100.0);
        raw.clamp(0.20, 1.00)
    }

    #[must_use]
    pub fn price_demand(price: i32, reference_price: i32) -> f64 {
        if reference_price <= 0 {
            return 1.0;
        }
        let ratio = f64::from(price) / f64::from(reference_price);
        (1.35 - 0.35 * ratio).clamp(0.35, 1.15)
    }

    #[must_use]
    pub fn attendance_for_tier(
        capacity: i32,
        home_power: i32,
        away_power: i32,
        price: i32,
        reference_price: i32,
        form: f64,
        competition_prestige: f64,
    ) -> i32 {
        if capacity <= 0 {
            return 0;
        }
        let appeal = Self::attractiveness(home_power, away_power, form, competition_prestige);
        let demand = (appeal * Self::price_demand(price, reference_price)).clamp(
            FinanceRules::MIN_ATTENDANCE_SHARE,
            FinanceRules::MAX_ATTENDANCE_SHARE,
        );
        (f64::from(capacity) * demand).round() as i32
    }

    #[must_use]
    pub fn box_attendance(
        capacity: i32,
        home_power: i32,
        away_power: i32,
        form: f64,
        competition_prestige: f64,
    ) -> i32 {
        if capacity <= 0 {
            return 0;
        }
        let appeal = Self::attractiveness(home_power, away_power, form, competition_prestige);
        let demand = (0.55 + 0.45 * appeal).clamp(0.55, 1.00);
        (f64::from(capacity) * demand).round() as i32
    }

    #[must_use]
    pub fn reference_price(base_price: i32, division: i32) -> i32 {
        if base_price <= 0 {
            return 0;
        }
        let price = (f64::from(base_price) * EconomyScale::tier(division)).round() as i32;
        price.max(1)
    }

    #[must_use]
    pub fn is_ticket_price_allowed(price: i32, reference_price: i32) -> bool {
        if price < 0 || reference_price <= 0 {
            return false;
        }
        let ratio = f64::from(price) / f64::from(reference_price);
        (FinanceRules::MIN_TICKET_PRICE_MULTIPLE..=FinanceRules::MAX_TICKET_PRICE_MULTIPLE)
            .contains(&ratio)
    }

    #[must_use]
    pub fn matchday(gross_tickets: Money, total_attendance: i32) -> MatchdayBreakdown {
        let safe_gross = gross_tickets.max(0);
        let safe_att = i64::from(total_attendance.max(0));
        let hospitality = (safe_gross as f64 * FinanceRules::HOSPITALITY_RATE).round() as i64;
        let merchandise = safe_att * FinanceRules::MERCH_PER_FAN;
        let operating_cost = (safe_gross as f64 * FinanceRules::MATCHDAY_OPS_RATE).round() as i64;
        let net = safe_gross + hospitality + merchandise - operating_cost;
        MatchdayBreakdown {
            gross_tickets: safe_gross,
            hospitality,
            merchandise,
            operating_cost,
            net,
        }
    }

    #[must_use]
    pub fn broadcast_pool(
        country_power: i32,
        confederation: Confederation,
        division: i32,
        entrants: usize,
    ) -> Money {
        let safe_entrants = entrants.max(2) as f64;
        let entrants_factor = 0.75 + safe_entrants.ln() / 4.0;
        let scale = EconomyScale::national(country_power)
            * EconomyScale::confederation(confederation)
            * EconomyScale::tier(division)
            * entrants_factor;
        let pool = (FinanceRules::BROADCAST_BASE_POOL as f64 * scale).round() as i64;
        pool.max(FinanceRules::MIN_BROADCAST_SHARE)
    }

    /// Splits a domestic broadcast pool 70% equally / 30% by merit.
    /// Returned shares always sum exactly to `pool.max(0)`.
    #[must_use]
    pub fn broadcast_distribution(pool: Money, club_count: usize) -> BroadcastDistribution {
        if club_count == 0 {
            return BroadcastDistribution {
                pool,
                equal_share_total: 0,
                merit_share_total: 0,
                shares: Vec::new(),
            };
        }
        let safe_pool = pool.max(0);
        let equal_total = (safe_pool as f64 * FinanceRules::BROADCAST_EQUAL_SHARE).round() as i64;
        let merit_total = safe_pool - equal_total;

        let equal_shares = split_evenly(equal_total, club_count);
        let weights: Vec<i64> = (0..club_count).map(|i| (club_count - i) as i64).collect();
        let weight_sum: i64 = weights.iter().sum();

        let mut merit_shares = Vec::with_capacity(club_count);
        let mut assigned = 0_i64;
        for &w in &weights {
            let amount = if weight_sum == 0 {
                0
            } else {
                (merit_total as f64 * w as f64 / weight_sum as f64).round() as i64
            };
            merit_shares.push(amount);
            assigned += amount;
        }
        fix_rounding(&mut merit_shares, merit_total - assigned);

        let shares = equal_shares
            .into_iter()
            .zip(merit_shares)
            .map(|(eq, merit)| eq + merit)
            .collect();

        BroadcastDistribution {
            pool: safe_pool,
            equal_share_total: equal_total,
            merit_share_total: merit_total,
            shares,
        }
    }

    #[must_use]
    pub fn annual_amortization(fee: Money, contract_years: i32) -> Money {
        if fee <= 0 {
            return 0;
        }
        (fee as f64 / f64::from(contract_years.max(1))).round() as i64
    }

    #[must_use]
    pub fn book_value(fee: Money, contract_years: i32, elapsed_days: i64) -> Money {
        if fee <= 0 {
            return 0;
        }
        let annual = Self::annual_amortization(fee, contract_years);
        let years = elapsed_days.max(0) as f64 / 365.0;
        let remaining = fee - (annual as f64 * years).round() as i64;
        remaining.clamp(0, fee)
    }

    #[must_use]
    pub const fn sale_profit(sale_fee: Money, book_value: Money) -> Money {
        sale_fee - book_value
    }

    #[must_use]
    pub fn sell_on_amount(next_fee: Money, original_fee: Money, pct: f64) -> Money {
        let profit = (next_fee - original_fee).max(0);
        let safe_pct = pct.clamp(FinanceRules::SELL_ON_MIN_PCT, FinanceRules::SELL_ON_MAX_PCT);
        (profit as f64 * safe_pct).round() as i64
    }

    #[must_use]
    pub fn agent_fee(fee: Money) -> Money {
        if fee <= 0 {
            return 0;
        }
        let fee_based = (fee as f64 * FinanceRules::AGENT_FEE_RATE).round() as i64;
        fee_based.max(FinanceRules::AGENT_FEE_MINIMUM)
    }

    #[must_use]
    pub fn sign_on_bonus(weekly_wage: Money) -> Money {
        weekly_wage.max(0) * FinanceRules::FREE_AGENT_SIGN_ON_WEEKS
    }

    /// Weekly wage benchmark from overall rating, age, and club reputation (`0.85..=1.20`).
    #[must_use]
    pub fn market_wage(power: i32, age: i32, club_reputation: f64) -> Money {
        const ANCHORS: [(i32, [i64; 4]); 7] = [
            (50, [1_500, 2_500, 2_500, 2_000]),
            (60, [4_000, 7_000, 7_000, 5_000]),
            (70, [10_000, 18_000, 18_000, 12_000]),
            (80, [30_000, 55_000, 55_000, 35_000]),
            (85, [55_000, 95_000, 95_000, 60_000]),
            (90, [80_000, 140_000, 140_000, 85_000]),
            (95, [110_000, 200_000, 200_000, 110_000]),
        ];

        let band = if age <= 21 {
            0
        } else if age <= 27 {
            1
        } else if age <= 31 {
            2
        } else {
            3
        };

        let safe_power = power.clamp(40, 99);
        let wage = if safe_power <= ANCHORS[0].0 {
            ANCHORS[0].1[band] as f64
        } else if safe_power >= ANCHORS[ANCHORS.len() - 1].0 {
            ANCHORS[ANCHORS.len() - 1].1[band] as f64
        } else {
            let mut interpolated = ANCHORS[ANCHORS.len() - 1].1[band] as f64;
            for pair in ANCHORS.windows(2) {
                let (p0, w0) = pair[0];
                let (p1, w1) = pair[1];
                if safe_power >= p0 && safe_power <= p1 {
                    let t = f64::from(safe_power - p0) / f64::from(p1 - p0);
                    interpolated = w0[band] as f64 + t * (w1[band] - w0[band]) as f64;
                    break;
                }
            }
            interpolated
        };

        let adjusted = (wage * club_reputation.clamp(0.85, 1.20)).round() as i64;
        ((adjusted as f64 / 500.0).round() as i64) * 500
    }

    #[must_use]
    pub fn max_contract_bonus(annual_wage: Money) -> Money {
        (annual_wage as f64 * FinanceRules::CONTRACT_BONUS_CAP).round() as i64
    }

    #[must_use]
    pub fn minimum_cash_reserve(weekly_payroll: Money) -> Money {
        weekly_payroll.max(0) * FinanceRules::MIN_WAGE_RESERVE_WEEKS
    }

    #[must_use]
    pub fn transfer_budget_available(budget: Money, weekly_payroll: Money) -> Money {
        (budget - Self::minimum_cash_reserve(weekly_payroll)).max(0)
    }

    #[must_use]
    pub fn squad_cost(
        annual_player_wages: Money,
        annual_staff_wages: Money,
        annual_amortization: Money,
        agent_fees: Money,
    ) -> Money {
        annual_player_wages.max(0)
            + annual_staff_wages.max(0)
            + annual_amortization.max(0)
            + agent_fees.max(0)
    }

    #[must_use]
    pub fn squad_cost_ratio(squad_cost: Money, football_revenue: Money) -> f64 {
        if football_revenue <= 0 {
            return if squad_cost <= 0 { 0.0 } else { 99.0 };
        }
        squad_cost as f64 / football_revenue as f64
    }

    #[must_use]
    pub const fn psr_result(input: PsrInput) -> Money {
        input.ledger_net - input.board_funding + input.facility_capex + input.transfer_fees_paid
            - input.amortization
            - input.book_value_of_sales
            - input.book_value_write_off
    }

    /// Evaluates SCR + PSR telemetry and enforcement. When `enforcement == Off | Relaxed`,
    /// telemetry is still computed accurately, while `transfer_blocked` stays `false` (ADR-004).
    #[must_use]
    pub fn assess_ffp(input: FfpAssessmentInput) -> FfpAssessment {
        let rules = FinanceRules::for_enforcement(input.enforcement);
        let cost = Self::squad_cost(
            input.annual_player_wages,
            input.annual_staff_wages,
            input.annual_amortization,
            input.agent_fees,
        );
        let scr = Self::squad_cost_ratio(cost, input.football_revenue);
        let allowed_loss = rules.psr_allowed_loss(input.psr_revenue);
        let psr_breach = input.psr_result < -allowed_loss;
        let psr_warning = !psr_breach
            && (input.psr_result as f64) < -(allowed_loss as f64 * rules.psr_warning_fraction());

        let level = if scr > rules.scr_hard_block() {
            FfpLevel::HardBlock
        } else if scr > rules.scr_breach() || psr_breach {
            FfpLevel::Breach
        } else if scr > rules.scr_warning() || psr_warning {
            FfpLevel::Warning
        } else {
            FfpLevel::Compliant
        };

        let mut reasons = Vec::new();
        if scr > rules.scr_warning() {
            reasons.push(FfpReason::HighSquadCostRatio {
                ratio: scr,
                limit: rules.scr_breach(),
            });
        }
        if psr_warning {
            reasons.push(FfpReason::PsrLossNearLimit);
        }
        if psr_breach {
            reasons.push(FfpReason::PsrLossExceeded { allowed_loss });
        }
        if reasons.is_empty() {
            reasons.push(FfpReason::WithinRules);
        }

        let transfer_blocked = matches!(
            input.enforcement,
            FfpEnforcement::Standard | FfpEnforcement::Strict
        ) && (level == FfpLevel::HardBlock || psr_breach);

        FfpAssessment {
            squad_cost_ratio: scr,
            squad_cost: cost,
            football_revenue: input.football_revenue,
            psr_result: input.psr_result,
            allowed_loss,
            level,
            transfer_blocked,
            reasons,
        }
    }
}

fn split_evenly(total: Money, parts: usize) -> Vec<Money> {
    if parts == 0 {
        return Vec::new();
    }
    let parts_i64 = parts as i64;
    let base = total / parts_i64;
    let remainder = (total - base * parts_i64) as usize;
    (0..parts)
        .map(|i| base + i64::from(i < remainder))
        .collect()
}

fn fix_rounding(values: &mut [Money], delta: Money) {
    if values.is_empty() || delta == 0 {
        return;
    }
    let mut order: Vec<usize> = (0..values.len()).collect();
    order.sort_by(|&a, &b| values[b].cmp(&values[a]));
    let mut remaining = delta;
    let mut cursor = 0usize;
    let limit = order.len() * 4;
    while remaining != 0 && cursor < limit {
        let i = order[cursor % order.len()];
        let step = if remaining > 0 { 1 } else { -1 };
        if values[i] + step >= 0 {
            values[i] += step;
            remaining -= step;
        }
        cursor += 1;
    }
}
