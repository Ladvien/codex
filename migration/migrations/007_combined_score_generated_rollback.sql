-- Migration 007 Rollback: Remove Combined Score Generated Column
-- Purpose: Rollback combined_score GENERATED ALWAYS AS STORED column to nullable calculated field
-- CRITICAL-002 Rollback: Return to runtime three-component scoring calculation
-- Author: postgres-vector-optimizer
-- Date: 2025-08-22

BEGIN;

-- Step 1: Drop the trigger that auto-updates component scores
DROP TRIGGER IF EXISTS update_component_scores_trigger ON memories;
DROP FUNCTION IF EXISTS auto_update_component_scores();

-- Step 2: Drop performance monitoring views and functions
DROP VIEW IF EXISTS combined_score_performance_stats;
DROP FUNCTION IF EXISTS benchmark_combined_score_performance(VARCHAR, INTEGER, INTEGER);
DROP FUNCTION IF EXISTS validate_combined_score_generation(UUID);

-- Step 3: Drop all specialized indexes created for the generated column
DROP INDEX CONCURRENTLY IF EXISTS idx_memories_combined_score_working;
DROP INDEX CONCURRENTLY IF EXISTS idx_memories_combined_score_warm;
DROP INDEX CONCURRENTLY IF EXISTS idx_memories_combined_score_cold;
DROP INDEX CONCURRENTLY IF EXISTS idx_memories_combined_score_general;
DROP INDEX CONCURRENTLY IF EXISTS idx_memories_three_component_generated;

-- Step 4: Store current values of the generated combined_score column
-- This preserves the calculated values during the rollback process
CREATE TEMPORARY TABLE temp_combined_scores AS
SELECT id, combined_score
FROM memories
WHERE combined_score IS NOT NULL;

-- Step 5: Remove the generated column and its constraint
ALTER TABLE memories DROP CONSTRAINT IF EXISTS check_combined_score_generated;
ALTER TABLE memories DROP COLUMN IF EXISTS combined_score;

-- Step 6: Recreate combined_score as a nullable regular column (as it was before)
ALTER TABLE memories 
ADD COLUMN combined_score FLOAT;

-- Step 7: Add the original constraint for combined_score bounds
ALTER TABLE memories 
ADD CONSTRAINT check_combined_score 
CHECK (combined_score >= 0.0 AND combined_score <= 1.0);

-- Step 8: Restore the previous combined_score values from temporary table
UPDATE memories 
SET combined_score = temp.combined_score
FROM temp_combined_scores temp
WHERE memories.id = temp.id;

-- Step 9: Recreate the original three-component composite index (from migration 003)
CREATE INDEX memories_three_component_idx 
ON memories (combined_score DESC, importance_score DESC, recency_score DESC, relevance_score DESC)
WHERE status = 'active';

-- Step 10: Recreate the original combined_score index (from migration 003)
CREATE INDEX memories_combined_score_idx ON memories (combined_score DESC);

-- Step 11: Clean up temporary table
DROP TABLE temp_combined_scores;

-- Step 12: Verification - check that we've restored the pre-migration state
DO $$
DECLARE
    has_generated_col BOOLEAN;
    has_regular_col BOOLEAN;
    constraint_count INTEGER;
    index_count INTEGER;
BEGIN
    -- Check that combined_score is no longer a generated column
    SELECT EXISTS(
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'memories' 
        AND column_name = 'combined_score' 
        AND is_generated = 'ALWAYS'
    ) INTO has_generated_col;
    
    -- Check that combined_score exists as a regular column
    SELECT EXISTS(
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'memories' 
        AND column_name = 'combined_score' 
        AND is_generated = 'NEVER'
    ) INTO has_regular_col;
    
    -- Check constraint was restored
    SELECT COUNT(*) INTO constraint_count
    FROM information_schema.table_constraints tc
    JOIN information_schema.constraint_column_usage ccu ON tc.constraint_name = ccu.constraint_name
    WHERE tc.table_name = 'memories' 
    AND ccu.column_name = 'combined_score'
    AND tc.constraint_type = 'CHECK';
    
    -- Check indexes were restored
    SELECT COUNT(*) INTO index_count
    FROM pg_indexes 
    WHERE tablename = 'memories' 
    AND indexname IN ('memories_combined_score_idx', 'memories_three_component_idx');
    
    IF has_generated_col THEN
        RAISE EXCEPTION 'Rollback failed: combined_score is still a generated column';
    END IF;
    
    IF NOT has_regular_col THEN
        RAISE EXCEPTION 'Rollback failed: combined_score regular column was not created';
    END IF;
    
    IF constraint_count = 0 THEN
        RAISE WARNING 'Rollback warning: combined_score constraint may not have been restored';
    END IF;
    
    IF index_count < 2 THEN
        RAISE WARNING 'Rollback warning: original indexes may not have been fully restored (found % of 2)', index_count;
    END IF;
    
    RAISE NOTICE 'Rollback verification completed:';
    RAISE NOTICE '  - Generated column removed: %', NOT has_generated_col;
    RAISE NOTICE '  - Regular column restored: %', has_regular_col;
    RAISE NOTICE '  - Constraints restored: % found', constraint_count;
    RAISE NOTICE '  - Indexes restored: % of 2 expected', index_count;
END $$;

COMMIT;

-- Final rollback message
DO $$
BEGIN
    RAISE NOTICE 'Migration 007 rollback completed successfully';
    RAISE NOTICE 'Combined score reverted to nullable calculated field';
    RAISE NOTICE 'Application code must now handle runtime calculation of combined_score';
    RAISE NOTICE 'Performance will return to previous baseline (runtime calculation overhead)';
END $$;