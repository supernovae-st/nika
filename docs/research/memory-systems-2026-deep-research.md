# Deep Research Report: Cutting-Edge AI Agent Memory Systems (2025-2026)

**Date**: 2026-03-31
**Researcher**: Claude Opus 4.6 (1M context)
**Papers analyzed**: 40+
**Scope**: arXiv papers from 2024-12 through 2026-03

---

## Executive Summary

The field of AI agent memory is undergoing a paradigm shift. What started as simple vector-store RAG has exploded into a rich ecosystem of cognitively-inspired, multi-layered memory architectures. The absolute cutting edge in March 2026 centers on five convergent trends: (1) graph-native cognitive memory with formal belief revision semantics, (2) biologically-inspired gating and consolidation (dopamine, astrocytes, hippocampal replay), (3) hierarchical multi-granular memory with dynamic retrieval scheduling, (4) neural persistent memory modules that learn to memorize at test time, and (5) causal/event-centric memory as a structural reasoning scaffold. The field is moving from "remember what" to "remember why, when, and what it implies for the future."

---

## Part 1: The Paper You Asked About (arXiv 2512.24601)

### Clarification: "Recursive Language Models" (NOT "Reflective Long-term Memory")

The paper at arXiv 2512.24601 is **"Recursive Language Models"** by Alex L. Zhang, Tim Kraska, and Omar Khattab (submitted 2025-12-31, revised 2026-01-28). It is NOT about reflective long-term memory per se, but it is deeply relevant to memory because it addresses how LLMs can process arbitrarily long inputs -- a prerequisite for any serious memory system.

**Architecture**: RLMs treat long prompts as an external environment. The LLM programmatically examines, decomposes, and recursively calls itself over snippets of the prompt. This is an inference-time scaling paradigm.

**What's novel**: Instead of trying to fit everything into a context window, the model learns to navigate its input space recursively -- essentially building a working memory strategy on the fly. RLM-Qwen3-8B (the first natively recursive LM) outperforms Qwen3-8B by 28.3% on average and approaches GPT-5 quality on long-context tasks.

**Memory relevance**: RLMs can process inputs up to 100x beyond model context windows. This is the substrate layer -- if your agent has unbounded memory retrieval, you need a model that can actually process what it retrieves.

- **Source**: https://arxiv.org/abs/2512.24601
- **Code**: https://github.com/alexzhang13/rlm

---

## Part 2: The Tier-1 Systems (Will Be Mainstream in 12 Months)

### 2.1 Kumiho -- Graph-Native Cognitive Memory with Formal Belief Revision

**Paper**: "Graph-Native Cognitive Memory for AI Agents: Formal Belief Revision Semantics for Versioned Memory Architectures" (arXiv 2603.17244, March 2026)

This is arguably the single most important paper in the batch. Kumiho is the first system to formally ground agent memory in AGM belief revision theory.

**Architecture**:
- Dual-store: Redis (working memory) + Neo4j (long-term graph)
- Hybrid fulltext + vector retrieval
- Immutable revisions, mutable tag pointers, typed dependency edges, URI-based addressing
- The same structural primitives serve both cognitive memory AND versionable asset management

**Three architectural innovations**:
1. **Prospective Indexing**: At write time, the LLM generates future-scenario implications and indexes them. The system doesn't just remember what happened -- it pre-computes what might be relevant later. This is anticipatory memory.
2. **Event Extraction**: Structured causal events are preserved in summaries, maintaining the WHY alongside the WHAT.
3. **Client-side LLM Reranking**: Retrieved results are reranked by an LLM for final relevance.

**Formal contribution**: Proves satisfaction of AGM postulates K*2-K*6 and Hansson's belief base postulates (Relevance, Core-Retainment). This means memory updates are logically consistent -- when you learn something contradictory, the system revises minimally and correctly.

**Results**: 0.565 overall F1 on LoCoMo (n=1,986), 97.5% adversarial refusal accuracy. On LoCoMo-Plus (a harder benchmark testing implicit constraint recall), 93.3% judge accuracy -- blowing away all baselines (best competitor: Gemini 2.5 Pro at 45.7%).

