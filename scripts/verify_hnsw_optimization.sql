-- HNSW Vector Index Optimization Verification Script
-- Purpose: Verify that HNSW parameters are optimally configured for 1536-dimensional vectors
-- CODEX-006: Optimize HNSW Vector Parameters

-- ========================================
-- VERIFY HNSW INDEX PARAMETERS
-- ========================================

-- Check current HNSW indexes and their parameters
SELECT 
    schemaname,
    tablename,
    indexname,
    indexdef,
    CASE 
        WHEN indexdef LIKE '%m = 48%' AND indexdef LIKE '%ef_construction = 200%' 
        THEN '✅ OPTIMAL'
        WHEN indexdef LIKE '%m = 16%' 
        THEN '❌ SUBOPTIMAL (m=16)'
        ELSE '⚠️  UNKNOWN'
    END as parameter_status,
    pg_size_pretty(pg_relation_size(indexname::regclass)) as index_size
FROM pg_indexes 
WHERE indexdef LIKE '%USING hnsw%'
  AND schemaname = 'public'
ORDER BY tablename, indexname;

-- ========================================
-- VERIFY QUERY-TIME SETTINGS
-- ========================================

-- Check current hnsw.ef_search setting
SHOW hnsw.ef_search;

-- Check maintenance_work_mem setting
SHOW maintenance_work_mem;

-- ========================================
-- VERIFY INDEX USAGE AND PERFORMANCE
-- ========================================

-- Check index statistics for HNSW indexes
SELECT 
    schemaname,
    tablename, 
    indexname,
    idx_tup_read as total_reads,
    idx_tup_fetch as total_fetches,
    CASE 
        WHEN idx_tup_read > 0 
        THEN ROUND((idx_tup_fetch::FLOAT / idx_tup_read * 100), 2)
        ELSE 0 
    END as hit_ratio_percent,
    pg_size_pretty(pg_relation_size(indexrelid)) as size
FROM pg_stat_user_indexes 
WHERE indexname LIKE '%hnsw%'
ORDER BY idx_tup_read DESC;

-- ========================================
-- TEST VECTOR SIMILARITY QUERY PERFORMANCE  
-- ========================================

-- Create a simple test query with EXPLAIN ANALYZE
-- This will show if the HNSW index is being used effectively
\echo 'Testing HNSW index usage with sample query:'

EXPLAIN (ANALYZE, BUFFERS, FORMAT TEXT) 
SELECT id, content, (1 - (embedding <=> '[0.1,0.2,0.3,0.4,0.5]'::vector)) as similarity
FROM memories 
WHERE status = 'active' 
  AND embedding IS NOT NULL
ORDER BY embedding <=> '[0.1,0.2,0.3,0.4,0.5]'::vector
LIMIT 10;

-- ========================================
-- PERFORMANCE BASELINE METRICS
-- ========================================

-- Check vector operation performance baselines
SELECT 
    measurement_type,
    query_pattern,
    AVG(execution_time_ms) as avg_execution_time_ms,
    MAX(execution_time_ms) as max_execution_time_ms,
    COUNT(*) as measurement_count
FROM index_performance_baselines 
WHERE measurement_type IN ('after_migration_011_critical', 'after_migration_010')
  AND query_pattern LIKE '%vector%'
GROUP BY measurement_type, query_pattern
ORDER BY measurement_type, avg_execution_time_ms DESC;

-- ========================================
-- HNSW INDEX HEALTH CHECK
-- ========================================

-- Check for any issues with HNSW indexes
SELECT 
    'HNSW Index Health Report' as report_section,
    COUNT(*) as total_hnsw_indexes,
    SUM(CASE WHEN indexdef LIKE '%m = 48%' THEN 1 ELSE 0 END) as optimized_indexes,
    SUM(CASE WHEN indexdef LIKE '%m = 16%' THEN 1 ELSE 0 END) as suboptimal_indexes
FROM pg_indexes 
WHERE indexdef LIKE '%USING hnsw%' 
  AND schemaname = 'public';

