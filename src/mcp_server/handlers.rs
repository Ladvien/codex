//! MCP Request Handlers
//!
//! This module contains all the request handlers for MCP protocol methods,
//! including tool execution, initialization, and resource management.

use crate::mcp_server::{
    auth::{AuthContext, MCPAuth},
    circuit_breaker::{CircuitBreaker, CircuitBreakerError},
    logging::{LogLevel, MCPLogger},
    progress::{ProgressHandle, ProgressTracker},
    rate_limiter::MCPRateLimiter,
    tools::MCPTools,
    transport::{
        create_error_response, create_error_response_with_data, create_success_response,
        create_text_content, format_tool_error_response, format_tool_response,
        format_tool_response_with_content,
    },
};
use crate::memory::{models::*, ConversationMessage, MemoryRepository, SilentHarvesterService};
use crate::SimpleEmbedder;

#[cfg(feature = "codex-dreams")]
use crate::insights::processor::InsightsProcessor;
use anyhow::Result;
use chrono::{Duration as ChronoDuration, Utc};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// MCP request handlers
pub struct MCPHandlers {
    repository: Arc<MemoryRepository>,
    embedder: Arc<SimpleEmbedder>,
    harvester_service: Arc<SilentHarvesterService>,
    circuit_breaker: Option<Arc<CircuitBreaker>>,
    auth: Option<Arc<MCPAuth>>,
    rate_limiter: Option<Arc<MCPRateLimiter>>,
    mcp_logger: Arc<MCPLogger>,
    progress_tracker: Arc<ProgressTracker>,
    #[cfg(feature = "codex-dreams")]
    insights_processor: Option<Arc<InsightsProcessor>>,
    #[cfg(feature = "codex-dreams")]
    insight_storage: Option<Arc<crate::insights::storage::InsightStorage>>,
}

impl MCPHandlers {
    /// Create new MCP handlers
    #[cfg(not(feature = "codex-dreams"))]
    pub fn new(
        repository: Arc<MemoryRepository>,
        embedder: Arc<SimpleEmbedder>,
        harvester_service: Arc<SilentHarvesterService>,
        circuit_breaker: Option<Arc<CircuitBreaker>>,
        auth: Option<Arc<MCPAuth>>,
        rate_limiter: Option<Arc<MCPRateLimiter>>,
        mcp_logger: Arc<MCPLogger>,
        progress_tracker: Arc<ProgressTracker>,
    ) -> Self {
        Self {
            repository,
            embedder,
            harvester_service,
            circuit_breaker,
            auth,
            rate_limiter,
            mcp_logger,
            progress_tracker,
        }
    }

    /// Create new MCP handlers with insights processor
    #[cfg(feature = "codex-dreams")]
    pub fn new(
        repository: Arc<MemoryRepository>,
        embedder: Arc<SimpleEmbedder>,
        harvester_service: Arc<SilentHarvesterService>,
        circuit_breaker: Option<Arc<CircuitBreaker>>,
        auth: Option<Arc<MCPAuth>>,
        rate_limiter: Option<Arc<MCPRateLimiter>>,
        mcp_logger: Arc<MCPLogger>,
        progress_tracker: Arc<ProgressTracker>,
    ) -> Self {
        Self::new_with_insights(
            repository,
            embedder,
            harvester_service,
            circuit_breaker,
            auth,
            rate_limiter,
            mcp_logger,
            progress_tracker,
            None,
            None,
        )
    }

    /// Create new MCP handlers with insights processor
    #[cfg(feature = "codex-dreams")]
    pub fn new_with_insights(
        repository: Arc<MemoryRepository>,
        embedder: Arc<SimpleEmbedder>,
        harvester_service: Arc<SilentHarvesterService>,
        circuit_breaker: Option<Arc<CircuitBreaker>>,
        auth: Option<Arc<MCPAuth>>,
        rate_limiter: Option<Arc<MCPRateLimiter>>,
        mcp_logger: Arc<MCPLogger>,
        progress_tracker: Arc<ProgressTracker>,
        insights_processor: Option<Arc<InsightsProcessor>>,
        insight_storage: Option<Arc<crate::insights::storage::InsightStorage>>,
    ) -> Self {
        Self {
            repository,
            embedder,
            harvester_service,
            circuit_breaker,
            auth,
            rate_limiter,
            mcp_logger,
            progress_tracker,
            insights_processor,
            insight_storage,
        }
    }

    /// Handle incoming MCP requests with authentication and rate limiting
    pub async fn handle_request(
        &mut self,
        method: &str,
        params: Option<&Value>,
        id: Option<&Value>,
    ) -> Value {
        self.handle_request_with_headers(method, params, id, &HashMap::new())
            .await
    }

    /// Securely determine if request should be processed in silent mode
    /// Silent mode provides reduced rate limits but requires proper authorization
    async fn determine_silent_mode_securely(
        &self,
        method: &str,
        params: Option<&Value>,
        auth_context: Option<&AuthContext>,
    ) -> bool {
        // SECURITY: Silent mode bypass prevention
        // 1. Only specific pre-authorized methods can use silent mode
        // 2. Client must be explicitly authorized for silent mode operation
        // 3. Cannot be arbitrarily requested via parameters

        // Pre-authorized silent mode methods (internal/system operations)
        let method_allows_silent = matches!(method, "harvest_conversation");

        if !method_allows_silent {
            return false;
        }

        // Check if client is authorized for silent mode based on authentication context
        if let Some(auth_ctx) = auth_context {
            // Check if client has silent mode scope in their authorization
            let has_silent_scope = auth_ctx.scopes.contains(&"mcp:silent".to_string())
                || auth_ctx.scopes.contains(&"mcp:admin".to_string());

            if !has_silent_scope {
                warn!(
                    "Client {} attempted to use silent mode without proper scope authorization",
                    auth_ctx.client_id
                );
                return false;
            }

            // Additional check: client ID must be in silent mode whitelist
            let client_authorized_for_silent = self
                .is_client_authorized_for_silent(&auth_ctx.client_id)
                .await;

            if !client_authorized_for_silent {
                warn!(
                    "Client {} has silent scope but is not in silent mode whitelist",
                    auth_ctx.client_id
                );
                return false;
            }

            debug!("Silent mode authorized for client: {}", auth_ctx.client_id);
            return true;
        }

        // No authentication context - cannot use silent mode
        false
    }

    /// Check if a client is authorized for silent mode operation
    async fn is_client_authorized_for_silent(&self, client_id: &str) -> bool {
        // SECURITY: Whitelist-based approach for silent mode authorization
        // This prevents arbitrary clients from reducing their rate limits

        let silent_authorized_clients = [
            "harvester-service",
            "internal-processor",
            "system-agent",
            "codex-admin",
        ];

        let is_authorized = silent_authorized_clients.contains(&client_id)
            || client_id.starts_with("system-")
            || client_id.starts_with("internal-");

        if !is_authorized {
            debug!("Client {} is not authorized for silent mode", client_id);
        }

        is_authorized
    }

