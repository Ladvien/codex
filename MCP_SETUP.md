# Agentic Memory System - MCP Setup Guide

## Overview

The Agentic Memory System operates as an MCP (Model Context Protocol) server that provides intelligent memory management for Claude Desktop. This guide shows how to securely configure PostgreSQL credentials and other settings.

## macOS Configuration Paths

### Claude Desktop
Configuration file: `~/Library/Application Support/Claude/claude_desktop_config.json`

### Claude Code  
Project-level configuration: `.mcp.json` in project root

## Configuration Methods

### Method 1: Environment Variables (Recommended)

The most secure approach is to use environment variables for sensitive credentials:

#### 1. Create Environment File

Create a `.env` file in your project directory (this file should **never** be committed to git):

```bash
# Database Configuration
DATABASE_URL=postgresql://username:password@hostname:5432/database_name
DB_HOST=your.postgres.host.com
DB_NAME=your_database_name
DB_USER=your_username
DB_PASSWORD=your_secure_password
DB_PORT=5432

# Embedding Service Configuration
EMBEDDING_PROVIDER=ollama
EMBEDDING_BASE_URL=http://localhost:11434
EMBEDDING_MODEL=nomic-embed-text
EMBEDDING_TIMEOUT_SECONDS=60

# Optional: OpenAI API Key (backup embedding provider)
OPENAI_API_KEY=sk-your-openai-api-key-here

# Server Configuration
HTTP_PORT=8080
MCP_PORT=8081
LOG_LEVEL=info

# Memory Tier Configuration
WORKING_TIER_LIMIT=1000
WARM_TIER_LIMIT=10000
WORKING_TO_WARM_DAYS=7
WARM_TO_COLD_DAYS=30
IMPORTANCE_THRESHOLD=0.7

# Operational Configuration
MAX_DB_CONNECTIONS=10
REQUEST_TIMEOUT_SECONDS=30
ENABLE_METRICS=true
```

#### 2. Configure Claude Desktop

In your Claude Desktop configuration file, reference the environment variables:

**For macOS**: `~/Library/Application Support/Claude/claude_desktop_config.json`
**For Windows**: `%APPDATA%\Claude\claude_desktop_config.json`

```json
{
  "mcpServers": {
    "agentic-memory": {
      "command": "/path/to/codex-memory",
      "args": ["start"],
      "env": {
        "DATABASE_URL": "${DATABASE_URL}",
        "EMBEDDING_PROVIDER": "${EMBEDDING_PROVIDER:-ollama}",
        "EMBEDDING_BASE_URL": "${EMBEDDING_BASE_URL:-http://localhost:11434}",
        "EMBEDDING_MODEL": "${EMBEDDING_MODEL:-nomic-embed-text}",
        "LOG_LEVEL": "${LOG_LEVEL:-info}"
      }
    }
  }
}
```

### Method 2: Direct Configuration

For non-sensitive settings, you can configure directly in the MCP config:

```json
{
  "mcpServers": {
    "agentic-memory": {
      "command": "/path/to/codex-memory",
      "args": ["start"],
      "env": {
        "DATABASE_URL": "postgresql://username:password@localhost:5432/memory_db",
        "EMBEDDING_PROVIDER": "ollama",
        "EMBEDDING_BASE_URL": "http://192.168.1.110:11434",
        "EMBEDDING_MODEL": "nomic-embed-text",
        "LOG_LEVEL": "info",
        "HTTP_PORT": "8080",
        "MCP_PORT": "8081"
      }
    }
  }
}
```

### Method 3: Project-Specific Configuration

For project-specific settings, create a `.mcp.json` file in your project root:

```json
{
  "servers": {
    "agentic-memory": {
      "command": "./target/release/codex-memory",
      "args": ["start"],
      "env": {
        "DATABASE_URL": "${PROJECT_DATABASE_URL}",
        "EMBEDDING_PROVIDER": "ollama",
        "EMBEDDING_BASE_URL": "http://localhost:11434",
        "EMBEDDING_MODEL": "nomic-embed-text",
        "LOG_LEVEL": "debug"
      }
    }
  }
}
```

## Security Best Practices

### 1. Credential Storage
- ✅ **DO**: Use environment variables for sensitive data
- ✅ **DO**: Store credentials in system keychain when possible
- ✅ **DO**: Use connection strings with minimal required permissions
- ❌ **DON'T**: Store passwords in configuration files
- ❌ **DON'T**: Commit `.env` files to version control

### 2. Database Security
```bash
# Create dedicated database user with minimal permissions
CREATE USER memory_user WITH PASSWORD 'secure_random_password';
CREATE DATABASE memory_db OWNER memory_user;

# Grant only required permissions
GRANT CONNECT ON DATABASE memory_db TO memory_user;
GRANT USAGE ON SCHEMA public TO memory_user;
GRANT CREATE ON SCHEMA public TO memory_user;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO memory_user;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO memory_user;

# Enable required extensions (as superuser)
\c memory_db postgres
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pgvector";
```

