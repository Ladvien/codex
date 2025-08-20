#!/bin/bash
# ============================================================================
# Script to run tests with proper configuration
# ============================================================================

echo "🚀 Setting up test environment..."

# Database configuration for test server at .104
# IMPORTANT: Replace 'your_actual_password' with the real postgres password
export TEST_DATABASE_URL="postgresql://postgres:your_actual_password@192.168.1.104:5432/codex_test"

# Skip strict validations for testing
export SKIP_DUPLICATE_CHECK=true
export SKIP_XSS_CHECK=true

# Optional: Configure test-specific settings
export RUST_LOG=info
export RUST_BACKTRACE=1

# Verify environment variables are set
echo "🔍 Environment variables:"
echo "  TEST_DATABASE_URL: $TEST_DATABASE_URL"
echo "  SKIP_DUPLICATE_CHECK: $SKIP_DUPLICATE_CHECK"
echo "  SKIP_XSS_CHECK: $SKIP_XSS_CHECK"
echo ""

# Create test database if needed (you'll need to provide the password)
echo "📦 Ensuring test database exists..."
echo "If prompted, enter the PostgreSQL password for the postgres user on .104"

# You can uncomment and run this if you have psql installed locally:
# psql -h 192.168.1.104 -U postgres -c "CREATE DATABASE IF NOT EXISTS codex_test;"

echo "🔧 Running database migrations..."
echo "Please run the SQL migration script on your test database first:"
echo "  psql -h 192.168.1.104 -U postgres -d codex_test -f migration/fix_test_database.sql"
echo ""
echo "Press Enter when migrations are complete..."
read

echo "🧪 Running tests..."

# Run tests with proper flags
if [ "$1" == "--no-fail-fast" ]; then
    echo "Running all tests (no fail fast)..."
    cargo test --no-fail-fast
elif [ "$1" == "--unit" ]; then
    echo "Running unit tests only..."
    cargo test --lib
elif [ "$1" == "--integration" ]; then
    echo "Running integration tests only..."
    cargo test --test '*integration*'
elif [ "$1" == "--e2e" ]; then
    echo "Running e2e tests only..."
    cargo test --test '*e2e*'
elif [ "$1" == "--phase4" ]; then
    echo "Running Phase 4 tests only..."
    cargo test --test property_based_testing
    cargo test --test advanced_concurrency_testing
    cargo test --test chaos_engineering
    cargo test --test test_infrastructure_improvements
else
    echo "Running all tests..."
    cargo test
fi

echo "✅ Test run complete!"