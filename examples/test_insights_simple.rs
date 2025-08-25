use anyhow::Result;
use codex_memory::Config;
use sqlx::postgres::PgPool;
use std::sync::Arc;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Load environment
    dotenv::dotenv().ok();

    info!("Starting simple insight generation test...");

    // Connect to database
    let database_url = std::env::var("DATABASE_URL")?;
    let pool = Arc::new(PgPool::connect(&database_url).await?);
    info!("Connected to database");

    // First, let's just check if we can store an insight directly
    info!("Testing direct insight storage...");

    // Create a test insight
    let insert_query = r#"
        INSERT INTO insights (
            content,
            insight_type,
            confidence_score,
            source_memory_ids
        ) VALUES (
            'Test insight: The Codex Dreams system has been successfully integrated with MCP handlers',
            'learning',
            0.85,
            ARRAY['55950a9f-316f-4df0-873f-01a8e357cdbf'::uuid]
        )
        RETURNING id
    "#;

    let result: (uuid::Uuid,) = sqlx::query_as(insert_query)
        .fetch_one(pool.as_ref())
        .await?;

    info!("Successfully created insight with ID: {}", result.0);

    // Check the count
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM insights")
        .fetch_one(pool.as_ref())
        .await?;

    info!("Total insights in database: {}", count);

    // Retrieve the insight
    let retrieve_query =
        "SELECT content, insight_type, confidence_score FROM insights WHERE id = $1";
    let (content, insight_type, confidence): (String, String, f64) = sqlx::query_as(retrieve_query)
        .bind(result.0)
        .fetch_one(pool.as_ref())
        .await?;

    info!("Retrieved insight:");
    info!("  Content: {}", content);
    info!("  Type: {}", insight_type);
    info!("  Confidence: {:.1}%", confidence * 100.0);

    Ok(())
}
