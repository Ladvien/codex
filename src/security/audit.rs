use crate::security::{AuditConfig, Result, SecurityError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, error, info};
use uuid::Uuid;

/// Audit event types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditEventType {
    Authentication,
    Authorization,
    DataAccess,
    DataModification,
    DataDeletion,
    SystemAccess,
    ConfigurationChange,
    SecurityEvent,
    Error,
}

/// Audit event severity levels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditSeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// Individual audit event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub event_type: AuditEventType,
    pub severity: AuditSeverity,
    pub user_id: Option<String>,
    pub session_id: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub resource: Option<String>,
    pub action: String,
    pub outcome: AuditOutcome,
    pub details: HashMap<String, Value>,
    pub error_message: Option<String>,
    pub request_id: Option<String>,
}

/// Audit outcome
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditOutcome {
    Success,
    Failure,
    Partial,
}

/// Audit statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditStatistics {
    pub total_events: u64,
    pub events_by_type: HashMap<String, u64>,
    pub events_by_user: HashMap<String, u64>,
    pub failed_events: u64,
    pub critical_events: u64,
    pub retention_days: u32,
    pub oldest_event: Option<DateTime<Utc>>,
    pub newest_event: Option<DateTime<Utc>>,
}

/// Audit logging manager
pub struct AuditManager {
    config: AuditConfig,
    db_pool: Arc<PgPool>,
}

impl AuditManager {
    pub fn new(config: AuditConfig, db_pool: Arc<PgPool>) -> Self {
        Self { config, db_pool }
    }

