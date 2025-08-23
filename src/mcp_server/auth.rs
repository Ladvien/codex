//! MCP Authentication Middleware
//!
//! This module provides authentication middleware for MCP requests,
//! supporting API keys, JWT tokens, and certificate-based authentication.

use crate::security::{audit::AuditLogger, SecurityError};
use anyhow::{anyhow, Result};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::env;
use std::sync::Arc;
use tracing::{debug, error, warn};
use uuid::Uuid;

/// JWT claims structure
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String,        // Subject (user ID)
    pub client_id: String,  // Client identifier
    pub scope: Vec<String>, // Permissions/scopes
    pub iat: i64,           // Issued at
    pub exp: i64,           // Expiration time
    pub jti: String,        // JWT ID (for revocation)
}

/// Authentication method types
#[derive(Debug, Clone, PartialEq)]
pub enum AuthMethod {
    ApiKey,
    JwtToken,
    Certificate,
    None,
}

/// Authentication context for validated requests
#[derive(Debug, Clone)]
pub struct AuthContext {
    pub client_id: String,
    pub user_id: String,
    pub method: AuthMethod,
    pub scopes: Vec<String>,
    pub expires_at: Option<chrono::DateTime<Utc>>,
    pub request_id: String,
}

/// MCP Authentication configuration
#[derive(Debug, Clone)]
pub struct MCPAuthConfig {
    pub enabled: bool,
    pub jwt_secret: String,
    pub jwt_expiry_seconds: u64,
    pub api_keys: HashMap<String, ApiKeyInfo>,
    pub allowed_certificates: HashSet<String>,
    pub require_scope: Vec<String>,
    pub performance_target_ms: u64,
}

/// API Key information
#[derive(Debug, Clone)]
pub struct ApiKeyInfo {
    pub client_id: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<chrono::DateTime<Utc>>,
    pub last_used: Option<chrono::DateTime<Utc>>,
    pub usage_count: u64,
}

impl Default for MCPAuthConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            jwt_secret: env::var("MCP_JWT_SECRET").unwrap_or_else(|_| {
                "change-me-in-production-super-secret-key-minimum-32-chars".to_string()
            }),
            jwt_expiry_seconds: env::var("MCP_JWT_EXPIRY_SECONDS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(3600), // 1 hour
            api_keys: Self::load_api_keys_from_env(),
            allowed_certificates: Self::load_certificates_from_env(),
            require_scope: vec!["mcp:read".to_string(), "mcp:write".to_string()],
            performance_target_ms: 5, // Must be <5ms per requirement
        }
    }
}

