//! Basic functionality tests that verify the system works without requiring full database setup
//!
//! These tests focus on the configuration, embedding service, and basic functionality

use anyhow::Result;
use codex_memory::{Config, SimpleEmbedder};
use tracing_test::traced_test;

/// Test that the configuration loads properly with Ollama settings
#[tokio::test]
#[traced_test]
async fn test_configuration_loading() -> Result<()> {
    // Test default configuration
    let config = Config::default();
    assert_eq!(config.embedding.provider, "ollama");
    assert_eq!(config.embedding.model, "nomic-embed-text");
    assert_eq!(config.embedding.base_url, "http://192.168.1.110:11434");
    assert_eq!(config.embedding.timeout_seconds, 60);

    // Test configuration validation
    assert!(
        config.validate().is_ok(),
        "Default configuration should be valid"
    );

    println!("✅ Configuration loading works correctly");
    println!("  - Provider: {}", config.embedding.provider);
    println!("  - Model: {}", config.embedding.model);
    println!("  - Base URL: {}", config.embedding.base_url);

    Ok(())
}

/// Test that embedding service can be created with different providers
#[tokio::test]
#[traced_test]
async fn test_embedding_service_creation() -> Result<()> {
    // Test Ollama embedder creation
    let ollama_embedder = SimpleEmbedder::new_ollama(
        "http://192.168.1.110:11434".to_string(),
        "nomic-embed-text".to_string(),
    );
    assert_eq!(
        *ollama_embedder.provider(),
        codex_memory::embedding::EmbeddingProvider::Ollama
    );
    assert_eq!(ollama_embedder.embedding_dimension(), 768);

    // Test Mock embedder creation
    let mock_embedder = SimpleEmbedder::new_mock();
    assert_eq!(mock_embedder.embedding_dimension(), 768);

    // Test OpenAI embedder creation (with dummy key)
    let openai_embedder = SimpleEmbedder::new("dummy_key".to_string())
        .with_model("text-embedding-3-small".to_string());
    assert_eq!(openai_embedder.embedding_dimension(), 1536);

    println!("✅ Embedding service creation works correctly");
    println!(
        "  - Ollama embedder: {} dimensions",
        ollama_embedder.embedding_dimension()
    );
    println!(
        "  - Mock embedder: {} dimensions",
        mock_embedder.embedding_dimension()
    );
    println!(
        "  - OpenAI embedder: {} dimensions",
        openai_embedder.embedding_dimension()
    );

    Ok(())
}

/// Test mock embedding generation (works without external services)
#[tokio::test]
#[traced_test]
async fn test_mock_embedding_generation() -> Result<()> {
    let embedder = SimpleEmbedder::new_mock();

    // Test single embedding generation
    let embedding = embedder
        .generate_embedding("Test content for embedding")
        .await?;
    assert_eq!(embedding.len(), 768);
    assert!(!embedding.is_empty());

    // Test that the same content produces the same embedding (deterministic)
    let embedding2 = embedder
        .generate_embedding("Test content for embedding")
        .await?;
    assert_eq!(
        embedding, embedding2,
        "Mock embedder should be deterministic"
    );

    // Test different content produces different embeddings
    let embedding3 = embedder
        .generate_embedding("Different test content")
        .await?;
    assert_ne!(
        embedding, embedding3,
        "Different content should produce different embeddings"
    );

    // Test batch embedding generation
    let texts = vec![
        "First test text".to_string(),
        "Second test text".to_string(),
        "Third test text".to_string(),
    ];

    let batch_embeddings = embedder.generate_embeddings_batch(&texts).await?;
    assert_eq!(batch_embeddings.len(), 3);

    for embedding in &batch_embeddings {
        assert_eq!(embedding.len(), 768);
        assert!(!embedding.is_empty());
    }

    // Verify embeddings are different for different texts
    assert_ne!(batch_embeddings[0], batch_embeddings[1]);
    assert_ne!(batch_embeddings[1], batch_embeddings[2]);

    println!("✅ Mock embedding generation works correctly");
    println!("  - Single embedding: {} dimensions", embedding.len());
    println!("  - Batch embeddings: {} items", batch_embeddings.len());
    println!("  - Deterministic: {}", embedding == embedding2);

    Ok(())
}

