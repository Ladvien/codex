use anyhow::Result;
use codex_memory::database_setup::DatabaseSetup;
use proptest::prelude::*;
use regex::Regex;

/// Comprehensive SQL injection prevention tests
/// Tests all the security measures implemented to prevent SQL injection attacks
#[cfg(test)]
mod sql_injection_tests {
    use super::*;

    /// Test database identifier validation against injection attempts
    #[test]
    fn test_database_identifier_validation() -> Result<()> {
        // Debug the DROP case
        let drop_result = DatabaseSetup::validate_database_identifier("DROP");
        println!("DROP validation result: {:?}", drop_result);
        // Valid identifiers should pass
        assert!(DatabaseSetup::validate_database_identifier("valid_db").is_ok());
        assert!(DatabaseSetup::validate_database_identifier("test123").is_ok());
        assert!(DatabaseSetup::validate_database_identifier("db_with_underscores").is_ok());
        assert!(DatabaseSetup::validate_database_identifier("db$with$dollar").is_ok());

        // SQL injection attempts should fail
        assert!(DatabaseSetup::validate_database_identifier("db'; DROP TABLE users; --").is_err());
        assert!(DatabaseSetup::validate_database_identifier("db\"; CREATE DATABASE evil; --").is_err());
        assert!(DatabaseSetup::validate_database_identifier("db' UNION SELECT * FROM passwords").is_err());
        assert!(DatabaseSetup::validate_database_identifier("db/**/OR/**/1=1").is_err());
        
        // Path traversal attempts should fail
        assert!(DatabaseSetup::validate_database_identifier("../../../etc/passwd").is_err());
        assert!(DatabaseSetup::validate_database_identifier("db\\..\\..\\windows\\system32").is_err());
        
        // Command injection attempts should fail
        assert!(DatabaseSetup::validate_database_identifier("db`rm -rf /`").is_err());
        assert!(DatabaseSetup::validate_database_identifier("db$(rm -rf /)").is_err());
        
        // Special characters that could be used for injection should fail
        assert!(DatabaseSetup::validate_database_identifier("db'").is_err());
        assert!(DatabaseSetup::validate_database_identifier("db\"").is_err());
        assert!(DatabaseSetup::validate_database_identifier("db;").is_err());
        assert!(DatabaseSetup::validate_database_identifier("db--").is_err());
        assert!(DatabaseSetup::validate_database_identifier("db/*").is_err());
        assert!(DatabaseSetup::validate_database_identifier("db*/").is_err());
        assert!(DatabaseSetup::validate_database_identifier("db<script>").is_err());
        assert!(DatabaseSetup::validate_database_identifier("db&lt;script&gt;").is_err());
        
        // Empty and null-like values should fail
        assert!(DatabaseSetup::validate_database_identifier("").is_err());
        assert!(DatabaseSetup::validate_database_identifier("   ").is_err());
        
        // Very long identifiers should fail (PostgreSQL limit is 63 chars)
        let long_identifier = "a".repeat(64);
        assert!(DatabaseSetup::validate_database_identifier(&long_identifier).is_err());
        
        // Reserved keywords should fail
        assert!(DatabaseSetup::validate_database_identifier("SELECT").is_err());
        assert!(DatabaseSetup::validate_database_identifier("DROP").is_err());
        assert!(DatabaseSetup::validate_database_identifier("CREATE").is_err());
        assert!(DatabaseSetup::validate_database_identifier("DELETE").is_err());
        assert!(DatabaseSetup::validate_database_identifier("UPDATE").is_err());
        assert!(DatabaseSetup::validate_database_identifier("INSERT").is_err());
        assert!(DatabaseSetup::validate_database_identifier("UNION").is_err());
        assert!(DatabaseSetup::validate_database_identifier("WHERE").is_err());
        
        // Case insensitive keyword detection
        assert!(DatabaseSetup::validate_database_identifier("select").is_err());
        assert!(DatabaseSetup::validate_database_identifier("Select").is_err());
        assert!(DatabaseSetup::validate_database_identifier("sElEcT").is_err());

        Ok(())
    }

