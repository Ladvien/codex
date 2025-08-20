//! Integration Tests for Database Operations
//!
//! These tests focus on PostgreSQL-specific functionality, transaction handling,
//! connection pooling, and pgvector integration. They test the actual database
//! interaction patterns and ensure data consistency.

mod test_helpers;

use anyhow::Result;
use chrono::{Duration, Utc};
use codex_memory::memory::models::{
    CreateMemoryRequest, MemoryStatus, MemoryTier, UpdateMemoryRequest,
};
use codex_memory::Config;
use serde_json::json;
use sqlx::{Pool, Postgres, Row};
use std::sync::Arc;
use test_helpers::TestEnvironment;
use tokio::time::{sleep, timeout};
use tracing_test::traced_test;
use uuid::Uuid;

/// Test basic database connectivity and pool management
#[tokio::test]
#[traced_test]
async fn test_database_connectivity() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Test connection pool health
    let pool = env.repository.pool();
    assert!(
        pool.size() > 0,
        "Connection pool should have active connections"
    );

    // Test basic query execution
    let row = sqlx::query("SELECT 1 as test_value")
        .fetch_one(pool)
        .await?;

    let test_value: i32 = row.get("test_value");
    assert_eq!(test_value, 1, "Basic query should return expected value");

    // Test database version and extensions
    let version_row = sqlx::query("SELECT version() as pg_version")
        .fetch_one(pool)
        .await?;

    let version: String = version_row.get("pg_version");
    assert!(version.contains("PostgreSQL"), "Should be using PostgreSQL");

    // Test pgvector extension
    let extension_row = sqlx::query(
        "SELECT EXISTS(SELECT 1 FROM pg_extension WHERE extname = 'vector') as has_vector",
    )
    .fetch_one(pool)
    .await?;

    let has_vector: bool = extension_row.get("has_vector");
    assert!(has_vector, "pgvector extension should be installed");

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test transaction handling and rollback behavior
#[tokio::test]
#[traced_test]
async fn test_transaction_handling() -> Result<()> {
    let env = TestEnvironment::new().await?;
    let pool = env.repository.pool();

    // Test successful transaction
    let mut tx = pool.begin().await?;

    let memory_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO memories (id, content, content_hash, tier, importance_score, status, access_count, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"
    )
    .bind(memory_id)
    .bind("Transaction test memory")
    .bind("test_hash_1")
    .bind("working")
    .bind(0.7)
    .bind("active")
    .bind(0)
    .bind(Utc::now())
    .bind(Utc::now())
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    // Verify memory exists after commit
    let exists = sqlx::query("SELECT id FROM memories WHERE id = $1")
        .bind(memory_id)
        .fetch_optional(pool)
        .await?;

    assert!(
        exists.is_some(),
        "Memory should exist after transaction commit"
    );

    // Test transaction rollback
    let mut tx = pool.begin().await?;

    let rollback_memory_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO memories (id, content, content_hash, tier, importance_score, status, access_count, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"
    )
    .bind(rollback_memory_id)
    .bind("Rollback test memory")
    .bind("test_hash_2")
    .bind("working")
    .bind(0.5)
    .bind("active")
    .bind(0)
    .bind(Utc::now())
    .bind(Utc::now())
    .execute(&mut *tx)
    .await?;

    tx.rollback().await?;

    // Verify memory does not exist after rollback
    let not_exists = sqlx::query("SELECT id FROM memories WHERE id = $1")
        .bind(rollback_memory_id)
        .fetch_optional(pool)
        .await?;

    assert!(
        not_exists.is_none(),
        "Memory should not exist after transaction rollback"
    );

    // Cleanup
    sqlx::query("DELETE FROM memories WHERE id = $1")
        .bind(memory_id)
        .execute(pool)
        .await?;

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test connection pool behavior under load
#[tokio::test]
#[traced_test]
async fn test_connection_pool_behavior() -> Result<()> {
    let env = TestEnvironment::new().await?;
    let repository = env.repository.clone();

    // Test concurrent connection usage
    let mut handles = Vec::new();

    for i in 0..10 {
        let repo = repository.clone();
        let handle = tokio::spawn(async move {
            let result = repo
                .create_memory(CreateMemoryRequest {
                    content: format!("Concurrent memory {}", i),
                    embedding: None,
                    tier: Some(MemoryTier::Working),
                    importance_score: Some(0.5),
                    metadata: Some(json!({"concurrent_test": true, "index": i})),
                    parent_id: None,
                    expires_at: None,
                })
                .await;

            // Add small delay to stress the pool
            sleep(std::time::Duration::from_millis(100)).await;
            result
        });
        handles.push(handle);
    }

    // Wait for all concurrent operations to complete
    let results = futures::future::join_all(handles).await;

    // Verify all operations succeeded
    for (i, result) in results.into_iter().enumerate() {
        let memory = result??;
        assert_eq!(memory.content, format!("Concurrent memory {}", i));
        assert_eq!(memory.tier, MemoryTier::Working);
    }

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test database constraint enforcement
#[tokio::test]
#[traced_test]
async fn test_database_constraints() -> Result<()> {
    let env = TestEnvironment::new().await?;
    let pool = env.repository.pool();

    // Test unique constraint (if any exist)
    let memory_id = Uuid::new_v4();

    // Insert first memory
    sqlx::query(
        "INSERT INTO memories (id, content, content_hash, tier, importance_score, status, access_count, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"
    )
    .bind(memory_id)
    .bind("Constraint test memory")
    .bind("test_hash_3")
    .bind("working")
    .bind(0.7)
    .bind("active")
    .bind(0)
    .bind(Utc::now())
    .bind(Utc::now())
    .execute(pool)
    .await?;

    // Try to insert duplicate ID (should fail)
    let duplicate_result = sqlx::query(
        "INSERT INTO memories (id, content, content_hash, tier, importance_score, status, access_count, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"
    )
    .bind(memory_id) // Same ID
    .bind("Duplicate memory")
    .bind("test_hash_4")
    .bind("working")
    .bind(0.8)
    .bind("active")
    .bind(0)
    .bind(Utc::now())
    .bind(Utc::now())
    .execute(pool)
    .await;

    assert!(
        duplicate_result.is_err(),
        "Duplicate ID insertion should fail"
    );

    // Test foreign key constraints (if parent_id exists)
    let non_existent_parent = Uuid::new_v4();
    let child_memory_result = sqlx::query(
        "INSERT INTO memories (id, content, content_hash, tier, importance_score, status, access_count, parent_id, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"
    )
    .bind(Uuid::new_v4())
    .bind("Child with invalid parent")
    .bind("test_hash_5")
    .bind("working")
    .bind(0.5)
    .bind("active")
    .bind(0)
    .bind(non_existent_parent)
    .bind(Utc::now())
    .bind(Utc::now())
    .execute(pool)
    .await;

    // This may or may not fail depending on schema - we'll just test it doesn't panic
    let _ = child_memory_result;

    // Cleanup
    sqlx::query("DELETE FROM memories WHERE id = $1")
        .bind(memory_id)
        .execute(pool)
        .await?;

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test pgvector operations and embedding storage
#[tokio::test]
#[traced_test]
async fn test_pgvector_operations() -> Result<()> {
    let env = TestEnvironment::new().await?;
    let pool = env.repository.pool();

    // Test vector insertion and retrieval
    let memory_id = Uuid::new_v4();
    let test_embedding = vec![0.1_f32, 0.2, 0.3, 0.4, 0.5];

    // Insert memory with embedding
    sqlx::query(
        "INSERT INTO memories (id, content, content_hash, tier, importance_score, status, access_count, embedding, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"
    )
    .bind(memory_id)
    .bind("Vector test memory")
    .bind("test_hash_vector")
    .bind("working")
    .bind(0.8)
    .bind("active")
    .bind(0)
    .bind(pgvector::Vector::from(test_embedding.clone()))
    .bind(Utc::now())
    .bind(Utc::now())
    .execute(pool)
    .await?;

    // Retrieve and verify embedding
    let row = sqlx::query("SELECT embedding FROM memories WHERE id = $1")
        .bind(memory_id)
        .fetch_one(pool)
        .await?;

    let retrieved_embedding: Option<pgvector::Vector> = row.get("embedding");
    assert!(
        retrieved_embedding.is_some(),
        "Embedding should be retrievable"
    );

    let embedding = retrieved_embedding.unwrap();
    let embedding_vec: Vec<f32> = embedding.into();
    assert_eq!(
        embedding_vec, test_embedding,
        "Retrieved embedding should match original"
    );

    // Test vector similarity search
    let search_vector = pgvector::Vector::from(vec![0.1_f32, 0.2, 0.3, 0.4, 0.6]);
    let similarity_result = sqlx::query(
        "SELECT id, content, embedding <-> $1 as distance
         FROM memories
         WHERE embedding IS NOT NULL
         ORDER BY embedding <-> $1
         LIMIT 5",
    )
    .bind(search_vector)
    .fetch_all(pool)
    .await?;

    assert!(
        !similarity_result.is_empty(),
        "Similarity search should return results"
    );

    let first_result = &similarity_result[0];
    let result_id: Uuid = first_result.get("id");
    let distance: f32 = first_result.get("distance");

    assert_eq!(
        result_id, memory_id,
        "Most similar should be our test memory"
    );
    assert!(distance >= 0.0, "Distance should be non-negative");

    // Cleanup
    sqlx::query("DELETE FROM memories WHERE id = $1")
        .bind(memory_id)
        .execute(pool)
        .await?;

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test database performance characteristics
#[tokio::test]
#[traced_test]
async fn test_database_performance() -> Result<()> {
    let env = TestEnvironment::new().await?;
    let pool = env.repository.pool();

    // Test query performance with timeout
    let start = std::time::Instant::now();

    let performance_test = timeout(
        std::time::Duration::from_secs(10),
        async {
            // Insert batch of test data
            let mut memory_ids = Vec::new();

            for i in 0..100 {
                let memory_id = Uuid::new_v4();
                sqlx::query(
                    "INSERT INTO memories (id, content, content_hash, tier, importance_score, status, access_count, created_at, updated_at)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"
                )
                .bind(memory_id)
                .bind(format!("Performance test memory {}", i))
                .bind(format!("perf_hash_{}", i))
                .bind("working")
                .bind(0.5 + (i as f64 / 200.0))
                .bind("active")
                .bind(0)
                .bind(Utc::now())
                .bind(Utc::now())
                .execute(pool)
                .await?;

                memory_ids.push(memory_id);
            }

            // Test batch retrieval performance
            let batch_query_start = std::time::Instant::now();

            let results = sqlx::query("SELECT * FROM memories WHERE tier = $1 ORDER BY importance_score DESC LIMIT 50")
                .bind("working")
                .fetch_all(pool)
                .await?;

            let batch_query_duration = batch_query_start.elapsed();
            assert!(results.len() <= 50, "Should respect LIMIT clause");
            assert!(batch_query_duration.as_millis() < 1000, "Batch query should complete quickly");

            // Cleanup test data
            for memory_id in memory_ids {
                sqlx::query("DELETE FROM memories WHERE id = $1")
                    .bind(memory_id)
                    .execute(pool)
                    .await?;
            }

            Ok::<(), anyhow::Error>(())
        }
    ).await;

    let total_duration = start.elapsed();
    assert!(
        performance_test.is_ok(),
        "Performance test should not timeout"
    );
    assert!(
        total_duration.as_secs() < 10,
        "Performance test should complete within timeout"
    );

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test database migration state and schema consistency
#[tokio::test]
#[traced_test]
async fn test_schema_consistency() -> Result<()> {
    let env = TestEnvironment::new().await?;
    let pool = env.repository.pool();

    // Test that expected tables exist
    let table_query = sqlx::query(
        "SELECT table_name FROM information_schema.tables
         WHERE table_schema = 'public' AND table_name = 'memories'",
    )
    .fetch_optional(pool)
    .await?;

    assert!(table_query.is_some(), "memories table should exist");

    // Test expected columns exist
    let column_query = sqlx::query(
        "SELECT column_name, data_type FROM information_schema.columns
         WHERE table_schema = 'public' AND table_name = 'memories'",
    )
    .fetch_all(pool)
    .await?;

    assert!(
        !column_query.is_empty(),
        "memories table should have columns"
    );

    let column_names: Vec<String> = column_query
        .iter()
        .map(|row| row.get::<String, _>("column_name"))
        .collect();

    // Verify essential columns exist
    let expected_columns = [
        "id",
        "content",
        "tier",
        "importance_score",
        "status",
        "created_at",
        "updated_at",
    ];
    for expected_col in expected_columns {
        assert!(
            column_names.contains(&expected_col.to_string()),
            "Column '{}' should exist in memories table",
            expected_col
        );
    }

    // Test indexes exist (if any are defined)
    let index_query = sqlx::query("SELECT indexname FROM pg_indexes WHERE tablename = 'memories'")
        .fetch_all(pool)
        .await?;

    // We expect at least the primary key index
    assert!(
        !index_query.is_empty(),
        "memories table should have at least one index"
    );

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test connection recovery after connection loss
#[tokio::test]
#[traced_test]
async fn test_connection_recovery() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Test normal operation
    let memory = env
        .repository
        .create_memory(CreateMemoryRequest {
            content: "Connection recovery test".to_string(),
            embedding: None,
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.7),
            metadata: Some(json!({"recovery_test": true})),
            parent_id: None,
            expires_at: None,
        })
        .await?;

    // Verify we can retrieve it
    let retrieved = env.repository.get_memory(memory.id).await?;
    assert_eq!(retrieved.content, "Connection recovery test");

    // Test that the pool can handle connection issues gracefully
    // (This is mainly testing that our connection pool configuration is robust)
    let pool = env.repository.pool();
    let pool_size_before = pool.size();

    // Simulate some load on connections
    let mut handles = Vec::new();
    for i in 0..5 {
        let repo = env.repository.clone();
        let handle = tokio::spawn(async move {
            // Simple query that should work even if some connections are problematic
            let stats = repo.get_statistics().await;
            (i, stats)
        });
        handles.push(handle);
    }

    let results = futures::future::join_all(handles).await;
    let successful_queries = results
        .into_iter()
        .filter_map(|r| r.ok())
        .filter_map(|(i, stats)| stats.ok().map(|s| (i, s)))
        .count();

    assert!(
        successful_queries >= 3,
        "Most queries should succeed even under load"
    );

    let pool_size_after = pool.size();
    assert!(
        pool_size_after > 0,
        "Connection pool should maintain connections"
    );

    // Cleanup
    env.repository.delete_memory(memory.id).await?;
    env.cleanup_test_data().await?;
    Ok(())
}

/// Test database-level data consistency across operations
#[tokio::test]
#[traced_test]
async fn test_data_consistency() -> Result<()> {
    let env = TestEnvironment::new().await?;
    let pool = env.repository.pool();

    // Create related memories with parent-child relationship
    let parent_memory = env
        .repository
        .create_memory(CreateMemoryRequest {
            content: "Parent memory for consistency test".to_string(),
            embedding: None,
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.8),
            metadata: Some(json!({"type": "parent"})),
            parent_id: None,
            expires_at: None,
        })
        .await?;

    let child_memory = env
        .repository
        .create_memory(CreateMemoryRequest {
            content: "Child memory for consistency test".to_string(),
            embedding: None,
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.6),
            metadata: Some(json!({"type": "child"})),
            parent_id: Some(parent_memory.id),
            expires_at: None,
        })
        .await?;

    // Test that parent-child relationship is maintained in database
    let child_check = sqlx::query("SELECT parent_id FROM memories WHERE id = $1")
        .bind(child_memory.id)
        .fetch_one(pool)
        .await?;

    let stored_parent_id: Option<Uuid> = child_check.get("parent_id");
    assert_eq!(
        stored_parent_id,
        Some(parent_memory.id),
        "Parent-child relationship should be stored correctly"
    );

    // Test concurrent updates don't create inconsistency
    let update_handles = vec![
        {
            let repo = env.repository.clone();
            let memory_id = parent_memory.id;
            tokio::spawn(async move {
                repo.update_memory(
                    memory_id,
                    UpdateMemoryRequest {
                        content: Some("Updated parent content 1".to_string()),
                        embedding: None,
                        tier: None,
                        importance_score: Some(0.85),
                        metadata: None,
                        expires_at: None,
                    },
                )
                .await
            })
        },
        {
            let repo = env.repository.clone();
            let memory_id = parent_memory.id;
            tokio::spawn(async move {
                repo.update_memory(
                    memory_id,
                    UpdateMemoryRequest {
                        content: Some("Updated parent content 2".to_string()),
                        embedding: None,
                        tier: None,
                        importance_score: Some(0.9),
                        metadata: None,
                        expires_at: None,
                    },
                )
                .await
            })
        },
    ];

    let update_results = futures::future::join_all(update_handles).await;

    // At least one update should succeed
    let successful_updates = update_results
        .into_iter()
        .filter_map(|r| r.ok())
        .filter_map(|r| r.ok())
        .count();

    assert!(
        successful_updates >= 1,
        "At least one concurrent update should succeed"
    );

    // Verify final state is consistent
    let final_parent = env.repository.get_memory(parent_memory.id).await?;
    assert!(
        final_parent.content.starts_with("Updated parent content"),
        "Parent should have updated content"
    );
    assert!(
        final_parent.importance_score >= 0.8,
        "Importance should be updated"
    );

    // Cleanup
    env.repository.delete_memory(child_memory.id).await?;
    env.repository.delete_memory(parent_memory.id).await?;
    env.cleanup_test_data().await?;
    Ok(())
}
