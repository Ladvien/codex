-- ============================================================================
-- FIX TEST DATABASE SCHEMA FOR CODEX MEMORY SYSTEM
-- Run this on your test database at .104
-- ============================================================================

-- 1. First, check if the test database exists and create it if needed
-- (Run this as a superuser on the postgres database if the test db doesn't exist)
-- CREATE DATABASE codex_test;

-- Connect to your test database before running the rest
-- \c codex_test

-- 2. Enable required extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "vector";

-- 3. Create enum types if they don't exist
DO $$ 
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'memory_tier') THEN
        CREATE TYPE memory_tier AS ENUM ('working', 'warm', 'cold');
    END IF;
    
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'memory_status') THEN
        CREATE TYPE memory_status AS ENUM ('active', 'archived', 'deleted');
    END IF;
END $$;

-- 4. Fix the memories table schema
-- First, check if the table exists
DO $$
BEGIN
    -- If table doesn't exist, create it with the correct schema
    IF NOT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'memories') THEN
        CREATE TABLE memories (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            content TEXT NOT NULL,
            content_hash VARCHAR(64) NOT NULL,
            embedding vector(768),
            tier memory_tier NOT NULL DEFAULT 'working',
            status memory_status NOT NULL DEFAULT 'active',
            importance_score FLOAT NOT NULL DEFAULT 0.5 CHECK (importance_score >= 0 AND importance_score <= 1),
            access_count INTEGER NOT NULL DEFAULT 0,
            last_accessed_at TIMESTAMPTZ,  -- Correct column name
            metadata JSONB NOT NULL DEFAULT '{}',
            parent_id UUID REFERENCES memories(id) ON DELETE CASCADE,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            expires_at TIMESTAMPTZ,
            
            -- Constraints
            UNIQUE(content_hash, tier),
            CHECK (char_length(content) <= 1048576)
        );
    ELSE
        -- Table exists, so we need to fix/add missing columns and rename incorrect ones
        
        -- Add missing columns if they don't exist
        ALTER TABLE memories ADD COLUMN IF NOT EXISTS content_hash VARCHAR(64);
        ALTER TABLE memories ADD COLUMN IF NOT EXISTS status VARCHAR(20) DEFAULT 'active';
        ALTER TABLE memories ADD COLUMN IF NOT EXISTS expires_at TIMESTAMPTZ;
        
        -- Rename last_accessed to last_accessed_at if it exists
        DO $inner$
        BEGIN
            IF EXISTS (SELECT 1 FROM information_schema.columns 
                      WHERE table_name = 'memories' AND column_name = 'last_accessed') THEN
                ALTER TABLE memories RENAME COLUMN last_accessed TO last_accessed_at;
            END IF;
        END $inner$;
        
        -- Add last_accessed_at if it doesn't exist at all
        ALTER TABLE memories ADD COLUMN IF NOT EXISTS last_accessed_at TIMESTAMPTZ;
        
        -- Fix importance column name if needed (some schemas have 'importance' instead of 'importance_score')
        DO $inner$
        BEGIN
            IF EXISTS (SELECT 1 FROM information_schema.columns 
                      WHERE table_name = 'memories' AND column_name = 'importance') THEN
                ALTER TABLE memories RENAME COLUMN importance TO importance_score;
            END IF;
        END $inner$;
        
        -- Ensure importance_score exists
        ALTER TABLE memories ADD COLUMN IF NOT EXISTS importance_score FLOAT DEFAULT 0.5;
        
        -- Update content_hash for existing records if it's null
        UPDATE memories 
        SET content_hash = encode(sha256(content::bytea), 'hex')
        WHERE content_hash IS NULL OR content_hash = '';
        
        -- Make content_hash NOT NULL after populating
        ALTER TABLE memories ALTER COLUMN content_hash SET NOT NULL;
        
        -- Add constraints if they don't exist
        DO $inner$
        BEGIN
            IF NOT EXISTS (SELECT 1 FROM pg_constraint 
                          WHERE conname = 'memories_content_hash_tier_key') THEN
                ALTER TABLE memories ADD CONSTRAINT memories_content_hash_tier_key 
                UNIQUE(content_hash, tier);
            END IF;
        EXCEPTION
            WHEN duplicate_object THEN NULL;
            WHEN duplicate_table THEN NULL;
        END $inner$;
        
        -- Fix tier column type if it's just VARCHAR
        DO $inner$
        BEGIN
            -- Check if tier is VARCHAR and needs conversion
            IF EXISTS (SELECT 1 FROM information_schema.columns 
                      WHERE table_name = 'memories' 
                      AND column_name = 'tier' 
                      AND data_type = 'character varying') THEN
                      
                -- First ensure all values are valid
                UPDATE memories SET tier = 'working' 
                WHERE tier NOT IN ('working', 'warm', 'cold');
                
                -- Convert column to enum
                ALTER TABLE memories 
                ALTER COLUMN tier TYPE memory_tier 
                USING tier::memory_tier;
            END IF;
        EXCEPTION
            WHEN others THEN 
                -- If conversion fails, just ensure the column exists
                ALTER TABLE memories ADD COLUMN IF NOT EXISTS tier VARCHAR(20) DEFAULT 'working';
        END $inner$;
        
        -- Fix status column type if needed
        DO $inner$
        BEGIN
            IF EXISTS (SELECT 1 FROM information_schema.columns 
                      WHERE table_name = 'memories' 
                      AND column_name = 'status' 
                      AND data_type = 'character varying') THEN
                      
                UPDATE memories SET status = 'active' 
                WHERE status NOT IN ('active', 'archived', 'deleted');
                
                ALTER TABLE memories 
                ALTER COLUMN status TYPE memory_status 
                USING status::memory_status;
            END IF;
        EXCEPTION
            WHEN others THEN 
                ALTER TABLE memories ADD COLUMN IF NOT EXISTS status VARCHAR(20) DEFAULT 'active';
        END $inner$;
    END IF;
