-- Migration 002: Database Schema Evolution for Consolidation
-- Story 1: Add memory consolidation, decay tracking, and frozen storage support
-- Author: codex-memory system
-- Date: 2025-08-21

BEGIN;

-- Add new columns to memories table for consolidation tracking
ALTER TABLE memories 
ADD COLUMN IF NOT EXISTS consolidation_strength FLOAT DEFAULT 1.0,
ADD COLUMN IF NOT EXISTS decay_rate FLOAT DEFAULT 1.0,
ADD COLUMN IF NOT EXISTS recall_probability FLOAT,
ADD COLUMN IF NOT EXISTS last_recall_interval INTERVAL;

-- Update existing memories with default consolidation values
UPDATE memories 
SET consolidation_strength = 1.0,
    decay_rate = 1.0,
    recall_probability = 0.8,
    last_recall_interval = INTERVAL '0 seconds'
WHERE consolidation_strength IS NULL;

-- Create memory_consolidation_log table for tracking consolidation events
CREATE TABLE IF NOT EXISTS memory_consolidation_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    memory_id UUID NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    old_consolidation_strength FLOAT NOT NULL,
    new_consolidation_strength FLOAT NOT NULL,
    old_recall_probability FLOAT,
    new_recall_probability FLOAT,
    consolidation_event VARCHAR(50) NOT NULL, -- 'access', 'decay', 'manual'
    trigger_reason TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Create frozen_memories archive table with compressed JSONB storage
CREATE TABLE IF NOT EXISTS frozen_memories (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    original_memory_id UUID NOT NULL UNIQUE, -- Reference to original memory
    compressed_content JSONB NOT NULL, -- Compressed representation of memory data
    original_metadata JSONB DEFAULT '{}',
    freeze_reason VARCHAR(100),
    frozen_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    unfreeze_count INTEGER DEFAULT 0,
    last_unfrozen_at TIMESTAMP WITH TIME ZONE,
    compression_ratio FLOAT -- For monitoring storage efficiency
);

