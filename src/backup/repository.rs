use super::{BackupError, BackupMetadata, BackupStatus, BackupType, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use std::sync::Arc;
use tracing::debug;

/// Repository abstraction for backup metadata operations
#[async_trait]
pub trait BackupRepository: Send + Sync + std::fmt::Debug {
    async fn initialize(&self) -> Result<()>;
    async fn store_metadata(&self, metadata: &BackupMetadata) -> Result<()>;
    async fn update_metadata(&self, metadata: &BackupMetadata) -> Result<()>;
    async fn get_latest_backup(&self) -> Result<Option<BackupMetadata>>;
    async fn get_expired_backups(&self, retention_days: u32) -> Result<Vec<BackupMetadata>>;
    async fn get_recent_backups(&self, days: u32) -> Result<Vec<BackupMetadata>>;
    async fn count_backups(&self) -> Result<u32>;
    async fn mark_backup_expired(&self, backup_id: &str) -> Result<()>;
    async fn get_current_wal_lsn(&self) -> Result<String>;
    async fn verify_postgres_config(&self) -> Result<()>;
}

/// PostgreSQL implementation of backup repository
#[derive(Debug)]
pub struct PostgresBackupRepository {
    db_pool: Arc<PgPool>,
}

impl PostgresBackupRepository {
    pub fn new(db_pool: Arc<PgPool>) -> Self {
        Self { db_pool }
    }
}

#[async_trait]
impl BackupRepository for PostgresBackupRepository {
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

    async fn get_current_wal_lsn(&self) -> Result<String> {
        let lsn: String = sqlx::query_scalar("SELECT pg_current_wal_lsn()")
            .fetch_one(self.db_pool.as_ref())
            .await?;
        Ok(lsn)
    }

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
            tracing::warn!("Archive mode is not enabled, continuous archiving will not work");
        }

        debug!("PostgreSQL configuration verified");
        Ok(())
    }
}

impl PostgresBackupRepository {
    fn row_to_metadata(&self, row: sqlx::postgres::PgRow) -> Result<BackupMetadata> {
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

/// Mock repository for testing
#[derive(Debug)]
pub struct MockBackupRepository;

#[async_trait]
impl BackupRepository for MockBackupRepository {
    async fn initialize(&self) -> Result<()> { Ok(()) }
    async fn store_metadata(&self, _metadata: &BackupMetadata) -> Result<()> { Ok(()) }
    async fn update_metadata(&self, _metadata: &BackupMetadata) -> Result<()> { Ok(()) }
    async fn get_latest_backup(&self) -> Result<Option<BackupMetadata>> { Ok(None) }
    async fn get_expired_backups(&self, _retention_days: u32) -> Result<Vec<BackupMetadata>> { Ok(vec![]) }
    async fn get_recent_backups(&self, _days: u32) -> Result<Vec<BackupMetadata>> { Ok(vec![]) }
    async fn count_backups(&self) -> Result<u32> { Ok(0) }
    async fn mark_backup_expired(&self, _backup_id: &str) -> Result<()> { Ok(()) }
    async fn get_current_wal_lsn(&self) -> Result<String> { Ok("0/0".to_string()) }
    async fn verify_postgres_config(&self) -> Result<()> { Ok(()) }
}