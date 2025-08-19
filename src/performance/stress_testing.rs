//! Stress testing implementation for finding system breaking points

use super::{PerformanceMetrics, PerformanceTestResult, StressTestConfig, TestType};
use crate::memory::models::{CreateMemoryRequest, SearchRequest};
use crate::memory::{MemoryRepository, MemoryTier};
use anyhow::Result;
use chrono::Utc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use sysinfo::System;
use tokio::sync::{RwLock, Semaphore};
use tokio::time;
use tracing::{debug, info, warn};

/// Stress testing orchestrator
pub struct StressTester {
    config: StressTestConfig,
    repository: Arc<MemoryRepository>,
    metrics: Arc<StressTestMetrics>,
    connection_semaphore: Arc<Semaphore>,
}

/// Metrics collected during stress testing
struct StressTestMetrics {
    total_requests: AtomicU64,
    successful_requests: AtomicU64,
    failed_requests: AtomicU64,
    current_connections: AtomicU64,
    max_connections_reached: AtomicU64,
    system_metrics: RwLock<SystemMetrics>,
    breaking_point_found: AtomicBool,
    breaking_point_connections: AtomicU64,
}

#[derive(Debug, Default)]
struct SystemMetrics {
    cpu_samples: Vec<f32>,
    memory_samples: Vec<u64>,
    max_cpu_usage: f32,
    max_memory_usage: u64,
}

impl StressTester {
    pub fn new(config: StressTestConfig, repository: Arc<MemoryRepository>) -> Self {
        // Limit concurrent connections to avoid pool exhaustion
        // Use 70% of max connections as per CLAUDE.md best practices
        let max_concurrent = (config.max_connections as f64 * 0.7) as usize;

        Self {
            config,
            repository,
            metrics: Arc::new(StressTestMetrics {
                total_requests: AtomicU64::new(0),
                successful_requests: AtomicU64::new(0),
                failed_requests: AtomicU64::new(0),
                current_connections: AtomicU64::new(0),
                max_connections_reached: AtomicU64::new(0),
                system_metrics: RwLock::new(SystemMetrics::default()),
                breaking_point_found: AtomicBool::new(false),
                breaking_point_connections: AtomicU64::new(0),
            }),
            connection_semaphore: Arc::new(Semaphore::new(max_concurrent.max(10))),
        }
    }

    /// Run stress test to find breaking points
    pub async fn run_stress_test(&self) -> Result<PerformanceTestResult> {
        info!(
            "Starting stress test with max {} connections",
            self.config.max_connections
        );

        let test_start = Utc::now();
        let start_time = Instant::now();

        // Start system monitoring
        let monitor_handle = self.start_system_monitoring();

        // Gradually increase load until breaking point
        let result = self.find_breaking_point().await;

        // Stop monitoring
        self.metrics
            .breaking_point_found
            .store(true, Ordering::Relaxed);
        monitor_handle.await?;

        let test_end = Utc::now();
        let duration = start_time.elapsed();

        // Calculate final metrics
        let metrics = self.calculate_metrics().await?;

        // Check for stress test specific violations
        let sla_violations = self.check_stress_violations(&metrics);

        let result = PerformanceTestResult {
            test_name: "Stress Test".to_string(),
            test_type: TestType::Stress,
            start_time: test_start,
            end_time: test_end,
            duration,
            metrics,
            sla_violations,
            passed: result.is_ok(),
        };

        info!(
            "Stress test completed. Breaking point: {} connections",
            self.metrics
                .breaking_point_connections
                .load(Ordering::Relaxed)
        );

        Ok(result)
    }

