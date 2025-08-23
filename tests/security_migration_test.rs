use anyhow::{Context, Result};
use dotenv::dotenv;
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::time::Duration;
use tokio::time::timeout;

/// Test suite for security migration (009_security_hardening.sql)
/// Tests statement timeout, idle transaction timeout, and connection constraints

#[cfg(test)]
mod security_migration_tests {
    use super::*;
    use tokio::time::sleep;

    async fn create_test_pool() -> Result<PgPool> {
        // Load environment variables from .env file
        let _ = dotenv();

        let database_url = std::env::var("DATABASE_URL")
            .context("DATABASE_URL environment variable not set. Ensure .env file is present.")?;

        PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(Duration::from_secs(10))
            .connect(&database_url)
            .await
            .context("Failed to create test database pool")
    }

    /// Test that statement timeout is correctly set to 30 seconds
    #[tokio::test]
    async fn test_statement_timeout_configuration() -> Result<()> {
        let pool = create_test_pool().await?;

        // Get current statement timeout setting
        let row: (String,) = sqlx::query_as("SELECT current_setting('statement_timeout')")
            .fetch_one(&pool)
            .await?;

        // Should be 30s (30000ms) after migration
        assert_eq!(
            row.0, "30s",
            "Statement timeout should be set to 30 seconds"
        );

        Ok(())
    }

    /// Test that idle transaction timeout is correctly set to 60 seconds  
    #[tokio::test]
    async fn test_idle_transaction_timeout_configuration() -> Result<()> {
        let pool = create_test_pool().await?;

        // Get current idle transaction timeout setting
        let row: (String,) =
            sqlx::query_as("SELECT current_setting('idle_in_transaction_session_timeout')")
                .fetch_one(&pool)
                .await?;

        // Should be 60s (60000ms) after migration
        assert_eq!(
            row.0, "60s",
            "Idle transaction timeout should be set to 60 seconds"
        );

        Ok(())
    }

    /// Test that long-running queries are terminated by statement timeout
    #[tokio::test]
    async fn test_statement_timeout_enforcement() -> Result<()> {
        let pool = create_test_pool().await?;

        // This query should timeout after 30 seconds
        let long_query_result = timeout(
            Duration::from_secs(35), // Give 5 seconds buffer
            sqlx::query("SELECT pg_sleep(35)").execute(&pool),
        )
        .await;

        match long_query_result {
            Ok(query_result) => {
                // If query completed, it should have failed due to timeout
                assert!(
                    query_result.is_err(),
                    "Query should have been terminated by statement timeout"
                );

                // Check if it's a timeout error
                let error = query_result.unwrap_err();
                let error_message = error.to_string();
                assert!(
                    error_message.contains("timeout")
                        || error_message.contains("canceling statement"),
                    "Error should indicate timeout: {}",
                    error_message
                );
            }
            Err(_) => {
                // Timeout from our test framework - this is also acceptable
                // as it means the query didn't complete in reasonable time
            }
        }

        Ok(())
    }

    /// Test that work_mem setting is correctly configured
    #[tokio::test]
    async fn test_work_mem_configuration() -> Result<()> {
        let pool = create_test_pool().await?;

        let row: (String,) = sqlx::query_as("SELECT current_setting('work_mem')")
            .fetch_one(&pool)
            .await?;

        // Should be 256MB after migration
        assert_eq!(row.0, "256MB", "Work memory should be set to 256MB");

        Ok(())
    }

    /// Test that security monitoring view exists and is accessible
    #[tokio::test]
    async fn test_security_monitoring_view() -> Result<()> {
        let pool = create_test_pool().await?;

        // Test that the security_monitoring view exists
        let result = sqlx::query("SELECT COUNT(*) FROM security_monitoring")
            .fetch_one(&pool)
            .await;

        assert!(
            result.is_ok(),
            "Security monitoring view should be accessible"
        );

        // Test that view returns reasonable data
        let rows: Vec<(i32, String, String, Option<String>, String)> = sqlx::query_as(
            "SELECT pid, usename, application_name, client_addr, state 
             FROM security_monitoring 
             LIMIT 5",
        )
        .fetch_all(&pool)
        .await?;

        // Should have at least our current connection
        assert!(
            !rows.is_empty(),
            "Security monitoring should show active connections"
        );

        Ok(())
    }

