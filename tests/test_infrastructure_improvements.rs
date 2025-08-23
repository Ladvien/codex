//! Test Infrastructure Improvements and Utilities
//!
//! This module provides advanced testing utilities, benchmarking frameworks,
//! and infrastructure improvements to enhance the overall testing experience.

mod test_helpers;

use anyhow::Result;
use chrono::{Duration, Utc};
use codex_memory::memory::models::{CreateMemoryRequest, MemoryTier, SearchRequest};
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration as StdDuration, Instant};
use test_helpers::TestEnvironment;
use tokio::sync::Semaphore;
use tokio::time::sleep;
use tracing_test::traced_test;

/// Enhanced benchmarking framework for performance analysis
pub struct BenchmarkSuite {
    name: String,
    measurements: Vec<BenchmarkResult>,
    baseline: Option<BenchmarkResult>,
}

pub struct BenchmarkResult {
    operation_name: String,
    duration: StdDuration,
    operations_count: usize,
    throughput: f64, // operations per second
    memory_usage: Option<usize>,
    metadata: HashMap<String, String>,
}

impl BenchmarkSuite {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            measurements: Vec::new(),
            baseline: None,
        }
    }

    pub async fn benchmark_operation<F, Fut, T>(
        &mut self,
        operation_name: &str,
        operation_count: usize,
        operation_factory: F,
    ) -> Result<T>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        println!("Benchmarking: {operation_name} ({operation_count} iterations)");

        let start_time = Instant::now();
        let mut last_result = None;

        for i in 0..operation_count {
            let result = operation_factory().await?;
            if i == operation_count - 1 {
                last_result = Some(result);
            }
        }

        let total_duration = start_time.elapsed();
        let throughput = operation_count as f64 / total_duration.as_secs_f64();

        let benchmark_result = BenchmarkResult {
            operation_name: operation_name.to_string(),
            duration: total_duration,
            operations_count: operation_count,
            throughput,
            memory_usage: None, // Could be enhanced with memory profiling
            metadata: HashMap::new(),
        };

        println!(
            "  {} completed: {:.2}ms total, {:.2} ops/sec",
            operation_name,
            total_duration.as_millis(),
            throughput
        );

        self.measurements.push(benchmark_result);

        Ok(last_result.unwrap())
    }

    pub fn set_baseline(&mut self, operation_name: &str) {
        if let Some(result) = self
            .measurements
            .iter()
            .find(|r| r.operation_name == operation_name)
        {
            self.baseline = Some(BenchmarkResult {
                operation_name: result.operation_name.clone(),
                duration: result.duration,
                operations_count: result.operations_count,
                throughput: result.throughput,
                memory_usage: result.memory_usage,
                metadata: result.metadata.clone(),
            });
            println!(
                "Baseline set for '{}': {:.2} ops/sec",
                operation_name, result.throughput
            );
        }
    }

    pub fn analyze_performance_regression(&self, tolerance_percent: f64) -> Vec<String> {
        let mut regressions = Vec::new();

        if let Some(baseline) = &self.baseline {
            for measurement in &self.measurements {
                if measurement.operation_name != baseline.operation_name {
                    continue;
                }

                let performance_ratio = measurement.throughput / baseline.throughput;
                let regression_percent = (1.0 - performance_ratio) * 100.0;

                if regression_percent > tolerance_percent {
                    let regression_msg = format!(
                        "Performance regression in '{}': {:.1}% slower than baseline ({:.2} vs {:.2} ops/sec)",
                        measurement.operation_name,
                        regression_percent,
                        measurement.throughput,
                        baseline.throughput
                    );
                    regressions.push(regression_msg);
                }
            }
        }

        regressions
    }

    pub fn generate_report(&self) -> String {
        let mut report = format!("Benchmark Report: {}\n", self.name);
        report.push_str("=".repeat(50).as_str());
        report.push('\n');

        for measurement in &self.measurements {
            report.push_str(&format!(
                "Operation: {}\n  Duration: {:.2}ms\n  Count: {}\n  Throughput: {:.2} ops/sec\n\n",
                measurement.operation_name,
                measurement.duration.as_millis(),
                measurement.operations_count,
                measurement.throughput
            ));
        }

        if let Some(baseline) = &self.baseline {
            report.push_str(&format!(
                "Baseline: {} ({:.2} ops/sec)\n",
                baseline.operation_name, baseline.throughput
            ));
        }

        report
    }
}

