use crate::embedding::EmbeddingService;
use crate::memory::{ImportanceAssessmentPipeline, Memory, MemoryRepository, MemoryTier};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use prometheus::{Counter, Histogram, Registry};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::{Mutex, RwLock};
use tokio::time::{interval, timeout};
use tracing::{debug, error, info, trace, warn};

#[derive(Debug, Error)]
pub enum HarvesterError {
    #[error("Pattern extraction failed: {0}")]
    ExtractionFailed(String),

    #[error("Deduplication failed: {0}")]
    DeduplicationFailed(String),

    #[error("Batch processing failed: {0}")]
    BatchProcessingFailed(String),

    #[error("Repository operation failed: {0}")]
    RepositoryFailed(#[from] crate::memory::error::MemoryError),

    #[error("Importance assessment failed: {0}")]
    ImportanceAssessmentFailed(String),

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Background task failed: {0}")]
    BackgroundTaskFailed(String),
}

/// Types of memory patterns that can be extracted
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MemoryPatternType {
    Preference,
    Fact,
    Decision,
    Correction,
    Emotion,
    Goal,
    Relationship,
    Skill,
}

/// A detected memory pattern with confidence score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedMemoryPattern {
    pub pattern_type: MemoryPatternType,
    pub content: String,
    pub confidence: f64,
    pub extracted_at: DateTime<Utc>,
    pub source_message_id: Option<String>,
    pub context: String,
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Configuration for the silent harvester
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SilentHarvesterConfig {
    /// Auto-store threshold (default: 0.7)
    pub confidence_threshold: f64,

    /// Deduplication similarity threshold (default: 0.85)
    pub deduplication_threshold: f64,

    /// Trigger every N messages (default: 10)
    pub message_trigger_count: usize,

    /// Trigger every N minutes (default: 5)
    pub time_trigger_minutes: u64,

    /// Maximum batch size for processing (default: 50)
    pub max_batch_size: usize,

    /// Performance target: max processing time in seconds (default: 2)
    pub max_processing_time_seconds: u64,

    /// Enable silent mode (no user feedback)
    pub silent_mode: bool,

    /// Pattern extraction configuration
    pub pattern_config: PatternExtractionConfig,
}

impl Default for SilentHarvesterConfig {
    fn default() -> Self {
        Self {
            confidence_threshold: 0.7,
            deduplication_threshold: 0.85,
            message_trigger_count: 10,
            time_trigger_minutes: 5,
            max_batch_size: 50,
            max_processing_time_seconds: 2,
            silent_mode: true,
            pattern_config: PatternExtractionConfig::default(),
        }
    }
}

/// Configuration for pattern extraction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternExtractionConfig {
    pub preference_patterns: Vec<String>,
    pub fact_patterns: Vec<String>,
    pub decision_patterns: Vec<String>,
    pub correction_patterns: Vec<String>,
    pub emotion_patterns: Vec<String>,
    pub goal_patterns: Vec<String>,
    pub relationship_patterns: Vec<String>,
    pub skill_patterns: Vec<String>,
}

impl Default for PatternExtractionConfig {
    fn default() -> Self {
        Self {
            preference_patterns: vec![
                r"(?i)I prefer|I like|I enjoy|I love|I hate|I dislike".to_string(),
                r"(?i)my favorite|I'd rather|I tend to|I usually".to_string(),
                r"(?i)I always|I never|I often|I rarely".to_string(),
            ],
            fact_patterns: vec![
                r"(?i)I am|I work|I live|I have|my name is".to_string(),
                r"(?i)I was born|I graduated|I studied|I learned".to_string(),
                r"(?i)the fact is|it's true that|I know that".to_string(),
            ],
            decision_patterns: vec![
                r"(?i)I decided|I chose|I will|I'm going to".to_string(),
                r"(?i)I've decided|my decision|I'll go with".to_string(),
                r"(?i)I think we should|let's go with|I recommend".to_string(),
            ],
            correction_patterns: vec![
                r"(?i)actually|correction|I meant|let me clarify".to_string(),
                r"(?i)that's wrong|that's incorrect|I misspoke".to_string(),
                r"(?i)sorry, I meant|to be clear|what I should have said".to_string(),
            ],
            emotion_patterns: vec![
                r"(?i)I feel|I'm excited|I'm worried|I'm happy".to_string(),
                r"(?i)I'm frustrated|I'm confused|I'm concerned".to_string(),
                r"(?i)this makes me|I'm feeling|emotionally".to_string(),
            ],
            goal_patterns: vec![
                r"(?i)I want to|I hope to|my goal|I'm trying to".to_string(),
                r"(?i)I'm working toward|I aim to|I plan to".to_string(),
                r"(?i)I need to|I should|I must".to_string(),
            ],
            relationship_patterns: vec![
                r"(?i)my friend|my colleague|my family|my partner".to_string(),
                r"(?i)I work with|I know someone|my relationship".to_string(),
                r"(?i)my team|my boss|my client".to_string(),
            ],
            skill_patterns: vec![
                r"(?i)I can|I'm good at|I know how to|I'm skilled".to_string(),
                r"(?i)I'm learning|I'm studying|I practice".to_string(),
                r"(?i)I'm experienced|I specialize|my expertise".to_string(),
            ],
        }
    }
}

