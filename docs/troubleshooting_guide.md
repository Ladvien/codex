# Troubleshooting Guide - Agentic Memory System

## Overview

This troubleshooting guide provides systematic approaches to diagnosing and resolving common issues with the Agentic Memory System. It includes step-by-step troubleshooting procedures, common error patterns, and resolution strategies.

## Table of Contents

1. [General Troubleshooting Approach](#general-troubleshooting-approach)
2. [Connection Issues](#connection-issues)
3. [Performance Problems](#performance-problems)
4. [Search and Retrieval Issues](#search-and-retrieval-issues)
5. [Memory Management Problems](#memory-management-problems)
6. [Database Issues](#database-issues)
7. [Authentication and Authorization](#authentication-and-authorization)
8. [Backup and Recovery Issues](#backup-and-recovery-issues)
9. [Integration Problems](#integration-problems)
10. [Monitoring and Alerting Issues](#monitoring-and-alerting-issues)
11. [Common Error Messages](#common-error-messages)
12. [Debug Mode and Logging](#debug-mode-and-logging)

## General Troubleshooting Approach

### 1. Information Gathering

Before diving into specific solutions, gather essential information:

```bash
# System status
systemctl status memory-system
docker ps | grep memory

# Health check
curl -s http://localhost:3333/api/v1/health | jq '.'

# Recent logs
journalctl -u memory-system --since "1 hour ago" | tail -50

# Resource usage
top -p $(pgrep memory-system)
df -h
free -h
```

### 2. Systematic Diagnosis

Follow this hierarchy when troubleshooting:
1. **Network connectivity** (can you reach the service?)
2. **Service health** (is the service running?)
3. **Database connectivity** (can the service reach the database?)
4. **Resource availability** (CPU, memory, disk space)
5. **Configuration correctness** (environment variables, settings)
6. **Application-specific issues** (business logic, data integrity)

### 3. Documentation and Escalation

Always document:
- Symptoms observed
- Steps taken to diagnose
- Solutions attempted
- Final resolution
- Lessons learned

## Connection Issues

### Problem: Cannot Connect to Memory System

**Symptoms:**
- Connection refused errors
- Timeouts when accessing API endpoints
- Service appears down

**Diagnosis Steps:**

```bash
# 1. Check if service is running
systemctl status memory-system
ps aux | grep memory-system

# 2. Check port availability
netstat -tlnp | grep 3333
lsof -i :3333

# 3. Test local connectivity
curl -v http://localhost:3333/api/v1/health

# 4. Check firewall rules
iptables -L | grep 3333
ufw status

# 5. Verify configuration
cat /etc/memory-system/config.toml | grep -A 5 "[server]"
```

**Common Solutions:**

1. **Service not running:**
   ```bash
   systemctl start memory-system
   systemctl enable memory-system
   ```

2. **Port conflict:**
   ```bash
   # Check what's using the port
   lsof -i :3333
   
   # Change port in configuration
   vim /etc/memory-system/config.toml
   # [server]
   # port = 3334
   
   systemctl restart memory-system
   ```

3. **Firewall blocking:**
   ```bash
   # Allow port through firewall
   ufw allow 3333
   iptables -A INPUT -p tcp --dport 3333 -j ACCEPT
   ```

### Problem: Database Connection Issues

**Symptoms:**
- "Database connection failed" errors
- Connection pool exhausted messages
- Slow response times

**Diagnosis Steps:**

```bash
# 1. Test database connectivity directly
psql $DATABASE_URL -c "SELECT 1;"

# 2. Check connection pool status
curl -s http://localhost:3333/metrics | grep pool

# 3. Monitor active connections
psql $DATABASE_URL -c "SELECT count(*) FROM pg_stat_activity WHERE datname = 'memory_db';"

# 4. Check for long-running queries
psql $DATABASE_URL -c "
SELECT pid, now() - pg_stat_activity.query_start AS duration, query 
FROM pg_stat_activity 
WHERE (now() - pg_stat_activity.query_start) > interval '5 minutes';"
```

**Common Solutions:**

1. **Connection pool exhausted:**
   ```bash
   # Increase pool size
   vim /etc/memory-system/config.toml
   # [database]
   # max_connections = 50  # Increase from 20
   
   systemctl restart memory-system
   ```

2. **PostgreSQL not accepting connections:**
   ```bash
   # Check PostgreSQL status
   systemctl status postgresql
   
   # Check PostgreSQL configuration
   sudo -u postgres psql -c "SHOW max_connections;"
   sudo -u postgres psql -c "SHOW shared_buffers;"
   ```

3. **Network connectivity to database:**
   ```bash
   # Test network connectivity
   telnet db-server 5432
   
   # Check DNS resolution
   nslookup db-server
   
   # Verify firewall rules on database server
   ```

## Performance Problems

### Problem: High Response Times

**Symptoms:**
- API responses taking >5 seconds
- User complaints about slow performance
- High CPU usage

**Diagnosis Steps:**

```bash
# 1. Check current performance metrics
curl -s http://localhost:3333/metrics | grep -E "(duration|latency)"

# 2. Monitor system resources
top -p $(pgrep memory-system)
iostat -x 1 5

# 3. Identify slow database queries
psql $DATABASE_URL -c "
SELECT query, calls, total_exec_time, mean_exec_time
FROM pg_stat_statements 
ORDER BY mean_exec_time DESC 
LIMIT 10;"

# 4. Check for blocking queries
psql $DATABASE_URL -c "
SELECT blocked_locks.pid AS blocked_pid,
       blocked_activity.usename AS blocked_user,
       blocking_locks.pid AS blocking_pid,
       blocking_activity.usename AS blocking_user,
       blocked_activity.query AS blocked_statement
FROM pg_catalog.pg_locks blocked_locks
JOIN pg_catalog.pg_stat_activity blocked_activity ON blocked_activity.pid = blocked_locks.pid
JOIN pg_catalog.pg_locks blocking_locks ON blocking_locks.locktype = blocked_locks.locktype
JOIN pg_catalog.pg_stat_activity blocking_activity ON blocking_activity.pid = blocking_locks.pid
WHERE NOT blocked_locks.granted;"
```

**Common Solutions:**

1. **Database query optimization:**
   ```sql
   -- Analyze slow queries
   EXPLAIN (ANALYZE, BUFFERS) SELECT * FROM memories WHERE tier = 'working';
   
   -- Add missing indexes
   CREATE INDEX CONCURRENTLY idx_memories_tier_accessed 
   ON memories (tier, last_accessed_at DESC);
   
   -- Update table statistics
   ANALYZE memories;
   ```

2. **Connection pool optimization:**
   ```bash
   # Increase connection pool
   vim /etc/memory-system/config.toml
   # [database]
   # max_connections = 50
   # idle_timeout_seconds = 300
   
   systemctl restart memory-system
   ```

3. **Memory tier rebalancing:**
   ```sql
   -- Move hot data to working tier
   UPDATE memories 
   SET tier = 'working' 
   WHERE access_count > 100 
   AND tier != 'working' 
   AND last_accessed_at > now() - interval '7 days';
   ```

### Problem: High Memory Usage

**Symptoms:**
- System running out of RAM
- OOM killer terminating processes
- Swap usage increasing

**Diagnosis Steps:**

```bash
# 1. Check memory usage
free -h
ps aux --sort=-%mem | head -20

# 2. Monitor memory trends
vmstat 1 10

# 3. Check for memory leaks
valgrind --leak-check=full ./memory-system 2>&1 | grep "definitely lost"

# 4. Database memory usage
psql $DATABASE_URL -c "
SELECT name, setting, unit 
FROM pg_settings 
WHERE name IN ('shared_buffers', 'work_mem', 'maintenance_work_mem');"
```

**Common Solutions:**

1. **Optimize application memory:**
   ```bash
   # Reduce connection pool size if too high
   vim /etc/memory-system/config.toml
   # [database]
   # max_connections = 20  # Reduce if set too high
   ```

2. **Database memory tuning:**
   ```sql
   -- Reduce work_mem if set too high
   ALTER SYSTEM SET work_mem = '16MB';  -- Down from 256MB
   ALTER SYSTEM SET shared_buffers = '256MB';  -- Appropriate for system RAM
   SELECT pg_reload_conf();
   ```

3. **System-level optimization:**
   ```bash
   # Add swap if needed
   fallocate -l 2G /swapfile
   chmod 600 /swapfile
   mkswap /swapfile
   swapon /swapfile
   ```

## Search and Retrieval Issues

### Problem: Search Returns No Results

**Symptoms:**
- Queries that should return results return empty
- Vector search not working properly
- Text search missing obvious matches

**Diagnosis Steps:**

```bash
# 1. Test basic connectivity
curl -X POST http://localhost:3333/api/v1/search \
  -H "Content-Type: application/json" \
  -d '{"query_text": "test", "limit": 5}'

# 2. Check database for data
psql $DATABASE_URL -c "SELECT COUNT(*) FROM memories WHERE status = 'active';"

# 3. Test embedding service
curl -X POST http://localhost:8080/embed \
  -H "Content-Type: application/json" \
  -d '{"text": "test query"}'

# 4. Check search indexes
psql $DATABASE_URL -c "
SELECT indexname, idx_scan, idx_tup_read, idx_tup_fetch
FROM pg_stat_user_indexes 
WHERE tablename = 'memories';"
```

**Common Solutions:**

1. **Rebuild search indexes:**
   ```sql
   -- Reindex vector search
   REINDEX INDEX CONCURRENTLY idx_memories_embedding;
   
   -- Update statistics
   ANALYZE memories;
   ```

2. **Check embedding service:**
   ```bash
   # Restart embedding service
   systemctl restart embedding-service
   
   # Check service logs
   journalctl -u embedding-service --since "1 hour ago"
   ```

3. **Verify data integrity:**
   ```sql
   -- Check for memories without embeddings
   SELECT COUNT(*) FROM memories WHERE embedding IS NULL AND status = 'active';
   
   -- Regenerate missing embeddings (if needed)
   -- This would be application-specific logic
   ```

### Problem: Search Results Not Relevant

**Symptoms:**
- Search returns results but they seem unrelated
- Vector similarity scores are unexpectedly low
- Text search not matching expected content

**Diagnosis Steps:**

```bash
# 1. Test with known good data
curl -X POST http://localhost:3333/api/v1/search \
  -H "Content-Type: application/json" \
  -d '{"query_text": "specific content you know exists", "limit": 10, "explain_score": true}'

# 2. Check embedding quality
psql $DATABASE_URL -c "
SELECT id, content, 
       array_length(embedding, 1) as embedding_dim,
       tier, importance_score 
FROM memories 
WHERE content ILIKE '%specific content%' 
LIMIT 5;"

# 3. Verify search parameters
# Check if similarity thresholds are too high
# Check if metadata filters are too restrictive
```

**Common Solutions:**

1. **Adjust search parameters:**
   ```json
   {
     "query_text": "your search",
     "similarity_threshold": 0.3,  // Lower threshold
     "limit": 20,  // More results
     "tier": null  // Don't restrict by tier
   }
   ```

2. **Embedding recalibration:**
   ```bash
   # If using custom embedding service, verify it's working correctly
   # May need to regenerate embeddings with updated model
   ```

3. **Search algorithm tuning:**
   ```sql
   -- Adjust hybrid search weights in application configuration
   -- This is typically done in application code, not SQL
   ```

## Memory Management Problems

### Problem: Memory Tier Migration Not Working

**Symptoms:**
- All memories staying in working tier
- Cold tier never populated
- Performance degradation over time

**Diagnosis Steps:**

```bash
# 1. Check tier distribution
psql $DATABASE_URL -c "
SELECT tier, COUNT(*) as count, 
       AVG(access_count) as avg_access,
       MIN(last_accessed_at) as oldest_access
FROM memories 
WHERE status = 'active' 
GROUP BY tier;"

# 2. Check migration configuration
grep -A 10 "tier_migration" /etc/memory-system/config.toml

# 3. Look for migration errors in logs
journalctl -u memory-system | grep -i "migration\|tier"

# 4. Check migration history
psql $DATABASE_URL -c "
SELECT * FROM migration_history 
ORDER BY migrated_at DESC 
LIMIT 20;"
```

**Common Solutions:**

1. **Manual tier migration:**
   ```sql
   -- Move old, rarely accessed memories to warm tier
   UPDATE memories 
   SET tier = 'warm', updated_at = now()
   WHERE tier = 'working' 
     AND last_accessed_at < now() - interval '30 days'
     AND access_count < 5;
   
   -- Move very old memories to cold tier
   UPDATE memories 
   SET tier = 'cold', updated_at = now()
   WHERE tier = 'warm' 
     AND last_accessed_at < now() - interval '90 days';
   ```

2. **Enable automatic migration:**
   ```bash
   # Check if migration service is running
   systemctl status memory-migration-service
   
   # Enable if disabled
   systemctl enable memory-migration-service
   systemctl start memory-migration-service
   ```

3. **Adjust migration thresholds:**
   ```bash
   vim /etc/memory-system/config.toml
   # [tier_migration]
   # working_to_warm_days = 7      # Down from 30
   # warm_to_cold_days = 30        # Down from 90
   # min_access_count = 1          # Down from 10
   ```

### Problem: Memory Limit Exceeded

**Symptoms:**
- "Memory limit exceeded" errors
- Unable to create new memories
- Storage tier full warnings

**Diagnosis Steps:**

```bash
# 1. Check current memory usage by tier
psql $DATABASE_URL -c "
SELECT 
    tier,
    COUNT(*) as memory_count,
    pg_size_pretty(SUM(length(content))) as total_content_size,
    AVG(length(content)) as avg_content_size
FROM memories 
WHERE status = 'active'
GROUP BY tier;"

# 2. Check disk space
df -h /var/lib/postgresql/

# 3. Look for large memories
psql $DATABASE_URL -c "
SELECT id, length(content) as size, tier, created_at
FROM memories 
WHERE status = 'active'
ORDER BY length(content) DESC
LIMIT 20;"
```

**Common Solutions:**

1. **Clean up old memories:**
   ```sql
   -- Delete expired memories
   DELETE FROM memories WHERE expires_at < now();
   
   -- Archive very old, unused memories
   UPDATE memories 
   SET status = 'archived' 
   WHERE tier = 'cold' 
     AND last_accessed_at < now() - interval '1 year';
   ```

2. **Increase storage limits:**
   ```bash
   # Add more disk space
   # Extend filesystem if possible
   # Configure additional storage tiers
   ```

3. **Implement cleanup policies:**
   ```sql
   -- Set up automatic cleanup job
   -- Delete memories older than 2 years with low importance
   DELETE FROM memories 
   WHERE created_at < now() - interval '2 years'
     AND importance_score < 0.2
     AND access_count < 2;
   ```

## Database Issues

### Problem: Database Connection Failures

**Symptoms:**
- "Connection refused" errors
- "Too many connections" messages
- Database timeout errors

**Diagnosis Steps:**

```bash
# 1. Check PostgreSQL status
systemctl status postgresql
ps aux | grep postgres

# 2. Test direct connection
psql $DATABASE_URL -c "SELECT now();"

# 3. Check connection limits
psql $DATABASE_URL -c "
SELECT 
    count(*) as current_connections,
    setting as max_connections
FROM pg_stat_activity, pg_settings 
WHERE name = 'max_connections';"

# 4. Identify connection sources
psql $DATABASE_URL -c "
SELECT client_addr, count(*) as connection_count
FROM pg_stat_activity 
GROUP BY client_addr 
ORDER BY connection_count DESC;"
```

**Common Solutions:**

1. **Increase connection limits:**
   ```sql
   -- As postgres user
   ALTER SYSTEM SET max_connections = 200;
   SELECT pg_reload_conf();
   ```

2. **Kill idle connections:**
   ```sql
   -- Kill long-idle connections
   SELECT pg_terminate_backend(pid)
   FROM pg_stat_activity
   WHERE state = 'idle'
     AND state_change < now() - interval '30 minutes';
   ```

3. **Configure connection pooling:**
   ```bash
   # Install and configure PgBouncer
   apt install pgbouncer
   
   # Configure pool settings
   vim /etc/pgbouncer/pgbouncer.ini
   # pool_mode = transaction
   # default_pool_size = 20
   # max_client_conn = 100
   ```

### Problem: Database Performance Issues

**Symptoms:**
- Slow query execution
- High database CPU usage
- Lock contention warnings

**Diagnosis Steps:**

```bash
# 1. Identify slow queries
psql $DATABASE_URL -c "
SELECT query, calls, total_exec_time / 1000 as total_seconds,
       mean_exec_time / 1000 as mean_seconds
FROM pg_stat_statements 
ORDER BY total_exec_time DESC 
LIMIT 10;"

# 2. Check for locks
psql $DATABASE_URL -c "
SELECT 
    blocked_locks.pid AS blocked_pid,
    blocked_activity.usename AS blocked_user,
    blocking_locks.pid AS blocking_pid,
    blocking_activity.usename AS blocking_user,
    blocked_activity.query AS blocked_statement,
    blocking_activity.query AS current_statement_in_blocking_process
FROM pg_catalog.pg_locks blocked_locks
JOIN pg_catalog.pg_stat_activity blocked_activity ON blocked_activity.pid = blocked_locks.pid
JOIN pg_catalog.pg_locks blocking_locks ON blocking_locks.locktype = blocked_locks.locktype
JOIN pg_catalog.pg_stat_activity blocking_activity ON blocking_activity.pid = blocking_locks.pid
WHERE NOT blocked_locks.granted;"

# 3. Check index usage
psql $DATABASE_URL -c "
SELECT 
    schemaname, tablename, attname, n_distinct, correlation
FROM pg_stats 
WHERE tablename = 'memories';"
```

**Common Solutions:**

1. **Query optimization:**
   ```sql
   -- Add missing indexes
   CREATE INDEX CONCURRENTLY idx_memories_tier_importance 
   ON memories (tier, importance_score DESC);
   
   CREATE INDEX CONCURRENTLY idx_memories_accessed 
   ON memories (last_accessed_at) 
   WHERE status = 'active';
   ```

2. **Database maintenance:**
   ```sql
   -- Update table statistics
   ANALYZE VERBOSE memories;
   
   -- Rebuild fragmented indexes
   REINDEX INDEX CONCURRENTLY idx_memories_embedding;
   
   -- Vacuum to reclaim space
   VACUUM ANALYZE memories;
   ```

3. **Configuration tuning:**
   ```sql
   -- Increase work_mem for complex queries
   ALTER SYSTEM SET work_mem = '64MB';
   
   -- Enable parallel queries
   ALTER SYSTEM SET max_parallel_workers_per_gather = 4;
   
   -- Optimize for SSD storage
   ALTER SYSTEM SET random_page_cost = 1.1;
   
   SELECT pg_reload_conf();
   ```

## Authentication and Authorization

### Problem: Authentication Failures

**Symptoms:**
- "Unauthorized" errors from API
- Valid credentials being rejected
- JWT token validation failures

**Diagnosis Steps:**

```bash
# 1. Test authentication endpoint
curl -v -X POST http://localhost:3333/api/v1/auth \
  -H "Content-Type: application/json" \
  -d '{"username": "test", "password": "test"}'

# 2. Check authentication logs
journalctl -u memory-system | grep -i auth

# 3. Verify JWT configuration
grep -A 5 "jwt" /etc/memory-system/config.toml

# 4. Test with valid token
curl -H "Authorization: Bearer <token>" http://localhost:3333/api/v1/health
```

**Common Solutions:**

1. **JWT token issues:**
   ```bash
   # Check token expiration
   echo "<jwt_token>" | cut -d. -f2 | base64 -d | jq '.exp'
   
   # Generate new token for testing
   ./scripts/generate-test-token.sh
   ```

2. **Secret key rotation:**
   ```bash
   # Update JWT secret
   vim /etc/memory-system/config.toml
   # [auth]
   # jwt_secret = "new-random-secret-key"
   
   systemctl restart memory-system
   ```

3. **User permissions:**
   ```sql
   -- Check user permissions in database
   SELECT username, permissions FROM users WHERE username = 'testuser';
   
   -- Grant necessary permissions
   UPDATE users SET permissions = permissions || '["memory:read", "memory:write"]' 
   WHERE username = 'testuser';
   ```

### Problem: Authorization Failures

**Symptoms:**
- "Forbidden" errors despite authentication
- Users unable to access certain endpoints
- Permission denied messages

**Diagnosis Steps:**

```bash
# 1. Check user permissions
curl -H "Authorization: Bearer <token>" \
  http://localhost:3333/api/v1/user/permissions

# 2. Review authorization logs
journalctl -u memory-system | grep -i "forbidden\|permission"

# 3. Verify role-based access configuration
cat /etc/memory-system/config.toml | grep -A 10 "[rbac]"
```

**Common Solutions:**

1. **Update user roles:**
   ```sql
   -- Check current roles
   SELECT username, roles FROM users WHERE username = 'testuser';
   
   -- Assign required role
   UPDATE users SET roles = roles || '["memory_admin"]' WHERE username = 'testuser';
   ```

2. **Review permission mappings:**
   ```bash
   # Check role permission mappings in configuration
   vim /etc/memory-system/config.toml
   # [rbac]
   # [rbac.roles.memory_admin]
   # permissions = ["memory:read", "memory:write", "memory:delete"]
   ```

## Backup and Recovery Issues

### Problem: Backup Failures

**Symptoms:**
- Backup scripts failing
- Incomplete backup files
- Backup verification errors

**Diagnosis Steps:**

```bash
# 1. Check backup script logs
tail -100 /var/log/backup.log

# 2. Test manual backup
pg_dump $DATABASE_URL > /tmp/test_backup.sql

# 3. Check backup storage space
df -h /backup/

# 4. Verify backup integrity
pg_dump $DATABASE_URL | gzip > /tmp/test.sql.gz
gzip -t /tmp/test.sql.gz
```

**Common Solutions:**

1. **Fix backup permissions:**
   ```bash
   # Ensure backup user has proper permissions
   chown -R backup:backup /backup/
   chmod 755 /backup/
   
   # Grant database backup permissions
   psql $DATABASE_URL -c "GRANT SELECT ON ALL TABLES IN SCHEMA public TO backup_user;"
   ```

2. **Resolve storage issues:**
   ```bash
   # Clean up old backups
   find /backup/ -name "*.sql.gz" -mtime +30 -delete
   
   # Add more storage space
   # Extend filesystem or add new mount point
   ```

3. **Fix backup script:**
   ```bash
   # Check for script errors
   bash -x /usr/local/bin/backup-script.sh
   
   # Update backup configuration
   vim /etc/memory-system/backup.conf
   ```

### Problem: Recovery Failures

**Symptoms:**
- Restore process hangs or fails
- Restored data is inconsistent
- Cannot start service after recovery

**Diagnosis Steps:**

```bash
# 1. Test backup file integrity
pg_restore --list /backup/latest.dump

# 2. Check available disk space
df -h /var/lib/postgresql/

# 3. Verify PostgreSQL is stopped during restore
systemctl status postgresql

# 4. Check restore logs
tail -100 /var/lib/postgresql/data/log/postgresql-*.log
```

**Common Solutions:**

1. **Proper restore procedure:**
   ```bash
   # Stop all services
   systemctl stop memory-system
   systemctl stop postgresql
   
   # Clear data directory
   rm -rf /var/lib/postgresql/data/*
   
   # Initialize new cluster
   sudo -u postgres initdb /var/lib/postgresql/data
   
   # Start PostgreSQL
   systemctl start postgresql
   
   # Restore database
   createdb memory_db
   pg_restore -d memory_db /backup/latest.dump
   ```

2. **Handle restore conflicts:**
   ```bash
   # Drop existing database if needed
   dropdb memory_db
   createdb memory_db
   
   # Restore with error handling
   pg_restore -d memory_db --clean --if-exists /backup/latest.dump
   ```

## Integration Problems

### Problem: Claude Code/Desktop Integration Issues

**Symptoms:**
- Claude applications cannot connect
- MCP protocol errors
- Memory operations failing from Claude

**Diagnosis Steps:**

```bash
# 1. Test MCP endpoint
curl -X POST http://localhost:3333/mcp \
  -H "Content-Type: application/json" \
  -d '{"method": "memory.health", "params": {}}'

# 2. Check MCP server logs
journalctl -u memory-system | grep -i mcp

# 3. Verify protocol compatibility
grep -A 5 "mcp_version" /etc/memory-system/config.toml

# 4. Test memory operations via MCP
curl -X POST http://localhost:3333/mcp \
  -H "Content-Type: application/json" \
  -d '{
    "method": "memory.create", 
    "params": {"content": "test memory", "tier": "Working"}
  }'
```

**Common Solutions:**

1. **MCP protocol version mismatch:**
   ```bash
   # Update to compatible MCP version
   vim /etc/memory-system/config.toml
   # [mcp]
   # version = "1.0.0"
   # compatible_versions = ["1.0.0", "0.9.0"]
   
   systemctl restart memory-system
   ```

2. **Authentication configuration:**
   ```bash
   # Configure Claude application credentials
   # This is typically done in Claude's configuration
   # Check Claude documentation for MCP auth setup
   ```

3. **Network connectivity:**
   ```bash
   # Test connectivity from Claude application host
   telnet memory-system-host 3333
   
   # Check firewall rules
   iptables -L | grep 3333
   ```

### Problem: Embedding Service Integration

**Symptoms:**
- Embedding generation fails
- Vector search not working
- "Embedding service unavailable" errors

**Diagnosis Steps:**

```bash
# 1. Test embedding service directly
curl -X POST http://localhost:8080/embed \
  -H "Content-Type: application/json" \
  -d '{"text": "test content"}'

# 2. Check embedding service status
systemctl status embedding-service

# 3. Verify service configuration
grep -A 5 "embedding" /etc/memory-system/config.toml

# 4. Test with known good input
curl -X POST http://localhost:8080/embed \
  -H "Content-Type: application/json" \
  -d '{"text": "The quick brown fox jumps over the lazy dog"}'
```

**Common Solutions:**

1. **Restart embedding service:**
   ```bash
   systemctl restart embedding-service
   
   # Wait for service to be ready
   sleep 10
   
   # Verify it's responding
   curl http://localhost:8080/health
   ```

2. **Update service configuration:**
   ```bash
   vim /etc/memory-system/config.toml
   # [embedding]
   # service_url = "http://localhost:8080"
   # timeout_seconds = 30
   # retry_attempts = 3
   
   systemctl restart memory-system
   ```

3. **Deploy alternative embedding service:**
   ```bash
   # Switch to backup embedding service
   vim /etc/memory-system/config.toml
   # [embedding]
   # service_url = "http://backup-embedding-service:8080"
   ```

## Monitoring and Alerting Issues

### Problem: Metrics Not Collecting

**Symptoms:**
- Grafana dashboards showing no data
- Prometheus not scraping metrics
- Missing alerts

**Diagnosis Steps:**

```bash
# 1. Test metrics endpoint
curl http://localhost:3333/metrics

# 2. Check Prometheus configuration
curl http://localhost:9090/api/v1/targets

# 3. Verify Prometheus is scraping
curl "http://localhost:9090/api/v1/query?query=up{job='memory-system'}"

# 4. Check Grafana data sources
curl -u admin:admin http://localhost:3000/api/datasources
```

**Common Solutions:**

1. **Fix Prometheus configuration:**
   ```yaml
   # prometheus.yml
   scrape_configs:
     - job_name: 'memory-system'
       static_configs:
         - targets: ['localhost:3333']
       metrics_path: '/metrics'
       scrape_interval: 15s
   ```

2. **Restart monitoring stack:**
   ```bash
   systemctl restart prometheus
   systemctl restart grafana-server
   
   # Verify services are up
   curl http://localhost:9090/
   curl http://localhost:3000/
   ```

3. **Import Grafana dashboards:**
   ```bash
   # Import dashboard via API
   curl -X POST \
     -H "Content-Type: application/json" \
     -u admin:admin \
     http://localhost:3000/api/dashboards/db \
     -d @memory-system-dashboard.json
   ```

### Problem: Alerts Not Firing

**Symptoms:**
- No alerts despite issues
- Alert manager not receiving alerts
- Notification channels not working

**Diagnosis Steps:**

```bash
# 1. Check alert rules
curl http://localhost:9090/api/v1/rules

# 2. Test alert manager
curl http://localhost:9093/api/v1/alerts

# 3. Verify notification channels
curl -u admin:admin http://localhost:3000/api/alert-notifications

# 4. Check alert manager logs
journalctl -u alertmanager --since "1 hour ago"
```

**Common Solutions:**

1. **Fix alert rules:**
   ```yaml
   # alert.rules.yml
   groups:
   - name: memory-system.rules
     rules:
     - alert: MemorySystemDown
       expr: up{job="memory-system"} == 0
       for: 1m
       labels:
         severity: critical
       annotations:
         summary: Memory System is down
   ```

2. **Configure alert manager:**
   ```yaml
   # alertmanager.yml
   global:
     smtp_smarthost: 'localhost:587'
   route:
     group_by: ['alertname']
     receiver: 'web.hook'
   receivers:
   - name: 'web.hook'
     email_configs:
     - to: 'admin@company.com'
       from: 'alerts@company.com'
       subject: 'Alert: {{ .GroupLabels.alertname }}'
   ```

## Common Error Messages

### "Memory not found"

**Error Code:** `MEMORY_NOT_FOUND`
**HTTP Status:** 404

**Possible Causes:**
- Memory ID doesn't exist
- Memory was deleted
- Database connectivity issues

**Resolution:**
```bash
# 1. Verify memory ID format
echo "550e8400-e29b-41d4-a716-446655440000" | grep -E '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'

# 2. Check if memory exists in database
psql $DATABASE_URL -c "SELECT id, status FROM memories WHERE id = '550e8400-e29b-41d4-a716-446655440000';"

# 3. Check for soft-deleted memories
psql $DATABASE_URL -c "SELECT id, status FROM memories WHERE id = '550e8400-e29b-41d4-a716-446655440000' AND status = 'deleted';"
```

### "Content too large"

**Error Code:** `CONTENT_TOO_LARGE`
**HTTP Status:** 413

**Possible Causes:**
- Content exceeds maximum size limit (1MB default)
- Encoding issues causing size inflation

**Resolution:**
```bash
# 1. Check content size
echo "your content here" | wc -c

# 2. Adjust size limits if needed
vim /etc/memory-system/config.toml
# [memory]
# max_content_size = 2097152  # 2MB

# 3. Compress content if possible
# Consider breaking large content into smaller pieces
```

### "Rate limit exceeded"

**Error Code:** `RATE_LIMIT_EXCEEDED`
**HTTP Status:** 429

**Possible Causes:**
- Too many requests from same client
- Rate limit configuration too restrictive

**Resolution:**
```bash
# 1. Check current rate limits
curl -I http://localhost:3333/api/v1/health

# 2. Adjust rate limits
vim /etc/memory-system/config.toml
# [rate_limiting]
# requests_per_hour = 2000  # Increase from 1000

# 3. Implement exponential backoff in client
```

### "Database connection failed"

**Error Code:** `DATABASE_ERROR`
**HTTP Status:** 500

**Possible Causes:**
- PostgreSQL service down
- Connection pool exhausted
- Network connectivity issues

**Resolution:**
```bash
# 1. Check PostgreSQL status
systemctl status postgresql

# 2. Test direct connection
psql $DATABASE_URL -c "SELECT 1;"

# 3. Check connection pool
curl -s http://localhost:3333/metrics | grep pool

# 4. Restart services if needed
systemctl restart postgresql
systemctl restart memory-system
```

## Debug Mode and Logging

### Enabling Debug Mode

```bash
# 1. Set log level to debug
vim /etc/memory-system/config.toml
# [logging]
# level = "debug"

# 2. Restart service
systemctl restart memory-system

# 3. Watch debug logs
journalctl -u memory-system -f | grep DEBUG
```

### Log Analysis

```bash
# 1. Search for specific errors
journalctl -u memory-system | grep -i "error\|fail\|exception"

# 2. Filter by time range
journalctl -u memory-system --since "2024-01-15 10:00" --until "2024-01-15 11:00"

# 3. Export logs for analysis
journalctl -u memory-system --since "1 hour ago" > /tmp/memory-system-logs.txt

# 4. Monitor real-time logs
tail -f /var/log/memory-system.log
```

### Performance Profiling

```bash
# 1. Enable profiling
curl -X POST http://localhost:3333/debug/profile/start

# 2. Run operations to profile
# ... perform operations ...

# 3. Stop profiling and get results
curl http://localhost:3333/debug/profile/stop > profile.json

# 4. Analyze profile
# Use tools like flamegraph to visualize performance bottlenecks
```

This troubleshooting guide should help you quickly diagnose and resolve most common issues with the Agentic Memory System. Remember to always check the basics (service status, connectivity, resources) before diving into complex diagnostics.

For issues not covered in this guide, enable debug logging and contact the development team with detailed logs and reproduction steps.