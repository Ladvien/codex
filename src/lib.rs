pub mod backup;
pub mod config;
pub mod database_setup;
pub mod embedding;
pub mod manager;
pub mod mcp;
pub mod memory;
pub mod monitoring;
pub mod performance;
pub mod security;
pub mod setup;

pub use config::Config;
pub use database_setup::{DatabaseHealth, DatabaseSetup};
pub use embedding::{EmbeddingHealth, EmbeddingModelInfo, SimpleEmbedder};
pub use setup::SetupManager;

// Re-export memory types for convenience
pub use memory::{
    connection::{create_pool, get_pool},
    error::MemoryError,
    Memory, MemoryRepository, MemoryStatus, MemoryTier,
};

// Re-export MCP server
pub use mcp::server::MCPServer;

// Re-export monitoring types
pub use monitoring::{
    AlertManager, HealthChecker, HealthStatus, MetricsCollector, PerformanceProfiler,
    PerformanceSummary, SystemHealth,
};

// Re-export backup types
pub use backup::{
    BackupConfig, BackupEncryption, BackupManager, BackupMetadata, BackupStatus, BackupType,
    BackupVerifier, DisasterRecoveryManager, DisasterType, PointInTimeRecovery, RecoveryOptions,
    WalArchiver,
};

// Re-export security types
pub use security::{
    AuditEvent, AuditEventType, AuditManager, AuthManager, AuthMethod, Claims, ComplianceManager,
    DataSubjectRequest, DataSubjectRequestType, PiiDetectionResult, PiiManager, RateLimitManager,
    RbacManager, SecretsManager, SecurityConfig, SecurityError, TlsManager, UserSession,
    ValidationManager, ValidationManager as InputValidator,
};

// Re-export manager types
pub use manager::{ManagerPaths, ServerManager};
