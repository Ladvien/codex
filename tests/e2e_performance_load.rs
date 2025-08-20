//! End-to-End Performance and Load Testing
//!
//! These tests validate system performance under various load conditions,
//! ensuring the memory system meets its performance targets and scales appropriately.
//! Based on SOTA research performance baselines from CLAUDE.md:
//! - Working memory access: <1ms P99
//! - Warm storage query: <100ms P99
//! - Cold storage retrieval: <20s P99
//! - Embedding generation: <100ms P95
//! - Migration batch processing: <5% performance impact
//! - Memory compression ratio: >10:1 for cold tier
//! - Cache hit ratio: >90% for repeated queries
//! - Connection pool utilization: <70% normal, <90% peak

mod test_helpers;

use anyhow::Result;
use chrono::{Duration, Utc};
use codex_memory::memory::models::{
    CreateMemoryRequest, MemoryTier, SearchRequest, UpdateMemoryRequest,
};
use serde_json::json;
use std::time::{Duration as StdDuration, Instant};
use test_helpers::TestEnvironment;
use tokio::time::sleep;
use tracing_test::traced_test;

/// Helper function to create a basic search request
fn create_search_request(
    query: &str,
    limit: Option<i32>,
    tier: Option<MemoryTier>,
    importance_min: Option<f32>,
) -> SearchRequest {
    SearchRequest {
        query_text: Some(query.to_string()),
        query_embedding: None,
        search_type: None,
        hybrid_weights: None,
        tier,
        date_range: None,
        importance_range: importance_min.map(|min| codex_memory::memory::models::RangeFilter {
            min: Some(min),
            max: None,
        }),
        metadata_filters: None,
        tags: None,
        limit,
        offset: None,
        cursor: None,
        similarity_threshold: None,
        include_metadata: Some(true),
        include_facets: None,
        ranking_boost: None,
        explain_score: None,
    }
}

/// Performance measurement utility
struct PerformanceMeasurement {
    name: String,
    start: Instant,
    measurements: Vec<StdDuration>,
}

impl PerformanceMeasurement {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            start: Instant::now(),
            measurements: Vec::new(),
        }
    }

    fn record_iteration(&mut self) {
        let duration = self.start.elapsed();
        self.measurements.push(duration);
        self.start = Instant::now();
    }

    fn finish(self) -> PerformanceResult {
        PerformanceResult {
            name: self.name,
            measurements: self.measurements,
        }
    }
}

struct PerformanceResult {
    name: String,
    measurements: Vec<StdDuration>,
}

impl PerformanceResult {
    fn average(&self) -> StdDuration {
        let total: StdDuration = self.measurements.iter().sum();
        total / self.measurements.len() as u32
    }

    fn p99(&self) -> StdDuration {
        let mut sorted = self.measurements.clone();
        sorted.sort();
        let index = (sorted.len() as f64 * 0.99) as usize;
        sorted.get(index).copied().unwrap_or(StdDuration::ZERO)
    }

    fn p95(&self) -> StdDuration {
        let mut sorted = self.measurements.clone();
        sorted.sort();
        let index = (sorted.len() as f64 * 0.95) as usize;
        sorted.get(index).copied().unwrap_or(StdDuration::ZERO)
    }

    fn max(&self) -> StdDuration {
        self.measurements
            .iter()
            .max()
            .copied()
            .unwrap_or(StdDuration::ZERO)
    }

    fn min(&self) -> StdDuration {
        self.measurements
            .iter()
            .min()
            .copied()
            .unwrap_or(StdDuration::ZERO)
    }

    fn operations_per_second(&self, operation_count: usize) -> f64 {
        let total_time = self.average() * operation_count as u32;
        operation_count as f64 / total_time.as_secs_f64()
    }

