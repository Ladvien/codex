-- ============================================================================
-- ARCHITECTURE ALIGNMENT MIGRATION
-- Aligns database with Enhanced Agentic Memory System Architecture v2.0
-- ============================================================================

BEGIN;

-- 1. ADD THREE-COMPONENT SCORING SYSTEM
-- Add missing scoring columns to memories table
ALTER TABLE memories 
ADD COLUMN IF NOT EXISTS recency_score FLOAT,
ADD COLUMN IF NOT EXISTS relevance_score FLOAT DEFAULT 0.0;

-- Update existing memories with initial recency scores
UPDATE memories 
SET recency_score = CASE 
    WHEN last_accessed_at IS NOT NULL THEN 
        EXP(-0.005 * EXTRACT(EPOCH FROM (NOW() - last_accessed_at)) / 3600.0)
    ELSE 
        EXP(-0.005 * EXTRACT(EPOCH FROM (NOW() - created_at)) / 3600.0)
END
WHERE recency_score IS NULL;

-- Add combined score computed column (PostgreSQL 12+ generated column)
DO $$ BEGIN
    BEGIN
        ALTER TABLE memories 
        ADD COLUMN combined_score FLOAT GENERATED ALWAYS AS 
            (0.333 * COALESCE(recency_score, 0.0) + 
             0.333 * COALESCE(importance_score, 0.0) + 
             0.333 * COALESCE(relevance_score, 0.0)) STORED;
    EXCEPTION
        WHEN duplicate_column THEN NULL;
    END;
END $$;

-- Add index for combined score
CREATE INDEX IF NOT EXISTS idx_memories_combined_score ON memories (combined_score DESC);

-- 2. CREATE HARVEST SESSIONS TABLE
CREATE TABLE IF NOT EXISTS harvest_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    conversation_id TEXT,
    timestamp TIMESTAMPTZ DEFAULT NOW(),
    memories_extracted INTEGER DEFAULT 0,
    patterns_detected JSONB DEFAULT '{}',
    confidence_scores FLOAT[] DEFAULT '{}',
    extraction_duration_ms INTEGER,
    trigger_type TEXT, -- 'interval', 'pattern', 'manual'
    client_info JSONB DEFAULT '{}'
);

