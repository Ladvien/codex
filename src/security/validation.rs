use crate::security::{Result, SecurityError, ValidationConfig};
use axum::{
    extract::{Request, State},
    http::{header, HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, warn};
use validator::{Validate, ValidationError};

/// Input validation and sanitization manager
pub struct ValidationManager {
    config: ValidationConfig,
    sql_injection_patterns: Vec<Regex>,
    xss_patterns: Vec<Regex>,
    malicious_patterns: Vec<Regex>,
}

impl ValidationManager {
    pub fn new(config: ValidationConfig) -> Result<Self> {
        let mut manager = Self {
            config,
            sql_injection_patterns: Vec::new(),
            xss_patterns: Vec::new(),
            malicious_patterns: Vec::new(),
        };

        if manager.config.enabled {
            manager.initialize_patterns()?;
        }

        Ok(manager)
    }

    fn initialize_patterns(&mut self) -> Result<()> {
        // SQL injection patterns
        if self.config.sql_injection_protection {
            let sql_patterns = vec![
                r"(?i)(union\s+select)",
                r"(?i)(drop\s+table)",
                r"(?i)(delete\s+from)",
                r"(?i)(insert\s+into)",
                r"(?i)(update\s+set)",
                r"(?i)(alter\s+table)",
                r"(?i)(create\s+table)",
                r"(?i)(exec\s*\()",
                r"(?i)(execute\s*\()",
                r"(?i)(\'\s*or\s*\'\s*=\s*\')",
                r"(?i)(\'\s*or\s*1\s*=\s*1)",
                r"(?i)(\'\s*;\s*drop)",
                r"(?i)(--\s*)",
                r"(?i)(/\*.*\*/)",
                r"(?i)(xp_cmdshell)",
                r"(?i)(sp_executesql)",
            ];

            for pattern in sql_patterns {
                self.sql_injection_patterns
                    .push(
                        Regex::new(pattern).map_err(|e| SecurityError::ValidationError {
                            message: format!("Failed to compile SQL injection pattern: {e}"),
                        })?,
                    );
            }
        }

        // XSS patterns
        if self.config.xss_protection {
            let xss_patterns = vec![
                r"(?i)<script[^>]*>",
                r"(?i)</script>",
                r"(?i)<iframe[^>]*>",
                r"(?i)<object[^>]*>",
                r"(?i)<embed[^>]*>",
                r"(?i)<link[^>]*>",
                r"(?i)<meta[^>]*>",
                r"(?i)javascript:",
                r"(?i)vbscript:",
                r"(?i)onload\s*=",
                r"(?i)onerror\s*=",
                r"(?i)onclick\s*=",
                r"(?i)onmouseover\s*=",
                r"(?i)onfocus\s*=",
                r"(?i)onblur\s*=",
                r"(?i)onchange\s*=",
                r"(?i)onsubmit\s*=",
                r"(?i)expression\s*\(",
                r"(?i)url\s*\(",
                r"(?i)@import",
            ];

            for pattern in xss_patterns {
                self.xss_patterns.push(Regex::new(pattern).map_err(|e| {
                    SecurityError::ValidationError {
                        message: format!("Failed to compile XSS pattern: {e}"),
                    }
                })?);
            }
        }

        // General malicious patterns
        let malicious_patterns = vec![
            r"(?i)(\.\.\/){2,}", // Path traversal
            r"(?i)\.\.\\",       // Windows path traversal
            r"(?i)\/etc\/passwd",
            r"(?i)\/etc\/shadow",
            r"(?i)\/proc\/",
            r"(?i)c:\\windows\\",
            r"(?i)cmd\.exe",
            r"(?i)powershell\.exe",
            r"(?i)bash\s*-c",
            r"(?i)sh\s*-c",
            r"(?i)\$\([^)]*\)", // Command substitution
            r"(?i)`[^`]*`",     // Backtick command execution
        ];

        for pattern in malicious_patterns {
            self.malicious_patterns
                .push(
                    Regex::new(pattern).map_err(|e| SecurityError::ValidationError {
                        message: format!("Failed to compile malicious pattern: {e}"),
                    })?,
                );
        }

        debug!(
            "Initialized validation patterns: {} SQL, {} XSS, {} malicious",
            self.sql_injection_patterns.len(),
            self.xss_patterns.len(),
            self.malicious_patterns.len()
        );

        Ok(())
    }

    /// Validate and sanitize input string
    pub fn validate_input(&self, input: &str) -> Result<String> {
        if !self.config.enabled {
            return Ok(input.to_string());
        }

        // Check for SQL injection
        if self.config.sql_injection_protection {
            for pattern in &self.sql_injection_patterns {
                if pattern.is_match(input) {
                    warn!("SQL injection attempt detected: {}", pattern.as_str());
                    return Err(SecurityError::ValidationError {
                        message: "Potential SQL injection detected".to_string(),
                    });
                }
            }
        }

        // Check for XSS
        if self.config.xss_protection {
            for pattern in &self.xss_patterns {
                if pattern.is_match(input) {
                    warn!("XSS attempt detected: {}", pattern.as_str());
                    return Err(SecurityError::ValidationError {
                        message: "Potential XSS detected".to_string(),
                    });
                }
            }
        }

        // Check for general malicious patterns
        for pattern in &self.malicious_patterns {
            if pattern.is_match(input) {
                warn!("Malicious pattern detected: {}", pattern.as_str());
                return Err(SecurityError::ValidationError {
                    message: "Malicious content detected".to_string(),
                });
            }
        }

        // Sanitize if enabled
        if self.config.sanitize_input {
            Ok(self.sanitize_input(input))
        } else {
            Ok(input.to_string())
        }
    }

    /// Sanitize input by removing or escaping dangerous characters
    fn sanitize_input(&self, input: &str) -> String {
        let mut sanitized = input.to_string();

        // Remove null bytes
        sanitized = sanitized.replace('\0', "");

        // Remove or escape common dangerous characters
        sanitized = sanitized.replace('\r', "");
        sanitized = sanitized.replace('\n', " ");
        sanitized = sanitized.replace('\t', " ");

        // Escape HTML entities if XSS protection is enabled
        if self.config.xss_protection {
            sanitized = sanitized.replace('<', "&lt;");
            sanitized = sanitized.replace('>', "&gt;");
            sanitized = sanitized.replace('"', "&quot;");
            sanitized = sanitized.replace('\'', "&#x27;");
            sanitized = sanitized.replace('&', "&amp;");
        }

        // Limit length to prevent buffer overflow attacks
        if sanitized.len() > 10000 {
            sanitized.truncate(10000);
            sanitized.push_str("...");
        }

        sanitized
    }

    /// Validate JSON payload
    pub fn validate_json(&self, json_str: &str) -> Result<serde_json::Value> {
        if !self.config.enabled {
            return serde_json::from_str(json_str).map_err(|e| SecurityError::ValidationError {
                message: format!("Invalid JSON: {e}"),
            });
        }

        // Check JSON size
        if json_str.len() > self.config.max_request_size as usize {
            return Err(SecurityError::ValidationError {
                message: "Request size exceeds maximum allowed".to_string(),
            });
        }

        // Parse JSON
        let json_value: serde_json::Value =
            serde_json::from_str(json_str).map_err(|e| SecurityError::ValidationError {
                message: format!("Invalid JSON: {e}"),
            })?;

        // Validate JSON content recursively
        self.validate_json_value(&json_value)?;

        Ok(json_value)
    }

    fn validate_json_value(&self, value: &serde_json::Value) -> Result<()> {
        match value {
            serde_json::Value::String(s) => {
                self.validate_input(s)?;
            }
            serde_json::Value::Array(arr) => {
                for item in arr {
                    self.validate_json_value(item)?;
                }
            }
            serde_json::Value::Object(obj) => {
                for (key, val) in obj {
                    self.validate_input(key)?;
                    self.validate_json_value(val)?;
                }
            }
            _ => {} // Numbers, booleans, null are safe
        }
        Ok(())
    }

    /// Validate HTTP headers
    pub fn validate_headers(&self, headers: &HeaderMap) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        // Check User-Agent header
        if let Some(user_agent) = headers.get(header::USER_AGENT) {
            if let Ok(ua_str) = user_agent.to_str() {
                self.validate_input(ua_str)?;

                // Check for suspicious user agents
                let suspicious_patterns = vec![
                    r"(?i)(sqlmap|nmap|nikto|dirb|gobuster)",
                    r"(?i)(masscan|zap|burp|wget|curl)",
                    r"(?i)(python-requests|libwww-perl)",
                ];

                for pattern_str in suspicious_patterns {
                    let pattern = Regex::new(pattern_str).unwrap();
                    if pattern.is_match(ua_str) {
                        warn!("Suspicious user agent detected: {}", ua_str);
                        return Err(SecurityError::ValidationError {
                            message: "Suspicious user agent".to_string(),
                        });
                    }
                }
            }
        }

        // Check Referer header for common attacks
        if let Some(referer) = headers.get(header::REFERER) {
            if let Ok(referer_str) = referer.to_str() {
                self.validate_input(referer_str)?;
            }
        }

        // Check custom headers
        for (_name, value) in headers {
            if let Ok(value_str) = value.to_str() {
                self.validate_input(value_str)?;
            }
        }

        Ok(())
    }

    /// Check if content type is allowed
    pub fn validate_content_type(&self, content_type: Option<&str>) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        let allowed_types = [
            "application/json",
            "application/x-www-form-urlencoded",
            "text/plain",
            "multipart/form-data",
        ];

        if let Some(ct) = content_type {
            let ct_main = ct.split(';').next().unwrap_or(ct).trim();

            if !allowed_types.contains(&ct_main) {
                return Err(SecurityError::ValidationError {
                    message: format!("Content type not allowed: {ct_main}"),
                });
            }
        }

        Ok(())
    }

    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    pub fn get_max_request_size(&self) -> u64 {
        self.config.max_request_size
    }
}

