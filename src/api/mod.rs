pub mod config_api;
pub mod harvester_api;

use axum::{
    response::{Html, Json},
    routing::{get, post, put},
    Router,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

use crate::memory::{MemoryRepository, SilentHarvesterService};

/// Application state for the web API
#[derive(Clone)]
pub struct AppState {
    pub repository: Arc<MemoryRepository>,
    pub harvester_service: Option<Arc<SilentHarvesterService>>,
}

/// Create the main API router
pub fn create_api_router(state: AppState) -> Router {
    Router::new()
        // Health check endpoint
        .route("/api/health", get(health_check))
        // Configuration API routes
        .route(
            "/api/config/harvester",
            get(config_api::get_harvester_config),
        )
        .route(
            "/api/config/harvester",
            put(config_api::update_harvester_config),
        )
        // Harvester API routes
        .route("/api/harvester/status", get(harvester_api::get_status))
        .route(
            "/api/harvester/toggle",
            post(harvester_api::toggle_harvester),
        )
        .route("/api/harvester/stats", get(harvester_api::get_statistics))
        .route(
            "/api/harvester/recent",
            get(harvester_api::get_recent_memories),
        )
        .route("/api/harvester/export", get(harvester_api::export_history))
        // Serve static files (HTML, CSS, JS)
        .nest_service("/", ServeDir::new("static"))
        .route("/", get(serve_dashboard))
        .with_state(state)
        .layer(CorsLayer::permissive())
}

/// Health check endpoint
async fn health_check() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "service": "codex-memory-api",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

/// Serve the main dashboard HTML
async fn serve_dashboard() -> Html<&'static str> {
    Html(include_str!("../../static/index.html"))
}
