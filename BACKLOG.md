# Product Backlog - Codex Memory System

## Epic Overview
This backlog addresses critical issues found during comprehensive code review by the specialized agent team. Issues are organized by priority and epic for systematic resolution.

---

## 🔴 CRITICAL PRIORITY - Security & Stability

### EPIC: Security Hardening & Authentication
**Epic Description:** Address critical security vulnerabilities and authentication gaps that could compromise system security in production.

#### ✅ CODEX-001: Fix MCP Protocol JSON-RPC Compliance Violations [COMPLETED]
- **Priority:** Critical  
- **Story Points:** 8
- **Components:** MCP Server, Protocol Implementation
- **Resolution Summary:**
  - ✅ Implemented comprehensive JSON-RPC validation in transport layer
  - ✅ Added JSON-RPC version validation in authentication handler
  - ✅ Created header extraction mechanism for stdio transport
  - ✅ Added 15 comprehensive test cases covering all validation scenarios
  - ✅ All MCP responses maintain proper JSON-RPC structure via helper functions
- **Commit:** 6b0ee2c - fix: Implement JSON-RPC 2.0 compliance validation in MCP protocol

#### CODEX-002: Implement Proper Authentication Security
- **Priority:** Critical  
- **Story Points:** 13
- **Components:** Authentication, Security
- **Acceptance Criteria:**
  - Replace predictable default JWT secret with secure random generation
  - Implement token rotation mechanism
  - Add certificate expiry validation beyond thumbprint checking
  - Production authentication warnings logged to audit trail, not just stderr
  - Add audit logging for authentication bypasses
- **Technical Details:**
  - Files: `/src/mcp_server/auth.rs` lines 75-77, 179-199
  - Current default JWT secret is predictable
  - Certificate validation only checks thumbprint, no expiry validation

#### CODEX-003: Fix SQL Injection Vulnerabilities
- **Priority:** Critical
- **Story Points:** 5
- **Components:** Database, Security
- **Acceptance Criteria:**
  - Replace all dynamic SQL construction with parameterized queries
  - Remove format! macro usage in database query building
  - Implement safe query builder that prevents injection attacks
  - All instances in `/src/database_setup.rs` lines 164, 465, 479 secured
- **Technical Details:**
  - Current: `format!("CREATE DATABASE \"{}\"", db_info.database)`
  - Risk: SQL injection potential from user-controlled database names
  - Fix: Use proper parameterized queries or prepared statements

### EPIC: Memory Leak & Resource Management
**Epic Description:** Address critical memory leaks and resource management issues that could cause system instability.

#### CODEX-004: Fix Database Connection Leaks
- **Priority:** Critical
- **Story Points:** 8
- **Components:** Database, Connection Management
- **Acceptance Criteria:**
  - Implement proper connection pooling with cleanup mechanisms
  - Replace individual connection spawning with pool-based approach
  - Add connection leak detection and monitoring
  - All spawned tasks in `/src/database_setup.rs` lines 103-107, 143-147, 185-189, 220-226, 264-270 properly managed
- **Technical Details:**
  - Issue: `tokio::spawn(async move { connection.await })` without proper cleanup
  - Impact: Connection leaks under error conditions
  - Missing proper connection pooling implementation

#### CODEX-005: Fix Background Task Resource Leaks  
- **Priority:** Critical
- **Story Points:** 5
- **Components:** Task Management, MCP Server
- **Acceptance Criteria:**
  - Implement proper lifecycle management for background tasks
  - Add timeout protection to prevent resource exhaustion attacks
  - Implement task cleanup mechanisms with proper shutdown handling
  - Fix spawned tasks in `/src/mcp_server/handlers.rs` lines 395-444, 645-811, 696-716
- **Technical Details:**
  - `harvest_conversation` spawns background tasks without timeout limits
  - Chunked processing lacks individual chunk timeout
  - Background tokio::spawn tasks have no cleanup mechanism

#### CODEX-006: Fix Rate Limiter Memory Leaks
- **Priority:** Critical
- **Story Points:** 3
- **Components:** Rate Limiting, Memory Management  
- **Acceptance Criteria:**
  - Implement cleanup/garbage collection for client rate limiters
  - Add bounded growth protection for rate limiter HashMap
  - Implement distributed rate limiting support for multi-instance deployments
  - Fix indefinite growth in `/src/mcp_server/rate_limiter.rs` lines 262-296
- **Technical Details:**
  - Client rate limiters created on-demand without cleanup
  - HashMap grows indefinitely causing memory leak
  - Silent mode multiplier exploitation possible

---

## 🟠 HIGH PRIORITY - Performance & Functionality

