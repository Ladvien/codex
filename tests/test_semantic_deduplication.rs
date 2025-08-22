use codex_memory::{
    memory::{
        models::CreateMemoryRequest, MemoryRepository, MemoryTier, SemanticDeduplicationConfig,
        SemanticDeduplicationEngine,
    },
    embedding::SimpleEmbedder,
};
use pgvector::Vector;
use sqlx::PgPool;
use std::sync::Arc;
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

#[tokio::test]
async fn test_semantic_deduplication_basic() {
    let pool = setup_test_pool().await;
    let repository = Arc::new(MemoryRepository::new(pool));

    // Create a mock embedding service
    let embedding_service = Arc::new(SimpleEmbedder::new_mock());

    // Create deduplication configuration
    let config = SemanticDeduplicationConfig {
        similarity_threshold: 0.85,
        batch_size: 10,
        max_memories_per_operation: 100,
        ..Default::default()
    };

    // Create the deduplication engine
    let engine = SemanticDeduplicationEngine::new(config, repository.clone(), embedding_service);

    // Create some test memories with similar content
    let memory1 = repository
        .create_memory(CreateMemoryRequest {
            content: "This is a test memory about machine learning".to_string(),
            embedding: Some(vec![0.1, 0.2, 0.3, 0.4, 0.5]),
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.8),
            metadata: None,
            parent_id: None,
            expires_at: None,
        })
        .await
        .expect("Failed to create test memory 1");

    let memory2 = repository
        .create_memory(CreateMemoryRequest {
            content: "This is a test memory about machine learning concepts".to_string(),
            embedding: Some(vec![0.1, 0.2, 0.3, 0.4, 0.6]), // Very similar embedding
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.7),
            metadata: None,
            parent_id: None,
            expires_at: None,
        })
        .await
        .expect("Failed to create test memory 2");

    let memory_ids = vec![memory1.id, memory2.id];

    // Test deduplication
    let result = engine
        .deduplicate_batch(&memory_ids)
        .await
        .expect("Deduplication should succeed");

    // Verify results
    assert_eq!(result.total_processed, 2);
    assert!(result.execution_time_ms > 0);
    println!("Deduplication result: {:?}", result);

    // Verify metrics
    let metrics = engine.get_metrics().await;
    assert_eq!(metrics.total_operations, 1);
    assert_eq!(metrics.total_memories_processed, 2);
    println!("Deduplication metrics: {:?}", metrics);
}

#[tokio::test]
async fn test_memory_statistics() {
    let pool = setup_test_pool().await;
    let repository = Arc::new(MemoryRepository::new(pool));
    let embedding_service = Arc::new(SimpleEmbedder::new_mock());
    let config = SemanticDeduplicationConfig::default();

    let engine = SemanticDeduplicationEngine::new(config, repository.clone(), embedding_service);

    // Test getting memory statistics
    let stats = engine
        .get_memory_statistics()
        .await
        .expect("Should get memory statistics");

    assert!(stats.total_memories >= 0);
    assert!(stats.total_content_bytes >= 0);
    println!("Memory statistics: {:?}", stats);
}

#[tokio::test]
async fn test_cosine_similarity_calculation() {
    let pool = setup_test_pool().await;
    let repository = Arc::new(MemoryRepository::new(pool));
    let embedding_service = Arc::new(SimpleEmbedder::new_mock());
    let config = SemanticDeduplicationConfig::default();

    let engine = SemanticDeduplicationEngine::new(config, repository, embedding_service);

    // Test identical vectors
    let vec1 = Vector::from(vec![1.0, 0.0, 0.0]);
    let vec2 = Vector::from(vec![1.0, 0.0, 0.0]);
    let similarity = engine
        .calculate_cosine_similarity(&vec1, &vec2)
        .expect("Should calculate similarity");
    assert!((similarity - 1.0).abs() < 0.001);

    // Test orthogonal vectors
    let vec3 = Vector::from(vec![1.0, 0.0, 0.0]);
    let vec4 = Vector::from(vec![0.0, 1.0, 0.0]);
    let similarity = engine
        .calculate_cosine_similarity(&vec3, &vec4)
        .expect("Should calculate similarity");
    assert!(similarity.abs() < 0.001);

    println!("Cosine similarity tests passed");
}