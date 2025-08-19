-- Rollback Migration 001: Initial Schema Setup
-- Purpose: Safely remove all schema objects created in migration 001

-- Drop triggers first
DROP TRIGGER IF EXISTS update_memories_updated_at ON memories;
DROP TRIGGER IF EXISTS update_memory_summaries_updated_at ON memory_summaries;
DROP TRIGGER IF EXISTS update_memory_clusters_updated_at ON memory_clusters;
DROP TRIGGER IF EXISTS check_memory_duplicate ON memories;

-- Drop functions
DROP FUNCTION IF EXISTS update_updated_at_column();
DROP FUNCTION IF EXISTS check_content_duplicate();
DROP FUNCTION IF EXISTS track_memory_access();

-- Drop indexes (explicitly to ensure cleanup)
DROP INDEX IF EXISTS idx_memories_working_embedding;
DROP INDEX IF EXISTS idx_memories_working_importance;
DROP INDEX IF EXISTS idx_memories_working_access;
DROP INDEX IF EXISTS idx_memories_warm_embedding;
DROP INDEX IF EXISTS idx_memories_warm_temporal;
DROP INDEX IF EXISTS idx_memories_cold_hash;
DROP INDEX IF EXISTS idx_memories_cold_metadata;
DROP INDEX IF EXISTS idx_memories_parent;
DROP INDEX IF EXISTS idx_memories_expires;
DROP INDEX IF EXISTS idx_memories_status_tier;
DROP INDEX IF EXISTS idx_summaries_embedding;
DROP INDEX IF EXISTS idx_summaries_time_range;
DROP INDEX IF EXISTS idx_summaries_level_time;
DROP INDEX IF EXISTS idx_clusters_embedding;
DROP INDEX IF EXISTS idx_clusters_tags;
DROP INDEX IF EXISTS idx_migration_history_memory;
DROP INDEX IF EXISTS idx_migration_history_time;

-- Drop tables in correct order (respecting foreign key constraints)
DROP TABLE IF EXISTS memory_cluster_mappings CASCADE;
DROP TABLE IF EXISTS memory_summary_mappings CASCADE;
DROP TABLE IF EXISTS migration_history CASCADE;
DROP TABLE IF EXISTS backup_metadata CASCADE;
DROP TABLE IF EXISTS memory_clusters CASCADE;
DROP TABLE IF EXISTS memory_summaries CASCADE;
DROP TABLE IF EXISTS memories CASCADE;

-- Drop custom types
DROP TYPE IF EXISTS memory_status CASCADE;
DROP TYPE IF EXISTS memory_tier CASCADE;

-- Note: We don't drop extensions as they might be used by other schemas
-- DROP EXTENSION IF EXISTS "pgvector";
-- DROP EXTENSION IF EXISTS "uuid-ossp";
-- DROP EXTENSION IF EXISTS "pg_stat_statements";