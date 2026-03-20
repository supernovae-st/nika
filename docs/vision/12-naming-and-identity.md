# 12 -- Naming & Identity

> One Piece-inspired naming for Nika architecture, evolved to descriptive agent presets.
> v0 philosophy: no backward compatibility, no legacy, just rename.

**Status:** APPROVED | **Date:** 2026-03-14 | **Updated:** 2026-03-20

---

## Origin

Nika (the project) is named after the Sun God Nika from One Piece -- the Hito Hito no Mi, Model: Nika. The fruit of execution, freedom, and imagination. The runtime is the **body** that does.

The rest of the SuperNovae architecture maps deeply onto Dr. Vegapunk's satellite system from the Egghead arc.

---

## The Mapping

```
+===============================================================================+
|                    ONE PIECE  <->  SUPERNOVAE                                  |
+===============================================================================+
|                                                                                |
|  Nika (Sun God Fruit)          ->  Nika (Runtime)                              |
|  "Limited only by imagination"     "Limited only by the YAML you write"        |
|  Execution - Freedom - Joy         Execution - Freedom - Workflows             |
|                                                                                |
|  Stella (corps original)       ->  L'utilisateur                               |
|  La conscience centrale             La volonte qui ecrit les workflows          |
|  Dirige les satellites              Dirige les agents et l'execution            |
|                                                                                |
|  Punk Records                  ->  Punk Records (tier WARM)                    |
|  Cerveau externalise de Vegapunk    Memoire locale sur disque (NDJSON)         |
|  Accumule le savoir des satellites  Accumule les Records des workflow runs     |
|  Personnel a Vegapunk               Personnel a Nika (.nika/records/)          |
|                                                                                |
|  Egghead Island (laboratoire)  ->  RunContext (in-memory run data)             |
|  Lab ephemere de Vegapunk           Memoire in-memory d'un run                 |
|  Detruit pendant l'arc              Detruit a la fin du workflow                |
|  Satellites y travaillent           Tasks y stockent leurs resultats            |
|                                                                                |
|  Shaka (PUNK-01, Sagesse)      ->  orchestrate (dynamic orchestration)         |
|  Leader de facto des satellites     LLM qui dispatch les agents                 |
|  Prend les decisions strategiques   Decide quelles tasks lancer                 |
|  Le plus rationnel et prescient     Raisonne sur les Records accumules          |
|                                                                                |
|  Edison (PUNK-03, Intelligence) ->  Agent preset: default (creation)            |
|  Inventions et engineering          Generation creative, redaction, code        |
|                                                                                |
|  Pythagoras (PUNK-04, Logique)  ->  Agent preset: think (raisonnement)          |
|  Analyse logique et computation     Extended thinking, analyse profonde         |
|                                                                                |
|  Atlas (PUNK-05, Force)         ->  Agent preset: lite (execution rapide)       |
|  Puissance physique brute           Tasks structurees, rapides, pas cheres      |
|                                                                                |
|  York (PUNK-06, Ressources)     ->  Agent preset: search (recherche)            |
|  Collecte de ressources             Recherche web, RAG, collecte d'info        |
|                                                                                |
|  Lilith (PUNK-02, Defense)      ->  Security layer (doc only)                   |
|  Protege Egghead des intrus         Guardrails, blocklist, path traversal      |
|                                                                                |
|  Den Den Mushi (transmissions)  ->  Pas renomme (traces/events)                 |
|  Vegapunk Broadcast             ->  Pas renomme                                 |
|  Poneglyphs (inscriptions)      ->  Pas renomme (workflows restent workflows)   |
|  Emet (Iron Giant)              ->  Pas renomme (exec: reste exec:)             |
|                                                                                |
+===============================================================================+
```

---

## Naming Evolution: Vegapunk -> Descriptive Presets

The original Vegapunk naming (edison/atlas/york/pythagoras) was cohesive with lore but created
onboarding friction. The evolved naming uses **descriptive agent presets** that are instantly
understandable without learning One Piece lore:

| Vegapunk Name | Descriptive Preset | Cognitive Role |
|---------------|-------------------|----------------|
| edison | `default` | Primary creative work -- generation, writing, orchestration |
| atlas | `lite` | Fast execution, structured tasks, formatting |
| pythagoras | `think` | Deep reasoning, planning, critique, review |
| york | `search` | Search, retrieval, data collection |
| -- | `vision` | Visual analysis, OCR, image understanding |
| -- | `judge` | Quality evaluation, scoring, validation |
| -- | `coder` | Code generation, review, execution |
| -- | `summary` | Compression, summarization, extraction |

The 8-preset system expands beyond the original 4 Vegapunk slots to cover all functional roles
that workflows actually dispatch to. See Doc 21 for the full research validation.

---

## Decisions de renaming

### Confirmes

