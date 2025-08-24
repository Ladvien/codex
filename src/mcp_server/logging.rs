//! MCP Logging Implementation
//!
//! This module provides structured logging capabilities for MCP servers
//! as specified in the MCP specification.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{debug, error, info, trace, warn};

/// Log levels supported by MCP
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Notice,
    Warning,
    Error,
    Critical,
    Alert,
    Emergency,
}

impl From<LogLevel> for tracing::Level {
    fn from(level: LogLevel) -> Self {
        match level {
            LogLevel::Debug => tracing::Level::DEBUG,
            LogLevel::Info | LogLevel::Notice => tracing::Level::INFO,
            LogLevel::Warning => tracing::Level::WARN,
            LogLevel::Error | LogLevel::Critical | LogLevel::Alert | LogLevel::Emergency => {
                tracing::Level::ERROR
            }
        }
    }
}

/// MCP Log message structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogMessage {
    pub level: LogLevel,
    pub logger: Option<String>,
    pub data: Value,
}

/// MCP Logging service
pub struct MCPLogger {
    sender: broadcast::Sender<LogMessage>,
    min_level: LogLevel,
}

impl MCPLogger {
    /// Create a new MCP logger with the specified minimum log level
    pub fn new(min_level: LogLevel) -> Self {
        let (sender, _) = broadcast::channel(1000);
        Self { sender, min_level }
    }

    /// Log a message at the specified level
    pub fn log(&self, level: LogLevel, logger: Option<String>, data: Value) {
        if level >= self.min_level {
            let message = LogMessage { level, logger, data };
            
            // Also log through tracing for local debugging
            let tracing_level = message.level.clone().into();
            match tracing_level {
                tracing::Level::DEBUG => debug!(logger = ?message.logger, data = %message.data, "MCP Log"),
                tracing::Level::INFO => info!(logger = ?message.logger, data = %message.data, "MCP Log"),
                tracing::Level::WARN => warn!(logger = ?message.logger, data = %message.data, "MCP Log"),
                tracing::Level::ERROR => error!(logger = ?message.logger, data = %message.data, "MCP Log"),
                _ => trace!(logger = ?message.logger, data = %message.data, "MCP Log"),
            }

            // Send to MCP clients if they're listening
            if let Err(_) = self.sender.send(message) {
                debug!("No MCP clients listening for log messages");
            }
        }
    }

    /// Log debug message
    pub fn debug(&self, logger: Option<String>, data: Value) {
        self.log(LogLevel::Debug, logger, data);
    }

    /// Log info message
    pub fn info(&self, logger: Option<String>, data: Value) {
        self.log(LogLevel::Info, logger, data);
    }

    /// Log warning message
    pub fn warning(&self, logger: Option<String>, data: Value) {
        self.log(LogLevel::Warning, logger, data);
    }

    /// Log error message
    pub fn error(&self, logger: Option<String>, data: Value) {
        self.log(LogLevel::Error, logger, data);
    }

    /// Create a subscription to log messages for MCP clients
    pub fn subscribe(&self) -> broadcast::Receiver<LogMessage> {
        self.sender.subscribe()
    }

    /// Set minimum log level
    pub fn set_min_level(&mut self, level: LogLevel) {
        self.min_level = level;
    }

    /// Get current minimum log level
    pub fn min_level(&self) -> &LogLevel {
        &self.min_level
    }

    /// Create MCP notification message for log
    pub fn create_log_notification(message: &LogMessage) -> Value {
        json!({
            "jsonrpc": "2.0",
            "method": "notifications/message",
            "params": {
                "level": message.level,
                "logger": message.logger,
                "data": message.data
            }
        })
    }
}

impl Default for MCPLogger {
    fn default() -> Self {
        Self::new(LogLevel::Info)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_level_ordering() {
        assert!(LogLevel::Debug < LogLevel::Info);
        assert!(LogLevel::Info < LogLevel::Warning);
        assert!(LogLevel::Warning < LogLevel::Error);
        assert!(LogLevel::Error < LogLevel::Critical);
    }

    #[tokio::test]
    async fn test_logger_creation() {
        let logger = MCPLogger::new(LogLevel::Debug);
        assert_eq!(logger.min_level(), &LogLevel::Debug);
    }

    #[tokio::test]
    async fn test_log_subscription() {
        let logger = MCPLogger::new(LogLevel::Debug);
        let mut receiver = logger.subscribe();

        logger.info(Some("test".to_string()), json!({"message": "test"}));

        let message = receiver.recv().await.unwrap();
        assert_eq!(message.level, LogLevel::Info);
        assert_eq!(message.logger, Some("test".to_string()));
        assert_eq!(message.data["message"], "test");
    }

    #[test]
    fn test_log_notification_format() {
        let message = LogMessage {
            level: LogLevel::Error,
            logger: Some("memory".to_string()),
            data: json!({"error": "Failed to store memory", "id": "123"}),
        };

        let notification = MCPLogger::create_log_notification(&message);
        assert_eq!(notification["method"], "notifications/message");
        assert_eq!(notification["params"]["level"], "error");
        assert_eq!(notification["params"]["logger"], "memory");
        assert_eq!(notification["params"]["data"]["error"], "Failed to store memory");
    }

    #[tokio::test]
    async fn test_min_level_filtering() {
        let logger = MCPLogger::new(LogLevel::Warning);
        let mut receiver = logger.subscribe();

        // Debug message should be filtered out
        logger.debug(None, json!({"message": "debug"}));
        
        // Warning message should go through
        logger.warning(None, json!({"message": "warning"}));

        // Should only receive the warning message
        let message = receiver.recv().await.unwrap();
        assert_eq!(message.level, LogLevel::Warning);
        assert_eq!(message.data["message"], "warning");

        // No more messages should be available
        match receiver.try_recv() {
            Err(broadcast::error::TryRecvError::Empty) => {}, // Expected
            other => panic!("Expected empty channel, got: {:?}", other),
        }
    }
}