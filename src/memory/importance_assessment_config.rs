use crate::memory::importance_assessment::{
    CircuitBreakerConfig, ImportanceAssessmentConfig, ImportancePattern, PerformanceConfig,
    ReferenceEmbedding, Stage1Config, Stage2Config, Stage3Config,
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use tracing::{info, warn};

/// Configuration loader for importance assessment pipeline
pub struct ImportanceAssessmentConfigLoader;

impl ImportanceAssessmentConfigLoader {
    /// Load configuration from a TOML file
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<ImportanceAssessmentConfig> {
        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read config file: {}", path.as_ref().display()))?;

        let config: ImportanceAssessmentConfigFile = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {}", path.as_ref().display()))?;

        info!(
            "Loaded importance assessment config from: {}",
            path.as_ref().display()
        );
        Self::validate_and_convert(config)
    }

    /// Load configuration from environment variables with defaults
    pub fn load_from_env() -> Result<ImportanceAssessmentConfig> {
        let mut config = ImportanceAssessmentConfig::default();

        // Stage 1 configuration
        if let Ok(threshold) = std::env::var("CODEX_STAGE1_CONFIDENCE_THRESHOLD") {
            config.stage1.confidence_threshold = threshold
                .parse()
                .context("Invalid CODEX_STAGE1_CONFIDENCE_THRESHOLD")?;
        }

        if let Ok(max_time) = std::env::var("CODEX_STAGE1_MAX_TIME_MS") {
            config.stage1.max_processing_time_ms = max_time
                .parse()
                .context("Invalid CODEX_STAGE1_MAX_TIME_MS")?;
        }

        // Stage 2 configuration
        if let Ok(threshold) = std::env::var("CODEX_STAGE2_CONFIDENCE_THRESHOLD") {
            config.stage2.confidence_threshold = threshold
                .parse()
                .context("Invalid CODEX_STAGE2_CONFIDENCE_THRESHOLD")?;
        }

        if let Ok(max_time) = std::env::var("CODEX_STAGE2_MAX_TIME_MS") {
            config.stage2.max_processing_time_ms = max_time
                .parse()
                .context("Invalid CODEX_STAGE2_MAX_TIME_MS")?;
        }

        if let Ok(cache_ttl) = std::env::var("CODEX_STAGE2_CACHE_TTL_SECONDS") {
            config.stage2.embedding_cache_ttl_seconds = cache_ttl
                .parse()
                .context("Invalid CODEX_STAGE2_CACHE_TTL_SECONDS")?;
        }

        if let Ok(similarity_threshold) = std::env::var("CODEX_STAGE2_SIMILARITY_THRESHOLD") {
            config.stage2.similarity_threshold = similarity_threshold
                .parse()
                .context("Invalid CODEX_STAGE2_SIMILARITY_THRESHOLD")?;
        }

        // Stage 3 configuration
        if let Ok(max_time) = std::env::var("CODEX_STAGE3_MAX_TIME_MS") {
            config.stage3.max_processing_time_ms = max_time
                .parse()
                .context("Invalid CODEX_STAGE3_MAX_TIME_MS")?;
        }

        if let Ok(endpoint) = std::env::var("CODEX_STAGE3_LLM_ENDPOINT") {
            config.stage3.llm_endpoint = endpoint;
        }

        if let Ok(max_concurrent) = std::env::var("CODEX_STAGE3_MAX_CONCURRENT") {
            config.stage3.max_concurrent_requests = max_concurrent
                .parse()
                .context("Invalid CODEX_STAGE3_MAX_CONCURRENT")?;
        }

        if let Ok(usage_pct) = std::env::var("CODEX_STAGE3_TARGET_USAGE_PCT") {
            config.stage3.target_usage_percentage = usage_pct
                .parse()
                .context("Invalid CODEX_STAGE3_TARGET_USAGE_PCT")?;
        }

        // Circuit breaker configuration
        if let Ok(failure_threshold) = std::env::var("CODEX_CB_FAILURE_THRESHOLD") {
            config.circuit_breaker.failure_threshold = failure_threshold
                .parse()
                .context("Invalid CODEX_CB_FAILURE_THRESHOLD")?;
        }

        if let Ok(failure_window) = std::env::var("CODEX_CB_FAILURE_WINDOW_SECONDS") {
            config.circuit_breaker.failure_window_seconds = failure_window
                .parse()
                .context("Invalid CODEX_CB_FAILURE_WINDOW_SECONDS")?;
        }

        if let Ok(recovery_timeout) = std::env::var("CODEX_CB_RECOVERY_TIMEOUT_SECONDS") {
            config.circuit_breaker.recovery_timeout_seconds = recovery_timeout
                .parse()
                .context("Invalid CODEX_CB_RECOVERY_TIMEOUT_SECONDS")?;
        }

        if let Ok(min_requests) = std::env::var("CODEX_CB_MINIMUM_REQUESTS") {
            config.circuit_breaker.minimum_requests = min_requests
                .parse()
                .context("Invalid CODEX_CB_MINIMUM_REQUESTS")?;
        }

        // Performance configuration
        if let Ok(target_ms) = std::env::var("CODEX_PERF_STAGE1_TARGET_MS") {
            config.performance.stage1_target_ms = target_ms
                .parse()
                .context("Invalid CODEX_PERF_STAGE1_TARGET_MS")?;
        }

        if let Ok(target_ms) = std::env::var("CODEX_PERF_STAGE2_TARGET_MS") {
            config.performance.stage2_target_ms = target_ms
                .parse()
                .context("Invalid CODEX_PERF_STAGE2_TARGET_MS")?;
        }

        if let Ok(target_ms) = std::env::var("CODEX_PERF_STAGE3_TARGET_MS") {
            config.performance.stage3_target_ms = target_ms
                .parse()
                .context("Invalid CODEX_PERF_STAGE3_TARGET_MS")?;
        }

        info!("Loaded importance assessment config from environment variables");
        Self::validate_config(&config)?;
        Ok(config)
    }

    /// Load configuration with fallback: file -> env -> defaults
    pub fn load_with_fallback<P: AsRef<Path>>(
        config_path: Option<P>,
    ) -> Result<ImportanceAssessmentConfig> {
        // Try to load from file first
        if let Some(path) = config_path {
            if path.as_ref().exists() {
                match Self::load_from_file(path) {
                    Ok(config) => return Ok(config),
                    Err(e) => {
                        warn!(
                            "Failed to load config from file, falling back to environment: {}",
                            e
                        );
                    }
                }
            } else {
                info!(
                    "Config file not found: {}, using environment/defaults",
                    path.as_ref().display()
                );
            }
        }

        // Fall back to environment variables
        Self::load_from_env()
    }

    /// Create a production-ready configuration
    pub fn production_config() -> ImportanceAssessmentConfig {
        let mut config = ImportanceAssessmentConfig::default();

        // Production tuning for performance and reliability
        config.stage1.confidence_threshold = 0.7; // Higher threshold for production
        config.stage1.max_processing_time_ms = 5; // Very fast Stage 1

        config.stage2.confidence_threshold = 0.8; // Higher threshold for production
        config.stage2.max_processing_time_ms = 50; // Fast Stage 2
        config.stage2.embedding_cache_ttl_seconds = 7200; // 2 hour cache
        config.stage2.similarity_threshold = 0.75; // Higher similarity requirement

        config.stage3.max_processing_time_ms = 500; // Faster Stage 3 timeout
        config.stage3.max_concurrent_requests = 3; // Conservative concurrency
        config.stage3.target_usage_percentage = 15.0; // Lower Stage 3 usage

        config.circuit_breaker.failure_threshold = 3; // More sensitive circuit breaker
        config.circuit_breaker.failure_window_seconds = 30; // Shorter failure window
        config.circuit_breaker.recovery_timeout_seconds = 60; // Longer recovery time
        config.circuit_breaker.minimum_requests = 2; // Lower minimum requests

        config.performance.stage1_target_ms = 5; // Aggressive Stage 1 target
        config.performance.stage2_target_ms = 50; // Aggressive Stage 2 target
        config.performance.stage3_target_ms = 500; // Aggressive Stage 3 target

        info!("Created production-optimized importance assessment config");
        config
    }

    /// Create a development-friendly configuration
    pub fn development_config() -> ImportanceAssessmentConfig {
        let mut config = ImportanceAssessmentConfig::default();

        // Development tuning for easier debugging and testing
        config.stage1.confidence_threshold = 0.5; // Lower threshold for development
        config.stage1.max_processing_time_ms = 20; // More lenient timing

        config.stage2.confidence_threshold = 0.6; // Lower threshold for development
        config.stage2.max_processing_time_ms = 200; // More lenient timing
        config.stage2.embedding_cache_ttl_seconds = 1800; // 30 minute cache
        config.stage2.similarity_threshold = 0.6; // Lower similarity requirement

        config.stage3.max_processing_time_ms = 2000; // Generous Stage 3 timeout
        config.stage3.max_concurrent_requests = 10; // Higher concurrency for testing
        config.stage3.target_usage_percentage = 30.0; // Higher Stage 3 usage for testing

        config.circuit_breaker.failure_threshold = 10; // Less sensitive circuit breaker
        config.circuit_breaker.failure_window_seconds = 120; // Longer failure window
        config.circuit_breaker.recovery_timeout_seconds = 30; // Shorter recovery time
        config.circuit_breaker.minimum_requests = 5; // Higher minimum requests

        config.performance.stage1_target_ms = 20; // Lenient Stage 1 target
        config.performance.stage2_target_ms = 200; // Lenient Stage 2 target
        config.performance.stage3_target_ms = 2000; // Lenient Stage 3 target

        info!("Created development-optimized importance assessment config");
        config
    }

    /// Export configuration to TOML file
    pub fn export_to_file<P: AsRef<Path>>(
        config: &ImportanceAssessmentConfig,
        path: P,
    ) -> Result<()> {
        let config_file = Self::convert_to_file_format(config);
        let toml_content =
            toml::to_string_pretty(&config_file).context("Failed to serialize config to TOML")?;

        fs::write(&path, toml_content)
            .with_context(|| format!("Failed to write config file: {}", path.as_ref().display()))?;

        info!(
            "Exported importance assessment config to: {}",
            path.as_ref().display()
        );
        Ok(())
    }

    fn validate_and_convert(
        config_file: ImportanceAssessmentConfigFile,
    ) -> Result<ImportanceAssessmentConfig> {
        let config = ImportanceAssessmentConfig {
            stage1: Stage1Config {
                confidence_threshold: config_file.stage1.confidence_threshold,
                pattern_library: config_file
                    .stage1
                    .patterns
                    .into_iter()
                    .map(|p| ImportancePattern {
                        name: p.name,
                        pattern: p.pattern,
                        weight: p.weight,
                        context_boosters: p.context_boosters.unwrap_or_default(),
                        category: p.category,
                    })
                    .collect(),
                max_processing_time_ms: config_file.stage1.max_processing_time_ms,
            },
            stage2: Stage2Config {
                confidence_threshold: config_file.stage2.confidence_threshold,
                max_processing_time_ms: config_file.stage2.max_processing_time_ms,
                embedding_cache_ttl_seconds: config_file.stage2.embedding_cache_ttl_seconds,
                embedding_cache_max_size: config_file
                    .stage2
                    .embedding_cache_max_size
                    .unwrap_or(10000),
                cache_eviction_threshold: config_file
                    .stage2
                    .cache_eviction_threshold
                    .unwrap_or(0.8),
                similarity_threshold: config_file.stage2.similarity_threshold,
                reference_embeddings: config_file
                    .stage2
                    .reference_embeddings
                    .unwrap_or_default()
                    .into_iter()
                    .map(|r| ReferenceEmbedding {
                        name: r.name,
                        embedding: r.embedding,
                        weight: r.weight,
                        category: r.category,
                    })
                    .collect(),
            },
            stage3: Stage3Config {
                max_processing_time_ms: config_file.stage3.max_processing_time_ms,
                llm_endpoint: config_file.stage3.llm_endpoint,
                max_concurrent_requests: config_file.stage3.max_concurrent_requests,
                prompt_template: config_file.stage3.prompt_template,
                target_usage_percentage: config_file.stage3.target_usage_percentage,
            },
            circuit_breaker: CircuitBreakerConfig {
                failure_threshold: config_file.circuit_breaker.failure_threshold,
                failure_window_seconds: config_file.circuit_breaker.failure_window_seconds,
                recovery_timeout_seconds: config_file.circuit_breaker.recovery_timeout_seconds,
                minimum_requests: config_file.circuit_breaker.minimum_requests,
            },
            performance: PerformanceConfig {
                stage1_target_ms: config_file.performance.stage1_target_ms,
                stage2_target_ms: config_file.performance.stage2_target_ms,
                stage3_target_ms: config_file.performance.stage3_target_ms,
            },
        };

        Self::validate_config(&config)?;
        Ok(config)
    }

    fn convert_to_file_format(
        config: &ImportanceAssessmentConfig,
    ) -> ImportanceAssessmentConfigFile {
        ImportanceAssessmentConfigFile {
            stage1: Stage1ConfigFile {
                confidence_threshold: config.stage1.confidence_threshold,
                max_processing_time_ms: config.stage1.max_processing_time_ms,
                patterns: config
                    .stage1
                    .pattern_library
                    .iter()
                    .map(|p| ImportancePatternFile {
                        name: p.name.clone(),
                        pattern: p.pattern.clone(),
                        weight: p.weight,
                        context_boosters: if p.context_boosters.is_empty() {
                            None
                        } else {
                            Some(p.context_boosters.clone())
                        },
                        category: p.category.clone(),
                    })
                    .collect(),
            },
            stage2: Stage2ConfigFile {
                confidence_threshold: config.stage2.confidence_threshold,
                max_processing_time_ms: config.stage2.max_processing_time_ms,
                embedding_cache_ttl_seconds: config.stage2.embedding_cache_ttl_seconds,
                embedding_cache_max_size: Some(config.stage2.embedding_cache_max_size),
                cache_eviction_threshold: Some(config.stage2.cache_eviction_threshold),
                similarity_threshold: config.stage2.similarity_threshold,
                reference_embeddings: if config.stage2.reference_embeddings.is_empty() {
                    None
                } else {
                    Some(
                        config
                            .stage2
                            .reference_embeddings
                            .iter()
                            .map(|r| ReferenceEmbeddingFile {
                                name: r.name.clone(),
                                embedding: r.embedding.clone(),
                                weight: r.weight,
                                category: r.category.clone(),
                            })
                            .collect(),
                    )
                },
            },
            stage3: Stage3ConfigFile {
                max_processing_time_ms: config.stage3.max_processing_time_ms,
                llm_endpoint: config.stage3.llm_endpoint.clone(),
                max_concurrent_requests: config.stage3.max_concurrent_requests,
                prompt_template: config.stage3.prompt_template.clone(),
                target_usage_percentage: config.stage3.target_usage_percentage,
            },
            circuit_breaker: CircuitBreakerConfigFile {
                failure_threshold: config.circuit_breaker.failure_threshold,
                failure_window_seconds: config.circuit_breaker.failure_window_seconds,
                recovery_timeout_seconds: config.circuit_breaker.recovery_timeout_seconds,
                minimum_requests: config.circuit_breaker.minimum_requests,
            },
            performance: PerformanceConfigFile {
                stage1_target_ms: config.performance.stage1_target_ms,
                stage2_target_ms: config.performance.stage2_target_ms,
                stage3_target_ms: config.performance.stage3_target_ms,
            },
        }
    }

    fn validate_config(config: &ImportanceAssessmentConfig) -> Result<()> {
        // Validate Stage 1
        if config.stage1.confidence_threshold < 0.0 || config.stage1.confidence_threshold > 1.0 {
            return Err(anyhow::anyhow!(
                "Stage 1 confidence threshold must be between 0.0 and 1.0"
            ));
        }

        if config.stage1.max_processing_time_ms == 0 {
            return Err(anyhow::anyhow!(
                "Stage 1 max processing time must be greater than 0"
            ));
        }

        if config.stage1.pattern_library.is_empty() {
            return Err(anyhow::anyhow!("Stage 1 must have at least one pattern"));
        }

        for pattern in &config.stage1.pattern_library {
            if pattern.weight < 0.0 || pattern.weight > 1.0 {
                return Err(anyhow::anyhow!(
                    "Pattern '{}' weight must be between 0.0 and 1.0",
                    pattern.name
                ));
            }

            // Test regex compilation
            if let Err(e) = regex::Regex::new(&pattern.pattern) {
                return Err(anyhow::anyhow!(
                    "Pattern '{}' has invalid regex: {}",
                    pattern.name,
                    e
                ));
            }
        }

        // Validate Stage 2
        if config.stage2.confidence_threshold < 0.0 || config.stage2.confidence_threshold > 1.0 {
            return Err(anyhow::anyhow!(
                "Stage 2 confidence threshold must be between 0.0 and 1.0"
            ));
        }

        if config.stage2.max_processing_time_ms == 0 {
            return Err(anyhow::anyhow!(
                "Stage 2 max processing time must be greater than 0"
            ));
        }

        if config.stage2.similarity_threshold < 0.0 || config.stage2.similarity_threshold > 1.0 {
            return Err(anyhow::anyhow!(
                "Stage 2 similarity threshold must be between 0.0 and 1.0"
            ));
        }

        for reference in &config.stage2.reference_embeddings {
            if reference.weight < 0.0 || reference.weight > 1.0 {
                return Err(anyhow::anyhow!(
                    "Reference '{}' weight must be between 0.0 and 1.0",
                    reference.name
                ));
            }

            if reference.embedding.is_empty() {
                return Err(anyhow::anyhow!(
                    "Reference '{}' embedding cannot be empty",
                    reference.name
                ));
            }
        }

        // Validate Stage 3
        if config.stage3.max_processing_time_ms == 0 {
            return Err(anyhow::anyhow!(
                "Stage 3 max processing time must be greater than 0"
            ));
        }

        if config.stage3.max_concurrent_requests == 0 {
            return Err(anyhow::anyhow!(
                "Stage 3 max concurrent requests must be greater than 0"
            ));
        }

        if config.stage3.target_usage_percentage < 0.0
            || config.stage3.target_usage_percentage > 100.0
        {
            return Err(anyhow::anyhow!(
                "Stage 3 target usage percentage must be between 0.0 and 100.0"
            ));
        }

        // Validate Circuit Breaker
        if config.circuit_breaker.failure_threshold == 0 {
            return Err(anyhow::anyhow!(
                "Circuit breaker failure threshold must be greater than 0"
            ));
        }

        if config.circuit_breaker.failure_window_seconds == 0 {
            return Err(anyhow::anyhow!(
                "Circuit breaker failure window must be greater than 0"
            ));
        }

        if config.circuit_breaker.recovery_timeout_seconds == 0 {
            return Err(anyhow::anyhow!(
                "Circuit breaker recovery timeout must be greater than 0"
            ));
        }

        // Validate Performance
        if config.performance.stage1_target_ms == 0 {
            return Err(anyhow::anyhow!(
                "Stage 1 target time must be greater than 0"
            ));
        }

        if config.performance.stage2_target_ms == 0 {
            return Err(anyhow::anyhow!(
                "Stage 2 target time must be greater than 0"
            ));
        }

        if config.performance.stage3_target_ms == 0 {
            return Err(anyhow::anyhow!(
                "Stage 3 target time must be greater than 0"
            ));
        }

        // Validate logical constraints
        if config.stage1.confidence_threshold >= config.stage2.confidence_threshold {
            warn!(
                "Stage 1 confidence threshold ({}) should be lower than Stage 2 ({})",
                config.stage1.confidence_threshold, config.stage2.confidence_threshold
            );
        }

        if config.performance.stage1_target_ms >= config.performance.stage2_target_ms {
            warn!(
                "Stage 1 target time ({}) should be lower than Stage 2 ({})",
                config.performance.stage1_target_ms, config.performance.stage2_target_ms
            );
        }

        if config.performance.stage2_target_ms >= config.performance.stage3_target_ms {
            warn!(
                "Stage 2 target time ({}) should be lower than Stage 3 ({})",
                config.performance.stage2_target_ms, config.performance.stage3_target_ms
            );
        }

        Ok(())
    }
}

// File format structures for TOML serialization
#[derive(Debug, Serialize, Deserialize)]
struct ImportanceAssessmentConfigFile {
    stage1: Stage1ConfigFile,
    stage2: Stage2ConfigFile,
    stage3: Stage3ConfigFile,
    circuit_breaker: CircuitBreakerConfigFile,
    performance: PerformanceConfigFile,
}

#[derive(Debug, Serialize, Deserialize)]
struct Stage1ConfigFile {
    confidence_threshold: f64,
    max_processing_time_ms: u64,
    patterns: Vec<ImportancePatternFile>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ImportancePatternFile {
    name: String,
    pattern: String,
    weight: f64,
    context_boosters: Option<Vec<String>>,
    category: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Stage2ConfigFile {
    confidence_threshold: f64,
    max_processing_time_ms: u64,
    embedding_cache_ttl_seconds: u64,
    embedding_cache_max_size: Option<usize>,
    cache_eviction_threshold: Option<f64>,
    similarity_threshold: f32,
    reference_embeddings: Option<Vec<ReferenceEmbeddingFile>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ReferenceEmbeddingFile {
    name: String,
    embedding: Vec<f32>,
    weight: f64,
    category: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Stage3ConfigFile {
    max_processing_time_ms: u64,
    llm_endpoint: String,
    max_concurrent_requests: usize,
    prompt_template: String,
    target_usage_percentage: f64,
}

#[derive(Debug, Serialize, Deserialize)]
struct CircuitBreakerConfigFile {
    failure_threshold: usize,
    failure_window_seconds: u64,
    recovery_timeout_seconds: u64,
    minimum_requests: usize,
}

#[derive(Debug, Serialize, Deserialize)]
struct PerformanceConfigFile {
    stage1_target_ms: u64,
    stage2_target_ms: u64,
    stage3_target_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_production_config_validation() {
        let config = ImportanceAssessmentConfigLoader::production_config();
        assert!(ImportanceAssessmentConfigLoader::validate_config(&config).is_ok());

        // Verify production settings
        assert!(config.stage1.confidence_threshold > 0.6);
        assert!(config.stage2.confidence_threshold > 0.7);
        assert!(config.stage3.target_usage_percentage < 20.0);
        assert!(config.circuit_breaker.failure_threshold <= 5);
    }

    #[test]
    fn test_development_config_validation() {
        let config = ImportanceAssessmentConfigLoader::development_config();
        assert!(ImportanceAssessmentConfigLoader::validate_config(&config).is_ok());

        // Verify development settings
        assert!(config.stage1.confidence_threshold <= 0.6);
        assert!(config.stage2.confidence_threshold <= 0.7);
        assert!(config.stage3.target_usage_percentage >= 25.0);
        assert!(config.circuit_breaker.failure_threshold >= 5);
    }

    #[test]
    fn test_config_export_import() -> Result<()> {
        let original_config = ImportanceAssessmentConfigLoader::production_config();

        let temp_file = NamedTempFile::new()?;
        ImportanceAssessmentConfigLoader::export_to_file(&original_config, temp_file.path())?;

        let loaded_config = ImportanceAssessmentConfigLoader::load_from_file(temp_file.path())?;

        // Verify key settings match
        assert_eq!(
            original_config.stage1.confidence_threshold,
            loaded_config.stage1.confidence_threshold
        );
        assert_eq!(
            original_config.stage2.confidence_threshold,
            loaded_config.stage2.confidence_threshold
        );
        assert_eq!(
            original_config.stage3.target_usage_percentage,
            loaded_config.stage3.target_usage_percentage
        );

        Ok(())
    }

    #[test]
    fn test_invalid_config_validation() {
        let mut config = ImportanceAssessmentConfig::default();

        // Test invalid confidence threshold
        config.stage1.confidence_threshold = 1.5;
        assert!(ImportanceAssessmentConfigLoader::validate_config(&config).is_err());

        config.stage1.confidence_threshold = 0.6;

        // Test invalid similarity threshold
        config.stage2.similarity_threshold = 1.5;
        assert!(ImportanceAssessmentConfigLoader::validate_config(&config).is_err());

        config.stage2.similarity_threshold = 0.7;

        // Test zero processing time
        config.stage1.max_processing_time_ms = 0;
        assert!(ImportanceAssessmentConfigLoader::validate_config(&config).is_err());
    }

    #[test]
    fn test_env_config_loading() {
        // Set test environment variables
        std::env::set_var("CODEX_STAGE1_CONFIDENCE_THRESHOLD", "0.8");
        std::env::set_var("CODEX_STAGE2_MAX_TIME_MS", "150");
        std::env::set_var("CODEX_STAGE3_TARGET_USAGE_PCT", "25.0");

        let config = ImportanceAssessmentConfigLoader::load_from_env().unwrap();

        assert_eq!(config.stage1.confidence_threshold, 0.8);
        assert_eq!(config.stage2.max_processing_time_ms, 150);
        assert_eq!(config.stage3.target_usage_percentage, 25.0);

        // Clean up
        std::env::remove_var("CODEX_STAGE1_CONFIDENCE_THRESHOLD");
        std::env::remove_var("CODEX_STAGE2_MAX_TIME_MS");
        std::env::remove_var("CODEX_STAGE3_TARGET_USAGE_PCT");
    }
}
