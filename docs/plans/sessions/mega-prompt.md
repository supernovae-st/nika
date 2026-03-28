Tu es l'orchestrateur autonome du projet Nika. Tu vas travailler pendant 60-70 heures sans intervention humaine. Commit et push en permanence pour que Thibaut puisse suivre via `git log`. Tu as un BUDGET ILLIMITÉ pour les vrais appels LLM (Anthropic, OpenAI) — utilise autant d'argent que nécessaire pour les tests E2E réels.

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

### Tests
1. `cargo test --workspace --lib` TOUJOURS (--lib pour keychain macOS)
2. TDD STRICT: écrire test qui FAIL → implémenter fix → test PASS → run full suite → commit
3. Utilise le skill `/spn-powers:test-driven-development` pour CHAQUE fix
4. APRÈS chaque session: lance des VRAIS workflows E2E avec `nika run`:
   - `nika run tests/e2e-mega-test.nika.yaml` (provider: mock — gratuit, rapide)
   - `nika run tests/e2e-mega-test.nika.yaml --provider anthropic` (VRAIS appels LLM — budget illimité)
   - `nika run tests/e2e-mega-test.nika.yaml --provider openai` (test cross-provider)
   - `nika run tests/e2e-mega-test.nika.yaml --provider gemini` (test Gemini)
   - `nika run tests/e2e-mega-test.nika.yaml --provider xai` (test Grok/xAI)
   - Crée des workflows de test spécifiques à chaque session
5. Si un test E2E fail → c'est un BUG, fixe-le avant de passer à la suite

### Commits & Releases
6. 1 fix = 1 commit. Format: `type(scope): description` + co-authors Claude + Nika 🦋
7. `git push` après chaque 2-3 commits
8. Après chaque BLOC (A-F, G-I, J-K, L-N, O): bump version, update CHANGELOG, `git tag`
9. À la fin de chaque bloc: `cargo clippy --workspace -- -D warnings` — ZERO warnings

### Qualité
10. JAMAIS marquer un bug "done" sans code + test qui prouve le fix
11. JAMAIS `unwrap_or(0)`, `_ => {}`, `.ok()` sans logging explicite
12. Si bloqué sur un bug (3 tentatives max) → skip, note dans progress.md, continue
13. Si un fix casse tests → REVERT immédiatement (`git checkout -- file`), passe au suivant
14. Utilise `/spn-powers:systematic-debugging` pour les bugs complexes
15. Utilise `/spn-powers:verification-before-completion` avant de déclarer une session finie
16. Utilise `/spn-powers:brainstorming` avant les gros changements d'architecture

### Recherche & DX
17. Utilise Context7 (`ctx7 library` + `ctx7 docs`) pour les docs de crates
18. Utilise Perplexity pour les patterns Rust avancés
19. Utilise `/find-docs` pour les docs Claude Code / API
20. Améliore le CLI, l'UX, la cohérence visuelle au fur et à mesure
21. Push les designs à fond — chaque output doit être beau et cohérent

### Infrastructure
22. Setup Scaleway: VPS (51.15.136.200), H100 (51.159.167.12), L40S (51.159.159.245)
23. Setup bot Telegram webhook pour notifications de workflow
24. Vérifie que les custom endpoints fonctionnent avec les GPU Scaleway
25. Lance des vrais workflows sur les GPU distants pour vérifier

## 22 SESSIONS, 7 BLOCS (dans l'ordre)

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

### BLOC 5: INFRASTRUCTURE (Sessions O-R, ~8h)

**Session O: Daemon + Media Pipeline** (~2h)
- Daemon LSP bridge (`2026-03-27-daemon-lsp-bridge-design.md`)
- Media pipeline findings (`2026-03-28-media-handoff.md`)
- Fix tous les bugs media trouvés par l'audit
- Test: import → thumbnail → convert → export avec vrais fichiers

**Session P: Scaleway + Custom Endpoints + GPU** (~2h)
- Setup/vérifie VPS 51.15.136.200
- Setup/vérifie H100 51.159.167.12 (vLLM Qwen3.5-27B)
- Setup/vérifie L40S 51.159.159.245
- Configure `~/.config/nika/config.toml` avec endpoints GPU
- Lance un vrai workflow sur le H100: `nika run` avec `provider: h100`
- Vérifie inference routing entre cloud et self-hosted
- Lis `docs/plans/2026-03-27-inference-routing-roadmap.md`

**Session Q: Telegram Bot + Webhooks** (~2h)
- Lis la recherche Hermes: `docs/research/2026-03-27-hermes-agent-deep-dive.md`
- Implémente Telegram webhook trigger pour lancer des workflows à distance
- `nika trigger telegram --workflow research.nika.yaml`
- Setup le bot avec BotFather, webhook URL sur le VPS
- Lis `docs/plans/2026-03-28-phase2-ecosystem.md` pour le contexte

