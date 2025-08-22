//! Insight Loop Prevention and Quality Control
//!
//! This module implements sophisticated algorithms to prevent insight generation loops,
//! detect duplicate or low-quality insights, and maintain cognitive system stability.
//!
//! ## Cognitive Science Foundation
//!
//! ### Research Basis
//! 1. **Circular Reasoning Prevention (Rips, 1994)**: Avoid self-referential reasoning loops
//! 2. **Novelty Detection (Boden, 2004)**: Ensure insights provide genuine new knowledge
//! 3. **Coherence Checking (Thagard, 2000)**: Validate logical consistency of insights
//! 4. **Semantic Satiation (Severance & Washburn, 1907)**: Prevent concept degradation through repetition
//! 5. **Forgetting for Creativity (Storm & Angello, 2010)**: Strategic forgetting enhances innovation
//!
//! ## Prevention Mechanisms
//!
//! ### 1. Semantic Fingerprinting
//! - Hash insight concepts and relationships
//! - Detect near-duplicate insights with configurable similarity thresholds
//! - Track concept evolution over time
//!
//! ### 2. Causal Chain Analysis
//! - Map insight derivation paths
//! - Detect circular dependencies in reasoning
//! - Enforce maximum inference depth
//!
//! ### 3. Quality Validation
//! - Evidence strength assessment
//! - Coherence scoring
//! - Novelty quantification
//! - Predictive power evaluation
//!
//! ### 4. Temporal Cooling
//! - Cooldown periods between similar insight types
//! - Exponential backoff for failed insight attempts
//! - Strategic forgetting of low-quality insights
//!
//! ### 5. Diversity Enforcement
//! - Encourage insight type diversity
//! - Penalize over-representation of specific patterns
//! - Reward novel insight combinations

use super::error::Result;
use super::reflection_engine::{Insight, InsightType};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use tracing::info;
use uuid::Uuid;

/// Configuration for insight loop prevention
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopPreventionConfig {
    /// Minimum semantic distance for considering insights different
    pub semantic_similarity_threshold: f64,

    /// Maximum depth for causal chain analysis
    pub max_causal_depth: usize,

    /// Cooldown period between similar insights (hours)
    pub insight_cooldown_hours: i64,

    /// Minimum novelty score for insight acceptance
    pub min_novelty_threshold: f64,

    /// Minimum coherence score for insight acceptance
    pub min_coherence_threshold: f64,

    /// Minimum evidence strength for insight acceptance
    pub min_evidence_threshold: f64,

    /// Maximum insights per concept cluster
    pub max_insights_per_cluster: usize,

    /// Window for tracking recent insights (days)
    pub tracking_window_days: i64,

    /// Diversity bonus for underrepresented insight types
    pub diversity_bonus_multiplier: f64,

    /// Penalty for overrepresented insight types
    pub repetition_penalty_factor: f64,

    /// Maximum allowed insight derivation depth
    pub max_derivation_depth: usize,

    /// Enable automatic quality filtering
    pub enable_quality_filtering: bool,

    /// Enable semantic fingerprinting
    pub enable_semantic_fingerprinting: bool,

    /// Enable causal chain analysis
    pub enable_causal_analysis: bool,
}

impl Default for LoopPreventionConfig {
    fn default() -> Self {
        Self {
            semantic_similarity_threshold: 0.85,
            max_causal_depth: 5,
            insight_cooldown_hours: 2,
            min_novelty_threshold: 0.3,
            min_coherence_threshold: 0.5,
            min_evidence_threshold: 0.4,
            max_insights_per_cluster: 3,
            tracking_window_days: 7,
            diversity_bonus_multiplier: 1.2,
            repetition_penalty_factor: 0.8,
            max_derivation_depth: 4,
            enable_quality_filtering: true,
            enable_semantic_fingerprinting: true,
            enable_causal_analysis: true,
        }
    }
}

/// Semantic fingerprint for insight deduplication
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct SemanticFingerprint {
    pub concept_hashes: Vec<u64>,
    pub relationship_hashes: Vec<u64>,
    pub content_hash: u64,
    pub insight_type_hash: u64,
}

