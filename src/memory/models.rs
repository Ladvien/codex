use chrono::{DateTime, Utc};
use pgvector::Vector;
use serde::{Deserialize, Serialize};
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
#[sqlx(type_name = "memory_tier", rename_all = "lowercase")]
pub enum MemoryTier {
    Working,
    Warm,
    Cold,
}

impl FromStr for MemoryTier {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "working" => Ok(MemoryTier::Working),
            "warm" => Ok(MemoryTier::Warm),
            "cold" => Ok(MemoryTier::Cold),
            _ => Err(format!("Invalid memory tier: {s}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "memory_status", rename_all = "lowercase")]
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
    pub importance_score: f32,
    pub access_count: i32,
    pub last_accessed_at: Option<DateTime<Utc>>,
    pub metadata: serde_json::Value,
    pub parent_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

impl Serialize for Memory {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("Memory", 15)?;
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
    pub importance_score: Option<f32>,
    pub metadata: Option<serde_json::Value>,
    pub parent_id: Option<Uuid>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateMemoryRequest {
    pub content: Option<String>,
    pub embedding: Option<Vec<f32>>,
    pub tier: Option<MemoryTier>,
    pub importance_score: Option<f32>,
    pub metadata: Option<serde_json::Value>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub importance_score: f32,
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

    pub fn should_migrate(&self) -> bool {
        match self.tier {
            MemoryTier::Working => {
                // Migrate if importance is low and hasn't been accessed recently
                self.importance_score < 0.3
                    || (self.last_accessed_at.is_some()
                        && Utc::now()
                            .signed_duration_since(self.last_accessed_at.unwrap())
                            .num_hours()
                            > 24)
            }
            MemoryTier::Warm => {
                // Migrate to cold if very low importance and old
                self.importance_score < 0.1
                    && Utc::now().signed_duration_since(self.updated_at).num_days() > 7
            }
            MemoryTier::Cold => false,
        }
    }

    pub fn next_tier(&self) -> Option<MemoryTier> {
        match self.tier {
            MemoryTier::Working => Some(MemoryTier::Warm),
            MemoryTier::Warm => Some(MemoryTier::Cold),
            MemoryTier::Cold => None,
        }
    }
}
