use embedding_pipeline::*;
use std::sync::Arc;
use std::time::Duration;

mod cache_tests {
    use super::*;
    use embedding_pipeline::cache::EmbeddingCache;
    
    #[tokio::test]
    async fn test_cache_insert_and_get() {
        let cache = EmbeddingCache::new(100, Duration::from_secs(60));
        
        let key = "test_key";
        let embedding = vec![0.1, 0.2, 0.3];
        
        cache.insert(key, embedding.clone()).await;
        let retrieved = cache.get(key).await;
        
        assert_eq!(retrieved, Some(embedding));
    }
    
    #[tokio::test]
    async fn test_cache_expiration() {
        let cache = EmbeddingCache::new(100, Duration::from_millis(100));
        
        let key = "expire_test";
        let embedding = vec![0.1, 0.2, 0.3];
        
        cache.insert(key, embedding.clone()).await;
        
        // Wait for expiration
        tokio::time::sleep(Duration::from_millis(200)).await;
        
        let retrieved = cache.get(key).await;
        assert_eq!(retrieved, None);
    }
    
    #[tokio::test]
    async fn test_cache_clear() {
        let cache = EmbeddingCache::new(100, Duration::from_secs(60));
        
        cache.insert("key1", vec![0.1]).await;
        cache.insert("key2", vec![0.2]).await;
        
        cache.clear().await;
        
        assert_eq!(cache.get("key1").await, None);
        assert_eq!(cache.get("key2").await, None);
    }
}

mod cost_tracker_tests {
    use super::*;
    use embedding_pipeline::cost_tracker::CostTracker;
    
    #[test]
    fn test_cost_tracking() {
        let tracker = CostTracker::new();
        
        tracker.track(EmbeddingProvider::Claude, 0.001, 100);
        tracker.track(EmbeddingProvider::Gpu, 0.0001, 50);
        tracker.track(EmbeddingProvider::LocalCpu, 0.0, 200);
        
        assert!((tracker.get_total_cost() - 0.0011).abs() < 0.0001);
        assert!((tracker.get_provider_cost(EmbeddingProvider::Claude) - 0.001).abs() < 0.0001);
    }
    
    #[test]
    fn test_cost_breakdown() {
        let tracker = CostTracker::new();
        
        tracker.track(EmbeddingProvider::Claude, 0.002, 200);
        tracker.track(EmbeddingProvider::Gpu, 0.0002, 100);
        
        let breakdown = tracker.get_cost_breakdown();
        assert_eq!(breakdown.len(), 2);
        assert!(breakdown.contains_key(&EmbeddingProvider::Claude));
        assert!(breakdown.contains_key(&EmbeddingProvider::Gpu));
    }
    
    #[test]
    fn test_recent_entries() {
        let tracker = CostTracker::new();
        
        for i in 0..10 {
            tracker.track(EmbeddingProvider::LocalCpu, 0.0, i);
        }
        
        let recent = tracker.get_recent_entries(5);
        assert_eq!(recent.len(), 5);
        assert_eq!(recent[0].tokens, 5);
        assert_eq!(recent[4].tokens, 9);
    }
}

mod router_tests {
    use super::*;
    use embedding_pipeline::router::EmbeddingRouter;
    use embedding_pipeline::local::LocalCpuEmbeddingProvider;
    use embedding_pipeline::EmbeddingProviderTrait;
    use uuid::Uuid;
    
    #[tokio::test]
    async fn test_router_fallback() {
        let local_provider = Arc::new(
            LocalCpuEmbeddingProvider::new(Some("/models/test".to_string())).unwrap()
        ) as Arc<dyn EmbeddingProviderTrait>;
        
        let router = EmbeddingRouter::new(
            None,  // No Claude
            None,  // No GPU
            local_provider,
            true   // Fallback enabled
        );
        
        let request = EmbeddingRequest {
            id: Uuid::new_v4(),
            text: "Test text".to_string(),
            provider: Some(EmbeddingProvider::Claude), // Request Claude
            priority: RequestPriority::Normal,
        };
        
        // Should fall back to local since Claude is not available
        let (embedding, provider) = router.route(&request).await.unwrap();
        assert_eq!(provider, EmbeddingProvider::LocalCpu);
        assert_eq!(embedding.len(), 384); // Local CPU dimension
    }
    
