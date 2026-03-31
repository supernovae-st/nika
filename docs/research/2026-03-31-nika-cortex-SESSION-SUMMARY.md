# Nika Cortex — Complete Session Summary

> Date: 2026-03-31
> Duration: ~4 hours
> Agents deployed: 29 total
> This document summarizes EVERYTHING discussed and decided in this session.

---

## Part 1: Hermes Agent Analysis (start of session)

### Context
User asked for deep analysis of Hermes Agent by Nous Research, cross-referenced with Nika's capabilities, with an ElevenLabs audio output.

### Research deployed
- 4 web researchers (Hermes deep-dive, agent landscape, Nika positioning, Nous Research ecosystem)
- 7 Haiku explorers (changelog, roadmap, features code, test count, LOC/arch, memory sessions, brand, security)
- Total: 11 agents for Hermes analysis

### Key Hermes findings (verified from GitHub source code)
- **19,376 stars**, 2,342 forks, 15 messaging platforms (not 6 as initially claimed)
- **600+ models** (400 Nous Portal + 200+ OpenRouter), not 200
- **Learning loop**: SKILL.md auto-extraction after 5+ tool calls — verified in source
- **Honcho**: Dialectic user modeling with prefetch pattern — verified
- **MCP serve**: 10 tools exposed, stdio + HTTP transport — verified
- **tinker-atropos**: Real RL training with trajectory compression — verified
- **Bus factor risk**: Teknium = 2,217 of ~2,600 commits
- **Security incident**: Removed compromised `litellm` dependency in v0.5.0
- **Synchronous core**: Agent loop is entirely synchronous (from AGENTS.md)

### Audio produced
- `hermes-vs-nika-analysis.mp3` — 13m15s, Marcus voice, English
- `hermes-vs-nika-fr.mp3` — 17m44s, Marcus voice, French
- ElevenLabs v3 model, 192kbps, BGM + SFX, loudness normalized

### Agent landscape (live GitHub data, March 31 2026)
| Framework | Stars | MCP | Self-Improvement |
|-----------|-------|-----|-------------------|
| Claude Code | 84,987 | Native | No |
| AutoGen | 56,492 | Yes | No |
| CrewAI | 47,665 | Via Composio | No |
| LangGraph | 28,014 | Via ecosystem | No |
| Composio | 27,590 | Indirect | No |
| Mastra | 22,507 | Author+consume | No |
| OpenAI Agents | 20,442 | First-class | No |
| Hermes Agent | 19,376 | Both | **Yes** |
| Nika | 4 | Native | No (planned: Cortex) |

---

## Part 2: Memory Architecture Research

### Context
User asked about Google's "needle in haystack" tech, best memory approach, whether to use NovaNet or something better. Then escalated to "the most ambitious possible memory system."

### Research deployed
- 5 web researchers (Google context, agent memory architectures, Rust crates, mem0/Zep/GraphRAG, RLM paper)
- 2 Haiku explorers (current Nika memory code, NovaNet concepts extraction)
- Total: 7 agents for memory research

### Key Google findings
- Gemini stays at 1M tokens (no 2M or 10M)
- Infini-attention = research paper only, NOT in production
- ADK Context Compression = shipped (sliding window + LLM summarization)
- File Search = managed RAG (chunking + embedding + semantic search)
- MemoryService = Vertex AI Memory Bank (cloud-only, production-grade)

### Memory landscape findings
| Solution | Stars | Approach | Infra needed |
|----------|-------|----------|-------------|
| mem0 | 51,561 | Vector + Graph (Neo4j) | Qdrant + Neo4j |
| Graphiti (Zep) | 24,373 | Temporal KG | Neo4j/FalkorDB |
| GraphRAG | 31,882 | Community hierarchy | LLM-heavy, batch |
| LightRAG | 31,252 | KG + vector | Flexible |
| Letta (MemGPT) | 21,823 | Virtual context | PostgreSQL |
| Memvid | 13,662 | Single-file, Rust | None |
| ICM | 129 | SQLite+FTS5+vec, Rust | None |

