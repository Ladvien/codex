#!/bin/bash
# ============================================================================
# Debug database connection issues
# ============================================================================

echo "🔍 Debugging connection to .104 server..."
echo ""

DB_HOST="192.168.1.104"
DB_PORT="5432"

echo "1. Testing network connectivity..."
if ping -c 3 "$DB_HOST" &>/dev/null; then
    echo "✅ Can ping $DB_HOST"
else
    echo "❌ Cannot ping $DB_HOST"
    echo "Check if the server is on the network and accessible"
fi

echo ""
echo "2. Testing if PostgreSQL port is open..."
if nc -z "$DB_HOST" "$DB_PORT" 2>/dev/null; then
    echo "✅ Port $DB_PORT is open on $DB_HOST"
else
    echo "❌ Port $DB_PORT is not accessible on $DB_HOST"
    echo "Possible issues:"
    echo "  - PostgreSQL is not running"
    echo "  - PostgreSQL is not listening on external interfaces"
    echo "  - Firewall is blocking the connection"
fi

echo ""
echo "3. Checking if psql is installed locally..."
if command -v psql &>/dev/null; then
    echo "✅ psql is available"
    psql --version
else
    echo "❌ psql is not installed"
    echo "Install with: brew install postgresql"
fi

echo ""
echo "📋 Common PostgreSQL configuration fixes needed on the server:"
echo ""
echo "A. Edit /etc/postgresql/*/main/postgresql.conf:"
echo "   listen_addresses = '*'  # or '0.0.0.0' to accept external connections"
echo ""
echo "B. Edit /etc/postgresql/*/main/pg_hba.conf:"
echo "   Add line: host all postgres 192.168.1.0/24 md5"
echo ""
echo "C. Restart PostgreSQL:"
echo "   sudo systemctl restart postgresql"
echo ""
echo "D. Check if PostgreSQL is running:"
echo "   sudo systemctl status postgresql"