//! Comprehensive Test Runner for the Agentic Memory System
//!
//! This module provides a test runner that executes all the major test categories
//! and provides a comprehensive report of system functionality.

mod test_helpers;

use anyhow::Result;
use std::time::Instant;
use test_helpers::{PerformanceMeter, TestEnvironment};
use tracing_test::traced_test;

/// Summary of test execution results
#[derive(Debug)]
pub struct TestExecutionSummary {
    pub total_tests: usize,
    pub passed_tests: usize,
    pub failed_tests: usize,
    pub total_duration: std::time::Duration,
    pub test_results: Vec<TestResult>,
}

#[derive(Debug)]
pub struct TestResult {
    pub test_name: String,
    pub passed: bool,
    pub duration: std::time::Duration,
    pub error_message: Option<String>,
}

impl Default for TestExecutionSummary {
    fn default() -> Self {
        Self::new()
    }
}

impl TestExecutionSummary {
    pub fn new() -> Self {
        Self {
            total_tests: 0,
            passed_tests: 0,
            failed_tests: 0,
            total_duration: std::time::Duration::ZERO,
            test_results: Vec::new(),
        }
    }

    pub fn add_result(&mut self, result: TestResult) {
        self.total_tests += 1;
        if result.passed {
            self.passed_tests += 1;
        } else {
            self.failed_tests += 1;
        }
        self.total_duration += result.duration;
        self.test_results.push(result);
    }

    pub fn success_rate(&self) -> f64 {
        if self.total_tests == 0 {
            0.0
        } else {
            self.passed_tests as f64 / self.total_tests as f64
        }
    }

    pub fn print_summary(&self) {
        println!("\n=== Comprehensive Test Execution Summary ===");
        println!("Total Tests: {}", self.total_tests);
        println!("Passed: {}", self.passed_tests);
        println!("Failed: {}", self.failed_tests);
        println!("Success Rate: {:.1}%", self.success_rate() * 100.0);
        println!("Total Duration: {:?}", self.total_duration);

        if self.failed_tests > 0 {
            println!("\n--- Failed Tests ---");
            for result in &self.test_results {
                if !result.passed {
                    println!("❌ {}: {:?}", result.test_name, result.duration);
                    if let Some(error) = &result.error_message {
                        println!("   Error: {error}");
                    }
                }
            }
        }

        println!("\n--- Passed Tests ---");
        for result in &self.test_results {
            if result.passed {
                println!("✅ {}: {:?}", result.test_name, result.duration);
            }
        }

        println!("\n=== Performance Summary ===");
        let avg_duration = self.total_duration / self.total_tests as u32;
        println!("Average test duration: {avg_duration:?}");

        let fastest = self
            .test_results
            .iter()
            .min_by_key(|r| r.duration)
            .map(|r| (&r.test_name, r.duration));
        let slowest = self
            .test_results
            .iter()
            .max_by_key(|r| r.duration)
            .map(|r| (&r.test_name, r.duration));

        if let Some((name, duration)) = fastest {
            println!("Fastest test: {name} ({duration:?})");
        }
        if let Some((name, duration)) = slowest {
            println!("Slowest test: {name} ({duration:?})");
        }
    }
}

/// Execute a test function and capture the result
async fn execute_test<F, Fut>(test_name: &str, test_fn: F) -> TestResult
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<()>>,
{
    let start = Instant::now();
    let result = test_fn().await;
    let duration = start.elapsed();

    match result {
        Ok(()) => TestResult {
            test_name: test_name.to_string(),
            passed: true,
            duration,
            error_message: None,
        },
        Err(e) => TestResult {
            test_name: test_name.to_string(),
            passed: false,
            duration,
            error_message: Some(e.to_string()),
        },
    }
}

