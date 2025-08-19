# Agentic Memory System Setup Guide

This guide will help you set up the Agentic Memory System with automatic embedding model management and database configuration.

## Quick Start

The easiest way to get started is with the automated setup script:

```bash
# Run complete setup
./scripts/setup.sh

# Or run individual setup steps
./scripts/setup.sh deps      # Check dependencies
./scripts/setup.sh database  # Setup database only
./scripts/setup.sh models    # Setup embedding models only
./scripts/setup.sh health    # Run health checks
```

## Manual Setup

### 1. Prerequisites

- **Rust**: Install from [rustup.rs](https://rustup.rs/)
- **PostgreSQL**: Version 12+ with pgvector extension
- **Ollama**: Running at accessible host (default: `192.168.1.110:11434`)

### 2. Configuration

Create your configuration file:

```bash
# Generate sample configuration
cargo run --bin codex-memory init-config

# Copy and customize
cp .env.example .env
```

Edit `.env` with your specific settings:

```env
# Database Configuration
DATABASE_URL=postgresql://postgres:postgres@localhost:5432/codex_memory

# Embedding Configuration
EMBEDDING_PROVIDER=ollama
EMBEDDING_MODEL=nomic-embed-text
EMBEDDING_BASE_URL=http://192.168.1.110:11434

# Server Configuration
HTTP_PORT=8080
LOG_LEVEL=info
```

### 3. Database Setup

The system can automatically set up your database:

```bash
# Complete database setup (creates DB, installs pgvector, runs migrations)
cargo run --bin codex-memory database setup

# Check database health
cargo run --bin codex-memory database health

# Run migrations only
cargo run --bin codex-memory database migrate
```

### 4. Embedding Model Setup

The system will automatically detect and pull suitable embedding models:

```bash
# Run complete setup (includes model detection and pulling)
cargo run --bin codex-memory setup

# List available models
cargo run --bin codex-memory models

# Setup models only (skip database)
cargo run --bin codex-memory setup --skip-database
```

### 5. Health Checks

Verify everything is working:

```bash
# Quick health check
cargo run --bin codex-memory health

# Detailed health check
cargo run --bin codex-memory health --detailed
```

### 6. Start the Server

```bash
# Start with pre-flight checks
cargo run --bin codex-memory start

# Skip setup checks and start immediately
cargo run --bin codex-memory start --skip-setup
```

## Automatic Model Management

The system includes intelligent embedding model management:

### Supported Models

**Recommended (High Performance):**
- `nomic-embed-text` (768D) - High-quality text embeddings
- `mxbai-embed-large` (1024D) - Large multilingual embeddings

**Compatible Models:**
- `all-minilm` (384D) - Compact sentence embeddings
- `all-mpnet-base-v2` (768D) - Sentence transformer embeddings
- `bge-small-en` (384D) - BGE small English embeddings
- `bge-base-en` (768D) - BGE base English embeddings
- `e5-base` (768D) - E5 base embeddings

### Auto-Detection Features

1. **Model Discovery**: Automatically scans Ollama for available embedding models
2. **Intelligent Selection**: Prefers recommended models, falls back to available ones
3. **Automatic Pulling**: Downloads recommended models if none are available
4. **Fallback Support**: Includes fallback model chains for reliability
5. **Health Monitoring**: Continuously monitors embedding service health

### Manual Model Management

```bash
# Pull specific models via Ollama
ollama pull nomic-embed-text
ollama pull mxbai-embed-large
ollama pull all-minilm

# List available models
ollama list

# Check what the system detects
cargo run --bin codex-memory models
```

## Database Setup Details

### Automatic Database Setup

The system can automatically:

1. **Create Database**: Creates the target database if it doesn't exist
2. **Install pgvector**: Installs and configures the pgvector extension
3. **Run Migrations**: Creates all required tables and indexes
4. **Verify Setup**: Tests vector operations and connectivity

### Manual Database Setup

If you prefer manual setup:

```sql
-- Create database
CREATE DATABASE codex_memory;

-- Connect to the database
\c codex_memory

-- Install pgvector extension
CREATE EXTENSION vector;

-- Verify installation
SELECT vector_dims('[1,2,3]'::vector);
```

Then run migrations:

```bash
cargo run --bin codex-memory database migrate
```

### Database Health Monitoring

The system continuously monitors:

- **Connectivity**: Database connection status
- **pgvector**: Extension availability and functionality
- **Schema**: Required tables and indexes
- **Memory Count**: Current memory storage statistics

## Troubleshooting

### Common Issues

**1. Ollama Connection Failed**
```bash
# Check if Ollama is running
curl http://192.168.1.110:11434/api/tags

# Update Ollama host in .env
EMBEDDING_BASE_URL=http://your-ollama-host:11434
```

**2. Database Connection Issues**
```bash
# Test PostgreSQL connectivity
psql postgresql://postgres:postgres@localhost:5432/postgres

# Check if database exists
cargo run --bin codex-memory database health
```

**3. pgvector Extension Missing**
```bash
# Install pgvector (macOS with Homebrew)
brew install pgvector

# Install pgvector (Ubuntu/Debian)
apt install postgresql-15-pgvector

# Manual installation from source
git clone https://github.com/pgvector/pgvector.git
cd pgvector
make
sudo make install
```

**4. No Embedding Models Available**
```bash
# Pull recommended models
ollama pull nomic-embed-text
ollama pull mxbai-embed-large

# Verify models are available
ollama list
cargo run --bin codex-memory models
```

### Health Check Diagnostics

```bash
# Run comprehensive diagnostics
cargo run --bin codex-memory health --detailed

# Check individual components
cargo run --bin codex-memory database health
./scripts/setup.sh health
```

### Reset and Cleanup

```bash
# Reset database (⚠️ DESTRUCTIVE)
cargo run --bin codex-memory database setup --force

# Clean build artifacts
cargo clean

# Rebuild everything
cargo build --release
```

## Performance Optimization

### Recommended Settings

**For Development:**
```env
LOG_LEVEL=debug
MAX_DB_CONNECTIONS=5
WORKING_TIER_LIMIT=100
```

**For Production:**
```env
LOG_LEVEL=info
MAX_DB_CONNECTIONS=20
WORKING_TIER_LIMIT=10000
ENABLE_METRICS=true
```

### Database Optimization

```sql
-- Optimize PostgreSQL for vector operations
ALTER SYSTEM SET shared_preload_libraries = 'pg_stat_statements';
ALTER SYSTEM SET max_connections = 100;
ALTER SYSTEM SET shared_buffers = '256MB';
ALTER SYSTEM SET effective_cache_size = '1GB';
ALTER SYSTEM SET maintenance_work_mem = '64MB';
```

## Integration Examples

### Basic API Usage

```bash
# Health check
curl http://localhost:8080/health

# Create a memory
curl -X POST http://localhost:8080/api/v1/memories \
  -H "Content-Type: application/json" \
  -d '{"content": "This is a test memory", "metadata": {"type": "test"}}'

# Search memories
curl -X POST http://localhost:8080/api/v1/memories/search \
  -H "Content-Type: application/json" \
  -d '{"query_text": "test", "limit": 10}'
```

### MCP Server Usage

If MCP server is enabled:

```bash
# MCP server runs on separate port (default: 8081)
# Connect via MCP client tools
```

## Advanced Configuration

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `DATABASE_URL` | PostgreSQL connection string | `postgresql://postgres:postgres@localhost:5432/codex_memory` |
| `EMBEDDING_PROVIDER` | Embedding provider (`ollama`, `openai`, `mock`) | `ollama` |
| `EMBEDDING_MODEL` | Model name for embeddings | `nomic-embed-text` |
| `EMBEDDING_BASE_URL` | Embedding service URL | `http://192.168.1.110:11434` |
| `HTTP_PORT` | HTTP server port | `8080` |
| `MCP_PORT` | MCP server port (optional) | `None` |
| `LOG_LEVEL` | Logging level | `info` |
| `MAX_DB_CONNECTIONS` | Database connection pool size | `10` |
| `WORKING_TIER_LIMIT` | Working memory limit | `1000` |
| `WARM_TIER_LIMIT` | Warm memory limit | `10000` |

### Security Configuration

```env
# Enable TLS (requires certificates)
TLS_ENABLED=true
TLS_CERT_PATH=/path/to/cert.pem
TLS_KEY_PATH=/path/to/key.pem

# Enable authentication
AUTH_ENABLED=true
JWT_SECRET=your-secret-key

# Enable rate limiting
RATE_LIMITING_ENABLED=true
REQUESTS_PER_MINUTE=100
```

## Next Steps

After successful setup:

1. **Explore the API**: Try the REST endpoints for memory management
2. **Monitor Performance**: Check `/metrics` endpoint if enabled
3. **Scale Configuration**: Adjust memory limits and connection pools
4. **Backup Strategy**: Implement regular database backups
5. **Integration**: Connect with your AI agents or applications

For detailed API documentation, see `docs/api_reference.md`.
For operational procedures, see `docs/operations_runbook.md`.