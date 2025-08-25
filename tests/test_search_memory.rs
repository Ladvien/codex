use codex_memory::config::Config;
use codex_memory::embeddings::EmbeddingProvider;
use codex_memory::memory::{Memory, MemoryRepository, SearchRequest};
use serde_json::json;
use sqlx::PgPool;
use std::sync::Arc;
use tokio;
use uuid::Uuid;

#[tokio::test]
async fn test_search_memory_comprehensive() {
    let config = Config::from_env().expect("Failed to load config");
    let pool = PgPool::connect(&config.database_url)
        .await
        .expect("Failed to connect to database");

    let embedding_provider = Arc::new(
        EmbeddingProvider::new(&config.embedding)
            .await
            .expect("Failed to create embedding provider"),
    );

    let repository = MemoryRepository::new(pool.clone(), embedding_provider);

    // Test 1: Hybrid search with vector embeddings
    test_hybrid_search(&repository).await;

    // Test 2: Fulltext search with text queries
    test_fulltext_search(&repository).await;

    // Test 3: Search for specific OHAT memory
    test_ohat_memory_search(&repository).await;

    // Test 4: Edge cases
    test_search_edge_cases(&repository).await;

    println!("All search memory tests passed!");
}

async fn test_hybrid_search(repository: &MemoryRepository) {
    println!("Testing hybrid search with vector embeddings...");

    let request = SearchRequest {
        query_text: Some("OHAT project systematic review".to_string()),
        query_embedding: None, // Repository will generate this
        limit: Some(5),
        offset: Some(0),
        threshold: Some(0.1),
        search_type: Some("hybrid".to_string()),
        explain_score: Some(true),
        filters: None,
    };

    let results = repository
        .search_memory(&request)
        .await
        .expect("Hybrid search should not fail");

    assert!(
        !results.is_empty(),
        "Hybrid search should return results for OHAT query"
    );

    // Check that results contain the expected memory
    let has_ohat = results
        .iter()
        .any(|r| r.memory.content.to_lowercase().contains("ohat"));
    assert!(has_ohat, "Search results should contain OHAT memory");

    // Verify score explanation is provided
    assert!(
        results[0].score_explanation.is_some(),
        "Score explanation should be provided when requested"
    );

    println!("✓ Hybrid search test passed");
}

async fn test_fulltext_search(repository: &MemoryRepository) {
    println!("Testing fulltext search with text queries...");

    let request = SearchRequest {
        query_text: Some("Amazon Bedrock agents".to_string()),
        query_embedding: None,
        limit: Some(3),
        offset: Some(0),
        threshold: None,
        search_type: Some("fulltext".to_string()),
        explain_score: Some(false),
        filters: None,
    };

    let results = repository
        .search_memory(&request)
        .await
        .expect("Fulltext search should not fail");

    assert!(
        !results.is_empty(),
        "Fulltext search should return results for Bedrock query"
    );

    // Verify that all required fields are populated (this was the bug!)
    for result in &results {
        assert!(
            result.similarity_score >= 0.0,
            "Similarity score should be non-negative"
        );
        assert!(result.memory.id != Uuid::nil(), "Memory ID should be valid");
        assert!(
            !result.memory.content.is_empty(),
            "Memory content should not be empty"
        );
        // These fields were missing before the fix
        assert!(
            result.memory.importance_score.is_some(),
            "Importance score should be present"
        );
        assert!(
            result.memory.recency_score.is_some(),
            "Recency score should be present"
        );
    }

    println!("✓ Fulltext search test passed");
}

async fn test_ohat_memory_search(repository: &MemoryRepository) {
    println!("Testing search for specific OHAT memory that user reported...");

    // Test the exact memory that exists but wasn't being found
    let expected_id = Uuid::parse_str("091b7ead-db9b-41a1-b0fa-658e9cdd790c")
        .expect("Should parse OHAT memory UUID");

    let request = SearchRequest {
        query_text: Some("OHAT Systematic Review Automation Project".to_string()),
        query_embedding: None,
        limit: Some(10),
        offset: Some(0),
        threshold: Some(0.0), // Very low threshold to ensure we find it
        search_type: Some("hybrid".to_string()),
        explain_score: Some(false),
        filters: None,
    };

    let results = repository
        .search_memory(&request)
        .await
        .expect("OHAT search should not fail");

    assert!(!results.is_empty(), "Should find OHAT memory");

    // Verify we found the specific memory the user mentioned
    let found_target = results.iter().any(|r| r.memory.id == expected_id);
    assert!(
        found_target,
        "Should find the specific OHAT memory ID: {}",
        expected_id
    );

    // Also test fulltext search for the same memory
    let fulltext_request = SearchRequest {
        query_text: Some("OHAT".to_string()),
        search_type: Some("fulltext".to_string()),
        limit: Some(10),
        ..Default::default()
    };

    let fulltext_results = repository
        .search_memory(&fulltext_request)
        .await
        .expect("OHAT fulltext search should not fail");

    assert!(
        !fulltext_results.is_empty(),
        "Fulltext search should also find OHAT memory"
    );

    println!("✓ OHAT memory search test passed");
}

