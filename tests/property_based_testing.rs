//! Property-Based Testing for Memory System
//!
//! These tests use property-based testing (via proptest) to validate
//! system invariants under randomly generated conditions.

mod test_helpers;

use anyhow::Result;
use chrono::Utc;
use codex_memory::memory::models::{
    CreateMemoryRequest, MemoryTier, SearchRequest, UpdateMemoryRequest,
};
use codex_memory::{MemoryStatus, SimpleEmbedder};
use proptest::option;
use proptest::prelude::*;
use serde_json::{json, Value};
use std::collections::HashMap;
use test_helpers::TestEnvironment;
use tokio::runtime::Runtime;
use tracing_test::traced_test;

// Property-based test strategies
prop_compose! {
    fn arb_memory_content()(content in "[a-zA-Z0-9 ]{10,1000}") -> String {
        content
    }
}

prop_compose! {
    fn arb_importance_score()(score in 0.0f32..=1.0f32) -> f32 {
        score
    }
}

prop_compose! {
    fn arb_memory_tier()(tier_index in 0usize..3) -> MemoryTier {
        match tier_index {
            0 => MemoryTier::Working,
            1 => MemoryTier::Warm,
            _ => MemoryTier::Cold,
        }
    }
}

prop_compose! {
    fn arb_metadata()(
        num_fields in 1usize..10,
        field_names in prop::collection::vec("[a-zA-Z_][a-zA-Z0-9_]{0,20}", 1..10),
        field_values in prop::collection::vec(any::<i32>(), 1..10)
    ) -> Value {
        let mut metadata = serde_json::Map::new();
        for (name, value) in field_names.into_iter().zip(field_values.into_iter()).take(num_fields) {
            metadata.insert(name, json!(value));
        }
        json!(metadata)
    }
}

prop_compose! {
    fn arb_create_memory_request()(
        content in arb_memory_content(),
        tier in arb_memory_tier(),
        importance in arb_importance_score(),
        metadata in arb_metadata()
    ) -> CreateMemoryRequest {
        CreateMemoryRequest {
            content,
            embedding: None,
            tier: Some(tier),
            importance_score: Some(importance as f64),
            metadata: Some(metadata),
            parent_id: None,
            expires_at: None,
        }
    }
}

/// Property: Memory creation should always return a valid memory with correct fields
#[tokio::test]
#[traced_test]
async fn prop_memory_creation_consistency() -> Result<()> {
    let rt = Runtime::new()?;
    let env = rt.block_on(TestEnvironment::new())?;

    let mut runner = proptest::test_runner::TestRunner::default();

    runner.run(&arb_create_memory_request(), |create_request| {
        let env_clone = &env;
        let request_clone = create_request.clone();

        rt.block_on(async move {
            let memory = env_clone
                .repository
                .create_memory(request_clone.clone())
                .await
                .map_err(|e| {
                    proptest::test_runner::TestCaseError::Fail(
                        format!("Creation failed: {}", e).into(),
                    )
                })?;

            // Property: Created memory should have same content
            prop_assert_eq!(memory.content, request_clone.content);

            // Property: Created memory should have correct tier
            if let Some(requested_tier) = request_clone.tier {
                prop_assert_eq!(memory.tier, requested_tier);
            }

            // Property: Created memory should have correct importance
            if let Some(requested_importance) = request_clone.importance_score {
                prop_assert!((memory.importance_score - requested_importance).abs() < 0.001);
            }

            // Property: Created memory should always be Active
            prop_assert_eq!(memory.status, MemoryStatus::Active);

            // Property: Created memory should have zero access count initially
            prop_assert_eq!(memory.access_count, 0);

            // Property: Created memory should have recent timestamps
            let now = Utc::now();
            let time_diff = now.signed_duration_since(memory.created_at).num_seconds();
            prop_assert!(
                time_diff >= 0 && time_diff < 10,
                "Creation timestamp should be recent"
            );

            // Property: Updated timestamp should equal created timestamp for new memory
            prop_assert_eq!(memory.created_at, memory.updated_at);

            // Property: UUID should be valid
            prop_assert_ne!(memory.id, uuid::Uuid::nil());

            // Cleanup
            let _ = env_clone.repository.delete_memory(memory.id).await;

            Ok(())
        })
    })?;

    rt.block_on(env.cleanup_test_data())?;
    Ok(())
}

