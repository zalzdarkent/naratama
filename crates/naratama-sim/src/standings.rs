use std::cmp::Ordering;
use std::collections::HashMap;
use std::hash::BuildHasher;

use naratama_model::{FixtureMatch, MatchStatus, StandingRow, TeamId};

/// Computes sorted league/group standings from completed matches.
/// Tiebreakers: `points DESC -> goal_difference DESC -> goals_for DESC -> name ASC`.
#[must_use]
pub fn compute_standings<S: BuildHasher>(
    club_ids: &[TeamId],
    team_names: &HashMap<TeamId, String, S>,
    matches: &[FixtureMatch],
) -> Vec<StandingRow> {
    let mut table: HashMap<TeamId, StandingRow> = club_ids
        .iter()
        .map(|&id| {
            (
                id,
                StandingRow {
                    team_id: id,
                    team_name: team_names
                        .get(&id)
                        .cloned()
                        .unwrap_or_else(|| format!("Club {id}")),
                    played: 0,
                    wins: 0,
                    draws: 0,
                    losses: 0,
                    goals_for: 0,
                    goals_against: 0,
                    points: 0,
                },
            )
        })
        .collect();

    for m in matches {
        if m.status != MatchStatus::Finished {
            continue;
        }
        let (Some(hg), Some(ag)) = (m.home_goals, m.away_goals) else {
            continue;
        };

        {
            let h = ensure_entry(&mut table, team_names, m.home_team_id);
            h.played += 1;
            h.goals_for += hg;
            h.goals_against += ag;
            match hg.cmp(&ag) {
                Ordering::Greater => {
                    h.wins += 1;
                    h.points += 3;
                }
                Ordering::Equal => {
                    h.draws += 1;
                    h.points += 1;
                }
                Ordering::Less => {
                    h.losses += 1;
                }
            }
        }
        {
            let a = ensure_entry(&mut table, team_names, m.away_team_id);
            a.played += 1;
            a.goals_for += ag;
            a.goals_against += hg;
            match ag.cmp(&hg) {
                Ordering::Greater => {
                    a.wins += 1;
                    a.points += 3;
                }
                Ordering::Equal => {
                    a.draws += 1;
                    a.points += 1;
                }
                Ordering::Less => {
                    a.losses += 1;
                }
            }
        }
    }

    let mut rows: Vec<StandingRow> = table.into_values().collect();
    rows.sort_by(|a, b| {
        b.points
            .cmp(&a.points)
            .then_with(|| b.goal_difference().cmp(&a.goal_difference()))
            .then_with(|| b.goals_for.cmp(&a.goals_for))
            .then_with(|| {
                a.team_name
                    .to_ascii_lowercase()
                    .cmp(&b.team_name.to_ascii_lowercase())
            })
    });
    rows
}

fn ensure_entry<'a, S: BuildHasher>(
    tbl: &'a mut HashMap<TeamId, StandingRow>,
    team_names: &HashMap<TeamId, String, S>,
    id: TeamId,
) -> &'a mut StandingRow {
    let name = team_names
        .get(&id)
        .cloned()
        .unwrap_or_else(|| format!("Club {id}"));
    tbl.entry(id).or_insert(StandingRow {
        team_id: id,
        team_name: name,
        played: 0,
        wins: 0,
        draws: 0,
        losses: 0,
        goals_for: 0,
        goals_against: 0,
        points: 0,
    })
}
