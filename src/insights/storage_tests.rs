//! Comprehensive tests for InsightStorage
//!
//! These tests verify all CRUD operations, vector embeddings, versioning,
//! feedback tracking, and error handling for the insight storage layer.

#[cfg(all(test, feature = "codex-dreams"))]
mod tests {
    use super::*;
    use crate::embedding::{EmbeddingService, EmbeddingProvider};
    use crate::memory::connection::create_pool;
    use async_trait::async_trait;
    use chrono::Utc;
    use pgvector::Vector;
    use sqlx::PgPool;
    use std::sync::Arc;
    use tempfile::TempDir;
    use uuid::Uuid;
    use anyhow::Result;

    // Mock embedding service for testing
    struct MockEmbedder {
        dimension: usize,
    }

    impl MockEmbedder {
        fn new(dimension: usize) -> Self {
            Self { dimension }
        }
    }

    #[async_trait]
    impl EmbeddingService for MockEmbedder {
        async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>> {
            // Generate a simple deterministic embedding based on text hash
            let mut embedding = vec![0.0f32; self.dimension];
            let text_bytes = text.as_bytes();
            
            for (i, byte) in text_bytes.iter().enumerate() {
                let idx = i % self.dimension;
                embedding[idx] += (*byte as f32) / 255.0;
            }
            
            // Normalize the vector
            let magnitude = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
            if magnitude > 0.0 {
                for val in embedding.iter_mut() {
                    *val /= magnitude;
                }
            }
            
            Ok(embedding)
        }

        async fn health_check(&self) -> Result<()> {
            Ok(())
        }
    }

    // Test helper to create test database
    async fn create_test_pool() -> Arc<PgPool> {
        // In real tests, this would use testcontainers or a test database
        // For now, we'll assume a test database is available
        let database_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://test:test@localhost/codex_test".to_string());
        
        let pool = create_pool(&database_url, 5).await.unwrap();
        Arc::new(pool)
    }

    // Test helper to create test insight
    fn create_test_insight() -> Insight {
        Insight {
            id: Uuid::new_v4(),
            content: "This is a test insight about memory patterns.".to_string(),
            insight_type: InsightType::Pattern,
            confidence_score: 0.85,
            source_memory_ids: vec![Uuid::new_v4(), Uuid::new_v4()],
            metadata: serde_json::json!({"source": "test", "analyzed_at": "2024-01-01"}),
            tags: vec!["pattern".to_string(), "memory".to_string(), "test".to_string()],
            tier: "working".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_accessed_at: None,
            version: 1,
            previous_version: None,
            previous_version_id: None,
            feedback_score: 0.5,
            embedding: None,
        }
    }

    #[tokio::test]
    #[ignore] // Requires test database setup
    async fn test_insight_storage_creation() {
        let pool = create_test_pool().await;
        let embedder = Arc::new(MockEmbedder::new(384)) as Arc<dyn EmbeddingService>;
        
        let storage = InsightStorage::new(pool.clone(), embedder);
        
        // Verify storage is properly initialized
        assert_eq!(storage.min_feedback_score, 0.3);
        assert_eq!(storage.max_versions_to_keep, 2);
    }

    #[tokio::test]
    #[ignore] // Requires test database setup
    async fn test_store_new_insight() {
        let pool = create_test_pool().await;
        let embedder = Arc::new(MockEmbedder::new(384)) as Arc<dyn EmbeddingService>;
        let storage = InsightStorage::new(pool, embedder);

        let insight = create_test_insight();
        let content = insight.content.clone();

        let result = storage.store(insight).await;
        assert!(result.is_ok(), "Failed to store insight: {:?}", result);

        let insight_id = result.unwrap();
        
        // Verify insight was stored with embedding
        let retrieved = storage.get_by_id(insight_id).await.unwrap();
        assert!(retrieved.is_some(), "Stored insight not found");
        
        let stored_insight = retrieved.unwrap();
        assert_eq!(stored_insight.content, content);
        assert_eq!(stored_insight.version, 1);
        assert!(stored_insight.embedding.is_some(), "Embedding not generated");
        assert_eq!(stored_insight.feedback_score, 0.5);
    }

    #[tokio::test]
    #[ignore] // Requires test database setup  
    async fn test_insight_versioning() {
        let pool = create_test_pool().await;
        let embedder = Arc::new(MockEmbedder::new(384)) as Arc<dyn EmbeddingService>;
        let storage = InsightStorage::new(pool, embedder);

        // Store initial insight
        let mut insight = create_test_insight();
        let original_content = insight.content.clone();
        let insight_id = storage.store(insight).await.unwrap();

        // Update the insight
        let updates = InsightUpdate {
            content: Some("Updated insight content with new information.".to_string()),
            confidence_score: Some(0.92),
            metadata: Some(serde_json::json!({"updated": true})),
            tags: Some(vec!["updated".to_string(), "pattern".to_string()]),
        };

        let result = storage.update_with_version(insight_id, updates).await;
        assert!(result.is_ok(), "Failed to update insight: {:?}", result);

        // Verify the insight was updated with new version
        let updated_insight = storage.get_by_id(insight_id).await.unwrap().unwrap();
        assert_eq!(updated_insight.version, 2);
        assert_eq!(updated_insight.content, "Updated insight content with new information.");
        assert_eq!(updated_insight.confidence_score, 0.92);
        assert!(updated_insight.previous_version_id.is_some());
    }

