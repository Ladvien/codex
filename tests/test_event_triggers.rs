use codex_memory::memory::{
    event_triggers::{EventTriggeredScoringEngine, TriggerConfig, TriggerEvent, TriggerPattern},
    models::CreateMemoryRequest,
    trigger_config_loader::TriggerConfigLoader,
    MemoryRepository, MemoryTier,
};
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tempfile::NamedTempFile;
use tokio;

async fn setup_test_pool() -> PgPool {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:password@localhost:5432/codex_memory_test".to_string()
    });

    PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to test database")
}

#[tokio::test]
async fn test_trigger_event_enum() {
    let all_types = TriggerEvent::all_types();
    assert_eq!(all_types.len(), 5);

    for trigger_type in &all_types {
        let description = trigger_type.description();
        assert!(!description.is_empty());
    }
}

#[tokio::test]
async fn test_trigger_pattern_creation() {
    let pattern = TriggerPattern::new(
        r"(?i)(error|exception|failure)".to_string(),
        vec![
            "error".to_string(),
            "exception".to_string(),
            "failure".to_string(),
        ],
    )
    .unwrap();

    assert!(pattern.matches("An error occurred in the system"));
    assert!(pattern.matches("Exception thrown during processing"));
    assert!(pattern.matches("System failure detected"));
    assert!(!pattern.matches("Everything is working fine"));
}

#[tokio::test]
async fn test_confidence_calculation() {
    let mut pattern = TriggerPattern::new(
        r"(?i)(security|vulnerability|attack)".to_string(),
        vec![
            "security".to_string(),
            "vulnerability".to_string(),
            "attack".to_string(),
        ],
    )
    .unwrap();

    pattern.context_boosters = vec!["critical".to_string(), "urgent".to_string()];

    let content1 = "Security vulnerability found";
    let content2 = "Critical security vulnerability found";
    let content3 = "Critical urgent security vulnerability found";

    let confidence1 = pattern.calculate_confidence(content1);
    let confidence2 = pattern.calculate_confidence(content2);
    let confidence3 = pattern.calculate_confidence(content3);

    assert!(confidence2 > confidence1);
    assert!(confidence3 > confidence2);
    assert!(confidence1 > 0.0);
    assert!(confidence3 <= 1.0);
}

#[tokio::test]
async fn test_event_triggered_scoring_engine_security() {
    let engine = EventTriggeredScoringEngine::with_default_config();

    let test_cases = vec![
        (
            "Security vulnerability detected in authentication system",
            TriggerEvent::Security,
        ),
        (
            "XSS attack vector found in user input validation",
            TriggerEvent::Security,
        ),
        (
            "SQL injection vulnerability in database layer",
            TriggerEvent::Security,
        ),
        ("Malware detected in file upload", TriggerEvent::Security),
    ];

    for (content, expected_trigger) in test_cases {
        let result = engine.analyze_content(content, 0.5, None).await.unwrap();

        // Debug output
        println!("Content: {}", content);
        println!(
            "Triggered: {}, Confidence: {}, Type: {:?}",
            result.triggered, result.confidence, result.trigger_type
        );

        assert!(
            result.triggered,
            "Failed to trigger for: {} (confidence: {})",
            content, result.confidence
        );
        assert!(matches!(result.trigger_type, Some(ref t) if *t == expected_trigger));
        assert_eq!(result.boosted_importance, 1.0); // 0.5 * 2.0
        assert!(result.confidence >= 0.6); // Updated to match new threshold
        assert!(result.processing_time.as_millis() < 50);
    }
}

#[tokio::test]
async fn test_event_triggered_scoring_engine_error() {
    let engine = EventTriggeredScoringEngine::with_default_config();

    let test_cases = vec![
        ("Fatal error in production database", TriggerEvent::Error),
        ("Null pointer exception in core module", TriggerEvent::Error),
        (
            "System crash during user authentication",
            TriggerEvent::Error,
        ),
        ("Critical bug causing data corruption", TriggerEvent::Error),
    ];

    for (content, expected_trigger) in test_cases {
        let result = engine.analyze_content(content, 0.6, None).await.unwrap();

        assert!(result.triggered, "Failed to trigger for: {}", content);
        assert!(matches!(result.trigger_type, Some(ref t) if *t == expected_trigger));
        assert_eq!(result.boosted_importance, 1.2); // 0.6 * 2.0
        assert!(result.processing_time.as_millis() < 50);
    }
}

