use codex_memory::{
    api::{create_api_router, AppState},
    config::Config,
    memory::{connection::create_pool, MemoryRepository},
    Config as CodexConfig,
};
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::{info, warn};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    info!("🚀 Starting Memory Harvester Configuration Web UI");

    // Load configuration
    let config = match Config::from_file("config.toml") {
        Ok(config) => config,
        Err(e) => {
            warn!("Could not load config file, using defaults: {}", e);
            Config::default()
        }
    };

    // Create database connection pool
    let pool = create_pool(&config.database.url, config.database.max_connections).await?;
    let repository = Arc::new(MemoryRepository::new(pool));

    // Create application state
    let app_state = AppState {
        repository,
        harvester_service: None, // TODO: Initialize harvester service if needed
    };

    // Create API router
    let app = create_api_router(app_state);

    // Determine port
    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "3001".to_string())
        .parse::<u16>()
        .unwrap_or(3001);

    let addr = format!("0.0.0.0:{}", port);

    info!("🌐 Web UI server starting on http://{}", addr);
    info!("📊 Dashboard available at http://{}/", addr);
    info!(
        "🔧 Configuration API available at http://{}/api/config/harvester",
        addr
    );

    // Start server
    let listener = TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
