# 12 — Vegapunk Naming

> One Piece-inspired naming for Nika v0.30 architecture.
> v0 philosophy: no backward compatibility, no legacy, just rename.

**Status:** APPROVED | **Date:** 2026-03-14

---

## Origin

Nika (the project) is named after the Sun God Nika from One Piece — the Hito Hito no Mi, Model: Nika. The fruit of execution, freedom, and imagination. The runtime is the **body** that does.

The rest of the SuperNovae architecture maps deeply onto Dr. Vegapunk's satellite system from the Egghead arc.

---

## The Mapping

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                    ONE PIECE  ←→  SUPERNOVAE                                  ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  Nika (Sun God Fruit)          →  Nika (Runtime 🦋)                           ║
║  "Limited only by imagination"     "Limited only by the YAML you write"       ║
║  Execution · Freedom · Joy         Execution · Freedom · Workflows            ║
║                                                                               ║
║  Stella (corps original)       →  L'utilisateur                               ║
║  La conscience centrale             La volonté qui écrit les workflows         ║
║  Dirige les satellites              Dirige les agents et l'exécution           ║
║                                                                               ║
║  Punk Records                  →  NovaNet                                     ║
║  Cerveau externalisé                Knowledge graph externalisé                ║
║  Mémoire persistante partagée       Mémoire persistante (Neo4j)               ║
║  Survit à la mort des satellites    Survit aux exécutions de workflows         ║
║                                                                               ║
║  Egghead Island (laboratoire)  →  Egghead (DataStore)                         ║
║  Lab éphémère de Vegapunk           Mémoire in-memory d'un run                ║
║  Détruit pendant l'arc              Détruit à la fin du workflow               ║
║  Satellites y travaillent           Tasks y stockent leurs résultats           ║
║                                                                               ║
║  Shaka (PUNK-01, Sagesse)      →  Orchestrateur dynamique                     ║
║  Leader de facto des satellites     LLM qui dispatch les satellites            ║
║  Prend les décisions stratégiques   Décide quelles tasks lancer               ║
║  Le plus rationnel et prescient     Raisonne sur les Records accumulés         ║
║                                                                               ║
║  Edison (PUNK-03, Intelligence) →  Slot main (création)                       ║
║  Inventions et engineering          Génération créative, rédaction, code       ║
║                                                                               ║
║  Pythagoras (PUNK-04, Logique)  →  Slot reasoning (raisonnement)              ║
║  Analyse logique et computation     Extended thinking, analyse profonde        ║
║                                                                               ║
║  Atlas (PUNK-05, Force)         →  Slot tactical (exécution rapide)           ║
║  Puissance physique brute           Tasks structurées, rapides, pas chères     ║
║                                                                               ║
║  York (PUNK-06, Ressources)     →  Slot search (recherche)                    ║
║  Collecte de ressources             Recherche web, RAG, collecte d'info       ║
║                                                                               ║
║  Lilith (PUNK-02, Défense)      →  Security layer (doc only)                  ║
║  Protège Egghead des intrus         Guardrails, blocklist, path traversal     ║
║                                                                               ║
║  Den Den Mushi (transmissions)  →  Pas renommé (traces/events)                ║
║  Vegapunk Broadcast             →  Pas renommé                                ║
║  Poneglyphs (inscriptions)      →  Pas renommé (workflows restent workflows)  ║
║  Emet (Iron Giant)              →  Pas renommé (exec: reste exec:)            ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

---

## Décisions de renaming

### Confirmés

| Ancien | Nouveau | Scope | Raison |
|--------|---------|-------|--------|
| `Episode` | `Record` | Struct, YAML, tools, docs | Punk Records — mémoire compressée |
| `episodes` | `records` | Partout | Cohérence |
| `DataStore` | `Egghead` | Struct Rust interne | Lab éphémère du run |
| `orchestration: strategy` | `orchestration: shaka` | YAML, Rust | PUNK-01, le leader stratège |
| `strategy` (alias) | accepté | Parser YAML | Backward-friendly pour onboarding |
| `tactics` | `satellites` | YAML templates | Dispatchés par Shaka |
| `main` (slot) | `edison` | YAML + Rust | PUNK-03, intelligence/création |
| `tactical` (slot) | `atlas` | YAML + Rust | PUNK-05, force rapide |
| `reasoning` (slot) | `pythagoras` | YAML + Rust | PUNK-04, logique profonde |
| `search` (slot) | `york` | YAML + Rust | PUNK-06, ressources |
| `nika:episodes` | `nika:records` | Introspection tool | Cohérence Records |
| `nika:strategy_state` | `nika:shaka` | Introspection tool | Cohérence Shaka |