**Session R: CI Pipeline + Release** (~2h)
- Vérifie `.github/workflows/release.yml`
- Lance un cycle CI complet: build → test → clippy → publish
- Build les 7 targets (macOS arm64/x64, Linux arm64/x64, Windows, musl, docker)
- Test Homebrew tap update
- Version bump dans workspace Cargo.toml
- CHANGELOG.md complet
- Git tag + release
- Vérifie que `cargo install nika` fonctionne

### BLOC 6: FEATURES AVANCÉES (Sessions S-U, ~6h)

**Session S: Hermes-Inspired Self-Improvement** (~2h)
- Lis `docs/research/2026-03-27-hermes-agent-deep-dive.md`
- Implémente le loop de self-improvement: memory locale NDJSON
- Agent qui review ses propres outputs et s'améliore
- Background nudges pour quality assessment

**Session T: MCP Server Mode** (~2h)
- Lis `docs/research/2026-03-23-mcp-universal-bridge-research.md`
- Nika = MCP server exposant les workflows à Claude Code, Cursor, etc.
- `nika mcp serve` — expose les workflows comme tools MCP
- Test: depuis Claude Code, `invoke: "nika::run_workflow"` avec un workflow réel

**Session U: Registry + Packages** (~2h)
- Lis `docs/plans/2026-03-28-phase2-ecosystem.md`
- Deploy registry.supernovae.studio (GitHub Pages static)
- `nika pkg publish` pour publier des workflows
- `nika pkg install` pour installer depuis le registry
- Seed avec 10 workflows de showcase

### BLOC 7: FINAL (Session V, ~4h)

**Session V: E2E Mega-Test + Polish Final**
- Lance TOUS les tests E2E avec TOUS les providers (mock, anthropic, openai, h100)
- Vérifie CHAQUE feature documentée dans CLAUDE.md fonctionne réellement
- Fix tout ce qui ne marche pas
- CI en boucle: relance jusqu'à 100% green
- Update CLAUDE.md, README.md, CHANGELOG.md
- Release finale
- Commit de status final dans progress.md

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

## DÉMARRAGE — PREMIÈRE CHOSE À FAIRE (avant de lire les plans)

```bash
# 1. Déterminer ce qui est DÉJÀ fait (le watchdog a peut-être redémarré cette session)
git log --oneline -50
cat docs/plans/sessions/progress.md 2>/dev/null || echo "Première exécution"

# 2. Vérifier que le code compile et les tests passent
cd /Users/thibaut/dev/supernovae/nika/tools
cargo test --workspace --lib 2>&1 | tail -5
cargo clippy --workspace -- -D warnings 2>&1 | tail -3

# 3. Vérifier les API keys (nécessaires pour E2E)
echo "ANTHROPIC: $(echo $ANTHROPIC_API_KEY | head -c 8)..."
echo "OPENAI: $(echo $OPENAI_API_KEY | head -c 8)..."

# 4. Builder le binaire pour les tests E2E
cargo build -p nika 2>&1 | tail -3
```

Si progress.md existe et montre des sessions DONE → SKIP ces sessions.
Si le code ne compile pas → `git log --oneline -5` et `git diff HEAD~1` pour comprendre.
Si les tests ne passent pas → `git stash` et retenter.

## PROTECTION CONTRE LES CRASH / REPRISES

### Si tu es une session RELANCÉE par le watchdog:
1. Lis progress.md pour savoir où tu en es
2. Lis `git log --oneline -10` pour voir les derniers commits
3. Vérifie que le code compile: `cargo check --workspace`
4. Si ça compile → continue à la prochaine session non-terminée
5. Si ça ne compile pas → `git reset --hard HEAD~1` et continue

### Si les crédits API s'épuisent:
- Claude va s'arrêter proprement
- Le watchdog détecte la mort et attend 30 min
- Thibaut peut recharger les crédits
- Le watchdog relance automatiquement
- La nouvelle session reprend via progress.md

### JAMAIS commiter du code qui ne compile pas:
```bash
# AVANT chaque commit, TOUJOURS:
cargo check --workspace 2>&1 | tail -3
# Si "error" → ne PAS commiter, fixer d'abord
```

## SESSIONS QUI NÉCESSITENT UN SETUP HUMAIN

Les sessions P (Scaleway SSH) et Q (Telegram BotFather) nécessitent des accès que
Claude n'a peut-être pas. Si SSH vers 51.15.136.200 échoue ou si le bot token
n'est pas disponible:
1. Note dans progress.md: "Session P/Q: SKIP — requires human SSH/Telegram setup"
2. Passe directement à la session suivante
3. Thibaut fera le setup manuellement

## MONITORING (Thibaut depuis son tel)
```bash
git log --oneline -30
cat docs/plans/sessions/progress.md
```

Commence MAINTENANT. Lis TOUS les plans cités, puis attaque Session A.
