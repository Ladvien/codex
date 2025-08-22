-- Migration 003: Knowledge Graph and Three-Component Scoring Schema
-- Story 6: Reflection & Insight Generator - Knowledge Graph Implementation
-- Story 1: Three-Component Memory Scoring System Integration
-- Author: cognitive-memory-expert
-- Date: 2025-08-22

BEGIN;

-- Add three-component scoring fields to memories table
ALTER TABLE memories 
ADD COLUMN IF NOT EXISTS recency_score FLOAT DEFAULT 1.0,
ADD COLUMN IF NOT EXISTS relevance_score FLOAT DEFAULT 0.5,
ADD COLUMN IF NOT EXISTS combined_score FLOAT;

-- Add constraints for three-component scores
ALTER TABLE memories 
ADD CONSTRAINT check_recency_score 
CHECK (recency_score >= 0.0 AND recency_score <= 1.0);

ALTER TABLE memories 
ADD CONSTRAINT check_relevance_score 
CHECK (relevance_score >= 0.0 AND relevance_score <= 1.0);

ALTER TABLE memories 
ADD CONSTRAINT check_combined_score 
CHECK (combined_score >= 0.0 AND combined_score <= 1.0);

-- Create knowledge graph nodes table
CREATE TABLE IF NOT EXISTS knowledge_nodes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    concept VARCHAR(500) NOT NULL,
    node_type VARCHAR(50) NOT NULL CHECK (node_type IN ('concept', 'entity', 'relationship', 'insight', 'memory')),
    embedding vector(384), -- Adjustable dimension based on embedding model
    confidence FLOAT NOT NULL DEFAULT 0.8 CHECK (confidence >= 0.0 AND confidence <= 1.0),
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    created_by VARCHAR(100) DEFAULT 'system'
);

-- Create knowledge graph edges table for relationships
CREATE TABLE IF NOT EXISTS knowledge_edges (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    source_node_id UUID NOT NULL REFERENCES knowledge_nodes(id) ON DELETE CASCADE,
    target_node_id UUID NOT NULL REFERENCES knowledge_nodes(id) ON DELETE CASCADE,
    relationship_type VARCHAR(50) NOT NULL,
    strength FLOAT NOT NULL DEFAULT 0.5 CHECK (strength >= 0.0 AND strength <= 1.0),
    evidence_memories UUID[] DEFAULT '{}', -- Array of memory IDs supporting this relationship
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    UNIQUE(source_node_id, target_node_id, relationship_type)
);