### Critical papers discovered
| Paper | Key Innovation | Impact |
|-------|---------------|--------|
| Kumiho (2603.17244) | Prospective indexing + AGM belief revision | 93.3% LoCoMo |
| D-MEM (2603.14597) | Dopamine-gated memory | -80% tokens |
| GAAMA (2603.27910) | 4-node hierarchy + PageRank | 78.9% LoCoMo-10 |
| TraceMem (2602.09712) | Narrative consolidation | Topic clustering |
| AMA-Agent (2602.22769) | Causal graphs | +11% vs baselines |
| RLM (2512.24601) | Recursive self-invocation | 100x context |
| CoALA (2309.02427) | Canonical cognitive architecture | Working + LTM |
| A-Mem (2502.12110) | Zettelkasten method | 85-93% token reduction |

### Rust projects discovered
| Project | Stars | Key Innovation |
|---------|-------|---------------|
| Shodh | 182 | Hebbian learning, 55ms store, zero LLM calls |
| Vestige | 456 | FSRS-6, synaptic tagging, memory dreaming |
| Grafeo | 463 | 6 query languages, HNSW+BM25 in one, pure Rust |
| ICM | 129 | Dual model (memories/memoirs), typed relations |
| Pensyve | 1 | 6-signal RRF, Bayesian procedural, salience |
| Nocturne | 867 | First-person memory sovereignty |
| Memvid | 13,662 | Single-file memory, Tantivy+HNSW |

### TurboQuant correction
- TurboQuant = KV cache compression for LLM inference (WRONG DOMAIN)
- NOT for vector search/embedding quantization
- usearch i8 = correct choice (98.9% recall, 4x compression)
- RaBitQ = SOTA for vector quant if needed at 10M+ scale
- Decision: usearch i8 sufficient, later replaced by Grafeo HNSW

---

## Part 3: Stack Decisions

### Evolution of stack decisions (chronological)
1. **Initial**: SQLite + FTS5 + sqlite-vec + petgraph + fastembed
2. **After usearch research**: SQLite + FTS5 + usearch (i8) + petgraph + fastembed
3. **After Grafeo NO-GO**: Same as #2, with Grafeo as Phase 2 option
4. **After user insight about forking**: Grafeo GO with fork strategy
5. **FINAL**: Grafeo + rusqlite + fastembed

### Rejected with reasoning
| Crate | Decision | Why |
|-------|----------|-----|
| redb | WRONG ABSTRACTION | KV-only, no FTS, no vector, no multi-process. SQLite beats it for our use case. |
| fjall | WRONG ABSTRACTION | Same as redb + no multi-process support at all |
| sled | DEAD | Last stable release 2021, 170 open issues, in perpetual 1.0 rewrite |
| sqlite-vec | REPLACED | Grafeo has HNSW built-in with Scalar/Binary/Product quantization |
| usearch | REPLACED | Grafeo has HNSW built-in |
| petgraph (for Cortex) | REPLACED | Grafeo has 22 graph algorithms built-in |
| FTS5 (for Cortex) | REPLACED | Grafeo has BM25 built-in |
| turbo-quant | WRONG DOMAIN | KV cache compression, not vector search |
| Kuzu | STALE | C++ FFI, last commit Oct 2025 |
| CozoDB | STALE | Last release Dec 2023 |
| SurrealDB | LICENSE | BSL 1.1, AGPL-incompatible |

### Grafeo detailed evaluation
- **Initial assessment**: NO-GO (64 days old, bus factor 1, 786 downloads)
- **Revised to GO** because:
  1. Only pure Rust doing graph+vector+FTS in one engine
  2. Apache-2.0 = can fork freely
  3. Cypher + SPARQL = ontological self-describing graph
  4. Replaces 3 deps with 1
- **Verified from source code**: ONNX embeddings (load-dynamic), concurrent access (exclusive writer + shared readers), 3 quantization types (Scalar/Binary/PQ), single `.grafeo` file format
- **Fork**: SuperNovae-st/grafeo, pin =0.5.30, sn/nika branch for patches

### FINAL Stack
| Layer | Crate | What it provides |
|-------|-------|-----------------|
| Graph+Vector+FTS | grafeo =0.5.30 | Cypher, GQL, SPARQL, HNSW, BM25, RRF, 22 algos, WAL |
| Metadata | rusqlite (bundled) | FSRS state, access logs, triggers, changelog |
| Embeddings | fastembed 5.13 (opt-in) | Static ONNX Runtime, 35+ models |

---

## Part 4: Cognitive Mechanisms Design

