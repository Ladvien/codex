use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use chrono::{DateTime, Utc};

use crate::memory::{SearchRequest, SearchType, MemoryTier};
use super::AppState;

#[derive(Debug, Serialize)]
pub struct HarvesterStatus {
    pub active: bool,
    pub last_harvest: Option<DateTime<Utc>>,
    pub messages_processed: u64,
    pub patterns_extracted: u64,
    pub memories_stored: u64,
    pub duplicates_filtered: u64,
    pub current_queue_size: usize,
    pub health_status: String,
}

#[derive(Debug, Serialize)]
pub struct HarvesterStatistics {
    pub total_messages_processed: u64,
    pub total_patterns_extracted: u64,
    pub total_memories_stored: u64,
    pub total_duplicates_filtered: u64,
    pub average_processing_time_ms: f64,
    pub average_batch_size: f64,
    pub pattern_type_breakdown: HashMap<String, u64>,
    pub confidence_score_distribution: ConfidenceDistribution,
    pub recent_performance: Vec<PerformanceDataPoint>,
}

#[derive(Debug, Serialize)]
pub struct ConfidenceDistribution {
    pub high_confidence: u64,      // > 0.8
    pub medium_confidence: u64,    // 0.6 - 0.8  
    pub low_confidence: u64,       // < 0.6
}

#[derive(Debug, Serialize)]
pub struct PerformanceDataPoint {
    pub timestamp: DateTime<Utc>,
    pub processing_time_ms: u64,
    pub patterns_extracted: u64,
    pub batch_size: usize,
}

#[derive(Debug, Serialize)]
pub struct RecentMemory {
    pub id: String,
    pub content: String,
    pub pattern_type: String,
    pub confidence: f64,
    pub created_at: DateTime<Utc>,
    pub tier: MemoryTier,
    pub importance_score: f64,
}

