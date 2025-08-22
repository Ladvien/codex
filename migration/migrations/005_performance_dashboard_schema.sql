-- Story 10: Performance Dashboard Schema
-- Migration: 005_performance_dashboard_schema.sql
-- Purpose: Create tables for performance monitoring and alerting

-- Create performance alerts table
CREATE TABLE IF NOT EXISTS performance_alerts (
    id VARCHAR(255) PRIMARY KEY,
    metric_name VARCHAR(255) NOT NULL,
    threshold_type TEXT NOT NULL,
    threshold_value FLOAT NOT NULL,
    current_value FLOAT NOT NULL,
    severity TEXT NOT NULL,
    message TEXT NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    resolved BOOLEAN NOT NULL DEFAULT FALSE,
    resolved_at TIMESTAMPTZ NULL,
    resolved_by VARCHAR(255) NULL
);

-- Create index for faster queries
CREATE INDEX IF NOT EXISTS idx_performance_alerts_timestamp 
    ON performance_alerts(timestamp);
CREATE INDEX IF NOT EXISTS idx_performance_alerts_metric_name 
    ON performance_alerts(metric_name);
CREATE INDEX IF NOT EXISTS idx_performance_alerts_resolved 
    ON performance_alerts(resolved);
CREATE INDEX IF NOT EXISTS idx_performance_alerts_severity 
    ON performance_alerts(severity);

-- Create performance metrics history table
CREATE TABLE IF NOT EXISTS performance_metrics_history (
    id SERIAL PRIMARY KEY,
    metric_name VARCHAR(255) NOT NULL,
    metric_value FLOAT NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    tags JSONB DEFAULT '{}'::jsonb,
    metadata JSONB DEFAULT '{}'::jsonb
);

-- Create index for time-series queries
CREATE INDEX IF NOT EXISTS idx_performance_metrics_timestamp 
    ON performance_metrics_history(timestamp);
CREATE INDEX IF NOT EXISTS idx_performance_metrics_name_timestamp 
    ON performance_metrics_history(metric_name, timestamp);
CREATE INDEX IF NOT EXISTS idx_performance_metrics_tags 
    ON performance_metrics_history USING GIN(tags);

-- Create performance baselines table for regression detection
CREATE TABLE IF NOT EXISTS performance_baselines (
    id SERIAL PRIMARY KEY,
    metric_name VARCHAR(255) NOT NULL,
    baseline_value FLOAT NOT NULL,
    confidence_interval_lower FLOAT,
    confidence_interval_upper FLOAT,
    sample_size INTEGER NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    version VARCHAR(50),
    environment VARCHAR(100) DEFAULT 'production'
);

-- Ensure unique baselines per metric/version/environment
CREATE UNIQUE INDEX IF NOT EXISTS idx_performance_baselines_unique 
    ON performance_baselines(metric_name, version, environment);

-- Create performance test results table
CREATE TABLE IF NOT EXISTS performance_test_results (
    id SERIAL PRIMARY KEY,
    test_name VARCHAR(255) NOT NULL,
    test_type VARCHAR(100) NOT NULL,
    start_time TIMESTAMPTZ NOT NULL,
    end_time TIMESTAMPTZ NOT NULL,
    duration_ms BIGINT NOT NULL,
    passed BOOLEAN NOT NULL,
    metrics JSONB NOT NULL DEFAULT '{}'::jsonb,
    sla_violations JSONB DEFAULT '[]'::jsonb,
    configuration JSONB DEFAULT '{}'::jsonb,
    version VARCHAR(50),
    environment VARCHAR(100) DEFAULT 'test'
);

-- Create indexes for performance test queries
CREATE INDEX IF NOT EXISTS idx_performance_test_results_start_time 
    ON performance_test_results(start_time);
CREATE INDEX IF NOT EXISTS idx_performance_test_results_test_name 
    ON performance_test_results(test_name);
CREATE INDEX IF NOT EXISTS idx_performance_test_results_passed 
    ON performance_test_results(passed);
CREATE INDEX IF NOT EXISTS idx_performance_test_results_metrics 
    ON performance_test_results USING GIN(metrics);

