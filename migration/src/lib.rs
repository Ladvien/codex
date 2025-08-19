use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, Transaction};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{error, info, warn};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Migration {
    pub id: String,
    pub name: String,
    pub up_sql: String,
    pub down_sql: Option<String>,
    pub checksum: String,
}

#[derive(Debug, Clone)]
pub struct MigrationHistory {
    pub id: Uuid,
    pub migration_id: String,
    pub applied_at: DateTime<Utc>,
    pub checksum: String,
    pub execution_time_ms: i64,
}

pub struct MigrationRunner {
    pool: PgPool,
    migrations_dir: PathBuf,
}

impl MigrationRunner {
    pub fn new(pool: PgPool, migrations_dir: impl AsRef<Path>) -> Self {
        Self {
            pool,
            migrations_dir: migrations_dir.as_ref().to_path_buf(),
        }
    }

    pub async fn initialize(&self) -> Result<()> {
        // Create the migration history table first
        let create_table_query = r#"
            CREATE TABLE IF NOT EXISTS migration_history (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                migration_id VARCHAR(255) NOT NULL UNIQUE,
                checksum VARCHAR(64) NOT NULL,
                applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                execution_time_ms BIGINT NOT NULL,
                rolled_back BOOLEAN NOT NULL DEFAULT FALSE,
                rolled_back_at TIMESTAMPTZ
            )
        "#;

        sqlx::query(create_table_query)
            .execute(&self.pool)
            .await
            .context("Failed to create migration history table")?;

        // Create the index separately
        let create_index_query = r#"
            CREATE INDEX IF NOT EXISTS idx_migration_history_applied 
                ON migration_history (applied_at DESC)
        "#;

        sqlx::query(create_index_query)
            .execute(&self.pool)
            .await
            .context("Failed to create migration history index")?;

        info!("Migration history table initialized");
        Ok(())
    }

    pub async fn load_migrations(&self) -> Result<Vec<Migration>> {
        let mut migrations = Vec::new();

        let entries =
            fs::read_dir(&self.migrations_dir).context("Failed to read migrations directory")?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) != Some("sql") {
                continue;
            }

            let filename = path
                .file_name()
                .and_then(|s| s.to_str())
                .ok_or_else(|| anyhow::anyhow!("Invalid filename"))?;

            // Skip rollback files
            if filename.contains("rollback") {
                continue;
            }

            let id = filename
                .split('_')
                .next()
                .ok_or_else(|| anyhow::anyhow!("Invalid migration filename format"))?
                .to_string();

            let name = filename.trim_end_matches(".sql").to_string();

            let up_sql = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read migration file: {path:?}"))?;

            // Look for corresponding rollback file
            let rollback_path = path.with_file_name(format!("{name}_rollback.sql"));
            let down_sql =
                if rollback_path.exists() {
                    Some(fs::read_to_string(&rollback_path).with_context(|| {
                        format!("Failed to read rollback file: {rollback_path:?}")
                    })?)
                } else {
                    None
                };

            let checksum = self.calculate_checksum(&up_sql);

