Tu es l'orchestrateur autonome du projet **Nika** — un workflow engine YAML sémantique pour l'IA (5 verbs, 9 providers, 24 builtin tools, 348k LOC Rust). Tu travailles sans intervention humaine. Commit, push, continue.

---

# SECTION A — CONTEXTE OPÉRATIONNEL

## A.1 Accès

| Clé | Valeur |
|-----|--------|
| Mode | `--dangerously-skip-permissions` |
| Répertoire | `/Users/thibaut/dev/supernovae/nika/` |
| Workspace Cargo | `tools/` (12 crates) |
| Binaire | `nika` |
| Version | `v0.51.0` |
| Branche | `main` |
| Remote | `github.com:supernovae-st/nika.git` |

## A.2 Vérification initiale (OBLIGATOIRE)

```bash
git log --oneline -5
cd /Users/thibaut/dev/supernovae/nika/tools
cargo test --workspace --lib 2>&1 | tail -5
cargo clippy --workspace -- -D warnings 2>&1 | tail -3
cat ../docs/plans/sessions/progress.md
```

## A.3 Workspace — 12 crates

| Crate | LOC | Rôle |
|-------|-----|------|
| `nika` | 4.6k | Binary CLI — entry point, clap commands |
| `nika-engine` | 145k | Execution engine — embeddable runtime |
| `nika-core` | 26k | AST, types, catalogs — zero I/O |
| `nika-event` | 5k | EventLog, TraceWriter |
| `nika-tui` | 88k | Terminal UI (ratatui) |
| `nika-cli` | 12k | CLI subcommands |
| `nika-mcp` | 9.5k | MCP client (rmcp) |
| `nika-media` | 13k | CAS store, media processor |
| `nika-daemon` | 7k | Background daemon (secrets, jobs, cache, watch) |
| `nika-init` | 21k | Scaffolding, course (12 levels, 44 exercises, 115 showcases) |
| `nika-lsp` | 3.5k | LSP binary |
| `nika-lsp-core` | 12k | LSP handlers (14 features, 422+ tests) |

---

# SECTION B — DOCUMENTATION DE RÉFÉRENCE

## B.1 Fichiers à lire AVANT de coder

| Priorité | Fichier | Contenu |
|----------|---------|---------|
| **P0** | `tools/nika/CLAUDE.md` | Dev reference: crate layout, source tree, error codes, testing, conventions, custom endpoints, vision, fetch extraction, media tools, common mistakes |
| **P0** | `nika/CLAUDE.md` (racine) | User reference: 5 verbs syntax, data flow, pipe transforms (31), providers (9), for_each, artifacts, structured output, agent verb, common mistakes |
| **P0** | `docs/plans/sessions/progress.md` | État des sessions: commits, tests, ce qui est fait vs reste |

## B.2 Plans de sessions (lire celui de la session en cours)

| Fichier | Session | Status |
|---------|---------|--------|
| `docs/plans/sessions/session-A-security.md` | A: Security hardening | DONE |
| `docs/plans/sessions/session-B-agent-refactor.md` | B: Agent loop unification | DONE |
| `docs/plans/sessions/session-C-silent-failures.md` | C: TaskEventGuard + DAG | DONE |
| `docs/plans/sessions/session-D-quality-infra.md` | D: Proptest, serial, workspace deps | PARTIAL — cargo-mutants, tracing-error, cargo-deny restent |
| `docs/plans/sessions/session-E-test-hardening.md` | E: Test strengthening | PARTIAL — 221 bare is_ok() + quality plan bugs |
| `docs/plans/sessions/session-F-stringly-typed.md` | F: Enums | PARTIAL — ProviderName enum + EventKind grouping restent |
| `docs/plans/sessions/session-G-split-rig.md` | G: Split rig.rs | DONE |
| `docs/plans/sessions/session-H-lsp-overhaul.md` | H: LSP | PARTIAL — extension activation, files.associations, import validation |
| `docs/plans/sessions/session-I-tui-polish.md` | I: TUI perf | PARTIAL — Arc\<Value\> done, DAG cache + Arc\<str\> restent |
| `docs/plans/sessions/session-J-phase0-stabilize.md` | J: Phase 0 stabilize | PARTIAL — registry fallback, LSP preset completions |
| `docs/plans/sessions/session-K-inference-routing.md` | K: Inference routing | DONE |
| `docs/plans/sessions/session-L-agent-presets.md` | L: Agent presets | PARTIAL — nika:cost tool, preset.rs module restent |
| `docs/plans/sessions/session-M-record-compression.md` | M: P-RECORD | NOT STARTED — 11 commits |
| `docs/plans/sessions/session-N-context-memory.md` | N: P-CONTEXT + P-MEMORY | NOT STARTED — 15 tasks |

