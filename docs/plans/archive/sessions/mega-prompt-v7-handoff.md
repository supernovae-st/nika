Tu es l'orchestrateur autonome du projet **Nika** — un workflow engine YAML pour l'IA. Tu vas travailler sans intervention humaine. Thibaut n'est pas là. Commit, push, et continue jusqu'à ce que tout soit fait ou que ton context window se remplisse.

---

## 1. IDENTITÉ + ACCÈS

- **Mode**: `--dangerously-skip-permissions` — accès complet filesystem, git, terminal
- **Répertoire**: `/Users/thibaut/dev/supernovae/nika/`
- **Code Rust**: `tools/` (12 crates, workspace Cargo)
- **Binaire**: `nika`
- **Version**: v0.51.0
- **Branche**: `main`
- **Remote**: `github.com:supernovae-st/nika.git`

---

## 2. ÉTAT AU MOMENT DU HANDOFF

```
Commits poussés : 64 (sur cette initiative qualité)
Tests           : 8,719 (0 failures)
Clippy          : 0 warnings
TODOs engine    : 1 seul (scope: not implemented)
```

### PREMIÈRE CHOSE À FAIRE — VÉRIFIER L'ÉTAT

```bash
git log --oneline -5
cd /Users/thibaut/dev/supernovae/nika/tools
cargo test --workspace --lib 2>&1 | tail -5
cargo clippy --workspace -- -D warnings 2>&1 | tail -3
cat ../docs/plans/sessions/progress.md
```

Si les tests ne passent pas → lire les erreurs, fixer, puis continuer.

---

## 3. ARCHITECTURE DU PROJET

```
tools/
├── nika/           Binary CLI (4.6k LOC) — entry point, clap commands
├── nika-engine/    Execution engine (145k LOC) — embeddable runtime
│   ├── src/ast/            Three-phase: Raw → Analyzed → Lower
│   ├── src/runtime/        Runner, TaskExecutor, RigAgentLoop, builtins
│   ├── src/provider/       LLM providers (rig-core + cost.rs)
│   ├── src/binding/        Templates, transforms, JSONPath, resolve
│   ├── src/dag/            DAG validation + cycle detection
│   ├── src/display/        CLI renderers, bench display, format_event
│   ├── src/media/          CAS store, media processor
│   └── src/mcp/            MCP client integration
├── nika-core/      AST, types, catalogs (26k LOC) — zero I/O
│   ├── src/ast/raw/        Phase 1: YAML → Raw AST (parser.rs 3600 LOC)
│   ├── src/ast/analyzed/   Phase 2: Validated AST
│   ├── src/ast/analyzer/   Phase 2: Validation transforms
│   └── src/catalogs/       Providers, models, MCP aliases
├── nika-event/     EventLog, TraceWriter (5k LOC)
├── nika-tui/       Terminal UI ratatui (88k LOC)
├── nika-cli/       CLI subcommands (12k LOC)
├── nika-mcp/       MCP client rmcp (9.5k LOC)
├── nika-media/     CAS store + processor (13k LOC)
├── nika-daemon/    Background daemon (7k LOC)
├── nika-init/      Scaffolding + course (21k LOC)
├── nika-lsp/       LSP binary (3.5k LOC)
└── nika-lsp-core/  LSP handlers (12k LOC)
```

**5 verbs**: `infer:`, `exec:`, `fetch:`, `invoke:`, `agent:`
**9 providers**: anthropic, openai, mistral, groq, deepseek, gemini, xai, native, mock
**Schema**: `nika/workflow@0.12`

---

## 4. DOCUMENTATION DE RÉFÉRENCE — LIRE AVANT DE CODER

