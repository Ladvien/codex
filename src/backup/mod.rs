pub mod backup_manager;
pub mod backup_verification;
pub mod disaster_recovery;
pub mod encryption;
pub mod point_in_time_recovery;
pub mod repository;
pub mod wal_archiver;

pub use backup_manager::*;
pub use backup_verification::*;
pub use disaster_recovery::*;
pub use encryption::*;
pub use point_in_time_recovery::*;
pub use repository::*;
pub use wal_archiver::*;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupConfig {
    /// Directory where backups are stored
    pub backup_directory: PathBuf,

    /// WAL archive directory
    pub wal_archive_directory: PathBuf,

    /// Backup retention policy in days
    pub retention_days: u32,

    /// Enable encryption for backups
    pub enable_encryption: bool,

    /// Encryption key path
    pub encryption_key_path: Option<PathBuf>,

    /// Backup schedule (cron expression)
    pub backup_schedule: String,

    /// Enable cross-region backup replication
    pub enable_replication: bool,

    /// Replication targets
    pub replication_targets: Vec<ReplicationTarget>,

    /// Recovery time objective in minutes
    pub rto_minutes: u32,

    /// Recovery point objective in minutes  
    pub rpo_minutes: u32,

    /// Enable backup verification
    pub enable_verification: bool,

    /// Verification schedule (cron expression)
    pub verification_schedule: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationTarget {
    pub name: String,
    pub endpoint: String,
    pub region: String,
    pub credentials: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupMetadata {
    pub id: String,
    pub backup_type: BackupType,
    pub status: BackupStatus,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub size_bytes: u64,
    pub compressed_size_bytes: u64,
    pub file_path: PathBuf,
    pub checksum: String,
    pub database_name: String,
    pub wal_start_lsn: Option<String>,
    pub wal_end_lsn: Option<String>,
    pub encryption_enabled: bool,
    pub replication_status: HashMap<String, ReplicationStatus>,
    pub verification_status: Option<VerificationStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BackupType {
    Full,
    Incremental,
    Differential,
    WalArchive,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BackupStatus {
    InProgress,
    Completed,
    Failed,
    Expired,
    Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReplicationStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationStatus {
    pub verified: bool,
    pub verification_time: DateTime<Utc>,
    pub integrity_check_passed: bool,
    pub restoration_test_passed: bool,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryOptions {
    pub target_time: Option<DateTime<Utc>>,
    pub target_lsn: Option<String>,
    pub target_transaction_id: Option<u64>,
    pub target_name: Option<String>,
    pub recovery_target_action: RecoveryTargetAction,
    pub recovery_target_inclusive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecoveryTargetAction {
    Pause,
    Promote,
    Shutdown,
}

impl Default for BackupConfig {
    fn default() -> Self {
        Self {
            backup_directory: PathBuf::from("/var/lib/codex/backups"),
            wal_archive_directory: PathBuf::from("/var/lib/codex/wal_archive"),
            retention_days: 30,
            enable_encryption: true,
            encryption_key_path: Some(PathBuf::from("/etc/codex/backup.key")),
            backup_schedule: "0 2 * * *".to_string(), // Daily at 2 AM
            enable_replication: false,
            replication_targets: Vec::new(),
            rto_minutes: 60, // 1 hour RTO
            rpo_minutes: 5,  // 5 minute RPO
            enable_verification: true,
            verification_schedule: "0 3 * * 0".to_string(), // Weekly on Sunday at 3 AM
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BackupError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Backup failed: {message}")]
    BackupFailed { message: String },

    #[error("Recovery failed: {message}")]
    RecoveryFailed { message: String },

    #[error("Verification failed: {message}")]
    VerificationFailed { message: String },

    #[error("Encryption error: {message}")]
    EncryptionError { message: String },

    #[error("Replication error: {message}")]
    ReplicationError { message: String },

    #[error("Configuration error: {message}")]
    ConfigurationError { message: String },

    #[error("Timeout error: operation timed out after {seconds} seconds")]
    Timeout { seconds: u64 },
}

pub type Result<T> = std::result::Result<T, BackupError>;
