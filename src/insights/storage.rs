//! Insight Storage Layer
//! 
//! This module implements the storage layer for the Codex Dreams insights feature.
//! It provides CRUD operations, vector embeddings, versioning, and feedback tracking.
//!
//! # Architecture
//! - Uses the same patterns as the main memory repository
//! - Implements proper transaction handling and error recovery
//! - Supports semantic search via vector embeddings
//! - Tracks insight versions (current + previous only)
//! - Records user feedback and calculates quality scores

#[cfg(feature = "codex-dreams")]
use crate::embedding::EmbeddingService;
#[cfg(feature = "codex-dreams")]
use crate::memory::error::{MemoryError, Result};
#[cfg(feature = "codex-dreams")]
use chrono::{DateTime, Utc};
#[cfg(feature = "codex-dreams")]
use pgvector::Vector;
#[cfg(feature = "codex-dreams")]
use sqlx::{PgPool, Postgres, Row, Transaction};
#[cfg(feature = "codex-dreams")]
use std::sync::Arc;
#[cfg(feature = "codex-dreams")]
use tracing::{debug, info, warn};
#[cfg(feature = "codex-dreams")]
use uuid::Uuid;

// Placeholder types that will be replaced when Story 2 (models) is complete
#[cfg(feature = "codex-dreams")]
#[derive(Debug, Clone)]
pub struct Insight {
    pub id: Uuid,
    pub content: String,
    pub insight_type: InsightType,
    pub confidence_score: f64,
    pub source_memory_ids: Vec<Uuid>,
    pub metadata: serde_json::Value,
    pub tags: Vec<String>,
    pub tier: String, // Will use proper tier enum when models are ready
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub version: i32,
    pub previous_version_id: Option<Uuid>,
    pub feedback_score: f64,
    pub embedding: Option<Vector>,
}

#[cfg(feature = "codex-dreams")]
#[derive(Debug, Clone)]
pub enum InsightType {
    Learning,
    Connection,
    Relationship,
    Assertion,
    MentalModel,
    Pattern,
}

#[cfg(feature = "codex-dreams")]
#[derive(Debug, Clone)]
pub struct InsightUpdate {
    pub content: Option<String>,
    pub confidence_score: Option<f64>,
    pub metadata: Option<serde_json::Value>,
    pub tags: Option<Vec<String>>,
}

#[cfg(feature = "codex-dreams")]
#[derive(Debug, Clone)]
pub struct Feedback {
    pub insight_id: Uuid,
    pub rating: FeedbackRating,
    pub comment: Option<String>,
    pub user_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[cfg(feature = "codex-dreams")]
#[derive(Debug, Clone)]
pub enum FeedbackRating {
    Helpful,
    NotHelpful,
    Incorrect,
}

#[cfg(feature = "codex-dreams")]
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub insight: Insight,
    pub similarity_score: f64,
    pub rank: usize,
}

/// Storage layer for insights with CRUD operations, vector search, and feedback
#[cfg(feature = "codex-dreams")]
pub struct InsightStorage {
    pool: Arc<PgPool>,
    embedder: Arc<dyn EmbeddingService>,
    // Configuration for pruning and quality thresholds
    min_feedback_score: f64,
    max_versions_to_keep: i32,
}

#[cfg(feature = "codex-dreams")]
impl InsightStorage {
    /// Create a new InsightStorage instance
    pub fn new(
        pool: Arc<PgPool>,
        embedder: Arc<dyn EmbeddingService>,
    ) -> Self {
        Self {
            pool,
            embedder,
            min_feedback_score: 0.3, // Configurable threshold for pruning
            max_versions_to_keep: 2,  // Current + previous only
        }
    }

