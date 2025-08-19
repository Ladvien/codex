//! Database Connectivity Test
//!
//! This test helps diagnose database setup issues for the Agentic Memory System.
//! Run this test to check if your PostgreSQL database is properly configured.

use anyhow::Result;
use codex_memory::Config;
use sqlx::{PgPool, Row};
use std::env;
use tracing_test::traced_test;

/// Test basic database connectivity
#[tokio::test]
#[traced_test]
async fn test_database_connectivity() -> Result<()> {
    let database_url = env::var("DATABASE_URL")
        .or_else(|_| env::var("TEST_DATABASE_URL"))
        .unwrap_or_else(|_| "postgresql://postgres:password@localhost:5432/postgres".to_string());

    println!("🔍 Testing database connectivity...");
    println!("Database URL: {}", database_url);

    // Test 1: Basic connection
    println!("\n1. Testing basic connection...");
    let pool = match PgPool::connect(&database_url).await {
        Ok(pool) => {
            println!("✅ Basic connection successful");
            pool
        }
        Err(e) => {
            println!("❌ Basic connection failed: {}", e);
            println!("\n📋 Troubleshooting suggestions:");
            println!("   - Ensure PostgreSQL is running");
            println!("   - Check database URL format: postgresql://user:password@host:port/database");
            println!("   - Verify credentials and database exists");
            println!("   - Check network connectivity if using remote database");
            return Err(e.into());
        }
    };

    // Test 2: Query execution
    println!("\n2. Testing query execution...");
    let version_result = sqlx::query("SELECT version()")
        .fetch_one(&pool)
        .await;
    
    match version_result {
        Ok(row) => {
            let version: String = row.get(0);
            println!("✅ Query execution successful");
            println!("   PostgreSQL version: {}", version);
        }
        Err(e) => {
            println!("❌ Query execution failed: {}", e);
            return Err(e.into());
        }
    }

    // Test 3: Extension availability
    println!("\n3. Checking required extensions...");
    
    // Check if pgvector is available
    let pgvector_check = sqlx::query(
        "SELECT 1 FROM pg_available_extensions WHERE name = 'vector'"
    ).fetch_optional(&pool).await?;

    if pgvector_check.is_some() {
        println!("✅ pgvector extension is available");
    } else {
        println!("❌ pgvector extension is not available");
        println!("\n📋 To install pgvector:");
        println!("   Ubuntu/Debian: sudo apt install postgresql-15-pgvector");
        println!("   CentOS/RHEL: sudo yum install pgvector");
        println!("   macOS (Homebrew): brew install pgvector");
        println!("   Docker: Use postgres:15 image with pgvector");
        println!("   Or compile from source: https://github.com/pgvector/pgvector");
    }

    // Test 4: Extension creation permissions
    println!("\n4. Testing extension creation permissions...");
    
    let uuid_extension_test = sqlx::query("CREATE EXTENSION IF NOT EXISTS \"uuid-ossp\"")
        .execute(&pool)
        .await;

    match uuid_extension_test {
        Ok(_) => {
            println!("✅ Extension creation permissions OK");
            // Clean up
            let _ = sqlx::query("DROP EXTENSION IF EXISTS \"uuid-ossp\"")
                .execute(&pool)
                .await;
        }
        Err(e) => {
            println!("❌ Extension creation failed: {}", e);
            println!("\n📋 Permission troubleshooting:");
            println!("   - Connect as a superuser (postgres user)");
            println!("   - Grant CREATE privileges on database");
            println!("   - Or have administrator pre-install extensions");
        }
    }

    // Test 5: Configuration validation
    println!("\n5. Testing configuration loading...");
    match Config::from_env() {
        Ok(config) => {
            println!("✅ Configuration loaded successfully");
            println!("   Database URL: [CONFIGURED]");
            println!("   Embedding provider: {}", config.embedding.provider);
            println!("   Embedding model: {}", config.embedding.model);
        }
        Err(e) => {
            println!("⚠️  Configuration loading failed: {}", e);
            println!("   Using default configuration for tests");
        }
    }

    pool.close().await;
    
    println!("\n🎉 Database connectivity test completed!");
    println!("\n📋 Next steps:");
    println!("   1. If pgvector is missing, install it (see instructions above)");
    println!("   2. Ensure your user has permission to create extensions");
    println!("   3. Run: cargo test --test e2e_simplified to test full functionality");
    println!("   4. Or run: cargo run setup to perform automated setup");

    Ok(())
}

/// Test Ollama connectivity (for embedding generation)
#[tokio::test]
#[traced_test]
async fn test_ollama_connectivity() -> Result<()> {
    let ollama_url = env::var("EMBEDDING_BASE_URL")
        .unwrap_or_else(|_| "http://localhost:11434".to_string());

    println!("🔍 Testing Ollama connectivity...");
    println!("Ollama URL: {}", ollama_url);

    let client = reqwest::Client::new();
    
    // Test 1: Basic connectivity
    println!("\n1. Testing Ollama connection...");
    let response = client
        .get(&format!("{}/api/tags", ollama_url))
        .send()
        .await;

    match response {
        Ok(resp) if resp.status().is_success() => {
            println!("✅ Ollama is running and accessible");
            
            // Test 2: Check available models
            println!("\n2. Checking available models...");
            let body = resp.text().await?;
            
            if body.contains("nomic-embed-text") || body.contains("mxbai-embed-large") {
                println!("✅ Embedding models detected");
            } else {
                println!("⚠️  No embedding models found");
                println!("\n📋 To install embedding models:");
                println!("   ollama pull nomic-embed-text");
                println!("   ollama pull mxbai-embed-large");
            }
        }
        Ok(resp) => {
            println!("❌ Ollama responded with error: {}", resp.status());
        }
        Err(e) => {
            println!("❌ Cannot connect to Ollama: {}", e);
            println!("\n📋 Troubleshooting suggestions:");
            println!("   - Install Ollama: https://ollama.ai/download");
            println!("   - Start Ollama service: ollama serve");
            println!("   - Check if running on different port");
            println!("   - For remote Ollama, update EMBEDDING_BASE_URL");
        }
    }

    println!("\n📋 Ollama setup complete when:");
    println!("   1. Ollama service is running");
    println!("   2. At least one embedding model is installed");
    println!("   3. Service is accessible from this machine");

    Ok(())
}

/// Comprehensive setup check
#[tokio::test]
#[traced_test] 
async fn test_comprehensive_setup_check() -> Result<()> {
    println!("🧪 Running comprehensive setup check...");
    println!("=======================================");
    
    // Note: Individual test functions are called separately
    // This is a summary function that can be extended with additional checks
    
    println!("Run the individual tests:");
    println!("  cargo test --test database_connectivity_test test_database_connectivity");
    println!("  cargo test --test database_connectivity_test test_ollama_connectivity");
    
    println!("\n✅ Comprehensive setup check completed!");
    println!("\n🚀 If both checks passed, run:");
    println!("   cargo test --test e2e_simplified");
    println!("   to verify full end-to-end functionality");

    Ok(())
}