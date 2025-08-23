//! Basic End-to-End CRUD Test
//!
//! This test verifies basic database operations and Ollama integration
//! without requiring pgvector extension, focusing on core functionality.

use anyhow::Result;
use codex_memory::{Config, SimpleEmbedder};
use serde_json::json;
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use tracing_test::traced_test;
use uuid::Uuid;

#[tokio::test]
#[traced_test]
async fn test_database_basic_operations() -> Result<()> {
    println!("🧪 Starting Basic Database Operations Test");

    // Load configuration from environment
    let config = Config::from_env().unwrap_or_else(|_| Config::default());

    // Connect to database
    let pool = PgPool::connect(&config.database_url).await?;

    // Test 1: Basic table operations (without vector columns)
    println!("1. Testing basic table operations...");

    // Create a simple test table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS test_memories (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            content TEXT NOT NULL,
            content_hash VARCHAR(64) NOT NULL,
            metadata JSONB NOT NULL DEFAULT '{}',
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )
    "#,
    )
    .execute(&pool)
    .await?;

    // Insert test data
    let test_content = "This is a test memory content";
    let test_hash = format!("{:x}", md5::compute(test_content));
    let test_metadata = json!({"test": true, "source": "e2e_test"});

    let inserted_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO test_memories (content, content_hash, metadata)
        VALUES ($1, $2, $3)
        RETURNING id
    "#,
    )
    .bind(test_content)
    .bind(&test_hash)
    .bind(&test_metadata)
    .fetch_one(&pool)
    .await?;

    println!("✅ Successfully inserted memory with ID: {inserted_id}");

    // Test 2: Query operations
    println!("2. Testing query operations...");

    let retrieved_row = sqlx::query(
        r#"
        SELECT id, content, content_hash, metadata, created_at
        FROM test_memories
        WHERE id = $1
    "#,
    )
    .bind(inserted_id)
    .fetch_one(&pool)
    .await?;

    let retrieved_content: String = retrieved_row.get("content");
    let retrieved_hash: String = retrieved_row.get("content_hash");
    let retrieved_metadata: serde_json::Value = retrieved_row.get("metadata");

    assert_eq!(retrieved_content, test_content);
    assert_eq!(retrieved_hash, test_hash);
    assert_eq!(retrieved_metadata["test"], json!(true));

    println!("✅ Successfully retrieved and validated memory");

    // Test 3: Update operations
    println!("3. Testing update operations...");

    let updated_content = "This is updated test memory content";
    let updated_hash = format!("{:x}", md5::compute(updated_content));

    sqlx::query(
        r#"
        UPDATE test_memories
        SET content = $1, content_hash = $2, updated_at = NOW()
        WHERE id = $3
    "#,
    )
    .bind(updated_content)
    .bind(&updated_hash)
    .bind(inserted_id)
    .execute(&pool)
    .await?;

    let updated_row = sqlx::query(
        r#"
        SELECT content, content_hash FROM test_memories WHERE id = $1
    "#,
    )
    .bind(inserted_id)
    .fetch_one(&pool)
    .await?;

    let final_content: String = updated_row.get("content");
    let final_hash: String = updated_row.get("content_hash");

    assert_eq!(final_content, updated_content);
    assert_eq!(final_hash, updated_hash);

    println!("✅ Successfully updated memory");

    // Test 4: Batch operations
    println!("4. Testing batch operations...");

    let batch_data = vec![
        ("First batch memory", json!({"batch": 1})),
        ("Second batch memory", json!({"batch": 2})),
        ("Third batch memory", json!({"batch": 3})),
    ];

    let mut inserted_ids = Vec::new();
    for (content, metadata) in &batch_data {
        let hash = format!("{:x}", md5::compute(content));
        let id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO test_memories (content, content_hash, metadata)
            VALUES ($1, $2, $3)
            RETURNING id
        "#,
        )
        .bind(content)
        .bind(&hash)
        .bind(metadata)
        .fetch_one(&pool)
        .await?;

        inserted_ids.push(id);
    }

    // Query batch data
    let batch_rows = sqlx::query(
        r#"
        SELECT id, content, metadata
        FROM test_memories
        WHERE id = ANY($1)
        ORDER BY (metadata->>'batch')::int
    "#,
    )
    .bind(&inserted_ids)
    .fetch_all(&pool)
    .await?;

    assert_eq!(batch_rows.len(), 3);
    for (i, row) in batch_rows.iter().enumerate() {
        let content: String = row.get("content");
        let metadata: serde_json::Value = row.get("metadata");

        assert_eq!(content, batch_data[i].0);
        assert_eq!(metadata["batch"], json!(i + 1));
    }

    println!("✅ Successfully processed batch operations");

    // Test 5: Search and filtering
    println!("5. Testing search and filtering...");

    let search_results = sqlx::query(
        r#"
        SELECT id, content, metadata
        FROM test_memories
        WHERE content ILIKE $1
        AND metadata->>'test' IS NOT NULL
        ORDER BY created_at
    "#,
    )
    .bind("%test%")
    .fetch_all(&pool)
    .await?;

    assert!(!search_results.is_empty());
    println!(
        "✅ Successfully performed search operations (found {} results)",
        search_results.len()
    );

    // Cleanup
    println!("6. Cleaning up test data...");
    let deleted_count = sqlx::query(
        r#"
        DELETE FROM test_memories
        WHERE metadata->>'source' = 'e2e_test' OR metadata->>'batch' IS NOT NULL
    "#,
    )
    .execute(&pool)
    .await?
    .rows_affected();

    println!("✅ Successfully cleaned up {deleted_count} test records");

    // Drop test table
    sqlx::query("DROP TABLE IF EXISTS test_memories")
        .execute(&pool)
        .await?;

    pool.close().await;

    println!("🎉 Basic Database Operations Test completed successfully!");
    Ok(())
}

