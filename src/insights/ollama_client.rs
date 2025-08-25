#[cfg(feature = "codex-dreams")]
use async_trait::async_trait;
#[cfg(feature = "codex-dreams")]
use reqwest::Client;
#[cfg(feature = "codex-dreams")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "codex-dreams")]
use std::time::Duration;
#[cfg(feature = "codex-dreams")]
use thiserror::Error;
#[cfg(feature = "codex-dreams")]
use tracing::{debug, error, info, warn};
#[cfg(feature = "codex-dreams")]
use url::Url;
#[cfg(feature = "codex-dreams")]
use uuid::Uuid;

#[cfg(feature = "codex-dreams")]
use super::models::InsightType;
#[cfg(feature = "codex-dreams")]
use crate::memory::Memory;

#[cfg(feature = "codex-dreams")]
#[derive(Debug, Error)]
pub enum OllamaClientError {
    #[error("Invalid URL: {0}")]
    InvalidUrl(String),
    #[error("Security violation: Only localhost URLs are allowed, got: {0}")]
    SecurityViolation(String),
    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),
    #[error("Response parsing failed: {0}")]
    ParseError(String),
    #[error("Timeout exceeded")]
    Timeout,
    #[error("Ollama service unavailable: {0}")]
    ServiceUnavailable(String),
    #[error("Malformed response: {0}")]
    MalformedResponse(String),
    #[error("Configuration error: {0}")]
    ConfigError(String),
}

#[cfg(feature = "codex-dreams")]
#[derive(Debug, Clone)]
pub struct OllamaConfig {
    /// Base URL for Ollama service (must be localhost)
    pub base_url: String,
    /// Model name to use for insight generation
    pub model: String,
    /// Timeout for requests in seconds (recommended: 600s+ for large models like 20B)
    pub timeout_seconds: u64,
    /// Maximum retries for transient failures
    pub max_retries: u32,
    /// Initial retry delay in milliseconds
    pub initial_retry_delay_ms: u64,
    /// Maximum retry delay in milliseconds
    pub max_retry_delay_ms: u64,
    /// Enable streaming responses for faster initial response (recommended for large models)
    pub enable_streaming: bool,
}

#[cfg(feature = "codex-dreams")]
impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:11434".to_string(),
            model: "gpt-oss:20b".to_string(), // Changed from llama2 to available model
            timeout_seconds: 600,             // 10 minutes for 20B parameter model
            max_retries: 3,
            initial_retry_delay_ms: 100,
            max_retry_delay_ms: 5000,
            enable_streaming: true, // Enable streaming for large models
        }
    }
}

#[cfg(feature = "codex-dreams")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsightResponse {
    pub id: Uuid,
    pub insight_type: InsightType,
    pub content: String,
    pub confidence_score: f64,
    pub source_memory_ids: Vec<Uuid>,
    pub metadata: serde_json::Value,
    pub tags: Vec<String>,
}

#[cfg(feature = "codex-dreams")]
#[derive(Debug, Serialize, Deserialize)]
struct OllamaRequest {
    model: String,
    prompt: String,
    stream: bool,
    options: OllamaOptions,
}

#[cfg(feature = "codex-dreams")]
#[derive(Debug, Serialize, Deserialize)]
struct OllamaOptions {
    temperature: f32,
    top_p: f32,
    max_tokens: u32,
}

#[cfg(feature = "codex-dreams")]
#[derive(Debug, Deserialize)]
struct OllamaResponse {
    response: String,
    done: bool,
}

#[cfg(feature = "codex-dreams")]
#[async_trait]
pub trait OllamaClientTrait {
    async fn generate_insight(
        &self,
        memories: Vec<Memory>,
    ) -> Result<InsightResponse, OllamaClientError>;
    async fn health_check(&self) -> Result<bool, OllamaClientError>;
}

#[cfg(feature = "codex-dreams")]
#[derive(Clone, Debug)]
pub struct OllamaClient {
    config: OllamaConfig,
    client: Client,
}

