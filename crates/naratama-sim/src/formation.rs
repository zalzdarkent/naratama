use std::collections::HashSet;

use naratama_model::{Player, PlayerId, PlayerStatus, PositionGroup, TeamId};
use serde::{Deserialize, Serialize};

/// Tactical shape controlling how many outfield players fill each line (plus 1 GK).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Formation {
    pub name: &'static str,
    pub defenders: usize,
    pub midfielders: usize,
    pub forwards: usize,
}

impl Serialize for Formation {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.name)
    }
}

impl<'de> Deserialize<'de> for Formation {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Ok(Self::parse(Some(&s)))
    }
}

impl Formation {
    pub const F442: Self = Self::new("4-4-2", 4, 4, 2);
    pub const F433: Self = Self::new("4-3-3", 4, 3, 3);
    pub const F4231: Self = Self::new("4-2-3-1", 4, 5, 1);
    pub const F352: Self = Self::new("3-5-2", 3, 5, 2);
    pub const F532: Self = Self::new("5-3-2", 5, 3, 2);
    pub const F343: Self = Self::new("3-4-3", 3, 4, 3);

    pub const ALL: [Self; 6] = [
        Self::F442,
        Self::F433,
        Self::F4231,
        Self::F352,
        Self::F532,
        Self::F343,
    ];

    #[must_use]
    pub const fn new(
        name: &'static str,
        defenders: usize,
        midfielders: usize,
        forwards: usize,
    ) -> Self {
        Self {
            name,
            defenders,
            midfielders,
            forwards,
        }
    }

    #[must_use]
    pub const fn outfielders(self) -> usize {
        self.defenders + self.midfielders + self.forwards
    }

    /// Parses a stored formation name, falling back to `4-4-2`.
    #[must_use]
    pub fn parse(name: Option<&str>) -> Self {
        if let Some(raw) = name {
            for f in Self::ALL {
                if f.name == raw {
                    return f;
                }
            }
        }
        Self::F442
    }
}

impl Default for Formation {
    fn default() -> Self {
        Self::F442
    }
}

/// Selected starting XI, bench (up to 7), and unavailable player IDs.
#[derive(Debug, Clone)]
pub struct Lineup {
    pub starters: Vec<Player>,
    pub substitutes: Vec<Player>,
    pub unavailable_ids: HashSet<PlayerId>,
}

impl Lineup {
    pub const NEED_GK: usize = 1;
    pub const DEFAULT_FITNESS_FLOOR: i32 = 35;
    pub const MAX_BENCH: usize = 7;

    /// Canonical 4-group tactical mapping (`naratama-model` single source of truth).
    #[must_use]
    pub const fn group_of(player: &Player) -> PositionGroup {
        PositionGroup::from_position_id(player.position_id)
    }

    /// Selection score combining base power and fitness.
    #[must_use]
    pub fn effective(player: &Player) -> f64 {
        let fit = f64::from(player.fitness.clamp(0, 100)) / 100.0;
        f64::from(player.power) * (0.70 + 0.30 * fit)
    }

