#!/bin/bash

# HNSW Vector Index Optimization Verification Script
# Purpose: Verify CODEX-006 optimization implementation
# Usage: ./scripts/verify_hnsw_optimization.sh

set -e

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo "=========================================="
echo -e "${BLUE}CODEX-006: HNSW Optimization Verification${NC}"
echo "=========================================="
echo

# Load environment variables
if [ -f .env ]; then
    export $(cat .env | grep -v '^#' | xargs)
    echo -e "${GREEN}✓${NC} Loaded .env file"
else
    echo -e "${RED}✗${NC} No .env file found!"
    echo "  Please create one: cp .env.example .env"
    exit 1
fi

# Check if DATABASE_URL is set
if [ -z "$DATABASE_URL" ]; then
    echo -e "${RED}✗${NC} DATABASE_URL not set in .env file"
    exit 1
fi

echo -e "${GREEN}✓${NC} DATABASE_URL configured"

# Parse DATABASE_URL
if [[ $DATABASE_URL =~ postgresql://([^:]+):([^@]+)@([^:]+):([^/]+)/(.+) ]]; then
    DB_USER="${BASH_REMATCH[1]}"
    DB_PASS="${BASH_REMATCH[2]}"
    DB_HOST="${BASH_REMATCH[3]}"
    DB_PORT="${BASH_REMATCH[4]}"
    DB_NAME="${BASH_REMATCH[5]}"
else
    echo -e "${RED}✗${NC} Could not parse DATABASE_URL"
    exit 1
fi

echo "Database: $DB_HOST:$DB_PORT/$DB_NAME"
echo

# Test database connectivity first
echo -n "Testing database connection... "
if PGPASSWORD="$DB_PASS" psql -h $DB_HOST -p $DB_PORT -U $DB_USER -d $DB_NAME -c "SELECT 1;" > /dev/null 2>&1; then
    echo -e "${GREEN}✓${NC}"
else
    echo -e "${RED}✗${NC}"
    echo "  Cannot connect to database"
    echo "  Please ensure database is running and accessible"
    exit 1
fi

# Test pgvector extension
echo -n "Checking pgvector extension... "
if PGPASSWORD="$DB_PASS" psql -h $DB_HOST -p $DB_PORT -U $DB_USER -d $DB_NAME -c "SELECT extversion FROM pg_extension WHERE extname='vector';" | grep -q "[0-9]"; then
    echo -e "${GREEN}✓${NC}"
else
    echo -e "${RED}✗${NC}"
    echo "  pgvector extension not found - HNSW optimization requires pgvector"
    exit 1
fi

echo
echo "Running HNSW optimization verification..."
echo

# Execute the verification SQL script
PGPASSWORD="$DB_PASS" psql -h $DB_HOST -p $DB_PORT -U $DB_USER -d $DB_NAME -f scripts/verify_hnsw_optimization.sql

echo
echo "=========================================="
echo -e "${GREEN}HNSW Optimization Verification Complete${NC}"
echo "=========================================="
echo
echo "Next steps if issues found:"
echo "1. Apply migration 011: cargo run -- database migrate"
echo "2. Update postgresql.conf: hnsw.ef_search = 64"
echo "3. Set maintenance_work_mem = '4GB' for index builds"
echo "4. Run performance tests to validate <50ms P99 target"
echo