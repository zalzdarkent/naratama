pub mod club_repo;
pub mod finance_repo;
pub mod intl_repo;
pub mod logo_service;
pub mod market_repo;
pub mod migrations;
pub mod reference_repo;
pub mod season_repo;
pub mod session;

pub use season_repo::SeasonRepo;

use chrono::NaiveDate;
use naratama_model::NaratamaError;

#[must_use]
#[allow(clippy::needless_pass_by_value)]
pub fn db_err(err: rusqlite::Error) -> NaratamaError {
    NaratamaError::Db(err.to_string())
}

pub(crate) fn lossy_text(row: &rusqlite::Row<'_>, idx: usize) -> rusqlite::Result<String> {
    match row.get_ref(idx)? {
        rusqlite::types::ValueRef::Null => Ok(String::new()),
        rusqlite::types::ValueRef::Text(b) | rusqlite::types::ValueRef::Blob(b) => {
            Ok(String::from_utf8_lossy(b).into_owned())
        }
        rusqlite::types::ValueRef::Integer(v) => Ok(v.to_string()),
        rusqlite::types::ValueRef::Real(v) => Ok(v.to_string()),
    }
}

pub(crate) fn parse_sqlite_date(raw: Option<String>) -> Option<NaiveDate> {
    let text = raw?;
    let trimmed = text.trim();
    if trimmed.len() < 10 {
        return None;
    }
    NaiveDate::parse_from_str(&trimmed[..10], "%Y-%m-%d").ok()
}
