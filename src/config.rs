use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::env;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// PostgreSQL database connection URL
    pub database_url: String,

    /// Embedding service configuration
    pub embedding: EmbeddingConfig,

    /// HTTP server port
    pub http_port: u16,

    /// MCP server port (if different from HTTP)
    pub mcp_port: Option<u16>,

    /// Memory tier configuration
    pub tier_config: TierConfig,

    /// Operational settings
    pub operational: OperationalConfig,

    /// Backup and disaster recovery settings
    pub backup: BackupConfiguration,

    /// Security and compliance settings
    pub security: SecurityConfiguration,

    /// Tier manager configuration
    pub tier_manager: TierManagerConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    /// Embedding provider (openai or ollama)
    pub provider: String,

    /// Model name to use for embeddings
    pub model: String,

    /// API key (for OpenAI, empty for Ollama)
    pub api_key: String,

    /// Base URL for the embedding service
    pub base_url: String,

    /// Request timeout in seconds
    pub timeout_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierConfig {
    /// Maximum memories in working tier
    pub working_tier_limit: usize,

    /// Maximum memories in warm tier  
    pub warm_tier_limit: usize,

    /// Days before moving from working to warm
    pub working_to_warm_days: u32,

    /// Days before moving from warm to cold
    pub warm_to_cold_days: u32,

    /// Importance threshold for tier promotion
    pub importance_threshold: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationalConfig {
    /// Maximum database connections
    pub max_db_connections: u32,

    /// Request timeout in seconds
    pub request_timeout_seconds: u64,

    /// Enable metrics endpoint
    pub enable_metrics: bool,

    /// Log level (error, warn, info, debug, trace)
    pub log_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupConfiguration {
    /// Enable automated backups
    pub enabled: bool,

    /// Directory for backup storage
    pub backup_directory: PathBuf,

    /// WAL archive directory
    pub wal_archive_directory: PathBuf,

    /// Backup retention in days
    pub retention_days: u32,

    /// Enable backup encryption
    pub enable_encryption: bool,

    /// Backup schedule (cron format)
    pub schedule: String,

    /// Recovery time objective in minutes
    pub rto_minutes: u32,

    /// Recovery point objective in minutes
    pub rpo_minutes: u32,

    /// Enable backup verification
    pub enable_verification: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            database_url: "postgresql://postgres:postgres@localhost:5432/codex_memory".to_string(),
            embedding: EmbeddingConfig::default(),
            http_port: 8080,
            mcp_port: None,
            tier_config: TierConfig::default(),
            operational: OperationalConfig::default(),
            backup: BackupConfiguration::default(),
            security: SecurityConfiguration::default(),
            tier_manager: TierManagerConfig::default(),
        }
    }
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            provider: "ollama".to_string(),
            model: "nomic-embed-text".to_string(),
            api_key: String::new(),
            base_url: "http://192.168.1.110:11434".to_string(),
            timeout_seconds: 60,
        }
    }
}

impl Default for TierConfig {
    fn default() -> Self {
        Self {
            working_tier_limit: 1000,
            warm_tier_limit: 10000,
            working_to_warm_days: 7,
            warm_to_cold_days: 30,
            importance_threshold: 0.7,
        }
    }
}

impl Default for OperationalConfig {
    fn default() -> Self {
        Self {
            max_db_connections: 10,
            request_timeout_seconds: 30,
            enable_metrics: true,
            log_level: "info".to_string(),
        }
    }
}

