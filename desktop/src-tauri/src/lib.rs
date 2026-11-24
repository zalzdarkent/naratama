use std::path::PathBuf;
use std::sync::Mutex;

use chrono::{Datelike, NaiveDate};
use naratama_db::club_repo::ClubRepo;
use naratama_db::reference_repo::ReferenceRepo;
use naratama_db::session::SessionManager;
use naratama_engine::{
    ClubLifeNews, DashboardSnapshot, DesktopFacade, FinanceService, GenerateCareerRequest,
    InboxMessageRow, InboxService, MatchdayOutcome, MatchdayService, NationalTeamService,
    NewsEngine, NewsItemRow, SeasonGenerator, SquadTacticsService, TrainingFocus,
};
use naratama_model::{
    Country, CountryId, MatchStatus, PlayableCountrySummary, Player, SaveSlotInfo, SeasonId,
    TeamId, TournamentId,
};
use rand::rngs::StdRng;
use rand::SeedableRng;
use rusqlite::{params, Connection};
use tauri::State;

pub struct ActiveCareer {
    pub slot_index: u8,
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

#[tauri::command]
fn list_save_slots(state: State<'_, AppState>) -> Result<Vec<SaveSlotInfo>, String> {
    let mgr = SessionManager::new(&state.saves_dir, &state.seed_db);
    mgr.list_slots().map_err(|e| e.to_string())
}

#[tauri::command]
fn list_playable_countries(state: State<'_, AppState>) -> Result<Vec<PlayableCountrySummary>, String> {
    let conn = Connection::open(&state.seed_db).map_err(|e| e.to_string())?;
    ReferenceRepo::new(&conn)
        .playable_countries()
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn start_new_career(
    state: State<'_, AppState>,
    slot_index: u8,
    country_id: i64,
    team_id: i64,
) -> Result<DashboardSnapshot, String> {
    let mgr = SessionManager::new(&state.saves_dir, &state.seed_db);
    mgr.create_slot_from_seed(slot_index)
        .map_err(|e| e.to_string())?;
    let mut conn = mgr.begin_session(slot_index).map_err(|e| e.to_string())?;
    let mut rng = StdRng::seed_from_u64(2025 + u64::from(slot_index));

    let init_date = NaiveDate::from_ymd_opt(2025, 7, 1)
        .ok_or_else(|| "Invalid init date".to_string())?;
    FinanceService::ensure_ready(&mut conn, SeasonId(1), init_date).map_err(|e| e.to_string())?;

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
        .map_or("europe", |c| NewsEngine::region_for_confederation(c.confederation))
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

    let mut guard = state
        .active
        .lock()
        .map_err(|_| "Career lock poisoned".to_string())?;
    *guard = Some(ActiveCareer {
        slot_index,
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
    mgr.commit_session(active.slot_index)
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
            list_playable_countries,
            start_new_career,
            get_dashboard,
            get_squad,
            simulate_next_matchday,
            train_focus,
            get_news_feed,
            get_inbox,
            mark_message_read,
            get_fifa_rankings,
            save_career_slot,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Naratama Gelanggang desktop application");
}

