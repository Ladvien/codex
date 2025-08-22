# Memory System Architecture Compliance - Jira Stories

---

## CRITICAL-001: Add Frozen Tier Support to Database Schema

**Story Type:** Bug  
**Priority:** Critical  
**Components:** Database Layer, Memory Repository  
**Estimated Points:** 5  

### Description
The database schema currently only supports three memory tiers (working, warm, cold) but the architecture specification requires four tiers including frozen. This violation prevents the system from implementing the complete hierarchical memory management system as designed.

### Acceptance Criteria
- [ ] Database enum `memory_tier` includes four values: working, warm, cold, and frozen
- [ ] All existing queries that reference memory tiers support the frozen tier
- [ ] Migration script successfully updates existing database without data loss
- [ ] Frozen tier properly supports 5:1 compression ratio requirement
- [ ] Storage format for frozen tier uses BYTEA with zstd compression, not JSONB
- [ ] Compression algorithm field exists to track compression method
- [ ] Original size field exists to measure compression effectiveness
- [ ] System can successfully migrate memories to frozen tier based on P(r) < 0.2 threshold
- [ ] Frozen tier retrieval maintains 2-5 second intentional delay as specified
- [ ] Integration tests verify all four tiers operate correctly

### References
- Architecture Document: 4-Tier Memory Storage System (Section 3.2)
- MemGPT-inspired hierarchical memory design patterns
- Postgres-vector-optimizer audit findings from 2025-08-22

### Definition of Done
- [ ] Code changes pass all unit tests
- [ ] Integration tests pass with 100% success rate
- [ ] Database migration tested on staging environment
- [ ] Performance benchmarks meet specified latency targets
- [ ] Code review completed by at least two team members
- [ ] Documentation updated to reflect changes
- [ ] No critical security vulnerabilities introduced
- [ ] Changes deployed to development environment successfully

---

## CRITICAL-002: Implement Combined Score as Generated Column

**Story Type:** Performance  
**Priority:** Critical  
**Components:** Database Layer, Query Optimization  
**Estimated Points:** 3  

### Description
The three-component scoring system (recency + importance + relevance) is currently calculated at runtime for every query, violating the P99 <1ms latency requirement for working memory. This must be implemented as a PostgreSQL generated column for optimal performance.

### Acceptance Criteria
- [ ] Combined_score exists as a GENERATED ALWAYS AS column in memories table
- [ ] Formula correctly implements: (0.333 * recency_score + 0.333 * importance_score + 0.334 * relevance_score)
- [ ] Column is STORED rather than VIRTUAL for query performance
- [ ] Appropriate index exists on combined_score column (DESC order)
- [ ] Hot path queries for working memory achieve P99 <1ms latency
- [ ] Score automatically updates when component scores change
- [ ] No manual score calculation occurs in application code
- [ ] Query plans show index usage for score-based retrievals
- [ ] Performance tests demonstrate >50% improvement in query latency

### References
- Architecture Document: Three-Component Scoring System (Section 4.1)
- Cognitive Processing specifications for memory scoring
- Performance baseline requirements documentation

### Definition of Done
- [ ] Code changes pass all unit tests
- [ ] Integration tests pass with 100% success rate
- [ ] Database migration tested on staging environment
- [ ] Performance benchmarks meet specified latency targets
- [ ] Code review completed by at least two team members
- [ ] Documentation updated to reflect changes
- [ ] No critical security vulnerabilities introduced
- [ ] Changes deployed to development environment successfully

---

## CRITICAL-003: Consolidate MCP Protocol Implementation

**Story Type:** Technical Debt  
**Priority:** Critical  
**Components:** MCP Protocol Layer  
**Estimated Points:** 8  

### Description
System currently has two conflicting MCP implementations - one using proper stdio transport in main.rs and another using incorrect JSON-RPC over TCP in src/mcp/. This creates protocol conflicts and maintenance nightmares.