/// Metrics for tracking harvester performance
#[derive(Debug)]
pub struct HarvesterMetrics {
    pub messages_processed: Arc<AtomicU64>,
    pub patterns_extracted: Arc<AtomicU64>,
    pub memories_stored: Arc<AtomicU64>,
    pub duplicates_filtered: Arc<AtomicU64>,
    pub extraction_time_ms: Arc<AtomicU64>,
    pub batch_processing_time_ms: Arc<AtomicU64>,
    pub last_harvest_time: Arc<Mutex<Option<DateTime<Utc>>>>,

    // Prometheus metrics
    pub extraction_counter: Counter,
    pub storage_counter: Counter,
    pub deduplication_counter: Counter,
    pub processing_time_histogram: Histogram,
    pub batch_size_histogram: Histogram,
    pub confidence_histogram: Histogram,
}

impl HarvesterMetrics {
    pub fn new(registry: &Registry) -> Result<Self> {
        let extraction_counter = Counter::new(
            "harvester_patterns_extracted_total",
            "Total number of patterns extracted",
        )?;
        registry.register(Box::new(extraction_counter.clone()))?;

        let storage_counter = Counter::new(
            "harvester_memories_stored_total",
            "Total number of memories stored",
        )?;
        registry.register(Box::new(storage_counter.clone()))?;

        let deduplication_counter = Counter::new(
            "harvester_duplicates_filtered_total",
            "Total number of duplicates filtered out",
        )?;
        registry.register(Box::new(deduplication_counter.clone()))?;

        let processing_time_histogram = Histogram::with_opts(
            prometheus::HistogramOpts::new(
                "harvester_processing_duration_seconds",
                "Time spent processing messages",
            )
            .buckets(vec![0.001, 0.01, 0.1, 0.5, 1.0, 2.0, 5.0]),
        )?;
        registry.register(Box::new(processing_time_histogram.clone()))?;

        let batch_size_histogram = Histogram::with_opts(
            prometheus::HistogramOpts::new("harvester_batch_size", "Size of processing batches")
                .buckets(vec![1.0, 5.0, 10.0, 25.0, 50.0, 100.0]),
        )?;
        registry.register(Box::new(batch_size_histogram.clone()))?;

        let confidence_histogram = Histogram::with_opts(
            prometheus::HistogramOpts::new(
                "harvester_pattern_confidence",
                "Confidence scores of extracted patterns",
            )
            .buckets(vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0]),
        )?;
        registry.register(Box::new(confidence_histogram.clone()))?;

        Ok(Self {
            messages_processed: Arc::new(AtomicU64::new(0)),
            patterns_extracted: Arc::new(AtomicU64::new(0)),
            memories_stored: Arc::new(AtomicU64::new(0)),
            duplicates_filtered: Arc::new(AtomicU64::new(0)),
            extraction_time_ms: Arc::new(AtomicU64::new(0)),
            batch_processing_time_ms: Arc::new(AtomicU64::new(0)),
            last_harvest_time: Arc::new(Mutex::new(None)),
            extraction_counter,
            storage_counter,
            deduplication_counter,
            processing_time_histogram,
            batch_size_histogram,
            confidence_histogram,
        })
    }

    pub async fn record_harvest(&self, patterns_count: u64, processing_time_ms: u64) {
        self.patterns_extracted
            .fetch_add(patterns_count, Ordering::Relaxed);
        self.extraction_time_ms
            .fetch_add(processing_time_ms, Ordering::Relaxed);
        *self.last_harvest_time.lock().await = Some(Utc::now());

        self.extraction_counter.inc_by(patterns_count as f64);
        self.processing_time_histogram
            .observe(processing_time_ms as f64 / 1000.0);
    }

    pub async fn record_storage(&self, stored_count: u64, duplicates_count: u64) {
        self.memories_stored
            .fetch_add(stored_count, Ordering::Relaxed);
        self.duplicates_filtered
            .fetch_add(duplicates_count, Ordering::Relaxed);

        self.storage_counter.inc_by(stored_count as f64);
        self.deduplication_counter.inc_by(duplicates_count as f64);
    }

    pub fn record_batch_processing(&self, batch_size: usize, processing_time_ms: u64) {
        self.batch_processing_time_ms
            .fetch_add(processing_time_ms, Ordering::Relaxed);
        self.batch_size_histogram.observe(batch_size as f64);
    }

    pub fn record_pattern_confidence(&self, confidence: f64) {
        self.confidence_histogram.observe(confidence);
    }
}

