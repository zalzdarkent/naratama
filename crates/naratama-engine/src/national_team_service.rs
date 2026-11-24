use std::collections::{HashMap, HashSet};

use chrono::{Duration, NaiveDate};
use naratama_db::club_repo::ClubRepo;
use naratama_db::db_err;
use naratama_db::intl_repo::IntlRepo;
use naratama_db::SeasonRepo;
use naratama_model::{
    tournament_ids, Confederation, Country, CountryId, InternationalWindow,
    InternationalWindowType, NaratamaResult, Player, PlayerId, PositionGroup, SeasonId, TeamId,
};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NationalTeamSquadSelection {
    pub country_id: CountryId,
    pub country_name: String,
    pub players: Vec<Player>,
    pub captain_id: PlayerId,
    pub squad_numbers: HashMap<PlayerId, i32>,
}

/// Manages FIFA World Rankings, dual-nationality / cap-tie eligibility, and national team power sync.
pub struct NationalTeamService;

impl NationalTeamService {
    pub fn get_fifa_rankings(
        conn: &Connection,
        confederation: Option<Confederation>,
    ) -> NaratamaResult<Vec<Country>> {
        IntlRepo::fifa_rankings(conn, confederation)
    }

    pub fn snapshot_world_rankings(conn: &mut Connection) -> NaratamaResult<usize> {
        IntlRepo::snapshot_world_rankings(conn)
    }

    pub fn eligible_country_ids_for_player(
        conn: &Connection,
        player_id: PlayerId,
    ) -> NaratamaResult<Vec<CountryId>> {
        IntlRepo::eligible_country_ids_for_player(conn, player_id)
    }

    pub fn refresh_all_country_powers(conn: &Connection) -> NaratamaResult<usize> {
        IntlRepo::refresh_all_country_powers(conn)
    }
}

/// Selects balanced 23-player national squads, coordinates call-ups & friendly release approvals.
pub struct NationalTeamSelectionService;

