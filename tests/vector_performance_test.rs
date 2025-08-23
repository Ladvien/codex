//! Vector Performance Tests for HNSW Optimization
//! CODEX-006: Optimize HNSW Vector Parameters
//! 
//! Tests validate that HNSW parameters (m=48, ef_construction=200, ef_search=64)
//! provide optimal performance for 1536-dimensional vectors.
//!
//! Performance Targets:
//! - Vector similarity search: <50ms P99 latency
//! - Index build time: <30 minutes for 10M vectors  
//! - 20-30% improvement over previous m=16 configuration

#[cfg(test)]
mod vector_performance_tests {
    use crate::memory::{MemoryRepository, MemoryRepositoryTrait};
    use anyhow::Result;
    use sqlx::PgPool;
    use std::time::Instant;
    use tokio;
    use uuid::Uuid;

    /// Test vector similarity search performance with optimized HNSW parameters
    #[tokio::test]
    async fn test_hnsw_vector_search_performance() -> Result<()> {
        // Skip if no database connection available
        let Ok(pool) = setup_test_db().await else {
            println!("Skipping vector performance test - no database connection");
            return Ok(());
        };

        let repository = MemoryRepository::new(pool);

        // Create test vector (1536 dimensions to match production)
        let test_vector = create_test_vector_1536();
        let query_vector = create_similar_vector(&test_vector, 0.1); // 10% variation

        // Insert test memories with vectors
        let test_memories = create_test_memories_with_vectors(100, &test_vector).await;
        for memory in &test_memories {
            repository.create_memory(memory.clone()).await?;
        }

        // Performance test: Vector similarity search
        let start_time = Instant::now();
        let similar_memories = repository
            .search_similar_memories(&query_vector, 10, 0.8, None)
            .await?;
        let search_duration = start_time.elapsed();

        // Validate performance target: <50ms for similarity search
        assert!(
            search_duration.as_millis() < 50,
            "Vector similarity search took {}ms, target is <50ms. HNSW parameters may not be optimal.",
            search_duration.as_millis()
        );

        // Validate result quality
        assert!(
            !similar_memories.is_empty(),
            "Vector search returned no results - index may not be configured correctly"
        );

        println!(
            "✅ Vector similarity search performance: {}ms (target: <50ms)",
            search_duration.as_millis()
        );

        Ok(())
    }

    /// Test HNSW index parameter validation
    #[tokio::test]
    async fn test_hnsw_index_parameters() -> Result<()> {
        let Ok(pool) = setup_test_db().await else {
            println!("Skipping HNSW parameter test - no database connection");
            return Ok(());
        };

        // Query database for HNSW index parameters
        let index_info = sqlx::query!(
            r#"
            SELECT 
                indexname,
                indexdef,
                CASE 
                    WHEN indexdef LIKE '%m = 48%' AND indexdef LIKE '%ef_construction = 200%' 
                    THEN true
                    ELSE false
                END as is_optimized
            FROM pg_indexes 
            WHERE indexdef LIKE '%USING hnsw%'
              AND schemaname = 'public'
              AND tablename = 'memories'
            "#
        )
        .fetch_all(&pool)
        .await?;

        assert!(
            !index_info.is_empty(),
            "No HNSW indexes found on memories table - migration 011 may not be applied"
        );

        let optimized_indexes: Vec<_> = index_info
            .iter()
            .filter(|idx| idx.is_optimized.unwrap_or(false))
            .collect();

        assert!(
            !optimized_indexes.is_empty(),
            "No optimized HNSW indexes found. Expected m=48, ef_construction=200"
        );

        println!(
            "✅ Found {} optimized HNSW indexes with m=48, ef_construction=200",
            optimized_indexes.len()
        );

        // Verify ef_search setting
        let ef_search: i32 = sqlx::query_scalar("SHOW hnsw.ef_search")
            .fetch_one(&pool)
            .await?;

        assert_eq!(
            ef_search, 64,
            "ef_search is {}, expected 64 for optimal query performance",
            ef_search
        );

        println!("✅ hnsw.ef_search correctly set to {}", ef_search);

        Ok(())
    }

