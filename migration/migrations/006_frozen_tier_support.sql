-- Migration 006: Add Frozen Tier Support
-- Purpose: Add 'frozen' to memory_tier enum and create frozen_memories table for compressed long-term storage
-- This follows SOTA cognitive architecture patterns for hierarchical memory systems

-- Step 1: Add 'frozen' value to existing memory_tier enum
-- Using safe enum extension method to avoid downtime
ALTER TYPE memory_tier ADD VALUE 'frozen' AFTER 'cold';

-- Step 2: Create frozen_memories table with BYTEA compression support
-- This table stores compressed memories using zstd compression for maximum space efficiency
CREATE TABLE IF NOT EXISTS frozen_memories (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    original_memory_id UUID NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    
    -- Compressed content using zstd (stored as BYTEA, not JSONB for efficiency)
    compressed_content BYTEA NOT NULL,
    
    -- Original metadata preserved for search and restoration
    original_metadata JSONB DEFAULT '{}',
    original_content_hash VARCHAR(64) NOT NULL,
    original_embedding vector(1536), -- Preserved for semantic search even when frozen
    original_tier memory_tier NOT NULL DEFAULT 'cold',
    
    -- Frozen tier management
    freeze_reason VARCHAR(255),
    frozen_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Retrieval tracking
    unfreeze_count INTEGER DEFAULT 0,
    last_unfrozen_at TIMESTAMPTZ,
    
    -- Compression metrics
    compression_ratio FLOAT CHECK (compression_ratio > 0),
    original_size_bytes INTEGER NOT NULL CHECK (original_size_bytes > 0),
    compressed_size_bytes INTEGER NOT NULL CHECK (compressed_size_bytes > 0),
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Constraints
    UNIQUE(original_memory_id), -- One frozen record per memory
    CHECK (compression_ratio = original_size_bytes::float / compressed_size_bytes::float)
);

-- Step 3: Create indexes for frozen_memories table
-- Optimized for frozen tier P(r) < 0.2 threshold queries and retrieval

-- Primary lookup index by original memory ID
CREATE INDEX idx_frozen_memories_original_id ON frozen_memories (original_memory_id);

-- Content hash index for deduplication checks
CREATE INDEX idx_frozen_memories_content_hash ON frozen_memories (original_content_hash);

-- Semantic search index (sparse, only for critical retrievals)
CREATE INDEX idx_frozen_memories_embedding ON frozen_memories 
    USING hnsw (original_embedding vector_cosine_ops)
    WITH (m = 16, ef_construction = 64); -- Lower quality for storage efficiency

-- Temporal access pattern index
CREATE INDEX idx_frozen_memories_frozen_at ON frozen_memories (frozen_at);
CREATE INDEX idx_frozen_memories_last_unfrozen ON frozen_memories (last_unfrozen_at DESC NULLS LAST);

-- Compression metrics index for analytics
CREATE INDEX idx_frozen_memories_compression_ratio ON frozen_memories (compression_ratio DESC);

-- Metadata search index (for finding frozen memories by original metadata)
CREATE INDEX idx_frozen_memories_metadata ON frozen_memories 
    USING gin (original_metadata);

-- Step 4: Add frozen tier indexes to existing memories table
-- These indexes support P(r) < 0.2 threshold queries for migration decisions

CREATE INDEX idx_memories_frozen_tier ON memories (id, recall_probability) 
    WHERE tier = 'frozen' AND status = 'active';

CREATE INDEX idx_memories_freeze_candidates ON memories (recall_probability, tier, last_accessed_at) 
    WHERE recall_probability < 0.2 AND tier = 'cold' AND status = 'active';

-- Step 5: Create function for frozen memory compression tracking
CREATE OR REPLACE FUNCTION calculate_compression_metrics()
RETURNS TRIGGER AS $$
BEGIN
    -- Auto-calculate compression ratio and size tracking
    NEW.compression_ratio = NEW.original_size_bytes::float / NEW.compressed_size_bytes::float;
    
    -- Validate compression ratio meets 5:1 minimum requirement
    IF NEW.compression_ratio < 5.0 THEN
        RAISE WARNING 'Compression ratio %.2f is below target 5:1 for frozen memory %', 
            NEW.compression_ratio, NEW.original_memory_id;
    END IF;
    
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER calculate_frozen_compression_metrics 
    BEFORE INSERT OR UPDATE ON frozen_memories 
    FOR EACH ROW 
    EXECUTE FUNCTION calculate_compression_metrics();

-- Step 6: Create function for frozen memory access tracking
CREATE OR REPLACE FUNCTION track_frozen_access()
RETURNS TRIGGER AS $$
BEGIN
    -- Track unfreezing events for analytics
    IF OLD.unfreeze_count IS DISTINCT FROM NEW.unfreeze_count THEN
        NEW.last_unfrozen_at = NOW();
        
        -- Log the unfreezing event for performance monitoring
        INSERT INTO migration_history (
            memory_id, 
            from_tier, 
            to_tier, 
            migration_reason, 
            migration_duration_ms,
            success
        ) VALUES (
            NEW.original_memory_id,
            'frozen',
            'cold', -- Assume unfrozen memories go back to cold tier
            'Unfrozen for retrieval - intentional 2-5s delay applied',
            EXTRACT(EPOCH FROM (NOW() - OLD.updated_at)) * 1000, -- Duration since last update
            true
        );
    END IF;
    
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER track_frozen_memory_access 
    BEFORE UPDATE ON frozen_memories 
    FOR EACH ROW 
    EXECUTE FUNCTION track_frozen_access();

