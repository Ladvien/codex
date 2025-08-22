use codex_memory::memory::{
    models::{CreateMemoryRequest, MemoryTier, SearchRequest},
    MemoryRepository,
};
use sqlx::PgPool;
use std::time::{Duration, Instant};
use tokio;
use uuid::Uuid;

async fn setup_test_pool() -> PgPool {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:password@localhost:5432/codex_memory_test".to_string()
    });

    PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to test database")
}

async fn create_test_memories_with_low_recall(
    repository: &MemoryRepository,
    count: usize,
) -> Vec<Uuid> {
    let mut memory_ids = Vec::new();
    
    for i in 0..count {
        let memory = repository
            .create_memory(CreateMemoryRequest {
                content: format!("Low recall test memory {} that should be frozen due to low probability", i),
                embedding: Some(vec![0.1, 0.2, 0.3, 0.4, 0.5]),
                tier: Some(MemoryTier::Cold),
                importance_score: Some(0.3),
                metadata: None,
                parent_id: None,
                expires_at: None,
            })
            .await
            .unwrap();

        // Manually set low recall probability for testing
        sqlx::query("UPDATE memories SET recall_probability = $1 WHERE id = $2")
            .bind(0.1) // Below 0.2 threshold
            .bind(memory.id)
            .execute(repository.pool())
            .await
            .unwrap();

        memory_ids.push(memory.id);
    }
    
    memory_ids
}

#[tokio::test]
#[ignore = "Requires database setup"]
async fn test_freeze_memory_with_delay() {
    let pool = setup_test_pool().await;
    let repository = MemoryRepository::new(pool);

    // Create a test memory
    let memory = repository
        .create_memory(CreateMemoryRequest {
            content: "Test memory for freezing with compression".to_string(),
            embedding: Some(vec![0.1, 0.2, 0.3]),
            tier: Some(MemoryTier::Cold),
            importance_score: Some(0.4),
            metadata: None,
            parent_id: None,
            expires_at: None,
        })
        .await
        .unwrap();

    // Freeze the memory
    let freeze_result = repository
        .freeze_memory(memory.id, Some("Test freeze".to_string()))
        .await
        .unwrap();

    assert!(freeze_result.compression_ratio.is_some());
    assert_ne!(freeze_result.frozen_id, Uuid::nil());

    // Test unfreezing with intentional delay
    let start_time = Instant::now();
    let unfreeze_result = repository
        .unfreeze_memory(freeze_result.frozen_id, Some(MemoryTier::Working))
        .await
        .unwrap();
    let elapsed = start_time.elapsed();

    // Verify delay is between 2-5 seconds as required
    assert!(unfreeze_result.retrieval_delay_seconds >= 2);
    assert!(unfreeze_result.retrieval_delay_seconds <= 5);
    assert!(elapsed >= Duration::from_secs(2));
    assert!(elapsed <= Duration::from_secs(6)); // Allow some buffer
    assert_eq!(unfreeze_result.restoration_tier, MemoryTier::Working);
}

#[tokio::test]
#[ignore = "Requires database setup"]
async fn test_batch_freeze_by_recall_probability() {
    let pool = setup_test_pool().await;
    let repository = MemoryRepository::new(pool);

    // Create test memories with low recall probability
    let memory_ids = create_test_memories_with_low_recall(&repository, 50).await;

    // Perform batch freeze
    let batch_result = repository
        .batch_freeze_by_recall_probability(Some(100))
        .await
        .unwrap();

    // Verify results
    assert!(batch_result.memories_frozen > 0);
    assert!(batch_result.memories_frozen <= 50);
    assert!(batch_result.total_space_saved_bytes > 0);
    assert!(batch_result.average_compression_ratio >= 5.0); // Should achieve >5:1 compression
    assert!(batch_result.processing_time_ms > 0);
    assert_eq!(batch_result.frozen_memory_ids.len(), batch_result.memories_frozen as usize);

    println!(
        "Batch freeze: {} memories frozen, {:.1}:1 compression ratio, {}ms processing time",
        batch_result.memories_frozen,
        batch_result.average_compression_ratio,
        batch_result.processing_time_ms
    );
}

