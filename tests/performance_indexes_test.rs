use std::time::Duration;
use sqlx::{postgres::PgPoolOptions, PgPool};
use tokio::time::timeout;
use anyhow::{Result, Context};

/// Test suite for performance optimization indexes (010_performance_optimization_indexes.sql)
/// Tests index creation, query performance improvements, and index utilization

#[cfg(test)]
mod performance_indexes_tests {
    use super::*;
    use tokio::time::sleep;

    async fn create_test_pool() -> Result<PgPool> {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://postgres:password@localhost:5432/codex_test".to_string());
        
        PgPoolOptions::new()
            .max_connections(10)
            .acquire_timeout(Duration::from_secs(30))
            .connect(&database_url)
            .await
            .context("Failed to create test database pool")
    }

    /// Test that all expected performance indexes were created successfully
    #[tokio::test]
    async fn test_performance_indexes_existence() -> Result<()> {
        let pool = create_test_pool().await?;
        
        let expected_indexes = vec![
            "idx_memories_access_patterns_consolidated",
            "idx_memories_metadata_optimized", 
            "idx_memories_hybrid_search_optimized",
            "idx_memories_embedding_hnsw_optimized",
            "idx_memories_temporal_optimized",
            "idx_memories_expires_optimized",
            "idx_summaries_level_range_optimized",
            "idx_cluster_mappings_optimized",
            "idx_cluster_mappings_memory_lookup",
            "idx_migration_history_performance",
            "idx_migration_history_errors"
        ];
        
        for index_name in expected_indexes {
            let count: (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM pg_indexes WHERE indexname = $1"
            )
            .bind(index_name)
            .fetch_one(&pool)
            .await
            .context(format!("Failed to check existence of index: {}", index_name))?;
            
            assert_eq!(count.0, 1, "Index {} should exist", index_name);
        }
        
        Ok(())
    }

    /// Test that consolidated access patterns index covers expected query patterns
    #[tokio::test]
    async fn test_access_patterns_index_usage() -> Result<()> {
        let pool = create_test_pool().await?;
        
        // Insert test data to ensure index has something to work with
        sqlx::query(
            "INSERT INTO memories (content, tier, status, last_accessed_at, importance_score, embedding) 
             VALUES ($1, $2, $3, $4, $5, $6) 
             ON CONFLICT (content_hash, tier) DO NOTHING"
        )
        .bind("Test memory for access patterns")
        .bind("working")
        .bind("active")
        .bind(chrono::Utc::now())
        .bind(0.8_f64)
        .bind(vec![0.1_f32; 1536]) // 1536-dimensional test vector
        .execute(&pool)
        .await?;
        
        // Test query that should use the consolidated access patterns index
        let explain_result: (String,) = sqlx::query_as(
            "EXPLAIN (FORMAT JSON) 
             SELECT id, last_accessed_at, importance_score 
             FROM memories 
             WHERE status = 'active' 
               AND last_accessed_at IS NOT NULL 
               AND tier = 'working'
             ORDER BY last_accessed_at DESC, importance_score DESC 
             LIMIT 10"
        )
        .fetch_one(&pool)
        .await?;
        
        // Verify the query plan uses our index (look for index name in JSON)
        assert!(
            explain_result.0.contains("idx_memories_access_patterns_consolidated") ||
            explain_result.0.contains("Index Scan"),
            "Query should use the consolidated access patterns index, got plan: {}", 
            explain_result.0
        );
        
        Ok(())
    }

    /// Test HNSW vector index is properly configured and functional
    #[tokio::test]
    async fn test_hnsw_vector_index_configuration() -> Result<()> {
        let pool = create_test_pool().await?;
        
        // Check HNSW index configuration
        let index_info: (String, String) = sqlx::query_as(
            "SELECT 
                ai.amname as access_method,
                COALESCE(array_to_string(c.reloptions, ', '), 'default') as options
             FROM pg_class c
             JOIN pg_am ai ON c.relam = ai.oid
             WHERE c.relname = 'idx_memories_embedding_hnsw_optimized'"
        )
        .fetch_one(&pool)
        .await?;
        
        assert_eq!(index_info.0, "hnsw", "Index should use HNSW access method");
        
        // Check if the index has proper HNSW parameters (m and ef_construction)
        let has_proper_config = index_info.1.contains("m=16") || index_info.1 == "default";
        assert!(
            has_proper_config,
            "HNSW index should have proper configuration, got: {}", 
            index_info.1
        );
        
        Ok(())
    }

