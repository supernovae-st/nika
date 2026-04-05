Tu es l'orchestrateur autonome du projet **Nika** — un workflow engine YAML sémantique pour l'IA (5 verbs, 9 providers, 30+ builtin tools, 353k+ LOC Rust). Tu travailles sans intervention humaine. Commit, push, continue.

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
cd /Users/thibaut/dev/supernovae/nika/tools
git log --oneline -5
cargo test --workspace --lib 2>&1 | tail -5
cargo clippy --workspace -- -D warnings 2>&1 | tail -3
cat ../docs/plans/sessions/progress.md
```

## A.3 Workspace — 12 crates

| Crate | LOC | Tests | Rôle |
|-------|-----|-------|------|
| `nika` | 4.6k | 0 | Binary CLI — entry point |
| `nika-engine` | 149k | 4,173 | Execution engine — embeddable runtime |
| `nika-core` | 27k | 833 | AST, types, catalogs — zero I/O |
| `nika-event` | 5.2k | 141 | EventLog, TraceWriter |
| `nika-tui` | 88k | 2,153 | Terminal UI (ratatui) |
| `nika-cli` | 12.5k | 388 | CLI subcommands |
| `nika-mcp` | 9.5k | 292 | MCP client (rmcp) |
| `nika-media` | 13.5k | 329 | CAS store, media processor |
| `nika-daemon` | 6.8k | 164 | Background daemon (secrets, jobs, cache, watch) |
| `nika-init` | 21k | 142 | Scaffolding, course (12 levels, 44 exercises, 115 showcases) |
| `nika-lsp` | 3.5k | 0 | LSP binary |
| `nika-lsp-core` | 12k | 230 | LSP handlers (14 features) |
| **TOTAL** | **353k** | **8,845** | |

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
| `session-D-quality-infra.md` | D: Proptest, serial, workspace deps | **PARTIAL** — cargo-mutants, tracing-error, cargo-deny remain |
| `session-E-test-hardening.md` | E: Test strengthening | **PARTIAL** — ~200 bare is_ok() in low-priority files |
| `session-F-stringly-typed.md` | F: Enums | **PARTIAL** — ProviderName enum created, EventKind grouping + full migration pending |
| `session-G-split-rig.md` | G: Split rig.rs | **DONE** |
| `session-H-lsp-overhaul.md` | H: LSP | **PARTIAL** — extension activation, files.associations |
| `session-I-tui-polish.md` | I: TUI perf | **PARTIAL** — Arc\<Value\> done, DAG cache + Arc\<str\> remain |
| `session-J-phase0-stabilize.md` | J: Phase 0 stabilize | **PARTIAL** — registry fallback, LSP preset completions |
| `session-K-inference-routing.md` | K: Inference routing | **DONE** |
| `session-L-agent-presets.md` | L: Agent presets | **DONE** |
| `session-M-record-compression.md` | M: P-RECORD | **DONE** (LLM compression wired) |
| `session-N-context-memory.md` | N: P-CONTEXT + P-MEMORY | **DONE** (FTS5 deferred, frozen guard deferred) |

## B.3 Plans stratégiques (v1.0 master plan)

| Fichier | Contenu | Status |
|---------|---------|--------|
| `2026-03-28-v1-master-plan.md` | Roadmap v1.0: Phase 0→1→2, critical path | **CURRENT** |
| `2026-03-28-v051-master-quality-plan.md` | 70+ bugs from quality audit | **PARTIALLY ADDRESSED** |
| `session-review-findings.md` | 350+ findings des audits | **REFERENCE** |
| `2026-03-28-phase1-record.md` | Design P-RECORD | **DONE** |
| `2026-03-28-phase1-context-memory.md` | Design P-CONTEXT | **DONE** |
| `2026-03-28-phase1-orchestrate.md` | Design P-ORCHESTRATE | **NOT STARTED** |
| `2026-03-28-phase2-ecosystem.md` | Phase 2: registry, community, Telegram | **NOT STARTED** |

---

# SECTION C — ÉTAT ACTUEL (2026-03-29)

## C.1 Snapshot

```
Commits poussés : 95
Tests           : 8,845 (0 failures, 0 clippy warnings)
  nika-engine   : 4,173
  nika-tui      : 2,153
  nika-core     : 833
  nika-cli      : 388
  nika-media    : 329
  nika-mcp      : 292
  nika-lsp-core : 230
  nika-daemon   : 164
  nika-init     : 142
  nika-event    : 141