impl MCPAuthConfig {
    /// Load API keys from environment variables
    fn load_api_keys_from_env() -> HashMap<String, ApiKeyInfo> {
        let mut api_keys = HashMap::new();

        // Load from MCP_API_KEYS environment variable (JSON format)
        if let Ok(keys_json) = env::var("MCP_API_KEYS") {
            match serde_json::from_str::<HashMap<String, Value>>(&keys_json) {
                Ok(keys) => {
                    for (key, info) in keys {
                        if let Ok(client_id) = info
                            .get("client_id")
                            .and_then(|v| v.as_str())
                            .ok_or("Missing client_id")
                        {
                            let scopes = info
                                .get("scopes")
                                .and_then(|v| v.as_array())
                                .map(|arr| {
                                    arr.iter()
                                        .filter_map(|s| s.as_str().map(String::from))
                                        .collect()
                                })
                                .unwrap_or_else(|| {
                                    vec!["mcp:read".to_string(), "mcp:write".to_string()]
                                });

                            let expires_at = info
                                .get("expires_at")
                                .and_then(|v| v.as_str())
                                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                                .map(|dt| dt.with_timezone(&Utc));

                            api_keys.insert(
                                key,
                                ApiKeyInfo {
                                    client_id: client_id.to_string(),
                                    scopes,
                                    expires_at,
                                    last_used: None,
                                    usage_count: 0,
                                },
                            );
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to parse MCP_API_KEYS: {}", e);
                }
            }
        }

        // Fallback: single API key from MCP_API_KEY
        if api_keys.is_empty() {
            if let Ok(api_key) = env::var("MCP_API_KEY") {
                let client_id =
                    env::var("MCP_CLIENT_ID").unwrap_or_else(|_| "default-client".to_string());

                api_keys.insert(
                    api_key,
                    ApiKeyInfo {
                        client_id,
                        scopes: vec!["mcp:read".to_string(), "mcp:write".to_string()],
                        expires_at: None,
                        last_used: None,
                        usage_count: 0,
                    },
                );
            }
        }

        api_keys
    }

    /// Load allowed certificates from environment variables
    fn load_certificates_from_env() -> HashSet<String> {
        let mut certs = HashSet::new();

        if let Ok(cert_thumbprints) = env::var("MCP_ALLOWED_CERTS") {
            for thumbprint in cert_thumbprints.split(',') {
                certs.insert(thumbprint.trim().to_string());
            }
        }

        certs
    }

    /// Create configuration from environment variables
    pub fn from_env() -> Self {
        // In production, authentication should be enabled by default for security
        let is_production = env::var("ENVIRONMENT")
            .unwrap_or_else(|_| "development".to_string())
            .to_lowercase()
            == "production";

        // Default to enabled unless explicitly disabled
        let auth_enabled = env::var("MCP_AUTH_ENABLED")
            .map(|s| s.parse().unwrap_or(true))
            .unwrap_or(true);

        // Warn if authentication is disabled in production
        if is_production && !auth_enabled {
            eprintln!("WARNING: Authentication is disabled in production environment! This is a security risk.");
        }

        Self {
            enabled: auth_enabled,
            ..Self::default()
        }
    }
}

/// MCP Authentication middleware
pub struct MCPAuth {
    config: MCPAuthConfig,
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    audit_logger: Arc<AuditLogger>,
    revoked_tokens: Arc<tokio::sync::RwLock<HashSet<String>>>,
}

impl MCPAuth {
    /// Create a new authentication middleware
    pub fn new(config: MCPAuthConfig, audit_logger: Arc<AuditLogger>) -> Result<Self> {
        let encoding_key = EncodingKey::from_secret(config.jwt_secret.as_bytes());
        let decoding_key = DecodingKey::from_secret(config.jwt_secret.as_bytes());

        Ok(Self {
            config,
            encoding_key,
            decoding_key,
            audit_logger,
            revoked_tokens: Arc::new(tokio::sync::RwLock::new(HashSet::new())),
        })
    }

    /// Authenticate an MCP request
    pub async fn authenticate_request(
        &self,
        method: &str,
        _params: Option<&Value>,
        headers: &HashMap<String, String>,
    ) -> Result<Option<AuthContext>> {
        let start_time = std::time::Instant::now();

        // Validate JSON-RPC version in headers (added by transport layer)
        if let Some(jsonrpc_version) = headers.get("JSON-RPC-Version") {
            if jsonrpc_version != "2.0" {
                return Err(anyhow::anyhow!(
                    "Invalid JSON-RPC version: {}. Expected '2.0'", jsonrpc_version
                ));
            }
        } else {
            // If transport layer didn't set JSON-RPC-Version header, this is a protocol violation
            return Err(anyhow::anyhow!(
                "Missing JSON-RPC version in request headers - protocol violation"
            ));
        }

        // Skip authentication if disabled
        if !self.config.enabled {
            return Ok(None);
        }

        let request_id = Uuid::new_v4().to_string();

        // Determine authentication method and validate
        let auth_result = if let Some(auth_header) = headers.get("authorization") {
            if let Some(token) = auth_header.strip_prefix("Bearer ") {
                self.validate_jwt_token(token, &request_id).await
            } else if let Some(api_key) = auth_header.strip_prefix("ApiKey ") {
                self.validate_api_key(api_key, &request_id).await
            } else {
                Err(anyhow!("Invalid authorization header format"))
            }
        } else if let Some(cert_thumbprint) = headers.get("x-client-cert-thumbprint") {
            self.validate_certificate(cert_thumbprint, &request_id)
                .await
        } else if let Some(api_key) = headers.get("x-api-key") {
            self.validate_api_key(api_key, &request_id).await
        } else {
            Err(anyhow!("No authentication credentials provided"))
        };

        let elapsed = start_time.elapsed();

        // Check performance requirement
        if elapsed.as_millis() > self.config.performance_target_ms as u128 {
            warn!(
                "Authentication took {}ms, exceeding target of {}ms",
                elapsed.as_millis(),
                self.config.performance_target_ms
            );
        }

        match auth_result {
            Ok(context) => {
                debug!(
                    "Authentication successful for client: {}",
                    context.client_id
                );

                // Log successful authentication
                self.audit_logger
                    .log_auth_event(&context.client_id, &context.user_id, method, true, None)
                    .await;

                Ok(Some(context))
            }
            Err(e) => {
                error!("Authentication failed: {}", e);

                // Log failed authentication
                let client_id = headers
                    .get("x-client-id")
                    .or_else(|| headers.get("client-id"))
                    .map(|s| s.as_str())
                    .unwrap_or("unknown");

                self.audit_logger
                    .log_auth_event(client_id, "unknown", method, false, Some(&e.to_string()))
                    .await;

                Err(SecurityError::AuthenticationFailed {
                    message: e.to_string(),
                }
                .into())
            }
        }
    }

    /// Validate JWT token
    async fn validate_jwt_token(&self, token: &str, request_id: &str) -> Result<AuthContext> {
        // Check if token is revoked
        {
            let revoked = self.revoked_tokens.read().await;
            if revoked.contains(token) {
                return Err(anyhow!("Token has been revoked"));
            }
        }

        let mut validation = Validation::new(Algorithm::HS256);
        validation.set_required_spec_claims(&["sub", "exp", "iat"]);

        let token_data = decode::<Claims>(token, &self.decoding_key, &validation)
            .map_err(|e| anyhow!("Invalid JWT token: {}", e))?;

        let claims = token_data.claims;

        // Verify token is not expired
        let now = Utc::now().timestamp();
        if claims.exp < now {
            return Err(anyhow!("Token has expired"));
        }

        // Verify required scopes
        if !self.has_required_scopes(&claims.scope) {
            return Err(anyhow!("Insufficient permissions"));
        }

        Ok(AuthContext {
            client_id: claims.client_id,
            user_id: claims.sub,
            method: AuthMethod::JwtToken,
            scopes: claims.scope,
            expires_at: chrono::DateTime::from_timestamp(claims.exp, 0),
            request_id: request_id.to_string(),
        })
    }

    /// Validate API key
    async fn validate_api_key(&self, api_key: &str, request_id: &str) -> Result<AuthContext> {
        let api_key_info = self
            .config
            .api_keys
            .get(api_key)
            .ok_or_else(|| anyhow!("Invalid API key"))?;

        // Check if key is expired
        if let Some(expires_at) = api_key_info.expires_at {
            if Utc::now() > expires_at {
                return Err(anyhow!("API key has expired"));
            }
        }

        // Verify required scopes
        if !self.has_required_scopes(&api_key_info.scopes) {
            return Err(anyhow!("Insufficient permissions"));
        }

        Ok(AuthContext {
            client_id: api_key_info.client_id.clone(),
            user_id: api_key_info.client_id.clone(), // Use client_id as user_id for API keys
            method: AuthMethod::ApiKey,
            scopes: api_key_info.scopes.clone(),
            expires_at: api_key_info.expires_at,
            request_id: request_id.to_string(),
        })
    }

    /// Validate client certificate
    async fn validate_certificate(
        &self,
        thumbprint: &str,
        request_id: &str,
    ) -> Result<AuthContext> {
        if !self.config.allowed_certificates.contains(thumbprint) {
            return Err(anyhow!("Certificate not allowed"));
        }

        // For certificate-based auth, we grant full access
        // In production, you'd extract more info from the certificate
        Ok(AuthContext {
            client_id: format!("cert-{thumbprint}"),
            user_id: format!("cert-{thumbprint}"),
            method: AuthMethod::Certificate,
            scopes: vec!["mcp:read".to_string(), "mcp:write".to_string()],
            expires_at: None, // Certificates don't expire in this context
            request_id: request_id.to_string(),
        })
    }

    /// Check if provided scopes meet requirements
    fn has_required_scopes(&self, provided_scopes: &[String]) -> bool {
        if self.config.require_scope.is_empty() {
            return true;
        }

        self.config
            .require_scope
            .iter()
            .all(|required| provided_scopes.contains(required))
    }

    /// Generate JWT token for a client
    pub async fn generate_token(
        &self,
        client_id: &str,
        user_id: &str,
        scopes: Vec<String>,
    ) -> Result<String> {
        let now = Utc::now();
        let exp = now + Duration::seconds(self.config.jwt_expiry_seconds as i64);

        let claims = Claims {
            sub: user_id.to_string(),
            client_id: client_id.to_string(),
            scope: scopes,
            iat: now.timestamp(),
            exp: exp.timestamp(),
            jti: Uuid::new_v4().to_string(),
        };

        encode(&Header::default(), &claims, &self.encoding_key)
            .map_err(|e| anyhow!("Failed to generate token: {}", e))
    }

    /// Revoke a JWT token
    pub async fn revoke_token(&self, token: &str) -> Result<()> {
        let mut revoked = self.revoked_tokens.write().await;
        revoked.insert(token.to_string());
        debug!("Token revoked");
        Ok(())
    }

    /// Validate tool access permissions
    pub fn validate_tool_access(&self, context: &AuthContext, tool_name: &str) -> Result<()> {
        // Map tools to required scopes
        let required_scope = match tool_name {
            "store_memory" | "harvest_conversation" | "migrate_memory" | "delete_memory" => {
                "mcp:write"
            }
            "search_memory"
            | "get_statistics"
            | "what_did_you_remember"
            | "get_harvester_metrics" => "mcp:read",
            _ => "mcp:read", // Default to read access
        };

        if !context.scopes.contains(&required_scope.to_string()) {
            return Err(SecurityError::AuthorizationFailed {
                message: format!("Tool '{tool_name}' requires '{required_scope}' scope"),
            }
            .into());
        }

        Ok(())
    }

    /// Get authentication statistics
    pub async fn get_stats(&self) -> serde_json::Value {
        let revoked_count = self.revoked_tokens.read().await.len();

        serde_json::json!({
            "enabled": self.config.enabled,
            "api_keys_configured": self.config.api_keys.len(),
            "certificates_allowed": self.config.allowed_certificates.len(),
            "revoked_tokens": revoked_count,
            "performance_target_ms": self.config.performance_target_ms,
            "jwt_expiry_seconds": self.config.jwt_expiry_seconds,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::AuditConfig;
    use std::collections::HashMap;
    use tempfile::tempdir;

    fn create_test_config() -> MCPAuthConfig {
        let mut api_keys = HashMap::new();
        api_keys.insert(
            "test-key-123".to_string(),
            ApiKeyInfo {
                client_id: "test-client".to_string(),
                scopes: vec!["mcp:read".to_string(), "mcp:write".to_string()],
                expires_at: None,
                last_used: None,
                usage_count: 0,
            },
        );

        let mut certs = HashSet::new();
        certs.insert("abc123def456".to_string());

        MCPAuthConfig {
            enabled: true,
            jwt_secret: "test-secret-key-minimum-32-characters-long".to_string(),
            jwt_expiry_seconds: 3600,
            api_keys,
            allowed_certificates: certs,
            require_scope: vec!["mcp:read".to_string()],
            performance_target_ms: 5,
        }
    }

    async fn create_test_auth() -> MCPAuth {
        let config = create_test_config();
        let temp_dir = tempdir().unwrap();
        let audit_config = AuditConfig {
            enabled: true,
            log_all_requests: true,
            log_data_access: true,
            log_modifications: true,
            log_auth_events: true,
            retention_days: 30,
        };
        let audit_logger = Arc::new(AuditLogger::new(audit_config).unwrap());
        MCPAuth::new(config, audit_logger).unwrap()
    }

    #[tokio::test]
    async fn test_api_key_authentication() {
        let auth = create_test_auth().await;

        let mut headers = HashMap::new();
        headers.insert(
            "authorization".to_string(),
            "ApiKey test-key-123".to_string(),
        );

        let result = auth
            .authenticate_request("tools/call", None, &headers)
            .await;
        assert!(result.is_ok());

        let context = result.unwrap().unwrap();
        assert_eq!(context.client_id, "test-client");
        assert_eq!(context.method, AuthMethod::ApiKey);
        assert!(context.scopes.contains(&"mcp:read".to_string()));
    }

    #[tokio::test]
    async fn test_jwt_authentication() {
        let auth = create_test_auth().await;

        // Generate a test token
        let token = auth
            .generate_token(
                "test-client",
                "test-user",
                vec!["mcp:read".to_string(), "mcp:write".to_string()],
            )
            .await
            .unwrap();

        let mut headers = HashMap::new();
        headers.insert("authorization".to_string(), format!("Bearer {token}"));

        let result = auth
            .authenticate_request("tools/call", None, &headers)
            .await;
        assert!(result.is_ok());

        let context = result.unwrap().unwrap();
        assert_eq!(context.client_id, "test-client");
        assert_eq!(context.user_id, "test-user");
        assert_eq!(context.method, AuthMethod::JwtToken);
    }

    #[tokio::test]
    async fn test_certificate_authentication() {
        let auth = create_test_auth().await;

        let mut headers = HashMap::new();
        headers.insert(
            "x-client-cert-thumbprint".to_string(),
            "abc123def456".to_string(),
        );

        let result = auth
            .authenticate_request("tools/call", None, &headers)
            .await;
        assert!(result.is_ok());

        let context = result.unwrap().unwrap();
        assert_eq!(context.client_id, "cert-abc123def456");
        assert_eq!(context.method, AuthMethod::Certificate);
    }

    #[tokio::test]
    async fn test_invalid_api_key() {
        let auth = create_test_auth().await;

        let mut headers = HashMap::new();
        headers.insert(
            "authorization".to_string(),
            "ApiKey invalid-key".to_string(),
        );

        let result = auth
            .authenticate_request("tools/call", None, &headers)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_tool_access_validation() {
        let auth = create_test_auth().await;

        let context = AuthContext {
            client_id: "test-client".to_string(),
            user_id: "test-user".to_string(),
            method: AuthMethod::ApiKey,
            scopes: vec!["mcp:read".to_string()],
            expires_at: None,
            request_id: "test-request".to_string(),
        };

        // Should allow read operations
        assert!(auth.validate_tool_access(&context, "search_memory").is_ok());
        assert!(auth
            .validate_tool_access(&context, "get_statistics")
            .is_ok());

        // Should deny write operations
        assert!(auth.validate_tool_access(&context, "store_memory").is_err());
        assert!(auth
            .validate_tool_access(&context, "delete_memory")
            .is_err());
    }

    #[tokio::test]
    async fn test_token_revocation() {
        let auth = create_test_auth().await;

        let token = auth
            .generate_token("test-client", "test-user", vec!["mcp:read".to_string()])
            .await
            .unwrap();

        // Token should work initially
        let mut headers = HashMap::new();
        headers.insert("authorization".to_string(), format!("Bearer {token}"));

        let result = auth
            .authenticate_request("tools/call", None, &headers)
            .await;
        assert!(result.is_ok());

        // Revoke the token
        auth.revoke_token(&token).await.unwrap();

        // Token should no longer work
        let result = auth
            .authenticate_request("tools/call", None, &headers)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_disabled_authentication() {
        let mut config = create_test_config();
        config.enabled = false;

        let temp_dir = tempdir().unwrap();
        let audit_config = AuditConfig {
            enabled: true,
            log_all_requests: true,
            log_data_access: true,
            log_modifications: true,
            log_auth_events: true,
            retention_days: 30,
        };
        let audit_logger = Arc::new(AuditLogger::new(audit_config).unwrap());
        let auth = MCPAuth::new(config, audit_logger).unwrap();

        let headers = HashMap::new();
        let result = auth
            .authenticate_request("tools/call", None, &headers)
            .await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none()); // Should return None when disabled
    }
}
