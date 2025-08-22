# Performance Optimization Indexes Migration Runbook - TICKET-011

## Overview

This runbook provides detailed procedures for applying the performance optimization indexes migration (010_performance_optimization_indexes.sql) to improve query performance by preventing full table scans and N+1 query patterns. The migration implements specialized indexes for high-frequency query patterns in the codex memory system.

## Performance Impact Summary

| Query Pattern | Before Migration | After Migration | Expected Improvement |
|---------------|------------------|-----------------|---------------------|
| Last Accessed Sort | Full table scan | Partial index scan | >80% faster |
| Metadata Filtering | Sequential scan | GIN index lookup | >90% faster |
| Hybrid Vector Search | Multiple index scans | Single composite index | >60% faster |
| Temporal Range Queries | Full table scan | B-tree range scan | >70% faster |
| Cluster Analysis | Multiple joins | Optimized index joins | >50% faster |

## Index Size Projections (REVISED)

**Estimated Storage Requirements:**
- `idx_memories_access_patterns_consolidated`: ~80MB (replaces 2 separate indexes)
- `idx_memories_metadata_optimized`: ~120MB (GIN index on JSONB metadata)
- `idx_memories_embedding_hnsw_optimized`: ~400-800MB (HNSW vector index, 1536-dim)
- `idx_memories_temporal_optimized`: ~60MB (B-tree composite index)
- **Total additional storage**: ~660-1060MB (significantly higher due to HNSW reality)

**⚠️ CRITICAL**: Vector indexes require 4-6x raw data size for 1536-dimensional embeddings

## Pre-Migration Checklist

### Performance Baseline Capture
- [ ] Run `benchmark_common_queries('before_migration_010')` to establish baseline
- [ ] Capture current slow query log statistics
- [ ] Document current connection pool utilization
- [ ] Record average query execution times for target query patterns

### System Resources Verification
- [ ] Verify available disk space (need ~600MB for index creation + overhead)
- [ ] Check `maintenance_work_mem` setting (recommend 2GB minimum)
- [ ] Ensure `max_parallel_maintenance_workers` ≥ 4
- [ ] Verify system has capacity for CONCURRENT index builds

### Staging Environment Testing
- [ ] Apply migration to staging environment
- [ ] Verify all indexes created successfully
- [ ] Run performance benchmarks and validate improvements
- [ ] Test rollback procedures
- [ ] Monitor index bloat and maintenance requirements

## Zero-Downtime Migration Execution

### Phase 1: Pre-Migration Setup (No Impact)
```sql
-- Connect to database
psql -h ${DB_HOST} -U ${DB_ADMIN_USER} -d codex_memory

-- Verify current performance baseline
SELECT * FROM benchmark_common_queries('before_migration_010');

-- Check available disk space
SELECT 
    pg_size_pretty(pg_database_size(current_database())) as current_size,
    pg_size_pretty(pg_total_relation_size('memories')) as memories_table_size;

-- Verify memory settings for index builds
SHOW maintenance_work_mem;
SHOW max_parallel_maintenance_workers;
```

### Phase 2: CONCURRENT Index Creation (Minimal Impact)
```bash
# ⚠️ CRITICAL: CONCURRENT indexes cannot run in transactions
# The migration script handles this correctly by running each CONCURRENT operation separately

# Apply the migration
psql -h ${DB_HOST} -U ${DB_ADMIN_USER} -d codex_memory \
  -f migration/migrations/010_performance_optimization_indexes.sql

# Monitor index creation progress (especially for large HNSW index)
psql -h ${DB_HOST} -U ${DB_ADMIN_USER} -d codex_memory \
  -c "SELECT 
        now()::time(0) as current_time,
        a.query,
        p.phase,
        round(p.blocks_done::numeric * 100 / p.blocks_total, 2) AS percent_done
      FROM pg_stat_progress_create_index p 
      JOIN pg_stat_activity a ON p.pid = a.pid;"

# Check final index sizes
psql -h ${DB_HOST} -U ${DB_ADMIN_USER} -d codex_memory \
  -c "SELECT 
        indexname, 
        pg_size_pretty(pg_relation_size(indexrelid)) as size,
        idx_tup_read,
        idx_tup_fetch
      FROM pg_stat_user_indexes 
      WHERE indexname LIKE 'idx_memories_%_consolidated' 
         OR indexname LIKE 'idx_memories_%_optimized'
      ORDER BY pg_relation_size(indexrelid) DESC;"
```

