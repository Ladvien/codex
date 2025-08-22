use anyhow::Result;
use chrono::{Duration, Utc};
use codex_memory::{
    config::TierManagerConfig,
    memory::{
        connection::create_pool,
        models::{CreateMemoryRequest, Memory, MemoryStatus, MemoryTier},
        repository::MemoryRepository,
        tier_manager::{TierManager, TierMigrationCandidate},
    },
    Config,
};
use std::sync::Arc;
use tokio::time::{sleep, Duration as TokioDuration};
use uuid::Uuid;

// Test helper function to create a test database pool
async fn create_test_pool() -> Result<sqlx::PgPool> {
    let database_url = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
        "postgresql://postgres:postgres@localhost:5432/codex_memory_test".to_string()
    });

    create_pool(&database_url, 5).await
}

// Test helper to create a memory with specific properties
async fn create_test_memory(
    repository: &MemoryRepository,
    content: &str,
    tier: MemoryTier,
    importance_score: f64,
    consolidation_strength: f64,
    decay_rate: f64,
    hours_old: i64,
) -> Result<Memory> {
    let created_at = Utc::now() - Duration::hours(hours_old);

    let request = CreateMemoryRequest {
        content: content.to_string(),
        embedding: None,
        tier: Some(tier),
        importance_score: Some(importance_score),
        metadata: None,
        parent_id: None,
        expires_at: None,
    };

    let mut memory = repository.create_memory(request).await?;

    // Update the memory with custom consolidation parameters and created_at time
    sqlx::query!(
        r#"
        UPDATE memories 
        SET consolidation_strength = $1, 
            decay_rate = $2, 
            created_at = $3,
            updated_at = $3
        WHERE id = $4
        "#,
        consolidation_strength,
        decay_rate,
        created_at,
        memory.id
    )
    .execute(repository.pool())
    .await?;

    // Reload memory to get updated values
    memory = repository.get_memory(memory.id).await?;
    Ok(memory)
}

#[tokio::test]
async fn test_tier_manager_creation() -> Result<()> {
    let pool = create_test_pool().await?;
    let repository = Arc::new(MemoryRepository::new(pool));
    let config = TierManagerConfig::default();

    let tier_manager = TierManager::new(repository, config)?;

    // Verify metrics are initialized
    let metrics = tier_manager.get_metrics().await?;
    assert_eq!(metrics.total_migrations_completed, 0);
    assert_eq!(metrics.total_migrations_failed, 0);
    assert!(!metrics.is_running);

    Ok(())
}

#[tokio::test]
async fn test_tier_manager_start_stop() -> Result<()> {
    let pool = create_test_pool().await?;
    let repository = Arc::new(MemoryRepository::new(pool));
    let config = TierManagerConfig {
        enabled: true,
        scan_interval_seconds: 1, // Fast for testing
        ..TierManagerConfig::default()
    };

    let tier_manager = TierManager::new(repository, config)?;

    // Start the tier manager
    tier_manager.start().await?;

    // Give it a moment to start
    sleep(TokioDuration::from_millis(100)).await;

    let metrics = tier_manager.get_metrics().await?;
    assert!(metrics.is_running);

    // Stop the tier manager
    tier_manager.stop().await;

    // Give it a moment to stop
    sleep(TokioDuration::from_millis(100)).await;

    let metrics = tier_manager.get_metrics().await?;
    assert!(!metrics.is_running);

    Ok(())
}

#[tokio::test]
async fn test_working_to_warm_migration() -> Result<()> {
    let pool = create_test_pool().await?;
    let repository = Arc::new(MemoryRepository::new(pool));

    // Create a memory in working tier with low recall probability
    let memory = create_test_memory(
        &repository,
        "Test memory for working to warm migration",
        MemoryTier::Working,
        0.3, // Low importance
        1.0, // Low consolidation strength
        2.0, // High decay rate
        25,  // 25 hours old (meets minimum age requirement)
    )
    .await?;

    let config = TierManagerConfig {
        enabled: true,
        working_to_warm_threshold: 0.7, // Should trigger migration
        min_working_age_hours: 1,
        ..TierManagerConfig::default()
    };

    let tier_manager = TierManager::new(repository.clone(), config)?;

    // Force a scan to trigger migration
    let result = tier_manager.force_scan().await?;

    // Verify migration occurred
    assert!(result.successful_migrations.len() > 0);
    assert!(result.successful_migrations.contains(&memory.id));

    // Verify memory is now in warm tier
    let updated_memory = repository.get_memory(memory.id).await?;
    assert_eq!(updated_memory.tier, MemoryTier::Warm);

    Ok(())
}

