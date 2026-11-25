use crate::migrations::ensure_schema;
use crate::{db_err, parse_sqlite_date};
use naratama_model::{NaratamaError, NaratamaResult, SaveSlotInfo};
use rusqlite::{Connection, OptionalExtension};
use std::fs;
use std::path::{Path, PathBuf};

/// Manages the 5 save slots and `.session` atomic rollback/commit snapshots.
#[derive(Debug, Clone)]
pub struct SessionManager {
    saves_dir: PathBuf,
    seed_db_path: PathBuf,
}

impl SessionManager {
    pub const MAX_SLOTS: u8 = 5;

    #[must_use]
    pub fn new(saves_dir: impl Into<PathBuf>, seed_db_path: impl Into<PathBuf>) -> Self {
        Self {
            saves_dir: saves_dir.into(),
            seed_db_path: seed_db_path.into(),
        }
    }

    pub fn validate_slot(slot: u8) -> NaratamaResult<()> {
        if (1..=Self::MAX_SLOTS).contains(&slot) {
            Ok(())
        } else {
            Err(NaratamaError::InvalidSlot(slot))
        }
    }

    pub fn slot_path(&self, slot: u8) -> NaratamaResult<PathBuf> {
        Self::validate_slot(slot)?;
        Ok(self.saves_dir.join(format!("naratama_slot_{slot}.db")))
    }

    pub fn session_path(&self, slot: u8) -> NaratamaResult<PathBuf> {
        let base = self.slot_path(slot)?;
        Ok(PathBuf::from(format!("{}.session", base.display())))
    }

    pub fn open_connection(path: &Path) -> NaratamaResult<Connection> {
        let conn = Connection::open(path).map_err(db_err)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA temp_store = MEMORY;
             PRAGMA foreign_keys = OFF;",
        )
        .map_err(db_err)?;
        ensure_schema(&conn)?;
        Ok(conn)
    }

    /// Copies the immutable seed DB into `slot` and runs initial schema migrations.
    pub fn create_slot_from_seed(&self, slot: u8) -> NaratamaResult<PathBuf> {
        let dest = self.slot_path(slot)?;
        if !self.seed_db_path.exists() {
            return Err(NaratamaError::NotFound(format!(
                "Seed database not found at {}",
                self.seed_db_path.display()
            )));
        }
        fs::create_dir_all(&self.saves_dir)?;
        remove_sqlite_sidecars(&dest);
        fs::copy(&self.seed_db_path, &dest)?;

        let conn = Self::open_connection(&dest)?;
        drop(conn);
        Ok(dest)
    }

    /// Starts a playable session by cloning the committed slot DB into `.session`.
    pub fn begin_session(&self, slot: u8) -> NaratamaResult<Connection> {
        let slot_file = self.slot_path(slot)?;
        if !slot_file.exists() {
            return Err(NaratamaError::NotFound(format!(
                "Save slot {slot} does not exist"
            )));
        }

        let session_file = self.session_path(slot)?;
        remove_sqlite_sidecars(&session_file);
        fs::copy(&slot_file, &session_file)?;
        Self::open_connection(&session_file)
    }

    /// Checkpoints WAL and atomically copies the active `.session` DB into the committed slot file
    /// without closing the live SQLite connection.
    pub fn checkpoint_save_session(&self, slot: u8, conn: &Connection) -> NaratamaResult<()> {
        let slot_file = self.slot_path(slot)?;
        let session_file = self.session_path(slot)?;
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .map_err(db_err)?;
        let tmp_dest = PathBuf::from(format!("{}.tmp", slot_file.display()));
        fs::copy(&session_file, &tmp_dest)?;
        remove_sqlite_sidecars(&slot_file);
        fs::rename(&tmp_dest, &slot_file)?;
        Ok(())
    }

    /// Ends a session: if `save` is true, checkpoints WAL and atomically replaces the slot DB.
    /// If `save` is false, discards `.session` so the committed slot DB remains untouched.
    pub fn finish_session(&self, slot: u8, conn: Connection, save: bool) -> NaratamaResult<()> {
        let slot_file = self.slot_path(slot)?;
        let session_file = self.session_path(slot)?;

        if save {
            conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
                .map_err(db_err)?;
        }
        drop(conn);

        if save {
            let tmp_dest = PathBuf::from(format!("{}.tmp", slot_file.display()));
            fs::copy(&session_file, &tmp_dest)?;
            remove_sqlite_sidecars(&slot_file);
            fs::rename(&tmp_dest, &slot_file)?;
        }

        remove_sqlite_sidecars(&session_file);
        let _ = fs::remove_file(&session_file);
        Ok(())
    }

    pub fn delete_slot(&self, slot: u8) -> NaratamaResult<()> {
        let slot_file = self.slot_path(slot)?;
        let session_file = self.session_path(slot)?;
        remove_sqlite_sidecars(&slot_file);
        remove_sqlite_sidecars(&session_file);
        if slot_file.exists() {
            fs::remove_file(&slot_file)?;
        }
        if session_file.exists() {
            fs::remove_file(&session_file)?;
        }
        Ok(())
    }

    pub fn get_all_slots_info(&self) -> NaratamaResult<Vec<SaveSlotInfo>> {
        let mut list = Vec::with_capacity(usize::from(Self::MAX_SLOTS));
        for slot in 1..=Self::MAX_SLOTS {
            let path = self.slot_path(slot)?;
            if !path.exists() {
                list.push(SaveSlotInfo {
                    slot_index: slot,
                    is_empty: true,
                    manager_name: None,
                    club_name: None,
                    season_year: None,
                    game_date: None,
                });
                continue;
            }

            let conn = Connection::open(&path).map_err(db_err)?;
            let row: Option<(String, Option<String>, Option<i32>, String)> = conn
                .query_row(
                    "SELECT g.manager_name, t.name, s.start_year, g.current_date
                     FROM local_game_state g
                     LEFT JOIN teams t ON t.team_id = g.user_team_id
                     LEFT JOIN seasons s ON s.season_id = g.current_season_id
                     WHERE g.id = 1",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )
                .optional()
                .map_err(db_err)?;

            match row {
                Some((manager_name, club_name, season_year, current_date)) => {
                    list.push(SaveSlotInfo {
                        slot_index: slot,
                        is_empty: false,
                        manager_name: Some(manager_name),
                        club_name,
                        season_year,
                        game_date: parse_sqlite_date(Some(current_date)),
                    });
                }
                None => {
                    list.push(SaveSlotInfo {
                        slot_index: slot,
                        is_empty: true,
                        manager_name: None,
                        club_name: None,
                        season_year: None,
                        game_date: None,
                    });
                }
            }
        }
        Ok(list)
    }
}

fn remove_sqlite_sidecars(db_path: &Path) {
    for suffix in ["-wal", "-shm"] {
        let sidecar = PathBuf::from(format!("{}{}", db_path.display(), suffix));
        if sidecar.exists() {
            let _ = fs::remove_file(sidecar);
        }
    }
}