    /// Builds the best available XI for `formation` plus a substitution bench.
    #[must_use]
    pub fn best(
        squad: &[Player],
        formation: Formation,
        fitness_floor: i32,
        banned_ids: &HashSet<PlayerId>,
        preferred_ids: &[PlayerId],
    ) -> Self {
        let mut unavailable = HashSet::new();
        let mut eligible = Vec::with_capacity(squad.len());

        for p in squad {
            if p.status != PlayerStatus::Active {
                continue;
            }
            let injured = p.fitness < fitness_floor && p.position_id != 1;
            let banned = banned_ids.contains(&p.player_id);
            if injured || banned {
                unavailable.insert(p.player_id);
                continue;
            }
            eligible.push(p.clone());
        }

        let mut sorted = eligible.clone();
        sorted.sort_by(|a, b| Self::effective(b).total_cmp(&Self::effective(a)));

        if !preferred_ids.is_empty() {
            return Self::with_preferred(preferred_ids, &eligible, sorted, formation, unavailable);
        }

        let need = [
            Self::NEED_GK,
            formation.defenders,
            formation.midfielders,
            formation.forwards,
        ];
        let mut chosen: [Vec<Player>; 4] = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
        let mut unassigned = Vec::new();

        // Pass 1: fill each player's natural position group.
        for p in sorted {
            let idx = group_index(Self::group_of(&p));
            if chosen[idx].len() < need[idx] {
                chosen[idx].push(p);
            } else {
                unassigned.push(p);
            }
        }

        // Pass 2: cross-fill outfield deficits from neighboring lines (GK never cross-fills).
        for g in 1..4 {
            while chosen[g].len() < need[g] {
                let mut best_pos: Option<usize> = None;
                let mut best_dist = usize::MAX;
                for (i, p) in unassigned.iter().enumerate() {
                    let pg = group_index(Self::group_of(p));
                    if !LINE_ADJACENCY[g].contains(&pg) {
                        continue;
                    }
                    let dist = pg.abs_diff(g);
                    if dist < best_dist {
                        best_dist = dist;
                        best_pos = Some(i);
                    }
                }
                let Some(idx) = best_pos else { break };
                chosen[g].push(unassigned.remove(idx));
            }
        }

        // Pass 3: emergency backfill with any remaining outfielder.
        let total_chosen: usize = chosen.iter().map(Vec::len).sum();
        if total_chosen < 11 {
            for g in 1..4 {
                while chosen[g].len() < need[g] {
                    let Some(idx) = unassigned
                        .iter()
                        .position(|p| Self::group_of(p) != PositionGroup::Goalkeeper)
                    else {
                        break;
                    };
                    chosen[g].push(unassigned.remove(idx));
                }
            }
        }

        sort_bench(&mut unassigned);
        let substitutes = unassigned.into_iter().take(Self::MAX_BENCH).collect();
        let mut starters = Vec::with_capacity(11);
        for line in chosen {
            starters.extend(line);
        }

        Self {
            starters,
            substitutes,
            unavailable_ids: unavailable,
        }
    }

    fn with_preferred(
        preferred_ids: &[PlayerId],
        eligible: &[Player],
        sorted: Vec<Player>,
        formation: Formation,
        unavailable_ids: HashSet<PlayerId>,
    ) -> Self {
        let mut used = HashSet::new();
        let mut starters = Vec::with_capacity(11);

        for &id in preferred_ids {
            if starters.len() >= 11 {
                break;
            }
            if used.contains(&id) {
                continue;
            }
            if let Some(p) = eligible.iter().find(|p| p.player_id == id) {
                starters.push(p.clone());
                used.insert(id);
            }
        }

        let need = [
            Self::NEED_GK,
            formation.defenders,
            formation.midfielders,
            formation.forwards,
        ];
        let mut remaining: Vec<Player> = sorted
            .into_iter()
            .filter(|p| !used.contains(&p.player_id))
            .collect();

        let take_group = |rem: &mut Vec<Player>, target_idx: usize| -> Option<Player> {
            let pos = rem
                .iter()
                .position(|p| group_index(Self::group_of(p)) == target_idx)?;
            Some(rem.remove(pos))
        };

        let count_group = |st: &[Player], target_idx: usize| -> usize {
            st.iter()
                .filter(|p| group_index(Self::group_of(p)) == target_idx)
                .count()
        };

        // Ensure a goalkeeper is fielded when available.
        if count_group(&starters, 0) == 0 {
            if let Some(gk) = take_group(&mut remaining, 0) {
                if starters.len() >= 11 {
                    starters.pop();
                }
                starters.push(gk);
            }
        }

        for (g, &quota) in need.iter().enumerate() {
            while starters.len() < 11 && count_group(&starters, g) < quota {
                let Some(p) = take_group(&mut remaining, g) else {
                    break;
                };
                starters.push(p);
            }
        }

        if starters.len() < 11 {
            for (g, &quota) in need.iter().enumerate() {
                while starters.len() < 11 && count_group(&starters, g) < quota {
                    let mut picked = None;
                    for &from in LINE_ADJACENCY[g] {
                        if let Some(p) = take_group(&mut remaining, from) {
                            picked = Some(p);
                            break;
                        }
                    }
                    let Some(p) = picked else { break };
                    starters.push(p);
                }
            }
        }

        while starters.len() < 11 && !remaining.is_empty() {
            starters.push(remaining.remove(0));
        }

        sort_bench(&mut remaining);
        let substitutes = remaining.into_iter().take(Self::MAX_BENCH).collect();

        Self {
            starters,
            substitutes,
            unavailable_ids,
        }
    }

