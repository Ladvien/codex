pub mod compression;
pub mod connection;
pub mod consolidation_job;
pub mod enhanced_retrieval;
pub mod error;
pub mod importance_assessment;
pub mod importance_assessment_config;
pub mod math_engine;
pub mod models;
pub mod repository;
pub mod semantic_deduplication;
pub mod simple_consolidation;
pub mod tier_manager;

// Cognitive enhancement modules
pub mod background_reflection_service;
pub mod cognitive_consolidation;
pub mod cognitive_memory_system;
pub mod event_triggers;
pub mod insight_loop_prevention;
pub mod reflection_engine;
pub mod silent_harvester;
pub mod three_component_scoring;
pub mod trigger_config_loader;

pub use compression::{
    CompressionResult as ZstdCompressionResult, CompressionStats, FrozenMemoryCompression,
    MemoryData, StorageSavings, ZstdCompressionEngine,
};
pub use consolidation_job::{
    spawn_consolidation_job, ConsolidationJob, ConsolidationJobConfig, ConsolidationJobResult,
    ConsolidationPerformanceMetrics,
};
pub use error::MemoryError;
pub use math_engine::{MathEngine, MathEngineConfig, MemoryParameters};
pub use models::{
    CreateMemoryRequest, Memory, MemoryStatus, MemoryTier, SearchRequest, SearchType,
};
pub use repository::MemoryRepository;
pub use simple_consolidation::{
    ConsolidationBatchResult, ConsolidationProcessor, SimpleConsolidationConfig,
    SimpleConsolidationEngine, SimpleConsolidationResult,
};

// Cognitive system exports
pub use cognitive_consolidation::{
    CognitiveConsolidationConfig, CognitiveConsolidationEngine, CognitiveConsolidationResult,
    CognitiveFactors, RetrievalContext,
};
pub use cognitive_memory_system::{
    CognitiveFlags, CognitiveMemoryConfig, CognitiveMemoryRequest, CognitiveMemoryResult,
    CognitiveMemorySystem, CognitivePerformanceMetrics,
};
pub use insight_loop_prevention::{
    LoopDetectionResult, LoopPreventionConfig, LoopPreventionEngine, PreventionStatistics,
    QualityAssessment,
};
pub use reflection_engine::{
    Insight, InsightType, KnowledgeGraph, KnowledgeNode, MemoryCluster, ReflectionConfig,
    ReflectionEngine, ReflectionSession,
};
pub use three_component_scoring::{
    EnhancedSearchResult, EnhancedSearchService, ScoringContext, ScoringResult,
    ThreeComponentConfig, ThreeComponentEngine,
};

// Event triggers exports
pub use event_triggers::{
    EventTriggeredScoringEngine, TriggerConfig, TriggerDetectionResult, TriggerEvent,
    TriggerMetrics, TriggerPattern,
};
pub use trigger_config_loader::TriggerConfigLoader;

// Background reflection service exports
pub use background_reflection_service::{
    BackgroundReflectionConfig, BackgroundReflectionService, PriorityThresholds,
    ReflectionPriority, ReflectionServiceMetrics, ReflectionTrigger, TriggerType,
};

// Silent harvester exports
pub use silent_harvester::{
    ConversationMessage, DeduplicationService, ExtractedMemoryPattern, HarvestResult,
    HarvesterError, HarvesterMetrics, HarvesterMetricsSummary, HarvestingEngine, MemoryPatternType,
    PatternExtractionConfig, PatternMatcher, SilentHarvesterConfig, SilentHarvesterService,
};

// Importance assessment exports
pub use importance_assessment::{
    AssessmentStage, ImportanceAssessmentConfig, ImportanceAssessmentError,
    ImportanceAssessmentPipeline, ImportanceAssessmentResult, ImportancePattern,
    PipelineStatistics, ReferenceEmbedding, Stage1Config, Stage2Config, Stage3Config, StageDetails,
    StageResult,
};
pub use importance_assessment_config::ImportanceAssessmentConfigLoader;

// Semantic deduplication exports
pub use semantic_deduplication::{
    AuditEntry, AuditTrail, AutoPruner, CompressionManager, CompressionResult,
    DeduplicationMetrics, DeduplicationResult, GroupMergeResult, HeadroomMaintenanceResult,
    MemoryMerger, MemoryStatistics, MergeResult, MergeStrategy, PruningResult, ReversibleOperation,
    SemanticDeduplicationConfig, SemanticDeduplicationEngine, SimilarMemoryGroup,
};

// Enhanced retrieval exports
pub use enhanced_retrieval::{
    BoostExplanation, ConsolidationEvent, EnhancedRetrievalConfig, MemoryAncestor,
    MemoryAwareRetrievalEngine, MemoryAwareSearchRequest, MemoryAwareSearchResponse,
    MemoryAwareSearchResult, MemoryDescendant, MemoryLineage, PerformanceMetrics,
    ProvenanceMetadata, QueryPatternCache, RelationshipType,
};

// Tier manager exports
pub use tier_manager::{
    TierManager, TierManagerMetrics, TierMigrationBatch, TierMigrationCandidate,
    TierMigrationResult,
};
