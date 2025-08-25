//! Stdio Transport Implementation for MCP Protocol
//!
//! This module provides the stdio transport layer for MCP communication,
//! handling JSON-RPC messages over standard input/output streams with
//! transport-level security including rate limiting and connection throttling.

use crate::mcp_server::{handlers::MCPHandlers, rate_limiter::MCPRateLimiter};
use crate::security::SecurityError;
use anyhow::Result;
use governor::{
    clock::DefaultClock,
    middleware::NoOpMiddleware,
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter as GovernorRateLimiter,
};
use nonzero_ext::nonzero;
use serde_json::Value;
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::RwLock;
use tracing::{debug, error, warn};

/// Transport-level connection state tracking
#[derive(Debug)]
struct ConnectionState {
    malformed_requests: u32,
    last_malformed: Option<Instant>,
    total_requests: u64,
    first_request: Instant,
    backoff_until: Option<Instant>,
}

impl Default for ConnectionState {
    fn default() -> Self {
        Self {
            malformed_requests: 0,
            last_malformed: None,
            total_requests: 0,
            first_request: Instant::now(),
            backoff_until: None,
        }
    }
}

/// Transport-level rate limiting configuration
#[derive(Debug, Clone)]
pub struct TransportRateLimitConfig {
    /// Maximum malformed requests before backoff
    pub max_malformed_requests: u32,
    /// Backoff duration for malformed requests (exponential)
    pub malformed_backoff_base_ms: u64,
    /// Maximum backoff duration
    pub max_backoff_duration_ms: u64,
    /// Reset period for malformed request counting
    pub malformed_reset_period_ms: u64,
    /// Maximum message size in bytes
    pub max_message_size: usize,
    /// Transport-level requests per minute (before parsing)
    pub transport_requests_per_minute: u32,
    /// Transport-level burst size
    pub transport_burst_size: u32,
}

impl Default for TransportRateLimitConfig {
    fn default() -> Self {
        Self {
            max_malformed_requests: 5,
            malformed_backoff_base_ms: 1000,    // Start with 1 second
            max_backoff_duration_ms: 60_000,    // Max 1 minute
            malformed_reset_period_ms: 300_000, // Reset after 5 minutes
            max_message_size: 1_048_576,        // 1MB
            transport_requests_per_minute: 120, // 2 per second at transport level
            transport_burst_size: 10,
        }
    }
}

/// Stdio transport for MCP protocol with security hardening
pub struct StdioTransport {
    request_timeout: Duration,
    connection_state: Arc<RwLock<ConnectionState>>,
    transport_config: TransportRateLimitConfig,
    transport_limiter:
        Arc<GovernorRateLimiter<NotKeyed, InMemoryState, DefaultClock, NoOpMiddleware>>,
}

impl StdioTransport {
    /// Create a new stdio transport with the specified timeout
    pub fn new(timeout_ms: u64) -> Result<Self> {
        let transport_config = TransportRateLimitConfig::default();

        // Create transport-level rate limiter
        let rate = NonZeroU32::new(transport_config.transport_requests_per_minute)
            .ok_or_else(|| anyhow::anyhow!("Invalid transport rate limit"))?;
        let burst = NonZeroU32::new(transport_config.transport_burst_size)
            .ok_or_else(|| anyhow::anyhow!("Invalid transport burst size"))?;
        let quota = Quota::per_minute(rate).allow_burst(burst);
        let transport_limiter = Arc::new(GovernorRateLimiter::direct(quota));

        Ok(Self {
            request_timeout: Duration::from_millis(timeout_ms),
            connection_state: Arc::new(RwLock::new(ConnectionState::default())),
            transport_config,
            transport_limiter,
        })
    }

