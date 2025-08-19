use crate::security::{Result, SecurityError, TlsConfig};
use axum_server::tls_rustls::RustlsConfig;
use tracing::{debug, info};

/// TLS configuration and certificate management
pub struct TlsManager {
    config: TlsConfig,
}

impl TlsManager {
    pub fn new(config: TlsConfig) -> Result<Self> {
        let manager = Self { config };

        if manager.config.enabled {
            manager.validate_config()?;
        }

        Ok(manager)
    }

    fn validate_config(&self) -> Result<()> {
        if !self.config.cert_path.exists() {
            return Err(SecurityError::TlsError {
                message: format!("TLS certificate not found: {:?}", self.config.cert_path),
            });
        }

        if !self.config.key_path.exists() {
            return Err(SecurityError::TlsError {
                message: format!("TLS private key not found: {:?}", self.config.key_path),
            });
        }

        if let Some(ca_path) = &self.config.client_ca_path {
            if !ca_path.exists() {
                return Err(SecurityError::TlsError {
                    message: format!("Client CA certificate not found: {ca_path:?}"),
                });
            }
        }

        Ok(())
    }

    /// Create Rustls config for Axum server
    pub async fn create_rustls_config(&self) -> Result<RustlsConfig> {
        if !self.config.enabled {
            return Err(SecurityError::TlsError {
                message: "TLS is not enabled".to_string(),
            });
        }

        info!(
            "Creating TLS configuration from cert: {:?}, key: {:?}",
            self.config.cert_path, self.config.key_path
        );

        RustlsConfig::from_pem_file(&self.config.cert_path, &self.config.key_path)
            .await
            .map_err(|e| SecurityError::TlsError {
                message: format!("Failed to create TLS config: {e}"),
            })
    }

    /// Check if TLS is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Check if mTLS (mutual TLS) is required
    pub fn requires_client_cert(&self) -> bool {
        self.config.enabled && self.config.require_client_cert
    }

    /// Get the TLS port
    pub fn get_port(&self) -> u16 {
        self.config.port
    }

    /// Check if this configuration supports mTLS
    pub fn supports_mtls(&self) -> bool {
        self.config.enabled && self.config.client_ca_path.is_some()
    }

    /// Get certificate information (for monitoring/diagnostics)
    pub fn get_cert_info(&self) -> TlsCertInfo {
        TlsCertInfo {
            cert_path: self.config.cert_path.clone(),
            key_path: self.config.key_path.clone(),
            client_ca_path: self.config.client_ca_path.clone(),
            enabled: self.config.enabled,
            mtls_enabled: self.config.require_client_cert,
            port: self.config.port,
        }
    }

    /// Validate that certificates exist and are readable
    pub fn validate_certificates(&self) -> Result<()> {
        if !self.config.enabled {
            debug!("TLS is disabled, skipping certificate validation");
            return Ok(());
        }

        self.validate_config()?;

        // Basic file readability check
        std::fs::read(&self.config.cert_path).map_err(|e| SecurityError::TlsError {
            message: format!("Cannot read certificate file: {e}"),
        })?;

        std::fs::read(&self.config.key_path).map_err(|e| SecurityError::TlsError {
            message: format!("Cannot read private key file: {e}"),
        })?;

        if let Some(ca_path) = &self.config.client_ca_path {
            std::fs::read(ca_path).map_err(|e| SecurityError::TlsError {
                message: format!("Cannot read client CA file: {e}"),
            })?;
        }

        info!("TLS certificates validated successfully");
        Ok(())
    }
}

/// TLS certificate information for monitoring
#[derive(Debug, Clone)]
pub struct TlsCertInfo {
    pub cert_path: std::path::PathBuf,
    pub key_path: std::path::PathBuf,
    pub client_ca_path: Option<std::path::PathBuf>,
    pub enabled: bool,
    pub mtls_enabled: bool,
    pub port: u16,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_tls_manager_disabled() {
        let config = TlsConfig {
            enabled: false,
            cert_path: PathBuf::from("/nonexistent"),
            key_path: PathBuf::from("/nonexistent"),
            port: 8443,
            require_client_cert: false,
            client_ca_path: None,
        };

        let manager = TlsManager::new(config).unwrap();
        assert!(!manager.is_enabled());
        assert!(!manager.requires_client_cert());
        assert_eq!(manager.get_port(), 8443);
    }

    #[test]
    fn test_tls_manager_invalid_cert_path() {
        let config = TlsConfig {
            enabled: true,
            cert_path: PathBuf::from("/nonexistent/cert.pem"),
            key_path: PathBuf::from("/nonexistent/key.pem"),
            port: 8443,
            require_client_cert: false,
            client_ca_path: None,
        };

        let result = TlsManager::new(config);
        assert!(result.is_err());

        if let Err(SecurityError::TlsError { message }) = result {
            assert!(message.contains("certificate not found"));
        }
    }

    #[test]
    fn test_tls_cert_info() {
        let config = TlsConfig {
            enabled: true,
            cert_path: PathBuf::from("/test/cert.pem"),
            key_path: PathBuf::from("/test/key.pem"),
            port: 8443,
            require_client_cert: true,
            client_ca_path: Some(PathBuf::from("/test/ca.pem")),
        };

        // This will fail validation but we can still test cert info
        let manager = TlsManager {
            config: config.clone(),
        };
        let cert_info = manager.get_cert_info();

        assert_eq!(cert_info.cert_path, config.cert_path);
        assert_eq!(cert_info.key_path, config.key_path);
        assert_eq!(cert_info.client_ca_path, config.client_ca_path);
        assert_eq!(cert_info.enabled, config.enabled);
        assert_eq!(cert_info.mtls_enabled, config.require_client_cert);
        assert_eq!(cert_info.port, config.port);
    }

    #[tokio::test]
    async fn test_create_rustls_config_disabled() {
        let config = TlsConfig {
            enabled: false,
            cert_path: PathBuf::from("/nonexistent"),
            key_path: PathBuf::from("/nonexistent"),
            port: 8443,
            require_client_cert: false,
            client_ca_path: None,
        };

        let manager = TlsManager::new(config).unwrap();
        let result = manager.create_rustls_config().await;
        assert!(result.is_err());

        if let Err(SecurityError::TlsError { message }) = result {
            assert!(message.contains("TLS is not enabled"));
        }
    }
}
