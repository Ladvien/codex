# Agentic Memory System

An intelligent, tiered memory management system for Claude AI agents, implementing hierarchical storage with semantic search capabilities. Built as an MCP (Model Context Protocol) server for seamless integration with Claude Desktop and Claude Code.

## Features

- 🧠 **Hierarchical Memory Tiers**: Working, Warm, and Cold storage with automatic migration
- 🔍 **Semantic Search**: Vector-based similarity search using pgvector
- 🚀 **MCP Integration**: Native support for Claude Desktop and Claude Code
- 📊 **Performance Monitoring**: Built-in metrics, health checks, and observability
- 🔒 **Security First**: Secure credential handling, encryption support, audit logging
- 🎯 **Flexible Embeddings**: Support for Ollama (local), OpenAI, or mock providers
- 💾 **PostgreSQL Backend**: Robust storage with pgvector for vector operations
- 🛡️ **Production Ready**: Comprehensive error handling, retries, and circuit breakers

## Quick Start

### Prerequisites

- Rust 1.70+ and Cargo
- PostgreSQL 15+ with pgvector extension
- Ollama (for local embeddings) or OpenAI API key
- macOS (for Claude Desktop/Code integration)

### Installation

```bash
# Clone the repository
git clone https://github.com/yourusername/codex-memory.git
cd codex-memory

# Build the release binary
cargo build --release

# Run setup validation
./target/release/codex-memory mcp validate
```

### Configuration

1. **Copy the example environment file:**
```bash
cp .env.example .env
```

2. **Edit `.env` with your credentials:**
```env
DATABASE_URL=postgresql://username:password@localhost:5432/memory_db
EMBEDDING_PROVIDER=ollama
EMBEDDING_BASE_URL=http://localhost:11434
EMBEDDING_MODEL=nomic-embed-text
```

3. **Test your configuration:**
```bash
./target/release/codex-memory mcp test
```

## MCP Integration

### Claude Desktop Setup (macOS)

1. **Generate configuration template:**
```bash
./target/release/codex-memory mcp template --template-type production --output ~/Desktop/mcp-config.json
```

2. **Add to Claude Desktop configuration:**

Edit `~/Library/Application Support/Claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "codex-memory": {
      "command": "/Users/yourusername/codex/target/release/codex-memory",
      "args": ["start"],
      "env": {
        "DATABASE_URL": "postgresql://codex_user:password@192.168.1.104:5432/codex_db",
        "EMBEDDING_PROVIDER": "ollama",
        "EMBEDDING_BASE_URL": "http://192.168.1.110:11434",
        "EMBEDDING_MODEL": "nomic-embed-text",
        "LOG_LEVEL": "info"
      }
    }
  }
}
```

3. **Restart Claude Desktop** to load the new MCP server.

### Claude Code Setup (Project-Level)

1. **Create `.mcp.json` in your project root:**

```json
{
  "servers": {
    "codex-memory": {
      "command": "./target/release/codex-memory",
      "args": ["start"],
      "env": {
        "DATABASE_URL": "${DATABASE_URL}",
        "EMBEDDING_PROVIDER": "${EMBEDDING_PROVIDER:-ollama}",
        "EMBEDDING_BASE_URL": "${EMBEDDING_BASE_URL:-http://localhost:11434}",
        "EMBEDDING_MODEL": "${EMBEDDING_MODEL:-nomic-embed-text}",
        "LOG_LEVEL": "debug"
      }
    }
  }
}
```

2. **The MCP server will automatically start** when you open the project in Claude Code.

## Database Setup

### PostgreSQL with pgvector

1. **Install PostgreSQL and pgvector:**
```bash
# macOS (Homebrew)
brew install postgresql@15
brew install pgvector

# Ubuntu/Debian
sudo apt install postgresql-15 postgresql-15-pgvector

# From source
git clone https://github.com/pgvector/pgvector.git
cd pgvector && make && sudo make install
```

2. **Create database and user:**
```sql
CREATE USER codex_user WITH PASSWORD 'secure_password';
CREATE DATABASE codex_db OWNER codex_user;
\c codex_db
CREATE EXTENSION IF NOT EXISTS vector;
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
```

3. **Run migrations:**
```bash
./target/release/codex-memory database migrate
```

## Embedding Setup

### Option 1: Ollama (Recommended for Local Development)

1. **Install Ollama:**
```bash
# macOS
brew install ollama

# Linux
curl -fsSL https://ollama.ai/install.sh | sh
```

2. **Start Ollama and pull embedding model:**
```bash
ollama serve
ollama pull nomic-embed-text
```

### Option 2: OpenAI

Set your API key in `.env`:
```env
EMBEDDING_PROVIDER=openai
OPENAI_API_KEY=sk-your-api-key-here
EMBEDDING_MODEL=text-embedding-3-small
```

