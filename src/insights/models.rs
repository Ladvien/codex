//! Core data models and types for the Codex Dreams insights feature.
//! 
//! This module contains all the Rust data structures needed for automated
//! insight generation, storage, and management. All types are feature-gated
//! behind the `codex-dreams` feature flag.

#[cfg(feature = "codex-dreams")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "codex-dreams")]
use sqlx::FromRow;
#[cfg(feature = "codex-dreams")]
use uuid::Uuid;
#[cfg(feature = "codex-dreams")]
use chrono::{DateTime, Utc};
#[cfg(feature = "codex-dreams")]
use std::collections::HashMap;

/// Represents different types of insights that can be generated
#[cfg(feature = "codex-dreams")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "insight_type", rename_all = "lowercase")]
pub enum InsightType {
    /// Learning insights represent new knowledge or understanding
    Learning,
    /// Connection insights identify relationships between concepts
    Connection,
    /// Relationship insights show how entities relate to each other
    Relationship,
    /// Assertion insights represent claims or statements
    Assertion,
    /// Mental model insights represent cognitive frameworks
    MentalModel,
    /// Pattern insights identify recurring structures or behaviors
    Pattern,
}

/// Processing status for insights pipeline
#[cfg(feature = "codex-dreams")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "processing_status", rename_all = "lowercase")]
pub enum ProcessingStatus {
    /// Insight generation is queued but not started
    Pending,
    /// Insight generation is currently in progress
    Processing,
    /// Insight generation completed successfully
    Completed,
    /// Insight generation failed
    Failed,
}

/// Core insight data model matching the database schema
#[cfg(feature = "codex-dreams")]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Insight {
    /// Unique identifier for the insight
    pub id: Uuid,
    /// The main content/text of the insight
    pub content: String,
    /// Type classification of this insight
    pub insight_type: InsightType,
    /// Confidence score from 0.0 to 1.0
    pub confidence_score: f32,
    /// IDs of source memories that generated this insight
    pub source_memory_ids: Vec<Uuid>,
    /// Additional metadata as JSON
    pub metadata: serde_json::Value,
    /// Tags for categorization and search
    pub tags: Vec<String>,
    /// Storage tier (working, warm, cold)
    pub tier: String,
    /// When the insight was created
    pub created_at: DateTime<Utc>,
    /// When the insight was last modified
    pub updated_at: DateTime<Utc>,
    /// When the insight was last accessed
    pub last_accessed_at: Option<DateTime<Utc>>,
    /// Aggregated feedback score
    pub feedback_score: f32,
    /// Version number for updates
    pub version: i32,
    /// Previous version content (for rollback)
    pub previous_version: Option<String>,
}

/// User feedback on insights
#[cfg(feature = "codex-dreams")]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct InsightFeedback {
    /// Unique identifier for the feedback
    pub id: Uuid,
    /// ID of the insight being reviewed
    pub insight_id: Uuid,
    /// User's rating: helpful (1), not_helpful (-1), incorrect (-2)
    pub rating: i32,
    /// Optional comment from user
    pub comment: Option<String>,
    /// When the feedback was provided
    pub created_at: DateTime<Utc>,
    /// User identifier (if applicable)
    pub user_id: Option<String>,
}

/// Export format for insights with Markdown and JSON-LD serialization
#[cfg(feature = "codex-dreams")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsightExport {
    /// The insights to export
    pub insights: Vec<Insight>,
    /// Export metadata
    pub metadata: ExportMetadata,
}

/// Metadata for exports
#[cfg(feature = "codex-dreams")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportMetadata {
    /// When the export was generated
    pub generated_at: DateTime<Utc>,
    /// Total number of insights
    pub total_insights: usize,
    /// Export format used
    pub format: ExportFormat,
    /// Applied filters
    pub filters: ExportFilter,
}

/// Available export formats
#[cfg(feature = "codex-dreams")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportFormat {
    /// Markdown format
    Markdown,
    /// JSON-LD with Schema.org vocabulary
    JsonLd,
}

/// Filters for export operations
#[cfg(feature = "codex-dreams")]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExportFilter {
    /// Filter by date range
    pub date_from: Option<DateTime<Utc>>,
    pub date_to: Option<DateTime<Utc>>,
    /// Filter by insight type
    pub insight_types: Option<Vec<InsightType>>,
    /// Minimum confidence threshold
    pub min_confidence: Option<f32>,
    /// Filter by tags
    pub tags: Option<Vec<String>>,
}

/// Update structure for modifying insights
#[cfg(feature = "codex-dreams")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsightUpdate {
    /// Updated content
    pub content: Option<String>,
    /// Updated confidence score
    pub confidence_score: Option<f32>,
    /// Updated metadata
    pub metadata: Option<serde_json::Value>,
    /// Updated tags
    pub tags: Option<Vec<String>>,
}

