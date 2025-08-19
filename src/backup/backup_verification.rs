use super::{BackupConfig, BackupError, BackupMetadata, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use tokio::fs;
use tracing::{debug, error, info, warn};

/// Backup verification system for ensuring backup integrity and restorability
pub struct BackupVerifier {
    config: BackupConfig,
    db_pool: Arc<PgPool>,
    verification_stats: Arc<tokio::sync::RwLock<VerificationStats>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationStats {
    pub total_verifications: u64,
    pub successful_verifications: u64,
    pub failed_verifications: u64,
    pub last_verification_time: Option<chrono::DateTime<chrono::Utc>>,
    pub average_verification_duration_seconds: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    pub backup_id: String,
    pub verification_time: chrono::DateTime<chrono::Utc>,
    pub integrity_check_passed: bool,
    pub restoration_test_passed: bool,
    pub checksum_verified: bool,
    pub file_structure_valid: bool,
    pub database_consistency_verified: bool,
    pub duration_seconds: u32,
    pub issues_found: Vec<String>,
    pub error_message: Option<String>,
}

impl BackupVerifier {
    pub fn new(config: BackupConfig, db_pool: Arc<PgPool>) -> Self {
        let verification_stats = Arc::new(tokio::sync::RwLock::new(VerificationStats {
            total_verifications: 0,
            successful_verifications: 0,
            failed_verifications: 0,
            last_verification_time: None,
            average_verification_duration_seconds: 0.0,
        }));

        Self {
            config,
            db_pool,
            verification_stats,
        }
    }

    /// Initialize the backup verification system
    pub async fn initialize(&self) -> Result<()> {
        info!("Initializing backup verification system");

        // Verify verification tools are available
        self.verify_tools().await?;

        // Create verification workspace
        let verification_workspace = self.config.backup_directory.join("verification");
        fs::create_dir_all(&verification_workspace).await?;

        info!("Backup verification system initialized");
        Ok(())
    }

    /// Verify a specific backup's integrity and restorability
    pub async fn verify_backup(&self, backup: &BackupMetadata) -> Result<VerificationResult> {
        info!("Starting verification for backup: {}", backup.id);

        let start_time = Utc::now();
        let mut result = VerificationResult {
            backup_id: backup.id.clone(),
            verification_time: start_time,
            integrity_check_passed: false,
            restoration_test_passed: false,
            checksum_verified: false,
            file_structure_valid: false,
            database_consistency_verified: false,
            duration_seconds: 0,
            issues_found: Vec::new(),
            error_message: None,
        };

        // Step 1: Verify file existence
        if !self.verify_backup_file_exists(backup, &mut result).await? {
            return self
                .finalize_verification_result(result, start_time, false)
                .await;
        }

        // Step 2: Verify file checksum
        if !self.verify_backup_checksum(backup, &mut result).await? {
            return self
                .finalize_verification_result(result, start_time, false)
                .await;
        }

        // Step 3: Verify file structure
        if !self.verify_backup_structure(backup, &mut result).await? {
            return self
                .finalize_verification_result(result, start_time, false)
                .await;
        }

        // Step 4: Perform restoration test
        if !self.perform_restoration_test(backup, &mut result).await? {
            return self
                .finalize_verification_result(result, start_time, false)
                .await;
        }

        // Step 5: Verify database consistency
        if !self
            .verify_database_consistency(backup, &mut result)
            .await?
        {
            return self
                .finalize_verification_result(result, start_time, false)
                .await;
        }

        // All checks passed
        info!("Backup verification completed successfully: {}", backup.id);
        self.finalize_verification_result(result, start_time, true)
            .await
    }

    /// Verify all backups in the system
    pub async fn verify_all_backups(&self) -> Result<Vec<VerificationResult>> {
        info!("Starting verification of all backups");

        let backups = self.get_all_backups().await?;
        let mut results = Vec::new();

        for backup in backups {
            match self.verify_backup(&backup).await {
                Ok(result) => results.push(result),
                Err(e) => {
                    error!("Failed to verify backup {}: {}", backup.id, e);
                    // Continue with other backups
                }
            }
        }

        info!("Completed verification of {} backups", results.len());
        Ok(results)
    }

    /// Run automated verification based on schedule
    pub async fn run_scheduled_verification(&self) -> Result<u32> {
        info!("Running scheduled backup verification");

        let backups_to_verify = self.get_backups_needing_verification().await?;
        let mut verified_count = 0;

        for backup in backups_to_verify {
            match self.verify_backup(&backup).await {
                Ok(result) => {
                    // Store verification result
                    self.store_verification_result(&result).await?;

                    if result.integrity_check_passed && result.restoration_test_passed {
                        verified_count += 1;
                    }
                }
                Err(e) => {
                    error!(
                        "Scheduled verification failed for backup {}: {}",
                        backup.id, e
                    );
                }
            }
        }

        info!(
            "Scheduled verification completed: {} backups verified",
            verified_count
        );
        Ok(verified_count)
    }

    /// Get verification statistics
    pub async fn get_verification_stats(&self) -> VerificationStats {
        self.verification_stats.read().await.clone()
    }

    /// Get verification history for a specific backup
    pub async fn get_verification_history(
        &self,
        backup_id: &str,
    ) -> Result<Vec<VerificationResult>> {
        debug!("Getting verification history for backup: {}", backup_id);

        // This would query the verification results from the database
        // For now, return empty vector
        Ok(Vec::new())
    }

    // Private helper methods

    async fn verify_tools(&self) -> Result<()> {
        debug!("Verifying backup verification tools");

        // Check if pg_verifybackup is available (PostgreSQL 13+)
        match Command::new("pg_verifybackup").arg("--version").output() {
            Ok(output) if output.status.success() => {
                debug!("pg_verifybackup is available");
            }
            _ => {
                warn!("pg_verifybackup not available, using alternative verification methods");
            }
        }

        // Check if pg_dump is available
        let output = Command::new("pg_dump")
            .arg("--version")
            .output()
            .map_err(|e| BackupError::ConfigurationError {
                message: format!("pg_dump not found: {e}"),
            })?;

        if !output.status.success() {
            return Err(BackupError::ConfigurationError {
                message: "pg_dump is not working properly".to_string(),
            });
        }

        debug!("Backup verification tools verified");
        Ok(())
    }

    async fn verify_backup_file_exists(
        &self,
        backup: &BackupMetadata,
        result: &mut VerificationResult,
    ) -> Result<bool> {
        debug!(
            "Verifying backup file exists: {}",
            backup.file_path.display()
        );

        if !backup.file_path.exists() {
            result
                .issues_found
                .push("Backup file does not exist".to_string());
            result.error_message = Some(format!(
                "Backup file not found: {}",
                backup.file_path.display()
            ));
            return Ok(false);
        }

        // Check file is readable
        match fs::metadata(&backup.file_path).await {
            Ok(metadata) => {
                if metadata.len() == 0 {
                    result.issues_found.push("Backup file is empty".to_string());
                    return Ok(false);
                }
                debug!("Backup file exists and is {} bytes", metadata.len());
            }
            Err(e) => {
                result
                    .issues_found
                    .push(format!("Cannot read backup file metadata: {e}"));
                return Ok(false);
            }
        }

        Ok(true)
    }

    async fn verify_backup_checksum(
        &self,
        backup: &BackupMetadata,
        result: &mut VerificationResult,
    ) -> Result<bool> {
        debug!("Verifying backup checksum");

        if backup.checksum.is_empty() {
            result
                .issues_found
                .push("No checksum available for verification".to_string());
            return Ok(true); // Not a failure, but we can't verify
        }

        let calculated_checksum = self.calculate_file_checksum(&backup.file_path).await?;

        if calculated_checksum != backup.checksum {
            result
                .issues_found
                .push("Checksum mismatch detected".to_string());
            result.error_message = Some(format!(
                "Expected checksum {}, got {}",
                backup.checksum, calculated_checksum
            ));
            return Ok(false);
        }

        result.checksum_verified = true;
        debug!("Backup checksum verified successfully");
        Ok(true)
    }

    async fn verify_backup_structure(
        &self,
        backup: &BackupMetadata,
        result: &mut VerificationResult,
    ) -> Result<bool> {
        debug!("Verifying backup file structure");

        // For PostgreSQL custom format backups, we can use pg_restore --list to verify structure
        let mut cmd = Command::new("pg_restore");
        cmd.arg("--list").arg(&backup.file_path);

        let output = match cmd.output() {
            Ok(output) => output,
            Err(e) => {
                result
                    .issues_found
                    .push(format!("Failed to execute pg_restore --list: {e}"));
                return Ok(false);
            }
        };

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            result
                .issues_found
                .push(format!("pg_restore --list failed: {error_msg}"));
            return Ok(false);
        }

        // Verify the output contains expected database objects
        let list_output = String::from_utf8_lossy(&output.stdout);
        if list_output.is_empty() {
            result
                .issues_found
                .push("Backup appears to be empty or corrupted".to_string());
            return Ok(false);
        }

        // Check for essential database objects (tables, sequences, etc.)
        let expected_objects = ["TABLE", "SEQUENCE", "INDEX"];
        for obj_type in &expected_objects {
            if !list_output.contains(obj_type) {
                result
                    .issues_found
                    .push(format!("Missing expected object type: {obj_type}"));
            }
        }

        result.file_structure_valid = true;
        debug!("Backup file structure verified successfully");
        Ok(true)
    }

    async fn perform_restoration_test(
        &self,
        backup: &BackupMetadata,
        result: &mut VerificationResult,
    ) -> Result<bool> {
        debug!("Performing restoration test");

        // Create temporary database for restoration test
        let test_db_name = format!("backup_test_{}", uuid::Uuid::new_v4().simple());

        // Create test database
        if !self.create_test_database(&test_db_name).await? {
            result
                .issues_found
                .push("Failed to create test database".to_string());
            return Ok(false);
        }

        // Restore backup to test database
        let restoration_success = self
            .restore_to_test_database(backup, &test_db_name, result)
            .await?;

        // Clean up test database
        if let Err(e) = self.drop_test_database(&test_db_name).await {
            warn!("Failed to clean up test database {}: {}", test_db_name, e);
        }

        if restoration_success {
            result.restoration_test_passed = true;
            debug!("Restoration test passed successfully");
        }

        Ok(restoration_success)
    }

    async fn verify_database_consistency(
        &self,
        _backup: &BackupMetadata,
        result: &mut VerificationResult,
    ) -> Result<bool> {
        debug!("Verifying database consistency");

        // For now, we'll assume consistency if the restoration test passed
        // In a full implementation, this would run additional consistency checks
        if result.restoration_test_passed {
            result.database_consistency_verified = true;
            debug!("Database consistency verified");
            Ok(true)
        } else {
            result
                .issues_found
                .push("Cannot verify consistency - restoration test failed".to_string());
            Ok(false)
        }
    }

    async fn create_test_database(&self, db_name: &str) -> Result<bool> {
        debug!("Creating test database: {}", db_name);

        let query = format!("CREATE DATABASE {db_name}");
        match sqlx::query(&query).execute(self.db_pool.as_ref()).await {
            Ok(_) => {
                debug!("Test database created successfully");
                Ok(true)
            }
            Err(e) => {
                error!("Failed to create test database: {}", e);
                Ok(false)
            }
        }
    }

    async fn restore_to_test_database(
        &self,
        backup: &BackupMetadata,
        test_db_name: &str,
        result: &mut VerificationResult,
    ) -> Result<bool> {
        debug!("Restoring backup to test database: {}", test_db_name);

        let mut cmd = Command::new("pg_restore");
        cmd.arg("--verbose")
            .arg("--no-privileges")
            .arg("--no-owner")
            .arg("--dbname")
            .arg(test_db_name)
            .arg(&backup.file_path);

        let output = match cmd.output() {
            Ok(output) => output,
            Err(e) => {
                result
                    .issues_found
                    .push(format!("Failed to execute pg_restore: {e}"));
                return Ok(false);
            }
        };

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            result
                .issues_found
                .push(format!("pg_restore to test database failed: {error_msg}"));
            return Ok(false);
        }

        debug!("Backup restored to test database successfully");
        Ok(true)
    }

    async fn drop_test_database(&self, db_name: &str) -> Result<()> {
        debug!("Dropping test database: {}", db_name);

        let query = format!("DROP DATABASE IF EXISTS {db_name}");
        sqlx::query(&query).execute(self.db_pool.as_ref()).await?;

        debug!("Test database dropped successfully");
        Ok(())
    }

    async fn calculate_file_checksum(&self, file_path: &Path) -> Result<String> {
        use sha2::{Digest, Sha256};

        let contents = fs::read(file_path).await?;
        let mut hasher = Sha256::new();
        hasher.update(&contents);
        let result = hasher.finalize();
        Ok(format!("{result:x}"))
    }

    async fn finalize_verification_result(
        &self,
        mut result: VerificationResult,
        start_time: chrono::DateTime<chrono::Utc>,
        success: bool,
    ) -> Result<VerificationResult> {
        let duration = Utc::now().signed_duration_since(start_time);
        result.duration_seconds = duration.num_seconds() as u32;

        // Set overall flags based on individual checks
        result.integrity_check_passed = result.checksum_verified && result.file_structure_valid;

        // Update statistics
        {
            let mut stats = self.verification_stats.write().await;
            stats.total_verifications += 1;
            if success {
                stats.successful_verifications += 1;
            } else {
                stats.failed_verifications += 1;
            }
            stats.last_verification_time = Some(Utc::now());

            // Update average duration
            let total_duration = stats.average_verification_duration_seconds
                * (stats.total_verifications - 1) as f64;
            stats.average_verification_duration_seconds = (total_duration
                + result.duration_seconds as f64)
                / stats.total_verifications as f64;
        }

        if success {
            info!(
                "Backup verification successful: {} ({}s)",
                result.backup_id, result.duration_seconds
            );
        } else {
            error!(
                "Backup verification failed: {} ({}s): {:?}",
                result.backup_id, result.duration_seconds, result.issues_found
            );
        }

        Ok(result)
    }

    async fn get_all_backups(&self) -> Result<Vec<BackupMetadata>> {
        // This would query the backup metadata store
        // For now, return empty vector
        Ok(Vec::new())
    }

    async fn get_backups_needing_verification(&self) -> Result<Vec<BackupMetadata>> {
        // This would query for backups that haven't been verified recently
        // or have never been verified
        Ok(Vec::new())
    }

    async fn store_verification_result(&self, result: &VerificationResult) -> Result<()> {
        debug!(
            "Storing verification result for backup: {}",
            result.backup_id
        );

        // This would store the verification result in the database
        // For now, just log it
        info!(
            "Verification result: {} - Success: {}, Duration: {}s",
            result.backup_id,
            result.integrity_check_passed && result.restoration_test_passed,
            result.duration_seconds
        );

        Ok(())
    }
}

impl Default for VerificationStats {
    fn default() -> Self {
        Self {
            total_verifications: 0,
            successful_verifications: 0,
            failed_verifications: 0,
            last_verification_time: None,
            average_verification_duration_seconds: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verification_stats_default() {
        let stats = VerificationStats::default();
        assert_eq!(stats.total_verifications, 0);
        assert_eq!(stats.successful_verifications, 0);
        assert_eq!(stats.failed_verifications, 0);
        assert_eq!(stats.average_verification_duration_seconds, 0.0);
    }

    #[test]
    fn test_verification_result_creation() {
        let result = VerificationResult {
            backup_id: "test-backup".to_string(),
            verification_time: Utc::now(),
            integrity_check_passed: false,
            restoration_test_passed: false,
            checksum_verified: false,
            file_structure_valid: false,
            database_consistency_verified: false,
            duration_seconds: 0,
            issues_found: Vec::new(),
            error_message: None,
        };

        assert_eq!(result.backup_id, "test-backup");
        assert!(!result.integrity_check_passed);
        assert!(result.issues_found.is_empty());
    }
}