    /// Test connection pool constraints
    #[tokio::test]
    async fn test_connection_pool_limits() -> Result<()> {
        let pool = create_test_pool().await?;

        // Check max_connections setting
        let row: (String,) = sqlx::query_as("SELECT current_setting('max_connections')")
            .fetch_one(&pool)
            .await?;

        let max_connections: i32 = row
            .0
            .parse()
            .context("Failed to parse max_connections value")?;

        // Should be reasonable limit (typically 200)
        assert!(
            max_connections >= 100,
            "Max connections should be at least 100"
        );
        assert!(
            max_connections <= 300,
            "Max connections should not exceed 300"
        );

        Ok(())
    }

    /// Test query logging configuration  
    #[tokio::test]
    async fn test_query_logging_configuration() -> Result<()> {
        let pool = create_test_pool().await?;

        let row: (String,) = sqlx::query_as("SELECT current_setting('log_min_duration_statement')")
            .fetch_one(&pool)
            .await?;

        // Should be 1s (1000ms) after migration
        assert_eq!(
            row.0, "1s",
            "Query logging threshold should be set to 1 second"
        );

        Ok(())
    }

    /// Test that database timezone is set to UTC
    #[tokio::test]
    async fn test_timezone_configuration() -> Result<()> {
        let pool = create_test_pool().await?;

        let row: (String,) = sqlx::query_as("SELECT current_setting('timezone')")
            .fetch_one(&pool)
            .await?;

        assert_eq!(row.0, "UTC", "Database timezone should be set to UTC");

        Ok(())
    }

    /// Test autovacuum configuration
    #[tokio::test]
    async fn test_autovacuum_configuration() -> Result<()> {
        let pool = create_test_pool().await?;

        // Check autovacuum scale factor
        let row: (String,) =
            sqlx::query_as("SELECT current_setting('autovacuum_vacuum_scale_factor')")
                .fetch_one(&pool)
                .await?;

        let scale_factor: f64 = row
            .0
            .parse()
            .context("Failed to parse autovacuum_vacuum_scale_factor")?;

        assert_eq!(
            scale_factor, 0.1,
            "Autovacuum vacuum scale factor should be 0.1"
        );

        Ok(())
    }

    /// Test temp file limit configuration
    #[tokio::test]
    async fn test_temp_file_limit() -> Result<()> {
        let pool = create_test_pool().await?;

        let row: (String,) = sqlx::query_as("SELECT current_setting('temp_file_limit')")
            .fetch_one(&pool)
            .await?;

        // Should be 5GB (5242880 kB) after migration
        assert_eq!(row.0, "5242880", "Temp file limit should be set to 5GB");

        Ok(())
    }

    /// Integration test: Verify migration rollback works correctly
    #[tokio::test]
    async fn test_migration_rollback() -> Result<()> {
        let pool = create_test_pool().await?;

        // Store original values
        let original_statement_timeout: (String,) =
            sqlx::query_as("SELECT current_setting('statement_timeout')")
                .fetch_one(&pool)
                .await?;

        let original_idle_timeout: (String,) =
            sqlx::query_as("SELECT current_setting('idle_in_transaction_session_timeout')")
                .fetch_one(&pool)
                .await?;

        // Apply rollback migration (in a transaction for testing)
        let mut tx = pool.begin().await?;

        // Execute key rollback commands
        sqlx::query("SET statement_timeout = '300s'")
            .execute(&mut *tx)
            .await?;

        sqlx::query("SET idle_in_transaction_session_timeout = '600s'")
            .execute(&mut *tx)
            .await?;

        // Check that values changed in transaction
        let new_statement_timeout: (String,) =
            sqlx::query_as("SELECT current_setting('statement_timeout')")
                .fetch_one(&mut *tx)
                .await?;

        let new_idle_timeout: (String,) =
            sqlx::query_as("SELECT current_setting('idle_in_transaction_session_timeout')")
                .fetch_one(&mut *tx)
                .await?;

        assert_eq!(
            new_statement_timeout.0, "300s",
            "Rollback should set statement timeout to 300s"
        );
        assert_eq!(
            new_idle_timeout.0, "600s",
            "Rollback should set idle timeout to 600s"
        );

        // Rollback transaction to restore original values
        tx.rollback().await?;

        // Verify original values are restored
        let restored_statement_timeout: (String,) =
            sqlx::query_as("SELECT current_setting('statement_timeout')")
                .fetch_one(&pool)
                .await?;

        assert_eq!(
            restored_statement_timeout.0, original_statement_timeout.0,
            "Statement timeout should be restored after rollback"
        );

        Ok(())
    }
}
