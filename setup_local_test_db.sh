#!/bin/bash
# ============================================================================
# Setup local test database as alternative to remote connection
# ============================================================================

echo "🚀 Setting up local test database..."
echo ""

LOCAL_DB_NAME="codex_test_local"

# Check if PostgreSQL is running locally
if ! pgrep -x "postgres" > /dev/null; then
    echo "📦 Starting PostgreSQL locally..."
    brew services start postgresql
    sleep 3
fi

echo "📦 Creating local test database..."
createdb "$LOCAL_DB_NAME" 2>/dev/null || echo "Database may already exist"

echo "🔧 Applying migration to local database..."
if [ -f "migration/fix_test_database.sql" ]; then
    psql -d "$LOCAL_DB_NAME" -f "migration/fix_test_database.sql"
else
    echo "❌ Migration file not found at migration/fix_test_database.sql"
    echo "Let me check for alternative locations..."
    find . -name "*migration*.sql" -type f
    exit 1
fi

echo ""
echo "✅ Local test database setup complete!"
echo ""
echo "To use the local database for testing, set:"
echo "export TEST_DATABASE_URL=\"postgresql://localhost:5432/$LOCAL_DB_NAME\""
echo ""
echo "📊 Verifying local database structure..."
psql -d "$LOCAL_DB_NAME" -c "\d memories" | head -20