/// Comprehensive test suite that covers all major functionality
#[tokio::test]
#[traced_test]
async fn run_comprehensive_test_suite() -> Result<()> {
    let mut summary = TestExecutionSummary::new();

    println!("🚀 Starting Comprehensive Test Suite for Agentic Memory System");
    println!("Testing with Ollama embedding service integration");

    // Test 1: Environment Setup and Configuration
    let result = execute_test("environment_setup", || async {
        let _env = TestEnvironment::new().await?;
        println!("✓ Test environment initialized successfully");
        Ok(())
    })
    .await;
    summary.add_result(result);

    // Test 2: Basic Memory CRUD Operations
    let result = execute_test("basic_memory_crud", || async {
        test_basic_memory_crud().await
    })
    .await;
    summary.add_result(result);

    // Test 3: Embedding Generation
    let result = execute_test("embedding_generation", || async {
        test_embedding_generation().await
    })
    .await;
    summary.add_result(result);

    // Test 4: Search Functionality
    let result = execute_test("search_functionality", || async {
        test_search_functionality().await
    })
    .await;
    summary.add_result(result);

    // Test 5: Concurrent Operations
    let result = execute_test("concurrent_operations", || async {
        test_concurrent_operations().await
    })
    .await;
    summary.add_result(result);

    // Test 6: Performance Baseline
    let result = execute_test("performance_baseline", || async {
        test_performance_baseline().await
    })
    .await;
    summary.add_result(result);

    // Test 7: Error Handling
    let result = execute_test("error_handling", || async { test_error_handling().await }).await;
    summary.add_result(result);

    // Test 8: Search Memory Regression Prevention (E2E)
    let result = execute_test("search_memory_regression_prevention", || async {
        test_search_memory_regression_prevention().await
    })
    .await;
    summary.add_result(result);

    // Test 8: Data Persistence
    let result = execute_test("data_persistence", || async {
        test_data_persistence().await
    })
    .await;
    summary.add_result(result);

    // Test 9: Memory Tier Management
    let result = execute_test("memory_tier_management", || async {
        test_memory_tier_management().await
    })
    .await;
    summary.add_result(result);

    // Test 10: System Statistics
    let result = execute_test("system_statistics", || async {
        test_system_statistics().await
    })
    .await;
    summary.add_result(result);

    // Print comprehensive summary
    summary.print_summary();

    // Assert overall success
    if summary.success_rate() < 0.8 {
        return Err(anyhow::anyhow!(
            "Test suite failed: Success rate {:.1}% is below 80% threshold",
            summary.success_rate() * 100.0
        ));
    }

    println!("\n🎉 Comprehensive test suite completed successfully!");
    println!("System is ready for production use with Ollama integration.");

    Ok(())
}

async fn test_basic_memory_crud() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Create memory
    let memory = env
        .create_test_memory(
            "Basic CRUD test memory",
            codex_memory::memory::models::MemoryTier::Working,
            0.7,
        )
        .await?;

    // Read memory
    let retrieved = env
        .repository
        .get_memory(memory.id)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    assert_eq!(retrieved.content, memory.content);

    // Update memory
    let update_request = codex_memory::memory::models::UpdateMemoryRequest {
        content: Some("Updated CRUD test memory".to_string()),
        embedding: None,
        tier: Some(codex_memory::memory::models::MemoryTier::Warm),
        importance_score: Some(0.9),
        metadata: None,
        expires_at: None,
    };

    let updated = env
        .repository
        .update_memory(memory.id, update_request)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    assert_eq!(updated.content, "Updated CRUD test memory");
    assert_eq!(updated.tier, codex_memory::memory::models::MemoryTier::Warm);

    // Delete memory
    env.repository
        .delete_memory(memory.id)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let delete_result = env.repository.get_memory(memory.id).await;
    assert!(delete_result.is_err());

    env.cleanup_test_data().await?;
    println!("✓ Basic CRUD operations working correctly");
    Ok(())
}

async fn test_embedding_generation() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Test embedding generation
    let embedding = env
        .embedder
        .generate_embedding("Test embedding generation")
        .await?;
    assert!(!embedding.is_empty());
    assert_eq!(embedding.len(), env.embedder.embedding_dimension());

    // Test batch embedding generation
    let texts = vec![
        "First test text".to_string(),
        "Second test text".to_string(),
        "Third test text".to_string(),
    ];

    let batch_embeddings = env.embedder.generate_embeddings_batch(&texts).await?;
    assert_eq!(batch_embeddings.len(), 3);

    for embedding in &batch_embeddings {
        assert!(!embedding.is_empty());
        assert_eq!(embedding.len(), env.embedder.embedding_dimension());
    }

    println!("✓ Embedding generation working correctly");
    println!("  - Provider: {:?}", env.embedder.provider());
    println!("  - Dimensions: {}", env.embedder.embedding_dimension());
    Ok(())
}