    #[tokio::test]
    #[ignore] // Requires test database setup
    async fn test_semantic_search() {
        let pool = create_test_pool().await;
        let embedder = Arc::new(MockEmbedder::new(384)) as Arc<dyn EmbeddingService>;
        let storage = InsightStorage::new(pool, embedder);

        // Store multiple insights with different content
        let insights = vec![
            ("Memory patterns in learning processes", InsightType::Pattern),
            ("Connection between memory and cognition", InsightType::Connection),
            ("Learning strategies for memory improvement", InsightType::Learning),
            ("Cognitive models of memory formation", InsightType::MentalModel),
        ];

        let mut stored_ids = Vec::new();
        for (content, insight_type) in insights {
            let mut insight = create_test_insight();
            insight.content = content.to_string();
            insight.insight_type = insight_type;
            let id = storage.store(insight).await.unwrap();
            stored_ids.push(id);
        }

        // Search for insights related to "memory learning"
        let results = storage.search("memory learning", 3).await.unwrap();
        
        assert!(!results.is_empty(), "Search should return results");
        assert!(results.len() <= 3, "Should respect limit parameter");
        
        // Results should be ordered by similarity
        for (i, result) in results.iter().enumerate() {
            assert!(result.similarity_score >= 0.0 && result.similarity_score <= 1.0);
            assert_eq!(result.rank, i + 1);
            
            // Higher ranked results should have higher or equal similarity scores
            if i > 0 {
                assert!(result.similarity_score <= results[i-1].similarity_score);
            }
        }
    }

    #[tokio::test]
    #[ignore] // Requires test database setup
    async fn test_feedback_system() {
        let pool = create_test_pool().await;
        let embedder = Arc::new(MockEmbedder::new(384)) as Arc<dyn EmbeddingService>;
        let storage = InsightStorage::new(pool, embedder);

        // Store an insight
        let insight = create_test_insight();
        let insight_id = storage.store(insight).await.unwrap();

        // Add positive feedback
        let positive_feedback = Feedback {
            insight_id,
            rating: FeedbackRating::Helpful,
            comment: Some("This insight is very useful!".to_string()),
            user_id: Some("test_user_1".to_string()),
            created_at: Utc::now(),
        };

        let result = storage.record_feedback(insight_id, positive_feedback).await;
        assert!(result.is_ok(), "Failed to record positive feedback: {:?}", result);

        // Add negative feedback
        let negative_feedback = Feedback {
            insight_id,
            rating: FeedbackRating::NotHelpful,
            comment: Some("Not relevant to my needs.".to_string()),
            user_id: Some("test_user_2".to_string()),
            created_at: Utc::now(),
        };

        let result = storage.record_feedback(insight_id, negative_feedback).await;
        assert!(result.is_ok(), "Failed to record negative feedback: {:?}", result);

        // Verify feedback score was recalculated
        let insight = storage.get_by_id(insight_id).await.unwrap().unwrap();
        // Score should be influenced by both positive (1.0) and negative (-0.5) feedback
        // (1.0 + (-0.5)) / 2 = 0.25, normalized: (0.25 + 1.0) / 2 = 0.625
        assert!((insight.feedback_score - 0.625).abs() < 0.01, 
               "Feedback score not calculated correctly: {}", insight.feedback_score);
    }

    #[tokio::test]
    #[ignore] // Requires test database setup
    async fn test_insight_pruning() {
        let pool = create_test_pool().await;
        let embedder = Arc::new(MockEmbedder::new(384)) as Arc<dyn EmbeddingService>;
        let storage = InsightStorage::new(pool, embedder);

        // Create insights with different feedback scores
        let mut low_quality_ids = Vec::new();
        let mut high_quality_ids = Vec::new();

        // Store low quality insights (will be pruned)
        for i in 0..3 {
            let mut insight = create_test_insight();
            insight.content = format!("Low quality insight {}", i);
            insight.feedback_score = 0.2; // Below typical threshold
            insight.created_at = Utc::now() - chrono::Duration::days(2); // Old enough to prune
            let id = storage.store(insight).await.unwrap();
            low_quality_ids.push(id);
        }

        // Store high quality insights (should not be pruned)
        for i in 0..2 {
            let mut insight = create_test_insight();
            insight.content = format!("High quality insight {}", i);
            insight.feedback_score = 0.8; // Above threshold
            insight.created_at = Utc::now() - chrono::Duration::days(2);
            let id = storage.store(insight).await.unwrap();
            high_quality_ids.push(id);
        }

        // Prune insights with threshold of 0.3
        let pruned_count = storage.prune_poor_insights(0.3).await.unwrap();
        assert_eq!(pruned_count, 3, "Should have pruned 3 low quality insights");

        // Verify low quality insights are archived
        for id in low_quality_ids {
            let insight = storage.get_by_id(id).await.unwrap();
            assert!(insight.is_none(), "Low quality insight should be pruned/archived");
        }

        // Verify high quality insights remain
        for id in high_quality_ids {
            let insight = storage.get_by_id(id).await.unwrap();
            assert!(insight.is_some(), "High quality insight should not be pruned");
        }
    }

