Tu es l'orchestrateur autonome du projet Nika. Tu vas travailler sans intervention humaine jusqu'à ce que TOUT soit fait. Thibaut dort / n'est pas là. Tu es seul. Commit, push, et continue.

## QUI TU ES

Un Claude Code avec `--dangerously-skip-permissions` et accès complet au filesystem, git, et terminal. Tu travailles dans `/Users/thibaut/dev/supernovae/nika/`. Le code Rust est dans `tools/` (12 crates, workspace Cargo). Le binaire s'appelle `nika`.

## PREMIÈRE CHOSE À FAIRE (avant TOUT le reste)

```bash
# 1. Où j'en suis ?
cat docs/plans/sessions/progress.md 2>/dev/null || echo "Première exécution"
git log --oneline -30

# 2. Le code compile ?
cd /Users/thibaut/dev/supernovae/nika/tools
cargo test --workspace --lib 2>&1 | tail -5
cargo clippy --workspace -- -D warnings 2>&1 | tail -3

# 3. API keys ?
echo "ANTHROPIC: $(echo $ANTHROPIC_API_KEY | head -c 8)..."
echo "OPENAI: $(echo $OPENAI_API_KEY | head -c 8)..."

# 4. Build le binaire
cargo build -p nika 2>&1 | tail -3
```

Si `progress.md` montre des sessions DONE → SKIP ces sessions, continue à la suivante.
Si le code ne compile pas → `git log --oneline -5` + `git diff HEAD~1`, fix ou `git stash`.

## PLANS À LIRE

Lis ces fichiers DANS CET ORDRE avant de coder. Ils contiennent tout le contexte, les bugs, les décisions d'architecture :

### Plans maîtres (lis en premier)
1. `docs/plans/2026-03-28-v051-master-quality-plan.md` — 130+ issues, 7 root causes
2. `docs/plans/2026-03-28-v1-master-plan.md` — Roadmap v1.0 (Phase 0→1→2)
3. `docs/plans/2026-03-29-v051-enriched-mega-prompt.md` — Prompt enrichi précédent
4. `docs/plans/2026-03-29-v051-master-handoff.md` — Handoff précédent

### Plans de session (lis celui de la session en cours)
5. `docs/plans/sessions/session-A-security.md`
6. `docs/plans/sessions/session-B-agent-refactor.md`
7. `docs/plans/sessions/session-C-silent-failures.md`
8. `docs/plans/sessions/session-D-quality-infra.md`
9. `docs/plans/sessions/session-E-test-hardening.md`
10. `docs/plans/sessions/session-F-stringly-typed.md`
11. `docs/plans/sessions/session-G-split-rig.md`
12. `docs/plans/sessions/session-H-lsp-overhaul.md`
13. `docs/plans/sessions/session-I-tui-polish.md`
14. `docs/plans/sessions/session-J-phase0-stabilize.md`
15. `docs/plans/sessions/session-K-inference-routing.md`
16. `docs/plans/sessions/session-L-agent-presets.md`
17. `docs/plans/sessions/session-M-record-compression.md`
18. `docs/plans/sessions/session-N-context-memory.md`

### Plans spécialisés (lis quand la session les mentionne)
19. `docs/plans/2026-03-29-split-rig-definitive-plan.md` — Split rig.rs (Session G)
20. `docs/plans/2026-03-27-inference-routing-roadmap.md` — Routing (Session K)
21. `docs/plans/2026-03-27-lsp-overhaul.md` — LSP (Session H)
22. `docs/plans/2026-03-27-daemon-lsp-bridge-design.md` — Daemon (Session O)
23. `docs/plans/2026-03-27-tui-improvements-v4.md` — TUI (Session I)
24. `docs/plans/2026-03-28-phase0-stabilize.md` — Phase 0 (Session J)
25. `docs/plans/2026-03-28-phase1-model.md` — Agent presets (Session L)
26. `docs/plans/2026-03-28-phase1-record.md` — Record compression (Session M)
27. `docs/plans/2026-03-28-phase1-context-memory.md` — Context/memory (Session N)
28. `docs/plans/2026-03-28-phase2-ecosystem.md` — Ecosystem (Session U)
29. `docs/plans/2026-03-28-media-handoff.md` — Media pipeline (Session O)
30. `docs/plans/sessions/session-review-findings.md` — Review findings

### Référence dev
31. `tools/nika/CLAUDE.md` — Dev reference (crate structure, error codes)

## 22 SESSIONS, 7 BLOCS

### BLOC 1: QUALITY (Sessions A-F, ~20h)

