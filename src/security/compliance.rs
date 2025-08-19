use crate::security::{AuditManager, AuditSeverity, GdprConfig, Result, SecurityError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};
use uuid::Uuid;

/// GDPR compliance manager
pub struct ComplianceManager {
    config: GdprConfig,
    db_pool: Arc<PgPool>,
    audit_manager: Option<Arc<AuditManager>>,
}

/// Data subject request types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DataSubjectRequestType {
    Access,        // Right to access personal data
    Rectification, // Right to rectify incorrect data
    Erasure,       // Right to be forgotten
    Portability,   // Right to data portability
    Restriction,   // Right to restrict processing
    Objection,     // Right to object to processing
}

/// Data subject request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSubjectRequest {
    pub id: String,
    pub request_type: DataSubjectRequestType,
    pub subject_id: String,
    pub subject_email: Option<String>,
    pub requested_at: DateTime<Utc>,
    pub status: RequestStatus,
    pub processed_at: Option<DateTime<Utc>>,
    pub processed_by: Option<String>,
    pub notes: Option<String>,
    pub data_export: Option<String>, // JSON string for portability requests
}

/// Request processing status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RequestStatus {
    Pending,
    InProgress,
    Completed,
    Rejected,
    PartiallyCompleted,
}

/// Personal data category
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DataCategory {
    Identity,  // Name, email, etc.
    Contact,   // Address, phone, etc.
    Technical, // IP addresses, session data
    Usage,     // Interaction data, preferences
    Generated, // AI-generated content, embeddings
    Metadata,  // Creation times, etc.
}

/// Data retention policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionPolicy {
    pub category: DataCategory,
    pub retention_days: u32,
    pub auto_delete: bool,
    pub legal_basis: String,
}

/// Data processing record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingRecord {
    pub id: String,
    pub subject_id: String,
    pub category: DataCategory,
    pub purpose: String,
    pub legal_basis: String,
    pub processed_at: DateTime<Utc>,
    pub retention_until: Option<DateTime<Utc>>,
    pub consent_given: Option<bool>,
}

/// Consent record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsentRecord {
    pub id: String,
    pub subject_id: String,
    pub purpose: String,
    pub consent_given: bool,
    pub given_at: DateTime<Utc>,
    pub withdrawn_at: Option<DateTime<Utc>>,
    pub version: String,
}

/// Data export for portability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataExport {
    pub subject_id: String,
    pub exported_at: DateTime<Utc>,
    pub data_categories: Vec<DataCategory>,
    pub personal_data: HashMap<String, serde_json::Value>,
    pub metadata: HashMap<String, String>,
}

impl ComplianceManager {
    pub fn new(config: GdprConfig, db_pool: Arc<PgPool>) -> Self {
        Self {
            config,
            db_pool,
            audit_manager: None,
        }
    }

    pub fn with_audit_manager(mut self, audit_manager: Arc<AuditManager>) -> Self {
        self.audit_manager = Some(audit_manager);
        self
    }

