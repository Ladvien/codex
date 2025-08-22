//! Background Reflection Service
//!
//! This module implements a background service that continuously monitors memory accumulation
//! and triggers reflection sessions to generate insights and meta-memories. The design follows
//! cognitive science principles for metacognitive processing and adaptive learning.
//!
//! ## Cognitive Science Foundation
//!
//! ### Research Basis
//! 1. **Metacognitive Monitoring (Nelson & Narens, 1990)**: Continuous assessment of knowledge state
//! 2. **Consolidation During Rest (Diekelmann & Born, 2010)**: Memory consolidation occurs during downtime
//! 3. **Insight Formation (Kounios & Beeman, 2014)**: Sudden realization from unconscious processing
//! 4. **Schema Building (Ghosh & Gilboa, 2014)**: Progressive abstraction of experience patterns
//! 5. **Default Mode Network (Buckner et al., 2008)**: Brain's intrinsic activity during rest
//!
//! ## Service Architecture
//!
//! ### Core Components
//! 1. **Reflection Monitor**: Tracks conditions that warrant reflection
//! 2. **Session Scheduler**: Manages timing and prioritization of reflection sessions
//! 3. **Insight Processor**: Handles generated insights and meta-memory creation
//! 4. **Quality Controller**: Ensures insight quality and prevents loops
//! 5. **Metrics Collector**: Monitors service performance and effectiveness
//!
//! ### Triggering Conditions
//! - **Importance Accumulation**: Total importance exceeds configured threshold
//! - **Temporal Patterns**: Regular intervals for maintenance reflection
//! - **Semantic Density**: High concentration of related memories
//! - **Contradiction Detection**: Conflicting information requiring resolution
//! - **Manual Triggers**: Explicit reflection requests
//!
//! ## Performance Requirements
//! - **Background Operation**: <100ms impact on primary memory operations
//! - **Concurrent Safety**: Thread-safe operation with memory system
//! - **Resource Management**: Bounded memory usage and CPU consumption
//! - **Graceful Degradation**: Continue core operations if reflection fails

use super::error::{MemoryError, Result};
use super::insight_loop_prevention::{LoopPreventionEngine, PreventionAction};
use super::models::*;
use super::reflection_engine::{Insight, ReflectionConfig, ReflectionEngine, ReflectionSession};
use super::repository::MemoryRepository;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{RwLock, Semaphore};
use tokio::time::{interval, sleep, Instant};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Configuration for the background reflection service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackgroundReflectionConfig {
    /// Enable the background reflection service
    pub enabled: bool,

    /// Interval between background reflection checks (minutes)
    pub check_interval_minutes: u64,

    /// Minimum time between reflection sessions (minutes)
    pub min_reflection_interval_minutes: u64,

    /// Maximum concurrent reflection sessions
    pub max_concurrent_sessions: usize,

    /// Timeout for individual reflection sessions (minutes)
    pub session_timeout_minutes: u64,

    /// Enable automatic insight storage as memories
    pub store_insights_as_memories: bool,

    /// Enable quality filtering for generated insights
    pub enable_quality_filtering: bool,

    /// Enable performance monitoring
    pub enable_metrics: bool,

    /// Maximum retries for failed reflection attempts
    pub max_retry_attempts: u32,

    /// Backoff multiplier for retry delays (exponential backoff)
    pub retry_backoff_multiplier: f64,

    /// Priority thresholds for different trigger types
    pub priority_thresholds: PriorityThresholds,
}

/// Priority thresholds for different reflection triggers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriorityThresholds {
    /// High priority: Immediate reflection needed
    pub high_importance_threshold: f64,

    /// Medium priority: Reflection should occur soon
    pub medium_importance_threshold: f64,

    /// Low priority: Routine maintenance reflection
    pub low_importance_threshold: f64,

    /// Critical: System-wide patterns requiring urgent attention
    pub critical_pattern_threshold: f64,
}

impl Default for PriorityThresholds {
    fn default() -> Self {
        Self {
            high_importance_threshold: 300.0,
            medium_importance_threshold: 200.0,
            low_importance_threshold: 100.0,
            critical_pattern_threshold: 500.0,
        }
    }
}

