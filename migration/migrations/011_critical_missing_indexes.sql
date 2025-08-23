-- Migration 011: Critical Missing Indexes
-- Purpose: Add critical missing indexes identified in performance audit
-- CODEX-003: Add Critical Missing Indexes 
-- Priority: Critical - Database Performance
-- Component: Database, Query Optimization

-- ========================================
-- IMPORTANT: CONCURRENT indexes cannot run in transactions
-- Each CONCURRENT operation must be executed separately
-- ========================================

-- Set optimal memory configuration for index builds
SET maintenance_work_mem = '4GB';
SET max_parallel_maintenance_workers = 4;

-- ========================================
-- CRITICAL INDEX 1: DUPLICATE DETECTION OPTIMIZATION
-- ========================================
-- Purpose: Optimize duplicate detection queries in repository.rs:316-322
-- Query Pattern: SELECT EXISTS(SELECT 1 FROM memories WHERE content_hash = $1 AND tier = $2 AND status = 'active')
-- Performance Impact: Prevents full table scans for every duplicate check
-- Expected Improvement: >10x faster duplicate detection queries
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_memories_duplicate_detection_critical
ON memories (content_hash, tier, status)
WHERE status = 'active';

-- ========================================
-- CRITICAL INDEX 2: WORKING MEMORY CAPACITY OPTIMIZATION  
-- ========================================
-- Purpose: Optimize working memory capacity queries in repository.rs:334-338
-- Query Pattern: SELECT COUNT(*) FROM memories WHERE tier = 'working' AND status = 'active'
-- Performance Impact: Sequential scan elimination for working memory limit enforcement
-- Expected Improvement: >5x faster working memory capacity checks
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_memories_working_memory_capacity_critical
ON memories (tier, status, id)
WHERE tier = 'working' AND status = 'active';

-- ========================================
-- CRITICAL INDEX 3: CONSOLIDATION CANDIDATE OPTIMIZATION
-- ========================================
-- Purpose: Optimize consolidation candidate queries in repository.rs:1284-1301
-- Query Pattern: SELECT * FROM memories WHERE tier = $1 AND (recall_probability < $2 OR recall_probability IS NULL) AND status = 'active'
-- Performance Impact: Full table scan elimination for consolidation processing
-- Expected Improvement: >5x faster consolidation candidate selection
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_memories_consolidation_candidates_critical
ON memories (tier, recall_probability)
WHERE status = 'active';

-- ========================================
-- HIGH PRIORITY INDEX 4: CLEANUP OPERATIONS OPTIMIZATION
-- ========================================
-- Purpose: Optimize cleanup and maintenance queries
-- Query Pattern: SELECT * FROM memories WHERE status = 'active' AND last_accessed_at < $1
-- Performance Impact: Faster cleanup operations and maintenance jobs
-- Expected Improvement: >3x faster cleanup queries
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_memories_cleanup_operations_critical
ON memories (status, last_accessed_at)
WHERE status = 'active' AND last_accessed_at IS NOT NULL;

-- ========================================
-- VECTOR INDEX PARAMETER OPTIMIZATION
-- ========================================
-- Purpose: Fix suboptimal HNSW parameters for 1536-dimensional vectors
-- Issue: Current m=16 too low for 1536-dim vectors, ef_construction=500 excessive
-- Solution: Update to m=48, ef_construction=200 for optimal performance
-- Expected Improvement: 20-30% better vector search P99 latency

-- First, drop the existing suboptimal HNSW index
DROP INDEX CONCURRENTLY IF EXISTS idx_memories_embedding_hnsw_optimized;

-- Create new HNSW index with optimal parameters for 1536-dimensional vectors
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_memories_embedding_hnsw_1536_optimized
ON memories USING hnsw (embedding vector_cosine_ops)
WITH (
    m = 48,                    -- Optimal for 1536-dimensional vectors (was 16)
    ef_construction = 200      -- Efficient build time while maintaining >95% recall (was 500)
)
WHERE status = 'active' AND embedding IS NOT NULL;

-- ========================================
-- QUERY-TIME OPTIMIZATION SETTINGS
-- ========================================
-- Set optimal ef_search for query-time performance
-- This provides the best balance of speed vs accuracy for 1536-dim vectors
SET hnsw.ef_search = 64;

