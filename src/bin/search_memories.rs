use anyhow::Result;
use codex_memory::config::Config;
use codex_memory::memory::connection::create_pool;
use codex_memory::memory::models::SearchRequest;
use codex_memory::memory::repository::MemoryRepository;

#[tokio::main]
async fn main() -> Result<()> {
    // Load configuration
    let config = Config::from_env()?;

    // Create database connection pool
    let pool = create_pool(&config.database_url, 10).await?;

    // Create repository
    let repository = MemoryRepository::new(pool);

    // Search for memories containing "MCP server"
    let search_request = SearchRequest {
        query_text: Some("MCP server".to_string()),
        query_embedding: None,
        search_type: None,
        hybrid_weights: None,
        tier: None,
        date_range: None,
        importance_range: None,
        metadata_filters: None,
        tags: None,
        limit: Some(10),
        offset: Some(0),
        cursor: None,
        similarity_threshold: Some(0.3),
        include_facets: None,
        include_metadata: Some(true),
        ranking_boost: None,
        explain_score: None,
    };

    println!("Searching for memories containing 'MCP server'...");
    let results = repository.search_memories_simple(search_request).await?;

    if results.is_empty() {
        println!("No memories found for query: 'MCP server'");
    } else {
        println!("Found {} memories:", results.len());
        for (i, result) in results.iter().enumerate() {
            println!("\n--- Memory {} ---", i + 1);
            println!("Score: {:.3}", result.similarity_score);
            println!("Tier: {:?}", result.memory.tier);
            println!(
                "Created: {}",
                result.memory.created_at.format("%Y-%m-%d %H:%M:%S")
            );
            println!("Content: {}", result.memory.content);
            if let Ok(tags) = serde_json::from_value::<Vec<String>>(result.memory.metadata.clone())
            {
                if !tags.is_empty() {
                    println!("Tags: {:?}", tags);
                }
            }
        }
    }

    // Search for memories containing "PID 62513"
    let search_request2 = SearchRequest {
        query_text: Some("PID 62513".to_string()),
        query_embedding: None,
        search_type: None,
        hybrid_weights: None,
        tier: None,
        date_range: None,
        importance_range: None,
        metadata_filters: None,
        tags: None,
        limit: Some(10),
        offset: Some(0),
        cursor: None,
        similarity_threshold: Some(0.3),
        include_facets: None,
        include_metadata: Some(true),
        ranking_boost: None,
        explain_score: None,
    };

    println!("\n\nSearching for memories containing 'PID 62513'...");
    let results2 = repository.search_memories_simple(search_request2).await?;

    if results2.is_empty() {
        println!("No memories found for query: 'PID 62513'");
    } else {
        println!("Found {} memories:", results2.len());
        for (i, result) in results2.iter().enumerate() {
            println!("\n--- Memory {} ---", i + 1);
            println!("Score: {:.3}", result.similarity_score);
            println!("Tier: {:?}", result.memory.tier);
            println!(
                "Created: {}",
                result.memory.created_at.format("%Y-%m-%d %H:%M:%S")
            );
            println!("Content: {}", result.memory.content);
            if let Ok(tags) = serde_json::from_value::<Vec<String>>(result.memory.metadata.clone())
            {
                if !tags.is_empty() {
                    println!("Tags: {:?}", tags);
                }
            }
        }
    }

    // Search for memories containing "Enhanced Agentic Memory System v2.0"
    let search_request3 = SearchRequest {
        query_text: Some("Enhanced Agentic Memory System v2.0".to_string()),
        query_embedding: None,
        search_type: None,
        hybrid_weights: None,
        tier: None,
        date_range: None,
        importance_range: None,
        metadata_filters: None,
        tags: None,
        limit: Some(10),
        offset: Some(0),
        cursor: None,
        similarity_threshold: Some(0.3),
        include_facets: None,
        include_metadata: Some(true),
        ranking_boost: None,
        explain_score: None,
    };

    println!("\n\nSearching for memories containing 'Enhanced Agentic Memory System v2.0'...");
    let results3 = repository.search_memories_simple(search_request3).await?;

    if results3.is_empty() {
        println!("No memories found for query: 'Enhanced Agentic Memory System v2.0'");
    } else {
        println!("Found {} memories:", results3.len());
        for (i, result) in results3.iter().enumerate() {
            println!("\n--- Memory {} ---", i + 1);
            println!("Score: {:.3}", result.similarity_score);
            println!("Tier: {:?}", result.memory.tier);
            println!(
                "Created: {}",
                result.memory.created_at.format("%Y-%m-%d %H:%M:%S")
            );
            println!("Content: {}", result.memory.content);
            if let Ok(tags) = serde_json::from_value::<Vec<String>>(result.memory.metadata.clone())
            {
                if !tags.is_empty() {
                    println!("Tags: {:?}", tags);
                }
            }
        }
    }

    Ok(())
}
