-- Migration 014 Rollback: Codex Dreams - Remove Automated Insight Generation Schema
-- Purpose: Safely remove all insights-related tables, indexes, and functions
-- Story: CODEX DREAMS Story 1 - Database Schema Rollback
-- Component: Database, Infrastructure

-- ========================================
-- FEATURE FLAG VALIDATION FOR ROLLBACK
-- ========================================
DO $$
BEGIN
    RAISE NOTICE '🔄 Starting rollback of Codex Dreams insights schema (Migration 014)';
    RAISE NOTICE '⚠️  This will remove all insights data permanently!';
END $$;

-- ========================================
-- DROP TRIGGERS FIRST (to prevent issues during table drops)
-- ========================================

-- Drop insight-related triggers
DROP TRIGGER IF EXISTS update_insights_updated_at ON insights;
DROP TRIGGER IF EXISTS generate_insight_hash ON insights;
DROP TRIGGER IF EXISTS update_feedback_score_on_insert ON insight_feedback;
DROP TRIGGER IF EXISTS update_feedback_score_on_update ON insight_feedback;
DROP TRIGGER IF EXISTS update_feedback_score_on_delete ON insight_feedback;
DROP TRIGGER IF EXISTS validate_queue_status ON processing_queue;

-- ========================================
-- DROP FUNCTIONS
-- ========================================

-- Drop insight-specific functions
DROP FUNCTION IF EXISTS generate_insight_content_hash() CASCADE;
DROP FUNCTION IF EXISTS update_insight_feedback_score() CASCADE;
DROP FUNCTION IF EXISTS validate_processing_queue() CASCADE;
DROP FUNCTION IF EXISTS is_memory_ready_for_reprocessing(UUID, INTEGER) CASCADE;
DROP FUNCTION IF EXISTS mark_memories_as_processed(UUID[], UUID[]) CASCADE;
DROP FUNCTION IF EXISTS get_insights_processing_stats() CASCADE;

-- ========================================
-- DROP INDEXES (in reverse dependency order)
-- ========================================

-- Drop insight-related indexes
DROP INDEX CONCURRENTLY IF EXISTS idx_insights_type_confidence;
DROP INDEX CONCURRENTLY IF EXISTS idx_insights_tier_status;
DROP INDEX CONCURRENTLY IF EXISTS idx_insights_content_hash;
DROP INDEX CONCURRENTLY IF EXISTS idx_insights_tags;
DROP INDEX CONCURRENTLY IF EXISTS idx_insights_source_memories;
DROP INDEX CONCURRENTLY IF EXISTS idx_insights_feedback_score;
DROP INDEX CONCURRENTLY IF EXISTS idx_insights_expires_at;

-- Drop vector indexes
DROP INDEX CONCURRENTLY IF EXISTS idx_insight_vectors_embedding_hnsw;
DROP INDEX CONCURRENTLY IF EXISTS idx_insight_vectors_model;

-- Drop feedback indexes
DROP INDEX CONCURRENTLY IF EXISTS idx_insight_feedback_insight_type;
DROP INDEX CONCURRENTLY IF EXISTS idx_insight_feedback_type_created;

-- Drop processing queue indexes
DROP INDEX CONCURRENTLY IF EXISTS idx_processing_queue_status_priority;
DROP INDEX CONCURRENTLY IF EXISTS idx_processing_queue_retry;
DROP INDEX CONCURRENTLY IF EXISTS idx_processing_queue_memory_ids;

-- Drop memory processing metadata index
DROP INDEX CONCURRENTLY IF EXISTS idx_memories_processing_metadata;

-- ========================================
-- DROP TABLES (in reverse dependency order)
-- ========================================

-- Drop dependent tables first
DROP TABLE IF EXISTS insight_feedback CASCADE;
DROP TABLE IF EXISTS insight_vectors CASCADE;
DROP TABLE IF EXISTS processing_queue CASCADE;

-- Drop main insights table
DROP TABLE IF EXISTS insights CASCADE;

-- ========================================
-- REMOVE COLUMN FROM MEMORIES TABLE
-- ========================================

-- Remove processing_metadata column from memories table
ALTER TABLE memories DROP COLUMN IF EXISTS processing_metadata;

