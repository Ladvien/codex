#!/bin/bash
# Codex Dreams E2E Test Runner
# Executes the complete test suite with proper environment setup

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
TEST_DB_NAME="${TEST_DB_NAME:-codex_test}"
POSTGRES_USER="${POSTGRES_USER:-postgres}"
POSTGRES_HOST="${POSTGRES_HOST:-localhost}"
POSTGRES_PORT="${POSTGRES_PORT:-5432}"

# Test database URL
export TEST_DATABASE_URL="postgresql://$POSTGRES_USER:$POSTGRES_USER@$POSTGRES_HOST:$POSTGRES_PORT/$TEST_DB_NAME"
export CODEX_DREAMS_ENABLED=true

echo_header() {
    echo -e "${BLUE}============================================${NC}"
    echo -e "${BLUE}$1${NC}"
    echo -e "${BLUE}============================================${NC}"
}

echo_step() {
    echo -e "${YELLOW}▶ $1${NC}"
}

echo_success() {
    echo -e "${GREEN}✅ $1${NC}"
}

echo_error() {
    echo -e "${RED}❌ $1${NC}"
}

# Function to check if PostgreSQL is running
check_postgres() {
    echo_step "Checking PostgreSQL connection..."
    if ! pg_isready -h "$POSTGRES_HOST" -p "$POSTGRES_PORT" -U "$POSTGRES_USER" >/dev/null 2>&1; then
        echo_error "PostgreSQL is not running or not accessible"
        echo "Please ensure PostgreSQL is running on $POSTGRES_HOST:$POSTGRES_PORT"
        exit 1
    fi
    echo_success "PostgreSQL is running"
}

# Function to setup test database
setup_test_db() {
    echo_step "Setting up test database..."
    
    # Drop and recreate test database
    psql -h "$POSTGRES_HOST" -p "$POSTGRES_PORT" -U "$POSTGRES_USER" -d postgres -c "DROP DATABASE IF EXISTS $TEST_DB_NAME;" >/dev/null 2>&1 || true
    psql -h "$POSTGRES_HOST" -p "$POSTGRES_PORT" -U "$POSTGRES_USER" -d postgres -c "CREATE DATABASE $TEST_DB_NAME;" >/dev/null 2>&1
    
    # Install pgvector extension
    psql -h "$POSTGRES_HOST" -p "$POSTGRES_PORT" -U "$POSTGRES_USER" -d "$TEST_DB_NAME" -c "CREATE EXTENSION IF NOT EXISTS vector;" >/dev/null 2>&1
    
    echo_success "Test database created with pgvector extension"
}

# Function to run cargo checks
run_checks() {
    echo_step "Running cargo checks..."
    cd "$PROJECT_ROOT"
    
    # Check if codex-dreams feature compiles
    if ! cargo check --features codex-dreams >/dev/null 2>&1; then
        echo_error "Code does not compile with codex-dreams feature"
        return 1
    fi
    echo_success "Code compiles successfully"
    
    # Run clippy on E2E tests
    if ! cargo clippy --features codex-dreams --tests >/dev/null 2>&1; then
        echo_error "Clippy warnings in E2E tests"
        return 1
    fi
    echo_success "No clippy warnings"
}

# Function to run specific test category
run_test_category() {
    local category="$1"
    local description="$2"
    
    echo_step "Running $description tests..."
    cd "$PROJECT_ROOT"
    
    if cargo test --features codex-dreams "$category" --nocapture; then
        echo_success "$description tests passed"
        return 0
    else
        echo_error "$description tests failed"
        return 1
    fi
}

# Main test execution
main() {
    echo_header "Codex Dreams E2E Test Suite"
    
    # Pre-flight checks
    check_postgres
    setup_test_db
    run_checks
    
    local failed_tests=()
    
    echo_header "Executing Test Categories"
    
    # Database migrations
    if ! run_test_category "e2e_codex_dreams_migrations" "Database Migration"; then
        failed_tests+=("migrations")
    fi
    
    # Ollama integration
    if ! run_test_category "e2e_ollama_integration" "Ollama Integration"; then
        failed_tests+=("ollama")
    fi
    
    # MCP commands
    if ! run_test_category "e2e_mcp_insights_commands" "MCP Command Interface"; then
        failed_tests+=("mcp")
    fi
    
    # Scheduler and performance
    if ! run_test_category "e2e_scheduler_performance" "Scheduler & Performance"; then
        failed_tests+=("scheduler")
    fi
    
    # Full pipeline integration
    if ! run_test_category "e2e_codex_dreams_full" "Full Pipeline Integration"; then
        failed_tests+=("pipeline")
    fi
    
    # Summary
    echo_header "Test Results Summary"
    
    if [ ${#failed_tests[@]} -eq 0 ]; then
        echo_success "All E2E tests passed! 🎉"
        echo ""
        echo "Test Coverage:"
        echo "  ✅ Database migrations and schema validation"
        echo "  ✅ Ollama integration with security and resilience"
        echo "  ✅ MCP command interface and parameter validation"
        echo "  ✅ Background scheduling and performance monitoring"
        echo "  ✅ Complete insight generation pipeline"
        echo ""
        echo "The Codex Dreams feature is ready for production deployment."
        return 0
    else
        echo_error "Some tests failed: ${failed_tests[*]}"
        echo ""
        echo "Please review the test output above and fix any issues."
        echo "Check the troubleshooting section in docs/testing/codex-dreams-e2e-tests.md"
        return 1
    fi
}

# Cleanup function
cleanup() {
    echo_step "Cleaning up test database..."
    psql -h "$POSTGRES_HOST" -p "$POSTGRES_PORT" -U "$POSTGRES_USER" -d postgres -c "DROP DATABASE IF EXISTS $TEST_DB_NAME;" >/dev/null 2>&1 || true
    echo_success "Cleanup completed"
}

# Set trap for cleanup
trap cleanup EXIT

# Run main function
main "$@"