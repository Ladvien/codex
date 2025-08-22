-- Migration 008 Rollback: Remove Missing Database Tables Schema
-- Story: MEDIUM-002 - Create Missing Database Tables (Rollback)
-- Purpose: Remove harvest sessions table and related infrastructure
-- Author: PostgreSQL optimization expert
-- Date: 2025-08-22

BEGIN;

-- Drop triggers first
DROP TRIGGER IF EXISTS harvest_sessions_completion_trigger ON harvest_sessions;
DROP TRIGGER IF EXISTS memory_access_logging_trigger ON memories;

-- Drop functions
DROP FUNCTION IF EXISTS update_harvest_session_completion();
DROP FUNCTION IF EXISTS log_memory_access();
DROP FUNCTION IF EXISTS cleanup_old_harvest_sessions();
DROP FUNCTION IF EXISTS cleanup_old_access_logs();
DROP FUNCTION IF EXISTS archive_old_metrics_snapshots();
DROP FUNCTION IF EXISTS calculate_harvest_success_rate(INTEGER);
DROP FUNCTION IF EXISTS get_tier_migration_stats(INTEGER);
DROP FUNCTION IF EXISTS get_top_harvest_patterns(INTEGER, INTEGER);

-- Drop indexes (in reverse order of creation)
DROP INDEX IF EXISTS memory_access_log_recent_idx;
DROP INDEX IF EXISTS harvest_patterns_pending_idx;
DROP INDEX IF EXISTS harvest_sessions_active_idx;
DROP INDEX IF EXISTS consolidation_events_memory_event_time_idx;
DROP INDEX IF EXISTS harvest_patterns_session_status_type_idx;
DROP INDEX IF EXISTS system_metrics_snapshots_type_recorded_at_idx;
DROP INDEX IF EXISTS memory_access_log_session_id_idx;
DROP INDEX IF EXISTS memory_access_log_access_type_idx;
DROP INDEX IF EXISTS memory_access_log_memory_id_accessed_at_idx;
DROP INDEX IF EXISTS consolidation_events_tier_migration_idx;
DROP INDEX IF EXISTS consolidation_events_event_type_idx;
DROP INDEX IF EXISTS consolidation_events_memory_id_created_at_idx;
DROP INDEX IF EXISTS harvest_patterns_memory_id_idx;
DROP INDEX IF EXISTS harvest_patterns_status_confidence_idx;
DROP INDEX IF EXISTS harvest_patterns_pattern_type_idx;
DROP INDEX IF EXISTS harvest_patterns_session_id_idx;
DROP INDEX IF EXISTS harvest_sessions_completed_at_idx;
DROP INDEX IF EXISTS harvest_sessions_session_type_idx;
DROP INDEX IF EXISTS harvest_sessions_status_started_at_idx;

-- Drop tables (in dependency order)
DROP TABLE IF EXISTS harvest_patterns CASCADE;
DROP TABLE IF EXISTS memory_access_log CASCADE;
DROP TABLE IF EXISTS consolidation_events CASCADE;
DROP TABLE IF EXISTS system_metrics_snapshots CASCADE;
DROP TABLE IF EXISTS harvest_sessions CASCADE;

-- Clean up any migration history entries related to this migration
DELETE FROM migration_history 
WHERE migration_reason LIKE '%008_missing_tables_schema%'
   OR migration_reason LIKE '%harvest sessions%'
   OR migration_reason LIKE '%consolidation events%';

COMMIT;