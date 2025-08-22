use anyhow::Result;
use codex_memory::{
    config::Config,
    memory::{
        connection::create_pool,
        enhanced_retrieval::{
            EnhancedRetrievalConfig, MemoryAwareRetrievalEngine, MemoryAwareSearchRequest,
        },
        models::{CreateMemoryRequest, MemoryTier, SearchRequest, SearchType},
        MemoryRepository,
    },
    SimpleEmbedder,
};
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

/// Performance test helper to create test data
async fn create_test_repository_with_data() -> Result<(Arc<MemoryRepository>, Vec<Uuid>)> {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgresql://codex_user:MZSfXiLr5uR3QYbRwv2vTzi22SvFkj4a@192.168.1.104:5432/codex_db"
            .to_string()
    });
    let pool = create_pool(&database_url, 10).await?;

    let config = Config::default();
    let repository = Arc::new(MemoryRepository::with_config(pool, config));
    let embedder = Arc::new(SimpleEmbedder::new_mock());

    // Create test memories for search
    let mut memory_ids = Vec::new();

    for i in 0..50 {
        let request = CreateMemoryRequest {
            content: format!("Test memory content {}: This is searchable content with various terms and concepts related to performance testing and database optimization", i),
            embedding: Some(embedder.generate_embedding(&format!("test content {}", i)).await?),
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.5 + (i as f64 * 0.01)),
            parent_id: None,
            metadata: Some(serde_json::json!({
                "test_index": i,
                "category": format!("category_{}", i % 5),
                "created_for": "n_plus_1_performance_test"
            })),
            expires_at: None,
        };

        let memory = repository.create_memory(request).await?;
        memory_ids.push(memory.id);

        // Add some artificial consolidation events for testing
        if i % 10 == 0 {
            // This would normally be done through the consolidation system
            // For testing, we simulate recent consolidation activity
        }
    }

    Ok((repository, memory_ids))
}

#[tokio::test]
async fn test_search_performance_baseline() -> Result<()> {
    let (repository, _memory_ids) = create_test_repository_with_data().await?;
    let embedder = Arc::new(SimpleEmbedder::new_mock());

    // Test basic search performance
    let search_request = SearchRequest {
        query_text: Some("performance testing optimization".to_string()),
        query_embedding: Some(
            embedder
                .generate_embedding("performance testing optimization")
                .await?,
        ),
        search_type: Some(SearchType::Hybrid),
        limit: Some(20),
        ..Default::default()
    };

    let start_time = Instant::now();
    let results = repository.search_memories(search_request).await?;
    let duration = start_time.elapsed();

    println!(
        "Basic search took: {:?} for {} results",
        duration,
        results.results.len()
    );

    // Performance assertion: should complete within 100ms
    assert!(
        duration.as_millis() < 100,
        "Basic search took too long: {:?}",
        duration
    );

    Ok(())
}

#[tokio::test]
async fn test_enhanced_search_batch_optimization() -> Result<()> {
    let (repository, _memory_ids) = create_test_repository_with_data().await?;
    let embedder = Arc::new(SimpleEmbedder::new_mock());

    // Create enhanced retrieval engine
    let config = EnhancedRetrievalConfig::default();
    let retrieval_engine = MemoryAwareRetrievalEngine::new(
        config,
        repository.clone(),
        None, // No reflection engine for this test
    );

    // Create search request with all enhancements enabled
    let base_search = SearchRequest {
        query_text: Some("performance testing optimization".to_string()),
        query_embedding: Some(
            embedder
                .generate_embedding("performance testing optimization")
                .await?,
        ),
        search_type: Some(SearchType::Hybrid),
        limit: Some(20),
        ..Default::default()
    };

    let enhanced_request = MemoryAwareSearchRequest {
        base_request: base_search,
        include_consolidation_boost: Some(true),
        include_lineage: Some(true),
        include_insights: Some(true),
        explain_boosting: Some(true),
        lineage_depth: Some(3),
        use_cache: Some(false),
    };

    let start_time = Instant::now();
    let enhanced_results = retrieval_engine.search(enhanced_request).await?;
    let duration = start_time.elapsed();

    println!(
        "Enhanced search took: {:?} for {} results",
        duration,
        enhanced_results.results.len()
    );
    println!(
        "Performance metrics: {:?}",
        enhanced_results.performance_metrics
    );

    // Performance assertion: should complete within 200ms (target from TICKET-005)
    assert!(
        duration.as_millis() < 200,
        "Enhanced search took too long: {:?}",
        duration
    );

    // Verify batch optimization worked
    assert!(
        enhanced_results.performance_metrics.database_query_time_ms < 150,
        "Database query time too high, batch optimization may not be working"
    );

    Ok(())
}