impl Config {
    /// Load configuration from environment variables with MCP-friendly error handling
    pub fn from_env() -> Result<Self> {
        dotenv::dotenv().ok(); // Load .env file if present

        // Handle Claude Desktop MCP_ prefixed environment variables
        if let Ok(mcp_db_url) = std::env::var("MCP_DATABASE_URL") {
            std::env::set_var("DATABASE_URL", mcp_db_url);
        }
        if let Ok(mcp_provider) = std::env::var("MCP_EMBEDDING_PROVIDER") {
            std::env::set_var("EMBEDDING_PROVIDER", mcp_provider);
        }
        if let Ok(mcp_model) = std::env::var("MCP_EMBEDDING_MODEL") {
            std::env::set_var("EMBEDDING_MODEL", mcp_model);
        }
        if let Ok(mcp_api_key) = std::env::var("MCP_OPENAI_API_KEY") {
            std::env::set_var("OPENAI_API_KEY", mcp_api_key);
        }
        if let Ok(mcp_ollama_url) = std::env::var("MCP_OLLAMA_BASE_URL") {
            std::env::set_var("OLLAMA_BASE_URL", mcp_ollama_url);
        }
        if let Ok(mcp_log_level) = std::env::var("MCP_LOG_LEVEL") {
            std::env::set_var("RUST_LOG", mcp_log_level);
        }

        let mut config = Config {
            database_url: Self::get_database_url_from_env()
                .map_err(|e| anyhow::anyhow!("Database configuration error: {e}"))?,
            ..Config::default()
        };

        // Embedding configuration
        if let Ok(provider) = env::var("EMBEDDING_PROVIDER") {
            config.embedding.provider = provider;
        }

        if let Ok(model) = env::var("EMBEDDING_MODEL") {
            config.embedding.model = model;
        }

        if let Ok(base_url) = env::var("EMBEDDING_BASE_URL") {
            config.embedding.base_url = base_url;
        }

        if let Ok(timeout) = env::var("EMBEDDING_TIMEOUT_SECONDS") {
            config.embedding.timeout_seconds = timeout
                .parse()
                .map_err(|e| anyhow::anyhow!("Invalid EMBEDDING_TIMEOUT_SECONDS: {}", e))?;
        }

        // API key is optional (not needed for Ollama)
        if let Ok(api_key) = env::var("OPENAI_API_KEY") {
            config.embedding.api_key = api_key;
        }

        // For backward compatibility, also check EMBEDDING_API_KEY
        if let Ok(api_key) = env::var("EMBEDDING_API_KEY") {
            config.embedding.api_key = api_key;
        }

        // Optional environment variables
        if let Ok(port) = env::var("HTTP_PORT") {
            config.http_port = port
                .parse()
                .map_err(|e| anyhow::anyhow!("Invalid HTTP_PORT: {}", e))?;
        }

        if let Ok(port) = env::var("MCP_PORT") {
            config.mcp_port = Some(
                port.parse()
                    .map_err(|e| anyhow::anyhow!("Invalid MCP_PORT: {}", e))?,
            );
        }

        // Tier configuration
        if let Ok(limit) = env::var("WORKING_TIER_LIMIT") {
            config.tier_config.working_tier_limit = limit
                .parse()
                .map_err(|e| anyhow::anyhow!("Invalid WORKING_TIER_LIMIT: {}", e))?;
        }

        if let Ok(limit) = env::var("WARM_TIER_LIMIT") {
            config.tier_config.warm_tier_limit = limit
                .parse()
                .map_err(|e| anyhow::anyhow!("Invalid WARM_TIER_LIMIT: {}", e))?;
        }

        if let Ok(days) = env::var("WORKING_TO_WARM_DAYS") {
            config.tier_config.working_to_warm_days = days
                .parse()
                .map_err(|e| anyhow::anyhow!("Invalid WORKING_TO_WARM_DAYS: {}", e))?;
        }

        if let Ok(days) = env::var("WARM_TO_COLD_DAYS") {
            config.tier_config.warm_to_cold_days = days
                .parse()
                .map_err(|e| anyhow::anyhow!("Invalid WARM_TO_COLD_DAYS: {}", e))?;
        }

        if let Ok(threshold) = env::var("IMPORTANCE_THRESHOLD") {
            config.tier_config.importance_threshold = threshold
                .parse()
                .map_err(|e| anyhow::anyhow!("Invalid IMPORTANCE_THRESHOLD: {}", e))?;
        }

        // Operational configuration
        if let Ok(conns) = env::var("MAX_DB_CONNECTIONS") {
            config.operational.max_db_connections = conns
                .parse()
                .map_err(|e| anyhow::anyhow!("Invalid MAX_DB_CONNECTIONS: {}", e))?;
        }

        if let Ok(timeout) = env::var("REQUEST_TIMEOUT_SECONDS") {
            config.operational.request_timeout_seconds = timeout
                .parse()
                .map_err(|e| anyhow::anyhow!("Invalid REQUEST_TIMEOUT_SECONDS: {}", e))?;
        }

        if let Ok(enable) = env::var("ENABLE_METRICS") {
            config.operational.enable_metrics = enable
                .parse()
                .map_err(|e| anyhow::anyhow!("Invalid ENABLE_METRICS: {}", e))?;
        }

        if let Ok(level) = env::var("LOG_LEVEL") {
            config.operational.log_level = level;
        }

        Ok(config)
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        if self.database_url.is_empty() {
            return Err(anyhow::anyhow!("Database URL is required"));
        }

        // Validate embedding configuration
        match self.embedding.provider.as_str() {
            "openai" => {
                if self.embedding.api_key.is_empty() {
                    return Err(anyhow::anyhow!("API key is required for OpenAI provider"));
                }
            }
            "ollama" => {
                if self.embedding.base_url.is_empty() {
                    return Err(anyhow::anyhow!("Base URL is required for Ollama provider"));
                }
            }
            "mock" => {
                // Mock provider for testing - no additional validation needed
            }
            _ => {
                return Err(anyhow::anyhow!(
                    "Invalid embedding provider: {}. Must be 'openai', 'ollama', or 'mock'",
                    self.embedding.provider
                ));
            }
        }

        if self.embedding.model.is_empty() {
            return Err(anyhow::anyhow!("Embedding model is required"));
        }

        if self.tier_config.working_tier_limit == 0 {
            return Err(anyhow::anyhow!("Working tier limit must be greater than 0"));
        }

        if self.tier_config.warm_tier_limit == 0 {
            return Err(anyhow::anyhow!("Warm tier limit must be greater than 0"));
        }

        if self.tier_config.importance_threshold < 0.0
            || self.tier_config.importance_threshold > 1.0
        {
            return Err(anyhow::anyhow!(
                "Importance threshold must be between 0.0 and 1.0"
            ));
        }

        Ok(())
    }

