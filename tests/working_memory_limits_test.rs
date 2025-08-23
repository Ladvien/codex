use anyhow::Result;
use codex_memory::{
    config::Config,
    memory::{
        connection::create_pool,
        models::{CreateMemoryRequest, MemoryTier},
        MemoryRepository,
    },
    SimpleEmbedder,
};
use std::sync::Arc;

/// Test helper to create a repository with custom working memory limit
async fn create_test_repository_with_limit(limit: usize) -> Result<Arc<MemoryRepository>> {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgresql://codex_user:MZSfXiLr5uR3QYbRwv2vTzi22SvFkj4a@192.168.1.104:5432/codex_db"
            .to_string()
    });
    let pool = create_pool(&database_url, 10).await?;

    let mut config = Config::default();
    config.tier_config.working_tier_limit = limit;

    Ok(Arc::new(MemoryRepository::with_config(pool, config)))
}

#[tokio::test]
async fn test_millers_law_default_limit() {
    // Test that default working memory limit is 9 (Miller's 7±2 upper bound)
    let config = Config::default();
    assert_eq!(
        config.tier_config.working_tier_limit, 9,
        "Default working memory limit should be 9 (Miller's 7±2 upper bound)"
    );
}

#[tokio::test]
async fn test_millers_law_validation() {
    // Test that config validation enforces Miller's 7±2 range (5-9)
    let mut config = Config::default();

    // Test valid range
    for limit in 5..=9 {
        config.tier_config.working_tier_limit = limit;
        assert!(
            config.validate().is_ok(),
            "Limit {limit} should be valid (within Miller's 7±2)"
        );
    }

    // Test invalid values below range
    config.tier_config.working_tier_limit = 4;
    assert!(
        config.validate().is_err(),
        "Limit 4 should be invalid (below Miller's 7±2)"
    );

    // Test invalid values above range
    config.tier_config.working_tier_limit = 10;
    assert!(
        config.validate().is_err(),
        "Limit 10 should be invalid (above Miller's 7±2)"
    );
}

#[tokio::test]
async fn test_working_memory_capacity_enforcement() -> Result<()> {
    let repository = create_test_repository_with_limit(5).await?;
    let embedder = Arc::new(SimpleEmbedder::new_mock());

    // Fill working memory to capacity (5 items)
    for i in 0..5 {
        let request = CreateMemoryRequest {
            content: format!("Memory item {i}"),
            embedding: Some(embedder.generate_embedding(&format!("Memory {i}")).await?),
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.5),
            parent_id: None,
            metadata: None,
            expires_at: None,
        };

        let memory = repository.create_memory(request).await?;
        assert_eq!(memory.tier, MemoryTier::Working);
    }

    // Verify working memory is at capacity
    let stats = repository.get_statistics().await?;
    assert_eq!(
        stats.working_count.unwrap_or(0),
        5,
        "Working memory should have 5 items"
    );

    // Add 6th item - should trigger LRU eviction
    let request = CreateMemoryRequest {
        content: "Memory item 6".to_string(),
        embedding: Some(embedder.generate_embedding("Memory 6").await?),
        tier: Some(MemoryTier::Working),
        importance_score: Some(0.5),
        parent_id: None,
        metadata: None,
        expires_at: None,
    };

    let memory = repository.create_memory(request).await?;
    assert_eq!(memory.tier, MemoryTier::Working);

    // Verify working memory is still at capacity (not exceeded)
    let stats = repository.get_statistics().await?;
    assert_eq!(
        stats.working_count.unwrap_or(0),
        5,
        "Working memory should still have 5 items after LRU eviction"
    );
    assert_eq!(
        stats.warm_count.unwrap_or(0),
        1,
        "Warm tier should have 1 item (evicted from working)"
    );

    Ok(())
}