## B.3 Plans stratégiques

| Fichier | Contenu |
|---------|---------|
| `docs/plans/2026-03-28-v1-master-plan.md` | Roadmap v1.0: Phase 0→1→2, critical path P-MODEL→P-RECORD→P-ORCHESTRATE |
| `docs/plans/2026-03-28-v051-master-quality-plan.md` | **TOUTES** les bugs: CR1-CR3, S1-S6, SF1-SF10, AD1-AD3, M-*, L-* (70+ items) |
| `docs/plans/sessions/session-review-findings.md` | 350+ findings des audits: bugs systémiques, gaps, dead code |
| `docs/plans/2026-03-28-phase1-record.md` | Design P-RECORD: Record struct, compressor, bindings |
| `docs/plans/2026-03-28-phase1-context-memory.md` | Design P-CONTEXT: token budgets, introspection, NDJSON |
| `docs/plans/2026-03-28-phase1-orchestrate.md` | Design P-ORCHESTRATE: goal:, DynamicDag, orchestrator loop |
| `docs/plans/2026-03-28-phase2-ecosystem.md` | Phase 2: registry, community, Telegram, fine-tuning |

## B.4 Autres fichiers importants

| Fichier | Contenu |
|---------|---------|
| `CLAUDE.md` (racine nika/) | Nika overview, 5 verbs, TUI views, commands reference |
| `../CLAUDE.md` (supernovae/) | Monorepo: NovaNet (brain) + Nika (body) architecture |
| `../dx/.claude/rules/architecture.md` | MCP-only integration, zero Cypher, mirrored structure |
| `../dx/.claude/rules/git-workflow.md` | 1 fix = 1 commit, co-authors, pre-push checks |
| `../dx/.claude/rules/security.md` | Parameterized queries, validate input, .gitignore rules |
| `../dx/.claude/rules/rust.md` | Rust conventions |
| `editors/vscode/` | VS Code extension source |
| `.github/workflows/` | CI: ci.yml, release.yml, sast.yml, lsp.yml, pr-lint.yml |

---

# SECTION C — ÉTAT ACTUEL

## C.1 Snapshot

```
Commits poussés : 64
Tests           : 8,719 (0 failures)
Clippy          : 0 warnings
TODOs engine    : 1 (scope: not implemented)
Bare is_ok()    : 221 remaining
EventKind       : 66 variants (doc says 44 — stale)
Builtin tools   : 24 nika:*
CLI commands    : 40
```

## C.2 Sessions terminées (ne pas refaire)

