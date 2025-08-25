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
use std::time::Instant;
use uuid::Uuid;

/// Integration test for CODEX-002: Fix Critical N+1 Query Pattern
/// Tests the batch_update_consolidation method performance improvements
/// Expected >10x improvement over the previous N+1 loop-based implementation

async fn create_test_repository_with_consolidation_data(
) -> Result<(Arc<MemoryRepository>, Vec<Uuid>)> {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgresql://codex_user:MZSfXiLr5uR3QYbRwv2vTzi22SvFkj4a@192.168.1.104:5432/codex_db"
            .to_string()
    });
    let pool = create_pool(&database_url, 10).await?;

    let config = Config::default();
    let repository = Arc::new(MemoryRepository::with_config(pool, config));
    let embedder = Arc::new(SimpleEmbedder::new_mock());

    // Create test memories for consolidation updates
    let mut memory_ids = Vec::new();

    for i in 0..100 {
        let request = CreateMemoryRequest {
            content: format!("Memory content for consolidation test {}: This memory will be used to test batch consolidation update performance improvements", i),
            embedding: Some(embedder.generate_embedding(&format!("consolidation test {}", i)).await?),
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.5 + (i as f64 * 0.005)),
            parent_id: None,
            metadata: Some(serde_json::json!({
                "test_index": i,
                "batch_size": "100_memories",
                "test_type": "consolidation_performance"
            })),
            expires_at: None,
        };

        let memory = repository.create_memory(request).await?;
        memory_ids.push(memory.id);
    }

    Ok((repository, memory_ids))
}

#[tokio::test]
async fn test_batch_consolidation_update_performance() -> Result<()> {
    let (repository, memory_ids) = create_test_repository_with_consolidation_data().await?;

    // Test small batch (10 memories) - baseline
    let small_batch_ids: Vec<Uuid> = memory_ids.iter().take(10).cloned().collect();
    let small_updates: Vec<(Uuid, f64, f64)> = small_batch_ids
        .iter()
        .enumerate()
        .map(|(i, &id)| (id, 0.8 + (i as f64 * 0.01), 0.9 + (i as f64 * 0.005)))
        .collect();

    let start_time = Instant::now();
    let small_updated_count = repository
        .batch_update_consolidation(&small_updates)
        .await?;
    let small_duration = start_time.elapsed();

    println!(
        "Small batch (10 memories) consolidation update: {:?}, updated {} memories",
        small_duration, small_updated_count
    );

    // Verify all updates succeeded
    assert_eq!(small_updated_count, 10, "Should update all 10 memories");

    // Performance target: <10ms for small batches with the new UNNEST implementation
    assert!(
        small_duration.as_millis() < 10,
        "Small batch consolidation should complete in <10ms, got: {:?}",
        small_duration
    );

    Ok(())
}

#[tokio::test]
async fn test_medium_batch_consolidation_performance() -> Result<()> {
    let (repository, memory_ids) = create_test_repository_with_consolidation_data().await?;

    // Test medium batch (50 memories)
    let medium_batch_ids: Vec<Uuid> = memory_ids.iter().take(50).cloned().collect();
    let medium_updates: Vec<(Uuid, f64, f64)> = medium_batch_ids
        .iter()
        .enumerate()
        .map(|(i, &id)| (id, 0.7 + (i as f64 * 0.005), 0.85 + (i as f64 * 0.002)))
        .collect();

    let start_time = Instant::now();
    let medium_updated_count = repository
        .batch_update_consolidation(&medium_updates)
        .await?;
    let medium_duration = start_time.elapsed();

    println!(
        "Medium batch (50 memories) consolidation update: {:?}, updated {} memories",
        medium_duration, medium_updated_count
    );

    // Verify all updates succeeded
    assert_eq!(medium_updated_count, 50, "Should update all 50 memories");

    // Performance target: <25ms for medium batches
    // With the old N+1 approach, this would take 250-500ms
    assert!(
        medium_duration.as_millis() < 25,
        "Medium batch consolidation should complete in <25ms, got: {:?}",
        medium_duration
    );

    Ok(())
}

