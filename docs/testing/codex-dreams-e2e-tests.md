# Codex Dreams End-to-End Testing Suite

This document describes the comprehensive E2E test suite for the Codex Dreams feature, covering all 9 stories in the epic from memory processing through insight generation, storage, search, feedback, and export.

## Overview

The Codex Dreams E2E tests validate the complete insight generation pipeline:

1. **Memory Processing** → Memories are selected and queued for insight generation
2. **Ollama Integration** → LLM generates insights from memory content using localhost-only connections
3. **Insight Storage** → Generated insights are stored with proper indexing and relationships
4. **Semantic Search** → Vector-based search across insights with performance requirements
5. **Feedback System** → User feedback collection and insight quality scoring
6. **Export Functions** → Markdown and JSON-LD export with proper formatting
7. **Scheduling** → Background processing with failure resilience
8. **Performance** → Load testing and resource monitoring
9. **Database Schema** → Migration testing and constraint validation

## Test Structure

### Core Test Files

- `tests/e2e_codex_dreams_full.rs` - Complete pipeline integration tests
- `tests/e2e_ollama_integration.rs` - Ollama client and mock server tests
- `tests/e2e_scheduler_performance.rs` - Scheduler and performance tests
- `tests/e2e_mcp_insights_commands.rs` - MCP command interface tests
- `tests/e2e_codex_dreams_migrations.rs` - Database migration tests

### Test Infrastructure

- `tests/helpers/insights_test_utils.rs` - Shared utilities and builders
- `tests/helpers/ollama_mock.rs` - Mock Ollama server for predictable testing

## Test Categories

### 1. Pipeline Integration Tests (`e2e_codex_dreams_full.rs`)

**Purpose**: Validate end-to-end insight generation flow

**Key Tests**:
- `test_full_insight_generation_pipeline()` - Complete flow from memories to insights
- `test_batch_processing_performance()` - 100-memory batch processing
- `test_realtime_insight_generation()` - Sub-1-second processing for critical memories
- `test_insight_export()` - Markdown and JSON-LD export validation
- `test_insight_deduplication()` - Duplicate content handling
- `test_insight_tier_migration()` - Automatic tier management

**Performance Requirements**:
- Batch processing: >10 memories/second
- Real-time processing: <1 second for critical insights
- Semantic search: <50ms P99

### 2. Ollama Integration Tests (`e2e_ollama_integration.rs`)

**Purpose**: Validate LLM integration with security and resilience

**Key Tests**:
- `test_ollama_client_basic_functionality()` - Health checks and insight generation
- `test_ollama_timeout_handling()` - Connection timeout management
- `test_ollama_retry_mechanism()` - Exponential backoff retry logic
- `test_localhost_validation()` - Security: localhost-only connections
- `test_circuit_breaker()` - Failure rate monitoring and circuit breaking
- `test_concurrent_requests()` - Multi-threaded request handling

**Security Features**:
- Only localhost URLs accepted (127.0.0.1, ::1, localhost)
- Connection timeouts and retry limits
- Circuit breaker pattern for fault tolerance

### 3. Scheduler and Performance Tests (`e2e_scheduler_performance.rs`)

**Purpose**: Background processing and system resilience

**Key Tests**:
- `test_scheduler_basic_functionality()` - Periodic processing with 2-second intervals
- `test_scheduler_failure_handling()` - Graceful handling of Ollama failures
- `test_large_dataset_performance()` - 1000-memory processing performance
- `test_memory_usage()` - Resource consumption monitoring
- `test_concurrent_processing()` - Multi-processor concurrent execution
- `test_scheduler_graceful_shutdown()` - Clean shutdown with timeout

**Performance Targets**:
- Memory growth: <500MB for 1000 memories
- Processing rate: >5 memories/second per batch
- Overall rate: >10 memories/second system-wide

### 4. MCP Command Tests (`e2e_mcp_insights_commands.rs`)

**Purpose**: Model Context Protocol command interface validation

**Key Commands Tested**:
- `generate_insights` - Trigger insight generation with timeframe/topic filters
- `show_insights` - List recent insights with pagination and type filtering
- `search_insights` - Semantic search with query and limit parameters
- `insight_feedback` - Record user feedback (helpful/unhelpful/irrelevant)
- `export_insights` - Export to markdown/JSON-LD with filtering options

**Validation Areas**:
- Parameter validation and error handling
- Response formatting with ★ insight markers
- Concurrent command execution
- Tool schema compliance

### 5. Migration Tests (`e2e_codex_dreams_migrations.rs`)

**Purpose**: Database schema evolution and rollback testing

**Key Tests**:
- `test_migration_creates_all_schema_elements()` - All tables and columns created
- `test_migration_creates_optimized_indexes()` - HNSW, GIN, and composite indexes
- `test_migration_rollback()` - Clean rollback without data loss
- `test_migration_idempotency()` - Safe to run multiple times
- `test_migration_constraints()` - Data validation rules enforced