    /// Create a new stdio transport with custom configuration
    pub fn new_with_config(
        timeout_ms: u64,
        transport_config: TransportRateLimitConfig,
    ) -> Result<Self> {
        // Create transport-level rate limiter
        let rate = NonZeroU32::new(transport_config.transport_requests_per_minute)
            .ok_or_else(|| anyhow::anyhow!("Invalid transport rate limit"))?;
        let burst = NonZeroU32::new(transport_config.transport_burst_size)
            .ok_or_else(|| anyhow::anyhow!("Invalid transport burst size"))?;
        let quota = Quota::per_minute(rate).allow_burst(burst);
        let transport_limiter = Arc::new(GovernorRateLimiter::direct(quota));

        Ok(Self {
            request_timeout: Duration::from_millis(timeout_ms),
            connection_state: Arc::new(RwLock::new(ConnectionState::default())),
            transport_config,
            transport_limiter,
        })
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

    /// Check transport-level security before processing messages
    async fn check_transport_security(&self, message: &str) -> Result<()> {
        let now = Instant::now();

        // Check transport-level rate limiting first
        if self.transport_limiter.check().is_err() {
            warn!("Transport-level rate limit exceeded");
            return Err(SecurityError::RateLimitExceeded.into());
        }

        let mut state = self.connection_state.write().await;
        state.total_requests += 1;

        // Check if we're in backoff period
        if let Some(backoff_until) = state.backoff_until {
            if now < backoff_until {
                warn!("Connection in backoff period until {:?}", backoff_until);
                return Err(SecurityError::RateLimitExceeded.into());
            } else {
                // Reset backoff
                state.backoff_until = None;
                debug!("Backoff period expired, resuming normal processing");
            }
        }

        // Check message size limit
        if message.len() > self.transport_config.max_message_size {
            warn!(
                "Message size {} exceeds limit {}",
                message.len(),
                self.transport_config.max_message_size
            );
            self.handle_malformed_request(&mut state, now).await?;
            return Err(anyhow::anyhow!("Message too large"));
        }

        // Reset malformed request counter if enough time has passed
        if let Some(last_malformed) = state.last_malformed {
            if now.duration_since(last_malformed).as_millis()
                > self.transport_config.malformed_reset_period_ms as u128
            {
                debug!("Resetting malformed request counter after timeout");
                state.malformed_requests = 0;
                state.last_malformed = None;
            }
        }

        Ok(())
    }

    /// Handle malformed requests with exponential backoff
    async fn handle_malformed_request(
        &self,
        state: &mut ConnectionState,
        now: Instant,
    ) -> Result<()> {
        state.malformed_requests += 1;
        state.last_malformed = Some(now);

        warn!(
            "Malformed request detected, count: {}/{}",
            state.malformed_requests, self.transport_config.max_malformed_requests
        );

        if state.malformed_requests >= self.transport_config.max_malformed_requests {
            // Calculate exponential backoff
            let backoff_multiplier = 2_u64.pow(
                (state.malformed_requests - self.transport_config.max_malformed_requests).min(10),
            );
            let backoff_ms = (self.transport_config.malformed_backoff_base_ms * backoff_multiplier)
                .min(self.transport_config.max_backoff_duration_ms);

            state.backoff_until = Some(now + Duration::from_millis(backoff_ms));

            warn!(
                "Too many malformed requests ({}), entering backoff for {}ms",
                state.malformed_requests, backoff_ms
            );

            return Err(SecurityError::RateLimitExceeded.into());
        }

        Ok(())
    }

    /// Process a single message, handling both single requests and batch requests
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

        // SECURITY: Check transport-level security BEFORE parsing JSON
        // This prevents malformed requests from bypassing rate limits
        if let Err(e) = self.check_transport_security(line).await {
            warn!("Transport security check failed: {}", e);
            // Don't send response for rate limited requests to avoid amplifying attacks
            return Ok(());
        }

        // Parse JSON-RPC message
        let request: Value = match serde_json::from_str(line) {
            Ok(req) => req,
            Err(e) => {
                error!("Failed to parse JSON-RPC request: {}", e);

                // SECURITY: Track malformed JSON requests
                let mut state = self.connection_state.write().await;
                if let Err(security_err) = self
                    .handle_malformed_request(&mut state, Instant::now())
                    .await
                {
                    warn!(
                        "Malformed request triggered security backoff: {}",
                        security_err
                    );
                    // Don't send parse error response during backoff
                    return Ok(());
                }

                self.send_parse_error(stdout, None).await?;
                return Ok(());
            }
        };

        // Handle batch requests (array) vs single requests (object)
        if request.is_array() {
            self.process_batch_request(&request, handlers, stdout)
                .await?;
        } else {
            self.process_single_request(&request, handlers, stdout)
                .await?;
        }

        Ok(())
    }