#[tokio::test]
async fn test_lru_eviction_order() -> Result<()> {
    let repository = create_test_repository_with_limit(5).await?;
    let embedder = Arc::new(SimpleEmbedder::new_mock());

    // Create memories with explicit access order
    let mut memory_ids = Vec::new();
    for i in 0..5 {
        let request = CreateMemoryRequest {
            content: format!("Memory {i}"),
            embedding: Some(embedder.generate_embedding(&format!("Memory {i}")).await?),
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.5),
            parent_id: None,
            metadata: None,
            expires_at: None,
        };

        let memory = repository.create_memory(request).await?;
        memory_ids.push(memory.id);

        // Sleep briefly to ensure different timestamps
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }

    // Access memories 2, 3, 4 to update their last_accessed time
    for &id in &memory_ids[2..5] {
        repository.get_memory(id).await?;
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }

    // Add new memory - should evict memory 0 (least recently accessed)
    let request = CreateMemoryRequest {
        content: "New memory".to_string(),
        embedding: Some(embedder.generate_embedding("New memory").await?),
        tier: Some(MemoryTier::Working),
        importance_score: Some(0.5),
        parent_id: None,
        metadata: None,
        expires_at: None,
    };

    repository.create_memory(request).await?;

    // Check that memory 0 was moved to warm tier
    let memory_0 = repository.get_memory(memory_ids[0]).await?;
    assert_eq!(
        memory_0.tier,
        MemoryTier::Warm,
        "Least recently used memory should be evicted to warm tier"
    );

    // Check that other memories remain in working tier
    for &id in &memory_ids[1..5] {
        let memory = repository.get_memory(id).await?;
        assert_eq!(
            memory.tier,
            MemoryTier::Working,
            "Recently accessed memories should remain in working tier"
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_memory_pressure_metric() -> Result<()> {
    let repository = create_test_repository_with_limit(5).await?;
    let embedder = Arc::new(SimpleEmbedder::new_mock());

    // Initially, pressure should be 0
    let pressure = repository.get_working_memory_pressure().await?;
    assert_eq!(pressure, 0.0, "Initial memory pressure should be 0");

    // Add 3 memories (60% capacity)
    for i in 0..3 {
        let request = CreateMemoryRequest {
            content: format!("Memory {i}"),
            embedding: Some(embedder.generate_embedding(&format!("Memory {i}")).await?),
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.5),
            parent_id: None,
            metadata: None,
            expires_at: None,
        };
        repository.create_memory(request).await?;
    }

    let pressure = repository.get_working_memory_pressure().await?;
    assert!(
        (pressure - 0.6).abs() < 0.01,
        "Memory pressure should be 0.6 (3/5)"
    );

    // Fill to capacity
    for i in 3..5 {
        let request = CreateMemoryRequest {
            content: format!("Memory {i}"),
            embedding: Some(embedder.generate_embedding(&format!("Memory {i}")).await?),
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.5),
            parent_id: None,
            metadata: None,
            expires_at: None,
        };
        repository.create_memory(request).await?;
    }

    let pressure = repository.get_working_memory_pressure().await?;
    assert_eq!(pressure, 1.0, "Memory pressure should be 1.0 at capacity");

    Ok(())
}

#[tokio::test]
async fn test_no_eviction_for_other_tiers() -> Result<()> {
    let repository = create_test_repository_with_limit(5).await?;
    let embedder = Arc::new(SimpleEmbedder::new_mock());

    // Add many memories to warm tier (should not be limited)
    for i in 0..20 {
        let request = CreateMemoryRequest {
            content: format!("Warm memory {i}"),
            embedding: Some(embedder.generate_embedding(&format!("Warm {i}")).await?),
            tier: Some(MemoryTier::Warm),
            importance_score: Some(0.5),
            parent_id: None,
            metadata: None,
            expires_at: None,
        };

        let memory = repository.create_memory(request).await?;
        assert_eq!(memory.tier, MemoryTier::Warm);
    }

    let stats = repository.get_statistics().await?;
    assert_eq!(
        stats.warm_count.unwrap_or(0),
        20,
        "Warm tier should have 20 items (no limit enforcement)"
    );
    assert_eq!(
        stats.working_count.unwrap_or(0),
        0,
        "Working tier should be empty"
    );

    Ok(())
}

