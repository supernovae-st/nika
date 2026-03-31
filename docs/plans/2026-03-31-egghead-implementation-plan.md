# Nika Memory — Implementation Plan

> Status: DRAFT — to be validated and refined
> Date: 2026-03-31
> Prerequisite: Fork Grafeo at SuperNovae-st/grafeo
> Target: future version (post-stabilization)
> Crate: tools/nika-memory/
> Design doc: docs/research/2026-03-31-nika-cortex-FINAL.md

---

## Overview

6 phases, ordered by dependency. Each phase produces a shippable increment.
Phase 1 is the MVP. Phase 6 is the polish.
P2 and P3 can run in parallel. Everything else is sequential.

---

## Phase 0: Pre-work

### P0.1 Fork Grafeo
- Fork GrafeoDB/grafeo to SuperNovae-st/grafeo
- Clone locally in supernovae-hq/grafeo/
- Create branch sn/nika for our patches
- Verify cargo check --features embedded compiles
- Verify cargo test passes
- Note binary size

### P0.2 Scaffold nika-memory crate
- Create tools/nika-memory/ directory
- Cargo.toml with grafeo =0.5.30 (embedded), rusqlite (workspace), nika-core, nika-event, blake3, chrono, serde, serde_json, tracing
- Optional dep: fastembed behind "embed" feature flag
- Feature flags: default=[], embed, cypher, sparql, full
- Add to workspace members in tools/Cargo.toml

### P0.3 Skeleton files
```
src/
  lib.rs
  store/mod.rs, grafeo.rs, meta.rs, dedup.rs
  memory/mod.rs, node.rs, edge.rs, types.rs, evidence.rs
  cognitive/mod.rs
  retrieval/mod.rs
  tools/mod.rs
```
Verify: cargo check -p nika-memory passes

---

## Phase 1: Foundation

Goal: Store facts in Grafeo, retrieve by text search. Basic remember and recall.

### P1.1 Data model types (~120 LOC)
File: src/memory/types.rs
- MemoryKind enum (Working, Episodic, Semantic, Procedural, Reflective, Conceptual)
- Realm enum (System, User, Discovered)
- Source enum (Workflow id, User, Inferred, Consolidated)
- EdgeType enum (Supports, Contradicts, Causes, DerivedFrom, SupersededBy, Refines, RelatedTo, PartOf, InstanceOf)
- EdgeFamily enum (Causal, Semantic, Temporal, Structural)
- Tests: serde roundtrip for all types

### P1.2 CortexNode + CortexEdge structs (~210 LOC)
File: src/memory/node.rs
- CortexNode with all fields: id, kind, node_type, realm, content, properties, source, confidence, created_at, updated_at, superseded_by, surprise, utility, salience, anticipations, success_count, failure_count, reliability, embedding
- CognitiveState struct (stored in SQLite): node_id, activation, storage_strength, retrieval_strength, fsrs (FsrsState), access_log

File: src/memory/edge.rs
- CortexEdge: source, target, edge_type, family, weight, created_at

### P1.3 Evidence packets (~80 LOC)
File: src/memory/evidence.rs
- EvidencePacket: node_id, content, relevance, distance, tokens, signal_scores
- SignalScores: bm25, cosine, pagerank, activation, intent, confidence, interference, salience, context_congruence, modality_match (10 signals)
- RecallResult: packets, total_tokens, budget_used, truncated, coverage_score, query_time_ms

### P1.4 Grafeo store wrapper (~300 LOC)
File: src/store/grafeo.rs
- GrafeoStore struct wrapping GrafeoDB
- open(path) - open or create memory.grafeo with exclusive lock
- open_read_only(path) - shared lock for CLI/TUI
- create_node(node) -> NodeId
- get_node(id) -> Option CortexNode
- update_node(id, updates)
- create_edge(edge)
- get_edges(node_id, family) -> Vec CortexEdge
- search_text(query, limit) -> Vec (NodeId, f64) using BM25
- node_count() -> u64
- edge_count() -> u64
- Create GQL text index on content + anticipations fields at first open
- Node labels = MemoryKind name (:Episodic, :Semantic, etc.)
- Edge labels = EdgeType name (:Supports, :Causes, etc.)
- Tests: in-memory CRUD, text search