### Pas touché

| Concept | Raison |
|---------|--------|
| 5 verbes (infer, exec, fetch, invoke, agent) | Sacrés — ADR-001 |
| Traces / Events / EventLog | Technique pure, pas de lore dans l'observabilité |
| `security.rs` | Clarté > esthétique pour la sécurité. Lilith = doc seulement |
| Workflows (fichiers `.nika.yaml`) | Restent "workflows", pas poneglyphs |
| `McpClient` | Infrastructure MCP standard |
| `orchestration: dag` | Nom technique neutre pour le mode statique |
| `nika:dag_state` | Explicite pour un tool d'introspection |
| `nika:budget`, `nika:task_status`, `nika:context` | Utilitaire, pas de lore |

---

## YAML avant / après

### Model Slots

```yaml
# AVANT
models:
  main: claude-sonnet-4-20250514
  tactical: claude-haiku-35
  reasoning: claude-sonnet-4-20250514
  search: perplexity/sonar-pro

# APRÈS
models:
  edison: claude-sonnet-4-20250514          # PUNK-03 — création, génération
  atlas: claude-haiku-35             # PUNK-05 — tasks rapides, structurées
  pythagoras: claude-sonnet-4-20250514      # PUNK-04 — raisonnement profond
  york: perplexity/sonar-pro         # PUNK-06 — recherche, collecte
```

### Orchestration Shaka

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

# APRÈS
orchestration: shaka                         # PUNK-01 — le stratège
shaka:
  provider: anthropic
  model: claude-sonnet-4-20250514
  max_rounds: 10
  record_budget: 15000                       # budget total des Records

satellites:                                  # templates dispatchées par Shaka
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

# APRÈS
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
  - nika:episodes        # Records accumulés
  - nika:strategy_state  # État de la stratégie

# APRÈS
tools:
  - nika:records         # Records accumulés (Punk Records)
  - nika:shaka           # État de l'orchestrateur
```

---

## Rust avant / après

### Egghead (ex-DataStore)

```rust
// AVANT
pub struct DataStore {
    results: DashMap<String, TaskResult>,
    context: DashMap<String, Value>,
}

// APRÈS
pub struct Egghead {
    results: DashMap<String, TaskResult>,
    context: DashMap<String, Value>,
}
```

File rename: `src/store/datastore.rs` → `src/store/egghead.rs`

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

// APRÈS
pub struct Record {
    pub task_id: String,
    pub summary: String,
    pub key_findings: Vec<String>,
    pub confidence: f64,
    pub tokens_used: usize,
}
```

Files: `src/runtime/episode.rs` → `src/runtime/record.rs`
`src/runtime/episode_compress.rs` → `src/runtime/record_compress.rs`

### ModelSlots

```rust
// AVANT (planned)
pub struct ModelSlots {
    pub main: ModelConfig,
    pub tactical: ModelConfig,
    pub reasoning: ModelConfig,
    pub search: ModelConfig,
}

// APRÈS
pub struct ModelSlots {
    pub edison: ModelConfig,      // PUNK-03 — main creative work
    pub atlas: ModelConfig,       // PUNK-05 — tactical fast tasks
    pub pythagoras: ModelConfig,  // PUNK-04 — deep reasoning
    pub york: ModelConfig,        // PUNK-06 — search & retrieval
}
```

### Shaka Orchestrator

```rust
// AVANT (planned)
pub enum OrchestrationMode {
    Dag,
    Strategy,
}

pub struct StrategyRunner { ... }

// APRÈS
pub enum OrchestrationMode {
    Dag,
    Shaka,
}

pub struct ShakaRunner { ... }
```

---

## Codebase Impact Analysis

