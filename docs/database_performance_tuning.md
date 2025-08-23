# Database Performance Tuning Guide - HIGH-004 Implementation

## Overview

This guide documents the comprehensive database optimization implemented as part of HIGH-004: Optimize Database Connection Pool Configuration. The optimization targets >1000 operations/second throughput with vector-heavy workloads while maintaining sub-200ms P99 latencies.

## Architecture

### Connection Pool Architecture
```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│   Application   │────▶│    PgBouncer    │────▶│   PostgreSQL    │
│   (2000 conns)  │     │  (Pooling/Mgmt) │     │  (200 max conn) │
│                 │     │                 │     │                 │
│ - Codex Memory  │     │ - Session mgmt  │     │ - Vector ops    │
│ - MCP Server    │     │ - Load balance  │     │ - HNSW indices  │
│ - Admin tools   │     │ - Monitoring    │     │ - Optimized     │
└─────────────────┘     └─────────────────┘     └─────────────────┘
```

## Connection Pool Configuration

### Rust Application Layer
- **Max Connections**: 100 minimum (auto-scales based on demand)
- **Min Connections**: 20 (aggressive pre-warming)
- **Connection Timeout**: 10 seconds (fast failure detection)
- **Idle Timeout**: 5 minutes (prevent resource waste)
- **Max Lifetime**: 1 hour (balance recycling vs overhead)
- **Statement Timeout**: 5 minutes (for vector operations)

### PgBouncer Configuration
- **Max Client Connections**: 2000 (supports all application clients)
- **Default Pool Size**: 100 per database
- **Pool Mode**: Transaction-level pooling
- **Server Connect Timeout**: 15 seconds
- **Query Timeout**: 300 seconds (5 minutes for vector ops)

### PostgreSQL Settings
- **max_connections**: 200 (pool + maintenance connections)
- **Connection management optimized for pooled access**

## Memory Optimization

### PostgreSQL Memory Configuration
Based on 32GB RAM system (adjust proportionally for your hardware):

```sql
-- Core Memory Settings
shared_buffers = 8GB                     -- 25% of total RAM
effective_cache_size = 24GB              -- 75% of total RAM  
work_mem = 256MB                         -- For vector operations
maintenance_work_mem = 2GB               -- Critical for vector index builds
temp_buffers = 128MB                     -- Temporary operations

-- Vector-Specific Optimizations
huge_pages = try                         -- Use huge pages if available
dynamic_shared_memory_type = posix       -- Optimize shared memory
hash_mem_multiplier = 2.0               -- Increase hash table memory
```

### Memory Sizing Guidelines

| System RAM | shared_buffers | effective_cache_size | work_mem | maintenance_work_mem |
|------------|---------------|---------------------|----------|---------------------|
| 16GB       | 4GB           | 12GB               | 128MB    | 1GB                |
| 32GB       | 8GB           | 24GB               | 256MB    | 2GB                |
| 64GB       | 16GB          | 48GB               | 512MB    | 4GB                |
| 128GB      | 32GB          | 96GB               | 1GB      | 8GB                |

## Vector Operation Optimization

### Index Configuration
```sql
-- Create HNSW index for vector similarity search
CREATE INDEX CONCURRENTLY memories_embedding_hnsw_idx 
ON memories USING hnsw (embedding vector_cosine_ops)
WITH (m = 16, ef_construction = 64);

-- Optimize for different vector dimensions
-- For 384-dimensional vectors: m=16, ef_construction=64
-- For 768-dimensional vectors: m=16, ef_construction=128  
-- For 1536-dimensional vectors: m=48, ef_construction=200 (CODEX-006 optimized)
```

### Query Optimization
```sql
-- Optimized vector similarity query with limits
SELECT id, content, embedding <-> $1::vector as distance
FROM memories
WHERE tier = 'working'  -- Filter early
ORDER BY embedding <-> $1::vector
LIMIT 10;

-- Use prepared statements for repeated queries
PREPARE vector_search(vector) AS
SELECT id, content, embedding <-> $1 as distance
FROM memories
ORDER BY embedding <-> $1
LIMIT $2;

-- Execute with: EXECUTE vector_search('[0.1,0.2,...]'::vector, 10);
```

