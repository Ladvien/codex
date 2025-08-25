//! MCP Progress Reporting Implementation
//!
//! This module provides progress reporting capabilities for long-running operations
//! as specified in the MCP specification.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

/// Progress report for a long-running operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressReport {
    pub progress_token: String,
    pub progress: f64, // 0.0 to 1.0
    pub total: Option<u64>,
    pub current: Option<u64>,
    pub message: Option<String>,
}

/// Progress tracker for managing multiple ongoing operations
pub struct ProgressTracker {
    operations: Arc<RwLock<std::collections::HashMap<String, ProgressReport>>>,
    sender: broadcast::Sender<ProgressReport>,
}

impl ProgressTracker {
    /// Create a new progress tracker
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(1000);
        Self {
            operations: Arc::new(RwLock::new(std::collections::HashMap::new())),
            sender,
        }
    }

    /// Start tracking a new operation and return a progress token
    pub async fn start_operation(&self, message: Option<String>) -> String {
        let token = Uuid::new_v4().to_string();
        let report = ProgressReport {
            progress_token: token.clone(),
            progress: 0.0,
            total: None,
            current: None,
            message,
        };

        self.operations
            .write()
            .await
            .insert(token.clone(), report.clone());

        // Send initial progress notification
        let _ = self.sender.send(report);

        token
    }

    /// Update progress for an operation
    pub async fn update_progress(
        &self,
        token: &str,
        progress: f64,
        current: Option<u64>,
        total: Option<u64>,
        message: Option<String>,
    ) -> Result<(), String> {
        let mut operations = self.operations.write().await;

        if let Some(report) = operations.get_mut(token) {
            report.progress = progress.clamp(0.0, 1.0);
            report.current = current;
            report.total = total;
            if let Some(msg) = message {
                report.message = Some(msg);
            }

            // Send progress notification
            let _ = self.sender.send(report.clone());
            Ok(())
        } else {
            Err(format!("Progress token not found: {}", token))
        }
    }

    /// Complete an operation and remove it from tracking
    pub async fn complete_operation(&self, token: &str) -> Result<(), String> {
        let mut operations = self.operations.write().await;

        if let Some(mut report) = operations.remove(token) {
            report.progress = 1.0;
            report.message = Some("Operation completed".to_string());

            // Send completion notification
            let _ = self.sender.send(report);
            Ok(())
        } else {
            Err(format!("Progress token not found: {}", token))
        }
    }

    /// Cancel an operation and remove it from tracking
    pub async fn cancel_operation(
        &self,
        token: &str,
        reason: Option<String>,
    ) -> Result<(), String> {
        let mut operations = self.operations.write().await;

        if let Some(mut report) = operations.remove(token) {
            report.message = reason.or_else(|| Some("Operation cancelled".to_string()));

            // Send cancellation notification
            let _ = self.sender.send(report);
            Ok(())
        } else {
            Err(format!("Progress token not found: {}", token))
        }
    }

    /// Get current progress for an operation
    pub async fn get_progress(&self, token: &str) -> Option<ProgressReport> {
        self.operations.read().await.get(token).cloned()
    }

    /// List all active operations
    pub async fn list_operations(&self) -> Vec<ProgressReport> {
        self.operations.read().await.values().cloned().collect()
    }

    /// Subscribe to progress updates
    pub fn subscribe(&self) -> broadcast::Receiver<ProgressReport> {
        self.sender.subscribe()
    }

    /// Create MCP notification message for progress report
    pub fn create_progress_notification(report: &ProgressReport) -> Value {
        json!({
            "jsonrpc": "2.0",
            "method": "notifications/progress",
            "params": {
                "progressToken": report.progress_token,
                "progress": report.progress,
                "total": report.total,
                "current": report.current,
                "message": report.message
            }
        })
    }
}

impl Default for ProgressTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Progress tracking handle for a specific operation
pub struct ProgressHandle {
    tracker: Arc<ProgressTracker>,
    token: String,
}

impl ProgressHandle {
    /// Create a new progress handle
    pub fn new(tracker: Arc<ProgressTracker>, token: String) -> Self {
        Self { tracker, token }
    }

    /// Update progress
    pub async fn update(
        &self,
        progress: f64,
        current: Option<u64>,
        total: Option<u64>,
        message: Option<String>,
    ) -> Result<(), String> {
        self.tracker
            .update_progress(&self.token, progress, current, total, message)
            .await
    }

    /// Complete the operation
    pub async fn complete(&self) -> Result<(), String> {
        self.tracker.complete_operation(&self.token).await
    }

    /// Cancel the operation
    pub async fn cancel(&self, reason: Option<String>) -> Result<(), String> {
        self.tracker.cancel_operation(&self.token, reason).await
    }