    /// Store a new insight with vector embedding generation
    pub async fn store(&self, mut insight: Insight) -> Result<Uuid> {
        let mut tx = self.pool.begin().await
            .map_err(|e| MemoryError::DatabaseError(e.to_string()))?;

        // Generate embedding for the insight content
        let embedding = self.generate_embedding(&insight.content).await?;
        insight.embedding = Some(embedding);
        insight.id = Uuid::new_v4(); // Ensure unique ID
        insight.version = 1; // New insights start at version 1
        insight.created_at = Utc::now();
        insight.updated_at = insight.created_at;
        insight.feedback_score = 0.5; // Neutral starting score

        // Store insight in database (placeholder query - will update when schema is ready)
        let query = r#"
            INSERT INTO insights (
                id, content, insight_type, confidence_score, source_memory_ids,
                metadata, tags, tier, created_at, updated_at, version,
                previous_version_id, feedback_score, embedding
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14
            )
        "#;

        let insight_type_str = self.insight_type_to_string(&insight.insight_type);
        let tags_json = serde_json::to_value(&insight.tags)
            .map_err(|e| MemoryError::SerializationError(e.to_string()))?;
        let source_ids_json = serde_json::to_value(&insight.source_memory_ids)
            .map_err(|e| MemoryError::SerializationError(e.to_string()))?;

        sqlx::query(query)
            .bind(&insight.id)
            .bind(&insight.content)
            .bind(&insight_type_str)
            .bind(&insight.confidence_score)
            .bind(&source_ids_json)
            .bind(&insight.metadata)
            .bind(&tags_json)
            .bind(&insight.tier)
            .bind(&insight.created_at)
            .bind(&insight.updated_at)
            .bind(&insight.version)
            .bind(&insight.previous_version_id)
            .bind(&insight.feedback_score)
            .bind(&insight.embedding)
            .execute(&mut *tx)
            .await
            .map_err(|e| MemoryError::DatabaseError(e.to_string()))?;

        tx.commit().await
            .map_err(|e| MemoryError::DatabaseError(e.to_string()))?;

        info!("Stored new insight with ID: {}", insight.id);
        Ok(insight.id)
    }

    /// Update an existing insight with versioning support
    pub async fn update_with_version(&self, id: Uuid, updates: InsightUpdate) -> Result<()> {
        let mut tx = self.pool.begin().await
            .map_err(|e| MemoryError::DatabaseError(e.to_string()))?;

        // First, get the current insight to create a version
        let current = self.get_by_id_tx(&mut tx, id).await?;

        // Archive the current version by updating its status
        let archive_query = r#"
            UPDATE insights 
            SET tier = 'archived', updated_at = $1
            WHERE id = $2
        "#;
        
        sqlx::query(archive_query)
            .bind(Utc::now())
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| MemoryError::DatabaseError(e.to_string()))?;

        // Create new version with updates
        let mut new_version = current.clone();
        new_version.id = Uuid::new_v4();
        new_version.version = current.version + 1;
        new_version.previous_version_id = Some(current.id);
        new_version.updated_at = Utc::now();

        // Apply updates
        if let Some(content) = updates.content {
            new_version.content = content;
            // Regenerate embedding if content changed
            new_version.embedding = Some(self.generate_embedding(&new_version.content).await?);
        }
        if let Some(confidence) = updates.confidence_score {
            new_version.confidence_score = confidence;
        }
        if let Some(metadata) = updates.metadata {
            new_version.metadata = metadata;
        }
        if let Some(tags) = updates.tags {
            new_version.tags = tags;
        }

        // Store the new version
        let insert_query = r#"
            INSERT INTO insights (
                id, content, insight_type, confidence_score, source_memory_ids,
                metadata, tags, tier, created_at, updated_at, version,
                previous_version_id, feedback_score, embedding
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14
            )
        "#;

        let insight_type_str = self.insight_type_to_string(&new_version.insight_type);
        let tags_json = serde_json::to_value(&new_version.tags)
            .map_err(|e| MemoryError::SerializationError(e.to_string()))?;
        let source_ids_json = serde_json::to_value(&new_version.source_memory_ids)
            .map_err(|e| MemoryError::SerializationError(e.to_string()))?;