#[cfg(feature = "codex-dreams")]
impl OllamaClient {
    /// Create a new Ollama client with the given configuration
    pub fn new(config: OllamaConfig) -> Result<Self, OllamaClientError> {
        // Validate URL format and security
        Self::validate_url(&config.base_url)?;

        // Create HTTP client with timeout
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_seconds))
            .build()
            .map_err(OllamaClientError::HttpError)?;

        Ok(Self { config, client })
    }

    /// Validate URL format and security
    /// Allows localhost and configured URLs from environment
    fn validate_url(url_str: &str) -> Result<(), OllamaClientError> {
        let url = Url::parse(url_str)
            .map_err(|e| OllamaClientError::InvalidUrl(format!("Failed to parse URL: {}", e)))?;

        let host = url
            .host_str()
            .ok_or_else(|| OllamaClientError::InvalidUrl("URL must contain a host".to_string()))?;

        // Check if this URL is from environment configuration
        let is_from_env = std::env::var("OLLAMA_BASE_URL")
            .map(|env_url| env_url == url_str)
            .unwrap_or(false);

        // Allow localhost, local IPs, and URLs from environment configuration
        if !is_from_env && !matches!(host, "localhost" | "127.0.0.1" | "::1") {
            // Also allow private network IPs (192.168.x.x, 10.x.x.x, 172.16-31.x.x)
            if let Ok(ip) = host.parse::<std::net::IpAddr>() {
                if !ip.is_loopback()
                    && !matches!(ip, std::net::IpAddr::V4(ipv4) if ipv4.is_private())
                {
                    return Err(OllamaClientError::SecurityViolation(url_str.to_string()));
                }
            } else if !host.starts_with("192.168.")
                && !host.starts_with("10.")
                && !host.starts_with("172.")
            {
                // If it's not an IP, only allow localhost variants
                return Err(OllamaClientError::SecurityViolation(url_str.to_string()));
            }
        }

        Ok(())
    }

    /// Create the system prompt for insight generation
    fn create_system_prompt(&self, memories: &[Memory]) -> String {
        let memory_contents: Vec<String> = memories
            .iter()
            .enumerate()
            .map(|(i, memory)| {
                format!(
                    "Memory {}: {} (importance: {:.2}, tier: {:?})",
                    i + 1,
                    memory.content,
                    memory.importance_score,
                    memory.tier
                )
            })
            .collect();

        format!(
            r#"You are an AI assistant specialized in analyzing stored memories to generate insights.

Given the following memories, analyze them to identify patterns, connections, or learnings.

Memories to analyze:
{}

Generate ONE insight from these memories. Respond ONLY with a valid JSON object in this exact format:
{{
  "insight_type": "learning|connection|relationship|assertion|mentalmodel|pattern",
  "content": "The actual insight text",
  "confidence_score": 0.85,
  "tags": ["tag1", "tag2"],
  "reasoning": "Brief explanation of why this insight was generated"
}}

Requirements:
- confidence_score must be between 0.0 and 1.0
- content should be a clear, actionable insight
- tags should be relevant keywords (2-5 tags)
- Choose the most appropriate insight_type
- Keep content under 500 characters
- Do not include any text outside the JSON object"#,
            memory_contents.join("\n")
        )
    }

    /// Parse the Ollama response into an InsightResponse
    fn parse_insight_response(
        &self,
        response_text: &str,
        memory_ids: Vec<Uuid>,
    ) -> Result<InsightResponse, OllamaClientError> {
        // Try to extract JSON from the response (in case there's extra text)
        let json_start = response_text.find('{').ok_or_else(|| {
            OllamaClientError::MalformedResponse("No JSON object found in response".to_string())
        })?;

        let json_end = response_text.rfind('}').ok_or_else(|| {
            OllamaClientError::MalformedResponse("No closing brace found in JSON".to_string())
        })?;

        let json_str = &response_text[json_start..=json_end];

        #[derive(Deserialize)]
        struct ParsedInsight {
            insight_type: String,
            content: String,
            confidence_score: f64,
            tags: Vec<String>,
            reasoning: Option<String>,
        }

        let parsed: ParsedInsight = serde_json::from_str(json_str)
            .map_err(|e| OllamaClientError::ParseError(format!("JSON parsing failed: {}", e)))?;

        // Validate confidence score
        if !(0.0..=1.0).contains(&parsed.confidence_score) {
            return Err(OllamaClientError::ParseError(
                "Confidence score must be between 0.0 and 1.0".to_string(),
            ));
        }

        // Parse insight type
        let insight_type = match parsed.insight_type.to_lowercase().as_str() {
            "learning" => InsightType::Learning,
            "connection" => InsightType::Connection,
            "relationship" => InsightType::Relationship,
            "assertion" => InsightType::Assertion,
            "mentalmodel" => InsightType::MentalModel,
            "pattern" => InsightType::Pattern,
            _ => {
                return Err(OllamaClientError::ParseError(format!(
                    "Invalid insight_type: {}",
                    parsed.insight_type
                )))
            }
        };

        // Create metadata with reasoning if provided
        let mut metadata = serde_json::json!({});
        if let Some(reasoning) = parsed.reasoning {
            metadata["reasoning"] = serde_json::Value::String(reasoning);
        }

        Ok(InsightResponse {
            id: Uuid::new_v4(),
            insight_type,
            content: parsed.content,
            confidence_score: parsed.confidence_score,
            source_memory_ids: memory_ids,
            metadata,
            tags: parsed.tags,
        })
    }

    /// Execute request with simple exponential backoff retry
    async fn execute_with_retry<F, Fut, T>(&self, operation: F) -> Result<T, OllamaClientError>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T, OllamaClientError>>,
    {
        let mut retry_count = 0;
        let mut delay_ms = self.config.initial_retry_delay_ms;

        loop {
            match operation().await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    retry_count += 1;

                    if retry_count >= self.config.max_retries {
                        return Err(e);
                    }

                    warn!(
                        "Ollama request failed (attempt {}/{}), retrying in {}ms",
                        retry_count, self.config.max_retries, delay_ms
                    );

                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;

                    // Exponential backoff with cap
                    delay_ms = (delay_ms * 2).min(self.config.max_retry_delay_ms);
                }
            }
        }
    }

    #[cfg(feature = "codex-dreams")]
    pub async fn health_check(&self) -> bool {
        match self
            .execute_with_retry(|| async {
                let response = self
                    .client
                    .post(&format!("{}/api/version", self.config.base_url))
                    .timeout(Duration::from_millis(5000))
                    .send()
                    .await
                    .map_err(|e| OllamaClientError::HttpError(e))?;

                if response.status().is_success() {
                    Ok(())
                } else {
                    Err(OllamaClientError::ServiceUnavailable(format!(
                        "Health check failed with status: {}",
                        response.status()
                    )))
                }
            })
            .await
        {
            Ok(_) => true,
            Err(_) => false,
        }
    }

    #[cfg(feature = "codex-dreams")]
    pub async fn generate_insights_batch(
        &self,
        memories: Vec<crate::memory::Memory>,
    ) -> Result<InsightResponse, OllamaClientError> {
        // Use the trait method implementation
        OllamaClientTrait::generate_insight(self, memories).await
    }
}