/// Processing report for batch operations
#[cfg(feature = "codex-dreams")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingReport {
    /// Number of memories processed
    pub memories_processed: usize,
    /// Number of insights generated
    pub insights_generated: usize,
    /// Processing duration in seconds
    pub duration_seconds: f64,
    /// Any errors encountered
    pub errors: Vec<String>,
    /// Success rate (0.0 to 1.0)
    pub success_rate: f32,
}

/// Health status for the insights system
#[cfg(feature = "codex-dreams")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    /// Overall system health
    pub healthy: bool,
    /// Individual component status
    pub components: HashMap<String, bool>,
    /// Last processing time
    pub last_processed: Option<DateTime<Utc>>,
    /// Next scheduled processing time
    pub next_scheduled: Option<DateTime<Utc>>,
}

// Database conversion implementations
#[cfg(feature = "codex-dreams")]
impl From<sqlx::postgres::PgRow> for Insight {
    fn from(row: sqlx::postgres::PgRow) -> Self {
        use sqlx::Row;
        
        Self {
            id: row.get("id"),
            content: row.get("content"),
            insight_type: row.get("insight_type"),
            confidence_score: row.get("confidence_score"),
            source_memory_ids: row.get("source_memory_ids"),
            metadata: row.get("metadata"),
            tags: row.get("tags"),
            tier: row.get("tier"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
            last_accessed_at: row.get("last_accessed_at"),
            feedback_score: row.get("feedback_score"),
            version: row.get("version"),
            previous_version: row.get("previous_version"),
        }
    }
}

#[cfg(feature = "codex-dreams")]
impl From<sqlx::postgres::PgRow> for InsightFeedback {
    fn from(row: sqlx::postgres::PgRow) -> Self {
        use sqlx::Row;
        
        Self {
            id: row.get("id"),
            insight_id: row.get("insight_id"),
            rating: row.get("rating"),
            comment: row.get("comment"),
            created_at: row.get("created_at"),
            user_id: row.get("user_id"),
        }
    }
}

#[cfg(feature = "codex-dreams")]
impl InsightExport {
    /// Generate Markdown representation of the export
    pub fn to_markdown(&self) -> String {
        let mut markdown = String::new();
        
        // Header
        markdown.push_str("# Codex Dreams Insights Export\n\n");
        markdown.push_str(&format!(
            "Generated: {}\n", 
            self.metadata.generated_at.format("%Y-%m-%d %H:%M:%S UTC")
        ));
        markdown.push_str(&format!("Total Insights: {}\n\n", self.metadata.total_insights));
        
        // Insights
        for insight in &self.insights {
            markdown.push_str(&format!(
                "## {} Insight (Confidence: {:.1}%)\n\n",
                format!("{:?}", insight.insight_type),
                insight.confidence_score * 100.0
            ));
            
            markdown.push_str(&insight.content);
            markdown.push_str("\n\n");
            
            if !insight.tags.is_empty() {
                markdown.push_str("**Tags:** ");
                markdown.push_str(&insight.tags.join(", "));
                markdown.push_str("\n\n");
            }
            
            markdown.push_str(&format!(
                "*Created: {} | Feedback: {:.1}*\n\n",
                insight.created_at.format("%Y-%m-%d"),
                insight.feedback_score
            ));
            
            markdown.push_str("---\n\n");
        }
        
        markdown
    }
    
