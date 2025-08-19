pub mod migration_engine;
pub mod triggers;
pub mod scheduler;
pub mod worker_pool;
pub mod progress_tracker;
pub mod deadlock_detector;
pub mod memory_monitor;
pub mod decay_function;
pub mod access_tracker;

use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum MigrationError {
    #[error("Database error: {0}")]
    Database(String),
    
    #[error("Migration failed: {0}")]
    MigrationFailed(String),
    
    #[error("Deadlock detected: {0}")]
    Deadlock(String),
    
    #[error("Worker pool error: {0}")]
    WorkerPool(String),
    
    #[error("Memory pressure: {0}")]
    MemoryPressure(String),
    
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
    
    #[error("Migration in progress: {id}")]
    MigrationInProgress { id: String },
    
    #[error("Migration not found: {id}")]
    MigrationNotFound { id: String },
    
    #[error("Rollback failed: {reason}")]
    RollbackFailed { reason: String },
}

pub type Result<T> = std::result::Result<T, MigrationError>;

// Re-exports
pub use migration_engine::MigrationEngine;
pub use scheduler::MigrationScheduler;
pub use worker_pool::WorkerPool;
pub use triggers::{MigrationTrigger, TriggerType};
pub use progress_tracker::{MigrationProgress, ProgressTracker};
pub use deadlock_detector::DeadlockDetector;
pub use memory_monitor::MemoryPressureMonitor;
pub use decay_function::{DecayFunction, ExponentialDecayFunction};
pub use access_tracker::AccessPatternTracker;