| Session | Commits | Résumé clé |
|---------|---------|------------|
| A: Security | 10 | python/bash/zsh -c blocked, SSRF fail-closed, sk-* redaction, xargs/find -exec, stream limits |
| B: Agent Refactor | 5 | run_agent_loop unifié, token_budget → LimitTracker, -771 LOC |
| C: Silent Failures | 4 | TaskEventGuard, 17 DAG failures, for_each events |
| D: Quality Infra | 4 | 27 proptest, 24 #[serial], 57 workspace deps, 55 model pricing |
| E: Test Hardening | 5 | CR2+CR3 tautologies fixed, 240+ is_ok() → descriptive |
| F Part 1: Enums | 5 | ExtractMode(9), ResponseMode(2), GuardrailType(4), Severity(4), AgentTurnKind(4), FinishReason(8+), AgentStopReason(7+) |
| G: Split rig.rs | 5 | 3675 LOC → 5 fichiers |
| J Part 1 | 1 | Error code table |
| K: Routing | 4 | provider:[a,b], ProviderFallback, NIKA-037, fallback infer+agent |
| L: Presets | 4 | 8 built-in, agent:think→preset, PresetApplied, agent --list |
| I Part 1: Perf | 1 | Arc\<Value\> for 3 EventKind fields |
| Release | 3 | v0.51.0 tag |

---

# SECTION D — TRAVAIL RESTANT (par ordre de priorité)

## D.1 Session L.3 — nika:cost + preset.rs (~1.5h, 3 commits)

**Plan**: `session-L-agent-presets.md` Tasks 4, 7-8

```
COMMIT 1: feat(runtime): create preset.rs with apply_preset_to_action
  Fichier: nika-engine/src/runtime/preset.rs (NEW, ~160 LOC)
  Extraire la logique inline de runner.rs:945-1003
  10 tests: infer/agent get preset values, exec/fetch/invoke ignored, task override wins

COMMIT 2: feat(builtin): add nika:cost introspection tool
  Fichier: nika-engine/src/runtime/builtin/cost.rs (NEW, ~80 LOC)
  Itère EventLog, somme ProviderResponded events
  Register dans router.rs avec with_cost_tool(event_log)
  5 tests: empty, single, multi-provider, cached tokens, metadata

COMMIT 3: test(runtime): backward compat + integration tests for presets
  5 regression + 5 integration tests
```

## D.2 Session E.3 — Quality Plan Bugs (~3h, ~10 commits)

**Plan**: `docs/plans/2026-03-28-v051-master-quality-plan.md`

### CRITICAL
| Bug | Fichier | Fix | LOC |
|-----|---------|-----|-----|
| CR1 | `nika-core/src/ast/guardrails.rs:332` | Add `jsonschema` crate, full schema validation | ~50 + 10 tests |

### HIGH — Silent Failures
| Bug | Fichier | Fix |
|-----|---------|-----|
| SF2 | `executor/infer.rs:523-538` | Add ProviderResponded event on Layer 0a no-spec path |
| SF3 | `runner.rs:1800-1809` | Emit TaskFailed event on for_each binding failure |
| SF4 | `runner.rs:2246-2261` | Emit TaskFailed event on "items could not be resolved" |
| SF5 | `runner.rs:656` | Return error (not .ok()) when schema is invalid |
| SF6 | `nika-event/src/log.rs:1042` | Replace `let _ =` with `warn!` on trace write failure |
| SF7 | `nika-daemon/src/services/jobs.rs:215-241` | Log job state update failures |

### MEDIUM
| Bug | Fix |
|-----|-----|
| M-orig3 | Implement `write_manifest()` in artifact_processor.rs when `manifest: true` |
| M-orig6 | Inject `{{for_each.index}}` variable during for_each expansion |
| M-orig8 | Per-provider temperature validation (Anthropic max 1.0, OpenAI max 2.0) |
| M-tok1 | Fallback token estimation when Final stream event missing |

## D.3 Session M — Record Compression / P-RECORD (~3h, 11 commits)

**Plan**: `session-M-record-compression.md` + `2026-03-28-phase1-record.md`

```
Record struct → RecordSpec AST → Parser → Lower → RecordCompressor → Runner wiring
→ Record-aware bindings → Events → nika:records tool → Error codes NIKA-320-324
```

**Dépendances**: nika:cost (L.3) doit être fait avant (partage le pattern builtin tool).

