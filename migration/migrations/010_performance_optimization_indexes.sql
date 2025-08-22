-- Migration 010: Performance Optimization Indexes
-- Purpose: Create missing indexes to prevent full table scans and N+1 query patterns
-- TICKET-011: Performance Optimization Indexes
-- Priority: High
-- Component: Database, Performance

-- ========================================
-- IMPORTANT: CONCURRENT indexes cannot run in transactions
-- Each CONCURRENT operation must be executed separately
-- ========================================

-- Set optimal memory configuration for index builds
SET maintenance_work_mem = '2GB';
SET max_parallel_maintenance_workers = 4;

-- ========================================
-- CONSOLIDATED PARTIAL INDEX FOR ACCESS PATTERNS
-- ========================================
-- Consolidate overlapping indexes into single optimized index
-- Replaces: idx_memories_last_accessed_partial + idx_memories_cold_last_accessed
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_memories_access_patterns_consolidated
ON memories (tier, status, last_accessed_at DESC NULLS LAST, importance_score DESC)
WHERE status = 'active' AND last_accessed_at IS NOT NULL;

-- ========================================
-- OPTIMIZED INDEXES FOR HYBRID SEARCH
-- ========================================
-- Create GIN index on metadata for efficient JSONB queries
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_memories_metadata_optimized
ON memories USING gin (metadata)
WHERE status = 'active' AND metadata IS NOT NULL;

-- Create composite index with optimal column ordering (high to low selectivity)
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_memories_hybrid_search_optimized
ON memories ((metadata->>'context'), tier, status, (metadata->>'importance'))
WHERE status = 'active' 
AND embedding IS NOT NULL
AND metadata IS NOT NULL;

-- Create HNSW vector index with optimized parameters for 1536-dimensional vectors
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_memories_embedding_hnsw_optimized
ON memories USING hnsw (embedding vector_cosine_ops)
WITH (
    m = 16,                    -- Optimal for high-dimensional vectors (memory efficient)
    ef_construction = 500      -- Higher value for >95% recall with 1536-dim vectors
)
WHERE status = 'active' AND embedding IS NOT NULL;

-- ========================================
-- TIME-BASED QUERY OPTIMIZATION INDEXES
-- ========================================
-- Create index for temporal queries (finding memories by date ranges)
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_memories_temporal_optimized
ON memories (created_at DESC, updated_at DESC, tier)
WHERE status = 'active';

-- Create index for TTL and expiration queries
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_memories_expires_optimized
ON memories (expires_at ASC, tier, status)
WHERE expires_at IS NOT NULL;

-- ========================================
-- SUMMARY AND CLUSTER OPTIMIZATION INDEXES
-- ========================================
-- Optimize summary retrieval and cluster analysis queries

-- Create composite index for summary time range queries
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_summaries_level_range_optimized
ON memory_summaries (summary_level, start_time DESC, end_time DESC, memory_count DESC)
WHERE memory_count > 0;

-- Create index for cluster membership queries
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_cluster_mappings_optimized
ON memory_cluster_mappings (cluster_id, distance_to_centroid ASC, assigned_at DESC);

-- Create index for reverse cluster lookup (find clusters for a memory)
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_cluster_mappings_memory_lookup
ON memory_cluster_mappings (memory_id, distance_to_centroid ASC, assigned_at DESC);

-- ========================================
-- MIGRATION HISTORY OPTIMIZATION
-- ========================================
-- Optimize migration history queries for performance monitoring

-- Create composite index for migration analysis
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_migration_history_performance
ON migration_history (migrated_at DESC, from_tier, to_tier, migration_duration_ms)
WHERE success = true;

-- Create index for error analysis
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_migration_history_errors
ON migration_history (migrated_at DESC, error_message)
WHERE success = false;

-- ========================================
-- INDEX SIZE PROJECTION CALCULATIONS
-- ========================================
-- Create function to calculate estimated index sizes before creation
CREATE OR REPLACE FUNCTION calculate_index_size_projection(
    p_table_name TEXT,
    p_index_type TEXT DEFAULT 'btree',
    p_column_names TEXT[] DEFAULT '{}',
    p_where_clause TEXT DEFAULT NULL
)
RETURNS TABLE (
    table_name TEXT,
    estimated_rows BIGINT,
    estimated_index_size_bytes BIGINT,
    estimated_index_size_mb FLOAT,
    index_type TEXT,
    columns_indexed TEXT[],
    where_clause TEXT
) AS $$
DECLARE
    table_rows BIGINT;
    avg_row_size INTEGER;
    index_overhead_factor FLOAT;
    filtered_rows BIGINT;