**Key Benefits of CONCURRENT Creation:**
- Tables remain fully accessible during index builds
- No blocking of INSERT/UPDATE/DELETE operations
- Automatic failover if index creation encounters issues
- Gradual performance improvement as indexes come online

### Phase 3: Validation and Performance Verification
```sql
-- Verify all indexes were created successfully
SELECT 
    schemaname, 
    tablename, 
    indexname, 
    pg_size_pretty(pg_relation_size(indexrelid)) as index_size
FROM pg_stat_user_indexes 
WHERE indexname LIKE 'idx_memories_%'
   OR indexname LIKE 'idx_summaries_%'  
   OR indexname LIKE 'idx_cluster_%'
   OR indexname LIKE 'idx_migration_%'
ORDER BY pg_relation_size(indexrelid) DESC;

-- Run performance benchmark to measure improvements
SELECT * FROM benchmark_common_queries('after_migration_010');

-- Check query plan improvements
EXPLAIN (ANALYZE, BUFFERS) 
SELECT * FROM memories 
WHERE last_accessed_at IS NOT NULL 
  AND status = 'active'
ORDER BY last_accessed_at DESC 
LIMIT 100;
```

## Index Maintenance Strategy

### Automated Maintenance Monitoring
```sql
-- Check index health and usage patterns
SELECT * FROM index_maintenance_stats
WHERE index_status IN ('UNUSED', 'LOW_EFFICIENCY')
ORDER BY index_size DESC;

-- Get maintenance recommendations
SELECT * FROM generate_index_maintenance_recommendations()
ORDER BY priority DESC, recommendation_type;
```

### Scheduled Maintenance Tasks

**Daily:**
- Monitor index usage statistics
- Check for index bloat on high-write indexes
- Verify query performance maintains targets

**Weekly:**
- Run `ANALYZE` on tables with new indexes
- Review slow query logs for optimization opportunities
- Update index statistics if usage patterns change

**Monthly:**
- Evaluate index effectiveness using performance baselines
- Consider `REINDEX CONCURRENTLY` for bloated indexes
- Review and update maintenance recommendations

## Rollback Procedures

### When to Rollback
- Index creation fails and cannot be retried
- Significant performance degradation (>20% slower queries)
- Disk space exhaustion due to unexpected index sizes
- Critical application errors related to new query plans

### Rollback Execution
```bash
# Execute rollback migration
psql -h ${DB_HOST} -U ${DB_ADMIN_USER} -d codex_memory \
  -f migration/migrations/010_performance_optimization_indexes_rollback.sql

# Verify rollback success
psql -h ${DB_HOST} -U ${DB_ADMIN_USER} -d codex_memory \
  -c "SELECT indexname FROM pg_indexes 
      WHERE indexname LIKE 'idx_memories_%_partial' 
         OR indexname LIKE 'idx_memories_%_hybrid';"
# Should return 0 rows after successful rollback
```

### Post-Rollback Actions
1. Document rollback reason and performance impact
2. Analyze root cause (insufficient resources, query conflicts, etc.)
3. Plan remediation approach (resource scaling, query optimization)
4. Schedule retry with fixes applied

## Performance Monitoring

### Key Metrics to Track

**Index Usage Metrics:**
```sql
-- Track index scan efficiency
SELECT 
    indexname,
    idx_scan as scans,
    idx_tup_read as tuples_read,
    idx_tup_fetch as tuples_fetched,
    CASE WHEN idx_tup_read > 0 
         THEN (idx_tup_fetch::FLOAT / idx_tup_read * 100)::NUMERIC(5,2) 
         ELSE 0 
    END as hit_ratio_percent
FROM pg_stat_user_indexes 
WHERE indexname LIKE 'idx_memories_%'
ORDER BY idx_scan DESC;
```