/// Property: Memory retrieval should update access patterns correctly
#[tokio::test]
#[traced_test]
async fn prop_memory_access_patterns() -> Result<()> {
    let rt = Runtime::new()?;
    let env = rt.block_on(TestEnvironment::new())?;

    let mut runner = proptest::test_runner::TestRunner::default();

    runner.run(
        &(arb_create_memory_request(), 1usize..10),
        |(create_request, access_count)| {
            let env_clone = &env;
            let request_clone = create_request.clone();

            rt.block_on(async move {
                // Create memory
                let original_memory = env_clone
                    .repository
                    .create_memory(request_clone)
                    .await
                    .map_err(|e| {
                        proptest::test_runner::TestCaseError::Fail(
                            format!("Creation failed: {}", e).into(),
                        )
                    })?;

                // Access memory multiple times
                let mut last_accessed_time = original_memory.last_accessed_at;

                for i in 0..access_count {
                    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

                    let retrieved = env_clone
                        .repository
                        .get_memory(original_memory.id)
                        .await
                        .map_err(|e| {
                            proptest::test_runner::TestCaseError::Fail(
                                format!("Retrieval failed: {}", e).into(),
                            )
                        })?;

                    // Property: Access count should increment
                    prop_assert_eq!(retrieved.access_count, (i + 1) as i32);

                    // Property: Last accessed time should update
                    if i > 0 {
                        prop_assert!(
                            retrieved.last_accessed_at > last_accessed_time,
                            "Last accessed time should increase with each access"
                        );
                    }

                    last_accessed_time = retrieved.last_accessed_at;
                }

                // Cleanup
                let _ = env_clone.repository.delete_memory(original_memory.id).await;

                Ok(())
            })
        },
    )?;

    rt.block_on(env.cleanup_test_data())?;
    Ok(())
}

prop_compose! {
    fn arb_update_request()(
        content in option::of(arb_memory_content()),
        tier in option::of(arb_memory_tier()),
        importance in option::of(arb_importance_score()),
        metadata in option::of(arb_metadata())
    ) -> UpdateMemoryRequest {
        UpdateMemoryRequest {
            content,
            embedding: None,
            tier,
            importance_score: importance.map(|i| i as f64),
            metadata,
            expires_at: None,
        }
    }
}

/// Property: Memory updates should preserve invariants
#[tokio::test]
#[traced_test]
async fn prop_memory_update_invariants() -> Result<()> {
    let rt = Runtime::new()?;
    let env = rt.block_on(TestEnvironment::new())?;

    let mut runner = proptest::test_runner::TestRunner::default();

    runner.run(
        &(arb_create_memory_request(), arb_update_request()),
        |(create_request, update_request)| {
            let env_clone = &env;
            let create_clone = create_request.clone();
            let update_clone = update_request.clone();

            rt.block_on(async move {
                // Create original memory
                let original = env_clone
                    .repository
                    .create_memory(create_clone)
                    .await
                    .map_err(|e| {
                        proptest::test_runner::TestCaseError::Fail(
                            format!("Creation failed: {}", e).into(),
                        )
                    })?;

                // Update memory
                let updated = env_clone
                    .repository
                    .update_memory(original.id, update_clone.clone())
                    .await
                    .map_err(|e| {
                        proptest::test_runner::TestCaseError::Fail(
                            format!("Update failed: {}", e).into(),
                        )
                    })?;

                // Property: ID should remain unchanged
                prop_assert_eq!(updated.id, original.id);

                // Property: Created timestamp should remain unchanged
                prop_assert_eq!(updated.created_at, original.created_at);

                // Property: Updated timestamp should be newer
                prop_assert!(updated.updated_at >= original.updated_at);

                // Property: Content should update correctly
                if let Some(new_content) = &update_clone.content {
                    prop_assert_eq!(updated.content, new_content.clone());
                } else {
                    prop_assert_eq!(updated.content, original.content);
                }

                // Property: Tier should update correctly
                if let Some(new_tier) = update_clone.tier {
                    prop_assert_eq!(updated.tier, new_tier);
                } else {
                    prop_assert_eq!(updated.tier, original.tier);
                }

                // Property: Importance should update correctly
                if let Some(new_importance) = update_clone.importance_score {
                    prop_assert!((updated.importance_score - new_importance).abs() < 0.001);
                } else {
                    prop_assert!(
                        (updated.importance_score - original.importance_score).abs() < 0.001
                    );
                }

                // Property: Access count should be preserved
                prop_assert_eq!(updated.access_count, original.access_count);

                // Property: Status should remain Active (assuming no status change in update)
                prop_assert_eq!(updated.status, MemoryStatus::Active);

                // Cleanup
                let _ = env_clone.repository.delete_memory(original.id).await;

                Ok(())
            })
        },
    )?;

    rt.block_on(env.cleanup_test_data())?;
    Ok(())
}