#[tokio::test]
#[traced_test]
async fn test_ollama_embedding_integration() -> Result<()> {
    println!("🧪 Starting Ollama Embedding Integration Test");

    // Load configuration from environment
    let config = Config::from_env().unwrap_or_else(|_| Config::default());

    // Test 1: Create embedder from configuration
    println!("1. Creating embedder from configuration...");
    let embedder = SimpleEmbedder::new_ollama(
        config.embedding.base_url.clone(),
        config.embedding.model.clone(),
    );

    println!("✅ Successfully created Ollama embedder");
    println!("  - Provider: {:?}", embedder.provider());
    println!("  - Dimensions: {}", embedder.embedding_dimension());

    // Test 2: Single embedding generation
    println!("2. Testing single embedding generation...");
    let test_content = "This is a test for embedding generation with the Agentic Memory System.";

    let start_time = std::time::Instant::now();
    let embedding = embedder.generate_embedding(test_content).await?;
    let generation_time = start_time.elapsed();

    assert_eq!(
        embedding.len(),
        768,
        "Expected 768-dimensional embedding for nomic-embed-text"
    );
    assert!(!embedding.is_empty(), "Embedding should not be empty");

    // Verify embedding is roughly normalized (cosine similarity compatible)
    let magnitude: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
    assert!(
        magnitude > 0.5,
        "Embedding magnitude should be reasonable: {magnitude}"
    );

    println!("✅ Successfully generated single embedding");
    println!("  - Dimensions: {}", embedding.len());
    println!("  - Magnitude: {magnitude:.4}");
    println!("  - Generation time: {generation_time:?}");

    // Test 3: Batch embedding generation
    println!("3. Testing batch embedding generation...");

    let batch_content = vec![
        "First memory: User preferences for dark mode".to_string(),
        "Second memory: API usage patterns and rate limits".to_string(),
        "Third memory: Database connection configuration".to_string(),
        "Fourth memory: Embedding model performance metrics".to_string(),
    ];

    let batch_start = std::time::Instant::now();
    let batch_embeddings = embedder.generate_embeddings_batch(&batch_content).await?;
    let batch_time = batch_start.elapsed();

    assert_eq!(batch_embeddings.len(), 4, "Should generate 4 embeddings");

    for (i, embedding) in batch_embeddings.iter().enumerate() {
        assert_eq!(
            embedding.len(),
            768,
            "All embeddings should have same dimensions"
        );
        let magnitude: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(
            magnitude > 0.5,
            "Embedding {i} magnitude should be reasonable: {magnitude}"
        );
    }

    // Verify embeddings are different for different content
    assert_ne!(
        batch_embeddings[0], batch_embeddings[1],
        "Different content should produce different embeddings"
    );
    assert_ne!(
        batch_embeddings[1], batch_embeddings[2],
        "Different content should produce different embeddings"
    );

    let embeddings_per_second = 4.0 / batch_time.as_secs_f64();

    println!("✅ Successfully generated batch embeddings");
    println!("  - Batch size: {}", batch_embeddings.len());
    println!("  - Batch time: {batch_time:?}");
    println!(
        "  - Throughput: {embeddings_per_second:.1} embeddings/second"
    );

    // Test 4: Determinism check
    println!("4. Testing embedding determinism...");

    let embedding1 = embedder.generate_embedding(test_content).await?;
    let embedding2 = embedder.generate_embedding(test_content).await?;

    // For Ollama, embeddings might have slight variations due to model implementation
    // So we check that they're very similar rather than identical
    let similarity = cosine_similarity(&embedding1, &embedding2);
    assert!(
        similarity > 0.99,
        "Same content should produce very similar embeddings: {similarity:.4}"
    );

    println!("✅ Embedding determinism verified");
    println!("  - Cosine similarity: {similarity:.6}");

    // Test 5: Performance characteristics
    println!("5. Testing performance characteristics...");

    let long_text = "Very long text content. ".repeat(100);
    let performance_tests = vec![
        ("Short text", "Quick test"),
        (
            "Medium text",
            "This is a medium-length text for testing embedding generation performance.",
        ),
        ("Long text", &long_text),
    ];

    let mut performance_results = HashMap::new();

    for (name, content) in &performance_tests {
        let start = std::time::Instant::now();
        let _embedding = embedder.generate_embedding(content).await?;
        let duration = start.elapsed();

        performance_results.insert(name, duration);
        println!("  - {name}: {duration:?}");
    }

    // All should complete in reasonable time (less than 10 seconds for even long text)
    for (name, duration) in &performance_results {
        assert!(
            duration.as_secs() < 10,
            "{name} took too long: {duration:?}"
        );
    }

    println!("✅ Performance characteristics within acceptable ranges");

    println!("🎉 Ollama Embedding Integration Test completed successfully!");
    Ok(())
}

