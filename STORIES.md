# Epic: Upgrade Memory System to SOTA Cognitive Architecture

## Story 5: Implement Semantic Deduplication System
**Assignee:** Data Subagent  
**Story Points:** 8  
**Priority:** High  
**Dependencies:** Story 1

**Description:**
Prevent memory bloat through intelligent deduplication and merging.

**Acceptance Criteria:**
- [ ] Detect memories with cosine similarity > 0.85
- [ ] Merge algorithm preserves all metadata
- [ ] Combined embedding generation for merged memories
- [ ] Track merge history for potential unmerge
- [ ] Hierarchical compression (critical=lossless, normal=lossy)
- [ ] Auto-prune when P(recall) < 0.2 after 30 days
- [ ] Maintain 20% memory headroom
- [ ] Audit trail for all merge/prune operations

**Definition of Done:**
- Deduplication reduces storage by > 30%
- No information loss in critical memories
- Merge operations reversible for 7 days
- Compression achieves 3:1 ratio minimum
- Performance: deduplicate 10K memories in < 30 seconds
- Pruning runs daily without service interruption

---

## Story 6: Create Reflection & Insight Generator
**Assignee:** AI Subagent  
**Story Points:** 13  
**Priority:** Medium  
**Dependencies:** Stories 1, 2

**Description:**
Generate higher-level insights from accumulated memories through reflection.

**Acceptance Criteria:**
- [ ] Trigger reflection when importance sum > 150 points
- [ ] Generate 2-3 insights per reflection
- [ ] Create meta-memories linking source memories
- [ ] Build knowledge graph from relationships
- [ ] Importance scoring for insights (1.5x base memories)
- [ ] Configurable reflection strategies
- [ ] LLM prompt templates for insight generation
- [ ] Prevent insight loops/redundancy

**Definition of Done:**
- Insights are meaningful and actionable
- Source memories traceable from insights
- Knowledge graph visualizable
- Reflection completes in < 30 seconds
- A/B tests show 25% better retrieval with insights
- User feedback mechanism implemented

---

## Story 7: Add Event-Triggered Scoring System
**Assignee:** Backend Subagent  
**Story Points:** 5  
**Priority:** High  
**Dependencies:** Story 3

**Description:**
Implement immediate evaluation for critical content patterns.

**Acceptance Criteria:**
- [ ] Define TriggerEvent enum with 5 types
- [ ] Pattern detection for each trigger type
- [ ] Immediate processing pipeline bypass
- [ ] Boost importance by 2x for triggered events
- [ ] Configure trigger patterns via JSON
- [ ] Metrics for trigger frequency
- [ ] A/B testing framework for patterns
- [ ] User-specific trigger customization

**Definition of Done:**
- All trigger types detected with > 90% accuracy
- Triggered memories process in < 50ms
- Configuration hot-reloadable
- Unit tests cover all trigger patterns
- Metrics dashboard shows trigger distribution
- Documentation includes pattern examples

---

## Story 8: Implement Frozen Memory Tier
**Assignee:** Storage Subagent  
**Story Points:** 13  
**Priority:** Medium  
**Dependencies:** Story 2

**Description:**
Add fourth memory tier with compression and intentional retrieval delay.

**Acceptance Criteria:**
- [ ] Add 'frozen' to MemoryTier enum
- [ ] Compress with zstd (level 3) before freezing
- [ ] Store in separate table or S3
- [ ] Implement 2-5 second unfreeze delay
- [ ] Batch freeze/unfreeze operations
- [ ] Migration rules: Cold→Frozen when P(recall) < 0.2
- [ ] Search excludes frozen unless explicit
- [ ] Compression ratio > 5:1 for text

**Definition of Done:**
- Frozen memories use 80% less storage
- Unfreeze delay consistently 2-5 seconds
- Data integrity verified post freeze/unfreeze
- Can freeze 100K memories in batch
- S3 integration tested if configured
- API includes freeze/unfreeze endpoints

---

