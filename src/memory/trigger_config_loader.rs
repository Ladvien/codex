//! Configuration loader for event-triggered scoring system with hot-reloading support

use crate::memory::error::{MemoryError, Result};
use crate::memory::event_triggers::{TriggerConfig, TriggerEvent, TriggerPattern};
use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;
use tokio::time::interval;
use tracing::{error, info, warn};

/// Configuration loader with hot-reloading capabilities
pub struct TriggerConfigLoader {
    config_path: String,
    last_modified: Arc<RwLock<Option<SystemTime>>>,
    current_config: Arc<RwLock<TriggerConfig>>,
    hot_reload_enabled: bool,
}

impl TriggerConfigLoader {
    /// Create new configuration loader
    pub fn new(config_path: String) -> Self {
        Self {
            config_path,
            last_modified: Arc::new(RwLock::new(None)),
            current_config: Arc::new(RwLock::new(TriggerConfig::default())),
            hot_reload_enabled: false,
        }
    }

    /// Enable hot-reloading with specified check interval
    pub fn enable_hot_reload(&mut self, check_interval: Duration) {
        self.hot_reload_enabled = true;
        
        let config_path = self.config_path.clone();
        let last_modified = self.last_modified.clone();
        let current_config = self.current_config.clone();

        tokio::spawn(async move {
            let mut timer = interval(check_interval);
            
            loop {
                timer.tick().await;
                
                if let Err(e) = Self::check_and_reload_config(
                    &config_path,
                    &last_modified,
                    &current_config,
                ).await {
                    error!("Failed to check/reload config: {}", e);
                }
            }
        });
    }

    /// Load configuration from file
    pub async fn load_config(&self) -> Result<TriggerConfig> {
        let config = Self::load_config_from_file(&self.config_path).await?;
        
        // Update current config and last modified time
        {
            let mut current = self.current_config.write().await;
            *current = config.clone();
        }
        
        if let Ok(metadata) = fs::metadata(&self.config_path) {
            if let Ok(modified) = metadata.modified() {
                let mut last_mod = self.last_modified.write().await;
                *last_mod = Some(modified);
            }
        }
        
        Ok(config)
    }

    /// Get current configuration
    pub async fn get_current_config(&self) -> TriggerConfig {
        self.current_config.read().await.clone()
    }

    /// Save configuration to file
    pub async fn save_config(&self, config: &TriggerConfig) -> Result<()> {
        Self::save_config_to_file(&self.config_path, config).await?;
        
        // Update current config
        {
            let mut current = self.current_config.write().await;
            *current = config.clone();
        }
        
        Ok(())
    }

    /// Validate configuration without loading
    pub async fn validate_config_file(&self) -> Result<()> {
        Self::validate_config_file_at_path(&self.config_path).await
    }

    // Private methods
    async fn load_config_from_file(config_path: &str) -> Result<TriggerConfig> {
        if !Path::new(config_path).exists() {
            info!("Config file not found, creating default: {}", config_path);
            let default_config = TriggerConfig::default();
            Self::save_config_to_file(config_path, &default_config).await?;
            return Ok(default_config);
        }

        let content = fs::read_to_string(config_path).map_err(|e| {
            MemoryError::Configuration(format!("Failed to read config file {}: {}", config_path, e))
        })?;

        let json_value: Value = serde_json::from_str(&content).map_err(|e| {
            MemoryError::Configuration(format!("Invalid JSON in config file {}: {}", config_path, e))
        })?;

        Self::parse_config_from_json(json_value).await
    }