/// Pattern matcher for extracting specific types of memories
pub struct PatternMatcher {
    preference_regexes: Vec<Regex>,
    fact_regexes: Vec<Regex>,
    decision_regexes: Vec<Regex>,
    correction_regexes: Vec<Regex>,
    emotion_regexes: Vec<Regex>,
    goal_regexes: Vec<Regex>,
    relationship_regexes: Vec<Regex>,
    skill_regexes: Vec<Regex>,
}

impl PatternMatcher {
    pub fn new(config: &PatternExtractionConfig) -> Result<Self> {
        let compile_patterns = |patterns: &[String]| -> Result<Vec<Regex>> {
            patterns
                .iter()
                .map(|p| Regex::new(p).context("Failed to compile regex pattern"))
                .collect()
        };

        Ok(Self {
            preference_regexes: compile_patterns(&config.preference_patterns)?,
            fact_regexes: compile_patterns(&config.fact_patterns)?,
            decision_regexes: compile_patterns(&config.decision_patterns)?,
            correction_regexes: compile_patterns(&config.correction_patterns)?,
            emotion_regexes: compile_patterns(&config.emotion_patterns)?,
            goal_regexes: compile_patterns(&config.goal_patterns)?,
            relationship_regexes: compile_patterns(&config.relationship_patterns)?,
            skill_regexes: compile_patterns(&config.skill_patterns)?,
        })
    }

    /// Extract all patterns from a message
    pub fn extract_patterns(&self, message: &str, context: &str) -> Vec<ExtractedMemoryPattern> {
        let mut patterns = Vec::new();
        let extracted_at = Utc::now();

        // Extract each pattern type
        patterns.extend(self.extract_pattern_type(
            message,
            context,
            MemoryPatternType::Preference,
            &self.preference_regexes,
            extracted_at,
        ));

        patterns.extend(self.extract_pattern_type(
            message,
            context,
            MemoryPatternType::Fact,
            &self.fact_regexes,
            extracted_at,
        ));

        patterns.extend(self.extract_pattern_type(
            message,
            context,
            MemoryPatternType::Decision,
            &self.decision_regexes,
            extracted_at,
        ));

        patterns.extend(self.extract_pattern_type(
            message,
            context,
            MemoryPatternType::Correction,
            &self.correction_regexes,
            extracted_at,
        ));

        patterns.extend(self.extract_pattern_type(
            message,
            context,
            MemoryPatternType::Emotion,
            &self.emotion_regexes,
            extracted_at,
        ));

        patterns.extend(self.extract_pattern_type(
            message,
            context,
            MemoryPatternType::Goal,
            &self.goal_regexes,
            extracted_at,
        ));

        patterns.extend(self.extract_pattern_type(
            message,
            context,
            MemoryPatternType::Relationship,
            &self.relationship_regexes,
            extracted_at,
        ));

        patterns.extend(self.extract_pattern_type(
            message,
            context,
            MemoryPatternType::Skill,
            &self.skill_regexes,
            extracted_at,
        ));

        patterns
    }

    fn extract_pattern_type(
        &self,
        message: &str,
        context: &str,
        pattern_type: MemoryPatternType,
        regexes: &[Regex],
        extracted_at: DateTime<Utc>,
    ) -> Vec<ExtractedMemoryPattern> {
        let mut patterns = Vec::new();

        for regex in regexes {
            for mat in regex.find_iter(message) {
                // Extract the sentence containing the match
                let content = self.extract_sentence_with_match(message, mat.start(), mat.end());

                // Calculate confidence based on pattern strength and context
                let confidence =
                    self.calculate_pattern_confidence(&pattern_type, &content, context);

                let mut metadata = HashMap::new();
                metadata.insert(
                    "match_start".to_string(),
                    serde_json::Value::Number(mat.start().into()),
                );
                metadata.insert(
                    "match_end".to_string(),
                    serde_json::Value::Number(mat.end().into()),
                );
                metadata.insert(
                    "matched_text".to_string(),
                    serde_json::Value::String(mat.as_str().to_string()),
                );

                patterns.push(ExtractedMemoryPattern {
                    pattern_type: pattern_type.clone(),
                    content,
                    confidence,
                    extracted_at,
                    source_message_id: None, // Will be set by caller
                    context: context.to_string(),
                    metadata,
                });
            }
        }

        patterns
    }

