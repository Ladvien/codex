use crate::application::DependencyContainer;
use anyhow::Result;
use std::sync::Arc;
use tokio::signal;
use tracing::{error, info};

/// Manages application lifecycle events and graceful shutdown
pub struct ApplicationLifecycle {
    container: Arc<DependencyContainer>,
}

impl ApplicationLifecycle {
    pub fn new(container: Arc<DependencyContainer>) -> Self {
        Self { container }
    }

    /// Initialize the application
    pub async fn initialize(&self) -> Result<()> {
        info!("🚀 Initializing application...");

        // Validate configuration
        self.container.config.validate()?;

        // Run health checks
        if !self.container.health_check().await? {
            return Err(anyhow::anyhow!("Initial health check failed"));
        }

        info!("✅ Application initialized successfully");
        Ok(())
    }

    /// Graceful shutdown
    pub async fn shutdown(&self) -> Result<()> {
        info!("🛑 Initiating graceful shutdown...");

        // Stop all services gracefully
        if let Some(ref _harvester) = self.container.harvester_service {
            // TODO: Implement proper stop method
            // if let Err(e) = harvester.stop().await {
            //     error!("Error stopping harvester service: {}", e);
            // }
        }

        if let Some(ref tier_manager) = self.container.tier_manager {
            // TierManager stop returns () not Result, so no error handling needed
            tier_manager.stop().await;
        }

        info!("🎉 Graceful shutdown completed");
        Ok(())
    }

    /// Wait for shutdown signals
    pub async fn wait_for_shutdown(&self) {
        let ctrl_c = async {
            if let Err(e) = signal::ctrl_c().await {
                error!("Failed to install Ctrl+C handler: {}", e);
            }
        };

        #[cfg(unix)]
        let terminate = async {
            match signal::unix::signal(signal::unix::SignalKind::terminate()) {
                Ok(mut stream) => {
                    stream.recv().await;
                }
                Err(e) => {
                    error!("Failed to install terminate signal handler: {}", e);
                }
            }
        };

        #[cfg(not(unix))]
        let terminate = std::future::pending::<()>();

        tokio::select! {
            _ = ctrl_c => {},
            _ = terminate => {},
        }

        info!("Shutdown signal received");
    }
}
