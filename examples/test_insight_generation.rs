use anyhow::Result;
use codex_memory::{
    insights::{
        ollama_client::{OllamaClient, OllamaConfig},
        processor::{InsightsProcessor, ProcessorConfig},
        storage::InsightStorage,
    },
    memory::MemoryRepository,
    Config, SimpleEmbedder,
};
use sqlx::postgres::PgPool;
use std::sync::Arc;
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Load environment
    dotenv::dotenv().ok();

    info!("Starting insight generation test...");

    // Connect to database
    let database_url = std::env::var("DATABASE_URL")?;
    let pool = Arc::new(PgPool::connect(&database_url).await?);
    info!("Connected to database");

    // Create embedder (using Ollama)
    let embedder = Arc::new(SimpleEmbedder::new_ollama(
        "http://192.168.1.110:11434".to_string(),
        "nomic-embed-text".to_string(),
    ));
    info!("Created embedder");

    // Create repository
    let config = Config::from_env()?;
    let repository = Arc::new(MemoryRepository::with_config((*pool).clone(), config));
    info!("Created memory repository");

    // Create Ollama client
    let ollama_config = OllamaConfig {
        base_url: "http://192.168.1.110:11434".to_string(),
        model: "llama3.2:latest".to_string(), // Using a model that should work
        timeout_seconds: 60,
        max_retries: 3,
        initial_retry_delay_ms: 1000,
        max_retry_delay_ms: 10000,
    };

    let ollama_client = match OllamaClient::new(ollama_config) {
        Ok(client) => {
            info!("Created Ollama client");
            Arc::new(client)
        }
        Err(e) => {
            error!("Failed to create Ollama client: {}", e);
            return Err(e.into());
        }
    };

    // Create insight storage
    let insight_storage = Arc::new(InsightStorage::new(pool.clone(), embedder.clone()));
    info!("Created insight storage");

    // Create processor config
    let processor_config = ProcessorConfig {
        batch_size: 10,
        max_retries: 3,
        timeout_seconds: 120,
        circuit_breaker_threshold: 5,
        circuit_breaker_recovery_timeout: 60,
        min_confidence_threshold: 0.5,
        max_insights_per_batch: 5,
    };

    // Create insights processor
    let mut processor = InsightsProcessor::new(
        repository.clone(),
        ollama_client,
        insight_storage.clone(),
        processor_config,
    );
    info!("Created insights processor");

    // Get recent memories (without semantic search, just chronological)
    info!("Fetching recent memories...");
    let search_request = codex_memory::memory::models::SearchRequest {
        query_text: None,      // No text search
        query_embedding: None, // No semantic search, just get recent memories
        limit: Some(10),
        date_range: Some(codex_memory::memory::models::DateRange {
            start: Some(chrono::Utc::now() - chrono::Duration::days(7)),
            end: None,
        }),
        ..Default::default()
    };

    let search_response = repository.search_memories(search_request).await?;
    info!("Found {} memories", search_response.results.len());

    if search_response.results.is_empty() {
        error!("No memories found to process");
        return Ok(());
    }

    // Extract memory IDs
    let memory_ids: Vec<_> = search_response
        .results
        .iter()
        .take(5)
        .map(|r| r.memory.id)
        .collect();

    info!("Processing {} memories for insights...", memory_ids.len());

    // Process batch
    match processor.process_batch(memory_ids).await {
        Ok(result) => {
            info!("Processing complete!");
            info!("  Generated {} insights", result.insights.len());
            info!("  Success rate: {:.1}%", result.report.success_rate * 100.0);
            info!("  Duration: {:.2}s", result.report.duration_seconds);

            for (i, insight) in result.insights.iter().enumerate() {
                info!(
                    "Insight {}: {}",
                    i + 1,
                    &insight.content[..100.min(insight.content.len())]
                );
                info!(
                    "  Type: {:?}, Confidence: {:.1}%",
                    insight.insight_type,
                    insight.confidence_score * 100.0
                );
            }
        }
        Err(e) => {
            error!("Failed to process batch: {}", e);
        }
    }

    // Check database for insights
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM insights")
        .fetch_one(pool.as_ref())
        .await?;

    info!("Total insights in database: {}", count);

    Ok(())
}
