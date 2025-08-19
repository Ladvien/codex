# Claude Developer Guide - Agentic Memory System Integration

## Overview

The Agentic Memory System provides persistent, searchable memory capabilities for Claude Code and Claude Desktop applications. This guide covers how to integrate with and use the memory system effectively.

## Table of Contents

1. [Quick Start](#quick-start)
2. [Architecture Overview](#architecture-overview) 
3. [MCP Protocol Integration](#mcp-protocol-integration)
4. [Memory Operations](#memory-operations)
5. [Search and Retrieval](#search-and-retrieval)
6. [Performance Considerations](#performance-considerations)
7. [Error Handling](#error-handling)
8. [Best Practices](#best-practices)
9. [Troubleshooting](#troubleshooting)
10. [API Reference](#api-reference)

## Quick Start

### Prerequisites

- Rust 1.70+
- PostgreSQL 14+ with pgvector extension
- Claude Code or Claude Desktop application

### Basic Setup

```rust
use codex_memory::{
    Config,
    MemoryRepository,
    MCPServer,
    SimpleEmbedder,
    memory::models::*,
};

// Initialize the memory system
let config = Config::from_env()?;
let pool = create_pool(&config.database_url).await?;
let repository = Arc::new(MemoryRepository::new(pool));
let embedder = Arc::new(SimpleEmbedder::new());
let mcp_server = MCPServer::new(repository.clone(), embedder)?;

// Start the MCP server
mcp_server.start(([127, 0, 0, 1], 3333).into()).await?;
```

### Creating Your First Memory

```rust
let request = CreateMemoryRequest {
    content: "Claude helped debug a Rust async issue with tokio".to_string(),
    embedding: None, // Will be auto-generated
    tier: Some(MemoryTier::Working),
    importance_score: Some(0.8),
    metadata: Some(json!({
        "session_id": "your_session_id",
        "application": "claude_code",
        "language": "rust",
        "topic": "debugging"
    })),
    parent_id: None,
    expires_at: None,
};

let memory = repository.create_memory(request).await?;
println!("Created memory with ID: {}", memory.id);
```

## Architecture Overview

### Memory Tiers

The system uses a three-tier architecture:

- **Working Memory**: Recently accessed, high-importance memories (fast access)
- **Warm Storage**: Moderately important memories (balanced access)  
- **Cold Storage**: Long-term, archived memories (slower but persistent)

### Components

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   Claude Code   │    │ Claude Desktop  │    │  Other Clients  │
└─────────────────┘    └─────────────────┘    └─────────────────┘
         │                       │                       │
         └───────────────────────┼───────────────────────┘
                                 │
              ┌─────────────────────────────────┐
              │        MCP Protocol Layer       │
              └─────────────────────────────────┘
                                 │
              ┌─────────────────────────────────┐
              │      Memory Repository          │
              │  - CRUD Operations              │
              │  - Search & Retrieval           │  
              │  - Tier Management              │
              └─────────────────────────────────┘
                                 │
              ┌─────────────────────────────────┐
              │     PostgreSQL + pgvector       │
              │  - Working Memory (hot)         │
              │  - Warm Storage                 │
              │  - Cold Storage (archived)      │
              └─────────────────────────────────┘
```

## MCP Protocol Integration

### Connection Establishment

Claude applications connect to the memory system via the MCP (Model Context Protocol):

```typescript
// Claude Code/Desktop client example
const mcpClient = new MCPClient({
    transport: new TCPTransport('localhost', 3333),
    capabilities: {
        memory: true,
        search: true,
        persistence: true
    }
});

await mcpClient.connect();
```

### Supported MCP Methods

| Method | Description | Parameters |
|--------|-------------|------------|
| `memory.create` | Create new memory | `CreateMemoryRequest` |
| `memory.get` | Retrieve memory by ID | `memory_id: string` |
| `memory.update` | Update existing memory | `memory_id: string, UpdateMemoryRequest` |
| `memory.delete` | Delete memory | `memory_id: string` |
| `memory.search` | Search memories | `SearchRequest` |
| `memory.list_tiers` | List all memory tiers | - |
| `memory.health` | Check system health | - |

## Memory Operations

### Creating Memories

Different types of content require different approaches:

#### Code Context
```rust
let code_memory = CreateMemoryRequest {
    content: format!("File: {} - Function: {} - Context: {}", 
                     file_path, function_name, context),
    tier: Some(MemoryTier::Working),
    importance_score: Some(0.9), // High importance for active code
    metadata: Some(json!({
        "type": "code_context",
        "file_path": file_path,
        "function": function_name,
        "language": "rust",
        "line_number": 42
    })),
    parent_id: None,
    expires_at: None,
};
```

#### Conversation Context
```rust
let conversation_memory = CreateMemoryRequest {
    content: "User asked about implementing OAuth2 in Rust. Recommended using the oauth2 crate with async/await patterns.".to_string(),
    tier: Some(MemoryTier::Working),
    importance_score: Some(0.7),
    metadata: Some(json!({
        "type": "conversation",
        "conversation_id": conversation_id,
        "message_type": "assistant_response",
        "topic": "oauth2",
        "technologies": ["rust", "oauth2", "async"]
    })),
    parent_id: None,
    expires_at: Some(Utc::now() + Duration::days(30)), // Auto-expire old conversations
};
```

#### Document Analysis
```rust
let document_memory = CreateMemoryRequest {
    content: "Analyzed technical documentation for REST API design. Key patterns: resource-based URLs, HTTP verbs, stateless design.".to_string(),
    tier: Some(MemoryTier::Warm),
    importance_score: Some(0.8),
    metadata: Some(json!({
        "type": "document_analysis", 
        "document_title": "REST API Best Practices",
        "analysis_date": Utc::now(),
        "key_topics": ["rest", "api", "design", "http"]
    })),
    parent_id: None,
    expires_at: None,
};
```

### Updating Memories

```rust
let update_request = UpdateMemoryRequest {
    content: Some("Updated content with new insights".to_string()),
    tier: Some(MemoryTier::Warm), // Demote from Working to Warm
    importance_score: Some(0.6), // Reduce importance over time
    metadata: Some(json!({
        "updated_at": Utc::now(),
        "update_reason": "additional_context"
    })),
    expires_at: None,
};

let updated_memory = repository.update_memory(memory_id, update_request).await?;
```

## Search and Retrieval

### Basic Text Search

```rust
let search_request = SearchRequest {
    query_text: Some("rust async await patterns".to_string()),
    limit: Some(10),
    similarity_threshold: Some(0.7),
    include_metadata: Some(true),
    ..Default::default()
};

let results = repository.search_memories_simple(search_request).await?;
```

### Filtered Search

```rust
let filtered_search = SearchRequest {
    query_text: Some("debugging".to_string()),
    metadata_filters: Some(json!({
        "language": "rust",
        "type": "code_context"
    })),
    tier: Some(MemoryTier::Working),
    importance_range: Some(RangeFilter {
        min: Some(0.8),
        max: None,
    }),
    limit: Some(5),
    ..Default::default()
};
```

### Temporal Search

```rust
let temporal_search = SearchRequest {
    query_text: Some("API design".to_string()),
    date_range: Some(DateRange {
        start: Some(Utc::now() - Duration::days(7)),
        end: Some(Utc::now()),
    }),
    ranking_boost: Some(RankingBoost {
        recency_boost: Some(1.2),
        importance_boost: Some(1.5),
        ..Default::default()
    }),
    ..Default::default()
};
```

### Context-Aware Search

```rust
let context_search = SearchRequest {
    query_text: Some("similar debugging approach".to_string()),
    metadata_filters: Some(json!({
        "session_id": current_session_id
    })),
    hybrid_weights: Some(HybridWeights {
        semantic_weight: 0.6,
        temporal_weight: 0.2,
        importance_weight: 0.2,
        access_frequency_weight: 0.0,
    }),
    ..Default::default()
};
```

## Performance Considerations

### Memory Tier Strategy

- **Working Memory**: Keep actively used memories here (< 1ms access)
- **Warm Storage**: Move occasionally accessed memories (< 100ms access)
- **Cold Storage**: Archive old memories (< 20s access acceptable)

### Batch Operations

```rust
// Batch create for better performance
let memories: Vec<CreateMemoryRequest> = conversation_turns
    .into_iter()
    .map(|turn| CreateMemoryRequest {
        content: turn.content,
        // ... other fields
    })
    .collect();

// Process in batches of 100
for batch in memories.chunks(100) {
    for memory_request in batch {
        repository.create_memory(memory_request.clone()).await?;
    }
}
```

### Connection Pooling

```rust
// Configure connection pool for your workload
let config = Config {
    database_pool_size: 20,
    database_max_connections: 100,
    database_connection_timeout: Duration::from_secs(30),
    ..Default::default()
};
```

### Search Optimization

```rust
// Use specific filters to reduce search space
let optimized_search = SearchRequest {
    query_text: Some("rust error handling".to_string()),
    tier: Some(MemoryTier::Working), // Limit to working memory only
    limit: Some(5), // Limit results for faster response
    metadata_filters: Some(json!({
        "language": "rust" // Pre-filter by language
    })),
    similarity_threshold: Some(0.8), // Higher threshold = fewer results
    ..Default::default()
};
```

## Error Handling

### Common Error Patterns

```rust
use codex_memory::memory::error::MemoryError;

match repository.get_memory(memory_id).await {
    Ok(memory) => {
        // Handle successful retrieval
        process_memory(memory);
    }
    Err(MemoryError::NotFound { id }) => {
        // Memory doesn't exist
        warn!("Memory {} not found", id);
    }
    Err(MemoryError::DatabaseError(e)) => {
        // Database connection or query error
        error!("Database error: {}", e);
    }
    Err(MemoryError::DuplicateContent { tier }) => {
        // Content already exists in this tier
        info!("Duplicate content in tier {}", tier);
    }
    Err(e) => {
        // Other errors
        error!("Unexpected error: {}", e);
    }
}
```

### Graceful Degradation

```rust
async fn search_with_fallback(
    repository: &MemoryRepository,
    query: &str
) -> Vec<SearchResult> {
    // Try semantic search first
    let semantic_search = SearchRequest {
        query_text: Some(query.to_string()),
        similarity_threshold: Some(0.7),
        limit: Some(10),
        ..Default::default()
    };
    
    match repository.search_memories_simple(semantic_search).await {
        Ok(results) if !results.is_empty() => results,
        _ => {
            // Fallback to simple text search
            let fallback_search = SearchRequest {
                query_text: Some(query.to_string()),
                similarity_threshold: Some(0.5), // Lower threshold
                limit: Some(20), // More results
                ..Default::default()
            };
            
            repository.search_memories_simple(fallback_search).await
                .unwrap_or_else(|_| Vec::new())
        }
    }
}
```

## Best Practices

### 1. Metadata Design

Use structured metadata for effective filtering:

```rust
// Good: Structured metadata
let metadata = json!({
    "application": "claude_code",
    "session_id": session_id,
    "file_info": {
        "path": "/src/main.rs",
        "language": "rust",
        "project": "my_project"
    },
    "interaction": {
        "type": "code_assistance",
        "topic": "error_handling",
        "difficulty": "intermediate"
    },
    "tags": ["rust", "error", "result", "anyhow"]
});

// Bad: Unstructured metadata
let bad_metadata = json!({
    "info": "some rust code stuff"
});
```

### 2. Content Formatting

Structure content for better searchability:

```rust
// Good: Structured content
let content = format!(
    "Context: {} | Problem: {} | Solution: {} | Code: {}",
    context_description,
    problem_statement, 
    solution_approach,
    code_snippet
);

// Good: JSON structured content for complex data
let structured_content = json!({
    "type": "code_review",
    "file": file_path,
    "issues": issues_found,
    "suggestions": improvement_suggestions,
    "code_snippet": code
}).to_string();
```

### 3. Importance Scoring

Use consistent importance scoring:

```rust
fn calculate_importance(context: &InteractionContext) -> f32 {
    let mut score = 0.5; // Base score
    
    // Boost for user-initiated interactions
    if context.user_initiated { score += 0.2; }
    
    // Boost for error/problem solving
    if context.involves_error { score += 0.3; }
    
    // Boost for code generation
    if context.involves_code_gen { score += 0.2; }
    
    // Recent interactions are more important
    let hours_ago = context.hours_since_interaction();
    if hours_ago < 1.0 { score += 0.1; }
    
    score.min(1.0)
}
```

### 4. Session Management

Organize memories by session:

```rust
// Create session-scoped memories
let session_id = Uuid::new_v4().to_string();

// Link related memories
let parent_memory = repository.create_memory(initial_request).await?;

let follow_up_request = CreateMemoryRequest {
    content: "Follow-up question about the previous topic".to_string(),
    parent_id: Some(parent_memory.id),
    metadata: Some(json!({
        "session_id": session_id,
        "sequence": 2,
        "parent_topic": "rust_error_handling"
    })),
    ..Default::default()
};
```

### 5. Memory Lifecycle Management

Implement proper cleanup:

```rust
// Set expiration for temporary memories
let temp_memory = CreateMemoryRequest {
    content: "Temporary debug information".to_string(),
    expires_at: Some(Utc::now() + Duration::hours(24)),
    ..Default::default()
};

// Archive old memories instead of deleting
let archive_request = UpdateMemoryRequest {
    tier: Some(MemoryTier::Cold),
    importance_score: Some(0.1), // Low importance for archived
    metadata: Some(json!({
        "archived_at": Utc::now(),
        "archive_reason": "age_based_archival"
    })),
    ..Default::default()
};
```

## Troubleshooting

### Common Issues

#### 1. Connection Errors

```
Error: Failed to connect to database
```

**Solution:**
- Check PostgreSQL is running
- Verify connection string in environment variables
- Ensure pgvector extension is installed

```bash
# Install pgvector
CREATE EXTENSION vector;

# Test connection
psql "postgresql://user:password@localhost/dbname"
```

#### 2. Search Returns No Results

**Potential Causes:**
- Embedding service not available
- Similarity threshold too high
- No matching metadata filters

**Debug Steps:**
```rust
// Lower similarity threshold
let debug_search = SearchRequest {
    query_text: Some("your query".to_string()),
    similarity_threshold: Some(0.3), // Lower threshold
    limit: Some(50), // More results
    ..Default::default()
};

// Check total memory count
let total_memories = repository.get_statistics().await?.total_memories;
println!("Total memories in system: {}", total_memories);
```

#### 3. Slow Search Performance

**Optimization Steps:**
- Add appropriate database indexes
- Use tier-specific searches
- Reduce similarity threshold
- Add metadata filters

```sql
-- Add performance indexes
CREATE INDEX idx_memories_tier ON memories(tier);
CREATE INDEX idx_memories_metadata ON memories USING gin(metadata);
CREATE INDEX idx_memories_importance ON memories(importance_score DESC);
```

### Monitoring and Debugging

Enable detailed logging:

```rust
// Enable debug logging
env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();

// Monitor performance
let start = Instant::now();
let results = repository.search_memories_simple(search_request).await?;
let duration = start.elapsed();
debug!("Search completed in {:?}, found {} results", duration, results.len());
```

Monitor system health:

```rust
let health = health_checker.check_system_health().await?;
println!("System health: {:?}", health);

if health.database_connected && health.embedding_service_available {
    println!("✅ All systems operational");
} else {
    println!("⚠️  System degraded");
}
```

## API Reference

### Core Types

```rust
// Memory creation request
pub struct CreateMemoryRequest {
    pub content: String,
    pub embedding: Option<Vec<f32>>,
    pub tier: Option<MemoryTier>,
    pub importance_score: Option<f32>,
    pub metadata: Option<serde_json::Value>,
    pub parent_id: Option<Uuid>,
    pub expires_at: Option<DateTime<Utc>>,
}

// Memory update request  
pub struct UpdateMemoryRequest {
    pub content: Option<String>,
    pub embedding: Option<Vec<f32>>,
    pub tier: Option<MemoryTier>,
    pub importance_score: Option<f32>,
    pub metadata: Option<serde_json::Value>,
    pub expires_at: Option<DateTime<Utc>>,
}

// Search request with all options
pub struct SearchRequest {
    pub query_text: Option<String>,
    pub query_embedding: Option<Vec<f32>>,
    pub search_type: Option<SearchType>,
    pub tier: Option<MemoryTier>,
    pub limit: Option<i32>,
    pub offset: Option<i64>,
    pub similarity_threshold: Option<f32>,
    pub metadata_filters: Option<serde_json::Value>,
    // ... additional fields
}
```

### Repository Methods

```rust
impl MemoryRepository {
    // CRUD operations
    pub async fn create_memory(&self, request: CreateMemoryRequest) -> Result<Memory>;
    pub async fn get_memory(&self, id: Uuid) -> Result<Memory>;
    pub async fn update_memory(&self, id: Uuid, request: UpdateMemoryRequest) -> Result<Memory>;
    pub async fn delete_memory(&self, id: Uuid) -> Result<()>;
    
    // Search operations
    pub async fn search_memories(&self, request: SearchRequest) -> Result<SearchResponse>;
    pub async fn search_memories_simple(&self, request: SearchRequest) -> Result<Vec<SearchResult>>;
    
    // Utility operations
    pub async fn get_statistics(&self) -> Result<MemoryStatistics>;
    pub async fn get_memories_by_tier(&self, tier: MemoryTier) -> Result<Vec<Memory>>;
}
```

## Support and Resources

- **GitHub Issues**: [Report bugs and request features](https://github.com/anthropics/claude-code/issues)
- **Documentation**: [Full API documentation](https://docs.anthropic.com/claude-memory)
- **Performance Guide**: [Optimization best practices](./performance_tuning_guide.md)
- **Migration Guide**: [Upgrading between versions](./migration_guide.md)

---

*Last updated: January 2025*
*Version: 1.0*