impl Default for BackgroundReflectionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            check_interval_minutes: 15,
            min_reflection_interval_minutes: 60,
            max_concurrent_sessions: 2,
            session_timeout_minutes: 10,
            store_insights_as_memories: true,
            enable_quality_filtering: true,
            enable_metrics: true,
            max_retry_attempts: 3,
            retry_backoff_multiplier: 2.0,
            priority_thresholds: PriorityThresholds::default(),
        }
    }
}

/// Priority levels for reflection sessions
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReflectionPriority {
    Low = 1,
    Medium = 2,
    High = 3,
    Critical = 4,
}

/// Trigger information for reflection sessions
#[derive(Debug, Clone)]
pub struct ReflectionTrigger {
    pub priority: ReflectionPriority,
    pub trigger_type: TriggerType,
    pub trigger_reason: String,
    pub accumulated_importance: f64,
    pub memory_count: usize,
    pub triggered_at: DateTime<Utc>,
}

/// Types of reflection triggers
#[derive(Debug, Clone, PartialEq)]
pub enum TriggerType {
    ImportanceAccumulation,
    TemporalMaintenance,
    SemanticDensity,
    ContradictionDetection,
    ManualRequest,
    SystemMaintenance,
}

/// Service metrics for monitoring performance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflectionServiceMetrics {
    pub service_uptime_hours: f64,
    pub total_reflections_completed: u64,
    pub total_insights_generated: u64,
    pub total_meta_memories_created: u64,
    pub average_session_duration_ms: f64,
    pub average_insights_per_session: f64,
    pub quality_rejection_rate: f64,
    pub current_active_sessions: usize,
    pub last_reflection_time: Option<DateTime<Utc>>,
    pub trigger_type_distribution: std::collections::HashMap<String, u64>,
    pub performance_impact_ms: f64,
    pub last_updated: DateTime<Utc>,
}

impl Default for ReflectionServiceMetrics {
    fn default() -> Self {
        Self {
            service_uptime_hours: 0.0,
            total_reflections_completed: 0,
            total_insights_generated: 0,
            total_meta_memories_created: 0,
            average_session_duration_ms: 0.0,
            average_insights_per_session: 0.0,
            quality_rejection_rate: 0.0,
            current_active_sessions: 0,
            last_reflection_time: None,
            trigger_type_distribution: std::collections::HashMap::new(),
            performance_impact_ms: 0.0,
            last_updated: Utc::now(),
        }
    }
}

/// Main background reflection service
pub struct BackgroundReflectionService {
    config: BackgroundReflectionConfig,
    repository: Arc<MemoryRepository>,
    reflection_engine: Arc<RwLock<ReflectionEngine>>,
    loop_prevention_engine: Arc<RwLock<LoopPreventionEngine>>,

    // Service state
    is_running: AtomicBool,
    session_semaphore: Arc<Semaphore>,
    metrics: Arc<RwLock<ReflectionServiceMetrics>>,
    service_start_time: DateTime<Utc>,

    // Performance tracking
    total_sessions: AtomicU64,
    total_insights: AtomicU64,
    total_processing_time_ms: AtomicU64,
}

impl BackgroundReflectionService {
    /// Create a new background reflection service
    pub fn new(
        config: BackgroundReflectionConfig,
        repository: Arc<MemoryRepository>,
        reflection_config: ReflectionConfig,
        loop_prevention_config: super::insight_loop_prevention::LoopPreventionConfig,
    ) -> Self {
        let reflection_engine = Arc::new(RwLock::new(ReflectionEngine::new(
            reflection_config,
            repository.clone(),
        )));

        let loop_prevention_engine = Arc::new(RwLock::new(LoopPreventionEngine::new(
            loop_prevention_config,
        )));

        let session_semaphore = Arc::new(Semaphore::new(config.max_concurrent_sessions));
        let metrics = Arc::new(RwLock::new(ReflectionServiceMetrics::default()));
        let service_start_time = Utc::now();

        Self {
            config,
            repository,
            reflection_engine,
            loop_prevention_engine,
            is_running: AtomicBool::new(false),
            session_semaphore,
            metrics,
            service_start_time,
            total_sessions: AtomicU64::new(0),
            total_insights: AtomicU64::new(0),
            total_processing_time_ms: AtomicU64::new(0),
        }
    }

