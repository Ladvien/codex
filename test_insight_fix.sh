#!/bin/bash

# Test script to verify the insight generation fixes
echo "Testing insight generation fixes..."

# Source environment
source .env

echo "1. Testing Ollama connectivity..."
curl -s "http://192.168.1.110:11434/api/version" | jq .

echo -e "\n2. Testing available models..."
curl -s "http://192.168.1.110:11434/api/tags" | jq '.models[].name'

echo -e "\n3. Testing database connectivity..."
psql $DATABASE_URL -c "SELECT COUNT(*) as memory_count FROM memories WHERE created_at > NOW() - INTERVAL '1 hour';" 2>/dev/null

echo -e "\n4. Testing insights table structure..."  
psql $DATABASE_URL -c "\\d insights" 2>/dev/null | head -5

echo -e "\n5. Testing MCP generate_insights command..."
echo '{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "generate_insights",
    "arguments": {
      "time_period": "last_hour",
      "max_insights": 3,
      "insight_type": "pattern"
    }
  }
}' | ~/.cargo/bin/codex-memory mcp 2>&1 | head -20

echo -e "\nTest completed!"