    /// Test vector validation against injection attempts
    #[test] 
    fn test_vector_validation() -> Result<()> {
        // Valid vectors should pass
        let valid_vector = vec![0.1, 0.2, 0.3, 0.4, 0.5];
        assert!(DatabaseSetup::validate_and_format_vector(&valid_vector).is_ok());
        
        let zero_vector = vec![0.0; 100];
        assert!(DatabaseSetup::validate_and_format_vector(&zero_vector).is_ok());
        
        let normalized_vector = vec![-1.0, -0.5, 0.0, 0.5, 1.0];
        assert!(DatabaseSetup::validate_and_format_vector(&normalized_vector).is_ok());

        // Empty vector should fail
        let empty_vector: Vec<f32> = vec![];
        assert!(DatabaseSetup::validate_and_format_vector(&empty_vector).is_err());

        // Vectors with invalid values should fail
        let nan_vector = vec![0.1, f32::NAN, 0.3];
        assert!(DatabaseSetup::validate_and_format_vector(&nan_vector).is_err());
        
        let infinity_vector = vec![0.1, f32::INFINITY, 0.3];
        assert!(DatabaseSetup::validate_and_format_vector(&infinity_vector).is_err());
        
        let neg_infinity_vector = vec![0.1, f32::NEG_INFINITY, 0.3];
        assert!(DatabaseSetup::validate_and_format_vector(&neg_infinity_vector).is_err());

        // Vector too large should fail
        let oversized_vector = vec![0.1; 2001];
        assert!(DatabaseSetup::validate_and_format_vector(&oversized_vector).is_err());

        Ok(())
    }

    /// Test that formatted vectors contain only safe characters
    #[test]
    fn test_vector_formatting_safety() -> Result<()> {
        let test_vector = vec![0.1, -0.2, 3.14159, 0.0, 1.23e-4];
        let formatted = DatabaseSetup::validate_and_format_vector(&test_vector)?;
        
        // Should only contain digits, dots, commas, and minus signs
        let safe_pattern = Regex::new(r"^[-0-9.,e]+$")?;
        assert!(safe_pattern.is_match(&formatted), 
                "Formatted vector '{}' contains unsafe characters", formatted);
        
        // Should not contain SQL injection patterns
        assert!(!formatted.contains("'"));
        assert!(!formatted.contains("\""));
        assert!(!formatted.contains(";"));
        assert!(!formatted.contains("--"));
        assert!(!formatted.contains("/*"));
        assert!(!formatted.contains("*/"));
        assert!(!formatted.contains("DROP"));
        assert!(!formatted.contains("SELECT"));
        assert!(!formatted.contains("INSERT"));
        assert!(!formatted.contains("UPDATE"));
        assert!(!formatted.contains("DELETE"));
        assert!(!formatted.contains("UNION"));

        Ok(())
    }

    /// Property-based test for database identifier validation
    /// Tests a wide range of randomly generated strings to ensure validation is robust
    proptest! {
        #[test]
        fn fuzz_database_identifier_validation(s in ".*") {
            // This should never panic, only return Ok or Err
            let _ = DatabaseSetup::validate_database_identifier(&s);
        }
        
        #[test]
        fn fuzz_vector_validation(vec in prop::collection::vec(prop::num::f32::NORMAL, 1..1000)) {
            // Filter out NaN and infinite values for this test since we expect them to fail
            let clean_vec: Vec<f32> = vec.into_iter().filter(|x| x.is_finite()).collect();
            if !clean_vec.is_empty() {
                // This should never panic, only return Ok or Err  
                let _ = DatabaseSetup::validate_and_format_vector(&clean_vec);
            }
        }
    }

