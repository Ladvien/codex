use anyhow::Result;
use chrono::{DateTime, Utc};
use codex_memory::config::Config;
use codex_memory::memory::connection::create_pool;
use sqlx::{PgPool, Row};

#[tokio::main]
async fn main() -> Result<()> {
    // Load configuration
    let config = Config::from_env()?;

    // Create database connection pool
    let pool = create_pool(&config.database_url, 10).await?;

    // Search for memories containing "MCP server"
    println!("Searching for memories containing 'MCP server'...");
    let results1 = search_text(&pool, "MCP server").await?;
    print_results("MCP server", &results1);

    // Search for memories containing "PID 62513"
    println!("\n\nSearching for memories containing 'PID 62513'...");
    let results2 = search_text(&pool, "PID 62513").await?;
    print_results("PID 62513", &results2);

    // Search for memories containing "Enhanced Agentic Memory System v2.0"
    println!("\n\nSearching for memories containing 'Enhanced Agentic Memory System v2.0'...");
    let results3 = search_text(&pool, "Enhanced Agentic Memory System v2.0").await?;
    print_results("Enhanced Agentic Memory System v2.0", &results3);

    Ok(())
}

async fn search_text(pool: &PgPool, query: &str) -> Result<Vec<MemoryResult>> {
    let query_pattern = format!("%{}%", query);

    let rows = sqlx::query(
        r#"
        SELECT id, content, tier, created_at, importance_score, access_count, metadata
        FROM memories 
        WHERE content ILIKE $1 
        AND status = 'active'
        ORDER BY created_at DESC 
        LIMIT 10
        "#,
    )
    .bind(&query_pattern)
    .fetch_all(pool)
    .await?;

    let mut results = Vec::new();
    for row in rows {
        results.push(MemoryResult {
            id: row.get("id"),
            content: row.get("content"),
            tier: row.get("tier"),
            created_at: row.get("created_at"),
            importance_score: row.get("importance_score"),
            access_count: row.get("access_count"),
            metadata: row.get("metadata"),
        });
    }

    Ok(results)
}

fn print_results(query: &str, results: &[MemoryResult]) {
    if results.is_empty() {
        println!("No memories found for query: '{}'", query);
    } else {
        println!("Found {} memories:", results.len());
        for (i, result) in results.iter().enumerate() {
            println!("\n--- Memory {} ---", i + 1);
            println!("ID: {}", result.id);
            println!("Tier: {}", result.tier);
            println!("Created: {}", result.created_at.format("%Y-%m-%d %H:%M:%S"));
            println!("Importance: {:.3}", result.importance_score);
            println!("Access Count: {}", result.access_count);
            println!("Content: {}", result.content);

            // Try to parse metadata as tags
            if let Ok(metadata_obj) = serde_json::from_value::<
                serde_json::Map<String, serde_json::Value>,
            >(result.metadata.clone())
            {
                if let Some(tags) = metadata_obj.get("tags") {
                    if let Ok(tag_list) = serde_json::from_value::<Vec<String>>(tags.clone()) {
                        if !tag_list.is_empty() {
                            println!("Tags: {:?}", tag_list);
                        }
                    }
                }
            }
        }
    }
}

#[derive(Debug)]
struct MemoryResult {
    id: uuid::Uuid,
    content: String,
    tier: String,
    created_at: DateTime<Utc>,
    importance_score: f64,
    access_count: i32,
    metadata: serde_json::Value,
}