**Schema Elements**:
- `insights` table with content, type, confidence scoring
- `insight_vectors` table with HNSW indexing for semantic search
- `insight_feedback` table with user rating and comments
- `processing_queue` table for background job management
- `processing_metadata` column added to existing `memories` table

## Mock Services

### MockOllamaServer

**Purpose**: Predictable testing without real LLM dependencies

**Configuration Options**:
```rust
pub struct MockOllamaConfig {
    pub port: u16,                          // Server port
    pub model: String,                      // Model name to simulate
    pub fail_after: Option<usize>,          // Fail after N requests (retry testing)
    pub response_delay_ms: Option<u64>,     // Simulate slow responses
    pub always_fail: bool,                  // Test error handling paths
}
```

**Response Simulation**:
- Generates realistic insight content with confidence scores
- Simulates various failure modes (timeouts, errors, circuit breaker)
- Supports batch processing and concurrent requests
- Validates request format and model parameters

### Test Data Builders

**TestMemoryBuilder**:
```rust
TestMemoryBuilder::new("Memory content")
    .with_importance(0.8)
    .with_tier(MemoryTier::Working)
    .create(&repository).await?
```

**TestInsightBuilder**:
```rust
TestInsightBuilder::new("Generated insight content")
    .with_type(InsightType::Learning)
    .with_confidence(0.7)
    .build()
```

## Running Tests

### Prerequisites

1. **PostgreSQL with pgvector**:
   ```bash
   # Install pgvector extension
   CREATE EXTENSION IF NOT EXISTS vector;
   ```

2. **Environment Variables**:
   ```bash
   export TEST_DATABASE_URL="postgresql://postgres:postgres@localhost:5432/codex_test"
   export CODEX_DREAMS_ENABLED=true
   ```

3. **Feature Flag**:
   ```bash
   cargo test --features codex-dreams
   ```

### Test Execution

**Run All E2E Tests**:
```bash
cargo test --features codex-dreams e2e_
```

**Run Specific Test Categories**:
```bash
# Pipeline tests
cargo test --features codex-dreams e2e_codex_dreams_full

# Ollama integration
cargo test --features codex-dreams e2e_ollama_integration

# Scheduler and performance
cargo test --features codex-dreams e2e_scheduler_performance

# MCP commands
cargo test --features codex-dreams e2e_mcp_insights_commands

# Database migrations
cargo test --features codex-dreams e2e_codex_dreams_migrations
```

**Run with Real Ollama** (optional):
```bash
# Requires Ollama running on localhost:11434
cargo test --features codex-dreams test_real_ollama_integration -- --ignored
```

### Performance Validation

The test suite includes automated performance validation:

- **Response Times**: Semantic search <50ms P99, real-time insights <1s
- **Throughput**: >10 memories/second batch processing
- **Resource Usage**: <500MB memory growth for 1000 memories
- **Concurrency**: Support for 10+ concurrent insight generation requests

### Troubleshooting

**Common Issues**:

1. **pgvector Extension Missing**:
   ```sql
   CREATE EXTENSION IF NOT EXISTS vector;
   ```

2. **Database Connection Errors**:
   - Verify PostgreSQL is running
   - Check TEST_DATABASE_URL environment variable
   - Ensure test database exists and is accessible

3. **Ollama Mock Server Port Conflicts**:
   - Tests use dynamic port allocation
   - Wait for server startup with 100ms delay
   - Check for firewall blocking localhost connections

4. **Feature Flag Not Set**:
   - All tests require `--features codex-dreams`
   - Verify CODEX_DREAMS_ENABLED=true in environment

## Test Coverage

The E2E test suite provides comprehensive coverage of:

✅ **Memory Processing Pipeline** - Selection, queuing, batch processing  
✅ **LLM Integration** - Ollama client, security, error handling  
✅ **Insight Storage** - Database schema, indexing, relationships  
✅ **Semantic Search** - Vector search, performance, relevance  
✅ **Feedback System** - User ratings, quality scoring, analytics  
✅ **Export Functions** - Markdown, JSON-LD, filtering, formatting  
✅ **Background Scheduling** - Periodic processing, failure recovery  
✅ **Performance Monitoring** - Resource usage, throughput, latency  
✅ **Database Migrations** - Schema evolution, rollback, constraints  

## Integration with CI/CD

The tests are designed for automated execution in CI/CD pipelines:

- **No external dependencies** (uses mock Ollama server)
- **Isolated test databases** (each test creates its own schema)
- **Deterministic results** (fixed random seeds, controlled timing)
- **Performance regression detection** (automated threshold validation)
- **Feature flag compliance** (tests disabled without codex-dreams feature)

This comprehensive test suite ensures the Codex Dreams feature is production-ready with proper error handling, performance characteristics, and data integrity guarantees.