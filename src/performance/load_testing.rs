//! Load testing implementation for performance validation

use super::{LoadTestConfig, PerformanceMetrics, PerformanceTestResult, TestType};
use crate::memory::models::{CreateMemoryRequest, SearchRequest, UpdateMemoryRequest};
use crate::memory::{MemoryRepository, MemoryTier};
use anyhow::Result;
use chrono::Utc;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time;
use tracing::{debug, info};

/// Load testing orchestrator
pub struct LoadTester {
    config: LoadTestConfig,
    repository: Arc<MemoryRepository>,
    metrics: Arc<LoadTestMetrics>,
}

/// Metrics collected during load testing
struct LoadTestMetrics {
    total_requests: AtomicU64,
    successful_requests: AtomicU64,
    failed_requests: AtomicU64,
    latencies: RwLock<VecDeque<u64>>,
    errors: RwLock<Vec<String>>,
    start_time: Instant,
}

impl LoadTester {
    pub fn new(config: LoadTestConfig, repository: Arc<MemoryRepository>) -> Self {
        Self {
            config,
            repository,
            metrics: Arc::new(LoadTestMetrics {
                total_requests: AtomicU64::new(0),
                successful_requests: AtomicU64::new(0),
                failed_requests: AtomicU64::new(0),
                latencies: RwLock::new(VecDeque::new()),
                errors: RwLock::new(Vec::new()),
                start_time: Instant::now(),
            }),
        }
    }

    /// Run load test with specified configuration
    pub async fn run_load_test(&self) -> Result<PerformanceTestResult> {
        info!(
            "Starting load test with {} concurrent users",
            self.config.concurrent_users
        );

        let test_start = Utc::now();
        let start_time = Instant::now();

        // Create a pool of virtual users
        let mut handles = Vec::new();

        // Calculate requests per user
        let requests_per_user = self.config.target_rps as usize / self.config.concurrent_users;
        let request_interval = Duration::from_secs(1) / requests_per_user as u32;

        // Ramp up users gradually
        let ramp_up_interval = self.config.ramp_up_time / self.config.concurrent_users as u32;

        for user_id in 0..self.config.concurrent_users {
            let repository = Arc::clone(&self.repository);
            let metrics = Arc::clone(&self.metrics);
            let test_duration = self.config.test_duration;
            let interval = request_interval;

            let handle = tokio::spawn(async move {
                // Wait for ramp-up
                time::sleep(ramp_up_interval * user_id as u32).await;

                let user_start = Instant::now();

                while user_start.elapsed() < test_duration {
                    let request_start = Instant::now();

                    // Simulate user operations
                    let result = Self::simulate_user_operation(&repository, user_id).await;

                    let latency_ms = request_start.elapsed().as_millis() as u64;

                    // Record metrics
                    metrics.total_requests.fetch_add(1, Ordering::Relaxed);

                    match result {
                        Ok(_) => {
                            metrics.successful_requests.fetch_add(1, Ordering::Relaxed);
                            let mut latencies = metrics.latencies.write().await;
                            latencies.push_back(latency_ms);

                            // Keep only last 10000 samples for percentile calculation
                            if latencies.len() > 10000 {
                                latencies.pop_front();
                            }
                        }
                        Err(e) => {
                            metrics.failed_requests.fetch_add(1, Ordering::Relaxed);
                            let mut errors = metrics.errors.write().await;
                            errors.push(e.to_string());
                        }
                    }

                    // Wait for next request interval
                    if request_start.elapsed() < interval {
                        time::sleep(interval - request_start.elapsed()).await;
                    }
                }

                debug!("User {} completed load test", user_id);
            });

            handles.push(handle);
        }

        // Wait for all users to complete
        for handle in handles {
            handle.await?;
        }

        let test_end = Utc::now();
        let duration = start_time.elapsed();

        // Calculate final metrics
        let metrics = self.calculate_metrics().await?;

        // Check for SLA violations
        let sla_violations = self.check_sla_violations(&metrics);
        let passed = sla_violations.is_empty();

        let result = PerformanceTestResult {
            test_name: "Load Test".to_string(),
            test_type: TestType::Load,
            start_time: test_start,
            end_time: test_end,
            duration,
            metrics,
            sla_violations,
            passed,
        };

        info!("Load test completed. Result: {:?}", result.passed);

        Ok(result)
    }

