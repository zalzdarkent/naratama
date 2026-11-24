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

CREATE TABLE IF NOT EXISTS competition_configs (
    tournament_id INTEGER NOT NULL,
    season_start_year INTEGER NOT NULL DEFAULT 0,
    name TEXT NOT NULL DEFAULT '',
    type TEXT NOT NULL DEFAULT 'league',
    format TEXT NOT NULL DEFAULT 'doubleRoundRobin',
    participant_count INTEGER,
    double_round_robin INTEGER NOT NULL DEFAULT 1,
    matchday_interval_days INTEGER NOT NULL DEFAULT 7,
    min_rest_days INTEGER NOT NULL DEFAULT 2,
    group_size INTEGER NOT NULL DEFAULT 4,
    group_count INTEGER NOT NULL DEFAULT 0,
    group_double_round_robin INTEGER NOT NULL DEFAULT 1,
    group_qualifiers_per_group INTEGER NOT NULL DEFAULT 2,
    league_phase_opponents INTEGER NOT NULL DEFAULT 0,
    league_phase_home_matches INTEGER NOT NULL DEFAULT 0,
    league_phase_away_matches INTEGER NOT NULL DEFAULT 0,
    league_phase_qualifiers INTEGER NOT NULL DEFAULT 0,
    knockout_legs INTEGER NOT NULL DEFAULT 1,
    final_single_match INTEGER NOT NULL DEFAULT 1,
    neutral_final INTEGER NOT NULL DEFAULT 0,
    extra_time INTEGER NOT NULL DEFAULT 1,
    penalties INTEGER NOT NULL DEFAULT 1,
    away_goals INTEGER NOT NULL DEFAULT 0,
    use_byes INTEGER NOT NULL DEFAULT 1,
    trim_to_power_of_two INTEGER NOT NULL DEFAULT 0,
    field_size INTEGER,
    promotion_slots INTEGER NOT NULL DEFAULT 0,
    relegation_slots INTEGER NOT NULL DEFAULT 0,
    unavailable_dates TEXT NOT NULL DEFAULT '[]',
    allowed_weekdays TEXT NOT NULL DEFAULT '[]',
    PRIMARY KEY (tournament_id, season_start_year)
);