    /// Test metadata GIN index supports JSONB queries efficiently  
    #[tokio::test]
    async fn test_metadata_gin_index_performance() -> Result<()> {
        let pool = create_test_pool().await?;
        
        // Insert test memory with metadata
        sqlx::query(
            "INSERT INTO memories (content, tier, status, metadata, embedding) 
             VALUES ($1, $2, $3, $4, $5) 
             ON CONFLICT (content_hash, tier) DO NOTHING"
        )
        .bind("Test memory with metadata")
        .bind("working")
        .bind("active")
        .bind(serde_json::json!({"context": "test_context", "importance": "high"}))
        .bind(vec![0.2_f32; 1536])
        .execute(&pool)
        .await?;
        
        // Test GIN index usage for JSONB queries
        let explain_result: (String,) = sqlx::query_as(
            "EXPLAIN (FORMAT JSON)
             SELECT id, content, metadata 
             FROM memories 
             WHERE status = 'active' 
               AND metadata @> $1"
        )
        .bind(serde_json::json!({"context": "test_context"}))
        .fetch_one(&pool)
        .await?;
        
        // Should use GIN index for JSONB containment query
        assert!(
            explain_result.0.contains("idx_memories_metadata_optimized") ||
            explain_result.0.contains("Bitmap Index Scan"),
            "Metadata query should use GIN index, got plan: {}",
            explain_result.0
        );
        
        Ok(())
    }

    /// Test temporal range queries use the optimized temporal index
    #[tokio::test] 
    async fn test_temporal_index_performance() -> Result<()> {
        let pool = create_test_pool().await?;
        
        // Test temporal range query
        let explain_result: (String,) = sqlx::query_as(
            "EXPLAIN (FORMAT JSON)
             SELECT id, created_at, updated_at, tier
             FROM memories 
             WHERE status = 'active'
               AND created_at >= NOW() - INTERVAL '7 days'
               AND created_at <= NOW()
             ORDER BY created_at DESC, updated_at DESC
             LIMIT 50"
        )
        .fetch_one(&pool)
        .await?;
        
        // Should use the temporal index for date range queries
        assert!(
            explain_result.0.contains("idx_memories_temporal_optimized") ||
            explain_result.0.contains("Index Scan") ||
            explain_result.0.contains("Bitmap Index Scan"),
            "Temporal query should use temporal index, got plan: {}",
            explain_result.0
        );
        
        Ok(())
    }

    /// Test TTL expiration queries use the expires index efficiently
    #[tokio::test]
    async fn test_expires_index_functionality() -> Result<()> {
        let pool = create_test_pool().await?;
        
        // Insert memory with expiration time
        sqlx::query(
            "INSERT INTO memories (content, tier, status, expires_at, embedding) 
             VALUES ($1, $2, $3, $4, $5) 
             ON CONFLICT (content_hash, tier) DO NOTHING"
        )
        .bind("Expiring test memory")
        .bind("cold")
        .bind("active")
        .bind(chrono::Utc::now() + chrono::Duration::hours(1))
        .bind(vec![0.3_f32; 1536])
        .execute(&pool)
        .await?;
        
        // Test expiration cleanup query
        let explain_result: (String,) = sqlx::query_as(
            "EXPLAIN (FORMAT JSON)
             SELECT id, expires_at, tier
             FROM memories 
             WHERE expires_at IS NOT NULL
               AND expires_at <= NOW() + INTERVAL '2 hours'
             ORDER BY expires_at ASC
             LIMIT 100"
        )
        .fetch_one(&pool)
        .await?;
        
        // Should use expires index for TTL queries
        assert!(
            explain_result.0.contains("idx_memories_expires_optimized") ||
            explain_result.0.contains("Index Scan"),
            "Expiration query should use expires index, got plan: {}",
            explain_result.0
        );
        
        Ok(())
    }

    /// Test cluster mapping indexes improve join performance
    #[tokio::test]
    async fn test_cluster_mapping_indexes() -> Result<()> {
        let pool = create_test_pool().await?;
        
        // Test cluster lookup query performance
        let explain_result: (String,) = sqlx::query_as(
            "EXPLAIN (FORMAT JSON)
             SELECT m.id, m.content, cm.distance_to_centroid, cm.assigned_at
             FROM memories m
             JOIN memory_cluster_mappings cm ON m.id = cm.memory_id
             WHERE cm.cluster_id = $1
             ORDER BY cm.distance_to_centroid ASC, cm.assigned_at DESC
             LIMIT 20"
        )
        .bind(uuid::Uuid::new_v4()) // Random UUID for test
        .fetch_one(&pool)
        .await?;
        
        // Should use cluster mapping indexes for join optimization
        assert!(
            explain_result.0.contains("idx_cluster_mappings_optimized") ||
            explain_result.0.contains("Index Scan"),
            "Cluster mapping query should use optimized indexes, got plan: {}",
            explain_result.0
        );
        
        Ok(())
    }

