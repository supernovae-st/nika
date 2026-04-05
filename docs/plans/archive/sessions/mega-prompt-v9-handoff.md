Tu es l'orchestrateur autonome du projet **Nika** — un workflow engine YAML sémantique pour l'IA (5 verbs, 9 providers, 24+ builtin tools, 350k+ LOC Rust). Tu travailles sans intervention humaine. Commit, push, continue.

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
| `nika-engine` | 150k | Execution engine — embeddable runtime |
| `nika-core` | 27k | AST, types, catalogs — zero I/O |
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
| **P0** | `tools/nika/CLAUDE.md` | Dev reference: crate layout, source tree, error codes, testing, conventions |
| **P0** | `nika/CLAUDE.md` (racine) | User reference: 5 verbs syntax, data flow, pipe transforms, providers |
| **P0** | `docs/plans/sessions/progress.md` | État des sessions: commits, tests, ce qui est fait vs reste |

## B.2 Plans de sessions

| Fichier | Session | Status |
|---------|---------|--------|
| `session-A-security.md` | A: Security hardening | **DONE** |
| `session-B-agent-refactor.md` | B: Agent loop unification | **DONE** |
| `session-C-silent-failures.md` | C: TaskEventGuard + DAG | **DONE** |
| `session-D-quality-infra.md` | D: Proptest, serial, workspace deps | **PARTIAL** — cargo-mutants, tracing-error, cargo-deny |
| `session-E-test-hardening.md` | E: Test strengthening | **PARTIAL** — ~200 bare is_ok() in low-priority files |
| `session-F-stringly-typed.md` | F: Enums | **PARTIAL** — ProviderName enum + EventKind grouping |
| `session-G-split-rig.md` | G: Split rig.rs | **DONE** |
| `session-H-lsp-overhaul.md` | H: LSP | **PARTIAL** — extension activation, files.associations |
| `session-I-tui-polish.md` | I: TUI perf | **PARTIAL** — Arc\<Value\> done, DAG cache + Arc\<str\> |
| `session-J-phase0-stabilize.md` | J: Phase 0 stabilize | **PARTIAL** — registry fallback, LSP preset completions |
| `session-K-inference-routing.md` | K: Inference routing | **DONE** |
| `session-L-agent-presets.md` | L: Agent presets | **DONE** (preset.rs + nika:cost + integration) |
| `session-M-record-compression.md` | M: P-RECORD | **DONE** (foundation) — LLM compression deferred |
| `session-N-context-memory.md` | N: P-CONTEXT + P-MEMORY | **NOT STARTED** — 15 tasks |

## B.3 Plans stratégiques

| Fichier | Contenu |
|---------|---------|
| `2026-03-28-v1-master-plan.md` | Roadmap v1.0: Phase 0→1→2, critical path |
| `2026-03-28-v051-master-quality-plan.md` | **TOUTES** les bugs: 70+ items |
| `session-review-findings.md` | 350+ findings des audits |
| `2026-03-28-phase1-record.md` | Design P-RECORD |
| `2026-03-28-phase1-context-memory.md` | Design P-CONTEXT |
| `2026-03-28-phase1-orchestrate.md` | Design P-ORCHESTRATE |
| `2026-03-28-phase2-ecosystem.md` | Phase 2: registry, community, Telegram |

---

# SECTION C — ÉTAT ACTUEL (2026-03-30)

## C.1 Snapshot

```
Commits poussés : 84
Tests           : 8,785 (0 failures, 0 clippy warnings)
  nika-engine   : 4,132
  nika-tui      : 2,153
  nika-core     : 817
  nika-media    : 329
  nika-mcp      : 292
  nika-lsp-core : 230
  nika-daemon   : 164
  nika-event    : 140
  nika-init     : 140
  nika-cli      : 388
Builtin tools   : 26 nika:* (24 original + cost + records)
EventKind       : 68 variants (40 tested in wave2)
CLI commands    : 40
```

## C.2 Ce qui est fait (ne PAS refaire)