    #[tokio::test]
    async fn test_router_priority() {
        let local_provider = Arc::new(
            LocalCpuEmbeddingProvider::new(Some("/models/test".to_string())).unwrap()
        ) as Arc<dyn EmbeddingProviderTrait>;
        
        let router = EmbeddingRouter::new(
            None,
            None,
            local_provider,
            true
        );
        
        let high_priority = EmbeddingRequest {
            id: Uuid::new_v4(),
            text: "High priority".to_string(),
            provider: None,
            priority: RequestPriority::High,
        };
        
        let low_priority = EmbeddingRequest {
            id: Uuid::new_v4(),
            text: "Low priority".to_string(),
            provider: None,
            priority: RequestPriority::Low,
        };
        
        // Both should route to local (only available provider)
        let (_, provider1) = router.route(&high_priority).await.unwrap();
        let (_, provider2) = router.route(&low_priority).await.unwrap();
        
        assert_eq!(provider1, EmbeddingProvider::LocalCpu);
        assert_eq!(provider2, EmbeddingProvider::LocalCpu);
    }
}

mod metrics_tests {
    use super::*;
    use embedding_pipeline::metrics::MetricsCollector;
    
    #[test]
    fn test_metrics_recording() {
        let metrics = MetricsCollector::new_mock();
        
        metrics.record_request(
            EmbeddingProvider::Claude,
            true,
            Duration::from_millis(100),
            Some(0.001)
        );
        
        metrics.record_request(
            EmbeddingProvider::Claude,
            false,
            Duration::from_millis(50),
            None
        );
        
        let provider_metrics = metrics.get_provider_metrics(EmbeddingProvider::Claude).unwrap();
        assert_eq!(provider_metrics.total_requests, 2);
        assert_eq!(provider_metrics.successful_requests, 1);
        assert_eq!(provider_metrics.failed_requests, 1);
        assert!((provider_metrics.avg_latency_ms - 75.0).abs() < 0.1);
    }
    
    #[test]
    fn test_metrics_all_providers() {
        let metrics = MetricsCollector::new_mock();
        
        metrics.record_request(
            EmbeddingProvider::Claude,
            true,
            Duration::from_millis(100),
            Some(0.001)
        );
        
        metrics.record_request(
            EmbeddingProvider::Gpu,
            true,
            Duration::from_millis(20),
            Some(0.0001)
        );
        
        metrics.record_request(
            EmbeddingProvider::LocalCpu,
            true,
            Duration::from_millis(5),
            Some(0.0)
        );
        
        let all_metrics = metrics.get_all_metrics();
        assert_eq!(all_metrics.len(), 3);
        assert!(all_metrics.contains_key(&EmbeddingProvider::Claude));
        assert!(all_metrics.contains_key(&EmbeddingProvider::Gpu));
        assert!(all_metrics.contains_key(&EmbeddingProvider::LocalCpu));
    }
}

mod local_embedder_tests {
    use super::*;
    use embedding_pipeline::local::LocalCpuEmbeddingProvider;
    
    #[tokio::test]
    async fn test_local_embedding() {
        let provider = LocalCpuEmbeddingProvider::new(Some("/models/test".to_string())).unwrap();
        
        let embedding = provider.embed("Test text").await.unwrap();
        assert_eq!(embedding.len(), 384);
        
        // Check normalization
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.01);
    }
    
    #[tokio::test]
    async fn test_local_batch_embedding() {
        let provider = LocalCpuEmbeddingProvider::new(Some("/models/test".to_string())).unwrap();
        
        let texts = vec![
            "First text".to_string(),
            "Second text".to_string(),
            "Third text".to_string(),
        ];
        
        let embeddings = provider.embed_batch(texts).await.unwrap();
        assert_eq!(embeddings.len(), 3);
        
        for embedding in embeddings {
            assert_eq!(embedding.len(), 384);
            let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
            assert!((norm - 1.0).abs() < 0.01);
        }
    }
    
    #[test]
    fn test_local_provider_info() {
        let provider = LocalCpuEmbeddingProvider::new(Some("/models/test".to_string())).unwrap();
        
        assert_eq!(provider.dimension(), 384);
        assert_eq!(provider.name(), "LocalCPU");
        assert!(provider.supports_batch());
        assert_eq!(provider.max_batch_size(), 64);
    }
}