impl NationalTeamSelectionService {
    /// Selects a balanced 23-player squad (min 3 GK, 7 DEF, 6 MID, 4 ATT + 3 best remaining).
    /// Score formula: `power + (fitness / 100.0) * 3.0`.
    #[allow(clippy::too_many_lines)]
    pub fn select_squad_for_country(
        conn: &mut Connection,
        country_id: CountryId,
        season_id: SeasonId,
        window: &InternationalWindow,
        squad_size: usize,
        user_team_id: Option<TeamId>,
    ) -> NaratamaResult<NationalTeamSquadSelection> {
        let country_name = IntlRepo::country_by_id(conn, country_id)?
            .map_or_else(|| "National Team".to_string(), |c| c.name);

        let existing = IntlRepo::duties_for_window(conn, season_id, country_id, &window.window_id)?;
        if existing.len() >= squad_size {
            let ids: Vec<i64> = existing.iter().map(|d| d.player_id.0).collect();
            let players = load_players_by_ids(conn, &ids)?;
            let captain_id = existing
                .iter()
                .find(|d| d.is_captain)
                .map(|d| d.player_id)
                .or_else(|| players.first().map(|p| p.player_id))
                .unwrap_or(PlayerId(0));
            let squad_numbers = existing
                .iter()
                .map(|d| (d.player_id, d.squad_number))
                .collect();
            return Ok(NationalTeamSquadSelection {
                country_id,
                country_name,
                players,
                captain_id,
                squad_numbers,
            });
        }

        let candidates = IntlRepo::eligible_candidates_for_country(conn, country_id, 180)?;
        let mut available: Vec<Player> = Vec::new();
        let mut seen_names: HashSet<String> = HashSet::new();

        for p in candidates {
            if p.fitness < 40 {
                continue;
            }
            if p.injury_return_date
                .is_some_and(|ret| ret > window.start_date)
            {
                continue;
            }
            if !IntlRepo::can_callup_player(
                conn,
                p.player_id,
                window.release_start_date,
                window.return_date,
            )? {
                continue;
            }
            let norm = p.name.trim().to_lowercase();
            if seen_names.insert(norm) {
                available.push(p);
            }
        }

        // Score descending: `power + (fitness / 100.0) * 3.0`
        available.sort_by(|a, b| {
            let sa = f64::from(a.power) + (f64::from(a.fitness) / 100.0) * 3.0;
            let sb = f64::from(b.power) + (f64::from(b.fitness) / 100.0) * 3.0;
            sb.total_cmp(&sa)
                .then_with(|| a.player_id.cmp(&b.player_id))
        });

        let quotas = [
            (PositionGroup::Goalkeeper, 3_usize),
            (PositionGroup::Defender, 7_usize),
            (PositionGroup::Midfielder, 6_usize),
            (PositionGroup::Attacker, 4_usize),
        ];

        let mut selected: Vec<Player> = Vec::with_capacity(squad_size);
        let mut picked_ids: HashSet<PlayerId> = HashSet::new();

        for (group, min_count) in quotas {
            for p in &available {
                if selected
                    .iter()
                    .filter(|s| PositionGroup::from_position_id(s.position_id) == group)
                    .count()
                    >= min_count
                {
                    break;
                }
                if PositionGroup::from_position_id(p.position_id) == group
                    && picked_ids.insert(p.player_id)
                {
                    selected.push(p.clone());
                }
            }
        }

        for p in available {
            if selected.len() >= squad_size {
                break;
            }
            if picked_ids.insert(p.player_id) {
                selected.push(p);
            }
        }

        let captain_id = selected
            .iter()
            .filter(|p| PositionGroup::from_position_id(p.position_id) != PositionGroup::Goalkeeper)
            .max_by_key(|p| p.power)
            .or_else(|| selected.first())
            .map_or(PlayerId(0), |p| p.player_id);

        let mut squad_numbers = HashMap::new();
        let mut numbers_vec = Vec::with_capacity(selected.len());
        let mut gk_assigned = false;
        let mut next_num = 2_i32;
        for p in &selected {
            let num = if PositionGroup::from_position_id(p.position_id) == PositionGroup::Goalkeeper
                && !gk_assigned
            {
                gk_assigned = true;
                1
            } else {
                while next_num == 1 {
                    next_num += 1;
                }
                let val = next_num;
                next_num += 1;
                val
            };
            squad_numbers.insert(p.player_id, num);
            numbers_vec.push((p.player_id, num));
        }

        let requires_user_approval = is_friendly_only_window(conn, season_id, country_id, window)?;
        let mut pending_user_players = HashSet::new();
        if requires_user_approval {
            if let Some(uid) = user_team_id {
                let user_pids: HashSet<PlayerId> = ClubRepo::squad_for_team(conn, uid)?
                    .into_iter()
                    .map(|p| p.player_id)
                    .collect();
                for p in &selected {
                    if user_pids.contains(&p.player_id) {
                        pending_user_players.insert(p.player_id);
                    }
                }
            }
        }

        IntlRepo::save_callups_and_duties(
            conn,
            season_id,
            country_id,
            window,
            &selected,
            &pending_user_players,
            captain_id,
            &numbers_vec,
        )?;

        Ok(NationalTeamSquadSelection {
            country_id,
            country_name,
            players: selected,
            captain_id,
            squad_numbers,
        })
    }

