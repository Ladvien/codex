/// End-to-End Search Memory Regression Prevention Tests
///
/// These tests specifically target the search_memory functionality through the MCP server
/// to prevent regressions like the column mismatch bug that broke Claude Desktop integration.
///
/// Tests run against the actual MCP server and database to simulate real-world usage.
use codex_memory::config::Config;
use codex_memory::embeddings::EmbeddingProvider;
use codex_memory::harvester::service::HarvesterService;
use codex_memory::insights::processor::InsightProcessor;
use codex_memory::insights::scheduler::SchedulerService;
use codex_memory::insights::storage::InsightStorage;
use codex_memory::mcp_server::handlers::MCPHandlers;
use codex_memory::mcp_server::logging::MCPLogger;
use codex_memory::mcp_server::progress::ProgressTracker;
use codex_memory::memory::{CreateMemoryRequest, MemoryRepository, MemoryTier};
use serde_json::{json, Value};
use sqlx::PgPool;
use std::sync::Arc;
use tokio;
use uuid::Uuid;

/// Test the exact search_memory MCP tool that was failing in Claude Desktop
#[tokio::test]
async fn test_e2e_search_memory_mcp_tool_fulltext() {
    let config = Config::from_env().expect("Failed to load config");
    let pool = PgPool::connect(&config.database_url)
        .await
        .expect("Failed to connect to database");

    let embedding_provider = Arc::new(
        EmbeddingProvider::new(&config.embedding)
            .await
            .expect("Failed to create embedding provider"),
    );

    let repository = Arc::new(MemoryRepository::new(
        pool.clone(),
        embedding_provider.clone(),
    ));
    let insight_storage = Arc::new(InsightStorage::new(
        Arc::new(pool.clone()),
        embedding_provider.clone(),
    ));
    let insight_processor = Arc::new(InsightProcessor::new(
        repository.clone(),
        insight_storage.clone(),
        embedding_provider.clone(),
        config.clone(),
    ));
    let scheduler_service = Arc::new(SchedulerService::new(
        insight_processor.clone(),
        repository.clone(),
    ));
    let harvester_service = Arc::new(HarvesterService::new(
        repository.clone(),
        embedding_provider.clone(),
        config.clone(),
    ));
    let mcp_logger = Arc::new(MCPLogger::new());
    let progress_tracker = Arc::new(ProgressTracker::new());

    let handlers = MCPHandlers::new(
        repository.clone(),
        insight_storage,
        insight_processor,
        scheduler_service,
        embedding_provider,
        harvester_service,
        mcp_logger,
        progress_tracker,
    );

    // Create test memory to ensure we have something to search for
    let test_memory_request = CreateMemoryRequest {
        content: "E2E Test Memory for search regression prevention with unique keyword REGRESSION_TEST_E2E_SEARCH".to_string(),
        metadata: Some(json!({"test": "e2e_search", "purpose": "regression_prevention"})),
        importance_score: Some(0.8),
        tier: Some(MemoryTier::Working),
        parent_id: None,
        expires_at: None,
    };

    let test_memory = repository
        .create_memory(test_memory_request)
        .await
        .expect("Failed to create test memory");

    // Test 1: Fulltext search through MCP tool (this was the failing case)
    let search_params = json!({
        "query_text": "REGRESSION_TEST_E2E_SEARCH",
        "search_type": "fulltext",
        "limit": 5,
        "explain_score": false
    });

    let fulltext_result = handlers.search_memory(&search_params, None).await;

    // This should NOT fail with column mismatch errors
    assert!(
        fulltext_result.is_ok(),
        "Fulltext search through MCP tool should not fail: {:?}",
        fulltext_result.err()
    );

    let fulltext_response = fulltext_result.unwrap();
    assert!(
        fulltext_response.is_object(),
        "Response should be JSON object"
    );

    let results = fulltext_response
        .get("results")
        .expect("Response should have results");
    assert!(results.is_array(), "Results should be array");

    let results_array = results.as_array().unwrap();
    assert!(!results_array.is_empty(), "Should find test memory");

    // Verify the result structure matches expectations (preventing column mismatch)
    let first_result = &results_array[0];
    assert!(
        first_result.get("memory").is_some(),
        "Result should have memory object"
    );
    assert!(
        first_result.get("similarity_score").is_some(),
        "Result should have similarity_score"
    );

    let memory = first_result.get("memory").unwrap();
    assert!(memory.get("id").is_some(), "Memory should have id");
    assert!(
        memory.get("content").is_some(),
        "Memory should have content"
    );
    assert!(
        memory.get("importance_score").is_some(),
        "Memory should have importance_score"
    );
    assert!(
        memory.get("recency_score").is_some(),
        "Memory should have recency_score"
    );
    assert!(
        memory.get("relevance_score").is_some(),
        "Memory should have relevance_score"
    );

    println!("✅ E2E Fulltext search through MCP tool passed");

    // Test 2: Hybrid search through MCP tool
    let hybrid_search_params = json!({
        "query_text": "REGRESSION_TEST_E2E_SEARCH",
        "search_type": "hybrid",
        "limit": 5,
        "threshold": 0.0,
        "explain_score": true
    });

    let hybrid_result = handlers.search_memory(&hybrid_search_params, None).await;
    assert!(
        hybrid_result.is_ok(),
        "Hybrid search through MCP tool should not fail: {:?}",
        hybrid_result.err()
    );

    let hybrid_response = hybrid_result.unwrap();
    let hybrid_results = hybrid_response.get("results").unwrap().as_array().unwrap();
    assert!(
        !hybrid_results.is_empty(),
        "Hybrid search should find test memory"
    );

    // Verify score explanation is included when requested
    let first_hybrid_result = &hybrid_results[0];
    assert!(
        first_hybrid_result.get("score_explanation").is_some(),
        "Should have score explanation when requested"
    );

    println!("✅ E2E Hybrid search through MCP tool passed");

    // Test 3: Search with edge cases that might trigger column issues
    let edge_case_params = json!({
        "query_text": "",
        "search_type": "fulltext",
        "limit": 0
    });

    let edge_result = handlers.search_memory(&edge_case_params, None).await;
    // Should handle gracefully, not crash with column errors
    match edge_result {
        Ok(_) => println!("✅ Edge case handled successfully"),
        Err(e) => {
            // Acceptable to return an error for invalid input, but NOT column mismatch errors
            let error_msg = format!("{:?}", e);
            assert!(
                !error_msg.contains("column"),
                "Should not have column-related errors: {}",
                error_msg
            );
            assert!(
                !error_msg.contains("try_get"),
                "Should not have column mapping errors: {}",
                error_msg
            );
            println!(
                "✅ Edge case properly rejected with non-column error: {}",
                error_msg
            );
        }
    }

    // Cleanup test memory
    repository
        .delete_memory(test_memory.id, Some("E2E test cleanup".to_string()))
        .await
        .expect("Failed to cleanup test memory");

    println!("✅ All E2E search regression prevention tests passed");
}

