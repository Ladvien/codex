# Agentic Memory System API Reference

## Overview

The Agentic Memory System provides a comprehensive API for memory operations, search functionality, and system management. This document serves as the authoritative reference for all public APIs, including data structures, methods, error handling, and usage patterns.

## Table of Contents

1. [Core Types](#core-types)
2. [Memory Operations](#memory-operations)
3. [Search API](#search-api)
4. [MCP Protocol Methods](#mcp-protocol-methods)
5. [Error Handling](#error-handling)
6. [Authentication & Authorization](#authentication--authorization)
7. [Rate Limiting](#rate-limiting)
8. [Monitoring & Health](#monitoring--health)
9. [Configuration](#configuration)
10. [Examples](#examples)

## Core Types

### Memory

The central data structure representing a memory entry.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub id: Uuid,                           // Unique identifier
    pub content: String,                    // Memory content
    pub content_hash: String,               // SHA-256 hash for deduplication
    pub embedding: Option<Vec<f32>>,        // Vector embedding (1536 dimensions)
    pub tier: MemoryTier,                   // Storage tier
    pub status: MemoryStatus,               // Current status
    pub importance_score: f32,              // Importance score (0.0-1.0)
    pub access_count: i32,                  // Number of accesses
    pub last_accessed_at: Option<DateTime<Utc>>,  // Last access timestamp
    pub metadata: serde_json::Value,        // Structured metadata
    pub parent_id: Option<Uuid>,            // Parent memory ID
    pub created_at: DateTime<Utc>,          // Creation timestamp
    pub updated_at: DateTime<Utc>,          // Last update timestamp
    pub expires_at: Option<DateTime<Utc>>,  // Expiration timestamp
}
```

### MemoryTier

Defines the storage tier for memories with different performance characteristics.

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryTier {
    Working,  // Hot tier: <1ms P99, frequently accessed
    Warm,     // Warm tier: <100ms P99, moderately accessed  
    Cold,     // Cold tier: <20s P99, rarely accessed archive
}
```

### MemoryStatus

Represents the current status of a memory entry.

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryStatus {
    Active,     // Available for normal operations
    Migrating,  // Currently being moved between tiers
    Archived,   // Moved to long-term storage
    Deleted,    // Marked for deletion (soft delete)
}
```

### CreateMemoryRequest

Request structure for creating new memories.

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateMemoryRequest {
    #[validate(length(min = 1, max = 1048576))]  // Max 1MB content
    pub content: String,
    pub embedding: Option<Vec<f32>>,             // Auto-generated if None
    pub tier: Option<MemoryTier>,                // Default: Working
    #[validate(range(min = 0.0, max = 1.0))]
    pub importance_score: Option<f32>,           // Default: 0.5
    pub metadata: Option<serde_json::Value>,     // Default: {}
    pub parent_id: Option<Uuid>,                 // For hierarchical memories
    pub expires_at: Option<DateTime<Utc>>,       // TTL for auto-cleanup
}
```

### UpdateMemoryRequest  

Request structure for updating existing memories.

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UpdateMemoryRequest {
    #[validate(length(min = 1, max = 1048576))]
    pub content: Option<String>,
    pub embedding: Option<Vec<f32>>,
    pub tier: Option<MemoryTier>,
    #[validate(range(min = 0.0, max = 1.0))]
    pub importance_score: Option<f32>,
    pub metadata: Option<serde_json::Value>,
    pub expires_at: Option<DateTime<Utc>>,
}
```

### SearchRequest

Comprehensive search request with all available options.

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SearchRequest {
    pub query_text: Option<String>,                    // Text-based search
    pub query_embedding: Option<Vec<f32>>,             // Vector search
    pub search_type: Option<SearchType>,               // Search strategy
    pub hybrid_weights: Option<HybridWeights>,         // Weight distribution
    pub tier: Option<MemoryTier>,                      // Tier filter
    pub date_range: Option<DateRange>,                 // Time-based filter
    pub importance_range: Option<RangeFilter>,         // Importance filter
    pub metadata_filters: Option<serde_json::Value>,   // Metadata constraints
    pub tags: Option<Vec<String>>,                     // Tag filters
    #[validate(range(min = 1, max = 1000))]
    pub limit: Option<i32>,                            // Result limit
    pub offset: Option<i64>,                           // Pagination offset
    pub cursor: Option<String>,                        // Cursor-based pagination
    #[validate(range(min = 0.0, max = 1.0))]
    pub similarity_threshold: Option<f32>,             // Min similarity score
    pub include_metadata: Option<bool>,                // Include metadata in results
    pub include_facets: Option<bool>,                  // Include result facets
    pub ranking_boost: Option<RankingBoost>,           // Custom ranking weights
    pub explain_score: Option<bool>,                   // Include score explanation
}
```

### SearchResult

Result structure for search operations.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub memory: Memory,                       // The memory entry
    pub score: f32,                          // Relevance score (0.0-1.0)
    pub explanation: Option<ScoreExplanation>, // Score breakdown
    pub highlights: Option<Vec<Highlight>>,   // Content highlights
    pub distance: Option<f32>,               // Vector distance
}
```

## Memory Operations

### Creating Memories

#### POST /api/v1/memories

Creates a new memory entry with optional automatic embedding generation.

**Request Body:**
```json
{
  "content": "Sample memory content for testing",
  "tier": "Working",
  "importance_score": 0.8,
  "metadata": {
    "source": "api_test",
    "tags": ["example", "documentation"]
  },
  "expires_at": "2024-12-31T23:59:59Z"
}
```

**Response:**
```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "content": "Sample memory content for testing",
  "content_hash": "a665a45920422f9d417e4867efdc4fb8a04a1f3fff1fa07e998e86f7f7a27ae3",
  "embedding": [0.1, 0.2, ...],
  "tier": "Working",
  "status": "Active",
  "importance_score": 0.8,
  "access_count": 0,
  "last_accessed_at": null,
  "metadata": {
    "source": "api_test",
    "tags": ["example", "documentation"]
  },
  "parent_id": null,
  "created_at": "2024-01-15T10:30:00Z",
  "updated_at": "2024-01-15T10:30:00Z",
  "expires_at": "2024-12-31T23:59:59Z"
}
```

### Retrieving Memories

#### GET /api/v1/memories/{id}

Retrieves a specific memory by ID and increments access count.

**Path Parameters:**
- `id` (UUID): Memory identifier

**Response:**
```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "content": "Sample memory content",
  "tier": "Working",
  "access_count": 5,
  "last_accessed_at": "2024-01-15T14:22:10Z",
  ...
}
```

### Updating Memories

#### PUT /api/v1/memories/{id}

Updates an existing memory with partial update support.

**Request Body:**
```json
{
  "content": "Updated memory content",
  "importance_score": 0.9,
  "tier": "Warm"
}
```

### Deleting Memories

#### DELETE /api/v1/memories/{id}

Performs soft delete by marking memory as deleted.

**Response:**
```json
{
  "success": true,
  "message": "Memory marked for deletion",
  "deleted_at": "2024-01-15T15:30:00Z"
}
```

## Search API

### Basic Search

#### POST /api/v1/search

Performs hybrid search combining text and vector similarity.

**Request Body:**
```json
{
  "query_text": "machine learning algorithms",
  "limit": 10,
  "similarity_threshold": 0.7,
  "include_metadata": true,
  "tier": "Working"
}
```

**Response:**
```json
{
  "results": [
    {
      "memory": {
        "id": "550e8400-e29b-41d4-a716-446655440000",
        "content": "Overview of machine learning algorithms including supervised and unsupervised methods",
        "score": 0.95,
        "tier": "Working",
        ...
      },
      "score": 0.95,
      "highlights": [
        {
          "field": "content",
          "fragment": "Overview of <em>machine learning algorithms</em> including"
        }
      ]
    }
  ],
  "total_count": 15,
  "facets": {
    "tier": {"Working": 8, "Warm": 6, "Cold": 1},
    "source": {"user_input": 10, "system": 5}
  },
  "query_time_ms": 45
}
```

### Advanced Search

#### POST /api/v1/search/advanced

Advanced search with complex filtering and ranking options.

**Request Body:**
```json
{
  "query_text": "debugging rust async code",
  "hybrid_weights": {
    "semantic_weight": 0.6,
    "temporal_weight": 0.2,
    "importance_weight": 0.2
  },
  "metadata_filters": {
    "language": "rust",
    "type": "troubleshooting"
  },
  "date_range": {
    "start": "2024-01-01T00:00:00Z",
    "end": "2024-01-15T23:59:59Z"
  },
  "ranking_boost": {
    "recency_boost": 1.2,
    "importance_boost": 1.5
  },
  "explain_score": true
}
```

### Semantic Search

#### POST /api/v1/search/semantic

Pure vector-based semantic search.

**Request Body:**
```json
{
  "query_embedding": [0.1, 0.2, 0.3, ...],
  "similarity_threshold": 0.8,
  "limit": 5
}
```

## MCP Protocol Methods

The system implements the Model Context Protocol for integration with Claude applications.

### memory.create

Creates a new memory through MCP.

**Parameters:**
```json
{
  "content": "string",
  "tier": "Working" | "Warm" | "Cold",
  "importance_score": 0.8,
  "metadata": {}
}
```

### memory.get

Retrieves memory by ID.

**Parameters:**
```json
{
  "memory_id": "uuid"
}
```

### memory.update

Updates existing memory.

**Parameters:**
```json
{
  "memory_id": "uuid",
  "updates": {
    "content": "string",
    "importance_score": 0.9
  }
}
```

### memory.delete

Deletes memory by ID.

**Parameters:**
```json
{
  "memory_id": "uuid"
}
```

### memory.search

Searches memories with MCP protocol.

**Parameters:**
```json
{
  "query": "search terms",
  "options": {
    "limit": 10,
    "tier": "Working"
  }
}
```

### memory.list_tiers

Lists all available memory tiers.

**Response:**
```json
{
  "tiers": [
    {
      "name": "Working",
      "description": "Hot tier for frequently accessed memories",
      "performance": "<1ms P99"
    },
    {
      "name": "Warm", 
      "description": "Warm tier for moderately accessed memories",
      "performance": "<100ms P99"
    },
    {
      "name": "Cold",
      "description": "Cold tier for archived memories", 
      "performance": "<20s P99"
    }
  ]
}
```

### memory.health

Returns system health status.

**Response:**
```json
{
  "status": "healthy",
  "database_connected": true,
  "embedding_service_available": true,
  "metrics": {
    "total_memories": 15420,
    "working_tier_count": 1250,
    "warm_tier_count": 8170,
    "cold_tier_count": 6000
  }
}
```

## Error Handling

### Error Response Format

All API errors follow a consistent format:

```json
{
  "error": {
    "code": "MEMORY_NOT_FOUND",
    "message": "Memory with ID 550e8400-e29b-41d4-a716-446655440000 not found",
    "details": {
      "memory_id": "550e8400-e29b-41d4-a716-446655440000",
      "timestamp": "2024-01-15T10:30:00Z"
    },
    "request_id": "req_123456789"
  }
}
```

### Common Error Codes

| Code | HTTP Status | Description |
|------|-------------|-------------|
| `MEMORY_NOT_FOUND` | 404 | Memory with specified ID not found |
| `INVALID_REQUEST` | 400 | Request validation failed |
| `DUPLICATE_CONTENT` | 409 | Content already exists in specified tier |
| `CONTENT_TOO_LARGE` | 413 | Content exceeds maximum size limit |
| `RATE_LIMIT_EXCEEDED` | 429 | Request rate limit exceeded |
| `UNAUTHORIZED` | 401 | Authentication required |
| `FORBIDDEN` | 403 | Insufficient permissions |
| `INTERNAL_ERROR` | 500 | Unexpected server error |
| `SERVICE_UNAVAILABLE` | 503 | Service temporarily unavailable |
| `DATABASE_ERROR` | 500 | Database operation failed |
| `EMBEDDING_ERROR` | 500 | Embedding generation failed |

### Error Handling Best Practices

1. **Always check HTTP status codes** before processing responses
2. **Use error codes** for programmatic error handling, not messages
3. **Include request_id** in error reports for debugging
4. **Implement exponential backoff** for retryable errors
5. **Log detailed error information** for monitoring and debugging

## Authentication & Authorization

### API Key Authentication

Include API key in request headers:

```
Authorization: Bearer <your-api-key>
```

### JWT Token Authentication

For user-based authentication:

```
Authorization: Bearer <jwt-token>
```

### Permissions

| Operation | Required Permission |
|-----------|-------------------|
| Create Memory | `memory:write` |
| Read Memory | `memory:read` |
| Update Memory | `memory:write` |
| Delete Memory | `memory:delete` |
| Search Memories | `memory:search` |
| Admin Operations | `admin:*` |

## Rate Limiting

### Rate Limit Headers

All responses include rate limiting information:

```
X-RateLimit-Limit: 1000
X-RateLimit-Remaining: 999
X-RateLimit-Reset: 1642248000
X-RateLimit-Window: 3600
```

### Default Limits

| Tier | Requests/Hour | Burst |
|------|---------------|-------|
| Free | 100 | 10 |
| Basic | 1,000 | 50 |
| Premium | 10,000 | 200 |
| Enterprise | Unlimited | 1000 |

### Rate Limiting Best Practices

1. **Monitor rate limit headers** and adjust request frequency
2. **Implement exponential backoff** when limits are exceeded
3. **Use bulk operations** when possible to reduce API calls
4. **Cache results** to minimize repeated requests
5. **Distribute requests** across time to avoid burst limits

## Monitoring & Health

### Health Check Endpoint

#### GET /api/v1/health

Returns comprehensive system health status.

**Response:**
```json
{
  "status": "healthy",
  "timestamp": "2024-01-15T10:30:00Z",
  "uptime_seconds": 86400,
  "version": "1.0.0",
  "components": {
    "database": {
      "status": "healthy",
      "response_time_ms": 2,
      "connection_pool": {
        "active": 5,
        "idle": 15,
        "max": 20
      }
    },
    "embedding_service": {
      "status": "healthy",
      "response_time_ms": 45
    },
    "memory_tiers": {
      "working": {"count": 1250, "avg_response_ms": 0.8},
      "warm": {"count": 8170, "avg_response_ms": 45},
      "cold": {"count": 6000, "avg_response_ms": 2500}
    }
  },
  "metrics": {
    "requests_per_second": 125.5,
    "error_rate": 0.002,
    "memory_usage_mb": 512,
    "cpu_usage_percent": 15.2
  }
}
```

### Metrics Endpoint

#### GET /api/v1/metrics

Returns Prometheus-compatible metrics.

```
# HELP memory_operations_total Total number of memory operations
# TYPE memory_operations_total counter
memory_operations_total{operation="create"} 1542
memory_operations_total{operation="read"} 8751
memory_operations_total{operation="update"} 324
memory_operations_total{operation="delete"} 89

# HELP memory_tier_distribution Number of memories by tier
# TYPE memory_tier_distribution gauge
memory_tier_distribution{tier="working"} 1250
memory_tier_distribution{tier="warm"} 8170
memory_tier_distribution{tier="cold"} 6000
```

## Configuration

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `DATABASE_URL` | PostgreSQL connection string | Required |
| `EMBEDDING_SERVICE_URL` | Embedding service endpoint | `http://localhost:8080` |
| `API_PORT` | Server port | `3333` |
| `LOG_LEVEL` | Logging level | `info` |
| `RATE_LIMIT_PER_HOUR` | Default rate limit | `1000` |
| `MAX_CONTENT_SIZE` | Max memory content size | `1048576` |
| `CONNECTION_POOL_SIZE` | Database connection pool size | `20` |

### Configuration File

Example `config.toml`:

```toml
[server]
port = 3333
host = "0.0.0.0"

[database]
url = "postgresql://user:pass@localhost/memory_db"
max_connections = 20
connection_timeout_seconds = 30

[embedding]
service_url = "http://localhost:8080"
timeout_seconds = 30
retry_attempts = 3

[rate_limiting]
default_per_hour = 1000
burst_size = 50

[logging]
level = "info"
format = "json"
```

## Examples

### Basic Memory Operations

```rust
use codex_memory::{MemoryRepository, CreateMemoryRequest, MemoryTier};

// Create a memory
let request = CreateMemoryRequest {
    content: "Example memory content".to_string(),
    tier: Some(MemoryTier::Working),
    importance_score: Some(0.8),
    metadata: Some(json!({"type": "example"})),
    ..Default::default()
};

let memory = repository.create_memory(request).await?;

// Retrieve the memory
let retrieved = repository.get_memory(memory.id).await?;

// Update the memory
let update = UpdateMemoryRequest {
    content: Some("Updated content".to_string()),
    tier: Some(MemoryTier::Warm),
    ..Default::default()
};

let updated = repository.update_memory(memory.id, update).await?;
```

### Search Operations

```rust
use codex_memory::{SearchRequest, SearchType};

// Basic text search
let search = SearchRequest {
    query_text: Some("machine learning".to_string()),
    limit: Some(10),
    similarity_threshold: Some(0.7),
    ..Default::default()
};

let results = repository.search_memories_simple(search).await?;

// Advanced search with filters
let advanced_search = SearchRequest {
    query_text: Some("rust programming".to_string()),
    tier: Some(MemoryTier::Working),
    metadata_filters: Some(json!({"language": "rust"})),
    importance_range: Some(RangeFilter {
        min: Some(0.8),
        max: None,
    }),
    limit: Some(5),
    ..Default::default()
};

let filtered_results = repository.search_memories_simple(advanced_search).await?;
```

### MCP Integration

```rust
use codex_memory::mcp::MCPServer;

// Initialize MCP server
let mcp_server = MCPServer::new(repository, embedder)?;

// Start server
mcp_server.start(([127, 0, 0, 1], 3333).into()).await?;
```

### Error Handling

```rust
use codex_memory::memory::error::MemoryError;

match repository.get_memory(memory_id).await {
    Ok(memory) => {
        // Process memory
        println!("Found memory: {}", memory.content);
    },
    Err(MemoryError::NotFound { id }) => {
        // Handle not found
        eprintln!("Memory {} not found", id);
    },
    Err(MemoryError::DatabaseError(e)) => {
        // Handle database error
        eprintln!("Database error: {}", e);
    },
    Err(e) => {
        // Handle other errors
        eprintln!("Unexpected error: {}", e);
    }
}
```

This API reference provides comprehensive documentation for all public interfaces of the Agentic Memory System. For implementation details, see the [Architecture Documentation](architecture.md) and [Developer Guide](claude_developer_guide.md).

For the latest API documentation generated from code, visit: `/target/doc/codex_memory/index.html` (generated via `cargo doc`).