    /// Find the system breaking point by gradually increasing load
    async fn find_breaking_point(&self) -> Result<()> {
        let mut current_load = 10;
        let load_increment = 10;
        let stabilization_time = Duration::from_secs(5);

        while current_load <= self.config.max_connections {
            info!("Testing with {} concurrent connections", current_load);

            // Spawn connections
            let mut handles = Vec::new();

            for conn_id in 0..current_load {
                let repository = Arc::clone(&self.repository);
                let metrics = Arc::clone(&self.metrics);
                let semaphore = Arc::clone(&self.connection_semaphore);
                let should_stop = Arc::new(AtomicBool::new(false));
                let stop_clone = Arc::clone(&should_stop);

                let handle = tokio::spawn(async move {
                    // Acquire semaphore permit to limit concurrent connections
                    let _permit = match semaphore.acquire().await {
                        Ok(permit) => permit,
                        Err(_) => {
                            debug!("Failed to acquire semaphore permit");
                            return;
                        }
                    };

                    metrics.current_connections.fetch_add(1, Ordering::Relaxed);

                    // Update max connections if needed
                    let current = metrics.current_connections.load(Ordering::Relaxed);
                    let mut max = metrics.max_connections_reached.load(Ordering::Relaxed);
                    while current > max {
                        match metrics.max_connections_reached.compare_exchange(
                            max,
                            current,
                            Ordering::Relaxed,
                            Ordering::Relaxed,
                        ) {
                            Ok(_) => break,
                            Err(x) => max = x,
                        }
                    }

                    // Continuously make requests
                    while !stop_clone.load(Ordering::Relaxed) {
                        let request_start = Instant::now();

                        let result = Self::stress_operation(&repository, conn_id).await;

                        metrics.total_requests.fetch_add(1, Ordering::Relaxed);

                        match result {
                            Ok(_) => {
                                metrics.successful_requests.fetch_add(1, Ordering::Relaxed);
                            }
                            Err(e) => {
                                metrics.failed_requests.fetch_add(1, Ordering::Relaxed);
                                debug!("Request failed: {}", e);
                            }
                        }

                        // Small delay to prevent CPU spinning
                        if request_start.elapsed() < Duration::from_millis(10) {
                            time::sleep(Duration::from_millis(10)).await;
                        }
                    }

                    metrics.current_connections.fetch_sub(1, Ordering::Relaxed);
                });

                handles.push((handle, should_stop));
            }

            // Let the system stabilize
            time::sleep(stabilization_time).await;

            // Check if system is under stress
            if self.is_system_under_stress().await {
                warn!("System under stress at {} connections", current_load);
                self.metrics
                    .breaking_point_found
                    .store(true, Ordering::Relaxed);
                self.metrics
                    .breaking_point_connections
                    .store(current_load as u64, Ordering::Relaxed);

                // Stop all connections
                for (_, should_stop) in &handles {
                    should_stop.store(true, Ordering::Relaxed);
                }

                // Wait for all to complete
                for (handle, _) in handles {
                    handle.await?;
                }

                break;
            }

            // Check error rate
            let total = self.metrics.total_requests.load(Ordering::Relaxed);
            let failed = self.metrics.failed_requests.load(Ordering::Relaxed);

            if total > 0 && (failed as f64 / total as f64) > 0.1 {
                warn!("High error rate detected at {} connections", current_load);
                self.metrics
                    .breaking_point_found
                    .store(true, Ordering::Relaxed);
                self.metrics
                    .breaking_point_connections
                    .store(current_load as u64, Ordering::Relaxed);

                // Stop all connections
                for (_, should_stop) in &handles {
                    should_stop.store(true, Ordering::Relaxed);
                }

                // Wait for all to complete
                for (handle, _) in handles {
                    handle.await?;
                }

                break;
            }

            // Stop current load level
            for (_, should_stop) in &handles {
                should_stop.store(true, Ordering::Relaxed);
            }

            // Wait for all to complete
            for (handle, _) in handles {
                handle.await?;
            }

            // Increase load
            current_load += load_increment;
        }

        Ok(())
    }

