Tu es l'orchestrateur autonome du projet Nika. Tu vas travailler pendant 30-40 heures sans intervention humaine. Commit et push en permanence pour que Thibaut puisse suivre via `git log`.

## CONTEXTE
- Nika = semantic YAML workflow engine pour AI. Schema: nika/workflow@0.12
- Workspace: /Users/thibaut/dev/supernovae/nika/tools/ (12 crates Rust)
- Branche: main, 8613+ tests, 0 clippy warnings
- 130+ bugs trouvés par 10 agents d'audit + 7 root causes architecturales
- Master plan v1.0: Phase 0 (Stabilize) → Phase 1 (Intelligence) → Phase 2 (Ecosystem)

## DOCUMENTS CLÉS — LIS-LES TOUS AVANT DE COMMENCER
- `docs/plans/2026-03-28-v051-master-quality-plan.md` — Master quality plan (130+ issues)
- `docs/plans/2026-03-28-v1-master-plan.md` — v1.0 roadmap (Phase 0→1→2)
- `docs/plans/sessions/session-A-security.md` à `session-F-stringly-typed.md` — 6 plans enrichis
- `docs/plans/sessions/session-review-findings.md` — Review findings
- `docs/plans/2026-03-29-split-rig-definitive-plan.md` — Split rig.rs 3675→5 files
- `docs/plans/2026-03-28-phase1-record.md` — P-RECORD feature
- `docs/plans/2026-03-28-phase1-model.md` — P-MODEL agent presets
- `docs/plans/2026-03-28-phase0-stabilize.md` — Phase 0 stabilization
- `docs/plans/2026-03-27-inference-routing-roadmap.md` — Routing levels 1→6
- `docs/plans/2026-03-27-lsp-overhaul.md` — LSP from broken to best-in-class
- `docs/plans/2026-03-27-daemon-lsp-bridge-design.md` — Daemon ↔ LSP bridge
- `docs/plans/2026-03-27-tui-improvements-v4.md` — TUI perf + coverage
- `docs/plans/2026-03-28-phase1-context-memory.md` — Context budgets + memory
- `docs/plans/2026-03-28-phase2-ecosystem.md` — Registry, pkg, community
- `docs/plans/2026-03-28-media-handoff.md` — Media pipeline findings
- `tools/nika/CLAUDE.md` — Dev reference

## RÈGLES ABSOLUES
1. `cargo test --workspace --lib` TOUJOURS (--lib pour keychain macOS)
2. 1 fix = 1 commit. Format: `type(scope): description` + co-authors Claude + Nika 🦋
3. `git push` après chaque 3-4 commits
4. Si un fix casse tests → REVERT immédiatement, passe au suivant
5. TDD: test FAIL d'abord → fix → test PASS → commit
6. JAMAIS marquer un bug "done" sans code + test
7. JAMAIS `unwrap_or(0)`, `_ => {}`, `.ok()` sans logging
8. Clippy ZERO: `cargo clippy --workspace -- -D warnings`
9. Si bloqué → skip, note dans progress.md, continue
10. Utilise les skills: /spn-powers:test-driven-development, /spn-powers:systematic-debugging
11. Après chaque session: lance des vrais workflows avec `nika run` (provider: mock ET anthropic)

## 15 SESSIONS (dans l'ordre)

### BLOC 1: QUALITY (Sessions A-F, ~20h)

**Session A: Sécurité** (~2-3h)
Lis `session-A-security.md`. 12+ vulns: python3 -c, bash -c, DNS fail-closed, template injection, skill path traversal, schema validator .ok(), redact_for_event.
E2E: lance les 4 workflows de test sécurité du plan.

**Session B: Refactor Agent Loop** (~4-5h)
Lis `session-B-agent-refactor.md`. 1505→600 LOC. 6 étapes atomiques.
Wire token_budget + extended_thinking. E2E: 5 workflows agent.

**Session C: Silent Failures** (~3-4h)
Lis `session-C-silent-failures.md`. TaskEventGuard pattern. 17 silent TaskResult::failed.
93 unwrap_or(0). SchemaGuardrail full validation. E2E: context + for_each + structured.