-- ========================================
-- DROP CUSTOM TYPES
-- ========================================

-- Drop insight-related enums
DROP TYPE IF EXISTS insight_type CASCADE;
DROP TYPE IF EXISTS processing_status CASCADE;
DROP TYPE IF EXISTS insight_feedback_type CASCADE;

-- ========================================
-- VALIDATION AND VERIFICATION
-- ========================================

-- Verify all insights tables were removed successfully
DO $$
DECLARE
    table_count INTEGER := 0;
    insight_tables TEXT[] := ARRAY[
        'insights',
        'insight_vectors',
        'insight_feedback', 
        'processing_queue'
    ];
    remaining_tables TEXT[] := '{}';
    table_name TEXT;
    index_count INTEGER := 0;
    function_count INTEGER := 0;
    type_count INTEGER := 0;
BEGIN
    -- Check for remaining insight tables
    FOREACH table_name IN ARRAY insight_tables
    LOOP
        SELECT COUNT(*) INTO table_count
        FROM information_schema.tables 
        WHERE table_name = table_name;
        
        IF table_count > 0 THEN
            remaining_tables := array_append(remaining_tables, table_name);
        END IF;
    END LOOP;
    
    -- Check if processing_metadata column was removed
    SELECT COUNT(*) INTO table_count
    FROM information_schema.columns 
    WHERE table_name = 'memories' 
    AND column_name = 'processing_metadata';
    
    -- Count remaining insight-related indexes
    SELECT COUNT(*) INTO index_count
    FROM pg_indexes 
    WHERE indexname LIKE 'idx_insights_%' 
    OR indexname LIKE 'idx_insight_%' 
    OR indexname LIKE 'idx_processing_%'
    OR indexname = 'idx_memories_processing_metadata';
    
    -- Count remaining insight-related functions
    SELECT COUNT(*) INTO function_count
    FROM information_schema.routines 
    WHERE routine_name IN (
        'generate_insight_content_hash',
        'update_insight_feedback_score',
        'validate_processing_queue',
        'is_memory_ready_for_reprocessing',
        'mark_memories_as_processed',
        'get_insights_processing_stats'
    );
    
    -- Count remaining insight-related types
    SELECT COUNT(*) INTO type_count
    FROM pg_type 
    WHERE typname IN ('insight_type', 'processing_status', 'insight_feedback_type');
    
    -- Report results
    IF array_length(remaining_tables, 1) > 0 THEN
        RAISE WARNING 'Some insight tables were not removed: %', array_to_string(remaining_tables, ', ');
    ELSE
        RAISE NOTICE '✅ All insight tables successfully removed';
    END IF;
    
    IF table_count > 0 THEN
        RAISE WARNING 'processing_metadata column was not removed from memories table';
    ELSE
        RAISE NOTICE '✅ processing_metadata column removed from memories table';
    END IF;
    
    IF index_count > 0 THEN
        RAISE WARNING '% insight-related indexes still remain', index_count;
    ELSE
        RAISE NOTICE '✅ All insight-related indexes removed';
    END IF;
    
    IF function_count > 0 THEN
        RAISE WARNING '% insight-related functions still remain', function_count;
    ELSE
        RAISE NOTICE '✅ All insight-related functions removed';
    END IF;
    
    IF type_count > 0 THEN
        RAISE WARNING '% insight-related types still remain', type_count;
    ELSE
        RAISE NOTICE '✅ All insight-related types removed';
    END IF;
    
    -- Final status
    IF array_length(remaining_tables, 1) = 0 AND table_count = 0 AND 
       index_count = 0 AND function_count = 0 AND type_count = 0 THEN
        RAISE NOTICE '🚀 Migration 014 ROLLBACK COMPLETED: Codex Dreams schema fully removed';
        RAISE NOTICE '✅ Database restored to pre-insights state';
        RAISE NOTICE 'ℹ️  All insights data has been permanently deleted';
    ELSE
        RAISE WARNING '⚠️  Rollback partially completed - some elements may require manual cleanup';
    END IF;
END $$;

-- Final rollback confirmation
SELECT 'Codex Dreams insights schema rollback completed! 🔄' as rollback_result;