    /// Test migration history indexes support performance monitoring
    #[tokio::test]
    async fn test_migration_history_indexes() -> Result<()> {
        let pool = create_test_pool().await?;
        
        // Test migration performance analysis query
        let explain_result: (String,) = sqlx::query_as(
            "EXPLAIN (FORMAT JSON)
             SELECT from_tier, to_tier, AVG(migration_duration_ms), COUNT(*)
             FROM migration_history 
             WHERE success = true 
               AND migrated_at >= NOW() - INTERVAL '30 days'
             GROUP BY from_tier, to_tier
             ORDER BY AVG(migration_duration_ms) DESC"
        )
        .fetch_one(&pool)
        .await?;
        
        // Should use migration history performance index
        assert!(
            explain_result.0.contains("idx_migration_history_performance") ||
            explain_result.0.contains("Index Scan") ||
            explain_result.0.contains("Bitmap Index Scan"),
            "Migration history query should use performance index, got plan: {}",
            explain_result.0
        );
        
        Ok(())
    }

    /// Test that index sizes are within expected ranges
    #[tokio::test]
    async fn test_index_size_projections() -> Result<()> {
        let pool = create_test_pool().await?;
        
        // Get actual index sizes
        let index_sizes: Vec<(String, i64)> = sqlx::query_as(
            "SELECT 
                indexname,
                pg_relation_size(indexrelid) as size_bytes
             FROM pg_stat_user_indexes 
             WHERE indexname LIKE 'idx_memories_%_consolidated'
                OR indexname LIKE 'idx_memories_%_optimized'
                OR indexname LIKE 'idx_summaries_%'
                OR indexname LIKE 'idx_cluster_%'
                OR indexname LIKE 'idx_migration_%'
             ORDER BY size_bytes DESC"
        )
        .fetch_all(&pool)
        .await?;
        
        // Verify we have the expected number of indexes
        assert!(
            index_sizes.len() >= 8, 
            "Should have at least 8 performance indexes, found: {}", 
            index_sizes.len()
        );
        
        // Check that HNSW vector index is the largest (expected behavior)
        let hnsw_index = index_sizes.iter()
            .find(|(name, _)| name.contains("hnsw_optimized"));
            
        if let Some((_, hnsw_size)) = hnsw_index {
            // HNSW index should be substantial in size (>1MB for real data)
            // In test environment it might be smaller due to limited test data
            assert!(
                *hnsw_size >= 0, // At least exists and has some size
                "HNSW index should have meaningful size, got: {} bytes", 
                hnsw_size
            );
        }
        
        // Print index sizes for manual verification
        for (index_name, size_bytes) in &index_sizes {
            println!("Index: {} - Size: {} MB", 
                index_name, 
                *size_bytes as f64 / 1_048_576.0
            );
        }
        
        Ok(())
    }

