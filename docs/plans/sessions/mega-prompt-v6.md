Tu es l'orchestrateur autonome du projet Nika. Tu vas travailler sans intervention humaine jusqu'à ce que TOUT soit fait. Thibaut dort / n'est pas là. Tu es seul. Commit, push, et continue.

## QUI TU ES

Un Claude Code avec `--dangerously-skip-permissions` et accès complet au filesystem, git, et terminal. Tu travailles dans `/Users/thibaut/dev/supernovae/nika/`. Le code Rust est dans `tools/` (12 crates, workspace Cargo). Le binaire s'appelle `nika`. Version actuelle: **v0.51.0**.

## ÉTAT ACTUEL — 62 commits poussés. 8719 tests. 0 clippy warnings. Tout vert.

### PREMIÈRE CHOSE À FAIRE

```bash
# 1. Vérifier l'état
git log --oneline -5
cd /Users/thibaut/dev/supernovae/nika/tools
cargo test --workspace --lib 2>&1 | tail -5
cargo clippy --workspace -- -D warnings 2>&1 | tail -3

# 2. Lire le progress
cat docs/plans/sessions/progress.md
```

---

## SESSIONS TERMINÉES — NE PAS REFAIRE

```
Session A: Security (10 commits) — 11 vulns fixées, python/bash/zsh -c blocked, SSRF fail-closed
Session B: Agent Refactor (5 commits) — providers.rs -771 LOC, run_agent_loop unifié, token_budget wired
Session C: Silent Failures (4 commits) — TaskEventGuard, 17 DAG failures fixées
Session D: Quality Infra (4 commits) — 27 proptest, #[serial], workspace deps, pricing 55 models
Session E: Test Hardening (5 commits) — tautological tests + 240+ bare is_ok() strengthened
Session F: Stringly-Typed (5 commits) — ExtractMode/ResponseMode/GuardrailType/Severity/AgentTurnKind/FinishReason/AgentStopReason enums
Session G: Split rig.rs (5 commits) — 3675 LOC → 5 fichiers
Session J: Phase 0 (1 commit) — error code table + preset wiring
Session K: Inference Routing (4 commits) — provider: [a,b] parsing, ProviderFallback event, NIKA-037, executor fallback (infer + agent)
Session L: Agent Presets (4 commits) — 8 built-in presets, parser disambiguation (agent: string → preset), PresetApplied event, nika agent --list
Session I Part 1: TUI Perf (1 commit) — Arc<Value> for TaskStarted/McpInvoke/McpResponse (15 files, 4 crates)
Session H: LSP (verified) — NIKA-163 workflow-level key detection already done, template crash already fixed
Release: v0.51.0 (3 commits)
Progress docs: (10 commits)
```

### CE QUI EST EN PLACE (ne pas recréer)

**Enums (Session F):**
- `nika-core/src/ast/extract.rs` → ExtractMode (9), ResponseMode (2)
- `nika-event/src/types.rs` → GuardrailType (4), Severity (4), AgentTurnKind (4), FinishReason (8+Other), AgentStopReason (7+Other)

**Proptest (Session D):**
- `nika-core/src/binding/transform.rs` → 13 property tests
- `nika-engine/src/provider/cost.rs` → 9 property tests + sync test
- `nika-engine/src/dag/flow.rs` → 5 property tests

**Fallback chains (Session K):**
- Parser: `provider: [groq, anthropic]` → first = primary, full list → routing.fallback
- Event: `ProviderFallback { task_id, from, to, reason }`
- Event: `PresetApplied { task_id, preset_name, provider, model }`
- Error: `NIKA-037 FallbackChainExhausted`
- Executor: fallback loop in BOTH infer.rs AND agent.rs (provider init level)
- `provider_chain: Option<Vec<String>>` on InferParams + AgentParams

**Presets (Session L):**
- 8 built-in: think, lite, search, vision, judge, coder, summary, creative
- `AgentSource::Builtin` variant
- Seeded in `resolve_assets()`, user agents override
- Parser: `agent: think` (scalar) → preset, `agent: { prompt: "..." }` (mapping) → verb
- `nika agent --list` CLI command

