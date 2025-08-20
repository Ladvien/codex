# Codex Memory - Claude Desktop Extension

An advanced hierarchical memory system for Claude Desktop that provides persistent, searchable memory with semantic understanding.

## Features

- **Semantic Search**: Find memories using natural language queries
- **Auto-Tiering**: Automatically organize memories by importance and recency
- **PostgreSQL Backend**: Robust, scalable storage with pgvector for embeddings
- **Configurable**: Easy setup through Claude Desktop's UI
- **Local or Cloud Embeddings**: Support for both Ollama (local) and OpenAI

## Installation

### Quick Install (Coming Soon)

Once published to the Claude Desktop extension directory:
1. Open Claude Desktop
2. Go to Settings → Extensions
3. Search for "Codex Memory"
4. Click Install

### Manual Installation

1. Download the latest `codex-memory.dxt` from [Releases](https://github.com/Ladvien/codex/releases)
2. Open Claude Desktop
3. Go to Settings → Extensions
4. Click "Install Extension"
5. Select the downloaded `.dxt` file

### Build from Source

```bash
# Clone the repository
git clone https://github.com/Ladvien/codex.git
cd codex

# Build the extension
cd extension
./build-extension.sh

# The extension will be created as codex-memory.dxt
```

## Prerequisites

### Database Setup

You need PostgreSQL with the pgvector extension:

```bash
# macOS
brew install postgresql
brew install pgvector

# Ubuntu/Debian
sudo apt install postgresql postgresql-contrib
sudo apt install postgresql-15-pgvector

# Create database
createdb codex
psql codex -c "CREATE EXTENSION IF NOT EXISTS vector;"
```

### Embedding Provider

Choose one:

#### Option 1: Ollama (Local, Recommended)
```bash
# Install Ollama
curl -fsSL https://ollama.com/install.sh | sh

# Pull embedding model
ollama pull nomic-embed-text
```

#### Option 2: OpenAI (Cloud)
- Get an API key from [OpenAI](https://platform.openai.com/api-keys)
- Configure in Claude Desktop settings

## Configuration

After installation, configure the extension in Claude Desktop:

### Required Settings

- **Database URL**: PostgreSQL connection string
  - Example: `postgresql://user:password@localhost/codex`
  - For local dev: `postgresql://localhost/codex`

### Embedding Settings

For Ollama (local):
- **Embedding Provider**: `ollama`
- **Embedding Model**: `nomic-embed-text`
- **Ollama Base URL**: `http://localhost:11434`

For OpenAI:
- **Embedding Provider**: `openai`
- **Embedding Model**: `text-embedding-ada-002`
- **OpenAI API Key**: Your API key

### Optional Settings

- **Working Memory Limit**: Max items in hot cache (default: 1000)
- **Warm Memory Limit**: Max items in warm tier (default: 10000)
- **Enable Auto-Tiering**: Automatically organize memories (default: true)
- **Log Level**: Verbosity (error/warn/info/debug/trace)

## Usage

Once configured, Claude can use these tools:

### Store Memory
```
Store this for later: [your content]
```

### Search Memory
```
What do I remember about [topic]?
Search my memories for [query]
```

### Get Statistics
```
Show memory statistics
How many memories do I have?
```

## Architecture

```
┌─────────────────┐
│  Claude Desktop │
├─────────────────┤
│   MCP Protocol  │
├─────────────────┤
│  Codex Memory   │
├─────────────────┤
│   PostgreSQL    │
│   + pgvector    │
└─────────────────┘
```

### Memory Tiers

1. **Working** (Hot): Recently accessed, high importance
2. **Warm**: Moderate access, medium importance  
3. **Cold**: Rarely accessed, archived

### Features

- **Semantic Search**: Vector similarity using pgvector
- **Auto-Tiering**: Automatic memory organization
- **Importance Scoring**: Track memory relevance
- **Access Tracking**: Monitor usage patterns
- **Metadata Support**: Tag and categorize memories

## Troubleshooting

### Database Connection Issues

```bash
# Test connection
psql postgresql://localhost/codex -c "SELECT 1;"

# Check pgvector
psql codex -c "SELECT * FROM pg_extension WHERE extname = 'vector';"
```

### Ollama Issues

```bash
# Check if Ollama is running
curl http://localhost:11434/api/tags

# Test embedding generation
curl http://localhost:11434/api/embeddings -d '{
  "model": "nomic-embed-text",
  "prompt": "test"
}'
```

### View Logs

Logs are stored in:
- macOS: `~/Library/Logs/Claude/`
- Linux: `~/.local/share/Claude/logs/`
- Windows: `%APPDATA%\Claude\logs\`

### Reset Extension

1. Open Claude Desktop Settings
2. Go to Extensions
3. Find Codex Memory
4. Click Remove
5. Reinstall

## Development

### Project Structure

```
codex/
├── src/                 # Rust source code
├── extension/          # Desktop extension files
│   ├── manifest.json   # Extension manifest
│   ├── run-codex.sh   # Wrapper script
│   └── build-extension.sh
└── target/release/     # Compiled binary
```

### Building

```bash
# Build Rust binary
cargo build --release

# Run tests
cargo test

# Build extension
cd extension
./build-extension.sh
```

### Testing Locally

```bash
# Run in stdio mode (for testing)
./target/release/codex-memory mcp-stdio

# Send test commands (in another terminal)
echo '{"jsonrpc":"2.0","method":"initialize","params":{},"id":1}' | ./target/release/codex-memory mcp-stdio
```

## Contributing

Contributions welcome! Please:

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests
5. Submit a pull request

## License

GPL-3.0 - See [LICENSE](../LICENSE) for details

## Support

- [GitHub Issues](https://github.com/Ladvien/codex/issues)
- [Documentation](https://github.com/Ladvien/codex/wiki)

## Roadmap

- [ ] Publish to Claude Desktop Extension Directory
- [ ] Add memory export/import
- [ ] Support for more embedding models
- [ ] Memory visualization tools
- [ ] Batch operations
- [ ] Memory chains and relationships