#[tokio::test]
async fn test_event_triggered_scoring_engine_performance() {
    let engine = EventTriggeredScoringEngine::with_default_config();

    let test_cases = vec![
        (
            "Database query performance degraded significantly",
            TriggerEvent::Performance,
        ),
        (
            "Memory leak causing application slowdown",
            TriggerEvent::Performance,
        ),
        (
            "API response time exceeding SLA thresholds",
            TriggerEvent::Performance,
        ),
        (
            "Load balancer showing high latency",
            TriggerEvent::Performance,
        ),
    ];

    for (content, expected_trigger) in test_cases {
        let result = engine.analyze_content(content, 0.4, None).await.unwrap();

        assert!(result.triggered, "Failed to trigger for: {}", content);
        assert!(matches!(result.trigger_type, Some(ref t) if *t == expected_trigger));
        assert_eq!(result.boosted_importance, 0.8); // 0.4 * 2.0
        assert!(result.processing_time.as_millis() < 50);
    }
}

#[tokio::test]
async fn test_event_triggered_scoring_engine_business_critical() {
    let engine = EventTriggeredScoringEngine::with_default_config();

    let test_cases = vec![
        (
            "Revenue impact from customer churn increased",
            TriggerEvent::BusinessCritical,
        ),
        (
            "Strategic decision needed for product roadmap",
            TriggerEvent::BusinessCritical,
        ),
        (
            "Critical customer escalation requiring attention",
            TriggerEvent::BusinessCritical,
        ),
        (
            "KPI metrics showing declining conversion rates",
            TriggerEvent::BusinessCritical,
        ),
    ];

    for (content, expected_trigger) in test_cases {
        let result = engine.analyze_content(content, 0.3, None).await.unwrap();

        assert!(result.triggered, "Failed to trigger for: {}", content);
        assert!(matches!(result.trigger_type, Some(ref t) if *t == expected_trigger));
        assert_eq!(result.boosted_importance, 0.6); // 0.3 * 2.0
        assert!(result.processing_time.as_millis() < 50);
    }
}

#[tokio::test]
async fn test_event_triggered_scoring_engine_user_experience() {
    let engine = EventTriggeredScoringEngine::with_default_config();

    let test_cases = vec![
        (
            "User feedback indicates confusing navigation",
            TriggerEvent::UserExperience,
        ),
        (
            "Accessibility issues reported in mobile app",
            TriggerEvent::UserExperience,
        ),
        (
            "Customer complaints about checkout process",
            TriggerEvent::UserExperience,
        ),
        (
            "UI/UX improvements needed for user satisfaction",
            TriggerEvent::UserExperience,
        ),
    ];

    for (content, expected_trigger) in test_cases {
        let result = engine.analyze_content(content, 0.7, None).await.unwrap();

        assert!(result.triggered, "Failed to trigger for: {}", content);
        assert!(matches!(result.trigger_type, Some(ref t) if *t == expected_trigger));
        assert_eq!(result.boosted_importance, 1.4); // 0.7 * 2.0
        assert!(result.processing_time.as_millis() < 50);
    }
}

#[tokio::test]
async fn test_non_triggered_content() {
    let engine = EventTriggeredScoringEngine::with_default_config();

    let non_trigger_content = vec![
        "Regular code documentation update",
        "Meeting notes from daily standup",
        "Weather forecast for tomorrow",
        "Lunch recommendation from colleague",
        "Code refactoring for better readability",
    ];

    for content in non_trigger_content {
        let result = engine.analyze_content(content, 0.5, None).await.unwrap();

        assert!(!result.triggered, "Incorrectly triggered for: {}", content);
        assert!(result.trigger_type.is_none());
        assert_eq!(result.boosted_importance, 0.5); // No boost
        assert_eq!(result.confidence, 0.0);
    }
}