| Session | Commits | Résumé |
|---------|---------|--------|
| A: Security | 10 | python/bash/zsh -c blocked, SSRF fail-closed, sk-* redaction |
| B: Agent Refactor | 5 | run_agent_loop unifié, token_budget → LimitTracker |
| C: Silent Failures | 4 | TaskEventGuard, 17 DAG failures, for_each events |
| D: Quality Infra | 4 | 27 proptest, 24 #[serial], 57 workspace deps |
| E: Test Hardening | 9 | CR2+CR3 tautologies, 240+ is_ok() → descriptive |
| F Part 1: Enums | 5 | ExtractMode, ResponseMode, GuardrailType, Severity, FinishReason |
| G: Split rig.rs | 5 | 3675 LOC → 5 fichiers |
| K: Routing | 4 | provider:[a,b], ProviderFallback, NIKA-037 |
| L: Presets | 7 | 8 built-in, parser disambiguation, preset.rs, nika:cost |
| I Part 1 | 1 | Arc\<Value\> for EventKind fields |
| M: P-RECORD | 11 | Record struct, RecordSpec, events, compressor, records tool, bindings, error codes, quality refactoring |
| E.3 Quality | 4 | SF7, SF2, CR1 jsonschema, M-orig8 temperature |
| Release | 3 | v0.51.0 tag |
| CI/docs | ~8 | Various CI fixes, progress docs |

## C.3 Architecture P-RECORD (nouveau)

```
RecordSpec (nika-core)     → AST node, parsed from record: field
    ↓
Record (nika-engine)       → Compressed output with summary, key_findings, confidence
    ↓
RecordCompressor           → CompressorLlm trait, fallback to truncation
    ↓
Runner wiring              → After task success, if record: { compress: true }
    ↓
RunContext.records          → Arc<Record> in DashMap (zero-copy reads)
    ↓
Binding resolution         → $task → Record.summary, $task.raw → raw output
    ↓
nika:records tool           → Introspection tool for agents
    ↓
NIKA-320-324               → 5 error codes, all recoverable
```

**LLM compression NOT YET WIRED** — the `CompressorLlm` trait exists with tests but the runner uses truncation-only. Wiring needs: provider resolution from agents: block → executor infer call wrapped in CompressorLlm impl.

---

# SECTION D — TRAVAIL RESTANT (par ordre de priorité)

## D.1 Session N — Context + Memory / P-CONTEXT (~3h, 15 tasks) ⭐ NEXT

**Plan**: `session-N-context-memory.md` + `2026-03-28-phase1-context-memory.md`

```
Token counting → context_budget: AST → Budget enforcement → Introspection tools (4)
→ NDJSON persistence → SQLite FTS5 → nika trace search → File locking → Output scanner
```

**Dépendances**: Records (M) existent maintenant.

**Architecture clé**:
- Token counting: heuristique char-based, CJK-aware (CHARS_PER_TOKEN constant exists)
- Budget enforcement: truncation proportionnelle, minimum 50 tokens/binding
- 4 introspection tools: `nika:dag_info`, `nika:task_status`, `nika:threads`, `nika:orchestrate` (stub)
- Persistence: `.nika/records/{workflow}_{timestamp}.ndjson`
- Index: SQLite FTS5 via rusqlite + WAL (même pattern que daemon storage)
- `nika trace search <query>`: CLI full-text search cross-session

## D.2 Session M.remaining — LLM Compression + Wiring Gaps (~2h)

**Gaps trouvés par audit de vérification (2026-03-30):**

1. **CRITICAL: `nika:records` tool NOT wired in executor** — `with_records_tool()` exists on `BuiltinToolRouter` but is NEVER called in `executor/mod.rs:180-183`. The tool is unreachable at runtime.
   - Fix: Add `.with_records_tool(Arc::clone(&datastore))` after `.with_cost_tool(event_log.clone())` in executor construction
2. **`retain` field parsed but never consumed** — `RecordSpec.retain` is stored in AST but `RecordCompressor.compress()` ignores it. Should filter key_findings or raw output.
3. `resolve_compression_provider()` — Look up "summary" agent in resolved_assets, fallback to cheapest model
4. `ExecutorCompressorLlm` struct — Wraps `TaskExecutor::execute()` into `CompressorLlm` trait
5. Wire into runner: replace truncation-only with `compressor.compress()` call
6. E2E test: workflow with `record: { compress: true }` + mock provider → verify Record has LLM-generated summary

## D.3 Session F.2 — ProviderName enum + EventKind grouping (~3h)

**Plan**: `session-F-stringly-typed.md` Parts 2, 4, 7