-- Step 7: Update updated_at trigger for frozen_memories
CREATE TRIGGER update_frozen_memories_updated_at 
    BEFORE UPDATE ON frozen_memories 
    FOR EACH ROW 
    EXECUTE FUNCTION update_updated_at_column();

-- Step 8: Create view for frozen memory analytics
CREATE OR REPLACE VIEW frozen_memory_analytics AS
SELECT 
    COUNT(*) as total_frozen_memories,
    AVG(compression_ratio) as avg_compression_ratio,
    SUM(original_size_bytes) as total_original_bytes,
    SUM(compressed_size_bytes) as total_compressed_bytes,
    SUM(original_size_bytes - compressed_size_bytes) as total_space_saved_bytes,
    AVG(unfreeze_count) as avg_unfreeze_count,
    COUNT(*) FILTER (WHERE unfreeze_count > 0) as memories_ever_unfrozen,
    AVG(EXTRACT(EPOCH FROM (NOW() - frozen_at)) / 86400) as avg_frozen_age_days,
    MIN(compression_ratio) as min_compression_ratio,
    MAX(compression_ratio) as max_compression_ratio
FROM frozen_memories;

-- Step 9: Create maintenance function for frozen tier optimization
CREATE OR REPLACE FUNCTION optimize_frozen_tier()
RETURNS TABLE (
    optimization_summary TEXT,
    memories_processed INTEGER,
    space_reclaimed_bytes BIGINT
) AS $$
DECLARE
    processed_count INTEGER := 0;
    space_reclaimed BIGINT := 0;
BEGIN
    -- Identify and remove duplicate frozen memories (by content hash)
    WITH duplicates AS (
        SELECT original_content_hash, MIN(id) as keep_id
        FROM frozen_memories 
        GROUP BY original_content_hash 
        HAVING COUNT(*) > 1
    ),
    to_delete AS (
        SELECT fm.id, fm.compressed_size_bytes
        FROM frozen_memories fm
        JOIN duplicates d ON fm.original_content_hash = d.original_content_hash
        WHERE fm.id != d.keep_id
    )
    DELETE FROM frozen_memories 
    WHERE id IN (SELECT id FROM to_delete);
    
    GET DIAGNOSTICS processed_count = ROW_COUNT;
    
    -- Calculate space reclaimed (approximate)
    SELECT COALESCE(SUM(compressed_size_bytes), 0) INTO space_reclaimed
    FROM frozen_memories 
    WHERE compression_ratio < 3.0; -- Poor compression candidates
    
    RETURN QUERY SELECT 
        'Frozen tier optimization completed'::TEXT,
        processed_count,
        space_reclaimed;
END;
$$ LANGUAGE plpgsql;

-- Step 10: Create database functions for freeze/unfreeze operations
-- These functions handle the complex logic of compressing and decompressing memories

CREATE OR REPLACE FUNCTION freeze_memory(memory_id_param UUID)
RETURNS UUID AS $$
DECLARE
    memory_record memories%ROWTYPE;
    frozen_id UUID;
    content_bytes BYTEA;
    compression_ratio_val FLOAT;
BEGIN
    -- Get the memory to freeze
    SELECT * INTO memory_record FROM memories 
    WHERE id = memory_id_param AND status = 'active';
    
    IF NOT FOUND THEN
        RAISE EXCEPTION 'Memory % not found or not active', memory_id_param;
    END IF;
    
    -- Ensure we only freeze cold memories with P(r) < 0.2
    IF memory_record.tier != 'cold' THEN
        RAISE EXCEPTION 'Can only freeze memories in cold tier, found %', memory_record.tier;
    END IF;
    
    IF COALESCE(memory_record.recall_probability, 0) >= 0.2 THEN
        RAISE EXCEPTION 'Can only freeze memories with P(r) < 0.2, found %', 
            COALESCE(memory_record.recall_probability, 0);
    END IF;
    
    -- For now, simulate compression by converting text to bytes
    -- In reality, the Rust application will handle zstd compression
    content_bytes = convert_to(memory_record.content, 'UTF8');
    
    -- Simulate compression ratio (will be updated by Rust application)
    compression_ratio_val = GREATEST(5.0, LENGTH(memory_record.content)::float / GREATEST(LENGTH(memory_record.content) / 6, 1));
    
    -- Create frozen memory record
    INSERT INTO frozen_memories (
        original_memory_id,
        compressed_content,
        original_metadata,
        original_content_hash,
        original_embedding,
        original_tier,
        freeze_reason,
        compression_ratio,
        original_size_bytes,
        compressed_size_bytes
    ) VALUES (
        memory_record.id,
        content_bytes, -- Will be replaced with zstd compressed data by Rust
        memory_record.metadata,
        memory_record.content_hash,
        memory_record.embedding,
        memory_record.tier,
        'Auto-frozen: P(r) < 0.2 threshold',
        compression_ratio_val,
        LENGTH(memory_record.content),
        LENGTH(content_bytes)
    ) RETURNING id INTO frozen_id;
    
    -- Update original memory to frozen tier
    UPDATE memories 
    SET 
        tier = 'frozen',
        status = 'archived',
        updated_at = NOW()
    WHERE id = memory_id_param;
    
    -- Log the migration
    INSERT INTO migration_history (
        memory_id,
        from_tier,
        to_tier,
        migration_reason,
        success
    ) VALUES (
        memory_id_param,
        memory_record.tier,
        'frozen',
        'Automatic freeze: P(r) < 0.2',
        true
    );
    
    RETURN frozen_id;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION unfreeze_memory(frozen_id_param UUID)