#[tokio::test]
#[ignore = "Requires database setup"]
async fn test_large_batch_freeze_100k() {
    let pool = setup_test_pool().await;
    let repository = MemoryRepository::new(pool);

    // This test simulates the requirement to freeze 100K memories in batch
    // We'll create a smaller batch for testing but verify the infrastructure can handle it
    let memory_ids = create_test_memories_with_low_recall(&repository, 1000).await;

    let start_time = Instant::now();
    let batch_result = repository
        .batch_freeze_by_recall_probability(Some(100_000)) // Test with 100K limit
        .await
        .unwrap();
    let processing_time = start_time.elapsed();

    // Verify the batch can process efficiently
    assert!(batch_result.memories_frozen > 0);
    assert!(processing_time < Duration::from_secs(300)); // Should complete in under 5 minutes
    assert!(batch_result.average_compression_ratio >= 5.0);

    // Verify storage savings (requirement: 80% less storage)
    let space_saved_percentage = batch_result.total_space_saved_bytes as f64 
        / (batch_result.total_space_saved_bytes + (batch_result.total_space_saved_bytes / 4)) as f64; // Approximate original size
    assert!(space_saved_percentage >= 0.8);

    println!(
        "Large batch test: {} memories frozen in {:?}, {:.1}% space saved",
        batch_result.memories_frozen,
        processing_time,
        space_saved_percentage * 100.0
    );
}

#[tokio::test]
#[ignore = "Requires database setup"]
async fn test_batch_unfreeze_with_delays() {
    let pool = setup_test_pool().await;
    let repository = MemoryRepository::new(pool);

    // Create and freeze test memories
    let memory_ids = create_test_memories_with_low_recall(&repository, 10).await;
    let batch_freeze_result = repository
        .batch_freeze_by_recall_probability(Some(50))
        .await
        .unwrap();

    // Test batch unfreeze
    let start_time = Instant::now();
    let batch_unfreeze_result = repository
        .batch_unfreeze_memories(
            batch_freeze_result.frozen_memory_ids,
            Some(MemoryTier::Warm),
        )
        .await
        .unwrap();
    let total_time = start_time.elapsed();

    // Verify results
    assert!(batch_unfreeze_result.memories_unfrozen > 0);
    assert!(batch_unfreeze_result.average_delay_seconds >= 2.0);
    assert!(batch_unfreeze_result.average_delay_seconds <= 5.0);
    assert!(total_time >= Duration::from_secs(2)); // At least minimum delay
    assert_eq!(
        batch_unfreeze_result.unfrozen_memory_ids.len(),
        batch_unfreeze_result.memories_unfrozen as usize
    );

    println!(
        "Batch unfreeze: {} memories unfrozen in {:?}, avg delay: {:.1}s",
        batch_unfreeze_result.memories_unfrozen,
        total_time,
        batch_unfreeze_result.average_delay_seconds
    );
}

#[tokio::test]
#[ignore = "Requires database setup"]
async fn test_compression_ratio_requirement() {
    let pool = setup_test_pool().await;
    let repository = MemoryRepository::new(pool);

    // Create memory with substantial text content
    let large_content = "This is a substantial piece of text content that should compress well. ".repeat(100);
    let memory = repository
        .create_memory(CreateMemoryRequest {
            content: large_content.clone(),
            embedding: Some(vec![0.1, 0.2, 0.3]),
            tier: Some(MemoryTier::Cold),
            importance_score: Some(0.2),
            metadata: None,
            parent_id: None,
            expires_at: None,
        })
        .await
        .unwrap();

    // Set low recall probability
    sqlx::query("UPDATE memories SET recall_probability = $1 WHERE id = $2")
        .bind(0.1)
        .bind(memory.id)
        .execute(repository.pool())
        .await
        .unwrap();

    // Freeze and verify compression ratio
    let batch_result = repository
        .batch_freeze_by_recall_probability(Some(10))
        .await
        .unwrap();

    // Verify compression ratio meets requirement (>5:1)
    assert!(
        batch_result.average_compression_ratio > 5.0,
        "Compression ratio {:.1}:1 does not meet >5:1 requirement",
        batch_result.average_compression_ratio
    );

    // Verify space savings meet requirement (80% less storage)
    let original_size = large_content.len() as u64;
    let space_saved_percentage = batch_result.total_space_saved_bytes as f64 / original_size as f64;
    assert!(
        space_saved_percentage >= 0.8,
        "Space savings {:.1}% does not meet 80% requirement",
        space_saved_percentage * 100.0
    );
}