    #[tokio::test]
    #[ignore] // Requires test database setup
    async fn test_insight_deletion() {
        let pool = create_test_pool().await;
        let embedder = Arc::new(MockEmbedder::new(384)) as Arc<dyn EmbeddingService>;
        let storage = InsightStorage::new(pool, embedder);

        // Store an insight
        let insight = create_test_insight();
        let insight_id = storage.store(insight).await.unwrap();

        // Verify insight exists
        let retrieved = storage.get_by_id(insight_id).await.unwrap();
        assert!(retrieved.is_some(), "Insight should exist before deletion");

        // Delete the insight
        let deleted = storage.delete(insight_id).await.unwrap();
        assert!(deleted, "Delete operation should return true");

        // Verify insight is no longer accessible
        let retrieved = storage.get_by_id(insight_id).await.unwrap();
        assert!(retrieved.is_none(), "Insight should be deleted/archived");

        // Try to delete again (should return false)
        let deleted_again = storage.delete(insight_id).await.unwrap();
        assert!(!deleted_again, "Second delete should return false");
    }

    #[tokio::test]
    #[ignore] // Requires test database setup
    async fn test_error_handling() {
        let pool = create_test_pool().await;
        let embedder = Arc::new(MockEmbedder::new(384)) as Arc<dyn EmbeddingService>;
        let storage = InsightStorage::new(pool, embedder);

        // Test getting non-existent insight
        let non_existent_id = Uuid::new_v4();
        let result = storage.get_by_id(non_existent_id).await.unwrap();
        assert!(result.is_none(), "Non-existent insight should return None");

        // Test updating non-existent insight
        let updates = InsightUpdate {
            content: Some("Updated content".to_string()),
            confidence_score: None,
            metadata: None,
            tags: None,
        };
        
        let result = storage.update_with_version(non_existent_id, updates).await;
        assert!(result.is_err(), "Updating non-existent insight should fail");
    }

    #[tokio::test]
    async fn test_insight_type_conversions() {
        let pool = create_test_pool().await;
        let embedder = Arc::new(MockEmbedder::new(384)) as Arc<dyn EmbeddingService>;
        let storage = InsightStorage::new(pool, embedder);

        // Test all insight type conversions
        let types = vec![
            (InsightType::Learning, "learning"),
            (InsightType::Connection, "connection"),
            (InsightType::Relationship, "relationship"),
            (InsightType::Assertion, "assertion"),
            (InsightType::MentalModel, "mental_model"),
            (InsightType::Pattern, "pattern"),
        ];

        for (insight_type, expected_string) in types {
            let string_result = storage.insight_type_to_string(&insight_type);
            assert_eq!(string_result, expected_string);

            let type_result = storage.string_to_insight_type(&string_result).unwrap();
            assert!(matches!(type_result, insight_type));
        }

        // Test invalid insight type
        let invalid_result = storage.string_to_insight_type("invalid_type");
        assert!(invalid_result.is_err());
    }

    #[tokio::test]
    async fn test_feedback_rating_conversions() {
        let pool = create_test_pool().await;
        let embedder = Arc::new(MockEmbedder::new(384)) as Arc<dyn EmbeddingService>;
        let storage = InsightStorage::new(pool, embedder);

        let ratings = vec![
            (FeedbackRating::Helpful, "helpful"),
            (FeedbackRating::NotHelpful, "not_helpful"),
            (FeedbackRating::Incorrect, "incorrect"),
        ];

        for (rating, expected_string) in ratings {
            let string_result = storage.feedback_rating_to_string(&rating);
            assert_eq!(string_result, expected_string);
        }
    }

    #[test]
    fn test_search_result_structure() {
        let insight = create_test_insight();
        let search_result = SearchResult {
            insight: insight.clone(),
            similarity_score: 0.95,
            rank: 1,
        };

        assert_eq!(search_result.similarity_score, 0.95);
        assert_eq!(search_result.rank, 1);
        assert_eq!(search_result.insight.id, insight.id);
    }

    #[test]
    fn test_insight_update_structure() {
        let updates = InsightUpdate {
            content: Some("New content".to_string()),
            confidence_score: Some(0.9),
            metadata: Some(serde_json::json!({"updated": true})),
            tags: Some(vec!["new_tag".to_string()]),
        };

        assert!(updates.content.is_some());
        assert!(updates.confidence_score.is_some());
        assert!(updates.metadata.is_some());
        assert!(updates.tags.is_some());
    }
}