-- Migration 009: Security Hardening
-- Purpose: Implement database security configurations to prevent DoS and improve performance
-- Requirements: TICKET-010 - Create Security Fix Migration Scripts

-- ========================================
-- STATEMENT TIMEOUT CONFIGURATION (30s)
-- ========================================
-- Set statement timeout to 30 seconds to prevent runaway queries
-- This setting will be applied at the database level for all connections
ALTER DATABASE codex_memory SET statement_timeout = '30s';

-- For the current session (immediate effect)
SET statement_timeout = '30s';

-- ========================================
-- IDLE TRANSACTION TIMEOUT (60s)  
-- ========================================
-- Set idle transaction timeout to 60 seconds to prevent long-running idle transactions
-- This helps prevent lock holding and connection pool exhaustion
ALTER DATABASE codex_memory SET idle_in_transaction_session_timeout = '60s';

-- For the current session (immediate effect)
SET idle_in_transaction_session_timeout = '60s';

-- ========================================
-- CONNECTION POOL CONSTRAINTS
-- ========================================
-- Set database-level connection limits to work with connection pooling
-- These settings complement the postgresql.conf settings
ALTER DATABASE codex_memory SET max_connections = '200';

-- Set per-role connection limits for application users
-- CREATE ROLE IF NOT EXISTS codex_app_user;
-- ALTER ROLE codex_app_user CONNECTION LIMIT 150;

-- ========================================
-- QUERY COMPLEXITY LIMITS
-- ========================================
-- Set work_mem limit to prevent excessive memory usage per connection
ALTER DATABASE codex_memory SET work_mem = '256MB';

-- Set temp_file_limit to prevent excessive temp file creation
ALTER DATABASE codex_memory SET temp_file_limit = '5GB';

-- Set log_min_duration_statement for query monitoring
ALTER DATABASE codex_memory SET log_min_duration_statement = '1s';

-- ========================================
-- ADDITIONAL SECURITY SETTINGS
-- ========================================
-- Enable slow query logging for monitoring
ALTER DATABASE codex_memory SET log_lock_waits = on;
ALTER DATABASE codex_memory SET log_temp_files = '100MB';

-- Set timezone explicitly for consistency
ALTER DATABASE codex_memory SET timezone = 'UTC';

-- Enable constraint exclusion for partitioned tables (if used)
ALTER DATABASE codex_memory SET constraint_exclusion = 'partition';

-- ========================================
-- VACUUM AND MAINTENANCE SETTINGS
-- ========================================
-- Configure autovacuum settings for better maintenance
ALTER DATABASE codex_memory SET autovacuum_vacuum_scale_factor = 0.1;
ALTER DATABASE codex_memory SET autovacuum_analyze_scale_factor = 0.05;

-- Reduce maintenance_work_mem for routine operations
-- (Large operations can override this as needed)
ALTER DATABASE codex_memory SET maintenance_work_mem = '1GB';

-- ========================================
-- MONITORING AND ALERTING
-- ========================================
-- Ensure pg_stat_statements is available for monitoring
-- (Extension should already be enabled from previous migrations)
ALTER DATABASE codex_memory SET track_activity_query_size = '2048';
ALTER DATABASE codex_memory SET track_functions = 'all';

-- ========================================
-- SECURITY PERMISSIONS
-- ========================================
-- Revoke public schema permissions (if not already done)
-- REVOKE ALL ON SCHEMA public FROM PUBLIC;
-- GRANT USAGE ON SCHEMA public TO codex_app_user;

-- Create security monitoring view
CREATE OR REPLACE VIEW security_monitoring AS
SELECT 
    pid,
    usename,
    application_name,
    client_addr,
    state,
    query_start,
    state_change,
    wait_event_type,
    wait_event,
    EXTRACT(EPOCH FROM (now() - query_start))::int AS query_duration_seconds,
    EXTRACT(EPOCH FROM (now() - state_change))::int AS state_duration_seconds,
    query
FROM pg_stat_activity 
WHERE state != 'idle' 
ORDER BY query_start ASC;

-- Grant select permission on monitoring view
GRANT SELECT ON security_monitoring TO PUBLIC;

-- ========================================
-- VALIDATION QUERIES
-- ========================================
-- Verify settings are applied correctly
DO $$
BEGIN
    -- Check statement timeout
    IF current_setting('statement_timeout') != '30s' THEN
        RAISE EXCEPTION 'Statement timeout not set correctly: %', current_setting('statement_timeout');
    END IF;
    
    -- Check idle transaction timeout
    IF current_setting('idle_in_transaction_session_timeout') != '60s' THEN
        RAISE EXCEPTION 'Idle transaction timeout not set correctly: %', current_setting('idle_in_transaction_session_timeout');
    END IF;
    
    RAISE NOTICE 'Security hardening migration completed successfully';
    RAISE NOTICE 'Statement timeout: %', current_setting('statement_timeout');
    RAISE NOTICE 'Idle transaction timeout: %', current_setting('idle_in_transaction_session_timeout');
    RAISE NOTICE 'Work mem: %', current_setting('work_mem');
END
$$;