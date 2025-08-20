//! End-to-End MCP (Model Context Protocol) Integration Tests
//!
//! These tests validate the MCP server functionality including:
//! - MCP protocol compliance
//! - Memory operations via MCP interface
//! - Error handling and protocol-specific responses
//! - Integration with Claude Code/Desktop workflows

mod test_helpers;

use anyhow::Result;
use codex_memory::{
    mcp::MCPServer,
    memory::models::{CreateMemoryRequest, MemoryTier, SearchRequest},
};
use serde_json::{json, Value};
use std::sync::Arc;
use test_helpers::{TestConfigBuilder, TestEnvironment};
use tokio::time::{timeout, Duration};
use tracing_test::traced_test;

/// Test MCP server initialization and basic functionality
#[tokio::test]
#[traced_test]
async fn test_mcp_server_initialization() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Test 1: Create MCP server instance
    let mcp_server = MCPServer::new(Arc::clone(&env.repository), Arc::clone(&env.embedder))?;

    // Verify server is properly initialized
    assert!(
        mcp_server.is_ready(),
        "MCP server should be ready after initialization"
    );

    // Test 2: Test server capabilities
    let capabilities = mcp_server.get_capabilities().await?;
    assert!(
        capabilities.contains_key("tools"),
        "Server should expose tool capabilities"
    );
    assert!(
        capabilities.contains_key("resources"),
        "Server should expose resource capabilities"
    );

    // Test 3: Verify tool definitions
    let tools = capabilities["tools"].as_array().unwrap();
    let tool_names: Vec<String> = tools
        .iter()
        .filter_map(|t| t["name"].as_str())
        .map(|s| s.to_string())
        .collect();

    assert!(tool_names.contains(&"create_memory".to_string()));
    assert!(tool_names.contains(&"search_memories".to_string()));
    assert!(tool_names.contains(&"get_memory".to_string()));
    assert!(tool_names.contains(&"update_memory".to_string()));
    assert!(tool_names.contains(&"delete_memory".to_string()));

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test MCP memory creation tool
#[tokio::test]
#[traced_test]
async fn test_mcp_create_memory_tool() -> Result<()> {
    let env = TestEnvironment::new().await?;
    let mcp_server = MCPServer::new(Arc::clone(&env.repository), Arc::clone(&env.embedder))?;

    // Test 1: Create memory via MCP tool
    let create_request = json!({
        "name": "create_memory",
        "arguments": {
            "content": "Test memory created via MCP tool",
            "tier": "working",
            "importance_score": 0.8,
            "metadata": env.get_test_metadata(Some(json!({"source": "mcp_tool"})))
        }
    });

    let create_response = mcp_server.execute_tool(create_request).await?;
    assert!(create_response["success"].as_bool().unwrap_or(false));

    let memory_data = &create_response["result"];
    assert!(memory_data["id"].is_string());
    assert_eq!(memory_data["content"], "Test memory created via MCP tool");
    assert_eq!(memory_data["tier"], "working");
    assert_eq!(memory_data["importance_score"], 0.8);

    // Test 2: Create memory with minimal parameters
    let minimal_request = json!({
        "name": "create_memory",
        "arguments": {
            "content": "Minimal memory via MCP"
        }
    });

    let minimal_response = mcp_server.execute_tool(minimal_request).await?;
    assert!(minimal_response["success"].as_bool().unwrap_or(false));

    // Should use default values
    let minimal_data = &minimal_response["result"];
    assert_eq!(minimal_data["content"], "Minimal memory via MCP");
    assert!(minimal_data["tier"].is_string()); // Should have default tier

    // Test 3: Invalid creation request
    let invalid_request = json!({
        "name": "create_memory",
        "arguments": {
            // Missing required content field
            "tier": "working"
        }
    });

    let invalid_response = mcp_server.execute_tool(invalid_request).await?;
    assert!(!invalid_response["success"].as_bool().unwrap_or(true));
    assert!(invalid_response["error"].is_string());

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test MCP memory search tool
#[tokio::test]
#[traced_test]
async fn test_mcp_search_memories_tool() -> Result<()> {
    let env = TestEnvironment::new().await?;
    let mcp_server = MCPServer::new(Arc::clone(&env.repository), Arc::clone(&env.embedder))?;

    // Setup: Create test memories
    let test_memories = vec![
        "How to implement authentication in Rust",
        "Database migration best practices",
        "API rate limiting strategies",
        "Error handling patterns in async code",
    ];

    for content in &test_memories {
        let request = CreateMemoryRequest {
            content: content.to_string(),
            embedding: None,
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.7),
            metadata: Some(env.get_test_metadata(Some(json!({"mcp_search_test": true})))),
            parent_id: None,
            expires_at: None,
        };
        env.repository.create_memory(request).await?;
    }

    env.wait_for_consistency().await;

    // Test 1: Basic text search via MCP
    let search_request = json!({
        "name": "search_memories",
        "arguments": {
            "query_text": "Rust programming",
            "limit": 5
        }
    });

    let search_response = mcp_server.execute_tool(search_request).await?;
    assert!(search_response["success"].as_bool().unwrap_or(false));

    let results = search_response["result"]["results"].as_array().unwrap();
    assert!(!results.is_empty());

    // Should find the Rust-related memory
    let has_rust_content = results.iter().any(|r| {
        r["memory"]["content"]
            .as_str()
            .unwrap_or("")
            .contains("Rust")
    });
    assert!(has_rust_content);

    // Test 2: Search with filters via MCP
    let filtered_search = json!({
        "name": "search_memories",
        "arguments": {
            "query_text": "database",
            "tier": "working",
            "importance_range": {
                "min": 0.5
            },
            "limit": 10
        }
    });

    let filtered_response = mcp_server.execute_tool(filtered_search).await?;
    assert!(filtered_response["success"].as_bool().unwrap_or(false));

    let filtered_results = filtered_response["result"]["results"].as_array().unwrap();
    // Verify filtering worked
    for result in filtered_results {
        let memory = &result["memory"];
        assert_eq!(memory["tier"], "working");
        assert!(memory["importance_score"].as_f64().unwrap_or(0.0) >= 0.5);
    }

    // Test 3: Empty search
    let empty_search = json!({
        "name": "search_memories",
        "arguments": {
            "query_text": "nonexistent query that should not match anything",
            "limit": 5
        }
    });

    let empty_response = mcp_server.execute_tool(empty_search).await?;
    assert!(empty_response["success"].as_bool().unwrap_or(false));

    let empty_results = empty_response["result"]["results"].as_array().unwrap();
    // May be empty or have very low similarity scores
    assert!(empty_results.len() <= 5);

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test MCP memory retrieval and update tools
#[tokio::test]
#[traced_test]
async fn test_mcp_memory_crud_tools() -> Result<()> {
    let env = TestEnvironment::new().await?;
    let mcp_server = MCPServer::new(Arc::clone(&env.repository), Arc::clone(&env.embedder))?;

    // Setup: Create a test memory
    let create_request = json!({
        "name": "create_memory",
        "arguments": {
            "content": "Original content for CRUD testing",
            "tier": "working",
            "importance_score": 0.6,
            "metadata": env.get_test_metadata(Some(json!({"crud_test": true})))
        }
    });

    let create_response = mcp_server.execute_tool(create_request).await?;
    let memory_id = create_response["result"]["id"].as_str().unwrap();

    // Test 1: Get memory via MCP tool
    let get_request = json!({
        "name": "get_memory",
        "arguments": {
            "id": memory_id
        }
    });

    let get_response = mcp_server.execute_tool(get_request).await?;
    assert!(get_response["success"].as_bool().unwrap_or(false));

    let retrieved_memory = &get_response["result"];
    assert_eq!(retrieved_memory["id"], memory_id);
    assert_eq!(
        retrieved_memory["content"],
        "Original content for CRUD testing"
    );
    assert_eq!(retrieved_memory["tier"], "working");

    // Test 2: Update memory via MCP tool
    let update_request = json!({
        "name": "update_memory",
        "arguments": {
            "id": memory_id,
            "content": "Updated content via MCP tool",
            "tier": "warm",
            "importance_score": 0.9
        }
    });

    let update_response = mcp_server.execute_tool(update_request).await?;
    assert!(update_response["success"].as_bool().unwrap_or(false));

    let updated_memory = &update_response["result"];
    assert_eq!(updated_memory["content"], "Updated content via MCP tool");
    assert_eq!(updated_memory["tier"], "warm");
    assert_eq!(updated_memory["importance_score"], 0.9);

    // Test 3: Verify update persisted
    let verify_get = json!({
        "name": "get_memory",
        "arguments": {
            "id": memory_id
        }
    });

    let verify_response = mcp_server.execute_tool(verify_get).await?;
    let verified_memory = &verify_response["result"];
    assert_eq!(verified_memory["content"], "Updated content via MCP tool");

    // Test 4: Delete memory via MCP tool
    let delete_request = json!({
        "name": "delete_memory",
        "arguments": {
            "id": memory_id
        }
    });

    let delete_response = mcp_server.execute_tool(delete_request).await?;
    assert!(delete_response["success"].as_bool().unwrap_or(false));

    // Test 5: Verify deletion
    let get_deleted = json!({
        "name": "get_memory",
        "arguments": {
            "id": memory_id
        }
    });

    let deleted_response = mcp_server.execute_tool(get_deleted).await?;
    assert!(!deleted_response["success"].as_bool().unwrap_or(true));
    assert!(deleted_response["error"]
        .as_str()
        .unwrap()
        .contains("not found"));

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test MCP error handling and protocol compliance
#[tokio::test]
#[traced_test]
async fn test_mcp_error_handling() -> Result<()> {
    let env = TestEnvironment::new().await?;
    let mcp_server = MCPServer::new(Arc::clone(&env.repository), Arc::clone(&env.embedder))?;

    // Test 1: Invalid tool name
    let invalid_tool = json!({
        "name": "nonexistent_tool",
        "arguments": {}
    });

    let invalid_response = mcp_server.execute_tool(invalid_tool).await?;
    assert!(!invalid_response["success"].as_bool().unwrap_or(true));
    assert!(invalid_response["error"]
        .as_str()
        .unwrap()
        .contains("Unknown tool"));

    // Test 2: Missing required arguments
    let missing_args = json!({
        "name": "create_memory",
        "arguments": {} // Missing required 'content' argument
    });

    let missing_response = mcp_server.execute_tool(missing_args).await?;
    assert!(!missing_response["success"].as_bool().unwrap_or(true));
    assert!(missing_response["error"].is_string());

    // Test 3: Invalid argument types
    let invalid_types = json!({
        "name": "create_memory",
        "arguments": {
            "content": "Test content",
            "importance_score": "not_a_number" // Should be a float
        }
    });

    let types_response = mcp_server.execute_tool(invalid_types).await?;
    assert!(!types_response["success"].as_bool().unwrap_or(true));

    // Test 4: Invalid UUID format
    let invalid_uuid = json!({
        "name": "get_memory",
        "arguments": {
            "id": "not-a-valid-uuid"
        }
    });

    let uuid_response = mcp_server.execute_tool(invalid_uuid).await?;
    assert!(!uuid_response["success"].as_bool().unwrap_or(true));

    // Test 5: Very large content (stress test)
    let large_content = "x".repeat(1_000_000); // 1MB content
    let large_request = json!({
        "name": "create_memory",
        "arguments": {
            "content": large_content
        }
    });

    let large_response = timeout(
        Duration::from_secs(30),
        mcp_server.execute_tool(large_request),
    )
    .await;

    match large_response {
        Ok(response) => {
            // Either succeeds or fails gracefully
            if response?["success"].as_bool().unwrap_or(false) {
                println!("Large content accepted by MCP server");
            } else {
                println!(
                    "Large content rejected (acceptable): {}",
                    response["error"].as_str().unwrap_or("Unknown error")
                );
            }
        }
        Err(_) => {
            // Timeout is also acceptable for very large content
            println!("Large content request timed out (acceptable)");
        }
    }

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test MCP resource management and discovery
#[tokio::test]
#[traced_test]
async fn test_mcp_resource_management() -> Result<()> {
    let env = TestEnvironment::new().await?;
    let mcp_server = MCPServer::new(Arc::clone(&env.repository), Arc::clone(&env.embedder))?;

    // Test 1: List available resources
    let resources = mcp_server.list_resources().await?;
    assert!(resources.is_array());

    // Should have memory-related resources
    let resource_names: Vec<String> = resources
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| r["name"].as_str())
        .map(|s| s.to_string())
        .collect();

    assert!(resource_names.contains(&"memory_statistics".to_string()));
    assert!(resource_names.contains(&"memory_schema".to_string()));

    // Test 2: Get memory statistics resource
    let stats_resource = mcp_server.get_resource("memory_statistics").await?;
    assert!(stats_resource["success"].as_bool().unwrap_or(false));

    let stats_data = &stats_resource["result"];
    assert!(stats_data["total_memories"].is_number());

    // Test 3: Get memory schema resource
    let schema_resource = mcp_server.get_resource("memory_schema").await?;
    assert!(schema_resource["success"].as_bool().unwrap_or(false));

    let schema_data = &schema_resource["result"];
    assert!(schema_data["memory_fields"].is_array());

    // Test 4: Invalid resource request
    let invalid_resource = mcp_server.get_resource("nonexistent_resource").await?;
    assert!(!invalid_resource["success"].as_bool().unwrap_or(true));

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test MCP server with concurrent connections (simulated)
#[tokio::test]
#[traced_test]
async fn test_mcp_concurrent_operations() -> Result<()> {
    let env = TestEnvironment::new().await?;
    let mcp_server = Arc::new(MCPServer::new(
        Arc::clone(&env.repository),
        Arc::clone(&env.embedder),
    )?);

    // Test concurrent tool executions
    let mut handles = Vec::new();

    for i in 0..10 {
        let server = Arc::clone(&mcp_server);
        let test_id = env.test_id.clone();

        let handle = tokio::spawn(async move {
            // Each "connection" creates a memory
            let create_request = json!({
                "name": "create_memory",
                "arguments": {
                    "content": format!("Concurrent memory {}", i),
                    "metadata": {
                        "test_id": test_id,
                        "concurrent": true,
                        "worker": i
                    }
                }
            });

            let create_response = server.execute_tool(create_request).await?;
            assert!(create_response["success"].as_bool().unwrap_or(false));

            let memory_id = create_response["result"]["id"].as_str().unwrap();

            // Then searches for it
            let search_request = json!({
                "name": "search_memories",
                "arguments": {
                    "query_text": format!("Concurrent memory {}", i),
                    "limit": 5
                }
            });

            let search_response = server.execute_tool(search_request).await?;
            assert!(search_response["success"].as_bool().unwrap_or(false));

            Ok::<String, anyhow::Error>(memory_id.to_string())
        });

        handles.push(handle);
    }

    // Wait for all concurrent operations
    let mut memory_ids = Vec::new();
    for handle in handles {
        let memory_id = handle.await??;
        memory_ids.push(memory_id);
    }

    assert_eq!(memory_ids.len(), 10);

    // Verify all memories were created successfully
    let server = Arc::clone(&mcp_server);
    for memory_id in memory_ids {
        let get_request = json!({
            "name": "get_memory",
            "arguments": {
                "id": memory_id
            }
        });

        let get_response = server.execute_tool(get_request).await?;
        assert!(get_response["success"].as_bool().unwrap_or(false));
    }

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test MCP integration with Claude Code workflow simulation
#[tokio::test]
#[traced_test]
async fn test_claude_code_workflow_simulation() -> Result<()> {
    let env = TestEnvironment::new().await?;
    let mcp_server = MCPServer::new(Arc::clone(&env.repository), Arc::clone(&env.embedder))?;

    // Simulate a typical Claude Code session workflow

    // Step 1: User asks a question about their codebase
    let question_memory = json!({
        "name": "create_memory",
        "arguments": {
            "content": "User asked: How do I implement proper error handling in my Rust API?",
            "tier": "working",
            "importance_score": 0.8,
            "metadata": env.get_test_metadata(Some(json!({
                "type": "user_question",
                "source": "claude_code",
                "session_id": "test_session_001"
            })))
        }
    });

    let question_response = mcp_server.execute_tool(question_memory).await?;
    assert!(question_response["success"].as_bool().unwrap_or(false));

    // Step 2: Claude searches for relevant existing knowledge
    let search_request = json!({
        "name": "search_memories",
        "arguments": {
            "query_text": "Rust error handling patterns best practices",
            "limit": 10,
            "include_metadata": true
        }
    });

    let search_response = mcp_server.execute_tool(search_request).await?;
    assert!(search_response["success"].as_bool().unwrap_or(false));

    // Step 3: Claude stores code context from the user's project
    let code_context = json!({
        "name": "create_memory",
        "arguments": {
            "content": "User's current code: fn process_request() -> Result<Response, ApiError> { /* implementation */ }",
            "tier": "working",
            "importance_score": 0.9,
            "metadata": env.get_test_metadata(Some(json!({
                "type": "code_context",
                "language": "rust",
                "file_path": "/src/api/mod.rs",
                "session_id": "test_session_001"
            })))
        }
    });

    let code_response = mcp_server.execute_tool(code_context).await?;
    assert!(code_response["success"].as_bool().unwrap_or(false));

    // Step 4: Claude provides an answer and stores it
    let answer_memory = json!({
        "name": "create_memory",
        "arguments": {
            "content": "Claude's response: For Rust error handling, use Result<T, E> with custom error types implementing std::error::Error. Consider using thiserror crate for ergonomic error definitions.",
            "tier": "working",
            "importance_score": 0.85,
            "metadata": env.get_test_metadata(Some(json!({
                "type": "claude_response",
                "topic": "error_handling",
                "language": "rust",
                "session_id": "test_session_001"
            })))
        }
    });

    let answer_response = mcp_server.execute_tool(answer_memory).await?;
    assert!(answer_response["success"].as_bool().unwrap_or(false));

    // Step 5: User provides feedback
    let feedback_memory = json!({
        "name": "create_memory",
        "arguments": {
            "content": "User feedback: This helped! I implemented the error handling pattern and it works great.",
            "tier": "warm",
            "importance_score": 0.7,
            "metadata": env.get_test_metadata(Some(json!({
                "type": "user_feedback",
                "sentiment": "positive",
                "session_id": "test_session_001"
            })))
        }
    });

    let feedback_response = mcp_server.execute_tool(feedback_memory).await?;
    assert!(feedback_response["success"].as_bool().unwrap_or(false));

    // Step 6: Verify we can reconstruct the conversation
    let conversation_search = json!({
        "name": "search_memories",
        "arguments": {
            "query_text": "session_id test_session_001",
            "limit": 20,
            "include_metadata": true
        }
    });

    let conversation_response = mcp_server.execute_tool(conversation_search).await?;
    let conversation_results = conversation_response["result"]["results"]
        .as_array()
        .unwrap();

    assert!(
        conversation_results.len() >= 4,
        "Should find all session memories"
    );

    // Verify we have all the expected memory types
    let memory_types: Vec<String> = conversation_results
        .iter()
        .filter_map(|r| r["memory"]["metadata"]["type"].as_str())
        .map(|s| s.to_string())
        .collect();

    assert!(memory_types.contains(&"user_question".to_string()));
    assert!(memory_types.contains(&"code_context".to_string()));
    assert!(memory_types.contains(&"claude_response".to_string()));
    assert!(memory_types.contains(&"user_feedback".to_string()));

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test MCP server performance and response times
#[tokio::test]
#[traced_test]
async fn test_mcp_performance_characteristics() -> Result<()> {
    let env = TestEnvironment::new().await?;
    let mcp_server = MCPServer::new(Arc::clone(&env.repository), Arc::clone(&env.embedder))?;

    // Test 1: Tool execution timing
    let start = std::time::Instant::now();

    let create_request = json!({
        "name": "create_memory",
        "arguments": {
            "content": "Performance test memory with moderate content length to simulate realistic usage patterns",
            "tier": "working",
            "importance_score": 0.7,
            "metadata": env.get_test_metadata(Some(json!({"performance_test": true})))
        }
    });

    let create_response = mcp_server.execute_tool(create_request).await?;
    let create_duration = start.elapsed();

    assert!(create_response["success"].as_bool().unwrap_or(false));
    assert!(
        create_duration < Duration::from_secs(5),
        "Memory creation should be fast"
    );

    println!("MCP memory creation took: {:?}", create_duration);

    // Test 2: Search performance
    let search_start = std::time::Instant::now();

    let search_request = json!({
        "name": "search_memories",
        "arguments": {
            "query_text": "Performance test realistic usage patterns",
            "limit": 10
        }
    });

    let search_response = mcp_server.execute_tool(search_request).await?;
    let search_duration = search_start.elapsed();

    assert!(search_response["success"].as_bool().unwrap_or(false));
    assert!(
        search_duration < Duration::from_secs(3),
        "Search should be fast"
    );

    println!("MCP search took: {:?}", search_duration);

    // Test 3: Batch operations timing
    let batch_start = std::time::Instant::now();
    let mut batch_operations = Vec::new();

    for i in 0..10 {
        let request = json!({
            "name": "create_memory",
            "arguments": {
                "content": format!("Batch memory {}", i),
                "metadata": env.get_test_metadata(Some(json!({"batch_test": true, "index": i})))
            }
        });

        batch_operations.push(mcp_server.execute_tool(request));
    }

    let batch_results = futures::future::join_all(batch_operations).await;
    let batch_duration = batch_start.elapsed();

    // All should succeed
    for result in batch_results {
        assert!(result?["success"].as_bool().unwrap_or(false));
    }

    assert!(
        batch_duration < Duration::from_secs(15),
        "Batch operations should complete reasonably fast"
    );
    println!("MCP batch operations took: {:?}", batch_duration);

    let ops_per_second = 10.0 / batch_duration.as_secs_f64();
    println!("MCP throughput: {:.1} ops/sec", ops_per_second);

    env.cleanup_test_data().await?;
    Ok(())
}