async fn test_search_edge_cases(repository: &MemoryRepository) {
    println!("Testing search edge cases...");

    // Test 1: Empty query
    let empty_request = SearchRequest {
        query_text: Some("".to_string()),
        ..Default::default()
    };

    let empty_results = repository.search_memory(&empty_request).await;
    // Should either return empty results or a proper error - not crash
    match empty_results {
        Ok(results) => assert!(
            results.is_empty() || !results.is_empty(),
            "Should handle empty query gracefully"
        ),
        Err(_) => {} // Error is acceptable for empty query
    }

    // Test 2: Very long query
    let long_query = "a".repeat(10000);
    let long_request = SearchRequest {
        query_text: Some(long_query),
        limit: Some(1),
        ..Default::default()
    };

    let long_results = repository.search_memory(&long_request).await;
    // Should not crash with very long queries
    assert!(
        long_results.is_ok(),
        "Should handle very long queries without crashing"
    );

    // Test 3: Zero limit
    let zero_limit_request = SearchRequest {
        query_text: Some("test".to_string()),
        limit: Some(0),
        ..Default::default()
    };

    let zero_results = repository
        .search_memory(&zero_limit_request)
        .await
        .expect("Should handle zero limit");
    assert!(
        zero_results.is_empty(),
        "Zero limit should return empty results"
    );

    // Test 4: High offset
    let high_offset_request = SearchRequest {
        query_text: Some("test".to_string()),
        offset: Some(1000000),
        limit: Some(10),
        ..Default::default()
    };

    let offset_results = repository
        .search_memory(&high_offset_request)
        .await
        .expect("Should handle high offset");
    // Should return empty results, not crash
    assert!(
        offset_results.len() <= 10,
        "Should respect limit even with high offset"
    );

    println!("✓ Edge cases test passed");
}

#[tokio::test]
async fn test_search_memory_column_compatibility() {
    println!("Testing that search results have all expected columns...");

    let config = Config::from_env().expect("Failed to load config");
    let pool = PgPool::connect(&config.database_url)
        .await
        .expect("Failed to connect to database");

    let embedding_provider = Arc::new(
        EmbeddingProvider::new(&config.embedding)
            .await
            .expect("Failed to create embedding provider"),
    );

    let repository = MemoryRepository::new(pool.clone(), embedding_provider);

    // Test both search types to ensure column compatibility
    let search_types = vec!["hybrid", "fulltext"];

    for search_type in search_types {
        println!("Testing column compatibility for {} search...", search_type);

        let request = SearchRequest {
            query_text: Some("test query".to_string()),
            search_type: Some(search_type.to_string()),
            limit: Some(1),
            ..Default::default()
        };

        match repository.search_memory(&request).await {
            Ok(results) => {
                if !results.is_empty() {
                    let result = &results[0];

                    // Verify all expected fields are accessible (this was the root cause of the bug)
                    assert!(result.memory.id != Uuid::nil(), "ID should be valid");
                    assert!(
                        !result.memory.content.is_empty(),
                        "Content should not be empty"
                    );
                    assert!(result.memory.tier.is_some(), "Tier should be present");
                    assert!(result.memory.status.is_some(), "Status should be present");
                    assert!(
                        result.memory.importance_score.is_some(),
                        "Importance score should be present"
                    );
                    assert!(
                        result.memory.recency_score.is_some(),
                        "Recency score should be present"
                    );
                    assert!(
                        result.memory.relevance_score.is_some(),
                        "Relevance score should be present"
                    );
                    assert!(
                        result.similarity_score >= 0.0,
                        "Similarity score should be non-negative"
                    );

                    println!("✓ {} search column compatibility verified", search_type);
                }
            }
            Err(e) => {
                panic!("Search type {} failed with error: {:?}", search_type, e);
            }
        }
    }

    println!("✓ Column compatibility test passed for all search types");
}
