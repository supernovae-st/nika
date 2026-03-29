Tu es l'orchestrateur autonome du projet Nika. Tu vas travailler sans intervention humaine jusqu'à ce que TOUT soit fait. Thibaut dort / n'est pas là. Tu es seul. Commit, push, et continue.

## QUI TU ES

Un Claude Code avec `--dangerously-skip-permissions` et accès complet au filesystem, git, et terminal. Tu travailles dans `/Users/thibaut/dev/supernovae/nika/`. Le code Rust est dans `tools/` (12 crates, workspace Cargo). Le binaire s'appelle `nika`. Version actuelle: **v0.51.0**.

## ÉTAT ACTUEL — 50 commits poussés. 8711 tests. 0 clippy warnings. Tout vert.

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
Session A: Security (10 commits) — 11 vulns fixées
Session B: Agent Refactor (5 commits) — providers.rs -771 LOC, run_agent_loop unifié
Session C: Silent Failures (4 commits) — TaskEventGuard, 17 DAG failures fixées
Session D: Quality Infra (4 commits) — 27 proptest, #[serial], workspace deps, pricing merge
Session E: Test Hardening (1 commit) — tautological tests CR2+CR3
Session F: Stringly-Typed (5 commits) — ExtractMode/ResponseMode/GuardrailType/Severity/AgentTurnKind/FinishReason/AgentStopReason enums
Session G: Split rig.rs (5 commits) — 3675 LOC → 5 fichiers
Session J: Phase 0 (1 commit) — error code table + preset wiring
Session K Part 1: Inference Routing (3 commits) — provider: [a,b] parsing, ProviderFallback event, executor fallback loop
Session L Part 1: Agent Presets (1 commit) — 8 built-in presets (think, lite, search, vision, judge, coder, summary, creative)
Release: v0.51.0 (3 commits)
Progress docs: (8 commits)
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
- Error: `NIKA-037 FallbackChainExhausted`
- Executor: fallback loop in `infer.rs` (provider init level)
- `provider_chain: Option<Vec<String>>` on InferParams + AgentParams

**Presets (Session L):**
- 8 built-in: think, lite, search, vision, judge, coder, summary, creative
- `AgentSource::Builtin` variant
- Seeded in `resolve_assets()`, user agents override

**Pricing (Session D RC4):**
- 55 model entries (was 22) in `nika-core/src/catalogs/cost.rs`
- Two-pass matching: exact first, contains() fallback
- Sync test in engine validates core covers all engine models

**Workspace deps (Session D RC6):**
- 57 deps moved to [workspace.dependencies]
- keyring features unified (superset)
- zeroize version unified (1.8)

---

## SESSIONS RESTANTES — PAR PRIORITÉ

### 1. Session K Part 2: nika bench + Agent Fallback (~2h)

Plan: `docs/plans/sessions/session-K-inference-routing.md` (Part 2)

Restant:
1. **Agent executor fallback** — même pattern que infer, dans `executor/agent.rs` ou `rig_agent_loop/`
2. **`nika bench` CLI command** — benchmark LLM providers
   - Argument parsing: `--iterations N`, `--providers a,b,c`, `--json`
   - Bench runner loop: clone workflow, override provider, collect stats
   - Comparison table: speed (TTFT, tok/s), cost ($/run), profile
   - Cache: `.nika/bench-cache/`
3. **Tests E2E mock provider fallback**

### 2. Session L Part 2: Parser Disambiguation + Events (~1h)

Plan: `docs/plans/sessions/session-L-agent-presets.md` (Tasks 1-5)

Restant:
1. **Parser disambiguation**: `agent: think` (string) → preset ref vs `agent: { prompt: "..." }` (mapping) → verb
   - In parse_action(), check if agent: value is Scalar → store as agent_preset on RawTask
   - New field: `RawTask.agent_preset: Option<Spanned<String>>`
2. **Propagate agent_preset** through AnalyzedTask → lower.rs → Task
3. **Merge agent_preset with preset** field (unify into single preset: field)
4. **AgentPresetUsed event** — track when a preset is applied
5. **`nika agent list` CLI command** — list available presets

### 3. Session I: TUI Performance (~2h)

Plan: `docs/plans/sessions/session-I-tui-polish.md`

4 optimizations:
1. `Arc<Value>` wrapping pour JSON event data (éviter clone per-frame)
2. DAG layout caching (invalidate seulement sur task status change)
3. `Arc<str>` pour task_id everywhere (éviter .to_string() allocations)
4. Documenter les format!() per-frame allocations restantes

### 4. Session M: Record Compression (~3h)

Plan: `docs/plans/sessions/session-M-record-compression.md`

L'idée: un task produit 10K tokens, mais le downstream n'a besoin que d'un summary de 500 tokens.

11 commits prévus:
1. `Record` struct: summary, key_findings, confidence, tokens
2. `RecordCompressor` + LLM summarization
3. Fallback: truncation simple si LLM fail
4. `record:` field dans Task AST
5. Record-aware bindings: `$task` retourne summary, `$task.raw` pour raw
6. `nika:records` introspection tool
7. Events + tests + docs

### 5. Session N: Context + Memory (~3h)

Plan: `docs/plans/sessions/session-N-context-memory.md`

3 sous-parties:
- N.1: Token budgets (`context_budget:` field, CJK-aware counting)
- N.2: 4 introspection tools (nika:dag_info, nika:task_status, nika:threads, nika:orchestrate)
- N.3: NDJSON records + SQLite FTS5 + `nika trace search`

### 6. Session E Part 2: Bare is_ok() strengthening (~1h)

132+ instances de `assert!(result.is_ok())` à renforcer.
Priorité: security.rs, template.rs, runner.rs.
Pattern: `assert!(result.is_ok())` → `let val = result.unwrap(); assert_eq!(val, expected);`

### 7. Session H: LSP E2E Tests (~1h)

Plan: `docs/plans/sessions/session-H-lsp-overhaul.md`

Restant:
- VS Code extension version sync
- Extension command IDs matching
- YAML detection pattern
- LSP validation parity (NIKA-163 diagnostics)
- E2E test harness stdio JSON-RPC + 6 protocol tests

### 8. Sessions Infrastructure (si SSH/tokens dispo)

- Session O: Daemon + Media Pipeline
- Session P: Scaleway + GPU (VPS 51.15.136.200, H100, L40S)
- Session Q: Telegram Bot
- Session R: CI Pipeline + Release

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

1. **Session K.2** — nika bench + agent fallback
2. **Session L.2** — parser disambiguation, AgentPresetUsed
3. **Session I** — TUI performance
4. **Session M** — record compression
5. **Session N** — context + memory
6. **Session E.2** — bare is_ok() strengthening
7. **Session H** — LSP E2E

Si du temps après: R (CI), O (daemon), S-V (advanced).

---

## PLANS DÉTAILLÉS À LIRE

Lis le plan de la session en cours AVANT de coder:
- `docs/plans/sessions/session-{X}-*.md` — plan de session
- `docs/plans/2026-03-28-v051-master-quality-plan.md` — master plan bugs
- `docs/plans/2026-03-28-v1-master-plan.md` — roadmap v1.0
- `tools/nika/CLAUDE.md` — dev reference

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
claude --dangerously-skip-permissions --model opus -p "$(cat docs/plans/sessions/mega-prompt-v5.md)"
```

## COMMENCER

1. Lis `docs/plans/sessions/progress.md` pour savoir où tu en es
2. Lis le plan de la prochaine session
3. Compile + teste AVANT de commencer
4. Attaque.

Pas de questions. Pas d'hésitation. Lis, code, test, commit, push, continue.