```
COMMIT 1-3: ProviderName enum
  - Variants: Anthropic, OpenAI, Mistral, Groq, DeepSeek, Gemini, XAi, Native, Mock, Custom(String)
  - Manual Serde avec alias support (claude → Anthropic, gpt → OpenAI)
  - Migration: Option<String> → Option<ProviderName> dans AST + Event + Engine
  - Impact: ~916 occurrences across ~116 files (utiliser agents parallèles!)

COMMIT 4-5: EventKind grouping
  - 68 flat variants → nested: TaskEvent, ProviderEvent, McpEvent, AgentEvent, MediaEvent
  - Custom Serialize/Deserialize pour backward compat traces NDJSON
  - Update all consumers: renderer.rs, live.rs, event_handler/, tests
```

## D.4 Session I.2 — TUI Performance (~1h)

**Plan**: `session-I-tui-polish.md` Bugs 2-4

| Task | LOC |
|------|-----|
| DAG layout cache: invalidate on task status change only | ~80 |
| Arc\<str\> for task_id in TUI state (~10 .to_string() → Arc::clone) | ~40 |

## D.5 Session D.2 — Quality Infrastructure (~2h)

**Plan**: `session-D-quality-infra.md`

| Task | Outil |
|------|-------|
| Mutation testing 5 critical files | `cargo-mutants` |
| SpanTrace error context | `tracing-error` |
| License check (AGPL), CVE scan, source validation | `cargo-deny` |
| 3 E2E stress workflows (concurrency, extraction, transforms) | Mock provider |

## D.6 Session J.2 — Registry + Docs (~1h)

| Task | Détail |
|------|--------|
| Registry graceful fallback | Friendly error when unreachable |
| LSP preset completions | Completions from agents block |

## D.7 Session H.2 — LSP Remaining (~1.5h)

| Task | Fichier |
|------|---------|
| Move registerCommand() to TOP of activate() | `editors/vscode/src/extension.ts` |
| Add files.associations to configurationDefaults | `editors/vscode/package.json` |
| Import path validation | `nika-lsp/src/diagnostics.rs` |

## D.8 Quality Plan Bugs STILL OPEN (verified 2026-03-30)

### HIGH — Silent Failures (confirmed NOT fixed)

| Bug | Fichier | Fix |
|-----|---------|-----|
| **SF3** | `runner.rs:1800-1809` | for_each binding failure → emit TaskFailed event |
| **SF4** | `runner.rs:2246-2261` | "items could not be resolved" → emit TaskFailed event |
| **SF5** | `runner.rs:656` | `jsonschema::validator_for().ok()` → return error if invalid |
| **SF6** | `nika-event/src/log.rs:1042` | Replace `let _ =` with `warn!` on trace write failure |

### MEDIUM — Features (confirmed NOT fixed)

| Bug | Fix |
|-----|-----|
| **M-orig3** | `manifest: true` never writes artifacts.json → implement `write_manifest()` |
| **M-orig6** | `{{for_each.index}}` unavailable in artifact paths → inject variable |
| **M-tok1** | Token counts = 0 when Final stream missing → add fallback estimation |

### Systemic Issues

| Catégorie | Count | Action |
|-----------|-------|--------|
| `_ => {}` sans logging | 60+ | Ajouter `tracing::warn!` ou `tracing::debug!` |
| `unwrap_or(0)` en production | 50+ | Remplacer par estimation ou error |
| EventKind variants non testés | ~28 | Écrire tests d'émission |
| `#[allow(dead_code)]` suspects | 42 | Auditer et supprimer ou justifier |
| `unreachable!()` atteignables | 5 | runner.rs:5612, template.rs:313, rig.rs |
| Dead mpsc channels | 2 | infer.rs:498, 850 |
| `unsafe { env::set_var }` in async | 2 | fallback.rs:55, 81 |

## D.9 Sessions Futures (v0.53+)

| Session | Quoi | Plan |
|---------|------|------|
| O | P-ORCHESTRATE: goal:, DynamicDag | `2026-03-28-phase1-orchestrate.md` |
| P | Scaleway GPU deployment | — |
| Q | Telegram Bot trigger | `2026-03-28-phase2-ecosystem.md` |
| R | CI Pipeline + Release | `.github/workflows/release.yml` |
| S | Self-Improvement / Hermes | `2026-03-28-v1-master-plan.md` Phase 1.5 |
| T | MCP Server Mode | `nika serve --mcp` |
| U | Registry + Packages | `2026-03-28-phase2-ecosystem.md` |

---

# SECTION E — ARCHITECTURE AUDIT (résultats session précédente)

## E.1 Top 5 refactorings architecturaux