### Current infrastructure audit
- **2,888 LOC reusable** across 12 existing files
- RecordSpec, RecordCompressor, OutputScanner, TokenBudget, EventLog = all reusable
- RecordWriter migrates from NDJSON to Grafeo
- NovaNet COLD tier removed (Cortex is self-contained)

### 22 Cognitive Mechanisms (final count)

**12 Neuroscience-based:**
1. Hebbian strengthening (+2.5%/-10%, floor 0.05, half-life 24h)
2. Dual decay (FSRS-6 + ACT-R + Bjork dual-strength)
3. Dopamine gate (Surprise × Utility threshold)
4. Prospective indexing (write-time LLM anticipation)
5. Narrative consolidation (episodes → threads, sleep replay)
6. Contradiction detection (AGM belief revision)
7. Salience encoding (4-factor: novelty, importance, extremity, specificity)
8. Feedback correction (wrong recall → closed-loop learning)
9. Synaptic tagging (retroactive importance boost, 6h window)
10. Interference detection (proactive/retroactive, cosine>0.9)
11. Auto-linking (Zettelkasten, cosine>0.6 → edges)
12. Conditional triggers (pattern-based auto-recall)

**7 Psychology-based:**
13. Peak-End compression (-60-80% episodic volume)
14. Dunning-Kruger correction (sparse topic → penalty)
15. Deframing (strip valence at write, store neutral canonical)
16. Anti-echo-chamber (log diminishing returns on exposure)
17. Dual-process retrieval (System 1 fast / System 2 deep)
18. Zeigarnik priority (incomplete/failed = higher recall)
19. Challenger mechanism (anti sunk-cost, Bayesian only)

**3 Anti-bias (from social proof research):**
20. Adversarial retrieval (15% budget for contradicting evidence)
21. Endowment correction (30% fresh data floor, 1.3x novelty boost)
22. Goal gradient recall (broad early → focused late)

### 15 Sub-mechanisms
- Valence dimension on surprise (failures encoded stronger)
- Curiosity score (learnability × relevance for gap-filling)
- Flow-based task assignment (challenge-skill matching)
- Two-tier mandatory/voluntary memory architecture
- Coverage-weighted confidence (epistemic uncertainty)
- Importance^1.5 × Urgency^0.8 weighting
- Narrative coherence check (anti-confabulation)
- Alternative narrative generation
- Competence trajectory tracking (trend, not just level)
- Memory visibility model (private/shared/global)
- Exploration bonus (epsilon-greedy in retrieval)
- Contradiction premium (contradictions get EXTRA weight)
- 3:1 revision threshold (calibrated stubbornness)
- Path diversity (shortest + one longer alternative)
- Homeostatic scaling (normalize outlier edge weights)

### 6 Memory Levels
| Level | Type | Decay | Volume |
|-------|------|-------|--------|
| L0 | Working | Session end | Transient |
| L1 | Episodic | FSRS-6 fast | High |
| L2 | Semantic | ACT-R slow | Medium |
| L3 | Procedural | Utility-based | Low |
| L4 | Reflective | Very slow | Low (auto-generated) |
| L5 | Conceptual | PageRank hubs | Very low (Louvain clusters) |

### 8-Signal Dual-Process Retrieval
- System 1 (fast): Grafeo hybrid (BM25+HNSW+graph) → top-3, satisfice if >0.85
- System 2 (deep): full 8-signal + recursive recall (max depth 3)
- ⓪ Trigger check → ① BM25 → ② HNSW → ③ PageRank → ④ ACT-R → ⑤ Intent → ⑥ Confidence×FSRS → ⑦ Interference → ⑧ Salience
- Post: Dunning-Kruger penalty, importance/urgency weighting, path diversity

### 11-Step Write Pipeline
1. Dedup (blake3 + cosine>0.92)
2. Dopamine gate + valence
3. Deframing (neutral canonical)
4. Salience encoding
5. Peak-End check
6. Contradiction check (AGM, 3:1 threshold)
7. Auto-linking (Zettelkasten)
8. Prospective indexing (LLM, if full processing)
9. Synaptic tagging (6h window)
10. Zeigarnik check (boost unresolved)
11. Persist (Grafeo + SQLite + events)