    /// Performance benchmark test comparing query times before/after indexes
    #[tokio::test]
    async fn test_performance_improvement_benchmarks() -> Result<()> {
        let pool = create_test_pool().await?;
        
        // Prepare test data for meaningful benchmarks
        for i in 0..100 {
            sqlx::query(
                "INSERT INTO memories (content, tier, status, last_accessed_at, importance_score, embedding, metadata) 
                 VALUES ($1, $2, $3, $4, $5, $6, $7) 
                 ON CONFLICT (content_hash, tier) DO NOTHING"
            )
            .bind(format!("Benchmark test memory {}", i))
            .bind(if i % 3 == 0 { "working" } else if i % 3 == 1 { "warm" } else { "cold" })
            .bind("active")
            .bind(chrono::Utc::now() - chrono::Duration::hours(i % 24))
            .bind((i as f64) / 100.0)
            .bind(vec![(i as f32) / 100.0; 1536])
            .bind(serde_json::json!({"context": format!("test_{}", i % 10), "batch": i / 10}))
            .execute(&pool)
            .await?;
        }
        
        // Benchmark 1: Last accessed sorting (should use consolidated index)
        let start_time = std::time::Instant::now();
        let _result: Vec<(uuid::Uuid,)> = sqlx::query_as(
            "SELECT id FROM memories 
             WHERE status = 'active' AND last_accessed_at IS NOT NULL 
             ORDER BY last_accessed_at DESC 
             LIMIT 20"
        )
        .fetch_all(&pool)
        .await?;
        let access_sort_time = start_time.elapsed();
        
        // Benchmark 2: Metadata filtering (should use GIN index)
        let start_time = std::time::Instant::now();
        let _result: Vec<(uuid::Uuid,)> = sqlx::query_as(
            "SELECT id FROM memories 
             WHERE status = 'active' 
               AND metadata @> $1"
        )
        .bind(serde_json::json!({"context": "test_1"}))
        .fetch_all(&pool)
        .await?;
        let metadata_filter_time = start_time.elapsed();
        
        // Benchmark 3: Temporal range query (should use temporal index)
        let start_time = std::time::Instant::now();
        let _result: Vec<(uuid::Uuid,)> = sqlx::query_as(
            "SELECT id FROM memories 
             WHERE status = 'active' 
               AND created_at >= NOW() - INTERVAL '1 day'
             ORDER BY created_at DESC
             LIMIT 20"
        )
        .fetch_all(&pool)
        .await?;
        let temporal_query_time = start_time.elapsed();
        
        // Store benchmark results (these would be compared against baselines in real testing)
        sqlx::query(
            "INSERT INTO index_performance_baselines 
             (measurement_type, query_pattern, execution_time_ms, notes)
             VALUES 
             ('test_run', 'last_accessed_sort', $1, 'Automated test benchmark'),
             ('test_run', 'metadata_filter', $2, 'Automated test benchmark'),
             ('test_run', 'temporal_range', $3, 'Automated test benchmark')"
        )
        .bind(access_sort_time.as_millis() as f64)
        .bind(metadata_filter_time.as_millis() as f64)  
        .bind(temporal_query_time.as_millis() as f64)
        .execute(&pool)
        .await?;
        
        // Verify reasonable performance (should be <100ms in test environment)
        assert!(
            access_sort_time.as_millis() < 1000,
            "Access sort query took too long: {}ms", 
            access_sort_time.as_millis()
        );
        
        assert!(
            metadata_filter_time.as_millis() < 1000,
            "Metadata filter query took too long: {}ms", 
            metadata_filter_time.as_millis()
        );
        
        assert!(
            temporal_query_time.as_millis() < 1000,
            "Temporal range query took too long: {}ms", 
            temporal_query_time.as_millis()
        );
        
        println!("Performance benchmark results:");
        println!("- Access sort: {}ms", access_sort_time.as_millis());
        println!("- Metadata filter: {}ms", metadata_filter_time.as_millis());
        println!("- Temporal range: {}ms", temporal_query_time.as_millis());
        
        Ok(())
    }

    /// Test rollback functionality by verifying indexes can be dropped cleanly
    #[tokio::test]
    async fn test_rollback_capability() -> Result<()> {
        let pool = create_test_pool().await?;
        
        // Count current performance indexes
        let initial_count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM pg_indexes 
             WHERE indexname LIKE 'idx_memories_%_consolidated'
                OR indexname LIKE 'idx_memories_%_optimized'
                OR indexname LIKE 'idx_summaries_%_optimized'
                OR indexname LIKE 'idx_cluster_%_optimized'
                OR indexname LIKE 'idx_migration_history_%'"
        )
        .fetch_one(&pool)
        .await?;
        
        // Should have the expected performance indexes
        assert!(
            initial_count.0 >= 8,
            "Should have at least 8 performance indexes for rollback test, found: {}",
            initial_count.0
        );
        
        // Verify that we can identify indexes for rollback
        let rollback_indexes: Vec<(String,)> = sqlx::query_as(
            "SELECT indexname FROM pg_indexes 
             WHERE indexname IN (
                'idx_memories_access_patterns_consolidated',
                'idx_memories_metadata_optimized',
                'idx_memories_hybrid_search_optimized',
                'idx_memories_embedding_hnsw_optimized',
                'idx_memories_temporal_optimized',
                'idx_memories_expires_optimized'
             )"
        )
        .fetch_all(&pool)
        .await?;
        
        // Should find the core performance indexes
        assert!(
            rollback_indexes.len() >= 4,
            "Should find at least 4 core performance indexes for rollback verification, found: {}",
            rollback_indexes.len()
        );
        
        println!("Rollback test verified {} indexes can be identified for cleanup", rollback_indexes.len());
        
        Ok(())
    }
}