#[tokio::test]
async fn test_warm_to_cold_migration() -> Result<()> {
    let pool = create_test_pool().await?;
    let repository = Arc::new(MemoryRepository::new(pool));

    // Create a memory in warm tier with very low recall probability
    let memory = create_test_memory(
        &repository,
        "Test memory for warm to cold migration",
        MemoryTier::Warm,
        0.2, // Very low importance
        0.5, // Low consolidation strength
        3.0, // High decay rate
        168, // 1 week old (meets minimum age requirement for warm)
    )
    .await?;

    let config = TierManagerConfig {
        enabled: true,
        warm_to_cold_threshold: 0.5, // Should trigger migration
        min_warm_age_hours: 24,
        ..TierManagerConfig::default()
    };

    let tier_manager = TierManager::new(repository.clone(), config)?;

    // Force a scan to trigger migration
    let result = tier_manager.force_scan().await?;

    // Verify migration occurred
    assert!(result.successful_migrations.len() > 0);
    assert!(result.successful_migrations.contains(&memory.id));

    // Verify memory is now in cold tier
    let updated_memory = repository.get_memory(memory.id).await?;
    assert_eq!(updated_memory.tier, MemoryTier::Cold);

    Ok(())
}

#[tokio::test]
async fn test_cold_to_frozen_migration() -> Result<()> {
    let pool = create_test_pool().await?;
    let repository = Arc::new(MemoryRepository::new(pool));

    // Create a memory in cold tier with extremely low recall probability
    let memory = create_test_memory(
        &repository,
        "Test memory for cold to frozen migration",
        MemoryTier::Cold,
        0.1, // Extremely low importance
        0.1, // Very low consolidation strength
        5.0, // Very high decay rate
        720, // 30 days old (meets minimum age requirement for cold)
    )
    .await?;

    let config = TierManagerConfig {
        enabled: true,
        cold_to_frozen_threshold: 0.2, // Should trigger migration
        min_cold_age_hours: 168,       // 1 week
        ..TierManagerConfig::default()
    };

    let tier_manager = TierManager::new(repository.clone(), config)?;

    // Force a scan to trigger migration
    let result = tier_manager.force_scan().await?;

    // Verify migration occurred
    assert!(result.successful_migrations.len() > 0);
    assert!(result.successful_migrations.contains(&memory.id));

    // Verify memory is now in frozen tier
    let updated_memory = repository.get_memory(memory.id).await?;
    assert_eq!(updated_memory.tier, MemoryTier::Frozen);

    Ok(())
}

#[tokio::test]
async fn test_minimum_age_protection() -> Result<()> {
    let pool = create_test_pool().await?;
    let repository = Arc::new(MemoryRepository::new(pool));

    // Create a memory that's too young to migrate despite low importance
    let memory = create_test_memory(
        &repository,
        "Test memory that's too young to migrate",
        MemoryTier::Working,
        0.1, // Very low importance (should trigger migration if old enough)
        0.5, // Low consolidation strength
        3.0, // High decay rate
        0,   // Just created (doesn't meet minimum age)
    )
    .await?;

    let config = TierManagerConfig {
        enabled: true,
        working_to_warm_threshold: 0.7,
        min_working_age_hours: 24, // Requires 24 hours minimum
        ..TierManagerConfig::default()
    };

    let tier_manager = TierManager::new(repository.clone(), config)?;

    // Force a scan
    let result = tier_manager.force_scan().await?;

    // Verify no migration occurred due to age protection
    assert!(!result.successful_migrations.contains(&memory.id));

    // Verify memory is still in working tier
    let updated_memory = repository.get_memory(memory.id).await?;
    assert_eq!(updated_memory.tier, MemoryTier::Working);

    Ok(())
}

