use anyhow::Result;
use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use memory_core::{ConnectionConfig, ConnectionPool, MemoryRepository};
use migration::MigrationRunner;
use prometheus::{Encoder, TextEncoder};
use serde_json::json;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::signal;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Clone)]
struct AppState {
    pool: Arc<ConnectionPool>,
    repository: Arc<MemoryRepository>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,codex_memory=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Load configuration
    dotenv::dotenv().ok();
    let config = load_config()?;

    // Create connection pool
    let pool = ConnectionPool::new(config.clone()).await?;
    let pg_pool = pool.pool().clone();

    // Run migrations if enabled
    if std::env::var("AUTO_MIGRATE").unwrap_or_else(|_| "false".to_string()) == "true" {
        info!("Running database migrations...");
        let migration_dir = std::env::var("MIGRATION_DIR")
            .unwrap_or_else(|_| "./migration/migrations".to_string());
        
        let runner = MigrationRunner::new(pg_pool.clone(), migration_dir);
        runner.migrate().await?;
        runner.verify_checksums().await?;
        info!("Migrations completed successfully");
    }

    // Create repository
    let repository = Arc::new(MemoryRepository::new(pg_pool.clone()));

    // Create app state
    let state = AppState {
        pool: Arc::new(pool),
        repository,
    };

    // Build router
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/metrics", get(metrics_handler))
        .route("/api/v1/memories", post(create_memory))
        .route("/api/v1/memories/:id", get(get_memory))
        .route("/api/v1/memories/search", post(search_memories))
        .route("/api/v1/stats", get(get_statistics))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    // Start server
    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    info!("Starting server on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    info!("Shutdown signal received");
}

async fn health_check(State(state): State<AppState>) -> Result<Json<serde_json::Value>, StatusCode> {
    match state.pool.check_health().await {
        Ok(true) => {
            let stats = state.pool.get_pool_stats().await;
            Ok(Json(json!({
                "status": "healthy",
                "database": "connected",
                "pool": {
                    "size": stats.size,
                    "idle": stats.idle,
                    "max_size": stats.max_size,
                    "utilization_percentage": stats.utilization_percentage()
                }
            })))
        }
        _ => Err(StatusCode::SERVICE_UNAVAILABLE),
    }
}

async fn metrics_handler() -> Result<String, StatusCode> {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = vec![];
    
    encoder.encode(&metric_families, &mut buffer)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    String::from_utf8(buffer)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn create_memory(
    State(state): State<AppState>,
    Json(request): Json<memory_core::CreateMemoryRequest>,
) -> Result<Json<memory_core::Memory>, StatusCode> {
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
) -> Result<Json<memory_core::Memory>, StatusCode> {
    match state.repository.get_memory(id).await {
        Ok(memory) => Ok(Json(memory)),
        Err(memory_core::MemoryError::NotFound { .. }) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            error!("Failed to get memory: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn search_memories(
    State(state): State<AppState>,
    Json(request): Json<memory_core::SearchRequest>,
) -> Result<Json<Vec<memory_core::SearchResult>>, StatusCode> {
    match state.repository.search_memories(request).await {
        Ok(results) => Ok(Json(results)),
        Err(e) => {
            error!("Failed to search memories: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn get_statistics(
    State(state): State<AppState>,
) -> Result<Json<memory_core::MemoryStatistics>, StatusCode> {
    match state.repository.get_statistics().await {
        Ok(stats) => Ok(Json(stats)),
        Err(e) => {
            error!("Failed to get statistics: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

fn load_config() -> Result<ConnectionConfig> {
    Ok(ConnectionConfig {
        host: std::env::var("DATABASE_HOST").unwrap_or_else(|_| "localhost".to_string()),
        port: std::env::var("DATABASE_PORT")
            .unwrap_or_else(|_| "5432".to_string())
            .parse()?,
        database: std::env::var("DATABASE_NAME").unwrap_or_else(|_| "codex_memory".to_string()),
        username: std::env::var("DATABASE_USER").unwrap_or_else(|_| "postgres".to_string()),
        password: std::env::var("DATABASE_PASSWORD").unwrap_or_else(|_| "postgres".to_string()),
        max_connections: std::env::var("MAX_CONNECTIONS")
            .unwrap_or_else(|_| "100".to_string())
            .parse()?,
        min_connections: std::env::var("MIN_CONNECTIONS")
            .unwrap_or_else(|_| "10".to_string())
            .parse()?,
        connection_timeout_seconds: std::env::var("CONNECTION_TIMEOUT_SECONDS")
            .unwrap_or_else(|_| "30".to_string())
            .parse()?,
        idle_timeout_seconds: std::env::var("IDLE_TIMEOUT_SECONDS")
            .unwrap_or_else(|_| "600".to_string())
            .parse()?,
        max_lifetime_seconds: std::env::var("MAX_LIFETIME_SECONDS")
            .unwrap_or_else(|_| "1800".to_string())
            .parse()?,
    })
}