### Acceptance Criteria
- [ ] Single MCP implementation exists using stdio transport protocol
- [ ] All MCP functionality consolidated in appropriate module structure
- [ ] JSON-RPC over TCP implementation completely removed
- [ ] All MCP tools follow MCP protocol specification 2025-06-18
- [ ] Tool schemas properly validated and enforced
- [ ] Silent Harvester fully integrated with MCP layer
- [ ] "What did you remember?" query pattern properly implemented
- [ ] Circuit breaker implementation uses proper error handling (no panic!() calls)
- [ ] All MCP handlers properly timeout according to specifications
- [ ] MCP server responds correctly to all standard protocol messages

### References
- MCP Protocol Specification 2025-06-18
- Architecture Document: MCP Protocol Layer (Section 2.0)
- Silent Memory Harvesting design patterns

### Definition of Done
- [ ] Code changes pass all unit tests
- [ ] Integration tests pass with 100% success rate
- [ ] Database migration tested on staging environment
- [ ] Performance benchmarks meet specified latency targets
- [ ] Code review completed by at least two team members
- [ ] Documentation updated to reflect changes
- [ ] No critical security vulnerabilities introduced
- [ ] Changes deployed to development environment successfully

---

## HIGH-001: Implement MCP Authentication and Rate Limiting

**Story Type:** Security  
**Priority:** High  
**Components:** MCP Protocol Layer, Security  
**Estimated Points:** 5  

### Description
MCP protocol layer currently has no authentication or rate limiting, allowing unrestricted access to memory system operations. This violates security requirements and silent operation protocols.

### Acceptance Criteria
- [ ] Authentication middleware validates all MCP requests
- [ ] Support for API keys, tokens, or certificate-based authentication
- [ ] Rate limiting enforced per client/tool
- [ ] Rate limits configurable via environment variables
- [ ] Silent operation mode respects rate limits
- [ ] Authentication failures logged appropriately (security events)
- [ ] Rate limit violations return proper MCP error responses
- [ ] Documentation includes authentication setup guide
- [ ] Integration with existing security audit system
- [ ] Performance impact of auth layer <5ms per request

### References
- Architecture Document: Auth & Validation Layer (Section 2.3)
- Security compliance requirements
- MCP Protocol security best practices

### Definition of Done
- [ ] Code changes pass all unit tests
- [ ] Integration tests pass with 100% success rate
- [ ] Database migration tested on staging environment
- [ ] Performance benchmarks meet specified latency targets
- [ ] Code review completed by at least two team members
- [ ] Documentation updated to reflect changes
- [ ] No critical security vulnerabilities introduced
- [ ] Changes deployed to development environment successfully

---

## HIGH-002: Create Centralized Tier Management Service

**Story Type:** Feature  
**Priority:** High  
**Components:** Application Layer  
**Estimated Points:** 8  

### Description
Architecture specifies a dedicated 4-Tier Manager service but implementation uses scattered repository methods. Need centralized service to coordinate automated tier migrations based on memory scoring thresholds.

### Acceptance Criteria
- [ ] TierManager service exists as dedicated component
- [ ] Service monitors memory scores continuously
- [ ] Automated migration triggers at correct thresholds:
  - [ ] Working→Warm when P(r) < 0.7
  - [ ] Warm→Cold when P(r) < 0.5
  - [ ] Cold→Frozen when P(r) < 0.2
- [ ] Batch migration support for efficiency (1000 memories/sec target)
- [ ] Migration history tracked in consolidation_log table
- [ ] Service handles migration failures gracefully
- [ ] Configurable migration batch sizes and intervals
- [ ] Metrics exposed for migration performance monitoring
- [ ] Integration with existing memory repository
- [ ] Background job processing for non-blocking migrations

### References
- Architecture Document: 4-Tier Manager (Section 3.1)
- Memory tier migration rules and thresholds
- Cognitive memory research on forgetting curves

