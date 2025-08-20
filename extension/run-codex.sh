#!/bin/bash
# Wrapper script for Codex Memory MCP Server
# This script translates Claude Desktop configuration to environment variables

# Set up environment from Claude Desktop config
# Claude Desktop passes config as environment variables with MCP_ prefix

# Database configuration
if [ -n "$MCP_DATABASE_URL" ]; then
    export DATABASE_URL="$MCP_DATABASE_URL"
fi

# Embedding configuration
if [ -n "$MCP_EMBEDDING_PROVIDER" ]; then
    export EMBEDDING_PROVIDER="$MCP_EMBEDDING_PROVIDER"
fi

if [ -n "$MCP_EMBEDDING_MODEL" ]; then
    export EMBEDDING_MODEL="$MCP_EMBEDDING_MODEL"
fi

if [ -n "$MCP_OPENAI_API_KEY" ]; then
    export OPENAI_API_KEY="$MCP_OPENAI_API_KEY"
fi

if [ -n "$MCP_OLLAMA_BASE_URL" ]; then
    export OLLAMA_BASE_URL="$MCP_OLLAMA_BASE_URL"
else
    export OLLAMA_BASE_URL="http://localhost:11434"
fi

# Tier configuration
if [ -n "$MCP_WORKING_TIER_LIMIT" ]; then
    export WORKING_TIER_LIMIT="$MCP_WORKING_TIER_LIMIT"
fi

if [ -n "$MCP_WARM_TIER_LIMIT" ]; then
    export WARM_TIER_LIMIT="$MCP_WARM_TIER_LIMIT"
fi

if [ -n "$MCP_ENABLE_AUTO_TIERING" ]; then
    export ENABLE_AUTO_TIERING="$MCP_ENABLE_AUTO_TIERING"
fi

# Operational configuration
if [ -n "$MCP_ENABLE_METRICS" ]; then
    export ENABLE_METRICS="$MCP_ENABLE_METRICS"
fi

if [ -n "$MCP_LOG_LEVEL" ]; then
    export RUST_LOG="$MCP_LOG_LEVEL"
else
    export RUST_LOG="info"
fi

# Get the directory where this script is located
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Execute the binary with stdio mode
exec "${SCRIPT_DIR}/codex-memory" mcp-stdio --skip-setup