Builtin tools   : 30 nika:* (24 media + cost + records + dag_info + task_status + threads + orchestrate)
EventKind       : ~70 variants
CLI commands    : 40+
LOC total       : 353,250
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
| F Part 2: ProviderName | 1 | ProviderName enum in nika-core (10 tests) |
| G: Split rig.rs | 5 | 3675 LOC → 5 fichiers |
| K: Routing | 4 | provider:[a,b], ProviderFallback, NIKA-037 |
| L: Presets | 7 | 8 built-in, parser disambiguation, preset.rs, nika:cost |
| I Part 1 | 1 | Arc\<Value\> for EventKind fields |
| M: P-RECORD | 12 | Record struct, compressor, events, bindings, errors, LLM wiring |
| N: P-CONTEXT | 10 | context_budget, 4 introspection tools, NDJSON persistence, output scanner |
| E.3 Quality | 4 | SF7, SF2, CR1 jsonschema, M-orig8 temperature |
| Release | 3 | v0.51.0 tag |
| CI/docs | ~10 | Various CI fixes, progress docs |

## C.3 Architecture highlights (nouveautés Session N)

```
context_budget: u32 (AST)     → Token counting (CJK-aware)
    ↓
enforce_budget()               → Proportional truncation
    ↓
BudgetOk / BudgetExceeded     → Events + Display (live, renderer, TUI)
```

```
4 Introspection Tools:
  nika:dag_info      → task count + completion status from EventLog
  nika:task_status   → per-task metrics (tokens, cost, records)
  nika:threads       → all tasks with status/verb, filterable
  nika:orchestrate   → aggregate stats (stub for P-ORCHESTRATE)
```

```
RecordWriter (NDJSON)          → .nika/records/{name}_{timestamp}.ndjson
nika trace search <query>      → Cross-session search (--workflow, --since)
```

```
ExecutorCompressorLlm          → Bridges TaskExecutor to CompressorLlm trait
    ↓
resolve_cheap_model()          → haiku for Anthropic, gpt-4.1-mini for OpenAI
    ↓
RecordCompressor.compress()    → Real LLM calls, truncation fallback
```

---

# SECTION D — TRAVAIL RESTANT (par ordre de priorité)

## D.1 Session F.2 remaining — ProviderName migration (~4h) ⭐ NEXT

**Plan**: `session-F-stringly-typed.md` Parts 2, 7

```
COMMIT 1-3: ProviderName migration
  - Enum exists in nika-core (ProviderName with 9 variants + Custom)
  - Migration: Option<String> → Option<ProviderName> in:
    - AnalyzedTask.provider, AnalyzedWorkflow.provider
    - InferParams.provider, AgentParams.provider
    - executor default_provider, config.provider
  - ~20 struct fields + hundreds of usages
  - Use parallel agents on different files!

COMMIT 4-5: EventKind grouping (OPTIONAL — big effort)
  - 70 flat variants → nested: TaskEvent, ProviderEvent, McpEvent, AgentEvent, MediaEvent
  - Custom Serialize/Deserialize for backward compat traces NDJSON
  - Update all consumers: renderer.rs, live.rs, event_handler/, tests
```

## D.2 Session I.2 — TUI Performance (~1h)

