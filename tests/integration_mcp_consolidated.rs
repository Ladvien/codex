//! Integration tests for consolidated MCP implementation
//!
//! Tests the complete MCP server functionality including:
//! - Protocol initialization
//! - Tool execution
//! - Circuit breaker behavior
//! - Silent harvester integration
//! - Error handling and timeouts

use codex_memory::{
    mcp_server::{
        circuit_breaker::{CircuitBreaker, CircuitBreakerConfig, CircuitState},
        handlers::MCPHandlers,
        logging::MCPLogger,
        progress::ProgressTracker,
        tools::MCPTools,
        MCPServer, MCPServerConfig,
    },
    memory::{connection::create_pool, MemoryRepository},
    Config, SimpleEmbedder,
};
use serde_json::json;
use std::{sync::Arc, time::Duration};
use tokio::time::timeout;

#[tokio::test]
async fn test_mcp_server_creation() -> anyhow::Result<()> {
    let config = test_config();
    let (repository, embedder) = create_test_components(config).await?;

    let mcp_config = MCPServerConfig::default();
    let mcp_server = MCPServer::new(repository, embedder, mcp_config)?;

    // Test server statistics
    let stats = mcp_server.get_stats().await?;
    assert!(stats["server"]["protocol_version"] == "2025-06-18");
    assert!(stats["server"]["transport"] == "stdio");

    Ok(())
}

#[tokio::test]
async fn test_mcp_tools_schema() -> anyhow::Result<()> {
    let tools = MCPTools::get_tools_list();
    let tools_array = tools["tools"].as_array().unwrap();

    // Check that all expected tools are present
    let tool_names: Vec<&str> = tools_array
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();

    assert!(tool_names.contains(&"store_memory"));
    assert!(tool_names.contains(&"search_memory"));
    assert!(tool_names.contains(&"get_statistics"));
    assert!(tool_names.contains(&"what_did_you_remember"));
    assert!(tool_names.contains(&"harvest_conversation"));
    assert!(tool_names.contains(&"get_harvester_metrics"));
    assert!(tool_names.contains(&"migrate_memory"));
    assert!(tool_names.contains(&"delete_memory"));

    // Validate tool schema structure
    for tool in tools_array {
        assert!(tool["name"].is_string());
        assert!(tool["description"].is_string());
        assert!(tool["inputSchema"].is_object());
        assert!(tool["inputSchema"]["type"] == "object");
        assert!(tool["inputSchema"]["properties"].is_object());
    }

    Ok(())
}