    /// Get the progress token
    pub fn token(&self) -> &str {
        &self.token
    }
}

impl Drop for ProgressHandle {
    fn drop(&mut self) {
        // Attempt to complete the operation when the handle is dropped
        // This is fire-and-forget to avoid blocking the drop
        let tracker = self.tracker.clone();
        let token = self.token.clone();
        tokio::spawn(async move {
            let _ = tracker.complete_operation(&token).await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn test_progress_tracker_creation() {
        let tracker = ProgressTracker::new();
        assert_eq!(tracker.list_operations().await.len(), 0);
    }

    #[tokio::test]
    async fn test_operation_lifecycle() {
        let tracker = ProgressTracker::new();

        // Start operation
        let token = tracker
            .start_operation(Some("Test operation".to_string()))
            .await;
        assert_eq!(tracker.list_operations().await.len(), 1);

        // Update progress
        tracker
            .update_progress(
                &token,
                0.5,
                Some(50),
                Some(100),
                Some("Half done".to_string()),
            )
            .await
            .unwrap();

        let progress = tracker.get_progress(&token).await.unwrap();
        assert_eq!(progress.progress, 0.5);
        assert_eq!(progress.current, Some(50));
        assert_eq!(progress.total, Some(100));
        assert_eq!(progress.message, Some("Half done".to_string()));

        // Complete operation
        tracker.complete_operation(&token).await.unwrap();
        assert_eq!(tracker.list_operations().await.len(), 0);
    }

    #[tokio::test]
    async fn test_progress_notifications() {
        let tracker = ProgressTracker::new();
        let mut receiver = tracker.subscribe();

        // Start operation
        let token = tracker.start_operation(Some("Test".to_string())).await;

        // Should receive initial progress
        let report = receiver.recv().await.unwrap();
        assert_eq!(report.progress, 0.0);
        assert_eq!(report.message, Some("Test".to_string()));

        // Update progress
        tracker
            .update_progress(&token, 0.8, None, None, Some("Almost done".to_string()))
            .await
            .unwrap();

        // Should receive update
        let report = receiver.recv().await.unwrap();
        assert_eq!(report.progress, 0.8);
        assert_eq!(report.message, Some("Almost done".to_string()));
    }

    #[tokio::test]
    async fn test_progress_handle() {
        let tracker = Arc::new(ProgressTracker::new());
        let token = tracker
            .start_operation(Some("Handle test".to_string()))
            .await;

        let handle = ProgressHandle::new(tracker.clone(), token.clone());

        // Update through handle
        handle
            .update(0.3, Some(3), Some(10), Some("Progress".to_string()))
            .await
            .unwrap();

        let progress = tracker.get_progress(&token).await.unwrap();
        assert_eq!(progress.progress, 0.3);

        // Complete through handle
        handle.complete().await.unwrap();
        assert!(tracker.get_progress(&token).await.is_none());
    }

    #[tokio::test]
    async fn test_progress_notification_format() {
        let report = ProgressReport {
            progress_token: "test-123".to_string(),
            progress: 0.75,
            total: Some(100),
            current: Some(75),
            message: Some("Processing...".to_string()),
        };

        let notification = ProgressTracker::create_progress_notification(&report);
        assert_eq!(notification["method"], "notifications/progress");
        assert_eq!(notification["params"]["progressToken"], "test-123");
        assert_eq!(notification["params"]["progress"], 0.75);
        assert_eq!(notification["params"]["total"], 100);
        assert_eq!(notification["params"]["current"], 75);
        assert_eq!(notification["params"]["message"], "Processing...");
    }

    #[tokio::test]
    async fn test_handle_drop_completion() {
        let tracker = Arc::new(ProgressTracker::new());
        let token = tracker.start_operation(Some("Drop test".to_string())).await;

        {
            let handle = ProgressHandle::new(tracker.clone(), token.clone());
            handle.update(0.5, None, None, None).await.unwrap();
            // Handle goes out of scope here
        }

        // Give the async drop some time to complete
        sleep(Duration::from_millis(10)).await;

        // Operation should be completed
        assert!(tracker.get_progress(&token).await.is_none());
    }

    #[tokio::test]
    async fn test_progress_clamping() {
        let tracker = ProgressTracker::new();
        let token = tracker.start_operation(None).await;

        // Test progress is clamped to valid range
        tracker
            .update_progress(&token, -0.5, None, None, None)
            .await
            .unwrap();
        let progress = tracker.get_progress(&token).await.unwrap();
        assert_eq!(progress.progress, 0.0);

        tracker
            .update_progress(&token, 1.5, None, None, None)
            .await
            .unwrap();
        let progress = tracker.get_progress(&token).await.unwrap();
        assert_eq!(progress.progress, 1.0);
    }
}
