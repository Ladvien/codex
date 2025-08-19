#!/bin/bash

# Start the Agentic Memory System MCP Server

set -e

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Get the script directory
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
cd "$SCRIPT_DIR"

# Check if .env file exists
if [ ! -f .env ]; then
    echo -e "${YELLOW}⚠️  No .env file found. Run setup_mcp.sh first.${NC}"
    exit 1
fi

# Load environment variables
source .env

echo "🚀 Starting Agentic Memory System MCP Server"
echo "============================================"
echo ""
echo "Configuration:"
echo "  Database: ${DATABASE_URL%@*}@***"
echo "  Embedding: $EMBEDDING_PROVIDER ($EMBEDDING_MODEL)"
echo "  HTTP Port: $HTTP_PORT"
echo "  MCP Port: $MCP_PORT"
echo "  Log Level: $LOG_LEVEL"
echo ""
echo -e "${GREEN}Starting server...${NC}"
echo "Press Ctrl+C to stop"
echo ""

# Start the server
exec ./target/release/codex-memory start