    async fn parse_config_from_json(json_value: Value) -> Result<TriggerConfig> {
        let obj = json_value.as_object().ok_or_else(|| {
            MemoryError::Configuration("Config must be a JSON object".to_string())
        })?;

        // Parse basic settings
        let importance_multiplier = obj
            .get("importance_multiplier")
            .and_then(|v| v.as_f64())
            .unwrap_or(2.0);

        let max_processing_time_ms = obj
            .get("max_processing_time_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(50);

        let enable_ab_testing = obj
            .get("enable_ab_testing")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Parse patterns
        let mut patterns = HashMap::new();
        if let Some(patterns_obj) = obj.get("patterns").and_then(|v| v.as_object()) {
            for (trigger_name, pattern_value) in patterns_obj {
                let trigger_event = Self::parse_trigger_event(trigger_name)?;
                let pattern = Self::parse_trigger_pattern(pattern_value).await?;
                patterns.insert(trigger_event, pattern);
            }
        }

        // Parse user customizations
        let mut user_customizations = HashMap::new();
        if let Some(customizations_obj) = obj.get("user_customizations").and_then(|v| v.as_object()) {
            for (user_id, user_patterns_value) in customizations_obj {
                if let Some(user_patterns_obj) = user_patterns_value.as_object() {
                    let mut user_patterns = HashMap::new();
                    for (trigger_name, pattern_value) in user_patterns_obj {
                        let trigger_event = Self::parse_trigger_event(trigger_name)?;
                        let pattern = Self::parse_trigger_pattern(pattern_value).await?;
                        user_patterns.insert(trigger_event, pattern);
                    }
                    user_customizations.insert(user_id.clone(), user_patterns);
                }
            }
        }

        Ok(TriggerConfig {
            patterns,
            importance_multiplier,
            max_processing_time_ms,
            enable_ab_testing,
            user_customizations,
        })
    }

    fn parse_trigger_event(trigger_name: &str) -> Result<TriggerEvent> {
        match trigger_name {
            "Security" => Ok(TriggerEvent::Security),
            "Error" => Ok(TriggerEvent::Error),
            "Performance" => Ok(TriggerEvent::Performance),
            "BusinessCritical" => Ok(TriggerEvent::BusinessCritical),
            "UserExperience" => Ok(TriggerEvent::UserExperience),
            _ => Err(MemoryError::Configuration(format!(
                "Unknown trigger event type: {}",
                trigger_name
            ))),
        }
    }

    async fn parse_trigger_pattern(pattern_value: &Value) -> Result<TriggerPattern> {
        let pattern_obj = pattern_value.as_object().ok_or_else(|| {
            MemoryError::Configuration("Trigger pattern must be an object".to_string())
        })?;

        let regex = pattern_obj
            .get("regex")
            .and_then(|v| v.as_str())
            .ok_or_else(|| MemoryError::Configuration("Missing regex field".to_string()))?
            .to_string();

        // Validate regex
        Regex::new(&regex).map_err(|e| {
            MemoryError::Configuration(format!("Invalid regex pattern: {}", e))
        })?;

        let keywords = pattern_obj
            .get("keywords")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default();

        let context_boosters = pattern_obj
            .get("context_boosters")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default();

        let confidence_threshold = pattern_obj
            .get("confidence_threshold")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.7);

        let enabled = pattern_obj
            .get("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let mut pattern = TriggerPattern::new(regex, keywords)?;
        pattern.context_boosters = context_boosters;
        pattern.confidence_threshold = confidence_threshold;
        pattern.enabled = enabled;

        Ok(pattern)
    }

    async fn save_config_to_file(config_path: &str, config: &TriggerConfig) -> Result<()> {
        let json_value = Self::config_to_json(config).await?;
        let content = serde_json::to_string_pretty(&json_value).map_err(|e| {
            MemoryError::Configuration(format!("Failed to serialize config: {}", e))
        })?;

        fs::write(config_path, content).map_err(|e| {
            MemoryError::Configuration(format!("Failed to write config file {}: {}", config_path, e))
        })?;

        info!("Configuration saved to: {}", config_path);
        Ok(())
    }

    async fn config_to_json(config: &TriggerConfig) -> Result<Value> {
        let mut patterns_obj = serde_json::Map::new();
        for (trigger_event, pattern) in &config.patterns {
            let trigger_name = match trigger_event {
                TriggerEvent::Security => "Security",
                TriggerEvent::Error => "Error", 
                TriggerEvent::Performance => "Performance",
                TriggerEvent::BusinessCritical => "BusinessCritical",
                TriggerEvent::UserExperience => "UserExperience",
            };

            let pattern_obj = serde_json::json!({
                "regex": pattern.regex,
                "keywords": pattern.keywords,
                "context_boosters": pattern.context_boosters,
                "confidence_threshold": pattern.confidence_threshold,
                "enabled": pattern.enabled
            });

            patterns_obj.insert(trigger_name.to_string(), pattern_obj);
        }

        let mut user_customizations_obj = serde_json::Map::new();
        for (user_id, user_patterns) in &config.user_customizations {
            let mut user_patterns_obj = serde_json::Map::new();
            for (trigger_event, pattern) in user_patterns {
                let trigger_name = match trigger_event {
                    TriggerEvent::Security => "Security",
                    TriggerEvent::Error => "Error",
                    TriggerEvent::Performance => "Performance", 
                    TriggerEvent::BusinessCritical => "BusinessCritical",
                    TriggerEvent::UserExperience => "UserExperience",
                };

                let pattern_obj = serde_json::json!({
                    "regex": pattern.regex,
                    "keywords": pattern.keywords,
                    "context_boosters": pattern.context_boosters,
                    "confidence_threshold": pattern.confidence_threshold,
                    "enabled": pattern.enabled
                });

                user_patterns_obj.insert(trigger_name.to_string(), pattern_obj);
            }
            user_customizations_obj.insert(user_id.clone(), Value::Object(user_patterns_obj));
        }

        Ok(serde_json::json!({
            "importance_multiplier": config.importance_multiplier,
            "max_processing_time_ms": config.max_processing_time_ms,
            "enable_ab_testing": config.enable_ab_testing,
            "patterns": Value::Object(patterns_obj),
            "user_customizations": Value::Object(user_customizations_obj)
        }))
    }

    async fn validate_config_file_at_path(config_path: &str) -> Result<()> {
        if !Path::new(config_path).exists() {
            return Err(MemoryError::Configuration(format!(
                "Config file does not exist: {}",
                config_path
            )));
        }

        // Try to load and parse the config
        Self::load_config_from_file(config_path).await?;
        Ok(())
    }

    async fn check_and_reload_config(
        config_path: &str,
        last_modified: &Arc<RwLock<Option<SystemTime>>>,
        current_config: &Arc<RwLock<TriggerConfig>>,
    ) -> Result<()> {
        if let Ok(metadata) = fs::metadata(config_path) {
            if let Ok(modified) = metadata.modified() {
                let should_reload = {
                    let last_mod = last_modified.read().await;
                    match &*last_mod {
                        Some(last) => modified > *last,
                        None => true,
                    }
                };

                if should_reload {
                    match Self::load_config_from_file(config_path).await {
                        Ok(new_config) => {
                            {
                                let mut config = current_config.write().await;
                                *config = new_config;
                            }
                            {
                                let mut last_mod = last_modified.write().await;
                                *last_mod = Some(modified);
                            }
                            info!("Configuration hot-reloaded from: {}", config_path);
                        }
                        Err(e) => {
                            warn!("Failed to reload config (keeping current): {}", e);
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_load_default_config() {
        let temp_file = NamedTempFile::new().unwrap();
        let config_path = temp_file.path().to_str().unwrap().to_string();
        
        // Remove the temp file so loader creates default
        std::fs::remove_file(&config_path).unwrap();
        
        let loader = TriggerConfigLoader::new(config_path.clone());
        let config = loader.load_config().await.unwrap();
        
        assert_eq!(config.importance_multiplier, 2.0);
        assert_eq!(config.max_processing_time_ms, 50);
        assert_eq!(config.patterns.len(), 5);
        
        // Verify file was created
        assert!(Path::new(&config_path).exists());
    }

    #[tokio::test]
    async fn test_load_custom_config() {
        let temp_file = NamedTempFile::new().unwrap();
        let config_path = temp_file.path().to_str().unwrap().to_string();
        
        let custom_config = r#"{
            "importance_multiplier": 3.0,
            "max_processing_time_ms": 100,
            "enable_ab_testing": true,
            "patterns": {
                "Security": {
                    "regex": "(?i)(security|threat)",
                    "keywords": ["security", "threat"],
                    "context_boosters": ["critical"],
                    "confidence_threshold": 0.9,
                    "enabled": true
                }
            },
            "user_customizations": {}
        }"#;
        
        std::fs::write(&config_path, custom_config).unwrap();
        
        let loader = TriggerConfigLoader::new(config_path);
        let config = loader.load_config().await.unwrap();
        
        assert_eq!(config.importance_multiplier, 3.0);
        assert_eq!(config.max_processing_time_ms, 100);
        assert!(config.enable_ab_testing);
        assert_eq!(config.patterns.len(), 1);
    }

    #[tokio::test]
    async fn test_save_and_reload() {
        let temp_file = NamedTempFile::new().unwrap();
        let config_path = temp_file.path().to_str().unwrap().to_string();
        
        let loader = TriggerConfigLoader::new(config_path.clone());
        
        // Create a custom config
        let mut config = TriggerConfig::default();
        config.importance_multiplier = 4.0;
        
        // Save it
        loader.save_config(&config).await.unwrap();
        
        // Create new loader and load
        let new_loader = TriggerConfigLoader::new(config_path);
        let loaded_config = new_loader.load_config().await.unwrap();
        
        assert_eq!(loaded_config.importance_multiplier, 4.0);
    }

    #[tokio::test]
    async fn test_config_validation() {
        let temp_file = NamedTempFile::new().unwrap();
        let config_path = temp_file.path().to_str().unwrap().to_string();
        
        // Write invalid JSON
        std::fs::write(&config_path, "invalid json").unwrap();
        
        let loader = TriggerConfigLoader::new(config_path);
        assert!(loader.validate_config_file().await.is_err());
    }

    #[tokio::test]
    async fn test_hot_reload() {
        let temp_file = NamedTempFile::new().unwrap();
        let config_path = temp_file.path().to_str().unwrap().to_string();
        
        // Initial config
        let initial_config = r#"{
            "importance_multiplier": 2.0,
            "max_processing_time_ms": 50,
            "enable_ab_testing": false,
            "patterns": {},
            "user_customizations": {}
        }"#;
        std::fs::write(&config_path, initial_config).unwrap();
        
        let mut loader = TriggerConfigLoader::new(config_path.clone());
        loader.enable_hot_reload(Duration::from_millis(50));
        
        // Load initial config
        let config = loader.load_config().await.unwrap();
        assert_eq!(config.importance_multiplier, 2.0);
        
        // Wait a bit for hot-reload timer to start
        sleep(Duration::from_millis(100)).await;
        
        // Update config file
        let updated_config = r#"{
            "importance_multiplier": 5.0,
            "max_processing_time_ms": 50,
            "enable_ab_testing": false,
            "patterns": {},
            "user_customizations": {}
        }"#;
        std::fs::write(&config_path, updated_config).unwrap();
        
        // Wait for hot-reload to pick up changes
        sleep(Duration::from_millis(200)).await;
        
        let current_config = loader.get_current_config().await;
        assert_eq!(current_config.importance_multiplier, 5.0);
    }
}