    fn extract_sentence_with_match(&self, text: &str, start: usize, end: usize) -> String {
        // Find sentence boundaries around the match
        let before = &text[..start];
        let after = &text[end..];

        // Find start of sentence (look for . ! ? or start of text)
        let sentence_start = before
            .rfind(|c| c == '.' || c == '!' || c == '?')
            .map(|pos| pos + 1)
            .unwrap_or(0);

        // Find end of sentence (look for . ! ? or end of text)
        let sentence_end = after
            .find(|c| c == '.' || c == '!' || c == '?')
            .map(|pos| end + pos + 1)
            .unwrap_or(text.len());

        text[sentence_start..sentence_end].trim().to_string()
    }

    fn calculate_pattern_confidence(
        &self,
        pattern_type: &MemoryPatternType,
        content: &str,
        _context: &str,
    ) -> f64 {
        let mut confidence: f64 = 0.5; // Base confidence

        // Adjust based on pattern type specificity
        match pattern_type {
            MemoryPatternType::Preference => confidence += 0.1,
            MemoryPatternType::Fact => confidence += 0.15,
            MemoryPatternType::Decision => confidence += 0.2,
            MemoryPatternType::Correction => confidence += 0.25,
            MemoryPatternType::Emotion => confidence += 0.1,
            MemoryPatternType::Goal => confidence += 0.15,
            MemoryPatternType::Relationship => confidence += 0.1,
            MemoryPatternType::Skill => confidence += 0.15,
        }

        // Adjust based on content length and quality
        if content.len() > 50 {
            confidence += 0.1;
        }
        if content.len() > 100 {
            confidence += 0.1;
        }

        // Adjust based on first-person indicators
        if content.contains("I ") || content.contains("my ") || content.contains("me ") {
            confidence += 0.1;
        }

        // Penalize very short or very long extractions
        if content.len() < 10 {
            confidence -= 0.2;
        }
        if content.len() > 500 {
            confidence -= 0.1;
        }

        // Ensure confidence is within valid range
        confidence.clamp(0.0, 1.0)
    }
}

/// Service for deduplicating extracted patterns
pub struct DeduplicationService {
    threshold: f64,
    embedding_service: Arc<dyn EmbeddingService>,
    recent_embeddings: Arc<RwLock<VecDeque<(String, Vec<f32>)>>>,
    max_cache_size: usize,
}

impl DeduplicationService {
    pub fn new(
        threshold: f64,
        embedding_service: Arc<dyn EmbeddingService>,
        max_cache_size: usize,
    ) -> Self {
        Self {
            threshold,
            embedding_service,
            recent_embeddings: Arc::new(RwLock::new(VecDeque::new())),
            max_cache_size,
        }
    }

    /// Check if a pattern is a duplicate of recent patterns
    pub async fn is_duplicate(&self, pattern: &ExtractedMemoryPattern) -> Result<bool> {
        let embedding = self
            .embedding_service
            .generate_embedding(&pattern.content)
            .await
            .context("Failed to generate embedding for deduplication")?;

        let recent_embeddings = self.recent_embeddings.read().await;

        for (_, cached_embedding) in recent_embeddings.iter() {
            let similarity = self.cosine_similarity(&embedding, cached_embedding);
            if similarity >= self.threshold {
                trace!(
                    "Duplicate pattern detected with similarity {:.3}: '{}'",
                    similarity,
                    pattern.content.chars().take(50).collect::<String>()
                );
                return Ok(true);
            }
        }

        drop(recent_embeddings);

        // Add to cache
        let mut cache = self.recent_embeddings.write().await;
        cache.push_back((pattern.content.clone(), embedding));

        // Maintain cache size
        while cache.len() > self.max_cache_size {
            cache.pop_front();
        }

        Ok(false)
    }