### EPIC: Database Performance Optimization  
**Epic Description:** Address N+1 query patterns, missing indexes, and database performance bottlenecks.

#### CODEX-007: Fix N+1 Query Pattern in Memory Access Tracking
- **Priority:** High
- **Story Points:** 8
- **Components:** Database, Performance
- **Acceptance Criteria:**
  - Replace individual UPDATE statements with batch operations
  - Implement async access tracking to prevent blocking
  - Reduce memory access tracking from seconds to milliseconds for batch operations
  - Fix trigger function causing N+1 pattern in `/src/memory/repository.rs` lines 234-241
- **Technical Details:**
  - Current: Individual UPDATE statements in trigger function
  - Impact: Seconds of delay for bulk operations
  - Solution: Batch update or async access tracking

#### CODEX-008: Implement Cursor-Based Pagination
- **Priority:** High
- **Story Points:** 5
- **Components:** Database, Pagination, Performance
- **Acceptance Criteria:**
  - Replace OFFSET-based pagination with cursor-based approach
  - Eliminate O(n) performance degradation with page depth
  - Update all pagination queries to use cursor approach
  - Fix pagination in `/src/memory/repository.rs` lines 199-200, 662-663
- **Technical Details:**
  - Current: `LIMIT ${} OFFSET ${}` becomes O(n) for large offsets
  - Impact: Exponential slowdown with page depth
  - Solution: Cursor-based pagination using indexed columns

#### CODEX-009: Optimize Vector Search Performance
- **Priority:** High
- **Story Points:** 8
- **Components:** Vector Search, Database Indexes
- **Acceptance Criteria:**
  - Add HNSW index usage hints to vector similarity queries
  - Optimize ef_search parameter for 1536-dimensional vectors
  - Eliminate full vector table scans on large datasets
  - Update vector search in `/src/memory/repository.rs` lines 650-663
- **Technical Details:**
  - Current: Basic cosine distance without proper indexing hints
  - Missing: HNSW index optimization (m=16, ef_construction=500)
  - Impact: Full vector table scans on large datasets

#### CODEX-010: Add Missing Database Indexes
- **Priority:** High  
- **Story Points:** 3
- **Components:** Database, Indexes, Performance
- **Acceptance Criteria:**
  - Add composite index on (tier, status, last_accessed_at DESC NULLS LAST)
  - Add specialized index on (recall_probability, tier) for freeze operations
  - Set vector column statistics target to 1000 for better query planning
  - Eliminate table scans on temporal queries
- **Technical Details:**
  - Missing indexes cause full table scans on ORDER BY last_accessed_at queries
  - Freeze operation queries without index support for P(r) < 0.2 threshold
  - Poor query planner estimates for vector operations

### EPIC: Cognitive Architecture Accuracy
**Epic Description:** Fix mathematical formulas and cognitive science implementation deviations.

#### CODEX-011: Fix Three-Component Scoring Mathematical Errors
- **Priority:** High
- **Story Points:** 13
- **Components:** Cognitive Architecture, Mathematical Accuracy
- **Acceptance Criteria:**
  - Correct recency calculation lambda value from 0.005 to 0.99 per Park et al. (2023)
  - Fix access pattern normalization to use log-scaled denominator
  - Replace static 0.2 base relevance fallback with importance_factor * 0.1
  - Update mathematical formulas in `/src/memory/three_component_scoring.rs` lines 328, 346-347, 375-376, 383-391
- **Technical Details:**
  - Current lambda (0.005) causes too rapid memory decay
  - Research compliance: Park et al. used λ=0.99 for hourly decay
  - Cognitive implausibility: Static relevance violates Collins & Loftus theory

#### CODEX-012: Implement Proper Forgetting Curve Formula
- **Priority:** High
- **Story Points:** 8  
- **Components:** Mathematical Engine, Cognitive Science
- **Acceptance Criteria:**
  - Replace incorrect formula with research-backed `R(t) = e^(-t/S)` implementation
  - Fix documentation discrepancy between documented and actual formulas
  - Implement Bahrick (1984) permastore research thresholds (0.7 for long-term retention)
  - Correct formula in `/src/memory/math_engine.rs` lines 12-13, 49-51
- **Technical Details:**
  - Documented: `p(t) = [1 - exp(-r * e^(-t/gn))] / (1 - e^(-1))`
  - Should be: `R(t) = e^(-t/S)` where S = consolidation strength
  - Migration thresholds lack cognitive research justification

