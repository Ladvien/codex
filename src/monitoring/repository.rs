use anyhow::Result;
use async_trait::async_trait;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::debug;

/// Repository abstraction for monitoring operations
#[async_trait]
pub trait MonitoringRepository: Send + Sync + std::fmt::Debug {
    async fn health_check(&self) -> Result<()>;
    async fn get_memory_tier_distribution(&self) -> Result<HashMap<String, i64>>;
    async fn check_migration_failures(&self, hours: i64) -> Result<i64>;
    async fn get_connection_pool_stats(&self) -> Result<ConnectionPoolStats>;
}

#[derive(Debug, Clone)]
pub struct ConnectionPoolStats {
    pub total_connections: u32,
    pub idle_connections: u32,
    pub active_connections: u32,
}

/// PostgreSQL implementation of monitoring repository
#[derive(Debug)]
pub struct PostgresMonitoringRepository {
    db_pool: Arc<PgPool>,
}

impl PostgresMonitoringRepository {
    pub fn new(db_pool: Arc<PgPool>) -> Self {
        Self { db_pool }
    }
}

#[async_trait]
impl MonitoringRepository for PostgresMonitoringRepository {
    async fn health_check(&self) -> Result<()> {
        debug!("Performing database health check");

        // Test basic connectivity
        sqlx::query("SELECT 1 as health_check")
            .fetch_one(self.db_pool.as_ref())
            .await?;

        // Test with a more complex query
        sqlx::query("SELECT COUNT(*) FROM memories WHERE status = 'active'")
            .fetch_one(self.db_pool.as_ref())
            .await?;

        debug!("Database health check passed");
        Ok(())
    }

    async fn get_memory_tier_distribution(&self) -> Result<HashMap<String, i64>> {
        debug!("Getting memory tier distribution");

        let rows = sqlx::query_as::<_, (String, i64)>(
            "SELECT tier, COUNT(*) FROM memories WHERE status = 'active' GROUP BY tier",
        )
        .fetch_all(self.db_pool.as_ref())
        .await?;

        let mut distribution = HashMap::new();
        for (tier, count) in rows {
            distribution.insert(tier, count);
        }

        Ok(distribution)
    }

    async fn check_migration_failures(&self, hours: i64) -> Result<i64> {
        debug!("Checking migration failures for last {} hours", hours);

        // First check if the success column exists
        let column_exists = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM information_schema.columns WHERE table_name = 'migration_history' AND column_name = 'success'"
        )
        .fetch_one(self.db_pool.as_ref())
        .await?;

        if column_exists > 0 {
            let failure_count = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM migration_history WHERE success = false AND migrated_at > NOW() - INTERVAL $1 || ' hours'"
            )
            .bind(hours)
            .fetch_one(self.db_pool.as_ref())
            .await?;

            Ok(failure_count)
        } else {
            // Column doesn't exist, return 0
            Ok(0)
        }
    }

    async fn get_connection_pool_stats(&self) -> Result<ConnectionPoolStats> {
        debug!("Getting connection pool statistics");

        let total_connections = self.db_pool.size();
        let idle_connections = self.db_pool.num_idle();
        let active_connections = total_connections - idle_connections as u32;

        Ok(ConnectionPoolStats {
            total_connections,
            idle_connections: idle_connections as u32,
            active_connections,
        })
    }
}

/// Mock repository for testing
#[derive(Debug)]
pub struct MockMonitoringRepository;

#[async_trait]
impl MonitoringRepository for MockMonitoringRepository {
    async fn health_check(&self) -> Result<()> {
        Ok(())
    }

    async fn get_memory_tier_distribution(&self) -> Result<HashMap<String, i64>> {
        let mut distribution = HashMap::new();
        distribution.insert("working".to_string(), 100);
        distribution.insert("warm".to_string(), 200);
        distribution.insert("cold".to_string(), 300);
        Ok(distribution)
    }

    async fn check_migration_failures(&self, _hours: i64) -> Result<i64> {
        Ok(0)
    }

    async fn get_connection_pool_stats(&self) -> Result<ConnectionPoolStats> {
        Ok(ConnectionPoolStats {
            total_connections: 20,
            idle_connections: 15,
            active_connections: 5,
        })
    }
}
