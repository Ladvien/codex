//! Event-triggered scoring system for immediate evaluation of critical content patterns
//!
//! This module implements a sophisticated pattern detection system that can identify
//! critical content types and boost their importance scores by 2x, ensuring they
//! bypass normal processing pipelines for immediate attention.

use crate::memory::error::{MemoryError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use regex::Regex;
use tokio::sync::RwLock;
use std::sync::Arc;

/// Five core trigger event types for pattern detection
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum TriggerEvent {
    /// Security-related content (vulnerabilities, threats, incidents)
    Security,
    /// Error conditions and critical failures
    Error,
    /// Performance bottlenecks and optimization opportunities
    Performance,
    /// Business-critical decisions and strategic insights
    BusinessCritical,
    /// User feedback and experience issues
    UserExperience,
}

impl TriggerEvent {
    /// Get all trigger event types
    pub fn all_types() -> Vec<TriggerEvent> {
        vec![
            TriggerEvent::Security,
            TriggerEvent::Error,
            TriggerEvent::Performance,
            TriggerEvent::BusinessCritical,
            TriggerEvent::UserExperience,
        ]
    }

    /// Get human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            TriggerEvent::Security => "Security vulnerabilities, threats, and incidents",
            TriggerEvent::Error => "Error conditions and critical system failures",
            TriggerEvent::Performance => "Performance bottlenecks and optimization needs",
            TriggerEvent::BusinessCritical => "Strategic decisions and business-critical insights",
            TriggerEvent::UserExperience => "User feedback and experience issues",
        }
    }

    /// Get priority level for conflict resolution (higher = more important)
    pub fn priority(&self) -> u8 {
        match self {
            TriggerEvent::Security => 100,      // Highest priority
            TriggerEvent::Error => 90,          // Critical system issues
            TriggerEvent::Performance => 70,    // Important but not critical
            TriggerEvent::BusinessCritical => 80, // High business impact
            TriggerEvent::UserExperience => 60, // Important but lower priority
        }
    }
}

/// Pattern configuration for trigger detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerPattern {
    /// Regular expression for pattern matching
    pub regex: String,
    /// Compiled regex (not serialized)
    #[serde(skip)]
    pub compiled_regex: Option<Regex>,
    /// Keywords that indicate this trigger type
    pub keywords: Vec<String>,
    /// Context words that boost confidence
    pub context_boosters: Vec<String>,
    /// Minimum confidence threshold (0.0-1.0)
    pub confidence_threshold: f64,
    /// Whether this pattern is enabled
    pub enabled: bool,
}

impl TriggerPattern {
    /// Create new trigger pattern
    pub fn new(regex: String, keywords: Vec<String>) -> Result<Self> {
        let compiled_regex = Some(Regex::new(&regex).map_err(|e| {
            MemoryError::Configuration(format!("Invalid regex pattern: {}", e))
        })?);

        Ok(TriggerPattern {
            regex,
            compiled_regex,
            keywords,
            context_boosters: Vec::new(),
            confidence_threshold: 0.7,
            enabled: true,
        })
    }

    /// Check if content matches this pattern
    pub fn matches(&self, content: &str) -> bool {
        if !self.enabled {
            return false;
        }

        // Check regex match
        if let Some(ref regex) = self.compiled_regex {
            if regex.is_match(content) {
                return true;
            }
        }

        // Check keyword matches
        let content_lower = content.to_lowercase();
        self.keywords.iter().any(|keyword| content_lower.contains(&keyword.to_lowercase()))
    }

    /// Calculate confidence score for a match (0.0-1.0)
    pub fn calculate_confidence(&self, content: &str) -> f64 {
        if !self.matches(content) {
            return 0.0;
        }

        let content_lower = content.to_lowercase();
        let mut confidence = 0.4; // Base confidence for any match

        // High-value security terms get extra boost
        let high_value_security_terms = ["xss", "injection", "csrf", "vulnerability", "exploit", "malware", "phishing"];
        let has_high_value_security = high_value_security_terms.iter()
            .any(|term| content_lower.contains(term));

        // Boost for keyword matches - more generous scoring
        let keyword_matches = self.keywords.iter()
            .filter(|keyword| content_lower.contains(&keyword.to_lowercase()))
            .count() as f64;
        if keyword_matches > 0.0 {
            // Give a good boost for any keyword matches, with diminishing returns
            confidence += 0.3 + (keyword_matches / self.keywords.len() as f64) * 0.2;
            
            // Extra boost for high-value security terms
            if has_high_value_security && self.keywords.iter().any(|k| high_value_security_terms.contains(&k.as_str())) {
                confidence += 0.1;
            }
        }

        // Boost for context words
        let context_matches = self.context_boosters.iter()
            .filter(|booster| content_lower.contains(&booster.to_lowercase()))
            .count() as f64;
        if !self.context_boosters.is_empty() && context_matches > 0.0 {
            confidence += (context_matches / self.context_boosters.len() as f64) * 0.1;
        }

        confidence.min(1.0)
    }
}