    /// Generate JSON-LD representation with Schema.org vocabulary
    pub fn to_json_ld(&self) -> serde_json::Value {
        serde_json::json!({
            "@context": "https://schema.org",
            "@type": "Dataset",
            "name": "Codex Dreams Insights",
            "description": "AI-generated insights from personal memory system",
            "dateCreated": self.metadata.generated_at,
            "creator": {
                "@type": "SoftwareApplication",
                "name": "Codex Dreams",
                "version": "1.0"
            },
            "distribution": {
                "@type": "DataDownload",
                "encodingFormat": "application/ld+json",
                "contentSize": self.insights.len()
            },
            "hasPart": self.insights.iter().map(|insight| {
                serde_json::json!({
                    "@type": "CreativeWork",
                    "text": insight.content,
                    "about": format!("{:?}", insight.insight_type),
                    "dateCreated": insight.created_at,
                    "version": insight.version,
                    "keywords": insight.tags,
                    "aggregateRating": {
                        "@type": "AggregateRating",
                        "ratingValue": insight.feedback_score,
                        "ratingCount": 1
                    },
                    "additionalProperty": {
                        "@type": "PropertyValue",
                        "name": "confidenceScore",
                        "value": insight.confidence_score
                    }
                })
            }).collect::<Vec<_>>()
        })
    }
}

#[cfg(all(feature = "codex-dreams", test))]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    #[test]
    fn test_insight_type_serialization() {
        let insight_type = InsightType::Learning;
        let json = serde_json::to_string(&insight_type).unwrap();
        // Note: sqlx::Type serialization uses PascalCase, not lowercase
        assert_eq!(json, "\"Learning\"");
        
        let deserialized: InsightType = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, insight_type);
    }
    
    #[test]
    fn test_processing_status_serialization() {
        let status = ProcessingStatus::Processing;
        let json = serde_json::to_string(&status).unwrap();
        // Note: sqlx::Type serialization uses PascalCase, not lowercase
        assert_eq!(json, "\"Processing\"");
        
        let deserialized: ProcessingStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, status);
    }
    
    #[test]
    fn test_insight_serialization() {
        let insight = create_test_insight();
        
        // Test JSON serialization
        let json = serde_json::to_string(&insight).unwrap();
        let deserialized: Insight = serde_json::from_str(&json).unwrap();
        
        assert_eq!(deserialized.id, insight.id);
        assert_eq!(deserialized.content, insight.content);
        assert_eq!(deserialized.insight_type, insight.insight_type);
    }
    
    #[test]
    fn test_insight_feedback_serialization() {
        let feedback = InsightFeedback {
            id: Uuid::new_v4(),
            insight_id: Uuid::new_v4(),
            rating: 1,
            comment: Some("Very helpful insight".to_string()),
            created_at: Utc::now(),
            user_id: Some("user123".to_string()),
        };
        
        let json = serde_json::to_string(&feedback).unwrap();
        let deserialized: InsightFeedback = serde_json::from_str(&json).unwrap();
        
        assert_eq!(deserialized.id, feedback.id);
        assert_eq!(deserialized.rating, feedback.rating);
        assert_eq!(deserialized.comment, feedback.comment);
    }
    
    #[test]
    fn test_export_filter_default() {
        let filter = ExportFilter::default();
        assert!(filter.date_from.is_none());
        assert!(filter.date_to.is_none());
        assert!(filter.insight_types.is_none());
        assert!(filter.min_confidence.is_none());
        assert!(filter.tags.is_none());
    }
    
    #[test]
    fn test_insight_export_markdown() {
        let export = create_test_export();
        let markdown = export.to_markdown();
        
        assert!(markdown.contains("# Codex Dreams Insights Export"));
        assert!(markdown.contains("Learning Insight"));
        assert!(markdown.contains("This is a test insight"));
        assert!(markdown.contains("test, learning"));
    }
    
    #[test]
    fn test_insight_export_json_ld() {
        let export = create_test_export();
        let json_ld = export.to_json_ld();
        
        assert_eq!(json_ld["@type"], "Dataset");
        assert_eq!(json_ld["name"], "Codex Dreams Insights");
        assert!(json_ld["hasPart"].is_array());
        
        let parts = json_ld["hasPart"].as_array().unwrap();
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0]["@type"], "CreativeWork");
        assert_eq!(parts[0]["text"], "This is a test insight");
    }
    
    #[test]
    fn test_processing_report_calculation() {
        let report = ProcessingReport {
            memories_processed: 10,
            insights_generated: 8,
            duration_seconds: 45.0,
            errors: vec!["Minor timeout".to_string()],
            success_rate: 0.8,
        };
        
        assert_eq!(report.success_rate, 0.8);
        assert_eq!(report.errors.len(), 1);
    }
    
    #[test]
    fn test_health_status_healthy() {
        let mut components = HashMap::new();
        components.insert("ollama".to_string(), true);
        components.insert("database".to_string(), true);
        
        let health = HealthStatus {
            healthy: true,
            components,
            last_processed: Some(Utc::now()),
            next_scheduled: Some(Utc::now()),
        };
        
        assert!(health.healthy);
        assert_eq!(health.components.len(), 2);
    }
    
    #[test]
    fn test_insight_update_partial() {
        let update = InsightUpdate {
            content: Some("Updated content".to_string()),
            confidence_score: None,
            metadata: None,
            tags: Some(vec!["updated".to_string()]),
        };
        
        assert!(update.content.is_some());
        assert!(update.confidence_score.is_none());
        assert!(update.tags.is_some());
    }
    
    // Helper functions for tests
    fn create_test_insight() -> Insight {
        Insight {
            id: Uuid::new_v4(),
            content: "This is a test insight".to_string(),
            insight_type: InsightType::Learning,
            confidence_score: 0.85,
            source_memory_ids: vec![Uuid::new_v4()],
            metadata: serde_json::json!({"test": true}),
            tags: vec!["test".to_string(), "learning".to_string()],
            tier: "working".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_accessed_at: None,
            feedback_score: 0.0,
            version: 1,
            previous_version: None,
        }
    }
    
    fn create_test_export() -> InsightExport {
        InsightExport {
            insights: vec![create_test_insight()],
            metadata: ExportMetadata {
                generated_at: Utc::now(),
                total_insights: 1,
                format: ExportFormat::Markdown,
                filters: ExportFilter::default(),
            },
        }
    }
}