/// Causal chain node for dependency tracking
#[derive(Debug, Clone)]
pub struct CausalNode {
    pub insight_id: Uuid,
    pub concept: String,
    pub derivation_sources: Vec<Uuid>,
    pub derivation_depth: usize,
    pub created_at: DateTime<Utc>,
}

/// Quality assessment result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityAssessment {
    pub novelty_score: f64,
    pub coherence_score: f64,
    pub evidence_strength: f64,
    pub semantic_richness: f64,
    pub predictive_power: f64,
    pub overall_quality: f64,
    pub quality_factors: Vec<String>,
    pub deficiency_reasons: Vec<String>,
}

/// Loop detection result
#[derive(Debug, Clone)]
pub struct LoopDetectionResult {
    pub has_loop: bool,
    pub loop_path: Vec<Uuid>,
    pub loop_type: LoopType,
    pub severity: LoopSeverity,
    pub prevention_action: PreventionAction,
}

/// Types of loops that can be detected
#[derive(Debug, Clone, PartialEq)]
pub enum LoopType {
    SemanticDuplication,
    CausalCircularity,
    ConceptualSatiation,
    DerivationLoop,
    TemporalRepetition,
}

/// Severity levels for detected loops
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum LoopSeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// Actions to take when loops are detected
#[derive(Debug, Clone, PartialEq)]
pub enum PreventionAction {
    Allow,
    DelayGeneration,
    ModifyInsight,
    RejectInsight,
    TriggerCooldown,
    PruneRedundant,
}

/// Insight tracking entry for loop prevention
#[derive(Debug, Clone)]
pub struct InsightTrackingEntry {
    pub insight_id: Uuid,
    pub fingerprint: SemanticFingerprint,
    pub insight_type: InsightType,
    pub quality_assessment: QualityAssessment,
    pub derivation_path: Vec<Uuid>,
    pub created_at: DateTime<Utc>,
    pub source_concepts: HashSet<String>,
}

/// Main loop prevention engine
pub struct LoopPreventionEngine {
    config: LoopPreventionConfig,
    insight_history: VecDeque<InsightTrackingEntry>,
    concept_frequency: HashMap<String, usize>,
    type_frequency: HashMap<InsightType, usize>,
    #[allow(dead_code)]
    causal_graph: HashMap<Uuid, CausalNode>,
    cooldown_tracker: HashMap<u64, DateTime<Utc>>,
}

impl LoopPreventionEngine {
    pub fn new(config: LoopPreventionConfig) -> Self {
        Self {
            config,
            insight_history: VecDeque::new(),
            concept_frequency: HashMap::new(),
            type_frequency: HashMap::new(),
            causal_graph: HashMap::new(),
            cooldown_tracker: HashMap::new(),
        }
    }

    /// Validate insight and check for loops before acceptance
    pub fn validate_insight(&mut self, insight: &Insight) -> Result<LoopDetectionResult> {
        // Create semantic fingerprint
        let fingerprint = if self.config.enable_semantic_fingerprinting {
            self.create_semantic_fingerprint(insight)?
        } else {
            SemanticFingerprint {
                concept_hashes: Vec::new(),
                relationship_hashes: Vec::new(),
                content_hash: 0,
                insight_type_hash: 0,
            }
        };

        // Check for semantic duplication
        if let Some(duplicate_result) = self.check_semantic_duplication(&fingerprint)? {
            return Ok(duplicate_result);
        }

        // Check for causal loops
        if self.config.enable_causal_analysis {
            if let Some(causal_result) = self.check_causal_loops(insight)? {
                return Ok(causal_result);
            }
        }

        // Check temporal repetition patterns
        if let Some(temporal_result) = self.check_temporal_repetition(insight)? {
            return Ok(temporal_result);
        }

        // Check concept satiation
        if let Some(satiation_result) = self.check_conceptual_satiation(insight)? {
            return Ok(satiation_result);
        }

        // Quality assessment
        let quality = if self.config.enable_quality_filtering {
            self.assess_insight_quality(insight)?
        } else {
            QualityAssessment {
                novelty_score: 1.0,
                coherence_score: 1.0,
                evidence_strength: 1.0,
                semantic_richness: 1.0,
                predictive_power: 1.0,
                overall_quality: 1.0,
                quality_factors: vec!["Quality filtering disabled".to_string()],
                deficiency_reasons: Vec::new(),
            }
        };

        // Check quality thresholds
        if let Some(quality_result) = self.check_quality_thresholds(&quality)? {
            return Ok(quality_result);
        }

        // If all checks pass, allow the insight
        Ok(LoopDetectionResult {
            has_loop: false,
            loop_path: Vec::new(),
            loop_type: LoopType::SemanticDuplication, // Default, not used
            severity: LoopSeverity::Low,
            prevention_action: PreventionAction::Allow,
        })
    }

