use crate::security::{Result, SecretsConfig, SecurityError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Secrets management system
pub struct SecretsManager {
    config: SecretsConfig,
    secrets_cache: Arc<RwLock<HashMap<String, SecretValue>>>,
}

/// Secret value with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretValue {
    pub value: String,
    pub version: u32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub metadata: HashMap<String, String>,
}

/// Secret metadata only (without the actual value)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretMetadata {
    pub key: String,
    pub version: u32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub metadata: HashMap<String, String>,
}

impl SecretsManager {
    pub fn new(config: SecretsConfig) -> Result<Self> {
        let manager = Self {
            config,
            secrets_cache: Arc::new(RwLock::new(HashMap::new())),
        };

        Ok(manager)
    }

    /// Initialize secrets management system
    pub async fn initialize(&self) -> Result<()> {
        info!("Initializing secrets management system");

        if self.config.vault_enabled {
            self.initialize_vault().await?;
        } else if self.config.env_fallback {
            self.load_from_environment().await?;
        }

        info!("Secrets management system initialized");
        Ok(())
    }

    /// Initialize HashiCorp Vault connection (mock implementation)
    async fn initialize_vault(&self) -> Result<()> {
        if let Some(vault_address) = &self.config.vault_address {
            info!("Connecting to Vault at: {}", vault_address);

            // In a real implementation, this would:
            // 1. Load the Vault token from the token path
            // 2. Create a Vault client
            // 3. Test the connection
            // 4. Set up token renewal if needed

            if let Some(token_path) = &self.config.vault_token_path {
                if !token_path.exists() {
                    return Err(SecurityError::SecretsError {
                        message: format!("Vault token file not found: {token_path:?}"),
                    });
                }

                debug!("Vault token found at: {:?}", token_path);

                // Mock: Load token from file
                let _token =
                    fs::read_to_string(token_path).map_err(|e| SecurityError::SecretsError {
                        message: format!("Failed to read Vault token: {e}"),
                    })?;

                info!("Connected to Vault successfully");
            } else {
                warn!("Vault enabled but no token path configured");
            }
        } else {
            return Err(SecurityError::SecretsError {
                message: "Vault enabled but no address configured".to_string(),
            });
        }

        Ok(())
    }

    /// Load secrets from environment variables
    async fn load_from_environment(&self) -> Result<()> {
        debug!("Loading secrets from environment variables");

        let mut cache = self.secrets_cache.write().await;

        // Load common secret environment variables
        let env_secrets = vec![
            "DATABASE_URL",
            "OPENAI_API_KEY",
            "JWT_SECRET",
            "ENCRYPTION_KEY",
            "VAULT_TOKEN",
            "API_KEY",
            "SESSION_SECRET",
        ];

        for secret_name in env_secrets {
            if let Ok(secret_value) = std::env::var(secret_name) {
                let secret = SecretValue {
                    value: secret_value,
                    version: 1,
                    created_at: chrono::Utc::now(),
                    expires_at: None,
                    metadata: HashMap::new(),
                };

                cache.insert(secret_name.to_string(), secret);
                debug!("Loaded secret from environment: {}", secret_name);
            }
        }

        info!("Loaded {} secrets from environment", cache.len());
        Ok(())
    }

    /// Get secret value
    pub async fn get_secret(&self, key: &str) -> Result<String> {
        // First check cache
        {
            let cache = self.secrets_cache.read().await;
            if let Some(secret) = cache.get(key) {
                // Check if secret is expired
                if let Some(expires_at) = secret.expires_at {
                    if chrono::Utc::now() > expires_at {
                        warn!("Secret '{}' has expired", key);
                        return Err(SecurityError::SecretsError {
                            message: format!("Secret '{key}' has expired"),
                        });
                    }
                }

                debug!("Retrieved secret from cache: {}", key);
                return Ok(secret.value.clone());
            }
        }

        // If not in cache, try to load from source
        if self.config.vault_enabled {
            self.get_secret_from_vault(key).await
        } else if self.config.env_fallback {
            self.get_secret_from_env(key).await
        } else {
            Err(SecurityError::SecretsError {
                message: format!("Secret '{key}' not found and no fallback configured"),
            })
        }
    }

    /// Get secret from HashiCorp Vault (mock implementation)
    async fn get_secret_from_vault(&self, key: &str) -> Result<String> {
        // In a real implementation, this would make an HTTP request to Vault
        // For now, this is a mock that returns an error
        Err(SecurityError::SecretsError {
            message: format!("Vault integration not fully implemented for key: {key}"),
        })
    }

