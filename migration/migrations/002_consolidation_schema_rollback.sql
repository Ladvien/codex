-- Rollback Migration 002: Database Schema Evolution for Consolidation
-- Safely removes consolidation features and restores original schema
-- Author: codex-memory system
-- Date: 2025-08-21

BEGIN;

-- Drop trigger first
DROP TRIGGER IF EXISTS memories_consolidation_trigger ON memories;

-- Drop trigger functions
DROP FUNCTION IF EXISTS trigger_consolidation_update();
DROP FUNCTION IF EXISTS update_consolidation_strength(FLOAT, INTERVAL);
DROP FUNCTION IF EXISTS calculate_recall_probability(FLOAT, FLOAT, INTERVAL);

-- Drop indexes (in reverse order of creation)
DROP INDEX CONCURRENTLY IF EXISTS memories_consolidation_access_idx;
DROP INDEX CONCURRENTLY IF EXISTS memories_tier_recall_prob_idx;
DROP INDEX CONCURRENTLY IF EXISTS memories_last_recall_interval_idx;
DROP INDEX CONCURRENTLY IF EXISTS memories_decay_rate_idx;
DROP INDEX CONCURRENTLY IF EXISTS memories_recall_probability_idx;
DROP INDEX CONCURRENTLY IF EXISTS memories_consolidation_strength_idx;

-- Drop constraints
ALTER TABLE memories DROP CONSTRAINT IF EXISTS check_recall_probability;
ALTER TABLE memories DROP CONSTRAINT IF EXISTS check_decay_rate;
ALTER TABLE memories DROP CONSTRAINT IF EXISTS check_consolidation_strength;

-- Drop new tables
DROP TABLE IF EXISTS memory_tier_statistics;
DROP TABLE IF EXISTS frozen_memories;
DROP TABLE IF EXISTS memory_consolidation_log;

-- Remove new columns from memories table
ALTER TABLE memories 
DROP COLUMN IF EXISTS last_recall_interval,
DROP COLUMN IF EXISTS recall_probability,
DROP COLUMN IF EXISTS decay_rate,
DROP COLUMN IF EXISTS consolidation_strength;

-- Record rollback completion
INSERT INTO migration_history (migration_name, success, completed_at, migration_notes)
VALUES (
    '002_consolidation_schema_rollback',
    true,
    NOW(),
    'Rolled back consolidation schema changes, restored original memory table structure'
);

COMMIT;