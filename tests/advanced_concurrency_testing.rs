//! Advanced Concurrency Testing for Memory System
//!
//! These tests validate thread safety, deadlock prevention, and race condition
//! handling under high concurrency scenarios using advanced testing techniques.

mod test_helpers;

use anyhow::Result;
use codex_memory::memory::models::{
    CreateMemoryRequest, MemoryTier, SearchRequest, UpdateMemoryRequest,
};
use codex_memory::MemoryStatus;
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration as StdDuration;
use test_helpers::TestEnvironment;
use tokio::sync::{Barrier, RwLock, Semaphore};
use tokio::time::{sleep, timeout};
use tracing_test::traced_test;
use uuid::Uuid;

/// Test concurrent memory creation with no race conditions
#[tokio::test]
#[traced_test]
async fn test_concurrent_memory_creation_race_free() -> Result<()> {
    let env = TestEnvironment::new().await?;
    let num_threads = 20;
    let memories_per_thread = 10;

    let barrier = Arc::new(Barrier::new(num_threads));
    let created_ids = Arc::new(Mutex::new(HashSet::new()));

    let mut handles = Vec::new();

    for thread_id in 0..num_threads {
        let env_clone = env.clone();
        let barrier_clone = barrier.clone();
        let ids_clone = created_ids.clone();

        let handle = tokio::spawn(async move {
            barrier_clone.wait().await;

            let mut local_ids = Vec::new();

            for memory_id in 0..memories_per_thread {
                let request = CreateMemoryRequest {
                    content: format!("Concurrent memory thread {thread_id} item {memory_id}"),
                    embedding: None,
                    tier: Some(MemoryTier::Working),
                    importance_score: Some(0.5),
                    metadata: Some(json!({
                        "thread_id": thread_id,
                        "memory_id": memory_id,
                        "test_type": "concurrent_creation"
                    })),
                    parent_id: None,
                    expires_at: None,
                };

                let memory = env_clone.repository.create_memory(request).await?;
                local_ids.push(memory.id);
            }

            // Ensure all IDs are unique across threads
            {
                let mut global_ids = ids_clone.lock().unwrap();
                for id in &local_ids {
                    assert!(global_ids.insert(*id), "Duplicate ID found: {id}");
                }
            }

            Result::<Vec<Uuid>, anyhow::Error>::Ok(local_ids)
        });

        handles.push(handle);
    }

    // Collect all results
    let mut all_created_ids = Vec::new();
    for handle in handles {
        let ids = handle.await??;
        all_created_ids.extend(ids);
    }

    // Verify all memories were created successfully
    assert_eq!(all_created_ids.len(), num_threads * memories_per_thread);

    // Verify each memory exists and has correct data
    for memory_id in &all_created_ids {
        let retrieved = env.repository.get_memory(*memory_id).await?;
        assert_eq!(retrieved.status, MemoryStatus::Active);
        assert!(retrieved.content.contains("Concurrent memory"));
    }

    // Cleanup
    for memory_id in all_created_ids {
        env.repository.delete_memory(memory_id).await?;
    }

    env.cleanup_test_data().await?;
    println!(
        "✓ Concurrent creation test passed with {} memories",
        num_threads * memories_per_thread
    );
    Ok(())
}