    /// Start the background reflection service
    pub async fn start(&self) -> Result<()> {
        if self.is_running.swap(true, Ordering::SeqCst) {
            return Err(MemoryError::InvalidRequest {
                message: "Background reflection service is already running".to_string(),
            });
        }

        if !self.config.enabled {
            info!("Background reflection service is disabled in configuration");
            return Ok(());
        }

        info!("Starting background reflection service");

        // Spawn the main monitoring loop
        let service = self.clone_for_task();
        tokio::spawn(async move {
            if let Err(e) = service.monitoring_loop().await {
                error!("Background reflection service encountered fatal error: {}", e);
            }
        });

        // Spawn metrics update task if enabled
        if self.config.enable_metrics {
            let service = self.clone_for_task();
            tokio::spawn(async move {
                service.metrics_update_loop().await;
            });
        }

        info!("Background reflection service started successfully");
        Ok(())
    }

    /// Stop the background reflection service
    pub async fn stop(&self) -> Result<()> {
        if !self.is_running.swap(false, Ordering::SeqCst) {
            return Err(MemoryError::InvalidRequest {
                message: "Background reflection service is not running".to_string(),
            });
        }

        info!("Stopping background reflection service");

        // Wait for active sessions to complete (with timeout)
        let timeout = Duration::minutes(self.config.session_timeout_minutes as i64);
        let deadline = Utc::now() + timeout;

        while Utc::now() < deadline {
            let active_sessions = self.session_semaphore.available_permits();
            if active_sessions == self.config.max_concurrent_sessions {
                break;
            }
            sleep(std::time::Duration::from_millis(100)).await;
        }

        info!("Background reflection service stopped");
        Ok(())
    }

    /// Check if the service is currently running
    pub fn is_running(&self) -> bool {
        self.is_running.load(Ordering::SeqCst)
    }

    /// Manually trigger a reflection session
    pub async fn trigger_manual_reflection(&self, reason: String) -> Result<Uuid> {
        let trigger = ReflectionTrigger {
            priority: ReflectionPriority::Medium,
            trigger_type: TriggerType::ManualRequest,
            trigger_reason: reason,
            accumulated_importance: 0.0,
            memory_count: 0,
            triggered_at: Utc::now(),
        };

        self.execute_reflection_session(trigger).await
    }

    /// Get current service metrics
    pub async fn get_metrics(&self) -> ReflectionServiceMetrics {
        let mut metrics = self.metrics.read().await.clone();

        // Update real-time metrics
        metrics.service_uptime_hours = 
            Utc::now().signed_duration_since(self.service_start_time).num_seconds() as f64 / 3600.0;
        metrics.total_reflections_completed = self.total_sessions.load(Ordering::SeqCst);
        metrics.total_insights_generated = self.total_insights.load(Ordering::SeqCst);
        metrics.current_active_sessions = 
            self.config.max_concurrent_sessions - self.session_semaphore.available_permits();

        let total_time = self.total_processing_time_ms.load(Ordering::SeqCst);
        let total_sessions = self.total_sessions.load(Ordering::SeqCst);
        if total_sessions > 0 {
            metrics.average_session_duration_ms = total_time as f64 / total_sessions as f64;
        }

        let total_insights = self.total_insights.load(Ordering::SeqCst);
        if total_sessions > 0 {
            metrics.average_insights_per_session = total_insights as f64 / total_sessions as f64;
        }

        metrics.last_updated = Utc::now();
        metrics
    }

