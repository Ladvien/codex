-- Combined Score Performance Benchmark Script
-- Purpose: Validate P99 <1ms latency achievement for working memory queries
-- CRITICAL-002: Performance validation for generated column optimization
-- Author: postgres-vector-optimizer
-- Date: 2025-08-22

-- This script benchmarks the performance improvement from using the generated
-- combined_score column instead of runtime calculation

BEGIN;

-- Setup benchmark environment
SET work_mem = '64MB';
SET random_page_cost = 1.1;
SET enable_seqscan = off; -- Force index usage for accurate testing

-- Create temporary function to run performance tests
CREATE OR REPLACE FUNCTION run_combined_score_benchmark()
RETURNS TABLE (
    test_name TEXT,
    query_type TEXT,
    avg_execution_time_ms FLOAT,
    min_execution_time_ms FLOAT,
    max_execution_time_ms FLOAT,
    p95_execution_time_ms FLOAT,
    p99_execution_time_ms FLOAT,
    rows_processed INTEGER,
    meets_sla BOOLEAN
) AS $$
DECLARE
    execution_times FLOAT[];
    i INTEGER;
    start_time TIMESTAMP;
    end_time TIMESTAMP;
    exec_time FLOAT;
    row_count INTEGER;
    test_iterations INTEGER := 100; -- Enough for accurate P99 measurement
