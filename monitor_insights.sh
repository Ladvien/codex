#!/bin/bash

# Monitoring script for periodic insight generation
# This script runs the automated insights generator every 2 minutes
# and logs all output for monitoring

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LOG_DIR="$SCRIPT_DIR/logs"
LOG_FILE="$LOG_DIR/insights_monitor.log"
INSIGHTS_SCRIPT="$SCRIPT_DIR/examples/automated_insights.sh"
INTERVAL_SECONDS=30  # Run every 30 seconds for testing
INSIGHTS_OUTPUT_FILE="$LOG_DIR/insights_export.md"

# Create logs directory if it doesn't exist
mkdir -p "$LOG_DIR"

# Function to log with timestamp
log_message() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $1" | tee -a "$LOG_FILE"
}

# Function to run insight generation
run_insights() {
    log_message "========================================="
    log_message "Starting insight generation cycle..."
    
    # Set environment variables for the script
    export INSIGHTS_LOG_FILE="$LOG_DIR/insights_content.log"
    export EXPORT_AFTER_GENERATION="true"
    export EXPORT_FILE_PATH="$INSIGHTS_OUTPUT_FILE"
    
    # Load environment for Ollama configuration
    if [ -f "$SCRIPT_DIR/.env" ]; then
        source "$SCRIPT_DIR/.env"
        export OLLAMA_BASE_URL="$EMBEDDING_BASE_URL"
        export OLLAMA_MODEL="${OLLAMA_MODEL:-llama3.2}"
    fi
    
    # Run the automated insights script
    # Try different time periods to ensure we catch some data
    # Include broader time periods since memories may be older
    local time_periods=("last_hour" "last_day" "last_week")
    local insight_types=("all" "learning" "pattern" "connection")
    
    # Randomly select time period and type for variety
    local period=${time_periods[$((RANDOM % ${#time_periods[@]}))]}
    local type=${insight_types[$((RANDOM % ${#insight_types[@]}))]}
    
    log_message "Generating insights for period: $period, type: $type"
    
    # Run the script and capture output
    if OUTPUT=$("$INSIGHTS_SCRIPT" "$period" "$type" 5 2>&1); then
        log_message "Insight generation completed successfully"
        echo "$OUTPUT" >> "$LOG_FILE"
        
        # Check if insights were actually generated
        if echo "$OUTPUT" | grep -q "Insights generated successfully"; then
            log_message "✓ New insights generated!"
            
            # Show a snippet of the generated insights
            if [ -f "$INSIGHTS_OUTPUT_FILE" ]; then
                log_message "Latest export preview:"
                head -10 "$INSIGHTS_OUTPUT_FILE" | while IFS= read -r line; do
                    log_message "  $line"
                done
            fi
        else
            log_message "No new insights generated (may not have enough recent memories)"
        fi
    else
        log_message "✗ Insight generation failed: $OUTPUT"
    fi
    
    log_message "Cycle complete. Waiting $INTERVAL_SECONDS seconds..."
}

# Signal handler for graceful shutdown
cleanup() {
    log_message "Received shutdown signal. Stopping monitor..."
    exit 0
}

trap cleanup SIGINT SIGTERM

# Main monitoring loop
log_message "========================================="
log_message "Insight Generation Monitor Started"
log_message "Log file: $LOG_FILE"
log_message "Interval: $INTERVAL_SECONDS seconds"
log_message "Press Ctrl+C to stop"
log_message "========================================="

# Initial run
run_insights

# Continuous monitoring loop
while true; do
    sleep "$INTERVAL_SECONDS"
    run_insights
done