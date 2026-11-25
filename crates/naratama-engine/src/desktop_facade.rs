use std::collections::HashMap;

use chrono::NaiveDate;
use naratama_db::club_repo::{ClubRepo, OffseasonStateRow};
use naratama_db::db_err;
use naratama_db::market_repo::MarketRepo;
use naratama_db::reference_repo::ReferenceRepo;
use naratama_db::SeasonRepo;
use naratama_model::{
    FixtureMatch, MatchStatus, Money, NaratamaError, NaratamaResult, Season, SeasonId, StandingRow,
    Team, TeamId, TournamentId,
};
use naratama_sim::{compute_standings, MatchEvent};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::manager_service::{InboxMessageRow, InboxService};
use crate::news_service::{ClubLifeNews, NewsEngine, NewsItemRow};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardSnapshot {
    pub season: Season,
    pub club: Team,
    pub current_date: NaiveDate,
    pub transfer_window_open: bool,
    pub transfer_window_label: String,
    pub fan_mood: i32,
    pub unread_inbox: i64,
    pub next_match: Option<FixtureMatch>,
    /// Resolved opponent of the next fixture (includes crest, so the UI never shows `Club #id`).
    pub next_opponent: Option<Team>,
    pub standings_preview: Vec<StandingRow>,
    pub recent_news: Vec<NewsItemRow>,
    pub recent_inbox: Vec<InboxMessageRow>,
    pub offseason: Option<OffseasonStateRow>,
    pub weekly_payroll: Money,
}

/// Stored timeline of a finished user match, replayed by the desktop match viewer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchReplay {
    pub match_id: i64,
    pub date: String,
    pub home_team_id: TeamId,
    pub away_team_id: TeamId,
    pub home_name: String,
    pub away_name: String,
    pub home_logo: Option<String>,
    pub away_logo: Option<String>,
    pub home_goals: i32,
    pub away_goals: i32,
    pub penalty_home_goals: Option<i32>,
    pub penalty_away_goals: Option<i32>,
    pub events: Vec<MatchEvent>,
}

struct RawReplayMatch {
    match_id: i64,
    date: Option<String>,
    home_id: i64,
    away_id: i64,
    home_goals: i32,
    away_goals: i32,
    penalty_home_goals: Option<i32>,
    penalty_away_goals: Option<i32>,
}

/// High-level facade for the Tauri v2 Desktop IPC commands.
pub struct DesktopFacade;

impl DesktopFacade {
    pub fn dashboard_snapshot(
        conn: &Connection,
        season_id: SeasonId,
        user_team_id: TeamId,
        league_tournament_id: TournamentId,
    ) -> NaratamaResult<DashboardSnapshot> {
        let ref_repo = ReferenceRepo::new(conn);
        let club = ref_repo
            .team_by_id(user_team_id)?
            .ok_or_else(|| NaratamaError::NotFound("Club not found".into()))?;
        let (start_year, end_year) = SeasonRepo::new(conn)
            .season_years(season_id)?
            .unwrap_or((2025, 2026));
        let season = Season {
            season_id,
            name: format!("Season {start_year}/{:02}", end_year % 100),
            start_year,
            end_year,
        };

        let next_match = find_next_club_match(conn, season_id, user_team_id)?;
        let current_date = next_match
            .as_ref()
            .and_then(|m| m.date)
            .unwrap_or_else(|| NaiveDate::from_ymd_opt(end_year, 6, 15).unwrap_or(NaiveDate::MIN));

        let active_win =
            MarketRepo::active_window_for_team(conn, season_id, user_team_id, current_date)?;
        let (transfer_window_open, transfer_window_label) = if let Some(w) = active_win {
            let days = (w.end_date - current_date).num_days().max(0);
            (true, format!("OPEN ({days}d)"))
        } else {
            (false, "CLOSED".to_string())
        };

        let team_names: HashMap<TeamId, String> = ref_repo
            .all_teams()?
            .into_iter()
            .map(|t| (t.team_id, t.name))
            .collect();
        let league_matches: Vec<FixtureMatch> = SeasonRepo::new(conn)
            .matches_for_tournament(season_id, league_tournament_id)?
            .into_iter()
            .filter(|m| m.phase <= 1)
            .collect();
        let mut clubs = Vec::new();
        for m in &league_matches {
            if !clubs.contains(&m.home_team_id) {
                clubs.push(m.home_team_id);
            }
            if !clubs.contains(&m.away_team_id) {
                clubs.push(m.away_team_id);
            }
        }
        let standings_preview = compute_standings(&clubs, &team_names, &league_matches);
        let next_match = find_next_club_match(conn, season_id, user_team_id)?;
        let next_opponent = next_match.as_ref().and_then(|m| {
            let opponent_id = if m.home_team_id == user_team_id {
                m.away_team_id
            } else {
                m.home_team_id
            };
            ref_repo.team_by_id(opponent_id).ok().flatten()
        });
        let fan_mood = ClubLifeNews::fan_mood(conn, season_id, user_team_id, 6)?;
        let unread_inbox = InboxService::unread_count(conn)?;
        let recent_news = NewsEngine::feed(conn, None, 8)?;
        let recent_inbox = InboxService::list_messages(conn, 6)?;
        let offseason = ClubRepo::offseason_state(conn)?;

        let weekly_payroll: Money = conn
            .query_row(
                "SELECT COALESCE(SUM(p.salary), 0)
                 FROM team_players tp
                 JOIN players p ON p.player_id = tp.player_id
                 WHERE tp.team_id = ?1 AND COALESCE(p.status_id, 1) = 1",
                params![user_team_id.0],
                |r| r.get(0),
            )
            .map_err(db_err)?;

        Ok(DashboardSnapshot {
            season,
            club,
            current_date,
            transfer_window_open,
            transfer_window_label,
            fan_mood,
            unread_inbox,
            next_match,
            next_opponent,
            standings_preview,
            recent_news,
            recent_inbox,
            offseason,
            weekly_payroll,
        })
    }
}