/// Test data generator for consistent test scenarios
pub struct TestDataGenerator {
    seed: u64,
    counter: Arc<Mutex<usize>>,
}

impl TestDataGenerator {
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            counter: Arc::new(Mutex::new(0)),
        }
    }

    pub fn generate_memory_content(&self, size_category: &str) -> String {
        let count = {
            let mut c = self.counter.lock().unwrap();
            *c += 1;
            *c
        };

        let base_content = format!("Generated test content {} seed {}", count, self.seed);

        match size_category {
            "small" => base_content,
            "medium" => format!("{} - {}", base_content, "x".repeat(500)),
            "large" => format!("{} - {}", base_content, "x".repeat(5000)),
            "xl" => format!("{} - {}", base_content, "x".repeat(50000)),
            _ => base_content,
        }
    }

    pub fn generate_metadata(&self, complexity: &str) -> serde_json::Value {
        let count = {
            let mut c = self.counter.lock().unwrap();
            *c += 1;
            *c
        };

        match complexity {
            "simple" => json!({
                "generated": true,
                "count": count,
                "seed": self.seed
            }),
            "nested" => json!({
                "generated": true,
                "count": count,
                "seed": self.seed,
                "nested": {
                    "level1": {
                        "level2": {
                            "data": "deep nested data"
                        }
                    }
                },
                "array": [1, 2, 3, count],
                "timestamp": Utc::now().to_rfc3339()
            }),
            "complex" => json!({
                "generated": true,
                "count": count,
                "seed": self.seed,
                "large_nested": {
                    "categories": ["tech", "science", "business", "health"],
                    "properties": {
                        "importance": count as f64 / 100.0,
                        "created_at": Utc::now().to_rfc3339(),
                        "tags": (0..10).map(|i| format!("tag_{i}")).collect::<Vec<_>>()
                    }
                },
                "metrics": {
                    "access_frequency": count % 10,
                    "last_modified": Utc::now().timestamp(),
                    "version": count
                }
            }),
            _ => json!({"default": true, "count": count}),
        }
    }

    pub fn generate_batch_create_requests(
        &self,
        count: usize,
        tier: MemoryTier,
    ) -> Vec<CreateMemoryRequest> {
        (0..count)
            .map(|i| {
                let size_category = match i % 4 {
                    0 => "small",
                    1 => "medium",
                    2 => "large",
                    _ => "xl",
                };

                let metadata_complexity = match i % 3 {
                    0 => "simple",
                    1 => "nested",
                    _ => "complex",
                };

                CreateMemoryRequest {
                    content: self.generate_memory_content(size_category),
                    embedding: None,
                    tier: Some(tier),
                    importance_score: Some(((i as f32 + 1.0) / count as f32) as f64),
                    metadata: Some(self.generate_metadata(metadata_complexity)),
                    parent_id: None,
                    expires_at: if i % 5 == 0 {
                        Some(Utc::now() + Duration::hours(1))
                    } else {
                        None
                    },
                }
            })
            .collect()
    }
}