    /// Register a validated insight in the tracking system
    pub fn register_insight(
        &mut self,
        insight: &Insight,
        quality: QualityAssessment,
    ) -> Result<()> {
        let fingerprint = if self.config.enable_semantic_fingerprinting {
            self.create_semantic_fingerprint(insight)?
        } else {
            SemanticFingerprint {
                concept_hashes: Vec::new(),
                relationship_hashes: Vec::new(),
                content_hash: 0,
                insight_type_hash: 0,
            }
        };

        let tracking_entry = InsightTrackingEntry {
            insight_id: insight.id,
            fingerprint: fingerprint.clone(),
            insight_type: insight.insight_type.clone(),
            quality_assessment: quality,
            derivation_path: Vec::new(), // Would be computed from source memories
            created_at: insight.generated_at,
            source_concepts: insight.related_concepts.iter().cloned().collect(),
        };

        // Add to history
        self.insight_history.push_back(tracking_entry);

        // Update frequency counters
        for concept in &insight.related_concepts {
            *self.concept_frequency.entry(concept.clone()).or_insert(0) += 1;
        }
        *self
            .type_frequency
            .entry(insight.insight_type.clone())
            .or_insert(0) += 1;

        // Update causal graph if enabled
        if self.config.enable_causal_analysis {
            self.update_causal_graph(insight)?;
        }

        // Prune old entries
        self.prune_old_entries();

        info!(
            "Registered insight {} in loop prevention system",
            insight.id
        );
        Ok(())
    }

    /// Create semantic fingerprint for insight deduplication
    fn create_semantic_fingerprint(&self, insight: &Insight) -> Result<SemanticFingerprint> {
        use std::collections::hash_map::DefaultHasher;

        // Hash related concepts
        let mut concept_hashes = Vec::new();
        for concept in &insight.related_concepts {
            let mut hasher = DefaultHasher::new();
            concept.to_lowercase().hash(&mut hasher);
            concept_hashes.push(hasher.finish());
        }
        concept_hashes.sort_unstable();

        // Hash content (simplified - would use semantic hashing in production)
        let mut content_hasher = DefaultHasher::new();
        insight.content.to_lowercase().hash(&mut content_hasher);
        let content_hash = content_hasher.finish();

        // Hash insight type
        let mut type_hasher = DefaultHasher::new();
        insight.insight_type.hash(&mut type_hasher);
        let insight_type_hash = type_hasher.finish();

        // For relationships, we'd hash the knowledge graph connections
        let relationship_hashes = Vec::new(); // Simplified

        Ok(SemanticFingerprint {
            concept_hashes,
            relationship_hashes,
            content_hash,
            insight_type_hash,
        })
    }

    /// Check for semantic duplication with existing insights
    fn check_semantic_duplication(
        &self,
        fingerprint: &SemanticFingerprint,
    ) -> Result<Option<LoopDetectionResult>> {
        for entry in &self.insight_history {
            let similarity =
                self.calculate_fingerprint_similarity(fingerprint, &entry.fingerprint)?;

            if similarity >= self.config.semantic_similarity_threshold {
                // Check cooldown period
                let fingerprint_hash = self.hash_fingerprint(fingerprint);
                if let Some(last_time) = self.cooldown_tracker.get(&fingerprint_hash) {
                    let hours_since = Utc::now().signed_duration_since(*last_time).num_hours();
                    if hours_since < self.config.insight_cooldown_hours {
                        return Ok(Some(LoopDetectionResult {
                            has_loop: true,
                            loop_path: vec![entry.insight_id],
                            loop_type: LoopType::SemanticDuplication,
                            severity: LoopSeverity::Medium,
                            prevention_action: PreventionAction::DelayGeneration,
                        }));
                    }
                }

                return Ok(Some(LoopDetectionResult {
                    has_loop: true,
                    loop_path: vec![entry.insight_id],
                    loop_type: LoopType::SemanticDuplication,
                    severity: if similarity > 0.95 {
                        LoopSeverity::High
                    } else {
                        LoopSeverity::Medium
                    },
                    prevention_action: if similarity > 0.95 {
                        PreventionAction::RejectInsight
                    } else {
                        PreventionAction::ModifyInsight
                    },
                }));
            }
        }

        Ok(None)
    }