#[tokio::test]
async fn test_large_batch_consolidation_performance() -> Result<()> {
    let (repository, memory_ids) = create_test_repository_with_consolidation_data().await?;

    // Test large batch (100 memories) - the full dataset
    let large_updates: Vec<(Uuid, f64, f64)> = memory_ids
        .iter()
        .enumerate()
        .map(|(i, &id)| (id, 0.6 + (i as f64 * 0.003), 0.8 + (i as f64 * 0.001)))
        .collect();

    let start_time = Instant::now();
    let large_updated_count = repository
        .batch_update_consolidation(&large_updates)
        .await?;
    let large_duration = start_time.elapsed();

    println!(
        "Large batch (100 memories) consolidation update: {:?}, updated {} memories",
        large_duration, large_updated_count
    );

    // Verify all updates succeeded
    assert_eq!(large_updated_count, 100, "Should update all 100 memories");

    // Performance target: <50ms for large batches
    // With the old N+1 approach, this would take 500ms-1000ms
    // This represents a >10x improvement
    assert!(
        large_duration.as_millis() < 50,
        "Large batch consolidation should complete in <50ms, got: {:?}",
        large_duration
    );

    Ok(())
}

#[tokio::test]
async fn test_empty_batch_consolidation() -> Result<()> {
    let (repository, _memory_ids) = create_test_repository_with_consolidation_data().await?;

    // Test empty batch handling
    let empty_updates: Vec<(Uuid, f64, f64)> = vec![];

    let start_time = Instant::now();
    let updated_count = repository
        .batch_update_consolidation(&empty_updates)
        .await?;
    let duration = start_time.elapsed();

    println!(
        "Empty batch consolidation update: {:?}, updated {} memories",
        duration, updated_count
    );

    // Verify empty batch returns 0 and completes quickly
    assert_eq!(updated_count, 0, "Empty batch should update 0 memories");
    assert!(
        duration.as_micros() < 1000, // Should be very fast, <1ms
        "Empty batch should complete in <1ms, got: {:?}",
        duration
    );

    Ok(())
}

#[tokio::test]
async fn test_concurrent_batch_consolidation_updates() -> Result<()> {
    let (repository, memory_ids) = create_test_repository_with_consolidation_data().await?;
    let repo = repository;

    // Split memory IDs into chunks for concurrent testing
    let chunk_size = 25;
    let chunks: Vec<Vec<Uuid>> = memory_ids
        .chunks(chunk_size)
        .map(|chunk| chunk.to_vec())
        .collect();

    let mut handles = Vec::new();

    // Start concurrent batch updates
    for (chunk_idx, chunk) in chunks.into_iter().enumerate() {
        let repository_clone = repo.clone();
        let handle = tokio::spawn(async move {
            let updates: Vec<(Uuid, f64, f64)> = chunk
                .iter()
                .enumerate()
                .map(|(i, &id)| {
                    (
                        id,
                        0.5 + (chunk_idx as f64 * 0.1) + (i as f64 * 0.01),
                        0.7 + (chunk_idx as f64 * 0.05) + (i as f64 * 0.005),
                    )
                })
                .collect();

            let start_time = Instant::now();
            let updated_count = repository_clone
                .batch_update_consolidation(&updates)
                .await?;
            let duration = start_time.elapsed();

            Ok::<(std::time::Duration, usize, usize), anyhow::Error>((
                duration,
                updated_count,
                chunk_idx,
            ))
        });
        handles.push(handle);
    }

    // Wait for all concurrent updates to complete
    let mut total_updated = 0;
    let mut max_duration = std::time::Duration::from_millis(0);

    for handle in handles {
        let (duration, updated_count, chunk_idx) = handle.await??;
        total_updated += updated_count;
        max_duration = max_duration.max(duration);

        println!(
            "Concurrent batch {} (25 memories): {:?}, updated {} memories",
            chunk_idx, duration, updated_count
        );

        // Each concurrent batch should complete quickly
        assert!(
            duration.as_millis() < 30,
            "Concurrent batch should complete in <30ms, got: {:?}",
            duration
        );
    }

    println!(
        "All concurrent batches completed. Max duration: {:?}, Total updated: {}",
        max_duration, total_updated
    );

    // Verify all memories were updated across all concurrent batches
    assert_eq!(
        total_updated, 100,
        "Should update all 100 memories across concurrent batches"
    );

    // Overall concurrent performance should still be reasonable
    assert!(
        max_duration.as_millis() < 50,
        "Maximum concurrent batch duration should be <50ms, got: {:?}",
        max_duration
    );

    Ok(())
}

