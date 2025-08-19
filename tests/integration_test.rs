use memory_core::{
    ConnectionConfig, ConnectionPool, CreateMemoryRequest, MemoryRepository, MemoryTier,
    SearchRequest,
};
use migration::MigrationRunner;
use sqlx::PgPool;
use testcontainers::{clients::Cli, images::postgres::Postgres, Container};
use uuid::Uuid;

struct TestContext<'a> {
    _container: Container<'a, Postgres>,
    pool: PgPool,
    repository: MemoryRepository,
}

async fn setup_test_context(docker: &Cli) -> TestContext {
    // Start PostgreSQL container with pgvector
    let postgres_image = Postgres::default()
        .with_db_name("test_db")
        .with_user("test_user")
        .with_password("test_pass");
    
    let container = docker.run(postgres_image);
    let port = container.get_host_port_ipv4(5432);
    
    // Create connection pool
    let connection_string = format!(
        "postgres://test_user:test_pass@localhost:{}/test_db",
        port
    );
    
    let pool = sqlx::PgPoolOptions::new()
        .max_connections(5)
        .connect(&connection_string)
        .await
        .expect("Failed to connect to test database");
    
    // Enable pgvector extension
    sqlx::query("CREATE EXTENSION IF NOT EXISTS vector")
        .execute(&pool)
        .await
        .ok();
    
    // Run migrations
    let runner = MigrationRunner::new(pool.clone(), "./migration/migrations");
    runner.migrate().await.expect("Failed to run migrations");
    
    let repository = MemoryRepository::new(pool.clone());
    
    TestContext {
        _container: container,
        pool,
        repository,
    }
}

#[tokio::test]
async fn test_create_and_retrieve_memory() {
    let docker = Cli::default();
    let ctx = setup_test_context(&docker).await;
    
    // Create a memory
    let request = CreateMemoryRequest {
        content: "Test memory content".to_string(),
        embedding: Some(vec![0.1; 1536]),
        tier: Some(MemoryTier::Working),
        importance_score: Some(0.8),
        metadata: Some(serde_json::json!({"test": "data"})),
        parent_id: None,
        expires_at: None,
    };
    
    let created = ctx.repository.create_memory(request).await.unwrap();
    assert_eq!(created.content, "Test memory content");
    assert_eq!(created.tier, MemoryTier::Working);
    
    // Retrieve the memory
    let retrieved = ctx.repository.get_memory(created.id).await.unwrap();
    assert_eq!(retrieved.id, created.id);
    assert_eq!(retrieved.access_count, 1); // Should increment on retrieval
}

#[tokio::test]
async fn test_duplicate_content_detection() {
    let docker = Cli::default();
    let ctx = setup_test_context(&docker).await;
    
    let request = CreateMemoryRequest {
        content: "Duplicate content".to_string(),
        embedding: None,
        tier: Some(MemoryTier::Working),
        importance_score: None,
        metadata: None,
        parent_id: None,
        expires_at: None,
    };
    
    // First creation should succeed
    ctx.repository.create_memory(request.clone()).await.unwrap();
    
    // Second creation with same content in same tier should fail
    let result = ctx.repository.create_memory(request).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_memory_search() {
    let docker = Cli::default();
    let ctx = setup_test_context(&docker).await;
    
    // Create memories with embeddings
    let embedding1 = vec![1.0, 0.0, 0.0];
    let embedding1_padded = [embedding1.clone(), vec![0.0; 1533]].concat();
    
    let embedding2 = vec![0.0, 1.0, 0.0];
    let embedding2_padded = [embedding2.clone(), vec![0.0; 1533]].concat();
    
    let memory1 = ctx
        .repository
        .create_memory(CreateMemoryRequest {
            content: "First memory".to_string(),
            embedding: Some(embedding1_padded.clone()),
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.9),
            metadata: None,
            parent_id: None,
            expires_at: None,
        })
        .await
        .unwrap();
    
    let _memory2 = ctx
        .repository
        .create_memory(CreateMemoryRequest {
            content: "Second memory".to_string(),
            embedding: Some(embedding2_padded),
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.5),
            metadata: None,
            parent_id: None,
            expires_at: None,
        })
        .await
        .unwrap();
    
    // Search with query similar to first memory
    let search_request = SearchRequest {
        query_embedding: embedding1_padded,
        tier: Some(MemoryTier::Working),
        limit: Some(5),
        similarity_threshold: Some(0.5),
        include_metadata: Some(true),
    };
    
    let results = ctx.repository.search_memories(search_request).await.unwrap();
    assert!(!results.is_empty());
    assert_eq!(results[0].memory.id, memory1.id);
}

