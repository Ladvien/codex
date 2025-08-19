#!/bin/bash

# Agentic Memory System - MCP Server Manager
# Provides comprehensive management for the MCP server

set -e

# Configuration
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
cd "$SCRIPT_DIR"

# Files and paths
PID_FILE="$SCRIPT_DIR/.mcp-server.pid"
LOG_FILE="$SCRIPT_DIR/mcp-server.log"
ENV_FILE="$SCRIPT_DIR/.env"
BINARY="$SCRIPT_DIR/target/release/codex-memory"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
PURPLE='\033[0;35m'
NC='\033[0m' # No Color

# Check if .env exists
check_env() {
    if [ ! -f "$ENV_FILE" ]; then
        echo -e "${RED}❌ Error: .env file not found${NC}"
        echo "   Run: ./setup_mcp.sh first"
        exit 1
    fi
    source "$ENV_FILE"
}

# Check if binary exists
check_binary() {
    if [ ! -f "$BINARY" ]; then
        echo -e "${RED}❌ Error: Binary not found at $BINARY${NC}"
        echo "   Run: cargo build --release"
        exit 1
    fi
}

# Get server PID if running
get_pid() {
    if [ -f "$PID_FILE" ]; then
        PID=$(cat "$PID_FILE")
        if ps -p "$PID" > /dev/null 2>&1; then
            echo "$PID"
        else
            # Process not running, clean up PID file
            rm -f "$PID_FILE"
            echo ""
        fi
    else
        echo ""
    fi
}

# Start the server
start_server() {
    echo -e "${BLUE}🚀 Starting Agentic Memory System MCP Server...${NC}"
    
    check_env
    check_binary
    
    PID=$(get_pid)
    if [ -n "$PID" ]; then
        echo -e "${YELLOW}⚠️  Server is already running (PID: $PID)${NC}"
        echo "   Use '$0 restart' to restart the server"
        exit 1
    fi
    
    # Validate configuration first
    echo "Validating configuration..."
    if ! "$BINARY" mcp validate > /dev/null 2>&1; then
        echo -e "${RED}❌ Configuration validation failed${NC}"
        echo "   Run: $BINARY mcp validate"
        exit 1
    fi
    
    # Start server in background
    echo "Starting server..."
    nohup "$BINARY" start >> "$LOG_FILE" 2>&1 &
    PID=$!
    echo "$PID" > "$PID_FILE"
    
    # Wait a moment to check if it started successfully
    sleep 2
    
    if ps -p "$PID" > /dev/null 2>&1; then
        echo -e "${GREEN}✅ Server started successfully (PID: $PID)${NC}"
        echo "   HTTP Port: $HTTP_PORT"
        echo "   MCP Port: $MCP_PORT"
        echo "   Log file: $LOG_FILE"
    else
        echo -e "${RED}❌ Server failed to start${NC}"
        echo "   Check logs: tail -f $LOG_FILE"
        rm -f "$PID_FILE"
        exit 1
    fi
}

# Stop the server
stop_server() {
    echo -e "${BLUE}🛑 Stopping Agentic Memory System MCP Server...${NC}"
    
    PID=$(get_pid)
    if [ -z "$PID" ]; then
        echo -e "${YELLOW}⚠️  Server is not running${NC}"
        exit 0
    fi
    
    echo "Stopping server (PID: $PID)..."
    kill "$PID" 2>/dev/null || true
    
    # Wait for graceful shutdown (max 10 seconds)
    COUNTER=0
    while ps -p "$PID" > /dev/null 2>&1; do
        if [ $COUNTER -ge 10 ]; then
            echo "Force killing server..."
            kill -9 "$PID" 2>/dev/null || true
            break
        fi
        sleep 1
        COUNTER=$((COUNTER + 1))
    done
    
    rm -f "$PID_FILE"
    echo -e "${GREEN}✅ Server stopped${NC}"
}

# Restart the server
restart_server() {
    echo -e "${BLUE}🔄 Restarting Agentic Memory System MCP Server...${NC}"
    stop_server
    sleep 1
    start_server
}

