-- Migration 014: Codex Dreams - Automated Insight Generation Schema
-- Purpose: Create database schema for insights feature including vectors, feedback, and processing queue
-- Story: CODEX DREAMS Story 1 - Database Schema and Migrations for Insights  
-- Priority: High - Foundation Story
-- Feature Flag: codex-dreams
-- Component: Database, Infrastructure

-- ========================================
-- FEATURE FLAG VALIDATION
-- ========================================
-- Check if codex-dreams feature is enabled before proceeding
DO $$
DECLARE
    feature_enabled BOOLEAN := FALSE;
BEGIN
    -- Check environment variable for feature flag
    SELECT COALESCE(
        current_setting('codex.dreams_enabled', true),
        'false'
    )::BOOLEAN INTO feature_enabled;
    
    -- Alternative check via custom function (if implemented)
    BEGIN
        SELECT is_feature_enabled('codex-dreams') INTO feature_enabled;
    EXCEPTION WHEN undefined_function THEN
        -- Feature function not available, check environment
        feature_enabled := COALESCE(
            nullif(pg_getenv('CODEX_DREAMS_ENABLED'), ''),
            'false'
        )::BOOLEAN;
    END;
    
    IF NOT feature_enabled THEN
        RAISE NOTICE 'CODEX-DREAMS feature flag not enabled - skipping insights schema creation';
        RAISE NOTICE 'To enable: SET codex.dreams_enabled = true; or export CODEX_DREAMS_ENABLED=true';
        RETURN;
    END IF;
    
    RAISE NOTICE '🚀 CODEX-DREAMS feature enabled - proceeding with insights schema creation';
END $$;

-- ========================================
-- CREATE INSIGHT TYPES AND ENUMS
-- ========================================

-- Insight type enumeration based on cognitive patterns
CREATE TYPE IF NOT EXISTS insight_type AS ENUM (
    'learning',        -- New knowledge or skill acquired
    'connection',      -- Relationship between concepts discovered
    'relationship',    -- Personal or interpersonal pattern
    'assertion',       -- Strong belief or conviction formed
    'mental_model',    -- Conceptual framework established
    'pattern'          -- Behavioral or temporal pattern recognized
);

-- Processing status for queue management
CREATE TYPE IF NOT EXISTS processing_status AS ENUM (
    'pending',         -- Queued for processing
    'processing',      -- Currently being processed
    'completed',       -- Successfully processed
    'failed'           -- Processing failed
);

-- Feedback enumeration for user rating
CREATE TYPE IF NOT EXISTS insight_feedback_type AS ENUM (
    'helpful',         -- Insight was useful/accurate
    'not_helpful',     -- Insight was not useful
    'incorrect'        -- Insight contains factual errors
);

-- ========================================
-- CORE INSIGHTS TABLE
-- ========================================

-- Primary insights table storing generated insights
CREATE TABLE IF NOT EXISTS insights (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    content TEXT NOT NULL CHECK (char_length(content) >= 10 AND char_length(content) <= 65536),
    content_hash VARCHAR(64) NOT NULL, -- SHA-256 hash for deduplication
    
    -- Insight classification and quality metrics
    insight_type insight_type NOT NULL,
    confidence_score FLOAT NOT NULL DEFAULT 0.5 CHECK (confidence_score >= 0.0 AND confidence_score <= 1.0),
    feedback_score FLOAT DEFAULT 0.0 CHECK (feedback_score >= -1.0 AND feedback_score <= 1.0),
    
    -- Source memories and relationships
    source_memory_ids UUID[] NOT NULL DEFAULT '{}',
    related_insight_ids UUID[] DEFAULT '{}',
    
    -- Metadata and categorization
    metadata JSONB NOT NULL DEFAULT '{}',
    tags TEXT[] NOT NULL DEFAULT '{}',
    
    -- Hierarchical tier management (similar to memories)
    tier memory_tier NOT NULL DEFAULT 'working',
    status memory_status NOT NULL DEFAULT 'active',
    
    -- Version tracking for iterative improvement
    version INTEGER NOT NULL DEFAULT 1,
    previous_version_id UUID REFERENCES insights(id) ON DELETE SET NULL,
    
    -- Temporal tracking
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_accessed_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ,
    
    -- Generation metadata
    generation_model VARCHAR(255),
    generation_prompt_hash VARCHAR(64),
    processing_duration_ms INTEGER,
    
    -- Deduplication constraint
    UNIQUE(content_hash, insight_type),
    
    -- Source memory validation
    CHECK (array_length(source_memory_ids, 1) >= 1), -- At least one source memory
    CHECK (array_length(source_memory_ids, 1) <= 100) -- Reasonable upper bound
);