    /// Get secret from environment variable
    async fn get_secret_from_env(&self, key: &str) -> Result<String> {
        match std::env::var(key) {
            Ok(value) => {
                // Cache the secret
                let secret = SecretValue {
                    value: value.clone(),
                    version: 1,
                    created_at: chrono::Utc::now(),
                    expires_at: None,
                    metadata: HashMap::new(),
                };

                let mut cache = self.secrets_cache.write().await;
                cache.insert(key.to_string(), secret);

                debug!("Retrieved secret from environment: {}", key);
                Ok(value)
            }
            Err(_) => Err(SecurityError::SecretsError {
                message: format!("Secret '{key}' not found in environment"),
            }),
        }
    }

    /// Set secret value (for testing or manual secret management)
    pub async fn set_secret(
        &self,
        key: &str,
        value: &str,
        expires_in_seconds: Option<i64>,
    ) -> Result<()> {
        let expires_at = expires_in_seconds
            .map(|seconds| chrono::Utc::now() + chrono::Duration::seconds(seconds));

        let secret = SecretValue {
            value: value.to_string(),
            version: 1,
            created_at: chrono::Utc::now(),
            expires_at,
            metadata: HashMap::new(),
        };

        let mut cache = self.secrets_cache.write().await;
        cache.insert(key.to_string(), secret);

        debug!("Set secret: {}", key);
        Ok(())
    }

    /// Delete secret from cache
    pub async fn delete_secret(&self, key: &str) -> Result<()> {
        let mut cache = self.secrets_cache.write().await;

        if cache.remove(key).is_some() {
            debug!("Deleted secret from cache: {}", key);
            Ok(())
        } else {
            Err(SecurityError::SecretsError {
                message: format!("Secret '{key}' not found in cache"),
            })
        }
    }

    /// Get secret metadata (without the actual value)
    pub async fn get_secret_metadata(&self, key: &str) -> Result<SecretMetadata> {
        let cache = self.secrets_cache.read().await;

        if let Some(secret) = cache.get(key) {
            Ok(SecretMetadata {
                key: key.to_string(),
                version: secret.version,
                created_at: secret.created_at,
                expires_at: secret.expires_at,
                metadata: secret.metadata.clone(),
            })
        } else {
            Err(SecurityError::SecretsError {
                message: format!("Secret '{key}' not found"),
            })
        }
    }

    /// List all secret keys (without values)
    pub async fn list_secrets(&self) -> Vec<String> {
        let cache = self.secrets_cache.read().await;
        cache.keys().cloned().collect()
    }

    /// Rotate secret (create new version)
    pub async fn rotate_secret(&self, key: &str, new_value: &str) -> Result<u32> {
        let mut cache = self.secrets_cache.write().await;

        let new_version = if let Some(existing_secret) = cache.get(key) {
            existing_secret.version + 1
        } else {
            1
        };

        let secret = SecretValue {
            value: new_value.to_string(),
            version: new_version,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: HashMap::new(),
        };

        cache.insert(key.to_string(), secret);

        info!("Rotated secret '{}' to version {}", key, new_version);
        Ok(new_version)
    }

    /// Clean up expired secrets
    pub async fn cleanup_expired_secrets(&self) -> Result<usize> {
        let mut cache = self.secrets_cache.write().await;
        let now = chrono::Utc::now();

        let initial_count = cache.len();

        cache.retain(|key, secret| {
            if let Some(expires_at) = secret.expires_at {
                if now > expires_at {
                    debug!("Removing expired secret: {}", key);
                    return false;
                }
            }
            true
        });

        let removed_count = initial_count - cache.len();

        if removed_count > 0 {
            info!("Cleaned up {} expired secrets", removed_count);
        }

        Ok(removed_count)
    }

    /// Get secrets statistics
    pub async fn get_statistics(&self) -> SecretsStatistics {
        let cache = self.secrets_cache.read().await;
        let now = chrono::Utc::now();

        let total_secrets = cache.len();
        let mut expired_secrets = 0;
        let mut expiring_soon = 0; // Within 24 hours

        for secret in cache.values() {
            if let Some(expires_at) = secret.expires_at {
                if now > expires_at {
                    expired_secrets += 1;
                } else if expires_at - now < chrono::Duration::hours(24) {
                    expiring_soon += 1;
                }
            }
        }

        SecretsStatistics {
            total_secrets,
            expired_secrets,
            expiring_soon,
            vault_enabled: self.config.vault_enabled,
            env_fallback_enabled: self.config.env_fallback,
        }
    }