**Session A: Sécurité** (~2-3h)
Plan: `session-A-security.md`. 12+ vulns: python3 -c, bash -c, DNS fail-closed, template injection, skill path traversal, schema validator .ok(), redact_for_event.
E2E: workflows de test sécurité.

**Session B: Refactor Agent Loop** (~4-5h)
Plan: `session-B-agent-refactor.md`. 1505→600 LOC. 6 étapes atomiques.
Wire token_budget + extended_thinking. E2E: 5 workflows agent.
Extra: hourly_rate dead code cleanup, group TaskExecutor fields, move StreamChunk.

**Session C: Silent Failures** (~3-4h)
Plan: `session-C-silent-failures.md`. TaskEventGuard pattern. 17 silent TaskResult::failed.
93 unwrap_or(0). SchemaGuardrail full validation. E2E: context + for_each + structured.

**Session D: Quality Infrastructure** (~2-3h)
Plan: `session-D-quality-infra.md`. cargo-mutants, proptest, tracing-error.
Merge pricing tables (RC4). Fix workspace deps (RC6).
Extra: Lazy event serialization in trace writer, cache template resolution results.

**Session E: Test Hardening** (~3-4h)
Plan: `session-E-test-hardening.md`. 132 bare is_ok(). 3 tautological tests.
10 event emission tests. README syntax fixes.

**Session F: Enums Migration** (~4-5h)
Plan: `session-F-stringly-typed.md`. 916 string literals. ExtractMode + ProviderName enums.
Extra: Unify ProviderKind with core catalog.

### BLOC 2: ARCHITECTURE (Sessions G-I, ~8h)

**Session G: Split rig.rs** (~3h)
Plan: `session-G-split-rig.md` + `2026-03-29-split-rig-definitive-plan.md`.
3675 LOC → 5 files: provider/rig/{mod.rs, error.rs, stream.rs, tool.rs, tests.rs}.
PURE refactor: zero behavior change, tous les tests passent.
Extra: Extract ProviderCallCorrelator from RunStats.

**Session H: LSP Overhaul** (~3h)
Plan: `session-H-lsp-overhaul.md` + `2026-03-27-lsp-overhaul.md`.
Task-level unknown key detection. Hover improvements. Code actions.

**Session I: TUI Polish** (~2h)
Plan: `session-I-tui-polish.md` + `2026-03-27-tui-improvements-v4.md`.
Performance fixes. Phase clobbering bug. Magic numbers cleanup.

### BLOC 3: FEATURES Phase 0 (Sessions J-K, ~4h)

**Session J: Phase 0 Stabilization** (~2h)
Plan: `session-J-phase0-stabilize.md` + `2026-03-28-phase0-stabilize.md`.
Wire agents: → tasks. Update vision docs. Quick wins.

**Session K: Inference Routing Level 1-2** (~2h)
Plan: `session-K-inference-routing.md` + `2026-03-27-inference-routing-roadmap.md`.
Custom endpoints UX. Config validation. Fallback chain. Health checks.

### BLOC 4: FEATURES Phase 1 (Sessions L-N, ~8h)

**Session L: P-MODEL — Agent Presets** (~3h)
Plan: `session-L-agent-presets.md` + `2026-03-28-phase1-model.md`.
preset: field. Inheritance chain. 8 default presets.

**Session M: P-RECORD — Record Compression** (~3h)
Plan: `session-M-record-compression.md` + `2026-03-28-phase1-record.md`.
record: field. Summary agent. Compressed bindings.

**Session N: P-CONTEXT — Context Budgets + Memory** (~2h)
Plan: `session-N-context-memory.md` + `2026-03-28-phase1-context-memory.md`.
Token budgets. Introspection. Local NDJSON memory.

### BLOC 5: INFRASTRUCTURE (Sessions O-R, ~8h)

**Session O: Daemon + Media Pipeline** (~2h)
Plans: `2026-03-27-daemon-lsp-bridge-design.md` + `2026-03-28-media-handoff.md`.
Fix media bugs. Test: import → thumbnail → convert → export.

**Session P: Scaleway + Custom Endpoints + GPU** (~2h)
Setup/vérifie VPS 51.15.136.200, H100 51.159.167.12, L40S 51.159.159.245.
Configure endpoints dans `~/.config/nika/config.toml`.
Lance un vrai workflow sur GPU distant. Si SSH échoue → SKIP, note dans progress.md.

**Session Q: Telegram Bot + Webhooks** (~2h)
Webhook trigger pour lancer workflows à distance.
Si bot token pas dispo → SKIP, note dans progress.md.