END $$;

-- 5. Create necessary indexes for performance
CREATE INDEX IF NOT EXISTS idx_memories_tier ON memories(tier);
CREATE INDEX IF NOT EXISTS idx_memories_status ON memories(status);
CREATE INDEX IF NOT EXISTS idx_memories_importance ON memories(importance_score);
CREATE INDEX IF NOT EXISTS idx_memories_created_at ON memories(created_at);
CREATE INDEX IF NOT EXISTS idx_memories_last_accessed ON memories(last_accessed_at);
CREATE INDEX IF NOT EXISTS idx_memories_content_hash ON memories(content_hash);
CREATE INDEX IF NOT EXISTS idx_memories_parent_id ON memories(parent_id);

-- Create vector similarity search index (if pgvector is installed)
DO $$
BEGIN
    CREATE INDEX IF NOT EXISTS memories_embedding_idx 
    ON memories USING hnsw (embedding vector_cosine_ops);
EXCEPTION
    WHEN undefined_object THEN
        RAISE NOTICE 'pgvector extension not found, skipping vector index';
END $$;

-- 6. Create migration_history table for tracking
CREATE TABLE IF NOT EXISTS migration_history (
    id SERIAL PRIMARY KEY,
    migration_name VARCHAR(255) NOT NULL,
    applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    checksum VARCHAR(64)
);

-- 7. Record this migration
INSERT INTO migration_history (migration_name, checksum)
VALUES ('fix_test_database_schema', encode(sha256('fix_test_database_2024'::bytea), 'hex'));

-- 8. Create a function to update the updated_at timestamp
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- 9. Create trigger for automatic updated_at updates
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'update_memories_updated_at') THEN
        CREATE TRIGGER update_memories_updated_at
        BEFORE UPDATE ON memories
        FOR EACH ROW
        EXECUTE FUNCTION update_updated_at_column();
    END IF;
END $$;

-- 10. Verify the schema is correct
DO $$
DECLARE
    missing_columns TEXT := '';
    col_record RECORD;
BEGIN
    -- Check for required columns
    FOR col_record IN 
        SELECT column_name FROM (
            VALUES ('id'), ('content'), ('content_hash'), ('embedding'), 
                   ('tier'), ('status'), ('importance_score'), ('access_count'),
                   ('last_accessed_at'), ('metadata'), ('parent_id'), 
                   ('created_at'), ('updated_at'), ('expires_at')
        ) AS required(column_name)
        WHERE NOT EXISTS (
            SELECT 1 FROM information_schema.columns 
            WHERE table_name = 'memories' 
            AND column_name = required.column_name
        )
    LOOP
        missing_columns := missing_columns || col_record.column_name || ', ';
    END LOOP;
    
    IF missing_columns != '' THEN
        RAISE NOTICE 'Warning: Missing columns: %', missing_columns;
    ELSE
        RAISE NOTICE 'Success: All required columns are present!';
    END IF;
END $$;

-- 11. Show final schema
\d memories

-- ============================================================================
-- DONE! Your test database should now be compatible with the test suite
-- ============================================================================