# How Codex Memory Works

## Overview
Codex Memory is a **passive storage system** that only stores information when explicitly instructed. It does NOT automatically collect or store memories without your knowledge.

## Key Principles

### 1. Explicit Storage Only
- **No automatic collection**: The system NEVER stores memories without an explicit request
- **User-initiated**: Memories are only created when you or an AI assistant explicitly calls the `store_memory` function
- **Transparent operation**: Every memory storage action is visible and intentional

### 2. How Memories Are Created

#### Manual Storage via AI Assistant
When you tell Claude (or another AI using the MCP integration):
- "Remember that my favorite color is black"
- "Store the fact that I prefer Python over Java"
- "Keep track of this project's main goal"

The AI will explicitly call the `store_memory` tool, and you'll see a confirmation like:
```
Successfully stored memory with ID: 3ed88419-0204-4716-8fe1-7149e52bc455
```

#### No Background Collection
The system does NOT:
- Monitor your conversations automatically
- Store information without being asked
- Create memories from your file system or activities
- Track your behavior patterns
- Listen to other applications

## Architecture Components

### 1. Storage Tiers
The system uses three tiers to organize memories by importance and access frequency:

```
┌─────────────────────────────────────────────┐
│         WORKING TIER (Hot Cache)            │
│   • Most frequently accessed memories       │
│   • Fastest retrieval (<1ms)                │
│   • Limited capacity (default: 1000)        │
└─────────────────────────────────────────────┘
                      ↕
┌─────────────────────────────────────────────┐
│            WARM TIER (Medium)               │
│   • Moderately accessed memories            │
│   • Quick retrieval (<100ms)                │
│   • Medium capacity (default: 10000)        │
└─────────────────────────────────────────────┘
                      ↕
┌─────────────────────────────────────────────┐
│          COLD TIER (Archive)                │
│   • Rarely accessed memories                │
│   • Slower retrieval (<20s)                 │
│   • Unlimited capacity                      │
│   • Compressed storage                      │
└─────────────────────────────────────────────┘
```

### 2. Automatic Tier Migration
While storage is manual, the system does automatically optimize WHERE memories are kept:
- **Promotion**: Frequently accessed memories move to faster tiers
- **Demotion**: Unused memories gradually move to slower, cheaper storage
- **Transparent**: This optimization doesn't create new memories, just reorganizes existing ones

### 3. Semantic Search with Embeddings
Each memory is converted to a mathematical representation (embedding) for intelligent search:
```
"I like my family" → [0.23, -0.45, 0.78, ...] (1536 dimensions)
```
This allows finding related memories even with different wording:
- Search: "relatives" → Finds: "I like my family"
- Search: "preferences" → Finds: "favorite color is black"

## Data Flow

### Storing a Memory
```
1. User: "Remember that I'm working on a Rust project"
       ↓
2. AI Assistant: Calls store_memory("I'm working on a Rust project")
       ↓
3. Codex Memory:
   a. Generates embedding using Ollama/OpenAI
   b. Checks for duplicates
   c. Stores in PostgreSQL database
   d. Returns confirmation with memory ID
       ↓
4. User sees: "Successfully stored memory with ID: [uuid]"
```

### Searching Memories
```
1. User: "What programming language am I using?"
       ↓
2. AI Assistant: Calls search_memory("programming language")
       ↓
3. Codex Memory:
   a. Generates embedding for search query
   b. Finds similar memories using pgvector
   c. Returns ranked results
       ↓
4. User sees: "Found: I'm working on a Rust project"
```

## Privacy and Security Features

### What's Stored
- **Content**: The exact text you ask to store
- **Metadata**: Timestamp, access count, importance score
- **Embedding**: Mathematical representation for search
- **NO personal data** unless you explicitly store it

### What's NOT Stored
- Conversations that aren't explicitly marked for storage
- File contents from your system
- Browsing history or application usage
- Any data you don't explicitly provide

### Security Measures
```rust
// Built-in protections:
- PII Detection: Warns before storing sensitive data
- Input validation: Prevents SQL injection
- Rate limiting: Prevents abuse
- Encryption: At-rest and in-transit
- Access control: Only you can access your memories
```

