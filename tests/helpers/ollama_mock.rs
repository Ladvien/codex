//! Mock Ollama server for testing the Codex Dreams feature
//! 
//! Provides a controllable mock server that simulates Ollama's API
//! for predictable testing without requiring actual LLM infrastructure.

use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Clone)]
pub struct MockOllamaConfig {
    pub port: u16,
    pub model: String,
    pub fail_after: Option<usize>, // Fail after N requests for testing resilience
    pub response_delay_ms: Option<u64>, // Simulate slow responses
    pub always_fail: bool, // For testing error handling
}

impl Default for MockOllamaConfig {
    fn default() -> Self {
        Self {
            port: 11435, // Different from default Ollama port to avoid conflicts
            model: "llama2:latest".to_string(),
            fail_after: None,
            response_delay_ms: None,
            always_fail: false,
        }
    }
}

pub struct MockOllamaServer {
    config: MockOllamaConfig,
    request_count: Arc<RwLock<usize>>,
}

impl MockOllamaServer {
    pub fn new(config: MockOllamaConfig) -> Self {
        Self {
            config,
            request_count: Arc::new(RwLock::new(0)),
        }
    }

    pub async fn start(self) -> Result<String, Box<dyn std::error::Error>> {
        let addr = format!("127.0.0.1:{}", self.config.port);
        let state = AppState {
            config: self.config.clone(),
            request_count: self.request_count,
        };

        let app = Router::new()
            .route("/api/generate", post(generate_handler))
            .route("/api/embeddings", post(embeddings_handler))
            .route("/api/tags", get(tags_handler))
            .route("/", get(health_handler))
            .with_state(state);

        let listener = tokio::net::TcpListener::bind(&addr).await?;
        
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        Ok(format!("http://{}", addr))
    }
}

#[derive(Clone)]
struct AppState {
    config: MockOllamaConfig,
    request_count: Arc<RwLock<usize>>,
}

async fn health_handler() -> StatusCode {
    StatusCode::OK
}

async fn tags_handler(State(state): State<AppState>) -> Json<Value> {
    Json(json!({
        "models": [
            {
                "name": state.config.model,
                "modified_at": "2024-01-01T00:00:00Z",
                "size": 4000000000,
            }
        ]
    }))
}

#[derive(Deserialize)]
struct GenerateRequest {
    model: String,
    prompt: String,
    #[allow(dead_code)]
    stream: Option<bool>,
}

#[derive(Serialize)]
struct GenerateResponse {
    model: String,
    created_at: String,
    response: String,
    done: bool,
}

async fn generate_handler(
    State(state): State<AppState>,
    Json(request): Json<GenerateRequest>,
) -> Result<Json<GenerateResponse>, StatusCode> {
    // Update request count
    let mut count = state.request_count.write().await;
    *count += 1;
    let current_count = *count;
    drop(count);

    // Simulate delay if configured
    if let Some(delay) = state.config.response_delay_ms {
        tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
    }

    // Check if we should fail
    if state.config.always_fail {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    
    if let Some(fail_after) = state.config.fail_after {
        if current_count > fail_after {
            return Err(StatusCode::SERVICE_UNAVAILABLE);
        }
    }

    // Generate mock insight based on prompt
    let insight = generate_mock_insight(&request.prompt);
    
    Ok(Json(GenerateResponse {
        model: request.model,
        created_at: chrono::Utc::now().to_rfc3339(),
        response: insight,
        done: true,
    }))
}

#[derive(Deserialize)]
struct EmbeddingsRequest {
    model: String,
    prompt: String,
}

#[derive(Serialize)]
struct EmbeddingsResponse {
    embedding: Vec<f32>,
}

async fn embeddings_handler(
    State(_state): State<AppState>,
    Json(request): Json<EmbeddingsRequest>,
) -> Json<EmbeddingsResponse> {
    // Generate deterministic mock embedding based on prompt
    let mut embedding = vec![0.0f32; 1536];
    let bytes = request.prompt.as_bytes();
    
    for (i, byte) in bytes.iter().enumerate() {
        let idx = i % 1536;
        embedding[idx] = (*byte as f32 / 255.0) - 0.5;
    }
    
    // Normalize
    let magnitude = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
    if magnitude > 0.0 {
        for val in embedding.iter_mut() {
            *val /= magnitude;
        }
    }
    
    Json(EmbeddingsResponse { embedding })
}

fn generate_mock_insight(prompt: &str) -> String {
    // Parse memories from prompt and generate realistic insight
    if prompt.contains("test memory") || prompt.contains("Test memory") {
        format!(
            r#"{{
                "id": "{}",
                "content": "Pattern detected: Test memories indicate system validation phase. The recurring test patterns suggest systematic quality assurance procedures.",
                "insight_type": "pattern",
                "confidence_score": 0.85,
                "source_memory_ids": ["{}"],
                "tags": ["testing", "qa", "patterns"]
            }}"#,
            Uuid::new_v4(),
            Uuid::new_v4()
        )
    } else if prompt.contains("code") || prompt.contains("implementation") {
        format!(
            r#"{{
                "id": "{}",
                "content": "Learning: Code implementation patterns show iterative development with focus on test-driven design and modular architecture.",
                "insight_type": "learning",
                "confidence_score": 0.75,
                "source_memory_ids": ["{}"],
                "tags": ["development", "architecture", "tdd"]
            }}"#,
            Uuid::new_v4(),
            Uuid::new_v4()
        )
    } else {
        format!(
            r#"{{
                "id": "{}",
                "content": "Connection identified: The analyzed memories show relationships between different concepts that suggest an emerging mental model.",
                "insight_type": "connection",
                "confidence_score": 0.65,
                "source_memory_ids": ["{}"],
                "tags": ["relationships", "mental-model"]
            }}"#,
            Uuid::new_v4(),
            Uuid::new_v4()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_ollama_server() {
        let config = MockOllamaConfig::default();
        let server = MockOllamaServer::new(config);
        let url = server.start().await.unwrap();
        
        // Give server time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        
        // Test health endpoint
        let client = reqwest::Client::new();
        let response = client.get(&url).send().await.unwrap();
        assert_eq!(response.status(), 200);
    }
}