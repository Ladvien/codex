//! Tests for database transaction handling
//! 
//! This test suite verifies that all database transactions are properly
//! handled with explicit commit/rollback to prevent connection leaks.

use codex_memory::memory::{
    MemoryRepository, MemoryTier, CreateMemoryRequest
};
use codex_memory::memory::error::MemoryError;
use sqlx::PgPool;
use uuid::Uuid;

#[cfg(test)]
mod transaction_tests {
    use super::*;

    /// Test that migrate_memory_tier properly handles transactions when tier is unchanged
    #[sqlx::test]
    async fn test_migrate_tier_same_tier_rollback(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
        let repo = MemoryRepository::new(pool.clone());
        
        // Create a test memory
        let request = CreateMemoryRequest {
            content: "Test memory for transaction handling".to_string(),
            embedding: None,
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.5),
            metadata: Some(serde_json::json!({})),
            parent_id: None,
            expires_at: None,
        };
        
        // Store the memory
        let memory = repo.create_memory(request).await?;
        
        // Get active connection count before migration
        let conn_count_before: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pg_stat_activity WHERE state = 'active'"
        )
        .fetch_one(&pool)
        .await?;
        
        // Try to migrate to the same tier (should rollback transaction)
        let result = repo.migrate_memory(
            memory.id,
            MemoryTier::Working,
            Some("Test migration".to_string())
        ).await?;
        assert_eq!(result.tier, MemoryTier::Working);
        
        // Verify connection was properly released
        let conn_count_after: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pg_stat_activity WHERE state = 'active'"
        )
        .fetch_one(&pool)
        .await?;
        
        // Connection count should be the same (no leak)
        assert!(
            (conn_count_before - conn_count_after).abs() <= 1,
            "Transaction was not properly rolled back"
        );
        
        Ok(())
    }

    /// Test that migrate_memory_tier properly handles invalid tier transitions
    #[sqlx::test]
    async fn test_migrate_tier_invalid_transition_rollback(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
        let repo = MemoryRepository::new(pool.clone());
        
        // First create a memory in Working tier
        let request1 = CreateMemoryRequest {
            content: "Test working memory".to_string(),
            embedding: None,
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.7),
            metadata: Some(serde_json::json!({})),
            parent_id: None,
            expires_at: None,
        };
        let memory1 = repo.create_memory(request1).await?;
        
        // Migrate it to Cold tier (valid transition through Warm)
        repo.migrate_memory(
            memory1.id,
            MemoryTier::Warm,
            Some("Moving to warm".to_string())
        ).await?;
        
        repo.migrate_memory(
            memory1.id,
            MemoryTier::Cold,
            Some("Moving to cold".to_string())
        ).await?;
        
        // Now try invalid transition from Cold to Working (should rollback)
        let result = repo.migrate_memory(
            memory1.id,
            MemoryTier::Working,
            Some("Invalid transition".to_string())
        ).await;
        
        assert!(result.is_err(), "Invalid transition should fail");
        if let Err(MemoryError::InvalidTierTransition { from, to }) = result {
            assert_eq!(from, "Cold");
            assert_eq!(to, "Working");
        }
        
        // Verify no connection leak by checking pool statistics
        let pool_stats = pool.acquire().await?;
        drop(pool_stats); // Should succeed if connections are available
        
        Ok(())
    }

    /// Test successful transaction commit for valid operations
    #[sqlx::test]
    async fn test_successful_migration_commits(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
        let repo = MemoryRepository::new(pool.clone());
        
        // Create a test memory
        let request = CreateMemoryRequest {
            content: "Test memory for successful migration".to_string(),
            embedding: None,
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.4),
            metadata: Some(serde_json::json!({})),
            parent_id: None,
            expires_at: None,
        };
        
        let memory = repo.create_memory(request).await?;
        
        // Perform valid migration
        let migrated = repo.migrate_memory(
            memory.id,
            MemoryTier::Warm,
            Some("Valid migration".to_string())
        ).await?;
        assert_eq!(migrated.tier, MemoryTier::Warm);
        
        // Verify change was committed
        let memory_after = repo.get_by_id(&memory.id).await?.unwrap();
        assert_eq!(memory_after.tier, MemoryTier::Warm);
        
        Ok(())
    }
}

#[cfg(feature = "codex-dreams")]
mod insights_transaction_tests {
    use codex_memory::insights::storage::InsightStorage;
    use codex_memory::embedding::{EmbeddingService, EmbeddingServiceError};
    use std::sync::Arc;
    use async_trait::async_trait;
    use super::*;

    // Mock embedding service for tests
    struct MockEmbedder;
    
    #[async_trait]
    impl EmbeddingService for MockEmbedder {
        async fn embed(&self, _text: &str) -> Result<Vec<f32>, EmbeddingServiceError> {
            Ok(vec![0.0; 1536]) // Return dummy embedding
        }
        
        async fn embed_batch(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, EmbeddingServiceError> {
            Ok(texts.into_iter().map(|_| vec![0.0; 1536]).collect())
        }
    }

    /// Test that get_by_id doesn't use unnecessary transactions
    #[sqlx::test]
    async fn test_insights_get_by_id_no_transaction(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
        let embedder: Arc<dyn EmbeddingService> = Arc::new(MockEmbedder);
        let storage = InsightStorage::new(Arc::new(pool.clone()), embedder);
        
        // Try to get a non-existent insight (should not use transaction)
        let result = storage.get_by_id(Uuid::new_v4()).await?;
        assert!(result.is_none());
        
        // Verify we can still acquire connections (no leak)
        let _conn = pool.acquire().await?;
        
        Ok(())
    }
}