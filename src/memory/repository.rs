use super::error::{MemoryError, Result};
use super::models::*;
use chrono::Utc;
use pgvector::Vector;
use sqlx::{PgPool, Postgres, Row, Transaction};
use std::collections::HashMap;
use std::time::Instant;
use tracing::{debug, info};
use uuid::Uuid;

pub struct MemoryRepository {
    pool: PgPool,
}

impl MemoryRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn create_memory(&self, request: CreateMemoryRequest) -> Result<Memory> {
        let id = Uuid::new_v4();
        let content_hash = Memory::calculate_content_hash(&request.content);
        let tier = request.tier.unwrap_or(MemoryTier::Working);

        // Check for duplicates
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

        let embedding = request.embedding.map(Vector::from);

        let memory = sqlx::query_as::<_, Memory>(
            r#"
            INSERT INTO memories (
                id, content, content_hash, embedding, tier, status, 
                importance_score, metadata, parent_id, expires_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(&request.content)
        .bind(&content_hash)
        .bind(embedding)
        .bind(tier)
        .bind(MemoryStatus::Active)
        .bind(request.importance_score.unwrap_or(0.5))
        .bind(request.metadata.unwrap_or(serde_json::json!({})))
        .bind(request.parent_id)
        .bind(request.expires_at)
        .fetch_one(&self.pool)
        .await?;

        info!("Created memory {} in tier {:?}", memory.id, memory.tier);
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

        let mut query_parts = vec![
            "SELECT m.*, 1 - (m.embedding <=> $1) as similarity_score".to_string(),
            "FROM memories m".to_string(),
            "WHERE m.status = 'active' AND m.embedding IS NOT NULL".to_string(),
        ];

        // Add filters
        self.add_filters(request, &mut query_parts)?;

        query_parts.push(format!("AND 1 - (m.embedding <=> $1) >= {threshold}"));
        query_parts.push("ORDER BY similarity_score DESC".to_string());
        query_parts.push(format!("LIMIT {limit} OFFSET {offset}"));

        let query = query_parts.join(" ");
        let rows = sqlx::query(&query)
            .bind(&query_embedding)
            .fetch_all(&self.pool)
            .await?;

        self.build_search_results(rows, request).await
    }

    async fn temporal_search(&self, request: &SearchRequest) -> Result<Vec<SearchResult>> {
        let limit = request.limit.unwrap_or(10);
        let offset = request.offset.unwrap_or(0);

        let mut query_parts = vec![
            "SELECT m.*, 0.0 as similarity_score".to_string(),
            "FROM memories m".to_string(),
            "WHERE m.status = 'active'".to_string(),
        ];

        // Add filters
        self.add_filters(request, &mut query_parts)?;

        query_parts.push("ORDER BY m.created_at DESC, m.updated_at DESC".to_string());
        query_parts.push(format!("LIMIT {limit} OFFSET {offset}"));

        let query = query_parts.join(" ");
        let rows = sqlx::query(&query).fetch_all(&self.pool).await?;

        self.build_search_results(rows, request).await
    }

    async fn hybrid_search(&self, request: &SearchRequest) -> Result<Vec<SearchResult>> {
        let weights = request.hybrid_weights.as_ref().unwrap_or(&HybridWeights {
            semantic_weight: 0.4,
            temporal_weight: 0.3,
            importance_weight: 0.2,
            access_frequency_weight: 0.1,
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

        let query = format!(
            r#"
            SELECT m.*,
                1 - (m.embedding <=> $1) as similarity_score,
                EXTRACT(EPOCH FROM (NOW() - m.created_at))::float / 86400 as days_old,
                m.importance_score,
                COALESCE(m.access_count, 0) as access_count,
                (
                    {} * (1 - (m.embedding <=> $1)) +
                    {} * GREATEST(0, 1 - (EXTRACT(EPOCH FROM (NOW() - m.created_at))::float / 2592000)) + -- 30 days
                    {} * m.importance_score +
                    {} * LEAST(1.0, COALESCE(m.access_count, 0)::float / 100.0)
                ) as combined_score
            FROM memories m
            WHERE m.status = 'active'
                AND m.embedding IS NOT NULL
                AND 1 - (m.embedding <=> $1) >= {}
            ORDER BY combined_score DESC
            LIMIT {} OFFSET {}
            "#,
            weights.semantic_weight,
            weights.temporal_weight,
            weights.importance_weight,
            weights.access_frequency_weight,
            threshold,
            limit,
            offset
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

    fn add_filters(&self, request: &SearchRequest, query_parts: &mut Vec<String>) -> Result<()> {
        if let Some(tier) = &request.tier {
            query_parts.push(format!("AND m.tier = '{tier:?}'"));
        }

        if let Some(date_range) = &request.date_range {
            if let Some(start) = &date_range.start {
                query_parts.push(format!(
                    "AND m.created_at >= '{}'",
                    start.format("%Y-%m-%d %H:%M:%S")
                ));
            }
            if let Some(end) = &date_range.end {
                query_parts.push(format!(
                    "AND m.created_at <= '{}'",
                    end.format("%Y-%m-%d %H:%M:%S")
                ));
            }
        }

        if let Some(importance_range) = &request.importance_range {
            if let Some(min) = importance_range.min {
                query_parts.push(format!("AND m.importance_score >= {min}"));
            }
            if let Some(max) = importance_range.max {
                query_parts.push(format!("AND m.importance_score <= {max}"));
            }
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
        };

        let memories = sqlx::query_as::<_, Memory>(query)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;

        Ok(memories)
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
                AVG(access_count) FILTER (WHERE status = 'active') as avg_access_count
            FROM memories
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(stats)
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

        // Working tier with low importance should migrate
        memory.tier = MemoryTier::Working;
        memory.importance_score = 0.2;
        assert!(memory.should_migrate());

        // Working tier with high importance should not migrate
        memory.importance_score = 0.8;
        assert!(!memory.should_migrate());

        // Cold tier should never migrate
        memory.tier = MemoryTier::Cold;
        memory.importance_score = 0.0;
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
        assert_eq!(memory.next_tier(), None);
    }
}
