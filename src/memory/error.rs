use thiserror::Error;

#[derive(Error, Debug)]
pub enum MemoryError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Duplicate content found in tier {tier}")]
    DuplicateContent { tier: String },

    #[error("Memory not found: {id}")]
    NotFound { id: String },

    #[error("Invalid tier transition from {from} to {to}")]
    InvalidTierTransition { from: String, to: String },

    #[error("Migration failed: {reason}")]
    MigrationFailed { reason: String },

    #[error("Connection pool error: {0}")]
    ConnectionPool(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Invalid request: {message}")]
    InvalidRequest { message: String },

    #[error("Invalid data: {message}")]
    InvalidData { message: String },

    #[error("Math engine error: {0}")]
    MathEngine(#[from] super::math_engine::MathEngineError),

    #[error("Database error: {message}")]
    DatabaseError { message: String },

    #[error("Concurrency error: {message}")]
    ConcurrencyError { message: String },

    #[error("Operation timeout: {message}")]
    OperationTimeout { message: String },

    #[error("Safety violation: {message}")]
    SafetyViolation { message: String },

    #[error("Unknown error: {0}")]
    Unknown(String),
}

pub type Result<T> = std::result::Result<T, MemoryError>;
