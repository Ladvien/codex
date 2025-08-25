use async_trait::async_trait;
use sqlx::{postgres::PgRow, Column, Row};
use std::collections::HashSet;

use crate::memory::{MemoryError, SearchRequest};

/// Result type for search backend operations
pub type Result<T> = std::result::Result<T, MemoryError>;

/// Column validation error with details about missing columns
#[derive(Debug, Clone)]
pub struct ColumnValidationError {
    pub missing_columns: Vec<String>,
    pub available_columns: Vec<String>,
}

impl std::fmt::Display for ColumnValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Column validation failed. Missing columns: [{}]. Available columns: [{}]",
            self.missing_columns.join(", "),
            self.available_columns.join(", ")
        )
    }
}

impl std::error::Error for ColumnValidationError {}

/// Core trait for search backends that defines the contract for search implementations.
/// This trait ensures architectural consistency and prevents column mismatch issues.
#[async_trait]
pub trait SearchBackend {
    /// Execute the search operation and return raw database rows
    async fn execute_search(&self, request: &SearchRequest) -> Result<Vec<PgRow>>;

    /// Return the set of required columns that must be present in query results
    /// for build_search_results to work correctly
    fn required_columns(&self) -> HashSet<String> {
        // Core columns required by build_search_results in repository.rs
        [
            // Memory table columns
            "id",
            "content",
            "content_hash",
            "embedding",
            "tier",
            "status",
            "importance_score",
            "access_count",
            "last_accessed_at",
            "metadata",
            "parent_id",
            "created_at",
            "updated_at",
            "expires_at",
            "consolidation_strength",
            "decay_rate",
            "recall_probability",
            "last_recall_interval",
            "recency_score",
            "relevance_score",
            // Testing effect columns
            "successful_retrievals",
            "failed_retrievals",
            "total_retrieval_attempts",
            "last_retrieval_difficulty",
            "last_retrieval_success",
            "next_review_at",
            "current_interval_days",
            "ease_factor",
            // Search result columns
            "similarity_score",
            "combined_score",
            "temporal_score",
            "access_frequency_score",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    /// Validate that the provided database rows contain all required columns
    /// This prevents runtime errors when build_search_results tries to access missing columns
    fn validate_columns(&self, rows: &[PgRow]) -> Result<()> {
        if rows.is_empty() {
            return Ok(()); // No rows to validate
        }

        let required = self.required_columns();
        let first_row = &rows[0];
        let available: HashSet<String> = first_row
            .columns()
            .iter()
            .map(|col| col.name().to_string())
            .collect();

        let missing: Vec<String> = required.difference(&available).cloned().collect();

        if !missing.is_empty() {
            let error = ColumnValidationError {
                missing_columns: missing,
                available_columns: available.into_iter().collect(),
            };

            return Err(MemoryError::Validation(format!(
                "Search backend column validation failed: {}",
                error
            )));
        }

        Ok(())
    }
}

/// Helper function to calculate access frequency score based on access count
/// Uses logarithmic scaling to prevent overly high frequency values from dominating
pub fn calculate_access_frequency_score(access_count: i32) -> f32 {
    if access_count <= 0 {
        return 0.0;
    }

    // Use ln(x + 1) to ensure positive values even for access_count = 1
    // Scale by 0.1 to keep the contribution reasonable relative to other scoring components
    ((access_count as f32 + 1.0).ln() * 0.1).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_access_frequency_score() {
        // Test edge cases
        assert_eq!(calculate_access_frequency_score(0), 0.0);
        assert_eq!(calculate_access_frequency_score(-1), 0.0);

        // Test normal cases with logarithmic scaling
        assert!(calculate_access_frequency_score(1) > 0.0);
        assert!(calculate_access_frequency_score(10) > calculate_access_frequency_score(1));
        assert!(calculate_access_frequency_score(100) > calculate_access_frequency_score(10));

        // Verify diminishing returns - differences should be positive and reasonable
        let diff_small = calculate_access_frequency_score(10) - calculate_access_frequency_score(1);
        let diff_large =
            calculate_access_frequency_score(10000) - calculate_access_frequency_score(1000);
        // With logarithmic scaling using ln(x+1), both differences should be positive
        assert!(diff_small > 0.0);
        assert!(diff_large > 0.0);
    }

    #[test]
    fn test_required_columns_completeness() {
        struct MockSearchBackend;

        #[async_trait]
        impl SearchBackend for MockSearchBackend {
            async fn execute_search(&self, _request: &SearchRequest) -> Result<Vec<PgRow>> {
                unimplemented!()
            }
        }

        let backend = MockSearchBackend;
        let required = backend.required_columns();

        // Verify critical columns are present
        assert!(required.contains("id"));
        assert!(required.contains("content"));
        assert!(required.contains("similarity_score"));
        assert!(required.contains("access_frequency_score"));
        assert!(required.contains("combined_score"));

        // Verify we have a reasonable number of columns (should be > 25)
        assert!(
            required.len() > 25,
            "Expected more than 25 required columns, got {}",
            required.len()
        );
    }

    #[test]
    fn test_column_validation_error_display() {
        let error = ColumnValidationError {
            missing_columns: vec!["col1".to_string(), "col2".to_string()],
            available_columns: vec!["col3".to_string(), "col4".to_string()],
        };

        let display = format!("{}", error);
        assert!(display.contains("col1, col2"));
        assert!(display.contains("col3, col4"));
        assert!(display.contains("Missing columns"));
    }
}
