# CLAUDE.md - Engineering Best Practices Guide

## Table of Contents
- [Rust Development Standards](#rust-development-standards)
- [PostgreSQL Operations](#postgresql-operations)
- [Code Quality & Testing](#code-quality--testing)
- [Performance Guidelines](#performance-guidelines)
- [Memory System Patterns](#memory-system-patterns)
- [Operational Excellence](#operational-excellence)

## Rust Development Standards

### Code Quality Enforcement
**Always run before committing:**
```bash
cargo fmt -- --check        # Format code consistently
cargo clippy -- -D warnings # Catch common mistakes
cargo audit                 # Check for security vulnerabilities
cargo outdated             # Identify outdated dependencies
```

### Rust Best Practices
- **Use `Result<T, E>` everywhere** - Never use `unwrap()` in production code
- **Implement `From` traits for error types** - Enable clean error propagation with `?`
- **Prefer `Arc<RwLock<T>>` over `Arc<Mutex<T>>`** when reads dominate writes
- **Use `tokio::select!`** for handling multiple async operations
- **Implement graceful shutdown** - Always clean up resources properly
- **Use `#[derive(Debug, Clone)]`** on all public structs
- **Document panic conditions** - If a function can panic, document when
- **Prefer iterators over loops** - More idiomatic and often more performant
- **Use `Cow<'a, str>`** when you might or might not need to allocate
- **Implement `Drop` trait** for resources requiring cleanup

### Testing Standards
- **Maintain 80%+ code coverage** minimum
- **Test the unhappy path** - Error conditions are more important than success
- **Use `proptest`** for property-based testing of complex logic
- **Mock external dependencies** - Never hit real services in unit tests
- **Use `#[should_panic]`** sparingly - Prefer returning `Result` in tests
- **Benchmark critical paths** using `criterion`
- **Test concurrent access** using `loom` for lock-free structures

## PostgreSQL Operations

### Connection Management
- **Always use connection pooling** - Never create connections per request
- **Set connection timeouts** - Prevent zombie connections
- **Use read replicas** for read-heavy workloads
- **Implement retry logic** with exponential backoff
- **Monitor connection pool saturation** - Alert at 70% usage
- **Set `statement_timeout`** to prevent runaway queries
- **Use `idle_in_transaction_session_timeout`** to prevent lock holding

### Query Practices
- **Always use parameterized queries** - Never concatenate SQL strings
- **Run `EXPLAIN ANALYZE`** on new queries before production
- **Create indexes CONCURRENTLY** to avoid locking
- **Use `SELECT ... FOR UPDATE SKIP LOCKED`** for queue patterns
- **Batch INSERTs** when possible - Use `COPY` for bulk data
- **Vacuum regularly** - Set appropriate autovacuum settings
- **Monitor slow query log** - Investigate queries >100ms
- **Use CTEs sparingly** - They create optimization barriers

### pgvector Specific
- **Choose appropriate vector dimensions** - Smaller is faster
- **Use HNSW index** for approximate search, B-tree for exact
- **Set `maintenance_work_mem`** higher during index creation
- **Monitor index build time** - HNSW indexes can be slow to build
- **Use `vector_cosine_ops`** for normalized vectors
- **Limit result sets** - Vector searches can be memory intensive
- **Pre-normalize vectors** when using cosine similarity

### Backup & Recovery
- **Test restores regularly** - Backups are worthless if they don't restore
- **Use `pg_dump` with `--format=custom`** for flexibility
- **Enable WAL archiving** for point-in-time recovery
- **Monitor backup duration** - Alert if it exceeds expected time
- **Verify backup integrity** with `pg_verifybackup`
- **Document recovery procedures** - Practice during low-traffic periods
- **Keep 30 days of backups** minimum, 90 days recommended

## Code Quality & Testing

### Pre-Commit Checks
- **Run formatters** - `cargo fmt`, `prettier`, `black` as appropriate
- **Run linters** - `clippy`, `eslint`, `pylint` as needed
- **Check types** - `cargo check`, `mypy`, `tsc` for type safety
- **Run security scanners** - `cargo audit`, `safety`, `npm audit`
- **Validate schemas** - JSON Schema, OpenAPI, Protocol Buffers
- **Check documentation** - Ensure public APIs are documented

### Testing Pyramid
- **70% Unit Tests** - Fast, isolated, test business logic
- **20% Integration Tests** - Test component interactions
- **10% E2E Tests** - Critical user paths only
- **Load test before release** - Identify bottlenecks early
- **Chaos test quarterly** - Verify fault tolerance
- **Security test monthly** - Automated penetration testing

## Performance Guidelines

### Memory Management
- **Profile before optimizing** - Use `perf`, `flamegraph`, `heaptrack`
- **Avoid unnecessary allocations** - Reuse buffers when possible
- **Use `SmallVec` for small collections** - Avoid heap allocation
- **Implement object pools** for frequently allocated objects
- **Monitor memory fragmentation** - Can impact long-running services
- **Set memory limits** - Prevent OOM kills
- **Use `jemalloc`** for better multi-threaded performance

### Concurrency
- **Prefer async over threads** for I/O-bound work
- **Use `rayon` for CPU-bound** parallel processing
- **Implement backpressure** - Don't overwhelm downstream services
- **Use lock-free structures** where appropriate
- **Avoid mutex in hot paths** - Consider `RwLock` or atomic operations
- **Monitor thread pool saturation** - Size appropriately
- **Implement circuit breakers** for external dependencies

### Optimization Principles
- **Measure first** - Never optimize based on assumptions
- **Focus on algorithmic improvements** before micro-optimizations
- **Cache computed results** - But implement cache invalidation
- **Batch operations** - Reduce round trips and syscalls
- **Use zero-copy techniques** where possible
- **Implement rate limiting** - Protect against abuse
- **Monitor tail latencies** - P99 matters more than average

## Memory System Patterns

### Tiering Strategy
- **Promote on access frequency** not recency alone
- **Decay importance scores** using exponential decay
- **Batch migrations** during low-traffic periods
- **Implement hysteresis** - Prevent ping-ponging between tiers
- **Monitor tier distribution** - Ensure appropriate balance
- **Use write-through caching** for critical data
- **Implement read-ahead** for predictable access patterns

### Summarization
- **Summarize before evicting** from working memory
- **Use recursive summarization** for deep hierarchies
- **Maintain bidirectional references** between summaries and source
- **Implement incremental updates** - Don't regenerate everything
- **Version summaries** - Track what changed
- **Monitor compression ratios** - Should be >10:1 for cold storage
- **Validate summary quality** - Use ROUGE or similar metrics

### Search Patterns
- **Implement query caching** with smart invalidation
- **Use cursor-based pagination** not offset/limit
- **Limit concurrent searches** per client
- **Implement search timeout** - Kill long-running queries
- **Monitor search relevance** - Track click-through rates
- **Use query expansion** for better recall
- **Implement faceted search** for exploration

## Operational Excellence

### Monitoring & Alerting
- **Alert on symptoms, not causes** - User impact matters
- **Implement SLI/SLO/SLA hierarchy** - Define what matters
- **Use structured logging** - Make logs queryable
- **Implement distributed tracing** - Understand request flow
- **Monitor business metrics** not just technical ones
- **Set up error budgets** - Know when to slow down
- **Create runbooks** for common issues
- **Practice incident response** - Run game days

### Deployment Practices
- **Use feature flags** for gradual rollouts
- **Implement blue-green deployments** for zero downtime
- **Always have rollback plan** - Test it regularly
- **Monitor canary deployments** - Catch issues early
- **Use health checks** - Separate liveness from readiness
- **Implement graceful shutdown** - Drain connections properly
- **Version your APIs** - Never break backwards compatibility
- **Document breaking changes** - Communicate early and often

### Security Hygiene
- **Rotate secrets regularly** - Use tools like Vault
- **Implement defense in depth** - Multiple security layers
- **Use least privilege principle** - Minimal permissions needed
- **Audit access logs** - Know who accessed what
- **Encrypt data at rest and in transit** - Always use TLS
- **Implement rate limiting** - Prevent abuse
- **Use prepared statements** - Prevent SQL injection
- **Validate all inputs** - Never trust user data

### Documentation Standards
- **Document decisions** not just implementations
- **Keep README current** - First thing developers read
- **Use inline documentation** - Code should be self-documenting
- **Maintain architecture decision records (ADRs)**
- **Document failure modes** - What happens when things break
- **Create operational runbooks** - Step-by-step procedures
- **Maintain glossary** - Define domain terms
- **Use examples liberally** - Show, don't just tell

### Performance Baselines
Based on SOTA research, maintain these performance targets:
- **Working memory access**: <1ms P99
- **Warm storage query**: <100ms P99
- **Cold storage retrieval**: <20s P99
- **Embedding generation**: <100ms P95
- **Migration batch processing**: <5% performance impact
- **Memory compression ratio**: >10:1 for cold tier
- **Cache hit ratio**: >90% for repeated queries
- **Connection pool utilization**: <70% normal, <90% peak

### Recovery Targets
- **RTO (Recovery Time Objective)**: <1 hour
- **RPO (Recovery Point Objective)**: <5 minutes
- **Backup validation**: Weekly automated restore test
- **Failover time**: <2 minutes for automated failover
- **Data consistency check**: After every recovery
- **Degraded mode operation**: Must support 50% capacity

Remember: **These are minimums, not targets. Always strive to exceed them.**