    /// Process a batch of JSON-RPC requests
    async fn process_batch_request(
        &self,
        requests: &Value,
        handlers: &mut MCPHandlers,
        stdout: &mut tokio::io::Stdout,
    ) -> Result<()> {
        let request_array = match requests.as_array() {
            Some(arr) if !arr.is_empty() => arr,
            _ => {
                // Empty batch is invalid
                self.send_invalid_request_error(stdout, None).await?;
                return Ok(());
            }
        };

        let mut responses = Vec::new();

        for request in request_array {
            // Process each request in the batch
            let response = self
                .process_single_request_internal(request, handlers)
                .await;

            // Only add non-notification responses to the batch
            if let Some(resp) = response {
                responses.push(resp);
            }
        }

        // Send batch response (only if we have responses)
        if !responses.is_empty() {
            let batch_response = Value::Array(responses);
            self.send_response(stdout, &batch_response).await?;
        }

        Ok(())
    }

    /// Process a single JSON-RPC request
    async fn process_single_request(
        &self,
        request: &Value,
        handlers: &mut MCPHandlers,
        stdout: &mut tokio::io::Stdout,
    ) -> Result<()> {
        if let Some(response) = self
            .process_single_request_internal(request, handlers)
            .await
        {
            self.send_response(stdout, &response).await?;
        }
        Ok(())
    }

    /// Process a single request and return the response (if any)
    async fn process_single_request_internal(
        &self,
        request: &Value,
        handlers: &mut MCPHandlers,
    ) -> Option<Value> {
        // Validate JSON-RPC structure according to specification
        if let Err(validation_error) = self.validate_jsonrpc_request(request) {
            error!("JSON-RPC validation failed: {}", validation_error);
            let id = request.get("id");
            return Some(create_error_response(id, -32600, "Invalid Request"));
        }

        // Extract method and ID
        let method = match request.get("method").and_then(|m| m.as_str()) {
            Some(m) => m,
            None => {
                error!("Missing method in request");
                let id = request.get("id");
                return Some(create_error_response(id, -32600, "Invalid Request"));
            }
        };

        let id = request.get("id");

        // Handle notifications (no response needed) - proper JSON-RPC 2.0 notification detection
        if id.is_none() {
            debug!("Received JSON-RPC notification: {}", method);
            self.handle_notification(method, request.get("params"))
                .await;
            return None;
        }

        // Extract headers from JSON-RPC extensions (if any)
        let headers = self.extract_headers_from_request(request);

        // Process request with timeout
        let response = tokio::time::timeout(
            self.request_timeout,
            handlers.handle_request_with_headers(method, request.get("params"), id, &headers),
        )
        .await;

        match response {
            Ok(resp) => Some(resp),
            Err(_) => {
                error!("Request processing timeout for method: {}", method);
                Some(create_error_response_with_data(
                    id,
                    -32603,
                    "Internal error",
                    Some(serde_json::json!({
                        "type": "timeout",
                        "details": "Request processing timeout exceeded"
                    })),
                ))
            }
        }
    }

    /// Handle JSON-RPC 2.0 notifications (no response expected)
    async fn handle_notification(&self, method: &str, params: Option<&Value>) {
        debug!(
            "Processing notification: {} with params: {:?}",
            method, params
        );

        match method {
            "notifications/initialized" => {
                debug!("Client initialized notification received");
            }
            "notifications/cancelled" => {
                if let Some(params) = params {
                    if let Some(request_id) = params.get("requestId") {
                        debug!("Request cancellation notification: {:?}", request_id);
                        // TODO: Implement request cancellation logic
                    }
                }
            }
            _ => {
                debug!("Unknown notification method: {}", method);
            }
        }
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
        let error_response = create_error_response_with_data(
            id,
            -32603,
            "Internal error",
            Some(serde_json::json!({
                "type": "timeout",
                "details": "Request processing timeout exceeded"
            })),
        );
        self.send_response(stdout, &error_response).await
    }

    /// Send an internal error response
    async fn send_internal_error(
        &self,
        stdout: &mut tokio::io::Stdout,
        id: Option<&Value>,
        details: &str,
    ) -> Result<()> {
        let error_response = create_error_response_with_data(
            id,
            -32603,
            "Internal error",
            Some(serde_json::json!({
                "type": "internal",
                "details": details
            })),
        );
        self.send_response(stdout, &error_response).await
    }