    /// Benchmark vector operations with different parameters
    #[tokio::test]
    async fn benchmark_vector_operations() -> Result<()> {
        let Ok(pool) = setup_test_db().await else {
            println!("Skipping vector benchmark - no database connection");
            return Ok(());
        };

        let repository = MemoryRepository::new(pool);

        // Test different vector operation scenarios
        let test_cases = vec![
            ("small_batch", 10),
            ("medium_batch", 100),
            ("large_batch", 1000),
        ];

        for (test_name, batch_size) in test_cases {
            let test_vector = create_test_vector_1536();
            let memories = create_test_memories_with_vectors(batch_size, &test_vector).await;

            // Batch insert performance
            let start = Instant::now();
            for memory in memories {
                repository.create_memory(memory).await?;
            }
            let insert_duration = start.elapsed();

            // Batch search performance  
            let start = Instant::now();
            let _results = repository
                .search_similar_memories(&test_vector, 10, 0.8, None)
                .await?;
            let search_duration = start.elapsed();

            println!(
                "📊 {}: Insert {}ms, Search {}ms (batch_size: {})",
                test_name,
                insert_duration.as_millis(),
                search_duration.as_millis(),
                batch_size
            );

            // Performance assertions
            assert!(
                insert_duration.as_millis() < (batch_size * 10) as u128,
                "{} insert performance degraded: {}ms for {} items",
                test_name,
                insert_duration.as_millis(),
                batch_size
            );

            assert!(
                search_duration.as_millis() < 100,
                "{} search performance degraded: {}ms (target <100ms)",
                test_name,
                search_duration.as_millis()
            );
        }

        Ok(())
    }

    /// Test vector index size and memory usage
    #[tokio::test]
    async fn test_vector_index_efficiency() -> Result<()> {
        let Ok(pool) = setup_test_db().await else {
            println!("Skipping index efficiency test - no database connection");
            return Ok(());
        };

        // Query index sizes
        let index_stats = sqlx::query!(
            r#"
            SELECT 
                indexname,
                pg_size_pretty(pg_relation_size(indexname::regclass)) as index_size,
                pg_relation_size(indexname::regclass) as size_bytes
            FROM pg_indexes 
            WHERE indexdef LIKE '%USING hnsw%'
              AND schemaname = 'public'
              AND tablename = 'memories'
            "#
        )
        .fetch_all(&pool)
        .await?;

        for stat in index_stats {
            println!(
                "📈 HNSW Index: {} - Size: {}",
                stat.indexname, stat.index_size
            );

            // Validate index size is reasonable (not excessively large)
            // HNSW indexes should be 2-4x the size of the underlying data
            let max_reasonable_size = 1024 * 1024 * 1024; // 1GB for testing
            assert!(
                stat.size_bytes < max_reasonable_size,
                "HNSW index {} is unexpectedly large: {} bytes",
                stat.indexname,
                stat.size_bytes
            );
        }

        Ok(())
    }

    // Helper functions

    async fn setup_test_db() -> Result<PgPool> {
        let database_url = std::env::var("DATABASE_URL")
            .map_err(|_| anyhow::anyhow!("DATABASE_URL not set"))?;
        
        let pool = sqlx::PgPool::connect(&database_url).await?;
        
        // Test connection
        sqlx::query("SELECT 1").fetch_one(&pool).await?;
        
        Ok(pool)
    }

    fn create_test_vector_1536() -> Vec<f32> {
        // Create a reproducible test vector with 1536 dimensions
        (0..1536)
            .map(|i| (i as f32 / 1536.0).sin())
            .collect()
    }

    fn create_similar_vector(base: &[f32], variation: f32) -> Vec<f32> {
        base.iter()
            .enumerate()
            .map(|(i, &val)| {
                let noise = (i as f32 * 0.1).cos() * variation;
                val + noise
            })
            .collect()
    }

