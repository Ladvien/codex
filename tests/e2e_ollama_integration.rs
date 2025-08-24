//! End-to-end tests for Ollama integration in Codex Dreams
//!
//! Tests real and mock Ollama interactions including error handling,
//! timeouts, retries, and circuit breaker functionality.

#![cfg(feature = "codex-dreams")]

mod helpers;

use anyhow::Result;
use codex_memory::insights::{
    ollama_client::{OllamaClient, OllamaClientConfig, InsightResponse},
    models::{InsightType},
};
use helpers::{
    ollama_mock::{MockOllamaConfig, MockOllamaServer},
    insights_test_utils::PerformanceMetrics,
};
use serde_json::json;
use std::time::Duration;
use uuid::Uuid;

/// Test basic Ollama client functionality with mock server
#[tokio::test]
async fn test_ollama_client_basic_functionality() -> Result<()> {
    let config = MockOllamaConfig::default();
    let server = MockOllamaServer::new(config.clone());
    let url = server.start().await?;
    
    // Wait for server to start
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    let client_config = OllamaClientConfig {
        base_url: url,
        model: config.model,
        timeout_seconds: 30,
    };
    
    let client = OllamaClient::new(client_config)?;
    
    // Test health check
    let is_healthy = client.health_check().await?;
    assert!(is_healthy, "Ollama mock server should be healthy");
    
    // Test insight generation
    let memories = vec!["Test memory for insight generation".to_string()];
    let response = client.generate_insight(memories).await?;
    
    assert!(!response.content.is_empty(), "Should generate insight content");
    assert!(response.confidence_score >= 0.0 && response.confidence_score <= 1.0, 
        "Confidence score should be in valid range");
    assert!(!response.source_memory_ids.is_empty(), "Should have source memory references");
    
    Ok(())
}

/// Test Ollama client timeout handling
#[tokio::test]
async fn test_ollama_timeout_handling() -> Result<()> {
    let config = MockOllamaConfig {
        response_delay_ms: Some(2000), // 2 second delay
        ..Default::default()
    };
    
    let server = MockOllamaServer::new(config.clone());
    let url = server.start().await?;
    
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    let client_config = OllamaClientConfig {
        base_url: url,
        model: config.model,
        timeout_seconds: 1, // Short timeout
    };
    
    let client = OllamaClient::new(client_config)?;
    
    // This should timeout
    let memories = vec!["Test memory for timeout testing".to_string()];
    let result = client.generate_insight(memories).await;
    
    assert!(result.is_err(), "Should timeout with short timeout setting");
    
    Ok(())
}

/// Test Ollama client retry mechanism with exponential backoff
#[tokio::test]
async fn test_ollama_retry_mechanism() -> Result<()> {
    let config = MockOllamaConfig {
        fail_after: Some(2), // Fail first 2 requests, then succeed
        ..Default::default()
    };
    
    let server = MockOllamaServer::new(config.clone());
    let url = server.start().await?;
    
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    let client_config = OllamaClientConfig {
        base_url: url,
        model: config.model,
        timeout_seconds: 30,
    };
    
    let client = OllamaClient::new(client_config)?;
    
    let mut metrics = PerformanceMetrics::new();
    
    // Should retry and eventually succeed
    let memories = vec!["Test memory for retry testing".to_string()];
    let response = metrics.measure_async(
        "retry_test",
        client.generate_insight(memories)
    ).await?;
    
    assert!(!response.content.is_empty(), "Should eventually succeed after retries");
    
    // Should have taken some time due to retries
    metrics.assert_under_threshold("retry_test", 10000); // But not too long
    metrics.report();
    
    Ok(())
}

/// Test localhost-only validation security feature
#[tokio::test]
async fn test_localhost_validation() -> Result<()> {
    let client_configs = vec![
        // Valid localhost URLs
        OllamaClientConfig {
            base_url: "http://localhost:11434".to_string(),
            model: "llama2:latest".to_string(),
            timeout_seconds: 30,
        },
        OllamaClientConfig {
            base_url: "http://127.0.0.1:11434".to_string(),
            model: "llama2:latest".to_string(),
            timeout_seconds: 30,
        },
        OllamaClientConfig {
            base_url: "http://[::1]:11434".to_string(),
            model: "llama2:latest".to_string(),
            timeout_seconds: 30,
        },
    ];
    
    for config in client_configs {
        let client = OllamaClient::new(config.clone());
        assert!(client.is_ok(), "Should accept localhost URL: {}", config.base_url);
    }
    
    // Invalid non-localhost URLs
    let invalid_configs = vec![
        "http://example.com:11434",
        "http://192.168.1.100:11434",
        "https://api.openai.com",
        "http://ollama.remote.com:11434",
    ];
    
    for url in invalid_configs {
        let config = OllamaClientConfig {
            base_url: url.to_string(),
            model: "llama2:latest".to_string(),
            timeout_seconds: 30,
        };
        
        let client = OllamaClient::new(config);
        assert!(client.is_err(), "Should reject non-localhost URL: {}", url);
    }
    
    Ok(())
}