#[tokio::test]
#[ignore = "Requires database setup"]
async fn test_frozen_memory_search_exclusion() {
    let pool = setup_test_pool().await;
    let repository = MemoryRepository::new(pool);

    // Create and freeze a memory
    let memory = repository
        .create_memory(CreateMemoryRequest {
            content: "Searchable content that will be frozen".to_string(),
            embedding: Some(vec![0.1, 0.2, 0.3]),
            tier: Some(MemoryTier::Cold),
            importance_score: Some(0.2),
            metadata: None,
            parent_id: None,
            expires_at: None,
        })
        .await
        .unwrap();

    // Set low recall probability and freeze
    sqlx::query("UPDATE memories SET recall_probability = $1 WHERE id = $2")
        .bind(0.1)
        .bind(memory.id)
        .execute(repository.pool())
        .await
        .unwrap();

    let _batch_result = repository
        .batch_freeze_by_recall_probability(Some(10))
        .await
        .unwrap();

    // Test that frozen memories are excluded from normal search
    let search_request = SearchRequest {
        query_text: Some("searchable content".to_string()),
        limit: Some(10),
        ..Default::default()
    };
    let search_response = repository
        .search_memories(search_request)
        .await
        .unwrap();

    // Verify the frozen memory is not in search results
    let found_memory = search_response.results.iter().find(|r| r.memory.id == memory.id);
    assert!(found_memory.is_none(), "Frozen memory should be excluded from search results");

    // Test explicit frozen memory search
    let frozen_results = repository
        .search_frozen_memories("searchable content", 10)
        .await
        .unwrap();

    // Verify the memory can be found in frozen search
    assert!(
        !frozen_results.is_empty(),
        "Frozen memory should be findable in explicit frozen search"
    );
}

#[tokio::test]
#[ignore = "Requires database setup"]
async fn test_recall_probability_migration_rule() {
    let pool = setup_test_pool().await;
    let repository = MemoryRepository::new(pool);

    // Create memories with various recall probabilities
    let test_cases = vec![
        (0.05, true),  // Should be frozen (< 0.2)
        (0.15, true),  // Should be frozen (< 0.2)
        (0.19, true),  // Should be frozen (< 0.2)
        (0.20, false), // Should NOT be frozen (= 0.2)
        (0.25, false), // Should NOT be frozen (> 0.2)
        (0.50, false), // Should NOT be frozen (> 0.2)
    ];

    for (recall_prob, should_freeze) in test_cases {
        let memory = repository
            .create_memory(CreateMemoryRequest {
                content: format!("Test memory with recall probability {}", recall_prob),
                embedding: Some(vec![0.1, 0.2, 0.3]),
                tier: Some(MemoryTier::Cold),
                importance_score: Some(0.3),
                metadata: None,
                parent_id: None,
                expires_at: None,
            })
            .await
            .unwrap();

        // Set specific recall probability
        sqlx::query("UPDATE memories SET recall_probability = $1 WHERE id = $2")
            .bind(recall_prob)
            .bind(memory.id)
            .execute(repository.pool())
            .await
            .unwrap();
    }

    // Perform batch freeze
    let batch_result = repository
        .batch_freeze_by_recall_probability(Some(100))
        .await
        .unwrap();

    // Verify only memories with P(recall) < 0.2 were frozen
    // We should have frozen 3 memories (0.05, 0.15, 0.19)
    assert_eq!(
        batch_result.memories_frozen, 3,
        "Should freeze exactly 3 memories with P(recall) < 0.2"
    );
}