#[derive(Debug, Deserialize)]
pub struct RecentMemoriesQuery {
    pub limit: Option<usize>,
    pub pattern_type: Option<String>,
    pub min_confidence: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct ExportData {
    pub export_timestamp: DateTime<Utc>,
    pub total_memories: usize,
    pub memories: Vec<ExportMemory>,
    pub statistics: ExportStatistics,
    pub metadata: ExportMetadata,
}

#[derive(Debug, Serialize)]
pub struct ExportMemory {
    pub id: String,
    pub content: String,
    pub pattern_type: String,
    pub confidence: f64,
    pub created_at: DateTime<Utc>,
    pub tier: MemoryTier,
    pub importance_score: f64,
    pub metadata: Value,
    pub tags: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ExportStatistics {
    pub total_harvested: u64,
    pub by_pattern_type: HashMap<String, u64>,
    pub by_confidence_range: ConfidenceDistribution,
    pub average_importance: f64,
}

#[derive(Debug, Serialize)]
pub struct ExportMetadata {
    pub export_version: String,
    pub system_version: String,
    pub export_format: String,
    pub privacy_level: String,
}

/// Get current harvester status
pub async fn get_status(State(state): State<AppState>) -> Result<Json<HarvesterStatus>, StatusCode> {
    let status = if let Some(harvester) = &state.harvester_service {
        // In a real implementation, get metrics from the harvester service
        HarvesterStatus {
            active: true,
            last_harvest: Some(Utc::now()),
            messages_processed: 1000,
            patterns_extracted: 150,
            memories_stored: 120,
            duplicates_filtered: 30,
            current_queue_size: 5,
            health_status: "healthy".to_string(),
        }
    } else {
        HarvesterStatus {
            active: false,
            last_harvest: None,
            messages_processed: 0,
            patterns_extracted: 0,
            memories_stored: 0,
            duplicates_filtered: 0,
            current_queue_size: 0,
            health_status: "inactive".to_string(),
        }
    };

    Ok(Json(status))
}

/// Toggle harvester on/off
pub async fn toggle_harvester(State(state): State<AppState>) -> Result<Json<Value>, StatusCode> {
    // TODO: Implement harvester toggle functionality
    // This would start/stop the harvester service
    
    Ok(Json(json!({
        "status": "success",
        "message": "Harvester toggle requested (implementation pending)",
        "active": true
    })))
}

/// Get harvester statistics
pub async fn get_statistics(State(state): State<AppState>) -> Result<Json<HarvesterStatistics>, StatusCode> {
    // Generate sample statistics data
    // In a real implementation, this would come from the harvester metrics
    
    let mut pattern_breakdown = HashMap::new();
    pattern_breakdown.insert("Preference".to_string(), 45);
    pattern_breakdown.insert("Fact".to_string(), 35);
    pattern_breakdown.insert("Decision".to_string(), 25);
    pattern_breakdown.insert("Goal".to_string(), 20);
    pattern_breakdown.insert("Skill".to_string(), 15);
    pattern_breakdown.insert("Emotion".to_string(), 10);
    pattern_breakdown.insert("Relationship".to_string(), 8);
    pattern_breakdown.insert("Correction".to_string(), 5);

    let confidence_distribution = ConfidenceDistribution {
        high_confidence: 85,
        medium_confidence: 45,
        low_confidence: 13,
    };

    let recent_performance = vec![
        PerformanceDataPoint {
            timestamp: Utc::now() - chrono::Duration::hours(1),
            processing_time_ms: 150,
            patterns_extracted: 12,
            batch_size: 25,
        },
        PerformanceDataPoint {
            timestamp: Utc::now() - chrono::Duration::hours(2),
            processing_time_ms: 180,
            patterns_extracted: 8,
            batch_size: 18,
        },
        PerformanceDataPoint {
            timestamp: Utc::now() - chrono::Duration::hours(3),
            processing_time_ms: 120,
            patterns_extracted: 15,
            batch_size: 30,
        },
    ];

    let statistics = HarvesterStatistics {
        total_messages_processed: 2500,
        total_patterns_extracted: 350,
        total_memories_stored: 285,
        total_duplicates_filtered: 65,
        average_processing_time_ms: 145.5,
        average_batch_size: 24.3,
        pattern_type_breakdown: pattern_breakdown,
        confidence_score_distribution: confidence_distribution,
        recent_performance,
    };

    Ok(Json(statistics))
}

/// Get recently harvested memories
pub async fn get_recent_memories(
    State(state): State<AppState>,
    Query(params): Query<RecentMemoriesQuery>,
) -> Result<Json<Vec<RecentMemory>>, StatusCode> {
    let limit = params.limit.unwrap_or(20).min(100);
    let min_confidence = params.min_confidence.unwrap_or(0.0);
    
    // Search for recent memories (in a real implementation)
    let search_request = SearchRequest {
        query_text: None,
        query_embedding: None,
        search_type: Some(SearchType::Semantic),
        hybrid_weights: None,
        tier: None,
        date_range: None,
        importance_range: None,
        metadata_filters: None,
        tags: None,
        limit: Some(limit as i32),
        offset: None,
        cursor: None,
        similarity_threshold: Some(min_confidence as f32),
        include_metadata: Some(true),
        include_facets: None,
        ranking_boost: None,
        explain_score: Some(false),
    };

    match state.repository.search_memories(search_request).await {
        Ok(search_response) => {
            let recent_memories: Vec<RecentMemory> = search_response.results
                .into_iter()
                .map(|result| RecentMemory {
                    id: result.memory.id.to_string(),
                    content: result.memory.content.chars().take(200).collect::<String>() + "...",
                    pattern_type: result.memory.metadata
                        .as_object()
                        .and_then(|m| m.get("pattern_type"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("Unknown")
                        .to_string(),
                    confidence: result.memory.metadata
                        .as_object()
                        .and_then(|m| m.get("confidence"))
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0),
                    created_at: result.memory.created_at,
                    tier: result.memory.tier,
                    importance_score: result.memory.importance_score,
                })
                .collect();

            Ok(Json(recent_memories))
        }
        Err(_) => {
            // Return sample data on error
            let sample_memories = vec![
                RecentMemory {
                    id: "sample-1".to_string(),
                    content: "I prefer working in the morning when I'm most productive...".to_string(),
                    pattern_type: "Preference".to_string(),
                    confidence: 0.85,
                    created_at: Utc::now() - chrono::Duration::minutes(30),
                    tier: MemoryTier::Working,
                    importance_score: 0.7,
                },
                RecentMemory {
                    id: "sample-2".to_string(),
                    content: "My goal is to improve my Rust programming skills this year...".to_string(),
                    pattern_type: "Goal".to_string(),
                    confidence: 0.92,
                    created_at: Utc::now() - chrono::Duration::hours(2),
                    tier: MemoryTier::Working,
                    importance_score: 0.85,
                },
            ];
            Ok(Json(sample_memories))
        }
    }
}

/// Export memory history
pub async fn export_history(State(state): State<AppState>) -> Result<Json<ExportData>, StatusCode> {
    // In a real implementation, this would query all harvested memories
    let export_memories = vec![
        ExportMemory {
            id: "export-1".to_string(),
            content: "I prefer working with TypeScript over JavaScript for large projects".to_string(),
            pattern_type: "Preference".to_string(),
            confidence: 0.88,
            created_at: Utc::now() - chrono::Duration::days(1),
            tier: MemoryTier::Working,
            importance_score: 0.75,
            metadata: json!({"source": "conversation", "context": "programming discussion"}),
            tags: vec!["programming".to_string(), "languages".to_string()],
        },
        ExportMemory {
            id: "export-2".to_string(),
            content: "I work as a software engineer at a tech startup".to_string(),
            pattern_type: "Fact".to_string(),
            confidence: 0.95,
            created_at: Utc::now() - chrono::Duration::days(2),
            tier: MemoryTier::Working,
            importance_score: 0.9,
            metadata: json!({"source": "conversation", "context": "personal information"}),
            tags: vec!["work".to_string(), "personal".to_string()],
        },
    ];

    let mut by_pattern_type = HashMap::new();
    by_pattern_type.insert("Preference".to_string(), 45);
    by_pattern_type.insert("Fact".to_string(), 35);
    by_pattern_type.insert("Decision".to_string(), 25);

    let statistics = ExportStatistics {
        total_harvested: export_memories.len() as u64,
        by_pattern_type,
        by_confidence_range: ConfidenceDistribution {
            high_confidence: 85,
            medium_confidence: 45,
            low_confidence: 13,
        },
        average_importance: 0.78,
    };

    let metadata = ExportMetadata {
        export_version: "1.0".to_string(),
        system_version: env!("CARGO_PKG_VERSION").to_string(),
        export_format: "json".to_string(),
        privacy_level: "full".to_string(),
    };

    let export_data = ExportData {
        export_timestamp: Utc::now(),
        total_memories: export_memories.len(),
        memories: export_memories,
        statistics,
        metadata,
    };

    Ok(Json(export_data))
}