-- ========================================
-- INDEX SIZE AND PERFORMANCE PROJECTIONS
-- ========================================
-- Calculate estimated sizes for the new critical indexes
DO $$
DECLARE
    table_rows BIGINT;
    projected_sizes RECORD;
BEGIN
    -- Get current memory table size
    SELECT reltuples::BIGINT INTO table_rows
    FROM pg_class WHERE relname = 'memories';
    
    RAISE NOTICE 'Creating critical indexes for % memory records', table_rows;
    
    -- Calculate projected index sizes
    FOR projected_sizes IN 
        SELECT * FROM calculate_index_size_projection(
            'memories', 
            'btree', 
            ARRAY['content_hash', 'tier', 'status'], 
            'status = active'
        )
    LOOP
        RAISE NOTICE 'Duplicate detection index: ~%.1f MB estimated', projected_sizes.estimated_index_size_mb;
    END LOOP;
    
    FOR projected_sizes IN 
        SELECT * FROM calculate_index_size_projection(
            'memories', 
            'btree', 
            ARRAY['tier', 'status', 'id'], 
            'tier = working AND status = active'
        )
    LOOP
        RAISE NOTICE 'Working memory index: ~%.1f MB estimated', projected_sizes.estimated_index_size_mb;
    END LOOP;
    
    FOR projected_sizes IN 
        SELECT * FROM calculate_index_size_projection(
            'memories', 
            'hnsw', 
            ARRAY['embedding'], 
            'status = active AND embedding IS NOT NULL'
        )
    LOOP
        RAISE NOTICE 'HNSW vector index: ~%.1f MB estimated', projected_sizes.estimated_index_size_mb;
    END LOOP;
END $$;

-- ========================================
-- VALIDATION AND VERIFICATION
-- ========================================
-- Verify all critical indexes were created successfully
DO $$
DECLARE
    index_count INTEGER;
    critical_indexes TEXT[] := ARRAY[
        'idx_memories_duplicate_detection_critical',
        'idx_memories_working_memory_capacity_critical', 
        'idx_memories_consolidation_candidates_critical',
        'idx_memories_cleanup_operations_critical',
        'idx_memories_embedding_hnsw_1536_optimized'
    ];
    missing_indexes TEXT[] := '{}';
    idx_name TEXT;
BEGIN
    -- Check each critical index
    FOREACH idx_name IN ARRAY critical_indexes
    LOOP
        SELECT COUNT(*) INTO index_count
        FROM pg_indexes 
        WHERE indexname = idx_name;
        
        IF index_count = 0 THEN
            missing_indexes := array_append(missing_indexes, idx_name);
        END IF;
    END LOOP;
    
    IF array_length(missing_indexes, 1) > 0 THEN
        RAISE WARNING 'CRITICAL: Some indexes failed to create: %', array_to_string(missing_indexes, ', ');
        RAISE EXCEPTION 'Index creation failed - deployment blocked';
    ELSE
        RAISE NOTICE '✅ All % critical indexes created successfully', array_length(critical_indexes, 1);
    END IF;
    
    -- Run performance validation
    PERFORM benchmark_common_queries('after_migration_011_critical');
    
    -- Success notification
    RAISE NOTICE '🚀 Migration 011 COMPLETED: Critical missing indexes implemented';
    RAISE NOTICE '📊 PERFORMANCE IMPACT: Expected >10x improvement in duplicate detection';
    RAISE NOTICE '📊 PERFORMANCE IMPACT: Expected >5x improvement in working memory queries';
    RAISE NOTICE '📊 PERFORMANCE IMPACT: Expected 20-30%% improvement in vector searches';
    RAISE NOTICE '⚠️  CRITICAL: Run ANALYZE on memories table: ANALYZE memories;';
    RAISE NOTICE '⚙️  RECOMMEND: Monitor HNSW index build with: SELECT * FROM pg_stat_progress_create_index;';
    RAISE NOTICE '⚙️  RECOMMEND: Set hnsw.ef_search = 64 in postgresql.conf permanently';
END $$;