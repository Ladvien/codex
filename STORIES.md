I'll convert these issues into detailed Jira stories organized by priority. Each ticket includes acceptance criteria, minimal code examples where necessary, and definition of done.

## P0 - Critical Issues (Production Blockers)


### TICKET-003: Enable MCP Authentication by Default
**Type:** Security  
**Priority:** Critical  
**Component:** MCP, Authentication  
**Story Points:** 5  

**Description:**
MCP server currently allows unauthenticated connections by default, creating unauthorized access risk.

**Acceptance Criteria:**
- [ ] Authentication required by default in MCP configuration
- [ ] Environment variable override available for development only
- [ ] Token-based authentication implemented
- [ ] Connection rejection for unauthenticated requests
- [ ] Graceful error messages for auth failures
- [ ] Rate limiting on authentication attempts

**Definition of Done:**
- Authentication enabled in default configuration
- Documentation updated with auth setup instructions
- Integration tests verify auth enforcement
- Penetration test confirms no bypass methods
- Monitoring alerts configured for auth failures

---

## P1 - High Priority Issues

### TICKET-004: Implement Working Memory Capacity Limits
**Type:** Feature  
**Priority:** High  
**Component:** Cognitive Systems  
**Story Points:** 8  

**Description:**
System violates Miller's 7±2 cognitive constraint, allowing unlimited working memory items causing cognitive overload.

**Acceptance Criteria:**
- [ ] Working memory limited to maximum 9 items
- [ ] LRU eviction policy when limit exceeded
- [ ] Configurable limit between 5-9 items
- [ ] Metrics tracking memory pressure
- [ ] Automatic tier migration when approaching limits
- [ ] API returns 507 when memory full

**Definition of Done:**
- Working memory enforcement active in all code paths
- Unit tests verify limit enforcement
- Performance tests confirm no memory exhaustion
- Cognitive science review completed
- Monitoring dashboard shows memory utilization

---

### TICKET-005: Fix N+1 Query Patterns in Hybrid Search
**Type:** Performance  
**Priority:** High  
**Component:** Database, Search  
**Story Points:** 5  

**Description:**
Hybrid search executes individual queries per result instead of batch fetching, causing severe performance degradation.

**Acceptance Criteria:**
- [ ] Single batch query for all search results
- [ ] JOIN operations replace iterative fetches
- [ ] Query plan optimized with EXPLAIN ANALYZE
- [ ] Response time <100ms for 100 results
- [ ] Database round trips reduced by 90%

**Definition of Done:**
- N+1 queries eliminated in all search paths
- Performance tests show 10x improvement
- Database monitoring confirms reduced load
- Query execution plans reviewed by DBA
- No functional regressions

---

### TICKET-006: Implement Forgetting Mechanisms
**Type:** Feature  
**Priority:** High  
**Component:** Memory Management  
**Story Points:** 13  

**Description:**
System lacks forgetting curves and memory decay, leading to unbounded memory growth and exhaustion.

**Acceptance Criteria:**
- [ ] Ebbinghaus forgetting curve implemented
- [ ] Configurable decay rates per memory tier
- [ ] Automatic cleanup of decayed memories
- [ ] Reinforcement learning updates decay rate
- [ ] Memory importance scoring affects decay
- [ ] Batch cleanup job runs hourly

**Definition of Done:**
- Forgetting mechanism active in production
- Memory growth rate stabilized
- Long-term storage costs projected and acceptable
- A/B test shows improved retrieval relevance
- Documentation includes decay configuration guide

---

## P2 - Medium Priority Issues

### TICKET-007: Fix Layer Boundary Violations
**Type:** Technical Debt  
**Priority:** Medium  
**Component:** Architecture  
**Story Points:** 8  

**Description:**
Direct database access from service layer violates clean architecture principles, reducing maintainability.

**Acceptance Criteria:**
- [ ] Repository pattern consistently implemented
- [ ] No direct SQL in service layer
- [ ] Domain models separate from database entities
- [ ] Dependency injection for all repositories
- [ ] Clear separation of concerns
- [ ] No circular dependencies

**Definition of Done:**
- Architecture linting rules pass
- Dependency graph shows clean layers
- Code review by architecture team
- Documentation updated with layer responsibilities
- No performance regression

---

### TICKET-008: Add Security Test Coverage
**Type:** Testing  
**Priority:** Medium  
**Component:** Security, Testing  
**Story Points:** 5  

**Description:**
Security-critical paths have zero test coverage, leaving vulnerabilities undetected.

**Acceptance Criteria:**
- [ ] Authentication bypass tests
- [ ] SQL injection prevention tests
- [ ] Input validation fuzzing
- [ ] Rate limiting verification
- [ ] Token expiration handling
- [ ] 80% coverage on security modules

**Definition of Done:**
- Security test suite integrated in CI/CD
- All tests passing in pipeline
- Coverage report shows >80% for security paths
- Penetration test validates test effectiveness
- Test documentation complete

---

## P3 - Low Priority Issues

### TICKET-009: Complete Documentation Gaps
**Type:** Documentation  
**Priority:** Low  
**Component:** Documentation  
**Story Points:** 3  

**Description:**
Critical operational procedures undocumented, causing deployment and maintenance issues.

**Acceptance Criteria:**
- [ ] Authentication setup guide complete
- [ ] Database migration runbook created
- [ ] Performance tuning guide written
- [ ] Troubleshooting procedures documented
- [ ] API documentation auto-generated
- [ ] Architecture decision records added

**Definition of Done:**
- Documentation reviewed by ops team
- All procedures tested by new team member
- Documentation site deployed and accessible
- Feedback incorporated from first production incident
- Automated checks for documentation updates

---

## Epic: Migration Scripts

### TICKET-010: Create Security Fix Migration Scripts
**Type:** Database Migration  
**Priority:** Critical  
**Component:** Database  
**Story Points:** 3  

**Description:**
Database requires security hardening through migration scripts.

**Acceptance Criteria:**
- [ ] Statement timeout configuration (30s)
- [ ] Idle transaction timeout (60s)
- [ ] Query complexity limits
- [ ] Connection pool constraints
- [ ] Rollback procedures documented
- [ ] Zero-downtime migration path

**Definition of Done:**
- Migration tested in staging environment
- Rollback successfully tested
- Performance baseline established
- DBA approval obtained
- Production deployment checklist complete

---

### TICKET-011: Performance Optimization Indexes
**Type:** Database Migration  
**Priority:** High  
**Component:** Database, Performance  
**Story Points:** 3  

**Description:**
Missing indexes cause full table scans and N+1 query patterns.

**Acceptance Criteria:**
- [ ] Partial index on memories.last_accessed
- [ ] Composite index for hybrid search
- [ ] CONCURRENTLY option for zero downtime
- [ ] Index size projections calculated
- [ ] Query plan improvements verified

**Definition of Done:**
- Indexes created without blocking production
- Query performance improved by >50%
- Index maintenance schedule established
- Monitoring alerts configured for index bloat
- Documentation updated with index strategy

Each ticket is now ready for sprint planning with clear scope, acceptance criteria, and definition of done. The team can prioritize P0 tickets for immediate remediation before production deployment.