# BACKLOG - Codex-Dreams Extension

## Date: 2025-08-25
## Team: Multi-Agent Architecture Review

---

## 🚨 EPIC: Fix Critical Insights Generation Architecture

### Overview
The insights generation system is currently failing to process memories despite having 145+ memories in the database. Root cause analysis by specialized agents identified fundamental architectural inconsistencies between search methods, cognitive model violations, and silent failure patterns.

---

## P0 - CRITICAL (Blocking Production)

### CDEX-001: Fix Search Method Consistency
**Type**: Bug / Architecture  
**Priority**: P0 - Critical  
**Story Points**: 8  

**As a** developer maintaining the insights system  
**I want** all search methods to return consistent column structures  
**So that** memory retrieval works reliably across different search types  

**Problem Statement**:
- `generate_insights` uses `SearchType::Temporal` returning different columns than semantic search
- Column mismatch causes `build_search_results()` to fail silently
- Recent fix (commit 813dcb2) addressed symptoms but not root cause

**Acceptance Criteria**:
- [ ] Create `SearchBackend` trait with `required_columns()` contract
- [ ] Implement validation in `build_search_results()` to catch column mismatches
- [ ] Add integration tests verifying temporal_search vs semantic search parity
- [ ] Ensure all search methods return identical `SearchResult` structure
- [ ] Fix `access_frequency_score` calculation in temporal search

**Technical Details**:
```rust
pub trait SearchBackend {
    async fn execute_search(&self, request: SearchRequest) -> Result<Vec<SearchResult>>;
    fn required_columns(&self) -> &[&str];
}
```

---

### CDEX-002: Replace Silent Failures with Proper Error Handling
**Type**: Bug  
**Priority**: P0 - Critical  
**Story Points**: 5  

**As a** user of the insights generation feature  
**I want** clear error messages when insights can't be generated  
**So that** I understand what went wrong and how to fix it  

**Problem Statement**:
- Missing insights processor returns success message instead of error
- Scripts report "✓ New insights generated!" while exporting null
- Users receive false confirmations of successful operations

**Acceptance Criteria**:
- [ ] Replace silent success in `handlers.rs` with proper error propagation
- [ ] Implement `InsightsCapability` enum (Available/Disabled/Failed)
- [ ] Add specific error messages for different failure scenarios
- [ ] Update MCP protocol responses to include error details
- [ ] Fix JQ parsing paths in shell scripts (`.result.content[0].text` incorrect)

**Code Location**: `/src/mcp_server/handlers.rs` lines 1112+

---

## P1 - HIGH PRIORITY (Architecture Debt)

### CDEX-003: Implement Cognitive Memory Selection
**Type**: Feature / Research  
**Priority**: P1 - High  
**Story Points**: 13  

**As a** cognitive memory system  
**I want** to select memories based on consolidation strength and accessibility  
**So that** insights generation respects cognitive science research  

**Problem Statement**:
- System ignores `consolidation_strength`, `recall_probability`, and `successful_retrievals`
- Violates Ebbinghaus forgetting curves and spaced repetition research
- Treats memory as simple database rather than cognitive model

**Acceptance Criteria**:
- [ ] Replace temporal-only search with multi-factor cognitive selection
- [ ] Implement `recall_probability` thresholds for candidate selection
- [ ] Add testing effect integration (prioritize successful retrievals)
- [ ] Update scheduler to use cognitive accessibility scoring
- [ ] Implement proper tier-based memory selection

**Research References**:
- Squire & Alvarez (1995) - Systems consolidation theory
- Roediger & Karpicke (2006) - Testing effect and retrieval practice
- Diekelmann & Born (2010) - Sleep-dependent memory consolidation

---

### CDEX-004: Centralize Database Connection Management
**Type**: Technical Debt  
**Priority**: P1 - High  
**Story Points**: 8  

**As a** system administrator  
**I want** centralized database connection management  
**So that** connections are efficiently used and monitored  

**Problem Statement**:
- Multiple independent connection pools without coordination
- No shared connection management between components
- Potential for connection exhaustion under load

**Acceptance Criteria**:
- [ ] Create `DatabaseManager` with shared connection pools
- [ ] Migrate `MemoryRepository` and `InsightStorage` to use DatabaseManager
- [ ] Implement connection health monitoring
- [ ] Add connection pool metrics and alerting
- [ ] Separate pools for different workload types

---

## P2 - MEDIUM PRIORITY (Technical Debt)

### CDEX-005: Implement Circuit Breaker at HTTP Layer
**Type**: Performance / Reliability  
**Priority**: P2 - Medium  
**Story Points**: 5  

