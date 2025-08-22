use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio_postgres::{Config as PgConfig, NoTls};
use tracing::{error, info, warn};

use crate::config::Config;
use crate::embedding::SimpleEmbedder;

/// Available embedding models with their configurations
#[derive(Debug, Clone)]
pub struct EmbeddingModelInfo {
    pub name: String,
    pub dimensions: usize,
    pub description: String,
    pub preferred: bool,
}

/// Setup manager for the Agentic Memory System
pub struct SetupManager {
    client: Client,
    config: Config,
}

/// Ollama API response structures
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

#[derive(Debug, Serialize)]
struct OllamaPullRequest {
    name: String,
}

#[derive(Debug, Deserialize)]
struct OllamaPullResponse {
    status: String,
    #[serde(default)]
    completed: Option<u64>,
    #[serde(default)]
    total: Option<u64>,
}

impl SetupManager {
    pub fn new(config: Config) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .expect("Failed to create HTTP client");

        Self { client, config }
    }

    /// Run complete setup process
    pub async fn run_setup(&self) -> Result<()> {
        info!("🚀 Starting Agentic Memory System setup...");

        // 1. Check Ollama connectivity
        self.check_ollama_connectivity().await?;

        // 2. Detect and pull embedding models
        let available_models = self.detect_embedding_models().await?;
        let selected_model = self.ensure_embedding_model(available_models).await?;

        // 3. Update configuration with selected model
        let mut updated_config = self.config.clone();
        updated_config.embedding.model = selected_model.name.clone();

        // 4. Test embedding generation
        self.test_embedding_generation(&updated_config).await?;

        // 5. Setup database
        self.setup_database().await?;

        // 6. Run comprehensive health checks
        self.run_health_checks(&updated_config).await?;

        info!("✅ Setup completed successfully!");
        info!(
            "Selected embedding model: {} ({}D)",
            selected_model.name, selected_model.dimensions
        );

        Ok(())
    }

    /// Check if Ollama is running and accessible
    async fn check_ollama_connectivity(&self) -> Result<()> {
        info!(
            "🔍 Checking Ollama connectivity at {}",
            self.config.embedding.base_url
        );

        let response = self
            .client
            .get(format!("{}/api/tags", self.config.embedding.base_url))
            .send()
            .await
            .context("Failed to connect to Ollama. Is it running and accessible?")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Ollama returned error status: {}",
                response.status()
            ));
        }

        info!("✅ Ollama is running and accessible");
        Ok(())
    }

    /// Detect available embedding models on Ollama
    async fn detect_embedding_models(&self) -> Result<Vec<EmbeddingModelInfo>> {
        info!("🔍 Detecting available embedding models...");

        let response = self
            .client
            .get(format!("{}/api/tags", self.config.embedding.base_url))
            .send()
            .await?;

        let models_response: OllamaModelsResponse = response.json().await?;

        let mut embedding_models = Vec::new();

        for model in models_response.models {
            if let Some(model_info) = self.classify_embedding_model(&model.name) {
                embedding_models.push(model_info);
            }
        }

        if embedding_models.is_empty() {
            warn!("No embedding models found on Ollama");
        } else {
            info!("Found {} embedding models:", embedding_models.len());
            for model in &embedding_models {
                info!(
                    "  - {} ({}D) {}",
                    model.name,
                    model.dimensions,
                    if model.preferred {
                        "⭐ RECOMMENDED"
                    } else {
                        ""
                    }
                );
            }
        }

        Ok(embedding_models)
    }

    /// Classify a model name as an embedding model and return its info
    fn classify_embedding_model(&self, model_name: &str) -> Option<EmbeddingModelInfo> {
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

    /// Ensure a suitable embedding model is available, pulling if necessary
    async fn ensure_embedding_model(
        &self,
        available_models: Vec<EmbeddingModelInfo>,
    ) -> Result<EmbeddingModelInfo> {
        info!("🎯 Selecting embedding model...");

        // If we have a preferred model available, use it
        if let Some(preferred) = available_models.iter().find(|m| m.preferred) {
            info!("✅ Using preferred model: {}", preferred.name);
            return Ok(preferred.clone());
        }

        // If we have any available model, use the first one
        if !available_models.is_empty() {
            let selected = available_models[0].clone();
            info!("✅ Using available model: {}", selected.name);
            return Ok(selected);
        }

        // No embedding models available, try to pull recommended ones
        info!("📥 No embedding models found. Attempting to pull recommended models...");

        let recommended_models = [
            ("nomic-embed-text", 768, "High-quality text embeddings"),
            ("mxbai-embed-large", 1024, "Large multilingual embeddings"),
            ("all-minilm", 384, "Compact sentence embeddings"),
        ];

        for (model_name, dimensions, description) in recommended_models {
            info!("📥 Attempting to pull model: {}", model_name);

            match self.pull_model(model_name).await {
                Ok(_) => {
                    info!("✅ Successfully pulled model: {}", model_name);
                    return Ok(EmbeddingModelInfo {
                        name: model_name.to_string(),
                        dimensions,
                        description: description.to_string(),
                        preferred: true,
                    });
                }
                Err(e) => {
                    warn!("Failed to pull model {}: {}", model_name, e);
                    continue;
                }
            }
        }

        Err(anyhow::anyhow!(
            "Failed to find or pull any suitable embedding models. Please manually pull an embedding model using 'ollama pull nomic-embed-text'"
        ))
    }

    /// Pull a model from Ollama
    async fn pull_model(&self, model_name: &str) -> Result<()> {
        info!("📥 Pulling model: {}", model_name);

        let request = OllamaPullRequest {
            name: model_name.to_string(),
        };

        let response = self
            .client
            .post(format!("{}/api/pull", self.config.embedding.base_url))
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(anyhow::anyhow!(
                "Failed to pull model {}: HTTP {} - {}",
                model_name,
                status,
                error_text
            ));
        }

        // Stream the response to show progress
        let lines = response.text().await?;

        // Ollama returns JSONL (JSON Lines) for streaming responses
        for line in lines.lines() {
            if line.trim().is_empty() {
                continue;
            }

            match serde_json::from_str::<OllamaPullResponse>(line) {
                Ok(pull_response) => match pull_response.status.as_str() {
                    "downloading" => {
                        if let (Some(completed), Some(total)) =
                            (pull_response.completed, pull_response.total)
                        {
                            let progress = (completed as f64 / total as f64) * 100.0;
                            info!(
                                "  📊 Downloading: {:.1}% ({}/{})",
                                progress, completed, total
                            );
                        }
                    }
                    "verifying sha256" => {
                        info!("  🔍 Verifying checksum...");
                    }
                    "success" => {
                        info!("  ✅ Pull completed successfully");
                        return Ok(());
                    }
                    status => {
                        info!("  📦 Status: {}", status);
                    }
                },
                Err(_) => {
                    // Sometimes Ollama sends non-JSON status lines
                    if line.contains("success") {
                        info!("  ✅ Pull completed successfully");
                        return Ok(());
                    }
                    info!("  📦 {}", line);
                }
            }
        }

        Ok(())
    }

    /// Test embedding generation with the selected model
    async fn test_embedding_generation(&self, config: &Config) -> Result<()> {
        info!("🧪 Testing embedding generation...");

        let embedder = SimpleEmbedder::new_ollama(
            config.embedding.base_url.clone(),
            config.embedding.model.clone(),
        );

        let test_text = "This is a test sentence for embedding generation.";

        match embedder.generate_embedding(test_text).await {
            Ok(embedding) => {
                info!("✅ Embedding generation successful!");
                info!("  📊 Embedding dimensions: {}", embedding.len());
                info!(
                    "  📊 Sample values: [{:.4}, {:.4}, {:.4}, ...]",
                    embedding.first().unwrap_or(&0.0),
                    embedding.get(1).unwrap_or(&0.0),
                    embedding.get(2).unwrap_or(&0.0)
                );
                Ok(())
            }
            Err(e) => {
                error!("❌ Embedding generation failed: {}", e);
                Err(e)
            }
        }
    }

    /// Setup database with required extensions and tables
    async fn setup_database(&self) -> Result<()> {
        info!("🗄️  Setting up database...");

        // Parse the database URL
        let db_config: PgConfig = self
            .config
            .database_url
            .parse()
            .context("Invalid database URL")?;

        // Connect to the database
        let (client, connection) = db_config
            .connect(NoTls)
            .await
            .context("Failed to connect to database")?;

        // Spawn the connection
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                error!("Database connection error: {}", e);
            }
        });

        // Check if pgvector extension is available
        info!("🔍 Checking for pgvector extension...");

        let extension_check = client
            .query(
                "SELECT 1 FROM pg_available_extensions WHERE name = 'vector'",
                &[],
            )
            .await?;

        if extension_check.is_empty() {
            warn!("⚠️  pgvector extension is not available in this PostgreSQL instance");
            warn!("   Please install pgvector: https://github.com/pgvector/pgvector");
            return Err(anyhow::anyhow!("pgvector extension not available"));
        }

        // Enable pgvector extension
        info!("🔧 Enabling pgvector extension...");
        client
            .execute("CREATE EXTENSION IF NOT EXISTS vector", &[])
            .await
            .context("Failed to enable pgvector extension")?;

        // Check if our tables exist
        info!("🔍 Checking database schema...");
        let table_check = client
            .query(
                "SELECT table_name FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'memories'",
                &[]
            )
            .await?;

        if table_check.is_empty() {
            info!("📋 Running database migrations...");
            // Run migrations using the migration crate
            // This would typically be done through the migration module
            warn!("⚠️  Please run database migrations: cargo run --bin migration");
        } else {
            info!("✅ Database schema is ready");
        }

        Ok(())
    }

    /// Run comprehensive health checks
    pub async fn run_health_checks(&self, config: &Config) -> Result<()> {
        info!("🩺 Running comprehensive health checks...");

        let mut checks_passed = 0;
        let mut total_checks = 0;

        // Check 1: Ollama connectivity
        total_checks += 1;
        match self.check_ollama_connectivity().await {
            Ok(_) => {
                info!("  ✅ Ollama connectivity");
                checks_passed += 1;
            }
            Err(e) => {
                error!("  ❌ Ollama connectivity: {}", e);
            }
        }

        // Check 2: Embedding model availability
        total_checks += 1;
        let embedder = SimpleEmbedder::new_ollama(
            config.embedding.base_url.clone(),
            config.embedding.model.clone(),
        );

        match embedder.generate_embedding("health check").await {
            Ok(_) => {
                info!("  ✅ Embedding generation");
                checks_passed += 1;
            }
            Err(e) => {
                error!("  ❌ Embedding generation: {}", e);
            }
        }

        // Check 3: Database connectivity
        total_checks += 1;
        match self.check_database_connectivity().await {
            Ok(_) => {
                info!("  ✅ Database connectivity");
                checks_passed += 1;
            }
            Err(e) => {
                error!("  ❌ Database connectivity: {}", e);
            }
        }

        // Check 4: pgvector extension
        total_checks += 1;
        match self.check_pgvector_extension().await {
            Ok(_) => {
                info!("  ✅ pgvector extension");
                checks_passed += 1;
            }
            Err(e) => {
                error!("  ❌ pgvector extension: {}", e);
            }
        }

        // Summary
        info!(
            "📊 Health check summary: {}/{} checks passed",
            checks_passed, total_checks
        );

        if checks_passed == total_checks {
            info!("🎉 All health checks passed! System is ready.");
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "Some health checks failed. Please address the issues above."
            ))
        }
    }

    /// Check database connectivity
    async fn check_database_connectivity(&self) -> Result<()> {
        let db_config: PgConfig = self.config.database_url.parse()?;
        let (client, connection) = db_config.connect(NoTls).await?;

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                error!("Database connection error: {}", e);
            }
        });

        // Simple connectivity test
        client.query("SELECT 1", &[]).await?;
        Ok(())
    }

    /// Check pgvector extension
    async fn check_pgvector_extension(&self) -> Result<()> {
        let db_config: PgConfig = self.config.database_url.parse()?;
        let (client, connection) = db_config.connect(NoTls).await?;

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                error!("Database connection error: {}", e);
            }
        });

        // Check if pgvector is installed and functional
        client
            .query("SELECT vector_dims(vector '[1,2,3]')", &[])
            .await
            .context("pgvector extension not available or not working")?;

        Ok(())
    }

    /// List available models for user selection
    pub async fn list_available_models(&self) -> Result<()> {
        info!("📋 Available embedding models:");

        let available_models = self.detect_embedding_models().await?;

        if available_models.is_empty() {
            info!("  No embedding models currently available");
            info!("  Recommended models to pull:");
            info!("    ollama pull nomic-embed-text");
            info!("    ollama pull mxbai-embed-large");
            info!("    ollama pull all-minilm");
        } else {
            for model in available_models {
                let icon = if model.preferred { "⭐" } else { "  " };
                info!(
                    "{} {} ({}D) - {}",
                    icon, model.name, model.dimensions, model.description
                );
            }
        }

        Ok(())
    }

    /// Quick health check without setup
    pub async fn quick_health_check(&self) -> Result<()> {
        info!("🏥 Running quick health check...");

        // Check Ollama
        match self.check_ollama_connectivity().await {
            Ok(_) => info!("✅ Ollama: Running"),
            Err(_) => info!("❌ Ollama: Not accessible"),
        }

        // Check database
        match self.check_database_connectivity().await {
            Ok(_) => info!("✅ Database: Connected"),
            Err(_) => info!("❌ Database: Connection failed"),
        }

        // Check embedding model
        let embedder = SimpleEmbedder::new_ollama(
            self.config.embedding.base_url.clone(),
            self.config.embedding.model.clone(),
        );

        match embedder.generate_embedding("test").await {
            Ok(_) => info!("✅ Embeddings: Working"),
            Err(_) => info!("❌ Embeddings: Failed"),
        }

        Ok(())
    }
}

