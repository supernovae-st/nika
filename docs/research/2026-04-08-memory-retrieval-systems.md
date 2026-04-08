# Research Report: Cutting-Edge Retrieval & Memory Systems (2025-2026)

## Summary

The AI memory and retrieval landscape has undergone a fundamental shift between 2025 and 2026. The field has moved beyond simple vector-based RAG toward biologically-inspired memory architectures, graph-augmented retrieval, agentic self-organizing memory, and MCP-native persistence layers. The most significant trend is the convergence of knowledge graphs + vector search + cognitive science into unified systems that learn, forget, and strengthen memories over time -- mirroring how biological memory actually works.

---

## 1. HippoRAG (now HippoRAG 2)

**Repository**: https://github.com/OSU-NLP-Group/HippoRAG
**Stars**: 3,325 | **Language**: Python | **Papers**: NeurIPS '24 (v1), ICML '25 (v2)

### Architecture: Hippocampal Indexing Theory

HippoRAG is directly modeled on the **Complementary Learning Systems (CLS) theory** of human memory, which describes how the hippocampus and neocortex work together for long-term memory:

| Brain Component | HippoRAG Analog | Function |
|-----------------|-----------------|----------|
| **Neocortex** | LLM (GPT-4o, Llama) | General world knowledge, pattern completion |
| **Parahippocampal Region** | Embedding model (NV-Embed-v2) | Encodes incoming information into dense representations |
| **Hippocampal Index** | Knowledge Graph + Personalized PageRank | Creates sparse, associative connections between concepts |

### How Hippocampal Indexing Works

1. **Offline Indexing**: Documents are processed by an LLM to extract named entities and relationships, forming a knowledge graph (the "hippocampal index"). Each entity becomes a node; relationships become edges.

2. **Online Retrieval**: When a query arrives:
   - The LLM extracts query entities
   - Personalized PageRank (PPR) runs from query entity nodes through the knowledge graph
   - PPR scores propagate through multi-hop paths, naturally surfacing documents connected through chains of relationships
   - Retrieved passages are ranked and returned

3. **Key Innovation**: Unlike standard RAG (which does flat vector similarity), HippoRAG's PageRank traversal discovers **multi-hop associative connections** -- exactly how human memory works when you "remember something related to something related to the question."

### HippoRAG 2 Improvements (Feb 2025)

- **Continual learning**: Can integrate new documents without re-indexing the entire corpus
- **Sense-making**: Handles complex narrative comprehension (NarrativeQA benchmark)
- **Cost efficiency**: Uses significantly fewer resources than GraphRAG or RAPTOR for offline indexing
- Benchmarked across factual memory (NaturalQuestions, PopQA), sense-making (NarrativeQA), and associativity (MuSiQue, 2Wiki, HotpotQA, LV-Eval) -- outperforms on all

### Usage

```python
from hipporag import HippoRAG

hipporag = HippoRAG(save_dir='outputs',
                     llm_model_name='gpt-4o-mini',
                     embedding_model_name='nvidia/NV-Embed-v2')

hipporag.index(docs=documents)
results = hipporag.retrieve(queries=["What county is Erik Hort's birthplace part of?"], num_to_retrieve=2)
```

**Source**: https://arxiv.org/abs/2405.14831 (v1), https://arxiv.org/abs/2502.14802 (v2)

---

## 2. RAPTOR -- Recursive Abstractive Processing for Tree-Organized Retrieval

**Repository**: https://github.com/parthsarthi03/raptor
**Stars**: 1,629 | **Language**: Python | **Paper**: ICLR 2024

### How the Tree is Built

RAPTOR constructs a **hierarchical summarization tree** bottom-up:

1. **Leaf Level**: Text is chunked into passages (the leaves of the tree)
2. **Clustering**: Chunks are embedded and clustered using soft-clustering (Gaussian Mixture Models), allowing a chunk to belong to multiple clusters
3. **Summarization**: Each cluster is summarized by an LLM, creating parent nodes
4. **Recursion**: The summaries themselves are embedded, clustered, and summarized again, repeating until a single root summary exists
5. **Tree Structure**: The result is a multi-level tree where:
   - Leaves = original text chunks
   - Middle nodes = cluster summaries (paragraph-level abstractions)
   - Root = global document summary