    /// Get database URL from environment variables with multiple fallback options
    /// for MCP compatibility
    fn get_database_url_from_env() -> Result<String> {
        // Try DATABASE_URL first (standard convention)
        if let Ok(url) = env::var("DATABASE_URL") {
            return Ok(url);
        }

        // Try individual components for MCP-style configuration
        if let (Ok(host), Ok(user), Ok(db)) = (
            env::var("DB_HOST"),
            env::var("DB_USER"),
            env::var("DB_NAME"),
        ) {
            let password = env::var("DB_PASSWORD").unwrap_or_default();
            let port = env::var("DB_PORT").unwrap_or_else(|_| "5432".to_string());

            if password.is_empty() {
                return Ok(format!("postgresql://{user}@{host}:{port}/{db}"));
            } else {
                return Ok(format!("postgresql://{user}:{password}@{host}:{port}/{db}"));
            }
        }

        // Fall back to DB_CONN if provided
        if let Ok(conn) = env::var("DB_CONN") {
            return Ok(conn);
        }

        Err(anyhow::anyhow!(
            "Database credentials not found. Please provide either:\n\
             1. DATABASE_URL environment variable, or\n\
             2. DB_HOST, DB_USER, DB_NAME (and optionally DB_PASSWORD, DB_PORT), or\n\
             3. DB_CONN environment variable\n\n\
             Example:\n\
             DATABASE_URL=postgresql://user:password@localhost:5432/database\n\
             or\n\
             DB_HOST=localhost\n\
             DB_USER=myuser\n\
             DB_PASSWORD=mypassword\n\
             DB_NAME=mydatabase"
        ))
    }

    /// Generate a safe connection string for logging (masks password)
    pub fn safe_database_url(&self) -> String {
        if let Some(at_pos) = self.database_url.find('@') {
            if let Some(colon_pos) = self.database_url[..at_pos].rfind(':') {
                // postgresql://user:password@host:port/db -> postgresql://user:***@host:port/db
                let mut masked = self.database_url.clone();
                masked.replace_range(colon_pos + 1..at_pos, "***");
                return masked;
            }
        }
        // If we can't parse it, just show the prefix
        format!(
            "postgresql://[credentials-hidden]{}",
            self.database_url
                .split_once('@')
                .map(|(_, rest)| rest)
                .unwrap_or("")
        )
    }

    /// Validate MCP environment configuration
    pub fn validate_mcp_environment(&self) -> Result<()> {
        // Standard validation first
        self.validate()?;

        // MCP-specific validations
        if self.embedding.provider == "openai" && self.embedding.api_key.len() < 20 {
            return Err(anyhow::anyhow!(
                "OpenAI API key appears to be invalid (too short)"
            ));
        }

        // Check for reasonable port configuration
        if self.http_port < 1024 {
            tracing::warn!(
                "HTTP port {} requires root privileges. Consider using port >= 1024 for MCP deployment.",
                self.http_port
            );
        }

        if let Some(mcp_port) = self.mcp_port {
            if mcp_port == self.http_port {
                return Err(anyhow::anyhow!(
                    "MCP port and HTTP port cannot be the same ({})",
                    mcp_port
                ));
            }
        }

        Ok(())
    }

