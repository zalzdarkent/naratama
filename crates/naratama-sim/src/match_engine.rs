use std::collections::{HashMap, HashSet};

use naratama_model::{Money, NaratamaError, Player, PlayerId, PositionGroup, Stadium, TeamId};
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::finance_calc::FinanceCalc;
use crate::formation::{Formation, Lineup};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum MatchSide {
    Home,
    Away,
}

impl MatchSide {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Home => "home",
            Self::Away => "away",
        }
    }

    #[must_use]
    pub const fn opposite(self) -> Self {
        match self {
            Self::Home => Self::Away,
            Self::Away => Self::Home,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MatchEventType {
    Goal,
    Assist,
    Save,
    YellowCard,
    RedCard,
    PenaltyScored,
    PenaltyMissed,
    OwnGoal,
    Injury,
    SubstitutionIn,
    SubstitutionOut,
    VarCheck,
    VarGoalDisallowed,
    ShootoutKick,
}

impl MatchEventType {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Goal => "goal",
            Self::Assist => "assist",
            Self::Save => "save",
            Self::YellowCard => "yellow_card",
            Self::RedCard => "red_card",
            Self::PenaltyScored => "penalty_scored",
            Self::PenaltyMissed => "penalty_missed",
            Self::OwnGoal => "own_goal",
            Self::Injury => "injury",
            Self::SubstitutionIn => "substitution_in",
            Self::SubstitutionOut => "substitution_out",
            Self::VarCheck => "var_check",
            Self::VarGoalDisallowed => "var_goal_disallowed",
            Self::ShootoutKick => "shootout_kick",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchEvent {
    pub minute: i32,
    pub event_type: MatchEventType,
    pub side: MatchSide,
    pub player_id: Option<PlayerId>,
    pub player_name: Option<String>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchPlayerRow {
    pub player_id: PlayerId,
    pub player_name: String,
    pub team_id: TeamId,
    pub starter: bool,
    pub minutes: i32,
    pub goals: i32,
    pub assists: i32,
    pub yellow_cards: i32,
    pub red_cards: i32,
    pub mvp: bool,
    /// Rating stored x10 (`20..=100` representing `2.0..=10.0`).
    pub rating: i32,
    pub points: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchInput {
    pub home_squad: Vec<Player>,
    pub away_squad: Vec<Player>,
    pub home_team_id: TeamId,
    pub away_team_id: TeamId,
    pub capacity_general: i32,
    pub capacity_stand: i32,
    pub capacity_terrace: i32,
    pub capacity_boxes: i32,
    pub ticket_price_general: i32,
    pub ticket_price_stand: i32,
    pub ticket_price_terrace: i32,
    pub ticket_price_boxes: i32,
    pub reference_price_general: i32,
    pub reference_price_stand: i32,
    pub reference_price_terrace: i32,
    pub reference_price_boxes: i32,
    pub home_form: f64,
    pub competition_prestige: f64,
    pub home_power: i32,
    pub away_power: i32,
    pub home_formation: Formation,
    pub away_formation: Formation,
    pub home_lineup_ids: Vec<PlayerId>,
    pub away_lineup_ids: Vec<PlayerId>,
    pub referee_temper: i32,
    pub home_injury_reduction: f64,
    pub away_injury_reduction: f64,
    pub home_tactical_bonus: f64,
    pub away_tactical_bonus: f64,
    /// ADR-004: removes `+2.0/-1.2` home edge and `+1.5` possession bias for finals/WC.
    pub neutral_venue: bool,
    /// ADR-004: simulates minutes `91..=120` when level after 90'.
    pub extra_time: bool,
}

impl Default for MatchInput {
    fn default() -> Self {
        Self {
            home_squad: Vec::new(),
            away_squad: Vec::new(),
            home_team_id: TeamId(0),
            away_team_id: TeamId(0),
            capacity_general: 15_000,
            capacity_stand: 10_000,
            capacity_terrace: 5_000,
            capacity_boxes: 500,
            ticket_price_general: Stadium::DEFAULT_GENERAL_PRICE,
            ticket_price_stand: Stadium::DEFAULT_STAND_PRICE,
            ticket_price_terrace: Stadium::DEFAULT_TERRACE_PRICE,
            ticket_price_boxes: Stadium::DEFAULT_BOXES_PRICE,
            reference_price_general: Stadium::DEFAULT_GENERAL_PRICE,
            reference_price_stand: Stadium::DEFAULT_STAND_PRICE,
            reference_price_terrace: Stadium::DEFAULT_TERRACE_PRICE,
            reference_price_boxes: Stadium::DEFAULT_BOXES_PRICE,
            home_form: 0.5,
            competition_prestige: 0.5,
            home_power: 70,
            away_power: 70,
            home_formation: Formation::F442,
            away_formation: Formation::F442,
            home_lineup_ids: Vec::new(),
            away_lineup_ids: Vec::new(),
            referee_temper: 2,
            home_injury_reduction: 0.0,
            away_injury_reduction: 0.0,
            home_tactical_bonus: 0.0,
            away_tactical_bonus: 0.0,
            neutral_venue: false,
            extra_time: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchResult {
    pub home_goals: i32,
    pub away_goals: i32,
    pub went_to_extra_time: bool,
    pub events: Vec<MatchEvent>,
    pub player_rows: Vec<MatchPlayerRow>,
    pub attendance_general: i32,
    pub attendance_stand: i32,
    pub attendance_terrace: i32,
    pub attendance_boxes: i32,
    /// Gross ticket revenue before hospitality/merch/ops.
    pub revenue: Money,
    pub home_shots: i32,
    pub away_shots: i32,
    pub home_shots_on_target: i32,
    pub away_shots_on_target: i32,
    pub home_saves: i32,
    pub away_saves: i32,
    pub home_possession: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchPenaltyShootout {
    pub home_score: i32,
    pub away_score: i32,
    pub events: Vec<MatchEvent>,
}

#[derive(Debug, Clone)]
struct OnPitch {
    player: Player,
    entered_at: i32,
}

/// Pure, deterministic football match simulation engine (zero DB/UI coupling).
pub struct MatchEngine;

impl MatchEngine {
    const BASE_CHANCE_PER_MINUTE: f64 = 0.112;
    const CONVERSION_BASE: f64 = 0.132;
    const ON_TARGET_SHARE: f64 = 0.35;
    const PENALTY_POOL: f64 = 0.060;
    const PENALTY_CONVERT: f64 = 0.76;
    const OWN_GOAL_POOL: f64 = 0.020;
    const YELLOW_POOL: f64 = 3.6;
    const RED_POOL: f64 = 0.12;
    const INJURY_POOL: f64 = 0.40;
    const ATTACKER_INJURY_BIAS: f64 = 0.30;

    #[must_use]
    pub fn temps_factor(temper: i32) -> f64 {
        0.6 + 0.2 * f64::from(temper)
    }

    /// Ultra-fast Poisson score simulator for background AI-vs-AI fixtures.
    #[must_use]
    pub fn quick_simulate(
        rng: &mut impl Rng,
        home_power: i32,
        away_power: i32,
        neutral_venue: bool,
    ) -> (i32, i32) {
        let home_bonus = if neutral_venue { 0.0 } else { 1.8 };
        let diff = (f64::from(home_power) + home_bonus) - f64::from(away_power);
        let home_exp = if diff >= 0.0 {
            (1.35 * (diff * 0.038).exp()).clamp(0.12, 4.60)
        } else {
            (1.35 * (diff * 0.055).exp()).clamp(0.08, 4.60)
        };
        let away_exp = if diff <= 0.0 {
            (1.18 * (-diff * 0.038).exp()).clamp(0.10, 4.40)
        } else {
            (1.18 * (-diff * 0.055).exp()).clamp(0.06, 4.40)
        };
        (sample_poisson(rng, home_exp), sample_poisson(rng, away_exp))
    }

    /// Simulates a full match (90 minutes, plus optional 30-minute extra time).
    #[must_use]
    pub fn simulate(
        rng: &mut impl Rng,
        input: &MatchInput,
        record_events: bool,
        banned_ids: &HashSet<PlayerId>,
    ) -> MatchResult {
        let home = Lineup::best(
            &input.home_squad,
            input.home_formation,
            Lineup::DEFAULT_FITNESS_FLOOR,
            banned_ids,
            &input.home_lineup_ids,
        );
        let away = Lineup::best(
            &input.away_squad,
            input.away_formation,
            Lineup::DEFAULT_FITNESS_FLOOR,
            banned_ids,
            &input.away_lineup_ids,
        );
        Self::simulate_lineups(rng, input, &home, &away, record_events)
    }

    fn simulate_lineups<R: Rng>(
        rng: &mut R,
        input: &MatchInput,
        home: &Lineup,
        away: &Lineup,
        record_events: bool,
    ) -> MatchResult {
        LiveMatch::from_lineups(input, home, away, record_events).run_full(rng)
    }

    /// Rebuilds the scoring parameters from the players currently available to each side.
    fn compute_bases(
        input: &MatchInput,
        home_starters: &[Player],
        away_starters: &[Player],
        adjustment: Option<(MatchSide, MatchMentality)>,
    ) -> MatchBases {
        let raw_home_att = Lineup::attack_on(home_starters) / 9.4;
        let raw_away_att = Lineup::attack_on(away_starters) / 9.4;
        let raw_home_mid = Lineup::midfield_on(home_starters) / 8.0;
        let raw_away_mid = Lineup::midfield_on(away_starters) / 8.0;
        let raw_home_def = Lineup::defense_on(home_starters) / 10.4;
        let raw_away_def = Lineup::defense_on(away_starters) / 10.4;

        let home_squad_ovr = (raw_home_att + raw_home_mid + raw_home_def) / 3.0;
        let away_squad_ovr = (raw_away_att + raw_away_mid + raw_away_def) / 3.0;

        let blend = |squad_val: f64, team_pwr: i32, w_squad: f64, w_team: f64| -> f64 {
            if team_pwr > 0 {
                squad_val * w_squad + f64::from(team_pwr) * w_team
            } else {
                squad_val
            }
        };

        let home_ovr = blend(home_squad_ovr, input.home_power, 0.65, 0.35);
        let away_ovr = blend(away_squad_ovr, input.away_power, 0.65, 0.35);
        let home_att = blend(raw_home_att, input.home_power, 0.75, 0.25);
        let away_att = blend(raw_away_att, input.away_power, 0.75, 0.25);
        let home_mid = blend(raw_home_mid, input.home_power, 0.75, 0.25);
        let away_mid = blend(raw_away_mid, input.away_power, 0.75, 0.25);
        let home_def = blend(raw_home_def, input.home_power, 0.75, 0.25);
        let away_def = blend(raw_away_def, input.away_power, 0.75, 0.25);

        let ovr_gap = home_ovr - away_ovr;
        let (home_adv, away_adv) = if input.neutral_venue {
            (0.0, 0.0)
        } else {
            (2.0, -1.2)
        };
        let home_edge =
            (home_att - away_def) * 0.45 + (home_mid - away_mid) * 0.25 + ovr_gap * 0.40 + home_adv;
        let away_edge =
            (away_att - home_def) * 0.45 + (away_mid - home_mid) * 0.25 - ovr_gap * 0.40 + away_adv;

        let temp_mid = MatchEngine::temps_factor(input.referee_temper);
        let mut base_home_rate = shot_rate_for_edge(home_edge);
        let mut base_away_rate = shot_rate_for_edge(away_edge);
        let mut base_home_conv = conversion_for_edge(home_edge, input.home_tactical_bonus);
        let mut base_away_conv = conversion_for_edge(away_edge, input.away_tactical_bonus);
        if let Some((side, mentality)) = adjustment {
            let (rate_mul, conv_mul, opp_rate_mul) = mentality.multipliers();
            match side {
                MatchSide::Home => {
                    base_home_rate *= rate_mul;
                    base_home_conv = (base_home_conv * conv_mul).clamp(0.03, 0.40);
                    base_away_rate *= opp_rate_mul;
                }
                MatchSide::Away => {
                    base_away_rate *= rate_mul;
                    base_away_conv = (base_away_conv * conv_mul).clamp(0.03, 0.40);
                    base_home_rate *= opp_rate_mul;
                }
            }
        }

        let home_attack_share =
            (base_home_rate / (base_home_rate + base_away_rate).max(0.001)).clamp(0.12, 0.88);
        let home_penalty_rate = (MatchEngine::PENALTY_POOL / 90.0) * home_attack_share;
        let away_penalty_rate = (MatchEngine::PENALTY_POOL / 90.0) * (1.0 - home_attack_share);
        let own_goal_rate = MatchEngine::OWN_GOAL_POOL / 90.0;
        let yellow_rate = MatchEngine::YELLOW_POOL / 90.0 / 2.0 * temp_mid;
        let red_rate = MatchEngine::RED_POOL / 90.0 / 2.0 * temp_mid;
        let injury_rate = MatchEngine::INJURY_POOL / 90.0 / 2.0;

        MatchBases {
            home_mid,
            away_mid,
            ovr_gap,
            base_home_rate,
            base_away_rate,
            base_home_conv,
            base_away_conv,
            home_attack_share,
            home_penalty_rate,
            away_penalty_rate,
            own_goal_rate,
            yellow_rate,
            red_rate,
            injury_rate,
        }
    }
}

/// Scoring parameters derived from the on-pitch elevens.
#[derive(Debug, Clone, Copy)]
struct MatchBases {
    home_mid: f64,
    away_mid: f64,
    ovr_gap: f64,
    base_home_rate: f64,
    base_away_rate: f64,
    base_home_conv: f64,
    base_away_conv: f64,
    home_attack_share: f64,
    home_penalty_rate: f64,
    away_penalty_rate: f64,
    own_goal_rate: f64,
    yellow_rate: f64,
    red_rate: f64,
    injury_rate: f64,
}

/// Tactical instruction applied to the user's side for the remaining minutes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum MatchMentality {
    Defensive,
    #[default]
    Balanced,
    Attacking,
}

impl MatchMentality {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Defensive => "defensive",
            Self::Balanced => "balanced",
            Self::Attacking => "attacking",
        }
    }

    /// `(own shot-rate, own conversion, opponent shot-rate)` multipliers.
    const fn multipliers(self) -> (f64, f64, f64) {
        match self {
            Self::Defensive => (0.88, 0.97, 0.93),
            Self::Balanced => (1.0, 1.0, 1.0),
            Self::Attacking => (1.15, 1.03, 1.06),
        }
    }
}

/// Incremental live match: the same simulation, pausable so the manager can adjust at the break.
pub struct LiveMatch {
    input: MatchInput,
    home_starters: Vec<Player>,
    away_starters: Vec<Player>,
    state: SimState,
    bases: MatchBases,
    minute: i32,
    adjustment: Option<(MatchSide, MatchMentality)>,
    subs_used: [i32; 2],
}

impl LiveMatch {
    /// Maximum manual substitutions per side, matching the automatic rotation cap.
    pub const MAX_SUBS: i32 = 5;

    /// Builds the session the same way [`MatchEngine::simulate`] picks its elevens.
    #[must_use]
    pub fn from_input(input: &MatchInput, record_events: bool, banned: &HashSet<PlayerId>) -> Self {
        let home = Lineup::best(
            &input.home_squad,
            input.home_formation,
            Lineup::DEFAULT_FITNESS_FLOOR,
            banned,
            &input.home_lineup_ids,
        );
        let away = Lineup::best(
            &input.away_squad,
            input.away_formation,
            Lineup::DEFAULT_FITNESS_FLOOR,
            banned,
            &input.away_lineup_ids,
        );
        Self::from_lineups(input, &home, &away, record_events)
    }

    #[must_use]
    pub fn from_lineups(
        input: &MatchInput,
        home: &Lineup,
        away: &Lineup,
        record_events: bool,
    ) -> Self {
        Self {
            input: input.clone(),
            home_starters: home.starters.clone(),
            away_starters: away.starters.clone(),
            state: SimState::new(input, home, away, record_events),
            bases: MatchEngine::compute_bases(input, &home.starters, &away.starters, None),
            minute: 0,
            adjustment: None,
            subs_used: [0, 0],
        }
    }

    #[must_use]
    pub const fn minute(&self) -> i32 {
        self.minute
    }

    #[must_use]
    pub const fn score(&self) -> (i32, i32) {
        (self.state.home_goals, self.state.away_goals)
    }

    #[must_use]
    pub fn on_pitch(&self, side: MatchSide) -> Vec<Player> {
        self.state
            .on_pitch(side)
            .iter()
            .map(|o| o.player.clone())
            .collect()
    }

    #[must_use]
    pub fn bench(&self, side: MatchSide) -> Vec<Player> {
        match side {
            MatchSide::Home => self.state.home_bench.clone(),
            MatchSide::Away => self.state.away_bench.clone(),
        }
    }

    #[must_use]
    pub fn events(&self) -> &[MatchEvent] {
        &self.state.events
    }

    #[must_use]
    pub const fn subs_used(&self, side: MatchSide) -> i32 {
        match side {
            MatchSide::Home => self.subs_used[0],
            MatchSide::Away => self.subs_used[1],
        }
    }

    /// Simulates every minute up to `until` (clamped to `120`) and returns the new clock.
    pub fn advance_to<R: Rng>(&mut self, rng: &mut R, until: i32) -> i32 {
        let end = until.clamp(self.minute, 120);
        if end > self.minute {
            let start = self.minute + 1;
            self.run_minutes(rng, start, end);
            self.minute = end;
        }
        end
    }

    /// Applies substitutions and/or a mentality change for `side`; the remaining minutes use them.
    pub fn adjust<R: Rng>(
        &mut self,
        rng: &mut R,
        side: MatchSide,
        substitutions: &[(PlayerId, PlayerId)],
        mentality: Option<MatchMentality>,
    ) -> Result<(), NaratamaError> {
        if self.minute >= 90 {
            return Err(NaratamaError::Validation(
                "Match is over; no further adjustments".into(),
            ));
        }
        let slot = match side {
            MatchSide::Home => 0,
            MatchSide::Away => 1,
        };
        let requested = i32::try_from(substitutions.len()).unwrap_or(0);
        if self.subs_used[slot] + requested > Self::MAX_SUBS {
            return Err(NaratamaError::Validation(format!(
                "Only {} substitutions remain",
                Self::MAX_SUBS - self.subs_used[slot]
            )));
        }
        for &(out_id, in_id) in substitutions {
            let team_id = match side {
                MatchSide::Home => self.input.home_team_id,
                MatchSide::Away => self.input.away_team_id,
            };
            if !self
                .state
                .force_substitution(side, out_id, in_id, self.minute, team_id)
            {
                return Err(NaratamaError::Validation(format!(
                    "Cannot substitute {} for {} at {}'",
                    in_id.0, out_id.0, self.minute
                )));
            }
        }
        self.subs_used[slot] += requested;
        if let Some(mentality) = mentality {
            self.adjustment = Some((side, mentality));
        }
        // The remaining minutes are played with the players now on the pitch.
        let home_players: Vec<Player> = self.on_pitch(MatchSide::Home);
        let away_players: Vec<Player> = self.on_pitch(MatchSide::Away);
        self.bases =
            MatchEngine::compute_bases(&self.input, &home_players, &away_players, self.adjustment);
        let _ = rng;
        Ok(())
    }

    /// Plays out the remaining minutes (including extra time when configured) and builds the result.
    #[must_use]
    pub fn run_full(mut self, rng: &mut impl Rng) -> MatchResult {
        self.advance_to(rng, 90);
        let went_to_extra_time =
            self.input.extra_time && self.state.home_goals == self.state.away_goals;
        let final_minute = if went_to_extra_time {
            self.advance_to(rng, 120)
        } else {
            90
        };
        self.state.finalize_minutes(rng, final_minute);
        self.state
            .finalize_ratings(rng, &self.input, &self.home_starters, &self.away_starters);
        self.into_result(went_to_extra_time)
    }

    /// Finishes the match from the current minute, including extra time when configured.
    #[must_use]
    pub fn finish(self, rng: &mut impl Rng) -> MatchResult {
        self.run_full(rng)
    }

    fn into_result(self, went_to_extra_time: bool) -> MatchResult {
        let input = &self.input;
        let att_general = FinanceCalc::attendance_for_tier(
            input.capacity_general,
            input.home_power,
            input.away_power,
            input.ticket_price_general,
            input.reference_price_general,
            input.home_form,
            input.competition_prestige,
        );
        let att_stand = FinanceCalc::attendance_for_tier(
            input.capacity_stand,
            input.home_power,
            input.away_power,
            input.ticket_price_stand,
            input.reference_price_stand,
            input.home_form,
            input.competition_prestige,
        );
        let att_terrace = FinanceCalc::attendance_for_tier(
            input.capacity_terrace,
            input.home_power,
            input.away_power,
            input.ticket_price_terrace,
            input.reference_price_terrace,
            input.home_form,
            input.competition_prestige,
        );
        let att_boxes = FinanceCalc::box_attendance(
            input.capacity_boxes,
            input.home_power,
            input.away_power,
            input.home_form,
            input.competition_prestige,
        );

        let revenue = i64::from(att_general) * i64::from(input.ticket_price_general)
            + i64::from(att_stand) * i64::from(input.ticket_price_stand)
            + i64::from(att_terrace) * i64::from(input.ticket_price_terrace)
            + i64::from(att_boxes) * i64::from(input.ticket_price_boxes);

        let mut state = self.state;
        state.events.sort_by(|a, b| {
            a.minute
                .cmp(&b.minute)
                .then_with(|| a.event_type.as_str().cmp(b.event_type.as_str()))
        });

        let poss_home_edge = if input.neutral_venue { 0.0 } else { 1.5 };
        let mid_diff = (self.bases.home_mid - self.bases.away_mid) * 0.65
            + self.bases.ovr_gap * 0.35
            + poss_home_edge;
        let home_possession = (50.0 + mid_diff * 0.85).round().clamp(25.0, 75.0) as i32;

        let mut player_rows: Vec<MatchPlayerRow> =
            state.rows.into_values().filter(|r| r.minutes > 0).collect();
        player_rows.sort_by_key(|r| (r.team_id, !r.starter, r.player_id));

        MatchResult {
            home_goals: state.home_goals,
            away_goals: state.away_goals,
            went_to_extra_time,
            events: if state.record_events {
                state.events
            } else {
                Vec::new()
            },
            player_rows,
            attendance_general: att_general,
            attendance_stand: att_stand,
            attendance_terrace: att_terrace,
            attendance_boxes: att_boxes,
            revenue,
            home_shots: state.home_shots,
            away_shots: state.away_shots,
            home_shots_on_target: state.home_shots_on_target,
            away_shots_on_target: state.away_shots_on_target,
            home_saves: state.home_saves,
            away_saves: state.away_saves,
            home_possession,
        }
    }

    #[allow(clippy::too_many_lines)]
    fn run_minutes<R: Rng>(&mut self, rng: &mut R, start_min: i32, end_min: i32) {
        let bases = self.bases;
        let input = &self.input;
        let state = &mut self.state;
        for minute in start_min..=end_min {
            let home_missing = 11_i32.saturating_sub(state.home_on.len() as i32);
            let away_missing = 11_i32.saturating_sub(state.away_on.len() as i32);

            let mut home_rate =
                bases.base_home_rate * 0.82_f64.powi(home_missing) * 1.16_f64.powi(away_missing);
            let mut away_rate =
                bases.base_away_rate * 0.82_f64.powi(away_missing) * 1.16_f64.powi(home_missing);
            let mut home_conv = bases.base_home_conv * 1.08_f64.powi(away_missing);
            let mut away_conv = bases.base_away_conv * 1.08_f64.powi(home_missing);

            if bases.ovr_gap >= 10.0 && minute >= 35 && (state.home_goals - state.away_goals) <= 1 {
                let surge = (1.0 + (bases.ovr_gap - 8.0) * 0.015).clamp(1.08, 1.38);
                home_rate *= surge;
                home_conv = (home_conv * 1.12).clamp(0.04, 0.36);
            } else if bases.ovr_gap <= -10.0
                && minute >= 35
                && (state.away_goals - state.home_goals) <= 1
            {
                let surge = (1.0 + (-bases.ovr_gap - 8.0) * 0.015).clamp(1.08, 1.38);
                away_rate *= surge;
                away_conv = (away_conv * 1.12).clamp(0.04, 0.36);
            }

            // Home & away open-play attacks
            if rng.gen::<f64>() < home_rate {
                state.process_attack(rng, MatchSide::Home, minute, home_conv);
            }
            if rng.gen::<f64>() < away_rate {
                state.process_attack(rng, MatchSide::Away, minute, away_conv);
            }

            // Penalties
            if rng.gen::<f64>() < bases.home_penalty_rate {
                state.process_penalty(rng, MatchSide::Home, minute);
            }
            if rng.gen::<f64>() < bases.away_penalty_rate {
                state.process_penalty(rng, MatchSide::Away, minute);
            }

            // Own goals
            if rng.gen::<f64>() < bases.own_goal_rate {
                let benefits_home = rng.gen::<f64>() < bases.home_attack_share;
                state.process_own_goal(rng, benefits_home, minute);
            }

            // Cards
            let home_foul_share = (1.0 - bases.home_attack_share * 0.4 + 0.2).clamp(0.30, 0.70);
            if rng.gen::<f64>() < bases.yellow_rate * 2.0 {
                let side = if rng.gen::<f64>() < home_foul_share {
                    MatchSide::Home
                } else {
                    MatchSide::Away
                };
                state.process_yellow_card(rng, side, minute);
            }
            if rng.gen::<f64>() < bases.red_rate * 2.0 {
                let side = if rng.gen::<f64>() < home_foul_share {
                    MatchSide::Home
                } else {
                    MatchSide::Away
                };
                state.process_straight_red(rng, side, minute);
            }

            // Injuries
            let inj_side = if rng.gen::<bool>() {
                MatchSide::Home
            } else {
                MatchSide::Away
            };
            let reduction = match inj_side {
                MatchSide::Home => input.home_injury_reduction,
                MatchSide::Away => input.away_injury_reduction,
            };
            if rng.gen::<f64>() < bases.injury_rate * (1.0 - reduction).clamp(0.1, 1.0) {
                state.process_injury(rng, inj_side, minute, input);
            }

            // Routine substitutions
            if minute == 62 || minute == 72 || minute == 84 || minute == 105 {
                if rng.gen::<f64>() < 0.85 {
                    state.make_substitution(MatchSide::Home, minute, input);
                }
                if rng.gen::<f64>() < 0.85 {
                    state.make_substitution(MatchSide::Away, minute, input);
                }
            }
        }
    }
}

impl MatchEngine {
    /// Best-of-5 penalty shootout followed by sudden death.
    /// Fixes Bug #16: tracks `home_taken` and `away_taken` separately for early termination.
    #[must_use]
    pub fn penalty_shootout(rng: &mut impl Rng, input: &MatchInput) -> MatchPenaltyShootout {
        let empty = HashSet::new();
        let home = Lineup::best(
            &input.home_squad,
            input.home_formation,
            Lineup::DEFAULT_FITNESS_FLOOR,
            &empty,
            &input.home_lineup_ids,
        );
        let away = Lineup::best(
            &input.away_squad,
            input.away_formation,
            Lineup::DEFAULT_FITNESS_FLOOR,
            &empty,
            &input.away_lineup_ids,
        );
        let home_kickers = shootout_kickers(&home.starters);
        let away_kickers = shootout_kickers(&away.starters);

        let mut events = Vec::new();
        let mut home_score = 0_i32;
        let mut away_score = 0_i32;
        let mut home_taken = 0_i32;
        let mut away_taken = 0_i32;
        let mut round = 1_usize;

        loop {
            let sudden_death = round > 5;
            for side in [MatchSide::Home, MatchSide::Away] {
                let kickers = match side {
                    MatchSide::Home => &home_kickers,
                    MatchSide::Away => &away_kickers,
                };
                let shooter = if kickers.is_empty() {
                    None
                } else {
                    Some(&kickers[(round - 1) % kickers.len()])
                };

                let scored = rng.gen::<f64>() < Self::PENALTY_CONVERT;
                match side {
                    MatchSide::Home => {
                        home_taken += 1;
                        if scored {
                            home_score += 1;
                        }
                    }
                    MatchSide::Away => {
                        away_taken += 1;
                        if scored {
                            away_score += 1;
                        }
                    }
                }

                events.push(MatchEvent {
                    minute: 120,
                    event_type: MatchEventType::ShootoutKick,
                    side,
                    player_id: shooter.map(|p| p.player_id),
                    player_name: shooter.map(|p| p.name.clone()),
                    detail: Some(format!(
                        "{} {round}",
                        if scored { "scored" } else { "missed" }
                    )),
                });

                if !sudden_death {
                    let home_rem = (5 - home_taken).max(0);
                    let away_rem = (5 - away_taken).max(0);
                    if home_score > away_score + away_rem || away_score > home_score + home_rem {
                        return MatchPenaltyShootout {
                            home_score,
                            away_score,
                            events,
                        };
                    }
                }
            }

            if sudden_death && home_score != away_score {
                return MatchPenaltyShootout {
                    home_score,
                    away_score,
                    events,
                };
            }
            round += 1;
        }
    }
}

struct SimState {
    record_events: bool,
    home_goals: i32,
    away_goals: i32,
    home_saves: i32,
    away_saves: i32,
    home_shots: i32,
    away_shots: i32,
    home_shots_on_target: i32,
    away_shots_on_target: i32,
    events: Vec<MatchEvent>,
    home_on: Vec<OnPitch>,
    away_on: Vec<OnPitch>,
    home_bench: Vec<Player>,
    away_bench: Vec<Player>,
    rows: HashMap<PlayerId, MatchPlayerRow>,
    injured_ids: HashSet<PlayerId>,
}

impl SimState {
    fn new(input: &MatchInput, home: &Lineup, away: &Lineup, record_events: bool) -> Self {
        let mut rows = HashMap::new();
        for p in &home.starters {
            register_row(&mut rows, p, true, input.home_team_id);
        }
        for p in &away.starters {
            register_row(&mut rows, p, true, input.away_team_id);
        }
        for p in &home.substitutes {
            register_row(&mut rows, p, false, input.home_team_id);
        }
        for p in &away.substitutes {
            register_row(&mut rows, p, false, input.away_team_id);
        }

        let home_on = home
            .starters
            .iter()
            .cloned()
            .map(|player| OnPitch {
                player,
                entered_at: 0,
            })
            .collect();
        let away_on = away
            .starters
            .iter()
            .cloned()
            .map(|player| OnPitch {
                player,
                entered_at: 0,
            })
            .collect();

        Self {
            record_events,
            home_goals: 0,
            away_goals: 0,
            home_saves: 0,
            away_saves: 0,
            home_shots: 0,
            away_shots: 0,
            home_shots_on_target: 0,
            away_shots_on_target: 0,
            events: Vec::new(),
            home_on,
            away_on,
            home_bench: home.substitutes.clone(),
            away_bench: away.substitutes.clone(),
            rows,
            injured_ids: HashSet::new(),
        }
    }

    fn on_pitch(&self, side: MatchSide) -> &[OnPitch] {
        match side {
            MatchSide::Home => &self.home_on,
            MatchSide::Away => &self.away_on,
        }
    }

    fn on_pitch_mut(&mut self, side: MatchSide) -> &mut Vec<OnPitch> {
        match side {
            MatchSide::Home => &mut self.home_on,
            MatchSide::Away => &mut self.away_on,
        }
    }

    fn add_event(
        &mut self,
        event_type: MatchEventType,
        minute: i32,
        side: MatchSide,
        player: Option<&Player>,
        detail: Option<&str>,
    ) {
        if !self.record_events {
            return;
        }
        self.events.push(MatchEvent {
            minute,
            event_type,
            side,
            player_id: player.map(|p| p.player_id),
            player_name: player.map(|p| p.name.clone()),
            detail: detail.map(str::to_string),
        });
    }

    fn pick_scorer(&self, rng: &mut impl Rng, side: MatchSide, set_piece: bool) -> Option<Player> {
        weighted_pick(rng, self.on_pitch(side), |p| {
            let base = match Lineup::group_of(p) {
                PositionGroup::Attacker => 3.2,
                PositionGroup::Midfielder => 1.4,
                PositionGroup::Defender => 0.50,
                PositionGroup::Goalkeeper => 0.03,
            };
            let mut w = base * (f64::from(p.power.clamp(35, 99)) / 70.0);
            if set_piece {
                w += f64::from(p.power) / 250.0;
            }
            w
        })
    }

    fn pick_assister(
        &self,
        rng: &mut impl Rng,
        side: MatchSide,
        scorer_id: PlayerId,
    ) -> Option<Player> {
        let pool: Vec<&OnPitch> = self
            .on_pitch(side)
            .iter()
            .filter(|o| {
                o.player.player_id != scorer_id
                    && Lineup::group_of(&o.player) != PositionGroup::Goalkeeper
            })
            .collect();
        weighted_pick_refs(rng, &pool, |p| {
            let role_w = match Lineup::group_of(p) {
                PositionGroup::Midfielder => 1.6,
                PositionGroup::Attacker => 1.2,
                PositionGroup::Defender | PositionGroup::Goalkeeper => 0.8,
            };
            role_w * (f64::from(p.power.clamp(35, 99)) / 70.0)
        })
    }

    fn pick_fouler(&self, rng: &mut impl Rng, side: MatchSide) -> Option<Player> {
        weighted_pick(rng, self.on_pitch(side), |p| match Lineup::group_of(p) {
            PositionGroup::Defender => 1.6,
            PositionGroup::Midfielder => 1.3,
            PositionGroup::Attacker | PositionGroup::Goalkeeper => 0.9,
        })
    }

    fn pick_injured(&self, rng: &mut impl Rng, side: MatchSide) -> Option<Player> {
        weighted_pick(rng, self.on_pitch(side), |p| {
            if Lineup::group_of(p) == PositionGroup::Attacker {
                1.0 + MatchEngine::ATTACKER_INJURY_BIAS
            } else {
                1.0
            }
        })
    }

    fn pick_keeper(&self, side: MatchSide) -> Option<Player> {
        self.on_pitch(side)
            .iter()
            .find(|o| Lineup::group_of(&o.player) == PositionGroup::Goalkeeper)
            .map(|o| o.player.clone())
    }

    fn process_attack(&mut self, rng: &mut impl Rng, side: MatchSide, minute: i32, conv: f64) {
        match side {
            MatchSide::Home => self.home_shots += 1,
            MatchSide::Away => self.away_shots += 1,
        }
        let scorer = self.pick_scorer(rng, side, false);
        let assister = scorer
            .as_ref()
            .and_then(|s| self.pick_assister(rng, side, s.player_id));

        let roll = rng.gen::<f64>();
        if roll < conv {
            match side {
                MatchSide::Home => self.home_shots_on_target += 1,
                MatchSide::Away => self.away_shots_on_target += 1,
            }
            self.score_goal(rng, side, minute, scorer.as_ref(), assister.as_ref(), false);
        } else if roll < conv + MatchEngine::ON_TARGET_SHARE {
            match side {
                MatchSide::Home => {
                    self.home_shots_on_target += 1;
                    self.away_saves += 1;
                }
                MatchSide::Away => {
                    self.away_shots_on_target += 1;
                    self.home_saves += 1;
                }
            }
            let keeper = self.pick_keeper(side.opposite());
            self.add_event(
                MatchEventType::Save,
                minute,
                side.opposite(),
                keeper.as_ref(),
                None,
            );
        }
    }

    fn process_penalty(&mut self, rng: &mut impl Rng, side: MatchSide, minute: i32) {
        match side {
            MatchSide::Home => self.home_shots += 1,
            MatchSide::Away => self.away_shots += 1,
        }
        let taker = self.pick_scorer(rng, side, true);
        if rng.gen::<f64>() < MatchEngine::PENALTY_CONVERT {
            match side {
                MatchSide::Home => self.home_shots_on_target += 1,
                MatchSide::Away => self.away_shots_on_target += 1,
            }
            self.score_goal(rng, side, minute, taker.as_ref(), None, true);
        } else {
            self.add_event(
                MatchEventType::PenaltyMissed,
                minute,
                side,
                taker.as_ref(),
                None,
            );
        }
    }

    fn process_own_goal(&mut self, rng: &mut impl Rng, benefits_home: bool, minute: i32) {
        let culprit_side = if benefits_home {
            MatchSide::Away
        } else {
            MatchSide::Home
        };
        let culprit = weighted_pick(rng, self.on_pitch(culprit_side), |p| {
            if Lineup::group_of(p) == PositionGroup::Defender {
                2.0
            } else {
                1.0
            }
        });
        self.add_event(
            MatchEventType::OwnGoal,
            minute,
            culprit_side,
            culprit.as_ref(),
            Some("deflected in"),
        );
        if benefits_home {
            self.home_goals += 1;
            self.home_shots += 1;
            self.home_shots_on_target += 1;
        } else {
            self.away_goals += 1;
            self.away_shots += 1;
            self.away_shots_on_target += 1;
        }
    }

    fn score_goal(
        &mut self,
        rng: &mut impl Rng,
        side: MatchSide,
        minute: i32,
        scorer: Option<&Player>,
        assister: Option<&Player>,
        from_penalty: bool,
    ) {
        let Some(scorer) = scorer else { return };
        if !self.rows.contains_key(&scorer.player_id) {
            return;
        }

        match side {
            MatchSide::Home => self.home_goals += 1,
            MatchSide::Away => self.away_goals += 1,
        }
        if let Some(row) = self.rows.get_mut(&scorer.player_id) {
            row.goals += 1;
        }

        if let Some(assister) = assister.filter(|_| !from_penalty) {
            if let Some(assist_row) = self.rows.get_mut(&assister.player_id) {
                assist_row.assists += 1;
                self.add_event(MatchEventType::Assist, minute, side, Some(assister), None);
            }
        }

        if from_penalty {
            self.add_event(
                MatchEventType::PenaltyScored,
                minute,
                side,
                Some(scorer),
                Some("converted"),
            );
            return;
        }

        self.add_event(MatchEventType::Goal, minute, side, Some(scorer), None);
        if self.record_events && rng.gen::<f64>() < 0.10 {
            self.add_event(MatchEventType::VarCheck, minute, side, None, None);
            if rng.gen::<f64>() < 0.20 {
                self.add_event(
                    MatchEventType::VarGoalDisallowed,
                    minute,
                    side,
                    Some(scorer),
                    Some("goal disallowed"),
                );
                match side {
                    MatchSide::Home => self.home_goals -= 1,
                    MatchSide::Away => self.away_goals -= 1,
                }
                if let Some(row) = self.rows.get_mut(&scorer.player_id) {
                    row.goals = (row.goals - 1).max(0);
                }
                if let Some(assister) = assister {
                    if let Some(assist_row) = self.rows.get_mut(&assister.player_id) {
                        assist_row.assists = (assist_row.assists - 1).max(0);
                    }
                    self.events.retain(|ev| {
                        !(ev.event_type == MatchEventType::Assist
                            && ev.minute == minute
                            && ev.player_id == Some(assister.player_id))
                    });
                }
            }
        }
    }

    fn process_yellow_card(&mut self, rng: &mut impl Rng, side: MatchSide, minute: i32) {
        let Some(fouler) = self.pick_fouler(rng, side) else {
            return;
        };
        let second_yellow = {
            let Some(row) = self.rows.get_mut(&fouler.player_id) else {
                return;
            };
            row.yellow_cards += 1;
            row.yellow_cards >= 2
        };
        self.add_event(
            MatchEventType::YellowCard,
            minute,
            side,
            Some(&fouler),
            None,
        );

        if second_yellow {
            if let Some(row) = self.rows.get_mut(&fouler.player_id) {
                row.red_cards = 1;
                row.yellow_cards = 1;
            }
            self.send_off(side, fouler.player_id, minute);
            self.add_event(
                MatchEventType::RedCard,
                minute,
                side,
                Some(&fouler),
                Some("second yellow"),
            );
        } else if let Some(row) = self.rows.get_mut(&fouler.player_id) {
            row.rating -= 4;
        }
    }

    fn process_straight_red(&mut self, rng: &mut impl Rng, side: MatchSide, minute: i32) {
        let Some(fouler) = self.pick_fouler(rng, side) else {
            return;
        };
        if let Some(row) = self.rows.get_mut(&fouler.player_id) {
            row.red_cards = 1;
        } else {
            return;
        }
        self.send_off(side, fouler.player_id, minute);
        self.add_event(
            MatchEventType::RedCard,
            minute,
            side,
            Some(&fouler),
            Some("straight red"),
        );
    }

    fn send_off(&mut self, side: MatchSide, player_id: PlayerId, minute: i32) {
        let on = self.on_pitch_mut(side);
        if let Some(pos) = on.iter().position(|o| o.player.player_id == player_id) {
            let removed = on.remove(pos);
            if let Some(row) = self.rows.get_mut(&player_id) {
                row.minutes = if removed.entered_at <= 0 {
                    minute
                } else {
                    minute - removed.entered_at
                };
            }
        }
    }

    fn process_injury(
        &mut self,
        rng: &mut impl Rng,
        side: MatchSide,
        minute: i32,
        input: &MatchInput,
    ) {
        let Some(hurt) = self.pick_injured(rng, side) else {
            return;
        };
        self.injured_ids.insert(hurt.player_id);
        self.add_event(MatchEventType::Injury, minute, side, Some(&hurt), None);
        self.make_substitution(side, minute, input);
    }

    fn make_substitution(&mut self, side: MatchSide, minute: i32, input: &MatchInput) {
        let (on, bench, team_id) = match side {
            MatchSide::Home => (&mut self.home_on, &mut self.home_bench, input.home_team_id),
            MatchSide::Away => (&mut self.away_on, &mut self.away_bench, input.away_team_id),
        };
        if bench.is_empty() || on.len() <= 7 {
            return;
        }
        let Some(bench_idx) = bench
            .iter()
            .position(|p| Lineup::group_of(p) != PositionGroup::Goalkeeper)
        else {
            return;
        };
        let incoming = bench.remove(bench_idx);
        let incoming_group = Lineup::group_of(&incoming);

        let mut worst_same: Option<(usize, i32)> = None;
        let mut worst_any: Option<(usize, i32)> = None;
        for (idx, o) in on.iter().enumerate() {
            let g = Lineup::group_of(&o.player);
            if g == PositionGroup::Goalkeeper {
                continue;
            }
            if worst_any.is_none_or(|(_, p)| o.player.power < p) {
                worst_any = Some((idx, o.player.power));
            }
            if g == incoming_group && worst_same.is_none_or(|(_, p)| o.player.power < p) {
                worst_same = Some((idx, o.player.power));
            }
        }
        let Some((out_idx, _)) = worst_same.or(worst_any) else {
            return;
        };

        register_row(&mut self.rows, &incoming, false, team_id);
        let outgoing = on.remove(out_idx);
        if let Some(out_row) = self.rows.get_mut(&outgoing.player.player_id) {
            out_row.minutes = if outgoing.entered_at <= 0 {
                minute
            } else {
                minute - outgoing.entered_at
            };
        }
        if let Some(in_row) = self.rows.get_mut(&incoming.player_id) {
            in_row.starter = false;
        }
        on.push(OnPitch {
            player: incoming.clone(),
            entered_at: minute,
        });

        self.add_event(
            MatchEventType::SubstitutionOut,
            minute,
            side,
            Some(&outgoing.player),
            None,
        );
        self.add_event(
            MatchEventType::SubstitutionIn,
            minute,
            side,
            Some(&incoming),
            None,
        );
    }

    /// Manual substitution chosen by the manager; returns false when the pair is not legal.
    fn force_substitution(
        &mut self,
        side: MatchSide,
        out_id: PlayerId,
        in_id: PlayerId,
        minute: i32,
        team_id: TeamId,
    ) -> bool {
        let (on, bench) = match side {
            MatchSide::Home => (&mut self.home_on, &mut self.home_bench),
            MatchSide::Away => (&mut self.away_on, &mut self.away_bench),
        };
        if on.len() <= 7 {
            return false;
        }
        let Some(bench_idx) = bench.iter().position(|p| p.player_id == in_id) else {
            return false;
        };
        let Some(out_idx) = on.iter().position(|o| o.player.player_id == out_id) else {
            return false;
        };
        let incoming = bench.remove(bench_idx);
        register_row(&mut self.rows, &incoming, false, team_id);
        let outgoing = on.remove(out_idx);
        if let Some(out_row) = self.rows.get_mut(&outgoing.player.player_id) {
            out_row.minutes = if outgoing.entered_at <= 0 {
                minute
            } else {
                minute - outgoing.entered_at
            };
        }
        if let Some(in_row) = self.rows.get_mut(&incoming.player_id) {
            in_row.starter = false;
        }
        on.push(OnPitch {
            player: incoming.clone(),
            entered_at: minute,
        });
        self.add_event(
            MatchEventType::SubstitutionOut,
            minute,
            side,
            Some(&outgoing.player),
            None,
        );
        self.add_event(
            MatchEventType::SubstitutionIn,
            minute,
            side,
            Some(&incoming),
            None,
        );
        true
    }

    fn finalize_minutes(&mut self, rng: &mut impl Rng, final_minute: i32) {
        for o in self.home_on.iter().chain(self.away_on.iter()) {
            if let Some(row) = self.rows.get_mut(&o.player.player_id) {
                row.minutes = if o.entered_at <= 0 {
                    final_minute
                } else {
                    final_minute - o.entered_at
                };
            }
        }
        for &id in &self.injured_ids {
            if let Some(row) = self.rows.get_mut(&id) {
                if row.minutes > 80 {
                    row.minutes = 25 + rng.gen_range(0..45);
                }
            }
        }
    }

    fn finalize_ratings(
        &mut self,
        rng: &mut impl Rng,
        input: &MatchInput,
        home_starters: &[Player],
        away_starters: &[Player],
    ) {
        let home_outcome = match self.home_goals.cmp(&self.away_goals) {
            std::cmp::Ordering::Greater => 3,
            std::cmp::Ordering::Equal => 1,
            std::cmp::Ordering::Less => 0,
        };
        let away_outcome = match self.away_goals.cmp(&self.home_goals) {
            std::cmp::Ordering::Greater => 3,
            std::cmp::Ordering::Equal => 1,
            std::cmp::Ordering::Less => 0,
        };

        // Deterministic iteration order by player_id so seeded RNG gives identical ratings.
        let mut ids: Vec<PlayerId> = self.rows.keys().copied().collect();
        ids.sort();
        for id in ids {
            if let Some(row) = self.rows.get_mut(&id) {
                if row.minutes == 0 {
                    continue;
                }
                // Bug #7 fix: check team_id directly instead of searching mutated bench.
                let outcome = if row.team_id == input.home_team_id {
                    home_outcome
                } else {
                    away_outcome
                };
                let noise = (rng.gen::<f64>() - 0.5) * 8.0;
                let red_pen = if row.red_cards == 1 { -16 } else { 0 };
                let raw = f64::from(
                    44 + outcome * 6 + row.goals * 14 + row.assists * 8 + red_pen
                        - row.yellow_cards * 3,
                ) + noise;
                row.rating = (raw.round() as i32).clamp(20, 100);
                row.points = outcome + row.goals * 2;
            }
        }

        let apply_keeper_credit = |rows: &mut HashMap<PlayerId, MatchPlayerRow>,
                                   starters: &[Player],
                                   saves: i32,
                                   conceded: i32| {
            for p in starters {
                if Lineup::group_of(p) != PositionGroup::Goalkeeper {
                    continue;
                }
                if let Some(row) = rows.get_mut(&p.player_id) {
                    if row.minutes == 0 {
                        continue;
                    }
                    let clean_sheet = if conceded == 0 { 12 } else { 0 };
                    let penalty = if conceded > 1 { (conceded - 1) * 2 } else { 0 };
                    row.rating = (row.rating + saves * 4 + clean_sheet - penalty).clamp(20, 100);
                    row.points += (saves / 3) + if conceded == 0 { 3 } else { 0 };
                }
            }
        };
        apply_keeper_credit(
            &mut self.rows,
            home_starters,
            self.home_saves,
            self.away_goals,
        );
        apply_keeper_credit(
            &mut self.rows,
            away_starters,
            self.away_saves,
            self.home_goals,
        );

        let mvp_id = self
            .rows
            .values()
            .filter(|r| r.minutes >= 30)
            .max_by_key(|r| (r.rating, std::cmp::Reverse(r.player_id)))
            .map(|r| r.player_id);
        if let Some(id) = mvp_id {
            if let Some(row) = self.rows.get_mut(&id) {
                row.mvp = true;
            }
        }
    }
}

fn register_row(
    rows: &mut HashMap<PlayerId, MatchPlayerRow>,
    p: &Player,
    starter: bool,
    team_id: TeamId,
) {
    rows.entry(p.player_id).or_insert_with(|| MatchPlayerRow {
        player_id: p.player_id,
        player_name: p.name.clone(),
        team_id,
        starter,
        minutes: 0,
        goals: 0,
        assists: 0,
        yellow_cards: 0,
        red_cards: 0,
        mvp: false,
        rating: 50,
        points: 0,
    });
}

fn weighted_pick(
    rng: &mut impl Rng,
    pool: &[OnPitch],
    weight_fn: impl Fn(&Player) -> f64,
) -> Option<Player> {
    let refs: Vec<&OnPitch> = pool.iter().collect();
    weighted_pick_refs(rng, &refs, weight_fn)
}

fn weighted_pick_refs(
    rng: &mut impl Rng,
    pool: &[&OnPitch],
    weight_fn: impl Fn(&Player) -> f64,
) -> Option<Player> {
    if pool.is_empty() {
        return None;
    }
    let total: f64 = pool.iter().map(|o| weight_fn(&o.player)).sum();
    if total <= 0.0 {
        return Some(pool[0].player.clone());
    }
    let mut r = rng.gen::<f64>() * total;
    for o in pool {
        r -= weight_fn(&o.player);
        if r <= 0.0 {
            return Some(o.player.clone());
        }
    }
    pool.last().map(|o| o.player.clone())
}

fn shootout_kickers(starters: &[Player]) -> Vec<Player> {
    let mut outfield: Vec<Player> = starters
        .iter()
        .filter(|p| p.position_id != 1)
        .cloned()
        .collect();
    outfield.sort_by_key(|a| std::cmp::Reverse(a.power));
    let keepers: Vec<Player> = starters
        .iter()
        .filter(|p| p.position_id == 1)
        .cloned()
        .collect();
    outfield.extend(keepers);
    outfield
}

fn shot_rate_for_edge(edge: f64) -> f64 {
    let mult = if edge >= 0.0 {
        (edge * 0.026).exp().clamp(1.0, 2.35)
    } else {
        (edge * 0.052).exp().clamp(0.16, 1.0)
    };
    MatchEngine::BASE_CHANCE_PER_MINUTE * mult
}

fn conversion_for_edge(edge: f64, tactical_bonus: f64) -> f64 {
    let mult = if edge >= 0.0 {
        (edge * 0.015).exp().clamp(1.0, 1.62)
    } else {
        (edge * 0.035).exp().clamp(0.32, 1.0)
    };
    (MatchEngine::CONVERSION_BASE * mult * (1.0 + tactical_bonus)).clamp(0.035, 0.34)
}

fn sample_poisson(rng: &mut impl Rng, lambda: f64) -> i32 {
    let l = (-lambda).exp();
    let mut p = 1.0;
    let mut k = 0;
    loop {
        k += 1;
        p *= rng.gen::<f64>();
        if p <= l {
            return k - 1;
        }
    }
}