-- ========================================
-- RECOMMENDATIONS
-- ========================================

-- Generate optimization recommendations
DO $$
DECLARE
    hnsw_count INTEGER;
    optimal_count INTEGER;
    ef_search_setting TEXT;
    work_mem_setting TEXT;
BEGIN
    -- Count HNSW indexes
    SELECT COUNT(*) INTO hnsw_count 
    FROM pg_indexes 
    WHERE indexdef LIKE '%USING hnsw%' AND schemaname = 'public';
    
    -- Count optimal indexes
    SELECT COUNT(*) INTO optimal_count 
    FROM pg_indexes 
    WHERE indexdef LIKE '%m = 48%' AND indexdef LIKE '%ef_construction = 200%'
    AND schemaname = 'public';
    
    -- Get current settings
    SELECT setting INTO ef_search_setting FROM pg_settings WHERE name = 'hnsw.ef_search';
    SELECT setting INTO work_mem_setting FROM pg_settings WHERE name = 'maintenance_work_mem';
    
    RAISE NOTICE '========================================';
    RAISE NOTICE 'HNSW OPTIMIZATION STATUS REPORT';
    RAISE NOTICE '========================================';
    RAISE NOTICE 'Total HNSW indexes: %', hnsw_count;
    RAISE NOTICE 'Optimized indexes: %', optimal_count;
    
    IF optimal_count = hnsw_count AND hnsw_count > 0 THEN
        RAISE NOTICE '✅ STATUS: All HNSW indexes are optimally configured';
    ELSIF optimal_count < hnsw_count THEN
        RAISE NOTICE '❌ STATUS: % indexes need optimization', (hnsw_count - optimal_count);
        RAISE NOTICE '📝 ACTION: Run migration 011 to optimize HNSW parameters';
    ELSE
        RAISE NOTICE '⚠️  STATUS: No HNSW indexes found';
        RAISE NOTICE '📝 ACTION: Create HNSW indexes with optimal parameters';
    END IF;
    
    RAISE NOTICE '⚙️  Current ef_search: %', ef_search_setting;
    IF ef_search_setting::INTEGER != 64 THEN
        RAISE NOTICE '❌ RECOMMENDATION: Set hnsw.ef_search = 64 for optimal performance';
    ELSE
        RAISE NOTICE '✅ ef_search optimally configured';
    END IF;
    
    RAISE NOTICE '⚙️  Current maintenance_work_mem: %', work_mem_setting;
    IF work_mem_setting NOT LIKE '%GB' THEN
        RAISE NOTICE '❌ RECOMMENDATION: Increase maintenance_work_mem to 4GB for vector index builds';
    ELSE
        RAISE NOTICE '✅ maintenance_work_mem appropriately configured';
    END IF;
    
    RAISE NOTICE '========================================';
END $$;

-- ========================================
-- PERFORMANCE TARGET VALIDATION
-- ========================================

-- Validate that performance targets are met
-- Target: <50ms P99 latency for 1536-dim vector similarity search

\echo 'Performance target validation:'
\echo 'Target: <50ms P99 latency for vector similarity search on 1536-dim vectors'
\echo 'Expected improvement: 20-30% faster than previous m=16 configuration'

-- Show recent performance metrics if available
SELECT 
    'Performance Target Status' as metric,
    CASE 
        WHEN MAX(execution_time_ms) < 50 THEN '✅ MEETING TARGET (<50ms)'
        WHEN MAX(execution_time_ms) < 100 THEN '⚠️  CLOSE TO TARGET (<100ms)'
        ELSE '❌ MISSING TARGET (>' || ROUND(MAX(execution_time_ms)) || 'ms)'
    END as status,
    ROUND(AVG(execution_time_ms), 2) as avg_ms,
    ROUND(MAX(execution_time_ms), 2) as max_ms
FROM index_performance_baselines 
WHERE measurement_type = 'after_migration_011_critical'
  AND measured_at >= NOW() - INTERVAL '1 day';