-- ========================================
-- INSIGHT VECTORS TABLE FOR SEMANTIC SEARCH
-- ========================================

-- Separate table for vector embeddings (performance optimization)
CREATE TABLE IF NOT EXISTS insight_vectors (
    insight_id UUID PRIMARY KEY REFERENCES insights(id) ON DELETE CASCADE,
    embedding vector(1536) NOT NULL, -- Match memory embedding dimensions
    embedding_model VARCHAR(255) NOT NULL DEFAULT 'text-embedding-3-small',
    embedding_created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Performance tracking
    embedding_generation_ms INTEGER,
    
    -- Validation
    CHECK (vector_dims(embedding) = 1536)
);

-- ========================================
-- INSIGHT FEEDBACK TABLE
-- ========================================

-- User feedback for insight quality improvement
CREATE TABLE IF NOT EXISTS insight_feedback (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    insight_id UUID NOT NULL REFERENCES insights(id) ON DELETE CASCADE,
    feedback_type insight_feedback_type NOT NULL,
    
    -- Optional detailed feedback
    feedback_text TEXT CHECK (char_length(feedback_text) <= 2048),
    
    -- User context (if available)
    user_context JSONB DEFAULT '{}',
    
    -- Source tracking
    feedback_source VARCHAR(100) DEFAULT 'mcp_command',
    
    -- Temporal tracking
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ========================================
-- PROCESSING QUEUE TABLE
-- ========================================

-- Queue for batch processing of insights
CREATE TABLE IF NOT EXISTS processing_queue (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    
    -- Processing target
    memory_ids UUID[] NOT NULL,
    processing_type VARCHAR(50) NOT NULL DEFAULT 'insight_generation',
    
    -- Status and priority
    status processing_status NOT NULL DEFAULT 'pending',
    priority INTEGER NOT NULL DEFAULT 5 CHECK (priority >= 1 AND priority <= 10),
    
    -- Retry and error handling
    retry_count INTEGER NOT NULL DEFAULT 0,
    max_retries INTEGER NOT NULL DEFAULT 3,
    error_message TEXT,
    
    -- Batch configuration
    batch_size INTEGER NOT NULL DEFAULT 10 CHECK (batch_size >= 1 AND batch_size <= 100),
    processing_options JSONB DEFAULT '{}',
    
    -- Temporal tracking
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    next_retry_at TIMESTAMPTZ,
    
    -- Processing metadata
    processing_duration_ms INTEGER,
    insights_generated INTEGER DEFAULT 0,
    
    -- Validation
    CHECK (array_length(memory_ids, 1) >= 1),
    CHECK (array_length(memory_ids, 1) <= batch_size),
    CHECK (retry_count <= max_retries),
    CHECK (
        CASE 
            WHEN status = 'completed' THEN completed_at IS NOT NULL
            WHEN status = 'processing' THEN started_at IS NOT NULL
            WHEN status = 'failed' AND retry_count >= max_retries THEN error_message IS NOT NULL
            ELSE true
        END
    )
);

-- ========================================
-- EXTEND MEMORIES TABLE FOR PROCESSING METADATA
-- ========================================

-- Add processing metadata to existing memories table
ALTER TABLE memories ADD COLUMN IF NOT EXISTS processing_metadata JSONB DEFAULT '{}';

-- Add comment for documentation
COMMENT ON COLUMN memories.processing_metadata IS 'Metadata for insight processing including last_processed_at, processing_status, insight_ids_generated, etc.';

-- ========================================
-- CREATE PERFORMANCE OPTIMIZED INDEXES
-- ========================================

-- Set optimal memory for index creation
SET maintenance_work_mem = '4GB';
SET max_parallel_maintenance_workers = 4;

-- Insights table indexes
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_insights_type_confidence 
ON insights (insight_type, confidence_score DESC, created_at DESC) 
WHERE status = 'active';

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_insights_tier_status 
ON insights (tier, status, updated_at DESC);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_insights_content_hash 
ON insights (content_hash) 
WHERE status = 'active';

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_insights_tags 
ON insights USING gin (tags) 
WHERE status = 'active';

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_insights_source_memories 
ON insights USING gin (source_memory_ids) 
WHERE status = 'active';

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_insights_feedback_score 
ON insights (feedback_score DESC, confidence_score DESC) 
WHERE status = 'active' AND feedback_score IS NOT NULL;

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_insights_expires_at 
ON insights (expires_at) 
WHERE expires_at IS NOT NULL AND status = 'active';

-- Vector search optimization (HNSW for similarity search)
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_insight_vectors_embedding_hnsw 
ON insight_vectors USING hnsw (embedding vector_cosine_ops) 
WITH (
    m = 48,                    -- Optimal for 1536-dimensional vectors
    ef_construction = 200      -- Balanced build time vs accuracy
);

-- Additional vector index for exact search if needed
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_insight_vectors_model 
ON insight_vectors (embedding_model, embedding_created_at DESC);

-- Feedback table indexes
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_insight_feedback_insight_type 
ON insight_feedback (insight_id, feedback_type, created_at DESC);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_insight_feedback_type_created 
ON insight_feedback (feedback_type, created_at DESC);

-- Processing queue indexes
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_processing_queue_status_priority 
ON processing_queue (status, priority DESC, created_at ASC) 
WHERE status IN ('pending', 'processing');

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_processing_queue_retry 
ON processing_queue (next_retry_at) 
WHERE status = 'failed' AND retry_count < max_retries;

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_processing_queue_memory_ids 
ON processing_queue USING gin (memory_ids);

-- Memory table processing metadata index
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_memories_processing_metadata 
ON memories USING gin (processing_metadata) 
WHERE processing_metadata IS NOT NULL AND processing_metadata != '{}';

-- ========================================
-- CREATE TRIGGERS AND FUNCTIONS
-- ========================================

-- Update trigger for insights
CREATE TRIGGER update_insights_updated_at 
    BEFORE UPDATE ON insights 
    FOR EACH ROW 
    EXECUTE FUNCTION update_updated_at_column();

-- Content hash generation for insights
CREATE OR REPLACE FUNCTION generate_insight_content_hash()
RETURNS TRIGGER AS $$
BEGIN
    NEW.content_hash = encode(digest(NEW.content || NEW.insight_type::text, 'sha256'), 'hex');
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER generate_insight_hash 
    BEFORE INSERT OR UPDATE OF content, insight_type ON insights 
    FOR EACH ROW 
    EXECUTE FUNCTION generate_insight_content_hash();

-- Insight feedback score calculation
CREATE OR REPLACE FUNCTION update_insight_feedback_score()
RETURNS TRIGGER AS $$
DECLARE
    helpful_count INTEGER := 0;
    not_helpful_count INTEGER := 0;
    incorrect_count INTEGER := 0;
    total_count INTEGER := 0;
    new_score FLOAT := 0.0;
BEGIN
    -- Count feedback types for this insight
    SELECT 
        COUNT(*) FILTER (WHERE feedback_type = 'helpful') as helpful,
        COUNT(*) FILTER (WHERE feedback_type = 'not_helpful') as not_helpful,
        COUNT(*) FILTER (WHERE feedback_type = 'incorrect') as incorrect,
        COUNT(*) as total
    INTO helpful_count, not_helpful_count, incorrect_count, total_count
    FROM insight_feedback 
    WHERE insight_id = COALESCE(NEW.insight_id, OLD.insight_id);
    
    -- Calculate weighted feedback score (-1 to +1)
    -- helpful: +1, not_helpful: -0.5, incorrect: -1
    IF total_count > 0 THEN
        new_score = (
            (helpful_count * 1.0) + 
            (not_helpful_count * -0.5) + 
            (incorrect_count * -1.0)
        ) / total_count;
    END IF;
    
    -- Update the insight
    UPDATE insights 
    SET 
        feedback_score = new_score,
        updated_at = NOW()
    WHERE id = COALESCE(NEW.insight_id, OLD.insight_id);
    
    RETURN COALESCE(NEW, OLD);
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER update_feedback_score_on_insert 
    AFTER INSERT ON insight_feedback 
    FOR EACH ROW 
    EXECUTE FUNCTION update_insight_feedback_score();

CREATE TRIGGER update_feedback_score_on_update 
    AFTER UPDATE ON insight_feedback 
    FOR EACH ROW 
    EXECUTE FUNCTION update_insight_feedback_score();

CREATE TRIGGER update_feedback_score_on_delete 
    AFTER DELETE ON insight_feedback 
    FOR EACH ROW 
    EXECUTE FUNCTION update_insight_feedback_score();

-- Processing queue status validation
CREATE OR REPLACE FUNCTION validate_processing_queue()
RETURNS TRIGGER AS $$
BEGIN
    -- Set started_at when status changes to processing
    IF NEW.status = 'processing' AND OLD.status != 'processing' THEN
        NEW.started_at = NOW();
    END IF;
    
    -- Set completed_at when status changes to completed or failed
    IF NEW.status IN ('completed', 'failed') AND OLD.status NOT IN ('completed', 'failed') THEN
        NEW.completed_at = NOW();
        
        -- Calculate processing duration
        IF NEW.started_at IS NOT NULL THEN
            NEW.processing_duration_ms = EXTRACT(EPOCH FROM (NOW() - NEW.started_at)) * 1000;
        END IF;
    END IF;
    
    -- Set next retry time for failed jobs
    IF NEW.status = 'failed' AND NEW.retry_count < NEW.max_retries THEN
        -- Exponential backoff: 1min, 5min, 25min
        NEW.next_retry_at = NOW() + (POWER(5, NEW.retry_count) * INTERVAL '1 minute');
    END IF;
    
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER validate_queue_status 
    BEFORE UPDATE ON processing_queue 
    FOR EACH ROW 
    EXECUTE FUNCTION validate_processing_queue();

-- ========================================
-- CREATE HELPER FUNCTIONS
-- ========================================

-- Function to check if memories are ready for reprocessing
CREATE OR REPLACE FUNCTION is_memory_ready_for_reprocessing(
    memory_id UUID,
    reprocess_interval_hours INTEGER DEFAULT 24
) RETURNS BOOLEAN AS $$
DECLARE
    last_processed_at TIMESTAMPTZ;
BEGIN
    -- Extract last processing timestamp from metadata
    SELECT (processing_metadata->>'last_processed_at')::TIMESTAMPTZ
    INTO last_processed_at
    FROM memories 
    WHERE id = memory_id;
    
    -- Return true if never processed or older than interval
    RETURN (
        last_processed_at IS NULL OR 
        last_processed_at < (NOW() - (reprocess_interval_hours * INTERVAL '1 hour'))
    );
END;
$$ LANGUAGE plpgsql;

-- Function to mark memories as processed
CREATE OR REPLACE FUNCTION mark_memories_as_processed(
    memory_ids UUID[],
    insight_ids UUID[] DEFAULT '{}'
) RETURNS VOID AS $$
BEGIN
    UPDATE memories 
    SET 
        processing_metadata = processing_metadata || jsonb_build_object(
            'last_processed_at', NOW(),
            'processing_status', 'completed',
            'insight_ids_generated', insight_ids,
            'last_insight_count', array_length(insight_ids, 1)
        ),
        updated_at = NOW()
    WHERE id = ANY(memory_ids);
END;
$$ LANGUAGE plpgsql;

-- Function to get processing statistics
CREATE OR REPLACE FUNCTION get_insights_processing_stats()
RETURNS TABLE(
    total_insights BIGINT,
    insights_by_type JSONB,
    avg_confidence FLOAT,
    avg_feedback_score FLOAT,
    processing_queue_size BIGINT,
    failed_jobs BIGINT,
    top_tags JSONB
) AS $$
BEGIN
    RETURN QUERY
    SELECT 
        (SELECT COUNT(*) FROM insights WHERE status = 'active') as total_insights,
        
        (SELECT jsonb_object_agg(insight_type, cnt) FROM (
            SELECT insight_type, COUNT(*) as cnt 
            FROM insights 
            WHERE status = 'active' 
            GROUP BY insight_type
        ) t) as insights_by_type,
        
        (SELECT AVG(confidence_score) FROM insights WHERE status = 'active') as avg_confidence,
        
        (SELECT AVG(feedback_score) FROM insights WHERE status = 'active' AND feedback_score IS NOT NULL) as avg_feedback_score,
        
        (SELECT COUNT(*) FROM processing_queue WHERE status IN ('pending', 'processing')) as processing_queue_size,
        
        (SELECT COUNT(*) FROM processing_queue WHERE status = 'failed' AND retry_count >= max_retries) as failed_jobs,
        
        (SELECT jsonb_object_agg(tag, cnt) FROM (
            SELECT unnest(tags) as tag, COUNT(*) as cnt 
            FROM insights 
            WHERE status = 'active' AND array_length(tags, 1) > 0
            GROUP BY tag 
            ORDER BY cnt DESC 
            LIMIT 10
        ) t) as top_tags;
END;
$$ LANGUAGE plpgsql;

-- ========================================
-- VALIDATION AND VERIFICATION
-- ========================================

-- Verify all tables were created successfully
DO $$
DECLARE
    table_count INTEGER;
    required_tables TEXT[] := ARRAY[
        'insights',
        'insight_vectors', 
        'insight_feedback',
        'processing_queue'
    ];
    missing_tables TEXT[] := '{}';
    table_name TEXT;
    index_count INTEGER;
BEGIN
    -- Check each required table
    FOREACH table_name IN ARRAY required_tables
    LOOP
        SELECT COUNT(*) INTO table_count
        FROM information_schema.tables 
        WHERE table_name = table_name;
        
        IF table_count = 0 THEN
            missing_tables := array_append(missing_tables, table_name);
        END IF;
    END LOOP;
    
    IF array_length(missing_tables, 1) > 0 THEN
        RAISE WARNING 'CRITICAL: Some tables failed to create: %', array_to_string(missing_tables, ', ');
        RAISE EXCEPTION 'Table creation failed - deployment blocked';
    END IF;
    
    -- Verify processing_metadata column was added
    SELECT COUNT(*) INTO table_count
    FROM information_schema.columns 
    WHERE table_name = 'memories' 
    AND column_name = 'processing_metadata';
    
    IF table_count = 0 THEN
        RAISE EXCEPTION 'Failed to add processing_metadata column to memories table';
    END IF;
    
    -- Count indexes created
    SELECT COUNT(*) INTO index_count
    FROM pg_indexes 
    WHERE indexname LIKE 'idx_insights_%' 
    OR indexname LIKE 'idx_insight_%' 
    OR indexname LIKE 'idx_processing_%'
    OR indexname = 'idx_memories_processing_metadata';
    
    RAISE NOTICE '✅ All % tables created successfully', array_length(required_tables, 1);
    RAISE NOTICE '✅ processing_metadata column added to memories table';
    RAISE NOTICE '✅ % indexes created for insights performance', index_count;
    RAISE NOTICE '🚀 Migration 014 COMPLETED: Codex Dreams insights schema implemented';
    RAISE NOTICE '📊 PERFORMANCE: HNSW vector index optimized for 1536-dim embeddings';
    RAISE NOTICE '🔍 FEATURES: Automatic feedback scoring, processing queue, version tracking';
    RAISE NOTICE '⚙️  NEXT STEPS: Run Story 2 (Data Models) and Story 3 (Ollama Client)';
    RAISE NOTICE '⚠️  REMINDER: Set hnsw.ef_search = 64 for optimal vector query performance';
    
    -- Display statistics function
    RAISE NOTICE 'ℹ️  Use get_insights_processing_stats() for processing statistics';
END $$;

-- Set optimal ef_search for vector queries
SET hnsw.ef_search = 64;

-- Final success message
SELECT 'Codex Dreams insights schema created successfully! 🚀' as migration_result;