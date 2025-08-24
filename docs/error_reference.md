# Error Code Reference Guide

This comprehensive reference documents all error types in the CODEX memory system, their causes, resolution steps, and operational procedures for effective troubleshooting.

## Quick Navigation

- [Memory System Errors](#memory-system-errors)
- [Mathematical Engine Errors](#mathematical-engine-errors)
- [MCP Server Errors](#mcp-server-errors)
- [Security Errors](#security-errors)
- [Backup System Errors](#backup-system-errors)
- [Importance Assessment Errors](#importance-assessment-errors)
- [Harvester Errors](#harvester-errors)
- [Error Handling Patterns](#error-handling-patterns)
- [Troubleshooting Flowchart](#troubleshooting-flowchart)

## Memory System Errors

### Database Errors

#### `MemoryError::Database`
**Error Pattern**: `Database error: {sqlx_error}`

**Causes**:
- PostgreSQL connection failures
- SQL query syntax errors
- Database constraint violations
- Connection pool exhaustion
- Transaction deadlocks

**Resolution Steps**:
1. Check PostgreSQL server status: `systemctl status postgresql`
2. Verify connection parameters in environment variables
3. Check connection pool utilization: `SELECT count(*) FROM pg_stat_activity;`
4. Review PostgreSQL logs for specific error details
5. Restart database connections if pool exhausted

**Operational Procedures**:
- Monitor connection pool at 70% saturation threshold
- Set up alerts for database connection failures
- Implement automatic failover to read replicas for read operations
- Regular vacuum and analyze operations

**Common Scenarios**:
- High concurrent load causing connection exhaustion
- Long-running queries blocking other operations
- Network connectivity issues between application and database

---

#### `MemoryError::ConnectionPool`
**Error Pattern**: `Connection pool error: {message}`

**Causes**:
- Maximum connection limit reached (current: 100)
- Connection timeouts during heavy operations
- Database server overload
- Network latency issues

**Resolution Steps**:
1. Check current pool status in metrics dashboard
2. Kill long-running queries: `SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE state = 'active' AND query_start < now() - interval '5 minutes';`
3. Increase pool size if consistently hitting limits
4. Implement connection retry with exponential backoff
5. Scale database resources if overloaded

**Operational Procedures**:
- Alert at 70% pool utilization
- Implement circuit breakers for database operations
- Monitor P99 query latency (<100ms target)
- Regular connection pool health checks

---

### Storage and Migration Errors

#### `MemoryError::DuplicateContent`
**Error Pattern**: `Duplicate content found in tier {tier}`

**Causes**:
- Same content being inserted multiple times
- Race conditions in deduplication logic
- Hash collision (extremely rare)
- Concurrent memory creation without proper locking

**Resolution Steps**:
1. Query existing memory: `SELECT id, content_hash FROM memories WHERE content_hash = ?`
2. If legitimate duplicate, merge metadata instead of creating new entry
3. Check for race conditions in application logs
4. Implement optimistic locking for memory creation
5. Add unique constraints if missing

**Operational Procedures**:
- Monitor duplicate detection rate (should be <1%)
- Implement content-based deduplication at ingestion
- Regular integrity checks for duplicate detection

**Prevention**:
- Use `INSERT ... ON CONFLICT` patterns
- Implement distributed locking for critical sections
- Content hash verification before insertion

---

#### `MemoryError::StorageExhausted`
**Error Pattern**: `Storage exhausted in tier {tier}: limit {limit} reached`

**Causes**:
- Working memory at 7±2 capacity limit
- Warm storage exceeding configured limits
- Cold storage disk space exhaustion
- Frozen tier compression failures

**Resolution Steps**:
1. **Working Memory**: Trigger immediate consolidation to warm tier
2. **Warm Storage**: Migrate oldest memories to cold tier
3. **Cold Storage**: Enable compression or add storage capacity
4. **All Tiers**: Check migration thresholds and adjust if needed

**Operational Procedures**:
- Set up alerts at 90% capacity for each tier
- Implement automatic tier migration when approaching limits
- Monitor migration queue length and processing time
- Regular capacity planning based on ingestion rates

**Tier-Specific Actions**:
- **Working**: `SELECT migrate_to_warm(limit => 2)`
- **Warm**: `SELECT migrate_to_cold(age_threshold => '24 hours')`
- **Cold**: Check disk space and compression ratios

---

#### `MemoryError::NotFound`
**Error Pattern**: `Memory not found: {id}`

**Causes**:
- Memory deleted by cleanup processes
- Invalid memory ID provided by client
- Concurrent deletion during access
- Database replication lag

**Resolution Steps**:
1. Verify memory ID format (should be UUID)
2. Check if memory was recently deleted: `SELECT * FROM deleted_memories WHERE id = ?`
3. For API calls, return 404 with helpful error message
4. Check replication lag if using read replicas
5. Implement soft deletes if immediate deletion is problematic

**Operational Procedures**:
- Log all memory access patterns for debugging
- Implement memory access audit trail
- Monitor deletion rates and patterns

---

#### `MemoryError::InvalidTierTransition`
**Error Pattern**: `Invalid tier transition from {from} to {to}`

**Causes**:
- Attempting to move memory backwards (e.g., cold → working)
- Skipping intermediate tiers
- Concurrent tier migrations
- Invalid consolidation strength values

**Resolution Steps**:
1. Check current memory state: `SELECT tier, consolidation_strength, recall_probability FROM memories WHERE id = ?`
2. Verify tier transition rules in configuration
3. If legitimate, use administrative override with proper justification
4. Fix consolidation algorithm if producing invalid transitions

**Valid Transitions**:
- `working → warm` (consolidation_strength < 0.7)
- `warm → cold` (consolidation_strength < 0.5)
- `cold → frozen` (consolidation_strength < 0.2)
- `frozen → working` (explicit recall only)

**Operational Procedures**:
- Monitor tier transition patterns for anomalies
- Alert on high rates of invalid transitions
- Regular audit of tier migration logic

---

#### `MemoryError::MigrationFailed`
**Error Pattern**: `Migration failed: {reason}`

**Causes**:
- Disk space exhaustion during migration
- Compression failures for frozen tier
- Network issues during tier transfers
- Database transaction failures

**Resolution Steps**:
1. Check available disk space on target tier storage
2. Verify compression settings for frozen tier
3. Restart failed migration with proper error handling
4. Check database transaction logs for deadlocks
5. Implement retry logic with exponential backoff

**Operational Procedures**:
- Monitor migration success rates (target: >99%)
- Set up alerts for migration queue backlog
- Regular maintenance of storage systems
- Test migration procedures during low-traffic periods

---

### Data Integrity Errors

#### `MemoryError::IntegrityError`
**Error Pattern**: `Data integrity error: {message}`

**Causes**:
- Corrupted memory content during storage
- Vector embedding mismatches
- Metadata inconsistencies
- Database constraint violations

**Resolution Steps**:
1. Run integrity check: `SELECT verify_memory_integrity(?)`
2. Compare content hashes with stored values
3. Regenerate corrupted embeddings
4. Fix metadata inconsistencies
5. Restore from backup if corruption is extensive

**Operational Procedures**:
- Daily automated integrity checks
- Checksum verification for all stored content
- Regular backup validation
- Monitor for data corruption patterns

---

### Serialization and Compression Errors

#### `MemoryError::SerializationError` / `MemoryError::Serialization`
**Error Pattern**: `Serialization error: {message}`

**Causes**:
- Invalid JSON in memory content
- Schema version mismatches
- Large objects exceeding serialization limits
- Character encoding issues

**Resolution Steps**:
1. Validate JSON structure: `SELECT json_valid(content)`
2. Check schema version compatibility
3. Split large objects if exceeding limits
4. Convert character encoding to UTF-8
5. Update serialization format if needed

**Prevention**:
- Validate all input before serialization
- Implement schema versioning
- Set reasonable size limits for serializable objects

---

#### `MemoryError::CompressionError` / `MemoryError::DecompressionError`
**Error Pattern**: `Compression/Decompression error: {message}`

**Causes**:
- zstd compression library errors
- Corrupted compressed data
- Incompatible compression versions
- Memory pressure during compression

**Resolution Steps**:
1. Verify zstd library version compatibility
2. Test decompression with sample data
3. Check available memory during compression
4. Use fallback compression if primary fails
5. Restore from uncompressed backup

**Operational Procedures**:
- Monitor compression ratios (target: >10:1 for cold tier)
- Regular compression algorithm testing
- Backup both compressed and uncompressed versions

---

### Performance and Safety Errors

#### `MemoryError::OperationTimeout`
**Error Pattern**: `Operation timeout: {message}`

**Causes**:
- Long-running database queries
- Vector similarity searches on large datasets
- Network latency issues
- Resource contention

**Resolution Steps**:
1. Check current operation duration in metrics
2. Kill long-running queries if safe to do so
3. Optimize query with proper indexes
4. Increase timeout if operation is legitimate
5. Implement operation cancellation

**Timeout Targets**:
- Working memory access: <1ms P99
- Warm storage query: <100ms P99  
- Cold storage retrieval: <20s P99
- Vector similarity search: <50ms P99

---

#### `MemoryError::SafetyViolation`
**Error Pattern**: `Safety violation: {message}`

**Causes**:
- Attempting unsafe operations on active memories
- Concurrent modifications without proper locking
- Resource limits exceeded
- Invalid memory state transitions

**Resolution Steps**:
1. Identify the specific safety violation type
2. Check memory lock status and current operations
3. Wait for ongoing operations to complete
4. Use administrative override if absolutely necessary
5. Review safety constraints for legitimacy

**Safety Constraints**:
- No deletion of memories currently being accessed
- No tier transitions during active consolidation
- Maximum 7±2 items in working memory
- Proper locking for all state transitions

---

#### `MemoryError::ConcurrencyError`
**Error Pattern**: `Concurrency error: {message}`

**Causes**:
- Race conditions between concurrent operations
- Deadlock detection in database transactions
- Lock timeout exceeded
- Optimistic locking failures

**Resolution Steps**:
1. Retry operation with exponential backoff
2. Check for deadlock patterns in logs
3. Implement proper lock ordering to prevent deadlocks
4. Use shorter transactions where possible
5. Consider using optimistic concurrency control

**Prevention**:
- Consistent lock ordering across all operations
- Short-lived transactions
- Proper isolation levels
- Regular deadlock monitoring

---

## Mathematical Engine Errors

### Parameter and Calculation Errors

#### `MathEngineError::InvalidParameter`
**Error Pattern**: `Invalid parameter: {parameter} = {value}, expected {constraint}`

**Causes**:
- Negative values for parameters that must be positive
- Values outside expected ranges for mathematical functions
- NaN or infinite values from calculations
- Uninitialized parameters

**Resolution Steps**:
1. Check parameter validation logic for the specific parameter
2. Verify input data doesn't contain invalid values
3. Add bounds checking for all mathematical inputs
4. Use default values for invalid parameters if safe
5. Log parameter validation failures for pattern analysis

**Common Parameters**:
- `consolidation_strength`: Must be 0.0 ≤ value ≤ 1.0
- `recall_probability`: Must be 0.0 ≤ value ≤ 1.0
- `time_elapsed`: Must be ≥ 0.0
- `importance_score`: Must be 0.0 ≤ value ≤ 1.0

**Prevention**:
- Input validation at API boundaries
- Type safety for all mathematical parameters
- Property-based testing with random inputs

---

#### `MathEngineError::MathematicalOverflow`
**Error Pattern**: `Mathematical overflow in calculation: {operation}`

**Causes**:
- Exponential functions with large inputs
- Multiplication of very large numbers
- Division by very small numbers
- Accumulation of floating-point errors

**Resolution Steps**:
1. Implement overflow checking for the specific operation
2. Use logarithmic calculations where possible
3. Clamp values to reasonable ranges
4. Use higher precision arithmetic if needed
5. Break down complex calculations into smaller steps

**Prevention**:
- Bounds checking on all inputs
- Use `checked_*` arithmetic operations
- Implement saturation arithmetic for safety
- Regular testing with extreme values

---

#### `MathEngineError::AccuracyError`
**Error Pattern**: `Calculation accuracy exceeded tolerance: expected {expected}, got {actual}, tolerance {tolerance}`

**Causes**:
- Floating-point precision limitations
- Algorithmic instability
- Accumulated rounding errors
- Implementation bugs in mathematical formulas

**Resolution Steps**:
1. Review mathematical formula implementation
2. Compare with reference implementation
3. Increase precision if needed
4. Use numerically stable algorithms
5. Validate against published research benchmarks

**Accuracy Targets**:
- Ebbinghaus forgetting curve: ±0.001 tolerance
- Three-component scoring: ±0.005 tolerance
- Consolidation calculations: ±0.01 tolerance

---

#### `MathEngineError::PerformanceError`
**Error Pattern**: `Performance target exceeded: {duration_ms}ms > {target_ms}ms`

**Causes**:
- Inefficient algorithms
- Large dataset processing
- System resource constraints
- Lock contention

**Resolution Steps**:
1. Profile the specific calculation causing slowness
2. Optimize algorithms for better complexity
3. Implement caching for repeated calculations
4. Use parallel processing where appropriate
5. Consider approximation algorithms if exact results aren't critical

**Performance Targets**:
- Individual calculations: <10ms
- Batch processing: <5ms per item
- Vector operations: <1ms for single similarity calculation

---

#### `MathEngineError::BatchProcessingError`
**Error Pattern**: `Batch processing error: {message}`

**Causes**:
- Memory exhaustion during large batch processing
- Inconsistent batch size handling
- Failed individual calculations within batch
- Transaction size limits exceeded

**Resolution Steps**:
1. Reduce batch size to manageable chunks
2. Implement streaming processing for large datasets
3. Add progress tracking and resumption capability
4. Handle individual failures without failing entire batch
5. Monitor memory usage during batch operations

**Batch Optimization**:
- Optimal batch size: 100-1000 items
- Streaming processing for >10,000 items
- Progress checkpointing every 1,000 items
- Memory cleanup between batches

---

## MCP Server Errors

### Circuit Breaker Errors

#### `CircuitBreakerError::CircuitOpen`
**Error Pattern**: `Circuit breaker is open - service temporarily unavailable`

**Causes**:
- Downstream service failures exceed threshold
- Network connectivity issues
- Service overload conditions
- Cascading failures

**Resolution Steps**:
1. Check downstream service health
2. Review circuit breaker metrics and thresholds
3. Wait for automatic recovery (typically 30-60 seconds)
4. Manually reset circuit breaker if safe to do so
5. Implement fallback responses for critical operations

**Circuit Breaker Configuration**:
- Failure threshold: 10 failures in 60 seconds
- Recovery timeout: 30 seconds
- Half-open test calls: 3 successful calls needed

**Operational Procedures**:
- Monitor circuit breaker state transitions
- Alert on circuit open events
- Implement graceful degradation
- Regular health check validation

---

#### `CircuitBreakerError::HalfOpenLimitExceeded`
**Error Pattern**: `Circuit breaker half-open call limit exceeded`

**Causes**:
- Too many test calls during recovery period
- High concurrent load during recovery
- Aggressive retry patterns
- Misconfigured half-open limits

**Resolution Steps**:
1. Reduce concurrent test calls during recovery
2. Implement backoff strategies for clients
3. Adjust half-open call limits if too restrictive
4. Monitor recovery success patterns
5. Queue requests during half-open state

---

## Security Errors

### Authentication and Authorization

#### `SecurityError::AuthenticationFailed`
**Error Pattern**: `Authentication failed: {message}`

**Causes**:
- Invalid credentials (JWT, API key, certificates)
- Expired authentication tokens
- Certificate validation failures
- Network issues during authentication

**Resolution Steps**:
1. Verify credentials are current and valid
2. Check token expiration times
3. Validate certificate chain and expiration
4. Refresh tokens if expired
5. Check network connectivity to authentication services

**Authentication Types**:
- **JWT**: Check signature and expiration
- **API Key**: Verify against whitelist
- **Certificate**: Validate chain, expiration, revocation

**Security Procedures**:
- Log all authentication attempts (success/failure)
- Implement rate limiting on authentication endpoints
- Regular credential rotation
- Monitor for brute force attempts

---

#### `SecurityError::AuthorizationFailed`
**Error Pattern**: `Authorization failed: {message}`

**Causes**:
- Insufficient permissions for requested operation
- Role-based access control violations
- Resource ownership violations
- Scope limitations in tokens

**Resolution Steps**:
1. Check user permissions and roles
2. Verify resource ownership
3. Review token scopes and claims
4. Update permissions if legitimate access
5. Implement principle of least privilege

**Authorization Levels**:
- **Read**: Memory queries and basic operations
- **Write**: Memory creation and updates
- **Admin**: System configuration and user management
- **System**: Internal service-to-service calls

---

#### `SecurityError::RateLimitExceeded`
**Error Pattern**: `Rate limit exceeded`

**Causes**:
- Client exceeding configured request limits
- DDoS or abuse attempts
- Misconfigured rate limits
- Burst traffic patterns

**Resolution Steps**:
1. Check client request patterns
2. Verify rate limit configuration is appropriate
3. Implement backoff strategies for legitimate clients
4. Block malicious IP addresses
5. Scale infrastructure if legitimate traffic

**Rate Limits**:
- Anonymous users: 100 requests/hour
- Authenticated users: 1000 requests/hour
- System services: 10000 requests/hour
- Burst allowance: 10 requests/second for 10 seconds

---

### Data Protection and Compliance

#### `SecurityError::PiiDetected`
**Error Pattern**: `PII detected in content`

**Causes**:
- Personal information in memory content
- Email addresses, phone numbers, SSN detection
- GDPR compliance violations
- Insufficient input sanitization

**Resolution Steps**:
1. Remove or redact detected PII
2. Implement PII detection at ingestion
3. Add content scanning before storage
4. Update privacy policies if needed
5. Notify data protection officer if required

**PII Detection Patterns**:
- Email addresses: regex pattern matching
- Phone numbers: format validation
- SSN/National IDs: country-specific patterns
- Credit card numbers: Luhn algorithm validation

**Compliance Procedures**:
- Automatic PII redaction
- Audit logs for PII detection
- User consent management
- Right to be forgotten implementation

---

#### `SecurityError::GdprError`
**Error Pattern**: `GDPR compliance error: {message}`

**Causes**:
- Data processing without proper consent
- Right to be forgotten request failures
- Data export/portability issues
- Retention policy violations

**Resolution Steps**:
1. Review consent records for affected data
2. Process right to be forgotten requests immediately
3. Implement data portability export
4. Update retention policies
5. Notify regulatory authorities if required

**GDPR Compliance**:
- Data consent tracking
- Right to access implementation
- Right to rectification
- Right to erasure (be forgotten)
- Data portability export

---

## Backup System Errors

### Backup Operations

#### `BackupError::BackupFailed`
**Error Pattern**: `Backup failed: {message}`

**Causes**:
- Insufficient disk space for backup files
- Database connection failures during backup
- Backup file corruption
- Network issues to backup storage

**Resolution Steps**:
1. Check available disk space: `df -h`
2. Verify database connectivity
3. Test backup file integrity
4. Check network connectivity to backup storage
5. Retry backup with different storage location

**Backup Procedures**:
- Daily automated backups at 2 AM UTC
- Weekly full backups, daily incrementals
- Backup retention: 30 days daily, 12 weeks weekly
- Regular backup restoration testing

---

#### `BackupError::RecoveryFailed`
**Error Pattern**: `Recovery failed: {message}`

**Causes**:
- Corrupted backup files
- Incompatible backup versions
- Insufficient permissions for restoration
- Database schema mismatches

**Resolution Steps**:
1. Verify backup file integrity: `pg_verifybackup`
2. Check backup version compatibility
3. Ensure sufficient permissions for restoration
4. Test schema compatibility before full restoration
5. Use point-in-time recovery if available

**Recovery Procedures**:
- Test restoration monthly
- Document recovery procedures
- Maintain recovery time objective (RTO): <1 hour
- Recovery point objective (RPO): <5 minutes

---

#### `BackupError::EncryptionError`
**Error Pattern**: `Encryption error: {message}`

**Causes**:
- Invalid encryption keys
- Key rotation failures
- Encryption algorithm mismatches
- Hardware security module (HSM) failures

**Resolution Steps**:
1. Verify encryption key availability and validity
2. Test key rotation procedures
3. Check encryption algorithm compatibility
4. Validate HSM connectivity if used
5. Use backup encryption keys if primary fails

**Encryption Standards**:
- AES-256 encryption for backup files
- Key rotation every 90 days
- HSM storage for encryption keys
- End-to-end encryption for off-site backups

---

## Importance Assessment Errors

### Assessment Pipeline Errors

#### `ImportanceAssessmentError::Stage1Failed`
**Error Pattern**: `Stage 1 pattern matching failed: {message}`

**Causes**:
- Regular expression compilation failures
- Invalid pattern matching rules
- Performance timeouts on large texts
- Missing pattern configuration

**Resolution Steps**:
1. Validate pattern matching rules syntax
2. Test patterns with sample data
3. Optimize patterns for performance
4. Add timeout handling for large texts
5. Update pattern configuration if needed

**Stage 1 Patterns**:
- High importance keywords: urgent, critical, deadline
- Technical patterns: error, exception, failure
- Business patterns: revenue, customer, strategic
- Custom user-defined patterns

---

#### `ImportanceAssessmentError::Stage2Failed`
**Error Pattern**: `Stage 2 semantic analysis failed: {message}`

**Causes**:
- Vector embedding generation failures
- Semantic similarity calculation errors
- Insufficient training data for models
- Model compatibility issues

**Resolution Steps**:
1. Verify embedding model availability
2. Test semantic similarity calculations
3. Update model training data if needed
4. Check model version compatibility
5. Use fallback semantic analysis

**Semantic Analysis**:
- Vector embeddings: 1536-dimensional
- Similarity threshold: 0.7 for high importance
- Model: OpenAI text-embedding-ada-002
- Fallback: TF-IDF similarity if embedding fails

---

#### `ImportanceAssessmentError::Stage3Failed`
**Error Pattern**: `Stage 3 LLM scoring failed: {message}`

**Causes**:
- LLM API failures or rate limits
- Invalid prompts or context length exceeded
- Model hallucination or inconsistent scoring
- Network connectivity issues

**Resolution Steps**:
1. Check LLM API status and rate limits
2. Validate prompt structure and length
3. Test scoring consistency with sample data
4. Implement retry logic with backoff
5. Use cached scores for repeated content

**LLM Scoring**:
- Model: GPT-4 or Claude for importance scoring
- Score range: 0.0-1.0 with 0.1 granularity
- Timeout: 30 seconds per request
- Fallback: Rule-based scoring if LLM unavailable

---

#### `ImportanceAssessmentError::CircuitBreakerOpen`
**Error Pattern**: `Circuit breaker is open: {message}`

**Causes**:
- Repeated failures in importance assessment pipeline
- External service unavailability
- Performance degradation
- Resource exhaustion

**Resolution Steps**:
1. Check health of all pipeline stages
2. Verify external service availability
3. Wait for automatic circuit recovery
4. Use cached importance scores if available
5. Implement graceful degradation

---

## Harvester Errors

### Data Processing Errors

#### `HarvesterError::ExtractionFailed`
**Error Pattern**: `Pattern extraction failed: {message}`

**Causes**:
- Complex text patterns that exceed processing limits
- Malformed input data
- Regular expression timeouts
- Memory exhaustion during extraction

**Resolution Steps**:
1. Validate input data format and structure
2. Optimize extraction patterns for performance
3. Implement extraction timeouts and limits
4. Break down complex patterns into simpler ones
5. Monitor memory usage during extraction

**Extraction Patterns**:
- Text chunking: Maximum 10,000 characters per chunk
- Pattern timeout: 5 seconds per pattern
- Memory limit: 100MB per extraction operation
- Concurrent extractions: Maximum 10 parallel

---

#### `HarvesterError::DeduplicationFailed`
**Error Pattern**: `Deduplication failed: {message}`

**Causes**:
- Hash collision detection failures
- Content similarity calculation errors
- Large datasets causing memory issues
- Concurrent deduplication conflicts

**Resolution Steps**:
1. Verify content hash algorithms
2. Test similarity calculations with known data
3. Implement streaming deduplication for large datasets
4. Add proper locking for concurrent operations
5. Monitor deduplication success rates

**Deduplication Strategy**:
- Content hashing: SHA-256
- Similarity threshold: 95% for exact duplicates
- Near-duplicate detection: 85% similarity threshold
- Batch processing: 1,000 items per batch

---

#### `HarvesterError::BatchProcessingFailed`
**Error Pattern**: `Batch processing failed: {message}`

**Causes**:
- Memory exhaustion during large batch processing
- Database transaction size limits
- Individual item processing failures
- Network timeouts during batch operations

**Resolution Steps**:
1. Reduce batch size for memory-intensive operations
2. Implement transaction batching with proper limits
3. Handle individual failures without failing entire batch
4. Add progress tracking and resumption capability
5. Monitor resource usage during batch processing

**Batch Configuration**:
- Optimal batch size: 500 items for harvesting
- Transaction limit: 1,000 database operations
- Progress checkpointing: Every 100 items
- Memory monitoring: Alert at 80% usage

---

## Error Handling Patterns

### Standard Error Response Format

All errors follow a consistent JSON structure:

```json
{
    "error": {
        "code": "MEMORY_ERROR_DATABASE",
        "message": "Database error: connection pool exhausted",
        "details": {
            "category": "Database",
            "severity": "High", 
            "retry_after": 30,
            "context": {
                "pool_size": 100,
                "active_connections": 100,
                "operation": "memory_query"
            }
        },
        "timestamp": "2025-08-24T10:30:00Z",
        "trace_id": "req-12345-67890"
    }
}
```

### Error Severity Levels

1. **Critical**: System failure, immediate intervention required
2. **High**: Major functionality impaired, requires urgent attention  
3. **Medium**: Degraded performance or partial functionality loss
4. **Low**: Minor issues that don't significantly impact operations
5. **Info**: Operational information, no action required

### Retry Strategies

#### Exponential Backoff
```rust
async fn retry_with_backoff<T, E, F, Fut>(
    operation: F,
    max_attempts: usize,
) -> Result<T, E>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    let mut delay = Duration::from_millis(100);
    
    for attempt in 0..max_attempts {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) if attempt == max_attempts - 1 => return Err(e),
            Err(_) => {
                tokio::time::sleep(delay).await;
                delay = delay * 2; // Exponential backoff
            }
        }
    }
    
    unreachable!()
}
```

#### Circuit Breaker Pattern
```rust
pub struct ErrorHandlerConfig {
    pub failure_threshold: u32,    // 10 failures
    pub recovery_timeout: Duration, // 30 seconds
    pub half_open_max_calls: u32,  // 3 test calls
}
```

### Logging and Observability

#### Error Logging Format
```rust
error!(
    error_code = "MEMORY_ERROR_DATABASE",
    error_message = %error,
    trace_id = %trace_id,
    user_id = %user_id,
    operation = "memory_query",
    duration_ms = %duration.as_millis(),
    "Database operation failed"
);
```

#### Metrics Collection
- Error rate by category and severity
- Error response time distribution
- Retry attempt success rates
- Circuit breaker state changes
- Error correlation with system load

### Error Recovery Procedures

#### Database Errors
1. **Connection Issues**: Retry with backoff, switch to read replica
2. **Query Timeouts**: Kill query, optimize, increase timeout
3. **Deadlocks**: Retry with random delay, review lock ordering
4. **Constraint Violations**: Validate input, handle gracefully

#### Memory System Errors  
1. **Storage Exhaustion**: Trigger migration, increase capacity
2. **Migration Failures**: Retry with smaller batches, check disk space
3. **Corruption**: Restore from backup, regenerate affected data
4. **Performance Issues**: Scale resources, optimize algorithms

#### External Service Errors
1. **API Failures**: Circuit breaker activation, fallback responses
2. **Rate Limits**: Backoff and retry, request quota increase
3. **Timeouts**: Increase timeout, optimize requests
4. **Authentication**: Refresh tokens, check credentials

## Troubleshooting Flowchart

```
Error Occurred
├── Database Related?
│   ├── Connection Issues? → Check pool, restart connections
│   ├── Query Timeout? → Optimize query, check indexes
│   ├── Deadlock? → Retry with delay
│   └── Constraint Violation? → Validate input data
├── Memory System?
│   ├── Storage Full? → Trigger migration
│   ├── Not Found? → Check ID validity
│   ├── Tier Transition? → Verify consolidation strength
│   └── Migration Failed? → Check disk space, retry
├── Performance Related?
│   ├── Timeout? → Check system load, optimize
│   ├── High Latency? → Review query plans, add indexes
│   ├── Memory Usage? → Check for leaks, garbage collect
│   └── CPU Usage? → Profile code, optimize algorithms
├── Security Related?
│   ├── Authentication? → Verify credentials, refresh tokens
│   ├── Authorization? → Check permissions, update roles
│   ├── Rate Limiting? → Implement backoff, check quotas
│   └── PII Detected? → Redact data, update policies
└── External Services?
    ├── API Failure? → Check service health, use fallbacks
    ├── Network Issues? → Test connectivity, retry
    ├── Rate Limits? → Implement backoff, request increases
    └── Circuit Open? → Wait for recovery, check thresholds
```

## Operational Procedures

### Daily Operations Checklist
- [ ] Monitor error rates across all categories
- [ ] Check circuit breaker status and recovery
- [ ] Verify backup completion and integrity
- [ ] Review performance metrics and alerts
- [ ] Validate tier migration success rates
- [ ] Check database connection pool utilization
- [ ] Verify security audit logs for anomalies

### Weekly Operations Checklist
- [ ] Test backup restoration procedures
- [ ] Review error patterns and trends
- [ ] Update security configurations
- [ ] Performance optimization based on metrics
- [ ] Review and update error documentation
- [ ] Validate monitoring thresholds
- [ ] Check compliance with data retention policies

### Monthly Operations Checklist
- [ ] Comprehensive security audit
- [ ] Disaster recovery testing
- [ ] Performance baseline review
- [ ] Error handling procedure updates
- [ ] Team training on new error scenarios
- [ ] Review and update operational runbooks
- [ ] Capacity planning based on error trends

### Emergency Response Procedures

#### Critical System Failures
1. **Immediate**: Activate incident response team
2. **Assess**: Determine scope and impact
3. **Communicate**: Notify stakeholders and users
4. **Mitigate**: Implement temporary fixes
5. **Resolve**: Apply permanent solutions
6. **Review**: Post-incident analysis and improvements

#### Data Loss or Corruption
1. **Stop**: Halt all operations affecting corrupted data
2. **Assess**: Determine extent of corruption
3. **Isolate**: Prevent further corruption spread
4. **Restore**: From most recent clean backup
5. **Verify**: Data integrity after restoration
6. **Resume**: Operations with enhanced monitoring

#### Security Incidents
1. **Contain**: Isolate affected systems
2. **Investigate**: Determine attack vectors
3. **Notify**: Security team and authorities
4. **Remediate**: Fix vulnerabilities
5. **Monitor**: Enhanced security monitoring
6. **Report**: Incident documentation

## Monitoring and Alerting

### Critical Alerts (Immediate Response Required)
- Database connection failures >50% of pool
- Memory system corruption detected
- Security authentication bypass attempts
- System-wide error rate >10%
- Critical tier storage >95% full

### High Priority Alerts (Response Within 30 Minutes)
- Individual service error rate >5%
- Performance degradation >2x baseline
- Backup failures or verification errors
- Circuit breaker open for >10 minutes
- PII detection in stored content

### Medium Priority Alerts (Response Within 2 Hours)
- Unusual error patterns or spikes
- Performance below SLA thresholds
- Rate limiting activation
- Migration queue backlog
- Security audit anomalies

### Low Priority Alerts (Response Within 24 Hours)
- Minor configuration issues
- Non-critical service degradation
- Informational security events
- Performance optimization opportunities
- Documentation update needs

## Integration with Operational Tools

### Monitoring Dashboards
- Error rate trends by category and severity
- System health and performance metrics  
- Database and connection pool status
- Security events and authentication failures
- Backup and recovery status

### Incident Management
- Automated ticket creation for critical errors
- Error correlation and root cause analysis
- Escalation procedures based on severity
- Post-incident review and improvement tracking

### Configuration Management
- Error threshold configuration updates
- Security policy enforcement
- Backup retention and schedule management
- Performance baseline adjustments

This comprehensive error reference enables operations teams to quickly identify, diagnose, and resolve issues while maintaining system reliability and security. Regular updates ensure accuracy as the system evolves.