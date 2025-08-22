-- Migration 004: Database Schema for Semantic Deduplication System
-- Story 5: Implement comprehensive semantic deduplication with intelligent merging
-- Author: codex-memory system
-- Date: 2025-08-22

BEGIN;

-- Create audit trail table for all deduplication operations
CREATE TABLE IF NOT EXISTS deduplication_audit_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    operation_type VARCHAR(50) NOT NULL, -- 'merge', 'prune', 'compress', 'unmerge'
    operation_data JSONB NOT NULL, -- Details about the operation (memory IDs, strategy, etc.)
    completion_data JSONB, -- Results of the operation (compression ratios, storage saved, etc.)
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    completed_at TIMESTAMP WITH TIME ZONE,
    status VARCHAR(20) DEFAULT 'in_progress', -- 'in_progress', 'completed', 'failed', 'reversed'
    error_message TEXT,
    reversible_until TIMESTAMP WITH TIME ZONE, -- For 7-day reversibility requirement
    created_by VARCHAR(100) DEFAULT 'system'
);

-- Create table for tracking merged memory relationships
CREATE TABLE IF NOT EXISTS memory_merge_history (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    merged_memory_id UUID NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    original_memory_id UUID NOT NULL, -- Reference to original (now archived) memory
    merge_operation_id UUID NOT NULL REFERENCES deduplication_audit_log(id),
    similarity_score FLOAT NOT NULL CHECK (similarity_score >= 0.0 AND similarity_score <= 1.0),
    merge_strategy VARCHAR(50) NOT NULL, -- 'lossless', 'metadata_consolidation', 'content_summarization'
    weight_in_merge FLOAT DEFAULT 1.0, -- How much this memory contributed to the final merge
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Create table for compression tracking and reversibility
CREATE TABLE IF NOT EXISTS memory_compression_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    memory_id UUID NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    original_content TEXT NOT NULL, -- For lossless reversal
    original_metadata JSONB, -- For lossless reversal
    compression_type VARCHAR(50) NOT NULL, -- 'lossless', 'lossy'
    compression_ratio FLOAT NOT NULL,
    compression_algorithm VARCHAR(100),
    compressed_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    reversible_until TIMESTAMP WITH TIME ZONE,
    audit_log_id UUID REFERENCES deduplication_audit_log(id)
);

-- Create table for deduplication metrics and monitoring
CREATE TABLE IF NOT EXISTS deduplication_metrics (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    measurement_type VARCHAR(50) NOT NULL, -- 'daily_summary', 'operation_metrics', 'system_health'
    metrics_data JSONB NOT NULL,
    recorded_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    tier VARCHAR(20), -- Optional tier-specific metrics
    time_period_start TIMESTAMP WITH TIME ZONE,
    time_period_end TIMESTAMP WITH TIME ZONE
);

-- Create table for similarity cache (performance optimization)
CREATE TABLE IF NOT EXISTS memory_similarity_cache (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    memory_id_1 UUID NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    memory_id_2 UUID NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    similarity_score FLOAT NOT NULL CHECK (similarity_score >= 0.0 AND similarity_score <= 1.0),
    calculated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    algorithm_version VARCHAR(20) DEFAULT 'cosine_v1',
    expires_at TIMESTAMP WITH TIME ZONE DEFAULT (NOW() + INTERVAL '7 days'),
    
    -- Ensure consistent ordering and uniqueness
    UNIQUE(memory_id_1, memory_id_2),
    CHECK(memory_id_1 < memory_id_2) -- Enforce consistent ordering
);

-- Create table for pruning candidates and decisions
CREATE TABLE IF NOT EXISTS memory_pruning_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    memory_id UUID NOT NULL, -- May reference deleted memory
    recall_probability FLOAT NOT NULL,
    age_days INTEGER NOT NULL,
    tier VARCHAR(20) NOT NULL,
    importance_score FLOAT NOT NULL,
    access_count INTEGER NOT NULL,
    content_size_bytes INTEGER NOT NULL,
    pruning_reason TEXT NOT NULL,
    pruned_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    audit_log_id UUID REFERENCES deduplication_audit_log(id)
);

