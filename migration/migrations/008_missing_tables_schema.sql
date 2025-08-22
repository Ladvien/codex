-- Migration 008: Missing Database Tables Schema
-- Story: MEDIUM-002 - Create Missing Database Tables
-- Purpose: Add harvest sessions table and fix any remaining table gaps
-- Author: PostgreSQL optimization expert
-- Date: 2025-08-22

BEGIN;

-- Add frozen tier to memory_tier enum if it doesn't exist
DO $$ 
BEGIN
    BEGIN
        ALTER TYPE memory_tier ADD VALUE 'frozen';
    EXCEPTION
        WHEN duplicate_object THEN
            -- Frozen tier already exists, continue
            NULL;
    END;
END $$;

-- Create harvest_sessions table for tracking silent harvester operations
CREATE TABLE IF NOT EXISTS harvest_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_type VARCHAR(50) NOT NULL CHECK (session_type IN ('silent', 'manual', 'scheduled', 'forced')),
    trigger_reason TEXT NOT NULL,
    started_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    completed_at TIMESTAMP WITH TIME ZONE,
    status VARCHAR(20) NOT NULL DEFAULT 'in_progress' CHECK (status IN ('in_progress', 'completed', 'failed', 'cancelled')),
    
    -- Processing metrics
    messages_processed INTEGER DEFAULT 0,
    patterns_extracted INTEGER DEFAULT 0,
    patterns_stored INTEGER DEFAULT 0,
    duplicates_filtered INTEGER DEFAULT 0,
    processing_time_ms BIGINT DEFAULT 0,
    
    -- Configuration snapshot for reproducibility
    config_snapshot JSONB DEFAULT '{}',
    
    -- Error handling
    error_message TEXT,
    retry_count INTEGER DEFAULT 0,
    
    -- Performance metrics
    extraction_time_ms BIGINT DEFAULT 0,
    deduplication_time_ms BIGINT DEFAULT 0,
    storage_time_ms BIGINT DEFAULT 0,
    
    -- Resource usage tracking
    memory_usage_mb FLOAT,
    cpu_usage_percent FLOAT,
    
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Create harvest_patterns table for tracking extracted patterns before they become memories
CREATE TABLE IF NOT EXISTS harvest_patterns (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    harvest_session_id UUID NOT NULL REFERENCES harvest_sessions(id) ON DELETE CASCADE,
    pattern_type VARCHAR(50) NOT NULL CHECK (pattern_type IN ('preference', 'fact', 'decision', 'correction', 'emotion', 'goal', 'relationship', 'skill')),
    content TEXT NOT NULL,
    confidence_score FLOAT NOT NULL CHECK (confidence_score >= 0.0 AND confidence_score <= 1.0),
    source_message_id VARCHAR(255),
    context TEXT,
    metadata JSONB DEFAULT '{}',
    
    -- Processing status
    status VARCHAR(20) NOT NULL DEFAULT 'extracted' CHECK (status IN ('extracted', 'stored', 'duplicate', 'rejected')),
    memory_id UUID REFERENCES memories(id) ON DELETE SET NULL, -- Links to created memory if stored
    rejection_reason TEXT,
    
    -- Extraction metrics
    extraction_confidence FLOAT CHECK (extraction_confidence >= 0.0 AND extraction_confidence <= 1.0),
    similarity_to_existing FLOAT CHECK (similarity_to_existing >= 0.0 AND similarity_to_existing <= 1.0),
    
    extracted_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Create consolidation_events table for comprehensive tier migration tracking
-- This complements the existing memory_consolidation_log with additional event types
CREATE TABLE IF NOT EXISTS consolidation_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_type VARCHAR(50) NOT NULL CHECK (event_type IN ('tier_migration', 'importance_update', 'access_decay', 'batch_consolidation', 'manual_override')),
    memory_id UUID NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    
    -- Tier migration details
    source_tier VARCHAR(20),
    target_tier VARCHAR(20),
    migration_reason TEXT,
    
    -- Consolidation strength tracking
    old_consolidation_strength FLOAT,
    new_consolidation_strength FLOAT,
    strength_delta FLOAT,
    
    -- Recall probability tracking
    old_recall_probability FLOAT,
    new_recall_probability FLOAT,
    probability_delta FLOAT,
    
    -- Performance metrics
    processing_time_ms INTEGER,
    
    -- Context and metadata
    triggered_by VARCHAR(100), -- 'user', 'system', 'scheduler', 'background_service'
    context_metadata JSONB DEFAULT '{}',
    
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Create memory_access_log table for detailed access tracking
CREATE TABLE IF NOT EXISTS memory_access_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    memory_id UUID NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    access_type VARCHAR(50) NOT NULL CHECK (access_type IN ('search', 'direct_retrieval', 'similarity_match', 'reflection_analysis', 'consolidation_process')),
    
    -- Access context
    session_id UUID, -- Could reference various session types
    user_context VARCHAR(255),
    query_context TEXT,
    
    -- Performance metrics
    retrieval_time_ms INTEGER,
    similarity_score FLOAT CHECK (similarity_score >= 0.0 AND similarity_score <= 1.0),
    ranking_position INTEGER,
    
    -- Impact tracking
    importance_boost FLOAT DEFAULT 0.0,
    access_count_increment INTEGER DEFAULT 1,
    
    accessed_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Create system_metrics_snapshots table for performance monitoring
CREATE TABLE IF NOT EXISTS system_metrics_snapshots (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    snapshot_type VARCHAR(50) NOT NULL CHECK (snapshot_type IN ('hourly', 'daily', 'weekly', 'on_demand', 'incident')),
    
    -- Memory tier statistics
    working_memory_count INTEGER DEFAULT 0,
    warm_memory_count INTEGER DEFAULT 0,
    cold_memory_count INTEGER DEFAULT 0,
    frozen_memory_count INTEGER DEFAULT 0,
    
    -- Storage metrics
    total_storage_bytes BIGINT DEFAULT 0,
    compressed_storage_bytes BIGINT DEFAULT 0,
    average_compression_ratio FLOAT,
    
    -- Performance metrics
    average_query_time_ms FLOAT,
    p95_query_time_ms FLOAT,
    p99_query_time_ms FLOAT,
    slow_query_count INTEGER DEFAULT 0,
    
    -- Memory system health
    consolidation_backlog INTEGER DEFAULT 0,
    migration_queue_size INTEGER DEFAULT 0,
    failed_operations_count INTEGER DEFAULT 0,
    
    -- Vector index performance
    vector_index_size_mb FLOAT,
    vector_search_performance JSONB DEFAULT '{}',
    
    -- System resources
    database_cpu_percent FLOAT,
    database_memory_mb FLOAT,
    connection_count INTEGER,
    active_connections INTEGER,
    
    recorded_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Create improved indexes for optimal query performance

-- Harvest sessions indexes
CREATE INDEX IF NOT EXISTS harvest_sessions_status_started_at_idx 
ON harvest_sessions (status, started_at DESC);

CREATE INDEX IF NOT EXISTS harvest_sessions_session_type_idx 
ON harvest_sessions (session_type);

CREATE INDEX IF NOT EXISTS harvest_sessions_completed_at_idx 
ON harvest_sessions (completed_at DESC) WHERE completed_at IS NOT NULL;

-- Harvest patterns indexes
CREATE INDEX IF NOT EXISTS harvest_patterns_session_id_idx 
ON harvest_patterns (harvest_session_id);

CREATE INDEX IF NOT EXISTS harvest_patterns_pattern_type_idx 
ON harvest_patterns (pattern_type);

CREATE INDEX IF NOT EXISTS harvest_patterns_status_confidence_idx 
ON harvest_patterns (status, confidence_score DESC);

CREATE INDEX IF NOT EXISTS harvest_patterns_memory_id_idx 
ON harvest_patterns (memory_id) WHERE memory_id IS NOT NULL;

-- Consolidation events indexes
CREATE INDEX IF NOT EXISTS consolidation_events_memory_id_created_at_idx 
ON consolidation_events (memory_id, created_at DESC);

CREATE INDEX IF NOT EXISTS consolidation_events_event_type_idx 
ON consolidation_events (event_type);

CREATE INDEX IF NOT EXISTS consolidation_events_tier_migration_idx 
ON consolidation_events (source_tier, target_tier) WHERE event_type = 'tier_migration';

-- Memory access log indexes
CREATE INDEX IF NOT EXISTS memory_access_log_memory_id_accessed_at_idx 
ON memory_access_log (memory_id, accessed_at DESC);

CREATE INDEX IF NOT EXISTS memory_access_log_access_type_idx 
ON memory_access_log (access_type);

CREATE INDEX IF NOT EXISTS memory_access_log_session_id_idx 
ON memory_access_log (session_id) WHERE session_id IS NOT NULL;

-- System metrics snapshots indexes
CREATE INDEX IF NOT EXISTS system_metrics_snapshots_type_recorded_at_idx 
ON system_metrics_snapshots (snapshot_type, recorded_at DESC);

-- Composite indexes for complex queries
CREATE INDEX IF NOT EXISTS harvest_patterns_session_status_type_idx 
ON harvest_patterns (harvest_session_id, status, pattern_type);

CREATE INDEX IF NOT EXISTS consolidation_events_memory_event_time_idx 
ON consolidation_events (memory_id, event_type, created_at DESC);

-- Partial indexes for performance optimization
CREATE INDEX IF NOT EXISTS harvest_sessions_active_idx 
ON harvest_sessions (started_at DESC) 
WHERE status IN ('in_progress', 'failed');

CREATE INDEX IF NOT EXISTS harvest_patterns_pending_idx 
ON harvest_patterns (extracted_at) 
WHERE status = 'extracted';

CREATE INDEX IF NOT EXISTS memory_access_log_recent_idx 
ON memory_access_log (memory_id, accessed_at DESC);

-- Create functions for automated data management

-- Function to clean up old harvest sessions (keep last 90 days)
CREATE OR REPLACE FUNCTION cleanup_old_harvest_sessions() RETURNS INTEGER AS $$
DECLARE
    deleted_count INTEGER;
BEGIN
    DELETE FROM harvest_sessions 
    WHERE completed_at < NOW() - INTERVAL '90 days' 
    AND status IN ('completed', 'failed', 'cancelled');
    
    GET DIAGNOSTICS deleted_count = ROW_COUNT;
    
    RETURN deleted_count;
END;
$$ LANGUAGE plpgsql;

-- Function to clean up old access logs (keep last 30 days for non-critical accesses)
CREATE OR REPLACE FUNCTION cleanup_old_access_logs() RETURNS INTEGER AS $$
DECLARE
    deleted_count INTEGER;
BEGIN
    DELETE FROM memory_access_log 
    WHERE accessed_at < NOW() - INTERVAL '30 days' 
    AND access_type NOT IN ('reflection_analysis', 'consolidation_process');
    
    GET DIAGNOSTICS deleted_count = ROW_COUNT;
    
    RETURN deleted_count;
END;
$$ LANGUAGE plpgsql;

-- Function to archive old system metrics snapshots
CREATE OR REPLACE FUNCTION archive_old_metrics_snapshots() RETURNS INTEGER AS $$
DECLARE
    deleted_count INTEGER;
BEGIN
    -- Keep daily snapshots for 1 year, hourly for 1 month, weekly forever
    DELETE FROM system_metrics_snapshots 
    WHERE (
        snapshot_type = 'hourly' AND recorded_at < NOW() - INTERVAL '1 month'
    ) OR (
        snapshot_type = 'daily' AND recorded_at < NOW() - INTERVAL '1 year'
    );
    
    GET DIAGNOSTICS deleted_count = ROW_COUNT;
    
    RETURN deleted_count;
END;
$$ LANGUAGE plpgsql;

-- Function to calculate harvest session success rate
CREATE OR REPLACE FUNCTION calculate_harvest_success_rate(
    p_days_back INTEGER DEFAULT 7
) RETURNS TABLE(
    total_sessions INTEGER,
    successful_sessions INTEGER,
    failed_sessions INTEGER,
    success_rate FLOAT,
    average_processing_time_ms FLOAT
) AS $$
BEGIN
    RETURN QUERY
    SELECT 
        COUNT(*)::INTEGER as total_sessions,
        COUNT(*) FILTER (WHERE status = 'completed')::INTEGER as successful_sessions,
        COUNT(*) FILTER (WHERE status = 'failed')::INTEGER as failed_sessions,
        (COUNT(*) FILTER (WHERE status = 'completed')::FLOAT / GREATEST(COUNT(*), 1)::FLOAT) as success_rate,
        AVG(processing_time_ms)::FLOAT as average_processing_time_ms
    FROM harvest_sessions 
    WHERE started_at > NOW() - (p_days_back || ' days')::INTERVAL;
END;
$$ LANGUAGE plpgsql;

-- Function to get memory tier migration statistics
CREATE OR REPLACE FUNCTION get_tier_migration_stats(
    p_days_back INTEGER DEFAULT 30
) RETURNS TABLE(
    source_tier VARCHAR(20),
    target_tier VARCHAR(20),
    migration_count INTEGER,
    avg_processing_time_ms FLOAT,
    success_rate FLOAT
) AS $$
BEGIN
    RETURN QUERY
    SELECT 
        ce.source_tier,
        ce.target_tier,
        COUNT(*)::INTEGER as migration_count,
        AVG(ce.processing_time_ms)::FLOAT as avg_processing_time_ms,
        -- Calculate success rate by checking if memory actually moved to target tier
        (COUNT(*) FILTER (WHERE m.tier::text = ce.target_tier)::FLOAT / COUNT(*)::FLOAT) as success_rate
    FROM consolidation_events ce
    JOIN memories m ON ce.memory_id = m.id
    WHERE ce.event_type = 'tier_migration'
    AND ce.created_at > NOW() - (p_days_back || ' days')::INTERVAL
    GROUP BY ce.source_tier, ce.target_tier
    ORDER BY migration_count DESC;
END;
$$ LANGUAGE plpgsql;

-- Function to get top performing harvest patterns
CREATE OR REPLACE FUNCTION get_top_harvest_patterns(
    p_limit INTEGER DEFAULT 10,
    p_days_back INTEGER DEFAULT 30
) RETURNS TABLE(
    pattern_type VARCHAR(50),
    total_extracted INTEGER,
    total_stored INTEGER,
    avg_confidence FLOAT,
    success_rate FLOAT
) AS $$
BEGIN
    RETURN QUERY
    SELECT 
        hp.pattern_type,
        COUNT(*)::INTEGER as total_extracted,
        COUNT(*) FILTER (WHERE hp.status = 'stored')::INTEGER as total_stored,
        AVG(hp.confidence_score)::FLOAT as avg_confidence,
        (COUNT(*) FILTER (WHERE hp.status = 'stored')::FLOAT / COUNT(*)::FLOAT) as success_rate
    FROM harvest_patterns hp
    WHERE hp.extracted_at > NOW() - (p_days_back || ' days')::INTERVAL
    GROUP BY hp.pattern_type
    ORDER BY success_rate DESC, total_stored DESC
    LIMIT p_limit;
END;
$$ LANGUAGE plpgsql;

-- Create triggers for automated maintenance

-- Trigger to update harvest session completion metrics
CREATE OR REPLACE FUNCTION update_harvest_session_completion() RETURNS TRIGGER AS $$
BEGIN
    IF OLD.status != NEW.status AND NEW.status IN ('completed', 'failed', 'cancelled') THEN
        NEW.completed_at = NOW();
        
        -- Calculate total processing time if not already set
        IF NEW.processing_time_ms IS NULL OR NEW.processing_time_ms = 0 THEN
            NEW.processing_time_ms = EXTRACT(EPOCH FROM (NEW.completed_at - NEW.started_at)) * 1000;
        END IF;
    END IF;
    
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER harvest_sessions_completion_trigger
    BEFORE UPDATE ON harvest_sessions
    FOR EACH ROW
    EXECUTE FUNCTION update_harvest_session_completion();

-- Trigger to automatically create access log entries when memories are accessed
CREATE OR REPLACE FUNCTION log_memory_access() RETURNS TRIGGER AS $$
BEGIN
    -- Only log when last_accessed_at changes (actual access, not just update)
    IF TG_OP = 'UPDATE' AND OLD.last_accessed_at IS DISTINCT FROM NEW.last_accessed_at THEN
        INSERT INTO memory_access_log (
            memory_id,
            access_type,
            importance_boost,
            access_count_increment
        ) VALUES (
            NEW.id,
            'direct_retrieval',
            NEW.importance_score - OLD.importance_score,
            NEW.access_count - OLD.access_count
        );
    END IF;
    
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER memory_access_logging_trigger
    AFTER UPDATE ON memories
    FOR EACH ROW
    EXECUTE FUNCTION log_memory_access();

-- Add constraints and validations
ALTER TABLE harvest_sessions 
ADD CONSTRAINT check_harvest_session_completion 
CHECK (
    (status IN ('completed', 'failed', 'cancelled') AND completed_at IS NOT NULL) OR
    (status = 'in_progress' AND completed_at IS NULL)
);

ALTER TABLE harvest_patterns 
ADD CONSTRAINT check_harvest_pattern_memory_link 
CHECK (
    (status = 'stored' AND memory_id IS NOT NULL) OR
    (status != 'stored')
);

ALTER TABLE consolidation_events 
ADD CONSTRAINT check_consolidation_strength_delta 
CHECK (
    (old_consolidation_strength IS NULL AND new_consolidation_strength IS NULL) OR
    (old_consolidation_strength IS NOT NULL AND new_consolidation_strength IS NOT NULL AND 
     strength_delta = new_consolidation_strength - old_consolidation_strength)
);

-- Insert initial system metrics snapshot
INSERT INTO system_metrics_snapshots (
    snapshot_type,
    working_memory_count,
    warm_memory_count,
    cold_memory_count,
    frozen_memory_count,
    total_storage_bytes
) 
SELECT 
    'on_demand' as snapshot_type,
    COUNT(*) FILTER (WHERE tier = 'working') as working_memory_count,
    COUNT(*) FILTER (WHERE tier = 'warm') as warm_memory_count,
    COUNT(*) FILTER (WHERE tier = 'cold') as cold_memory_count,
    0 as frozen_memory_count,
    SUM(LENGTH(content::text))::BIGINT as total_storage_bytes
FROM memories 
WHERE status = 'active';

-- Migration 008 completed successfully
-- Added harvest_sessions, harvest_patterns, consolidation_events, memory_access_log, and system_metrics_snapshots tables

COMMIT;