    /// Validate JSON-RPC request structure according to specification
    pub fn validate_jsonrpc_request(&self, request: &Value) -> Result<(), String> {
        // Check required jsonrpc field
        match request.get("jsonrpc") {
            Some(version) => {
                if version.as_str() != Some("2.0") {
                    return Err("Invalid JSON-RPC version, must be '2.0'".to_string());
                }
            }
            None => {
                return Err("Missing required 'jsonrpc' field".to_string());
            }
        }

        // Validate method field exists (for regular requests)
        if request.get("method").is_none() {
            return Err("Missing required 'method' field".to_string());
        }

        // Validate method is a string
        if let Some(method) = request.get("method") {
            if method.as_str().is_none() {
                return Err("Method field must be a string".to_string());
            }
        }

        // Validate id field if present (can be string, number, or null)
        if let Some(id) = request.get("id") {
            if !id.is_string() && !id.is_number() && !id.is_null() {
                return Err("ID field must be a string, number, or null".to_string());
            }
        }

        // Validate params field if present (must be object or array)
        if let Some(params) = request.get("params") {
            if !params.is_object() && !params.is_array() {
                return Err("Params field must be an object or array".to_string());
            }
        }

        Ok(())
    }

    /// Extract headers from JSON-RPC request extensions or provide defaults for stdio
    fn extract_headers_from_request(&self, request: &Value) -> HashMap<String, String> {
        let mut headers = HashMap::new();

        // Add default headers for stdio transport
        headers.insert("Content-Type".to_string(), "application/json".to_string());
        headers.insert("Transport".to_string(), "stdio".to_string());

        // Extract any custom headers from JSON-RPC extensions
        if let Some(extensions) = request.get("extensions") {
            if let Some(ext_headers) = extensions.get("headers") {
                if let Some(header_obj) = ext_headers.as_object() {
                    for (key, value) in header_obj {
                        if let Some(value_str) = value.as_str() {
                            headers.insert(key.clone(), value_str.to_string());
                        }
                    }
                }
            }
        }

        // Validate JSON-RPC version is carried in headers for authentication
        if let Some(jsonrpc) = request.get("jsonrpc") {
            if let Some(version) = jsonrpc.as_str() {
                headers.insert("JSON-RPC-Version".to_string(), version.to_string());
            }
        }

        headers
    }
}

/// Helper function to create JSON-RPC error responses with optional data
pub fn create_error_response(id: Option<&Value>, code: i32, message: &str) -> Value {
    create_error_response_with_data(id, code, message, None)
}