BEGIN
    -- Test 1: Working tier queries using generated column (new approach)
    execution_times := ARRAY[]::FLOAT[];
    FOR i IN 1..test_iterations LOOP
        start_time := clock_timestamp();
        
        SELECT COUNT(*) INTO row_count
        FROM memories m
        WHERE m.tier = 'working'
        AND m.status = 'active'
        ORDER BY m.combined_score DESC
        LIMIT 50;
        
        end_time := clock_timestamp();
        exec_time := EXTRACT(EPOCH FROM (end_time - start_time)) * 1000;
        execution_times := array_append(execution_times, exec_time);
    END LOOP;
    
    -- Sort execution times for percentile calculation
    SELECT array_agg(time ORDER BY time) INTO execution_times FROM unnest(execution_times) as time;
    
    RETURN QUERY SELECT 
        'Working Tier - Generated Column'::TEXT,
        'SELECT with ORDER BY combined_score'::TEXT,
        (SELECT AVG(time) FROM unnest(execution_times) as time)::FLOAT,
        execution_times[1]::FLOAT,
        execution_times[array_length(execution_times, 1)]::FLOAT,
        execution_times[CEIL(array_length(execution_times, 1) * 0.95)]::FLOAT,
        execution_times[CEIL(array_length(execution_times, 1) * 0.99)]::FLOAT,
        row_count,
        execution_times[CEIL(array_length(execution_times, 1) * 0.99)] < 1.0; -- P99 < 1ms SLA
    
    -- Test 2: Working tier queries using runtime calculation (old approach - simulated)
    execution_times := ARRAY[]::FLOAT[];
    FOR i IN 1..test_iterations LOOP
        start_time := clock_timestamp();
        
        SELECT COUNT(*) INTO row_count
        FROM memories m
        WHERE m.tier = 'working'
        AND m.status = 'active'
        ORDER BY calculate_combined_score(m.recency_score, m.importance_score, m.relevance_score, 0.333, 0.333, 0.334) DESC
        LIMIT 50;
        
        end_time := clock_timestamp();
        exec_time := EXTRACT(EPOCH FROM (end_time - start_time)) * 1000;
        execution_times := array_append(execution_times, exec_time);
    END LOOP;
    
    SELECT array_agg(time ORDER BY time) INTO execution_times FROM unnest(execution_times) as time;
    
    RETURN QUERY SELECT 
        'Working Tier - Runtime Calculation'::TEXT,
        'SELECT with ORDER BY calculate_combined_score()'::TEXT,
        (SELECT AVG(time) FROM unnest(execution_times) as time)::FLOAT,
        execution_times[1]::FLOAT,
        execution_times[array_length(execution_times, 1)]::FLOAT,
        execution_times[CEIL(array_length(execution_times, 1) * 0.95)]::FLOAT,
        execution_times[CEIL(array_length(execution_times, 1) * 0.99)]::FLOAT,
        row_count,
        execution_times[CEIL(array_length(execution_times, 1) * 0.99)] < 1.0; -- P99 < 1ms SLA
    
    -- Test 3: Warm tier queries using generated column
    execution_times := ARRAY[]::FLOAT[];
    FOR i IN 1..test_iterations LOOP
        start_time := clock_timestamp();
        
        SELECT COUNT(*) INTO row_count
        FROM memories m
        WHERE m.tier = 'warm'
        AND m.status = 'active'
        ORDER BY m.combined_score DESC
        LIMIT 100;
        
        end_time := clock_timestamp();
        exec_time := EXTRACT(EPOCH FROM (end_time - start_time)) * 1000;
        execution_times := array_append(execution_times, exec_time);
    END LOOP;
    
    SELECT array_agg(time ORDER BY time) INTO execution_times FROM unnest(execution_times) as time;
    
    RETURN QUERY SELECT 
        'Warm Tier - Generated Column'::TEXT,
        'SELECT with ORDER BY combined_score'::TEXT,
        (SELECT AVG(time) FROM unnest(execution_times) as time)::FLOAT,
        execution_times[1]::FLOAT,
        execution_times[array_length(execution_times, 1)]::FLOAT,
        execution_times[CEIL(array_length(execution_times, 1) * 0.95)]::FLOAT,
        execution_times[CEIL(array_length(execution_times, 1) * 0.99)]::FLOAT,
        row_count,
        execution_times[CEIL(array_length(execution_times, 1) * 0.99)] < 1.0;
    
    -- Test 4: Cross-tier queries using generated column
    execution_times := ARRAY[]::FLOAT[];
    FOR i IN 1..test_iterations LOOP
        start_time := clock_timestamp();
        
        SELECT COUNT(*) INTO row_count
        FROM memories m
        WHERE m.status = 'active'
        AND m.tier IN ('working', 'warm', 'cold')
        ORDER BY m.combined_score DESC
        LIMIT 200;
        
        end_time := clock_timestamp();
        exec_time := EXTRACT(EPOCH FROM (end_time - start_time)) * 1000;
        execution_times := array_append(execution_times, exec_time);
    END LOOP;
    
    SELECT array_agg(time ORDER BY time) INTO execution_times FROM unnest(execution_times) as time;
    
    RETURN QUERY SELECT 
        'Cross-Tier - Generated Column'::TEXT,
        'SELECT with ORDER BY combined_score'::TEXT,
        (SELECT AVG(time) FROM unnest(execution_times) as time)::FLOAT,
        execution_times[1]::FLOAT,
        execution_times[array_length(execution_times, 1)]::FLOAT,
        execution_times[CEIL(array_length(execution_times, 1) * 0.95)]::FLOAT,
        execution_times[CEIL(array_length(execution_times, 1) * 0.99)]::FLOAT,
        row_count,
        execution_times[CEIL(array_length(execution_times, 1) * 0.99)] < 100.0; -- P99 < 100ms SLA for cross-tier
END;
$$ LANGUAGE plpgsql;