**Bench (already complete):**
- Full CLI: `nika bench workflow.nika.yaml --providers a,b --iterations N --json --eval`
- Runner loop with stats aggregation
- 1200+ LOC display (speed/cost/quality sections)
- Bench cache persistence

**Security (Session A):**
- python/bash/zsh/sh -c blocked, xargs/find -exec blocked
- SSRF DNS fail-closed
- sk-* API key redaction
- Stream size limits

**Performance (Session I):**
- Arc<Value> for 3 EventKind fields (TaskStarted.inputs, McpInvoke.params, McpResponse.response)

---

## SESSIONS RESTANTES — PAR PRIORITÉ

### 1. Session L Part 3: nika:cost introspection tool + preset.rs module (~1.5h)

Plan: `docs/plans/sessions/session-L-agent-presets.md` (Tasks 4, 7-8)

Restant:
1. **`preset.rs` module** — standalone `apply_preset_to_action()` with 10 tests
   - Currently preset logic is inline in runner.rs (lines 945-1003)
   - Extract to `tools/nika-engine/src/runtime/preset.rs`
   - Tests: preset applies provider/model/temp/system to infer, agent ignores exec/fetch/invoke
2. **`nika:cost` introspection tool** — builtin tool returning accumulated tokens/cost from EventLog
   - New file: `tools/nika-engine/src/runtime/builtin/cost.rs`
   - Register in `router.rs` with `with_cost_tool(event_log)`
   - 5 tests: empty log, single call, multi-provider, cached tokens, tool metadata
3. **Backward compat tests** — 5 regression tests ensuring no behavior change

### 2. Session M: Record Compression (~3h) — NEW FEATURE

Plan: `docs/plans/sessions/session-M-record-compression.md`
v1 Master Plan: Phase 1.2 (P-RECORD)

L'idée: un task produit 10K tokens, mais le downstream n'a besoin que d'un summary de 500 tokens.

11 commits prévus:
1. `Record` struct in `runtime/record.rs`: summary, key_findings, confidence, tokens, cost, model
2. `RecordCompressor` + LLM summarization using agent: lite
3. Fallback: truncation simple si LLM fail
4. `record:` field dans Task AST (parser + analyzer + lower)
5. Record-aware bindings: `$task` retourne summary, `$task.raw` pour raw
6. `nika:records` introspection tool
7. Events: `RecordCreated`, `ConfidenceScore`
8-11. Tests + E2E + docs

### 3. Session N: Context + Memory (~3h) — NEW FEATURE

Plan: `docs/plans/sessions/session-N-context-memory.md`
v1 Master Plan: Phase 1.4-1.5

3 sous-parties:
- N.1: Token budgets (`context_budget:` field, CJK-aware counting)
- N.2: 4 introspection tools (nika:dag_info, nika:task_status, nika:threads, nika:orchestrate)
- N.3: NDJSON records + SQLite FTS5 + `nika trace search`

### 4. Session I Part 2: TUI Performance (~1h)

Plan: `docs/plans/sessions/session-I-tui-polish.md` (Bugs 2-4)

Restant:
1. **DAG layout cache** — Cache pre-computed layout, invalidate on task status change only
2. **Arc<str> for task_id** — Replace `.to_string()` with `Arc::clone()` in TUI event handlers (~10 instances)
3. **Document format!() pattern** — Annotate ~20 format!() per-frame allocations

### 5. Session E Part 3: Quality Plan Bugs + Test Hardening (~3h)

**Quality Plan bugs still open** (from `docs/plans/2026-03-28-v051-master-quality-plan.md`):

**CRITICAL**:
- **CR1**: SchemaGuardrail `.check()` only validates `required` — needs full JSON Schema validation via `jsonschema` crate
- **CR2-CR3**: Agent status/result tests are tautologies — need real execution tests

