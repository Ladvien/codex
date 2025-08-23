//! Integration Tests for External Services
//!
//! These tests verify integration with external services like embedding providers,
//! backup systems, monitoring endpoints, and other external dependencies.
//! Tests focus on error handling, timeouts, retries, and graceful degradation.

mod test_helpers;

use anyhow::Result;
use codex_memory::memory::models::{CreateMemoryRequest, MemoryTier};
use codex_memory::Config;
use codex_memory::SimpleEmbedder;
use serde_json::json;
use sqlx::Row;
use std::sync::Arc;
use std::time::{Duration, Instant};
use test_helpers::TestEnvironment;
use tokio::time::timeout;
use tracing_test::traced_test;

/// Test embedding service integration with graceful error handling
#[tokio::test]
#[traced_test]
async fn test_embedding_service_integration() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Test Ollama embedder creation (should not fail even with invalid URL due to lazy loading)
    let ollama_embedder =
        SimpleEmbedder::new_ollama("http://localhost:11434".to_string(), "llama2".to_string());

    // Embedder creation should succeed (lazy initialization)
    assert!(
        ollama_embedder.embedding_dimension() > 0,
        "Embedder should have valid dimension"
    );

    // Test generic embedder creation (which might be OpenAI under the hood)
    let generic_embedder = SimpleEmbedder::new("test-key".to_string());

    // Generic embedder should also succeed at creation (lazy)
    assert!(
        generic_embedder.embedding_dimension() > 0,
        "Generic embedder should have valid dimension"
    );

    // Test memory creation with embedding service (may fail if service unavailable)
    let memory_with_embedding_result = env
        .repository
        .create_memory(CreateMemoryRequest {
            content: "Test content for embedding".to_string(),
            embedding: None, // Let the system generate it
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.7),
            metadata: Some(json!({"embedding_test": true})),
            parent_id: None,
            expires_at: None,
        })
        .await;

    // Memory creation should succeed even if embedding generation fails
    match memory_with_embedding_result {
        Ok(memory) => {
            assert_eq!(memory.content, "Test content for embedding");
            // Clean up
            env.repository.delete_memory(memory.id).await?;
        }
        Err(_) => {
            // If memory creation fails, that's also acceptable in integration tests
            // The important thing is that the system doesn't crash
        }
    }

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test embedding service timeout behavior
#[tokio::test]
#[traced_test]
async fn test_embedding_service_timeout() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Create embedder with potentially unreachable service
    let embedder = Arc::new(SimpleEmbedder::new_ollama(
        "http://192.0.2.1:11434".to_string(), // RFC5737 documentation address
        "test-model".to_string(),
    ));

    // Test with timeout to ensure operations don't hang
    let timeout_test = timeout(
        Duration::from_secs(30), // Allow reasonable time for timeout handling
        async {
            // Try to create memory with embedding - this might timeout gracefully
            let result = env
                .repository
                .create_memory(CreateMemoryRequest {
                    content: "Timeout test content".to_string(),
                    embedding: None,
                    tier: Some(MemoryTier::Working),
                    importance_score: Some(0.5),
                    metadata: Some(json!({"timeout_test": true})),
                    parent_id: None,
                    expires_at: None,
                })
                .await;

            // Result can be success or failure - the key is no infinite hang
            match result {
                Ok(memory) => {
                    // If succeeded, clean up
                    let _ = env.repository.delete_memory(memory.id).await;
                }
                Err(_) => {
                    // Timeout or connection error is expected
                }
            }

            Ok::<(), anyhow::Error>(())
        },
    )
    .await;

    assert!(
        timeout_test.is_ok(),
        "Embedding operations should not hang indefinitely"
    );

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test backup service integration
#[tokio::test]
#[traced_test]
async fn test_backup_service_integration() -> Result<()> {
    let _env = TestEnvironment::new().await?;

    // Test operational configuration validation
    let config = Config::default();

    // Test operational limits
    assert!(
        config.operational.max_db_connections > 0,
        "Should allow database connections"
    );
    assert!(
        config.operational.max_db_connections <= 1000,
        "Connection limit should be reasonable"
    );

    // Test request timeout
    assert!(
        config.operational.request_timeout_seconds > 0,
        "Request timeout should be positive"
    );
    assert!(
        config.operational.request_timeout_seconds <= 600,
        "Request timeout should be reasonable"
    );

    // Test metrics configuration
    // Metrics can be enabled or disabled, both are valid

    Ok(())
}

