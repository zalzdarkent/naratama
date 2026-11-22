use crate::db_err;
use naratama_model::NaratamaResult;
use rusqlite::{params, Connection};
use std::collections::HashSet;

const LOCAL_TABLES_DDL: &str = r"
CREATE TABLE IF NOT EXISTS local_game_state (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    manager_name TEXT NOT NULL,
    manager_country_id INTEGER NOT NULL DEFAULT 94,
    user_team_id INTEGER NOT NULL,
    current_season_id INTEGER NOT NULL DEFAULT 1,
    current_date TEXT NOT NULL,
    game_mode TEXT NOT NULL DEFAULT 'career',
    ffp_enforcement TEXT NOT NULL DEFAULT 'off',
    national_team_id INTEGER,
    is_retired INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS local_club_state (
    team_id INTEGER PRIMARY KEY,
    fan_happiness INTEGER NOT NULL DEFAULT 60,
    board_confidence INTEGER NOT NULL DEFAULT 60,
    board_expectation TEXT NOT NULL DEFAULT 'mid_table',
    league_target_position INTEGER NOT NULL DEFAULT 10,
    transfer_budget_ratio REAL NOT NULL DEFAULT 0.70,
    wage_budget_weekly INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS local_squad_state (
    player_id INTEGER PRIMARY KEY,
    team_id INTEGER NOT NULL,
    squad_status INTEGER NOT NULL DEFAULT 2,
    market_status INTEGER NOT NULL DEFAULT 1,
    is_captain INTEGER NOT NULL DEFAULT 0,
    shirt_number INTEGER NOT NULL DEFAULT 0,
    morale INTEGER NOT NULL DEFAULT 70,
    contract_end_year INTEGER NOT NULL DEFAULT 2028
);

CREATE TABLE IF NOT EXISTS local_sponsors_catalog (
    sponsor_id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    tier INTEGER NOT NULL,
    min_power INTEGER NOT NULL DEFAULT 1,
    base_weekly_income INTEGER NOT NULL,
    win_bonus INTEGER NOT NULL DEFAULT 0,
    title_bonus INTEGER NOT NULL DEFAULT 0,
    category TEXT NOT NULL DEFAULT 'general'
);

CREATE TABLE IF NOT EXISTS local_club_sponsors (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    team_id INTEGER NOT NULL,
    sponsor_id INTEGER NOT NULL,
    slot_type TEXT NOT NULL,
    weekly_income INTEGER NOT NULL,
    win_bonus INTEGER NOT NULL DEFAULT 0,
    start_season_id INTEGER NOT NULL,
    end_season_id INTEGER NOT NULL,
    is_active INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS local_club_staff (
    staff_id INTEGER PRIMARY KEY AUTOINCREMENT,
    team_id INTEGER NOT NULL,
    role TEXT NOT NULL,
    name TEXT NOT NULL,
    country_id INTEGER NOT NULL,
    skill_level INTEGER NOT NULL,
    weekly_salary INTEGER NOT NULL,
    contract_end_year INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS local_scouts (
    scout_id INTEGER PRIMARY KEY AUTOINCREMENT,
    team_id INTEGER NOT NULL,
    name TEXT NOT NULL,
    country_id INTEGER NOT NULL,
    region_code TEXT NOT NULL,
    level INTEGER NOT NULL DEFAULT 1,
    weekly_salary INTEGER NOT NULL,
    assigned_country_id INTEGER
);

CREATE TABLE IF NOT EXISTS local_scout_reports (
    report_id INTEGER PRIMARY KEY AUTOINCREMENT,
    team_id INTEGER NOT NULL,
    scout_id INTEGER NOT NULL,
    player_id INTEGER NOT NULL,
    knowledge_pct INTEGER NOT NULL DEFAULT 25,
    estimated_min_value INTEGER NOT NULL DEFAULT 0,
    estimated_max_value INTEGER NOT NULL DEFAULT 0,
    created_date TEXT NOT NULL,
    UNIQUE(team_id, player_id)
);

CREATE TABLE IF NOT EXISTS local_tactics (
    team_id INTEGER PRIMARY KEY,
    formation TEXT NOT NULL DEFAULT '4-3-3',
    mentality TEXT NOT NULL DEFAULT 'balanced',
    pressing TEXT NOT NULL DEFAULT 'medium',
    tempo TEXT NOT NULL DEFAULT 'normal',
    passing_style TEXT NOT NULL DEFAULT 'mixed',
    marking TEXT NOT NULL DEFAULT 'zonal',
    offside_trap INTEGER NOT NULL DEFAULT 0,
    counter_attack INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS local_transfer_offers (
    offer_id INTEGER PRIMARY KEY AUTOINCREMENT,
    season_id INTEGER NOT NULL,
    player_id INTEGER NOT NULL,
    from_team_id INTEGER NOT NULL,
    to_team_id INTEGER NOT NULL,
    offer_amount INTEGER NOT NULL,
    offered_salary INTEGER NOT NULL DEFAULT 0,
    contract_years INTEGER NOT NULL DEFAULT 3,
    is_loan INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'pending',
    created_date TEXT NOT NULL,
    resolved_date TEXT
);

CREATE TABLE IF NOT EXISTS local_player_stats (
    season_id INTEGER NOT NULL,
    tournament_id INTEGER NOT NULL,
    player_id INTEGER NOT NULL,
    team_id INTEGER NOT NULL,
    appearances INTEGER NOT NULL DEFAULT 0,
    minutes_played INTEGER NOT NULL DEFAULT 0,
    goals INTEGER NOT NULL DEFAULT 0,
    assists INTEGER NOT NULL DEFAULT 0,
    yellow_cards INTEGER NOT NULL DEFAULT 0,
    red_cards INTEGER NOT NULL DEFAULT 0,
    clean_sheets INTEGER NOT NULL DEFAULT 0,
    rating_sum REAL NOT NULL DEFAULT 0.0,
    rating_count INTEGER NOT NULL DEFAULT 0,
    motm_count INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (season_id, tournament_id, player_id, team_id)
);

CREATE TABLE IF NOT EXISTS local_finance_ledger (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    team_id INTEGER NOT NULL,
    season_id INTEGER NOT NULL,
    date TEXT NOT NULL,
    period TEXT NOT NULL,
    category TEXT NOT NULL,
    amount INTEGER NOT NULL,
    description TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS local_club_awards (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    season_id INTEGER NOT NULL,
    tournament_id INTEGER NOT NULL,
    award_type TEXT NOT NULL,
    winner_team_id INTEGER,
    winner_player_id INTEGER,
    stat_value INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS local_news (
    news_id INTEGER PRIMARY KEY AUTOINCREMENT,
    date TEXT NOT NULL,
    season_id INTEGER NOT NULL,
    category TEXT NOT NULL,
    title_key TEXT NOT NULL,
    payload_json TEXT NOT NULL DEFAULT '{}',
    team_id INTEGER,
    is_read INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS local_season_archives (
    season_id INTEGER PRIMARY KEY,
    season_name TEXT NOT NULL,
    champion_summary_json TEXT NOT NULL DEFAULT '{}',
    archived_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS local_player_growth_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    season_id INTEGER NOT NULL,
    player_id INTEGER NOT NULL,
    old_power INTEGER NOT NULL,
    new_power INTEGER NOT NULL,
    reason TEXT NOT NULL DEFAULT 'training'
);

CREATE TABLE IF NOT EXISTS local_manager_offers (
    offer_id INTEGER PRIMARY KEY AUTOINCREMENT,
    team_id INTEGER NOT NULL,
    offered_date TEXT NOT NULL,
    wage_weekly INTEGER NOT NULL,
    transfer_budget INTEGER NOT NULL,
    expectation TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending'
);

CREATE TABLE IF NOT EXISTS local_job_applications (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    team_id INTEGER NOT NULL,
    applied_date TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending'
);

CREATE TABLE IF NOT EXISTS local_national_squad_selections (
    country_id INTEGER NOT NULL,
    player_id INTEGER NOT NULL,
    role TEXT NOT NULL DEFAULT 'squad',
    is_captain INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (country_id, player_id)
);

CREATE TABLE IF NOT EXISTS local_international_windows (
    window_id INTEGER PRIMARY KEY AUTOINCREMENT,
    season_id INTEGER NOT NULL,
    start_date TEXT NOT NULL,
    end_date TEXT NOT NULL,
    window_type TEXT NOT NULL,
    tournament_id INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS local_match_events (
    event_id INTEGER PRIMARY KEY AUTOINCREMENT,
    match_id INTEGER NOT NULL,
    minute INTEGER NOT NULL,
    event_type TEXT NOT NULL,
    team_id INTEGER NOT NULL,
    player_id INTEGER,
    assist_player_id INTEGER,
    detail_json TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE IF NOT EXISTS local_club_facilities (
    team_id INTEGER PRIMARY KEY,
    training_level INTEGER NOT NULL DEFAULT 1,
    medical_level INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS local_youth_academy (
    prospect_id INTEGER PRIMARY KEY AUTOINCREMENT,
    team_id INTEGER NOT NULL,
    name TEXT NOT NULL,
    age INTEGER NOT NULL,
    position_id INTEGER NOT NULL,
    country_id INTEGER NOT NULL,
    current_power INTEGER NOT NULL,
    potential INTEGER NOT NULL,
    preferred_foot TEXT NOT NULL DEFAULT 'D',
    created_season_id INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS local_season_trophies (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    team_id INTEGER NOT NULL,
    season_id INTEGER NOT NULL,
    tournament_id INTEGER NOT NULL,
    trophy_name TEXT NOT NULL
);
";

const INDEXES_DDL: &str = r"
CREATE INDEX IF NOT EXISTS idx_matches_tournament_season ON matches(tournament_id, season_id);
CREATE INDEX IF NOT EXISTS idx_matches_date ON matches(date);
CREATE INDEX IF NOT EXISTS idx_finance_ledger_team_season ON local_finance_ledger(team_id, season_id);
CREATE INDEX IF NOT EXISTS idx_player_stats_player ON local_player_stats(player_id, season_id);
";

/// Applies idempotent local schema migrations, indexes, backfills, and reference seed fixes.
pub fn ensure_schema(conn: &Connection) -> NaratamaResult<()> {
    // Drop legacy triggers that auto-delete matches/news behind the engine's back.
    conn.execute_batch(
        "DROP TRIGGER IF EXISTS t_clear_season;
         DROP TRIGGER IF EXISTS t_clear_season_countries;
         DROP TRIGGER IF EXISTS t_delete_old_messages;",
    )
    .map_err(db_err)?;

    conn.execute_batch(LOCAL_TABLES_DDL).map_err(db_err)?;

    ensure_column(conn, "players", "injury_return_date", "TEXT")?;
    ensure_column(
        conn,
        "players",
        "national_caps",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        conn,
        "players",
        "national_goals",
        "INTEGER NOT NULL DEFAULT 0",
    )?;

    ensure_column(
        conn,
        "matches",
        "neutral_venue",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(conn, "matches", "extra_time", "INTEGER NOT NULL DEFAULT 0")?;
    ensure_column(
        conn,
        "matches",
        "away_goals_rule",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(conn, "matches", "original_date", "TEXT")?;
    ensure_column(conn, "matches", "reschedule_reason", "TEXT")?;

    ensure_column(conn, "teams", "logo_base64", "TEXT")?;
    ensure_column(conn, "tournaments", "logo_base64", "TEXT")?;

    conn.execute_batch(INDEXES_DDL).map_err(db_err)?;

    backfill_team_budgets(conn)?;
    seed_extra_tournaments(conn)?;
    fix_international_slots(conn)?;
    seed_sponsors_catalog(conn)?;

    Ok(())
}

fn table_columns(conn: &Connection, table: &str) -> NaratamaResult<HashSet<String>> {
    let sql = format!("PRAGMA table_info({table})");
    let mut stmt = conn.prepare(&sql).map_err(db_err)?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(db_err)?;

    let mut cols = HashSet::new();
    for col in rows {
        cols.insert(col.map_err(db_err)?);
    }
    Ok(cols)
}

fn ensure_column(
    conn: &Connection,
    table: &str,
    column: &str,
    col_def: &str,
) -> NaratamaResult<()> {
    let cols = table_columns(conn, table)?;
    if !cols.contains(column) {
        let sql = format!("ALTER TABLE {table} ADD COLUMN {column} {col_def}");
        conn.execute(&sql, []).map_err(db_err)?;
    }
    Ok(())
}

fn backfill_team_budgets(conn: &Connection) -> NaratamaResult<()> {
    conn.execute(
        "UPDATE teams
         SET budget = CASE
             WHEN COALESCE(power, 0) <= 40 THEN 500000
             ELSE MAX(500000, (power - 40) * (power - 40) * 40000)
         END
         WHERE budget IS NULL OR budget = 0",
        [],
    )
    .map_err(db_err)?;
    Ok(())
}

fn seed_extra_tournaments(conn: &Connection) -> NaratamaResult<()> {
    let extras: &[(i64, &str, i64, i64, &str, i64, &str)] = &[
        (9999, "Pre-Season Friendly", 0, 4, "32", 0, "1,2,3,4,5,6,7"),
        (1650, "AFC Challenge League", 0, 2, "16", 3_000_000, "2,3,4"),
        (1651, "AFC Super Cup", 0, 2, "2", 2_500_000, "3,6"),
        (
            900,
            "International Friendly",
            0,
            4,
            "32",
            0,
            "1,2,3,4,5,6,7",
        ),
        (901, "Nations League", 0, 2, "16", 5_000_000, "3,6,7"),
        (902, "ASEAN Championship", 0, 2, "10", 1_500_000, "3,6,7"),
        (903, "FIFA ASEAN Cup", 0, 2, "8", 2_000_000, "3,6,7"),
        (904, "World Cup Qualifiers", 0, 2, "48", 4_000_000, "3,6,7"),
    ];

    let mut stmt = conn
        .prepare(
            "INSERT OR IGNORE INTO tournaments (
                tournament_id, name, division_id, status, organization,
                rounds, reversed, play_days, slots, points,
                promotion_slots, relegation_spots, prize_teams, prize_top_scorer, prize_mvp
            ) VALUES (?1, ?2, ?3, 1, ?4, 1, 0, ?7, ?5, 1.0, 0, 0, ?6, 250000, 250000)",
        )
        .map_err(db_err)?;

    for &(id, name, div_id, org, slots, prize, play_days) in extras {
        stmt.execute(params![id, name, div_id, org, slots, prize, play_days])
            .map_err(db_err)?;
    }
    Ok(())
}

fn fix_international_slots(conn: &Connection) -> NaratamaResult<()> {
    // Clean up malformed slots pointing to zero/invalid countries.
    conn.execute(
        "DELETE FROM international_slots WHERE country_id <= 0 OR tournament_id <= 0",
        [],
    )
    .map_err(db_err)?;

    // Ensure AFC Challenge League (1650) has baseline Asian emerging-league slots if missing.
    let existing: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM international_slots WHERE tournament_id = 1650",
            [],
            |row| row.get(0),
        )
        .map_err(db_err)?;

    if existing == 0 {
        let mut stmt = conn
            .prepare(
                "INSERT OR IGNORE INTO international_slots (tournament_id, country_id, slots)
                 SELECT 1650, c.country_id, 1
                 FROM countries c
                 WHERE c.confederation = 5
                   AND EXISTS (SELECT 1 FROM teams t WHERE t.country_id = c.country_id)
                 ORDER BY c.power ASC
                 LIMIT 8",
            )
            .map_err(db_err)?;
        stmt.execute([]).map_err(db_err)?;
    }

    Ok(())
}

struct SponsorSeed {
    id: i64,
    name: &'static str,
    tier: i64,
    min_power: i64,
    weekly_income: i64,
    win_bonus: i64,
    title_bonus: i64,
    category: &'static str,
}

const DEFAULT_SPONSORS: &[SponsorSeed] = &[
    SponsorSeed {
        id: 1,
        name: "Nusantara Air",
        tier: 5,
        min_power: 82,
        weekly_income: 1_400_000,
        win_bonus: 250_000,
        title_bonus: 6_000_000,
        category: "airline",
    },
    SponsorSeed {
        id: 2,
        name: "Garuda Energy",
        tier: 5,
        min_power: 80,
        weekly_income: 1_250_000,
        win_bonus: 200_000,
        title_bonus: 5_000_000,
        category: "energy",
    },
    SponsorSeed {
        id: 3,
        name: "GlobalPay Prime",
        tier: 5,
        min_power: 78,
        weekly_income: 1_100_000,
        win_bonus: 180_000,
        title_bonus: 4_500_000,
        category: "fintech",
    },
    SponsorSeed {
        id: 4,
        name: "Apex Telecom",
        tier: 4,
        min_power: 74,
        weekly_income: 800_000,
        win_bonus: 120_000,
        title_bonus: 3_000_000,
        category: "telecom",
    },
    SponsorSeed {
        id: 5,
        name: "Veloce Motors",
        tier: 4,
        min_power: 72,
        weekly_income: 720_000,
        win_bonus: 100_000,
        title_bonus: 2_500_000,
        category: "automotive",
    },
    SponsorSeed {
        id: 6,
        name: "Pacific Bank",
        tier: 4,
        min_power: 70,
        weekly_income: 650_000,
        win_bonus: 90_000,
        title_bonus: 2_200_000,
        category: "finance",
    },
    SponsorSeed {
        id: 7,
        name: "Karya Sports",
        tier: 3,
        min_power: 64,
        weekly_income: 420_000,
        win_bonus: 60_000,
        title_bonus: 1_200_000,
        category: "apparel",
    },
    SponsorSeed {
        id: 8,
        name: "Aurora Cloud",
        tier: 3,
        min_power: 62,
        weekly_income: 380_000,
        win_bonus: 50_000,
        title_bonus: 1_000_000,
        category: "tech",
    },
    SponsorSeed {
        id: 9,
        name: "Metro Logistics",
        tier: 3,
        min_power: 58,
        weekly_income: 320_000,
        win_bonus: 40_000,
        title_bonus: 800_000,
        category: "logistics",
    },
    SponsorSeed {
        id: 10,
        name: "Bintang Drink",
        tier: 2,
        min_power: 50,
        weekly_income: 180_000,
        win_bonus: 25_000,
        title_bonus: 400_000,
        category: "beverage",
    },
    SponsorSeed {
        id: 11,
        name: "Surya Retail",
        tier: 2,
        min_power: 46,
        weekly_income: 140_000,
        win_bonus: 20_000,
        title_bonus: 300_000,
        category: "retail",
    },
    SponsorSeed {
        id: 12,
        name: "Harapan Insurance",
        tier: 2,
        min_power: 42,
        weekly_income: 110_000,
        win_bonus: 15_000,
        title_bonus: 250_000,
        category: "finance",
    },
    SponsorSeed {
        id: 13,
        name: "Lokal Kopi",
        tier: 1,
        min_power: 1,
        weekly_income: 55_000,
        win_bonus: 8_000,
        title_bonus: 100_000,
        category: "food",
    },
    SponsorSeed {
        id: 14,
        name: "Kota Mart",
        tier: 1,
        min_power: 1,
        weekly_income: 45_000,
        win_bonus: 6_000,
        title_bonus: 80_000,
        category: "retail",
    },
    SponsorSeed {
        id: 15,
        name: "Sentosa Construction",
        tier: 1,
        min_power: 1,
        weekly_income: 38_000,
        win_bonus: 5_000,
        title_bonus: 65_000,
        category: "construction",
    },
];

fn seed_sponsors_catalog(conn: &Connection) -> NaratamaResult<()> {
    let existing: i64 = conn
        .query_row("SELECT COUNT(*) FROM local_sponsors_catalog", [], |row| {
            row.get(0)
        })
        .map_err(db_err)?;

    if existing > 0 {
        return Ok(());
    }

    let mut stmt = conn
        .prepare(
            "INSERT OR IGNORE INTO local_sponsors_catalog (
                sponsor_id, name, tier, min_power, base_weekly_income,
                win_bonus, title_bonus, category
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .map_err(db_err)?;

    for s in DEFAULT_SPONSORS {
        stmt.execute(params![
            s.id,
            s.name,
            s.tier,
            s.min_power,
            s.weekly_income,
            s.win_bonus,
            s.title_bonus,
            s.category
        ])
        .map_err(db_err)?;
    }

    Ok(())
}