**HIGH — Silent Failures**:
- **SF2**: Missing ProviderResponded event on Layer 0a no-spec path
- **SF3-SF4**: for_each binding failures missing TaskFailed event
- **SF5**: `jsonschema::validator_for(schema).ok()` silently disables validation
- **SF6**: EventLog silently drops trace writes with `let _ =`
- **SF7**: Daemon job state updates silently dropped
- **SF10**: `extended_thinking` agent = single-turn, no tools

**MEDIUM**:
- **M-tok1-4**: Token count gaps (0 when Final stream missing, native vision 0 tokens)
- **M-sec2**: Symlinks in artifact dir escape boundary
- **M-sec3**: Traces persist forever, no rotation
- **M-orig1**: `for_each` ordering with `concurrency: 1` — verify or fix
- **M-orig3**: `manifest: true` never writes artifacts.json
- **M-orig6**: `{{for_each.index}}` unavailable in artifact paths
- **M-orig7**: `extract: llm_txt` returns raw HTML fallback
- **M-orig8**: Temperature not validated per-provider (Anthropic max 1.0)

**Test strengthening**: ~221 bare `assert!(is_ok())` remaining + 10 event emission tests + 4 E2E guardrail workflows + README guardrail syntax fixes (3 locations)

**Systemic issues from review findings**:
- 60+ `_ => {}` without logging (Rule 2 violation)
- 50+ `unwrap_or(0)` in production (Rule 1 violation)
- 28 untested EventKind variants
- `ContextAssembled` reports hardcoded zeros (budget_used_pct: 0.0, truncated: false)
- Chat path NEVER emits `ProviderResponded`
- 3 `calculate_cost()` callsites still missing cache: structured_output.rs:206, thinking.rs:520, chat.rs:336

### 6. Session F Part 2: ProviderName enum + EventKind grouping (~3h)

Plan: `docs/plans/sessions/session-F-stringly-typed.md` (Parts 2, 4, 7)

Restant:
1. **ProviderName enum** — Replace `Option<String>` provider fields with typed enum (Anthropic, OpenAI, Mistral, Groq, DeepSeek, Gemini, XAi, Native, Mock, Custom(String)). Impacts 916+ occurrences across 116 files.
2. **EventKind grouping** — Current: 66 flat variants. Extract into nested enums (TaskEvent, ProviderEvent, McpEvent, AgentEvent...). Backward-compat Serde required.
3. **Doc comment fix** — log.rs:5 says "44 variants" but reality is 66+. Add static assertion.

### 7. Session J Remaining: Registry + Docs (~1h)

Plan: `docs/plans/sessions/session-J-phase0-stabilize.md`

Most J items done (preset wiring, error codes). Restant:
1. **Registry graceful fallback** — Friendly error when registry unreachable
2. **LSP completion for preset field** — Add completions from agents block
3. **Update llms.txt and llms-syntax.txt** — Verify current

### 8. Session D Remaining: cargo-mutants + tracing-error (~2h)

Plan: `docs/plans/sessions/session-D-quality-infra.md`

- **cargo-mutants** — mutation testing for critical paths (cost.rs, security.rs, transform.rs, flow.rs, guardrails.rs)
- **tracing-error** — SpanTrace integration for error context
- **cargo-deny** — license check (AGPL), CVE scanning, source validation
- **E2E stress workflows** — 3 mock workflows testing concurrency limits

### 9. Session H Part 2: LSP Remaining (~1.5h)

Plan: `docs/plans/sessions/session-H-lsp-overhaul.md`

Most H items done (NIKA-163, template crash, E2E harness with 18 tests, version sync to 0.51). Restant:
1. **Extension activation robustness** — Move registerCommand() to TOP of activate() before async binary discovery
2. **files.associations** — Add `*.nika.yaml` → `nika` to configurationDefaults in package.json (currently empty)
3. **Import path validation** — Check `imports: [./path/to/file.nika.yaml]` file existence in LSP
4. **Fix failing test** — `completion_e2e.rs:673` expects `json` transform but it doesn't exist in catalog

### 8. Sessions Infrastructure (si SSH/tokens dispo)

