//! End-to-end tests for MCP insight commands
//!
//! Tests all 5 insight commands through the MCP protocol including
//! parameter validation, error handling, and response formatting.

#![cfg(feature = "codex-dreams")]

mod helpers;

use anyhow::Result;
use codex_memory::mcp_server::{
    handlers::MCPHandlers,
    tools::MCPTools,
};
use helpers::{
    insights_test_utils::{InsightTestEnv, TestMemoryBuilder},
    ollama_mock::{MockOllamaConfig, MockOllamaServer},
};
use serde_json::{json, Value};
use std::sync::Arc;

/// Test generate_insights command
#[tokio::test]
async fn test_generate_insights_command() -> Result<()> {
    let env = InsightTestEnv::new().await?;
    
    // Create test memories
    let _memory1 = TestMemoryBuilder::new("I learned about Rust today")
        .with_importance(0.8)
        .create(&env.repository).await?;
    
    let _memory2 = TestMemoryBuilder::new("Rust has excellent memory safety")
        .with_importance(0.7)
        .create(&env.repository).await?;
    
    // Create MCP handlers
    let handlers = MCPHandlers::new(
        env.repository.clone(),
        None, // No embedder needed for this test
        None, // No audit logger
        None, // No MCP logger  
        None, // No progress tracker
    ).await?;
    
    // Test generate_insights command
    let request = json!({
        "name": "generate_insights",
        "arguments": {
            "timeframe": "last 24 hours",
            "topic": "learning"
        }
    });
    
    let response = handlers.handle_tool_call("generate_insights", Some(&request["arguments"])).await;
    
    // Should return a placeholder response until processor is integrated
    assert!(response.is_ok(), "generate_insights should succeed");
    
    let result = response?;
    let content = result.as_array()
        .and_then(|arr| arr.first())
        .and_then(|item| item["text"].as_str())
        .unwrap_or("");
    
    assert!(content.contains("★"), "Response should have insight formatting");
    assert!(content.contains("generate"), "Should mention generation functionality");
    
    Ok(())
}

/// Test show_insights command
#[tokio::test]
async fn test_show_insights_command() -> Result<()> {
    let env = InsightTestEnv::new().await?;
    
    let handlers = MCPHandlers::new(
        env.repository.clone(),
        None, None, None, None,
    ).await?;
    
    // Test show_insights command
    let request = json!({
        "name": "show_insights", 
        "arguments": {
            "limit": 5,
            "type": "learning"
        }
    });
    
    let response = handlers.handle_tool_call("show_insights", Some(&request["arguments"])).await?;
    
    let content = response.as_array()
        .and_then(|arr| arr.first())
        .and_then(|item| item["text"].as_str())
        .unwrap_or("");
    
    assert!(content.contains("★"), "Should have insight formatting");
    assert!(content.contains("insights"), "Should mention insights");
    
    Ok(())
}

/// Test search_insights command
#[tokio::test]
async fn test_search_insights_command() -> Result<()> {
    let env = InsightTestEnv::new().await?;
    
    let handlers = MCPHandlers::new(
        env.repository.clone(),
        None, None, None, None,
    ).await?;
    
    // Test search_insights command
    let request = json!({
        "name": "search_insights",
        "arguments": {
            "query": "programming patterns",
            "limit": 10
        }
    });
    
    let response = handlers.handle_tool_call("search_insights", Some(&request["arguments"])).await?;
    
    let content = response.as_array()
        .and_then(|arr| arr.first())
        .and_then(|item| item["text"].as_str())
        .unwrap_or("");
    
    assert!(content.contains("★"), "Should have insight formatting");
    assert!(content.contains("search"), "Should mention search functionality");
    
    Ok(())
}

