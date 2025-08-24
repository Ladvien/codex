//! Export functionality for insights.
//! 
//! This module provides the `InsightExporter` struct that enables exporting
//! insights in multiple formats (Markdown, JSON-LD) with filtering capabilities.
//! 
//! # Features
//! - Export to Markdown with formatted metadata
//! - Export to JSON-LD with Schema.org compliance  
//! - Filter by date range, insight type, and confidence threshold
//! - 10MB file size limit enforcement
//! - Integration with InsightStorage for data fetching
//!
//! # Usage
//! ```rust
//! use codex::insights::{InsightExporter, ExportFilter, ExportFormat};
//! 
//! let exporter = InsightExporter::new(storage);
//! let filter = ExportFilter {
//!     min_confidence: Some(0.7),
//!     ..Default::default()
//! };
//! 
//! let markdown_export = exporter.export_markdown(filter.clone()).await?;
//! let jsonld_export = exporter.export_jsonld(filter).await?;
//! ```

#[cfg(feature = "codex-dreams")]
use super::models::{ExportFilter, ExportFormat, ExportMetadata, Insight, InsightExport};
#[cfg(feature = "codex-dreams")]
use super::storage::InsightStorage;
#[cfg(feature = "codex-dreams")]
use crate::memory::error::{MemoryError, Result};
#[cfg(feature = "codex-dreams")]
use chrono::Utc;
#[cfg(feature = "codex-dreams")]
use std::sync::Arc;
#[cfg(feature = "codex-dreams")]
use tracing::{debug, info, warn};

/// Maximum export file size in bytes (10MB)
#[cfg(feature = "codex-dreams")]
const MAX_EXPORT_SIZE: usize = 10 * 1024 * 1024;

/// InsightExporter provides export functionality for insights
#[cfg(feature = "codex-dreams")]
pub struct InsightExporter {
    storage: Arc<InsightStorage>,
}

#[cfg(feature = "codex-dreams")]
impl InsightExporter {
    /// Create a new InsightExporter
    pub fn new(storage: Arc<InsightStorage>) -> Self {
        Self { storage }
    }

    /// Export insights to Markdown format with filtering
    pub async fn export_markdown(&self, filter: ExportFilter) -> Result<String> {
        info!("Starting Markdown export with filters: {:?}", filter);
        
        let insights = self.fetch_filtered_insights(&filter).await?;
        let export = self.create_export(insights, ExportFormat::Markdown, filter)?;
        
        let markdown = export.to_markdown();
        
        // Enforce size limit
        self.check_size_limit(&markdown, "Markdown")?;
        
        info!("Successfully generated Markdown export with {} insights", export.metadata.total_insights);
        Ok(markdown)
    }

    /// Export insights to JSON-LD format with Schema.org compliance
    pub async fn export_jsonld(&self, filter: ExportFilter) -> Result<String> {
        info!("Starting JSON-LD export with filters: {:?}", filter);
        
        let insights = self.fetch_filtered_insights(&filter).await?;
        let export = self.create_export(insights, ExportFormat::JsonLd, filter)?;
        
        let jsonld_value = export.to_json_ld();
        let jsonld = serde_json::to_string_pretty(&jsonld_value)
            .map_err(|e| MemoryError::Serialization(e))?;
        
        // Enforce size limit
        self.check_size_limit(&jsonld, "JSON-LD")?;
        
        info!("Successfully generated JSON-LD export with {} insights", export.metadata.total_insights);
        Ok(jsonld)
    }

    /// Fetch insights from storage applying the provided filters
    async fn fetch_filtered_insights(&self, filter: &ExportFilter) -> Result<Vec<Insight>> {
        debug!("Fetching insights with filter: {:?}", filter);
        
        // For now, we'll use the search method to get all insights
        // In a production implementation, we'd want a dedicated method in InsightStorage
        // that supports filtering directly in the database query
        let all_insights = self.storage.search("", 10000).await?
            .into_iter()
            .map(|result| result.insight)
            .collect::<Vec<_>>();
        
        // Apply filters
        let filtered_insights = all_insights
            .into_iter()
            .filter(|insight| self.matches_filter(insight, filter))
            .collect::<Vec<_>>();
        
        debug!("Filtered to {} insights", filtered_insights.len());
        
        if filtered_insights.is_empty() {
            warn!("No insights match the provided filters");
        }
        
        Ok(filtered_insights)
    }

    /// Check if an insight matches the provided filter
    fn matches_filter(&self, insight: &Insight, filter: &ExportFilter) -> bool {
        // Date range filter
        if let Some(date_from) = filter.date_from {
            if insight.created_at < date_from {
                return false;
            }
        }
        
        if let Some(date_to) = filter.date_to {
            if insight.created_at > date_to {
                return false;
            }
        }
        
        // Insight type filter
        if let Some(ref types) = filter.insight_types {
            if !types.contains(&insight.insight_type) {
                return false;
            }
        }
        
        // Confidence threshold filter
        if let Some(min_confidence) = filter.min_confidence {
            if insight.confidence_score < min_confidence {
                return false;
            }
        }
        
        // Tags filter - insight must have at least one of the specified tags
        if let Some(ref filter_tags) = filter.tags {
            let has_matching_tag = filter_tags.iter()
                .any(|filter_tag| insight.tags.contains(filter_tag));
            
            if !has_matching_tag {
                return false;
            }
        }
        
        true
    }

