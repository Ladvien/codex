## Epic: Implement Human-Like Memory Consolidation & Cold Storage System

### Story 1: Database Schema Evolution for Consolidation
**Assignee:** Database Subagent  
**Story Points:** 5  
**Priority:** Critical

**Description:**
Extend the database schema to support memory consolidation, decay tracking, and frozen storage tier.

**Acceptance Criteria:**
- [ ] Add consolidation_strength (FLOAT, DEFAULT 1.0) column to memories table
- [ ] Add decay_rate (FLOAT, DEFAULT 1.0) column to memories table  
- [ ] Add recall_probability (FLOAT) column to memories table
- [ ] Add last_recall_interval (INTERVAL) column to memories table
- [ ] Add 'frozen' value to MemoryTier enum
- [ ] Create memory_consolidation_log table for tracking consolidation events
- [ ] Create frozen_memories archive table with compressed JSONB storage
- [ ] Add indexes for consolidation_strength and recall_probability queries
- [ ] Migration script executes without errors and is reversible

**Definition of Done:**
- All migrations run successfully on test and production databases
- No existing data is lost during migration
- Performance benchmarks show <5% degradation on existing queries
- Rollback script tested and verified
- All existing tests pass
- New schema documented in database documentation

**Test Requirements:**
```sql
-- Test migration up and down
-- Test that existing memories get default consolidation values
-- Test frozen tier enum works correctly
-- Test indexes improve query performance
```

---

### Story 2: Consolidation Mathematics Engine
**Assignee:** Algorithm Subagent  
**Story Points:** 8  
**Priority:** Critical

**Description:**
Implement the mathematical models for memory decay, consolidation, and recall probability calculations.

**Acceptance Criteria:**
- [ ] Implement forgetting curve formula: p(t) = [1 - exp(-r*e^(-t/gn))] / (1 - e^-1)
- [ ] Implement consolidation update: gn = gn-1 + (1 - e^-t)/(1 + e^-t)
- [ ] Calculate decay rate based on access patterns
- [ ] Handle edge cases (new memories, never accessed, etc.)
- [ ] Configurable thresholds for memory tiers (0.86 for cold, 0.3 for frozen)
- [ ] Batch processing capability for efficiency
- [ ] Mathematical accuracy within 0.001 tolerance

**Definition of Done:**
- Unit tests cover all mathematical functions with known inputs/outputs
- Property-based tests verify mathematical properties hold
- Benchmarks show <10ms calculation time per memory
- Code reviewed by second developer
- Mathematical correctness verified against research paper examples
- Documentation includes formula derivations

**Test Requirements:**
```rust
#[test]
fn test_forgetting_curve_new_memory() { /* ... */ }
#[test]
fn test_consolidation_strength_increases() { /* ... */ }
#[test]
fn test_recall_probability_decreases_over_time() { /* ... */ }
#[test]
fn test_batch_calculation_performance() { /* ... */ }
```

---

### Story 3: Memory Similarity & Merging System
**Assignee:** ML Subagent  
**Story Points:** 13  
**Priority:** High

**Description:**
Implement semantic similarity detection and intelligent memory merging to reduce redundancy.

**Acceptance Criteria:**
- [ ] Calculate cosine similarity between memory embeddings
- [ ] Identify memory clusters with similarity > 0.9
- [ ] Merge algorithm preserves all metadata in combined format
- [ ] Generate new embedding for merged content
- [ ] Track merge history for potential unmerge
- [ ] Handle parent-child relationships during merge
- [ ] Configurable similarity thresholds per tier
- [ ] Async processing to avoid blocking operations

**Definition of Done:**
- Integration tests verify correct merging behavior
- No data loss during merge operations
- Merged memories are retrievable by original queries
- Performance: can process 10,000 memories in <30 seconds
- Rollback capability tested
- Merge events logged in audit trail

**Test Requirements:**
```rust
#[test]
async fn test_similarity_detection_accuracy() { /* ... */ }
#[test]
async fn test_merge_preserves_metadata() { /* ... */ }
#[test]
async fn test_merge_improves_search_quality() { /* ... */ }
#[test]
async fn test_merge_rollback() { /* ... */ }
```

