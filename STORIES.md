# Memory System Architecture Compliance - Jira Stories

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