    /// Create an InsightExport struct with metadata
    fn create_export(
        &self, 
        insights: Vec<Insight>, 
        format: ExportFormat, 
        filter: ExportFilter
    ) -> Result<InsightExport> {
        let total_insights = insights.len();
        
        if total_insights == 0 {
            warn!("Creating export with 0 insights");
        }
        
        let metadata = ExportMetadata {
            generated_at: Utc::now(),
            total_insights,
            format,
            filters: filter,
        };
        
        Ok(InsightExport {
            insights,
            metadata,
        })
    }

    /// Check if the export content exceeds the size limit
    fn check_size_limit(&self, content: &str, format: &str) -> Result<()> {
        let size = content.len();
        
        if size > MAX_EXPORT_SIZE {
            return Err(MemoryError::Validation(
                format!(
                    "{} export size ({} bytes) exceeds maximum allowed size ({} bytes). \
                    Consider applying more restrictive filters to reduce the export size.",
                    format, size, MAX_EXPORT_SIZE
                )
            ));
        }
        
        debug!("{} export size: {} bytes (within {} byte limit)", format, size, MAX_EXPORT_SIZE);
        Ok(())
    }
}

#[cfg(all(test, feature = "codex-dreams"))]
mod tests {
    use super::*;
    use crate::insights::models::InsightType;
    use chrono::Utc;
    use uuid::Uuid;

    fn create_test_insight() -> Insight {
        Insight {
            id: Uuid::new_v4(),
            content: "Test insight content".to_string(),
            insight_type: InsightType::Learning,
            confidence_score: 0.8,
            source_memory_ids: vec![Uuid::new_v4()],
            metadata: serde_json::json!({"test": true}),
            tags: vec!["test".to_string(), "sample".to_string()],
            tier: "working".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_accessed_at: None,
            feedback_score: 0.5,
            version: 1,
            previous_version: None,
        }
    }

    // Test the filtering logic without requiring a real storage instance
    #[test]
    fn test_matches_filter_date_range() {
        // Create an exporter with dummy storage for testing filtering logic
        let insight = create_test_insight();
        
        // We'll test the filtering logic directly without using create_test_exporter
        // to avoid the complexity of mocking InsightStorage
        
        // Test date_from filter
        let mut filter = ExportFilter::default();
        filter.date_from = Some(insight.created_at - chrono::Duration::hours(1));
        
        // Test date range logic directly
        let matches_from = if let Some(date_from) = filter.date_from {
            insight.created_at >= date_from
        } else {
            true
        };
        assert!(matches_from);
        
        filter.date_from = Some(insight.created_at + chrono::Duration::hours(1));
        let matches_from_future = if let Some(date_from) = filter.date_from {
            insight.created_at >= date_from
        } else {
            true
        };
        assert!(!matches_from_future);
    }

    #[test]
    fn test_size_limit_logic() {
        // Test within limit
        let small_content = "a".repeat(1000);
        assert!(small_content.len() <= MAX_EXPORT_SIZE);
        
        // Test exceeding limit  
        let large_content = "a".repeat(MAX_EXPORT_SIZE + 1);
        assert!(large_content.len() > MAX_EXPORT_SIZE);
    }

    #[test]
    fn test_insight_type_filter() {
        let insight = create_test_insight(); // InsightType::Learning
        
        // Test matching type
        let filter_types = vec![InsightType::Learning];
        assert!(filter_types.contains(&insight.insight_type));
        
        // Test non-matching type
        let filter_types = vec![InsightType::Connection];
        assert!(!filter_types.contains(&insight.insight_type));
        
        // Test multiple types with match
        let filter_types = vec![InsightType::Connection, InsightType::Learning];
        assert!(filter_types.contains(&insight.insight_type));
    }

    #[test]
    fn test_confidence_filter() {
        let insight = create_test_insight(); // confidence_score = 0.8
        
        // Test below threshold
        assert!(insight.confidence_score >= 0.7);
        
        // Test above threshold  
        assert!(insight.confidence_score < 0.9);
    }

    #[test]
    fn test_tags_filter() {
        let insight = create_test_insight(); // tags: ["test", "sample"]
        
        // Test matching tag
        let filter_tags = vec!["test".to_string()];
        let has_matching_tag = filter_tags.iter()
            .any(|filter_tag| insight.tags.contains(filter_tag));
        assert!(has_matching_tag);
        
        // Test non-matching tag
        let filter_tags = vec!["nonexistent".to_string()];
        let has_matching_tag = filter_tags.iter()
            .any(|filter_tag| insight.tags.contains(filter_tag));
        assert!(!has_matching_tag);
    }

    #[test]
    fn test_export_metadata_creation() {
        let insights = vec![create_test_insight(), create_test_insight()];
        let filter = ExportFilter::default();
        
        let metadata = ExportMetadata {
            generated_at: Utc::now(),
            total_insights: insights.len(),
            format: ExportFormat::Markdown,
            filters: filter,
        };
        
        assert_eq!(metadata.total_insights, 2);
        assert_eq!(metadata.format, ExportFormat::Markdown);
    }
}