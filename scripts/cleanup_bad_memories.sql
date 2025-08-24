-- Cleanup script for memories that were corrupted with "About to be deleted"
-- These appear to be test data that was improperly handled

BEGIN;

-- First, let's see how many we're dealing with
SELECT COUNT(*) as total_corrupted 
FROM memories 
WHERE content = 'About to be deleted';

-- Check if any have status other than 'deleted'
SELECT status, COUNT(*) as count
FROM memories
WHERE content = 'About to be deleted'
GROUP BY status;

-- These are clearly test data that should be removed completely
-- The metadata shows they were marked as temporary but the cleanup failed
DELETE FROM memories
WHERE content = 'About to be deleted'
AND metadata->>'temporary' = 'true';

-- Show what we deleted
SELECT 'Deleted ' || COUNT(*) || ' corrupted test memories' as result
FROM memories
WHERE content = 'About to be deleted';

COMMIT;

-- Final check
SELECT COUNT(*) as remaining_corrupted
FROM memories
WHERE content = 'About to be deleted';