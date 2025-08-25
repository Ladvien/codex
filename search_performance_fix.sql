-- Performance fixes for search_memory crashes
-- Based on database analysis findings

-- 1. Create composite index for vector search filtering
-- This prevents full table scans during vector searches
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_memories_search_composite 
ON memories (status, tier, importance_score) 
WHERE embedding IS NOT NULL;

-- 2. Optimize HNSW index parameters for better performance
-- Drop and recreate with optimized parameters
DROP INDEX IF EXISTS memories_embedding_idx;
CREATE INDEX CONCURRENTLY memories_embedding_idx 
ON memories USING hnsw (embedding vector_cosine_ops) 
WITH (m = 24, ef_construction = 128);

-- 3. Add GiST index for better text search performance
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_memories_fulltext_gin
ON memories USING gin (to_tsvector('english', content));

-- 4. Create index for temporal queries
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_memories_temporal_search
ON memories (created_at, status, tier) 
WHERE embedding IS NOT NULL;

-- 5. Add partial index for active memories only
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_memories_active_embedding
ON memories (tier, importance_score, created_at)
WHERE status = 'active' AND embedding IS NOT NULL;

-- Performance monitoring queries to verify improvements
-- Run AFTER creating indexes:

-- Check index usage
-- SELECT schemaname, tablename, indexname, idx_tup_read, idx_tup_fetch 
-- FROM pg_stat_user_indexes WHERE tablename = 'memories';

-- Check table statistics  
-- SELECT schemaname, tablename, n_tup_ins, n_tup_upd, n_tup_del, n_live_tup, n_dead_tup
-- FROM pg_stat_user_tables WHERE tablename = 'memories';

-- Test query performance (should be <100ms after indexes)
-- EXPLAIN ANALYZE SELECT * FROM memories 
-- WHERE status = 'active' AND embedding IS NOT NULL 
-- ORDER BY importance_score DESC LIMIT 10;

COMMIT;