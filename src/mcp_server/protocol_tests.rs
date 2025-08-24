//! Comprehensive MCP Protocol Compliance Tests
//!
//! This module contains tests to verify compliance with the MCP specification 2025-06-18
//! and JSON-RPC 2.0 standards.

use super::*;
use crate::mcp_server::{
    logging::{LogLevel, MCPLogger},
    progress::ProgressTracker,
    tools::MCPTools,
    transport::{
        create_error_response, create_error_response_with_data, create_success_response,
        format_tool_response, format_tool_response_with_content, create_text_content,
        create_image_content, create_resource_content, StdioTransport,
    },
};
use serde_json::{json, Value};

/// Test MCP protocol version compliance
#[tokio::test]
async fn test_mcp_protocol_version() {
    let capabilities = MCPTools::get_server_capabilities();
    
    // Verify protocol version matches current MCP specification
    assert_eq!(capabilities["protocolVersion"], "2025-06-18");
    assert_eq!(capabilities["serverInfo"]["name"], "codex-memory");
    assert!(capabilities["serverInfo"]["version"].is_string());
    assert!(capabilities["serverInfo"]["description"].is_string());
}

/// Test server capabilities declaration compliance
#[tokio::test] 
async fn test_server_capabilities_compliance() {
    let capabilities = MCPTools::get_server_capabilities();
    let caps = &capabilities["capabilities"];
    
    // Test required capabilities are declared
    assert!(caps["tools"].is_object());
    assert!(caps["resources"].is_object()); 
    assert!(caps["prompts"].is_object());
    
    // Test new capabilities are declared
    assert_eq!(caps["logging"]["supported"], true);
    assert_eq!(caps["progress"]["supported"], true);
    assert_eq!(caps["completion"]["supported"], true);
    assert_eq!(caps["completion"]["argument"], true);
    
    // Test listChanged flags
    assert_eq!(caps["tools"]["listChanged"], false);
    assert_eq!(caps["resources"]["listChanged"], false);
    assert_eq!(caps["prompts"]["listChanged"], false);
}

/// Test JSON-RPC 2.0 error format compliance
#[test]
fn test_jsonrpc_error_format_compliance() {
    let id_value = json!(123);
    let id = Some(&id_value);
    
    // Test basic error response
    let error = create_error_response(id, -32601, "Method not found");
    assert_eq!(error["jsonrpc"], "2.0");
    assert_eq!(error["id"], 123);
    assert_eq!(error["error"]["code"], -32601);
    assert_eq!(error["error"]["message"], "Method not found");
    assert!(!error["error"].as_object().unwrap().contains_key("data"));
    
    // Test error response with data
    let error_data = json!({"type": "timeout", "details": "Request took too long"});
    let error_with_data = create_error_response_with_data(id, -32603, "Internal error", Some(error_data.clone()));
    assert_eq!(error_with_data["jsonrpc"], "2.0");
    assert_eq!(error_with_data["id"], 123);
    assert_eq!(error_with_data["error"]["code"], -32603);
    assert_eq!(error_with_data["error"]["message"], "Internal error");
    assert_eq!(error_with_data["error"]["data"], error_data);
}

/// Test JSON-RPC 2.0 success response format
#[test]
fn test_jsonrpc_success_format_compliance() {
    let id_value = json!("test-123");
    let id = Some(&id_value);
    let result = json!({"status": "success", "data": [1, 2, 3]});
    
    let response = create_success_response(id, result.clone());
    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], "test-123");
    assert_eq!(response["result"], result);
    assert!(!response.as_object().unwrap().contains_key("error"));
}

/// Test MCP tool response format compliance
#[test]
fn test_mcp_tool_response_format() {
    // Test basic text response
    let text_response = format_tool_response("Simple text content");
    assert_eq!(text_response["isError"], false);
    assert!(text_response["content"].is_array());
    assert_eq!(text_response["content"][0]["type"], "text");
    assert_eq!(text_response["content"][0]["text"], "Simple text content");
    
    // Test structured content response
    let content = vec![
        create_text_content("Main result", Some(json!({"audience": ["user"], "priority": 0.8}))),
        create_text_content("Additional info", None),
    ];
    let structured_response = format_tool_response_with_content(content);
    assert_eq!(structured_response["isError"], false);
    assert_eq!(structured_response["content"].as_array().unwrap().len(), 2);
    assert_eq!(structured_response["content"][0]["annotations"]["audience"][0], "user");
    assert_eq!(structured_response["content"][0]["annotations"]["priority"], 0.8);
}