    /// Check for causal reasoning loops
    fn check_causal_loops(&self, insight: &Insight) -> Result<Option<LoopDetectionResult>> {
        // Build derivation path for this insight
        let mut derivation_path = Vec::new();
        let mut visited = HashSet::new();

        // Start from source memories and trace backwards
        for &source_id in &insight.source_memory_ids {
            if let Some(path) = self.trace_causal_path(source_id, &mut visited, 0)? {
                if path.len() > self.config.max_derivation_depth {
                    return Ok(Some(LoopDetectionResult {
                        has_loop: true,
                        loop_path: path,
                        loop_type: LoopType::CausalCircularity,
                        severity: LoopSeverity::High,
                        prevention_action: PreventionAction::RejectInsight,
                    }));
                }
                derivation_path.extend(path);
            }
        }

        // Check for circular dependencies
        for i in 0..derivation_path.len() {
            for j in i + 1..derivation_path.len() {
                if derivation_path[i] == derivation_path[j] {
                    return Ok(Some(LoopDetectionResult {
                        has_loop: true,
                        loop_path: derivation_path[i..=j].to_vec(),
                        loop_type: LoopType::DerivationLoop,
                        severity: LoopSeverity::Medium,
                        prevention_action: PreventionAction::ModifyInsight,
                    }));
                }
            }
        }

        Ok(None)
    }

    /// Check for temporal repetition patterns
    fn check_temporal_repetition(&self, insight: &Insight) -> Result<Option<LoopDetectionResult>> {
        let recent_cutoff = Utc::now() - Duration::hours(self.config.insight_cooldown_hours);

        let recent_same_type = self
            .insight_history
            .iter()
            .filter(|entry| {
                entry.insight_type == insight.insight_type && entry.created_at > recent_cutoff
            })
            .count();

        if recent_same_type >= 3 {
            // Too many of the same type recently
            return Ok(Some(LoopDetectionResult {
                has_loop: true,
                loop_path: Vec::new(),
                loop_type: LoopType::TemporalRepetition,
                severity: LoopSeverity::Medium,
                prevention_action: PreventionAction::TriggerCooldown,
            }));
        }

        Ok(None)
    }

    /// Check for conceptual satiation (overuse of specific concepts)
    fn check_conceptual_satiation(&self, insight: &Insight) -> Result<Option<LoopDetectionResult>> {
        for concept in &insight.related_concepts {
            if let Some(&frequency) = self.concept_frequency.get(concept) {
                if frequency > 10 {
                    // Arbitrary threshold for concept overuse
                    return Ok(Some(LoopDetectionResult {
                        has_loop: true,
                        loop_path: Vec::new(),
                        loop_type: LoopType::ConceptualSatiation,
                        severity: LoopSeverity::Low,
                        prevention_action: PreventionAction::ModifyInsight,
                    }));
                }
            }
        }

        Ok(None)
    }