/// Create a sample .env file with default configuration
pub fn create_sample_env_file() -> Result<()> {
    let env_content = r#"# Agentic Memory System Configuration

# Database Configuration
DATABASE_URL=postgresql://postgres:postgres@localhost:5432/codex_memory

# Embedding Configuration
EMBEDDING_PROVIDER=ollama
EMBEDDING_MODEL=nomic-embed-text
EMBEDDING_BASE_URL=http://192.168.1.110:11434
EMBEDDING_TIMEOUT_SECONDS=60

# Server Configuration
HTTP_PORT=8080
LOG_LEVEL=info

# Memory Tier Configuration
WORKING_TIER_LIMIT=1000
WARM_TIER_LIMIT=10000
WORKING_TO_WARM_DAYS=7
WARM_TO_COLD_DAYS=30
IMPORTANCE_THRESHOLD=0.7

# Operational Configuration
MAX_DB_CONNECTIONS=10
REQUEST_TIMEOUT_SECONDS=30
ENABLE_METRICS=true
"#;

    std::fs::write(".env.example", env_content).context("Failed to create .env.example file")?;

    info!("📋 Created .env.example file with default configuration");
    info!("   Copy this to .env and modify as needed");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_embedding_model() {
        let setup = SetupManager::new(Config::default());

        // Known models
        let nomic = setup.classify_embedding_model("nomic-embed-text").unwrap();
        assert_eq!(nomic.dimensions, 768);
        assert!(nomic.preferred);

        let mxbai = setup.classify_embedding_model("mxbai-embed-large").unwrap();
        assert_eq!(mxbai.dimensions, 1024);
        assert!(mxbai.preferred);

        // Unknown embedding model
        let unknown = setup
            .classify_embedding_model("custom-embed-model")
            .unwrap();
        assert_eq!(unknown.dimensions, 768); // Default
        assert!(!unknown.preferred);

        // Non-embedding model
        let non_embed = setup.classify_embedding_model("llama2");
        assert!(non_embed.is_none());
    }

    #[test]
    fn test_known_models_classification() {
        let setup = SetupManager::new(Config::default());

        let test_cases = [
            ("nomic-embed-text", true, 768),
            ("all-minilm", false, 384),
            ("bge-base-en", false, 768),
            ("e5-large", false, 1024),
        ];

        for (model_name, expected_preferred, expected_dims) in test_cases {
            let result = setup.classify_embedding_model(model_name);
            assert!(
                result.is_some(),
                "Should classify {} as embedding model",
                model_name
            );

            let info = result.unwrap();
            assert_eq!(info.preferred, expected_preferred);
            assert_eq!(info.dimensions, expected_dims);
        }
    }
}