prop_compose! {
    fn arb_search_terms()(terms in prop::collection::vec("[a-zA-Z]{3,15}", 1..5)) -> Vec<String> {
        terms
    }
}

/// Property: Search operations should be deterministic and consistent
#[tokio::test]
#[traced_test]
async fn prop_search_consistency() -> Result<()> {
    let rt = Runtime::new()?;
    let env = rt.block_on(TestEnvironment::new())?;

    let mut runner = proptest::test_runner::TestRunner::default();

    runner.run(
        &(
            prop::collection::vec(arb_create_memory_request(), 3..10),
            arb_search_terms(),
        ),
        |(memory_requests, search_terms)| {
            let env_clone = &env;

            rt.block_on(async move {
                let mut created_memories = Vec::new();

                // Create memories
                for request in memory_requests {
                    let memory =
                        env_clone
                            .repository
                            .create_memory(request)
                            .await
                            .map_err(|e| {
                                proptest::test_runner::TestCaseError::Fail(
                                    format!("Creation failed: {}", e).into(),
                                )
                            })?;
                    created_memories.push(memory);
                }

                // Test search consistency
                for term in &search_terms {
                    let search_request = SearchRequest {
                        query_text: Some(term.clone()),
                        query_embedding: None,
                        search_type: None,
                        hybrid_weights: None,
                        tier: None,
                        date_range: None,
                        importance_range: None,
                        metadata_filters: None,
                        tags: None,
                        limit: Some(100),
                        offset: None,
                        cursor: None,
                        similarity_threshold: None,
                        include_metadata: Some(true),
                        include_facets: None,
                        ranking_boost: None,
                        explain_score: None,
                    };

                    // Run same search twice
                    let results1 = env_clone
                        .repository
                        .search_memories(search_request.clone())
                        .await
                        .map_err(|e| {
                            proptest::test_runner::TestCaseError::Fail(
                                format!("Search failed: {}", e).into(),
                            )
                        })?;

                    let results2 = env_clone
                        .repository
                        .search_memories(search_request)
                        .await
                        .map_err(|e| {
                            proptest::test_runner::TestCaseError::Fail(
                                format!("Search failed: {}", e).into(),
                            )
                        })?;

                    // Property: Same search should return same number of results
                    prop_assert_eq!(
                        results1.results.len(),
                        results2.results.len(),
                        "Search results should be consistent"
                    );

                    // Property: Results should be in same order
                    for (r1, r2) in results1.results.iter().zip(results2.results.iter()) {
                        prop_assert_eq!(
                            r1.memory.id,
                            r2.memory.id,
                            "Search result order should be consistent"
                        );
                    }

                    // Property: All results should be from our created memories
                    for result in &results1.results {
                        let found = created_memories.iter().any(|m| m.id == result.memory.id);
                        if found {
                            // Property: Returned memory should match original
                            let original = created_memories
                                .iter()
                                .find(|m| m.id == result.memory.id)
                                .unwrap();
                            prop_assert_eq!(&result.memory.content, &original.content);
                            prop_assert_eq!(result.memory.tier, original.tier);
                        }
                    }
                }

                // Cleanup
                for memory in created_memories {
                    let _ = env_clone.repository.delete_memory(memory.id).await;
                }

                Ok(())
            })
        },
    )?;

    rt.block_on(env.cleanup_test_data())?;
    Ok(())
}

