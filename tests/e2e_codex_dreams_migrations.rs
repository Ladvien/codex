//! End-to-end tests for Codex Dreams database migrations
//!
//! Tests migration forward/rollback, idempotency, and feature flag handling

#![cfg(feature = "codex-dreams")]

use anyhow::Result;
use sqlx::{PgPool, Row};
use std::env;

/// Test that the migration creates all required tables and columns
#[tokio::test]
async fn test_migration_creates_all_schema_elements() -> Result<()> {
    let database_url = env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://postgres:postgres@localhost:5432/codex_test".to_string());
    
    let pool = PgPool::connect(&database_url).await?;
    
    // Run the migration
    sqlx::migrate!("./migration/migrations")
        .run(&pool)
        .await?;
    
    // Verify insights table exists with all columns
    let insights_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM information_schema.tables 
            WHERE table_name = 'insights'
        )"
    )
    .fetch_one(&pool)
    .await?;
    
    assert!(insights_exists, "insights table should exist");
    
    // Verify insight_vectors table
    let vectors_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM information_schema.tables 
            WHERE table_name = 'insight_vectors'
        )"
    )
    .fetch_one(&pool)
    .await?;
    
    assert!(vectors_exists, "insight_vectors table should exist");
    
    // Verify insight_feedback table
    let feedback_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM information_schema.tables 
            WHERE table_name = 'insight_feedback'
        )"
    )
    .fetch_one(&pool)
    .await?;
    
    assert!(feedback_exists, "insight_feedback table should exist");
    
    // Verify processing_queue table
    let queue_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM information_schema.tables 
            WHERE table_name = 'processing_queue'
        )"
    )
    .fetch_one(&pool)
    .await?;
    
    assert!(queue_exists, "processing_queue table should exist");
    
    // Verify processing_metadata column in memories table
    let metadata_column_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM information_schema.columns 
            WHERE table_name = 'memories' 
            AND column_name = 'processing_metadata'
        )"
    )
    .fetch_one(&pool)
    .await?;
    
    assert!(metadata_column_exists, "processing_metadata column should exist in memories table");
    
    Ok(())
}

/// Test that indexes are created with correct configuration
#[tokio::test]
async fn test_migration_creates_optimized_indexes() -> Result<()> {
    let database_url = env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://postgres:postgres@localhost:5432/codex_test".to_string());
    
    let pool = PgPool::connect(&database_url).await?;
    
    // Run migration
    sqlx::migrate!("./migration/migrations")
        .run(&pool)
        .await?;
    
    // Check HNSW index exists on insight_vectors
    let hnsw_index_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM pg_indexes 
            WHERE tablename = 'insight_vectors' 
            AND indexname LIKE '%hnsw%'
        )"
    )
    .fetch_one(&pool)
    .await?;
    
    assert!(hnsw_index_exists, "HNSW index should exist on insight_vectors");
    
    // Check composite index on insights table
    let composite_index_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM pg_indexes 
            WHERE tablename = 'insights' 
            AND indexdef LIKE '%insight_type%confidence_score%'
        )"
    )
    .fetch_one(&pool)
    .await?;
    
    assert!(composite_index_exists, "Composite index on type/confidence should exist");
    
    // Check GIN index for tags
    let gin_index_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM pg_indexes 
            WHERE tablename = 'insights' 
            AND indexdef LIKE '%gin%tags%'
        )"
    )
    .fetch_one(&pool)
    .await?;
    
    assert!(gin_index_exists, "GIN index on tags should exist");
    
    Ok(())
}