-- Create memory_tier_statistics table for system monitoring
CREATE TABLE IF NOT EXISTS memory_tier_statistics (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tier VARCHAR(20) NOT NULL,
    memory_count INTEGER NOT NULL,
    avg_consolidation_strength FLOAT,
    avg_recall_probability FLOAT,
    avg_access_count FLOAT,
    total_storage_bytes BIGINT,
    recorded_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Add indexes for consolidation_strength and recall_probability queries
-- Create indexes for frozen_memories table
CREATE INDEX IF NOT EXISTS frozen_memories_original_memory_id_idx 
ON frozen_memories (original_memory_id);

CREATE INDEX IF NOT EXISTS frozen_memories_frozen_at_idx 
ON frozen_memories (frozen_at DESC);

CREATE INDEX IF NOT EXISTS frozen_memories_freeze_reason_idx 
ON frozen_memories (freeze_reason);

-- Create indexes for memory_tier_statistics table
CREATE INDEX IF NOT EXISTS memory_tier_statistics_tier_idx 
ON memory_tier_statistics (tier);

CREATE INDEX IF NOT EXISTS memory_tier_statistics_recorded_at_idx 
ON memory_tier_statistics (recorded_at DESC);

-- Create indexes for memories table consolidation columns
CREATE INDEX IF NOT EXISTS memories_consolidation_strength_idx 
ON memories (consolidation_strength DESC);

CREATE INDEX IF NOT EXISTS memories_recall_probability_idx 
ON memories (recall_probability DESC);

CREATE INDEX IF NOT EXISTS memories_decay_rate_idx 
ON memories (decay_rate);

CREATE INDEX IF NOT EXISTS memories_last_recall_interval_idx 
ON memories (last_recall_interval);

-- Composite indexes for efficient tier migration queries
CREATE INDEX IF NOT EXISTS memories_tier_recall_prob_idx 
ON memories (tier, recall_probability DESC) 
WHERE status = 'active';

CREATE INDEX IF NOT EXISTS memories_consolidation_access_idx 
ON memories (consolidation_strength DESC, access_count DESC, last_accessed_at DESC);

-- Add constraints for data validation
ALTER TABLE memories 
ADD CONSTRAINT check_consolidation_strength 
CHECK (consolidation_strength >= 0.0 AND consolidation_strength <= 10.0);

ALTER TABLE memories 
ADD CONSTRAINT check_decay_rate 
CHECK (decay_rate >= 0.0 AND decay_rate <= 5.0);

ALTER TABLE memories 
ADD CONSTRAINT check_recall_probability 
CHECK (recall_probability >= 0.0 AND recall_probability <= 1.0);

-- Create database function for calculating recall probability using forgetting curve
CREATE OR REPLACE FUNCTION calculate_recall_probability(
    p_consolidation_strength FLOAT,
    p_decay_rate FLOAT,
    p_time_since_access INTERVAL
) RETURNS FLOAT AS $$
DECLARE
    t FLOAT;
    gn FLOAT;
    recall_prob FLOAT;
BEGIN
    -- Convert interval to hours for calculation
    t := EXTRACT(EPOCH FROM p_time_since_access) / 3600.0;
    gn := p_consolidation_strength;
    
    -- Apply forgetting curve formula: p(t) = [1 - exp(-r*e^(-t/gn))] / (1 - e^-1)
    -- Simplified version for initial implementation
    IF t = 0 THEN
        recall_prob := 1.0;
    ELSE
        recall_prob := GREATEST(0.0, LEAST(1.0, 
            (1.0 - EXP(-p_decay_rate * EXP(-t/gn))) / (1.0 - EXP(-1.0))
        ));
    END IF;
    
    RETURN recall_prob;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Create function for updating consolidation strength
CREATE OR REPLACE FUNCTION update_consolidation_strength(
    p_current_strength FLOAT,
    p_time_since_last_access INTERVAL
) RETURNS FLOAT AS $$
DECLARE
    t FLOAT;
    new_strength FLOAT;
BEGIN
    -- Convert interval to hours
    t := EXTRACT(EPOCH FROM p_time_since_last_access) / 3600.0;
    
    -- Apply consolidation formula: gn = gn-1 + (1 - e^-t)/(1 + e^-t)
    new_strength := p_current_strength + (1.0 - EXP(-t))/(1.0 + EXP(-t));
    
    -- Cap consolidation strength at reasonable maximum
    RETURN LEAST(10.0, new_strength);
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Create trigger function for automatic consolidation updates
CREATE OR REPLACE FUNCTION trigger_consolidation_update() RETURNS TRIGGER AS $$
DECLARE
    time_diff INTERVAL;
    new_consolidation FLOAT;
    new_recall_prob FLOAT;
BEGIN
    -- Only trigger on access updates (when last_accessed_at changes)
    IF TG_OP = 'UPDATE' AND OLD.last_accessed_at != NEW.last_accessed_at THEN
        
        -- Calculate time since last access
        time_diff := NEW.last_accessed_at - OLD.last_accessed_at;
        
        -- Update consolidation strength
        new_consolidation := update_consolidation_strength(
            COALESCE(OLD.consolidation_strength, 1.0), 
            time_diff
        );
        
        -- Calculate new recall probability
        new_recall_prob := calculate_recall_probability(
            new_consolidation,
            COALESCE(NEW.decay_rate, 1.0),
            INTERVAL '0 seconds' -- Just accessed, so immediate recall
        );
        
        -- Update the new record
        NEW.consolidation_strength := new_consolidation;
        NEW.recall_probability := new_recall_prob;
        NEW.last_recall_interval := time_diff;
        
        -- Log the consolidation event
        INSERT INTO memory_consolidation_log (
            memory_id,
            old_consolidation_strength,
            new_consolidation_strength,
            old_recall_probability,
            new_recall_probability,
            consolidation_event,
            trigger_reason
        ) VALUES (
            NEW.id,
            COALESCE(OLD.consolidation_strength, 1.0),
            new_consolidation,
            OLD.recall_probability,
            new_recall_prob,
            'access',
            'Automatic consolidation on memory access'
        );
    END IF;
    
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Create the trigger
DROP TRIGGER IF EXISTS memories_consolidation_trigger ON memories;
CREATE TRIGGER memories_consolidation_trigger
    BEFORE UPDATE ON memories
    FOR EACH ROW
    EXECUTE FUNCTION trigger_consolidation_update();

-- Insert initial tier statistics
INSERT INTO memory_tier_statistics (tier, memory_count, avg_consolidation_strength, avg_recall_probability)
SELECT 
    tier,
    COUNT(*) as memory_count,
    AVG(COALESCE(consolidation_strength, 1.0)) as avg_consolidation_strength,
    AVG(COALESCE(recall_probability, 0.8)) as avg_recall_probability
FROM memories 
WHERE status = 'active'
GROUP BY tier;

-- Record migration completion
-- Note: Using the existing migration_history table structure
INSERT INTO migration_history (migration_name, success, migration_reason)
VALUES (
    '002_consolidation_schema',
    true,
    'Added consolidation tracking, frozen storage, and mathematical memory models'
);

COMMIT;