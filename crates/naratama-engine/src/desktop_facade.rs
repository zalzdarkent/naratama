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
use naratama_sim::compute_standings;
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
    pub standings_preview: Vec<StandingRow>,
    pub recent_news: Vec<NewsItemRow>,
    pub recent_inbox: Vec<InboxMessageRow>,
    pub offseason: Option<OffseasonStateRow>,
    pub weekly_payroll: Money,
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
            standings_preview,
            recent_news,
            recent_inbox,
            offseason,
            weekly_payroll,
        })
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