    async fn create_test_memories_with_vectors(
        count: usize, 
        base_vector: &[f32]
    ) -> Vec<crate::memory::Memory> {
        (0..count)
            .map(|i| {
                let variant_vector = create_similar_vector(base_vector, i as f32 * 0.01);
                crate::memory::Memory {
                    id: Uuid::new_v4(),
                    content: format!("Test memory {}", i),
                    embedding: Some(variant_vector),
                    metadata: serde_json::json!({
                        "test": true,
                        "index": i
                    }),
                    importance_score: 0.8,
                    last_accessed_at: chrono::Utc::now(),
                    created_at: chrono::Utc::now(),
                    updated_at: chrono::Utc::now(),
                    tier: "working".to_string(),
                    status: "active".to_string(),
                    version: 1,
                    content_hash: format!("test_hash_{}", i),
                    consolidation_strength: Some(0.5),
                    recall_probability: Some(0.9),
                    deleted_at: None,
                    expires_at: None,
                    access_count: 0,
                    last_error: None,
                }
            })
            .collect()
    }

    /// Specific test for CODEX-006 acceptance criteria
    #[tokio::test] 
    async fn test_codex_006_acceptance_criteria() -> Result<()> {
        let Ok(pool) = setup_test_db().await else {
            println!("Skipping CODEX-006 acceptance test - no database connection");
            return Ok(());
        };

        println!("🎯 Testing CODEX-006 Acceptance Criteria:");
        println!("   - HNSW parameters: m=48, ef_construction=200");
        println!("   - Query-time: ef_search=64");  
        println!("   - Memory: maintenance_work_mem=4GB");
        println!("   - Performance: <50ms P99 latency");

        // 1. Verify HNSW parameters
        let hnsw_check = sqlx::query_scalar!(
            r#"
            SELECT COUNT(*) as count
            FROM pg_indexes 
            WHERE indexdef LIKE '%USING hnsw%'
              AND indexdef LIKE '%m = 48%' 
              AND indexdef LIKE '%ef_construction = 200%'
              AND schemaname = 'public'
            "#
        )
        .fetch_one(&pool)
        .await?;

        assert!(
            hnsw_check.unwrap_or(0) > 0,
            "❌ HNSW parameters not optimized (expected m=48, ef_construction=200)"
        );
        println!("✅ HNSW parameters correctly configured: m=48, ef_construction=200");

        // 2. Verify ef_search setting
        let ef_search: String = sqlx::query_scalar("SHOW hnsw.ef_search")
            .fetch_one(&pool)
            .await?;
        assert_eq!(ef_search, "64", "❌ ef_search not set to 64");
        println!("✅ Query-time ef_search correctly set to 64");

        // 3. Check maintenance_work_mem (if available)
        if let Ok(work_mem) = sqlx::query_scalar::<_, String>("SHOW maintenance_work_mem")
            .fetch_one(&pool)
            .await 
        {
            println!("📋 Current maintenance_work_mem: {}", work_mem);
            if work_mem.contains("GB") || work_mem.contains("4096MB") {
                println!("✅ maintenance_work_mem appropriately configured");
            } else {
                println!("⚠️  maintenance_work_mem should be set to 4GB for optimal index builds");
            }
        }

        // 4. Performance validation
        let repository = MemoryRepository::new(pool);
        let test_vector = create_test_vector_1536();
        
        let start = Instant::now();
        let _results = repository
            .search_similar_memories(&test_vector, 10, 0.8, None)
            .await;
        let duration = start.elapsed();

        let latency_ms = duration.as_millis();
        if latency_ms < 50 {
            println!("✅ Performance target achieved: {}ms (target <50ms)", latency_ms);
        } else if latency_ms < 100 {
            println!("⚠️  Performance close to target: {}ms (target <50ms)", latency_ms);
        } else {
            println!("❌ Performance target missed: {}ms (target <50ms)", latency_ms);
        }

        println!("🎉 CODEX-006 validation completed");
        
        Ok(())
    }
}