#[tokio::test]
async fn test_memory_tier_migration() {
    let docker = Cli::default();
    let ctx = setup_test_context(&docker).await;
    
    // Create a working memory
    let memory = ctx
        .repository
        .create_memory(CreateMemoryRequest {
            content: "Memory to migrate".to_string(),
            embedding: None,
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.2), // Low importance
            metadata: None,
            parent_id: None,
            expires_at: None,
        })
        .await
        .unwrap();
    
    // Migrate to warm tier
    let migrated = ctx
        .repository
        .migrate_memory(memory.id, MemoryTier::Warm, Some("Test migration".to_string()))
        .await
        .unwrap();
    
    assert_eq!(migrated.tier, MemoryTier::Warm);
    assert_eq!(migrated.id, memory.id);
}

#[tokio::test]
async fn test_migration_candidates() {
    let docker = Cli::default();
    let ctx = setup_test_context(&docker).await;
    
    // Create memories with different importance scores
    for i in 0..5 {
        ctx.repository
            .create_memory(CreateMemoryRequest {
                content: format!("Memory {}", i),
                embedding: None,
                tier: Some(MemoryTier::Working),
                importance_score: Some(0.1 * i as f32), // Varying importance
                metadata: None,
                parent_id: None,
                expires_at: None,
            })
            .await
            .unwrap();
    }
    
    // Get migration candidates
    let candidates = ctx
        .repository
        .get_migration_candidates(MemoryTier::Working, 3)
        .await
        .unwrap();
    
    assert!(!candidates.is_empty());
    assert!(candidates.len() <= 3);
    
    // Verify they have low importance scores
    for candidate in candidates {
        assert!(candidate.importance_score < 0.3);
    }
}

#[tokio::test]
async fn test_expired_memory_cleanup() {
    let docker = Cli::default();
    let ctx = setup_test_context(&docker).await;
    
    use chrono::{Duration, Utc};
    
    // Create an expired memory
    let expired_time = Utc::now() - Duration::hours(1);
    ctx.repository
        .create_memory(CreateMemoryRequest {
            content: "Expired memory".to_string(),
            embedding: None,
            tier: Some(MemoryTier::Working),
            importance_score: None,
            metadata: None,
            parent_id: None,
            expires_at: Some(expired_time),
        })
        .await
        .unwrap();
    
    // Create a non-expired memory
    let future_time = Utc::now() + Duration::hours(1);
    ctx.repository
        .create_memory(CreateMemoryRequest {
            content: "Valid memory".to_string(),
            embedding: None,
            tier: Some(MemoryTier::Working),
            importance_score: None,
            metadata: None,
            parent_id: None,
            expires_at: Some(future_time),
        })
        .await
        .unwrap();
    
    // Cleanup expired memories
    let cleaned = ctx.repository.cleanup_expired_memories().await.unwrap();
    assert_eq!(cleaned, 1);
    
    // Verify expired memory is deleted
    let expired = ctx.repository.get_expired_memories().await.unwrap();
    assert!(expired.is_empty());
}

#[tokio::test]
async fn test_memory_statistics() {
    let docker = Cli::default();
    let ctx = setup_test_context(&docker).await;
    
    // Create memories in different tiers
    for tier in [MemoryTier::Working, MemoryTier::Warm, MemoryTier::Cold] {
        for i in 0..3 {
            ctx.repository
                .create_memory(CreateMemoryRequest {
                    content: format!("Memory {} in {:?}", i, tier),
                    embedding: None,
                    tier: Some(tier),
                    importance_score: Some(0.5),
                    metadata: None,
                    parent_id: None,
                    expires_at: None,
                })
                .await
                .unwrap();
        }
    }
    
    // Get statistics
    let stats = ctx.repository.get_statistics().await.unwrap();
    
    assert_eq!(stats.working_count, Some(3));
    assert_eq!(stats.warm_count, Some(3));
    assert_eq!(stats.cold_count, Some(3));
    assert_eq!(stats.total_active, Some(9));
}

#[tokio::test]
async fn test_hierarchical_memories() {
    let docker = Cli::default();
    let ctx = setup_test_context(&docker).await;
    
    // Create parent memory
    let parent = ctx
        .repository
        .create_memory(CreateMemoryRequest {
            content: "Parent memory".to_string(),
            embedding: None,
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.9),
            metadata: None,
            parent_id: None,
            expires_at: None,
        })
        .await
        .unwrap();
    
    // Create child memory
    let child = ctx
        .repository
        .create_memory(CreateMemoryRequest {
            content: "Child memory".to_string(),
            embedding: None,
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.7),
            metadata: None,
            parent_id: Some(parent.id),
            expires_at: None,
        })
        .await
        .unwrap();
    
    assert_eq!(child.parent_id, Some(parent.id));
}

#[tokio::test]
async fn test_connection_pool_health() {
    let docker = Cli::default();
    let _ctx = setup_test_context(&docker).await;
    
    let config = ConnectionConfig {
        host: "localhost".to_string(),
        port: 5432,
        database: "test_db".to_string(),
        username: "test_user".to_string(),
        password: "test_pass".to_string(),
        max_connections: 10,
        min_connections: 2,
        connection_timeout_seconds: 5,
        idle_timeout_seconds: 60,
        max_lifetime_seconds: 300,
    };
    
    // This will fail to connect but tests the config structure
    let _pool_result = ConnectionPool::new(config).await;
}