#!/bin/bash

# Agentic Memory System - MCP Setup Script for macOS
# This script helps set up the MCP server for Claude Desktop and Claude Code

set -e

echo "🚀 Agentic Memory System - MCP Setup"
echo "====================================="
echo ""

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Get the script directory
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
cd "$SCRIPT_DIR"

# Check if .env file exists
if [ ! -f .env ]; then
    echo -e "${YELLOW}⚠️  No .env file found. Creating from example...${NC}"
    if [ -f .env.example ]; then
        cp .env.example .env
        echo -e "${GREEN}✅ Created .env file. Please edit it with your credentials.${NC}"
        echo "   Run this script again after updating .env"
        exit 0
    else
        echo -e "${RED}❌ No .env.example file found. Please create .env manually.${NC}"
        exit 1
    fi
fi

# Load environment variables
source .env

echo "1. Building release binary..."
cargo build --release
echo -e "${GREEN}✅ Binary built successfully${NC}"

echo ""
echo "2. Validating configuration..."
./target/release/codex-memory mcp validate
if [ $? -ne 0 ]; then
    echo -e "${RED}❌ Configuration validation failed${NC}"
    exit 1
fi

echo ""
echo "3. Testing connectivity..."
./target/release/codex-memory mcp test
if [ $? -ne 0 ]; then
    echo -e "${RED}❌ Connectivity test failed${NC}"
    exit 1
fi

echo ""
echo "4. Setting up Claude Desktop configuration..."
CLAUDE_CONFIG_DIR="$HOME/Library/Application Support/Claude"
CLAUDE_CONFIG_FILE="$CLAUDE_CONFIG_DIR/claude_desktop_config.json"

if [ ! -d "$CLAUDE_CONFIG_DIR" ]; then
    echo -e "${YELLOW}⚠️  Claude Desktop directory not found. Is Claude Desktop installed?${NC}"
else
    # Check if config file exists
    if [ -f "$CLAUDE_CONFIG_FILE" ]; then
        # Check if codex-memory is already configured
        if grep -q "codex-memory" "$CLAUDE_CONFIG_FILE"; then
            echo -e "${GREEN}✅ codex-memory already configured in Claude Desktop${NC}"
        else
            echo -e "${YELLOW}⚠️  Claude Desktop config exists. Please manually add codex-memory configuration.${NC}"
            echo "   Add this to your $CLAUDE_CONFIG_FILE:"
            echo ""
            cat << EOF
    "codex-memory": {
      "command": "$SCRIPT_DIR/target/release/codex-memory",
      "args": ["start"],
      "env": {
        "DATABASE_URL": "$DATABASE_URL",
        "EMBEDDING_PROVIDER": "$EMBEDDING_PROVIDER",
        "EMBEDDING_BASE_URL": "$EMBEDDING_BASE_URL",
        "EMBEDDING_MODEL": "$EMBEDDING_MODEL",
        "HTTP_PORT": "$HTTP_PORT",
        "MCP_PORT": "$MCP_PORT",
        "LOG_LEVEL": "$LOG_LEVEL"
      }
    }
EOF
        fi
    else
        echo "Creating new Claude Desktop configuration..."
        cat > "$CLAUDE_CONFIG_FILE" << EOF
{
  "mcpServers": {
    "codex-memory": {
      "command": "$SCRIPT_DIR/target/release/codex-memory",
      "args": ["start"],
      "env": {
        "DATABASE_URL": "$DATABASE_URL",
        "EMBEDDING_PROVIDER": "$EMBEDDING_PROVIDER",
        "EMBEDDING_BASE_URL": "$EMBEDDING_BASE_URL",
        "EMBEDDING_MODEL": "$EMBEDDING_MODEL",
        "HTTP_PORT": "$HTTP_PORT",
        "MCP_PORT": "$MCP_PORT",
        "LOG_LEVEL": "$LOG_LEVEL"
      }
    }
  }
}
EOF
        echo -e "${GREEN}✅ Created Claude Desktop configuration${NC}"
    fi
fi

echo ""
echo "5. Setting up Claude Code configuration..."
if [ ! -f .mcp.json ]; then
    ./target/release/codex-memory mcp template --template-type production --output .mcp.json
    echo -e "${GREEN}✅ Created .mcp.json for Claude Code${NC}"
else
    echo -e "${GREEN}✅ .mcp.json already exists${NC}"
fi

echo ""
echo "6. Running database migrations..."
./target/release/codex-memory database migrate || echo -e "${YELLOW}⚠️  Migration failed (may need pgvector extension)${NC}"

echo ""
echo "======================================"
echo -e "${GREEN}🎉 Setup Complete!${NC}"
echo ""
echo "Next steps:"
echo "1. Restart Claude Desktop to load the MCP server"
echo "2. Open this project in Claude Code to use project-level MCP"
echo "3. Test by asking Claude to 'remember' something"
echo ""
echo "Useful commands:"
echo "  ./target/release/codex-memory mcp diagnose  # Troubleshoot issues"
echo "  ./target/release/codex-memory health        # Check system health"
echo "  ./target/release/codex-memory start         # Start server manually"