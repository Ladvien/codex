use anyhow::Result;
use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use clap::{Parser, Subcommand};
use codex_memory::{
    memory::{
        connection::create_pool,
        models::{CreateMemoryRequest, MemoryTier, SearchRequest},
        repository::MemoryStatistics,
    },
    mcp_server::{MCPServer, MCPServerConfig},
    setup::create_sample_env_file,
    Config, DatabaseSetup, MemoryRepository, SetupManager, SimpleEmbedder,
};
// migration functionality removed for crates.io version
use prometheus::{Encoder, TextEncoder};
use serde_json::json;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::signal;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::{error, info, warn};
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

#[derive(Clone)]
struct AppState {
    config: Config,
    repository: Arc<MemoryRepository>,
    embedder: Arc<SimpleEmbedder>,
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

    match cli.command {
        Some(Commands::Setup {
            force,
            skip_database,
            skip_models,
        }) => run_setup(force, skip_database, skip_models).await,
        Some(Commands::Health { detailed }) => run_health_check(detailed).await,
        Some(Commands::Models) => list_models().await,
        Some(Commands::InitConfig) => {
            create_sample_env_file()?;
            Ok(())
        }
        Some(Commands::Database { command }) => handle_database_command(command).await,
        Some(Commands::Mcp { command }) => handle_mcp_command(command).await,
        Some(Commands::Manager { command }) => handle_manager_command(command).await,
        Some(Commands::Start { skip_setup }) => start_server(skip_setup).await,
        Some(Commands::McpStdio { skip_setup }) => start_mcp_stdio(skip_setup).await,
        None => {
            // Default to starting the server
            start_server(false).await
        }
    }
}

async fn run_setup(force: bool, skip_database: bool, skip_models: bool) -> Result<()> {
    info!("🚀 Starting Agentic Memory System setup...");

    // Load configuration
    let config = Config::from_env().unwrap_or_else(|_| {
        info!("⚠️  No configuration found, using defaults");
        Config::default()
    });

    if !force {
        // Check if system appears to already be set up
        let setup_manager = SetupManager::new(config.clone());
        match setup_manager.quick_health_check().await {
            Ok(_) => {
                info!("✅ System appears to be already set up. Use --force to run setup anyway.");
                return Ok(());
            }
            Err(_) => {
                info!("🔧 System needs setup, proceeding...");
            }
        }
    }

    // Run setup components
    if !skip_models {
        let setup_manager = SetupManager::new(config.clone());
        setup_manager.run_setup().await?;
    }

    if !skip_database {
        let db_setup = DatabaseSetup::new(config.database_url.clone());
        db_setup.setup().await?;
    }

    info!("🎉 Setup completed successfully!");
    info!("💡 You can now start the server with: codex-memory start");

    Ok(())
}

async fn run_health_check(detailed: bool) -> Result<()> {
    let config = Config::from_env()?;

    info!("🏥 Running system health check...");

    let setup_manager = SetupManager::new(config.clone());
    let db_setup = DatabaseSetup::new(config.database_url.clone());

    // Quick health check
    let _ = setup_manager.quick_health_check().await;

    if detailed {
        info!("🔍 Running detailed health checks...");

        // Database health
        match db_setup.health_check().await {
            Ok(health) => {
                info!("📊 Database: {}", health.status_summary());
                if detailed {
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
                }
            }
            Err(e) => {
                error!("❌ Database health check failed: {}", e);
            }
        }

        // Embedding health
        let embedder = match config.embedding.provider.as_str() {
            "ollama" => SimpleEmbedder::new_ollama(
                config.embedding.base_url.clone(),
                config.embedding.model.clone(),
            ),
            "openai" => SimpleEmbedder::new(config.embedding.api_key.clone()),
            _ => SimpleEmbedder::new_mock(),
        };

        match embedder.health_check().await {
            Ok(health) => {
                info!(
                    "🧠 Embeddings: {} ({}ms)",
                    health.status, health.response_time_ms
                );
                if detailed {
                    info!("   - Model: {}", health.model);
                    info!("   - Provider: {}", health.provider);
                    info!("   - Dimensions: {}", health.embedding_dimensions);
                }
            }
            Err(e) => {
                error!("❌ Embedding health check failed: {}", e);
            }
        }
    }

    Ok(())
}

