# Performance Tuning Guide - Agentic Memory System

## Overview

This guide provides comprehensive strategies for optimizing the performance of the Agentic Memory System. It covers database optimization, application tuning, infrastructure configuration, and monitoring approaches to achieve optimal performance across all system components.

## Table of Contents

1. [Performance Baselines and Targets](#performance-baselines-and-targets)
2. [Database Performance Tuning](#database-performance-tuning)
3. [Application Performance Optimization](#application-performance-optimization)
4. [Memory Tier Optimization](#memory-tier-optimization)
5. [Search Performance Tuning](#search-performance-tuning)
6. [Infrastructure and System Tuning](#infrastructure-and-system-tuning)
7. [Connection Pool Optimization](#connection-pool-optimization)
8. [Monitoring and Profiling](#monitoring-and-profiling)
9. [Load Testing and Benchmarking](#load-testing-and-benchmarking)
10. [Troubleshooting Performance Issues](#troubleshooting-performance-issues)

## Performance Baselines and Targets

### Target Performance Metrics

Based on SOTA research and production requirements, maintain these performance targets:

| Metric | Target | Critical Threshold |
|--------|--------|--------------------|
| Working Memory Access | <1ms P99 | <2ms P99 |
| Warm Storage Query | <100ms P99 | <500ms P99 |
| Cold Storage Retrieval | <20s P99 | <60s P99 |
| Memory Creation Rate | >1000/sec sustained | >500/sec |
| Search Operations | >500/sec complex queries | >200/sec |
| Concurrent Users | >100 simultaneous | >50 simultaneous |
| Error Rate | <1% | <5% |
| Database Connection Pool | <70% utilization | <90% utilization |
| Memory Compression Ratio | >10:1 for cold tier | >5:1 |
| Cache Hit Ratio | >90% for repeated queries | >70% |

### Performance Testing Methodology

```bash
# 1. Baseline measurement
./scripts/performance-baseline.sh

# 2. Load testing
./scripts/load-test.sh --users 100 --duration 300s

# 3. Stress testing  
./scripts/stress-test.sh --memory-pressure --cpu-pressure

# 4. Performance regression detection
./scripts/performance-regression-test.sh --compare-against baseline
```

## Database Performance Tuning

### PostgreSQL Configuration Optimization

#### 1. Memory Settings

```postgresql
-- postgresql.conf optimizations for 16GB RAM system
-- Adjust proportionally for your system

-- Shared memory
shared_buffers = '4GB'                    -- 25% of RAM
effective_cache_size = '12GB'             -- 75% of RAM  
work_mem = '64MB'                         -- Per connection, adjust based on max_connections
maintenance_work_mem = '1GB'              -- For maintenance operations

-- WAL settings
wal_buffers = '16MB'                      -- -1 for auto-tuning is usually fine
checkpoint_completion_target = 0.9        -- Spread checkpoints over 90% of interval
max_wal_size = '2GB'                      -- Larger for heavy write workloads
min_wal_size = '80MB'

-- Connection settings  
max_connections = 200                     -- Adjust based on expected load
shared_preload_libraries = 'pg_stat_statements'  -- Enable query statistics
```

#### 2. Query Performance Settings

```postgresql
-- Enable query optimization
enable_hashjoin = on
enable_mergejoin = on
enable_nestloop = on

-- Cost-based optimizer settings for SSD storage
random_page_cost = 1.1                    -- Default 4.0 for HDD, 1.1 for SSD
seq_page_cost = 1.0                       -- Keep default
cpu_tuple_cost = 0.01                     -- Default is fine
cpu_operator_cost = 0.0025                -- Default is fine

-- Parallel query settings
max_parallel_workers_per_gather = 4       -- Adjust based on CPU cores
max_parallel_workers = 8                  -- Total parallel workers
parallel_tuple_cost = 0.1                 -- Default is fine
```

#### 3. Autovacuum Tuning for High-Write Workloads

```postgresql
-- Aggressive autovacuum for memory system's write-heavy workload
autovacuum_max_workers = 6                -- More workers for faster cleanup
autovacuum_naptime = 10s                  -- Check more frequently
autovacuum_vacuum_threshold = 50          -- Start vacuum sooner
autovacuum_analyze_threshold = 50         -- Analyze more frequently
autovacuum_vacuum_scale_factor = 0.1      -- 10% of table size
autovacuum_analyze_scale_factor = 0.05    -- 5% of table size

-- Table-specific vacuum settings for high-churn tables
ALTER TABLE memories SET (
  autovacuum_vacuum_scale_factor = 0.05,
  autovacuum_analyze_scale_factor = 0.02,
  autovacuum_vacuum_threshold = 100
);
```

### Index Optimization

#### 1. Tier-Specific Index Strategy

```sql
-- Working Memory Indexes (optimized for speed)
CREATE INDEX CONCURRENTLY idx_memories_working_embedding 
ON memories USING hnsw (embedding vector_cosine_ops) 
WHERE tier = 'working' AND status = 'active';

CREATE INDEX CONCURRENTLY idx_memories_working_importance 
ON memories (importance_score DESC, last_accessed_at DESC NULLS LAST) 
WHERE tier = 'working' AND status = 'active';

CREATE INDEX CONCURRENTLY idx_memories_working_access 
ON memories (access_count DESC, updated_at DESC) 
WHERE tier = 'working' AND status = 'active';

-- Warm Memory Indexes (balanced performance/storage)
CREATE INDEX CONCURRENTLY idx_memories_warm_embedding 
ON memories USING hnsw (embedding vector_cosine_ops) 
WHERE tier = 'warm' AND status = 'active';

CREATE INDEX CONCURRENTLY idx_memories_warm_temporal 
ON memories (created_at DESC, updated_at DESC) 
WHERE tier = 'warm' AND status = 'active';

-- Cold Memory Indexes (optimized for storage)
CREATE INDEX CONCURRENTLY idx_memories_cold_hash 
ON memories (content_hash) 
WHERE tier = 'cold';

CREATE INDEX CONCURRENTLY idx_memories_cold_metadata 
ON memories USING gin (metadata) 
WHERE tier = 'cold' AND status = 'active';
```

#### 2. Search-Optimized Indexes

```sql
-- Composite indexes for common search patterns
CREATE INDEX CONCURRENTLY idx_memories_search_composite
ON memories (tier, importance_score DESC, last_accessed_at DESC, status)
WHERE status = 'active';

-- Metadata search optimization
CREATE INDEX CONCURRENTLY idx_memories_metadata_gin
ON memories USING gin (metadata jsonb_path_ops);

-- Content hash for deduplication
CREATE UNIQUE INDEX CONCURRENTLY idx_memories_content_hash_tier
ON memories (content_hash, tier)
WHERE status = 'active';

-- Temporal queries
CREATE INDEX CONCURRENTLY idx_memories_temporal_range
ON memories (created_at, updated_at)
WHERE status = 'active';
```

#### 3. Index Maintenance

```sql
-- Monitor index usage
SELECT 
    schemaname, tablename, indexname,
    idx_scan, idx_tup_read, idx_tup_fetch,
    pg_size_pretty(pg_relation_size(indexname::regclass)) as size
FROM pg_stat_user_indexes 
WHERE schemaname = 'public'
ORDER BY idx_scan DESC;

-- Identify unused indexes
SELECT 
    schemaname, tablename, indexname,
    pg_size_pretty(pg_relation_size(indexname::regclass)) as size
FROM pg_stat_user_indexes 
WHERE idx_scan = 0 
  AND pg_relation_size(indexname::regclass) > 100000000  -- > 100MB
ORDER BY pg_relation_size(indexname::regclass) DESC;

-- Rebuild fragmented indexes (during maintenance window)
REINDEX INDEX CONCURRENTLY idx_memories_working_embedding;
REINDEX INDEX CONCURRENTLY idx_memories_warm_embedding;
```

### Query Optimization

#### 1. Prepared Statements

```rust
// Use prepared statements for frequently executed queries
pub struct MemoryRepository {
    pool: PgPool,
    // Cache prepared statements
    get_memory_stmt: OnceCell<String>,
    search_memories_stmt: OnceCell<String>,
}

impl MemoryRepository {
    pub async fn get_memory_optimized(&self, id: Uuid) -> Result<Memory> {
        // Use prepared statement with plan caching
        let query = "
            SELECT id, content, embedding, tier, importance_score, 
                   access_count, last_accessed_at, metadata, 
                   created_at, updated_at
            FROM memories 
            WHERE id = $1 AND status = 'active'
        ";
        
        sqlx::query_as::<_, Memory>(query)
            .bind(id)
            .fetch_one(&self.pool)
            .await
    }
}
```

#### 2. Query Plan Analysis

```sql
-- Analyze query performance
EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) 
SELECT * FROM memories 
WHERE tier = 'working' 
  AND importance_score > 0.8 
  AND last_accessed_at > now() - interval '1 day'
ORDER BY importance_score DESC 
LIMIT 10;

-- Look for sequential scans and high buffer usage
-- Sequential Scan indicates missing indexes
-- High buffer reads indicate inefficient queries
```

#### 3. Optimized Search Queries

```sql
-- Efficient vector search with pre-filtering
WITH filtered_memories AS (
  SELECT id, embedding, importance_score
  FROM memories 
  WHERE tier = 'working' 
    AND status = 'active'
    AND importance_score > 0.7
  ORDER BY importance_score DESC
  LIMIT 1000  -- Pre-filter to reduce vector search space
)
SELECT m.*, 1 - (m.embedding <=> $1) as similarity
FROM filtered_memories m
WHERE 1 - (m.embedding <=> $1) > 0.8  -- Similarity threshold
ORDER BY m.embedding <=> $1
LIMIT 20;

-- Use covering indexes to avoid table lookups
CREATE INDEX CONCURRENTLY idx_memories_covering_search
ON memories (tier, status, importance_score) 
INCLUDE (id, embedding, content, metadata)
WHERE status = 'active';
```

## Application Performance Optimization

### Rust Application Tuning

#### 1. Memory Management Optimization

```rust
// Use object pools for frequent allocations
use object_pool::{Pool, Reusable};

pub struct MemoryRepository {
    pool: PgPool,
    embedding_pool: Pool<Vec<f32>>,
    query_pool: Pool<String>,
}

impl MemoryRepository {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            // Pre-allocate embedding vectors
            embedding_pool: Pool::new(32, || Vec::with_capacity(1536)),
            // Pre-allocate query strings
            query_pool: Pool::new(16, || String::with_capacity(1024)),
        }
    }
    
    pub async fn search_with_pooling(&self, query: &str) -> Result<Vec<SearchResult>> {
        // Reuse allocated vectors
        let mut embedding_vec: Reusable<Vec<f32>> = self.embedding_pool.try_pull()
            .unwrap_or_else(|| self.embedding_pool.pull());
        
        // Generate embedding efficiently
        self.embedder.generate_into(&mut embedding_vec, query).await?;
        
        // Vector is automatically returned to pool when dropped
        self.vector_search(&embedding_vec).await
    }
}
```

#### 2. Async Performance Optimization

```rust
// Use tokio configuration for high throughput
#[tokio::main(flavor = "multi_thread", worker_threads = 8)]
async fn main() -> Result<()> {
    // Configure tokio runtime for throughput
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .thread_stack_size(3 * 1024 * 1024)  // 3MB stack
        .enable_all()
        .build()?;
    
    // Use semaphores for backpressure control
    let semaphore = Arc::new(Semaphore::new(100));
    
    // Process requests with controlled concurrency
    let permit = semaphore.clone().acquire_owned().await?;
    tokio::spawn(async move {
        let _permit = permit;  // Hold permit until task completes
        process_request().await
    });
    
    Ok(())
}

// Batch operations for efficiency
pub async fn batch_create_memories(
    &self, 
    requests: Vec<CreateMemoryRequest>
) -> Result<Vec<Memory>> {
    // Process in batches to avoid overwhelming the database
    const BATCH_SIZE: usize = 100;
    
    let mut results = Vec::with_capacity(requests.len());
    
    for batch in requests.chunks(BATCH_SIZE) {
        let batch_results = self.create_memories_batch(batch).await?;
        results.extend(batch_results);
        
        // Small delay between batches to prevent resource exhaustion
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    
    Ok(results)
}
```

#### 3. Caching Strategies

```rust
use moka::future::Cache;
use std::time::Duration;

pub struct CachedMemoryRepository {
    inner: MemoryRepository,
    memory_cache: Cache<Uuid, Memory>,
    search_cache: Cache<String, Vec<SearchResult>>,
    embedding_cache: Cache<String, Vec<f32>>,
}

impl CachedMemoryRepository {
    pub fn new(repository: MemoryRepository) -> Self {
        Self {
            inner: repository,
            // Configure cache with TTL and size limits
            memory_cache: Cache::builder()
                .max_capacity(10_000)
                .time_to_live(Duration::from_secs(300))  // 5 minutes
                .build(),
            search_cache: Cache::builder()
                .max_capacity(1_000)
                .time_to_live(Duration::from_secs(60))   // 1 minute
                .build(),
            embedding_cache: Cache::builder()
                .max_capacity(5_000)
                .time_to_live(Duration::from_secs(3600)) // 1 hour
                .build(),
        }
    }
    
    pub async fn get_memory(&self, id: Uuid) -> Result<Memory> {
        if let Some(memory) = self.memory_cache.get(&id).await {
            return Ok(memory);
        }
        
        let memory = self.inner.get_memory(id).await?;
        self.memory_cache.insert(id, memory.clone()).await;
        Ok(memory)
    }
}
```

## Memory Tier Optimization

### Intelligent Tier Management

#### 1. Access Pattern Analysis

```sql
-- Analyze access patterns for tier optimization
WITH access_stats AS (
  SELECT 
    id, tier, access_count, last_accessed_at,
    importance_score, length(content) as content_size,
    EXTRACT(days FROM now() - last_accessed_at) as days_since_access,
    CASE 
      WHEN access_count = 0 THEN 0
      ELSE access_count::float / EXTRACT(days FROM now() - created_at) 
    END as access_frequency
  FROM memories 
  WHERE status = 'active'
)
SELECT 
  tier,
  COUNT(*) as memory_count,
  AVG(access_frequency) as avg_access_freq,
  AVG(importance_score) as avg_importance,
  SUM(content_size) as total_size,
  COUNT(CASE WHEN days_since_access > 30 THEN 1 END) as stale_memories
FROM access_stats 
GROUP BY tier;
```

#### 2. Automated Tier Migration

```rust
pub struct TierManager {
    repository: Arc<MemoryRepository>,
    config: TierConfig,
}

#[derive(Debug, Clone)]
pub struct TierConfig {
    pub working_to_warm_threshold: Duration,      // 7 days
    pub warm_to_cold_threshold: Duration,         // 30 days
    pub min_access_count_for_working: i32,        // 5 accesses
    pub importance_boost_threshold: f32,          // 0.8
    pub migration_batch_size: usize,              // 1000
}

impl TierManager {
    pub async fn run_migration_cycle(&self) -> Result<MigrationStats> {
        let mut stats = MigrationStats::default();
        
        // 1. Promote high-importance memories
        stats.promoted += self.promote_important_memories().await?;
        
        // 2. Demote stale working memories
        stats.demoted += self.demote_stale_working_memories().await?;
        
        // 3. Archive old warm memories
        stats.archived += self.archive_old_warm_memories().await?;
        
        // 4. Update access statistics
        self.update_tier_statistics().await?;
        
        Ok(stats)
    }
    
    async fn promote_important_memories(&self) -> Result<usize> {
        let query = "
            UPDATE memories 
            SET tier = 'working', updated_at = now()
            WHERE tier != 'working' 
              AND (
                importance_score > $1 
                OR (access_count > $2 AND last_accessed_at > now() - interval '7 days')
              )
              AND status = 'active'
            RETURNING id
        ";
        
        let results = sqlx::query(query)
            .bind(self.config.importance_boost_threshold)
            .bind(self.config.min_access_count_for_working)
            .fetch_all(&self.repository.pool)
            .await?;
            
        Ok(results.len())
    }
}
```

#### 3. Tier-Specific Performance Optimizations

```rust
// Different strategies per tier
impl MemoryRepository {
    pub async fn get_memory_by_tier(&self, id: Uuid) -> Result<Memory> {
        // First check which tier (fast lookup)
        let tier_query = "SELECT tier FROM memories WHERE id = $1 AND status = 'active'";
        let tier: MemoryTier = sqlx::query_scalar(tier_query)
            .bind(id)
            .fetch_one(&self.pool)
            .await?;
            
        match tier {
            MemoryTier::Working => {
                // Fast path for working memory
                self.get_working_memory(id).await
            },
            MemoryTier::Warm => {
                // Cached path for warm memory  
                self.get_warm_memory_cached(id).await
            },
            MemoryTier::Cold => {
                // Background fetch for cold memory
                self.get_cold_memory_async(id).await
            }
        }
    }
    
    async fn get_working_memory(&self, id: Uuid) -> Result<Memory> {
        // Optimized query for working memory
        let query = "
            SELECT * FROM memories 
            WHERE id = $1 AND tier = 'working' AND status = 'active'
        ";
        // Use dedicated working memory index
        sqlx::query_as::<_, Memory>(query)
            .bind(id)
            .fetch_one(&self.pool)
            .await
    }
}
```

## Search Performance Tuning

### Vector Search Optimization

#### 1. HNSW Index Tuning

```sql
-- Optimize HNSW parameters for different workloads
-- For high recall accuracy (slower but more accurate)
CREATE INDEX idx_memories_embedding_accurate
ON memories USING hnsw (embedding vector_cosine_ops)
WITH (m = 32, ef_construction = 400)
WHERE tier IN ('working', 'warm') AND status = 'active';

-- For high throughput (faster but less accurate)
CREATE INDEX idx_memories_embedding_fast  
ON memories USING hnsw (embedding vector_cosine_ops)
WITH (m = 16, ef_construction = 200)
WHERE tier = 'working' AND status = 'active';

-- Runtime search parameters
SET hnsw.ef_search = 100;  -- Higher for better recall, lower for speed
```

#### 2. Hybrid Search Optimization

```rust
pub struct OptimizedSearchEngine {
    repository: Arc<MemoryRepository>,
    text_index: tantivy::Index,
    embedding_cache: Cache<String, Vec<f32>>,
}

impl OptimizedSearchEngine {
    pub async fn hybrid_search(
        &self, 
        request: SearchRequest
    ) -> Result<Vec<SearchResult>> {
        // Parallel execution of text and vector search
        let (text_results, vector_results) = tokio::join!(
            self.text_search(&request),
            self.vector_search(&request)
        );
        
        // Merge results with configurable weights
        let merged = self.merge_search_results(
            text_results?,
            vector_results?,
            &request.hybrid_weights.unwrap_or_default()
        );
        
        Ok(merged)
    }
    
    async fn vector_search(&self, request: &SearchRequest) -> Result<Vec<SearchResult>> {
        // Generate embedding with caching
        let embedding = if let Some(cached) = self.embedding_cache.get(&request.query_text).await {
            cached
        } else {
            let embedding = self.generate_embedding(&request.query_text).await?;
            self.embedding_cache.insert(request.query_text.clone(), embedding.clone()).await;
            embedding
        };
        
        // Tier-specific search for performance
        let tier_filter = request.tier.unwrap_or(MemoryTier::Working);
        let query = format!(
            "SELECT *, 1 - (embedding <=> $1) as similarity 
             FROM memories 
             WHERE tier = '{}' AND status = 'active'
               AND 1 - (embedding <=> $1) > $2
             ORDER BY embedding <=> $1 
             LIMIT $3",
            tier_filter
        );
        
        sqlx::query_as::<_, SearchResult>(&query)
            .bind(&embedding)
            .bind(request.similarity_threshold.unwrap_or(0.7))
            .bind(request.limit.unwrap_or(10))
            .fetch_all(&self.repository.pool)
            .await
    }
}
```

#### 3. Search Result Caching

```rust
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

impl OptimizedSearchEngine {
    pub async fn cached_search(&self, request: SearchRequest) -> Result<Vec<SearchResult>> {
        // Create cache key from search parameters
        let cache_key = self.create_search_cache_key(&request);
        
        if let Some(cached_results) = self.search_cache.get(&cache_key).await {
            return Ok(cached_results);
        }
        
        // Perform search
        let results = self.hybrid_search(request).await?;
        
        // Cache results with TTL based on tier
        let ttl = match request.tier {
            Some(MemoryTier::Working) => Duration::from_secs(60),   // 1 minute
            Some(MemoryTier::Warm) => Duration::from_secs(300),     // 5 minutes  
            Some(MemoryTier::Cold) => Duration::from_secs(1800),    // 30 minutes
            None => Duration::from_secs(180),                       // 3 minutes
        };
        
        self.search_cache.insert_with_ttl(cache_key, results.clone(), ttl).await;
        Ok(results)
    }
    
    fn create_search_cache_key(&self, request: &SearchRequest) -> String {
        let mut hasher = DefaultHasher::new();
        request.query_text.hash(&mut hasher);
        request.tier.hash(&mut hasher);
        request.limit.hash(&mut hasher);
        request.similarity_threshold.hash(&mut hasher);
        
        format!("search_{:x}", hasher.finish())
    }
}
```

## Infrastructure and System Tuning

### Operating System Optimization

#### 1. Kernel Parameters

```bash
# /etc/sysctl.conf optimizations for memory system

# Network performance
net.core.rmem_max = 134217728              # 128MB receive buffer
net.core.wmem_max = 134217728              # 128MB send buffer  
net.core.netdev_max_backlog = 5000
net.core.somaxconn = 1024

# Memory management
vm.swappiness = 1                          # Minimize swapping
vm.dirty_ratio = 15                        # Dirty page cache limit
vm.dirty_background_ratio = 5              # Background writeback threshold
vm.vfs_cache_pressure = 50                 # Retain inode/dentry cache

# File system performance
fs.file-max = 1048576                      # Max open files system-wide

# Apply changes
sysctl -p
```

#### 2. I/O Scheduler Optimization

```bash
# Set I/O scheduler for SSD (deadline or noop)
echo deadline > /sys/block/sda/queue/scheduler

# Or for NVMe drives
echo none > /sys/block/nvme0n1/queue/scheduler

# Make permanent in /etc/default/grub
GRUB_CMDLINE_LINUX="elevator=deadline"
update-grub
```

#### 3. CPU Performance Settings

```bash
# Set CPU governor to performance mode
echo performance > /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor

# Or use cpupower utility
cpupower frequency-set -g performance

# Check CPU frequency scaling
cpupower frequency-info
```

### File System Optimization

#### 1. Mount Options for Performance

```bash
# /etc/fstab optimizations for database storage
# For ext4 filesystem
/dev/sdb1 /var/lib/postgresql ext4 noatime,nobarrier,data=writeback 0 2

# For XFS filesystem (recommended for large databases)
/dev/sdb1 /var/lib/postgresql xfs noatime,nobarrier,logbufs=8,logbsize=256k 0 2

# Remount with new options
mount -o remount,noatime,nobarrier /var/lib/postgresql
```

#### 2. SSD-Specific Optimizations

```bash
# Enable TRIM support
echo deadline > /sys/block/sda/queue/scheduler
echo 0 > /sys/block/sda/queue/rotational
echo 1 > /sys/block/sda/queue/iosched/fifo_batch

# For systemd systems, enable fstrim.timer
systemctl enable fstrim.timer
systemctl start fstrim.timer
```

## Connection Pool Optimization

### Database Connection Tuning

#### 1. Connection Pool Configuration

```toml
# /etc/memory-system/config.toml
[database]
max_connections = 50                       # Adjust based on load
min_connections = 10                       # Keep some connections warm
idle_timeout_seconds = 300                 # Close idle connections after 5 min
connection_timeout_seconds = 30            # Connection establishment timeout
acquire_timeout_seconds = 60               # Pool acquisition timeout
max_lifetime_seconds = 1800                # Recreate connections every 30 min

# Connection validation
test_before_acquire = true                 # Validate before use
test_query = "SELECT 1"                    # Simple validation query
```

#### 2. Connection Pool Monitoring

```rust
pub struct ConnectionPoolMetrics {
    pub active_connections: u32,
    pub idle_connections: u32, 
    pub total_connections: u32,
    pub pending_requests: u32,
    pub pool_utilization: f64,
}

impl MemoryRepository {
    pub async fn get_pool_metrics(&self) -> ConnectionPoolMetrics {
        let pool_state = self.pool.state();
        
        ConnectionPoolMetrics {
            active_connections: pool_state.connections - pool_state.idle_connections,
            idle_connections: pool_state.idle_connections,
            total_connections: pool_state.connections,
            pending_requests: pool_state.pending,
            pool_utilization: (pool_state.connections as f64) / (self.pool.max_size() as f64),
        }
    }
    
    pub async fn log_pool_metrics(&self) {
        let metrics = self.get_pool_metrics().await;
        
        if metrics.pool_utilization > 0.8 {
            tracing::warn!(
                "High connection pool utilization: {:.1}% ({}/{})", 
                metrics.pool_utilization * 100.0,
                metrics.total_connections,
                self.pool.max_size()
            );
        }
        
        tracing::info!(
            "Pool: active={}, idle={}, pending={}, utilization={:.1}%",
            metrics.active_connections,
            metrics.idle_connections, 
            metrics.pending_requests,
            metrics.pool_utilization * 100.0
        );
    }
}
```

#### 3. Connection Pool Optimization Strategies

```rust
// Implement connection pool warming
impl MemoryRepository {
    pub async fn warm_connection_pool(&self) -> Result<()> {
        let target_warm_connections = (self.pool.max_size() as f64 * 0.5) as usize;
        let mut connections = Vec::new();
        
        // Acquire connections to warm the pool
        for _ in 0..target_warm_connections {
            if let Ok(conn) = self.pool.acquire().await {
                connections.push(conn);
            }
        }
        
        // Release them back to the pool
        drop(connections);
        
        tracing::info!("Warmed {} connections", target_warm_connections);
        Ok(())
    }
    
    // Monitor and alert on pool pressure
    pub async fn monitor_pool_pressure(&self) {
        let metrics = self.get_pool_metrics().await;
        
        if metrics.pool_utilization > 0.9 {
            // Critical: Pool almost exhausted
            tracing::error!("Critical pool pressure: {:.1}%", metrics.pool_utilization * 100.0);
        } else if metrics.pool_utilization > 0.8 {
            // Warning: High utilization
            tracing::warn!("High pool utilization: {:.1}%", metrics.pool_utilization * 100.0);
        }
        
        if metrics.pending_requests > 10 {
            tracing::warn!("High pending requests: {}", metrics.pending_requests);
        }
    }
}
```

## Monitoring and Profiling

### Performance Metrics Collection

#### 1. Application-Level Metrics

```rust
use prometheus::{Histogram, Counter, Gauge, Registry};
use std::time::Instant;

pub struct PerformanceMetrics {
    request_duration: Histogram,
    request_count: Counter,
    active_connections: Gauge,
    memory_tier_distribution: Gauge,
    search_latency: Histogram,
    cache_hit_ratio: Histogram,
}

impl PerformanceMetrics {
    pub fn new(registry: &Registry) -> Self {
        let request_duration = Histogram::with_opts(
            prometheus::HistogramOpts::new("request_duration_seconds", "Request duration")
                .buckets(vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0])
        ).unwrap();
        
        let search_latency = Histogram::with_opts(
            prometheus::HistogramOpts::new("search_latency_seconds", "Search operation latency")
                .buckets(vec![0.01, 0.05, 0.1, 0.5, 1.0, 2.0, 5.0, 10.0])
        ).unwrap();
        
        registry.register(Box::new(request_duration.clone())).unwrap();
        registry.register(Box::new(search_latency.clone())).unwrap();
        
        Self {
            request_duration,
            search_latency,
            // ... other metrics
        }
    }
    
    pub fn time_request<F, T>(&self, f: F) -> T 
    where F: FnOnce() -> T {
        let start = Instant::now();
        let result = f();
        self.request_duration.observe(start.elapsed().as_secs_f64());
        result
    }
}
```

#### 2. Database Performance Monitoring

```sql
-- Create monitoring views for performance analysis
CREATE OR REPLACE VIEW performance_summary AS
SELECT 
  'queries' as metric_type,
  query_id::text as identifier,
  calls,
  total_exec_time / 1000 as total_seconds,
  mean_exec_time / 1000 as mean_seconds,
  rows,
  shared_blks_hit,
  shared_blks_read
FROM pg_stat_statements 
WHERE calls > 100
ORDER BY total_exec_time DESC
LIMIT 20;

-- Index usage analysis
CREATE OR REPLACE VIEW index_usage_summary AS
SELECT 
  schemaname,
  tablename, 
  indexname,
  idx_scan,
  idx_tup_read,
  idx_tup_fetch,
  pg_size_pretty(pg_relation_size(indexname::regclass)) as size
FROM pg_stat_user_indexes
ORDER BY idx_scan DESC;

-- Connection and lock monitoring  
CREATE OR REPLACE VIEW connection_summary AS
SELECT 
  count(*) as total_connections,
  count(*) FILTER (WHERE state = 'active') as active,
  count(*) FILTER (WHERE state = 'idle') as idle,
  count(*) FILTER (WHERE state = 'idle in transaction') as idle_in_transaction
FROM pg_stat_activity;
```

#### 3. Automated Performance Reports

```bash
#!/bin/bash
# /usr/local/bin/performance-report.sh

echo "=== Memory System Performance Report $(date) ===" > /tmp/perf-report.txt

# Application metrics
echo "--- Request Metrics ---" >> /tmp/perf-report.txt
curl -s http://localhost:3333/metrics | grep -E "(request_duration|search_latency)" >> /tmp/perf-report.txt

# Database performance  
echo "--- Database Performance ---" >> /tmp/perf-report.txt
psql $DATABASE_URL -c "SELECT * FROM performance_summary;" >> /tmp/perf-report.txt

# System resources
echo "--- System Resources ---" >> /tmp/perf-report.txt
echo "CPU: $(top -bn1 | grep "Cpu(s)" | awk '{print $2}' | cut -d'%' -f1)%" >> /tmp/perf-report.txt
echo "Memory: $(free -m | awk 'NR==2{printf "%.1f%%\n", $3*100/$2 }')" >> /tmp/perf-report.txt
echo "Disk: $(df -h /var/lib/postgresql | awk 'NR==2{print $5}')" >> /tmp/perf-report.txt

# Connection pool status
echo "--- Connection Pool ---" >> /tmp/perf-report.txt
curl -s http://localhost:3333/debug/pool-stats >> /tmp/perf-report.txt

# Send report  
mail -s "Memory System Performance Report" admin@company.com < /tmp/perf-report.txt
```

## Load Testing and Benchmarking

### Load Testing Framework

#### 1. Basic Load Test

```bash
#!/bin/bash
# /usr/local/bin/load-test.sh

CONCURRENCY=${1:-50}
DURATION=${2:-300}
TARGET_HOST=${3:-localhost:3333}

echo "Starting load test: $CONCURRENCY concurrent users for ${DURATION}s"

# Test memory operations
ab -n 10000 -c $CONCURRENCY -T application/json -p create-memory.json \
  http://$TARGET_HOST/api/v1/memories

# Test search operations  
ab -n 5000 -c $((CONCURRENCY/2)) -T application/json -p search-request.json \
  http://$TARGET_HOST/api/v1/search

# Test read operations
ab -n 15000 -c $CONCURRENCY \
  http://$TARGET_HOST/api/v1/memories/550e8400-e29b-41d4-a716-446655440000

echo "Load test completed"
```

#### 2. Realistic Workload Simulation

```python
#!/usr/bin/env python3
# realistic-load-test.py

import asyncio
import aiohttp
import json
import random
import time
from typing import List

class MemorySystemLoadTest:
    def __init__(self, base_url: str, concurrent_users: int):
        self.base_url = base_url
        self.concurrent_users = concurrent_users
        self.memory_ids: List[str] = []
        
    async def create_memory(self, session: aiohttp.ClientSession, user_id: int):
        """Simulate memory creation"""
        content = f"User {user_id} memory content {random.randint(1, 1000)}"
        data = {
            "content": content,
            "tier": random.choice(["Working", "Warm", "Cold"]),
            "importance_score": random.uniform(0.1, 1.0),
            "metadata": {"user_id": user_id, "test": True}
        }
        
        async with session.post(f"{self.base_url}/api/v1/memories", json=data) as resp:
            if resp.status == 200:
                result = await resp.json()
                self.memory_ids.append(result["id"])
                return True
        return False
    
    async def search_memories(self, session: aiohttp.ClientSession, user_id: int):
        """Simulate search operations"""
        queries = [
            "machine learning", "database optimization", "rust programming",
            "performance tuning", "memory management", "search algorithms"
        ]
        
        data = {
            "query_text": random.choice(queries),
            "limit": random.randint(5, 20),
            "similarity_threshold": random.uniform(0.6, 0.9)
        }
        
        async with session.post(f"{self.base_url}/api/v1/search", json=data) as resp:
            return resp.status == 200
    
    async def read_memory(self, session: aiohttp.ClientSession):
        """Simulate memory reads"""
        if not self.memory_ids:
            return False
            
        memory_id = random.choice(self.memory_ids)
        async with session.get(f"{self.base_url}/api/v1/memories/{memory_id}") as resp:
            return resp.status == 200
    
    async def user_simulation(self, user_id: int, duration: int):
        """Simulate realistic user behavior"""
        async with aiohttp.ClientSession() as session:
            start_time = time.time()
            operations = {"create": 0, "search": 0, "read": 0, "errors": 0}
            
            while time.time() - start_time < duration:
                # Realistic operation distribution
                rand = random.random()
                
                try:
                    if rand < 0.2:  # 20% creates
                        success = await self.create_memory(session, user_id)
                        operations["create"] += 1
                    elif rand < 0.5:  # 30% searches  
                        success = await self.search_memories(session, user_id)
                        operations["search"] += 1
                    else:  # 50% reads
                        success = await self.read_memory(session)
                        operations["read"] += 1
                        
                    if not success:
                        operations["errors"] += 1
                        
                except Exception as e:
                    operations["errors"] += 1
                    
                # Realistic think time
                await asyncio.sleep(random.uniform(0.1, 2.0))
            
            return operations
    
    async def run_load_test(self, duration: int = 300):
        """Run the complete load test"""
        print(f"Starting load test: {self.concurrent_users} users for {duration}s")
        
        # Start all user simulations
        tasks = [
            self.user_simulation(user_id, duration) 
            for user_id in range(self.concurrent_users)
        ]
        
        results = await asyncio.gather(*tasks)
        
        # Aggregate results
        total_ops = {"create": 0, "search": 0, "read": 0, "errors": 0}
        for result in results:
            for op, count in result.items():
                total_ops[op] += count
        
        total_requests = sum(total_ops.values())
        throughput = total_requests / duration
        error_rate = (total_ops["errors"] / total_requests) * 100 if total_requests > 0 else 0
        
        print(f"\nLoad Test Results:")
        print(f"Total Requests: {total_requests}")
        print(f"Throughput: {throughput:.1f} req/sec")
        print(f"Error Rate: {error_rate:.2f}%")
        print(f"Operations: {total_ops}")

if __name__ == "__main__":
    test = MemorySystemLoadTest("http://localhost:3333", 50)
    asyncio.run(test.run_load_test(300))
```

### Performance Benchmarking

#### 1. Database Benchmark Script

```bash
#!/bin/bash
# database-benchmark.sh

echo "=== Database Performance Benchmark ==="

# Test connection establishment
echo "--- Connection Test ---"
time for i in {1..100}; do
  psql $DATABASE_URL -c "SELECT 1;" > /dev/null
done

# Test memory tier queries
echo "--- Memory Tier Query Performance ---"
for tier in working warm cold; do
  echo "Testing $tier tier:"
  time psql $DATABASE_URL -c "
    SELECT COUNT(*) FROM memories 
    WHERE tier = '$tier' AND status = 'active';
  "
done

# Test vector search performance
echo "--- Vector Search Performance ---"
time psql $DATABASE_URL -c "
  SELECT id, 1 - (embedding <=> '\$1') as similarity 
  FROM memories 
  WHERE tier = 'working' AND status = 'active'
  ORDER BY embedding <=> '\$1' 
  LIMIT 10;
" -v embedding="[0.1,0.2,0.3,0.4,0.5]"

# Test concurrent connections
echo "--- Concurrent Connection Test ---"
for i in {1..10}; do
  psql $DATABASE_URL -c "SELECT pg_sleep(1);" &
done
wait

echo "Benchmark completed"
```

## Troubleshooting Performance Issues

### Performance Issue Diagnosis

#### 1. Systematic Performance Debugging

```bash
#!/bin/bash
# performance-debug.sh

echo "=== Performance Diagnosis ==="

# 1. Check system resources
echo "--- System Resources ---"
echo "CPU Usage:"
top -bn1 | grep "Cpu(s)"
echo "Memory Usage:"
free -m
echo "Disk I/O:"
iostat -x 1 1

# 2. Check application metrics
echo "--- Application Metrics ---"
curl -s http://localhost:3333/metrics | grep -E "(duration|latency|pool)"

# 3. Check database performance
echo "--- Database Performance ---"
psql $DATABASE_URL -c "
  SELECT query, calls, total_exec_time / calls as avg_time
  FROM pg_stat_statements 
  ORDER BY total_exec_time DESC 
  LIMIT 10;
"

# 4. Check for blocking queries
echo "--- Blocking Queries ---"
psql $DATABASE_URL -c "
  SELECT blocked_locks.pid, blocked_activity.query
  FROM pg_catalog.pg_locks blocked_locks
  JOIN pg_catalog.pg_stat_activity blocked_activity ON blocked_activity.pid = blocked_locks.pid
  WHERE NOT blocked_locks.granted;
"

# 5. Check connection pool status
echo "--- Connection Pool ---"
curl -s http://localhost:3333/debug/pool-status | jq '.'
```

#### 2. Performance Regression Detection

```python
#!/usr/bin/env python3
# performance-regression.py

import json
import subprocess
import time
from typing import Dict, List

class PerformanceRegression:
    def __init__(self, baseline_file: str = "performance_baseline.json"):
        self.baseline_file = baseline_file
        
    def measure_performance(self) -> Dict:
        """Measure current performance metrics"""
        metrics = {}
        
        # Measure API response times
        start = time.time()
        subprocess.run(["curl", "-s", "http://localhost:3333/api/v1/health"], 
                      capture_output=True)
        metrics["health_check_time"] = time.time() - start
        
        # Measure search performance
        start = time.time()
        subprocess.run([
            "curl", "-s", "-X", "POST", 
            "http://localhost:3333/api/v1/search",
            "-H", "Content-Type: application/json",
            "-d", '{"query_text": "test", "limit": 10}'
        ], capture_output=True)
        metrics["search_time"] = time.time() - start
        
        return metrics
    
    def save_baseline(self):
        """Save current performance as baseline"""
        metrics = self.measure_performance()
        with open(self.baseline_file, 'w') as f:
            json.dump(metrics, f, indent=2)
        print(f"Baseline saved: {metrics}")
    
    def check_regression(self, threshold: float = 0.2):
        """Check for performance regression"""
        try:
            with open(self.baseline_file, 'r') as f:
                baseline = json.load(f)
        except FileNotFoundError:
            print("No baseline found. Run with --save-baseline first.")
            return
        
        current = self.measure_performance()
        regressions = []
        
        for metric, current_value in current.items():
            baseline_value = baseline.get(metric)
            if baseline_value:
                regression = (current_value - baseline_value) / baseline_value
                if regression > threshold:
                    regressions.append({
                        "metric": metric,
                        "baseline": baseline_value,
                        "current": current_value,
                        "regression": f"{regression*100:.1f}%"
                    })
        
        if regressions:
            print("⚠️  Performance regressions detected:")
            for reg in regressions:
                print(f"  {reg['metric']}: {reg['baseline']:.3f}s → {reg['current']:.3f}s ({reg['regression']})")
            return False
        else:
            print("✅ No performance regressions detected")
            return True

if __name__ == "__main__":
    import sys
    pr = PerformanceRegression()
    
    if "--save-baseline" in sys.argv:
        pr.save_baseline()
    else:
        pr.check_regression()
```

This performance tuning guide provides comprehensive strategies for optimizing every aspect of the Agentic Memory System. Regular application of these techniques will ensure the system maintains optimal performance as it scales.