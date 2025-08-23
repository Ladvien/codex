//! Simplified End-to-End Security Testing Scenarios
//!
//! These tests validate security mechanisms work correctly in realistic scenarios,
//! focusing on the most critical security workflows without complex cloning requirements.

mod test_helpers;

use anyhow::Result;
use chrono::Utc;
use codex_memory::memory::models::{CreateMemoryRequest, MemoryTier, SearchRequest};
use codex_memory::security::{
    AuthConfig, AuthManager, PiiConfig, PiiManager, RateLimitConfig, RateLimitManager,
    ValidationConfig, ValidationManager,
};
use serde_json::json;
use std::time::Duration as StdDuration;
use test_helpers::TestEnvironment;
use tokio::time::sleep;
use tracing_test::traced_test;

/// Helper function to create a basic search request
fn create_search_request(
    query: &str,
    limit: Option<i32>,
    tier: Option<MemoryTier>,
) -> SearchRequest {
    SearchRequest {
        query_text: Some(query.to_string()),
        query_embedding: None,
        search_type: None,
        hybrid_weights: None,
        tier,
        date_range: None,
        importance_range: None,
        metadata_filters: None,
        tags: None,
        limit,
        offset: None,
        cursor: None,
        similarity_threshold: None,
        include_metadata: Some(true),
        include_facets: None,
        ranking_boost: None,
        explain_score: None,
    }
}

/// Test authentication workflow with token validation
#[tokio::test]
#[traced_test]
async fn test_authentication_workflow_e2e() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Set up authentication manager
    let mut auth_config = AuthConfig::default();
    auth_config.enabled = true;
    auth_config.jwt_secret =
        "test-secret-key-for-e2e-security-testing-with-sufficient-length-required".to_string();
    auth_config.jwt_expiry_seconds = 3600; // 1 hour

    let auth_manager = AuthManager::new(auth_config)?;

    // Test user scenarios
    let test_users = vec![
        ("user1", "testuser1", "user", vec!["read".to_string()]),
        (
            "admin1",
            "adminuser1",
            "admin",
            vec![
                "read".to_string(),
                "write".to_string(),
                "delete".to_string(),
            ],
        ),
    ];

    // Create and validate tokens
    for (user_id, username, role, permissions) in &test_users {
        let token = auth_manager
            .create_jwt_token(user_id, username, role, permissions.clone())
            .await?;

        assert!(
            !token.is_empty(),
            "Token should be generated for user {user_id}"
        );

        let claims = auth_manager.validate_jwt_token(&token).await?;
        assert_eq!(
            claims.sub, *user_id,
            "Token should be valid for user {user_id}"
        );
        assert_eq!(claims.permissions, *permissions, "Permissions should match");

        println!(
            "✓ Authentication successful for user: {username} (role: {role})"
        );
    }

    // Test expired token
    let expired_config = AuthConfig {
        enabled: true,
        jwt_secret: "expired-test-secret-key-for-e2e-security-testing".to_string(),
        jwt_expiry_seconds: 1, // 1 second expiry
        ..Default::default()
    };

    let expired_auth_manager = AuthManager::new(expired_config)?;
    let short_lived_token = expired_auth_manager
        .create_jwt_token("temp_user", "tempuser", "user", vec!["read".to_string()])
        .await?;

    sleep(StdDuration::from_secs(2)).await;

    let expired_validation = expired_auth_manager
        .validate_jwt_token(&short_lived_token)
        .await;
    assert!(
        expired_validation.is_err(),
        "Expired token should fail validation"
    );

    println!("✓ Token expiry validation working correctly");

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test PII detection in memory operations
#[tokio::test]
#[traced_test]
async fn test_pii_protection_workflow_e2e() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Set up PII protection
    let mut pii_config = PiiConfig::default();
    pii_config.enabled = true;

    let pii_manager = PiiManager::new(pii_config)?;

    // Test content with PII
    let test_cases = vec![
        (
            "User profile: john.doe@example.com, SSN: 123-45-6789",
            true,
            "contact_with_pii",
        ),
        (
            "Technical documentation for API endpoints",
            false,
            "safe_technical",
        ),
    ];

    let mut created_memories = Vec::new();

    for (content, should_detect_pii, category) in &test_cases {
        let detection_result = pii_manager.detect_pii(content);

        if *should_detect_pii {
            assert!(
                !detection_result.found_patterns.is_empty(),
                "Should detect PII in content: {category}"
            );
            assert!(
                detection_result.requires_action,
                "Should require action for PII content: {category}"
            );
        } else {
            assert!(
                detection_result.found_patterns.is_empty(),
                "Should not detect PII in safe content: {category}"
            );
        }

        println!(
            "PII detection for {}: {} patterns found",
            category,
            detection_result.found_patterns.len()
        );

        // Create memory regardless of PII (in production, PII might be blocked)
        let memory = env
            .repository
            .create_memory(CreateMemoryRequest {
                content: content.to_string(),
                embedding: None,
                tier: Some(MemoryTier::Working),
                importance_score: Some(0.6),
                metadata: Some(json!({
                    "security_test": "pii_protection",
                    "category": category,
                    "pii_detected": !detection_result.found_patterns.is_empty()
                })),
                parent_id: None,
                expires_at: None,
            })
            .await?;

        created_memories.push(memory);
    }

    // Test search for PII-containing memories
    let search_request = create_search_request("security_test", Some(10), None);
    let search_results = env.repository.search_memories(search_request).await?;

    println!(
        "Found {} memories in PII search",
        search_results.results.len()
    );

    // Cleanup
    for memory in created_memories {
        env.repository.delete_memory(memory.id).await?;
    }

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test rate limiting workflow
#[tokio::test]
#[traced_test]
async fn test_rate_limiting_workflow_e2e() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Set up rate limiting
    let mut rate_config = RateLimitConfig::default();
    rate_config.enabled = true;
    rate_config.requests_per_minute = 10; // Low for testing
    rate_config.burst_size = 3;

    let rate_limiter = RateLimitManager::new(rate_config);

    // Create test memory
    let test_memory = env
        .repository
        .create_memory(CreateMemoryRequest {
            content: "Rate limiting test memory".to_string(),
            embedding: None,
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.5),
            metadata: Some(json!({"security_test": "rate_limiting"})),
            parent_id: None,
            expires_at: None,
        })
        .await?;

    // Test rate limiting
    let test_ip: std::net::IpAddr = "192.168.1.100".parse()?;
    let mut successful_requests = 0;
    let mut rate_limited_requests = 0;

    // Try multiple requests
    for i in 0..8 {
        match rate_limiter.check_ip_limit(test_ip).await {
            Ok(_) => {
                // Perform operation
                let _retrieved = env.repository.get_memory(test_memory.id).await?;
                successful_requests += 1;
                println!("Request {} successful", i + 1);
            }
            Err(_) => {
                rate_limited_requests += 1;
                println!("Request {} rate limited", i + 1);
            }
        }

        sleep(StdDuration::from_millis(100)).await;
    }

    println!(
        "Rate limiting test: {}/{} requests successful",
        successful_requests,
        successful_requests + rate_limited_requests
    );

    // Should have some successful requests initially, then rate limiting kicks in
    assert!(
        successful_requests >= 2,
        "Should allow some requests initially"
    );

    // Test different IP has separate limit
    let different_ip: std::net::IpAddr = "10.0.0.50".parse()?;
    let different_result = rate_limiter.check_ip_limit(different_ip).await;

    if different_result.is_ok() {
        println!("✓ Different IP has separate rate limit");
    }

    // Cleanup
    env.repository.delete_memory(test_memory.id).await?;
    env.cleanup_test_data().await?;
    Ok(())
}

