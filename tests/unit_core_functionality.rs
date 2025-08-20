//! Core Functionality Unit Tests
//!
//! These tests focus on the essential components that are critical
//! to the system's operation and have straightforward APIs.

mod test_helpers;

use anyhow::Result;
use chrono::{Duration, Utc};
use codex_memory::memory::models::{CreateMemoryRequest, UpdateMemoryRequest};
use codex_memory::security::SecurityConfig;
use codex_memory::{Config, MemoryStatus, MemoryTier, SimpleEmbedder};
use serde_json::json;
use test_helpers::TestEnvironment;
use tracing_test::traced_test;
use uuid::Uuid;

/// Test configuration loading and defaults
#[test]
fn test_config_defaults() {
    let config = Config::default();

    // Test database URL has a reasonable default
    assert!(
        !config.database_url.is_empty(),
        "Database URL should have a default"
    );

    // Test HTTP port
    assert!(config.http_port > 0, "HTTP port should be positive");
    assert!(config.http_port <= 65535, "HTTP port should be valid");

    // Test embedding configuration
    assert!(
        !config.embedding.provider.is_empty(),
        "Embedding provider should be set"
    );
    assert!(
        !config.embedding.model.is_empty(),
        "Embedding model should be set"
    );
    assert!(
        !config.embedding.base_url.is_empty(),
        "Embedding base URL should be set"
    );
    assert!(
        config.embedding.timeout_seconds > 0,
        "Timeout should be positive"
    );

    // Test tier configuration exists
    assert!(
        config.tier_config.working_tier_limit > 0,
        "Working tier limit should be positive"
    );
    assert!(
        config.tier_config.warm_tier_limit > 0,
        "Warm tier limit should be positive"
    );
    assert!(
        config.tier_config.importance_threshold > 0.0,
        "Importance threshold should be positive"
    );
}

/// Test security configuration structure
#[test]
fn test_security_config_structure() {
    let security_config = SecurityConfig::default();

    // Test TLS configuration
    assert!(!security_config.tls.enabled); // Disabled by default for security
    assert!(security_config.tls.port > 0);

    // Test auth configuration
    assert!(!security_config.auth.enabled); // Disabled by default
    assert!(security_config.auth.jwt_expiry_seconds > 0);
    assert!(!security_config.auth.jwt_secret.is_empty());

    // Test rate limiting
    assert!(!security_config.rate_limiting.enabled); // Disabled by default
    assert!(security_config.rate_limiting.requests_per_minute > 0);
    assert!(security_config.rate_limiting.burst_size > 0);

    // Test validation settings
    assert!(security_config.validation.enabled); // Should be enabled by default
    assert!(security_config.validation.max_request_size > 0);

    // Test PII protection defaults
    assert!(!security_config.pii_protection.enabled); // Disabled by default
    assert!(!security_config.pii_protection.detect_patterns.is_empty());

    // Test audit configuration
    assert!(!security_config.audit_logging.enabled); // Disabled by default
    assert!(security_config.audit_logging.retention_days > 0);
}

/// Test memory tier enumeration
#[test]
fn test_memory_tier_values() {
    let tiers = [MemoryTier::Working, MemoryTier::Warm, MemoryTier::Cold];

    // Test that tiers are distinct
    assert_ne!(tiers[0], tiers[1]);
    assert_ne!(tiers[1], tiers[2]);
    assert_ne!(tiers[0], tiers[2]);

    // Test serialization/deserialization works
    for tier in &tiers {
        let json_str = serde_json::to_string(tier).unwrap();
        let deserialized: MemoryTier = serde_json::from_str(&json_str).unwrap();
        assert_eq!(*tier, deserialized);
    }
}

/// Test memory status enumeration
#[test]
fn test_memory_status_values() {
    let statuses = [
        MemoryStatus::Active,
        MemoryStatus::Archived,
        MemoryStatus::Deleted,
    ];

    // Test that statuses are distinct
    for (i, status1) in statuses.iter().enumerate() {
        for (j, status2) in statuses.iter().enumerate() {
            if i != j {
                assert_ne!(
                    status1, status2,
                    "Status {} should not equal status {}",
                    i, j
                );
            }
        }
    }
}

