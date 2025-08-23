//! MCP Tools Definition and Schema
//!
//! This module defines the tools exposed through the MCP protocol,
//! including their schemas and capabilities for memory management.

use serde_json::{json, Value};

/// MCP Tools registry and schema definitions
pub struct MCPTools;

impl MCPTools {
    /// Get the list of available tools with their schemas
    pub fn get_tools_list() -> Value {
        json!({
            "tools": [
                {
                    "name": "store_memory",
                    "description": "Store a memory in the hierarchical memory system",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "content": {
                                "type": "string",
                                "description": "The content to store as a memory"
                            },
                            "tier": {
                                "type": "string",
                                "enum": ["working", "warm", "cold"],
                                "description": "The tier to store the memory in (defaults to automatic placement)",
                                "default": "working"
                            },
                            "tags": {
                                "type": "array",
                                "items": {"type": "string"},
                                "description": "Optional tags to associate with the memory for categorization"
                            },
                            "importance_score": {
                                "type": "number",
                                "minimum": 0.0,
                                "maximum": 1.0,
                                "description": "Optional importance score (0.0 to 1.0) for the memory"
                            },
                            "metadata": {
                                "type": "object",
                                "description": "Optional additional metadata to store with the memory"
                            }
                        },
                        "required": ["content"]
                    }
                },
                {
                    "name": "search_memory",
                    "description": "Search memories using semantic similarity",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "query": {
                                "type": "string",
                                "description": "The search query text"
                            },
                            "limit": {
                                "type": "integer",
                                "default": 10,
                                "minimum": 1,
                                "maximum": 100,
                                "description": "Maximum number of results to return"
                            },
                            "similarity_threshold": {
                                "type": "number",
                                "minimum": 0.0,
                                "maximum": 1.0,
                                "default": 0.5,
                                "description": "Minimum similarity score for results"
                            },
                            "tier": {
                                "type": "string",
                                "enum": ["working", "warm", "cold"],
                                "description": "Optional tier filter to search within"
                            },
                            "tags": {
                                "type": "array",
                                "items": {"type": "string"},
                                "description": "Optional tags to filter results by"
                            },
                            "include_metadata": {
                                "type": "boolean",
                                "default": true,
                                "description": "Whether to include metadata in results"
                            }
                        },
                        "required": ["query"]
                    }
                },
                {
                    "name": "get_statistics",
                    "description": "Get memory system statistics and metrics",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "detailed": {
                                "type": "boolean",
                                "default": false,
                                "description": "Whether to include detailed metrics"
                            }
                        },
                        "required": []
                    }
                },
                {
                    "name": "what_did_you_remember",
                    "description": "Query what the system has remembered about recent interactions",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "context": {
                                "type": "string",
                                "description": "Optional context to filter memories by (e.g., 'conversation', 'project')",
                                "default": "conversation"
                            },
                            "time_range": {
                                "type": "string",
                                "enum": ["last_hour", "last_day", "last_week", "last_month"],
                                "description": "Time range to search within",
                                "default": "last_day"
                            },
                            "limit": {
                                "type": "integer",
                                "minimum": 1,
                                "maximum": 50,
                                "default": 10,
                                "description": "Maximum number of memories to return"
                            }
                        },
                        "required": []
                    }
                },
                {
                    "name": "harvest_conversation",
                    "description": "Trigger the silent harvester to process current conversation context",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "message": {
                                "type": "string",
                                "description": "Message content to harvest"
                            },
                            "context": {
                                "type": "string",
                                "description": "Context for the message (e.g., 'conversation', 'project')",
                                "default": "conversation"
                            },
                            "role": {
                                "type": "string",
                                "enum": ["user", "assistant", "system"],
                                "description": "Role of the message sender",
                                "default": "user"
                            },
                            "force_harvest": {
                                "type": "boolean",
                                "default": false,
                                "description": "Force immediate harvest instead of queuing"
                            },
                            "silent_mode": {
                                "type": "boolean",
                                "default": true,
                                "description": "Run in silent mode (minimal output)"
                            }
                        },
                        "required": []
                    }
                },
                {
                    "name": "get_harvester_metrics",
                    "description": "Get metrics and status from the silent harvester service",
                    "inputSchema": {
                        "type": "object",
                        "properties": {},
                        "required": []
                    }
                },
                {
                    "name": "migrate_memory",
                    "description": "Move a memory between tiers in the hierarchical system",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "memory_id": {
                                "type": "string",
                                "description": "UUID of the memory to migrate"
                            },
                            "target_tier": {
                                "type": "string",
                                "enum": ["working", "warm", "cold", "frozen"],
                                "description": "Target tier for migration"
                            },
                            "reason": {
                                "type": "string",
                                "description": "Optional reason for migration"
                            }
                        },
                        "required": ["memory_id", "target_tier"]
                    }
                },
                {
                    "name": "delete_memory",
                    "description": "Delete a specific memory from the system",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "memory_id": {
                                "type": "string",
                                "description": "UUID of the memory to delete"
                            },
                            "confirm": {
                                "type": "boolean",
                                "default": false,
                                "description": "Confirmation flag to prevent accidental deletions"
                            }
                        },
                        "required": ["memory_id", "confirm"]
                    }
                }
            ]
        })
    }

    /// Get empty resources list (codex-memory doesn't expose resources)
    pub fn get_resources_list() -> Value {
        json!({
            "resources": []
        })
    }

    /// Get empty prompts list (codex-memory doesn't use prompts)
    pub fn get_prompts_list() -> Value {
        json!({
            "prompts": []
        })
    }

    /// Get server capabilities for MCP initialization
    pub fn get_server_capabilities() -> Value {
        json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {
                "tools": {
                    "listChanged": false
                },
                "resources": {
                    "listChanged": false
                },
                "prompts": {
                    "listChanged": false
                }
            },
            "serverInfo": {
                "name": "codex-memory",
                "version": env!("CARGO_PKG_VERSION"),
                "description": "Hierarchical memory system with semantic search and automated consolidation"
            }
        })
    }

    /// Validate tool arguments against schema
    pub fn validate_tool_args(tool_name: &str, args: &Value) -> Result<(), String> {
        match tool_name {
            "store_memory" => {
                if args
                    .get("content")
                    .and_then(|c| c.as_str())
                    .is_none_or(|s| s.is_empty())
                {
                    return Err("Content is required and cannot be empty".to_string());
                }

                // Validate tier if provided
                if let Some(tier) = args.get("tier").and_then(|t| t.as_str()) {
                    if !["working", "warm", "cold"].contains(&tier) {
                        return Err(
                            "Invalid tier. Must be 'working', 'warm', or 'cold'".to_string()
                        );
                    }
                }

                // Validate importance score if provided
                if let Some(score) = args.get("importance_score").and_then(|s| s.as_f64()) {
                    if !(0.0..=1.0).contains(&score) {
                        return Err("Importance score must be between 0.0 and 1.0".to_string());
                    }
                }
            }
            "search_memory" => {
                if args
                    .get("query")
                    .and_then(|q| q.as_str())
                    .is_none_or(|s| s.is_empty())
                {
                    return Err("Query is required and cannot be empty".to_string());
                }

                // Validate limit if provided
                if let Some(limit) = args.get("limit").and_then(|l| l.as_i64()) {
                    if !(1..=100).contains(&limit) {
                        return Err("Limit must be between 1 and 100".to_string());
                    }
                }

                // Validate similarity threshold if provided
                if let Some(threshold) = args.get("similarity_threshold").and_then(|t| t.as_f64()) {
                    if !(0.0..=1.0).contains(&threshold) {
                        return Err("Similarity threshold must be between 0.0 and 1.0".to_string());
                    }
                }
            }
            "migrate_memory" => {
                if args
                    .get("memory_id")
                    .and_then(|id| id.as_str())
                    .is_none_or(|s| s.is_empty())
                {
                    return Err("Memory ID is required".to_string());
                }

                if let Some(tier) = args.get("target_tier").and_then(|t| t.as_str()) {
                    if !["working", "warm", "cold", "frozen"].contains(&tier) {
                        return Err("Invalid target tier".to_string());
                    }
                } else {
                    return Err("Target tier is required".to_string());
                }
            }
            "delete_memory" => {
                if args
                    .get("memory_id")
                    .and_then(|id| id.as_str())
                    .is_none_or(|s| s.is_empty())
                {
                    return Err("Memory ID is required".to_string());
                }

                if !args
                    .get("confirm")
                    .and_then(|c| c.as_bool())
                    .unwrap_or(false)
                {
                    return Err("Confirmation required for deletion".to_string());
                }
            }
            "what_did_you_remember" => {
                // Validate time_range if provided
                if let Some(range) = args.get("time_range").and_then(|r| r.as_str()) {
                    if !["last_hour", "last_day", "last_week", "last_month"].contains(&range) {
                        return Err("Invalid time range".to_string());
                    }
                }
            }
            "harvest_conversation" => {
                // Validate role if provided
                if let Some(role) = args.get("role").and_then(|r| r.as_str()) {
                    if !["user", "assistant", "system"].contains(&role) {
                        return Err(
                            "Invalid role. Must be 'user', 'assistant', or 'system'".to_string()
                        );
                    }
                }
            }
            "get_statistics" | "get_harvester_metrics" => {
                // These tools don't require validation
            }
            _ => return Err(format!("Unknown tool: {tool_name}")),
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tools_list_structure() {
        let tools = MCPTools::get_tools_list();
        let tools_array = tools["tools"].as_array().unwrap();

        assert!(!tools_array.is_empty());

        // Check that store_memory tool exists with required schema
        let store_memory = tools_array
            .iter()
            .find(|t| t["name"] == "store_memory")
            .unwrap();

        assert_eq!(store_memory["name"], "store_memory");
        assert!(store_memory["inputSchema"]["properties"]["content"].is_object());
        assert_eq!(store_memory["inputSchema"]["required"][0], "content");
    }

    #[test]
    fn test_tool_validation() {
        // Test valid store_memory args
        let valid_args = json!({
            "content": "Test memory",
            "tier": "working",
            "importance_score": 0.8
        });
        assert!(MCPTools::validate_tool_args("store_memory", &valid_args).is_ok());

        // Test invalid content
        let invalid_args = json!({
            "content": "",
            "tier": "working"
        });
        assert!(MCPTools::validate_tool_args("store_memory", &invalid_args).is_err());

        // Test invalid tier
        let invalid_tier = json!({
            "content": "Test",
            "tier": "invalid"
        });
        assert!(MCPTools::validate_tool_args("store_memory", &invalid_tier).is_err());

        // Test unknown tool
        assert!(MCPTools::validate_tool_args("unknown_tool", &valid_args).is_err());
    }

    #[test]
    fn test_server_capabilities() {
        let capabilities = MCPTools::get_server_capabilities();
        assert_eq!(capabilities["protocolVersion"], "2025-06-18");
        assert_eq!(capabilities["serverInfo"]["name"], "codex-memory");
        assert!(capabilities["capabilities"]["tools"]["listChanged"] == false);
    }
}