### P1.5 SQLite metadata store (~250 LOC)
File: src/store/meta.rs
- MetaStore struct wrapping rusqlite Connection
- Schema v1 with tables:
  - cognitive_state (node_id PK, activation, storage_str, retrieval_str, fsrs_*, access_log)
  - trigger_rules (id, pattern, node_id, created_at)
  - memory_changelog (id, action, node_id, details, timestamp)
  - node_types (name PK, realm, parent, schema, source, instance_count, created_at)
- Init builtin types: fact, entity, event, skill, preference (system realm)
- CRUD methods for all tables
- WAL mode + SYNCHRONOUS=NORMAL (same as daemon pattern)
- Tests: schema creation, CRUD, version check

### P1.6 Dedup module (~80 LOC)
File: src/store/dedup.rs
- exact_dedup(content) -> NodeId via blake3 hash
- near_dedup(store, embedding, threshold=0.92) -> Option NodeId
- Tests: exact match, near-match, no match

### P1.7 Memory facade (~150 LOC)
File: src/lib.rs
- Memory struct: grafeo, meta, config
- MemoryConfig: data_dir, embed_model, max_evidence_packets (7), token_budget (2000)
- open(config) -> Memory (writer mode)
- open_read_only(config) -> Memory (reader mode)
- remember(content, kind, source) -> NodeId (basic store, no pipeline yet)
- recall(query, budget) -> RecallResult (BM25 text only, no vectors yet)
- health() -> HealthReport (node count, edge count, db sizes)
- close()
- Tests: in-memory open, remember, recall, health

### P1.8 Wire into nika-engine (~200 LOC)
File: nika-engine/src/runtime/builtin/memory_admin.rs (NEW)
- Register 3 tools: nika:remember, nika:recall, nika:memory
- remember_handler: parse params (content, kind, mode), dispatch to Memory
- recall_handler: parse params (query, budget, mode), dispatch to Memory
- memory_handler: parse mode (schema, audit, history), stub for Phase 6

File: nika-engine/src/runtime/executor/mod.rs
- Initialize Memory on workflow start (if memory feature enabled)
- Pass reference to task executor

File: nika-event/src/lib.rs
- Add event types: MemoryRemembered, MemoryRecalled

### P1 Exit Criteria
- cargo check -p nika-memory passes
- cargo test -p nika-memory passes (all unit tests)
- E2E: workflow calls nika:remember then nika:recall finds it
- memory.grafeo file created on disk
- memory-meta.db file created on disk
- Health check returns node/edge counts

---

## Phase 2: Cognitive Core

Goal: Add 6 foundational cognitive mechanisms that do not require LLM calls.

### P2.1 Constants module (~50 LOC)
File: src/cognitive/mod.rs
- All Hebbian constants (BOOST_HELPFUL=0.025, DECAY_MISLEADING=0.10, IMPORTANCE_FLOOR=0.05, EDGE_HALF_LIFE_HOURS=24.0, MAX_ENTITY_DEGREE=500, POTENTIATION_THRESHOLD=5)
- Gate constants (FULL_PROCESSING_THRESHOLD=0.3, ROUTINE_THRESHOLD=0.1)
- Salience weights (NOVELTY=0.4, IMPORTANCE=0.3, EXTREMITY=0.1, SPECIFICITY=0.2)
- Interference threshold (0.9)

### P2.2 Hebbian strengthening (~100 LOC)
File: src/cognitive/hebbian.rs
- strengthen_edge(edge, helpful: bool) - boost +2.5% or penalize -10%
- decay_edge(edge, hours_elapsed) - exponential decay with 24h half-life
- floor enforced at 0.05
- Per-memory-type asymmetry: episodic 2.5x, semantic 4x, procedural 5x
- Tests: strengthen, penalize, floor, decay, type-specific ratios