#[tokio::test]
async fn test_mcp_handlers_initialization() -> anyhow::Result<()> {
    let config = test_config();
    let (repository, embedder) = create_test_components(config).await?;

    // Test with circuit breaker
    let circuit_breaker = Some(Arc::new(CircuitBreaker::new(
        CircuitBreakerConfig::default(),
    )));

    let importance_config = codex_memory::memory::ImportanceAssessmentConfig::default();
    let importance_pipeline = Arc::new(codex_memory::memory::ImportanceAssessmentPipeline::new(
        importance_config,
        embedder.clone(),
        &prometheus::default_registry(),
    )?);

    let harvester_service = Arc::new(codex_memory::memory::SilentHarvesterService::new(
        repository.clone(),
        importance_pipeline,
        embedder.clone(),
        None,
        &prometheus::default_registry(),
    )?);

    let mcp_logger = Arc::new(MCPLogger::new(None));
    let progress_tracker = Arc::new(ProgressTracker::new());

    let mut handlers = MCPHandlers::new(
        repository,
        embedder,
        harvester_service,
        circuit_breaker.clone(),
        None, // auth
        None, // rate_limiter
        mcp_logger,
        progress_tracker,
    );

    // Test initialization request
    let init_response = handlers
        .handle_request("initialize", None, Some(&json!(1)))
        .await;

    assert_eq!(init_response["jsonrpc"], "2.0");
    assert_eq!(init_response["id"], 1);
    assert_eq!(init_response["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(
        init_response["result"]["serverInfo"]["name"],
        "codex-memory"
    );

    // Test tools/list request
    let tools_response = handlers
        .handle_request("tools/list", None, Some(&json!(2)))
        .await;

    assert_eq!(tools_response["jsonrpc"], "2.0");
    assert_eq!(tools_response["id"], 2);
    assert!(tools_response["result"]["tools"].is_array());

    Ok(())
}

#[tokio::test]
async fn test_store_memory_tool() -> anyhow::Result<()> {
    let config = test_config();
    let (repository, embedder) = create_test_components(config).await?;
    let mut handlers = create_test_handlers(repository, embedder).await?;

    let params = json!({
        "name": "store_memory",
        "arguments": {
            "content": "Test memory content for MCP integration",
            "tier": "working",
            "tags": ["test", "mcp", "integration"]
        }
    });

    let response = handlers
        .handle_request("tools/call", Some(&params), Some(&json!(3)))
        .await;

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 3);
    assert!(response.get("error").is_none());
    assert!(response["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("Successfully stored memory"));

    Ok(())
}

#[tokio::test]
async fn test_search_memory_tool() -> anyhow::Result<()> {
    let config = test_config();
    let (repository, embedder) = create_test_components(config).await?;
    let mut handlers = create_test_handlers(repository.clone(), embedder.clone()).await?;

    // First store a memory
    let store_params = json!({
        "name": "store_memory",
        "arguments": {
            "content": "Integration test memory for searching",
            "tier": "working"
        }
    });

    let store_response = handlers
        .handle_request("tools/call", Some(&store_params), Some(&json!(1)))
        .await;
    assert!(store_response.get("error").is_none());

    // Wait a moment for the memory to be indexed
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Now search for it
    let search_params = json!({
        "name": "search_memory",
        "arguments": {
            "query": "integration test memory",
            "limit": 5
        }
    });

    let search_response = handlers
        .handle_request("tools/call", Some(&search_params), Some(&json!(2)))
        .await;

    assert_eq!(search_response["jsonrpc"], "2.0");
    assert_eq!(search_response["id"], 2);
    assert!(search_response.get("error").is_none());

    let result_text = search_response["result"]["content"][0]["text"]
        .as_str()
        .unwrap();
    assert!(result_text.contains("Found") || result_text.contains("No memories found"));

    Ok(())
}

#[tokio::test]
async fn test_get_statistics_tool() -> anyhow::Result<()> {
    let config = test_config();
    let (repository, embedder) = create_test_components(config).await?;
    let mut handlers = create_test_handlers(repository, embedder).await?;

    let params = json!({
        "name": "get_statistics",
        "arguments": {
            "detailed": true
        }
    });

    let response = handlers
        .handle_request("tools/call", Some(&params), Some(&json!(4)))
        .await;

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 4);
    assert!(response.get("error").is_none());

    let result_text = response["result"]["content"][0]["text"].as_str().unwrap();
    assert!(result_text.contains("Memory System Statistics"));

    Ok(())
}

#[tokio::test]
async fn test_what_did_you_remember_tool() -> anyhow::Result<()> {
    let config = test_config();
    let (repository, embedder) = create_test_components(config).await?;
    let mut handlers = create_test_handlers(repository, embedder).await?;

    let params = json!({
        "name": "what_did_you_remember",
        "arguments": {
            "context": "conversation",
            "time_range": "last_day",
            "limit": 5
        }
    });

    let response = handlers
        .handle_request("tools/call", Some(&params), Some(&json!(5)))
        .await;

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 5);
    assert!(response.get("error").is_none());

    Ok(())
}

#[tokio::test]
async fn test_harvest_conversation_tool() -> anyhow::Result<()> {
    let config = test_config();
    let (repository, embedder) = create_test_components(config).await?;
    let mut handlers = create_test_handlers(repository, embedder).await?;

    // Test with message queuing
    let params = json!({
        "name": "harvest_conversation",
        "arguments": {
            "message": "Test message for harvesting",
            "context": "test_conversation",
            "role": "user",
            "silent_mode": true
        }
    });

    let response = handlers
        .handle_request("tools/call", Some(&params), Some(&json!(6)))
        .await;

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 6);
    assert!(response.get("error").is_none());

    let result_text = response["result"]["content"][0]["text"].as_str().unwrap();
    assert!(result_text.contains("queued") || result_text.contains("completed"));

    Ok(())
}

