//! Performance benchmarks for establishing baseline metrics

use crate::memory::models::{CreateMemoryRequest, SearchRequest, UpdateMemoryRequest};
use crate::memory::{MemoryRepository, MemoryTier};
use anyhow::Result;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::info;

/// Benchmark results for a specific operation
#[derive(Debug, Clone)]
pub struct BenchmarkResult {
    pub operation: String,
    pub iterations: u32,
    pub total_duration: Duration,
    pub avg_duration: Duration,
    pub min_duration: Duration,
    pub max_duration: Duration,
    pub ops_per_second: f64,
}

/// Benchmark suite for memory operations
pub struct MemoryBenchmarks {
    repository: Arc<MemoryRepository>,
    iterations: u32,
}

impl MemoryBenchmarks {
    pub fn new(repository: Arc<MemoryRepository>, iterations: u32) -> Self {
        Self {
            repository,
            iterations,
        }
    }

    /// Run all benchmarks and return results
    pub async fn run_all(&self) -> Result<Vec<BenchmarkResult>> {
        let mut results = Vec::new();

        info!("Running memory operation benchmarks...");

        // Benchmark create operation
        results.push(self.benchmark_create().await?);

        // Benchmark read operation
        results.push(self.benchmark_read().await?);

        // Benchmark update operation
        results.push(self.benchmark_update().await?);

        // Benchmark delete operation
        results.push(self.benchmark_delete().await?);

        // Benchmark search operation
        results.push(self.benchmark_search().await?);

        // Benchmark bulk operations
        results.push(self.benchmark_bulk_insert().await?);

        // Benchmark concurrent reads
        results.push(self.benchmark_concurrent_reads().await?);

        Ok(results)
    }

    /// Benchmark memory creation
    async fn benchmark_create(&self) -> Result<BenchmarkResult> {
        let mut durations = Vec::new();

        for i in 0..self.iterations {
            let content = format!("Benchmark create content {i}");
            let request = CreateMemoryRequest {
                content,
                embedding: None,
                tier: Some(MemoryTier::Working),
                importance_score: Some(0.5),
                metadata: Some(serde_json::json!({"benchmark": true})),
                parent_id: None,
                expires_at: None,
            };

            let start = Instant::now();
            self.repository.create_memory(request).await?;
            let duration = start.elapsed();

            durations.push(duration);
        }

        Ok(self.calculate_result("Create", durations))
    }

    /// Benchmark memory read
    async fn benchmark_read(&self) -> Result<BenchmarkResult> {
        // First create test memories and store their IDs
        let mut memory_ids = Vec::new();
        for i in 0..self.iterations {
            let content = format!("Benchmark read content {i}");
            let request = CreateMemoryRequest {
                content,
                embedding: None,
                tier: Some(MemoryTier::Working),
                importance_score: Some(0.5),
                metadata: Some(serde_json::json!({"benchmark": true})),
                parent_id: None,
                expires_at: None,
            };
            let memory = self.repository.create_memory(request).await?;
            memory_ids.push(memory.id);
        }

        let mut durations = Vec::new();

        for id in memory_ids {
            let start = Instant::now();
            self.repository.get_memory(id).await?;
            let duration = start.elapsed();

            durations.push(duration);
        }

        Ok(self.calculate_result("Read", durations))
    }

    /// Benchmark memory update
    async fn benchmark_update(&self) -> Result<BenchmarkResult> {
        // First create test memories and store their IDs
        let mut memory_ids = Vec::new();
        for i in 0..self.iterations {
            let content = format!("Original content {i}");
            let request = CreateMemoryRequest {
                content,
                embedding: None,
                tier: Some(MemoryTier::Working),
                importance_score: Some(0.5),
                metadata: Some(serde_json::json!({"benchmark": true})),
                parent_id: None,
                expires_at: None,
            };
            let memory = self.repository.create_memory(request).await?;
            memory_ids.push(memory.id);
        }

        let mut durations = Vec::new();

        for (i, id) in memory_ids.iter().enumerate() {
            let update_request = UpdateMemoryRequest {
                content: Some(format!("Updated content {i}")),
                embedding: None,
                tier: None,
                importance_score: Some(0.7),
                metadata: None,
                expires_at: None,
            };

            let start = Instant::now();
            self.repository.update_memory(*id, update_request).await?;
            let duration = start.elapsed();

            durations.push(duration);
        }

        Ok(self.calculate_result("Update", durations))
    }

    /// Benchmark memory deletion
    async fn benchmark_delete(&self) -> Result<BenchmarkResult> {
        // First create test memories and store their IDs
        let mut memory_ids = Vec::new();
        for i in 0..self.iterations {
            let content = format!("Content to delete {i}");
            let request = CreateMemoryRequest {
                content,
                embedding: None,
                tier: Some(MemoryTier::Working),
                importance_score: Some(0.5),
                metadata: Some(serde_json::json!({"benchmark": true})),
                parent_id: None,
                expires_at: None,
            };
            let memory = self.repository.create_memory(request).await?;
            memory_ids.push(memory.id);
        }

        let mut durations = Vec::new();

        for id in memory_ids {
            let start = Instant::now();
            self.repository.delete_memory(id).await?;
            let duration = start.elapsed();

            durations.push(duration);
        }

        Ok(self.calculate_result("Delete", durations))
    }