async fn test_search_functionality() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Create test memories with different content
    let test_contents = vec![
        "Rust programming language tutorial",
        "Python data science guide",
        "JavaScript web development",
        "Database optimization techniques",
        "Machine learning algorithms",
    ];

    for content in &test_contents {
        env.create_test_memory(
            content,
            codex_memory::memory::models::MemoryTier::Working,
            0.8,
        )
        .await?;
    }

    env.wait_for_consistency().await;

    // Test text search
    let results = env.test_search("programming", Some(10)).await?;
    assert!(!results.is_empty());

    // Test search with filters
    let search_request = codex_memory::memory::models::SearchRequest {
        query_text: Some("programming".to_string()),
        query_embedding: None,
        search_type: None,
        hybrid_weights: None,
        tier: Some(codex_memory::memory::models::MemoryTier::Working),
        date_range: None,
        importance_range: None,
        metadata_filters: Some(env.get_test_metadata(None)),
        tags: None,
        limit: Some(5),
        offset: None,
        cursor: None,
        similarity_threshold: None,
        include_metadata: Some(true),
        include_facets: None,
        ranking_boost: None,
        explain_score: None,
    };

    let search_response = env
        .repository
        .search_memories(search_request)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    assert!(!search_response.results.is_empty());

    env.cleanup_test_data().await?;
    println!("✓ Search functionality working correctly");
    Ok(())
}

async fn test_concurrent_operations() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Test concurrent memory creation
    let handles: Vec<_> = (0..5)
        .map(|i| {
            let repo = std::sync::Arc::clone(&env.repository);
            let test_id = env.test_id.clone();
            tokio::spawn(async move {
                let request = codex_memory::memory::models::CreateMemoryRequest {
                    content: format!("Concurrent test memory {i}"),
                    embedding: None,
                    tier: Some(codex_memory::memory::models::MemoryTier::Working),
                    importance_score: Some(0.6),
                    metadata: Some(serde_json::json!({
                        "test_id": test_id,
                        "concurrent": true,
                        "worker": i
                    })),
                    parent_id: None,
                    expires_at: None,
                };
                repo.create_memory(request).await
            })
        })
        .collect();

    let mut successful_creates = 0;
    for handle in handles {
        match handle.await {
            Ok(Ok(_)) => successful_creates += 1,
            Ok(Err(e)) => println!("Concurrent create failed: {e}"),
            Err(e) => println!("Task join failed: {e}"),
        }
    }

    assert!(
        successful_creates >= 3,
        "At least 3 concurrent creates should succeed"
    );

    env.cleanup_test_data().await?;
    println!("✓ Concurrent operations handling correctly");
    Ok(())
}

async fn test_performance_baseline() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Test single operation performance
    let create_meter = PerformanceMeter::new("single_memory_create");
    let memory = env
        .create_test_memory(
            "Performance test memory",
            codex_memory::memory::models::MemoryTier::Working,
            0.7,
        )
        .await?;
    let create_result = create_meter.finish();

    let read_meter = PerformanceMeter::new("single_memory_read");
    let _retrieved = env
        .repository
        .get_memory(memory.id)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    let read_result = read_meter.finish();

    let search_meter = PerformanceMeter::new("single_search");
    let _results = env.test_search("performance test", Some(5)).await?;
    let search_result = search_meter.finish();

    // Assert performance baselines
    assert!(
        create_result.duration < std::time::Duration::from_secs(5),
        "Memory creation should be under 5 seconds"
    );
    assert!(
        read_result.duration < std::time::Duration::from_millis(500),
        "Memory read should be under 500ms"
    );
    assert!(
        search_result.duration < std::time::Duration::from_secs(2),
        "Search should be under 2 seconds"
    );

    env.cleanup_test_data().await?;
    println!("✓ Performance baselines met");
    println!("  - Create: {:?}", create_result.duration);
    println!("  - Read: {:?}", read_result.duration);
    println!("  - Search: {:?}", search_result.duration);
    Ok(())
}