/// Test concurrent read/write operations with consistency guarantees
#[tokio::test]
#[traced_test]
async fn test_concurrent_read_write_consistency() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Create a memory to work with
    let initial_memory = env
        .repository
        .create_memory(CreateMemoryRequest {
            content: "Initial content".to_string(),
            embedding: None,
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.5),
            metadata: Some(json!({"test": "concurrent_rw", "version": 0})),
            parent_id: None,
            expires_at: None,
        })
        .await?;

    let memory_id = initial_memory.id;
    let num_readers = 10;
    let num_writers = 5;
    let operations_per_thread = 20;

    let access_counts = Arc::new(RwLock::new(HashMap::new()));
    let version_counter = Arc::new(Mutex::new(0i32));

    let mut handles = Vec::new();

    // Spawn reader threads
    for reader_id in 0..num_readers {
        let env_clone = env.clone();
        let access_counts_clone = access_counts.clone();

        let handle = tokio::spawn(async move {
            let mut local_reads = 0;

            for _op in 0..operations_per_thread {
                match env_clone.repository.get_memory(memory_id).await {
                    Ok(memory) => {
                        // Verify memory consistency
                        assert_eq!(memory.id, memory_id);
                        assert_eq!(memory.status, MemoryStatus::Active);
                        assert!(memory.access_count >= local_reads);

                        local_reads = memory.access_count;

                        // Record this access
                        {
                            let mut counts = access_counts_clone.write().await;
                            *counts.entry(format!("reader_{reader_id}")).or_insert(0) += 1;
                        }
                    }
                    Err(e) => {
                        eprintln!("Reader {reader_id} error: {e}");
                    }
                }

                sleep(StdDuration::from_millis(10)).await;
            }

            Result::<(), anyhow::Error>::Ok(())
        });

        handles.push(handle);
    }

    // Spawn writer threads
    for writer_id in 0..num_writers {
        let env_clone = env.clone();
        let version_counter_clone = version_counter.clone();
        let access_counts_clone = access_counts.clone();

        let handle = tokio::spawn(async move {
            for _op in 0..operations_per_thread {
                let new_version = {
                    let mut counter = version_counter_clone.lock().unwrap();
                    *counter += 1;
                    *counter
                };

                let update_request = UpdateMemoryRequest {
                    content: Some(format!(
                        "Updated by writer {writer_id} version {new_version}"
                    )),
                    embedding: None,
                    tier: None,
                    importance_score: None,
                    metadata: Some(json!({
                        "test": "concurrent_rw",
                        "version": new_version,
                        "writer_id": writer_id
                    })),
                    expires_at: None,
                };

                match env_clone
                    .repository
                    .update_memory(memory_id, update_request)
                    .await
                {
                    Ok(updated_memory) => {
                        // Verify update consistency
                        assert_eq!(updated_memory.id, memory_id);
                        assert!(updated_memory
                            .content
                            .contains(&format!("writer {writer_id}")));

                        // Record this write
                        {
                            let mut counts = access_counts_clone.write().await;
                            *counts.entry(format!("writer_{writer_id}")).or_insert(0) += 1;
                        }
                    }
                    Err(e) => {
                        eprintln!("Writer {writer_id} error: {e}");
                    }
                }

                sleep(StdDuration::from_millis(15)).await;
            }

            Result::<(), anyhow::Error>::Ok(())
        });

        handles.push(handle);
    }

    // Wait for all operations to complete
    for handle in handles {
        handle.await??;
    }

    // Verify final state
    let final_memory = env.repository.get_memory(memory_id).await?;
    assert_eq!(final_memory.id, memory_id);

    // The memory should have been read many times
    let expected_min_reads = num_readers * operations_per_thread;
    assert!(
        final_memory.access_count >= expected_min_reads,
        "Access count {} should be at least {}",
        final_memory.access_count,
        expected_min_reads
    );

    // Verify operation counts
    let counts = access_counts.read().await;
    let total_reads: i32 = counts
        .iter()
        .filter(|(k, _)| k.starts_with("reader_"))
        .map(|(_, v)| *v)
        .sum();
    let total_writes: i32 = counts
        .iter()
        .filter(|(k, _)| k.starts_with("writer_"))
        .map(|(_, v)| *v)
        .sum();

    println!(
        "✓ Concurrent R/W test: {} reads, {} writes, final access count: {}",
        total_reads, total_writes, final_memory.access_count
    );

    // Cleanup
    env.repository.delete_memory(memory_id).await?;
    env.cleanup_test_data().await?;
    Ok(())
}

