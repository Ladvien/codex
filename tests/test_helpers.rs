//! Test helpers and infrastructure for the Agentic Memory System
//!
//! This module provides comprehensive test infrastructure including:
//! - Test environment setup with proper database isolation
//! - Mock and real embedding services
//! - Test data generation and cleanup
//! - Performance and load testing utilities

use anyhow::{Context, Result};
use codex_memory::{
    memory::{
        connection::create_pool,
        models::{CreateMemoryRequest, MemoryTier, SearchRequest},
        MemoryRepository,
    },
    Config, SimpleEmbedder,
};
use serde_json::json;
use sqlx::{PgPool, Row};
use std::sync::Arc;
use uuid::Uuid;

/// Test environment that provides isolated database and configured services
#[derive(Clone)]
pub struct TestEnvironment {
    pub repository: Arc<MemoryRepository>,
    pub embedder: Arc<SimpleEmbedder>,
    pub config: Config,
    pub test_id: String,
    pub pool: PgPool,
}

impl Drop for TestEnvironment {
    fn drop(&mut self) {
        tracing::info!("Cleaning up test environment {}", self.test_id);
        // Note: Async cleanup would need to be done before drop
        // For now, we rely on database transactions and proper cleanup
    }
}

impl TestEnvironment {
    /// Create a new test environment with proper isolation
    pub async fn new() -> Result<Self> {
        Self::new_with_config(None).await
    }

    /// Create a new test environment with custom configuration
    pub async fn new_with_config(custom_config: Option<Config>) -> Result<Self> {
        let test_id = Uuid::new_v4().to_string()[..8].to_string();

        // Load configuration - use custom or environment
        let config = match custom_config {
            Some(config) => config,
            None => Config::from_env().unwrap_or_else(|_| {
                // Use test-friendly defaults if env config fails
                let mut config = Config::default();
                config.database_url = std::env::var("TEST_DATABASE_URL")
                    .or_else(|_| std::env::var("DATABASE_URL"))
                    .unwrap_or_else(|_| {
                        "postgresql://postgres:password@localhost:5432/postgres".to_string()
                    });

                // Use mock embedder by default for tests unless explicitly configured
                if std::env::var("USE_REAL_EMBEDDINGS").unwrap_or_else(|_| "false".to_string())
                    == "true"
                {
                    // Keep Ollama configuration for real embedding tests
                    config.embedding.provider = "ollama".to_string();
                    config.embedding.model = "nomic-embed-text".to_string();
                    config.embedding.base_url = "http://192.168.1.110:11434".to_string();
                } else {
                    // Use mock embedder for fast tests
                    config.embedding.provider = "mock".to_string();
                    config.embedding.model = "mock-model".to_string();
                    config.embedding.base_url = "http://localhost:11434".to_string();
                }
                config
            }),
        };

        // Create connection pool
        let pool = create_pool(&config.database_url, config.operational.max_db_connections).await
            .map_err(|e| anyhow::anyhow!("Failed to create database pool for tests. Ensure test database is available: {}", e))?;

        // Set up database schema for tests (migration functionality not available in crates.io version)
        Self::setup_test_schema(&pool).await?;

        // Create repository
        let repository = Arc::new(MemoryRepository::new(pool.clone()));

        // Create embedder based on configuration
        let embedder = Arc::new(Self::create_embedder(&config)?);

        Ok(TestEnvironment {
            repository,
            embedder,
            config,
            test_id,
            pool,
        })
    }

    /// Create an embedder based on configuration
    fn create_embedder(config: &Config) -> Result<SimpleEmbedder> {
        match config.embedding.provider.as_str() {
            "openai" => Ok(SimpleEmbedder::new(config.embedding.api_key.clone())
                .with_model(config.embedding.model.clone())
                .with_base_url(config.embedding.base_url.clone())),
            "ollama" => Ok(SimpleEmbedder::new_ollama(
                config.embedding.base_url.clone(),
                config.embedding.model.clone(),
            )),
            "mock" => Ok(SimpleEmbedder::new_mock()),
            _ => Err(anyhow::anyhow!(
                "Unsupported embedding provider for tests: {}",
                config.embedding.provider
            )),
        }
    }

