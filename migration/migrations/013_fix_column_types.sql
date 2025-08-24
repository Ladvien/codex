-- Migration: Fix column types for testing effect fields
-- Fixes type mismatches between Rust f64 and SQL FLOAT4

BEGIN;

-- Fix column types to match Rust f64 (DOUBLE PRECISION)
ALTER TABLE memories 
ALTER COLUMN current_interval_days TYPE DOUBLE PRECISION,
ALTER COLUMN ease_factor TYPE DOUBLE PRECISION;

-- Add missing columns that were not in the original migration
ALTER TABLE memories 
ADD COLUMN IF NOT EXISTS last_retrieval_difficulty DOUBLE PRECISION,
ADD COLUMN IF NOT EXISTS last_retrieval_success BOOLEAN;

-- Add comments for the new columns
COMMENT ON COLUMN memories.last_retrieval_difficulty IS 'Difficulty score of the last retrieval attempt (0.0 to 1.0)';
COMMENT ON COLUMN memories.last_retrieval_success IS 'Whether the last retrieval attempt was successful';

COMMIT;