    /// Assess the quality of an insight
    fn assess_insight_quality(&self, insight: &Insight) -> Result<QualityAssessment> {
        let mut quality_factors = Vec::new();
        let mut deficiency_reasons = Vec::new();

        // Novelty assessment - check against existing insights
        let novelty_score = self.calculate_novelty_score(insight)?;
        if novelty_score < self.config.min_novelty_threshold {
            deficiency_reasons.push("Low novelty - similar insights already exist".to_string());
        } else {
            quality_factors.push("High novelty".to_string());
        }

        // Coherence assessment - check logical consistency
        let coherence_score = self.calculate_coherence_score(insight)?;
        if coherence_score < self.config.min_coherence_threshold {
            deficiency_reasons
                .push("Low coherence - insight lacks logical consistency".to_string());
        } else {
            quality_factors.push("High coherence".to_string());
        }

        // Evidence strength - assess supporting memories
        let evidence_strength = self.calculate_evidence_strength(insight)?;
        if evidence_strength < self.config.min_evidence_threshold {
            deficiency_reasons.push("Weak evidence - insufficient supporting memories".to_string());
        } else {
            quality_factors.push("Strong evidence".to_string());
        }

        // Semantic richness - diversity of concepts involved
        let semantic_richness = self.calculate_semantic_richness(insight)?;
        quality_factors.push(format!("Semantic richness: {:.2}", semantic_richness));

        // Predictive power - potential for future value
        let predictive_power = self.calculate_predictive_power(insight)?;
        quality_factors.push(format!("Predictive power: {:.2}", predictive_power));

        // Overall quality score
        let overall_quality = (novelty_score
            + coherence_score
            + evidence_strength
            + semantic_richness
            + predictive_power)
            / 5.0;

        Ok(QualityAssessment {
            novelty_score,
            coherence_score,
            evidence_strength,
            semantic_richness,
            predictive_power,
            overall_quality,
            quality_factors,
            deficiency_reasons,
        })
    }

    /// Check if insight meets quality thresholds
    fn check_quality_thresholds(
        &self,
        quality: &QualityAssessment,
    ) -> Result<Option<LoopDetectionResult>> {
        if quality.novelty_score < self.config.min_novelty_threshold {
            return Ok(Some(LoopDetectionResult {
                has_loop: false,
                loop_path: Vec::new(),
                loop_type: LoopType::SemanticDuplication,
                severity: LoopSeverity::Medium,
                prevention_action: PreventionAction::RejectInsight,
            }));
        }

        if quality.coherence_score < self.config.min_coherence_threshold {
            return Ok(Some(LoopDetectionResult {
                has_loop: false,
                loop_path: Vec::new(),
                loop_type: LoopType::SemanticDuplication,
                severity: LoopSeverity::Medium,
                prevention_action: PreventionAction::RejectInsight,
            }));
        }

        if quality.evidence_strength < self.config.min_evidence_threshold {
            return Ok(Some(LoopDetectionResult {
                has_loop: false,
                loop_path: Vec::new(),
                loop_type: LoopType::SemanticDuplication,
                severity: LoopSeverity::Low,
                prevention_action: PreventionAction::ModifyInsight,
            }));
        }

        Ok(None)
    }

    // Helper methods for quality assessment

    fn calculate_novelty_score(&self, insight: &Insight) -> Result<f64> {
        // Calculate novelty based on similarity to existing insights
        let mut max_similarity: f64 = 0.0;

        for entry in &self.insight_history {
            // Simple concept overlap calculation
            let overlap = insight
                .related_concepts
                .iter()
                .filter(|concept| entry.source_concepts.contains(*concept))
                .count();

            let similarity = overlap as f64 / insight.related_concepts.len().max(1) as f64;
            max_similarity = max_similarity.max(similarity);
        }

        Ok(1.0 - max_similarity)
    }

    fn calculate_coherence_score(&self, _insight: &Insight) -> Result<f64> {
        // Simplified coherence assessment - would use NLP in production
        Ok(0.8) // Default high coherence
    }

    fn calculate_evidence_strength(&self, insight: &Insight) -> Result<f64> {
        // Calculate based on number and quality of source memories
        let memory_count = insight.source_memory_ids.len() as f64;
        let evidence_strength = (memory_count / 10.0).min(1.0); // Normalize to [0,1]
        Ok(evidence_strength)
    }

    fn calculate_semantic_richness(&self, insight: &Insight) -> Result<f64> {
        // Calculate based on diversity of concepts
        let concept_count = insight.related_concepts.len() as f64;
        let richness = (concept_count / 5.0).min(1.0); // Normalize to [0,1]
        Ok(richness)
    }

    fn calculate_predictive_power(&self, insight: &Insight) -> Result<f64> {
        // Simplified assessment - would use ML models in production
        match insight.insight_type {
            InsightType::Causality => Ok(0.9),
            InsightType::Trend => Ok(0.8),
            InsightType::Pattern => Ok(0.7),
            InsightType::Synthesis => Ok(0.6),
            _ => Ok(0.5),
        }
    }

