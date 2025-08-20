//! Simplified Unit Tests for Memory Repository Layer
//!
//! These tests focus on the core CRUD operations and business logic
//! without complex dependencies or type mismatches.

mod test_helpers;

use anyhow::Result;
use chrono::{Duration, Utc};
use codex_memory::memory::models::{
    CreateMemoryRequest, MemoryStatus, MemoryTier, UpdateMemoryRequest,
};
use serde_json::json;
use test_helpers::TestEnvironment;
use tracing_test::traced_test;
use uuid::Uuid;

/// Test basic memory creation with minimal setup
#[tokio::test]
#[traced_test]
async fn test_create_memory_basic() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Test minimal memory creation
    let request = CreateMemoryRequest {
        content: "Test memory content".to_string(),
        embedding: None,
        tier: Some(MemoryTier::Working),
        importance_score: Some(0.7),
        metadata: Some(json!({"test": "create_basic"})),
        parent_id: None,
        expires_at: None,
    };

    let memory = env.repository.create_memory(request).await?;

    assert_eq!(memory.content, "Test memory content");
    assert_eq!(memory.tier, MemoryTier::Working);
    assert_eq!(memory.importance_score, 0.7);
    assert_eq!(memory.status, MemoryStatus::Active);
    assert_eq!(memory.access_count, 0);
    assert!(memory.last_accessed_at.is_none());
    assert!(!memory.metadata.is_null());

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test memory retrieval and access tracking
#[tokio::test]
#[traced_test]
async fn test_get_memory_access_tracking() -> Result<()> {
    let env = TestEnvironment::new().await?;

    let memory = env
        .repository
        .create_memory(CreateMemoryRequest {
            content: "Memory for access test".to_string(),
            embedding: None,
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.6),
            metadata: Some(json!({"test": "access_tracking"})),
            parent_id: None,
            expires_at: None,
        })
        .await?;

    let initial_access_count = memory.access_count;

    // Retrieve memory should increment access count
    let retrieved = env.repository.get_memory(memory.id).await?;
    assert_eq!(retrieved.content, memory.content);
    assert_eq!(retrieved.access_count, initial_access_count + 1);
    assert!(retrieved.last_accessed_at.is_some());

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test memory updates
#[tokio::test]
#[traced_test]
async fn test_update_memory() -> Result<()> {
    let env = TestEnvironment::new().await?;

    let memory = env
        .repository
        .create_memory(CreateMemoryRequest {
            content: "Original content".to_string(),
            embedding: None,
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.5),
            metadata: Some(json!({"original": true})),
            parent_id: None,
            expires_at: None,
        })
        .await?;

    // Update content and importance
    let update_request = UpdateMemoryRequest {
        content: Some("Updated content".to_string()),
        embedding: None,
        tier: Some(MemoryTier::Warm),
        importance_score: Some(0.8),
        metadata: Some(json!({"updated": true})),
        expires_at: Some(Utc::now() + Duration::hours(1)),
    };

    let updated = env
        .repository
        .update_memory(memory.id, update_request)
        .await?;

    assert_eq!(updated.content, "Updated content");
    assert_eq!(updated.tier, MemoryTier::Warm);
    assert_eq!(updated.importance_score, 0.8);
    assert!(updated.expires_at.is_some());
    assert!(updated.updated_at > memory.updated_at);
    assert_eq!(updated.metadata["updated"], true);

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test memory deletion
#[tokio::test]
#[traced_test]
async fn test_delete_memory() -> Result<()> {
    let env = TestEnvironment::new().await?;

    let memory = env
        .repository
        .create_memory(CreateMemoryRequest {
            content: "Memory to delete".to_string(),
            embedding: None,
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.5),
            metadata: Some(json!({"delete_test": true})),
            parent_id: None,
            expires_at: None,
        })
        .await?;

    // Delete memory
    env.repository.delete_memory(memory.id).await?;

    // Verify deletion
    let result = env.repository.get_memory(memory.id).await;
    assert!(result.is_err(), "Deleted memory should not be retrievable");

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test error handling for non-existent memory
#[tokio::test]
#[traced_test]
async fn test_nonexistent_memory() -> Result<()> {
    let env = TestEnvironment::new().await?;

    let non_existent_id = Uuid::new_v4();

    // Try to get non-existent memory
    let get_result = env.repository.get_memory(non_existent_id).await;
    assert!(
        get_result.is_err(),
        "Should fail to get non-existent memory"
    );

    // Try to update non-existent memory
    let update_result = env
        .repository
        .update_memory(
            non_existent_id,
            UpdateMemoryRequest {
                content: Some("Test".to_string()),
                embedding: None,
                tier: None,
                importance_score: None,
                metadata: None,
                expires_at: None,
            },
        )
        .await;
    assert!(
        update_result.is_err(),
        "Should fail to update non-existent memory"
    );

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test repository statistics
#[tokio::test]
#[traced_test]
async fn test_repository_statistics() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Get initial stats
    let initial_stats = env.repository.get_statistics().await?;
    let initial_count = initial_stats.total_active.unwrap_or(0);

    // Create test memories
    for i in 0..5 {
        env.repository
            .create_memory(CreateMemoryRequest {
                content: format!("Stats test memory {}", i),
                embedding: None,
                tier: Some(match i % 3 {
                    0 => MemoryTier::Working,
                    1 => MemoryTier::Warm,
                    _ => MemoryTier::Cold,
                }),
                importance_score: Some(0.1 + (i as f64 * 0.2)),
                metadata: Some(json!({"stats_test": true, "index": i})),
                parent_id: None,
                expires_at: None,
            })
            .await?;
    }

    // Get updated stats
    let updated_stats = env.repository.get_statistics().await?;
    let final_count = updated_stats.total_active.unwrap_or(0);

    // Should have increased by 5
    assert!(
        final_count >= initial_count + 5,
        "Statistics should reflect new memories"
    );

    env.cleanup_test_data().await?;
    Ok(())
}