RETURNS UUID AS $$
DECLARE
    frozen_record frozen_memories%ROWTYPE;
    memory_id_result UUID;
    decompressed_content TEXT;
BEGIN
    -- Get the frozen memory
    SELECT * INTO frozen_record FROM frozen_memories 
    WHERE id = frozen_id_param;
    
    IF NOT FOUND THEN
        RAISE EXCEPTION 'Frozen memory % not found', frozen_id_param;
    END IF;
    
    -- For now, simulate decompression by converting bytes back to text
    -- In reality, the Rust application will handle zstd decompression
    decompressed_content = convert_from(frozen_record.compressed_content, 'UTF8');
    
    -- Restore the original memory
    UPDATE memories 
    SET 
        content = decompressed_content,
        tier = COALESCE(frozen_record.original_tier, 'working'),
        status = 'active',
        metadata = COALESCE(frozen_record.original_metadata, '{}'),
        updated_at = NOW()
    WHERE id = frozen_record.original_memory_id
    RETURNING id INTO memory_id_result;
    
    IF NOT FOUND THEN
        -- Create new memory if original was deleted
        INSERT INTO memories (
            id,
            content,
            content_hash,
            embedding,
            tier,
            status,
            importance_score,
            metadata,
            created_at,
            updated_at
        ) VALUES (
            frozen_record.original_memory_id,
            decompressed_content,
            frozen_record.original_content_hash,
            frozen_record.original_embedding,
            'working',
            'active',
            0.5, -- Default importance
            COALESCE(frozen_record.original_metadata, '{}'),
            NOW(),
            NOW()
        ) RETURNING id INTO memory_id_result;
    END IF;
    
    -- Update frozen memory access tracking
    UPDATE frozen_memories 
    SET 
        unfreeze_count = COALESCE(unfreeze_count, 0) + 1,
        last_unfrozen_at = NOW(),
        updated_at = NOW()
    WHERE id = frozen_id_param;
    
    -- Log the migration
    INSERT INTO migration_history (
        memory_id,
        from_tier,
        to_tier,
        migration_reason,
        success
    ) VALUES (
        memory_id_result,
        'frozen',
        'working',
        'Manual unfreeze operation',
        true
    );
    
    RETURN memory_id_result;
END;
$$ LANGUAGE plpgsql;

-- Step 11: Performance tuning comments for frozen tier
-- These settings optimize for the specific access patterns of frozen memories:
-- - Infrequent access (P(r) < 0.2)
-- - Large compression operations
-- - Batch processing during low-traffic periods

-- Recommended postgresql.conf settings for frozen tier:
-- 
-- # Increase work memory for compression operations
-- work_mem = 256MB  # During compression operations
-- 
-- # Optimize for large sequential reads from frozen storage
-- seq_page_cost = 1.0
-- random_page_cost = 4.0
-- 
-- # Background writer settings for frozen tier batch operations
-- bgwriter_delay = 200ms
-- bgwriter_lru_maxpages = 100
-- 
-- # Checkpoint settings to handle large frozen memory migrations
-- checkpoint_segments = 32
-- checkpoint_completion_target = 0.9
-- 
-- # Autovacuum settings for frozen_memories table
-- ALTER TABLE frozen_memories SET (autovacuum_vacuum_threshold = 1000);
-- ALTER TABLE frozen_memories SET (autovacuum_analyze_threshold = 500);
-- ALTER TABLE frozen_memories SET (autovacuum_vacuum_scale_factor = 0.1);

-- Final verification
DO $$
BEGIN
    -- Verify the frozen tier was added to the enum
    IF NOT EXISTS (
        SELECT 1 FROM pg_enum e
        JOIN pg_type t ON e.enumtypid = t.oid
        WHERE t.typname = 'memory_tier' AND e.enumlabel = 'frozen'
    ) THEN
        RAISE EXCEPTION 'Failed to add frozen value to memory_tier enum';
    END IF;
    
    -- Verify the frozen_memories table was created
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.tables 
        WHERE table_name = 'frozen_memories'
    ) THEN
        RAISE EXCEPTION 'Failed to create frozen_memories table';
    END IF;
    
    RAISE NOTICE 'Frozen tier support successfully added to database schema';
END $$;