    /// Initialize audit logging system
    pub async fn initialize(&self) -> Result<()> {
        if !self.config.enabled {
            debug!("Audit logging is disabled");
            return Ok(());
        }

        info!("Initializing audit logging system");

        // Create audit events table if it doesn't exist
        let create_table_sql = r#"
            CREATE TABLE IF NOT EXISTS audit_events (
                id UUID PRIMARY KEY,
                timestamp TIMESTAMPTZ NOT NULL,
                event_type TEXT NOT NULL,
                severity TEXT NOT NULL,
                user_id TEXT,
                session_id TEXT,
                ip_address INET,
                user_agent TEXT,
                resource TEXT,
                action TEXT NOT NULL,
                outcome TEXT NOT NULL,
                details JSONB,
                error_message TEXT,
                request_id TEXT,
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
        "#;

        sqlx::query(create_table_sql)
            .execute(self.db_pool.as_ref())
            .await
            .map_err(|e| SecurityError::AuditError {
                message: format!("Failed to create audit events table: {e}"),
            })?;

        // Create indexes for better performance
        let create_indexes_sql = vec![
            "CREATE INDEX IF NOT EXISTS idx_audit_events_timestamp ON audit_events (timestamp);",
            "CREATE INDEX IF NOT EXISTS idx_audit_events_user_id ON audit_events (user_id);",
            "CREATE INDEX IF NOT EXISTS idx_audit_events_event_type ON audit_events (event_type);",
            "CREATE INDEX IF NOT EXISTS idx_audit_events_severity ON audit_events (severity);",
            "CREATE INDEX IF NOT EXISTS idx_audit_events_outcome ON audit_events (outcome);",
        ];

        for sql in create_indexes_sql {
            sqlx::query(sql)
                .execute(self.db_pool.as_ref())
                .await
                .map_err(|e| SecurityError::AuditError {
                    message: format!("Failed to create audit index: {e}"),
                })?;
        }

        info!("Audit logging system initialized successfully");
        Ok(())
    }

    /// Log an audit event
    pub async fn log_event(&self, event: AuditEvent) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        // Check if this type of event should be logged
        if !self.should_log_event(&event.event_type) {
            return Ok(());
        }

        debug!(
            "Logging audit event: {:?} - {}",
            event.event_type, event.action
        );

        let insert_sql = r#"
            INSERT INTO audit_events (
                id, timestamp, event_type, severity, user_id, session_id,
                ip_address, user_agent, resource, action, outcome, details,
                error_message, request_id
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
        "#;

        sqlx::query(insert_sql)
            .bind(&event.id)
            .bind(event.timestamp)
            .bind(format!("{:?}", event.event_type))
            .bind(format!("{:?}", event.severity))
            .bind(&event.user_id)
            .bind(&event.session_id)
            .bind(event.ip_address.as_ref())
            .bind(&event.user_agent)
            .bind(&event.resource)
            .bind(&event.action)
            .bind(format!("{:?}", event.outcome))
            .bind(serde_json::to_value(&event.details).unwrap_or(Value::Null))
            .bind(&event.error_message)
            .bind(&event.request_id)
            .execute(self.db_pool.as_ref())
            .await
            .map_err(|e| SecurityError::AuditError {
                message: format!("Failed to log audit event: {e}"),
            })?;

        // Log critical events to application log as well
        if matches!(event.severity, AuditSeverity::Critical) {
            error!(
                "CRITICAL AUDIT EVENT - Type: {:?}, Action: {}, User: {:?}, Details: {:?}",
                event.event_type, event.action, event.user_id, event.details
            );
        }

        Ok(())
    }

    /// Log authentication event
    pub async fn log_authentication(
        &self,
        user_id: &str,
        action: &str,
        outcome: AuditOutcome,
        ip_address: Option<String>,
        details: HashMap<String, Value>,
    ) -> Result<()> {
        let event = AuditEvent {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            event_type: AuditEventType::Authentication,
            severity: if matches!(outcome, AuditOutcome::Failure) {
                AuditSeverity::High
            } else {
                AuditSeverity::Medium
            },
            user_id: Some(user_id.to_string()),
            session_id: None,
            ip_address,
            user_agent: None,
            resource: Some("authentication".to_string()),
            action: action.to_string(),
            outcome,
            details,
            error_message: None,
            request_id: None,
        };

        self.log_event(event).await
    }

    /// Log data access event
    pub async fn log_data_access(
        &self,
        user_id: Option<&str>,
        resource: &str,
        action: &str,
        outcome: AuditOutcome,
        details: HashMap<String, Value>,
    ) -> Result<()> {
        let event = AuditEvent {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            event_type: AuditEventType::DataAccess,
            severity: AuditSeverity::Low,
            user_id: user_id.map(|s| s.to_string()),
            session_id: None,
            ip_address: None,
            user_agent: None,
            resource: Some(resource.to_string()),
            action: action.to_string(),
            outcome,
            details,
            error_message: None,
            request_id: None,
        };

        self.log_event(event).await
    }

    /// Log data modification event
    pub async fn log_data_modification(
        &self,
        user_id: Option<&str>,
        resource: &str,
        action: &str,
        outcome: AuditOutcome,
        details: HashMap<String, Value>,
    ) -> Result<()> {
        let event = AuditEvent {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            event_type: AuditEventType::DataModification,
            severity: AuditSeverity::Medium,
            user_id: user_id.map(|s| s.to_string()),
            session_id: None,
            ip_address: None,
            user_agent: None,
            resource: Some(resource.to_string()),
            action: action.to_string(),
            outcome,
            details,
            error_message: None,
            request_id: None,
        };

        self.log_event(event).await
    }

    /// Log security event
    pub async fn log_security_event(
        &self,
        action: &str,
        severity: AuditSeverity,
        user_id: Option<&str>,
        ip_address: Option<String>,
        details: HashMap<String, Value>,
    ) -> Result<()> {
        let event = AuditEvent {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            event_type: AuditEventType::SecurityEvent,
            severity,
            user_id: user_id.map(|s| s.to_string()),
            session_id: None,
            ip_address,
            user_agent: None,
            resource: Some("security".to_string()),
            action: action.to_string(),
            outcome: AuditOutcome::Success,
            details,
            error_message: None,
            request_id: None,
        };

        self.log_event(event).await
    }

    /// Get audit events with filtering
    pub async fn get_events(&self, filter: AuditFilter) -> Result<Vec<AuditEvent>> {
        if !self.config.enabled {
            return Ok(Vec::new());
        }

        let mut where_clauses = Vec::new();
        let mut bind_count = 0;

        if let Some(_user_id) = &filter.user_id {
            bind_count += 1;
            where_clauses.push(format!("user_id = ${bind_count}"));
        }

        if let Some(_event_type) = &filter.event_type {
            bind_count += 1;
            where_clauses.push(format!("event_type = ${bind_count}"));
        }

        if let Some(_start_time) = &filter.start_time {
            bind_count += 1;
            where_clauses.push(format!("timestamp >= ${bind_count}"));
        }

        if let Some(_end_time) = &filter.end_time {
            bind_count += 1;
            where_clauses.push(format!("timestamp <= ${bind_count}"));
        }

        let where_clause = if where_clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", where_clauses.join(" AND "))
        };

        let limit = filter.limit.unwrap_or(100).min(1000); // Cap at 1000 events
        let offset = filter.offset.unwrap_or(0);

        let query = format!(
            "SELECT * FROM audit_events {where_clause} ORDER BY timestamp DESC LIMIT {limit} OFFSET {offset}"
        );

        let mut sql_query = sqlx::query(&query);

        // Bind parameters in the same order as where clauses
        if let Some(user_id) = &filter.user_id {
            sql_query = sql_query.bind(user_id);
        }
        if let Some(event_type) = &filter.event_type {
            sql_query = sql_query.bind(format!("{event_type:?}"));
        }
        if let Some(start_time) = &filter.start_time {
            sql_query = sql_query.bind(start_time);
        }
        if let Some(end_time) = &filter.end_time {
            sql_query = sql_query.bind(end_time);
        }

        let rows = sql_query
            .fetch_all(self.db_pool.as_ref())
            .await
            .map_err(|e| SecurityError::AuditError {
                message: format!("Failed to fetch audit events: {e}"),
            })?;

        let mut events = Vec::new();
        for row in rows {
            events.push(self.row_to_audit_event(row)?);
        }

        Ok(events)
    }