### P2.3 FSRS-6 + ACT-R decay (~120 LOC)
File: src/cognitive/decay.rs
- FsrsState struct: difficulty, stability, elapsed, reps, lapses
- retrievability() -> f64: (1 + elapsed/(9*stability))^(-1)
- on_recall() - increase stability, effortful recall bonus (1-R)^0.3
- on_forget() - decrease stability: S * 0.3 * (11-D) / 10
- actr_activation(access_log, now) -> f64: ln(sum(t_j^(-0.5)))
- generation_effect_modifier(source) -> f64: Inferred=1.3, Consolidated=1.2, User=1.0, Workflow=0.9
- Tests: decay curve, recall boost, forget penalty, ACT-R with multiple accesses

### P2.4 Dopamine gate (~80 LOC)
File: src/cognitive/gate.rs
- GateDecision enum: Routine, Intermediate, FullProcessing
- evaluate_gate(surprise, utility, valence) -> (GateDecision, encoding_strength)
- encoding_strength = surprise * (1 + |valence| * 0.5)
- calculate_surprise(store, content, embedding) -> f64: 1.0 - max_cosine_to_existing
- Tests: routine, full, intermediate, negative valence boost

### P2.5 Salience encoding (~60 LOC)
File: src/cognitive/salience.rs
- encode_salience(novelty, importance, extremity, specificity) -> f64
- calculate_novelty(store, embedding) -> f64
- calculate_specificity(store, content) -> f64
- Tests: known inputs, completely new fact = high novelty

### P2.6 Interference detection (~60 LOC)
File: src/cognitive/interference.rs
- detect_interference(results) -> Vec of (index, index) pairs where cosine > 0.9
- apply_interference_penalty(results, pairs) - penalize older item in each pair
- Tests: similar results penalized, dissimilar untouched

### P2.7 Feedback correction (~80 LOC)
File: src/cognitive/feedback.rs
- record_correction(grafeo, meta, wrong_id, correct_content) -> new NodeId
- Creates new correct node + Contradicts edge + Hebbian penalty on wrong node
- Logs correction in changelog
- Tests: correction creates node + edge, weights decrease

### P2.8 Wire cognitive into remember/recall
- Update remember: after dedup, call gate, calculate salience, store cognitive state
- Update recall: apply interference detection, load cognitive state, apply FSRS retrievability, update access_log + FSRS on_recall (testing effect)

### P2 Exit Criteria
- Hebbian weights change on co-access
- FSRS retrievability decays, increases on recall
- Gate filters routine from full processing
- Salience scores novel facts higher
- Interference penalizes similar results
- Feedback creates Contradicts edges
- All constants match paper values

---

## Phase 3: Deep Retrieval

Goal: Full 10-signal dual-process retrieval with RRF fusion.
Can run IN PARALLEL with Phase 2.

### P3.1 Embeddings integration (~80 LOC)
File: src/store/embed.rs (feature-gated behind "embed")
- Embedder struct wrapping fastembed TextEmbedding
- new(model_name) - defaults to BGE-small-en-v1.5 (384d, 33MB)
- embed(texts) -> Vec of Vec f32
- embed_one(text) -> Vec f32
- Fallback: embed_noop() -> None when no feature

### P3.2 Grafeo vector + hybrid search (~100 LOC)
File: src/store/grafeo.rs (extend)
- create_vector_index(label, property, dimensions)
- search_vector(embedding, limit) -> Vec (NodeId, f64) via HNSW
- hybrid_search(text, embedding, limit) -> Vec (NodeId, bm25, cosine)
- Tests: embed, index, search, hybrid

### P3.3 ACT-R spreading activation (~100 LOC)
File: src/retrieval/activation.rs
- spreading_activation(grafeo, meta, seed_nodes, depth=3, decay=0.7) -> HashMap NodeId -> f64
- BFS from seeds, A_j = sum(W_i/n_i) * S_ij * decay^depth
- Tests: 1-hop, 2-hop, 3-hop, decay verified

