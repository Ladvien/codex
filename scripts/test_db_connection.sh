#!/bin/bash

# Test Database Connection Script
# This script verifies that tests can connect to the PostgreSQL database

set -e

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo "==================================="
echo "   Database Connection Test"
echo "==================================="
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

echo -e "${GREEN}✓${NC} DATABASE_URL is set"

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

echo
echo "Database Configuration:"
echo "  Host: $DB_HOST"
echo "  Port: $DB_PORT"
echo "  Database: $DB_NAME"
echo "  User: $DB_USER"
echo

# Test network connectivity
echo -n "Testing network connectivity... "
if ping -c 1 -W 2 $DB_HOST > /dev/null 2>&1; then
    echo -e "${GREEN}✓${NC}"
else
    echo -e "${RED}✗${NC}"
    echo "  Cannot reach host $DB_HOST"
    echo "  Check your network connection or VPN"
    exit 1
fi

# Test PostgreSQL port
echo -n "Testing PostgreSQL port... "
if nc -z -w2 $DB_HOST $DB_PORT 2>/dev/null; then
    echo -e "${GREEN}✓${NC}"
else
    echo -e "${RED}✗${NC}"
    echo "  Port $DB_PORT is not open on $DB_HOST"
    echo "  Check if PostgreSQL is running"
    exit 1
fi

# Test database connection
echo -n "Testing database connection... "
if PGPASSWORD="$DB_PASS" psql -h $DB_HOST -p $DB_PORT -U $DB_USER -d $DB_NAME -c "SELECT 1;" > /dev/null 2>&1; then
    echo -e "${GREEN}✓${NC}"
else
    echo -e "${RED}✗${NC}"
    echo "  Cannot connect to database"
    echo "  Check credentials in .env file"
    exit 1
fi

# Test pgvector extension
echo -n "Testing pgvector extension... "
if PGPASSWORD="$DB_PASS" psql -h $DB_HOST -p $DB_PORT -U $DB_USER -d $DB_NAME -c "SELECT extversion FROM pg_extension WHERE extname='vector';" | grep -q "[0-9]"; then
    echo -e "${GREEN}✓${NC}"
else
    echo -e "${YELLOW}⚠${NC}"
    echo "  pgvector extension not found"
    echo "  Run: CREATE EXTENSION IF NOT EXISTS vector;"
fi

# Test memories table
echo -n "Testing memories table... "
if PGPASSWORD="$DB_PASS" psql -h $DB_HOST -p $DB_PORT -U $DB_USER -d $DB_NAME -c "\dt memories" | grep -q "memories"; then
    echo -e "${GREEN}✓${NC}"
    
    # Get memory count
    COUNT=$(PGPASSWORD="$DB_PASS" psql -h $DB_HOST -p $DB_PORT -U $DB_USER -d $DB_NAME -t -c "SELECT COUNT(*) FROM memories WHERE deleted_at IS NULL;" | tr -d ' ')
    echo "  Current memories in database: $COUNT"
else
    echo -e "${YELLOW}⚠${NC}"
    echo "  memories table not found"
    echo "  Run: cargo run -- database migrate"
fi

echo
echo "==================================="
echo -e "${GREEN}Database connection successful!${NC}"
echo "==================================="
echo
echo "You can now run tests with:"
echo "  cargo test --all"
echo
echo "For specific tests:"
echo "  cargo test --lib tier_manager::tests::test_tier_manager_creation"
echo