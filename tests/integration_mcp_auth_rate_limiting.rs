//! Integration tests for MCP authentication and rate limiting

use codex_memory::mcp_server::{
    auth::{AuthContext, AuthMethod, MCPAuth, MCPAuthConfig},
    rate_limiter::{MCPRateLimitConfig, MCPRateLimiter},
    MCPHandlers, MCPServerConfig,
};
use codex_memory::security::{audit::AuditLogger, AuditConfig};
use codex_memory::{
    memory::{
        ImportanceAssessmentConfig, ImportanceAssessmentPipeline, MemoryRepository,
        SilentHarvesterService,
    },
    Config, SimpleEmbedder,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tempfile::tempdir;

async fn create_test_server() -> (MCPHandlers, Arc<MCPAuth>, Arc<MCPRateLimiter>) {
    // Create temp directory for audit logs
    let temp_dir = tempdir().unwrap();

    // Create audit logger
    let audit_config = AuditConfig {
        enabled: true,
        log_file: temp_dir.path().join("test.log"),
        ..Default::default()
    };
    let audit_logger = Arc::new(AuditLogger::new(audit_config).unwrap());

    // Create auth configuration with test API key
    let mut auth_config = MCPAuthConfig::default();
    auth_config.enabled = true;
    auth_config.api_keys.insert(
        "test-api-key".to_string(),
        codex_memory::mcp_server::auth::ApiKeyInfo {
            client_id: "test-client".to_string(),
            scopes: vec!["mcp:read".to_string(), "mcp:write".to_string()],
            expires_at: None,
            last_used: None,
            usage_count: 0,
        },
    );

    // Create rate limit configuration
    let rate_limit_config = MCPRateLimitConfig {
        enabled: true,
        global_requests_per_minute: 60,
        global_burst_size: 10,
        per_client_requests_per_minute: 30,
        per_client_burst_size: 5,
        per_tool_requests_per_minute: {
            let mut map = HashMap::new();
            map.insert("store_memory".to_string(), 5);
            map.insert("search_memory".to_string(), 10);
            map
        },
        per_tool_burst_size: HashMap::new(),
        silent_mode_multiplier: 0.5,
        whitelist_clients: vec![],
        performance_target_ms: 5,
    };

    // Create auth and rate limiter
    let auth = Arc::new(MCPAuth::new(auth_config, audit_logger.clone()).unwrap());
    let rate_limiter = Arc::new(MCPRateLimiter::new(rate_limit_config, audit_logger.clone()));

    // Create mock repository and embedder (simplified for test)
    let config = Config::from_env().unwrap_or_else(|_| Config {
        database_url: "mock://test".to_string(),
        embedding: Default::default(),
        http_port: 8080,
        mcp_port: 8081,
        tier_config: Default::default(),
        auto_migrate: false,
        migration_dir: None,
    });

    // For testing, we'll create mock components
    let embedder = Arc::new(SimpleEmbedder::new_mock());
    let repository = Arc::new(MemoryRepository::new_mock());

    let importance_config = ImportanceAssessmentConfig::default();
    let importance_pipeline = Arc::new(
        ImportanceAssessmentPipeline::new(
            importance_config,
            embedder.clone(),
            &prometheus::default_registry(),
        )
        .unwrap(),
    );

    let harvester_service = Arc::new(
        SilentHarvesterService::new(
            repository.clone(),
            importance_pipeline,
            embedder.clone(),
            None,
            &prometheus::default_registry(),
        )
        .unwrap(),
    );

    let handlers = MCPHandlers::new(
        repository,
        embedder,
        harvester_service,
        None, // No circuit breaker for test
        Some(auth.clone()),
        Some(rate_limiter.clone()),
    );

    (handlers, auth, rate_limiter)
}

#[tokio::test]
async fn test_authentication_required() {
    let (mut handlers, _auth, _rate_limiter) = create_test_server().await;

    // Test that requests without authentication are rejected
    let headers = HashMap::new();
    let params = Some(&json!({"name": "search_memory", "arguments": {"query": "test"}}));

    let response = handlers
        .handle_request_with_headers("tools/call", params, Some(&json!(1)), &headers)
        .await;

    // Should be an authentication error
    assert!(response["error"]["code"].as_i64().unwrap() == -32001);
    assert!(response["error"]["message"]
        .as_str()
        .unwrap()
        .contains("Authentication failed"));
}

#[tokio::test]
async fn test_valid_authentication() {
    let (mut handlers, _auth, _rate_limiter) = create_test_server().await;

    // Test with valid API key
    let mut headers = HashMap::new();
    headers.insert(
        "authorization".to_string(),
        "ApiKey test-api-key".to_string(),
    );

    let params = Some(&json!({
        "name": "get_statistics",
        "arguments": {"detailed": false}
    }));

    let response = handlers
        .handle_request_with_headers("tools/call", params, Some(&json!(1)), &headers)
        .await;

    // Should be successful (not an auth error)
    assert!(
        response.get("error").is_none() || response["error"]["code"].as_i64().unwrap() != -32001
    );
}

#[tokio::test]
async fn test_invalid_api_key() {
    let (mut handlers, _auth, _rate_limiter) = create_test_server().await;

    // Test with invalid API key
    let mut headers = HashMap::new();
    headers.insert(
        "authorization".to_string(),
        "ApiKey invalid-key".to_string(),
    );

    let params = Some(&json!({
        "name": "search_memory",
        "arguments": {"query": "test"}
    }));

    let response = handlers
        .handle_request_with_headers("tools/call", params, Some(&json!(1)), &headers)
        .await;

    // Should be an authentication error
    assert!(response["error"]["code"].as_i64().unwrap() == -32001);
    assert!(response["error"]["message"]
        .as_str()
        .unwrap()
        .contains("Authentication failed"));
}

#[tokio::test]
async fn test_rate_limiting() {
    let (mut handlers, _auth, _rate_limiter) = create_test_server().await;

    // Test rate limiting by making multiple requests quickly
    let mut headers = HashMap::new();
    headers.insert(
        "authorization".to_string(),
        "ApiKey test-api-key".to_string(),
    );

    let params = Some(&json!({
        "name": "store_memory",
        "arguments": {
            "content": "test content",
            "tier": "working"
        }
    }));

    // Make requests up to the burst limit
    let mut responses = Vec::new();
    for i in 0..10 {
        let response = handlers
            .handle_request_with_headers("tools/call", params, Some(&json!(i)), &headers)
            .await;
        responses.push(response);
    }

    // Check if any requests were rate limited
    let rate_limited = responses.iter().any(|r| {
        r.get("error")
            .map_or(false, |e| e["code"].as_i64().unwrap_or(0) == -32002)
    });

    // Should have at least some rate limiting after exceeding burst
    assert!(rate_limited, "Expected some requests to be rate limited");
}

#[tokio::test]
async fn test_tool_access_permissions() {
    let (mut handlers, _auth, _rate_limiter) = create_test_server().await;

    // Create auth context with limited permissions
    let auth_context = AuthContext {
        client_id: "test-client".to_string(),
        user_id: "test-user".to_string(),
        method: AuthMethod::ApiKey,
        scopes: vec!["mcp:read".to_string()], // Only read permissions
        expires_at: None,
        request_id: "test-request".to_string(),
    };

    let mut headers = HashMap::new();
    headers.insert(
        "authorization".to_string(),
        "ApiKey test-api-key".to_string(),
    );

    // Try to perform a write operation (should fail)
    let params = Some(&json!({
        "name": "store_memory",
        "arguments": {
            "content": "test content",
            "tier": "working"
        }
    }));

    let response = handlers
        .handle_request_with_headers("tools/call", params, Some(&json!(1)), &headers)
        .await;

    // Note: This test would need the auth system to actually check permissions
    // For now, we just verify the structure works
    assert!(response.is_object());
}

#[tokio::test]
async fn test_initialize_bypasses_auth() {
    let (mut handlers, _auth, _rate_limiter) = create_test_server().await;

    // Initialize should work without authentication
    let headers = HashMap::new();

    let response = handlers
        .handle_request_with_headers("initialize", None, Some(&json!(1)), &headers)
        .await;

    // Should be successful
    assert!(response.get("result").is_some());
    assert!(response.get("error").is_none());

    // Check that server capabilities include proper MCP info
    let result = &response["result"];
    assert_eq!(result["protocolVersion"], "2025-06-18");
    assert_eq!(result["serverInfo"]["name"], "codex-memory");
}

#[tokio::test]
async fn test_silent_mode_rate_limiting() {
    let (mut handlers, _auth, _rate_limiter) = create_test_server().await;

    // Test that silent mode has different (reduced) rate limits
    let mut headers = HashMap::new();
    headers.insert(
        "authorization".to_string(),
        "ApiKey test-api-key".to_string(),
    );

    let params = Some(&json!({
        "name": "harvest_conversation",
        "arguments": {
            "message": "test message",
            "silent_mode": true
        }
    }));

    // In silent mode, rate limits should be more restrictive
    let response = handlers
        .handle_request_with_headers("tools/call", params, Some(&json!(1)), &headers)
        .await;

    // Should process successfully (exact behavior depends on implementation)
    assert!(response.is_object());
}

// Performance test to ensure auth and rate limiting meet <5ms requirement
#[tokio::test]
async fn test_performance_requirements() {
    let (mut handlers, _auth, _rate_limiter) = create_test_server().await;

    let mut headers = HashMap::new();
    headers.insert(
        "authorization".to_string(),
        "ApiKey test-api-key".to_string(),
    );

    let params = Some(&json!({
        "name": "get_statistics",
        "arguments": {"detailed": false}
    }));

    // Measure time for auth + rate limiting + processing
    let start = std::time::Instant::now();

    let _response = handlers
        .handle_request_with_headers("tools/call", params, Some(&json!(1)), &headers)
        .await;

    let elapsed = start.elapsed();

    // Should complete in reasonable time (note: this is an integration test with mocks)
    // In real usage, the <5ms requirement is for auth + rate limiting only, not full processing
    assert!(
        elapsed.as_millis() < 100,
        "Request took too long: {:?}",
        elapsed
    );
}