/// Configuration for the entire trigger system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerConfig {
    /// Patterns for each trigger type
    pub patterns: HashMap<TriggerEvent, TriggerPattern>,
    /// Importance multiplier for triggered events (default: 2.0)
    pub importance_multiplier: f64,
    /// Maximum processing time for triggers (default: 50ms)
    pub max_processing_time_ms: u64,
    /// Whether to enable A/B testing
    pub enable_ab_testing: bool,
    /// User-specific customizations
    pub user_customizations: HashMap<String, HashMap<TriggerEvent, TriggerPattern>>,
}

impl Default for TriggerConfig {
    fn default() -> Self {
        let mut patterns = HashMap::new();
        
        // Security patterns
        if let Ok(mut security_pattern) = TriggerPattern::new(
            r"(?i)(vulnerability|exploit|attack|breach|security|threat|malware|phishing|xss|injection|csrf)".to_string(),
            vec![
                "vulnerability".to_string(),
                "exploit".to_string(),
                "attack".to_string(),
                "breach".to_string(),
                "security".to_string(),
                "threat".to_string(),
                "malware".to_string(),
                "phishing".to_string(),
                "xss".to_string(),
                "injection".to_string(),
                "csrf".to_string(),
            ]
        ) {
            security_pattern.confidence_threshold = 0.6; // Lower threshold for better matching
            patterns.insert(TriggerEvent::Security, security_pattern);
        }

        // Error patterns
        if let Ok(mut error_pattern) = TriggerPattern::new(
            r"(?i)(error|exception|failure|crash|panic|fatal|critical)".to_string(),
            vec![
                "error".to_string(),
                "exception".to_string(),
                "failure".to_string(),
                "crash".to_string(),
                "panic".to_string(),
                "fatal".to_string(),
                "critical".to_string(),
            ]
        ) {
            error_pattern.confidence_threshold = 0.6;
            patterns.insert(TriggerEvent::Error, error_pattern);
        }

        // Performance patterns
        if let Ok(mut performance_pattern) = TriggerPattern::new(
            r"(?i)(slow|latency|bottleneck|performance|optimization|memory leak|timeout)".to_string(),
            vec![
                "slow".to_string(),
                "latency".to_string(),
                "bottleneck".to_string(),
                "performance".to_string(),
                "optimization".to_string(),
            ]
        ) {
            performance_pattern.confidence_threshold = 0.6;
            patterns.insert(TriggerEvent::Performance, performance_pattern);
        }

        // Business critical patterns
        if let Ok(mut business_pattern) = TriggerPattern::new(
            r"(?i)(revenue|profit|loss|critical|strategic|decision|customer|retention)".to_string(),
            vec![
                "revenue".to_string(),
                "profit".to_string(),
                "critical".to_string(),
                "strategic".to_string(),
                "decision".to_string(),
            ]
        ) {
            business_pattern.confidence_threshold = 0.6;
            patterns.insert(TriggerEvent::BusinessCritical, business_pattern);
        }

        // User experience patterns
        if let Ok(mut ux_pattern) = TriggerPattern::new(
            r"(?i)(user|usability|feedback|complaint|satisfaction|experience|ui|ux)".to_string(),
            vec![
                "user".to_string(),
                "usability".to_string(),
                "feedback".to_string(),
                "complaint".to_string(),
                "satisfaction".to_string(),
            ]
        ) {
            ux_pattern.confidence_threshold = 0.6;
            patterns.insert(TriggerEvent::UserExperience, ux_pattern);
        }

        TriggerConfig {
            patterns,
            importance_multiplier: 2.0,
            max_processing_time_ms: 50,
            enable_ab_testing: false,
            user_customizations: HashMap::new(),
        }
    }
}

/// Metrics for trigger frequency and performance
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TriggerMetrics {
    /// Total triggers fired by type
    pub triggers_by_type: HashMap<TriggerEvent, u64>,
    /// Total processing time by type
    pub processing_time_by_type: HashMap<TriggerEvent, Duration>,
    /// Detection accuracy by type (for A/B testing)
    pub accuracy_by_type: HashMap<TriggerEvent, f64>,
    /// Total memories processed
    pub total_memories_processed: u64,
    /// Total triggered memories
    pub total_triggered_memories: u64,
    /// Average processing time
    pub average_processing_time: Duration,
}