**Query Performance Tracking:**
```sql
-- Compare performance before/after migration
SELECT 
    measurement_type,
    query_pattern,
    AVG(execution_time_ms) as avg_time_ms,
    MIN(execution_time_ms) as min_time_ms,
    MAX(execution_time_ms) as max_time_ms,
    COUNT(*) as measurement_count
FROM index_performance_baselines
WHERE query_pattern IN ('last_accessed_sort', 'metadata_filter', 'hybrid_search')
GROUP BY measurement_type, query_pattern
ORDER BY measurement_type, query_pattern;
```

### Alerting Thresholds
- **Critical**: Query execution time >5x baseline average
- **Warning**: Index hit ratio <85% for frequently used indexes
- **Info**: Index size growth >50% week-over-week

## Troubleshooting Guide

### Issue: CONCURRENT Index Creation Fails
```sql
-- Check for blocking transactions
SELECT 
    pid, 
    state, 
    query_start, 
    query
FROM pg_stat_activity 
WHERE state = 'active' 
  AND query LIKE '%CREATE INDEX%';

-- Check for conflicting locks
SELECT 
    blocked_locks.pid AS blocked_pid,
    blocking_locks.pid AS blocking_pid,
    blocked_activity.query AS blocked_query,
    blocking_activity.query AS blocking_query
FROM pg_catalog.pg_locks blocked_locks
JOIN pg_catalog.pg_stat_activity blocked_activity 
  ON blocked_activity.pid = blocked_locks.pid
JOIN pg_catalog.pg_locks blocking_locks 
  ON blocking_locks.locktype = blocked_locks.locktype
 AND blocking_locks.database IS NOT DISTINCT FROM blocked_locks.database;
```

**Solution**: Wait for blocking transactions to complete or restart during low-traffic window

### Issue: Index Size Larger Than Expected
```sql
-- Analyze index bloat and actual vs. estimated sizes
SELECT 
    schemaname,
    tablename, 
    indexname,
    pg_size_pretty(pg_relation_size(indexrelid)) as actual_size,
    (CASE 
        WHEN indexname LIKE '%hnsw%' THEN 'Vector index (expected large)'
        WHEN indexname LIKE '%gin%' THEN 'JSONB index (variable size)'
        ELSE 'B-tree index (predictable size)'
    END) as index_type
FROM pg_stat_user_indexes 
WHERE pg_relation_size(indexrelid) > 100000000  -- >100MB
ORDER BY pg_relation_size(indexrelid) DESC;
```

**Solution**: Review data distribution and consider partial index optimization

### Issue: No Performance Improvement After Migration
```sql
-- Verify indexes are being used in query plans
EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)
SELECT * FROM memories 
WHERE last_accessed_at > NOW() - INTERVAL '1 day'
  AND status = 'active'
ORDER BY last_accessed_at DESC
LIMIT 100;
```

**Solution**: 
1. Run `ANALYZE` on affected tables
2. Check if query patterns match index design
3. Verify query planner statistics are current

## Success Criteria

### Performance Validation
- [ ] Query execution time improvements ≥50% for target patterns
- [ ] Index hit ratios ≥90% for frequently accessed indexes
- [ ] No increase in connection pool wait times
- [ ] Memory usage increase <10% overall

### Operational Validation
- [ ] All indexes created without blocking production traffic  
- [ ] Index maintenance monitoring is functional
- [ ] Performance baselines documented and tracked
- [ ] Rollback procedures tested and validated

### Business Impact Validation
- [ ] User-facing query response times improved
- [ ] System can handle higher concurrent load
- [ ] Database CPU utilization decreased for query processing
- [ ] Storage growth is within projected parameters

## Contact Information

- **Database Team**: [Database Administrator Contact]
- **Performance Team**: [Performance Engineer Contact]
- **On-Call Engineer**: [Primary On-Call Contact]
- **Escalation Path**: [Technical Lead → Engineering Manager → VP Engineering]

---

**Note**: This migration focuses on read performance optimization. Monitor write performance closely during the first 48 hours post-deployment to ensure index maintenance doesn't impact write operations.