# Media Pipeline — Autonomous Execution Brief

> Copie ce fichier comme prompt dans un nouveau terminal Claude.
> Il contient TOUT pour executer les 3 PRs en autonomie totale.

---

## Identite

Tu es un ingenieur Rust senior qui implemente le media pipeline de Nika — un workflow engine YAML pour tasks AI. Tu travailles en **autonomie totale** avec des quality gates strictes entre chaque phase.

## Baseline

- **Nika**: v0.30.8 (Cargo.toml)
- **Schema**: @0.12 (`nika/workflow@0.12`) — latest, ne PAS bumper pour PR1/PR2
- **Tests**: ~5,219 pass, 0 fail
- **Clippy**: 2 warnings existants max, zero nouveau
- **Branch main**: `ea237946` (Live DAG + ANSI fix)

## Fichiers Plan (LIS DANS CET ORDRE)

```
docs/plans/2026-03-18-media-pipeline-master-plan.md      ← Architecture, 29 decisions
docs/plans/2026-03-18-media-pipeline-pr1-core.md         ← 16 commits, ~45 tests (95KB!)
docs/plans/2026-03-18-media-pipeline-pr2-artifacts.md    ← 8 commits, ~15 tests
docs/plans/2026-03-18-media-pipeline-pr3-input.md        ← 10 commits, future, @0.13
docs/plans/2026-03-18-media-types-research.md            ← 6 P0 MIME types
docs/plans/2026-03-18-media-pipeline-innovations-research.md ← Competitive intel
```

Lis le master plan + le PR en cours AVANT de coder. Le plan PR1 fait 95KB — chaque commit y est detaille avec le code exact, les fichiers, les line numbers, les tests.

---

## Workflow Global

```
PR1: feat/media-pipeline (Extraction + Processing)
│
├── Phase A: Type Foundation (commits 1-3)
│   ├── 1. refactor(mcp): ContentBlock struct → enum
│   ├── 2. fix(mcp): extract all 5 content types from rmcp
│   └── 3. feat(mcp): add ToolCallResult media helpers
│   └── ⏸️ QUALITY GATE A
│
├── Phase B: Media Module (commits 4-9)
│   ├── 4. feat(media): error types NIKA-251..259
│   ├── 5. feat(media): types MediaRef, MediaType, MediaBudget
│   ├── 6. feat(media): MIME detection with declare-then-verify
│   ├── 7. feat(media): async CAS store with io::atomic
│   ├── 8. feat(media): MediaProcessor pipeline
│   └── 9. feat(media): register module + deps
│   └── ⏸️ QUALITY GATE B
│
├── Phase C: Integration (commits 10-12)
│   ├── 10. feat(store): TaskResult.media + RunContext.media_staging
│   ├── 11. feat(event): add 4 media events (32 → 36)
│   └── 12. feat(runtime): wire MediaProcessor into invoke
│   └── ⏸️ QUALITY GATE C
│
├── Phase D: Tests (commits 13-16)
│   ├── 13. test(mcp): ContentBlock enum + media helpers
│   ├── 14. test(media): MIME, CAS, processor, budget, proptest
│   ├── 15. test(event): media events serde + variant count
│   └── 16. test(runtime): invoke → media pipeline integration
│   └── ⏸️ QUALITY GATE D (FINAL)
│
├── 🔍 CODE REVIEW (multi-agent)
├── bump version → v0.31.0
└── merge to main

PR2: feat/media-artifacts (after PR1 merged)
│
├── Commits 1-8 (Binary format, write_binary, CLI, E2E)
├── 🔍 CODE REVIEW
├── bump version → v0.32.0
└── merge to main

PR3: feat/media-input (future, after PR2)
│
├── Commits 1-10 (AST pipeline, rig-core vision, @0.13)
├── 🔍 CODE REVIEW
└── merge to main
```

---

## Protocole Par Commit

Pour CHAQUE commit, suis ce cycle exact :

### 1. LIRE le plan
```
Ouvre le plan PR (pr1-core.md, pr2-artifacts.md, ou pr3-input.md).
Lis la section du commit en cours. Note :
- Fichiers a toucher
- Code exact propose
- Tests attendus
- Points critiques signales (B1, H2, etc.)
```

### 2. TDD RED — Ecrire le test d'abord
```
Ecris le test qui DOIT FAIL avant implementation.
Si le plan montre des tests pour ce commit, ecris-les.
Verifie que le test fail : cargo test --lib <test_name>
```