### Definition of Done
- [ ] Code changes pass all unit tests
- [ ] Integration tests pass with 100% success rate
- [ ] Database migration tested on staging environment
- [ ] Performance benchmarks meet specified latency targets
- [ ] Code review completed by at least two team members
- [ ] Documentation updated to reflect changes
- [ ] No critical security vulnerabilities introduced
- [ ] Changes deployed to development environment successfully

---

## HIGH-003: Fix Tier Migration Threshold Values

**Story Type:** Bug  
**Priority:** High  
**Components:** Cognitive Processing Layer  
**Estimated Points:** 2  

### Description
Tier migration thresholds in math_engine.rs don't match architecture specification. Current values cause incorrect memory migrations, affecting system performance and memory retention.

### Acceptance Criteria
- [ ] COLD_MIGRATION_THRESHOLD updated from 0.86 to 0.5
- [ ] FROZEN_MIGRATION_THRESHOLD updated from 0.3 to 0.2
- [ ] All references to these thresholds use correct values
- [ ] Migration logic properly evaluates against new thresholds
- [ ] Unit tests verify correct threshold application
- [ ] Integration tests confirm proper tier migrations
- [ ] No memories incorrectly retained or prematurely archived
- [ ] Performance metrics show improved memory distribution
- [ ] Documentation updated with correct threshold values

### References
- Architecture Document: Tier Migration Rules (Section 4.3)
- Cognitive memory research on retention probabilities
- Math engine specification for forgetting curves

### Definition of Done
- [ ] Code changes pass all unit tests
- [ ] Integration tests pass with 100% success rate
- [ ] Database migration tested on staging environment
- [ ] Performance benchmarks meet specified latency targets
- [ ] Code review completed by at least two team members
- [ ] Documentation updated to reflect changes
- [ ] No critical security vulnerabilities introduced
- [ ] Changes deployed to development environment successfully

---

## HIGH-004: Optimize Database Connection Pool Configuration

**Story Type:** Performance  
**Priority:** High  
**Components:** Database Layer, Configuration  
**Estimated Points:** 3  

### Description
Current connection pool configuration (10 connections) is severely undersized for >1000 ops/sec throughput requirement. PostgreSQL memory settings are also inadequate for vector operations.

### Acceptance Criteria
- [ ] Connection pool size increased to minimum 100 connections
- [ ] PgBouncer configuration supports 2000 client connections
- [ ] PostgreSQL memory settings updated:
  - [ ] shared_buffers set to 8GB (25-40% of available RAM)
  - [ ] effective_cache_size set to 24GB (50-75% of RAM)
  - [ ] maintenance_work_mem set to 2GB for vector index builds
  - [ ] work_mem set to 256MB for vector operations
- [ ] Connection pool properly handles connection recycling
- [ ] Timeout settings prevent connection exhaustion
- [ ] Load testing confirms >1000 ops/sec throughput
- [ ] No connection pool exhaustion under peak load
- [ ] Monitoring alerts configured for pool saturation
- [ ] Documentation includes tuning guidelines

### References
- Architecture Document: Performance Requirements (Section 5.0)
- PostgreSQL vector optimization best practices
- pgvector performance tuning documentation

### Definition of Done
- [ ] Code changes pass all unit tests
- [ ] Integration tests pass with 100% success rate
- [ ] Database migration tested on staging environment
- [ ] Performance benchmarks meet specified latency targets
- [ ] Code review completed by at least two team members
- [ ] Documentation updated to reflect changes
- [ ] No critical security vulnerabilities introduced
- [ ] Changes deployed to development environment successfully

---

## MEDIUM-001: Integrate Reflection Generator Service

**Story Type:** Feature  
**Priority:** Medium  
**Components:** Cognitive Processing Layer  
**Estimated Points:** 5  

### Description
Reflection Generator exists in codebase but isn't integrated into the main cognitive processing pipeline. This component should be creating meta-memories and generating insights from existing memories.

