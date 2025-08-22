// Debug script to test trigger patterns
use codex_memory::memory::event_triggers::{TriggerConfig, EventTriggeredScoringEngine};

#[tokio::main]
async fn main() {
    let engine = EventTriggeredScoringEngine::with_default_config();
    
    let test_content = "Security vulnerability detected in authentication system";
    let result = engine.analyze_content(test_content, 0.5, None).await.unwrap();
    
    println!("Content: {}", test_content);
    println!("Triggered: {}", result.triggered);
    println!("Trigger type: {:?}", result.trigger_type);
    println!("Confidence: {}", result.confidence);
    println!("Original importance: {}", result.original_importance);
    println!("Boosted importance: {}", result.boosted_importance);
    
    // Test the pattern directly
    let config = TriggerConfig::default();
    if let Some(security_pattern) = config.patterns.get(&codex_memory::memory::event_triggers::TriggerEvent::Security) {
        println!("\nPattern matches: {}", security_pattern.matches(test_content));
        println!("Pattern confidence: {}", security_pattern.calculate_confidence(test_content));
        println!("Pattern threshold: {}", security_pattern.confidence_threshold);
        println!("Pattern keywords: {:?}", security_pattern.keywords);
        println!("Pattern regex: {}", security_pattern.regex);
    }
}