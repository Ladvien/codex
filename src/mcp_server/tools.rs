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
        let mut tools = vec![
            json!({
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
            }),
            json!({
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
            }),
            json!({
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
            }),
            json!({
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
            }),
            json!({
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
            }),
            json!({
                "name": "get_harvester_metrics",
                "description": "Get metrics and status from the silent harvester service",
                "inputSchema": {
                    "type": "object",
                    "properties": {},
                    "required": []
                }
            }),
            json!({
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
            }),
            json!({
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
            }),
        ];

        // Add Codex Dreams insight tools if feature is enabled
        #[cfg(feature = "codex-dreams")]
        {
            tools.extend(vec![
                json!({
                    "name": "generate_insights",
                    "description": "★ Generate insights from memories (time period/topic)",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "time_period": {
                                "type": "string",
                                "enum": ["last_hour", "last_day", "last_week", "last_month", "all"],
                                "description": "Time period to analyze for insights",
                                "default": "last_day"
                            },
                            "topic": {
                                "type": "string",
                                "description": "Optional topic/theme to focus insight generation on"
                            },
                            "insight_type": {
                                "type": "string",
                                "enum": ["learning", "connection", "relationship", "assertion", "mental_model", "pattern", "all"],
                                "description": "Type of insight to generate",
                                "default": "all"
                            },
                            "max_insights": {
                                "type": "integer",
                                "minimum": 1,
                                "maximum": 20,
                                "default": 5,
                                "description": "Maximum number of insights to generate"
                            }
                        },
                        "required": []
                    }
                }),
                json!({
                    "name": "show_insights",
                    "description": "★ Display recent insights with confidence scores",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "limit": {
                                "type": "integer",
                                "minimum": 1,
                                "maximum": 50,
                                "default": 10,
                                "description": "Number of insights to display"
                            },
                            "insight_type": {
                                "type": "string",
                                "enum": ["learning", "connection", "relationship", "assertion", "mental_model", "pattern", "all"],
                                "description": "Filter by insight type",
                                "default": "all"
                            },
                            "min_confidence": {
                                "type": "number",
                                "minimum": 0.0,
                                "maximum": 1.0,
                                "default": 0.6,
                                "description": "Minimum confidence score threshold"
                            },
                            "include_feedback": {
                                "type": "boolean",
                                "default": true,
                                "description": "Include user feedback scores"
                            }
                        },
                        "required": []
                    }
                }),
                json!({
                    "name": "search_insights",
                    "description": "★ Search insights using semantic similarity",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "query": {
                                "type": "string",
                                "description": "Search query for insights"
                            },
                            "limit": {
                                "type": "integer",
                                "minimum": 1,
                                "maximum": 50,
                                "default": 10,
                                "description": "Maximum number of results"
                            },
                            "similarity_threshold": {
                                "type": "number",
                                "minimum": 0.0,
                                "maximum": 1.0,
                                "default": 0.7,
                                "description": "Minimum similarity score"
                            },
                            "insight_type": {
                                "type": "string",
                                "enum": ["learning", "connection", "relationship", "assertion", "mental_model", "pattern", "all"],
                                "description": "Filter by insight type",
                                "default": "all"
                            }
                        },
                        "required": ["query"]
                    }
                }),
                json!({
                    "name": "insight_feedback",
                    "description": "★ Provide feedback on insight quality (helpful/not helpful)",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "insight_id": {
                                "type": "string",
                                "description": "UUID of the insight to rate"
                            },
                            "helpful": {
                                "type": "boolean",
                                "description": "Whether the insight was helpful (true) or not (false)"
                            },
                            "comment": {
                                "type": "string",
                                "description": "Optional feedback comment"
                            }
                        },
                        "required": ["insight_id", "helpful"]
                    }
                }),
                json!({
                    "name": "export_insights",
                    "description": "★ Export insights in Markdown or JSON format",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "format": {
                                "type": "string",
                                "enum": ["markdown", "json"],
                                "description": "Export format",
                                "default": "markdown"
                            },
                            "time_period": {
                                "type": "string",
                                "enum": ["last_hour", "last_day", "last_week", "last_month", "all"],
                                "description": "Time period to export",
                                "default": "all"
                            },
                            "insight_type": {
                                "type": "string",
                                "enum": ["learning", "connection", "relationship", "assertion", "mental_model", "pattern", "all"],
                                "description": "Filter by insight type",
                                "default": "all"
                            },
                            "min_confidence": {
                                "type": "number",
                                "minimum": 0.0,
                                "maximum": 1.0,
                                "default": 0.6,
                                "description": "Minimum confidence score threshold"
                            },
                            "include_metadata": {
                                "type": "boolean",
                                "default": true,
                                "description": "Include metadata in export"
                            }
                        },
                        "required": []
                    }
                })
            ]);
        }

        json!({
            "tools": tools
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
                },
                "logging": {
                    "supported": true
                },
                "progress": {
                    "supported": true
                },
                "completion": {
                    "supported": true,
                    "argument": true
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

                // Validate tier if provided
                if let Some(tier) = args.get("tier").and_then(|t| t.as_str()) {
                    if !["working", "warm", "cold"].contains(&tier) {
                        return Err(
                            "Invalid tier. Must be 'working', 'warm', or 'cold'".to_string()
                        );
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
            #[cfg(feature = "codex-dreams")]
            "generate_insights" => {
                // Validate time_period if provided
                if let Some(period) = args.get("time_period").and_then(|p| p.as_str()) {
                    if !["last_hour", "last_day", "last_week", "last_month", "all"]
                        .contains(&period)
                    {
                        return Err("Invalid time period".to_string());
                    }
                }

                // Validate insight_type if provided
                if let Some(itype) = args.get("insight_type").and_then(|t| t.as_str()) {
                    if ![
                        "learning",
                        "connection",
                        "relationship",
                        "assertion",
                        "mental_model",
                        "pattern",
                        "all",
                    ]
                    .contains(&itype)
                    {
                        return Err("Invalid insight type".to_string());
                    }
                }

                // Validate max_insights if provided
                if let Some(max) = args.get("max_insights").and_then(|m| m.as_i64()) {
                    if !(1..=20).contains(&max) {
                        return Err("Max insights must be between 1 and 20".to_string());
                    }
                }
            }
            #[cfg(feature = "codex-dreams")]
            "show_insights" => {
                // Validate limit if provided
                if let Some(limit) = args.get("limit").and_then(|l| l.as_i64()) {
                    if !(1..=50).contains(&limit) {
                        return Err("Limit must be between 1 and 50".to_string());
                    }
                }

                // Validate min_confidence if provided
                if let Some(conf) = args.get("min_confidence").and_then(|c| c.as_f64()) {
                    if !(0.0..=1.0).contains(&conf) {
                        return Err("Minimum confidence must be between 0.0 and 1.0".to_string());
                    }
                }

                // Validate insight_type if provided
                if let Some(itype) = args.get("insight_type").and_then(|t| t.as_str()) {
                    if ![
                        "learning",
                        "connection",
                        "relationship",
                        "assertion",
                        "mental_model",
                        "pattern",
                        "all",
                    ]
                    .contains(&itype)
                    {
                        return Err("Invalid insight type".to_string());
                    }
                }
            }
            #[cfg(feature = "codex-dreams")]
            "search_insights" => {
                if args
                    .get("query")
                    .and_then(|q| q.as_str())
                    .is_none_or(|s| s.is_empty())
                {
                    return Err("Query is required and cannot be empty".to_string());
                }

                // Validate limit if provided
                if let Some(limit) = args.get("limit").and_then(|l| l.as_i64()) {
                    if !(1..=50).contains(&limit) {
                        return Err("Limit must be between 1 and 50".to_string());
                    }
                }

                // Validate similarity_threshold if provided
                if let Some(threshold) = args.get("similarity_threshold").and_then(|t| t.as_f64()) {
                    if !(0.0..=1.0).contains(&threshold) {
                        return Err("Similarity threshold must be between 0.0 and 1.0".to_string());
                    }
                }

                // Validate insight_type if provided
                if let Some(itype) = args.get("insight_type").and_then(|t| t.as_str()) {
                    if ![
                        "learning",
                        "connection",
                        "relationship",
                        "assertion",
                        "mental_model",
                        "pattern",
                        "all",
                    ]
                    .contains(&itype)
                    {
                        return Err("Invalid insight type".to_string());
                    }
                }
            }
            #[cfg(feature = "codex-dreams")]
            "insight_feedback" => {
                if args
                    .get("insight_id")
                    .and_then(|id| id.as_str())
                    .is_none_or(|s| s.is_empty())
                {
                    return Err("Insight ID is required".to_string());
                }

                if args.get("helpful").and_then(|h| h.as_bool()).is_none() {
                    return Err("Helpful flag is required (true/false)".to_string());
                }
            }
            #[cfg(feature = "codex-dreams")]
            "export_insights" => {
                // Validate format if provided
                if let Some(format) = args.get("format").and_then(|f| f.as_str()) {
                    if !["markdown", "json"].contains(&format) {
                        return Err("Invalid format. Must be 'markdown' or 'json'".to_string());
                    }
                }

                // Validate time_period if provided
                if let Some(period) = args.get("time_period").and_then(|p| p.as_str()) {
                    if !["last_hour", "last_day", "last_week", "last_month", "all"]
                        .contains(&period)
                    {
                        return Err("Invalid time period".to_string());
                    }
                }

                // Validate insight_type if provided
                if let Some(itype) = args.get("insight_type").and_then(|t| t.as_str()) {
                    if ![
                        "learning",
                        "connection",
                        "relationship",
                        "assertion",
                        "mental_model",
                        "pattern",
                        "all",
                    ]
                    .contains(&itype)
                    {
                        return Err("Invalid insight type".to_string());
                    }
                }

                // Validate min_confidence if provided
                if let Some(conf) = args.get("min_confidence").and_then(|c| c.as_f64()) {
                    if !(0.0..=1.0).contains(&conf) {
                        return Err("Minimum confidence must be between 0.0 and 1.0".to_string());
                    }
                }
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

        // Test search_memory with valid tier
        let valid_search = json!({
            "query": "test",
            "tier": "working"
        });
        assert!(MCPTools::validate_tool_args("search_memory", &valid_search).is_ok());

        // Test search_memory with invalid tier
        let invalid_search_tier = json!({
            "query": "test",
            "tier": "invalid"
        });
        assert!(MCPTools::validate_tool_args("search_memory", &invalid_search_tier).is_err());

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

    #[cfg(feature = "codex-dreams")]
    #[test]
    fn test_insight_tools_available() {
        let tools = MCPTools::get_tools_list();
        let tools_array = tools["tools"].as_array().unwrap();

        // Check that insight tools are included when feature is enabled
        let insight_tools = [
            "generate_insights",
            "show_insights",
            "search_insights",
            "insight_feedback",
            "export_insights",
        ];

        for tool_name in &insight_tools {
            let found = tools_array.iter().any(|t| t["name"] == *tool_name);
            assert!(
                found,
                "Insight tool {} should be available when codex-dreams feature is enabled",
                tool_name
            );
        }
    }

    #[cfg(feature = "codex-dreams")]
    #[test]
    fn test_insight_tool_validation() {
        // Test generate_insights validation
        let valid_generate = json!({
            "time_period": "last_day",
            "insight_type": "learning",
            "max_insights": 5
        });
        assert!(MCPTools::validate_tool_args("generate_insights", &valid_generate).is_ok());

        // Test search_insights validation - requires query
        let valid_search = json!({
            "query": "test query",
            "limit": 10
        });
        assert!(MCPTools::validate_tool_args("search_insights", &valid_search).is_ok());

        let invalid_search = json!({
            "limit": 10
        });
        assert!(MCPTools::validate_tool_args("search_insights", &invalid_search).is_err());

        // Test insight_feedback validation - requires insight_id and helpful
        let valid_feedback = json!({
            "insight_id": "123e4567-e89b-12d3-a456-426614174000",
            "helpful": true
        });
        assert!(MCPTools::validate_tool_args("insight_feedback", &valid_feedback).is_ok());
    }
}
