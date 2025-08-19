# Developer Onboarding Guide - Agentic Memory System

## Welcome to the Team! 🚀

This guide will help you get up and running with the Agentic Memory System codebase. By the end of this guide, you'll have a complete development environment and understand how to contribute effectively to the project.

## Table of Contents

1. [Prerequisites](#prerequisites)
2. [Development Environment Setup](#development-environment-setup)
3. [Architecture Overview](#architecture-overview)
4. [Codebase Structure](#codebase-structure)
5. [Development Workflow](#development-workflow)
6. [Testing Strategy](#testing-strategy)
7. [Contributing Guidelines](#contributing-guidelines)
8. [Common Development Tasks](#common-development-tasks)
9. [Debugging and Troubleshooting](#debugging-and-troubleshooting)
10. [Resources and Learning Materials](#resources-and-learning-materials)

## Prerequisites

Before you begin, ensure you have the following installed:

### Required Software
- **Rust 1.70+**: [Install Rust](https://rustup.rs/)
- **PostgreSQL 14+**: [Install PostgreSQL](https://postgresql.org/download/)
- **Git**: Version control system
- **Docker & Docker Compose**: For containerized development
- **Node.js 18+**: For documentation tools (optional)

### Recommended Tools
- **VS Code** with Rust extensions:
  - rust-analyzer
  - CodeLLDB (for debugging)
  - Error Lens
  - GitLens
- **DBeaver** or **pgAdmin**: Database management
- **Postman** or **Insomnia**: API testing
- **cargo-watch**: For automatic rebuilds

### System Requirements
- **OS**: macOS, Linux, or Windows with WSL2
- **RAM**: Minimum 8GB, recommended 16GB+
- **Storage**: At least 10GB free space
- **Network**: Stable internet connection for dependencies

## Development Environment Setup

### 1. Clone the Repository

```bash
# Clone the repository
git clone https://github.com/company/agentic-memory-system.git
cd agentic-memory-system

# Set up your git configuration
git config user.name "Your Name"
git config user.email "your.email@company.com"
```

### 2. Install Rust Dependencies

```bash
# Install Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Install additional tools
cargo install cargo-watch cargo-audit cargo-outdated
rustup component add clippy rustfmt
```

### 3. Set Up PostgreSQL

#### Option A: Local Installation
```bash
# macOS with Homebrew
brew install postgresql@14 pgvector
brew services start postgresql@14

# Ubuntu/Debian
sudo apt-get install postgresql-14 postgresql-14-pgvector
sudo systemctl start postgresql

# Create development database
createdb memory_dev
psql memory_dev -c "CREATE EXTENSION vector;"
```

#### Option B: Docker (Recommended)
```bash
# Use the provided Docker Compose setup
docker-compose up -d postgres

# Verify database is running
docker-compose exec postgres psql -U postgres -d memory_dev -c "SELECT version();"
```

### 4. Environment Configuration

```bash
# Copy environment template
cp .env.example .env

# Edit configuration (use your preferred editor)
vim .env
```

Example `.env` file:
```bash
# Database Configuration
DATABASE_URL=postgresql://postgres:password@localhost:5432/memory_dev

# Application Configuration
API_PORT=3333
LOG_LEVEL=debug
RUST_LOG=debug

# Embedding Service (optional for development)
EMBEDDING_SERVICE_URL=http://localhost:8080

# Development Settings
DEVELOPMENT_MODE=true
SKIP_MIGRATIONS=false
```

### 5. Build and Run Tests

```bash
# Build the project
cargo build

# Run tests to verify setup
cargo test

# Run database migrations
cargo run --bin migrate

# Start the development server
cargo run
```

### 6. Verify Installation

```bash
# Test API endpoint
curl http://localhost:3333/api/v1/health

# Expected response:
# {"status":"healthy","database_connected":true}
```

## Architecture Overview

### High-Level Components

```
┌─────────────────────────────────────────────────────────┐
│                    Client Layer                         │
│  (Claude Code, Claude Desktop, API Clients)            │
└─────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────┐
│                  MCP Server Layer                       │
│  (Authentication, Rate Limiting, Protocol Handling)    │
└─────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────┐
│                Application Layer                        │
│  (Memory Repository, Search, Tier Management)          │
└─────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────┐
│                   Data Layer                            │
│  (PostgreSQL + pgvector, Memory Tiers)                 │
└─────────────────────────────────────────────────────────┘
```

### Key Concepts

1. **Memory Tiers**: Working (hot), Warm, Cold storage tiers
2. **Vector Embeddings**: Semantic search capabilities
3. **MCP Protocol**: Model Context Protocol for Claude integration
4. **Async Architecture**: Built on Tokio for high performance
5. **Type Safety**: Comprehensive Rust type system usage

## Codebase Structure

```
agentic-memory-system/
├── src/
│   ├── main.rs                 # Application entry point
│   ├── lib.rs                  # Library root
│   ├── config.rs               # Configuration management
│   ├── memory/                 # Core memory functionality
│   │   ├── mod.rs
│   │   ├── repository.rs       # Database operations
│   │   ├── models.rs           # Data models
│   │   └── error.rs            # Error types
│   ├── mcp/                    # MCP protocol implementation
│   │   ├── mod.rs
│   │   ├── server.rs           # MCP server
│   │   └── handlers.rs         # Request handlers
│   ├── embedding/              # Vector embedding service
│   │   ├── mod.rs
│   │   └── client.rs           # Embedding client
│   ├── monitoring/             # Metrics and health checks
│   │   ├── mod.rs
│   │   └── metrics.rs          # Prometheus metrics
│   └── security/               # Authentication & authorization
│       ├── mod.rs
│       └── auth.rs             # Auth implementation
├── migrations/                 # Database migrations
├── tests/                      # Integration tests
├── docs/                       # Documentation
├── scripts/                    # Utility scripts
├── docker/                     # Docker configurations
└── config/                     # Configuration files
```

### Important Files to Understand

1. **`src/memory/repository.rs`**: Core business logic for memory operations
2. **`src/memory/models.rs`**: Data structures and types
3. **`src/mcp/server.rs`**: MCP protocol implementation
4. **`src/config.rs`**: Application configuration management
5. **`migrations/`**: Database schema evolution

## Development Workflow

### 1. Feature Development Process

```bash
# 1. Create feature branch
git checkout -b feature/your-feature-name

# 2. Make changes with frequent commits
git add .
git commit -m "feat: add new memory tier logic"

# 3. Run tests locally
cargo test
cargo clippy
cargo fmt

# 4. Push and create PR
git push origin feature/your-feature-name
```

### 2. Code Quality Checks

Always run these before committing:

```bash
# Format code
cargo fmt

# Lint code
cargo clippy -- -D warnings

# Security audit
cargo audit

# Check for outdated dependencies
cargo outdated
```

### 3. Development Server

```bash
# Watch for changes and auto-rebuild
cargo watch -x run

# Run with debug logging
RUST_LOG=debug cargo run

# Run specific tests
cargo test memory::repository::tests
```

## Testing Strategy

### Test Types and Structure

1. **Unit Tests**: Test individual functions and modules
2. **Integration Tests**: Test API endpoints and database interactions
3. **Performance Tests**: Benchmark critical paths
4. **Property Tests**: Use `proptest` for edge case discovery

### Running Tests

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run specific test
cargo test test_create_memory

# Run tests with coverage
cargo tarpaulin --out Html
```

### Writing Tests

Example unit test:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::models::{CreateMemoryRequest, MemoryTier};

    #[tokio::test]
    async fn test_create_memory() {
        let repo = setup_test_repository().await;
        
        let request = CreateMemoryRequest {
            content: "Test memory".to_string(),
            tier: Some(MemoryTier::Working),
            importance_score: Some(0.8),
            ..Default::default()
        };

        let memory = repo.create_memory(request).await.unwrap();
        assert_eq!(memory.content, "Test memory");
        assert_eq!(memory.tier, MemoryTier::Working);
    }
}
```

### Test Database Setup

```bash
# Create test database
createdb memory_test

# Set test environment
export DATABASE_URL=postgresql://postgres:password@localhost:5432/memory_test

# Run migrations for tests
cargo run --bin migrate
```

## Contributing Guidelines

### Code Style

1. **Follow Rust conventions**: Use `rustfmt` for formatting
2. **Use descriptive names**: Clear variable and function names
3. **Document public APIs**: All public functions need docs
4. **Handle errors properly**: Use `Result<T, E>` everywhere
5. **Write tests**: Maintain 80%+ code coverage

### Git Workflow

1. **Commit messages**: Follow [Conventional Commits](https://conventionalcommits.org/)
   ```
   feat: add new memory tier migration logic
   fix: resolve connection pool exhaustion issue
   docs: update API documentation for search endpoints
   test: add integration tests for tier management
   ```

2. **Branch naming**:
   - `feature/description` - New features
   - `fix/issue-description` - Bug fixes
   - `docs/update-topic` - Documentation updates
   - `refactor/component-name` - Code refactoring

3. **Pull Request Process**:
   - Fill out PR template completely
   - Ensure all tests pass
   - Request review from team members
   - Address feedback promptly

### Code Review Checklist

- [ ] Code follows style guidelines
- [ ] Tests are included and passing
- [ ] Documentation is updated
- [ ] No security vulnerabilities introduced
- [ ] Performance impact considered
- [ ] Error handling is comprehensive

## Common Development Tasks

### 1. Adding a New API Endpoint

```rust
// 1. Add to MCP handlers
impl MCPServer {
    async fn handle_memory_custom_operation(
        &self,
        params: CustomOperationParams,
    ) -> Result<CustomOperationResponse, MCPError> {
        // Implementation here
    }
}

// 2. Add to repository
impl MemoryRepository {
    pub async fn custom_operation(
        &self,
        params: CustomOperationParams,
    ) -> Result<CustomOperationResult, MemoryError> {
        // Implementation here
    }
}

// 3. Add tests
#[tokio::test]
async fn test_custom_operation() {
    // Test implementation
}
```

### 2. Adding Database Migration

```bash
# Create migration file
touch migrations/$(date +%Y%m%d_%H%M%S)_add_custom_field.sql
```

```sql
-- migrations/20240115_120000_add_custom_field.sql

-- Add new column
ALTER TABLE memories 
ADD COLUMN custom_field TEXT;

-- Create index if needed
CREATE INDEX CONCURRENTLY idx_memories_custom_field 
ON memories (custom_field) 
WHERE custom_field IS NOT NULL;

-- Update constraints
ALTER TABLE memories 
ADD CONSTRAINT check_custom_field_length 
CHECK (length(custom_field) <= 255);
```

### 3. Adding Configuration Option

```rust
// In src/config.rs
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub custom_feature_enabled: bool,
    pub custom_threshold: f32,
    // ... other fields
}

impl Default for Config {
    fn default() -> Self {
        Self {
            custom_feature_enabled: false,
            custom_threshold: 0.5,
            // ... other defaults
        }
    }
}
```

### 4. Adding Metrics

```rust
// In src/monitoring/metrics.rs
use prometheus::{Counter, Histogram};

lazy_static! {
    pub static ref CUSTOM_OPERATIONS_TOTAL: Counter = Counter::new(
        "custom_operations_total",
        "Total number of custom operations"
    ).unwrap();
    
    pub static ref CUSTOM_OPERATION_DURATION: Histogram = Histogram::new(
        "custom_operation_duration_seconds",
        "Time spent on custom operations"
    ).unwrap();
}

// In your handler
let timer = CUSTOM_OPERATION_DURATION.start_timer();
// ... operation code ...
timer.observe_duration();
CUSTOM_OPERATIONS_TOTAL.inc();
```

## Debugging and Troubleshooting

### Development Debugging

```bash
# Enable debug logging
export RUST_LOG=debug

# Run with backtrace
export RUST_BACKTRACE=1

# Use debugger in VS Code
# Set breakpoints and press F5

# Database debugging
psql $DATABASE_URL -c "SELECT * FROM memories LIMIT 5;"
```

### Common Issues and Solutions

1. **Connection Pool Exhausted**
   ```bash
   # Increase pool size in config
   # Check for connection leaks in code
   ```

2. **Test Database Issues**
   ```bash
   # Reset test database
   dropdb memory_test && createdb memory_test
   cargo run --bin migrate
   ```

3. **Build Errors**
   ```bash
   # Clean rebuild
   cargo clean
   cargo build
   ```

4. **Performance Issues**
   ```bash
   # Profile with perf
   cargo build --release
   perf record --call-graph=dwarf target/release/memory-system
   ```

### Logging Best Practices

```rust
use tracing::{info, warn, error, debug, span, Level};

// Structured logging
info!(
    memory_id = %memory.id,
    tier = ?memory.tier,
    "Memory created successfully"
);

// Use spans for tracing
let span = span!(Level::INFO, "memory_operation", operation = "create");
let _enter = span.enter();
```

### Database Query Debugging

```sql
-- Enable query logging in PostgreSQL
ALTER SYSTEM SET log_statement = 'all';
SELECT pg_reload_conf();

-- Analyze slow queries
SELECT query, calls, total_exec_time, mean_exec_time
FROM pg_stat_statements 
ORDER BY total_exec_time DESC 
LIMIT 10;

-- Check query plans
EXPLAIN (ANALYZE, BUFFERS) 
SELECT * FROM memories WHERE tier = 'working';
```

## Resources and Learning Materials

### Internal Documentation
- [Architecture Documentation](architecture.md)
- [API Reference](api_reference.md)
- [Operations Runbook](operations_runbook.md)
- [Troubleshooting Guide](troubleshooting_guide.md)
- [Performance Tuning Guide](performance_tuning_guide.md)

### Rust Learning Resources
- [The Rust Programming Language](https://doc.rust-lang.org/book/) - Official Rust book
- [Rust by Example](https://doc.rust-lang.org/rust-by-example/) - Learn by examples
- [Async Book](https://rust-lang.github.io/async-book/) - Async programming in Rust
- [Tokio Tutorial](https://tokio.rs/tokio/tutorial) - Async runtime

### Database Resources
- [PostgreSQL Documentation](https://postgresql.org/docs/)
- [pgvector Documentation](https://github.com/pgvector/pgvector)
- [SQL Performance Explained](https://use-the-index-luke.com/)

### Testing Resources
- [Rust Testing Guide](https://doc.rust-lang.org/book/ch11-00-testing.html)
- [Property-based Testing](https://github.com/AltSysrq/proptest)
- [Integration Testing Best Practices](https://doc.rust-lang.org/book/ch11-03-test-organization.html)

### Development Tools
- [cargo-watch](https://github.com/watchexec/cargo-watch) - Auto-rebuild
- [cargo-audit](https://github.com/RustSec/cargo-audit) - Security auditing
- [cargo-tarpaulin](https://github.com/xd009642/tarpaulin) - Code coverage

### Team Communication
- **Slack Channel**: #memory-system-dev
- **Stand-up**: Daily at 10:00 AM
- **Code Review**: Required for all PRs
- **Architecture Discussions**: Weekly on Fridays

## Getting Help

### When You're Stuck

1. **Check Documentation**: Search existing docs first
2. **Reproduce Locally**: Create minimal reproduction case
3. **Check Logs**: Enable debug logging for more details
4. **Ask Team**: Use Slack or schedule 1:1 with mentor
5. **Create Issue**: Document bugs or feature requests

### Escalation Path

1. **Team Members**: For code questions and reviews
2. **Tech Lead**: For architecture and design decisions
3. **Product Manager**: For feature requirements
4. **DevOps Team**: For infrastructure and deployment issues

### Office Hours

- **Architecture Reviews**: Wednesdays 2-4 PM
- **Code Review Sessions**: Daily 4-5 PM
- **Open Q&A**: Fridays 3-4 PM

## Next Steps

Now that you have your development environment set up:

1. **Complete Tutorial**: Work through the [Getting Started Tutorial](getting_started_tutorial.md)
2. **Pick First Task**: Look for issues labeled `good-first-issue`
3. **Shadow Code Review**: Observe a few code reviews before participating
4. **Read Codebase**: Spend time understanding the existing code patterns
5. **Set Up IDE**: Configure your development environment for maximum productivity

Welcome to the team! We're excited to have you contribute to the Agentic Memory System. Don't hesitate to ask questions – we're here to help you succeed.

---

**Remember**: This is a living document. Please contribute improvements and updates as you discover better practices or encounter new challenges.