### Retrieval Strategies

- **Tree traversal**: Start at the root, navigate down to relevant leaves (top-down)
- **Collapsed tree**: Flatten all nodes into one retrieval pool, search across all abstraction levels simultaneously (often works better)

### Key Insight

Standard RAG only retrieves at one granularity level (chunk size). RAPTOR retrieves at **multiple levels of abstraction simultaneously** -- a question about a detail gets the leaf node, a question about a theme gets a mid-level summary, a question about the whole document gets the root. This is especially powerful for book-length texts where the answer requires understanding context spread across many pages.

**Source**: https://arxiv.org/abs/2401.18059

---

## 3. Cognee -- Cognitive Architecture for AI Memory

**Repository**: https://github.com/topoteretes/cognee
**Stars**: 14,809 | **Language**: Python | **License**: Apache 2.0

### What Makes Cognee Different

Cognee is a **knowledge engine**, not just a vector store. It combines three retrieval paradigms into one system:

1. **Vector Search**: Semantic similarity for fuzzy matching
2. **Knowledge Graph**: Entity-relationship extraction for structured reasoning
3. **Cognitive Science Approaches**: Ontology grounding, relationship evolution over time

### Architecture

```
Documents -> Ingestion Pipeline -> Knowledge Graph + Vector Index
                                          |
                                   Hybrid Search (graph traversal + semantic similarity)
                                          |
                                   AI Agent Context
```

### Key Differentiators

- **Unified ingestion**: Any format (PDF, text, images, audio) goes through a single pipeline
- **Ontology grounding**: Entities are linked to formal ontologies, not just string-matched
- **Cross-agent knowledge sharing**: Multiple agents share the same knowledge graph
- **Learning from feedback**: The graph evolves based on which retrievals were useful
- **Audit trails**: Full traceability via OTEL collector
- **Multi-tenant isolation**: Per-user/per-tenant data separation

### Usage

```python
import cognee

await cognee.add("Cognee turns documents into AI memory.")
await cognee.cognify()  # Build knowledge graph
results = await cognee.search("What does Cognee do?")
```

### Deployment

Supports Cognee Cloud (managed), Modal (serverless), Railway, Fly.io, Render, Docker, and local development. Backed by a Neo4j-compatible graph database + vector store.

**Source**: https://docs.cognee.ai/

---

## 4. MemWalker -- Graph-Based Memory Navigation

**Paper**: "MemWalker: Navigating Memory for Long-Context Understanding" (2023-2024)

### How the Agent Traverses Memory

MemWalker is a research concept (no standalone major GitHub repo) that builds an **interactive memory tree** from long documents:

1. **Tree Construction**: Long text is segmented and organized into a tree structure where leaf nodes are text segments and internal nodes are summaries
2. **Interactive Navigation**: When a query arrives, the LLM acts as a "walker" that:
   - Starts at the root of the memory tree
   - Reads the summary at each node
   - **Decides which child node to descend into** based on relevance to the query
   - Continues walking down until it reaches a leaf with the answer
3. **Backtracking**: If the walker reaches a dead end (irrelevant leaf), it backtracks and tries a different path

### Key Innovation

Unlike flat retrieval (search everything) or tree retrieval (search all levels), MemWalker models retrieval as a **sequential decision problem** where the LLM actively navigates. This makes it work on extremely long contexts (100K+ tokens) where flat search would be overwhelmed with irrelevant results.

The approach is conceptually related to how humans "think through" where an answer might be -- "I remember it was in the chapter about X, in the section about Y..."

**Source**: https://arxiv.org/abs/2310.05029

---

## 5. Adaptive-RAG -- Routing Between Retrieval Strategies