-- Add indexes for harvest sessions
CREATE INDEX IF NOT EXISTS idx_harvest_sessions_timestamp ON harvest_sessions (timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_harvest_sessions_conversation ON harvest_sessions (conversation_id);

-- 3. CREATE INSIGHTS AND REFLECTION SYSTEM
CREATE TABLE IF NOT EXISTS insights (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    content TEXT NOT NULL,
    embedding vector(768), -- Match current vector dimensions
    source_memory_ids UUID[] DEFAULT '{}',
    importance_multiplier FLOAT DEFAULT 1.5,
    insight_type TEXT DEFAULT 'reflection', -- 'reflection', 'pattern', 'synthesis'
    confidence_score FLOAT DEFAULT 0.0,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    last_accessed_at TIMESTAMPTZ DEFAULT NOW(),
    access_count INTEGER DEFAULT 0
);

-- Add indexes for insights
CREATE INDEX IF NOT EXISTS idx_insights_created_at ON insights (created_at DESC);
CREATE INDEX IF NOT EXISTS idx_insights_confidence ON insights (confidence_score DESC);
CREATE INDEX IF NOT EXISTS idx_insights_type ON insights (insight_type);
CREATE INDEX IF NOT EXISTS idx_insights_embedding ON insights USING hnsw (embedding vector_cosine_ops);

-- 4. ADD HARVEST SOURCE TRACKING
ALTER TABLE memories 
ADD COLUMN IF NOT EXISTS harvest_source TEXT DEFAULT 'explicit', -- 'explicit', 'auto', 'reflection'
ADD COLUMN IF NOT EXISTS harvest_session_id UUID REFERENCES harvest_sessions(id),
ADD COLUMN IF NOT EXISTS parent_insight_id UUID REFERENCES insights(id);

-- Add indexes for harvest tracking
CREATE INDEX IF NOT EXISTS idx_memories_harvest_source ON memories (harvest_source);
CREATE INDEX IF NOT EXISTS idx_memories_harvest_session ON memories (harvest_session_id);

-- 5. ENHANCE METADATA SCHEMA
-- Add additional metadata columns mentioned in architecture
ALTER TABLE memories 
ADD COLUMN IF NOT EXISTS merge_parent_ids UUID[],
ADD COLUMN IF NOT EXISTS emotional_context TEXT,
ADD COLUMN IF NOT EXISTS user_correction_flag BOOLEAN DEFAULT FALSE,
ADD COLUMN IF NOT EXISTS explicit_remember_flag BOOLEAN DEFAULT FALSE;

-- 6. CREATE KNOWLEDGE GRAPH RELATIONSHIPS TABLE
CREATE TABLE IF NOT EXISTS memory_relationships (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    source_memory_id UUID NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    target_memory_id UUID NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    relationship_type TEXT NOT NULL, -- 'similar', 'causal', 'temporal', 'contextual'
    strength FLOAT DEFAULT 0.0 CHECK (strength >= 0.0 AND strength <= 1.0),
    created_at TIMESTAMPTZ DEFAULT NOW(),
    last_validated_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(source_memory_id, target_memory_id, relationship_type)
);

-- Add indexes for relationships
CREATE INDEX IF NOT EXISTS idx_memory_relationships_source ON memory_relationships (source_memory_id);
CREATE INDEX IF NOT EXISTS idx_memory_relationships_target ON memory_relationships (target_memory_id);
CREATE INDEX IF NOT EXISTS idx_memory_relationships_type ON memory_relationships (relationship_type);
CREATE INDEX IF NOT EXISTS idx_memory_relationships_strength ON memory_relationships (strength DESC);

-- 7. CREATE PERFORMANCE DASHBOARD TABLES
CREATE TABLE IF NOT EXISTS memory_performance_metrics (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    metric_type TEXT NOT NULL, -- 'consolidation', 'retrieval', 'harvesting'
    timestamp TIMESTAMPTZ DEFAULT NOW(),
    value FLOAT NOT NULL,
    metadata JSONB DEFAULT '{}',
    tier_breakdown JSONB DEFAULT '{}'
);

-- Add index for metrics
CREATE INDEX IF NOT EXISTS idx_memory_metrics_type_timestamp ON memory_performance_metrics (metric_type, timestamp DESC);

-- 8. ADD TRIGGER FUNCTIONS FOR ARCHITECTURE FEATURES

-- Function to update recency score on access
CREATE OR REPLACE FUNCTION update_recency_score() RETURNS TRIGGER AS $$
BEGIN
    NEW.recency_score = EXP(-0.005 * EXTRACT(EPOCH FROM (NOW() - NEW.last_accessed_at)) / 3600.0);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Trigger to update recency score
DROP TRIGGER IF EXISTS update_recency_score_trigger ON memories;
CREATE TRIGGER update_recency_score_trigger
    BEFORE UPDATE OF last_accessed_at ON memories
    FOR EACH ROW EXECUTE FUNCTION update_recency_score();

-- Function to track insight access
CREATE OR REPLACE FUNCTION update_insight_access() RETURNS TRIGGER AS $$
BEGIN
    NEW.last_accessed_at = NOW();
    NEW.access_count = OLD.access_count + 1;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- 9. CREATE VIEWS FOR ARCHITECTURE ALIGNMENT

-- Memory health dashboard view
CREATE OR REPLACE VIEW memory_health_dashboard AS
SELECT 
    tier,
    COUNT(*) as memory_count,
    AVG(importance_score) as avg_importance,
    AVG(consolidation_strength) as avg_consolidation,
    AVG(recall_probability) as avg_recall_probability,
    AVG(combined_score) as avg_combined_score,
    COUNT(*) FILTER (WHERE last_accessed_at > NOW() - INTERVAL '24 hours') as recently_accessed
FROM memories 
WHERE status = 'active' 
GROUP BY tier;

-- Harvesting performance view
CREATE OR REPLACE VIEW harvesting_performance AS
SELECT 
    DATE_TRUNC('day', timestamp) as harvest_date,
    COUNT(*) as sessions,
    SUM(memories_extracted) as total_memories,
    AVG(memories_extracted::float) as avg_memories_per_session,
    AVG(extraction_duration_ms) as avg_duration_ms
FROM harvest_sessions 
GROUP BY DATE_TRUNC('day', timestamp)
ORDER BY harvest_date DESC;

-- Architecture compliance check view
CREATE OR REPLACE VIEW architecture_compliance AS
SELECT 
    'Three-Component Scoring' as feature,
    CASE 
        WHEN COUNT(*) FILTER (WHERE recency_score IS NOT NULL AND relevance_score IS NOT NULL) > 0 
        THEN 'Implemented' 
        ELSE 'Missing' 
    END as status,
    COUNT(*) as total_memories
FROM memories
UNION ALL
SELECT 
    'Harvest Sessions' as feature,
    CASE WHEN COUNT(*) > 0 THEN 'Implemented' ELSE 'Missing' END as status,
    COUNT(*) as total_records
FROM harvest_sessions
UNION ALL
SELECT 
    'Insights System' as feature,
    CASE WHEN COUNT(*) > 0 THEN 'Implemented' ELSE 'Missing' END as status,
    COUNT(*) as total_insights
FROM insights;

-- 10. UPDATE MIGRATION HISTORY
INSERT INTO migration_history (
    migration_name,
    applied_at
) VALUES (
    'architecture_alignment_v2',
    NOW()
) ON CONFLICT (migration_name) DO UPDATE SET
    applied_at = NOW();

COMMIT;

-- Display alignment status
SELECT 'Architecture alignment migration completed successfully!' as status;
SELECT * FROM architecture_compliance;