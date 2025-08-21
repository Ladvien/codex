-- Migration 002: Consolidation Schema for Memory Evolution
-- Purpose: Add consolidation, decay tracking, and frozen storage capabilities
-- Based on human memory consolidation research and forgetting curve mathematics

-- Update memory_tier enum to include frozen tier
ALTER TYPE memory_tier ADD VALUE 'frozen';

-- Add consolidation fields to existing memories table
ALTER TABLE memories 
ADD COLUMN IF NOT EXISTS consolidation_strength FLOAT NOT NULL DEFAULT 1.0 
    CHECK (consolidation_strength >= 0.0 AND consolidation_strength <= 10.0),
ADD COLUMN IF NOT EXISTS decay_rate FLOAT NOT NULL DEFAULT 1.0 
    CHECK (decay_rate >= 0.0 AND decay_rate <= 5.0),
ADD COLUMN IF NOT EXISTS recall_probability FLOAT 
    CHECK (recall_probability >= 0.0 AND recall_probability <= 1.0),
ADD COLUMN IF NOT EXISTS last_recall_interval INTERVAL;

-- Create memory consolidation log table for tracking consolidation events
CREATE TABLE IF NOT EXISTS memory_consolidation_log (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    memory_id UUID NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    event_type VARCHAR(50) NOT NULL CHECK (event_type IN ('access', 'consolidation', 'decay', 'recall')),
    previous_consolidation_strength FLOAT NOT NULL,
    new_consolidation_strength FLOAT NOT NULL,
    previous_recall_probability FLOAT,
    new_recall_probability FLOAT,
    recall_interval INTERVAL,
    access_context JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Ensure logical progression
    CHECK (new_consolidation_strength >= 0.0 AND new_consolidation_strength <= 10.0),
    CHECK (previous_consolidation_strength >= 0.0 AND previous_consolidation_strength <= 10.0),
    CHECK (new_recall_probability IS NULL OR (new_recall_probability >= 0.0 AND new_recall_probability <= 1.0)),
    CHECK (previous_recall_probability IS NULL OR (previous_recall_probability >= 0.0 AND previous_recall_probability <= 1.0))
);

-- Create frozen memories archive table with compression
CREATE TABLE IF NOT EXISTS frozen_memories (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    original_memory_id UUID NOT NULL, -- Reference to original memory (may be deleted)
    compressed_content BYTEA NOT NULL, -- zstd compressed content
    compressed_metadata BYTEA, -- zstd compressed metadata
    embedding_summary vector(128), -- Reduced dimension embedding for search
    original_tier memory_tier NOT NULL,
    frozen_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_access_before_freeze TIMESTAMPTZ,
    access_count_before_freeze INTEGER NOT NULL DEFAULT 0,
    final_consolidation_strength FLOAT NOT NULL,
    final_recall_probability FLOAT,
    compression_ratio FLOAT, -- For monitoring compression effectiveness
    retrieval_difficulty_seconds INTEGER NOT NULL DEFAULT 3 CHECK (retrieval_difficulty_seconds BETWEEN 2 AND 5),
    freeze_reason VARCHAR(255),
    parent_relationships JSONB DEFAULT '[]', -- Store parent/child relationships
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Constraints
    UNIQUE(original_memory_id), -- Each memory can only be frozen once
    CHECK (compression_ratio IS NULL OR compression_ratio > 1.0)
);

-- Create memory tier statistics table for analytics
CREATE TABLE IF NOT EXISTS memory_tier_statistics (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    tier memory_tier NOT NULL,
    total_memories BIGINT NOT NULL DEFAULT 0,
    average_consolidation_strength FLOAT,
    average_recall_probability FLOAT,
    average_age_days FLOAT,
    total_storage_bytes BIGINT NOT NULL DEFAULT 0,
    snapshot_timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    UNIQUE(tier, snapshot_timestamp)
);

-- Add performance indexes for consolidation queries

-- Indexes for consolidation strength queries
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_memories_consolidation_strength 
    ON memories (consolidation_strength DESC, tier, status) 
    WHERE status = 'active';

