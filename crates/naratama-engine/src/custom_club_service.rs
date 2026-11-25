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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BadgeShape {
    Shield,
    Circle,
    Diamond,
    Classic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BadgeSymbol {
    Soccer,
    Star,
    Crown,
    Eagle,
    Flame,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JerseyPattern {
    Plain,
    StripesVertical,
    StripesHorizontal,
    Diagonal,
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
    #[serde(default)]
    pub nickname: Option<String>,
    #[serde(default)]
    pub badge_shape: Option<BadgeShape>,
    #[serde(default)]
    pub badge_symbol: Option<BadgeSymbol>,
    #[serde(default)]
    pub jersey_pattern: Option<JerseyPattern>,
}

/// Builds a deterministic SVG crest (stored as a data URI) for a custom club.
///
/// The desktop `WebView` renders SVG directly, so no raster encoder is needed and the
/// crest stays a few hundred bytes instead of a 128×128 PNG.
#[must_use]
pub fn build_crest_data_uri(req: &CustomClubRequest) -> String {
    let shape = req.badge_shape.unwrap_or(BadgeShape::Shield);
    let symbol = req.badge_symbol.unwrap_or(BadgeSymbol::Soccer);
    let primary = sanitize_color(&req.primary_color, "#E5A93C");
    let secondary = sanitize_color(&req.secondary_color, "#070D18");
    let abbrev = req
        .abbreviation
        .trim()
        .to_uppercase()
        .chars()
        .filter(char::is_ascii_alphabetic)
        .take(3)
        .collect::<String>();

    let body = match shape {
        BadgeShape::Shield => {
            "<path d='M8 6 H56 V34 C56 50 40 58 32 62 C24 58 8 50 8 34 Z' fill='var(p)' stroke='var(s)' stroke-width='3'/>"
        }
        BadgeShape::Circle => {
            "<circle cx='32' cy='32' r='26' fill='var(p)' stroke='var(s)' stroke-width='3'/>"
        }
        BadgeShape::Diamond => {
            "<path d='M32 4 L60 32 L32 60 L4 32 Z' fill='var(p)' stroke='var(s)' stroke-width='3'/>"
        }
        BadgeShape::Classic => {
            "<circle cx='32' cy='32' r='26' fill='var(p)' stroke='var(s)' stroke-width='3'/><circle cx='32' cy='32' r='20' fill='none' stroke='var(s)' stroke-width='1.5'/>"
        }
    };
    let icon = match symbol {
        BadgeSymbol::Soccer => {
            "<circle cx='32' cy='26' r='7' fill='var(s)'/><path d='M32 19 L34 16 M32 19 L30 16 M25 26 L21 24 M39 26 L43 24 M27 32 L24 36 M37 32 L40 36' stroke='var(s)' stroke-width='2' />"
        }
        BadgeSymbol::Star => {
            "<path d='M32 12 L35.5 21.5 L45.5 21.5 L37.5 27.5 L40.5 37 L32 31.5 L23.5 37 L26.5 27.5 L18.5 21.5 L28.5 21.5 Z' fill='var(s)'/>"
        }
        BadgeSymbol::Crown => {
            "<path d='M20 34 L20 22 L26 28 L32 18 L38 28 L44 22 L44 34 Z' fill='var(s)'/>"
        }
        BadgeSymbol::Eagle => {
            "<path d='M32 16 C28 20 22 22 16 22 C22 26 26 28 30 30 L32 38 L34 30 C38 28 42 26 48 22 C42 22 36 20 32 16 Z' fill='var(s)'/>"
        }
        BadgeSymbol::Flame => {
            "<path d='M32 14 C38 22 40 26 38 32 C36 38 32 38 32 42 C30 38 26 36 26 30 C26 26 29 22 32 14 Z' fill='var(s)'/>"
        }
    };
    let text = if abbrev.is_empty() {
        String::new()
    } else {
        format!(
            "<text x='32' y='52' text-anchor='middle' font-family='Verdana,Arial,sans-serif' font-size='13' font-weight='bold' fill='{secondary}'>{abbrev}</text>"
        )
        .replace("{secondary}", &secondary)
    };
    let svg = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 64 64' width='128' height='128'>         <rect width='64' height='64' rx='10' fill='{secondary}'/>{body}{icon}{text}</svg>"
    )
    .replace("var(p)", &primary)
    .replace("var(s)", &secondary);
    format!(
        "data:image/svg+xml;base64,{}",
        base64_encode(svg.as_bytes())
    )
}

fn sanitize_color(raw: &str, fallback: &str) -> String {
    let trimmed = raw.trim();
    let hex = trimmed.strip_prefix('#').unwrap_or(trimmed);
    if (hex.len() == 6 || hex.len() == 3) && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        format!("#{}", hex.to_lowercase())
    } else {
        fallback.to_string()
    }
}

/// Minimal RFC 4648 base64 encoder (no padding-free variants needed here).
fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = chunk.get(1).copied().map_or(0, u32::from);
        let b2 = chunk.get(2).copied().map_or(0, u32::from);
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((triple >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((triple >> 12) & 0x3F) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((triple >> 6) & 0x3F) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(triple & 0x3F) as usize] as char
        } else {
            '='
        });
    }
    out
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
                stadium_id, coach_id, founded_year, logo_base64
             ) VALUES (?1, ?2, ?3, 62, 10000000, 0, ?4, ?5, 1, ?6, ?7)",
            params![
                req.name.trim(),
                req.abbreviation.trim(),
                req.country_id.0,
                max_div,
                stadium_id.0,
                req.founded_year,
                build_crest_data_uri(req),
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
                logo_base64: Some(build_crest_data_uri(req)),
            },
            stadium_id,
            replaced_team_id,
        })
    }
}