#### CODEX-013: Implement Working Memory Capacity Constraints
- **Priority:** High
- **Story Points:** 5
- **Components:** Memory Tiers, Cognitive Architecture
- **Acceptance Criteria:**
  - Enforce Miller's 7±2 rule for working memory tier (5-9 items maximum)
  - Implement chunking mechanisms for overflow handling
  - Add working memory capacity validation and constraints
  - Update memory models in `/src/memory/models.rs` lines 39-44
- **Technical Details:**
  - Missing enforcement of cognitive working memory limits
  - No chunking mechanisms for overflow
  - Violates established cognitive research on working memory capacity

### EPIC: Error Handling & Reliability
**Epic Description:** Improve error handling, remove unsafe practices, and enhance system reliability.

#### CODEX-014: Eliminate Unsafe unwrap() Usage
- **Priority:** High
- **Story Points:** 8
- **Components:** Error Handling, Production Safety
- **Acceptance Criteria:**
  - Replace all unwrap() and expect() calls with proper Result handling
  - Add structured error logging with correlation IDs  
  - Implement graceful error recovery mechanisms
  - Fix unsafe usage in `/src/mcp_server/handlers.rs` lines 257, 846, 880 and other locations
- **Technical Details:**
  - 20+ instances of unwrap()/expect() violate CLAUDE.md requirements
  - Risk: Potential runtime panics in production
  - Locations: setup.rs, embedding.rs, performance/dashboard.rs

#### CODEX-015: Implement Proper Circuit Breaker Error Handling
- **Priority:** High
- **Story Points:** 5
- **Components:** Circuit Breaker, Error Context
- **Acceptance Criteria:**
  - Preserve original error context in circuit breaker wrapper
  - Implement proper error chain propagation
  - Add call success/failure ratio tracking in half-open state
  - Fix error context loss in `/src/mcp_server/circuit_breaker.rs` lines 87-99
- **Technical Details:**
  - Call() method converts all errors to CircuitBreakerError::CircuitOpen
  - Loses original error context for debugging
  - Half-open state doesn't track call metrics properly

#### CODEX-016: Complete TODO Implementations
- **Priority:** High
- **Story Points:** 13
- **Components:** API Functionality, Feature Completion
- **Acceptance Criteria:**
  - Implement privacy mode functionality or return proper "not implemented" responses
  - Complete configuration persistence mechanisms  
  - Implement harvester toggle functionality
  - Address all TODO items: config_api.rs:116,232,249; harvester_api.rs:144
- **Technical Details:**
  - Runtime failures possible if unimplemented endpoints are called
  - Missing critical functionality in production code paths
  - 16 TODO items indicate incomplete functionality

---

## 🟡 MEDIUM PRIORITY - Quality & Documentation

### EPIC: Documentation & Knowledge Preservation
**Epic Description:** Address critical documentation gaps that impact system operation and maintenance.

#### CODEX-017: Create Comprehensive Error Reference Guide  
- **Priority:** Medium
- **Story Points:** 8
- **Components:** Documentation, Error Handling
- **Acceptance Criteria:**
  - Document all error codes from `/src/memory/error.rs` with mappings
  - Include common causes, recovery procedures, and troubleshooting steps
  - Create `docs/error_reference.md` with user-friendly error explanations
  - Add correlation between error types and system components
- **Technical Details:**
  - No centralized error message documentation despite comprehensive error types
  - Developers and users cannot troubleshoot issues effectively
  - Missing error code mappings and recovery procedures

#### CODEX-018: Create Production Deployment Guide
- **Priority:** Medium
- **Story Points:** 13  
- **Components:** Documentation, Deployment, Operations
- **Acceptance Criteria:**
  - Document service installation procedures for systemd/launchd
  - Create production security hardening guide
  - Document backup/recovery procedures with examples
  - Include monitoring setup and alerting threshold configuration
  - Cover TLS setup, authentication configuration, and performance tuning
- **Technical Details:**
  - SETUP.md covers development but lacks production guidance  
  - Missing systemd/launchd service files documentation
  - No production security or backup/recovery procedures
  - Rich monitoring system exists but lacks operational documentation

#### CODEX-019: Document Configuration Schema and Examples
- **Priority:** Medium
- **Story Points:** 5
- **Components:** Configuration, Documentation
- **Acceptance Criteria:**
  - Document all configuration options in `/src/config.rs` with validation rules
  - Provide configuration examples for different deployment scenarios
  - Include schema validation and error handling examples
  - Document privacy mode, persistence, and harvester configurations
- **Technical Details:**
  - Complex configuration lacks comprehensive documentation
  - Missing validation rules and configuration examples
  - 16 TODO items indicate incomplete configuration functionality

### EPIC: Code Quality & Performance Monitoring
**Epic Description:** Implement comprehensive testing, monitoring, and performance optimizations.