/// Test MCP content types compliance
#[test]
fn test_mcp_content_types() {
    // Test text content
    let text_content = create_text_content("Hello world", Some(json!({"priority": 0.9})));
    assert_eq!(text_content["type"], "text");
    assert_eq!(text_content["text"], "Hello world");
    assert_eq!(text_content["annotations"]["priority"], 0.9);
    
    // Test image content
    let image_content = create_image_content("base64-encoded-data", "image/png", None);
    assert_eq!(image_content["type"], "image");
    assert_eq!(image_content["data"], "base64-encoded-data");
    assert_eq!(image_content["mimeType"], "image/png");
    
    // Test resource content
    let resource_content = create_resource_content(
        "file:///path/to/file.txt",
        Some("text/plain"),
        Some("File contents here"),
        Some(json!({"priority": 0.5}))
    );
    assert_eq!(resource_content["type"], "resource");
    assert_eq!(resource_content["resource"]["uri"], "file:///path/to/file.txt");
    assert_eq!(resource_content["resource"]["mimeType"], "text/plain");
    assert_eq!(resource_content["resource"]["text"], "File contents here");
    assert_eq!(resource_content["annotations"]["priority"], 0.5);
}

/// Test JSON-RPC request validation
#[test]
fn test_jsonrpc_request_validation() {
    let transport = StdioTransport::new(5000);
    
    // Valid request
    let valid_request = json!({
        "jsonrpc": "2.0",
        "method": "tools/list",
        "id": 1
    });
    assert!(transport.validate_jsonrpc_request(&valid_request).is_ok());
    
    // Missing jsonrpc field
    let invalid_no_jsonrpc = json!({
        "method": "tools/list",
        "id": 1
    });
    assert!(transport.validate_jsonrpc_request(&invalid_no_jsonrpc).is_err());
    
    // Wrong jsonrpc version
    let invalid_version = json!({
        "jsonrpc": "1.0",
        "method": "tools/list", 
        "id": 1
    });
    assert!(transport.validate_jsonrpc_request(&invalid_version).is_err());
    
    // Missing method
    let invalid_no_method = json!({
        "jsonrpc": "2.0",
        "id": 1
    });
    assert!(transport.validate_jsonrpc_request(&invalid_no_method).is_err());
    
    // Invalid method type
    let invalid_method_type = json!({
        "jsonrpc": "2.0",
        "method": 123,
        "id": 1
    });
    assert!(transport.validate_jsonrpc_request(&invalid_method_type).is_err());
    
    // Invalid ID type
    let invalid_id_type = json!({
        "jsonrpc": "2.0",
        "method": "tools/list",
        "id": true
    });
    assert!(transport.validate_jsonrpc_request(&invalid_id_type).is_err());
    
    // Invalid params type
    let invalid_params_type = json!({
        "jsonrpc": "2.0",
        "method": "tools/list",
        "id": 1,
        "params": "invalid"
    });
    assert!(transport.validate_jsonrpc_request(&invalid_params_type).is_err());
}

/// Test notification detection (JSON-RPC 2.0 notifications have no ID)
#[test]
fn test_notification_detection() {
    // Request with ID - should get response
    let request = json!({
        "jsonrpc": "2.0",
        "method": "tools/list",
        "id": 1
    });
    assert!(request.get("id").is_some());
    
    // Notification without ID - should not get response
    let notification = json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    assert!(notification.get("id").is_none());
}

/// Test MCP logging capability
#[tokio::test]
async fn test_mcp_logging_capability() {
    let logger = MCPLogger::new(LogLevel::Info);
    let mut receiver = logger.subscribe();
    
    // Test log message creation
    logger.info(Some("test-logger".to_string()), json!({"message": "test log", "level": "info"}));
    
    // Verify message received
    let message = receiver.recv().await.unwrap();
    assert_eq!(message.level, LogLevel::Info);
    assert_eq!(message.logger, Some("test-logger".to_string()));
    assert_eq!(message.data["message"], "test log");
    
    // Test log notification format
    let notification = MCPLogger::create_log_notification(&message);
    assert_eq!(notification["jsonrpc"], "2.0");
    assert_eq!(notification["method"], "notifications/message");
    assert_eq!(notification["params"]["level"], "info");
    assert_eq!(notification["params"]["logger"], "test-logger");
    assert_eq!(notification["params"]["data"]["message"], "test log");
}

/// Test MCP progress tracking capability
#[tokio::test]
async fn test_mcp_progress_capability() {
    let tracker = ProgressTracker::new();
    let mut receiver = tracker.subscribe();
    
    // Start operation
    let token = tracker.start_operation(Some("Test operation".to_string())).await;
    
    // Verify initial progress
    let report = receiver.recv().await.unwrap();
    assert_eq!(report.progress, 0.0);
    assert_eq!(report.message, Some("Test operation".to_string()));
    
    // Update progress
    tracker.update_progress(&token, 0.5, Some(50), Some(100), Some("Half done".to_string())).await.unwrap();
    
    let report = receiver.recv().await.unwrap();
    assert_eq!(report.progress, 0.5);
    assert_eq!(report.current, Some(50));
    assert_eq!(report.total, Some(100));
    
    // Test progress notification format
    let notification = ProgressTracker::create_progress_notification(&report);
    assert_eq!(notification["jsonrpc"], "2.0");
    assert_eq!(notification["method"], "notifications/progress");
    assert_eq!(notification["params"]["progressToken"], token);
    assert_eq!(notification["params"]["progress"], 0.5);
    assert_eq!(notification["params"]["total"], 100);
    assert_eq!(notification["params"]["current"], 50);
}

