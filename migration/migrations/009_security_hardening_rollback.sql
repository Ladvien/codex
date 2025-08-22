-- Rollback Migration 009: Security Hardening
-- Purpose: Revert security configurations to previous values
-- Requirements: TICKET-010 - Rollback procedures documented

-- ========================================
-- REVERT STATEMENT TIMEOUT CONFIGURATION
-- ========================================
-- Revert statement timeout back to 5 minutes (original postgresql.conf setting)
ALTER DATABASE codex_memory SET statement_timeout = '300s';
SET statement_timeout = '300s';

-- ========================================
-- REVERT IDLE TRANSACTION TIMEOUT
-- ========================================
-- Revert idle transaction timeout back to 10 minutes (original setting)
ALTER DATABASE codex_memory SET idle_in_transaction_session_timeout = '600s';
SET idle_in_transaction_session_timeout = '600s';

-- ========================================
-- REVERT CONNECTION POOL CONSTRAINTS
-- ========================================
-- Reset to default connection limits
ALTER DATABASE codex_memory RESET max_connections;

-- Remove per-role connection limits if they were set
-- ALTER ROLE codex_app_user CONNECTION LIMIT -1;

-- ========================================
-- REVERT QUERY COMPLEXITY LIMITS
-- ========================================
-- Reset work_mem to default
ALTER DATABASE codex_memory RESET work_mem;

-- Reset temp_file_limit to default (unlimited)
ALTER DATABASE codex_memory RESET temp_file_limit;

-- Reset query logging threshold
ALTER DATABASE codex_memory SET log_min_duration_statement = '100ms';

-- ========================================
-- REVERT ADDITIONAL SECURITY SETTINGS
-- ========================================
-- Reset logging settings to defaults
ALTER DATABASE codex_memory RESET log_lock_waits;
ALTER DATABASE codex_memory RESET log_temp_files;

-- Reset timezone (PostgreSQL default will be used)
ALTER DATABASE codex_memory RESET timezone;

-- Reset constraint exclusion
ALTER DATABASE codex_memory RESET constraint_exclusion;

-- ========================================
-- REVERT VACUUM AND MAINTENANCE SETTINGS
-- ========================================
-- Reset autovacuum settings to PostgreSQL defaults
ALTER DATABASE codex_memory RESET autovacuum_vacuum_scale_factor;
ALTER DATABASE codex_memory RESET autovacuum_analyze_scale_factor;

-- Reset maintenance_work_mem to original value
ALTER DATABASE codex_memory SET maintenance_work_mem = '2GB';

-- ========================================
-- REVERT MONITORING SETTINGS
-- ========================================
-- Reset monitoring settings
ALTER DATABASE codex_memory RESET track_activity_query_size;
ALTER DATABASE codex_memory RESET track_functions;

-- ========================================
-- CLEANUP SECURITY MONITORING VIEW
-- ========================================
-- Drop the security monitoring view
DROP VIEW IF EXISTS security_monitoring;

-- ========================================
-- REVERT SECURITY PERMISSIONS
-- ========================================
-- Note: We don't automatically revert schema permissions as they may have
-- been set by other migrations or manual operations. Review manually if needed.

-- ========================================
-- VALIDATION QUERIES
-- ========================================
-- Verify rollback was successful
DO $$
BEGIN
    RAISE NOTICE 'Security hardening rollback completed successfully';
    RAISE NOTICE 'Statement timeout reverted to: %', current_setting('statement_timeout');
    RAISE NOTICE 'Idle transaction timeout reverted to: %', current_setting('idle_in_transaction_session_timeout');
    RAISE NOTICE 'Work mem reverted to: %', current_setting('work_mem');
    
    -- Verify critical settings are back to expected values
    IF current_setting('statement_timeout') != '300s' THEN
        RAISE WARNING 'Statement timeout may not be reverted correctly: %', current_setting('statement_timeout');
    END IF;
    
    IF current_setting('idle_in_transaction_session_timeout') != '600s' THEN
        RAISE WARNING 'Idle transaction timeout may not be reverted correctly: %', current_setting('idle_in_transaction_session_timeout');
    END IF;
END
$$;