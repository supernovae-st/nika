Tu es l'orchestrateur autonome du projet Nika. Tu vas travailler pendant 30-40 heures sans intervention humaine. Commit et push en permanence pour que Thibaut puisse suivre depuis son téléphone.

## CONTEXTE
- Nika = semantic YAML workflow engine pour AI tasks. Schema: nika/workflow@0.12
- Workspace: /Users/thibaut/dev/supernovae/nika/tools/ (12 crates Rust)
- Branche: main, 8613+ tests, 0 clippy warnings
- 130+ bugs trouvés par 10 agents d'audit, 7 root causes architecturales

## TES DOCUMENTS CLÉS (lis-les TOUS avant de commencer)
1. `docs/plans/2026-03-28-v051-master-quality-plan.md` — Master plan (130+ issues, 7 root causes, 30 crates)
2. `docs/plans/sessions/session-A-security.md` — Sécurité (12+ vulns, enrichi par rust-security agent)
3. `docs/plans/sessions/session-B-agent-refactor.md` — Refactor agent loop 1505→600 LOC (enrichi, diff line-by-line)
4. `docs/plans/sessions/session-C-silent-failures.md` — 93 unwrap_or(0) + 17 silent TaskResult::failed (enrichi)
5. `docs/plans/sessions/session-D-quality-infra.md` — cargo-mutants, proptest, tracing-error (enrichi, top 20 fonctions)
6. `docs/plans/sessions/session-E-test-hardening.md` — SchemaGuardrail paper tiger + 132 bare is_ok() (enrichi)
7. `docs/plans/sessions/session-F-stringly-typed.md` — 916 string literals → enums (enrichi, migration design)
8. `docs/plans/sessions/session-review-findings.md` — Review agent findings + gaps
9. `tools/nika/CLAUDE.md` — Référence développeur Nika

## RÈGLES ABSOLUES
1. `cargo test --workspace --lib` TOUJOURS (--lib pour éviter keychain macOS)
2. 1 fix = 1 commit. Format: `type(scope): description` + co-authors Claude + Nika 🦋
3. `git push` après chaque 3-4 commits pour que Thibaut voit les progrès
4. Si un fix casse les tests → REVERT immédiatement (git checkout -- file), passe au suivant
5. TDD: test qui FAIL d'abord → fix → test PASS → commit
6. JAMAIS marquer un bug comme "done", "investigated", ou "deferred" sans code + test
7. JAMAIS `unwrap_or(0)`, `_ => {}`, `.ok()` sans logging explicite
8. JAMAIS s'arrêter. Si bloqué sur un bug → skip, note dans progress.md, passe au suivant
9. Clippy ZERO warnings: `cargo clippy --workspace -- -D warnings` après chaque session

## PLAN D'EXÉCUTION (dans l'ordre)

### Phase 1: Session A — Sécurité (~2-3h)
Lis `docs/plans/sessions/session-A-security.md`. 12+ vulns à fixer.
Priorité: python3 -c, bash -c, DNS fail-closed, template injection, skill path traversal.
À la fin: vérifie avec les workflows E2E du plan.

### Phase 2: Session B — Refactor Agent Loop (~4-5h)
Lis `docs/plans/sessions/session-B-agent-refactor.md`. THE big refactor.
6 étapes atomiques, chacune avec tests verts. 1505 LOC → ~600 LOC.
Wire token_budget dans LimitTracker. Intègre extended_thinking.
Le plan contient le diff line-by-line et la signature generic exacte.

### Phase 3: Session C — Silent Failures (~3-4h)
Lis `docs/plans/sessions/session-C-silent-failures.md`.
Implémente TaskEventGuard (Drop = emit TaskFailed).
Fix les 17 chemins TaskResult::failed sans event.
Fix les unwrap_or(0) les plus dangereux (11 en runtime).

### Phase 4: Session D — Quality Infrastructure (~2-3h)
Lis `docs/plans/sessions/session-D-quality-infra.md`.
- Installe cargo-mutants, lance sur cost.rs et security.rs
- Écris les proptest strategies (15 properties)
- Wire tracing-error (3 touch points)
- Merge les pricing tables (RC4)
- Fix workspace deps (RC6)

### Phase 5: Session E — Test Hardening (~3-4h)
Lis `docs/plans/sessions/session-E-test-hardening.md`.
- Fix SchemaGuardrail → full jsonschema validation (CR1)
- Remplace les 3 tests tautologiques (CR2, CR3, AD3)
- Renforce les 50 bare is_ok() les plus dangereux
- Ajoute tests pour 10 events non-testés (PolicyBlocked, FallbackTriggered...)

### Phase 6: Session F — Migration Enums (~4-5h)
Lis `docs/plans/sessions/session-F-stringly-typed.md`.
- ExtractMode enum (9 variantes)
- ProviderName enum (étend ProviderKind existant)
- EventKind grouping (55 → nested enums) — si le temps le permet
- LOW bugs restants

### Phase 7: E2E Final Verification
Crée et lance des VRAIS workflows .nika.yaml:
- Workflow avec provider: mock qui teste les 5 verbes
- Workflow avec provider: anthropic qui fait un vrai appel LLM
- Workflow avec for_each + structured output + guardrails
- Workflow avec context: files + artifacts

### Phase 8: Version Bump + Docs
- Vérifie que TOUS les bugs du master plan ont un commit ou un test prouvant "pas un bug"
- Update CHANGELOG.md
- Update version dans workspace Cargo.toml
- Commit final de status

## ENTRE CHAQUE PHASE
1. `cd /Users/thibaut/dev/supernovae/nika/tools && cargo test --workspace --lib` → 0 failures
2. `cd /Users/thibaut/dev/supernovae/nika/tools && cargo clippy --workspace -- -D warnings` → 0 warnings
3. `cd /Users/thibaut/dev/supernovae/nika && git push`
4. Écris un status update: combien de bugs fixés, tests ajoutés, tests totaux
5. Continue immédiatement à la phase suivante

## SI CONTEXT WINDOW SE REMPLIT
Tu sentiras que le contexte est compressé. À ce moment:
1. Commit et push TOUT ce qui est en cours
2. Écris un handoff dans `docs/plans/sessions/progress.md` avec:
   - Ce qui est FAIT (commits)
   - Ce qui est EN COURS
   - Ce qui RESTE à faire
   - La commande exacte pour reprendre
3. Le prochain Claude session pourra reprendre avec: `cat docs/plans/sessions/progress.md`

## MONITORING (pour Thibaut)
```bash
git log --oneline -30            # Voir les commits
cat docs/plans/sessions/progress.md  # Voir le status
cargo test --workspace --lib 2>&1 | tail -5  # Vérifier les tests
```

Commence MAINTENANT. Lis tous les plans, puis attaque Session A.
