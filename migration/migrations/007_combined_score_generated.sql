-- Migration 007: Add Combined Score as Generated Column
-- Purpose: Implement combined_score as GENERATED ALWAYS AS STORED column for P99 <1ms latency
-- CRITICAL-002: Three-component scoring performance optimization
-- Author: postgres-vector-optimizer
-- Date: 2025-08-22

BEGIN;

-- Step 1: Remove existing combined_score column if it exists (it's currently nullable and runtime-calculated)
ALTER TABLE memories DROP COLUMN IF EXISTS combined_score;

-- Step 2: Add combined_score as GENERATED ALWAYS AS STORED column
-- Formula: (0.333 * recency_score + 0.333 * importance_score + 0.334 * relevance_score)
-- Using STORED for maximum query performance (P99 <1ms requirement)
ALTER TABLE memories 
ADD COLUMN combined_score FLOAT 
GENERATED ALWAYS AS (
    GREATEST(0.0, LEAST(1.0,
        0.333 * COALESCE(recency_score, 0.0) + 
        0.333 * COALESCE(importance_score, 0.0) + 
        0.334 * COALESCE(relevance_score, 0.0)
    ))
) STORED;

-- Step 3: Add constraint to ensure generated column stays in bounds [0,1]
ALTER TABLE memories 
ADD CONSTRAINT check_combined_score_generated 
CHECK (combined_score >= 0.0 AND combined_score <= 1.0);

-- Step 4: Create optimized index on combined_score for working memory queries
-- This index is critical for P99 <1ms latency on working tier
CREATE INDEX CONCURRENTLY idx_memories_combined_score_working 
ON memories (combined_score DESC, updated_at DESC) 
WHERE tier = 'working' AND status = 'active';

-- Step 5: Create optimized index for warm tier queries
CREATE INDEX CONCURRENTLY idx_memories_combined_score_warm 
ON memories (combined_score DESC, last_accessed_at DESC NULLS LAST) 
WHERE tier = 'warm' AND status = 'active';

-- Step 6: Create composite index for cold tier with recall probability
CREATE INDEX CONCURRENTLY idx_memories_combined_score_cold 
ON memories (combined_score DESC, recall_probability DESC NULLS LAST) 
WHERE tier = 'cold' AND status = 'active';

-- Step 7: Create general combined_score index for cross-tier queries
CREATE INDEX CONCURRENTLY idx_memories_combined_score_general 
ON memories (combined_score DESC, tier, status) 
WHERE status = 'active';

-- Step 8: Update the existing three-component composite index to use generated column
DROP INDEX IF EXISTS memories_three_component_idx;
CREATE INDEX idx_memories_three_component_generated 
ON memories (combined_score DESC, importance_score DESC, recency_score DESC, relevance_score DESC)
WHERE status = 'active';

-- Step 9: Create function to validate combined_score generation (for testing)
CREATE OR REPLACE FUNCTION validate_combined_score_generation(p_memory_id UUID)
RETURNS TABLE (
    memory_id UUID,
    calculated_score FLOAT,
    generated_score FLOAT,
    score_matches BOOLEAN
) AS $$
BEGIN
    RETURN QUERY
    SELECT 
        m.id,
        GREATEST(0.0, LEAST(1.0,
            0.333 * COALESCE(m.recency_score, 0.0) + 
            0.333 * COALESCE(m.importance_score, 0.0) + 
            0.334 * COALESCE(m.relevance_score, 0.0)
        )) as calculated_score,
        m.combined_score as generated_score,
        ABS(
            GREATEST(0.0, LEAST(1.0,
                0.333 * COALESCE(m.recency_score, 0.0) + 
                0.333 * COALESCE(m.importance_score, 0.0) + 
                0.334 * COALESCE(m.relevance_score, 0.0)
            )) - m.combined_score
        ) < 0.000001 as score_matches
    FROM memories m
    WHERE m.id = p_memory_id;
END;
$$ LANGUAGE plpgsql;

-- Step 10: Create monitoring view for combined score performance
CREATE OR REPLACE VIEW combined_score_performance_stats AS
SELECT 
    tier,
    COUNT(*) as memory_count,
    AVG(combined_score) as avg_combined_score,
    MIN(combined_score) as min_combined_score,
    MAX(combined_score) as max_combined_score,
    STDDEV(combined_score) as stddev_combined_score,
    COUNT(*) FILTER (WHERE combined_score > 0.8) as high_score_count,
    COUNT(*) FILTER (WHERE combined_score BETWEEN 0.5 AND 0.8) as medium_score_count,
    COUNT(*) FILTER (WHERE combined_score < 0.5) as low_score_count,
    AVG(COALESCE(recency_score, 0.0)) as avg_recency_component,
    AVG(COALESCE(importance_score, 0.0)) as avg_importance_component,
    AVG(COALESCE(relevance_score, 0.0)) as avg_relevance_component
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

-- Step 11: Performance tuning recommendations for generated column
-- These settings optimize for the specific access patterns of working memory:
-- - Frequent ORDER BY combined_score DESC queries
-- - High read-to-write ratio
-- - Sub-millisecond latency requirements

-- Recommended postgresql.conf settings for generated column performance:
-- 
-- # Increase shared_buffers to keep working tier indexes in memory
-- shared_buffers = 4GB  # 25-40% of RAM
-- 
-- # Optimize work_mem for ORDER BY operations on combined_score
-- work_mem = 64MB
-- 
-- # Ensure statistics are current for query planner
-- default_statistics_target = 1000
-- 
-- # Optimize random_page_cost for SSD storage
-- random_page_cost = 1.1
-- seq_page_cost = 1.0
-- 
-- # Checkpoint settings to handle generated column updates efficiently
-- checkpoint_completion_target = 0.9
-- max_wal_size = 4GB
-- min_wal_size = 1GB

-- Step 12: Create trigger to update recency/relevance scores which will auto-update combined_score
CREATE OR REPLACE FUNCTION auto_update_component_scores()
RETURNS TRIGGER AS $$
BEGIN
    -- Only update scores if the memory was accessed or importance changed
    IF (OLD.access_count IS DISTINCT FROM NEW.access_count) OR 
       (OLD.last_accessed_at IS DISTINCT FROM NEW.last_accessed_at) OR
       (OLD.importance_score IS DISTINCT FROM NEW.importance_score) THEN
        
        -- Update recency score using exponential decay
        NEW.recency_score = calculate_recency_score(NEW.last_accessed_at, NEW.created_at, 0.005);
        
        -- Update relevance score based on importance and access patterns
        NEW.relevance_score = LEAST(1.0, 
            0.5 * NEW.importance_score + 
            0.3 * LEAST(1.0, NEW.access_count / 100.0) + 
            0.2
        );
        
        -- combined_score will be automatically recalculated by generated column
    END IF;
    
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER update_component_scores_trigger
    BEFORE UPDATE ON memories
    FOR EACH ROW
    EXECUTE FUNCTION auto_update_component_scores();

-- Step 13: Initial population of recency and relevance scores for existing memories
-- This will automatically populate the generated combined_score column
UPDATE memories 
SET recency_score = calculate_recency_score(last_accessed_at, created_at, 0.005),
    relevance_score = LEAST(1.0, 
        0.5 * importance_score + 
        0.3 * LEAST(1.0, access_count / 100.0) + 
        0.2
    )
WHERE status = 'active'
AND (recency_score IS NULL OR relevance_score IS NULL);

-- Step 14: Verify the generated column is working correctly
DO $$
DECLARE
    test_count INTEGER;
    invalid_scores INTEGER;
BEGIN
    -- Count memories with generated combined_score
    SELECT COUNT(*) INTO test_count
    FROM memories 
    WHERE status = 'active' AND combined_score IS NOT NULL;
    
    -- Count any invalid generated scores (should be 0)
    SELECT COUNT(*) INTO invalid_scores
    FROM memories 
    WHERE status = 'active' 
    AND (combined_score < 0.0 OR combined_score > 1.0 OR combined_score IS NULL);
    
    IF invalid_scores > 0 THEN
        RAISE EXCEPTION 'Generated column validation failed: % invalid scores found', invalid_scores;
    END IF;
    
    RAISE NOTICE 'Combined score generation validation: % memories processed, % invalid scores (expected: 0)', 
        test_count, invalid_scores;
END $$;

-- Step 15: Create performance benchmark function
CREATE OR REPLACE FUNCTION benchmark_combined_score_performance(
    p_tier VARCHAR DEFAULT 'working',
    p_limit INTEGER DEFAULT 1000,
    p_iterations INTEGER DEFAULT 10
)
RETURNS TABLE (
    iteration INTEGER,
    execution_time_ms FLOAT,
    rows_returned INTEGER,
    avg_combined_score FLOAT
) AS $$
DECLARE
    start_time TIMESTAMP;
    end_time TIMESTAMP;
    exec_time FLOAT;
    i INTEGER;
    row_count INTEGER;
    avg_score FLOAT;
BEGIN
    FOR i IN 1..p_iterations LOOP
        start_time := clock_timestamp();
        
        -- Execute the critical query pattern for working memory
        SELECT COUNT(*), AVG(m.combined_score) INTO row_count, avg_score
        FROM memories m
        WHERE m.tier = p_tier::memory_tier
        AND m.status = 'active'
        ORDER BY m.combined_score DESC
        LIMIT p_limit;
        
        end_time := clock_timestamp();
        exec_time := EXTRACT(EPOCH FROM (end_time - start_time)) * 1000;
        
        RETURN QUERY SELECT i, exec_time, row_count, avg_score;
    END LOOP;
END;
$$ LANGUAGE plpgsql;

COMMIT;

-- Final verification message
DO $$
BEGIN
    RAISE NOTICE 'Migration 007 completed successfully: combined_score implemented as GENERATED ALWAYS AS STORED column';
    RAISE NOTICE 'Performance optimization: P99 <1ms latency target for working memory queries';
    RAISE NOTICE 'Indexes created: 4 specialized indexes for optimal query performance';
    RAISE NOTICE 'Next step: Update application code to use generated column instead of runtime calculation';
END $$;