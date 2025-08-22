use anyhow::Result;
use codex_memory::embedding::EmbeddingService;
use codex_memory::memory::{
    AssessmentStage, ImportanceAssessmentConfig, ImportanceAssessmentPipeline, ImportancePattern,
    ReferenceEmbedding, StageDetails,
};
use prometheus::Registry;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;

// Mock embedding service for testing
struct MockEmbeddingService;

#[async_trait::async_trait]
impl EmbeddingService for MockEmbeddingService {
    async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>> {
        // Generate a simple mock embedding based on text length and content
        let mut embedding = vec![0.0; 384]; // Standard embedding size

        // Create deterministic but varying embeddings based on content
        let text_hash = text.len() as f32;
        let text_chars: Vec<char> = text.chars().collect();

        for (i, val) in embedding.iter_mut().enumerate() {
            let char_influence = text_chars
                .get(i % text_chars.len())
                .map(|c| *c as u32 as f32 / 1000.0)
                .unwrap_or(0.0);
            *val = ((i as f32 + text_hash + char_influence) * 0.01).sin();
        }

        // Normalize the embedding
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for val in &mut embedding {
                *val /= norm;
            }
        }

        Ok(embedding)
    }

    async fn health_check(&self) -> Result<()> {
        Ok(())
    }
}

async fn create_test_config() -> ImportanceAssessmentConfig {
    let mut config = ImportanceAssessmentConfig::default();

    // Create reference embeddings for testing
    let embedding_service = MockEmbeddingService;

    let remember_embedding = embedding_service
        .generate_embedding("remember this important thing")
        .await
        .unwrap();
    let prefer_embedding = embedding_service
        .generate_embedding("I prefer this approach")
        .await
        .unwrap();
    let decide_embedding = embedding_service
        .generate_embedding("I decide to use this method")
        .await
        .unwrap();

    config.stage2.reference_embeddings = vec![
        ReferenceEmbedding {
            name: "memory_command".to_string(),
            embedding: remember_embedding,
            weight: 0.9,
            category: "memory".to_string(),
        },
        ReferenceEmbedding {
            name: "preference_statement".to_string(),
            embedding: prefer_embedding,
            weight: 0.7,
            category: "preference".to_string(),
        },
        ReferenceEmbedding {
            name: "decision_statement".to_string(),
            embedding: decide_embedding,
            weight: 0.8,
            category: "decision".to_string(),
        },
    ];

    // Adjust thresholds for testing - make Stage 1 very permissive to test completion
    config.stage1.confidence_threshold = 0.95; // Very high threshold - most content should NOT pass
    config.stage2.confidence_threshold = 0.99; // Very high threshold to prevent Stage 2 progression
    config.stage3.llm_endpoint = "http://localhost:8999/mock-llm".to_string(); // Mock endpoint

    config
}

async fn create_test_config_permissive() -> ImportanceAssessmentConfig {
    let mut config = create_test_config().await;

    // Make thresholds more permissive to allow progression to Stage 2
    config.stage1.confidence_threshold = 0.3; // Low threshold to allow progression
    config.stage2.confidence_threshold = 0.99; // Very high threshold to stop at Stage 2

    config
}

async fn create_test_pipeline() -> Result<ImportanceAssessmentPipeline> {
    let config = create_test_config().await;
    let embedding_service = Arc::new(MockEmbeddingService);
    let registry = Registry::new();

    ImportanceAssessmentPipeline::new(config, embedding_service, &registry)
        .map_err(|e| anyhow::anyhow!("Failed to create pipeline: {}", e))
}

async fn create_test_pipeline_permissive() -> Result<ImportanceAssessmentPipeline> {
    let config = create_test_config_permissive().await;
    let embedding_service = Arc::new(MockEmbeddingService);
    let registry = Registry::new();

    ImportanceAssessmentPipeline::new(config, embedding_service, &registry)
        .map_err(|e| anyhow::anyhow!("Failed to create pipeline: {}", e))
}

#[tokio::test]
async fn test_stage1_pattern_matching() -> Result<()> {
    let pipeline = create_test_pipeline().await?;

    // Test content that should NOT pass Stage 1 threshold
    let low_importance_content = "Remember this simple thing.";
    let result = pipeline.assess_importance(low_importance_content).await?;

    // Should complete at Stage 1 with low confidence, not passing to Stage 2
    assert_eq!(result.final_stage, AssessmentStage::Stage1PatternMatching);
    assert!(result.importance_score > 0.0);
    assert!(result.total_processing_time_ms < 50); // Should be very fast

    // Verify stage results
    assert_eq!(result.stage_results.len(), 1);
    let stage1_result = &result.stage_results[0];
    assert_eq!(stage1_result.stage, AssessmentStage::Stage1PatternMatching);
    assert!(stage1_result.processing_time_ms <= 10); // Stage 1 target

    println!(
        "Stage 1 test passed: score={:.3}, confidence={:.3}, time={}ms",
        result.importance_score, result.confidence, result.total_processing_time_ms
    );

    Ok(())
}