# Show server status
show_status() {
    echo -e "${BLUE}📊 Agentic Memory System MCP Server Status${NC}"
    echo "========================================="
    
    PID=$(get_pid)
    if [ -n "$PID" ]; then
        echo -e "${GREEN}● Server is running${NC}"
        echo "  PID: $PID"
        
        # Get process info
        if command -v ps > /dev/null; then
            echo ""
            echo "Process Information:"
            ps -p "$PID" -o pid,ppid,%cpu,%mem,etime,command | tail -n +1
        fi
        
        # Check ports
        check_env
        echo ""
        echo "Network Status:"
        if command -v lsof > /dev/null; then
            echo "  HTTP Port ($HTTP_PORT):"
            lsof -i :$HTTP_PORT 2>/dev/null | grep LISTEN || echo "    Not listening"
            echo "  MCP Port ($MCP_PORT):"
            lsof -i :$MCP_PORT 2>/dev/null | grep LISTEN || echo "    Not listening"
        else
            echo "  HTTP Port: $HTTP_PORT (configured)"
            echo "  MCP Port: $MCP_PORT (configured)"
        fi
        
        # Recent logs
        echo ""
        echo "Recent Log Entries:"
        tail -n 5 "$LOG_FILE" 2>/dev/null | sed 's/^/  /'
    else
        echo -e "${RED}○ Server is not running${NC}"
    fi
    
    echo ""
    echo "Configuration:"
    if [ -f "$ENV_FILE" ]; then
        check_env
        echo "  Database: ${DATABASE_URL%@*}@***"
        echo "  Embedding: $EMBEDDING_PROVIDER ($EMBEDDING_MODEL)"
        echo "  Log Level: $LOG_LEVEL"
    else
        echo -e "  ${RED}No .env file found${NC}"
    fi
    
    echo ""
    echo "Files:"
    echo "  PID File: $PID_FILE"
    echo "  Log File: $LOG_FILE"
    echo "  Binary: $BINARY"
}

# Show logs
show_logs() {
    if [ ! -f "$LOG_FILE" ]; then
        echo -e "${YELLOW}⚠️  No log file found${NC}"
        exit 0
    fi
    
    case "${2:-tail}" in
        follow|f|-f|--follow)
            echo -e "${BLUE}📜 Following logs (Ctrl+C to stop)...${NC}"
            tail -f "$LOG_FILE"
            ;;
        all|--all|-a)
            echo -e "${BLUE}📜 All logs:${NC}"
            cat "$LOG_FILE"
            ;;
        clear|--clear)
            echo -e "${BLUE}🗑️  Clearing logs...${NC}"
            > "$LOG_FILE"
            echo -e "${GREEN}✅ Logs cleared${NC}"
            ;;
        *)
            LINES="${2:-50}"
            echo -e "${BLUE}📜 Last $LINES lines of logs:${NC}"
            tail -n "$LINES" "$LOG_FILE"
            ;;
    esac
}

# Run health check
health_check() {
    echo -e "${BLUE}🩺 Running Health Check...${NC}"
    echo "========================="
    
    check_env
    check_binary
    
    # Check if server is running
    PID=$(get_pid)
    if [ -n "$PID" ]; then
        echo -e "${GREEN}✅ Server Process: Running (PID: $PID)${NC}"
    else
        echo -e "${YELLOW}⚠️  Server Process: Not running${NC}"
    fi
    
    # Test configuration
    echo ""
    echo "Testing configuration..."
    if "$BINARY" mcp validate > /dev/null 2>&1; then
        echo -e "${GREEN}✅ Configuration: Valid${NC}"
    else
        echo -e "${RED}❌ Configuration: Invalid${NC}"
        "$BINARY" mcp validate 2>&1 | sed 's/^/  /'
    fi
    
    # Test connectivity
    echo ""
    echo "Testing connectivity..."
    "$BINARY" mcp test 2>&1 | grep -E "(✅|❌|Database|Embedding)" | sed 's/^/  /'
    
    # Check HTTP endpoint if server is running
    if [ -n "$PID" ]; then
        echo ""
        echo "Testing HTTP endpoint..."
        if curl -s -f "http://localhost:$HTTP_PORT/health" > /dev/null 2>&1; then
            echo -e "  ${GREEN}✅ HTTP Health: Responding${NC}"
        else
            echo -e "  ${YELLOW}⚠️  HTTP Health: Not responding${NC}"
        fi
    fi
}