impl DesktopFacade {
    /// Latest finished match of `user_team_id` in the season, with its stored incident timeline.
    pub fn latest_match_replay(
        conn: &Connection,
        season_id: SeasonId,
        user_team_id: TeamId,
    ) -> NaratamaResult<Option<MatchReplay>> {
        let row: Option<RawReplayMatch> = conn
            .query_row(
                "SELECT match_id, date, home_team_id, away_team_id,
                        CAST(COALESCE(home_goals, 0) AS INTEGER),
                        CAST(COALESCE(away_goals, 0) AS INTEGER),
                        penalty_home_goals, penalty_away_goals
                 FROM matches
                 WHERE season_id = ?1 AND status = ?2
                   AND (home_team_id = ?3 OR away_team_id = ?3)
                 ORDER BY date DESC, match_id DESC
                 LIMIT 1",
                params![season_id.0, MatchStatus::Finished as i64, user_team_id.0],
                |r| {
                    Ok(RawReplayMatch {
                        match_id: r.get(0)?,
                        date: r.get(1)?,
                        home_id: r.get(2)?,
                        away_id: r.get(3)?,
                        home_goals: r.get(4)?,
                        away_goals: r.get(5)?,
                        penalty_home_goals: r.get(6)?,
                        penalty_away_goals: r.get(7)?,
                    })
                },
            )
            .optional()
            .map_err(db_err)?;
        let Some(raw) = row else {
            return Ok(None);
        };
        let RawReplayMatch {
            match_id,
            date,
            home_id,
            away_id,
            home_goals,
            away_goals,
            penalty_home_goals,
            penalty_away_goals,
        } = raw;

        let mut stmt = conn
            .prepare(
                "SELECT data FROM match_incidents WHERE match_id = ?1 ORDER BY incident_id ASC",
            )
            .map_err(db_err)?;
        let events: Vec<MatchEvent> = stmt
            .query_map(params![match_id], |r| r.get::<_, Option<String>>(0))
            .map_err(db_err)?
            .filter_map(Result::ok)
            .flatten()
            .filter_map(|raw| serde_json::from_str::<MatchEvent>(&raw).ok())
            .collect();

        let team_of = |team_id: i64| -> NaratamaResult<Option<naratama_model::Team>> {
            ReferenceRepo::new(conn).team_by_id(TeamId(team_id))
        };
        let name_of = |team_id: i64| -> NaratamaResult<String> {
            Ok(team_of(team_id)?.map_or_else(|| format!("Club #{team_id}"), |t| t.name))
        };
        let logo_of = |team_id: i64| -> NaratamaResult<Option<String>> {
            Ok(team_of(team_id)?.and_then(|t| t.logo_base64))
        };
        Ok(Some(MatchReplay {
            match_id,
            date: date.unwrap_or_default(),
            home_team_id: TeamId(home_id),
            away_team_id: TeamId(away_id),
            home_name: name_of(home_id)?,
            away_name: name_of(away_id)?,
            home_logo: logo_of(home_id)?,
            away_logo: logo_of(away_id)?,
            home_goals,
            away_goals,
            penalty_home_goals,
            penalty_away_goals,
            events,
        }))
    }
}

fn find_next_club_match(
    conn: &Connection,
    season_id: SeasonId,
    team_id: TeamId,
) -> NaratamaResult<Option<FixtureMatch>> {
    let mid: Option<i64> = conn
        .query_row(
            "SELECT match_id FROM matches
             WHERE season_id = ?1 AND status = ?2
               AND (home_team_id = ?3 OR away_team_id = ?3)
             ORDER BY date ASC, match_id ASC
             LIMIT 1",
            params![season_id.0, MatchStatus::Scheduled as i64, team_id.0],
            |r| r.get(0),
        )
        .optional()
        .map_err(db_err)?;
    match mid {
        Some(id) => SeasonRepo::new(conn).match_by_id(naratama_model::MatchId(id)),
        None => Ok(None),
    }
}