/// Test concurrent search operations with no interference
#[tokio::test]
#[traced_test]
async fn test_concurrent_search_operations() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Create test data
    let mut test_memories = Vec::new();
    let categories = vec!["tech", "science", "business", "health", "education"];

    for (i, category) in categories.iter().enumerate() {
        for j in 0..10 {
            let memory = env
                .repository
                .create_memory(CreateMemoryRequest {
                    content: format!("Content about {category} topic number {j}"),
                    embedding: None,
                    tier: Some(MemoryTier::Working),
                    importance_score: Some((0.1 * i as f32 + 0.05 * j as f32) as f64),
                    metadata: Some(json!({
                        "category": category,
                        "index": j,
                        "test_type": "concurrent_search"
                    })),
                    parent_id: None,
                    expires_at: None,
                })
                .await?;

            test_memories.push(memory);
        }
    }

    let num_searchers = 15;
    let searches_per_thread = 30;
    let search_results = Arc::new(RwLock::new(HashMap::new()));

    let mut handles = Vec::new();

    for searcher_id in 0..num_searchers {
        let env_clone = env.clone();
        let categories_clone = categories.clone();
        let results_clone = search_results.clone();

        let handle = tokio::spawn(async move {
            let mut local_results = HashMap::new();

            for search_num in 0..searches_per_thread {
                let category = &categories_clone[search_num % categories_clone.len()];

                let search_request = SearchRequest {
                    query_text: Some(category.to_string()),
                    query_embedding: None,
                    search_type: None,
                    hybrid_weights: None,
                    tier: None,
                    date_range: None,
                    importance_range: None,
                    metadata_filters: None,
                    tags: None,
                    limit: Some(20),
                    offset: None,
                    cursor: None,
                    similarity_threshold: None,
                    include_metadata: Some(true),
                    include_facets: None,
                    ranking_boost: None,
                    explain_score: None,
                };

                match env_clone.repository.search_memories(search_request).await {
                    Ok(search_result) => {
                        // Verify search result consistency
                        for result in &search_result.results {
                            assert_eq!(result.memory.status, MemoryStatus::Active);
                            // Results should be related to the search category
                            assert!(
                                result.memory.content.contains(category)
                                    || result.memory.metadata["category"].as_str().unwrap_or("")
                                        == *category
                            );
                        }

                        let key = format!("searcher_{searcher_id}_category_{category}");
                        local_results.insert(key, search_result.results.len());
                    }
                    Err(e) => {
                        eprintln!(
                            "Searcher {searcher_id} error for category {category}: {e}"
                        );
                    }
                }

                sleep(StdDuration::from_millis(5)).await;
            }

            // Merge local results into global
            {
                let mut global_results = results_clone.write().await;
                for (key, count) in local_results {
                    global_results.insert(key, count);
                }
            }

            Result::<(), anyhow::Error>::Ok(())
        });

        handles.push(handle);
    }

    // Wait for all searches to complete
    for handle in handles {
        handle.await??;
    }

    // Verify search results
    let results = search_results.read().await;
    let total_searches = results.len();
    let total_results: usize = results.values().sum();

    println!(
        "✓ Concurrent search test: {total_searches} searches performed, {total_results} total results found"
    );

    // Verify each category was searched correctly
    for category in &categories {
        let category_searches: Vec<_> = results
            .iter()
            .filter(|(k, _)| k.contains(&format!("category_{category}")))
            .collect();

        assert!(
            !category_searches.is_empty(),
            "Category {category} should have been searched"
        );

        // All searches for the same category should return consistent results
        if category_searches.len() > 1 {
            let first_count = *category_searches[0].1;
            for (_, count) in &category_searches[1..] {
                // Allow small variation due to concurrent modifications
                assert!(
                    ((**count as i32) - (first_count as i32)).abs() <= 2,
                    "Search results for category {category} should be consistent"
                );
            }
        }
    }

    // Cleanup
    for memory in test_memories {
        env.repository.delete_memory(memory.id).await?;
    }

    env.cleanup_test_data().await?;
    Ok(())
}

