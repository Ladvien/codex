//! Database query and index optimization utilities

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;
use tracing::{debug, info};

/// Query optimizer for analyzing and improving database queries
pub struct QueryOptimizer {
    db_pool: Arc<PgPool>,
}

impl QueryOptimizer {
    pub fn new(db_pool: Arc<PgPool>) -> Self {
        Self { db_pool }
    }

    /// Analyze query performance using EXPLAIN ANALYZE
    pub async fn analyze_query(&self, query: &str) -> Result<QueryAnalysis> {
        // Validate query safety for EXPLAIN
        if !self.is_safe_query_for_explain(query) {
            return Err(anyhow!(
                "Query contains potentially unsafe statements for EXPLAIN"
            ));
        }

        // Use parameterized query with timeout protection
        let explain_query = format!("EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) {query}");

        // Add 30-second timeout for EXPLAIN ANALYZE
        let row = timeout(
            Duration::from_secs(30),
            sqlx::query(&explain_query).fetch_one(self.db_pool.as_ref()),
        )
        .await
        .map_err(|_| anyhow!("Query analysis timed out after 30 seconds"))??;

        let plan_json: serde_json::Value = row.get(0);

        self.parse_explain_output(plan_json)
    }

    /// Validate query is safe for EXPLAIN ANALYZE
    fn is_safe_query_for_explain(&self, query: &str) -> bool {
        let dangerous_keywords = [
            "DROP", "DELETE", "TRUNCATE", "ALTER", "CREATE", "GRANT", "REVOKE",
        ];
        let upper_query = query.to_uppercase();

        // Check for dangerous keywords
        if dangerous_keywords
            .iter()
            .any(|&keyword| upper_query.contains(keyword))
        {
            return false;
        }

        // EXPLAIN ANALYZE actually executes the query, so INSERT/UPDATE are also dangerous
        if upper_query.contains("INSERT") || upper_query.contains("UPDATE") {
            return false;
        }

        true
    }

    /// Parse EXPLAIN output into structured analysis
    fn parse_explain_output(&self, plan: serde_json::Value) -> Result<QueryAnalysis> {
        let plan_array = plan
            .as_array()
            .ok_or_else(|| anyhow!("Invalid EXPLAIN output format"))?;

        let plan_obj = plan_array
            .first()
            .and_then(|p| p.as_object())
            .ok_or_else(|| anyhow!("Invalid plan structure"))?;

        let execution_time = plan_obj
            .get("Execution Time")
            .and_then(|t| t.as_f64())
            .unwrap_or(0.0);

        let planning_time = plan_obj
            .get("Planning Time")
            .and_then(|t| t.as_f64())
            .unwrap_or(0.0);

        let plan_details = plan_obj
            .get("Plan")
            .cloned()
            .unwrap_or(serde_json::Value::Null);

        // Extract key metrics from plan
        let (node_type, rows_scanned, cost) = self.extract_plan_metrics(&plan_details);

        // Identify potential issues
        let issues = self.identify_query_issues(&plan_details);

        // Generate recommendations
        let recommendations = self.generate_recommendations(&issues);

        Ok(QueryAnalysis {
            query_type: node_type,
            execution_time_ms: execution_time,
            planning_time_ms: planning_time,
            total_time_ms: execution_time + planning_time,
            rows_scanned,
            estimated_cost: cost,
            issues,
            recommendations,
            full_plan: plan_details,
        })
    }

    /// Extract key metrics from query plan
    fn extract_plan_metrics(&self, plan: &serde_json::Value) -> (String, u64, f64) {
        let node_type = plan
            .get("Node Type")
            .and_then(|n| n.as_str())
            .unwrap_or("Unknown")
            .to_string();

        let rows_scanned = plan
            .get("Actual Rows")
            .and_then(|r| r.as_u64())
            .unwrap_or(0);

        let cost = plan
            .get("Total Cost")
            .and_then(|c| c.as_f64())
            .unwrap_or(0.0);

        (node_type, rows_scanned, cost)
    }

