//! Comprehensive security tests for MCP authentication
//!
//! This module contains extensive security tests covering all authentication
//! scenarios, bypass attempts, and edge cases to ensure the MCP server
//! is secure against various attack vectors.

use super::auth::*;
use crate::security::AuditConfig;
use anyhow::Result;
use chrono::{Duration, Utc};
use serde_json::json;
use std::collections::HashMap;
use tempfile::tempdir;
use uuid::Uuid;

/// Create test authentication configuration with secure defaults
fn create_secure_test_config() -> MCPAuthConfig {
    let mut api_keys = HashMap::new();
    api_keys.insert(
        "secure-test-key-123456789".to_string(),
        ApiKeyInfo {
            client_id: "test-client".to_string(),
            scopes: vec!["mcp:read".to_string(), "mcp:write".to_string()],
            expires_at: Some(Utc::now() + Duration::hours(1)),
            last_used: None,
            usage_count: 0,
        },
    );

    let mut certs = HashMap::new();
    certs.insert(
        "valid-cert-thumbprint".to_string(),
        CertificateInfo {
            thumbprint: "valid-cert-thumbprint".to_string(),
            client_id: "cert-client".to_string(),
            subject: "CN=Valid Client".to_string(),
            issuer: "CN=Test CA".to_string(),
            not_before: Utc::now() - Duration::days(1),
            not_after: Utc::now() + Duration::days(30),
            scopes: vec!["mcp:read".to_string(), "mcp:write".to_string()],
            revoked: false,
        },
    );

    // Add expired certificate for testing
    certs.insert(
        "expired-cert-thumbprint".to_string(),
        CertificateInfo {
            thumbprint: "expired-cert-thumbprint".to_string(),
            client_id: "expired-cert-client".to_string(),
            subject: "CN=Expired Client".to_string(),
            issuer: "CN=Test CA".to_string(),
            not_before: Utc::now() - Duration::days(60),
            not_after: Utc::now() - Duration::days(1), // Expired
            scopes: vec!["mcp:read".to_string()],
            revoked: false,
        },
    );

    // Add revoked certificate for testing
    certs.insert(
        "revoked-cert-thumbprint".to_string(),
        CertificateInfo {
            thumbprint: "revoked-cert-thumbprint".to_string(),
            client_id: "revoked-cert-client".to_string(),
            subject: "CN=Revoked Client".to_string(),
            issuer: "CN=Test CA".to_string(),
            not_before: Utc::now() - Duration::days(1),
            not_after: Utc::now() + Duration::days(30),
            scopes: vec!["mcp:read".to_string(), "mcp:write".to_string()],
            revoked: true, // Revoked!
        },
    );

    MCPAuthConfig {
        enabled: true,
        jwt_secret: "secure-test-jwt-secret-key-minimum-32-characters-long-for-security".to_string(),
        jwt_expiry_seconds: 3600,
        api_keys,
        allowed_certificates: certs,
        require_scope: vec!["mcp:read".to_string()],
        performance_target_ms: 5,
    }
}

async fn create_secure_test_auth() -> MCPAuth {
    let config = create_secure_test_config();
    let temp_dir = tempdir().unwrap();
    let audit_config = AuditConfig {
        enabled: true,
        log_all_requests: true,
        log_data_access: true,
        log_modifications: true,
        log_auth_events: true,
        retention_days: 30,
    };
    let audit_logger = std::sync::Arc::new(crate::security::AuditLogger::new(audit_config).unwrap());
    MCPAuth::new(config, audit_logger).unwrap()
}

