//! Test for Codex Dreams insights migration
//!
//! This test validates the database migration for Story 1:
//! - Verifies migration syntax is valid  
//! - Tests forward and rollback migrations
//! - Validates schema structure and constraints
//! - Tests feature flag behavior

use anyhow::{Context, Result};
use sqlx::{PgPool, Row};
use std::env;
use std::fs;
use uuid::Uuid;

const MIGRATION_FILE: &str = "migration/migrations/014_codex_dreams_insights_schema.sql";
const ROLLBACK_FILE: &str = "migration/migrations/014_codex_dreams_insights_schema_rollback.sql";

#[tokio::test]
async fn test_insights_migration_syntax() -> Result<()> {
    // Test that migration files exist and are readable
    let migration_content =
        fs::read_to_string(MIGRATION_FILE).context("Failed to read migration file")?;

    let rollback_content =
        fs::read_to_string(ROLLBACK_FILE).context("Failed to read rollback file")?;

    // Basic syntax validation
    assert!(
        !migration_content.is_empty(),
        "Migration file should not be empty"
    );
    assert!(
        !rollback_content.is_empty(),
        "Rollback file should not be empty"
    );

    // Check for required keywords in migration
    assert!(
        migration_content.contains("CREATE TABLE IF NOT EXISTS insights"),
        "Should create insights table"
    );
    assert!(
        migration_content.contains("CREATE TABLE IF NOT EXISTS insight_vectors"),
        "Should create insight_vectors table"
    );
    assert!(
        migration_content.contains("CREATE TABLE IF NOT EXISTS insight_feedback"),
        "Should create insight_feedback table"
    );
    assert!(
        migration_content.contains("CREATE TABLE IF NOT EXISTS processing_queue"),
        "Should create processing_queue table"
    );
    assert!(
        migration_content
            .contains("ALTER TABLE memories ADD COLUMN IF NOT EXISTS processing_metadata"),
        "Should add processing_metadata column"
    );

    // Check for required keywords in rollback
    assert!(
        rollback_content.contains("DROP TABLE IF EXISTS insights"),
        "Should drop insights table"
    );
    assert!(
        rollback_content.contains("DROP TABLE IF EXISTS insight_vectors"),
        "Should drop insight_vectors table"
    );
    assert!(
        rollback_content.contains("DROP TABLE IF EXISTS insight_feedback"),
        "Should drop insight_feedback table"
    );
    assert!(
        rollback_content.contains("DROP TABLE IF EXISTS processing_queue"),
        "Should drop processing_queue table"
    );
    assert!(
        rollback_content.contains("ALTER TABLE memories DROP COLUMN IF EXISTS processing_metadata"),
        "Should drop processing_metadata column"
    );

    // Check for feature flag handling
    assert!(
        migration_content.contains("codex-dreams"),
        "Should check codex-dreams feature flag"
    );
    assert!(
        migration_content.contains("CODEX_DREAMS_ENABLED"),
        "Should check environment variable"
    );

    // Check for performance optimizations
    assert!(
        migration_content.contains("HNSW"),
        "Should create HNSW vector index"
    );
    assert!(
        migration_content.contains("m = 48"),
        "Should use optimized HNSW parameters"
    );
    assert!(
        migration_content.contains("ef_construction = 200"),
        "Should use optimized ef_construction"
    );

    println!("✅ Migration syntax validation passed");
    Ok(())
}