            migrations.push(Migration {
                id,
                name,
                up_sql,
                down_sql,
                checksum,
            });
        }

        migrations.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(migrations)
    }

    pub async fn migrate(&self) -> Result<()> {
        self.initialize().await?;

        let migrations = self.load_migrations().await?;
        let applied = self.get_applied_migrations().await?;

        for migration in migrations {
            if applied.contains(&migration.id) {
                info!("Skipping already applied migration: {}", migration.name);
                continue;
            }

            info!("Applying migration: {}", migration.name);
            let start = std::time::Instant::now();

            let mut tx = self.pool.begin().await?;

            match self.apply_migration(&mut tx, &migration).await {
                Ok(_) => {
                    let execution_time_ms = start.elapsed().as_millis() as i64;

                    self.record_migration(&mut tx, &migration, execution_time_ms)
                        .await?;
                    tx.commit().await?;

                    info!(
                        "Successfully applied migration {} in {}ms",
                        migration.name, execution_time_ms
                    );
                }
                Err(e) => {
                    error!("Failed to apply migration {}: {}", migration.name, e);
                    tx.rollback().await?;
                    return Err(e);
                }
            }
        }

        info!("All migrations applied successfully");
        Ok(())
    }

    pub async fn rollback(&self, target: Option<String>) -> Result<()> {
        let migrations = self.load_migrations().await?;
        let applied = self.get_applied_migrations_ordered().await?;

        if applied.is_empty() {
            info!("No migrations to rollback");
            return Ok(());
        }

        let migrations_to_rollback = if let Some(target_id) = target {
            let target_index = applied
                .iter()
                .position(|m| m == &target_id)
                .ok_or_else(|| anyhow::anyhow!("Target migration {} not found", target_id))?;

            applied[0..=target_index].to_vec()
        } else {
            vec![applied[0].clone()]
        };

        for migration_id in migrations_to_rollback.iter().rev() {
            let migration = migrations
                .iter()
                .find(|m| &m.id == migration_id)
                .ok_or_else(|| anyhow::anyhow!("Migration {} not found", migration_id))?;

            if migration.down_sql.is_none() {
                warn!(
                    "No rollback script for migration {}. Skipping.",
                    migration.name
                );
                continue;
            }

            info!("Rolling back migration: {}", migration.name);
            let mut tx = self.pool.begin().await?;

            match self.rollback_migration(&mut tx, migration).await {
                Ok(_) => {
                    self.mark_rolled_back(&mut tx, &migration.id).await?;
                    tx.commit().await?;
                    info!("Successfully rolled back migration {}", migration.name);
                }
                Err(e) => {
                    error!("Failed to rollback migration {}: {}", migration.name, e);
                    tx.rollback().await?;
                    return Err(e);
                }
            }
        }

        Ok(())
    }

    async fn apply_migration(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        migration: &Migration,
    ) -> Result<()> {
        // Split the SQL by semicolons and execute each statement separately
        let statements: Vec<&str> = migration
            .up_sql
            .split(';')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty() && !s.starts_with("--"))
            .collect();

        for statement in statements {
            if !statement.trim().is_empty() {
                sqlx::query(statement)
                    .execute(&mut **tx)
                    .await
                    .with_context(|| {
                        format!(
                            "Failed to execute migration statement in {}: {}",
                            migration.name, statement
                        )
                    })?;
            }
        }
        Ok(())
    }

    async fn rollback_migration(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        migration: &Migration,
    ) -> Result<()> {
        if let Some(down_sql) = &migration.down_sql {
            sqlx::query(down_sql)
                .execute(&mut **tx)
                .await
                .with_context(|| format!("Failed to rollback migration: {}", migration.name))?;
        }
        Ok(())
    }

    async fn record_migration(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        migration: &Migration,
        execution_time_ms: i64,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO migration_history (migration_id, checksum, execution_time_ms)
            VALUES ($1, $2, $3)
            "#,
        )
        .bind(&migration.id)
        .bind(&migration.checksum)
        .bind(execution_time_ms)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    async fn mark_rolled_back(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        migration_id: &str,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE migration_history 
            SET rolled_back = TRUE, rolled_back_at = NOW()
            WHERE migration_id = $1
            "#,
        )
        .bind(migration_id)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    async fn get_applied_migrations(&self) -> Result<Vec<String>> {
        let rows = sqlx::query_as::<_, (String,)>(
            "SELECT migration_id FROM migration_history WHERE rolled_back = FALSE ORDER BY applied_at",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    async fn get_applied_migrations_ordered(&self) -> Result<Vec<String>> {
        let rows = sqlx::query_as::<_, (String,)>(
            "SELECT migration_id FROM migration_history WHERE rolled_back = FALSE ORDER BY applied_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    fn calculate_checksum(&self, content: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }

    pub async fn verify_checksums(&self) -> Result<()> {
        let migrations = self.load_migrations().await?;
        let history = sqlx::query_as::<_, (String, String)>(
            "SELECT migration_id, checksum FROM migration_history WHERE rolled_back = FALSE",
        )
        .fetch_all(&self.pool)
        .await?;

        for (migration_id, stored_checksum) in history {
            if let Some(migration) = migrations.iter().find(|m| m.id == migration_id) {
                if migration.checksum != stored_checksum {
                    return Err(anyhow::anyhow!(
                        "Checksum mismatch for migration {}. File may have been modified after application.",
                        migration_id
                    ));
                }
            }
        }

        info!("All migration checksums verified");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;
    use tempfile::TempDir;

    async fn setup_test_pool() -> PgPool {
        let database_url = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
            "postgres://postgres:postgres@localhost/test_migrations".to_string()
        });

        PgPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .expect("Failed to create test pool")
    }

    #[tokio::test]
    #[ignore = "requires database"]
    async fn test_migration_runner_initialization() {
        let pool = setup_test_pool().await;
        let temp_dir = TempDir::new().unwrap();
        let runner = MigrationRunner::new(pool, temp_dir.path());

        assert!(runner.initialize().await.is_ok());
    }

    #[tokio::test]
    #[ignore = "requires database"]
    async fn test_load_migrations() {
        let pool = setup_test_pool().await;
        let temp_dir = TempDir::new().unwrap();

        // Create test migration files
        let migration_content = "CREATE TABLE test_table (id SERIAL PRIMARY KEY);";
        fs::write(
            temp_dir.path().join("001_test_migration.sql"),
            migration_content,
        )
        .unwrap();

        let runner = MigrationRunner::new(pool, temp_dir.path());
        let migrations = runner.load_migrations().await.unwrap();

        assert_eq!(migrations.len(), 1);
        assert_eq!(migrations[0].id, "001");
        assert_eq!(migrations[0].up_sql, migration_content);
    }
}