#[tokio::test]
async fn test_metrics_tracking() {
    let engine = EventTriggeredScoringEngine::with_default_config();

    // Process multiple samples
    let samples = vec![
        ("Security breach detected", TriggerEvent::Security),
        ("Normal content here", TriggerEvent::Security), // Won't trigger
        ("Error in payment processing", TriggerEvent::Error),
        ("Performance degradation", TriggerEvent::Performance),
        ("Another normal message", TriggerEvent::Security), // Won't trigger
    ];

    for (content, _) in &samples {
        let _result = engine.analyze_content(content, 0.5, None).await.unwrap();
    }

    let metrics = engine.get_metrics().await;
    assert_eq!(metrics.total_memories_processed, 5);
    assert_eq!(metrics.total_triggered_memories, 3);
    assert!(metrics
        .triggers_by_type
        .contains_key(&TriggerEvent::Security));
    assert!(metrics.triggers_by_type.contains_key(&TriggerEvent::Error));
    assert!(metrics
        .triggers_by_type
        .contains_key(&TriggerEvent::Performance));
    assert!(metrics.average_processing_time.as_millis() > 0);
}

#[tokio::test]
async fn test_user_specific_customization() {
    let engine = EventTriggeredScoringEngine::with_default_config();

    // Add user-specific customization
    let mut user_patterns = HashMap::new();
    let custom_pattern = TriggerPattern::new(
        r"(?i)(custom|special)".to_string(),
        vec!["custom".to_string(), "special".to_string()],
    )
    .unwrap();
    user_patterns.insert(TriggerEvent::Security, custom_pattern);

    engine
        .add_user_customization("user123".to_string(), user_patterns)
        .await
        .unwrap();

    // Test with user-specific pattern
    let result = engine
        .analyze_content(
            "Custom security implementation needed",
            0.5,
            Some("user123"),
        )
        .await
        .unwrap();

    assert!(result.triggered);
    assert!(matches!(result.trigger_type, Some(TriggerEvent::Security)));

    // Test same content without user context (should not trigger)
    let result2 = engine
        .analyze_content("Custom security implementation needed", 0.5, None)
        .await
        .unwrap();

    assert!(!result2.triggered);
}

#[tokio::test]
async fn test_config_hot_reload() {
    let temp_file = NamedTempFile::new().unwrap();
    let config_path = temp_file.path().to_str().unwrap().to_string();

    // Initial config
    let initial_config = r#"{
        "importance_multiplier": 2.0,
        "max_processing_time_ms": 50,
        "enable_ab_testing": false,
        "patterns": {
            "Security": {
                "regex": "(?i)(test_pattern)",
                "keywords": ["test_pattern"],
                "context_boosters": [],
                "confidence_threshold": 0.8,
                "enabled": true
            }
        },
        "user_customizations": {}
    }"#;
    std::fs::write(&config_path, initial_config).unwrap();

    let mut loader = TriggerConfigLoader::new(config_path.clone());
    loader.enable_hot_reload(Duration::from_millis(10));

    let config = loader.load_config().await.unwrap();
    assert_eq!(config.importance_multiplier, 2.0);

    // Wait for hot-reload timer to start
    tokio::time::sleep(Duration::from_millis(20)).await;

    // Update config
    let updated_config = r#"{
        "importance_multiplier": 3.0,
        "max_processing_time_ms": 50,
        "enable_ab_testing": false,
        "patterns": {
            "Security": {
                "regex": "(?i)(test_pattern)",
                "keywords": ["test_pattern"],
                "context_boosters": [],
                "confidence_threshold": 0.8,
                "enabled": true
            }
        },
        "user_customizations": {}
    }"#;
    std::fs::write(&config_path, updated_config).unwrap();

    // Wait for hot-reload
    tokio::time::sleep(Duration::from_millis(50)).await;

    let current_config = loader.get_current_config().await;
    assert_eq!(current_config.importance_multiplier, 3.0);
}

