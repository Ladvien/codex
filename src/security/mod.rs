pub mod audit;
pub mod auth;
pub mod compliance;
pub mod pii;
pub mod rate_limit;
pub mod rbac;
pub mod secrets;
pub mod tls;
pub mod validation;

// Complex integration tests removed for now

pub use audit::*;
pub use auth::*;
pub use compliance::*;
pub use pii::*;
pub use rate_limit::*;
pub use rbac::*;
pub use secrets::*;
pub use tls::*;
pub use validation::*;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Main security configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SecurityConfig {
    /// TLS configuration
    pub tls: TlsConfig,

    /// Authentication configuration
    pub auth: AuthConfig,

    /// Rate limiting configuration
    pub rate_limiting: RateLimitConfig,

    /// Audit logging configuration
    pub audit_logging: AuditConfig,

    /// PII detection and masking configuration
    pub pii_protection: PiiConfig,

    /// RBAC configuration
    pub rbac: RbacConfig,

    /// Secrets management configuration
    pub secrets: SecretsConfig,

    /// GDPR compliance configuration
    pub gdpr: GdprConfig,

    /// Input validation configuration
    pub validation: ValidationConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    pub enabled: bool,
    pub cert_path: PathBuf,
    pub key_path: PathBuf,
    pub port: u16,
    pub require_client_cert: bool,
    pub client_ca_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    pub enabled: bool,
    pub jwt_secret: String,
    pub jwt_expiry_seconds: u64,
    pub api_key_enabled: bool,
    pub mtls_enabled: bool,
    pub session_timeout_minutes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    pub enabled: bool,
    pub requests_per_minute: u32,
    pub burst_size: u32,
    pub per_ip: bool,
    pub per_user: bool,
    pub whitelist_ips: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditConfig {
    pub enabled: bool,
    pub log_all_requests: bool,
    pub log_data_access: bool,
    pub log_modifications: bool,
    pub log_auth_events: bool,
    pub retention_days: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiiConfig {
    pub enabled: bool,
    pub detect_patterns: Vec<String>,
    pub mask_in_logs: bool,
    pub mask_in_responses: bool,
    pub anonymize_storage: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RbacConfig {
    pub enabled: bool,
    pub default_role: String,
    pub roles: HashMap<String, Vec<String>>, // role -> permissions
    pub admin_users: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretsConfig {
    pub vault_enabled: bool,
    pub vault_address: Option<String>,
    pub vault_token_path: Option<PathBuf>,
    pub env_fallback: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GdprConfig {
    pub enabled: bool,
    pub data_retention_days: u32,
    pub auto_cleanup: bool,
    pub consent_required: bool,
    pub right_to_be_forgotten: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationConfig {
    pub enabled: bool,
    pub max_request_size: u64,
    pub sanitize_input: bool,
    pub xss_protection: bool,
    pub sql_injection_protection: bool,
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            cert_path: PathBuf::from("/etc/ssl/certs/codex.crt"),
            key_path: PathBuf::from("/etc/ssl/private/codex.key"),
            port: 8443,
            require_client_cert: false,
            client_ca_path: None,
        }
    }
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            jwt_secret: "change-me-in-production".to_string(),
            jwt_expiry_seconds: 3600, // 1 hour
            api_key_enabled: false,
            mtls_enabled: false,
            session_timeout_minutes: 30,
        }
    }
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            requests_per_minute: 100,
            burst_size: 10,
            per_ip: true,
            per_user: true,
            whitelist_ips: vec!["127.0.0.1".to_string(), "::1".to_string()],
        }
    }
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            log_all_requests: false,
            log_data_access: true,
            log_modifications: true,
            log_auth_events: true,
            retention_days: 90,
        }
    }
}

impl Default for PiiConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            detect_patterns: vec![
                r"\b\d{3}-\d{2}-\d{4}\b".to_string(),                      // SSN
                r"\b[\w\.-]+@[\w\.-]+\.\w+\b".to_string(),                 // Email
                r"\b\d{4}[\s-]?\d{4}[\s-]?\d{4}[\s-]?\d{4}\b".to_string(), // Credit card
            ],
            mask_in_logs: true,
            mask_in_responses: false,
            anonymize_storage: false,
        }
    }
}

impl Default for RbacConfig {
    fn default() -> Self {
        let mut roles = HashMap::new();
        roles.insert("user".to_string(), vec!["read".to_string()]);
        roles.insert(
            "admin".to_string(),
            vec![
                "read".to_string(),
                "write".to_string(),
                "delete".to_string(),
            ],
        );

        Self {
            enabled: false,
            default_role: "user".to_string(),
            roles,
            admin_users: Vec::new(),
        }
    }
}

impl Default for SecretsConfig {
    fn default() -> Self {
        Self {
            vault_enabled: false,
            vault_address: None,
            vault_token_path: None,
            env_fallback: true,
        }
    }
}

impl Default for GdprConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            data_retention_days: 730, // 2 years
            auto_cleanup: false,
            consent_required: false,
            right_to_be_forgotten: false,
        }
    }
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_request_size: 10 * 1024 * 1024, // 10MB
            sanitize_input: true,
            xss_protection: true,
            sql_injection_protection: true,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SecurityError {
    #[error("Authentication failed: {message}")]
    AuthenticationFailed { message: String },

    #[error("Authorization failed: {message}")]
    AuthorizationFailed { message: String },

    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    #[error("TLS error: {message}")]
    TlsError { message: String },

    #[error("Validation error: {message}")]
    ValidationError { message: String },

    #[error("PII detected in content")]
    PiiDetected,

    #[error("GDPR compliance error: {message}")]
    GdprError { message: String },

    #[error("Audit error: {message}")]
    AuditError { message: String },

    #[error("Secrets management error: {message}")]
    SecretsError { message: String },
}

pub type Result<T> = std::result::Result<T, SecurityError>;