**Session D: Quality Infrastructure** (~2-3h)
Lis `session-D-quality-infra.md`. cargo-mutants, proptest, tracing-error.
Merge pricing tables (RC4). Fix workspace deps (RC6).

**Session E: Test Hardening** (~3-4h)
Lis `session-E-test-hardening.md`. 132 bare is_ok(). 3 tautological tests.
10 event emission tests. README syntax fixes.

**Session F: Enums Migration** (~4-5h)
Lis `session-F-stringly-typed.md`. 916 string literals. ExtractMode + ProviderName enums.
EventKind grouping si temps.

### BLOC 2: ARCHITECTURE (Sessions G-I, ~8h)

**Session G: Split rig.rs** (~3h)
Lis `2026-03-29-split-rig-definitive-plan.md`. 3675 LOC → 5 files.
provider/rig/{mod.rs, error.rs, stream.rs, tool.rs, tests.rs}.
PURE refactor: zero behavior change, tous les tests doivent passer.

**Session H: LSP Overhaul** (~3h)
Lis `2026-03-27-lsp-overhaul.md`. Fix LSP from broken to best-in-class.
Task-level unknown key detection. Hover improvements. Code actions.

**Session I: TUI Polish** (~2h)
Lis `2026-03-27-tui-improvements-v4.md`. Performance fixes. Phase clobbering bug.
Magic numbers cleanup. Test infrastructure.

### BLOC 3: FEATURES Phase 0 (Sessions J-K, ~4h)

**Session J: Phase 0 Stabilization** (~2h)
Lis `2026-03-28-phase0-stabilize.md` + `2026-03-28-v1-master-plan.md` Phase 0.
Wire agents: → tasks. Update vision docs. Quick wins.

**Session K: Inference Routing Level 1-2** (~2h)
Lis `2026-03-27-inference-routing-roadmap.md`. Custom endpoints UX.
Config validation. Fallback chain. Health checks.

### BLOC 4: FEATURES Phase 1 (Sessions L-N, ~8h)

**Session L: P-MODEL — Agent Presets** (~3h)
Lis `2026-03-28-phase1-model.md`. preset: field. Inheritance chain.
agent_defs in AST. 8 default presets (lite/think/search/vision/judge/coder/summary/default).

**Session M: P-RECORD — Record Compression** (~3h)
Lis `2026-03-28-phase1-record.md`. record: field. Summary agent.
Compressed bindings. Context growth = logarithmic.

**Session N: P-CONTEXT — Context Budgets + Memory** (~2h)
Lis `2026-03-28-phase1-context-memory.md`. Token budgets. Introspection.
Local NDJSON memory. Self-improvement hooks.

### BLOC 5: POLISH (Session O, ~2h)

**Session O: Daemon + Media + Release**
- Daemon LSP bridge (`2026-03-27-daemon-lsp-bridge-design.md`)
- Media pipeline findings (`2026-03-28-media-handoff.md`)
- Version bump, CHANGELOG, release prep
- Final E2E mega-test: ALL verbes, ALL providers, ALL features

## ENTRE CHAQUE SESSION
1. `cargo test --workspace --lib` → 0 failures
2. `cargo clippy --workspace -- -D warnings` → 0 warnings
3. `git push`
4. Update `docs/plans/sessions/progress.md` avec:
   - Session terminée
   - Bugs fixés / tests ajoutés
   - Test count total
   - Prochaine session
5. Lance un vrai workflow E2E: `cd tests && nika run verify-all.nika.yaml` (si existe)
6. Continue IMMÉDIATEMENT

## SI CONTEXT WINDOW SE REMPLIT
1. Commit et push TOUT
2. Écris handoff dans `docs/plans/sessions/progress.md`:
   - Ce qui est FAIT (commits)
   - Ce qui est EN COURS (quel bug de quelle session)
   - Ce qui RESTE
   - Instruction pour reprendre
3. Le prochain Claude reprend avec: `cat docs/plans/sessions/progress.md`

## MONITORING (Thibaut depuis son tel)
```bash
git log --oneline -30
cat docs/plans/sessions/progress.md
```

Commence MAINTENANT. Lis TOUS les plans cités, puis attaque Session A.
