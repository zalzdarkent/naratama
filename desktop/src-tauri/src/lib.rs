use std::path::PathBuf;
use std::sync::Mutex;

use chrono::{Datelike, NaiveDate};
use naratama_db::club_repo::{ClubRepo, ClubStaffRow};
use naratama_db::finance_repo::{FinanceRepo, SponsorshipRow};
use naratama_db::reference_repo::ReferenceRepo;
use naratama_db::session::SessionManager;
use naratama_db::SeasonRepo;
use naratama_engine::{
    BoardAmbition, ClubLifeNews, CustomClubRequest, CustomClubService, DashboardSnapshot,
    DesktopFacade, FinanceService, GenerateCareerRequest, InboxMessageRow, InboxService,
    InitialSquadPreset, MatchdayOutcome, MatchdayService, NationalTeamService, NewsEngine,
    NewsItemRow, OfferResponseAction, OffseasonService, PreseasonCamp, SeasonGenerator,
    MatchReplay, SquadTacticsService, StadiumArchitecture, TrainingFocus, TransferMarketService,
};
use naratama_model::{
    ClubFacilities, Country, CountryId, FfpEnforcement, LeagueSummary, MatchStatus, Money,
    PlayableCountrySummary, Player, PlayerId, SaveSlotInfo, Season, SeasonId, StaffRole, Team,
    TeamId, TournamentId,
};
use rand::rngs::StdRng;
use rand::SeedableRng;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;

pub struct ActiveCareer {
    pub slot_index: u8,
    pub manager_name: String,
    pub conn: Connection,
    pub season_id: SeasonId,
    pub user_team_id: TeamId,
    pub league_tournament_id: TournamentId,
    pub user_region: String,
    pub rng: StdRng,
}

