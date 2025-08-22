# Enhanced Agentic Memory System Architecture v2.0

## Executive Summary

The Enhanced Agentic Memory System is a cognitive-inspired, production-grade memory solution that mimics human memory consolidation patterns. It features a 4-tier storage hierarchy, intelligent forgetting curves, autonomous memory harvesting from Claude conversations, and sophisticated consolidation mechanics based on cutting-edge research in LLM memory systems.

## System Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                               CLIENT LAYER                                        │
├─────────────────────────────────────────────────────────────────────────────────┤
│  ┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐              │
│  │   Claude Code   │    │ Claude Desktop  │    │  Other Clients  │              │
│  └─────────────────┘    └─────────────────┘    └─────────────────┘              │
└─────────────────────────────────────────────────────────────────────────────────┘
                                     │
                                     ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│                            MCP PROTOCOL LAYER                                    │
├─────────────────────────────────────────────────────────────────────────────────┤
│  ┌─────────────────────────────────────────────────────────────────────────────┐ │
│  │                           MCP Server                                        │ │
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐   │ │
│  │  │   Handlers   │  │   Silent     │  │ Rate Limiter │  │ Auth & Valid │   │ │
│  │  │              │  │  Harvester   │  │              │  │              │   │ │
│  │  └──────────────┘  └──────────────┘  └──────────────┘  └──────────────┘   │ │
│  └─────────────────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────────────┘
                                     │
                                     ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│                         COGNITIVE PROCESSING LAYER                               │
├─────────────────────────────────────────────────────────────────────────────────┤
│  ┌─────────────────────────────────────────────────────────────────────────────┐ │
│  │                    Memory Consolidation Engine                              │ │
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐   │ │
│  │  │  Three-Score │  │  Forgetting  │  │  Similarity  │  │  Reflection  │   │ │
│  │  │   Scoring    │  │    Curve     │  │   Merger     │  │  Generator   │   │ │
│  │  └──────────────┘  └──────────────┘  └──────────────┘  └──────────────┘   │ │
│  └─────────────────────────────────────────────────────────────────────────────┘ │
│  ┌─────────────────────────────────────────────────────────────────────────────┐ │
│  │                    Importance Assessment Pipeline                           │ │
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐   │ │
│  │  │   Pattern    │  │   Semantic   │  │     LLM      │  │    Event     │   │ │
│  │  │   Matching   │  │  Similarity  │  │   Scoring    │  │   Triggers   │   │ │
│  │  │   (<10ms)    │  │ (10-100ms)   │  │ (100ms-1s)   │  │  (Immediate) │   │ │
│  │  └──────────────┘  └──────────────┘  └──────────────┘  └──────────────┘   │ │
│  └─────────────────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────────────┘
                                     │
                                     ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│                            APPLICATION LAYER                                     │
├─────────────────────────────────────────────────────────────────────────────────┤
│  ┌─────────────────────────────────────────────────────────────────────────────┐ │
│  │                      Memory Repository                                      │ │
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐   │ │
│  │  │    CRUD      │  │  Enhanced    │  │   4-Tier     │  │  Automated   │   │ │
│  │  │  Operations  │  │    Search    │  │   Manager    │  │  Migration   │   │ │
│  │  └──────────────┘  └──────────────┘  └──────────────┘  └──────────────┘   │ │
│  └─────────────────────────────────────────────────────────────────────────────┘ │
│  ┌─────────────────────────────────────────────────────────────────────────────┐ │
│  │                         Supporting Services                                 │ │
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐   │ │
│  │  │   Embedder   │  │ Deduplicator │  │  Harvester   │  │  Analytics   │   │ │
│  │  │   Service    │  │   & Merger   │  │   Service    │  │   Engine     │   │ │
│  │  └──────────────┘  └──────────────┘  └──────────────┘  └──────────────┘   │ │
│  └─────────────────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────────────┘
                                     │
                                     ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│                              DATA LAYER                                          │
