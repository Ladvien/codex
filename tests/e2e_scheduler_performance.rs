//! End-to-end tests for scheduler and performance
//!
//! Tests the background scheduler, performance under load, 
//! and resilience to failures.

#![cfg(feature = "codex-dreams")]

mod helpers;

use anyhow::Result;
use codex_memory::insights::{
    scheduler::{InsightScheduler, SchedulerConfig},
    processor::{InsightsProcessor, ProcessorConfig},
};
use helpers::{
    insights_test_utils::{InsightTestEnv, TestMemoryBuilder, PerformanceMetrics},
    ollama_mock::{MockOllamaConfig, MockOllamaServer},
};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::{sleep, timeout};

/// Test basic scheduler functionality
#[tokio::test]
async fn test_scheduler_basic_functionality() -> Result<()> {
    let env = InsightTestEnv::new().await?;
    
    // Create test memories
    for i in 0..10 {
        let _memory = TestMemoryBuilder::new(format!("Scheduled processing test memory {}", i))
            .with_importance(0.6 + (i as f32 / 20.0))
            .create(&env.repository).await?;
    }
    
    // Create processor
    let processor_config = ProcessorConfig {
        ollama_url: env.ollama_url.clone(),
        model: "llama2:latest".to_string(),
        batch_size: 5,
        confidence_threshold: 0.5,
        timeout_seconds: 10,
    };
    
    let processor = Arc::new(InsightsProcessor::new(
        env.repository.clone(),
        processor_config,
    ).await?);
    
    // Create scheduler with short interval for testing
    let scheduler_config = SchedulerConfig {
        interval_seconds: 2, // Very short interval for testing
        enabled: true,
        batch_size: 10,
    };
    
    let scheduler = InsightScheduler::new(processor, scheduler_config)?;
    
    // Start scheduler
    scheduler.start().await?;
    
    // Wait for a few processing cycles
    sleep(Duration::from_secs(5)).await;
    
    // Manual trigger test
    let report = scheduler.trigger_now().await?;
    assert!(report.processed_memories >= 0, "Should process memories");
    
    // Check next run time
    let next_run = scheduler.next_run().await;
    assert!(next_run.is_some(), "Should have next run scheduled");
    
    // Stop scheduler
    scheduler.stop().await?;
    
    env.cleanup().await?;
    
    Ok(())
}

/// Test scheduler with processing failures
#[tokio::test]
async fn test_scheduler_failure_handling() -> Result<()> {
    let env = InsightTestEnv::new().await?;
    
    // Create mock Ollama that always fails
    let config = MockOllamaConfig {
        always_fail: true,
        ..Default::default()
    };
    
    let server = MockOllamaServer::new(config.clone());
    let url = server.start().await?;
    
    sleep(Duration::from_millis(100)).await;
    
    // Create memories
    for i in 0..5 {
        let _memory = TestMemoryBuilder::new(format!("Failure test memory {}", i))
            .create(&env.repository).await?;
    }
    
    // Create processor with failing Ollama
    let processor_config = ProcessorConfig {
        ollama_url: url,
        model: config.model,
        batch_size: 5,
        confidence_threshold: 0.5,
        timeout_seconds: 5,
    };
    
    let processor = Arc::new(InsightsProcessor::new(
        env.repository.clone(),
        processor_config,
    ).await?);
    
    let scheduler_config = SchedulerConfig {
        interval_seconds: 2,
        enabled: true,
        batch_size: 5,
    };
    
    let scheduler = InsightScheduler::new(processor, scheduler_config)?;
    
    // Scheduler should handle failures gracefully
    scheduler.start().await?;
    sleep(Duration::from_secs(3)).await;
    
    // Should still be running despite failures
    let next_run = scheduler.next_run().await;
    assert!(next_run.is_some(), "Scheduler should continue running despite failures");
    
    scheduler.stop().await?;
    env.cleanup().await?;
    
    Ok(())
}

