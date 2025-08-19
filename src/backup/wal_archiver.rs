use super::{BackupConfig, BackupError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use tokio::fs;
use tracing::{debug, error, info, warn};

/// WAL (Write-Ahead Log) archiver for continuous backup capability
pub struct WalArchiver {
    config: BackupConfig,
    db_pool: Arc<PgPool>,
    archive_status: Arc<tokio::sync::RwLock<ArchiveStatus>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveStatus {
    pub is_archiving: bool,
    pub last_archived_wal: Option<String>,
    pub last_archive_time: Option<DateTime<Utc>>,
    pub archived_wal_count: u64,
    pub failed_archives: u64,
    pub archive_lag_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalFile {
    pub name: String,
    pub path: PathBuf,
    pub size_bytes: u64,
    pub created_time: DateTime<Utc>,
    pub archived_time: Option<DateTime<Utc>>,
    pub checksum: String,
}

impl WalArchiver {
    pub fn new(config: BackupConfig, db_pool: Arc<PgPool>) -> Self {
        let archive_status = Arc::new(tokio::sync::RwLock::new(ArchiveStatus {
            is_archiving: false,
            last_archived_wal: None,
            last_archive_time: None,
            archived_wal_count: 0,
            failed_archives: 0,
            archive_lag_seconds: 0,
        }));

        Self {
            config,
            db_pool,
            archive_status,
        }
    }

    /// Initialize WAL archiving
    pub async fn initialize(&self) -> Result<()> {
        info!("Initializing WAL archiver");

        // Create WAL archive directory
        fs::create_dir_all(&self.config.wal_archive_directory).await?;

        // Verify PostgreSQL archiving configuration
        self.verify_archiving_config().await?;

        // Set up archive command if not configured
        self.configure_archive_command().await?;

        info!("WAL archiver initialized successfully");
        Ok(())
    }

    /// Start continuous WAL archiving
    pub async fn start_archiving(&self) -> Result<()> {
        info!("Starting WAL archiving");

        {
            let mut status = self.archive_status.write().await;
            status.is_archiving = true;
        }

        // Enable archiving in PostgreSQL
        self.enable_postgresql_archiving().await?;

        // Start monitoring task
        let archiver = self.clone();
        tokio::spawn(async move {
            archiver.archive_monitoring_loop().await;
        });

        info!("WAL archiving started");
        Ok(())
    }

    /// Stop WAL archiving
    pub async fn stop_archiving(&self) -> Result<()> {
        info!("Stopping WAL archiving");

        {
            let mut status = self.archive_status.write().await;
            status.is_archiving = false;
        }

        // Disable archiving in PostgreSQL
        self.disable_postgresql_archiving().await?;

        info!("WAL archiving stopped");
        Ok(())
    }

    /// Archive a specific WAL file
    pub async fn archive_wal_file(&self, wal_filename: &str, wal_path: &Path) -> Result<WalFile> {
        debug!("Archiving WAL file: {}", wal_filename);

        let archive_path = self.config.wal_archive_directory.join(wal_filename);

        // Copy WAL file to archive directory
        fs::copy(wal_path, &archive_path).await?;

        // Calculate checksum
        let checksum = self.calculate_wal_checksum(&archive_path).await?;

        // Get file metadata
        let metadata = fs::metadata(&archive_path).await?;
        let file_size = metadata.len();

        let wal_file = WalFile {
            name: wal_filename.to_string(),
            path: archive_path,
            size_bytes: file_size,
            created_time: Utc::now(), // This would be extracted from actual WAL file in production
            archived_time: Some(Utc::now()),
            checksum,
        };

        // Update archive status
        {
            let mut status = self.archive_status.write().await;
            status.last_archived_wal = Some(wal_filename.to_string());
            status.last_archive_time = Some(Utc::now());
            status.archived_wal_count += 1;
        }

        // Optionally compress the WAL file
        if self.should_compress_wal() {
            self.compress_wal_file(&wal_file.path).await?;
        }

        // Optionally encrypt the WAL file
        if self.config.enable_encryption {
            self.encrypt_wal_file(&wal_file.path).await?;
        }

        info!(
            "WAL file archived successfully: {} ({} bytes)",
            wal_filename, file_size
        );
        Ok(wal_file)
    }

    /// Get list of archived WAL files
    pub async fn get_archived_wal_files(&self) -> Result<Vec<WalFile>> {
        debug!("Getting list of archived WAL files");

        let mut wal_files = Vec::new();
        let mut entries = fs::read_dir(&self.config.wal_archive_directory).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_file() {
                if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                    // Check if this looks like a WAL file
                    if self.is_wal_filename(filename) {
                        let metadata = fs::metadata(&path).await?;
                        let checksum = self.calculate_wal_checksum(&path).await?;

                        let wal_file = WalFile {
                            name: filename.to_string(),
                            path: path.clone(),
                            size_bytes: metadata.len(),
                            created_time: DateTime::from_timestamp(
                                metadata
                                    .created()
                                    .unwrap_or(std::time::UNIX_EPOCH)
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap()
                                    .as_secs() as i64,
                                0,
                            )
                            .unwrap_or_else(Utc::now),
                            archived_time: Some(
                                DateTime::from_timestamp(
                                    metadata
                                        .modified()
                                        .unwrap_or(std::time::UNIX_EPOCH)
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap()
                                        .as_secs() as i64,
                                    0,
                                )
                                .unwrap_or_else(Utc::now),
                            ),
                            checksum,
                        };

                        wal_files.push(wal_file);
                    }
                }
            }
        }

        // Sort by creation time
        wal_files.sort_by(|a, b| a.created_time.cmp(&b.created_time));

        debug!("Found {} archived WAL files", wal_files.len());
        Ok(wal_files)
    }

    /// Clean up old WAL archives based on retention policy
    pub async fn cleanup_old_wal_files(&self) -> Result<u32> {
        info!("Starting WAL archive cleanup");

        let wal_files = self.get_archived_wal_files().await?;
        let retention_cutoff =
            Utc::now() - chrono::Duration::days(self.config.retention_days as i64);

        let mut cleanup_count = 0;

        for wal_file in wal_files {
            if wal_file.archived_time.unwrap_or(Utc::now()) < retention_cutoff {
                match fs::remove_file(&wal_file.path).await {
                    Ok(_) => {
                        cleanup_count += 1;
                        debug!("Deleted old WAL file: {}", wal_file.name);
                    }
                    Err(e) => {
                        error!("Failed to delete old WAL file {}: {}", wal_file.name, e);
                    }
                }
            }
        }

        info!(
            "WAL archive cleanup completed: {} files deleted",
            cleanup_count
        );
        Ok(cleanup_count)
    }

    /// Get current archive status
    pub async fn get_archive_status(&self) -> ArchiveStatus {
        self.archive_status.read().await.clone()
    }

    /// Restore WAL files for point-in-time recovery
    pub async fn restore_wal_files(
        &self,
        target_lsn: &str,
        restore_directory: &Path,
    ) -> Result<Vec<WalFile>> {
        info!("Restoring WAL files up to LSN: {}", target_lsn);

        let wal_files = self.get_archived_wal_files().await?;
        let mut restored_files = Vec::new();

        // Create restore directory
        fs::create_dir_all(restore_directory).await?;

        for wal_file in wal_files {
            // In a real implementation, we'd parse the LSN from WAL file names
            // and only restore files needed for the target LSN
            let restore_path = restore_directory.join(&wal_file.name);

            // Decrypt if encrypted
            let source_path = if self.config.enable_encryption {
                let decrypted_path = self.decrypt_wal_file(&wal_file.path).await?;
                decrypted_path
            } else {
                wal_file.path.clone()
            };

            // Decompress if compressed
            let final_source = if self.is_compressed_wal(&source_path) {
                self.decompress_wal_file(&source_path).await?
            } else {
                source_path
            };

            fs::copy(&final_source, &restore_path).await?;

            let mut restored_wal = wal_file.clone();
            restored_wal.path = restore_path;
            restored_files.push(restored_wal);
        }

        info!("Restored {} WAL files for recovery", restored_files.len());
        Ok(restored_files)
    }

    // Private helper methods

    async fn verify_archiving_config(&self) -> Result<()> {
        debug!("Verifying PostgreSQL archiving configuration");

        // Check WAL level
        let wal_level: String = sqlx::query_scalar("SHOW wal_level")
            .fetch_one(self.db_pool.as_ref())
            .await?;

        if wal_level != "replica" && wal_level != "logical" {
            return Err(BackupError::ConfigurationError {
                message: format!(
                    "WAL level must be 'replica' or 'logical' for archiving, found: {wal_level}"
                ),
            });
        }

        debug!("WAL archiving configuration verified");
        Ok(())
    }

    async fn configure_archive_command(&self) -> Result<()> {
        debug!("Configuring PostgreSQL archive command");

        // In a production system, this would set up the archive_command
        // For now, we'll just verify the current setting
        let archive_command: Option<String> = sqlx::query_scalar("SHOW archive_command")
            .fetch_optional(self.db_pool.as_ref())
            .await?;

        match archive_command {
            Some(cmd) if !cmd.is_empty() && cmd != "false" => {
                info!("Archive command already configured: {}", cmd);
            }
            _ => {
                warn!("Archive command not configured. WAL archiving may not work properly.");
            }
        }

        Ok(())
    }

    async fn enable_postgresql_archiving(&self) -> Result<()> {
        debug!("Enabling PostgreSQL archiving");

        // Check current archive mode
        let archive_mode: String = sqlx::query_scalar("SHOW archive_mode")
            .fetch_one(self.db_pool.as_ref())
            .await?;

        if archive_mode != "on" {
            warn!("Archive mode is not enabled. This requires a PostgreSQL restart to change.");
            // In production, this might trigger a configuration update and restart
        }

        Ok(())
    }

    async fn disable_postgresql_archiving(&self) -> Result<()> {
        debug!("Disabling PostgreSQL archiving");
        // This would typically involve updating PostgreSQL configuration
        // For this implementation, we'll just log the action
        Ok(())
    }

    async fn archive_monitoring_loop(&self) {
        info!("Starting WAL archive monitoring loop");

        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));

        loop {
            interval.tick().await;

            let is_archiving = {
                let status = self.archive_status.read().await;
                status.is_archiving
            };

            if !is_archiving {
                break;
            }

            // Monitor archive lag
            if let Err(e) = self.check_archive_lag().await {
                error!("Error checking archive lag: {}", e);
            }
        }

        info!("WAL archive monitoring loop stopped");
    }

    async fn check_archive_lag(&self) -> Result<()> {
        // Query PostgreSQL for current WAL position and last archived position
        let _current_lsn: String = sqlx::query_scalar("SELECT pg_current_wal_lsn()")
            .fetch_one(self.db_pool.as_ref())
            .await?;

        let last_archived_lsn: Option<String> =
            sqlx::query_scalar("SELECT last_archived_wal FROM pg_stat_archiver")
                .fetch_optional(self.db_pool.as_ref())
                .await?;

        // Calculate lag (simplified - in production would parse LSN values)
        let lag_seconds = if let Some(_last_lsn) = last_archived_lsn {
            // This would calculate actual LSN difference
            0
        } else {
            // No archived WAL yet
            300 // 5 minutes default
        };

        // Update status
        {
            let mut status = self.archive_status.write().await;
            status.archive_lag_seconds = lag_seconds;
        }

        if lag_seconds > 300 {
            // More than 5 minutes lag
            warn!("High WAL archive lag detected: {} seconds", lag_seconds);
        }

        Ok(())
    }

    async fn calculate_wal_checksum(&self, wal_path: &Path) -> Result<String> {
        use sha2::{Digest, Sha256};

        let contents = fs::read(wal_path).await?;
        let mut hasher = Sha256::new();
        hasher.update(&contents);
        let result = hasher.finalize();
        Ok(format!("{result:x}"))
    }

    fn should_compress_wal(&self) -> bool {
        Self::should_compress_wal_static()
    }

    fn should_compress_wal_static() -> bool {
        // Compress WAL files to save space
        true
    }

    async fn compress_wal_file(&self, wal_path: &Path) -> Result<PathBuf> {
        debug!("Compressing WAL file: {}", wal_path.display());

        let compressed_path = wal_path.with_extension("gz");

        let mut cmd = Command::new("gzip");
        cmd.arg("--force").arg(wal_path);

        let output = cmd.output().map_err(|e| BackupError::BackupFailed {
            message: format!("Failed to compress WAL file: {e}"),
        })?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(BackupError::BackupFailed {
                message: format!("WAL compression failed: {error_msg}"),
            });
        }

        Ok(compressed_path)
    }

    async fn encrypt_wal_file(&self, wal_path: &Path) -> Result<PathBuf> {
        debug!("Encrypting WAL file: {}", wal_path.display());

        // This would implement actual encryption using the configured key
        // For now, just return the original path
        Ok(wal_path.to_path_buf())
    }

    async fn decrypt_wal_file(&self, encrypted_path: &Path) -> Result<PathBuf> {
        debug!("Decrypting WAL file: {}", encrypted_path.display());

        // This would implement actual decryption
        // For now, just return the original path
        Ok(encrypted_path.to_path_buf())
    }

    fn is_compressed_wal(&self, path: &Path) -> bool {
        Self::is_compressed_wal_static(path)
    }

    fn is_compressed_wal_static(path: &Path) -> bool {
        path.extension().and_then(|ext| ext.to_str()) == Some("gz")
    }

    async fn decompress_wal_file(&self, compressed_path: &Path) -> Result<PathBuf> {
        debug!("Decompressing WAL file: {}", compressed_path.display());

        let decompressed_path = compressed_path.with_extension("");

        let mut cmd = Command::new("gunzip");
        cmd.arg("--force").arg("--keep").arg(compressed_path);

        let output = cmd.output().map_err(|e| BackupError::BackupFailed {
            message: format!("Failed to decompress WAL file: {e}"),
        })?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(BackupError::BackupFailed {
                message: format!("WAL decompression failed: {error_msg}"),
            });
        }

        Ok(decompressed_path)
    }

    fn is_wal_filename(&self, filename: &str) -> bool {
        Self::is_wal_filename_static(filename)
    }

    fn is_wal_filename_static(filename: &str) -> bool {
        // Check if filename follows PostgreSQL WAL naming convention
        // WAL files are typically 24 characters long hexadecimal names
        filename.len() == 24 && filename.chars().all(|c| c.is_ascii_hexdigit())
            || filename.contains(".partial") // Partial WAL files
            || filename.contains(".backup") // Backup label files
    }
}

