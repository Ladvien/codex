# Frequently Asked Questions (FAQ)

## Overview

This FAQ addresses the most common questions about the Agentic Memory System based on user feedback, support tickets, and community discussions. If you can't find your answer here, please check our [Troubleshooting Guide](troubleshooting_guide.md) or contact support.

---

## Table of Contents

1. [General Questions](#general-questions)
2. [Setup and Installation](#setup-and-installation)
3. [Memory Operations](#memory-operations)
4. [Search and Retrieval](#search-and-retrieval)
5. [Performance and Scaling](#performance-and-scaling)
6. [Integration with Claude](#integration-with-claude)
7. [Troubleshooting](#troubleshooting)
8. [Development and API](#development-and-api)
9. [Security and Privacy](#security-and-privacy)
10. [Billing and Limits](#billing-and-limits)

---

## General Questions

### Q: What is the Agentic Memory System?
**A:** The Agentic Memory System is a production-grade memory solution designed for Claude applications. It provides hierarchical storage with intelligent tiering (Working, Warm, Cold), vector-based semantic search, and comprehensive monitoring. It enables Claude to maintain persistent context across conversations and sessions.

### Q: How does the memory system improve Claude's performance?
**A:** The system allows Claude to:
- Remember context from previous conversations
- Access relevant information quickly through semantic search
- Maintain project-specific knowledge across sessions
- Learn from past interactions to provide better responses
- Reduce repetitive explanations by referencing stored knowledge

### Q: Is my data secure in the memory system?
**A:** Yes, security is a top priority. The system includes:
- End-to-end encryption for data at rest and in transit
- Role-based access control (RBAC)
- Comprehensive audit logging
- Regular security audits and compliance checks
- Data isolation between users and organizations

---

## Setup and Installation

### Q: What are the minimum system requirements?
**A:** 
- **RAM**: Minimum 4GB, recommended 8GB+
- **Storage**: At least 10GB free space
- **Database**: PostgreSQL 14+ with pgvector extension
- **Network**: Stable internet connection
- **OS**: Linux, macOS, or Windows with WSL2

### Q: Can I run this on Windows?
**A:** Yes, but we recommend using Windows Subsystem for Linux (WSL2) for the best experience. Native Windows support is available but may have some limitations with certain features.

### Q: How do I install the pgvector extension?
**A:**
```bash
# On Ubuntu/Debian
sudo apt-get install postgresql-14-pgvector

# On macOS with Homebrew
brew install pgvector

# Enable extension in your database
psql your_database -c "CREATE EXTENSION IF NOT EXISTS vector;"
```

### Q: Can I use a different database instead of PostgreSQL?
**A:** Currently, only PostgreSQL with pgvector is supported due to our vector search requirements. We're exploring other vector databases for future releases.

### Q: How do I migrate from an existing memory system?
**A:** We provide migration tools and scripts. Contact our support team for assistance with large-scale migrations. The process typically involves:
1. Export data from your existing system
2. Transform data to our format
3. Import using our bulk import tools
4. Verify data integrity

---

## Memory Operations

### Q: What's the difference between memory tiers?
**A:**
- **Working Memory (Hot)**: <1ms access time, for frequently used data
- **Warm Storage**: <100ms access time, for moderately accessed data  
- **Cold Archive**: <20s access time, for long-term storage

Data automatically migrates between tiers based on access patterns and importance scores.

### Q: How large can a single memory be?
**A:** The default limit is 1MB per memory entry. This can be configured up to 10MB for enterprise installations, but we recommend breaking large content into smaller, more manageable pieces.

### Q: Can I organize memories hierarchically?
**A:** Yes, memories can have parent-child relationships using the `parent_id` field. This enables hierarchical organization and context inheritance.

### Q: How does deduplication work?
**A:** The system uses SHA-256 content hashing to detect duplicate memories. When identical content is detected, the system:
- Merges metadata from both entries
- Updates access patterns and importance scores
- Maintains the most recent version

### Q: Can memories expire automatically?
**A:** Yes, you can set expiration dates using the `expires_at` field. The system automatically cleans up expired memories during maintenance cycles.

---

## Search and Retrieval

### Q: How does semantic search work?
**A:** Semantic search uses vector embeddings to understand meaning rather than just keywords. When you create a memory, we generate a high-dimensional vector representation. Searches compare query vectors with stored vectors to find semantically similar content.

### Q: Can I search using multiple criteria?
**A:** Yes, the advanced search API supports:
- Text-based queries
- Semantic similarity
- Metadata filters
- Date ranges
- Importance scores
- Memory tiers
- Custom tags

### Q: Why don't I see results for content I know exists?
**A:** Common reasons:
1. **Tier restrictions**: Check if you're filtering by specific tiers
2. **Similarity threshold**: Lower the threshold for broader results
3. **Access permissions**: Ensure you have read permissions
4. **Soft deletion**: The memory might be marked as deleted
5. **Migration status**: Memory might be currently migrating between tiers

### Q: How can I improve search relevance?
**A:**
- Use descriptive, specific search terms
- Include relevant metadata when creating memories
- Set appropriate importance scores
- Use tags for better categorization
- Consider hybrid search with custom weights

### Q: Can I search within specific time ranges?
**A:** Yes, use the `date_range` parameter in search requests:
```json
{
  "query_text": "project updates",
  "date_range": {
    "start": "2024-01-01T00:00:00Z",
    "end": "2024-01-31T23:59:59Z"
  }
}
```

---

## Performance and Scaling

### Q: How many memories can the system handle?
**A:** The system is designed to scale to millions of memories. Performance characteristics:
- Working tier: Optimized for <1000 memories per user
- Warm tier: Efficiently handles 10,000s of memories
- Cold tier: Scales to millions with acceptable latency

### Q: What affects search performance?
**A:** Key factors:
- **Number of memories searched**: More memories = longer search time
- **Vector dimensions**: Higher dimensions = more computation
- **Index quality**: Regular maintenance improves performance
- **Query complexity**: Multiple filters add overhead
- **Hardware resources**: CPU, RAM, and storage speed

### Q: How can I optimize performance?
**A:**
1. **Use appropriate tiers**: Keep frequently accessed data in Working tier
2. **Optimize queries**: Use specific filters to reduce search scope
3. **Regular maintenance**: Run VACUUM and ANALYZE on the database
4. **Monitor resources**: Ensure adequate CPU and RAM
5. **Index tuning**: Regularly rebuild vector indexes

### Q: What happens when I reach storage limits?
**A:** The system implements graceful degradation:
1. Warnings at 80% capacity
2. Automatic tier migration to create space
3. Cleanup of expired memories
4. Read-only mode at 95% capacity
5. Contact administrators for capacity expansion

---

## Integration with Claude

### Q: How do I connect Claude Code to the memory system?
**A:** The memory system uses the MCP (Model Context Protocol) for integration:
1. Ensure the memory system is running
2. Configure the MCP endpoint in Claude Code settings
3. Authenticate using your API key
4. Test the connection with a health check

### Q: Can multiple Claude instances share the same memory?
**A:** Yes, but access is controlled by user authentication and permissions. Each user has isolated memory spaces by default, but shared spaces can be configured for teams.

### Q: How does Claude decide what to remember?
**A:** Memory storage is typically triggered by:
- Explicit user requests ("Remember this")
- Important information in conversations
- User corrections or feedback
- Project-specific context
- Configuration-based rules

### Q: Can I control what Claude remembers about me?
**A:** Absolutely. You have full control over your memories:
- View all stored memories
- Delete specific memories
- Set retention policies
- Configure privacy settings
- Export your data at any time

---

## Troubleshooting

### Q: The system says it's unhealthy, what should I do?
**A:**
1. Check the health endpoint: `curl http://localhost:3333/api/v1/health`
2. Verify database connectivity
3. Check available disk space
4. Review system logs for errors
5. Restart services if needed

See our [Troubleshooting Guide](troubleshooting_guide.md) for detailed steps.

### Q: I'm getting "Memory not found" errors
**A:** This usually means:
- The memory ID is incorrect or malformed
- The memory was deleted or expired
- You lack read permissions
- The memory is currently migrating between tiers

### Q: Search is returning empty results
**A:** Common solutions:
- Lower the similarity threshold
- Check tier filters in your query
- Verify the content exists in the database
- Ensure the embedding service is running
- Check for typos in search terms

### Q: I'm experiencing slow performance
**A:**
1. Check system resource usage (CPU, RAM, disk)
2. Review database performance metrics
3. Look for slow queries in logs
4. Verify index status and rebuild if needed
5. Consider scaling up hardware resources

### Q: Connection timeout errors
**A:**
- Increase connection timeout settings
- Check network connectivity
- Verify firewall configurations
- Monitor connection pool usage
- Consider adding read replicas

---

## Development and API

### Q: Is there an API rate limit?
**A:** Yes, default limits are:
- Free tier: 100 requests/hour
- Basic: 1,000 requests/hour
- Premium: 10,000 requests/hour
- Enterprise: Custom limits

Rate limits are per API key and include burst allowances.

### Q: How do I authenticate with the API?
**A:** Use Bearer token authentication:
```
Authorization: Bearer <your-api-key>
```

For development, you can also use JWT tokens for user-based authentication.

### Q: Can I bulk import memories?
**A:** Yes, use the bulk import endpoint:
```bash
POST /api/v1/memories/bulk
Content-Type: application/json

{
  "memories": [
    {"content": "Memory 1", "tier": "Working"},
    {"content": "Memory 2", "tier": "Warm"}
  ]
}
```

### Q: How do I handle errors in my integration?
**A:** Follow these best practices:
- Always check HTTP status codes
- Use error codes for programmatic handling
- Implement exponential backoff for retryable errors
- Log detailed error information including request IDs

### Q: Is there a SDK or client library?
**A:** Currently, we provide:
- REST API documentation
- OpenAPI/Swagger specs
- Example code in multiple languages
- Official Rust SDK (in beta)

Python and JavaScript SDKs are planned for future releases.

---

## Security and Privacy

### Q: How is my data encrypted?
**A:** We use multiple layers of encryption:
- AES-256 encryption for data at rest
- TLS 1.3 for data in transit
- Application-level encryption for sensitive fields
- Encrypted database backups

### Q: Can I use my own encryption keys?
**A:** Yes, enterprise customers can use customer-managed keys (CMK) through key management services like AWS KMS or Azure Key Vault.

### Q: How long is data retained?
**A:** Default retention policies:
- Active memories: No automatic deletion
- Deleted memories: 30-day soft delete period
- Audit logs: 90 days
- Backup data: 2 years

Custom retention policies can be configured per organization.

### Q: Is the system GDPR compliant?
**A:** Yes, the system includes:
- Data portability (export functionality)
- Right to deletion (hard delete capabilities)
- Data minimization principles
- Privacy by design architecture
- Regular compliance audits

### Q: Can I audit who accessed my data?
**A:** Yes, comprehensive audit logging tracks:
- User authentication events
- Memory access and modifications
- Search queries (with user consent)
- Administrative actions
- Failed access attempts

---

## Billing and Limits

### Q: How is usage calculated?
**A:** Billing is based on:
- Number of stored memories
- API requests per month
- Storage used (GB)
- Compute resources for search operations

### Q: What happens if I exceed my limits?
**A:** Depending on the limit:
- **Storage**: Automatic tier migration and cleanup
- **API requests**: Rate limiting with queuing
- **Compute**: Temporary throttling

We send notifications before limits are reached.

### Q: Can I monitor my usage?
**A:** Yes, through:
- Dashboard with real-time usage metrics
- Monthly usage reports via email
- API endpoints for programmatic monitoring
- Alerts when approaching limits

### Q: How do I upgrade my plan?
**A:** You can upgrade:
- Through the web dashboard
- By contacting our sales team
- Via the billing API
- Automatic upgrade recommendations based on usage patterns

---

## Still Need Help?

### Contact Support
- **Email**: support@company.com
- **Slack**: #memory-system-support
- **Documentation**: [docs.memorySystem.com](https://docs.memorySystem.com)
- **Status Page**: [status.memorySystem.com](https://status.memorySystem.com)

### Community Resources
- **GitHub Issues**: Report bugs and request features
- **Community Forum**: Discuss best practices
- **Blog**: Technical articles and announcements
- **Webinars**: Regular training sessions

### Professional Services
- **Migration assistance**: Help moving from other systems
- **Custom integration**: Tailored solutions for enterprise needs
- **Training**: Team training and best practices
- **Support plans**: Premium support with SLA guarantees

---

**Last Updated**: January 2024  
**Version**: 1.0.0

*This FAQ is regularly updated based on user feedback and new features. Please let us know if you have questions that aren't covered here.*