-- Indexes for recall probability queries  
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_memories_recall_probability 
    ON memories (recall_probability ASC NULLS LAST, tier, status) 
    WHERE status = 'active' AND recall_probability IS NOT NULL;

-- Indexes for decay rate analysis
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_memories_decay_rate 
    ON memories (decay_rate, last_accessed_at DESC NULLS LAST) 
    WHERE status = 'active';

-- Index for finding memories ready for migration
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_memories_migration_candidates 
    ON memories (tier, recall_probability ASC NULLS LAST, consolidation_strength ASC) 
    WHERE status = 'active';

-- Consolidation log indexes
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_consolidation_log_memory 
    ON memory_consolidation_log (memory_id, created_at DESC);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_consolidation_log_event_type 
    ON memory_consolidation_log (event_type, created_at DESC);

-- Frozen memories indexes
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_frozen_memories_embedding 
    ON frozen_memories USING hnsw (embedding_summary vector_cosine_ops);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_frozen_memories_original_id 
    ON frozen_memories (original_memory_id);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_frozen_memories_frozen_at 
    ON frozen_memories (frozen_at DESC);

-- Tier statistics index
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_tier_statistics_snapshot 
    ON memory_tier_statistics (snapshot_timestamp DESC, tier);

-- Create functions for consolidation mathematics

-- Function to calculate recall probability using forgetting curve
-- Based on: p(t) = [1 - exp(-r*e^(-t/gn))] / (1 - e^-1)
-- Where: t = time since last access, r = decay rate, gn = consolidation strength
CREATE OR REPLACE FUNCTION calculate_recall_probability(
    last_access TIMESTAMPTZ,
    consolidation_strength FLOAT,
    decay_rate FLOAT
) RETURNS FLOAT AS $$
DECLARE
    time_hours FLOAT;
    t_normalized FLOAT;
    exponent FLOAT;
    probability FLOAT;
BEGIN
    -- Handle edge cases
    IF last_access IS NULL THEN
        RETURN NULL;
    END IF;
    
    IF consolidation_strength <= 0 OR decay_rate <= 0 THEN
        RETURN 0.0;
    END IF;
    
    -- Calculate time since last access in hours
    time_hours := EXTRACT(EPOCH FROM (NOW() - last_access)) / 3600.0;
    
    -- Normalize time by consolidation strength
    t_normalized := time_hours / GREATEST(consolidation_strength, 0.1);
    
    -- Calculate forgetting curve: p(t) = [1 - exp(-r*e^(-t/gn))] / (1 - e^-1)
    exponent := -decay_rate * exp(-t_normalized);
    probability := (1.0 - exp(exponent)) / (1.0 - exp(-1.0));
    
    -- Ensure probability is within bounds
    RETURN GREATEST(0.0, LEAST(1.0, probability));
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Function to update consolidation strength on access
-- Based on: gn = gn-1 + (1 - e^-t)/(1 + e^-t) 
CREATE OR REPLACE FUNCTION update_consolidation_strength(
    current_strength FLOAT,
    time_since_last_access INTERVAL
) RETURNS FLOAT AS $$
DECLARE
    time_hours FLOAT;
    strength_increment FLOAT;
    new_strength FLOAT;
BEGIN
    -- Handle edge cases
    IF time_since_last_access IS NULL THEN
        RETURN GREATEST(current_strength, 1.0);
    END IF;
    
    -- Convert interval to hours
    time_hours := EXTRACT(EPOCH FROM time_since_last_access) / 3600.0;
    
    -- Calculate strength increment: (1 - e^-t)/(1 + e^-t)
    strength_increment := (1.0 - exp(-time_hours)) / (1.0 + exp(-time_hours));
    
    -- Update consolidation strength
    new_strength := current_strength + strength_increment;
    
    -- Cap at maximum value of 10.0
    RETURN LEAST(10.0, new_strength);
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Trigger function to automatically update recall probability on memory access
CREATE OR REPLACE FUNCTION auto_update_memory_consolidation()
RETURNS TRIGGER AS $$
DECLARE
    time_since_access INTERVAL;
    new_consolidation FLOAT;
    new_recall_prob FLOAT;