### Acceptance Criteria
- [ ] Reflection Generator integrated into main processing pipeline
- [ ] Service runs as background process at configurable intervals
- [ ] Meta-memories created based on pattern detection
- [ ] Insights table populated with generated insights
- [ ] Importance multiplier (1.5x) applied to insight-based memories
- [ ] Integration with existing memory scoring system
- [ ] Configurable thresholds for insight generation
- [ ] Performance impact <100ms for reflection operations
- [ ] Metrics exposed for insight generation rate
- [ ] Documentation includes reflection patterns guide

### References
- Architecture Document: Reflection Generator (Section 4.2)
- Cognitive architecture research on meta-cognition
- Memory consolidation patterns

### Definition of Done
- [ ] Code changes pass all unit tests
- [ ] Integration tests pass with 100% success rate
- [ ] Database migration tested on staging environment
- [ ] Performance benchmarks meet specified latency targets
- [ ] Code review completed by at least two team members
- [ ] Documentation updated to reflect changes
- [ ] No critical security vulnerabilities introduced
- [ ] Changes deployed to development environment successfully

---

## MEDIUM-002: Create Missing Database Tables

**Story Type:** Feature  
**Priority:** Medium  
**Components:** Database Layer  
**Estimated Points:** 3  

### Description
Several tables specified in architecture are missing from database schema: insights, harvest_sessions, consolidation_log, and knowledge graph tables.

### Acceptance Criteria
- [ ] harvest_sessions table created with proper schema
- [ ] insights table created with relationship to source memories
- [ ] consolidation_log table tracks all tier migrations
- [ ] Knowledge graph tables support relationship storage
- [ ] All tables have appropriate indexes for query performance
- [ ] Foreign key relationships properly established
- [ ] Migration scripts handle table creation safely
- [ ] Sample data validates table structures
- [ ] Integration with existing repository methods
- [ ] Documentation includes table relationship diagram

### References
- Architecture Document: Database Schema (Section 3.3)
- Entity relationship diagrams
- Data model specifications

### Definition of Done
- [ ] Code changes pass all unit tests
- [ ] Integration tests pass with 100% success rate
- [ ] Database migration tested on staging environment
- [ ] Performance benchmarks meet specified latency targets
- [ ] Code review completed by at least two team members
- [ ] Documentation updated to reflect changes
- [ ] No critical security vulnerabilities introduced
- [ ] Changes deployed to development environment successfully

---

## MEDIUM-003: Fix Layer Architecture Violations

**Story Type:** Technical Debt  
**Priority:** Medium  
**Components:** All Layers  
**Estimated Points:** 13  

### Description
Multiple components are violating layer boundaries by directly accessing lower layers instead of using proper abstractions. This creates tight coupling and maintenance issues.

### Acceptance Criteria
- [ ] Main.rs no longer contains business logic implementations
- [ ] All MCP operations routed through MCP layer handlers
- [ ] Backup module uses repository abstraction instead of direct PgPool
- [ ] Monitoring health checks use repository methods, not raw SQL
- [ ] Security modules access data through proper interfaces
- [ ] SemanticDeduplicationEngine uses repository, not direct database
- [ ] API layer routes through MCP protocol layer
- [ ] No modules skip their adjacent layer
- [ ] Dependency injection used for layer connections
- [ ] Clear interfaces defined between all layers

### References
- Architecture Document: System Layers (Section 1.0)
- Clean architecture principles
- Layer separation patterns

### Definition of Done
- [ ] Code changes pass all unit tests
- [ ] Integration tests pass with 100% success rate
- [ ] Database migration tested on staging environment
- [ ] Performance benchmarks meet specified latency targets
- [ ] Code review completed by at least two team members
- [ ] Documentation updated to reflect changes
- [ ] No critical security vulnerabilities introduced
- [ ] Changes deployed to development environment successfully