# Run diagnostics
run_diagnostics() {
    echo -e "${BLUE}🔍 Running Diagnostics...${NC}"
    check_env
    check_binary
    "$BINARY" mcp diagnose
}

# Clean up
cleanup() {
    echo -e "${BLUE}🧹 Cleaning up...${NC}"
    
    # Stop server if running
    PID=$(get_pid)
    if [ -n "$PID" ]; then
        echo "Stopping server..."
        stop_server
    fi
    
    # Remove files
    echo "Removing temporary files..."
    rm -f "$PID_FILE"
    
    # Ask about logs
    if [ -f "$LOG_FILE" ]; then
        read -p "Remove log file? (y/N) " -n 1 -r
        echo
        if [[ $REPLY =~ ^[Yy]$ ]]; then
            rm -f "$LOG_FILE"
            echo -e "${GREEN}✅ Log file removed${NC}"
        fi
    fi
    
    echo -e "${GREEN}✅ Cleanup complete${NC}"
}

# Show configuration
show_config() {
    echo -e "${BLUE}⚙️  Configuration Details${NC}"
    echo "======================="
    
    if [ ! -f "$ENV_FILE" ]; then
        echo -e "${RED}❌ No .env file found${NC}"
        exit 1
    fi
    
    check_env
    
    echo "Database:"
    echo "  URL: ${DATABASE_URL%@*}@***"
    echo "  Host: ${HOST:-extracted from URL}"
    echo "  Database: ${DB_NAME:-extracted from URL}"
    echo "  User: ${DB_USER:-extracted from URL}"
    
    echo ""
    echo "Embedding Service:"
    echo "  Provider: $EMBEDDING_PROVIDER"
    echo "  Model: $EMBEDDING_MODEL"
    echo "  Base URL: $EMBEDDING_BASE_URL"
    echo "  Timeout: ${EMBEDDING_TIMEOUT_SECONDS}s"
    
    echo ""
    echo "Server:"
    echo "  HTTP Port: $HTTP_PORT"
    echo "  MCP Port: $MCP_PORT"
    echo "  Log Level: $LOG_LEVEL"
    echo "  Auto Migrate: ${AUTO_MIGRATE:-false}"
    
    echo ""
    echo "Memory Tiers:"
    echo "  Working Limit: ${WORKING_TIER_LIMIT:-1000}"
    echo "  Warm Limit: ${WARM_TIER_LIMIT:-10000}"
    echo "  Working→Warm: ${WORKING_TO_WARM_DAYS:-7} days"
    echo "  Warm→Cold: ${WARM_TO_COLD_DAYS:-30} days"
}

# Install systemd service (Linux)
install_service() {
    if [[ "$OSTYPE" != "linux-gnu"* ]]; then
        echo -e "${YELLOW}⚠️  Service installation is only available on Linux${NC}"
        echo "   On macOS, use launchd or run manually"
        exit 1
    fi
    
    echo -e "${BLUE}📦 Installing systemd service...${NC}"
    
    SERVICE_FILE="/etc/systemd/system/codex-memory.service"
    
    # Create service file
    sudo tee "$SERVICE_FILE" > /dev/null << EOF
[Unit]
Description=Agentic Memory System MCP Server
After=network.target postgresql.service

[Service]
Type=simple
User=$USER
WorkingDirectory=$SCRIPT_DIR
EnvironmentFile=$ENV_FILE
ExecStart=$BINARY start
ExecStop=/bin/kill -TERM \$MAINPID
Restart=on-failure
RestartSec=10
StandardOutput=append:$LOG_FILE
StandardError=append:$LOG_FILE

[Install]
WantedBy=multi-user.target
EOF
    
    # Reload systemd and enable service
    sudo systemctl daemon-reload
    sudo systemctl enable codex-memory.service
    
    echo -e "${GREEN}✅ Service installed${NC}"
    echo "   Start: sudo systemctl start codex-memory"
    echo "   Stop: sudo systemctl stop codex-memory"
    echo "   Status: sudo systemctl status codex-memory"
    echo "   Logs: journalctl -u codex-memory -f"
}

