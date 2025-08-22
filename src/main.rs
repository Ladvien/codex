use anyhow::Result;
use clap::{Parser, Subcommand};
use codex_memory::application::*;
use codex_memory::setup::create_sample_env_file;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser)]
#[command(name = "codex-memory")]
#[command(about = "Agentic Memory System - Advanced memory management for AI agents")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the memory system server
    Start {
        /// Skip setup checks and start immediately
        #[arg(long)]
        skip_setup: bool,
    },
    /// Start MCP server (stdio mode for Claude Desktop)
    McpStdio {
        /// Skip setup checks and start immediately
        #[arg(long)]
        skip_setup: bool,
    },
    /// Setup the memory system
    Setup {
        /// Force setup even if already configured
        #[arg(long)]
        force: bool,
        /// Skip database setup
        #[arg(long)]
        skip_database: bool,
        /// Skip Ollama model setup
        #[arg(long)]
        skip_models: bool,
    },
    /// Check system health
    Health {
        /// Show detailed health information
        #[arg(long)]
        detailed: bool,
    },
    /// List available embedding models
    Models,
    /// Generate sample configuration file
    InitConfig,
    /// Database management commands
    Database {
        #[command(subcommand)]
        command: DatabaseCommands,
    },
    /// MCP configuration and diagnostic commands
    Mcp {
        #[command(subcommand)]
        command: McpCommands,
    },
    /// Server lifecycle management
    Manager {
        #[command(subcommand)]
        command: ManagerCommands,
    },
    /// Backup management commands
    Backup {
        #[command(subcommand)]
        command: BackupCommands,
    },
}

#[derive(Subcommand)]
enum DatabaseCommands {
    /// Setup database with pgvector extension
    Setup,
    /// Check database health and status
    Health,
    /// Run database migrations
    Migrate,
}

#[derive(Subcommand)]
enum McpCommands {
    /// Validate MCP configuration
    Validate,
    /// Generate diagnostic report
    Diagnose,
    /// Test MCP server connectivity
    Test,
    /// Generate MCP configuration templates
    Template {
        /// Configuration template type
        #[arg(long, default_value = "basic")]
        template_type: String,
        /// Output file path
        #[arg(long)]
        output: Option<String>,
    },
}

#[derive(Subcommand)]
enum ManagerCommands {
    /// Start server as a daemon/background process
    Start {
        /// Daemonize the process
        #[arg(long, default_value = "true")]
        daemon: bool,
        /// Custom PID file path
        #[arg(long)]
        pid_file: Option<String>,
        /// Custom log file path
        #[arg(long)]
        log_file: Option<String>,
    },
    /// Stop the running server
    Stop {
        /// Custom PID file path
        #[arg(long)]
        pid_file: Option<String>,
    },
    /// Restart the server
    Restart {
        /// Custom PID file path
        #[arg(long)]
        pid_file: Option<String>,
    },
    /// Show server status
    Status {
        /// Show detailed status
        #[arg(long)]
        detailed: bool,
    },
    /// Show server logs
    Logs {
        /// Number of lines to show (default: 50)
        #[arg(long, default_value = "50")]
        lines: usize,
        /// Follow log output
        #[arg(long)]
        follow: bool,
    },
    /// Install as system service
    Install {
        /// Service type (systemd, launchd, or windows)
        #[arg(long)]
        service_type: Option<String>,
    },
    /// Uninstall system service
    Uninstall,
}

#[derive(Subcommand)]
enum BackupCommands {
    /// Create a full backup
    Create,
    /// List all backups
    List,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Skip logging initialization for MCP stdio mode
    let is_mcp_stdio = matches!(cli.command, Some(Commands::McpStdio { .. }));

    if !is_mcp_stdio {
        // Initialize basic logging for CLI commands
        tracing_subscriber::registry()
            .with(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "info".into()),
            )
            .with(tracing_subscriber::fmt::layer())
            .init();
    }

    // Initialize application
    let app = Application::new().await?;
    app.initialize().await?;

    // Route commands to appropriate handlers
    match cli.command {
        Some(Commands::Setup { force, skip_database, skip_models }) => {
            let handler = SetupCommandHandler::new(app.container.clone());
            handler.run_setup(force, skip_database, skip_models).await
        }
        Some(Commands::Health { detailed }) => {
            let handler = HealthCommandHandler::new(app.container.clone());
            handler.run_health_check(detailed).await
        }
        Some(Commands::Models) => {
            let handler = ModelCommandHandler::new(app.container.clone());
            handler.list_models().await
        }
        Some(Commands::InitConfig) => {
            create_sample_env_file()?;
            Ok(())
        }
        Some(Commands::Database { command }) => {
            handle_database_command(command, &app).await
        }
        Some(Commands::Mcp { command }) => {
            handle_mcp_command(command, &app).await
        }
        Some(Commands::Manager { command }) => {
            handle_manager_command(command, &app).await
        }
        Some(Commands::Backup { command }) => {
            handle_backup_command(command, &app).await
        }
        Some(Commands::Start { skip_setup }) => {
            let handler = ServerCommandHandler::new(app.container.clone());
            handler.start_http(skip_setup).await
        }
        Some(Commands::McpStdio { skip_setup }) => {
            let handler = ServerCommandHandler::new(app.container.clone());
            handler.start_mcp_stdio(skip_setup).await
        }
        None => {
            // Default to starting the server
            let handler = ServerCommandHandler::new(app.container.clone());
            handler.start_http(false).await
        }
    }
}

async fn handle_database_command(command: DatabaseCommands, app: &Application) -> Result<()> {
    let handler = DatabaseCommandHandler::new(app.container.clone());
    match command {
        DatabaseCommands::Setup => handler.setup().await,
        DatabaseCommands::Health => handler.health().await,
        DatabaseCommands::Migrate => handler.migrate().await,
    }
}

async fn handle_mcp_command(command: McpCommands, app: &Application) -> Result<()> {
    let handler = McpCommandHandler::new(app.container.clone());
    match command {
        McpCommands::Validate => handler.validate().await,
        McpCommands::Diagnose => handler.diagnose().await,
        McpCommands::Test => handler.test().await,
        McpCommands::Template { template_type, output } => {
            handler.template(template_type, output).await
        }
    }
}

async fn handle_manager_command(command: ManagerCommands, app: &Application) -> Result<()> {
    let handler = ManagerCommandHandler::new(app.container.clone());
    match command {
        ManagerCommands::Start { daemon, pid_file, log_file } => {
            handler.start(daemon, pid_file, log_file).await
        }
        ManagerCommands::Stop { pid_file } => handler.stop(pid_file).await,
        ManagerCommands::Restart { pid_file } => handler.restart(pid_file).await,
        ManagerCommands::Status { detailed } => handler.status(detailed).await,
        ManagerCommands::Logs { lines, follow } => handler.logs(lines, follow).await,
        ManagerCommands::Install { service_type } => handler.install(service_type).await,
        ManagerCommands::Uninstall => handler.uninstall().await,
    }
}

async fn handle_backup_command(command: BackupCommands, app: &Application) -> Result<()> {
    let handler = BackupCommandHandler::new(app.container.clone());
    match command {
        BackupCommands::Create => handler.create_backup().await,
        BackupCommands::List => handler.list_backups().await,
    }
}