├─────────────────────────────────────────────────────────────────────────────────┤
│                      PostgreSQL 14+ with pgvector                               │
│  ┌─────────────────────────────────────────────────────────────────────────────┐ │
│  │                          4-Tier Memory Storage                              │ │
│  │  ┌────────────┐  ┌────────────┐  ┌────────────┐  ┌────────────┐           │ │
│  │  │  Working   │  │    Warm    │  │    Cold    │  │   Frozen   │           │ │
│  │  │   Memory   │  │  Storage   │  │  Archive   │  │   Storage  │           │ │
│  │  │            │  │            │  │            │  │            │           │ │
│  │  │  <1ms P99  │  │ <100ms P99 │  │  <1s P99   │  │ 2-5s delay │           │ │
│  │  │ P(r) > 0.7 │  │ P(r) > 0.5 │  │ P(r) > 0.2 │  │ P(r) < 0.2 │           │ │
│  │  └────────────┘  └────────────┘  └────────────┘  └────────────┘           │ │
│  └─────────────────────────────────────────────────────────────────────────────┘ │
│  ┌─────────────────────────────────────────────────────────────────────────────┐ │
│  │                       Enhanced Schema Tables                                │ │
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐   │ │
│  │  │ Consolidation│  │   Insights   │  │   Merge      │  │   Harvest    │   │ │
│  │  │     Log      │  │  & Patterns  │  │   History    │  │   Metadata   │   │ │
│  │  └──────────────┘  └──────────────┘  └──────────────┘  └──────────────┘   │ │
│  └─────────────────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────────────┘
```

## Core Enhancements

### 1. Cognitive Processing Layer (NEW)

#### Memory Consolidation Engine
Implements human-like memory consolidation based on forgetting curve research:

**Three-Component Scoring Formula:**
```
memory_score = α × recency_score + β × importance_score + γ × relevance_score

where:
- recency_score = e^(-λt), λ = 0.005 per hour
- importance_score = LLM-generated (0-1)
- relevance_score = cosine_similarity during retrieval
- α = β = γ = 0.333 (configurable)
```

**Forgetting Curve Implementation:**
```
P(recall) = [1 - exp(-r*e^(-t/gn))] / (1 - e^-1)
gn = gn-1 + (1 - e^-t)/(1 + e^-t)

where:
- r = decay rate (individual per memory)
- gn = consolidation strength (increases with recalls)
- t = time since last access
```

#### Multi-Stage Importance Assessment
Real-time evaluation pipeline with tiered processing:

1. **Stage 1: Pattern Matching** (<10ms)
   - Keywords: "remember", "prefer", "decide", "important"
   - User corrections and emotional content
   - Explicit memory requests

2. **Stage 2: Semantic Similarity** (10-100ms)
   - Compare against existing important memories
   - Use cached embeddings for speed
   - Threshold: 0.7 confidence

3. **Stage 3: LLM Scoring** (100ms-1s)
   - Only for Stage 1-2 passes
   - Deep contextual understanding
   - Circuit breaker protection

### 2. Silent Memory Harvesting System (NEW)

#### Autonomous Claude Conversation Harvesting
```javascript
class SilentMemoryHarvester {
    triggers: {
        message_interval: 10,      // Every 10 messages
        time_interval: 300,        // Every 5 minutes
        pattern_detection: true,   // On important patterns
        conversation_start: true   // Load context at start
    },
    
    extraction_patterns: [
        "user_preferences",    // "I prefer...", "I like..."
        "user_facts",          // "I work at...", "My name is..."
        "decisions_made",      // "Let's go with...", "I've decided..."
        "code_snippets",       // Artifact contents
        "task_outcomes",       // "Completed...", "Failed to..."
        "learned_patterns",    // Repeated requests/corrections
        "emotional_context"    // User frustration/satisfaction
    ],
    