### 9 Builtin Tools
| Tool | Purpose |
|------|---------|
| nika:remember | 11-step write pipeline |
| nika:recall | 8-signal dual-process retrieval |
| nika:revise | Supersedes chain (never delete) |
| nika:correct | Feedback correction loop |
| nika:consolidate | Merge + contradiction + replay |
| nika:egghead_schema | Auto-evolving ontology (SPARQL) |
| nika:egghead_audit | CSR score + integrity |
| nika:egghead_export | YAML/JSON/NDJSON export |
| nika:egghead_history | Changelog, diff, rollback |

### 13 Innovations
| # | Innovation |
|---|-----------|
| 1 | Cypher-native retrieval |
| 2 | SPARQL ontology self-description |
| 3 | Graph-vector hybrid queries |
| 4 | Louvain auto concept generation |
| 5 | Causal chain discovery (SSSP) |
| 6 | TUI graph panel |
| 7 | Workflow-as-procedural-memory |
| 8 | Memory-guided orchestration |
| 9 | Cortex MCP server |
| 10 | Memory import (Hermes, Claude, ICM) |
| 11 | Cross-workflow memory |
| 12 | Embedding cache in graph nodes |
| 13 | BJ Fogg B=MAP for AI (novel, nobody has done this) |

---

## Part 5: Psychology Integration

### Research deployed
- 10 psychology-focused agents
- 106 patterns extracted from growth.design/psychology
- 30+ academic references (Ebbinghaus 1885 → D-MEM 2026)