BEGIN
    -- Only update on actual memory retrieval (when last_accessed_at changes)
    IF TG_OP = 'UPDATE' AND OLD.last_accessed_at IS DISTINCT FROM NEW.last_accessed_at THEN
        
        -- Calculate time since last access
        time_since_access := COALESCE(NEW.last_accessed_at - OLD.last_accessed_at, INTERVAL '0');
        
        -- Update consolidation strength
        new_consolidation := update_consolidation_strength(OLD.consolidation_strength, time_since_access);
        NEW.consolidation_strength := new_consolidation;
        
        -- Calculate new recall probability
        new_recall_prob := calculate_recall_probability(
            NEW.last_accessed_at, 
            new_consolidation, 
            NEW.decay_rate
        );
        NEW.recall_probability := new_recall_prob;
        
        -- Store recall interval
        NEW.last_recall_interval := time_since_access;
        
        -- Log the consolidation event
        INSERT INTO memory_consolidation_log (
            memory_id,
            event_type,
            previous_consolidation_strength,
            new_consolidation_strength,
            previous_recall_probability,
            new_recall_probability,
            recall_interval,
            access_context
        ) VALUES (
            NEW.id,
            'access',
            OLD.consolidation_strength,
            new_consolidation,
            OLD.recall_probability,
            new_recall_prob,
            time_since_access,
            jsonb_build_object(
                'access_count', NEW.access_count,
                'tier', NEW.tier::text,
                'importance_score', NEW.importance_score
            )
        );
    END IF;
    
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Create trigger for automatic consolidation updates
CREATE TRIGGER update_memory_consolidation_trigger
    BEFORE UPDATE ON memories
    FOR EACH ROW
    EXECUTE FUNCTION auto_update_memory_consolidation();

-- Function to freeze a memory and compress its content
CREATE OR REPLACE FUNCTION freeze_memory(memory_uuid UUID) 
RETURNS UUID AS $$
DECLARE
    memory_record memories%ROWTYPE;
    frozen_id UUID;
    compressed_content BYTEA;
    compressed_meta BYTEA;
    compression_ratio_val FLOAT;
BEGIN
    -- Get the memory record
    SELECT * INTO memory_record FROM memories WHERE id = memory_uuid AND status = 'active';
    
    IF NOT FOUND THEN
        RAISE EXCEPTION 'Memory % not found or not active', memory_uuid;
    END IF;
    
    -- Don't freeze memories that are already frozen
    IF memory_record.tier = 'frozen' THEN
        RAISE EXCEPTION 'Memory % is already frozen', memory_uuid;
    END IF;
    
    -- Simple compression simulation (in production, use actual compression)
    -- For now, we'll just store the content as-is but track compression ratio
    compressed_content := memory_record.content::bytea;
    compressed_meta := memory_record.metadata::text::bytea;
    compression_ratio_val := length(memory_record.content)::float / length(compressed_content::text);
    
    -- Create frozen memory record
    INSERT INTO frozen_memories (
        original_memory_id,
        compressed_content,
        compressed_metadata,
        embedding_summary,
        original_tier,
        last_access_before_freeze,
        access_count_before_freeze,
        final_consolidation_strength,
        final_recall_probability,
        compression_ratio,
        freeze_reason
    ) VALUES (
        memory_record.id,
        compressed_content,
        compressed_meta,
        -- Create reduced-dimension embedding (simplified - in production use proper dimensionality reduction)
        CASE 
            WHEN memory_record.embedding IS NOT NULL THEN 
                (SELECT array_to_vector(array_agg(val)::float[]) 
                 FROM (SELECT unnest(memory_record.embedding::float[]) as val LIMIT 128) sub)
            ELSE NULL 
        END,
        memory_record.tier,
        memory_record.last_accessed_at,
        memory_record.access_count,
        memory_record.consolidation_strength,
        memory_record.recall_probability,
        compression_ratio_val,
        'automatic_freezing'
    ) RETURNING id INTO frozen_id;
    
    -- Update original memory to frozen status
    UPDATE memories 
    SET 
        tier = 'frozen',
        status = 'archived',
        updated_at = NOW()
    WHERE id = memory_uuid;
    
    -- Log the freeze event
    INSERT INTO memory_consolidation_log (
        memory_id,
        event_type,
        previous_consolidation_strength,
        new_consolidation_strength,
        previous_recall_probability,
        new_recall_probability,
        access_context
    ) VALUES (
        memory_uuid,
        'freeze',
        memory_record.consolidation_strength,
        memory_record.consolidation_strength,
        memory_record.recall_probability,
        memory_record.recall_probability,
        jsonb_build_object(
            'frozen_id', frozen_id,
            'compression_ratio', compression_ratio_val,
            'original_tier', memory_record.tier::text
        )
    );
    
    RETURN frozen_id;