    confidence_threshold: 0.7,
    deduplication_threshold: 0.85
}
```

**Silent Operation Protocol:**
- No user interruption during harvesting
- Background processing via MCP tools
- Only announce when explicitly requested
- "What did you remember?" query available

### 3. Enhanced 4-Tier Memory System

#### Tier Characteristics

| Tier | Latency | P(recall) | Capacity | Storage | Use Case |
|------|---------|-----------|----------|---------|----------|
| **Working** | <1ms P99 | >0.7 | Active memories | Uncompressed | Current context |
| **Warm** | <100ms P99 | 0.5-0.7 | Recent history | Light compression | Recent projects |
| **Cold** | <1s P99 | 0.2-0.5 | Archived data | Compressed | Historical reference |
| **Frozen** | 2-5s delay | <0.2 | Deep archive | Heavy compression (5:1) | Rarely accessed |

#### Automated Tier Migration
```rust
async fn migrate_memories() {
    // Runs every hour
    Working → Warm when P(recall) < 0.7
    Warm → Cold when P(recall) < 0.5
    Cold → Frozen when P(recall) < 0.2
    
    // Batch processing for efficiency
    batch_size: 100
    respect_capacity_limits: true
}
```

### 4. Semantic Deduplication & Merging (NEW)

#### Intelligent Memory Compression
```rust
struct DeduplicationEngine {
    similarity_threshold: 0.85,
    merge_strategy: "preserve_all_metadata",
    compression_levels: {
        critical: "lossless",
        normal: "lossy_acceptable",
        archive: "maximum_compression"
    },
    auto_prune_threshold: 0.2,  // P(recall) after 30 days
    target_headroom: 0.20        // 20% free capacity
}
```

**Expected Outcomes:**
- 30% storage reduction through deduplication
- 70% reduction in context tokens
- Maintains information integrity

### 5. Reflection & Insight Generation (NEW)

#### Meta-Memory Creation
```python
class ReflectionGenerator:
    trigger_threshold: 150  # Accumulated importance points
    
    async def generate_insights(memories):
        # Identify patterns across memories
        patterns = detect_patterns(memories)
        
        # Generate 2-3 insights per reflection
        insights = llm_synthesize(patterns)
        
        # Create knowledge graph
        graph = build_relationships(insights, memories)
        
        # Store as meta-memories with 1.5x importance
        store_insights(insights, importance_multiplier=1.5)
```

### 6. Event-Triggered Scoring (NEW)

#### Critical Content Detection
```rust
enum TriggerEvent {
    ExplicitRemember,    // "Remember that..." → 2x importance
    ErrorCorrection,     // User corrects Claude → 1.8x
    EmotionalContent,    // Frustration/satisfaction → 1.5x
    DecisionMade,        // "Let's go with..." → 1.7x
    PreferenceStated,    // "I prefer..." → 1.6x
}
```

## Enhanced Data Schema

### Core Memory Table
```sql
CREATE TABLE memories (
    id UUID PRIMARY KEY,
    content TEXT NOT NULL,
    embedding vector(1536),
    tier memory_tier NOT NULL,
    
    -- Three-component scoring
    importance_score FLOAT NOT NULL,
    recency_score FLOAT NOT NULL,
    relevance_score FLOAT DEFAULT 0,
    combined_score FLOAT GENERATED ALWAYS AS 
        (0.333 * recency_score + 0.333 * importance_score + 0.333 * relevance_score),
    
    -- Consolidation mechanics
    consolidation_strength FLOAT DEFAULT 1.0,
    decay_rate FLOAT DEFAULT 1.0,
    recall_count INTEGER DEFAULT 0,
    recall_probability FLOAT,
    last_recall_interval INTERVAL,
    
    -- Metadata
    created_at TIMESTAMPTZ NOT NULL,
    last_accessed_at TIMESTAMPTZ,
    harvest_source TEXT,  -- 'explicit', 'auto', 'reflection'
    merge_parent_ids UUID[],
    
    -- Indexes
    INDEX idx_combined_score (combined_score DESC),
    INDEX idx_recall_probability (recall_probability),
    INDEX idx_tier_probability (tier, recall_probability)
);
```

### Supporting Tables
```sql
-- Consolidation tracking
CREATE TABLE consolidation_log (
    memory_id UUID REFERENCES memories(id),
    timestamp TIMESTAMPTZ,
    old_strength FLOAT,
    new_strength FLOAT,
    trigger_type TEXT
);

-- Frozen storage with compression
CREATE TABLE frozen_memories (
    id UUID PRIMARY KEY,
    compressed_content BYTEA,  -- zstd compressed
    compression_ratio FLOAT,
    original_size INTEGER,
    metadata JSONB
);