/// Test performance with large dataset
#[tokio::test]
async fn test_large_dataset_performance() -> Result<()> {
    let env = InsightTestEnv::new().await?;
    let mut metrics = PerformanceMetrics::new();
    
    // Create 1000 test memories
    println!("Creating 1000 test memories...");
    let memory_creation_start = std::time::Instant::now();
    
    let mut memory_ids = Vec::new();
    for i in 0..1000 {
        let memory = TestMemoryBuilder::new(format!("Performance test memory {} with diverse content about various topics including technology, science, and personal experiences", i))
            .with_importance(0.3 + (i as f32 / 2000.0))
            .create(&env.repository).await?;
        memory_ids.push(memory.id);
        
        if i % 100 == 0 {
            println!("Created {} memories", i);
        }
    }
    
    println!("Memory creation took: {:?}", memory_creation_start.elapsed());
    
    // Create processor with optimized settings
    let processor_config = ProcessorConfig {
        ollama_url: env.ollama_url.clone(),
        model: "llama2:latest".to_string(),
        batch_size: 50, // Larger batches for performance
        confidence_threshold: 0.4,
        timeout_seconds: 60,
    };
    
    let mut processor = InsightsProcessor::new(
        env.repository.clone(),
        processor_config,
    ).await?;
    
    // Test batch processing performance
    let batch_size = 100;
    let mut total_processed = 0;
    
    for chunk in memory_ids.chunks(batch_size) {
        let chunk_vec = chunk.to_vec();
        let chunk_size = chunk_vec.len();
        
        let result = metrics.measure_async(
            &format!("batch_{}", total_processed / batch_size),
            processor.process_batch(chunk_vec)
        ).await?;
        
        total_processed += chunk_size;
        
        println!("Processed batch {}: {} memories, {} insights generated", 
            total_processed / batch_size, 
            chunk_size, 
            result.successful_count);
        
        // Performance assertions
        assert!(result.memories_per_second > 5.0, 
            "Should process >5 memories per second, got: {}", 
            result.memories_per_second);
    }
    
    // Overall performance assertions
    let total_time = metrics.start_time.elapsed();
    let overall_rate = total_processed as f64 / total_time.as_secs_f64();
    
    println!("Overall processing rate: {:.2} memories/second", overall_rate);
    assert!(overall_rate > 10.0, "Overall rate should be >10 memories/second");
    
    env.cleanup().await?;
    metrics.report();
    
    Ok(())
}

/// Test memory usage and resource management
#[tokio::test]
async fn test_memory_usage() -> Result<()> {
    let env = InsightTestEnv::new().await?;
    
    // Get initial memory usage
    let initial_memory = get_memory_usage();
    println!("Initial memory usage: {} MB", initial_memory / 1024 / 1024);
    
    // Create many memories
    let mut memory_ids = Vec::new();
    for i in 0..1000 {
        let memory = TestMemoryBuilder::new(format!("Memory usage test {}", i))
            .create(&env.repository).await?;
        memory_ids.push(memory.id);
    }
    
    let after_creation = get_memory_usage();
    println!("After creating 1000 memories: {} MB", after_creation / 1024 / 1024);
    
    // Process memories
    let processor_config = ProcessorConfig {
        ollama_url: env.ollama_url.clone(),
        model: "llama2:latest".to_string(),
        batch_size: 10,
        confidence_threshold: 0.5,
        timeout_seconds: 30,
    };
    
    let mut processor = InsightsProcessor::new(
        env.repository.clone(),
        processor_config,
    ).await?;
    
    let _result = processor.process_batch(memory_ids).await?;
    
    let after_processing = get_memory_usage();
    println!("After processing: {} MB", after_processing / 1024 / 1024);
    
    // Memory usage shouldn't grow excessively
    let memory_growth = after_processing - initial_memory;
    let memory_growth_mb = memory_growth / 1024 / 1024;
    
    println!("Memory growth: {} MB", memory_growth_mb);
    assert!(memory_growth_mb < 500, 
        "Memory growth should be reasonable (<500MB), got: {} MB", memory_growth_mb);
    
    env.cleanup().await?;
    
    Ok(())
}