#[tokio::test]
#[traced_test]
async fn test_comprehensive_e2e_flow() -> Result<()> {
    println!("🚀 Starting Comprehensive End-to-End Flow Test");

    // Load configuration from environment
    let config = Config::from_env().unwrap_or_else(|_| Config::default());

    // Step 1: Initialize services
    println!("Step 1: Initializing services...");

    let pool = PgPool::connect(&config.database_url).await?;
    let embedder = SimpleEmbedder::new_ollama(
        config.embedding.base_url.clone(),
        config.embedding.model.clone(),
    );

    println!("✅ Services initialized successfully");

    // Step 2: Create schema for e2e test (clean slate)
    println!("Step 2: Creating test schema...");

    // Drop table if it exists to ensure clean state
    sqlx::query("DROP TABLE IF EXISTS e2e_test_memories")
        .execute(&pool)
        .await?;

    sqlx::query(
        r#"
        CREATE TABLE e2e_test_memories (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            content TEXT NOT NULL,
            content_hash VARCHAR(64) NOT NULL,
            embedding_json TEXT, -- Store embedding as JSON for now
            metadata JSONB NOT NULL DEFAULT '{}',
            tier VARCHAR(20) NOT NULL DEFAULT 'working',
            importance_score FLOAT NOT NULL DEFAULT 0.5,
            access_count INTEGER NOT NULL DEFAULT 0,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )
    "#,
    )
    .execute(&pool)
    .await?;

    println!("✅ Test schema created");

    // Step 3: Memory creation with embeddings
    println!("Step 3: Creating memories with embeddings...");

    let test_memories = vec![
        (
            "User prefers dark mode interface",
            json!({"type": "preference", "category": "ui"}),
        ),
        (
            "Database connection uses PostgreSQL at 192.168.1.104",
            json!({"type": "config", "category": "database"}),
        ),
        (
            "Embedding model is nomic-embed-text running on Ollama",
            json!({"type": "config", "category": "ai"}),
        ),
        (
            "System supports three memory tiers: working, warm, cold",
            json!({"type": "architecture", "category": "memory"}),
        ),
    ];

    let mut memory_ids = Vec::new();

    for (content, metadata) in &test_memories {
        // Generate embedding
        let embedding = embedder.generate_embedding(content).await?;
        let embedding_json = serde_json::to_string(&embedding)?;

        // Store memory with embedding
        let content_hash = format!("{:x}", md5::compute(content));
        let memory_id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO e2e_test_memories (content, content_hash, embedding_json, metadata)
            VALUES ($1, $2, $3, $4)
            RETURNING id
        "#,
        )
        .bind(content)
        .bind(&content_hash)
        .bind(&embedding_json)
        .bind(metadata)
        .fetch_one(&pool)
        .await?;

        memory_ids.push(memory_id);
    }

    println!("✅ Created {} memories with embeddings", memory_ids.len());

    // Step 4: Memory retrieval and validation
    println!("Step 4: Testing memory retrieval...");

    for memory_id in &memory_ids {
        let row = sqlx::query(
            r#"
            SELECT content, embedding_json, metadata
            FROM e2e_test_memories
            WHERE id = $1
        "#,
        )
        .bind(memory_id)
        .fetch_one(&pool)
        .await?;

        let content: String = row.get("content");
        let embedding_json: String = row.get("embedding_json");
        let metadata: serde_json::Value = row.get("metadata");

        // Validate embedding
        let embedding: Vec<f32> = serde_json::from_str(&embedding_json)?;
        assert_eq!(
            embedding.len(),
            768,
            "Stored embedding should maintain dimensions"
        );

        // Validate content integrity
        assert!(!content.is_empty(), "Content should not be empty");
        assert!(metadata.is_object(), "Metadata should be a JSON object");
    }

    println!("✅ All memories retrieved and validated");

    // Step 5: Semantic similarity testing
    println!("Step 5: Testing semantic similarity...");

    let query_content = "What is the database configuration?";
    let query_embedding = embedder.generate_embedding(query_content).await?;

    // Retrieve all memories with their embeddings for similarity comparison
    let memory_rows = sqlx::query(
        r#"
        SELECT id, content, embedding_json, metadata
        FROM e2e_test_memories
        ORDER BY created_at
    "#,
    )
    .fetch_all(&pool)
    .await?;

    let mut similarities = Vec::new();

    for row in &memory_rows {
        let content: String = row.get("content");
        let embedding_json: String = row.get("embedding_json");
        let stored_embedding: Vec<f32> = serde_json::from_str(&embedding_json)?;

        let similarity = cosine_similarity(&query_embedding, &stored_embedding);
        similarities.push((content.clone(), similarity));
    }

    // Sort by similarity (highest first)
    similarities.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    println!("Similarity rankings for query: '{query_content}'");
    for (i, (content, similarity)) in similarities.iter().enumerate() {
        println!("  {}. {:.4}: {}", i + 1, similarity, content);
    }

    // The database-related memory should have highest similarity
    assert!(
        similarities[0].1 > 0.5,
        "Most similar memory should have reasonable similarity score: {:.4}",
        similarities[0].1
    );
    assert!(
        similarities[0].0.contains("Database") || similarities[0].0.contains("PostgreSQL"),
        "Most similar memory should be database-related: {}",
        similarities[0].0
    );

    println!("✅ Semantic similarity working correctly");

    // Step 6: Memory tier simulation
    println!("Step 6: Testing memory tier operations...");

    // Simulate access patterns and tier changes
    for memory_id in &memory_ids[0..2] {
        sqlx::query(
            r#"
            UPDATE e2e_test_memories
            SET access_count = access_count + 1,
                importance_score = 0.8,
                tier = 'warm'
            WHERE id = $1
        "#,
        )
        .bind(memory_id)
        .execute(&pool)
        .await?;
    }

    // Query memories by tier
    let working_memories = sqlx::query(
        r#"
        SELECT COUNT(*) as count FROM e2e_test_memories WHERE tier = 'working'
    "#,
    )
    .fetch_one(&pool)
    .await?;

    let warm_memories = sqlx::query(
        r#"
        SELECT COUNT(*) as count FROM e2e_test_memories WHERE tier = 'warm'
    "#,
    )
    .fetch_one(&pool)
    .await?;

    let working_count: i64 = working_memories.get("count");
    let warm_count: i64 = warm_memories.get("count");

    assert_eq!(working_count, 2, "Should have 2 memories in working tier");
    assert_eq!(warm_count, 2, "Should have 2 memories in warm tier");

    println!("✅ Memory tier operations working correctly");

    // Step 7: Performance validation
    println!("Step 7: Validating performance characteristics...");

    let performance_start = std::time::Instant::now();

    // Batch process multiple operations
    let batch_queries = vec![
        ("preferences", "metadata->>'category' = 'ui'"),
        ("config", "metadata->>'category' IN ('database', 'ai')"),
        ("architecture", "metadata->>'type' = 'architecture'"),
    ];

    for (name, where_clause) in &batch_queries {
        let query = format!(
            "SELECT id, content FROM e2e_test_memories WHERE {where_clause}"
        );

        let results = sqlx::query(&query).fetch_all(&pool).await?;

        println!("  - Query '{}': {} results", name, results.len());
    }

    let performance_time = performance_start.elapsed();
    assert!(
        performance_time.as_millis() < 1000,
        "Batch queries should complete quickly"
    );

    println!(
        "✅ Performance characteristics acceptable ({performance_time:?})"
    );

    // Step 8: Cleanup
    println!("Step 8: Cleaning up test data...");

    let deleted_count = sqlx::query(
        r#"
        DELETE FROM e2e_test_memories
    "#,
    )
    .execute(&pool)
    .await?
    .rows_affected();

    sqlx::query("DROP TABLE IF EXISTS e2e_test_memories")
        .execute(&pool)
        .await?;

    println!("✅ Cleaned up {deleted_count} test records");

    pool.close().await;

    println!("🎉 Comprehensive End-to-End Flow Test completed successfully!");
    println!("\n📊 Test Summary:");
    println!("  ✅ Database connectivity and operations");
    println!("  ✅ Ollama embedding generation");
    println!("  ✅ Memory storage with embeddings");
    println!("  ✅ Semantic similarity calculations");
    println!("  ✅ Memory tier operations");
    println!("  ✅ Query performance");
    println!("  ✅ Complete cleanup");

    println!("\n🚀 System Status: READY");
    println!("  - Database: PostgreSQL 17 connected");
    println!("  - Embeddings: Ollama nomic-embed-text working");
    println!("  - Performance: Within acceptable ranges");
    println!("  - Integration: Full e2e flow validated");

    Ok(())
}

/// Helper function to calculate cosine similarity between two vectors
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "Vectors must have same length");

    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let magnitude_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let magnitude_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if magnitude_a == 0.0 || magnitude_b == 0.0 {
        return 0.0;
    }

    dot_product / (magnitude_a * magnitude_b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity() {
        let vec1 = vec![1.0, 0.0, 0.0];
        let vec2 = vec![1.0, 0.0, 0.0];
        let vec3 = vec![0.0, 1.0, 0.0];

        assert_eq!(cosine_similarity(&vec1, &vec2), 1.0);
        assert_eq!(cosine_similarity(&vec1, &vec3), 0.0);

        let vec4 = vec![1.0, 1.0];
        let vec5 = vec![1.0, 1.0];
        assert!((cosine_similarity(&vec4, &vec5) - 1.0).abs() < 0.0001);
    }
}