### Existing code (v0.27 — needs rename NOW)

| Rename | Occurrences | Files | Criticité |
|--------|-------------|-------|-----------|
| `DataStore` → `Egghead` | **668** | 29 (19 src + 10 tests) | **CRITIQUE** |
| `strategy` (DecomposeStrategy) | 51 | 13 | MOYEN (attention: `BackoffStrategy` n'est PAS à renommer) |
| `episode` | 17 | 1 (tier6.rs exemples) | FAIBLE |
| `tactics` | 0 | 0 | Pas encore implémenté |
| `model slots` | 0 | 0 | Pas encore implémenté |

### Planned code (v0.28-v0.30 — use new names directly)

| Feature | Fichiers à créer | Naming |
|---------|------------------|--------|
| Model routing (v0.28) | `src/runtime/model_slots.rs` | `ModelSlots { edison, atlas, pythagoras, york }` |
| Record compression (v0.28) | `src/runtime/record.rs`, `src/runtime/record_compress.rs` | `Record`, `RecordCompressor` |
| Shaka orchestration (v0.29) | `src/runtime/shaka.rs`, `src/runtime/shaka_runner.rs`, `src/ast/shaka.rs` | `ShakaRunner`, `ShakaConfig`, `Satellite` |
| Context budgets (v0.29) | `src/runtime/budget.rs` | Pas de rename (technique) |
| NovaNet memory (v0.30) | `src/runtime/memory.rs` | `PersistentRecord` (Records stored in NovaNet) |
| Introspection (v0.30) | 6 builtin tools | `nika:records`, `nika:shaka`, rest technique |

### Detailed file impact: DataStore → Egghead

**Tier 1 — Critical (>50 occurrences):**

| File | Count | Nature |
|------|-------|--------|
| `src/store/datastore.rs` | 164 | Struct definition, methods, tests |
| `src/binding/template.rs` | 119 | Template resolution with lazy bindings |
| `src/binding/resolve.rs` | 94 | Binding resolution, input paths |
| `src/runtime/runner.rs` | 77 | Core executor, dependency tracking |
| `src/runtime/executor/tests.rs` | 66 | Unit tests |

**Tier 2 — High (10-50 occurrences):**

| File | Count | Nature |
|------|-------|--------|
| `tests/lazy_binding_test.rs` | 57 | Integration tests |
| `tests/binding_integration.rs` | 46 | Integration tests |
| `tests/fetch_wiremock_test.rs` | 39 | HTTP mock tests |
| `tests/executor_fetch_errors_test.rs` | 26 | Error tests |
| `src/runtime/artifact_processor.rs` | 24 | Artifact system |
| `src/runtime/executor/verbs.rs` | 18 | Verb implementations |
| `src/runtime/executor/decompose.rs` | 16 | Dynamic decomposition |

**Tier 3 — Low (<10 occurrences):**
14 additional files with 1-15 occurrences each.

### Detailed file impact: strategy → shaka

**WARNING**: Only `DecomposeStrategy` and orchestration-related `strategy` references should be renamed. `BackoffStrategy` in `src/jobs/retry.rs` is a **different concept** (retry backoff) and MUST NOT be renamed.

| File | Count | What to rename |
|------|-------|---------------|
| `src/ast/decompose.rs` | 14 | `DecomposeStrategy` → `DecomposeMode` or keep (it's about decompose, not Shaka) |
| `src/init/tier6.rs` | 10 | Example workflow variable names |
| `src/ast/schema_validator.rs` | 5 | Schema validation examples |
| `src/runtime/executor/decompose.rs` | 2 | Strategy field matching |
| `src/runtime/runner.rs` | 1 | Logging |

**Note**: `DecomposeStrategy { Semantic, Static, Nested }` is about how `decompose:` works, not about the Shaka orchestration mode. Consider renaming to `DecomposeMode` to avoid confusion, but this is NOT the same as `orchestration: shaka`.

---

## Refactoring Plan

### Philosophy

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  v0 PHILOSOPHY                                                                ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  • No backward compatibility                                                  ║
║  • No deprecated aliases in code                                              ║
║  • No "old name → new name" shims                                             ║
║  • No migration guides                                                        ║
║  • Just rename. Clean. Done.                                                  ║
║                                                                               ║
║  Exception: orchestration: strategy|shaka in YAML parser                      ║
║  (both accepted, shaka is canonical)                                          ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### Phase 1 — DataStore → Egghead (v0.28)

The biggest rename. 668 occurrences, 29 files. Pure mechanical refactor.

```bash
# 1. Rename the file
mv src/store/datastore.rs src/store/egghead.rs

# 2. Global rename in src/
sed -i 's/DataStore/Egghead/g' src/**/*.rs
sed -i 's/datastore/egghead/g' src/**/*.rs
sed -i 's/data_store/egghead/g' src/**/*.rs

# 3. Global rename in tests/
sed -i 's/DataStore/Egghead/g' tests/**/*.rs
sed -i 's/datastore/egghead/g' tests/**/*.rs

# 4. Update mod.rs exports
# pub mod egghead;
# pub use egghead::Egghead;

# 5. cargo test (6,157 tests must pass)
# 6. cargo clippy -- -D warnings
```

**Risk**: LOW — pure rename, no logic change. All tests validate the same behavior.

### Phase 2 — Record infrastructure (v0.28)

Create new files with correct naming from day 1:

```
src/runtime/record.rs           # Record struct
src/runtime/record_compress.rs  # RecordCompressor (tactical LLM summarization)
```

Add `record:` field to task AST in `src/ast/action.rs`.

### Phase 3 — Model Slots as Satellites (v0.28)

Create new files with correct naming:

```
src/runtime/model_slots.rs      # ModelSlots { edison, atlas, pythagoras, york }
src/ast/model_slots.rs          # YAML parsing for models: block
```

### Phase 4 — Shaka Orchestrator (v0.29)

Create new files:

```
src/runtime/shaka.rs            # ShakaRunner — dynamic orchestration
src/runtime/shaka_dispatch.rs   # Satellite dispatch logic
src/ast/shaka.rs                # ShakaConfig, Satellite AST parsing
```

Add `orchestration:` field to workflow AST. Parser accepts both `shaka` and `strategy`.

### Phase 5 — Introspection tools (v0.30)

Add to `src/runtime/builtin/`:

```
records.rs     # nika:records — accumulated Records
shaka.rs       # nika:shaka — Shaka orchestrator state
```

Other introspection tools keep technical names (dag_state, budget, task_status, context).

---

## Brainstorm docs update scope

| File | Occurrences to update | Priority |
|------|----------------------|----------|
| `05-evolution-roadmap.md` | ~63 | HIGH — core roadmap |
| `08-nika-030-complete-guide.md` | ~33 | HIGH — user guide |
| `11-nika-030-technical-reference.md` | ~100+ | HIGH — just created |
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
╔═══════════════════════════════════════════════════════════════════════════════╗
║                    VEGAPUNK NAMING — QUICK REFERENCE                          ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  🦋 NIKA        = Le runtime (exécution, liberté)                             ║
║  ⭐ STELLA       = L'utilisateur (la volonté qui dirige)                      ║
║  🧠 NOVANET     = Punk Records (mémoire persistante)                         ║
║  🏝️ EGGHEAD     = DataStore (mémoire éphémère du run)                        ║
║                                                                               ║
║  🎯 SHAKA       = Orchestrateur dynamique (PUNK-01, sagesse)                  ║
║  💡 EDISON      = Slot main — création (PUNK-03, intelligence)                ║
║  🧮 PYTHAGORAS  = Slot reasoning — logique (PUNK-04, calcul)                  ║
║  💪 ATLAS       = Slot tactical — force rapide (PUNK-05, puissance)           ║
║  🔍 YORK        = Slot search — ressources (PUNK-06, collecte)               ║
║  🛡️ LILITH      = Security layer — doc only (PUNK-02, défense)               ║
║                                                                               ║
║  📦 RECORD      = Résultat compressé (ex-Episode, Punk Records)               ║
║  🛰️ SATELLITE   = Task template dispatchée par Shaka (ex-tactic)             ║
║                                                                               ║
║  orchestration: dag     → mode statique (inchangé)                            ║
║  orchestration: shaka   → mode dynamique (alias: strategy)                    ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```