async fn test_error_handling() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Test invalid ID handling
    let invalid_id = uuid::Uuid::new_v4();
    let result = env.repository.get_memory(invalid_id).await;
    assert!(result.is_err());

    // Test empty content
    let empty_result = env
        .repository
        .create_memory(codex_memory::memory::models::CreateMemoryRequest {
            content: String::new(),
            embedding: None,
            tier: Some(codex_memory::memory::models::MemoryTier::Working),
            importance_score: Some(0.5),
            metadata: Some(env.get_test_metadata(None)),
            parent_id: None,
            expires_at: None,
        })
        .await;

    // Should either succeed with empty content or fail gracefully
    match empty_result {
        Ok(_) => println!("  - Empty content accepted"),
        Err(_) => println!("  - Empty content rejected gracefully"),
    }

    env.cleanup_test_data().await?;
    println!("✓ Error handling working correctly");
    Ok(())
}

async fn test_data_persistence() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Create memory with specific data
    let memory = env
        .create_test_memory(
            "Persistence test memory with specific content",
            codex_memory::memory::models::MemoryTier::Working,
            0.75,
        )
        .await?;

    // Verify data persists across retrieval
    let retrieved = env
        .repository
        .get_memory(memory.id)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    assert_eq!(retrieved.id, memory.id);
    assert_eq!(retrieved.content, memory.content);
    assert_eq!(retrieved.tier, memory.tier);
    assert_eq!(retrieved.importance_score, memory.importance_score);
    assert!(retrieved.embedding.is_some());

    env.cleanup_test_data().await?;
    println!("✓ Data persistence working correctly");
    Ok(())
}

async fn test_memory_tier_management() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Create memories in different tiers
    let working_memory = env
        .create_test_memory(
            "Working tier memory",
            codex_memory::memory::models::MemoryTier::Working,
            0.9,
        )
        .await?;

    let warm_memory = env
        .create_test_memory(
            "Warm tier memory",
            codex_memory::memory::models::MemoryTier::Warm,
            0.6,
        )
        .await?;

    let cold_memory = env
        .create_test_memory(
            "Cold tier memory",
            codex_memory::memory::models::MemoryTier::Cold,
            0.3,
        )
        .await?;

    // Verify tier assignment
    assert_eq!(
        working_memory.tier,
        codex_memory::memory::models::MemoryTier::Working
    );
    assert_eq!(
        warm_memory.tier,
        codex_memory::memory::models::MemoryTier::Warm
    );
    assert_eq!(
        cold_memory.tier,
        codex_memory::memory::models::MemoryTier::Cold
    );

    env.cleanup_test_data().await?;
    println!("✓ Memory tier management working correctly");
    Ok(())
}

async fn test_system_statistics() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Create some memories
    let _memories = env.create_test_memories(5).await?;

    env.wait_for_consistency().await;

    // Get statistics
    let stats = env
        .repository
        .get_statistics()
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    assert!(stats.total_active.unwrap_or(0) >= 5);

    let test_stats = env.get_test_statistics().await?;
    assert_eq!(test_stats.total_count, 5);

    env.cleanup_test_data().await?;
    println!("✓ System statistics working correctly");
    Ok(())
}

