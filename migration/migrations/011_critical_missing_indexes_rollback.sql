-- Migration 011 Rollback: Remove Critical Missing Indexes
-- Purpose: Rollback critical indexes added in migration 011
-- CODEX-003: Rollback Critical Missing Indexes
-- Priority: Rollback Support
-- Component: Database, Migration Management

-- ========================================
-- ROLLBACK CRITICAL INDEXES
-- ========================================
-- Remove indexes created in migration 011
-- Use CONCURRENTLY to avoid locking during rollback

-- Remove duplicate detection index
DROP INDEX CONCURRENTLY IF EXISTS idx_memories_duplicate_detection_critical;

-- Remove working memory capacity index  
DROP INDEX CONCURRENTLY IF EXISTS idx_memories_working_memory_capacity_critical;

-- Remove consolidation candidates index
DROP INDEX CONCURRENTLY IF EXISTS idx_memories_consolidation_candidates_critical;

-- Remove cleanup operations index
DROP INDEX CONCURRENTLY IF EXISTS idx_memories_cleanup_operations_critical;

-- Remove optimized HNSW vector index
DROP INDEX CONCURRENTLY IF EXISTS idx_memories_embedding_hnsw_1536_optimized;

-- ========================================
-- RESTORE PREVIOUS HNSW INDEX (if needed)
-- ========================================
-- Recreate the previous suboptimal HNSW index to restore functionality
-- Note: This will have worse performance but maintains compatibility

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_memories_embedding_hnsw_optimized
ON memories USING hnsw (embedding vector_cosine_ops)
WITH (
    m = 16,                    -- Previous suboptimal value
    ef_construction = 500      -- Previous excessive value
)
WHERE status = 'active' AND embedding IS NOT NULL;

-- ========================================
-- RESET QUERY-TIME SETTINGS
-- ========================================
-- Reset to previous ef_search setting
SET hnsw.ef_search = 100; -- Previous setting from migration 010

-- ========================================
-- ROLLBACK VERIFICATION
-- ========================================
-- Verify all critical indexes were removed successfully
DO $$
DECLARE
    index_count INTEGER;
    rolled_back_indexes TEXT[] := ARRAY[
        'idx_memories_duplicate_detection_critical',
        'idx_memories_working_memory_capacity_critical', 
        'idx_memories_consolidation_candidates_critical',
        'idx_memories_cleanup_operations_critical',
        'idx_memories_embedding_hnsw_1536_optimized'
    ];
    remaining_indexes TEXT[] := '{}';
    idx_name TEXT;
BEGIN
    -- Check each index was removed
    FOREACH idx_name IN ARRAY rolled_back_indexes
    LOOP
        SELECT COUNT(*) INTO index_count
        FROM pg_indexes 
        WHERE indexname = idx_name;
        
        IF index_count > 0 THEN
            remaining_indexes := array_append(remaining_indexes, idx_name);
        END IF;
    END LOOP;
    
    IF array_length(remaining_indexes, 1) > 0 THEN
        RAISE WARNING 'Some indexes were not removed during rollback: %', array_to_string(remaining_indexes, ', ');
    ELSE
        RAISE NOTICE '✅ All critical indexes successfully removed during rollback';
    END IF;
    
    -- Verify previous HNSW index was restored
    SELECT COUNT(*) INTO index_count
    FROM pg_indexes 
    WHERE indexname = 'idx_memories_embedding_hnsw_optimized';
    
    IF index_count > 0 THEN
        RAISE NOTICE '✅ Previous HNSW index restored successfully';
    ELSE
        RAISE WARNING '⚠️ Previous HNSW index was not restored - vector search may be impacted';
    END IF;
    
    -- Run performance baseline after rollback
    PERFORM benchmark_common_queries('after_migration_011_rollback');
    
    -- Rollback completion notice
    RAISE NOTICE '🔄 Migration 011 ROLLBACK COMPLETED successfully';
    RAISE NOTICE '📊 PERFORMANCE: Queries will return to pre-optimization performance levels';
    RAISE NOTICE '⚠️  CRITICAL: Run ANALYZE on memories table: ANALYZE memories;';
    RAISE NOTICE '📈 IMPACT: Duplicate detection queries will be slower (no content_hash index)';
    RAISE NOTICE '📈 IMPACT: Working memory capacity checks will be slower (sequential scans)';
    RAISE NOTICE '📈 IMPACT: Vector searches will be 20-30%% slower (suboptimal HNSW parameters)';
END $$;