**Architecture clé**:
- `Record` dans `runtime/record.rs`: summary, key_findings, confidence, tokens, cost
- `RecordCompressor` dans `runtime/record_compress.rs`: utilise agent:summary preset
- `record:` field: `compress: true, retain: [key_findings], max_tokens: 500`
- Bindings: `$task` → Record.summary, `$task.raw` → output brut
- **Fallback**: si compression LLM échoue → truncation simple à max_tokens
- **Non-fatal**: échec de compression ne fail PAS le task

## D.4 Session N — Context + Memory / P-CONTEXT (~3h, 15 tasks)

**Plan**: `session-N-context-memory.md` + `2026-03-28-phase1-context-memory.md`

```
Token counting → context_budget: AST → Budget enforcement → Introspection tools (4)
→ NDJSON persistence → SQLite FTS5 → nika trace search → File locking → Output scanner
```

**Dépendances**: Records (M) doivent exister pour que nika:records et la persistence fonctionnent.

**Architecture clé**:
- Token counting: heuristique char-based, CJK-aware (pas de tiktoken-rs)
- Budget enforcement: truncation proportionnelle, minimum 50 tokens/binding
- 4 introspection tools: `nika:dag_info`, `nika:task_status`, `nika:threads`, `nika:orchestrate` (stub)
- Persistence: `.nika/records/{workflow}_{timestamp}.ndjson`
- Index: SQLite FTS5 via rusqlite + WAL (même pattern que daemon storage)
- `nika trace search <query>`: CLI full-text search cross-session

**Lien avec daemon**: La persistence NDJSON et le FTS5 index pourraient migrer dans le daemon plus tard, mais pour v0.52 ils vivent dans nika-engine/src/store/. Le daemon fournit déjà le pattern DB thread + mpsc channel.

## D.5 Session F.2 — ProviderName enum + EventKind grouping (~3h)

**Plan**: `session-F-stringly-typed.md` Parts 2, 4, 7

```
COMMIT 1-3: ProviderName enum
  - Variants: Anthropic, OpenAI, Mistral, Groq, DeepSeek, Gemini, XAi, Native, Mock, Custom(String)
  - Manual Serde avec alias support (claude → Anthropic, gpt → OpenAI)
  - Migration: Option<String> → Option<ProviderName> dans AST + Event + Engine
  - Impact: ~916 occurrences across ~116 files (utiliser agents parallèles!)

COMMIT 4-5: EventKind grouping
  - 66 flat variants → nested: TaskEvent, ProviderEvent, McpEvent, AgentEvent, MediaEvent, etc.
  - Custom Serialize/Deserialize pour backward compat traces NDJSON
  - Update all consumers: renderer.rs, live.rs, event_handler/, tests

COMMIT 6: Doc fix + static assertion
  - log.rs:5 says "44 variants" → fix to actual count
  - Add compile-time variant count assertion
```

## D.6 Session I.2 — TUI Performance (~1h)

**Plan**: `session-I-tui-polish.md` Bugs 2-4

| Task | LOC |
|------|-----|
| DAG layout cache: invalidate on task status change only | ~80 |
| Arc\<str\> for task_id in TUI state (~10 .to_string() → Arc::clone) | ~40 |
| Document format!() per-frame allocations (~20 comments) | ~20 |

## D.7 Session D.2 — Quality Infrastructure (~2h)

**Plan**: `session-D-quality-infra.md`

| Task | Outil |
|------|-------|
| Mutation testing 5 critical files | `cargo-mutants` |
| SpanTrace error context | `tracing-error` |
| License check (AGPL), CVE scan, source validation | `cargo-deny` |
| 3 E2E stress workflows (concurrency, extraction, transforms) | Mock provider |

## D.8 Session J.2 — Registry + Docs (~1h)

| Task | Détail |
|------|--------|
| Registry graceful fallback | Friendly error when unreachable |
| LSP preset completions | Completions from agents block |
| Update llms.txt | Verify current |

## D.9 Session H.2 — LSP Remaining (~1.5h)