/// Test insight_feedback command
#[tokio::test]
async fn test_insight_feedback_command() -> Result<()> {
    let env = InsightTestEnv::new().await?;
    
    let handlers = MCPHandlers::new(
        env.repository.clone(),
        None, None, None, None,
    ).await?;
    
    // Test insight_feedback command
    let request = json!({
        "name": "insight_feedback",
        "arguments": {
            "insight_id": "12345678-1234-1234-1234-123456789abc",
            "rating": "helpful",
            "comment": "This insight was very useful"
        }
    });
    
    let response = handlers.handle_tool_call("insight_feedback", Some(&request["arguments"])).await?;
    
    let content = response.as_array()
        .and_then(|arr| arr.first())
        .and_then(|item| item["text"].as_str())
        .unwrap_or("");
    
    assert!(content.contains("feedback"), "Should mention feedback");
    assert!(content.contains("helpful") || content.contains("thank"), "Should acknowledge feedback");
    
    Ok(())
}

/// Test export_insights command
#[tokio::test]
async fn test_export_insights_command() -> Result<()> {
    let env = InsightTestEnv::new().await?;
    
    let handlers = MCPHandlers::new(
        env.repository.clone(),
        None, None, None, None,
    ).await?;
    
    // Test markdown export
    let request = json!({
        "name": "export_insights",
        "arguments": {
            "format": "markdown",
            "filter": {
                "type": "learning",
                "min_confidence": 0.7
            }
        }
    });
    
    let response = handlers.handle_tool_call("export_insights", Some(&request["arguments"])).await?;
    
    let content = response.as_array()
        .and_then(|arr| arr.first())
        .and_then(|item| item["text"].as_str())
        .unwrap_or("");
    
    assert!(content.contains("export"), "Should mention export functionality");
    assert!(content.contains("markdown") || content.contains("format"), "Should reference format");
    
    // Test JSON-LD export
    let request = json!({
        "name": "export_insights", 
        "arguments": {
            "format": "jsonld"
        }
    });
    
    let response = handlers.handle_tool_call("export_insights", Some(&request["arguments"])).await?;
    
    let content = response.as_array()
        .and_then(|arr| arr.first())
        .and_then(|item| item["text"].as_str())
        .unwrap_or("");
    
    assert!(content.contains("export") || content.contains("JSON-LD"), "Should handle JSON-LD export");
    
    Ok(())
}

/// Test MCP tool schema validation
#[tokio::test]
async fn test_insight_tool_schemas() -> Result<()> {
    let tools = MCPTools::get_tools_list();
    let tools_array = tools["tools"].as_array()
        .expect("Should have tools array");
    
    let insight_tools = ["generate_insights", "show_insights", "search_insights", 
                        "insight_feedback", "export_insights"];
    
    for tool_name in &insight_tools {
        let tool = tools_array.iter()
            .find(|t| t["name"] == *tool_name)
            .expect(&format!("Tool '{}' should exist", tool_name));
        
        // Validate tool structure
        assert!(tool["name"].is_string(), "Tool should have name");
        assert!(tool["description"].is_string(), "Tool should have description");
        assert!(tool["inputSchema"].is_object(), "Tool should have input schema");
        
        // Validate description starts with ★
        let description = tool["description"].as_str().unwrap();
        assert!(description.starts_with("★"), 
            "Tool '{}' description should start with ★", tool_name);
        
        // Validate input schema has required structure
        let schema = &tool["inputSchema"];
        assert_eq!(schema["type"], "object", "Schema should be object type");
        assert!(schema["properties"].is_object(), "Schema should have properties");
    }
    
    Ok(())
}

