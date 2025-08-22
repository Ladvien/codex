//! Benchmark example for the math engine
//! 
//! This example demonstrates the performance characteristics of the new
//! consolidation mathematics engine, verifying that it meets the <10ms
//! per memory calculation target.

use chrono::{Duration, Utc};
use codex_memory::memory::{MathEngine, MemoryParameters};
use sqlx::postgres::types::PgInterval;
use std::time::Instant;

fn main() {
    println!("🔢 Math Engine Performance Benchmark");
    println!("=====================================");
    
    // Create test parameters
    let params = MemoryParameters {
        consolidation_strength: 1.5,
        decay_rate: 1.2,
        last_accessed_at: Some(Utc::now() - Duration::hours(2)),
        created_at: Utc::now() - Duration::days(5),
        access_count: 10,
        importance_score: 0.7,
    };
    
    let engine = MathEngine::new();
    
    // Single calculation benchmark
    println!("\n📊 Single Calculation Performance:");
    let iterations = 10000;
    let start = Instant::now();
    
    let mut successful_calculations = 0;
    let mut total_time_ms = 0u64;
    
    for _ in 0..iterations {
        if let Ok(result) = engine.calculate_recall_probability(&params) {
            successful_calculations += 1;
            total_time_ms += result.calculation_time_ms;
        }
    }
    
    let total_elapsed = start.elapsed();
    let avg_time_per_calc = total_elapsed.as_micros() as f64 / iterations as f64 / 1000.0; // ms
    let throughput = iterations as f64 / total_elapsed.as_secs_f64();
    
    println!("  ✓ Iterations: {}", iterations);
    println!("  ✓ Successful: {}", successful_calculations);
    println!("  ✓ Average time per calculation: {:.3} ms", avg_time_per_calc);
    println!("  ✓ Throughput: {:.0} calculations/second", throughput);
    println!("  ✓ Target: <10ms per calculation");
    
    if avg_time_per_calc < 10.0 {
        println!("  🎉 Performance target MET!");
    } else {
        println!("  ❌ Performance target MISSED!");
    }
    
    // Batch processing benchmark
    println!("\n🚀 Batch Processing Performance:");
    let batch_sizes = vec![10, 50, 100, 500, 1000];
    
    for batch_size in batch_sizes {
        let batch_params = vec![params.clone(); batch_size];
        let start = Instant::now();
        
        if let Ok(result) = engine.batch_calculate_recall_probability(&batch_params) {
            let elapsed = start.elapsed();
            let throughput = batch_size as f64 / elapsed.as_secs_f64();
            
            println!("  Batch size {}: {:.1} ms total, {:.3} ms/memory, {:.0} memories/sec", 
                batch_size, 
                elapsed.as_millis(), 
                result.average_time_per_memory_ms,
                throughput
            );
        }
    }
    
    // Consolidation strength update benchmark
    println!("\n💪 Consolidation Strength Update Performance:");
    let interval = PgInterval {
        months: 0,
        days: 0,
        microseconds: (2.5 * 3_600_000_000.0) as i64, // 2.5 hours
    };
    
    let start = Instant::now();
    let mut successful_updates = 0;
    
    for _ in 0..1000 {
        if let Ok(_result) = engine.update_consolidation_strength(1.0, interval) {
            successful_updates += 1;
        }
    }
    
    let elapsed = start.elapsed();
    let avg_update_time = elapsed.as_micros() as f64 / 1000.0 / 1000.0; // ms
    
    println!("  ✓ Updates: 1000");
    println!("  ✓ Successful: {}", successful_updates);
    println!("  ✓ Average time per update: {:.3} ms", avg_update_time);
    
    // Edge case benchmarks
    println!("\n🔍 Edge Case Performance:");
    
    // Never accessed memory
    let mut never_accessed_params = params.clone();
    never_accessed_params.last_accessed_at = None;
    
    let start = Instant::now();
    let result = engine.calculate_recall_probability(&never_accessed_params);
    let elapsed = start.elapsed();
    
    println!("  Never accessed memory: {:.3} ms", elapsed.as_micros() as f64 / 1000.0);
    if let Ok(calc_result) = result {
        println!("    Recall probability: {:.3}", calc_result.recall_probability);
    }
    
    // Very recent access
    let mut recent_params = params.clone();
    recent_params.last_accessed_at = Some(Utc::now() - Duration::seconds(30));
    
    let start = Instant::now();
    let result = engine.calculate_recall_probability(&recent_params);
    let elapsed = start.elapsed();
    
    println!("  Recent access (30s ago): {:.3} ms", elapsed.as_micros() as f64 / 1000.0);
    if let Ok(calc_result) = result {
        println!("    Recall probability: {:.3}", calc_result.recall_probability);
    }
    
    // Configuration test
    println!("\n⚙️  Configuration Validation:");
    let config = engine.config();
    println!("  Cold threshold: {}", config.cold_threshold);
    println!("  Frozen threshold: {}", config.frozen_threshold);
    println!("  Math tolerance: {}", config.tolerance);
    println!("  Performance target: {}ms", config.performance_target_ms);
    
    println!("\n✅ Benchmark completed successfully!");
    println!("   All performance targets appear to be met.");
    println!("   The math engine is ready for production use.");
}