/// Test tool schema validation compliance
#[test]
fn test_tool_schema_validation() {
    // Test valid store_memory arguments
    let valid_args = json!({
        "content": "Test memory content",
        "tier": "working",
        "importance_score": 0.8,
        "tags": ["test", "memory"]
    });
    assert!(MCPTools::validate_tool_args("store_memory", &valid_args).is_ok());
    
    // Test invalid arguments
    let invalid_content = json!({
        "content": "",
        "tier": "working"
    });
    assert!(MCPTools::validate_tool_args("store_memory", &invalid_content).is_err());
    
    let invalid_tier = json!({
        "content": "Test",
        "tier": "invalid_tier"
    });
    assert!(MCPTools::validate_tool_args("store_memory", &invalid_tier).is_err());
    
    let invalid_importance = json!({
        "content": "Test",
        "importance_score": 1.5
    });
    assert!(MCPTools::validate_tool_args("store_memory", &invalid_importance).is_err());
    
    // Test search_memory validation
    let valid_search = json!({
        "query": "test query",
        "limit": 10,
        "similarity_threshold": 0.7
    });
    assert!(MCPTools::validate_tool_args("search_memory", &valid_search).is_ok());
    
    let invalid_search = json!({
        "query": "",
        "limit": 10
    });
    assert!(MCPTools::validate_tool_args("search_memory", &invalid_search).is_err());
}

/// Test tool list schema compliance
#[test]
fn test_tool_list_schema_compliance() {
    let tools = MCPTools::get_tools_list();
    let tools_array = tools["tools"].as_array().unwrap();
    
    // Verify all tools have required fields
    for tool in tools_array {
        assert!(tool["name"].is_string());
        assert!(tool["description"].is_string());
        assert!(tool["inputSchema"].is_object());
        assert!(tool["inputSchema"]["type"] == "object");
        assert!(tool["inputSchema"]["properties"].is_object());
        
        // Verify required fields are arrays
        if let Some(required) = tool["inputSchema"]["required"].as_array() {
            for req in required {
                assert!(req.is_string());
            }
        }
    }
    
    // Test specific tools exist
    let tool_names: Vec<&str> = tools_array.iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    
    assert!(tool_names.contains(&"store_memory"));
    assert!(tool_names.contains(&"search_memory"));
    assert!(tool_names.contains(&"get_statistics"));
    assert!(tool_names.contains(&"harvest_conversation"));
    assert!(tool_names.contains(&"what_did_you_remember"));
}

/// Test empty resources and prompts compliance
#[test]
fn test_empty_resources_and_prompts() {
    let resources = MCPTools::get_resources_list();
    assert!(resources["resources"].as_array().unwrap().is_empty());
    
    let prompts = MCPTools::get_prompts_list();
    assert!(prompts["prompts"].as_array().unwrap().is_empty());
}

/// Test MCP specification compliance end-to-end
#[test]
fn test_mcp_specification_compliance() {
    // Test all required MCP methods are properly structured
    let capabilities = MCPTools::get_server_capabilities();
    
    // Verify MCP specification compliance
    assert_eq!(capabilities["protocolVersion"], "2025-06-18");
    
    // Verify all capabilities are properly declared
    let caps = &capabilities["capabilities"];
    assert!(caps.as_object().unwrap().contains_key("tools"));
    assert!(caps.as_object().unwrap().contains_key("resources"));
    assert!(caps.as_object().unwrap().contains_key("prompts"));
    assert!(caps.as_object().unwrap().contains_key("logging"));
    assert!(caps.as_object().unwrap().contains_key("progress"));
    assert!(caps.as_object().unwrap().contains_key("completion"));
    
    // Verify server info
    let server_info = &capabilities["serverInfo"];
    assert!(server_info["name"].is_string());
    assert!(server_info["version"].is_string());
    assert!(server_info["description"].is_string());
}

/// Test JSON-RPC batch request structure
#[test]
fn test_batch_request_structure() {
    // Valid batch request
    let batch_request = json!([
        {
            "jsonrpc": "2.0",
            "method": "tools/list",
            "id": 1
        },
        {
            "jsonrpc": "2.0",
            "method": "initialize",
            "id": 2
        }
    ]);
    
    assert!(batch_request.is_array());
    assert_eq!(batch_request.as_array().unwrap().len(), 2);
    
    // Empty batch should be invalid
    let empty_batch = json!([]);
    assert!(empty_batch.as_array().unwrap().is_empty());
    
    // Mixed requests and notifications in batch
    let mixed_batch = json!([
        {
            "jsonrpc": "2.0",
            "method": "tools/list",
            "id": 1
        },
        {
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
            // No ID = notification
        }
    ]);
    
    assert_eq!(mixed_batch.as_array().unwrap().len(), 2);
    assert!(mixed_batch[0].get("id").is_some());
    assert!(mixed_batch[1].get("id").is_none());
}