#[tokio::test]
#[ignore] // Only run when database is available
async fn test_migration_with_database() -> Result<()> {
    // This test requires a test database - only run when DATABASE_URL is available
    let database_url = match env::var("DATABASE_URL") {
        Ok(url) => url,
        Err(_) => {
            println!("⚠️ Skipping database test - DATABASE_URL not set");
            return Ok(());
        }
    };

    // Connect to database
    let pool = sqlx::PgPool::connect(&database_url)
        .await
        .context("Failed to connect to test database")?;

    // Check if we're running against a test database (safety check)
    let db_name: String = sqlx::query("SELECT current_database()")
        .fetch_one(&pool)
        .await?
        .get(0);

    if !db_name.contains("test") {
        println!("⚠️ Skipping database test - not running against test database");
        return Ok(());
    }

    // Enable feature flag for testing
    sqlx::query("SET codex.dreams_enabled = true")
        .execute(&pool)
        .await
        .context("Failed to enable feature flag")?;

    // Load and execute migration
    let migration_content = fs::read_to_string(MIGRATION_FILE)?;
    sqlx::query(&migration_content)
        .execute(&pool)
        .await
        .context("Forward migration failed")?;

    // Verify tables were created
    let table_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM information_schema.tables 
         WHERE table_name IN ('insights', 'insight_vectors', 'insight_feedback', 'processing_queue')"
    )
    .fetch_one(&pool)
    .await?;

    assert_eq!(table_count, 4, "All insights tables should be created");

    // Verify processing_metadata column was added
    let column_exists: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM information_schema.columns 
         WHERE table_name = 'memories' AND column_name = 'processing_metadata'",
    )
    .fetch_one(&pool)
    .await?;

    assert_eq!(
        column_exists, 1,
        "processing_metadata column should be added to memories"
    );

    // Test basic insert into insights table
    let insight_id = Uuid::new_v4();
    let memory_id = Uuid::new_v4();

    // First create a test memory (required for foreign key)
    sqlx::query("INSERT INTO memories (id, content, content_hash, tier) VALUES ($1, $2, $3, $4)")
        .bind(memory_id)
        .bind("Test memory content")
        .bind("test_hash")
        .bind("working")
        .execute(&pool)
        .await
        .context("Failed to create test memory")?;

    // Insert test insight
    sqlx::query(
        "INSERT INTO insights (id, content, insight_type, source_memory_ids) VALUES ($1, $2, $3, $4)"
    )
    .bind(insight_id)
    .bind("Test insight content")
    .bind("learning")
    .bind(vec![memory_id])
    .execute(&pool)
    .await
    .context("Failed to insert test insight")?;

    // Verify insight was created
    let insight_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM insights WHERE id = $1")
        .bind(insight_id)
        .fetch_one(&pool)
        .await?;

    assert_eq!(insight_count, 1, "Test insight should be created");

    // Test rollback migration
    let rollback_content = fs::read_to_string(ROLLBACK_FILE)?;
    sqlx::query(&rollback_content)
        .execute(&pool)
        .await
        .context("Rollback migration failed")?;

    // Verify tables were removed
    let remaining_tables: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM information_schema.tables 
         WHERE table_name IN ('insights', 'insight_vectors', 'insight_feedback', 'processing_queue')"
    )
    .fetch_one(&pool)
    .await?;

    assert_eq!(
        remaining_tables, 0,
        "All insights tables should be removed after rollback"
    );

    // Verify processing_metadata column was removed
    let column_remains: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM information_schema.columns 
         WHERE table_name = 'memories' AND column_name = 'processing_metadata'",
    )
    .fetch_one(&pool)
    .await?;

    assert_eq!(
        column_remains, 0,
        "processing_metadata column should be removed after rollback"
    );

    println!("✅ Database migration test passed");
    Ok(())
}

#[tokio::test]
#[ignore] // Only run when database is available
async fn test_migration_performance() -> Result<()> {
    let database_url = match env::var("DATABASE_URL") {
        Ok(url) => url,
        Err(_) => {
            println!("⚠️ Skipping performance test - DATABASE_URL not set");
            return Ok(());
        }
    };

    let pool = sqlx::PgPool::connect(&database_url).await?;

    // Check database name for safety
    let db_name: String = sqlx::query("SELECT current_database()")
        .fetch_one(&pool)
        .await?
        .get(0);

    if !db_name.contains("test") {
        println!("⚠️ Skipping performance test - not running against test database");
        return Ok(());
    }

    // Enable feature flag
    sqlx::query("SET codex.dreams_enabled = true")
        .execute(&pool)
        .await?;

    // Time the migration
    let start = std::time::Instant::now();

    let migration_content = fs::read_to_string(MIGRATION_FILE)?;
    sqlx::query(&migration_content).execute(&pool).await?;

    let migration_duration = start.elapsed();

    // Migration should complete in reasonable time (under 30 seconds)
    assert!(
        migration_duration.as_secs() < 30,
        "Migration took too long: {}s",
        migration_duration.as_secs()
    );

    // Test index creation time by doing sample operations
    let start = std::time::Instant::now();

    // Sample vector for testing HNSW index
    let sample_vector = vec![0.1f32; 1536];

    sqlx::query("INSERT INTO insight_vectors (insight_id, embedding) VALUES ($1, $2)")
        .bind(Uuid::new_v4())
        .bind(sample_vector.clone())
        .execute(&pool)
        .await
        .context("Failed to test vector insert")?;

    let vector_insert_duration = start.elapsed();

    // Vector operations should be reasonably fast
    assert!(
        vector_insert_duration.as_millis() < 1000,
        "Vector insert took too long: {}ms",
        vector_insert_duration.as_millis()
    );

    println!(
        "✅ Performance test passed - Migration: {}s, Vector insert: {}ms",
        migration_duration.as_secs(),
        vector_insert_duration.as_millis()
    );

    // Clean up
    let rollback_content = fs::read_to_string(ROLLBACK_FILE)?;
    sqlx::query(&rollback_content).execute(&pool).await?;

    Ok(())
}

#[test]
fn test_schema_documentation_exists() {
    // Verify documentation was created
    assert!(
        std::path::Path::new("docs/database/insights-schema.md").exists(),
        "Schema documentation should exist"
    );

    let doc_content = fs::read_to_string("docs/database/insights-schema.md")
        .expect("Should be able to read documentation");

    // Check for required sections
    assert!(
        doc_content.contains("## Schema Tables"),
        "Should document schema tables"
    );
    assert!(
        doc_content.contains("## Index Strategy"),
        "Should document indexing strategy"
    );
    assert!(
        doc_content.contains("## Performance Targets"),
        "Should document performance targets"
    );
    assert!(
        doc_content.contains("## Feature Flag Implementation"),
        "Should document feature flags"
    );

    println!("✅ Schema documentation validation passed");
}