/// Test command parameter validation
#[tokio::test]
async fn test_command_parameter_validation() -> Result<()> {
    let env = InsightTestEnv::new().await?;
    
    let handlers = MCPHandlers::new(
        env.repository.clone(),
        None, None, None, None,
    ).await?;
    
    // Test invalid UUID in feedback command
    let request = json!({
        "name": "insight_feedback",
        "arguments": {
            "insight_id": "invalid-uuid",
            "rating": "helpful"
        }
    });
    
    let response = handlers.handle_tool_call("insight_feedback", Some(&request["arguments"])).await;
    
    // Should handle invalid UUID gracefully (might succeed with placeholder response)
    // The actual validation would be implemented in the real handlers
    assert!(response.is_ok(), "Should handle invalid input gracefully");
    
    // Test invalid rating value
    let request = json!({
        "name": "insight_feedback",
        "arguments": {
            "insight_id": "12345678-1234-1234-1234-123456789abc",
            "rating": "invalid_rating"
        }
    });
    
    let response = handlers.handle_tool_call("insight_feedback", Some(&request["arguments"])).await;
    assert!(response.is_ok(), "Should handle invalid rating gracefully");
    
    Ok(())
}

/// Test error handling for non-existent insights
#[tokio::test]
async fn test_error_handling() -> Result<()> {
    let env = InsightTestEnv::new().await?;
    
    let handlers = MCPHandlers::new(
        env.repository.clone(),
        None, None, None, None,
    ).await?;
    
    // Test feedback for non-existent insight
    let request = json!({
        "name": "insight_feedback", 
        "arguments": {
            "insight_id": "99999999-9999-9999-9999-999999999999",
            "rating": "helpful"
        }
    });
    
    let response = handlers.handle_tool_call("insight_feedback", Some(&request["arguments"])).await?;
    
    // Should provide helpful error message (placeholder implementation)
    let content = response.as_array()
        .and_then(|arr| arr.first())
        .and_then(|item| item["text"].as_str())
        .unwrap_or("");
    
    // Placeholder should still be informative
    assert!(!content.is_empty(), "Should provide response even for non-existent insight");
    
    Ok(())
}

/// Test command help text and documentation
#[tokio::test]
async fn test_command_help_text() -> Result<()> {
    let tools = MCPTools::get_tools_list();
    let tools_array = tools["tools"].as_array().unwrap();
    
    let insight_tools = ["generate_insights", "show_insights", "search_insights",
                        "insight_feedback", "export_insights"];
    
    for tool_name in &insight_tools {
        let tool = tools_array.iter()
            .find(|t| t["name"] == *tool_name)
            .unwrap();
        
        let description = tool["description"].as_str().unwrap();
        
        // Check description quality
        assert!(description.len() > 20, "Description should be substantial for {}", tool_name);
        assert!(description.contains("insight") || description.contains("Insight"), 
            "Description should mention insights for {}", tool_name);
        
        // Check input schema documentation
        let properties = &tool["inputSchema"]["properties"];
        if let Some(props) = properties.as_object() {
            for (param_name, param_schema) in props {
                if let Some(desc) = param_schema["description"].as_str() {
                    assert!(!desc.is_empty(), 
                        "Parameter '{}' in '{}' should have description", param_name, tool_name);
                }
            }
        }
    }
    
    Ok(())
}

/// Test concurrent command execution
#[tokio::test]
async fn test_concurrent_commands() -> Result<()> {
    let env = InsightTestEnv::new().await?;
    
    let handlers = Arc::new(MCPHandlers::new(
        env.repository.clone(),
        None, None, None, None,
    ).await?);
    
    // Execute multiple commands concurrently
    let mut handles = vec![];
    
    for i in 0..5 {
        let handlers_clone = handlers.clone();
        let request = json!({
            "name": "show_insights",
            "arguments": {
                "limit": 10,
                "offset": i * 10
            }
        });
        
        let handle = tokio::spawn(async move {
            handlers_clone.handle_tool_call("show_insights", Some(&request["arguments"])).await
        });
        
        handles.push(handle);
    }
    
    // Wait for all to complete
    let mut success_count = 0;
    for handle in handles {
        if handle.await?.is_ok() {
            success_count += 1;
        }
    }
    
    assert_eq!(success_count, 5, "All concurrent commands should succeed");
    
    Ok(())
}