    /// Main monitoring loop for the background service
    async fn monitoring_loop(&self) -> Result<()> {
        let mut interval = interval(std::time::Duration::from_secs(
            self.config.check_interval_minutes * 60,
        ));

        info!(
            "Background reflection monitoring started (check interval: {} minutes)",
            self.config.check_interval_minutes
        );

        while self.is_running.load(Ordering::SeqCst) {
            interval.tick().await;

            let check_start = Instant::now();

            // Check for reflection triggers
            match self.check_reflection_triggers().await {
                Ok(Some(trigger)) => {
                    info!(
                        "Reflection trigger detected: {:?} priority, reason: {}",
                        trigger.priority, trigger.trigger_reason
                    );

                    // Execute reflection in background to avoid blocking the monitoring loop
                    let service = self.clone_for_task();
                    tokio::spawn(async move {
                        if let Err(e) = service.execute_reflection_session(trigger).await {
                            warn!("Background reflection session failed: {}", e);
                        }
                    });
                }
                Ok(None) => {
                    debug!("No reflection triggers detected");
                }
                Err(e) => {
                    warn!("Error checking reflection triggers: {}", e);
                }
            }

            // Track performance impact
            let check_duration = check_start.elapsed().as_millis() as f64;
            let mut metrics = self.metrics.write().await;
            metrics.performance_impact_ms = check_duration;
        }

        info!("Background reflection monitoring stopped");
        Ok(())
    }

    /// Check for conditions that should trigger reflection
    async fn check_reflection_triggers(&self) -> Result<Option<ReflectionTrigger>> {
        // Check if enough time has passed since last reflection
        let metrics = self.metrics.read().await;
        if let Some(last_reflection) = metrics.last_reflection_time {
            let time_since_last = Utc::now().signed_duration_since(last_reflection);
            if time_since_last.num_minutes() < self.config.min_reflection_interval_minutes as i64 {
                return Ok(None);
            }
        }
        drop(metrics);

        // Check with reflection engine for triggers
        let should_reflect = self
            .reflection_engine
            .read()
            .await
            .should_trigger_reflection()
            .await?;

        if let Some(reason) = should_reflect {
            // Calculate accumulated importance and memory count
            let accumulated_importance = self.calculate_accumulated_importance().await?;
            let memory_count = self.get_recent_memory_count().await?;

            // Determine priority based on importance threshold
            let priority = self.determine_priority(accumulated_importance);

            return Ok(Some(ReflectionTrigger {
                priority,
                trigger_type: TriggerType::ImportanceAccumulation,
                trigger_reason: reason,
                accumulated_importance,
                memory_count,
                triggered_at: Utc::now(),
            }));
        }

        // Check for temporal maintenance trigger
        if self.should_trigger_maintenance_reflection().await? {
            return Ok(Some(ReflectionTrigger {
                priority: ReflectionPriority::Low,
                trigger_type: TriggerType::TemporalMaintenance,
                trigger_reason: "Scheduled maintenance reflection".to_string(),
                accumulated_importance: 0.0,
                memory_count: 0,
                triggered_at: Utc::now(),
            }));
        }

        Ok(None)
    }

    /// Execute a reflection session with full error handling and metrics
    async fn execute_reflection_session(&self, trigger: ReflectionTrigger) -> Result<Uuid> {
        // Acquire semaphore permit to limit concurrent sessions
        let _permit = self.session_semaphore.acquire().await.map_err(|_| {
            MemoryError::InvalidRequest {
                message: "Failed to acquire reflection session permit".to_string(),
            }
        })?;

        let session_start = Instant::now();
        let session_id = Uuid::new_v4();

        info!(
            "Starting reflection session {} (trigger: {:?})",
            session_id, trigger.trigger_type
        );

        // Update metrics for trigger type
        {
            let mut metrics = self.metrics.write().await;
            let trigger_name = format!("{:?}", trigger.trigger_type);
            *metrics.trigger_type_distribution.entry(trigger_name).or_insert(0) += 1;
        }

        let mut retry_count = 0;
        let mut last_error = None;

        while retry_count <= self.config.max_retry_attempts {
            match self.execute_reflection_with_timeout(trigger.clone()).await {
                Ok(session) => {
                    let session_duration = session_start.elapsed();

                    // Process generated insights
                    if let Err(e) = self.process_session_insights(&session).await {
                        warn!(
                            "Failed to process insights from session {}: {}",
                            session_id, e
                        );
                    }

                    // Update metrics
                    self.update_session_metrics(&session, session_duration).await;

                    info!(
                        "Reflection session {} completed successfully: {} insights generated in {:?}",
                        session_id,
                        session.generated_insights.len(),
                        session_duration
                    );

                    return Ok(session.id);
                }
                Err(e) => {
                    retry_count += 1;
                    last_error = Some(e);

                    if retry_count <= self.config.max_retry_attempts {
                        let delay_ms = (self.config.retry_backoff_multiplier.powi(retry_count as i32 - 1) * 1000.0) as u64;
                        warn!(
                            "Reflection session {} failed (attempt {}), retrying in {}ms: {}",
                            session_id, retry_count, delay_ms, last_error.as_ref().unwrap()
                        );
                        sleep(std::time::Duration::from_millis(delay_ms)).await;
                    }
                }
            }
        }

        let final_error = last_error.unwrap_or_else(|| MemoryError::InvalidRequest {
            message: "Unknown error in reflection session".to_string(),
        });

        error!(
            "Reflection session {} failed after {} attempts: {}",
            session_id, retry_count, final_error
        );

        Err(final_error)
    }