| Ancien | Nouveau | Scope | Raison |
|--------|---------|-------|--------|
| `Episode` | `Record` | Struct, YAML, tools, docs | Punk Records -- memoire compressee |
| `episodes` | `records` | Partout | Coherence |
| `DataStore` | `RunContext` | Struct Rust interne | Industry standard name (voir Appendix), extended with vector_index (Doc 17) |
| `orchestration: strategy` | `orchestration: orchestrate` | YAML, Rust | Dynamic orchestration mode (ex-Shaka) |
| `strategy` (alias) | accepte | Parser YAML | Backward-friendly pour onboarding |
| `tactics` | `satellites` | YAML templates | Dispatches par l'orchestrateur |
| `main` (slot) | `default` | YAML + Rust | Primary creative agent preset |
| `tactical` (slot) | `lite` | YAML + Rust | Fast tactical execution |
| `reasoning` (slot) | `think` | YAML + Rust | Deep reasoning agent preset |
| `search` (slot) | `search` | YAML + Rust | Search and retrieval (unchanged) |
| `fast` (slot) | `lite` | YAML + Rust | Renamed for clarity |
| `reason` (slot) | `think` | YAML + Rust | Renamed for clarity |
| `code` (slot) | `coder` | YAML + Rust | Code generation agent preset |
| `nika:episodes` | `nika:records` | Introspection tool | Coherence Records |
| `nika:strategy_state` | `nika:orchestrate` | Introspection tool | Coherence orchestration |

**Cross-references:**
- **Appendix (ex-Doc 14)**: DataStore -> RunContext validated (industry standard name)
- **Doc 17**: RunContext extended with `vector_index` field for nika:search (native RAG)
- **Doc 21**: Model routing & agents design (8-preset validation)

### Pas touche

| Concept | Raison |
|---------|--------|
| 5 verbes (infer, exec, fetch, invoke, agent) | Sacres -- ADR-001 |
| Traces / Events / EventLog | Technique pure, pas de lore dans l'observabilite |
| `security.rs` | Clarte > esthetique pour la securite. Lilith = doc seulement |
| Workflows (fichiers `.nika.yaml`) | Restent "workflows", pas poneglyphs |
| `McpClient` | Infrastructure MCP standard |
| `orchestration: dag` | Nom technique neutre pour le mode statique |
| `nika:dag_state` | Explicite pour un tool d'introspection |
| `nika:budget`, `nika:task_status`, `nika:context` | Utilitaire, pas de lore |

---

## YAML avant / apres

### Agent Presets (unified agents: block)

```yaml
# AVANT (Vegapunk naming, separate model_slots:)
model_slots:
  edison: claude-sonnet-4-20250514          # PUNK-03
  atlas: claude-haiku-35             # PUNK-05
  pythagoras: claude-sonnet-4-20250514      # PUNK-04
  york: perplexity/sonar-pro         # PUNK-06

# APRES (descriptive presets, unified agents: block)
agents:
  default:
    provider: anthropic
    model: claude-sonnet-4-20250514          # Primary creative work
  lite:
    provider: groq
    model: llama-3.3-70b-versatile    # Fast tactical execution
  think:
    provider: anthropic
    model: claude-sonnet-4-20250514          # Deep reasoning
    extended_thinking: true
  search:
    provider: deepseek
    model: deepseek-chat              # Search & retrieval
  vision:
    provider: openai
    model: gpt-4o                     # Visual analysis
  judge:
    provider: anthropic
    model: claude-sonnet-4-20250514          # Quality evaluation
  coder:
    provider: anthropic
    model: claude-sonnet-4-20250514          # Code generation
  summary:
    provider: groq
    model: llama-3.3-70b-versatile    # Compression
```

### Orchestration (ex-Shaka)

```yaml
# AVANT
orchestration: strategy
strategy:
  provider: anthropic
  model: claude-sonnet-4-20250514
  max_rounds: 10
  episode_budget: 15000

tactics:
  research:
    infer: "Research the topic"
    episode: { compress: true, max_tokens: 300 }
  write:
    infer: "Write the content"
    episode: { compress: true, retain: [content] }

# APRES
orchestration: orchestrate               # Dynamic orchestration mode
orchestrate:
  provider: anthropic
  model: claude-sonnet-4-20250514
  max_rounds: 10
  record_budget: 15000                   # budget total des Records

satellites:                              # templates dispatchees par l'orchestrateur
  research:
    infer: "Research the topic"
    record: { compress: true, max_tokens: 300 }
  write:
    infer: "Write the content"
    record: { compress: true, retain: [content] }
```

### Records (ex-Episodes)

```yaml
# AVANT
tasks:
  - id: research
    infer: "Research quantum computing"
    episode:
      compress: true
      max_tokens: 500
      confidence_threshold: 0.8

# APRES
tasks:
  - id: research
    infer: "Research quantum computing"
    record:
      compress: true
      max_tokens: 500
      confidence_threshold: 0.8
```

### Introspection Tools

```yaml
# AVANT
tools:
  - nika:episodes        # Records accumules
  - nika:strategy_state  # Etat de la strategie

# APRES
tools:
  - nika:records         # Records accumules (Punk Records)
  - nika:orchestrate     # Etat de l'orchestrateur
```

---

## Rust avant / apres

### RunContext (ex-DataStore)