## Story 9: Enhance Memory-Aware Retrieval
**Assignee:** Search Subagent  
**Story Points:** 8  
**Priority:** High  
**Dependencies:** Stories 1, 2, 8

**Description:**
Upgrade search to consider memory state and relationships.

**Acceptance Criteria:**
- [ ] Search excludes frozen by default
- [ ] Option to include frozen with warning
- [ ] Boost recently consolidated memories (2x)
- [ ] Include reflection/insights in results
- [ ] Return memory lineage/provenance
- [ ] Explain relevance scoring
- [ ] Support temporal search queries
- [ ] Cache frequent query patterns

**Definition of Done:**
- Search accuracy improves by 30%
- Frozen exclusion reduces latency by 50%
- Lineage tracking works 3 levels deep
- Explanation includes all score components
- p95 search latency < 200ms
- Cache hit rate > 60%

---

## Story 10: Production Performance Optimization
**Assignee:** Performance Subagent  
**Story Points:** 8  
**Priority:** High  
**Dependencies:** All other stories

**Description:**
Optimize system to meet production benchmarks.

**Acceptance Criteria:**
- [ ] p95 latency < 2 seconds for all operations
- [ ] 90% token reduction vs full context
- [ ] Maintain 20% memory headroom
- [ ] Batch operations with configurable size
- [ ] Connection pooling optimized
- [ ] Index optimization for new fields
- [ ] Query plan analysis and optimization
- [ ] Monitoring alerts for performance degradation

**Definition of Done:**
- Load test passes with 10K concurrent users
- Memory growth logarithmic, not linear
- All queries use indexes effectively
- Benchmarks documented and baselined
- Performance dashboard deployed
- Regression tests prevent degradation

---

## Story 11: Memory Harvester Configuration UI
**Assignee:** Frontend Subagent  
**Story Points:** 5  
**Priority:** Medium  
**Dependencies:** Story 4

**Description:**
Create user interface for configuring memory harvesting preferences.

**Acceptance Criteria:**
- [ ] Toggle for enabling/disabling harvesting
- [ ] Confidence threshold slider (0.5-0.9)
- [ ] Pattern selection checkboxes
- [ ] Harvest frequency configuration
- [ ] View recently harvested memories
- [ ] Export memory history
- [ ] Privacy mode toggle
- [ ] Statistics dashboard

**Definition of Done:**
- UI responsive on all devices
- Changes apply without restart
- User preferences persisted
- Export includes all metadata
- Privacy mode fully disables harvesting
- Help documentation included

---

## Story 12: Integration Testing Suite
**Assignee:** QA Subagent  
**Story Points:** 8  
**Priority:** High  
**Dependencies:** Stories 1-11

**Description:**
Comprehensive test suite for memory system integration.

**Acceptance Criteria:**
- [ ] End-to-end memory lifecycle tests
- [ ] Load tests with 1M+ memories
- [ ] Consolidation behavior validation
- [ ] Tier migration verification
- [ ] Deduplication accuracy tests
- [ ] Performance regression tests
- [ ] Chaos testing for resilience
- [ ] Multi-user isolation tests

**Definition of Done:**
- 90% code coverage achieved
- All edge cases documented and tested
- Performance benchmarks automated
- Tests run in CI/CD pipeline
- Failure scenarios properly handled
- Test data generators included

---

## Execution Waves:

**Wave 1 (Foundation):**
- Story 1: Three-Component Scoring
- Story 2: Consolidation Mechanics
- Story 3: Multi-Stage Assessment

**Wave 2 (Collection):**
- Story 4: Silent Harvester
- Story 5: Deduplication
- Story 7: Event Triggers

**Wave 3 (Intelligence):**
- Story 6: Reflection Generator
- Story 8: Frozen Tier
- Story 9: Enhanced Retrieval

**Wave 4 (Polish):**
- Story 10: Performance Optimization
- Story 11: Configuration UI
- Story 12: Integration Testing

Each story is designed to be worked on by a specialized subagent, with clear dependencies marked for coordination.