    fn cosine_similarity(&self, a: &[f32], b: &[f32]) -> f64 {
        if a.len() != b.len() {
            return 0.0;
        }

        let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

        if norm_a == 0.0 || norm_b == 0.0 {
            return 0.0;
        }

        (dot_product / (norm_a * norm_b)) as f64
    }
}

/// Message queue for batch processing
#[derive(Debug, Clone)]
pub struct ConversationMessage {
    pub id: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub role: String, // "user" or "assistant"
    pub context: String,
}

/// Core harvesting engine
pub struct HarvestingEngine {
    config: SilentHarvesterConfig,
    pattern_matcher: PatternMatcher,
    deduplication_service: DeduplicationService,
    repository: Arc<MemoryRepository>,
    importance_pipeline: Arc<ImportanceAssessmentPipeline>,
    metrics: Arc<HarvesterMetrics>,
    message_queue: Arc<Mutex<VecDeque<ConversationMessage>>>,
    last_harvest_time: Arc<Mutex<Option<Instant>>>,
}

impl HarvestingEngine {
    pub fn new(
        config: SilentHarvesterConfig,
        repository: Arc<MemoryRepository>,
        importance_pipeline: Arc<ImportanceAssessmentPipeline>,
        embedding_service: Arc<dyn EmbeddingService>,
        metrics: Arc<HarvesterMetrics>,
    ) -> Result<Self> {
        let pattern_matcher = PatternMatcher::new(&config.pattern_config)?;
        let deduplication_service = DeduplicationService::new(
            config.deduplication_threshold,
            embedding_service,
            1000, // Cache size
        );

        Ok(Self {
            config,
            pattern_matcher,
            deduplication_service,
            repository,
            importance_pipeline,
            metrics,
            message_queue: Arc::new(Mutex::new(VecDeque::new())),
            last_harvest_time: Arc::new(Mutex::new(None)),
        })
    }

    /// Add a message to the processing queue
    pub async fn queue_message(&self, message: ConversationMessage) -> Result<()> {
        let mut queue = self.message_queue.lock().await;
        queue.push_back(message);

        // Check if we should trigger processing
        let should_process =
            queue.len() >= self.config.message_trigger_count || self.should_trigger_by_time().await;

        if should_process {
            // Clone the queue contents and clear it
            let messages: Vec<ConversationMessage> = queue.drain(..).collect();
            drop(queue);

            // Process in background
            let engine = self.clone_for_background();
            tokio::spawn(async move {
                if let Err(e) = engine.process_message_batch(messages).await {
                    error!("Background harvest processing failed: {}", e);
                }
            });
        }

        Ok(())
    }

    /// Check if we should trigger processing based on time
    async fn should_trigger_by_time(&self) -> bool {
        let last_harvest = self.last_harvest_time.lock().await;
        match *last_harvest {
            Some(last_time) => {
                let elapsed = last_time.elapsed();
                elapsed >= Duration::from_secs(self.config.time_trigger_minutes * 60)
            }
            None => true, // First run
        }
    }

    /// Process a batch of messages
    pub async fn process_message_batch(&self, messages: Vec<ConversationMessage>) -> Result<()> {
        if messages.is_empty() {
            return Ok(());
        }

        let start_time = Instant::now();
        debug!("Processing batch of {} messages", messages.len());

        // Set processing timeout
        let processing_future = self.process_messages_internal(messages.clone());
        let timeout_duration = Duration::from_secs(self.config.max_processing_time_seconds);

        match timeout(timeout_duration, processing_future).await {
            Ok(result) => {
                let processing_time = start_time.elapsed();
                self.metrics
                    .record_batch_processing(messages.len(), processing_time.as_millis() as u64);

                *self.last_harvest_time.lock().await = Some(Instant::now());

                result
            }
            Err(_) => {
                warn!(
                    "Message batch processing timed out after {:?}",
                    timeout_duration
                );
                Err(HarvesterError::BatchProcessingFailed(
                    "Processing timeout exceeded".to_string(),
                )
                .into())
            }
        }
    }