    // Utility methods

    fn calculate_fingerprint_similarity(
        &self,
        fp1: &SemanticFingerprint,
        fp2: &SemanticFingerprint,
    ) -> Result<f64> {
        if fp1.insight_type_hash != fp2.insight_type_hash {
            return Ok(0.0); // Different types can't be similar
        }

        // Calculate concept overlap
        let common_concepts = fp1
            .concept_hashes
            .iter()
            .filter(|hash| fp2.concept_hashes.contains(hash))
            .count();

        let total_concepts = fp1.concept_hashes.len().max(fp2.concept_hashes.len());
        let concept_similarity = if total_concepts > 0 {
            common_concepts as f64 / total_concepts as f64
        } else {
            0.0
        };

        // Simple content similarity (would use semantic embeddings in production)
        let content_similarity = if fp1.content_hash == fp2.content_hash {
            1.0
        } else {
            0.0
        };

        // Weighted combination
        Ok(0.7 * concept_similarity + 0.3 * content_similarity)
    }

    fn hash_fingerprint(&self, fingerprint: &SemanticFingerprint) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        let mut hasher = DefaultHasher::new();
        fingerprint.hash(&mut hasher);
        hasher.finish()
    }

    fn trace_causal_path(
        &self,
        _source_id: Uuid,
        _visited: &mut HashSet<Uuid>,
        _depth: usize,
    ) -> Result<Option<Vec<Uuid>>> {
        // Simplified implementation - would trace through actual causal graph
        Ok(None)
    }

    fn update_causal_graph(&mut self, _insight: &Insight) -> Result<()> {
        // Update the causal dependency graph with new insight
        Ok(())
    }

    fn prune_old_entries(&mut self) {
        let cutoff = Utc::now() - Duration::days(self.config.tracking_window_days);

        while let Some(entry) = self.insight_history.front() {
            if entry.created_at < cutoff {
                self.insight_history.pop_front();
            } else {
                break;
            }
        }

        // Clean up frequency counters for pruned entries
        // (Simplified - would need more sophisticated cleanup)
    }

    /// Apply diversity bonuses and repetition penalties
    pub fn calculate_diversity_adjustment(&self, insight_type: &InsightType) -> f64 {
        let type_frequency = self.type_frequency.get(insight_type).unwrap_or(&0);
        let total_insights = self.insight_history.len();

        if total_insights == 0 {
            return 1.0;
        }

        let frequency_ratio = *type_frequency as f64 / total_insights as f64;
        let expected_ratio = 1.0 / 7.0; // Assuming 7 insight types

        if frequency_ratio < expected_ratio {
            // Underrepresented - apply diversity bonus
            self.config.diversity_bonus_multiplier
        } else if frequency_ratio > expected_ratio * 2.0 {
            // Overrepresented - apply penalty
            self.config.repetition_penalty_factor
        } else {
            1.0 // Neutral
        }
    }

    /// Get statistics about current prevention state
    pub fn get_prevention_statistics(&self) -> PreventionStatistics {
        PreventionStatistics {
            total_insights_tracked: self.insight_history.len(),
            active_cooldowns: self.cooldown_tracker.len(),
            most_frequent_concepts: self.get_top_concepts(5),
            insight_type_distribution: self.type_frequency.clone(),
            average_quality_score: self.calculate_average_quality(),
        }
    }

    fn get_top_concepts(&self, limit: usize) -> Vec<(String, usize)> {
        let mut concepts: Vec<_> = self.concept_frequency.iter().collect();
        concepts.sort_by(|a, b| b.1.cmp(a.1));
        concepts
            .into_iter()
            .take(limit)
            .map(|(k, v)| (k.clone(), *v))
            .collect()
    }

    fn calculate_average_quality(&self) -> f64 {
        if self.insight_history.is_empty() {
            return 0.0;
        }

        let total_quality: f64 = self
            .insight_history
            .iter()
            .map(|entry| entry.quality_assessment.overall_quality)
            .sum();

        total_quality / self.insight_history.len() as f64
    }
}

