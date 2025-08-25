#!/bin/bash

# Script to add test memories for insight generation testing

echo "Adding test memories to the database..."

# Function to create an MCP request for storing a memory
store_memory() {
    local content="$1"
    local tags="$2"
    local importance="$3"
    
    cat <<EOF | codex-memory mcp-stdio --skip-setup 2>/dev/null
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "store_memory",
    "arguments": {
      "content": "$content",
      "tags": [$tags],
      "importance_score": $importance,
      "tier": "working"
    }
  }
}
EOF
}

# Add various test memories with different themes
echo "1. Adding coding memory..."
store_memory "Successfully implemented the Codex Dreams insight generation system using Rust and PostgreSQL. The system uses Ollama for local LLM processing and can identify patterns in stored memories." '"coding", "rust", "achievement"' 0.9

echo "2. Adding learning memory..."
store_memory "Learned that Arc<RwLock<T>> is more efficient than Arc<Mutex<T>> for read-heavy workloads in Rust. This resulted in 40% performance improvement in our API." '"learning", "rust", "performance"' 0.85

echo "3. Adding pattern observation..."
store_memory "Noticed that most bugs occur when handling edge cases in async code. Need to add more comprehensive error handling for tokio runtime errors." '"pattern", "debugging", "async"' 0.75

echo "4. Adding project milestone..."
store_memory "Completed the MCP integration for Claude Desktop and Claude Code. The system now supports manual insight generation through natural language commands." '"milestone", "project", "mcp"' 0.8

echo "5. Adding technical decision..."
store_memory "Decided to use manual-only insight generation by default to respect user privacy and give full control over when processing occurs. Automation remains optional via cron scripts." '"decision", "architecture", "privacy"' 0.9

echo "6. Adding debugging insight..."
store_memory "Fixed the OllamaClient security restriction that was blocking non-localhost URLs. The solution was to allow private network IPs and environment-configured URLs." '"debugging", "fix", "security"' 0.7

echo "7. Adding performance observation..."
store_memory "Database queries with pgvector are performing well under 100ms for semantic search across 1000+ memories. HNSW indexing is proving effective." '"performance", "database", "pgvector"' 0.8

echo "8. Adding workflow improvement..."
store_memory "Using TodoWrite tool consistently helps track complex multi-step tasks and improves completion rate. Should use it for all non-trivial implementations." '"workflow", "productivity", "tools"' 0.75

echo "9. Adding testing insight..."
store_memory "E2E tests are crucial for catching integration issues between MCP handlers and the insights processor. Unit tests alone missed the placeholder implementation bug." '"testing", "quality", "e2e"' 0.85

echo "10. Adding collaboration note..."
store_memory "User feedback about empty insights table led to discovering that MCP handlers were using placeholders. Always verify end-to-end functionality, not just compilation." '"collaboration", "feedback", "debugging"' 0.9

echo "Test memories added successfully!"
echo "The insight generator should now have data to process."