| Fichier | Quoi | Quand le lire |
|---------|------|---------------|
| `tools/nika/CLAUDE.md` | Dev reference complet (crate layout, error codes, conventions, testing) | **TOUJOURS en premier** |
| `nika/CLAUDE.md` (racine) | Syntax des 5 verbs, data flow, pipe transforms, providers, common mistakes | Pour écrire du YAML ou toucher le parser |
| `docs/plans/sessions/progress.md` | État des sessions, commits, ce qui est fait | **Avant chaque session** |
| `docs/plans/2026-03-28-v051-master-quality-plan.md` | **TOUTES** les bugs identifiées (CR1-CR3, S1-S6, SF1-SF10, M-*, L-*) | Pour la session E.3 (bugs) |
| `docs/plans/2026-03-28-v1-master-plan.md` | Roadmap v1.0: Phase 0→1→2, P-MODEL/RECORD/ORCHESTRATE/CONTEXT | Pour comprendre la vision |
| `docs/plans/sessions/session-review-findings.md` | 350+ findings détaillés des audits (bugs, systemic issues, gaps) | Pour les sessions de bugs |
| `docs/plans/sessions/session-{X}-*.md` | Plan détaillé de chaque session (14 sessions A→N) | **Avant de commencer une session** |
| `docs/plans/2026-03-28-phase1-record.md` | Design détaillé P-RECORD (Record struct, compressor, bindings) | Pour Session M |
| `docs/plans/2026-03-28-phase1-context-memory.md` | Design détaillé P-CONTEXT (token budgets, introspection, NDJSON) | Pour Session N |

---

## 5. CE QUI EST FAIT — NE PAS REFAIRE

### Sessions complètes

| Session | Commits | Résumé |
|---------|---------|--------|
| **A: Security** | 10 | 11 vulns fixées: python/bash/zsh -c blocked, SSRF fail-closed, sk-* redaction, xargs/find -exec, stream limits |
| **B: Agent Refactor** | 5 | providers.rs -771 LOC, run_agent_loop unifié, token_budget wired into LimitTracker |
| **C: Silent Failures** | 4 | TaskEventGuard pattern, 17 DAG failures fixées |
| **D: Quality Infra** | 4 | 27 proptest, 24 #[serial], 57 workspace deps, pricing 55 models |
| **E: Test Hardening** | 5 | Tautological tests CR2+CR3 remplacés, 240+ bare is_ok() → descriptive assertions |
| **F Part 1: Enums** | 5 | ExtractMode(9), ResponseMode(2), GuardrailType(4), Severity(4), AgentTurnKind(4), FinishReason(8+Other), AgentStopReason(7+Other) |
| **G: Split rig.rs** | 5 | 3675 LOC → 5 fichiers |
| **J Part 1** | 1 | Error code table fix |
| **K: Inference Routing** | 4 | `provider: [a,b]` parsing, ProviderFallback event, NIKA-037, fallback infer+agent |
| **L: Agent Presets** | 4 | 8 built-in presets, `agent: think` (scalar) → preset, PresetApplied event, `nika agent --list` |
| **I Part 1: TUI Perf** | 1 | Arc\<Value\> for TaskStarted.inputs, McpInvoke.params, McpResponse.response |
| **Release** | 3 | v0.51.0 tag |

### Features déjà implémentées (ne pas recréer)

- **nika bench**: CLI complet + runner + display 1200 LOC + cache
- **8 agent presets**: think, lite, search, vision, judge, coder, summary, creative
- **Fallback chains**: parser + event + executor (infer + agent)
- **NIKA-163**: Workflow-level unknown key detection with did-you-mean
- **Template validation**: No crash on malformed YAML
- **VS Code extension**: Synced to v0.51.0, 18 E2E tests

---

## 6. SESSIONS RESTANTES — PAR ORDRE DE PRIORITÉ

### PRIORITÉ 1: Session L.3 — nika:cost + preset.rs (~1.5h)

**Plan**: `docs/plans/sessions/session-L-agent-presets.md` (Tasks 4, 7-8)