/// Test Ollama response parsing and validation
#[tokio::test]
async fn test_ollama_response_parsing() -> Result<()> {
    let config = MockOllamaConfig::default();
    let server = MockOllamaServer::new(config.clone());
    let url = server.start().await?;
    
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    let client_config = OllamaClientConfig {
        base_url: url,
        model: config.model,
        timeout_seconds: 30,
    };
    
    let client = OllamaClient::new(client_config)?;
    
    // Test with different types of memory content
    let test_cases = vec![
        ("This is a test memory about coding", InsightType::Learning),
        ("I discovered a pattern in the data", InsightType::Pattern),
        ("There's a connection between A and B", InsightType::Connection),
    ];
    
    for (memory_content, _expected_type) in test_cases {
        let memories = vec![memory_content.to_string()];
        let response = client.generate_insight(memories).await?;
        
        // Validate response structure
        assert!(!response.content.is_empty(), "Content should not be empty");
        assert!(response.confidence_score >= 0.0 && response.confidence_score <= 1.0,
            "Confidence score should be valid");
        assert!(!response.source_memory_ids.is_empty(), 
            "Should have source memory IDs");
        
        // Validate JSON structure if response is JSON
        if response.content.starts_with('{') {
            let parsed: serde_json::Value = serde_json::from_str(&response.content)?;
            assert!(parsed["content"].is_string(), "Should have content field");
            assert!(parsed["confidence_score"].is_number(), "Should have confidence score");
        }
    }
    
    Ok(())
}

/// Test circuit breaker functionality
#[tokio::test]
async fn test_circuit_breaker() -> Result<()> {
    let config = MockOllamaConfig {
        always_fail: true, // Always fail to trigger circuit breaker
        ..Default::default()
    };
    
    let server = MockOllamaServer::new(config.clone());
    let url = server.start().await?;
    
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    let client_config = OllamaClientConfig {
        base_url: url,
        model: config.model,
        timeout_seconds: 5,
    };
    
    let client = OllamaClient::new(client_config)?;
    
    // Make several requests that should fail
    let memories = vec!["Test memory for circuit breaker".to_string()];
    
    for i in 0..6 { // Should trigger circuit breaker around 5 failures
        let result = client.generate_insight(memories.clone()).await;
        assert!(result.is_err(), "Request {} should fail", i + 1);
        
        // Small delay between requests
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    
    // Circuit breaker should now be open, so requests should fail faster
    let start = std::time::Instant::now();
    let result = client.generate_insight(memories).await;
    let elapsed = start.elapsed();
    
    assert!(result.is_err(), "Should still fail with circuit breaker open");
    assert!(elapsed < Duration::from_secs(2), 
        "Should fail quickly with circuit breaker open, took: {:?}", elapsed);
    
    Ok(())
}

/// Test concurrent request handling
#[tokio::test]
async fn test_concurrent_requests() -> Result<()> {
    let config = MockOllamaConfig {
        response_delay_ms: Some(100), // Small delay to test concurrency
        ..Default::default()
    };
    
    let server = MockOllamaServer::new(config.clone());
    let url = server.start().await?;
    
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    let client_config = OllamaClientConfig {
        base_url: url,
        model: config.model,
        timeout_seconds: 30,
    };
    
    let client = OllamaClient::new(client_config)?;
    
    // Create multiple concurrent requests
    let mut handles = vec![];
    
    for i in 0..10 {
        let client_clone = client.clone();
        let memory = format!("Concurrent test memory {}", i);
        
        let handle = tokio::spawn(async move {
            client_clone.generate_insight(vec![memory]).await
        });
        
        handles.push(handle);
    }
    
    // Wait for all requests to complete
    let mut success_count = 0;
    for handle in handles {
        if handle.await?.is_ok() {
            success_count += 1;
        }
    }
    
    assert_eq!(success_count, 10, "All concurrent requests should succeed");
    
    Ok(())
}

/// Integration test with real Ollama (if available)
/// This test is marked to skip if Ollama is not running
#[tokio::test]
#[ignore = "Requires real Ollama server - run with --ignored to test"]
async fn test_real_ollama_integration() -> Result<()> {
    let client_config = OllamaClientConfig {
        base_url: "http://localhost:11434".to_string(),
        model: "llama2:latest".to_string(),
        timeout_seconds: 30,
    };
    
    let client = OllamaClient::new(client_config)?;
    
    // Test health check first
    let is_healthy = client.health_check().await?;
    if !is_healthy {
        println!("Skipping real Ollama test - server not available");
        return Ok(());
    }
    
    // Test insight generation with real LLM
    let memories = vec![
        "I learned Rust programming today".to_string(),
        "Rust has great memory safety features".to_string(),
        "The borrow checker prevents memory leaks".to_string(),
    ];
    
    let response = client.generate_insight(memories).await?;
    
    // Validate real response
    assert!(!response.content.is_empty(), "Should generate real insight");
    assert!(response.content.len() > 20, "Real insight should be substantial");
    println!("Real Ollama generated insight: {}", response.content);
    
    Ok(())
}