---

### Story 4: Automated Tier Migration Service
**Assignee:** Backend Subagent 1  
**Story Points:** 8  
**Priority:** Critical

**Description:**
Create background service that automatically migrates memories between tiers based on recall probability and consolidation strength.

**Acceptance Criteria:**
- [ ] Background task runs every configurable interval (default 1 hour)
- [ ] Migrate Working→Warm when recall probability < 0.7
- [ ] Migrate Warm→Cold when recall probability < 0.5
- [ ] Migrate Cold→Frozen when recall probability < 0.2
- [ ] Batch migrations for efficiency (100 memories per transaction)
- [ ] Respect tier capacity limits
- [ ] Emit metrics for monitoring
- [ ] Graceful shutdown handling
- [ ] Configurable via environment variables

**Definition of Done:**
- Service runs continuously for 24 hours without memory leaks
- Migrations logged with timing and count metrics
- Error recovery tested (database failures, etc.)
- CPU usage < 5% during normal operation
- Integration tests verify tier transitions
- Monitoring dashboard shows migration patterns

**Test Requirements:**
```rust
#[tokio::test]
async fn test_migration_service_lifecycle() { /* ... */ }
#[tokio::test]
async fn test_tier_capacity_limits_respected() { /* ... */ }
#[tokio::test]
async fn test_migration_rollback_on_error() { /* ... */ }
```

---

### Story 5: Frozen Storage Implementation
**Assignee:** Backend Subagent 2  
**Story Points:** 13  
**Priority:** High

**Description:**
Implement the frozen storage tier with compression, archival, and intentionally slow retrieval.

**Acceptance Criteria:**
- [ ] Compress memories using zstd before frozen storage
- [ ] Store in separate frozen_memories table or S3-compatible storage
- [ ] Retrieval requires explicit "unfreeze" operation
- [ ] Unfreeze operation takes 2-5 seconds (simulated cognitive effort)
- [ ] Batch unfreezing capability for related memories
- [ ] Memory graph relationships preserved in frozen state
- [ ] Search excludes frozen memories unless explicitly requested
- [ ] Compression ratio > 5:1 for text content

**Definition of Done:**
- Frozen memories use 80% less storage than active memories
- Retrieval time consistently 2-5 seconds
- Data integrity verified after freeze/unfreeze cycle
- S3 integration tested if configured
- Benchmarks show 100K memories can be frozen
- API documentation updated

**Test Requirements:**
```rust
#[test]
async fn test_freeze_compression_ratio() { /* ... */ }
#[test]
async fn test_unfreeze_preserves_data() { /* ... */ }
#[test]
async fn test_frozen_search_exclusion() { /* ... */ }
#[test]
async fn test_batch_unfreeze_performance() { /* ... */ }
```

---

### Story 6: Subgoal-Based Memory Chunking
**Assignee:** Cognitive Subagent  
**Story Points:** 8  
**Priority:** High

**Description:**
Implement HiAgent-style hierarchical memory chunking for completed subgoals.

**Acceptance Criteria:**
- [ ] Detect task/subgoal completion patterns
- [ ] Chunk related memories into subgoal units
- [ ] Generate summary for each completed subgoal
- [ ] Create new embedding for summary
- [ ] Maintain links to original memories
- [ ] 26.4% reduction in context size (per research)
- [ ] Queryable by subgoal or original content
- [ ] Configurable chunking strategies

**Definition of Done:**
- Chunking reduces memory count by >25%
- Summaries accurately represent chunked content
- Original memories retrievable from chunks
- Performance improvement measurable in search
- Integration tests verify chunking behavior
- Documentation includes chunking strategies

**Test Requirements:**
```rust
#[test]
fn test_subgoal_detection_accuracy() { /* ... */ }
#[test]
fn test_chunk_summary_quality() { /* ... */ }
#[test]
fn test_context_reduction_percentage() { /* ... */ }
```

---

### Story 7: Memory Access Pattern Analyzer
**Assignee:** Analytics Subagent  
**Story Points:** 5  
**Priority:** Medium

**Description:**
Build analytics system to track and analyze memory access patterns for optimizing consolidation parameters.