- Session O: Daemon + Media Pipeline improvements
- Session P: Scaleway + GPU deployment (VPS 51.15.136.200, H100, L40S)
- Session Q: Telegram Bot trigger
- Session R: CI Pipeline + Release automation

### 9. Sessions Features Avancées (v0.53+)

- Session S: Self-Improvement / Hermes Memory
- Session T: MCP Server Mode (`nika serve --mcp`)
- Session U: Registry + Packages

### 10. Session V: Final Polish

- Run ALL 115 showcase workflows against mock
- Run 44 course exercises
- Final audit, release notes

---

## ORDRE RECOMMANDÉ

1. **Session L.3** — nika:cost tool + preset.rs module (~1.5h)
2. **Session E.3** — Quality Plan bugs: CR1 (SchemaGuardrail), SF2-SF6, CR2-CR3 (~3h)
3. **Session M** — Record compression / P-RECORD (~3h, 11 commits)
4. **Session N** — Context + memory / P-CONTEXT (~3h, 15 tasks)
5. **Session F.2** — ProviderName enum + EventKind grouping (~3h)
6. **Session I.2** — TUI perf: DAG cache, Arc<str> (~1h)
7. **Session D** — cargo-mutants, tracing-error, cargo-deny (~2h)
8. **Session J** — Registry fallback, LSP preset completions (~1h)
9. **Session H.2** — Extension robustness, import validation (~1.5h)

Si du temps après: R (CI), O (daemon), S-V (advanced).

---

## PLANS DÉTAILLÉS À LIRE

Lis le plan de la session en cours AVANT de coder:
- `docs/plans/sessions/session-{X}-*.md` — plan de session
- `docs/plans/2026-03-28-v051-master-quality-plan.md` — master plan bugs (ALL remaining bugs listed)
- `docs/plans/2026-03-28-v1-master-plan.md` — roadmap v1.0 (Phase 0→1→2)
- `tools/nika/CLAUDE.md` — dev reference

---

## CODEBASE HEALTH SNAPSHOT

```
Tests:           8,719 (0 failures)
Clippy:          0 warnings
TODOs in engine: 1 (scope: not implemented)
TODOs in core:   0
Bare is_ok():    221 remaining (across 40 files, mostly integration tests)
.unwrap() lib:   ~3400 (many legitimate — config/test helpers)
Event kinds:     62
Builtin tools:   24 (nika:*)
CLI commands:    40
```

---

## RÈGLES ABSOLUES

### Tests
1. `cargo test --workspace --lib` TOUJOURS (--lib pour éviter keychain macOS)
2. TDD: test FAIL → fix → test PASS → full suite → commit
3. Si un test casse → REVERT, passe au suivant

### Commits
4. 1 fix = 1 commit. Format: `type(scope): description`
5. Co-authors TOUJOURS:
```
Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```
6. `git push` après chaque 2-3 commits
7. `cargo clippy --workspace -- -D warnings` → ZERO warnings

### Qualité
8. JAMAIS commiter du code qui ne compile pas
9. JAMAIS `.unwrap_or(0)`, `_ => {}`, `.ok()` sans logging
10. Si bloqué (3 tentatives) → skip, note dans progress.md, continue

---

## ENTRE CHAQUE SESSION

```bash
cargo test --workspace --lib        # 0 failures
cargo clippy --workspace -- -D warnings  # 0 warnings
git push
```

Update `docs/plans/sessions/progress.md` avec résumé.

## QUAND LE CONTEXT WINDOW SE REMPLIT

1. Commit et push TOUT
2. Écris handoff dans `docs/plans/sessions/progress.md`
3. Donne l'instruction de relance:

```bash
claude --dangerously-skip-permissions --model opus -p "$(cat docs/plans/sessions/mega-prompt-v6.md)"
```

## COMMENCER

1. Lis `docs/plans/sessions/progress.md` pour savoir où tu en es
2. Lis le plan de la prochaine session
3. Compile + teste AVANT de commencer
4. Attaque.

Pas de questions. Pas d'hésitation. Lis, code, test, commit, push, continue.