## MCP Integration with Claude Desktop

### How It Works
1. **Claude Desktop** loads the MCP server from the extension
2. **MCP Server** connects to your PostgreSQL database
3. **Tools Available**:
   - `store_memory`: Save new information
   - `search_memory`: Find related memories
   - `get_statistics`: View system metrics

### Important: No Automatic Storage
Even with Claude Desktop integration:
- Claude does NOT automatically remember everything
- You must explicitly ask Claude to "remember" something
- Each storage action requires the `store_memory` tool call
- You control what gets stored

## Common Misconceptions

### ❌ MYTH: "It's recording everything I do"
**✅ REALITY**: Only stores what you explicitly tell it to store

### ❌ MYTH: "It learns from my behavior"
**✅ REALITY**: It's a passive database, not a learning system

### ❌ MYTH: "Claude automatically builds a profile of me"
**✅ REALITY**: Claude only stores memories when you say "remember this"

### ❌ MYTH: "Memories are shared between applications"
**✅ REALITY**: Memories are private to your database instance

## Usage Examples

### Explicit Storage (This WILL store)
```
You: "Remember that my dog's name is Max"
Claude: "I'll store that information for you."
→ Creates memory: "My dog's name is Max"
```

### Normal Conversation (This will NOT store)
```
You: "My dog Max loves walks"
Claude: "That's wonderful! Dogs need regular exercise."
→ No memory created (unless you ask to remember it)
```

### Searching Existing Memories
```
You: "What's my dog's name?"
Claude: [Searches memories] "Your dog's name is Max"
→ No new memory created, just retrieves existing one
```

## Database Schema

The system stores memories in PostgreSQL with this structure:

```sql
CREATE TABLE memories (
    id UUID PRIMARY KEY,
    content TEXT NOT NULL,           -- What you asked to remember
    content_hash VARCHAR(64),         -- Duplicate detection
    embedding vector(1536),           -- For semantic search
    importance_score FLOAT,           -- 0.0 to 1.0
    access_count INTEGER DEFAULT 0,   -- Track usage
    tier VARCHAR(20),                 -- working/warm/cold
    created_at TIMESTAMP,             -- When created
    last_accessed_at TIMESTAMP,       -- Last retrieval
    metadata JSONB,                   -- Tags, categories, etc.
    -- No automatic data collection fields
);
```

## Monitoring and Control

### View Statistics
```bash
# Check what's stored
echo '{"jsonrpc":"2.0","method":"tools/call","params":{"name":"get_statistics","arguments":{}},"id":1}' | codex-memory mcp-stdio --skip-setup

# Returns:
📊 Total Active: 8187
📊 Total Deleted: 595
🔥 Working Tier: 5271
🌡️ Warm Tier: 1518
🧊 Cold Tier: 1398
```

### Direct Database Access
```bash
# Connect to your database
psql "postgresql://codex_user@192.168.1.104:5432/codex_db"

# View recent memories
SELECT id, content, created_at FROM memories 
ORDER BY created_at DESC LIMIT 10;

# Delete specific memory
DELETE FROM memories WHERE id = 'uuid-here';

# Clear all memories (use with caution!)
TRUNCATE TABLE memories;
```

## Configuration

### Environment Variables
```bash
# .env file
DATABASE_URL=postgresql://user:pass@host/db  # Your database
EMBEDDING_PROVIDER=ollama                     # Local AI
EMBEDDING_MODEL=nomic-embed-text             # Model for embeddings
AUTO_MIGRATE=false                            # No automatic changes
```

### Tier Limits
```json
{
  "working_tier_limit": 1000,  // Hot cache size
  "warm_tier_limit": 10000,     // Medium storage
  "enable_auto_tiering": true   // Optimize storage location
}
```

## Summary

**Codex Memory is a tool, not a spy.** It's like a notebook that only writes when you hand it the pen. It provides:

1. **Explicit control**: You decide what to store
2. **Transparent operation**: Every action is visible
3. **Semantic search**: Find related information intelligently
4. **Efficient storage**: Automatic organization, not collection
5. **Privacy-first**: No background monitoring or profiling

The system enhances AI assistants by giving them a way to remember what you WANT them to remember, nothing more, nothing less.