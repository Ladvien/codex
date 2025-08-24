-- Fix Memory Tiers and Scores Script V2
-- Moves test data and dev artifacts out of working memory search space

BEGIN;

-- 1. Move test data to cold tier
UPDATE memories 
SET 
    importance_score = 0.1,
    tier = 'cold',
    recency_score = 0.1,
    combined_score = 0.1,
    metadata = jsonb_set(
        COALESCE(metadata, '{}'::jsonb),
        '{auto_tiered}',
        'true'::jsonb
    ),
    updated_at = NOW()
WHERE status = 'active'
AND tier IN ('working', 'warm')
AND (
    content LIKE 'Concurrent memory thread%'
    OR content LIKE 'Test memory%'
    OR content LIKE 'Health check completed%'
    OR content LIKE '%item %'
    OR id IN (
        'b1a35f50-45a1-4c02-9d22-5209ed412c69',
        'e088c424-b0eb-43a5-a47a-9c50e72b02aa', 
        '2b67478b-1274-4754-aeb2-c7b12a73ee58',
        '9e111f72-32e2-46c9-8b58-d2cfb6fac339',
        '534d3180-8ee6-4cf8-abb7-ab683b646e3f'
    )
);

-- 2. Move development artifacts to warm tier
UPDATE memories
SET 
    importance_score = 0.3,
    tier = 'warm',
    recency_score = 0.3,
    combined_score = 0.3,
    metadata = jsonb_set(
        COALESCE(metadata, '{}'::jsonb),
        '{auto_tiered}',
        'true'::jsonb
    ),
    updated_at = NOW()
WHERE status = 'active'
AND tier = 'working'
AND (
    content LIKE '%Jira story%'
    OR content LIKE '%Story %:%'
    OR content LIKE '%## Messages.md%'
    OR id IN (
        '914549d2-3103-42f1-afc9-dfd59b0e6e05',
        '70fb03a6-f0f0-4301-ade8-ea83610cecfa',
        '38455c05-4e2c-4afc-9c4c-f3475abd7cd4'
    )
);

-- 3. Show what changed
SELECT 
    'Moved to ' || tier as action,
    COUNT(*) as count,
    ROUND(AVG(importance_score)::numeric, 2) as avg_importance
FROM memories
WHERE metadata->>'auto_tiered' = 'true'
GROUP BY tier;

COMMIT;

-- 4. Final tier distribution
SELECT 
    tier,
    COUNT(*) as memories,
    ROUND(AVG(importance_score)::numeric, 3) as avg_importance,
    ROUND(AVG(combined_score)::numeric, 3) as avg_score
FROM memories
WHERE status = 'active'
GROUP BY tier
ORDER BY tier;