#!/bin/bash

# Codex Dreams - Automated Insight Generation Script
# 
# This script can be used with cron or other schedulers to periodically
# generate insights from your memories. By default, insights are only
# generated when explicitly requested.
#
# Usage:
#   ./automated_insights.sh                    # Generate insights from last hour
#   ./automated_insights.sh last_day          # Generate insights from last day
#   ./automated_insights.sh last_week learning # Generate learning insights from last week
#
# Cron example (every 30 minutes):
#   */30 * * * * /path/to/automated_insights.sh last_hour
#
# Cron example (daily at 2 AM):
#   0 2 * * * /path/to/automated_insights.sh last_day

# Configuration
CODEX_BINARY="${CODEX_BINARY:-codex-memory}"
TIME_PERIOD="${1:-last_hour}"
INSIGHT_TYPE="${2:-all}"
MAX_INSIGHTS="${3:-5}"

# Check if codex-memory is available
if ! command -v "$CODEX_BINARY" &> /dev/null; then
    echo "Error: codex-memory not found. Please install it or set CODEX_BINARY environment variable."
    exit 1
fi

# Timestamp for logging
TIMESTAMP=$(date '+%Y-%m-%d %H:%M:%S')
echo "[$TIMESTAMP] Starting automated insight generation..."

# Create the MCP request
MCP_REQUEST=$(cat <<EOF
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "generate_insights",
    "arguments": {
      "time_period": "$TIME_PERIOD",
      "insight_type": "$INSIGHT_TYPE",
      "max_insights": $MAX_INSIGHTS
    }
  }
}
EOF
)

# Execute the insight generation
echo "[$TIMESTAMP] Generating insights for period: $TIME_PERIOD, type: $INSIGHT_TYPE"
RESPONSE=$(echo "$MCP_REQUEST" | $CODEX_BINARY mcp-stdio --skip-setup 2>/dev/null)

# Check if successful
if echo "$RESPONSE" | grep -q "error"; then
    echo "[$TIMESTAMP] Error generating insights:"
    echo "$RESPONSE" | jq -r '.error.message' 2>/dev/null || echo "$RESPONSE"
    exit 1
else
    # Extract and display result
    RESULT=$(echo "$RESPONSE" | jq -r '.result.content[0].text' 2>/dev/null)
    if [ -n "$RESULT" ]; then
        echo "[$TIMESTAMP] Insights generated successfully:"
        echo "$RESULT" | head -20
        
        # Optional: Save to log file
        if [ -n "$INSIGHTS_LOG_FILE" ]; then
            echo "[$TIMESTAMP] $RESULT" >> "$INSIGHTS_LOG_FILE"
        fi
    else
        echo "[$TIMESTAMP] Insights generated but no content returned"
    fi
fi

# Optional: Export insights after generation
if [ "$EXPORT_AFTER_GENERATION" = "true" ]; then
    echo "[$TIMESTAMP] Exporting insights..."
    EXPORT_REQUEST=$(cat <<EOF
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/call",
  "params": {
    "name": "export_insights",
    "arguments": {
      "format": "markdown",
      "time_period": "$TIME_PERIOD",
      "min_confidence": 0.6
    }
  }
}
EOF
)
    
    EXPORT_RESPONSE=$(echo "$EXPORT_REQUEST" | $CODEX_BINARY mcp-stdio --skip-setup 2>/dev/null)
    
    # Save export to file if path provided
    if [ -n "$EXPORT_FILE_PATH" ]; then
        echo "$EXPORT_RESPONSE" | jq -r '.result.content[0].text' > "$EXPORT_FILE_PATH"
        echo "[$TIMESTAMP] Insights exported to $EXPORT_FILE_PATH"
    fi
fi

echo "[$TIMESTAMP] Automated insight generation complete"

# Exit successfully
exit 0