    async fn process_messages_internal(&self, messages: Vec<ConversationMessage>) -> Result<()> {
        let mut all_patterns = Vec::new();
        let extraction_start = Instant::now();

        // Extract patterns from all messages
        for message in &messages {
            let patterns = self
                .pattern_matcher
                .extract_patterns(&message.content, &message.context);

            for mut pattern in patterns {
                pattern.source_message_id = Some(message.id.clone());
                self.metrics.record_pattern_confidence(pattern.confidence);
                all_patterns.push(pattern);
            }
        }

        let extraction_time = extraction_start.elapsed();
        self.metrics
            .record_harvest(
                all_patterns.len() as u64,
                extraction_time.as_millis() as u64,
            )
            .await;

        if all_patterns.is_empty() {
            debug!("No patterns extracted from message batch");
            return Ok(());
        }

        debug!(
            "Extracted {} patterns from {} messages",
            all_patterns.len(),
            messages.len()
        );

        // Filter patterns by confidence threshold
        let high_confidence_patterns: Vec<ExtractedMemoryPattern> = all_patterns
            .into_iter()
            .filter(|p| p.confidence >= self.config.confidence_threshold)
            .collect();

        if high_confidence_patterns.is_empty() {
            debug!(
                "No patterns met confidence threshold of {}",
                self.config.confidence_threshold
            );
            return Ok(());
        }

        // Deduplicate patterns
        let mut unique_patterns = Vec::new();
        let mut duplicate_count = 0;

        for pattern in high_confidence_patterns {
            match self.deduplication_service.is_duplicate(&pattern).await {
                Ok(is_duplicate) => {
                    if is_duplicate {
                        duplicate_count += 1;
                    } else {
                        unique_patterns.push(pattern);
                    }
                }
                Err(e) => {
                    warn!("Deduplication check failed for pattern: {}", e);
                    // Include the pattern anyway to avoid losing data
                    unique_patterns.push(pattern);
                }
            }
        }

        debug!(
            "After deduplication: {} unique patterns, {} duplicates",
            unique_patterns.len(),
            duplicate_count
        );

        // Store unique patterns as memories
        let mut stored_count = 0;
        for pattern in unique_patterns {
            match self.store_pattern_as_memory(pattern).await {
                Ok(_) => stored_count += 1,
                Err(e) => {
                    warn!("Failed to store pattern as memory: {}", e);
                }
            }
        }

        self.metrics
            .record_storage(stored_count, duplicate_count)
            .await;

        if self.config.silent_mode {
            // Silent operation - only log at debug level
            debug!(
                "Silent harvest completed: {} patterns stored, {} duplicates filtered",
                stored_count, duplicate_count
            );
        } else {
            info!(
                "Memory harvest completed: {} patterns stored, {} duplicates filtered",
                stored_count, duplicate_count
            );
        }

        Ok(())
    }

    async fn store_pattern_as_memory(&self, pattern: ExtractedMemoryPattern) -> Result<Memory> {
        // Create metadata for the memory
        let mut metadata = pattern.metadata.clone();
        metadata.insert(
            "pattern_type".to_string(),
            serde_json::to_value(&pattern.pattern_type)?,
        );
        metadata.insert(
            "extraction_confidence".to_string(),
            serde_json::Value::Number(
                serde_json::Number::from_f64(pattern.confidence)
                    .unwrap_or_else(|| serde_json::Number::from(0)),
            ),
        );
        metadata.insert(
            "extracted_at".to_string(),
            serde_json::Value::String(pattern.extracted_at.to_rfc3339()),
        );
        if let Some(ref source_id) = pattern.source_message_id {
            metadata.insert(
                "source_message_id".to_string(),
                serde_json::Value::String(source_id.clone()),
            );
        }
        metadata.insert(
            "context".to_string(),
            serde_json::Value::String(pattern.context.clone()),
        );
        metadata.insert(
            "harvester_version".to_string(),
            serde_json::Value::String("1.0".to_string()),
        );

        // Use importance assessment to determine final confidence
        let assessment_result = self
            .importance_pipeline
            .assess_importance(&pattern.content)
            .await
            .map_err(|e| HarvesterError::ImportanceAssessmentFailed(e.to_string()))?;

        let final_importance = assessment_result.importance_score.max(pattern.confidence);

        // Create memory request
        let create_request = crate::memory::models::CreateMemoryRequest {
            content: pattern.content,
            embedding: None,                 // Will be generated by repository
            tier: Some(MemoryTier::Working), // Start in working memory
            importance_score: Some(final_importance),
            metadata: Some(serde_json::Value::Object(metadata.into_iter().collect())),
            parent_id: None,
            expires_at: None,
        };

        // Store the memory
        self.repository
            .create_memory(create_request)
            .await
            .map_err(HarvesterError::RepositoryFailed)
            .map_err(Into::into)
    }

