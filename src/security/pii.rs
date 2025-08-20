use crate::security::{PiiConfig, Result, SecurityError};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, warn};

/// PII detection and masking manager
pub struct PiiManager {
    config: PiiConfig,
    patterns: Vec<PiiPattern>,
}

/// PII pattern definition
#[derive(Debug, Clone)]
pub struct PiiPattern {
    pub name: String,
    pub regex: Regex,
    pub mask_char: char,
    pub severity: PiiSeverity,
}

/// PII severity levels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PiiSeverity {
    Low,      // Public information that might be personal
    Medium,   // Sensitive personal information
    High,     // Highly sensitive information (SSN, credit cards)
    Critical, // Extremely sensitive (passwords, tokens)
}

/// PII detection result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiiDetectionResult {
    pub found_patterns: Vec<PiiMatch>,
    pub masked_content: String,
    pub severity: PiiSeverity,
    pub requires_action: bool,
}

/// Individual PII match
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiiMatch {
    pub pattern_name: String,
    pub severity: PiiSeverity,
    pub start: usize,
    pub end: usize,
    pub matched_text: String,
    pub masked_text: String,
}

/// PII statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiiStatistics {
    pub total_scans: u64,
    pub total_matches: u64,
    pub matches_by_type: HashMap<String, u64>,
    pub high_severity_matches: u64,
    pub critical_matches: u64,
}

impl PiiManager {
    pub fn new(config: PiiConfig) -> Result<Self> {
        let mut manager = Self {
            config,
            patterns: Vec::new(),
        };

        if manager.config.enabled {
            manager.initialize_patterns()?;
        }

        Ok(manager)
    }

    fn initialize_patterns(&mut self) -> Result<()> {
        // Email addresses
        self.add_pattern(
            "email",
            r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b",
            '*',
            PiiSeverity::Medium,
        )?;

        // Social Security Numbers (US)
        self.add_pattern(
            "ssn",
            r"\b\d{3}-\d{2}-\d{4}\b|\b\d{9}\b",
            'X',
            PiiSeverity::High,
        )?;

        // Credit card numbers (basic pattern)
        self.add_pattern(
            "credit_card",
            r"\b(?:\d{4}[-\s]?){3}\d{4}\b",
            '*',
            PiiSeverity::High,
        )?;

        // Phone numbers (US format)
        self.add_pattern(
            "phone",
            r"\b(?:\+1[-.\s]?)?\(?[0-9]{3}\)?[-.\s]?[0-9]{3}[-.\s]?[0-9]{4}\b",
            'X',
            PiiSeverity::Medium,
        )?;

        // IPv4 addresses
        self.add_pattern(
            "ipv4",
            r"\b(?:(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\.){3}(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\b",
            'X',
            PiiSeverity::Low,
        )?;

        // API keys (generic patterns)
        // Pattern matches "api_key:", "api-key=", "secret_key ", etc. followed by 20+ alphanumeric chars
        self.add_pattern(
            "api_key",
            r"(?i)(api[_-]?key|access[_-]?token|secret[_-]?key)[\s:=]+[\w-]{20,}",
            '*',
            PiiSeverity::Critical,
        )?;

        // Passwords in URLs or code
        self.add_pattern(
            "password",
            r"(?i)(password|pwd|pass)[\s:=]+\S{4,}",
            '*',
            PiiSeverity::Critical,
        )?;

        // JWT tokens
        self.add_pattern(
            "jwt_token",
            r"eyJ[a-zA-Z0-9_-]+\.eyJ[a-zA-Z0-9_-]+\.[a-zA-Z0-9_-]+",
            '*',
            PiiSeverity::Critical,
        )?;

        // Bitcoin addresses
        self.add_pattern(
            "bitcoin",
            r"\b[13][a-km-zA-HJ-NP-Z1-9]{25,34}\b|bc1[a-z0-9]{39,59}\b",
            'X',
            PiiSeverity::Medium,
        )?;

        // Bank account numbers (generic)
        self.add_pattern("bank_account", r"\b\d{8,17}\b", 'X', PiiSeverity::High)?;

        // Driver's license numbers (US format)
        self.add_pattern(
            "drivers_license",
            r"\b[A-Z]{1,2}\d{6,8}\b|\b\d{8,9}\b",
            'X',
            PiiSeverity::High,
        )?;

        // Add custom patterns from config (clone to avoid borrow checker issues)
        let custom_patterns = self.config.detect_patterns.clone();
        for pattern in custom_patterns {
            self.add_pattern("custom", &pattern, '*', PiiSeverity::Medium)?;
        }

        debug!("Initialized {} PII detection patterns", self.patterns.len());
        Ok(())
    }

