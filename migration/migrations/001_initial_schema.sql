-- Migration 001: Initial Schema Setup
-- Purpose: Create base tables for hierarchical memory system with pgvector support

-- Enable required extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pgvector";
CREATE EXTENSION IF NOT EXISTS "pg_stat_statements";

-- Create enum types for memory tiers
CREATE TYPE memory_tier AS ENUM ('working', 'warm', 'cold');
CREATE TYPE memory_status AS ENUM ('active', 'migrating', 'archived', 'deleted');

-- Main memory table with hierarchical structure
CREATE TABLE IF NOT EXISTS memories (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    content TEXT NOT NULL,
    content_hash VARCHAR(64) NOT NULL, -- SHA-256 hash for deduplication
    embedding vector(1536), -- Configurable dimension, defaulting to OpenAI's size
    tier memory_tier NOT NULL DEFAULT 'working',
    status memory_status NOT NULL DEFAULT 'active',
    importance_score FLOAT NOT NULL DEFAULT 0.5 CHECK (importance_score >= 0 AND importance_score <= 1),
    access_count INTEGER NOT NULL DEFAULT 0,
    last_accessed_at TIMESTAMPTZ,
    metadata JSONB NOT NULL DEFAULT '{}',
    parent_id UUID REFERENCES memories(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ, -- For TTL-based eviction
    
    -- Constraints
    UNIQUE(content_hash, tier), -- Prevent duplicates within same tier
    CHECK (char_length(content) <= 1048576) -- Max 1MB per memory
);

-- Memory summaries table for hierarchical summarization
CREATE TABLE IF NOT EXISTS memory_summaries (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    summary_level VARCHAR(20) NOT NULL CHECK (summary_level IN ('hour', 'day', 'week', 'month', 'year')),
    summary_content TEXT NOT NULL,
    summary_embedding vector(1536),
    start_time TIMESTAMPTZ NOT NULL,
    end_time TIMESTAMPTZ NOT NULL,
    memory_count INTEGER NOT NULL DEFAULT 0,
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Ensure no overlapping summaries at same level
    EXCLUDE USING gist (
        tstzrange(start_time, end_time) WITH &&,
        summary_level WITH =
    )
);

-- Memory clusters for semantic grouping
CREATE TABLE IF NOT EXISTS memory_clusters (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    cluster_name VARCHAR(255) NOT NULL,
    centroid_embedding vector(1536) NOT NULL,
    concept_tags TEXT[] NOT NULL DEFAULT '{}',
    member_count INTEGER NOT NULL DEFAULT 0,
    tier memory_tier NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    UNIQUE(cluster_name, tier)
);

-- Relationship table between memories and summaries
CREATE TABLE IF NOT EXISTS memory_summary_mappings (
    memory_id UUID NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    summary_id UUID NOT NULL REFERENCES memory_summaries(id) ON DELETE CASCADE,
    relevance_score FLOAT NOT NULL DEFAULT 1.0 CHECK (relevance_score >= 0 AND relevance_score <= 1),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    PRIMARY KEY (memory_id, summary_id)
);

-- Relationship table between memories and clusters
CREATE TABLE IF NOT EXISTS memory_cluster_mappings (
    memory_id UUID NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    cluster_id UUID NOT NULL REFERENCES memory_clusters(id) ON DELETE CASCADE,
    distance_to_centroid FLOAT NOT NULL,
    assigned_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    PRIMARY KEY (memory_id, cluster_id)
);

-- Migration history table for tracking tier transitions
CREATE TABLE IF NOT EXISTS migration_history (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    memory_id UUID NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    from_tier memory_tier NOT NULL,
    to_tier memory_tier NOT NULL,
    migration_reason VARCHAR(255),
    migrated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    migration_duration_ms INTEGER,
    success BOOLEAN NOT NULL DEFAULT true,
    error_message TEXT,
    
    CHECK (from_tier != to_tier)
);

-- Backup metadata table
CREATE TABLE IF NOT EXISTS backup_metadata (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    backup_type VARCHAR(50) NOT NULL CHECK (backup_type IN ('full', 'incremental', 'differential')),
    backup_location TEXT NOT NULL,
    backup_size_bytes BIGINT,
    start_time TIMESTAMPTZ NOT NULL,
    end_time TIMESTAMPTZ NOT NULL,
    success BOOLEAN NOT NULL,
    error_message TEXT,
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create optimized indexes for each tier

-- Working memory indexes (optimized for speed)
CREATE INDEX idx_memories_working_embedding ON memories 
    USING hnsw (embedding vector_cosine_ops) 
    WHERE tier = 'working' AND status = 'active';

CREATE INDEX idx_memories_working_importance ON memories (importance_score DESC, last_accessed_at DESC NULLS LAST) 
    WHERE tier = 'working' AND status = 'active';

CREATE INDEX idx_memories_working_access ON memories (access_count DESC, updated_at DESC) 
    WHERE tier = 'working' AND status = 'active';

-- Warm memory indexes (balanced)
CREATE INDEX idx_memories_warm_embedding ON memories 
    USING hnsw (embedding vector_cosine_ops) 
    WHERE tier = 'warm' AND status = 'active';

CREATE INDEX idx_memories_warm_temporal ON memories (created_at DESC, updated_at DESC) 
    WHERE tier = 'warm' AND status = 'active';

-- Cold memory indexes (optimized for storage)
CREATE INDEX idx_memories_cold_hash ON memories (content_hash) 
    WHERE tier = 'cold';

CREATE INDEX idx_memories_cold_metadata ON memories 
    USING gin (metadata) 
    WHERE tier = 'cold' AND status = 'active';

-- General indexes
CREATE INDEX idx_memories_parent ON memories (parent_id) 
    WHERE parent_id IS NOT NULL;

CREATE INDEX idx_memories_expires ON memories (expires_at) 
    WHERE expires_at IS NOT NULL AND status = 'active';

CREATE INDEX idx_memories_status_tier ON memories (status, tier);

-- Summary indexes
CREATE INDEX idx_summaries_embedding ON memory_summaries 
    USING hnsw (summary_embedding vector_cosine_ops);

CREATE INDEX idx_summaries_time_range ON memory_summaries 
    USING gist (tstzrange(start_time, end_time));

CREATE INDEX idx_summaries_level_time ON memory_summaries (summary_level, start_time DESC);

-- Cluster indexes
CREATE INDEX idx_clusters_embedding ON memory_clusters 
    USING hnsw (centroid_embedding vector_cosine_ops);

CREATE INDEX idx_clusters_tags ON memory_clusters 
    USING gin (concept_tags);

-- Migration history indexes
CREATE INDEX idx_migration_history_memory ON migration_history (memory_id, migrated_at DESC);
CREATE INDEX idx_migration_history_time ON migration_history (migrated_at DESC);

-- Create update trigger for updated_at
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER update_memories_updated_at 
    BEFORE UPDATE ON memories 
    FOR EACH ROW 
    EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_memory_summaries_updated_at 
    BEFORE UPDATE ON memory_summaries 
    FOR EACH ROW 
    EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_memory_clusters_updated_at 
    BEFORE UPDATE ON memory_clusters 
    FOR EACH ROW 
    EXECUTE FUNCTION update_updated_at_column();

-- Create function for content deduplication
CREATE OR REPLACE FUNCTION check_content_duplicate()
RETURNS TRIGGER AS $$
BEGIN
    -- Generate content hash
    NEW.content_hash = encode(digest(NEW.content, 'sha256'), 'hex');
    
    -- Check for existing duplicate in same tier
    IF EXISTS (
        SELECT 1 FROM memories 
        WHERE content_hash = NEW.content_hash 
        AND tier = NEW.tier 
        AND status = 'active'
        AND id != COALESCE(NEW.id, uuid_generate_v4())
    ) THEN
        RAISE EXCEPTION 'Duplicate content already exists in tier %', NEW.tier;
    END IF;
    
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER check_memory_duplicate 
    BEFORE INSERT OR UPDATE ON memories 
    FOR EACH ROW 
    EXECUTE FUNCTION check_content_duplicate();

-- Create function for automatic access tracking
CREATE OR REPLACE FUNCTION track_memory_access()
RETURNS TRIGGER AS $$
BEGIN
    -- Only track for SELECT operations in application context
    IF TG_OP = 'SELECT' THEN
        UPDATE memories 
        SET 
            access_count = access_count + 1,
            last_accessed_at = NOW()
        WHERE id = NEW.id;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Performance tuning settings (to be applied at session or database level)
-- These are comments for documentation; actual settings should be applied via ALTER SYSTEM or postgresql.conf

-- Memory settings for pgvector index building
-- SET maintenance_work_mem = '2GB';
-- SET max_parallel_maintenance_workers = 4;

-- Connection pooling recommendations
-- max_connections = 200
-- shared_buffers = 4GB
-- effective_cache_size = 12GB
-- work_mem = 32MB

-- Autovacuum settings for high-write workload
-- autovacuum_max_workers = 4
-- autovacuum_naptime = 10s
-- autovacuum_vacuum_threshold = 50
-- autovacuum_analyze_threshold = 50