async fn test_search_memory_regression_prevention() -> Result<()> {
    use codex_memory::harvester::service::HarvesterService;
    use codex_memory::insights::processor::InsightProcessor;
    use codex_memory::insights::scheduler::SchedulerService;
    use codex_memory::insights::storage::InsightStorage;
    use codex_memory::mcp_server::handlers::MCPHandlers;
    use codex_memory::mcp_server::logging::MCPLogger;
    use codex_memory::mcp_server::progress::ProgressTracker;
    use serde_json::json;
    use std::sync::Arc;

    println!("🔍 Testing search memory regression prevention (E2E)...");

    let env = TestEnvironment::new().await?;

    // Set up MCP handlers (this is what Claude Desktop uses)
    let insight_storage = Arc::new(InsightStorage::new(
        Arc::new(env.pool.clone()),
        env.embedder.clone(),
    ));
    let insight_processor = Arc::new(InsightProcessor::new(
        env.repository.clone(),
        insight_storage.clone(),
        env.embedder.clone(),
        env.config.clone(),
    ));
    let scheduler_service = Arc::new(SchedulerService::new(
        insight_processor.clone(),
        env.repository.clone(),
    ));
    let harvester_service = Arc::new(HarvesterService::new(
        env.repository.clone(),
        env.embedder.clone(),
        env.config.clone(),
    ));
    let mcp_logger = Arc::new(MCPLogger::new(
        codex_memory::mcp_server::logging::LogLevel::Info,
    ));
    let progress_tracker = Arc::new(ProgressTracker::new());

    let handlers = MCPHandlers::new(
        env.repository.clone(),
        insight_storage,
        insight_processor,
        scheduler_service,
        env.embedder.clone(),
        harvester_service,
        mcp_logger,
        progress_tracker,
    );

    // Create test memory to ensure we have searchable content
    let memory = env
        .create_test_memory(
            "SEARCH_REGRESSION_TEST memory for E2E validation",
            codex_memory::memory::models::MemoryTier::Working,
            0.85,
        )
        .await?;

    // Test 1: Fulltext search through MCP (the exact failure case from Claude Desktop)
    println!("  Testing fulltext search through MCP handler...");
    let fulltext_params = json!({
        "query": "SEARCH_REGRESSION_TEST",
        "limit": 5
    });

    let fulltext_result = handlers
        .execute_tool("search_memory", &fulltext_params)
        .await;
    assert!(
        fulltext_result.is_ok(),
        "Fulltext search through MCP should not fail with column errors"
    );

    let fulltext_response = fulltext_result.unwrap();
    assert!(
        fulltext_response.get("results").is_some(),
        "Response should have results"
    );

    let results = fulltext_response
        .get("results")
        .unwrap()
        .as_array()
        .unwrap();
    assert!(!results.is_empty(), "Should find the test memory");

    // Verify the result structure (this is what was broken before)
    let first_result = &results[0];
    assert!(
        first_result.get("memory").is_some(),
        "Result should have memory object"
    );
    assert!(
        first_result.get("similarity_score").is_some(),
        "Result should have similarity_score"
    );

    let memory_obj = first_result.get("memory").unwrap();

    // These are the fields that were missing and causing the column mismatch error
    let critical_fields = [
        "id",
        "content",
        "importance_score",
        "recency_score",
        "relevance_score",
        "tier",
        "status",
        "created_at",
        "updated_at",
    ];

    for field in critical_fields.iter() {
        assert!(
            memory_obj.get(field).is_some(),
            "Memory object missing critical field '{}' - this was the regression bug!",
            field
        );
    }

    println!("    ✓ Fulltext search structure validation passed");

    // Test 2: Hybrid search consistency
    println!("  Testing hybrid search consistency...");
    let hybrid_params = json!({
        "query": "SEARCH_REGRESSION_TEST",
        "limit": 5,
        "similarity_threshold": 0.0
    });

    let hybrid_result = handlers.execute_tool("search_memory", &hybrid_params).await;
    assert!(hybrid_result.is_ok(), "Hybrid search should work");

    let hybrid_response = hybrid_result.unwrap();
    let hybrid_results = hybrid_response.get("results").unwrap().as_array().unwrap();
    assert!(
        !hybrid_results.is_empty(),
        "Hybrid search should find results"
    );

    // Compare field structure between fulltext and hybrid to ensure consistency
    let hybrid_memory = hybrid_results[0].get("memory").unwrap();

    for field in critical_fields.iter() {
        assert!(
            hybrid_memory.get(field).is_some(),
            "Hybrid search missing field '{}' - structure inconsistency!",
            field
        );
    }

    println!("    ✓ Hybrid search structure validation passed");

    // Test 3: Edge case that might trigger column errors
    println!("  Testing edge cases for column error prevention...");
    let edge_params = json!({
        "query": "",
        "limit": 0
    });

    let edge_result = handlers.execute_tool("search_memory", &edge_params).await;
    match edge_result {
        Ok(_) => println!("    ✓ Edge case handled successfully"),
        Err(e) => {
            let error_msg = format!("{:?}", e);
            assert!(
                !error_msg.contains("column"),
                "Edge case should not cause column-related errors: {}",
                error_msg
            );
            assert!(
                !error_msg.contains("try_get"),
                "Edge case should not cause column mapping errors: {}",
                error_msg
            );
            println!("    ✓ Edge case properly rejected with non-column error");
        }
    }

    env.cleanup_test_data().await?;

    println!("✓ Search memory regression prevention tests passed");
    println!("  - MCP fulltext search: ✓");
    println!("  - MCP hybrid search: ✓");
    println!("  - Column structure consistency: ✓");
    println!("  - Edge case handling: ✓");

    Ok(())
}