**Paper**: "Adaptive-RAG: Learning to Adapt Retrieval-Augmented Large Language Models through Question Complexity" (NAACL 2024)

### How It Decides Which Retrieval Mode to Use

Adaptive-RAG trains a **lightweight classifier** (typically a small model fine-tuned on query complexity) that routes each incoming question to the appropriate retrieval strategy:

| Query Complexity | Strategy | Example |
|-----------------|----------|---------|
| **Simple** | No retrieval needed | "What is 2+2?" |
| **Moderate** | Single-step retrieval | "Who wrote Hamlet?" |
| **Complex** | Multi-step iterative retrieval | "How did the policies of X influence the economy of Y through Z?" |

### Architecture

```
Query -> Complexity Classifier -> Route to:
  A) Direct LLM generation (no retrieval)
  B) Single-pass RAG (retrieve once, generate)
  C) Iterative RAG (retrieve, reason, retrieve more, reason again)
```

### Key Insight

Most RAG systems apply the same retrieval pipeline regardless of query difficulty. Simple questions waste compute on unnecessary retrieval. Complex questions fail because one retrieval pass is insufficient. Adaptive-RAG solves this by **adapting the retrieval depth to the question complexity**.

The classifier is trained on silver labels: queries are automatically labeled as simple/moderate/complex based on whether they can be answered correctly by no-retrieval, single-retrieval, or multi-retrieval pipelines.

**Source**: https://arxiv.org/abs/2403.14403

---

## 6. Agentic RAG Patterns in 2026

### Self-RAG (Self-Reflective RAG)

**Repository**: https://github.com/AkariAsai/self-rag | **Stars**: 2,353

Self-RAG trains the LLM itself to generate **special reflection tokens** that control retrieval:

- `[Retrieve]` / `[No Retrieve]` -- Should I search for information?
- `[IsRelevant]` / `[IsNotRelevant]` -- Was the retrieved passage useful?
- `[IsSupportive]` / `[IsNotSupportive]` -- Does the passage support my generation?

The LLM becomes its own critic, deciding when retrieval helps and when it hurts. This eliminates the blind "always retrieve" pattern.

### Corrective RAG (CRAG)

CRAG adds a **knowledge correction step** after retrieval:

1. Retrieve documents
2. A lightweight evaluator scores each document's relevance
3. If documents are **correct** -> use them
4. If documents are **ambiguous** -> decompose query and search again
5. If documents are **incorrect** -> fall back to web search

The correction step catches the most common RAG failure mode: the retriever returns plausible-looking but actually irrelevant documents.

### Speculative RAG

Inspired by speculative decoding. Multiple "draft" retrievals happen in parallel across different retrieval strategies. A verifier model then picks the best result. This achieves the quality of expensive multi-step retrieval at the latency of single-step.

### Multi-Hop Retrieval (2026 state)

The most mature implementations now combine:
- **Chain-of-retrieval**: Query -> Retrieve -> Extract sub-question -> Retrieve again -> ...
- **Graph-guided hops**: Use knowledge graph edges to identify what to retrieve next
- **Interleaved reasoning**: LLM reasons between each retrieval step, refining the next query

HippoRAG 2's Personalized PageRank is the most elegant solution here -- it handles multi-hop traversal mathematically rather than requiring sequential LLM calls.

---

## 7. New Projects Combining Multiple Memory Paradigms

### Tier 1: Major Players (10K+ stars)

| Project | Stars | Language | Key Innovation |
|---------|-------|----------|----------------|
| **mem0** | 51,562 | Python | Universal memory layer. +26% accuracy vs OpenAI Memory, 91% faster, 90% fewer tokens. Y Combinator S24. |
| **LightRAG** | 31,260 | Python | Graph + vector hybrid. Faster and cheaper than Microsoft GraphRAG. EMNLP 2025. |
| **Microsoft GraphRAG** | 31,883 | Python | Community detection + hierarchical summarization for global queries. |
| **Letta (ex-MemGPT)** | 21,824 | Python | Stateful agents with self-improving memory. Operating-system metaphor for LLM memory management. |
| **Cognee** | 14,809 | Python | Knowledge engine: vector + graph + cognitive science. See section 3. |
| **GitNexus** | 20,926 | TypeScript | Client-side knowledge graph from GitHub repos. Runs entirely in browser. Zero server. |