#### CODEX-020: Implement Comprehensive Test Coverage
- **Priority:** Medium
- **Story Points:** 21
- **Components:** Testing, Quality Assurance  
- **Acceptance Criteria:**
  - Achieve 80%+ code coverage minimum per CLAUDE.md standards
  - Add unit tests for all API endpoints and error paths
  - Implement integration tests for database operations
  - Add performance regression tests and benchmarks
  - Focus on unhappy path testing for error conditions
- **Technical Details:**
  - Only `/src/database_setup.rs` has unit tests (lines 614-649)
  - API endpoints and error paths completely untested
  - High risk of runtime failures in production without proper test coverage

#### CODEX-021: Implement Structured Logging and Observability
- **Priority:** Medium  
- **Story Points:** 8
- **Components:** Logging, Monitoring, Observability
- **Acceptance Criteria:**
  - Add structured logging with tracing spans for request/response cycles
  - Implement metrics collection for performance monitoring
  - Add health checks and SLA/SLO monitoring
  - Create operational runbooks for common scenarios
  - Add correlation IDs for distributed request tracing
- **Technical Details:**
  - Missing structured logging for request/response cycles
  - No metrics collection for performance monitoring per CLAUDE.md standards
  - Missing observability for production troubleshooting

#### CODEX-022: Optimize Vector Index Configuration
- **Priority:** Medium
- **Story Points:** 5
- **Components:** Database, Vector Search, Performance
- **Acceptance Criteria:**
  - Configure HNSW index with optimal parameters (m=16, ef_construction=500) for 1536-dim vectors
  - Optimize autovacuum settings for vector workload (autovacuum_vacuum_scale_factor = 0.05)
  - Implement statement timeout differentiation (5s for searches, 30s for batch operations)
  - Update index configuration in `/migration/migrations/001_initial_schema.sql` lines 122-124
- **Technical Details:**
  - HNSW index uses default parameters, not optimized for 1536-dimensional vectors
  - Default autovacuum settings may not handle vector index bloat optimally
  - Statement timeout (30s) may be too long for real-time vector searches

### EPIC: Cognitive Science Research Compliance  
**Epic Description:** Ensure cognitive architecture aligns with established research and best practices.

#### CODEX-023: Fix Consolidation Algorithm Research Compliance
- **Priority:** Medium
- **Story Points:** 8
- **Components:** Cognitive Consolidation, Research Compliance
- **Acceptance Criteria:**
  - Update alpha parameter from 0.3 to 0.6-0.8 per Bjork (1994) research
  - Increase min_spacing_hours from 0.5 to 1.0 to align with spacing effect research
  - Reduce max_strength from 15.0 to ≤10.0 for cognitive plausibility
  - Implement optimal interval calculation: R = R × (2.5 + (0.15 × EF))
  - Fix parameters in `/src/memory/cognitive_consolidation.rs` lines 76-84, 164-179
- **Technical Details:**
  - Current parameters violate spaced repetition research findings
  - Missing critical components of spacing effect calculation
  - No implementation of generation effect in testing calculations

#### CODEX-024: Implement Proper Reflection Engine Cognitive Parameters
- **Priority:** Medium
- **Story Points:** 5  
- **Components:** Reflection Engine, Cognitive Science
- **Acceptance Criteria:**
  - Replace arbitrary importance_trigger_threshold (150.0) with information density-based triggers per Flavell (1979)
  - Adjust clustering_similarity_threshold from 0.75 to 0.6-0.65 per Collins & Loftus research
  - Update min_cluster_size from 3 to 2 to align with dual-coding theory
  - Add confidence-based reflection triggers implementation
  - Update parameters in `/src/memory/reflection_engine.rs` lines 86-87, 92-93, 96
- **Technical Details:**
  - Current thresholds lack cognitive research basis
  - Clustering parameters don't match semantic network research
  - Missing metacognition research compliance

---

## 🟢 LOW PRIORITY - Enhancements & Optimization

### EPIC: Performance Optimization & Fine-Tuning
**Epic Description:** Minor performance improvements and optimization opportunities.

#### CODEX-025: Optimize String Allocation Performance
- **Priority:** Low
- **Story Points:** 3
- **Components:** Performance, Memory Allocation
- **Acceptance Criteria:**
  - Replace O(n²) vector to string conversion with optimized approach
  - Use `format!` with pre-allocated capacity for large vector serialization
  - Implement dedicated serialization for vector operations
  - Optimize `/src/database_setup.rs` lines 458-477