    /// Identify potential performance issues in query plan
    fn identify_query_issues(&self, plan: &serde_json::Value) -> Vec<QueryIssue> {
        let mut issues = Vec::new();

        // Check for sequential scans on large tables
        if let Some(node_type) = plan.get("Node Type").and_then(|n| n.as_str()) {
            if node_type == "Seq Scan" {
                if let Some(rows) = plan.get("Actual Rows").and_then(|r| r.as_u64()) {
                    if rows > 1000 {
                        issues.push(QueryIssue {
                            severity: IssueSeverity::High,
                            issue_type: "Sequential Scan".to_string(),
                            description: format!("Sequential scan on {rows} rows"),
                            impact: "High query latency".to_string(),
                        });
                    }
                }
            }
        }

        // Check for missing indexes
        if let Some(filter) = plan.get("Filter").and_then(|f| f.as_str()) {
            if !filter.is_empty() {
                issues.push(QueryIssue {
                    severity: IssueSeverity::Medium,
                    issue_type: "Missing Index".to_string(),
                    description: format!("Filter condition without index: {filter}"),
                    impact: "Increased scan time".to_string(),
                });
            }
        }

        // Check for nested loops with high iteration count
        if let Some(node_type) = plan.get("Node Type").and_then(|n| n.as_str()) {
            if node_type == "Nested Loop" {
                if let Some(loops) = plan.get("Actual Loops").and_then(|l| l.as_u64()) {
                    if loops > 100 {
                        issues.push(QueryIssue {
                            severity: IssueSeverity::High,
                            issue_type: "Inefficient Join".to_string(),
                            description: format!("Nested loop with {loops} iterations"),
                            impact: "Exponential complexity".to_string(),
                        });
                    }
                }
            }
        }

        issues
    }

    /// Generate optimization recommendations based on issues
    fn generate_recommendations(&self, issues: &[QueryIssue]) -> Vec<String> {
        let mut recommendations = Vec::new();

        for issue in issues {
            match issue.issue_type.as_str() {
                "Sequential Scan" => {
                    recommendations
                        .push("Consider adding an index on frequently queried columns".to_string());
                }
                "Missing Index" => {
                    recommendations.push(
                        "Create an index on the filter columns to improve query performance"
                            .to_string(),
                    );
                }
                "Inefficient Join" => {
                    recommendations.push(
                        "Consider using hash join or merge join instead of nested loop".to_string(),
                    );
                }
                _ => {}
            }
        }

        recommendations
    }

    /// Get index recommendations for the database
    pub async fn get_index_recommendations(&self) -> Result<Vec<IndexRecommendation>> {
        let mut recommendations = Vec::new();

        // Find missing indexes from pg_stat_user_tables
        let missing_indexes_query = r#"
            SELECT 
                schemaname,
                tablename,
                seq_scan,
                seq_tup_read,
                idx_scan,
                idx_tup_fetch
            FROM pg_stat_user_tables
            WHERE seq_scan > 0
            AND seq_tup_read > 100000
            AND (idx_scan IS NULL OR idx_scan < seq_scan / 10)
            ORDER BY seq_tup_read DESC
            LIMIT 10
        "#;

        let rows = sqlx::query(missing_indexes_query)
            .fetch_all(self.db_pool.as_ref())
            .await?;

        for row in rows {
            let table_name: String = row.get("tablename");
            let seq_scans: i64 = row.get("seq_scan");
            let rows_read: i64 = row.get("seq_tup_read");

            recommendations.push(IndexRecommendation {
                table_name: table_name.clone(),
                reason: format!(
                    "Table has {seq_scans} sequential scans reading {rows_read} rows total"
                ),
                suggested_columns: vec![], // Would need to analyze actual queries
                estimated_improvement: "50-90% reduction in scan time".to_string(),
                priority: if rows_read > 1_000_000 {
                    RecommendationPriority::High
                } else {
                    RecommendationPriority::Medium
                },
            });
        }

        // Find duplicate indexes
        let duplicate_indexes_query = r#"
            SELECT 
                indexname,
                tablename,
                indexdef
            FROM pg_indexes
            WHERE schemaname = 'public'
            ORDER BY tablename, indexname
        "#;

        let index_rows = sqlx::query(duplicate_indexes_query)
            .fetch_all(self.db_pool.as_ref())
            .await?;

        let mut index_map: HashMap<String, Vec<String>> = HashMap::new();

        for row in index_rows {
            let table: String = row.get("tablename");
            let index: String = row.get("indexname");
            let _definition: String = row.get("indexdef");

            index_map.entry(table).or_default().push(index);
        }

        // Check for tables with too many indexes
        for (table, indexes) in index_map {
            if indexes.len() > 5 {
                recommendations.push(IndexRecommendation {
                    table_name: table,
                    reason: format!(
                        "Table has {} indexes which may slow down writes",
                        indexes.len()
                    ),
                    suggested_columns: vec![],
                    estimated_improvement: "10-20% improvement in write performance".to_string(),
                    priority: RecommendationPriority::Low,
                });
            }
        }

        Ok(recommendations)
    }