| Task | LOC |
|------|-----|
| DAG layout cache: invalidate on task status change only | ~80 |
| Arc\<str\> for task_id in TUI state (~10 .to_string() → Arc::clone) | ~40 |

## D.3 Session D.2 — Quality Infrastructure (~2h)

| Task | Outil |
|------|-------|
| Mutation testing 5 critical files | `cargo-mutants` |
| SpanTrace error context | `tracing-error` |
| License check (AGPL), CVE scan, source validation | `cargo-deny` |
| 3 E2E stress workflows (concurrency, extraction, transforms) | Mock provider |

## D.4 Session J.2 — Registry + Docs (~1h)

| Task | Détail |
|------|--------|
| Registry graceful fallback | Friendly error when unreachable |
| LSP preset completions | Completions from agents block |

## D.5 Session H.2 — LSP Remaining (~1.5h)

| Task | Fichier |
|------|---------|
| Move registerCommand() to TOP of activate() | `editors/vscode/src/extension.ts` |
| Add files.associations to configurationDefaults | `editors/vscode/package.json` |
| Import path validation | `nika-lsp/src/diagnostics.rs` |

## D.6 Code Quality Sweep (NEW — no session assigned, ~6h)

| Anti-Pattern | Count | Fix |
|-------------|-------|-----|
| `_ => {}` without logging | 74 | `tracing::warn!("unhandled variant")` |
| `unwrap_or(0)` in production | 125 | Explicit logging or estimation |
| `#[allow(dead_code)]` | 385 | Audit: remove dead code or justify |
| `unreachable!()` reachable | 3 | Proper error handling |
| Untested EventKind variants | ~25 | Write emission tests |

## D.7 Sessions Futures (v0.53+)

| Session | Quoi | Plan | Effort |
|---------|------|------|--------|
| O | P-ORCHESTRATE: goal:, DynamicDag | `2026-03-28-phase1-orchestrate.md` | 4 weeks |
| P | Scaleway GPU deployment | — | 1 week |
| Q | Telegram Bot trigger | `2026-03-28-phase2-ecosystem.md` | 1 week |
| R | CI Pipeline + Release | `.github/workflows/release.yml` | 2 days |
| S | Self-Improvement / Hermes | `2026-03-28-v1-master-plan.md` Phase 1.5 | 2 weeks |
| T | MCP Server Mode | `nika serve --mcp` | 1 week |
| U | Registry + Packages | `2026-03-28-phase2-ecosystem.md` | 2 weeks |

---

# SECTION E — V1.0 CRITICAL PATH

```
Phase 0: Stabilize (v0.50)           ✅ DONE
Phase 1: Intelligence (v0.51-0.55)
  ├─ P-MODEL (v0.51)                 ✅ DONE (presets, routing, nika:cost)
  ├─ P-RECORD (v0.52)                ✅ DONE (struct, compressor, events, bindings, LLM wiring)
  ├─ P-CONTEXT (v0.54)               ✅ DONE (context_budget, token counting, enforcement)
  ├─ P-INTROSPECT (v0.54)            ✅ DONE (4 tools: dag_info, task_status, threads, orchestrate)
  ├─ P-MEMORY-LOCAL (v0.55)          ✅ DONE (NDJSON persistence, CLI search, output scanner)
  ├─ P-ORCHESTRATE (v0.53)           ❌ NOT STARTED (goal:, DynamicDag, Orchestrator loop)
  └─ Quality polish                   🔶 PARTIAL (F.2, I.2, D.2, code sweep)

Phase 2: Ecosystem (v0.56-0.60)       ❌ NOT STARTED
  ├─ 2.1: Registry & publishing
  ├─ 2.2: Community & content
  └─ 2.3: Integration & distribution
```

**Next critical path item**: P-ORCHESTRATE or quality polish (your choice).

---

# SECTION F — MÉTHODOLOGIE

## F.1 Cycle TDD pour chaque changement

