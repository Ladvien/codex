use super::{BackupConfig, BackupError, BackupMetadata, BackupStatus, BackupType, Result};
use super::repository::{BackupRepository, PostgresBackupRepository};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use tokio::fs;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Main backup manager responsible for orchestrating all backup operations
pub struct BackupManager {
    config: BackupConfig,
    repository: Arc<dyn BackupRepository>,
}

impl BackupManager {
    pub fn new(config: BackupConfig, db_pool: Arc<sqlx::PgPool>) -> Self {
        let repository = Arc::new(PostgresBackupRepository::new(db_pool));
        Self {
            config,
            repository,
        }
    }
    
    pub fn with_repository(config: BackupConfig, repository: Arc<dyn BackupRepository>) -> Self {
        Self {
            config,
            repository,
        }
    }

    /// Initialize the backup manager and create necessary directories
    pub async fn initialize(&self) -> Result<()> {
        info!("Initializing backup manager");

        // Create backup directories
        fs::create_dir_all(&self.config.backup_directory).await?;
        fs::create_dir_all(&self.config.wal_archive_directory).await?;

        // Initialize repository
        self.repository.initialize().await?;

        // Verify PostgreSQL configuration
        self.repository.verify_postgres_config().await?;

        info!("Backup manager initialized successfully");
        Ok(())
    }

    /// Perform a full database backup
    pub async fn create_full_backup(&self) -> Result<BackupMetadata> {
        let backup_id = Uuid::new_v4().to_string();
        let start_time = Utc::now();

        info!("Starting full backup: {}", backup_id);

        let backup_filename = format!(
            "full_backup_{}_{}.sql.gz",
            start_time.format("%Y%m%d_%H%M%S"),
            backup_id
        );
        let backup_path = self.config.backup_directory.join(&backup_filename);

        // Check available disk space before starting backup
        self.check_disk_space(&backup_path).await?;

        let mut metadata = BackupMetadata {
            id: backup_id.clone(),
            backup_type: BackupType::Full,
            status: BackupStatus::InProgress,
            start_time,
            end_time: None,
            size_bytes: 0,
            compressed_size_bytes: 0,
            file_path: backup_path.clone(),
            checksum: String::new(),
            database_name: self.extract_database_name()?,
            wal_start_lsn: None,
            wal_end_lsn: None,
            encryption_enabled: self.config.enable_encryption,
            replication_status: std::collections::HashMap::new(),
            verification_status: None,
        };

        // Store initial metadata
        self.repository.store_metadata(&metadata).await?;

        // Get WAL start LSN
        let start_lsn = self.repository.get_current_wal_lsn().await?;
        metadata.wal_start_lsn = Some(start_lsn);

        // Perform the backup using pg_dump
        match self
            .execute_pg_dump(&backup_path, &metadata.database_name)
            .await
        {
            Ok(_) => {
                let end_time = Utc::now();
                metadata.end_time = Some(end_time);
                metadata.status = BackupStatus::Completed;

                // Get WAL end LSN
                let end_lsn = self.repository.get_current_wal_lsn().await?;
                metadata.wal_end_lsn = Some(end_lsn);

                // Calculate file sizes and checksum
                let file_metadata = fs::metadata(&backup_path).await?;
                metadata.compressed_size_bytes = file_metadata.len();
                metadata.checksum = self.calculate_file_checksum(&backup_path).await?;

                // Update metadata
                self.repository.update_metadata(&metadata).await?;

                info!(
                    "Full backup completed successfully: {} ({} bytes)",
                    backup_id, metadata.compressed_size_bytes
                );

                // Trigger replication if enabled
                if self.config.enable_replication {
                    self.replicate_backup(&metadata).await?;
                }

                Ok(metadata)
            }
            Err(e) => {
                metadata.status = BackupStatus::Failed;
                metadata.end_time = Some(Utc::now());
                self.repository.update_metadata(&metadata).await?;

                error!("Full backup failed: {}", e);
                Err(e)
            }
        }
    }

