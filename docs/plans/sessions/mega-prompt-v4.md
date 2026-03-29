Tu es l'orchestrateur autonome du projet Nika. Tu vas travailler sans intervention humaine jusqu'à ce que TOUT soit fait. Thibaut dort / n'est pas là. Tu es seul. Commit, push, et continue.

## QUI TU ES

Un Claude Code avec `--dangerously-skip-permissions` et accès complet au filesystem, git, et terminal. Tu travailles dans `/Users/thibaut/dev/supernovae/nika/`. Le code Rust est dans `tools/` (12 crates, workspace Cargo). Le binaire s'appelle `nika`. Version actuelle: **v0.51.0**.

## ÉTAT ACTUEL (ne pas refaire ce qui est fait)

### 41 commits poussés. 2153 tests (--lib). 0 clippy warnings. Tout vert.

```
Sessions TERMINÉES — NE PAS REFAIRE:
  A: Security (10 commits) — 11 vulns fixées (shell -c, SSRF DNS, path traversal, etc.)
  B: Agent Refactor (5 commits) — providers.rs 1505→734 LOC, run_agent_loop unifié, token_budget wired
  C: Silent Failures (4 commits) — TaskEventGuard, 17 DAG failures, SF2 ProviderResponded, SF6 event logging
  E: Test Hardening (1 commit) — tautological tests CR2+CR3 remplacés
  F: Stringly-Typed (5 commits) — ExtractMode/ResponseMode/GuardrailType/Severity/AgentTurnKind/FinishReason/AgentStopReason enums
  G: Split rig.rs (5 commits) — 3675 LOC → 5 fichiers (mod/error/stream/tool/tests)
  J: Phase 0 (1 commit) — error code table + preset: already existed
  Release: v0.51.0 bump, tag, CHANGELOG (3 commits)
  Progress: 6 handoff docs
```

### CE QUI A ÉTÉ FAIT DANS SESSION F (détail)

**Part 1** — `ExtractMode` (9 variants: Markdown/Article/Text/Selector/Metadata/Links/Jsonpath/Feed/LlmTxt) + `ResponseMode` (Full/Binary) dans `nika-core/src/ast/extract.rs`. Migré AnalyzedFetchAction, FetchParams, apply_extract(), fetch.rs, CLI. ~186 assertions de tests mises à jour dans 14 fichiers.

**Part 2** — `GuardrailType` (Length/Schema/Regex/Llm), `Severity` (Low/Medium/High/Critical), `AgentTurnKind` (Started/Continue/NaturalCompletion/ExplicitCompletion), `FinishReason` (Stop/EndTurn/ToolUse/..., + Other(String)), `AgentStopReason` (EndTurn/MaxTurns/..., + Other(String)) dans `nika-event/src/types.rs`. Migré EventKind, display, TUI, 24+ fichiers.

**Part 3** — LSP completions utilisent `ExtractMode::ALL_NAMES`. compact filtre les empty strings. round(0) retourne int. EventKind variant count: 44→58.

**NON FAIT (déféré):**
- Part 4: EventKind grouping en sub-enums (risque ÉLEVÉ, scope énorme de serde compat)
- ProviderName enum migration (916 occurrences, besoin de Custom(String))

### BUGS DÉJÀ FIXÉS DANS DES SESSIONS PRÉCÉDENTES

- NIKA-163: Unknown workflow/task keys → DÉTECTÉ avec suggestions Levenshtein (parser.rs:1398, 1847)
- template_validation crash → validate_document gère déjà les parse errors via match
- Tous les bugs CRITICAL/HIGH du master quality plan v0.51 sont fixés

### PREMIÈRE CHOSE À FAIRE

```bash
# 1. Où j'en suis ?
cat docs/plans/sessions/progress.md
git log --oneline -10

# 2. Le code compile ?
cd /Users/thibaut/dev/supernovae/nika/tools
cargo test --workspace --lib 2>&1 | tail -5
cargo clippy --workspace -- -D warnings 2>&1 | tail -3
```

---

## SESSIONS RESTANTES (par priorité)

### BLOC QUALITY (restant)

**Session D: Quality Infrastructure** (~2-3h)
Plan: `docs/plans/sessions/session-D-quality-infra.md`

Tâches:
1. `cargo install cargo-mutants` puis mutation testing sur 5 fichiers critiques:
   - `nika-core/src/binding/transform.rs` — 31 transforms, tester que les mutations cassent les tests
   - `nika-engine/src/provider/cost.rs` — pricing tables, NaN guards
   - `nika-engine/src/runtime/security.rs` — exec blocklist (security-critical)
   - `nika-core/src/ast/raw/parser.rs` — NIKA-163, schema validation
   - `nika-core/src/ast/analyzer/analyze.rs` — analyzer correctness
2. proptest strategies:
   - All 31 transforms never panic + handle null correctly
   - Cost calculation always valid (no NaN/negative/infinite)
   - Template parsing robustness with random inputs
3. RC4: merge dual pricing tables dans cost.rs (cloud_cost vs COST_TABLE)
4. RC6: workspace dependency cleanup (unused deps)