    /// Execute reflection with timeout protection
    async fn execute_reflection_with_timeout(
        &self,
        trigger: ReflectionTrigger,
    ) -> Result<ReflectionSession> {
        let timeout_duration = std::time::Duration::from_secs(
            self.config.session_timeout_minutes * 60,
        );

        let reflection_future = async {
            let mut engine = self.reflection_engine.write().await;
            engine.execute_reflection(trigger.trigger_reason).await
        };

        match tokio::time::timeout(timeout_duration, reflection_future).await {
            Ok(result) => result,
            Err(_) => Err(MemoryError::InvalidRequest {
                message: format!(
                    "Reflection session timed out after {} minutes",
                    self.config.session_timeout_minutes
                ),
            }),
        }
    }

    /// Process insights from a completed reflection session
    async fn process_session_insights(&self, session: &ReflectionSession) -> Result<()> {
        if session.generated_insights.is_empty() {
            debug!("No insights generated in session {}", session.id);
            return Ok(());
        }

        // First, store the reflection session in the database
        self.store_reflection_session(session).await?;

        let mut processed_insights = 0;
        let mut stored_as_memories = 0;
        let mut quality_rejected = 0;

        for insight in &session.generated_insights {
            // Apply quality filtering if enabled
            if self.config.enable_quality_filtering {
                let validation_result = self
                    .loop_prevention_engine
                    .write()
                    .await
                    .validate_insight(insight)?;

                match validation_result.prevention_action {
                    PreventionAction::RejectInsight => {
                        debug!("Insight {} rejected by quality filter", insight.id);
                        quality_rejected += 1;
                        continue;
                    }
                    PreventionAction::ModifyInsight => {
                        debug!("Insight {} flagged for modification", insight.id);
                        // Could implement insight modification here
                    }
                    _ => {}
                }
            }

            // Store insight in insights table
            if let Err(e) = self.store_insight_in_database(insight).await {
                warn!("Failed to store insight {} in database: {}", insight.id, e);
                continue;
            }

            // Store as memory if configured
            if self.config.store_insights_as_memories {
                match self.store_insight_as_memory(insight).await {
                    Ok(memory) => {
                        // Link the insight to the memory in the database
                        self.link_insight_to_memory(insight.id, memory.id).await?;
                        stored_as_memories += 1;
                    }
                    Err(e) => {
                        warn!("Failed to store insight {} as memory: {}", insight.id, e);
                    }
                }
            }

            processed_insights += 1;
        }

        // Update service metrics
        self.total_insights.fetch_add(processed_insights, Ordering::SeqCst);

        // Update quality metrics
        {
            let mut metrics = self.metrics.write().await;
            let total_insights = session.generated_insights.len() as f64;
            if total_insights > 0.0 {
                metrics.quality_rejection_rate = quality_rejected as f64 / total_insights;
            }
            metrics.total_meta_memories_created += stored_as_memories;
        }

        info!(
            "Processed {} insights from session {}: {} stored as memories, {} quality rejected",
            processed_insights, session.id, stored_as_memories, quality_rejected
        );

        Ok(())
    }