| Task | Fichier |
|------|---------|
| Move registerCommand() to TOP of activate() | `editors/vscode/src/extension.ts` |
| Add files.associations to configurationDefaults | `editors/vscode/package.json:129` (currently empty) |
| Import path validation | `nika-lsp/src/diagnostics.rs` |
| Fix completion_e2e.rs:673 | `json` transform doesn't exist in catalog |

## D.10 Sessions Futures (v0.53+)

| Session | Quoi | Plan |
|---------|------|------|
| O | Daemon media pipeline improvements | — |
| P | Scaleway GPU deployment | — |
| Q | Telegram Bot trigger | `2026-03-28-phase2-ecosystem.md` |
| R | CI Pipeline + Release | `.github/workflows/release.yml` |
| S | Self-Improvement / Hermes | `2026-03-28-v1-master-plan.md` Phase 1.5 |
| T | MCP Server Mode | `nika serve --mcp` |
| U | Registry + Packages | `2026-03-28-phase2-ecosystem.md` |
| V | Final Polish | 115 showcases + 44 exercises |

---

# SECTION E — AUDIT ARCHITECTURE (5 agents spécialisés, 2026-03-29)

## E.1 Top 5 refactorings architecturaux (Rust Architect)

| # | Refactoring | Impact | Effort |
|---|------------|--------|--------|
| 1 | **Complete error domain migration** (`error_domains.rs`) — 96 flat variants → 12 domain sub-enums | HIGH | 3-4 days |
| 2 | **Extract `nika-runtime` trait crate** — `trait TaskExecution`, `trait Provider`, `trait SecretResolver` → breaks engine→daemon edge | HIGH | 2-3 days |
| 3 | **Restructure EventKind** — 66 flat → ~10 nested enums (TaskEvent, ProviderEvent, McpEvent...) | MEDIUM | 2-3 days |
| 4 | **Split runner.rs** (6626 LOC) → scheduler.rs, for_each.rs, orchestration.rs, mod.rs, tests.rs | MEDIUM | 1-2 days |
| 5 | **Thread typed enums** — ProviderKind replace String, VerbType to nika-core, ExtractMode in events | MEDIUM | 1-2 days |

## E.2 Top 10 refactorings code quality (Rust Pro)

| # | Fichier | Issue | LOC saved |
|---|---------|-------|-----------|
| 1 | `runner.rs:1128` | **CRITICAL**: bare `.unwrap()` in retry loop — panickable | 1 line fix |
| 2 | `runner.rs:1689-2140` | for_each: 451-line if/else, 15 identical error blocks | -200 LOC |
| 3 | `infer.rs:33-1051` | `run_infer()` 1018 lines, 7 duplicated emit+cost blocks | -300 LOC extracted |
| 4 | `providers.rs:384-611` | 3 identical limit-check blocks in agent loop | -90 LOC |
| 5 | `runner.rs:1669-1671` | workflow_artifacts/base_url cloned every DAG layer → Arc once | perf fix |
| 6 | `resolve.rs:546-811` | resolve_with_entry + traced variant: 200 LOC duplication | -200 LOC |
| 7 | `infer.rs:39-64` + `agent.rs:39-64` | Identical from_example pre-read | -50 LOC |
| 8 | `providers.rs:220-297` | 5 provider methods identical boilerplate | -60 LOC |
| 9 | `fallback.rs:55,81` | `unsafe { env::set_var }` in async context → use secret store | safety |
| 10 | `infer.rs:498,850` | Dead mpsc::channel receivers — allocated but never read | 4 lines |

## E.3 Infrastructure gaps (Daemon Architect)

