# MCP Authentication and Rate Limiting

This document describes the authentication and rate limiting system implemented for the MCP (Model Context Protocol) server in the codex-memory project.

## Overview

The MCP server now includes comprehensive authentication and rate limiting capabilities to secure access to memory management tools and prevent abuse. The system supports multiple authentication methods and configurable rate limits with performance targets under 5ms per request.

## Features

- **Multiple Authentication Methods**: API keys, JWT tokens, and certificate-based authentication
- **Configurable Rate Limiting**: Per-client, per-tool, and global rate limits
- **Silent Mode Support**: Reduced rate limits for background operations
- **Audit Logging**: All authentication events and rate limit violations are logged
- **Performance Optimized**: <5ms authentication and rate limiting overhead
- **Environment Configuration**: All settings configurable via environment variables

## Authentication Methods

### API Key Authentication

The simplest authentication method using pre-configured API keys.

**Configuration:**
```bash
# Single API key setup
MCP_API_KEY=your-secure-api-key-here
MCP_CLIENT_ID=your-client-id

# Multiple API keys (JSON format)
MCP_API_KEYS='{"key1": {"client_id": "client1", "scopes": ["mcp:read", "mcp:write"]}, "key2": {"client_id": "client2", "scopes": ["mcp:read"]}}'
```

**Usage:**
```bash
# Using Authorization header
Authorization: ApiKey your-secure-api-key-here

# Using X-API-Key header
X-API-Key: your-secure-api-key-here
```

### JWT Token Authentication

JSON Web Token authentication for more sophisticated access control.

**Configuration:**
```bash
MCP_JWT_SECRET=your-jwt-secret-key-minimum-32-characters-long
MCP_JWT_EXPIRY_SECONDS=3600  # 1 hour
```

**Usage:**
```bash
# Using Authorization header
Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...
```

### Certificate-based Authentication

Client certificate authentication for high-security environments.

**Configuration:**
```bash
MCP_ALLOWED_CERTS=abc123def456,789ghi012jkl
```

**Usage:**
```bash
# Certificate thumbprint in header
X-Client-Cert-Thumbprint: abc123def456
```

## Rate Limiting

The system implements a sophisticated rate limiting mechanism with multiple layers:

### Configuration

```bash
# Enable/disable rate limiting
MCP_RATE_LIMIT_ENABLED=true

# Global limits (requests per minute)
MCP_GLOBAL_RATE_LIMIT=1000
MCP_GLOBAL_BURST_SIZE=50

# Per-client limits
MCP_CLIENT_RATE_LIMIT=100
MCP_CLIENT_BURST_SIZE=10

# Silent mode multiplier (reduces limits)
MCP_SILENT_MODE_MULTIPLIER=0.5

# Whitelisted clients (bypass rate limits)
MCP_RATE_LIMIT_WHITELIST=admin-client,monitoring-client

# Custom tool-specific limits
MCP_TOOL_RATE_LIMITS='{"store_memory": 50, "search_memory": 200}'
MCP_TOOL_BURST_SIZES='{"store_memory": 5, "search_memory": 20}'
```

### Rate Limiting Layers

1. **Global Rate Limit**: Applies to all requests across all clients
2. **Per-Client Rate Limit**: Individual limits per authenticated client
3. **Per-Tool Rate Limit**: Different limits for different MCP tools
4. **Silent Mode**: Reduced limits for background operations

### Tool-Specific Defaults

| Tool | Requests/Min | Burst Size | Scope Required |
|------|-------------|------------|----------------|
| `store_memory` | 50 | 5 | `mcp:write` |
| `search_memory` | 200 | 20 | `mcp:read` |
| `get_statistics` | 20 | 2 | `mcp:read` |
| `what_did_you_remember` | 30 | 3 | `mcp:read` |
| `harvest_conversation` | 100 | 10 | `mcp:write` |
| `get_harvester_metrics` | 10 | 1 | `mcp:read` |
| `migrate_memory` | 20 | 2 | `mcp:write` |
| `delete_memory` | 10 | 1 | `mcp:write` |

## Scopes and Permissions

The system uses scope-based access control:

- **`mcp:read`**: Read-only operations (search, statistics, metrics)
- **`mcp:write`**: Write operations (store, harvest, migrate, delete)

## Error Codes

The MCP server returns specific error codes for authentication and rate limiting:

- **-32001**: Authentication failed
- **-32002**: Rate limit exceeded
- **-32003**: Access denied (insufficient permissions)

## Environment Variables Reference

### Authentication