-- Create function to analyze query plans for optimization verification
CREATE OR REPLACE FUNCTION analyze_combined_score_query_plans()
RETURNS TABLE (
    query_description TEXT,
    execution_plan TEXT,
    uses_index BOOLEAN,
    estimated_cost FLOAT,
    estimated_rows INTEGER
) AS $$
BEGIN
    -- Analyze working tier query with generated column
    RETURN QUERY 
    SELECT 
        'Working Tier - Generated Column Query'::TEXT,
        plan_text::TEXT,
        plan_text LIKE '%Index Scan%'::BOOLEAN,
        total_cost::FLOAT,
        plan_rows::INTEGER
    FROM (
        SELECT 
            unnest(string_to_array(query_plan, E'\n')) as plan_text,
            regexp_replace(
                substring(query_plan from 'cost=([0-9.]+)\.\.([0-9.]+)'),
                'cost=([0-9.]+)\.\.([0-9.]+)', '\2'
            )::FLOAT as total_cost,
            regexp_replace(
                substring(query_plan from 'rows=([0-9]+)'),
                'rows=([0-9]+)', '\1'
            )::INTEGER as plan_rows
        FROM (
            SELECT query_plan FROM (
                SELECT explain_result as query_plan
                FROM (
                    SELECT * FROM dblink('',
                        'EXPLAIN (FORMAT TEXT, COSTS ON, BUFFERS ON) 
                         SELECT * FROM memories m 
                         WHERE m.tier = ''working'' AND m.status = ''active'' 
                         ORDER BY m.combined_score DESC LIMIT 50'
                    ) AS t(explain_result TEXT)
                ) AS explain_query
            ) AS plan_query
        ) AS parsed_plan
        WHERE plan_text IS NOT NULL AND plan_text != ''
        LIMIT 1
    );
    
    -- Analyze runtime calculation query
    RETURN QUERY 
    SELECT 
        'Working Tier - Runtime Calculation Query'::TEXT,
        plan_text::TEXT,
        plan_text LIKE '%Index Scan%'::BOOLEAN,
        total_cost::FLOAT,
        plan_rows::INTEGER
    FROM (
        SELECT 
            unnest(string_to_array(query_plan, E'\n')) as plan_text,
            regexp_replace(
                substring(query_plan from 'cost=([0-9.]+)\.\.([0-9.]+)'),
                'cost=([0-9.]+)\.\.([0-9.]+)', '\2'
            )::FLOAT as total_cost,
            regexp_replace(
                substring(query_plan from 'rows=([0-9]+)'),
                'rows=([0-9]+)', '\1'
            )::INTEGER as plan_rows
        FROM (
            SELECT query_plan FROM (
                SELECT explain_result as query_plan
                FROM (
                    SELECT * FROM dblink('',
                        'EXPLAIN (FORMAT TEXT, COSTS ON, BUFFERS ON) 
                         SELECT * FROM memories m 
                         WHERE m.tier = ''working'' AND m.status = ''active'' 
                         ORDER BY calculate_combined_score(m.recency_score, m.importance_score, m.relevance_score, 0.333, 0.333, 0.334) DESC 
                         LIMIT 50'
                    ) AS t(explain_result TEXT)
                ) AS explain_query
            ) AS plan_query
        ) AS parsed_plan
        WHERE plan_text IS NOT NULL AND plan_text != ''
        LIMIT 1
    );
END;
$$ LANGUAGE plpgsql;

-- Create reporting function for benchmark results
CREATE OR REPLACE FUNCTION generate_performance_report()
RETURNS TEXT AS $$
DECLARE
    report_text TEXT := '';
    benchmark_results RECORD;
    working_tier_improvement FLOAT;
    total_tests INTEGER := 0;
    passed_tests INTEGER := 0;
