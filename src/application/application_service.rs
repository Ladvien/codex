use crate::application::DependencyContainer;
use anyhow::Result;
use std::sync::Arc;
use tracing::info;

/// Application service coordinates high-level business operations
/// without containing business logic itself
pub struct ApplicationService {
    container: Arc<DependencyContainer>,
}

impl ApplicationService {
    pub fn new(container: Arc<DependencyContainer>) -> Self {
        Self { container }
    }

    /// Get system health status
    pub async fn get_health_status(&self) -> Result<bool> {
        self.container.health_check().await
    }

    /// Get system configuration summary
    pub fn get_config_summary(&self) -> ConfigSummary {
        ConfigSummary {
            database_url: self.container.config.safe_database_url(),
            embedding_provider: self.container.config.embedding.provider.clone(),
            embedding_model: self.container.config.embedding.model.clone(),
            http_port: self.container.config.http_port,
            mcp_port: self.container.config.mcp_port,
            tier_manager_enabled: self.container.config.tier_manager.enabled,
            backup_enabled: self.container.config.backup.enabled,
        }
    }

    /// Initialize all managed services
    pub async fn initialize_services(&self) -> Result<()> {
        info!("🔧 Initializing application services...");

        // Initialize tier manager if enabled
        if let Some(ref tier_manager) = self.container.tier_manager {
            tier_manager.start().await?;
            info!("✅ Tier management service started");
        }

        // Initialize backup manager if enabled
        if let Some(ref backup_manager) = self.container.backup_manager {
            backup_manager.initialize().await?;
            info!("✅ Backup service initialized");
        }

        // Initialize harvester service
        if let Some(ref _harvester) = self.container.harvester_service {
            // TODO: Implement proper start method
            // harvester.start().await?;
            info!("✅ Silent harvester service started (placeholder)");
        }

        info!("🎉 All application services initialized successfully");
        Ok(())
    }

    /// Shutdown all managed services gracefully
    pub async fn shutdown_services(&self) -> Result<()> {
        info!("🛑 Shutting down application services...");

        // Shutdown in reverse order
        if let Some(ref _harvester) = self.container.harvester_service {
            // TODO: Implement proper stop method
            // harvester.stop().await?;
            info!("✅ Silent harvester service stopped (placeholder)");
        }

        if let Some(ref tier_manager) = self.container.tier_manager {
            tier_manager.stop().await;
            info!("✅ Tier management service stopped");
        }

        info!("🎉 All application services shutdown gracefully");
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct ConfigSummary {
    pub database_url: String,
    pub embedding_provider: String,
    pub embedding_model: String,
    pub http_port: u16,
    pub mcp_port: Option<u16>,
    pub tier_manager_enabled: bool,
    pub backup_enabled: bool,
}