-- Create performance dashboard configuration table
CREATE TABLE IF NOT EXISTS performance_dashboard_config (
    id SERIAL PRIMARY KEY,
    config_key VARCHAR(255) NOT NULL UNIQUE,
    config_value JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Insert default dashboard configuration
INSERT INTO performance_dashboard_config (config_key, config_value) VALUES
('alert_thresholds', '{
    "p95_latency_ms": {
        "warning_threshold": 1500.0,
        "critical_threshold": 2000.0,
        "threshold_type": "GreaterThan",
        "enabled": true
    },
    "memory_headroom_percent": {
        "warning_threshold": 25.0,
        "critical_threshold": 20.0,
        "threshold_type": "LessThan",
        "enabled": true
    },
    "token_reduction_percent": {
        "warning_threshold": 85.0,
        "critical_threshold": 90.0,
        "threshold_type": "LessThan",
        "enabled": true
    },
    "connection_pool_usage": {
        "warning_threshold": 70.0,
        "critical_threshold": 85.0,
        "threshold_type": "GreaterThan",
        "enabled": true
    },
    "batch_throughput_regression": {
        "warning_threshold": 15.0,
        "critical_threshold": 25.0,
        "threshold_type": "PercentageDecrease",
        "enabled": true
    }
}'::jsonb),
('performance_targets', '{
    "p95_latency_ms": 2000.0,
    "token_reduction_percent": 90.0,
    "memory_headroom_percent": 20.0,
    "batch_throughput_ops_sec": 1000.0,
    "cache_hit_ratio": 0.9,
    "connection_pool_usage": 0.7
}'::jsonb),
('dashboard_settings', '{
    "monitoring_interval_seconds": 60,
    "retention_days": 30,
    "enable_auto_scaling": true
}'::jsonb)
ON CONFLICT (config_key) DO NOTHING;

-- Create function to automatically update updated_at timestamps
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ language 'plpgsql';

-- Create triggers for updated_at columns
DROP TRIGGER IF EXISTS update_performance_baselines_updated_at ON performance_baselines;
CREATE TRIGGER update_performance_baselines_updated_at 
    BEFORE UPDATE ON performance_baselines 
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

DROP TRIGGER IF EXISTS update_dashboard_config_updated_at ON performance_dashboard_config;
CREATE TRIGGER update_dashboard_config_updated_at 
    BEFORE UPDATE ON performance_dashboard_config 
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- Create views for common dashboard queries

-- Current performance status view
CREATE OR REPLACE VIEW performance_status AS
SELECT 
    metric_name,
    AVG(metric_value) as avg_value,
    MIN(metric_value) as min_value,
    MAX(metric_value) as max_value,
    COUNT(*) as sample_count,
    MAX(timestamp) as last_updated
FROM performance_metrics_history 
WHERE timestamp > NOW() - INTERVAL '1 hour'
GROUP BY metric_name;

-- Active alerts view
CREATE OR REPLACE VIEW active_performance_alerts AS
SELECT 
    id,
    metric_name,
    severity,
    current_value,
    threshold_value,
    message,
    timestamp,
    EXTRACT(EPOCH FROM (NOW() - timestamp))/60 as age_minutes
FROM performance_alerts 
WHERE NOT resolved
ORDER BY 
    CASE severity 
        WHEN '"Critical"' THEN 1 
        WHEN '"Warning"' THEN 2 
        ELSE 3 
    END,
    timestamp DESC;

-- Story 10 compliance view
CREATE OR REPLACE VIEW story10_compliance AS
WITH latest_metrics AS (
    SELECT DISTINCT ON (metric_name) 
        metric_name, 
        metric_value,
        timestamp
    FROM performance_metrics_history 
    WHERE timestamp > NOW() - INTERVAL '5 minutes'
    ORDER BY metric_name, timestamp DESC
)
SELECT 
    CASE 
        WHEN p95.metric_value < 2000 THEN true 
        ELSE false 
    END as p95_latency_compliant,
    CASE 
        WHEN tr.metric_value >= 90 THEN true 
        ELSE false 
    END as token_reduction_compliant,
    CASE 
        WHEN mh.metric_value >= 20 THEN true 
        ELSE false 
    END as memory_headroom_compliant,
    p95.metric_value as current_p95_latency_ms,
    tr.metric_value as current_token_reduction_percent,
    mh.metric_value as current_memory_headroom_percent,
    NOW() as last_checked
FROM 
    latest_metrics p95
    FULL OUTER JOIN latest_metrics tr ON tr.metric_name = 'token_reduction_percent'
    FULL OUTER JOIN latest_metrics mh ON mh.metric_name = 'memory_headroom_percent'
WHERE p95.metric_name = 'p95_latency_ms';

-- Performance trend analysis view
CREATE OR REPLACE VIEW performance_trends AS
SELECT 
    metric_name,
    DATE_TRUNC('hour', timestamp) as hour,
    AVG(metric_value) as avg_value,
    MIN(metric_value) as min_value,
    MAX(metric_value) as max_value,
    COUNT(*) as sample_count
FROM performance_metrics_history 
WHERE timestamp > NOW() - INTERVAL '24 hours'
GROUP BY metric_name, DATE_TRUNC('hour', timestamp)
ORDER BY metric_name, hour;

-- Grant permissions to application user
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'codex_memory_app') THEN
        GRANT SELECT, INSERT, UPDATE, DELETE ON performance_alerts TO codex_memory_app;
        GRANT SELECT, INSERT ON performance_metrics_history TO codex_memory_app;
        GRANT SELECT, INSERT, UPDATE ON performance_baselines TO codex_memory_app;
        GRANT SELECT, INSERT ON performance_test_results TO codex_memory_app;
        GRANT SELECT, UPDATE ON performance_dashboard_config TO codex_memory_app;
        GRANT SELECT ON performance_status TO codex_memory_app;
        GRANT SELECT ON active_performance_alerts TO codex_memory_app;
        GRANT SELECT ON story10_compliance TO codex_memory_app;
        GRANT SELECT ON performance_trends TO codex_memory_app;
        GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO codex_memory_app;
    END IF;
END
$$;