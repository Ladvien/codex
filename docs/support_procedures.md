# Support Procedures - Agentic Memory System

## Overview

This document outlines support procedures for the Agentic Memory System used by Claude Code and Claude Desktop applications.

## Table of Contents

1. [Incident Response](#incident-response)
2. [Common Issues](#common-issues)
3. [Monitoring and Alerts](#monitoring-and-alerts)
4. [Performance Troubleshooting](#performance-troubleshooting)
5. [Data Recovery](#data-recovery)
6. [Escalation Procedures](#escalation-procedures)
7. [Maintenance Windows](#maintenance-windows)

## Incident Response

### Severity Levels

#### P0 - Critical (< 15 minutes response)
- Complete system outage
- Data corruption
- Security breach
- Memory system unavailable for all users

#### P1 - High (< 1 hour response)  
- Significant performance degradation (>5s response times)
- Search functionality not working
- Memory creation failing for >50% of requests
- Single instance failure in multi-instance setup

#### P2 - Medium (< 4 hours response)
- Minor performance issues
- Individual feature not working
- Non-critical error rates elevated

#### P3 - Low (< 24 hours response)
- Minor bugs
- Feature requests
- Documentation issues

### Response Procedures

#### P0 Incident Response

1. **Immediate Actions (0-5 minutes)**
   ```bash
   # Check system health
   curl http://localhost:3333/health
   
   # Check database connectivity
   psql $DATABASE_URL -c "SELECT 1;"
   
   # Check recent logs
   tail -n 100 /var/log/memory-system.log
   ```

2. **Assessment (5-10 minutes)**
   ```bash
   # Check system metrics
   docker stats memory-system-container
   
   # Database connection status
   SELECT count(*) FROM pg_stat_activity WHERE datname = 'memory_db';
   
   # Check disk space
   df -h
   ```

3. **Initial Mitigation (10-15 minutes)**
   ```bash
   # Restart service if needed
   systemctl restart memory-system
   
   # Scale horizontally if possible
   docker-compose up --scale memory-system=3
   
   # Enable read-only mode if database is compromised
   # Set READONLY_MODE=true in environment
   ```

#### P1 Incident Response

1. **Diagnostic Commands**
   ```bash
   # Performance metrics
   htop
   iotop -a
   
   # Database performance
   SELECT * FROM pg_stat_statements ORDER BY total_exec_time DESC LIMIT 10;
   
   # Connection pool status
   SELECT count(*), state FROM pg_stat_activity GROUP BY state;
   ```

2. **Common Fixes**
   ```bash
   # Clear connection pool
   SELECT pg_terminate_backend(pid) FROM pg_stat_activity 
   WHERE state = 'idle in transaction' AND state_change < now() - interval '5 minutes';
   
   # Reindex if needed
   REINDEX INDEX CONCURRENTLY memories_embedding_idx;
   
   # Restart specific components
   docker restart memory-system-embedder
   ```

## Common Issues

### Database Connection Issues

**Symptoms:**
- Connection timeouts
- "Connection pool exhausted" errors
- Slow query responses

**Diagnosis:**
```bash
# Check active connections
SELECT count(*) FROM pg_stat_activity;

# Check long-running queries
SELECT pid, now() - pg_stat_activity.query_start AS duration, query 
FROM pg_stat_activity 
WHERE (now() - pg_stat_activity.query_start) > interval '5 minutes';
```

**Resolution:**
```bash
# Kill long-running queries
SELECT pg_terminate_backend(<pid>);

# Adjust connection limits
ALTER SYSTEM SET max_connections = 200;
SELECT pg_reload_conf();

# Restart connection pool
systemctl restart memory-system
```

### Search Performance Issues

**Symptoms:**
- Search queries taking >10 seconds
- High CPU usage during searches
- Memory usage spikes

**Diagnosis:**
```sql
-- Check index usage
SELECT schemaname, tablename, attname, n_distinct, correlation 
FROM pg_stats WHERE tablename = 'memories';

-- Check slow queries
SELECT query, mean_exec_time, calls, total_exec_time 
FROM pg_stat_statements 
WHERE query LIKE '%memories%' 
ORDER BY mean_exec_time DESC;
```

**Resolution:**
```sql
-- Add missing indexes
CREATE INDEX CONCURRENTLY idx_memories_content_search 
ON memories USING gin(to_tsvector('english', content));

-- Update statistics
ANALYZE memories;

-- Increase work_mem for searches
SET work_mem = '256MB';
```

### Memory Creation Failures

**Symptoms:**
- "Duplicate content" errors
- Embedding generation timeouts
- Content hash validation failures

**Diagnosis:**
```bash
# Check embedding service
curl -X POST http://embedding-service:8080/embed \
  -H "Content-Type: application/json" \
  -d '{"text": "test"}'

# Check content hash duplicates
SELECT content_hash, count(*) FROM memories 
GROUP BY content_hash HAVING count(*) > 1;
```

**Resolution:**
```bash
# Restart embedding service
docker restart embedding-service

# Clear duplicate handling cache
redis-cli FLUSHDB

# Update content hash algorithm if needed
# (requires code deployment)
```

### Memory Coherence Issues

**Symptoms:**
- Inconsistent search results across instances
- Stale data being returned
- Transaction isolation problems

**Diagnosis:**
```sql
-- Check replication lag
SELECT client_addr, state, sent_lsn, write_lsn, flush_lsn, replay_lsn 
FROM pg_stat_replication;

-- Check transaction isolation
SELECT txid_current(), txid_snapshot_xmin(txid_current_snapshot());
```

**Resolution:**
```sql
-- Force synchronous replication
ALTER SYSTEM SET synchronous_commit = 'on';
SELECT pg_reload_conf();

-- Clear application-level cache
# Restart all instances with cache clear flag
```

## Monitoring and Alerts

### Key Metrics to Monitor

1. **System Metrics**
   - CPU usage (alert > 80%)
   - Memory usage (alert > 85%)
   - Disk usage (alert > 90%)
   - Network I/O

2. **Application Metrics**
   - Request latency (P95 > 1000ms)
   - Error rate (> 5%)
   - Memory creation rate
   - Search success rate

3. **Database Metrics**
   - Connection count (alert > 80% of max)
   - Query response time (P95 > 100ms)
   - Index hit ratio (< 95%)
   - Replication lag (> 1MB)

### Alert Configuration Examples

```yaml
# Prometheus alerting rules
groups:
- name: memory-system
  rules:
  - alert: HighLatency
    expr: histogram_quantile(0.95, rate(http_request_duration_seconds_bucket[5m])) > 1.0
    for: 2m
    labels:
      severity: warning
    annotations:
      summary: High request latency detected

  - alert: DatabaseConnectionsHigh
    expr: pg_stat_database_numbackends / pg_settings_max_connections > 0.8
    for: 1m
    labels:
      severity: critical
    annotations:
      summary: Database connection pool near capacity

  - alert: SearchErrorRate
    expr: rate(search_errors_total[5m]) / rate(search_requests_total[5m]) > 0.05
    for: 3m
    labels:
      severity: warning
    annotations:
      summary: High search error rate
```

### Dashboard Queries

```promql
# Request rate
rate(http_requests_total[5m])

# Error rate
rate(http_requests_total{status=~"5.."}[5m]) / rate(http_requests_total[5m])

# Database connections
pg_stat_database_numbackends{datname="memory_db"}

# Memory tier distribution
sum by (tier) (memory_count_by_tier)
```

## Performance Troubleshooting

### Slow Queries

1. **Identify slow queries**
   ```sql
   SELECT query, mean_exec_time, calls, total_exec_time / 1000 as total_seconds
   FROM pg_stat_statements 
   WHERE mean_exec_time > 100 
   ORDER BY mean_exec_time DESC 
   LIMIT 20;
   ```

2. **Analyze query plans**
   ```sql
   EXPLAIN (ANALYZE, BUFFERS) 
   SELECT * FROM memories 
   WHERE embedding <-> $1 < 0.8 
   ORDER BY embedding <-> $1 
   LIMIT 10;
   ```

3. **Fix common issues**
   ```sql
   -- Missing index on frequently searched columns
   CREATE INDEX CONCURRENTLY idx_memories_tier_importance 
   ON memories (tier, importance_score DESC);
   
   -- Improve vector search performance
   CREATE INDEX CONCURRENTLY idx_memories_embedding_hnsw 
   ON memories USING hnsw (embedding vector_cosine_ops);
   ```

### High CPU Usage

1. **Check query patterns**
   ```bash
   # Top CPU consuming queries
   SELECT query, mean_exec_time, calls 
   FROM pg_stat_statements 
   ORDER BY (mean_exec_time * calls) DESC 
   LIMIT 10;
   ```

2. **Optimize settings**
   ```sql
   -- Adjust work memory
   ALTER SYSTEM SET work_mem = '256MB';
   
   -- Enable parallel processing
   ALTER SYSTEM SET max_parallel_workers_per_gather = 4;
   
   SELECT pg_reload_conf();
   ```

### High Memory Usage

1. **Check memory distribution**
   ```bash
   # Process memory usage
   ps aux --sort=-%mem | head -20
   
   # PostgreSQL memory usage
   SELECT name, setting, unit FROM pg_settings 
   WHERE name IN ('shared_buffers', 'work_mem', 'maintenance_work_mem');
   ```

2. **Optimize memory settings**
   ```sql
   -- Adjust PostgreSQL memory
   ALTER SYSTEM SET shared_buffers = '256MB';
   ALTER SYSTEM SET work_mem = '64MB';
   ALTER SYSTEM SET maintenance_work_mem = '512MB';
   
   SELECT pg_reload_conf();
   ```

## Data Recovery

### Backup Verification

```bash
# Verify latest backup
pg_verifybackup /backup/path/latest/

# Test restore in isolated environment
pg_restore -d test_restore_db /backup/path/memory_backup.dump

# Verify data integrity
psql test_restore_db -c "SELECT COUNT(*) FROM memories;"
```

### Point-in-Time Recovery

```bash
# Stop the service
systemctl stop memory-system

# Restore to specific point in time
pg_restore -d memory_db_restored -t "2025-01-15 14:30:00" /backup/path/

# Verify restored data
psql memory_db_restored -c "SELECT MAX(created_at) FROM memories;"

# Switch to restored database
# Update connection string in configuration
```

### Disaster Recovery

1. **Failover to Secondary**
   ```bash
   # Promote standby to primary
   su - postgres -c "pg_ctl promote -D /var/lib/postgresql/data"
   
   # Update DNS or load balancer
   # Point traffic to new primary
   
   # Update application configuration
   export DATABASE_URL="postgresql://user:pass@new-primary:5432/db"
   systemctl restart memory-system
   ```

2. **Data Validation After Recovery**
   ```sql
   -- Check data consistency
   SELECT COUNT(*) FROM memories WHERE tier IS NULL;
   SELECT COUNT(*) FROM memories WHERE content_hash IS NULL;
   
   -- Verify embedding integrity  
   SELECT COUNT(*) FROM memories WHERE embedding IS NOT NULL;
   
   -- Check metadata integrity
   SELECT COUNT(*) FROM memories WHERE metadata IS NOT NULL;
   ```

## Escalation Procedures

### Internal Escalation

1. **Level 1 - Operations Team**
   - Initial triage and common issue resolution
   - Basic system health checks
   - Standard recovery procedures

2. **Level 2 - Engineering Team** 
   - Complex troubleshooting
   - Code-level debugging
   - Performance optimization
   - Schema changes

3. **Level 3 - Architecture Team**
   - System design issues
   - Scalability problems
   - Major incident response
   - Post-incident reviews

### External Escalation

1. **Database Vendor Support**
   - PostgreSQL community support
   - Commercial support contract
   - pgvector extension issues

2. **Cloud Provider Support**
   - Infrastructure issues
   - Network connectivity
   - Managed service problems

3. **Security Team**
   - Data breach incidents
   - Access control issues
   - Compliance violations

### Communication Templates

**Initial Incident Report**
```
Subject: [P1] Memory System - High Latency Detected

Incident ID: INC-2025-001
Severity: P1
Start Time: 2025-01-15 14:30:00 UTC
Status: Investigating

Description: Memory search operations experiencing high latency (>5s)
Impact: 50% of search requests timing out
Current Actions: Investigating database query performance
Next Update: 15:00 UTC

Contact: ops-team@company.com
```

**Resolution Update**
```
Subject: [RESOLVED] INC-2025-001 - Memory System High Latency

Incident ID: INC-2025-001
Status: RESOLVED
Resolution Time: 2025-01-15 15:45:00 UTC
Duration: 1h 15m

Root Cause: Missing index on frequently queried columns
Resolution: Created optimized indexes, performance restored
Post-Incident: PIR scheduled for 2025-01-16 10:00 UTC

Monitoring: Continued monitoring for 24h to ensure stability
```

## Maintenance Windows

### Scheduled Maintenance

**Weekly Maintenance (Sundays 02:00-04:00 UTC)**
- Database statistics update
- Index maintenance
- Log rotation
- Performance metric analysis

**Monthly Maintenance (First Sunday 01:00-05:00 UTC)**
- Software updates
- Database maintenance (VACUUM, REINDEX)
- Backup validation
- Capacity planning review

**Quarterly Maintenance (As needed)**
- Major version updates
- Schema migrations
- Hardware maintenance
- DR testing

### Maintenance Procedures

```bash
#!/bin/bash
# Weekly maintenance script

echo "Starting weekly maintenance at $(date)"

# Update database statistics
psql $DATABASE_URL -c "ANALYZE;"

# Clean up old logs
find /var/log -name "memory-system-*.log" -mtime +30 -delete

# Check and rotate large indexes
psql $DATABASE_URL -c "
SELECT schemaname, tablename, indexname, pg_size_pretty(pg_relation_size(indexname::regclass)) as size
FROM pg_indexes 
JOIN pg_stat_user_indexes USING (schemaname, tablename, indexname)
WHERE pg_relation_size(indexname::regclass) > 1000000000;
"

# Backup validation
pg_verifybackup /backup/latest/

echo "Weekly maintenance completed at $(date)"
```

### Emergency Maintenance

**Triggers for Emergency Maintenance:**
- Critical security patches
- Data corruption detected
- Performance degradation >90%
- Compliance violation

**Emergency Process:**
1. **Immediate notification** (all stakeholders)
2. **Change approval** (expedited process)
3. **Execution** (minimal downtime procedures)
4. **Validation** (comprehensive testing)
5. **Communication** (status updates every 15 minutes)

---

**Document Version:** 1.0  
**Last Updated:** January 2025  
**Review Schedule:** Quarterly  
**Owner:** Operations Team