pub struct AppState {
    pub saves_dir: PathBuf,
    pub seed_db: PathBuf,
    pub active: Mutex<Option<ActiveCareer>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TacticsPayload {
    pub formation: String,
    pub starter_ids: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClubHubPayload {
    pub facilities: ClubFacilities,
    pub staff: Vec<ClubStaffRow>,
    pub sponsorships: Vec<SponsorshipRow>,
    pub cash_summary: Vec<(String, i64)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketListingDto {
    pub player: Player,
    pub club_id: i64,
    pub club_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomClubInput {
    pub slot_index: u8,
    pub manager_name: String,
    pub name: String,
    pub abbreviation: String,
    pub country_id: i64,
    pub stadium_name: String,
    pub preset: String,
    pub starting_budget: Money,
    #[serde(default)]
    pub ffp_enforcement: String,
}

fn apply_ffp_mode(conn: &Connection, mode: &str) -> Result<(), String> {
    FinanceRepo::set_mode(conn, FfpEnforcement::parse(mode)).map_err(|e| e.to_string())
}

#[tauri::command]
fn list_save_slots(state: State<'_, AppState>) -> Result<Vec<SaveSlotInfo>, String> {
    let mgr = SessionManager::new(&state.saves_dir, &state.seed_db);
    mgr.get_all_slots_info().map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_save_slot(state: State<'_, AppState>, slot_index: u8) -> Result<(), String> {
    let mgr = SessionManager::new(&state.saves_dir, &state.seed_db);
    mgr.delete_slot(slot_index).map_err(|e| e.to_string())
}

/// Packs a save slot into `<saves>/exports/*.zip` and returns the archive path.
#[tauri::command]
fn export_save_slot(state: State<'_, AppState>, slot_index: u8) -> Result<String, String> {
    let mgr = SessionManager::new(&state.saves_dir, &state.seed_db);
    {
        let guard = state
            .active
            .lock()
            .map_err(|_| "Career lock poisoned".to_string())?;
        if let Some(active) = guard.as_ref() {
            if active.slot_index == slot_index {
                mgr.checkpoint_save_session(slot_index, &active.conn)
                    .map_err(|e| e.to_string())?;
            }
        }
    }
    let archive = mgr.export_slot(slot_index).map_err(|e| e.to_string())?;
    Ok(archive.display().to_string())
}

/// Lists `.zip` archives previously produced by [`export_save_slot`].
#[tauri::command]
fn list_save_exports(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let mgr = SessionManager::new(&state.saves_dir, &state.seed_db);
    Ok(mgr
        .list_exports()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|path| path.display().to_string())
        .collect())
}

/// Restores a save slot from an archive inside `<saves>/exports`.
#[tauri::command]
fn import_save_slot(
    state: State<'_, AppState>,
    slot_index: u8,
    archive_path: String,
) -> Result<(), String> {
    {
        let guard = state
            .active
            .lock()
            .map_err(|_| "Career lock poisoned".to_string())?;
        if guard.as_ref().is_some_and(|a| a.slot_index == slot_index) {
            return Err("Leave the active career before importing into its slot".to_string());
        }
    }
    let mgr = SessionManager::new(&state.saves_dir, &state.seed_db);
    mgr.import_slot(slot_index, std::path::Path::new(&archive_path))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn list_playable_countries(
    state: State<'_, AppState>,
) -> Result<Vec<PlayableCountrySummary>, String> {
    let conn = Connection::open(&state.seed_db).map_err(|e| e.to_string())?;
    ReferenceRepo::new(&conn)
        .playable_countries()
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn list_leagues_for_country(
    state: State<'_, AppState>,
    country_id: i64,
) -> Result<Vec<LeagueSummary>, String> {
    let conn = Connection::open(&state.seed_db).map_err(|e| e.to_string())?;
    ReferenceRepo::new(&conn)
        .leagues_for_country(CountryId(country_id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn list_teams_for_division(
    state: State<'_, AppState>,
    tournament_id: i64,
) -> Result<Vec<Team>, String> {
    let conn = Connection::open(&state.seed_db).map_err(|e| e.to_string())?;
    ReferenceRepo::new(&conn)
        .teams_for_division(TournamentId(tournament_id))
        .map_err(|e| e.to_string())
}

fn upsert_local_game_state(
    conn: &Connection,
    manager_name: &str,
    country_id: CountryId,
    user_team_id: TeamId,
    season_id: SeasonId,
    date_str: &str,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO local_game_state (
            id, manager_name, manager_country_id, user_team_id, current_season_id,
            current_date, game_mode, ffp_enforcement, is_retired, created_at, updated_at
         ) VALUES (1, ?1, ?2, ?3, ?4, ?5, 'career', 'off', 0, ?5, ?5)
         ON CONFLICT(id) DO UPDATE SET
            manager_name = excluded.manager_name,
            user_team_id = excluded.user_team_id,
            current_season_id = excluded.current_season_id,
            current_date = excluded.current_date,
            updated_at = excluded.updated_at",
        params![
            manager_name,
            country_id.0,
            user_team_id.0,
            season_id.0,
            date_str
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn start_new_career(
    state: State<'_, AppState>,
    slot_index: u8,
    manager_name: String,
    country_id: i64,
    team_id: i64,
    ffp_enforcement: Option<String>,
) -> Result<DashboardSnapshot, String> {
    let mgr = SessionManager::new(&state.saves_dir, &state.seed_db);
    mgr.create_slot_from_seed(slot_index)
        .map_err(|e| e.to_string())?;
    let mut conn = mgr.begin_session(slot_index).map_err(|e| e.to_string())?;
    let mut rng = StdRng::seed_from_u64(2025 + u64::from(slot_index));

    let init_date =
        NaiveDate::from_ymd_opt(2025, 7, 1).ok_or_else(|| "Invalid init date".to_string())?;
    FinanceService::ensure_ready(&mut conn, SeasonId(1), init_date).map_err(|e| e.to_string())?;
    apply_ffp_mode(&conn, ffp_enforcement.as_deref().unwrap_or_default())?;

    let cid = CountryId(country_id);
    let tid = TeamId(team_id);
    let ref_repo = ReferenceRepo::new(&conn);
    let team = ref_repo
        .team_by_id(tid)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Selected club not found".to_string())?;
    let leagues = ref_repo
        .leagues_for_country(cid)
        .map_err(|e| e.to_string())?;
    let primary = leagues
        .iter()
        .find(|l| l.division_id == team.division)
        .or_else(|| leagues.first())
        .and_then(|l| ref_repo.tournament_by_id(l.tournament_id).ok().flatten())
        .ok_or_else(|| "No domestic league found".to_string())?;
    let other_leagues: Vec<_> = leagues
        .iter()
        .filter(|l| l.tournament_id != primary.tournament_id)
        .filter_map(|l| ref_repo.tournament_by_id(l.tournament_id).ok().flatten())
        .collect();
    let cup = ref_repo.national_cup(cid).map_err(|e| e.to_string())?;
    let internationals = ref_repo.international_cups().map_err(|e| e.to_string())?;
    let region = ref_repo
        .country_by_id(cid)
        .map_err(|e| e.to_string())?
        .map_or("europe", |c| {
            NewsEngine::region_for_confederation(c.confederation)
        })
        .to_string();

    let plan = SeasonGenerator::generate_career(
        &conn,
        &GenerateCareerRequest {
            country_id: cid,
            league: primary.clone(),
            cup,
            internationals,
            other_leagues,
            start_year: 2025,
            user_team_id: Some(tid),
            include_preseason_friendlies: true,
        },
        &mut rng,
    )
    .map_err(|e| e.to_string())?;

    let _ = NewsEngine::preview(
        &conn,
        plan.season.season_id,
        init_date,
        primary.tournament_id,
        &region,
    );

    let snap = DesktopFacade::dashboard_snapshot(
        &conn,
        plan.season.season_id,
        tid,
        primary.tournament_id,
    )
    .map_err(|e| e.to_string())?;

    let mname = if manager_name.trim().is_empty() {
        "Manager".to_string()
    } else {
        manager_name.trim().to_string()
    };
    upsert_local_game_state(
        &conn,
        &mname,
        cid,
        tid,
        plan.season.season_id,
        &snap.current_date.format("%Y-%m-%d").to_string(),
    )?;
    mgr.checkpoint_save_session(slot_index, &conn)
        .map_err(|e| e.to_string())?;

    let mut guard = state
        .active
        .lock()
        .map_err(|_| "Career lock poisoned".to_string())?;
    *guard = Some(ActiveCareer {
        slot_index,
        manager_name: mname,
        conn,
        season_id: plan.season.season_id,
        user_team_id: tid,
        league_tournament_id: primary.tournament_id,
        user_region: region,
        rng,
    });
    Ok(snap)
}

#[tauri::command]
fn start_custom_club_career(
    state: State<'_, AppState>,
    input: CustomClubInput,
) -> Result<DashboardSnapshot, String> {
    let mgr = SessionManager::new(&state.saves_dir, &state.seed_db);
    mgr.create_slot_from_seed(input.slot_index)
        .map_err(|e| e.to_string())?;
    let mut conn = mgr
        .begin_session(input.slot_index)
        .map_err(|e| e.to_string())?;
    let mut rng = StdRng::seed_from_u64(3030 + u64::from(input.slot_index));

    let init_date =
        NaiveDate::from_ymd_opt(2025, 7, 1).ok_or_else(|| "Invalid init date".to_string())?;
    FinanceService::ensure_ready(&mut conn, SeasonId(1), init_date).map_err(|e| e.to_string())?;

    let preset = match input.preset.as_str() {
        "youth" => InitialSquadPreset::YouthProject,
        "veteran" => InitialSquadPreset::VeteranCore,
        _ => InitialSquadPreset::Competitive,
    };
    let cid = CountryId(input.country_id);
    let created = CustomClubService::create_custom_club(
        &mut conn,
        &CustomClubRequest {
            name: input.name,
            abbreviation: input.abbreviation,
            country_id: cid,
            founded_year: 2025,
            primary_color: "#E5A93C".into(),
            secondary_color: "#070D18".into(),
            stadium_name: input.stadium_name,
            stadium_architecture: StadiumArchitecture::CompactModern,
            squad_preset: preset,
            starting_budget: input.starting_budget.max(1_000_000),
        },
        SeasonId(1),
        init_date,
        &mut rng,
    )
    .map_err(|e| e.to_string())?;

    let tid = created.team.team_id;
    apply_ffp_mode(&conn, &input.ffp_enforcement)?;
    let ref_repo = ReferenceRepo::new(&conn);
    let leagues = ref_repo
        .leagues_for_country(cid)
        .map_err(|e| e.to_string())?;
    let primary = leagues
        .iter()
        .find(|l| l.division_id == created.team.division)
        .or_else(|| leagues.first())
        .and_then(|l| ref_repo.tournament_by_id(l.tournament_id).ok().flatten())
        .ok_or_else(|| "No domestic league found".to_string())?;
    let other_leagues: Vec<_> = leagues
        .iter()
        .filter(|l| l.tournament_id != primary.tournament_id)
        .filter_map(|l| ref_repo.tournament_by_id(l.tournament_id).ok().flatten())
        .collect();
    let cup = ref_repo.national_cup(cid).map_err(|e| e.to_string())?;
    let internationals = ref_repo.international_cups().map_err(|e| e.to_string())?;
    let region = ref_repo
        .country_by_id(cid)
        .map_err(|e| e.to_string())?
        .map_or("europe", |c| {
            NewsEngine::region_for_confederation(c.confederation)
        })
        .to_string();

    let plan = SeasonGenerator::generate_career(
        &conn,
        &GenerateCareerRequest {
            country_id: cid,
            league: primary.clone(),
            cup,
            internationals,
            other_leagues,
            start_year: 2025,
            user_team_id: Some(tid),
            include_preseason_friendlies: true,
        },
        &mut rng,
    )
    .map_err(|e| e.to_string())?;

    let _ = NewsEngine::preview(
        &conn,
        plan.season.season_id,
        init_date,
        primary.tournament_id,
        &region,
    );
    let snap = DesktopFacade::dashboard_snapshot(
        &conn,
        plan.season.season_id,
        tid,
        primary.tournament_id,
    )
    .map_err(|e| e.to_string())?;

    let mname = if input.manager_name.trim().is_empty() {
        "Manager".to_string()
    } else {
        input.manager_name.trim().to_string()
    };
    upsert_local_game_state(
        &conn,
        &mname,
        cid,
        tid,
        plan.season.season_id,
        &snap.current_date.format("%Y-%m-%d").to_string(),
    )?;
    mgr.checkpoint_save_session(input.slot_index, &conn)
        .map_err(|e| e.to_string())?;

    let mut guard = state
        .active
        .lock()
        .map_err(|_| "Career lock poisoned".to_string())?;
    *guard = Some(ActiveCareer {
        slot_index: input.slot_index,
        manager_name: mname,
        conn,
        season_id: plan.season.season_id,
        user_team_id: tid,
        league_tournament_id: primary.tournament_id,
        user_region: region,
        rng,
    });
    Ok(snap)
}

#[tauri::command]
fn load_career_slot(
    state: State<'_, AppState>,
    slot_index: u8,
) -> Result<DashboardSnapshot, String> {
    let mgr = SessionManager::new(&state.saves_dir, &state.seed_db);
    let conn = mgr.begin_session(slot_index).map_err(|e| e.to_string())?;

    let (manager_name, user_team_raw, season_raw): (String, i64, i64) = conn
        .query_row(
            "SELECT manager_name, user_team_id, current_season_id FROM local_game_state WHERE id = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|_| "Save slot has no active career metadata".to_string())?;

    let tid = TeamId(user_team_raw);
    let sid = SeasonId(season_raw);
    let ref_repo = ReferenceRepo::new(&conn);
    let team = ref_repo
        .team_by_id(tid)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Club not found in save".to_string())?;
    let leagues = ref_repo
        .leagues_for_country(team.country_id)
        .map_err(|e| e.to_string())?;
    let primary_id = leagues
        .iter()
        .find(|l| l.division_id == team.division)
        .or_else(|| leagues.first())
        .map(|l| l.tournament_id)
        .ok_or_else(|| "No league found for club".to_string())?;
    let region = ref_repo
        .country_by_id(team.country_id)
        .map_err(|e| e.to_string())?
        .map_or("europe", |c| {
            NewsEngine::region_for_confederation(c.confederation)
        })
        .to_string();

    let snap = DesktopFacade::dashboard_snapshot(&conn, sid, tid, primary_id)
        .map_err(|e| e.to_string())?;

    let mut guard = state
        .active
        .lock()
        .map_err(|_| "Career lock poisoned".to_string())?;
    *guard = Some(ActiveCareer {
        slot_index,
        manager_name,
        conn,
        season_id: sid,
        user_team_id: tid,
        league_tournament_id: primary_id,
        user_region: region,
        rng: StdRng::seed_from_u64(9000 + u64::from(slot_index)),
    });
    Ok(snap)
}

#[tauri::command]
fn leave_career(state: State<'_, AppState>, save: bool) -> Result<(), String> {
    let mut guard = state
        .active
        .lock()
        .map_err(|_| "Career lock poisoned".to_string())?;
    if let Some(active) = guard.take() {
        let mgr = SessionManager::new(&state.saves_dir, &state.seed_db);
        mgr.finish_session(active.slot_index, active.conn, save)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn get_dashboard(state: State<'_, AppState>) -> Result<DashboardSnapshot, String> {
    let guard = state
        .active
        .lock()
        .map_err(|_| "Career lock poisoned".to_string())?;
    let active = guard
        .as_ref()
        .ok_or_else(|| "No active career session".to_string())?;
    DesktopFacade::dashboard_snapshot(
        &active.conn,
        active.season_id,
        active.user_team_id,
        active.league_tournament_id,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_squad(state: State<'_, AppState>) -> Result<Vec<Player>, String> {
    let guard = state
        .active
        .lock()
        .map_err(|_| "Career lock poisoned".to_string())?;
    let active = guard
        .as_ref()
        .ok_or_else(|| "No active career session".to_string())?;
    ClubRepo::squad_for_team(&active.conn, active.user_team_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_tactics(state: State<'_, AppState>) -> Result<TacticsPayload, String> {
    let guard = state
        .active
        .lock()
        .map_err(|_| "Career lock poisoned".to_string())?;
    let active = guard
        .as_ref()
        .ok_or_else(|| "No active career session".to_string())?;
    let (formation, ids) = ClubRepo::lineup_and_formation(&active.conn, active.user_team_id)
        .map_err(|e| e.to_string())?;
    Ok(TacticsPayload {
        formation,
        starter_ids: ids.into_iter().map(|p| p.0).collect(),
    })
}

#[tauri::command]
fn save_tactics(
    state: State<'_, AppState>,
    formation: String,
    starter_ids: Vec<i64>,
) -> Result<(), String> {
    let mut guard = state
        .active
        .lock()
        .map_err(|_| "Career lock poisoned".to_string())?;
    let active = guard
        .as_mut()
        .ok_or_else(|| "No active career session".to_string())?;
    let pids: Vec<PlayerId> = starter_ids.into_iter().map(PlayerId).collect();
    ClubRepo::save_lineup_and_formation(&mut active.conn, active.user_team_id, &formation, &pids)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn simulate_next_matchday(state: State<'_, AppState>) -> Result<Vec<MatchdayOutcome>, String> {
    let mut guard = state
        .active
        .lock()
        .map_err(|_| "Career lock poisoned".to_string())?;
    let active = guard
        .as_mut()
        .ok_or_else(|| "No active career session".to_string())?;

    let next_date_str: Option<String> = active
        .conn
        .query_row(
            "SELECT MIN(date) FROM matches WHERE season_id = ?1 AND status = ?2 AND date IS NOT NULL",
            params![active.season_id.0, MatchStatus::Scheduled as i64],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let Some(ds) = next_date_str else {
        return Ok(Vec::new());
    };
    let date = NaiveDate::parse_from_str(&ds, "%Y-%m-%d").map_err(|e| e.to_string())?;
    let outcomes = MatchdayService::simulate_matchday(
        &mut active.conn,
        active.season_id,
        date,
        None,
        true,
        active.user_team_id,
        &mut active.rng,
        None,
    )
    .map_err(|e| e.to_string())?;

    let _ = NewsEngine::on_matchday(
        &active.conn,
        active.season_id,
        date,
        active.league_tournament_id,
        &active.user_region,
    );
    if let Ok(Some(club)) = ReferenceRepo::new(&active.conn).team_by_id(active.user_team_id) {
        let _ = ClubLifeNews::on_matchday(
            &active.conn,
            active.season_id,
            date,
            active.user_team_id,
            &club.name,
        );
        let _ = upsert_local_game_state(
            &active.conn,
            &active.manager_name,
            club.country_id,
            active.user_team_id,
            active.season_id,
            &ds,
        );
    }

    if date.weekday() == chrono::Weekday::Sun || date.weekday() == chrono::Weekday::Wed {
        let _ = MatchdayService::weekly_housekeeping(
            &mut active.conn,
            active.season_id,
            active.user_team_id,
            date,
            &mut active.rng,
        );
    }
    Ok(outcomes)
}

#[tauri::command]
fn get_match_replay(state: State<'_, AppState>) -> Result<Option<MatchReplay>, String> {
    let guard = state
        .active
        .lock()
        .map_err(|_| "Career lock poisoned".to_string())?;
    let active = guard
        .as_ref()
        .ok_or_else(|| "No active career session".to_string())?;
    DesktopFacade::latest_match_replay(&active.conn, active.season_id, active.user_team_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn train_focus(state: State<'_, AppState>, focus: String) -> Result<usize, String> {
    let mut guard = state
        .active
        .lock()
        .map_err(|_| "Career lock poisoned".to_string())?;
    let active = guard
        .as_mut()
        .ok_or_else(|| "No active career session".to_string())?;
    let parsed = match focus.as_str() {
        "attacking" => TrainingFocus::Attacking,
        "defending" => TrainingFocus::Defending,
        "playmaking" => TrainingFocus::Playmaking,
        "goalkeeping" => TrainingFocus::Goalkeeping,
        _ => TrainingFocus::Fitness,
    };
    let snap = DesktopFacade::dashboard_snapshot(
        &active.conn,
        active.season_id,
        active.user_team_id,
        active.league_tournament_id,
    )
    .map_err(|e| e.to_string())?;
    let improved = SquadTacticsService::train_focus(
        &mut active.conn,
        active.user_team_id,
        active.season_id,
        parsed,
        snap.current_date,
    )
    .map_err(|e| e.to_string())?;
    Ok(improved.len())
}

#[tauri::command]
fn get_club_hub(state: State<'_, AppState>) -> Result<ClubHubPayload, String> {
    let guard = state
        .active
        .lock()
        .map_err(|_| "Career lock poisoned".to_string())?;
    let active = guard
        .as_ref()
        .ok_or_else(|| "No active career session".to_string())?;
    let facilities =
        ClubRepo::facilities(&active.conn, active.user_team_id).map_err(|e| e.to_string())?;
    let staff =
        ClubRepo::staff_for_team(&active.conn, active.user_team_id).map_err(|e| e.to_string())?;
    let sponsorships = FinanceRepo::active_sponsorships_for_team(
        &active.conn,
        active.user_team_id,
        active.season_id,
    )
    .map_err(|e| e.to_string())?;
    let cash_summary = FinanceRepo::team_season_cash_summary(
        &active.conn,
        active.user_team_id,
        active.season_id,
    )
    .map_err(|e| e.to_string())?;
    Ok(ClubHubPayload {
        facilities,
        staff,
        sponsorships,
        cash_summary,
    })
}

#[tauri::command]
fn upgrade_training_ground(state: State<'_, AppState>) -> Result<ClubFacilities, String> {
    let mut guard = state
        .active
        .lock()
        .map_err(|_| "Career lock poisoned".to_string())?;
    let active = guard
        .as_mut()
        .ok_or_else(|| "No active career session".to_string())?;
    let snap = DesktopFacade::dashboard_snapshot(
        &active.conn,
        active.season_id,
        active.user_team_id,
        active.league_tournament_id,
    )
    .map_err(|e| e.to_string())?;
    SquadTacticsService::upgrade_training_ground(
        &mut active.conn,
        active.user_team_id,
        active.season_id,
        snap.current_date,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn hire_staff(
    state: State<'_, AppState>,
    role: String,
    name: String,
    stars: i32,
) -> Result<(), String> {
    let mut guard = state
        .active
        .lock()
        .map_err(|_| "Career lock poisoned".to_string())?;
    let active = guard
        .as_mut()
        .ok_or_else(|| "No active career session".to_string())?;
    let snap = DesktopFacade::dashboard_snapshot(
        &active.conn,
        active.season_id,
        active.user_team_id,
        active.league_tournament_id,
    )
    .map_err(|e| e.to_string())?;
    let parsed_role = match role.as_str() {
        "tactical_analyst" => StaffRole::TacticalAnalyst,
        "head_scout" => StaffRole::HeadScout,
        _ => StaffRole::HeadPhysio,
    };
    SquadTacticsService::hire_staff(
        &mut active.conn,
        active.user_team_id,
        active.season_id,
        parsed_role,
        &name,
        stars,
        snap.current_date,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_market_players(
    state: State<'_, AppState>,
    search: Option<String>,
    max_age: Option<i32>,
) -> Result<Vec<MarketListingDto>, String> {
    let guard = state
        .active
        .lock()
        .map_err(|_| "Career lock poisoned".to_string())?;
    let active = guard
        .as_ref()
        .ok_or_else(|| "No active career session".to_string())?;
    let pattern = format!("%{}%", search.unwrap_or_default().trim());
    let age_cap = max_age.unwrap_or(45);
    let mut stmt = active
        .conn
        .prepare(
            "SELECT p.*, tp.team_id, COALESCE(tm.name, 'Free Agent') AS club_name
             FROM players p
             JOIN team_players tp ON tp.player_id = p.player_id
             LEFT JOIN teams tm ON tm.team_id = tp.team_id
             WHERE COALESCE(p.status_id, 1) = 1
               AND tp.team_id != ?1
               AND p.name LIKE ?2
               AND COALESCE(p.age, 25) <= ?3
             ORDER BY p.power DESC, p.value DESC
             LIMIT 60",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![active.user_team_id.0, pattern, age_cap], |r| {
            Ok(MarketListingDto {
                player: ClubRepo::player_from_row(r)?,
                club_id: r.get("team_id")?,
                club_name: r.get("club_name")?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

#[tauri::command]
fn make_transfer_offer(
    state: State<'_, AppState>,
    player_id: i64,
    seller_team_id: i64,
    offer_amount: Money,
) -> Result<String, String> {
    let mut guard = state
        .active
        .lock()
        .map_err(|_| "Career lock poisoned".to_string())?;
    let active = guard
        .as_mut()
        .ok_or_else(|| "No active career session".to_string())?;
    let snap = DesktopFacade::dashboard_snapshot(
        &active.conn,
        active.season_id,
        active.user_team_id,
        active.league_tournament_id,
    )
    .map_err(|e| e.to_string())?;
    let ffp_mode = FinanceRepo::settings(&active.conn)
        .map_or(FfpEnforcement::Off, |s| s.mode);
    if ffp_mode != FfpEnforcement::Off {
        let ffp = FinanceService::assess_club_psr(
            &active.conn,
            active.user_team_id,
            active.season_id,
            ffp_mode,
        )
        .map_err(|e| e.to_string())?;
        if ffp.transfer_blocked {
            return Err(format!(
                "FFP {} sanction blocks player registration (SCR {:.1}%)",
                ffp_mode.as_str(),
                ffp.squad_cost_ratio * 100.0
            ));
        }
    }
    let offer_id = TransferMarketService::make_transfer_offer(
        &active.conn,
        PlayerId(player_id),
        TeamId(seller_team_id),
        active.user_team_id,
        offer_amount,
        false,
        1,
        snap.current_date,
    )
    .map_err(|e| e.to_string())?;
    // Auto-evaluate AI response cleanly for instant desktop feedback
    let value: Money = active
        .conn
        .query_row(
            "SELECT COALESCE(value, 100000) FROM players WHERE player_id = ?1",
            params![player_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .unwrap_or(100_000);
    let action = if offer_amount >= value {
        OfferResponseAction::Accept
    } else {
        OfferResponseAction::Reject
    };
    let outcome = TransferMarketService::respond_to_offer(
        &mut active.conn,
        active.season_id,
        offer_id,
        action,
        snap.current_date,
    )
    .map_err(|e| e.to_string())?;
    Ok(outcome.message)
}

#[tauri::command]
fn offseason_advance(
    state: State<'_, AppState>,
    action: String,
    param: Option<String>,
) -> Result<DashboardSnapshot, String> {
    let mut guard = state
        .active
        .lock()
        .map_err(|_| "Career lock poisoned".to_string())?;
    let active = guard
        .as_mut()
        .ok_or_else(|| "No active career session".to_string())?;
    let (sy, ey) = SeasonRepo::new(&active.conn)
        .season_years(active.season_id)
        .map_err(|e| e.to_string())?
        .unwrap_or((2025, 2026));
    let season = Season {
        season_id: active.season_id,
        name: format!("Season {sy}/{:02}", ey % 100),
        start_year: sy,
        end_year: ey,
    };
    let off_date = NaiveDate::from_ymd_opt(ey, 6, 15).unwrap_or(NaiveDate::MIN);

    match action.as_str() {
        "begin" => {
            OffseasonService::begin_offseason(
                &mut active.conn,
                &season,
                active.user_team_id,
                &active.manager_name,
                off_date,
                &mut active.rng,
            )
            .map_err(|e| e.to_string())?;
        }
        "contracts" => {
            OffseasonService::enter_contracts(
                &mut active.conn,
                &season,
                active.user_team_id,
                off_date,
                &mut active.rng,
            )
            .map_err(|e| e.to_string())?;
        }
        "development" => {
            OffseasonService::enter_development(&mut active.conn, &season, active.user_team_id)
                .map_err(|e| e.to_string())?;
        }
        "transfers" => {
            OffseasonService::enter_transfers(
                &mut active.conn,
                &season,
                active.user_team_id,
                off_date,
                &mut active.rng,
            )
            .map_err(|e| e.to_string())?;
        }
        "camp" => {
            let camp = match param.as_deref() {
                Some("fitness") => PreseasonCamp::FitnessBootcamp,
                Some("youth") => PreseasonCamp::YouthShowcase,
                _ => PreseasonCamp::TacticalRetreat,
            };
            OffseasonService::apply_preseason_camp(
                &mut active.conn,
                &season,
                active.user_team_id,
                camp,
            )
            .map_err(|e| e.to_string())?;
        }
        "board" => {
            let amb = match param.as_deref() {
                Some("title") => BoardAmbition::TitleChallenge,
                Some("consolidate") => BoardAmbition::Consolidate,
                _ => BoardAmbition::TopHalf,
            };
            OffseasonService::choose_board_ambition(
                &mut active.conn,
                &season,
                active.user_team_id,
                amb,
                off_date,
            )
            .map_err(|e| e.to_string())?;
        }
        "start_next_season" => {
            let plan = OffseasonService::start_next_season(
                &mut active.conn,
                &season,
                active.user_team_id,
                &mut active.rng,
            )
            .map_err(|e| e.to_string())?;
            active.season_id = plan.season.season_id;
            active.league_tournament_id = plan.league_tournament_id;
        }
        _ => return Err("Unknown offseason step action".to_string()),
    }

    DesktopFacade::dashboard_snapshot(
        &active.conn,
        active.season_id,
        active.user_team_id,
        active.league_tournament_id,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_news_feed(
    state: State<'_, AppState>,
    scope: Option<String>,
) -> Result<Vec<NewsItemRow>, String> {
    let guard = state
        .active
        .lock()
        .map_err(|_| "Career lock poisoned".to_string())?;
    let active = guard
        .as_ref()
        .ok_or_else(|| "No active career session".to_string())?;
    NewsEngine::feed(&active.conn, scope.as_deref(), 40).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_inbox(state: State<'_, AppState>) -> Result<Vec<InboxMessageRow>, String> {
    let guard = state
        .active
        .lock()
        .map_err(|_| "Career lock poisoned".to_string())?;
    let active = guard
        .as_ref()
        .ok_or_else(|| "No active career session".to_string())?;
    InboxService::list_messages(&active.conn, 50).map_err(|e| e.to_string())
}

#[tauri::command]
fn mark_message_read(state: State<'_, AppState>, message_id: i64) -> Result<(), String> {
    let guard = state
        .active
        .lock()
        .map_err(|_| "Career lock poisoned".to_string())?;
    let active = guard
        .as_ref()
        .ok_or_else(|| "No active career session".to_string())?;
    InboxService::mark_read(&active.conn, message_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_fifa_rankings(state: State<'_, AppState>) -> Result<Vec<Country>, String> {
    let guard = state
        .active
        .lock()
        .map_err(|_| "Career lock poisoned".to_string())?;
    let active = guard
        .as_ref()
        .ok_or_else(|| "No active career session".to_string())?;
    NationalTeamService::get_fifa_rankings(&active.conn, None).map_err(|e| e.to_string())
}

#[tauri::command]
fn save_career_slot(state: State<'_, AppState>) -> Result<(), String> {
    let guard = state
        .active
        .lock()
        .map_err(|_| "Career lock poisoned".to_string())?;
    let active = guard
        .as_ref()
        .ok_or_else(|| "No active career session".to_string())?;
    let mgr = SessionManager::new(&state.saves_dir, &state.seed_db);
    mgr.checkpoint_save_session(active.slot_index, &active.conn)
        .map_err(|e| e.to_string())
}

pub fn run() {
    let base_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let saves_dir = base_dir.join("saves");
    let seed_db = base_dir.join("assets/data/superkickoff_2025.db");

    tauri::Builder::default()
        .manage(AppState {
            saves_dir,
            seed_db,
            active: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            list_save_slots,
            delete_save_slot,
            export_save_slot,
            list_save_exports,
            import_save_slot,
            list_playable_countries,
            list_leagues_for_country,
            list_teams_for_division,
            start_new_career,
            start_custom_club_career,
            load_career_slot,
            leave_career,
            get_dashboard,
            get_squad,
            get_tactics,
            save_tactics,
            simulate_next_matchday,
            get_match_replay,
            train_focus,
            get_club_hub,
            upgrade_training_ground,
            hire_staff,
            get_market_players,
            make_transfer_offer,
            offseason_advance,
            get_news_feed,
            get_inbox,
            mark_message_read,
            get_fifa_rankings,
            save_career_slot,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Naratama Gelanggang desktop application");
}