    /// Benchmark search operation
    async fn benchmark_search(&self) -> Result<BenchmarkResult> {
        // Create diverse test data for searching
        for i in 0..100 {
            let content = format!(
                "Search benchmark content with keyword{} and topic{}",
                i % 10,
                i % 5
            );
            let request = CreateMemoryRequest {
                content,
                embedding: None,
                tier: Some(MemoryTier::Working),
                importance_score: Some(0.5),
                metadata: Some(serde_json::json!({"benchmark": true})),
                parent_id: None,
                expires_at: None,
            };
            self.repository.create_memory(request).await?;
        }

        let mut durations = Vec::new();
        let queries = ["keyword1", "topic2", "benchmark", "content", "search"];

        for i in 0..self.iterations {
            let query = queries[i as usize % queries.len()];
            let search_request = SearchRequest {
                query_text: Some(query.to_string()),
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
                include_metadata: None,
                include_facets: None,
                ranking_boost: None,
                explain_score: None,
            };

            let start = Instant::now();
            self.repository
                .search_memories_simple(search_request)
                .await?;
            let duration = start.elapsed();

            durations.push(duration);
        }

        Ok(self.calculate_result("Search", durations))
    }

    /// Benchmark bulk insert operations
    async fn benchmark_bulk_insert(&self) -> Result<BenchmarkResult> {
        let mut durations = Vec::new();
        let batch_size = 100;

        for batch in 0..(self.iterations / batch_size).max(1) {
            let start = Instant::now();

            for i in 0..batch_size {
                let content = format!("Bulk insert batch {batch} item {i}");
                let request = CreateMemoryRequest {
                    content,
                    embedding: None,
                    tier: Some(MemoryTier::Working),
                    importance_score: Some(0.5),
                    metadata: Some(serde_json::json!({"benchmark": true, "batch": batch})),
                    parent_id: None,
                    expires_at: None,
                };
                self.repository.create_memory(request).await?;
            }

            let duration = start.elapsed();
            durations.push(duration);
        }

        Ok(self.calculate_result("Bulk Insert (100 items)", durations))
    }

    /// Benchmark concurrent read operations
    async fn benchmark_concurrent_reads(&self) -> Result<BenchmarkResult> {
        // Create test data and store IDs
        let mut memory_ids = Vec::new();
        for i in 0..100 {
            let content = format!("Concurrent read content {i}");
            let request = CreateMemoryRequest {
                content,
                embedding: None,
                tier: Some(MemoryTier::Working),
                importance_score: Some(0.5),
                metadata: Some(serde_json::json!({"benchmark": true})),
                parent_id: None,
                expires_at: None,
            };
            let memory = self.repository.create_memory(request).await?;
            memory_ids.push(memory.id);
        }

        let mut durations = Vec::new();
        let concurrent_reads = 10;

        for _ in 0..(self.iterations / concurrent_reads).max(1) {
            let start = Instant::now();

            let mut handles = Vec::new();
            for i in 0..concurrent_reads {
                let repo = Arc::clone(&self.repository);
                let id = memory_ids[i as usize % memory_ids.len()];

                let handle = tokio::spawn(async move { repo.get_memory(id).await });

                handles.push(handle);
            }

            // Wait for all reads to complete
            for handle in handles {
                handle.await??;
            }

            let duration = start.elapsed();
            durations.push(duration);
        }

        Ok(self.calculate_result("Concurrent Reads (10)", durations))
    }

    /// Calculate benchmark result from duration samples
    fn calculate_result(&self, operation: &str, durations: Vec<Duration>) -> BenchmarkResult {
        let total_duration: Duration = durations.iter().sum();
        let avg_duration = total_duration / durations.len() as u32;
        let min_duration = durations.iter().min().cloned().unwrap_or(Duration::ZERO);
        let max_duration = durations.iter().max().cloned().unwrap_or(Duration::ZERO);

        let ops_per_second = if total_duration.as_secs_f64() > 0.0 {
            durations.len() as f64 / total_duration.as_secs_f64()
        } else {
            0.0
        };

        BenchmarkResult {
            operation: operation.to_string(),
            iterations: durations.len() as u32,
            total_duration,
            avg_duration,
            min_duration,
            max_duration,
            ops_per_second,
        }
    }

    /// Print benchmark results in a formatted table
    pub fn print_results(results: &[BenchmarkResult]) {
        println!("\n=== Performance Benchmark Results ===\n");
        println!(
            "{:<20} {:>10} {:>15} {:>15} {:>15} {:>10}",
            "Operation", "Iterations", "Avg (ms)", "Min (ms)", "Max (ms)", "Ops/sec"
        );
        println!("{:-<90}", "");

        for result in results {
            println!(
                "{:<20} {:>10} {:>15.2} {:>15.2} {:>15.2} {:>10.2}",
                result.operation,
                result.iterations,
                result.avg_duration.as_secs_f64() * 1000.0,
                result.min_duration.as_secs_f64() * 1000.0,
                result.max_duration.as_secs_f64() * 1000.0,
                result.ops_per_second
            );
        }

        println!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_calculate_result() {
        let benchmarks = MemoryBenchmarks {
            repository: Arc::new(MemoryRepository::new(
                sqlx::PgPool::connect_lazy("postgresql://localhost/test").unwrap(),
            )),
            iterations: 100,
        };

        let durations = vec![
            Duration::from_millis(10),
            Duration::from_millis(20),
            Duration::from_millis(15),
            Duration::from_millis(25),
            Duration::from_millis(30),
        ];

        let result = benchmarks.calculate_result("Test", durations);

        assert_eq!(result.operation, "Test");
        assert_eq!(result.iterations, 5);
        assert_eq!(result.min_duration, Duration::from_millis(10));
        assert_eq!(result.max_duration, Duration::from_millis(30));
        assert_eq!(result.avg_duration, Duration::from_millis(20));
    }
}
