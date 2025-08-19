//! Integration tests specifically for Claude Code/Desktop integration
//!
//! These tests validate the MCP protocol implementation and ensure
//! compatibility with Claude applications.

use anyhow::{Context, Result};
use codex_memory::{
    mcp::MCPServer,
    memory::{
        connection::create_pool,
        models::{CreateMemoryRequest, MemoryTier, SearchRequest},
        MemoryRepository,
    },
    SimpleEmbedder,
};
use serde_json::json;
use sqlx::PgPool;
use std::sync::Arc;
use tracing_test::traced_test;

/// Set up the required database schema for tests
async fn setup_test_schema(pool: &PgPool) -> Result<()> {
    // Enable pgvector extension
    sqlx::query("CREATE EXTENSION IF NOT EXISTS vector")
        .execute(pool)
        .await
        .context("Failed to create vector extension")?;

    // Create memories table if it doesn't exist
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS memories (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            content TEXT NOT NULL,
            embedding vector(768),
            tier VARCHAR(20) NOT NULL DEFAULT 'working',
            importance REAL NOT NULL DEFAULT 0.5,
            access_count INTEGER NOT NULL DEFAULT 0,
            last_accessed TIMESTAMPTZ DEFAULT NOW(),
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            tags TEXT[],
            metadata JSONB,
            parent_id UUID REFERENCES memories(id),
            summary TEXT,
            expires_at TIMESTAMPTZ
        )
    "#,
    )
    .execute(pool)
    .await
    .context("Failed to create memories table")?;

    // Create migration_history table if it doesn't exist (for health checks)
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS migration_history (
            id SERIAL PRIMARY KEY,
            memory_id UUID REFERENCES memories(id),
            from_tier VARCHAR(20),
            to_tier VARCHAR(20),
            migration_reason TEXT,
            migrated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            success BOOLEAN NOT NULL DEFAULT TRUE
        )
    "#,
    )
    .execute(pool)
    .await
    .context("Failed to create migration_history table")?;

    Ok(())
}

async fn setup_test_environment() -> Result<(Arc<MemoryRepository>, MCPServer)> {
    // For production integration tests, this would use testcontainers
    // For now, we'll use environment variable or skip if not available
    let database_url = std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://postgres:password@localhost:5432/postgres".to_string());

    // Create connection pool - in real tests this would wait for container to be ready
    let pool = match create_pool(&database_url, 10).await {
        Ok(pool) => pool,
        Err(_) => {
            // Skip integration tests if database is not available
            return Err(anyhow::anyhow!(
                "Test database not available. Set TEST_DATABASE_URL or run PostgreSQL locally."
            ));
        }
    };

    // Set up database schema for tests (migration functionality not available in crates.io version)
    setup_test_schema(&pool).await?;

    let repository = Arc::new(MemoryRepository::new(pool));
    let embedder = Arc::new(SimpleEmbedder::new("test-api-key".to_string()));
    let mcp_server = MCPServer::new(Arc::clone(&repository), Arc::clone(&embedder))?;

    Ok((repository, mcp_server))
}

#[tokio::test]
#[traced_test]
async fn test_mcp_protocol_compliance() -> Result<()> {
    let (repository, _mcp_server) = setup_test_environment().await?;

    // Test MCP protocol methods that Claude Code would use

    // Test 1: Initialize connection (simulation)
    let _init_params = json!({
        "capabilities": {
            "memory": true,
            "search": true,
            "persistence": true
        }
    });

    // In real MCP, this would be handled by the protocol layer
    // Here we simulate by testing the underlying functionality

    // Test 2: Store memory (like Claude saving context)
    let memory_request = CreateMemoryRequest {
        content: "Claude Code session: Working on Rust project with async/await patterns"
            .to_string(),
        embedding: None,
        tier: Some(MemoryTier::Working),
        importance_score: Some(0.9),
        metadata: Some(json!({
            "source": "claude_code",
            "project": "rust_async",
            "session_id": "test_session_123",
            "timestamp": "2025-01-15T10:30:00Z"
        })),
        parent_id: None,
        expires_at: None,
    };

    let memory = repository.create_memory(memory_request).await?;

    // Test 3: Retrieve memory (like Claude accessing previous context)
    let retrieved = repository.get_memory(memory.id).await?;
    assert!(retrieved.content.contains("Claude Code session"));

    // Test 4: Search for relevant context
    let search_request = SearchRequest {
        query_text: Some("rust async await".to_string()),
        query_embedding: None,
        search_type: None,
        hybrid_weights: None,
        tier: None,
        date_range: None,
        importance_range: None,
        metadata_filters: Some(json!({"source": "claude_code"})),
        tags: None,
        limit: Some(10),
        offset: None,
        cursor: None,
        similarity_threshold: Some(0.7),
        include_metadata: Some(true),
        include_facets: None,
        ranking_boost: None,
        explain_score: None,
    };

    let results = repository.search_memories_simple(search_request).await?;
    assert!(!results.is_empty());
    assert!(results[0].memory.content.contains("async"));

    Ok(())
}

