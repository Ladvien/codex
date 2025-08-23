//! End-to-End Complete Workflow Tests
//!
//! These tests simulate complete user workflows and system interactions,
//! validating that all components work together seamlessly from memory creation
//! through search, organization, and lifecycle management.

mod test_helpers;

use anyhow::Result;
use chrono::{Duration, Utc};
use codex_memory::memory::models::{
    CreateMemoryRequest, MemoryTier, SearchRequest, UpdateMemoryRequest,
};
use codex_memory::SimpleEmbedder;
use codex_memory::{MCPServer, MCPServerConfig};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use test_helpers::TestEnvironment;
use tokio::time::sleep;
use tracing_test::traced_test;
use uuid::Uuid;

/// Helper function to create a basic search request
fn create_search_request(
    query: &str,
    limit: Option<i32>,
    tier: Option<MemoryTier>,
    importance_min: Option<f32>,
) -> SearchRequest {
    SearchRequest {
        query_text: Some(query.to_string()),
        query_embedding: None,
        search_type: None,
        hybrid_weights: None,
        tier,
        date_range: None,
        importance_range: importance_min.map(|min| codex_memory::memory::models::RangeFilter {
            min: Some(min),
            max: None,
        }),
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

/// Test complete memory lifecycle workflow
#[tokio::test]
#[traced_test]
async fn test_complete_memory_lifecycle_workflow() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Step 1: Create initial memory in working tier
    let create_request = CreateMemoryRequest {
        content: "This is a comprehensive test of the memory system lifecycle".to_string(),
        embedding: None, // Let system generate embedding
        tier: Some(MemoryTier::Working),
        importance_score: Some(0.8),
        metadata: Some(json!({
            "workflow": "complete_lifecycle",
            "stage": "creation",
            "test_id": "lifecycle_001",
            "tags": ["important", "test", "lifecycle"]
        })),
        parent_id: None,
        expires_at: Some(Utc::now() + Duration::hours(24)),
    };

    let memory = env.repository.create_memory(create_request).await?;
    assert_eq!(memory.tier, MemoryTier::Working);
    assert_eq!(memory.importance_score, 0.8);
    assert!(memory.expires_at.is_some());

    // Step 2: Access the memory multiple times to increase access count
    for i in 1..=5 {
        let accessed_memory = env.repository.get_memory(memory.id).await?;
        assert_eq!(accessed_memory.access_count, i);
        assert!(accessed_memory.last_accessed_at.is_some());

        // Small delay between accesses
        sleep(std::time::Duration::from_millis(100)).await;
    }

    // Step 3: Update memory content and metadata
    let update_request = UpdateMemoryRequest {
        content: Some("Updated lifecycle test content with additional information".to_string()),
        embedding: None,
        tier: Some(MemoryTier::Warm), // Move to warm tier
        importance_score: Some(0.9),  // Increase importance
        metadata: Some(json!({
            "workflow": "complete_lifecycle",
            "stage": "updated",
            "test_id": "lifecycle_001",
            "tags": ["important", "test", "lifecycle", "updated"],
            "update_count": 1
        })),
        expires_at: None, // Remove expiration
    };

    let updated_memory = env
        .repository
        .update_memory(memory.id, update_request)
        .await?;
    assert_eq!(updated_memory.tier, MemoryTier::Warm);
    assert_eq!(updated_memory.importance_score, 0.9);
    assert!(updated_memory.expires_at.is_none());
    assert!(updated_memory.content.contains("Updated lifecycle"));

    // Step 4: Create related memories with parent-child relationships
    let child_memory = env
        .repository
        .create_memory(CreateMemoryRequest {
            content: "Child memory related to the main lifecycle test".to_string(),
            embedding: None,
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.6),
            metadata: Some(json!({
                "workflow": "complete_lifecycle",
                "stage": "child_creation",
                "parent_test_id": "lifecycle_001",
                "relationship": "child"
            })),
            parent_id: Some(memory.id),
            expires_at: None,
        })
        .await?;

    assert_eq!(child_memory.parent_id, Some(memory.id));

    // Step 5: Search for related memories
    let search_request = create_search_request("lifecycle test", Some(50), None, None);
    let search_results = env.repository.search_memories(search_request).await?;

    assert!(
        search_results.results.len() >= 2,
        "Should find both parent and child memories"
    );
    let found_ids: Vec<Uuid> = search_results.results.iter().map(|r| r.memory.id).collect();
    assert!(found_ids.contains(&memory.id));
    assert!(found_ids.contains(&child_memory.id));

    // Step 6: Test statistics reflect the changes
    let stats = env.repository.get_statistics().await?;
    if let Some(total) = stats.total_active {
        assert!(total >= 2, "Should have at least our test memories");
    }

    // Step 7: Clean up - delete child first, then parent
    env.repository.delete_memory(child_memory.id).await?;
    env.repository.delete_memory(memory.id).await?;

    // Verify deletion
    let deleted_parent = env.repository.get_memory(memory.id).await;
    let deleted_child = env.repository.get_memory(child_memory.id).await;
    assert!(deleted_parent.is_err());
    assert!(deleted_child.is_err());

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test hierarchical memory organization workflow
#[tokio::test]
#[traced_test]
async fn test_hierarchical_memory_organization_workflow() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Create a hierarchical structure: Root -> Category -> Items

    // Root memory: Project overview
    let root_memory = env
        .repository
        .create_memory(CreateMemoryRequest {
            content: "Software Development Project: Advanced Memory System".to_string(),
            embedding: None,
            tier: Some(MemoryTier::Working),
            importance_score: Some(1.0),
            metadata: Some(json!({
                "type": "project",
                "level": "root",
                "hierarchy_test": true
            })),
            parent_id: None,
            expires_at: None,
        })
        .await?;

    // Category memories: Different aspects of the project
    let mut category_memories = Vec::new();
    let categories = vec![
        ("Architecture", "System architecture and design patterns"),
        ("Testing", "Test strategies and implementation"),
        ("Performance", "Performance optimization and benchmarking"),
        ("Security", "Security measures and authentication"),
    ];

    for (category, description) in categories {
        let category_memory = env
            .repository
            .create_memory(CreateMemoryRequest {
                content: format!("{}: {}", category, description),
                embedding: None,
                tier: Some(MemoryTier::Working),
                importance_score: Some(0.8),
                metadata: Some(json!({
                    "type": "category",
                    "level": "category",
                    "category": category,
                    "hierarchy_test": true
                })),
                parent_id: Some(root_memory.id),
                expires_at: None,
            })
            .await?;
        category_memories.push((category, category_memory));
    }

    // Item memories: Specific details under each category
    let mut all_item_memories = Vec::new();

    for (category_name, category_memory) in &category_memories {
        let items = match category_name.as_ref() {
            "Architecture" => vec![
                "Database schema design with PostgreSQL and pgvector",
                "Memory tier management system",
                "Embedding service integration patterns",
            ],
            "Testing" => vec![
                "Unit test implementation for core components",
                "Integration testing strategy",
                "End-to-end workflow validation",
            ],
            "Performance" => vec![
                "Query optimization techniques",
                "Connection pool management",
                "Memory usage profiling",
            ],
            "Security" => vec![
                "JWT authentication implementation",
                "PII detection and masking",
                "Rate limiting strategies",
            ],
            _ => vec![],
        };

        for item_content in items {
            let item_memory = env
                .repository
                .create_memory(CreateMemoryRequest {
                    content: item_content.to_string(),
                    embedding: None,
                    tier: Some(MemoryTier::Working),
                    importance_score: Some(0.6),
                    metadata: Some(json!({
                        "type": "item",
                        "level": "item",
                        "category": category_name,
                        "hierarchy_test": true
                    })),
                    parent_id: Some(category_memory.id),
                    expires_at: None,
                })
                .await?;
            all_item_memories.push(item_memory);
        }
    }

    // Test hierarchical queries and relationships

    // Search within specific categories
    let architecture_search = create_search_request("database schema", Some(10), None, None);
    let architecture_results = env.repository.search_memories(architecture_search).await?;

    assert!(
        !architecture_results.results.is_empty(),
        "Should find architecture-related memories"
    );

    // Search for testing-related content
    let testing_search = create_search_request("unit test", Some(10), None, None);
    let testing_results = env.repository.search_memories(testing_search).await?;

    assert!(
        !testing_results.results.is_empty(),
        "Should find testing-related memories"
    );

    // Verify hierarchical structure integrity
    let total_memories = 1 + category_memories.len() + all_item_memories.len(); // root + categories + items

    let all_hierarchy_search = create_search_request("hierarchy_test", Some(50), None, None);
    let all_hierarchy_memories = env.repository.search_memories(all_hierarchy_search).await?;

    // Should find all memories in our hierarchy
    assert!(
        all_hierarchy_memories.results.len() >= total_memories,
        "Should find all hierarchical memories"
    );

    // Test tier migration simulation
    // Move some memories to different tiers based on importance
    for item_memory in &all_item_memories[..2] {
        // Move first 2 items to warm tier
        env.repository
            .update_memory(
                item_memory.id,
                UpdateMemoryRequest {
                    content: None,
                    embedding: None,
                    tier: Some(MemoryTier::Warm),
                    importance_score: None,
                    metadata: None,
                    expires_at: None,
                },
            )
            .await?;
    }

    // Verify tier changes
    let updated_item = env.repository.get_memory(all_item_memories[0].id).await?;
    assert_eq!(updated_item.tier, MemoryTier::Warm);

    // Cleanup hierarchy (delete in reverse order: items -> categories -> root)
    for item_memory in all_item_memories {
        env.repository.delete_memory(item_memory.id).await?;
    }

    for (_, category_memory) in category_memories {
        env.repository.delete_memory(category_memory.id).await?;
    }

    env.repository.delete_memory(root_memory.id).await?;

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test search and discovery workflow
#[tokio::test]
#[traced_test]
async fn test_search_and_discovery_workflow() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Create a diverse set of memories for search testing
    let test_memories = vec![
        (
            "Rust programming language features",
            vec!["rust", "programming", "systems"],
            MemoryTier::Working,
            0.9,
        ),
        (
            "PostgreSQL database optimization",
            vec!["database", "postgresql", "performance"],
            MemoryTier::Working,
            0.8,
        ),
        (
            "Machine learning algorithms",
            vec!["ai", "ml", "algorithms"],
            MemoryTier::Warm,
            0.7,
        ),
        (
            "Web security best practices",
            vec!["security", "web", "authentication"],
            MemoryTier::Working,
            0.9,
        ),
        (
            "Docker containerization guide",
            vec!["docker", "containers", "devops"],
            MemoryTier::Warm,
            0.6,
        ),
        (
            "API design principles",
            vec!["api", "design", "rest"],
            MemoryTier::Working,
            0.8,
        ),
        (
            "Testing strategies overview",
            vec!["testing", "qa", "automation"],
            MemoryTier::Working,
            0.7,
        ),
        (
            "Performance monitoring tools",
            vec!["monitoring", "performance", "observability"],
            MemoryTier::Warm,
            0.6,
        ),
    ];

    let mut created_memories = Vec::new();

    for (content, tags, tier, importance) in test_memories {
        let memory = env
            .repository
            .create_memory(CreateMemoryRequest {
                content: content.to_string(),
                embedding: None,
                tier: Some(tier),
                importance_score: Some(importance),
                metadata: Some(json!({
                    "tags": tags,
                    "search_test": true,
                    "content_type": "educational"
                })),
                parent_id: None,
                expires_at: None,
            })
            .await?;
        created_memories.push(memory);

        // Small delay to ensure different timestamps
        sleep(std::time::Duration::from_millis(50)).await;
    }

    // Test various search scenarios

    // 1. Keyword search
    let programming_search = create_search_request("programming", Some(10), None, None);
    let programming_results = env.repository.search_memories(programming_search).await?;

    assert!(
        !programming_results.results.is_empty(),
        "Should find programming-related memories"
    );

    // 2. Technology-specific search
    let rust_search = create_search_request("rust", Some(10), None, None);
    let rust_results = env.repository.search_memories(rust_search).await?;

    assert!(
        !rust_results.results.is_empty(),
        "Should find Rust-related memories"
    );

    // 3. Tier-specific search
    let working_tier_search =
        create_search_request("test", Some(20), Some(MemoryTier::Working), None);
    let working_tier_results = env.repository.search_memories(working_tier_search).await?;

    let warm_tier_search = create_search_request("test", Some(20), Some(MemoryTier::Warm), None);
    let warm_tier_results = env.repository.search_memories(warm_tier_search).await?;

    assert!(
        !working_tier_results.results.is_empty(),
        "Should find memories in working tier"
    );
    assert!(
        !warm_tier_results.results.is_empty(),
        "Should find memories in warm tier"
    );

    // 4. Importance-based search (simulate filtering)
    let all_search = create_search_request("search_test", Some(50), None, None);
    let all_results = env.repository.search_memories(all_search).await?;

    let high_importance_count = all_results
        .results
        .iter()
        .filter(|r| r.memory.importance_score >= 0.8)
        .count();
    let medium_importance_count = all_results
        .results
        .iter()
        .filter(|r| r.memory.importance_score >= 0.6 && r.memory.importance_score < 0.8)
        .count();

    assert!(
        high_importance_count > 0,
        "Should have high importance memories"
    );
    assert!(
        medium_importance_count > 0,
        "Should have medium importance memories"
    );

    // 5. Test search result ordering (by relevance/importance)
    let ordered_search = create_search_request("performance", Some(10), None, None);
    let ordered_results = env.repository.search_memories(ordered_search).await?;

    // Verify we get results
    assert!(
        !ordered_results.results.is_empty(),
        "Should find performance-related memories"
    );

    // 6. Test pagination-like behavior with limits
    let limited_search = create_search_request("test", Some(3), None, None);
    let limited_results = env.repository.search_memories(limited_search).await?;

    assert!(
        limited_results.results.len() <= 3,
        "Should respect limit parameter"
    );

    // Test memory access patterns during search
    let initial_access_counts: HashMap<Uuid, i32> = created_memories
        .iter()
        .map(|m| (m.id, m.access_count))
        .collect();

    // Access some memories through get_memory (simulating user interaction)
    for memory in &created_memories[..3] {
        let _ = env.repository.get_memory(memory.id).await?;
    }

    // Verify access counts increased
    for memory in &created_memories[..3] {
        let updated = env.repository.get_memory(memory.id).await?;
        let initial_count = initial_access_counts.get(&memory.id).unwrap_or(&0);
        assert!(
            updated.access_count > *initial_count,
            "Access count should increase after retrieval"
        );
    }

    // Cleanup
    for memory in created_memories {
        env.repository.delete_memory(memory.id).await?;
    }

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test MCP server workflow integration
#[tokio::test]
#[traced_test]
async fn test_mcp_server_workflow_integration() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Create embedder and MCP server
    let embedder = Arc::new(SimpleEmbedder::new_ollama(
        "http://localhost:11434".to_string(),
        "llama2".to_string(),
    ));
    let _mcp_server = MCPServer::new(env.repository.clone(), embedder, MCPServerConfig::default())?;

    // Test workflow through repository (simulating MCP operations)

    // 1. Create multiple memories as if through MCP tools
    let workflow_memories = vec![
        ("User asks about Rust programming", "question", 0.8),
        ("System provides Rust documentation", "response", 0.7),
        ("User requests code examples", "question", 0.9),
        ("System shares Rust code snippets", "response", 0.8),
        ("User saves important concepts", "note", 0.9),
    ];

    let mut conversation_memories = Vec::new();
    let mut previous_memory_id = None;

    for (content, message_type, importance) in workflow_memories {
        let memory = env
            .repository
            .create_memory(CreateMemoryRequest {
                content: content.to_string(),
                embedding: None,
                tier: Some(MemoryTier::Working),
                importance_score: Some(importance),
                metadata: Some(json!({
                    "message_type": message_type,
                    "conversation_id": "rust_help_001",
                    "mcp_workflow": true,
                    "timestamp": Utc::now().to_rfc3339()
                })),
                parent_id: previous_memory_id,
                expires_at: None,
            })
            .await?;

        conversation_memories.push(memory.clone());
        previous_memory_id = Some(memory.id);

        // Simulate processing time
        sleep(std::time::Duration::from_millis(100)).await;
    }

    // 2. Test conversation retrieval and context building
    let conversation_search_req = create_search_request("rust_help_001", Some(20), None, None);
    let conversation_search = env
        .repository
        .search_memories(conversation_search_req)
        .await?;

    assert_eq!(
        conversation_search.results.len(),
        5,
        "Should find all conversation memories"
    );

    // 3. Test memory importance-based organization
    let important_search = create_search_request("mcp_workflow", Some(20), None, Some(0.85));
    let important_memories = env.repository.search_memories(important_search).await?;

    let important_count = conversation_memories
        .iter()
        .filter(|m| m.importance_score >= 0.85)
        .count();

    // Should find memories with high importance scores
    assert!(
        important_memories.results.len() >= important_count,
        "Should find high-importance memories"
    );

    // 4. Test memory updates (simulating user feedback)
    let first_memory = &conversation_memories[0];
    let updated_memory = env
        .repository
        .update_memory(
            first_memory.id,
            UpdateMemoryRequest {
                content: None,
                embedding: None,
                tier: None,
                importance_score: Some(1.0), // Increase importance
                metadata: Some(json!({
                    "message_type": "question",
                    "conversation_id": "rust_help_001",
                    "mcp_workflow": true,
                    "timestamp": Utc::now().to_rfc3339(),
                    "user_feedback": "helpful",
                    "updated": true
                })),
                expires_at: None,
            },
        )
        .await?;

    assert_eq!(updated_memory.importance_score, 1.0);
    assert_eq!(updated_memory.metadata["updated"], true);

    // 5. Test memory statistics for the workflow
    let stats = env.repository.get_statistics().await?;
    if let Some(total) = stats.total_active {
        assert!(total >= 5, "Should include our workflow memories");
    }

    // 6. Simulate memory tier migration based on usage
    // Move older, less important memories to warm tier
    let older_memories = &conversation_memories[1..3]; // Skip first (updated) and last (recent)

    for memory in older_memories {
        env.repository
            .update_memory(
                memory.id,
                UpdateMemoryRequest {
                    content: None,
                    embedding: None,
                    tier: Some(MemoryTier::Warm),
                    importance_score: None,
                    metadata: None,
                    expires_at: None,
                },
            )
            .await?;
    }

    // Verify tier changes
    for memory in older_memories {
        let updated = env.repository.get_memory(memory.id).await?;
        assert_eq!(updated.tier, MemoryTier::Warm);
    }

    // 7. Test final conversation summary creation
    let summary_content = format!(
        "Conversation Summary: User assistance with Rust programming. {} messages exchanged.",
        conversation_memories.len()
    );

    let summary_memory = env
        .repository
        .create_memory(CreateMemoryRequest {
            content: summary_content,
            embedding: None,
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.95),
            metadata: Some(json!({
                "message_type": "summary",
                "conversation_id": "rust_help_001",
                "mcp_workflow": true,
                "summary": true,
                "message_count": conversation_memories.len()
            })),
            parent_id: None,
            expires_at: None,
        })
        .await?;

    assert!(summary_memory.content.contains("Conversation Summary"));

    // Cleanup
    env.repository.delete_memory(summary_memory.id).await?;
    for memory in conversation_memories {
        env.repository.delete_memory(memory.id).await?;
    }

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test multi-tier memory management workflow
#[tokio::test]
#[traced_test]
async fn test_multi_tier_memory_management_workflow() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Create memories in different tiers to simulate a mature system
    let mut tier_memories: HashMap<MemoryTier, Vec<Uuid>> = HashMap::new();
    tier_memories.insert(MemoryTier::Working, Vec::new());
    tier_memories.insert(MemoryTier::Warm, Vec::new());
    tier_memories.insert(MemoryTier::Cold, Vec::new());

    // Working tier: Recent, highly accessed memories
    for i in 0..5 {
        let memory = env
            .repository
            .create_memory(CreateMemoryRequest {
                content: format!(
                    "Active work item {}: Current project tasks and immediate needs",
                    i
                ),
                embedding: None,
                tier: Some(MemoryTier::Working),
                importance_score: Some(0.8 + (i as f64 * 0.04)), // 0.8 to 0.96
                metadata: Some(json!({
                    "tier_test": true,
                    "category": "working",
                    "activity_level": "high",
                    "index": i
                })),
                parent_id: None,
                expires_at: Some(Utc::now() + Duration::hours(48)),
            })
            .await?;

        tier_memories
            .get_mut(&MemoryTier::Working)
            .unwrap()
            .push(memory.id);
    }

    // Warm tier: Moderately important, less frequently accessed
    for i in 0..8 {
        let memory = env
            .repository
            .create_memory(CreateMemoryRequest {
                content: format!(
                    "Reference material {}: Documentation and guides for occasional reference",
                    i
                ),
                embedding: None,
                tier: Some(MemoryTier::Warm),
                importance_score: Some(0.5 + (i as f64 * 0.03)), // 0.5 to 0.71
                metadata: Some(json!({
                    "tier_test": true,
                    "category": "warm",
                    "activity_level": "medium",
                    "index": i
                })),
                parent_id: None,
                expires_at: Some(Utc::now() + Duration::days(30)),
            })
            .await?;

        tier_memories
            .get_mut(&MemoryTier::Warm)
            .unwrap()
            .push(memory.id);
    }

    // Cold tier: Archive data, rarely accessed
    for i in 0..12 {
        let memory = env
            .repository
            .create_memory(CreateMemoryRequest {
                content: format!(
                    "Archived data {}: Historical information and completed project notes",
                    i
                ),
                embedding: None,
                tier: Some(MemoryTier::Cold),
                importance_score: Some(0.2 + (i as f64 * 0.02)), // 0.2 to 0.42
                metadata: Some(json!({
                    "tier_test": true,
                    "category": "cold",
                    "activity_level": "low",
                    "index": i
                })),
                parent_id: None,
                expires_at: None, // No expiration for archived data
            })
            .await?;

        tier_memories
            .get_mut(&MemoryTier::Cold)
            .unwrap()
            .push(memory.id);
    }

    // Test tier-specific operations

    // 1. Verify tier distribution
    for tier in [MemoryTier::Working, MemoryTier::Warm, MemoryTier::Cold] {
        let tier_search = create_search_request("tier_test", Some(50), Some(tier), None);
        let tier_results = env.repository.search_memories(tier_search).await?;

        let expected_count = tier_memories.get(&tier).unwrap().len();
        let actual_count = tier_results
            .results
            .iter()
            .filter(|r| r.memory.tier == tier)
            .count();

        assert!(
            actual_count >= expected_count,
            "Should find expected number of memories in {:?} tier",
            tier
        );
    }

    // 2. Test access pattern simulation
    // Heavily access working tier memories
    for memory_id in tier_memories.get(&MemoryTier::Working).unwrap() {
        for _ in 0..5 {
            let _ = env.repository.get_memory(*memory_id).await?;
        }
    }

    // Moderately access warm tier memories
    for memory_id in tier_memories.get(&MemoryTier::Warm).unwrap().iter().take(3) {
        for _ in 0..2 {
            let _ = env.repository.get_memory(*memory_id).await?;
        }
    }

    // Rarely access cold tier memories
    let cold_memory_id = tier_memories.get(&MemoryTier::Cold).unwrap()[0];
    let _ = env.repository.get_memory(cold_memory_id).await?;

    // 3. Verify access patterns
    let working_memory = env
        .repository
        .get_memory(tier_memories.get(&MemoryTier::Working).unwrap()[0])
        .await?;
    assert!(
        working_memory.access_count >= 5,
        "Working memory should have high access count"
    );

    let cold_memory = env.repository.get_memory(cold_memory_id).await?;
    assert_eq!(
        cold_memory.access_count, 1,
        "Cold memory should have low access count"
    );

    // 4. Test tier migration simulation
    // Promote a warm tier memory to working tier (simulating increased importance)
    let promoted_memory_id = tier_memories.get(&MemoryTier::Warm).unwrap()[0];
    let promoted = env
        .repository
        .update_memory(
            promoted_memory_id,
            UpdateMemoryRequest {
                content: None,
                embedding: None,
                tier: Some(MemoryTier::Working),
                importance_score: Some(0.85), // Increase importance
                metadata: Some(json!({
                    "tier_test": true,
                    "category": "promoted",
                    "activity_level": "high",
                    "migration": "warm_to_working"
                })),
                expires_at: Some(Utc::now() + Duration::hours(48)),
            },
        )
        .await?;

    assert_eq!(promoted.tier, MemoryTier::Working);
    assert_eq!(promoted.importance_score, 0.85);

    // Demote a working tier memory to warm tier (simulating decreased relevance)
    let demoted_memory_id = tier_memories.get(&MemoryTier::Working).unwrap()[4]; // Last one
    let demoted = env
        .repository
        .update_memory(
            demoted_memory_id,
            UpdateMemoryRequest {
                content: None,
                embedding: None,
                tier: Some(MemoryTier::Warm),
                importance_score: Some(0.6), // Decrease importance
                metadata: Some(json!({
                    "tier_test": true,
                    "category": "demoted",
                    "activity_level": "medium",
                    "migration": "working_to_warm"
                })),
                expires_at: Some(Utc::now() + Duration::days(14)),
            },
        )
        .await?;

    assert_eq!(demoted.tier, MemoryTier::Warm);
    assert_eq!(demoted.importance_score, 0.6);

    // 5. Test comprehensive search across all tiers
    let all_tier_search = create_search_request("tier_test", Some(50), None, None);
    let all_tier_results = env.repository.search_memories(all_tier_search).await?;

    let total_expected = tier_memories.values().map(|v| v.len()).sum::<usize>();
    assert!(
        all_tier_results.results.len() >= total_expected,
        "Should find memories across all tiers"
    );

    // 6. Test importance-based filtering across tiers
    let high_importance_search = create_search_request("tier_test", Some(50), None, Some(0.8));
    let high_importance_results = env
        .repository
        .search_memories(high_importance_search)
        .await?;

    // Should primarily find working tier memories (higher importance)
    let working_tier_count = high_importance_results
        .results
        .iter()
        .filter(|r| r.memory.tier == MemoryTier::Working)
        .count();

    assert!(
        working_tier_count > 0,
        "Should find high-importance working tier memories"
    );

    // Cleanup all memories
    for (_, memory_ids) in tier_memories {
        for memory_id in memory_ids {
            let _ = env.repository.delete_memory(memory_id).await; // Use _ to ignore errors during cleanup
        }
    }

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test error handling and recovery workflows
#[tokio::test]
#[traced_test]
async fn test_error_handling_and_recovery_workflow() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Test various error scenarios and recovery mechanisms

    // 1. Test duplicate ID handling (shouldn't be possible with UUIDs, but test the system)
    let test_memory = env
        .repository
        .create_memory(CreateMemoryRequest {
            content: "Error handling test memory".to_string(),
            embedding: None,
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.7),
            metadata: Some(json!({"error_test": true})),
            parent_id: None,
            expires_at: None,
        })
        .await?;

    // 2. Test operations on non-existent memory IDs
    let fake_id = Uuid::new_v4();

    let get_result = env.repository.get_memory(fake_id).await;
    assert!(
        get_result.is_err(),
        "Should fail to get non-existent memory"
    );

    let update_result = env
        .repository
        .update_memory(
            fake_id,
            UpdateMemoryRequest {
                content: Some("This should fail".to_string()),
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

    let _delete_result = env.repository.delete_memory(fake_id).await;
    // Delete might succeed silently depending on implementation

    // 3. Test invalid parent ID references
    let another_fake_id = Uuid::new_v4();
    let invalid_parent_result = env
        .repository
        .create_memory(CreateMemoryRequest {
            content: "Child with invalid parent".to_string(),
            embedding: None,
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.5),
            metadata: Some(json!({"error_test": true, "invalid_parent": true})),
            parent_id: Some(another_fake_id),
            expires_at: None,
        })
        .await;

    // This may succeed or fail depending on foreign key constraints
    match invalid_parent_result {
        Ok(memory) => {
            // If it succeeds, clean it up
            let _ = env.repository.delete_memory(memory.id).await;
        }
        Err(_) => {
            // Expected behavior with foreign key constraints
        }
    }

    // 4. Test search with various edge cases
    let empty_search_req = create_search_request("", Some(10), None, None);
    let _empty_search = env.repository.search_memories(empty_search_req).await;
    // Should either return empty results or handle gracefully

    let very_long_search_req = create_search_request(&"a".repeat(1000), Some(10), None, None);
    let _very_long_search = env.repository.search_memories(very_long_search_req).await;
    // Should handle gracefully without crashing

    let zero_limit_search_req = create_search_request("test", Some(0), None, None);
    let _zero_limit_search = env.repository.search_memories(zero_limit_search_req).await;
    // Should return empty results or handle gracefully

    // 5. Test concurrent operations and potential conflicts
    let base_memory = env
        .repository
        .create_memory(CreateMemoryRequest {
            content: "Concurrent update test".to_string(),
            embedding: None,
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.5),
            metadata: Some(json!({"concurrent_test": true})),
            parent_id: None,
            expires_at: None,
        })
        .await?;

    // Try concurrent updates
    let update_handles = vec![
        {
            let repository = env.repository.clone();
            let memory_id = base_memory.id;
            tokio::spawn(async move {
                repository
                    .update_memory(
                        memory_id,
                        UpdateMemoryRequest {
                            content: Some("Update 1".to_string()),
                            embedding: None,
                            tier: None,
                            importance_score: Some(0.6),
                            metadata: None,
                            expires_at: None,
                        },
                    )
                    .await
            })
        },
        {
            let repository = env.repository.clone();
            let memory_id = base_memory.id;
            tokio::spawn(async move {
                repository
                    .update_memory(
                        memory_id,
                        UpdateMemoryRequest {
                            content: Some("Update 2".to_string()),
                            embedding: None,
                            tier: None,
                            importance_score: Some(0.7),
                            metadata: None,
                            expires_at: None,
                        },
                    )
                    .await
            })
        },
    ];

    let update_results = futures::future::join_all(update_handles).await;

    // At least one update should succeed
    let successful_updates = update_results
        .into_iter()
        .filter_map(|r| r.ok())
        .filter_map(|r| r.ok())
        .count();

    assert!(
        successful_updates >= 1,
        "At least one concurrent update should succeed"
    );

    // 6. Test system recovery after errors
    // Verify that the system is still functional after error scenarios
    let recovery_memory = env
        .repository
        .create_memory(CreateMemoryRequest {
            content: "System recovery test - memory created after error scenarios".to_string(),
            embedding: None,
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.8),
            metadata: Some(json!({"recovery_test": true})),
            parent_id: None,
            expires_at: None,
        })
        .await?;

    assert!(recovery_memory.content.contains("recovery test"));

    let recovery_search_req = create_search_request("recovery_test", Some(10), None, None);
    let recovery_search = env.repository.search_memories(recovery_search_req).await?;

    assert!(
        !recovery_search.results.is_empty(),
        "System should be functional after error scenarios"
    );

    // 7. Test statistics collection still works
    let final_stats = env.repository.get_statistics().await?;
    assert!(
        final_stats.total_active.is_some(),
        "Statistics should still be available"
    );

    // Cleanup
    env.repository.delete_memory(test_memory.id).await?;
    env.repository.delete_memory(base_memory.id).await?;
    env.repository.delete_memory(recovery_memory.id).await?;

    env.cleanup_test_data().await?;
    Ok(())
}
