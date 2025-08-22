-- Migration 006 Rollback: Remove Frozen Tier Support
-- Purpose: Safely rollback frozen tier implementation
-- WARNING: This will permanently delete all frozen memories data

-- Step 1: Check if there are frozen memories before rollback
DO $$
DECLARE 
    frozen_count INTEGER;
BEGIN
    SELECT COUNT(*) INTO frozen_count FROM frozen_memories;
    
    IF frozen_count > 0 THEN
        RAISE WARNING 'Rolling back frozen tier with % frozen memories. Data will be permanently lost!', frozen_count;
        
        -- Log the rollback for audit purposes
        INSERT INTO migration_history (
            memory_id,
            from_tier,
            to_tier,
            migration_reason,
            success,
            error_message
        ) 
        SELECT 
            original_memory_id,
            'frozen',
            'deleted',
            'Rollback of frozen tier migration 006',
            false,
            'Frozen memories permanently deleted during rollback'
        FROM frozen_memories;
    END IF;
END $$;

-- Step 2: Drop frozen tier views and analytics
DROP VIEW IF EXISTS frozen_memory_analytics;

-- Step 3: Drop frozen tier maintenance functions
DROP FUNCTION IF EXISTS optimize_frozen_tier();
DROP FUNCTION IF EXISTS track_frozen_access();
DROP FUNCTION IF EXISTS calculate_compression_metrics();

-- Step 4: Drop triggers on frozen_memories table
DROP TRIGGER IF EXISTS track_frozen_memory_access ON frozen_memories;
DROP TRIGGER IF EXISTS calculate_frozen_compression_metrics ON frozen_memories;
DROP TRIGGER IF EXISTS update_frozen_memories_updated_at ON frozen_memories;

-- Step 5: Drop indexes on frozen_memories table
DROP INDEX IF EXISTS idx_frozen_memories_original_id;
DROP INDEX IF EXISTS idx_frozen_memories_content_hash;
DROP INDEX IF EXISTS idx_frozen_memories_embedding;
DROP INDEX IF EXISTS idx_frozen_memories_frozen_at;
DROP INDEX IF EXISTS idx_frozen_memories_last_unfrozen;
DROP INDEX IF EXISTS idx_frozen_memories_compression_ratio;
DROP INDEX IF EXISTS idx_frozen_memories_metadata;

-- Step 6: Drop frozen tier indexes on memories table
DROP INDEX IF EXISTS idx_memories_frozen_tier;
DROP INDEX IF EXISTS idx_memories_freeze_candidates;

-- Step 7: Drop the frozen_memories table
-- WARNING: This permanently destroys all frozen memory data
DROP TABLE IF EXISTS frozen_memories;

-- Step 8: Update any existing memories in frozen tier back to cold tier
-- This prevents foreign key constraint issues
UPDATE memories 
SET tier = 'cold'::memory_tier,
    status = 'active'::memory_status
WHERE tier = 'frozen'::memory_tier;

-- Step 9: Remove 'frozen' value from memory_tier enum
-- NOTE: PostgreSQL doesn't support removing enum values directly
-- We need to recreate the enum without 'frozen'

-- Create temporary enum without frozen
CREATE TYPE memory_tier_temp AS ENUM ('working', 'warm', 'cold');

-- Update all references to use the temporary enum
ALTER TABLE memories ALTER COLUMN tier TYPE memory_tier_temp 
    USING tier::text::memory_tier_temp;

ALTER TABLE memory_clusters ALTER COLUMN tier TYPE memory_tier_temp 
    USING tier::text::memory_tier_temp;

ALTER TABLE migration_history ALTER COLUMN from_tier TYPE memory_tier_temp 
    USING from_tier::text::memory_tier_temp;

ALTER TABLE migration_history ALTER COLUMN to_tier TYPE memory_tier_temp 
    USING to_tier::text::memory_tier_temp;

-- Drop the old enum
DROP TYPE memory_tier;

-- Rename the temporary enum
ALTER TYPE memory_tier_temp RENAME TO memory_tier;

-- Step 10: Recreate dropped indexes that depend on memory_tier enum
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
CREATE INDEX idx_memories_status_tier ON memories (status, tier);

-- Step 11: Vacuum analyze affected tables to reclaim space and update statistics
VACUUM ANALYZE memories;
VACUUM ANALYZE memory_clusters;
VACUUM ANALYZE migration_history;

-- Step 12: Final verification
DO $$
BEGIN
    -- Verify the frozen value was removed from the enum
    IF EXISTS (
        SELECT 1 FROM pg_enum e
        JOIN pg_type t ON e.enumtypid = t.oid
        WHERE t.typname = 'memory_tier' AND e.enumlabel = 'frozen'
    ) THEN
        RAISE EXCEPTION 'Failed to remove frozen value from memory_tier enum';
    END IF;
    
    -- Verify the frozen_memories table was dropped
    IF EXISTS (
        SELECT 1 FROM information_schema.tables 
        WHERE table_name = 'frozen_memories'
    ) THEN
        RAISE EXCEPTION 'Failed to drop frozen_memories table';
    END IF;
    
    -- Verify no memories are still in frozen tier
    IF EXISTS (SELECT 1 FROM memories WHERE tier::text = 'frozen') THEN
        RAISE EXCEPTION 'Some memories are still in frozen tier after rollback';
    END IF;
    
    RAISE NOTICE 'Frozen tier support successfully removed from database schema';
    RAISE NOTICE 'All frozen memories data has been permanently deleted';
END $$;