    /// Perform a stress test operation
    async fn stress_operation(repository: &Arc<MemoryRepository>, conn_id: usize) -> Result<()> {
        use rand::Rng;

        // Generate random value before any await
        let operation = {
            let mut rng = rand::thread_rng();
            rng.gen_range(0..3)
        };

        // Heavy operations to stress the system
        match operation {
            0 => {
                // Bulk write
                for i in 0..10 {
                    let content = format!("{}_{}_{}_{}", "x".repeat(250), conn_id, i, Utc::now());
                    let request = CreateMemoryRequest {
                        content,
                        embedding: None,
                        tier: Some(MemoryTier::Working),
                        importance_score: Some(0.5),
                        metadata: Some(
                            serde_json::json!({"stress_test": true, "conn_id": conn_id}),
                        ),
                        parent_id: None,
                        expires_at: None,
                    };
                    repository.create_memory(request).await?;
                }
            }
            1 => {
                // Complex search
                let query_num = {
                    let mut rng = rand::thread_rng();
                    rng.gen_range(0..100)
                };
                let query = format!("stress test query {query_num}");
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
                    limit: Some(100),
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
            2 => {
                // Bulk read - attempt to read random UUIDs (most will fail, which is fine for stress testing)
                for _ in 0..20 {
                    let id = uuid::Uuid::new_v4();
                    let _ = repository.get_memory(id).await;
                }
            }
            _ => unreachable!(),
        }

        Ok(())
    }

    /// Check if system is under stress based on CPU and memory thresholds
    async fn is_system_under_stress(&self) -> bool {
        let metrics = self.metrics.system_metrics.read().await;

        // Check CPU threshold
        if metrics.max_cpu_usage > self.config.cpu_pressure_threshold as f32 {
            return true;
        }

        // Check memory threshold
        let mut sys = System::new();
        sys.refresh_memory();

        let total_memory = sys.total_memory();
        let used_memory = sys.used_memory();
        let memory_usage_percent = (used_memory as f64 / total_memory as f64) * 100.0;

        memory_usage_percent > self.config.memory_pressure_threshold as f64
    }

    /// Start monitoring system metrics
    fn start_system_monitoring(&self) -> tokio::task::JoinHandle<()> {
        let metrics = Arc::clone(&self.metrics);

        tokio::spawn(async move {
            let mut sys = System::new();
            let mut interval = time::interval(Duration::from_secs(1));

            while !metrics.breaking_point_found.load(Ordering::Relaxed) {
                interval.tick().await;

                // Refresh system info
                sys.refresh_cpu();
                sys.refresh_memory();

                // Get CPU usage - calculate average across all CPUs
                let cpu_usage = sys.cpus().iter().map(|cpu| cpu.cpu_usage()).sum::<f32>()
                    / sys.cpus().len() as f32;

                // Get memory usage
                let used_memory = sys.used_memory();

                // Store samples
                let mut system_metrics = metrics.system_metrics.write().await;
                system_metrics.cpu_samples.push(cpu_usage);
                system_metrics.memory_samples.push(used_memory);

                // Update maximums
                if cpu_usage > system_metrics.max_cpu_usage {
                    system_metrics.max_cpu_usage = cpu_usage;
                }

                if used_memory > system_metrics.max_memory_usage {
                    system_metrics.max_memory_usage = used_memory;
                }

                // Keep only last 60 samples
                if system_metrics.cpu_samples.len() > 60 {
                    system_metrics.cpu_samples.remove(0);
                }
                if system_metrics.memory_samples.len() > 60 {
                    system_metrics.memory_samples.remove(0);
                }
            }
        })
    }

    /// Calculate performance metrics from stress test
    async fn calculate_metrics(&self) -> Result<PerformanceMetrics> {
        let total_requests = self.metrics.total_requests.load(Ordering::Relaxed);
        let successful_requests = self.metrics.successful_requests.load(Ordering::Relaxed);
        let failed_requests = self.metrics.failed_requests.load(Ordering::Relaxed);

        let error_rate = if total_requests > 0 {
            (failed_requests as f64 / total_requests as f64) * 100.0
        } else {
            0.0
        };

        let system_metrics = self.metrics.system_metrics.read().await;

        // Calculate averages
        let cpu_usage_avg = if !system_metrics.cpu_samples.is_empty() {
            system_metrics.cpu_samples.iter().sum::<f32>() / system_metrics.cpu_samples.len() as f32
        } else {
            0.0
        } as f64;

        let memory_usage_avg = if !system_metrics.memory_samples.is_empty() {
            system_metrics.memory_samples.iter().sum::<u64>()
                / system_metrics.memory_samples.len() as u64
        } else {
            0
        } as f64;

        Ok(PerformanceMetrics {
            total_requests,
            successful_requests,
            failed_requests,
            throughput_rps: 0.0, // Not relevant for stress test
            latency_p50_ms: 0,
            latency_p95_ms: 0,
            latency_p99_ms: 0,
            latency_max_ms: 0,
            error_rate,
            cpu_usage_avg,
            memory_usage_avg,
            cache_hit_ratio: 0.0,
            db_connections_used: self.metrics.max_connections_reached.load(Ordering::Relaxed)
                as u32,
            network_bytes_sent: 0,
            network_bytes_received: 0,
        })
    }

    /// Check for stress test specific violations
    fn check_stress_violations(&self, metrics: &PerformanceMetrics) -> Vec<super::SlaViolation> {
        let mut violations = Vec::new();

        // Check if breaking point was found too early
        let breaking_point = self
            .metrics
            .breaking_point_connections
            .load(Ordering::Relaxed);

        if breaking_point > 0 && breaking_point < 100 {
            violations.push(super::SlaViolation {
                metric: "Breaking Point".to_string(),
                threshold: 100.0,
                actual_value: breaking_point as f64,
                severity: super::ViolationSeverity::Critical,
                timestamp: Utc::now(),
            });
        }

        // Check error rate under stress
        if metrics.error_rate > 10.0 {
            violations.push(super::SlaViolation {
                metric: "Stress Error Rate".to_string(),
                threshold: 10.0,
                actual_value: metrics.error_rate,
                severity: super::ViolationSeverity::Warning,
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
    fn test_stress_config_defaults() {
        let config = StressTestConfig::default();
        assert_eq!(config.max_connections, 10000);
        assert_eq!(config.memory_pressure_threshold, 80);
        assert_eq!(config.cpu_pressure_threshold, 90);
        assert!(!config.chaos_testing_enabled);
    }
}