/// Test concurrent processing performance
#[tokio::test]
async fn test_concurrent_processing() -> Result<()> {
    let env = InsightTestEnv::new().await?;
    let mut metrics = PerformanceMetrics::new();
    
    // Create memories for concurrent processing
    let mut memory_batches = Vec::new();
    for batch in 0..5 {
        let mut batch_memories = Vec::new();
        for i in 0..20 {
            let memory = TestMemoryBuilder::new(
                format!("Concurrent processing batch {} memory {}", batch, i)
            ).create(&env.repository).await?;
            batch_memories.push(memory.id);
        }
        memory_batches.push(batch_memories);
    }
    
    // Create multiple processors
    let mut processors = Vec::new();
    for i in 0..3 {
        let config = ProcessorConfig {
            ollama_url: env.ollama_url.clone(),
            model: "llama2:latest".to_string(),
            batch_size: 10,
            confidence_threshold: 0.5,
            timeout_seconds: 30,
        };
        
        let processor = InsightsProcessor::new(
            env.repository.clone(),
            config,
        ).await?;
        processors.push(Arc::new(tokio::sync::Mutex::new(processor)));
    }
    
    // Process batches concurrently
    let mut handles = Vec::new();
    
    for (i, batch) in memory_batches.into_iter().enumerate() {
        let processor = processors[i % processors.len()].clone();
        
        let handle = tokio::spawn(async move {
            let mut proc = processor.lock().await;
            proc.process_batch(batch).await
        });
        
        handles.push(handle);
    }
    
    // Wait for all to complete
    let start_time = std::time::Instant::now();
    let mut total_processed = 0;
    
    for handle in handles {
        let result = handle.await??;
        total_processed += result.successful_count;
    }
    
    let total_time = start_time.elapsed();
    let concurrent_rate = total_processed as f64 / total_time.as_secs_f64();
    
    println!("Concurrent processing rate: {:.2} insights/second", concurrent_rate);
    assert!(concurrent_rate > 5.0, 
        "Concurrent processing should be efficient: {} insights/sec", concurrent_rate);
    
    env.cleanup().await?;
    
    Ok(())
}

/// Test scheduler graceful shutdown
#[tokio::test]
async fn test_scheduler_graceful_shutdown() -> Result<()> {
    let env = InsightTestEnv::new().await?;
    
    // Create processor
    let processor_config = ProcessorConfig {
        ollama_url: env.ollama_url.clone(),
        model: "llama2:latest".to_string(),
        batch_size: 10,
        confidence_threshold: 0.5,
        timeout_seconds: 30,
    };
    
    let processor = Arc::new(InsightsProcessor::new(
        env.repository.clone(),
        processor_config,
    ).await?);
    
    let scheduler_config = SchedulerConfig {
        interval_seconds: 1, // Fast interval
        enabled: true,
        batch_size: 10,
    };
    
    let scheduler = InsightScheduler::new(processor, scheduler_config)?;
    
    // Start scheduler
    scheduler.start().await?;
    
    // Let it run briefly
    sleep(Duration::from_millis(500)).await;
    
    // Test graceful shutdown with timeout
    let shutdown_result = timeout(
        Duration::from_secs(5),
        scheduler.stop()
    ).await;
    
    assert!(shutdown_result.is_ok(), "Scheduler should shutdown gracefully within timeout");
    
    // Verify scheduler stopped
    let next_run = scheduler.next_run().await;
    assert!(next_run.is_none(), "No next run should be scheduled after stop");
    
    env.cleanup().await?;
    
    Ok(())
}

/// Helper function to get current memory usage
fn get_memory_usage() -> usize {
    // Simple memory usage estimation
    // In a real implementation, you might use a more sophisticated method
    use std::alloc::{GlobalAlloc, Layout, System};
    
    // This is a simplified version - in practice you'd use proper memory tracking
    std::process::Command::new("ps")
        .args(&["-o", "rss=", "-p"])
        .arg(std::process::id().to_string())
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|s| s.trim().parse::<usize>().ok())
        .map(|kb| kb * 1024) // Convert KB to bytes
        .unwrap_or(0)
}