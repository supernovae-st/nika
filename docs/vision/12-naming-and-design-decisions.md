# 12 -- Naming & Design Decisions

> **Note (v0.49):** The One Piece naming (edison/atlas/york/pythagoras) was the original design
> inspiration but has been **replaced with functional names** (default/lite/think/search/vision/
> judge/coder/summary) in all user-facing contexts. The Vegapunk mapping below is preserved as
> INTERNAL architectural reference -- it remains brilliant as a mental model for the satellite
> system, but users never see these names.
>
> Additionally, `model_slots:` was never implemented as a separate top-level key. It has been
> superseded by the existing `agents:` system, which unifies model routing with agent behavior
> configuration in a single block.

> Part 1: Naming evolution -- One Piece origins, Vegapunk-to-descriptive presets, DataStore->RunContext, full renaming table.
> Part 2: Model routing & agents design -- Gen1->Gen3, industry survey, 8 presets, unified agents: block, devil's advocate.
> v0 philosophy: no backward compatibility, no legacy, just rename.

**Status:** APPROVED | **Date:** 2026-03-14 | **Updated:** 2026-03-27 (v0.49)
**Dependencies:** Doc 05 (Evolution Roadmap), Doc 17 (Smart Router)
**Research Sources:** Perplexity (5 queries), arXiv (4 papers), framework docs (6 frameworks)

---

# Part 1: Naming Evolution

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

## Naming Evolution: Vegapunk -> Descriptive Presets (COMPLETED)

The original Vegapunk naming (edison/atlas/york/pythagoras) was cohesive with lore but created
onboarding friction. The evolved naming uses **descriptive agent presets** that are instantly
understandable without learning One Piece lore. As of v0.49, all user-facing code, docs, and
YAML use the functional names exclusively:

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
that workflows actually dispatch to. See Part 2 for the full research validation.

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
# DESIGN DOCUMENT (Vegapunk naming, separate model_slots: -- NEVER SHIPPED)
# model_slots:
#   edison: claude-sonnet-4-20250514          # PUNK-03
#   atlas: claude-haiku-35                    # PUNK-05
#   pythagoras: claude-sonnet-4-20250514      # PUNK-04
#   york: perplexity/sonar-pro               # PUNK-06

# IMPLEMENTED (descriptive presets, unified agents: block -- shipped v0.35+)
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

### Agent Presets (model_slots: never shipped -- agents: is the implementation)

```rust
// DESIGN DOCUMENT ONLY (Vegapunk naming -- never implemented)
// pub struct ModelSlots {
//     pub edison: ModelConfig,      // PUNK-03
//     pub atlas: ModelConfig,       // PUNK-05
//     pub pythagoras: ModelConfig,  // PUNK-04
//     pub york: ModelConfig,        // PUNK-06
// }

// IMPLEMENTED (descriptive presets via agents: block)
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

### Historical: code at v0.27 (renames completed)

> **Note (v0.49):** The `DataStore` -> `RunContext` rename was completed. The `model_slots:`
> top-level key was **never implemented** -- agent presets are configured via the `agents:`
> block in the workflow header. Records/orchestration/introspection remain future work.

| Rename | Occurrences | Files | Status |
|--------|-------------|-------|--------|
| `DataStore` -> `RunContext` | **668** | 29 (19 src + 10 tests) | DONE |
| `strategy` (DecomposeStrategy) | 51 | 13 | DONE |
| `episode` | 17 | 1 (tier6.rs exemples) | DONE |
| `tactics` | 0 | 0 | Never implemented |
| `model_slots:` | 0 | 0 | Never implemented -- superseded by `agents:` |

### Future code (use new names directly)

| Feature | Fichiers a creer | Naming |
|---------|------------------|--------|
| Record compression | `src/runtime/record.rs`, `src/runtime/record_compress.rs` | `Record`, `RecordCompressor` |
| Orchestration | `src/runtime/orchestrate.rs`, `src/runtime/orchestrate_runner.rs`, `src/ast/orchestrate.rs` | `OrchestrateRunner`, `OrchestrateConfig`, `Satellite` |
| Context budgets | `src/runtime/budget.rs` | Pas de rename (technique) |
| Punk Records | `src/runtime/record_log.rs` | `RecordLog` (NDJSON on disk, .nika/records/) |
| NovaNet memory | `src/runtime/promote.rs` | `Record` node class + `RecordLog::promote()` |
| Introspection | 6 builtin tools | `nika:records`, `nika:orchestrate`, rest technique |

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

### Phase 1 -- DataStore -> RunContext (DONE)

The biggest rename. 668 occurrences, 29 files. Pure mechanical refactor. Completed pre-v0.34.

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

# 6. cargo test (8457 tests pass as of v0.49.3)
# 7. cargo clippy -- -D warnings
```

**Risk**: LOW -- pure rename, no logic change. All tests validate the same behavior.

### Phase 2 -- Record infrastructure (FUTURE)

Create new files with correct naming from day 1:

```
src/runtime/record.rs           # Record struct
src/runtime/record_compress.rs  # RecordCompressor (tactical LLM summarization)
```

Add `record:` field to task AST in `src/ast/action.rs`.

### Phase 2b -- Punk Records tier WARM (FUTURE)

Create the local disk persistence layer:

```
src/runtime/record_log.rs       # RecordLog -- manages .nika/records/
src/runtime/record_config.rs    # RecordConfig -- TTL, max_size, promotion settings
```

