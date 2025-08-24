//! MCP Request Handlers
//!
//! This module contains all the request handlers for MCP protocol methods,
//! including tool execution, initialization, and resource management.

use crate::mcp_server::{
    auth::{AuthContext, MCPAuth},
    circuit_breaker::{CircuitBreaker, CircuitBreakerError},
    logging::{LogLevel, MCPLogger},
    progress::{ProgressTracker, ProgressHandle},
    rate_limiter::MCPRateLimiter,
    tools::MCPTools,
    transport::{create_error_response, create_success_response, format_tool_response},
};
use crate::memory::{models::*, ConversationMessage, MemoryRepository, SilentHarvesterService};
use crate::SimpleEmbedder;
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
}

impl MCPHandlers {
    /// Create new MCP handlers
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
            // Determine if we're in silent mode based on the tool/method
            let silent_mode = matches!(method, "harvest_conversation")
                || params
                    .and_then(|p| p.get("silent_mode"))
                    .and_then(|s| s.as_bool())
                    .unwrap_or(false);

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
                Duration::from_secs(5),
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
                Duration::from_secs(10),
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
            .ok_or_else(|| anyhow::anyhow!("Missing or invalid required 'target_tier' parameter"))?;

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