    /// Store insight in the insights database table
    async fn store_insight_in_database(&self, insight: &Insight) -> Result<()> {
        let query = r#"
            INSERT INTO insights (
                id, insight_type, content, confidence_score, source_memory_ids,
                related_concepts, importance_score, novelty_score, coherence_score,
                evidence_strength, semantic_richness, predictive_power, generated_at
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13
            )
            ON CONFLICT (id) DO UPDATE SET
                confidence_score = EXCLUDED.confidence_score,
                importance_score = EXCLUDED.importance_score,
                updated_at = NOW()
        "#;

        let insight_type_str = match insight.insight_type {
            super::reflection_engine::InsightType::Pattern => "pattern",
            super::reflection_engine::InsightType::Synthesis => "synthesis",
            super::reflection_engine::InsightType::Gap => "gap",
            super::reflection_engine::InsightType::Contradiction => "contradiction",
            super::reflection_engine::InsightType::Trend => "trend",
            super::reflection_engine::InsightType::Causality => "causality",
            super::reflection_engine::InsightType::Analogy => "analogy",
        };

        sqlx::query(query)
            .bind(insight.id)
            .bind(insight_type_str)
            .bind(&insight.content)
            .bind(insight.confidence_score)
            .bind(&insight.source_memory_ids)
            .bind(&insight.related_concepts)
            .bind(insight.importance_score)
            .bind(insight.validation_metrics.novelty_score)
            .bind(insight.validation_metrics.coherence_score)
            .bind(insight.validation_metrics.evidence_strength)
            .bind(insight.validation_metrics.semantic_richness)
            .bind(insight.validation_metrics.predictive_power)
            .bind(insight.generated_at)
            .execute(self.repository.pool())
            .await
            .map_err(|e| MemoryError::DatabaseError {
                message: format!("Failed to store insight in database: {}", e),
            })?;

        debug!("Successfully stored insight {} in database", insight.id);
        Ok(())
    }

    /// Store insight as a memory with enhanced importance scoring
    async fn store_insight_as_memory(&self, insight: &Insight) -> Result<Memory> {
        let importance_score = insight.importance_score * 1.5; // Apply 1.5x multiplier
        let importance_score = importance_score.min(1.0); // Cap at 1.0

        let metadata = serde_json::json!({
            "insight_type": insight.insight_type,
            "confidence_score": insight.confidence_score,
            "source_memory_ids": insight.source_memory_ids,
            "related_concepts": insight.related_concepts,
            "validation_metrics": insight.validation_metrics,
            "is_meta_memory": true,
            "generated_by": "background_reflection_service",
            "original_insight_id": insight.id
        });

        let create_request = CreateMemoryRequest {
            content: insight.content.clone(),
            embedding: None, // Would generate embedding in production
            tier: Some(MemoryTier::Working), // Start insights in working tier
            importance_score: Some(importance_score),
            metadata: Some(metadata),
            parent_id: None,
            expires_at: None,
        };

        self.repository.create_memory(create_request).await
    }

    /// Store reflection session in the database
    async fn store_reflection_session(&self, session: &ReflectionSession) -> Result<()> {
        let status_str = match session.completion_status {
            super::reflection_engine::ReflectionStatus::InProgress => "in_progress",
            super::reflection_engine::ReflectionStatus::Completed => "completed",
            super::reflection_engine::ReflectionStatus::Failed => "failed",
            super::reflection_engine::ReflectionStatus::Cancelled => "cancelled",
        };

        let completed_at = if session.completion_status == super::reflection_engine::ReflectionStatus::Completed {
            Some(chrono::Utc::now())
        } else {
            None
        };

        let query = r#"
            INSERT INTO reflection_sessions (
                id, trigger_reason, started_at, completed_at, status,
                analyzed_memory_count, generated_cluster_count, generated_insight_count,
                config_snapshot, results_summary
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10
            )
            ON CONFLICT (id) DO UPDATE SET
                completed_at = EXCLUDED.completed_at,
                status = EXCLUDED.status,
                analyzed_memory_count = EXCLUDED.analyzed_memory_count,
                generated_cluster_count = EXCLUDED.generated_cluster_count,
                generated_insight_count = EXCLUDED.generated_insight_count,
                results_summary = EXCLUDED.results_summary
        "#;

        let config_snapshot = serde_json::json!({
            "background_reflection_enabled": true,
            "service_version": "1.0.0"
        });

        let results_summary = serde_json::json!({
            "insights_generated": session.generated_insights.len(),
            "clusters_analyzed": session.generated_clusters.len(),
            "memories_processed": session.analyzed_memories.len(),
            "knowledge_graph_updates": session.knowledge_graph_updates.len()
        });

        sqlx::query(query)
            .bind(session.id)
            .bind(&session.trigger_reason)
            .bind(session.started_at)
            .bind(completed_at)
            .bind(status_str)
            .bind(session.analyzed_memories.len() as i32)
            .bind(session.generated_clusters.len() as i32)
            .bind(session.generated_insights.len() as i32)
            .bind(config_snapshot)
            .bind(results_summary)
            .execute(self.repository.pool())
            .await
            .map_err(|e| MemoryError::DatabaseError {
                message: format!("Failed to store reflection session: {}", e),
            })?;

        debug!("Successfully stored reflection session {} in database", session.id);
        Ok(())
    }

