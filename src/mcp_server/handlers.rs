//! MCP Request Handlers
//!
//! This module contains all the request handlers for MCP protocol methods,
//! including tool execution, initialization, and resource management.

use crate::memory::{models::*, ConversationMessage, MemoryRepository, SilentHarvesterService};
use crate::mcp_server::{
    auth::{MCPAuth, AuthContext},
    circuit_breaker::{CircuitBreaker, CircuitBreakerError},
    rate_limiter::MCPRateLimiter,
    tools::MCPTools,
    transport::{create_error_response, create_success_response, format_tool_response},
};
use crate::SimpleEmbedder;
use anyhow::Result;
use chrono::{Duration, Utc};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
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
    ) -> Self {
        Self {
            repository,
            embedder,
            harvester_service,
            circuit_breaker,
            auth,
            rate_limiter,
        }
    }

    /// Handle incoming MCP requests with authentication and rate limiting
    pub async fn handle_request(&mut self, method: &str, params: Option<&Value>, id: Option<&Value>) -> Value {
        self.handle_request_with_headers(method, params, id, &HashMap::new()).await
    }

    /// Handle incoming MCP requests with headers for auth/rate limiting
    pub async fn handle_request_with_headers(
        &mut self, 
        method: &str, 
        params: Option<&Value>, 
        id: Option<&Value>,
        headers: &HashMap<String, String>
    ) -> Value {
        debug!("Handling MCP request: {}", method);

        // Skip auth for initialize method
        if method == "initialize" {
            return self.handle_initialize(id, params).await;
        }

        // Authenticate request
        let auth_context = match &self.auth {
            Some(auth) => {
                match auth.authenticate_request(method, params, headers).await {
                    Ok(ctx) => ctx,
                    Err(e) => {
                        error!("Authentication failed: {}", e);
                        return create_error_response(id, -32001, &format!("Authentication failed: {}", e));
                    }
                }
            }
            None => None,
        };

        // Check rate limits
        if let Some(ref rate_limiter) = self.rate_limiter {
            // Determine if we're in silent mode based on the tool/method
            let silent_mode = matches!(method, "harvest_conversation") || 
                params.and_then(|p| p.get("silent_mode"))
                    .and_then(|s| s.as_bool())
                    .unwrap_or(false);

            let tool_name = if method == "tools/call" {
                params.and_then(|p| p.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or("unknown")
            } else {
                method
            };

            if let Err(e) = rate_limiter.check_rate_limit(auth_context.as_ref(), tool_name, silent_mode).await {
                warn!("Rate limit exceeded for method: {}", method);
                return create_error_response(id, -32002, "Rate limit exceeded. Please try again later.");
            }
        }

        // Proceed with normal request handling
        match method {
            "tools/list" => self.handle_tools_list(id).await,
            "tools/call" => self.handle_tools_call(id, params, auth_context.as_ref()).await,
            "resources/list" => self.handle_resources_list(id).await,
            "prompts/list" => self.handle_prompts_list(id).await,
            _ => {
                warn!("Unknown method: {}", method);
                create_error_response(id, -32601, "Method not found")
            }
        }
    }

    /// Handle initialize request
    async fn handle_initialize(&self, id: Option<&Value>, _params: Option<&Value>) -> Value {
        info!("MCP server initializing");
        create_success_response(id, MCPTools::get_server_capabilities())
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
    async fn handle_tools_call(&mut self, id: Option<&Value>, params: Option<&Value>, auth_context: Option<&AuthContext>) -> Value {
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
                    return create_error_response(id, -32003, &format!("Access denied: {}", e));
                }
            }
        }

        debug!("Executing tool: {} with args: {}", tool_name, arguments);

        // Execute tool with circuit breaker protection if enabled
        if let Some(ref circuit_breaker) = self.circuit_breaker {
            match circuit_breaker.call(|| async {
                self.execute_tool(tool_name, arguments).await
            }).await {
                Ok(result) => create_success_response(id, result),
                Err(CircuitBreakerError::CircuitOpen) => {
                    create_error_response(id, -32603, "Service temporarily unavailable (circuit breaker open)")
                }
                Err(CircuitBreakerError::HalfOpenLimitExceeded) => {
                    create_error_response(id, -32603, "Service temporarily unavailable (half-open limit exceeded)")
                }
            }
        } else {
            match self.execute_tool(tool_name, arguments).await {
                Ok(result) => create_success_response(id, result),
                Err(e) => {
                    error!("Tool execution failed: {}", e);
                    create_error_response(id, -32603, &format!("Tool execution failed: {}", e))
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
        let content = args.get("content").and_then(|c| c.as_str()).unwrap();

        // Parse optional parameters
        let tier = args.get("tier")
            .and_then(|t| t.as_str())
            .and_then(|t| t.parse::<MemoryTier>().ok());

        let importance_score = args.get("importance_score")
            .and_then(|s| s.as_f64());

        let tags = args.get("tags")
            .and_then(|t| t.as_array())
            .map(|arr| {
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
        let memory = self.repository.create_memory(request).await?;

        let response_text = format!(
            "Successfully stored memory with ID: {}\nContent: {}\nTier: {:?}",
            memory.id,
            content.chars().take(100).collect::<String>(),
            memory.tier
        );

        Ok(format_tool_response(&response_text))
    }

    /// Execute search_memory tool
    async fn execute_search_memory(&self, args: &Value) -> Result<Value> {
        let query = args.get("query").and_then(|q| q.as_str()).unwrap();
        
        let limit = args.get("limit")
            .and_then(|l| l.as_i64())
            .map(|l| l as i32)
            .unwrap_or(10);

        let similarity_threshold = args.get("similarity_threshold")
            .and_then(|t| t.as_f64())
            .map(|t| t as f32)
            .unwrap_or(0.5);

        let tier = args.get("tier")
            .and_then(|t| t.as_str())
            .and_then(|t| t.parse::<MemoryTier>().ok());

        let include_metadata = args.get("include_metadata")
            .and_then(|m| m.as_bool())
            .unwrap_or(true);

        // Generate query embedding
        let embedding = self.embedder.generate_embedding(query).await?;

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

        // Perform search
        let results = self.repository.search_memories_simple(search_req).await?;

        if results.is_empty() {
            Ok(format_tool_response(&format!("No memories found for query: {}", query)))
        } else {
            let formatted_results = results.iter()
                .map(|r| {
                    let content_preview = r.memory.content.chars().take(200).collect::<String>();
                    format!(
                        "[Score: {:.3}] [Tier: {:?}] {}\n  Created: {}",
                        r.similarity_score,
                        r.memory.tier,
                        content_preview,
                        r.memory.created_at.format("%Y-%m-%d %H:%M")
                    )
                })
                .collect::<Vec<String>>()
                .join("\n\n");

            let response_text = format!("Found {} memories:\n\n{}", results.len(), formatted_results);
            Ok(format_tool_response(&response_text))
        }
    }

    /// Execute get_statistics tool
    async fn execute_get_statistics(&self, args: &Value) -> Result<Value> {
        let detailed = args.get("detailed")
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
        let context = args.get("context")
            .and_then(|c| c.as_str())
            .unwrap_or("conversation");

        let time_range = args.get("time_range")
            .and_then(|r| r.as_str())
            .unwrap_or("last_day");

        let limit = args.get("limit")
            .and_then(|l| l.as_i64())
            .map(|l| l as i32)
            .unwrap_or(10);

        // Calculate date range
        let now = Utc::now();
        let start_date = match time_range {
            "last_hour" => now - Duration::hours(1),
            "last_day" => now - Duration::days(1),
            "last_week" => now - Duration::weeks(1),
            "last_month" => now - Duration::days(30),
            _ => now - Duration::days(1),
        };

        // Search for recent memories
        let search_req = SearchRequest {
            query_text: Some(format!("context:{}", context)),
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
        let embedding = self.embedder.generate_embedding(&format!("context:{}", context)).await?;
        let mut search_req = search_req;
        search_req.query_embedding = Some(embedding);

        let results = self.repository.search_memories_simple(search_req).await?;

        if results.is_empty() {
            let response_text = format!(
                "I haven't remembered anything specific about {} in the {}. \
                 You might want to check if memories were properly harvested or stored.",
                context, time_range.replace('_', " ")
            );
            Ok(format_tool_response(&response_text))
        } else {
            let formatted_memories = results.iter()
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
                        age_str,
                        content_preview,
                        r.memory.tier,
                        r.memory.importance_score
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

    /// Execute harvest_conversation tool
    async fn execute_harvest_conversation(&self, args: &Value) -> Result<Value> {
        let message = args.get("message")
            .and_then(|m| m.as_str());

        let context = args.get("context")
            .and_then(|c| c.as_str())
            .unwrap_or("conversation");

        let role = args.get("role")
            .and_then(|r| r.as_str())
            .unwrap_or("user");

        let force_harvest = args.get("force_harvest")
            .and_then(|f| f.as_bool())
            .unwrap_or(false);

        let silent_mode = args.get("silent_mode")
            .and_then(|s| s.as_bool())
            .unwrap_or(true);

        // Add message to harvester queue if provided
        if let Some(message_content) = message {
            let conversation_message = ConversationMessage {
                id: Uuid::new_v4().to_string(),
                content: message_content.to_string(),
                timestamp: Utc::now(),
                role: role.to_string(),
                context: context.to_string(),
            };

            self.harvester_service.add_message(conversation_message).await?;
        }

        // Force harvest if requested
        if force_harvest {
            match self.harvester_service.force_harvest().await {
                Ok(result) => {
                    let response_text = if silent_mode {
                        format!("Harvest completed: {} messages processed", 
                                result.messages_processed)
                    } else {
                        format!("Force harvest completed:\n• Messages processed: {}\n• Patterns extracted: {}\n• Patterns stored: {}\n• Duplicates filtered: {}\n• Processing time: {}ms",
                                result.messages_processed,
                                result.patterns_extracted,
                                result.patterns_stored,
                                result.duplicates_filtered,
                                result.processing_time_ms)
                    };
                    Ok(format_tool_response(&response_text))
                }
                Err(e) => {
                    error!("Force harvest failed: {}", e);
                    Err(anyhow::anyhow!("Harvest failed: {}", e))
                }
            }
        } else {
            // Silent queuing mode
            let response_text = if message.is_some() {
                "Message queued for background harvesting"
            } else {
                "Background harvesting is active and monitoring conversations"
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
            metrics.last_harvest_time
                .map(|t| format!("{} ago", format_duration(Utc::now() - t)))
                .unwrap_or_else(|| "Never".to_string())
        );

        Ok(format_tool_response(&metrics_text))
    }

    /// Execute migrate_memory tool
    async fn execute_migrate_memory(&self, args: &Value) -> Result<Value> {
        let memory_id_str = args.get("memory_id").and_then(|id| id.as_str()).unwrap();
        let memory_id = Uuid::parse_str(memory_id_str)?;

        let target_tier = args.get("target_tier")
            .and_then(|t| t.as_str())
            .and_then(|t| t.parse::<MemoryTier>().ok())
            .unwrap();

        let reason = args.get("reason")
            .and_then(|r| r.as_str())
            .map(String::from);

        // Perform migration
        let updated_memory = self.repository.migrate_memory(memory_id, target_tier, reason.clone()).await?;

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
        let memory_id_str = args.get("memory_id").and_then(|id| id.as_str()).unwrap();
        let memory_id = Uuid::parse_str(memory_id_str)?;

        // Perform deletion
        self.repository.delete_memory(memory_id).await?;

        let response_text = format!("Successfully deleted memory {}", memory_id);
        Ok(format_tool_response(&response_text))
    }
}

/// Format duration for human-readable display
fn format_duration(duration: chrono::Duration) -> String {
    let total_seconds = duration.num_seconds();
    
    if total_seconds < 60 {
        format!("{}s", total_seconds)
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
        assert_eq!(format_duration(Duration::seconds(30)), "30s");
        assert_eq!(format_duration(Duration::minutes(5)), "300s");
        assert_eq!(format_duration(Duration::hours(2)), "2h");
        assert_eq!(format_duration(Duration::days(3)), "3d");
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