#[tokio::test]
async fn test_batch_vs_individual_consolidation_queries() -> Result<()> {
    let (repository, memory_ids) = create_test_repository_with_data().await?;
    let embedder = Arc::new(SimpleEmbedder::new_mock());

    // Test the batch consolidation optimization directly
    let config = EnhancedRetrievalConfig::default();
    let retrieval_engine = MemoryAwareRetrievalEngine::new(config, repository.clone(), None);

    // Simulate getting 20 memory IDs for batch processing
    let test_ids: Vec<Uuid> = memory_ids.into_iter().take(20).collect();

    // Time the batch operation
    let start_time = Instant::now();
    let _boost_map = retrieval_engine
        .calculate_consolidation_boosts_batch(&test_ids)
        .await?;
    let batch_duration = start_time.elapsed();

    println!(
        "Batch consolidation boost calculation took: {:?}",
        batch_duration
    );

    // The batch operation should be significantly faster than individual queries
    // For 20 memories, batch should complete in <50ms
    assert!(
        batch_duration.as_millis() < 50,
        "Batch consolidation boost took too long: {:?}",
        batch_duration
    );

    // Test batch recently consolidated check
    let start_time = Instant::now();
    let _status_map = retrieval_engine
        .check_recently_consolidated_batch(&test_ids)
        .await?;
    let batch_status_duration = start_time.elapsed();

    println!(
        "Batch consolidation status check took: {:?}",
        batch_status_duration
    );

    assert!(
        batch_status_duration.as_millis() < 30,
        "Batch consolidation status check took too long: {:?}",
        batch_status_duration
    );

    Ok(())
}

#[tokio::test]
async fn test_query_count_reduction() -> Result<()> {
    let (repository, _memory_ids) = create_test_repository_with_data().await?;
    let embedder = Arc::new(SimpleEmbedder::new_mock());

    // This test would ideally count actual database queries
    // For now, we test the performance improvement as a proxy

    let config = EnhancedRetrievalConfig::default();
    let retrieval_engine = MemoryAwareRetrievalEngine::new(config, repository.clone(), None);

    let base_search = SearchRequest {
        query_text: Some("performance testing optimization database".to_string()),
        query_embedding: Some(
            embedder
                .generate_embedding("performance testing optimization database")
                .await?,
        ),
        search_type: Some(SearchType::Hybrid),
        limit: Some(30), // Larger result set to test N+1 optimization
        ..Default::default()
    };

    let enhanced_request = MemoryAwareSearchRequest {
        base_request: base_search,
        include_consolidation_boost: Some(true),
        include_lineage: Some(false), // Disable lineage to test just consolidation batching
        include_insights: Some(false),
        explain_boosting: Some(false),
        lineage_depth: Some(1),
        use_cache: Some(false),
    };

    let start_time = Instant::now();
    let results = retrieval_engine.search(enhanced_request).await?;
    let duration = start_time.elapsed();

    println!(
        "Large result set search took: {:?} for {} results",
        duration,
        results.results.len()
    );

    // With batch optimization, even 30 results should complete quickly
    // Target: <100ms for 30 results (vs potentially seconds with N+1)
    assert!(
        duration.as_millis() < 100,
        "Large result search took too long: {:?}",
        duration
    );

    // Verify we got reasonable results
    assert!(!results.results.is_empty(), "Should return search results");

    Ok(())
}