```rust
// AVANT
pub struct DataStore {
    results: DashMap<String, TaskResult>,
    context: DashMap<String, Value>,
}

// APRES
pub struct RunContext {
    results: DashMap<String, TaskResult>,
    context: DashMap<String, Value>,
    vector_index: Option<VectorIndex>,  // Doc 17: native RAG
}
```

File rename: `src/store/datastore.rs` -> `src/store/run_context.rs`

### Record (ex-Episode)

```rust
// AVANT
pub struct Episode {
    pub task_id: String,
    pub summary: String,
    pub key_findings: Vec<String>,
    pub confidence: f64,
    pub tokens_used: usize,
}

// APRES
pub struct Record {
    pub task_id: String,
    pub summary: String,
    pub key_findings: Vec<String>,
    pub confidence: f64,
    pub tokens_used: usize,
}
```

Files: `src/runtime/episode.rs` -> `src/runtime/record.rs`
`src/runtime/episode_compress.rs` -> `src/runtime/record_compress.rs`

### Agent Presets (ex-ModelSlots)

```rust
// AVANT (Vegapunk naming)
pub struct ModelSlots {
    pub edison: ModelConfig,      // PUNK-03
    pub atlas: ModelConfig,       // PUNK-05
    pub pythagoras: ModelConfig,  // PUNK-04
    pub york: ModelConfig,        // PUNK-06
}

// APRES (descriptive presets)
pub enum AgentPreset {
    Default,    // Primary creative work
    Lite,       // Fast tactical execution
    Think,      // Deep reasoning
    Search,     // Search & retrieval
    Vision,     // Visual analysis
    Judge,      // Quality evaluation
    Coder,      // Code generation
    Summary,    // Compression
}
```

### Orchestration (ex-Shaka)

```rust
// AVANT (planned)
pub enum OrchestrationMode {
    Dag,
    Strategy,
}

pub struct StrategyRunner { ... }

// APRES
pub enum OrchestrationMode {
    Dag,
    Orchestrate,
}

pub struct OrchestrateRunner { ... }
```

---

## Codebase Impact Analysis

### Existing code (v0.27 -- needs rename NOW)

