//! Capacity planning and scaling predictions

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Capacity planner for predicting system scaling needs
pub struct CapacityPlanner {
    historical_data: VecDeque<SystemMetrics>,
    max_history: usize,
}

impl CapacityPlanner {
    pub fn new(max_history: usize) -> Self {
        Self {
            historical_data: VecDeque::with_capacity(max_history),
            max_history,
        }
    }

    /// Add system metrics data point
    pub fn add_metrics(&mut self, metrics: SystemMetrics) {
        if self.historical_data.len() >= self.max_history {
            self.historical_data.pop_front();
        }
        self.historical_data.push_back(metrics);
    }

    /// Generate capacity planning report
    pub fn generate_report(&self) -> CapacityPlanningReport {
        let current_metrics = self.get_current_metrics();
        let growth_trends = self.calculate_growth_trends();
        let predictions = self.generate_predictions(&growth_trends);
        let scaling_triggers = self.identify_scaling_triggers(&predictions);
        let recommendations = self.generate_scaling_recommendations(&predictions);

        CapacityPlanningReport {
            timestamp: chrono::Utc::now(),
            current_metrics,
            growth_trends,
            predictions,
            recommendations,
            scaling_triggers,
        }
    }

    /// Get current system metrics
    fn get_current_metrics(&self) -> SystemMetrics {
        self.historical_data
            .back()
            .cloned()
            .unwrap_or_else(SystemMetrics::default)
    }

    /// Calculate growth trends from historical data
    fn calculate_growth_trends(&self) -> GrowthTrends {
        if self.historical_data.len() < 2 {
            return GrowthTrends::default();
        }

        let first = self.historical_data.front().unwrap();
        let last = self.historical_data.back().unwrap();
        let duration_hours = (last.timestamp - first.timestamp).num_hours() as f64;

        if duration_hours == 0.0 {
            return GrowthTrends::default();
        }

        GrowthTrends {
            memory_growth_rate: (last.memory_usage as f64 - first.memory_usage as f64)
                / duration_hours,
            cpu_growth_rate: (last.cpu_usage - first.cpu_usage) / duration_hours,
            request_growth_rate: (last.requests_per_second - first.requests_per_second)
                / duration_hours,
            storage_growth_rate: (last.storage_usage as f64 - first.storage_usage as f64)
                / duration_hours,
            connection_growth_rate: (last.active_connections as f64
                - first.active_connections as f64)
                / duration_hours,
        }
    }

    /// Generate predictions based on growth trends
    fn generate_predictions(&self, trends: &GrowthTrends) -> ScalingPredictions {
        let current = self.get_current_metrics();

        // Predict when resources will be exhausted
        let memory_exhaustion_hours = if trends.memory_growth_rate > 0.0 {
            let remaining_memory = (current.max_memory - current.memory_usage) as f64;
            Some((remaining_memory / trends.memory_growth_rate) as u32)
        } else {
            None
        };

        let cpu_exhaustion_hours = if trends.cpu_growth_rate > 0.0 {
            let remaining_cpu = 100.0 - current.cpu_usage;
            Some((remaining_cpu / trends.cpu_growth_rate) as u32)
        } else {
            None
        };

        let storage_exhaustion_hours = if trends.storage_growth_rate > 0.0 {
            let remaining_storage = (current.max_storage - current.storage_usage) as f64;
            Some((remaining_storage / trends.storage_growth_rate) as u32)
        } else {
            None
        };

        // Predict future resource needs
        let memory_needed_7d =
            current.memory_usage as f64 + (trends.memory_growth_rate * 24.0 * 7.0);
        let cpu_needed_7d = current.cpu_usage + (trends.cpu_growth_rate * 24.0 * 7.0);
        let storage_needed_7d =
            current.storage_usage as f64 + (trends.storage_growth_rate * 24.0 * 7.0);

        let memory_needed_30d =
            current.memory_usage as f64 + (trends.memory_growth_rate * 24.0 * 30.0);
        let cpu_needed_30d = current.cpu_usage + (trends.cpu_growth_rate * 24.0 * 30.0);
        let storage_needed_30d =
            current.storage_usage as f64 + (trends.storage_growth_rate * 24.0 * 30.0);

        ScalingPredictions {
            memory_exhaustion_hours,
            cpu_exhaustion_hours,
            storage_exhaustion_hours,
            memory_needed_7d: memory_needed_7d as u64,
            cpu_needed_7d,
            storage_needed_7d: storage_needed_7d as u64,
            memory_needed_30d: memory_needed_30d as u64,
            cpu_needed_30d,
            storage_needed_30d: storage_needed_30d as u64,
            predicted_peak_rps: current.requests_per_second * 1.5, // Simple peak prediction
        }
    }