    /// Initialize GDPR compliance system
    pub async fn initialize(&self) -> Result<()> {
        if !self.config.enabled {
            debug!("GDPR compliance is disabled");
            return Ok(());
        }

        info!("Initializing GDPR compliance system");

        // Create data subject requests table
        let create_requests_table = r#"
            CREATE TABLE IF NOT EXISTS gdpr_requests (
                id UUID PRIMARY KEY,
                request_type TEXT NOT NULL,
                subject_id TEXT NOT NULL,
                subject_email TEXT,
                requested_at TIMESTAMPTZ NOT NULL,
                status TEXT NOT NULL,
                processed_at TIMESTAMPTZ,
                processed_by TEXT,
                notes TEXT,
                data_export JSONB,
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
        "#;

        // Create consent records table
        let create_consent_table = r#"
            CREATE TABLE IF NOT EXISTS gdpr_consent (
                id UUID PRIMARY KEY,
                subject_id TEXT NOT NULL,
                purpose TEXT NOT NULL,
                consent_given BOOLEAN NOT NULL,
                given_at TIMESTAMPTZ NOT NULL,
                withdrawn_at TIMESTAMPTZ,
                version TEXT NOT NULL,
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
        "#;

        // Create processing records table
        let create_processing_table = r#"
            CREATE TABLE IF NOT EXISTS gdpr_processing (
                id UUID PRIMARY KEY,
                subject_id TEXT NOT NULL,
                category TEXT NOT NULL,
                purpose TEXT NOT NULL,
                legal_basis TEXT NOT NULL,
                processed_at TIMESTAMPTZ NOT NULL,
                retention_until TIMESTAMPTZ,
                consent_given BOOLEAN,
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
        "#;

        for sql in [
            create_requests_table,
            create_consent_table,
            create_processing_table,
        ] {
            sqlx::query(sql)
                .execute(self.db_pool.as_ref())
                .await
                .map_err(|e| SecurityError::GdprError {
                    message: format!("Failed to create GDPR table: {e}"),
                })?;
        }

        // Create indexes
        let create_indexes = vec![
            "CREATE INDEX IF NOT EXISTS idx_gdpr_requests_subject_id ON gdpr_requests (subject_id);",
            "CREATE INDEX IF NOT EXISTS idx_gdpr_requests_status ON gdpr_requests (status);",
            "CREATE INDEX IF NOT EXISTS idx_gdpr_consent_subject_id ON gdpr_consent (subject_id);",
            "CREATE INDEX IF NOT EXISTS idx_gdpr_processing_subject_id ON gdpr_processing (subject_id);",
            "CREATE INDEX IF NOT EXISTS idx_gdpr_processing_retention ON gdpr_processing (retention_until);",
        ];

        for sql in create_indexes {
            sqlx::query(sql)
                .execute(self.db_pool.as_ref())
                .await
                .map_err(|e| SecurityError::GdprError {
                    message: format!("Failed to create GDPR index: {e}"),
                })?;
        }

        info!("GDPR compliance system initialized");
        Ok(())
    }

    /// Submit a data subject request
    pub async fn submit_request(
        &self,
        request_type: DataSubjectRequestType,
        subject_id: &str,
        subject_email: Option<&str>,
    ) -> Result<String> {
        if !self.config.enabled {
            return Err(SecurityError::GdprError {
                message: "GDPR compliance is disabled".to_string(),
            });
        }

        let request_id = Uuid::new_v4().to_string();
        let now = Utc::now();

        let request = DataSubjectRequest {
            id: request_id.clone(),
            request_type: request_type.clone(),
            subject_id: subject_id.to_string(),
            subject_email: subject_email.map(|s| s.to_string()),
            requested_at: now,
            status: RequestStatus::Pending,
            processed_at: None,
            processed_by: None,
            notes: None,
            data_export: None,
        };

        let insert_sql = r#"
            INSERT INTO gdpr_requests (id, request_type, subject_id, subject_email, requested_at, status)
            VALUES ($1, $2, $3, $4, $5, $6)
        "#;

        sqlx::query(insert_sql)
            .bind(&request.id)
            .bind(format!("{:?}", request.request_type))
            .bind(&request.subject_id)
            .bind(&request.subject_email)
            .bind(request.requested_at)
            .bind(format!("{:?}", request.status))
            .execute(self.db_pool.as_ref())
            .await
            .map_err(|e| SecurityError::GdprError {
                message: format!("Failed to submit GDPR request: {e}"),
            })?;

        // Audit log the request
        if let Some(audit_manager) = &self.audit_manager {
            let mut details = HashMap::new();
            details.insert(
                "request_id".to_string(),
                serde_json::Value::String(request_id.clone()),
            );
            details.insert(
                "request_type".to_string(),
                serde_json::Value::String(format!("{request_type:?}")),
            );
            details.insert(
                "subject_id".to_string(),
                serde_json::Value::String(subject_id.to_string()),
            );

            let _ = audit_manager
                .log_security_event(
                    "gdpr_request_submitted",
                    AuditSeverity::Medium,
                    Some(subject_id),
                    None,
                    details,
                )
                .await;
        }

        info!(
            "GDPR request submitted: {:?} for subject: {}",
            request_type, subject_id
        );
        Ok(request_id)
    }

    /// Process a right to be forgotten request
    pub async fn process_erasure_request(
        &self,
        request_id: &str,
        processor_id: &str,
    ) -> Result<()> {
        if !self.config.enabled || !self.config.right_to_be_forgotten {
            return Err(SecurityError::GdprError {
                message: "Right to be forgotten is not enabled".to_string(),
            });
        }

        // Get the request
        let request = self.get_request(request_id).await?;

        if !matches!(request.request_type, DataSubjectRequestType::Erasure) {
            return Err(SecurityError::GdprError {
                message: "Request is not an erasure request".to_string(),
            });
        }

        if !matches!(
            request.status,
            RequestStatus::Pending | RequestStatus::InProgress
        ) {
            return Err(SecurityError::GdprError {
                message: "Request is not in a processable state".to_string(),
            });
        }

        // Update request status
        self.update_request_status(request_id, RequestStatus::InProgress, Some(processor_id))
            .await?;

        // Perform data erasure
        let erasure_result = self.erase_personal_data(&request.subject_id).await;

        match erasure_result {
            Ok(erased_count) => {
                // Mark request as completed
                let notes = format!("Successfully erased {erased_count} data records");
                self.complete_request(request_id, processor_id, Some(&notes))
                    .await?;

                info!(
                    "Erasure request completed for subject: {} ({} records erased)",
                    request.subject_id, erased_count
                );
            }
            Err(e) => {
                // Mark request as partially completed or rejected
                let notes = format!("Erasure partially failed: {e}");
                self.update_request_status(
                    request_id,
                    RequestStatus::PartiallyCompleted,
                    Some(processor_id),
                )
                .await?;
                self.update_request_notes(request_id, &notes).await?;

                warn!(
                    "Erasure request partially failed for subject: {}: {}",
                    request.subject_id, e
                );
            }
        }

        Ok(())
    }

    /// Process a data portability request
    pub async fn process_portability_request(
        &self,
        request_id: &str,
        processor_id: &str,
    ) -> Result<DataExport> {
        if !self.config.enabled {
            return Err(SecurityError::GdprError {
                message: "GDPR compliance is disabled".to_string(),
            });
        }

        let request = self.get_request(request_id).await?;

        if !matches!(request.request_type, DataSubjectRequestType::Portability) {
            return Err(SecurityError::GdprError {
                message: "Request is not a portability request".to_string(),
            });
        }

        // Update request status
        self.update_request_status(request_id, RequestStatus::InProgress, Some(processor_id))
            .await?;

        // Export personal data
        let data_export = self.export_personal_data(&request.subject_id).await?;

        // Store export data in request
        let export_json =
            serde_json::to_string(&data_export).map_err(|e| SecurityError::GdprError {
                message: format!("Failed to serialize data export: {e}"),
            })?;

        sqlx::query("UPDATE gdpr_requests SET data_export = $1 WHERE id = $2")
            .bind(&export_json)
            .bind(request_id)
            .execute(self.db_pool.as_ref())
            .await
            .map_err(|e| SecurityError::GdprError {
                message: format!("Failed to store data export: {e}"),
            })?;

        // Complete the request
        let notes = format!(
            "Data exported successfully ({} categories)",
            data_export.data_categories.len()
        );
        self.complete_request(request_id, processor_id, Some(&notes))
            .await?;

        info!(
            "Portability request completed for subject: {}",
            request.subject_id
        );
        Ok(data_export)
    }

    /// Erase personal data for a subject
    async fn erase_personal_data(&self, subject_id: &str) -> Result<u32> {
        let mut total_erased = 0;

        // Erase from memories table
        let memory_result: i64 =
            sqlx::query_scalar("DELETE FROM memories WHERE user_id = $1 RETURNING COUNT(*)")
                .bind(subject_id)
                .fetch_optional(self.db_pool.as_ref())
                .await
                .map_err(|e| SecurityError::GdprError {
                    message: format!("Failed to erase memory data: {e}"),
                })?
                .unwrap_or(0);

        total_erased += memory_result as u32;

        // Erase from audit_events table (if it exists)
        let audit_result =
            sqlx::query_scalar::<_, i64>("DELETE FROM audit_events WHERE user_id = $1")
                .bind(subject_id)
                .fetch_optional(self.db_pool.as_ref())
                .await;

        if let Ok(Some(count)) = audit_result {
            total_erased += count as u32;
        }

        // Erase from backup_metadata table (if it exists)
        let backup_result = sqlx::query_scalar::<_, i64>(
            "UPDATE backup_metadata SET user_info = NULL WHERE user_info::jsonb ? $1",
        )
        .bind(subject_id)
        .fetch_optional(self.db_pool.as_ref())
        .await;

        if let Ok(Some(count)) = backup_result {
            total_erased += count as u32;
        }

        // Log the erasure
        if let Some(audit_manager) = &self.audit_manager {
            let mut details = HashMap::new();
            details.insert(
                "subject_id".to_string(),
                serde_json::Value::String(subject_id.to_string()),
            );
            details.insert(
                "records_erased".to_string(),
                serde_json::Value::Number(total_erased.into()),
            );

            let _ = audit_manager
                .log_security_event(
                    "personal_data_erased",
                    AuditSeverity::High,
                    Some(subject_id),
                    None,
                    details,
                )
                .await;
        }

        Ok(total_erased)
    }

    /// Export personal data for portability
    async fn export_personal_data(&self, subject_id: &str) -> Result<DataExport> {
        let mut personal_data = HashMap::new();
        let mut data_categories = Vec::new();

        // Export memory data
        let memory_rows = sqlx::query(
            "SELECT id, content, tier, created_at, last_accessed_at FROM memories WHERE user_id = $1"
        )
        .bind(subject_id)
        .fetch_all(self.db_pool.as_ref())
        .await
        .map_err(|e| SecurityError::GdprError {
            message: format!("Failed to export memory data: {e}"),
        })?;

        if !memory_rows.is_empty() {
            let mut memories = Vec::new();
            for row in memory_rows {
                let memory_data = serde_json::json!({
                    "id": row.get::<String, _>("id"),
                    "content": row.get::<String, _>("content"),
                    "tier": row.get::<String, _>("tier"),
                    "created_at": row.get::<DateTime<Utc>, _>("created_at"),
                    "last_accessed_at": row.get::<Option<DateTime<Utc>>, _>("last_accessed_at")
                });
                memories.push(memory_data);
            }
            personal_data.insert("memories".to_string(), serde_json::Value::Array(memories));
            data_categories.push(DataCategory::Generated);
        }

        // Export processing records
        let processing_rows = sqlx::query("SELECT * FROM gdpr_processing WHERE subject_id = $1")
            .bind(subject_id)
            .fetch_all(self.db_pool.as_ref())
            .await
            .unwrap_or_default();

        if !processing_rows.is_empty() {
            let mut processing_records = Vec::new();
            for row in processing_rows {
                let record = serde_json::json!({
                    "id": row.get::<String, _>("id"),
                    "category": row.get::<String, _>("category"),
                    "purpose": row.get::<String, _>("purpose"),
                    "legal_basis": row.get::<String, _>("legal_basis"),
                    "processed_at": row.get::<DateTime<Utc>, _>("processed_at"),
                    "retention_until": row.get::<Option<DateTime<Utc>>, _>("retention_until"),
                    "consent_given": row.get::<Option<bool>, _>("consent_given")
                });
                processing_records.push(record);
            }
            personal_data.insert(
                "processing_records".to_string(),
                serde_json::Value::Array(processing_records),
            );
            data_categories.push(DataCategory::Metadata);
        }

        // Export consent records
        let consent_rows = sqlx::query("SELECT * FROM gdpr_consent WHERE subject_id = $1")
            .bind(subject_id)
            .fetch_all(self.db_pool.as_ref())
            .await
            .unwrap_or_default();

        if !consent_rows.is_empty() {
            let mut consent_records = Vec::new();
            for row in consent_rows {
                let record = serde_json::json!({
                    "id": row.get::<String, _>("id"),
                    "purpose": row.get::<String, _>("purpose"),
                    "consent_given": row.get::<bool, _>("consent_given"),
                    "given_at": row.get::<DateTime<Utc>, _>("given_at"),
                    "withdrawn_at": row.get::<Option<DateTime<Utc>>, _>("withdrawn_at"),
                    "version": row.get::<String, _>("version")
                });
                consent_records.push(record);
            }
            personal_data.insert(
                "consent_records".to_string(),
                serde_json::Value::Array(consent_records),
            );
            data_categories.push(DataCategory::Identity);
        }

        let mut metadata = HashMap::new();
        metadata.insert("export_format".to_string(), "JSON".to_string());
        metadata.insert("gdpr_version".to_string(), "2018".to_string());

        Ok(DataExport {
            subject_id: subject_id.to_string(),
            exported_at: Utc::now(),
            data_categories,
            personal_data,
            metadata,
        })
    }

    /// Get a data subject request
    async fn get_request(&self, request_id: &str) -> Result<DataSubjectRequest> {
        let row = sqlx::query("SELECT * FROM gdpr_requests WHERE id = $1")
            .bind(request_id)
            .fetch_optional(self.db_pool.as_ref())
            .await
            .map_err(|e| SecurityError::GdprError {
                message: format!("Failed to fetch GDPR request: {e}"),
            })?
            .ok_or_else(|| SecurityError::GdprError {
                message: format!("GDPR request not found: {request_id}"),
            })?;

        self.row_to_request(row)
    }

    fn row_to_request(&self, row: sqlx::postgres::PgRow) -> Result<DataSubjectRequest> {
        let request_type_str: String = row.get("request_type");
        let request_type = match request_type_str.as_str() {
            "Access" => DataSubjectRequestType::Access,
            "Rectification" => DataSubjectRequestType::Rectification,
            "Erasure" => DataSubjectRequestType::Erasure,
            "Portability" => DataSubjectRequestType::Portability,
            "Restriction" => DataSubjectRequestType::Restriction,
            "Objection" => DataSubjectRequestType::Objection,
            _ => DataSubjectRequestType::Access,
        };

        let status_str: String = row.get("status");
        let status = match status_str.as_str() {
            "Pending" => RequestStatus::Pending,
            "InProgress" => RequestStatus::InProgress,
            "Completed" => RequestStatus::Completed,
            "Rejected" => RequestStatus::Rejected,
            "PartiallyCompleted" => RequestStatus::PartiallyCompleted,
            _ => RequestStatus::Pending,
        };

        Ok(DataSubjectRequest {
            id: row.get("id"),
            request_type,
            subject_id: row.get("subject_id"),
            subject_email: row.get("subject_email"),
            requested_at: row.get("requested_at"),
            status,
            processed_at: row.get("processed_at"),
            processed_by: row.get("processed_by"),
            notes: row.get("notes"),
            data_export: row.get("data_export"),
        })
    }

    async fn update_request_status(
        &self,
        request_id: &str,
        status: RequestStatus,
        processor_id: Option<&str>,
    ) -> Result<()> {
        sqlx::query("UPDATE gdpr_requests SET status = $1, processed_by = $2 WHERE id = $3")
            .bind(format!("{status:?}"))
            .bind(processor_id)
            .bind(request_id)
            .execute(self.db_pool.as_ref())
            .await
            .map_err(|e| SecurityError::GdprError {
                message: format!("Failed to update request status: {e}"),
            })?;

        Ok(())
    }

    async fn complete_request(
        &self,
        request_id: &str,
        processor_id: &str,
        notes: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE gdpr_requests SET status = $1, processed_at = $2, processed_by = $3, notes = $4 WHERE id = $5"
        )
        .bind(format!("{:?}", RequestStatus::Completed))
        .bind(Utc::now())
        .bind(processor_id)
        .bind(notes)
        .bind(request_id)
        .execute(self.db_pool.as_ref())
        .await
        .map_err(|e| SecurityError::GdprError {
            message: format!("Failed to complete request: {e}"),
        })?;

        Ok(())
    }

    async fn update_request_notes(&self, request_id: &str, notes: &str) -> Result<()> {
        sqlx::query("UPDATE gdpr_requests SET notes = $1 WHERE id = $2")
            .bind(notes)
            .bind(request_id)
            .execute(self.db_pool.as_ref())
            .await
            .map_err(|e| SecurityError::GdprError {
                message: format!("Failed to update request notes: {e}"),
            })?;

        Ok(())
    }

    /// Clean up expired data based on retention policies
    pub async fn cleanup_expired_data(&self) -> Result<u32> {
        if !self.config.enabled || !self.config.auto_cleanup {
            return Ok(0);
        }

        let cutoff_date =
            Utc::now() - chrono::Duration::days(self.config.data_retention_days as i64);
        let mut total_cleaned = 0;

        // Clean up old memories
        let memory_result: i64 = sqlx::query_scalar(
            "DELETE FROM memories WHERE created_at < $1 AND tier = 'Cold' RETURNING COUNT(*)",
        )
        .bind(cutoff_date)
        .fetch_optional(self.db_pool.as_ref())
        .await
        .map_err(|e| SecurityError::GdprError {
            message: format!("Failed to cleanup memory data: {e}"),
        })?
        .unwrap_or(0);

        total_cleaned += memory_result as u32;

        // Clean up old processing records
        let processing_result: i64 = sqlx::query_scalar(
            "DELETE FROM gdpr_processing WHERE retention_until IS NOT NULL AND retention_until < NOW() RETURNING COUNT(*)"
        )
        .fetch_optional(self.db_pool.as_ref())
        .await
        .unwrap_or(Some(0))
        .unwrap_or(0);

        total_cleaned += processing_result as u32;

        if total_cleaned > 0 {
            info!("Cleaned up {} expired data records", total_cleaned);
        }

        Ok(total_cleaned)
    }

    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    pub fn right_to_be_forgotten_enabled(&self) -> bool {
        self.config.enabled && self.config.right_to_be_forgotten
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compliance_manager_creation() {
        let config = GdprConfig::default();
        let pool = Arc::new(PgPool::connect_lazy("postgresql://localhost").unwrap());
        let manager = ComplianceManager::new(config, pool);
        assert!(!manager.is_enabled()); // disabled by default
    }

    #[test]
    fn test_data_subject_request_serialization() {
        let request = DataSubjectRequest {
            id: "test-id".to_string(),
            request_type: DataSubjectRequestType::Erasure,
            subject_id: "user123".to_string(),
            subject_email: Some("user@example.com".to_string()),
            requested_at: Utc::now(),
            status: RequestStatus::Pending,
            processed_at: None,
            processed_by: None,
            notes: None,
            data_export: None,
        };

        let serialized = serde_json::to_string(&request).unwrap();
        let deserialized: DataSubjectRequest = serde_json::from_str(&serialized).unwrap();

        assert_eq!(request.id, deserialized.id);
        assert_eq!(request.subject_id, deserialized.subject_id);
        assert!(matches!(
            deserialized.request_type,
            DataSubjectRequestType::Erasure
        ));
        assert!(matches!(deserialized.status, RequestStatus::Pending));
    }

    #[test]
    fn test_data_export_creation() {
        let mut personal_data = HashMap::new();
        personal_data.insert("test_key".to_string(), serde_json::json!({"value": "test"}));

        let mut metadata = HashMap::new();
        metadata.insert("format".to_string(), "JSON".to_string());

        let export = DataExport {
            subject_id: "user123".to_string(),
            exported_at: Utc::now(),
            data_categories: vec![DataCategory::Identity, DataCategory::Usage],
            personal_data,
            metadata,
        };

        assert_eq!(export.subject_id, "user123");
        assert_eq!(export.data_categories.len(), 2);
        assert_eq!(export.personal_data.len(), 1);
        assert_eq!(export.metadata.len(), 1);
    }

    #[test]
    fn test_retention_policy() {
        let policy = RetentionPolicy {
            category: DataCategory::Usage,
            retention_days: 90,
            auto_delete: true,
            legal_basis: "Legitimate interest".to_string(),
        };

        assert!(matches!(policy.category, DataCategory::Usage));
        assert_eq!(policy.retention_days, 90);
        assert!(policy.auto_delete);
    }

    #[test]
    fn test_consent_record() {
        let consent = ConsentRecord {
            id: "consent-123".to_string(),
            subject_id: "user123".to_string(),
            purpose: "Marketing communications".to_string(),
            consent_given: true,
            given_at: Utc::now(),
            withdrawn_at: None,
            version: "1.0".to_string(),
        };

        assert_eq!(consent.subject_id, "user123");
        assert!(consent.consent_given);
        assert!(consent.withdrawn_at.is_none());
    }

    #[test]
    fn test_processing_record() {
        let record = ProcessingRecord {
            id: "proc-123".to_string(),
            subject_id: "user123".to_string(),
            category: DataCategory::Technical,
            purpose: "Service delivery".to_string(),
            legal_basis: "Contract".to_string(),
            processed_at: Utc::now(),
            retention_until: Some(Utc::now() + chrono::Duration::days(365)),
            consent_given: Some(true),
        };

        assert!(matches!(record.category, DataCategory::Technical));
        assert!(record.retention_until.is_some());
        assert_eq!(record.consent_given, Some(true));
    }
}
