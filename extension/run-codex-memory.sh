#!/bin/bash
# Wrapper script for codex-memory MCP server

# Set required environment variables
export DATABASE_URL="postgresql://codex_user:MZSfXiLr5uR3QYbRwv2vTzi22SvFkj4a@192.168.1.104:5432/codex_db"
export EMBEDDING_PROVIDER="ollama"
export EMBEDDING_MODEL="nomic-embed-text"
export EMBEDDING_BASE_URL="http://192.168.1.110:11434"
export RUST_LOG="info"

# Get the directory where this script is located
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"

# Execute the binary from the same directory
exec "$SCRIPT_DIR/codex-memory" "$@"