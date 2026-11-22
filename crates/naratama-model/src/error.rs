use thiserror::Error;

#[derive(Debug, Error)]
pub enum NaratamaError {
    #[error("database error: {0}")]
    Db(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("invalid save slot {0}; expected 1..=5")]
    InvalidSlot(u8),

    #[error("invalid state: {0}")]
    InvalidState(String),

    #[error("validation error: {0}")]
    Validation(String),

    #[error("not found: {0}")]
    NotFound(String),
}

pub type NaratamaResult<T> = Result<T, NaratamaError>;
