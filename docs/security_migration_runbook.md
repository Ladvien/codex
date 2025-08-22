# Security Migration Runbook - TICKET-010

## Overview

This runbook provides detailed procedures for applying the security hardening migration (009_security_hardening.sql) to production environments. The migration implements database-level security configurations including timeout limits, connection constraints, and query complexity controls.

## Security Changes Summary

| Setting | Before | After | Purpose |
|---------|--------|--------|---------|
| Statement Timeout | 300s (5 min) | 30s | Prevent runaway queries |
| Idle Transaction Timeout | 600s (10 min) | 60s | Prevent connection pool exhaustion |
| Work Memory | 256MB | 256MB (enforced at DB level) | Consistent memory limits |
| Query Logging | 100ms threshold | 1s threshold | Focus on slow queries |
| Connection Limits | 200 (config file) | 200 (database enforced) | Connection pool safety |

## Pre-Migration Checklist

### Performance Baseline
- [ ] Capture current performance metrics
- [ ] Document average query execution times
- [ ] Record connection pool utilization
- [ ] Backup current postgresql.conf settings

### Staging Environment Testing
- [ ] Apply migration to staging environment
- [ ] Run full application test suite
- [ ] Verify no queries timeout under normal load
- [ ] Test rollback procedures
- [ ] Validate monitoring and alerting

### Production Readiness
- [ ] Schedule maintenance window (recommended: low traffic period)
- [ ] Notify development team of new timeout limits
- [ ] Prepare monitoring dashboard for migration impact
- [ ] Ensure DBA approval is obtained

## Zero-Downtime Migration Path

### Step 1: Pre-Migration Preparation (No Downtime)
```bash
# Connect to primary database
psql -h ${DB_HOST} -U ${DB_ADMIN_USER} -d codex_memory

-- Verify current settings
SELECT name, setting, unit FROM pg_settings 
WHERE name IN ('statement_timeout', 'idle_in_transaction_session_timeout');

-- Check active connections
SELECT count(*) as active_connections, state 
FROM pg_stat_activity 
WHERE datname = 'codex_memory' 
GROUP BY state;
```

### Step 2: Apply Migration (Minimal Impact)
```bash
# Apply the security migration
psql -h ${DB_HOST} -U ${DB_ADMIN_USER} -d codex_memory \
  -f migration/migrations/009_security_hardening.sql

# Verify application immediately
# Settings take effect for NEW connections only
```

### Step 3: Gradual Rollout
1. **Immediate Effect**: New connections will use new timeout settings
2. **Existing Connections**: Continue with old settings until they disconnect
3. **Monitoring**: Watch for timeout-related errors in application logs
4. **Rollback Window**: If issues arise, rollback is available

### Step 4: Post-Migration Validation
```bash
# Verify settings are active
SELECT name, setting FROM pg_settings 
WHERE name LIKE '%timeout%' AND setting != 'default';

# Monitor for timeout issues
SELECT * FROM security_monitoring 
WHERE query_duration_seconds > 25;  -- Close to 30s limit

# Check connection pool health
SELECT count(*), state FROM pg_stat_activity 
WHERE datname = 'codex_memory' GROUP BY state;
```

## Rollback Procedures

### When to Rollback
- Application errors due to query timeouts increase >5%
- Connection pool exhaustion occurs
- Critical business processes fail
- Performance degradation >20%

### Rollback Execution
```bash
# Execute rollback migration
psql -h ${DB_HOST} -U ${DB_ADMIN_USER} -d codex_memory \
  -f migration/migrations/009_security_hardening_rollback.sql

# Verify rollback success
SELECT setting FROM pg_settings WHERE name = 'statement_timeout';
-- Should show: 300s

# Restart connection pool to ensure settings take effect
# (Application-specific - may require app restart)
```

### Post-Rollback Actions
1. Document the reason for rollback
2. Analyze root cause (query performance issues, etc.)
3. Create remediation plan
4. Schedule retry with fixes

## Monitoring and Alerting

### Key Metrics to Monitor
```sql
-- Query timeout monitoring
SELECT COUNT(*) as timeout_errors
FROM pg_stat_database 
WHERE datname = 'codex_memory';

-- Connection monitoring
SELECT count(*) as total_connections,
       count(*) FILTER (WHERE state = 'active') as active,
       count(*) FILTER (WHERE state = 'idle in transaction') as idle_in_trans
FROM pg_stat_activity 
WHERE datname = 'codex_memory';

-- Slow query detection
SELECT query, query_start, state_change, 
       EXTRACT(EPOCH FROM (now() - query_start))::int as duration_sec
FROM pg_stat_activity 
WHERE state = 'active' 
  AND query_start < now() - interval '25 seconds';
```

### Alerting Thresholds
- **Critical**: Query duration > 25 seconds (5s before timeout)
- **Warning**: Idle transactions > 50 seconds
- **Info**: Connection count > 160 (80% of max)

## Application Impact Assessment

### Expected Behavior Changes
1. **Long-running queries** will now timeout at 30s instead of 300s
2. **Idle transactions** will be terminated at 60s instead of 600s
3. **Connection pooling** will be more strictly enforced
4. **Query monitoring** will focus on queries >1s instead of >100ms

### Application Modifications Required
- Review and optimize queries taking >25 seconds
- Ensure transactions complete within 60 seconds
- Implement proper connection pool management
- Add timeout handling in application code

## Testing Procedures

### Functional Testing
```sql
-- Test statement timeout
BEGIN;
SELECT pg_sleep(35);  -- Should timeout
ROLLBACK;

-- Test idle transaction timeout  
BEGIN;
-- Wait 65 seconds without activity
-- Connection should be terminated
```

### Performance Testing
```bash
# Load test with new timeouts
pgbench -h ${DB_HOST} -U ${DB_USER} -d codex_memory -T 300 -c 20

# Monitor timeout errors during load test
tail -f /var/log/postgresql/postgresql.log | grep -i timeout
```

## Troubleshooting Guide

### Common Issues

**Issue**: Application queries timing out
```sql
-- Identify slow queries
SELECT query, query_start, 
       EXTRACT(EPOCH FROM (now() - query_start))::int as duration
FROM pg_stat_activity 
WHERE state = 'active' AND query_start < now() - interval '20 seconds';
```
**Solution**: Optimize queries or increase timeout for specific operations

**Issue**: Connection pool exhaustion
```sql
-- Check connection states
SELECT state, count(*) FROM pg_stat_activity 
WHERE datname = 'codex_memory' GROUP BY state;
```
**Solution**: Review application connection management

**Issue**: Idle transaction buildup
```sql
-- Find long-running idle transactions
SELECT pid, usename, query_start, state_change, query
FROM pg_stat_activity 
WHERE state = 'idle in transaction' 
  AND state_change < now() - interval '30 seconds';
```
**Solution**: Fix application transaction management

## Success Criteria

### Performance Validation
- [ ] No increase in application errors
- [ ] Query response times remain within SLA
- [ ] Connection pool utilization stable
- [ ] No timeout-related customer impact

### Security Validation  
- [ ] Statement timeout enforced at 30s
- [ ] Idle transaction timeout enforced at 60s
- [ ] Query complexity limits active
- [ ] Monitoring and alerting functional

### Operational Validation
- [ ] DBA approval obtained
- [ ] Rollback procedures tested
- [ ] Documentation complete
- [ ] Team training completed

## Contact Information

- **DBA Team**: [Insert contact information]
- **Application Team**: [Insert contact information]  
- **On-Call Engineer**: [Insert contact information]
- **Escalation**: [Insert escalation procedures]