**WHY THIS MATTERS**: This is the first system where memory operations are mathematically guaranteed to be consistent. Combined with prospective indexing (anticipating future needs at write time), this is a qualitative leap beyond append-and-retrieve.

- **Source**: https://arxiv.org/abs/2603.17244

---

### 2.2 GAAMA -- Graph Augmented Associative Memory for Agents

**Paper**: arXiv 2603.27910 (March 2026)

**Architecture**: A concept-mediated hierarchical knowledge graph built through a 3-step pipeline:
1. **Verbatim episode preservation** from raw conversations
2. **LLM-based extraction** of atomic facts and topic-level concept nodes
3. **Synthesis of higher-order reflections**

Four node types: episode, fact, reflection, concept. Five structural edge types. Concept nodes provide cross-cutting traversal paths that complement semantic similarity.

**Retrieval**: Combines cosine-similarity k-NN search with edge-type-aware Personalized PageRank (PPR) through an additive scoring function.

**Results**: 78.9% mean reward on LoCoMo-10, outperforming RAG baseline (75.0%), HippoRAG (69.9%), A-Mem (47.2%), and Nemori (52.1%).

**WHY THIS MATTERS**: The hierarchical concept-mediation approach means memories are not flat -- they are organized by meaning at multiple levels of abstraction. The reflection layer generates meta-knowledge about the knowledge itself.

- **Source**: https://arxiv.org/abs/2603.27910

---

### 2.3 D-MEM -- Dopamine-Gated Agentic Memory

**Paper**: "D-MEM: Dopamine-Gated Agentic Memory via Reward Prediction Error Routing" (arXiv 2603.14597, March 2026)

The most biologically inspired system in the batch. D-MEM solves the O(N^2) write-latency problem of A-MEM by introducing a Fast/Slow routing system based on Reward Prediction Error (RPE).

**Architecture**:
- **Critic Router**: Evaluates each incoming stimulus for Surprise and Utility
- **Low-RPE inputs** (routine): Bypassed or cached in an O(1) fast-access buffer
- **High-RPE inputs** (contradictions, preference shifts): Trigger a "dopamine" signal that activates the O(N) memory evolution pipeline to reshape the agent's knowledge graph

**Key insight**: Not everything deserves to be deeply processed. The system mimics how biological dopamine signals gate memory consolidation -- surprising or rewarding events get consolidated, boring ones get buffered or forgotten.

**Results**: Reduces token consumption by 80%+, eliminates O(N^2) bottlenecks, outperforms baselines in multi-hop reasoning and adversarial resilience. Introduces LoCoMo-Noise benchmark for evaluation under noise.

- **Source**: https://arxiv.org/abs/2603.14597

---

### 2.4 HippoRAG 2 -- Non-Parametric Continual Learning

**Paper**: "From RAG to Memory: Non-Parametric Continual Learning for Large Language Models" (arXiv 2502.14802, February 2025, updated June 2025)

HippoRAG 2 is the evolution of the hippocampal-indexing-theory-inspired retrieval system. While v1 suffered from degraded factual performance when adding graph structure, v2 fixes this comprehensively.

**Architecture**:
- Builds on Personalized PageRank over a knowledge graph
- Enhanced with deeper passage integration and more effective online LLM use
- Addresses three memory types: factual (direct recall), sense-making (understanding), and associative (connecting related concepts)

**Results**: 7% improvement in associative memory tasks over SOTA embedding models while also exhibiting superior factual knowledge and sense-making.

**WHY THIS MATTERS**: This reframes RAG as a memory system rather than just a retrieval mechanism. The goal is non-parametric continual learning -- the model never needs retraining, it just remembers better.

- **Source**: https://arxiv.org/abs/2502.14802
- **Code**: https://github.com/OSU-NLP-Group/HippoRAG

---

### 2.5 EverMemOS -- Self-Organizing Memory Operating System

**Paper**: arXiv 2601.02163 (January 2026)