/// Request validation data for structured validation
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ValidatedRequest {
    #[validate(length(
        min = 1,
        max = 1000,
        message = "Content must be between 1 and 1000 characters"
    ))]
    pub content: Option<String>,

    #[validate(email(message = "Invalid email format"))]
    pub email: Option<String>,

    #[validate(url(message = "Invalid URL format"))]
    pub url: Option<String>,

    #[validate(range(min = 1, max = 1000, message = "Limit must be between 1 and 1000"))]
    pub limit: Option<i32>,

    #[validate(range(min = 0, message = "Offset must be non-negative"))]
    pub offset: Option<i32>,

    #[validate(length(min = 1, max = 1000))]
    pub query: Option<String>,
}

/// Custom validator for safe strings
#[allow(dead_code)]
fn validate_safe_string(value: &str) -> std::result::Result<(), ValidationError> {
    // Check for dangerous characters
    if value.contains("<script") || value.contains("javascript:") || value.contains("../../") {
        return Err(ValidationError::new("unsafe_content"));
    }

    Ok(())
}

/// Validation middleware for Axum
pub async fn validation_middleware(
    State(validator): State<Arc<ValidationManager>>,
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> std::result::Result<Response, StatusCode> {
    if !validator.is_enabled() {
        return Ok(next.run(request).await);
    }

    // Validate headers
    if validator.validate_headers(&headers).is_err() {
        warn!("Request validation failed: invalid headers");
        return Err(StatusCode::BAD_REQUEST);
    }

    // Validate content type
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|ct| ct.to_str().ok());

    if validator.validate_content_type(content_type).is_err() {
        warn!("Request validation failed: invalid content type");
        return Err(StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    // Check request size
    if let Some(content_length) = headers.get(header::CONTENT_LENGTH) {
        if let Ok(length_str) = content_length.to_str() {
            if let Ok(length) = length_str.parse::<u64>() {
                if length > validator.get_max_request_size() {
                    warn!(
                        "Request validation failed: request too large ({} bytes)",
                        length
                    );
                    return Err(StatusCode::PAYLOAD_TOO_LARGE);
                }
            }
        }
    }

    debug!("Request validation passed");
    Ok(next.run(request).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_manager_creation() {
        let config = ValidationConfig::default();
        let manager = ValidationManager::new(config).unwrap();
        assert!(manager.is_enabled());
    }

    #[test]
    fn test_sql_injection_detection() {
        let mut config = ValidationConfig::default();
        config.sql_injection_protection = true;

        let manager = ValidationManager::new(config).unwrap();

        // Test valid input
        let result = manager.validate_input("SELECT name FROM users WHERE id = 1");
        assert!(result.is_ok());

        // Test SQL injection attempt
        let result = manager.validate_input("'; DROP TABLE users; --");
        assert!(result.is_err());

        if let Err(SecurityError::ValidationError { message }) = result {
            assert!(message.contains("SQL injection"));
        }
    }

    #[test]
    fn test_xss_detection() {
        let mut config = ValidationConfig::default();
        config.xss_protection = true;

        let manager = ValidationManager::new(config).unwrap();

        // Test valid input
        let result = manager.validate_input("Hello world!");
        assert!(result.is_ok());

        // Test XSS attempt
        let result = manager.validate_input("<script>alert('xss')</script>");
        assert!(result.is_err());

        if let Err(SecurityError::ValidationError { message }) = result {
            assert!(message.contains("XSS"));
        }
    }

    #[test]
    fn test_input_sanitization() {
        let mut config = ValidationConfig::default();
        config.sanitize_input = true;
        config.xss_protection = true;

        let manager = ValidationManager::new(config).unwrap();

        let result = manager
            .validate_input("Hello <world> & 'test' \"quote\"")
            .unwrap();
        assert_eq!(
            result,
            "Hello &lt;world&gt; &amp; &#x27;test&#x27; &quot;quote&quot;"
        );
    }

    #[test]
    fn test_malicious_pattern_detection() {
        let config = ValidationConfig::default();
        let manager = ValidationManager::new(config).unwrap();

        // Test path traversal
        let result = manager.validate_input("../../../etc/passwd");
        assert!(result.is_err());

        // Test command injection
        let result = manager.validate_input("test; rm -rf /");
        assert!(result.is_ok()); // This specific pattern isn't in our malicious patterns

        // Test directory traversal
        let result = manager.validate_input("../../etc/shadow");
        assert!(result.is_err());
    }

    #[test]
    fn test_json_validation() {
        let config = ValidationConfig::default();
        let manager = ValidationManager::new(config).unwrap();

        // Valid JSON
        let json = r#"{"name": "test", "value": 123}"#;
        let result = manager.validate_json(json);
        assert!(result.is_ok());

        // Invalid JSON
        let invalid_json = r#"{"name": "test", "value":}"#;
        let result = manager.validate_json(invalid_json);
        assert!(result.is_err());
    }

    #[test]
    fn test_json_content_validation() {
        let mut config = ValidationConfig::default();
        config.xss_protection = true;

        let manager = ValidationManager::new(config).unwrap();

        // JSON with XSS content
        let json = r#"{"comment": "<script>alert('xss')</script>"}"#;
        let result = manager.validate_json(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_content_type_validation() {
        let config = ValidationConfig::default();
        let manager = ValidationManager::new(config).unwrap();

        // Allowed content type
        let result = manager.validate_content_type(Some("application/json"));
        assert!(result.is_ok());

        // Not allowed content type
        let result = manager.validate_content_type(Some("application/x-executable"));
        assert!(result.is_err());

        // Content type with charset
        let result = manager.validate_content_type(Some("application/json; charset=utf-8"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_validation_disabled() {
        let mut config = ValidationConfig::default();
        config.enabled = false;

        let manager = ValidationManager::new(config).unwrap();
        assert!(!manager.is_enabled());

        // Should pass even with malicious content when disabled
        let result = manager.validate_input("<script>alert('xss')</script>");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validated_request_struct() {
        let request = ValidatedRequest {
            content: Some("Hello world".to_string()),
            email: Some("test@example.com".to_string()),
            url: Some("https://example.com".to_string()),
            limit: Some(100),
            offset: Some(0),
            query: Some("safe query".to_string()),
        };

        let validation_result = request.validate();
        assert!(validation_result.is_ok());
    }

    #[test]
    fn test_validated_request_invalid() {
        let request = ValidatedRequest {
            content: Some("".to_string()),                            // Too short
            email: Some("invalid-email".to_string()),                 // Invalid email
            url: Some("not-a-url".to_string()),                       // Invalid URL
            limit: Some(2000),                                        // Too large
            offset: Some(-1),                                         // Negative
            query: Some("<script>alert('xss')</script>".to_string()), // Unsafe content
        };

        let validation_result = request.validate();
        assert!(validation_result.is_err());

        let errors = validation_result.unwrap_err();
        assert!(!errors.field_errors().is_empty());
    }

    #[test]
    fn test_custom_validator() {
        let valid_result = validate_safe_string("This is a safe string");
        assert!(valid_result.is_ok());

        let invalid_result = validate_safe_string("<script>alert('test')</script>");
        assert!(invalid_result.is_err());

        let traversal_result = validate_safe_string("../../etc/passwd");
        assert!(traversal_result.is_err());
    }

    #[test]
    fn test_request_size_limits() {
        let config = ValidationConfig {
            enabled: true,
            max_request_size: 1024, // 1KB limit
            sanitize_input: true,
            xss_protection: true,
            sql_injection_protection: true,
        };

        let manager = ValidationManager::new(config).unwrap();
        assert_eq!(manager.get_max_request_size(), 1024);

        // Large JSON should fail
        let large_json = "x".repeat(2000);
        let json = format!(r#"{{"data": "{large_json}"}}"#);
        let result = manager.validate_json(&json);
        assert!(result.is_err());

        if let Err(SecurityError::ValidationError { message }) = result {
            assert!(message.contains("exceeds maximum"));
        }
    }
}