    /// Set up the required database schema for tests
    async fn setup_test_schema(pool: &PgPool) -> Result<()> {
        // Enable pgvector extension
        sqlx::query("CREATE EXTENSION IF NOT EXISTS vector")
            .execute(pool)
            .await
            .context("Failed to create vector extension")?;

        // Create memories table if it doesn't exist
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS memories (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                content TEXT NOT NULL,
                content_hash VARCHAR(64) NOT NULL,
                embedding vector(768),
                tier VARCHAR(20) NOT NULL DEFAULT 'working',
                status VARCHAR(20) NOT NULL DEFAULT 'active',
                importance_score REAL NOT NULL DEFAULT 0.5,
                access_count INTEGER NOT NULL DEFAULT 0,
                last_accessed_at TIMESTAMPTZ DEFAULT NOW(),
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                metadata JSONB DEFAULT '{}',
                parent_id UUID REFERENCES memories(id),
                expires_at TIMESTAMPTZ,
                -- Consolidation fields
                consolidation_strength REAL DEFAULT 1.0,
                decay_rate REAL DEFAULT 1.0,
                recall_probability REAL,
                last_recall_interval INTERVAL,
                CONSTRAINT check_consolidation_strength CHECK (consolidation_strength >= 0.0 AND consolidation_strength <= 10.0),
                CONSTRAINT check_decay_rate CHECK (decay_rate >= 0.0 AND decay_rate <= 5.0),
                CONSTRAINT check_recall_probability CHECK (recall_probability >= 0.0 AND recall_probability <= 1.0)
            )
        "#,
        )
        .execute(pool)
        .await
        .context("Failed to create memories table")?;

        // Create indexes for performance
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_memories_tier ON memories(tier)")
            .execute(pool)
            .await
            .ok();

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_memories_importance ON memories(importance)")
            .execute(pool)
            .await
            .ok();

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_memories_created_at ON memories(created_at)")
            .execute(pool)
            .await
            .ok();

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_memories_embedding_cosine ON memories USING ivfflat (embedding vector_cosine_ops)")
            .execute(pool)
            .await
            .ok(); // Index creation might fail if ivfflat is not available