CREATE TABLE IF NOT EXISTS fixture_reschedules (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    match_id INTEGER NOT NULL,
    old_date TEXT,
    new_date TEXT NOT NULL,
    reason TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS knockout_byes (
    season_id INTEGER NOT NULL,
    tournament_id INTEGER NOT NULL,
    phase INTEGER NOT NULL,
    team_id INTEGER NOT NULL,
    PRIMARY KEY (season_id, tournament_id, phase, team_id)
);

CREATE TABLE IF NOT EXISTS transfer_clauses (
    clause_id INTEGER PRIMARY KEY AUTOINCREMENT,
    player_id INTEGER NOT NULL,
    beneficiary_team_id INTEGER NOT NULL,
    type TEXT NOT NULL,
    pct REAL NOT NULL DEFAULT 0.0,
    amount INTEGER NOT NULL DEFAULT 0,
    active INTEGER NOT NULL DEFAULT 1,
    created_date TEXT,
    settled_date TEXT
);

CREATE TABLE IF NOT EXISTS career_state (
    season_id INTEGER PRIMARY KEY,
    orientation_completed INTEGER NOT NULL DEFAULT 0,
    pre_season_date TEXT,
    last_housekeeping_date TEXT
);

CREATE TABLE IF NOT EXISTS team_facilities (
    team_id INTEGER PRIMARY KEY,
    training_level INTEGER NOT NULL DEFAULT 1,
    medical_level INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS team_staff (
    staff_id INTEGER PRIMARY KEY AUTOINCREMENT,
    team_id INTEGER NOT NULL,
    role TEXT NOT NULL,
    name TEXT NOT NULL,
    stars INTEGER NOT NULL DEFAULT 1,
    salary INTEGER NOT NULL DEFAULT 5000,
    specialty TEXT NOT NULL DEFAULT '',
    hired_date TEXT,
    UNIQUE(team_id, role)
);
";

const INDEXES_DDL: &str = r"
CREATE INDEX IF NOT EXISTS idx_matches_tournament_season ON matches(tournament_id, season_id);
CREATE INDEX IF NOT EXISTS idx_matches_date ON matches(date);
CREATE INDEX IF NOT EXISTS idx_finance_ledger_team_season ON local_finance_ledger(team_id, season_id);
CREATE INDEX IF NOT EXISTS idx_player_stats_player ON local_player_stats(player_id, season_id);
CREATE INDEX IF NOT EXISTS idx_ledger_team_season ON finance_ledger(team_id, season_id);
CREATE INDEX IF NOT EXISTS idx_ledger_team_period ON finance_ledger(team_id, season_id, period);
CREATE INDEX IF NOT EXISTS idx_ledger_team_cat ON finance_ledger(team_id, season_id, category);
CREATE INDEX IF NOT EXISTS idx_registration_player_status ON player_registrations(player_id, status);
CREATE INDEX IF NOT EXISTS idx_registration_team_status ON player_registrations(team_id, status);
CREATE INDEX IF NOT EXISTS idx_clause_player ON transfer_clauses(player_id, active);
CREATE INDEX IF NOT EXISTS idx_clause_beneficiary ON transfer_clauses(beneficiary_team_id, active);
CREATE INDEX IF NOT EXISTS idx_country_matches_season_tour ON country_matches(season_id, tournament_id);
CREATE INDEX IF NOT EXISTS idx_country_matches_date ON country_matches(date);
CREATE INDEX IF NOT EXISTS idx_intl_duty_player_status ON international_duty(player_id, status);
CREATE UNIQUE INDEX IF NOT EXISTS ux_pssc_player_country_season ON player_season_stats_countries(player_id, country_id, season_id);
CREATE UNIQUE INDEX IF NOT EXISTS ux_mpc_player_match ON match_players_countries(player_id, match_id);
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
    ensure_column(conn, "matches", "kickoff_time", "TEXT")?;

    ensure_column(conn, "teams", "logo_base64", "TEXT")?;
    ensure_column(conn, "tournaments", "logo_base64", "TEXT")?;
    ensure_column(conn, "countries", "previous_rank", "INTEGER")?;
    ensure_column(conn, "countries", "points", "REAL NOT NULL DEFAULT 0.0")?;
    ensure_column(conn, "player_awards", "rating", "REAL")?;
    ensure_column(conn, "player_awards", "stat_subtitle", "TEXT")?;
    ensure_column(conn, "player_awards", "position_id", "INTEGER")?;
    ensure_column(conn, "career_state", "last_housekeeping_date", "TEXT")?;

    conn.execute_batch(INDEXES_DDL).map_err(db_err)?;
    // Best-effort unique fixture guard (ignored if legacy saves already contain duplicates).
    let _ = conn.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS ux_matches_fixture
         ON matches(season_id, tournament_id, phase, leg, round, home_team_id, away_team_id, IFNULL(group_id, -1))",
        [],
    );

    backfill_team_budgets(conn)?;
    seed_extra_tournaments(conn)?;
    fix_international_slots(conn)?;
    seed_sponsors_catalog(conn)?;
    seed_initial_fifa_points(conn)?;

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

/// Official FIFA World Ranking 2024 SUM/Elo starting points across 254 nations.
const INITIAL_FIFA_POINTS: &[(i64, f64)] = &[
    (1, 1034.37),
    (2, 1377.91),
    (3, 1692.19),
    (4, 1486.03),
    (5, 981.7),
    (6, 1281.09),
    (7, 782.08),
    (9, 1034.71),
    (10, 1433.16),
    (11, 1889.02),
    (12, 1230.43),
    (13, 868.4),
    (14, 1544.3),
    (15, 1580.37),
    (16, 1173.15),
    (17, 1768.14),
    (18, 817.25),
    (19, 1315.49),
    (20, 896.71),
    (21, 944.1),
    (22, 913.42),
    (23, 1257.69),
    (24, 904.06),
    (25, 1229.07),
    (26, 988.98),
    (27, 1293.76),
    (28, 1332.39),
    (29, 1050.15),
    (30, 1772.02),
    (31, 906.83),
    (32, 1302.68),
    (33, 1387.5),
    (34, 1101.79),
    (35, 1379.17),
    (36, 919.7),
    (37, 1458.51),
    (38, 1502.31),
    (39, 932.44),
    (40, 1467.98),
    (41, 1253.77),
    (42, 1137.06),
    (44, 1738.72),
    (45, 1159.64),
    (46, 1160.33),
    (47, 1416.91),
    (48, 1183.02),
    (49, 1572.61),
    (50, 1509.12),
    (51, 1466.08),
    (52, 1699.6),
    (53, 991.73),
    (54, 1261.38),
    (55, 1621.7),
    (56, 916.73),
    (57, 1535.43),
    (58, 1515.64),
    (59, 1307.04),
    (60, 1377.22),
    (61, 820.0),
    (62, 1485.89),
    (63, 1462.98),
    (64, 1836.42),
    (65, 1632.19),
    (66, 1140.39),
    (67, 1063.44),
    (68, 1050.03),
    (69, 1384.36),
    (70, 1038.29),
    (71, 1851.92),
    (72, 1292.88),
    (73, 1109.05),
    (74, 1378.22),
    (75, 1360.43),
    (76, 842.4),
    (77, 946.7),
    (78, 1469.18),
    (80, 740.0),
    (81, 821.91),
    (82, 1200.43),
    (83, 740.0),
    (84, 740.0),
    (85, 1298.3),
    (86, 1250.57),
    (87, 1166.02),
    (88, 1019.99),
    (89, 1278.56),
    (90, 1311.3),
    (91, 1017.66),
    (92, 1511.06),
    (93, 1133.52),
    (94, 1124.17),
    (95, 1622.92),
    (96, 1435.9),
    (97, 1394.63),
    (102, 1355.75),
    (103, 973.34),
    (104, 857.37),
    (106, 877.53),
    (108, 1089.51),
    (111, 1003.48),
    (116, 1064.11),
    (117, 802.81),
    (119, 790.63),
    (120, 792.15),
    (121, 1307.8),
    (122, 1726.31),
    (123, 1402.95),
    (124, 1639.6),
    (126, 1377.52),
    (127, 1193.21),
    (128, 1208.96),
    (129, 1201.11),
    (131, 1111.62),
    (132, 1167.49),
    (133, 889.62),
    (134, 1034.02),
    (135, 1097.98),
    (136, 1064.97),
    (137, 1153.97),
    (138, 827.19),
    (139, 1090.89),
    (140, 1261.95),
    (141, 1635.66),
    (143, 871.52),
    (144, 1351.18),
    (145, 1196.56),
    (146, 1117.64),
    (147, 1117.42),
    (148, 1455.16),
    (149, 971.89),
    (150, 1676.14),
    (152, 927.57),
    (153, 1179.04),
    (156, 1042.81),
    (157, 883.57),
    (158, 1332.68),
    (159, 924.14),
    (160, 1226.03),
    (161, 1201.3),
    (163, 935.93),
    (164, 1118.04),
    (165, 1132.39),
    (166, 1498.02),
    (167, 700.0),
    (168, 1472.16),
    (169, 1008.92),
    (170, 1247.08),
    (171, 1309.27),
    (172, 1759.95),
    (173, 842.59),
    (175, 1228.99),
    (176, 1502.55),
    (177, 984.23),
    (178, 1423.98),
    (179, 1483.2),
    (180, 1016.83),
    (181, 1525.08),
    (182, 1752.14),
    (183, 1021.62),
    (184, 1481.69),
    (185, 1817.28),
    (186, 1130.7),
    (187, 1473.19),
    (188, 1057.75),
    (189, 967.83),
    (191, 1123.32),
    (192, 1481.4),
    (193, 1508.73),
    (195, 900.79),
    (196, 883.17),
    (198, 1068.25),
    (199, 746.05),
    (202, 945.8),
    (204, 997.86),
    (205, 878.09),
    (206, 1620.87),
    (207, 1505.08),
    (208, 831.19),
    (209, 1134.26),
    (210, 1008.26),
    (212, 1252.91),
    (213, 829.81),
    (214, 831.39),
    (215, 1415.07),
    (216, 1154.5),
    (217, 1528.18),
    (218, 1641.0),
    (219, 1094.39),
    (221, 1012.17),
    (222, 1208.01),
    (223, 1223.06),
    (224, 991.34),
    (225, 1188.24),
    (228, 844.75),
    (229, 1158.81),
    (231, 835.64),
    (232, 1213.25),
    (233, 1504.86),
    (234, 1065.42),
    (235, 1538.52),
    (237, 1548.29),
    (238, 1257.64),
    (239, 1701.34),
    (240, 1413.06),
    (241, 1005.49),
    (242, 1488.56),
    (243, 1161.38),
    (245, 1021.24),
    (246, 875.13),
    (247, 1252.79),
    (248, 1134.99),
    (249, 1461.75),
    (250, 1526.53),
    (251, 1339.64),
    (252, 1205.96),
    (253, 1008.92),
];

fn seed_initial_fifa_points(conn: &Connection) -> NaratamaResult<()> {
    let ranked_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM countries WHERE COALESCE(points, 0.0) > 0.0",
            [],
            |r| r.get(0),
        )
        .map_err(db_err)?;
    if ranked_count > 0 {
        return Ok(());
    }

    let tx = conn.unchecked_transaction().map_err(db_err)?;
    {
        let mut stmt = tx
            .prepare("UPDATE countries SET points = ?1 WHERE country_id = ?2")
            .map_err(db_err)?;
        for &(cid, pts) in INITIAL_FIFA_POINTS {
            stmt.execute(params![pts, cid]).map_err(db_err)?;
        }
    }

    let mut rank_stmt = tx
        .prepare("SELECT country_id FROM countries ORDER BY COALESCE(points, 0.0) DESC, power DESC, name ASC")
        .map_err(db_err)?;
    let ranked_ids: Vec<i64> = rank_stmt
        .query_map([], |r| r.get(0))
        .map_err(db_err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_err)?;
    drop(rank_stmt);

    {
        let mut upd = tx
            .prepare("UPDATE countries SET previous_rank = ?1 WHERE country_id = ?2 AND previous_rank IS NULL")
            .map_err(db_err)?;
        for (idx, cid) in ranked_ids.into_iter().enumerate() {
            let pos = i32::try_from(idx + 1).unwrap_or(999);
            upd.execute(params![pos, cid]).map_err(db_err)?;
        }
    }
    tx.commit().map_err(db_err)?;
    Ok(())
}
