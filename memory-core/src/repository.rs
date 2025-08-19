use crate::error::{MemoryError, Result};
use crate::models::*;
use chrono::{DateTime, Utc};
use pgvector::Vector;
use sqlx::{PgPool, Postgres, Transaction, Row};
use tracing::{debug, info};
use uuid::Uuid;

pub struct MemoryRepository {
    pool: PgPool,
}

impl MemoryRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
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
        .bind(&tier)
        .fetch_one(&self.pool)
        .await?;

        if duplicate_exists {
            return Err(MemoryError::DuplicateContent { 
                tier: format!("{:?}", tier) 
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
        .ok_or_else(|| MemoryError::NotFound { 
            id: id.to_string() 
        })?;

        debug!("Retrieved memory {} from tier {:?}", id, memory.tier);
        Ok(memory)
    }

    pub async fn update_memory(&self, id: Uuid, request: UpdateMemoryRequest) -> Result<Memory> {
        let mut tx = self.pool.begin().await?;

        // Get current memory
        let current = sqlx::query_as::<_, Memory>(
            "SELECT * FROM memories WHERE id = $1 AND status = 'active' FOR UPDATE"
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| MemoryError::NotFound { 
            id: id.to_string() 
        })?;

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
            self.record_migration(&mut tx, id, current.tier, tier, Some("Manual update".to_string()))
                .await?;
        }

        tx.commit().await?;
        info!("Updated memory {}", id);
        Ok(updated)
    }

    pub async fn delete_memory(&self, id: Uuid) -> Result<()> {
        let result = sqlx::query(
            "UPDATE memories SET status = 'deleted' WHERE id = $1 AND status = 'active'"
        )
        .bind(id)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(MemoryError::NotFound { 
                id: id.to_string() 
            });
        }

        info!("Soft deleted memory {}", id);
        Ok(())
    }

    pub async fn search_memories(&self, request: SearchRequest) -> Result<Vec<SearchResult>> {
        let query_embedding = Vector::from(request.query_embedding);
        let limit = request.limit.unwrap_or(10);
        let threshold = request.similarity_threshold.unwrap_or(0.7);

        let mut query = String::from(
            r#"
            SELECT 
                m.*,
                1 - (m.embedding <=> $1) as similarity_score
            FROM memories m
            WHERE m.status = 'active'
                AND m.embedding IS NOT NULL
            "#
        );

        if let Some(tier) = request.tier {
            query.push_str(&format!(" AND m.tier = '{:?}'", tier));
        }

        query.push_str(&format!(
            r#"
                AND 1 - (m.embedding <=> $1) >= {}
            ORDER BY similarity_score DESC
            LIMIT {}
            "#,
            threshold, limit
        ));

        struct MemoryWithScore {
            memory: Memory,
            similarity_score: f32,
        }

        let mut results = Vec::new();
        let rows = sqlx::query(&query)
            .bind(query_embedding)
            .fetch_all(&self.pool)
            .await?;

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
            let similarity_score: f32 = row.try_get("similarity_score")?;
            
            results.push(SearchResult {
                memory,
                similarity_score,
            });
        }

        debug!("Found {} memories matching search criteria", results.len());
        Ok(results)
    }

    pub async fn get_memories_by_tier(&self, tier: MemoryTier, limit: Option<i64>) -> Result<Vec<Memory>> {
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

    pub async fn migrate_memory(&self, id: Uuid, to_tier: MemoryTier, reason: Option<String>) -> Result<Memory> {
        let mut tx = self.pool.begin().await?;

        // Get current memory with lock
        let current = sqlx::query_as::<_, Memory>(
            "SELECT * FROM memories WHERE id = $1 AND status = 'active' FOR UPDATE"
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| MemoryError::NotFound { 
            id: id.to_string() 
        })?;

        if current.tier == to_tier {
            return Ok(current);
        }

        // Validate tier transition
        let valid_transition = match (current.tier, to_tier) {
            (MemoryTier::Working, MemoryTier::Warm) |
            (MemoryTier::Working, MemoryTier::Cold) |
            (MemoryTier::Warm, MemoryTier::Cold) |
            (MemoryTier::Warm, MemoryTier::Working) |
            (MemoryTier::Cold, MemoryTier::Warm) => true,
            _ => false,
        };

        if !valid_transition {
            return Err(MemoryError::InvalidTierTransition {
                from: format!("{:?}", current.tier),
                to: format!("{:?}", to_tier),
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

    pub async fn get_migration_candidates(&self, tier: MemoryTier, limit: i64) -> Result<Vec<Memory>> {
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