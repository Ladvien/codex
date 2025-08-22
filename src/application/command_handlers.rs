use crate::application::DependencyContainer;
use anyhow::Result;
use std::sync::Arc;
use tracing::{error, info, warn};

/// Clean separation of command handling logic from main.rs
pub struct SetupCommandHandler {
    container: Arc<DependencyContainer>,
}

impl SetupCommandHandler {
    pub fn new(container: Arc<DependencyContainer>) -> Self {
        Self { container }
    }

    pub async fn run_setup(
        &self,
        force: bool,
        skip_database: bool,
        skip_models: bool,
    ) -> Result<()> {
        info!("🚀 Starting Agentic Memory System setup...");

        if !force {
            match self.container.health_check().await {
                Ok(true) => {
                    info!(
                        "✅ System appears to be already set up. Use --force to run setup anyway."
                    );
                    return Ok(());
                }
                _ => {
                    info!("🔧 System needs setup, proceeding...");
                }
            }
        }

        if !skip_models {
            self.container.setup_manager.run_setup().await?;
        }

        if !skip_database {
            self.container.database_setup.setup().await?;
        }

        info!("🎉 Setup completed successfully!");
        info!("💡 You can now start the server with: codex-memory start");

        Ok(())
    }
}

pub struct HealthCommandHandler {
    container: Arc<DependencyContainer>,
}

impl HealthCommandHandler {
    pub fn new(container: Arc<DependencyContainer>) -> Self {
        Self { container }
    }

    pub async fn run_health_check(&self, detailed: bool) -> Result<()> {
        info!("🏥 Running system health check...");

        if detailed {
            let health = self.container.health_checker.check_system_health().await?;

            info!("📊 System Health: {:?}", health.status);
            info!("⏱️  Uptime: {} seconds", health.uptime_seconds);
            info!(
                "💾 Memory Usage: {} MB",
                health.memory_usage_bytes / (1024 * 1024)
            );
            info!("🔥 CPU Usage: {:.1}%", health.cpu_usage_percent);

            for (component, component_health) in &health.components {
                match component_health.status {
                    crate::monitoring::HealthStatus::Healthy => {
                        info!("✅ {}: Healthy", component);
                    }
                    crate::monitoring::HealthStatus::Degraded => {
                        warn!(
                            "⚠️  {}: Degraded - {:?}",
                            component, component_health.message
                        );
                    }
                    crate::monitoring::HealthStatus::Unhealthy => {
                        error!(
                            "❌ {}: Unhealthy - {:?}",
                            component, component_health.message
                        );
                    }
                }
            }
        } else {
            match self.container.health_check().await {
                Ok(true) => info!("✅ System is healthy"),
                _ => error!("❌ System health check failed"),
            }
        }

        Ok(())
    }
}

pub struct ModelCommandHandler {
    container: Arc<DependencyContainer>,
}

impl ModelCommandHandler {
    pub fn new(container: Arc<DependencyContainer>) -> Self {
        Self { container }
    }

    pub async fn list_models(&self) -> Result<()> {
        self.container.setup_manager.list_available_models().await
    }
}

pub struct DatabaseCommandHandler {
    container: Arc<DependencyContainer>,
}

impl DatabaseCommandHandler {
    pub fn new(container: Arc<DependencyContainer>) -> Self {
        Self { container }
    }

    pub async fn setup(&self) -> Result<()> {
        self.container.database_setup.setup().await
    }

    pub async fn health(&self) -> Result<()> {
        let health = self.container.database_setup.health_check().await?;
        info!("📊 Database Health: {}", health.status_summary());
        info!(
            "   - Connectivity: {}",
            if health.connectivity { "✅" } else { "❌" }
        );
        info!(
            "   - pgvector: {}",
            if health.pgvector_installed {
                "✅"
            } else {
                "❌"
            }
        );
        info!(
            "   - Schema: {}",
            if health.schema_ready { "✅" } else { "❌" }
        );
        info!("   - Memory count: {}", health.memory_count);
        Ok(())
    }

    pub async fn migrate(&self) -> Result<()> {
        error!("❌ Migration support not available in this build");
        info!("💡 Use direct SQL or database tools to run migrations");
        Err(anyhow::anyhow!("Migration support not compiled in"))
    }
}

pub struct McpCommandHandler {
    container: Arc<DependencyContainer>,
}

impl McpCommandHandler {
    pub fn new(container: Arc<DependencyContainer>) -> Self {
        Self { container }
    }

