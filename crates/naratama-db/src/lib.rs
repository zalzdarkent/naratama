pub mod logo_service;
pub mod migrations;
pub mod reference_repo;
pub mod session;

use chrono::NaiveDate;
use naratama_model::NaratamaError;

#[allow(clippy::needless_pass_by_value)]
pub(crate) fn db_err(err: rusqlite::Error) -> NaratamaError {
    NaratamaError::Db(err.to_string())
}

pub(crate) fn parse_sqlite_date(raw: Option<String>) -> Option<NaiveDate> {
    let text = raw?;
    let trimmed = text.trim();
    if trimmed.len() < 10 {
        return None;
    }
    NaiveDate::parse_from_str(&trimmed[..10], "%Y-%m-%d").ok()
}