- **Technical Details:**
  - Current: `vec![0.1f32; 768].iter().map(|f| f.to_string()).collect::<Vec<_>>().join(",")`
  - Impact: Unnecessary allocations and poor performance for large vectors
  - Solution: Pre-allocated capacity or dedicated serialization

#### CODEX-026: Optimize Connection Pool Configuration
- **Priority:** Low
- **Story Points:** 2
- **Components:** Database, Connection Management
- **Acceptance Criteria:**
  - Align PgBouncer pool sizes with application connection pool settings
  - Optimize min_connections from 20 to 25-30 for better prewarming
  - Document optimal connection pool sizing for different deployment scenarios
- **Technical Details:**
  - Pool sizes don't align between PgBouncer (100) and connection.rs (100 max, 20 min)
  - Potential connection exhaustion under load
  - Suboptimal connection prewarming

#### CODEX-027: Implement Request Size Validation
- **Priority:** Low  
- **Story Points:** 2
- **Components:** Security, Input Validation
- **Acceptance Criteria:**
  - Add request size limits before JSON parsing in transport layer
  - Implement proper input validation for large payloads
  - Add monitoring for request size distribution
  - Update transport layer in `/src/mcp_server/transport.rs` lines 112-127
- **Technical Details:**
  - No request size validation before JSON parsing
  - Potential DoS vector through large request payloads
  - Missing input size limits

### EPIC: Minor Features & Quality of Life
**Epic Description:** Small improvements and feature completions.

#### CODEX-028: Improve API Error Response Quality  
- **Priority:** Low
- **Story Points:** 3
- **Components:** API, Error Handling, User Experience
- **Acceptance Criteria:**
  - Replace generic BAD_REQUEST responses with specific error messages
  - Include field-level validation errors in response body
  - Add request correlation IDs to all error responses
  - Improve error responses in `/src/api/config_api.rs` lines 143, 151, 159, 168, 177
- **Technical Details:**
  - Current: Returns generic BAD_REQUEST without context
  - Impact: Poor API usability, difficult troubleshooting for users
  - Solution: Structured error responses with detailed validation messages

#### CODEX-029: Standardize Documentation Terminology
- **Priority:** Low
- **Story Points:** 2
- **Components:** Documentation, Consistency
- **Acceptance Criteria:**
  - Create comprehensive glossary defining domain-specific terms
  - Standardize usage of "memory", "memories", "memory entries" throughout documentation  
  - Update all documentation to use consistent terminology
  - Define "Agentic Memory System" vs "Memory System" vs "Codex Memory" usage
- **Technical Details:**
  - Mixed terminology usage causes confusion for developers and users
  - No glossary defining domain-specific terms
  - Inconsistent naming conventions across documentation

#### CODEX-030: Complete Integration Examples Documentation
- **Priority:** Low
- **Story Points:** 5
- **Components:** Documentation, Integration, Examples  
- **Acceptance Criteria:**
  - Create real-world MCP client integration examples
  - Document batch operation patterns and best practices
  - Add error handling examples for common scenarios
  - Include performance optimization examples and patterns
  - Extend API_REFERENCE.md with comprehensive integration guidance
- **Technical Details:**
  - Current API_REFERENCE.md shows individual endpoints but lacks integration patterns
  - Missing batch operation examples and error handling guidance
  - No performance optimization examples for developers

---

## Summary Statistics

**Total Stories:** 30
**Epic Count:** 9

### Priority Distribution:
- **Critical:** 6 stories (Security & Stability)
- **High:** 10 stories (Performance & Functionality)  
- **Medium:** 10 stories (Quality & Documentation)
- **Low:** 4 stories (Enhancements & Optimization)

### Story Points Distribution:
- **Critical:** 42 points
- **High:** 84 points
- **Medium:** 68 points  
- **Low:** 17 points
- **Total:** 211 points

### Key Focus Areas:
1. **Security Hardening** (42 points) - Address authentication vulnerabilities and SQL injection risks
2. **Performance Optimization** (84 points) - Fix N+1 queries, implement proper indexing, optimize vector search
3. **Cognitive Architecture Compliance** (68 points) - Align mathematical formulas with research, fix cognitive parameters
4. **Documentation & Operations** (17 points) - Create production guides, error references, and integration examples

### Immediate Sprint Recommendations:
**Sprint 1 (Critical Security):** CODEX-001, CODEX-002, CODEX-003, CODEX-004
**Sprint 2 (Performance Core):** CODEX-007, CODEX-008, CODEX-009, CODEX-010
**Sprint 3 (Cognitive Accuracy):** CODEX-011, CODEX-012, CODEX-013, CODEX-014

This backlog provides a systematic approach to addressing all issues identified by the specialized agent team, with clear priorities and actionable acceptance criteria for each story.