## Usage

### Start the MCP Server

```bash
# Using environment file
./target/release/codex-memory start

# Or with explicit configuration
DATABASE_URL="postgresql://..." ./target/release/codex-memory start
```

### CLI Commands

```bash
# MCP Management
codex-memory mcp validate     # Validate configuration
codex-memory mcp diagnose     # Generate diagnostic report
codex-memory mcp test         # Test connectivity
codex-memory mcp template     # Generate config templates

# Database Management  
codex-memory database setup   # Setup database with extensions
codex-memory database health  # Check database health
codex-memory database migrate # Run migrations

# System Management
codex-memory setup            # Run interactive setup
codex-memory health           # Check system health
codex-memory models           # List available embedding models
```

### Memory Operations (via MCP)

Once integrated with Claude, you can use natural language to interact with the memory system:

- "Remember that the user prefers dark mode"
- "What do you know about my database configuration?"
- "Search for memories about API endpoints"
- "Forget everything about passwords"

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                  Claude Desktop/Code                 │
│                         (MCP)                        │
└─────────────────────┬───────────────────────────────┘
                      │
┌─────────────────────┴───────────────────────────────┐
│              Agentic Memory System                   │
│                                                      │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────┐  │
│  │   Working    │  │     Warm     │  │   Cold   │  │
│  │   Memory     │◄─┤    Memory    │◄─┤  Memory  │  │
│  │  (Hot tier)  │  │ (Medium tier)│  │(Archive) │  │
│  └──────────────┘  └──────────────┘  └──────────┘  │
│                                                      │
│  ┌──────────────────────────────────────────────┐   │
│  │            Embedding Service                  │   │
│  │        (Ollama/OpenAI/Mock)                  │   │
│  └──────────────────────────────────────────────┘   │
└─────────────────────┬───────────────────────────────┘
                      │
┌─────────────────────┴───────────────────────────────┐
│              PostgreSQL + pgvector                   │
└─────────────────────────────────────────────────────┘
```

## Performance

- **Working Memory Access**: <1ms P99
- **Semantic Search**: <100ms P99 for 1M vectors
- **Embedding Generation**: ~10ms (Ollama local)
- **Batch Operations**: 100+ ops/second
- **Memory Capacity**: 1K working, 10K warm, unlimited cold

## Security

- **Credential Management**: Environment variables with secure defaults
- **Connection Security**: TLS/SSL support for database connections
- **Access Control**: Database user with minimal required permissions
- **Audit Logging**: Complete audit trail of all operations
- **Data Encryption**: At-rest and in-transit encryption support

## Monitoring

### Prometheus Metrics

The system exposes metrics on `:8080/metrics`:
- Memory operations (create, read, update, delete)
- Search performance and hit rates
- Tier migration statistics
- Database connection pool metrics
- Embedding generation latency

### Health Checks

```bash
# System health
curl http://localhost:8080/health

# Detailed diagnostics
./target/release/codex-memory mcp diagnose
```

## Troubleshooting

### Common Issues

**Database Connection Failed:**
```bash
# Check PostgreSQL is running
pg_isready -h localhost -p 5432

# Verify credentials
psql -U codex_user -d codex_db -c "SELECT 1"
```

**Ollama Not Responding:**
```bash
# Check Ollama is running
curl http://localhost:11434/api/tags

# Pull embedding model
ollama pull nomic-embed-text
```

**MCP Not Loading in Claude:**
```bash
# Validate configuration
./target/release/codex-memory mcp validate

# Check logs
tail -f ~/.claude/logs/mcp.log  # Location may vary
```

### Debug Mode

Enable detailed logging:
```bash
LOG_LEVEL=debug ./target/release/codex-memory start
```

## Development

### Building from Source

```bash
# Debug build
cargo build

# Release build with optimizations
cargo build --release

# Run tests
cargo test

# Run with hot reload
cargo watch -x run
```

### Running Tests

```bash
# All tests
cargo test

# Specific test suites
cargo test --test e2e_basic_crud
cargo test --test database_connectivity_test

# With output
cargo test -- --nocapture
```

## Contributing

We welcome contributions! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

### Development Workflow

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests for new functionality
5. Run `cargo fmt` and `cargo clippy`
6. Submit a pull request

## License

This project is licensed under the MIT License - see [LICENSE](LICENSE) for details.

## Support

- **Documentation**: [MCP_SETUP.md](MCP_SETUP.md)
- **Issues**: GitHub Issues
- **Discussions**: GitHub Discussions

## Acknowledgments

- Built for the Claude AI ecosystem
- Uses pgvector for efficient vector operations
- Inspired by hierarchical memory architectures in cognitive science