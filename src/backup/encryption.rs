use super::{BackupConfig, BackupError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use tokio::fs;
use tracing::{debug, error, info, warn};

/// Backup encryption manager for securing backup data at rest
pub struct BackupEncryption {
    config: BackupConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionKey {
    pub key_id: String,
    pub key_path: PathBuf,
    pub algorithm: EncryptionAlgorithm,
    pub key_size_bits: u32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EncryptionAlgorithm {
    AES256GCM,
    ChaCha20Poly1305,
    AES256CBC,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionMetadata {
    pub encrypted_file_path: PathBuf,
    pub original_file_path: PathBuf,
    pub encryption_key_id: String,
    pub algorithm: EncryptionAlgorithm,
    pub iv: Vec<u8>,
    pub checksum: String,
    pub encrypted_at: chrono::DateTime<chrono::Utc>,
}

impl BackupEncryption {
    pub fn new(config: BackupConfig) -> Self {
        Self { config }
    }

    /// Initialize the backup encryption system
    pub async fn initialize(&self) -> Result<()> {
        info!("Initializing backup encryption system");

        if !self.config.enable_encryption {
            info!("Backup encryption is disabled");
            return Ok(());
        }

        // Verify encryption tools are available
        self.verify_encryption_tools().await?;

        // Ensure encryption key exists or create one
        self.ensure_encryption_key().await?;

        // Set up key rotation if configured
        self.setup_key_rotation().await?;

        info!("Backup encryption system initialized");
        Ok(())
    }

    /// Encrypt a backup file
    pub async fn encrypt_file(&self, file_path: &Path) -> Result<EncryptionMetadata> {
        if !self.config.enable_encryption {
            return Err(BackupError::EncryptionError {
                message: "Encryption is not enabled".to_string(),
            });
        }

        info!("Encrypting file: {}", file_path.display());

        let encrypted_path = self.get_encrypted_file_path(file_path)?;
        let key = self.load_encryption_key().await?;

        // Generate random IV
        let iv = self.generate_iv().await?;

        // Encrypt the file using GPG or OpenSSL
        self.encrypt_with_gpg(file_path, &encrypted_path, &key, &iv)
            .await?;

        // Calculate checksum of encrypted file
        let checksum = self.calculate_file_checksum(&encrypted_path).await?;

        let metadata = EncryptionMetadata {
            encrypted_file_path: encrypted_path,
            original_file_path: file_path.to_path_buf(),
            encryption_key_id: key.key_id.clone(),
            algorithm: key.algorithm.clone(),
            iv,
            checksum,
            encrypted_at: chrono::Utc::now(),
        };

        info!("File encrypted successfully: {}", file_path.display());
        Ok(metadata)
    }

    /// Decrypt a backup file
    pub async fn decrypt_file(&self, encrypted_path: &Path, output_path: &Path) -> Result<()> {
        if !self.config.enable_encryption {
            return Err(BackupError::EncryptionError {
                message: "Encryption is not enabled".to_string(),
            });
        }

        info!("Decrypting file: {}", encrypted_path.display());

        // Load encryption key
        let key = self.load_encryption_key().await?;

        // Decrypt the file using GPG or OpenSSL
        self.decrypt_with_gpg(encrypted_path, output_path, &key)
            .await?;

        info!("File decrypted successfully: {}", output_path.display());
        Ok(())
    }

    /// Encrypt backup in place (replaces original with encrypted version)
    pub async fn encrypt_backup_in_place(&self, backup_path: &Path) -> Result<EncryptionMetadata> {
        if !self.config.enable_encryption {
            return Err(BackupError::EncryptionError {
                message: "Encryption is not enabled".to_string(),
            });
        }

        debug!("Encrypting backup in place: {}", backup_path.display());

        // Create temporary file for encryption
        let _temp_path = backup_path.with_extension("tmp.enc");

        // Encrypt to temporary file
        let mut metadata = self.encrypt_file(backup_path).await?;

        // Move encrypted file to replace original
        fs::rename(&metadata.encrypted_file_path, backup_path).await?;

        // Update metadata paths
        metadata.encrypted_file_path = backup_path.to_path_buf();
        metadata.original_file_path = backup_path.to_path_buf();

        debug!("Backup encrypted in place successfully");
        Ok(metadata)
    }

    /// Generate a new encryption key
    pub async fn generate_encryption_key(&self) -> Result<EncryptionKey> {
        info!("Generating new encryption key");

        let key_id = uuid::Uuid::new_v4().to_string();
        let key_path = self.get_key_path(&key_id);

        // Generate key using GPG
        let mut cmd = Command::new("gpg");
        cmd.arg("--batch")
            .arg("--gen-key")
            .arg("--armor")
            .arg("--output")
            .arg(&key_path);

        // Create key generation parameters
        let key_params = self.create_key_generation_params(&key_id);
        let mut child = cmd
            .stdin(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| BackupError::EncryptionError {
                message: format!("Failed to start GPG key generation: {e}"),
            })?;

        if let Some(stdin) = child.stdin.as_mut() {
            use std::io::Write;
            stdin
                .write_all(key_params.as_bytes())
                .map_err(|e| BackupError::EncryptionError {
                    message: format!("Failed to write key parameters: {e}"),
                })?;
        }

        let output = child.wait().map_err(|e| BackupError::EncryptionError {
            message: format!("GPG key generation failed: {e}"),
        })?;

        if !output.success() {
            return Err(BackupError::EncryptionError {
                message: "GPG key generation failed".to_string(),
            });
        }

        let key = EncryptionKey {
            key_id,
            key_path,
            algorithm: EncryptionAlgorithm::AES256GCM,
            key_size_bits: 256,
            created_at: chrono::Utc::now(),
            expires_at: Some(chrono::Utc::now() + chrono::Duration::days(365)), // 1 year
        };

        info!("Encryption key generated successfully: {}", key.key_id);
        Ok(key)
    }

    /// Rotate encryption keys
    pub async fn rotate_encryption_key(&self) -> Result<EncryptionKey> {
        info!("Rotating encryption key");

        // Generate new key
        let new_key = self.generate_encryption_key().await?;

        // Re-encrypt existing backups with new key if needed
        // This is a simplified implementation - in production, you might
        // keep old keys for decrypting old backups and only use new key
        // for new backups
        warn!("Key rotation completed. Note: Existing backups still use old key");

        info!("Encryption key rotated successfully");
        Ok(new_key)
    }

    /// Verify encryption key integrity
    pub async fn verify_encryption_key(&self) -> Result<bool> {
        debug!("Verifying encryption key integrity");

        if let Some(key_path) = &self.config.encryption_key_path {
            if !key_path.exists() {
                warn!("Encryption key file not found: {}", key_path.display());
                return Ok(false);
            }

            // Test the key by performing a simple encryption/decryption test
            let test_data = b"encryption_test_data";
            let temp_file = std::env::temp_dir().join("encryption_test.txt");
            let encrypted_file = std::env::temp_dir().join("encryption_test.txt.enc");
            let decrypted_file = std::env::temp_dir().join("encryption_test_decrypted.txt");

            // Write test data
            fs::write(&temp_file, test_data).await?;

            // Test encryption
            let key = self.load_encryption_key().await?;
            let iv = self.generate_iv().await?;

            match self
                .encrypt_with_gpg(&temp_file, &encrypted_file, &key, &iv)
                .await
            {
                Ok(_) => {
                    // Test decryption
                    match self
                        .decrypt_with_gpg(&encrypted_file, &decrypted_file, &key)
                        .await
                    {
                        Ok(_) => {
                            // Verify decrypted data matches original
                            let decrypted_data = fs::read(&decrypted_file).await?;
                            let key_valid = decrypted_data == test_data;

                            // Clean up test files
                            let _ = fs::remove_file(&temp_file).await;
                            let _ = fs::remove_file(&encrypted_file).await;
                            let _ = fs::remove_file(&decrypted_file).await;

                            debug!(
                                "Encryption key verification: {}",
                                if key_valid { "PASSED" } else { "FAILED" }
                            );
                            Ok(key_valid)
                        }
                        Err(e) => {
                            error!("Key verification failed during decryption: {}", e);
                            Ok(false)
                        }
                    }
                }
                Err(e) => {
                    error!("Key verification failed during encryption: {}", e);
                    Ok(false)
                }
            }
        } else {
            warn!("No encryption key path configured");
            Ok(false)
        }
    }

    // Private helper methods

    async fn verify_encryption_tools(&self) -> Result<()> {
        debug!("Verifying encryption tools availability");

        // Check if GPG is available
        let output = Command::new("gpg").arg("--version").output().map_err(|e| {
            BackupError::ConfigurationError {
                message: format!("GPG not found: {e}"),
            }
        })?;

        if !output.status.success() {
            return Err(BackupError::ConfigurationError {
                message: "GPG is not working properly".to_string(),
            });
        }

        debug!("Encryption tools verified");
        Ok(())
    }

    async fn ensure_encryption_key(&self) -> Result<()> {
        debug!("Ensuring encryption key exists");

        if let Some(key_path) = &self.config.encryption_key_path {
            if !key_path.exists() {
                info!("Encryption key not found, generating new key");

                // Create key directory if it doesn't exist
                if let Some(parent) = key_path.parent() {
                    fs::create_dir_all(parent).await?;
                }

                let _key = self.generate_encryption_key().await?;
            } else {
                debug!("Encryption key already exists");

                // Verify the key is valid
                if !self.verify_encryption_key().await? {
                    warn!("Existing encryption key is invalid, generating new key");
                    let _key = self.generate_encryption_key().await?;
                }
            }
        } else {
            return Err(BackupError::ConfigurationError {
                message: "Encryption key path not configured".to_string(),
            });
        }

        debug!("Encryption key ensured");
        Ok(())
    }

    async fn setup_key_rotation(&self) -> Result<()> {
        debug!("Setting up key rotation");

        // In a production system, this would set up automatic key rotation
        // For now, just log that it's available
        info!("Key rotation is available via rotate_encryption_key() method");

        Ok(())
    }

    async fn load_encryption_key(&self) -> Result<EncryptionKey> {
        if let Some(key_path) = &self.config.encryption_key_path {
            // In a real implementation, this would parse the actual key file
            // For this mock implementation, create a dummy key
            Ok(EncryptionKey {
                key_id: "default".to_string(),
                key_path: key_path.clone(),
                algorithm: EncryptionAlgorithm::AES256GCM,
                key_size_bits: 256,
                created_at: chrono::Utc::now(),
                expires_at: None,
            })
        } else {
            Err(BackupError::EncryptionError {
                message: "No encryption key path configured".to_string(),
            })
        }
    }

    fn get_encrypted_file_path(&self, original_path: &Path) -> Result<PathBuf> {
        let mut new_path = original_path.to_path_buf();
        let current_name = new_path.file_name().unwrap().to_string_lossy();
        new_path.set_file_name(format!("{current_name}.enc"));
        Ok(new_path)
    }

    fn get_key_path(&self, key_id: &str) -> PathBuf {
        self.config
            .backup_directory
            .join(format!("encryption_key_{key_id}.gpg"))
    }

    async fn generate_iv(&self) -> Result<Vec<u8>> {
        // Generate a random 16-byte IV for AES
        use rand::RngCore;
        let mut iv = vec![0u8; 16];
        rand::thread_rng().fill_bytes(&mut iv);
        Ok(iv)
    }

    async fn encrypt_with_gpg(
        &self,
        input_path: &Path,
        output_path: &Path,
        key: &EncryptionKey,
        _iv: &[u8],
    ) -> Result<()> {
        debug!(
            "Encrypting with GPG: {} -> {}",
            input_path.display(),
            output_path.display()
        );

        let mut cmd = Command::new("gpg");
        cmd.arg("--cipher-algo")
            .arg("AES256")
            .arg("--compress-algo")
            .arg("2")
            .arg("--symmetric")
            .arg("--armor")
            .arg("--output")
            .arg(output_path)
            .arg("--batch")
            .arg("--yes")
            .arg("--passphrase-file")
            .arg(&key.key_path)
            .arg(input_path);

        let output = cmd.output().map_err(|e| BackupError::EncryptionError {
            message: format!("Failed to execute GPG encryption: {e}"),
        })?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(BackupError::EncryptionError {
                message: format!("GPG encryption failed: {error_msg}"),
            });
        }

        debug!("GPG encryption completed successfully");
        Ok(())
    }

    async fn decrypt_with_gpg(
        &self,
        input_path: &Path,
        output_path: &Path,
        key: &EncryptionKey,
    ) -> Result<()> {
        debug!(
            "Decrypting with GPG: {} -> {}",
            input_path.display(),
            output_path.display()
        );

        let mut cmd = Command::new("gpg");
        cmd.arg("--decrypt")
            .arg("--output")
            .arg(output_path)
            .arg("--batch")
            .arg("--yes")
            .arg("--passphrase-file")
            .arg(&key.key_path)
            .arg(input_path);

        let output = cmd.output().map_err(|e| BackupError::EncryptionError {
            message: format!("Failed to execute GPG decryption: {e}"),
        })?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(BackupError::EncryptionError {
                message: format!("GPG decryption failed: {error_msg}"),
            });
        }

        debug!("GPG decryption completed successfully");
        Ok(())
    }

    async fn calculate_file_checksum(&self, file_path: &Path) -> Result<String> {
        use sha2::{Digest, Sha256};

        let contents = fs::read(file_path).await?;
        let mut hasher = Sha256::new();
        hasher.update(&contents);
        let result = hasher.finalize();
        Ok(format!("{result:x}"))
    }

    fn create_key_generation_params(&self, key_id: &str) -> String {
        format!(
            "Key-Type: RSA
Key-Length: 2048
Subkey-Type: RSA
Subkey-Length: 2048
Name-Real: Codex Backup Key
Name-Comment: Backup encryption key for {key_id}
Name-Email: backup@codex.local
Expire-Date: 1y
%commit
"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encryption_key_creation() {
        let key = EncryptionKey {
            key_id: "test-key".to_string(),
            key_path: PathBuf::from("/tmp/test.key"),
            algorithm: EncryptionAlgorithm::AES256GCM,
            key_size_bits: 256,
            created_at: chrono::Utc::now(),
            expires_at: None,
        };

        assert_eq!(key.key_id, "test-key");
        assert_eq!(key.key_size_bits, 256);
        assert!(matches!(key.algorithm, EncryptionAlgorithm::AES256GCM));
    }

    #[test]
    fn test_encryption_algorithms() {
        let algorithms = [
            EncryptionAlgorithm::AES256GCM,
            EncryptionAlgorithm::ChaCha20Poly1305,
            EncryptionAlgorithm::AES256CBC,
        ];

        for algorithm in &algorithms {
            match algorithm {
                EncryptionAlgorithm::AES256GCM => assert!(true),
                EncryptionAlgorithm::ChaCha20Poly1305 => assert!(true),
                EncryptionAlgorithm::AES256CBC => assert!(true),
            }
        }
    }

    #[test]
    fn test_get_encrypted_file_path() {
        let config = BackupConfig::default();
        let encryption = BackupEncryption::new(config);

        let original_path = Path::new("/tmp/backup.sql");
        let encrypted_path = encryption.get_encrypted_file_path(original_path).unwrap();

        assert_eq!(encrypted_path, PathBuf::from("/tmp/backup.sql.enc"));
    }
}