        // Create consolidation tables
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS memory_consolidation_log (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                memory_id UUID NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
                old_consolidation_strength FLOAT NOT NULL,
                new_consolidation_strength FLOAT NOT NULL,
                old_recall_probability FLOAT,
                new_recall_probability FLOAT,
                consolidation_event VARCHAR(50) NOT NULL,
                trigger_reason TEXT,
                created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
            )
        "#,
        )
        .execute(pool)
        .await
        .context("Failed to create memory_consolidation_log table")?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS frozen_memories (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                original_memory_id UUID NOT NULL UNIQUE,
                compressed_content JSONB NOT NULL,
                original_metadata JSONB DEFAULT '{}',
                freeze_reason VARCHAR(100),
                frozen_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
                unfreeze_count INTEGER DEFAULT 0,
                last_unfrozen_at TIMESTAMP WITH TIME ZONE,
                compression_ratio FLOAT
            )
        "#,
        )
        .execute(pool)
        .await
        .context("Failed to create frozen_memories table")?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS memory_tier_statistics (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                tier VARCHAR(20) NOT NULL,
                memory_count INTEGER NOT NULL,
                avg_consolidation_strength FLOAT,
                avg_recall_probability FLOAT,
                avg_access_count FLOAT,
                total_storage_bytes BIGINT,
                recorded_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
            )
        "#,
        )
        .execute(pool)
        .await
        .context("Failed to create memory_tier_statistics table")?;

        // Create consolidation functions (with error handling for concurrent creation)
        let create_recall_fn = sqlx::query(
            r#"
            CREATE OR REPLACE FUNCTION calculate_recall_probability(
                p_consolidation_strength FLOAT,
                p_decay_rate FLOAT,
                p_time_since_access INTERVAL
            ) RETURNS FLOAT AS $$
            DECLARE
                t FLOAT;
                gn FLOAT;
                recall_prob FLOAT;
            BEGIN
                t := EXTRACT(EPOCH FROM p_time_since_access) / 3600.0;
                gn := p_consolidation_strength;
                
                IF t = 0 THEN
                    recall_prob := 1.0;
                ELSE
                    recall_prob := GREATEST(0.0, LEAST(1.0, 
                        (1.0 - EXP(-p_decay_rate * EXP(-t/gn))) / (1.0 - EXP(-1.0))
                    ));
                END IF;
                
                RETURN recall_prob;
            END;
            $$ LANGUAGE plpgsql IMMUTABLE
        "#,
        )
        .execute(pool)
        .await;

        if let Err(e) = create_recall_fn {
            // Ignore if function already exists due to concurrent test setup
            if !e.to_string().contains("tuple concurrently updated")
                && !e.to_string().contains("already exists")
            {
                return Err(anyhow::anyhow!(
                    "Failed to create calculate_recall_probability function: {}",
                    e
                ));
            }
        }

        let create_strength_fn = sqlx::query(
            r#"
            CREATE OR REPLACE FUNCTION update_consolidation_strength(
                p_current_strength FLOAT,
                p_time_since_last_access INTERVAL
            ) RETURNS FLOAT AS $$
            DECLARE
                t FLOAT;
                new_strength FLOAT;
            BEGIN
                t := EXTRACT(EPOCH FROM p_time_since_last_access) / 3600.0;
                new_strength := p_current_strength + (1.0 - EXP(-t))/(1.0 + EXP(-t));
                RETURN LEAST(10.0, new_strength);
            END;
            $$ LANGUAGE plpgsql IMMUTABLE
        "#,
        )
        .execute(pool)
        .await;

        if let Err(e) = create_strength_fn {
            // Ignore if function already exists due to concurrent test setup
            if !e.to_string().contains("tuple concurrently updated")
                && !e.to_string().contains("already exists")
            {
                return Err(anyhow::anyhow!(
                    "Failed to create update_consolidation_strength function: {}",
                    e
                ));
            }
        }

        // Create consolidation trigger function
        let create_trigger_fn = sqlx::query(
            r#"
            CREATE OR REPLACE FUNCTION trigger_consolidation_update() RETURNS TRIGGER AS $$
            DECLARE
                time_diff INTERVAL;
                new_consolidation FLOAT;
                new_recall_prob FLOAT;
            BEGIN
                -- Only trigger on access updates (when last_accessed_at changes)
                IF TG_OP = 'UPDATE' AND OLD.last_accessed_at != NEW.last_accessed_at THEN
                    
                    -- Calculate time since last access
                    time_diff := NEW.last_accessed_at - OLD.last_accessed_at;
                    
                    -- Update consolidation strength
                    new_consolidation := update_consolidation_strength(
                        COALESCE(OLD.consolidation_strength, 1.0), 
                        time_diff
                    );
                    
                    -- Calculate new recall probability
                    new_recall_prob := calculate_recall_probability(
                        new_consolidation,
                        COALESCE(NEW.decay_rate, 1.0),
                        INTERVAL '0 seconds' -- Just accessed, so immediate recall
                    );
                    
                    -- Update the new record
                    NEW.consolidation_strength := new_consolidation;
                    NEW.recall_probability := new_recall_prob;
                    NEW.last_recall_interval := time_diff;
                    
                    -- Log the consolidation event
                    INSERT INTO memory_consolidation_log (
                        memory_id,
                        old_consolidation_strength,
                        new_consolidation_strength,
                        old_recall_probability,
                        new_recall_probability,
                        consolidation_event,
                        trigger_reason
                    ) VALUES (
                        NEW.id,
                        COALESCE(OLD.consolidation_strength, 1.0),
                        new_consolidation,
                        OLD.recall_probability,
                        new_recall_prob,
                        'access',
                        'Automatic consolidation on memory access'
                    );
                END IF;
                
                RETURN NEW;
            END;
            $$ LANGUAGE plpgsql
        "#,
        )
        .execute(pool)
        .await;
        
        if let Err(e) = create_trigger_fn {
            if !e.to_string().contains("tuple concurrently updated")
                && !e.to_string().contains("already exists")
            {
                return Err(anyhow::anyhow!(
                    "Failed to create trigger function: {}",
                    e
                ));
            }
        }

        // Create the trigger itself
        let _create_trigger = sqlx::query(
            r#"
            DROP TRIGGER IF EXISTS memories_consolidation_trigger ON memories;
            CREATE TRIGGER memories_consolidation_trigger
                BEFORE UPDATE ON memories
                FOR EACH ROW
                EXECUTE FUNCTION trigger_consolidation_update()
        "#,
        )
        .execute(pool)
        .await; // Allow this to fail silently in test environments

        // Create migration_history table if it doesn't exist (for health checks)
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS migration_history (
                id SERIAL PRIMARY KEY,
                migration_name VARCHAR(255) NOT NULL,
                success BOOLEAN NOT NULL DEFAULT TRUE,
                completed_at TIMESTAMPTZ DEFAULT NOW(),
                migration_reason TEXT,
                migration_notes TEXT
            )
        "#,
        )
        .execute(pool)
        .await
        .context("Failed to create migration_history table")?;

        Ok(())
    }

    /// Get test-specific metadata that includes the test ID for cleanup
    pub fn get_test_metadata(&self, additional: Option<serde_json::Value>) -> serde_json::Value {
        let mut metadata = json!({
            "test_id": self.test_id,
            "test_env": true,
            "created_at": chrono::Utc::now().to_rfc3339()
        });

        if let Some(additional) = additional {
            if let (serde_json::Value::Object(ref mut base), serde_json::Value::Object(extra)) =
                (&mut metadata, additional)
            {
                for (key, value) in extra {
                    base.insert(key, value);
                }
            }
        }

        metadata
    }

    /// Clean up all test data created by this environment
    pub async fn cleanup_test_data(&self) -> Result<()> {
        // Delete all memories with this test_id
        let cleanup_query = r#"
            DELETE FROM memories
            WHERE metadata->>'test_id' = $1
        "#;

        sqlx::query(cleanup_query)
            .bind(&self.test_id)
            .execute(&self.pool)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to cleanup test data: {}", e))?;

        tracing::info!("Cleaned up test data for test_id: {}", self.test_id);
        Ok(())
    }

    /// Create a test memory with standard test metadata
    pub async fn create_test_memory(
        &self,
        content: &str,
        tier: MemoryTier,
        importance: f64,
    ) -> Result<codex_memory::Memory> {
        let request = CreateMemoryRequest {
            content: content.to_string(),
            embedding: None, // Will be generated
            tier: Some(tier),
            importance_score: Some(importance),
            metadata: Some(self.get_test_metadata(Some(json!({
                "content_type": "test_memory",
                "tier": tier,
                "importance": importance
            })))),
            parent_id: None,
            expires_at: None,
        };

        self.repository
            .create_memory(request)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    }

    /// Create multiple test memories with different characteristics
    pub async fn create_test_memories(&self, count: usize) -> Result<Vec<codex_memory::Memory>> {
        let mut memories = Vec::new();

        for i in 0..count {
            let content = format!("Test memory {} - {}", i, self.test_id);
            let tier = match i % 3 {
                0 => MemoryTier::Working,
                1 => MemoryTier::Warm,
                _ => MemoryTier::Cold,
            };
            let importance = 0.1 + ((i as f64 * 0.1) % 1.0);

            let memory = self.create_test_memory(&content, tier, importance).await?;
            memories.push(memory);
        }

        Ok(memories)
    }

    /// Perform a test search with common parameters
    pub async fn test_search(
        &self,
        query: &str,
        limit: Option<i32>,
    ) -> Result<Vec<codex_memory::memory::models::SearchResult>> {
        let request = SearchRequest {
            query_text: Some(query.to_string()),
            query_embedding: None,
            search_type: None,
            hybrid_weights: None,
            tier: None,
            date_range: None,
            importance_range: None,
            metadata_filters: Some(json!({"test_id": self.test_id})), // Only search our test data
            tags: None,
            limit: limit.or(Some(10)),
            offset: None,
            cursor: None,
            similarity_threshold: None,
            include_metadata: Some(true),
            include_facets: None,
            ranking_boost: None,
            explain_score: None,
        };

        self.repository
            .search_memories(request)
            .await
            .map(|response| response.results)
            .map_err(|e| anyhow::anyhow!("{}", e))
    }

    /// Wait for database operations to be committed (useful for concurrent tests)
    pub async fn wait_for_consistency(&self) {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    /// Get statistics about memories in this test environment
    pub async fn get_test_statistics(&self) -> Result<TestStatistics> {
        let query = r#"
            SELECT
                COUNT(*) as total_count,
                COUNT(*) FILTER (WHERE tier = 'working') as working_count,
                COUNT(*) FILTER (WHERE tier = 'warm') as warm_count,
                COUNT(*) FILTER (WHERE tier = 'cold') as cold_count,
                AVG(importance_score) as avg_importance,
                AVG(access_count) as avg_access_count
            FROM memories
            WHERE metadata->>'test_id' = $1
        "#;

        let row = sqlx::query(query)
            .bind(&self.test_id)
            .fetch_one(&self.pool)
            .await?;

        Ok(TestStatistics {
            total_count: row.get::<i64, _>("total_count") as usize,
            working_count: row.get::<i64, _>("working_count") as usize,
            warm_count: row.get::<i64, _>("warm_count") as usize,
            cold_count: row.get::<i64, _>("cold_count") as usize,
            avg_importance: row.get::<Option<f64>, _>("avg_importance").unwrap_or(0.0),
            avg_access_count: row.get::<Option<f64>, _>("avg_access_count").unwrap_or(0.0),
        })
    }
}