-- Harvesting metadata
CREATE TABLE harvest_sessions (
    id UUID PRIMARY KEY,
    conversation_id TEXT,
    timestamp TIMESTAMPTZ,
    memories_extracted INTEGER,
    patterns_detected JSONB,
    confidence_scores FLOAT[]
);

-- Insights and reflections
CREATE TABLE insights (
    id UUID PRIMARY KEY,
    content TEXT,
    source_memory_ids UUID[],
    importance_multiplier FLOAT DEFAULT 1.5,
    insight_type TEXT,
    created_at TIMESTAMPTZ
);
```

## Performance Characteristics

### Latency Targets (P99)
- **Working Memory**: <1ms (hot cache + optimized indexes)
- **Warm Storage**: <100ms (balanced performance)
- **Cold Archive**: <1s (acceptable for archived data)
- **Frozen Storage**: 2-5s (intentional cognitive delay)
- **Consolidation Calculation**: <10ms per memory
- **Similarity Detection**: <50ms for 1000 memories
- **Harvesting**: <2s for 50 messages

### Throughput Targets
- **Memory Creation**: >1000 ops/sec
- **Deduplication**: 10,000 memories in <30s
- **Tier Migration**: 1000 memories per second
- **Reflection Generation**: <30s for full analysis
- **Pattern Matching**: >10,000 ops/sec

### Storage Efficiency
- **Working → Warm**: 20% size reduction
- **Warm → Cold**: 50% size reduction
- **Cold → Frozen**: 80% size reduction (5:1 compression)
- **Overall Token Reduction**: 90% vs full context
- **Deduplication Savings**: 30% fewer memories

## Monitoring & Analytics

### Key Performance Indicators
```yaml
Memory Health:
  - Consolidation strength distribution
  - Tier distribution percentages
  - Deduplication rate
  - Insight generation frequency

Harvesting Metrics:
  - Memories per conversation
  - Pattern detection accuracy
  - Extraction confidence scores
  - Silent operation success rate

Cognitive Metrics:
  - Average recall probability
  - Forgetting curve accuracy
  - Consolidation effectiveness
  - Reflection quality scores

System Performance:
  - Tier migration latency
  - Search relevance scores
  - Memory retrieval accuracy
  - Storage efficiency ratio
```

## Implementation Priorities

### Phase 1: Foundation (Weeks 1-2)
1. Three-component scoring system
2. Consolidation mechanics
3. Database schema updates

### Phase 2: Collection (Weeks 3-4)
1. Silent memory harvester
2. Multi-stage assessment pipeline
3. Event-triggered scoring

### Phase 3: Intelligence (Weeks 5-6)
1. Semantic deduplication
2. Frozen tier implementation
3. Reflection generator

### Phase 4: Optimization (Weeks 7-8)
1. Performance tuning
2. Integration testing
3. Production deployment

## Security & Privacy Considerations

### Memory Harvesting Privacy
```yaml
User Controls:
  - Opt-in/opt-out toggles
  - Memory visibility settings
  - Deletion rights (GDPR compliant)
  - Export capabilities

Data Protection:
  - End-to-end encryption for sensitive memories
  - User-specific encryption keys
  - Audit trail for all harvesting
  - Automatic PII detection and masking
```

## Future Enhancements

### Planned Features
1. **Multi-Modal Memories**: Support for images, code, documents
2. **Collaborative Memory**: Shared team knowledge bases
3. **Temporal Reasoning**: Time-aware memory relationships
4. **Causal Chains**: Track decision consequences
5. **Memory Personas**: Context-specific memory sets
6. **Active Learning**: Request clarification on uncertain memories

### Research Integration
- Continuous integration of latest memory research
- A/B testing framework for consolidation algorithms
- User study feedback loops
- Performance benchmarking against human memory

## Conclusion

This enhanced architecture transforms the memory system from a simple storage mechanism to a cognitive system that truly "remembers" like humans do - strengthening important memories through use, gracefully forgetting the irrelevant, and generating insights from accumulated knowledge. The silent harvesting capability ensures Claude continuously learns from interactions without interrupting the user experience, while the 4-tier system with consolidation mechanics provides unprecedented efficiency and scale.