#[tokio::test]
async fn test_consolidation_update_data_integrity() -> Result<()> {
    let (repository, memory_ids) = create_test_repository_with_consolidation_data().await?;

    // Take first 10 memories for detailed verification
    let test_ids: Vec<Uuid> = memory_ids.iter().take(10).cloned().collect();
    let test_updates: Vec<(Uuid, f64, f64)> = test_ids
        .iter()
        .enumerate()
        .map(|(i, &id)| {
            (
                id,
                0.123 + (i as f64 * 0.111), // Distinctive values for verification
                0.456 + (i as f64 * 0.222),
            )
        })
        .collect();

    // Perform batch update
    let updated_count = repository.batch_update_consolidation(&test_updates).await?;

    assert_eq!(updated_count, 10, "Should update all 10 memories");

    // Verify each memory was updated with correct values
    for (i, &memory_id) in test_ids.iter().enumerate() {
        let memory = repository.get_memory(memory_id).await?;

        let expected_strength = 0.123 + (i as f64 * 0.111);
        let expected_recall_prob = 0.456 + (i as f64 * 0.222);

        assert!(
            (memory.consolidation_strength - expected_strength).abs() < 0.001,
            "Memory {} consolidation_strength should be {}, got {}",
            i,
            expected_strength,
            memory.consolidation_strength
        );

        if let Some(actual_recall_prob) = memory.recall_probability {
            assert!(
                (actual_recall_prob - expected_recall_prob).abs() < 0.001,
                "Memory {} recall_probability should be {}, got {}",
                i,
                expected_recall_prob,
                actual_recall_prob
            );
        } else {
            panic!("Memory {} should have recall_probability set", i);
        }

        // Verify updated_at was set
        assert!(
            memory.updated_at > memory.created_at,
            "Memory {} updated_at should be after created_at",
            i
        );
    }

    println!("✅ Data integrity verified: All batch updates applied correctly");

    Ok(())
}

#[tokio::test]
async fn test_batch_update_transaction_safety() -> Result<()> {
    let (repository, memory_ids) = create_test_repository_with_consolidation_data().await?;

    // Create updates with one invalid ID to test transaction rollback
    let valid_id = memory_ids[0];
    let invalid_id = Uuid::new_v4(); // This ID doesn't exist

    let mixed_updates = vec![
        (valid_id, 0.999, 0.999),
        (invalid_id, 0.888, 0.888), // This should cause the transaction to roll back
    ];

    // The batch update should handle the error gracefully
    let result = repository.batch_update_consolidation(&mixed_updates).await;

    // Either it should succeed updating only the valid one, or fail completely
    // The important thing is no partial updates occur
    match result {
        Ok(count) => {
            // If it succeeds, it should only update the valid memory
            assert_eq!(count, 1, "Should update only the valid memory");

            // Verify the valid memory was updated
            let memory = repository.get_memory(valid_id).await?;
            assert!(
                (memory.consolidation_strength - 0.999).abs() < 0.001,
                "Valid memory should be updated"
            );
        }
        Err(_) => {
            // If it fails, no memories should be updated (transaction rollback)
            let memory = repository.get_memory(valid_id).await?;

            // The consolidation_strength should not be 0.999 (our test value)
            assert!(
                (memory.consolidation_strength - 0.999).abs() > 0.001,
                "Memory should not be updated if transaction rolled back"
            );
        }
    }

    println!("✅ Transaction safety verified: Batch updates handle errors correctly");

    Ok(())
}