### 3. Network Security
- Use SSL/TLS for database connections: `postgresql://user:pass@host:5432/db?sslmode=require`
- Restrict database access to specific IP addresses
- Use VPN or SSH tunneling for remote databases
- Configure firewall rules appropriately

### 4. Environment Variable Security
```bash
# Set restrictive permissions on .env file
chmod 600 .env

# Add .env to .gitignore
echo ".env" >> .gitignore

# Use a .env.example template for documentation
cp .env .env.example
# Remove actual credentials from .env.example before committing
```

## Configuration Validation

The system includes built-in configuration validation:

```bash
# Test configuration
codex-memory database health

# Validate all settings
codex-memory setup --validate

# Test connectivity
codex-memory health
```

## Troubleshooting

### Common Issues

1. **Connection Refused**
   ```bash
   Error: connection refused
   ```
   - Check if PostgreSQL is running
   - Verify host and port are correct
   - Check firewall settings

2. **Authentication Failed**
   ```bash
   Error: password authentication failed
   ```
   - Verify username and password
   - Check `pg_hba.conf` authentication settings
   - Ensure user exists and has permissions

3. **Database Not Found**
   ```bash
   Error: database "memory_db" does not exist
   ```
   - Create the database first
   - Check database name spelling
   - Verify user has connect permissions

4. **Extension Missing**
   ```bash
   Error: extension "pgvector" is not available
   ```
   - Install pgvector extension on PostgreSQL server
   - Ensure user has CREATE EXTENSION permissions

### Debug Mode

Enable detailed logging for troubleshooting:

```bash
LOG_LEVEL=debug codex-memory start
```

Or in the MCP configuration:
```json
{
  "env": {
    "LOG_LEVEL": "debug"
  }
}
```

## Multiple Database Support

For different environments or projects:

```json
{
  "mcpServers": {
    "memory-dev": {
      "command": "/path/to/codex-memory",
      "args": ["start"],
      "env": {
        "DATABASE_URL": "${DEV_DATABASE_URL}",
        "MCP_PORT": "8081"
      }
    },
    "memory-prod": {
      "command": "/path/to/codex-memory", 
      "args": ["start"],
      "env": {
        "DATABASE_URL": "${PROD_DATABASE_URL}",
        "MCP_PORT": "8082"
      }
    }
  }
}
```

## Health Monitoring

The MCP server provides health endpoints:

```bash
# Check overall system health
curl -X POST http://localhost:8081 -H "Content-Type: application/json" -d '{
  "jsonrpc": "2.0",
  "method": "health",
  "id": 1
}'

# Get performance metrics
curl -X POST http://localhost:8081 -H "Content-Type: application/json" -d '{
  "jsonrpc": "2.0", 
  "method": "metrics",
  "id": 1
}'
```

## MCP Command Line Tools

The Agentic Memory System provides built-in MCP management commands:

### Validate Configuration
```bash
codex-memory mcp validate
```
Validates your MCP configuration and reports any issues.

### Generate Diagnostic Report
```bash
codex-memory mcp diagnose
```
Creates a comprehensive diagnostic report for troubleshooting.

### Test Connectivity
```bash
codex-memory mcp test
```
Tests database and embedding service connectivity.

### Generate Configuration Templates
```bash
# Basic template
codex-memory mcp template --template-type basic

# Production template with environment variables
codex-memory mcp template --template-type production

# Development template with mock services
codex-memory mcp template --template-type development

# Save template to file
codex-memory mcp template --template-type production --output claude_desktop_config.json
```

## Quick Setup Guide

### 1. Install and Build
```bash
git clone <your-repo>
cd codex-memory
cargo build --release
```

### 2. Create Environment File
```bash
cat > .env << 'EOF'
DATABASE_URL=postgresql://username:password@localhost:5432/memory_db
EMBEDDING_PROVIDER=ollama
EMBEDDING_BASE_URL=http://localhost:11434
EMBEDDING_MODEL=nomic-embed-text
LOG_LEVEL=info
EOF
```

### 3. Test Configuration
```bash
./target/release/codex-memory mcp validate
./target/release/codex-memory mcp test
```

### 4. Generate MCP Config
```bash
./target/release/codex-memory mcp template --template-type production --output ~/Library/Application\ Support/Claude/claude_desktop_config.json
```

### 5. Update Claude Desktop Config
Edit the generated config file to use your actual paths and environment variables.

## Getting Help

- **Check logs**: The system provides detailed logging for troubleshooting
- **Validate config**: `codex-memory mcp validate`
- **Diagnose issues**: `codex-memory mcp diagnose`  
- **Test connectivity**: `codex-memory mcp test`
- **Generate templates**: `codex-memory mcp template --help`

For more advanced configuration options, see the [Configuration Reference](CONFIG.md).