/// Test input validation workflow
#[tokio::test]
#[traced_test]
async fn test_input_validation_workflow_e2e() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Set up input validation
    let mut validation_config = ValidationConfig::default();
    validation_config.enabled = true;
    validation_config.sanitize_input = true;
    validation_config.xss_protection = true;
    validation_config.sql_injection_protection = true;

    let validator = ValidationManager::new(validation_config)?;

    // Test various input types
    let validation_test_cases = vec![
        (
            "<script>alert('xss')</script>",
            "xss_script",
            true, // should be modified
        ),
        ("'; DROP TABLE users; --", "sql_injection", true),
        (
            "Normal safe content",
            "safe_content",
            false, // should not be modified
        ),
    ];

    let mut created_memories = Vec::new();

    for (input, test_id, should_be_modified) in &validation_test_cases {
        println!("Testing validation for: {test_id}");

        let validation_result = validator.validate_input(input)?;
        let was_modified = validation_result != *input;

        if *should_be_modified {
            assert!(
                was_modified,
                "Malicious input should be modified for test: {test_id}"
            );
        }

        println!("  Input: '{input}'");
        println!("  Output: '{validation_result}'");
        println!("  Modified: {was_modified}");

        // Create memory with validated input
        let memory = env
            .repository
            .create_memory(CreateMemoryRequest {
                content: validation_result,
                embedding: None,
                tier: Some(MemoryTier::Working),
                importance_score: Some(0.5),
                metadata: Some(json!({
                    "security_test": "input_validation",
                    "test_id": test_id,
                    "was_modified": was_modified
                })),
                parent_id: None,
                expires_at: None,
            })
            .await?;

        created_memories.push(memory);
    }

    // Test search with validated queries
    for malicious_query in &[
        "<script>alert('search')</script>",
        "'; DROP TABLE memories; --",
    ] {
        let validated_query = validator.validate_input(malicious_query)?;
        println!(
            "Search query '{malicious_query}' validated to: '{validated_query}'"
        );

        let search_request = create_search_request(&validated_query, Some(5), None);
        let search_results = env.repository.search_memories(search_request).await?;

        println!(
            "  Search completed, found {} results",
            search_results.results.len()
        );

        // Verify no malicious content in results
        for result in &search_results.results {
            assert!(
                !result.memory.content.contains("<script>"),
                "Results should not contain unescaped script tags"
            );
        }
    }

    // Cleanup
    for memory in created_memories {
        env.repository.delete_memory(memory.id).await?;
    }

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test integrated security workflow (simplified)
#[tokio::test]
#[traced_test]
async fn test_integrated_security_workflow_simple() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Set up all security components
    let auth_config = AuthConfig {
        enabled: true,
        jwt_secret: "integrated-test-secret-key-with-sufficient-length".to_string(),
        ..Default::default()
    };

    let pii_config = PiiConfig {
        enabled: true,
        ..Default::default()
    };

    let rate_config = RateLimitConfig {
        enabled: true,
        requests_per_minute: 20,
        burst_size: 5,
        ..Default::default()
    };

    let validation_config = ValidationConfig {
        enabled: true,
        sanitize_input: true,
        ..Default::default()
    };

    let auth_manager = AuthManager::new(auth_config)?;
    let pii_manager = PiiManager::new(pii_config)?;
    let rate_limiter = RateLimitManager::new(rate_config);
    let validator = ValidationManager::new(validation_config)?;

    println!("All security components initialized");

    // Simulate user workflow
    let user_id = "test_user";
    let client_ip: std::net::IpAddr = "192.168.1.100".parse()?;

    // Step 1: Authentication
    let token = auth_manager
        .create_jwt_token(
            user_id,
            "testuser",
            "user",
            vec!["read".to_string(), "write".to_string()],
        )
        .await?;

    let claims = auth_manager.validate_jwt_token(&token).await?;
    assert_eq!(claims.sub, user_id);
    println!("✓ Authentication successful");

    // Step 2: Rate limiting
    let rate_check = rate_limiter.check_ip_limit(client_ip).await;
    assert!(rate_check.is_ok(), "Rate limit check should pass");
    println!("✓ Rate limiting passed");

    // Step 3: Process content with security checks
    let test_content = "User wants to store: This is safe technical documentation";

    // Input validation
    let validated_content = validator.validate_input(test_content)?;
    println!("✓ Input validation passed");

    // PII detection
    let pii_result = pii_manager.detect_pii(&validated_content);
    if pii_result.requires_action {
        println!(
            "⚠ PII detected: {} patterns",
            pii_result.found_patterns.len()
        );
    } else {
        println!("✓ PII scan clean");
    }

    // Step 4: Authorization check
    let can_write = claims.permissions.contains(&"write".to_string());
    assert!(can_write, "User should have write permission");

    // Step 5: Create memory with security metadata
    let memory = env
        .repository
        .create_memory(CreateMemoryRequest {
            content: validated_content,
            embedding: None,
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.6),
            metadata: Some(json!({
                "security_test": "integrated_workflow",
                "created_by": user_id,
                "client_ip": client_ip.to_string(),
                "pii_detected": pii_result.requires_action,
                "timestamp": Utc::now().to_rfc3339()
            })),
            parent_id: None,
            expires_at: None,
        })
        .await?;

    println!("✓ Memory created successfully: {}", memory.id);

    // Step 6: Verify with read operation
    let retrieved = env.repository.get_memory(memory.id).await?;
    assert_eq!(retrieved.id, memory.id);
    println!("✓ Memory retrieval verified");

    // Step 7: Search operation with security checks
    let search_query = "technical documentation";
    let validated_search = validator.validate_input(search_query)?;

    let search_request = create_search_request(&validated_search, Some(5), None);
    let search_results = env.repository.search_memories(search_request).await?;

    println!(
        "✓ Secure search completed: {} results",
        search_results.results.len()
    );

    // Cleanup
    env.repository.delete_memory(memory.id).await?;

    println!("✓ Integrated security workflow completed successfully");

    env.cleanup_test_data().await?;
    Ok(())
}