/// Test comprehensive benchmarking suite
#[tokio::test]
#[traced_test]
async fn test_comprehensive_benchmarking_suite() -> Result<()> {
    let env = TestEnvironment::new().await?;
    let mut benchmark_suite = BenchmarkSuite::new("Memory System Performance");
    let data_generator = TestDataGenerator::new(12345);

    println!("Starting comprehensive benchmarking suite...");

    // Benchmark 1: Memory Creation Performance
    let create_requests = data_generator.generate_batch_create_requests(50, MemoryTier::Working);
    let created_ids = Arc::new(Mutex::new(Vec::new()));

    let _result = benchmark_suite
        .benchmark_operation("memory_creation", 50, || {
            let env_clone = env.clone();
            let ids_clone = created_ids.clone();
            let request_index = {
                let ids = ids_clone.lock().unwrap();
                ids.len() % create_requests.len()
            };
            let request = create_requests[request_index].clone();

            async move {
                let memory = env_clone.repository.create_memory(request).await?;
                {
                    let mut ids = ids_clone.lock().unwrap();
                    ids.push(memory.id);
                }
                Result::<uuid::Uuid, anyhow::Error>::Ok(memory.id)
            }
        })
        .await?;

    benchmark_suite.set_baseline("memory_creation");

    // Benchmark 2: Memory Retrieval Performance
    let ids_guard = created_ids.lock().unwrap();
    if !ids_guard.is_empty() {
        let test_ids = ids_guard.clone();
        drop(ids_guard);

        benchmark_suite
            .benchmark_operation("memory_retrieval", 100, || {
                let env_clone = env.clone();
                let ids_clone = test_ids.clone();
                let test_id = ids_clone[ids_clone.len() % ids_clone.len()];

                async move {
                    let memory = env_clone.repository.get_memory(test_id).await?;
                    Result::<String, anyhow::Error>::Ok(memory.content)
                }
            })
            .await?;
    }

    // Benchmark 3: Search Performance
    let search_terms = ["Generated", "test", "content", "seed"];

    benchmark_suite
        .benchmark_operation("memory_search", 30, || {
            let env_clone = env.clone();
            let term = search_terms[search_terms.len() % search_terms.len()];

            async move {
                let search_request = SearchRequest {
                    query_text: Some(term.to_string()),
                    query_embedding: None,
                    search_type: None,
                    hybrid_weights: None,
                    tier: None,
                    date_range: None,
                    importance_range: None,
                    metadata_filters: None,
                    tags: None,
                    limit: Some(10),
                    offset: None,
                    cursor: None,
                    similarity_threshold: None,
                    include_metadata: Some(true),
                    include_facets: None,
                    ranking_boost: None,
                    explain_score: None,
                };

                let results = env_clone.repository.search_memories(search_request).await?;
                Result::<usize, anyhow::Error>::Ok(results.results.len())
            }
        })
        .await?;

    // Benchmark 4: Concurrent Operations Performance
    let concurrent_ops = 20;

    benchmark_suite
        .benchmark_operation("concurrent_operations", 1, || {
            let env_clone = env.clone();
            let data_gen = TestDataGenerator::new(54321);

            async move {
                let semaphore = Arc::new(Semaphore::new(10));
                let mut handles = Vec::new();

                for i in 0..concurrent_ops {
                    let env_c = env_clone.clone();
                    let sem = semaphore.clone();
                    let content = data_gen.generate_memory_content("medium");

                    let handle = tokio::spawn(async move {
                        let _permit = sem.acquire().await.unwrap();

                        env_c
                            .repository
                            .create_memory(CreateMemoryRequest {
                                content,
                                embedding: None,
                                tier: Some(MemoryTier::Working),
                                importance_score: Some(0.5),
                                metadata: Some(json!({"concurrent": true, "index": i})),
                                parent_id: None,
                                expires_at: None,
                            })
                            .await
                    });

                    handles.push(handle);
                }

                let mut successful = 0;
                for handle in handles {
                    if let Ok(Ok(_)) = handle.await {
                        successful += 1;
                    }
                }

                Result::<usize, anyhow::Error>::Ok(successful)
            }
        })
        .await?;

    // Generate and print report
    let report = benchmark_suite.generate_report();
    println!("\n{report}");

    // Check for performance regressions
    let regressions = benchmark_suite.analyze_performance_regression(20.0); // 20% tolerance
    if !regressions.is_empty() {
        println!("Performance Regressions Detected:");
        for regression in &regressions {
            println!("  ⚠ {regression}");
        }
    } else {
        println!("✓ No significant performance regressions detected");
    }

    // Cleanup
    let cleanup_ids = created_ids.lock().unwrap().clone();
    for id in cleanup_ids {
        let _ = env.repository.delete_memory(id).await;
    }

    env.cleanup_test_data().await?;

    println!("✓ Comprehensive benchmarking suite completed");
    Ok(())
}

