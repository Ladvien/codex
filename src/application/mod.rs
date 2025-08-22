use anyhow::Result;
use std::sync::Arc;

pub mod application_service;
pub mod command_handlers;
pub mod dependency_container;
pub mod lifecycle;

pub use application_service::ApplicationService;
pub use command_handlers::{
    BackupCommandHandler, DatabaseCommandHandler, HealthCommandHandler, ManagerCommandHandler,
    McpCommandHandler, ModelCommandHandler, ServerCommandHandler, SetupCommandHandler,
};
pub use dependency_container::DependencyContainer;
pub use lifecycle::ApplicationLifecycle;

/// Application layer - coordinates business operations without containing business logic
pub struct Application {
    pub container: Arc<DependencyContainer>,
    pub service: Arc<ApplicationService>,
    pub lifecycle: Arc<ApplicationLifecycle>,
}

impl Application {
    pub async fn new() -> Result<Self> {
        let container = Arc::new(DependencyContainer::new().await?);
        let service = Arc::new(ApplicationService::new(container.clone()));
        let lifecycle = Arc::new(ApplicationLifecycle::new(container.clone()));

        Ok(Self {
            container,
            service,
            lifecycle,
        })
    }

    pub async fn initialize(&self) -> Result<()> {
        self.lifecycle.initialize().await
    }

    pub async fn shutdown(&self) -> Result<()> {
        self.lifecycle.shutdown().await
    }
}