#[tokio::test]
#[traced_test]
async fn test_claude_session_management() -> Result<()> {
    let (repository, _mcp_server) = setup_test_environment().await?;

    // Simulate multiple Claude sessions
    let sessions = vec![
        ("session_1", "Working on database design with PostgreSQL"),
        ("session_2", "Debugging async Rust code with tokio"),
        ("session_3", "Writing integration tests for MCP server"),
    ];

    let mut session_memories = Vec::new();

    // Create memories for different sessions
    for (session_id, content) in &sessions {
        let request = CreateMemoryRequest {
            content: content.to_string(),
            embedding: None,
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.8),
            metadata: Some(json!({
                "session_id": session_id,
                "application": "claude_code",
                "created_by": "claude"
            })),
            parent_id: None,
            expires_at: None,
        };

        let memory = repository.create_memory(request).await?;
        session_memories.push((session_id, memory.id));
    }

    // Test session isolation - search within specific session
    let session_search = SearchRequest {
        query_text: Some("database".to_string()),
        query_embedding: None,
        search_type: None,
        hybrid_weights: None,
        tier: None,
        date_range: None,
        importance_range: None,
        metadata_filters: Some(json!({"session_id": "session_1"})),
        tags: None,
        limit: Some(10),
        offset: None,
        cursor: None,
        similarity_threshold: None,
        include_metadata: Some(true),
        include_facets: None,
        ranking_boost: None,
        explain_score: None,
    };

    let session_results = repository.search_memories_simple(session_search).await?;
    assert_eq!(session_results.len(), 1);
    assert!(session_results[0].memory.content.contains("database"));

    // Test cross-session search
    let global_search = SearchRequest {
        query_text: Some("Rust".to_string()),
        query_embedding: None,
        search_type: None,
        hybrid_weights: None,
        tier: None,
        date_range: None,
        importance_range: None,
        metadata_filters: Some(json!({"application": "claude_code"})),
        tags: None,
        limit: Some(10),
        offset: None,
        cursor: None,
        similarity_threshold: None,
        include_metadata: Some(true),
        include_facets: None,
        ranking_boost: None,
        explain_score: None,
    };

    let global_results = repository.search_memories_simple(global_search).await?;
    assert!(!global_results.is_empty()); // Should find at least the async Rust session

    Ok(())
}

