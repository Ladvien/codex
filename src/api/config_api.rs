use axum::{extract::State, http::StatusCode, response::Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::AppState;
use crate::memory::{MemoryPatternType, SilentHarvesterConfig};

#[derive(Debug, Serialize, Deserialize)]
pub struct HarvesterConfigResponse {
    pub enabled: bool,
    pub confidence_threshold: f64,
    pub deduplication_threshold: f64,
    pub message_trigger_count: usize,
    pub time_trigger_minutes: u64,
    pub max_batch_size: usize,
    pub max_processing_time_seconds: u64,
    pub silent_mode: bool,
    pub privacy_mode: bool,
    pub pattern_types: Vec<PatternTypeConfig>,
    pub harvest_frequency: HarvestFrequency,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PatternTypeConfig {
    pub pattern_type: MemoryPatternType,
    pub enabled: bool,
    pub patterns: Vec<String>,
    pub description: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HarvestFrequency {
    pub message_count: usize,
    pub time_minutes: u64,
}

#[derive(Debug, Deserialize)]
pub struct UpdateConfigRequest {
    pub enabled: Option<bool>,
    pub confidence_threshold: Option<f64>,
    pub deduplication_threshold: Option<f64>,
    pub message_trigger_count: Option<usize>,
    pub time_trigger_minutes: Option<u64>,
    pub max_batch_size: Option<usize>,
    pub silent_mode: Option<bool>,
    pub privacy_mode: Option<bool>,
    pub pattern_types: Option<Vec<PatternTypeConfig>>,
}

/// Get current harvester configuration
pub async fn get_harvester_config(
    State(state): State<AppState>,
) -> Result<Json<HarvesterConfigResponse>, StatusCode> {
    let config = get_current_config(&state).await?;

    let pattern_types = vec![
        PatternTypeConfig {
            pattern_type: MemoryPatternType::Preference,
            enabled: true,
            patterns: config.pattern_config.preference_patterns.clone(),
            description: "User preferences and likes/dislikes".to_string(),
        },
        PatternTypeConfig {
            pattern_type: MemoryPatternType::Fact,
            enabled: true,
            patterns: config.pattern_config.fact_patterns.clone(),
            description: "Personal facts and biographical information".to_string(),
        },
        PatternTypeConfig {
            pattern_type: MemoryPatternType::Decision,
            enabled: true,
            patterns: config.pattern_config.decision_patterns.clone(),
            description: "Decisions and choices made by the user".to_string(),
        },
        PatternTypeConfig {
            pattern_type: MemoryPatternType::Correction,
            enabled: true,
            patterns: config.pattern_config.correction_patterns.clone(),
            description: "Corrections and clarifications".to_string(),
        },
        PatternTypeConfig {
            pattern_type: MemoryPatternType::Emotion,
            enabled: true,
            patterns: config.pattern_config.emotion_patterns.clone(),
            description: "Emotional expressions and feelings".to_string(),
        },
        PatternTypeConfig {
            pattern_type: MemoryPatternType::Goal,
            enabled: true,
            patterns: config.pattern_config.goal_patterns.clone(),
            description: "Goals and aspirations".to_string(),
        },
        PatternTypeConfig {
            pattern_type: MemoryPatternType::Relationship,
            enabled: true,
            patterns: config.pattern_config.relationship_patterns.clone(),
            description: "Relationships and social connections".to_string(),
        },
        PatternTypeConfig {
            pattern_type: MemoryPatternType::Skill,
            enabled: true,
            patterns: config.pattern_config.skill_patterns.clone(),
            description: "Skills and abilities".to_string(),
        },
    ];

    let response = HarvesterConfigResponse {
        enabled: !config.silent_mode, // In our model, silent_mode means disabled UI
        confidence_threshold: config.confidence_threshold,
        deduplication_threshold: config.deduplication_threshold,
        message_trigger_count: config.message_trigger_count,
        time_trigger_minutes: config.time_trigger_minutes,
        max_batch_size: config.max_batch_size,
        max_processing_time_seconds: config.max_processing_time_seconds,
        silent_mode: config.silent_mode,
        privacy_mode: false, // TODO: Implement privacy mode
        pattern_types,
        harvest_frequency: HarvestFrequency {
            message_count: config.message_trigger_count,
            time_minutes: config.time_trigger_minutes,
        },
    };

    Ok(Json(response))
}

/// Update harvester configuration
pub async fn update_harvester_config(
    State(state): State<AppState>,
    Json(update): Json<UpdateConfigRequest>,
) -> Result<Json<Value>, StatusCode> {
    let mut config = get_current_config(&state).await?;

    // Apply updates
    if let Some(enabled) = update.enabled {
        config.silent_mode = !enabled; // Inverse relationship
    }

    if let Some(threshold) = update.confidence_threshold {
        if (0.5..=0.9).contains(&threshold) {
            config.confidence_threshold = threshold;
        } else {
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    if let Some(threshold) = update.deduplication_threshold {
        if (0.5..=1.0).contains(&threshold) {
            config.deduplication_threshold = threshold;
        } else {
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    if let Some(count) = update.message_trigger_count {
        if count > 0 && count <= 1000 {
            config.message_trigger_count = count;
        } else {
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    if let Some(minutes) = update.time_trigger_minutes {
        if minutes > 0 && minutes <= 1440 {
            // Max 24 hours
            config.time_trigger_minutes = minutes;
        } else {
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    if let Some(batch_size) = update.max_batch_size {
        if batch_size > 0 && batch_size <= 1000 {
            config.max_batch_size = batch_size;
        } else {
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    if let Some(silent_mode) = update.silent_mode {
        config.silent_mode = silent_mode;
    }

    // Update pattern configurations
    if let Some(pattern_types) = update.pattern_types {
        for pattern_type in pattern_types {
            match pattern_type.pattern_type {
                MemoryPatternType::Preference => {
                    if pattern_type.enabled {
                        config.pattern_config.preference_patterns = pattern_type.patterns;
                    }
                }
                MemoryPatternType::Fact => {
                    if pattern_type.enabled {
                        config.pattern_config.fact_patterns = pattern_type.patterns;
                    }
                }
                MemoryPatternType::Decision => {
                    if pattern_type.enabled {
                        config.pattern_config.decision_patterns = pattern_type.patterns;
                    }
                }
                MemoryPatternType::Correction => {
                    if pattern_type.enabled {
                        config.pattern_config.correction_patterns = pattern_type.patterns;
                    }
                }
                MemoryPatternType::Emotion => {
                    if pattern_type.enabled {
                        config.pattern_config.emotion_patterns = pattern_type.patterns;
                    }
                }
                MemoryPatternType::Goal => {
                    if pattern_type.enabled {
                        config.pattern_config.goal_patterns = pattern_type.patterns;
                    }
                }
                MemoryPatternType::Relationship => {
                    if pattern_type.enabled {
                        config.pattern_config.relationship_patterns = pattern_type.patterns;
                    }
                }
                MemoryPatternType::Skill => {
                    if pattern_type.enabled {
                        config.pattern_config.skill_patterns = pattern_type.patterns;
                    }
                }
            }
        }
    }

    // TODO: Persist configuration changes
    save_config(&state, &config).await?;

    Ok(Json(json!({
        "status": "success",
        "message": "Configuration updated successfully",
        "applied_immediately": true
    })))
}

async fn get_current_config(state: &AppState) -> Result<SilentHarvesterConfig, StatusCode> {
    // In a real implementation, this would load from database or config file
    // For now, return default configuration
    Ok(SilentHarvesterConfig::default())
}

async fn save_config(state: &AppState, config: &SilentHarvesterConfig) -> Result<(), StatusCode> {
    // TODO: Implement configuration persistence
    // This would save to database or config file
    tracing::info!("Configuration update requested (persistence not yet implemented)");
    Ok(())
}