#[tokio::test]
async fn test_stage2_semantic_similarity() -> Result<()> {
    let pipeline = create_test_pipeline_permissive().await?;

    // Test content that should pass Stage 1 and reach Stage 2
    let content = "I want to remember this preference for future decisions.";
    let result = pipeline.assess_importance(content).await?;

    // Should reach at least Stage 2
    assert!(matches!(
        result.final_stage,
        AssessmentStage::Stage2SemanticSimilarity | AssessmentStage::Stage3LLMScoring
    ));
    assert!(result.stage_results.len() >= 2);

    // Check Stage 2 performance
    let stage2_result = result
        .stage_results
        .iter()
        .find(|r| matches!(r.stage, AssessmentStage::Stage2SemanticSimilarity))
        .expect("Stage 2 should have executed");

    assert!(stage2_result.processing_time_ms <= 100); // Stage 2 target
    assert!(stage2_result.score >= 0.0);

    // Test caching - second call should be faster
    let start = Instant::now();
    let _cached_result = pipeline.assess_importance(content).await?;
    let cached_duration = start.elapsed();

    // Should be faster due to embedding cache
    assert!(cached_duration < Duration::from_millis(50));

    println!(
        "Stage 2 test passed: score={:.3}, confidence={:.3}, time={}ms",
        result.importance_score, result.confidence, result.total_processing_time_ms
    );

    Ok(())
}

#[tokio::test]
async fn test_low_importance_content() -> Result<()> {
    let pipeline = create_test_pipeline().await?;

    // Test content with no importance patterns
    let low_content = "The weather is nice today and I had lunch.";
    let result = pipeline.assess_importance(low_content).await?;

    // Should complete at Stage 1 with low score
    assert_eq!(result.final_stage, AssessmentStage::Stage1PatternMatching);
    assert!(result.importance_score < 0.3);
    assert!(result.confidence < 0.6); // Should not pass Stage 1 threshold

    println!(
        "Low importance test passed: score={:.3}, confidence={:.3}",
        result.importance_score, result.confidence
    );

    Ok(())
}