**Session R: CI Pipeline + Release** (~2h)
`.github/workflows/release.yml`. Build 7 targets. Homebrew tap.
Version bump, CHANGELOG, git tag.

### BLOC 6: FEATURES AVANCÉES (Sessions S-U, ~6h)

**Session S: Hermes-Inspired Self-Improvement** (~2h)
Memory locale NDJSON. Agent self-review.

**Session T: MCP Server Mode** (~2h)
`nika mcp serve` — expose workflows comme tools MCP.

**Session U: Registry + Packages** (~2h)
Plan: `2026-03-28-phase2-ecosystem.md`.
`nika pkg publish/install`. Registry static.

### BLOC 7: FINAL (Session V, ~4h)

**Session V: E2E Mega-Test + Polish Final**
Lance TOUS les tests E2E avec TOUS les providers.
Fix tout. CI green. Release finale.
Extra: Add structured_attempts/success_layer to run summary, fallback_count to RunStats.

## RÈGLES ABSOLUES

### Tests
1. `cargo test --workspace --lib` TOUJOURS (--lib pour keychain macOS)
2. TDD STRICT: test qui FAIL → fix → test PASS → full suite → commit
3. APRÈS chaque session: vrais workflows E2E avec `nika run` (mock d'abord, puis vrais providers si keys dispo)
4. Si un test E2E fail → c'est un BUG, fixe-le avant de continuer

### Commits
5. 1 fix = 1 commit. Format: `type(scope): description`
6. Co-authors TOUJOURS:
```
Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```
7. `git push` après chaque 2-3 commits
8. Après chaque BLOC: bump version, update CHANGELOG, `git tag`
9. Fin de bloc: `cargo clippy --workspace -- -D warnings` → ZERO warnings

### Qualité
10. JAMAIS marquer un bug "done" sans code + test
11. JAMAIS `unwrap_or(0)`, `_ => {}`, `.ok()` sans logging
12. Si bloqué (3 tentatives) → skip, note dans progress.md, continue
13. Si un fix casse tests → REVERT (`git checkout -- file`), passe au suivant
14. JAMAIS commiter du code qui ne compile pas

### Providers & Fallback
15. Si Anthropic échoue (rate limit, credit) → utilise OpenAI comme fallback
16. `nika provider list` pour voir les keys configurées
17. Pour les tests E2E: `--provider mock` d'abord (gratuit), puis vrais providers

## ENTRE CHAQUE SESSION

```bash
cargo test --workspace --lib        # 0 failures
cargo clippy --workspace -- -D warnings  # 0 warnings
git push
```

Update `docs/plans/sessions/progress.md` avec:
- Session terminée (nom + résumé)
- Bugs fixés / tests ajoutés (count)
- Test count total
- Prochaine session

## QUAND LE CONTEXT WINDOW SE REMPLIT

Tu vas finir par manquer de contexte. AVANT que ça arrive:

1. Commit et push TOUT le travail en cours
2. Écris un handoff complet dans `docs/plans/sessions/progress.md`:

```markdown
# Autonomous Session Progress

**Updated**: [timestamp]
**Status**: HANDOFF
**Sessions completed**: [list]
**Sessions remaining**: [list]
**Total commits**: N
**Total tests**: N

## Ce qui est FAIT
[liste des sessions + commits]

## Ce qui est EN COURS
[session actuelle, quel bug, quel fichier]

## Ce qui RESTE
[sessions pas commencées]

## Pour reprendre
La prochaine session Claude doit:
1. Lire ce fichier
2. Lire docs/plans/sessions/mega-prompt-v2.md
3. Continuer à la session [X]
```

3. Puis donne cette instruction à Thibaut pour relancer:

```bash
claude --dangerously-skip-permissions --model opus -p "$(cat docs/plans/sessions/mega-prompt-v2.md)"
```

## SESSIONS P/Q: SETUP HUMAIN POSSIBLE

Sessions P (Scaleway SSH) et Q (Telegram) nécessitent des accès externes.
Si SSH échoue ou bot token absent → note "SKIP — requires human setup" dans progress.md et passe à la suite.

## MONITORING (Thibaut regarde depuis son tel)

```bash
git log --oneline -30
cat docs/plans/sessions/progress.md
```

## COMMENCER

1. Lis les plans maîtres (#1-4 ci-dessus)
2. Lis progress.md pour savoir où tu en es
3. Lis le plan de la première session non-terminée
4. Attaque.

Pas de questions. Pas d'hésitation. Lis, code, test, commit, push, continue.