## Performance Monitoring

### Key Metrics to Track

#### Connection Pool Metrics
- **Utilization**: Target <70% normal, <90% peak
- **Wait Time**: Average connection acquisition time
- **Active Connections**: Current connections in use
- **Pool Turnover**: Connection creation/destruction rate

#### Query Performance
- **Slow Queries**: Queries >100ms (log and investigate)
- **Vector Query Latency**: Target <100ms P95, <200ms P99
- **Index Hit Ratio**: Target >95%
- **Cache Hit Ratio**: Target >99% for buffer cache

#### System Resources
- **Memory Usage**: Monitor PostgreSQL and system memory
- **CPU Usage**: Track query processing load
- **I/O Wait**: Monitor disk subsystem performance
- **Network**: Monitor connection and query traffic

### Monitoring Queries
```sql
-- Connection pool status
SELECT 
    state,
    COUNT(*) as connection_count
FROM pg_stat_activity 
GROUP BY state;

-- Long-running queries
SELECT 
    pid,
    now() - pg_stat_activity.query_start AS duration,
    query 
FROM pg_stat_activity 
WHERE state = 'active'
  AND now() - pg_stat_activity.query_start > interval '5 minutes';

-- Index usage statistics
SELECT 
    schemaname,
    tablename,
    attname,
    n_distinct,
    correlation 
FROM pg_stats 
WHERE tablename = 'memories';

-- Vector index performance
SELECT 
    indexrelname,
    idx_scan,
    idx_tup_read,
    idx_tup_fetch
FROM pg_stat_user_indexes
WHERE indexrelname LIKE '%embedding%';
```

## Alert Configuration

### Connection Pool Alerts
- **WARNING**: Pool utilization >70% for >5 minutes
- **CRITICAL**: Pool utilization >90% for >2 minutes
- **WARNING**: Average connection wait time >100ms
- **CRITICAL**: Connection failures >1% of requests

### Performance Alerts  
- **WARNING**: P95 query latency >200ms for vector operations
- **CRITICAL**: P99 query latency >500ms for any operations
- **WARNING**: Slow query count >10/minute
- **CRITICAL**: Database unavailable or connection failures

### Resource Alerts
- **WARNING**: Memory usage >80% of shared_buffers
- **CRITICAL**: Memory usage >95% of shared_buffers  
- **WARNING**: CPU usage >80% sustained for >10 minutes
- **CRITICAL**: Disk usage >90% of available space

## Troubleshooting Guide

### High Connection Pool Utilization
1. **Immediate Actions**:
   - Check for long-running queries: `SELECT * FROM pg_stat_activity WHERE state = 'active'`
   - Look for connection leaks in application logs
   - Verify PgBouncer is functioning correctly

2. **Investigation**:
   - Review application connection management
   - Check for database locks: `SELECT * FROM pg_locks WHERE NOT granted`
   - Monitor query patterns for inefficient operations

3. **Resolution**:
   - Increase pool size temporarily if needed
   - Optimize slow queries
   - Implement connection timeouts
   - Review application connection lifecycle

### Vector Query Performance Issues
1. **Index Problems**:
   ```sql
   -- Check index usage
   SELECT 
       schemaname, tablename, attname, n_distinct, correlation 
   FROM pg_stats 
   WHERE tablename = 'memories' AND attname = 'embedding';
   
   -- Rebuild index if needed
   REINDEX INDEX CONCURRENTLY memories_embedding_hnsw_idx;
   ```

2. **Query Optimization**:
   ```sql
   -- Analyze query plans
   EXPLAIN (ANALYZE, BUFFERS) 
   SELECT id FROM memories 
   ORDER BY embedding <-> '[0.1,0.2,...]'::vector 
   LIMIT 10;
   ```

3. **Memory Issues**:
   - Increase `work_mem` for vector operations
   - Monitor `maintenance_work_mem` during index builds
   - Check for memory pressure in system logs

### Connection Timeouts
1. **Client-side timeouts**: Review application timeout settings
2. **Network issues**: Check network latency and stability
3. **Database locks**: Investigate blocking queries
4. **Resource contention**: Monitor CPU and memory usage

## Load Testing