# Install launchd service (macOS)
install_launchd() {
    if [[ "$OSTYPE" != "darwin"* ]]; then
        echo -e "${YELLOW}⚠️  Launchd installation is only available on macOS${NC}"
        exit 1
    fi
    
    echo -e "${BLUE}📦 Installing launchd service...${NC}"
    
    PLIST_FILE="$HOME/Library/LaunchAgents/com.codex.memory.plist"
    
    # Create plist file
    cat > "$PLIST_FILE" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.codex.memory</string>
    <key>ProgramArguments</key>
    <array>
        <string>$BINARY</string>
        <string>start</string>
    </array>
    <key>WorkingDirectory</key>
    <string>$SCRIPT_DIR</string>
    <key>EnvironmentVariables</key>
    <dict>
EOF
    
    # Add environment variables
    check_env
    for var in DATABASE_URL EMBEDDING_PROVIDER EMBEDDING_BASE_URL EMBEDDING_MODEL HTTP_PORT MCP_PORT LOG_LEVEL; do
        if [ -n "${!var}" ]; then
            echo "        <key>$var</key>" >> "$PLIST_FILE"
            echo "        <string>${!var}</string>" >> "$PLIST_FILE"
        fi
    done
    
    cat >> "$PLIST_FILE" << EOF
    </dict>
    <key>StandardOutPath</key>
    <string>$LOG_FILE</string>
    <key>StandardErrorPath</key>
    <string>$LOG_FILE</string>
    <key>RunAtLoad</key>
    <false/>
    <key>KeepAlive</key>
    <dict>
        <key>SuccessfulExit</key>
        <false/>
    </dict>
</dict>
</plist>
EOF
    
    # Load the service
    launchctl load "$PLIST_FILE"
    
    echo -e "${GREEN}✅ Launchd service installed${NC}"
    echo "   Start: launchctl start com.codex.memory"
    echo "   Stop: launchctl stop com.codex.memory"
    echo "   Status: launchctl list | grep codex.memory"
    echo "   Uninstall: launchctl unload $PLIST_FILE"
}

# Show help
show_help() {
    echo -e "${PURPLE}Agentic Memory System - MCP Server Manager${NC}"
    echo "==========================================="
    echo ""
    echo "Usage: $0 <command> [options]"
    echo ""
    echo "Commands:"
    echo "  start              Start the MCP server"
    echo "  stop               Stop the MCP server"
    echo "  restart            Restart the MCP server"
    echo "  status             Show server status"
    echo "  logs [options]     Show server logs"
    echo "                     Options: follow, all, clear, [number]"
    echo "  health             Run health check"
    echo "  diagnose           Run diagnostics"
    echo "  config             Show configuration"
    echo "  cleanup            Stop server and clean up files"
    echo "  install-service    Install as system service (Linux)"
    echo "  install-launchd    Install as launchd service (macOS)"
    echo "  help               Show this help message"
    echo ""
    echo "Examples:"
    echo "  $0 start           # Start the server"
    echo "  $0 logs follow     # Follow logs in real-time"
    echo "  $0 logs 100        # Show last 100 lines"
    echo "  $0 status          # Check if server is running"
    echo "  $0 health          # Run health check"
    echo ""
    echo "Files:"
    echo "  PID: $PID_FILE"
    echo "  Log: $LOG_FILE"
    echo "  Env: $ENV_FILE"
}

# Main command handler
case "${1:-help}" in
    start)
        start_server
        ;;
    stop)
        stop_server
        ;;
    restart)
        restart_server
        ;;
    status)
        show_status
        ;;
    logs)
        show_logs "$@"
        ;;
    health)
        health_check
        ;;
    diagnose)
        run_diagnostics
        ;;
    config)
        show_config
        ;;
    cleanup)
        cleanup
        ;;
    install-service)
        install_service
        ;;
    install-launchd)
        install_launchd
        ;;
    help|--help|-h)
        show_help
        ;;
    *)
        echo -e "${RED}❌ Unknown command: $1${NC}"
        echo "   Run: $0 help"
        exit 1
        ;;
esac