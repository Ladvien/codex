//! Test utilities for Codex Dreams insight tests
//!
//! Provides helpers for setting up test environments, creating test data,
//! and validating insight generation results.

use anyhow::Result;
use codex_memory::{
    insights::models::{Insight, InsightType},
    memory::{Memory, MemoryRepository, MemoryTier},
};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

/// Test environment for insight tests
pub struct InsightTestEnv {
    pub pool: PgPool,
    pub repository: Arc<MemoryRepository>,
    pub ollama_url: String,
}

impl InsightTestEnv {
    /// Create a new test environment with database and mock Ollama
    pub async fn new() -> Result<Self> {
        // Use test database URL or create temp database
        let database_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://postgres:postgres@localhost:5432/codex_test".to_string());
        
        let pool = PgPool::connect(&database_url).await?;
        
        // Run migrations if needed
        sqlx::migrate!("./migration/migrations")
            .run(&pool)
            .await?;
        
        let repository = Arc::new(MemoryRepository::new(pool.clone()));
        
        // Start mock Ollama server
        let mock_config = crate::helpers::ollama_mock::MockOllamaConfig::default();
        let mock_server = crate::helpers::ollama_mock::MockOllamaServer::new(mock_config);
        let ollama_url = mock_server.start().await?;
        
        Ok(Self {
            pool,
            repository,
            ollama_url,
        })
    }
    
    /// Clean up test data after test
    pub async fn cleanup(&self) -> Result<()> {
        sqlx::query("DELETE FROM insights WHERE metadata->>'test' = 'true'")
            .execute(&self.pool)
            .await?;
        
        sqlx::query("DELETE FROM memories WHERE metadata->>'test' = 'true'")
            .execute(&self.pool)
            .await?;
        
        Ok(())
    }
}

/// Builder for creating test memories
pub struct TestMemoryBuilder {
    content: String,
    tier: MemoryTier,
    importance_score: f32,
    metadata: serde_json::Value,
}

impl TestMemoryBuilder {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            tier: MemoryTier::Working,
            importance_score: 0.5,
            metadata: serde_json::json!({
                "test": true,
                "created_by": "test_suite"
            }),
        }
    }
    
    pub fn with_tier(mut self, tier: MemoryTier) -> Self {
        self.tier = tier;
        self
    }
    
    pub fn with_importance(mut self, score: f32) -> Self {
        self.importance_score = score;
        self
    }
    
    pub fn with_metadata(mut self, key: &str, value: serde_json::Value) -> Self {
        self.metadata[key] = value;
        self
    }
    
    pub async fn create(self, repo: &MemoryRepository) -> Result<Memory> {
        let memory = Memory {
            id: Uuid::new_v4(),
            content: self.content,
            content_hash: String::new(), // Will be generated
            embedding: None,
            tier: self.tier,
            importance_score: self.importance_score,
            recency_score: 1.0,
            combined_score: self.importance_score,
            metadata: self.metadata,
            tags: vec![],
            parent_id: None,
            child_ids: vec![],
            status: codex_memory::memory::models::MemoryStatus::Active,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            last_accessed_at: Some(chrono::Utc::now()),
            access_count: 0,
            access_pattern: None,
            expires_at: None,
            recall_probability: Some(1.0),
            consolidation_score: Some(0.5),
        };
        
        repo.store_memory(memory.clone()).await?;
        Ok(memory)
    }
}

/// Builder for creating test insights
pub struct TestInsightBuilder {
    content: String,
    insight_type: InsightType,
    confidence_score: f32,
    source_memory_ids: Vec<Uuid>,
}

impl TestInsightBuilder {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            insight_type: InsightType::Learning,
            confidence_score: 0.75,
            source_memory_ids: vec![],
        }
    }
    
    pub fn with_type(mut self, insight_type: InsightType) -> Self {
        self.insight_type = insight_type;
        self
    }
    
    pub fn with_confidence(mut self, score: f32) -> Self {
        self.confidence_score = score;
        self
    }
    
    pub fn from_memory(mut self, memory_id: Uuid) -> Self {
        self.source_memory_ids.push(memory_id);
        self
    }
    
    pub fn build(self) -> Insight {
        Insight {
            id: Uuid::new_v4(),
            content: self.content,
            insight_type: self.insight_type,
            confidence_score: self.confidence_score,
            source_memory_ids: self.source_memory_ids,
            metadata: serde_json::json!({
                "test": true,
                "created_by": "test_suite"
            }),
            tags: vec![],
            tier: "working".to_string(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            last_accessed_at: None,
            feedback_score: 0.0,
            version: 1,
            previous_version: None,
            previous_version_id: None,
            embedding: None,
        }
    }
}

/// Assertions for validating insights
pub struct InsightAssertions;

impl InsightAssertions {
    pub fn assert_valid_insight(insight: &Insight) {
        assert!(!insight.content.is_empty(), "Insight content should not be empty");
        assert!(insight.content.len() >= 10, "Insight content too short");
        assert!(insight.confidence_score >= 0.0 && insight.confidence_score <= 1.0, 
            "Confidence score out of range");
        assert!(!insight.source_memory_ids.is_empty(), 
            "Insight should have source memories");
    }
    
    pub fn assert_insight_type(insight: &Insight, expected_type: InsightType) {
        assert_eq!(insight.insight_type, expected_type, 
            "Insight type mismatch. Expected {:?}, got {:?}", 
            expected_type, insight.insight_type);
    }
    
    pub fn assert_confidence_range(insight: &Insight, min: f32, max: f32) {
        assert!(
            insight.confidence_score >= min && insight.confidence_score <= max,
            "Confidence score {} outside expected range [{}, {}]",
            insight.confidence_score, min, max
        );
    }
}

/// Performance measurement utilities
pub struct PerformanceMetrics {
    start_time: std::time::Instant,
    measurements: Vec<(String, std::time::Duration)>,
}

impl PerformanceMetrics {
    pub fn new() -> Self {
        Self {
            start_time: std::time::Instant::now(),
            measurements: vec![],
        }
    }
    
    pub fn measure<F, R>(&mut self, name: &str, f: F) -> R 
    where
        F: FnOnce() -> R,
    {
        let start = std::time::Instant::now();
        let result = f();
        let duration = start.elapsed();
        self.measurements.push((name.to_string(), duration));
        result
    }
    
    pub async fn measure_async<F, R>(&mut self, name: &str, f: F) -> R
    where
        F: std::future::Future<Output = R>,
    {
        let start = std::time::Instant::now();
        let result = f.await;
        let duration = start.elapsed();
        self.measurements.push((name.to_string(), duration));
        result
    }
    
    pub fn assert_under_threshold(&self, name: &str, max_ms: u64) {
        let measurement = self.measurements.iter()
            .find(|(n, _)| n == name)
            .expect(&format!("No measurement found for '{}'", name));
        
        let duration_ms = measurement.1.as_millis() as u64;
        assert!(
            duration_ms <= max_ms,
            "Performance threshold exceeded for '{}': {}ms > {}ms",
            name, duration_ms, max_ms
        );
    }
    
    pub fn report(&self) {
        println!("\n=== Performance Report ===");
        for (name, duration) in &self.measurements {
            println!("{}: {:?}", name, duration);
        }
        println!("Total time: {:?}", self.start_time.elapsed());
        println!("========================\n");
    }
}