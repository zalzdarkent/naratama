use crate::enums::{
    Confederation, MarketStatus, MatchStatus, PlayerStatus, PreferredFoot, SquadStatus,
    TournamentOrg,
};
use crate::ids::{
    CoachId, CountryId, MatchId, Money, PlayerId, SeasonId, StadiumId, TeamId, TournamentId,
};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveSlotInfo {
    pub slot_index: u8,
    pub is_empty: bool,
    pub manager_name: Option<String>,
    pub club_name: Option<String>,
    pub season_year: Option<i32>,
    pub game_date: Option<NaiveDate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagerProfile {
    pub name: String,
    pub country_id: CountryId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Country {
    pub country_id: CountryId,
    pub name: String,
    pub iso2: String,
    pub power: i32,
    pub confederation: Confederation,
    pub points: f64,
    pub previous_rank: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Team {
    pub team_id: TeamId,
    pub name: String,
    pub abbreviation: String,
    pub country_id: CountryId,
    pub power: i32,
    pub value: Money,
    pub budget: Money,
    pub division: i32,
    pub stadium_id: StadiumId,
    pub coach_id: CoachId,
    pub founded_year: i32,
    pub logo_base64: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Player {
    pub player_id: PlayerId,
    pub name: String,
    pub age: i32,
    pub position_id: i32,
    pub preferred_foot: PreferredFoot,
    pub power: i32,
    pub potential: i32,
    pub fitness: i32,
    pub value: Money,
    pub salary: Money,
    pub country_id: CountryId,
    pub squad_status: SquadStatus,
    pub market_status: MarketStatus,
    pub status: PlayerStatus,
    pub is_captain: bool,
    pub shirt_number: i32,
    pub stars: i32,
    pub skill_a: i32,
    pub skill_b: i32,
    pub height: i32,
    pub championships: Option<String>,
    pub injury_return_date: Option<NaiveDate>,
}

impl Player {
    #[must_use]
    pub fn injury_days_remaining(&self, current_date: NaiveDate) -> Option<i64> {
        let ret = self.injury_return_date?;
        let days = (ret - current_date).num_days();
        (days > 0).then_some(days)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stadium {
    pub stadium_id: StadiumId,
    pub name: String,
    pub city: Option<String>,
    pub country_id: CountryId,
    pub capacity_general: i32,
    pub capacity_stand: i32,
    pub capacity_terrace: i32,
    pub capacity_boxes: i32,
    pub max_general: i32,
    pub max_stand: i32,
    pub max_terrace: i32,
    pub max_boxes: i32,
    pub ticket_price_general: i32,
    pub ticket_price_stand: i32,
    pub ticket_price_terrace: i32,
    pub ticket_price_boxes: i32,
}

impl Stadium {
    pub const DEFAULT_GENERAL_PRICE: i32 = 14;
    pub const DEFAULT_STAND_PRICE: i32 = 38;
    pub const DEFAULT_TERRACE_PRICE: i32 = 21;
    pub const DEFAULT_BOXES_PRICE: i32 = 120;

    #[must_use]
    pub const fn capacity_total(&self) -> i32 {
        self.capacity_general + self.capacity_stand + self.capacity_terrace + self.capacity_boxes
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tournament {
    pub tournament_id: TournamentId,
    pub name: String,
    pub organization: TournamentOrg,
    pub division_id: i32,
    pub slots: i32,
    pub promotion_slots: i32,
    pub relegation_spots: i32,
    pub points: f64,
    pub prize_teams: Money,
    pub prize_top_scorer: Money,
    pub prize_mvp: Money,
    pub play_days: String,
    pub rounds: i32,
    pub logo_base64: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Season {
    pub season_id: SeasonId,
    pub name: String,
    pub start_year: i32,
    pub end_year: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureMatch {
    pub match_id: MatchId,
    pub tournament_id: TournamentId,
    pub season_id: SeasonId,
    pub round: i32,
    pub phase: i32,
    pub group_id: Option<i32>,
    pub status: MatchStatus,
    pub home_team_id: TeamId,
    pub away_team_id: TeamId,
    pub stadium_id: StadiumId,
    pub date: Option<NaiveDate>,
    pub home_goals: Option<i32>,
    pub away_goals: Option<i32>,
    pub winner_id: Option<TeamId>,
    pub penalty_home_goals: Option<i32>,
    pub penalty_away_goals: Option<i32>,
    pub leg: i32,
    pub original_date: Option<NaiveDate>,
    pub reschedule_reason: Option<String>,
    pub neutral_venue: bool,
    pub extra_time: bool,
    pub away_goals_rule: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayableCountrySummary {
    pub country_id: CountryId,
    pub name: String,
    pub iso2: String,
    pub power: i32,
    pub confederation: Confederation,
    pub league_count: i64,
    pub team_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeagueSummary {
    pub tournament_id: TournamentId,
    pub name: String,
    pub division_id: i32,
    pub play_days: String,
    pub slots: i32,
    pub team_count: i64,
    pub logo_base64: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClubFacilities {
    pub team_id: TeamId,
    pub training_level: i32,
    pub medical_level: i32,
}
