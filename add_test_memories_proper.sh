#!/bin/bash

# Script to add test memories via proper MCP protocol

echo "Adding test memories via MCP protocol..."

# Function to store a memory using proper MCP format
store_memory() {
    local content="$1"
    local importance="$2"
    
    # Create a proper JSON-RPC request
    local request=$(cat <<EOF
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"store_memory","arguments":{"content":"$content","importance_score":$importance,"tier":"working"}}}
EOF
)
    
    echo "Storing: ${content:0:50}..."
    
    # Send the request and capture response
    echo "$request" | timeout 5 codex-memory mcp-stdio --skip-setup 2>/dev/null | head -1
    
    # Small delay to avoid overwhelming the system
    sleep 0.5
}

# Add test memories one by one
store_memory "Successfully implemented Codex Dreams insight generation with Rust and PostgreSQL" 0.9
store_memory "Learned that Arc RwLock is more efficient than Arc Mutex for read-heavy workloads" 0.85
store_memory "Most bugs occur in async edge cases - need better error handling for tokio" 0.75
store_memory "Completed MCP integration for Claude Desktop supporting manual insight generation" 0.8
store_memory "Chose manual-only insights by default to respect privacy and user control" 0.9
store_memory "Fixed OllamaClient security to allow private network IPs and env URLs" 0.7
store_memory "Database queries with pgvector perform under 100ms for 1000+ memories" 0.8
store_memory "TodoWrite tool helps track complex tasks and improves completion rate" 0.75
store_memory "E2E tests caught integration issues that unit tests missed" 0.85
store_memory "User feedback about empty insights table revealed placeholder handlers" 0.9

echo ""
echo "Test memories submission complete!"
echo "Checking memory count in database..."

# Verify memories were stored
source .env
MEMORY_COUNT=$(psql "$DATABASE_URL" -t -c "SELECT COUNT(*) FROM memories WHERE created_at > NOW() - INTERVAL '1 hour';" 2>/dev/null | tr -d ' ')

echo "Memories added in last hour: $MEMORY_COUNT"