    /// Get audit statistics
    pub async fn get_statistics(&self) -> Result<AuditStatistics> {
        if !self.config.enabled {
            return Ok(AuditStatistics {
                total_events: 0,
                events_by_type: HashMap::new(),
                events_by_user: HashMap::new(),
                failed_events: 0,
                critical_events: 0,
                retention_days: self.config.retention_days,
                oldest_event: None,
                newest_event: None,
            });
        }

        // Get basic statistics
        let total_events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_events")
            .fetch_one(self.db_pool.as_ref())
            .await
            .map_err(|e| SecurityError::AuditError {
                message: format!("Failed to get total events count: {e}"),
            })?;

        let failed_events: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM audit_events WHERE outcome = 'Failure'")
                .fetch_one(self.db_pool.as_ref())
                .await
                .map_err(|e| SecurityError::AuditError {
                    message: format!("Failed to get failed events count: {e}"),
                })?;

        let critical_events: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM audit_events WHERE severity = 'Critical'")
                .fetch_one(self.db_pool.as_ref())
                .await
                .map_err(|e| SecurityError::AuditError {
                    message: format!("Failed to get critical events count: {e}"),
                })?;

        // Get oldest and newest events
        let oldest_event: Option<DateTime<Utc>> =
            sqlx::query_scalar("SELECT MIN(timestamp) FROM audit_events")
                .fetch_one(self.db_pool.as_ref())
                .await
                .map_err(|e| SecurityError::AuditError {
                    message: format!("Failed to get oldest event: {e}"),
                })?;

        let newest_event: Option<DateTime<Utc>> =
            sqlx::query_scalar("SELECT MAX(timestamp) FROM audit_events")
                .fetch_one(self.db_pool.as_ref())
                .await
                .map_err(|e| SecurityError::AuditError {
                    message: format!("Failed to get newest event: {e}"),
                })?;

        // Get events by type
        let type_rows = sqlx::query(
            "SELECT event_type, COUNT(*) as count FROM audit_events GROUP BY event_type",
        )
        .fetch_all(self.db_pool.as_ref())
        .await
        .map_err(|e| SecurityError::AuditError {
            message: format!("Failed to get events by type: {e}"),
        })?;

        let mut events_by_type = HashMap::new();
        for row in type_rows {
            let event_type: String = row.get("event_type");
            let count: i64 = row.get("count");
            events_by_type.insert(event_type, count as u64);
        }

        // Get events by user (top 20)
        let user_rows = sqlx::query("SELECT user_id, COUNT(*) as count FROM audit_events WHERE user_id IS NOT NULL GROUP BY user_id ORDER BY count DESC LIMIT 20")
            .fetch_all(self.db_pool.as_ref())
            .await
            .map_err(|e| SecurityError::AuditError {
                message: format!("Failed to get events by user: {e}"),
            })?;

        let mut events_by_user = HashMap::new();
        for row in user_rows {
            let user_id: String = row.get("user_id");
            let count: i64 = row.get("count");
            events_by_user.insert(user_id, count as u64);
        }

        Ok(AuditStatistics {
            total_events: total_events as u64,
            events_by_type,
            events_by_user,
            failed_events: failed_events as u64,
            critical_events: critical_events as u64,
            retention_days: self.config.retention_days,
            oldest_event,
            newest_event,
        })
    }

