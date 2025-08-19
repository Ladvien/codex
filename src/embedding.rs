use anyhow::{Context, Result};
use backoff::{future::retry, ExponentialBackoff};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{info, warn};

#[derive(Debug, Clone)]
pub struct SimpleEmbedder {
    client: Client,
    api_key: String,
    model: String,
    base_url: String,
    provider: EmbeddingProvider,
    fallback_models: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EmbeddingProvider {
    OpenAI,
    Ollama,
    Mock, // For testing
}

// OpenAI API request/response structures
#[derive(Debug, Serialize)]
struct OpenAIEmbeddingRequest {
    input: String,
    model: String,
}

#[derive(Debug, Deserialize)]
struct OpenAIEmbeddingResponse {
    data: Vec<OpenAIEmbeddingData>,
}

#[derive(Debug, Deserialize)]
struct OpenAIEmbeddingData {
    embedding: Vec<f32>,
}

// Ollama API request/response structures
#[derive(Debug, Serialize)]
struct OllamaEmbeddingRequest {
    model: String,
    prompt: String,
}

#[derive(Debug, Deserialize)]
struct OllamaEmbeddingResponse {
    embedding: Vec<f32>,
}

#[derive(Debug, Deserialize)]
struct OllamaModel {
    name: String,
    #[allow(dead_code)]
    size: u64,
    #[serde(default)]
    #[allow(dead_code)]
    family: String,
}

#[derive(Debug, Deserialize)]
struct OllamaModelsResponse {
    models: Vec<OllamaModel>,
}

impl SimpleEmbedder {
    pub fn new(api_key: String) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            api_key,
            model: "text-embedding-3-small".to_string(),
            base_url: "https://api.openai.com".to_string(),
            provider: EmbeddingProvider::OpenAI,
            fallback_models: vec![
                "text-embedding-3-large".to_string(),
                "text-embedding-ada-002".to_string(),
            ],
        }
    }

    pub fn new_ollama(base_url: String, model: String) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(60)) // Ollama might be slower
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            api_key: String::new(), // Ollama doesn't need an API key
            model,
            base_url,
            provider: EmbeddingProvider::Ollama,
            fallback_models: vec![
                "nomic-embed-text".to_string(),
                "mxbai-embed-large".to_string(),
                "all-minilm".to_string(),
                "all-mpnet-base-v2".to_string(),
            ],
        }
    }

    pub fn new_mock() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(1))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            api_key: String::new(),
            model: "mock-model".to_string(),
            base_url: "http://mock:11434".to_string(),
            provider: EmbeddingProvider::Mock,
            fallback_models: vec!["mock-model-2".to_string()],
        }
    }

    pub fn with_model(mut self, model: String) -> Self {
        self.model = model;
        self
    }

    pub fn with_base_url(mut self, base_url: String) -> Self {
        self.base_url = base_url;
        self
    }

    /// Generate embedding for text with automatic retry
    pub async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>> {
        info!("Generating embedding for text of length: {}", text.len());

        let operation = || async {
            match self.generate_embedding_internal(text).await {
                Ok(embedding) => Ok(embedding),
                Err(e) => {
                    if e.to_string().contains("Rate limited") {
                        Err(backoff::Error::transient(e))
                    } else {
                        Err(backoff::Error::permanent(e))
                    }
                }
            }
        };

        let backoff = ExponentialBackoff {
            max_elapsed_time: Some(Duration::from_secs(60)),
            ..Default::default()
        };

        retry(backoff, operation).await
    }

    async fn generate_embedding_internal(&self, text: &str) -> Result<Vec<f32>> {
        match self.provider {
            EmbeddingProvider::OpenAI => self.generate_openai_embedding(text).await,
            EmbeddingProvider::Ollama => self.generate_ollama_embedding(text).await,
            EmbeddingProvider::Mock => self.generate_mock_embedding(text).await,
        }
    }

    async fn generate_openai_embedding(&self, text: &str) -> Result<Vec<f32>> {
        let request = OpenAIEmbeddingRequest {
            input: text.to_string(),
            model: self.model.clone(),
        };

        let response = self
            .client
            .post(&format!("{}/v1/embeddings", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());

            if status.as_u16() == 429 {
                warn!("Rate limited by OpenAI API, will retry");
                return Err(anyhow::anyhow!("Rate limited: {}", error_text));
            }

            return Err(anyhow::anyhow!(
                "OpenAI API request failed with status {}: {}",
                status,
                error_text
            ));
        }

        let embedding_response: OpenAIEmbeddingResponse = response.json().await?;

        if let Some(embedding_data) = embedding_response.data.first() {
            Ok(embedding_data.embedding.clone())
        } else {
            Err(anyhow::anyhow!("No embedding data in OpenAI response"))
        }
    }

    async fn generate_ollama_embedding(&self, text: &str) -> Result<Vec<f32>> {
        let request = OllamaEmbeddingRequest {
            model: self.model.clone(),
            prompt: text.to_string(),
        };

        let response = self
            .client
            .post(&format!("{}/api/embeddings", self.base_url))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());

            if status.as_u16() == 429 {
                warn!("Rate limited by Ollama API, will retry");
                return Err(anyhow::anyhow!("Rate limited: {}", error_text));
            }

            return Err(anyhow::anyhow!(
                "Ollama API request failed with status {}: {}",
                status,
                error_text
            ));
        }

        let embedding_response: OllamaEmbeddingResponse = response.json().await?;
        Ok(embedding_response.embedding)
    }

    async fn generate_mock_embedding(&self, text: &str) -> Result<Vec<f32>> {
        // Generate a deterministic mock embedding based on text content
        // This is useful for testing without requiring real embedding services
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        let hash = hasher.finish();

        // Generate a fixed-size embedding (768 dimensions for consistency)
        let dimensions = self.embedding_dimension();
        let mut embedding = Vec::with_capacity(dimensions);

        // Use the hash to seed a simple PRNG for consistent results
        let mut seed = hash;
        for _ in 0..dimensions {
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            let value = ((seed >> 16) % 1000) as f32 / 1000.0 - 0.5; // -0.5 to 0.5
            embedding.push(value);
        }

        // Normalize the embedding to unit length
        let magnitude: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if magnitude > 0.0 {
            for val in &mut embedding {
                *val /= magnitude;
            }
        }

        Ok(embedding)
    }

    /// Generate embeddings for multiple texts in batch
    pub async fn generate_embeddings_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        info!("Generating embeddings for {} texts", texts.len());

        let mut embeddings = Vec::with_capacity(texts.len());

        // Process in small batches to avoid rate limits
        for chunk in texts.chunks(10) {
            let mut chunk_embeddings = Vec::with_capacity(chunk.len());

            for text in chunk {
                match self.generate_embedding(text).await {
                    Ok(embedding) => chunk_embeddings.push(embedding),
                    Err(e) => {
                        warn!("Failed to generate embedding for text: {}", e);
                        return Err(e);
                    }
                }

                // Small delay to be respectful to the API
                tokio::time::sleep(Duration::from_millis(100)).await;
            }

            embeddings.extend(chunk_embeddings);
        }

        Ok(embeddings)
    }

    /// Get the dimension of embeddings for this model
    pub fn embedding_dimension(&self) -> usize {
        match self.provider {
            EmbeddingProvider::OpenAI => match self.model.as_str() {
                "text-embedding-3-small" => 1536,
                "text-embedding-3-large" => 3072,
                "text-embedding-ada-002" => 1536,
                _ => 1536, // Default to small model dimensions
            },
            EmbeddingProvider::Ollama => {
                // Ollama models vary in dimensions, but many common ones use these sizes
                match self.model.as_str() {
                    "gpt-oss:20b" => 4096, // Assuming this model has 4096 dimensions
                    "nomic-embed-text" => 768,
                    "mxbai-embed-large" => 1024,
                    "all-minilm" => 384,
                    _ => 768, // Default dimension for many embedding models
                }
            }
            EmbeddingProvider::Mock => 768, // Consistent mock embedding dimension
        }
    }

    /// Get the provider type
    pub fn provider(&self) -> &EmbeddingProvider {
        &self.provider
    }

    /// Auto-detect and configure the best available embedding model
    pub async fn auto_configure(base_url: String) -> Result<Self> {
        info!("🔍 Auto-detecting best available embedding model...");

        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .context("Failed to create HTTP client")?;

        // Try to get available models from Ollama
        let available_models = Self::detect_ollama_models(&client, &base_url).await?;

        if available_models.is_empty() {
            return Err(anyhow::anyhow!(
                "No embedding models found on Ollama server"
            ));
        }

        // Select the best available model
        let selected_model = Self::select_best_model(&available_models)?;

        info!(
            "✅ Selected model: {} ({}D)",
            selected_model.name, selected_model.dimensions
        );

        let mut embedder = Self::new_ollama(base_url, selected_model.name.clone());
        embedder.fallback_models = available_models
            .into_iter()
            .filter(|m| m.name != embedder.model)
            .map(|m| m.name)
            .collect();

        Ok(embedder)
    }

    /// Generate embedding with automatic fallback to alternative models
    pub async fn generate_embedding_with_fallback(&self, text: &str) -> Result<Vec<f32>> {
        // Try the primary model first
        match self.generate_embedding(text).await {
            Ok(embedding) => return Ok(embedding),
            Err(e) => {
                warn!("Primary model '{}' failed: {}", self.model, e);
            }
        }

        // Try fallback models
        for fallback_model in &self.fallback_models {
            info!("🔄 Trying fallback model: {}", fallback_model);

            let mut fallback_embedder = self.clone();
            fallback_embedder.model = fallback_model.clone();

            match fallback_embedder.generate_embedding(text).await {
                Ok(embedding) => {
                    info!("✅ Fallback model '{}' succeeded", fallback_model);
                    return Ok(embedding);
                }
                Err(e) => {
                    warn!("Fallback model '{}' failed: {}", fallback_model, e);
                    continue;
                }
            }
        }

        Err(anyhow::anyhow!(
            "All embedding models failed, including fallbacks"
        ))
    }

    /// Health check for the embedding service
    pub async fn health_check(&self) -> Result<EmbeddingHealth> {
        let start_time = std::time::Instant::now();

        let test_result = self.generate_embedding("Health check test").await;
        let response_time = start_time.elapsed();

        let health = match test_result {
            Ok(embedding) => EmbeddingHealth {
                status: "healthy".to_string(),
                model: self.model.clone(),
                provider: format!("{:?}", self.provider),
                response_time_ms: response_time.as_millis() as u64,
                embedding_dimensions: embedding.len(),
                error: None,
            },
            Err(e) => EmbeddingHealth {
                status: "unhealthy".to_string(),
                model: self.model.clone(),
                provider: format!("{:?}", self.provider),
                response_time_ms: response_time.as_millis() as u64,
                embedding_dimensions: 0,
                error: Some(e.to_string()),
            },
        };

        Ok(health)
    }

    /// Detect available embedding models on Ollama
    async fn detect_ollama_models(
        client: &Client,
        base_url: &str,
    ) -> Result<Vec<EmbeddingModelInfo>> {
        let response = client
            .get(&format!("{}/api/tags", base_url))
            .send()
            .await
            .context("Failed to connect to Ollama API")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Ollama API returned error: {}",
                response.status()
            ));
        }

        let models_response: OllamaModelsResponse = response
            .json()
            .await
            .context("Failed to parse Ollama models response")?;

        let mut embedding_models = Vec::new();

        for model in models_response.models {
            if let Some(model_info) = Self::classify_embedding_model(&model.name) {
                embedding_models.push(model_info);
            }
        }

        Ok(embedding_models)
    }

    /// Classify a model name as an embedding model
    fn classify_embedding_model(model_name: &str) -> Option<EmbeddingModelInfo> {
        let name_lower = model_name.to_lowercase();

        // Define known embedding models with their properties
        let known_models = [
            (
                "nomic-embed-text",
                768,
                "High-quality text embeddings",
                true,
            ),
            (
                "mxbai-embed-large",
                1024,
                "Large multilingual embeddings",
                true,
            ),
            ("all-minilm", 384, "Compact sentence embeddings", false),
            (
                "all-mpnet-base-v2",
                768,
                "Sentence transformer embeddings",
                false,
            ),
            ("bge-small-en", 384, "BGE small English embeddings", false),
            ("bge-base-en", 768, "BGE base English embeddings", false),
            ("bge-large-en", 1024, "BGE large English embeddings", false),
            ("e5-small", 384, "E5 small embeddings", false),
            ("e5-base", 768, "E5 base embeddings", false),
            ("e5-large", 1024, "E5 large embeddings", false),
        ];

        for (pattern, dimensions, description, preferred) in known_models {
            if name_lower.contains(pattern) || model_name.contains(pattern) {
                return Some(EmbeddingModelInfo {
                    name: model_name.to_string(),
                    dimensions,
                    description: description.to_string(),
                    preferred,
                });
            }
        }

        // Check if it's likely an embedding model based on common patterns
        if name_lower.contains("embed")
            || name_lower.contains("sentence")
            || name_lower.contains("vector")
        {
            return Some(EmbeddingModelInfo {
                name: model_name.to_string(),
                dimensions: 768, // Default assumption
                description: "Detected embedding model".to_string(),
                preferred: false,
            });
        }

        None
    }

    /// Select the best model from available options
    fn select_best_model(available_models: &[EmbeddingModelInfo]) -> Result<&EmbeddingModelInfo> {
        // Prefer recommended models first
        if let Some(preferred) = available_models.iter().find(|m| m.preferred) {
            return Ok(preferred);
        }

        // Fall back to any available model
        available_models
            .first()
            .ok_or_else(|| anyhow::anyhow!("No embedding models available"))
    }
}

