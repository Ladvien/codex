pub mod connection;
pub mod consolidation_job;
pub mod error;
pub mod importance_assessment;
pub mod importance_assessment_config;
pub mod math_engine;
pub mod models;
pub mod repository;
pub mod simple_consolidation;

// Cognitive enhancement modules
pub mod cognitive_consolidation;
pub mod cognitive_memory_system;
pub mod insight_loop_prevention;
pub mod reflection_engine;
pub mod three_component_scoring;

pub use consolidation_job::{
    spawn_consolidation_job, ConsolidationJob, ConsolidationJobConfig, ConsolidationJobResult,
    ConsolidationPerformanceMetrics,
};
pub use error::MemoryError;
pub use math_engine::{MathEngine, MathEngineConfig, MemoryParameters};
pub use models::{Memory, MemoryStatus, MemoryTier};
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

// Importance assessment exports
pub use importance_assessment::{
    AssessmentStage, ImportanceAssessmentConfig, ImportanceAssessmentError,
    ImportanceAssessmentPipeline, ImportanceAssessmentResult, ImportancePattern,
    PipelineStatistics, ReferenceEmbedding, Stage1Config, Stage2Config, Stage3Config, StageDetails,
    StageResult,
};
pub use importance_assessment_config::ImportanceAssessmentConfigLoader;
