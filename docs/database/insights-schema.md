# Codex Dreams - Insights Database Schema

## Overview

The Codex Dreams feature adds automated insight generation capabilities to the Codex Memory System. This document details the database schema implementation for storing, searching, and managing AI-generated insights from stored memories.

**Feature Flag**: `codex-dreams`  
**Migration**: `014_codex_dreams_insights_schema.sql`  
**Created**: 2025-08-24  
**Story**: CODEX DREAMS Story 1 - Database Schema and Migrations

## Architecture Decisions

### 1. Separate Vector Table Strategy
We use a separate `insight_vectors` table instead of embedding vectors directly in the `insights` table for optimal performance:
- **Performance**: Vector operations can be isolated and optimized independently
- **Storage**: Reduces main table size for non-vector queries
- **Indexing**: HNSW indexes can be tuned specifically for insight vectors
- **Maintenance**: Vector maintenance operations don't lock the main insights data

### 2. Hierarchical Tier Integration
Insights follow the same tiering strategy as memories (`working` → `warm` → `cold` → `frozen`) to maintain system consistency and enable proper lifecycle management.

### 3. Feature Flag Protection
All schema changes are protected by the `codex-dreams` feature flag to ensure safe deployment and rollback capabilities.

## Schema Tables

### `insights` - Core Insights Storage

Primary table storing all generated insights with rich metadata and lifecycle management.

```sql
CREATE TABLE insights (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    content TEXT NOT NULL CHECK (char_length(content) >= 10 AND char_length(content) <= 65536),
    content_hash VARCHAR(64) NOT NULL, -- SHA-256 for deduplication
    
    -- Classification and Quality
    insight_type insight_type NOT NULL,
    confidence_score FLOAT NOT NULL DEFAULT 0.5 CHECK (confidence_score >= 0.0 AND confidence_score <= 1.0),
    feedback_score FLOAT DEFAULT 0.0 CHECK (feedback_score >= -1.0 AND feedback_score <= 1.0),
    
    -- Relationships
    source_memory_ids UUID[] NOT NULL DEFAULT '{}',
    related_insight_ids UUID[] DEFAULT '{}',
    
    -- Metadata and Categorization
    metadata JSONB NOT NULL DEFAULT '{}',
    tags TEXT[] NOT NULL DEFAULT '{}',
    
    -- Lifecycle Management
    tier memory_tier NOT NULL DEFAULT 'working',
    status memory_status NOT NULL DEFAULT 'active',
    
    -- Version Control
    version INTEGER NOT NULL DEFAULT 1,
    previous_version_id UUID REFERENCES insights(id),
    
    -- Temporal Tracking
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_accessed_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ,
    
    -- Generation Metadata
    generation_model VARCHAR(255),
    generation_prompt_hash VARCHAR(64),
    processing_duration_ms INTEGER
);
```

#### Key Features:
- **Deduplication**: Content hash prevents duplicate insights
- **Quality Scoring**: Confidence from AI model + feedback from users
- **Source Tracking**: Links back to originating memories
- **Version Control**: Supports iterative improvement
- **Lifecycle Management**: Same tiering as memories for consistency

### `insight_vectors` - Vector Embeddings

Separate table optimized for vector similarity search operations.

```sql
CREATE TABLE insight_vectors (
    insight_id UUID PRIMARY KEY REFERENCES insights(id) ON DELETE CASCADE,
    embedding vector(1536) NOT NULL, -- Match memory embedding dimensions
    embedding_model VARCHAR(255) NOT NULL DEFAULT 'text-embedding-3-small',
    embedding_created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    embedding_generation_ms INTEGER
);
```

#### Key Features:
- **Dimension Consistency**: 1536 dimensions to match memory embeddings
- **Model Tracking**: Records which embedding model was used
- **Performance Metrics**: Tracks embedding generation time

### `insight_feedback` - User Feedback System

Captures user feedback to improve insight quality over time.