/// Result of trigger detection
#[derive(Debug, Clone)]
pub struct TriggerDetectionResult {
    /// Whether any trigger was detected
    pub triggered: bool,
    /// The specific trigger type detected (if any)
    pub trigger_type: Option<TriggerEvent>,
    /// Confidence score for the detection
    pub confidence: f64,
    /// Original importance score
    pub original_importance: f64,
    /// Boosted importance score (if triggered)
    pub boosted_importance: f64,
    /// Processing time
    pub processing_time: Duration,
}

/// Main event-triggered scoring engine
pub struct EventTriggeredScoringEngine {
    config: Arc<RwLock<TriggerConfig>>,
    metrics: Arc<RwLock<TriggerMetrics>>,
}

impl EventTriggeredScoringEngine {
    /// Create new event-triggered scoring engine
    pub fn new(config: TriggerConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            metrics: Arc::new(RwLock::new(TriggerMetrics::default())),
        }
    }

    /// Create with default configuration
    pub fn with_default_config() -> Self {
        Self::new(TriggerConfig::default())
    }

    /// Analyze content for trigger patterns with immediate processing
    pub async fn analyze_content(
        &self,
        content: &str,
        original_importance: f64,
        user_id: Option<&str>,
    ) -> Result<TriggerDetectionResult> {
        let start_time = Instant::now();
        let config = self.config.read().await;

        // Check for timeout early
        let max_duration = Duration::from_millis(config.max_processing_time_ms);

        // Get applicable patterns (user-specific or default)
        let patterns = if let Some(user) = user_id {
            config.user_customizations.get(user).unwrap_or(&config.patterns)
        } else {
            &config.patterns
        };

        let mut best_match: Option<(TriggerEvent, f64)> = None;

        // Check each pattern type
        for (trigger_type, pattern) in patterns {
            // Check timeout
            if start_time.elapsed() > max_duration {
                break;
            }

            if pattern.matches(content) {
                let confidence = pattern.calculate_confidence(content);
                if confidence >= pattern.confidence_threshold {
                    if let Some((current_type, current_confidence)) = &best_match {
                        // Use priority as tiebreaker for close confidence scores
                        let confidence_diff = confidence - current_confidence;
                        let should_replace = if confidence_diff.abs() < 0.05 {
                            // If confidence is very close, use priority
                            trigger_type.priority() > current_type.priority()
                        } else {
                            // Otherwise, use confidence
                            confidence > *current_confidence
                        };
                        
                        if should_replace {
                            best_match = Some((trigger_type.clone(), confidence));
                        }
                    } else {
                        best_match = Some((trigger_type.clone(), confidence));
                    }
                }
            }
        }

        let processing_time = start_time.elapsed();

        // Create result
        let result = if let Some((trigger_type, confidence)) = best_match {
            let boosted_importance = original_importance * config.importance_multiplier;
            
            // Update metrics
            self.update_metrics(&trigger_type, processing_time, true).await;

            TriggerDetectionResult {
                triggered: true,
                trigger_type: Some(trigger_type),
                confidence,
                original_importance,
                boosted_importance,
                processing_time,
            }
        } else {
            // Update metrics for non-triggered content
            self.update_metrics_non_triggered(processing_time).await;

            TriggerDetectionResult {
                triggered: false,
                trigger_type: None,
                confidence: 0.0,
                original_importance,
                boosted_importance: original_importance,
                processing_time,
            }
        };

        Ok(result)
    }

    /// Update configuration (hot-reloadable)
    pub async fn update_config(&self, new_config: TriggerConfig) -> Result<()> {
        let mut config = self.config.write().await;
        *config = new_config;
        Ok(())
    }

    /// Get current metrics
    pub async fn get_metrics(&self) -> TriggerMetrics {
        self.metrics.read().await.clone()
    }

    /// Reset metrics
    pub async fn reset_metrics(&self) -> Result<()> {
        let mut metrics = self.metrics.write().await;
        *metrics = TriggerMetrics::default();
        Ok(())
    }

    /// Add user-specific customization
    pub async fn add_user_customization(
        &self,
        user_id: String,
        customizations: HashMap<TriggerEvent, TriggerPattern>,
    ) -> Result<()> {
        let mut config = self.config.write().await;
        config.user_customizations.insert(user_id, customizations);
        Ok(())
    }

    // Private helper methods
    async fn update_metrics(&self, trigger_type: &TriggerEvent, processing_time: Duration, triggered: bool) {
        let mut metrics = self.metrics.write().await;
        
        metrics.total_memories_processed += 1;
        if triggered {
            metrics.total_triggered_memories += 1;
            *metrics.triggers_by_type.entry(trigger_type.clone()).or_insert(0) += 1;
            *metrics.processing_time_by_type.entry(trigger_type.clone()).or_insert(Duration::ZERO) += processing_time;
        }

        // Update average processing time
        let total_time = metrics.average_processing_time * (metrics.total_memories_processed - 1) as u32 + processing_time;
        metrics.average_processing_time = total_time / metrics.total_memories_processed as u32;
    }

    async fn update_metrics_non_triggered(&self, processing_time: Duration) {
        let mut metrics = self.metrics.write().await;
        metrics.total_memories_processed += 1;
        
        // Update average processing time
        let total_time = metrics.average_processing_time * (metrics.total_memories_processed - 1) as u32 + processing_time;
        metrics.average_processing_time = total_time / metrics.total_memories_processed as u32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trigger_event_types() {
        let all_types = TriggerEvent::all_types();
        assert_eq!(all_types.len(), 5);
        assert!(all_types.contains(&TriggerEvent::Security));
        assert!(all_types.contains(&TriggerEvent::Error));
        assert!(all_types.contains(&TriggerEvent::Performance));
        assert!(all_types.contains(&TriggerEvent::BusinessCritical));
        assert!(all_types.contains(&TriggerEvent::UserExperience));
    }

    #[test]
    fn test_trigger_pattern_creation() {
        let pattern = TriggerPattern::new(
            r"(?i)(error|exception)".to_string(),
            vec!["error".to_string(), "exception".to_string()],
        ).unwrap();

        assert!(pattern.matches("An error occurred"));
        assert!(pattern.matches("Exception thrown"));
        assert!(!pattern.matches("Everything is fine"));
    }

    #[test]
    fn test_confidence_calculation() {
        let mut pattern = TriggerPattern::new(
            r"(?i)(error|exception)".to_string(),
            vec!["error".to_string(), "exception".to_string()],
        ).unwrap();
        
        pattern.context_boosters = vec!["critical".to_string(), "fatal".to_string()];

        let confidence1 = pattern.calculate_confidence("An error occurred");
        let confidence2 = pattern.calculate_confidence("A critical error occurred");
        let confidence3 = pattern.calculate_confidence("A critical fatal error occurred");

        assert!(confidence2 > confidence1);
        assert!(confidence3 > confidence2);
        assert!(confidence1 > 0.0);
        assert!(confidence3 <= 1.0);
    }

    #[tokio::test]
    async fn test_event_triggered_scoring_engine() {
        let engine = EventTriggeredScoringEngine::with_default_config();

        // Test security trigger
        let result = engine.analyze_content(
            "Security vulnerability detected in the authentication system",
            0.5,
            None,
        ).await.unwrap();

        assert!(result.triggered);
        assert!(matches!(result.trigger_type, Some(TriggerEvent::Security)));
        assert_eq!(result.boosted_importance, 1.0); // 0.5 * 2.0
        assert!(result.confidence > 0.7);
        assert!(result.processing_time.as_millis() < 50);
    }

    #[tokio::test]
    async fn test_performance_within_limits() {
        let engine = EventTriggeredScoringEngine::with_default_config();

        let result = engine.analyze_content(
            "Performance bottleneck detected in database queries",
            0.6,
            None,
        ).await.unwrap();

        assert!(result.triggered);
        assert!(matches!(result.trigger_type, Some(TriggerEvent::Performance)));
        assert!(result.processing_time.as_millis() < 50);
    }

    #[tokio::test]
    async fn test_metrics_tracking() {
        let engine = EventTriggeredScoringEngine::with_default_config();

        // Process several samples
        let _result1 = engine.analyze_content("Error in system", 0.5, None).await.unwrap();
        let _result2 = engine.analyze_content("Normal content", 0.5, None).await.unwrap();
        let _result3 = engine.analyze_content("Security breach", 0.5, None).await.unwrap();

        let metrics = engine.get_metrics().await;
        assert_eq!(metrics.total_memories_processed, 3);
        assert_eq!(metrics.total_triggered_memories, 2);
        assert!(metrics.triggers_by_type.contains_key(&TriggerEvent::Error));
        assert!(metrics.triggers_by_type.contains_key(&TriggerEvent::Security));
    }
}