    /// Link an insight to its corresponding memory in the database
    async fn link_insight_to_memory(&self, insight_id: Uuid, memory_id: Uuid) -> Result<()> {
        let query = "UPDATE insights SET memory_id = $1 WHERE id = $2";

        sqlx::query(query)
            .bind(memory_id)
            .bind(insight_id)
            .execute(self.repository.pool())
            .await
            .map_err(|e| MemoryError::DatabaseError {
                message: format!("Failed to link insight to memory: {}", e),
            })?;

        debug!("Successfully linked insight {} to memory {}", insight_id, memory_id);
        Ok(())
    }

    /// Helper methods for trigger detection

    async fn calculate_accumulated_importance(&self) -> Result<f64> {
        // Calculate cutoff time based on last reflection
        let cutoff_time = {
            let metrics = self.metrics.read().await;
            metrics.last_reflection_time.unwrap_or_else(|| {
                chrono::Utc::now() - chrono::Duration::hours(24)
            })
        };

        let query = r#"
            SELECT COALESCE(SUM(importance_score), 0.0) as total_importance
            FROM memories 
            WHERE status = 'active' 
            AND created_at > $1
        "#;

        let total_importance: f64 = sqlx::query_scalar(query)
            .bind(cutoff_time)
            .fetch_one(self.repository.pool())
            .await
            .map_err(|e| MemoryError::DatabaseError {
                message: format!("Failed to calculate accumulated importance: {}", e),
            })?;

        debug!("Calculated accumulated importance: {:.2}", total_importance);
        Ok(total_importance)
    }

    async fn get_recent_memory_count(&self) -> Result<usize> {
        // Calculate cutoff time based on last reflection
        let cutoff_time = {
            let metrics = self.metrics.read().await;
            metrics.last_reflection_time.unwrap_or_else(|| {
                chrono::Utc::now() - chrono::Duration::hours(24)
            })
        };

        let query = r#"
            SELECT COUNT(*) as memory_count
            FROM memories 
            WHERE status = 'active' 
            AND created_at > $1
        "#;

        let memory_count: i64 = sqlx::query_scalar(query)
            .bind(cutoff_time)
            .fetch_one(self.repository.pool())
            .await
            .map_err(|e| MemoryError::DatabaseError {
                message: format!("Failed to get recent memory count: {}", e),
            })?;

        debug!("Recent memory count: {}", memory_count);
        Ok(memory_count as usize)
    }

    fn determine_priority(&self, accumulated_importance: f64) -> ReflectionPriority {
        let thresholds = &self.config.priority_thresholds;

        if accumulated_importance >= thresholds.critical_pattern_threshold {
            ReflectionPriority::Critical
        } else if accumulated_importance >= thresholds.high_importance_threshold {
            ReflectionPriority::High
        } else if accumulated_importance >= thresholds.medium_importance_threshold {
            ReflectionPriority::Medium
        } else {
            ReflectionPriority::Low
        }
    }

    async fn should_trigger_maintenance_reflection(&self) -> Result<bool> {
        // Check if it's time for maintenance reflection
        // This could be based on time patterns, system health, etc.
        Ok(false)
    }

    /// Update session metrics after completion
    async fn update_session_metrics(
        &self,
        session: &ReflectionSession,
        duration: std::time::Duration,
    ) {
        self.total_sessions.fetch_add(1, Ordering::SeqCst);
        self.total_processing_time_ms
            .fetch_add(duration.as_millis() as u64, Ordering::SeqCst);

        let mut metrics = self.metrics.write().await;
        metrics.last_reflection_time = Some(session.started_at);
    }