    fn add_pattern(
        &mut self,
        name: &str,
        pattern: &str,
        mask_char: char,
        severity: PiiSeverity,
    ) -> Result<()> {
        let regex = Regex::new(pattern).map_err(|e| SecurityError::ValidationError {
            message: format!("Invalid PII regex pattern '{pattern}': {e}"),
        })?;

        self.patterns.push(PiiPattern {
            name: name.to_string(),
            regex,
            mask_char,
            severity,
        });

        Ok(())
    }

    /// Detect PII in text content
    pub fn detect_pii(&self, content: &str) -> PiiDetectionResult {
        if !self.config.enabled {
            return PiiDetectionResult {
                found_patterns: Vec::new(),
                masked_content: content.to_string(),
                severity: PiiSeverity::Low,
                requires_action: false,
            };
        }

        // Debug: log patterns being used
        debug!(
            "Detecting PII in content with {} patterns",
            self.patterns.len()
        );

        let mut found_patterns = Vec::new();
        let mut masked_content = content.to_string();
        let mut max_severity = PiiSeverity::Low;

        // Apply each pattern
        for pattern in &self.patterns {
            for mat in pattern.regex.find_iter(content) {
                let start = mat.start();
                let end = mat.end();
                let matched_text = mat.as_str().to_string();

                // Create masked version
                let masked_text = self.create_mask(&matched_text, pattern.mask_char);

                // Update max severity
                max_severity = self.max_severity(&max_severity, &pattern.severity);

                found_patterns.push(PiiMatch {
                    pattern_name: pattern.name.clone(),
                    severity: pattern.severity.clone(),
                    start,
                    end,
                    matched_text: matched_text.clone(),
                    masked_text: masked_text.clone(),
                });
            }
        }

        // Apply masking if enabled
        if !found_patterns.is_empty() {
            // Sort matches by start position in reverse order to avoid position shifts
            found_patterns.sort_by(|a, b| b.start.cmp(&a.start));

            for pii_match in &found_patterns {
                masked_content
                    .replace_range(pii_match.start..pii_match.end, &pii_match.masked_text);
            }

            // Log PII detection
            warn!(
                "PII detected: {} matches, max severity: {:?}",
                found_patterns.len(),
                max_severity
            );
        }

        let requires_action = matches!(max_severity, PiiSeverity::High | PiiSeverity::Critical);

        PiiDetectionResult {
            found_patterns,
            masked_content,
            severity: max_severity,
            requires_action,
        }
    }

    /// Mask sensitive content for logging
    pub fn mask_for_logging(&self, content: &str) -> String {
        if !self.config.enabled || !self.config.mask_in_logs {
            return content.to_string();
        }

        let result = self.detect_pii(content);
        result.masked_content
    }

    /// Mask sensitive content for API responses
    pub fn mask_for_response(&self, content: &str) -> String {
        if !self.config.enabled || !self.config.mask_in_responses {
            return content.to_string();
        }

        let result = self.detect_pii(content);
        result.masked_content
    }

    /// Check if content should be anonymized for storage
    pub fn should_anonymize(&self, content: &str) -> bool {
        if !self.config.enabled || !self.config.anonymize_storage {
            return false;
        }

        let result = self.detect_pii(content);
        result.requires_action
    }

    /// Anonymize content for storage
    pub fn anonymize_for_storage(&self, content: &str) -> String {
        if !self.config.enabled || !self.config.anonymize_storage {
            return content.to_string();
        }

        let result = self.detect_pii(content);

        if result.requires_action {
            // For high-severity PII, replace with generic placeholders
            // We need to work with the original content and replace based on positions
            let mut anonymized = content.to_string();

            // Sort by position in reverse to avoid position shifts
            let mut high_severity_matches: Vec<_> = result
                .found_patterns
                .iter()
                .filter(|m| matches!(m.severity, PiiSeverity::High | PiiSeverity::Critical))
                .collect();
            high_severity_matches.sort_by(|a, b| b.start.cmp(&a.start));

            for pii_match in high_severity_matches {
                let placeholder = match pii_match.pattern_name.as_str() {
                    "email" => "[EMAIL]",
                    "ssn" => "[SSN]",
                    "credit_card" => "[CREDIT_CARD]",
                    "phone" => "[PHONE]",
                    "api_key" => "[API_KEY]",
                    "password" => "[PASSWORD]",
                    "jwt_token" => "[JWT_TOKEN]",
                    "bank_account" => "[BANK_ACCOUNT]",
                    "drivers_license" => "[DRIVERS_LICENSE]",
                    _ => "[PII]",
                };

                anonymized.replace_range(pii_match.start..pii_match.end, placeholder);
            }

            anonymized
        } else {
            result.masked_content
        }
    }