| # | Tâche | Fichiers | LOC |
|---|-------|----------|-----|
| 1 | Extract `preset.rs` module | `nika-engine/src/runtime/preset.rs` (NEW) | ~160 + 120 tests |
| | Actuellement inline dans runner.rs:945-1003 | Extraire `apply_preset_to_action()` | |
| | Tests: preset applies provider/model/temp/system to infer/agent, ignores exec/fetch/invoke | | |
| 2 | `nika:cost` introspection tool | `nika-engine/src/runtime/builtin/cost.rs` (NEW) | ~80 + 60 tests |
| | Itère EventLog, somme ProviderResponded events | Register dans `router.rs` | |
| | Returns: total_tokens, total_cost_usd, per_model breakdown | | |
| 3 | 5 backward compat + 5 integration tests | `runner.rs` tests | ~100 |

**Commit strategy**: 3 commits (preset.rs, nika:cost, tests)

---

### PRIORITÉ 2: Session E.3 — Quality Plan Bugs (~3h)

**Plan**: `docs/plans/2026-03-28-v051-master-quality-plan.md`

#### CRITICAL

| Bug | Fichier | Fix |
|-----|---------|-----|
| **CR1**: SchemaGuardrail `.check()` only validates `required` | `nika-core/src/ast/guardrails.rs:332` | Use `jsonschema` crate for full validation + 10 tests |

#### HIGH — Silent Failures

| Bug | Fichier | Fix |
|-----|---------|-----|
| **SF2**: Missing ProviderResponded on Layer 0a no-spec path | `executor/infer.rs:523-538` | Add event emission |
| **SF3**: for_each binding failures → no TaskFailed event | `runner.rs:1800-1809` | Emit TaskFailed |
| **SF4**: for_each "items could not be resolved" → no event | `runner.rs:2246-2261` | Emit TaskFailed |
| **SF5**: `jsonschema::validator_for().ok()` silently disables | `runner.rs:656` | Return error |
| **SF6**: EventLog drops trace writes with `let _ =` | `nika-event/src/log.rs:1042` | Log at warn! |
| **SF7**: Daemon job state updates silently dropped | `nika-daemon/src/services/jobs.rs:215-241` | Log failures |

#### MEDIUM

| Bug | Fix |
|-----|-----|
| **M-orig3**: `manifest: true` never writes artifacts.json | Implement `write_manifest()` in `artifact_processor.rs` |
| **M-orig6**: `{{for_each.index}}` unavailable in artifact paths | Inject variable during for_each expansion |
| **M-orig8**: Temperature not validated per-provider | Add validation (Anthropic max 1.0, OpenAI max 2.0) |
| **M-tok1**: Token counts = 0 when Final stream missing | Add fallback estimation |

**Commit strategy**: 1 bug = 1 commit. ~8-10 commits.

---

### PRIORITÉ 3: Session M — Record Compression / P-RECORD (~3h)

**Plans**: `docs/plans/sessions/session-M-record-compression.md` + `docs/plans/2026-03-28-phase1-record.md`

11 commits prévus:

| # | Tâche | Fichier(s) | LOC |
|---|-------|-----------|-----|
| 1 | `Record` struct | `runtime/record.rs` (NEW) | ~200 |
| 2 | Record storage in RunContext | `store/run_context.rs` | ~40 |
| 3 | `RecordSpec` AST type | `nika-core/src/ast/record.rs` (NEW) | ~50 |
| 4 | Parse `record:` from YAML | `parser.rs`, `task.rs`, `analyze.rs` | ~80 |
| 5 | Propagate through lower.rs | `workflow.rs`, `lower.rs` | ~10 |
| 6 | `RecordCompressor` | `runtime/record_compress.rs` (NEW) | ~400 |
| 7 | Wire into runner | `runner.rs` | ~80 |
| 8 | Record-aware bindings | `binding/resolve.rs` | ~60 |
| 9 | Events: RecordCreated, RecordSkipped | `nika-event/src/log.rs` | ~60 |
| 10 | `nika:records` introspection tool | `builtin/records.rs` (NEW) | ~140 |
| 11 | Error codes NIKA-320-324 | `error.rs` | ~30 |

---

### PRIORITÉ 4: Session N — Context + Memory / P-CONTEXT (~3h)

**Plans**: `docs/plans/sessions/session-N-context-memory.md` + `docs/plans/2026-03-28-phase1-context-memory.md`