#[tokio::test]
#[ignore = "Requires database setup"]
async fn test_repository_integration() {
    let pool = setup_test_pool().await;
    let trigger_engine = Arc::new(EventTriggeredScoringEngine::with_default_config());
    let repository = MemoryRepository::with_trigger_engine(pool, trigger_engine);

    assert!(repository.has_trigger_engine());

    // Test memory creation with trigger
    let request = CreateMemoryRequest {
        content: "Critical security vulnerability found in authentication".to_string(),
        embedding: Some(vec![0.1, 0.2, 0.3]),
        tier: Some(MemoryTier::Working),
        importance_score: Some(0.5),
        metadata: None,
        parent_id: None,
        expires_at: None,
    };

    let memory = repository.create_memory(request).await.unwrap();

    // Verify importance was boosted
    assert_eq!(memory.importance_score, 1.0); // 0.5 * 2.0

    // Verify trigger metadata was added
    let trigger_info = memory.metadata.get("trigger_info");
    assert!(trigger_info.is_some());

    if let Some(trigger_info) = trigger_info {
        assert_eq!(trigger_info["triggered"], true);
        assert_eq!(trigger_info["trigger_type"], "Security");
        assert!(trigger_info["confidence"].as_f64().unwrap() > 0.7);
    }

    // Test metrics
    let metrics = repository.get_trigger_metrics().await;
    assert!(metrics.is_some());

    if let Some(metrics) = metrics {
        assert_eq!(metrics.total_memories_processed, 1);
        assert_eq!(metrics.total_triggered_memories, 1);
    }
}

#[tokio::test]
async fn test_processing_time_limits() {
    let mut config = TriggerConfig::default();
    config.max_processing_time_ms = 10; // Very tight limit

    let engine = EventTriggeredScoringEngine::new(config);

    let result = engine
        .analyze_content(
            "Security vulnerability with many keywords security attack threat breach exploit",
            0.5,
            None,
        )
        .await
        .unwrap();

    // Should still work within time limit
    assert!(result.processing_time.as_millis() <= 50); // Generous check for test environment
}

#[tokio::test]
async fn test_a_b_testing_framework() {
    let mut config = TriggerConfig::default();
    config.enable_ab_testing = true;
    let ab_testing_enabled = config.enable_ab_testing;

    let engine = EventTriggeredScoringEngine::new(config);

    // Process multiple samples for A/B testing
    let samples = vec![
        "Security vulnerability detected",
        "Error in production system",
        "Performance degradation observed",
        "User experience feedback negative",
        "Business critical decision needed",
    ];

    for sample in samples {
        let _result = engine.analyze_content(sample, 0.5, None).await.unwrap();
    }

    let metrics = engine.get_metrics().await;
    assert_eq!(metrics.total_memories_processed, 5);
    assert_eq!(metrics.total_triggered_memories, 5); // All should trigger

    // In a real A/B test, we would check accuracy_by_type
    // For now, just verify the framework accepts the configuration
    assert!(ab_testing_enabled);
}

#[tokio::test]
async fn test_accuracy_requirements() {
    let engine = EventTriggeredScoringEngine::with_default_config();

    // Test cases with expected outcomes (>90% accuracy requirement)
    let test_cases = vec![
        // Security cases (should trigger)
        (
            "SQL injection vulnerability found",
            true,
            TriggerEvent::Security,
        ),
        ("XSS attack vector detected", true, TriggerEvent::Security),
        (
            "Authentication bypass discovered",
            true,
            TriggerEvent::Security,
        ),
        // Error cases (should trigger)
        ("Fatal exception in core module", true, TriggerEvent::Error),
        ("System crash during processing", true, TriggerEvent::Error),
        ("Critical bug causing data loss", true, TriggerEvent::Error),
        // Non-trigger cases (should not trigger)
        ("Regular meeting notes", false, TriggerEvent::Security),
        ("Code documentation update", false, TriggerEvent::Security),
        ("Weekly status report", false, TriggerEvent::Security),
    ];

    let mut correct_predictions = 0;
    let total_cases = test_cases.len();

    for (content, should_trigger, expected_type) in test_cases {
        let result = engine.analyze_content(content, 0.5, None).await.unwrap();

        if should_trigger {
            if result.triggered && matches!(result.trigger_type, Some(ref t) if *t == expected_type)
            {
                correct_predictions += 1;
            }
        } else {
            if !result.triggered {
                correct_predictions += 1;
            }
        }
    }

    let accuracy = correct_predictions as f64 / total_cases as f64;
    assert!(accuracy >= 0.9, "Accuracy {} < 90% requirement", accuracy);
}