```sql
CREATE TABLE insight_feedback (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    insight_id UUID NOT NULL REFERENCES insights(id) ON DELETE CASCADE,
    feedback_type insight_feedback_type NOT NULL, -- 'helpful', 'not_helpful', 'incorrect'
    feedback_text TEXT CHECK (char_length(feedback_text) <= 2048),
    user_context JSONB DEFAULT '{}',
    feedback_source VARCHAR(100) DEFAULT 'mcp_command',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

#### Key Features:
- **Structured Feedback**: Three-tier rating system
- **Contextual Data**: Optional detailed feedback and user context
- **Source Tracking**: Records how feedback was provided

### `processing_queue` - Batch Processing Management

Manages background processing of memory batches for insight generation.

```sql
CREATE TABLE processing_queue (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    memory_ids UUID[] NOT NULL,
    processing_type VARCHAR(50) NOT NULL DEFAULT 'insight_generation',
    status processing_status NOT NULL DEFAULT 'pending', -- 'pending', 'processing', 'completed', 'failed'
    priority INTEGER NOT NULL DEFAULT 5 CHECK (priority >= 1 AND priority <= 10),
    retry_count INTEGER NOT NULL DEFAULT 0,
    max_retries INTEGER NOT NULL DEFAULT 3,
    error_message TEXT,
    batch_size INTEGER NOT NULL DEFAULT 10,
    processing_options JSONB DEFAULT '{}',
    -- Temporal and metrics fields...
);
```

#### Key Features:
- **Batch Processing**: Groups memories for efficient processing
- **Priority Queue**: 1-10 priority system (10 = highest)
- **Retry Logic**: Exponential backoff for failed jobs
- **Error Tracking**: Detailed error logging for debugging

### `memories.processing_metadata` - Extended Memory Tracking

Adds processing state to existing memories table without breaking changes.

```sql
ALTER TABLE memories ADD COLUMN processing_metadata JSONB DEFAULT '{}';
```

#### Schema:
```json
{
  "last_processed_at": "2025-08-24T12:00:00Z",
  "processing_status": "completed",
  "insight_ids_generated": ["uuid1", "uuid2"],
  "last_insight_count": 2,
  "processing_errors": ["error message if any"],
  "processing_duration_ms": 1500
}
```

## Enums and Types

### `insight_type`
Categorizes insights based on cognitive patterns:
- `learning` - New knowledge or skill acquired
- `connection` - Relationship between concepts discovered  
- `relationship` - Personal or interpersonal pattern
- `assertion` - Strong belief or conviction formed
- `mental_model` - Conceptual framework established
- `pattern` - Behavioral or temporal pattern recognized

### `processing_status`
Tracks processing queue job states:
- `pending` - Queued for processing
- `processing` - Currently being processed
- `completed` - Successfully processed
- `failed` - Processing failed

### `insight_feedback_type`
User feedback categories:
- `helpful` - Insight was useful/accurate
- `not_helpful` - Insight was not useful
- `incorrect` - Insight contains factual errors

## Index Strategy

### Performance-Optimized Indexes

Our indexing strategy prioritizes the most common query patterns:

#### Primary Query Patterns:
1. **Find insights by type and quality**: `(insight_type, confidence_score DESC, created_at DESC)`
2. **Vector similarity search**: HNSW index on embeddings
3. **Source memory lookup**: GIN index on source_memory_ids array
4. **Tag-based filtering**: GIN index on tags array
5. **Processing queue management**: `(status, priority DESC, created_at ASC)`

#### HNSW Vector Index Configuration:
```sql
CREATE INDEX idx_insight_vectors_embedding_hnsw 
ON insight_vectors USING hnsw (embedding vector_cosine_ops) 
WITH (
    m = 48,                    -- Optimal for 1536-dimensional vectors
    ef_construction = 200      -- Balanced build time vs accuracy
);
```

**Parameters Explanation**:
- `m = 48`: Optimal connectivity for 1536-dimensional vectors (higher than default 16)
- `ef_construction = 200`: Provides >95% recall while maintaining reasonable build times
- `vector_cosine_ops`: Cosine similarity for normalized embeddings

## Triggers and Automation

### 1. Content Hash Generation
Automatically generates SHA-256 hash for deduplication:
```sql
CREATE TRIGGER generate_insight_hash 
    BEFORE INSERT OR UPDATE OF content, insight_type ON insights 
    FOR EACH ROW 
    EXECUTE FUNCTION generate_insight_content_hash();
```

### 2. Feedback Score Calculation  
Automatically updates insight feedback scores based on user ratings:
- `helpful`: +1.0 points
- `not_helpful`: -0.5 points  
- `incorrect`: -1.0 points
- Final score: weighted average from -1.0 to +1.0

### 3. Processing Queue Management
Automatically manages job state transitions:
- Sets `started_at` when status → `processing`
- Sets `completed_at` when status → `completed`/`failed`
- Calculates `processing_duration_ms`
- Schedules retry with exponential backoff for failed jobs

### 4. Updated Timestamp Maintenance
Automatically maintains `updated_at` timestamps for data consistency.

## Helper Functions

### `is_memory_ready_for_reprocessing(memory_id, interval_hours)`
Determines if a memory needs reprocessing based on last processing time.

### `mark_memories_as_processed(memory_ids, insight_ids)`
Updates memory processing metadata after successful insight generation.

### `get_insights_processing_stats()`
Returns comprehensive processing statistics including:
- Total insights by type
- Average confidence and feedback scores
- Processing queue status
- Top tags and trends

## Query Patterns and Performance

### Common Query Patterns:

#### 1. Find High-Quality Insights by Type
```sql
SELECT i.*, iv.embedding <=> $1 AS similarity
FROM insights i
LEFT JOIN insight_vectors iv ON i.id = iv.insight_id
WHERE i.insight_type = 'learning' 
  AND i.confidence_score >= 0.7
  AND i.status = 'active'