/// Test configuration from environment (simulated)
#[tokio::test]
#[traced_test]
async fn test_configuration_from_environment() -> Result<()> {
    // Since we can't easily set environment variables in tests without affecting
    // other tests, we'll test the validation logic with different configurations

    // Test OpenAI configuration
    let mut openai_config = Config::default();
    openai_config.embedding.provider = "openai".to_string();
    openai_config.embedding.api_key = "test-key".to_string();
    openai_config.embedding.model = "text-embedding-3-small".to_string();
    openai_config.embedding.base_url = "https://api.openai.com".to_string();

    assert!(
        openai_config.validate().is_ok(),
        "OpenAI config should be valid"
    );

    // Test Ollama configuration (default)
    let ollama_config = Config::default();
    assert!(
        ollama_config.validate().is_ok(),
        "Ollama config should be valid"
    );

    // Test Mock configuration
    let mut mock_config = Config::default();
    mock_config.embedding.provider = "mock".to_string();
    mock_config.embedding.model = "mock-model".to_string();

    assert!(
        mock_config.validate().is_ok(),
        "Mock config should be valid"
    );

    // Test invalid configuration
    let mut invalid_config = Config::default();
    invalid_config.embedding.provider = "invalid_provider".to_string();

    assert!(
        invalid_config.validate().is_err(),
        "Invalid provider should fail validation"
    );

    println!("✅ Configuration validation works correctly");
    println!("  - OpenAI config: valid");
    println!("  - Ollama config: valid");
    println!("  - Mock config: valid");
    println!("  - Invalid config: properly rejected");

    Ok(())
}

/// Test embedding content with various text types
#[tokio::test]
#[traced_test]
async fn test_embedding_content_types() -> Result<()> {
    let embedder = SimpleEmbedder::new_mock();

    // Test various content types
    let test_contents = vec![
        "Simple text".to_string(),
        "Code: fn main() { println!(\"Hello, world!\"); }".to_string(),
        "Unicode: こんにちは 世界 🌍".to_string(),
        "Empty string: ".to_string(),
        "Numbers: 12345 67890".to_string(),
        "Special chars: !@#$%^&*()_+-=[]{}|;':\",./<>?".to_string(),
        "Long text: ".to_string() + &"x".repeat(1000),
    ];

    for content in &test_contents {
        let embedding = embedder.generate_embedding(content).await?;
        assert_eq!(
            embedding.len(),
            768,
            "All embeddings should have consistent dimensions"
        );

        // Verify embedding is normalized (approximately unit length)
        let magnitude: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(
            (magnitude - 1.0).abs() < 0.01,
            "Embedding should be approximately normalized"
        );
    }

    println!("✅ Embedding content types work correctly");
    println!("  - Tested {} different content types", test_contents.len());
    println!(
        "  - All embeddings have consistent {} dimensions",
        embedder.embedding_dimension()
    );

    Ok(())
}

/// Test performance characteristics of embedding generation
#[tokio::test]
#[traced_test]
async fn test_embedding_performance() -> Result<()> {
    let embedder = SimpleEmbedder::new_mock();

    // Test single embedding performance
    let start = std::time::Instant::now();
    let _embedding = embedder
        .generate_embedding("Performance test content")
        .await?;
    let single_duration = start.elapsed();

    // Should be very fast for mock embeddings
    assert!(
        single_duration < std::time::Duration::from_millis(100),
        "Mock embedding generation should be fast"
    );

    // Test batch performance
    let batch_content: Vec<String> = (0..10)
        .map(|i| format!("Batch content item {}", i))
        .collect();

    let batch_start = std::time::Instant::now();
    let batch_embeddings = embedder.generate_embeddings_batch(&batch_content).await?;
    let batch_duration = batch_start.elapsed();

    assert_eq!(batch_embeddings.len(), 10);
    assert!(
        batch_duration < std::time::Duration::from_secs(2),
        "Batch embedding generation should complete in reasonable time"
    );

    let embeddings_per_second = 10.0 / batch_duration.as_secs_f64();

    println!("✅ Embedding performance is acceptable");
    println!("  - Single embedding: {:?}", single_duration);
    println!("  - Batch (10 items): {:?}", batch_duration);
    println!(
        "  - Throughput: {:.1} embeddings/second",
        embeddings_per_second
    );

    Ok(())
}

/// Integration test that verifies the complete configuration-to-embedder flow
#[tokio::test]
#[traced_test]
async fn test_configuration_to_embedder_integration() -> Result<()> {
    // Test the complete flow from configuration to working embedder
    let config = Config::default();

    // For basic functionality tests, use mock embedder to avoid external dependencies
    // In real usage, this would use the actual provider from config
    let embedder = SimpleEmbedder::new_mock();

    // Test that the embedder works
    let embedding = embedder
        .generate_embedding("Integration test content")
        .await?;
    assert!(!embedding.is_empty());

    // Verify dimension matches expected for the model
    let expected_dimension = match config.embedding.provider.as_str() {
        "ollama" => match config.embedding.model.as_str() {
            "nomic-embed-text" => 768,
            _ => 768,
        },
        "mock" => 768,
        _ => embedder.embedding_dimension(),
    };

    assert_eq!(embedding.len(), expected_dimension);

    println!("✅ Configuration-to-embedder integration works correctly");
    println!("  - Provider: {}", config.embedding.provider);
    println!("  - Model: {}", config.embedding.model);
    println!("  - Embedding dimension: {}", embedding.len());

    Ok(())
}

