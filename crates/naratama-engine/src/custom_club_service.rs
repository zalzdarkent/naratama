use chrono::NaiveDate;
use naratama_db::club_repo::CareerStateRow;
use naratama_db::club_repo::ClubRepo;
use naratama_db::db_err;
use naratama_db::finance_repo::FinanceRepo;
use naratama_model::{
    CountryId, Money, NaratamaError, NaratamaResult, Season, SeasonId, StadiumId, Team, TeamId,
};
use naratama_sim::PlayerValuationInput;
use naratama_sim::PlayerValuationService;
use rand::Rng;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InitialSquadPreset {
    Competitive,
    YouthProject,
    VeteranCore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StadiumArchitecture {
    CompactModern,
    TraditionalBowl,
    FutureArena,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomClubRequest {
    pub name: String,
    pub abbreviation: String,
    pub country_id: CountryId,
    pub founded_year: i32,
    pub primary_color: String,
    pub secondary_color: String,
    pub stadium_name: String,
    pub stadium_architecture: StadiumArchitecture,
    pub squad_preset: InitialSquadPreset,
    pub starting_budget: Money,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomClubCreated {
    pub team: Team,
    pub stadium_id: StadiumId,
    pub replaced_team_id: TeamId,
}

/// Handles onboarding pre-season orientation completion and custom club creation.
pub struct OnboardingService;

impl OnboardingService {
    pub fn complete_orientation(
        conn: &Connection,
        season_id: SeasonId,
        date: NaiveDate,
    ) -> NaratamaResult<()> {
        let mut row = ClubRepo::career_state(conn, season_id)?;
        row.orientation_completed = true;
        if row.pre_season_date.is_none() {
            row.pre_season_date = Some(date);
        }
        ClubRepo::save_career_state(conn, &row)
    }

    pub fn is_orientation_completed(
        conn: &Connection,
        season_id: SeasonId,
    ) -> NaratamaResult<bool> {
        Ok(ClubRepo::career_state(conn, season_id)?.orientation_completed)
    }

    pub fn init_season_state(
        conn: &Connection,
        season: &Season,
        pre_season_date: NaiveDate,
    ) -> NaratamaResult<()> {
        ClubRepo::save_career_state(
            conn,
            &CareerStateRow {
                season_id: season.season_id,
                orientation_completed: false,
                pre_season_date: Some(pre_season_date),
                last_housekeeping_date: None,
            },
        )
    }
}

/// Creates a playable user custom club + stadium + 18-player balanced squad replacing the weakest lowest-division club.
pub struct CustomClubService;

impl CustomClubService {
    #[allow(clippy::too_many_lines)]
    pub fn create_custom_club(
        conn: &mut Connection,
        req: &CustomClubRequest,
        season_id: SeasonId,
        date: NaiveDate,
        rng: &mut impl Rng,
    ) -> NaratamaResult<CustomClubCreated> {
        if req.name.trim().is_empty() {
            return Err(NaratamaError::Validation(
                "Custom club name cannot be empty".into(),
            ));
        }
        let max_div: i32 = conn
            .query_row(
                "SELECT COALESCE(MAX(division), 1) FROM teams WHERE country_id = ?1 AND division > 0",
                params![req.country_id.0],
                |r| r.get(0),
            )
            .map_err(db_err)?;

        let replaced_raw: i64 = conn
            .query_row(
                "SELECT team_id FROM teams
                 WHERE country_id = ?1 AND division = ?2
                 ORDER BY power ASC, team_id DESC
                 LIMIT 1",
                params![req.country_id.0, max_div],
                |r| r.get(0),
            )
            .optional()
            .map_err(db_err)?
            .ok_or_else(|| {
                NaratamaError::NotFound(format!(
                    "No lowest-division club found to replace in country {}",
                    req.country_id.0
                ))
            })?;
        let replaced_team_id = TeamId(replaced_raw);

        let (cap_gen, cap_stand, cap_terrace, cap_boxes) = match req.stadium_architecture {
            StadiumArchitecture::CompactModern => (8_000, 6_000, 3_500, 500),
            StadiumArchitecture::TraditionalBowl => (12_000, 9_000, 5_000, 750),
            StadiumArchitecture::FutureArena => (16_000, 12_000, 7_000, 1_500),
        };

        let tx = conn.transaction().map_err(db_err)?;

        // Reassign replaced club out of league pyramid (preserve history integrity)
        tx.execute(
            "UPDATE teams SET division = 0 WHERE team_id = ?1",
            params![replaced_team_id.0],
        )
        .map_err(db_err)?;

        tx.execute(
            "INSERT INTO stadiums (
                name, capacity_general, capacity_stand, capacity_terrace, capacity_boxes,
                max_general, max_stand, max_terrace, max_boxes, country_id, city,
                ticket_price_general, ticket_price_stand, ticket_price_terrace, ticket_price_boxes
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, '', 14, 38, 21, 120)",
            params![
                req.stadium_name,
                cap_gen,
                cap_stand,
                cap_terrace,
                cap_boxes,
                cap_gen * 2,
                cap_stand * 2,
                cap_terrace * 2,
                cap_boxes * 2,
                req.country_id.0,
            ],
        )
        .map_err(db_err)?;
        let stadium_id = StadiumId(tx.last_insert_rowid());

        tx.execute(
            "INSERT INTO teams (
                name, abbreviation, country_id, power, value, budget, division,
                stadium_id, coach_id, founded_year
             ) VALUES (?1, ?2, ?3, 62, 10000000, 0, ?4, ?5, 1, ?6)",
            params![
                req.name.trim(),
                req.abbreviation.trim(),
                req.country_id.0,
                max_div,
                stadium_id.0,
                req.founded_year,
            ],
        )
        .map_err(db_err)?;
        let new_team_id = TeamId(tx.last_insert_rowid());

        // Set starting budget strictly via ledger gate so teams.budget == SUM(finance_ledger.amount)
        FinanceRepo::set_initial_club_balance_tx(
            &tx,
            new_team_id,
            season_id,
            req.starting_budget.max(500_000),
            date,
        )?;

        // Generate 22 squad players (2 GK, 7 DEF, 7 MID, 6 ATT) matching country_id & preset
        let positions = [
            1, 1, // GKs
            2, 2, 3, 3, 4, 4, 3, // DEFs
            5, 6, 7, 8, 6, 7, 8, // MIDs
            9, 10, 11, 9, 10, 11, // ATTs
        ];
        let val_svc = PlayerValuationService::default();
        let date_str = date.format("%Y-%m-%d").to_string();
        let end_date_str = format!("{}-06-30", req.founded_year.max(2025) + 3);

        for (idx, &pos_id) in positions.iter().enumerate() {
            let (age, base_ovr, base_pot) = match req.squad_preset {
                InitialSquadPreset::Competitive => (
                    rng.gen_range(22..=29),
                    rng.gen_range(60..=70),
                    rng.gen_range(68..=78),
                ),
                InitialSquadPreset::YouthProject => (
                    rng.gen_range(18..=22),
                    rng.gen_range(56..=65),
                    rng.gen_range(76..=88),
                ),
                InitialSquadPreset::VeteranCore => (
                    rng.gen_range(26..=33),
                    rng.gen_range(62..=72),
                    rng.gen_range(65..=74),
                ),
            };
            let pot = base_pot.max(base_ovr + 2);
            let value = val_svc.calculate_target_market_value(PlayerValuationInput {
                power: base_ovr,
                potential: Some(pot),
                age,
                position_id: pos_id,
                ..PlayerValuationInput::default()
            });
            let salary = 2_500 + Money::from(base_ovr - 50).max(0) * 400;
            tx.execute(
                "INSERT INTO players (
                    name, age, height, stars, value, salary, profile, experience, fitness,
                    status_id, power, potential, market_status, country_id, position_id,
                    squad_status, is_captain, shirt_number, points, skill_a, skill_b
                 ) VALUES (?1, ?2, 179, 3, ?3, ?4, 'D', 20, 100, 1, ?5, ?6, 1, ?7, ?8, ?9, ?10, ?11, 0, 0, 0)",
                params![
                    format!("{} Player {}", req.abbreviation.trim(), idx + 1),
                    age,
                    value,
                    salary,
                    base_ovr,
                    pot,
                    req.country_id.0,
                    pos_id,
                    i32::from(idx < 11),
                    i32::from(idx == 4),
                    i32::try_from(idx + 1).unwrap_or(1),
                ],
            )
            .map_err(db_err)?;
            let pid = tx.last_insert_rowid();
            tx.execute(
                "INSERT INTO team_players (team_id, player_id) VALUES (?1, ?2)",
                params![new_team_id.0, pid],
            )
            .map_err(db_err)?;
            tx.execute(
                "INSERT INTO contracts (player_id, team_id, start_date, end_date, status, salary)
                 VALUES (?1, ?2, ?3, ?4, 1, ?5)",
                params![pid, new_team_id.0, date_str, end_date_str, salary],
            )
            .map_err(db_err)?;
        }

        let final_power = ClubRepo::refresh_team_power_tx(&tx, new_team_id)?;
        tx.commit().map_err(db_err)?;

        Ok(CustomClubCreated {
            team: Team {
                team_id: new_team_id,
                name: req.name.trim().to_string(),
                abbreviation: req.abbreviation.trim().to_string(),
                country_id: req.country_id,
                power: final_power,
                value: 10_000_000,
                budget: req.starting_budget.max(500_000),
                division: max_div,
                stadium_id,
                coach_id: naratama_model::CoachId(1),
                founded_year: req.founded_year,
                logo_base64: None,
            },
            stadium_id,
            replaced_team_id,
        })
    }
}