#[cfg(feature = "codex-dreams")]
#[async_trait]
impl OllamaClientTrait for OllamaClient {
    async fn generate_insight(
        &self,
        memories: Vec<Memory>,
    ) -> Result<InsightResponse, OllamaClientError> {
        if memories.is_empty() {
            return Err(OllamaClientError::ConfigError(
                "Cannot generate insight from empty memory list".to_string(),
            ));
        }

        info!(
            "Generating insight from {} memories using model {}",
            memories.len(),
            self.config.model
        );

        let memory_ids: Vec<Uuid> = memories.iter().map(|m| m.id).collect();
        let prompt = self.create_system_prompt(&memories);

        let request = OllamaRequest {
            model: self.config.model.clone(),
            prompt,
            stream: self.config.enable_streaming, // Use config setting for streaming
            options: OllamaOptions {
                temperature: 0.7,
                top_p: 0.9,
                max_tokens: 1000,
            },
        };

        let url = format!("{}/api/generate", self.config.base_url);

        let response = self
            .execute_with_retry(|| async {
                debug!("Sending request to Ollama: {}", url);

                let response = self.client.post(&url).json(&request).send().await?;

                if !response.status().is_success() {
                    return Err(OllamaClientError::ServiceUnavailable(format!(
                        "HTTP {}: {}",
                        response.status(),
                        response.status()
                    )));
                }

                let ollama_response: OllamaResponse = response.json().await?;

                if !ollama_response.done {
                    return Err(OllamaClientError::MalformedResponse(
                        "Received incomplete response from Ollama".to_string(),
                    ));
                }

                Ok(ollama_response.response)
            })
            .await?;

        debug!("Received response from Ollama: {}", response);

        let insight = self.parse_insight_response(&response, memory_ids)?;

        info!(
            "Generated insight of type {:?} with confidence {:.2}",
            insight.insight_type, insight.confidence_score
        );

        Ok(insight)
    }

    async fn health_check(&self) -> Result<bool, OllamaClientError> {
        let url = format!("{}/api/version", self.config.base_url);

        debug!("Performing health check: {}", url);

        let response = self.client.get(&url).send().await?;

        let is_healthy = response.status().is_success();

        if is_healthy {
            debug!("Ollama health check passed");
        } else {
            warn!("Ollama health check failed: HTTP {}", response.status());
        }

        Ok(is_healthy)
    }
}