    #[must_use]
    pub fn attack_on(starters: &[Player]) -> f64 {
        6.0 * Self::line_mean(starters, PositionGroup::Attacker)
            + 2.4 * Self::line_mean(starters, PositionGroup::Midfielder)
            + Self::line_mean(starters, PositionGroup::Defender)
    }

    #[must_use]
    pub fn midfield_on(starters: &[Player]) -> f64 {
        6.0 * Self::line_mean(starters, PositionGroup::Midfielder)
            + Self::line_mean(starters, PositionGroup::Attacker)
            + Self::line_mean(starters, PositionGroup::Defender)
    }

    #[must_use]
    pub fn defense_on(starters: &[Player]) -> f64 {
        6.0 * Self::line_mean(starters, PositionGroup::Defender)
            + 2.4 * Self::line_mean(starters, PositionGroup::Midfielder)
            + 2.0 * Self::line_mean(starters, PositionGroup::Goalkeeper)
    }

    /// Selects the highest-scoring formation for an AI club, with a deterministic
    /// per-club tiebreaker so AI clubs show tactical variety.
    #[must_use]
    pub fn best_ai_formation(team_id: TeamId, squad: &[Player]) -> Formation {
        let empty_ban = HashSet::new();
        let mut best_form = Formation::F442;
        let mut best_score = f64::NEG_INFINITY;

        for (idx, &form) in Formation::ALL.iter().enumerate() {
            let lineup = Self::best(squad, form, Self::DEFAULT_FITNESS_FLOOR, &empty_ban, &[]);
            let tie =
                ((team_id.get().unsigned_abs() ^ ((idx as u64 + 1) * 31)) % 100) as f64 * 0.05;
            let score = Self::attack_on(&lineup.starters)
                + Self::midfield_on(&lineup.starters)
                + Self::defense_on(&lineup.starters)
                + tie;
            if score > best_score {
                best_score = score;
                best_form = form;
            }
        }
        best_form
    }

    fn overall_mean(starters: &[Player]) -> f64 {
        if starters.is_empty() {
            return 50.0;
        }
        let sum: f64 = starters.iter().map(|p| f64::from(p.power)).sum();
        sum / starters.len() as f64
    }

    fn line_mean(starters: &[Player], group: PositionGroup) -> f64 {
        let mut sum = 0.0;
        let mut count = 0usize;
        for p in starters {
            if Self::group_of(p) == group {
                sum += f64::from(p.power);
                count += 1;
            }
        }
        if count > 0 {
            return sum / count as f64;
        }
        if starters.is_empty() {
            return 50.0;
        }
        Self::overall_mean(starters) * 0.93
    }
}

const LINE_ADJACENCY: [&[usize]; 4] = [&[], &[2, 3], &[1, 3], &[2, 1]];

const fn group_index(group: PositionGroup) -> usize {
    match group {
        PositionGroup::Goalkeeper => 0,
        PositionGroup::Defender => 1,
        PositionGroup::Midfielder => 2,
        PositionGroup::Attacker => 3,
    }
}

fn sort_bench(players: &mut [Player]) {
    players.sort_by(|a, b| {
        let ga = usize::from(Lineup::group_of(a) == PositionGroup::Goalkeeper);
        let gb = usize::from(Lineup::group_of(b) == PositionGroup::Goalkeeper);
        ga.cmp(&gb)
            .then_with(|| Lineup::effective(b).total_cmp(&Lineup::effective(a)))
    });
}