BEGIN
    report_text := report_text || E'# Combined Score Performance Benchmark Report\n';
    report_text := report_text || E'Generated on: ' || NOW()::TEXT || E'\n\n';
    report_text := report_text || E'## Performance Requirements\n';
    report_text := report_text || E'- Working Memory: P99 < 1ms\n';
    report_text := report_text || E'- Cross-tier queries: P99 < 100ms\n\n';
    report_text := report_text || E'## Benchmark Results\n\n';
    report_text := report_text || E'| Test | Query Type | Avg (ms) | Min (ms) | Max (ms) | P95 (ms) | P99 (ms) | Rows | SLA Met |\n';
    report_text := report_text || E'|------|------------|----------|----------|----------|----------|----------|------|----------|\n';
    
    -- Get benchmark results
    FOR benchmark_results IN SELECT * FROM run_combined_score_benchmark() ORDER BY test_name LOOP
        total_tests := total_tests + 1;
        IF benchmark_results.meets_sla THEN
            passed_tests := passed_tests + 1;
        END IF;
        
        report_text := report_text || E'| ' || benchmark_results.test_name || 
                      E' | ' || benchmark_results.query_type ||
                      E' | ' || ROUND(benchmark_results.avg_execution_time_ms::NUMERIC, 3)::TEXT ||
                      E' | ' || ROUND(benchmark_results.min_execution_time_ms::NUMERIC, 3)::TEXT ||
                      E' | ' || ROUND(benchmark_results.max_execution_time_ms::NUMERIC, 3)::TEXT ||
                      E' | ' || ROUND(benchmark_results.p95_execution_time_ms::NUMERIC, 3)::TEXT ||
                      E' | ' || ROUND(benchmark_results.p99_execution_time_ms::NUMERIC, 3)::TEXT ||
                      E' | ' || benchmark_results.rows_processed::TEXT ||
                      E' | ' || CASE WHEN benchmark_results.meets_sla THEN '✅' ELSE '❌' END ||
                      E' |\n';
    END LOOP;
    
    report_text := report_text || E'\n## Summary\n';
    report_text := report_text || E'- Tests Passed: ' || passed_tests || '/' || total_tests || E'\n';
    report_text := report_text || E'- Success Rate: ' || ROUND((passed_tests::FLOAT / total_tests::FLOAT * 100)::NUMERIC, 1) || E'%\n\n';
    
    IF passed_tests = total_tests THEN
        report_text := report_text || E'✅ **All performance requirements met!**\n';
        report_text := report_text || E'The generated combined_score column successfully achieves P99 <1ms latency for working memory queries.\n\n';
    ELSE
        report_text := report_text || E'❌ **Performance requirements not fully met.**\n';
        report_text := report_text || E'Additional optimization may be required.\n\n';
    END IF;
    
    report_text := report_text || E'## Recommendations\n';
    report_text := report_text || E'1. Monitor query performance in production\n';
    report_text := report_text || E'2. Consider adjusting postgresql.conf settings for optimal performance\n';
    report_text := report_text || E'3. Regularly update table statistics with ANALYZE\n';
    report_text := report_text || E'4. Monitor index usage and effectiveness\n';
    
    RETURN report_text;
END;
$$ LANGUAGE plpgsql;

COMMIT;

-- Run the benchmark and display results
-- Note: Uncomment the following lines to execute the benchmark
-- SELECT * FROM run_combined_score_benchmark();
-- SELECT generate_performance_report();

-- Performance monitoring queries for ongoing validation
-- These can be run regularly to ensure performance stays within SLA

-- Query 1: Check current performance of working tier queries
/*
EXPLAIN (ANALYZE, BUFFERS, FORMAT TEXT) 
SELECT * FROM memories m 
WHERE m.tier = 'working' AND m.status = 'active' 
ORDER BY m.combined_score DESC 
LIMIT 50;
*/

-- Query 2: Validate index usage
/*
SELECT 
    schemaname,
    tablename,
    indexname,
    idx_scan as index_scans,
    idx_tup_read as tuples_read,
    idx_tup_fetch as tuples_fetched
FROM pg_stat_user_indexes 
WHERE tablename = 'memories' 
AND indexname LIKE '%combined_score%'
ORDER BY idx_scan DESC;
*/

-- Query 3: Check combined_score distribution
/*
SELECT 
    tier,
    COUNT(*) as total_memories,
    AVG(combined_score) as avg_score,
    MIN(combined_score) as min_score,
    MAX(combined_score) as max_score,
    STDDEV(combined_score) as score_stddev
FROM memories 
WHERE status = 'active' AND combined_score IS NOT NULL
GROUP BY tier
ORDER BY tier;
*/

-- Usage Instructions:
-- 1. Run this script to create benchmark functions
-- 2. Execute: SELECT * FROM run_combined_score_benchmark();
-- 3. Execute: SELECT generate_performance_report();
-- 4. Use the monitoring queries above for ongoing performance validation