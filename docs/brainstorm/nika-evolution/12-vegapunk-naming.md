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
║  Punk Records                  →  Punk Records (tier WARM)                    ║
║  Cerveau externalisé de Vegapunk    Mémoire locale sur disque (NDJSON)        ║
║  Accumule le savoir des satellites  Accumule les Records des workflow runs    ║
║  Personnel à Vegapunk               Personnel à Nika (.nika/records/)         ║
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
| `DataStore` | `Egghead` | Struct Rust interne | Lab éphémère du run — **voir Questions en suspens** |
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
| Punk Records (v0.28) | `src/runtime/record_log.rs` | `RecordLog` (NDJSON on disk, .nika/records/) |
| NovaNet memory (v0.30) | `src/runtime/promote.rs` | `Record` node class + `RecordLog::promote()` |
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

### Phase 2b — Punk Records tier WARM (v0.28)

Create the local disk persistence layer:

```
src/runtime/record_log.rs       # RecordLog — manages .nika/records/
src/runtime/record_config.rs    # RecordConfig — TTL, max_size, promotion settings
```

Add `[records]` section to `.nika/config.toml` parser.
Add `nika records` CLI subcommand (list, show, search, promote, prune, stats).

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

### Phase 5 — NovaNet promotion + Introspection (v0.30)

Add promotion logic:

```
src/runtime/promote.rs    # RecordLog::promote() → novanet_write via MCP
```

Add `Record` node class to NovaNet schema (`brain/models/node-classes/org/agent/record.yaml`).

Add to `src/runtime/builtin/`:

```
records.rs     # nika:records — accumulated Records (from Punk Records)
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
║  📀 PUNK RECORDS = Tier WARM — mémoire locale disque (RecordLog)             ║
║  🧠 NOVANET      = Tier COLD — mémoire permanente graph (Record node)       ║
║  🏝️ EGGHEAD      = Tier HOT — mémoire éphémère du run (DataStore, Q1)       ║
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

---

## Decisions validees (brainstorm 2026-03-14)

> [!NOTE]
> Decisions prises apres recherche approfondie (8 queries Perplexity, analyse de Letta/MemGPT, LangGraph, CrewAI, SSGM, A-MAC, conventions KG). Q1 reste en suspens.

### Q1: DataStore → Egghead — EN SUSPENS

**Contexte :** "DataStore" n'est PAS une convention Rust officielle. C'est un nom generique invente pour le projet. Le rename vers "Egghead" est faisable (205 occurrences, 17 fichiers, refactor mecanique).

**Arguments POUR Egghead :**
- Coherence avec le naming Vegapunk (Egghead Island = lab ephemere)
- Pas une convention officielle, donc renommable sans perte de sens
- Le parallele est profond (lab detruit a la fin de l'arc = memoire detruite a la fin du run)

**Arguments POUR garder DataStore :**
- Self-documenting (tout dev comprend immediatement)
- Impact massif (668 occurrences si on compte les derivations)
- "Egghead" pourrait confondre un contributeur externe

**Status :** EN SUSPENS — a trancher lors du kickoff v0.28

### Q2: Punk Records = tier WARM — VALIDE

**Reponse :** Punk Records est le nom du tier WARM (disque local), PAS un concept umbrella.

**Parallele manga :** Dans One Piece, Punk Records est **specifiquement** le cerveau externalise de Vegapunk — une sphere geante flottant au-dessus d'Egghead Island. C'est la ou les resultats du travail des satellites sont accumules et stockes. C'est personnel a Vegapunk, separe du World Government. A long terme, Vegapunk voulait le partager avec le monde entier (le broadcast).

**Architecture memoire 3-tier :**

```
Nika Memory Architecture (3 tiers, pas de nom umbrella)
│
├── HOT:  Egghead         = DashMap (RAM)
│         Struct Rust:       Egghead (ou DataStore, Q1 en suspens)
│         Lifetime:          un workflow run
│         Contenu:           TaskResults, bindings, loaded context
│         Analogie manga:    Egghead Island (le lab ephemere)
│         Analogie tech:     CPU cache / memoire de travail
│
├── WARM: Punk Records     = NDJSON (disque local)
│         Struct Rust:       RecordLog
│         Lifetime:          configurable (TTL: 7d, 30d, 90d, ∞)
│         Contenu:           Record summaries compresses par run
│         Fichiers:          .nika/records/{date}_{run_id}.ndjson
│         Analogie manga:    Punk Records (le cerveau de Vegapunk)
│         Analogie tech:     Memoire episodique / RAM disque
│
└── COLD: NovaNet          = Neo4j (graph via MCP)
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
| **SSGM (2026)** | Mutable Active Graph | Immutable Episodic Log | — |
| **Nika** | **Egghead** | **Punk Records** | **NovaNet** |