impl Clone for WalArchiver {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            db_pool: self.db_pool.clone(),
            archive_status: self.archive_status.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wal_filename_validation() {
        // These tests don't need a database connection since they're just testing filename patterns

        // Valid WAL filename (24 hex characters)
        assert!(WalArchiver::is_wal_filename_static(
            "000000010000000000000001"
        ));

        // Invalid WAL filename
        assert!(!WalArchiver::is_wal_filename_static("invalid_filename"));
        assert!(!WalArchiver::is_wal_filename_static("short"));

        // Partial WAL files
        assert!(WalArchiver::is_wal_filename_static(
            "000000010000000000000001.partial"
        ));
    }

    #[test]
    fn test_archive_status_default() {
        let status = ArchiveStatus {
            is_archiving: false,
            last_archived_wal: None,
            last_archive_time: None,
            archived_wal_count: 0,
            failed_archives: 0,
            archive_lag_seconds: 0,
        };

        assert!(!status.is_archiving);
        assert_eq!(status.archived_wal_count, 0);
    }

    #[test]
    fn test_should_compress_wal() {
        // Test static method that doesn't require database connection
        assert!(WalArchiver::should_compress_wal_static());
    }

    #[test]
    fn test_is_compressed_wal() {
        // Test static method that doesn't require database connection
        assert!(WalArchiver::is_compressed_wal_static(Path::new("test.gz")));
        assert!(!WalArchiver::is_compressed_wal_static(Path::new(
            "test.txt"
        )));
    }
}