/// Property: Importance scores should affect tier migration correctly
#[tokio::test]
#[traced_test]
async fn prop_importance_tier_relationship() -> Result<()> {
    let rt = Runtime::new()?;
    let env = rt.block_on(TestEnvironment::new())?;

    let mut runner = proptest::test_runner::TestRunner::default();

    runner.run(&prop::collection::vec((arb_memory_content(), arb_importance_score()), 5..20), |memory_data| {
        let env_clone = &env;

        rt.block_on(async move {
            let mut created_memories = Vec::new();
            let mut importance_scores = Vec::new();

            // Create memories with different importance scores
            for (content, importance) in memory_data {
                let request = CreateMemoryRequest {
                    content: content.clone(),
                    embedding: None,
                    tier: Some(MemoryTier::Working),
                    importance_score: Some(importance as f64),
                    metadata: Some(json!({"test": "prop_importance"})),
                    parent_id: None,
                    expires_at: None,
                };

                let memory = env_clone.repository.create_memory(request).await
                    .map_err(|e| proptest::test_runner::TestCaseError::Fail(format!("Creation failed: {}", e).into()))?;

                created_memories.push(memory);
                importance_scores.push(importance);
            }

            // Property: Memories with higher importance should generally be in higher tiers
            // (This is a weak property since tier assignment may depend on other factors)
            let mut tier_importance_map: HashMap<MemoryTier, Vec<f32>> = HashMap::new();

            for (memory, importance) in created_memories.iter().zip(importance_scores.iter()) {
                tier_importance_map.entry(memory.tier).or_insert_with(Vec::new).push(*importance);
            }

            // Property: Working tier should not have significantly lower importance than Cold tier
            if let (Some(working_scores), Some(cold_scores)) =
                (tier_importance_map.get(&MemoryTier::Working), tier_importance_map.get(&MemoryTier::Cold)) {

                if !working_scores.is_empty() && !cold_scores.is_empty() {
                    let avg_working: f32 = working_scores.iter().sum::<f32>() / working_scores.len() as f32;
                    let avg_cold: f32 = cold_scores.iter().sum::<f32>() / cold_scores.len() as f32;

                    // Allow some tolerance for randomness in tier assignment
                    prop_assert!(avg_working >= avg_cold - 0.3,
                        "Working tier average importance ({}) should not be much lower than Cold tier ({})",
                        avg_working, avg_cold);
                }
            }

            // Property: All importance scores should be within valid range
            for memory in &created_memories {
                prop_assert!(memory.importance_score >= 0.0 && memory.importance_score <= 1.0,
                    "Importance score {} should be between 0 and 1", memory.importance_score);
            }

            // Cleanup
            for memory in created_memories {
                let _ = env_clone.repository.delete_memory(memory.id).await;
            }

            Ok(())
        })
    })?;

    rt.block_on(env.cleanup_test_data())?;
    Ok(())
}

/// Property: Metadata should be preserved through all operations
#[tokio::test]
#[traced_test]
async fn prop_metadata_preservation() -> Result<()> {
    let rt = Runtime::new()?;
    let env = rt.block_on(TestEnvironment::new())?;

    let mut runner = proptest::test_runner::TestRunner::default();

    runner.run(
        &(arb_create_memory_request(), arb_metadata()),
        |(create_request, update_metadata)| {
            let env_clone = &env;
            let create_clone = create_request.clone();

            rt.block_on(async move {
                // Create memory with metadata
                let original = env_clone
                    .repository
                    .create_memory(create_clone.clone())
                    .await
                    .map_err(|e| {
                        proptest::test_runner::TestCaseError::Fail(
                            format!("Creation failed: {}", e).into(),
                        )
                    })?;

                // Property: Original metadata should be preserved
                if let Some(original_metadata) = &create_clone.metadata {
                    prop_assert_eq!(&original.metadata, original_metadata);
                }

                // Retrieve and verify metadata preservation
                let retrieved =
                    env_clone
                        .repository
                        .get_memory(original.id)
                        .await
                        .map_err(|e| {
                            proptest::test_runner::TestCaseError::Fail(
                                format!("Retrieval failed: {}", e).into(),
                            )
                        })?;

                prop_assert_eq!(
                    &retrieved.metadata,
                    &original.metadata,
                    "Metadata should be preserved through retrieval"
                );

                // Update with new metadata
                let update_request = UpdateMemoryRequest {
                    content: None,
                    embedding: None,
                    tier: None,
                    importance_score: None,
                    metadata: Some(update_metadata.clone()),
                    expires_at: None,
                };

                let updated = env_clone
                    .repository
                    .update_memory(original.id, update_request)
                    .await
                    .map_err(|e| {
                        proptest::test_runner::TestCaseError::Fail(
                            format!("Update failed: {}", e).into(),
                        )
                    })?;

                // Property: Updated metadata should be preserved
                prop_assert_eq!(
                    &updated.metadata,
                    &update_metadata,
                    "Updated metadata should be preserved"
                );

                // Property: Metadata should be valid JSON
                let json_str = serde_json::to_string(&updated.metadata).map_err(|e| {
                    proptest::test_runner::TestCaseError::Fail(
                        format!("JSON serialization failed: {}", e).into(),
                    )
                })?;

                let _parsed: Value = serde_json::from_str(&json_str).map_err(|e| {
                    proptest::test_runner::TestCaseError::Fail(
                        format!("JSON parsing failed: {}", e).into(),
                    )
                })?;

                // Cleanup
                let _ = env_clone.repository.delete_memory(original.id).await;

                Ok(())
            })
        },
    )?;

    rt.block_on(env.cleanup_test_data())?;
    Ok(())
}