#[tokio::test]
async fn test_high_importance_protection() -> Result<()> {
    let pool = create_test_pool().await?;
    let repository = Arc::new(MemoryRepository::new(pool));

    // Create an old memory with high importance that should not migrate
    let memory = create_test_memory(
        &repository,
        "Test memory with high importance",
        MemoryTier::Working,
        0.9, // Very high importance
        5.0, // High consolidation strength
        0.5, // Low decay rate
        48,  // 48 hours old (meets age requirement)
    )
    .await?;

    let config = TierManagerConfig {
        enabled: true,
        working_to_warm_threshold: 0.7,
        min_working_age_hours: 1,
        ..TierManagerConfig::default()
    };

    let tier_manager = TierManager::new(repository.clone(), config)?;

    // Force a scan
    let result = tier_manager.force_scan().await?;

    // Verify no migration occurred due to high importance/recall probability
    assert!(!result.successful_migrations.contains(&memory.id));

    // Verify memory is still in working tier
    let updated_memory = repository.get_memory(memory.id).await?;
    assert_eq!(updated_memory.tier, MemoryTier::Working);

    Ok(())
}

#[tokio::test]
async fn test_batch_migration_performance() -> Result<()> {
    let pool = create_test_pool().await?;
    let repository = Arc::new(MemoryRepository::new(pool));

    // Create multiple memories that should be migrated
    let mut memory_ids = Vec::new();
    for i in 0..50 {
        let memory = create_test_memory(
            &repository,
            &format!("Test memory for batch migration {}", i),
            MemoryTier::Working,
            0.3, // Low importance
            1.0, // Low consolidation strength
            2.0, // High decay rate
            25,  // Old enough to migrate
        )
        .await?;
        memory_ids.push(memory.id);
    }

    let config = TierManagerConfig {
        enabled: true,
        working_to_warm_threshold: 0.7,
        min_working_age_hours: 1,
        migration_batch_size: 10, // Process in batches of 10
        max_concurrent_migrations: 2,
        ..TierManagerConfig::default()
    };

    let tier_manager = TierManager::new(repository.clone(), config)?;

    // Measure migration performance
    let start_time = std::time::Instant::now();
    let result = tier_manager.force_scan().await?;
    let duration = start_time.elapsed();

    // Verify most memories were migrated
    assert!(result.successful_migrations.len() >= 40); // Allow for some variation

    // Verify performance meets target (should be much faster than 1000/sec limit)
    let migrations_per_second = result.successful_migrations.len() as f64 / duration.as_secs_f64();
    println!(
        "Migration performance: {:.2} migrations/second",
        migrations_per_second
    );

    // Performance should be reasonable (we're not testing the exact 1000/sec here due to test environment)
    assert!(migrations_per_second > 10.0);

    Ok(())
}

#[tokio::test]
async fn test_migration_history_logging() -> Result<()> {
    let pool = create_test_pool().await?;
    let repository = Arc::new(MemoryRepository::new(pool.clone()));

    let memory = create_test_memory(
        &repository,
        "Test memory for migration logging",
        MemoryTier::Working,
        0.3,
        1.0,
        2.0,
        25,
    )
    .await?;

    let config = TierManagerConfig {
        enabled: true,
        working_to_warm_threshold: 0.7,
        min_working_age_hours: 1,
        log_migrations: true, // Enable migration logging
        ..TierManagerConfig::default()
    };

    let tier_manager = TierManager::new(repository.clone(), config)?;

    // Get initial log count
    let initial_log_count: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM memory_consolidation_log WHERE memory_id = $1",
        memory.id
    )
    .fetch_one(&pool)
    .await?
    .unwrap_or(0);

    // Force migration
    let result = tier_manager.force_scan().await?;
    assert!(result.successful_migrations.contains(&memory.id));

    // Check that migration was logged
    let final_log_count: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM memory_consolidation_log WHERE memory_id = $1 AND consolidation_event LIKE 'tier_migration_%'",
        memory.id
    )
    .fetch_one(&pool)
    .await?
    .unwrap_or(0);

    assert!(final_log_count > initial_log_count);

    // Verify log entry contains correct information
    let log_entry = sqlx::query!(
        r#"
        SELECT consolidation_event, trigger_reason
        FROM memory_consolidation_log 
        WHERE memory_id = $1 AND consolidation_event LIKE 'tier_migration_%'
        ORDER BY created_at DESC
        LIMIT 1
        "#,
        memory.id
    )
    .fetch_one(&pool)
    .await?;

    assert!(log_entry
        .consolidation_event
        .contains("tier_migration_working_warm"));

    // Verify trigger_reason contains migration details
    if let Some(reason) = log_entry.trigger_reason {
        assert!(reason.contains("Priority score"));
    }

    Ok(())
}