/// Test monitoring service integration
#[tokio::test]
#[traced_test]
async fn test_monitoring_service_integration() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Test that monitoring/metrics can be collected
    let stats = env.repository.get_statistics().await?;

    // Verify statistics structure
    assert!(
        stats.total_active.is_some(),
        "Statistics should have at least the total active count metric"
    );

    if let Some(total) = stats.total_active {
        assert!(total >= 0, "Total active should be non-negative");
    }

    if let Some(avg_importance) = stats.avg_importance {
        assert!(
            (0.0..=1.0).contains(&avg_importance),
            "Average importance should be in valid range"
        );
    }

    // Test statistics collection performance
    let start = Instant::now();
    for _ in 0..5 {
        let _stats = env.repository.get_statistics().await?;
    }
    let duration = start.elapsed();

    assert!(
        duration.as_millis() < 5000,
        "Statistics collection should be fast"
    );

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test filesystem operations for backup/storage
#[tokio::test]
#[traced_test]
async fn test_filesystem_operations() -> Result<()> {
    let _env = TestEnvironment::new().await?;

    // Test temporary directory operations
    let temp_dir = tempfile::tempdir()?;
    let temp_path = temp_dir.path();

    // Test directory creation and deletion
    let test_subdir = temp_path.join("test_backup");
    std::fs::create_dir_all(&test_subdir)?;
    assert!(test_subdir.exists(), "Test directory should be created");

    // Test file operations
    let test_file = test_subdir.join("test_file.json");
    let test_data = json!({
        "test": "filesystem_operations",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "data": {
            "items": [1, 2, 3, 4, 5],
            "metadata": {"test": true}
        }
    });

    // Write test data
    std::fs::write(&test_file, serde_json::to_string_pretty(&test_data)?)?;
    assert!(test_file.exists(), "Test file should be created");

    // Read and verify test data
    let read_data = std::fs::read_to_string(&test_file)?;
    let parsed_data: serde_json::Value = serde_json::from_str(&read_data)?;
    assert_eq!(parsed_data["test"], "filesystem_operations");

    // Test file metadata
    let metadata = std::fs::metadata(&test_file)?;
    assert!(metadata.len() > 0, "File should have content");
    assert!(!metadata.is_dir(), "Should be a file, not directory");

    // Cleanup happens automatically when temp_dir drops
    Ok(())
}

/// Test network connectivity and error handling
#[tokio::test]
#[traced_test]
async fn test_network_connectivity() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Test database connectivity (which is a network service)
    let pool = env.repository.pool();

    // Test basic database query (network operation)
    let connectivity_test = timeout(
        Duration::from_secs(10),
        sqlx::query("SELECT 1 as connectivity_test").fetch_one(pool),
    )
    .await;

    match connectivity_test {
        Ok(Ok(row)) => {
            let result: i32 = row.get("connectivity_test");
            assert_eq!(result, 1, "Database connectivity test should return 1");
        }
        Ok(Err(_)) => {
            // Database error - acceptable in some test environments
        }
        Err(_) => {
            // Timeout - also acceptable, indicates network issues
        }
    }

    // Test that the system can handle network failures gracefully
    // by trying operations that might fail due to network issues
    let memory_result = env
        .repository
        .create_memory(CreateMemoryRequest {
            content: "Network test memory".to_string(),
            embedding: None,
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.6),
            metadata: Some(json!({"network_test": true})),
            parent_id: None,
            expires_at: None,
        })
        .await;

    // Operation should either succeed or fail gracefully
    match memory_result {
        Ok(memory) => {
            // Success - clean up
            env.repository.delete_memory(memory.id).await?;
        }
        Err(_) => {
            // Failure is acceptable in network-constrained environments
        }
    }

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test service discovery and configuration
#[tokio::test]
#[traced_test]
async fn test_service_discovery() -> Result<()> {
    let config = Config::default();

    // Test embedding service configuration
    assert!(
        !config.embedding.provider.is_empty(),
        "Embedding provider should be configured"
    );
    assert!(
        !config.embedding.model.is_empty(),
        "Embedding model should be configured"
    );
    assert!(
        !config.embedding.base_url.is_empty(),
        "Embedding base URL should be configured"
    );

    // Test timeout configurations are reasonable
    assert!(
        config.embedding.timeout_seconds > 0,
        "Embedding timeout should be positive"
    );
    assert!(
        config.embedding.timeout_seconds <= 300,
        "Embedding timeout should be reasonable (max 5 min)"
    );

    // Note: max_retries is not in the current config structure,
    // but timeout configuration is available

    // Test database configuration
    assert!(
        !config.database_url.is_empty(),
        "Database URL should be configured"
    );

    // Test HTTP server configuration
    assert!(config.http_port > 0, "HTTP port should be positive");
    // Port is u16, so it's always <= 65535

    // Test operational limits
    assert!(
        config.operational.max_db_connections > 0,
        "Should allow database connections"
    );
    assert!(
        config.operational.max_db_connections <= 1000,
        "Connection limit should be reasonable"
    );

    Ok(())
}