/// Helper function to create JSON-RPC error responses with data field
pub fn create_error_response_with_data(
    id: Option<&Value>,
    code: i32,
    message: &str,
    data: Option<Value>,
) -> Value {
    let mut error = serde_json::json!({
        "code": code,
        "message": message
    });

    if let Some(error_data) = data {
        error["data"] = error_data;
    }

    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": error
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

/// Helper function to format content for MCP tool responses with text content
pub fn format_tool_response(text: &str) -> Value {
    format_tool_response_with_content(vec![create_text_content(text, None)])
}

/// Helper function to create MCP text content
pub fn create_text_content(text: &str, annotations: Option<Value>) -> Value {
    let mut content = serde_json::json!({
        "type": "text",
        "text": text
    });

    if let Some(annotations) = annotations {
        content["annotations"] = annotations;
    }

    content
}

/// Helper function to create MCP image content
pub fn create_image_content(data: &str, mime_type: &str, annotations: Option<Value>) -> Value {
    let mut content = serde_json::json!({
        "type": "image",
        "data": data,
        "mimeType": mime_type
    });

    if let Some(annotations) = annotations {
        content["annotations"] = annotations;
    }

    content
}

/// Helper function to create MCP resource content
pub fn create_resource_content(
    uri: &str,
    mime_type: Option<&str>,
    text: Option<&str>,
    annotations: Option<Value>,
) -> Value {
    let mut content = serde_json::json!({
        "type": "resource",
        "resource": {
            "uri": uri
        }
    });

    if let Some(mime_type) = mime_type {
        content["resource"]["mimeType"] = mime_type.into();
    }

    if let Some(text) = text {
        content["resource"]["text"] = text.into();
    }

    if let Some(annotations) = annotations {
        content["annotations"] = annotations;
    }

    content
}

/// Helper function to format MCP tool responses with multiple content types
pub fn format_tool_response_with_content(content: Vec<Value>) -> Value {
    serde_json::json!({
        "content": content,
        "isError": false
    })
}

/// Helper function to format MCP tool error responses
pub fn format_tool_error_response(text: &str, is_error: bool) -> Value {
    serde_json::json!({
        "content": [
            {
                "type": "text",
                "text": text
            }
        ],
        "isError": is_error
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
        let transport = StdioTransport::new(5000).unwrap();
        assert_eq!(transport.request_timeout, Duration::from_millis(5000));
    }

    #[test]
    fn test_validate_jsonrpc_request_valid() {
        let transport = StdioTransport::new(5000).unwrap();

        let valid_request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "initialize",
            "id": 1
        });

        assert!(transport.validate_jsonrpc_request(&valid_request).is_ok());
    }

    #[test]
    fn test_validate_jsonrpc_request_missing_jsonrpc() {
        let transport = StdioTransport::new(5000).unwrap();

        let invalid_request = serde_json::json!({
            "method": "initialize",
            "id": 1
        });

        let result = transport.validate_jsonrpc_request(&invalid_request);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Missing required 'jsonrpc' field"));
    }

    #[test]
    fn test_validate_jsonrpc_request_wrong_version() {
        let transport = StdioTransport::new(5000).unwrap();

        let invalid_request = serde_json::json!({
            "jsonrpc": "1.0",
            "method": "initialize",
            "id": 1
        });

        let result = transport.validate_jsonrpc_request(&invalid_request);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid JSON-RPC version"));
    }

    #[test]
    fn test_validate_jsonrpc_request_missing_method() {
        let transport = StdioTransport::new(5000).unwrap();

        let invalid_request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1
        });

        let result = transport.validate_jsonrpc_request(&invalid_request);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Missing required 'method' field"));
    }

    #[test]
    fn test_validate_jsonrpc_request_invalid_method_type() {
        let transport = StdioTransport::new(5000).unwrap();

        let invalid_request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": 123,
            "id": 1
        });

        let result = transport.validate_jsonrpc_request(&invalid_request);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Method field must be a string"));
    }

    #[test]
    fn test_validate_jsonrpc_request_invalid_id_type() {
        let transport = StdioTransport::new(5000).unwrap();

        let invalid_request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "initialize",
            "id": true
        });

        let result = transport.validate_jsonrpc_request(&invalid_request);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("ID field must be a string, number, or null"));
    }

    #[test]
    fn test_validate_jsonrpc_request_invalid_params_type() {
        let transport = StdioTransport::new(5000).unwrap();

        let invalid_request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "initialize",
            "id": 1,
            "params": "invalid"
        });

        let result = transport.validate_jsonrpc_request(&invalid_request);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Params field must be an object or array"));
    }

    #[test]
    fn test_validate_jsonrpc_request_valid_with_object_params() {
        let transport = StdioTransport::new(5000).unwrap();

        let valid_request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "tools/call",
            "id": 1,
            "params": {"name": "test", "arguments": {}}
        });

        assert!(transport.validate_jsonrpc_request(&valid_request).is_ok());
    }

    #[test]
    fn test_validate_jsonrpc_request_valid_with_array_params() {
        let transport = StdioTransport::new(5000).unwrap();

        let valid_request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "batch_call",
            "id": 1,
            "params": [1, 2, 3]
        });

        assert!(transport.validate_jsonrpc_request(&valid_request).is_ok());
    }

    #[test]
    fn test_extract_headers_from_request() {
        let transport = StdioTransport::new(5000).unwrap();

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "initialize",
            "id": 1,
            "extensions": {
                "headers": {
                    "Authorization": "Bearer token123",
                    "User-Agent": "TestClient/1.0"
                }
            }
        });

        let headers = transport.extract_headers_from_request(&request);

        assert_eq!(
            headers.get("Content-Type"),
            Some(&"application/json".to_string())
        );
        assert_eq!(headers.get("Transport"), Some(&"stdio".to_string()));
        assert_eq!(headers.get("JSON-RPC-Version"), Some(&"2.0".to_string()));
        assert_eq!(
            headers.get("Authorization"),
            Some(&"Bearer token123".to_string())
        );
        assert_eq!(
            headers.get("User-Agent"),
            Some(&"TestClient/1.0".to_string())
        );
    }

    #[test]
    fn test_extract_headers_defaults_only() {
        let transport = StdioTransport::new(5000).unwrap();

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "initialize",
            "id": 1
        });

        let headers = transport.extract_headers_from_request(&request);

        assert_eq!(
            headers.get("Content-Type"),
            Some(&"application/json".to_string())
        );
        assert_eq!(headers.get("Transport"), Some(&"stdio".to_string()));
        assert_eq!(headers.get("JSON-RPC-Version"), Some(&"2.0".to_string()));
        assert_eq!(headers.len(), 3);
    }
}