15 tasks:

| Partie | Tâches | Résumé |
|--------|--------|--------|
| N.1: Token budgets | 4 tasks | `context_budget:` field, CJK-aware counting, proportional truncation, events |
| N.2: Introspection | 5 tasks | `nika:dag_info`, `nika:task_status`, `nika:threads`, `nika:orchestrate` (stub), registration |
| N.3: Memory local | 4 tasks | NDJSON persistence, SQLite FTS5, `nika trace search`, file locking |
| N.4: Security | 2 tasks | Output scanner (invisible Unicode, exfiltration, injection), frozen snapshot |

---

### PRIORITÉ 5: Session F.2 — Stringly-typed cleanup (~3h)

**Plan**: `docs/plans/sessions/session-F-stringly-typed.md` (Parts 2, 4, 7)

| # | Tâche | Impact |
|---|-------|--------|
| 1 | **ProviderName enum** | Replace `Option<String>` → typed enum (Anthropic, OpenAI, Mistral, Groq, DeepSeek, Gemini, XAi, Native, Mock, Custom(String)). 916 occurrences, 116 files. |
| 2 | **EventKind grouping** | 66 flat variants → nested enums (TaskEvent, ProviderEvent, McpEvent, AgentEvent...). Backward-compat Serde. |
| 3 | **Doc comment fix** | log.rs:5 says "44 variants" but reality is 66+. Add static assertion. |

---

### PRIORITÉ 6: Session I.2 — TUI Perf (~1h)

**Plan**: `docs/plans/sessions/session-I-tui-polish.md` (Bugs 2-4)

| # | Tâche | LOC |
|---|-------|-----|
| 1 | DAG layout cache (invalidate on task status change) | ~80 |
| 2 | Arc\<str\> for task_id in TUI state (~10 .to_string() → Arc::clone) | ~40 |
| 3 | Document format!() per-frame allocations | ~20 comments |

---

### PRIORITÉ 7: Session D.2 — Quality Infrastructure (~2h)

**Plan**: `docs/plans/sessions/session-D-quality-infra.md`

| # | Tâche |
|---|-------|
| 1 | `cargo-mutants` on 5 critical files (cost.rs, security.rs, transform.rs, flow.rs, guardrails.rs) |
| 2 | `tracing-error` SpanTrace integration |
| 3 | `cargo-deny` setup (AGPL license check, CVE scan) |
| 4 | 3 E2E stress workflows (concurrency, parallel extraction, transform chains) |

---

### PRIORITÉ 8-9: Sessions J.2 + H.2 (~2.5h total)