END;
$$ LANGUAGE plpgsql;

-- Function to unfreeze a memory (with intentional delay)
CREATE OR REPLACE FUNCTION unfreeze_memory(frozen_uuid UUID)
RETURNS UUID AS $$
DECLARE
    frozen_record frozen_memories%ROWTYPE;
    memory_id UUID;
    retrieval_delay INTEGER;
BEGIN
    -- Get frozen memory record
    SELECT * INTO frozen_record FROM frozen_memories WHERE id = frozen_uuid;
    
    IF NOT FOUND THEN
        RAISE EXCEPTION 'Frozen memory % not found', frozen_uuid;
    END IF;
    
    -- Simulate cognitive effort delay (2-5 seconds)
    retrieval_delay := frozen_record.retrieval_difficulty_seconds;
    PERFORM pg_sleep(retrieval_delay);
    
    -- Restore memory to working tier
    INSERT INTO memories (
        id,
        content,
        content_hash,
        embedding,
        tier,
        status,
        importance_score,
        access_count,
        last_accessed_at,
        metadata,
        consolidation_strength,
        decay_rate,
        recall_probability,
        created_at,
        updated_at
    ) VALUES (
        frozen_record.original_memory_id,
        convert_from(frozen_record.compressed_content, 'UTF8'),
        encode(digest(convert_from(frozen_record.compressed_content, 'UTF8'), 'sha256'), 'hex'),
        NULL, -- Embedding will need to be regenerated
        'working',
        'active',
        0.3, -- Lower importance after unfreezing
        frozen_record.access_count_before_freeze + 1,
        NOW(),
        convert_from(frozen_record.compressed_metadata, 'UTF8')::jsonb,
        GREATEST(frozen_record.final_consolidation_strength * 0.8, 1.0), -- Slight degradation
        1.0,
        NULL, -- Will be recalculated on first access
        frozen_record.created_at,
        NOW()
    ) RETURNING id INTO memory_id;
    
    -- Log the unfreeze event
    INSERT INTO memory_consolidation_log (
        memory_id,
        event_type,
        previous_consolidation_strength,
        new_consolidation_strength,
        access_context
    ) VALUES (
        memory_id,
        'unfreeze',
        frozen_record.final_consolidation_strength,
        GREATEST(frozen_record.final_consolidation_strength * 0.8, 1.0),
        jsonb_build_object(
            'retrieval_delay_seconds', retrieval_delay,
            'frozen_id', frozen_uuid,
            'was_frozen_at', frozen_record.frozen_at
        )
    );
    
    RETURN memory_id;
END;
$$ LANGUAGE plpgsql;

