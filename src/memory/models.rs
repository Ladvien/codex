use chrono::{DateTime, Utc};
use pgvector::Vector;
use serde::{Deserialize, Serialize};
use sqlx::postgres::types::PgInterval;
use sqlx::FromRow;
use std::str::FromStr;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct SerializableVector(pub Option<Vector>);

impl Serialize for SerializableVector {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match &self.0 {
            Some(v) => v.as_slice().serialize(serializer),
            None => serializer.serialize_none(),
        }
    }
}

impl<'de> Deserialize<'de> for SerializableVector {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let opt_vec: Option<Vec<f32>> = Option::deserialize(deserializer)?;
        Ok(SerializableVector(opt_vec.map(Vector::from)))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "varchar", rename_all = "lowercase")]
pub enum MemoryTier {
    Working,
    Warm,
    Cold,
    Frozen,
}

impl FromStr for MemoryTier {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "working" => Ok(MemoryTier::Working),
            "warm" => Ok(MemoryTier::Warm),
            "cold" => Ok(MemoryTier::Cold),
            "frozen" => Ok(MemoryTier::Frozen),
            _ => Err(format!("Invalid memory tier: {s}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "varchar", rename_all = "lowercase")]
pub enum MemoryStatus {
    Active,
    Migrating,
    Archived,
    Deleted,
}

#[derive(Debug, Clone, FromRow)]
pub struct Memory {
    pub id: Uuid,
    pub content: String,
    pub content_hash: String,
    pub embedding: Option<Vector>,
    pub tier: MemoryTier,
    pub status: MemoryStatus,
    pub importance_score: f64,
    pub access_count: i32,
    pub last_accessed_at: Option<DateTime<Utc>>,
    pub metadata: serde_json::Value,
    pub parent_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    // Consolidation fields for memory decay and strengthening
    pub consolidation_strength: f64,
    pub decay_rate: f64,
    pub recall_probability: Option<f64>,
    pub last_recall_interval: Option<PgInterval>,
    // Three-component scoring fields
    pub recency_score: f64,
    pub relevance_score: f64,
}

impl Serialize for Memory {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("Memory", 21)?;
        state.serialize_field("id", &self.id)?;
        state.serialize_field("content", &self.content)?;
        state.serialize_field("content_hash", &self.content_hash)?;
        state.serialize_field("embedding", &self.embedding.as_ref().map(|v| v.as_slice()))?;
        state.serialize_field("tier", &self.tier)?;
        state.serialize_field("status", &self.status)?;
        state.serialize_field("importance_score", &self.importance_score)?;
        state.serialize_field("access_count", &self.access_count)?;
        state.serialize_field("last_accessed_at", &self.last_accessed_at)?;
        state.serialize_field("metadata", &self.metadata)?;
        state.serialize_field("parent_id", &self.parent_id)?;
        state.serialize_field("created_at", &self.created_at)?;
        state.serialize_field("updated_at", &self.updated_at)?;
        state.serialize_field("expires_at", &self.expires_at)?;
        state.serialize_field("consolidation_strength", &self.consolidation_strength)?;
        state.serialize_field("decay_rate", &self.decay_rate)?;
        state.serialize_field("recall_probability", &self.recall_probability)?;
        state.serialize_field(
            "last_recall_interval",
            &self.last_recall_interval.as_ref().map(|i| i.microseconds),
        )?;
        state.serialize_field("recency_score", &self.recency_score)?;
        state.serialize_field("relevance_score", &self.relevance_score)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for Memory {
    fn deserialize<D>(_deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // For now, we'll just return a default memory since we don't need to deserialize from JSON
        Ok(Memory::default())
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct MemorySummary {
    pub id: Uuid,
    pub summary_level: String,
    pub summary_content: String,
    pub summary_embedding: Option<Vector>,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub memory_count: i32,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Serialize for MemorySummary {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("MemorySummary", 10)?;
        state.serialize_field("id", &self.id)?;
        state.serialize_field("summary_level", &self.summary_level)?;
        state.serialize_field("summary_content", &self.summary_content)?;
        state.serialize_field(
            "summary_embedding",
            &self.summary_embedding.as_ref().map(|v| v.as_slice()),
        )?;
        state.serialize_field("start_time", &self.start_time)?;
        state.serialize_field("end_time", &self.end_time)?;
        state.serialize_field("memory_count", &self.memory_count)?;
        state.serialize_field("metadata", &self.metadata)?;
        state.serialize_field("created_at", &self.created_at)?;
        state.serialize_field("updated_at", &self.updated_at)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for MemorySummary {
    fn deserialize<D>(_deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        unimplemented!("MemorySummary deserialization not needed")
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct MemoryCluster {
    pub id: Uuid,
    pub cluster_name: String,
    pub centroid_embedding: Vector,
    pub concept_tags: Vec<String>,
    pub member_count: i32,
    pub tier: MemoryTier,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Serialize for MemoryCluster {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("MemoryCluster", 9)?;
        state.serialize_field("id", &self.id)?;
        state.serialize_field("cluster_name", &self.cluster_name)?;
        state.serialize_field("centroid_embedding", &self.centroid_embedding.as_slice())?;
        state.serialize_field("concept_tags", &self.concept_tags)?;
        state.serialize_field("member_count", &self.member_count)?;
        state.serialize_field("tier", &self.tier)?;
        state.serialize_field("metadata", &self.metadata)?;
        state.serialize_field("created_at", &self.created_at)?;
        state.serialize_field("updated_at", &self.updated_at)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for MemoryCluster {
    fn deserialize<D>(_deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        unimplemented!("MemoryCluster deserialization not needed")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MigrationHistoryEntry {
    pub id: Uuid,
    pub memory_id: Uuid,
    pub from_tier: MemoryTier,
    pub to_tier: MemoryTier,
    pub migration_reason: Option<String>,
    pub migrated_at: DateTime<Utc>,
    pub migration_duration_ms: Option<i32>,
    pub success: bool,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMemoryRequest {
    pub content: String,
    pub embedding: Option<Vec<f32>>,
    pub tier: Option<MemoryTier>,
    pub importance_score: Option<f64>,
    pub metadata: Option<serde_json::Value>,
    pub parent_id: Option<Uuid>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateMemoryRequest {
    pub content: Option<String>,
    pub embedding: Option<Vec<f32>>,
    pub tier: Option<MemoryTier>,
    pub importance_score: Option<f64>,
    pub metadata: Option<serde_json::Value>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SearchRequest {
    // Query options
    pub query_text: Option<String>,
    pub query_embedding: Option<Vec<f32>>,

    // Search type configuration
    pub search_type: Option<SearchType>,
    pub hybrid_weights: Option<HybridWeights>,

    // Filtering options
    pub tier: Option<MemoryTier>,
    pub date_range: Option<DateRange>,
    pub importance_range: Option<RangeFilter<f32>>,
    pub metadata_filters: Option<serde_json::Value>,
    pub tags: Option<Vec<String>>,

    // Result configuration
    pub limit: Option<i32>,
    pub offset: Option<i64>,    // For traditional pagination
    pub cursor: Option<String>, // For cursor-based pagination
    pub similarity_threshold: Option<f32>,
    pub include_metadata: Option<bool>,
    pub include_facets: Option<bool>,

    // Ranking configuration
    pub ranking_boost: Option<RankingBoost>,
    pub explain_score: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SearchType {
    Semantic,
    Temporal,
    Hybrid,
    FullText,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridWeights {
    pub semantic_weight: f32,
    pub temporal_weight: f32,
    pub importance_weight: f32,
    pub access_frequency_weight: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DateRange {
    pub start: Option<DateTime<Utc>>,
    pub end: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RangeFilter<T> {
    pub min: Option<T>,
    pub max: Option<T>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankingBoost {
    pub recency_boost: Option<f32>,
    pub importance_boost: Option<f32>,
    pub access_frequency_boost: Option<f32>,
    pub tier_boost: Option<std::collections::HashMap<MemoryTier, f32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub memory: Memory,
    pub similarity_score: f32,
    pub temporal_score: Option<f32>,
    pub importance_score: f64,
    pub access_frequency_score: Option<f32>,
    pub combined_score: f32,
    pub score_explanation: Option<ScoreExplanation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreExplanation {
    pub semantic_contribution: f32,
    pub temporal_contribution: f32,
    pub importance_contribution: f32,
    pub access_frequency_contribution: f32,
    pub total_score: f32,
    pub factors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse {
    pub results: Vec<SearchResult>,
    pub total_count: Option<i64>,
    pub facets: Option<SearchFacets>,
    pub suggestions: Option<Vec<String>>,
    pub next_cursor: Option<String>,
    pub execution_time_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchFacets {
    pub tiers: std::collections::HashMap<MemoryTier, i64>,
    pub date_histogram: Vec<DateBucket>,
    pub importance_ranges: Vec<ImportanceRange>,
    pub tags: std::collections::HashMap<String, i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DateBucket {
    pub date: DateTime<Utc>,
    pub count: i64,
    pub interval: String, // "day", "week", "month"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportanceRange {
    pub min: f32,
    pub max: f32,
    pub count: i64,
    pub label: String,
}

impl Default for Memory {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            content: String::new(),
            content_hash: String::new(),
            embedding: None,
            tier: MemoryTier::Working,
            status: MemoryStatus::Active,
            importance_score: 0.5,
            access_count: 0,
            last_accessed_at: None,
            metadata: serde_json::json!({}),
            parent_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            expires_at: None,
            consolidation_strength: 1.0,
            decay_rate: 1.0,
            recall_probability: None,
            last_recall_interval: None,
            recency_score: 0.0,
            relevance_score: 0.0,
        }
    }
}

impl Memory {
    pub fn calculate_content_hash(content: &str) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        hex::encode(hasher.finalize())
    }

    /// Get recall count (alias for access_count)
    pub fn recall_count(&self) -> i32 {
        self.access_count
    }

    pub fn should_migrate(&self) -> bool {
        use crate::memory::simple_consolidation::{
            SimpleConsolidationConfig, SimpleConsolidationEngine,
        };

        // Frozen tier never migrates
        if matches!(self.tier, MemoryTier::Frozen) {
            return false;
        }

        let config = SimpleConsolidationConfig::default();
        let engine = SimpleConsolidationEngine::new(config);

        // Use the simple consolidation engine to calculate recall probability
        match engine.calculate_recall_probability(self, None) {
            Ok(recall_prob) => recall_prob < 0.86, // Story 2 threshold
            Err(_) => {
                // Fallback to basic heuristics if calculation fails
                match self.tier {
                    MemoryTier::Working => {
                        self.importance_score < 0.3
                            || (self.last_accessed_at.is_some()
                                && Utc::now()
                                    .signed_duration_since(self.last_accessed_at.unwrap())
                                    .num_hours()
                                    > 24)
                    }
                    MemoryTier::Warm => {
                        self.importance_score < 0.1
                            && Utc::now().signed_duration_since(self.updated_at).num_days() > 7
                    }
                    MemoryTier::Cold => {
                        Utc::now().signed_duration_since(self.updated_at).num_days() > 30
                    }
                    MemoryTier::Frozen => false,
                }
            }
        }
    }

    pub fn next_tier(&self) -> Option<MemoryTier> {
        match self.tier {
            MemoryTier::Working => Some(MemoryTier::Warm),
            MemoryTier::Warm => Some(MemoryTier::Cold),
            MemoryTier::Cold => Some(MemoryTier::Frozen),
            MemoryTier::Frozen => None,
        }
    }

    /// Calculate recall probability using the new math engine
    /// This method now uses the optimized and more accurate math engine
    /// with proper edge case handling and performance optimization.
    pub fn calculate_recall_probability(&self) -> Option<f64> {
        use crate::memory::math_engine::{MathEngine, MemoryParameters};

        let engine = MathEngine::new();
        let params = MemoryParameters {
            consolidation_strength: self.consolidation_strength,
            decay_rate: self.decay_rate,
            last_accessed_at: self.last_accessed_at,
            created_at: self.created_at,
            access_count: self.access_count,
            importance_score: self.importance_score,
        };

        match engine.calculate_recall_probability(&params) {
            Ok(result) => Some(result.recall_probability),
            Err(e) => {
                tracing::warn!(
                    "Recall probability calculation failed for memory {}: {}. Using fallback.",
                    self.id,
                    e
                );
                // Use mathematically principled fallback based on importance and consolidation
                let fallback = (self.importance_score * self.consolidation_strength / 10.0)
                    .min(1.0)
                    .max(0.0);
                Some(fallback)
            }
        }
    }

    /// Update consolidation strength using the new math engine
    /// This method now uses the optimized and more accurate math engine
    /// with proper error handling and performance optimization.
    pub fn update_consolidation_strength(&mut self, recall_interval: PgInterval) {
        use crate::memory::math_engine::MathEngine;

        let engine = MathEngine::new();

        match engine.update_consolidation_strength(self.consolidation_strength, recall_interval) {
            Ok(result) => {
                self.consolidation_strength = result.new_consolidation_strength;
            }
            Err(_) => {
                // Fallback to simple increment if calculation fails
                let time_hours = recall_interval.microseconds as f64 / 3_600_000_000.0;
                let increment = time_hours.min(1.0) * 0.1; // Conservative increment
                self.consolidation_strength = (self.consolidation_strength + increment).min(10.0);
            }
        }
    }
}

// New model structs for consolidation features

#[derive(Debug, Clone, FromRow)]
pub struct MemoryConsolidationLog {
    pub id: Uuid,
    pub memory_id: Uuid,
    pub event_type: String,
    pub previous_consolidation_strength: f64,
    pub new_consolidation_strength: f64,
    pub previous_recall_probability: Option<f64>,
    pub new_recall_probability: Option<f64>,
    pub recall_interval: Option<PgInterval>,
    pub access_context: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

impl Serialize for MemoryConsolidationLog {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("MemoryConsolidationLog", 10)?;
        state.serialize_field("id", &self.id)?;
        state.serialize_field("memory_id", &self.memory_id)?;
        state.serialize_field("event_type", &self.event_type)?;
        state.serialize_field(
            "previous_consolidation_strength",
            &self.previous_consolidation_strength,
        )?;
        state.serialize_field(
            "new_consolidation_strength",
            &self.new_consolidation_strength,
        )?;
        state.serialize_field(
            "previous_recall_probability",
            &self.previous_recall_probability,
        )?;
        state.serialize_field("new_recall_probability", &self.new_recall_probability)?;
        state.serialize_field(
            "recall_interval_microseconds",
            &self.recall_interval.as_ref().map(|i| i.microseconds),
        )?;
        state.serialize_field("access_context", &self.access_context)?;
        state.serialize_field("created_at", &self.created_at)?;
        state.end()
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct FrozenMemory {
    pub id: Uuid,
    pub original_memory_id: Uuid,
    pub compressed_content: serde_json::Value, // Matches JSONB in database
    pub original_metadata: Option<serde_json::Value>, // Matches database
    pub freeze_reason: Option<String>,
    pub frozen_at: DateTime<Utc>,
    pub unfreeze_count: Option<i32>,             // Matches database
    pub last_unfrozen_at: Option<DateTime<Utc>>, // Matches database
    pub compression_ratio: Option<f64>,
}

impl Serialize for FrozenMemory {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("FrozenMemory", 9)?;
        state.serialize_field("id", &self.id)?;
        state.serialize_field("original_memory_id", &self.original_memory_id)?;
        state.serialize_field("compressed_content", &self.compressed_content)?;
        state.serialize_field("original_metadata", &self.original_metadata)?;
        state.serialize_field("freeze_reason", &self.freeze_reason)?;
        state.serialize_field("frozen_at", &self.frozen_at)?;
        state.serialize_field("unfreeze_count", &self.unfreeze_count)?;
        state.serialize_field("last_unfrozen_at", &self.last_unfrozen_at)?;
        state.serialize_field("compression_ratio", &self.compression_ratio)?;
        state.end()
    }
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct MemoryTierStatistics {
    pub id: Uuid,
    pub tier: MemoryTier,
    pub total_memories: i64,
    pub average_consolidation_strength: Option<f64>,
    pub average_recall_probability: Option<f64>,
    pub average_age_days: Option<f64>,
    pub total_storage_bytes: i64,
    pub snapshot_timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ConsolidationAnalytics {
    pub tier: MemoryTier,
    pub total_memories: i64,
    pub avg_consolidation_strength: Option<f64>,
    pub avg_recall_probability: Option<f64>,
    pub avg_decay_rate: Option<f64>,
    pub avg_age_days: Option<f64>,
    pub migration_candidates: i64,
    pub never_accessed: i64,
    pub accessed_recently: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ConsolidationEventSummary {
    pub event_type: String,
    pub event_count: i64,
    pub avg_strength_change: Option<f64>,
    pub avg_probability_change: Option<f64>,
    pub avg_recall_interval_hours: Option<f64>,
}

// Request/Response structures for freezing operations

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreezeMemoryRequest {
    pub memory_id: Uuid,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreezeMemoryResponse {
    pub frozen_id: Uuid,
    pub compression_ratio: Option<f64>,
    pub original_tier: MemoryTier,
    pub frozen_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnfreezeMemoryRequest {
    pub frozen_id: Uuid,
    pub target_tier: Option<MemoryTier>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnfreezeMemoryResponse {
    pub memory_id: Uuid,
    pub retrieval_delay_seconds: i32,
    pub restoration_tier: MemoryTier,
    pub unfrozen_at: DateTime<Utc>,
}

// Consolidation-specific search requests

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationSearchRequest {
    pub min_consolidation_strength: Option<f64>,
    pub max_consolidation_strength: Option<f64>,
    pub min_recall_probability: Option<f64>,
    pub max_recall_probability: Option<f64>,
    pub include_frozen: Option<bool>,
    pub tier: Option<MemoryTier>,
    pub limit: Option<i32>,
    pub offset: Option<i64>,
}