/// Test statistics for verification
#[derive(Debug)]
pub struct TestStatistics {
    pub total_count: usize,
    pub working_count: usize,
    pub warm_count: usize,
    pub cold_count: usize,
    pub avg_importance: f64,
    pub avg_access_count: f64,
}

// Note: MockEmbedder functionality is now built into SimpleEmbedder with the Mock provider

/// Test data generators for various scenarios
pub struct TestDataGenerator;

impl TestDataGenerator {
    /// Generate code-related test content
    pub fn code_samples() -> Vec<(&'static str, &'static str)> {
        vec![
            ("fn main() { println!(\"Hello, world!\"); }", "rust function"),
            ("class MyClass:\n    def __init__(self):\n        pass", "python class"),
            ("function add(a, b) { return a + b; }", "javascript function"),
            ("SELECT * FROM users WHERE id = ?", "sql query"),
            ("import React from 'react';", "react import"),
            ("use std::collections::HashMap;", "rust import"),
            ("def calculate_fibonacci(n): return n if n <= 1 else calculate_fibonacci(n-1) + calculate_fibonacci(n-2)", "python recursive function"),
            ("const users = await fetch('/api/users').then(r => r.json());", "javascript async"),
            ("CREATE TABLE users (id SERIAL PRIMARY KEY, name VARCHAR(255));", "sql ddl"),
            ("#[derive(Debug, Clone)] struct User { id: u32, name: String }", "rust struct"),
        ]
    }