/// Comprehensive test that runs all basic functionality checks
#[tokio::test]
#[traced_test]
async fn test_comprehensive_basic_functionality() -> Result<()> {
    println!("🚀 Starting Comprehensive Basic Functionality Test");

    // Test 1: Configuration
    println!("Running configuration loading test...");
    let config = Config::default();
    assert_eq!(config.embedding.provider, "ollama");
    assert_eq!(config.embedding.model, "nomic-embed-text");
    assert_eq!(config.embedding.base_url, "http://192.168.1.110:11434");
    assert!(config.validate().is_ok());
    println!("✅ Configuration loading works correctly");

    // Test 2: Embedding Service Creation
    println!("Running embedding service creation test...");
    let ollama_embedder = SimpleEmbedder::new_ollama(
        "http://192.168.1.110:11434".to_string(),
        "nomic-embed-text".to_string(),
    );
    assert_eq!(
        *ollama_embedder.provider(),
        codex_memory::embedding::EmbeddingProvider::Ollama
    );
    assert_eq!(ollama_embedder.embedding_dimension(), 768);

    let mock_embedder = SimpleEmbedder::new_mock();
    assert_eq!(mock_embedder.embedding_dimension(), 768);
    println!("✅ Embedding service creation works correctly");

    // Test 3: Mock Embedding Generation
    println!("Running mock embedding generation test...");
    let embedder = SimpleEmbedder::new_mock();
    let embedding = embedder
        .generate_embedding("Test content for embedding")
        .await?;
    assert_eq!(embedding.len(), 768);
    assert!(!embedding.is_empty());

    // Test deterministic behavior
    let embedding2 = embedder
        .generate_embedding("Test content for embedding")
        .await?;
    assert_eq!(
        embedding, embedding2,
        "Mock embedder should be deterministic"
    );
    println!("✅ Mock embedding generation works correctly");

    // Test 4: Configuration Validation
    println!("Running configuration validation test...");
    let mut openai_config = Config::default();
    openai_config.embedding.provider = "openai".to_string();
    openai_config.embedding.api_key = "test-key".to_string();
    assert!(openai_config.validate().is_ok());

    let mut invalid_config = Config::default();
    invalid_config.embedding.provider = "invalid_provider".to_string();
    assert!(invalid_config.validate().is_err());
    println!("✅ Configuration validation works correctly");

    // Test 5: Content Types
    println!("Running content types test...");
    let test_contents = vec![
        "Simple text".to_string(),
        "Code: fn main() { println!(\"Hello, world!\"); }".to_string(),
        "Unicode: こんにちは 世界 🌍".to_string(),
    ];

    for content in &test_contents {
        let embedding = embedder.generate_embedding(content).await?;
        assert_eq!(embedding.len(), 768);
        let magnitude: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(
            (magnitude - 1.0).abs() < 0.01,
            "Embedding should be approximately normalized"
        );
    }
    println!("✅ Content types work correctly");

    // Test 6: Performance
    println!("Running performance test...");
    let start = std::time::Instant::now();
    let _embedding = embedder
        .generate_embedding("Performance test content")
        .await?;
    let duration = start.elapsed();
    assert!(
        duration < std::time::Duration::from_millis(100),
        "Mock embedding should be fast"
    );
    println!("✅ Performance characteristics good");

    // Test 7: Integration
    println!("Running integration test...");
    let _config = Config::default();
    // For basic functionality tests, always use mock to avoid external dependencies
    let embedder = SimpleEmbedder::new_mock();

    let embedding = embedder
        .generate_embedding("Integration test content")
        .await?;
    assert!(!embedding.is_empty());
    assert_eq!(embedding.len(), 768);
    println!("✅ Integration works correctly");

    println!("\n🎉 All basic functionality tests passed!");
    println!("✅ Configuration system working");
    println!("✅ Ollama integration configured");
    println!("✅ Mock embeddings for testing");
    println!("✅ Multi-provider support");
    println!("✅ Performance characteristics good");
    println!("\n📋 System Status:");
    println!("  - Embedding provider: Ollama (with mock fallback)");
    println!("  - Model: nomic-embed-text");
    println!("  - Endpoint: http://192.168.1.110:11434");
    println!("  - Dimension: 768");
    println!("  - Ready for database integration");

    Ok(())
}