#[tokio::test]
async fn test_authentication_bypass_attempts() {
    let auth = create_secure_test_auth().await;

    // Test 1: No authentication headers provided
    let headers = HashMap::new();
    let result = auth.authenticate_request("tools/call", None, &headers).await;
    assert!(result.is_err(), "Should reject requests without authentication");

    // Test 2: Invalid authorization header format
    let mut headers = HashMap::new();
    headers.insert("authorization".to_string(), "Invalid format".to_string());
    let result = auth.authenticate_request("tools/call", None, &headers).await;
    assert!(result.is_err(), "Should reject invalid authorization formats");

    // Test 3: Empty bearer token
    let mut headers = HashMap::new();
    headers.insert("authorization".to_string(), "Bearer ".to_string());
    let result = auth.authenticate_request("tools/call", None, &headers).await;
    assert!(result.is_err(), "Should reject empty bearer tokens");

    // Test 4: Empty API key
    let mut headers = HashMap::new();
    headers.insert("authorization".to_string(), "ApiKey ".to_string());
    let result = auth.authenticate_request("tools/call", None, &headers).await;
    assert!(result.is_err(), "Should reject empty API keys");
}

#[tokio::test]
async fn test_jwt_token_security() {
    let auth = create_secure_test_auth().await;

    // Generate valid token
    let valid_token = auth
        .generate_token(
            "test-client",
            "test-user", 
            vec!["mcp:read".to_string(), "mcp:write".to_string()],
        )
        .await
        .unwrap();

    // Test valid token works
    let mut headers = HashMap::new();
    headers.insert("authorization".to_string(), format!("Bearer {}", valid_token));
    let result = auth.authenticate_request("tools/call", None, &headers).await;
    assert!(result.is_ok(), "Valid token should be accepted");

    // Test token revocation
    auth.revoke_token(&valid_token).await.unwrap();
    let result = auth.authenticate_request("tools/call", None, &headers).await;
    assert!(result.is_err(), "Revoked token should be rejected");

    // Test malformed JWT
    let mut headers = HashMap::new();
    headers.insert("authorization".to_string(), "Bearer malformed.jwt.token".to_string());
    let result = auth.authenticate_request("tools/call", None, &headers).await;
    assert!(result.is_err(), "Malformed JWT should be rejected");

    // Test JWT with wrong signature
    let mut headers = HashMap::new();
    headers.insert("authorization".to_string(), "Bearer eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiYWRtaW4iOnRydWV9.eoaDVGTClRdfxUZXiPs3f8FmJDkDE_VCQBNn-JPub6I".to_string());
    let result = auth.authenticate_request("tools/call", None, &headers).await;
    assert!(result.is_err(), "JWT with wrong signature should be rejected");
}

#[tokio::test]
async fn test_api_key_security() {
    let auth = create_secure_test_auth().await;

    // Test valid API key
    let mut headers = HashMap::new();
    headers.insert(
        "authorization".to_string(),
        "ApiKey secure-test-key-123456789".to_string(),
    );
    let result = auth.authenticate_request("tools/call", None, &headers).await;
    assert!(result.is_ok(), "Valid API key should be accepted");

    // Test invalid API key
    let mut headers = HashMap::new();
    headers.insert(
        "authorization".to_string(), 
        "ApiKey invalid-key".to_string(),
    );
    let result = auth.authenticate_request("tools/call", None, &headers).await;
    assert!(result.is_err(), "Invalid API key should be rejected");

    // Test API key with special characters (injection attempt)
    let mut headers = HashMap::new();
    headers.insert(
        "authorization".to_string(),
        "ApiKey '; DROP TABLE users; --".to_string(),
    );
    let result = auth.authenticate_request("tools/call", None, &headers).await;
    assert!(result.is_err(), "API key with injection attempt should be rejected");

    // Test very long API key (DoS attempt)
    let long_key = "a".repeat(10000);
    let mut headers = HashMap::new();
    headers.insert("authorization".to_string(), format!("ApiKey {}", long_key));
    let result = auth.authenticate_request("tools/call", None, &headers).await;
    assert!(result.is_err(), "Very long API key should be rejected");
}

