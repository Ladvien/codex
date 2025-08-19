use embedding_pipeline::*;
use std::time::Duration;
use uuid::Uuid;

#[tokio::test]
async fn test_pipeline_initialization() {
    let config = EmbeddingConfig {
        claude_api_key: Some("test_key".to_string()),
        claude_api_url: "https://api.anthropic.com".to_string(),
        claude_rate_limit: 10,
        gpu_service_url: "/models/test".to_string(),
        gpu_batch_size: 16,
        local_model_path: Some("/models/local".to_string()),
        cache_max_size: 1000,
        cache_ttl: Duration::from_secs(300),
        fallback_enabled: true,
        metrics_enabled: false,
        cost_tracking_enabled: true,
    };
    
    let pipeline = EmbeddingPipeline::new(config).await;
    assert!(pipeline.is_ok());
}

#[tokio::test]
async fn test_embedding_request() {
    let config = EmbeddingConfig {
        claude_api_key: None,
        claude_api_url: "".to_string(),
        claude_rate_limit: 10,
        gpu_service_url: "/models/test".to_string(),
        gpu_batch_size: 16,
        local_model_path: Some("/models/local".to_string()),
        cache_max_size: 1000,
        cache_ttl: Duration::from_secs(300),
        fallback_enabled: true,
        metrics_enabled: false,
        cost_tracking_enabled: true,
    };
    
    let pipeline = EmbeddingPipeline::new(config).await.unwrap();
    
    let request = EmbeddingRequest {
        id: Uuid::new_v4(),
        text: "Hello, world!".to_string(),
        provider: None,
        priority: RequestPriority::Normal,
    };
    
    let response = pipeline.embed(request).await.unwrap();
    // Dimension depends on which provider was used (GPU: 768, Local: 384)
    assert!(response.embedding.len() == 384 || response.embedding.len() == 768);
    assert!(response.latency_ms > 0);
}

#[tokio::test]
async fn test_batch_embedding() {
    let config = EmbeddingConfig {
        claude_api_key: None,
        claude_api_url: "".to_string(),
        claude_rate_limit: 10,
        gpu_service_url: "/models/test".to_string(),
        gpu_batch_size: 16,
        local_model_path: Some("/models/local".to_string()),
        cache_max_size: 1000,
        cache_ttl: Duration::from_secs(300),
        fallback_enabled: true,
        metrics_enabled: false,
        cost_tracking_enabled: true,
    };
    
    let pipeline = EmbeddingPipeline::new(config).await.unwrap();
    
    let requests = vec![
        EmbeddingRequest {
            id: Uuid::new_v4(),
            text: "First text".to_string(),
            provider: None,
            priority: RequestPriority::Normal,
        },
        EmbeddingRequest {
            id: Uuid::new_v4(),
            text: "Second text".to_string(),
            provider: None,
            priority: RequestPriority::High,
        },
    ];
    
    let batch_request = BatchEmbeddingRequest { requests, batch_size: None };
    let batch_response = pipeline.embed_batch(batch_request).await.unwrap();
    
    assert_eq!(batch_response.responses.len(), 2);
    assert!(batch_response.total_latency_ms > 0);
}

#[tokio::test]
async fn test_cache_functionality() {
    let config = EmbeddingConfig {
        claude_api_key: None,
        claude_api_url: "".to_string(),
        claude_rate_limit: 10,
        gpu_service_url: "/models/test".to_string(),
        gpu_batch_size: 16,
        local_model_path: Some("/models/local".to_string()),
        cache_max_size: 1000,
        cache_ttl: Duration::from_secs(300),
        fallback_enabled: true,
        metrics_enabled: false,
        cost_tracking_enabled: true,
    };
    
    let pipeline = EmbeddingPipeline::new(config).await.unwrap();
    
    let text = "Cached text".to_string();
    let request1 = EmbeddingRequest {
        id: Uuid::new_v4(),
        text: text.clone(),
        provider: None,
        priority: RequestPriority::Normal,
    };
    
    let response1 = pipeline.embed(request1).await.unwrap();
    
    // Second request with same text should hit cache
    let request2 = EmbeddingRequest {
        id: Uuid::new_v4(),
        text: text.clone(),
        provider: None,
        priority: RequestPriority::Normal,
    };
    
    let response2 = pipeline.embed(request2).await.unwrap();
    
    // Cache hit should be faster
    assert!(response2.latency_ms < response1.latency_ms);
    assert_eq!(response1.embedding, response2.embedding);
}

#[tokio::test]
async fn test_pipeline_stats() {
    let config = EmbeddingConfig {
        claude_api_key: None,
        claude_api_url: "".to_string(),
        claude_rate_limit: 10,
        gpu_service_url: "/models/test".to_string(),
        gpu_batch_size: 16,
        local_model_path: Some("/models/local".to_string()),
        cache_max_size: 1000,
        cache_ttl: Duration::from_secs(300),
        fallback_enabled: true,
        metrics_enabled: false,
        cost_tracking_enabled: true,
    };
    
    let pipeline = EmbeddingPipeline::new(config).await.unwrap();
    
    // Generate some activity
    for i in 0..5 {
        let request = EmbeddingRequest {
            id: Uuid::new_v4(),
            text: format!("Test text {}", i),
            provider: None,
            priority: RequestPriority::Normal,
        };
        let _ = pipeline.embed(request).await;
    }
    
    let stats = pipeline.get_stats().await;
    assert!(stats.cache_entries > 0);
    assert!(stats.total_cost_usd >= 0.0);
}