    /// Create a diagnostic report for troubleshooting MCP setup
    pub fn create_diagnostic_report(&self) -> String {
        let mut report = String::new();
        report.push_str("=== Agentic Memory System - MCP Configuration Report ===\n\n");

        // Database configuration
        report.push_str("Database Configuration:\n");
        report.push_str(&format!("  Connection: {}\n", self.safe_database_url()));

        // Embedding configuration
        report.push_str("\nEmbedding Configuration:\n");
        report.push_str(&format!("  Provider: {}\n", self.embedding.provider));
        report.push_str(&format!("  Model: {}\n", self.embedding.model));
        report.push_str(&format!("  Base URL: {}\n", self.embedding.base_url));
        report.push_str(&format!("  Timeout: {}s\n", self.embedding.timeout_seconds));
        report.push_str(&format!(
            "  API Key: {}\n",
            if self.embedding.api_key.is_empty() {
                "Not set"
            } else {
                "***configured***"
            }
        ));

        // Server configuration
        report.push_str("\nServer Configuration:\n");
        report.push_str(&format!("  HTTP Port: {}\n", self.http_port));
        report.push_str(&format!(
            "  MCP Port: {}\n",
            self.mcp_port
                .map(|p| p.to_string())
                .unwrap_or_else(|| "Not set".to_string())
        ));

        // Memory tier configuration
        report.push_str("\nMemory Tier Configuration:\n");
        report.push_str(&format!(
            "  Working Tier Limit: {}\n",
            self.tier_config.working_tier_limit
        ));
        report.push_str(&format!(
            "  Warm Tier Limit: {}\n",
            self.tier_config.warm_tier_limit
        ));
        report.push_str(&format!(
            "  Working->Warm: {} days\n",
            self.tier_config.working_to_warm_days
        ));
        report.push_str(&format!(
            "  Warm->Cold: {} days\n",
            self.tier_config.warm_to_cold_days
        ));

        // Validation results
        report.push_str("\nValidation Results:\n");
        match self.validate_mcp_environment() {
            Ok(_) => report.push_str("  ✅ All configuration checks passed\n"),
            Err(e) => report.push_str(&format!("  ❌ Configuration error: {e}\n")),
        }

        report.push_str("\n=== End Configuration Report ===\n");
        report
    }
}

