#!/bin/bash
# ============================================================================
# Simple connection test with better error reporting
# ============================================================================

echo "🔍 Testing PostgreSQL connection to .104..."
echo ""
echo "This will test the connection and show detailed error messages."
echo ""

DB_HOST="192.168.1.104"
DB_NAME="codex_test"
DB_USER="postgres"

echo "Attempting connection as: $DB_USER@$DB_HOST:5432/$DB_NAME"
echo ""

# Try connection with verbose error reporting
psql -h "$DB_HOST" -U "$DB_USER" -d "$DB_NAME" -c "SELECT 'Connection successful!' as status, version();"

echo ""
echo "If the connection failed, common causes are:"
echo "1. Wrong password"
echo "2. Database 'codex_test' doesn't exist"
echo "3. User 'postgres' doesn't exist or needs different authentication"
echo "4. pg_hba.conf doesn't allow connections from your IP"
echo ""
echo "To check if the database exists, try connecting to 'postgres' database first:"
echo "psql -h $DB_HOST -U $DB_USER -d postgres -c \"\\l\""