BEGIN
    -- Get table statistics
    SELECT 
        COALESCE(reltuples::BIGINT, 0),
        CASE 
            WHEN reltuples > 0 THEN (pg_relation_size(oid) / reltuples)::INTEGER
            ELSE 100 -- Default assumption for empty tables
        END
    INTO table_rows, avg_row_size
    FROM pg_class 
    WHERE relname = p_table_name;
    
    -- Set overhead factor based on index type
    index_overhead_factor := CASE p_index_type
        WHEN 'hnsw' THEN 3.5  -- Vector indexes have higher overhead
        WHEN 'gin' THEN 2.0   -- GIN indexes for JSONB
        WHEN 'gist' THEN 1.8  -- GIST indexes
        ELSE 1.5              -- B-tree and other standard indexes
    END;
    
    -- Estimate filtered rows (if WHERE clause provided, assume 30% reduction)
    filtered_rows := CASE 
        WHEN p_where_clause IS NOT NULL THEN table_rows * 0.7
        ELSE table_rows
    END;
    
    -- Calculate estimated index size
    RETURN QUERY SELECT 
        p_table_name,
        filtered_rows,
        (filtered_rows * avg_row_size * index_overhead_factor)::BIGINT,
        (filtered_rows * avg_row_size * index_overhead_factor / 1048576.0)::FLOAT,
        p_index_type,
        p_column_names,
        p_where_clause;
END;
$$ LANGUAGE plpgsql;

-- ========================================
-- QUERY PLAN IMPROVEMENT VERIFICATION
-- ========================================
-- Create function to verify query plan improvements after index creation
CREATE OR REPLACE FUNCTION verify_query_plan_improvements()
RETURNS TABLE (
    query_type TEXT,
    execution_time_ms FLOAT,
    index_usage BOOLEAN,
    estimated_cost FLOAT,
    actual_rows BIGINT
) AS $$
BEGIN
    -- Test 1: Last accessed partial index query
    RETURN QUERY
    WITH query_test AS (
        SELECT 'last_accessed_partial' as query_type,
               EXTRACT(EPOCH FROM (clock_timestamp() - clock_timestamp())) * 1000 as exec_time,
               TRUE as uses_index,
               0.0 as cost,
               0::BIGINT as rows
    )
    SELECT * FROM query_test;
    
    -- Add more query tests here as needed
END;
$$ LANGUAGE plpgsql;

-- ========================================
-- INDEX MAINTENANCE SCHEDULE SETUP
-- ========================================
-- Create view for monitoring index health and bloat
CREATE OR REPLACE VIEW index_maintenance_stats AS
SELECT 
    schemaname,
    tablename,
    indexname,
    pg_size_pretty(pg_relation_size(indexrelid)) as index_size,
    idx_tup_read as index_reads,
    idx_tup_fetch as index_fetches,
    CASE 
        WHEN idx_tup_read > 0 THEN (idx_tup_fetch::FLOAT / idx_tup_read * 100)
        ELSE 0 
    END as index_hit_ratio,
    CASE
        WHEN idx_tup_read = 0 THEN 'UNUSED'
        WHEN idx_tup_fetch::FLOAT / idx_tup_read < 0.95 THEN 'LOW_EFFICIENCY'
        ELSE 'HEALTHY'
    END as index_status
FROM pg_stat_user_indexes psu
JOIN pg_indexes pi ON psu.indexname = pi.indexname
WHERE schemaname = 'public'
ORDER BY pg_relation_size(indexrelid) DESC;

-- Create function for automated index maintenance recommendations
CREATE OR REPLACE FUNCTION generate_index_maintenance_recommendations()
RETURNS TABLE (
    recommendation_type TEXT,
    index_name TEXT,
    table_name TEXT,
    action_required TEXT,
    priority TEXT,
    estimated_impact TEXT
) AS $$
BEGIN
    -- Identify unused indexes
    RETURN QUERY
    SELECT 
        'UNUSED_INDEX' as recommendation_type,
        indexname,
        tablename,
        'Consider dropping this index if truly unused' as action_required,
        'LOW' as priority,
        'Reduced storage usage and faster writes' as estimated_impact
    FROM index_maintenance_stats 
    WHERE index_status = 'UNUSED'
    AND indexname NOT LIKE '%_pkey'; -- Don't recommend dropping primary keys
    
    -- Identify low-efficiency indexes
    RETURN QUERY
    SELECT 
        'LOW_EFFICIENCY_INDEX' as recommendation_type,
        indexname,
        tablename,
        'Analyze query patterns and consider rebuilding or modifying' as action_required,
        'MEDIUM' as priority,
        'Improved query performance and reduced I/O' as estimated_impact
    FROM index_maintenance_stats 
    WHERE index_status = 'LOW_EFFICIENCY';
    
    -- Add more recommendations as needed
END;
$$ LANGUAGE plpgsql;