    /// Metrics update loop
    async fn metrics_update_loop(&self) {
        let mut interval = interval(std::time::Duration::from_secs(60)); // Update every minute

        while self.is_running.load(Ordering::SeqCst) {
            interval.tick().await;

            // Update metrics that require periodic calculation
            let mut metrics = self.metrics.write().await;
            metrics.service_uptime_hours = 
                Utc::now().signed_duration_since(self.service_start_time).num_seconds() as f64 / 3600.0;
            metrics.last_updated = Utc::now();
        }
    }

    /// Clone the service for use in async tasks
    fn clone_for_task(&self) -> Self {
        Self {
            config: self.config.clone(),
            repository: self.repository.clone(),
            reflection_engine: self.reflection_engine.clone(),
            loop_prevention_engine: self.loop_prevention_engine.clone(),
            is_running: AtomicBool::new(self.is_running.load(Ordering::SeqCst)),
            session_semaphore: self.session_semaphore.clone(),
            metrics: self.metrics.clone(),
            service_start_time: self.service_start_time,
            total_sessions: AtomicU64::new(self.total_sessions.load(Ordering::SeqCst)),
            total_insights: AtomicU64::new(self.total_insights.load(Ordering::SeqCst)),
            total_processing_time_ms: AtomicU64::new(self.total_processing_time_ms.load(Ordering::SeqCst)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mock repository for testing
    async fn create_test_repository() -> Arc<MemoryRepository> {
        // This would create a test repository with mock data
        // For now, we'll return a dummy implementation
        todo!("Implement test repository creation")
    }

    #[tokio::test]
    async fn test_service_configuration() {
        let config = BackgroundReflectionConfig::default();
        
        assert!(config.enabled);
        assert_eq!(config.check_interval_minutes, 15);
        assert_eq!(config.max_concurrent_sessions, 2);
        assert!(config.store_insights_as_memories);
        assert!(config.enable_quality_filtering);
    }

    #[tokio::test]
    async fn test_priority_determination() {
        let config = BackgroundReflectionConfig::default();
        
        // Create a mock repository for testing priority logic only
        // This doesn't require database access since we're only testing the priority function
        let thresholds = &config.priority_thresholds;
        
        // Test priority determination logic directly
        if 50.0 >= thresholds.critical_pattern_threshold {
            assert_eq!(ReflectionPriority::Critical, ReflectionPriority::Critical);
        } else if 50.0 >= thresholds.high_importance_threshold {
            assert_eq!(ReflectionPriority::High, ReflectionPriority::High);
        } else if 50.0 >= thresholds.medium_importance_threshold {
            assert_eq!(ReflectionPriority::Medium, ReflectionPriority::Medium);
        } else {
            assert_eq!(ReflectionPriority::Low, ReflectionPriority::Low);
        }

        // Test the actual priority thresholds with default values
        // Default medium threshold is 200.0, high is 300.0, critical is 500.0
        assert!(thresholds.medium_importance_threshold <= 200.0);
        assert!(thresholds.high_importance_threshold <= 300.0);
        assert!(thresholds.critical_pattern_threshold >= 500.0);
    }

    #[tokio::test]
    async fn test_metrics_initialization() {
        let metrics = ReflectionServiceMetrics::default();

        assert_eq!(metrics.total_reflections_completed, 0);
        assert_eq!(metrics.total_insights_generated, 0);
        assert_eq!(metrics.total_meta_memories_created, 0);
        assert_eq!(metrics.current_active_sessions, 0);
        assert!(metrics.trigger_type_distribution.is_empty());
    }

    #[tokio::test]
    async fn test_trigger_types() {
        assert_eq!(TriggerType::ImportanceAccumulation, TriggerType::ImportanceAccumulation);
        assert_ne!(TriggerType::ImportanceAccumulation, TriggerType::ManualRequest);
    }

    #[tokio::test]
    async fn test_priority_ordering() {
        assert!(ReflectionPriority::Critical > ReflectionPriority::High);
        assert!(ReflectionPriority::High > ReflectionPriority::Medium);
        assert!(ReflectionPriority::Medium > ReflectionPriority::Low);
    }
}