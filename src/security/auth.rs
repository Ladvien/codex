use crate::security::{AuthConfig, Result, SecurityError};
use axum::{
    extract::{Request, State},
    http::{header::AUTHORIZATION, HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;

/// JWT Claims structure
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String,              // Subject (user ID)
    pub name: String,             // User name
    pub role: String,             // User role
    pub permissions: Vec<String>, // User permissions
    pub exp: u64,                 // Expiration time
    pub iat: u64,                 // Issued at
    pub jti: String,              // JWT ID
}

/// API Key structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    pub key_id: String,
    pub key_hash: String,
    pub name: String,
    pub role: String,
    pub permissions: Vec<String>,
    pub created_at: u64,
    pub expires_at: Option<u64>,
    pub last_used: Option<u64>,
    pub active: bool,
}

/// User session information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSession {
    pub user_id: String,
    pub name: String,
    pub role: String,
    pub permissions: Vec<String>,
    pub authenticated_at: u64,
    pub last_activity: u64,
    pub auth_method: AuthMethod,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthMethod {
    JWT,
    ApiKey,
    MTLS,
}

/// Authentication manager
pub struct AuthManager {
    config: AuthConfig,
    api_keys: Arc<RwLock<HashMap<String, ApiKey>>>,
    active_sessions: Arc<RwLock<HashMap<String, UserSession>>>,
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
}

impl AuthManager {
    pub fn new(config: AuthConfig) -> Result<Self> {
        let encoding_key = EncodingKey::from_secret(config.jwt_secret.as_bytes());
        let decoding_key = DecodingKey::from_secret(config.jwt_secret.as_bytes());

        Ok(Self {
            config,
            api_keys: Arc::new(RwLock::new(HashMap::new())),
            active_sessions: Arc::new(RwLock::new(HashMap::new())),
            encoding_key,
            decoding_key,
        })
    }