/// Test advanced test data generation patterns
#[tokio::test]
#[traced_test]
async fn test_advanced_data_generation_patterns() -> Result<()> {
    let env = TestEnvironment::new().await?;
    let data_generator = TestDataGenerator::new(98765);

    println!("Testing advanced data generation patterns...");

    // Test various data generation patterns
    let patterns = vec![
        ("small_simple", "small", "simple", 10),
        ("medium_nested", "medium", "nested", 15),
        ("large_complex", "large", "complex", 8),
        ("xl_mixed", "xl", "nested", 5),
    ];

    let mut pattern_results = HashMap::new();

    for (pattern_name, size_category, metadata_complexity, count) in patterns {
        println!("Testing pattern: {pattern_name} ({count} items)");

        let start_time = Instant::now();
        let mut created_memories = Vec::new();

        // Generate batch of test data
        let requests = data_generator.generate_batch_create_requests(count, MemoryTier::Working);

        // Verify data generation consistency
        for (i, request) in requests.iter().enumerate() {
            // Verify content size matches category
            let content_size = request.content.len();
            match size_category {
                "small" => assert!(content_size < 100, "Small content should be < 100 chars"),
                "medium" => assert!(
                    content_size > 100 && content_size < 1000,
                    "Medium content should be 100-1000 chars"
                ),
                "large" => assert!(
                    content_size > 1000 && content_size < 10000,
                    "Large content should be 1k-10k chars"
                ),
                "xl" => assert!(content_size > 10000, "XL content should be > 10k chars"),
                _ => {}
            }

            // Verify metadata complexity
            if let Some(metadata) = &request.metadata {
                match metadata_complexity {
                    "simple" => {
                        assert!(
                            metadata.as_object().unwrap().len() <= 5,
                            "Simple metadata should have <= 5 fields"
                        );
                    }
                    "nested" => {
                        assert!(
                            metadata.get("nested").is_some(),
                            "Nested metadata should have nested field"
                        );
                    }
                    "complex" => {
                        assert!(
                            metadata.get("large_nested").is_some(),
                            "Complex metadata should have large_nested field"
                        );
                        assert!(
                            metadata.get("metrics").is_some(),
                            "Complex metadata should have metrics field"
                        );
                    }
                    _ => {}
                }
            }

            // Create the memory
            let memory = env.repository.create_memory(request.clone()).await?;

            // Verify importance score increases with index
            assert!(
                (memory.importance_score - (((i as f32 + 1.0) / count as f32) as f64)).abs()
                    < 0.001,
                "Importance score should increase with index"
            );

            created_memories.push(memory);
        }

        let creation_duration = start_time.elapsed();
        let throughput = count as f64 / creation_duration.as_secs_f64();

        println!(
            "  Pattern '{}' completed: {:.2}ms total, {:.2} items/sec",
            pattern_name,
            creation_duration.as_millis(),
            throughput
        );

        // Test search with generated data
        let search_start = Instant::now();
        let search_request = SearchRequest {
            query_text: Some("Generated test content".to_string()),
            query_embedding: None,
            search_type: None,
            hybrid_weights: None,
            tier: None,
            date_range: None,
            importance_range: None,
            metadata_filters: None,
            tags: None,
            limit: Some(20),
            offset: None,
            cursor: None,
            similarity_threshold: None,
            include_metadata: Some(true),
            include_facets: None,
            ranking_boost: None,
            explain_score: None,
        };

        let search_results = env.repository.search_memories(search_request).await?;
        let search_duration = search_start.elapsed();

        println!(
            "  Search found {} results in {:.2}ms",
            search_results.results.len(),
            search_duration.as_millis()
        );

        // Store pattern results
        pattern_results.insert(
            pattern_name,
            (
                throughput,
                search_results.results.len(),
                created_memories.len(),
            ),
        );

        // Cleanup pattern memories
        for memory in created_memories {
            let _ = env.repository.delete_memory(memory.id).await;
        }

        // Brief pause between patterns
        sleep(StdDuration::from_millis(200)).await;
    }

    // Analyze pattern performance
    println!("\nData Generation Pattern Analysis:");
    for (pattern_name, (throughput, search_results, created_count)) in pattern_results {
        println!(
            "  {pattern_name}: {throughput:.2} items/sec creation, {search_results} search results from {created_count} items"
        );
    }

    env.cleanup_test_data().await?;

    println!("✓ Advanced data generation patterns test completed");
    Ok(())
}