| Rename | Occurrences | Files | Criticite |
|--------|-------------|-------|-----------|
| `DataStore` -> `RunContext` | **668** | 29 (19 src + 10 tests) | **CRITIQUE** |
| `strategy` (DecomposeStrategy) | 51 | 13 | MOYEN (attention: `BackoffStrategy` n'est PAS a renommer) |
| `episode` | 17 | 1 (tier6.rs exemples) | FAIBLE |
| `tactics` | 0 | 0 | Pas encore implemente |
| `model slots` | 0 | 0 | Pas encore implemente |

### Planned code (v0.28-v0.30 -- use new names directly)

| Feature | Fichiers a creer | Naming |
|---------|------------------|--------|
| Agent presets (v0.28) | `src/runtime/agent_presets.rs` | `AgentPreset { Default, Lite, Think, Search, Vision, Judge, Coder, Summary }` |
| Record compression (v0.28) | `src/runtime/record.rs`, `src/runtime/record_compress.rs` | `Record`, `RecordCompressor` |
| Orchestration (v0.29) | `src/runtime/orchestrate.rs`, `src/runtime/orchestrate_runner.rs`, `src/ast/orchestrate.rs` | `OrchestrateRunner`, `OrchestrateConfig`, `Satellite` |
| Context budgets (v0.29) | `src/runtime/budget.rs` | Pas de rename (technique) |
| Punk Records (v0.28) | `src/runtime/record_log.rs` | `RecordLog` (NDJSON on disk, .nika/records/) |
| NovaNet memory (v0.30) | `src/runtime/promote.rs` | `Record` node class + `RecordLog::promote()` |
| Introspection (v0.30) | 6 builtin tools | `nika:records`, `nika:orchestrate`, rest technique |

### Detailed file impact: DataStore -> RunContext

**Tier 1 -- Critical (>50 occurrences):**

| File | Count | Nature |
|------|-------|--------|
| `src/store/datastore.rs` | 164 | Struct definition, methods, tests |
| `src/binding/template.rs` | 119 | Template resolution with lazy bindings |
| `src/binding/resolve.rs` | 94 | Binding resolution, input paths |
| `src/runtime/runner.rs` | 77 | Core executor, dependency tracking |
| `src/runtime/executor/tests.rs` | 66 | Unit tests |

**Tier 2 -- High (10-50 occurrences):**

| File | Count | Nature |
|------|-------|--------|
| `tests/lazy_binding_test.rs` | 57 | Integration tests |
| `tests/binding_integration.rs` | 46 | Integration tests |
| `tests/fetch_wiremock_test.rs` | 39 | HTTP mock tests |
| `tests/executor_fetch_errors_test.rs` | 26 | Error tests |
| `src/runtime/artifact_processor.rs` | 24 | Artifact system |
| `src/runtime/executor/verbs.rs` | 18 | Verb implementations |
| `src/runtime/executor/decompose.rs` | 16 | Dynamic decomposition |

**Tier 3 -- Low (<10 occurrences):**
14 additional files with 1-15 occurrences each.

### Detailed file impact: strategy -> orchestrate

**WARNING**: Only `DecomposeStrategy` and orchestration-related `strategy` references should be renamed. `BackoffStrategy` in `src/jobs/retry.rs` is a **different concept** (retry backoff) and MUST NOT be renamed.

| File | Count | What to rename |
|------|-------|---------------|
| `src/ast/decompose.rs` | 14 | `DecomposeStrategy` -> `DecomposeMode` or keep (it's about decompose, not orchestration) |
| `src/init/tier6.rs` | 10 | Example workflow variable names |
| `src/ast/schema_validator.rs` | 5 | Schema validation examples |
| `src/runtime/executor/decompose.rs` | 2 | Strategy field matching |
| `src/runtime/runner.rs` | 1 | Logging |

**Note**: `DecomposeStrategy { Semantic, Static, Nested }` is about how `decompose:` works, not about orchestration. Consider renaming to `DecomposeMode` to avoid confusion, but this is NOT the same as `orchestration: orchestrate`.

---

## Refactoring Plan

### Philosophy

```
+===============================================================================+
|  v0 PHILOSOPHY                                                                 |
+===============================================================================+
|                                                                                |
|  - No backward compatibility                                                   |
|  - No deprecated aliases in code                                               |
|  - No "old name -> new name" shims                                             |
|  - No migration guides                                                         |
|  - Just rename. Clean. Done.                                                   |
|                                                                                |
|  Exception: orchestration: strategy|orchestrate in YAML parser                 |
|  (both accepted, orchestrate is canonical)                                     |
|                                                                                |
+===============================================================================+
```

### Phase 1 -- DataStore -> RunContext (v0.28)

The biggest rename. 668 occurrences, 29 files. Pure mechanical refactor.

```bash
# 1. Rename the file
mv src/store/datastore.rs src/store/run_context.rs

# 2. Global rename in src/
sed -i 's/DataStore/RunContext/g' src/**/*.rs
sed -i 's/datastore/run_context/g' src/**/*.rs
sed -i 's/data_store/run_context/g' src/**/*.rs

# 3. Global rename in tests/
sed -i 's/DataStore/RunContext/g' tests/**/*.rs
sed -i 's/datastore/run_context/g' tests/**/*.rs

# 4. Update mod.rs exports
# pub mod run_context;
# pub use run_context::RunContext;

# 5. Add vector_index field (Doc 17)
# pub struct RunContext {
#     results: DashMap<String, TaskResult>,
#     context: DashMap<String, Value>,
#     vector_index: Option<VectorIndex>,
# }

# 6. cargo test (6,157 tests must pass)
# 7. cargo clippy -- -D warnings
```

**Risk**: LOW -- pure rename, no logic change. All tests validate the same behavior.

### Phase 2 -- Record infrastructure (v0.28)

Create new files with correct naming from day 1:

```
src/runtime/record.rs           # Record struct
src/runtime/record_compress.rs  # RecordCompressor (tactical LLM summarization)
```

Add `record:` field to task AST in `src/ast/action.rs`.

### Phase 2b -- Punk Records tier WARM (v0.28)

Create the local disk persistence layer:

```
src/runtime/record_log.rs       # RecordLog -- manages .nika/records/
src/runtime/record_config.rs    # RecordConfig -- TTL, max_size, promotion settings
```

Add `[records]` section to `.nika/config.toml` parser.
Add `nika records` CLI subcommand (list, show, search, promote, prune, stats).

### Phase 3 -- Agent Presets (v0.28)

Create new files with correct naming:

```
src/runtime/agent_presets.rs    # AgentPreset { Default, Lite, Think, Search, Vision, Judge, Coder, Summary }
src/ast/agents.rs               # YAML parsing for agents: block
```

### Phase 4 -- Orchestration (v0.29)

Create new files:

```
src/runtime/orchestrate.rs            # OrchestrateRunner -- dynamic orchestration
src/runtime/orchestrate_dispatch.rs   # Satellite dispatch logic
src/ast/orchestrate.rs                # OrchestrateConfig, Satellite AST parsing
```

Add `orchestration:` field to workflow AST. Parser accepts both `orchestrate` and `strategy`.

### Phase 5 -- NovaNet promotion + Introspection (v0.30)

Add promotion logic:

```
src/runtime/promote.rs    # RecordLog::promote() -> novanet_write via MCP
```

Add `Record` node class to NovaNet schema (`brain/models/node-classes/org/agent/record.yaml`).

Add to `src/runtime/builtin/`:

```
records.rs       # nika:records -- accumulated Records (from Punk Records)
orchestrate.rs   # nika:orchestrate -- orchestrator state
```

Other introspection tools keep technical names (dag_state, budget, task_status, context).

---

## Brainstorm docs update scope

| File | Occurrences to update | Priority |
|------|----------------------|----------|
| `05-evolution-roadmap.md` | ~63 | HIGH -- core roadmap |
| `08-nika-030-complete-guide.md` | ~33 | HIGH -- user guide |
| `11-nika-030-technical-reference.md` | ~100+ | HIGH -- just created |
| `07-slate-deep-integration.md` | ~46 | MEDIUM |
| `06-research-synthesis-report.md` | ~42 | MEDIUM |
| `10-jarvis-tui-vision.md` | ~27 | MEDIUM |
| `09-use-cases-cookbook.md` | ~23 | MEDIUM |
| `00-README.md` | ~15 | LOW |
| `03-competitive-landscape.md` | ~15 | LOW |
| `02-scientific-literature.md` | ~14 | LOW |
| `04-nika-novanet-overlap.md` | ~3 | LOW |
| `01-current-features.md` | ~1 | LOW |

---

## Quick Reference Card

```
+===============================================================================+
|                    NAMING & IDENTITY -- QUICK REFERENCE                         |
+===============================================================================+
|                                                                                |
|  NIKA         = Le runtime (execution, liberte)                                |
|  STELLA       = L'utilisateur (la volonte qui dirige)                          |
|  PUNK RECORDS = Tier WARM -- memoire locale disque (RecordLog)                 |
|  NOVANET      = Tier COLD -- memoire permanente graph (Record node)            |
|  RUNCONTEXT   = Tier HOT -- memoire ephemere du run (ex-DataStore)             |
|                 Note: Egghead Island lore conserve, RunContext pour le code     |
|                                                                                |
|  ORCHESTRATE  = Mode dynamique (ex-Shaka, PUNK-01 lore)                        |
|  DEFAULT      = Agent preset primary -- creation (ex-Edison, PUNK-03 lore)     |
|  THINK        = Agent preset reasoning -- logique (ex-Pythagoras, PUNK-04)     |
|  LITE         = Agent preset tactical -- force rapide (ex-Atlas, PUNK-05)      |
|  SEARCH       = Agent preset search -- ressources (ex-York, PUNK-06)           |
|  VISION       = Agent preset vision -- analyse visuelle                         |
|  JUDGE        = Agent preset judge -- evaluation qualite                        |
|  CODER        = Agent preset code -- generation de code                         |
|  SUMMARY      = Agent preset summary -- compression                             |
|  LILITH       = Security layer -- doc only (PUNK-02, defense)                   |
|                                                                                |
|  RECORD       = Resultat compresse (ex-Episode, Punk Records)                   |
|  SATELLITE    = Task template dispatchee par l'orchestrateur (ex-tactic)        |
|                                                                                |
|  orchestration: dag          -> mode statique (inchange)                        |
|  orchestration: orchestrate  -> mode dynamique (alias: strategy)                |
|                                                                                |
+===============================================================================+
```

---

## Decisions validees (brainstorm 2026-03-14, updated 2026-03-20)

> [!NOTE]
> Decisions prises apres recherche approfondie (8 queries Perplexity, analyse de Letta/MemGPT, LangGraph, CrewAI, SSGM, A-MAC, conventions KG). Toutes les questions sont maintenant resolues.

### Q1: DataStore -> RunContext -- RESOLU

**Contexte :** "DataStore" n'est PAS une convention Rust officielle. C'est un nom generique invente pour le projet.

**Decision finale (Appendix) :** `DataStore` -> `RunContext`

**Raisons :**
- **Industry standard name** -- "RunContext" est utilise dans plusieurs frameworks de workflows
- **Self-documenting** -- Evident qu'il s'agit du contexte d'execution du run
- **Extension naturelle (Doc 17)** -- `vector_index` field added pour `nika:search` native RAG
- **Clear semantics** -- Context de runtime qui persiste pendant le run, disparu apres

**Status :** RESOLU -- implemente en v0.28

### Q2: Punk Records = tier WARM -- VALIDE

**Reponse :** Punk Records est le nom du tier WARM (disque local), PAS un concept umbrella.

**Parallele manga :** Dans One Piece, Punk Records est **specifiquement** le cerveau externalise de Vegapunk -- une sphere geante flottant au-dessus d'Egghead Island. C'est la ou les resultats du travail des satellites sont accumules et stockes. C'est personnel a Vegapunk, separe du World Government. A long terme, Vegapunk voulait le partager avec le monde entier (le broadcast).

**Architecture memoire 3-tier :**

```
Nika Memory Architecture (3 tiers, pas de nom umbrella)
|
+-- HOT:  RunContext       = DashMap (RAM)
|         Struct Rust:       RunContext (ex-DataStore, resolu Appendix)
|         Lifetime:          un workflow run
|         Contenu:           TaskResults, bindings, loaded context, vector_index
|         Analogie manga:    Egghead Island (le lab ephemere)
|         Analogie tech:     CPU cache / memoire de travail
|         Extension:         vector_index field (Doc 17) pour nika:search RAG
|
+-- WARM: Punk Records     = NDJSON (disque local)
|         Struct Rust:       RecordLog
|         Lifetime:          configurable (TTL: 7d, 30d, 90d, infinity)
|         Contenu:           Record summaries compresses par run
|         Fichiers:          .nika/records/{date}_{run_id}.ndjson
|         Analogie manga:    Punk Records (le cerveau de Vegapunk)
|         Analogie tech:     Memoire episodique / RAM disque
|
+-- COLD: NovaNet          = Neo4j (graph via MCP)
          Node class:        Record (nouveau, layer agent)
          Lifetime:          permanent, cure
          Contenu:           Records promus (haute confiance/valeur)
          Promotion:         auto (seuil configurable) ou manuelle
          Analogie manga:    Le World Government (savoir partage)
          Analogie tech:     Memoire semantique / SSD
```

**Etat de l'art comparatif :**

| Framework | HOT | WARM | COLD |
|-----------|-----|------|------|
| **Letta/MemGPT** | Core Memory | Recall Memory | Archival Memory |
| **LangGraph** | Thread State | MemorySaver | InMemoryStore/DB |
| **CrewAI** | Short-term (ChromaDB) | Entity Memory (SQLite) | Long-term (SQLite) |
| **SSGM (2026)** | Mutable Active Graph | Immutable Episodic Log | -- |
| **Nika** | **RunContext** | **Punk Records** | **NovaNet** |

### Q3: Node NovaNet = `Record` -- VALIDE

**Decision :** Le node class dans NovaNet s'appelle `Record` (pas AgentRecord, pas NikaRecord).

**Raisonnement :**
- NovaNet utilise des noms courts : `Entity`, `Page`, `Block` -- pas `NovaNetEntity`
- "Agent" est trompeur -- les Records viennent de TOUS les 5 verbes, pas juste `agent:`
- "Nika" couple au produit -- si demain un autre outil genere des records, on est coince
- `Record` est clair, propre, zero conflit avec les 59 node classes existantes
- Recherche KG : conventions CamelCase courts, discriminants (source: PuppyGraph, TowardsAI)

**Schema NovaNet :**

```yaml
# brain/models/node-classes/org/agent/record.yaml
name: Record
realm: org
layer: agent
description: "Compressed execution record promoted from Nika Punk Records"
properties:
  key: { type: string, required: true }       # record-{uuid}
  summary: { type: string, required: true }
  confidence: { type: float }
  tokens_used: { type: integer }
  source_workflow: { type: string }
  source_task: { type: string }
  verb: { type: string }                      # infer|exec|fetch|invoke|agent
  model: { type: string }
  created_at: { type: datetime }

# Arcs
# HAS_RECORD: Project -> Record (ownership)
# RELATES_TO: Record -> Entity (semantic link)
# FOR_LOCALE: Record -> Locale (if locale-specific)
```

---

## Punk Records -- Design complet

### Lifecycle d'un Record

```
Task executes dans RunContext (HOT)
    |
TaskResult cree (resultat brut)
    |
Workflow complete -> RecordCompressor s'execute
    |
Record { summary, key_findings, confidence } cree
    |
RecordLog.save() -> .nika/records/{date}_{run_id}.ndjson (WARM)
    |
[Si auto_promote = true ET confidence > seuil]
    -> RecordLog.promote() -> novanet_write Record node (COLD)
    |
[Apres TTL expire]
    -> RecordLog.prune() supprime les vieux NDJSON
```

### Format NDJSON

Chaque ligne = un Record :

```jsonl
{"id":"rec-abc123","task_id":"research","workflow":"seo-pipeline","summary":"Found 15 keywords with >1000 monthly searches for QR code domain","key_findings":["qr code generator","free qr code","qr code scanner"],"confidence":0.92,"tokens_used":1847,"created_at":"2026-03-14T20:00:00Z","verb":"infer","model":"claude-sonnet-4-20250514"}
{"id":"rec-def456","task_id":"generate_page","workflow":"seo-pipeline","summary":"Generated fr-FR landing page with 1200 words, SEO-optimized H1/H2 structure","key_findings":["targeting generateur qr code as primary keyword"],"confidence":0.87,"tokens_used":3200,"created_at":"2026-03-14T20:01:30Z","verb":"infer","model":"claude-sonnet-4-20250514"}
```

**Pourquoi NDJSON :**
- Deja utilise par Nika pour les traces (EventLog) -- meme tooling
- Append-only (pas de risque de corruption)
- Streamable (lecture ligne par ligne)
- grep-friendly (un record par ligne)
- Zero schema migration necessaire

### Structure disque

```
.nika/
+-- records/
|   +-- 2026-03-14_abc123.ndjson    # Un fichier par run
|   +-- 2026-03-14_def456.ndjson
|   +-- 2026-03-13_xyz789.ndjson.gz # Compresse apres compress_after
|   +-- index.json                   # Index pour recherche rapide
+-- sessions/                         # Existe deja (v0.8.0)
+-- config.toml                       # Existe deja (v0.8.0)
```

### Index pour lookups rapides

```json
{
  "version": 1,
  "runs": [
    {
      "run_id": "abc123",
      "workflow": "seo-pipeline",
      "created_at": "2026-03-14T20:00:00Z",
      "record_count": 5,
      "total_tokens": 12400,
      "avg_confidence": 0.89,
      "promoted": 1,
      "file": "2026-03-14_abc123.ndjson"
    }
  ]
}
```

### Configuration (.nika/config.toml)

```toml
[records]
enabled = true                   # Activer Punk Records
ttl = "30d"                      # Duree de retention (defaut: 30 jours)
max_size = "500mb"               # Taille max sur disque
prune_on_start = true            # Auto-prune au demarrage de Nika
compress_after = "7d"            # gzip les records apres 7 jours

[records.promotion]
enabled = true                   # Activer la promotion vers NovaNet
auto_promote = false             # false = promotion manuelle uniquement
confidence_threshold = 0.85      # Seuil pour auto-promotion
require_entity_link = true       # Promouvoir seulement si lie a une Entity
```

**Strategies TTL preconfigurrees :**

| Strategy | TTL | compress_after | max_size | Use case |
|----------|-----|----------------|----------|----------|
| `ephemeral` | 7d | 3d | 100mb | CI/CD, tests, workflows ponctuels |
| `standard` | 30d | 7d | 500mb | Dev quotidien (defaut) |
| `archival` | 90d | 14d | 2gb | Recherche, projets longs |
| `unlimited` | infinity | 30d | 10gb | Prune manuelle uniquement |

### Rust structs

```rust
pub struct Record {
    pub id: RecordId,              // rec-{uuid}
    pub task_id: String,           // quelle task a produit ce record
    pub workflow_id: String,       // quel workflow run
    pub verb: Verb,                // infer|exec|fetch|invoke|agent
    pub model: Option<String>,     // modele utilise (si applicable)
    pub summary: String,           // resume compresse par LLM
    pub key_findings: Vec<String>, // points cles extraits
    pub confidence: f64,           // 0.0-1.0
    pub tokens_used: usize,        // tokens consommes
    pub created_at: DateTime<Utc>,
    pub promoted: bool,            // true si promu dans NovaNet
}

pub struct RecordLog {
    records_dir: PathBuf,          // .nika/records/
    config: RecordConfig,          // depuis config.toml
}

impl RecordLog {
    /// Sauvegarder les records d'un run
    pub fn save(&self, run_id: &str, records: &[Record]) -> Result<PathBuf>;

    /// Rechercher dans les Punk Records
    pub fn query(&self, filter: RecordFilter) -> Result<Vec<Record>>;

    /// Garbage collection des records expires
    pub fn prune(&self) -> Result<usize>;

    /// Compresser les vieux fichiers NDJSON
    pub fn compress(&self) -> Result<usize>;

    /// Promouvoir un record vers NovaNet
    pub fn promote(&self, record_id: &RecordId, mcp: &McpClient) -> Result<()>;

    /// Stats des Punk Records
    pub fn stats(&self) -> Result<RecordStats>;
}
```

### CLI commands

```bash
# Gestion des Punk Records
nika records list                    # Lister les runs recents avec records
nika records show <run-id>           # Afficher les records d'un run
nika records search "keyword"        # Recherche full-text dans les records
nika records promote <record-id>     # Promouvoir vers NovaNet
nika records prune                   # Garbage collection manuelle
nika records stats                   # Usage disque, count, oldest/newest
nika records export <run-id>         # Export JSON/CSV
```

### Promotion vers NovaNet

Quand un Record est promu :

1. `RecordLog` lit le Record depuis le NDJSON
2. Appel `novanet_write` via MCP pour creer un node `Record` dans NovaNet
3. Creation des arcs : `RELATES_TO` (Record->Entity si applicable)
4. Le Record est marque `promoted: true` dans le NDJSON
5. Les records promus ne sont PAS supprimes par le TTL (ils vivent dans NovaNet)

**3 modes de promotion :**

| Mode | Declencheur | Config |
|------|-------------|--------|
| **Manuel** | `nika records promote <id>` | Toujours disponible |
| **Auto** | `confidence > threshold` a la fin du run | `auto_promote = true` |
| **Workflow** | `record: { promote: true }` dans le YAML | Par task |

```yaml
# Promotion explicite dans un workflow
tasks:
  - id: critical_research
    infer: "Deep analysis of competitor landscape"
    record:
      compress: true
      promote: true              # Force promotion vers NovaNet
      entity_link: "qr-code"    # Lie au node Entity dans NovaNet
```

---

## Appendix: DataStore -> RunContext Research

> Consolidated from archive/14-datastore-naming-research.md.
> Original research date: 2026-03-14.

### Context

Choosing a name for the DashMap-based RAM store that holds task results, context, and inputs during a single workflow execution run in Nika. Current name: `DataStore` (in `src/store/datastore.rs`).

### Workflow Engine Survey

| Framework | Language | Name | What It Stores | Lifetime | Concurrency Model |
|-----------|----------|------|----------------|----------|-------------------|
| **Temporal** | Rust SDK | `WorkflowContext` | Workflow state, timers, signals, queries | Per workflow execution | Event-sourced replay |
| **Temporal** | Rust core | `ManagedRun` / `WorkflowManager` | Activation state, command buffers | Per workflow task | Managed by worker |
| **Airflow** | Python | `XCom` ("cross-communication") | Task outputs for inter-task data passing | Per DAG run (DB-persisted) | DB-backed, not in-memory |
| **Airflow** | Python | `context` dict | Task instance metadata, execution date, params | Per task execution | Thread-local dict |
| **Prefect** | Python | `ResultStore` | Task/flow return values | Per flow run (persisted) | Serialized to storage |
| **Prefect** | Python | `FlowRunContext` / `TaskRunContext` | Runtime metadata, result factory, client | Per run | Context var (asyncio) |
| **Dagster** | Python | `StepExecutionContext` / `PlanExecutionContext` | I/O managers, resources, step outputs | Per step/plan execution | Thread-safe via resources |
| **n8n** | TypeScript | `IRunExecutionData` | `resultData.runData` (node outputs), `executionData.contextData` | Per workflow execution | Single-threaded (Node.js) |
| **Argo Workflows** | Go | `NodeStatus` (in WorkflowStatus) | Node outputs, artifacts, phase | Per workflow (K8s CRD) | K8s etcd-persisted |
| **Windmill** | Rust | `FlowStatus` + `FlowContext` | Job results, flow inputs, module status | Per flow run (DB-persisted) | PostgreSQL-backed |
| **LangGraph** | Python | `StateGraph` + `Channel` system | Graph state via typed channels (`LastValue`, `BinaryOp`) | Per graph invocation | Channel-based (Pregel model) |

**Key Observations:**

1. **"Context" is the dominant pattern** for "bag of data available during execution": Temporal (`WorkflowContext`), Dagster (`StepExecutionContext`), Prefect (`FlowRunContext`), Airflow (`context` dict), Windmill (`FlowContext`).

2. **"Store" appears when persistence is involved**: Prefect (`ResultStore`), Dagster (`DynamicPartitionsStore`), Semantic Kernel (`MemoryStore`, `VectorStore`).

3. **"State" appears when describing the shape of data, not the container**: LangGraph (`State` is a `TypedDict` schema), AutoGen (`AssistantAgentState`, `TeamState`).

### Agent Framework Survey

| Framework | Language | Name | What It Stores | Lifetime |
|-----------|----------|------|----------------|----------|
| **LangGraph** | Python | `State` (TypedDict) | User-defined graph state, messages | Per graph invocation |
| **CrewAI** | Python | `ShortTermMemory` / `LongTermMemory` / `EntityMemory` | RAG embeddings, conversation history | Short-term: per task; Long-term: persistent |
| **AutoGen** | Python | `ChatCompletionContext` / `BufferedChatCompletionContext` | Message history for agents | Per agent conversation |
| **Semantic Kernel** | Python | `RunContext` | In-process runtime execution context | Per agent run |

### Rust Ecosystem Patterns

| Library | Name | What It Stores | Pattern |
|---------|------|----------------|---------|
| **tokio** | `Context` (scheduler) | Current task, runtime handle | Per-worker thread-local |
| **axum** | `State<S>` | User-provided app state | Per-application (shared via `Arc`) |
| **bevy** | `World` | All ECS entities, components, resources | Per-app (the universal container) |
| **Temporal Rust SDK** | `WorkflowContext<W>` | Workflow handle, state access | Per workflow execution |
| **Windmill** | `FlowContext` | Flow inputs, flow status | Per flow execution |

### Candidate Analysis Summary

```
                    Accuracy  No-Confusion  Precedent  Rust-Idiomatic  Scope-Clarity
RunContext            +++        +++           +++          +++             +++
ExecutionState        ++         ++            +            ++              ++
RunState              ++         +             +            ++              +
RuntimeStore          ++         +             -            +               -
DataStore (current)   +          -             -            +               +
TaskStore             +          +             -            +               -
WorkingMemory         +          --            -            -               --
```

### Verdict: `RunContext`

1. **Industry standard**: Temporal, Dagster, Prefect, Windmill, Semantic Kernel all follow the `*Context` pattern.
2. **Precise scoping**: `Run` prefix makes it clear this is per-execution.
3. **Rust idiomatic**: Tokio, axum, Temporal Rust SDK all use `Context` for scoped state.
4. **No confusion with persistence**: Unlike `*Store`, `*Context` does not imply database backing.
5. **No confusion with lifecycle enum**: Unlike `RunState`, clearly means "the context of a run".
6. **No confusion with LLM memory**: Unlike `WorkingMemory`, no AI/cognitive science baggage.
7. **Matches Nika's module naming**: Nika already has `BootContext` and `LoadedContext`.

### Research Methodology

- **Repositories analyzed**: 14 (Temporal, Airflow, Prefect, Dagster, n8n, Argo, Windmill, LangGraph, AutoGen, Semantic Kernel, tokio, axum, bevy, Vector)
- **Language coverage**: Rust, Python, TypeScript, Go
- **Confidence**: High

---

<div align="center">

[<- 11 Technical Reference](./11-nika-030-technical-reference.md) . [13 Multimodal Workers ->](./13-multimodal-worker-architectures.md) . [Index](./00-README.md)

</div>