#[tokio::test]
async fn test_metrics_collection() -> Result<()> {
    let pool = create_test_pool().await?;
    let repository = Arc::new(MemoryRepository::new(pool));

    // Create memories in different tiers
    let _working_memory = create_test_memory(
        &repository,
        "Working tier memory",
        MemoryTier::Working,
        0.8,
        3.0,
        1.0,
        2,
    )
    .await?;

    let _warm_memory = create_test_memory(
        &repository,
        "Warm tier memory",
        MemoryTier::Warm,
        0.6,
        2.0,
        1.5,
        48,
    )
    .await?;

    let _cold_memory = create_test_memory(
        &repository,
        "Cold tier memory",
        MemoryTier::Cold,
        0.3,
        1.0,
        2.0,
        720,
    )
    .await?;

    let config = TierManagerConfig {
        enabled: true,
        enable_metrics: true,
        ..TierManagerConfig::default()
    };

    let tier_manager = TierManager::new(repository, config)?;

    // Get metrics
    let metrics = tier_manager.get_metrics().await?;

    // Verify tier counts are populated
    assert!(
        metrics
            .memories_by_tier
            .get(&MemoryTier::Working)
            .unwrap_or(&0)
            > &0
    );
    assert!(
        metrics
            .memories_by_tier
            .get(&MemoryTier::Warm)
            .unwrap_or(&0)
            > &0
    );
    assert!(
        metrics
            .memories_by_tier
            .get(&MemoryTier::Cold)
            .unwrap_or(&0)
            > &0
    );

    // Verify recall probability averages are reasonable
    if let Some(working_avg) = metrics
        .average_recall_probability_by_tier
        .get(&MemoryTier::Working)
    {
        assert!(*working_avg >= 0.0 && *working_avg <= 1.0);
    }

    if let Some(warm_avg) = metrics
        .average_recall_probability_by_tier
        .get(&MemoryTier::Warm)
    {
        assert!(*warm_avg >= 0.0 && *warm_avg <= 1.0);
    }

    if let Some(cold_avg) = metrics
        .average_recall_probability_by_tier
        .get(&MemoryTier::Cold)
    {
        assert!(*cold_avg >= 0.0 && *cold_avg <= 1.0);
    }

    Ok(())
}

#[tokio::test]
async fn test_disabled_tier_manager() -> Result<()> {
    let pool = create_test_pool().await?;
    let repository = Arc::new(MemoryRepository::new(pool));

    let config = TierManagerConfig {
        enabled: false, // Disabled
        ..TierManagerConfig::default()
    };

    let tier_manager = TierManager::new(repository, config)?;

    // Starting a disabled tier manager should succeed but not actually start
    tier_manager.start().await?;

    let metrics = tier_manager.get_metrics().await?;
    assert!(!metrics.is_running);

    Ok(())
}

#[tokio::test]
async fn test_concurrent_migration_limit() -> Result<()> {
    let pool = create_test_pool().await?;
    let repository = Arc::new(MemoryRepository::new(pool));

    // Create many memories to ensure we hit concurrency limits
    for i in 0..100 {
        create_test_memory(
            &repository,
            &format!("Concurrent test memory {}", i),
            MemoryTier::Working,
            0.3,
            1.0,
            2.0,
            25,
        )
        .await?;
    }

    let config = TierManagerConfig {
        enabled: true,
        working_to_warm_threshold: 0.7,
        min_working_age_hours: 1,
        migration_batch_size: 20,
        max_concurrent_migrations: 2, // Limit concurrency
        ..TierManagerConfig::default()
    };

    let tier_manager = TierManager::new(repository, config)?;

    // This should succeed even with concurrency limits
    let result = tier_manager.force_scan().await?;

    // Verify that migrations occurred (exact count may vary due to batching)
    assert!(result.successful_migrations.len() > 0);
    assert!(result.duration_ms > 0);

    Ok(())
}