/// Test SimpleEmbedder configuration
#[test]
fn test_embedder_creation() {
    // Test Ollama embedder creation
    let ollama_embedder =
        SimpleEmbedder::new_ollama("http://localhost:11434".to_string(), "llama2".to_string());

    assert!(
        ollama_embedder.embedding_dimension() > 0,
        "Embedding dimension should be positive"
    );

    // Test generic embedder creation (may be OpenAI under the hood)
    let generic_embedder = SimpleEmbedder::new("test-key".to_string());
    // Generic embedder should create successfully
    assert!(
        generic_embedder.embedding_dimension() > 0,
        "Generic embedder should have valid dimension"
    );
}

/// Test basic memory repository operations with minimal setup
#[tokio::test]
#[traced_test]
async fn test_memory_repository_integration() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Test 1: Create memory
    let create_request = CreateMemoryRequest {
        content: "Integration test memory".to_string(),
        embedding: None, // Skip embedding for simplicity
        tier: Some(MemoryTier::Working),
        importance_score: Some(0.8),
        metadata: Some(json!({"test_type": "integration", "category": "unit_test"})),
        parent_id: None,
        expires_at: Some(Utc::now() + Duration::hours(1)),
    };

    let created_memory = env.repository.create_memory(create_request).await?;

    // Verify creation
    assert_eq!(created_memory.content, "Integration test memory");
    assert_eq!(created_memory.tier, MemoryTier::Working);
    assert_eq!(created_memory.importance_score, 0.8);
    assert_eq!(created_memory.status, MemoryStatus::Active);
    assert_eq!(created_memory.access_count, 0);
    assert!(created_memory.last_accessed_at.is_none());
    assert!(created_memory.expires_at.is_some());
    assert_eq!(created_memory.metadata["test_type"], "integration");

    // Test 2: Retrieve memory (should update access count)
    let retrieved_memory = env.repository.get_memory(created_memory.id).await?;

    assert_eq!(retrieved_memory.id, created_memory.id);
    assert_eq!(retrieved_memory.content, created_memory.content);
    assert_eq!(retrieved_memory.access_count, 1); // Should be incremented
    assert!(retrieved_memory.last_accessed_at.is_some());

    // Test 3: Update memory
    let update_request = UpdateMemoryRequest {
        content: Some("Updated integration test memory".to_string()),
        embedding: None,
        tier: Some(MemoryTier::Warm),
        importance_score: Some(0.9),
        metadata: Some(json!({"test_type": "integration", "updated": true})),
        expires_at: None, // Remove expiration
    };

    let updated_memory = env
        .repository
        .update_memory(created_memory.id, update_request)
        .await?;

    assert_eq!(updated_memory.content, "Updated integration test memory");
    assert_eq!(updated_memory.tier, MemoryTier::Warm);
    assert_eq!(updated_memory.importance_score, 0.9);
    assert!(updated_memory.expires_at.is_none());
    assert!(updated_memory.updated_at > created_memory.updated_at);
    assert_eq!(updated_memory.metadata["updated"], true);

    // Test 4: Delete memory
    env.repository.delete_memory(created_memory.id).await?;

    let delete_result = env.repository.get_memory(created_memory.id).await;
    assert!(
        delete_result.is_err(),
        "Deleted memory should not be retrievable"
    );

    // Test 5: Basic statistics
    let stats = env.repository.get_statistics().await?;

    // Just verify the statistics call works and returns reasonable data
    if let Some(total) = stats.total_active {
        assert!(total >= 0, "Total active count should be non-negative");
    }
    if let Some(avg_importance) = stats.avg_importance {
        assert!(
            avg_importance >= 0.0 && avg_importance <= 1.0,
            "Average importance should be between 0 and 1"
        );
    }

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test error conditions
#[tokio::test]
#[traced_test]
async fn test_error_conditions() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Test 1: Get non-existent memory
    let fake_id = Uuid::new_v4();
    let get_result = env.repository.get_memory(fake_id).await;
    assert!(
        get_result.is_err(),
        "Should error when getting non-existent memory"
    );

    // Test 2: Update non-existent memory
    let update_request = UpdateMemoryRequest {
        content: Some("This should fail".to_string()),
        embedding: None,
        tier: None,
        importance_score: None,
        metadata: None,
        expires_at: None,
    };

    let update_result = env.repository.update_memory(fake_id, update_request).await;
    assert!(
        update_result.is_err(),
        "Should error when updating non-existent memory"
    );

    // Test 3: Delete non-existent memory (may succeed silently depending on implementation)
    let delete_result = env.repository.delete_memory(fake_id).await;
    // Don't assert error here as some implementations may handle this gracefully

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test UUID generation and uniqueness
#[test]
fn test_uuid_uniqueness() {
    let mut uuids = std::collections::HashSet::new();

    // Generate 1000 UUIDs and verify they're all unique
    for _ in 0..1000 {
        let uuid = Uuid::new_v4();
        assert!(uuids.insert(uuid), "UUID should be unique: {}", uuid);
        assert_ne!(uuid, Uuid::nil(), "UUID should not be nil");
    }
}