### 3. IMPLEMENT — Code minimal
```
Ecris le code minimal pour faire passer le test.
Suis le code exact du plan — il a ete verifie par 60+ agents.
```

### 4. VERIFY — Triple check
```bash
cargo check                     # Compilation
cargo test --lib                # TOUS les tests (pas juste le nouveau)
cargo clippy --lib              # Zero nouveau warning
```
Si un seul fail → FIX avant de continuer. JAMAIS de commit avec un test qui fail.

### 5. SELF-REVIEW — Relis le diff
```bash
git diff --staged               # Relis chaque ligne
```
Checklist mentale :
- [ ] Imports tous necessaires ?
- [ ] Pas de `unused` variables ?
- [ ] Pas de `todo!()` ou `unimplemented!()` oublie ?
- [ ] Serde attributes corrects (#[serde(rename = "mimeType")] etc.) ?
- [ ] Les types matchent (String vs &str, u64 vs usize, etc.) ?

### 6. COMMIT
```bash
git add <fichiers specifiques>
git commit -m "$(cat <<'EOF'
type(scope): description

- Detail 1
- Detail 2

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
EOF
)"
```

### 7. CONTINUE au commit suivant

---

## Quality Gates (entre chaque Phase)

Apres Phase A, B, C, et D, STOP et fais :

```bash
# 1. Full test suite
cargo test --lib
# Attendu: 5,219+ pass, 0 fail

# 2. Clippy clean
cargo clippy --lib
# Attendu: max 2 warnings existants, zero nouveau

# 3. Verification fonctionnelle
# Phase A: ContentBlock serde roundtrip fonctionne
# Phase B: CAS store ecrit et lit des fichiers
# Phase C: run_invoke detecte du media
# Phase D: Tous les tests d'integration passent
```

Si un gate FAIL → debug et fix AVANT de passer a la phase suivante.

---

## Code Review Inter-Phase (multi-agent)

Apres chaque phase, lance une review :

```
/spn-powers:requesting-code-review
```

Ou manuellement : utilise le skill `spn-powers:code-reviewer` pour reviewer tout le code ecrit dans la phase. Verifie :
- Patterns Rust idiomatiques
- Securite (path traversal, size limits, etc.)
- Performance (copies inutiles, allocations, async safety)
- Coherence avec le reste du codebase Nika

---

## Skills a Utiliser

| Quand | Skill |
|-------|-------|
| Avant d'ecrire du Rust | `/spn-rust:rust` |
| Cycle TDD | `/spn-powers:test-driven-development` |
| Avant de dire "c'est fait" | `/spn-powers:verification-before-completion` |
| Pour commit | `/spn-powers:git:commit` |
| Pour review | `/spn-powers:requesting-code-review` |
| Si besoin docs crate | `/find-docs` |
| Si bug complique | `/spn-powers:systematic-debugging` |
| Si pattern unclear | `/spn-powers:brainstorming` |

---

## 10 Points Critiques (BUGS CORRIGES DANS LE PLAN)

Ces bugs ont ete trouves par 60+ agents de review. Le plan v5.0 les corrige, mais tu DOIS les avoir en tete :

### Serde
1. **`Text { text: String }`** — PAS `Text(String)`. Serde internally tagged ne marche PAS avec newtype(primitive).
2. **`#[serde(rename = "mimeType")]`** sur `mime_type` dans Image, Audio, ResourceLink.
3. **`Resource(ResourceContent)`** fonctionne car ResourceContent est un struct (serde flatten OK).

### Runtime
4. **`datastore`** est un PARAMETRE de run_invoke (`&RunContext`), pas `self.datastore`.
5. **`workspace_root`** → `std::env::current_dir()` — TaskExecutor n'a PAS ce field.
6. **`make_task_result()`** puis `.with_media()` — PAS `TaskResult::success()` direct.

### Error Handling
7. **`MediaError`** DOIT derive `miette::Diagnostic` (NikaError l'exige via `#[diagnostic(transparent)]`).
8. **`MediaError::code()`** retourne `&'static str` ("NIKA-251"), PAS u16.

### CAS Store
9. **`io::atomic::write_fail()`** ne cree PAS les parent dirs → `create_dir_all()` avant.
10. **CAS filenames** = hash ONLY, PAS d'extension (fix dedup .jpeg/.jpg).

---

## Deps a Ajouter (Cargo.toml)

```toml
# 2 genuinely new
blake3 = { version = "1.8", features = ["mmap"] }
infer = "0.19"

# 4 zero-cost promotes (already compiled as transitive deps)
base64 = "0.22"
mime_guess = "2.0"
mime = "0.3"
bytes = "1"
```

---

## Fichiers Cles et Line Numbers (v0.30.8)

```
src/mcp/types.rs:367          ContentBlock struct → enum
src/mcp/rmcp_adapter.rs:373   filter_map bottleneck → exhaustive match
src/runtime/executor/verbs.rs:929   tool_result.text() → media processing
src/runtime/executor/mod.rs:40      TaskExecutor fields (no workspace_root!)
src/store/run_context.rs:40         TaskResult → add media field
src/event/log.rs:142                EventKind → add 4 media events
src/error.rs:481                    NIKA-250 (251-259 FREE)
src/io/atomic.rs:172                write_fail (CAS dedup O_EXCL)
src/runtime/runner.rs:809           make_task_result → splice with_media
src/lib.rs:44                       pub mod mcp → add pub mod media after
src/main.rs:148                     Commands enum → add Media subcommand
src/ast/schema.rs:12                SchemaVersion → V12 is latest, @0.12
```

---

## Verification Finale PR1 (apres les 16 commits)

```bash
# Compilation
cargo check

# Tests (baseline + ~45 nouveaux)
cargo test --lib

# Clippy
cargo clippy --lib

# E2E : verifier que les workflows existants fonctionnent toujours
# (les workflows text-only ne doivent PAS etre affectes)
```

### Checklist Manuelle
```
[ ] ContentBlock serde roundtrip — 5 variants (Text, Image, Audio, Resource, ResourceLink)
[ ] mimeType en camelCase dans le JSON serialise
[ ] CAS store dedup — same content = same file (hash-only filename)
[ ] Empty base64 "" → rejected (NIKA-258)
[ ] 101MB base64 → rejected (NIKA-257)
[ ] Hash prefix → "blake3:af1349..."
[ ] MIME case normalized → "IMAGE/PNG" → "image/png"
[ ] MIME declare-then-verify → category mismatch logged
[ ] MediaBudget → RunBudgetExceeded si > 500MB
[ ] write_fail AlreadyExists → dedup success (not error)
[ ] Read-back verify skipped for < 1MB
[ ] for_each media → per-iteration access (step[0].media, step[1].media)
[ ] for_each parent → media is empty (no aggregation)
[ ] EventKind count test → 36 variants
[ ] MediaStored event has pipeline_ms timing
[ ] NikaError::MediaError has #[diagnostic(transparent)]
[ ] All 5,219+ existing tests still pass
```

---

## Apres PR1 : Enchainer PR2

Une fois PR1 merge sur main :

```bash
git checkout main && git pull
git checkout -b feat/media-artifacts
```

Puis lis `docs/plans/2026-03-18-media-pipeline-pr2-artifacts.md` et execute les 8 commits avec le meme protocole. Target: v0.32.0.

---

## Apres PR2 : Enchainer PR3 (si decide)

PR3 est plus ambitieux (AST pipeline + vision). Schema bump @0.13.
Lis `docs/plans/2026-03-18-media-pipeline-pr3-input.md`.
Target: v0.33.0.

---

## Regles d'Or

1. **Le plan a TOUJOURS raison** — il a ete verifie par 60+ agents. Si tu doutes, relis le plan.
2. **TDD ou rien** — jamais de code sans test.
3. **Zero test qui fail** — jamais de commit avec un test rouge.
4. **1 commit = 1 changement logique** — pas de mega-commits.
5. **Review entre phases** — utilise `/spn-powers:requesting-code-review`.
6. **Si bloque → demande** — utilise AskUserQuestion plutot que deviner.
7. **Les line numbers sont exacts** — verifies contre v0.30.8, ±1 ligne max.

---

## GO

1. Cree la branche : `git checkout -b feat/media-pipeline`
2. Lis le master plan (master-plan.md sections "Key Decisions" + "Execution Protocol")
3. Lis le PR1 plan (pr1-core.md) EN ENTIER
4. Attaque Phase A, Commit 1 : `refactor(mcp): ContentBlock struct → enum`
5. Suis le protocole commit par commit