    pub async fn validate(&self) -> Result<()> {
        info!("🔍 Validating MCP configuration...");
        match self.container.config.validate_mcp_environment() {
            Ok(_) => {
                info!("✅ MCP configuration is valid");
                info!(
                    "   - Database: {}",
                    self.container.config.safe_database_url()
                );
                info!(
                    "   - Embedding: {} ({})",
                    self.container.config.embedding.provider, self.container.config.embedding.model
                );
                info!("   - HTTP Port: {}", self.container.config.http_port);
                if let Some(mcp_port) = self.container.config.mcp_port {
                    info!("   - MCP Port: {}", mcp_port);
                }
                Ok(())
            }
            Err(e) => {
                error!("❌ MCP configuration validation failed: {}", e);
                std::process::exit(1);
            }
        }
    }

    pub async fn diagnose(&self) -> Result<()> {
        info!("🔍 Generating MCP diagnostic report...");
        let report = self.container.config.create_diagnostic_report();
        println!("{}", report);
        Ok(())
    }

    pub async fn test(&self) -> Result<()> {
        info!("🧪 Testing MCP server connectivity...");
        self.container.config.validate_mcp_environment()?;

        // Test database
        info!("Testing database connectivity...");
        match self.container.database_setup.health_check().await {
            Ok(health) => {
                info!(
                    "✅ Database: {}",
                    if health.connectivity {
                        "Connected"
                    } else {
                        "Failed"
                    }
                );
            }
            Err(e) => {
                error!("❌ Database: Connection failed - {}", e);
            }
        }

        // Test embedding service
        info!("Testing embedding service...");
        match self.container.setup_manager.quick_health_check().await {
            Ok(_) => info!("✅ Embedding service: Available"),
            Err(e) => error!("❌ Embedding service: {}", e),
        }

        info!("🎉 MCP connectivity test completed");
        Ok(())
    }

    pub async fn template(&self, template_type: String, output: Option<String>) -> Result<()> {
        info!(
            "📋 Generating MCP configuration template: {}",
            template_type
        );

        let template_content = match template_type.as_str() {
            "basic" => self.generate_basic_template(),
            "production" => self.generate_production_template(),
            "development" => self.generate_development_template(),
            _ => {
                error!(
                    "❌ Unknown template type: {}. Available: basic, production, development",
                    template_type
                );
                return Err(anyhow::anyhow!("Invalid template type"));
            }
        };

        match output {
            Some(path) => {
                std::fs::write(&path, template_content)?;
                info!("✅ Template written to: {}", path);
            }
            None => {
                println!("{}", template_content);
            }
        }
        Ok(())
    }

    fn generate_basic_template(&self) -> String {
        r#"{
  "mcpServers": {
    "agentic-memory": {
      "command": "/path/to/codex-memory",
      "args": ["mcp-stdio"],
      "env": {
        "DATABASE_URL": "postgresql://username:password@localhost:5432/memory_db",
        "EMBEDDING_PROVIDER": "ollama",
        "EMBEDDING_BASE_URL": "http://localhost:11434",
        "EMBEDDING_MODEL": "nomic-embed-text",
        "LOG_LEVEL": "info"
      }
    }
  }
}"#
        .to_string()
    }

    fn generate_production_template(&self) -> String {
        r#"{
  "mcpServers": {
    "agentic-memory": {
      "command": "/usr/local/bin/codex-memory",
      "args": ["mcp-stdio"],
      "env": {
        "DATABASE_URL": "${DATABASE_URL}",
        "EMBEDDING_PROVIDER": "${EMBEDDING_PROVIDER:-ollama}",
        "EMBEDDING_BASE_URL": "${EMBEDDING_BASE_URL:-http://localhost:11434}",
        "EMBEDDING_MODEL": "${EMBEDDING_MODEL:-nomic-embed-text}",
        "OPENAI_API_KEY": "${OPENAI_API_KEY}",
        "LOG_LEVEL": "warn",
        "MAX_DB_CONNECTIONS": "20",
        "ENABLE_METRICS": "true"
      }
    }
  }
}"#
        .to_string()
    }

    fn generate_development_template(&self) -> String {
        r#"{
  "mcpServers": {
    "agentic-memory-dev": {
      "command": "./target/debug/codex-memory",
      "args": ["mcp-stdio"],
      "env": {
        "DATABASE_URL": "postgresql://dev_user:dev_password@localhost:5432/memory_dev_db",
        "EMBEDDING_PROVIDER": "mock",
        "EMBEDDING_MODEL": "mock-model",
        "LOG_LEVEL": "debug",
        "ENABLE_METRICS": "false"
      }
    }
  }
}"#
        .to_string()
    }
}

pub struct ManagerCommandHandler {
    container: Arc<DependencyContainer>,
}