/// Information about an embedding model
#[derive(Debug, Clone)]
pub struct EmbeddingModelInfo {
    pub name: String,
    pub dimensions: usize,
    pub description: String,
    pub preferred: bool,
}

/// Health status of the embedding service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingHealth {
    pub status: String,
    pub model: String,
    pub provider: String,
    pub response_time_ms: u64,
    pub embedding_dimensions: usize,
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires actual OpenAI API key
    async fn test_generate_openai_embedding() {
        let api_key = std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY not set");
        let embedder = SimpleEmbedder::new(api_key);

        let result = embedder.generate_embedding("Hello, world!").await;
        assert!(result.is_ok());

        let embedding = result.unwrap();
        assert_eq!(embedding.len(), 1536);
    }

    #[tokio::test]
    #[ignore] // Requires running Ollama instance
    async fn test_generate_ollama_embedding() {
        let embedder = SimpleEmbedder::new_ollama(
            "http://192.168.1.110:11434".to_string(),
            "nomic-embed-text".to_string(),
        );

        let result = embedder.generate_embedding("Hello, world!").await;
        assert!(result.is_ok());

        let embedding = result.unwrap();
        assert_eq!(embedding.len(), 768);
    }

    #[test]
    fn test_embedding_dimensions() {
        let embedder = SimpleEmbedder::new("dummy_key".to_string());
        assert_eq!(embedder.embedding_dimension(), 1536);

        let embedder = embedder.with_model("text-embedding-3-large".to_string());
        assert_eq!(embedder.embedding_dimension(), 3072);

        let ollama_embedder = SimpleEmbedder::new_ollama(
            "http://localhost:11434".to_string(),
            "nomic-embed-text".to_string(),
        );
        assert_eq!(ollama_embedder.embedding_dimension(), 768);

        let gpt_oss_embedder = SimpleEmbedder::new_ollama(
            "http://localhost:11434".to_string(),
            "gpt-oss:20b".to_string(),
        );
        assert_eq!(gpt_oss_embedder.embedding_dimension(), 4096);

        let mock_embedder = SimpleEmbedder::new_mock();
        assert_eq!(mock_embedder.embedding_dimension(), 768);
    }

    #[test]
    fn test_provider_types() {
        let openai_embedder = SimpleEmbedder::new("dummy_key".to_string());
        assert_eq!(openai_embedder.provider(), &EmbeddingProvider::OpenAI);

        let ollama_embedder = SimpleEmbedder::new_ollama(
            "http://localhost:11434".to_string(),
            "nomic-embed-text".to_string(),
        );
        assert_eq!(ollama_embedder.provider(), &EmbeddingProvider::Ollama);

        let mock_embedder = SimpleEmbedder::new_mock();
        assert_eq!(mock_embedder.provider(), &EmbeddingProvider::Mock);
    }
}