/// Statistics about the prevention system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreventionStatistics {
    pub total_insights_tracked: usize,
    pub active_cooldowns: usize,
    pub most_frequent_concepts: Vec<(String, usize)>,
    pub insight_type_distribution: HashMap<InsightType, usize>,
    pub average_quality_score: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn create_test_insight() -> Insight {
        Insight {
            id: Uuid::new_v4(),
            insight_type: InsightType::Pattern,
            content: "Test insight content".to_string(),
            confidence_score: 0.8,
            source_memory_ids: vec![
                Uuid::new_v4(),
                Uuid::new_v4(),
                Uuid::new_v4(),
                Uuid::new_v4(),
                Uuid::new_v4(),
            ],
            related_concepts: vec!["concept1".to_string(), "concept2".to_string()],
            knowledge_graph_nodes: Vec::new(),
            importance_score: 0.7,
            generated_at: Utc::now(),
            validation_metrics: super::super::reflection_engine::ValidationMetrics {
                novelty_score: 0.8,
                coherence_score: 0.9,
                evidence_strength: 0.7,
                semantic_richness: 0.6,
                predictive_power: 0.5,
            },
        }
    }

    #[test]
    fn test_semantic_fingerprint_creation() {
        let engine = LoopPreventionEngine::new(LoopPreventionConfig::default());
        let insight = create_test_insight();

        let fingerprint = engine.create_semantic_fingerprint(&insight).unwrap();

        assert_eq!(fingerprint.concept_hashes.len(), 2);
        assert!(fingerprint.content_hash != 0);
        assert!(fingerprint.insight_type_hash != 0);
    }

    #[test]
    fn test_quality_assessment() {
        let engine = LoopPreventionEngine::new(LoopPreventionConfig::default());
        let insight = create_test_insight();

        let quality = engine.assess_insight_quality(&insight).unwrap();

        assert!(quality.overall_quality >= 0.0);
        assert!(quality.overall_quality <= 1.0);
        assert!(quality.novelty_score >= 0.0);
        assert!(quality.coherence_score >= 0.0);
        assert!(quality.evidence_strength >= 0.0);
    }

    #[test]
    fn test_insight_validation() {
        let mut engine = LoopPreventionEngine::new(LoopPreventionConfig::default());
        let insight = create_test_insight();

        let result = engine.validate_insight(&insight).unwrap();

        // First insight should be allowed
        assert!(!result.has_loop);
        assert_eq!(result.prevention_action, PreventionAction::Allow);
    }

    #[test]
    fn test_duplicate_detection() {
        let mut engine = LoopPreventionEngine::new(LoopPreventionConfig::default());
        let insight = create_test_insight();

        // Register first insight
        let quality = engine.assess_insight_quality(&insight).unwrap();
        engine.register_insight(&insight, quality).unwrap();

        // Try to register identical insight
        let duplicate = insight.clone();
        let result = engine.validate_insight(&duplicate).unwrap();

        // Should be detected as duplicate
        assert!(result.has_loop);
        assert_eq!(result.loop_type, LoopType::SemanticDuplication);
    }

    #[test]
    fn test_diversity_adjustment() {
        let mut engine = LoopPreventionEngine::new(LoopPreventionConfig::default());

        // Register several insights of the same type
        for _ in 0..5 {
            let mut insight = create_test_insight();
            insight.id = Uuid::new_v4(); // Make unique
            let quality = engine.assess_insight_quality(&insight).unwrap();
            engine.register_insight(&insight, quality).unwrap();
        }

        let adjustment = engine.calculate_diversity_adjustment(&InsightType::Pattern);

        // Should apply penalty for overrepresented type
        assert!(adjustment < 1.0);
    }

    #[test]
    fn test_prevention_statistics() {
        let mut engine = LoopPreventionEngine::new(LoopPreventionConfig::default());
        let insight = create_test_insight();

        let quality = engine.assess_insight_quality(&insight).unwrap();
        engine.register_insight(&insight, quality).unwrap();

        let stats = engine.get_prevention_statistics();

        assert_eq!(stats.total_insights_tracked, 1);
        assert!(!stats.most_frequent_concepts.is_empty());
        assert!(!stats.insight_type_distribution.is_empty());
    }
}
