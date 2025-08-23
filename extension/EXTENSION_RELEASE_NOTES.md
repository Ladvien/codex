# Enhanced Agentic Memory System v2.0 Extension - Release 0.1.35

## Overview
Updated Claude Desktop Extension (.dxt) package containing the Enhanced Agentic Memory System v2.0 with comprehensive improvements and architectural alignment.

## What's New in v0.1.35

### Core System Enhancements
- **Three-Component Memory Scoring**: Recency, Importance, and Relevance scoring for sophisticated memory management
- **Cognitive Reflection Loops**: Advanced insight generation and consolidation processes
- **Architecture Alignment**: Full alignment with Enhanced Agentic Memory System v2.0 specifications
- **MCP Port Optimization**: Updated to use port 13513 (representing "mem" numerically)

### Database & Performance
- **Database Migration Applied**: Schema updates for three-component scoring system
- **PostgreSQL/pgvector Integration**: Enhanced vector operations and indexing
- **Connection Pool Optimization**: Improved database connection management
- **Performance Indexing**: Optimized queries for memory operations

### Extension Features
- **Updated Manifest**: Version 0.1.35 with enhanced descriptions
- **Improved Configuration**: Better environment variable handling
- **MCP Protocol Integration**: Seamless Claude Desktop integration
- **Error Handling**: Enhanced error reporting and debugging support

## Installation

### Option 1: Direct Installation
1. Download the updated `codex-memory.dxt` file
2. Open Claude Desktop
3. Go to Settings > Extensions
4. Click "Install Extension"
5. Select the `codex-memory.dxt` file

### Option 2: CLI Installation (if dxt tool available)
```bash
dxt install /path/to/codex-memory.dxt
```

## Configuration

The extension requires these configuration parameters:

- **Database URL**: PostgreSQL connection string with pgvector extension
- **Embedding Provider**: "ollama" (local) or "openai" (cloud)
- **Embedding Model**: Model name (default: "nomic-embed-text")
- **Ollama Base URL**: URL for local Ollama instance (if using local embeddings)
- **Log Level**: Logging verbosity (info, debug, warn, error)

## Key Improvements from Previous Version

### System Architecture
- ✅ Enhanced memory tiering with importance-based promotion
- ✅ Cognitive consolidation with reflection loops
- ✅ Semantic deduplication and memory compression
- ✅ Advanced retrieval with context-aware ranking

### Performance Optimizations
- ✅ 99.7% test success rate (295/296 passing tests)
- ✅ Optimized database indexes for vector operations
- ✅ Improved connection pooling and timeout handling
- ✅ Rate limiting and circuit breaker patterns

### Integration Features
- ✅ MCP protocol compliance for Claude Desktop
- ✅ Environment-based configuration system
- ✅ Comprehensive error handling and logging
- ✅ Graceful shutdown and resource cleanup

## Troubleshooting

### Common Issues
- **Database Connection**: Ensure PostgreSQL with pgvector extension is running
- **Port Conflicts**: The system uses port 13513 for MCP communication
- **Memory Access**: Verify database permissions for the codex_user account

### Debug Mode
Enable debug logging by setting log level to "debug" in extension configuration.

## Technical Specifications

- **Platform Support**: macOS, Linux
- **Database**: PostgreSQL 12+ with pgvector extension
- **Memory Model**: Hierarchical tiering (working/warm/cold/frozen)
- **Embedding Support**: Local (Ollama) and cloud (OpenAI) providers
- **Protocol**: Model Context Protocol (MCP) for Claude integration

## Developer Information

- **Version**: 0.1.35
- **Build Date**: 2025-08-23
- **Binary Size**: ~18.5MB (optimized release build)
- **Dependencies**: See Cargo.toml for complete dependency list

## Support

For issues or questions:
1. Check the troubleshooting guide above
2. Review system logs (accessible via debug mode)
3. Verify database connectivity and permissions
4. Ensure all environment variables are properly configured

---

**Note**: This extension requires a running PostgreSQL database with pgvector extension. The database schema will be automatically migrated on first startup if needed.