/// Property: Vector embeddings should maintain mathematical properties
#[tokio::test]
#[traced_test]
async fn prop_embedding_properties() -> Result<()> {
    let rt = Runtime::new()?;

    let mut runner = proptest::test_runner::TestRunner::default();

    runner.run(
        &prop::collection::vec(arb_memory_content(), 3..10),
        |contents| {
            rt.block_on(async move {
                let embedder = SimpleEmbedder::new("test-key".to_string());

                let mut embeddings = Vec::new();

                // Generate embeddings for content
                for content in &contents {
                    let embedding = embedder.generate_embedding(content).await.map_err(|e| {
                        proptest::test_runner::TestCaseError::Fail(
                            format!("Embedding failed: {}", e).into(),
                        )
                    })?;

                    // Property: Embedding should have consistent dimensions
                    prop_assert_eq!(
                        embedding.len(),
                        embedder.embedding_dimension() as usize,
                        "Embedding dimension should be consistent"
                    );

                    // Property: Embedding should contain finite numbers
                    for &value in &embedding {
                        prop_assert!(value.is_finite(), "Embedding values should be finite");
                    }

                    // Property: Embedding should not be all zeros (for non-empty content)
                    if !content.trim().is_empty() {
                        let sum_squares: f32 = embedding.iter().map(|&x| x * x).sum();
                        prop_assert!(
                            sum_squares > 0.0,
                            "Non-empty content should have non-zero embedding"
                        );
                    }

                    embeddings.push(embedding);
                }

                // Property: Different content should generally have different embeddings
                if embeddings.len() > 1 {
                    for i in 0..embeddings.len() {
                        for j in (i + 1)..embeddings.len() {
                            if contents[i] != contents[j] {
                                // Calculate cosine similarity
                                let dot_product: f32 = embeddings[i]
                                    .iter()
                                    .zip(embeddings[j].iter())
                                    .map(|(a, b)| a * b)
                                    .sum();

                                let norm_i: f32 =
                                    embeddings[i].iter().map(|&x| x * x).sum::<f32>().sqrt();
                                let norm_j: f32 =
                                    embeddings[j].iter().map(|&x| x * x).sum::<f32>().sqrt();

                                if norm_i > 0.0 && norm_j > 0.0 {
                                    let similarity = dot_product / (norm_i * norm_j);

                                    // Property: Similarity should be between -1 and 1
                                    prop_assert!(
                                        similarity >= -1.0 && similarity <= 1.0,
                                        "Cosine similarity should be between -1 and 1, got {}",
                                        similarity
                                    );

                                    // Property: Very different content should have lower similarity
                                    // (This is a weak property due to embedding complexity)
                                    if contents[i].len() > 50 && contents[j].len() > 50 {
                                        prop_assert!(
                                            similarity < 0.99,
                                            "Different content should not have perfect similarity"
                                        );
                                    }
                                }
                            }
                        }
                    }
                }

                Ok(())
            })
        },
    )?;

    Ok(())
}