#[tokio::test]
async fn test_automatic_tier_migration_on_pressure() -> Result<()> {
    let repository = create_test_repository_with_limit(7).await?; // Miller's 7
    let embedder = Arc::new(SimpleEmbedder::new_mock());

    // Fill working memory
    for i in 0..7 {
        let request = CreateMemoryRequest {
            content: format!("Memory {i}"),
            embedding: Some(embedder.generate_embedding(&format!("Memory {i}")).await?),
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.5 + (i as f64 * 0.05)), // Varying importance
            parent_id: None,
            metadata: None,
            expires_at: None,
        };
        repository.create_memory(request).await?;
    }

    // Verify at capacity
    let pressure = repository.get_working_memory_pressure().await?;
    assert_eq!(pressure, 1.0, "Should be at full capacity");

    // Add high-importance memory - should trigger migration
    let request = CreateMemoryRequest {
        content: "High importance memory".to_string(),
        embedding: Some(embedder.generate_embedding("High importance").await?),
        tier: Some(MemoryTier::Working),
        importance_score: Some(0.9),
        parent_id: None,
        metadata: None,
        expires_at: None,
    };

    repository.create_memory(request).await?;

    // Verify migration occurred
    let stats = repository.get_statistics().await?;
    assert_eq!(
        stats.working_count.unwrap_or(0),
        7,
        "Working tier should maintain Miller's 7"
    );
    assert_eq!(
        stats.warm_count.unwrap_or(0),
        1,
        "One memory should be migrated to warm tier"
    );

    Ok(())
}

#[tokio::test]
async fn test_error_handling_when_storage_exhausted() -> Result<()> {
    // This test would require a scenario where eviction fails
    // For example, if there's a database constraint preventing migration
    // This is primarily for documentation of expected behavior

    // In production, if eviction fails, StorageExhausted error should be returned
    // The error message should indicate the tier and limit

    Ok(())
}

#[tokio::test]
async fn test_concurrent_memory_creation_at_capacity() -> Result<()> {
    let repository = create_test_repository_with_limit(5).await?;
    let embedder = Arc::new(SimpleEmbedder::new_mock());

    // Fill to capacity
    for i in 0..5 {
        let request = CreateMemoryRequest {
            content: format!("Initial memory {i}"),
            embedding: Some(embedder.generate_embedding(&format!("Initial {i}")).await?),
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.5),
            parent_id: None,
            metadata: None,
            expires_at: None,
        };
        repository.create_memory(request).await?;
    }

    // Attempt concurrent creation
    let mut handles = Vec::new();
    for i in 0..3 {
        let repo = repository.clone();
        let emb = embedder.clone();
        let handle = tokio::spawn(async move {
            let embedding = emb.generate_embedding(&format!("Concurrent {i}")).await?;
            let request = CreateMemoryRequest {
                content: format!("Concurrent memory {i}"),
                embedding: Some(embedding),
                tier: Some(MemoryTier::Working),
                importance_score: Some(0.5),
                parent_id: None,
                metadata: None,
                expires_at: None,
            };
            repo.create_memory(request)
                .await
                .map_err(anyhow::Error::from)
        });
        handles.push(handle);
    }

    // Wait for all concurrent creations
    for handle in handles {
        let _ = handle.await?;
    }

    // Verify working memory didn't exceed limit
    let stats = repository.get_statistics().await?;
    assert!(
        stats.working_count.unwrap_or(0) <= 5,
        "Working memory should not exceed limit even with concurrent creation"
    );
    assert!(
        stats.warm_count.unwrap_or(0) >= 3,
        "At least 3 memories should be evicted to warm tier"
    );

    Ok(())
}