    /// Approves or rejects a `pending_release` friendly call-up; rejection picks a replacement from another club.
    pub fn respond_to_friendly_callup(
        conn: &mut Connection,
        duty_id: i64,
        approve: bool,
    ) -> NaratamaResult<()> {
        if approve {
            let (season_id, country_id, player_id): (i64, i64, i64) = conn
                .query_row(
                    "SELECT season_id, country_id, player_id FROM international_duty WHERE duty_id = ?1",
                    params![duty_id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .map_err(db_err)?;
            conn.execute(
                "UPDATE international_duty SET status = 'active' WHERE duty_id = ?1",
                params![duty_id],
            )
            .map_err(db_err)?;
            conn.execute(
                "INSERT OR IGNORE INTO country_callups (country_id, player_id, season_id) VALUES (?1, ?2, ?3)",
                params![country_id, player_id, season_id],
            )
            .map_err(db_err)?;
        } else {
            conn.execute(
                "UPDATE international_duty SET status = 'rejected' WHERE duty_id = ?1",
                params![duty_id],
            )
            .map_err(db_err)?;
        }
        Ok(())
    }

    /// Triggered 7 days before any window `release_start_date`; prepares call-ups for every playing nation.
    pub fn ensure_international_callups(
        conn: &mut Connection,
        season_id: SeasonId,
        user_team_id: Option<TeamId>,
        current_date: NaiveDate,
    ) -> NaratamaResult<usize> {
        let windows = SeasonRepo::new(conn).windows_for_season(season_id)?;
        let mut called_nations = 0_usize;

        for win in windows {
            if current_date < win.release_start_date - Duration::days(7)
                || current_date > win.return_date
            {
                continue;
            }
            let win_start = win.start_date.format("%Y-%m-%d").to_string();
            let win_end = win.end_date.format("%Y-%m-%d").to_string();
            let mut stmt = conn
                .prepare(
                    "SELECT DISTINCT home_country_id FROM country_matches
                     WHERE season_id = ?1 AND (window_id = ?2 OR (date >= ?3 AND date <= ?4))
                     UNION
                     SELECT DISTINCT away_country_id FROM country_matches
                     WHERE season_id = ?1 AND (window_id = ?2 OR (date >= ?3 AND date <= ?4))",
                )
                .map_err(db_err)?;
            let countries: Vec<CountryId> = stmt
                .query_map(
                    params![season_id.0, win.window_id, win_start, win_end],
                    |r| Ok(CountryId(r.get(0)?)),
                )
                .map_err(db_err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(db_err)?;
            drop(stmt);

            for cid in countries {
                let _ =
                    Self::select_squad_for_country(conn, cid, season_id, &win, 23, user_team_id)?;
                called_nations += 1;
            }
        }
        Ok(called_nations)
    }

    pub fn release_concluded_duties(conn: &Connection, date: NaiveDate) -> NaratamaResult<usize> {
        IntlRepo::release_concluded_duties(conn, date)
    }
}

fn is_friendly_only_window(
    conn: &Connection,
    season_id: SeasonId,
    country_id: CountryId,
    window: &InternationalWindow,
) -> NaratamaResult<bool> {
    if window.window_type == InternationalWindowType::Friendly || !window.is_mandatory_release {
        return Ok(true);
    }
    let start_str = window.start_date.format("%Y-%m-%d").to_string();
    let end_str = window.end_date.format("%Y-%m-%d").to_string();
    let total_matches: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM country_matches
             WHERE season_id = ?1 AND (home_country_id = ?2 OR away_country_id = ?2)
               AND date >= ?3 AND date <= ?4",
            params![season_id.0, country_id.0, start_str, end_str],
            |r| r.get(0),
        )
        .map_err(db_err)?;
    if total_matches == 0 {
        return Ok(false);
    }
    let non_friendly: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM country_matches
             WHERE season_id = ?1 AND (home_country_id = ?2 OR away_country_id = ?2)
               AND date >= ?3 AND date <= ?4 AND tournament_id != ?5",
            params![
                season_id.0,
                country_id.0,
                start_str,
                end_str,
                tournament_ids::INTL_FRIENDLY.0
            ],
            |r| r.get(0),
        )
        .map_err(db_err)?;
    Ok(non_friendly == 0)
}

fn load_players_by_ids(conn: &Connection, ids: &[i64]) -> NaratamaResult<Vec<Player>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = vec!["?"; ids.len()].join(",");
    let sql =
        format!("SELECT * FROM players WHERE player_id IN ({placeholders}) ORDER BY power DESC");
    let mut stmt = conn.prepare(&sql).map_err(db_err)?;
    let params: Vec<&dyn rusqlite::ToSql> = ids.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
    let rows = stmt
        .query_map(params.as_slice(), ClubRepo::player_from_row)
        .map_err(db_err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_err)?;
    Ok(rows)
}
