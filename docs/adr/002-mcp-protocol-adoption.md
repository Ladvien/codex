# ADR-002: MCP Protocol Adoption

## Status
Accepted

## Context
The system needs a standardized protocol for client-server communication that supports modern AI applications. We evaluated several options including REST APIs, GraphQL, and the Model Context Protocol (MCP).

## Decision
Adopt the Model Context Protocol (MCP) as the primary communication protocol for the memory system.

Key features implemented:
- MCP tools for memory operations (create, search, migrate, etc.)
- Resource providers for memory content access
- Authentication and rate limiting within MCP framework
- Silent harvesting of conversation context

## Consequences
**Positive:**
- Native integration with Claude clients (Desktop, Code, etc.)
- Standardized protocol reduces integration complexity
- Built-in support for resource management and tool calling
- Future-proof protocol designed for AI applications

**Negative:**  
- Dependency on relatively new protocol specification
- Additional learning curve for developers unfamiliar with MCP
- Need to maintain compatibility with protocol updates

**Risks:**
- MCP protocol evolution could require significant refactoring
- Limited ecosystem compared to REST/GraphQL