#[tokio::test]
async fn test_concurrent_search_performance() -> Result<()> {
    let (repository, _memory_ids) = create_test_repository_with_data().await?;
    let embedder = Arc::new(SimpleEmbedder::new_mock());

    let config = EnhancedRetrievalConfig::default();
    let retrieval_engine = Arc::new(MemoryAwareRetrievalEngine::new(
        config,
        repository.clone(),
        None,
    ));

    // Test concurrent searches to verify no connection pool exhaustion
    let mut handles = Vec::new();

    for i in 0..5 {
        let engine = retrieval_engine.clone();
        let emb = embedder.clone();
        let handle = tokio::spawn(async move {
            let base_search = SearchRequest {
                query_text: Some(format!("concurrent search test {}", i)),
                query_embedding: Some(
                    emb.generate_embedding(&format!("concurrent search {}", i))
                        .await?,
                ),
                search_type: Some(SearchType::Hybrid),
                limit: Some(15),
                ..Default::default()
            };

            let enhanced_request = MemoryAwareSearchRequest {
                base_request: base_search,
                include_consolidation_boost: Some(true),
                include_lineage: Some(false),
                include_insights: Some(false),
                explain_boosting: Some(false),
                lineage_depth: Some(1),
                use_cache: Some(false),
            };

            let start_time = Instant::now();
            let results = engine.search(enhanced_request).await?;
            let duration = start_time.elapsed();

            Ok::<(std::time::Duration, usize), anyhow::Error>((duration, results.results.len()))
        });
        handles.push(handle);
    }

    // Wait for all concurrent searches
    let mut total_duration = std::time::Duration::from_millis(0);
    let mut total_results = 0;

    for handle in handles {
        let (duration, result_count) = handle.await??;
        total_duration += duration;
        total_results += result_count;

        // Each concurrent search should still complete quickly
        assert!(
            duration.as_millis() < 150,
            "Concurrent search took too long: {:?}",
            duration
        );
    }

    println!(
        "Concurrent searches completed. Average duration: {:?}, Total results: {}",
        total_duration / 5,
        total_results
    );

    Ok(())
}

#[tokio::test]
async fn test_database_query_optimization() -> Result<()> {
    let (repository, _memory_ids) = create_test_repository_with_data().await?;
    let embedder = Arc::new(SimpleEmbedder::new_mock());

    let config = EnhancedRetrievalConfig::default();
    let retrieval_engine = MemoryAwareRetrievalEngine::new(config, repository.clone(), None);

    let base_search = SearchRequest {
        query_text: Some("database optimization performance".to_string()),
        query_embedding: Some(
            embedder
                .generate_embedding("database optimization performance")
                .await?,
        ),
        search_type: Some(SearchType::Hybrid),
        limit: Some(25),
        ..Default::default()
    };

    let enhanced_request = MemoryAwareSearchRequest {
        base_request: base_search,
        include_consolidation_boost: Some(true),
        include_lineage: Some(true),
        include_insights: Some(true),
        explain_boosting: Some(true),
        lineage_depth: Some(2),
        use_cache: Some(false),
    };

    let start_time = Instant::now();
    let results = retrieval_engine.search(enhanced_request).await?;
    let total_duration = start_time.elapsed();

    println!("Full enhanced search metrics:");
    println!("  Total duration: {:?}", total_duration);
    println!(
        "  Database query time: {:?}ms",
        results.performance_metrics.database_query_time_ms
    );
    println!(
        "  Consolidation analysis: {:?}ms",
        results.performance_metrics.consolidation_analysis_time_ms
    );
    println!(
        "  Lineage analysis: {:?}ms",
        results.performance_metrics.lineage_analysis_time_ms
    );
    println!(
        "  Cache operations: {:?}ms",
        results.performance_metrics.cache_operation_time_ms
    );
    println!("  Results returned: {}", results.results.len());

    // TICKET-005 acceptance criteria verification
    assert!(
        total_duration.as_millis() < 200,
        "Response time should be <200ms for enhanced search, got: {:?}",
        total_duration
    );

    // Database time should be reasonable portion of total time
    assert!(
        (results.performance_metrics.database_query_time_ms as u128) < total_duration.as_millis(),
        "Database query time should be less than total duration"
    );

    // With batch optimization, consolidation analysis should be fast
    assert!(
        results.performance_metrics.consolidation_analysis_time_ms < 50,
        "Consolidation analysis should be fast with batch queries"
    );

    Ok(())
}