impl Default for BackupConfiguration {
    fn default() -> Self {
        Self {
            enabled: true,
            backup_directory: PathBuf::from("/var/lib/codex/backups"),
            wal_archive_directory: PathBuf::from("/var/lib/codex/wal_archive"),
            retention_days: 30,
            enable_encryption: true,
            schedule: "0 2 * * *".to_string(), // Daily at 2 AM
            rto_minutes: 60,                   // 1 hour
            rpo_minutes: 5,                    // 5 minutes
            enable_verification: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierManagerConfig {
    /// Enable automatic tier management
    pub enabled: bool,

    /// Interval between tier management scans in seconds
    pub scan_interval_seconds: u64,

    /// Batch size for migration operations (memories per batch)
    pub migration_batch_size: usize,

    /// Maximum concurrent migration tasks
    pub max_concurrent_migrations: usize,

    /// Recall probability thresholds for tier migrations
    pub working_to_warm_threshold: f64, // P(r) < 0.7
    pub warm_to_cold_threshold: f64,   // P(r) < 0.5
    pub cold_to_frozen_threshold: f64, // P(r) < 0.2

    /// Minimum age before considering migration (prevents rapid tier changes)
    pub min_working_age_hours: u64,
    pub min_warm_age_hours: u64,
    pub min_cold_age_hours: u64,

    /// Migration performance targets
    pub target_migrations_per_second: u32,

    /// Enable migration history logging
    pub log_migrations: bool,

    /// Migration failure retry configuration
    pub max_retry_attempts: u32,
    pub retry_delay_seconds: u64,

    /// Enable metrics collection for migration monitoring
    pub enable_metrics: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfiguration {
    /// Enable security features
    pub enabled: bool,
    /// TLS configuration
    pub tls_enabled: bool,
    pub tls_cert_path: PathBuf,
    pub tls_key_path: PathBuf,
    pub tls_port: u16,
    /// Authentication configuration
    pub auth_enabled: bool,
    pub jwt_secret: String,
    pub jwt_expiry_hours: u32,
    pub api_key_enabled: bool,
    /// Rate limiting configuration
    pub rate_limiting_enabled: bool,
    pub requests_per_minute: u32,
    pub rate_limit_burst: u32,
    /// Audit logging configuration
    pub audit_enabled: bool,
    pub audit_retention_days: u32,
    /// PII detection configuration
    pub pii_detection_enabled: bool,
    pub pii_mask_logs: bool,
    /// GDPR compliance configuration
    pub gdpr_enabled: bool,
    pub gdpr_retention_days: u32,
    pub right_to_be_forgotten: bool,
    /// Secrets management configuration
    pub vault_enabled: bool,
    pub vault_address: Option<String>,
    pub vault_token_path: Option<PathBuf>,
    /// Input validation configuration
    pub input_validation_enabled: bool,
    pub max_request_size_mb: u32,
}

impl Default for SecurityConfiguration {
    fn default() -> Self {
        Self {
            enabled: true,
            tls_enabled: false,
            tls_cert_path: PathBuf::from("/etc/ssl/certs/codex.crt"),
            tls_key_path: PathBuf::from("/etc/ssl/private/codex.key"),
            tls_port: 8443,
            auth_enabled: true,
            jwt_secret: "change-me-in-production".to_string(),
            jwt_expiry_hours: 24,
            api_key_enabled: false,
            rate_limiting_enabled: false,
            requests_per_minute: 100,
            rate_limit_burst: 20,
            audit_enabled: false,
            audit_retention_days: 90,
            pii_detection_enabled: false,
            pii_mask_logs: true,
            gdpr_enabled: false,
            gdpr_retention_days: 730, // 2 years
            right_to_be_forgotten: false,
            vault_enabled: false,
            vault_address: None,
            vault_token_path: None,
            input_validation_enabled: true,
            max_request_size_mb: 10,
        }
    }
}

impl Default for TierManagerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            scan_interval_seconds: 300, // 5 minutes - frequent enough for responsive tier management
            migration_batch_size: 100,  // Process in batches to avoid long-running transactions
            max_concurrent_migrations: 4, // Balance throughput with resource usage

            // Cognitive research-based thresholds for forgetting curves
            working_to_warm_threshold: 0.7, // HIGH-002 requirement
            warm_to_cold_threshold: 0.5,    // HIGH-002 requirement
            cold_to_frozen_threshold: 0.2,  // HIGH-002 requirement

            // Minimum ages prevent thrashing between tiers
            min_working_age_hours: 1, // At least 1 hour in working memory
            min_warm_age_hours: 24,   // At least 1 day in warm storage
            min_cold_age_hours: 168,  // At least 1 week in cold storage

            // Performance target from HIGH-002
            target_migrations_per_second: 1000,

            log_migrations: true,    // Track for audit and analysis
            max_retry_attempts: 3,   // Reasonable retry policy
            retry_delay_seconds: 60, // 1 minute between retries
            enable_metrics: true,    // Essential for monitoring
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.http_port, 8080);
        assert_eq!(config.embedding.model, "nomic-embed-text");
        assert_eq!(config.embedding.provider, "ollama");
        assert_eq!(config.tier_config.working_tier_limit, 1000);
    }

    #[test]
    fn test_config_validation() {
        let mut config = Config::default();
        // Ollama provider with base_url should be valid
        assert!(config.validate().is_ok());

        // OpenAI provider without API key should fail
        config.embedding.provider = "openai".to_string();
        config.embedding.api_key = String::new();
        assert!(config.validate().is_err());

        // OpenAI provider with API key should pass
        config.embedding.api_key = "test-key".to_string();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_embedding_config_validation() {
        let mut config = Config::default();

        // Invalid provider should fail
        config.embedding.provider = "invalid".to_string();
        assert!(config.validate().is_err());

        // Empty model should fail
        config.embedding.provider = "ollama".to_string();
        config.embedding.model = String::new();
        assert!(config.validate().is_err());

        // Ollama without base_url should fail
        config.embedding.model = "test-model".to_string();
        config.embedding.base_url = String::new();
        assert!(config.validate().is_err());
    }
}