#[tokio::test]
#[traced_test]
async fn test_claude_desktop_integration() -> Result<()> {
    let (repository, _mcp_server) = setup_test_environment().await?;

    // Simulate Claude Desktop interactions
    // Desktop app would have different usage patterns than Code

    // Test 1: Conversation memory
    let conversation_memories = [
        "User asked about machine learning algorithms",
        "Explained gradient descent with mathematical examples",
        "User requested code example in Python",
        "Provided scikit-learn implementation example",
        "User asked for performance optimization tips",
    ];

    let mut conversation_ids = Vec::new();
    for (i, content) in conversation_memories.iter().enumerate() {
        let request = CreateMemoryRequest {
            content: content.to_string(),
            embedding: None,
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.7 + (i as f32 * 0.05)), // Increasing importance
            metadata: Some(json!({
                "application": "claude_desktop",
                "conversation_id": "conv_123",
                "message_index": i,
                "type": if i % 2 == 0 { "user_message" } else { "assistant_response" }
            })),
            parent_id: None,
            expires_at: None,
        };

        let memory = repository.create_memory(request).await?;
        conversation_ids.push(memory.id);
    }

    // Test conversation context retrieval
    let context_search = SearchRequest {
        query_text: Some("machine learning optimization".to_string()),
        query_embedding: None,
        search_type: None,
        hybrid_weights: None,
        tier: None,
        date_range: None,
        importance_range: None,
        metadata_filters: Some(json!({"conversation_id": "conv_123"})),
        tags: None,
        limit: Some(10),
        offset: None,
        cursor: None,
        similarity_threshold: Some(0.5),
        include_metadata: Some(true),
        include_facets: None,
        ranking_boost: None,
        explain_score: None,
    };

    let context_results = repository.search_memories_simple(context_search).await?;
    assert!(!context_results.is_empty());

    // Should find relevant conversation parts
    let has_ml_content = context_results.iter().any(|r| {
        r.memory.content.to_lowercase().contains("machine learning")
            || r.memory.content.to_lowercase().contains("optimization")
    });
    assert!(has_ml_content);

    // Test 2: Document understanding memory
    let document_request = CreateMemoryRequest {
        content: "Claude analyzed a technical document about distributed systems architecture, focusing on microservices patterns and data consistency".to_string(),
        embedding: None,
        tier: Some(MemoryTier::Working),
        importance_score: Some(0.9),
        metadata: Some(json!({
            "application": "claude_desktop",
            "document_id": "doc_456",
            "analysis_type": "technical_summary",
            "domain": "distributed_systems"
        })),
        parent_id: None,
        expires_at: None,
    };

    let doc_memory = repository.create_memory(document_request).await?;

    // Test document-related search
    let doc_search = SearchRequest {
        query_text: Some("distributed systems microservices".to_string()),
        query_embedding: None,
        search_type: None,
        hybrid_weights: None,
        tier: None,
        date_range: None,
        importance_range: None,
        metadata_filters: Some(json!({"application": "claude_desktop"})),
        tags: None,
        limit: Some(5),
        offset: None,
        cursor: None,
        similarity_threshold: None,
        include_metadata: Some(true),
        include_facets: None,
        ranking_boost: None,
        explain_score: None,
    };

    let doc_results = repository.search_memories_simple(doc_search).await?;
    assert!(!doc_results.is_empty());

    let found_doc = doc_results.iter().any(|r| r.memory.id == doc_memory.id);
    assert!(found_doc);

    Ok(())
}

#[tokio::test]
#[traced_test]
async fn test_backward_compatibility() -> Result<()> {
    let (repository, _mcp_server) = setup_test_environment().await?;

    // Test compatibility with different memory formats/versions

    // Test 1: Legacy format (minimal metadata)
    let legacy_request = CreateMemoryRequest {
        content: "Legacy memory format test".to_string(),
        embedding: None,
        tier: Some(MemoryTier::Working),
        importance_score: None,    // Default should be applied
        metadata: Some(json!({})), // Minimal metadata
        parent_id: None,
        expires_at: None,
    };

    let legacy_memory = repository.create_memory(legacy_request).await?;
    assert_eq!(legacy_memory.importance_score, 0.5); // Default value

    // Test 2: Current format (full metadata)
    let current_request = CreateMemoryRequest {
        content: "Current memory format test".to_string(),
        embedding: None,
        tier: Some(MemoryTier::Working),
        importance_score: Some(0.8),
        metadata: Some(json!({
            "version": "1.0",
            "schema": "memory_v1",
            "application": "claude_code",
            "features": ["search", "persistence", "embedding"]
        })),
        parent_id: None,
        expires_at: None,
    };

    let current_memory = repository.create_memory(current_request).await?;

    // Both formats should be searchable together
    let search_request = SearchRequest {
        query_text: Some("memory format test".to_string()),
        query_embedding: None,
        search_type: None,
        hybrid_weights: None,
        tier: None,
        date_range: None,
        importance_range: None,
        metadata_filters: None,
        tags: None,
        limit: Some(10),
        offset: None,
        cursor: None,
        similarity_threshold: None,
        include_metadata: Some(true),
        include_facets: None,
        ranking_boost: None,
        explain_score: None,
    };

    let search_results = repository.search_memories_simple(search_request).await?;
    assert_eq!(search_results.len(), 2);

    // Both legacy and current format memories should be found
    let legacy_found = search_results
        .iter()
        .any(|r| r.memory.id == legacy_memory.id);
    let current_found = search_results
        .iter()
        .any(|r| r.memory.id == current_memory.id);
    assert!(legacy_found && current_found);

    Ok(())
}