    /// Generate scaling recommendations
    fn generate_scaling_recommendations(
        &self,
        predictions: &ScalingPredictions,
    ) -> Vec<ScalingRecommendation> {
        let mut recommendations = Vec::new();

        // Check memory exhaustion
        if let Some(hours) = predictions.memory_exhaustion_hours {
            if hours < 24 {
                recommendations.push(ScalingRecommendation {
                    resource: ResourceType::Memory,
                    urgency: Urgency::Critical,
                    action: "Immediately increase memory allocation".to_string(),
                    reason: format!("Memory will be exhausted in {hours} hours"),
                    suggested_value: predictions.memory_needed_7d,
                });
            } else if hours < 168 {
                // 7 days
                recommendations.push(ScalingRecommendation {
                    resource: ResourceType::Memory,
                    urgency: Urgency::High,
                    action: "Plan memory upgrade within this week".to_string(),
                    reason: format!("Memory will be exhausted in {} days", hours / 24),
                    suggested_value: predictions.memory_needed_30d,
                });
            }
        }

        // Check CPU exhaustion
        if let Some(hours) = predictions.cpu_exhaustion_hours {
            if hours < 24 {
                recommendations.push(ScalingRecommendation {
                    resource: ResourceType::CPU,
                    urgency: Urgency::Critical,
                    action: "Add more CPU cores or scale horizontally".to_string(),
                    reason: format!("CPU will be saturated in {hours} hours"),
                    suggested_value: (predictions.cpu_needed_7d as u64).min(100),
                });
            }
        }

        // Check storage exhaustion
        if let Some(hours) = predictions.storage_exhaustion_hours {
            if hours < 168 {
                // 7 days
                recommendations.push(ScalingRecommendation {
                    resource: ResourceType::Storage,
                    urgency: if hours < 24 {
                        Urgency::Critical
                    } else {
                        Urgency::High
                    },
                    action: "Increase storage capacity".to_string(),
                    reason: format!("Storage will be full in {} days", hours / 24),
                    suggested_value: predictions.storage_needed_30d,
                });
            }
        }

        // Check if horizontal scaling is needed
        let current = self.get_current_metrics();
        if current.cpu_usage > 70.0 && current.memory_usage > current.max_memory * 70 / 100 {
            recommendations.push(ScalingRecommendation {
                resource: ResourceType::Instances,
                urgency: Urgency::Medium,
                action: "Consider horizontal scaling".to_string(),
                reason: "Both CPU and memory usage are high".to_string(),
                suggested_value: 2, // Suggest doubling instances
            });
        }

        recommendations
    }

    /// Identify scaling triggers
    fn identify_scaling_triggers(&self, predictions: &ScalingPredictions) -> Vec<ScalingTrigger> {
        let mut triggers = Vec::new();

        // Memory trigger
        triggers.push(ScalingTrigger {
            metric: "memory_usage_percent".to_string(),
            threshold: 80.0,
            action: ScalingAction::VerticalScale,
            cooldown_minutes: 30,
        });

        // CPU trigger
        triggers.push(ScalingTrigger {
            metric: "cpu_usage_percent".to_string(),
            threshold: 75.0,
            action: ScalingAction::HorizontalScale,
            cooldown_minutes: 15,
        });

        // Request rate trigger
        triggers.push(ScalingTrigger {
            metric: "requests_per_second".to_string(),
            threshold: predictions.predicted_peak_rps * 0.8,
            action: ScalingAction::HorizontalScale,
            cooldown_minutes: 10,
        });

        triggers
    }
}

/// System metrics snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetrics {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub cpu_usage: f64,
    pub memory_usage: u64,
    pub max_memory: u64,
    pub storage_usage: u64,
    pub max_storage: u64,
    pub requests_per_second: f64,
    pub active_connections: u32,
    pub error_rate: f64,
}