    /// Clone for background processing
    fn clone_for_background(&self) -> HarvestingEngineHandle {
        HarvestingEngineHandle {
            config: self.config.clone(),
            pattern_matcher: PatternMatcher::new(&self.config.pattern_config).unwrap(),
            deduplication_service: DeduplicationService::new(
                self.config.deduplication_threshold,
                // Note: This is a simplified clone - in practice you'd want to share the service
                self.deduplication_service.embedding_service.clone(),
                1000,
            ),
            repository: self.repository.clone(),
            importance_pipeline: self.importance_pipeline.clone(),
            metrics: self.metrics.clone(),
            last_harvest_time: self.last_harvest_time.clone(),
        }
    }

    /// Get current metrics summary
    pub async fn get_metrics_summary(&self) -> HarvesterMetricsSummary {
        HarvesterMetricsSummary {
            messages_processed: self.metrics.messages_processed.load(Ordering::Relaxed),
            patterns_extracted: self.metrics.patterns_extracted.load(Ordering::Relaxed),
            memories_stored: self.metrics.memories_stored.load(Ordering::Relaxed),
            duplicates_filtered: self.metrics.duplicates_filtered.load(Ordering::Relaxed),
            avg_extraction_time_ms: self.metrics.extraction_time_ms.load(Ordering::Relaxed),
            avg_batch_processing_time_ms: self
                .metrics
                .batch_processing_time_ms
                .load(Ordering::Relaxed),
            last_harvest_time: *self.metrics.last_harvest_time.lock().await,
        }
    }

    /// Force immediate harvest of queued messages
    pub async fn force_harvest(&self) -> Result<HarvestResult> {
        let messages = {
            let mut queue = self.message_queue.lock().await;
            queue.drain(..).collect::<Vec<_>>()
        };

        if messages.is_empty() {
            return Ok(HarvestResult {
                messages_processed: 0,
                patterns_extracted: 0,
                patterns_stored: 0,
                duplicates_filtered: 0,
                processing_time_ms: 0,
            });
        }

        let start_time = Instant::now();
        self.process_message_batch(messages.clone()).await?;
        let processing_time = start_time.elapsed();

        Ok(HarvestResult {
            messages_processed: messages.len(),
            patterns_extracted: 0,  // Would need to track during processing
            patterns_stored: 0,     // Would need to track during processing
            duplicates_filtered: 0, // Would need to track during processing
            processing_time_ms: processing_time.as_millis() as u64,
        })
    }
}

/// Simplified handle for background processing
struct HarvestingEngineHandle {
    config: SilentHarvesterConfig,
    pattern_matcher: PatternMatcher,
    deduplication_service: DeduplicationService,
    repository: Arc<MemoryRepository>,
    importance_pipeline: Arc<ImportanceAssessmentPipeline>,
    metrics: Arc<HarvesterMetrics>,
    #[allow(dead_code)] // May be used for future optimizations
    last_harvest_time: Arc<Mutex<Option<Instant>>>,
}

impl HarvestingEngineHandle {
    async fn process_message_batch(&self, messages: Vec<ConversationMessage>) -> Result<()> {
        // Simplified processing logic - reuse the main logic structure
        // This is essentially the same as the main engine's process_messages_internal
        let mut all_patterns = Vec::new();
        let extraction_start = Instant::now();

        // Extract patterns
        for message in &messages {
            let patterns = self
                .pattern_matcher
                .extract_patterns(&message.content, &message.context);

            for mut pattern in patterns {
                pattern.source_message_id = Some(message.id.clone());
                self.metrics.record_pattern_confidence(pattern.confidence);
                all_patterns.push(pattern);
            }
        }

        let extraction_time = extraction_start.elapsed();
        self.metrics
            .record_harvest(
                all_patterns.len() as u64,
                extraction_time.as_millis() as u64,
            )
            .await;

        // Filter by confidence
        let high_confidence_patterns: Vec<ExtractedMemoryPattern> = all_patterns
            .into_iter()
            .filter(|p| p.confidence >= self.config.confidence_threshold)
            .collect();

        // Deduplicate and store
        let mut stored_count = 0;
        let mut duplicate_count = 0;

        for pattern in high_confidence_patterns {
            match self.deduplication_service.is_duplicate(&pattern).await {
                Ok(is_duplicate) => {
                    if is_duplicate {
                        duplicate_count += 1;
                    } else {
                        match self.store_pattern_as_memory(pattern).await {
                            Ok(_) => stored_count += 1,
                            Err(e) => warn!("Failed to store pattern: {}", e),
                        }
                    }
                }
                Err(e) => {
                    warn!("Deduplication check failed: {}", e);
                }
            }
        }

        self.metrics
            .record_storage(stored_count, duplicate_count)
            .await;
        debug!(
            "Background harvest: {} stored, {} duplicates",
            stored_count, duplicate_count
        );

        Ok(())
    }