-- Create table for headroom monitoring and automatic triggers
CREATE TABLE IF NOT EXISTS memory_headroom_monitoring (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    total_memory_bytes BIGINT NOT NULL,
    used_memory_bytes BIGINT NOT NULL,
    free_memory_bytes BIGINT NOT NULL,
    utilization_percentage FLOAT NOT NULL,
    target_headroom_percentage FLOAT NOT NULL,
    triggered_action VARCHAR(100), -- 'deduplication', 'pruning', 'compression', 'none'
    action_result JSONB, -- Results of triggered action
    measured_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Add new columns to memories table for deduplication tracking
ALTER TABLE memories 
ADD COLUMN IF NOT EXISTS is_merged_result BOOLEAN DEFAULT FALSE,
ADD COLUMN IF NOT EXISTS original_memory_count INTEGER DEFAULT 1,
ADD COLUMN IF NOT EXISTS merge_generation INTEGER DEFAULT 0,
ADD COLUMN IF NOT EXISTS compression_applied BOOLEAN DEFAULT FALSE,
ADD COLUMN IF NOT EXISTS similarity_hash VARCHAR(64), -- For fast similarity grouping
ADD COLUMN IF NOT EXISTS deduplication_eligible BOOLEAN DEFAULT TRUE,
ADD COLUMN IF NOT EXISTS last_deduplication_check TIMESTAMP WITH TIME ZONE;

-- Create indexes for performance optimization

-- Audit log indexes
CREATE INDEX IF NOT EXISTS deduplication_audit_log_operation_type_idx 
ON deduplication_audit_log (operation_type);

CREATE INDEX IF NOT EXISTS deduplication_audit_log_created_at_idx 
ON deduplication_audit_log (created_at DESC);

CREATE INDEX IF NOT EXISTS deduplication_audit_log_status_idx 
ON deduplication_audit_log (status);

CREATE INDEX IF NOT EXISTS deduplication_audit_log_reversible_idx 
ON deduplication_audit_log (reversible_until) WHERE reversible_until IS NOT NULL;

-- Merge history indexes
CREATE INDEX IF NOT EXISTS memory_merge_history_merged_memory_idx 
ON memory_merge_history (merged_memory_id);

CREATE INDEX IF NOT EXISTS memory_merge_history_original_memory_idx 
ON memory_merge_history (original_memory_id);

CREATE INDEX IF NOT EXISTS memory_merge_history_operation_idx 
ON memory_merge_history (merge_operation_id);

CREATE INDEX IF NOT EXISTS memory_merge_history_similarity_idx 
ON memory_merge_history (similarity_score DESC);

-- Compression log indexes
CREATE INDEX IF NOT EXISTS memory_compression_log_memory_idx 
ON memory_compression_log (memory_id);

CREATE INDEX IF NOT EXISTS memory_compression_log_type_idx 
ON memory_compression_log (compression_type);

CREATE INDEX IF NOT EXISTS memory_compression_log_reversible_idx 
ON memory_compression_log (reversible_until) WHERE reversible_until IS NOT NULL;

-- Similarity cache indexes
CREATE INDEX IF NOT EXISTS memory_similarity_cache_memory1_idx 
ON memory_similarity_cache (memory_id_1);

CREATE INDEX IF NOT EXISTS memory_similarity_cache_memory2_idx 
ON memory_similarity_cache (memory_id_2);

CREATE INDEX IF NOT EXISTS memory_similarity_cache_score_idx 
ON memory_similarity_cache (similarity_score DESC);

CREATE INDEX IF NOT EXISTS memory_similarity_cache_expires_idx 
ON memory_similarity_cache (expires_at);

-- Metrics indexes
CREATE INDEX IF NOT EXISTS deduplication_metrics_type_recorded_idx 
ON deduplication_metrics (measurement_type, recorded_at DESC);

CREATE INDEX IF NOT EXISTS deduplication_metrics_tier_idx 
ON deduplication_metrics (tier) WHERE tier IS NOT NULL;

-- Pruning log indexes
CREATE INDEX IF NOT EXISTS memory_pruning_log_memory_idx 
ON memory_pruning_log (memory_id);

CREATE INDEX IF NOT EXISTS memory_pruning_log_pruned_at_idx 
ON memory_pruning_log (pruned_at DESC);

CREATE INDEX IF NOT EXISTS memory_pruning_log_recall_prob_idx 
ON memory_pruning_log (recall_probability);

-- Memory table new column indexes
CREATE INDEX IF NOT EXISTS memories_is_merged_result_idx 
ON memories (is_merged_result) WHERE is_merged_result = TRUE;

CREATE INDEX IF NOT EXISTS memories_similarity_hash_idx 
ON memories (similarity_hash) WHERE similarity_hash IS NOT NULL;

CREATE INDEX IF NOT EXISTS memories_deduplication_eligible_idx 
ON memories (deduplication_eligible) WHERE deduplication_eligible = TRUE;

CREATE INDEX IF NOT EXISTS memories_last_dedup_check_idx 
ON memories (last_deduplication_check);

-- Composite indexes for efficient deduplication queries
CREATE INDEX IF NOT EXISTS memories_dedup_candidates_idx 
ON memories (tier, deduplication_eligible, last_deduplication_check, created_at)
WHERE status = 'active' AND deduplication_eligible = TRUE;

CREATE INDEX IF NOT EXISTS memories_similarity_lookup_idx 
ON memories (similarity_hash, tier, status) 
WHERE similarity_hash IS NOT NULL AND status = 'active';

-- Create function for calculating content-based similarity hash
CREATE OR REPLACE FUNCTION calculate_similarity_hash(content TEXT) RETURNS VARCHAR(64) AS $$
BEGIN
    -- Simple hash based on content words (normalized)
    -- This creates groups of potentially similar content for faster similarity checking
    RETURN substring(
        md5(
            array_to_string(
                string_to_array(
                    lower(regexp_replace(content, '[^a-zA-Z0-9\s]', '', 'g')),
                    ' '
                ),
                ' '
            )
        ),
        1, 16
    );
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Create function for cleaning up expired similarity cache entries
CREATE OR REPLACE FUNCTION cleanup_similarity_cache() RETURNS INTEGER AS $$
DECLARE
    deleted_count INTEGER;
BEGIN
    DELETE FROM memory_similarity_cache WHERE expires_at < NOW();
    GET DIAGNOSTICS deleted_count = ROW_COUNT;
    RETURN deleted_count;
END;
$$ LANGUAGE plpgsql;

-- Create function for automatic reversibility expiration cleanup
CREATE OR REPLACE FUNCTION cleanup_expired_reversible_operations() RETURNS INTEGER AS $$
DECLARE
    deleted_audit_count INTEGER;
    deleted_compression_count INTEGER;
    total_deleted INTEGER;
BEGIN
    -- Update audit log entries past reversibility period
    UPDATE deduplication_audit_log 
    SET reversible_until = NULL, status = 'completed_irreversible'
    WHERE reversible_until < NOW() AND status = 'completed';
    
    GET DIAGNOSTICS deleted_audit_count = ROW_COUNT;
    
    -- Clean up compression log entries past reversibility period
    DELETE FROM memory_compression_log 
    WHERE reversible_until < NOW();
    
    GET DIAGNOSTICS deleted_compression_count = ROW_COUNT;
    
    total_deleted := deleted_audit_count + deleted_compression_count;
    
    RETURN total_deleted;
END;
$$ LANGUAGE plpgsql;

-- Create function for recording deduplication metrics
CREATE OR REPLACE FUNCTION record_deduplication_metrics(
    p_measurement_type VARCHAR(50),
    p_metrics_data JSONB,
    p_tier VARCHAR(20) DEFAULT NULL,
    p_time_period_start TIMESTAMP WITH TIME ZONE DEFAULT NULL,
    p_time_period_end TIMESTAMP WITH TIME ZONE DEFAULT NULL
) RETURNS UUID AS $$
DECLARE
    metric_id UUID;
BEGIN
    INSERT INTO deduplication_metrics (
        measurement_type,
        metrics_data,
        tier,
        time_period_start,
        time_period_end
    ) VALUES (
        p_measurement_type,
        p_metrics_data,
        p_tier,
        p_time_period_start,
        p_time_period_end
    ) RETURNING id INTO metric_id;
    
    RETURN metric_id;
END;
$$ LANGUAGE plpgsql;

-- Create function for getting deduplication candidates
CREATE OR REPLACE FUNCTION get_deduplication_candidates(
    p_tier VARCHAR(20) DEFAULT NULL,
    p_limit INTEGER DEFAULT 1000,
    p_min_age_hours INTEGER DEFAULT 1
) RETURNS TABLE (
    memory_id UUID,
    content_size INTEGER,
    similarity_hash VARCHAR(64),
    importance_score FLOAT,
    last_check TIMESTAMP WITH TIME ZONE
) AS $$
BEGIN
    RETURN QUERY
    SELECT 
        m.id,
        LENGTH(m.content),
        m.similarity_hash,
        m.importance_score,
        m.last_deduplication_check
    FROM memories m
    WHERE m.status = 'active'
    AND m.deduplication_eligible = TRUE
    AND m.embedding IS NOT NULL
    AND m.created_at < (NOW() - (p_min_age_hours || ' hours')::INTERVAL)
    AND (p_tier IS NULL OR m.tier = p_tier::memory_tier)
    AND (m.last_deduplication_check IS NULL OR 
         m.last_deduplication_check < (NOW() - INTERVAL '24 hours'))
    ORDER BY 
        m.last_deduplication_check NULLS FIRST,
        m.importance_score DESC,
        m.created_at ASC
    LIMIT p_limit;
END;
$$ LANGUAGE plpgsql;

-- Update existing memories with similarity hashes
UPDATE memories 
SET similarity_hash = calculate_similarity_hash(content),
    last_deduplication_check = NULL
WHERE status = 'active' AND embedding IS NOT NULL;

-- Create automated cleanup job schedule (to be run daily)
-- Note: This would typically be scheduled via cron or a job scheduler
-- CREATE EXTENSION IF NOT EXISTS pg_cron;
-- SELECT cron.schedule('cleanup-deduplication-cache', '0 2 * * *', 'SELECT cleanup_similarity_cache();');
-- SELECT cron.schedule('cleanup-reversible-ops', '0 3 * * *', 'SELECT cleanup_expired_reversible_operations();');

-- Insert initial deduplication metrics baseline
INSERT INTO deduplication_metrics (measurement_type, metrics_data)
SELECT 
    'baseline_before_deduplication',
    jsonb_build_object(
        'total_memories', COUNT(*),
        'total_content_bytes', SUM(LENGTH(content)),
        'memories_by_tier', jsonb_object_agg(tier, tier_count),
        'avg_importance_by_tier', jsonb_object_agg(tier || '_avg_importance', avg_importance),
        'measured_at', NOW()
    )
FROM (
    SELECT 
        tier,
        COUNT(*) as tier_count,
        AVG(importance_score) as avg_importance
    FROM memories 
    WHERE status = 'active'
    GROUP BY tier
) tier_stats;

-- Add constraints for data integrity
ALTER TABLE memory_merge_history 
ADD CONSTRAINT check_similarity_score 
CHECK (similarity_score >= 0.0 AND similarity_score <= 1.0);

ALTER TABLE memory_compression_log 
ADD CONSTRAINT check_compression_ratio 
CHECK (compression_ratio > 0.0);

ALTER TABLE memory_headroom_monitoring 
ADD CONSTRAINT check_utilization_percentage 
CHECK (utilization_percentage >= 0.0 AND utilization_percentage <= 100.0);

ALTER TABLE deduplication_audit_log 
ADD CONSTRAINT check_status_values 
CHECK (status IN ('in_progress', 'completed', 'failed', 'reversed', 'completed_irreversible'));

-- Set reversibility period for audit entries (7 days as per requirement)
CREATE OR REPLACE FUNCTION set_reversibility_period() RETURNS TRIGGER AS $$
BEGIN
    IF NEW.status = 'completed' AND NEW.operation_type IN ('merge', 'compress') THEN
        NEW.reversible_until := NEW.completed_at + INTERVAL '7 days';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER set_audit_reversibility_trigger
    BEFORE UPDATE ON deduplication_audit_log
    FOR EACH ROW
    WHEN (OLD.status != NEW.status AND NEW.status = 'completed')
    EXECUTE FUNCTION set_reversibility_period();

-- Record migration completion
INSERT INTO migration_history (migration_name, success, migration_reason)
VALUES (
    '004_semantic_deduplication_schema',
    true,
    'Added comprehensive semantic deduplication system with audit trails, reversibility, and intelligent merging support'
);

COMMIT;