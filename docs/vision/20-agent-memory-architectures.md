# Agent Memory/Record Architectures — Deep Technical Research

> Raw architectural details from LangGraph, CrewAI, Letta/MemGPT, Mem0, Cognee, and cutting-edge 2025-2026 frameworks.
> Focus: data formats, compression strategies, cross-run persistence, promotion patterns.

**Date**: 2026-03-15 | **Sources**: 25+ pages scraped, 8 Perplexity queries, 6 framework docs

---

## Table of Contents

1. [LangGraph (LangChain)](#1-langgraph-langchain)
2. [CrewAI](#2-crewai)
3. [Letta / MemGPT](#3-letta--memgpt)
4. [Mem0](#4-mem0)
5. [Cognee](#5-cognee)
6. [Other Cutting-Edge Frameworks](#6-other-cutting-edge-frameworks)
7. [Academic Papers (2025-2026)](#7-academic-papers-2025-2026)
8. [Cross-Framework Comparison](#8-cross-framework-comparison)
9. [Sources](#9-sources)

---

## 1. LangGraph (LangChain)

### 1.1 Dual Memory Model

LangGraph separates memory into two fundamentally different systems:

| System | Scope | Purpose | Backend |
|--------|-------|---------|---------|
| **Checkpointer** (short-term) | Thread-scoped | Conversation state per thread | InMemorySaver, SQLite, Postgres, Redis, MongoDB |
| **Store** (long-term) | Cross-thread | User/app data across sessions | BaseStore, InMemoryStore, custom backends |

### 1.2 Checkpointing Architecture

#### Core Data Format

Every checkpoint is a `StateSnapshot` containing:

```python
StateSnapshot(
    values={'foo': 'b', 'bar': ['a', 'b']},      # channel_values: serialized state
    next=(),                                        # next nodes to execute
    config={
        'configurable': {
            'thread_id': '1',
            'checkpoint_ns': '',                    # '' = root, 'node:uuid' = subgraph
            'checkpoint_id': '1ef663ba-28fe-6528-8002-5a559208592c'
        }
    },
    metadata={
        'source': 'loop',                           # 'input' or 'loop'
        'writes': {'node_b': {'foo': 'b', 'bar': ['b']}},
        'step': 2                                   # super-step number
    },
    created_at='2024-08-29T19:19:38.821749+00:00',
    parent_config={                                 # linked list of checkpoints
        'configurable': {
            'thread_id': '1',
            'checkpoint_ns': '',
            'checkpoint_id': '1ef663ba-28f9-6ec4-8001-31981c2c39f8'  # parent
        }
    },
    tasks=()
)
```

#### Internal Checkpoint Structure (Binary)

Serialized as MsgPack (default) with optional encryption:

```python
checkpoint = {
    "channel_values": {
        "messages": serialized_messages,  # MsgPack of AIMessage list
        "custom_state": serialized_dict
    },
    "channel_versions": {
        "messages": "v1.2.3",             # version per channel
        "custom_state": "v4.5"
    },
    "versions_seen": {
        "__start__": {"messages": "v1.2.3"},
        "model_node": {"messages": "v1.2.3", "custom_state": "v4.5"}
    },
    "checkpoint_id": "abc123",
    "parent_checkpoint_id": "def456"       # linked list for time-travel
}
```

#### Super-step Boundaries

Checkpoints are created at every **super-step** boundary. For `START -> A -> B -> END`:
- Checkpoint 0: Empty state + `START` as next
- Checkpoint 1: Input state + `node_a` as next
- Checkpoint 2: After `node_a` + `node_b` as next
- Checkpoint 3: After `node_b` + no next (complete)

Each forms a linked list via `parent_checkpoint_id`, enabling full time-travel.

#### Subgraph Namespacing

```
checkpoint_ns = ""                          # root graph
checkpoint_ns = "node_name:uuid"            # subgraph
checkpoint_ns = "outer:uuid|inner:uuid"     # nested subgraphs (| separator)
```

#### Pending Writes (Fault Tolerance)

When a node fails mid-super-step, LangGraph stores **pending writes** from successful nodes. On resume, successful nodes are NOT re-run.

### 1.3 Checkpointer Backends

| Backend | Package | Use Case | Persistence |
|---------|---------|----------|-------------|
| `InMemorySaver` | built-in | Dev/testing | Ephemeral (lost on restart) |
| `SqliteSaver` | `langgraph-checkpoint-sqlite` | Single-machine | File-based |
| `PostgresSaver` | `langgraph-checkpoint-postgres` | Production multi-machine | PostgreSQL |
| `AsyncPostgresSaver` | same package | Production async | PostgreSQL |
| `MongoDBSaver` | `langgraph-checkpoint-mongodb` | Document-oriented | MongoDB |
| `RedisSaver` | `langgraph-checkpoint-redis` | High-perf caching | Redis |

All implement `BaseCheckpointSaver` with methods:
- `put(config, checkpoint, metadata)` -- save
- `get(config)` -- latest checkpoint for thread
- `list(config)` -- checkpoint history
- `get_tuple(config)` -- checkpoint + metadata

#### Serialization

Default: **MsgPack** binary format. Optional Python-native serialization fallback for custom types. Encryption available via custom serializer wrapper.

### 1.4 Thread Model

```python
config = {"configurable": {"thread_id": "user-123"}}

# Thread isolates ALL state
graph.invoke({"messages": [("user", "Hi, I'm Alice")]}, config)
# Later...
graph.invoke({"messages": [("user", "What's my name?")]}, config)
# -> "Alice" (state persisted in thread)
```

Thread = primary key for checkpoint storage. Without `thread_id`, no persistence.

### 1.5 Long-Term Memory (Store)

The `Store` is a separate system from checkpointing, designed for cross-thread memory.

#### API

```python
from langgraph.store.memory import InMemoryStore

store = InMemoryStore(
    index={
        "dims": 1536,              # embedding dimensions
        "embed": embeddings_model,  # for semantic search
        "fields": ["text"]          # fields to index
    }
)

# Store a memory (namespaced)
store.put(
    namespace=("users", "alice"),
    key="preference-1",
    value={"text": "Prefers dark mode", "importance": 0.8}
)

# Retrieve by key
item = store.get(namespace=("users", "alice"), key="preference-1")

# Semantic search across namespace
results = store.search(
    namespace=("users", "alice"),
    query="UI preferences",
    limit=5
)
```

#### Namespace Model

Namespaces are **tuple-based hierarchies**:
- `("users", "alice")` -- user-specific
- `("teams", "engineering")` -- team-level
- `("global",)` -- application-wide

#### Accessing Store Inside Graph Nodes

```python
from langgraph.graph import StateGraph
from langchain_core.runnables import RunnableConfig

def my_node(state, config: RunnableConfig, *, store):
    # 'store' is injected when graph is compiled with store=
    user_id = config["configurable"]["user_id"]
    memories = store.search(namespace=("users", user_id), query=state["messages"][-1])
    # ... use memories in response
    store.put(namespace=("users", user_id), key="new-fact", value={"text": "..."})
```

#### Memory Types (Cognitive Framework)

LangGraph documents three memory types from the CoALA paper:

| Type | What | Human Analogy | Agent Example |
|------|------|---------------|---------------|
| **Semantic** | Facts/concepts | School knowledge | User preferences, facts |
| **Episodic** | Experiences | Personal events | Past agent actions |
| **Procedural** | Instructions | Motor skills | System prompts, rules |

Semantic memory has two sub-patterns:

- **Profile**: Single continuously-updated JSON document per entity. Risk: overwrites.
- **Collection**: Growing set of individual memory documents. Better recall, harder to manage.

### 1.6 Short-Term Memory Management

Three explicit strategies for managing conversation history:

1. **Trim messages**: Keep only N most recent messages
2. **Delete messages**: Remove specific messages by ID (via `RemoveMessage`)
3. **Summarize messages**: LLM-generated summary replaces old messages

```python
# Summarization pattern
def summarize_conversation(state):
    summary = model.invoke(f"Summarize: {state['messages']}")
    # Delete old messages, keep summary
    delete_messages = [RemoveMessage(id=m.id) for m in state["messages"][:-2]]
    return {"messages": delete_messages + [summary]}
```

### 1.7 Writing Memories: Hot Path vs Background

| Approach | When | Pros | Cons |
|----------|------|------|------|
| **Hot path** | During agent execution | Real-time, consistent | Adds latency |
| **Background** | Async after response | No latency impact | Eventually consistent |

---

## 2. CrewAI

### 2.1 Unified Memory Architecture (v1.10+)

CrewAI has evolved from a 3-type system to a **single unified `Memory` class** that replaces `ShortTermMemory`, `LongTermMemory`, and `EntityMemory`.

#### Core API

```python
from crewai import Memory

memory = Memory()

# 5 cognitive operations:
memory.remember("We decided to use PostgreSQL.")        # encode
matches = memory.recall("What database?", limit=5)      # recall
facts = memory.extract_memories("Long text...")          # extract
memory.forget(scope="/project/old")                      # forget
tree = memory.tree()                                     # consolidate/explore
```

### 2.2 Five Cognitive Operations

| Operation | Purpose | Details |
|-----------|---------|---------|
| **Encode** | Store memory | LLM analyzes content, assigns importance, detects contradictions |
| **Consolidate** | Organize | Self-organizing hierarchical scope tree |
| **Recall** | Retrieve | Adaptive-depth recall with composite scoring |
| **Extract** | Parse | Identifies atomic facts from unstructured text |
| **Forget** | Remove | By age, scope, or relevance threshold |

### 2.3 Composite Scoring System

Recall uses three weighted factors:

```python
memory = Memory(
    recency_weight=0.4,           # how recent
    semantic_weight=0.4,          # how relevant (embedding similarity)
    importance_weight=0.2,        # LLM-assigned importance score
    recency_half_life_days=14,    # decay rate for recency
)

# Score = recency_weight * recency_score
#       + semantic_weight * similarity_score
#       + importance_weight * importance_score
```

### 2.4 Hierarchical Scopes

Memories are organized in a **filesystem-like tree** that grows organically:

```
/
  /company
    /company/engineering
    /company/product
  /project
    /project/alpha
    /project/beta
  /agent
    /agent/researcher
    /agent/writer
```

#### Scope Inference

When `remember()` is called **without** a scope, the LLM analyzes content against the existing tree and suggests placement. New scopes are created automatically.

```python
memory.remember("We chose PostgreSQL.")
# LLM might place under /project/decisions or /engineering/database

memory.remember("Sprint velocity is 42 points", scope="/team/metrics")
# Explicit placement
```

#### MemoryScope (Subtree Views)

```python
agent_memory = memory.scope("/agent/researcher")

# All ops restricted to /agent/researcher subtree
agent_memory.remember("Found three relevant papers.")
agent_memory.recall("relevant papers")

# Narrow further
project_memory = agent_memory.subscope("project-alpha")
# -> /agent/researcher/project-alpha
```

### 2.5 Integration Patterns

```python
# With Crews -- auto extract/inject
crew = Crew(
    agents=[researcher, writer],
    tasks=[research_task, writing_task],
    memory=True,   # or memory=Memory(...)
)
# After each task: extracts discrete facts, stores them
# Before each task: recalls relevant context, injects into prompt

# With Flows -- built-in methods
class ResearchFlow(Flow):
    @start()
    def gather_data(self):
        self.remember(findings, scope="/research/databases")
        return findings

    @listen(gather_data)
    def write_report(self, findings):
        past = self.recall("database benchmarks")
        # ...
```

### 2.6 Underlying Storage

| Component | Technology | Purpose |
|-----------|-----------|---------|
| Vector embeddings | ChromaDB | Semantic search |
| Persistent storage | SQLite3 | Cross-session persistence |
| Scope tree | In-memory + SQLite | Hierarchical organization |

### 2.7 Extract Pattern (Conversation Compression)

```python
raw = """Meeting notes: We decided to migrate from MySQL to PostgreSQL
next quarter. The budget is $50k. Sarah will lead the migration."""

facts = memory.extract_memories(raw)
# -> ["Migration from MySQL to PostgreSQL planned for next quarter",
#     "Database migration budget is $50k",
#     "Sarah will lead the database migration"]

for fact in facts:
    memory.remember(fact)
```

This is CrewAI's approach to **conversation compression**: decompose unstructured text into atomic factual statements, each stored independently with its own scope, importance, and embedding.

---

## 3. Letta / MemGPT

### 3.1 Memory Architecture (OS-Inspired Hierarchy)

Letta implements a **two-tier memory hierarchy** inspired by operating system memory management:

```
+-------------------------------------------+
|           CONTEXT WINDOW (RAM)            |
|                                           |
|  +-------------------------------------+ |
|  | Memory Blocks (Core Memory)          | |
|  | - persona block                      | |
|  | - human block                        | |
|  | - custom blocks (scratchpad, etc.)   | |
|  +-------------------------------------+ |
|  | Message Buffer (recent messages)     | |
|  +-------------------------------------+ |
+-------------------------------------------+
           |                    |
           v                    v
+------------------+  +------------------+
| RECALL MEMORY    |  | ARCHIVAL MEMORY  |
| (Conversation    |  | (Vector DB /     |
|  History Search) |  |  Knowledge Store)|
| Out-of-context   |  | Out-of-context   |
| Auto-persisted   |  | Agent-curated    |
+------------------+  +------------------+
```

### 3.2 Core Memory (Memory Blocks)

Memory blocks are **structured sections of the context window** that persist across all interactions. They are always visible -- no retrieval needed.

#### Block Data Format

Each block consists of:
- `label` (string) -- unique identifier (e.g., "persona", "human", "scratchpad")
- `description` (string) -- describes purpose (critical for agent behavior)
- `value` (string) -- actual content/data
- `limit` (integer) -- max characters (default 5000)
- `read_only` (boolean) -- whether agent can modify

#### How Blocks Appear to the LLM

```xml
<memory_blocks>
  <persona>
    <description>The persona block: Stores details about your current persona...</description>
    <metadata>
      - chars_current=128
      - chars_limit=5000
    </metadata>
    <value>I am a helpful assistant named Sam. I enjoy helping users solve problems.</value>
  </persona>
  <human>
    <description>The human block: Stores key details about the person you are conversing with...</description>
    <metadata>
      - chars_current=84
      - chars_limit=5000
    </metadata>
    <value>The user's name is Alice. She is a software engineer who prefers concise answers.</value>
  </human>
</memory_blocks>
```

#### Block Operations (Agent Tools)

Agents manage their own memory through built-in tools:
- `memory_insert(block_label, content)` -- append new info (concurrent-safe)
- `memory_replace(block_label, old_str, new_str)` -- targeted edit (mostly safe)
- `memory_rethink(block_label, new_value)` -- full rewrite (last-writer-wins)

#### Shared Blocks

Multiple agents can access the **same physical block**. Update once, visible everywhere:

```python
shared_block = client.blocks.create(
    label="organization",
    description="Shared information between all agents.",
    value="Company policies and procedures..."
)

agent1 = client.agents.create(block_ids=[shared_block.id], ...)
agent2 = client.agents.create(block_ids=[shared_block.id], ...)
# Both see the same block in their context windows
```

### 3.3 Recall Memory (Conversation History)

Full conversation history stored in a database, searchable but NOT in context window.

- **Auto-persisted**: Every message (user, assistant, system, tool calls, tool returns) is saved
- **Searchable**: Via `conversation_search` tool
- **Cross-conversation**: All conversations within an agent are pooled together

When the context window fills, messages are **evicted** (compacted) but remain searchable.

### 3.4 Archival Memory (Knowledge Store)

Semantically searchable vector DB for intentional, long-term storage:

```python
# Agent tool call
archival_memory_insert(
    content="Deckard retired six replicants in the off-world colonies",
    tags=["replicant", "history", "retirement"]
)

# Search returns semantically relevant results
results = archival_memory_search(
    query="replicant lifespan",
    tags=["technical"],
    page=0
)
```

Key characteristics:
- **Agent-immutable** (append-only from agent perspective, developers can edit via SDK)
- **Unlimited storage**
- **Semantic search** (meaning-based, not keyword)
- **Tagged organization**

### 3.5 Conversations API (Parallel Threads)

A single agent can have **multiple conversations** running in parallel:

- Each conversation has its **own context window** (messages processed independently)
- All conversations **share memory blocks** (update in one, visible in all)
- All conversations **share searchable message history** (conversation_search spans all)
- Long conversations get **compacted independently**

```python
conversation = client.conversations.create(agent_id="agent-xxx")

# Model override per conversation or per request
conv = client.conversations.create(agent_id="agent-xxx", model="openai/gpt-5-mini")
stream = client.conversations.messages.create(
    conv.id,
    messages=[{"role": "user", "content": "Hello"}],
    override_model="anthropic/claude-haiku-4-5",  # per-request override
)
```

### 3.6 Evolution from MemGPT: Heartbeat Deprecation

The original MemGPT paper used a **heartbeat pattern** where:
- Every action was a tool call (including `send_message` for assistant responses)
- Heartbeats triggered memory management operations
- Memory compression happened inline during conversations

**Letta V1 (`letta_v1_agent`) deprecated this**:
- Assistant messages are now native (no `send_message` tool)
- Heartbeats removed
- Memory management via **sleep-time agents** (async, non-blocking)

### 3.7 Sleep-Time Compute

Instead of inline memory management, Letta now supports **async memory agents**:

- **Non-blocking**: Memory management happens asynchronously
- **Proactive refinement**: Memory blocks reorganized during idle periods
- **Better quality**: Not constrained by real-time response latency
- **Specialized agents**: Dedicated agents for memory curation

### 3.8 Message Eviction & Summarization

When context window reaches capacity:

1. **Evict** ~70% of oldest messages (keep recent for continuity)
2. **Recursive summarization**: Evicted messages summarized alongside existing summaries
3. Older messages have progressively less influence on summary
4. All evicted messages remain searchable via recall memory

### 3.9 Database Backend

Letta stores all state (memory blocks, messages, archival passages) in a persistent database:
- All messages persisted to disk automatically
- Evicted messages still retrievable via API and agent tools
- Server-side persistence (no client-side state management needed)

---

## 4. Mem0

### 4.1 Memory Layer Architecture

Mem0 separates memory into four distinct layers:

```
+-----------------+  +-----------------+  +-----------------+  +-----------------+
| Conversation    |  | Session Memory  |  | User Memory     |  | Org Memory      |
| (single turn)   |  | (minutes-hours) |  | (weeks-forever) |  | (global config) |
+-----------------+  +-----------------+  +-----------------+  +-----------------+
      |                    |                    |                    |
      +--------------------+--------------------+--------------------+
                           |
                    Mem0 Retrieval Layer
                    (merges all layers on query)
```

| Layer | Lifetime | Best For | Trade-offs |
|-------|----------|----------|------------|
| Conversation | Single response | Tool execution detail | Lost after turn |
| Session | Minutes to hours | Multi-step flows | Manual clear |
| User | Weeks to forever | Personalization | Needs consent |
| Org | Global config | Shared knowledge | Needs curation |

### 4.2 Core Data Format

Memories are stored as **atomic facts** extracted from conversations:

```python
from mem0 import Memory

memory = Memory(api_key=os.environ["MEM0_API_KEY"])

# Add memory with scope identifiers
memory.add(
    ["I'm Alex and I prefer boutique hotels."],
    user_id="alex",
    session_id="trip-planning-2025",
)

# Search merges all layers, user memory ranked first
results = memory.search(
    "Any hotel preferences?",
    user_id="alex",
    session_id="trip-planning-2025",
)
```

### 4.3 Memory Lifecycle (Capture-Promote-Retrieve)

1. **Capture**: Messages enter conversation layer during active turn
2. **Promote**: Relevant details persist to session or user memory based on `user_id`, `session_id`, and metadata
3. **Retrieve**: Search pipeline pulls from all layers, ranking: user > session > history

### 4.4 Storage Backend

| Component | Technology | Purpose |
|-----------|-----------|---------|
| Vector store | Qdrant, Chroma (OSS) | Semantic similarity |
| Graph store | Neo4j (Pro tier) | Entity relationships |
| Reranker | Cross-encoder models | Result quality |

### 4.5 Memory Operations

```python
# Add
memory.add(messages, user_id="user-1", session_id="session-1")

# Search (semantic + metadata filtering)
results = memory.search(query, user_id="user-1")

# Update (specific memory by ID)
memory.update(memory_id, data="Updated content")

# Delete
memory.delete(memory_id)
memory.delete_all(user_id="user-1")  # bulk delete
```

### 4.6 Graph Memory (Pro Tier)

Builds entity relationships alongside vector embeddings:
- Entities extracted from conversations
- Relationships tracked (subject-predicate-object triples)
- Traversal queries alongside semantic search
- Handles temporal reasoning

### 4.7 Key Design Choice: Atomic Fact Extraction

Unlike systems that store raw messages, Mem0 **extracts atomic facts**:

```
Input: "I'm Alex, I work at Google, and I prefer boutique hotels"
->
Memory 1: "User's name is Alex"
Memory 2: "User works at Google"
Memory 3: "User prefers boutique hotels"
```

Each fact gets its own embedding, metadata, and version history. This enables:
- Surgical updates (change one fact without touching others)
- Better retrieval precision
- Contradiction detection (new fact vs existing)

---

## 5. Cognee

### 5.1 Extract-Cognify-Load Pipeline

Cognee processes data through three phases:

1. **Extract**: Pull from 30+ data sources (files, APIs, databases)
2. **Cognify**: Transform into structured knowledge:
   - Chunking
   - Embedding generation
   - Graph-based extraction (subject-relation-object triples)
   - Contradiction resolution
3. **Load**: Store in hybrid graph + vector format

### 5.2 Hybrid Architecture: Knowledge Graph + Vector Search

```
+-------------------+     +-------------------+
| Knowledge Graph   |     | Vector Store      |
| (Entities +       |     | (Embeddings +     |
|  Relationships)   |     |  Semantic Search)  |
+-------------------+     +-------------------+
         |                         |
         +------------+------------+
                      |
              Graph-Aware Retrieval
              (multi-hop + semantic)
```

| Component | Backend Options | Purpose |
|-----------|----------------|---------|
| Knowledge Graph | Kuzu, Memgraph | Structured relationships |
| Vector Store | LanceDB, Qdrant | Semantic similarity |
| Persistent DB | PostgreSQL + pgvector | Primary storage |

### 5.3 Memify: Post-Deployment Memory Optimization

**Key innovation**: Memify treats post-processing as a first-class pipeline:

- Clean stale nodes and strengthen associations
- Adjust structure without full rebuilds
- **Feedback loops**: Rated responses aggregate into edge weights
- Reweight important facts for better retrieval

Memory improves over time through real usage, not just initial ingestion.

### 5.4 Associative Memory (2025-2026)

Recent addition: **Associative MCP Memory**:
- Dynamic associations between concepts in real-time
- Contextual learning adapts based on usage patterns
- Cross-domain linking connects info across domains

### 5.5 Key Differentiator

Cognee goes beyond traditional RAG by providing:
- **ACID-style guarantees** for memory operations
- **Contradiction resolution** when new facts conflict with existing
- **Multi-tenancy** with per-user isolation
- **RBAC, API keys, audit logs** for enterprise

---

## 6. Other Cutting-Edge Frameworks

### 6.1 A-MEM (Zettelkasten-Style Graph Memory)

Academic framework that replaces flat memory stores with **Zettelkasten-style graph networks**:
- Typed relationships between memory nodes
- 26% improvement over OpenAI baselines on LLM-as-Judge metrics
- Spreading activation enables associative recall
- Graph outperforms vectors in multi-hop/temporal reasoning

### 6.2 ACC (Adaptive Cognitive Compression)

Mimics brain consolidation:
- Compresses short-term memories into long-term during "sleep-like" processes
- Improves efficiency and stability
- Prevents catastrophic forgetting

### 6.3 Titans Architecture

Learned long-term memory module:
- Modules update at different speeds (fast context, medium consolidation, slow core)
- Prioritizes surprising information
- Enables multi-million token contexts via test-time memorization

### 6.4 Hindsight Framework

Agent-managed memory system:
- Agents self-edit memory blocks
- Agent-driven promotion decisions between working context and archives
- Inspired by OS memory hierarchies

### 6.5 Constitutional Memory Architecture (CMA)

From arXiv:2603.04740v1 (March 2026):
- **Memory-as-Ontology** paradigm: memory as foundation of digital agent existence
- Four-layer governance hierarchy
- Multi-layer semantic storage
- **Digital Citizen Lifecycle** for persistent identity across model changes
- Outperforms Mem0, Letta, Zep in long-term scenarios

### 6.6 Zep (Separate from Mem0)

Dedicated memory framework:
- Persistent storage + retrieval + summarization
- Long-term context with hybrid systems
- Task-oriented but lacks governance layer

### 6.7 OpenAI Agents SDK

Current state management:
- Basic context replay (RAG-style injection)
- Stateful ReAct loops with memory across steps
- Graph/workflow orchestration
- Limited compared to dedicated memory frameworks

### 6.8 Microsoft AutoGen

Memory integration:
- Short/long-term for multi-agent learning
- Event-driven with memory from prior runs
- Azure-integrated persistence
- Learning from interactions across sessions

---

## 7. Academic Papers (2025-2026)

### Key Papers

| Paper | Source | Key Contribution |
|-------|--------|-----------------|
| **Memory in the Age of AI Agents** | arXiv:2512.13564 | Comprehensive survey: taxonomies of memory forms (token-level, parametric, latent), functions (factual, experiential, working), dynamics (formation, evolution, retrieval) |
| **Constitutional Memory Architecture** | arXiv:2603.04740v1 | Memory-as-Ontology paradigm, 4-layer governance, Digital Citizen Lifecycle, outperforms Mem0/Letta/Zep |
| **A-MEM** | 2025 | Zettelkasten graph memory, 26% improvement over baselines, spreading activation |
| **ACC (Adaptive Cognitive Compression)** | 2025 | Brain-like consolidation, sleep-phase compression |
| **Titans Architecture** | 2025 | Multi-speed memory modules, test-time memorization |
| **CoALA** | arXiv:2309.02427 | Foundational: maps human memory types to AI agent memory |

### Research Trends

1. **Graph > Vector**: Knowledge graphs outperform pure vector stores for multi-hop reasoning
2. **Agent-managed memory**: Agents deciding what/when to remember (not just passive storage)
3. **Biologically-inspired**: Consolidation, forgetting, separate systems for different memory types
4. **Governance layers**: Identity continuity and trust, not just retrieval
5. **Async memory management**: Sleep-time compute, background memory agents

---

## 8. Cross-Framework Comparison

### 8.1 Architecture Patterns

```
                    In-Context        Out-of-Context       Cross-Thread
                    (Always visible)  (Retrieved on-demand) (Shared state)
                    +-----------+     +-----------+         +-----------+
LangGraph           | State     |     | Store     |         | Store     |
                    | (channels)|     | (namespace)|        | (namespace)|
                    +-----------+     +-----------+         +-----------+

CrewAI              | (scope    |     | ChromaDB  |         | Shared    |
                    |  tree)    |     | + SQLite  |         | scopes    |
                    +-----------+     +-----------+         +-----------+

Letta               | Memory    |     | Archival  |         | Shared    |
                    | Blocks    |     | + Recall  |         | Blocks    |
                    +-----------+     +-----------+         +-----------+

Mem0                | Conv      |     | Session + |         | Org       |
                    | history   |     | User      |         | memory    |
                    +-----------+     +-----------+         +-----------+

Cognee              | (none -   |     | Graph +   |         | Multi-    |
                    |  stateless)|    | Vector    |         | tenant    |
                    +-----------+     +-----------+         +-----------+
```

### 8.2 Data Format Comparison

| Framework | Short-Term Format | Long-Term Format | Compression |
|-----------|-------------------|------------------|-------------|
| **LangGraph** | MsgPack checkpoints (channel_values/versions) | JSON docs in Store (namespaced) | Trim/delete/summarize messages |
| **CrewAI** | Scope tree + ChromaDB vectors | SQLite records + embeddings | LLM extract_memories() to atomic facts |
| **Letta** | XML memory blocks in prompt + message buffer | Vector passages (archival) + full message history (recall) | Recursive summarization on eviction (~70%) |
| **Mem0** | Raw conversation messages | Atomic facts with embeddings + optional graph triples | Fact extraction from unstructured text |
| **Cognee** | N/A (stateless) | Knowledge graph + vector embeddings | ECL pipeline with contradiction resolution |

### 8.3 Persistence Strategies

| Framework | Thread/Session | Cross-Session | Cross-Agent |
|-----------|---------------|---------------|-------------|
| **LangGraph** | Checkpoint linked list per thread_id | Store with custom namespaces | Store with shared namespace |
| **CrewAI** | Scope subtree per agent/flow | SQLite persisted scopes | Shared scope paths |
| **Letta** | Conversation with own context window | Agent-level memory blocks + archival | Shared blocks (block_ids) |
| **Mem0** | session_id scoping | user_id scoping | org_id scoping |
| **Cognee** | N/A | PostgreSQL + pgvector | Multi-tenancy with RBAC |

### 8.4 Promotion Patterns

| Framework | How Data Moves Between Tiers |
|-----------|------------------------------|
| **LangGraph** | Manual: developer writes to Store in node logic. Background: async memory writing after response. |
| **CrewAI** | Automatic: after each task, crew extracts facts and stores them. Manual: agent calls remember(). Forget: time/scope-based. |
| **Letta** | Agent-driven: agent uses memory tools to self-edit blocks. Sleep-time: async agents curate memory during idle. Eviction: auto-summarize when context full. |
| **Mem0** | Automatic: fact extraction on add(). Scope-based: user_id/session_id determines layer. Search merges all layers. |
| **Cognee** | Pipeline: ECL extracts, cognifies, loads. Memify: post-deployment optimization with feedback loops. |

### 8.5 Compression Strategy Comparison

| Strategy | Used By | Mechanism | Trade-offs |
|----------|---------|-----------|------------|
| **Message trimming** | LangGraph | Keep N most recent messages | Lossy, simple |
| **Message summarization** | LangGraph, Letta | LLM generates summary of old messages | Quality depends on LLM |
| **Recursive summarization** | Letta | Summary of summary (cascading) | Progressive info loss |
| **Atomic fact extraction** | CrewAI, Mem0 | LLM decomposes text into discrete facts | Best precision, more storage |
| **Graph extraction** | Cognee, Mem0 Pro | Subject-predicate-object triples | Rich relationships, complex |
| **Block rewriting** | Letta | Agent rewrites entire memory block | Agent-managed quality |
| **Scope-based forgetting** | CrewAI | Drop memories in old scopes | Coarse-grained |
| **Eviction with archival** | Letta | Move from context to searchable DB | Never truly lost |

### 8.6 Database Backend Comparison

| Framework | Vector DB | Graph DB | Relational DB | Cache |
|-----------|-----------|----------|---------------|-------|
| **LangGraph** | Via Store (custom) | None native | Postgres, SQLite, MongoDB | Redis |
| **CrewAI** | ChromaDB | None | SQLite | None |
| **Letta** | Built-in (archival) | None | Server-managed | None |
| **Mem0** | Qdrant, Chroma | Neo4j (Pro) | None | None |
| **Cognee** | LanceDB, Qdrant | Kuzu, Memgraph | PostgreSQL + pgvector | Pre-computed subgraphs |

---

## 9. Sources

### Official Documentation (Scraped)

1. LangGraph Persistence: https://docs.langchain.com/oss/python/langgraph/persistence
2. LangGraph Memory: https://docs.langchain.com/oss/python/langgraph/add-memory
3. LangGraph Memory Concepts: https://docs.langchain.com/oss/python/langgraph/memory
4. CrewAI Memory: https://docs.crewai.com/en/concepts/memory
5. Letta Memory Blocks: https://docs.letta.com/guides/core-concepts/memory/memory-blocks
6. Letta Archival Memory: https://docs.letta.com/guides/core-concepts/memory/archival-memory
7. Letta Shared Memory: https://docs.letta.com/guides/core-concepts/memory/shared-memory
8. Letta Conversations: https://docs.letta.com/guides/core-concepts/messages/conversations
9. Letta Stateful Agents: https://docs.letta.com/guides/core-concepts/stateful-agents
10. Mem0 Memory Types: https://docs.mem0.ai/core-concepts/memory-types
11. Mem0 Platform Overview: https://docs.mem0.ai/platform/overview

### Blog Posts & Articles

12. Letta Blog: Agent Memory -- https://www.letta.com/blog/agent-memory
13. CrewAI Blog: Cognitive Memory -- https://crewai.com/blog/how-we-built-cognitive-memory-for-agentic-systems
14. The Checkpoint Ledger Behind LangGraph -- https://zalt.me/blog/2025/12/checkpoint-ledger-langgraph
15. Cognee: From RAG to Graphs -- https://memgraph.com/blog/from-rag-to-graphs-cognee-ai-memory
16. Cognee + LanceDB Case Study -- https://lancedb.com/blog/case-study-cognee/

### Academic Papers

17. Memory in the Age of AI Agents -- arXiv:2512.13564
18. Constitutional Memory Architecture (CMA) -- arXiv:2603.04740v1
19. CoALA: Cognitive Architectures for Language Agents -- arXiv:2309.02427
20. Agent Memory Paper List (GitHub) -- https://github.com/Shichun-Liu/Agent-Memory-Paper-List

### Comparison Articles

21. 6 Best AI Agent Memory Frameworks (2026) -- https://machinelearningmastery.com/the-6-best-ai-agent-memory-frameworks-you-should-try-in-2026/
22. 8 Best AI Agent Memory Systems Compared -- https://vectorize.io/articles/best-ai-agent-memory-systems
23. Top Agentic AI Frameworks 2026 -- https://www.alphamatch.ai/blog/top-agentic-ai-frameworks-2026

---

*Research compiled for Nika Evolution brainstorm series. All data from primary sources (official docs, papers) cross-referenced where possible.*
