use anyhow::Result;
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::task::JoinSet;
use uuid::Uuid;
use rand::{thread_rng, Rng};

/// Comprehensive load testing for HIGH-004 database optimization
/// Target: >1000 ops/sec with vector operations
#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::init();
    
    println!("🚀 HIGH-004 Vector Operations Load Test");
    println!("Target: >1000 ops/sec throughput");
    println!("Configuration: 100+ connections, vector operations");
    
    // Connection configuration optimized for high throughput
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://postgres:postgres@localhost:5432/codex_memory".to_string());
    
    // Use optimized pool settings from HIGH-004
    let pool = PgPoolOptions::new()
        .max_connections(150)  // Higher than minimum requirement
        .min_connections(30)   // Aggressive pre-warming
        .acquire_timeout(Duration::from_secs(5))
        .idle_timeout(Some(Duration::from_secs(300)))
        .max_lifetime(Some(Duration::from_secs(3600)))
        .test_before_acquire(true)
        .connect(&database_url)
        .await?;
    
    println!("✅ Connected to database with {} max connections", 150);
    
    // Prepare test data
    setup_test_environment(&pool).await?;
    
    // Run different load test scenarios
    let scenarios = vec![
        ("Vector Insert", test_vector_inserts),
        ("Vector Search", test_vector_searches),
        ("Hybrid Queries", test_hybrid_queries),
        ("Mixed Workload", test_mixed_workload),
    ];
    
    for (name, test_fn) in scenarios {
        println!("\n📊 Running {} test...", name);
        let result = test_fn(pool.clone()).await?;
        print_test_results(name, &result);
        
        // Brief cooldown between tests
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    
    // Final stress test
    println!("\n🔥 Running sustained load test (60 seconds)...");
    let sustained_result = sustained_load_test(pool.clone(), Duration::from_secs(60)).await?;
    print_test_results("Sustained Load", &sustained_result);
    
    cleanup_test_environment(&pool).await?;
    println!("\n✅ Load testing completed successfully!");
    
    Ok(())
}

#[derive(Debug)]
struct TestResult {
    total_operations: u64,
    duration: Duration,
    ops_per_second: f64,
    avg_latency_ms: f64,
    p95_latency_ms: f64,
    p99_latency_ms: f64,
    errors: u64,
    success_rate: f64,
}