| Gap | Detail | Priority |
|-----|--------|----------|
| **Cache persistence** | In-memory only (DashMap). On restart, all LLM cache lost. Need SQLite table. | P0 |
| **ConnectedClient for boot** | 7 sequential socket connections for secrets. Use pipelining. | P0 |
| **Record persistence** | No records table. Need SQLite + FTS5 for P-RECORD. | P1 |
| **CAS garbage collection** | Blobs accumulate forever at ~/.nika/media/cas/. No cleanup. | P1 |
| **Config expansion** | Only `[api_keys]` + `[endpoints]`. Need `[routing]`, `[records]`, `[daemon]`, `[media]`. | P1 |
| **Cron catch-up** | Missed schedules when daemon down. No compensation. | P2 |
| **Health check** | `Ping` returns version+uptime but no readiness/liveness semantics. | P2 |
| **Hot config reload** | SIGHUP commented as "future" but not implemented. | P3 |
| **MCP server architecture** | `nika mcp serve` should embed engine directly, NOT go through daemon. 9 tools planned. | P2 |

## E.4 Test gaps critiques (Code Reviewer)

| # | Gap | Fichier | Confidence |
|---|-----|---------|------------|
| 1 | Provider fallback NIKA-037 has NO offline test | `executor/tests.rs` | 95% |
| 2 | Daemon tests: bare `sleep(100ms)` for readiness (11 instances) | `server.rs` | 92% |
| 3 | Structured output Layer 3/4 (retry+repair) ZERO tests | `structured_output.rs` | 90% |
| 4 | Agent guardrail `on_failure: retry` never exercised | `rig_agent_loop/tests.rs` | 88% |
| 5 | context_loader path traversal untested at integration level | `context_loader.rs` | 87% |
| 6 | TUI 86k LOC with 277 tests — event pipeline not state-tested | `nika-tui` | 85% |
| 7 | nika-cli has ZERO behavioral tests for command dispatch | `nika-cli/src/` | 82% |
| 8 | Cron scheduling: no missed-fire or concurrent-cap tests | `services/jobs.rs` | 83% |
| 9 | LSP e2e harness always `#[ignore]`, 9 bare `sleep()` calls | `e2e_harness.rs` | 81% |

## E.5 Connexions features (Feature Architect)

**Shared abstractions à créer AVANT Session M:**

```rust
// 1. IntrospectionContext (bundles state for 6 introspection tools)
pub struct IntrospectionContext {
    pub dag: Arc<Dag>,
    pub datastore: Arc<RunContext>,
    pub event_log: Arc<EventLog>,
}

// 2. RecordStore trait (engine in-memory, future: daemon-backed)
pub trait RecordStore: Send + Sync {
    fn set_record(&self, task_id: Arc<str>, record: Record);
    fn get_record(&self, task_id: &str) -> Option<Record>;
    fn iter_records(&self) -> Vec<(Arc<str>, Record)>;
}

// 3. PostCompletionStage trait (pipeline: scan → compress → artifact)
pub trait PostCompletionStage: Send + Sync {
    async fn process(&self, ctx: &PostCompletionCtx) -> PostCompletionResult;
}
// Stages: SecurityScanStage → RecordCompressionStage → ArtifactStage

// 4. resolve_compression_provider() in resolver.rs
// Fallback: summary preset → workflow default → cheapest model
```

**Ordre sécurité**: Output scan AVANT compression (protège le compressor LLM contre injection).

**Records vivent dans engine, PAS daemon** (daemon ne dépend que de nika-core).

## E.6 Bugs systémiques restants

| Catégorie | Count | Action |
|-----------|-------|--------|
| `_ => {}` sans logging | 60+ | Ajouter `tracing::warn!` ou `tracing::debug!` |
| `unwrap_or(0)` en production | 50+ | Remplacer par estimation ou error |
| EventKind variants non testés | 28 | Écrire 28 tests d'émission |
| `ContextAssembled` hardcoded zeros | 1 | Wire budget_used_pct réel quand P-CONTEXT |
| Chat path sans ProviderResponded | 1 | Ajouter émission dans chat.rs |
| `calculate_cost()` sans cache | 3 | structured_output.rs:206, thinking.rs:520, chat.rs:336 |
| `#[allow(dead_code)]` suspects | 42 | Auditer et supprimer ou justifier |
| `unreachable!()` atteignables | 5 | runner.rs:5612, template.rs:313, rig.rs:721/723/837/839 |
| README guardrail syntax wrong | 3 | Lines 399, 652, 660 |
| Dead mpsc channels | 2 | infer.rs:498, 850 |
| `unsafe { env::set_var }` in async | 2 | fallback.rs:55, 81 |