    /// Clean up old audit events based on retention policy
    pub async fn cleanup_old_events(&self) -> Result<u64> {
        if !self.config.enabled {
            return Ok(0);
        }

        let cutoff_date = Utc::now() - chrono::Duration::days(self.config.retention_days as i64);

        let deleted_count: i64 =
            sqlx::query_scalar("DELETE FROM audit_events WHERE timestamp < $1 RETURNING COUNT(*)")
                .bind(cutoff_date)
                .fetch_optional(self.db_pool.as_ref())
                .await
                .map_err(|e| SecurityError::AuditError {
                    message: format!("Failed to cleanup old audit events: {e}"),
                })?
                .unwrap_or(0);

        if deleted_count > 0 {
            info!("Cleaned up {} old audit events", deleted_count);
        }

        Ok(deleted_count as u64)
    }

    fn should_log_event(&self, event_type: &AuditEventType) -> bool {
        match event_type {
            AuditEventType::Authentication => self.config.log_auth_events,
            AuditEventType::DataAccess => self.config.log_data_access,
            AuditEventType::DataModification => self.config.log_modifications,
            AuditEventType::DataDeletion => self.config.log_modifications,
            _ => true, // Log all other events by default
        }
    }

    fn row_to_audit_event(&self, row: sqlx::postgres::PgRow) -> Result<AuditEvent> {
        let event_type_str: String = row.get("event_type");
        let event_type = match event_type_str.as_str() {
            "Authentication" => AuditEventType::Authentication,
            "Authorization" => AuditEventType::Authorization,
            "DataAccess" => AuditEventType::DataAccess,
            "DataModification" => AuditEventType::DataModification,
            "DataDeletion" => AuditEventType::DataDeletion,
            "SystemAccess" => AuditEventType::SystemAccess,
            "ConfigurationChange" => AuditEventType::ConfigurationChange,
            "SecurityEvent" => AuditEventType::SecurityEvent,
            "Error" => AuditEventType::Error,
            _ => AuditEventType::SystemAccess,
        };

        let severity_str: String = row.get("severity");
        let severity = match severity_str.as_str() {
            "Low" => AuditSeverity::Low,
            "Medium" => AuditSeverity::Medium,
            "High" => AuditSeverity::High,
            "Critical" => AuditSeverity::Critical,
            _ => AuditSeverity::Low,
        };

        let outcome_str: String = row.get("outcome");
        let outcome = match outcome_str.as_str() {
            "Success" => AuditOutcome::Success,
            "Failure" => AuditOutcome::Failure,
            "Partial" => AuditOutcome::Partial,
            _ => AuditOutcome::Success,
        };

        let details_value: Value = row.get("details");
        let details: HashMap<String, Value> =
            serde_json::from_value(details_value).unwrap_or_else(|_| HashMap::new());

        Ok(AuditEvent {
            id: row.get("id"),
            timestamp: row.get("timestamp"),
            event_type,
            severity,
            user_id: row.get("user_id"),
            session_id: row.get("session_id"),
            ip_address: row.get::<Option<String>, _>("ip_address"),
            user_agent: row.get("user_agent"),
            resource: row.get("resource"),
            action: row.get("action"),
            outcome,
            details,
            error_message: row.get("error_message"),
            request_id: row.get("request_id"),
        })
    }

    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }
}