### Growth.design 106 patterns — 4 categories
1. **Too Much Information** — 29 patterns (Hick's Law, Confirmation Bias, Priming...)
2. **Not Enough Meaning** — 32 patterns (Social Proof, Curiosity Gap, Flow State...)
3. **Need To Act Fast** — 28 patterns (Loss Aversion, Sunk Cost, Decision Fatigue...)
4. **What Should We Remember** — 17 patterns (Peak-End, Zeigarnik, Chunking, Spacing...)

### Key psychology → Cortex mappings
- Loss aversion → Hebbian asymmetric (+2.5%/-10%)
- Spacing effect → FSRS-6 spaced repetition
- Emotional enhancement → Dopamine gate with valence
- Elaborative interrogation → Prospective indexing
- Chunking + narrative bias → Narrative consolidation
- Cognitive dissonance → Contradiction detection (AGM)
- Testing effect → Feedback correction
- Interference theory → Interference detection
- Prospective memory → Conditional triggers
- Peak-end rule → Peak-End compression
- Dunning-Kruger → Sparse topic penalty
- System 1/System 2 → Dual-process retrieval
- Zeigarnik effect → Failed/incomplete priority boost

### 5 Design Principles from cross-bias synthesis
1. **Cognitive Load Management** — 5-7 evidence slots, progressive compression
2. **Calibrated Stubbornness** — 3:1 ratio for belief revision
3. **Adaptive Retrieval** — satisfice (simple) vs maximize (complex)
4. **Structured Diversity** — penalize generic types, suggest specific
5. **Cautious Narrative** — require causal evidence, tolerate fragmentation

### Novel contributions (nobody else does this)
- BJ Fogg B=MAP for AI agents (Motivation × Ability × Prompt)
- Ebbinghaus forgetting curve in production agent system
- 22-mechanism combined cognitive+behavioral system
- Consumer psychology principles applied to AI memory architecture

---

## Part 6: Concrete Algorithms (verified, licensable)

| Algorithm | LOC | Source | License |
|-----------|-----|--------|---------|
| Spreading activation | ~100 | Rewrite from lucid-core | Own (GPL ref) |
| Personalized PageRank | ~80 | graphops v0.1.3 | MIT ✅ |
| FSRS-6 simplified | ~53 | pensyve-core | Apache-2.0 ✅ |
| RRF merge | ~40 | pensyve-core | Apache-2.0 ✅ |
| AGM belief revision | ~60 | pensyve-core | Apache-2.0 ✅ |
| Louvain clustering | ~60 | graphops label_propagation | MIT ✅ |
| Bayesian reliability | ~15 | Own impl | — |
| blake3 dedup | ~20 | blake3 1.8.4 | MIT+Apache ✅ |
| Cosine similarity | ~10 | Own impl | — |
| **Total core algorithms** | **~440 LOC** | | |

---

## Part 7: Crate Structure (FINAL)

```
tools/nika-cortex/ (~50 files, ~18-22K LOC)
├── src/
│   ├── lib.rs
│   ├── store/ (grafeo.rs, meta.rs, dedup.rs)
│   ├── memory/ (episodic, semantic, procedural, working, reflective, conceptual)
│   ├── cognitive/ (12 mechanisms: hebbian through triggers)
│   │   + (7 psycho: peak_end, dunning_kruger, deframe, echo_chamber, dual_process, zeigarnik, challenger)
│   │   + (3 anti-bias: adversarial, endowment, goal_gradient)
│   ├── retrieval/ (grafeo_query, postprocess, rrf, activation, recursive, assembly)
│   ├── tools/ (9 tools: remember through history)
│   ├── mcp/ (Cortex as MCP server)
│   └── import/ (hermes, claude, ndjson migration)
```

---

## Files Produced This Session

### Research docs
| File | Content |
|------|---------|
| `docs/research/2026-03-31-nika-cortex-FINAL.md` | Master design document |
| `docs/research/2026-03-31-nika-cortex-psychology.md` | Psychology integration (22 mechanisms + references) |
| `docs/research/2026-03-31-nika-cortex-stack.md` | Stack decisions + Grafeo fork strategy |
| `docs/research/2026-03-31-nika-cortex-data-model.md` | Rust structs + SQLite schema |
| `docs/research/2026-03-31-nika-cortex-complete-design.md` | Earlier complete design (superseded by FINAL) |
| `docs/research/2026-03-31-nika-cortex-SESSION-SUMMARY.md` | THIS FILE |
| `docs/research/memory-algorithms-implementation-guide.md` | Concrete algorithms with LOC + licenses |
| `docs/research/2026-03-31-social-motivational-psychology-cortex.md` | Social proof agent findings |
| `docs/research/behavioral-science-ai-agents-memory.md` | Behavioral science + AI papers |
| `docs/research/agent-persistent-memory-2026.md` | Memory landscape survey (existed before) |
| `docs/research/memory-architecture-blueprint.md` | Cognitive science foundation (existed before) |

### Audio
| File | Duration | Voice | Language |
|------|----------|-------|----------|
| `hermes-vs-nika-analysis.mp3` | 13m15s | Marcus | English |
| `hermes-vs-nika-fr.mp3` | 17m44s | Marcus | French |

### Memory
| File | Content |
|------|---------|
| `memory/project_nika_cortex_design.md` | Cross-session Cortex design state |
| `memory/user_open_source_activist.md` | Updated with consumer psychology background |

---

## Decisions Log

| # | Decision | Reasoning |
|---|----------|-----------|
| 1 | Grafeo as graph engine | Only pure Rust graph+vector+FTS. Fork mitigates risk. |
| 2 | Fork at SuperNovae-st/grafeo | Apache-2.0, contribute upstream, maintain if abandoned |
| 3 | rusqlite for metadata | Already in workspace, FSRS state needs frequent updates |
| 4 | fastembed over Grafeo ONNX | fastembed bundles ONNX statically (portable), Grafeo = load-dynamic |
| 5 | No TurboQuant | Wrong domain (KV cache, not vector search) |
| 6 | No redb/fjall | KV-only, no FTS/vector/multi-process |
| 7 | 22 mechanisms | 12 neuro + 7 psycho + 3 anti-bias, each with academic foundation |
| 8 | 6 memory levels | GAAMA-inspired hierarchy with auto-generated L4/L5 |
| 9 | Dual-process retrieval | D-MEM paper validates (96.7% accuracy at fraction of cost) |
| 10 | 11-step write pipeline | Every step has a psychology/neuroscience justification |
| 11 | First-person memory | Agent decides what to remember (Nocturne pattern, SDT autonomy) |
| 12 | NovaNet removed from memory tier | Cortex is self-contained, NovaNet not available |
| 13 | NDJSON → Grafeo migration | Existing RecordWriter pattern, swap backend |
| 14 | Cosine dedup threshold: 0.92 | Updated from 0.85 based on algorithm research |

---

## Next Steps

1. **Fork Grafeo** → SuperNovae-st/grafeo
2. **Implementation plan** → Phase-by-phase with timeline, files, tests
3. **Prototype** → Phase 1 MVP (Grafeo + basic remember/recall)
4. **Tests** → TDD on each cognitive mechanism
5. **Integration** → Wire into nika-engine executor