/// Test MCP search_memory tool with the exact OHAT memory that was failing
#[tokio::test]
async fn test_e2e_ohat_memory_search_mcp() {
    let config = Config::from_env().expect("Failed to load config");
    let pool = PgPool::connect(&config.database_url)
        .await
        .expect("Failed to connect to database");

    let embedding_provider = Arc::new(
        EmbeddingProvider::new(&config.embedding)
            .await
            .expect("Failed to create embedding provider"),
    );

    let repository = Arc::new(MemoryRepository::new(
        pool.clone(),
        embedding_provider.clone(),
    ));
    let insight_storage = Arc::new(InsightStorage::new(
        Arc::new(pool.clone()),
        embedding_provider.clone(),
    ));
    let insight_processor = Arc::new(InsightProcessor::new(
        repository.clone(),
        insight_storage.clone(),
        embedding_provider.clone(),
        config.clone(),
    ));
    let scheduler_service = Arc::new(SchedulerService::new(
        insight_processor.clone(),
        repository.clone(),
    ));
    let harvester_service = Arc::new(HarvesterService::new(
        repository.clone(),
        embedding_provider.clone(),
        config.clone(),
    ));
    let mcp_logger = Arc::new(MCPLogger::new());
    let progress_tracker = Arc::new(ProgressTracker::new());

    let handlers = MCPHandlers::new(
        repository.clone(),
        insight_storage,
        insight_processor,
        scheduler_service,
        embedding_provider,
        harvester_service,
        mcp_logger,
        progress_tracker,
    );

    // Test searching for the specific OHAT memory that was failing
    let ohat_search_params = json!({
        "query_text": "OHAT",
        "search_type": "fulltext",
        "limit": 10
    });

    let ohat_result = handlers.search_memory(&ohat_search_params, None).await;
    assert!(
        ohat_result.is_ok(),
        "OHAT search should not fail: {:?}",
        ohat_result.err()
    );

    let ohat_response = ohat_result.unwrap();
    let results = ohat_response.get("results").unwrap().as_array().unwrap();

    // Should find OHAT memories
    let has_ohat = results.iter().any(|result| {
        result
            .get("memory")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .map(|content| content.to_lowercase().contains("ohat"))
            .unwrap_or(false)
    });

    if has_ohat {
        println!("✅ Successfully found OHAT memory through MCP search tool");

        // Verify the specific memory ID if it exists
        let target_id = "091b7ead-db9b-41a1-b0fa-658e9cdd790c";
        let found_target = results.iter().any(|result| {
            result
                .get("memory")
                .and_then(|m| m.get("id"))
                .and_then(|id| id.as_str())
                .map(|id_str| id_str == target_id)
                .unwrap_or(false)
        });

        if found_target {
            println!(
                "✅ Found the specific OHAT memory that was previously failing: {}",
                target_id
            );
        } else {
            println!(
                "ℹ️  OHAT memories found, but not the specific target ID (may have been deleted)"
            );
        }

        // Verify all returned results have proper structure (no column mismatch)
        for result in results {
            let memory = result.get("memory").expect("Result should have memory");
            assert!(memory.get("id").is_some(), "Memory should have ID");
            assert!(
                memory.get("content").is_some(),
                "Memory should have content"
            );
            assert!(memory.get("tier").is_some(), "Memory should have tier");
            assert!(memory.get("status").is_some(), "Memory should have status");

            // These were the problematic columns in the original bug
            assert!(
                memory.get("importance_score").is_some(),
                "Memory should have importance_score"
            );
            assert!(
                memory.get("recency_score").is_some(),
                "Memory should have recency_score"
            );
            assert!(
                memory.get("relevance_score").is_some(),
                "Memory should have relevance_score"
            );
        }
    } else {
        println!("⚠️  No OHAT memories found - may have been deleted from database");
    }

    println!("✅ OHAT memory search regression test passed");
}