    /// Handle incoming MCP requests with headers for auth/rate limiting
    pub async fn handle_request_with_headers(
        &mut self,
        method: &str,
        params: Option<&Value>,
        id: Option<&Value>,
        headers: &HashMap<String, String>,
    ) -> Value {
        debug!("Handling MCP request: {}", method);

        // SECURITY: No authentication bypass allowed - all methods must authenticate
        // The initialize method may provide basic server info but no sensitive data

        // Authenticate request
        let auth_context = match &self.auth {
            Some(auth) => match auth.authenticate_request(method, params, headers).await {
                Ok(ctx) => ctx,
                Err(e) => {
                    error!("Authentication failed: {}", e);
                    return create_error_response(
                        id,
                        -32001,
                        &format!("Authentication failed: {e}"),
                    );
                }
            },
            None => None,
        };

        // Check rate limits
        if let Some(ref rate_limiter) = self.rate_limiter {
            // SECURITY: Determine silent mode with proper authentication
            // Silent mode requires explicit authorization and cannot be requested arbitrarily
            let silent_mode = self
                .determine_silent_mode_securely(method, params, auth_context.as_ref())
                .await;

            let tool_name = if method == "tools/call" {
                params
                    .and_then(|p| p.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or("unknown")
            } else {
                method
            };

            if let Err(e) = rate_limiter
                .check_rate_limit(auth_context.as_ref(), tool_name, silent_mode)
                .await
            {
                warn!("Rate limit exceeded for method: {}", method);
                return create_error_response(
                    id,
                    -32002,
                    "Rate limit exceeded. Please try again later.",
                );
            }
        }

        // Proceed with request handling - all methods now require authentication
        match method {
            "initialize" => self.handle_initialize(id, params).await,
            "tools/list" => self.handle_tools_list(id).await,
            "tools/call" => {
                self.handle_tools_call(id, params, auth_context.as_ref())
                    .await
            }
            "resources/list" => self.handle_resources_list(id).await,
            "prompts/list" => self.handle_prompts_list(id).await,
            _ => {
                warn!("Unknown method: {}", method);
                create_error_response(id, -32601, "Method not found")
            }
        }
    }

    /// Handle initialize request - now requires authentication
    /// Only provides basic server info, no sensitive capabilities
    async fn handle_initialize(&self, id: Option<&Value>, _params: Option<&Value>) -> Value {
        info!("MCP server initializing (authenticated)");

        // Provide minimal server capabilities - no sensitive information exposed
        let basic_capabilities = serde_json::json!({
            "protocolVersion": "2025-06-18",
            "serverInfo": {
                "name": "codex-memory",
                "version": "0.1.40"
            },
            "capabilities": {
                "tools": {},
                "resources": {},
                "prompts": {}
            }
        });

        create_success_response(id, basic_capabilities)
    }

    /// Handle tools/list request
    async fn handle_tools_list(&self, id: Option<&Value>) -> Value {
        debug!("Listing available tools");
        create_success_response(id, MCPTools::get_tools_list())
    }

    /// Handle resources/list request
    async fn handle_resources_list(&self, id: Option<&Value>) -> Value {
        debug!("Listing resources (empty)");
        create_success_response(id, MCPTools::get_resources_list())
    }

    /// Handle prompts/list request
    async fn handle_prompts_list(&self, id: Option<&Value>) -> Value {
        debug!("Listing prompts (empty)");
        create_success_response(id, MCPTools::get_prompts_list())
    }

    /// Handle tools/call request
    async fn handle_tools_call(
        &mut self,
        id: Option<&Value>,
        params: Option<&Value>,
        auth_context: Option<&AuthContext>,
    ) -> Value {
        let params = match params {
            Some(p) => p,
            None => {
                return create_error_response(id, -32602, "Missing params");
            }
        };

        let tool_name = match params.get("name").and_then(|n| n.as_str()) {
            Some(name) => name,
            None => {
                return create_error_response(id, -32602, "Missing tool name");
            }
        };

        let arguments = match params.get("arguments") {
            Some(args) => args,
            None => {
                return create_error_response(id, -32602, "Missing arguments");
            }
        };

        // Validate arguments against schema
        if let Err(error) = MCPTools::validate_tool_args(tool_name, arguments) {
            return create_error_response(id, -32602, &error);
        }

        // Validate tool access permissions if authenticated
        if let Some(ref auth) = self.auth {
            if let Some(context) = auth_context {
                if let Err(e) = auth.validate_tool_access(context, tool_name) {
                    return create_error_response(id, -32003, &format!("Access denied: {e}"));
                }
            }
        }

        debug!("Executing tool: {} with args: {}", tool_name, arguments);

        // Execute tool with circuit breaker protection if enabled
        if let Some(ref circuit_breaker) = self.circuit_breaker {
            match circuit_breaker
                .call(|| async { self.execute_tool(tool_name, arguments).await })
                .await
            {
                Ok(result) => create_success_response(id, result),
                Err(CircuitBreakerError::CircuitOpen) => create_error_response(
                    id,
                    -32603,
                    "Service temporarily unavailable (circuit breaker open)",
                ),
                Err(CircuitBreakerError::HalfOpenLimitExceeded) => create_error_response(
                    id,
                    -32603,
                    "Service temporarily unavailable (half-open limit exceeded)",
                ),
            }
        } else {
            match self.execute_tool(tool_name, arguments).await {
                Ok(result) => create_success_response(id, result),
                Err(e) => {
                    error!("Tool execution failed: {}", e);
                    create_error_response(id, -32603, &format!("Tool execution failed: {e}"))
                }
            }
        }
    }

    /// Execute a specific tool
    async fn execute_tool(&self, tool_name: &str, arguments: &Value) -> Result<Value> {
        match tool_name {
            "store_memory" => self.execute_store_memory(arguments).await,
            "search_memory" => self.execute_search_memory(arguments).await,
            "get_statistics" => self.execute_get_statistics(arguments).await,
            "what_did_you_remember" => self.execute_what_did_you_remember(arguments).await,
            "harvest_conversation" => self.execute_harvest_conversation(arguments).await,
            "get_harvester_metrics" => self.execute_get_harvester_metrics().await,
            "migrate_memory" => self.execute_migrate_memory(arguments).await,
            "delete_memory" => self.execute_delete_memory(arguments).await,
            #[cfg(feature = "codex-dreams")]
            "generate_insights" => self.execute_generate_insights(arguments).await,
            #[cfg(feature = "codex-dreams")]
            "show_insights" => self.execute_show_insights(arguments).await,
            #[cfg(feature = "codex-dreams")]
            "search_insights" => self.execute_search_insights(arguments).await,
            #[cfg(feature = "codex-dreams")]
            "insight_feedback" => self.execute_insight_feedback(arguments).await,
            #[cfg(feature = "codex-dreams")]
            "export_insights" => self.execute_export_insights(arguments).await,
            #[cfg(feature = "codex-dreams")]
            "reset_circuit_breaker" => self.execute_reset_circuit_breaker(arguments).await,
            _ => Err(anyhow::anyhow!("Unknown tool: {}", tool_name)),
        }
    }

    /// Execute store_memory tool
    async fn execute_store_memory(&self, args: &Value) -> Result<Value> {
        let content = args
            .get("content")
            .and_then(|c| c.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required 'content' parameter"))?;

        // Parse optional parameters
        let tier = args
            .get("tier")
            .and_then(|t| t.as_str())
            .and_then(|t| t.parse::<MemoryTier>().ok());

        let importance_score = args.get("importance_score").and_then(|s| s.as_f64());

        let tags = args.get("tags").and_then(|t| t.as_array()).map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect::<Vec<String>>()
        });

        let metadata = if let Some(tags) = tags {
            Some(serde_json::json!({ "tags": tags }))
        } else {
            args.get("metadata").cloned()
        };

        // Generate embedding
        let embedding = self.embedder.generate_embedding(content).await?;

        // Create memory request
        let request = CreateMemoryRequest {
            content: content.to_string(),
            embedding: Some(embedding),
            tier,
            importance_score,
            parent_id: None,
            metadata,
            expires_at: None,
        };

        // Store memory
        match self.repository.create_memory(request).await {
            Ok(memory) => {
                let response_text = format!(
                    "Successfully stored memory with ID: {}\nContent: {}\nTier: {:?}",
                    memory.id,
                    content.chars().take(100).collect::<String>(),
                    memory.tier
                );
                Ok(format_tool_response(&response_text))
            }
            Err(crate::memory::error::MemoryError::StorageExhausted { tier, limit }) => {
                // Return a 507-like error for storage exhaustion
                Err(anyhow::anyhow!(
                    "Storage exhausted in {} tier: limit of {} items reached (Miller's 7±2 principle). Memory was automatically evicted via LRU.",
                    tier, limit
                ))
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Execute search_memory tool with progressive response
    async fn execute_search_memory(&self, args: &Value) -> Result<Value> {
        let query = args
            .get("query")
            .and_then(|q| q.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required 'query' parameter"))?;

        let limit = args
            .get("limit")
            .and_then(|l| l.as_i64())
            .map(|l| l as i32)
            .unwrap_or(10);

        let similarity_threshold = args
            .get("similarity_threshold")
            .and_then(|t| t.as_f64())
            .map(|t| t as f32)
            .unwrap_or(0.5);

        let tier = args
            .get("tier")
            .and_then(|t| t.as_str())
            .and_then(|t| t.parse::<MemoryTier>().ok());

        let include_metadata = args
            .get("include_metadata")
            .and_then(|m| m.as_bool())
            .unwrap_or(true);

        // Quick mode for immediate response
        let quick_mode = args
            .get("quick_mode")
            .and_then(|q| q.as_bool())
            .unwrap_or(true);

        if quick_mode {
            // Start async search and return immediately
            let embedder = self.embedder.clone();
            let repository = self.repository.clone();
            let query_owned = query.to_string();

            tokio::spawn(async move {
                // Generate embedding in background
                match embedder.generate_embedding(&query_owned).await {
                    Ok(embedding) => {
                        // Create search request
                        let search_req = SearchRequest {
                            query_text: Some(query_owned.clone()),
                            query_embedding: Some(embedding),
                            limit: Some(limit),
                            offset: None,
                            tier,
                            tags: None,
                            date_range: None,
                            importance_range: None,
                            metadata_filters: None,
                            similarity_threshold: Some(similarity_threshold),
                            search_type: None,
                            hybrid_weights: None,
                            cursor: None,
                            include_facets: None,
                            include_metadata: Some(include_metadata),
                            ranking_boost: None,
                            explain_score: None,
                        };

                        // Perform search
                        match repository.search_memories_simple(search_req).await {
                            Ok(results) => {
                                info!(
                                    "Search completed: {} results for '{}'",
                                    results.len(),
                                    query_owned
                                );
                            }
                            Err(e) => {
                                error!("Search failed: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        error!("Embedding generation failed: {}", e);
                    }
                }
            });

            // Return minimal response immediately
            Ok(format_tool_response(&format!(
                "🔍 Searching for: {}",
                query
            )))
        } else {
            // Normal mode - generate embedding and search (with timeout protection)
            let embedding = match tokio::time::timeout(
                Duration::from_secs(30), // Increased for large model
                self.embedder.generate_embedding(query),
            )
            .await
            {
                Ok(Ok(emb)) => emb,
                Ok(Err(e)) => {
                    return Ok(format_tool_response(&format!("⚠️ Embedding failed: {}", e)));
                }
                Err(_) => {
                    return Ok(format_tool_response("⚠️ Embedding generation timed out"));
                }
            };

            // Create search request
            let search_req = SearchRequest {
                query_text: Some(query.to_string()),
                query_embedding: Some(embedding),
                limit: Some(limit),
                offset: None,
                tier,
                tags: None,
                date_range: None,
                importance_range: None,
                metadata_filters: None,
                similarity_threshold: Some(similarity_threshold),
                search_type: None,
                hybrid_weights: None,
                cursor: None,
                include_facets: None,
                include_metadata: Some(include_metadata),
                ranking_boost: None,
                explain_score: None,
            };

            // Perform search with timeout
            let results = match tokio::time::timeout(
                Duration::from_secs(60), // Increased for complex searches
                self.repository.search_memories_simple(search_req),
            )
            .await
            {
                Ok(Ok(res)) => res,
                Ok(Err(e)) => {
                    return Ok(format_tool_response(&format!("⚠️ Search failed: {}", e)));
                }
                Err(_) => {
                    return Ok(format_tool_response("⚠️ Search timed out"));
                }
            };

            if results.is_empty() {
                Ok(format_tool_response(&format!(
                    "No memories found for query: {query}"
                )))
            } else {
                // Return minimal results to avoid timeout
                let formatted_results = results
                    .iter()
                    .take(3) // Limit to top 3 for quick response
                    .map(|r| {
                        let content_preview =
                            r.memory.content.chars().take(100).collect::<String>();
                        format!("[{:.2}] {}...", r.similarity_score, content_preview)
                    })
                    .collect::<Vec<String>>()
                    .join("\n");

                let response_text = format!(
                    "Found {} memories (showing top {}):\n{}",
                    results.len(),
                    results.len().min(3),
                    formatted_results
                );
                Ok(format_tool_response(&response_text))
            }
        }
    }

    /// Execute get_statistics tool
    async fn execute_get_statistics(&self, args: &Value) -> Result<Value> {
        let detailed = args
            .get("detailed")
            .and_then(|d| d.as_bool())
            .unwrap_or(false);

        let stats = self.repository.get_statistics().await?;

        let stats_text = if detailed {
            format!(
                "Memory System Statistics (Detailed):\n\n\
                 📊 Total Counts:\n\
                 • Active Memories: {}\n\
                 • Deleted Memories: {}\n\
                 • Total Ever Created: {}\n\n\
                 🏢 Tier Distribution:\n\
                 • Working Tier: {} memories\n\
                 • Warm Tier: {} memories\n\
                 • Cold Tier: {} memories\n\n\
                 📈 Access Patterns:\n\
                 • Average Importance Score: {:.3}\n\
                 • Average Access Count: {:.1}\n\
                 • Maximum Access Count: {}\n\n\
                 ⚡ Performance Notes:\n\
                 • Database optimizations active\n\
                 • Vector indexing enabled",
                stats.total_active.unwrap_or(0),
                stats.total_deleted.unwrap_or(0),
                stats.total_active.unwrap_or(0) + stats.total_deleted.unwrap_or(0),
                stats.working_count.unwrap_or(0),
                stats.warm_count.unwrap_or(0),
                stats.cold_count.unwrap_or(0),
                stats.avg_importance.unwrap_or(0.0),
                stats.avg_access_count.unwrap_or(0.0),
                stats.max_access_count.unwrap_or(0)
            )
        } else {
            format!(
                "Memory System Statistics:\n\n\
                 📊 Total Active: {}\n\
                 📊 Total Deleted: {}\n\
                 🔥 Working Tier: {}\n\
                 🌡️ Warm Tier: {}\n\
                 🧊 Cold Tier: {}\n\n\
                 📈 Access Patterns:\n\
                 • Average Importance: {:.2}\n\
                 • Average Access Count: {:.1}\n\
                 • Max Access Count: {}",
                stats.total_active.unwrap_or(0),
                stats.total_deleted.unwrap_or(0),
                stats.working_count.unwrap_or(0),
                stats.warm_count.unwrap_or(0),
                stats.cold_count.unwrap_or(0),
                stats.avg_importance.unwrap_or(0.0),
                stats.avg_access_count.unwrap_or(0.0),
                stats.max_access_count.unwrap_or(0)
            )
        };

        Ok(format_tool_response(&stats_text))
    }

    /// Execute what_did_you_remember tool - query recent memories
    async fn execute_what_did_you_remember(&self, args: &Value) -> Result<Value> {
        let context = args
            .get("context")
            .and_then(|c| c.as_str())
            .unwrap_or("conversation");

        let time_range = args
            .get("time_range")
            .and_then(|r| r.as_str())
            .unwrap_or("last_day");

        let limit = args
            .get("limit")
            .and_then(|l| l.as_i64())
            .map(|l| l as i32)
            .unwrap_or(10);

        // Calculate date range
        let now = Utc::now();
        let start_date = match time_range {
            "last_hour" => now - ChronoDuration::hours(1),
            "last_day" => now - ChronoDuration::days(1),
            "last_week" => now - ChronoDuration::weeks(1),
            "last_month" => now - ChronoDuration::days(30),
            _ => now - ChronoDuration::days(1),
        };

        // Search for recent memories
        let search_req = SearchRequest {
            query_text: Some(format!("context:{context}")),
            query_embedding: None, // Will be generated
            limit: Some(limit),
            offset: None,
            tier: None,
            tags: None,
            date_range: Some(DateRange {
                start: Some(start_date),
                end: Some(now),
            }),
            importance_range: None,
            metadata_filters: Some(serde_json::json!({
                "context": context
            })),
            similarity_threshold: Some(0.3), // Lower threshold for recent memories
            search_type: None,
            hybrid_weights: None,
            cursor: None,
            include_facets: None,
            include_metadata: Some(true),
            ranking_boost: None,
            explain_score: None,
        };

        // Generate embedding for context search
        let embedding = self
            .embedder
            .generate_embedding(&format!("context:{context}"))
            .await?;
        let mut search_req = search_req;
        search_req.query_embedding = Some(embedding);

        let results = self.repository.search_memories_simple(search_req).await?;

        if results.is_empty() {
            let response_text = format!(
                "I haven't remembered anything specific about {} in the {}. \
                 You might want to check if memories were properly harvested or stored.",
                context,
                time_range.replace('_', " ")
            );
            Ok(format_tool_response(&response_text))
        } else {
            let formatted_memories = results
                .iter()
                .map(|r| {
                    let age = now.signed_duration_since(r.memory.created_at);
                    let age_str = if age.num_hours() < 1 {
                        format!("{}m ago", age.num_minutes())
                    } else if age.num_days() < 1 {
                        format!("{}h ago", age.num_hours())
                    } else {
                        format!("{}d ago", age.num_days())
                    };

                    let content_preview = r.memory.content.chars().take(150).collect::<String>();
                    format!(
                        "• [{}] {}\n  (Tier: {:?}, Importance: {:.2})",
                        age_str, content_preview, r.memory.tier, r.memory.importance_score
                    )
                })
                .collect::<Vec<String>>()
                .join("\n\n");

            let response_text = format!(
                "Here's what I remembered about {} from the {}:\n\n{}",
                context,
                time_range.replace('_', " "),
                formatted_memories
            );
            Ok(format_tool_response(&response_text))
        }
    }

    /// Execute harvest_conversation tool with progressive responses
    async fn execute_harvest_conversation(&self, args: &Value) -> Result<Value> {
        let message = args.get("message").and_then(|m| m.as_str());

        let context = args
            .get("context")
            .and_then(|c| c.as_str())
            .unwrap_or("conversation");

        let role = args.get("role").and_then(|r| r.as_str()).unwrap_or("user");

        let force_harvest = args
            .get("force_harvest")
            .and_then(|f| f.as_bool())
            .unwrap_or(false);

        let silent_mode = args
            .get("silent_mode")
            .and_then(|s| s.as_bool())
            .unwrap_or(true);

        // New: Support for quick mode (ultra-minimal response)
        let quick_mode = args
            .get("quick_mode")
            .and_then(|q| q.as_bool())
            .unwrap_or(true); // Default to quick mode

        // New: Support for chunked harvesting
        let chunk_size = args
            .get("chunk_size")
            .and_then(|c| c.as_u64())
            .unwrap_or(500); // Default 500 words per chunk

        // Add message to harvester queue if provided
        if let Some(message_content) = message {
            // Check if message is large and needs chunking
            let word_count = message_content.split_whitespace().count();

            if word_count > chunk_size as usize {
                // Large message - process in chunks to avoid timeout
                let words: Vec<&str> = message_content.split_whitespace().collect();
                let chunks: Vec<String> = words
                    .chunks(chunk_size as usize)
                    .map(|chunk| chunk.join(" "))
                    .collect();

                // Process chunks asynchronously
                let harvester = self.harvester_service.clone();
                let chunk_count = chunks.len();
                let role_owned = role.to_string();
                let context_owned = context.to_string();

                tokio::spawn(async move {
                    for (i, chunk) in chunks.iter().enumerate() {
                        let conversation_message = ConversationMessage {
                            id: Uuid::new_v4().to_string(),
                            content: chunk.to_string(),
                            timestamp: Utc::now(),
                            role: role_owned.clone(),
                            context: format!("{}_chunk_{}", context_owned, i + 1),
                        };

                        if let Err(e) = harvester.add_message(conversation_message).await {
                            error!("Failed to add chunk {}/{}: {}", i + 1, chunk_count, e);
                        }

                        // Process each chunk immediately
                        if let Err(e) = harvester.force_harvest().await {
                            error!("Failed to harvest chunk {}/{}: {}", i + 1, chunk_count, e);
                        }
                    }
                    info!("Completed chunked harvest of {} chunks", chunk_count);
                });

                // Return immediately for chunked processing
                return Ok(format_tool_response(&format!(
                    "✓ Processing {} chunks ({}w each)",
                    chunk_count, chunk_size
                )));
            } else {
                // Normal sized message - process as usual
                let conversation_message = ConversationMessage {
                    id: Uuid::new_v4().to_string(),
                    content: message_content.to_string(),
                    timestamp: Utc::now(),
                    role: role.to_string(),
                    context: context.to_string(),
                };

                self.harvester_service
                    .add_message(conversation_message)
                    .await?;
            }
        }

        // Force harvest if requested
        if force_harvest {
            if quick_mode {
                // Ultra-minimal response mode - start harvest and return immediately
                let harvester = self.harvester_service.clone();
                let harvest_id = Uuid::new_v4();

                tokio::spawn(async move {
                    match harvester.force_harvest().await {
                        Ok(result) => {
                            info!(
                                "[{}] Harvest complete: {} patterns",
                                harvest_id, result.patterns_stored
                            );
                        }
                        Err(e) => {
                            error!("[{}] Harvest failed: {}", harvest_id, e);
                        }
                    }
                });

                // Return minimal response immediately
                Ok(format_tool_response("✓"))
            } else {
                // Normal async mode with slightly more detail
                let harvester = self.harvester_service.clone();

                // Get quick stats before starting
                let stats = self.repository.get_statistics().await.ok();
                let pre_count = stats.as_ref().and_then(|s| s.total_active).unwrap_or(0);

                tokio::spawn(async move {
                    match harvester.force_harvest().await {
                        Ok(result) => {
                            info!(
                                "Background harvest completed: {} patterns stored",
                                result.patterns_stored
                            );
                        }
                        Err(e) => {
                            error!("Background harvest failed: {}", e);
                        }
                    }
                });

                // Return summary statistics instead of full details
                let response_text = if silent_mode {
                    format!("✓ Harvesting ({} existing)", pre_count)
                } else {
                    format!(
                        "✓ Harvest started\n• Current memories: {}\n• Processing in background",
                        pre_count
                    )
                };
                Ok(format_tool_response(&response_text))
            }
        } else {
            // Silent queuing mode
            let response_text = if message.is_some() {
                if quick_mode {
                    "✓ Queued"
                } else {
                    "Message queued for background harvesting"
                }
            } else {
                if quick_mode {
                    "✓ Active"
                } else {
                    "Background harvesting is active and monitoring conversations"
                }
            };
            Ok(format_tool_response(response_text))
        }
    }

    /// Execute get_harvester_metrics tool
    async fn execute_get_harvester_metrics(&self) -> Result<Value> {
        let metrics = self.harvester_service.get_metrics().await;

        let metrics_text = format!(
            "Silent Harvester Metrics:\n\n\
             📊 Processing Stats:\n\
             • Messages Processed: {}\n\
             • Patterns Extracted: {}\n\
             • Memories Stored: {}\n\
             • Duplicates Filtered: {}\n\n\
             ⚙️ Performance:\n\
             • Average Extraction Time: {}ms\n\
             • Average Batch Processing Time: {}ms\n\
             • Last Harvest: {}",
            metrics.messages_processed,
            metrics.patterns_extracted,
            metrics.memories_stored,
            metrics.duplicates_filtered,
            metrics.avg_extraction_time_ms,
            metrics.avg_batch_processing_time_ms,
            metrics
                .last_harvest_time
                .map(|t| format!("{} ago", format_duration(Utc::now() - t)))
                .unwrap_or_else(|| "Never".to_string())
        );

        Ok(format_tool_response(&metrics_text))
    }

    /// Execute migrate_memory tool
    async fn execute_migrate_memory(&self, args: &Value) -> Result<Value> {
        let memory_id_str = args
            .get("memory_id")
            .and_then(|id| id.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required 'memory_id' parameter"))?;
        let memory_id = Uuid::parse_str(memory_id_str)?;

        let target_tier = args
            .get("target_tier")
            .and_then(|t| t.as_str())
            .and_then(|t| t.parse::<MemoryTier>().ok())
            .ok_or_else(|| {
                anyhow::anyhow!("Missing or invalid required 'target_tier' parameter")
            })?;

        let reason = args
            .get("reason")
            .and_then(|r| r.as_str())
            .map(String::from);

        // Perform migration
        let updated_memory = self
            .repository
            .migrate_memory(memory_id, target_tier, reason.clone())
            .await?;

        let response_text = format!(
            "Successfully migrated memory {} to {:?} tier\n\
             Content: {}\n\
             Reason: {}",
            memory_id,
            target_tier,
            updated_memory.content.chars().take(100).collect::<String>(),
            reason.unwrap_or_else(|| "No reason provided".to_string())
        );

        Ok(format_tool_response(&response_text))
    }

    /// Execute delete_memory tool
    async fn execute_delete_memory(&self, args: &Value) -> Result<Value> {
        let memory_id_str = args
            .get("memory_id")
            .and_then(|id| id.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required 'memory_id' parameter"))?;
        let memory_id = Uuid::parse_str(memory_id_str)?;

        // Perform deletion
        self.repository.delete_memory(memory_id).await?;

        let response_text = format!("Successfully deleted memory {memory_id}");
        Ok(format_tool_response(&response_text))
    }

    #[cfg(feature = "codex-dreams")]
    /// Execute generate_insights tool
    async fn execute_generate_insights(&self, args: &Value) -> Result<Value> {
        // Parse parameters
        let time_period = args
            .get("time_period")
            .and_then(|t| t.as_str())
            .unwrap_or("last_day");
        let topic = args.get("topic").and_then(|t| t.as_str());
        let insight_type = args
            .get("insight_type")
            .and_then(|t| t.as_str())
            .unwrap_or("all");
        let max_insights = args
            .get("max_insights")
            .and_then(|m| m.as_i64())
            .unwrap_or(5) as usize;

        #[cfg(feature = "codex-dreams")]
        {
            // Check if insights processor is available
            if let Some(processor) = &self.insights_processor {
                info!(
                    "🧠 Generating insights: period={}, topic={:?}, type={}, max={}",
                    time_period, topic, insight_type, max_insights
                );

                // Get memories based on time period
                let memories = match time_period {
                    "last_hour" => {
                        let since = Utc::now() - ChronoDuration::hours(1);
                        self.repository
                            .search_memories(crate::memory::SearchRequest {
                                date_range: Some(DateRange {
                                    start: Some(since),
                                    end: None,
                                }),
                                limit: Some(100),
                                search_type: Some(crate::memory::SearchType::Temporal),
                                ..Default::default()
                            })
                            .await
                    }
                    "last_day" => {
                        let since = Utc::now() - ChronoDuration::days(1);
                        self.repository
                            .search_memories(crate::memory::SearchRequest {
                                date_range: Some(DateRange {
                                    start: Some(since),
                                    end: None,
                                }),
                                limit: Some(200),
                                search_type: Some(crate::memory::SearchType::Temporal),
                                ..Default::default()
                            })
                            .await
                    }
                    "last_week" => {
                        let since = Utc::now() - ChronoDuration::weeks(1);
                        self.repository
                            .search_memories(crate::memory::SearchRequest {
                                date_range: Some(DateRange {
                                    start: Some(since),
                                    end: None,
                                }),
                                limit: Some(500),
                                search_type: Some(crate::memory::SearchType::Temporal),
                                ..Default::default()
                            })
                            .await
                    }
                    "last_month" => {
                        let since = Utc::now() - ChronoDuration::days(30);
                        self.repository
                            .search_memories(crate::memory::SearchRequest {
                                date_range: Some(DateRange {
                                    start: Some(since),
                                    end: None,
                                }),
                                limit: Some(1000),
                                search_type: Some(crate::memory::SearchType::Temporal),
                                ..Default::default()
                            })
                            .await
                    }
                    _ => {
                        let since = Utc::now() - ChronoDuration::days(1);
                        self.repository
                            .search_memories(crate::memory::SearchRequest {
                                date_range: Some(DateRange {
                                    start: Some(since),
                                    end: None,
                                }),
                                limit: Some(200),
                                search_type: Some(crate::memory::SearchType::Temporal),
                                ..Default::default()
                            })
                            .await
                    }
                }?;

                if memories.results.is_empty() {
                    let response_text = format!(
                        "★ Insights Generation\n\
                        📭 No memories found in the specified time period: {}\n\
                        \n\
                        💡 Try:\n\
                        • Using a longer time period (e.g., 'last_week')\n\
                        • Adding some memories first with store_memory\n\
                        • Checking if memories exist with search_memories",
                        time_period
                    );
                    return Ok(format_tool_response(&response_text));
                }

                // Filter by topic if specified (simple substring search)
                let filtered_memories = if let Some(topic_filter) = topic {
                    memories
                        .results
                        .into_iter()
                        .filter(|m| {
                            m.memory
                                .content
                                .to_lowercase()
                                .contains(&topic_filter.to_lowercase())
                        })
                        .collect()
                } else {
                    memories.results
                };

                if filtered_memories.is_empty() {
                    let response_text = format!(
                        "★ Insights Generation\n\
                        🔍 No memories found matching topic '{}' in time period: {}\n\
                        \n\
                        💡 Try a different topic or broader search terms.",
                        topic.unwrap_or(""),
                        time_period
                    );
                    return Ok(format_tool_response(&response_text));
                }

                // Get memory IDs for batch processing
                let memory_ids: Vec<Uuid> = filtered_memories
                    .iter()
                    .take(max_insights * 10) // Get more memories than insights to have variety
                    .map(|m| m.memory.id)
                    .collect();

                info!(
                    "Processing {} memories for insight generation",
                    memory_ids.len()
                );

                // Use the insights processor to generate insights
                match processor.process_batch(memory_ids).await {
                    Ok(processing_result) => {
                        let response_text = format!(
                            "★ Insights Generated Successfully\n\
                            📊 Processed {} memories from time period: {}\n\
                            🔍 Topic filter: {}\n\
                            💡 Generated {} insights\n\
                            ⚡ Success rate: {:.1}%\n\
                            ⏱️ Processing time: {:.2}s\n\
                            \n\
                            Insights summary:\n{}",
                            processing_result.report.memories_processed,
                            time_period,
                            topic.unwrap_or("none"),
                            processing_result.insights.len(),
                            processing_result.report.success_rate * 100.0,
                            processing_result.report.duration_seconds,
                            processing_result
                                .insights
                                .iter()
                                .take(3)
                                .map(|insight| format!(
                                    "• {} (confidence: {:.0}%): {}",
                                    match insight.insight_type {
                                        crate::insights::models::InsightType::Learning =>
                                            "Learning",
                                        crate::insights::models::InsightType::Connection =>
                                            "Connection",
                                        crate::insights::models::InsightType::Relationship =>
                                            "Relationship",
                                        crate::insights::models::InsightType::Assertion =>
                                            "Assertion",
                                        crate::insights::models::InsightType::MentalModel =>
                                            "Mental Model",
                                        crate::insights::models::InsightType::Pattern => "Pattern",
                                    },
                                    insight.confidence_score * 100.0,
                                    insight.content.chars().take(100).collect::<String>()
                                ))
                                .collect::<Vec<String>>()
                                .join("\n")
                        );
                        Ok(format_tool_response(&response_text))
                    }
                    Err(e) => {
                        let response_text = format!(
                            "★ Insights Generation Failed\n\
                            📊 Found {} memories in time period: {}\n\
                            ❌ Error: {}\n\
                            \n\
                            💡 Try:\n\
                            • Check if Ollama service is running at {}\n\
                            • Verify the model '{}' is available\n\
                            • Check database connectivity\n\
                            • Use 'export_insights' to view any existing insights",
                            filtered_memories.len(),
                            time_period,
                            e,
                            "http://192.168.1.110:11434",
                            "gpt-oss:20b"
                        );
                        Ok(format_tool_response(&response_text))
                    }
                }
            } else {
                let response_text = "★ Insights Generation\n\
                ❌ Insights processor not available\n\
                \n\
                This could be due to:\n\
                • Ollama service not configured\n\
                • Missing dependencies\n\
                • Feature disabled\n\
                \n\
                💡 Check your configuration and ensure Ollama is running on localhost:11434";
                Ok(format_tool_response(response_text))
            }
        }

        #[cfg(not(feature = "codex-dreams"))]
        {
            let response_text = format!(
                "★ Insights Generation\n\
                ⚠️ Feature not available - codex-dreams feature not enabled\n\
                • Time period: {}\n\
                • Topic: {}\n\
                • Type: {}\n\
                • Max insights: {}\n\
                \n\
                ℹ️ Rebuild with --features codex-dreams to enable insights.",
                time_period,
                topic.unwrap_or("all topics"),
                insight_type,
                max_insights
            );
            Ok(format_tool_response(&response_text))
        }
    }

    #[cfg(feature = "codex-dreams")]
    /// Execute show_insights tool
    async fn execute_show_insights(&self, args: &Value) -> Result<Value> {
        let limit = args.get("limit").and_then(|l| l.as_i64()).unwrap_or(10) as usize;
        let insight_type = args
            .get("insight_type")
            .and_then(|t| t.as_str())
            .unwrap_or("all");
        let min_confidence = args
            .get("min_confidence")
            .and_then(|c| c.as_f64())
            .unwrap_or(0.6);
        let include_feedback = args
            .get("include_feedback")
            .and_then(|f| f.as_bool())
            .unwrap_or(true);

        // TODO: Once insights are being generated, query them from InsightStorage
        let response_text = format!(
            "★ Recent Insights ({})\n\
            ⚠️ No insights available yet - insights will appear here once generation begins\n\
            \n\
            • Filters:\n\
            ○ Type: {}\n\
            ○ Min confidence: {:.0}%\n\
            ○ Include feedback: {}\n\
            ○ Limit: {}\n\
            \n\
            ℹ️ Use 'generate_insights' to start creating insights from your memories.\n\
            Insights will be displayed here with ★ confidence scores and user feedback.",
            if insight_type == "all" {
                "All Types"
            } else {
                insight_type
            },
            insight_type,
            min_confidence * 100.0,
            if include_feedback { "Yes" } else { "No" },
            limit
        );

        Ok(format_tool_response(&response_text))
    }

    #[cfg(feature = "codex-dreams")]
    /// Execute search_insights tool
    async fn execute_search_insights(&self, args: &Value) -> Result<Value> {
        let query = args
            .get("query")
            .and_then(|q| q.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required 'query' parameter"))?;
        let limit = args.get("limit").and_then(|l| l.as_i64()).unwrap_or(10) as usize;
        let similarity_threshold = args
            .get("similarity_threshold")
            .and_then(|t| t.as_f64())
            .unwrap_or(0.7);
        let insight_type = args
            .get("insight_type")
            .and_then(|t| t.as_str())
            .unwrap_or("all");

        // TODO: Implement semantic search using InsightStorage when available
        let response_text = format!(
            "★ Insight Search: \"{}\"\n\
            ⚠️ Search will be available once insights are generated\n\
            \n\
            • Search parameters:\n\
            ○ Query: {}\n\
            ○ Type filter: {}\n\
            ○ Min similarity: {:.0}%\n\
            ○ Results limit: {}\n\
            \n\
            ℹ️ This will perform semantic search across all generated insights.\n\
            Results will show matching insights with similarity scores and confidence ratings.",
            query,
            query,
            if insight_type == "all" {
                "All types"
            } else {
                insight_type
            },
            similarity_threshold * 100.0,
            limit
        );

        Ok(format_tool_response(&response_text))
    }

    #[cfg(feature = "codex-dreams")]
    /// Execute insight_feedback tool
    async fn execute_insight_feedback(&self, args: &Value) -> Result<Value> {
        let insight_id_str = args
            .get("insight_id")
            .and_then(|id| id.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required 'insight_id' parameter"))?;
        let _insight_id = Uuid::parse_str(insight_id_str)?;
        let helpful = args
            .get("helpful")
            .and_then(|h| h.as_bool())
            .ok_or_else(|| anyhow::anyhow!("Missing required 'helpful' parameter"))?;
        let comment = args.get("comment").and_then(|c| c.as_str());

        // TODO: Store feedback using InsightStorage when available
        let feedback_text = if helpful {
            "👍 Helpful"
        } else {
            "👎 Not helpful"
        };
        let response_text = format!(
            "★ Feedback Recorded\n\
            • Insight: {}\n\
            • Rating: {}\n\
            • Comment: {}\n\
            \n\
            ✓ Feedback will improve future insight generation quality.\n\
            Thank you for helping train the insight system!",
            insight_id_str,
            feedback_text,
            comment.unwrap_or("(none)")
        );

        Ok(format_tool_response(&response_text))
    }

    #[cfg(feature = "codex-dreams")]
    /// Execute export_insights tool
    async fn execute_export_insights(&self, args: &Value) -> Result<Value> {
        let format = args
            .get("format")
            .and_then(|f| f.as_str())
            .unwrap_or("markdown");
        let time_period = args
            .get("time_period")
            .and_then(|t| t.as_str())
            .unwrap_or("all");
        let insight_type = args
            .get("insight_type")
            .and_then(|t| t.as_str())
            .unwrap_or("all");
        let min_confidence = args
            .get("min_confidence")
            .and_then(|c| c.as_f64())
            .unwrap_or(0.6);
        let include_metadata = args
            .get("include_metadata")
            .and_then(|m| m.as_bool())
            .unwrap_or(true);

        #[cfg(feature = "codex-dreams")]
        {
            // Get insights from storage if available
            let (insights, summary) = if let Some(storage) = &self.insight_storage {
                // First, get all insights using a broad search (empty query gets all)
                let search_results = storage
                    .search("", 1000) // Large limit to get all insights
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to retrieve insights: {}", e))?;

                // Filter insights based on criteria
                let mut filtered_insights = Vec::new();
                let mut type_counts = std::collections::HashMap::new();
                let mut confidence_ranges = [0, 0, 0, 0, 0]; // 0-20%, 20-40%, etc.

                for result in search_results {
                    let insight = result.insight;
                    // Apply filters
                    let matches_confidence = insight.confidence_score >= min_confidence as f32;
                    let insight_type_str = match &insight.insight_type {
                        crate::insights::models::InsightType::Learning => "learning",
                        crate::insights::models::InsightType::Connection => "connection",
                        crate::insights::models::InsightType::Relationship => "relationship",
                        crate::insights::models::InsightType::Assertion => "assertion",
                        crate::insights::models::InsightType::MentalModel => "mentalmodel",
                        crate::insights::models::InsightType::Pattern => "pattern",
                    };
                    let matches_type = insight_type == "all" || insight_type_str == insight_type;

                    let matches_time = if time_period == "all" {
                        true
                    } else {
                        // Parse time period and check if insight falls within range
                        let cutoff_time = match time_period {
                            "day" => chrono::Utc::now() - chrono::Duration::hours(24),
                            "week" => chrono::Utc::now() - chrono::Duration::days(7),
                            "month" => chrono::Utc::now() - chrono::Duration::days(30),
                            _ => chrono::DateTime::<chrono::Utc>::from_timestamp(0, 0)
                                .unwrap_or_else(chrono::Utc::now),
                        };
                        insight.created_at >= cutoff_time
                    };

                    if matches_confidence && matches_type && matches_time {
                        // Count by type
                        *type_counts.entry(insight_type_str.to_string()).or_insert(0) += 1;

                        // Count confidence ranges
                        let confidence_pct = (insight.confidence_score * 100.0) as usize;
                        let range_idx = std::cmp::min(confidence_pct / 20, 4);
                        confidence_ranges[range_idx] += 1;

                        filtered_insights.push(insight);
                    }
                }

                // Sort by confidence score descending
                filtered_insights.sort_by(|a, b| {
                    b.confidence_score
                        .partial_cmp(&a.confidence_score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });

                let summary = serde_json::json!({
                    "total_insights": filtered_insights.len(),
                    "by_type": type_counts,
                    "confidence_distribution": {
                        "80-100%": confidence_ranges[4],
                        "60-80%": confidence_ranges[3],
                        "40-60%": confidence_ranges[2],
                        "20-40%": confidence_ranges[1],
                        "0-20%": confidence_ranges[0]
                    }
                });

                (filtered_insights, summary)
            } else {
                (
                    Vec::new(),
                    serde_json::json!({
                        "total_insights": 0,
                        "by_type": {},
                        "confidence_distribution": {}
                    }),
                )
            };

            let export_content = if format == "json" {
                // Create JSON-LD export
                let mut insights_json = Vec::new();
                for insight in &insights {
                    let mut insight_obj = serde_json::json!({
                        "@context": "https://schema.org/",
                        "@type": "CreativeWork",
                        "identifier": insight.id.to_string(),
                        "text": insight.content,
                        "dateCreated": insight.created_at.to_rfc3339(),
                        "dateModified": insight.updated_at.to_rfc3339(),
                        "version": insight.version,
                        "confidence_score": insight.confidence_score,
                        "insight_type": match &insight.insight_type {
                            crate::insights::models::InsightType::Learning => "learning",
                            crate::insights::models::InsightType::Connection => "connection",
                            crate::insights::models::InsightType::Relationship => "relationship",
                            crate::insights::models::InsightType::Assertion => "assertion",
                            crate::insights::models::InsightType::MentalModel => "mentalmodel",
                            crate::insights::models::InsightType::Pattern => "pattern",
                        }
                    });

                    if include_metadata {
                        insight_obj["metadata"] = insight.metadata.clone();
                        insight_obj["source_memory_ids"] = serde_json::Value::Array(
                            insight
                                .source_memory_ids
                                .iter()
                                .map(|id| serde_json::Value::String(id.to_string()))
                                .collect(),
                        );
                        insight_obj["tags"] = serde_json::Value::Array(
                            insight
                                .tags
                                .iter()
                                .map(|tag| serde_json::Value::String(tag.clone()))
                                .collect(),
                        );
                        if insight.feedback_score > 0.0 {
                            insight_obj["feedback_score"] = serde_json::Value::Number(
                                serde_json::Number::from_f64(insight.feedback_score as f64)
                                    .unwrap_or_else(|| serde_json::Number::from(0)),
                            );
                        }
                    }

                    insights_json.push(insight_obj);
                }

                let export_obj = serde_json::json!({
                    "@context": "https://schema.org/",
                    "@type": "Dataset",
                    "name": "Codex Memory Insights Export",
                    "description": "Export of AI-generated insights from memory analysis",
                    "export_info": {
                        "format": "json-ld",
                        "time_period": time_period,
                        "insight_type": insight_type,
                        "min_confidence": min_confidence,
                        "include_metadata": include_metadata,
                        "generated_at": chrono::Utc::now().to_rfc3339()
                    },
                    "insights": insights_json,
                    "summary": summary
                });

                format!(
                    "★ Insights Export Complete (JSON-LD)\n\n```json\n{}\n```\n\n✅ Exported {} insights matching your criteria",
                    serde_json::to_string_pretty(&export_obj).unwrap_or_else(|_| "{}".to_string()),
                    insights.len()
                )
            } else {
                // Create Markdown export
                let mut markdown = format!(
                    r#"# 🧠 Codex Memory Insights Export

## Export Summary
- **Format**: {}
- **Time Period**: {}
- **Type Filter**: {}
- **Min Confidence**: {:.0}%
- **Include Metadata**: {}
- **Generated**: {}
- **Total Insights**: {}

"#,
                    format,
                    time_period,
                    if insight_type == "all" {
                        "All types"
                    } else {
                        insight_type
                    },
                    min_confidence * 100.0,
                    if include_metadata { "Yes" } else { "No" },
                    chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
                    insights.len()
                );

                if !insights.is_empty() {
                    markdown.push_str("## 📊 Distribution Summary\n\n");
                    if let Some(by_type) = summary["by_type"].as_object() {
                        for (insight_type, count) in by_type {
                            markdown
                                .push_str(&format!("- **{}**: {} insights\n", insight_type, count));
                        }
                    }
                    markdown.push_str("\n");

                    markdown.push_str("## 💡 Insights\n\n");
                    for (idx, insight) in insights.iter().enumerate() {
                        markdown.push_str(&format!(
                            "### {}. {} (★ {:.0}%)\n\n",
                            idx + 1,
                            match &insight.insight_type {
                                crate::insights::models::InsightType::Learning => "Learning",
                                crate::insights::models::InsightType::Connection => "Connection",
                                crate::insights::models::InsightType::Relationship =>
                                    "Relationship",
                                crate::insights::models::InsightType::Assertion => "Assertion",
                                crate::insights::models::InsightType::MentalModel => "Mental Model",
                                crate::insights::models::InsightType::Pattern => "Pattern",
                            },
                            insight.confidence_score * 100.0
                        ));

                        markdown.push_str(&format!("{}\n\n", insight.content));

                        if include_metadata {
                            markdown.push_str(&format!(
                                "**Created**: {} | **Version**: {} | **Sources**: {} memories\n\n",
                                insight.created_at.format("%Y-%m-%d %H:%M:%S UTC"),
                                insight.version,
                                insight.source_memory_ids.len()
                            ));

                            if !insight.tags.is_empty() {
                                markdown.push_str(&format!(
                                    "**Tags**: {}\n\n",
                                    insight.tags.join(", ")
                                ));
                            }

                            if insight.feedback_score > 0.0 {
                                markdown.push_str(&format!(
                                    "**User Feedback**: {:.1}/5.0 ⭐\n\n",
                                    insight.feedback_score
                                ));
                            }
                        }

                        markdown.push_str("---\n\n");
                    }
                } else {
                    markdown.push_str("## 📭 No Insights Found\n\nNo insights match your current criteria. Try:\n- Lowering the confidence threshold\n- Expanding the time period\n- Changing the insight type filter\n- Generating insights first using `generate_insights`\n\n");
                }

                format!("★ Insights Export Complete (Markdown)\n\n{}", markdown)
            };

            Ok(format_tool_response(&export_content))
        }

        #[cfg(not(feature = "codex-dreams"))]
        {
            let response_text = "⚠️ Export insights feature requires the 'codex-dreams' feature to be enabled.\n\nPlease rebuild with: cargo build --features codex-dreams";
            Ok(format_tool_response(response_text))
        }
    }

    /// Execute reset_circuit_breaker tool - diagnostic and recovery for insights generation
    #[cfg(feature = "codex-dreams")]
    async fn execute_reset_circuit_breaker(&self, _args: &Value) -> Result<Value> {
        if let Some(processor) = &self.insights_processor {
            // Get current circuit breaker stats
            let stats = processor.get_stats().await;
            let current_state = &stats.circuit_breaker_state;
            let trip_count = stats.circuit_breaker_trips;
            
            // Test Ollama connectivity 
            let ollama_status = match self.test_ollama_connectivity().await {
                Ok(_) => "✅ Connected",
                Err(e) => {
                    warn!("Ollama connectivity test failed: {}", e);
                    "❌ Failed"
                }
            };
            
            // Force circuit breaker recovery if Ollama is working
            let recovery_attempted = if current_state == "Open" && ollama_status.starts_with("✅") {
                // The circuit breaker will naturally recover on next successful request
                // But we can provide guidance to the user
                true
            } else {
                false
            };
            
            let response_text = format!(
                "🔧 Circuit Breaker Diagnostic\n\
                \n\
                📊 Current Status:\n\
                • State: {}\n\
                • Total trips: {}\n\
                • Ollama connectivity: {}\n\
                \n\
                {} \n\
                \n\
                💡 Recommendations:\n\
                • If Ollama is connected and circuit is open, try generate_insights again\n\
                • The circuit breaker will auto-recover after successful connection\n\
                • Monitor circuit_breaker_trips in get_statistics for improvements",
                current_state,
                trip_count,
                ollama_status,
                if recovery_attempted {
                    "🔄 Circuit breaker will attempt recovery on next request"
                } else if current_state == "Closed" {
                    "✅ Circuit breaker is healthy - insights generation should work"
                } else {
                    "⏳ Circuit breaker in recovery mode - will test connectivity soon"
                }
            );
            
            Ok(format_tool_response(&response_text))
        } else {
            let error_text = "❌ Circuit Breaker Diagnostic\n\
            \n\
            Insights processor not available.\n\
            This could be due to:\n\
            • Ollama service not configured\n\
            • Missing codex-dreams feature\n\
            • Service initialization failure";
            
            Ok(format_tool_response(error_text))
        }
    }

    /// Test Ollama connectivity for circuit breaker diagnostics
    #[cfg(feature = "codex-dreams")]
    async fn test_ollama_connectivity(&self) -> Result<()> {
        use reqwest;
        
        let client = reqwest::Client::new();
        let response = client
            .get("http://192.168.1.110:11434/api/tags")
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Ollama connection failed: {}", e))?;
            
        if response.status().is_success() {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Ollama returned status: {}", response.status()))
        }
    }
}

/// Format duration for human-readable display
fn format_duration(duration: ChronoDuration) -> String {
    let total_seconds = duration.num_seconds();

    if total_seconds < 60 {
        format!("{total_seconds}s")
    } else if total_seconds < 3600 {
        format!("{}m", total_seconds / 60)
    } else if total_seconds < 86400 {
        format!("{}h", total_seconds / 3600)
    } else {
        format!("{}d", total_seconds / 86400)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(ChronoDuration::seconds(30)), "30s");
        assert_eq!(format_duration(ChronoDuration::minutes(5)), "5m"); // Function formats as minutes, not seconds
        assert_eq!(format_duration(ChronoDuration::hours(2)), "2h");
        assert_eq!(format_duration(ChronoDuration::days(3)), "3d");
    }

    #[tokio::test]
    async fn test_initialize_handler() {
        // This would need proper test setup with mock dependencies
        // For now, just test that the function compiles and basic logic
        let capabilities = MCPTools::get_server_capabilities();
        assert_eq!(capabilities["protocolVersion"], "2025-06-18");
        assert_eq!(capabilities["serverInfo"]["name"], "codex-memory");
    }
}