### Tier 2: Emerging Systems (1K-10K stars)

| Project | Stars | Language | Created | Key Innovation |
|---------|-------|----------|---------|----------------|
| **RuVector** | 3,713 | Rust | Nov 2025 | Self-learning vector + GNN memory DB. 75 features including Graph RAG, DiskANN, self-optimizing search, PostgreSQL extension. CES 2026 award. |
| **HippoRAG 2** | 3,325 | Python | May 2024 | Hippocampus-inspired. See section 1. |
| **Engram** | 2,084 | Go | Feb 2026 | Persistent MCP memory for coding agents. Single Go binary, SQLite + FTS5. Works with any MCP client. |
| **RAPTOR** | 1,629 | Python | Feb 2024 | Hierarchical tree retrieval. See section 2. |
| **MemoryOS** | 1,299 | Python | May 2025 | OS-inspired 4-module architecture (Storage/Updating/Retrieval/Generation). +49% F1, +46% BLEU-1. EMNLP 2025 Oral. |
| **SAG** | 1,123 | Python | Nov 2025 | SQL-driven RAG engine. Auto-builds knowledge graph during querying (no separate indexing step). |

### Tier 3: Dark Horses -- The Bleeding Edge (100-1000 stars)

| Project | Stars | Language | Created | Why It Matters |
|---------|-------|----------|---------|----------------|
| **Nocturne Memory** | 867 | Python | Dec 2025 | "First-person sovereign memory" -- AI decides what to remember, not a pipeline. URI-based graph routing, conditional triggers, visual diff/rollback dashboard. Anti-RAG philosophy. |
| **A-Mem** | 842 | Python | Jan 2025 | Zettelkasten-inspired agentic memory. Agent dynamically organizes, links, and evolves its own memories. NeurIPS 2025. |
| **Wax** | 686 | Swift | Jan 2026 | Sub-millisecond RAG on Apple Silicon. Metal-optimized, single file, zero server. Pure Swift. |
| **ALucek/agentic-memory** | 520 | Python | Dec 2024 | 4-type cognitive architecture: Working, Episodic, Semantic, Procedural memory. Educational reference implementation. |
| **GraphBit** | 528 | Rust | Jun 2025 | Enterprise agentic framework. Rust core, Python wrapper. Minimal CPU/memory for production multi-agent workflows. |
| **Vestige** | 456 | Rust | Jan 2026 | FSRS-6 spaced repetition + 29 brain modules + 3D dashboard. Cognitive science meets engineering. MCP server. Single 22MB binary. |
| **Shodh-Memory** | 182 | Rust | Dec 2025 | Zero-LLM memory. Hebbian learning + exponential decay + local embeddings. 55ms store time vs ~20s for mem0. Robotics/ROS2 native. |
| **Grafeo** | 463 | Rust | Jan 2026 | Fastest graph DB on LDBC benchmark. Pure Rust, embeddable, 6 query languages (GQL, Cypher, SPARQL, GraphQL, Gremlin, SQL/PGQ). HNSW + hybrid search built-in. |

---

## Architectural Patterns Emerging in 2026

### 1. The MCP Memory Pattern

The dominant architecture for agent memory in 2026 is: **MCP server + SQLite/local storage + no-LLM indexing**. Projects like Engram, Shodh, Vestige, and Nocturne Memory all converge on this:

```
AI Agent (Claude Code, Cursor, etc.)
    | MCP protocol (stdio/HTTP)
Memory Server (single binary)
    | direct writes
Local Storage (SQLite + embeddings)
```

No cloud dependencies. No LLM calls for storage. Sub-100ms latency.