### Running Performance Tests
```bash
# Set database URL
export DATABASE_URL="postgresql://postgres:password@localhost:5432/codex_memory"

# Run comprehensive load tests
cd /path/to/codex
cargo run --bin load_test_vector_ops

# Expected results for optimized configuration:
# - Vector inserts: >1000 ops/sec
# - Vector searches: >800 ops/sec  
# - Hybrid queries: >600 ops/sec
# - Mixed workload: >1200 ops/sec
```

### Load Test Scenarios
1. **Vector Insert Test**: Pure insert operations with 768-dimensional vectors
2. **Vector Search Test**: Similarity search with HNSW index
3. **Hybrid Query Test**: Combined vector similarity and metadata filtering
4. **Mixed Workload Test**: 40% inserts, 50% searches, 10% updates
5. **Sustained Load Test**: 60-second continuous high-load test

## Maintenance Procedures

### Daily Maintenance
- Monitor connection pool utilization trends
- Review slow query logs
- Check alert notifications
- Verify backup completion

### Weekly Maintenance  
- Analyze query performance trends
- Review index usage statistics
- Check for schema drift or changes
- Update connection pool sizing if needed

### Monthly Maintenance
- Full performance review and optimization
- Update monitoring thresholds based on trends
- Review and update alert configurations
- Capacity planning review

### Quarterly Maintenance
- Major performance tuning review
- Hardware capacity assessment
- Disaster recovery testing
- Security and access review

## Configuration Files

### PostgreSQL Configuration
Location: `config/postgresql.conf`
- Complete memory optimization settings
- Connection and timeout configurations
- Query planner optimizations
- Vector-specific settings

### PgBouncer Configuration  
Location: `config/pgbouncer.ini`
- Connection pooling settings
- Authentication configuration
- Monitoring and logging setup

### Application Configuration
Location: `src/memory/connection.rs`
- Rust connection pool implementation
- Timeout and retry logic
- Health checking and monitoring

## Performance Baselines

### Target Performance (HIGH-004)
- **Throughput**: >1000 operations/second
- **Latency**: P99 <200ms for vector operations
- **Availability**: >99.9% uptime
- **Connection Pool**: <70% utilization under normal load

### Actual Performance (Post-Optimization)
Update these values after running load tests:
- **Vector Inserts**: ___ ops/sec (Target: >1000)
- **Vector Searches**: ___ ops/sec (Target: >800)  
- **Hybrid Queries**: ___ ops/sec (Target: >600)
- **P99 Latency**: ___ms (Target: <200ms)
- **Pool Utilization**: ___% (Target: <70%)

## Security Considerations

### Connection Security
- Use strong authentication (SCRAM-SHA-256)
- Enable SSL/TLS for all connections
- Implement connection limits per user/application
- Regular credential rotation

### Network Security
- Firewall rules limiting database access
- VPN or private network for database connections
- Monitor for unusual connection patterns
- Log all connection attempts

### Access Control
- Principle of least privilege for database users
- Regular access reviews
- Audit logs for all database operations
- Separate read/write access controls

## Future Optimizations

### Potential Improvements
1. **Read Replicas**: Scale read operations across multiple servers
2. **Connection Routing**: Intelligent routing based on query type
3. **Query Caching**: Cache frequent vector similarity queries
4. **Materialized Views**: Pre-compute common query results
5. **Partitioning**: Table partitioning for large datasets

### Scaling Considerations
- Horizontal scaling with read replicas
- Connection pooling at application layer
- Database sharding for extreme scale
- Async query processing for non-critical operations

## Support and Escalation

### Performance Issues
1. **Level 1**: Application team reviews connection usage
2. **Level 2**: Database team investigates query performance  
3. **Level 3**: System administrators check hardware resources
4. **Level 4**: Vendor support for PostgreSQL or pgvector issues

### Emergency Procedures
- Connection pool saturation: Temporary pool size increase
- Database unavailable: Failover to backup instance
- Memory pressure: Query cancellation and resource investigation
- Data corruption: Point-in-time recovery procedures

---

**Last Updated**: 2024-01-22  
**Version**: 1.0 (HIGH-004 Implementation)  
**Next Review**: 2024-02-22