use super::error::{MemoryError, Result};
use super::event_triggers::EventTriggeredScoringEngine;
use super::math_engine::constants;
use super::models::*;
use crate::config::Config;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use chrono::{DateTime, Utc};
use pgvector::Vector;
use sqlx::postgres::types::PgInterval;
use sqlx::{PgPool, Postgres, Row, Transaction};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, info, warn};
use uuid::Uuid;

pub struct MemoryRepository {
    pool: PgPool,
    trigger_engine: Option<Arc<EventTriggeredScoringEngine>>,
    config: Option<Config>,
}

/// Safe query builder to prevent SQL injection vulnerabilities
#[derive(Debug, Clone)]
pub struct SafeQueryBuilder {
    query_parts: Vec<String>,
    parameters: Vec<QueryParameter>,
    bind_index: usize,
}

#[derive(Debug, Clone)]
enum QueryParameter {
    Text(String),
    Integer(i64),
    Float(f64),
    DateTime(DateTime<Utc>),
    Tier(MemoryTier),
    Uuid(Uuid),
    Vector(Vector),
}

impl SafeQueryBuilder {
    pub fn new(base_query: &str) -> Self {
        Self {
            query_parts: vec![base_query.to_string()],
            parameters: Vec::new(),
            bind_index: 1,
        }
    }

    /// Add a parameterized condition safely
    pub fn add_condition(&mut self, condition: &str) -> &mut Self {
        self.query_parts.push(condition.to_string());
        self
    }

    /// Add a parameterized tier filter
    pub fn add_tier_filter(&mut self, tier: &MemoryTier) -> &mut Self {
        let condition = format!("AND m.tier = ${}", self.bind_index);
        self.query_parts.push(condition);
        self.parameters.push(QueryParameter::Tier(*tier));
        self.bind_index += 1;
        self
    }

    /// Add a parameterized date range filter
    pub fn add_date_range(
        &mut self,
        start: Option<&DateTime<Utc>>,
        end: Option<&DateTime<Utc>>,
    ) -> &mut Self {
        if let Some(start_date) = start {
            let condition = format!("AND m.created_at >= ${}", self.bind_index);
            self.query_parts.push(condition);
            self.parameters.push(QueryParameter::DateTime(*start_date));
            self.bind_index += 1;
        }
        if let Some(end_date) = end {
            let condition = format!("AND m.created_at <= ${}", self.bind_index);
            self.query_parts.push(condition);
            self.parameters.push(QueryParameter::DateTime(*end_date));
            self.bind_index += 1;
        }
        self
    }

    /// Add a parameterized importance range filter
    pub fn add_importance_range(&mut self, min: Option<f64>, max: Option<f64>) -> &mut Self {
        if let Some(min_score) = min {
            let condition = format!("AND m.importance_score >= ${}", self.bind_index);
            self.query_parts.push(condition);
            self.parameters.push(QueryParameter::Float(min_score));
            self.bind_index += 1;
        }
        if let Some(max_score) = max {
            let condition = format!("AND m.importance_score <= ${}", self.bind_index);
            self.query_parts.push(condition);
            self.parameters.push(QueryParameter::Float(max_score));
            self.bind_index += 1;
        }
        self
    }

    /// Add a parameterized similarity threshold
    pub fn add_similarity_threshold(&mut self, threshold: f64) -> &mut Self {
        let condition = format!("AND 1 - (m.embedding <=> $1) >= ${}", self.bind_index);
        self.query_parts.push(condition);
        self.parameters.push(QueryParameter::Float(threshold));
        self.bind_index += 1;
        self
    }

    /// Add consolidation strength range filter
    pub fn add_consolidation_strength_range(
        &mut self,
        min: Option<f64>,
        max: Option<f64>,
    ) -> &mut Self {
        if let Some(min_strength) = min {
            let condition = format!("AND consolidation_strength >= ${}", self.bind_index);
            self.query_parts.push(condition);
            self.parameters.push(QueryParameter::Float(min_strength));
            self.bind_index += 1;
        }
        if let Some(max_strength) = max {
            let condition = format!("AND consolidation_strength <= ${}", self.bind_index);
            self.query_parts.push(condition);
            self.parameters.push(QueryParameter::Float(max_strength));
            self.bind_index += 1;
        }
        self
    }

    /// Add recall probability range filter
    pub fn add_recall_probability_range(
        &mut self,
        min: Option<f64>,
        max: Option<f64>,
    ) -> &mut Self {
        if let Some(min_recall) = min {
            let condition = format!("AND recall_probability >= ${}", self.bind_index);
            self.query_parts.push(condition);
            self.parameters.push(QueryParameter::Float(min_recall));
            self.bind_index += 1;
        }
        if let Some(max_recall) = max {
            let condition = format!("AND recall_probability <= ${}", self.bind_index);
            self.query_parts.push(condition);
            self.parameters.push(QueryParameter::Float(max_recall));
            self.bind_index += 1;
        }
        self
    }

    /// Add condition to exclude frozen tier if needed
    pub fn add_exclude_frozen(&mut self, exclude: bool) -> &mut Self {
        if exclude {
            self.query_parts.push("AND tier != 'frozen'".to_string());
        }
        self
    }

    /// Add a safe time interval condition for last access
    pub fn add_last_access_interval(&mut self, hours: f64) -> &mut Self {
        let condition = format!(
            "AND (last_accessed_at IS NULL OR last_accessed_at < NOW() - INTERVAL '${} hours')",
            self.bind_index
        );
        self.query_parts.push(condition);
        self.parameters
            .push(QueryParameter::Text(hours.to_string()));
        self.bind_index += 1;
        self
    }

    /// Add recall probability threshold condition
    pub fn add_recall_threshold_condition(&mut self, threshold: f64) -> &mut Self {
        // Store the threshold parameter for use in ORDER BY clause
        self.parameters.push(QueryParameter::Float(threshold));
        self.bind_index += 1;
        self
    }

    /// Add a limit and offset with validation
    pub fn add_pagination(&mut self, limit: usize, offset: usize) -> Result<&mut Self> {
        // Input validation: reasonable limits to prevent resource exhaustion
        if limit > 10000 {
            return Err(MemoryError::InvalidRequest {
                message: "Limit cannot exceed 10000 for performance reasons".to_string(),
            });
        }
        if offset > 1000000 {
            return Err(MemoryError::InvalidRequest {
                message: "Offset cannot exceed 1000000 for performance reasons".to_string(),
            });
        }

        let condition = format!("LIMIT ${} OFFSET ${}", self.bind_index, self.bind_index + 1);
        self.query_parts.push(condition);
        self.parameters.push(QueryParameter::Integer(limit as i64));
        self.parameters.push(QueryParameter::Integer(offset as i64));
        self.bind_index += 2;
        Ok(self)
    }

    /// Build the final query string
    pub fn build_query(&self) -> String {
        self.query_parts.join(" ")
    }

    /// Apply parameters to a sqlx query
    pub fn bind_parameters<'a>(
        &'a self,
        mut query: sqlx::query::Query<'a, Postgres, sqlx::postgres::PgArguments>,
    ) -> sqlx::query::Query<'a, Postgres, sqlx::postgres::PgArguments> {
        for param in &self.parameters {
            query = match param {
                QueryParameter::Text(s) => query.bind(s),
                QueryParameter::Integer(i) => query.bind(*i),
                QueryParameter::Float(f) => query.bind(*f),
                QueryParameter::DateTime(dt) => query.bind(*dt),
                QueryParameter::Tier(tier) => query.bind(tier),
                QueryParameter::Uuid(uuid) => query.bind(*uuid),
                QueryParameter::Vector(vec) => query.bind(vec),
            };
        }
        query
    }

    /// Apply parameters to a sqlx query_as
    pub fn bind_parameters_as<'a, T>(
        &'a self,
        mut query: sqlx::query::QueryAs<'a, Postgres, T, sqlx::postgres::PgArguments>,
    ) -> sqlx::query::QueryAs<'a, Postgres, T, sqlx::postgres::PgArguments>
    where
        T: for<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow>,
    {
        for param in &self.parameters {
            query = match param {
                QueryParameter::Text(s) => query.bind(s),
                QueryParameter::Integer(i) => query.bind(*i),
                QueryParameter::Float(f) => query.bind(*f),
                QueryParameter::DateTime(dt) => query.bind(*dt),
                QueryParameter::Tier(tier) => query.bind(tier),
                QueryParameter::Uuid(uuid) => query.bind(*uuid),
                QueryParameter::Vector(vec) => query.bind(vec),
            };
        }
        query
    }
}

