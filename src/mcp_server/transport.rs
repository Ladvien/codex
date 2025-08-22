//! Stdio Transport Implementation for MCP Protocol
//!
//! This module provides the stdio transport layer for MCP communication,
//! handling JSON-RPC messages over standard input/output streams.

use crate::mcp_server::handlers::MCPHandlers;
use anyhow::Result;
use serde_json::Value;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{debug, error, warn};

/// Stdio transport for MCP protocol
pub struct StdioTransport {
    request_timeout: Duration,
}

impl StdioTransport {
    /// Create a new stdio transport with the specified timeout
    pub fn new(timeout_ms: u64) -> Self {
        Self {
            request_timeout: Duration::from_millis(timeout_ms),
        }
    }

    /// Start the transport layer and begin processing requests
    pub async fn start(&mut self, handlers: &mut MCPHandlers) -> Result<()> {
        debug!("Starting MCP stdio transport");

        // Set up stdin/stdout
        let stdin = tokio::io::stdin();
        let mut reader = BufReader::new(stdin);
        let mut stdout = tokio::io::stdout();

        // Process messages from stdin
        let mut line = String::new();
        loop {
            line.clear();

            // Read line with timeout
            let read_result =
                tokio::time::timeout(self.request_timeout, reader.read_line(&mut line)).await;

            match read_result {
                Ok(Ok(0)) => {
                    debug!("EOF received, shutting down transport");
                    break; // EOF
                }
                Ok(Ok(_)) => {
                    // Process the received message
                    if let Err(e) = self.process_message(&line, handlers, &mut stdout).await {
                        error!("Error processing message: {}", e);
                    }
                }
                Ok(Err(e)) => {
                    error!("IO error reading from stdin: {}", e);
                    break;
                }
                Err(_) => {
                    warn!("Read timeout exceeded, continuing...");
                    continue;
                }
            }
        }

        debug!("Stdio transport stopped");
        Ok(())
    }

    /// Process a single message
    async fn process_message(
        &self,
        line: &str,
        handlers: &mut MCPHandlers,
        stdout: &mut tokio::io::Stdout,
    ) -> Result<()> {
        let line = line.trim();
        if line.is_empty() {
            return Ok(());
        }

        // Parse JSON-RPC message
        let request: Value = match serde_json::from_str(line) {
            Ok(req) => req,
            Err(e) => {
                error!("Failed to parse JSON-RPC request: {}", e);
                self.send_parse_error(stdout, None).await?;
                return Ok(());
            }
        };

        // Extract method and ID
        let method = match request.get("method").and_then(|m| m.as_str()) {
            Some(m) => m,
            None => {
                error!("Missing method in request");
                let id = request.get("id");
                self.send_invalid_request_error(stdout, id).await?;
                return Ok(());
            }
        };

        let id = request.get("id");

        // Skip notifications (no response needed)
        if method.starts_with("notifications/") {
            debug!("Received notification: {}", method);
            return Ok(());
        }

        // Process request with timeout
        let response = tokio::time::timeout(
            self.request_timeout,
            handlers.handle_request(method, request.get("params"), id),
        )
        .await;

        match response {
            Ok(resp) => {
                self.send_response(stdout, &resp).await?;
            }
            Err(_) => {
                error!("Request processing timeout for method: {}", method);
                self.send_timeout_error(stdout, id).await?;
            }
        }

        Ok(())
    }

    /// Send a response to stdout
    async fn send_response(&self, stdout: &mut tokio::io::Stdout, response: &Value) -> Result<()> {
        let response_str = serde_json::to_string(response)?;
        stdout.write_all(response_str.as_bytes()).await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await?;

        debug!(
            "Sent response: {}",
            response_str.chars().take(200).collect::<String>()
        );
        Ok(())
    }

    /// Send a parse error response
    async fn send_parse_error(
        &self,
        stdout: &mut tokio::io::Stdout,
        id: Option<&Value>,
    ) -> Result<()> {
        let error_response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": -32700,
                "message": "Parse error"
            }
        });
        self.send_response(stdout, &error_response).await
    }

    /// Send an invalid request error
    async fn send_invalid_request_error(
        &self,
        stdout: &mut tokio::io::Stdout,
        id: Option<&Value>,
    ) -> Result<()> {
        let error_response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": -32600,
                "message": "Invalid Request"
            }
        });
        self.send_response(stdout, &error_response).await
    }

    /// Send a timeout error
    async fn send_timeout_error(
        &self,
        stdout: &mut tokio::io::Stdout,
        id: Option<&Value>,
    ) -> Result<()> {
        let error_response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": -32603,
                "message": "Request timeout"
            }
        });
        self.send_response(stdout, &error_response).await
    }
}

/// Helper function to create JSON-RPC error responses
pub fn create_error_response(id: Option<&Value>, code: i32, message: &str) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    })
}

/// Helper function to create JSON-RPC success responses  
pub fn create_success_response(id: Option<&Value>, result: Value) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    })
}

/// Helper function to format content for MCP tool responses
pub fn format_tool_response(text: &str) -> Value {
    serde_json::json!({
        "content": [
            {
                "type": "text",
                "text": text
            }
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_success_response() {
        let id_value = serde_json::json!(1);
        let id = Some(&id_value);
        let result = serde_json::json!({"status": "ok"});

        let response = create_success_response(id, result);

        assert_eq!(response["jsonrpc"], "2.0");
        assert_eq!(response["id"], 1);
        assert_eq!(response["result"]["status"], "ok");
    }

    #[test]
    fn test_create_error_response() {
        let id_value = serde_json::json!("test-id");
        let id = Some(&id_value);

        let response = create_error_response(id, -32601, "Method not found");

        assert_eq!(response["jsonrpc"], "2.0");
        assert_eq!(response["id"], "test-id");
        assert_eq!(response["error"]["code"], -32601);
        assert_eq!(response["error"]["message"], "Method not found");
    }

    #[test]
    fn test_format_tool_response() {
        let response = format_tool_response("Test message");

        assert_eq!(response["content"][0]["type"], "text");
        assert_eq!(response["content"][0]["text"], "Test message");
    }

    #[tokio::test]
    async fn test_transport_creation() {
        let transport = StdioTransport::new(5000);
        assert_eq!(transport.request_timeout, Duration::from_millis(5000));
    }
}