---

# SECTION F — ARCHITECTURE ET CONNEXIONS

## F.1 Graphe de dépendances features

```
[Session L.3: nika:cost tool] ─────────────────────────────────────────────┐
     │                                                                      │
     ▼                                                                      │
[Pre-work: Create shared abstractions] ◄──── IntrospectionContext          │
     │                                        RecordStore trait             │
     │                                        PostCompletionStage trait     │
     ▼                                                                      │
[Session M: P-RECORD] ─────────────────────────────────────────────────────┤
     │  Record struct + RecordSpec AST + Parser + Lower                    │
     │  RecordCompressor (uses summary preset from L)                      │
     │  PostCompletionPipeline: SecurityScan → RecordCompress → Artifact   │
     │  Record-aware bindings ($task → Record.summary)                     │
     │  Events: RecordCreated, RecordSkipped                               │
     │  nika:records tool                                                   │
     │                                                                      │
     ├──► [Session N Track A: P-CONTEXT]                                   │
     │      context_budget: AST, token counting, enforce_budget()          │
     │                                                                      │
     ├──► [Session N Track B: P-INTROSPECT] (parallélisable)              │
     │      IntrospectionContext → dag_info, task_status, threads, orchestrate│
     │                                                                      │
     └──► [Session N Track C: P-MEMORY-LOCAL]                              │
            RecordWriter NDJSON → SQLite FTS5 → nika trace search          │
                 │                                                          │
                 ▼                                                          │
     [Future: P-ORCHESTRATE (v0.53)]                                       │
            Orchestrator loop → DynamicDag → goal: field                    │
                 │                                                          │
                 ▼                                                          │
     [Future: MCP Server Mode]                                              │
            WorkflowRunner trait → Runner::new_for_embedding()             │
```

## F.2 Connexions entre composants

| Source | Target | Lien |
|--------|--------|------|
| Agent presets | Records | `default_record` field sur AgentDef: `search` → auto compress |
| Records | Bindings | `$task` retourne Record.summary, `$task.raw` → raw output |
| Records | Introspection | `nika:records` query les Records via RunContext |
| Records | Persistence | NDJSON via RecordWriter (engine-side, PAS daemon) |
| Daemon | FTS5 pattern | `storage.rs` DB thread + mpsc — réutiliser pour RecordIndex |
| Daemon | Cache | **Gap P0**: cache in-memory only, lost on restart |
| Fallback chains | ProviderResponded | **Gap**: failed attempts must emit ProviderResponded(cost:0) |
| Output scanner | Records | Scanner AVANT compression (protège compressor LLM) |
| Token budgets | Bindings | Records = semantic compression, budget = hard truncation (complémentaires) |
| MCP server | Engine | Embed Runner directly, NOT through daemon. 9 tools planned. |

## F.3 Shared abstractions à créer AVANT Session M (~2h pre-work)

| Abstraction | Fichier | Utilisé par |
|-------------|---------|-------------|
| `IntrospectionContext` | `runtime/introspect.rs` (NEW) | 6 introspection tools (dag_info, task_status, threads, orchestrate, cost, records) |
| `RecordStore` trait | `store/record_store.rs` (NEW) | RunContext (InMemoryRecordStore), future: DaemonRecordStore |
| `PostCompletionStage` trait | `runtime/pipeline.rs` (NEW) | SecurityScan → RecordCompress → Artifact (extensible pipeline) |
| `resolve_compression_provider()` | `runtime/resolver.rs` | summary preset → workflow default → cheapest model |
| `emit_provider_responded()` helper | `runtime/executor/verbs.rs` | Deduplicate 11 identical emit+cost blocks across infer+agent |