async fn list_models() -> Result<()> {
    let config = Config::from_env().unwrap_or_default();
    let setup_manager = SetupManager::new(config);

    setup_manager.list_available_models().await
}

async fn handle_database_command(command: DatabaseCommands) -> Result<()> {
    let config = Config::from_env()?;
    let db_setup = DatabaseSetup::new(config.database_url.clone());

    match command {
        DatabaseCommands::Setup => db_setup.setup().await,
        DatabaseCommands::Health => {
            let health = db_setup.health_check().await?;
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
        DatabaseCommands::Migrate => {
            error!("❌ Migration support not available in this build");
            info!("💡 Use direct SQL or database tools to run migrations");
            Err(anyhow::anyhow!("Migration support not compiled in"))
        }
    }
}

async fn handle_mcp_command(command: McpCommands) -> Result<()> {
    match command {
        McpCommands::Validate => {
            info!("🔍 Validating MCP configuration...");
            let config = Config::from_env()?;
            match config.validate_mcp_environment() {
                Ok(_) => {
                    info!("✅ MCP configuration is valid");
                    info!("   - Database: {}", config.safe_database_url());
                    info!(
                        "   - Embedding: {} ({})",
                        config.embedding.provider, config.embedding.model
                    );
                    info!("   - HTTP Port: {}", config.http_port);
                    if let Some(mcp_port) = config.mcp_port {
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
        McpCommands::Diagnose => {
            info!("🔍 Generating MCP diagnostic report...");
            let config = Config::from_env().unwrap_or_else(|_| {
                info!("⚠️  Unable to load full configuration, using defaults");
                Config::default()
            });

            let report = config.create_diagnostic_report();
            println!("{}", report);
            Ok(())
        }
        McpCommands::Test => {
            info!("🧪 Testing MCP server connectivity...");
            let config = Config::from_env()?;
            config.validate_mcp_environment()?;

            let setup_manager = SetupManager::new(config.clone());
            let db_setup = DatabaseSetup::new(config.database_url.clone());

            // Test database
            info!("Testing database connectivity...");
            match db_setup.health_check().await {
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
            match setup_manager.quick_health_check().await {
                Ok(_) => info!("✅ Embedding service: Available"),
                Err(e) => error!("❌ Embedding service: {}", e),
            }

            info!("🎉 MCP connectivity test completed");
            Ok(())
        }
        McpCommands::Template {
            template_type,
            output,
        } => {
            info!(
                "📋 Generating MCP configuration template: {}",
                template_type
            );

            let template_content = match template_type.as_str() {
                "basic" => generate_basic_mcp_template(),
                "production" => generate_production_mcp_template(),
                "development" => generate_development_mcp_template(),
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
    }
}

async fn handle_manager_command(command: ManagerCommands) -> Result<()> {
    use codex_memory::manager::ServerManager;

    let manager = match &command {
        ManagerCommands::Start {
            pid_file, log_file, ..
        } => ServerManager::with_paths(pid_file.clone(), log_file.clone()),
        ManagerCommands::Stop { pid_file } | ManagerCommands::Restart { pid_file } => {
            ServerManager::with_paths(pid_file.clone(), None)
        }
        _ => ServerManager::new(),
    };

    match command {
        ManagerCommands::Start { daemon, .. } => manager.start_daemon(daemon).await,
        ManagerCommands::Stop { .. } => manager.stop().await,
        ManagerCommands::Restart { .. } => manager.restart().await,
        ManagerCommands::Status { detailed } => manager.status(detailed).await,
        ManagerCommands::Logs { lines, follow } => manager.show_logs(lines, follow).await,
        ManagerCommands::Install { service_type } => manager.install_service(service_type).await,
        ManagerCommands::Uninstall => manager.uninstall_service().await,
    }
}

fn generate_basic_mcp_template() -> String {
    r#"{
  "mcpServers": {
    "agentic-memory": {
      "command": "/path/to/codex-memory",
      "args": ["start"],
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

fn generate_production_mcp_template() -> String {
    r#"{
  "mcpServers": {
    "agentic-memory": {
      "command": "/usr/local/bin/codex-memory",
      "args": ["start"],
      "env": {
        "DATABASE_URL": "${DATABASE_URL}",
        "EMBEDDING_PROVIDER": "${EMBEDDING_PROVIDER:-ollama}",
        "EMBEDDING_BASE_URL": "${EMBEDDING_BASE_URL:-http://localhost:11434}",
        "EMBEDDING_MODEL": "${EMBEDDING_MODEL:-nomic-embed-text}",
        "OPENAI_API_KEY": "${OPENAI_API_KEY}",
        "HTTP_PORT": "8080",
        "MCP_PORT": "8081",
        "LOG_LEVEL": "warn",
        "MAX_DB_CONNECTIONS": "20",
        "ENABLE_METRICS": "true"
      }
    }
  }
}"#
    .to_string()
}

fn generate_development_mcp_template() -> String {
    r#"{
  "mcpServers": {
    "agentic-memory-dev": {
      "command": "./target/debug/codex-memory",
      "args": ["start"],
      "env": {
        "DATABASE_URL": "postgresql://dev_user:dev_password@localhost:5432/memory_dev_db",
        "EMBEDDING_PROVIDER": "mock",
        "EMBEDDING_MODEL": "mock-model",
        "HTTP_PORT": "8080",
        "MCP_PORT": "8081",
        "LOG_LEVEL": "debug",
        "ENABLE_METRICS": "false"
      }
    }
  }
}"#
    .to_string()
}

async fn start_server(skip_setup: bool) -> Result<()> {
    // Load configuration first
    let config = Config::from_env()?;
    config.validate()?;

    info!("🚀 Starting Agentic Memory System server...");
    info!(
        "📊 Config: HTTP port {}, DB connections: {}",
        config.http_port, config.operational.max_db_connections
    );

    // Run setup check unless skipped
    if !skip_setup {
        info!("🔍 Running pre-flight checks...");
        let setup_manager = SetupManager::new(config.clone());
        let db_setup = DatabaseSetup::new(config.database_url.clone());

        // Quick health checks before starting
        match setup_manager.quick_health_check().await {
            Ok(_) => info!("✅ System health check passed"),
            Err(e) => {
                error!("❌ System health check failed: {}", e);
                info!("💡 Try running: codex-memory setup");
                return Err(e);
            }
        }

        match db_setup.health_check().await {
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
    }

    // Create database connection pool
    let pool = create_pool(&config.database_url, config.operational.max_db_connections).await?;

    // Migration support removed for crates.io version
    if std::env::var("AUTO_MIGRATE").unwrap_or_else(|_| "false".to_string()) == "true" {
        warn!("AUTO_MIGRATE=true but migration support not available in this build");
        info!("💡 Please set AUTO_MIGRATE=false or run migrations manually");
    }

    // Create repository and embedding service
    let repository = Arc::new(MemoryRepository::new(pool.clone()));
    let embedder = Arc::new(match config.embedding.provider.as_str() {
        "openai" => SimpleEmbedder::new(config.embedding.api_key.clone())
            .with_model(config.embedding.model.clone())
            .with_base_url(config.embedding.base_url.clone()),
        "ollama" => SimpleEmbedder::new_ollama(
            config.embedding.base_url.clone(),
            config.embedding.model.clone(),
        ),
        "mock" => SimpleEmbedder::new_mock(),
        _ => {
            return Err(anyhow::anyhow!(
                "Unsupported embedding provider: {}",
                config.embedding.provider
            ));
        }
    });

    // Create app state
    let state = AppState {
        config: config.clone(),
        repository,
        embedder,
    };

    // Build router
    let mut app = Router::new()
        .route("/health", get(health_check))
        .route("/api/v1/memories", post(create_memory))
        .route("/api/v1/memories/:id", get(get_memory))
        .route("/api/v1/memories/search", post(search_memories))
        .route("/api/v1/stats", get(get_statistics))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state.clone());

    // Add metrics endpoint if enabled
    if config.operational.enable_metrics {
        app = app.route("/metrics", get(metrics_handler));
    }

    // TCP MCP server removed - use 'codex-memory mcp-stdio' for MCP functionality

    // Start HTTP server
    let addr = SocketAddr::from(([0, 0, 0, 0], config.http_port));
    info!("Starting HTTP server on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

// TCP MCP server removed - only stdio MCP server is supported

async fn start_mcp_stdio(skip_setup: bool) -> Result<()> {
    // Load configuration
    let config = Config::from_env().map_err(|_| anyhow::anyhow!("Configuration error"))?;

    if !skip_setup {
        config
            .validate()
            .map_err(|_| anyhow::anyhow!("Configuration invalid"))?;
    }

    // Create database connection pool
    let pool = create_pool(&config.database_url, config.operational.max_db_connections)
        .await
        .map_err(|_| anyhow::anyhow!("Database connection failed"))?;

    // Create repository and embedder
    let repository = Arc::new(MemoryRepository::new(pool.clone()));
    let embedder = Arc::new(match config.embedding.provider.as_str() {
        "openai" => SimpleEmbedder::new(config.embedding.api_key.clone())
            .with_model(config.embedding.model.clone())
            .with_base_url(config.embedding.base_url.clone()),
        "ollama" => SimpleEmbedder::new_ollama(
            config.embedding.base_url.clone(),
            config.embedding.model.clone(),
        ),
        _ => SimpleEmbedder::new_mock(),
    });

    // Create MCP server configuration
    let mcp_config = MCPServerConfig::default();

    // Create and start MCP server
    let mut mcp_server = MCPServer::new(repository, embedder, mcp_config)?;
    mcp_server.start().await?;

    Ok(())
}

async fn shutdown_signal() {
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

async fn health_check(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // Simple health check - try to get statistics
    match state.repository.get_statistics().await {
        Ok(stats) => Ok(Json(json!({
            "status": "healthy",
            "database": "connected",
            "memory_count": stats.total_active.unwrap_or(0),
            "config": {
                "working_limit": state.config.tier_config.working_tier_limit,
                "warm_limit": state.config.tier_config.warm_tier_limit,
                "embedding_provider": state.config.embedding.provider,
                "embedding_model": state.config.embedding.model
            }
        }))),
        Err(_) => Err(StatusCode::SERVICE_UNAVAILABLE),
    }
}

async fn metrics_handler() -> Result<String, StatusCode> {
    if !prometheus::gather().is_empty() {
        let encoder = TextEncoder::new();
        let metric_families = prometheus::gather();
        let mut buffer = vec![];

        encoder
            .encode(&metric_families, &mut buffer)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        String::from_utf8(buffer).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
    } else {
        Ok("# No metrics available\n".to_string())
    }
}

async fn create_memory(
    State(state): State<AppState>,
    Json(mut request): Json<codex_memory::memory::models::CreateMemoryRequest>,
) -> Result<Json<codex_memory::Memory>, StatusCode> {
    // Generate embedding if not provided
    if request.embedding.is_none() {
        match state.embedder.generate_embedding(&request.content).await {
            Ok(embedding) => {
                request.embedding = Some(embedding);
            }
            Err(e) => {
                error!("Failed to generate embedding: {}", e);
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
    }

    match state.repository.create_memory(request).await {
        Ok(memory) => Ok(Json(memory)),
        Err(e) => {
            error!("Failed to create memory: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn get_memory(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<uuid::Uuid>,
) -> Result<Json<codex_memory::Memory>, StatusCode> {
    match state.repository.get_memory(id).await {
        Ok(memory) => Ok(Json(memory)),
        Err(codex_memory::MemoryError::NotFound { .. }) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            error!("Failed to get memory: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn search_memories(
    State(state): State<AppState>,
    Json(mut request): Json<codex_memory::memory::models::SearchRequest>,
) -> Result<Json<codex_memory::memory::models::SearchResponse>, StatusCode> {
    // Generate query embedding if semantic search is requested
    if let Some(ref query_text) = request.query_text {
        if request.query_embedding.is_none() {
            match state.embedder.generate_embedding(query_text).await {
                Ok(embedding) => {
                    request.query_embedding = Some(embedding);
                }
                Err(e) => {
                    error!("Failed to generate query embedding: {}", e);
                    return Err(StatusCode::INTERNAL_SERVER_ERROR);
                }
            }
        }
    }

    match state.repository.search_memories(request).await {
        Ok(response) => Ok(Json(response)),
        Err(e) => {
            error!("Failed to search memories: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn get_statistics(
    State(state): State<AppState>,
) -> Result<Json<MemoryStatistics>, StatusCode> {
    match state.repository.get_statistics().await {
        Ok(stats) => Ok(Json(stats)),
        Err(e) => {
            error!("Failed to get statistics: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