    /// Test connection to secrets backend
    pub async fn test_connection(&self) -> Result<()> {
        if self.config.vault_enabled {
            // Test Vault connection
            if let Some(vault_address) = &self.config.vault_address {
                debug!("Testing Vault connection to: {}", vault_address);
                // In a real implementation, this would make a health check request
                Ok(())
            } else {
                Err(SecurityError::SecretsError {
                    message: "Vault address not configured".to_string(),
                })
            }
        } else {
            // Test environment variable access
            match std::env::var("PATH") {
                Ok(_) => {
                    debug!("Environment variable access test passed");
                    Ok(())
                }
                Err(_) => Err(SecurityError::SecretsError {
                    message: "Cannot access environment variables".to_string(),
                }),
            }
        }
    }

    pub fn is_vault_enabled(&self) -> bool {
        self.config.vault_enabled
    }

    pub fn is_env_fallback_enabled(&self) -> bool {
        self.config.env_fallback
    }
}

/// Secrets management statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretsStatistics {
    pub total_secrets: usize,
    pub expired_secrets: usize,
    pub expiring_soon: usize,
    pub vault_enabled: bool,
    pub env_fallback_enabled: bool,
}

/// Utility functions for secret management
impl SecretsManager {
    /// Generate a random secret key
    pub fn generate_random_key(length: usize) -> String {
        use rand::Rng;
        const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

        let mut rng = rand::thread_rng();
        (0..length)
            .map(|_| {
                let idx = rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect()
    }

    /// Generate a secure JWT secret
    pub fn generate_jwt_secret() -> String {
        Self::generate_random_key(64)
    }

    /// Generate a secure API key
    pub fn generate_api_key() -> String {
        format!("ak_{}", Self::generate_random_key(32))
    }

    /// Generate a secure encryption key
    pub fn generate_encryption_key() -> String {
        Self::generate_random_key(32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_secrets_manager_creation() {
        let config = SecretsConfig::default();
        let manager = SecretsManager::new(config).unwrap();
        assert!(!manager.is_vault_enabled());
        assert!(manager.is_env_fallback_enabled());
    }

    #[tokio::test]
    async fn test_secret_operations() {
        let config = SecretsConfig::default();
        let manager = SecretsManager::new(config).unwrap();

        // Set a secret
        let result = manager.set_secret("test_key", "test_value", None).await;
        assert!(result.is_ok());

        // Get the secret
        let value = manager.get_secret("test_key").await.unwrap();
        assert_eq!(value, "test_value");

        // Get secret metadata
        let metadata = manager.get_secret_metadata("test_key").await.unwrap();
        assert_eq!(metadata.key, "test_key");
        assert_eq!(metadata.version, 1);

        // Delete the secret
        let result = manager.delete_secret("test_key").await;
        assert!(result.is_ok());

        // Try to get deleted secret
        let result = manager.get_secret("test_key").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_secret_expiration() {
        let config = SecretsConfig::default();
        let manager = SecretsManager::new(config).unwrap();

        // Set a secret that expires in 1 second
        manager
            .set_secret("expiring_key", "expiring_value", Some(1))
            .await
            .unwrap();

        // Should be able to get it immediately
        let value = manager.get_secret("expiring_key").await.unwrap();
        assert_eq!(value, "expiring_value");

        // Wait for expiration
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // Should now be expired
        let result = manager.get_secret("expiring_key").await;
        assert!(result.is_err());

        if let Err(SecurityError::SecretsError { message }) = result {
            assert!(message.contains("expired"));
        }
    }

    #[tokio::test]
    async fn test_secret_rotation() {
        let config = SecretsConfig::default();
        let manager = SecretsManager::new(config).unwrap();

        // Set initial secret
        manager
            .set_secret("rotating_key", "value_v1", None)
            .await
            .unwrap();

        // Rotate the secret
        let new_version = manager
            .rotate_secret("rotating_key", "value_v2")
            .await
            .unwrap();
        assert_eq!(new_version, 2);

        // Should get the new value
        let value = manager.get_secret("rotating_key").await.unwrap();
        assert_eq!(value, "value_v2");

        // Metadata should show new version
        let metadata = manager.get_secret_metadata("rotating_key").await.unwrap();
        assert_eq!(metadata.version, 2);
    }

    #[tokio::test]
    async fn test_cleanup_expired_secrets() {
        let config = SecretsConfig::default();
        let manager = SecretsManager::new(config).unwrap();

        // Set some secrets with different expiration times
        manager
            .set_secret("permanent", "value", None)
            .await
            .unwrap();
        manager
            .set_secret("short_lived", "value", Some(1))
            .await
            .unwrap();
        manager
            .set_secret("medium_lived", "value", Some(10))
            .await
            .unwrap();

        // Wait for short-lived secret to expire
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // Cleanup expired secrets
        let removed_count = manager.cleanup_expired_secrets().await.unwrap();
        assert_eq!(removed_count, 1);

        // Should still have the other secrets
        let secrets = manager.list_secrets().await;
        assert_eq!(secrets.len(), 2);
        assert!(secrets.contains(&"permanent".to_string()));
        assert!(secrets.contains(&"medium_lived".to_string()));
    }

    #[tokio::test]
    async fn test_get_statistics() {
        let config = SecretsConfig::default();
        let manager = SecretsManager::new(config).unwrap();

        // Add some test secrets
        manager.set_secret("secret1", "value1", None).await.unwrap();
        manager
            .set_secret("secret2", "value2", Some(1))
            .await
            .unwrap(); // Expires soon

        // Wait for one to expire
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        let stats = manager.get_statistics().await;
        assert_eq!(stats.total_secrets, 2);
        assert_eq!(stats.expired_secrets, 1);
        assert!(stats.env_fallback_enabled);
        assert!(!stats.vault_enabled);
    }

    #[tokio::test]
    async fn test_environment_fallback() {
        // Set a test environment variable
        std::env::set_var("TEST_SECRET_KEY", "test_secret_value");

        let config = SecretsConfig {
            vault_enabled: false,
            vault_address: None,
            vault_token_path: None,
            env_fallback: true,
        };

        let manager = SecretsManager::new(config).unwrap();

        // Should be able to get the environment variable
        let value = manager.get_secret("TEST_SECRET_KEY").await.unwrap();
        assert_eq!(value, "test_secret_value");

        // Clean up
        std::env::remove_var("TEST_SECRET_KEY");
    }

    #[tokio::test]
    async fn test_vault_token_path_validation() {
        let temp_dir = tempdir().unwrap();
        let token_path = temp_dir.path().join("vault_token");

        // Create a mock token file
        std::fs::write(&token_path, "mock_vault_token").unwrap();

        let config = SecretsConfig {
            vault_enabled: true,
            vault_address: Some("https://vault.example.com".to_string()),
            vault_token_path: Some(token_path),
            env_fallback: false,
        };

        let manager = SecretsManager::new(config).unwrap();

        // Should successfully initialize (even though it's a mock)
        let result = manager.initialize().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_vault_missing_token() {
        let config = SecretsConfig {
            vault_enabled: true,
            vault_address: Some("https://vault.example.com".to_string()),
            vault_token_path: Some(PathBuf::from("/nonexistent/path")),
            env_fallback: false,
        };

        let manager = SecretsManager::new(config).unwrap();

        // Should fail to initialize due to missing token
        let result = manager.initialize().await;
        assert!(result.is_err());

        if let Err(SecurityError::SecretsError { message }) = result {
            assert!(message.contains("token file not found"));
        }
    }

    #[test]
    fn test_random_key_generation() {
        let key1 = SecretsManager::generate_random_key(32);
        let key2 = SecretsManager::generate_random_key(32);

        assert_eq!(key1.len(), 32);
        assert_eq!(key2.len(), 32);
        assert_ne!(key1, key2); // Should be different

        // Should contain only valid characters
        for ch in key1.chars() {
            assert!(ch.is_ascii_alphanumeric());
        }
    }

    #[test]
    fn test_jwt_secret_generation() {
        let secret = SecretsManager::generate_jwt_secret();
        assert_eq!(secret.len(), 64);
    }

    #[test]
    fn test_api_key_generation() {
        let key = SecretsManager::generate_api_key();
        assert!(key.starts_with("ak_"));
        assert_eq!(key.len(), 35); // "ak_" + 32 characters
    }

    #[test]
    fn test_encryption_key_generation() {
        let key = SecretsManager::generate_encryption_key();
        assert_eq!(key.len(), 32);
    }
}