#[cfg(feature = "codex-dreams")]
#[derive(Clone)]
pub struct MockOllamaClient {
    should_fail: bool,
    fail_count: std::sync::Arc<std::sync::Mutex<usize>>,
}

#[cfg(feature = "codex-dreams")]
impl MockOllamaClient {
    pub fn new() -> Self {
        Self {
            should_fail: false,
            fail_count: std::sync::Arc::new(std::sync::Mutex::new(0)),
        }
    }

    pub fn with_failure() -> Self {
        Self {
            should_fail: true,
            fail_count: std::sync::Arc::new(std::sync::Mutex::new(0)),
        }
    }

    pub fn with_intermittent_failure(fail_times: usize) -> Self {
        let client = Self::new();
        *client.fail_count.lock().expect("Failed to lock fail_count") = fail_times;
        client
    }
}

#[cfg(feature = "codex-dreams")]
impl Default for MockOllamaClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "codex-dreams")]
#[async_trait]
impl OllamaClientTrait for MockOllamaClient {
    async fn generate_insight(
        &self,
        memories: Vec<Memory>,
    ) -> Result<InsightResponse, OllamaClientError> {
        if memories.is_empty() {
            return Err(OllamaClientError::ConfigError(
                "Cannot generate insight from empty memory list".to_string(),
            ));
        }

        // Check if we should fail
        {
            let mut count = self.fail_count.lock().expect("Failed to lock fail_count");
            if *count > 0 {
                *count -= 1;
                return Err(OllamaClientError::ServiceUnavailable(
                    "Mock client configured to fail".to_string(),
                ));
            }
        }

        if self.should_fail {
            return Err(OllamaClientError::ServiceUnavailable(
                "Mock client configured to always fail".to_string(),
            ));
        }

        let memory_ids: Vec<Uuid> = memories.iter().map(|m| m.id).collect();

        // Generate a mock insight based on memory content
        let insight_content = if memories.len() == 1 {
            format!(
                "Mock insight about: {}",
                memories[0].content.chars().take(50).collect::<String>()
            )
        } else {
            format!("Mock insight connecting {} memories", memories.len())
        };

        Ok(InsightResponse {
            id: Uuid::new_v4(),
            insight_type: InsightType::Pattern,
            content: insight_content,
            confidence_score: 0.85,
            source_memory_ids: memory_ids,
            metadata: serde_json::json!({
                "mock": true,
                "memory_count": memories.len()
            }),
            tags: vec!["mock".to_string(), "test".to_string()],
        })
    }

    async fn health_check(&self) -> Result<bool, OllamaClientError> {
        Ok(!self.should_fail)
    }
}

#[cfg(all(test, feature = "codex-dreams"))]
mod tests {
    use super::*;
    use crate::memory::{MemoryStatus, MemoryTier};
    use chrono::Utc;

    fn create_test_memory(content: &str) -> Memory {
        Memory {
            id: Uuid::new_v4(),
            content: content.to_string(),
            content_hash: "test_hash".to_string(),
            embedding: None,
            tier: MemoryTier::Working,
            status: MemoryStatus::Active,
            importance_score: 0.8,
            access_count: 1,
            last_accessed_at: Some(Utc::now()),
            metadata: serde_json::json!({}),
            parent_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            expires_at: None,
            consolidation_strength: 1.0,
            decay_rate: 1.0,
            recall_probability: Some(0.9),
            last_recall_interval: None,
            recency_score: 0.8,
            relevance_score: 0.8,
            successful_retrievals: 0,
            failed_retrievals: 0,
            total_retrieval_attempts: 0,
            last_retrieval_difficulty: None,
            last_retrieval_success: None,
            next_review_at: None,
            current_interval_days: Some(1.0),
            ease_factor: 2.5,
        }
    }

    #[test]
    fn test_config_default() {
        let config = OllamaConfig::default();
        assert_eq!(config.base_url, "http://localhost:11434");
        assert_eq!(config.model, "llama2");
        assert_eq!(config.timeout_seconds, 30);
        assert_eq!(config.max_retries, 3);
    }

