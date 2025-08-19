use crate::{EmbeddingError, EmbeddingProviderTrait, Result};
use async_trait::async_trait;
use governor::{Quota, RateLimiter};
use std::num::NonZeroU32;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

const CLAUDE_EMBEDDING_DIMENSION: usize = 1024;
const CLAUDE_MAX_TOKENS: usize = 8192;
const CLAUDE_COST_PER_1K_TOKENS: f64 = 0.0001; // Example cost

#[derive(Debug, Clone)]
pub struct ClaudeEmbeddingProvider {
    client: Client,
    api_key: String,
    api_url: String,
    rate_limiter: Arc<RateLimiter<governor::state::direct::NotKeyed, governor::state::InMemoryState, governor::clock::DefaultClock>>,
    retry_config: RetryConfig,
}

#[derive(Debug, Clone)]
struct RetryConfig {
    max_retries: u32,
    initial_delay: Duration,
    max_delay: Duration,
    exponential_base: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            exponential_base: 2.0,
        }
    }
}

#[derive(Debug, Serialize)]
struct ClaudeEmbeddingRequest {
    input: Vec<String>,
    model: String,
}

#[derive(Debug, Deserialize)]
struct ClaudeEmbeddingResponse {
    data: Vec<EmbeddingData>,
    usage: Usage,
}

#[derive(Debug, Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
    index: usize,
}

#[derive(Debug, Deserialize)]
struct Usage {
    prompt_tokens: u32,
    total_tokens: u32,
}

impl ClaudeEmbeddingProvider {
    pub fn new(api_key: String, api_url: String, rate_limit: u32) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| EmbeddingError::Unknown(e.to_string()))?;

        let rate_limit_nonzero = NonZeroU32::new(rate_limit).unwrap_or(NonZeroU32::new(100).unwrap());
        let rate_limiter = Arc::new(RateLimiter::direct(
            Quota::per_minute(rate_limit_nonzero)
        ));

        Ok(Self {
            client,
            api_key,
            api_url,
            rate_limiter,
            retry_config: RetryConfig::default(),
        })
    }

    async fn call_api(&self, texts: Vec<String>) -> Result<ClaudeEmbeddingResponse> {
        // Check rate limit
        self.rate_limiter.until_ready().await;

        let request = ClaudeEmbeddingRequest {
            input: texts.clone(),
            model: "claude-3-embedding".to_string(),
        };

        let mut retries = 0;
        let mut delay = self.retry_config.initial_delay;

        loop {
            let start = Instant::now();
            
            let response = self.client
                .post(&self.api_url)
                .header("Authorization", format!("Bearer {}", self.api_key))
                .header("Content-Type", "application/json")
                .header("Anthropic-Version", "2024-01-01")
                .json(&request)
                .send()
                .await;

            match response {
                Ok(resp) => {
                    let status = resp.status();
                    
                    if status.is_success() {
                        let result = resp.json::<ClaudeEmbeddingResponse>().await
                            .map_err(|e| EmbeddingError::Serialization(e.to_string()))?;
                        let latency = start.elapsed();
                        debug!("Claude API call successful, latency: {:?}", latency);
                        return Ok(result);
                    } else if status.as_u16() == 429 {
                        // Rate limited
                        warn!("Claude API rate limited, retrying...");
                        if retries >= self.retry_config.max_retries {
                            return Err(EmbeddingError::RateLimit(
                                "Max retries exceeded for rate limit".to_string()
                            ));
                        }
                    } else if status.is_server_error() {
                        // Server error, retry
                        error!("Claude API server error: {}", status);
                        if retries >= self.retry_config.max_retries {
                            return Err(EmbeddingError::ClaudeApi(
                                format!("Server error: {}", status)
                            ));
                        }
                    } else {
                        // Client error, don't retry
                        let error_text = resp.text().await.unwrap_or_default();
                        return Err(EmbeddingError::ClaudeApi(
                            format!("Client error {}: {}", status, error_text)
                        ));
                    }
                }
                Err(e) => {
                    error!("Claude API request failed: {}", e);
                    if retries >= self.retry_config.max_retries {
                        return Err(EmbeddingError::Network(e.to_string()));
                    }
                }
            }

            // Exponential backoff
            retries += 1;
            tokio::time::sleep(delay).await;
            delay = std::cmp::min(
                Duration::from_secs_f64(delay.as_secs_f64() * self.retry_config.exponential_base),
                self.retry_config.max_delay
            );
        }
    }

    pub fn estimate_cost(&self, text: &str) -> f64 {
        // Rough token estimation (4 chars per token)
        let estimated_tokens = text.len() / 4;
        (estimated_tokens as f64 / 1000.0) * CLAUDE_COST_PER_1K_TOKENS
    }

    fn validate_text(&self, text: &str) -> Result<()> {
        if text.is_empty() {
            return Err(EmbeddingError::InvalidInput("Text cannot be empty".to_string()));
        }
        
        // Check token limit (rough estimation)
        if text.len() / 4 > CLAUDE_MAX_TOKENS {
            return Err(EmbeddingError::InvalidInput(
                format!("Text exceeds maximum token limit of {}", CLAUDE_MAX_TOKENS)
            ));
        }
        
        Ok(())
    }
}

#[async_trait]
impl EmbeddingProviderTrait for ClaudeEmbeddingProvider {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        self.validate_text(text)?;
        
        let response = self.call_api(vec![text.to_string()]).await?;
        
        response.data
            .into_iter()
            .next()
            .map(|d| d.embedding)
            .ok_or_else(|| EmbeddingError::ClaudeApi("No embedding returned".to_string()))
    }

    async fn embed_batch(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        // Validate all texts
        for text in &texts {
            self.validate_text(text)?;
        }
        
        let response = self.call_api(texts).await?;
        
        // Sort by index to ensure correct order
        let mut data = response.data;
        data.sort_by_key(|d| d.index);
        
        Ok(data.into_iter().map(|d| d.embedding).collect())
    }

    fn dimension(&self) -> usize {
        CLAUDE_EMBEDDING_DIMENSION
    }

    fn name(&self) -> &'static str {
        "Claude"
    }

    fn supports_batch(&self) -> bool {
        true
    }

    fn max_batch_size(&self) -> usize {
        32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cost_estimation() {
        let provider = ClaudeEmbeddingProvider::new(
            "test_key".to_string(),
            "http://test.com".to_string(),
            100
        ).unwrap();
        
        let text = "a".repeat(4000); // ~1000 tokens
        let cost = provider.estimate_cost(&text);
        assert!((cost - CLAUDE_COST_PER_1K_TOKENS).abs() < 0.0001);
    }

    #[test]
    fn test_text_validation() {
        let provider = ClaudeEmbeddingProvider::new(
            "test_key".to_string(),
            "http://test.com".to_string(),
            100
        ).unwrap();
        
        // Empty text should fail
        assert!(provider.validate_text("").is_err());
        
        // Too long text should fail
        let long_text = "a".repeat(CLAUDE_MAX_TOKENS * 5);
        assert!(provider.validate_text(&long_text).is_err());
        
        // Normal text should pass
        assert!(provider.validate_text("Hello world").is_ok());
    }
}