### 2. The Cognitive Forgetting Pattern

Shodh-Memory and Vestige introduce **biologically-inspired forgetting**: memories decay over time unless reinforced by use. This is modeled on FSRS (spaced repetition) and Hebbian learning ("neurons that fire together wire together"). This is a fundamental shift from the "store everything forever" approach of traditional RAG.

### 3. The Hybrid Search Convergence

Every serious system now combines:
- **Vector search** for semantic similarity
- **Graph traversal** for relationship-based reasoning
- **Full-text search (BM25/FTS5)** for exact matching
- **Reranking** (cross-encoder) for final scoring

LightRAG, Cognee, RuVector, and Grafeo all implement this pattern.

### 4. First-Person vs Third-Person Memory

A philosophical divide has emerged:
- **Third-person** (mem0, Cognee, Zep): System extracts and stores memories from conversations
- **First-person** (Nocturne, Engram, A-Mem): Agent actively decides what to remember and how to organize it

The first-person approach produces more useful memories but requires agent cooperation.

### 5. The Rust Wave

Performance-critical memory systems are being rewritten in Rust: RuVector, Shodh-Memory, Vestige, Grafeo, GraphBit. The pattern is: Rust core for storage/indexing/search, Python or MCP bindings for agent integration.

---

## The Research Frontier -- Things Most People Do Not Know About

### 1. TITANS Architecture (Google DeepMind, late 2024)

Transformers with **neural long-term memory that learns at test time**. Instead of external memory stores, the model's weights themselves are modified during inference to encode new information. This could eventually make external RAG obsolete for many use cases. Still experimental -- the `pafos-ai/titans-trainer` repo (7 stars) is the only open implementation.

### 2. Nocturne Memory's "Sovereign AI" Philosophy

Most memory systems serve the *user*. Nocturne explicitly designs memory to serve the *agent's own identity*. The agent stores "shame logs," identity protocols, and mission statements -- first-person cognitive structures, not user preference databases. This is philosophically radical in the AI memory space.

### 3. Shodh-Memory's Zero-LLM Approach

While every other memory system (mem0, Cognee, Zep) makes 2-3 LLM API calls per memory operation, Shodh stores a memory in 55ms with zero LLM involvement. It uses local ONNX embeddings, mathematical decay curves, and Hebbian learning. This makes it viable for robots and edge devices.

### 4. HippoRAG 2's Continual Learning Without Re-Indexing

Most graph-based RAG systems require rebuilding the entire graph when new documents arrive. HippoRAG 2 incrementally updates its knowledge graph, making it the first practical solution for continuously evolving knowledge bases.

### 5. SAG's Query-Time Graph Construction

SAG (1,123 stars) does not pre-build a knowledge graph. It constructs relevant subgraphs **at query time** from SQL-indexed data. This eliminates the expensive offline indexing step that makes GraphRAG impractical for many use cases.

---

## Landscape Map

```
                         COMPLEXITY OF RETRIEVAL
                    Simple                    Complex
                      |                         |
    Flat Vector  -----+-- Standard RAG          |
    Search             |                        |
                       |  Self-RAG / CRAG       |
    Adaptive     ------+-- Adaptive-RAG --------+-- Multi-hop RAG
    Routing            |                        |
                       |                        |
    Tree-Based   ------+-- RAPTOR --------------+-- MemWalker
    Hierarchy          |                        |
                       |                        |
    Graph-Based  ------+-- LightRAG / GraphRAG -+-- HippoRAG 2
    Knowledge          |                        |
                       |                        |
    Cognitive    ------+-- Cognee / A-Mem ------+-- MemoryOS / Vestige
    Architecture       |                        |
                       |                        |
    MCP-Native   ------+-- Engram / Shodh ------+-- Nocturne Memory
    Agent Memory       |                        |
```

---

## Sources