    #[test]
    fn test_url_validation() {
        // Valid localhost URLs
        assert!(OllamaClient::validate_url("http://localhost:11434").is_ok());
        assert!(OllamaClient::validate_url("http://127.0.0.1:11434").is_ok());
        assert!(OllamaClient::validate_url("http://[::1]:11434").is_ok());

        // Valid private network URLs
        assert!(OllamaClient::validate_url("http://192.168.1.1:11434").is_ok());
        assert!(OllamaClient::validate_url("http://192.168.1.110:11434").is_ok());
        assert!(OllamaClient::validate_url("http://10.0.0.1:11434").is_ok());
        assert!(OllamaClient::validate_url("http://172.16.0.1:11434").is_ok());

        // Invalid public URLs (security violations)
        assert!(OllamaClient::validate_url("http://example.com:11434").is_err());
        assert!(OllamaClient::validate_url("http://8.8.8.8:11434").is_err());
        assert!(OllamaClient::validate_url("http://0.0.0.0:11434").is_err());

        // Invalid URL format
        assert!(OllamaClient::validate_url("not_a_url").is_err());
    }

    #[test]
    fn test_client_creation() {
        let config = OllamaConfig::default();
        let result = OllamaClient::new(config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_client_creation_with_invalid_url() {
        let config = OllamaConfig {
            base_url: "http://8.8.8.8:11434".to_string(), // Public IP should fail
            ..OllamaConfig::default()
        };
        let result = OllamaClient::new(config);
        assert!(result.is_err());
        match result.unwrap_err() {
            OllamaClientError::SecurityViolation(_) => {}
            _ => panic!("Expected SecurityViolation error"),
        }
    }

    #[test]
    fn test_client_creation_with_private_ip() {
        let config = OllamaConfig {
            base_url: "http://192.168.1.110:11434".to_string(), // Private IP should work
            ..OllamaConfig::default()
        };
        let result = OllamaClient::new(config);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_mock_client_success() {
        let client = MockOllamaClient::new();
        let memories = vec![create_test_memory("Test memory content")];

        let result = client.generate_insight(memories).await;
        assert!(result.is_ok());

        let insight = result.unwrap();
        assert!(!insight.content.is_empty());
        assert!(insight.confidence_score > 0.0);
        assert!(insight.confidence_score <= 1.0);
        assert_eq!(insight.source_memory_ids.len(), 1);
        assert!(insight.tags.contains(&"mock".to_string()));
    }

    #[tokio::test]
    async fn test_mock_client_failure() {
        let client = MockOllamaClient::with_failure();
        let memories = vec![create_test_memory("Test memory content")];

        let result = client.generate_insight(memories).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            OllamaClientError::ServiceUnavailable(_) => {}
            _ => panic!("Expected ServiceUnavailable error"),
        }
    }

    #[tokio::test]
    async fn test_mock_client_empty_memories() {
        let client = MockOllamaClient::new();
        let memories = vec![];

        let result = client.generate_insight(memories).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            OllamaClientError::ConfigError(_) => {}
            _ => panic!("Expected ConfigError"),
        }
    }

    #[tokio::test]
    async fn test_mock_client_health_check() {
        let client = MockOllamaClient::new();
        let result = client.health_check().await;
        assert!(result.is_ok());
        assert!(result.unwrap());

        let failing_client = MockOllamaClient::with_failure();
        let result = failing_client.health_check().await;
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[test]
    fn test_parse_insight_response() {
        let config = OllamaConfig::default();
        let client = OllamaClient::new(config).unwrap();
        let memory_ids = vec![Uuid::new_v4()];

        let response_text = r#"{"insight_type": "pattern", "content": "Test insight", "confidence_score": 0.85, "tags": ["test", "pattern"], "reasoning": "Test reasoning"}"#;

        let result = client.parse_insight_response(response_text, memory_ids.clone());
        assert!(result.is_ok());

        let insight = result.unwrap();
        assert_eq!(insight.content, "Test insight");
        assert_eq!(insight.confidence_score, 0.85);
        assert_eq!(insight.source_memory_ids, memory_ids);
        assert!(matches!(insight.insight_type, InsightType::Pattern));
        assert!(insight.tags.contains(&"test".to_string()));
    }

    #[test]
    fn test_parse_invalid_insight_response() {
        let config = OllamaConfig::default();
        let client = OllamaClient::new(config).unwrap();
        let memory_ids = vec![Uuid::new_v4()];

        // Invalid JSON
        let result = client.parse_insight_response("not json", memory_ids.clone());
        assert!(result.is_err());

        // Invalid confidence score
        let invalid_response = r#"{"insight_type": "pattern", "content": "Test", "confidence_score": 1.5, "tags": ["test"]}"#;
        let result = client.parse_insight_response(invalid_response, memory_ids.clone());
        assert!(result.is_err());

        // Invalid insight type
        let invalid_type = r#"{"insight_type": "invalid", "content": "Test", "confidence_score": 0.5, "tags": ["test"]}"#;
        let result = client.parse_insight_response(invalid_type, memory_ids);
        assert!(result.is_err());
    }
}
