use serde::{Deserialize, Serialize};

/// Continental confederation matching `countries.confederation` in `superkickoff_2025.db`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[repr(i64)]
pub enum Confederation {
    #[default]
    None = 0,
    Concacaf = 1,
    Conmebol = 2,
    Uefa = 3,
    Caf = 4,
    Afc = 5,
    Ofc = 6,
}

impl Confederation {
    #[must_use]
    pub const fn from_code(code: i64) -> Self {
        match code {
            1 => Self::Concacaf,
            2 => Self::Conmebol,
            3 => Self::Uefa,
            4 => Self::Caf,
            5 => Self::Afc,
            6 => Self::Ofc,
            _ => Self::None,
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::None => "FIFA",
            Self::Concacaf => "CONCACAF",
            Self::Conmebol => "CONMEBOL",
            Self::Uefa => "UEFA",
            Self::Caf => "CAF",
            Self::Afc => "AFC",
            Self::Ofc => "OFC",
        }
    }
}

/// Canonical 4-group tactical classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum PositionGroup {
    Goalkeeper = 1,
    Defender = 2,
    Midfielder = 3,
    Attacker = 4,
}

impl PositionGroup {
    /// Maps `position_id` (1..=11) using the database `positions.type` convention:
    /// 1=GK, 2..=4=DEF, 5..=8=MID, 9..=11=ATT.
    #[must_use]
    pub const fn from_position_id(position_id: i32) -> Self {
        match position_id {
            1 => Self::Goalkeeper,
            2..=4 => Self::Defender,
            5..=8 => Self::Midfielder,
            _ => Self::Attacker,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum PreferredFoot {
    #[default]
    Right,
    Left,
}

impl PreferredFoot {
    #[must_use]
    pub fn from_db(raw: &str) -> Self {
        if raw.eq_ignore_ascii_case("I") {
            Self::Left
        } else {
            Self::Right
        }
    }

    #[must_use]
    pub const fn to_db(self) -> &'static str {
        match self {
            Self::Right => "D",
            Self::Left => "I",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[repr(i64)]
pub enum PlayerStatus {
    #[default]
    Active = 1,
    Retired = 2,
    Dead = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[repr(i64)]
pub enum SquadStatus {
    Reserve = 0,
    #[default]
    FirstTeam = 1,
    LoanListed = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[repr(i64)]
pub enum MarketStatus {
    #[default]
    Active = 1,
    ForSale = 2,
    OnLoan = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[repr(i64)]
pub enum TournamentOrg {
    #[default]
    League = 1,
    International = 2,
    NationalCup = 3,
    Friendly = 4,
}

impl TournamentOrg {
    #[must_use]
    pub const fn from_code(code: i64) -> Self {
        match code {
            2 => Self::International,
            3 => Self::NationalCup,
            4 => Self::Friendly,
            _ => Self::League,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[repr(i64)]
pub enum MatchStatus {
    #[default]
    Scheduled = 1,
    Finished = 2,
    Cancelled = 3,
}

impl MatchStatus {
    #[must_use]
    pub const fn from_code(code: i64) -> Self {
        match code {
            2 => Self::Finished,
            3 => Self::Cancelled,
            _ => Self::Scheduled,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CompetitionType {
    League,
    DomesticCup,
    Continental,
    SuperCup,
    International,
    Friendly,
}

impl CompetitionType {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::League => "league",
            Self::DomesticCup => "domesticCup",
            Self::Continental => "continental",
            Self::SuperCup => "superCup",
            Self::International => "international",
            Self::Friendly => "friendly",
        }
    }

    #[must_use]
    pub fn from_db_str(s: &str) -> Self {
        match s {
            "domesticCup" => Self::DomesticCup,
            "continental" => Self::Continental,
            "superCup" => Self::SuperCup,
            "international" => Self::International,
            "friendly" => Self::Friendly,
            _ => Self::League,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CompetitionFormat {
    DoubleRoundRobin,
    SingleRoundRobin,
    LeaguePhase,
    GroupStage,
    Knockout,
    SingleMatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum FfpEnforcement {
    #[default]
    Off,
    Relaxed,
    Standard,
    Strict,
}

impl FfpEnforcement {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Relaxed => "relaxed",
            Self::Standard => "standard",
            Self::Strict => "strict",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LedgerPeriod {
    Weekly,
    Matchday,
    Transfer,
    Annual,
    Board,
}

impl LedgerPeriod {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Weekly => "weekly",
            Self::Matchday => "matchday",
            Self::Transfer => "transfer",
            Self::Annual => "annual",
            Self::Board => "board",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LedgerCategory {
    MatchdayTickets,
    MatchdayMerch,
    Broadcast,
    SponsorshipUpfront,
    SponsorshipWeekly,
    SponsorshipBonus,
    PrizeMoney,
    PlayerSales,
    SellOnReceived,
    LoanFeeReceived,
    BoardFunding,
    PlayerWages,
    StaffWages,
    AcademyUpkeep,
    FacilityUpkeep,
    MatchdayOps,
    TransferFees,
    AgentFees,
    SellOnPaid,
    LoanFeePaid,
    BonusPayouts,
    FfpLevy,
}

impl LedgerCategory {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MatchdayTickets => "matchday_tickets",
            Self::MatchdayMerch => "matchday_merch",
            Self::Broadcast => "broadcast",
            Self::SponsorshipUpfront => "sponsorship_upfront",
            Self::SponsorshipWeekly => "sponsorship_weekly",
            Self::SponsorshipBonus => "sponsorship_bonus",
            Self::PrizeMoney => "prize_money",
            Self::PlayerSales => "player_sales",
            Self::SellOnReceived => "sell_on_received",
            Self::LoanFeeReceived => "loan_fee_received",
            Self::BoardFunding => "board_funding",
            Self::PlayerWages => "player_wages",
            Self::StaffWages => "staff_wages",
            Self::AcademyUpkeep => "academy_upkeep",
            Self::FacilityUpkeep => "facility_upkeep",
            Self::MatchdayOps => "matchday_ops",
            Self::TransferFees => "transfer_fees",
            Self::AgentFees => "agent_fees",
            Self::SellOnPaid => "sell_on_paid",
            Self::LoanFeePaid => "loan_fee_paid",
            Self::BonusPayouts => "bonus_payouts",
            Self::FfpLevy => "ffp_levy",
        }
    }

    #[must_use]
    pub const fn is_football_revenue(self) -> bool {
        matches!(
            self,
            Self::MatchdayTickets
                | Self::MatchdayMerch
                | Self::Broadcast
                | Self::SponsorshipUpfront
                | Self::SponsorshipWeekly
                | Self::SponsorshipBonus
                | Self::PrizeMoney
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(i64)]
pub enum TransferOfferStatus {
    Pending = 1,
    Accepted = 2,
    Rejected = 3,
    Countered = 4,
    ContractNegotiation = 5,
    PlayerAccepted = 6,
    PlayerRejected = 7,
    Completed = 8,
    AwaitingWindow = 9,
    Cancelled = 10,
    Failed = 11,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(i64)]
pub enum AwardType {
    TopScorer = 1,
    Mvp = 2,
    YoungPlayer = 3,
    BallonDor = 4,
    GoldenGlove = 5,
    TeamOfTheYear = 6,
    ManagerOfTheYear = 7,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(i64)]
pub enum OffseasonStep {
    Summary = 0,
    Contracts = 1,
    Development = 2,
    Transfers = 3,
    Preseason = 4,
    Board = 5,
    NewSeason = 6,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StaffRole {
    HeadPhysio,
    TacticalAnalyst,
    HeadScout,
}

impl StaffRole {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HeadPhysio => "head_physio",
            Self::TacticalAnalyst => "tactical_analyst",
            Self::HeadScout => "head_scout",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InternationalWindowType {
    FifaWindow,
    Qualifiers,
    FinalTournament,
    Friendly,
}

impl InternationalWindowType {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FifaWindow => "fifaWindow",
            Self::Qualifiers => "qualifiers",
            Self::FinalTournament => "finalTournament",
            Self::Friendly => "friendly",
        }
    }
}