    /// Simulate a user operation (mix of reads, writes, searches)
    async fn simulate_user_operation(
        repository: &Arc<MemoryRepository>,
        user_id: usize,
    ) -> Result<()> {
        use rand::Rng;

        // Generate all random values before any await
        let (operation, importance_score1, query_num, importance_score2) = {
            let mut rng = rand::thread_rng();
            (
                rng.gen_range(0..100),
                rng.gen_range(0.0..1.0),
                rng.gen_range(0..100),
                rng.gen_range(0.0..1.0),
            )
        };

        // Create a realistic workload mix
        // 60% reads, 20% writes, 15% searches, 5% updates
        match operation {
            0..=59 => {
                // Read operation - try to read a random UUID (may not exist)
                let memory_id = uuid::Uuid::new_v4();
                let _ = repository.get_memory(memory_id).await;
            }
            60..=79 => {
                // Write operation
                let content = format!("Test content from user {} at {}", user_id, Utc::now());
                let request = CreateMemoryRequest {
                    content,
                    embedding: None,
                    tier: Some(MemoryTier::Working),
                    importance_score: Some(importance_score1),
                    metadata: Some(serde_json::json!({
                        "user_id": user_id,
                        "test": true
                    })),
                    parent_id: None,
                    expires_at: None,
                };
                repository.create_memory(request).await?;
            }
            80..=94 => {
                // Search operation
                let query = format!("test query {query_num}");
                let search_request = SearchRequest {
                    query_text: Some(query),
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
                repository.search_memories_simple(search_request).await?;
            }
            95..=99 => {
                // Update operation - try to update a random UUID (may not exist)
                let memory_id = uuid::Uuid::new_v4();
                if let Ok(_memory) = repository.get_memory(memory_id).await {
                    let update_request = UpdateMemoryRequest {
                        content: Some(format!("Updated content at {}", Utc::now())),
                        embedding: None,
                        tier: None,
                        importance_score: Some(importance_score2),
                        metadata: None,
                        expires_at: None,
                    };
                    repository.update_memory(memory_id, update_request).await?;
                }
            }
            _ => unreachable!(),
        }

        Ok(())
    }

    /// Calculate performance metrics from collected data
    async fn calculate_metrics(&self) -> Result<PerformanceMetrics> {
        let total_requests = self.metrics.total_requests.load(Ordering::Relaxed);
        let successful_requests = self.metrics.successful_requests.load(Ordering::Relaxed);
        let failed_requests = self.metrics.failed_requests.load(Ordering::Relaxed);

        let duration_secs = self.metrics.start_time.elapsed().as_secs_f64();
        let throughput_rps = total_requests as f64 / duration_secs;

        let error_rate = if total_requests > 0 {
            (failed_requests as f64 / total_requests as f64) * 100.0
        } else {
            0.0
        };

        // Calculate latency percentiles
        let latencies = self.metrics.latencies.read().await;
        let mut sorted_latencies: Vec<u64> = latencies.iter().cloned().collect();
        sorted_latencies.sort();

        let latency_p50_ms = Self::calculate_percentile(&sorted_latencies, 50.0);
        let latency_p95_ms = Self::calculate_percentile(&sorted_latencies, 95.0);
        let latency_p99_ms = Self::calculate_percentile(&sorted_latencies, 99.0);
        let latency_max_ms = sorted_latencies.last().cloned().unwrap_or(0);

        // TODO: Get actual CPU/memory metrics from system
        let cpu_usage_avg = 0.0;
        let memory_usage_avg = 0.0;

        // TODO: Get actual cache hit ratio from cache implementation
        let cache_hit_ratio = 0.0;

        // TODO: Get actual DB connection metrics
        let db_connections_used = 0;

        Ok(PerformanceMetrics {
            total_requests,
            successful_requests,
            failed_requests,
            throughput_rps,
            latency_p50_ms,
            latency_p95_ms,
            latency_p99_ms,
            latency_max_ms,
            error_rate,
            cpu_usage_avg,
            memory_usage_avg,
            cache_hit_ratio,
            db_connections_used,
            network_bytes_sent: 0,
            network_bytes_received: 0,
        })
    }

    /// Calculate percentile from sorted latencies
    fn calculate_percentile(sorted_latencies: &[u64], percentile: f64) -> u64 {
        if sorted_latencies.is_empty() {
            return 0;
        }

        let index = ((percentile / 100.0) * sorted_latencies.len() as f64) as usize;
        let index = index.min(sorted_latencies.len() - 1);

        sorted_latencies[index]
    }

    /// Check for SLA violations
    fn check_sla_violations(&self, metrics: &PerformanceMetrics) -> Vec<super::SlaViolation> {
        let mut violations = Vec::new();

        // Check latency SLAs
        if metrics.latency_p50_ms > 10 {
            violations.push(super::SlaViolation {
                metric: "P50 Latency".to_string(),
                threshold: 10.0,
                actual_value: metrics.latency_p50_ms as f64,
                severity: super::ViolationSeverity::Warning,
                timestamp: Utc::now(),
            });
        }

        if metrics.latency_p95_ms > 100 {
            violations.push(super::SlaViolation {
                metric: "P95 Latency".to_string(),
                threshold: 100.0,
                actual_value: metrics.latency_p95_ms as f64,
                severity: super::ViolationSeverity::Critical,
                timestamp: Utc::now(),
            });
        }

        if metrics.latency_p99_ms > 500 {
            violations.push(super::SlaViolation {
                metric: "P99 Latency".to_string(),
                threshold: 500.0,
                actual_value: metrics.latency_p99_ms as f64,
                severity: super::ViolationSeverity::Critical,
                timestamp: Utc::now(),
            });
        }

        // Check throughput SLA
        if metrics.throughput_rps < 100.0 {
            violations.push(super::SlaViolation {
                metric: "Throughput".to_string(),
                threshold: 100.0,
                actual_value: metrics.throughput_rps,
                severity: super::ViolationSeverity::Critical,
                timestamp: Utc::now(),
            });
        }

        // Check error rate SLA
        if metrics.error_rate > 1.0 {
            violations.push(super::SlaViolation {
                metric: "Error Rate".to_string(),
                threshold: 1.0,
                actual_value: metrics.error_rate,
                severity: super::ViolationSeverity::Critical,
                timestamp: Utc::now(),
            });
        }

        violations
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_percentile() {
        let latencies = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

        // 50th percentile of 10 elements = index 5 (6th element) = 6
        assert_eq!(LoadTester::calculate_percentile(&latencies, 50.0), 6);
        // 90th percentile of 10 elements = index 9 (10th element) = 10
        assert_eq!(LoadTester::calculate_percentile(&latencies, 90.0), 10);
        // 99th percentile of 10 elements = index 9 (clamped to last) = 10
        assert_eq!(LoadTester::calculate_percentile(&latencies, 99.0), 10);
    }

    #[test]
    fn test_calculate_percentile_empty() {
        let latencies = vec![];
        assert_eq!(LoadTester::calculate_percentile(&latencies, 50.0), 0);
    }
}