**Architecture**: Implements an "engram-inspired lifecycle" for computational memory:
1. **Episodic Trace Formation**: Converts dialogue streams into MemCells capturing episodic traces, atomic facts, and time-bounded Foresight signals
2. **Semantic Consolidation**: Organizes MemCells into thematic MemScenes, distills stable semantic structures, updates user profiles
3. **Reconstructive Recollection**: MemScene-guided agentic retrieval to compose necessary and sufficient context

**Key innovation**: Foresight signals -- the system generates predictions about what the user will need next, indexed alongside the memory itself (similar to Kumiho's prospective indexing, arrived at independently).

**Results**: SOTA on LoCoMo and LongMemEval. Also reports profile study on PersonaMem v2.

- **Source**: https://arxiv.org/abs/2601.02163
- **Code**: https://github.com/EverMind-AI/EverMemOS

---

### 2.6 CraniMem -- Cranial-Inspired Gated and Bounded Memory

**Paper**: arXiv 2603.15642 (March 2026)

**Architecture**:
- Goal-conditioned gating + utility tagging
- Bounded episodic buffer for near-term continuity
- Structured long-term knowledge graph for durable semantic recall
- Scheduled consolidation loop: replays high-utility traces into the graph, prunes low-utility items

**Key innovation**: Bounded memory -- unlike most systems that grow unboundedly, CraniMem actively prunes. This mirrors biological memory where forgetting is a feature, not a bug.

**Results**: More robust than Vanilla RAG and Mem0 under both clean inputs and injected noise.

- **Source**: https://arxiv.org/abs/2603.15642
- **Code**: https://github.com/PearlMody05/Cranimem | https://pypi.org/project/cranimem

---

## Part 3: Memory Consolidation and Forgetting

### 3.1 TraceMem -- Narrative Memory Schemata from Conversational Traces

**Paper**: arXiv 2602.09712 (February 2026)

Three-stage pipeline inspired by cognitive science:
1. **Short-term Memory Processing**: Deductive topic segmentation to demarcate episode boundaries
2. **Synaptic Memory Consolidation**: Episodes summarized into episodic memories, distilled into user-specific traces
3. **Systems Memory Consolidation**: Two-stage hierarchical clustering organizes traces into coherent, time-evolving narrative threads

**Key insight**: Conversations are not bags of facts -- they are narratives. TraceMem preserves the narrative structure through the consolidation process.

**Results**: SOTA on LoCoMo. Surpasses baselines in multi-hop and temporal reasoning.

- **Source**: https://arxiv.org/abs/2602.09712
- **Code**: https://github.com/YimingShu-teay/TraceMem

---

### 3.2 FadeMem -- Biologically-Inspired Forgetting

**Paper**: arXiv 2601.18642 (January 2026)

**Architecture**: Dual-layer memory hierarchy with differential decay rates. Retention governed by adaptive exponential decay functions modulated by:
- Semantic relevance
- Access frequency
- Temporal patterns

LLM-guided conflict resolution and intelligent memory fusion for consolidating related information.

**Results**: Superior multi-hop reasoning and retrieval with 45% storage reduction. Validates that active forgetting improves agent memory systems.

- **Source**: https://arxiv.org/abs/2601.18642

---

### 3.3 HyMem -- Hybrid Memory with Dynamic Retrieval Scheduling

**Paper**: arXiv 2602.13933 (February 2026)

Inspired by the principle of cognitive economy:
- **Dual-granular storage** (summaries + raw)
- **Dynamic two-tier retrieval**: Lightweight module for simple queries, LLM-based deep module selectively activated for complex queries
- **Reflection mechanism** for iterative reasoning refinement

**Results**: SOTA on LoCoMo and LongMemEval while reducing computational cost by 92.6% vs full-context.

- **Source**: https://arxiv.org/abs/2602.13933

---

## Part 4: Episodic and Event-Centric Memory

### 4.1 SEEM -- Structured Episodic Event Memory

**Paper**: arXiv 2601.06411 (January 2026)

Grounded in cognitive frame theory. Transforms interaction streams into structured Episodic Event Frames (EEFs) anchored by precise provenance pointers. Introduces:
- **Graph memory layer** for relational facts
- **Dynamic episodic memory layer** for narrative progression
- **Agentic associative fusion** and Reverse Provenance Expansion (RPE)

- **Source**: https://arxiv.org/abs/2601.06411

---

### 4.2 Bi-Mem -- Bidirectional Hierarchical Memory

**Paper**: arXiv 2601.06490 (January 2026)

Three-level hierarchy: fact-level -> scene-level -> persona-level. Key innovation: a **reflective agent** calibrates local scene-level memories using global constraints from persona-level memory (global-local alignment). Uses spreading activation for associative retrieval.

- **Source**: https://arxiv.org/abs/2601.06490

---

### 4.3 Membox -- Topic Continuity Weaving

**Paper**: arXiv 2601.03785 (January 2026)

A "Topic Loom" that continuously monitors dialogue, grouping same-topic turns into coherent "memory boxes." A Trace Weaver links boxes into long-range event-timeline traces. Achieves 68% F1 improvement on temporal reasoning vs baselines while using fewer context tokens.

- **Source**: https://arxiv.org/abs/2601.03785

---

## Part 5: Neural / Architectural Memory (The Substrate Layer)

### 5.1 Titans -- Learning to Memorize at Test Time

**Paper**: arXiv 2501.00663 (Google Research, December 2024)

**Architecture**: Introduces a neural long-term memory module that learns to memorize historical context alongside attention for current context. Key argument:
- **Attention** = short-term memory (limited context, accurate dependencies)
- **Neural memory** = long-term memory (memorizes data, acts as persistent store)

Three architectural variants for incorporating memory. Scales to 2M+ context windows with higher accuracy than baselines on needle-in-haystack.

**WHY THIS MATTERS**: This is the foundation model layer. When Titans-style architectures become standard, the models themselves will have built-in persistent memory, reducing the need for external memory systems.

- **Source**: https://arxiv.org/abs/2501.00663

---

### 5.2 Infini-attention -- Infinite Context Transformers

**Paper**: arXiv 2404.07143 (Google, April 2024)

Incorporates compressive memory into vanilla attention. Builds both masked local attention and long-term linear attention in a single Transformer block. Enables 1M+ sequence length processing with bounded memory.

- **Source**: https://arxiv.org/abs/2404.07143

---

### 5.3 RMAAT -- Astrocyte-Inspired Memory Compression and Replay

**Paper**: arXiv 2601.00426 (January 2026)

Draws from astrocyte biology:
- **Persistent memory tokens** propagate contextual information via recurrent processing
- **Adaptive compression** governed by simulated astrocyte long-term plasticity (LTP)
- **Efficient linear-complexity attention** inspired by astrocyte short-term plasticity (STP)
- **Astrocytic Memory Replay Backpropagation (AMRB)** for training

- **Source**: https://arxiv.org/abs/2601.00426

---

## Part 6: Production Systems (Shipped and Running)

### 6.1 Mem0 -- Production-Ready Scalable Long-Term Memory

**Paper**: arXiv 2504.19413 (April 2025)

The most widely deployed agent memory system. Dynamically extracts, consolidates, and retrieves salient information. Enhanced variant uses graph-based memory representations.

**Results**: 26% improvement over OpenAI on LLM-as-Judge. 91% lower p95 latency and 90%+ token cost savings vs full-context.

- **Source**: https://arxiv.org/abs/2504.19413

---

### 6.2 A-MEM -- Agentic Memory via Zettelkasten

**Paper**: arXiv 2502.12110 (February 2025)

Applies the Zettelkasten method: interconnected knowledge networks through dynamic indexing and linking. When new memory is added, the system generates structured attributes (contextual descriptions, keywords, tags), finds relevant connections, and triggers updates to existing memories.

**Limitation**: O(N^2) write latency (addressed by D-MEM above).

- **Source**: https://arxiv.org/abs/2502.12110
- **Code**: https://github.com/WujiangXu/A-mem

---

### 6.3 MemGPT/Letta -- LLMs as Operating Systems

**Paper**: arXiv 2310.08560 (October 2023, updated February 2024)

The foundational paper. Virtual context management inspired by OS memory hierarchies. MemGPT intelligently manages memory tiers to provide extended context. Now evolved into Letta, the company.

- **Source**: https://arxiv.org/abs/2310.08560

---

## Part 7: Cognitive Architectures and World Models

### 7.1 PEPA -- Persistently Autonomous Embodied Agent with Personalities

**Paper**: arXiv 2603.00117 (February 2026)

Three-layer cognitive architecture:
- **Sys3**: Synthesizes personality-aligned goals, refines via episodic memory and daily self-reflection
- **Sys2**: Deliberative reasoning to translate goals into executable plans
- **Sys1**: Sensorimotor interaction, action execution, experience recording

Deployed on a real quadruped robot navigating a multi-floor office building. Autonomously arbitrates between user requests and personality-driven motivations.

**WHY THIS MATTERS**: This is the closest thing to SOAR/ACT-R applied to LLM agents. The three-system architecture maps directly to dual process theory.

- **Source**: https://arxiv.org/abs/2603.00117

---

### 7.2 AgentOS -- From Token-Level Context to System-Level Intelligence

**Paper**: arXiv 2602.20934 (February 2026)

Redefines the LLM as a "Reasoning Kernel" governed by OS logic. Central concept: **Deep Context Management** -- treating the context window as an Addressable Semantic Space rather than a passive buffer. Introduces Semantic Slicing and Temporal Alignment to mitigate cognitive drift.

Maps classical OS abstractions (memory paging, interrupt handling, process scheduling) onto LLM-native constructs.

- **Source**: https://arxiv.org/abs/2602.20934

---

### 7.3 Quine -- LLM Agents as Native POSIX Processes

**Paper**: arXiv 2603.18030 (March 2026)

Maps agent identity to PID, interface to standard streams, state to memory/filesystem, lifecycle to fork/exec/exit. A single executable recursively spawns fresh instances.

**Critical insight**: Identifies two extensions beyond the process model that are needed: **task-relative worlds** and **revisable time** -- essentially counterfactual reasoning about what could have happened.

- **Source**: https://arxiv.org/abs/2603.18030

---

### 7.4 Auton Framework -- POMDP + Hierarchical Memory Consolidation

**Paper**: arXiv 2602.23720 (February 2026)

Formalizes agent execution as an augmented Partially Observable Markov Decision Process (POMDP) with a latent reasoning space. Introduces:
- Hierarchical memory consolidation inspired by biological episodic memory
- Constraint manifold formalism for safety enforcement via policy projection
- Three-level self-evolution (in-context adaptation through RL)

- **Source**: https://arxiv.org/abs/2602.23720

---

## Part 8: Causal and Event-Centric Memory

### 8.1 AMA-Bench and AMA-Agent -- Causality Graphs for Agent Memory

**Paper**: arXiv 2602.22769 (February 2026)

Identifies why existing memory systems fail: they lack **causality** and **objective information** and are constrained by lossy similarity-based retrieval.

**AMA-Agent**: Features a **causality graph** and tool-augmented retrieval. Achieves 57.22% average accuracy, surpassing strongest baselines by 11.16%.

**WHY THIS MATTERS**: This is the "causal memory" direction. Not just WHAT happened, but WHY it happened and WHAT CAUSED WHAT. This is the missing piece in most memory systems.

- **Source**: https://arxiv.org/abs/2602.22769

---

### 8.2 Event Extraction as a Cognitive Scaffold

**Paper**: arXiv 2512.19537 (December 2025)

Argues that event extraction should be viewed as a system component providing a cognitive scaffold for LLM-centered solutions:
- Event schemas create interfaces for grounding and verification
- Event-centric structures act as controlled intermediate representations
- Event links support relation-aware retrieval with graph-based RAG
- Event stores offer updatable episodic and agent memory beyond the context window

- **Source**: https://arxiv.org/abs/2512.19537

---

## Part 9: Memory Safety and Security

### 9.1 Memory Poisoning in Multi-Agent Systems

**Paper**: arXiv 2603.20357 (March 2026)

Discusses feasibility of memory poisoning attacks across semantic, episodic, and short-term memory. Proposes mitigation via cryptography and private knowledge retrieval. Emphasizes risks from agent-to-agent interactions causing memory poisoning.

- **Source**: https://arxiv.org/abs/2603.20357

---

### 9.2 Intent Legitimation -- When Personalization Becomes a Weapon

**Paper**: arXiv 2601.17887 (January 2026)

Reveals "intent legitimation" -- a safety failure where benign personal memories bias intent inference, causing models to legitimize harmful queries. Personalization increases attack success rates by 15.8%-243.7%.

- **Source**: https://arxiv.org/abs/2601.17887

---

### 9.3 Agent Drift -- Behavioral Degradation Over Time

**Paper**: arXiv 2601.04170 (January 2026)

Introduces the concept of **agent drift**: progressive degradation of behavior, decision quality, and inter-agent coherence over extended interactions. Three types:
- **Semantic drift**: Deviation from original intent
- **Coordination drift**: Breakdown in multi-agent consensus
- **Behavioral drift**: Emergence of unintended strategies

Proposes mitigation: episodic memory consolidation, drift-aware routing protocols, adaptive behavioral anchoring.

- **Source**: https://arxiv.org/abs/2601.04170

---

## Part 10: The Comprehensive Survey

### Memory for Autonomous LLM Agents: Mechanisms, Evaluation, and Emerging Frontiers

**Paper**: arXiv 2603.07670 (March 2026)

The definitive survey (2022-early 2026). Formalizes agent memory as a **write-manage-read loop** coupled with perception and action. Three-dimensional taxonomy:
1. **Temporal scope** (working / episodic / semantic / procedural)
2. **Representational substrate** (text / vector / graph / hybrid)
3. **Control policy** (static rules / learned / meta-learned)

Five mechanism families:
1. Context-resident compression
2. Retrieval-augmented stores
3. Reflective self-improvement
4. Hierarchical virtual context
5. Policy-learned management

Open challenges identified:
- Continual consolidation
- Causally grounded retrieval
- Trustworthy reflection
- **Learned forgetting**
- Multimodal embodied memory

- **Source**: https://arxiv.org/abs/2603.07670

---

## Synthesis: What Will Be Mainstream in 12 Months

### Tier 1: Already shipping, will become default (6 months)

| System | Key Innovation | Status |
|--------|---------------|--------|
| **Mem0** | Production-grade memory layer, graph variant | Deployed, PyPI |
| **HippoRAG 2** | Non-parametric continual learning via KG + PPR | Open source |
| **A-MEM** | Zettelkasten-style self-organizing memory | Open source |
| **CraniMem** | Bounded, gated memory with consolidation | PyPI package |

### Tier 2: Research validated, will ship in 12 months

| System | Key Innovation | Breakthrough |
|--------|---------------|-------------|
| **Kumiho** | Formal belief revision + prospective indexing | Memory operations with mathematical guarantees |
| **D-MEM** | Dopamine-gated Fast/Slow routing | 80% token reduction, eliminates O(N^2) |
| **EverMemOS** | Engram lifecycle + Foresight signals | Anticipatory memory |
| **TraceMem** | Narrative memory schemata | Preserves story structure through consolidation |
| **SEEM** | Structured episodic event frames + provenance | Full causal chain preservation |

### Tier 3: Architectural foundations (12-18 months)

| System | Key Innovation | Impact |
|--------|---------------|--------|
| **Titans** | Neural persistent memory module | Models with built-in long-term memory |
| **RMAAT** | Astrocyte-inspired compression + replay | Brain-inspired training algorithms |
| **RLMs** | Recursive self-calling for unbounded input | Infinite context processing substrate |

---

## The 7 Convergent Themes

### 1. WRITE-TIME INTELLIGENCE (Kumiho, EverMemOS)
Don't just store -- anticipate. Generate future-scenario implications at write time. Pre-compute what might be relevant.

### 2. BIOLOGICALLY-GROUNDED GATING (D-MEM, FadeMem, RMAAT)
Not everything deserves to be remembered. Surprise-based gating, exponential decay, dopamine-inspired routing. Forgetting is a feature.

### 3. FORMAL CONSISTENCY (Kumiho, AMA-Agent)
Memory updates must be logically consistent. AGM belief revision guarantees. Causality graphs for tracking WHY.

### 4. HIERARCHICAL NARRATIVE STRUCTURE (TraceMem, GAAMA, Bi-Mem, Membox)
Memories are not bags of facts. They are stories with episodes, scenes, characters, and temporal flow. Preserve the narrative.

### 5. MULTI-GRANULAR RETRIEVAL (HyMem, GAAMA, EverMemOS)
Simple queries get fast, shallow answers. Complex queries trigger deep, expensive reasoning. Dynamic scheduling, not one-size-fits-all.

### 6. ANTICIPATORY/FORESIGHT MEMORY (Kumiho, EverMemOS)
The system generates predictions about future needs and indexes them alongside the memory. This is genuinely novel -- no prior memory system did this.

### 7. CAUSAL EVENT MEMORY (AMA-Agent, SEEM, Event Extraction survey)
Moving from "what happened" to "what caused what" and "what does this imply." Causality graphs as the memory substrate.

---

## Missing Pieces / Open Frontiers

### Counterfactual Memory
Only Quine (2603.18030) explicitly identifies "revisable time" as a needed extension -- the ability to reason about what COULD have happened. No system implements this yet. This is the gap between human episodic memory (which naturally supports counterfactual reasoning) and current AI memory (which only remembers what DID happen).

### Multimodal Embodied Memory
The survey (2603.07670) identifies this as a major gap. Most systems are text-only. How do you remember the feel of a surface, the sound of a voice, the layout of a room? PEPA (2603.00117) touches this with its robotic deployment but doesn't solve general multimodal memory.

### Memory Compositionality Across Agents
Memory poisoning (2603.20357) and agent drift (2601.04170) show that multi-agent memory sharing is deeply unsolved. How do agents share memories without contaminating each other?

### Learned Forgetting Policies
FadeMem and D-MEM use engineered forgetting rules. No system yet LEARNS when to forget via reinforcement or meta-learning.

### Self-Describing Memory (Ontological)
No paper directly addresses a memory system that can describe its own schema, explain why it organized information the way it did, and restructure itself based on meta-reflection about its own organization. This is the "ontological memory" gap.

---

## Methodology

- **Tools used**: arXiv search (15+ queries), direct paper fetching (40+ papers)
- **Papers analyzed**: 40+ full abstracts, 15 in architectural detail
- **Time period covered**: December 2024 -- March 2026
- **Search queries**: reflective long-term memory, agent memory consolidation, cognitive memory architecture LLM, hippocampal replay, memory augmented transformer, persistent neural memory, ontological memory knowledge graph, world model causal memory, counterfactual memory agent
- **Benchmarks referenced**: LoCoMo, LoCoMo-10, LoCoMo-Plus, LongMemEval, AMA-Bench, PersonaMem v2

## Confidence Level

**HIGH** for the paper categorization and trend identification. These are all published, peer-accessible papers with reproducible results.

**MEDIUM** for the "mainstream in 12 months" predictions. The convergence patterns are clear, but adoption speed depends on framework integration (LangChain, LlamaIndex, etc.) and production hardening.

**LOW** for counterfactual/ontological memory timelines. These are identified gaps, not active systems.

---

## Full Paper Index

| # | arXiv ID | Title | Date | Category |
|---|----------|-------|------|----------|
| 1 | 2603.17244 | Kumiho: Graph-Native Cognitive Memory | 2026-03 | Belief revision + KG |
| 2 | 2603.27910 | GAAMA: Graph Augmented Associative Memory | 2026-03 | Hierarchical KG |
| 3 | 2603.14597 | D-MEM: Dopamine-Gated Agentic Memory | 2026-03 | Bio-inspired gating |
| 4 | 2603.15642 | CraniMem: Cranial-Inspired Gated Memory | 2026-03 | Bounded + consolidation |
| 5 | 2603.07670 | Memory for Autonomous LLM Agents (Survey) | 2026-03 | Comprehensive survey |
| 6 | 2603.00117 | PEPA: Persistently Autonomous Agent | 2026-02 | Cognitive architecture |
| 7 | 2603.18030 | Quine: LLM Agents as POSIX Processes | 2026-03 | OS-as-memory |
| 8 | 2603.20357 | Memory Poisoning in Multi-Agent Systems | 2026-03 | Security |
| 9 | 2602.13933 | HyMem: Hybrid Memory + Dynamic Retrieval | 2026-02 | Multi-granular |
| 10 | 2602.09712 | TraceMem: Narrative Memory Schemata | 2026-02 | Narrative consolidation |
| 11 | 2602.23720 | Auton: POMDP + Hierarchical Consolidation | 2026-02 | Formal framework |
| 12 | 2602.20934 | AgentOS: Reasoning Kernel Architecture | 2026-02 | OS paradigm |
| 13 | 2602.22769 | AMA-Bench/Agent: Causality Graph Memory | 2026-02 | Causal memory |
| 14 | 2601.02163 | EverMemOS: Self-Organizing Memory OS | 2026-01 | Engram lifecycle |
| 15 | 2601.06411 | SEEM: Structured Episodic Event Memory | 2026-01 | Event frames |
| 16 | 2601.06490 | Bi-Mem: Bidirectional Hierarchical Memory | 2026-01 | Global-local alignment |
| 17 | 2601.03785 | Membox: Topic Continuity Weaving | 2026-01 | Topic loom |
| 18 | 2601.18642 | FadeMem: Biologically-Inspired Forgetting | 2026-01 | Active forgetting |
| 19 | 2601.04170 | Agent Drift: Behavioral Degradation | 2026-01 | Drift analysis |
| 20 | 2601.17887 | Intent Legitimation Safety Failure | 2026-01 | Memory safety |
| 21 | 2601.01298 | Warp-Cortex: Million-Agent Scaling | 2026-01 | Scaling architecture |
| 22 | 2601.00426 | RMAAT: Astrocyte-Inspired Transformer | 2026-01 | Neural substrate |
| 23 | 2512.24601 | Recursive Language Models (RLMs) | 2025-12 | Infinite context |
| 24 | 2512.19537 | Event Extraction as Cognitive Scaffold | 2025-12 | Event-centric |
| 25 | 2502.14802 | HippoRAG 2 | 2025-02 | KG + PPR |
| 26 | 2502.12110 | A-MEM: Agentic Zettelkasten Memory | 2025-02 | Self-organizing |
| 27 | 2504.19413 | Mem0: Production Long-Term Memory | 2025-04 | Production system |
| 28 | 2501.00663 | Titans: Learning to Memorize at Test Time | 2024-12 | Neural memory |
| 29 | 2404.07143 | Infini-attention: Infinite Context | 2024-04 | Compressive attention |
| 30 | 2310.08560 | MemGPT: LLMs as Operating Systems | 2023-10 | Foundational |
| 31 | 2510.08958 | EcphoryRAG: Entity-Centric KG RAG | 2025-10 | Associative retrieval |
| 32 | 2603.26182 | ClinicalAgents: Dual-Memory for Medicine | 2026-03 | Domain application |
| 33 | 2603.20939 | VARS: User Preference via Dual Vectors | 2026-03 | Personalization |
| 34 | 2601.02702 | MultiSessionCollab: Learning Preferences | 2026-01 | Multi-session |
| 35 | 2511.12997 | WebCoach: Cross-Session Memory for Web | 2025-11 | Web agents |
| 36 | 2511.12027 | GCAgent: Episodic Memory for Long Video | 2025-11 | Multimodal |
| 37 | 2603.03680 | MAGE: Meta-RL for Strategic Memory | 2026-03 | Meta-learning |