**Acceptance Criteria:**
- [ ] Track access frequency, recency, and patterns
- [ ] Identify memory access clusters
- [ ] Calculate optimal decay rates per memory type
- [ ] Generate access heatmaps
- [ ] Predict future access likelihood
- [ ] Export metrics to Prometheus
- [ ] Real-time pattern detection
- [ ] Anomaly detection for unusual access patterns

**Definition of Done:**
- Analytics process <1ms overhead per access
- Predictions achieve >70% accuracy
- Grafana dashboard displays patterns
- No performance impact on main operations
- Unit tests cover all analytics functions
- Documentation includes metric definitions

---

### Story 8: Consolidation REST API & MCP Tools
**Assignee:** API Subagent  
**Story Points:** 5  
**Priority:** Medium

**Description:**
Expose consolidation features through REST API and MCP tools for Claude Desktop integration.

**Acceptance Criteria:**
- [ ] POST /api/v1/memories/consolidate - trigger consolidation
- [ ] GET /api/v1/memories/frozen - list frozen memories
- [ ] POST /api/v1/memories/{id}/unfreeze - unfreeze specific memory
- [ ] GET /api/v1/analytics/consolidation - get consolidation metrics
- [ ] MCP tool: consolidate_memories
- [ ] MCP tool: search_frozen_memories
- [ ] OpenAPI documentation generated
- [ ] Rate limiting on consolidation endpoints

**Definition of Done:**
- All endpoints return correct HTTP status codes
- API tests achieve 100% coverage
- MCP tools work in Claude Desktop
- Response times <100ms (except unfreeze)
- Error messages are user-friendly
- API versioning implemented

---

### Story 9: Memory Reflection & Insight Generation
**Assignee:** AI Subagent  
**Story Points:** 13  
**Priority:** Medium

**Description:**
Implement reflection system that generates insights from consolidated memory patterns.

**Acceptance Criteria:**
- [ ] Periodic reflection task (daily/weekly)
- [ ] Identify contradictions in memories
- [ ] Synthesize related memories into insights
- [ ] Generate meta-memories from patterns
- [ ] Importance scoring for insights
- [ ] Link insights to source memories
- [ ] Configurable reflection strategies
- [ ] LLM integration for insight generation

**Definition of Done:**
- Insights are meaningful and actionable
- No hallucinated information in insights
- Source memories traceable from insights
- Reflection completes in <5 minutes for 10K memories
- A/B tests show improved retrieval with insights
- User feedback mechanism implemented

---

### Story 10: Performance Testing & Optimization
**Assignee:** Performance Subagent  
**Story Points:** 8  
**Priority:** High

**Description:**
Comprehensive performance testing and optimization of consolidation system.

**Acceptance Criteria:**
- [ ] Load test with 1M+ memories
- [ ] Consolidation maintains <10ms p99 latency
- [ ] Memory usage grows logarithmically, not linearly
- [ ] Frozen storage retrieval 2-5 seconds consistently
- [ ] No memory leaks over 7-day test
- [ ] Database connection pool optimized
- [ ] Benchmark suite automated in CI/CD
- [ ] Performance regression detection

**Definition of Done:**
- All performance targets met or exceeded
- Benchmarks documented and baselined
- Performance dashboard deployed
- Optimization recommendations documented
- Load testing reproducible
- Performance regression tests in CI

**Test Requirements:**
```rust
#[bench]
fn bench_consolidation_calculation() { /* ... */ }
#[bench]
fn bench_similarity_detection() { /* ... */ }
#[bench]
fn bench_tier_migration() { /* ... */ }
#[bench]
fn bench_frozen_retrieval() { /* ... */ }
```

---

## Cross-Story Integration Tests

All stories must pass these integration tests:

```rust
#[tokio::test]
async fn test_end_to_end_memory_lifecycle() {
    // Create memory → Access multiple times → 
    // Watch consolidation increase → Tier migration → 
    // Eventually freeze → Explicit unfreeze → Verify data
}

#[tokio::test]
async fn test_system_under_load() {
    // 10K concurrent operations
    // Verify consolidation doesn't block operations
    // Verify tier migrations handle load
    // Verify frozen storage remains responsive
}