**J.2**: Registry graceful fallback, LSP preset completions, update llms.txt
**H.2**: Extension activation robustness (registerCommand before async), files.associations in package.json, import path validation, fix `completion_e2e.rs:673` (`json` transform doesn't exist)

---

### FUTURES (v0.53+)

| Session | Quoi |
|---------|------|
| O | Daemon + Media Pipeline |
| P | Scaleway + GPU (VPS 51.15.136.200, H100, L40S) |
| Q | Telegram Bot trigger |
| R | CI Pipeline + Release automation |
| S | Self-Improvement / Hermes Memory |
| T | MCP Server Mode (`nika serve --mcp`) |
| U | Registry + Packages |
| V | Final Polish (115 showcases, 44 course exercises) |

---

## 7. BUGS SYSTÉMIQUES ENCORE OUVERTS

Source: `docs/plans/sessions/session-review-findings.md`

| Catégorie | Count | Détail |
|-----------|-------|--------|
| `_ => {}` sans logging | 60+ | Rule 2 violation — catch-all silencieux |
| `unwrap_or(0)` en production | 50+ | Rule 1 violation — zéros silencieux |
| EventKind variants non testés | 28 | ArtifactWritten, MediaExtracted, etc. |
| `ContextAssembled` hardcoded zeros | 1 | budget_used_pct: 0.0, truncated: false |
| Chat path sans ProviderResponded | 1 | Event jamais émis en mode chat |
| `calculate_cost()` sans cache | 3 | structured_output.rs:206, thinking.rs:520, chat.rs:336 |
| `#[allow(dead_code)]` suspects | 42 | Potentiel code mort masqué |
| `unreachable!()` atteignables | 5 | runner.rs:5612, template.rs:313, rig.rs:721/723/837/839 |

---

## 8. MÉTHODOLOGIE

### Pour CHAQUE changement:

```
1. Lire le plan de session
2. Lire le code existant
3. Écrire un test qui ÉCHOUE
4. Implémenter le fix minimal
5. Vérifier que le test PASSE
6. cargo test --workspace --lib    → 0 failures
7. cargo clippy --workspace -- -D warnings → 0 warnings
8. git add <specific files>
9. git commit (format ci-dessous)
10. Répéter
11. git push après 2-3 commits
```

### Format de commit:

```
type(scope): description concise

Détails si nécessaire.

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

**Types**: `feat`, `fix`, `refactor`, `test`, `perf`, `docs`, `chore`
**Scopes**: `tui`, `ast`, `runtime`, `mcp`, `provider`, `dag`, `event`, `parser`, `cli`, `builtin`, `security`, `binding`

### Utilisation des agents parallèles:

Pour les tâches mécaniques (is_ok() strengthening, enum migration), dispatch **plusieurs agents en parallèle** sur des fichiers différents. Chaque agent travaille sur 2-3 fichiers max.

---

## 9. RÈGLES ABSOLUES

### Tests
1. **`cargo test --workspace --lib`** TOUJOURS (--lib pour éviter keychain macOS)
2. **TDD**: test FAIL → fix → test PASS → full suite → commit
3. Si un test casse → **REVERT**, passe au suivant

### Commits
4. **1 fix = 1 commit**. Jamais batching de fixes non-reliés.
5. **Co-authors TOUJOURS** (voir format ci-dessus)
6. **`git push`** après chaque 2-3 commits
7. **`cargo clippy --workspace -- -D warnings`** → ZERO warnings avant chaque commit

### Qualité
8. **JAMAIS** commiter du code qui ne compile pas
9. **JAMAIS** `.unwrap_or(0)`, `_ => {}`, `.ok()` sans logging
10. Si bloqué (3 tentatives) → **skip**, note dans progress.md, continue
11. **JAMAIS** marquer un bug comme "investigated/deferred/done" sans code fix + test

---

## 10. ENTRE CHAQUE SESSION

```bash
cd /Users/thibaut/dev/supernovae/nika/tools
cargo test --workspace --lib        # 0 failures
cargo clippy --workspace -- -D warnings  # 0 warnings
cd ..
git push
```

Mettre à jour `docs/plans/sessions/progress.md` avec:
- Nombre de commits
- Nombre de tests
- Résumé des changements
- Session(s) complétée(s)

---

## 11. QUAND LE CONTEXT WINDOW SE REMPLIT

1. **Commit et push TOUT** le travail en cours
2. Écrire un handoff dans `docs/plans/sessions/progress.md`:
   - Ce qui est fait
   - Ce qui est en cours
   - Ce qui reste
   - Le prochain item à attaquer
3. Donner l'instruction de relance:

```bash
claude --dangerously-skip-permissions --model opus -p "$(cat docs/plans/sessions/mega-prompt-v7-handoff.md)"
```

---

## 12. COMMENCER

```bash
# 1. Vérifier l'état
git log --oneline -5
cd /Users/thibaut/dev/supernovae/nika/tools
cargo test --workspace --lib 2>&1 | tail -5
cargo clippy --workspace -- -D warnings 2>&1 | tail -3

# 2. Lire progress
cat ../docs/plans/sessions/progress.md

# 3. Lire le plan de la prochaine session
# (Session L.3 = docs/plans/sessions/session-L-agent-presets.md, Tasks 4,7-8)

# 4. Attaque.
```

Pas de questions. Pas d'hésitation. Lis, code, test, commit, push, continue.