impl Default for SystemMetrics {
    fn default() -> Self {
        Self {
            timestamp: chrono::Utc::now(),
            cpu_usage: 0.0,
            memory_usage: 0,
            max_memory: 8_589_934_592, // 8 GB
            storage_usage: 0,
            max_storage: 107_374_182_400, // 100 GB
            requests_per_second: 0.0,
            active_connections: 0,
            error_rate: 0.0,
        }
    }
}

/// Growth trends analysis
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GrowthTrends {
    pub memory_growth_rate: f64,     // bytes per hour
    pub cpu_growth_rate: f64,        // percent per hour
    pub request_growth_rate: f64,    // requests/sec per hour
    pub storage_growth_rate: f64,    // bytes per hour
    pub connection_growth_rate: f64, // connections per hour
}

/// Scaling predictions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScalingPredictions {
    pub memory_exhaustion_hours: Option<u32>,
    pub cpu_exhaustion_hours: Option<u32>,
    pub storage_exhaustion_hours: Option<u32>,
    pub memory_needed_7d: u64,
    pub cpu_needed_7d: f64,
    pub storage_needed_7d: u64,
    pub memory_needed_30d: u64,
    pub cpu_needed_30d: f64,
    pub storage_needed_30d: u64,
    pub predicted_peak_rps: f64,
}

/// Scaling recommendation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScalingRecommendation {
    pub resource: ResourceType,
    pub urgency: Urgency,
    pub action: String,
    pub reason: String,
    pub suggested_value: u64,
}

/// Resource type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResourceType {
    CPU,
    Memory,
    Storage,
    Instances,
}

/// Urgency level
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Urgency {
    Low,
    Medium,
    High,
    Critical,
}

/// Scaling trigger
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScalingTrigger {
    pub metric: String,
    pub threshold: f64,
    pub action: ScalingAction,
    pub cooldown_minutes: u32,
}

/// Scaling action type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ScalingAction {
    HorizontalScale,
    VerticalScale,
    Alert,
}

/// Capacity planning report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapacityPlanningReport {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub current_metrics: SystemMetrics,
    pub growth_trends: GrowthTrends,
    pub predictions: ScalingPredictions,
    pub recommendations: Vec<ScalingRecommendation>,
    pub scaling_triggers: Vec<ScalingTrigger>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_capacity_planner() {
        let mut planner = CapacityPlanner::new(100);

        // Add some test metrics
        let base_time = Utc::now();

        planner.add_metrics(SystemMetrics {
            timestamp: base_time,
            cpu_usage: 30.0,
            memory_usage: 2_000_000_000,
            max_memory: 8_000_000_000,
            storage_usage: 10_000_000_000,
            max_storage: 100_000_000_000,
            requests_per_second: 100.0,
            active_connections: 20,
            error_rate: 0.1,
        });

        planner.add_metrics(SystemMetrics {
            timestamp: base_time + chrono::Duration::hours(1),
            cpu_usage: 35.0,
            memory_usage: 2_100_000_000,
            max_memory: 8_000_000_000,
            storage_usage: 10_500_000_000,
            max_storage: 100_000_000_000,
            requests_per_second: 110.0,
            active_connections: 22,
            error_rate: 0.15,
        });

        let report = planner.generate_report();

        assert!(report.growth_trends.cpu_growth_rate > 0.0);
        assert!(report.growth_trends.memory_growth_rate > 0.0);
        assert!(!report.recommendations.is_empty());
        assert!(!report.scaling_triggers.is_empty());
    }

    #[test]
    fn test_exhaustion_prediction() {
        let planner = CapacityPlanner::new(100);

        let trends = GrowthTrends {
            memory_growth_rate: 100_000_000.0, // 100 MB per hour
            cpu_growth_rate: 1.0,              // 1% per hour
            request_growth_rate: 10.0,
            storage_growth_rate: 1_000_000_000.0, // 1 GB per hour
            connection_growth_rate: 1.0,
        };

        let predictions = planner.generate_predictions(&trends);

        assert!(predictions.memory_exhaustion_hours.is_some());
        assert!(predictions.cpu_exhaustion_hours.is_some());
        assert!(predictions.storage_exhaustion_hours.is_some());
        assert!(predictions.memory_needed_7d > 0);
        assert!(predictions.cpu_needed_7d > 0.0);
    }
}