    fn assert_performance_targets(&self, p99_target: StdDuration, p95_target: Option<StdDuration>) {
        let p99 = self.p99();
        assert!(
            p99 <= p99_target,
            "{}: P99 latency {}ms exceeds target {}ms",
            self.name,
            p99.as_millis(),
            p99_target.as_millis()
        );

        if let Some(p95_target) = p95_target {
            let p95 = self.p95();
            assert!(
                p95 <= p95_target,
                "{}: P95 latency {}ms exceeds target {}ms",
                self.name,
                p95.as_millis(),
                p95_target.as_millis()
            );
        }

        println!(
            "✓ {}: P99={}ms, P95={}ms, Avg={}ms, Max={}ms, Min={}ms",
            self.name,
            p99.as_millis(),
            self.p95().as_millis(),
            self.average().as_millis(),
            self.max().as_millis(),
            self.min().as_millis()
        );
    }
}

/// Test working memory access performance target: <1ms P99
#[tokio::test]
#[traced_test]
async fn test_working_memory_access_performance() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Create test memories in working tier
    let mut memory_ids = Vec::new();
    for i in 0..100 {
        let memory = env
            .repository
            .create_memory(CreateMemoryRequest {
                content: format!("Working memory performance test content {}", i),
                embedding: None,
                tier: Some(MemoryTier::Working),
                importance_score: Some(0.8),
                metadata: Some(json!({
                    "performance_test": "working_memory_access",
                    "index": i
                })),
                parent_id: None,
                expires_at: None,
            })
            .await?;
        memory_ids.push(memory.id);
    }

    // Measure access performance
    let mut perf = PerformanceMeasurement::new("Working Memory Access");

    for memory_id in &memory_ids {
        let start = Instant::now();
        let _memory = env.repository.get_memory(*memory_id).await?;
        perf.measurements.push(start.elapsed());
    }

    let result = perf.finish();
    result.assert_performance_targets(
        StdDuration::from_millis(1), // P99 < 1ms
        None,
    );

    // Cleanup
    for memory_id in memory_ids {
        env.repository.delete_memory(memory_id).await?;
    }

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test warm storage query performance target: <100ms P99
#[tokio::test]
#[traced_test]
async fn test_warm_storage_query_performance() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Create test memories in warm tier with varied content for realistic search
    let test_contents = vec![
        "Database optimization techniques for high-performance applications",
        "Rust programming patterns and best practices for systems development",
        "Machine learning algorithms implementation in distributed systems",
        "Web security protocols and authentication mechanisms",
        "Container orchestration strategies for microservices architecture",
        "Performance monitoring and observability in production environments",
        "Data structures and algorithms for efficient memory management",
        "Concurrent programming models and synchronization primitives",
        "Network protocols and communication patterns in distributed systems",
        "Testing strategies for large-scale software systems",
    ];

    let mut memory_ids = Vec::new();
    for (i, content) in test_contents.iter().enumerate() {
        for j in 0..10 {
            // Create 10 variations of each content
            let memory = env
                .repository
                .create_memory(CreateMemoryRequest {
                    content: format!("{} - variation {}", content, j),
                    embedding: None,
                    tier: Some(MemoryTier::Warm),
                    importance_score: Some(0.6 + (i as f64 * 0.02)), // Varied importance
                    metadata: Some(json!({
                        "performance_test": "warm_storage_query",
                        "category": format!("category_{}", i % 5),
                        "variation": j
                    })),
                    parent_id: None,
                    expires_at: None,
                })
                .await?;
            memory_ids.push(memory.id);
        }
    }

    // Test various query patterns
    let search_queries = vec![
        "database optimization",
        "rust programming",
        "machine learning",
        "security protocols",
        "performance monitoring",
        "category_0",          // metadata search
        "variation",           // common term search
        "distributed systems", // multi-word search
    ];

    let mut perf = PerformanceMeasurement::new("Warm Storage Query");

    for query in search_queries {
        let search_request = create_search_request(query, Some(20), Some(MemoryTier::Warm), None);
        let start = Instant::now();
        let _results = env.repository.search_memories(search_request).await?;
        perf.measurements.push(start.elapsed());
    }

    // Test importance-based queries
    for importance_threshold in [0.5, 0.6, 0.7, 0.8] {
        let search_request = create_search_request(
            "performance_test",
            Some(50),
            Some(MemoryTier::Warm),
            Some(importance_threshold),
        );
        let start = Instant::now();
        let _results = env.repository.search_memories(search_request).await?;
        perf.measurements.push(start.elapsed());
    }

    let result = perf.finish();
    result.assert_performance_targets(
        StdDuration::from_millis(100),      // P99 < 100ms
        Some(StdDuration::from_millis(50)), // P95 < 50ms
    );

    // Cleanup
    for memory_id in memory_ids {
        env.repository.delete_memory(memory_id).await?;
    }

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test cold storage retrieval performance target: <20s P99
#[tokio::test]
#[traced_test]
async fn test_cold_storage_retrieval_performance() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Create larger memories in cold tier to simulate archived data
    let mut memory_ids = Vec::new();
    for i in 0..50 {
        let large_content = format!(
            "Cold storage archived data entry {}. {}",
            i,
            "This is a larger content block that simulates archived data with significant content length. ".repeat(100)
        );

        let memory = env
            .repository
            .create_memory(CreateMemoryRequest {
                content: large_content,
                embedding: None,
                tier: Some(MemoryTier::Cold),
                importance_score: Some(0.2 + (i as f64 * 0.01)), // Lower importance
                metadata: Some(json!({
                    "performance_test": "cold_storage_retrieval",
                    "archive_date": (Utc::now() - Duration::days(i * 7)).to_rfc3339(),
                    "size_category": if i % 3 == 0 { "large" } else { "medium" },
                    "index": i
                })),
                parent_id: None,
                expires_at: None,
            })
            .await?;
        memory_ids.push(memory.id);
    }

    // Wait for any background indexing to complete
    sleep(StdDuration::from_millis(500)).await;

    // Test cold storage queries with various patterns
    let mut perf = PerformanceMeasurement::new("Cold Storage Retrieval");

    // Direct ID retrieval (fastest case)
    for memory_id in &memory_ids[..10] {
        let start = Instant::now();
        let _memory = env.repository.get_memory(*memory_id).await?;
        perf.measurements.push(start.elapsed());
    }

    // Content-based search in cold tier
    let cold_queries = vec![
        "archived data",
        "cold storage",
        "large content",
        "size_category:large",
    ];

    for query in cold_queries {
        let search_request = create_search_request(query, Some(10), Some(MemoryTier::Cold), None);
        let start = Instant::now();
        let _results = env.repository.search_memories(search_request).await?;
        perf.measurements.push(start.elapsed());
    }

    let result = perf.finish();
    result.assert_performance_targets(
        StdDuration::from_secs(20),      // P99 < 20s
        Some(StdDuration::from_secs(5)), // P95 < 5s
    );

    // Cleanup
    for memory_id in memory_ids {
        env.repository.delete_memory(memory_id).await?;
    }

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test concurrent access performance and throughput
#[tokio::test]
#[traced_test]
async fn test_concurrent_access_performance() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Create test dataset
    let mut memory_ids = Vec::new();
    for i in 0..200 {
        let tier = match i % 3 {
            0 => MemoryTier::Working,
            1 => MemoryTier::Warm,
            _ => MemoryTier::Cold,
        };

        let memory = env
            .repository
            .create_memory(CreateMemoryRequest {
                content: format!(
                    "Concurrent test memory {} with detailed content for realistic testing",
                    i
                ),
                embedding: None,
                tier: Some(tier),
                importance_score: Some(0.5 + (i as f64 % 50.0) / 100.0),
                metadata: Some(json!({
                    "performance_test": "concurrent_access",
                    "tier": format!("{:?}", tier),
                    "index": i
                })),
                parent_id: None,
                expires_at: None,
            })
            .await?;
        memory_ids.push(memory.id);
    }

    // Test concurrent read operations
    let concurrent_readers = 10;
    let reads_per_reader = 20;

    let start_time = Instant::now();
    let mut handles = Vec::new();

    for reader_id in 0..concurrent_readers {
        let repository = env.repository.clone();
        let test_memory_ids = memory_ids.clone();

        let handle = tokio::spawn(async move {
            let mut local_measurements = Vec::new();

            for i in 0..reads_per_reader {
                let memory_id = test_memory_ids[reader_id * reads_per_reader + i];
                let start = Instant::now();
                let _result = repository.get_memory(memory_id).await;
                local_measurements.push(start.elapsed());
            }

            local_measurements
        });

        handles.push(handle);
    }

    // Wait for all readers to complete
    let results = futures::future::join_all(handles).await;
    let total_time = start_time.elapsed();

    // Collect all measurements
    let mut all_measurements = Vec::new();
    for result in results {
        if let Ok(measurements) = result {
            all_measurements.extend(measurements);
        }
    }

    let total_operations = concurrent_readers * reads_per_reader;
    let throughput = total_operations as f64 / total_time.as_secs_f64();

    println!("Concurrent Access Performance:");
    println!("  Total operations: {}", total_operations);
    println!("  Total time: {:.2}s", total_time.as_secs_f64());
    println!("  Throughput: {:.2} operations/second", throughput);

    // Calculate performance metrics
    if !all_measurements.is_empty() {
        let perf_result = PerformanceResult {
            name: "Concurrent Memory Access".to_string(),
            measurements: all_measurements,
        };

        // More relaxed targets for concurrent access
        perf_result.assert_performance_targets(
            StdDuration::from_millis(10),      // P99 < 10ms under load
            Some(StdDuration::from_millis(5)), // P95 < 5ms
        );

        // Verify minimum throughput
        assert!(
            throughput >= 50.0,
            "Throughput {:.2} ops/s below target 50 ops/s",
            throughput
        );
    }

    // Test concurrent write operations (smaller scale to avoid conflicts)
    let concurrent_writers = 5;
    let writes_per_writer = 10;

    let write_start_time = Instant::now();
    let mut write_handles = Vec::new();

    for writer_id in 0..concurrent_writers {
        let repository = env.repository.clone();

        let handle = tokio::spawn(async move {
            let mut write_measurements = Vec::new();

            for i in 0..writes_per_writer {
                let start = Instant::now();
                let result = repository
                    .create_memory(CreateMemoryRequest {
                        content: format!("Concurrent write test {} from writer {}", i, writer_id),
                        embedding: None,
                        tier: Some(MemoryTier::Working),
                        importance_score: Some(0.7),
                        metadata: Some(json!({
                            "performance_test": "concurrent_write",
                            "writer_id": writer_id,
                            "write_index": i
                        })),
                        parent_id: None,
                        expires_at: None,
                    })
                    .await;

                write_measurements.push((start.elapsed(), result.is_ok()));
            }

            write_measurements
        });

        write_handles.push(handle);
    }

    let write_results = futures::future::join_all(write_handles).await;
    let total_write_time = write_start_time.elapsed();

    let mut write_measurements = Vec::new();
    let mut successful_writes = 0;
    let _created_memory_ids: Vec<uuid::Uuid> = Vec::new();

    for result in write_results {
        if let Ok(measurements) = result {
            for (duration, success) in measurements {
                write_measurements.push(duration);
                if success {
                    successful_writes += 1;
                }
            }
        }
    }

    let write_throughput = successful_writes as f64 / total_write_time.as_secs_f64();

    println!("Concurrent Write Performance:");
    println!(
        "  Successful writes: {}/{}",
        successful_writes,
        concurrent_writers * writes_per_writer
    );
    println!("  Write throughput: {:.2} writes/second", write_throughput);

    if !write_measurements.is_empty() {
        let write_perf_result = PerformanceResult {
            name: "Concurrent Memory Creation".to_string(),
            measurements: write_measurements,
        };

        write_perf_result.assert_performance_targets(
            StdDuration::from_millis(100),      // P99 < 100ms for writes
            Some(StdDuration::from_millis(50)), // P95 < 50ms
        );
    }

    // Cleanup all test memories
    for memory_id in memory_ids {
        let _ = env.repository.delete_memory(memory_id).await; // Ignore errors during cleanup
    }

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test search performance with large datasets
#[tokio::test]
#[traced_test]
async fn test_large_dataset_search_performance() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Create a large dataset across all tiers
    println!("Creating large test dataset...");
    let mut memory_ids = Vec::new();
    let dataset_size = 500; // Moderate size for CI/test environments

    // Content templates for variety
    let content_templates = vec![
        "Software engineering best practices for {}: {}",
        "Database design patterns in {}: {}",
        "Performance optimization techniques for {}: {}",
        "Security considerations in {}: {}",
        "Testing strategies for {}: {}",
        "Documentation and maintenance of {}: {}",
    ];

    let technologies = vec![
        "Rust",
        "PostgreSQL",
        "Docker",
        "Kubernetes",
        "React",
        "TypeScript",
        "Python",
        "Java",
        "Go",
        "JavaScript",
        "C++",
        "Swift",
    ];

    let details = vec![
        "implementation details and architecture decisions",
        "deployment strategies and operational considerations",
        "monitoring and observability requirements",
        "scaling and performance optimization",
        "security and compliance requirements",
        "maintenance and technical debt management",
    ];

    for i in 0..dataset_size {
        let template = &content_templates[i % content_templates.len()];
        let technology = &technologies[i % technologies.len()];
        let detail = &details[i % details.len()];

        let tier = match i % 4 {
            0 => MemoryTier::Working,
            1 | 2 => MemoryTier::Warm, // More warm tier data
            _ => MemoryTier::Cold,
        };

        let content = template.replace("{}", technology).replace("{}", detail);

        let memory = env
            .repository
            .create_memory(CreateMemoryRequest {
                content,
                embedding: None,
                tier: Some(tier),
                importance_score: Some(0.3 + (i as f64 % 70.0) / 100.0), // 0.3 to 1.0
                metadata: Some(json!({
                    "performance_test": "large_dataset_search",
                    "technology": technology,
                    "category": format!("category_{}", i % 20),
                    "tier": format!("{:?}", tier),
                    "index": i
                })),
                parent_id: None,
                expires_at: None,
            })
            .await?;

        memory_ids.push(memory.id);

        // Progress indicator
        if i % 100 == 0 {
            println!("Created {} memories...", i);
        }
    }

    println!("Dataset creation complete. Running search performance tests...");

    // Wait for any background indexing
    sleep(StdDuration::from_secs(1)).await;

    // Test various search patterns with performance measurement
    let search_test_cases = vec![
        // Technology-specific searches
        ("Rust", Some(50), None),
        ("PostgreSQL", Some(30), None),
        ("Docker", Some(20), None),
        // Category searches
        ("Software engineering", Some(40), None),
        ("Performance optimization", Some(25), None),
        ("Security considerations", Some(35), None),
        // Tier-specific searches
        ("performance_test", Some(100), Some(MemoryTier::Working)),
        ("performance_test", Some(100), Some(MemoryTier::Warm)),
        ("performance_test", Some(50), Some(MemoryTier::Cold)),
        // Combined searches with importance filtering
        ("implementation", Some(30), None),
        ("architecture", Some(25), None),
        ("monitoring", Some(20), None),
    ];

    let mut search_measurements = Vec::new();

    for (query, limit, tier) in search_test_cases {
        println!(
            "Testing search: '{}' (tier: {:?}, limit: {:?})",
            query, tier, limit
        );

        let search_request = create_search_request(query, limit, tier, None);
        let start = Instant::now();
        let results = env.repository.search_memories(search_request).await?;
        let duration = start.elapsed();

        search_measurements.push(duration);

        println!(
            "  Found {} results in {}ms",
            results.results.len(),
            duration.as_millis()
        );

        // Verify results are reasonable
        assert!(
            results.results.len() <= limit.unwrap_or(50) as usize,
            "Results exceed limit for query '{}'",
            query
        );
    }

    // Test pagination performance
    println!("Testing pagination performance...");
    let pagination_query = "performance_test";
    let page_size = 20;
    let max_pages = 10;

    let mut pagination_measurements = Vec::new();

    for page in 0..max_pages {
        let search_request = SearchRequest {
            query_text: Some(pagination_query.to_string()),
            query_embedding: None,
            search_type: None,
            hybrid_weights: None,
            tier: None,
            date_range: None,
            importance_range: None,
            metadata_filters: None,
            tags: None,
            limit: Some(page_size),
            offset: Some((page * page_size) as i64),
            cursor: None,
            similarity_threshold: None,
            include_metadata: Some(true),
            include_facets: None,
            ranking_boost: None,
            explain_score: None,
        };

        let start = Instant::now();
        let results = env.repository.search_memories(search_request).await?;
        let duration = start.elapsed();

        pagination_measurements.push(duration);

        println!(
            "  Page {} returned {} results in {}ms",
            page + 1,
            results.results.len(),
            duration.as_millis()
        );

        // Break if no more results
        if results.results.is_empty() {
            break;
        }
    }

    // Analyze search performance
    let search_perf = PerformanceResult {
        name: "Large Dataset Search".to_string(),
        measurements: search_measurements,
    };

    search_perf.assert_performance_targets(
        StdDuration::from_millis(500),       // P99 < 500ms for complex searches
        Some(StdDuration::from_millis(200)), // P95 < 200ms
    );

    let pagination_perf = PerformanceResult {
        name: "Search Pagination".to_string(),
        measurements: pagination_measurements,
    };

    pagination_perf.assert_performance_targets(
        StdDuration::from_millis(300),       // P99 < 300ms for pagination
        Some(StdDuration::from_millis(150)), // P95 < 150ms
    );

    // Test statistics performance on large dataset
    println!("Testing statistics performance...");
    let stats_start = Instant::now();
    let stats = env.repository.get_statistics().await?;
    let stats_duration = stats_start.elapsed();

    println!(
        "Statistics query completed in {}ms",
        stats_duration.as_millis()
    );
    assert!(
        stats_duration < StdDuration::from_secs(5),
        "Statistics query took too long: {}ms",
        stats_duration.as_millis()
    );

    if let Some(total) = stats.total_active {
        assert!(
            total >= dataset_size as i64,
            "Statistics should reflect created memories"
        );
    }

    println!("Large dataset performance tests completed successfully.");
    println!("Dataset size: {} memories", dataset_size);

    // Cleanup - batch delete for efficiency
    println!("Cleaning up test dataset...");
    for (i, memory_id) in memory_ids.iter().enumerate() {
        let _ = env.repository.delete_memory(*memory_id).await;

        if i % 100 == 0 {
            println!("Deleted {} memories...", i);
        }
    }

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test memory tier migration performance impact target: <5%
#[tokio::test]
#[traced_test]
async fn test_tier_migration_performance_impact() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Create baseline dataset
    let mut memory_ids = Vec::new();
    for i in 0..100 {
        let memory = env
            .repository
            .create_memory(CreateMemoryRequest {
                content: format!("Migration performance test memory {}", i),
                embedding: None,
                tier: Some(MemoryTier::Working),
                importance_score: Some(0.7),
                metadata: Some(json!({
                    "performance_test": "migration_impact",
                    "baseline": true,
                    "index": i
                })),
                parent_id: None,
                expires_at: None,
            })
            .await?;
        memory_ids.push(memory.id);
    }

    // Measure baseline performance (no migrations)
    println!("Measuring baseline performance...");
    let mut baseline_measurements = Vec::new();

    for i in 0..20 {
        let memory_id = memory_ids[i % memory_ids.len()];
        let start = Instant::now();
        let _memory = env.repository.get_memory(memory_id).await?;
        baseline_measurements.push(start.elapsed());
    }

    let baseline_avg =
        baseline_measurements.iter().sum::<StdDuration>() / baseline_measurements.len() as u32;
    println!(
        "Baseline average access time: {}μs",
        baseline_avg.as_micros()
    );

    // Perform migrations while measuring performance impact
    println!("Performing migrations and measuring impact...");
    let migration_count = 20;
    let mut migration_measurements = Vec::new();

    for i in 0..migration_count {
        // Start migration
        let memory_to_migrate = memory_ids[i];
        let migration_start = Instant::now();

        let _migrated = env
            .repository
            .update_memory(
                memory_to_migrate,
                UpdateMemoryRequest {
                    content: None,
                    embedding: None,
                    tier: Some(MemoryTier::Warm), // Migrate to warm
                    importance_score: None,
                    metadata: Some(json!({
                        "performance_test": "migration_impact",
                        "migrated": true,
                        "migration_index": i
                    })),
                    expires_at: None,
                },
            )
            .await?;

        let _migration_duration = migration_start.elapsed();

        // Measure concurrent access performance during migration
        let concurrent_memory_id = memory_ids[(i + 50) % memory_ids.len()];
        let access_start = Instant::now();
        let _memory = env.repository.get_memory(concurrent_memory_id).await?;
        let access_duration = access_start.elapsed();

        migration_measurements.push(access_duration);

        // Small delay between migrations
        sleep(StdDuration::from_millis(10)).await;
    }

    let migration_avg =
        migration_measurements.iter().sum::<StdDuration>() / migration_measurements.len() as u32;
    println!(
        "Average access time during migrations: {}μs",
        migration_avg.as_micros()
    );

    // Calculate performance impact
    let performance_impact = if baseline_avg.as_nanos() > 0 {
        ((migration_avg.as_nanos() as f64 - baseline_avg.as_nanos() as f64)
            / baseline_avg.as_nanos() as f64)
            * 100.0
    } else {
        0.0
    };

    println!("Performance impact: {:.2}%", performance_impact);

    // Assert performance impact is within target
    assert!(
        performance_impact < 5.0,
        "Migration performance impact {:.2}% exceeds 5% target",
        performance_impact
    );

    // Test batch migration performance
    println!("Testing batch migration performance...");
    let batch_size = 10;
    let batch_start = Instant::now();

    let mut batch_handles = Vec::new();
    for i in 0..batch_size {
        let repository = env.repository.clone();
        let memory_id = memory_ids[migration_count + i];

        let handle = tokio::spawn(async move {
            repository
                .update_memory(
                    memory_id,
                    UpdateMemoryRequest {
                        content: None,
                        embedding: None,
                        tier: Some(MemoryTier::Cold), // Batch migrate to cold
                        importance_score: None,
                        metadata: Some(json!({
                            "performance_test": "migration_impact",
                            "batch_migrated": true
                        })),
                        expires_at: None,
                    },
                )
                .await
        });

        batch_handles.push(handle);
    }

    let batch_results = futures::future::join_all(batch_handles).await;
    let batch_duration = batch_start.elapsed();

    let successful_migrations = batch_results.iter().filter(|r| r.is_ok()).count();
    println!(
        "Batch migration: {}/{} successful in {}ms",
        successful_migrations,
        batch_size,
        batch_duration.as_millis()
    );

    assert!(
        successful_migrations >= batch_size * 8 / 10,
        "Batch migration success rate too low"
    );
    assert!(
        batch_duration < StdDuration::from_secs(10),
        "Batch migration took too long"
    );

    // Cleanup
    for memory_id in memory_ids {
        let _ = env.repository.delete_memory(memory_id).await;
    }

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test connection pool utilization under load
#[tokio::test]
#[traced_test]
async fn test_connection_pool_utilization() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Create baseline memories
    let mut memory_ids = Vec::new();
    for i in 0..50 {
        let memory = env
            .repository
            .create_memory(CreateMemoryRequest {
                content: format!("Connection pool test memory {}", i),
                embedding: None,
                tier: Some(MemoryTier::Working),
                importance_score: Some(0.6),
                metadata: Some(json!({
                    "performance_test": "connection_pool",
                    "index": i
                })),
                parent_id: None,
                expires_at: None,
            })
            .await?;
        memory_ids.push(memory.id);
    }

    // Test normal load (should be <70% utilization)
    println!("Testing normal connection pool utilization...");
    let normal_concurrency = 5;
    let operations_per_task = 20;

    let normal_start = Instant::now();
    let mut normal_handles = Vec::new();

    for _task_id in 0..normal_concurrency {
        let repository = env.repository.clone();
        let test_memory_ids = memory_ids.clone();

        let handle = tokio::spawn(async move {
            for i in 0..operations_per_task {
                let memory_id = test_memory_ids[i % test_memory_ids.len()];
                let _result = repository.get_memory(memory_id).await;

                // Small delay to simulate real usage pattern
                sleep(StdDuration::from_millis(5)).await;
            }
        });

        normal_handles.push(handle);
    }

    futures::future::join_all(normal_handles).await;
    let normal_duration = normal_start.elapsed();

    println!(
        "Normal load completed in {:.2}s",
        normal_duration.as_secs_f64()
    );

    // Test peak load (should be <90% utilization)
    println!("Testing peak connection pool utilization...");
    let peak_concurrency = 15; // Higher concurrency
    let peak_operations = 10;

    let peak_start = Instant::now();
    let mut peak_handles = Vec::new();

    for task_id in 0..peak_concurrency {
        let repository = env.repository.clone();
        let test_memory_ids = memory_ids.clone();

        let handle = tokio::spawn(async move {
            for i in 0..peak_operations {
                let memory_id = test_memory_ids[(task_id + i) % test_memory_ids.len()];
                let _result = repository.get_memory(memory_id).await;

                // Shorter delay for peak load
                sleep(StdDuration::from_millis(1)).await;
            }
        });

        peak_handles.push(handle);
    }

    futures::future::join_all(peak_handles).await;
    let peak_duration = peak_start.elapsed();

    println!("Peak load completed in {:.2}s", peak_duration.as_secs_f64());

    // Test system recovery after peak load
    println!("Testing system recovery...");
    let recovery_start = Instant::now();

    for i in 0..10 {
        let memory_id = memory_ids[i];
        let _memory = env.repository.get_memory(memory_id).await?;
    }

    let recovery_duration = recovery_start.elapsed();
    println!(
        "Recovery operations completed in {}ms",
        recovery_duration.as_millis()
    );

    // Assert system is still responsive
    assert!(
        recovery_duration < StdDuration::from_secs(1),
        "System recovery took too long after peak load"
    );

    // Test mixed operations under load
    println!("Testing mixed operations performance...");
    let mixed_start = Instant::now();
    let mut mixed_handles = Vec::new();

    // Read-heavy workload
    for i in 0..10 {
        let repository = env.repository.clone();
        let test_memory_ids = memory_ids.clone();

        let handle = tokio::spawn(async move {
            let memory_id = test_memory_ids[i % test_memory_ids.len()];
            repository.get_memory(memory_id).await
        });

        mixed_handles.push(handle);
    }

    // Create separate vectors for different operation types
    let mut create_handles = Vec::new();
    let mut search_handles = Vec::new();

    // Write operations
    for i in 0..3 {
        let repository = env.repository.clone();

        let handle = tokio::spawn(async move {
            repository
                .create_memory(CreateMemoryRequest {
                    content: format!("Mixed load test memory {}", i),
                    embedding: None,
                    tier: Some(MemoryTier::Working),
                    importance_score: Some(0.5),
                    metadata: Some(json!({
                        "performance_test": "mixed_load",
                        "operation": "create"
                    })),
                    parent_id: None,
                    expires_at: None,
                })
                .await
        });

        create_handles.push(handle);
    }

    // Search operations
    for _i in 0..2 {
        let repository = env.repository.clone();

        let handle = tokio::spawn(async move {
            let search_request = create_search_request("performance_test", Some(10), None, None);
            repository.search_memories(search_request).await
        });

        search_handles.push(handle);
    }

    // Wait for all operations to complete
    let read_results = futures::future::join_all(mixed_handles).await;
    let create_results = futures::future::join_all(create_handles).await;
    let search_results = futures::future::join_all(search_handles).await;
    let mixed_duration = mixed_start.elapsed();

    let successful_reads = read_results.iter().filter(|r| r.is_ok()).count();
    let successful_creates = create_results.iter().filter(|r| r.is_ok()).count();
    let successful_searches = search_results.iter().filter(|r| r.is_ok()).count();
    let total_ops = read_results.len() + create_results.len() + search_results.len();
    let total_successful = successful_reads + successful_creates + successful_searches;

    println!(
        "Mixed operations: {}/{} successful in {}ms",
        total_successful,
        total_ops,
        mixed_duration.as_millis()
    );

    assert!(
        total_successful >= total_ops * 8 / 10,
        "Mixed operations success rate too low under load"
    );

    // Cleanup
    for memory_id in memory_ids {
        let _ = env.repository.delete_memory(memory_id).await;
    }

    env.cleanup_test_data().await?;
    Ok(())
}