### P3.4 RRF merge (~50 LOC)
File: src/retrieval/rrf.rs
- reciprocal_rank_fusion(ranked_lists, weights, k) -> Vec (NodeId, f64)
- Formula: RRF(d) = sum w_r / (k + rank_r(d))
- Adaptive k = max(1, candidate_count / 10)
- Tests: two lists merged, weights applied, adaptive k

### P3.5 Signal extractors (~150 LOC)
File: src/retrieval/signals.rs
- signal_intent(query) -> f64: classify Question/Action/Recall/Code/Visual
- signal_confidence_fsrs(node, state) -> f64: confidence * retrievability
- signal_salience(node) -> f64
- signal_context_congruence(node, context) -> f64 (Tulving 1973)
- signal_modality_match(node, query_type) -> f64 (Morris 1977)

### P3.6 Dual-process retrieval (~200 LOC)
File: src/retrieval/mod.rs
- HybridRetriever struct: grafeo, meta, embedder
- recall(query, config) -> RecallResult
- system1: Grafeo hybrid -> if top > 0.85 -> return (satisfice)
- system2: full 10-signal + post-processing
- classify_complexity(query) -> System1 or System2
- Dunning-Kruger penalty in post-processing (< 5 facts -> halve confidence)
- Importance^1.5 * Urgency^0.8 weighting
- Serial position correction (+15% middle items)
- Endowment correction (30% fresh floor, 1.3x novelty)
- Adversarial retrieval (15% budget for contradicting evidence)

### P3.7 Recursive recall (~80 LOC)
File: src/retrieval/recursive.rs
- recursive_recall(retriever, initial_results, max_depth=3) -> Vec ScoredNode
- If top relevance < 0.3: extract entities, re-query, merge, dedup

### P3.8 Context assembly (~100 LOC)
File: src/retrieval/assembly.rs
- AssemblyMode enum: Workflow, Task, Knowledge, Targeted
- assemble_context(grafeo, mode, params, budget) -> RecallResult

### P3.9 Token budget + evidence packets (~80 LOC)
File: src/memory/evidence.rs (extend)
- build_evidence_packets(scored_nodes, token_budget, max_packets=7) -> RecallResult
- Primacy-recency ordering for LLM context

### P3 Exit Criteria
- Vector search works (embed, index, search)
- BM25 + vector + graph in one Grafeo query
- System 1 returns in < 10ms for simple queries
- System 2 with all 10 signals
- RRF merges multiple signal lists
- Recursive triggers on low confidence
- 4 assembly modes work
- Evidence packets respect budget + Miller 7 +/- 2

---

## Phase 4: Write Intelligence

Goal: Full 11-step write pipeline with LLM-powered features.

### P4.1 Deframing (~40 LOC)
File: src/cognitive/deframe.rs
- deframe(content) -> (neutral_content, original_sentiment)

### P4.2 Peak-End compression (~60 LOC)
File: src/cognitive/peak_end.rs
- is_peak_or_end(workflow_id, task_id, surprise, is_final, store) -> PeakEndDecision
- Peak = highest surprise in workflow. End = final task. Neither = compress.

### P4.3 Contradiction detection (~100 LOC)
File: src/cognitive/contradiction.rs
- REVISION_RATIO = 3.0 (need 3x evidence to revise)
- detect_contradictions(store, new_node, candidates) -> Vec Contradiction
- resolve_contradiction(store, meta, new, old) - AGM contraction, SupersededBy edge

### P4.4 Auto-linking (~80 LOC)
File: src/cognitive/autolink.rs
- LINK_THRESHOLD = 0.6 cosine
- auto_link(store, node_id, embedding, max_links=500) -> Vec CortexEdge
- Classify: RelatedTo, Refines, or Contradicts

### P4.5 Prospective indexing (~80 LOC)
File: src/cognitive/anticipation.rs
- prospective_index(content, llm) -> Vec String (async, uses CompressorLlm bridge)
- Only called when gate = FullProcessing

### P4.6 Synaptic tagging (~60 LOC)
File: src/cognitive/tagging.rs
- TAGGING_WINDOW_HOURS = 6.0, SALIENCE_THRESHOLD = 0.7
- retroactive_tag(store, meta, new_node, now) -> Vec NodeId boosted

