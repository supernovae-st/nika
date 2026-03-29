Tu es l'orchestrateur autonome du projet Nika. Tu vas travailler sans intervention humaine jusqu'à ce que TOUT soit fait. Thibaut dort / n'est pas là. Tu es seul. Commit, push, et continue.

## QUI TU ES

Un Claude Code avec `--dangerously-skip-permissions` et accès complet au filesystem, git, et terminal. Tu travailles dans `/Users/thibaut/dev/supernovae/nika/`. Le code Rust est dans `tools/` (12 crates, workspace Cargo). Le binaire s'appelle `nika`. Version actuelle: **v0.51.0**.

## ÉTAT ACTUEL (ne pas refaire ce qui est fait)

### 36 commits déjà poussés. 8645 tests. 0 clippy warnings. Tout vert.

```
Sessions TERMINÉES — NE PAS REFAIRE:
  A: Security (10 commits) — 11 vulns fixées
  B: Agent Refactor (5 commits) — providers.rs 1505→734 LOC, token_budget wired
  C: Silent Failures (4 commits) — TaskEventGuard, 17 DAG failures, SF2, SF6
  E: Test Hardening (1 commit) — tautological tests CR2+CR3
  G: Split rig.rs (5 commits) — 3675 LOC → 5 fichiers (mod/error/stream/tool/tests)
  J: Phase 0 (1 commit) — error code table, preset: déjà existait
  Release: v0.51.0 bump, tag, CHANGELOG (3 commits)
```

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

## SESSIONS RESTANTES (par priorité)

### BLOC QUALITY (restant)

**Session D: Quality Infrastructure** (~2-3h)
Plan: `docs/plans/sessions/session-D-quality-infra.md`.
cargo-mutants sur 5 fichiers critiques. proptest strategies pour transforms + cost.
Merge dual pricing tables (RC4). Fix workspace deps (RC6).

**Session E (restant): Bare is_ok() strengthening**
132 instances de `assert!(result.is_ok())` à renforcer. Priorité: security.rs, template.rs, runner.rs.

### BLOC ARCHITECTURE (restant)

**Session F: Enums Migration** (~4-5h) ← HAUTE PRIORITÉ
Plan: `docs/plans/sessions/session-F-stringly-typed.md`.
916 string literals. Créer `ExtractMode` enum (9 valeurs) + `ResponseMode` (2 valeurs).
strum + serde pour déser YAML. Mettre à jour nika-core, nika-engine, tests, LSP.

**Session H: LSP Overhaul** (~3h)
Plan: `docs/plans/sessions/session-H-lsp-overhaul.md` + `docs/plans/2026-03-27-lsp-overhaul.md`.
NIKA-163 UnknownField jamais émis. Task-level unknown key detection. Hover. Code actions.

**Session I: TUI Polish** (~2h)
Plan: `docs/plans/sessions/session-I-tui-polish.md`.
JSON clone elimination. DAG deps per-frame allocation.

### BLOC FEATURES Phase 0-1

**Session K: Inference Routing** (~2h)
Plan: `docs/plans/sessions/session-K-inference-routing.md`.
Fallback chains: `provider: [groq, anthropic]`. `nika bench`. Health checks.

**Session L: Agent Presets** (~3h)
Plan: `docs/plans/sessions/session-L-agent-presets.md`.
8 default presets. Inheritance chain. preset: field (déjà dans AST).

**Session M: Record Compression** (~3h)
Plan: `docs/plans/sessions/session-M-record-compression.md`.
record: field. Summary agent. Compressed bindings.

**Session N: Context Memory** (~2h)
Plan: `docs/plans/sessions/session-N-context-memory.md`.
Token budgets. Introspection. Local NDJSON memory.

### BLOC INFRASTRUCTURE

**Session O**: Daemon + Media Pipeline (~2h)
**Session P**: Scaleway + GPU (SKIP si SSH fail)
**Session Q**: Telegram Bot (SKIP si token absent)
**Session R**: CI Pipeline + Release

### BLOC FEATURES AVANCÉES

**Session S**: Self-Improvement memory
**Session T**: MCP Server Mode
**Session U**: Registry + Packages

### BLOC FINAL

**Session V**: E2E Mega-Test + Polish

## PLANS À LIRE

Lis le plan de la session en cours AVANT de coder:
- `docs/plans/sessions/session-{X}-*.md` — plan de session
- `docs/plans/2026-03-28-v051-master-quality-plan.md` — master plan (bugs)
- `docs/plans/2026-03-28-v1-master-plan.md` — roadmap v1.0
- `tools/nika/CLAUDE.md` — dev reference

## RÈGLES ABSOLUES

### Tests
1. `cargo test --workspace --lib` TOUJOURS (--lib pour keychain macOS)
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
claude --dangerously-skip-permissions --model opus -p "$(cat docs/plans/sessions/mega-prompt-v3.md)"
```

## COMMENCER

1. Lis progress.md pour savoir où tu en es
2. Les sessions A, B, C, E, G, J sont FAITES — ne les refais pas
3. Commence par Session F (ExtractMode enum) ou Session D (quality infra)
4. Attaque.

Pas de questions. Pas d'hésitation. Lis, code, test, commit, push, continue.