    fn create_mask(&self, text: &str, mask_char: char) -> String {
        if text.len() <= 4 {
            // For short strings, mask everything except first character
            let mut masked = String::new();
            for (i, _) in text.char_indices() {
                if i == 0 {
                    masked.push(text.chars().next().unwrap_or(mask_char));
                } else {
                    masked.push(mask_char);
                }
            }
            masked
        } else {
            // For longer strings, show first 2 and last 2 characters
            let chars: Vec<char> = text.chars().collect();
            let mut masked = String::new();

            for (i, &ch) in chars.iter().enumerate() {
                if i < 2 || i >= chars.len() - 2 {
                    masked.push(ch);
                } else {
                    masked.push(mask_char);
                }
            }

            masked
        }
    }

    fn max_severity(&self, a: &PiiSeverity, b: &PiiSeverity) -> PiiSeverity {
        match (a, b) {
            (PiiSeverity::Critical, _) | (_, PiiSeverity::Critical) => PiiSeverity::Critical,
            (PiiSeverity::High, _) | (_, PiiSeverity::High) => PiiSeverity::High,
            (PiiSeverity::Medium, _) | (_, PiiSeverity::Medium) => PiiSeverity::Medium,
            _ => PiiSeverity::Low,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    pub fn get_pattern_count(&self) -> usize {
        self.patterns.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pii_manager_creation() {
        let config = PiiConfig::default();
        let manager = PiiManager::new(config).unwrap();
        assert!(!manager.is_enabled()); // disabled by default
    }

    #[test]
    fn test_pii_manager_enabled() {
        let mut config = PiiConfig::default();
        config.enabled = true;

        let manager = PiiManager::new(config).unwrap();
        assert!(manager.is_enabled());
        assert!(manager.get_pattern_count() > 0);
    }

    #[test]
    fn test_email_detection() {
        let mut config = PiiConfig::default();
        config.enabled = true;
        config.detect_patterns.clear(); // Clear custom patterns to avoid duplicates

        let manager = PiiManager::new(config).unwrap();

        let text = "Please contact john.doe@example.com for support.";
        let result = manager.detect_pii(text);

        assert_eq!(result.found_patterns.len(), 1);
        assert_eq!(result.found_patterns[0].pattern_name, "email");
        assert!(matches!(
            result.found_patterns[0].severity,
            PiiSeverity::Medium
        ));
        assert_ne!(result.masked_content, text); // Should be masked
    }

    #[test]
    fn test_ssn_detection() {
        let mut config = PiiConfig::default();
        config.enabled = true;
        config.detect_patterns.clear(); // Clear custom patterns to avoid duplicates

        let manager = PiiManager::new(config).unwrap();

        let text = "My SSN is 123-45-6789.";
        let result = manager.detect_pii(text);

        assert_eq!(result.found_patterns.len(), 1);
        assert_eq!(result.found_patterns[0].pattern_name, "ssn");
        assert!(matches!(
            result.found_patterns[0].severity,
            PiiSeverity::High
        ));
        assert!(result.requires_action);
    }

    #[test]
    fn test_credit_card_detection() {
        let mut config = PiiConfig::default();
        config.enabled = true;
        config.detect_patterns.clear(); // Clear custom patterns to avoid duplicates

        let manager = PiiManager::new(config).unwrap();

        let text = "Credit card: 4532-1234-5678-9012";
        let result = manager.detect_pii(text);

        assert_eq!(result.found_patterns.len(), 1);
        assert_eq!(result.found_patterns[0].pattern_name, "credit_card");
        assert!(matches!(
            result.found_patterns[0].severity,
            PiiSeverity::High
        ));
    }

    #[test]
    fn test_api_key_detection() {
        let mut config = PiiConfig::default();
        config.enabled = true;
        config.detect_patterns.clear(); // Clear custom patterns to avoid duplicates

        let manager = PiiManager::new(config).unwrap();

        let text = "api_key: sk-1234567890abcdef1234567890abcdef";
        let result = manager.detect_pii(text);

        // Debug: print what was found
        println!(
            "API key test - found {} patterns",
            result.found_patterns.len()
        );
        for pattern in &result.found_patterns {
            println!(
                "  Found: {} - {}",
                pattern.pattern_name, pattern.matched_text
            );
        }

        assert_eq!(result.found_patterns.len(), 1);
        assert_eq!(result.found_patterns[0].pattern_name, "api_key");
        assert!(matches!(
            result.found_patterns[0].severity,
            PiiSeverity::Critical
        ));
        assert!(result.requires_action);
    }

    #[test]
    fn test_multiple_pii_detection() {
        let mut config = PiiConfig::default();
        config.enabled = true;
        config.detect_patterns.clear(); // Clear custom patterns to avoid duplicates

        let manager = PiiManager::new(config).unwrap();

        let text = "Contact john@example.com or call 555-123-4567 about SSN 123-45-6789.";
        let result = manager.detect_pii(text);

        assert_eq!(result.found_patterns.len(), 3);

        // Should detect email, phone, and SSN
        let pattern_names: Vec<&str> = result
            .found_patterns
            .iter()
            .map(|m| m.pattern_name.as_str())
            .collect();

        assert!(pattern_names.contains(&"email"));
        assert!(pattern_names.contains(&"phone"));
        assert!(pattern_names.contains(&"ssn"));

        // Max severity should be High (from SSN)
        assert!(matches!(result.severity, PiiSeverity::High));
        assert!(result.requires_action);
    }

    #[test]
    fn test_masking_for_logging() {
        let mut config = PiiConfig::default();
        config.enabled = true;
        config.mask_in_logs = true;

        let manager = PiiManager::new(config).unwrap();

        let text = "User email: john.doe@example.com";
        let masked = manager.mask_for_logging(text);

        assert_ne!(masked, text);
        assert!(!masked.contains("john.doe@example.com"));
    }

    #[test]
    fn test_masking_for_response() {
        let mut config = PiiConfig::default();
        config.enabled = true;
        config.mask_in_responses = true;

        let manager = PiiManager::new(config).unwrap();

        let text = "Phone: 555-123-4567";
        let masked = manager.mask_for_response(text);

        assert_ne!(masked, text);
        assert!(!masked.contains("555-123-4567"));
    }

    #[test]
    fn test_anonymization_for_storage() {
        let mut config = PiiConfig::default();
        config.enabled = true;
        config.anonymize_storage = true;
        config.detect_patterns.clear(); // Clear custom patterns to avoid duplicates

        let manager = PiiManager::new(config).unwrap();

        let text = "SSN: 123-45-6789 and email: john@example.com";
        let anonymized = manager.anonymize_for_storage(text);

        // High-severity PII (SSN) should be replaced with placeholder
        assert!(anonymized.contains("[SSN]"));
        // Medium-severity PII (email) should be masked but not replaced
        assert!(!anonymized.contains("123-45-6789"));
    }

    #[test]
    fn test_should_anonymize() {
        let mut config = PiiConfig::default();
        config.enabled = true;
        config.anonymize_storage = true;

        let manager = PiiManager::new(config).unwrap();

        // High-severity PII should trigger anonymization
        assert!(manager.should_anonymize("SSN: 123-45-6789"));

        // Low-severity PII should not trigger anonymization
        assert!(!manager.should_anonymize("IP: 192.168.1.1"));

        // No PII should not trigger anonymization
        assert!(!manager.should_anonymize("This is normal text"));
    }

    #[test]
    fn test_custom_patterns() {
        let config = PiiConfig {
            enabled: true,
            detect_patterns: vec![
                r"\bcustom-\d{6}\b".to_string(), // Custom pattern
            ],
            mask_in_logs: true,
            mask_in_responses: false,
            anonymize_storage: false,
        };

        let manager = PiiManager::new(config).unwrap();

        let text = "Reference number: custom-123456";
        let result = manager.detect_pii(text);

        assert_eq!(result.found_patterns.len(), 1);
        assert_eq!(result.found_patterns[0].pattern_name, "custom");
    }

    #[test]
    fn test_disabled_pii_detection() {
        let mut config = PiiConfig::default();
        config.enabled = false;

        let manager = PiiManager::new(config).unwrap();

        let text = "SSN: 123-45-6789 and email: john@example.com";
        let result = manager.detect_pii(text);

        assert_eq!(result.found_patterns.len(), 0);
        assert_eq!(result.masked_content, text);
        assert!(!result.requires_action);
    }

    #[test]
    fn test_mask_creation() {
        let mut config = PiiConfig::default();
        config.enabled = true;

        let manager = PiiManager::new(config).unwrap();

        // Test short string masking
        let short_mask = manager.create_mask("abc", '*');
        assert_eq!(short_mask, "a**");

        // Test longer string masking
        let long_mask = manager.create_mask("1234567890", 'X');
        assert_eq!(long_mask, "12XXXXXX90");

        // Test email masking (20 chars total: "john.doe@example.com")
        // Shows first 2 and last 2 chars, masks the middle 16
        let email_mask = manager.create_mask("john.doe@example.com", '*');
        assert_eq!(email_mask, "jo****************om");
    }

    #[test]
    fn test_severity_comparison() {
        let mut config = PiiConfig::default();
        config.enabled = true;

        let manager = PiiManager::new(config).unwrap();

        assert!(matches!(
            manager.max_severity(&PiiSeverity::Low, &PiiSeverity::High),
            PiiSeverity::High
        ));
        assert!(matches!(
            manager.max_severity(&PiiSeverity::Critical, &PiiSeverity::Medium),
            PiiSeverity::Critical
        ));
        assert!(matches!(
            manager.max_severity(&PiiSeverity::Low, &PiiSeverity::Low),
            PiiSeverity::Low
        ));
    }
}