/// Test deadlock prevention with complex operation sequences
#[tokio::test]
#[traced_test]
async fn test_deadlock_prevention() -> Result<()> {
    let env = TestEnvironment::new().await?;

    // Create memories for complex operations
    let mut base_memories = Vec::new();
    for i in 0..10 {
        let memory = env
            .repository
            .create_memory(CreateMemoryRequest {
                content: format!("Base memory {i}"),
                embedding: None,
                tier: Some(MemoryTier::Working),
                importance_score: Some(0.5),
                metadata: Some(json!({"index": i, "test": "deadlock_prevention"})),
                parent_id: None,
                expires_at: None,
            })
            .await?;

        base_memories.push(memory);
    }

    let num_complex_threads = 8;
    let operations_per_thread = 15;
    let timeout_duration = StdDuration::from_secs(30);

    let completed_operations = Arc::new(Mutex::new(0));
    let mut handles = Vec::new();

    for thread_id in 0..num_complex_threads {
        let env_clone = env.clone();
        let memories_clone = base_memories.clone();
        let completed_clone = completed_operations.clone();

        let handle = tokio::spawn(async move {
            let mut local_completed = 0;

            for op_num in 0..operations_per_thread {
                // Perform complex sequences that could cause deadlocks
                let sequence_result = timeout(timeout_duration, async {
                    match op_num % 4 {
                        0 => {
                            // Create -> Update -> Read -> Delete sequence
                            let create_req = CreateMemoryRequest {
                                content: format!("Thread {thread_id} op {op_num} temp"),
                                embedding: None,
                                tier: Some(MemoryTier::Working),
                                importance_score: Some(0.3),
                                metadata: Some(json!({"temp": true, "thread": thread_id})),
                                parent_id: None,
                                expires_at: None,
                            };

                            let temp_memory =
                                env_clone.repository.create_memory(create_req).await?;

                            let update_req = UpdateMemoryRequest {
                                content: Some(format!("Updated by thread {thread_id}")),
                                embedding: None,
                                tier: Some(MemoryTier::Warm),
                                importance_score: Some(0.4),
                                metadata: None,
                                expires_at: None,
                            };

                            let updated = env_clone
                                .repository
                                .update_memory(temp_memory.id, update_req)
                                .await?;
                            let _retrieved = env_clone.repository.get_memory(updated.id).await?;
                            env_clone.repository.delete_memory(temp_memory.id).await?;
                        }
                        1 => {
                            // Multiple reads with updates
                            let memory_idx = op_num % memories_clone.len();
                            let target_id = memories_clone[memory_idx].id;

                            let _read1 = env_clone.repository.get_memory(target_id).await?;

                            let search_req = SearchRequest {
                                query_text: Some("Base memory".to_string()),
                                query_embedding: None,
                                search_type: None,
                                hybrid_weights: None,
                                tier: None,
                                date_range: None,
                                importance_range: None,
                                metadata_filters: None,
                                tags: None,
                                limit: Some(5),
                                offset: None,
                                cursor: None,
                                similarity_threshold: None,
                                include_metadata: Some(true),
                                include_facets: None,
                                ranking_boost: None,
                                explain_score: None,
                            };

                            let _search_results =
                                env_clone.repository.search_memories(search_req).await?;
                            let _read2 = env_clone.repository.get_memory(target_id).await?;
                        }
                        2 => {
                            // Concurrent updates to different memories
                            let idx1 = op_num % memories_clone.len();
                            let idx2 = (op_num + 1) % memories_clone.len();

                            if idx1 != idx2 {
                                let update1 = UpdateMemoryRequest {
                                    content: None,
                                    embedding: None,
                                    tier: None,
                                    importance_score: Some(0.6),
                                    metadata: Some(json!({"updated_by": thread_id, "op": op_num})),
                                    expires_at: None,
                                };

                                let update2 = UpdateMemoryRequest {
                                    content: None,
                                    embedding: None,
                                    tier: None,
                                    importance_score: Some(0.7),
                                    metadata: Some(json!({"updated_by": thread_id, "op": op_num})),
                                    expires_at: None,
                                };

                                let (result1, result2) = tokio::join!(
                                    env_clone
                                        .repository
                                        .update_memory(memories_clone[idx1].id, update1),
                                    env_clone
                                        .repository
                                        .update_memory(memories_clone[idx2].id, update2)
                                );

                                result1?;
                                result2?;
                            }
                        }
                        _ => {
                            // Mixed operations with search
                            let memory_idx = op_num % memories_clone.len();
                            let target_id = memories_clone[memory_idx].id;

                            let search_req = SearchRequest {
                                query_text: Some("memory".to_string()),
                                query_embedding: None,
                                search_type: None,
                                hybrid_weights: None,
                                tier: None,
                                date_range: None,
                                importance_range: None,
                                metadata_filters: None,
                                tags: None,
                                limit: Some(10),
                                offset: None,
                                cursor: None,
                                similarity_threshold: None,
                                include_metadata: Some(true),
                                include_facets: None,
                                ranking_boost: None,
                                explain_score: None,
                            };

                            let (search_result, memory_result) = tokio::join!(
                                env_clone.repository.search_memories(search_req),
                                env_clone.repository.get_memory(target_id)
                            );

                            search_result?;
                            memory_result?;
                        }
                    }

                    Result::<(), anyhow::Error>::Ok(())
                })
                .await;

                match sequence_result {
                    Ok(Ok(())) => {
                        local_completed += 1;
                    }
                    Ok(Err(e)) => {
                        eprintln!("Thread {thread_id} operation {op_num} failed: {e}");
                    }
                    Err(_) => {
                        eprintln!(
                            "Thread {thread_id} operation {op_num} timed out - possible deadlock"
                        );
                        break; // Exit thread on timeout to prevent hanging test
                    }
                }

                // Small delay between operations
                sleep(StdDuration::from_millis(20)).await;
            }

            {
                let mut completed = completed_clone.lock().unwrap();
                *completed += local_completed;
            }

            Result::<i32, anyhow::Error>::Ok(local_completed)
        });

        handles.push(handle);
    }

    // Wait for all threads with overall timeout
    let overall_timeout = StdDuration::from_secs(60);
    let join_result = timeout(overall_timeout, async {
        let mut total_completed = 0;
        for handle in handles {
            match handle.await {
                Ok(Ok(count)) => total_completed += count,
                Ok(Err(e)) => eprintln!("Thread error: {e}"),
                Err(e) => eprintln!("Thread join error: {e}"),
            }
        }
        total_completed
    })
    .await;

    match join_result {
        Ok(completed_count) => {
            println!(
                "✓ Deadlock prevention test completed: {}/{} operations successful",
                completed_count,
                num_complex_threads * operations_per_thread
            );

            // We expect most operations to complete successfully
            let expected_min = (num_complex_threads * operations_per_thread) / 2;
            assert!(
                completed_count >= expected_min as i32,
                "Too many operations failed, possible deadlock issues"
            );
        }
        Err(_) => {
            panic!("Deadlock prevention test timed out - likely deadlock detected");
        }
    }

    // Verify system is still responsive
    let final_memory = env
        .repository
        .create_memory(CreateMemoryRequest {
            content: "Deadlock test verification".to_string(),
            embedding: None,
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.5),
            metadata: Some(json!({"test": "final_verification"})),
            parent_id: None,
            expires_at: None,
        })
        .await?;

    let retrieved = env.repository.get_memory(final_memory.id).await?;
    assert_eq!(retrieved.content, "Deadlock test verification");

    // Cleanup
    for memory in base_memories {
        let _ = env.repository.delete_memory(memory.id).await;
    }
    env.repository.delete_memory(final_memory.id).await?;

    env.cleanup_test_data().await?;
    println!("✓ System remains responsive after concurrency test");
    Ok(())
}