Add `[records]` section to `.nika/config.toml` parser.
Add `nika records` CLI subcommand (list, show, search, promote, prune, stats).

### Phase 3 -- Agent Presets (DONE via agents: block)

The `agents:` block is implemented in the workflow header. The 8 functional preset names
(default/lite/think/search/vision/judge/coder/summary) are the canonical user-facing names.
The separate `model_slots:` top-level key was never created -- `agents:` subsumes it entirely.

### Phase 4 -- Orchestration (FUTURE)

Create new files:

```
src/runtime/orchestrate.rs            # OrchestrateRunner -- dynamic orchestration
src/runtime/orchestrate_dispatch.rs   # Satellite dispatch logic
src/ast/orchestrate.rs                # OrchestrateConfig, Satellite AST parsing
```

Add `orchestration:` field to workflow AST. Parser accepts both `orchestrate` and `strategy`.

### Phase 5 -- NovaNet promotion + Introspection (FUTURE)

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

**Status :** RESOLU -- implemente (pre-v0.34)

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

# Part 2: Model Routing & Agents Design

> Industry survey of model routing patterns, naming conventions, and semantic agent preset systems.
> Validates Nika's unified `agents:` block with 8 presets (default/lite/think/search/vision/judge/coder/summary) against the state of the art.
> Includes adversarial analysis (devil's advocate counter-arguments) of the unified design.

---

## Why This Research Matters

Nika's `agents:` block (shipped since v0.35) lets workflows declare WHAT capability
they need (default, lite, think, search, vision, judge, coder, summary) rather than WHICH specific
model to use. This section validates the design against the 2025-2026 landscape.

```
THE CORE QUESTION
-----------------------------------------------------------------
Should workflows say:       Or should they say:

  model: claude-sonnet-4-6       agent: default
  model: llama-3.3-70b           agent: lite
  model: deepseek-chat           agent: search
  model: claude-sonnet-4-6       agent: think

Explicit model IDs             Semantic agent presets
(brittle, coupled)             (portable, intent-driven)
```

The industry is moving decisively toward **semantic routing** -- assigning models by capability
rather than by name. IDC predicts 70% of enterprises will adopt multi-model routing by 2028[^1].
This research confirms Nika's approach is aligned with the trajectory.

---

## Industry Survey: Model Routing Patterns

### The Three Generations of Model Selection

```
Generation 1 (2023-2024):    Single model, hardcoded
                             model: "gpt-4"

Generation 2 (2024-2025):    Per-task explicit model IDs
                             research_model = ChatOpenAI(model="gpt-4o-mini")
                             analysis_model = ChatOpenAI(model="o1-preview")

Generation 3 (2025-2026):    Unified agent presets + dynamic routing
                             agent: think            # Router picks best model
                             agent: lite              # Cost-optimized
```

### Platform-Level Routing (Gateways)

| Platform | Approach | Cost Savings | Key Innovation |
|----------|----------|:------------:|----------------|
| **OpenAI GPT-5 Router** | Internal router selects sub-models per query | Undisclosed | Trained on user switches, preference rates, correctness signals[^2] |
| **OpenRouter** | Unified API to 500+ models, provider fallback | 30-60% | Load balancing, "exacto" curated endpoints, 100T+ tokens routed in 2025[^3] |
| **Azure Model Router** | Complexity/cost/quality modes per request | 40-70% | Three modes: quality, cost, balanced[^4] |
| **RouteLLM (LMSYS)** | Open-source cost-quality router | 10-80% | SW ranking, matrix factorization, BERT classifiers[^5] |
| **LiteLLM** | OpenAI-compatible proxy, semantic tiers | 30-50% | Drop-in swap without code changes |
| **Cloudflare AI Gateway** | Edge routing with caching | 40-60% | Semantic caching, automatic failover |

### The GPT-5 Router Controversy

OpenAI's GPT-5 (August 2025) introduced a **model router** that automatically selects
between internal sub-models based on query complexity. The system was trained on:
- When users switch models (implicit preference signal)
- Preference rates for responses
- Measured correctness on evaluation benchmarks

This sparked user backlash due to inconsistencies and unpredictable behavior[^2].
Key lesson for Nika: **transparent, user-controlled routing beats opaque auto-routing**.
Nika's named slots give the user explicit control over which capability tier each task uses.

### Enterprise Adoption

IDC reports that 37% of enterprises already use 5+ models in production[^1].
The dominant pattern is a "two-stack" approach:

```
DEEP STACK          FAST STACK
(reasoning)         (throughput)
Claude Sonnet 4.5   Gemini 2.5 Flash
GPT-4o              Llama 3.3 70B
o1-preview          GPT-4o-mini
```

This maps directly to Nika's agent preset model: `think` for deep, `lite` for throughput,
with `default` and `search` providing additional specialization, plus `vision`, `judge`, `coder`,
and `summary` for targeted capabilities.

---

## Framework-Level Model Assignment

### LangGraph: Per-Node Model Binding

LangGraph (LangChain's graph orchestration) assigns models at the **node function level**.
Each node in the StateGraph can invoke a different LLM. There is no named-slot abstraction --
models are bound directly via Python code.

```python
# LangGraph: explicit model IDs per node
research_model = ChatOpenAI(model="gpt-4o-mini")     # Cheap
analysis_model = ChatOpenAI(model="o1-preview")       # Expensive

def research_node(state):
    return {"messages": research_model.invoke(state["messages"])}

def analysis_node(state):
    return {"messages": analysis_model.invoke(state["messages"])}

graph = StateGraph(State)
graph.add_node("research", research_node)
graph.add_node("analysis", analysis_node)
```

**Observation:** No semantic abstraction. Model IDs are hardcoded in Python. Changing
a model requires code changes, not config changes.

### CrewAI: Per-Agent Model Assignment

CrewAI assigns LLMs to **agents**, and tasks inherit from their assigned agent.
This is role-based model routing -- the "Researcher" agent gets a cheap model,
the "Writer" gets an expensive one.

```python
# CrewAI: model assignment via agent roles
researcher = Agent(
    role="Researcher",
    goal="Discover AI trends",
    llm=ChatOpenAI(model="gpt-4o-mini"),   # Cost-optimized
)
writer = Agent(
    role="Writer",
    goal="Summarize findings",
    llm=ChatOpenAI(model="o1"),            # High-reasoning
)

research_task = Task(agent=researcher)     # Inherits gpt-4o-mini
write_task = Task(agent=writer)            # Inherits o1
```

**Observation:** Role-based but still explicit model IDs. No indirection layer.
Agent roles serve as implicit capability categories (researcher = search, writer = creative).

### Microsoft AutoGen: model_client Per Agent

AutoGen uses `model_client` (or `llm_config`) per ConversableAgent. Each agent can
point to a different provider and model.

```python
# AutoGen: per-agent model_client
researcher = ConversableAgent(
    name="Researcher",
    model_client=dict(config_list=openai_config),    # gpt-4o-mini
)
analyst = ConversableAgent(
    name="Analyst",
    model_client=dict(config_list=azure_config),     # gpt-4o (Azure)
)
```

**Observation:** Provider-level routing (OpenAI vs Azure) in addition to model-level.
No named slot abstraction.

### Microsoft Semantic Kernel: Service-Based Routing

Semantic Kernel routes models via the kernel's service registry. Functions can specify
which registered service to use via `PromptExecutionSettings`.

```csharp
// Semantic Kernel: registered services as implicit slots
builder.AddOpenAIChatCompletion("gpt-4o-mini", key);   // "lite"
builder.AddAzureChatCompletion("gpt-4o", endpoint);    // "quality"

// Per-invocation selection
var args = new KernelArguments { { "model_id", "gpt-4o-mini" } };
var result = await kernel.InvokeAsync(function, args);
```

**Observation:** Closest to a slot-like system via service registration. But still
uses model IDs, not semantic names.

### DSPy: Compile-Time Model Optimization

DSPy takes a unique approach -- models are assigned per module, and the `Compile` step
can automatically optimize which model each module uses.

```python
# DSPy: per-module model with compile-time optimization
gpt4mini = dspy.OpenAI(model='gpt-4o-mini')
o1 = dspy.OpenAI(model='o1-preview')

research = dspy.ChainOfThought(Research, lm=gpt4mini)
generate = dspy.ChainOfThought(Generate, lm=o1)

# Compiler can swap models based on metrics
optimized = dspy.BootstrapFewShot(metric=accuracy).compile(program)
```

**Observation:** Compile-time optimization is the closest to "automatic semantic routing" --
DSPy figures out which model works best per module. However, the initial assignment
is still explicit model IDs.

### Mastra AI: Workflow Step Model Selection

Mastra AI (2025) supports per-step model selection in TypeScript workflows. Each step
in a workflow can reference a different model from the configured providers.

**Observation:** Limited public documentation as of March 2026. Follows the Generation 2
pattern of explicit model IDs per step.

---

## Academic Foundations: LLM Routing Research

### Key Papers

| Paper | Year | Key Contribution | Relevance to Nika |
|-------|:----:|------------------|-------------------|
| **FrugalGPT**[^6] | 2023 | Cascading: try cheap model first, escalate if uncertain | Confidence-based model escalation in orchestration |
| **AutoMix**[^7] | 2024 | Automatic routing between model sizes based on query | Validates per-task model routing |
| **RouteLLM**[^5] | 2024 | Open-source cost-quality routing with 4 classifier types | SW ranking + BERT classifiers for routing |
| **R2-Router**[^8] | 2026 | Treats LLMs as quality-cost curves, not points | Powerful models with constrained budgets can beat weak models |
| **Dynamic Routing Survey**[^9] | 2026 | Systematic analysis of multi-LLM routing and cascading | Well-designed routing outperforms any single model |
| **KNN Routing**[^10] | 2025 | Simple non-parametric routing beats complex methods | Simple slot assignment may outperform dynamic routing |

### FrugalGPT Cascading Pattern

FrugalGPT introduced the cascading model: try the cheapest model first, check confidence,
escalate to a more expensive model if needed.

```
Query arrives
    |
    v
Try GPT-3.5 (cheap)
    |
    +-- Confidence > 0.9? --> Return result
    |
    +-- Confidence < 0.9
    |
    v
Try GPT-4 (expensive)
    |
    v
Return result
```

**Cost savings:** Up to 98% cost reduction with minimal quality loss on certain benchmarks.
**Relevance to Nika:** This pattern maps to orchestration's confidence-based escalation (P-SHAKA).
A record with low confidence triggers re-execution with a more capable agent preset.

### R2-Router: Models as Quality-Cost Curves

R2-Router (February 2026) challenges the assumption that expensive models always produce
better results. By treating each LLM as a **quality-cost curve** (using length-constrained
instructions), the paper discovers configurations where:

- Powerful LLMs with **constrained budgets** outperform weaker models at full budget
- Optimal model selection depends on the specific cost-quality tradeoff desired

**Relevance to Nika:** Validates the `lite` agent preset concept -- a powerful model with
constrained parameters (fast, short outputs) can outperform a weaker model at full length.

### Dynamic Routing Survey (2026)

The most comprehensive survey to date[^9] analyzes routing across:
- Query difficulty estimation
- Human preference prediction
- Clustering-based routing
- Uncertainty quantification
- Reinforcement learning routers
- Cascading systems

**Key finding:** "Well-designed routing systems can outperform even the most powerful
individual models by strategically leveraging specialized capabilities across models
while maximizing efficiency gains."

This is the definitive academic validation for Nika's multi-preset approach.

---

## Naming Conventions Comparison

### Existing Naming Systems

| System | Naming Style | Tiers | Philosophy |
|--------|-------------|:-----:|------------|
| **Anthropic** | Poetry (Haiku/Sonnet/Opus) | 3 | Size + capability as literary forms |
| **OpenAI** | Version + suffix (gpt-4o, gpt-4o-mini, o1) | ~4 | Technical naming, "o" for optimized |
| **Google** | Gems (Gemini Flash/Pro/Ultra) | 3 | Speed metaphor (Flash) + quality (Ultra) |
| **Slate** | Role (main/subagent/search/reasoning) | 4 | Functional role in agent system |
| **OpenRouter** | Tiered (fast/quality/balanced) | 3 | Routing strategy, not model identity |
| **Azure Router** | Mode (quality/cost/balanced) | 3 | Optimization objective |
| **Nika (current)** | Descriptive presets (default/lite/think/search/vision/judge/coder/summary) | 8 | Functional capability presets in unified agents: block |

### Two Philosophies

```
PHILOSOPHY A: Name by size/speed              PHILOSOPHY B: Name by capability/role
---------------------------------------------  -------------------------------------------
Haiku / Sonnet / Opus                          main / subagent / search / reasoning (Slate)
Flash / Pro / Ultra                            fast / quality / balanced (OpenRouter)
mini / standard / premium                      default / lite / think / search (Nika)

Pros:                                          Pros:
- Clear cost/perf ordering                     - Decoupled from model identity
- Universal understanding                      - Portable across providers
- Stable naming across generations             - Intent-driven, not resource-driven

Cons:                                          Cons:
- Tied to a single provider                    - Learning curve for slot names
- Says nothing about task fit                  - Mapping must be user-configurable
- Cannot express multi-provider routing        - May confuse if names are obscure
```

### Slate's 4-Slot System (Direct Comparison)

Slate defines exactly 4 model slots, configured in `slate.json`:

| Slate Slot | Purpose | Default Model |
|-----------|---------|---------------|
| `main` | Primary content generation, orchestration | claude-sonnet-4-20250514 |
| `subagent` | Cheaper threads, tactical execution | claude-haiku-35 |
| `search` | Fast retrieval and search synthesis | perplexity/sonar-pro |
| `reasoning` | Deep thinking, planning, review | claude-sonnet-4-20250514 + thinking |

Nika's current mapping (unified `agents:` block with 8 presets):

| Slate Slot | Nika Agent Preset | Cognitive Role |
|-----------|-------------------|----------------|
| `main` | `default` | Primary creative generation, writing, orchestration |
| `subagent` | `lite` | Fast execution, structured tasks, formatting |
| `search` | `search` | Search, retrieval, data collection |
| `reasoning` | `think` | Deep reasoning, planning, critique, review |
| -- | `vision` | Visual analysis, OCR, image understanding |
| -- | `judge` | Quality evaluation, scoring, validation |
| -- | `coder` | Code generation, review, execution |
| -- | `summary` | Compression, summarization, extraction |

### Why Nika Evolved Beyond Slate's 4 Slots

The evolution from 4 named slots (edison/atlas/york/pythagoras) to 8 descriptive agent presets
(default/lite/think/search/vision/judge/coder/summary) serves three purposes:

1. **Descriptive over lore-based.** Functional names (default, lite, think) are instantly
   understandable without learning Vegapunk lore. Lower onboarding friction.

2. **Expanded coverage.** 4 slots could not adequately cover vision, code, judging, and
   summarization -- capabilities that workflows need distinct model configurations for.

3. **Unified `agents:` block.** Instead of a separate `model_slots:` top-level key, agent
   presets live inside the `agents:` block, unifying model configuration with agent behavior.

---

## Semantic Agent Preset Design Patterns

### Pattern: Agent Preset with Provider Binding

The dominant pattern across all frameworks that support multi-model is:

```
[User-Facing Preset]  -->  [Provider + Model Config]  -->  [Runtime Resolution]
     default           -->  anthropic / claude-sonnet   -->  API call
     lite              -->  groq / llama-3.3-70b        -->  API call
     search            -->  deepseek / deepseek-chat    -->  API call
     think             -->  anthropic / claude + think   -->  API call with thinking
     vision            -->  openai / gpt-4o              -->  API call
     judge             -->  anthropic / claude-sonnet    -->  API call
     coder             -->  anthropic / claude-sonnet    -->  API call
     summary           -->  groq / llama-3.3-70b         -->  API call
```

This is exactly what Nika implements in the unified `agents:` block (Doc 05).

### Pattern: Fallback Chain

Every production routing system implements fallback:

```yaml
# Nika agents with fallback (proposed for v0.28+)
agents:
  default:
    primary:
      provider: anthropic
      model: claude-sonnet-4-6
    fallback:
      provider: openai
      model: gpt-4o
```

This mirrors OpenRouter's automatic failover, Azure's provider redundancy,
and LiteLLM's fallback configuration.

### Pattern: Cost-Aware Preset Assignment

The key insight from FrugalGPT, RouteLLM, and enterprise adoption:

```
                    Cost per 1M tokens    Quality (avg)    Latency
                    -----------------    -------------    -------
think               $15.00               9.2/10           2-8s (thinking)
default             $3.00                8.5/10           1-3s
search              $0.27                7.8/10           0.5-1s
lite                $0.05                7.0/10           0.2-0.5s
```

The cost difference between presets can be **100-300x**, making routing a significant
optimization lever. IDC reports 70% cost reduction through intelligent routing[^1].

### Pattern: Confidence-Based Escalation

From FrugalGPT cascading + Nika's orchestration:

```
Task executes with lite (cheap, fast)
    |
    v
Record generated: confidence = 0.65 (below threshold)
    |
    v
Orchestrator sees low confidence --> Re-dispatch with default
    |
    v
Record generated: confidence = 0.92 (above threshold)
    |
    v
Continue workflow
```

This is not a separate routing system -- it emerges naturally from Nika's
P-RECORD (confidence tracking) + P-SHAKA (dynamic re-dispatch) integration.

---

## The 8-Preset Unified Agents Block: Deep Analysis

### The Eight Agent Presets

```
+===============================================================================+
|                    NIKA AGENT PRESETS (P-MODEL)                                 |
+===============================================================================+
|                                                                                |
|  DEFAULT                                                                       |
|  Role:     Primary creative work -- generation, writing, orchestration         |
|  Profile:  High quality, moderate cost, moderate speed                         |
|  Default:  claude-sonnet-4-6 / gpt-4o                                          |
|  Use:      Content generation, complex infer: tasks, general purpose           |
|                                                                                |
|  LITE                                                                          |
|  Role:     Fast tactical execution -- structured tasks, formatting             |
|  Profile:  Good quality, low cost, high speed                                  |
|  Default:  llama-3.3-70b (Groq) / gpt-4o-mini / claude-haiku                  |
|  Use:      Record compression, JSON extraction, simple transforms              |
|                                                                                |
|  THINK                                                                         |
|  Role:     Deep reasoning -- planning, analysis, critique, review              |
|  Profile:  Highest quality, high cost, slow (thinking enabled)                 |
|  Default:  claude-sonnet-4-6 + extended_thinking / o1-preview                  |
|  Use:      Strategic planning, complex analysis, multi-step reasoning          |
|                                                                                |
|  SEARCH                                                                        |
|  Role:     Search and retrieval -- research, data collection                   |
|  Profile:  Search-optimized, low cost, variable speed                          |
|  Default:  deepseek-chat / perplexity/sonar-pro                                |
|  Use:      Information gathering, search synthesis, RAG queries                |
|                                                                                |
|  VISION                                                                        |
|  Role:     Visual analysis -- image understanding, OCR                         |
|  Profile:  Vision-capable, moderate cost                                       |
|  Default:  openai/gpt-4o / native/qwen2-vl                                    |
|  Use:      Image analysis, visual QA, screenshot understanding                 |
|                                                                                |
|  JUDGE                                                                         |
|  Role:     Quality evaluation -- scoring, validation, gate checks              |
|  Profile:  High quality, moderate cost, deterministic                          |
|  Default:  claude-sonnet-4-6 / gpt-4o                                          |
|  Use:      Output validation, quality scoring, acceptance criteria              |
|                                                                                |
|  CODER                                                                         |
|  Role:     Code generation and review                                          |
|  Profile:  Code-optimized, moderate cost                                       |
|  Default:  claude-sonnet-4-6 / deepseek-coder                                  |
|  Use:      Code writing, refactoring, code review, debugging                   |
|                                                                                |
|  SUMMARY                                                                       |
|  Role:     Compression and summarization                                       |
|  Profile:  Good quality, low cost, fast                                        |
|  Default:  llama-3.3-70b (Groq) / gpt-4o-mini                                 |
|  Use:      Text summarization, record compression, key extraction              |
|                                                                                |
+===============================================================================+
```

### YAML Syntax (from Doc 05)

```yaml
schema: nika/workflow@0.12

agents:
  default:
    provider: anthropic
    model: claude-sonnet-4-6
  lite:
    provider: groq
    model: llama-3.3-70b-versatile
  search:
    provider: deepseek
    model: deepseek-chat
  think:
    provider: anthropic
    model: claude-sonnet-4-6
    extended_thinking: true
    thinking_budget: 16384
  vision:
    provider: openai
    model: gpt-4o
  judge:
    provider: anthropic
    model: claude-sonnet-4-6
  coder:
    provider: anthropic
    model: claude-sonnet-4-6
  summary:
    provider: groq
    model: llama-3.3-70b-versatile

tasks:
  - id: plan
    agent: think
    infer: "Create a content plan for {{with.entity}}"

  - id: generate_pages
    agent: default
    for_each: $pages
    infer: "Generate page {{with.item}}"

  - id: format
    agent: lite
    infer: "Format and validate the generated page"
```

### Why 8 Presets (Not 2, 3, or 4)

| Count | Examples | Problem |
|:-----:|----------|---------|
| 2 | fast / quality | Too coarse. Cannot distinguish reasoning from creative. |
| 3 | Haiku / Sonnet / Opus | Size-based, not capability-based. Search is not a "size". |
| 4 | default / lite / search / think | Misses vision, code, judging, summarization -- real workflow needs. |
| **8** | **default / lite / think / search / vision / judge / coder / summary** | **Covers all functional roles that workflows actually dispatch to.** |
| 10+ | Adding more specializations | Diminishing returns. The orchestrator is not a preset -- it is the dispatcher. |

The 8-preset design aligns with:
- **Slate's 4 slots** (main/subagent/search/reasoning) as a foundation -- extended with 4 more
- **Enterprise multi-model patterns** (deep + fast + search + reasoning + specialized)
- **Real workflow needs** (vision analysis, code gen, quality judging, summarization are first-class tasks)

### Agent Assignment Heuristics

When `agent:` is omitted, Nika uses the `default` preset by default.
In orchestrate mode, the orchestrator LLM can dynamically assign presets per task dispatch:

```
Orchestrator decision loop:
  "This task needs research" --> search
  "This task needs fast formatting" --> lite
  "This task needs creative writing" --> default
  "This task needs image analysis" --> vision
  "I need to review all results" --> judge
  "Compress this output" --> summary
```

---

## Cross-Framework Comparison Matrix

### Model Routing Capabilities

| Feature | Nika (current) | Slate | LangGraph | CrewAI | AutoGen | DSPy |
|---------|:-:|:-:|:-:|:-:|:-:|:-:|
| Named capability presets | **8 presets** | 4 slots | -- | -- | -- | -- |
| Per-task model selection | Yes | Yes | Yes (code) | Via agent | Via agent | Per module |
| Declarative (config/YAML) | **YAML** | JSON | Code only | Code only | Code only | Code only |
| Provider abstraction | 7 providers | ~3 | Via LangChain | Via LangChain | Direct | Direct |
| Fallback chain | Planned | Yes | Manual | Manual | Manual | -- |
| Cost tracking per slot | Yes (events) | Partial | Manual | Manual | Manual | Metrics |
| Dynamic re-routing | Via orchestrator | Via orchestrator | Conditional edges | -- | -- | Compile-time |
| Extended thinking support | Per slot | Per slot | Manual | -- | -- | -- |

### Abstraction Level

```
HIGH ABSTRACTION (semantic, portable)
  |
  |  Nika:      agent: default          (YAML, capability-named, 8 presets)
  |  Slate:     slot: main              (JSON, role-named)
  |
  |  OpenRouter: tier: quality           (API, objective-named)
  |  Azure:      mode: balanced          (API, optimization-named)
  |
  |  DSPy:       lm=gpt4mini            (Python, model-bound per module)
  |  CrewAI:     llm=ChatOpenAI(...)     (Python, model-bound per agent)
  |  LangGraph:  model = ChatOpenAI(...) (Python, model-bound per node)
  |  AutoGen:    model_client=dict(...)  (Python, config-bound per agent)
  |
LOW ABSTRACTION (explicit, brittle)
```

Nika and Slate sit at the highest abstraction level. The key differentiator is that
Nika uses **YAML-first declarative** configuration while Slate uses JSON/TypeScript.

### Naming Approach

| Framework | Naming Style | Memorable? | Portable? | Self-Documenting? |
|-----------|-------------|:----------:|:---------:|:-----------------:|
| **Nika** | Descriptive presets (default, lite, think) | Medium | High | High (self-documenting) |
| **Slate** | Role names (main, subagent) | Medium | High | High |
| **OpenRouter** | Objective names (fast, quality) | Medium | High | High |
| **Anthropic** | Poetry names (haiku, sonnet) | High | Low (vendor-specific) | Medium |
| **LangGraph** | Variable names (research_model) | Low | Low | High (developer context) |

---

## Recommendation Summary

### Validation Summary

The research validates Nika's 8-preset unified `agents:` design on every dimension:

| Dimension | Finding | Confidence |
|-----------|---------|:----------:|
| **Preset count (8)** | Covers all cognitive modes + specialized capabilities, aligns with enterprise patterns | High |
| **Named presets** | Higher abstraction than any competitor except Slate | High |
| **YAML-first** | Only declarative system with named presets (unique differentiator) | High |
| **Descriptive names** | Self-documenting, zero onboarding friction, no lore required | High |
| **Per-task assignment** | Standard pattern across all frameworks | High |
| **Fallback chains** | Industry standard, planned for v1 roadmap | High |
| **Confidence escalation** | Academically validated (FrugalGPT, RouteLLM) | High |

### Design Decisions: Confirmed

| Decision | Status | Rationale |
|----------|:------:|-----------|
| Use 8 named presets as unified `agents:` block | CONFIRMED | Covers all cognitive modes + specialized capabilities. |
| Use descriptive names (default/lite/think/search/vision/judge/coder/summary) | CONFIRMED | Self-documenting, zero onboarding friction, no lore dependency. |
| YAML-level declaration, not runtime-only | CONFIRMED | Declarative = version-controlled, auditable, reproducible. |
| `agent:` per task, `default` as fallback | CONFIRMED | Matches Slate pattern, granular control with sensible defaults. |
| Orchestrator can dynamically assign presets | CONFIRMED | Dynamic dispatch is validated by every orchestration framework. |
| Extended thinking as preset property, not separate preset | CONFIRMED | Thinking is a model capability, not a cognitive mode. |

### Gap: Fallback Chains (Future)

The one gap in the current P-MODEL design is **explicit fallback configuration**.
Every production routing system supports this. Proposed addition:

```yaml
agents:
  default:
    provider: anthropic
    model: claude-sonnet-4-6
    fallback:                        # NEW: fallback chain
      - provider: openai
        model: gpt-4o
      - provider: groq
        model: llama-3.3-70b-versatile
```

This enables resilience (provider outages) and flexibility (dev vs production configs).

### Gap: Preset Aliases (Deprioritized)

For migration from Vegapunk naming, consider accepting both old and new names:

```yaml
# Both accepted, descriptive names are canonical
agents:
  default:    ...    # Alias: edison, main
  lite:       ...    # Alias: atlas, fast, tactical
  think:      ...    # Alias: pythagoras, reason, reasoning
  search:     ...    # Alias: york
```

This would lower the migration curve while maintaining the descriptive canonical names.
The parser would normalize aliases to canonical names at parse time.

### Future: Auto-Routing (Post-v1)

Long-term, Nika could offer an `auto` mode inspired by RouteLLM and R2-Router:

```yaml
agents:
  default: auto       # Router picks best model for creative tasks
  lite: auto           # Router picks best model for fast tasks
```

This is explicitly post-v1 scope (transparent manual routing first),
but the preset abstraction makes it a natural future extension.

---

## Devil's Advocate: Counter-Arguments

> Consolidated from the adversarial analysis session (2026-03-20).
> Steel-manning the opposition: the strongest arguments AGAINST the unified `agents:` block design.

### What the Unified Block Merges

Three previously separate concepts became one unified `agents:` block:

```
CONCEPT 1: model aliases (lightweight)
  Purpose: Named model aliases (default, lite, think, search)
  Minimal: Just provider + model ID
  Example: lite: { provider: groq, model: llama-3.3-70b }

CONCEPT 2: satellites (medium)
  Purpose: Worker templates dispatched by orchestrator
  Rich:    Accept/produce MIME types, tools, agent preset, instructions
  Example: vision-analyst: { model: gpt-4o, accepts: image, tools: [read] }

CONCEPT 3: agent persona (full)
  Purpose: Reusable agent identity with instructions + tools + model + guardrails
  Full:    name, instructions, model, tools, guardrails, handoffs, output_type
  Example: code-reviewer: { instructions: "Review code...", tools: [...], guardrails: [...] }
```

### Risk: The Kubernetes Lesson -- Orthogonal Concerns

Kubernetes separates Pod/Service/Deployment/ConfigMap because they have different lifecycles, owners, and rates of change. In a team scenario:
- The **platform team** configures model routing (providers, API keys, cost limits, fallback chains).
- The **workflow author** configures agent behavior (instructions, tools, output schemas, guardrails).

If both live in one `agents:` block, you cannot share model configs without duplicating behavior, or lock down routing while letting users customize prompts.

**Assessment:** MEDIUM severity. Nika v0.x is single-user; becomes HIGH if targeting teams.

### Risk: Cognitive Overload -- Polymorphic Entries

Under the same `agents:` key, entries range from 2-line model aliases to 20-line full agent personas. This creates "what IS an agent?" confusion -- a known anti-pattern (Docker Compose `ports:` polymorphism).

```yaml
agents:
  lite:            # 2 lines -- just a model alias
    provider: groq
    model: llama-3.3-70b-versatile

  code-reviewer:   # 15 lines -- full agent persona
    provider: anthropic
    model: claude-sonnet-4-6
    instructions: |
      You are a senior code reviewer. Focus on security...
    tools: [nika:read, nika:exec]
    guardrails:
      output: [no-secrets-in-output]
    output_type: ReviewResult
```

**Assessment:** HIGH severity for DX with new users. Mitigated by Progressive Disclosure (shorthand syntax).

### Risk: The DRY Violation

Multiple agents using the same model must duplicate `provider: anthropic, model: claude-sonnet-4-6`. Changing the model requires updating N entries.

**Assessment:** HIGH severity. Scales with workflow complexity.

### Risk: Identity vs Execution Separation

No way to say "same agent persona, different model this time" without duplicating the entire agent definition. OpenAI's Assistants API solves this with per-Run overrides.

**Assessment:** HIGH severity. Real composability gap.

### Risk: Preset Proliferation

The jump from Slate's 4 to Nika's 8 shows pressure to add more. When native tools ship (translate, embed, audio, safety), 8 may not be enough.

**Assessment:** MEDIUM severity. Not a problem today; becomes one at 15+.

### Risk: Historical Precedent

Docker Compose v1->v3 progressively split concerns. Ansible roles split into galaxy+vault+inventory. React classes split into hooks. Terraform inlined providers were refactored to separate blocks. The pattern: unification -> split at scale.

**Assessment:** MEDIUM severity. Pattern-level concern.

### Recommended Mitigations

**Keep the unified `agents:` block** as the primary authoring surface (good DX for 80% case), but add two escape hatches:

**Mitigation A: Model References (solves DRY)**

```yaml
models:
  sonnet: { provider: anthropic, model: claude-sonnet-4-6 }

agents:
  default:
    model: sonnet            # reference, not inline
  code-reviewer:
    model: sonnet            # same reference, different behavior
    instructions: "..."
```

**Mitigation B: Per-Task Model Override (solves Identity vs Execution)**

```yaml
agents:
  code-reviewer:
    model: sonnet
    instructions: "Review code..."
    tools: [nika:read]

tasks:
  - id: quick-review
    agent: code-reviewer
    model: lite              # override model, keep instructions + tools
  - id: deep-review
    agent: code-reviewer
    model: think             # override model, keep instructions + tools
```

**What NOT to do:**
1. Do NOT split `agents:` into three separate blocks -- kills simplicity.
2. Do NOT add inheritance (`extends:`) -- config inheritance is a complexity trap.
3. Do NOT add more than 8-10 presets -- new capabilities should be custom agents, not built-in presets.

### Devil's Advocate Verdict

The unified `agents:` block is aligned with industry consensus. The strongest counter-arguments come from AutoGen's dependency injection, OpenAI's Run overrides, and Terraform's provider aliasing. Mitigations A and B address the urgent risks without abandoning the unified design.

**Confidence:** High on risk identification, Medium on mitigations (need prototyping).

---

## Sources

| # | Source | Type | Key Finding |
|:-:|--------|------|-------------|
| 1 | IDC, "The Future of AI is Model Routing" (Nov 2025)[^1] | Industry report | 70% enterprise adoption predicted by 2028 |
| 2 | Fortune, "GPT-5 Router Backlash" (Aug 2025)[^2] | News | Opaque routing causes user frustration |
| 3 | OpenRouter State of AI 2025[^3] | Data report | 100T+ tokens routed, two-stack pattern dominant |
| 4 | Swfte, "Intelligent LLM Routing" (Jan 2026)[^4] | Technical blog | 85% cost reduction with intelligent routing |
| 5 | RouteLLM, LMSYS (2024)[^5] | Open-source | 4 classifier types for cost-quality routing |
| 6 | FrugalGPT, Chen et al. (2023)[^6] | Academic paper | Cascading with up to 98% cost savings |
| 7 | AutoMix (2024)[^7] | Academic paper | Automatic routing between model sizes |
| 8 | R2-Router (Feb 2026)[^8] | Academic paper | Models as quality-cost curves |
| 9 | Dynamic Routing Survey (Feb 2026)[^9] | Survey paper | Routing outperforms any single model |
| 10 | KNN Routing (Oct 2025)[^10] | Academic paper | Simple methods beat complex routers |
| 11 | CrewAI docs (2026) | Framework docs | Per-agent LLM assignment |
| 12 | LangGraph docs (2025-2026) | Framework docs | Per-node model binding |
| 13 | Slate by Random Labs | Framework docs | 4-slot model system |
| 14 | Kosmoy, "6 AI Gateway Trends" (Jan 2026) | Industry analysis | Semantic routing as gateway trend |
| 15 | Kubernetes API design | Architecture reference | Separation of orthogonal concerns |
| 16 | AutoGen Component system | Framework source | Model client as injected dependency |
| 17 | OpenAI Assistants API | API reference | Per-Run model/instruction overrides |
| 18 | Terraform provider aliasing | IaC reference | Infrastructure config referenced, not inlined |

### Research Methodology

- 8 Perplexity searches covering naming, frameworks, gateways, academic papers, and KG conventions
- Cross-referenced with framework documentation (CrewAI, LangGraph, AutoGen, Semantic Kernel, DSPy)
- Validated against existing Nika vision documents (Doc 03, 05, 17)
- Adversarial analysis: steel-man opposition, cross-domain analogies (K8s, Terraform, Docker, React)
- Comparative analysis with Letta/MemGPT, LangGraph, CrewAI, SSGM, A-MAC conventions
- 14 repositories analyzed across Rust, Python, TypeScript, Go
- March 2026 data -- captures post-GPT-5 router landscape
- **Confidence**: High

---

<div align="center">

[<- 10 TUI Vision](./10-jarvis-tui-vision.md) . [Index](./00-README.md) . [15 Ecosystem ->](./15-ecosystem-coherence.md)

</div>

---

[^1]: IDC, "The Future of AI is Model Routing" -- https://www.idc.com/resource-center/blog/the-future-of-ai-is-model-routing/ (November 2025). 70% enterprise adoption predicted by 2028.
[^2]: Fortune, "GPT-5's model router ignited a user backlash against OpenAI" -- https://fortune.com/2025/08/12/openai-gpt-5-model-router-backlash-ai-future/ (August 2025).
[^3]: OpenRouter State of AI 2025 -- https://openrouter.ai/state-of-ai. 100T+ tokens routed. a16z analysis: https://a16z.com/state-of-ai/
[^4]: Swfte, "Intelligent LLM Routing: How Multi-Model AI Cuts Costs by 85%" -- https://www.swfte.com/blog/intelligent-llm-routing-multi-model-ai (January 2026).
[^5]: RouteLLM by LMSYS -- Open-source cost-quality router with SW ranking, matrix factorization, BERT, and causal LLM classifiers. https://github.com/lm-sys/RouteLLM (2024).
[^6]: FrugalGPT: How to Use Large Language Models While Reducing Cost and Improving Performance -- Chen et al. (2023). Cascading model selection with up to 98% cost reduction.
[^7]: AutoMix: Automatically Mixing Language Models -- Madaan et al. (2024). Automatic routing between model sizes based on query complexity.
[^8]: R2-Router: A New Paradigm for LLM Routing with Reasoning -- https://arxiv.org/html/2602.02823v1 (February 2026). Treats LLMs as quality-cost curves.
[^9]: Dynamic Model Routing and Cascading for Efficient LLM Inference -- https://arxiv.org/html/2603.04445v1 (February 2026). Comprehensive survey of multi-LLM routing approaches.
[^10]: Rethinking Predictive LLM Routing: When Simple KNN... -- https://openreview.net/pdf/09a1cf8eea342f695327cb4308918d85676c6637.pdf (October 2025). Non-parametric approaches for model selection.
