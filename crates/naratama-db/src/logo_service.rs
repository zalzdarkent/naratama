use crate::db_err;
use naratama_model::NaratamaResult;
use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Seeds `teams.logo_base64` from a JSON map `{ "<team_id>": "<base64_or_data_uri>" }`.
pub fn seed_default_club_logos(conn: &Connection, json_path: &Path) -> NaratamaResult<usize> {
    seed_logos_into_table(conn, json_path, "teams", "team_id")
}

/// Seeds `tournaments.logo_base64` from a JSON map `{ "<tournament_id>": "<base64_or_data_uri>" }`.
pub fn seed_default_league_logos(conn: &Connection, json_path: &Path) -> NaratamaResult<usize> {
    seed_logos_into_table(conn, json_path, "tournaments", "tournament_id")
}

fn seed_logos_into_table(
    conn: &Connection,
    json_path: &Path,
    table: &str,
    id_column: &str,
) -> NaratamaResult<usize> {
    let raw = fs::read_to_string(json_path)?;
    let entries: HashMap<String, String> = serde_json::from_str(&raw)?;

    let sql = format!(
        "UPDATE {table}
         SET logo_base64 = ?1
         WHERE {id_column} = ?2
           AND (logo_base64 IS NULL OR logo_base64 = '')"
    );

    let tx = conn.unchecked_transaction().map_err(db_err)?;
    let mut updated = 0usize;
    {
        let mut stmt = tx.prepare(&sql).map_err(db_err)?;
        for (id_str, payload) in entries {
            let Ok(id) = id_str.trim().parse::<i64>() else {
                continue;
            };
            let cleaned = strip_data_uri_prefix(&payload);
            if cleaned.is_empty() {
                continue;
            }
            updated += stmt.execute(params![cleaned, id]).map_err(db_err)?;
        }
    }
    tx.commit().map_err(db_err)?;
    Ok(updated)
}

fn strip_data_uri_prefix(raw: &str) -> &str {
    let trimmed = raw.trim();
    if let Some(idx) = trimmed.find(";base64,") {
        &trimmed[idx + 8..]
    } else {
        trimmed
    }
}