### P4.7 Zeigarnik priority (~50 LOC)
File: src/cognitive/zeigarnik.rs
- ZEIGARNIK_BOOST = 1.75
- check_open_loops(store, new_node) -> Vec NodeId to boost
- Discharge ONLY on resolution (SupersededBy or Supports edge), NOT on planning

### P4.8 Assemble 11-step pipeline (~250 LOC)
File: src/tools/remember.rs (rewrite)
- remember_pipeline(memory, content, kind, source, mode) -> RememberResult
- Store mode: all 11 steps (dedup, gate+valence, deframe, salience, peak-end, contradiction, autolink, prospective, synaptic, zeigarnik, persist)
- Revise mode: supersede old, create new with full pipeline
- Correct mode: feedback loop (mechanism 8)

### P4 Exit Criteria
- 11 steps run end-to-end
- Routine facts skip steps 3-8
- Contradictions detected and superseded
- Auto-links created
- Anticipations stored and searchable
- Synaptic tagging boosts recent facts
- Zeigarnik: failed workflows higher activation
- Changelog records every mutation

---

## Phase 5: Consolidation

Goal: Background daemon service for memory maintenance.

### P5.1 Consolidation engine (~300 LOC)
File: src/cognitive/consolidation.rs
- ConsolidationEngine struct
- run_cycle() -> ConsolidationReport
- merge_similar() - cosine > 0.92 pairs
- cluster_episodes() - narrative threading via Grafeo community detection
- extract_patterns() - create L4:Reflective from repeated patterns
- detect_communities() - Grafeo Louvain -> L5:Conceptual nodes
- apply_decay() - batch FSRS elapsed + Hebbian decay
- resolve_contradictions() - find unresolved, attempt merge
- 3:1 consolidation ratio enforcement

### P5.2 Daemon integration (~100 LOC)
File: nika-daemon/src/services/memory_admin.rs (NEW)
- MemoryService struct
- run_consolidation_loop() - Poisson-modulated interval (not fixed)
- More accumulated surprise -> shorter interval (hippocampal replay)

### P5.3 Anti-echo-chamber (~60 LOC)
File: src/cognitive/echo_chamber.rs
- exposure_boost(count) -> f64: log(1+count) * base_weight
- echo_chamber_index(store, node_id) -> f64: 0.0-1.0, alert at 0.7
- exploration_bonus() -> f64: epsilon-greedy for rarely-retrieved facts

### P5.4 Goal gradient (~50 LOC)
File: src/cognitive/goal_gradient.rs
- goal_gradient_config(progress) -> RecallConfig
- 0.0 -> broad (k=20, hops=3). 0.9 -> focused (k=5, hops=1). -1.0 (fail) -> reset

### P5.5 Challenger mechanism (~50 LOC)
File: src/cognitive/challenger.rs
- should_rechallenge(node, state) -> bool
- bayesian_reliability(success, failure) -> f64: Beta(1+s, 1+f) mean

### P5 Exit Criteria
- Consolidation merges, clusters, extracts, detects communities, decays
- L4 reflections auto-generated
- L5 concepts auto-generated by Louvain
- Daemon runs on Poisson schedule
- Echo chamber index computed
- Goal gradient modulates retrieval

---

## Phase 6: Polish

Goal: MCP server, import, ontology evolution, audit, history.

### P6.1 nika:memory schema mode (~150 LOC)
File: src/tools/memory_admin.rs (schema section)
- list: all node types with instance counts
- get: schema + properties of a type
- create: define new user type with JSON Schema
- evolve: auto-discover types from untyped facts (cluster by properties, graduate at 10+ instances with confidence > 0.8)

### P6.2 nika:memory audit mode (~150 LOC)
File: src/tools/memory_admin.rs (audit section)
- CSR score (0-100%)
- Checks: orphan nodes, stale facts, dedup candidates, interference pairs, schema violations, coverage gaps, echo chamber risk

### P6.3 nika:memory history mode (~100 LOC)
File: src/tools/memory_admin.rs (history section)
- log: show changelog of mutations
- diff: compare memory state between timestamps
- rollback: reverse changelog entries (create SupersededBy, never delete)