**As a** system operator  
**I want** circuit breakers at the HTTP transport layer  
**So that** failures are detected quickly and service degradation is minimized  

**Problem Statement**:
- Circuit breaker in wrong layer (InsightsProcessor instead of HTTP client)
- Thresholds too high (20 failures, 15-minute timeout)
- Slow failure detection impacts user experience

**Acceptance Criteria**:
- [ ] Move circuit breaker from InsightsProcessor to OllamaClient
- [ ] Adjust thresholds for faster failure detection (5 failures, 60s timeout)
- [ ] Add circuit breaker metrics and health endpoints
- [ ] Implement graceful degradation patterns
- [ ] Add cognitive load-based circuit breaking

---

### CDEX-006: Refactor Feature Flag Architecture  
**Type**: Technical Debt  
**Priority**: P2 - Medium  
**Story Points**: 8  

**As a** developer working on the codebase  
**I want** simplified feature flag management  
**So that** code is easier to maintain and test  

**Problem Statement**:
- `#[cfg(feature = "codex-dreams")]` repeated 40+ times
- Dual compilation paths create testing blind spots
- Maintenance burden and cognitive load high

**Acceptance Criteria**:
- [ ] Replace repetitive #[cfg] with capability pattern
- [ ] Create `FeatureManager` for centralized feature control
- [ ] Add tests for all feature flag combinations
- [ ] Document feature dependency requirements
- [ ] Implement feature discovery endpoints

---

## 📋 TECHNICAL TASKS

### Testing Infrastructure
- [ ] Add end-to-end insights generation integration tests
- [ ] Implement search method parity validation tests
- [ ] Create circuit breaker behavior tests
- [ ] Add feature flag combination testing
- [ ] Implement chaos testing for external services

### Observability
- [ ] Add structured logging for insights pipeline
- [ ] Implement metrics for search performance and accuracy
- [ ] Add distributed tracing for debugging
- [ ] Create dashboards for insights generation success rates
- [ ] Implement request correlation IDs

### Documentation
- [ ] Write Architecture Decision Records for search strategy
- [ ] Document cognitive memory model implementation
- [ ] Create troubleshooting guide for insights failures
- [ ] Document feature flag requirements
- [ ] Update API documentation with error scenarios

---

## 🔍 ROOT CAUSE SUMMARY

### Primary Issues
1. **Search Method Divergence**: Different search types return different column structures
2. **Silent Failures**: Success messages returned despite actual failures
3. **Cognitive Model Violations**: System ignores memory research principles

### Architecture Anti-Patterns Identified
1. Optional dependencies everywhere (`Option<Arc<Component>>`)
2. Search type fragmentation without contracts
3. Inconsistent error handling patterns
4. Database-first rather than cognitively-plausible design

### Testing Gaps
1. No end-to-end integration tests for insights
2. Manual fix verification instead of automated regression tests
3. Feature flag combinations not systematically tested
4. Search method parity not validated

---

## 📊 IMPACT ASSESSMENT

### Business Impact
- Insights generation completely broken for temporal queries
- User experience degraded by false success messages
- Cognitive research value not realized

### Technical Debt
- Architecture fragility requiring manual fixes
- Low testing confidence in critical paths
- High maintenance burden from feature flags

---

## 🚀 IMPLEMENTATION PHASES

### Phase 1: Stabilization (2-3 days)
- Fix search method consistency (CDEX-001)
- Replace silent failures (CDEX-002)
- Add critical integration tests

### Phase 2: Architecture Cleanup (1 week)
- Implement cognitive memory selection (CDEX-003)
- Centralize database connections (CDEX-004)
- Add observability infrastructure

### Phase 3: Optimization (1-2 weeks)
- Move circuit breaker to HTTP layer (CDEX-005)
- Refactor feature flags (CDEX-006)
- Complete documentation updates

---

## 📝 NOTES FROM REVIEW TEAM

**@rust-engineer**: "Core issue is architectural - different search methods should produce consistent results, but they're implemented with different underlying queries and error handling strategies."

**@cognitive-memory-expert**: "The fundamental issue is that insights generation treats memory as a database rather than implementing cognitively-plausible retrieval patterns."

**@postgres-expert**: "Database layer is architecturally sound but suffers from search method fragmentation where different query paths assume different column availability."

**@integration-testing-expert**: "System layers don't properly validate data flowing between them, leading to silent failures that appear successful at the protocol level but fail at the application logic level."

---

*Generated by Multi-Agent Architecture Review Team on 2025-08-25*