    async fn store_pattern_as_memory(&self, pattern: ExtractedMemoryPattern) -> Result<Memory> {
        // Create metadata
        let mut metadata = pattern.metadata.clone();
        metadata.insert(
            "pattern_type".to_string(),
            serde_json::to_value(&pattern.pattern_type)?,
        );
        metadata.insert(
            "extraction_confidence".to_string(),
            serde_json::Value::Number(
                serde_json::Number::from_f64(pattern.confidence)
                    .unwrap_or_else(|| serde_json::Number::from(0)),
            ),
        );

        // Use importance assessment
        let assessment_result = self
            .importance_pipeline
            .assess_importance(&pattern.content)
            .await
            .map_err(|e| HarvesterError::ImportanceAssessmentFailed(e.to_string()))?;

        let final_importance = assessment_result.importance_score.max(pattern.confidence);

        // Create and store memory
        let create_request = crate::memory::models::CreateMemoryRequest {
            content: pattern.content,
            embedding: None,
            tier: Some(MemoryTier::Working),
            importance_score: Some(final_importance),
            metadata: Some(serde_json::Value::Object(metadata.into_iter().collect())),
            parent_id: None,
            expires_at: None,
        };

        self.repository
            .create_memory(create_request)
            .await
            .map_err(HarvesterError::RepositoryFailed)
            .map_err(Into::into)
    }
}

/// Summary of harvester metrics
#[derive(Debug, Serialize, Deserialize)]
pub struct HarvesterMetricsSummary {
    pub messages_processed: u64,
    pub patterns_extracted: u64,
    pub memories_stored: u64,
    pub duplicates_filtered: u64,
    pub avg_extraction_time_ms: u64,
    pub avg_batch_processing_time_ms: u64,
    pub last_harvest_time: Option<DateTime<Utc>>,
}

/// Result of a harvest operation
#[derive(Debug, Serialize, Deserialize)]
pub struct HarvestResult {
    pub messages_processed: usize,
    pub patterns_extracted: usize,
    pub patterns_stored: usize,
    pub duplicates_filtered: usize,
    pub processing_time_ms: u64,
}

/// Background task manager for the silent harvester
pub struct SilentHarvesterService {
    engine: Arc<HarvestingEngine>,
    #[allow(dead_code)] // Stored for potential future use
    config: SilentHarvesterConfig,
    _shutdown_tx: tokio::sync::oneshot::Sender<()>,
}

impl SilentHarvesterService {
    pub fn new(
        repository: Arc<MemoryRepository>,
        importance_pipeline: Arc<ImportanceAssessmentPipeline>,
        embedding_service: Arc<dyn EmbeddingService>,
        config: Option<SilentHarvesterConfig>,
        registry: &Registry,
    ) -> Result<Self> {
        let config = config.unwrap_or_default();
        let metrics = Arc::new(HarvesterMetrics::new(registry)?);

        let engine = Arc::new(HarvestingEngine::new(
            config.clone(),
            repository,
            importance_pipeline,
            embedding_service,
            metrics,
        )?);

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

        // Start background task for time-based triggering
        let engine_clone = engine.clone();
        let time_trigger_minutes = config.time_trigger_minutes;

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(time_trigger_minutes * 60));
            let mut shutdown_rx = shutdown_rx;

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if let Err(e) = engine_clone.force_harvest().await {
                            error!("Scheduled harvest failed: {}", e);
                        }
                    }
                    _ = &mut shutdown_rx => {
                        info!("Silent harvester service shutting down");
                        break;
                    }
                }
            }
        });

        Ok(Self {
            engine,
            config,
            _shutdown_tx: shutdown_tx,
        })
    }

    /// Add a message to the harvesting queue
    pub async fn add_message(&self, message: ConversationMessage) -> Result<()> {
        self.engine.queue_message(message).await
    }

    /// Get the harvesting engine for direct access
    pub fn engine(&self) -> &Arc<HarvestingEngine> {
        &self.engine
    }

    /// Force immediate harvest
    pub async fn force_harvest(&self) -> Result<HarvestResult> {
        self.engine.force_harvest().await
    }

    /// Get metrics summary
    pub async fn get_metrics(&self) -> HarvesterMetricsSummary {
        self.engine.get_metrics_summary().await
    }
}