ORDER BY i.confidence_score DESC, i.created_at DESC
LIMIT 10;
```
**Performance**: Uses `idx_insights_type_confidence` index.

#### 2. Semantic Search Across Insights
```sql
SELECT i.*, iv.embedding <=> $1 AS similarity
FROM insight_vectors iv
JOIN insights i ON iv.insight_id = i.id
WHERE i.status = 'active'
ORDER BY iv.embedding <=> $1
LIMIT 10;
```
**Performance**: Uses HNSW index for <50ms P99 latency.

#### 3. Find Insights from Specific Memories
```sql
SELECT *
FROM insights 
WHERE source_memory_ids && ARRAY[$1, $2, $3]::UUID[]
  AND status = 'active'
ORDER BY confidence_score DESC;
```
**Performance**: Uses `idx_insights_source_memories` GIN index.

#### 4. Processing Queue Management
```sql
SELECT *
FROM processing_queue
WHERE status = 'pending'
ORDER BY priority DESC, created_at ASC
LIMIT 5;
```
**Performance**: Uses `idx_processing_queue_status_priority` index.

### Performance Targets:

| Operation | Target P99 Latency | Index Used |
|-----------|-------------------|------------|
| Type + Confidence Query | <10ms | `idx_insights_type_confidence` |
| Vector Similarity Search | <50ms | `idx_insight_vectors_embedding_hnsw` |
| Source Memory Lookup | <20ms | `idx_insights_source_memories` |
| Tag Filtering | <15ms | `idx_insights_tags` |
| Queue Processing | <5ms | `idx_processing_queue_status_priority` |

## Feature Flag Implementation

The schema is protected by the `codex-dreams` feature flag:

### Environment Variable Method:
```bash
export CODEX_DREAMS_ENABLED=true
```

### PostgreSQL Configuration Method:
```sql
SET codex.dreams_enabled = true;
```

### Migration Behavior:
- **Enabled**: Full schema creation with all tables, indexes, and functions
- **Disabled**: Migration skips with informational notice, no changes made
- **Rollback**: Complete removal of all insights-related schema elements

## Security Considerations

### 1. Content Validation
- Insights content: 10-65536 characters (prevents empty/huge insights)
- Feedback text: Max 2048 characters
- Source memories: 1-100 memory limit per insight

### 2. Data Integrity
- Foreign key constraints ensure referential integrity
- Check constraints prevent invalid scores and states
- Unique constraints prevent duplicate insights

### 3. Access Control
Schema changes are atomic and reversible, ensuring safe deployment and rollback.

## Migration and Deployment

### Forward Migration:
```bash
psql -f migration/migrations/014_codex_dreams_insights_schema.sql
```

### Rollback:
```bash
psql -f migration/migrations/014_codex_dreams_insights_schema_rollback.sql
```

### Validation:
The migration includes comprehensive validation to ensure:
- All tables created successfully
- All indexes built correctly  
- All functions and triggers active
- Processing metadata column added to memories
- Feature flag respected

### Post-Migration Steps:
1. Set `hnsw.ef_search = 64` in `postgresql.conf` for optimal vector query performance
2. Run `ANALYZE` on all new tables for query planner optimization
3. Monitor initial insight generation performance
4. Verify feature flag behavior in application code

## Monitoring and Maintenance

### Key Metrics to Monitor:
- Insight generation rate (insights/hour)
- Vector search performance (P99 latency)
- Processing queue depth and age
- Feedback score trends
- Storage growth rate

### Maintenance Tasks:
- Regular `ANALYZE` on insights tables (weekly)
- Monitor HNSW index size and performance
- Clean up old processing queue entries (monthly)
- Archive old insights based on feedback scores (quarterly)

### Troubleshooting:
- Use `get_insights_processing_stats()` for operational status
- Monitor processing queue for stuck jobs
- Check insight feedback trends for quality issues
- Validate vector index performance with `EXPLAIN ANALYZE`

## Next Steps

This schema supports the full Codex Dreams pipeline:

1. **Story 2**: Core Data Models - Rust structs matching this schema
2. **Story 3**: Ollama Client - LLM integration for insight generation  
3. **Story 4**: Memory Fetcher - Query optimization for batch processing
4. **Story 5**: Insight Storage - Repository layer implementation
5. **Story 6**: Processor/Orchestrator - End-to-end pipeline
6. **Story 7**: MCP Integration - User-facing commands
7. **Story 8**: Export Features - Markdown/JSON-LD output
8. **Story 9**: Background Scheduler - Automated processing

The database foundation is now ready to support the complete automated insight generation system! 🚀