        sqlx::query(insert_query)
            .bind(&new_version.id)
            .bind(&new_version.content)
            .bind(&insight_type_str)
            .bind(&new_version.confidence_score)
            .bind(&source_ids_json)
            .bind(&new_version.metadata)
            .bind(&tags_json)
            .bind(&new_version.tier)
            .bind(&new_version.created_at)
            .bind(&new_version.updated_at)
            .bind(&new_version.version)
            .bind(&new_version.previous_version_id)
            .bind(&new_version.feedback_score)
            .bind(&new_version.embedding)
            .execute(&mut *tx)
            .await
            .map_err(|e| MemoryError::DatabaseError(e.to_string()))?;

        tx.commit().await
            .map_err(|e| MemoryError::DatabaseError(e.to_string()))?;

        info!("Updated insight {} to version {}", id, new_version.version);
        Ok(())
    }

    /// Get insight by ID
    pub async fn get_by_id(&self, id: Uuid) -> Result<Option<Insight>> {
        let mut tx = self.pool.begin().await
            .map_err(|e| MemoryError::DatabaseError(e.to_string()))?;
        
        let result = self.get_by_id_tx(&mut tx, id).await;
        tx.rollback().await
            .map_err(|e| MemoryError::DatabaseError(e.to_string()))?;
        
        match result {
            Ok(insight) => Ok(Some(insight)),
            Err(MemoryError::NotFound) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Internal method to get insight within a transaction
    async fn get_by_id_tx(&self, tx: &mut Transaction<'_, Postgres>, id: Uuid) -> Result<Insight> {
        let query = r#"
            SELECT id, content, insight_type, confidence_score, source_memory_ids,
                   metadata, tags, tier, created_at, updated_at, version,
                   previous_version_id, feedback_score, embedding
            FROM insights
            WHERE id = $1 AND tier != 'archived'
            ORDER BY version DESC
            LIMIT 1
        "#;

        let row = sqlx::query(query)
            .bind(id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(|e| MemoryError::DatabaseError(e.to_string()))?;

        match row {
            Some(row) => self.row_to_insight(&row),
            None => Err(MemoryError::NotFound),
        }
    }

    /// Perform semantic search for insights
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        // Generate embedding for the search query
        let query_embedding = self.generate_embedding(query).await?;

        let search_query = r#"
            SELECT id, content, insight_type, confidence_score, source_memory_ids,
                   metadata, tags, tier, created_at, updated_at, version,
                   previous_version_id, feedback_score, embedding,
                   1 - (embedding <=> $1) as similarity_score
            FROM insights
            WHERE tier != 'archived'
              AND embedding IS NOT NULL
            ORDER BY similarity_score DESC
            LIMIT $2
        "#;

        let rows = sqlx::query(search_query)
            .bind(&query_embedding)
            .bind(limit as i64)
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| MemoryError::DatabaseError(e.to_string()))?;

        let mut results = Vec::new();
        for (rank, row) in rows.iter().enumerate() {
            let insight = self.row_to_insight(row)?;
            let similarity_score: f64 = row.try_get("similarity_score")
                .map_err(|e| MemoryError::DatabaseError(e.to_string()))?;

            results.push(SearchResult {
                insight,
                similarity_score,
                rank: rank + 1,
            });
        }

        debug!("Found {} insights for query: {}", results.len(), query);
        Ok(results)
    }

    /// Record user feedback for an insight
    pub async fn record_feedback(&self, insight_id: Uuid, feedback: Feedback) -> Result<()> {
        let mut tx = self.pool.begin().await
            .map_err(|e| MemoryError::DatabaseError(e.to_string()))?;

        // Store the feedback
        let feedback_query = r#"
            INSERT INTO insight_feedback (id, insight_id, rating, comment, user_id, created_at)
            VALUES ($1, $2, $3, $4, $5, $6)
        "#;

        let rating_str = self.feedback_rating_to_string(&feedback.rating);
        sqlx::query(feedback_query)
            .bind(Uuid::new_v4())
            .bind(insight_id)
            .bind(rating_str)
            .bind(&feedback.comment)
            .bind(&feedback.user_id)
            .bind(feedback.created_at)
            .execute(&mut *tx)
            .await
            .map_err(|e| MemoryError::DatabaseError(e.to_string()))?;

        // Recalculate feedback score
        self.update_feedback_score_tx(&mut tx, insight_id).await?;

        tx.commit().await
            .map_err(|e| MemoryError::DatabaseError(e.to_string()))?;

        info!("Recorded feedback for insight {}: {:?}", insight_id, feedback.rating);
        Ok(())
    }

    /// Prune insights with poor feedback scores
    pub async fn prune_poor_insights(&self, threshold: f32) -> Result<usize> {
        let threshold = threshold as f64;
        
        let query = r#"
            UPDATE insights
            SET tier = 'archived', updated_at = $1
            WHERE feedback_score < $2
              AND tier != 'archived'
              AND created_at < $3  -- Only prune insights older than 1 day
        "#;

        let one_day_ago = Utc::now() - chrono::Duration::days(1);
        let result = sqlx::query(query)
            .bind(Utc::now())
            .bind(threshold)
            .bind(one_day_ago)
            .execute(&*self.pool)
            .await
            .map_err(|e| MemoryError::DatabaseError(e.to_string()))?;

        let pruned_count = result.rows_affected() as usize;
        info!("Pruned {} insights with feedback score below {}", pruned_count, threshold);
        Ok(pruned_count)
    }

    /// Delete an insight (soft delete by archiving)
    pub async fn delete(&self, id: Uuid) -> Result<bool> {
        let query = r#"
            UPDATE insights
            SET tier = 'archived', updated_at = $1
            WHERE id = $2 AND tier != 'archived'
        "#;

        let result = sqlx::query(query)
            .bind(Utc::now())
            .bind(id)
            .execute(&*self.pool)
            .await
            .map_err(|e| MemoryError::DatabaseError(e.to_string()))?;

        let deleted = result.rows_affected() > 0;
        if deleted {
            info!("Deleted insight {}", id);
        }
        Ok(deleted)
    }

    // Helper methods

    /// Generate embedding for text using the configured embedder
    async fn generate_embedding(&self, text: &str) -> Result<Vector> {
        let embedding_vec = self.embedder.generate_embedding(text)
            .await
            .map_err(|e| MemoryError::EmbeddingGenerationError(e.to_string()))?;
        
        Ok(Vector::from(embedding_vec))
    }

    /// Update feedback score for an insight based on all its feedback
    async fn update_feedback_score_tx(&self, tx: &mut Transaction<'_, Postgres>, insight_id: Uuid) -> Result<()> {
        let score_query = r#"
            SELECT 
                COUNT(*) as total_feedback,
                SUM(CASE WHEN rating = 'helpful' THEN 1.0 
                         WHEN rating = 'not_helpful' THEN -0.5
                         WHEN rating = 'incorrect' THEN -1.0
                         ELSE 0.0 END) as score_sum
            FROM insight_feedback
            WHERE insight_id = $1
        "#;

        let row = sqlx::query(score_query)
            .bind(insight_id)
            .fetch_one(&mut **tx)
            .await
            .map_err(|e| MemoryError::DatabaseError(e.to_string()))?;

        let total_feedback: i64 = row.try_get("total_feedback")
            .map_err(|e| MemoryError::DatabaseError(e.to_string()))?;
        let score_sum: Option<f64> = row.try_get("score_sum")
            .map_err(|e| MemoryError::DatabaseError(e.to_string()))?;

        let feedback_score = if total_feedback > 0 {
            let raw_score = score_sum.unwrap_or(0.0);
            // Normalize to 0.0-1.0 range with 0.5 as neutral
            (raw_score / total_feedback as f64 + 1.0) / 2.0
        } else {
            0.5 // Neutral score for no feedback
        };

        let update_query = r#"
            UPDATE insights
            SET feedback_score = $1, updated_at = $2
            WHERE id = $3
        "#;

        sqlx::query(update_query)
            .bind(feedback_score)
            .bind(Utc::now())
            .bind(insight_id)
            .execute(&mut **tx)
            .await
            .map_err(|e| MemoryError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    /// Convert database row to Insight struct
    fn row_to_insight(&self, row: &Row) -> Result<Insight> {
        let source_memory_ids_json: serde_json::Value = row.try_get("source_memory_ids")
            .map_err(|e| MemoryError::DatabaseError(e.to_string()))?;
        let source_memory_ids: Vec<Uuid> = serde_json::from_value(source_memory_ids_json)
            .map_err(|e| MemoryError::DeserializationError(e.to_string()))?;

        let tags_json: serde_json::Value = row.try_get("tags")
            .map_err(|e| MemoryError::DatabaseError(e.to_string()))?;
        let tags: Vec<String> = serde_json::from_value(tags_json)
            .map_err(|e| MemoryError::DeserializationError(e.to_string()))?;

        let insight_type_str: String = row.try_get("insight_type")
            .map_err(|e| MemoryError::DatabaseError(e.to_string()))?;
        let insight_type = self.string_to_insight_type(&insight_type_str)?;

        Ok(Insight {
            id: row.try_get("id").map_err(|e| MemoryError::DatabaseError(e.to_string()))?,
            content: row.try_get("content").map_err(|e| MemoryError::DatabaseError(e.to_string()))?,
            insight_type,
            confidence_score: row.try_get("confidence_score").map_err(|e| MemoryError::DatabaseError(e.to_string()))?,
            source_memory_ids,
            metadata: row.try_get("metadata").map_err(|e| MemoryError::DatabaseError(e.to_string()))?,
            tags,
            tier: row.try_get("tier").map_err(|e| MemoryError::DatabaseError(e.to_string()))?,
            created_at: row.try_get("created_at").map_err(|e| MemoryError::DatabaseError(e.to_string()))?,
            updated_at: row.try_get("updated_at").map_err(|e| MemoryError::DatabaseError(e.to_string()))?,
            version: row.try_get("version").map_err(|e| MemoryError::DatabaseError(e.to_string()))?,
            previous_version_id: row.try_get("previous_version_id").map_err(|e| MemoryError::DatabaseError(e.to_string()))?,
            feedback_score: row.try_get("feedback_score").map_err(|e| MemoryError::DatabaseError(e.to_string()))?,
            embedding: row.try_get("embedding").map_err(|e| MemoryError::DatabaseError(e.to_string()))?,
        })
    }

    /// Convert InsightType enum to string
    fn insight_type_to_string(&self, insight_type: &InsightType) -> String {
        match insight_type {
            InsightType::Learning => "learning".to_string(),
            InsightType::Connection => "connection".to_string(),
            InsightType::Relationship => "relationship".to_string(),
            InsightType::Assertion => "assertion".to_string(),
            InsightType::MentalModel => "mental_model".to_string(),
            InsightType::Pattern => "pattern".to_string(),
        }
    }

    /// Convert string to InsightType enum
    fn string_to_insight_type(&self, s: &str) -> Result<InsightType> {
        match s {
            "learning" => Ok(InsightType::Learning),
            "connection" => Ok(InsightType::Connection),
            "relationship" => Ok(InsightType::Relationship),
            "assertion" => Ok(InsightType::Assertion),
            "mental_model" => Ok(InsightType::MentalModel),
            "pattern" => Ok(InsightType::Pattern),
            _ => Err(MemoryError::InvalidInsightType(s.to_string())),
        }
    }

    /// Convert FeedbackRating enum to string
    fn feedback_rating_to_string(&self, rating: &FeedbackRating) -> String {
        match rating {
            FeedbackRating::Helpful => "helpful".to_string(),
            FeedbackRating::NotHelpful => "not_helpful".to_string(),
            FeedbackRating::Incorrect => "incorrect".to_string(),
        }
    }
}

#[cfg(feature = "codex-dreams")]
impl Default for InsightStorage {
    fn default() -> Self {
        // This is just for compilation - actual usage requires proper initialization
        panic!("InsightStorage requires explicit initialization with pool and embedder")
    }
}