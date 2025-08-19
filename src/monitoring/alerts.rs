use super::{
    AlertCondition, AlertRule, AlertSeverity, HealthStatus, PerformanceMetrics, SystemHealth,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{error, info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub id: String,
    pub rule_name: String,
    pub severity: AlertSeverity,
    pub condition: AlertCondition,
    pub message: String,
    pub value: f64,
    pub threshold: f64,
    pub triggered_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertManager {
    rules: Vec<AlertRule>,
    active_alerts: HashMap<String, Alert>,
    alert_history: Vec<Alert>,
    notification_channels: Vec<NotificationChannel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationChannel {
    pub name: String,
    pub channel_type: ChannelType,
    pub config: serde_json::Value,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChannelType {
    Log,
    Webhook,
    Email,
    Slack,
}

impl Default for AlertManager {
    fn default() -> Self {
        Self::new()
    }
}

impl AlertManager {
    pub fn new() -> Self {
        Self {
            rules: Self::default_alert_rules(),
            active_alerts: HashMap::new(),
            alert_history: Vec::new(),
            notification_channels: vec![NotificationChannel {
                name: "log".to_string(),
                channel_type: ChannelType::Log,
                config: serde_json::json!({}),
                enabled: true,
            }],
        }
    }

    /// Default set of alert rules for the memory system
    fn default_alert_rules() -> Vec<AlertRule> {
        vec![
            AlertRule {
                name: "high_memory_pressure".to_string(),
                condition: AlertCondition::MemoryPressure,
                threshold: 80.0, // 80% memory usage
                severity: AlertSeverity::Warning,
                enabled: true,
            },
            AlertRule {
                name: "critical_memory_pressure".to_string(),
                condition: AlertCondition::MemoryPressure,
                threshold: 95.0, // 95% memory usage
                severity: AlertSeverity::Critical,
                enabled: true,
            },
            AlertRule {
                name: "high_error_rate".to_string(),
                condition: AlertCondition::HighErrorRate,
                threshold: 5.0, // 5% error rate
                severity: AlertSeverity::Warning,
                enabled: true,
            },
            AlertRule {
                name: "critical_error_rate".to_string(),
                condition: AlertCondition::HighErrorRate,
                threshold: 10.0, // 10% error rate
                severity: AlertSeverity::Critical,
                enabled: true,
            },
            AlertRule {
                name: "slow_response_time".to_string(),
                condition: AlertCondition::SlowResponse,
                threshold: 1000.0, // 1 second
                severity: AlertSeverity::Warning,
                enabled: true,
            },
            AlertRule {
                name: "connection_pool_saturation".to_string(),
                condition: AlertCondition::ConnectionPoolSaturation,
                threshold: 90.0, // 90% pool utilization
                severity: AlertSeverity::Critical,
                enabled: true,
            },
            AlertRule {
                name: "migration_failures".to_string(),
                condition: AlertCondition::MigrationFailures,
                threshold: 10.0, // 10 failures per hour
                severity: AlertSeverity::Warning,
                enabled: true,
            },
            AlertRule {
                name: "disk_usage".to_string(),
                condition: AlertCondition::DiskUsage,
                threshold: 85.0, // 85% disk usage
                severity: AlertSeverity::Warning,
                enabled: true,
            },
        ]
    }

    /// Evaluate all alert rules against current system state
    pub fn evaluate_alerts(
        &mut self,
        health: &SystemHealth,
        _metrics: Option<&PerformanceMetrics>,
    ) {
        let now = Utc::now();

        for rule in &self.rules {
            if !rule.enabled {
                continue;
            }

            let alert_id = format!("{}_{}", rule.name, rule.condition.to_string());
            let should_trigger = self.evaluate_condition(rule, health);
            let is_active = self.active_alerts.contains_key(&alert_id);

            match (should_trigger, is_active) {
                (true, false) => {
                    // Trigger new alert
                    let value = self.get_condition_value(rule, health);
                    let alert = Alert {
                        id: alert_id.clone(),
                        rule_name: rule.name.clone(),
                        severity: rule.severity.clone(),
                        condition: rule.condition.clone(),
                        message: self.format_alert_message(rule, value),
                        value,
                        threshold: rule.threshold,
                        triggered_at: now,
                        resolved_at: None,
                        metadata: self.get_alert_metadata(rule, health),
                    };

                    self.active_alerts.insert(alert_id.clone(), alert.clone());
                    self.alert_history.push(alert.clone());
                    self.send_notification(&alert, true);

                    match alert.severity {
                        AlertSeverity::Critical => error!("CRITICAL ALERT: {}", alert.message),
                        AlertSeverity::Warning => warn!("WARNING ALERT: {}", alert.message),
                        AlertSeverity::Info => info!("INFO ALERT: {}", alert.message),
                    }
                }
                (false, true) => {
                    // Resolve active alert
                    if let Some(mut alert) = self.active_alerts.remove(&alert_id) {
                        alert.resolved_at = Some(now);
                        self.send_notification(&alert, false);
                        info!("RESOLVED ALERT: {}", alert.message);

                        // Update in history
                        if let Some(history_alert) = self
                            .alert_history
                            .iter_mut()
                            .find(|a| a.id == alert_id && a.resolved_at.is_none())
                        {
                            history_alert.resolved_at = Some(now);
                        }
                    }
                }
                _ => {
                    // No change needed
                }
            }
        }
    }

    /// Evaluate a specific alert condition
    fn evaluate_condition(&self, rule: &AlertRule, health: &SystemHealth) -> bool {
        match rule.condition {
            AlertCondition::MemoryPressure => {
                // Check if working tier has too many memories
                let working_component = health.components.get("memory_system");
                if let Some(component) = working_component {
                    match component.status {
                        HealthStatus::Degraded | HealthStatus::Unhealthy => true,
                        _ => false,
                    }
                } else {
                    false
                }
            }
            AlertCondition::HighErrorRate => {
                // Check database and memory system for errors
                let total_errors: u64 = health.components.values().map(|c| c.error_count).sum();
                (total_errors as f64) > rule.threshold
            }
            AlertCondition::SlowResponse => {
                // Check if any component has slow response times
                health.components.values().any(|component| {
                    component
                        .response_time_ms
                        .map(|rt| rt as f64 > rule.threshold)
                        .unwrap_or(false)
                })
            }
            AlertCondition::ConnectionPoolSaturation => {
                // Check connection pool utilization
                let pool_component = health.components.get("connection_pool");
                if let Some(component) = pool_component {
                    match component.status {
                        HealthStatus::Degraded | HealthStatus::Unhealthy => true,
                        _ => false,
                    }
                } else {
                    false
                }
            }
            AlertCondition::MigrationFailures => {
                // Check memory system for migration failures
                let memory_component = health.components.get("memory_system");
                if let Some(component) = memory_component {
                    component.error_count > rule.threshold as u64
                } else {
                    false
                }
            }
            AlertCondition::DiskUsage => {
                // Simplified disk usage check
                health.memory_usage_bytes
                    > (rule.threshold / 100.0 * 1024.0 * 1024.0 * 1024.0) as u64
            }
        }
    }

    /// Get the current value for an alert condition
    fn get_condition_value(&self, rule: &AlertRule, health: &SystemHealth) -> f64 {
        match rule.condition {
            AlertCondition::MemoryPressure => {
                (health.memory_usage_bytes as f64) / (1024.0 * 1024.0 * 1024.0) * 100.0
            }
            AlertCondition::HighErrorRate => health
                .components
                .values()
                .map(|c| c.error_count as f64)
                .sum(),
            AlertCondition::SlowResponse => health
                .components
                .values()
                .filter_map(|c| c.response_time_ms)
                .max()
                .unwrap_or(0) as f64,
            AlertCondition::ConnectionPoolSaturation => {
                // Would calculate actual pool utilization in production
                75.0 // Placeholder
            }
            AlertCondition::MigrationFailures => health
                .components
                .get("memory_system")
                .map(|c| c.error_count as f64)
                .unwrap_or(0.0),
            AlertCondition::DiskUsage => {
                (health.memory_usage_bytes as f64) / (1024.0 * 1024.0 * 1024.0) * 100.0
            }
        }
    }

    /// Format alert message
    fn format_alert_message(&self, rule: &AlertRule, value: f64) -> String {
        match rule.condition {
            AlertCondition::MemoryPressure => {
                format!(
                    "High memory pressure detected: {:.1}% (threshold: {:.1}%)",
                    value, rule.threshold
                )
            }
            AlertCondition::HighErrorRate => {
                format!(
                    "High error rate: {:.0} errors (threshold: {:.0})",
                    value, rule.threshold
                )
            }
            AlertCondition::SlowResponse => {
                format!(
                    "Slow response time: {:.0}ms (threshold: {:.0}ms)",
                    value, rule.threshold
                )
            }
            AlertCondition::ConnectionPoolSaturation => {
                format!(
                    "Connection pool saturation: {:.1}% (threshold: {:.1}%)",
                    value, rule.threshold
                )
            }
            AlertCondition::MigrationFailures => {
                format!(
                    "Migration failures: {:.0} failures (threshold: {:.0})",
                    value, rule.threshold
                )
            }
            AlertCondition::DiskUsage => {
                format!(
                    "High disk usage: {:.1}% (threshold: {:.1}%)",
                    value, rule.threshold
                )
            }
        }
    }

    /// Get metadata for alert
    fn get_alert_metadata(
        &self,
        _rule: &AlertRule,
        health: &SystemHealth,
    ) -> HashMap<String, String> {
        let mut metadata = HashMap::new();
        metadata.insert("timestamp".to_string(), health.timestamp.to_rfc3339());
        metadata.insert(
            "uptime_seconds".to_string(),
            health.uptime_seconds.to_string(),
        );
        metadata.insert("system_status".to_string(), format!("{:?}", health.status));
        metadata
    }

    /// Send notification through configured channels
    fn send_notification(&self, alert: &Alert, is_trigger: bool) {
        for channel in &self.notification_channels {
            if !channel.enabled {
                continue;
            }

            match channel.channel_type {
                ChannelType::Log => {
                    let action = if is_trigger { "TRIGGERED" } else { "RESOLVED" };
                    let log_message =
                        format!("[ALERT {}] {} - {}", action, alert.rule_name, alert.message);

                    match alert.severity {
                        AlertSeverity::Critical => error!("{}", log_message),
                        AlertSeverity::Warning => warn!("{}", log_message),
                        AlertSeverity::Info => info!("{}", log_message),
                    }
                }
                ChannelType::Webhook => {
                    // Would implement HTTP webhook in production
                    info!(
                        "Would send webhook notification for alert: {}",
                        alert.rule_name
                    );
                }
                ChannelType::Email => {
                    // Would implement email notification in production
                    info!(
                        "Would send email notification for alert: {}",
                        alert.rule_name
                    );
                }
                ChannelType::Slack => {
                    // Would implement Slack notification in production
                    info!(
                        "Would send Slack notification for alert: {}",
                        alert.rule_name
                    );
                }
            }
        }
    }

    /// Get all active alerts
    pub fn get_active_alerts(&self) -> Vec<&Alert> {
        self.active_alerts.values().collect()
    }

    /// Get alert history
    pub fn get_alert_history(&self, limit: Option<usize>) -> Vec<&Alert> {
        let mut history: Vec<_> = self.alert_history.iter().collect();
        history.sort_by(|a, b| b.triggered_at.cmp(&a.triggered_at));

        if let Some(limit) = limit {
            history.into_iter().take(limit).collect()
        } else {
            history
        }
    }

    /// Add or update alert rule
    pub fn add_rule(&mut self, rule: AlertRule) {
        if let Some(existing) = self.rules.iter_mut().find(|r| r.name == rule.name) {
            *existing = rule;
            info!("Updated alert rule: {}", existing.name);
        } else {
            info!("Added new alert rule: {}", rule.name);
            self.rules.push(rule);
        }
    }

    /// Remove alert rule
    pub fn remove_rule(&mut self, rule_name: &str) -> bool {
        let initial_len = self.rules.len();
        self.rules.retain(|rule| rule.name != rule_name);
        let removed = self.rules.len() < initial_len;

        if removed {
            info!("Removed alert rule: {}", rule_name);
        }

        removed
    }

    /// Clear old alerts from history
    pub fn cleanup_old_alerts(&mut self, max_age_hours: u32) {
        let cutoff = Utc::now() - chrono::Duration::hours(max_age_hours as i64);
        let initial_len = self.alert_history.len();

        self.alert_history
            .retain(|alert| alert.triggered_at > cutoff);

        let removed = initial_len - self.alert_history.len();
        if removed > 0 {
            info!("Cleaned up {} old alerts from history", removed);
        }
    }
}

impl AlertCondition {
    fn to_string(&self) -> String {
        match self {
            AlertCondition::MemoryPressure => "memory_pressure".to_string(),
            AlertCondition::HighErrorRate => "high_error_rate".to_string(),
            AlertCondition::SlowResponse => "slow_response".to_string(),
            AlertCondition::ConnectionPoolSaturation => "connection_pool_saturation".to_string(),
            AlertCondition::MigrationFailures => "migration_failures".to_string(),
            AlertCondition::DiskUsage => "disk_usage".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monitoring::ComponentHealth;
    use std::collections::HashMap;

    #[test]
    fn test_alert_manager_creation() {
        let manager = AlertManager::new();
        assert!(!manager.rules.is_empty());
        assert!(manager.active_alerts.is_empty());
        assert!(!manager.notification_channels.is_empty());
    }

    #[test]
    fn test_alert_rule_management() {
        let mut manager = AlertManager::new();
        let initial_count = manager.rules.len();

        let new_rule = AlertRule {
            name: "test_rule".to_string(),
            condition: AlertCondition::HighErrorRate,
            threshold: 15.0,
            severity: AlertSeverity::Warning,
            enabled: true,
        };

        manager.add_rule(new_rule);
        assert_eq!(manager.rules.len(), initial_count + 1);

        let removed = manager.remove_rule("test_rule");
        assert!(removed);
        assert_eq!(manager.rules.len(), initial_count);
    }

    #[test]
    fn test_alert_evaluation() {
        let mut manager = AlertManager::new();

        // Create a degraded health state
        let mut components = HashMap::new();
        components.insert(
            "memory_system".to_string(),
            ComponentHealth {
                status: HealthStatus::Degraded,
                message: Some("Test degradation".to_string()),
                last_checked: Utc::now(),
                response_time_ms: Some(500),
                error_count: 15,
            },
        );

        let health = SystemHealth {
            status: HealthStatus::Degraded,
            timestamp: Utc::now(),
            components,
            uptime_seconds: 3600,
            memory_usage_bytes: 1024 * 1024 * 1024, // 1GB
            cpu_usage_percent: 75.0,
        };

        let initial_alerts = manager.active_alerts.len();
        manager.evaluate_alerts(&health, None);

        // Should have triggered some alerts
        assert!(manager.active_alerts.len() > initial_alerts);
        assert!(!manager.alert_history.is_empty());
    }

    #[test]
    fn test_alert_cleanup() {
        let mut manager = AlertManager::new();

        // Add some old alerts to history
        let old_alert = Alert {
            id: "test_alert".to_string(),
            rule_name: "test_rule".to_string(),
            severity: AlertSeverity::Warning,
            condition: AlertCondition::HighErrorRate,
            message: "Test alert".to_string(),
            value: 10.0,
            threshold: 5.0,
            triggered_at: Utc::now() - chrono::Duration::hours(25), // 25 hours ago
            resolved_at: None,
            metadata: HashMap::new(),
        };

        manager.alert_history.push(old_alert);
        assert_eq!(manager.alert_history.len(), 1);

        manager.cleanup_old_alerts(24); // Remove alerts older than 24 hours
        assert_eq!(manager.alert_history.len(), 0);
    }
}
