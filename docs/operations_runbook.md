# Operations Runbook - Agentic Memory System

## Overview

This runbook provides step-by-step procedures for common operational tasks, maintenance activities, and emergency responses for the Agentic Memory System. It serves as the primary reference for operations teams managing production deployments.

## Table of Contents

1. [Daily Operations](#daily-operations)
2. [System Deployment](#system-deployment)
3. [Database Operations](#database-operations)
4. [Monitoring and Alerting](#monitoring-and-alerting)
5. [Performance Management](#performance-management)
6. [Backup and Recovery](#backup-and-recovery)
7. [Emergency Procedures](#emergency-procedures)
8. [Maintenance Tasks](#maintenance-tasks)
9. [Troubleshooting](#troubleshooting)
10. [Security Operations](#security-operations)

## Daily Operations

### Morning Health Check

Perform these checks every morning to ensure system health:

#### 1. System Status Check
```bash
# Check service status
systemctl status memory-system

# Check Docker containers
docker ps | grep memory-system

# Verify API health
curl -s http://localhost:3333/api/v1/health | jq '.'
```

Expected response:
```json
{
  "status": "healthy",
  "database_connected": true,
  "embedding_service_available": true
}
```

#### 2. Database Health Check
```bash
# Connect to database
psql $DATABASE_URL

# Check connection count
SELECT count(*) FROM pg_stat_activity WHERE datname = 'memory_db';

# Check database size
SELECT pg_size_pretty(pg_database_size('memory_db'));

# Check memory tier distribution
SELECT tier, count(*) FROM memories WHERE status = 'active' GROUP BY tier;
```

#### 3. Performance Metrics Review
```bash
# Check Prometheus metrics endpoint
curl -s http://localhost:3333/metrics | grep memory_operations_total

# Review Grafana dashboards
# - Open Grafana dashboard at http://localhost:3000
# - Check "Memory System Overview" dashboard
# - Verify all metrics are green
```

### Evening Maintenance Check

#### 1. Log Review
```bash
# Check application logs for errors
journalctl -u memory-system --since "1 hour ago" | grep -i error

# Check database logs
tail -100 /var/lib/postgresql/data/log/postgresql-$(date +%Y-%m-%d).log
```

#### 2. Backup Verification
```bash
# Check latest backup status
ls -la /backup/memory_system/ | head -10

# Verify backup integrity
pg_verifybackup /backup/memory_system/$(date +%Y%m%d)
```

## System Deployment

### Production Deployment

#### 1. Pre-deployment Checklist
- [ ] Code reviewed and approved
- [ ] Tests passing in CI/CD pipeline
- [ ] Database migrations tested
- [ ] Rollback plan prepared
- [ ] Maintenance window scheduled
- [ ] Team notified

#### 2. Deployment Steps

```bash
# 1. Create backup before deployment
./scripts/backup.sh --type full --label "pre-deploy-$(date +%Y%m%d-%H%M)"

# 2. Stop traffic (if using load balancer)
# Set load balancer to maintenance mode

# 3. Stop application
systemctl stop memory-system

# 4. Deploy new version
docker pull memory-system:latest
docker-compose up -d --no-deps memory-system

# 5. Run migrations
./scripts/migrate.sh

# 6. Start application
systemctl start memory-system

# 7. Health check
sleep 30
curl -f http://localhost:3333/api/v1/health || exit 1

# 8. Enable traffic
# Remove maintenance mode from load balancer

# 9. Monitor for 15 minutes
watch -n 10 'curl -s http://localhost:3333/api/v1/health | jq ".status"'
```

#### 3. Post-deployment Verification
```bash
# Check application metrics
curl -s http://localhost:3333/metrics | grep -E "(up|memory_operations)"

# Verify database connectivity
psql $DATABASE_URL -c "SELECT COUNT(*) FROM memories;"

# Run smoke tests
./tests/smoke_tests.sh
```

### Rollback Procedure

```bash
# 1. Stop current version
systemctl stop memory-system

# 2. Restore previous version
docker-compose down
docker pull memory-system:previous-stable
docker-compose up -d

# 3. Restore database if needed
# WARNING: Only if database changes were made
pg_restore -d memory_db /backup/memory_system/pre-deploy-backup.dump

# 4. Verify rollback
curl -f http://localhost:3333/api/v1/health

# 5. Update monitoring
# Add annotation to Grafana indicating rollback
```

## Database Operations

### Daily Database Maintenance

#### 1. Connection Monitoring
```sql
-- Monitor active connections
SELECT 
    datname,
    numbackends,
    xact_commit,
    xact_rollback,
    blks_read,
    blks_hit
FROM pg_stat_database 
WHERE datname = 'memory_db';

-- Check for long-running queries
SELECT 
    pid,
    now() - pg_stat_activity.query_start AS duration,
    query,
    state
FROM pg_stat_activity 
WHERE (now() - pg_stat_activity.query_start) > interval '5 minutes';
```

#### 2. Performance Analysis
```sql
-- Top queries by execution time
SELECT 
    query,
    calls,
    total_exec_time,
    mean_exec_time,
    rows
FROM pg_stat_statements 
ORDER BY total_exec_time DESC 
LIMIT 10;

-- Index usage statistics
SELECT 
    indexrelname,
    idx_tup_read,
    idx_tup_fetch,
    idx_scan
FROM pg_stat_user_indexes 
ORDER BY idx_scan DESC;
```

### Weekly Database Maintenance

#### 1. Database Statistics Update
```bash
# Connect to database
psql $DATABASE_URL

# Update table statistics
ANALYZE VERBOSE;

# Check table bloat
SELECT 
    schemaname,
    tablename,
    n_tup_ins,
    n_tup_upd,
    n_tup_del
FROM pg_stat_user_tables 
ORDER BY n_tup_upd + n_tup_del DESC;
```

#### 2. Index Maintenance
```sql
-- Rebuild indexes if needed (during maintenance window)
REINDEX INDEX CONCURRENTLY idx_memories_embedding;
REINDEX INDEX CONCURRENTLY idx_memories_tier_importance;

-- Check for unused indexes
SELECT 
    indexrelname,
    idx_scan,
    pg_size_pretty(pg_relation_size(indexrelname::regclass)) as size
FROM pg_stat_user_indexes 
WHERE idx_scan = 0 
AND pg_relation_size(indexrelname::regclass) > 1000000;
```

### Memory Tier Management

#### 1. Tier Distribution Analysis
```sql
-- Current tier distribution
SELECT 
    tier,
    COUNT(*) as memory_count,
    AVG(importance_score) as avg_importance,
    AVG(access_count) as avg_access_count
FROM memories 
WHERE status = 'active'
GROUP BY tier;

-- Memories eligible for tier migration
SELECT 
    id,
    tier,
    importance_score,
    access_count,
    last_accessed_at,
    EXTRACT(days FROM now() - last_accessed_at) as days_since_access
FROM memories 
WHERE status = 'active' 
AND tier = 'working' 
AND last_accessed_at < now() - interval '7 days'
ORDER BY last_accessed_at;
```

#### 2. Manual Tier Migration
```sql
-- Promote important memories to working tier
UPDATE memories 
SET tier = 'working', updated_at = now()
WHERE importance_score > 0.8 
AND tier != 'working' 
AND access_count > 10;

-- Demote old memories from working to warm
UPDATE memories 
SET tier = 'warm', updated_at = now()
WHERE tier = 'working' 
AND last_accessed_at < now() - interval '30 days'
AND importance_score < 0.6;

-- Archive very old memories to cold tier
UPDATE memories 
SET tier = 'cold', updated_at = now()
WHERE tier = 'warm' 
AND last_accessed_at < now() - interval '90 days';
```

## Monitoring and Alerting

### Setting Up Monitoring

#### 1. Prometheus Configuration
```yaml
# prometheus.yml
scrape_configs:
  - job_name: 'memory-system'
    static_configs:
      - targets: ['localhost:3333']
    scrape_interval: 15s
    metrics_path: /metrics
```

#### 2. Grafana Dashboard Setup
```bash
# Import dashboard
curl -X POST \
  http://admin:admin@localhost:3000/api/dashboards/db \
  -H 'Content-Type: application/json' \
  -d @grafana-dashboard-memory-system.json
```

#### 3. Alert Rules
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
      description: Memory System has been down for more than 1 minute
      
  - alert: HighLatency
    expr: histogram_quantile(0.95, rate(http_request_duration_seconds_bucket[5m])) > 1.0
    for: 2m
    labels:
      severity: warning
    annotations:
      summary: High request latency
      description: 95th percentile latency is above 1 second
      
  - alert: DatabaseConnectionsHigh
    expr: pg_stat_database_numbackends / pg_settings_max_connections > 0.8
    for: 5m
    labels:
      severity: warning
    annotations:
      summary: Database connections high
      description: Database connection usage is above 80%
```

### Alert Response Procedures

#### Critical Alert: System Down
```bash
# Immediate actions
1. Check service status
   systemctl status memory-system
   
2. Check container logs
   docker logs memory-system-container --tail 100
   
3. Restart if necessary
   systemctl restart memory-system
   
4. Verify recovery
   curl -f http://localhost:3333/api/v1/health
   
5. Update incident tracking
   # Create incident ticket
   # Notify team via Slack/PagerDuty
```

#### Warning Alert: High Latency
```bash
# Investigation steps
1. Check database performance
   psql $DATABASE_URL -c "SELECT * FROM pg_stat_statements ORDER BY mean_exec_time DESC LIMIT 5;"
   
2. Check system resources
   top -p $(pgrep memory-system)
   iostat -x 1 5
   
3. Review recent logs
   journalctl -u memory-system --since "10 minutes ago"
   
4. Check for blocking queries
   SELECT pid, query FROM pg_stat_activity WHERE state = 'active';
```

## Performance Management

### Performance Monitoring

#### 1. Key Metrics to Track
```bash
# Request throughput
curl -s http://localhost:3333/metrics | grep memory_operations_total

# Response times
curl -s http://localhost:3333/metrics | grep http_request_duration_seconds

# Error rates
curl -s http://localhost:3333/metrics | grep http_requests_total | grep -v "200"

# Database metrics
psql $DATABASE_URL -c "SELECT * FROM pg_stat_database WHERE datname = 'memory_db';"
```

#### 2. Performance Baselines
- Working Memory Access: <1ms P99
- Warm Storage Query: <100ms P99
- Cold Storage Retrieval: <20s P99
- Memory Creation Rate: >1000/sec
- Search Operations: >500/sec
- Error Rate: <1%

### Performance Optimization

#### 1. Query Optimization
```sql
-- Find slow queries
SELECT 
    query,
    calls,
    total_exec_time / calls as avg_time,
    total_exec_time
FROM pg_stat_statements 
WHERE calls > 100
ORDER BY total_exec_time / calls DESC 
LIMIT 10;

-- Analyze query plans
EXPLAIN (ANALYZE, BUFFERS) 
SELECT * FROM memories 
WHERE tier = 'working' 
ORDER BY importance_score DESC 
LIMIT 10;
```

#### 2. Index Optimization
```sql
-- Check index usage
SELECT 
    t.tablename,
    indexname,
    c.reltuples AS num_rows,
    pg_size_pretty(pg_relation_size(quote_ident(t.tablename)::text)) AS table_size,
    pg_size_pretty(pg_relation_size(quote_ident(indexrelname)::text)) AS index_size,
    CASE WHEN indisunique THEN 'Y' ELSE 'N' END AS unique,
    idx_scan AS number_of_scans,
    idx_tup_read AS tuples_read,
    idx_tup_fetch AS tuples_fetched
FROM pg_tables t
LEFT OUTER JOIN pg_class c ON c.relname = t.tablename
LEFT OUTER JOIN 
    (SELECT 
        c.relname AS ctablename, 
        ipg.relname AS indexname, 
        x.indnatts AS number_of_columns, 
        idx_scan, 
        idx_tup_read, 
        idx_tup_fetch, 
        indexrelname, 
        indisunique 
    FROM pg_index x
    JOIN pg_class c ON c.oid = x.indrelid
    JOIN pg_class ipg ON ipg.oid = x.indexrelid
    JOIN pg_stat_all_indexes psai ON x.indexrelid = psai.indexrelid) 
AS foo ON t.tablename = foo.ctablename
WHERE t.schemaname = 'public'
ORDER BY 1, 2;
```

#### 3. Connection Pool Tuning
```bash
# Edit configuration
vim /etc/memory-system/config.toml

# Adjust pool settings
[database]
max_connections = 50
idle_timeout_seconds = 600
connection_timeout_seconds = 30

# Restart service
systemctl restart memory-system
```

## Backup and Recovery

### Automated Backup Setup

#### 1. Backup Script Configuration
```bash
#!/bin/bash
# /usr/local/bin/backup-memory-system.sh

BACKUP_DIR="/backup/memory_system"
DB_NAME="memory_db"
DATE=$(date +%Y%m%d_%H%M%S)

# Create backup directory
mkdir -p $BACKUP_DIR

# Full backup
pg_dump -h localhost -U postgres $DB_NAME | gzip > $BACKUP_DIR/full_backup_$DATE.sql.gz

# Backup verification
if [ $? -eq 0 ]; then
    echo "Backup successful: $BACKUP_DIR/full_backup_$DATE.sql.gz"
    # Clean up old backups (keep 30 days)
    find $BACKUP_DIR -name "*.sql.gz" -mtime +30 -delete
else
    echo "Backup failed!" | mail -s "Backup Alert" admin@company.com
    exit 1
fi
```

#### 2. Cron Configuration
```bash
# Add to crontab
crontab -e

# Full backup every night at 2 AM
0 2 * * * /usr/local/bin/backup-memory-system.sh

# WAL archiving (continuous)
*/5 * * * * pg_archivecleanup /backup/wal $(pg_controldata /var/lib/postgresql/data | grep "Latest checkpoint's REDO WAL file" | cut -d: -f2 | xargs)
```

### Recovery Procedures

#### 1. Point-in-Time Recovery
```bash
# Stop application
systemctl stop memory-system

# Stop database
systemctl stop postgresql

# Restore base backup
cd /var/lib/postgresql/data
rm -rf *
tar -xzf /backup/memory_system/base_backup_20240115.tar.gz

# Configure recovery
cat > recovery.conf << EOF
restore_command = 'cp /backup/wal/%f %p'
recovery_target_time = '2024-01-15 14:30:00'
EOF

# Start database
systemctl start postgresql

# Monitor recovery
tail -f /var/lib/postgresql/data/log/postgresql-*.log
```

#### 2. Full System Recovery
```bash
# Install system from backup
# 1. Restore application files
tar -xzf /backup/system/memory-system-backup.tar.gz -C /

# 2. Restore database
createdb memory_db
pg_restore -d memory_db /backup/memory_system/full_backup_latest.dump

# 3. Start services
systemctl start postgresql
systemctl start memory-system

# 4. Verify functionality
curl -f http://localhost:3333/api/v1/health
```

## Emergency Procedures

### Service Outage Response

#### 1. Immediate Response (0-5 minutes)
```bash
# Check service status
systemctl status memory-system

# Quick restart attempt
systemctl restart memory-system

# Verify health
curl -s http://localhost:3333/api/v1/health
```

#### 2. Extended Investigation (5-15 minutes)
```bash
# Check logs
journalctl -u memory-system --since "15 minutes ago" | tail -50

# Check system resources
top
df -h
free -h

# Check database
psql $DATABASE_URL -c "SELECT 1;"

# Check docker if applicable
docker ps
docker logs memory-system-container
```

#### 3. Escalation (15+ minutes)
```bash
# Create incident ticket
# Notify on-call engineer
# Execute communication plan
# Consider failover to backup systems
```

### Database Emergency Recovery

#### 1. Database Corruption
```bash
# Check database integrity
pg_dump $DATABASE_URL > /dev/null

# If corruption detected
# 1. Stop application immediately
systemctl stop memory-system

# 2. Assess corruption scope
psql $DATABASE_URL -c "SELECT count(*) FROM memories;" 2>&1

# 3. Restore from latest good backup
pg_restore -d memory_db_recovery /backup/memory_system/verified_backup.dump

# 4. Switch to recovered database
# Update connection string in configuration
```

#### 2. Disk Space Full
```bash
# Immediate space clearing
# Remove old logs
find /var/log -name "*.log" -mtime +7 -delete

# Archive old backups
gzip /backup/memory_system/*.sql
mv /backup/memory_system/*.sql.gz /backup/archive/

# Emergency database cleanup
psql $DATABASE_URL -c "DELETE FROM memories WHERE tier = 'cold' AND last_accessed_at < now() - interval '1 year';"

# Vacuum to reclaim space
psql $DATABASE_URL -c "VACUUM FULL;"
```

## Maintenance Tasks

### Weekly Maintenance Checklist

#### Every Sunday 2:00 AM - 4:00 AM

1. **Database Maintenance**
   ```bash
   # Update statistics
   psql $DATABASE_URL -c "ANALYZE VERBOSE;"
   
   # Rebuild fragmented indexes
   psql $DATABASE_URL -c "REINDEX INDEX CONCURRENTLY idx_memories_embedding;"
   
   # Clean up deleted records
   psql $DATABASE_URL -c "VACUUM memories;"
   ```

2. **Log Rotation**
   ```bash
   # Rotate application logs
   logrotate /etc/logrotate.d/memory-system
   
   # Clean up old logs
   find /var/log -name "memory-system*.log" -mtime +30 -delete
   ```

3. **Backup Verification**
   ```bash
   # Test latest backup
   pg_verifybackup /backup/memory_system/$(ls -t /backup/memory_system/ | head -1)
   
   # Test restore in isolated environment
   ./scripts/test-backup-restore.sh
   ```

### Monthly Maintenance Checklist

#### First Sunday of Month 1:00 AM - 5:00 AM

1. **System Updates**
   ```bash
   # Update system packages
   apt update && apt upgrade -y
   
   # Update Docker images
   docker pull postgres:14
   docker pull memory-system:latest
   ```

2. **Performance Review**
   ```bash
   # Generate performance report
   ./scripts/generate-performance-report.sh
   
   # Review slow queries
   psql $DATABASE_URL -f scripts/slow-query-report.sql > /tmp/slow-queries-$(date +%Y%m).txt
   ```

3. **Capacity Planning**
   ```bash
   # Check storage growth
   df -h
   psql $DATABASE_URL -c "SELECT pg_size_pretty(pg_database_size('memory_db'));"
   
   # Review memory tier distribution trends
   ./scripts/tier-distribution-analysis.sh
   ```

### Quarterly Maintenance Checklist

#### Planned Maintenance Window

1. **Major Version Updates**
   ```bash
   # Test update in staging environment
   # Create full system backup
   # Update application version
   # Run migration scripts
   # Verify functionality
   ```

2. **Security Audit**
   ```bash
   # Review access logs
   # Check for security vulnerabilities
   # Update security certificates
   # Review user permissions
   ```

3. **Disaster Recovery Testing**
   ```bash
   # Full DR drill
   # Test backup restoration
   # Verify failover procedures
   # Update DR documentation
   ```

## Security Operations

### Access Management

#### 1. User Access Review
```bash
# List database users
psql $DATABASE_URL -c "\du"

# Check active API keys
# Review application logs for authentication events
grep "authentication" /var/log/memory-system.log | tail -100
```

#### 2. Security Monitoring
```bash
# Check for suspicious activities
grep -i "failed\|error\|unauthorized" /var/log/memory-system.log | tail -50

# Monitor connection attempts
psql $DATABASE_URL -c "SELECT * FROM pg_stat_activity WHERE state = 'active';"

# Review audit logs
./scripts/generate-audit-report.sh
```

### Certificate Management

#### 1. Certificate Renewal
```bash
# Check certificate expiration
openssl x509 -in /etc/ssl/certs/memory-system.pem -noout -dates

# Renew certificate (Let's Encrypt)
certbot renew --webroot -w /var/www/html

# Restart services to use new certificate
systemctl restart memory-system
systemctl restart nginx
```

#### 2. Security Updates
```bash
# Check for security updates
apt list --upgradable | grep -i security

# Apply security updates
apt update && apt upgrade -y

# Restart affected services
systemctl restart memory-system
```

This operations runbook provides comprehensive procedures for managing the Agentic Memory System in production. Follow these procedures carefully and maintain this document as the system evolves.