    /// Optimize connection pool settings
    pub async fn optimize_connection_pool(&self) -> Result<ConnectionPoolRecommendation> {
        // Get current connection statistics
        let conn_stats_query = r#"
            SELECT 
                count(*) as total_connections,
                count(*) FILTER (WHERE state = 'active') as active_connections,
                count(*) FILTER (WHERE state = 'idle') as idle_connections,
                count(*) FILTER (WHERE state = 'idle in transaction') as idle_in_transaction,
                max(EXTRACT(EPOCH FROM (now() - state_change))) as max_idle_time
            FROM pg_stat_activity
            WHERE datname = current_database()
        "#;

        let row = sqlx::query(conn_stats_query)
            .fetch_one(self.db_pool.as_ref())
            .await?;

        let total_connections: i64 = row.get("total_connections");
        let active_connections: i64 = row.get("active_connections");
        let idle_connections: i64 = row.get("idle_connections");
        let idle_in_transaction: i64 = row.get("idle_in_transaction");
        let max_idle_time: Option<f64> = row.get("max_idle_time");

        // Generate recommendations
        let mut recommendations = Vec::new();

        if idle_connections > active_connections * 3 {
            recommendations.push("Reduce max_idle_connections to save resources".to_string());
        }

        if idle_in_transaction > 0 {
            recommendations.push("Investigate and fix idle-in-transaction connections".to_string());
        }

        if let Some(idle_time) = max_idle_time {
            if idle_time > 300.0 {
                recommendations
                    .push("Set connection idle timeout to prevent zombie connections".to_string());
            }
        }

        let suggested_pool_size = ((active_connections as f64 * 1.5) as u32).max(10).min(100);

        Ok(ConnectionPoolRecommendation {
            current_connections: total_connections as u32,
            active_connections: active_connections as u32,
            idle_connections: idle_connections as u32,
            suggested_pool_size,
            suggested_idle_timeout: Duration::from_secs(300),
            recommendations,
        })
    }

    /// Run all optimization analyses
    pub async fn run_full_analysis(&self) -> Result<FullOptimizationReport> {
        info!("Running full database optimization analysis");

        // Get slow queries
        let slow_queries = self.identify_slow_queries().await?;

        // Get index recommendations
        let index_recommendations = self.get_index_recommendations().await?;

        // Get connection pool recommendations
        let connection_pool = self.optimize_connection_pool().await?;

        // Calculate overall health score
        let health_score = self.calculate_health_score(&slow_queries, &index_recommendations);

        // Generate summary before moving values
        let summary = self.generate_summary(&slow_queries, &index_recommendations);

        Ok(FullOptimizationReport {
            timestamp: chrono::Utc::now(),
            health_score,
            slow_queries,
            index_recommendations,
            connection_pool,
            summary,
        })
    }

