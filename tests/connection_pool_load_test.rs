use codex_memory::memory::connection::{create_pool, monitor_vector_pool_health};
use sqlx::{PgPool, Row};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::timeout;
use tracing::{info, warn};

/// Connection Pool Load Testing for Vector Workloads
/// 
/// These tests validate that the connection pool can handle:
/// 1. High concurrency vector operations without exhaustion
/// 2. Connection saturation monitoring and alerting at 70%
/// 3. Recovery from pool exhaustion scenarios
/// 4. Sustained vector workload performance

#[cfg(test)]
mod connection_pool_load_tests {
    use super::*;

    /// Test concurrent vector similarity searches
    #[tokio::test]
    #[ignore = "requires database connection"]
    async fn test_concurrent_vector_searches() -> anyhow::Result<()> {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:postgres@localhost/codex_memory_test".to_string());
        
        // Create pool with increased connections for vector workloads
        let pool = Arc::new(create_pool(&database_url, 100).await?);
        
        let start_time = Instant::now();
        let mut handles = vec![];
        
        // Launch 50 concurrent vector similarity searches
        for i in 0..50 {
            let pool_clone = pool.clone();
            let handle = tokio::spawn(async move {
                let search_vector = format!("[{}]", (0..1536).map(|j| (i + j) as f32 / 1536.0).collect::<Vec<f32>>().iter().map(|x| x.to_string()).collect::<Vec<String>>().join(","));
                
                // Simulate vector similarity search that holds connection
                let result = sqlx::query(&format!(
                    "SELECT id, embedding <=> '{}' as distance FROM memories ORDER BY distance LIMIT 10",
                    search_vector
                ))
                .fetch_all(&*pool_clone)
                .await;
                
                match result {
                    Ok(rows) => {
                        info!("Search {} completed with {} results", i, rows.len());
                        Ok(rows.len())
                    }
                    Err(e) => {
                        warn!("Search {} failed: {}", i, e);
                        Err(e)
                    }
                }
            });
            handles.push(handle);
        }
        
        // Wait for all searches to complete with timeout
        let mut successes = 0;
        let mut failures = 0;
        
        for handle in handles {
            match timeout(Duration::from_secs(30), handle).await {
                Ok(Ok(Ok(_))) => successes += 1,
                Ok(Ok(Err(_))) => failures += 1,
                Ok(Err(_)) => failures += 1,
                Err(_) => failures += 1,
            }
        }
        
        let duration = start_time.elapsed();
        info!(
            "Concurrent vector search test completed: {}/{} successes in {:.2}s",
            successes, successes + failures, duration.as_secs_f64()
        );
        
        // Verify no connection exhaustion
        assert!(successes > 40, "Expected >40 successful searches, got {}", successes);
        assert!(duration < Duration::from_secs(20), "Test took too long: {:.2}s", duration.as_secs_f64());
        
        Ok(())
    }
    
