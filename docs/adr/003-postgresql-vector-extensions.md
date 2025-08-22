# ADR-003: PostgreSQL with Vector Extensions

## Status
Accepted

## Context
The memory system requires both relational data management and vector similarity search capabilities. We evaluated several options including separate vector databases (Pinecone, Weaviate, Qdrant) versus PostgreSQL with pgvector extension.

## Decision
Use PostgreSQL with pgvector extension as the primary data store.

Configuration:
- PostgreSQL 15+ with pgvector extension
- HNSW indexes for approximate nearest neighbor search
- Hybrid queries combining relational and vector search
- Connection pooling with optimized pool sizes

## Consequences
**Positive:**
- Single database system reduces operational complexity
- ACID compliance for critical memory operations
- Mature ecosystem with excellent tooling
- Cost-effective compared to managed vector databases
- Hybrid queries enable complex search patterns

**Negative:**
- Vector search performance slightly lower than specialized databases
- Need to manage PostgreSQL extensions and configuration
- Index building time can be significant for large datasets

**Risks:**
- pgvector extension compatibility with PostgreSQL updates
- Performance degradation with very large vector datasets (>10M vectors)