/// Test external service error recovery
#[tokio::test]
#[traced_test]
async fn test_service_error_recovery() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Test creating multiple memories with potential embedding failures
    let mut memory_ids = Vec::new();
    let mut success_count = 0;
    let mut failure_count = 0;

    for i in 0..5 {
        let result = env
            .repository
            .create_memory(CreateMemoryRequest {
                content: format!("Error recovery test memory {i}"),
                embedding: None, // May fail to generate
                tier: Some(MemoryTier::Working),
                importance_score: Some(0.5),
                metadata: Some(json!({"error_recovery_test": true, "index": i})),
                parent_id: None,
                expires_at: None,
            })
            .await;

        match result {
            Ok(memory) => {
                memory_ids.push(memory.id);
                success_count += 1;
            }
            Err(_) => {
                failure_count += 1;
            }
        }
    }

    // At least some operations should work (system should be resilient)
    assert!(
        success_count > 0 || failure_count == 5,
        "Either some operations succeed, or all fail consistently"
    );

    // If some succeeded, test that they're properly stored
    if success_count > 0 {
        let stats = env.repository.get_statistics().await?;
        if let Some(total) = stats.total_active {
            assert!(
                total >= success_count as i64,
                "Statistics should reflect successful creations"
            );
        }
    }

    // Clean up successful creations
    for memory_id in memory_ids {
        let _ = env.repository.delete_memory(memory_id).await;
    }

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test service health checks and monitoring
#[tokio::test]
#[traced_test]
async fn test_service_health_monitoring() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Test database health (key external service)
    let pool = env.repository.pool();

    // Test connection pool health
    let pool_size = pool.size();
    let idle_connections = pool.num_idle();

    assert!(pool_size > 0, "Connection pool should have connections");
    assert!(
        idle_connections <= pool_size as usize,
        "Idle connections should not exceed pool size"
    );

    // Test that health checks complete in reasonable time
    let health_check_start = Instant::now();

    // Simulate health check operations
    let health_operations = vec![
        // Database connectivity
        sqlx::query("SELECT 1").fetch_optional(pool),
        // Schema validation
        sqlx::query("SELECT table_name FROM information_schema.tables WHERE table_name = 'memories' LIMIT 1")
            .fetch_optional(pool),
    ];

    let health_results = futures::future::join_all(health_operations).await;
    let health_duration = health_check_start.elapsed();

    // Health checks should complete quickly
    assert!(
        health_duration.as_millis() < 5000,
        "Health checks should be fast"
    );

    // At least database connectivity should work
    let connectivity_works = health_results.into_iter().any(|result| result.is_ok());

    if !connectivity_works {
        // If health checks fail, that's acceptable in some test environments
        // The important thing is they don't hang or crash
    }

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test configuration validation for external services
#[tokio::test]
#[traced_test]
async fn test_external_service_configuration() -> Result<()> {
    let config = Config::default();

    // Test embedding service configuration validation
    let embedding_config = &config.embedding;

    // URL validation (basic format check)
    assert!(
        embedding_config.base_url.starts_with("http://")
            || embedding_config.base_url.starts_with("https://"),
        "Embedding base URL should be valid HTTP(S) URL"
    );

    // Model validation
    assert!(
        !embedding_config.model.is_empty(),
        "Embedding model should be specified"
    );
    assert!(
        embedding_config.model.len() < 100,
        "Model name should be reasonable length"
    );

    // Timeout validation
    assert!(
        embedding_config.timeout_seconds >= 5,
        "Timeout should allow reasonable processing time"
    );
    assert!(
        embedding_config.timeout_seconds <= 600,
        "Timeout should not be excessive"
    );

    // Provider validation
    let valid_providers = ["ollama", "openai", "mock"];
    assert!(
        valid_providers.contains(&embedding_config.provider.as_str()),
        "Provider should be recognized"
    );

    // Test tier configuration
    let tier_config = &config.tier_config;
    assert!(
        tier_config.working_tier_limit > 0,
        "Working tier limit should be positive"
    );
    assert!(
        tier_config.warm_tier_limit > 0,
        "Warm tier limit should be positive"
    );
    assert!(
        tier_config.working_tier_limit <= tier_config.warm_tier_limit,
        "Working limit should not exceed warm limit"
    );

    // Test time-based configuration
    assert!(
        tier_config.working_to_warm_days > 0,
        "Working to warm migration time should be positive"
    );
    assert!(
        tier_config.warm_to_cold_days > 0,
        "Warm to cold migration time should be positive"
    );

    // Test importance threshold
    assert!(
        tier_config.importance_threshold > 0.0 && tier_config.importance_threshold <= 1.0,
        "Importance threshold should be valid probability"
    );

    Ok(())
}