| # | Refactoring | Impact | Effort |
|---|------------|--------|--------|
| 1 | Complete error domain migration (96 flat → 12 sub-enums) | HIGH | 3-4 days |
| 2 | Extract `nika-runtime` trait crate (WorkflowRunner, Provider, SecretResolver) | HIGH | 2-3 days |
| 3 | EventKind grouping (68 flat → ~10 nested enums) | MEDIUM | 2-3 days |
| 4 | Split runner.rs (6700+ LOC) → scheduler, for_each, orchestration | MEDIUM | 1-2 days |
| 5 | Thread typed enums — ProviderKind replace String everywhere | MEDIUM | 1-2 days |

## E.2 Infrastructure gaps (Daemon)

| Gap | Priority |
|-----|----------|
| Cache persistence (in-memory only, lost on restart) | P0 |
| Record persistence (need SQLite + FTS5 for P-RECORD) | P1 |
| CAS garbage collection (blobs accumulate forever) | P1 |
| MCP server architecture (embed engine, not through daemon) | P2 |
| Hot config reload (SIGHUP commented as "future") | P3 |

## E.3 Test gaps critiques

| # | Gap | Confidence |
|---|-----|------------|
| 1 | Provider fallback NIKA-037 has NO offline test | 95% |
| 2 | Structured output Layer 3/4 (retry+repair) ZERO tests | 90% |
| 3 | Agent guardrail `on_failure: retry` never exercised | 88% |
| 4 | nika-cli has ZERO behavioral tests for command dispatch | 82% |

---

# SECTION F — CONNEXIONS FEATURES

## F.1 Graphe de dépendances

```
[DONE: L.3 nika:cost]
[DONE: M P-RECORD foundation]
     │
     ├──► [D.2: M.remaining — LLM compression wiring]
     │
     ├──► [Session N Track A: P-CONTEXT]
     │      context_budget: AST, token counting, enforce_budget()
     │
     ├──► [Session N Track B: P-INTROSPECT]
     │      IntrospectionContext → dag_info, task_status, threads, orchestrate
     │
     └──► [Session N Track C: P-MEMORY-LOCAL]
            RecordWriter NDJSON → SQLite FTS5 → nika trace search
                 │
                 ▼
     [Future: P-ORCHESTRATE (v0.53)]
            Orchestrator loop → DynamicDag → goal: field
```

## F.2 Shared abstractions à créer AVANT Session N

```rust
// IntrospectionContext (bundles state for introspection tools)
pub struct IntrospectionContext {
    pub dag: Arc<Dag>,
    pub datastore: Arc<RunContext>,  // needs Arc refactor
    pub event_log: EventLog,
}
```

**Records vivent dans engine, PAS daemon** (daemon ne dépend que de nika-core).

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
N (en cours) → M.remaining (nika:records wiring + LLM compression) → E.remaining (SF3-6, M-orig3/6) → F.2 → I.2 → D.2 → J.2 → H.2
```

Logique:
1. **N** est EN COURS par l'autre agent (Session N P-CONTEXT)
2. **M.remaining** CRITICAL: wire nika:records dans executor + CompressorLlm (2h)
3. **E.remaining** fix SF3-SF6 (silent failures) + M-orig3/6 (manifest, for_each.index)
4. **F.2** est un refactoring pur (ProviderName enum, pas de feature dependency)
5. **I.2/D.2/J.2/H.2** sont indépendants

## G.5 Qualité Rust (leçons de l'audit)

- **JAMAIS** `&str[..n]` sans vérifier char boundary (utiliser `is_char_boundary` ou `char_indices`)
- **TOUJOURS** `Arc<T>` dans DashMap quand T est cloné souvent (Record, Value)
- **TOUJOURS** délimiter les données utilisateur dans les prompts LLM
- **MSRV = 1.86.0** — pas de `floor_char_boundary` (1.91), pas de `let-else` pré-stabilisation
- **CHARS_PER_TOKEN = 4** constant dans `record.rs` — réutiliser partout

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
2. **Mettre à jour** `docs/plans/sessions/progress.md`
3. **Donner l'instruction de relance**:

```bash
claude --dangerously-skip-permissions --model opus -p "$(cat docs/plans/sessions/mega-prompt-v9-handoff.md)"
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
cat ../docs/plans/sessions/session-N-context-memory.md

# 4. GO — Session N (P-CONTEXT)
```

Pas de questions. Pas d'hésitation. Lis, code, test, commit, push, continue.