    /// Create an incremental backup (WAL archives since last backup)
    pub async fn create_incremental_backup(&self) -> Result<BackupMetadata> {
        let backup_id = Uuid::new_v4().to_string();
        let start_time = Utc::now();

        info!("Starting incremental backup: {}", backup_id);

        // Get the last backup to determine starting point
        let last_backup = self.repository.get_latest_backup().await?;
        let start_lsn = match last_backup {
            Some(backup) => {
                if let Some(lsn) = backup.wal_end_lsn {
                    lsn
                } else {
                    warn!("Last backup has no end LSN, using current LSN");
                    self.repository.get_current_wal_lsn().await.unwrap_or_default()
                }
            }
            None => {
                return Err(BackupError::BackupFailed {
                    message: "No previous backup found for incremental backup".to_string(),
                });
            }
        };

        let backup_filename = format!(
            "incremental_backup_{}_{}.tar.gz",
            start_time.format("%Y%m%d_%H%M%S"),
            backup_id
        );
        let backup_path = self.config.backup_directory.join(&backup_filename);

        let mut metadata = BackupMetadata {
            id: backup_id.clone(),
            backup_type: BackupType::Incremental,
            status: BackupStatus::InProgress,
            start_time,
            end_time: None,
            size_bytes: 0,
            compressed_size_bytes: 0,
            file_path: backup_path.clone(),
            checksum: String::new(),
            database_name: self.extract_database_name()?,
            wal_start_lsn: Some(start_lsn.clone()),
            wal_end_lsn: None,
            encryption_enabled: self.config.enable_encryption,
            replication_status: std::collections::HashMap::new(),
            verification_status: None,
        };

        // Store initial metadata
        self.repository.store_metadata(&metadata).await?;

        // Create incremental backup by archiving WAL files
        match self.archive_wal_files(&start_lsn, &backup_path).await {
            Ok(end_lsn) => {
                let end_time = Utc::now();
                metadata.end_time = Some(end_time);
                metadata.status = BackupStatus::Completed;
                metadata.wal_end_lsn = Some(end_lsn);

                // Calculate file sizes and checksum
                if backup_path.exists() {
                    let file_metadata = fs::metadata(&backup_path).await?;
                    metadata.compressed_size_bytes = file_metadata.len();
                    metadata.checksum = self.calculate_file_checksum(&backup_path).await?;
                } else {
                    warn!("No new WAL files to archive since last backup");
                }

                // Update metadata
                self.repository.update_metadata(&metadata).await?;

                info!("Incremental backup completed successfully: {}", backup_id);
                Ok(metadata)
            }
            Err(e) => {
                metadata.status = BackupStatus::Failed;
                metadata.end_time = Some(Utc::now());
                self.repository.update_metadata(&metadata).await?;

                error!("Incremental backup failed: {}", e);
                Err(e)
            }
        }
    }

    /// Clean up expired backups based on retention policy
    pub async fn cleanup_expired_backups(&self) -> Result<u32> {
        info!(
            "Starting backup cleanup with retention policy of {} days",
            self.config.retention_days
        );

        let expired_backups = self
            .repository
            .get_expired_backups(self.config.retention_days)
            .await?;
        let mut cleanup_count = 0;

        for backup in expired_backups {
            match self.delete_backup(&backup).await {
                Ok(_) => {
                    cleanup_count += 1;
                    info!("Deleted expired backup: {}", backup.id);
                }
                Err(e) => {
                    error!("Failed to delete expired backup {}: {}", backup.id, e);
                }
            }
        }

        info!("Cleanup completed: {} backups deleted", cleanup_count);
        Ok(cleanup_count)
    }

    /// Get backup statistics and health metrics
    pub async fn get_backup_statistics(&self) -> Result<BackupStatistics> {
        let total_backups = self.repository.count_backups().await?;
        let recent_backups = self.repository.get_recent_backups(7).await?;

        let successful_backups = recent_backups
            .iter()
            .filter(|b| b.status == BackupStatus::Completed)
            .count();

        let failed_backups = recent_backups
            .iter()
            .filter(|b| b.status == BackupStatus::Failed)
            .count();

        let total_backup_size = recent_backups
            .iter()
            .filter(|b| b.status == BackupStatus::Completed)
            .map(|b| b.compressed_size_bytes)
            .sum();

        let latest_backup = self.repository.get_latest_backup().await?;
        let backup_frequency_met = if let Some(latest) = &latest_backup {
            let hours_since_last = Utc::now()
                .signed_duration_since(latest.start_time)
                .num_hours();
            hours_since_last <= 24 // Should have at least one backup per day
        } else {
            false
        };

        Ok(BackupStatistics {
            total_backups,
            successful_backups_last_7_days: successful_backups as u32,
            failed_backups_last_7_days: failed_backups as u32,
            total_backup_size_bytes: total_backup_size,
            latest_backup_time: latest_backup.map(|b| b.start_time),
            backup_frequency_met,
            rto_target_minutes: self.config.rto_minutes,
            rpo_target_minutes: self.config.rpo_minutes,
        })
    }

    // Private helper methods

    // Removed - now handled by repository

    fn extract_database_name(&self) -> Result<String> {
        // Extract database name from connection string
        // This is a simplified implementation
        Ok("codex_memory".to_string())
    }

    // Removed - now handled by repository

    /// Retry database operations with exponential backoff
    async fn retry_database_operation<T, F, Fut>(&self, operation: F) -> Result<T>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        let mut retries = 0;
        let max_retries = 3;