#[tokio::test]
async fn test_get_harvester_metrics_tool() -> anyhow::Result<()> {
    let config = test_config();
    let (repository, embedder) = create_test_components(config).await?;
    let mut handlers = create_test_handlers(repository, embedder).await?;

    let params = json!({
        "name": "get_harvester_metrics",
        "arguments": {}
    });

    let response = handlers
        .handle_request("tools/call", Some(&params), Some(&json!(7)))
        .await;

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 7);
    assert!(response.get("error").is_none());

    let result_text = response["result"]["content"][0]["text"].as_str().unwrap();
    assert!(result_text.contains("Silent Harvester Metrics"));

    Ok(())
}

#[tokio::test]
async fn test_circuit_breaker_integration() -> anyhow::Result<()> {
    let config = test_config();
    let (repository, embedder) = create_test_components(config).await?;

    // Create circuit breaker with low failure threshold for testing
    let cb_config = CircuitBreakerConfig {
        failure_threshold: 2,
        success_threshold: 2,
        timeout: Duration::from_millis(100),
        half_open_max_calls: 1,
    };

    let circuit_breaker = Arc::new(CircuitBreaker::new(cb_config));

    // Test circuit breaker states
    assert_eq!(circuit_breaker.get_state().await, CircuitState::Closed);

    // Test successful call
    let result = circuit_breaker
        .call_sync(|| -> Result<String, &'static str> { Ok("success".to_string()) })
        .await;
    assert!(result.is_ok());
    assert_eq!(circuit_breaker.get_state().await, CircuitState::Closed);

    // Force failures to open circuit
    for _ in 0..2 {
        let _ = circuit_breaker
            .call_sync(|| -> Result<String, &'static str> { Err("failure") })
            .await;
    }
    assert_eq!(circuit_breaker.get_state().await, CircuitState::Open);

    // Test that circuit rejects calls when open
    let result = circuit_breaker
        .call_sync(|| -> Result<String, &'static str> { Ok("should be rejected".to_string()) })
        .await;
    assert!(result.is_err());

    // Wait for timeout and test recovery
    tokio::time::sleep(Duration::from_millis(150)).await;
    let result = circuit_breaker
        .call_sync(|| -> Result<String, &'static str> { Ok("recovery".to_string()) })
        .await;
    assert!(result.is_ok());

    Ok(())
}

#[tokio::test]
async fn test_tool_validation() -> anyhow::Result<()> {
    // Test valid arguments
    let valid_store_args = json!({
        "content": "Test memory",
        "tier": "working",
        "importance_score": 0.8
    });
    assert!(MCPTools::validate_tool_args("store_memory", &valid_store_args).is_ok());

    // Test invalid arguments
    let invalid_store_args = json!({
        "content": "",  // Empty content
        "tier": "working"
    });
    assert!(MCPTools::validate_tool_args("store_memory", &invalid_store_args).is_err());

    let invalid_tier_args = json!({
        "content": "Test",
        "tier": "invalid_tier"
    });
    assert!(MCPTools::validate_tool_args("store_memory", &invalid_tier_args).is_err());

    // Test search validation
    let valid_search_args = json!({
        "query": "test query",
        "limit": 10
    });
    assert!(MCPTools::validate_tool_args("search_memory", &valid_search_args).is_ok());

    let invalid_search_args = json!({
        "query": "",  // Empty query
        "limit": 10
    });
    assert!(MCPTools::validate_tool_args("search_memory", &invalid_search_args).is_err());

    // Test unknown tool
    let unknown_tool = json!({"param": "value"});
    assert!(MCPTools::validate_tool_args("unknown_tool", &unknown_tool).is_err());

    Ok(())
}

#[tokio::test]
async fn test_error_handling() -> anyhow::Result<()> {
    let config = test_config();
    let (repository, embedder) = create_test_components(config).await?;
    let mut handlers = create_test_handlers(repository, embedder).await?;

    // Test invalid method
    let response = handlers
        .handle_request("invalid_method", None, Some(&json!(1)))
        .await;
    assert_eq!(response["error"]["code"], -32601);
    assert_eq!(response["error"]["message"], "Method not found");

    // Test missing tool name
    let invalid_params = json!({
        "arguments": {"content": "test"}
    });
    let response = handlers
        .handle_request("tools/call", Some(&invalid_params), Some(&json!(2)))
        .await;
    assert_eq!(response["error"]["code"], -32602);

    // Test invalid tool arguments
    let invalid_tool_params = json!({
        "name": "store_memory",
        "arguments": {"content": ""}  // Empty content
    });
    let response = handlers
        .handle_request("tools/call", Some(&invalid_tool_params), Some(&json!(3)))
        .await;
    assert_eq!(response["error"]["code"], -32602);

    Ok(())
}

#[tokio::test]
async fn test_timeout_handling() -> anyhow::Result<()> {
    let config = test_config();
    let (repository, embedder) = create_test_components(config).await?;
    let mut handlers = create_test_handlers(repository, embedder).await?;

    // Test that normal operations complete within timeout
    let result = timeout(
        Duration::from_secs(5),
        handlers.handle_request("tools/list", None, Some(&json!(1))),
    )
    .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 1);

    Ok(())
}

// Helper functions

fn test_config() -> Config {
    use codex_memory::config::*;

    Config {
        database_url: std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://test:test@localhost:5432/codex_memory_test".to_string()
        }),
        embedding: EmbeddingConfig {
            provider: "mock".to_string(),
            api_key: "test".to_string(),
            model: "test-model".to_string(),
            base_url: "http://localhost:11434".to_string(),
            timeout_seconds: 30,
        },
        operational: OperationalConfig {
            max_db_connections: 5,
            request_timeout_seconds: 30,
            enable_metrics: false,
            log_level: "info".to_string(),
        },
        http_port: 3000,
        mcp_port: None,
        tier_config: TierConfig::default(),
        backup: BackupConfiguration::default(),
        security: SecurityConfiguration::default(),
        tier_manager: TierManagerConfig::default(),
        forgetting: ForgettingConfig::default(),
    }
}

async fn create_test_components(
    config: Config,
) -> anyhow::Result<(Arc<MemoryRepository>, Arc<SimpleEmbedder>)> {
    let pool = create_pool(&config.database_url, config.operational.max_db_connections).await?;
    let repository = Arc::new(MemoryRepository::new(pool));
    let embedder = Arc::new(SimpleEmbedder::new_mock());

    Ok((repository, embedder))
}

async fn create_test_handlers(
    repository: Arc<MemoryRepository>,
    embedder: Arc<SimpleEmbedder>,
) -> anyhow::Result<MCPHandlers> {
    let importance_config = codex_memory::memory::ImportanceAssessmentConfig::default();
    let importance_pipeline = Arc::new(codex_memory::memory::ImportanceAssessmentPipeline::new(
        importance_config,
        embedder.clone(),
        &prometheus::default_registry(),
    )?);

    let harvester_service = Arc::new(codex_memory::memory::SilentHarvesterService::new(
        repository.clone(),
        importance_pipeline,
        embedder.clone(),
        None,
        &prometheus::default_registry(),
    )?);

    let circuit_breaker = Some(Arc::new(CircuitBreaker::new(
        CircuitBreakerConfig::default(),
    )));

    let mcp_logger = Arc::new(MCPLogger::new(None));
    let progress_tracker = Arc::new(ProgressTracker::new());

    Ok(MCPHandlers::new(
        repository,
        embedder,
        harvester_service,
        circuit_breaker,
        None, // auth
        None, // rate_limiter
        mcp_logger,
        progress_tracker,
    ))
}