#[tokio::test]
async fn test_certificate_validation_security() {
    let auth = create_secure_test_auth().await;

    // Test valid certificate
    let mut headers = HashMap::new();
    headers.insert(
        "x-client-cert-thumbprint".to_string(),
        "valid-cert-thumbprint".to_string(),
    );
    let result = auth.authenticate_request("tools/call", None, &headers).await;
    assert!(result.is_ok(), "Valid certificate should be accepted");

    // Test expired certificate
    let mut headers = HashMap::new();
    headers.insert(
        "x-client-cert-thumbprint".to_string(),
        "expired-cert-thumbprint".to_string(),
    );
    let result = auth.authenticate_request("tools/call", None, &headers).await;
    assert!(result.is_err(), "Expired certificate should be rejected");

    // Test revoked certificate
    let mut headers = HashMap::new();
    headers.insert(
        "x-client-cert-thumbprint".to_string(),
        "revoked-cert-thumbprint".to_string(),
    );
    let result = auth.authenticate_request("tools/call", None, &headers).await;
    assert!(result.is_err(), "Revoked certificate should be rejected");

    // Test unknown certificate
    let mut headers = HashMap::new();
    headers.insert(
        "x-client-cert-thumbprint".to_string(),
        "unknown-cert-thumbprint".to_string(),
    );
    let result = auth.authenticate_request("tools/call", None, &headers).await;
    assert!(result.is_err(), "Unknown certificate should be rejected");

    // Test malicious certificate thumbprint
    let mut headers = HashMap::new();
    headers.insert(
        "x-client-cert-thumbprint".to_string(),
        "../../../etc/passwd".to_string(),
    );
    let result = auth.authenticate_request("tools/call", None, &headers).await;
    assert!(result.is_err(), "Path traversal attempt should be rejected");
}

#[tokio::test]
async fn test_scope_validation_security() {
    let auth = create_secure_test_auth().await;

    // Create auth context with limited scopes
    let limited_context = AuthContext {
        client_id: "test-client".to_string(),
        user_id: "test-user".to_string(),
        method: AuthMethod::ApiKey,
        scopes: vec!["mcp:read".to_string()], // Only read access
        expires_at: None,
        request_id: Uuid::new_v4().to_string(),
    };

    // Test read operation (should succeed)
    assert!(auth.validate_tool_access(&limited_context, "search_memory").is_ok());
    assert!(auth.validate_tool_access(&limited_context, "get_statistics").is_ok());

    // Test write operation (should fail)
    assert!(auth.validate_tool_access(&limited_context, "store_memory").is_err());
    assert!(auth.validate_tool_access(&limited_context, "delete_memory").is_err());
    assert!(auth.validate_tool_access(&limited_context, "migrate_memory").is_err());

    // Test with no scopes
    let no_scope_context = AuthContext {
        client_id: "test-client".to_string(),
        user_id: "test-user".to_string(),
        method: AuthMethod::ApiKey,
        scopes: vec![], // No scopes
        expires_at: None,
        request_id: Uuid::new_v4().to_string(),
    };

    assert!(auth.validate_tool_access(&no_scope_context, "search_memory").is_err());
    assert!(auth.validate_tool_access(&no_scope_context, "store_memory").is_err());
}

#[tokio::test]
async fn test_authentication_timing_attacks() {
    let auth = create_secure_test_auth().await;

    // Test that invalid authentication attempts take similar time
    // This helps prevent timing attacks to enumerate valid credentials
    
    let start_valid = std::time::Instant::now();
    let mut headers = HashMap::new();
    headers.insert(
        "authorization".to_string(),
        "ApiKey secure-test-key-123456789".to_string(),
    );
    let _ = auth.authenticate_request("tools/call", None, &headers).await;
    let valid_duration = start_valid.elapsed();

    let start_invalid = std::time::Instant::now();
    let mut headers = HashMap::new();
    headers.insert(
        "authorization".to_string(),
        "ApiKey invalid-key-same-length-12".to_string(),
    );
    let _ = auth.authenticate_request("tools/call", None, &headers).await;
    let invalid_duration = start_invalid.elapsed();

    // The timing difference should be minimal (within reasonable bounds)
    let timing_diff = if valid_duration > invalid_duration {
        valid_duration - invalid_duration
    } else {
        invalid_duration - valid_duration
    };

    // Allow up to 10ms difference to account for normal variation
    assert!(
        timing_diff.as_millis() < 10,
        "Timing difference too large: {}ms (could enable timing attacks)",
        timing_diff.as_millis()
    );
}