/// Test that both search types return consistent structure to prevent future column mismatches
#[tokio::test]
async fn test_e2e_search_consistency_prevention() {
    let config = Config::from_env().expect("Failed to load config");
    let pool = PgPool::connect(&config.database_url)
        .await
        .expect("Failed to connect to database");

    let embedding_provider = Arc::new(
        EmbeddingProvider::new(&config.embedding)
            .await
            .expect("Failed to create embedding provider"),
    );

    let repository = Arc::new(MemoryRepository::new(
        pool.clone(),
        embedding_provider.clone(),
    ));
    let insight_storage = Arc::new(InsightStorage::new(
        Arc::new(pool.clone()),
        embedding_provider.clone(),
    ));
    let insight_processor = Arc::new(InsightProcessor::new(
        repository.clone(),
        insight_storage.clone(),
        embedding_provider.clone(),
        config.clone(),
    ));
    let scheduler_service = Arc::new(SchedulerService::new(
        insight_processor.clone(),
        repository.clone(),
    ));
    let harvester_service = Arc::new(HarvesterService::new(
        repository.clone(),
        embedding_provider.clone(),
        config.clone(),
    ));
    let mcp_logger = Arc::new(MCPLogger::new());
    let progress_tracker = Arc::new(ProgressTracker::new());

    let handlers = MCPHandlers::new(
        repository.clone(),
        insight_storage,
        insight_processor,
        scheduler_service,
        embedding_provider,
        harvester_service,
        mcp_logger,
        progress_tracker,
    );

    // Create test memory with all possible fields populated
    let comprehensive_memory_request = CreateMemoryRequest {
        content: "CONSISTENCY_TEST comprehensive memory with all fields for structure validation"
            .to_string(),
        metadata: Some(json!({
            "test_type": "structure_consistency",
            "all_fields": true,
            "purpose": "regression_prevention"
        })),
        importance_score: Some(0.9),
        tier: Some(MemoryTier::Working),
        parent_id: None,
        expires_at: None,
    };

    let test_memory = repository
        .create_memory(comprehensive_memory_request)
        .await
        .expect("Failed to create comprehensive test memory");

    let search_types = vec!["fulltext", "hybrid"];
    let mut results_structures = Vec::new();

    // Test both search types and ensure they return consistent structures
    for search_type in search_types {
        let search_params = json!({
            "query_text": "CONSISTENCY_TEST",
            "search_type": search_type,
            "limit": 5,
            "explain_score": true
        });

        let result = handlers.search_memory(&search_params, None).await;
        assert!(
            result.is_ok(),
            "{} search should not fail: {:?}",
            search_type,
            result.err()
        );

        let response = result.unwrap();
        let results = response.get("results").unwrap().as_array().unwrap();
        assert!(
            !results.is_empty(),
            "{} search should find test memory",
            search_type
        );

        let first_result = &results[0];
        let memory = first_result.get("memory").unwrap();

        // Extract all field names for consistency checking
        let field_names: Vec<String> = memory.as_object().unwrap().keys().cloned().collect();

        results_structures.push((search_type, field_names));

        // Verify critical fields that caused the original bug
        let critical_fields = [
            "id",
            "content",
            "tier",
            "status",
            "importance_score",
            "recency_score",
            "relevance_score",
            "access_count",
            "created_at",
            "updated_at",
            "metadata",
        ];

        for field in critical_fields.iter() {
            assert!(
                memory.get(field).is_some(),
                "{} search missing critical field '{}': available fields: {:?}",
                search_type,
                field,
                memory.as_object().unwrap().keys().collect::<Vec<_>>()
            );
        }

        // Verify result-level fields
        assert!(
            first_result.get("similarity_score").is_some(),
            "{} search should have similarity_score",
            search_type
        );

        println!("✅ {} search structure validated", search_type);
    }

    // Compare structures between search types
    let (fulltext_type, fulltext_fields) = &results_structures[0];
    let (hybrid_type, hybrid_fields) = &results_structures[1];

    // Both search types should return the same memory structure
    // (The bug was that fulltext was missing columns that hybrid had)
    for field in fulltext_fields {
        assert!(
            hybrid_fields.contains(field),
            "Field '{}' present in {} but missing in {}",
            field,
            fulltext_type,
            hybrid_type
        );
    }

    for field in hybrid_fields {
        assert!(
            fulltext_fields.contains(field),
            "Field '{}' present in {} but missing in {}",
            field,
            hybrid_type,
            fulltext_type
        );
    }

    println!(
        "✅ Search structure consistency verified between {} and {}",
        fulltext_type, hybrid_type
    );

    // Cleanup
    repository
        .delete_memory(
            test_memory.id,
            Some("E2E consistency test cleanup".to_string()),
        )
        .await
        .expect("Failed to cleanup test memory");

    println!("✅ E2E search consistency prevention test passed");
}