impl ManagerCommandHandler {
    pub fn new(container: Arc<DependencyContainer>) -> Self {
        Self { container }
    }

    pub async fn start(
        &self,
        daemon: bool,
        pid_file: Option<String>,
        log_file: Option<String>,
    ) -> Result<()> {
        let manager = if pid_file.is_some() || log_file.is_some() {
            crate::manager::ServerManager::with_paths(pid_file, log_file)
        } else {
            crate::manager::ServerManager::new()
        };
        manager.start_daemon(daemon).await
    }

    pub async fn stop(&self, pid_file: Option<String>) -> Result<()> {
        let manager = if let Some(pid_file) = pid_file {
            crate::manager::ServerManager::with_paths(Some(pid_file), None)
        } else {
            crate::manager::ServerManager::new()
        };
        manager.stop().await
    }

    pub async fn restart(&self, pid_file: Option<String>) -> Result<()> {
        let manager = if let Some(pid_file) = pid_file {
            crate::manager::ServerManager::with_paths(Some(pid_file), None)
        } else {
            crate::manager::ServerManager::new()
        };
        manager.restart().await
    }

    pub async fn status(&self, detailed: bool) -> Result<()> {
        self.container.server_manager.status(detailed).await
    }

    pub async fn logs(&self, lines: usize, follow: bool) -> Result<()> {
        self.container.server_manager.show_logs(lines, follow).await
    }

    pub async fn install(&self, service_type: Option<String>) -> Result<()> {
        self.container
            .server_manager
            .install_service(service_type)
            .await
    }

    pub async fn uninstall(&self) -> Result<()> {
        self.container.server_manager.uninstall_service().await
    }
}

pub struct ServerCommandHandler {
    container: Arc<DependencyContainer>,
}

impl ServerCommandHandler {
    pub fn new(container: Arc<DependencyContainer>) -> Self {
        Self { container }
    }

    pub async fn start_http(&self, skip_setup: bool) -> Result<()> {
        info!("🚀 Starting HTTP server...");

        if !skip_setup {
            self.validate_system().await?;
        }

        // The actual HTTP server startup logic would be here
        // This is kept separate from main.rs business logic
        info!("HTTP server would start here with container dependencies");

        Ok(())
    }

    pub async fn start_mcp_stdio(&self, skip_setup: bool) -> Result<()> {
        info!("🚀 Starting MCP stdio server...");

        if !skip_setup {
            self.container.config.validate()?;
        }

        let mut mcp_server = self.container.create_mcp_server().await?;
        mcp_server.start().await?;

        Ok(())
    }

    async fn validate_system(&self) -> Result<()> {
        info!("🔍 Running pre-flight checks...");

        match self.container.setup_manager.quick_health_check().await {
            Ok(_) => info!("✅ System health check passed"),
            Err(e) => {
                error!("❌ System health check failed: {}", e);
                info!("💡 Try running: codex-memory setup");
                return Err(e);
            }
        }

        match self.container.database_setup.health_check().await {
            Ok(health) => {
                if health.is_healthy() {
                    info!("✅ Database health check passed");
                } else {
                    error!(
                        "❌ Database health check failed: {}",
                        health.status_summary()
                    );
                    info!("💡 Try running: codex-memory database setup");
                    return Err(anyhow::anyhow!("Database not ready"));
                }
            }
            Err(e) => {
                error!("❌ Database connectivity failed: {}", e);
                return Err(e);
            }
        }

        Ok(())
    }
}

pub struct BackupCommandHandler {
    container: Arc<DependencyContainer>,
}

impl BackupCommandHandler {
    pub fn new(container: Arc<DependencyContainer>) -> Self {
        Self { container }
    }

    pub async fn create_backup(&self) -> Result<()> {
        if let Some(ref backup_manager) = self.container.backup_manager {
            let metadata = backup_manager.create_full_backup().await?;
            info!("✅ Backup created: {}", metadata.id);
            Ok(())
        } else {
            Err(anyhow::anyhow!("Backup functionality is not enabled"))
        }
    }

    pub async fn list_backups(&self) -> Result<()> {
        if let Some(ref backup_manager) = self.container.backup_manager {
            let stats = backup_manager.get_backup_statistics().await?;
            info!("📊 Total backups: {}", stats.total_backups);
            info!(
                "✅ Successful (last 7 days): {}",
                stats.successful_backups_last_7_days
            );
            info!(
                "❌ Failed (last 7 days): {}",
                stats.failed_backups_last_7_days
            );
            Ok(())
        } else {
            Err(anyhow::anyhow!("Backup functionality is not enabled"))
        }
    }
}