    /// Generate conversation-style test content
    pub fn conversation_samples() -> Vec<&'static str> {
        vec![
            "How do I implement a binary search tree in Rust?",
            "The user wants to create a REST API with authentication",
            "Error: cannot borrow `x` as mutable more than once at a time",
            "Let's discuss the architecture for this microservice",
            "The database query is running slowly, we need to optimize it",
            "I need help understanding async/await in JavaScript",
            "The CI/CD pipeline is failing on the test stage",
            "How should we handle rate limiting in our API?",
            "The user reported a bug in the payment processing module",
            "Let's review the security implications of this change",
        ]
    }

    /// Generate large content for stress testing
    pub fn large_content(size_kb: usize) -> String {
        let base = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. ";
        let target_size = size_kb * 1024;
        let repeat_count = (target_size / base.len()) + 1;

        base.repeat(repeat_count)[..target_size].to_string()
    }

    /// Generate metadata with various patterns
    pub fn metadata_samples() -> Vec<serde_json::Value> {
        vec![
            json!({"type": "code", "language": "rust", "complexity": "medium"}),
            json!({"type": "conversation", "topic": "architecture", "urgency": "high"}),
            json!({"type": "error", "severity": "critical", "component": "database"}),
            json!({"type": "documentation", "section": "api", "version": "v1.2.3"}),
            json!({"type": "meeting", "attendees": 5, "duration_minutes": 45}),
        ]
    }
}

/// Performance measurement utilities
pub struct PerformanceMeter {
    start_time: std::time::Instant,
    operation_name: String,
}

impl PerformanceMeter {
    pub fn new(operation_name: &str) -> Self {
        Self {
            start_time: std::time::Instant::now(),
            operation_name: operation_name.to_string(),
        }
    }

    pub fn elapsed(&self) -> std::time::Duration {
        self.start_time.elapsed()
    }

    pub fn finish(self) -> PerformanceResult {
        let duration = self.elapsed();
        tracing::info!(
            "Operation '{}' completed in {:?}",
            self.operation_name,
            duration
        );

        PerformanceResult {
            operation_name: self.operation_name,
            duration,
        }
    }
}

#[derive(Debug)]
pub struct PerformanceResult {
    pub operation_name: String,
    pub duration: std::time::Duration,
}

impl PerformanceResult {
    pub fn assert_under(&self, max_duration: std::time::Duration) {
        assert!(
            self.duration <= max_duration,
            "Operation '{}' took {:?}, expected under {:?}",
            self.operation_name,
            self.duration,
            max_duration
        );
    }