    /// Test connection pool saturation monitoring
    #[tokio::test]
    #[ignore = "requires database connection"]
    async fn test_pool_saturation_monitoring() -> anyhow::Result<()> {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:postgres@localhost/codex_memory_test".to_string());
        
        // Create smaller pool to easily reach saturation
        let pool = Arc::new(create_pool(&database_url, 10).await?);
        
        // Launch tasks that hold connections to force saturation
        let mut handles = vec![];
        for i in 0..15 {  // More tasks than pool size
            let pool_clone = pool.clone();
            let handle = tokio::spawn(async move {
                // Hold connection for 5 seconds to create backpressure
                let _result = sqlx::query("SELECT pg_sleep(5)")
                    .execute(&*pool_clone)
                    .await;
                i
            });
            handles.push(handle);
        }
        
        // Monitor pool health while under load
        tokio::time::sleep(Duration::from_secs(1)).await;
        
        for _ in 0..5 {
            let is_healthy = monitor_vector_pool_health(&pool, "test_pool").await?;
            info!("Pool health check: {}", is_healthy);
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        
        // Wait for tasks to complete
        for handle in handles {
            let _ = handle.await;
        }
        
        Ok(())
    }
    
    /// Test sustained vector workload performance
    #[tokio::test]
    #[ignore = "requires database connection"]
    async fn test_sustained_vector_workload() -> anyhow::Result<()> {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:postgres@localhost/codex_memory_test".to_string());
        
        let pool = Arc::new(create_pool(&database_url, 100).await?);
        
        // Run sustained workload for 2 minutes
        let test_duration = Duration::from_secs(120);
        let start_time = Instant::now();
        let mut operation_count = 0;
        
        while start_time.elapsed() < test_duration {
            let mut batch_handles = vec![];
            
            // Launch batch of 10 concurrent operations
            for _ in 0..10 {
                let pool_clone = pool.clone();
                let handle = tokio::spawn(async move {
                    // Simulate complex vector operation
                    sqlx::query("SELECT vector_dims(random()::text::vector)")
                        .fetch_one(&*pool_clone)
                        .await
                });
                batch_handles.push(handle);
            }
            
            // Wait for batch completion
            let mut batch_successes = 0;
            for handle in batch_handles {
                if handle.await.is_ok() {
                    batch_successes += 1;
                }
            }
            
            operation_count += batch_successes;
            
            // Check pool health every 30 seconds
            if start_time.elapsed().as_secs() % 30 == 0 {
                let _ = monitor_vector_pool_health(&pool, "sustained_test").await;
            }
            
            // Brief pause between batches
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        
        let ops_per_second = operation_count as f64 / test_duration.as_secs_f64();
        info!(
            "Sustained workload test completed: {} operations ({:.1} ops/sec)",
            operation_count, ops_per_second
        );
        
        // Verify sustained performance
        assert!(ops_per_second > 50.0, "Expected >50 ops/sec, got {:.1}", ops_per_second);
        
        Ok(())
    }
    
    /// Test connection pool recovery from exhaustion
    #[tokio::test]
    #[ignore = "requires database connection"]
    async fn test_pool_recovery_from_exhaustion() -> anyhow::Result<()> {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:postgres@localhost/codex_memory_test".to_string());
        
        let pool = Arc::new(create_pool(&database_url, 5).await?); // Small pool
        
        // Exhaust the pool
        let blocking_handles: Vec<_> = (0..10).map(|_| {
            let pool_clone = pool.clone();
            tokio::spawn(async move {
                sqlx::query("SELECT pg_sleep(10)")
                    .execute(&*pool_clone)
                    .await
            })
        }).collect();
        
        // Wait for exhaustion
        tokio::time::sleep(Duration::from_secs(2)).await;
        
        // Verify pool is saturated
        let is_healthy = monitor_vector_pool_health(&pool, "recovery_test").await?;
        assert!(!is_healthy, "Pool should be unhealthy when saturated");
        
        // Cancel blocking operations to simulate recovery
        for handle in blocking_handles {
            handle.abort();
        }
        
        // Wait for recovery
        tokio::time::sleep(Duration::from_secs(2)).await;
        
        // Verify recovery
        let recovery_result = sqlx::query("SELECT 1")
            .fetch_one(&*pool)
            .await;
        
        assert!(recovery_result.is_ok(), "Pool should recover after connection release");
        
        let final_health = monitor_vector_pool_health(&pool, "recovery_test").await?;
        info!("Pool recovery status: {}", final_health);
        
        Ok(())
    }
}

/// Integration tests for production scenarios
#[cfg(test)]
mod integration_tests {
    use super::*;
    
    /// Test vector embedding insertion with high concurrency
    #[tokio::test]
    #[ignore = "requires database connection"]
    async fn test_concurrent_vector_insertions() -> anyhow::Result<()> {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:postgres@localhost/codex_memory_test".to_string());
        
        let pool = Arc::new(create_pool(&database_url, 100).await?);
        
        let mut handles = vec![];
        let insertion_count = 100;
        
        for i in 0..insertion_count {
            let pool_clone = pool.clone();
            let handle = tokio::spawn(async move {
                let embedding: Vec<f32> = (0..1536).map(|j| ((i + j) as f32).sin()).collect();
                let embedding_str = format!("[{}]", embedding.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(","));
                
                sqlx::query!(
                    "INSERT INTO memories (content, embedding, tier, importance_score) VALUES ($1, $2, 'working', $3) ON CONFLICT DO NOTHING",
                    format!("Test memory {}", i),
                    embedding_str,
                    0.5
                ).execute(&*pool_clone).await
            });
            handles.push(handle);
        }
        
        let start_time = Instant::now();
        let mut successful_insertions = 0;
        
        for handle in handles {
            if handle.await.is_ok() {
                successful_insertions += 1;
            }
        }
        
        let duration = start_time.elapsed();
        let insertions_per_second = successful_insertions as f64 / duration.as_secs_f64();
        
        info!(
            "Vector insertion test: {}/{} successful insertions in {:.2}s ({:.1} insertions/sec)",
            successful_insertions, insertion_count, duration.as_secs_f64(), insertions_per_second
        );
        
        assert!(successful_insertions >= insertion_count * 95 / 100, "Expected >95% success rate");
        assert!(insertions_per_second > 10.0, "Expected >10 insertions/sec");
        
        Ok(())
    }
}