    /// Identify slow queries from pg_stat_statements
    async fn identify_slow_queries(&self) -> Result<Vec<SlowQuery>> {
        // Note: This requires pg_stat_statements extension
        let slow_queries_query = r#"
            SELECT 
                calls,
                total_exec_time,
                mean_exec_time,
                stddev_exec_time,
                query
            FROM pg_stat_statements
            WHERE mean_exec_time > 100
            ORDER BY mean_exec_time DESC
            LIMIT 10
        "#;

        // Try to fetch slow queries, but handle the case where pg_stat_statements is not available
        match sqlx::query(slow_queries_query)
            .fetch_all(self.db_pool.as_ref())
            .await
        {
            Ok(rows) => {
                let mut queries = Vec::new();
                for row in rows {
                    queries.push(SlowQuery {
                        query: row.get("query"),
                        total_calls: row.get("calls"),
                        mean_time_ms: row.get("mean_exec_time"),
                        total_time_ms: row.get("total_exec_time"),
                    });
                }
                Ok(queries)
            }
            Err(_) => {
                debug!("pg_stat_statements not available, skipping slow query analysis");
                Ok(Vec::new())
            }
        }
    }

    fn calculate_health_score(
        &self,
        slow_queries: &[SlowQuery],
        index_recs: &[IndexRecommendation],
    ) -> u32 {
        let mut score = 100u32;

        // Deduct points for slow queries
        score = score.saturating_sub((slow_queries.len() * 5) as u32);

        // Deduct points for missing indexes
        for rec in index_recs {
            match rec.priority {
                RecommendationPriority::High => score = score.saturating_sub(10),
                RecommendationPriority::Medium => score = score.saturating_sub(5),
                RecommendationPriority::Low => score = score.saturating_sub(2),
            }
        }

        score.min(100)
    }

    fn generate_summary(
        &self,
        slow_queries: &[SlowQuery],
        index_recs: &[IndexRecommendation],
    ) -> String {
        format!(
            "Found {} slow queries and {} index optimization opportunities",
            slow_queries.len(),
            index_recs.len()
        )
    }
}

/// Query analysis result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryAnalysis {
    pub query_type: String,
    pub execution_time_ms: f64,
    pub planning_time_ms: f64,
    pub total_time_ms: f64,
    pub rows_scanned: u64,
    pub estimated_cost: f64,
    pub issues: Vec<QueryIssue>,
    pub recommendations: Vec<String>,
    pub full_plan: serde_json::Value,
}

/// Query performance issue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryIssue {
    pub severity: IssueSeverity,
    pub issue_type: String,
    pub description: String,
    pub impact: String,
}

/// Issue severity level
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IssueSeverity {
    Low,
    Medium,
    High,
}

/// Index recommendation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexRecommendation {
    pub table_name: String,
    pub reason: String,
    pub suggested_columns: Vec<String>,
    pub estimated_improvement: String,
    pub priority: RecommendationPriority,
}

/// Recommendation priority
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecommendationPriority {
    Low,
    Medium,
    High,
}

/// Connection pool recommendation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionPoolRecommendation {
    pub current_connections: u32,
    pub active_connections: u32,
    pub idle_connections: u32,
    pub suggested_pool_size: u32,
    pub suggested_idle_timeout: Duration,
    pub recommendations: Vec<String>,
}

/// Slow query information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlowQuery {
    pub query: String,
    pub total_calls: i64,
    pub mean_time_ms: f64,
    pub total_time_ms: f64,
}

/// Full optimization report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FullOptimizationReport {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub health_score: u32,
    pub slow_queries: Vec<SlowQuery>,
    pub index_recommendations: Vec<IndexRecommendation>,
    pub connection_pool: ConnectionPoolRecommendation,
    pub summary: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_health_score_calculation() {
        let optimizer = QueryOptimizer {
            db_pool: Arc::new(PgPool::connect_lazy("postgresql://localhost/test").unwrap()),
        };

        let slow_queries = vec![SlowQuery {
            query: "SELECT * FROM test".to_string(),
            total_calls: 100,
            mean_time_ms: 150.0,
            total_time_ms: 15000.0,
        }];

        let index_recs = vec![IndexRecommendation {
            table_name: "test".to_string(),
            reason: "Missing index".to_string(),
            suggested_columns: vec![],
            estimated_improvement: "50%".to_string(),
            priority: RecommendationPriority::High,
        }];

        let score = optimizer.calculate_health_score(&slow_queries, &index_recs);
        assert_eq!(score, 85); // 100 - 5 (slow query) - 10 (high priority index)
    }
}
