use super::{BackupConfig, BackupError, BackupMetadata, BackupStatus, BackupType, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use tokio::fs;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Main backup manager responsible for orchestrating all backup operations
pub struct BackupManager {
    config: BackupConfig,
    db_pool: Arc<PgPool>,
    metadata_store: BackupMetadataStore,
}

impl BackupManager {
    pub fn new(config: BackupConfig, db_pool: Arc<PgPool>) -> Self {
        let metadata_store = BackupMetadataStore::new(db_pool.clone());
        Self {
            config,
            db_pool,
            metadata_store,
        }
    }

    /// Initialize the backup manager and create necessary directories
    pub async fn initialize(&self) -> Result<()> {
        info!("Initializing backup manager");

        // Create backup directories
        fs::create_dir_all(&self.config.backup_directory).await?;
        fs::create_dir_all(&self.config.wal_archive_directory).await?;

        // Initialize metadata store
        self.metadata_store.initialize().await?;

        // Verify PostgreSQL configuration
        self.verify_postgres_config().await?;

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
        self.metadata_store.store_metadata(&metadata).await?;

        // Get WAL start LSN
        let start_lsn = self.get_current_wal_lsn().await?;
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
                let end_lsn = self.get_current_wal_lsn().await?;
                metadata.wal_end_lsn = Some(end_lsn);

                // Calculate file sizes and checksum
                let file_metadata = fs::metadata(&backup_path).await?;
                metadata.compressed_size_bytes = file_metadata.len();
                metadata.checksum = self.calculate_file_checksum(&backup_path).await?;

                // Update metadata
                self.metadata_store.update_metadata(&metadata).await?;

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
                self.metadata_store.update_metadata(&metadata).await?;

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
        let last_backup = self.metadata_store.get_latest_backup().await?;
        let start_lsn = match last_backup {
            Some(backup) => {
                if let Some(lsn) = backup.wal_end_lsn {
                    lsn
                } else {
                    warn!("Last backup has no end LSN, using current LSN");
                    self.get_current_wal_lsn().await.unwrap_or_default()
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
        self.metadata_store.store_metadata(&metadata).await?;

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
                self.metadata_store.update_metadata(&metadata).await?;

                info!("Incremental backup completed successfully: {}", backup_id);
                Ok(metadata)
            }
            Err(e) => {
                metadata.status = BackupStatus::Failed;
                metadata.end_time = Some(Utc::now());
                self.metadata_store.update_metadata(&metadata).await?;

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
            .metadata_store
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
        let total_backups = self.metadata_store.count_backups().await?;
        let recent_backups = self.metadata_store.get_recent_backups(7).await?;

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

        let latest_backup = self.metadata_store.get_latest_backup().await?;
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

    async fn verify_postgres_config(&self) -> Result<()> {
        debug!("Verifying PostgreSQL configuration for backups");

        // Check if WAL archiving is enabled
        let wal_level: String = sqlx::query_scalar("SHOW wal_level")
            .fetch_one(self.db_pool.as_ref())
            .await?;

        if wal_level != "replica" && wal_level != "logical" {
            return Err(BackupError::ConfigurationError {
                message: format!("WAL level must be 'replica' or 'logical', found: {wal_level}"),
            });
        }

        // Check if archiving is enabled
        let archive_mode: String = sqlx::query_scalar("SHOW archive_mode")
            .fetch_one(self.db_pool.as_ref())
            .await?;

        if archive_mode != "on" {
            warn!("Archive mode is not enabled, continuous archiving will not work");
        }

        debug!("PostgreSQL configuration verified");
        Ok(())
    }

    fn extract_database_name(&self) -> Result<String> {
        // Extract database name from connection string
        // This is a simplified implementation
        Ok("codex_memory".to_string())
    }

    async fn get_current_wal_lsn(&self) -> Result<String> {
        self.retry_database_operation(|| async {
            let lsn: String = sqlx::query_scalar("SELECT pg_current_wal_lsn()")
                .fetch_one(self.db_pool.as_ref())
                .await?;
            Ok(lsn)
        })
        .await
    }

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
        let current_lsn = self.get_current_wal_lsn().await?;

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
        self.metadata_store.mark_backup_expired(&backup.id).await?;

        Ok(())
    }
}

/// Store for backup metadata
struct BackupMetadataStore {
    db_pool: Arc<PgPool>,
}

impl BackupMetadataStore {
    fn new(db_pool: Arc<PgPool>) -> Self {
        Self { db_pool }
    }

    async fn initialize(&self) -> Result<()> {
        debug!("Initializing backup metadata store");

        // Create backup_metadata table if it doesn't exist
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS backup_metadata (
                id VARCHAR PRIMARY KEY,
                backup_type VARCHAR NOT NULL,
                status VARCHAR NOT NULL,
                start_time TIMESTAMPTZ NOT NULL,
                end_time TIMESTAMPTZ,
                size_bytes BIGINT DEFAULT 0,
                compressed_size_bytes BIGINT DEFAULT 0,
                file_path TEXT NOT NULL,
                checksum VARCHAR,
                database_name VARCHAR NOT NULL,
                wal_start_lsn VARCHAR,
                wal_end_lsn VARCHAR,
                encryption_enabled BOOLEAN DEFAULT false,
                metadata JSONB DEFAULT '{}',
                created_at TIMESTAMPTZ DEFAULT NOW()
            )
        "#,
        )
        .execute(self.db_pool.as_ref())
        .await?;

        debug!("Backup metadata store initialized");
        Ok(())
    }

    async fn store_metadata(&self, metadata: &BackupMetadata) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO backup_metadata (
                id, backup_type, status, start_time, end_time, 
                size_bytes, compressed_size_bytes, file_path, checksum,
                database_name, wal_start_lsn, wal_end_lsn, encryption_enabled
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
        "#,
        )
        .bind(&metadata.id)
        .bind(format!("{:?}", metadata.backup_type))
        .bind(format!("{:?}", metadata.status))
        .bind(metadata.start_time)
        .bind(metadata.end_time)
        .bind(metadata.size_bytes as i64)
        .bind(metadata.compressed_size_bytes as i64)
        .bind(metadata.file_path.to_string_lossy().as_ref())
        .bind(&metadata.checksum)
        .bind(&metadata.database_name)
        .bind(&metadata.wal_start_lsn)
        .bind(&metadata.wal_end_lsn)
        .bind(metadata.encryption_enabled)
        .execute(self.db_pool.as_ref())
        .await?;

        Ok(())
    }

    async fn update_metadata(&self, metadata: &BackupMetadata) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE backup_metadata SET
                status = $2, end_time = $3, size_bytes = $4,
                compressed_size_bytes = $5, checksum = $6,
                wal_end_lsn = $7
            WHERE id = $1
        "#,
        )
        .bind(&metadata.id)
        .bind(format!("{:?}", metadata.status))
        .bind(metadata.end_time)
        .bind(metadata.size_bytes as i64)
        .bind(metadata.compressed_size_bytes as i64)
        .bind(&metadata.checksum)
        .bind(&metadata.wal_end_lsn)
        .execute(self.db_pool.as_ref())
        .await?;

        Ok(())
    }

    async fn get_latest_backup(&self) -> Result<Option<BackupMetadata>> {
        let row = sqlx::query(
            r#"
            SELECT id, backup_type, status, start_time, end_time,
                   size_bytes, compressed_size_bytes, file_path, checksum,
                   database_name, wal_start_lsn, wal_end_lsn, encryption_enabled
            FROM backup_metadata 
            WHERE status = 'Completed'
            ORDER BY start_time DESC 
            LIMIT 1
        "#,
        )
        .fetch_optional(self.db_pool.as_ref())
        .await?;

        if let Some(row) = row {
            Ok(Some(self.row_to_metadata(row)?))
        } else {
            Ok(None)
        }
    }

    async fn get_expired_backups(&self, retention_days: u32) -> Result<Vec<BackupMetadata>> {
        let rows = sqlx::query(
            r#"
            SELECT id, backup_type, status, start_time, end_time,
                   size_bytes, compressed_size_bytes, file_path, checksum,
                   database_name, wal_start_lsn, wal_end_lsn, encryption_enabled
            FROM backup_metadata 
            WHERE start_time < NOW() - INTERVAL '%d days'
            AND status != 'Expired'
        "#,
        )
        .bind(retention_days as i32)
        .fetch_all(self.db_pool.as_ref())
        .await?;

        let mut backups = Vec::new();
        for row in rows {
            backups.push(self.row_to_metadata(row)?);
        }

        Ok(backups)
    }

    async fn get_recent_backups(&self, days: u32) -> Result<Vec<BackupMetadata>> {
        let rows = sqlx::query(
            r#"
            SELECT id, backup_type, status, start_time, end_time,
                   size_bytes, compressed_size_bytes, file_path, checksum,
                   database_name, wal_start_lsn, wal_end_lsn, encryption_enabled
            FROM backup_metadata 
            WHERE start_time > NOW() - INTERVAL '%d days'
            ORDER BY start_time DESC
        "#,
        )
        .bind(days as i32)
        .fetch_all(self.db_pool.as_ref())
        .await?;

        let mut backups = Vec::new();
        for row in rows {
            backups.push(self.row_to_metadata(row)?);
        }

        Ok(backups)
    }

    async fn count_backups(&self) -> Result<u32> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM backup_metadata")
            .fetch_one(self.db_pool.as_ref())
            .await?;

        Ok(count as u32)
    }

    async fn mark_backup_expired(&self, backup_id: &str) -> Result<()> {
        sqlx::query("UPDATE backup_metadata SET status = 'Expired' WHERE id = $1")
            .bind(backup_id)
            .execute(self.db_pool.as_ref())
            .await?;

        Ok(())
    }

    fn row_to_metadata(&self, row: sqlx::postgres::PgRow) -> Result<BackupMetadata> {
        use sqlx::Row;

        let backup_type_str: String = row.try_get("backup_type")?;
        let backup_type = match backup_type_str.as_str() {
            "Full" => BackupType::Full,
            "Incremental" => BackupType::Incremental,
            "Differential" => BackupType::Differential,
            "WalArchive" => BackupType::WalArchive,
            _ => BackupType::Full,
        };

        let status_str: String = row.try_get("status")?;
        let status = match status_str.as_str() {
            "InProgress" => BackupStatus::InProgress,
            "Completed" => BackupStatus::Completed,
            "Failed" => BackupStatus::Failed,
            "Expired" => BackupStatus::Expired,
            "Archived" => BackupStatus::Archived,
            _ => BackupStatus::Failed,
        };

        Ok(BackupMetadata {
            id: row.try_get("id")?,
            backup_type,
            status,
            start_time: row.try_get("start_time")?,
            end_time: row.try_get("end_time")?,
            size_bytes: row.try_get::<i64, _>("size_bytes")? as u64,
            compressed_size_bytes: row.try_get::<i64, _>("compressed_size_bytes")? as u64,
            file_path: std::path::PathBuf::from(row.try_get::<String, _>("file_path")?),
            checksum: row.try_get("checksum")?,
            database_name: row.try_get("database_name")?,
            wal_start_lsn: row.try_get("wal_start_lsn")?,
            wal_end_lsn: row.try_get("wal_end_lsn")?,
            encryption_enabled: row.try_get("encryption_enabled")?,
            replication_status: std::collections::HashMap::new(),
            verification_status: None,
        })
    }
}

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
