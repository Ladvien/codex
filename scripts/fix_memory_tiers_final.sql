-- Fix Memory Tiers - Final Version
-- Properly moves test/dev data out of working memory

BEGIN;

-- 1. Move specific test memories to cold tier by ID
UPDATE memories 
SET 
    importance_score = 0.05,
    tier = 'cold',
    recency_score = 0.05,
    updated_at = NOW()
WHERE id IN (
    'b1a35f50-45a1-4c02-9d22-5209ed412c69', -- Concurrent memory thread 15 item 1
    'e088c424-b0eb-43a5-a47a-9c50e72b02aa', -- Concurrent memory thread 8 item 8
    '2b67478b-1274-4754-aeb2-c7b12a73ee58', -- Concurrent memory thread 2 item 9
    '9e111f72-32e2-46c9-8b58-d2cfb6fac339', -- Test memory from health check
    '534d3180-8ee6-4cf8-abb7-ab683b646e3f'  -- Health check completed
);

-- 2. Move development artifacts to warm tier by ID
UPDATE memories
SET 
    importance_score = 0.25,
    tier = 'warm',
    recency_score = 0.3,
    updated_at = NOW()
WHERE id IN (
    '914549d2-3103-42f1-afc9-dfd59b0e6e05', -- User pivoted from code implementation
    '70fb03a6-f0f0-4301-ade8-ea83610cecfa', -- Creating Rust background insights
    '38455c05-4e2c-4afc-9c4c-f3475abd7cd4'  -- Messages.md Development Communication
);

-- 3. Move any other test-like content to cold
UPDATE memories
SET 
    importance_score = 0.1,
    tier = 'cold', 
    recency_score = 0.1
WHERE tier = 'working'
AND status = 'active'
AND (
    content LIKE '%thread % item %'
    OR content LIKE 'Test %'
    OR content LIKE 'Health check%'
);

COMMIT;

-- Show results
SELECT 
    'Tier Distribution After Cleanup' as status,
    tier,
    COUNT(*) as count,
    ROUND(AVG(importance_score)::numeric, 3) as avg_importance
FROM memories
WHERE status = 'active'
GROUP BY tier
ORDER BY 
    CASE tier 
        WHEN 'working' THEN 1
        WHEN 'warm' THEN 2
        WHEN 'cold' THEN 3
        ELSE 4
    END;

-- Show what's left in working memory
SELECT 
    LEFT(content, 80) as content_preview,
    importance_score,
    recency_score
FROM memories
WHERE tier = 'working'
AND status = 'active'
ORDER BY importance_score DESC
LIMIT 10;