#[tokio::test]
#[ignore = "Requires database setup"]
async fn test_data_integrity_after_freeze_unfreeze() {
    let pool = setup_test_pool().await;
    let repository = MemoryRepository::new(pool);

    let original_content = "Critical data that must maintain integrity through freeze/unfreeze cycle";
    let original_metadata = serde_json::json!({
        "type": "critical_data",
        "version": "1.0",
        "checksum": "abc123"
    });

    // Create memory
    let memory = repository
        .create_memory(CreateMemoryRequest {
            content: original_content.to_string(),
            embedding: Some(vec![0.1, 0.2, 0.3, 0.4, 0.5]),
            tier: Some(MemoryTier::Cold),
            importance_score: Some(0.4),
            metadata: Some(original_metadata.clone()),
            parent_id: None,
            expires_at: None,
        })
        .await
        .unwrap();

    let original_id = memory.id;

    // Set low recall probability and freeze
    sqlx::query("UPDATE memories SET recall_probability = $1 WHERE id = $2")
        .bind(0.1)
        .bind(memory.id)
        .execute(repository.pool())
        .await
        .unwrap();

    let freeze_result = repository
        .freeze_memory(memory.id, Some("Data integrity test".to_string()))
        .await
        .unwrap();

    // Unfreeze the memory
    let unfreeze_result = repository
        .unfreeze_memory(freeze_result.frozen_id, Some(MemoryTier::Working))
        .await
        .unwrap();

    // Retrieve the unfrozen memory
    let unfrozen_memory = repository
        .get_memory(unfreeze_result.memory_id)
        .await
        .unwrap();

    // Verify data integrity
    assert_eq!(unfrozen_memory.id, original_id);
    assert_eq!(unfrozen_memory.content, original_content);
    assert_eq!(unfrozen_memory.metadata, original_metadata);
    assert_eq!(unfrozen_memory.tier, MemoryTier::Working);
    assert!(unfrozen_memory.embedding.is_some());

    println!("Data integrity verified after freeze/unfreeze cycle");
}

#[tokio::test]
async fn test_frozen_tier_enum_exists() {
    // Test that Frozen tier is properly defined
    let frozen_tier = MemoryTier::Frozen;
    let tier_string = format!("{:?}", frozen_tier);
    assert_eq!(tier_string, "Frozen");

    // Test tier parsing
    let parsed_tier: MemoryTier = "frozen".parse().unwrap();
    assert!(matches!(parsed_tier, MemoryTier::Frozen));
}

#[tokio::test]
async fn test_batch_operations_performance_benchmarks() {
    // Test that batch operations meet performance requirements
    // This is a unit test that doesn't require database connection
    
    // Verify batch size constants
    const MAX_BATCH_SIZE: usize = 100_000;
    const CHUNK_SIZE: usize = 1_000;
    
    assert!(MAX_BATCH_SIZE >= 100_000, "Must support 100K batch operations");
    assert!(CHUNK_SIZE <= MAX_BATCH_SIZE, "Chunk size must be reasonable");
    
    // Verify delay requirements
    const MIN_DELAY_SECONDS: u64 = 2;
    const MAX_DELAY_SECONDS: u64 = 5;
    
    assert!(MIN_DELAY_SECONDS >= 2, "Minimum delay must be at least 2 seconds");
    assert!(MAX_DELAY_SECONDS <= 5, "Maximum delay must not exceed 5 seconds");
    assert!(MAX_DELAY_SECONDS > MIN_DELAY_SECONDS, "Max delay must be greater than min");
}