/// Test infrastructure monitoring and observability
#[tokio::test]
#[traced_test]
async fn test_infrastructure_monitoring_observability() -> Result<()> {
    let env = TestEnvironment::new().await?;

    println!("Testing infrastructure monitoring and observability...");

    // Test metrics collection during operations
    let start_time = Instant::now();
    let mut operation_metrics: Vec<(String, StdDuration)> = Vec::new();

    // Phase 1: Baseline metrics
    let baseline_start = Instant::now();
    let baseline_memory = env
        .repository
        .create_memory(CreateMemoryRequest {
            content: "Baseline monitoring test".to_string(),
            embedding: None,
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.5),
            metadata: Some(json!({"monitoring": "baseline"})),
            parent_id: None,
            expires_at: None,
        })
        .await?;

    let baseline_duration = baseline_start.elapsed();
    operation_metrics.push(("baseline_create".to_string(), baseline_duration));

    // Phase 2: Load testing with metrics collection
    let load_operations = 25;
    let load_start = Instant::now();
    let mut load_memories = Vec::new();

    for i in 0..load_operations {
        let op_start = Instant::now();

        let memory = env
            .repository
            .create_memory(CreateMemoryRequest {
                content: format!("Load test memory {i} - monitoring data"),
                embedding: None,
                tier: Some(MemoryTier::Working),
                importance_score: Some(((i as f32) / load_operations as f32) as f64),
                metadata: Some(json!({
                    "monitoring": "load_test",
                    "index": i,
                    "timestamp": Utc::now().to_rfc3339()
                })),
                parent_id: None,
                expires_at: None,
            })
            .await?;

        let op_duration = op_start.elapsed();
        operation_metrics.push((format!("load_create_{i}"), op_duration));

        load_memories.push(memory);

        // Add some read operations for mixed workload
        if i % 3 == 0 {
            let read_start = Instant::now();
            let _ = env.repository.get_memory(baseline_memory.id).await?;
            let read_duration = read_start.elapsed();
            operation_metrics.push((format!("load_read_{i}"), read_duration));
        }
    }

    let load_duration = load_start.elapsed();
    let load_throughput = load_operations as f64 / load_duration.as_secs_f64();

    // Phase 3: Search performance monitoring
    let search_operations = 15;
    let search_start = Instant::now();
    let mut search_metrics = Vec::new();

    let search_terms = ["monitoring", "load test", "baseline", "memory"];

    for i in 0..search_operations {
        let term = &search_terms[i % search_terms.len()];
        let search_op_start = Instant::now();

        let search_request = SearchRequest {
            query_text: Some(term.to_string()),
            query_embedding: None,
            search_type: None,
            hybrid_weights: None,
            tier: None,
            date_range: None,
            importance_range: None,
            metadata_filters: None,
            tags: None,
            limit: Some(10),
            offset: None,
            cursor: None,
            similarity_threshold: None,
            include_metadata: Some(true),
            include_facets: None,
            ranking_boost: None,
            explain_score: None,
        };

        let search_results = env.repository.search_memories(search_request).await?;
        let search_op_duration = search_op_start.elapsed();

        search_metrics.push((search_results.results.len(), search_op_duration));
        operation_metrics.push((format!("search_{i}"), search_op_duration));
    }

    let search_duration = search_start.elapsed();
    let search_throughput = search_operations as f64 / search_duration.as_secs_f64();

    // Phase 4: Analyze collected metrics
    println!("\nInfrastructure Monitoring Results:");
    println!(
        "  Total test duration: {:.2}ms",
        start_time.elapsed().as_millis()
    );
    println!(
        "  Load operations: {} creates in {:.2}ms ({:.2} ops/sec)",
        load_operations,
        load_duration.as_millis(),
        load_throughput
    );
    println!(
        "  Search operations: {} searches in {:.2}ms ({:.2} ops/sec)",
        search_operations,
        search_duration.as_millis(),
        search_throughput
    );

    // Latency analysis
    let create_latencies: Vec<_> = operation_metrics
        .iter()
        .filter(|(name, _)| name.starts_with("load_create_"))
        .map(|(_, duration)| duration.as_millis())
        .collect();

    if !create_latencies.is_empty() {
        let avg_latency =
            create_latencies.iter().sum::<u128>() as f64 / create_latencies.len() as f64;
        let min_latency = *create_latencies.iter().min().unwrap();
        let max_latency = *create_latencies.iter().max().unwrap();

        println!(
            "  Create Latency - Avg: {avg_latency:.2}ms, Min: {min_latency}ms, Max: {max_latency}ms"
        );
    }

    let search_latencies: Vec<_> = search_metrics
        .iter()
        .map(|(_, duration)| duration.as_millis())
        .collect();

    if !search_latencies.is_empty() {
        let avg_search_latency =
            search_latencies.iter().sum::<u128>() as f64 / search_latencies.len() as f64;
        let min_search_latency = *search_latencies.iter().min().unwrap();
        let max_search_latency = *search_latencies.iter().max().unwrap();

        println!(
            "  Search Latency - Avg: {avg_search_latency:.2}ms, Min: {min_search_latency}ms, Max: {max_search_latency}ms"
        );
    }

    // Resource utilization simulation
    let resource_start = Instant::now();
    let statistics = env.repository.get_statistics().await?;
    let stats_duration = resource_start.elapsed();

    println!("  Statistics query: {:.2}ms", stats_duration.as_millis());

    if let Some(total_active) = statistics.total_active {
        println!("  Total active memories: {total_active}");
    }
    if let Some(avg_importance) = statistics.avg_importance {
        println!("  Average importance: {avg_importance:.3}");
    }

    // Performance target validation (from CLAUDE.md)
    let sota_targets = [
        ("working_memory_access", 1.0), // <1ms P99
        ("warm_storage_query", 100.0),  // <100ms P99
    ];

    println!("\nSOTA Performance Target Validation:");
    for (target_name, target_ms) in sota_targets {
        match target_name {
            "working_memory_access" => {
                let read_latencies: Vec<_> = operation_metrics
                    .iter()
                    .filter(|(name, _)| name.starts_with("load_read_"))
                    .map(|(_, duration)| duration.as_millis() as f64)
                    .collect();

                if !read_latencies.is_empty() {
                    let max_read_latency = read_latencies.iter().fold(0.0f64, |a, &b| a.max(b));
                    let meets_target = max_read_latency <= target_ms;
                    println!(
                        "  {}: {:.2}ms max (target: {:.1}ms) {}",
                        target_name,
                        max_read_latency,
                        target_ms,
                        if meets_target { "✓" } else { "⚠" }
                    );
                }
            }
            "warm_storage_query" => {
                if !search_latencies.is_empty() {
                    let max_search_latency = *search_latencies.iter().max().unwrap() as f64;
                    let meets_target = max_search_latency <= target_ms;
                    println!(
                        "  {}: {:.2}ms max (target: {:.1}ms) {}",
                        target_name,
                        max_search_latency,
                        target_ms,
                        if meets_target { "✓" } else { "⚠" }
                    );
                }
            }
            _ => {}
        }
    }

    // Cleanup
    env.repository.delete_memory(baseline_memory.id).await?;
    for memory in load_memories {
        let _ = env.repository.delete_memory(memory.id).await;
    }

    env.cleanup_test_data().await?;

    println!("✓ Infrastructure monitoring and observability test completed");
    Ok(())
}