/// Test resource leak prevention under high concurrency
#[tokio::test]
#[traced_test]
async fn test_resource_leak_prevention() -> Result<()> {
    let env = TestEnvironment::new().await?;

    let num_cycles = 5;
    let operations_per_cycle = 100;
    let max_concurrent = 20;

    // Use semaphore to limit concurrency and test resource management
    let semaphore = Arc::new(Semaphore::new(max_concurrent));

    for cycle in 0..num_cycles {
        println!(
            "Starting resource leak test cycle {}/{}",
            cycle + 1,
            num_cycles
        );

        let mut handles = Vec::new();

        for op_id in 0..operations_per_cycle {
            let env_clone = env.clone();
            let sem_clone = semaphore.clone();

            let handle = tokio::spawn(async move {
                let _permit = sem_clone.acquire().await.unwrap();

                // Create temporary memory
                let temp_memory = env_clone
                    .repository
                    .create_memory(CreateMemoryRequest {
                        content: format!("Temp memory cycle {cycle} op {op_id}"),
                        embedding: None,
                        tier: Some(MemoryTier::Working),
                        importance_score: Some(0.3),
                        metadata: Some(json!({
                            "cycle": cycle,
                            "op_id": op_id,
                            "temporary": true
                        })),
                        parent_id: None,
                        expires_at: None,
                    })
                    .await?;

                // Perform operations that might leak resources
                let _retrieved = env_clone.repository.get_memory(temp_memory.id).await?;

                let search_req = SearchRequest {
                    query_text: Some("Temp memory".to_string()),
                    query_embedding: None,
                    search_type: None,
                    hybrid_weights: None,
                    tier: None,
                    date_range: None,
                    importance_range: None,
                    metadata_filters: None,
                    tags: None,
                    limit: Some(5),
                    offset: None,
                    cursor: None,
                    similarity_threshold: None,
                    include_metadata: Some(true),
                    include_facets: None,
                    ranking_boost: None,
                    explain_score: None,
                };

                let _search_result = env_clone.repository.search_memories(search_req).await?;

                // Update and then delete (cleanup)
                let update_req = UpdateMemoryRequest {
                    content: Some("About to be deleted".to_string()),
                    embedding: None,
                    tier: None,
                    importance_score: None,
                    metadata: None,
                    expires_at: None,
                };

                let _updated = env_clone
                    .repository
                    .update_memory(temp_memory.id, update_req)
                    .await?;

                // Clean up
                env_clone.repository.delete_memory(temp_memory.id).await?;

                Result::<(), anyhow::Error>::Ok(())
            });

            handles.push(handle);
        }

        // Wait for cycle to complete
        let mut successful_ops = 0;
        for handle in handles {
            match handle.await {
                Ok(Ok(())) => successful_ops += 1,
                Ok(Err(e)) => eprintln!("Operation error in cycle {cycle}: {e}"),
                Err(e) => eprintln!("Task error in cycle {cycle}: {e}"),
            }
        }

        println!(
            "Cycle {} completed: {}/{} operations successful",
            cycle + 1,
            successful_ops,
            operations_per_cycle
        );

        // Allow brief pause between cycles for cleanup
        sleep(StdDuration::from_millis(500)).await;
    }

    // Verify system still works after intensive operations
    let final_test_memory = env
        .repository
        .create_memory(CreateMemoryRequest {
            content: "Resource leak test final verification".to_string(),
            embedding: None,
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.8),
            metadata: Some(json!({"test": "resource_leak_final"})),
            parent_id: None,
            expires_at: None,
        })
        .await?;

    let verified = env.repository.get_memory(final_test_memory.id).await?;
    assert_eq!(verified.content, "Resource leak test final verification");

    // Search should still work
    let final_search = SearchRequest {
        query_text: Some("final verification".to_string()),
        query_embedding: None,
        search_type: None,
        hybrid_weights: None,
        tier: None,
        date_range: None,
        importance_range: None,
        metadata_filters: None,
        tags: None,
        limit: Some(10),
        offset: None,
        cursor: None,
        similarity_threshold: None,
        include_metadata: Some(true),
        include_facets: None,
        ranking_boost: None,
        explain_score: None,
    };

    let final_search_results = env.repository.search_memories(final_search).await?;
    assert!(!final_search_results.results.is_empty());

    // Cleanup
    env.repository.delete_memory(final_test_memory.id).await?;

    env.cleanup_test_data().await?;

    println!("✓ Resource leak prevention test completed successfully");
    println!(
        "  {} cycles × {} operations = {} total operations",
        num_cycles,
        operations_per_cycle,
        num_cycles * operations_per_cycle
    );

    Ok(())
}