### P6.4 MCP server (~150 LOC)
File: src/mcp/mod.rs + src/mcp/tools.rs
- Expose Memory as MCP server for Claude Code / Cursor
- Tools: memory_remember, memory_recall, memory_status

### P6.5 Import system (~200 LOC)
File: src/import/hermes.rs - Parse SKILL.md (YAML frontmatter + markdown), create L3:Procedural
File: src/import/claude.rs - Parse MEMORY.md index + files, create L2:Semantic
File: src/import/ndjson.rs - Parse .nika/records/*.ndjson, create L1:Episodic (migration)

### P6.6 nika:recall consolidate + export modes (~80 LOC)
File: src/tools/recall.rs (extend)
- mode=consolidate: trigger consolidation manually
- mode=export: dump memory as YAML/JSON/NDJSON artifact

### P6 Exit Criteria
- Schema: list, get, create, auto-evolve
- Audit: CSR score, all checks run
- History: log, diff, rollback
- MCP: external tools query Nika Memory
- Import: Hermes, Claude, NDJSON all work
- Export: YAML/JSON/NDJSON dump

---

## File Inventory (42 files across 6 phases)

```
tools/nika-memory/src/
  lib.rs                         P1    Memory facade
  store/
    mod.rs                       P1    trait MemoryStore
    grafeo.rs                    P1+P3 Grafeo wrapper
    meta.rs                      P1    SQLite metadata
    dedup.rs                     P1    blake3 + cosine dedup
    embed.rs                     P3    fastembed (feature-gated)
  memory/
    mod.rs                       P1    MemoryKind enum
    node.rs                      P1    CortexNode, CognitiveState
    edge.rs                      P1    CortexEdge, EdgeType
    types.rs                     P1    Realm, Source, NodeType
    evidence.rs                  P1+P3 EvidencePacket, RecallResult
  cognitive/
    mod.rs                       P2    Constants
    hebbian.rs                   P2    mechanism 1
    decay.rs                     P2    mechanism 2
    gate.rs                      P2    mechanism 3
    salience.rs                  P2    mechanism 7
    interference.rs              P2    mechanism 10
    feedback.rs                  P2    mechanism 8
    deframe.rs                   P4    mechanism 15
    peak_end.rs                  P4    mechanism 13
    contradiction.rs             P4    mechanism 6
    autolink.rs                  P4    mechanism 11
    anticipation.rs              P4    mechanism 4
    tagging.rs                   P4    mechanism 9
    zeigarnik.rs                 P4    mechanism 18
    consolidation.rs             P5    mechanism 5
    echo_chamber.rs              P5    mechanism 16
    goal_gradient.rs             P5    mechanism 22
    challenger.rs                P5    mechanism 19
    dunning_kruger.rs            P3    mechanism 14
    dual_process.rs              P3    mechanism 17
    adversarial.rs               P3    mechanism 20
    endowment.rs                 P3    mechanism 21
  retrieval/
    mod.rs                       P3    HybridRetriever
    grafeo_query.rs              P3    Grafeo hybrid
    postprocess.rs               P3    Post-filters
    rrf.rs                       P3    RRF merge
    activation.rs                P3    ACT-R spreading
    recursive.rs                 P3    RLM recursive
    assembly.rs                  P3    4 context modes
    signals.rs                   P3    Signal extractors
  tools/
    mod.rs                       P1    Registration
    remember.rs                  P1+P4 nika:remember (3 modes)
    recall.rs                    P1+P3 nika:recall (3 modes)
    memory_admin.rs                   P6    nika:memory (3 modes)
  mcp/
    mod.rs                       P6    MCP server
    tools.rs                     P6    MCP tool defs
  import/
    mod.rs                       P6    Import trait
    hermes.rs                    P6    Hermes SKILL.md
    claude.rs                    P6    Claude MEMORY.md
    ndjson.rs                    P6    Nika NDJSON migration
```

## LOC Estimates

Phase 0: 50 LOC (scaffold)
Phase 1: 1,400 LOC (types + stores + facade + wiring)
Phase 2: 700 LOC (6 cognitive mechanisms)
Phase 3: 1,200 LOC (10-signal retrieval + embeddings + RRF)
Phase 4: 900 LOC (11-step write pipeline + 7 mechanisms)
Phase 5: 700 LOC (consolidation + daemon + 4 mechanisms)
Phase 6: 800 LOC (schema + audit + history + MCP + import)
Tests: ~3,000 LOC (~150 tests)

Total: ~8,750 LOC

Note: lower than initial 18-22K estimate because Grafeo absorbs graph algos, HNSW, BM25, RRF that we would otherwise build ourselves.

## Dependencies

P0 -> P1 (sequential)
P1 -> P2 + P3 (P2 and P3 can run IN PARALLEL)
P2 + P3 -> P4 (both must complete)
P4 -> P5 (sequential)
P5 -> P6 (sequential)

## Integration with existing code

nika-engine: Add nika-memory dep, register 3 tools, init Memory on workflow start
nika-event: Add MemoryRemembered, MemoryRecalled event types
nika-daemon: Add MemoryService for consolidation loop (Phase 5)
nika-core: Optional memory: field on Task AST (Phase 4)
nika-cli: Optional nika memory subcommands (Phase 6)

## Files vs Memory — What lives where

FILES (on disk, user-managed, UNCHANGED):
- .nika.yaml workflows = recipes (declarative instructions) → STAY as files
- .skill.md skills = prompts (text for system prompt) → STAY as files
- traces/ = raw execution logs → STAY as files (Memory engine can READ to extract)
- config.toml = preferences → STAYS
- vault.enc = secrets → STAYS
- cache/ = LLM responses → STAYS
- daemon/ = job scheduler → STAYS (+ MemoryService added)

MEMORY (in memory.grafeo, agent-managed, NEW):
- L1 Episodic nodes = events, sessions, task results (replaces NDJSON records)
- L2 Semantic nodes = facts, entities, relations
- L3 Procedural nodes = skills learned, workflow patterns, reliability tracking
  NOTE: .skill.md FILES stay on disk. But when a skill is USED successfully,
  Memory engine creates a L3 node to track its reliability. The file = the prompt.
  The L3 node = "this prompt works 85% of the time".
- L4 Reflective nodes = auto-generated meta-observations
- L5 Conceptual nodes = auto-generated theme clusters
- All edges (Supports, Contradicts, Causes, DerivedFrom, SupersededBy...)
- All embeddings (HNSW vectors)
- All text indexes (BM25)

METADATA (in memory-meta.db, FAST-CHANGING counters):
- FSRS-6 state per node
- ACT-R access logs
- Trigger rules
- Memory changelog

## What gets deprecated

REMOVED (NDJSON) -> Memory remember (Phase 4)
.nika/records/*.ndjson -> imported into memory.grafeo (Phase 6, import/ndjson.rs)
  Files kept read-only as backup. No new NDJSON written after migration.
nika:records tool -> alias for nika:recall mode=query (Phase 1)
HOT/WARM/COLD naming -> L0-L5 hierarchy (Phase 1, docs only)
NovaNet COLD tier -> removed, Memory is self-contained (Phase 0)

## Test strategy

Unit tests per mechanism: ~60 tests (P1-P5)
Integration (store ops): ~20 tests (P1-P3)
Cognitive mechanism tests: ~30 tests (P2-P5)
Retrieval pipeline tests: ~15 tests (P3)
Write pipeline tests: ~15 tests (P4)
E2E workflow tests: ~10 tests (P1+)
Total: ~150 tests
All use GrafeoDB new_in_memory - no disk, no flakiness.

## Risks

Grafeo breaking changes: Pin =0.5.30, fork, test before upgrade
Grafeo perf at scale: Benchmark at 100K in P3
fastembed binary size: Feature-gated, opt-in
Concurrent access: Test daemon+CLI in P5
Consolidation perf: Benchmark, add time limits
LLM cost for prospective: Only on FullProcessing path (gate filters ~80%)