-- Create insights table for reflection-generated meta-memories
CREATE TABLE IF NOT EXISTS insights (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    insight_type VARCHAR(50) NOT NULL CHECK (insight_type IN ('pattern', 'synthesis', 'gap', 'contradiction', 'trend', 'causality', 'analogy')),
    content TEXT NOT NULL,
    confidence_score FLOAT NOT NULL CHECK (confidence_score >= 0.0 AND confidence_score <= 1.0),
    source_memory_ids UUID[] NOT NULL,
    related_concepts TEXT[] DEFAULT '{}',
    importance_score FLOAT NOT NULL DEFAULT 0.5 CHECK (importance_score >= 0.0 AND importance_score <= 1.0),
    
    -- Validation metrics for insight quality
    novelty_score FLOAT CHECK (novelty_score >= 0.0 AND novelty_score <= 1.0),
    coherence_score FLOAT CHECK (coherence_score >= 0.0 AND coherence_score <= 1.0),
    evidence_strength FLOAT CHECK (evidence_strength >= 0.0 AND evidence_strength <= 1.0),
    semantic_richness FLOAT CHECK (semantic_richness >= 0.0 AND semantic_richness <= 1.0),
    predictive_power FLOAT CHECK (predictive_power >= 0.0 AND predictive_power <= 1.0),
    
    -- Corresponding memory ID when insight is stored as memory
    memory_id UUID REFERENCES memories(id) ON DELETE SET NULL,
    
    -- Knowledge graph integration
    knowledge_node_id UUID REFERENCES knowledge_nodes(id) ON DELETE SET NULL,
    
    generated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Create reflection sessions table for tracking reflection processes
CREATE TABLE IF NOT EXISTS reflection_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    trigger_reason TEXT NOT NULL,
    started_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    completed_at TIMESTAMP WITH TIME ZONE,
    status VARCHAR(20) NOT NULL DEFAULT 'in_progress' CHECK (status IN ('in_progress', 'completed', 'failed', 'cancelled')),
    
    -- Metrics
    analyzed_memory_count INTEGER DEFAULT 0,
    generated_cluster_count INTEGER DEFAULT 0,
    generated_insight_count INTEGER DEFAULT 0,
    
    -- Configuration used
    config_snapshot JSONB DEFAULT '{}',
    
    -- Results summary
    results_summary JSONB DEFAULT '{}',
    
    error_message TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Create memory clusters table for insight generation
CREATE TABLE IF NOT EXISTS memory_clusters (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    reflection_session_id UUID REFERENCES reflection_sessions(id) ON DELETE CASCADE,
    memory_ids UUID[] NOT NULL,
    centroid_embedding vector(384),
    coherence_score FLOAT CHECK (coherence_score >= 0.0 AND coherence_score <= 1.0),
    dominant_concepts TEXT[] DEFAULT '{}',
    temporal_span_start TIMESTAMP WITH TIME ZONE,
    temporal_span_end TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Create indexes for efficient querying

-- Knowledge graph indexes
CREATE INDEX IF NOT EXISTS knowledge_nodes_concept_idx ON knowledge_nodes (concept);
CREATE INDEX IF NOT EXISTS knowledge_nodes_type_idx ON knowledge_nodes (node_type);
CREATE INDEX IF NOT EXISTS knowledge_nodes_embedding_idx ON knowledge_nodes USING ivfflat (embedding vector_cosine_ops);
CREATE INDEX IF NOT EXISTS knowledge_nodes_created_at_idx ON knowledge_nodes (created_at DESC);

CREATE INDEX IF NOT EXISTS knowledge_edges_source_idx ON knowledge_edges (source_node_id);
CREATE INDEX IF NOT EXISTS knowledge_edges_target_idx ON knowledge_edges (target_node_id);
CREATE INDEX IF NOT EXISTS knowledge_edges_type_idx ON knowledge_edges (relationship_type);
CREATE INDEX IF NOT EXISTS knowledge_edges_strength_idx ON knowledge_edges (strength DESC);

-- Insight indexes
CREATE INDEX IF NOT EXISTS insights_type_idx ON insights (insight_type);
CREATE INDEX IF NOT EXISTS insights_confidence_idx ON insights (confidence_score DESC);
CREATE INDEX IF NOT EXISTS insights_importance_idx ON insights (importance_score DESC);
CREATE INDEX IF NOT EXISTS insights_generated_at_idx ON insights (generated_at DESC);
CREATE INDEX IF NOT EXISTS insights_memory_id_idx ON insights (memory_id) WHERE memory_id IS NOT NULL;

-- Reflection session indexes
CREATE INDEX IF NOT EXISTS reflection_sessions_status_idx ON reflection_sessions (status);
CREATE INDEX IF NOT EXISTS reflection_sessions_started_at_idx ON reflection_sessions (started_at DESC);

-- Memory cluster indexes
CREATE INDEX IF NOT EXISTS memory_clusters_session_idx ON memory_clusters (reflection_session_id);
CREATE INDEX IF NOT EXISTS memory_clusters_coherence_idx ON memory_clusters (coherence_score DESC);

-- Three-component scoring indexes for memories
CREATE INDEX IF NOT EXISTS memories_recency_score_idx ON memories (recency_score DESC);
CREATE INDEX IF NOT EXISTS memories_relevance_score_idx ON memories (relevance_score DESC);
CREATE INDEX IF NOT EXISTS memories_combined_score_idx ON memories (combined_score DESC);

-- Composite index for three-component search
CREATE INDEX IF NOT EXISTS memories_three_component_idx 
ON memories (combined_score DESC, importance_score DESC, recency_score DESC, relevance_score DESC)
WHERE status = 'active';

-- Create functions for three-component scoring

-- Function to calculate recency score using exponential decay
CREATE OR REPLACE FUNCTION calculate_recency_score(
    p_last_accessed_at TIMESTAMP WITH TIME ZONE,
    p_created_at TIMESTAMP WITH TIME ZONE,
    p_lambda FLOAT DEFAULT 0.005
) RETURNS FLOAT AS $$
DECLARE
    reference_time TIMESTAMP WITH TIME ZONE;
    hours_elapsed FLOAT;
    recency_score FLOAT;
BEGIN
    -- Use last accessed time if available, otherwise creation time
    reference_time := COALESCE(p_last_accessed_at, p_created_at);
    
    -- Calculate hours since reference time
    hours_elapsed := EXTRACT(EPOCH FROM (NOW() - reference_time)) / 3600.0;
    
    -- Apply exponential decay: e^(-λt)
    recency_score := EXP(-p_lambda * hours_elapsed);
    
    -- Ensure bounds [0, 1]
    RETURN GREATEST(0.0, LEAST(1.0, recency_score));
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Function to calculate relevance score based on context similarity
CREATE OR REPLACE FUNCTION calculate_relevance_score(
    p_memory_embedding vector,
    p_context_embedding vector DEFAULT NULL,
    p_access_count INTEGER DEFAULT 0,
    p_importance_score FLOAT DEFAULT 0.5
) RETURNS FLOAT AS $$
DECLARE
    similarity_score FLOAT := 0.5; -- Default relevance
    access_factor FLOAT;
    relevance_score FLOAT;
BEGIN
    -- Calculate semantic similarity if embeddings are available
    IF p_memory_embedding IS NOT NULL AND p_context_embedding IS NOT NULL THEN
        similarity_score := 1.0 - (p_memory_embedding <=> p_context_embedding);
        similarity_score := GREATEST(0.0, LEAST(1.0, similarity_score));
    END IF;
    
    -- Factor in access patterns and importance
    access_factor := LEAST(1.0, p_access_count::FLOAT / 10.0); -- Normalize to [0,1]
    
    -- Combine factors: 60% similarity, 25% importance, 15% access pattern
    relevance_score := 0.6 * similarity_score + 0.25 * p_importance_score + 0.15 * access_factor;
    
    RETURN GREATEST(0.0, LEAST(1.0, relevance_score));
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Function to calculate combined three-component score
CREATE OR REPLACE FUNCTION calculate_combined_score(
    p_recency_score FLOAT,
    p_importance_score FLOAT,
    p_relevance_score FLOAT,
    p_alpha FLOAT DEFAULT 0.333,
    p_beta FLOAT DEFAULT 0.333,
    p_gamma FLOAT DEFAULT 0.334
) RETURNS FLOAT AS $$
DECLARE
    combined_score FLOAT;
BEGIN
    -- Weighted combination: α × recency + β × importance + γ × relevance
    combined_score := p_alpha * p_recency_score + p_beta * p_importance_score + p_gamma * p_relevance_score;
    
    -- Ensure bounds [0, 1]
    RETURN GREATEST(0.0, LEAST(1.0, combined_score));
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Function to update all three-component scores for a memory
CREATE OR REPLACE FUNCTION update_three_component_scores(
    p_memory_id UUID,
    p_context_embedding vector DEFAULT NULL,
    p_alpha FLOAT DEFAULT 0.333,
    p_beta FLOAT DEFAULT 0.333,
    p_gamma FLOAT DEFAULT 0.334
) RETURNS VOID AS $$
DECLARE
    memory_rec RECORD;
    new_recency FLOAT;
    new_relevance FLOAT;
    new_combined FLOAT;
BEGIN
    -- Get current memory data
    SELECT * INTO memory_rec FROM memories WHERE id = p_memory_id;
    
    IF NOT FOUND THEN
        RAISE EXCEPTION 'Memory not found: %', p_memory_id;
    END IF;
    
    -- Calculate new scores
    new_recency := calculate_recency_score(memory_rec.last_accessed_at, memory_rec.created_at);
    new_relevance := calculate_relevance_score(memory_rec.embedding, p_context_embedding, memory_rec.access_count, memory_rec.importance_score);
    new_combined := calculate_combined_score(new_recency, memory_rec.importance_score, new_relevance, p_alpha, p_beta, p_gamma);
    
    -- Update the memory
    UPDATE memories 
    SET recency_score = new_recency,
        relevance_score = new_relevance,
        combined_score = new_combined,
        updated_at = NOW()
    WHERE id = p_memory_id;
END;
$$ LANGUAGE plpgsql;

-- Function to batch update three-component scores
CREATE OR REPLACE FUNCTION batch_update_three_component_scores(
    p_limit INTEGER DEFAULT 1000,
    p_context_embedding vector DEFAULT NULL
) RETURNS INTEGER AS $$
DECLARE
    updated_count INTEGER := 0;
    memory_rec RECORD;
BEGIN
    FOR memory_rec IN 
        SELECT id FROM memories 
        WHERE status = 'active' 
        ORDER BY updated_at ASC 
        LIMIT p_limit
    LOOP
        PERFORM update_three_component_scores(memory_rec.id, p_context_embedding);
        updated_count := updated_count + 1;
    END LOOP;
    
    RETURN updated_count;
END;
$$ LANGUAGE plpgsql;

-- Function to add knowledge graph relationship
CREATE OR REPLACE FUNCTION add_knowledge_relationship(
    p_source_concept TEXT,
    p_target_concept TEXT,
    p_relationship_type TEXT,
    p_strength FLOAT DEFAULT 0.5,
    p_evidence_memories UUID[] DEFAULT '{}'
) RETURNS UUID AS $$
DECLARE
    source_node_id UUID;
    target_node_id UUID;
    edge_id UUID;
BEGIN
    -- Find or create source node
    SELECT id INTO source_node_id FROM knowledge_nodes WHERE concept = p_source_concept LIMIT 1;
    IF NOT FOUND THEN
        INSERT INTO knowledge_nodes (concept, node_type, confidence) 
        VALUES (p_source_concept, 'concept', 0.8) 
        RETURNING id INTO source_node_id;
    END IF;
    
    -- Find or create target node
    SELECT id INTO target_node_id FROM knowledge_nodes WHERE concept = p_target_concept LIMIT 1;
    IF NOT FOUND THEN
        INSERT INTO knowledge_nodes (concept, node_type, confidence) 
        VALUES (p_target_concept, 'concept', 0.8) 
        RETURNING id INTO target_node_id;
    END IF;
    
    -- Create or update edge
    INSERT INTO knowledge_edges (source_node_id, target_node_id, relationship_type, strength, evidence_memories)
    VALUES (source_node_id, target_node_id, p_relationship_type, p_strength, p_evidence_memories)
    ON CONFLICT (source_node_id, target_node_id, relationship_type) 
    DO UPDATE SET 
        strength = GREATEST(knowledge_edges.strength, EXCLUDED.strength),
        evidence_memories = array_cat(knowledge_edges.evidence_memories, EXCLUDED.evidence_memories),
        updated_at = NOW()
    RETURNING id INTO edge_id;
    
    RETURN edge_id;
END;
$$ LANGUAGE plpgsql;

-- Function to find knowledge graph paths between concepts
CREATE OR REPLACE FUNCTION find_knowledge_paths(
    p_source_concept TEXT,
    p_target_concept TEXT,
    p_max_depth INTEGER DEFAULT 3
) RETURNS TABLE(path_concepts TEXT[], path_strength FLOAT, path_length INTEGER) AS $$
BEGIN
    -- Simplified path finding - would implement proper graph traversal in production
    RETURN QUERY
    WITH RECURSIVE concept_paths AS (
        -- Base case: direct relationships
        SELECT 
            ARRAY[sn.concept, tn.concept] as concepts,
            e.strength,
            1 as depth
        FROM knowledge_edges e
        JOIN knowledge_nodes sn ON e.source_node_id = sn.id
        JOIN knowledge_nodes tn ON e.target_node_id = tn.id
        WHERE sn.concept = p_source_concept
        
        UNION ALL
        
        -- Recursive case: extend paths
        SELECT 
            cp.concepts || tn.concept,
            cp.strength * e.strength,
            cp.depth + 1
        FROM concept_paths cp
        JOIN knowledge_edges e ON e.source_node_id = (
            SELECT id FROM knowledge_nodes WHERE concept = cp.concepts[array_upper(cp.concepts, 1)]
        )
        JOIN knowledge_nodes tn ON e.target_node_id = tn.id
        WHERE cp.depth < p_max_depth
        AND tn.concept = p_target_concept
        AND NOT tn.concept = ANY(cp.concepts) -- Avoid cycles
    )
    SELECT concepts, strength, depth 
    FROM concept_paths 
    WHERE concepts[array_upper(concepts, 1)] = p_target_concept
    ORDER BY strength DESC, depth ASC;
END;
$$ LANGUAGE plpgsql;

-- Create trigger to automatically update three-component scores on memory access
CREATE OR REPLACE FUNCTION trigger_update_three_component_scores() RETURNS TRIGGER AS $$
BEGIN
    -- Only trigger on access or importance changes
    IF TG_OP = 'UPDATE' AND (
        OLD.last_accessed_at IS DISTINCT FROM NEW.last_accessed_at OR
        OLD.access_count IS DISTINCT FROM NEW.access_count OR
        OLD.importance_score IS DISTINCT FROM NEW.importance_score
    ) THEN
        -- Update three-component scores
        NEW.recency_score := calculate_recency_score(NEW.last_accessed_at, NEW.created_at);
        NEW.relevance_score := calculate_relevance_score(NEW.embedding, NULL, NEW.access_count, NEW.importance_score);
        NEW.combined_score := calculate_combined_score(NEW.recency_score, NEW.importance_score, NEW.relevance_score);
    END IF;
    
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Create the trigger
DROP TRIGGER IF EXISTS memories_three_component_trigger ON memories;
CREATE TRIGGER memories_three_component_trigger
    BEFORE UPDATE ON memories
    FOR EACH ROW
    EXECUTE FUNCTION trigger_update_three_component_scores();

-- Update existing memories with three-component scores
UPDATE memories 
SET recency_score = calculate_recency_score(last_accessed_at, created_at),
    relevance_score = calculate_relevance_score(embedding, NULL, access_count, importance_score),
    updated_at = NOW()
WHERE status = 'active';

-- Update combined scores
UPDATE memories 
SET combined_score = calculate_combined_score(recency_score, importance_score, relevance_score)
WHERE status = 'active' AND recency_score IS NOT NULL AND relevance_score IS NOT NULL;

-- Create view for memory search with enhanced scoring
CREATE OR REPLACE VIEW memory_search_enhanced AS
SELECT 
    m.*,
    -- Calculate dynamic scores for search context
    CASE 
        WHEN m.last_accessed_at IS NOT NULL THEN
            calculate_recency_score(m.last_accessed_at, m.created_at)
        ELSE 
            calculate_recency_score(m.created_at, m.created_at)
    END as dynamic_recency_score,
    
    -- Enhanced ranking score combining all factors
    (
        0.3 * COALESCE(m.combined_score, 0.5) +
        0.25 * COALESCE(m.recall_probability, 0.5) +
        0.2 * m.importance_score +
        0.15 * COALESCE(m.consolidation_strength / 10.0, 0.1) +
        0.1 * LEAST(1.0, m.access_count::FLOAT / 100.0)
    ) as enhanced_ranking_score
FROM memories m
WHERE m.status = 'active';

-- Insert initial statistics
INSERT INTO memory_tier_statistics (tier, memory_count, avg_consolidation_strength, avg_recall_probability)
SELECT 
    tier,
    COUNT(*) as memory_count,
    AVG(COALESCE(consolidation_strength, 1.0)) as avg_consolidation_strength,
    AVG(COALESCE(recall_probability, 0.8)) as avg_recall_probability
FROM memories 
WHERE status = 'active'
GROUP BY tier
ON CONFLICT DO NOTHING;

-- Record migration completion
INSERT INTO migration_history (migration_name, success, migration_reason)
VALUES (
    '003_knowledge_graph_schema',
    true,
    'Added knowledge graph, three-component scoring, and reflection system infrastructure'
);

COMMIT;