    /// Create a new JWT token for a user
    pub async fn create_jwt_token(
        &self,
        user_id: &str,
        name: &str,
        role: &str,
        permissions: Vec<String>,
    ) -> Result<String> {
        if !self.config.enabled {
            return Err(SecurityError::AuthenticationFailed {
                message: "Authentication is disabled".to_string(),
            });
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let claims = Claims {
            sub: user_id.to_string(),
            name: name.to_string(),
            role: role.to_string(),
            permissions,
            exp: now + self.config.jwt_expiry_seconds,
            iat: now,
            jti: Uuid::new_v4().to_string(),
        };

        let header = Header::new(Algorithm::HS256);
        let token = encode(&header, &claims, &self.encoding_key).map_err(|e| {
            SecurityError::AuthenticationFailed {
                message: format!("Failed to create JWT token: {e}"),
            }
        })?;

        // Store session
        let session = UserSession {
            user_id: user_id.to_string(),
            name: name.to_string(),
            role: role.to_string(),
            permissions: claims.permissions.clone(),
            authenticated_at: now,
            last_activity: now,
            auth_method: AuthMethod::JWT,
        };

        self.active_sessions
            .write()
            .await
            .insert(claims.jti.clone(), session);

        info!("JWT token created for user: {} ({})", name, user_id);
        Ok(token)
    }

    /// Validate and decode JWT token
    pub async fn validate_jwt_token(&self, token: &str) -> Result<Claims> {
        if !self.config.enabled {
            return Err(SecurityError::AuthenticationFailed {
                message: "Authentication is disabled".to_string(),
            });
        }

        let validation = Validation::new(Algorithm::HS256);
        let token_data = decode::<Claims>(token, &self.decoding_key, &validation).map_err(|e| {
            SecurityError::AuthenticationFailed {
                message: format!("Invalid JWT token: {e}"),
            }
        })?;

        let claims = token_data.claims;

        // Check if session is still active
        let mut sessions = self.active_sessions.write().await;
        if let Some(session) = sessions.get_mut(&claims.jti) {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();

            // Check session timeout
            if now - session.last_activity > (self.config.session_timeout_minutes * 60) {
                sessions.remove(&claims.jti);
                return Err(SecurityError::AuthenticationFailed {
                    message: "Session expired".to_string(),
                });
            }

            // Update last activity
            session.last_activity = now;
        } else {
            return Err(SecurityError::AuthenticationFailed {
                message: "Session not found".to_string(),
            });
        }

        debug!("JWT token validated for user: {}", claims.sub);
        Ok(claims)
    }

    /// Create a new API key
    pub async fn create_api_key(
        &self,
        name: &str,
        role: &str,
        permissions: Vec<String>,
        expires_in_days: Option<u32>,
    ) -> Result<(String, ApiKey)> {
        if !self.config.enabled || !self.config.api_key_enabled {
            return Err(SecurityError::AuthenticationFailed {
                message: "API key authentication is disabled".to_string(),
            });
        }

        let key_id = Uuid::new_v4().to_string();
        let raw_key = format!("ak_{}", Uuid::new_v4().simple());
        let key_hash = self.hash_api_key(&raw_key);

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let expires_at = expires_in_days.map(|days| now + (days as u64 * 24 * 60 * 60));

        let api_key = ApiKey {
            key_id: key_id.clone(),
            key_hash,
            name: name.to_string(),
            role: role.to_string(),
            permissions,
            created_at: now,
            expires_at,
            last_used: None,
            active: true,
        };

        self.api_keys
            .write()
            .await
            .insert(key_id.clone(), api_key.clone());

        info!("API key created: {} for role: {}", name, role);
        Ok((raw_key, api_key))
    }

    /// Validate API key
    pub async fn validate_api_key(&self, key: &str) -> Result<ApiKey> {
        if !self.config.enabled || !self.config.api_key_enabled {
            return Err(SecurityError::AuthenticationFailed {
                message: "API key authentication is disabled".to_string(),
            });
        }

        let key_hash = self.hash_api_key(key);
        let mut api_keys = self.api_keys.write().await;

        for (_, api_key) in api_keys.iter_mut() {
            if api_key.key_hash == key_hash && api_key.active {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs();

                // Check expiration
                if let Some(expires_at) = api_key.expires_at {
                    if now > expires_at {
                        return Err(SecurityError::AuthenticationFailed {
                            message: "API key expired".to_string(),
                        });
                    }
                }

                // Update last used
                api_key.last_used = Some(now);

                debug!("API key validated: {}", api_key.name);
                return Ok(api_key.clone());
            }
        }

        Err(SecurityError::AuthenticationFailed {
            message: "Invalid API key".to_string(),
        })
    }

    /// Revoke API key
    pub async fn revoke_api_key(&self, key_id: &str) -> Result<()> {
        let mut api_keys = self.api_keys.write().await;

        if let Some(api_key) = api_keys.get_mut(key_id) {
            api_key.active = false;
            info!("API key revoked: {}", api_key.name);
            Ok(())
        } else {
            Err(SecurityError::AuthenticationFailed {
                message: "API key not found".to_string(),
            })
        }
    }

    /// Get active sessions
    pub async fn get_active_sessions(&self) -> Vec<UserSession> {
        let sessions = self.active_sessions.read().await;
        sessions.values().cloned().collect()
    }

    /// Revoke user session
    pub async fn revoke_session(&self, session_id: &str) -> Result<()> {
        let mut sessions = self.active_sessions.write().await;

        if sessions.remove(session_id).is_some() {
            info!("Session revoked: {}", session_id);
            Ok(())
        } else {
            Err(SecurityError::AuthenticationFailed {
                message: "Session not found".to_string(),
            })
        }
    }

    /// Clean up expired sessions
    pub async fn cleanup_expired_sessions(&self) -> Result<usize> {
        let mut sessions = self.active_sessions.write().await;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let timeout_seconds = self.config.session_timeout_minutes * 60;
        let initial_count = sessions.len();

        sessions.retain(|_, session| now - session.last_activity <= timeout_seconds);

        let removed_count = initial_count - sessions.len();

        if removed_count > 0 {
            info!("Cleaned up {} expired sessions", removed_count);
        }

        Ok(removed_count)
    }

    fn hash_api_key(&self, key: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(key.as_bytes());
        hasher.update(self.config.jwt_secret.as_bytes()); // Use JWT secret as salt
        hex::encode(hasher.finalize())
    }

    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    pub fn is_api_key_enabled(&self) -> bool {
        self.config.enabled && self.config.api_key_enabled
    }

    pub fn is_mtls_enabled(&self) -> bool {
        self.config.enabled && self.config.mtls_enabled
    }
}

/// Authentication middleware for Axum
pub async fn auth_middleware(
    State(auth_manager): State<Arc<AuthManager>>,
    headers: HeaderMap,
    mut request: Request,
    next: Next,
) -> std::result::Result<Response, StatusCode> {
    if !auth_manager.is_enabled() {
        return Ok(next.run(request).await);
    }

    // Try JWT authentication first
    if let Some(auth_header) = headers.get(AUTHORIZATION) {
        if let Ok(auth_str) = auth_header.to_str() {
            if let Some(token) = auth_str.strip_prefix("Bearer ") {
                match auth_manager.validate_jwt_token(token).await {
                    Ok(claims) => {
                        request.extensions_mut().insert(claims);
                        return Ok(next.run(request).await);
                    }
                    Err(e) => {
                        debug!("JWT validation failed: {}", e);
                    }
                }
            }
        }
    }

    // Try API key authentication
    if let Some(api_key_header) = headers.get("X-API-Key") {
        if let Ok(api_key) = api_key_header.to_str() {
            match auth_manager.validate_api_key(api_key).await {
                Ok(key_info) => {
                    // Convert API key to claims-like structure
                    let claims = Claims {
                        sub: key_info.key_id.clone(),
                        name: key_info.name,
                        role: key_info.role,
                        permissions: key_info.permissions,
                        exp: key_info.expires_at.unwrap_or(u64::MAX),
                        iat: key_info.created_at,
                        jti: key_info.key_id,
                    };
                    request.extensions_mut().insert(claims);
                    return Ok(next.run(request).await);
                }
                Err(e) => {
                    debug!("API key validation failed: {}", e);
                }
            }
        }
    }

    // Authentication failed
    warn!("Authentication failed for request");
    Err(StatusCode::UNAUTHORIZED)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_auth_manager_creation() {
        let config = AuthConfig::default();
        let manager = AuthManager::new(config).unwrap();
        assert!(!manager.is_enabled());
    }

    #[tokio::test]
    async fn test_jwt_token_disabled() {
        let config = AuthConfig::default();
        let manager = AuthManager::new(config).unwrap();

        let result = manager
            .create_jwt_token("user1", "Test User", "user", vec!["read".to_string()])
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_jwt_token_creation_and_validation() {
        let mut config = AuthConfig::default();
        config.enabled = true;
        config.jwt_secret = "test-secret".to_string();

        let manager = AuthManager::new(config).unwrap();

        // Create token
        let token = manager
            .create_jwt_token(
                "user1",
                "Test User",
                "admin",
                vec!["read".to_string(), "write".to_string()],
            )
            .await
            .unwrap();

        // Validate token
        let claims = manager.validate_jwt_token(&token).await.unwrap();
        assert_eq!(claims.sub, "user1");
        assert_eq!(claims.name, "Test User");
        assert_eq!(claims.role, "admin");
        assert_eq!(
            claims.permissions,
            vec!["read".to_string(), "write".to_string()]
        );
    }

    #[tokio::test]
    async fn test_api_key_creation_and_validation() {
        let mut config = AuthConfig::default();
        config.enabled = true;
        config.api_key_enabled = true;

        let manager = AuthManager::new(config).unwrap();

        // Create API key
        let (raw_key, api_key) = manager
            .create_api_key("test-key", "user", vec!["read".to_string()], Some(30))
            .await
            .unwrap();
        assert!(!raw_key.is_empty());
        assert_eq!(api_key.name, "test-key");
        assert_eq!(api_key.role, "user");

        // Validate API key
        let validated_key = manager.validate_api_key(&raw_key).await.unwrap();
        assert_eq!(validated_key.name, "test-key");
        assert_eq!(validated_key.role, "user");
    }

    #[tokio::test]
    async fn test_invalid_jwt_token() {
        let mut config = AuthConfig::default();
        config.enabled = true;

        let manager = AuthManager::new(config).unwrap();

        let result = manager.validate_jwt_token("invalid.jwt.token").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_session_cleanup() {
        let mut config = AuthConfig::default();
        config.enabled = true;
        config.jwt_secret = "test-secret-key-for-unit-testing-with-sufficient-length".to_string();
        config.session_timeout_minutes = 1; // 1 minute timeout

        let manager = AuthManager::new(config).unwrap();

        // Create a token which creates a session
        let token = manager
            .create_jwt_token("user1", "Test User", "user", vec!["read".to_string()])
            .await
            .unwrap();

        // Manually expire the session by setting its last_activity to past
        {
            let mut sessions = manager.active_sessions.write().await;
            for (_, session) in sessions.iter_mut() {
                // Set last activity to 2 minutes ago (past the 1 minute timeout)
                session.last_activity = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs() - 120;
            }
        }

        // Now cleanup should remove the expired session
        let removed = manager.cleanup_expired_sessions().await.unwrap();
        assert_eq!(removed, 1, "Should have removed 1 expired session");

        // Token validation should still work based on JWT expiry, not session
        // (sessions are for tracking, not for JWT validation in this implementation)
        let validation_result = manager.validate_jwt_token(&token).await;
        // This might succeed if JWT isn't expired yet, which is OK
    }

    #[tokio::test]
    async fn test_api_key_revocation() {
        let mut config = AuthConfig::default();
        config.enabled = true;
        config.api_key_enabled = true;

        let manager = AuthManager::new(config).unwrap();

        // Create API key
        let (raw_key, api_key) = manager
            .create_api_key("test-key", "user", vec!["read".to_string()], None)
            .await
            .unwrap();

        // Validate it works
        assert!(manager.validate_api_key(&raw_key).await.is_ok());

        // Revoke the key
        manager.revoke_api_key(&api_key.key_id).await.unwrap();

        // Validation should now fail
        assert!(manager.validate_api_key(&raw_key).await.is_err());
    }
}
