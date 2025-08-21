-- Migration 002 Rollback: Remove Consolidation Schema
-- Purpose: Safely remove consolidation features while preserving existing memories

-- Drop views first (they depend on tables)
DROP VIEW IF EXISTS consolidation_event_summary;
DROP VIEW IF EXISTS memory_consolidation_analytics;

-- Drop functions (they may be referenced by triggers)
DROP FUNCTION IF EXISTS update_tier_statistics();
DROP FUNCTION IF EXISTS unfreeze_memory(UUID);
DROP FUNCTION IF EXISTS freeze_memory(UUID);

-- Drop triggers before dropping the function they use
DROP TRIGGER IF EXISTS update_memory_consolidation_trigger ON memories;
DROP FUNCTION IF EXISTS auto_update_memory_consolidation();

-- Drop mathematical functions
DROP FUNCTION IF EXISTS update_consolidation_strength(FLOAT, INTERVAL);
DROP FUNCTION IF EXISTS calculate_recall_probability(TIMESTAMPTZ, FLOAT, FLOAT);

-- Drop indexes (do this before dropping tables)
DROP INDEX CONCURRENTLY IF EXISTS idx_tier_statistics_snapshot;
DROP INDEX CONCURRENTLY IF EXISTS idx_frozen_memories_frozen_at;
DROP INDEX CONCURRENTLY IF EXISTS idx_frozen_memories_original_id;
DROP INDEX CONCURRENTLY IF EXISTS idx_frozen_memories_embedding;
DROP INDEX CONCURRENTLY IF EXISTS idx_consolidation_log_event_type;
DROP INDEX CONCURRENTLY IF EXISTS idx_consolidation_log_memory;
DROP INDEX CONCURRENTLY IF EXISTS idx_memories_migration_candidates;
DROP INDEX CONCURRENTLY IF EXISTS idx_memories_decay_rate;
DROP INDEX CONCURRENTLY IF EXISTS idx_memories_recall_probability;
DROP INDEX CONCURRENTLY IF EXISTS idx_memories_consolidation_strength;

-- Drop new tables (in reverse dependency order)
DROP TABLE IF EXISTS memory_tier_statistics;
DROP TABLE IF EXISTS frozen_memories;
DROP TABLE IF EXISTS memory_consolidation_log;

-- Remove new columns from memories table
-- Note: We keep the data but remove the consolidation functionality
ALTER TABLE memories 
    DROP COLUMN IF EXISTS last_recall_interval,
    DROP COLUMN IF EXISTS recall_probability,
    DROP COLUMN IF EXISTS decay_rate,
    DROP COLUMN IF EXISTS consolidation_strength;

-- Remove 'frozen' from memory_tier enum
-- Note: This is more complex and requires recreating the enum
-- First, ensure no memories are using 'frozen' tier
UPDATE memories SET tier = 'cold' WHERE tier = 'frozen';

-- Create new enum without frozen
CREATE TYPE memory_tier_new AS ENUM ('working', 'warm', 'cold');

-- Update table to use new enum
ALTER TABLE memories ALTER COLUMN tier TYPE memory_tier_new USING tier::text::memory_tier_new;
ALTER TABLE memory_clusters ALTER COLUMN tier TYPE memory_tier_new USING tier::text::memory_tier_new;
ALTER TABLE migration_history ALTER COLUMN from_tier TYPE memory_tier_new USING from_tier::text::memory_tier_new;
ALTER TABLE migration_history ALTER COLUMN to_tier TYPE memory_tier_new USING to_tier::text::memory_tier_new;

-- Drop old enum and rename new one
DROP TYPE memory_tier;
ALTER TYPE memory_tier_new RENAME TO memory_tier;

-- Verification: Check that rollback was successful
DO $$
DECLARE
    consolidation_columns INTEGER;
    frozen_tier_count INTEGER;
BEGIN
    -- Check that consolidation columns are removed
    SELECT COUNT(*) INTO consolidation_columns
    FROM information_schema.columns 
    WHERE table_name = 'memories' 
    AND column_name IN ('consolidation_strength', 'decay_rate', 'recall_probability', 'last_recall_interval');
    
    IF consolidation_columns > 0 THEN
        RAISE NOTICE 'WARNING: % consolidation columns still exist in memories table', consolidation_columns;
    ELSE
        RAISE NOTICE 'SUCCESS: All consolidation columns removed from memories table';
    END IF;
    
    -- Check that frozen tier is removed
    BEGIN
        SELECT COUNT(*) INTO frozen_tier_count FROM memories WHERE tier = 'frozen';
        RAISE NOTICE 'WARNING: Found % memories still in frozen tier', frozen_tier_count;
    EXCEPTION WHEN others THEN
        RAISE NOTICE 'SUCCESS: Frozen tier successfully removed from memory_tier enum';
    END;
    
    -- Check that new tables are removed
    IF NOT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'memory_consolidation_log') THEN
        RAISE NOTICE 'SUCCESS: memory_consolidation_log table removed';
    ELSE
        RAISE NOTICE 'WARNING: memory_consolidation_log table still exists';
    END IF;
    
    IF NOT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'frozen_memories') THEN
        RAISE NOTICE 'SUCCESS: frozen_memories table removed';
    ELSE
        RAISE NOTICE 'WARNING: frozen_memories table still exists';
    END IF;
END;
$$;

-- Comments for operational guidance
COMMENT ON TYPE memory_tier IS 'Memory tier enum restored to original three-tier system (working, warm, cold)';

-- Final cleanup: Remove any orphaned comments
COMMENT ON FUNCTION calculate_recall_probability IS NULL;
COMMENT ON FUNCTION freeze_memory IS NULL;
COMMENT ON FUNCTION unfreeze_memory IS NULL;
COMMENT ON TABLE memory_consolidation_log IS NULL;
COMMENT ON TABLE frozen_memories IS NULL;