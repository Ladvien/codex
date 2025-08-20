#!/bin/bash
# ============================================================================
# Ubuntu-specific script to apply SQL migration to test database on .104
# This handles the permission issues when running as postgres user
# ============================================================================

echo "🚀 Applying database migration on Ubuntu server (.104)..."
echo ""

# Configuration
DB_HOST="${DB_HOST:-localhost}"
DB_NAME="${DB_NAME:-codex_test}"
MIGRATION_FILE="${1:-migration_codex_01.sql}"

# Check if migration file exists
if [ ! -f "$MIGRATION_FILE" ]; then
    echo "❌ Error: Migration file not found: $MIGRATION_FILE"
    echo "   Make sure you're in the directory containing the migration file"
    exit 1
fi

echo "📋 Migration Options:"
echo ""
echo "Option 1: Copy to /tmp and apply (RECOMMENDED)"
echo "----------------------------------------"
echo "sudo cp $MIGRATION_FILE /tmp/"
echo "sudo chmod 644 /tmp/$MIGRATION_FILE"
echo "sudo -u postgres psql -d $DB_NAME -f /tmp/$MIGRATION_FILE"
echo ""

echo "Option 2: Use cat to pipe the SQL"
echo "----------------------------------------"
echo "cat $MIGRATION_FILE | sudo -u postgres psql -d $DB_NAME"
echo ""

echo "Option 3: Change file permissions (less secure)"
echo "----------------------------------------"
echo "chmod 644 $MIGRATION_FILE"
echo "sudo -u postgres psql -d $DB_NAME -f $MIGRATION_FILE"
echo ""

echo "Option 4: Run as your user with connection string"
echo "----------------------------------------"
echo "psql postgresql://postgres@localhost/$DB_NAME -f $MIGRATION_FILE"
echo ""

echo "🔧 Let's use Option 1 (most reliable)..."
echo ""

# Copy to /tmp with proper permissions
echo "📦 Copying migration file to /tmp..."
sudo cp "$MIGRATION_FILE" "/tmp/$MIGRATION_FILE"
sudo chmod 644 "/tmp/$MIGRATION_FILE"

# Ensure database exists
echo "📦 Ensuring database exists..."
sudo -u postgres psql -tc "SELECT 1 FROM pg_database WHERE datname = '$DB_NAME'" | grep -q 1 || {
    echo "   Creating database $DB_NAME..."
    sudo -u postgres psql -c "CREATE DATABASE $DB_NAME"
}

# Apply the migration
echo "🔧 Applying migration..."
sudo -u postgres psql -d "$DB_NAME" -f "/tmp/$MIGRATION_FILE"

if [ $? -eq 0 ]; then
    echo "✅ Migration applied successfully!"
    
    # Clean up temp file
    sudo rm "/tmp/$MIGRATION_FILE"
    
    # Show summary
    echo ""
    echo "📊 Verifying database structure..."
    sudo -u postgres psql -d "$DB_NAME" -c "\dt" 2>/dev/null
    
    echo ""
    echo "📋 Checking memories table columns..."
    sudo -u postgres psql -d "$DB_NAME" -c "SELECT column_name, data_type FROM information_schema.columns WHERE table_name = 'memories' ORDER BY ordinal_position;" 2>/dev/null
    
    echo ""
    echo "✅ Database migration complete!"
    echo ""
    echo "Next steps:"
    echo "1. Update TEST_DATABASE_URL in your local environment:"
    echo "   export TEST_DATABASE_URL=\"postgresql://postgres:your_password@192.168.1.104:5432/codex_test\""
    echo "2. Run the test suite from your local machine"
else
    echo "❌ Migration failed!"
    echo "Check the error messages above and try one of the other options manually."
    exit 1
fi