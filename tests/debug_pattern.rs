#[cfg(test)]
mod tests {
    use codex_memory::memory::event_triggers::{TriggerConfig, TriggerEvent, TriggerPattern};

    #[test]
    fn debug_pattern_matching() {
        let config = TriggerConfig::default();
        let content = "XSS attack vector found in user input validation";

        println!("Testing content: {}", content);

        // Test all patterns
        for (trigger_type, pattern) in &config.patterns {
            if pattern.matches(content) {
                let confidence = pattern.calculate_confidence(content);
                println!(
                    "{:?} pattern: confidence = {:.3}, threshold = {:.3}",
                    trigger_type, confidence, pattern.confidence_threshold
                );
                println!("  Keywords: {:?}", pattern.keywords);

                let content_lower = content.to_lowercase();
                let matching_keywords: Vec<&String> = pattern
                    .keywords
                    .iter()
                    .filter(|keyword| content_lower.contains(&keyword.to_lowercase()))
                    .collect();
                println!("  Matching keywords: {:?}", matching_keywords);
            }
        }
    }
}