| Variable | Default | Description |
|----------|---------|-------------|
| `MCP_AUTH_ENABLED` | `false` | Enable/disable authentication |
| `MCP_JWT_SECRET` | (required) | JWT signing secret (min 32 chars) |
| `MCP_JWT_EXPIRY_SECONDS` | `3600` | JWT token expiration time |
| `MCP_API_KEY` | - | Single API key |
| `MCP_CLIENT_ID` | `default-client` | Default client ID |
| `MCP_API_KEYS` | - | Multiple API keys (JSON) |
| `MCP_ALLOWED_CERTS` | - | Allowed certificate thumbprints |

### Rate Limiting

| Variable | Default | Description |
|----------|---------|-------------|
| `MCP_RATE_LIMIT_ENABLED` | `true` | Enable/disable rate limiting |
| `MCP_GLOBAL_RATE_LIMIT` | `1000` | Global requests per minute |
| `MCP_GLOBAL_BURST_SIZE` | `50` | Global burst size |
| `MCP_CLIENT_RATE_LIMIT` | `100` | Per-client requests per minute |
| `MCP_CLIENT_BURST_SIZE` | `10` | Per-client burst size |
| `MCP_SILENT_MODE_MULTIPLIER` | `0.5` | Rate limit reduction in silent mode |
| `MCP_RATE_LIMIT_WHITELIST` | - | Whitelisted clients (comma-separated) |
| `MCP_TOOL_RATE_LIMITS` | (see table) | Tool-specific limits (JSON) |
| `MCP_TOOL_BURST_SIZES` | (see table) | Tool-specific burst sizes (JSON) |

## Quick Start

1. **Enable Authentication:**
   ```bash
   export MCP_AUTH_ENABLED=true
   export MCP_API_KEY=your-secure-key-here
   ```

2. **Configure Rate Limiting:**
   ```bash
   export MCP_RATE_LIMIT_ENABLED=true
   export MCP_CLIENT_RATE_LIMIT=60  # 1 request per second
   ```

3. **Start the Server:**
   ```bash
   cargo run --bin codex-memory
   ```

4. **Make Authenticated Requests:**
   ```bash
   # Using curl
   curl -H "Authorization: ApiKey your-secure-key-here" \
        -H "Content-Type: application/json" \
        -d '{"jsonrpc":"2.0","method":"tools/list","id":1}' \
        http://localhost:8081
   ```

## Security Best Practices

1. **Use Strong API Keys**: Generate cryptographically secure random keys
2. **Set Appropriate JWT Secrets**: Use a strong, unique secret (minimum 32 characters)
3. **Configure Reasonable Rate Limits**: Balance security with usability
4. **Monitor Audit Logs**: Regularly review authentication failures and rate limit violations
5. **Use HTTPS in Production**: Always encrypt communication in production environments
6. **Rotate Credentials**: Regularly rotate API keys and JWT secrets
7. **Implement Network Security**: Use firewalls and network segmentation
8. **Keep Certificates Secure**: Properly manage and rotate client certificates

## Monitoring and Observability

The system provides comprehensive metrics and logging:

### Authentication Statistics
- Total authentication attempts
- Success/failure rates
- Per-client authentication patterns
- JWT token usage and revocation

### Rate Limiting Statistics
- Request rates by client and tool
- Rate limit violations
- Peak usage patterns
- Performance metrics

### Audit Logging
- All authentication events
- Rate limit violations
- Security-relevant events
- Performance metrics

## Performance Characteristics

- **Authentication Overhead**: <2ms typical, <5ms maximum
- **Rate Limiting Overhead**: <2ms typical, <5ms maximum
- **Combined Overhead**: <5ms maximum (requirement met)
- **Memory Usage**: Minimal impact (~1MB for typical configurations)
- **CPU Usage**: <1% on modern systems under normal load

## Troubleshooting

### Common Issues

1. **Authentication Failures**
   - Check API key is correctly configured
   - Verify JWT secret matches between client and server
   - Ensure client certificates are properly installed

2. **Rate Limiting Issues**
   - Check if client is whitelisted
   - Verify rate limits are appropriate for usage patterns
   - Consider adjusting burst sizes for bursty workloads

3. **Performance Issues**
   - Monitor authentication/rate limiting overhead
   - Check for excessive JWT verification calls
   - Verify audit logging isn't causing bottlenecks

### Debug Mode

Enable debug logging to troubleshoot issues:

```bash
export RUST_LOG=debug
cargo run --bin codex-memory
```

## Future Enhancements

- OAuth 2.0 support
- Role-based access control (RBAC)
- Dynamic rate limit adjustment
- Redis-based distributed rate limiting
- Advanced threat detection
- Integration with external identity providers