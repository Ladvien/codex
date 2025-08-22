-- Rollback Migration 004: Semantic Deduplication Schema
-- This script safely removes all semantic deduplication components
-- Author: codex-memory system
-- Date: 2025-08-22

BEGIN;

-- Drop triggers first
DROP TRIGGER IF EXISTS set_audit_reversibility_trigger ON deduplication_audit_log;

-- Drop functions
DROP FUNCTION IF EXISTS set_reversibility_period();
DROP FUNCTION IF EXISTS get_deduplication_candidates(VARCHAR, INTEGER, INTEGER);
DROP FUNCTION IF EXISTS record_deduplication_metrics(VARCHAR, JSONB, VARCHAR, TIMESTAMP WITH TIME ZONE, TIMESTAMP WITH TIME ZONE);
DROP FUNCTION IF EXISTS cleanup_expired_reversible_operations();
DROP FUNCTION IF EXISTS cleanup_similarity_cache();
DROP FUNCTION IF EXISTS calculate_similarity_hash(TEXT);

-- Remove new columns from memories table
ALTER TABLE memories 
DROP COLUMN IF EXISTS is_merged_result,
DROP COLUMN IF EXISTS original_memory_count,
DROP COLUMN IF EXISTS merge_generation,
DROP COLUMN IF EXISTS compression_applied,
DROP COLUMN IF EXISTS similarity_hash,
DROP COLUMN IF EXISTS deduplication_eligible,
DROP COLUMN IF EXISTS last_deduplication_check;

-- Drop indexes (will be automatically dropped with tables, but explicit for clarity)
DROP INDEX IF EXISTS deduplication_audit_log_operation_type_idx;
DROP INDEX IF EXISTS deduplication_audit_log_created_at_idx;
DROP INDEX IF EXISTS deduplication_audit_log_status_idx;
DROP INDEX IF EXISTS deduplication_audit_log_reversible_idx;
DROP INDEX IF EXISTS memory_merge_history_merged_memory_idx;
DROP INDEX IF EXISTS memory_merge_history_original_memory_idx;
DROP INDEX IF EXISTS memory_merge_history_operation_idx;
DROP INDEX IF EXISTS memory_merge_history_similarity_idx;
DROP INDEX IF EXISTS memory_compression_log_memory_idx;
DROP INDEX IF EXISTS memory_compression_log_type_idx;
DROP INDEX IF EXISTS memory_compression_log_reversible_idx;
DROP INDEX IF EXISTS memory_similarity_cache_memory1_idx;
DROP INDEX IF EXISTS memory_similarity_cache_memory2_idx;
DROP INDEX IF EXISTS memory_similarity_cache_score_idx;
DROP INDEX IF EXISTS memory_similarity_cache_expires_idx;
DROP INDEX IF EXISTS deduplication_metrics_type_recorded_idx;
DROP INDEX IF EXISTS deduplication_metrics_tier_idx;
DROP INDEX IF EXISTS memory_pruning_log_memory_idx;
DROP INDEX IF EXISTS memory_pruning_log_pruned_at_idx;
DROP INDEX IF EXISTS memory_pruning_log_recall_prob_idx;
DROP INDEX IF EXISTS memories_is_merged_result_idx;
DROP INDEX IF EXISTS memories_similarity_hash_idx;
DROP INDEX IF EXISTS memories_deduplication_eligible_idx;
DROP INDEX IF EXISTS memories_last_dedup_check_idx;
DROP INDEX IF EXISTS memories_dedup_candidates_idx;
DROP INDEX IF EXISTS memories_similarity_lookup_idx;

-- Drop tables in dependency order
DROP TABLE IF EXISTS memory_headroom_monitoring;
DROP TABLE IF EXISTS memory_pruning_log;
DROP TABLE IF EXISTS memory_similarity_cache;
DROP TABLE IF EXISTS deduplication_metrics;
DROP TABLE IF EXISTS memory_compression_log;
DROP TABLE IF EXISTS memory_merge_history;
DROP TABLE IF EXISTS deduplication_audit_log;

-- Remove the migration record
DELETE FROM migration_history WHERE migration_name = '004_semantic_deduplication_schema';

COMMIT;