    /// Test specific SQL injection payloads commonly used in attacks
    #[test]
    fn test_common_sql_injection_payloads() -> Result<()> {
        let injection_payloads = vec![
            // Classic SQL injection
            "'; DROP TABLE users; --",
            "' OR '1'='1",
            "' OR 1=1 --",
            "' UNION SELECT * FROM passwords --",
            
            // Comment-based injection
            "test'/**/OR/**/1=1/**/",
            "test'--",
            "test';--",
            
            // Stacked queries
            "test'; INSERT INTO users VALUES ('hacker', 'password'); --",
            "test'; CREATE USER hacker IDENTIFIED BY 'password'; --",
            
            // Time-based blind injection
            "test' AND (SELECT SLEEP(5)) --",
            "test' WAITFOR DELAY '00:00:05' --",
            
            // Boolean-based blind injection
            "test' AND SUBSTRING(@@version,1,1)='M' --",
            "test' AND LENGTH(database())>1 --",
            
            // Error-based injection
            "test' AND EXTRACTVALUE(1, CONCAT(0x7e, (SELECT version()), 0x7e)) --",
            "test' AND (SELECT * FROM (SELECT COUNT(*),CONCAT(version(),FLOOR(RAND(0)*2))x FROM information_schema.tables GROUP BY x)a) --",
            
            // Second-order injection
            "test\\' OR 1=1 --",
            "test\\\"; DROP TABLE users; --",
            
            // NoSQL injection patterns (shouldn't work but worth testing)
            "test'; db.users.drop(); //",
            "test'; return true; //",
            
            // XML injection
            "test'; ]]></test><script>alert('xss')</script><test><![CDATA[",
            
            // Command injection within SQL context
            "test'; EXEC xp_cmdshell('rm -rf /'); --",
            "test'; SELECT load_file('/etc/passwd'); --",
        ];

        for payload in injection_payloads {
            // All of these should be rejected by our validation
            assert!(
                DatabaseSetup::validate_database_identifier(payload).is_err(),
                "SQL injection payload should be rejected: '{}'", payload
            );
        }

        Ok(())
    }

    /// Test vector payloads that could attempt injection through vector data
    #[test]  
    fn test_vector_injection_payloads() -> Result<()> {
        // These would be caught by our f32 type safety, but let's test the string format too
        let vector_injection_attempts = vec![
            // Attempt to break out of vector format
            vec![], // Empty - should fail validation
            // Note: We can't easily test string injection through f32 vector since
            // Rust's type system prevents non-numeric values in Vec<f32>
        ];

        for payload in vector_injection_attempts {
            if payload.is_empty() {
                assert!(DatabaseSetup::validate_and_format_vector(&payload).is_err());
            }
        }

        // Test that our formatted output is safe by parsing various edge case floats
        let edge_case_vectors = vec![
            vec![f32::MIN, f32::MAX],
            vec![-0.0, 0.0],
            vec![1e-38, 1e38], // Near the limits of f32 precision
        ];

        for vector in edge_case_vectors {
            match DatabaseSetup::validate_and_format_vector(&vector) {
                Ok(formatted) => {
                    // Ensure no injection characters in output
                    assert!(!formatted.contains("'"));
                    assert!(!formatted.contains("\""));
                    assert!(!formatted.contains(";"));
                    assert!(!formatted.contains("--"));
                }
                Err(_) => {
                    // It's okay if extreme values are rejected
                }
            }
        }

        Ok(())
    }

    /// Test that the validation functions handle Unicode and special encoding attempts
    #[test]
    fn test_unicode_and_encoding_attacks() -> Result<()> {
        let unicode_attacks = vec![
            // Unicode variations of SQL keywords
            "ＳＥＬＥＣＴ", // Full-width characters
            "𝐒𝐄𝐋𝐄𝐂𝐓", // Mathematical bold
            
            // Unicode normalization attacks
            "test\u{0000}DROP", // Null byte
            "test\u{200B}SELECT", // Zero-width space
            "test\u{FEFF}UNION", // Byte order mark
            
            // Mixed scripts that could confuse parsers
            "тест", // Cyrillic that looks like "test"
            "𝓽𝓮𝓼𝓽", // Mathematical script
            
            // Encoding attempts
            "test%27%20OR%201=1", // URL encoded
            "test&#39; OR 1=1", // HTML entity encoded
        ];

        for attack in unicode_attacks {
            // Our regex pattern should reject non-ASCII characters
            assert!(
                DatabaseSetup::validate_database_identifier(attack).is_err(),
                "Unicode attack should be rejected: '{}'", attack
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    /// Integration test that would actually try SQL injection if not prevented
    /// This test should be run against a test database to verify our protections work
    #[tokio::test]
    #[ignore] // Only run with actual database connection
    async fn test_sql_injection_prevention_integration() -> Result<()> {
        // This test would require a test database connection
        // It would attempt to:
        // 1. Create a DatabaseSetup instance
        // 2. Try various injection payloads through the public API
        // 3. Verify that no injection occurs and errors are properly handled
        
        // For now, this serves as documentation of what an integration test would look like
        println!("Integration test for SQL injection prevention would require test DB");
        Ok(())
    }
}