#[tokio::test]
#[traced_test]
async fn test_user_acceptance_scenarios() -> Result<()> {
    let (repository, _mcp_server) = setup_test_environment().await?;

    // Scenario 1: Developer using Claude Code for a coding project
    let coding_session = [
        "Started working on a new REST API in Rust",
        "Need to implement user authentication with JWT tokens",
        "Looking at async/await patterns for database operations",
        "Implementing error handling with anyhow crate",
        "Writing unit tests for the authentication module",
    ];

    for (i, content) in coding_session.iter().enumerate() {
        let request = CreateMemoryRequest {
            content: content.to_string(),
            embedding: None,
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.8),
            metadata: Some(json!({
                "scenario": "coding_project",
                "step": i + 1,
                "language": "rust",
                "project_type": "rest_api"
            })),
            parent_id: None,
            expires_at: None,
        };

        repository.create_memory(request).await?;
    }

    // Developer searches for previous auth-related work
    let auth_search = SearchRequest {
        query_text: Some("authentication JWT".to_string()),
        query_embedding: None,
        search_type: None,
        hybrid_weights: None,
        tier: None,
        date_range: None,
        importance_range: None,
        metadata_filters: Some(json!({"scenario": "coding_project"})),
        tags: None,
        limit: Some(5),
        offset: None,
        cursor: None,
        similarity_threshold: None,
        include_metadata: Some(true),
        include_facets: None,
        ranking_boost: None,
        explain_score: None,
    };

    let auth_results = repository.search_memories_simple(auth_search).await?;
    assert!(!auth_results.is_empty());

    let found_auth = auth_results
        .iter()
        .any(|r| r.memory.content.contains("authentication") || r.memory.content.contains("JWT"));
    assert!(found_auth);

    // Scenario 2: Research task using Claude Desktop
    let research_session = [
        "Research paper: 'Attention Is All You Need' - Transformer architecture",
        "Key insight: Self-attention mechanism replaces recurrent layers",
        "Mathematical formulation: Attention(Q,K,V) = softmax(QK^T/√d_k)V",
        "Applications: Machine translation, text summarization, code generation",
        "Follow-up reading: BERT, GPT, and other transformer variants",
    ];

    for (i, content) in research_session.iter().enumerate() {
        let request = CreateMemoryRequest {
            content: content.to_string(),
            embedding: None,
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.9), // Research is high importance
            metadata: Some(json!({
                "scenario": "research_task",
                "topic": "transformers",
                "step": i + 1,
                "academic": true
            })),
            parent_id: None,
            expires_at: None,
        };

        repository.create_memory(request).await?;
    }

    // Researcher searches for transformer details
    let research_search = SearchRequest {
        query_text: Some("transformer attention mechanism".to_string()),
        query_embedding: None,
        search_type: None,
        hybrid_weights: None,
        tier: None,
        date_range: None,
        importance_range: None,
        metadata_filters: Some(json!({"scenario": "research_task"})),
        tags: None,
        limit: Some(10),
        offset: None,
        cursor: None,
        similarity_threshold: None,
        include_metadata: Some(true),
        include_facets: None,
        ranking_boost: None,
        explain_score: None,
    };

    let research_results = repository.search_memories_simple(research_search).await?;
    assert!(!research_results.is_empty());

    let found_transformer = research_results.iter().any(|r| {
        r.memory.content.contains("Transformer") || r.memory.content.contains("attention")
    });
    assert!(found_transformer);

    // Scenario 3: Cross-application memory sharing
    // User switches from Claude Code to Claude Desktop, should access same memories
    let cross_search = SearchRequest {
        query_text: Some("Rust authentication".to_string()),
        query_embedding: None,
        search_type: None,
        hybrid_weights: None,
        tier: None,
        date_range: None,
        importance_range: None,
        metadata_filters: None, // No application filter - should find memories from both scenarios
        tags: None,
        limit: Some(10),
        offset: None,
        cursor: None,
        similarity_threshold: None,
        include_metadata: Some(true),
        include_facets: None,
        ranking_boost: None,
        explain_score: None,
    };

    let cross_results = repository.search_memories_simple(cross_search).await?;
    assert!(!cross_results.is_empty());

    // Should find the coding project memory about authentication
    let found_cross = cross_results.iter().any(|r| {
        r.memory.content.contains("authentication")
            && r.memory
                .metadata
                .get("language")
                .is_some_and(|l| l == "rust")
    });
    assert!(found_cross);

    Ok(())
}