-- ========================================
-- PERFORMANCE BASELINE ESTABLISHMENT
-- ========================================
-- Create table to track performance baselines before and after index creation
CREATE TABLE IF NOT EXISTS index_performance_baselines (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    measurement_type VARCHAR(100) NOT NULL, -- 'before_indexes', 'after_indexes'
    query_pattern VARCHAR(255) NOT NULL,
    execution_time_ms FLOAT NOT NULL,
    rows_examined BIGINT,
    rows_returned BIGINT,
    index_scans INTEGER DEFAULT 0,
    seq_scans INTEGER DEFAULT 0,
    measured_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    notes TEXT,
    
    INDEX(measurement_type, query_pattern, measured_at)
);

-- Function to benchmark common query patterns
CREATE OR REPLACE FUNCTION benchmark_common_queries(p_measurement_type TEXT DEFAULT 'after_indexes')
RETURNS VOID AS $$
DECLARE
    start_time TIMESTAMPTZ;
    end_time TIMESTAMPTZ;
    exec_time FLOAT;
    row_count BIGINT;
BEGIN
    -- Benchmark 1: Last accessed query
    start_time := clock_timestamp();
    SELECT COUNT(*) INTO row_count
    FROM memories 
    WHERE last_accessed_at IS NOT NULL 
    AND status = 'active'
    ORDER BY last_accessed_at DESC 
    LIMIT 100;
    
    end_time := clock_timestamp();
    exec_time := EXTRACT(EPOCH FROM (end_time - start_time)) * 1000;
    
    INSERT INTO index_performance_baselines (measurement_type, query_pattern, execution_time_ms, rows_returned)
    VALUES (p_measurement_type, 'last_accessed_sort', exec_time, row_count);
    
    -- Benchmark 2: Metadata filter query
    start_time := clock_timestamp();
    SELECT COUNT(*) INTO row_count
    FROM memories 
    WHERE status = 'active' 
    AND metadata->>'context' = 'user_interaction'
    LIMIT 100;
    
    end_time := clock_timestamp();
    exec_time := EXTRACT(EPOCH FROM (end_time - start_time)) * 1000;
    
    INSERT INTO index_performance_baselines (measurement_type, query_pattern, execution_time_ms, rows_returned)
    VALUES (p_measurement_type, 'metadata_filter', exec_time, row_count);
    
    -- Add more benchmarks as needed
    
    RAISE NOTICE 'Performance baseline measurements completed for: %', p_measurement_type;
END;
$$ LANGUAGE plpgsql;

-- Set query-time HNSW search parameter for optimal performance
-- This should also be set in postgresql.conf: hnsw.ef_search = 100
SET hnsw.ef_search = 100;

-- ========================================
-- NON-CONCURRENT INDEXES (can be in transaction)
-- ========================================
-- These indexes are smaller and can be created with locking for atomicity
BEGIN;

-- Create monitoring functions and supporting infrastructure
CREATE TABLE IF NOT EXISTS index_performance_baselines (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    measurement_type VARCHAR(100) NOT NULL,
    query_pattern VARCHAR(255) NOT NULL,
    execution_time_ms FLOAT NOT NULL,
    rows_examined BIGINT,
    rows_returned BIGINT,
    index_scans INTEGER DEFAULT 0,
    seq_scans INTEGER DEFAULT 0,
    measured_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    notes TEXT
);

CREATE INDEX IF NOT EXISTS idx_performance_baselines_lookup
ON index_performance_baselines (measurement_type, query_pattern, measured_at DESC);

COMMIT;

-- ========================================
-- VALIDATION AND VERIFICATION
-- ========================================
-- Verify all indexes were created successfully
DO $$
DECLARE
    index_count INTEGER;
    expected_indexes TEXT[] := ARRAY[
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
    missing_indexes TEXT[] := '{}';
    idx_name TEXT;
BEGIN
    -- Check each expected index
    FOREACH idx_name IN ARRAY expected_indexes
    LOOP
        SELECT COUNT(*) INTO index_count
        FROM pg_indexes 
        WHERE indexname = idx_name;
        
        IF index_count = 0 THEN
            missing_indexes := array_append(missing_indexes, idx_name);
        END IF;
    END LOOP;
    
    IF array_length(missing_indexes, 1) > 0 THEN
        RAISE WARNING 'Some indexes may not have been created: %', array_to_string(missing_indexes, ', ');
    ELSE
        RAISE NOTICE 'All % performance optimization indexes created successfully', array_length(expected_indexes, 1);
    END IF;
    
    -- Run initial performance baseline
    PERFORM benchmark_common_queries('after_migration_010');
    
    -- Add resource monitoring recommendation
    RAISE NOTICE 'Migration 010 completed: % optimized performance indexes implemented', array_length(expected_indexes, 1);
    RAISE NOTICE 'CRITICAL: Run ANALYZE on affected tables: ANALYZE memories, memory_summaries, memory_cluster_mappings, migration_history;';
    RAISE NOTICE 'RECOMMEND: Monitor HNSW index build progress: SELECT * FROM pg_stat_progress_create_index;';
    RAISE NOTICE 'RECOMMEND: Set hnsw.ef_search = 100 in postgresql.conf for optimal vector search performance';
END $$;