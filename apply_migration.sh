#!/bin/bash
# ============================================================================
# Shell script to apply the SQL migration to the test database
# ============================================================================

# Configuration
DB_HOST="${DB_HOST:-localhost}"
DB_PORT="${DB_PORT:-5432}"
DB_USER="${DB_USER:-postgres}"
DB_NAME="${DB_NAME:-codex_test}"
MIGRATION_FILE="${1:-migration/fix_test_database.sql}"

echo "🚀 Applying database migration..."
echo "   Host: $DB_HOST:$DB_PORT"
echo "   Database: $DB_NAME"
echo "   User: $DB_USER"
echo "   Migration file: $MIGRATION_FILE"
echo ""

# Check if migration file exists
if [ ! -f "$MIGRATION_FILE" ]; then
    echo "❌ Error: Migration file not found: $MIGRATION_FILE"
    exit 1
fi

# Check if psql is installed
if ! command -v psql &> /dev/null; then
    echo "❌ Error: psql command not found. Please install PostgreSQL client."
    echo "   On Ubuntu/Debian: sudo apt-get install postgresql-client"
    echo "   On macOS: brew install postgresql"
    exit 1
fi

# Create database if it doesn't exist
echo "📦 Ensuring database exists..."
psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -tc "SELECT 1 FROM pg_database WHERE datname = '$DB_NAME'" | grep -q 1 || {
    echo "   Creating database $DB_NAME..."
    psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -c "CREATE DATABASE $DB_NAME"
}

# Apply the migration
echo "🔧 Applying migration..."
psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -f "$MIGRATION_FILE"

if [ $? -eq 0 ]; then
    echo "✅ Migration applied successfully!"
    
    # Show summary of tables
    echo ""
    echo "📊 Database tables:"
    psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -c "\dt"
    
    # Show memories table structure
    echo ""
    echo "📋 Memories table structure:"
    psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -c "\d memories" 2>/dev/null || echo "Note: memories table details not available"
else
    echo "❌ Migration failed!"
    exit 1
fi