-- Create view for memory consolidation analytics
CREATE OR REPLACE VIEW memory_consolidation_analytics AS
SELECT 
    tier,
    COUNT(*) as total_memories,
    AVG(consolidation_strength) as avg_consolidation_strength,
    AVG(recall_probability) as avg_recall_probability,
    AVG(decay_rate) as avg_decay_rate,
    AVG(EXTRACT(EPOCH FROM (NOW() - created_at)) / 86400) as avg_age_days,
    COUNT(*) FILTER (WHERE recall_probability < 0.3) as migration_candidates,
    COUNT(*) FILTER (WHERE last_accessed_at IS NULL) as never_accessed,
    COUNT(*) FILTER (WHERE last_accessed_at > NOW() - INTERVAL '24 hours') as accessed_recently
FROM memories 
WHERE status = 'active' 
GROUP BY tier
ORDER BY 
    CASE tier 
        WHEN 'working' THEN 1 
        WHEN 'warm' THEN 2 
        WHEN 'cold' THEN 3 
        WHEN 'frozen' THEN 4 
    END;

-- Create view for consolidation event analysis
CREATE OR REPLACE VIEW consolidation_event_summary AS
SELECT 
    event_type,
    COUNT(*) as event_count,
    AVG(new_consolidation_strength - previous_consolidation_strength) as avg_strength_change,
    AVG(COALESCE(new_recall_probability, 0) - COALESCE(previous_recall_probability, 0)) as avg_probability_change,
    AVG(EXTRACT(EPOCH FROM recall_interval) / 3600) as avg_recall_interval_hours
FROM memory_consolidation_log 
WHERE created_at > NOW() - INTERVAL '7 days'
GROUP BY event_type
ORDER BY event_count DESC;

-- Update tier statistics (to be run periodically)
CREATE OR REPLACE FUNCTION update_tier_statistics() RETURNS VOID AS $$
DECLARE
    tier_rec RECORD;
BEGIN
    -- Delete old statistics (keep last 30 days)
    DELETE FROM memory_tier_statistics 
    WHERE snapshot_timestamp < NOW() - INTERVAL '30 days';
    
    -- Calculate current statistics for each tier
    FOR tier_rec IN 
        SELECT 
            t.tier,
            COALESCE(m.total_memories, 0) as total_memories,
            m.avg_consolidation_strength,
            m.avg_recall_probability,
            m.avg_age_days,
            COALESCE(m.total_storage_bytes, 0) as total_storage_bytes
        FROM (VALUES ('working'::memory_tier), ('warm'::memory_tier), ('cold'::memory_tier), ('frozen'::memory_tier)) as t(tier)
        LEFT JOIN (
            SELECT 
                tier,
                COUNT(*) as total_memories,
                AVG(consolidation_strength) as avg_consolidation_strength,
                AVG(recall_probability) as avg_recall_probability,
                AVG(EXTRACT(EPOCH FROM (NOW() - created_at)) / 86400) as avg_age_days,
                SUM(length(content))::bigint as total_storage_bytes
            FROM memories 
            WHERE status = 'active'
            GROUP BY tier
        ) m ON t.tier = m.tier
    LOOP
        INSERT INTO memory_tier_statistics (
            tier,
            total_memories,
            average_consolidation_strength,
            average_recall_probability,
            average_age_days,
            total_storage_bytes
        ) VALUES (
            tier_rec.tier,
            tier_rec.total_memories,
            tier_rec.avg_consolidation_strength,
            tier_rec.avg_recall_probability,
            tier_rec.avg_age_days,
            tier_rec.total_storage_bytes
        );
    END LOOP;
END;
$$ LANGUAGE plpgsql;

-- Initial statistics update
SELECT update_tier_statistics();

-- Comments for operational guidance
COMMENT ON TABLE memory_consolidation_log IS 'Tracks memory access patterns and consolidation strength changes for analytics and debugging';
COMMENT ON TABLE frozen_memories IS 'Compressed archive storage for rarely accessed memories with intentional retrieval delay';
COMMENT ON FUNCTION calculate_recall_probability IS 'Implements forgetting curve mathematics for memory decay modeling';
COMMENT ON FUNCTION freeze_memory IS 'Compresses and archives a memory to frozen storage with compression tracking';
COMMENT ON FUNCTION unfreeze_memory IS 'Restores a frozen memory with simulated cognitive effort delay (2-5 seconds)';