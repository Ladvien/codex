-- Fix Memory Tiers and Scores Script
-- This script re-scores test data and development artifacts to move them out of working memory

BEGIN;

-- 1. Identify and mark test data with low importance
UPDATE memories 
SET 
    importance_score = 0.1,
    tier = 'cold',
    metadata = jsonb_set(
        COALESCE(metadata, '{}'::jsonb),
        '{auto_tiered}',
        'true'::jsonb
    ),
    updated_at = NOW()
WHERE status = 'active'
AND tier = 'working'
AND (
    -- Test data patterns
    content LIKE 'Concurrent memory thread%'
    OR content LIKE 'Test memory%'
    OR content LIKE 'Health check completed%'
    OR content LIKE '%test suite%'
    OR content LIKE '%Story %: %Status: COMPLETED%'
    OR content LIKE '%binary size:%'
);

-- 2. Move development/implementation notes to warm tier with medium importance
UPDATE memories
SET 
    importance_score = 0.3,
    tier = 'warm',
    metadata = jsonb_set(
        COALESCE(metadata, '{}'::jsonb),
        '{auto_tiered}',
        'true'::jsonb
    ),
    updated_at = NOW()
WHERE status = 'active'
AND tier = 'working'
AND (
    -- Development artifacts
    content LIKE '%Jira story%'
    OR content LIKE '%Creating Rust%processor%'
    OR content LIKE '%Development Communication Summary%'
    OR content LIKE '%## Messages.md%'
    OR content LIKE '%Story %:%'
    OR content LIKE '%Status: COMPLETED%'
);

-- 3. Recalculate combined scores for affected memories
UPDATE memories
SET combined_score = 
    CASE 
        WHEN tier = 'cold' THEN 
            (0.2 * importance_score + 0.3 * recency_score + 0.5 * frequency_score)
        WHEN tier = 'warm' THEN
            (0.3 * importance_score + 0.4 * recency_score + 0.3 * frequency_score)
        ELSE -- working
            (0.5 * importance_score + 0.3 * recency_score + 0.2 * frequency_score)
    END
WHERE status = 'active'
AND metadata->>'auto_tiered' = 'true';

-- 4. Show what we're changing
SELECT 
    tier,
    COUNT(*) as memories_affected,
    AVG(importance_score) as new_avg_importance,
    MIN(LEFT(content, 50)) as example_content
FROM memories
WHERE status = 'active'
AND metadata->>'auto_tiered' = 'true'
GROUP BY tier;

-- 5. Ensure working memory capacity limit (keep only top 9 truly important memories)
WITH ranked_memories AS (
    SELECT 
        id,
        ROW_NUMBER() OVER (ORDER BY combined_score DESC, importance_score DESC) as rank
    FROM memories
    WHERE tier = 'working'
    AND status = 'active'
)
UPDATE memories
SET tier = 'warm'
WHERE id IN (
    SELECT id FROM ranked_memories WHERE rank > 9
);

COMMIT;

-- Summary of changes
SELECT 
    'Tier Distribution After Fix' as report,
    tier,
    COUNT(*) as count,
    ROUND(AVG(importance_score)::numeric, 3) as avg_importance,
    ROUND(AVG(combined_score)::numeric, 3) as avg_combined
FROM memories
WHERE status = 'active'
GROUP BY tier
ORDER BY tier;