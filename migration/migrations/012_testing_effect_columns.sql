-- Migration: Add testing effect and spaced repetition columns
-- These columns support the testing effect (retrieval practice) feature

BEGIN;

-- Add testing effect columns to memories table
ALTER TABLE memories 
ADD COLUMN IF NOT EXISTS total_retrieval_attempts INTEGER DEFAULT 0,
ADD COLUMN IF NOT EXISTS successful_retrievals INTEGER DEFAULT 0,
ADD COLUMN IF NOT EXISTS failed_retrievals INTEGER DEFAULT 0,
ADD COLUMN IF NOT EXISTS last_retrieval_at TIMESTAMPTZ,
ADD COLUMN IF NOT EXISTS retrieval_strength REAL DEFAULT 0.0,
ADD COLUMN IF NOT EXISTS current_interval_days REAL DEFAULT 1.0,
ADD COLUMN IF NOT EXISTS ease_factor REAL DEFAULT 2.5,
ADD COLUMN IF NOT EXISTS next_review_at TIMESTAMPTZ;

-- Create indexes for testing effect queries
CREATE INDEX IF NOT EXISTS idx_memories_next_review_at ON memories(next_review_at) WHERE next_review_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_memories_retrieval_strength ON memories(retrieval_strength);
CREATE INDEX IF NOT EXISTS idx_memories_last_retrieval_at ON memories(last_retrieval_at) WHERE last_retrieval_at IS NOT NULL;

-- Add comment to document the columns
COMMENT ON COLUMN memories.total_retrieval_attempts IS 'Total number of times this memory has been retrieved';
COMMENT ON COLUMN memories.successful_retrievals IS 'Number of successful retrieval attempts';
COMMENT ON COLUMN memories.failed_retrievals IS 'Number of failed retrieval attempts';
COMMENT ON COLUMN memories.last_retrieval_at IS 'Timestamp of the last retrieval attempt';
COMMENT ON COLUMN memories.retrieval_strength IS 'Strength of retrieval based on testing effect (0.0 to 1.0)';
COMMENT ON COLUMN memories.current_interval_days IS 'Current review interval in days for spaced repetition';
COMMENT ON COLUMN memories.ease_factor IS 'Ease factor for spaced repetition algorithm (typically 1.3 to 2.5)';
COMMENT ON COLUMN memories.next_review_at IS 'Next scheduled review time for spaced repetition';

COMMIT;