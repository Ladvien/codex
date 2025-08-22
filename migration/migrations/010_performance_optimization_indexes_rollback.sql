-- Rollback Migration 010: Performance Optimization Indexes
-- Purpose: Remove performance optimization indexes if rollback is needed
-- TICKET-011: Performance Optimization Indexes Rollback

BEGIN;

-- ========================================
-- DROP PERFORMANCE OPTIMIZATION INDEXES
-- ========================================
-- Drop all indexes created in migration 010 in reverse order

-- Drop migration history indexes
DROP INDEX IF EXISTS idx_migration_history_errors;
DROP INDEX IF EXISTS idx_migration_history_performance;

-- Drop cluster mapping indexes
DROP INDEX IF EXISTS idx_cluster_mappings_memory_lookup;
DROP INDEX IF EXISTS idx_cluster_mappings_optimized;

-- Drop summary optimization indexes
DROP INDEX IF EXISTS idx_summaries_level_range_optimized;

-- Drop time-based indexes  
DROP INDEX IF EXISTS idx_memories_expires_optimized;
DROP INDEX IF EXISTS idx_memories_temporal_optimized;

-- Drop optimized hybrid search indexes
DROP INDEX IF EXISTS idx_memories_embedding_hnsw_optimized;
DROP INDEX IF EXISTS idx_memories_hybrid_search_optimized;
DROP INDEX IF EXISTS idx_memories_metadata_optimized;

-- Drop consolidated access patterns index
DROP INDEX IF EXISTS idx_memories_access_patterns_consolidated;

-- ========================================
-- DROP PERFORMANCE MONITORING FUNCTIONS
-- ========================================
-- Remove functions created for index monitoring and analysis

DROP FUNCTION IF EXISTS benchmark_common_queries(TEXT);
DROP FUNCTION IF EXISTS generate_index_maintenance_recommendations();
DROP FUNCTION IF EXISTS verify_query_plan_improvements();
DROP FUNCTION IF EXISTS calculate_index_size_projection(TEXT, TEXT, TEXT[], TEXT);

-- ========================================
-- DROP MONITORING VIEWS AND TABLES
-- ========================================
-- Remove monitoring infrastructure

DROP VIEW IF EXISTS index_maintenance_stats;
DROP INDEX IF EXISTS idx_performance_baselines_lookup;
DROP TABLE IF EXISTS index_performance_baselines;

COMMIT;

-- ========================================
-- ROLLBACK VALIDATION
-- ========================================
-- Verify rollback was successful
DO $$
DECLARE
    remaining_indexes INTEGER;
    rollback_indexes TEXT[] := ARRAY[
        'idx_memories_access_patterns_consolidated',
        'idx_memories_metadata_optimized',
        'idx_memories_hybrid_search_optimized',
        'idx_memories_embedding_hnsw_optimized',
        'idx_memories_temporal_optimized',
        'idx_memories_expires_optimized',
        'idx_summaries_level_range_optimized',
        'idx_cluster_mappings_optimized',
        'idx_cluster_mappings_memory_lookup',
        'idx_migration_history_performance',
        'idx_migration_history_errors'
    ];
    still_exists TEXT[] := '{}';
    idx_name TEXT;
BEGIN
    -- Check that indexes were actually dropped
    FOREACH idx_name IN ARRAY rollback_indexes
    LOOP
        SELECT COUNT(*) INTO remaining_indexes
        FROM pg_indexes 
        WHERE indexname = idx_name;
        
        IF remaining_indexes > 0 THEN
            still_exists := array_append(still_exists, idx_name);
        END IF;
    END LOOP;
    
    IF array_length(still_exists, 1) > 0 THEN
        RAISE WARNING 'Some indexes were not dropped during rollback: %', array_to_string(still_exists, ', ');
    ELSE
        RAISE NOTICE 'Rollback successful: All performance optimization indexes removed';
    END IF;
    
    -- Verify functions were dropped
    SELECT COUNT(*) INTO remaining_indexes
    FROM pg_proc 
    WHERE proname IN ('benchmark_common_queries', 'generate_index_maintenance_recommendations',
                     'verify_query_plan_improvements', 'calculate_index_size_projection');
    
    IF remaining_indexes > 0 THEN
        RAISE WARNING 'Some functions were not dropped during rollback';
    ELSE
        RAISE NOTICE 'All monitoring functions removed successfully';
    END IF;
    
    RAISE NOTICE 'Migration 010 rollback completed successfully';
    RAISE NOTICE 'Note: Original query performance will be restored to pre-migration state';
END $$;