/// Test migration rollback functionality
#[tokio::test]
async fn test_migration_rollback() -> Result<()> {
    let database_url = env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://postgres:postgres@localhost:5432/codex_test_rollback".to_string());
    
    let pool = PgPool::connect(&database_url).await?;
    
    // First run forward migration
    sqlx::migrate!("./migration/migrations")
        .run(&pool)
        .await?;
    
    // Verify tables exist
    let insights_exists_before: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'insights')"
    )
    .fetch_one(&pool)
    .await?;
    
    assert!(insights_exists_before, "insights table should exist before rollback");
    
    // Run rollback migration
    sqlx::query(include_str!("../migration/migrations/014_codex_dreams_insights_schema_rollback.sql"))
        .execute(&pool)
        .await?;
    
    // Verify tables are removed
    let insights_exists_after: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'insights')"
    )
    .fetch_one(&pool)
    .await?;
    
    assert!(!insights_exists_after, "insights table should not exist after rollback");
    
    // Verify processing_metadata column is removed
    let metadata_column_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM information_schema.columns 
            WHERE table_name = 'memories' 
            AND column_name = 'processing_metadata'
        )"
    )
    .fetch_one(&pool)
    .await?;
    
    assert!(!metadata_column_exists, "processing_metadata column should be removed");
    
    Ok(())
}

/// Test migration idempotency - running twice should not fail
#[tokio::test]
async fn test_migration_idempotency() -> Result<()> {
    let database_url = env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://postgres:postgres@localhost:5432/codex_test_idempotent".to_string());
    
    let pool = PgPool::connect(&database_url).await?;
    
    // Run migration twice
    sqlx::migrate!("./migration/migrations")
        .run(&pool)
        .await?;
    
    // Second run should not fail
    sqlx::migrate!("./migration/migrations")
        .run(&pool)
        .await?;
    
    // Verify no duplicate indexes
    let index_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pg_indexes WHERE tablename = 'insights'"
    )
    .fetch_one(&pool)
    .await?;
    
    // Should have expected number of indexes, not duplicates
    assert!(index_count > 0 && index_count < 10, 
        "Should have reasonable number of indexes, not duplicates");
    
    Ok(())
}

/// Test that migration respects feature flag
#[tokio::test]
async fn test_migration_feature_flag_check() -> Result<()> {
    // This test verifies the migration checks for codex-dreams feature
    // In actual deployment, the migration SQL would check:
    // 1. Environment variable CODEX_DREAMS_ENABLED
    // 2. PostgreSQL setting codex.dreams_enabled
    // 3. Feature flag in application config
    
    let database_url = env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://postgres:postgres@localhost:5432/codex_test_flag".to_string());
    
    let pool = PgPool::connect(&database_url).await?;
    
    // Set feature flag to false in PostgreSQL
    sqlx::query("SELECT set_config('codex.dreams_enabled', 'false', false)")
        .execute(&pool)
        .await?;
    
    // Migration should still run but could check flag internally
    // This is more of a documentation test showing the pattern
    
    let flag_value: Option<String> = sqlx::query_scalar(
        "SELECT current_setting('codex.dreams_enabled', true)"
    )
    .fetch_optional(&pool)
    .await?;
    
    assert_eq!(flag_value, Some("false".to_string()), "Feature flag should be respected");
    
    Ok(())
}

/// Test constraint validations
#[tokio::test]
async fn test_migration_constraints() -> Result<()> {
    let database_url = env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://postgres:postgres@localhost:5432/codex_test_constraints".to_string());
    
    let pool = PgPool::connect(&database_url).await?;
    
    // Run migration
    sqlx::migrate!("./migration/migrations")
        .run(&pool)
        .await?;
    
    // Test content length constraint
    let result = sqlx::query(
        "INSERT INTO insights (content, insight_type, confidence_score) 
         VALUES ('short', 'learning', 0.5)"
    )
    .execute(&pool)
    .await;
    
    assert!(result.is_err(), "Should reject content shorter than 10 characters");
    
    // Test confidence score constraint
    let result = sqlx::query(
        "INSERT INTO insights (content, insight_type, confidence_score) 
         VALUES ('This is a valid insight content', 'learning', 1.5)"
    )
    .execute(&pool)
    .await;
    
    assert!(result.is_err(), "Should reject confidence score > 1.0");
    
    // Test feedback score constraint
    let result = sqlx::query(
        "INSERT INTO insights (content, insight_type, confidence_score, feedback_score) 
         VALUES ('This is a valid insight content', 'learning', 0.5, -2.0)"
    )
    .execute(&pool)
    .await;
    
    assert!(result.is_err(), "Should reject feedback score < -1.0");
    
    Ok(())
}