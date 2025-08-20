#!/bin/bash
# ============================================================================
# Test database connection and verify schema
# ============================================================================

echo "🔍 Testing database connection to .104 server..."
echo ""

# Configuration
DB_HOST="192.168.1.104"
DB_NAME="codex_test"
DB_USER="postgres"

echo "Please enter the postgres password for the .104 server:"
read -s DB_PASSWORD

echo ""
echo "📊 Testing connection..."

# Test basic connection
if PGPASSWORD="$DB_PASSWORD" psql -h "$DB_HOST" -U "$DB_USER" -d "$DB_NAME" -c "SELECT version();" &>/dev/null; then
    echo "✅ Database connection successful!"
else
    echo "❌ Database connection failed!"
    echo "Please check:"
    echo "  - Network connectivity to $DB_HOST"
    echo "  - PostgreSQL is running on port 5432"
    echo "  - Password is correct"
    echo "  - Database '$DB_NAME' exists"
    exit 1
fi

echo ""
echo "📋 Checking memories table schema..."

# Check if last_accessed_at column exists
COLUMN_EXISTS=$(PGPASSWORD="$DB_PASSWORD" psql -h "$DB_HOST" -U "$DB_USER" -d "$DB_NAME" -t -c "
SELECT EXISTS (
    SELECT 1 FROM information_schema.columns 
    WHERE table_name = 'memories' 
    AND column_name = 'last_accessed_at'
);")

if [[ "$COLUMN_EXISTS" == *"t"* ]]; then
    echo "✅ last_accessed_at column exists"
else
    echo "❌ last_accessed_at column missing!"
    echo "Run the migration script first:"
    echo "  sudo -u postgres psql -d codex_test -f /tmp/migration_codex_01.sql"
    exit 1
fi

echo ""
echo "📊 Current table structure:"
PGPASSWORD="$DB_PASSWORD" psql -h "$DB_HOST" -U "$DB_USER" -d "$DB_NAME" -c "\d memories"

echo ""
echo "✅ Database is ready for testing!"
echo ""
echo "Your TEST_DATABASE_URL should be:"
echo "export TEST_DATABASE_URL=\"postgresql://postgres:$DB_PASSWORD@$DB_HOST:5432/$DB_NAME\""