1. [HippoRAG 2](https://github.com/OSU-NLP-Group/HippoRAG) -- 3,325 stars, NeurIPS'24 + ICML'25
2. [RAPTOR](https://github.com/parthsarthi03/raptor) -- 1,629 stars, ICLR 2024
3. [Cognee](https://github.com/topoteretes/cognee) -- 14,809 stars, knowledge engine
4. [MemWalker](https://arxiv.org/abs/2310.05029) -- Interactive memory tree navigation
5. [Adaptive-RAG](https://arxiv.org/abs/2403.14403) -- NAACL 2024, complexity-based routing
6. [Self-RAG](https://github.com/AkariAsai/self-rag) -- 2,353 stars, self-reflective retrieval
7. [mem0](https://github.com/mem0ai/mem0) -- 51,562 stars, universal memory layer
8. [LightRAG](https://github.com/HKUDS/LightRAG) -- 31,260 stars, EMNLP 2025
9. [Microsoft GraphRAG](https://github.com/microsoft/graphrag) -- 31,883 stars
10. [Letta](https://github.com/letta-ai/letta) -- 21,824 stars, stateful agents
11. [RuVector](https://github.com/ruvnet/RuVector) -- 3,713 stars, Rust, self-learning vector DB
12. [Engram](https://github.com/Gentleman-Programming/engram) -- 2,084 stars, Go, MCP memory
13. [MemoryOS](https://github.com/BAI-LAB/MemoryOS) -- 1,299 stars, EMNLP 2025 Oral
14. [A-Mem](https://github.com/WujiangXu/A-mem) -- 842 stars, NeurIPS 2025
15. [Nocturne Memory](https://github.com/Dataojitori/nocturne_memory) -- 867 stars, sovereign AI memory
16. [Wax](https://github.com/christopherkarani/Wax) -- 686 stars, Swift/Metal on-device RAG
17. [Vestige](https://github.com/samvallad33/vestige) -- 456 stars, Rust, FSRS-6 + 29 brain modules
18. [Shodh-Memory](https://github.com/varun29ankuS/shodh-memory) -- 182 stars, Rust, zero-LLM memory
19. [Grafeo](https://github.com/GrafeoDB/grafeo) -- 463 stars, Rust, fastest graph DB on LDBC
20. [SAG](https://github.com/Zleap-AI/SAG) -- 1,123 stars, query-time graph construction
21. [GitNexus](https://github.com/abhigyanpatwari/GitNexus) -- 20,926 stars, browser-side knowledge graph
22. [GraphBit](https://github.com/InfinitiBit/graphbit) -- 528 stars, Rust agentic framework

## Methodology

- Tools used: GitHub API (repo metadata, search), raw README fetching
- Repositories analyzed: 40+
- Papers referenced: 8+
- Time period covered: 2024-2026
- Search queries: "cognitive memory agent," "graph memory rust," "neural memory architecture," "agentic memory," "retrieval augmented generation," "speculative RAG," "corrective RAG," "adaptive RAG," "memory MCP server"

## Confidence Level

**High** for sections 1-6 (established papers with peer review at NeurIPS, ICML, ICLR, EMNLP, NAACL).
**Medium-High** for section 7 (GitHub metrics verified, but some newer projects lack peer-reviewed validation).
**Medium** for the "bleeding edge" discoveries -- these are fast-moving projects where star counts and features can change rapidly.

## Further Research Suggestions

1. **TITANS / Test-Time Training**: Google DeepMind's approach to memory that modifies model weights during inference. Very early stage but could be paradigm-shifting.
2. **Nocturne Memory + Nika integration**: The URI-based graph routing + conditional triggers maps well to Nika's workflow model. Worth investigating as a memory backend for agentic workflows.
3. **Shodh-Memory's Hebbian learning**: The mathematical memory strengthening/decay model could be adapted for Nika's skill/workflow caching.
4. **SAG's query-time graph construction**: Eliminates the offline indexing bottleneck -- relevant for real-time workflow execution.
5. **Grafeo as graph backend**: Pure Rust, embeddable, sub-millisecond queries, 6 query languages. Could replace Neo4j for embedded use cases.