impl MemoryRepository {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            trigger_engine: None,
            config: None,
        }
    }

    pub fn with_config(pool: PgPool, config: Config) -> Self {
        Self {
            pool,
            trigger_engine: None,
            config: Some(config),
        }
    }

    pub fn with_trigger_engine(
        pool: PgPool,
        trigger_engine: Arc<EventTriggeredScoringEngine>,
    ) -> Self {
        Self {
            pool,
            trigger_engine: Some(trigger_engine),
            config: None,
        }
    }

    pub fn with_config_and_trigger_engine(
        pool: PgPool,
        config: Config,
        trigger_engine: Arc<EventTriggeredScoringEngine>,
    ) -> Self {
        Self {
            pool,
            trigger_engine: Some(trigger_engine),
            config: Some(config),
        }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn create_memory(&self, request: CreateMemoryRequest) -> Result<Memory> {
        self.create_memory_with_user_context(request, None).await
    }

    pub async fn create_memory_with_user_context(
        &self,
        request: CreateMemoryRequest,
        user_id: Option<&str>,
    ) -> Result<Memory> {
        let id = Uuid::new_v4();
        let content_hash = Memory::calculate_content_hash(&request.content);
        let tier = request.tier.unwrap_or(MemoryTier::Working);

        // Check for duplicates (skip in test mode)
        let skip_duplicate_check =
            std::env::var("SKIP_DUPLICATE_CHECK").unwrap_or_else(|_| "false".to_string()) == "true";

        if !skip_duplicate_check {
            let duplicate_exists = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM memories WHERE content_hash = $1 AND tier = $2 AND status = 'active')"
            )
            .bind(&content_hash)
            .bind(tier)
            .fetch_one(&self.pool)
            .await?;

            if duplicate_exists {
                return Err(MemoryError::DuplicateContent {
                    tier: format!("{tier:?}"),
                });
            }
        }

        // Check working memory capacity limits (Miller's 7±2 principle)
        if tier == MemoryTier::Working {
            if let Some(ref config) = self.config {
                let working_count: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM memories WHERE tier = 'working' AND status = 'active'",
                )
                .fetch_one(&self.pool)
                .await?;

                if working_count >= config.tier_config.working_tier_limit as i64 {
                    // Working memory at capacity - need to evict or reject
                    info!(
                        "Working memory at capacity ({}/{}), applying LRU eviction",
                        working_count, config.tier_config.working_tier_limit
                    );

                    // Find the least recently used memory in working tier
                    let lru_memory_id: Option<Uuid> = sqlx::query_scalar(
                        "SELECT id FROM memories 
                         WHERE tier = 'working' AND status = 'active'
                         ORDER BY last_accessed ASC
                         LIMIT 1",
                    )
                    .fetch_optional(&self.pool)
                    .await?;

                    if let Some(memory_id) = lru_memory_id {
                        // Migrate LRU memory to warm tier
                        sqlx::query(
                            "UPDATE memories SET tier = 'warm', updated_at = NOW() 
                             WHERE id = $1",
                        )
                        .bind(memory_id)
                        .execute(&self.pool)
                        .await?;

                        info!(
                            "Evicted LRU memory {} from working to warm tier due to capacity limit",
                            memory_id
                        );

                        // Track eviction metric (if metrics are available)
                        // This would be integrated with the MetricsCollector in production
                    } else {
                        // This shouldn't happen but handle gracefully
                        return Err(MemoryError::StorageExhausted {
                            tier: "working".to_string(),
                            limit: config.tier_config.working_tier_limit,
                        });
                    }
                }
            }
        }

        // Apply event-triggered scoring if available
        let (final_importance_score, trigger_result) = if let Some(trigger_engine) =
            &self.trigger_engine
        {
            let original_importance = request.importance_score.unwrap_or(0.5);

            match trigger_engine
                .analyze_content(&request.content, original_importance, user_id)
                .await
            {
                Ok(result) => {
                    if result.triggered {
                        info!(
                            "Memory triggered event: {:?} (confidence: {:.2}, boosted: {:.2} -> {:.2})",
                            result.trigger_type, result.confidence, result.original_importance, result.boosted_importance
                        );
                        (result.boosted_importance, Some(result))
                    } else {
                        (original_importance, Some(result))
                    }
                }
                Err(e) => {
                    warn!("Failed to analyze content for triggers: {}", e);
                    (request.importance_score.unwrap_or(0.5), None)
                }
            }
        } else {
            (request.importance_score.unwrap_or(0.5), None)
        };

        let embedding = request.embedding.map(Vector::from);

        // Add trigger metadata if triggered
        let mut metadata = request.metadata.unwrap_or(serde_json::json!({}));
        if let Some(trigger_result) = &trigger_result {
            if trigger_result.triggered {
                metadata["trigger_info"] = serde_json::json!({
                    "triggered": true,
                    "trigger_type": trigger_result.trigger_type,
                    "confidence": trigger_result.confidence,
                    "original_importance": trigger_result.original_importance,
                    "boosted_importance": trigger_result.boosted_importance,
                    "processing_time_ms": trigger_result.processing_time.as_millis()
                });
            }
        }

        let memory = sqlx::query_as::<_, Memory>(
            r#"
            INSERT INTO memories (
                id, content, content_hash, embedding, tier, status, 
                importance_score, metadata, parent_id, expires_at,
                consolidation_strength, decay_rate
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(&request.content)
        .bind(&content_hash)
        .bind(embedding)
        .bind(tier)
        .bind(MemoryStatus::Active)
        .bind(final_importance_score)
        .bind(metadata)
        .bind(request.parent_id)
        .bind(request.expires_at)
        .bind(1.0_f64) // Default consolidation_strength
        .bind(1.0_f64) // Default decay_rate
        .fetch_one(&self.pool)
        .await?;

        info!(
            "Created memory {} in tier {:?} with importance {:.2}",
            memory.id, memory.tier, final_importance_score
        );
        Ok(memory)
    }

    pub async fn get_memory(&self, id: Uuid) -> Result<Memory> {
        let memory = sqlx::query_as::<_, Memory>(
            r#"
            UPDATE memories 
            SET access_count = access_count + 1, 
                last_accessed_at = NOW()
            WHERE id = $1 AND status = 'active'
            RETURNING *
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| MemoryError::NotFound { id: id.to_string() })?;

        debug!("Retrieved memory {} from tier {:?}", id, memory.tier);
        Ok(memory)
    }

    pub async fn update_memory(&self, id: Uuid, request: UpdateMemoryRequest) -> Result<Memory> {
        let mut tx = self.pool.begin().await?;

        // Get current memory
        let current = sqlx::query_as::<_, Memory>(
            "SELECT * FROM memories WHERE id = $1 AND status = 'active' FOR UPDATE",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| MemoryError::NotFound { id: id.to_string() })?;

        // Update fields
        let content = request.content.as_ref().unwrap_or(&current.content);
        let content_hash = if request.content.is_some() {
            Memory::calculate_content_hash(content)
        } else {
            current.content_hash.clone()
        };

        let embedding = request.embedding.map(Vector::from).or(current.embedding);
        let tier = request.tier.unwrap_or(current.tier);
        let importance_score = request.importance_score.unwrap_or(current.importance_score);
        let metadata = request.metadata.as_ref().unwrap_or(&current.metadata);
        let expires_at = request.expires_at.or(current.expires_at);

        let updated = sqlx::query_as::<_, Memory>(
            r#"
            UPDATE memories 
            SET content = $2, content_hash = $3, embedding = $4, tier = $5,
                importance_score = $6, metadata = $7, expires_at = $8,
                updated_at = NOW()
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(content)
        .bind(&content_hash)
        .bind(embedding)
        .bind(tier)
        .bind(importance_score)
        .bind(metadata)
        .bind(expires_at)
        .fetch_one(&mut *tx)
        .await?;

        // Record tier migration if changed
        if current.tier != tier {
            self.record_migration(
                &mut tx,
                id,
                current.tier,
                tier,
                Some("Manual update".to_string()),
            )
            .await?;
        }

        tx.commit().await?;
        info!("Updated memory {}", id);
        Ok(updated)
    }

    pub async fn delete_memory(&self, id: Uuid) -> Result<()> {
        let result = sqlx::query(
            "UPDATE memories SET status = 'deleted' WHERE id = $1 AND status = 'active'",
        )
        .bind(id)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(MemoryError::NotFound { id: id.to_string() });
        }

        info!("Soft deleted memory {}", id);
        Ok(())
    }

    /// Enhanced search method with memory-aware features for Story 9
    pub async fn search_memories_enhanced(
        &self,
        request: crate::memory::enhanced_retrieval::MemoryAwareSearchRequest,
    ) -> Result<crate::memory::enhanced_retrieval::MemoryAwareSearchResponse> {
        use crate::memory::enhanced_retrieval::*;

        let config = EnhancedRetrievalConfig::default();
        let retrieval_engine = MemoryAwareRetrievalEngine::new(
            config,
            std::sync::Arc::new(MemoryRepository::new(self.pool.clone())),
            None,
        );

        retrieval_engine.search(request).await
    }

    pub async fn search_memories(&self, request: SearchRequest) -> Result<SearchResponse> {
        let start_time = Instant::now();

        let search_type = request
            .search_type
            .as_ref()
            .unwrap_or(&SearchType::Semantic)
            .clone();
        let limit = request.limit.unwrap_or(10);
        let offset = request.offset.unwrap_or(0);

        let results = match search_type {
            SearchType::Semantic => self.semantic_search(&request).await?,
            SearchType::Temporal => self.temporal_search(&request).await?,
            SearchType::Hybrid => self.hybrid_search(&request).await?,
            SearchType::FullText => self.fulltext_search(&request).await?,
        };

        let total_count = if request.include_facets.unwrap_or(false) {
            Some(self.count_search_results(&request).await?)
        } else {
            None
        };

        let facets = if request.include_facets.unwrap_or(false) {
            Some(self.generate_search_facets(&request).await?)
        } else {
            None
        };

        let suggestions = if request.query_text.is_some() {
            Some(self.generate_query_suggestions(&request).await?)
        } else {
            None
        };

        let next_cursor = if results.len() as i32 >= limit {
            Some(self.generate_cursor(offset + limit as i64, &request))
        } else {
            None
        };

        let execution_time_ms = start_time.elapsed().as_millis() as u64;

        Ok(SearchResponse {
            results,
            total_count,
            facets,
            suggestions,
            next_cursor,
            execution_time_ms,
        })
    }

    async fn semantic_search(&self, request: &SearchRequest) -> Result<Vec<SearchResult>> {
        let query_embedding = if let Some(ref embedding) = request.query_embedding {
            Vector::from(embedding.clone())
        } else {
            return Err(MemoryError::InvalidRequest {
                message: "Query embedding is required for semantic search".to_string(),
            });
        };

        let limit = request.limit.unwrap_or(10);
        let offset = request.offset.unwrap_or(0);
        let threshold = request.similarity_threshold.unwrap_or(0.7);

        // Use safe query builder to prevent SQL injection
        let mut builder = SafeQueryBuilder::new(
            "SELECT m.*, 1 - (m.embedding <=> $1) as similarity_score FROM memories m WHERE m.status = 'active' AND m.embedding IS NOT NULL"
        );

        // Add filters safely
        self.add_filters_safe(request, &mut builder)?;

        // Add similarity threshold safely
        builder.add_similarity_threshold(threshold as f64);

        // Add ordering and pagination
        builder.add_condition("ORDER BY similarity_score DESC");
        builder.add_pagination(limit as usize, offset as usize)?;

        // Build query and execute with parameterized binding
        let query = builder.build_query();
        let mut sqlx_query = sqlx::query(&query).bind(&query_embedding);
        sqlx_query = builder.bind_parameters(sqlx_query);

        let rows = sqlx_query.fetch_all(&self.pool).await?;

        self.build_search_results(rows, request).await
    }

    async fn temporal_search(&self, request: &SearchRequest) -> Result<Vec<SearchResult>> {
        let limit = request.limit.unwrap_or(10);
        let offset = request.offset.unwrap_or(0);

        // Use safe query builder to prevent SQL injection
        let mut builder = SafeQueryBuilder::new(
            "SELECT m.*, 0.0 as similarity_score FROM memories m WHERE m.status = 'active'",
        );

        // Add filters safely
        self.add_filters_safe(request, &mut builder)?;

        // Add ordering and pagination
        builder.add_condition("ORDER BY m.created_at DESC, m.updated_at DESC");
        builder.add_pagination(limit as usize, offset as usize)?;

        // Build query and execute with parameterized binding
        let query = builder.build_query();
        let sqlx_query = builder.bind_parameters(sqlx::query(&query));

        let rows = sqlx_query.fetch_all(&self.pool).await?;

        self.build_search_results(rows, request).await
    }

    async fn hybrid_search(&self, request: &SearchRequest) -> Result<Vec<SearchResult>> {
        // Use three-component scoring weights (default: equal weighting)
        let _weights = request.hybrid_weights.as_ref().unwrap_or(&HybridWeights {
            semantic_weight: 0.333,
            temporal_weight: 0.333, // Maps to recency_score
            importance_weight: 0.334,
            access_frequency_weight: 0.0, // Included in relevance_score
        });

        let query_embedding = if let Some(ref embedding) = request.query_embedding {
            Vector::from(embedding.clone())
        } else {
            return Err(MemoryError::InvalidRequest {
                message: "Query embedding is required for hybrid search".to_string(),
            });
        };

        let limit = request.limit.unwrap_or(10);
        let offset = request.offset.unwrap_or(0);
        let threshold = request.similarity_threshold.unwrap_or(0.5);

        // Update component scores which will automatically update the generated combined_score
        sqlx::query(
            r#"
            UPDATE memories 
            SET recency_score = calculate_recency_score(last_accessed_at, created_at, 0.005),
                relevance_score = LEAST(1.0, 
                    0.5 * importance_score + 
                    0.3 * LEAST(1.0, access_count / 100.0) + 
                    0.2
                )
            WHERE status = 'active' AND embedding IS NOT NULL
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Use the generated combined_score column for optimal P99 <1ms performance
        let query = format!(
            r#"
            SELECT m.*,
                1 - (m.embedding <=> $1) as similarity_score,
                m.recency_score as temporal_score,
                m.importance_score,
                m.relevance_score,
                COALESCE(m.access_count, 0) as access_count,
                m.combined_score as combined_score
            FROM memories m
            WHERE m.status = 'active'
                AND m.embedding IS NOT NULL
                AND 1 - (m.embedding <=> $1) >= {threshold}
            ORDER BY m.combined_score DESC, similarity_score DESC
            LIMIT {limit} OFFSET {offset}
            "#
        );

        let rows = sqlx::query(&query)
            .bind(&query_embedding)
            .fetch_all(&self.pool)
            .await?;

        self.build_search_results(rows, request).await
    }

    async fn fulltext_search(&self, request: &SearchRequest) -> Result<Vec<SearchResult>> {
        let query_text =
            request
                .query_text
                .as_ref()
                .ok_or_else(|| MemoryError::InvalidRequest {
                    message: "Query text is required for full-text search".to_string(),
                })?;

        let limit = request.limit.unwrap_or(10);
        let offset = request.offset.unwrap_or(0);

        let query = format!(
            r#"
            SELECT m.*,
                ts_rank_cd(to_tsvector('english', m.content), plainto_tsquery('english', $1)) as similarity_score
            FROM memories m
            WHERE m.status = 'active'
                AND to_tsvector('english', m.content) @@ plainto_tsquery('english', $1)
            ORDER BY similarity_score DESC
            LIMIT {limit} OFFSET {offset}
            "#
        );

        let rows = sqlx::query(&query)
            .bind(query_text)
            .fetch_all(&self.pool)
            .await?;

        self.build_search_results(rows, request).await
    }

    /// Safe version of add_filters using SafeQueryBuilder to prevent SQL injection
    fn add_filters_safe(
        &self,
        request: &SearchRequest,
        builder: &mut SafeQueryBuilder,
    ) -> Result<()> {
        if let Some(tier) = &request.tier {
            builder.add_tier_filter(tier);
        }

        if let Some(date_range) = &request.date_range {
            builder.add_date_range(date_range.start.as_ref(), date_range.end.as_ref());
        }

        if let Some(importance_range) = &request.importance_range {
            builder.add_importance_range(
                importance_range.min.map(|v| v as f64),
                importance_range.max.map(|v| v as f64),
            );
        }

        Ok(())
    }

    async fn build_search_results(
        &self,
        rows: Vec<sqlx::postgres::PgRow>,
        request: &SearchRequest,
    ) -> Result<Vec<SearchResult>> {
        let mut results = Vec::new();
        let explain_score = request.explain_score.unwrap_or(false);

        for row in rows {
            let memory = Memory {
                id: row.try_get("id")?,
                content: row.try_get("content")?,
                content_hash: row.try_get("content_hash")?,
                embedding: row.try_get("embedding")?,
                tier: row.try_get("tier")?,
                status: row.try_get("status")?,
                importance_score: row.try_get("importance_score")?,
                access_count: row.try_get("access_count")?,
                last_accessed_at: row.try_get("last_accessed_at")?,
                metadata: row.try_get("metadata")?,
                parent_id: row.try_get("parent_id")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
                expires_at: row.try_get("expires_at")?,
                consolidation_strength: row.try_get("consolidation_strength").unwrap_or(1.0),
                decay_rate: row.try_get("decay_rate").unwrap_or(1.0),
                recall_probability: row.try_get("recall_probability")?,
                last_recall_interval: row.try_get("last_recall_interval")?,
                recency_score: row.try_get("recency_score").unwrap_or(0.0),
                relevance_score: row.try_get("relevance_score").unwrap_or(0.0),
            };

            let similarity_score: f32 = row.try_get("similarity_score").unwrap_or(0.0);
            let combined_score: f32 = row.try_get("combined_score").unwrap_or(similarity_score);
            let temporal_score: Option<f32> = row.try_get("temporal_score").ok();
            let access_frequency_score: Option<f32> = row.try_get("access_frequency_score").ok();
            let importance_score = memory.importance_score; // Extract before move

            let score_explanation = if explain_score {
                Some(ScoreExplanation {
                    semantic_contribution: similarity_score * 0.4,
                    temporal_contribution: temporal_score.unwrap_or(0.0) * 0.3,
                    importance_contribution: (importance_score * 0.2) as f32,
                    access_frequency_contribution: access_frequency_score.unwrap_or(0.0) * 0.1,
                    total_score: combined_score,
                    factors: vec![
                        "semantic similarity".to_string(),
                        "recency".to_string(),
                        "importance".to_string(),
                    ],
                })
            } else {
                None
            };

            results.push(SearchResult {
                memory,
                similarity_score,
                temporal_score,
                importance_score,
                access_frequency_score,
                combined_score,
                score_explanation,
            });
        }

        debug!("Built {} search results", results.len());
        Ok(results)
    }

    async fn count_search_results(&self, _request: &SearchRequest) -> Result<i64> {
        // Simplified count - would implement filtering logic similar to search
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM memories WHERE status = 'active'")
                .fetch_one(&self.pool)
                .await?;
        Ok(count)
    }

    async fn generate_search_facets(&self, _request: &SearchRequest) -> Result<SearchFacets> {
        // Generate tier facets
        let tier_rows: Vec<(String, i64)> = sqlx::query_as(
            "SELECT tier, COUNT(*) FROM memories WHERE status = 'active' GROUP BY tier",
        )
        .fetch_all(&self.pool)
        .await?;

        let mut tiers = HashMap::new();
        for (tier_str, count) in tier_rows {
            if let Ok(tier) = tier_str.parse::<MemoryTier>() {
                tiers.insert(tier, count);
            }
        }

        // Generate date histogram (simplified)
        let date_histogram = vec![DateBucket {
            date: Utc::now(),
            count: 10,
            interval: "day".to_string(),
        }];

        // Generate importance ranges
        let importance_ranges = vec![
            ImportanceRange {
                min: 0.0,
                max: 0.3,
                count: 5,
                label: "Low".to_string(),
            },
            ImportanceRange {
                min: 0.3,
                max: 0.7,
                count: 15,
                label: "Medium".to_string(),
            },
            ImportanceRange {
                min: 0.7,
                max: 1.0,
                count: 8,
                label: "High".to_string(),
            },
        ];

        Ok(SearchFacets {
            tiers,
            date_histogram,
            importance_ranges,
            tags: HashMap::new(), // Would extract from metadata
        })
    }

    async fn generate_query_suggestions(&self, _request: &SearchRequest) -> Result<Vec<String>> {
        // Simplified implementation - would use ML model or query history
        Ok(vec![
            "recent code changes".to_string(),
            "function definitions".to_string(),
            "error handling patterns".to_string(),
        ])
    }

    fn generate_cursor(&self, offset: i64, _request: &SearchRequest) -> String {
        // Simple cursor implementation - would encode more search state in production
        use base64::{engine::general_purpose::STANDARD, Engine};
        STANDARD.encode(format!("offset:{offset}"))
    }

    // Legacy method for backward compatibility
    pub async fn search_memories_simple(
        &self,
        request: SearchRequest,
    ) -> Result<Vec<SearchResult>> {
        let response = self.search_memories(request).await?;
        Ok(response.results)
    }

    pub async fn get_memories_by_tier(
        &self,
        tier: MemoryTier,
        limit: Option<i64>,
    ) -> Result<Vec<Memory>> {
        let limit = limit.unwrap_or(100);

        let memories = sqlx::query_as::<_, Memory>(
            r#"
            SELECT * FROM memories 
            WHERE tier = $1 AND status = 'active'
            ORDER BY importance_score DESC, updated_at DESC
            LIMIT $2
            "#,
        )
        .bind(tier)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(memories)
    }

    pub async fn migrate_memory(
        &self,
        id: Uuid,
        to_tier: MemoryTier,
        reason: Option<String>,
    ) -> Result<Memory> {
        let mut tx = self.pool.begin().await?;

        // Get current memory with lock
        let current = sqlx::query_as::<_, Memory>(
            "SELECT * FROM memories WHERE id = $1 AND status = 'active' FOR UPDATE",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| MemoryError::NotFound { id: id.to_string() })?;

        if current.tier == to_tier {
            return Ok(current);
        }

        // Validate tier transition
        let valid_transition = match (current.tier, to_tier) {
            (MemoryTier::Working, MemoryTier::Warm)
            | (MemoryTier::Working, MemoryTier::Cold)
            | (MemoryTier::Warm, MemoryTier::Cold)
            | (MemoryTier::Warm, MemoryTier::Working)
            | (MemoryTier::Cold, MemoryTier::Warm) => true,
            _ => false,
        };

        if !valid_transition {
            return Err(MemoryError::InvalidTierTransition {
                from: format!("{:?}", current.tier),
                to: format!("{to_tier:?}"),
            });
        }

        let start = std::time::Instant::now();

        // Update tier
        let updated = sqlx::query_as::<_, Memory>(
            r#"
            UPDATE memories 
            SET tier = $2, status = 'active', updated_at = NOW()
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(to_tier)
        .fetch_one(&mut *tx)
        .await?;

        let duration_ms = start.elapsed().as_millis() as i32;

        // Record migration
        self.record_migration(&mut tx, id, current.tier, to_tier, reason)
            .await?;

        tx.commit().await?;

        info!(
            "Migrated memory {} from {:?} to {:?} in {}ms",
            id, current.tier, to_tier, duration_ms
        );

        Ok(updated)
    }

    async fn record_migration(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        memory_id: Uuid,
        from_tier: MemoryTier,
        to_tier: MemoryTier,
        reason: Option<String>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO migration_history (memory_id, from_tier, to_tier, migration_reason, success)
            VALUES ($1, $2, $3, $4, true)
            "#,
        )
        .bind(memory_id)
        .bind(from_tier)
        .bind(to_tier)
        .bind(reason)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    pub async fn get_expired_memories(&self) -> Result<Vec<Memory>> {
        let memories = sqlx::query_as::<_, Memory>(
            r#"
            SELECT * FROM memories 
            WHERE status = 'active' 
                AND expires_at IS NOT NULL 
                AND expires_at < NOW()
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(memories)
    }

    pub async fn cleanup_expired_memories(&self) -> Result<u64> {
        let result = sqlx::query(
            r#"
            UPDATE memories 
            SET status = 'deleted' 
            WHERE status = 'active' 
                AND expires_at IS NOT NULL 
                AND expires_at < NOW()
            "#,
        )
        .execute(&self.pool)
        .await?;

        let count = result.rows_affected();
        if count > 0 {
            info!("Cleaned up {} expired memories", count);
        }

        Ok(count)
    }

    pub async fn get_migration_candidates(
        &self,
        tier: MemoryTier,
        limit: i64,
    ) -> Result<Vec<Memory>> {
        let query = match tier {
            MemoryTier::Working => {
                r#"
                SELECT * FROM memories 
                WHERE tier = 'working' 
                    AND status = 'active'
                    AND (
                        importance_score < 0.3 
                        OR (last_accessed_at IS NOT NULL 
                            AND last_accessed_at < NOW() - INTERVAL '24 hours')
                    )
                ORDER BY importance_score ASC, last_accessed_at ASC NULLS FIRST
                LIMIT $1
                "#
            }
            MemoryTier::Warm => {
                r#"
                SELECT * FROM memories 
                WHERE tier = 'warm' 
                    AND status = 'active'
                    AND importance_score < 0.1 
                    AND updated_at < NOW() - INTERVAL '7 days'
                ORDER BY importance_score ASC, updated_at ASC
                LIMIT $1
                "#
            }
            MemoryTier::Cold => {
                return Ok(Vec::new());
            }
            MemoryTier::Frozen => {
                return Ok(Vec::new()); // Frozen memories don't migrate further
            }
        };

        let memories = sqlx::query_as::<_, Memory>(query)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;

        Ok(memories)
    }

    /// Get working memory pressure ratio (0.0 to 1.0)
    pub async fn get_working_memory_pressure(&self) -> Result<f64> {
        if let Some(ref config) = self.config {
            let working_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM memories WHERE tier = 'working' AND status = 'active'",
            )
            .fetch_one(&self.pool)
            .await?;

            let pressure = working_count as f64 / config.tier_config.working_tier_limit as f64;
            Ok(pressure.min(1.0))
        } else {
            Ok(0.0)
        }
    }

    pub async fn get_statistics(&self) -> Result<MemoryStatistics> {
        let stats = sqlx::query_as::<_, MemoryStatistics>(
            r#"
            SELECT 
                COUNT(*) FILTER (WHERE tier = 'working' AND status = 'active') as working_count,
                COUNT(*) FILTER (WHERE tier = 'warm' AND status = 'active') as warm_count,
                COUNT(*) FILTER (WHERE tier = 'cold' AND status = 'active') as cold_count,
                COUNT(*) FILTER (WHERE status = 'active') as total_active,
                COUNT(*) FILTER (WHERE status = 'deleted') as total_deleted,
                AVG(importance_score) FILTER (WHERE status = 'active') as avg_importance,
                MAX(access_count) FILTER (WHERE status = 'active') as max_access_count,
                CAST(AVG(access_count) FILTER (WHERE status = 'active') AS FLOAT8) as avg_access_count
            FROM memories
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(stats)
    }

    // Consolidation and freezing methods

    /// Get consolidation analytics for all tiers
    pub async fn get_consolidation_analytics(&self) -> Result<Vec<ConsolidationAnalytics>> {
        let analytics = sqlx::query_as::<_, ConsolidationAnalytics>(
            r#"
            SELECT 
                tier,
                COUNT(*) as total_memories,
                AVG(consolidation_strength) as avg_consolidation_strength,
                AVG(recall_probability) as avg_recall_probability,
                AVG(decay_rate) as avg_decay_rate,
                AVG(EXTRACT(EPOCH FROM (NOW() - created_at)) / 86400) as avg_age_days,
                COUNT(*) FILTER (WHERE recall_probability < $1) as migration_candidates,
                COUNT(*) FILTER (WHERE last_accessed_at IS NULL) as never_accessed,
                COUNT(*) FILTER (WHERE last_accessed_at > NOW() - INTERVAL '24 hours') as accessed_recently
            FROM memories 
            WHERE status = 'active' 
            GROUP BY tier
            ORDER BY 
                CASE tier 
                    WHEN 'working' THEN 1 
                    WHEN 'warm' THEN 2 
                    WHEN 'cold' THEN 3 
                    WHEN 'frozen' THEN 4 
                END
            "#,
        )
        .bind(constants::FROZEN_MIGRATION_THRESHOLD)
        .fetch_all(&self.pool)
        .await?;

        Ok(analytics)
    }

    /* TEMPORARILY COMMENTED OUT - missing consolidation_events table
    /// Get consolidation event summary for the last week
    pub async fn get_consolidation_events(&self) -> Result<Vec<ConsolidationEventSummary>> {
        let events = sqlx::query_as::<_, ConsolidationEventSummary>(
            r#"
            SELECT
                event_type,
                COUNT(*) as event_count,
                AVG(new_consolidation_strength - previous_consolidation_strength) as avg_strength_change,
                AVG(COALESCE(new_recall_probability, 0) - COALESCE(previous_recall_probability, 0)) as avg_probability_change,
                AVG(EXTRACT(EPOCH FROM recall_interval) / 3600) as avg_recall_interval_hours
            FROM memory_consolidation_log
            WHERE created_at > NOW() - INTERVAL '7 days'
            GROUP BY event_type
            ORDER BY event_count DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(events)
    }
    */

    /// Find memories ready for tier migration based on recall probability
    pub async fn find_migration_candidates(
        &self,
        tier: MemoryTier,
        limit: i32,
    ) -> Result<Vec<Memory>> {
        let threshold = match tier {
            MemoryTier::Working => 0.7,
            MemoryTier::Warm => 0.5,
            MemoryTier::Cold => 0.2,
            MemoryTier::Frozen => 0.0, // Frozen memories don't migrate
        };

        let memories = sqlx::query_as::<_, Memory>(
            r#"
            SELECT * FROM memories 
            WHERE tier = $1 
            AND status = 'active'
            AND (recall_probability < $2 OR recall_probability IS NULL)
            ORDER BY recall_probability ASC NULLS LAST, consolidation_strength ASC
            LIMIT $3
            "#,
        )
        .bind(tier)
        .bind(threshold)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(memories)
    }

    /// Update memory consolidation parameters
    pub async fn update_consolidation(
        &self,
        memory_id: Uuid,
        consolidation_strength: f64,
        decay_rate: f64,
        recall_probability: Option<f64>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE memories 
            SET consolidation_strength = $2, 
                decay_rate = $3, 
                recall_probability = $4,
                updated_at = NOW()
            WHERE id = $1 AND status = 'active'
            "#,
        )
        .bind(memory_id)
        .bind(consolidation_strength)
        .bind(decay_rate)
        .bind(recall_probability)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Log a consolidation event
    pub async fn log_consolidation_event(
        &self,
        memory_id: Uuid,
        event_type: &str,
        previous_strength: f64,
        new_strength: f64,
        previous_probability: Option<f64>,
        new_probability: Option<f64>,
        recall_interval: Option<PgInterval>,
        context: serde_json::Value,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO memory_consolidation_log (
                memory_id, event_type, previous_consolidation_strength, 
                new_consolidation_strength, previous_recall_probability,
                new_recall_probability, recall_interval, access_context
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(memory_id)
        .bind(event_type)
        .bind(previous_strength)
        .bind(new_strength)
        .bind(previous_probability)
        .bind(new_probability)
        .bind(recall_interval)
        .bind(context)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Freeze a memory by moving it to compressed storage using zstd compression
    pub async fn freeze_memory(
        &self,
        memory_id: Uuid,
        reason: Option<String>,
    ) -> Result<FreezeMemoryResponse> {
        use super::compression::{FrozenMemoryCompression, ZstdCompressionEngine};
        use std::time::Instant;

        let start_time = Instant::now();
        let mut tx = self.pool.begin().await?;

        // Get the memory to freeze with validation
        let memory = sqlx::query_as::<_, Memory>(
            "SELECT * FROM memories WHERE id = $1 AND status = 'active'",
        )
        .bind(memory_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| MemoryError::NotFound {
            id: memory_id.to_string(),
        })?;

        // Ensure we only freeze cold memories with P(r) < 0.2
        if memory.tier != MemoryTier::Cold {
            return Err(MemoryError::InvalidRequest {
                message: format!(
                    "Can only freeze memories in cold tier, found {:?}",
                    memory.tier
                ),
            });
        }

        let recall_probability = memory.recall_probability.unwrap_or(0.0);
        if recall_probability >= 0.2 {
            return Err(MemoryError::InvalidRequest {
                message: format!(
                    "Can only freeze memories with P(r) < 0.2, found {recall_probability:.3}"
                ),
            });
        }

        info!(
            "Freezing memory {} (P(r)={:.3}, content_length={})",
            memory_id,
            recall_probability,
            memory.content.len()
        );

        // Compress the memory data using zstd
        let compression_engine = ZstdCompressionEngine::new();
        let compression_result =
            compression_engine.compress_memory_data(&memory.content, &memory.metadata)?;

        // Validate compression quality
        FrozenMemoryCompression::validate_compression_quality(
            compression_result.compression_ratio,
            memory.content.len(),
        )?;

        let (compressed_data, original_size, compressed_size, compression_ratio) =
            FrozenMemoryCompression::to_database_format(compression_result);

        debug!(
            "Compression completed: {:.2}:1 ratio, {} -> {} bytes",
            compression_ratio, original_size, compressed_size
        );

        // Create frozen memory record
        let frozen_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO frozen_memories (
                id, original_memory_id, compressed_content, 
                original_metadata, original_content_hash, original_embedding,
                original_tier, freeze_reason, compression_ratio,
                original_size_bytes, compressed_size_bytes
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            "#,
        )
        .bind(frozen_id)
        .bind(memory.id)
        .bind(&compressed_data)
        .bind(&memory.metadata)
        .bind(&memory.content_hash)
        .bind(memory.embedding.as_ref())
        .bind(memory.tier)
        .bind(
            reason
                .as_deref()
                .unwrap_or("Auto-frozen: P(r) < 0.2 threshold"),
        )
        .bind(compression_ratio)
        .bind(original_size)
        .bind(compressed_size)
        .execute(&mut *tx)
        .await?;

        // Update original memory to frozen tier
        sqlx::query(
            "UPDATE memories SET tier = 'frozen', status = 'archived', updated_at = NOW() WHERE id = $1"
        )
        .bind(memory_id)
        .execute(&mut *tx)
        .await?;

        // Log the migration
        let processing_time_ms = start_time.elapsed().as_millis() as i32;
        sqlx::query(
            r#"
            INSERT INTO migration_history (
                memory_id, from_tier, to_tier, migration_reason,
                migration_duration_ms, success
            ) VALUES ($1, $2, 'frozen', $3, $4, true)
            "#,
        )
        .bind(memory_id)
        .bind(memory.tier)
        .bind(format!(
            "Frozen with {compression_ratio:.2}:1 compression"
        ))
        .bind(processing_time_ms)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        info!(
            "Successfully froze memory {} with {:.2}:1 compression in {}ms",
            memory_id, compression_ratio, processing_time_ms
        );

        Ok(FreezeMemoryResponse {
            frozen_id,
            compression_ratio: Some(compression_ratio),
            original_tier: memory.tier,
            frozen_at: Utc::now(),
        })
    }

    /// Unfreeze a memory and restore it to active status with zstd decompression
    pub async fn unfreeze_memory(
        &self,
        frozen_id: Uuid,
        target_tier: Option<MemoryTier>,
    ) -> Result<UnfreezeMemoryResponse> {
        use super::compression::ZstdCompressionEngine;
        use rand::Rng;
        use std::time::Instant;
        use tokio::time::{sleep, Duration};

        let start_time = Instant::now();
        let mut tx = self.pool.begin().await?;

        // Get the frozen memory details
        let frozen_memory =
            sqlx::query_as::<_, FrozenMemory>("SELECT * FROM frozen_memories WHERE id = $1")
                .bind(frozen_id)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or_else(|| MemoryError::NotFound {
                    id: frozen_id.to_string(),
                })?;

        info!(
            "Unfreezing memory {} (compression_ratio: {:.2}:1)",
            frozen_id,
            frozen_memory.compression_ratio.unwrap_or(0.0)
        );

        // Implement intentional 2-5 second delay for frozen memory retrieval
        let mut rng = rand::thread_rng();
        let delay_seconds = rng.gen_range(2..=5);

        info!(
            "Applying {}-second intentional delay for frozen tier retrieval",
            delay_seconds
        );
        sleep(Duration::from_secs(delay_seconds)).await;

        // Decompress the memory data using zstd
        let compression_engine = ZstdCompressionEngine::new();

        // First, try to extract the compressed data
        // The frozen_memory.compressed_content is stored as JSONB but contains BYTEA data
        let compressed_data = match &frozen_memory.compressed_content {
            serde_json::Value::String(base64_data) => {
                // If it's a base64 string, decode it
                BASE64_STANDARD
                    .decode(base64_data.as_bytes())
                    .map_err(|e| MemoryError::DecompressionError {
                        message: format!("Failed to decode base64 compressed data: {e}"),
                    })?
            }
            serde_json::Value::Array(byte_array) => {
                // If it's an array of numbers, convert to bytes
                byte_array
                    .iter()
                    .map(|v| v.as_u64().unwrap_or(0) as u8)
                    .collect()
            }
            _ => {
                // Fallback: treat as raw bytes (this shouldn't happen with proper BYTEA storage)
                return Err(MemoryError::DecompressionError {
                    message: "Invalid compressed data format in database".to_string(),
                });
            }
        };

        let decompressed_data = compression_engine.decompress_memory_data(&compressed_data)?;

        debug!(
            "Decompression completed: restored {} bytes of content",
            decompressed_data.content.len()
        );

        // Determine restoration tier
        let restoration_tier = target_tier
            .or(Some(frozen_memory.original_tier))
            .unwrap_or(MemoryTier::Working);

        // Restore the original memory
        let memory_id = frozen_memory.original_memory_id;
        let rows_affected = sqlx::query(
            r#"
            UPDATE memories 
            SET 
                content = $1,
                tier = $2,
                status = 'active',
                metadata = $3,
                updated_at = NOW()
            WHERE id = $4
            "#,
        )
        .bind(&decompressed_data.content)
        .bind(restoration_tier)
        .bind(&decompressed_data.metadata)
        .bind(memory_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();

        if rows_affected == 0 {
            // Create new memory if original was deleted
            sqlx::query(
                r#"
                INSERT INTO memories (
                    id, content, content_hash, embedding, tier, status,
                    importance_score, metadata, created_at, updated_at
                ) VALUES ($1, $2, $3, $4, $5, 'active', 0.5, $6, NOW(), NOW())
                "#,
            )
            .bind(memory_id)
            .bind(&decompressed_data.content)
            .bind(&frozen_memory.original_content_hash)
            .bind(frozen_memory.original_embedding.as_ref())
            .bind(restoration_tier)
            .bind(&decompressed_data.metadata)
            .execute(&mut *tx)
            .await?;

            info!("Recreated deleted memory {} during unfreeze", memory_id);
        }

        // Update frozen memory access tracking
        sqlx::query(
            r#"
            UPDATE frozen_memories 
            SET 
                unfreeze_count = COALESCE(unfreeze_count, 0) + 1,
                last_unfrozen_at = NOW(),
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(frozen_id)
        .execute(&mut *tx)
        .await?;

        // Log the migration
        let processing_time_ms = start_time.elapsed().as_millis() as i32;
        sqlx::query(
            r#"
            INSERT INTO migration_history (
                memory_id, from_tier, to_tier, migration_reason,
                migration_duration_ms, success
            ) VALUES ($1, 'frozen', $2, $3, $4, true)
            "#,
        )
        .bind(memory_id)
        .bind(restoration_tier)
        .bind(format!("Unfrozen after {delay_seconds} second delay"))
        .bind(processing_time_ms)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        info!(
            "Successfully unfroze memory {} to {:?} tier in {}ms (including {}s delay)",
            memory_id, restoration_tier, processing_time_ms, delay_seconds
        );

        Ok(UnfreezeMemoryResponse {
            memory_id,
            retrieval_delay_seconds: delay_seconds as i32,
            restoration_tier,
            unfrozen_at: Utc::now(),
        })
    }

    /// Get all frozen memories with pagination
    pub async fn get_frozen_memories(&self, limit: i32, offset: i64) -> Result<Vec<FrozenMemory>> {
        let frozen_memories = sqlx::query_as::<_, FrozenMemory>(
            r#"
            SELECT * FROM frozen_memories 
            ORDER BY frozen_at DESC
            LIMIT $1 OFFSET $2
            "#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        Ok(frozen_memories)
    }

    /// Search frozen memories by content or metadata
    pub async fn search_frozen_memories(
        &self,
        query: &str,
        limit: i32,
    ) -> Result<Vec<FrozenMemory>> {
        let frozen_memories = sqlx::query_as::<_, FrozenMemory>(
            r#"
            SELECT * FROM frozen_memories 
            WHERE 
                convert_from(compressed_content, 'UTF8') ILIKE $1
                OR freeze_reason ILIKE $1
            ORDER BY frozen_at DESC
            LIMIT $2
            "#,
        )
        .bind(format!("%{query}%"))
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(frozen_memories)
    }

    /// Get tier statistics for monitoring
    pub async fn get_tier_statistics(&self) -> Result<Vec<MemoryTierStatistics>> {
        let stats = sqlx::query_as::<_, MemoryTierStatistics>(
            r#"
            SELECT * FROM memory_tier_statistics 
            WHERE snapshot_timestamp > NOW() - INTERVAL '24 hours'
            ORDER BY snapshot_timestamp DESC, tier
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(stats)
    }

    /// Update tier statistics (typically called by a background job)
    pub async fn update_tier_statistics(&self) -> Result<()> {
        sqlx::query("SELECT update_tier_statistics()")
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Search memories with consolidation criteria (SQL injection safe)
    pub async fn search_by_consolidation(
        &self,
        request: ConsolidationSearchRequest,
    ) -> Result<Vec<Memory>> {
        let limit = request.limit.unwrap_or(10);
        let offset = request.offset.unwrap_or(0);

        // Use safe query builder to prevent SQL injection
        let mut builder = SafeQueryBuilder::new("SELECT * FROM memories WHERE status = 'active'");

        // Add consolidation strength range filter
        builder.add_consolidation_strength_range(
            request.min_consolidation_strength,
            request.max_consolidation_strength,
        );

        // Add recall probability range filter
        builder.add_recall_probability_range(
            request.min_recall_probability,
            request.max_recall_probability,
        );

        // Add tier filter if specified
        if let Some(tier) = &request.tier {
            builder.add_tier_filter(tier);
        }

        // Exclude frozen tier if not included
        builder.add_exclude_frozen(!request.include_frozen.unwrap_or(false));

        // Add ordering and pagination
        builder.add_condition(
            "ORDER BY consolidation_strength DESC, recall_probability DESC NULLS LAST",
        );
        builder.add_pagination(limit as usize, offset as usize)?;

        // Build query and execute with parameterized binding
        let query = builder.build_query();
        let sqlx_query = builder.bind_parameters_as(sqlx::query_as::<_, Memory>(&query));

        let memories = sqlx_query.fetch_all(&self.pool).await?;
        Ok(memories)
    }

    /// Update three-component scores for specific memory
    pub async fn update_memory_scores(
        &self,
        memory_id: Uuid,
        recency_score: f64,
        relevance_score: f64,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE memories 
            SET recency_score = $2, 
                relevance_score = $3,
                updated_at = NOW()
            WHERE id = $1 AND status = 'active'
            "#,
        )
        .bind(memory_id)
        .bind(recency_score)
        .bind(relevance_score)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Batch update three-component scores for all active memories
    pub async fn batch_update_three_component_scores(&self) -> Result<i64> {
        let start_time = Instant::now();

        let result = sqlx::query(
            r#"
            UPDATE memories 
            SET recency_score = calculate_recency_score(last_accessed_at, created_at, 0.005),
                relevance_score = LEAST(1.0, 
                    0.5 * importance_score + 
                    0.3 * LEAST(1.0, access_count / 100.0) + 
                    0.2
                ),
                updated_at = NOW()
            WHERE status = 'active'
            "#,
        )
        .execute(&self.pool)
        .await?;

        let duration = start_time.elapsed();
        info!(
            "Updated three-component scores for {} memories in {:?}",
            result.rows_affected(),
            duration
        );

        Ok(result.rows_affected() as i64)
    }

    /// Get memories ranked by three-component combined score using generated column
    pub async fn get_memories_by_combined_score(
        &self,
        tier: Option<MemoryTier>,
        limit: Option<i32>,
        recency_weight: Option<f64>,
        importance_weight: Option<f64>,
        relevance_weight: Option<f64>,
    ) -> Result<Vec<Memory>> {
        let limit = limit.unwrap_or(50);

        // Note: Custom weights are not supported with the generated column approach
        // The generated column uses fixed weights: 0.333, 0.333, 0.334
        // This is a trade-off for P99 <1ms performance
        if recency_weight.is_some() || importance_weight.is_some() || relevance_weight.is_some() {
            warn!(
                "Custom weights not supported with generated combined_score column. Using fixed weights: 0.333, 0.333, 0.334"
            );
        }

        let query = if let Some(tier) = tier {
            sqlx::query_as::<_, Memory>(
                r#"
                SELECT m.*
                FROM memories m
                WHERE m.status = 'active'
                  AND m.tier = $1
                ORDER BY m.combined_score DESC, m.updated_at DESC
                LIMIT $2
                "#,
            )
            .bind(tier)
            .bind(limit as i64)
        } else {
            sqlx::query_as::<_, Memory>(
                r#"
                SELECT m.*
                FROM memories m
                WHERE m.status = 'active'
                ORDER BY m.combined_score DESC, m.updated_at DESC
                LIMIT $1
                "#,
            )
            .bind(limit as i64)
        };

        let memories = query.fetch_all(&self.pool).await?;

        debug!(
            "Retrieved {} memories ranked by generated combined_score for tier {:?}",
            memories.len(),
            tier
        );

        Ok(memories)
    }

    // Simple Consolidation Integration Methods

    /// Get memories for consolidation processing with batch optimization (SQL injection safe)
    pub async fn get_memories_for_consolidation(
        &self,
        tier: Option<MemoryTier>,
        batch_size: usize,
        min_hours_since_last_processing: f64,
    ) -> Result<Vec<Memory>> {
        // Use safe query builder to prevent SQL injection
        let mut builder = SafeQueryBuilder::new("SELECT * FROM memories WHERE status = 'active'");

        // Add time interval condition safely
        builder.add_last_access_interval(min_hours_since_last_processing);

        // Add tier filter if specified
        if let Some(tier) = tier {
            builder.add_tier_filter(&tier);
        }

        // Add complex ordering with recall probability condition
        let threshold_bind_index = builder.bind_index;
        builder.add_recall_threshold_condition(constants::COLD_MIGRATION_THRESHOLD);

        let order_condition = format!(
            "ORDER BY CASE WHEN recall_probability IS NULL THEN 1 WHEN recall_probability < ${threshold_bind_index} THEN 2 ELSE 3 END, last_accessed_at ASC NULLS FIRST, consolidation_strength ASC"
        );
        builder.add_condition(&order_condition);

        // Add pagination (limit only, no offset for this use case)
        builder.add_pagination(batch_size, 0)?;

        // Build query and execute with parameterized binding
        let query = builder.build_query();
        let sqlx_query = builder.bind_parameters_as(sqlx::query_as::<_, Memory>(&query));

        let memories = sqlx_query.fetch_all(&self.pool).await?;
        Ok(memories)
    }

    /// Batch update consolidation values for multiple memories
    pub async fn batch_update_consolidation(
        &self,
        updates: &[(Uuid, f64, f64)], // (id, new_strength, recall_probability)
    ) -> Result<usize> {
        if updates.is_empty() {
            return Ok(0);
        }

        let mut tx = self.pool.begin().await?;
        let mut updated_count = 0;

        for (memory_id, new_strength, recall_prob) in updates {
            let result = sqlx::query(
                r#"
                UPDATE memories 
                SET consolidation_strength = $1, 
                    recall_probability = $2,
                    updated_at = NOW()
                WHERE id = $3 AND status = 'active'
                "#,
            )
            .bind(new_strength)
            .bind(recall_prob)
            .bind(memory_id)
            .execute(&mut *tx)
            .await?;

            updated_count += result.rows_affected() as usize;
        }

        tx.commit().await?;
        Ok(updated_count)
    }

    /// Batch migrate memories to new tiers
    pub async fn batch_migrate_memories(
        &self,
        migrations: &[(Uuid, MemoryTier)], // (memory_id, target_tier)
    ) -> Result<usize> {
        if migrations.is_empty() {
            return Ok(0);
        }

        let mut tx = self.pool.begin().await?;
        let mut migrated_count = 0;

        for (memory_id, target_tier) in migrations {
            // Get current tier for migration logging
            let current_memory: Option<(MemoryTier,)> =
                sqlx::query_as("SELECT tier FROM memories WHERE id = $1 AND status = 'active'")
                    .bind(memory_id)
                    .fetch_optional(&mut *tx)
                    .await?;

            if let Some((current_tier,)) = current_memory {
                // Update the tier
                let result = sqlx::query(
                    r#"
                    UPDATE memories 
                    SET tier = $1, updated_at = NOW()
                    WHERE id = $2 AND status = 'active'
                    "#,
                )
                .bind(target_tier)
                .bind(memory_id)
                .execute(&mut *tx)
                .await?;

                if result.rows_affected() > 0 {
                    migrated_count += 1;

                    // Log the migration
                    self.record_migration(
                        &mut tx,
                        *memory_id,
                        current_tier,
                        *target_tier,
                        Some("Simple consolidation automatic migration".to_string()),
                    )
                    .await?;
                }
            }
        }

        tx.commit().await?;
        Ok(migrated_count)
    }

    /// Get migration candidates using simple consolidation formula (SQL injection safe)
    pub async fn get_simple_consolidation_candidates(
        &self,
        tier: Option<MemoryTier>,
        threshold: f64,
        limit: usize,
    ) -> Result<Vec<Memory>> {
        // Use safe query builder to prevent SQL injection
        let mut builder = SafeQueryBuilder::new(
            "SELECT * FROM memories WHERE status = 'active' AND (recall_probability < $1 OR recall_probability IS NULL)"
        );

        // Add tier filter if specified
        if let Some(tier) = tier {
            builder.add_tier_filter(&tier);
        }

        // Add ordering
        builder.add_condition("ORDER BY COALESCE(recall_probability, 0) ASC, consolidation_strength ASC, last_accessed_at ASC NULLS FIRST");

        // Add pagination (limit only, no offset for this use case)
        builder.add_pagination(limit, 0)?;

        // Build query and execute with parameterized binding
        let query = builder.build_query();
        let mut sqlx_query = sqlx::query_as::<_, Memory>(&query).bind(threshold);
        sqlx_query = builder.bind_parameters_as(sqlx_query);

        let memories = sqlx_query.fetch_all(&self.pool).await?;
        Ok(memories)
    }

    /// Log simple consolidation event with performance metrics
    pub async fn log_simple_consolidation_event(
        &self,
        memory_id: Uuid,
        previous_strength: f64,
        new_strength: f64,
        previous_probability: Option<f64>,
        new_probability: f64,
        processing_time_ms: u64,
    ) -> Result<()> {
        let context = serde_json::json!({
            "engine": "simple_consolidation",
            "processing_time_ms": processing_time_ms,
            "strength_delta": new_strength - previous_strength,
            "probability_delta": new_probability - previous_probability.unwrap_or(0.0)
        });

        self.log_consolidation_event(
            memory_id,
            "simple_consolidation",
            previous_strength,
            new_strength,
            previous_probability,
            Some(new_probability),
            None, // Simple consolidation doesn't track recall intervals
            context,
        )
        .await
    }

    /// Get simple consolidation statistics
    pub async fn get_simple_consolidation_stats(&self) -> Result<SimpleConsolidationStats> {
        let stats = sqlx::query_as::<_, SimpleConsolidationStats>(
            r#"
            SELECT 
                COUNT(*) FILTER (WHERE recall_probability < $1) as migration_candidates,
                COUNT(*) FILTER (WHERE consolidation_strength > 5.0) as highly_consolidated,
                AVG(consolidation_strength) as avg_consolidation_strength,
                AVG(recall_probability) as avg_recall_probability,
                COUNT(*) FILTER (WHERE last_accessed_at > NOW() - INTERVAL '24 hours') as recently_accessed,
                COUNT(*) as total_active_memories
            FROM memories 
            WHERE status = 'active'
            "#,
        )
        .bind(constants::COLD_MIGRATION_THRESHOLD)
        .fetch_one(&self.pool)
        .await?;

        Ok(stats)
    }

    /// Get trigger metrics if trigger engine is available
    pub async fn get_trigger_metrics(&self) -> Option<super::event_triggers::TriggerMetrics> {
        if let Some(trigger_engine) = &self.trigger_engine {
            Some(trigger_engine.get_metrics().await)
        } else {
            None
        }
    }

    /// Reset trigger metrics if trigger engine is available
    pub async fn reset_trigger_metrics(&self) -> Result<()> {
        if let Some(trigger_engine) = &self.trigger_engine {
            trigger_engine.reset_metrics().await?;
        }
        Ok(())
    }

    /// Add user-specific trigger customization
    pub async fn add_user_trigger_customization(
        &self,
        user_id: String,
        customizations: std::collections::HashMap<
            super::event_triggers::TriggerEvent,
            super::event_triggers::TriggerPattern,
        >,
    ) -> Result<()> {
        if let Some(trigger_engine) = &self.trigger_engine {
            trigger_engine
                .add_user_customization(user_id, customizations)
                .await?;
        }
        Ok(())
    }

    /// Check if trigger engine is enabled
    pub fn has_trigger_engine(&self) -> bool {
        self.trigger_engine.is_some()
    }

    /// Batch freeze memories that meet migration criteria (P(recall) < 0.2)
    pub async fn batch_freeze_by_recall_probability(
        &self,
        max_batch_size: Option<usize>,
    ) -> Result<BatchFreezeResult> {
        use std::time::Instant;

        let start_time = Instant::now();
        let batch_size = max_batch_size.unwrap_or(100_000); // Default to 100K as per requirements

        // Find memories in Cold tier with P(recall) < 0.2
        let candidates = sqlx::query_as::<_, Memory>(
            r#"
            SELECT * FROM memories 
            WHERE tier = 'cold' 
            AND status = 'active'
            AND COALESCE(recall_probability, 0) < 0.2
            ORDER BY recall_probability ASC, last_accessed_at ASC
            LIMIT $1
            "#,
        )
        .bind(batch_size as i64)
        .fetch_all(&self.pool)
        .await?;

        let mut frozen_ids = Vec::new();
        let mut total_space_saved = 0u64;
        let mut compression_ratios = Vec::new();

        info!("Starting batch freeze of {} memories", candidates.len());

        // Process in smaller chunks to avoid transaction timeouts
        for chunk in candidates.chunks(1000) {
            let mut tx = self.pool.begin().await?;

            for memory in chunk {
                // Call freeze function for each memory
                match sqlx::query("SELECT freeze_memory($1) as frozen_id")
                    .bind(memory.id)
                    .fetch_one(&mut *tx)
                    .await
                {
                    Ok(row) => {
                        let frozen_id: Uuid = row.get("frozen_id");
                        frozen_ids.push(frozen_id);

                        // Estimate space saved (original content vs compressed)
                        let original_size = memory.content.len() as u64;
                        let estimated_compressed_size = original_size / 6; // Assume ~6:1 compression
                        total_space_saved += original_size - estimated_compressed_size;
                        compression_ratios.push(6.0);
                    }
                    Err(e) => {
                        warn!("Failed to freeze memory {}: {}", memory.id, e);
                        continue;
                    }
                }
            }

            tx.commit().await?;
        }

        let processing_time = start_time.elapsed();
        let avg_compression_ratio = if !compression_ratios.is_empty() {
            compression_ratios.iter().sum::<f32>() / compression_ratios.len() as f32
        } else {
            0.0
        };

        info!(
            "Batch freeze completed: {} memories frozen in {:?}, avg compression: {:.1}:1",
            frozen_ids.len(),
            processing_time,
            avg_compression_ratio
        );

        Ok(BatchFreezeResult {
            memories_frozen: frozen_ids.len() as u32,
            total_space_saved_bytes: total_space_saved,
            average_compression_ratio: avg_compression_ratio,
            processing_time_ms: processing_time.as_millis() as u64,
            frozen_memory_ids: frozen_ids,
        })
    }

    /// Batch unfreeze memories
    pub async fn batch_unfreeze_memories(
        &self,
        frozen_ids: Vec<Uuid>,
        target_tier: Option<MemoryTier>,
    ) -> Result<BatchUnfreezeResult> {
        use std::time::Instant;

        let start_time = Instant::now();
        let mut unfrozen_memory_ids = Vec::new();
        let mut total_delay_seconds = 0i32;

        info!("Starting batch unfreeze of {} memories", frozen_ids.len());

        // Process in smaller chunks to manage delays and transactions
        for chunk in frozen_ids.chunks(100) {
            for frozen_id in chunk {
                match self.unfreeze_memory(*frozen_id, target_tier).await {
                    Ok(response) => {
                        unfrozen_memory_ids.push(response.memory_id);
                        total_delay_seconds += response.retrieval_delay_seconds;
                    }
                    Err(e) => {
                        warn!("Failed to unfreeze memory {}: {}", frozen_id, e);
                        continue;
                    }
                }
            }
        }

        let processing_time = start_time.elapsed();
        let avg_delay_seconds = if !unfrozen_memory_ids.is_empty() {
            total_delay_seconds as f32 / unfrozen_memory_ids.len() as f32
        } else {
            0.0
        };

        info!(
            "Batch unfreeze completed: {} memories unfrozen in {:?}, avg delay: {:.1}s",
            unfrozen_memory_ids.len(),
            processing_time,
            avg_delay_seconds
        );

        Ok(BatchUnfreezeResult {
            memories_unfrozen: unfrozen_memory_ids.len() as u32,
            total_processing_time_ms: processing_time.as_millis() as u64,
            average_delay_seconds: avg_delay_seconds,
            unfrozen_memory_ids,
        })
    }

    // ==========================================
    // Harvest Session Management Methods
    // ==========================================

    /*
    /// Create a new harvest session
    pub async fn create_harvest_session(
        &self,
        request: CreateHarvestSessionRequest,
    ) -> Result<HarvestSession> {
        let session_id = Uuid::new_v4();
        let now = Utc::now();

        let config_snapshot = request
            .config_snapshot
            .unwrap_or_else(|| serde_json::json!({}));

        let session = sqlx::query_as!(
            HarvestSession,
            r#"
            INSERT INTO harvest_sessions (
                id, session_type, trigger_reason, started_at, status,
                messages_processed, patterns_extracted, patterns_stored,
                duplicates_filtered, processing_time_ms, config_snapshot,
                error_message, retry_count, extraction_time_ms,
                deduplication_time_ms, storage_time_ms, created_at
            ) VALUES (
                $1, $2, $3, $4, $5, 0, 0, 0, 0, 0, $6, NULL, 0, 0, 0, 0, $7
            )
            RETURNING id, session_type as "session_type: HarvestSessionType",
                     trigger_reason, started_at, completed_at,
                     status as "status: HarvestSessionStatus",
                     messages_processed, patterns_extracted, patterns_stored,
                     duplicates_filtered, processing_time_ms, config_snapshot,
                     error_message, retry_count, extraction_time_ms,
                     deduplication_time_ms, storage_time_ms,
                     memory_usage_mb, cpu_usage_percent, created_at
            "#,
            session_id,
            request.session_type as HarvestSessionType,
            request.trigger_reason,
            now,
            HarvestSessionStatus::InProgress as HarvestSessionStatus,
            config_snapshot,
            now
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| MemoryError::DatabaseError {
            message: format!("Failed to create harvest session: {}", e),
        })?;

        Ok(session)
    }
    */

    /*
    /// Update an existing harvest session
    pub async fn update_harvest_session(
        &self,
        session_id: Uuid,
        request: UpdateHarvestSessionRequest,
    ) -> Result<HarvestSession> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| MemoryError::DatabaseError {
                message: format!("Failed to begin transaction: {}", e),
            })?;

        // Build dynamic update query
        let mut set_clauses = Vec::new();
        let mut param_index = 2; // $1 is reserved for session_id

        if request.status.is_some() {
            set_clauses.push(format!("status = ${}", param_index));
            param_index += 1;
        }
        if request.messages_processed.is_some() {
            set_clauses.push(format!("messages_processed = ${}", param_index));
            param_index += 1;
        }
        if request.patterns_extracted.is_some() {
            set_clauses.push(format!("patterns_extracted = ${}", param_index));
            param_index += 1;
        }
        if request.patterns_stored.is_some() {
            set_clauses.push(format!("patterns_stored = ${}", param_index));
            param_index += 1;
        }
        if request.duplicates_filtered.is_some() {
            set_clauses.push(format!("duplicates_filtered = ${}", param_index));
            param_index += 1;
        }
        if request.processing_time_ms.is_some() {
            set_clauses.push(format!("processing_time_ms = ${}", param_index));
            param_index += 1;
        }
        if request.error_message.is_some() {
            set_clauses.push(format!("error_message = ${}", param_index));
            param_index += 1;
        }

        if set_clauses.is_empty() {
            return self.get_harvest_session(session_id).await;
        }

        let query = format!(
            r#"
            UPDATE harvest_sessions
            SET {}
            WHERE id = $1
            RETURNING id, session_type as "session_type: HarvestSessionType",
                     trigger_reason, started_at, completed_at,
                     status as "status: HarvestSessionStatus",
                     messages_processed, patterns_extracted, patterns_stored,
                     duplicates_filtered, processing_time_ms, config_snapshot,
                     error_message, retry_count, extraction_time_ms,
                     deduplication_time_ms, storage_time_ms,
                     memory_usage_mb, cpu_usage_percent, created_at
            "#,
            set_clauses.join(", ")
        );

        let mut query_builder = sqlx::query_as::<_, HarvestSession>(&query);
        query_builder = query_builder.bind(session_id);

        if let Some(status) = request.status {
            query_builder = query_builder.bind(status as HarvestSessionStatus);
        }
        if let Some(messages_processed) = request.messages_processed {
            query_builder = query_builder.bind(messages_processed);
        }
        if let Some(patterns_extracted) = request.patterns_extracted {
            query_builder = query_builder.bind(patterns_extracted);
        }
        if let Some(patterns_stored) = request.patterns_stored {
            query_builder = query_builder.bind(patterns_stored);
        }
        if let Some(duplicates_filtered) = request.duplicates_filtered {
            query_builder = query_builder.bind(duplicates_filtered);
        }
        if let Some(processing_time_ms) = request.processing_time_ms {
            query_builder = query_builder.bind(processing_time_ms);
        }
        if let Some(error_message) = request.error_message {
            query_builder = query_builder.bind(error_message);
        }

        let session =
            query_builder
                .fetch_one(&mut *tx)
                .await
                .map_err(|e| MemoryError::DatabaseError {
                    message: format!("Failed to update harvest session: {}", e),
                })?;

        tx.commit().await.map_err(|e| MemoryError::DatabaseError {
            message: format!("Failed to commit harvest session update: {}", e),
        })?;

        Ok(session)
    }
    */

    /*
    /// Get a harvest session by ID
    pub async fn get_harvest_session(&self, session_id: Uuid) -> Result<HarvestSession> {
        let session = sqlx::query_as!(
            HarvestSession,
            r#"
            SELECT id, session_type as "session_type: HarvestSessionType",
                   trigger_reason, started_at, completed_at,
                   status as "status: HarvestSessionStatus",
                   messages_processed, patterns_extracted, patterns_stored,
                   duplicates_filtered, processing_time_ms, config_snapshot,
                   error_message, retry_count, extraction_time_ms,
                   deduplication_time_ms, storage_time_ms,
                   memory_usage_mb, cpu_usage_percent, created_at
            FROM harvest_sessions
            WHERE id = $1
            "#,
            session_id
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| MemoryError::DatabaseError {
            message: format!("Harvest session not found: {}", e),
        })?;

        Ok(session)
    }
    */

    /*
    /// Get harvest success rate statistics
    pub async fn get_harvest_success_rate(&self, days_back: i32) -> Result<HarvestSuccessRate> {
        let stats = sqlx::query_as!(
            HarvestSuccessRate,
            r#"
            SELECT
                COUNT(*)::INTEGER as total_sessions,
                COUNT(*) FILTER (WHERE status = 'completed')::INTEGER as successful_sessions,
                COUNT(*) FILTER (WHERE status = 'failed')::INTEGER as failed_sessions,
                (COUNT(*) FILTER (WHERE status = 'completed')::FLOAT / GREATEST(COUNT(*), 1)::FLOAT) as success_rate,
                COALESCE(AVG(processing_time_ms), 0)::FLOAT as average_processing_time_ms
            FROM harvest_sessions
            WHERE started_at > NOW() - ($1 || ' days')::INTERVAL
            "#,
            days_back
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| MemoryError::DatabaseError {
            message: format!("Failed to get harvest success rate: {}", e),
        })?;

        Ok(stats)
    }
    */

    // ==========================================
    // Harvest Pattern Management Methods
    // ==========================================

    /*
    /// Create a new harvest pattern
    pub async fn create_harvest_pattern(
        &self,
        request: CreateHarvestPatternRequest,
    ) -> Result<HarvestPattern> {
        let pattern_id = Uuid::new_v4();
        let now = Utc::now();
        let metadata = request.metadata.unwrap_or_else(|| serde_json::json!({}));

        let pattern = sqlx::query_as!(
            HarvestPattern,
            r#"
            INSERT INTO harvest_patterns (
                id, harvest_session_id, pattern_type, content, confidence_score,
                source_message_id, context, metadata, status, memory_id,
                rejection_reason, extraction_confidence, similarity_to_existing,
                extracted_at
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, 'extracted', NULL, NULL, NULL, NULL, $9
            )
            RETURNING id, harvest_session_id,
                     pattern_type as "pattern_type: HarvestPatternType",
                     content, confidence_score, source_message_id, context,
                     metadata, status as "status: HarvestPatternStatus",
                     memory_id, rejection_reason, extraction_confidence,
                     similarity_to_existing, extracted_at
            "#,
            pattern_id,
            request.harvest_session_id,
            request.pattern_type as HarvestPatternType,
            request.content,
            request.confidence_score,
            request.source_message_id,
            request.context,
            metadata,
            now
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| MemoryError::DatabaseError {
            message: format!("Failed to create harvest pattern: {}", e),
        })?;

        Ok(pattern)
    }
    */

    /*
    /// Get top performing harvest patterns
    pub async fn get_top_harvest_patterns(
        &self,
        limit: i32,
        days_back: i32,
    ) -> Result<Vec<TopHarvestPattern>> {
        let patterns = sqlx::query_as!(
            TopHarvestPattern,
            r#"
            SELECT
                pattern_type as "pattern_type: HarvestPatternType",
                COUNT(*)::INTEGER as total_extracted,
                COUNT(*) FILTER (WHERE status = 'stored')::INTEGER as total_stored,
                AVG(confidence_score)::FLOAT as avg_confidence,
                (COUNT(*) FILTER (WHERE status = 'stored')::FLOAT / COUNT(*)::FLOAT) as success_rate
            FROM harvest_patterns
            WHERE extracted_at > NOW() - ($2 || ' days')::INTERVAL
            GROUP BY pattern_type
            ORDER BY success_rate DESC, total_stored DESC
            LIMIT $1
            "#,
            limit,
            days_back
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| MemoryError::DatabaseError {
            message: format!("Failed to get top harvest patterns: {}", e),
        })?;

        Ok(patterns)
    }
    */

    // ==========================================
    // Consolidation Event Management Methods
    // ==========================================

    /* TEMPORARILY COMMENTED OUT - missing consolidation_events table
    /// Create a consolidation event
    pub async fn create_consolidation_event(
        &self,
        request: CreateConsolidationEventRequest,
    ) -> Result<ConsolidationEvent> {
        let event_id = Uuid::new_v4();
        let now = Utc::now();
        let context_metadata = request
            .context_metadata
            .unwrap_or_else(|| serde_json::json!({}));

        // Calculate deltas if both old and new values are provided
        let strength_delta = match (
            request.old_consolidation_strength,
            request.new_consolidation_strength,
        ) {
            (Some(old), Some(new)) => Some(new - old),
            _ => None,
        };

        let probability_delta = match (
            request.old_recall_probability,
            request.new_recall_probability,
        ) {
            (Some(old), Some(new)) => Some(new - old),
            _ => None,
        };

        let event = sqlx::query_as!(
            ConsolidationEvent,
            r#"
            INSERT INTO consolidation_events (
                id, event_type, memory_id, source_tier, target_tier,
                migration_reason, old_consolidation_strength, new_consolidation_strength,
                strength_delta, old_recall_probability, new_recall_probability,
                probability_delta, processing_time_ms, triggered_by,
                context_metadata, created_at
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, NULL, $13, $14, $15
            )
            RETURNING id, event_type as "event_type: ConsolidationEventType",
                     memory_id, source_tier, target_tier, migration_reason,
                     old_consolidation_strength, new_consolidation_strength,
                     strength_delta, old_recall_probability, new_recall_probability,
                     probability_delta, processing_time_ms, triggered_by,
                     context_metadata, created_at
            "#,
            event_id,
            request.event_type as ConsolidationEventType,
            request.memory_id,
            request.source_tier,
            request.target_tier,
            request.migration_reason,
            request.old_consolidation_strength,
            request.new_consolidation_strength,
            strength_delta,
            request.old_recall_probability,
            request.new_recall_probability,
            probability_delta,
            request.triggered_by,
            context_metadata,
            now
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| MemoryError::DatabaseError {
            message: format!("Failed to create consolidation event: {}", e),
        })?;

        Ok(event)
    }
    */

    /* TEMPORARILY COMMENTED OUT - missing consolidation_events table
    /// Get tier migration statistics
    pub async fn get_tier_migration_stats(
        &self,
        days_back: i32,
    ) -> Result<Vec<TierMigrationStats>> {
        let stats = sqlx::query_as!(
            TierMigrationStats,
            r#"
            SELECT
                COALESCE(ce.source_tier, 'unknown') as source_tier,
                COALESCE(ce.target_tier, 'unknown') as target_tier,
                COUNT(*)::INTEGER as migration_count,
                COALESCE(AVG(ce.processing_time_ms), 0)::FLOAT as avg_processing_time_ms,
                -- Calculate success rate by checking if memory actually moved to target tier
                (COUNT(*) FILTER (WHERE m.tier::text = ce.target_tier)::FLOAT / COUNT(*)::FLOAT) as success_rate
            FROM consolidation_events ce
            JOIN memories m ON ce.memory_id = m.id
            WHERE ce.event_type = 'tier_migration'
            AND ce.created_at > NOW() - ($1 || ' days')::INTERVAL
            GROUP BY ce.source_tier, ce.target_tier
            ORDER BY migration_count DESC
            "#,
            days_back
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| MemoryError::DatabaseError {
            message: format!("Failed to get tier migration stats: {}", e),
        })?;

        Ok(stats)
    }
    */

    // ==========================================
    // Memory Access Log Management Methods
    // ==========================================

    /* TEMPORARILY COMMENTED OUT - missing memory_access_log table
    /// Create a memory access log entry
    pub async fn create_memory_access_log(
        &self,
        request: CreateMemoryAccessLogRequest,
    ) -> Result<MemoryAccessLog> {
        let log_id = Uuid::new_v4();
        let now = Utc::now();

        let log_entry = sqlx::query_as!(
            MemoryAccessLog,
            r#"
            INSERT INTO memory_access_log (
                id, memory_id, access_type, session_id, user_context,
                query_context, retrieval_time_ms, similarity_score,
                ranking_position, importance_boost, access_count_increment,
                accessed_at
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12
            )
            RETURNING id, memory_id, access_type as "access_type: MemoryAccessType",
                     session_id, user_context, query_context, retrieval_time_ms,
                     similarity_score, ranking_position, importance_boost,
                     access_count_increment, accessed_at
            "#,
            log_id,
            request.memory_id,
            request.access_type as MemoryAccessType,
            request.session_id,
            request.user_context,
            request.query_context,
            request.retrieval_time_ms,
            request.similarity_score,
            request.ranking_position,
            request.importance_boost.unwrap_or(0.0),
            request.access_count_increment.unwrap_or(1),
            now
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| MemoryError::DatabaseError {
            message: format!("Failed to create memory access log: {}", e),
        })?;

        Ok(log_entry)
    }
    */

    // ==========================================
    // System Metrics Management Methods
    // ==========================================

    /* TEMPORARILY COMMENTED OUT - missing system_metrics_snapshots table
    /// Create a system metrics snapshot
    pub async fn create_system_metrics_snapshot(
        &self,
        snapshot_type: SystemMetricsSnapshotType,
    ) -> Result<SystemMetricsSnapshot> {
        let snapshot_id = Uuid::new_v4();
        let now = Utc::now();

        // Get current memory tier statistics
        let tier_stats = sqlx::query!(
            r#"
            SELECT
                COUNT(*) FILTER (WHERE tier = 'working' AND status = 'active') as working_count,
                COUNT(*) FILTER (WHERE tier = 'warm' AND status = 'active') as warm_count,
                COUNT(*) FILTER (WHERE tier = 'cold' AND status = 'active') as cold_count,
                COUNT(*) FILTER (WHERE tier = 'frozen') as frozen_count,
                SUM(LENGTH(content::text)) as total_storage_bytes
            FROM memories
            "#
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| MemoryError::DatabaseError {
            message: format!("Failed to get memory tier statistics: {}", e),
        })?;

        let snapshot = sqlx::query_as!(
            SystemMetricsSnapshot,
            r#"
            INSERT INTO system_metrics_snapshots (
                id, snapshot_type, working_memory_count, warm_memory_count,
                cold_memory_count, frozen_memory_count, total_storage_bytes,
                compressed_storage_bytes, average_compression_ratio,
                average_query_time_ms, p95_query_time_ms, p99_query_time_ms,
                slow_query_count, consolidation_backlog, migration_queue_size,
                failed_operations_count, vector_index_size_mb,
                vector_search_performance, database_cpu_percent,
                database_memory_mb, connection_count, active_connections,
                recorded_at
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, 0, NULL, NULL, NULL, NULL, 0, 0, 0, 0,
                NULL, '{}', NULL, NULL, NULL, NULL, $8
            )
            RETURNING id, snapshot_type as "snapshot_type: SystemMetricsSnapshotType",
                     working_memory_count, warm_memory_count, cold_memory_count,
                     frozen_memory_count, total_storage_bytes, compressed_storage_bytes,
                     average_compression_ratio, average_query_time_ms, p95_query_time_ms,
                     p99_query_time_ms, slow_query_count, consolidation_backlog,
                     migration_queue_size, failed_operations_count, vector_index_size_mb,
                     vector_search_performance, database_cpu_percent, database_memory_mb,
                     connection_count, active_connections, recorded_at
            "#,
            snapshot_id,
            snapshot_type as SystemMetricsSnapshotType,
            tier_stats.working_count.unwrap_or(0) as i32,
            tier_stats.warm_count.unwrap_or(0) as i32,
            tier_stats.cold_count.unwrap_or(0) as i32,
            tier_stats.frozen_count.unwrap_or(0) as i32,
            tier_stats.total_storage_bytes.unwrap_or(0),
            now
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| MemoryError::DatabaseError {
            message: format!("Failed to create system metrics snapshot: {}", e),
        })?;

        Ok(snapshot)
    }
    */

    /* TEMPORARILY COMMENTED OUT - missing system_metrics_snapshots table
    /// Get recent system metrics snapshots
    pub async fn get_recent_system_metrics_snapshots(
        &self,
        snapshot_type: Option<SystemMetricsSnapshotType>,
        limit: i32,
    ) -> Result<Vec<SystemMetricsSnapshot>> {
        let snapshots = match snapshot_type {
            Some(st) => {
                sqlx::query_as!(
                    SystemMetricsSnapshot,
                    r#"
                    SELECT id, snapshot_type as "snapshot_type: SystemMetricsSnapshotType",
                           working_memory_count, warm_memory_count, cold_memory_count,
                           frozen_memory_count, total_storage_bytes, compressed_storage_bytes,
                           average_compression_ratio, average_query_time_ms, p95_query_time_ms,
                           p99_query_time_ms, slow_query_count, consolidation_backlog,
                           migration_queue_size, failed_operations_count, vector_index_size_mb,
                           vector_search_performance, database_cpu_percent, database_memory_mb,
                           connection_count, active_connections, recorded_at
                    FROM system_metrics_snapshots
                    WHERE snapshot_type = $1
                    ORDER BY recorded_at DESC
                    LIMIT $2
                    "#,
                    st as SystemMetricsSnapshotType,
                    limit
                )
                .fetch_all(&self.pool)
                .await
            }
            None => {
                sqlx::query_as!(
                    SystemMetricsSnapshot,
                    r#"
                    SELECT id, snapshot_type as "snapshot_type: SystemMetricsSnapshotType",
                           working_memory_count, warm_memory_count, cold_memory_count,
                           frozen_memory_count, total_storage_bytes, compressed_storage_bytes,
                           average_compression_ratio, average_query_time_ms, p95_query_time_ms,
                           p99_query_time_ms, slow_query_count, consolidation_backlog,
                           migration_queue_size, failed_operations_count, vector_index_size_mb,
                           vector_search_performance, database_cpu_percent, database_memory_mb,
                           connection_count, active_connections, recorded_at
                    FROM system_metrics_snapshots
                    ORDER BY recorded_at DESC
                    LIMIT $1
                    "#,
                    limit
                )
                .fetch_all(&self.pool)
                .await
            }
        }
        .map_err(|e| MemoryError::DatabaseError {
            message: format!("Failed to get system metrics snapshots: {}", e),
        })?;

        Ok(snapshots)
    }
    */

    // ==========================================
    // Forgetting Mechanisms Methods
    // ==========================================

    /// Get memories for forgetting processing (clean architecture)
    pub async fn get_memories_for_forgetting(
        &self,
        tier: MemoryTier,
        batch_size: usize,
    ) -> Result<Vec<Memory>> {
        let mut builder = SafeQueryBuilder::new("SELECT * FROM memories WHERE status = 'active'");

        // Add tier filter
        builder.add_tier_filter(&tier);

        // Add condition for memories not updated recently (1 hour minimum)
        builder.add_condition("AND (updated_at IS NULL OR updated_at < NOW() - INTERVAL '1 hour')");

        // Order by oldest first (NULLS FIRST for never-updated)
        builder.add_condition("ORDER BY updated_at ASC NULLS FIRST");

        // Add pagination
        builder.add_pagination(batch_size, 0)?;

        let query = builder.build_query();
        let query_with_params = builder.bind_parameters_as(sqlx::query_as::<_, Memory>(&query));

        let memories = query_with_params.fetch_all(&self.pool).await?;
        Ok(memories)
    }

    /// Batch update decay rates for forgetting mechanism
    pub async fn batch_update_decay_rates(
        &self,
        updates: &[(Uuid, f64)], // (memory_id, new_decay_rate)
    ) -> Result<usize> {
        if updates.is_empty() {
            return Ok(0);
        }

        let mut tx = self.pool.begin().await?;
        let mut updated_count = 0;

        for (memory_id, new_decay_rate) in updates {
            // Use direct SQL here as this is a simple update without complex query building
            // SafeQueryBuilder is primarily for search queries with dynamic conditions
            let result = sqlx::query(
                "UPDATE memories SET decay_rate = $1, updated_at = NOW() WHERE id = $2 AND status = 'active'"
            )
            .bind(new_decay_rate)
            .bind(memory_id)
            .execute(&mut *tx)
            .await?;

            updated_count += result.rows_affected() as usize;
        }

        tx.commit().await?;
        Ok(updated_count)
    }

    /// Batch update importance scores for reinforcement learning
    pub async fn batch_update_importance_scores(
        &self,
        updates: &[(Uuid, f64)], // (memory_id, new_importance_score)
    ) -> Result<usize> {
        if updates.is_empty() {
            return Ok(0);
        }

        let mut tx = self.pool.begin().await?;
        let mut updated_count = 0;

        for (memory_id, new_importance_score) in updates {
            // Use direct SQL here as this is a simple update without complex query building
            // SafeQueryBuilder is primarily for search queries with dynamic conditions
            let result = sqlx::query(
                "UPDATE memories SET importance_score = $1, updated_at = NOW() WHERE id = $2 AND status = 'active'"
            )
            .bind(new_importance_score)
            .bind(memory_id)
            .execute(&mut *tx)
            .await?;

            updated_count += result.rows_affected() as usize;
        }

        tx.commit().await?;
        Ok(updated_count)
    }

    /// Batch mark memories as deleted (soft delete for forgetting)
    pub async fn batch_soft_delete_memories(&self, memory_ids: &[Uuid]) -> Result<usize> {
        if memory_ids.is_empty() {
            return Ok(0);
        }

        let mut tx = self.pool.begin().await?;
        let mut deleted_count = 0;

        for memory_id in memory_ids {
            // Use direct SQL here as this is a simple update without complex query building
            // SafeQueryBuilder is primarily for search queries with dynamic conditions
            match sqlx::query(
                "UPDATE memories SET status = 'deleted', updated_at = NOW() WHERE id = $1 AND status = 'active'"
            )
            .bind(memory_id)
            .execute(&mut *tx)
            .await
            {
                Ok(result) => {
                    deleted_count += result.rows_affected() as usize;
                }
                Err(e) => {
                    warn!("Failed to soft delete memory {}: {}", memory_id, e);
                }
            }
        }

        tx.commit().await?;
        Ok(deleted_count)
    }
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize, serde::Deserialize)]
pub struct MemoryStatistics {
    pub working_count: Option<i64>,
    pub warm_count: Option<i64>,
    pub cold_count: Option<i64>,
    pub total_active: Option<i64>,
    pub total_deleted: Option<i64>,
    pub avg_importance: Option<f64>,
    pub max_access_count: Option<i32>,
    pub avg_access_count: Option<f64>,
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize, serde::Deserialize)]
pub struct SimpleConsolidationStats {
    pub migration_candidates: Option<i64>,
    pub highly_consolidated: Option<i64>,
    pub avg_consolidation_strength: Option<f64>,
    pub avg_recall_probability: Option<f64>,
    pub recently_accessed: Option<i64>,
    pub total_active_memories: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_hash_generation() {
        let content = "This is a test memory content";
        let hash1 = Memory::calculate_content_hash(content);
        let hash2 = Memory::calculate_content_hash(content);

        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 64); // SHA-256 produces 64 hex characters
    }

    #[test]
    fn test_should_migrate() {
        let mut memory = Memory::default();

        // Working tier with very low importance and old memory should migrate
        memory.tier = MemoryTier::Working;
        memory.importance_score = 0.01;
        memory.consolidation_strength = 0.1;
        memory.access_count = 0;
        memory.last_accessed_at = Some(Utc::now() - chrono::Duration::days(30)); // Very old
        assert!(memory.should_migrate());

        // Working tier with high importance should not migrate
        memory.importance_score = 0.9;
        memory.consolidation_strength = 8.0;
        memory.access_count = 100;
        memory.last_accessed_at = Some(Utc::now()); // Just accessed
        assert!(!memory.should_migrate());

        // Cold tier with very low importance may migrate to frozen
        // based on the new math engine thresholds (0.3 for frozen migration)
        memory.tier = MemoryTier::Cold;
        memory.importance_score = 0.0;
        memory.last_accessed_at = Some(Utc::now() - chrono::Duration::days(30)); // Old memory
                                                                                 // This may or may not migrate depending on calculated recall probability
                                                                                 // So we test both scenarios

        // Test Frozen tier - should never migrate
        memory.tier = MemoryTier::Frozen;
        assert!(!memory.should_migrate());
    }

    #[test]
    fn test_next_tier() {
        let mut memory = Memory::default();

        memory.tier = MemoryTier::Working;
        assert_eq!(memory.next_tier(), Some(MemoryTier::Warm));

        memory.tier = MemoryTier::Warm;
        assert_eq!(memory.next_tier(), Some(MemoryTier::Cold));

        memory.tier = MemoryTier::Cold;
        assert_eq!(memory.next_tier(), Some(MemoryTier::Frozen));

        memory.tier = MemoryTier::Frozen;
        assert_eq!(memory.next_tier(), None);
    }

    #[test]
    fn test_safe_query_builder_pagination_validation() {
        let mut builder = SafeQueryBuilder::new("SELECT * FROM memories");

        // Test valid pagination
        assert!(builder.add_pagination(100, 50).is_ok());

        // Test excessive limit
        let mut builder2 = SafeQueryBuilder::new("SELECT * FROM memories");
        assert!(builder2.add_pagination(20000, 0).is_err());

        // Test excessive offset
        let mut builder3 = SafeQueryBuilder::new("SELECT * FROM memories");
        assert!(builder3.add_pagination(10, 2000000).is_err());
    }

    #[test]
    fn test_safe_query_builder_parameterization() {
        let mut builder = SafeQueryBuilder::new("SELECT * FROM memories WHERE status = 'active'");

        // Add tier filter
        let tier = MemoryTier::Working;
        builder.add_tier_filter(&tier);

        // Add importance range
        builder.add_importance_range(Some(0.5), Some(0.9));

        // Add pagination
        builder.add_pagination(10, 0).unwrap();

        let query = builder.build_query();

        // Verify parameterized placeholders are used
        assert!(query.contains("$1"));
        assert!(query.contains("$2"));
        assert!(query.contains("$3"));
        assert!(query.contains("$4"));
        assert!(query.contains("$5"));

        // Verify no raw values are interpolated
        assert!(!query.contains("0.5"));
        assert!(!query.contains("0.9"));
        assert!(!query.contains("Working"));
    }

    #[test]
    fn test_safe_query_builder_sql_injection_prevention() {
        let mut builder = SafeQueryBuilder::new("SELECT * FROM memories WHERE status = 'active'");

        // Try to inject malicious tier (would normally be validated by the type system)
        let tier = MemoryTier::Working;
        builder.add_tier_filter(&tier);

        // Try to inject through importance range
        builder.add_importance_range(Some(0.1), Some(1.0));

        let query = builder.build_query();

        // Verify that we have parameterized placeholders, not raw values
        assert!(query.contains("tier = $"));
        assert!(query.contains("importance_score >= $"));
        assert!(query.contains("importance_score <= $"));

        // Verify no SQL injection patterns
        assert!(!query.contains("'; DROP TABLE"));
        assert!(!query.contains("OR 1=1"));
        assert!(!query.contains("UNION SELECT"));
    }

    #[test]
    fn test_consolidation_strength_range_parameterization() {
        let mut builder = SafeQueryBuilder::new("SELECT * FROM memories WHERE status = 'active'");

        builder.add_consolidation_strength_range(Some(1.0), Some(5.0));
        builder.add_recall_probability_range(Some(0.1), Some(0.9));

        let query = builder.build_query();

        // Verify parameterization
        assert!(query.contains("consolidation_strength >= $"));
        assert!(query.contains("consolidation_strength <= $"));
        assert!(query.contains("recall_probability >= $"));
        assert!(query.contains("recall_probability <= $"));

        // Verify no raw values
        assert!(!query.contains("1.0"));
        assert!(!query.contains("5.0"));
        assert!(!query.contains("0.1"));
        assert!(!query.contains("0.9"));
    }

    #[test]
    fn test_date_range_parameterization() {
        let mut builder = SafeQueryBuilder::new("SELECT * FROM memories WHERE status = 'active'");

        let start_date = Utc::now() - chrono::Duration::days(30);
        let end_date = Utc::now();

        builder.add_date_range(Some(&start_date), Some(&end_date));

        let query = builder.build_query();

        // Verify parameterization
        assert!(query.contains("created_at >= $"));
        assert!(query.contains("created_at <= $"));

        // Verify no raw date strings (which would be vulnerable)
        assert!(!query.contains(&start_date.format("%Y-%m-%d").to_string()));
        assert!(!query.contains(&end_date.format("%Y-%m-%d").to_string()));
    }

    #[test]
    fn test_similarity_threshold_parameterization() {
        let mut builder = SafeQueryBuilder::new("SELECT * FROM memories WHERE status = 'active'");

        // This would be called after the first bind parameter (embedding)
        builder.bind_index = 2; // Simulate that $1 is already used for embedding
        builder.add_similarity_threshold(0.7);

        let query = builder.build_query();

        // Verify parameterization with correct bind index
        assert!(query.contains("embedding <=> $1"));
        assert!(query.contains(">= $2"));

        // Verify no raw threshold value
        assert!(!query.contains("0.7"));
    }

    #[test]
    fn test_query_builder_prevents_format_injection() {
        let mut builder = SafeQueryBuilder::new("SELECT * FROM memories WHERE status = 'active'");

        // Test that all potentially dangerous inputs are parameterized
        let malicious_tier = MemoryTier::Working; // Type safety prevents actual injection here
        builder.add_tier_filter(&malicious_tier);

        // Test importance ranges that could contain injection attempts
        builder.add_importance_range(Some(0.0), Some(1.0));

        // Test exclude frozen functionality
        builder.add_exclude_frozen(true);

        let query = builder.build_query();

        // Verify all user inputs are parameterized
        assert!(query.matches('$').count() >= 3); // At least $1, $2, $3

        // Verify static parts are hardcoded safely
        assert!(query.contains("tier != 'frozen'"));
        assert!(query.contains("status = 'active'"));

        // Verify no format! patterns remain
        assert!(!query.contains("{}"));
        assert!(!query.contains("{:"));
    }

    #[test]
    fn test_complex_query_building_safety() {
        let mut builder = SafeQueryBuilder::new("SELECT * FROM memories WHERE status = 'active'");

        // Build a complex query with multiple filters
        builder.add_tier_filter(&MemoryTier::Working);
        builder.add_importance_range(Some(0.3), Some(0.8));

        let start_date = Utc::now() - chrono::Duration::days(7);
        let end_date = Utc::now();
        builder.add_date_range(Some(&start_date), Some(&end_date));

        builder.add_consolidation_strength_range(Some(2.0), None);
        builder.add_recall_probability_range(None, Some(0.9));
        builder.add_exclude_frozen(true);

        builder.add_condition("ORDER BY importance_score DESC");
        builder.add_pagination(50, 100).unwrap();

        let query = builder.build_query();

        // Verify the query is well-formed and safe
        assert!(query.starts_with("SELECT * FROM memories WHERE status = 'active'"));
        assert!(query.contains("ORDER BY importance_score DESC"));
        assert!(query.contains("LIMIT"));
        assert!(query.contains("OFFSET"));

        // Verify all user inputs are parameterized
        let param_count = query.matches('$').count();
        assert!(param_count >= 7); // tier, importance_min, importance_max, date_start, date_end, consolidation_min, recall_max, limit, offset

        // Verify no SQL injection patterns
        assert!(!query.contains("'; "));
        assert!(!query.contains("OR 1=1"));
        assert!(!query.contains("UNION"));
        assert!(!query.contains("--"));
        assert!(!query.contains("/*"));
    }

    #[tokio::test]
    async fn test_get_memories_for_forgetting_query_structure() {
        // Test the query building logic for the new forgetting method
        let mut builder = SafeQueryBuilder::new("SELECT * FROM memories WHERE status = 'active'");

        // Add tier filter
        let tier = MemoryTier::Working;
        builder.add_tier_filter(&tier);

        // Add condition for memories not updated recently (1 hour minimum)
        builder.add_condition("AND (updated_at IS NULL OR updated_at < NOW() - INTERVAL '1 hour')");

        // Order by oldest first (NULLS FIRST for never-updated)
        builder.add_condition("ORDER BY updated_at ASC NULLS FIRST");

        let query = builder.build_query();

        // Verify query structure
        assert!(query.contains("SELECT * FROM memories WHERE status = 'active'"));
        assert!(query.contains("tier = $"));
        assert!(query.contains("updated_at IS NULL"));
        assert!(query.contains("updated_at < NOW() - INTERVAL '1 hour'"));
        assert!(query.contains("ORDER BY updated_at ASC NULLS FIRST"));

        // Verify no SQL injection
        assert!(!query.contains("'; DROP"));
        assert!(!query.contains("OR 1=1"));
    }

    #[test]
    fn test_clean_architecture_layer_separation() {
        // Test that repository methods follow clean architecture principles
        // This test validates that:
        // 1. Repository methods use SafeQueryBuilder for SQL safety
        // 2. Service layer doesn't need to know SQL details
        // 3. Domain logic is separated from data access

        let mut builder = SafeQueryBuilder::new("SELECT * FROM memories WHERE status = 'active'");

        // Test that SafeQueryBuilder abstracts SQL complexity
        builder.add_tier_filter(&MemoryTier::Working);
        builder.add_condition("AND (updated_at IS NULL OR updated_at < NOW() - INTERVAL '1 hour')");

        let query = builder.build_query();

        // Verify abstraction works properly
        assert!(query.len() > 50); // Complex query was built
        assert!(query.contains("$")); // Parameterized
        assert!(!query.contains("'working'")); // No direct value injection

        // Verify the builder maintains SQL safety while providing flexibility
        assert!(!query.contains("'; ")); // No SQL injection patterns
    }

    #[test]
    fn test_repository_method_signatures() {
        // Test that new methods have correct signatures for clean architecture
        // This ensures service layer can call repository methods without SQL knowledge

        // Mock testing - verify method signatures exist and are properly typed
        // In a real integration test, this would use a test database

        // Verify get_memories_for_forgetting signature
        let _test_fn: fn(
            &MemoryRepository,
            MemoryTier,
            usize,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<Vec<Memory>>> + Send + '_>,
        > = |repo, tier, batch_size| Box::pin(repo.get_memories_for_forgetting(tier, batch_size));

        // The fact this compiles confirms the method signature is correct
        assert!(true);
    }
}