```
1. Lire le plan de session (Section B)
2. Lire le code existant (grep/read les fichiers mentionnés)
3. Écrire un test qui ÉCHOUE (red)
4. Implémenter le fix minimal (green)
5. Refactorer si nécessaire (refactor)
6. cargo test --workspace --lib     → 0 failures
7. cargo clippy --workspace -- -D warnings → 0 warnings
8. git add <specific files>
9. git commit (format Section F.2)
10. Répéter. Push après 2-3 commits.
```

## F.2 Format de commit

```
type(scope): description concise

Détails optionnels.

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

**Types**: `feat`, `fix`, `refactor`, `test`, `perf`, `docs`, `chore`
**Scopes**: `runtime`, `ast`, `parser`, `event`, `cli`, `builtin`, `tui`, `dag`, `binding`, `provider`, `mcp`, `security`, `lsp`, `daemon`, `core`

## F.3 Parallélisation avec agents

Pour les tâches mécaniques (is_ok(), enum migration, ProviderName remplacement):
- Dispatch **4-6 agents en parallèle** sur des fichiers différents
- Chaque agent: 2-3 fichiers max, vérifie compilation après
- Toi: merge et commit après vérification globale

## F.4 Qualité Rust

- **JAMAIS** `&str[..n]` sans vérifier char boundary
- **TOUJOURS** `Arc<T>` dans DashMap quand T est cloné souvent
- **TOUJOURS** délimiter les données utilisateur dans les prompts LLM
- **MSRV = 1.86.0**
- **CHARS_PER_TOKEN = 4** constant dans `record.rs`

---

# SECTION G — RÈGLES ABSOLUES

```
TESTS
  1. cargo test --workspace --lib    TOUJOURS (--lib = pas de keychain macOS)
  2. TDD: test FAIL → fix → test PASS → full suite → commit
  3. Si test casse → REVERT, passe au suivant

COMMITS
  4. 1 fix = 1 commit (jamais de batch non-relié)
  5. Co-authors TOUJOURS (voir F.2)
  6. git push après 2-3 commits
  7. cargo clippy --workspace -- -D warnings → ZERO warnings

QUALITÉ
  8. JAMAIS commiter du code qui ne compile pas
  9. JAMAIS .unwrap_or(0), _ => {}, .ok() sans logging
  10. Si bloqué 3x → skip, note dans progress.md, continue
  11. JAMAIS marquer un bug "done" sans code fix + test réel
```

---

# SECTION H — GESTION DU CONTEXT WINDOW

## Quand le context se remplit:

1. **Commit et push** tout le travail en cours
2. **Mettre à jour** `docs/plans/sessions/progress.md`
3. **Donner l'instruction de relance**:

```bash
claude --dangerously-skip-permissions --model opus -p "$(cat docs/plans/sessions/mega-prompt-v11-handoff.md)"
```

---

# SECTION I — ORDRE D'EXÉCUTION RECOMMANDÉ

```
F.2 (ProviderName migration) → I.2 (TUI perf) → D.2 (quality infra) → J.2 (registry) → H.2 (LSP) → Code Sweep → O (P-ORCHESTRATE)
```

Logique:
1. **F.2** complète le refactoring fondamental (typed providers partout)
2. **I.2/D.2/J.2/H.2** sont indépendants et courts
3. **Code Sweep** est mécanique (agents parallèles)
4. **O** est la prochaine feature (P-ORCHESTRATE)

---

# SECTION J — COMMENCER

```bash
# 1. Vérifier
cd /Users/thibaut/dev/supernovae/nika/tools
git log --oneline -5
cargo test --workspace --lib 2>&1 | tail -5
cargo clippy --workspace -- -D warnings 2>&1 | tail -3

# 2. Progress
cat ../docs/plans/sessions/progress.md

# 3. GO — Choisir la prochaine session dans Section D
```

Pas de questions. Pas d'hésitation. Lis, code, test, commit, push, continue.