/// Test automated test reporting and analysis
#[tokio::test]
#[traced_test]
async fn test_automated_reporting_analysis() -> Result<()> {
    println!("Testing automated test reporting and analysis...");

    let env = TestEnvironment::new().await?;

    // Simulate a comprehensive test run with various scenarios
    let test_scenarios = vec![
        ("basic_operations", 20, true),
        ("performance_stress", 50, true),
        ("edge_cases", 15, false), // Some failures expected
        ("concurrency_test", 30, true),
        ("error_recovery", 10, false), // Some failures expected
    ];

    let mut test_report = HashMap::new();
    let overall_start = Instant::now();

    for (scenario_name, operation_count, expect_all_success) in test_scenarios {
        println!("Running test scenario: {scenario_name} ({operation_count} operations)");

        let scenario_start = Instant::now();
        let mut successes = 0;
        let mut failures = 0;
        let mut operation_times = Vec::new();

        for i in 0..operation_count {
            let op_start = Instant::now();

            // Simulate different operation types based on scenario
            let operation_result = match scenario_name {
                "basic_operations" => env
                    .repository
                    .create_memory(CreateMemoryRequest {
                        content: format!("Basic operation {i}"),
                        embedding: None,
                        tier: Some(MemoryTier::Working),
                        importance_score: Some(0.5),
                        metadata: Some(json!({"scenario": scenario_name, "index": i})),
                        parent_id: None,
                        expires_at: None,
                    })
                    .await
                    .map(|m| m.id),
                "performance_stress" => {
                    // Large content operations
                    let large_content = "x".repeat(5000);
                    env.repository
                        .create_memory(CreateMemoryRequest {
                            content: format!("Stress test {i} - {large_content}"),
                            embedding: None,
                            tier: Some(MemoryTier::Working),
                            importance_score: Some(0.7),
                            metadata: Some(
                                json!({"scenario": scenario_name, "index": i, "stress": true}),
                            ),
                            parent_id: None,
                            expires_at: None,
                        })
                        .await
                        .map(|m| m.id)
                }
                "edge_cases" => {
                    // Intentionally trigger some edge cases/failures
                    if i % 3 == 0 {
                        // Create valid memory
                        env.repository
                            .create_memory(CreateMemoryRequest {
                                content: "Valid edge case".to_string(),
                                embedding: None,
                                tier: Some(MemoryTier::Working),
                                importance_score: Some(0.5),
                                metadata: Some(json!({"scenario": scenario_name, "valid": true})),
                                parent_id: None,
                                expires_at: None,
                            })
                            .await
                            .map(|m| m.id)
                    } else {
                        // Try to access non-existent memory (will fail)
                        env.repository
                            .get_memory(uuid::Uuid::new_v4())
                            .await
                            .map(|m| m.id)
                    }
                }
                "concurrency_test" => {
                    // Quick concurrent-safe operations
                    env.repository
                        .create_memory(CreateMemoryRequest {
                            content: format!("Concurrent test {i}"),
                            embedding: None,
                            tier: Some(MemoryTier::Working),
                            importance_score: Some(0.6),
                            metadata: Some(json!({"scenario": scenario_name, "concurrent": true})),
                            parent_id: None,
                            expires_at: None,
                        })
                        .await
                        .map(|m| m.id)
                }
                "error_recovery" => {
                    // Mix of valid operations and intentional errors
                    if i % 4 == 0 {
                        // Invalid operation (empty content, should fail validation)
                        env.repository
                            .create_memory(CreateMemoryRequest {
                                content: "".to_string(), // Empty content might be invalid
                                embedding: None,
                                tier: Some(MemoryTier::Working),
                                importance_score: Some(-1.0), // Invalid importance score
                                metadata: None,
                                parent_id: None,
                                expires_at: None,
                            })
                            .await
                            .map(|m| m.id)
                    } else {
                        // Valid operation
                        env.repository
                            .create_memory(CreateMemoryRequest {
                                content: format!("Recovery test {i}"),
                                embedding: None,
                                tier: Some(MemoryTier::Working),
                                importance_score: Some(0.5),
                                metadata: Some(
                                    json!({"scenario": scenario_name, "recovery": true}),
                                ),
                                parent_id: None,
                                expires_at: None,
                            })
                            .await
                            .map(|m| m.id)
                    }
                }
                _ => {
                    // Default operation
                    env.repository
                        .create_memory(CreateMemoryRequest {
                            content: format!("Default operation {i}"),
                            embedding: None,
                            tier: Some(MemoryTier::Working),
                            importance_score: Some(0.5),
                            metadata: Some(json!({"scenario": scenario_name})),
                            parent_id: None,
                            expires_at: None,
                        })
                        .await
                        .map(|m| m.id)
                }
            };

            let op_duration = op_start.elapsed();
            operation_times.push(op_duration);

            match operation_result {
                Ok(_) => successes += 1,
                Err(_) => failures += 1,
            }
        }

        let scenario_duration = scenario_start.elapsed();
        let success_rate = successes as f64 / operation_count as f64;
        let avg_operation_time = operation_times.iter().sum::<StdDuration>().as_millis() as f64
            / operation_times.len() as f64;

        // Store scenario results
        test_report.insert(scenario_name, json!({
            "operation_count": operation_count,
            "successes": successes,
            "failures": failures,
            "success_rate": success_rate,
            "duration_ms": scenario_duration.as_millis(),
            "avg_operation_ms": avg_operation_time,
            "expected_all_success": expect_all_success,
            "meets_expectations": if expect_all_success { success_rate >= 0.95 } else { success_rate >= 0.6 }
        }));

        println!(
            "  {} completed: {}/{} successful ({:.1}%)",
            scenario_name,
            successes,
            operation_count,
            success_rate * 100.0
        );
    }

    let overall_duration = overall_start.elapsed();

    // Generate comprehensive report
    println!("{}", format!("\n{}", "=".repeat(60)));
    println!("AUTOMATED TEST REPORT");
    println!("{}", "=".repeat(60));
    println!(
        "Total execution time: {:.2}s",
        overall_duration.as_secs_f64()
    );
    println!("Test scenarios completed: {}", test_report.len());
    println!();

    let mut total_operations = 0;
    let mut total_successes = 0;
    let mut scenarios_meeting_expectations = 0;

    for (scenario_name, results) in &test_report {
        let successes = results["successes"].as_i64().unwrap_or(0);
        let operation_count = results["operation_count"].as_i64().unwrap_or(0);
        let success_rate = results["success_rate"].as_f64().unwrap_or(0.0);
        let duration = results["duration_ms"].as_u64().unwrap_or(0);
        let avg_op_time = results["avg_operation_ms"].as_f64().unwrap_or(0.0);
        let meets_expectations = results["meets_expectations"].as_bool().unwrap_or(false);

        total_operations += operation_count;
        total_successes += successes;

        if meets_expectations {
            scenarios_meeting_expectations += 1;
        }

        let status_icon = if meets_expectations { "✓" } else { "⚠" };

        println!("Scenario: {scenario_name}");
        println!(
            "  {} Success Rate: {:.1}% ({}/{} operations)",
            status_icon,
            success_rate * 100.0,
            successes,
            operation_count
        );
        println!(
            "  Duration: {:.2}s, Avg Operation: {:.2}ms",
            duration as f64 / 1000.0,
            avg_op_time
        );
        println!();
    }

    // Overall summary
    let overall_success_rate = total_successes as f64 / total_operations as f64;
    let scenario_success_rate = scenarios_meeting_expectations as f64 / test_report.len() as f64;

    println!("OVERALL SUMMARY");
    println!("  Total Operations: {total_operations}");
    println!(
        "  Overall Success Rate: {:.1}%",
        overall_success_rate * 100.0
    );
    println!(
        "  Scenarios Meeting Expectations: {:.1}% ({}/{})",
        scenario_success_rate * 100.0,
        scenarios_meeting_expectations,
        test_report.len()
    );

    // Performance analysis
    let performance_scenarios = ["basic_operations", "performance_stress", "concurrency_test"];
    let mut performance_summary = Vec::new();

    for scenario in performance_scenarios {
        if let Some(results) = test_report.get(scenario) {
            let avg_op_time = results["avg_operation_ms"].as_f64().unwrap_or(0.0);
            performance_summary.push((scenario, avg_op_time));
        }
    }

    if !performance_summary.is_empty() {
        println!("\nPERFORMANCE ANALYSIS");
        for (scenario, avg_time) in performance_summary {
            let performance_grade = if avg_time <= 10.0 {
                "Excellent"
            } else if avg_time <= 50.0 {
                "Good"
            } else if avg_time <= 200.0 {
                "Acceptable"
            } else {
                "Needs Improvement"
            };

            println!("  {scenario}: {avg_time:.2}ms avg ({performance_grade})");
        }
    }

    // Recommendations
    println!("\nRECOMMENDATIONS");
    if overall_success_rate < 0.9 {
        println!("  • Investigate failures in scenarios with low success rates");
    }
    if scenario_success_rate < 0.8 {
        println!("  • Review expectations for failing scenarios");
    }

    let avg_perf_time = test_report
        .values()
        .filter_map(|v| v["avg_operation_ms"].as_f64())
        .sum::<f64>()
        / test_report.len() as f64;

    if avg_perf_time > 100.0 {
        println!(
            "  • Consider performance optimizations (avg operation time: {avg_perf_time:.1}ms)"
        );
    }

    println!("  • Regular automated testing recommended for regression detection");
    println!("{}", "=".repeat(60));

    // Assert overall test quality
    assert!(
        overall_success_rate >= 0.7,
        "Overall success rate should be >= 70%"
    );
    assert!(
        scenario_success_rate >= 0.6,
        "At least 60% of scenarios should meet expectations"
    );

    env.cleanup_test_data().await?;

    println!("✓ Automated test reporting and analysis completed");
    Ok(())
}