#[tokio::test]
async fn test_pipeline_performance_targets() -> Result<()> {
    let pipeline = create_test_pipeline().await?;

    let test_cases = vec![
        "Remember this important decision",
        "I prefer this approach",
        "This is a critical error that needs fixing",
        "The quick brown fox jumps over the lazy dog",
        "Please remember to always use the correct method when deciding on important features",
    ];

    for (i, content) in test_cases.iter().enumerate() {
        let start = Instant::now();
        let result = pipeline.assess_importance(content).await?;
        let total_time = start.elapsed();

        // Verify performance targets based on final stage
        match result.final_stage {
            AssessmentStage::Stage1PatternMatching => {
                assert!(
                    total_time.as_millis() < 20,
                    "Stage 1 completion too slow: {}ms",
                    total_time.as_millis()
                );
            }
            AssessmentStage::Stage2SemanticSimilarity => {
                assert!(
                    total_time.as_millis() < 150,
                    "Stage 2 completion too slow: {}ms",
                    total_time.as_millis()
                );
            }
            AssessmentStage::Stage3LLMScoring => {
                // Stage 3 would be slower, but we're using mock LLM
                assert!(
                    total_time.as_millis() < 2000,
                    "Stage 3 completion too slow: {}ms",
                    total_time.as_millis()
                );
            }
        }

        println!(
            "Test case {}: {:?} - score={:.3}, time={}ms",
            i + 1,
            result.final_stage,
            result.importance_score,
            total_time.as_millis()
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_embedding_cache_functionality() -> Result<()> {
    let pipeline = create_test_pipeline_permissive().await?;

    let content = "Remember this important decision for future reference";

    // First assessment - should cache the embedding
    let start1 = Instant::now();
    let result1 = pipeline.assess_importance(content).await?;
    let time1 = start1.elapsed();

    // Second assessment - should use cached embedding
    let start2 = Instant::now();
    let result2 = pipeline.assess_importance(content).await?;
    let time2 = start2.elapsed();

    // Results should be identical
    assert_eq!(result1.importance_score, result2.importance_score);
    assert_eq!(result1.final_stage, result2.final_stage);

    // Second call should be faster (if it reached Stage 2)
    if result1.stage_results.len() >= 2 {
        assert!(
            time2 < time1,
            "Cached call should be faster: {}ms vs {}ms",
            time2.as_millis(),
            time1.as_millis()
        );
    }

    // Check cache statistics
    let stats = pipeline.get_statistics().await;
    assert!(stats.cache_hits > 0 || stats.cache_misses > 0);

    let cache_hit_ratio = pipeline.get_cache_hit_ratio();
    assert!(cache_hit_ratio >= 0.0 && cache_hit_ratio <= 1.0);

    println!(
        "Cache test passed: hit_ratio={:.2}, cache_size={}",
        cache_hit_ratio, stats.cache_size
    );

    Ok(())
}

#[tokio::test]
async fn test_stage_progression_rates() -> Result<()> {
    let pipeline = create_test_pipeline().await?;

    let test_contents = vec![
        // Should complete at Stage 1 (low importance)
        "Just a regular message",
        "The weather is okay",
        "Random text without patterns",
        // Should reach Stage 2 (medium importance)
        "I want to remember this",
        "This is somewhat important",
        "Please prefer this method",
        // Should potentially reach Stage 3 (high importance)
        "Remember this critical decision always",
        "This is extremely important to correct",
        "I definitely prefer this vital approach",
    ];

    let mut stage1_completions = 0;
    let mut stage2_completions = 0;
    let mut stage3_completions = 0;

    for content in test_contents {
        let result = pipeline.assess_importance(content).await?;

        match result.final_stage {
            AssessmentStage::Stage1PatternMatching => stage1_completions += 1,
            AssessmentStage::Stage2SemanticSimilarity => stage2_completions += 1,
            AssessmentStage::Stage3LLMScoring => stage3_completions += 1,
        }
    }

    // Verify that we have a reasonable distribution
    assert!(
        stage1_completions > 0,
        "Should have some Stage 1 completions"
    );

    let total = stage1_completions + stage2_completions + stage3_completions;
    let stage3_percentage = (stage3_completions as f64 / total as f64) * 100.0;

    // Stage 3 should be less than 30% of total (target is < 20%)
    assert!(
        stage3_percentage < 30.0,
        "Too many Stage 3 completions: {:.1}%",
        stage3_percentage
    );

    println!(
        "Stage progression: S1={}, S2={}, S3={} (S3: {:.1}%)",
        stage1_completions, stage2_completions, stage3_completions, stage3_percentage
    );

    Ok(())
}

#[tokio::test]
async fn test_pattern_matching_accuracy() -> Result<()> {
    let pipeline = create_test_pipeline().await?;

    let test_cases = vec![
        ("Remember this important thing", true),
        ("I prefer this approach", true),
        ("This is a critical decision", true),
        ("Please correct this error", true),
        ("Just some random text", false),
        ("The weather is nice", false),
        ("Hello world", false),
    ];

    for (content, should_match) in test_cases {
        let result = pipeline.assess_importance(content).await?;

        if should_match {
            assert!(
                result.importance_score > 0.0,
                "Should have non-zero importance for: {}",
                content
            );

            // Check that patterns were actually matched
            if let Some(stage1_result) = result.stage_results.first() {
                if let StageDetails::Stage1 {
                    matched_patterns, ..
                } = &stage1_result.details
                {
                    assert!(
                        !matched_patterns.is_empty(),
                        "Should have matched patterns for: {}",
                        content
                    );
                }
            }
        } else {
            // Low importance content should have low scores
            assert!(
                result.importance_score < 0.5,
                "Should have low importance for: {}",
                content
            );
        }

        println!(
            "Pattern test '{}': score={:.3}, should_match={}",
            content, result.importance_score, should_match
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_context_boosting() -> Result<()> {
    let pipeline = create_test_pipeline().await?;

    // Test content with and without context boosters
    let without_context = "Remember this thing";
    let with_context = "Remember this very important thing that is critical";

    let result1 = pipeline.assess_importance(without_context).await?;
    let result2 = pipeline.assess_importance(with_context).await?;

    // Content with context boosters should have higher importance
    assert!(
        result2.importance_score > result1.importance_score,
        "Context boosted content should have higher score: {:.3} vs {:.3}",
        result2.importance_score,
        result1.importance_score
    );

    println!(
        "Context boost test: without={:.3}, with={:.3}",
        result1.importance_score, result2.importance_score
    );

    Ok(())
}

#[tokio::test]
async fn test_pipeline_statistics() -> Result<()> {
    let pipeline = create_test_pipeline().await?;

    // Run several assessments to generate statistics
    let test_contents = vec![
        "Remember this",
        "I prefer that",
        "Random text",
        "Important decision",
        "Critical error fix",
    ];

    for content in test_contents {
        let _ = pipeline.assess_importance(content).await?;
    }

    let stats = pipeline.get_statistics().await;

    // Verify statistics are reasonable
    assert!(stats.stage1_executions > 0);
    assert!(stats.stage1_executions >= stats.stage2_executions);
    assert!(stats.stage2_executions >= stats.stage3_executions);

    let total_completions =
        stats.completed_at_stage1 + stats.completed_at_stage2 + stats.completed_at_stage3;
    assert_eq!(total_completions, stats.stage1_executions);

    println!("Pipeline statistics: {:?}", stats);

    Ok(())
}

#[tokio::test]
async fn test_concurrent_assessments() -> Result<()> {
    let pipeline = Arc::new(create_test_pipeline().await?);

    let test_contents = vec![
        "Remember this important decision",
        "I prefer this critical approach",
        "This is a vital correction",
        "Just some random text here",
        "Another important memory to keep",
    ];

    // Run concurrent assessments
    let mut handles = Vec::new();
    for content in test_contents.clone() {
        let pipeline_clone = Arc::clone(&pipeline);
        let content_owned = content.to_string();

        let handle =
            tokio::spawn(async move { pipeline_clone.assess_importance(&content_owned).await });
        handles.push(handle);
    }

    // Wait for all assessments to complete
    let mut results = Vec::new();
    for handle in handles {
        let result = handle.await??;
        results.push(result);
    }

    // Verify all completed successfully
    assert_eq!(results.len(), test_contents.len());

    for (i, result) in results.iter().enumerate() {
        assert!(result.importance_score >= 0.0 && result.importance_score <= 1.0);
        assert!(result.confidence >= 0.0 && result.confidence <= 1.0);
        println!(
            "Concurrent test {}: score={:.3}, confidence={:.3}",
            i + 1,
            result.importance_score,
            result.confidence
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_cache_cleanup() -> Result<()> {
    let pipeline = create_test_pipeline().await?;

    // Test cache clear functionality - use simple content that won't trigger Stage 3
    let _ = pipeline.assess_importance("Some basic content").await?;
    let _ = pipeline.assess_importance("Another simple message").await?;

    let stats_before = pipeline.get_statistics().await;

    // Clear cache
    pipeline.clear_cache().await;

    let stats_after = pipeline.get_statistics().await;
    assert_eq!(stats_after.cache_size, 0);

    println!(
        "Cache cleanup test: before={}, after={}",
        stats_before.cache_size, stats_after.cache_size
    );

    Ok(())
}

#[tokio::test]
async fn test_error_handling() -> Result<()> {
    // Test with invalid configuration
    let mut config = create_test_config().await;
    config.stage1.pattern_library.push(ImportancePattern {
        name: "invalid_regex".to_string(),
        pattern: "[invalid_regex".to_string(), // Unclosed bracket
        weight: 0.5,
        context_boosters: vec![],
        category: "test".to_string(),
    });

    let embedding_service = Arc::new(MockEmbeddingService);
    let registry = Registry::new();

    // Should fail to create pipeline with invalid regex
    let result = ImportanceAssessmentPipeline::new(config, embedding_service, &registry);
    assert!(result.is_err());

    println!("Error handling test passed: invalid regex correctly rejected");

    Ok(())
}

// Integration test that exercises the full pipeline with realistic scenarios
#[tokio::test]
async fn test_realistic_scenarios() -> Result<()> {
    let pipeline = create_test_pipeline_permissive().await?;

    let scenarios = vec![
        (
            "User expressing a strong preference",
            "I really prefer using TypeScript over JavaScript for all new projects. Please remember this.",
            0.4, // Should get decent score due to preference + remember patterns
        ),
        (
            "User making an important decision", 
            "I've decided that we should always use the new authentication system. This is critical.",
            0.5, // Should get high score due to decision + critical patterns
        ),
        (
            "User correcting previous information",
            "Actually, I was wrong about that approach. The correct method is to use dependency injection.",
            0.4, // Should get medium score due to correction patterns
        ),
        (
            "Casual conversation",
            "How's the weather today? I think it might rain later.",
            0.0, // Should get zero score - no importance patterns
        ),
        (
            "Technical discussion",
            "The algorithm runs in O(n log n) time complexity with space complexity of O(n).",
            0.0, // Should get zero score - no explicit importance patterns
        ),
    ];

    for (scenario, content, expected_min_score) in scenarios {
        let result = pipeline.assess_importance(content).await?;

        assert!(
            result.importance_score >= expected_min_score,
            "Scenario '{}' got score {:.3}, expected >= {:.3}",
            scenario,
            result.importance_score,
            expected_min_score
        );

        // Verify response is well-formed
        assert!(result.confidence >= 0.0 && result.confidence <= 1.0);
        assert!(result.total_processing_time_ms >= 0);
        assert!(!result.stage_results.is_empty());

        println!(
            "Scenario '{}': score={:.3}, confidence={:.3}, stage={:?}",
            scenario, result.importance_score, result.confidence, result.final_stage
        );
    }

    Ok(())
}