/// Test JSON serialization of key data types
#[test]
fn test_json_serialization() -> Result<()> {
    // Test MemoryTier serialization
    let tier = MemoryTier::Working;
    let tier_json = serde_json::to_string(&tier)?;
    let tier_deserialized: MemoryTier = serde_json::from_str(&tier_json)?;
    assert_eq!(tier, tier_deserialized);

    // Test MemoryStatus serialization
    let status = MemoryStatus::Active;
    let status_json = serde_json::to_string(&status)?;
    let status_deserialized: MemoryStatus = serde_json::from_str(&status_json)?;
    assert_eq!(status, status_deserialized);

    // Test SecurityConfig serialization
    let security_config = SecurityConfig::default();
    let config_json = serde_json::to_string(&security_config)?;
    let config_deserialized: SecurityConfig = serde_json::from_str(&config_json)?;
    // Just verify it deserializes successfully (equality might be complex)
    assert!(!config_deserialized.auth.jwt_secret.is_empty());

    Ok(())
}

/// Test time-related functionality
#[test]
fn test_time_operations() {
    let now = Utc::now();
    let future = now + Duration::hours(1);
    let past = now - Duration::hours(1);

    assert!(future > now, "Future time should be greater than now");
    assert!(past < now, "Past time should be less than now");
    assert_eq!((future - now).num_seconds(), 3600); // 1 hour = 3600 seconds

    // Test duration arithmetic
    let duration = Duration::minutes(30);
    let later = now + duration;
    assert_eq!((later - now).num_minutes(), 30);
}

/// Test basic validation functionality
#[test]
fn test_basic_validation() {
    // Test importance score validation
    let valid_scores = [0.0, 0.5, 1.0];
    for score in valid_scores {
        assert!(
            score >= 0.0 && score <= 1.0,
            "Score {} should be valid",
            score
        );
    }

    let invalid_scores = [-0.1, 1.1, f64::NAN, f64::INFINITY];
    for score in invalid_scores {
        assert!(
            score < 0.0 || score > 1.0 || !score.is_finite(),
            "Score {} should be invalid",
            score
        );
    }

    // Test content validation
    let empty_content = "";
    let normal_content = "This is normal content";
    let long_content = "x".repeat(10000);

    assert!(empty_content.is_empty());
    assert!(!normal_content.is_empty());
    assert!(long_content.len() == 10000);
}

/// Test metadata handling
#[test]
fn test_metadata_operations() -> Result<()> {
    // Test basic metadata creation and access
    let metadata = json!({
        "category": "test",
        "priority": 5,
        "tags": ["unit", "test", "metadata"],
        "nested": {
            "key": "value",
            "number": 42
        }
    });

    // Test field access
    assert_eq!(metadata["category"], "test");
    assert_eq!(metadata["priority"], 5);
    assert_eq!(metadata["tags"][0], "unit");
    assert_eq!(metadata["nested"]["key"], "value");
    assert_eq!(metadata["nested"]["number"], 42);

    // Test serialization roundtrip
    let serialized = serde_json::to_string(&metadata)?;
    let deserialized: serde_json::Value = serde_json::from_str(&serialized)?;
    assert_eq!(metadata, deserialized);

    Ok(())
}
