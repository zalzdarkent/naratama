use std::collections::{BTreeSet, HashMap, HashSet};

use chrono::{Duration, NaiveDate};
use naratama_db::db_err;
use naratama_db::intl_repo::{
    CountryMatchPersistPayload, CountryMatchPlayerInsert, CountryMatchRow, IntlRepo,
    NewCountryMatchRow,
};
use naratama_model::{
    tournament_ids, Confederation, Country, CountryId, InternationalWindow, MatchId, MatchStatus,
    NaratamaResult, Player, SeasonId, StadiumId, TeamId, TournamentId,
};
use naratama_sim::{
    calculate_fifa_elo, FifaEloInput, Formation, MatchEngine, MatchEventType, MatchInput,
    RoundRobinScheduler,
};
use rand::seq::SliceRandom;
use rand::Rng;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InternationalStandingRow {
    pub country_id: CountryId,
    pub country_name: String,
    pub played: i32,
    pub won: i32,
    pub drawn: i32,
    pub lost: i32,
    pub goals_for: i32,
    pub goals_against: i32,
    pub points: i32,
    pub group_id: Option<i32>,
}

impl InternationalStandingRow {
    #[must_use]
    pub const fn goal_difference(&self) -> i32 {
        self.goals_for - self.goals_against
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InternationalTournamentPodium {
    pub tournament_id: TournamentId,
    pub season_id: SeasonId,
    pub champion_country_id: CountryId,
    pub runner_up_country_id: CountryId,
    pub third_place_country_id: Option<CountryId>,
    pub champion_score: Option<i32>,
    pub runner_up_score: Option<i32>,
    pub penalty_champion_score: Option<i32>,
    pub penalty_runner_up_score: Option<i32>,
}

/// Simulates international fixtures using `MatchEngine`, updates FIFA Elo points, cap-ties, and injuries.
pub struct InternationalSimulationService;

impl InternationalSimulationService {
    #[allow(clippy::too_many_lines)]
    pub fn simulate_country_match(
        conn: &mut Connection,
        cm: &CountryMatchRow,
        rng: &mut impl Rng,
    ) -> NaratamaResult<()> {
        if cm.status == MatchStatus::Finished {
            return Ok(());
        }

        let home_country = IntlRepo::country_by_id(conn, cm.home_country_id)?;
        let away_country = IntlRepo::country_by_id(conn, cm.away_country_id)?;
        let home_power = home_country.as_ref().map_or(65, |c| c.power);
        let away_power = away_country.as_ref().map_or(65, |c| c.power);
        let home_elo_before = home_country.as_ref().map_or(1000.0, |c| c.points);
        let away_elo_before = away_country.as_ref().map_or(1000.0, |c| c.points);

        let home_squad = country_match_squad(conn, cm.season_id, cm.home_country_id, home_power)?;
        let away_squad = country_match_squad(conn, cm.season_id, cm.away_country_id, away_power)?;

        let home_team_synth = TeamId(cm.home_country_id.0);
        let away_team_synth = TeamId(cm.away_country_id.0);

        let input = MatchInput {
            home_squad,
            away_squad,
            home_team_id: home_team_synth,
            away_team_id: away_team_synth,
            capacity_general: 35_000,
            capacity_stand: 20_000,
            capacity_terrace: 9_000,
            capacity_boxes: 1_000,
            home_power,
            away_power,
            home_formation: Formation::F442,
            away_formation: Formation::F442,
            referee_temper: 2,
            neutral_venue: true,
            extra_time: false,
            ..MatchInput::default()
        };

        let empty_banned = HashSet::new();
        let sim = MatchEngine::simulate(rng, &input, true, &empty_banned);

        let mut pen_h = None;
        let mut pen_a = None;
        let mut has_pens = false;
        let mut home_won_pens = false;

        let winner_id = if sim.home_goals > sim.away_goals {
            Some(cm.home_country_id)
        } else if sim.away_goals > sim.home_goals {
            Some(cm.away_country_id)
        } else if cm.phase > 0 && cm.leg == 1 && cm.tournament_id != tournament_ids::INTL_FRIENDLY {
            let shootout = MatchEngine::penalty_shootout(rng, &input);
            pen_h = Some(shootout.home_score);
            pen_a = Some(shootout.away_score);
            has_pens = true;
            home_won_pens = shootout.home_score >= shootout.away_score;
            Some(if home_won_pens {
                cm.home_country_id
            } else {
                cm.away_country_id
            })
        } else {
            None
        };

        let match_date = cm
            .date
            .unwrap_or_else(|| NaiveDate::from_ymd_opt(2025, 10, 10).unwrap_or(NaiveDate::MIN));
        let mut injury_return_dates = Vec::new();
        let mut seen_inj = HashSet::new();
        for ev in &sim.events {
            if ev.event_type == MatchEventType::Injury {
                if let Some(pid) = ev.player_id {
                    if seen_inj.insert(pid) {
                        let days = rng.gen_range(7..=27);
                        injury_return_dates.push((pid, match_date + Duration::days(days)));
                    }
                }
            }
        }

        let player_rows: Vec<CountryMatchPlayerInsert> = sim
            .player_rows
            .iter()
            .filter(|r| r.player_id.0 > 0)
            .map(|r| {
                let c_id = if r.team_id == home_team_synth {
                    cm.home_country_id
                } else {
                    cm.away_country_id
                };
                let fatigue_drop = if r.starter {
                    rng.gen_range(8..=12)
                } else if r.minutes > 0 {
                    rng.gen_range(4..=6)
                } else {
                    0
                };
                CountryMatchPlayerInsert {
                    player_id: r.player_id,
                    country_id: c_id,
                    starter: r.starter,
                    minutes: r.minutes,
                    points: r.points,
                    rating: r.rating,
                    mvp: r.mvp,
                    goals: r.goals,
                    yellow_cards: r.yellow_cards,
                    red_cards: r.red_cards,
                    assists: r.assists,
                    fatigue_drop,
                }
            })
            .collect();

        let elo = calculate_fifa_elo(FifaEloInput {
            home_points: home_elo_before,
            away_points: away_elo_before,
            tournament_id: cm.tournament_id,
            phase: cm.phase,
            home_goals: sim.home_goals,
            away_goals: sim.away_goals,
            has_penalties: has_pens,
            home_won_penalties: home_won_pens,
        });

        let total_att = sim.attendance_general
            + sim.attendance_stand
            + sim.attendance_terrace
            + sim.attendance_boxes;

        IntlRepo::save_country_match_result(
            conn,
            &CountryMatchPersistPayload {
                match_id: cm.match_id,
                season_id: cm.season_id,
                tournament_id: cm.tournament_id,
                home_country_id: cm.home_country_id,
                away_country_id: cm.away_country_id,
                home_goals: sim.home_goals,
                away_goals: sim.away_goals,
                winner_id,
                penalty_home_goals: pen_h,
                penalty_away_goals: pen_a,
                attendance_total: total_att,
                revenue: sim.revenue,
                is_competitive: cm.tournament_id != tournament_ids::INTL_FRIENDLY,
                player_rows,
                injury_return_dates,
                new_home_points: elo.new_home_points,
                new_away_points: elo.new_away_points,
            },
        )
    }

    /// Simulates all pending international matches on or before `date` and iterates tournament knockout advancement.
    pub fn simulate_pending_matches_through_date(
        conn: &mut Connection,
        season_id: SeasonId,
        date: NaiveDate,
        rng: &mut impl Rng,
    ) -> NaratamaResult<usize> {
        let mut total_simulated = 0_usize;
        for _step in 0..7 {
            let pending = IntlRepo::pending_country_matches_through_date(conn, season_id, date)?;
            let count = pending.len();
            for cm in pending {
                Self::simulate_country_match(conn, &cm, rng)?;
                total_simulated += 1;
            }
            let advanced =
                InternationalTournamentService::check_and_advance_international_knockouts(
                    conn, season_id, date,
                )?;
            if count == 0 && !advanced {
                break;
            }
        }
        Ok(total_simulated)
    }
}

/// Schedules & advances the 4-year international tournament cycle (WC, EURO, Asian Cup, Nations League, FIFA ASEAN Cup, Friendlies).
pub struct InternationalTournamentService;

impl InternationalTournamentService {
    pub const ASEAN_TEAMS_16: [i64; 16] = [
        94,  // Indonesia
        223, // Thailand
        243, // Vietnam
        146, // Malaysia
        210, // Singapore
        68,  // Philippines
        26,  // Myanmar
        36,  // Cambodia
        133, // Laos
        31,  // Brunei
        228, // Timor Leste
        14,  // Australia (guest)
        93,  // India (guest)
        41,  // China (guest)
        91,  // Hong Kong (guest)
        126, // Jordan (guest)
    ];

    pub const ASEAN_CHAMP_10: [i64; 10] = [94, 223, 243, 146, 210, 68, 26, 36, 133, 31];

    /// Schedules the full seasonal international calendar following `(start_year + 1) % 4`:
    /// - `cycleYear == 1`: Nations League (`901`) + Friendlies (`900`)
    /// - `cycleYear == 2`: Friendlies (`900`) + FIFA World Cup (`57`, 48 teams)
    /// - `cycleYear == 3`: Nations League (`901`) + Friendlies (`900`) + FIFA ASEAN Cup (`903`, 16 teams)
    /// - `cycleYear == 0`: Friendlies (`900`) + AFC Asian Cup (`98`, 24) + UEFA EURO (`95`, 24)
    #[allow(clippy::too_many_lines)]
    pub fn schedule_cycle_for_season(
        conn: &Connection,
        season_id: SeasonId,
        start_year: i32,
        user_country_id: Option<CountryId>,
        windows: &[InternationalWindow],
        rng: &mut impl Rng,
    ) -> NaratamaResult<usize> {
        let cycle_year = (start_year + 1).rem_euclid(4);
        let oct = windows
            .iter()
            .find(|w| w.window_id.ends_with("_oct"))
            .cloned();
        let mar = windows
            .iter()
            .find(|w| w.window_id.ends_with("_mar"))
            .cloned();
        let summer = windows
            .iter()
            .find(|w| w.window_id.ends_with("_summer"))
            .cloned();

        let mut total = 0_usize;
        match cycle_year {
            1 => {
                if let (Some(w1), Some(w2)) = (&oct, &mar) {
                    total += Self::schedule_nations_league(
                        conn,
                        season_id,
                        w1,
                        w2,
                        user_country_id,
                        rng,
                    )?;
                }
                for w in [oct, mar, summer].into_iter().flatten() {
                    total += Self::schedule_friendlies_for_window(
                        conn,
                        season_id,
                        &w,
                        user_country_id,
                        rng,
                    )?;
                }
            }
            2 => {
                if let Some(w) = &oct {
                    total += Self::schedule_friendlies_for_window(
                        conn,
                        season_id,
                        w,
                        user_country_id,
                        rng,
                    )?;
                }
                if let Some(w) = &mar {
                    total += Self::schedule_friendlies_for_window(
                        conn,
                        season_id,
                        w,
                        user_country_id,
                        rng,
                    )?;
                }
                if let Some(w) = &summer {
                    total += Self::schedule_world_cup_tournament(
                        conn,
                        season_id,
                        w,
                        user_country_id,
                        rng,
                    )?;
                }
            }
            3 => {
                if let (Some(w1), Some(w2)) = (&oct, &mar) {
                    total += Self::schedule_nations_league(
                        conn,
                        season_id,
                        w1,
                        w2,
                        user_country_id,
                        rng,
                    )?;
                }
                for w in [oct, mar].into_iter().flatten() {
                    total += Self::schedule_friendlies_for_window(
                        conn,
                        season_id,
                        &w,
                        user_country_id,
                        rng,
                    )?;
                }
                if let Some(w) = &summer {
                    total += Self::schedule_fifa_asean_cup(conn, season_id, w, rng)?;
                }
            }
            _ => {
                // cycle_year == 0: continental championships (EURO, Asian Cup, Copa América, AFCON, Gold Cup)
                for w in [oct, mar].into_iter().flatten() {
                    total += Self::schedule_friendlies_for_window(
                        conn,
                        season_id,
                        &w,
                        user_country_id,
                        rng,
                    )?;
                }
                if let Some(w) = &summer {
                    for (tid, conf, teams, groups) in [
                        (tournament_ids::EURO_CUP, Confederation::Uefa, 24, 6),
                        (tournament_ids::ASIAN_CUP, Confederation::Afc, 24, 6),
                        (tournament_ids::COPA_AMERICA, Confederation::Conmebol, 16, 4),
                        (tournament_ids::AFRICA_CUP, Confederation::Caf, 24, 6),
                        (tournament_ids::GOLD_CUP, Confederation::Concacaf, 16, 4),
                    ] {
                        total += Self::schedule_continental_championship(
                            conn,
                            tid,
                            conf,
                            season_id,
                            w,
                            teams,
                            groups,
                            user_country_id,
                            rng,
                        )?;
                    }
                }
            }
        }
        Ok(total)
    }

    pub fn standings(
        conn: &Connection,
        tournament_id: TournamentId,
        season_id: SeasonId,
        group_id: Option<i32>,
    ) -> NaratamaResult<Vec<InternationalStandingRow>> {
        let matches: Vec<CountryMatchRow> =
            IntlRepo::country_matches_for_tournament(conn, season_id, tournament_id)?
                .into_iter()
                .filter(|m| m.phase == 0 && (group_id.is_none() || m.group_id == group_id))
                .collect();
        if matches.is_empty() {
            return Ok(Vec::new());
        }

        let name_map = load_country_names(conn)?;
        let mut table: HashMap<CountryId, InternationalStandingRow> = HashMap::new();
        for m in &matches {
            table
                .entry(m.home_country_id)
                .or_insert_with(|| InternationalStandingRow {
                    country_id: m.home_country_id,
                    country_name: name_map
                        .get(&m.home_country_id)
                        .cloned()
                        .unwrap_or_else(|| format!("Country {}", m.home_country_id.0)),
                    played: 0,
                    won: 0,
                    drawn: 0,
                    lost: 0,
                    goals_for: 0,
                    goals_against: 0,
                    points: 0,
                    group_id: m.group_id,
                });
            table
                .entry(m.away_country_id)
                .or_insert_with(|| InternationalStandingRow {
                    country_id: m.away_country_id,
                    country_name: name_map
                        .get(&m.away_country_id)
                        .cloned()
                        .unwrap_or_else(|| format!("Country {}", m.away_country_id.0)),
                    played: 0,
                    won: 0,
                    drawn: 0,
                    lost: 0,
                    goals_for: 0,
                    goals_against: 0,
                    points: 0,
                    group_id: m.group_id,
                });

            if m.status == MatchStatus::Finished {
                if let (Some(hg), Some(ag)) = (m.home_goals, m.away_goals) {
                    if let Some(h) = table.get_mut(&m.home_country_id) {
                        h.played += 1;
                        h.goals_for += hg;
                        h.goals_against += ag;
                        match hg.cmp(&ag) {
                            std::cmp::Ordering::Greater => {
                                h.won += 1;
                                h.points += 3;
                            }
                            std::cmp::Ordering::Equal => {
                                h.drawn += 1;
                                h.points += 1;
                            }
                            std::cmp::Ordering::Less => h.lost += 1,
                        }
                    }
                    if let Some(a) = table.get_mut(&m.away_country_id) {
                        a.played += 1;
                        a.goals_for += ag;
                        a.goals_against += hg;
                        match ag.cmp(&hg) {
                            std::cmp::Ordering::Greater => {
                                a.won += 1;
                                a.points += 3;
                            }
                            std::cmp::Ordering::Equal => {
                                a.drawn += 1;
                                a.points += 1;
                            }
                            std::cmp::Ordering::Less => a.lost += 1,
                        }
                    }
                }
            }
        }

        let mut rows: Vec<InternationalStandingRow> = table.into_values().collect();
        rows.sort_by(|a, b| {
            b.points
                .cmp(&a.points)
                .then_with(|| b.goal_difference().cmp(&a.goal_difference()))
                .then_with(|| b.goals_for.cmp(&a.goals_for))
                .then_with(|| a.country_name.cmp(&b.country_name))
        });
        Ok(rows)
    }

    pub fn best_third_placed_teams(
        conn: &Connection,
        tournament_id: TournamentId,
        season_id: SeasonId,
        group_count: i32,
        advance_count: usize,
    ) -> NaratamaResult<Vec<InternationalStandingRow>> {
        let mut thirds = Vec::new();
        for gid in 1..=group_count {
            let st = Self::standings(conn, tournament_id, season_id, Some(gid))?;
            if let Some(third) = st.get(2) {
                thirds.push(third.clone());
            }
        }
        thirds.sort_by(|a, b| {
            b.points
                .cmp(&a.points)
                .then_with(|| b.goal_difference().cmp(&a.goal_difference()))
                .then_with(|| b.goals_for.cmp(&a.goals_for))
                .then_with(|| a.country_name.cmp(&b.country_name))
        });
        thirds.truncate(advance_count);
        Ok(thirds)
    }

    /// Pot-seeded group draw + Circle Method round-robin scheduling across `match_dates`.
    #[allow(clippy::too_many_arguments)]
    pub fn schedule_group_stage(
        conn: &Connection,
        tournament_id: TournamentId,
        season_id: SeasonId,
        participants: &[CountryId],
        group_count: usize,
        match_dates: &[NaiveDate],
        window_id: Option<&str>,
        rng: &mut impl Rng,
    ) -> NaratamaResult<usize> {
        if participants.len() < group_count * 2 || group_count == 0 || match_dates.is_empty() {
            return Ok(0);
        }

        let mut groups: Vec<Vec<CountryId>> = vec![Vec::new(); group_count];
        let mut pot_start = 0_usize;
        while pot_start < participants.len() {
            let pot_end = (pot_start + group_count).min(participants.len());
            let mut pot = participants[pot_start..pot_end].to_vec();
            pot.shuffle(rng);
            for (g_idx, cid) in pot.into_iter().enumerate() {
                groups[g_idx].push(cid);
            }
            pot_start += group_count;
        }

        let mut rows = Vec::new();
        for (g_idx, g_teams) in groups.into_iter().enumerate() {
            let gid = i32::try_from(g_idx + 1).unwrap_or(1);
            let team_ids: Vec<TeamId> = g_teams.into_iter().map(|c| TeamId(c.0)).collect();
            let slots = RoundRobinScheduler::schedule(&team_ids, false, true, None);
            for slot in slots {
                let r_idx = usize::try_from((slot.round - 1).max(0)).unwrap_or(0);
                let date = match_dates[r_idx.min(match_dates.len() - 1)];
                rows.push(NewCountryMatchRow {
                    tournament_id,
                    season_id,
                    stadium_id: StadiumId(1),
                    home_country_id: CountryId(slot.home.0),
                    away_country_id: CountryId(slot.away.0),
                    phase: 0,
                    round: slot.round,
                    leg: 1,
                    date,
                    group_id: Some(gid),
                    window_id: window_id.map(ToString::to_string),
                });
            }
        }
        IntlRepo::insert_country_matches(conn, &rows)
    }

    pub fn schedule_world_cup_tournament(
        conn: &Connection,
        season_id: SeasonId,
        window: &InternationalWindow,
        user_country_id: Option<CountryId>,
        rng: &mut impl Rng,
    ) -> NaratamaResult<usize> {
        let mut all = IntlRepo::fifa_rankings(conn, None)?;
        all.sort_by_key(|a| std::cmp::Reverse(a.power));
        let participants = pick_seeded_participants(&all, 48, user_country_id);
        let dates = [
            window.start_date,
            window.start_date + Duration::days(5),
            window.start_date + Duration::days(10),
        ];
        Self::schedule_group_stage(
            conn,
            tournament_ids::WORLD_CUP,
            season_id,
            &participants,
            12,
            &dates,
            Some(&window.window_id),
            rng,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn schedule_continental_championship(
        conn: &Connection,
        tournament_id: TournamentId,
        confederation: Confederation,
        season_id: SeasonId,
        window: &InternationalWindow,
        target_teams: usize,
        group_count: usize,
        user_country_id: Option<CountryId>,
        rng: &mut impl Rng,
    ) -> NaratamaResult<usize> {
        let mut nations = IntlRepo::fifa_rankings(conn, Some(confederation))?;
        nations.sort_by_key(|a| std::cmp::Reverse(a.power));
        let participants = pick_seeded_participants(&nations, target_teams, user_country_id);
        let dates = [
            window.start_date,
            window.start_date + Duration::days(4),
            window.start_date + Duration::days(8),
        ];
        Self::schedule_group_stage(
            conn,
            tournament_id,
            season_id,
            &participants,
            group_count,
            &dates,
            Some(&window.window_id),
            rng,
        )
    }

    pub fn schedule_nations_league(
        conn: &Connection,
        season_id: SeasonId,
        w1: &InternationalWindow,
        w2: &InternationalWindow,
        user_country_id: Option<CountryId>,
        rng: &mut impl Rng,
    ) -> NaratamaResult<usize> {
        let mut uefa = IntlRepo::fifa_rankings(conn, Some(Confederation::Uefa))?;
        uefa.sort_by_key(|a| std::cmp::Reverse(a.power));
        let participants = pick_seeded_participants(&uefa, 16, user_country_id);
        let dates = [
            w1.start_date + Duration::days(2),
            w1.end_date - Duration::days(1),
            w2.start_date + Duration::days(2),
        ];
        Self::schedule_group_stage(
            conn,
            tournament_ids::NATIONS_LEAGUE,
            season_id,
            &participants,
            4,
            &dates,
            Some(&w1.window_id),
            rng,
        )
    }

    pub fn schedule_fifa_asean_cup(
        conn: &Connection,
        season_id: SeasonId,
        window: &InternationalWindow,
        rng: &mut impl Rng,
    ) -> NaratamaResult<usize> {
        let existing = IntlRepo::country_matches_for_tournament(
            conn,
            season_id,
            tournament_ids::FIFA_ASEAN_CUP,
        )?;
        if !existing.is_empty() {
            return Ok(0);
        }
        let participants: Vec<CountryId> =
            Self::ASEAN_TEAMS_16.into_iter().map(CountryId).collect();
        let dates = [
            window.start_date,
            window.start_date + Duration::days(3),
            window.start_date + Duration::days(6),
        ];
        Self::schedule_group_stage(
            conn,
            tournament_ids::FIFA_ASEAN_CUP,
            season_id,
            &participants,
            4,
            &dates,
            Some(&window.window_id),
            rng,
        )
    }

    pub fn schedule_asean_championship(
        conn: &Connection,
        season_id: SeasonId,
        window: &InternationalWindow,
        rng: &mut impl Rng,
    ) -> NaratamaResult<usize> {
        let participants: Vec<CountryId> =
            Self::ASEAN_CHAMP_10.into_iter().map(CountryId).collect();
        let dates = [
            window.start_date,
            window.start_date + Duration::days(3),
            window.start_date + Duration::days(6),
            window.start_date + Duration::days(9),
            window.start_date + Duration::days(12),
        ];
        Self::schedule_group_stage(
            conn,
            tournament_ids::ASEAN_CHAMPIONSHIP,
            season_id,
            &participants,
            2,
            &dates,
            Some(&window.window_id),
            rng,
        )
    }

    pub fn schedule_friendlies_for_window(
        conn: &Connection,
        season_id: SeasonId,
        window: &InternationalWindow,
        user_country_id: Option<CountryId>,
        rng: &mut impl Rng,
    ) -> NaratamaResult<usize> {
        // Avoid nations already playing competitive matches in this window.
        let win_start = window.start_date.format("%Y-%m-%d").to_string();
        let win_end = window.end_date.format("%Y-%m-%d").to_string();
        let mut busy_stmt = conn
            .prepare(
                "SELECT home_country_id, away_country_id FROM country_matches
                 WHERE season_id = ?1 AND date >= ?2 AND date <= ?3",
            )
            .map_err(db_err)?;
        let mut busy = HashSet::new();
        let pairs: Vec<(i64, i64)> = busy_stmt
            .query_map(params![season_id.0, win_start, win_end], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        drop(busy_stmt);
        for (h, a) in pairs {
            busy.insert(CountryId(h));
            busy.insert(CountryId(a));
        }

        let mut all = IntlRepo::fifa_rankings(conn, None)?;
        all.sort_by_key(|a| std::cmp::Reverse(a.power));
        let free: Vec<Country> = all
            .into_iter()
            .filter(|c| c.points > 0.0 && !busy.contains(&c.country_id))
            .collect();
        let pool = pick_seeded_participants(&free, 32, user_country_id);
        if pool.len() < 4 {
            return Ok(0);
        }

        let md1 = window.start_date + Duration::days(2);
        let md2 = (window.end_date - Duration::days(1)).max(md1 + Duration::days(2));
        let mut rows = Vec::new();
        for &[h, a] in pool.as_chunks::<2>().0 {
            rows.push(NewCountryMatchRow {
                tournament_id: tournament_ids::INTL_FRIENDLY,
                season_id,
                stadium_id: StadiumId(1),
                home_country_id: h,
                away_country_id: a,
                phase: 0,
                round: 1,
                leg: 1,
                date: md1,
                group_id: None,
                window_id: Some(window.window_id.clone()),
            });
        }
        let mut shuffled = pool;
        shuffled.shuffle(rng);
        for &[h, a] in shuffled.as_chunks::<2>().0 {
            rows.push(NewCountryMatchRow {
                tournament_id: tournament_ids::INTL_FRIENDLY,
                season_id,
                stadium_id: StadiumId(1),
                home_country_id: a,
                away_country_id: h,
                phase: 0,
                round: 2,
                leg: 1,
                date: md2,
                group_id: None,
                window_id: Some(window.window_id.clone()),
            });
        }
        IntlRepo::insert_country_matches(conn, &rows)
    }

    #[allow(clippy::too_many_lines)]
    pub fn check_and_advance_international_knockouts(
        conn: &Connection,
        season_id: SeasonId,
        current_date: NaiveDate,
    ) -> NaratamaResult<bool> {
        let major = [
            tournament_ids::WORLD_CUP,
            tournament_ids::ASIAN_CUP,
            tournament_ids::EURO_CUP,
            tournament_ids::FIFA_ASEAN_CUP,
            tournament_ids::NATIONS_LEAGUE,
            tournament_ids::ASEAN_CHAMPIONSHIP,
            tournament_ids::COPA_AMERICA,
            tournament_ids::AFRICA_CUP,
            tournament_ids::GOLD_CUP,
        ];
        let mut did_advance = false;

        for tid in major {
            let all = IntlRepo::country_matches_for_tournament(conn, season_id, tid)?;
            if all.is_empty() {
                continue;
            }
            let group_matches: Vec<&CountryMatchRow> =
                all.iter().filter(|m| m.phase == 0).collect();
            let group_done = !group_matches.is_empty()
                && group_matches
                    .iter()
                    .all(|m| m.status == MatchStatus::Finished);
            let has_ko = all.iter().any(|m| m.phase >= 1);

            if group_done && !has_ko {
                let gids: Vec<i32> = group_matches
                    .iter()
                    .map(|m| m.group_id.unwrap_or(1))
                    .collect::<BTreeSet<i32>>()
                    .into_iter()
                    .collect();

                let mut winners = Vec::new();
                let mut runners_up = Vec::new();
                for &gid in &gids {
                    let st = Self::standings(conn, tid, season_id, Some(gid))?;
                    if let Some(w) = st.first() {
                        winners.push(w.country_id);
                    }
                    if let Some(ru) = st.get(1) {
                        runners_up.push(ru.country_id);
                    }
                }

                let best_thirds: Vec<CountryId> =
                    if tid == tournament_ids::WORLD_CUP && gids.len() == 12 {
                        Self::best_third_placed_teams(conn, tid, season_id, 12, 8)?
                            .into_iter()
                            .map(|r| r.country_id)
                            .collect()
                    } else if (tid == tournament_ids::ASIAN_CUP || tid == tournament_ids::EURO_CUP)
                        && gids.len() == 6
                    {
                        Self::best_third_placed_teams(conn, tid, season_id, 6, 4)?
                            .into_iter()
                            .map(|r| r.country_id)
                            .collect()
                    } else {
                        Vec::new()
                    };

                let bracket = build_cross_group_pairs(&winners, &runners_up, &best_thirds);
                if !bracket.is_empty() {
                    let max_group_date = group_matches
                        .iter()
                        .filter_map(|m| m.date)
                        .max()
                        .unwrap_or(current_date);
                    let ko_date = max_group_date + Duration::days(3);
                    let win_id = group_matches.first().and_then(|m| m.window_id.clone());
                    let new_rows: Vec<NewCountryMatchRow> = bracket
                        .into_iter()
                        .enumerate()
                        .map(|(i, (h, a))| NewCountryMatchRow {
                            tournament_id: tid,
                            season_id,
                            stadium_id: StadiumId(1),
                            home_country_id: h,
                            away_country_id: a,
                            phase: 1,
                            round: i32::try_from(i + 1).unwrap_or(1),
                            leg: 1,
                            date: ko_date,
                            group_id: None,
                            window_id: win_id.clone(),
                        })
                        .collect();
                    IntlRepo::insert_country_matches(conn, &new_rows)?;
                    did_advance = true;
                }
            } else if has_ko {
                let max_phase = all.iter().map(|m| m.phase).max().unwrap_or(1);
                let latest: Vec<&CountryMatchRow> =
                    all.iter().filter(|m| m.phase == max_phase).collect();
                let done =
                    !latest.is_empty() && latest.iter().all(|m| m.status == MatchStatus::Finished);
                // Final phase has 1 match (or Final round=1 + 3rd place round=2, i.e., after 2-match Semifinal!).
                if !done || latest.is_empty() {
                    continue;
                }
                let prev_phase_matches = all.iter().filter(|m| m.phase == max_phase - 1).count();
                let already_final = latest.len() == 1
                    || (latest.len() == 2 && prev_phase_matches == 2 && max_phase > 1);
                if already_final {
                    continue;
                }

                let max_prev_date = latest
                    .iter()
                    .filter_map(|m| m.date)
                    .max()
                    .unwrap_or(current_date);
                let next_date = max_prev_date + Duration::days(4);
                let win_id = latest.first().and_then(|m| m.window_id.clone());

                let mut next_rows = Vec::new();
                if latest.len() == 2 {
                    // Semifinals finished -> schedule 3rd-place playoff (round=2 on H-1) + Final (round=1)
                    let mut finalists = Vec::new();
                    let mut losers = Vec::new();
                    for m in &latest {
                        let w = m.winner_id.unwrap_or(m.home_country_id);
                        let l = if w == m.home_country_id {
                            m.away_country_id
                        } else {
                            m.home_country_id
                        };
                        finalists.push(w);
                        losers.push(l);
                    }
                    next_rows.push(NewCountryMatchRow {
                        tournament_id: tid,
                        season_id,
                        stadium_id: StadiumId(1),
                        home_country_id: losers[0],
                        away_country_id: losers[1],
                        phase: max_phase + 1,
                        round: 2,
                        leg: 1,
                        date: next_date - Duration::days(1),
                        group_id: None,
                        window_id: win_id.clone(),
                    });
                    next_rows.push(NewCountryMatchRow {
                        tournament_id: tid,
                        season_id,
                        stadium_id: StadiumId(1),
                        home_country_id: finalists[0],
                        away_country_id: finalists[1],
                        phase: max_phase + 1,
                        round: 1,
                        leg: 1,
                        date: next_date,
                        group_id: None,
                        window_id: win_id,
                    });
                } else {
                    let winners: Vec<CountryId> = latest
                        .iter()
                        .map(|m| m.winner_id.unwrap_or(m.home_country_id))
                        .collect();
                    for (idx, &[h, a]) in winners.as_chunks::<2>().0.iter().enumerate() {
                        next_rows.push(NewCountryMatchRow {
                            tournament_id: tid,
                            season_id,
                            stadium_id: StadiumId(1),
                            home_country_id: h,
                            away_country_id: a,
                            phase: max_phase + 1,
                            round: i32::try_from(idx + 1).unwrap_or(1),
                            leg: 1,
                            date: next_date,
                            group_id: None,
                            window_id: win_id.clone(),
                        });
                    }
                }

                if !next_rows.is_empty() {
                    IntlRepo::insert_country_matches(conn, &next_rows)?;
                    did_advance = true;
                }
            }
        }
        let _ = MatchId(0);
        Ok(did_advance)
    }

    pub fn get_tournament_podium(
        conn: &Connection,
        tournament_id: TournamentId,
        season_id: SeasonId,
    ) -> NaratamaResult<Option<InternationalTournamentPodium>> {
        let all = IntlRepo::country_matches_for_tournament(conn, season_id, tournament_id)?;
        let max_phase = all.iter().map(|m| m.phase).max().unwrap_or(0);
        if max_phase == 0 {
            return Ok(None);
        }
        let Some(final_match) = all
            .iter()
            .find(|m| m.phase == max_phase && m.round == 1 && m.status == MatchStatus::Finished)
        else {
            return Ok(None);
        };

        let champion = final_match.winner_id.unwrap_or(final_match.home_country_id);
        let runner_up = if champion == final_match.home_country_id {
            final_match.away_country_id
        } else {
            final_match.home_country_id
        };
        let third_place = all
            .iter()
            .find(|m| m.phase == max_phase && m.round == 2 && m.status == MatchStatus::Finished)
            .and_then(|m| m.winner_id);

        let (champ_score, runner_score, pen_c, pen_r) = if champion == final_match.home_country_id {
            (
                final_match.home_goals,
                final_match.away_goals,
                final_match.penalty_home_goals,
                final_match.penalty_away_goals,
            )
        } else {
            (
                final_match.away_goals,
                final_match.home_goals,
                final_match.penalty_away_goals,
                final_match.penalty_home_goals,
            )
        };

        Ok(Some(InternationalTournamentPodium {
            tournament_id,
            season_id,
            champion_country_id: champion,
            runner_up_country_id: runner_up,
            third_place_country_id: third_place,
            champion_score: champ_score,
            runner_up_score: runner_score,
            penalty_champion_score: pen_c,
            penalty_runner_up_score: pen_r,
        }))
    }
}

fn country_match_squad(
    conn: &Connection,
    season_id: SeasonId,
    country_id: CountryId,
    fallback_power: i32,
) -> NaratamaResult<Vec<Player>> {
    let callups = IntlRepo::callup_players_for_country(conn, season_id, country_id)?;
    if callups.len() >= 11 {
        return Ok(callups);
    }
    let candidates = IntlRepo::eligible_candidates_for_country(conn, country_id, 23)?;
    if candidates.len() >= 11 {
        return Ok(candidates);
    }
    let mut squad = candidates;
    // Synthesize reserve international players for micro-nations lacking 11 club players
    while squad.len() < 16 {
        let i = i32::try_from(squad.len()).unwrap_or(0);
        let pos = match squad.len() {
            0 | 1 => 1,
            2..=6 => 3,
            7..=11 => 7,
            _ => 10,
        };
        squad.push(Player {
            player_id: naratama_model::PlayerId(-((country_id.0 * 100) + i64::from(i) + 1)),
            name: format!("National Player {}", i + 1),
            age: 24,
            position_id: pos,
            preferred_foot: naratama_model::PreferredFoot::Right,
            power: fallback_power.clamp(45, 80),
            potential: 75,
            fitness: 95,
            value: 500_000,
            salary: 5_000,
            country_id,
            squad_status: naratama_model::SquadStatus::FirstTeam,
            market_status: naratama_model::MarketStatus::Active,
            status: naratama_model::PlayerStatus::Active,
            is_captain: false,
            shirt_number: i + 1,
            stars: 3,
            skill_a: 0,
            skill_b: 0,
            height: 179,
            championships: None,
            injury_return_date: None,
        });
    }
    Ok(squad)
}

fn load_country_names(conn: &Connection) -> NaratamaResult<HashMap<CountryId, String>> {
    let mut stmt = conn
        .prepare("SELECT country_id, name FROM countries")
        .map_err(db_err)?;
    let map = stmt
        .query_map([], |r| Ok((CountryId(r.get(0)?), r.get(1)?)))
        .map_err(db_err)?
        .collect::<Result<HashMap<_, _>, _>>()
        .map_err(db_err)?;
    Ok(map)
}

fn pick_seeded_participants(
    sorted_by_power: &[Country],
    limit: usize,
    user_country_id: Option<CountryId>,
) -> Vec<CountryId> {
    let mut ids: Vec<CountryId> = sorted_by_power
        .iter()
        .take(limit)
        .map(|c| c.country_id)
        .collect();
    if let Some(uc) = user_country_id {
        if sorted_by_power.iter().any(|c| c.country_id == uc)
            && !ids.contains(&uc)
            && !ids.is_empty()
        {
            ids.pop();
            ids.push(uc);
        }
    }
    ids
}

fn build_cross_group_pairs(
    winners: &[CountryId],
    runners_up: &[CountryId],
    best_thirds: &[CountryId],
) -> Vec<(CountryId, CountryId)> {
    let mut pairs = Vec::new();
    if best_thirds.is_empty() && winners.len() == runners_up.len() && winners.len() >= 2 {
        if winners.len().is_multiple_of(2) {
            for i in (0..winners.len()).step_by(2) {
                pairs.push((winners[i], runners_up[i + 1]));
            }
            for i in (0..winners.len()).step_by(2) {
                pairs.push((winners[i + 1], runners_up[i]));
            }
        } else {
            for i in 0..winners.len() {
                pairs.push((winners[i], runners_up[(i + 1) % runners_up.len()]));
            }
        }
        return pairs;
    }

    let mut third_idx = 0_usize;
    let mut runner_idx = runners_up.len();
    let mut used_runners = HashSet::new();

    for (i, &w) in winners.iter().enumerate() {
        if third_idx < best_thirds.len() {
            pairs.push((w, best_thirds[third_idx]));
            third_idx += 1;
        } else {
            while runner_idx > 0 {
                runner_idx -= 1;
                if runner_idx != i && used_runners.insert(runner_idx) {
                    pairs.push((w, runners_up[runner_idx]));
                    break;
                }
            }
        }
    }

    let rem: Vec<CountryId> = runners_up
        .iter()
        .enumerate()
        .filter(|(idx, _)| !used_runners.contains(idx))
        .map(|(_, &cid)| cid)
        .collect();
    for &[a, b] in rem.as_chunks::<2>().0 {
        pairs.push((a, b));
    }
    pairs
}