**Session E (restant): Bare is_ok() strengthening**
132+ instances de `assert!(result.is_ok())` à renforcer.
Priorité: security.rs, template.rs, runner.rs.
Pattern: `assert!(result.is_ok())` → `let val = result.unwrap(); assert_eq!(val, expected);`

---

### BLOC ARCHITECTURE (restant)

**Session H: LSP Overhaul** (~2h restant)
Plan: `docs/plans/sessions/session-H-lsp-overhaul.md`

Bugs 4-6 (template crash, NIKA-163, task keys) DÉJÀ FIXÉS. Ce qui reste:
- Bug 1: VS Code extension version sync (version.json vs package.json)
- Bug 2: Extension command IDs ne match pas les enregistrements
- Bug 3: YAML detection pattern trop strict (.nika.yaml only)
- Bug 9: LSP validation parity (NIKA-163 errors dans LSP = diagnostics)
- Tasks 14-15: E2E test harness stdio JSON-RPC + 6 protocol tests
- Task 16: Fix `transform_chain_completions` test

**Session I: TUI Performance** (~2h)
Plan: `docs/plans/sessions/session-I-tui-polish.md`

4 optimizations:
1. Arc<Value> wrapping pour JSON event data (éviter clone per-frame)
2. DAG layout caching (invalidate seulement sur task status change)
3. Arc<str> pour task_id everywhere (éviter .to_string() allocations)
4. Documenter les format!() per-frame allocations restantes

---

### BLOC FEATURES Phase 1 (v0.51-0.55)

**Session K: Inference Routing** (~3h) ← HAUTE PRIORITÉ FEATURE
Plan: `docs/plans/sessions/session-K-inference-routing.md`

L'idée: `provider: [groq, anthropic]` = fallback chain.

11 commits prévus:
1. `ProviderFallback` struct + config parsing (provider: array syntax en YAML)
2. FallbackExecutor dans runner.rs (try provider[0], si fail → provider[1], etc.)
3. `ProviderFallback` event + NIKA-037 error code
4. Health check ping avant fallback (NIKA-038 = all providers exhausted)
5. `nika bench` command: benchmark LLM sur un prompt standard
6. `nika bench --compare` : table comparative (tokens/s, TTFT, cost)
7. `nika bench --profile quick|thorough` : presets
8. Cache des résultats bench en `.nika/bench/`
9. `--json` export pour CI
10. Tests E2E mock provider fallback
11. Docs + help text

**Session L: Agent Presets** (~3h)
Plan: `docs/plans/sessions/session-L-agent-presets.md`

L'idée: `agents:` block au niveau workflow, `agent: think` au niveau task = héritage.
Le field `preset:` existe DÉJÀ dans l'AST (depuis Phase 0). Il faut le wirer.

11 commits prévus:
1. Parser disambiguation: `agent: think` (string → preset ref) vs `agent: { prompt: "..." }` (verb)
2. Standalone `preset.rs` module: `apply_preset_to_action()`
3. Inheritance chain: agent def → task override → workflow default
4. 8 default presets: think, lite, search, vision, judge, coder, summary, creative
5. `AgentPresetUsed` event
6. `nika:cost` introspection tool (token/cost breakdown per-task)
7. `nika agent list` CLI command
8. Backward compat tests (agent: verb still works)
9. LSP completions pour agents: block
10. Docs + help text
11. Tests E2E preset resolution

**Session M: Record Compression** (~3h) ← PILIER POUR v0.53
Plan: `docs/plans/sessions/session-M-record-compression.md`

L'idée: un task produit 10K tokens, mais le downstream n'a besoin que d'un summary de 500 tokens.

11 commits prévus:
1. `Record` struct: summary, key_findings, confidence, tokens_raw, tokens_compressed, cost, model_used
2. `RecordCompressor` + LLM summarization (utilise agent: lite par défaut)
3. Fallback: si LLM fail → truncation simple (first 500 tokens)
4. `record:` field dans Task AST (compress: bool, retain: [list], max_tokens: N, confidence_threshold: 0.8)
5. Record-aware bindings: `with: { data: $task }` retourne le Record summary, pas le raw
6. `$task.raw` pour accéder au raw output (opt-in)
7. `nika:records` introspection tool: query accumulated records
8. `RecordCreated` + `RecordSkipped` events
9. Tests: compression ratio, confidence, backward compat
10. Docs + exemples
11. E2E workflow avec 3 tasks + compression chain

**Session N: Context + Memory** (~3h)
Plan: `docs/plans/sessions/session-N-context-memory.md`

3 sous-parties:

**N.1: P-CONTEXT** — Token budgets
- `context_budget:` field sur tasks (0-200K tokens)
- Token counting CJK-aware (~2 chars/token vs ~4 chars/token)
- `enforce_budget()` avec proportional truncation (min 50 tokens/binding)
- `BudgetOk` / `BudgetExceeded` events

**N.2: P-INTROSPECT** — 4 builtin tools
- `nika:dag_info` — task_count, edge_count, critical_path, parallel_groups
- `nika:task_status` — status, duration, tokens, cost, has_record
- `nika:threads` — running tasks by status
- `nika:orchestrate` — stub pour v0.53