    pub fn operations_per_second(&self, operation_count: usize) -> f64 {
        operation_count as f64 / self.duration.as_secs_f64()
    }
}

/// Concurrent testing utilities
pub struct ConcurrentTester;

impl ConcurrentTester {
    /// Run multiple async operations concurrently and collect results
    /// Returns all results (both successes and failures)
    pub async fn run_concurrent<F, Fut, T, E>(operations: Vec<F>) -> Result<Vec<Result<T, E>>>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<T, E>> + Send + 'static,
        T: Send + 'static,
        E: Send + 'static,
    {
        let handles: Vec<_> = operations
            .into_iter()
            .map(|op| tokio::spawn(op()))
            .collect();

        let mut results = Vec::new();
        for handle in handles {
            let result = handle.await?;
            results.push(result);
        }

        Ok(results)
    }

    /// Run the same operation multiple times concurrently
    /// Returns all results (both successes and failures)
    pub async fn run_parallel<F, Fut, T, E>(operation: F, count: usize) -> Result<Vec<Result<T, E>>>
    where
        F: Fn(usize) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<T, E>> + Send + 'static,
        T: Send + 'static,
        E: Send + 'static,
    {
        let operation = Arc::new(operation);
        let handles: Vec<_> = (0..count)
            .map(|i| {
                let op = Arc::clone(&operation);
                tokio::spawn(async move { op(i).await })
            })
            .collect();

        let mut results = Vec::new();
        for handle in handles {
            let result = handle.await?;
            results.push(result);
        }

        Ok(results)
    }

    /// Run operations concurrently and only return successful results
    pub async fn run_concurrent_success_only<F, Fut, T, E>(operations: Vec<F>) -> Result<Vec<T>>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<T, E>> + Send + 'static,
        T: Send + 'static,
        E: std::error::Error + Send + Sync + 'static,
    {
        let handles: Vec<_> = operations
            .into_iter()
            .map(|op| tokio::spawn(op()))
            .collect();

        let mut results = Vec::new();
        for handle in handles {
            let result = handle.await?;
            results.push(result.map_err(|e| anyhow::Error::new(e))?);
        }

        Ok(results)
    }

    /// Run operations in parallel and only return successful results
    pub async fn run_parallel_success_only<F, Fut, T, E>(
        operation: F,
        count: usize,
    ) -> Result<Vec<T>>
    where
        F: Fn(usize) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<T, E>> + Send + 'static,
        T: Send + 'static,
        E: std::error::Error + Send + Sync + 'static,
    {
        let operation = Arc::new(operation);
        let handles: Vec<_> = (0..count)
            .map(|i| {
                let op = Arc::clone(&operation);
                tokio::spawn(async move { op(i).await })
            })
            .collect();

        let mut results = Vec::new();
        for handle in handles {
            let result = handle.await?;
            results.push(result.map_err(|e| anyhow::Error::new(e))?);
        }

        Ok(results)
    }
}

/// Configuration helpers for tests
pub struct TestConfigBuilder {
    config: Config,
}

impl TestConfigBuilder {
    pub fn new() -> Self {
        Self {
            config: Config::default(),
        }
    }

    pub fn with_database_url(mut self, url: &str) -> Self {
        self.config.database_url = url.to_string();
        self
    }

    pub fn with_ollama(mut self, base_url: &str, model: &str) -> Self {
        self.config.embedding.provider = "ollama".to_string();
        self.config.embedding.base_url = base_url.to_string();
        self.config.embedding.model = model.to_string();
        self.config.embedding.api_key = String::new();
        self
    }

    pub fn with_openai(mut self, api_key: &str, model: &str) -> Self {
        self.config.embedding.provider = "openai".to_string();
        self.config.embedding.api_key = api_key.to_string();
        self.config.embedding.model = model.to_string();
        self.config.embedding.base_url = "https://api.openai.com".to_string();
        self
    }

    pub fn with_mock_embedder(mut self) -> Self {
        self.config.embedding.provider = "mock".to_string();
        self.config.embedding.model = "mock-model".to_string();
        self.config.embedding.base_url = "http://mock:11434".to_string();
        self.config.embedding.api_key = String::new();
        self
    }

    pub fn with_tier_limits(mut self, working: usize, warm: usize) -> Self {
        self.config.tier_config.working_tier_limit = working;
        self.config.tier_config.warm_tier_limit = warm;
        self
    }

    pub fn build(self) -> Config {
        self.config
    }
}

impl Default for TestConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}