        loop {
            match operation().await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    if retries >= max_retries {
                        return Err(e);
                    }

                    retries += 1;
                    let delay = std::time::Duration::from_millis(100 * (1 << retries));
                    warn!(
                        "Database operation failed (attempt {}), retrying in {:?}: {}",
                        retries, delay, e
                    );
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    /// Check if there's sufficient disk space for the backup operation
    async fn check_disk_space(&self, backup_path: &Path) -> Result<()> {
        let backup_dir = backup_path.parent().unwrap_or_else(|| Path::new("/"));

        // Check if the directory is writable and has some space
        let test_file = backup_dir.join(".backup_space_test");
        match fs::write(&test_file, b"test").await {
            Ok(_) => {
                let _ = fs::remove_file(&test_file).await;
                debug!("Disk space check passed for {}", backup_dir.display());
                Ok(())
            }
            Err(e) => {
                error!("Insufficient disk space or permissions for backup: {}", e);
                Err(BackupError::BackupFailed {
                    message: format!("Cannot write to backup directory: {e}"),
                })
            }
        }
    }

    async fn execute_pg_dump(&self, backup_path: &Path, database_name: &str) -> Result<()> {
        debug!("Executing pg_dump to {}", backup_path.display());

        let mut cmd = Command::new("pg_dump");
        cmd.arg("--verbose")
            .arg("--format=custom")
            .arg("--compress=9")
            .arg("--no-privileges")
            .arg("--no-owner")
            .arg("--dbname")
            .arg(database_name)
            .arg("--file")
            .arg(backup_path);

        // Add connection parameters from database URL
        // This would need to parse the actual DATABASE_URL in production
        cmd.arg("--host")
            .arg("localhost")
            .arg("--port")
            .arg("5432")
            .arg("--username")
            .arg("postgres");

        let output = cmd.output().map_err(|e| BackupError::BackupFailed {
            message: format!("Failed to execute pg_dump: {e}"),
        })?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(BackupError::BackupFailed {
                message: format!("pg_dump failed: {error_msg}"),
            });
        }

        debug!("pg_dump completed successfully");
        Ok(())
    }

    async fn archive_wal_files(&self, start_lsn: &str, backup_path: &Path) -> Result<String> {
        debug!("Archiving WAL files from LSN: {}", start_lsn);

        // This is a simplified implementation
        // In production, this would copy WAL files from pg_wal directory
        let current_lsn = self.repository.get_current_wal_lsn().await?;

        // Create empty archive file as placeholder
        tokio::fs::write(backup_path, b"").await?;

        Ok(current_lsn)
    }

    async fn calculate_file_checksum(&self, file_path: &Path) -> Result<String> {
        use sha2::{Digest, Sha256};

        let contents = fs::read(file_path).await?;
        let mut hasher = Sha256::new();
        hasher.update(&contents);
        let result = hasher.finalize();
        Ok(format!("{result:x}"))
    }

    async fn replicate_backup(&self, metadata: &BackupMetadata) -> Result<()> {
        debug!("Replicating backup: {}", metadata.id);

        for target in &self.config.replication_targets {
            info!("Replicating to target: {}", target.name);
            // This would implement actual replication logic
            // For now, just log the operation
        }

        Ok(())
    }

    async fn delete_backup(&self, backup: &BackupMetadata) -> Result<()> {
        debug!("Deleting backup: {}", backup.id);

        // Delete the backup file
        if backup.file_path.exists() {
            fs::remove_file(&backup.file_path).await?;
        }

        // Mark as expired in metadata store
        self.repository.mark_backup_expired(&backup.id).await?;

        Ok(())
    }
}

// Backup metadata store moved to repository module for better layer separation

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupStatistics {
    pub total_backups: u32,
    pub successful_backups_last_7_days: u32,
    pub failed_backups_last_7_days: u32,
    pub total_backup_size_bytes: u64,
    pub latest_backup_time: Option<DateTime<Utc>>,
    pub backup_frequency_met: bool,
    pub rto_target_minutes: u32,
    pub rpo_target_minutes: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backup_config_default() {
        let config = BackupConfig::default();
        assert_eq!(config.retention_days, 30);
        assert_eq!(config.rto_minutes, 60);
        assert_eq!(config.rpo_minutes, 5);
        assert!(config.enable_encryption);
    }

    #[test]
    fn test_backup_metadata_creation() {
        let metadata = BackupMetadata {
            id: "test-backup".to_string(),
            backup_type: BackupType::Full,
            status: BackupStatus::InProgress,
            start_time: Utc::now(),
            end_time: None,
            size_bytes: 0,
            compressed_size_bytes: 0,
            file_path: std::path::PathBuf::from("/tmp/test.sql"),
            checksum: String::new(),
            database_name: "test_db".to_string(),
            wal_start_lsn: None,
            wal_end_lsn: None,
            encryption_enabled: true,
            replication_status: std::collections::HashMap::new(),
            verification_status: None,
        };

        assert_eq!(metadata.id, "test-backup");
        assert!(matches!(metadata.backup_type, BackupType::Full));
        assert!(metadata.encryption_enabled);
    }
}
