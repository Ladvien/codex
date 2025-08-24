# Codex Dreams - Insight Generation Guide

## Overview

Codex Dreams is an AI-powered insight generation system that analyzes your stored memories to discover patterns, connections, and learnings. It uses Ollama (local LLM) to process memories and generate meaningful insights that are stored in the database for future reference.

## Manual Generation (Default Behavior)

By default, insight generation is **completely manual**. No background processing occurs unless you explicitly request it. This ensures:
- Full control over when processing happens
- No unexpected resource usage
- Privacy-first approach to memory analysis
- Predictable system behavior

### Generating Insights via Claude

Use the MCP commands through Claude to generate insights:

```
generate_insights
  - time_period: "last_hour" | "last_day" | "last_week" | "last_month" 
  - topic: (optional) filter memories by topic
  - insight_type: "learning" | "pattern" | "connection" | "all"
  - max_insights: number of insights to generate (default: 5)
```

Example conversation with Claude:
> "Generate insights from my memories from the last week"
> "Find patterns in my coding-related memories"
> "Generate learning insights from today's memories"

### Exporting Insights

Export your generated insights in various formats:

```
export_insights
  - format: "markdown" | "json"
  - time_period: "day" | "week" | "month" | "all"
  - min_confidence: 0.0 to 1.0 (default: 0.6)
  - include_metadata: true | false
```

## Requirements

### Ollama Setup

1. **Install Ollama** from https://ollama.ai
2. **Pull a model** (recommended: llama3.2 or llama2):
   ```bash
   ollama pull llama3.2
   ```
3. **Configure in .env**:
   ```env
   OLLAMA_BASE_URL=http://localhost:11434
   OLLAMA_MODEL=llama3.2:latest
   ```

### Network Configuration

If Ollama runs on a different machine:
- Private network IPs are allowed (192.168.x.x, 10.x.x.x)
- Update `OLLAMA_BASE_URL` to point to the correct address
- Ensure firewall allows connection on port 11434

## Automated Generation (Advanced Users)

While the system doesn't automatically generate insights, you can set up your own automation:

### Option 1: System Scheduler (Cron/Task Scheduler)

Create a cron job that triggers insight generation:

```bash
# Add to crontab (Linux/Mac)
*/30 * * * * /path/to/codex/examples/automated_insights.sh
```

### Option 2: Custom Script

Use the provided example script `examples/automated_insights.sh`:

```bash
#!/bin/bash
# Trigger insight generation via MCP
echo '{"method":"tools/call","params":{"name":"generate_insights","arguments":{"time_period":"last_hour"}}}' | \
  codex-memory mcp-stdio --skip-setup
```

### Option 3: External Automation

Use any automation tool (Zapier, IFTTT, etc.) to call the MCP endpoints periodically.

## Monitoring Insights

### Check Database Directly

```sql
-- Count insights
SELECT COUNT(*) FROM insights;

-- View recent insights
SELECT content, insight_type, confidence_score, created_at 
FROM insights 
ORDER BY created_at DESC 
LIMIT 10;

-- High-confidence insights
SELECT content, confidence_score 
FROM insights 
WHERE confidence_score > 0.8 
ORDER BY confidence_score DESC;
```

### Via MCP Commands

Use Claude to query insights:
- "Show me my recent insights"
- "Export all high-confidence insights"
- "Search for insights about productivity"

## Insight Types

The system generates six types of insights:

1. **Learning** - New knowledge or skills acquired
2. **Pattern** - Recurring themes or behaviors
3. **Connection** - Relationships between different memories
4. **Relationship** - Interpersonal or system relationships
5. **Assertion** - Beliefs or conclusions formed
6. **Mental Model** - Frameworks or understanding structures

## Configuration Options

### Environment Variables

```env
# Ollama Configuration
OLLAMA_BASE_URL=http://localhost:11434
OLLAMA_MODEL=llama3.2:latest
OLLAMA_TIMEOUT=60

# Insight Processing
INSIGHTS_BATCH_SIZE=50
INSIGHTS_MIN_CONFIDENCE=0.6
INSIGHTS_MAX_PER_BATCH=10

# Feature Flag (must be enabled at build time)
# Build with: cargo build --features codex-dreams
```

### Processing Settings

Adjust these for different processing behaviors:
- **Batch Size**: Number of memories to process at once
- **Min Confidence**: Minimum confidence score to keep insights
- **Max Per Batch**: Maximum insights to generate per processing run

## Troubleshooting

### No Insights Being Generated

1. **Check Ollama is running**:
   ```bash
   curl http://localhost:11434/api/version
   ```

2. **Verify memories exist**:
   ```sql
   SELECT COUNT(*) FROM memories;
   ```

3. **Check logs for errors**:
   ```bash
   cargo run --features codex-dreams --example test_insights_simple
   ```

### Ollama Connection Issues

- Ensure Ollama service is running
- Check firewall settings
- Verify OLLAMA_BASE_URL is correct
- Test with curl: `curl -X POST http://localhost:11434/api/generate -d '{"model":"llama3.2","prompt":"test"}'`

### Low Quality Insights

- Try a different Ollama model (llama3.2, mixtral, etc.)
- Increase MIN_CONFIDENCE threshold
- Process more memories per batch
- Ensure memories have sufficient content

## Privacy & Security

- All processing happens locally (no external API calls)
- Insights are stored in your local database only
- No automatic processing without explicit user action
- Ollama runs entirely on your machine/network

## Future Enhancements

The scheduler infrastructure exists in the codebase but is intentionally not activated. Future versions may offer:
- Opt-in background processing
- Scheduled insight generation
- Real-time insight triggers
- Webhooks for insight events

For now, all insight generation requires explicit user action, ensuring complete control over your data processing.

## Support

For issues or questions:
- Check the logs: `~/.codex/logs/`
- Run health check: `codex-memory health --detailed`
- See examples: `/examples/` directory
- GitHub Issues: https://github.com/Ladvien/codex

---

*Codex Dreams respects your privacy and gives you complete control over when and how your memories are analyzed.*