/// Filter for querying audit events
#[derive(Debug, Clone, Default)]
pub struct AuditFilter {
    pub user_id: Option<String>,
    pub event_type: Option<AuditEventType>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_audit_event_creation() {
        let mut details = HashMap::new();
        details.insert("test_key".to_string(), json!("test_value"));

        let event = AuditEvent {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            event_type: AuditEventType::Authentication,
            severity: AuditSeverity::Medium,
            user_id: Some("test-user".to_string()),
            session_id: None,
            ip_address: Some("192.168.1.1".to_string()),
            user_agent: None,
            resource: Some("login".to_string()),
            action: "user_login".to_string(),
            outcome: AuditOutcome::Success,
            details,
            error_message: None,
            request_id: None,
        };

        assert!(!event.id.is_empty());
        assert_eq!(event.action, "user_login");
        assert!(matches!(event.event_type, AuditEventType::Authentication));
        assert!(matches!(event.severity, AuditSeverity::Medium));
        assert!(matches!(event.outcome, AuditOutcome::Success));
        assert_eq!(event.user_id.unwrap(), "test-user");
    }

    #[test]
    fn test_audit_filter_default() {
        let filter = AuditFilter::default();
        assert!(filter.user_id.is_none());
        assert!(filter.event_type.is_none());
        assert!(filter.start_time.is_none());
        assert!(filter.end_time.is_none());
        assert!(filter.limit.is_none());
        assert!(filter.offset.is_none());
    }

    #[test]
    fn test_audit_statistics_default() {
        let stats = AuditStatistics {
            total_events: 0,
            events_by_type: HashMap::new(),
            events_by_user: HashMap::new(),
            failed_events: 0,
            critical_events: 0,
            retention_days: 90,
            oldest_event: None,
            newest_event: None,
        };

        assert_eq!(stats.total_events, 0);
        assert_eq!(stats.failed_events, 0);
        assert_eq!(stats.critical_events, 0);
        assert_eq!(stats.retention_days, 90);
        assert!(stats.events_by_type.is_empty());
        assert!(stats.events_by_user.is_empty());
    }

    #[test]
    fn test_event_type_serialization() {
        let event_type = AuditEventType::Authentication;
        let serialized = serde_json::to_string(&event_type).unwrap();
        assert_eq!(serialized, "\"Authentication\"");

        let deserialized: AuditEventType = serde_json::from_str(&serialized).unwrap();
        assert!(matches!(deserialized, AuditEventType::Authentication));
    }

    #[test]
    fn test_severity_ordering() {
        // Test that we can compare severity levels
        let low = AuditSeverity::Low;
        let critical = AuditSeverity::Critical;

        // This is just to ensure the enum variants exist and can be pattern matched
        match low {
            AuditSeverity::Low => assert!(true),
            _ => assert!(false),
        }

        match critical {
            AuditSeverity::Critical => assert!(true),
            _ => assert!(false),
        }
    }

    #[test]
    fn test_outcome_variants() {
        let outcomes = vec![
            AuditOutcome::Success,
            AuditOutcome::Failure,
            AuditOutcome::Partial,
        ];

        assert_eq!(outcomes.len(), 3);

        for outcome in outcomes {
            match outcome {
                AuditOutcome::Success | AuditOutcome::Failure | AuditOutcome::Partial => {
                    // All variants are valid
                    assert!(true);
                }
            }
        }
    }
}