**N.3: P-MEMORY-LOCAL** — Persistence cross-session
- NDJSON records: `.nika/records/{workflow}_{timestamp}.ndjson`
- SQLite FTS5 index pour full-text search
- `nika trace search` CLI avec --since, --workflow, --limit
- Frozen snapshot pattern (context locked après premier set)
- Advisory file locking pour concurrent writes
- Output security scanner (zero-width chars, exfiltration, role hijack)

---

### BLOC INFRASTRUCTURE

**Session O: Daemon + Media Pipeline** (~2h)
- Daemon auto-start improvements
- Media pipeline CAS integrity checks
- Binary artifact writer fixes

**Session P: Scaleway + GPU** (SKIP si SSH fail)
- VPS 51.15.136.200
- H100 51.159.167.12 (vLLM Qwen3.5-27B)
- L40S 51.159.159.245
- Deployer vLLM, tester `provider: h100` dans workflow

**Session Q: Telegram Bot** (SKIP si token absent)
- Webhook trigger: POST → lance workflow
- `nika trigger telegram` command

**Session R: CI Pipeline + Release**
- Build from tools/nika (NOT tools/)
- Tag must match HEAD
- 18 secrets, 9 publish platforms, SLSA, macOS notarize

---

### BLOC FEATURES AVANCÉES (v0.53+)

**Session S: Self-Improvement / Hermes Memory**
- Post-workflow review agent (nudge)
- Pattern extraction from successful runs
- Cost optimization suggestions

**Session T: MCP Server Mode**
- Nika AS an MCP server (expose workflows as tools)
- `nika serve --mcp` command
- Tool discovery, schema generation

**Session U: Registry + Packages**
- `nika pkg publish` / `nika pkg install`
- registry.supernovae.studio (GitHub static or proper server)
- Package versioning, dependencies

### BLOC FINAL

**Session V: E2E Mega-Test + Polish**
- Run ALL showcase workflows (115) against mock provider
- Run course workflows (44 exercises)
- Final audit, release notes, README update

---

## PLANS DÉTAILLÉS À LIRE

Lis le plan de la session en cours AVANT de coder:
- `docs/plans/sessions/session-{X}-*.md` — plan de session (contient commits exacts + fichiers)
- `docs/plans/2026-03-28-v051-master-quality-plan.md` — master plan bugs (3 CRITICAL, 16 HIGH, 25 MEDIUM fixés)
- `docs/plans/2026-03-28-v1-master-plan.md` — roadmap v1.0 (Phase 0 → 1 → 2)
- `tools/nika/CLAUDE.md` — dev reference (error codes, testing, conventions)

## ROADMAP V1.0 CONDENSÉE

```
Phase 0: DONE ✓ (v0.50-0.51)
  - LSP fixé, agents: wired, vision docs, showcase/course CLI

Phase 1: EN COURS (v0.51-0.55)
  v0.51 = P-MODEL (presets + routing)        ← Sessions K + L
  v0.52 = P-RECORD (compression)             ← Session M
  v0.53 = P-ORCHESTRATE (goal: + dynamic DAG) ← PAS DE SESSION (futur)
  v0.54 = P-CONTEXT + P-INTROSPECT           ← Session N
  v0.55 = P-MEMORY-LOCAL + self-improvement   ← Session N.3 + Session S

Phase 2: FUTUR (v0.56-0.60)
  - Registry deploy, nika pkg publish, community seed
  - Fine-tuning pipeline, Telegram trigger, MCP server expansion
```

## ORDRE RECOMMANDÉ POUR CETTE SESSION

1. **Session D** (quality infra) — cargo-mutants + proptest → fondation qualité
2. **Session K** (inference routing) — `provider: [groq, anthropic]`, `nika bench`
3. **Session L** (agent presets) — `agent: think`, 8 presets, inheritance
4. **Session I** (TUI perf) — Arc<Value>, DAG cache, Arc<str>
5. **Session M** (record compression) — Record struct, compressor, record-aware bindings
6. **Session N** (context + memory) — context_budget, introspection tools, NDJSON + FTS5

Si tu as du temps après: Session H (LSP E2E tests), Session E (bare is_ok), Session R (CI).

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

### Enums déjà créés (NE PAS RECRÉER)
- `nika-core/src/ast/extract.rs` → ExtractMode (9), ResponseMode (2)
- `nika-event/src/types.rs` → GuardrailType (4), Severity (4), AgentTurnKind (4), FinishReason (8+Other), AgentStopReason (7+Other)
- Re-exports: `nika-engine/src/ast/mod.rs` → `pub mod extract`
- Re-exports: `nika-event/src/lib.rs` → `pub use types::*`

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
claude --dangerously-skip-permissions --model opus -p "$(cat docs/plans/sessions/mega-prompt-v4.md)"
```

## COMMENCER

1. Lis `docs/plans/sessions/progress.md` pour savoir où tu en es
2. Lis le plan de la prochaine session (session-D, session-K, etc.)
3. Compile + teste AVANT de commencer
4. Attaque.

Pas de questions. Pas d'hésitation. Lis, code, test, commit, push, continue.