#[tokio::test]
async fn test_concurrent_authentication() {
    use futures::future::join_all;
    
    let auth = std::sync::Arc::new(create_secure_test_auth().await);

    // Test concurrent authentication requests don't interfere
    let mut handles = Vec::new();
    
    for i in 0..10 {
        let auth = auth.clone();
        let handle = tokio::spawn(async move {
            let mut headers = HashMap::new();
            headers.insert(
                "authorization".to_string(),
                "ApiKey secure-test-key-123456789".to_string(),
            );
            auth.authenticate_request(&format!("test-method-{}", i), None, &headers).await
        });
        handles.push(handle);
    }

    let results = join_all(handles).await;
    
    // All requests should succeed
    for (i, result) in results.into_iter().enumerate() {
        assert!(
            result.unwrap().is_ok(),
            "Concurrent authentication request {} should succeed",
            i
        );
    }
}

#[tokio::test]
async fn test_session_management() {
    let auth = create_secure_test_auth().await;

    // Generate token
    let token = auth
        .generate_token("test-client", "test-user", vec!["mcp:read".to_string()])
        .await
        .unwrap();

    // Use token successfully
    let mut headers = HashMap::new();
    headers.insert("authorization".to_string(), format!("Bearer {}", token));
    let result1 = auth.authenticate_request("tools/call", None, &headers).await;
    assert!(result1.is_ok());

    // Revoke token
    auth.revoke_token(&token).await.unwrap();

    // Token should no longer work
    let result2 = auth.authenticate_request("tools/call", None, &headers).await;
    assert!(result2.is_err());

    // Double revocation should not error
    let revoke_result = auth.revoke_token(&token).await;
    assert!(revoke_result.is_ok());
}

#[tokio::test]
async fn test_malformed_requests() {
    let auth = create_secure_test_auth().await;

    // Test with null bytes in headers (could cause issues in C libraries)
    let mut headers = HashMap::new();
    headers.insert("authorization".to_string(), "ApiKey test\0key".to_string());
    let result = auth.authenticate_request("tools/call", None, &headers).await;
    assert!(result.is_err(), "Headers with null bytes should be rejected");

    // Test with extremely long headers
    let long_auth = format!("ApiKey {}", "a".repeat(100000));
    let mut headers = HashMap::new();
    headers.insert("authorization".to_string(), long_auth);
    let result = auth.authenticate_request("tools/call", None, &headers).await;
    assert!(result.is_err(), "Extremely long headers should be rejected");

    // Test with unicode manipulation attempts
    let mut headers = HashMap::new();
    headers.insert("authorization".to_string(), "ApiKey test\u{200E}key".to_string());
    let result = auth.authenticate_request("tools/call", None, &headers).await;
    assert!(result.is_err(), "Headers with unicode manipulation should be rejected");
}

#[tokio::test]
async fn test_disabled_authentication() {
    // Test authentication when disabled
    let mut config = create_secure_test_config();
    config.enabled = false;

    let temp_dir = tempdir().unwrap();
    let audit_config = AuditConfig {
        enabled: true,
        log_all_requests: true,
        log_data_access: true,
        log_modifications: true,
        log_auth_events: true,
        retention_days: 30,
    };
    let audit_logger = std::sync::Arc::new(crate::security::AuditLogger::new(audit_config).unwrap());
    let auth = MCPAuth::new(config, audit_logger).unwrap();

    let headers = HashMap::new();
    let result = auth.authenticate_request("tools/call", None, &headers).await;
    
    // Should succeed but return None (no authentication context)
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}