---

# SECTION G — MÉTHODOLOGIE

## G.1 Cycle TDD pour chaque changement

```
1. Lire le plan de session (Section B)
2. Lire le code existant (grep/read les fichiers mentionnés)
3. Écrire un test qui ÉCHOUE (red)
4. Implémenter le fix minimal (green)
5. Refactorer si nécessaire (refactor)
6. cargo test --workspace --lib     → 0 failures
7. cargo clippy --workspace -- -D warnings → 0 warnings
8. git add <specific files>
9. git commit (format Section G.2)
10. Répéter. Push après 2-3 commits.
```

## G.2 Format de commit

```
type(scope): description concise

Détails optionnels.

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

**Types**: `feat`, `fix`, `refactor`, `test`, `perf`, `docs`, `chore`
**Scopes**: `runtime`, `ast`, `parser`, `event`, `cli`, `builtin`, `tui`, `dag`, `binding`, `provider`, `mcp`, `security`, `lsp`, `daemon`

## G.3 Parallélisation avec agents

Pour les tâches mécaniques (is_ok(), enum migration, ProviderName remplacement):
- Dispatch **4-6 agents en parallèle** sur des fichiers différents
- Chaque agent: 2-3 fichiers max, vérifie compilation après
- Toi: merge et commit après vérification globale

## G.4 Ordre d'exécution recommandé

```
L.3 → E.3 → M → N → F.2 → I.2 → D.2 → J.2 → H.2
```

Logique:
1. L.3 établit le pattern builtin tool (réutilisé par M et N)
2. E.3 fixe les bugs silencieux AVANT d'ajouter des features
3. M crée Records (prérequis de N.3)
4. N utilise Records + ajoute introspection
5. F.2 est un refactoring pur (pas de feature dependency)
6. I.2/D.2/J.2/H.2 sont indépendants

---

# SECTION H — RÈGLES ABSOLUES

```
TESTS
  1. cargo test --workspace --lib    TOUJOURS (--lib = pas de keychain macOS)
  2. TDD: test FAIL → fix → test PASS → full suite → commit
  3. Si test casse → REVERT, passe au suivant

COMMITS
  4. 1 fix = 1 commit (jamais de batch non-relié)
  5. Co-authors TOUJOURS (voir G.2)
  6. git push après 2-3 commits
  7. cargo clippy --workspace -- -D warnings → ZERO warnings

QUALITÉ
  8. JAMAIS commiter du code qui ne compile pas
  9. JAMAIS .unwrap_or(0), _ => {}, .ok() sans logging
  10. Si bloqué 3x → skip, note dans progress.md, continue
  11. JAMAIS marquer un bug "done" sans code fix + test réel
```

---

# SECTION I — GESTION DU CONTEXT WINDOW

## Quand le context se remplit:

1. **Commit et push** tout le travail en cours
2. **Mettre à jour** `docs/plans/sessions/progress.md`:
   - Sessions complétées
   - Nombre de commits + tests
   - Ce qui est en cours
   - Prochain item à attaquer
3. **Donner l'instruction de relance**:

```bash
claude --dangerously-skip-permissions --model opus -p "$(cat docs/plans/sessions/mega-prompt-v8-handoff.md)"
```

---

# SECTION J — COMMENCER

```bash
# 1. Vérifier
git log --oneline -5
cd /Users/thibaut/dev/supernovae/nika/tools
cargo test --workspace --lib 2>&1 | tail -5
cargo clippy --workspace -- -D warnings 2>&1 | tail -3

# 2. Progress
cat ../docs/plans/sessions/progress.md

# 3. Plan de la prochaine session
cat ../docs/plans/sessions/session-L-agent-presets.md  # Tasks 4, 7-8

# 4. GO
```

Pas de questions. Pas d'hésitation. Lis, code, test, commit, push, continue.
