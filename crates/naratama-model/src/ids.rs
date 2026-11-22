use serde::{Deserialize, Serialize};
use std::fmt;

/// Canonical in-game currency: signed integer USD (negative cash allowed).
pub type Money = i64;

macro_rules! define_id {
    ($name:ident, $doc:expr) => {
        #[doc = $doc]
        #[derive(
            Debug,
            Clone,
            Copy,
            PartialEq,
            Eq,
            PartialOrd,
            Ord,
            Hash,
            Serialize,
            Deserialize,
            Default,
        )]
        #[serde(transparent)]
        pub struct $name(pub i64);

        impl $name {
            #[must_use]
            pub const fn get(self) -> i64 {
                self.0
            }
        }

        impl From<i64> for $name {
            fn from(value: i64) -> Self {
                Self(value)
            }
        }

        impl From<$name> for i64 {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

define_id!(
    CountryId,
    "Unique identifier for a country / national association."
);
define_id!(TeamId, "Unique identifier for a football club.");
define_id!(PlayerId, "Unique identifier for a player.");
define_id!(CoachId, "Unique identifier for a coach / manager.");
define_id!(StadiumId, "Unique identifier for a stadium.");
define_id!(
    TournamentId,
    "Unique identifier for a competition / tournament."
);
define_id!(SeasonId, "Unique identifier for a season.");
define_id!(
    MatchId,
    "Unique identifier for a club or national team match."
);
define_id!(
    OfferId,
    "Unique identifier for a transfer or manager offer."
);
define_id!(
    SponsorshipId,
    "Unique identifier for a club sponsorship deal."
);
define_id!(ScoutId, "Unique identifier for a regional scout.");
define_id!(
    StaffId,
    "Unique identifier for a coaching/support staff member."
);

/// Well-known tournament IDs matching `superkickoff_2025.db`.
pub mod tournament_ids {
    use super::TournamentId;

    pub const CHAMPIONS_LEAGUE: TournamentId = TournamentId(49);
    pub const CONCACAF_CHAMPIONS_CUP: TournamentId = TournamentId(50);
    pub const EUROPA_LEAGUE: TournamentId = TournamentId(51);
    pub const LIBERTADORES: TournamentId = TournamentId(52);
    pub const SUDAMERICANA: TournamentId = TournamentId(53);
    pub const UEFA_SUPER_CUP: TournamentId = TournamentId(54);
    pub const RECOPA_SUDAMERICANA: TournamentId = TournamentId(55);
    pub const INTERCONTINENTAL_CUP: TournamentId = TournamentId(56);
    pub const WORLD_CUP: TournamentId = TournamentId(57);
    pub const COPA_AMERICA: TournamentId = TournamentId(94);
    pub const EURO_CUP: TournamentId = TournamentId(95);
    pub const GOLD_CUP: TournamentId = TournamentId(96);
    pub const AFRICA_CUP: TournamentId = TournamentId(97);
    pub const ASIAN_CUP: TournamentId = TournamentId(98);
    pub const ACL_ELITE: TournamentId = TournamentId(99);
    pub const CAF_CHAMPIONS_LEAGUE: TournamentId = TournamentId(100);
    pub const CONFERENCE_LEAGUE: TournamentId = TournamentId(135);
    pub const LEAGUES_CUP: TournamentId = TournamentId(162);
    pub const CLUB_WORLD_CUP: TournamentId = TournamentId(163);
    pub const ACL_TWO: TournamentId = TournamentId(164);
    pub const INTL_FRIENDLY: TournamentId = TournamentId(900);
    pub const NATIONS_LEAGUE: TournamentId = TournamentId(901);
    pub const ASEAN_CHAMPIONSHIP: TournamentId = TournamentId(902);
    pub const FIFA_ASEAN_CUP: TournamentId = TournamentId(903);
    pub const WC_QUALIFIERS: TournamentId = TournamentId(904);
    pub const ASIAN_CUP_QUALIFIERS: TournamentId = TournamentId(905);
    pub const EURO_QUALIFIERS: TournamentId = TournamentId(906);
    pub const AFC_CHALLENGE_LEAGUE: TournamentId = TournamentId(1650);
    pub const AFC_SUPER_CUP: TournamentId = TournamentId(1651);
    pub const PRE_SEASON_FRIENDLY: TournamentId = TournamentId(9999);
}
