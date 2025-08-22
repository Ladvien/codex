-- Rollback script for Story 10: Performance Dashboard Schema
-- Migration: 005_performance_dashboard_schema_rollback.sql

-- Drop views first (due to dependencies)
DROP VIEW IF EXISTS performance_trends;
DROP VIEW IF EXISTS story10_compliance;
DROP VIEW IF EXISTS active_performance_alerts;
DROP VIEW IF EXISTS performance_status;

-- Drop triggers
DROP TRIGGER IF EXISTS update_dashboard_config_updated_at ON performance_dashboard_config;
DROP TRIGGER IF EXISTS update_performance_baselines_updated_at ON performance_baselines;

-- Drop function
DROP FUNCTION IF EXISTS update_updated_at_column();

-- Drop tables
DROP TABLE IF EXISTS performance_dashboard_config;
DROP TABLE IF EXISTS performance_test_results;
DROP TABLE IF EXISTS performance_baselines;
DROP TABLE IF EXISTS performance_metrics_history;
DROP TABLE IF EXISTS performance_alerts;