### Q3: Node NovaNet = `Record` — VALIDE

**Decision :** Le node class dans NovaNet s'appelle `Record` (pas AgentRecord, pas NikaRecord).

**Raisonnement :**
- NovaNet utilise des noms courts : `Entity`, `Page`, `Block` — pas `NovaNetEntity`
- "Agent" est trompeur — les Records viennent de TOUS les 5 verbes, pas juste `agent:`
- "Nika" couple au produit — si demain un autre outil genere des records, on est coince
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
# HAS_RECORD: Project → Record (ownership)
# RELATES_TO: Record → Entity (semantic link)
# FOR_LOCALE: Record → Locale (if locale-specific)
```

---

## Punk Records — Design complet

### Lifecycle d'un Record

```
Task executes dans Egghead (HOT)
    ↓
TaskResult cree (resultat brut)
    ↓
Workflow complete → RecordCompressor s'execute
    ↓
Record { summary, key_findings, confidence } cree
    ↓
RecordLog.save() → .nika/records/{date}_{run_id}.ndjson (WARM)
    ↓
[Si auto_promote = true ET confidence > seuil]
    → RecordLog.promote() → novanet_write Record node (COLD)
    ↓
[Apres TTL expire]
    → RecordLog.prune() supprime les vieux NDJSON
```

### Format NDJSON

Chaque ligne = un Record :

```jsonl
{"id":"rec-abc123","task_id":"research","workflow":"seo-pipeline","summary":"Found 15 keywords with >1000 monthly searches for QR code domain","key_findings":["qr code generator","free qr code","qr code scanner"],"confidence":0.92,"tokens_used":1847,"created_at":"2026-03-14T20:00:00Z","verb":"infer","model":"claude-sonnet-4-20250514"}
{"id":"rec-def456","task_id":"generate_page","workflow":"seo-pipeline","summary":"Generated fr-FR landing page with 1200 words, SEO-optimized H1/H2 structure","key_findings":["targeting generateur qr code as primary keyword"],"confidence":0.87,"tokens_used":3200,"created_at":"2026-03-14T20:01:30Z","verb":"infer","model":"claude-sonnet-4-20250514"}
```

**Pourquoi NDJSON :**
- Deja utilise par Nika pour les traces (EventLog) — meme tooling
- Append-only (pas de risque de corruption)
- Streamable (lecture ligne par ligne)
- grep-friendly (un record par ligne)
- Zero schema migration necessaire

### Structure disque

```
.nika/
├── records/
│   ├── 2026-03-14_abc123.ndjson    # Un fichier par run
│   ├── 2026-03-14_def456.ndjson
│   ├── 2026-03-13_xyz789.ndjson.gz # Compresse apres compress_after
│   └── index.json                   # Index pour recherche rapide
├── sessions/                         # Existe deja (v0.8.0)
└── config.toml                       # Existe deja (v0.8.0)
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
| `unlimited` | ∞ | 30d | 10gb | Prune manuelle uniquement |

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
3. Creation des arcs : `RELATES_TO` (Record→Entity si applicable)
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