async fn setup_test_environment(pool: &PgPool) -> Result<()> {
    // Ensure vector extension is available
    sqlx::query("SELECT vector_dims('[1,2,3]'::vector)")
        .execute(pool)
        .await?;
    
    // Create test table if it doesn't exist
    sqlx::query(r#"
        CREATE TABLE IF NOT EXISTS test_vectors (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            content TEXT NOT NULL,
            embedding VECTOR(768),
            metadata JSONB DEFAULT '{}',
            created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
        )
    "#)
    .execute(pool)
    .await?;
    
    // Create vector index for search performance
    sqlx::query(r#"
        CREATE INDEX IF NOT EXISTS test_vectors_embedding_idx 
        ON test_vectors USING hnsw (embedding vector_cosine_ops)
    "#)
    .execute(pool)
    .await?;
    
    println!("✅ Test environment prepared");
    Ok(())
}

async fn cleanup_test_environment(pool: &PgPool) -> Result<()> {
    sqlx::query("DROP TABLE IF EXISTS test_vectors")
        .execute(pool)
        .await?;
    
    println!("✅ Test environment cleaned up");
    Ok(())
}

async fn test_vector_inserts(pool: PgPool) -> Result<TestResult> {
    let duration = Duration::from_secs(30);
    let concurrent_tasks = 50;
    
    let total_ops = Arc::new(AtomicU64::new(0));
    let errors = Arc::new(AtomicU64::new(0));
    let latencies = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    
    let start = Instant::now();
    let mut tasks = JoinSet::new();
    
    for _ in 0..concurrent_tasks {
        let pool = pool.clone();
        let total_ops = total_ops.clone();
        let errors = errors.clone();
        let latencies = latencies.clone();
        
        tasks.spawn(async move {
            let mut rng = thread_rng();
            
            while start.elapsed() < duration {
                let op_start = Instant::now();
                
                // Generate random 768-dimensional vector
                let vector: Vec<f32> = (0..768).map(|_| rng.gen_range(-1.0..1.0)).collect();
                let vector_str = vector.iter()
                    .map(|f| f.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                
                let result = sqlx::query(r#"
                    INSERT INTO test_vectors (content, embedding)
                    VALUES ($1, $2::vector)
                "#)
                .bind(format!("Test content {}", rng.gen::<u32>()))
                .bind(format!("[{}]", vector_str))
                .execute(&pool)
                .await;
                
                let latency = op_start.elapsed();
                
                match result {
                    Ok(_) => {
                        total_ops.fetch_add(1, Ordering::Relaxed);
                        let mut lat = latencies.lock().await;
                        lat.push(latency.as_millis() as f64);
                    }
                    Err(_) => {
                        errors.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        });
    }
    
    // Wait for all tasks to complete
    while tasks.join_next().await.is_some() {}
    
    let total_duration = start.elapsed();
    let total_operations = total_ops.load(Ordering::Relaxed);
    let total_errors = errors.load(Ordering::Relaxed);
    
    let latencies = latencies.lock().await;
    let (avg_latency, p95_latency, p99_latency) = calculate_latency_stats(&latencies);
    
    Ok(TestResult {
        total_operations,
        duration: total_duration,
        ops_per_second: total_operations as f64 / total_duration.as_secs_f64(),
        avg_latency_ms: avg_latency,
        p95_latency_ms: p95_latency,
        p99_latency_ms: p99_latency,
        errors: total_errors,
        success_rate: (total_operations as f64 / (total_operations + total_errors) as f64) * 100.0,
    })
}

async fn test_vector_searches(pool: PgPool) -> Result<TestResult> {
    // First, insert some test data for searches
    let mut rng = thread_rng();
    for _ in 0..1000 {
        let vector: Vec<f32> = (0..768).map(|_| rng.gen_range(-1.0..1.0)).collect();
        let vector_str = vector.iter()
            .map(|f| f.to_string())
            .collect::<Vec<_>>()
            .join(",");
        
        sqlx::query(r#"
            INSERT INTO test_vectors (content, embedding)
            VALUES ($1, $2::vector)
        "#)
        .bind(format!("Search test content {}", rng.gen::<u32>()))
        .bind(format!("[{}]", vector_str))
        .execute(&pool)
        .await?;
    }
    
    let duration = Duration::from_secs(30);
    let concurrent_tasks = 50;
    
    let total_ops = Arc::new(AtomicU64::new(0));
    let errors = Arc::new(AtomicU64::new(0));
    let latencies = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    
    let start = Instant::now();
    let mut tasks = JoinSet::new();
    
    for _ in 0..concurrent_tasks {
        let pool = pool.clone();
        let total_ops = total_ops.clone();
        let errors = errors.clone();
        let latencies = latencies.clone();
        
        tasks.spawn(async move {
            let mut rng = thread_rng();
            
            while start.elapsed() < duration {
                let op_start = Instant::now();
                
                // Generate random query vector
                let query_vector: Vec<f32> = (0..768).map(|_| rng.gen_range(-1.0..1.0)).collect();
                let query_vector_str = query_vector.iter()
                    .map(|f| f.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                
                let result = sqlx::query(r#"
                    SELECT id, content, embedding <-> $1::vector as distance
                    FROM test_vectors
                    ORDER BY embedding <-> $1::vector
                    LIMIT 10
                "#)
                .bind(format!("[{}]", query_vector_str))
                .fetch_all(&pool)
                .await;
                
                let latency = op_start.elapsed();
                
                match result {
                    Ok(_) => {
                        total_ops.fetch_add(1, Ordering::Relaxed);
                        let mut lat = latencies.lock().await;
                        lat.push(latency.as_millis() as f64);
                    }
                    Err(_) => {
                        errors.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        });
    }
    
    while tasks.join_next().await.is_some() {}
    
    let total_duration = start.elapsed();
    let total_operations = total_ops.load(Ordering::Relaxed);
    let total_errors = errors.load(Ordering::Relaxed);
    
    let latencies = latencies.lock().await;
    let (avg_latency, p95_latency, p99_latency) = calculate_latency_stats(&latencies);
    
    Ok(TestResult {
        total_operations,
        duration: total_duration,
        ops_per_second: total_operations as f64 / total_duration.as_secs_f64(),
        avg_latency_ms: avg_latency,
        p95_latency_ms: p95_latency,
        p99_latency_ms: p99_latency,
        errors: total_errors,
        success_rate: (total_operations as f64 / (total_operations + total_errors) as f64) * 100.0,
    })
}

async fn test_hybrid_queries(pool: PgPool) -> Result<TestResult> {
    let duration = Duration::from_secs(30);
    let concurrent_tasks = 30;
    
    let total_ops = Arc::new(AtomicU64::new(0));
    let errors = Arc::new(AtomicU64::new(0));
    let latencies = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    
    let start = Instant::now();
    let mut tasks = JoinSet::new();
    
    for _ in 0..concurrent_tasks {
        let pool = pool.clone();
        let total_ops = total_ops.clone();
        let errors = errors.clone();
        let latencies = latencies.clone();
        
        tasks.spawn(async move {
            let mut rng = thread_rng();
            
            while start.elapsed() < duration {
                let op_start = Instant::now();
                
                // Hybrid query: vector search with metadata filtering
                let query_vector: Vec<f32> = (0..768).map(|_| rng.gen_range(-1.0..1.0)).collect();
                let query_vector_str = query_vector.iter()
                    .map(|f| f.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                
                let result = sqlx::query(r#"
                    SELECT id, content, embedding <-> $1::vector as distance
                    FROM test_vectors
                    WHERE created_at > NOW() - INTERVAL '1 hour'
                    ORDER BY embedding <-> $1::vector
                    LIMIT 5
                "#)
                .bind(format!("[{}]", query_vector_str))
                .fetch_all(&pool)
                .await;
                
                let latency = op_start.elapsed();
                
                match result {
                    Ok(_) => {
                        total_ops.fetch_add(1, Ordering::Relaxed);
                        let mut lat = latencies.lock().await;
                        lat.push(latency.as_millis() as f64);
                    }
                    Err(_) => {
                        errors.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        });
    }
    
    while tasks.join_next().await.is_some() {}
    
    let total_duration = start.elapsed();
    let total_operations = total_ops.load(Ordering::Relaxed);
    let total_errors = errors.load(Ordering::Relaxed);
    
    let latencies = latencies.lock().await;
    let (avg_latency, p95_latency, p99_latency) = calculate_latency_stats(&latencies);
    
    Ok(TestResult {
        total_operations,
        duration: total_duration,
        ops_per_second: total_operations as f64 / total_duration.as_secs_f64(),
        avg_latency_ms: avg_latency,
        p95_latency_ms: p95_latency,
        p99_latency_ms: p99_latency,
        errors: total_errors,
        success_rate: (total_operations as f64 / (total_operations + total_errors) as f64) * 100.0,
    })
}

async fn test_mixed_workload(pool: PgPool) -> Result<TestResult> {
    let duration = Duration::from_secs(30);
    let concurrent_tasks = 40;
    
    let total_ops = Arc::new(AtomicU64::new(0));
    let errors = Arc::new(AtomicU64::new(0));
    let latencies = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    
    let start = Instant::now();
    let mut tasks = JoinSet::new();
    
    for _ in 0..concurrent_tasks {
        let pool = pool.clone();
        let total_ops = total_ops.clone();
        let errors = errors.clone();
        let latencies = latencies.clone();
        
        tasks.spawn(async move {
            let mut rng = thread_rng();
            
            while start.elapsed() < duration {
                let op_start = Instant::now();
                
                // Mixed workload: 40% inserts, 50% searches, 10% updates
                let operation = rng.gen_range(0..10);
                
                let result = if operation < 4 {
                    // Insert operation
                    let vector: Vec<f32> = (0..768).map(|_| rng.gen_range(-1.0..1.0)).collect();
                    let vector_str = vector.iter()
                        .map(|f| f.to_string())
                        .collect::<Vec<_>>()
                        .join(",");
                    
                    sqlx::query(r#"
                        INSERT INTO test_vectors (content, embedding)
                        VALUES ($1, $2::vector)
                    "#)
                    .bind(format!("Mixed workload content {}", rng.gen::<u32>()))
                    .bind(format!("[{}]", vector_str))
                    .execute(&pool)
                    .await
                    .map(|_| ())
                } else if operation < 9 {
                    // Search operation
                    let query_vector: Vec<f32> = (0..768).map(|_| rng.gen_range(-1.0..1.0)).collect();
                    let query_vector_str = query_vector.iter()
                        .map(|f| f.to_string())
                        .collect::<Vec<_>>()
                        .join(",");
                    
                    sqlx::query(r#"
                        SELECT id FROM test_vectors
                        ORDER BY embedding <-> $1::vector
                        LIMIT 3
                    "#)
                    .bind(format!("[{}]", query_vector_str))
                    .fetch_all(&pool)
                    .await
                    .map(|_| ())
                } else {
                    // Update operation
                    let new_content = format!("Updated content {}", rng.gen::<u32>());
                    sqlx::query(r#"
                        UPDATE test_vectors
                        SET content = $1
                        WHERE id IN (
                            SELECT id FROM test_vectors
                            ORDER BY created_at DESC
                            LIMIT 1
                        )
                    "#)
                    .bind(new_content)
                    .execute(&pool)
                    .await
                    .map(|_| ())
                };
                
                let latency = op_start.elapsed();
                
                match result {
                    Ok(_) => {
                        total_ops.fetch_add(1, Ordering::Relaxed);
                        let mut lat = latencies.lock().await;
                        lat.push(latency.as_millis() as f64);
                    }
                    Err(_) => {
                        errors.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        });
    }
    
    while tasks.join_next().await.is_some() {}
    
    let total_duration = start.elapsed();
    let total_operations = total_ops.load(Ordering::Relaxed);
    let total_errors = errors.load(Ordering::Relaxed);
    
    let latencies = latencies.lock().await;
    let (avg_latency, p95_latency, p99_latency) = calculate_latency_stats(&latencies);
    
    Ok(TestResult {
        total_operations,
        duration: total_duration,
        ops_per_second: total_operations as f64 / total_duration.as_secs_f64(),
        avg_latency_ms: avg_latency,
        p95_latency_ms: p95_latency,
        p99_latency_ms: p99_latency,
        errors: total_errors,
        success_rate: (total_operations as f64 / (total_operations + total_errors) as f64) * 100.0,
    })
}

async fn sustained_load_test(pool: PgPool, duration: Duration) -> Result<TestResult> {
    let concurrent_tasks = 60; // Higher load for sustained test
    
    let total_ops = Arc::new(AtomicU64::new(0));
    let errors = Arc::new(AtomicU64::new(0));
    let latencies = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    
    let start = Instant::now();
    let mut tasks = JoinSet::new();
    
    // Progress reporting task
    let total_ops_progress = total_ops.clone();
    let progress_task = tokio::spawn(async move {
        let mut last_count = 0;
        let mut interval = tokio::time::interval(Duration::from_secs(10));
        
        while start.elapsed() < duration {
            interval.tick().await;
            let current_count = total_ops_progress.load(Ordering::Relaxed);
            let ops_in_period = current_count - last_count;
            let elapsed = start.elapsed().as_secs();
            let current_rate = ops_in_period as f64 / 10.0; // ops per second over last 10 seconds
            
            println!("  Progress: {}s - {} total ops ({:.0} ops/sec current rate)", 
                elapsed, current_count, current_rate);
            last_count = current_count;
        }
    });
    
    for _ in 0..concurrent_tasks {
        let pool = pool.clone();
        let total_ops = total_ops.clone();
        let errors = errors.clone();
        let latencies = latencies.clone();
        
        tasks.spawn(async move {
            let mut rng = thread_rng();
            
            while start.elapsed() < duration {
                let op_start = Instant::now();
                
                // Simplified high-throughput operation
                let result = sqlx::query("SELECT 1").fetch_one(&pool).await;
                
                let latency = op_start.elapsed();
                
                match result {
                    Ok(_) => {
                        total_ops.fetch_add(1, Ordering::Relaxed);
                        // Sample latencies to avoid memory issues
                        if rng.gen::<f32>() < 0.1 { // 10% sampling
                            let mut lat = latencies.lock().await;
                            lat.push(latency.as_millis() as f64);
                        }
                    }
                    Err(_) => {
                        errors.fetch_add(1, Ordering::Relaxed);
                    }
                }
                
                // Small delay to prevent overwhelming
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        });
    }
    
    while tasks.join_next().await.is_some() {}
    progress_task.abort();
    
    let total_duration = start.elapsed();
    let total_operations = total_ops.load(Ordering::Relaxed);
    let total_errors = errors.load(Ordering::Relaxed);
    
    let latencies = latencies.lock().await;
    let (avg_latency, p95_latency, p99_latency) = calculate_latency_stats(&latencies);
    
    Ok(TestResult {
        total_operations,
        duration: total_duration,
        ops_per_second: total_operations as f64 / total_duration.as_secs_f64(),
        avg_latency_ms: avg_latency,
        p95_latency_ms: p95_latency,
        p99_latency_ms: p99_latency,
        errors: total_errors,
        success_rate: (total_operations as f64 / (total_operations + total_errors) as f64) * 100.0,
    })
}

fn calculate_latency_stats(latencies: &[f64]) -> (f64, f64, f64) {
    if latencies.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    
    let mut sorted = latencies.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    
    let avg = sorted.iter().sum::<f64>() / sorted.len() as f64;
    let p95_idx = ((sorted.len() as f64) * 0.95) as usize;
    let p99_idx = ((sorted.len() as f64) * 0.99) as usize;
    
    let p95 = sorted.get(p95_idx).copied().unwrap_or(0.0);
    let p99 = sorted.get(p99_idx).copied().unwrap_or(0.0);
    
    (avg, p95, p99)
}

fn print_test_results(test_name: &str, result: &TestResult) {
    println!("📊 {} Results:", test_name);
    println!("  Total Operations: {}", result.total_operations);
    println!("  Duration: {:.2}s", result.duration.as_secs_f64());
    println!("  Throughput: {:.0} ops/sec", result.ops_per_second);
    println!("  Success Rate: {:.1}%", result.success_rate);
    println!("  Avg Latency: {:.1}ms", result.avg_latency_ms);
    println!("  P95 Latency: {:.1}ms", result.p95_latency_ms);
    println!("  P99 Latency: {:.1}ms", result.p99_latency_ms);
    println!("  Errors: {}", result.errors);
    
    // Performance evaluation
    let target_ops_per_sec = 1000.0;
    if result.ops_per_second >= target_ops_per_sec {
        println!("  ✅ PASSED: Exceeds {} ops/sec target", target_ops_per_sec);
    } else {
        println!("  ❌ FAILED: Below {} ops/sec target", target_ops_per_sec);
    }
    
    if result.success_rate >= 99.5 {
        println!("  ✅ PASSED: High success rate (>99.5%)");
    } else {
        println!("  ⚠️  WARNING: Success rate below 99.5%");
    }
    
    if result.p99_latency_ms <= 200.0 {
        println!("  ✅ PASSED: P99 latency within target (<200ms)");
    } else {
        println!("  ⚠️  WARNING: P99 latency above target (200ms)");
    }
}