# Media Pipeline PR1 — Execution Brief

## Context

Tu es dans le repo Nika (workflow engine Rust pour AI tasks).
Tu vas implementer PR1 du media pipeline : extraction + processing des contenus binaires MCP.

**Baseline**: Nika v0.30.8 | **Target**: v0.31.0 | **Branch**: `feat/media-pipeline`

## Plan Files (LIRE EN PREMIER)

Lis ces fichiers dans cet ordre AVANT de coder :

1. **Master plan** : `docs/plans/2026-03-18-media-pipeline-master-plan.md`
   - Architecture, decisions (D1-D29), competitive advantages
   - Lis les sections "Key Decisions" et "Execution Protocol"

2. **PR1 plan** : `docs/plans/2026-03-18-media-pipeline-pr1-core.md`
   - 16 commits, 4 phases (A/B/C/D), ~45 tests
   - CHAQUE commit a son code exact, ses fichiers, ses tests
   - Lis la section "Execution Protocol" et "Verification Checklist"

3. **Media types** : `docs/plans/2026-03-18-media-types-research.md`
   - 6 P0 MIME types, SVG gap, alias handling

## Execution Protocol — CHAQUE COMMIT

```
1. TDD RED    — Ecris le test d'abord. Il doit FAIL.
2. IMPLEMENT  — Code minimal pour passer le test.
3. cargo check        — DOIT passer
4. cargo test --lib   — DOIT passer (5,219+ baseline)
5. cargo clippy --lib — Max 2 warnings existants, ZERO nouveau
6. SELF-REVIEW        — Relis ton diff. Verifie imports, unused vars, typos.
7. COMMIT             — Format ci-dessous
8. CONTINUE           — Passe au commit suivant
```

### Format de Commit

```
type(scope): description

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

Types: `feat`, `fix`, `refactor`, `test` | Scopes: `mcp`, `media`, `store`, `event`, `runtime`

## Quality Gates (entre chaque Phase)

Apres Phase A (commits 1-3), Phase B (4-9), Phase C (10-12), Phase D (13-16) :

```bash
cargo test --lib           # TOUS les tests passent
cargo clippy --lib         # Zero nouveau warning
```

Si un gate fail → FIX avant de continuer. Ne passe JAMAIS a la phase suivante avec un test qui fail.

## Les 16 Commits (resume)

### Phase A: Type Foundation
1. `refactor(mcp): ContentBlock struct → enum` — 5 struct variants, serde tag, 27+ call sites
2. `fix(mcp): extract all 5 content types from rmcp` — exhaustive match, MIME normalization
3. `feat(mcp): add ToolCallResult media helpers` — has_media(), images(), with_blob()

### Phase B: Media Module
4. `feat(media): error types NIKA-251..259` — thiserror + miette::Diagnostic
5. `feat(media): types MediaRef, MediaType, MediaBudget` — blake3 prefix, hash-only CAS
6. `feat(media): MIME detection with declare-then-verify` — infer + mime_guess + security
7. `feat(media): async CAS store with io::atomic` — write_fail, verify threshold, mmap
8. `feat(media): MediaProcessor pipeline` — size guard, empty rejection, budget
9. `feat(media): register module + deps` — Cargo.toml, lib.rs

### Phase C: Integration
10. `feat(store): TaskResult.media + RunContext.media_staging` — DashMap side-channel
11. `feat(event): add 4 media events (32 → 36)` — after ArtifactFailed
12. `feat(runtime): wire MediaProcessor into invoke` — datastore param, not self

### Phase D: Tests
13. `test(mcp): ContentBlock enum + media helpers` — serde roundtrip all 5 variants
14. `test(media): MIME, CAS, processor, budget, proptest` — edge cases
15. `test(event): media events serde + variant count (36)` — guard test
16. `test(runtime): invoke → media pipeline integration` — mock MCP

## Skills a Utiliser

- **`/spn-rust:rust`** — avant d'ecrire du Rust, charge le skill Rust
- **`/spn-powers:test-driven-development`** — pour le cycle TDD
- **`/spn-powers:verification-before-completion`** — avant de dire "c'est fait"
- **`/spn-powers:git:commit`** — pour les commits propres
- **`/find-docs`** — si besoin de docs crate (blake3, infer, serde)

## Points Critiques (BUGS CORRIGES DANS LE PLAN)

Le plan v5.0 a deja corrige 70+ issues. Les plus importants :

1. **ContentBlock::Text { text: String }** — PAS Text(String). Serde internally tagged ne marche pas avec newtype(primitive).
2. **#[serde(rename = "mimeType")]** sur mime_type dans Image, Audio, ResourceLink
3. **datastore** est un PARAMETRE de run_invoke, pas self.datastore
4. **workspace_root** → std::env::current_dir() (TaskExecutor n'a pas ce field)
5. **MediaError** doit derive miette::Diagnostic (NikaError l'exige)
6. **MediaError::code()** retourne &'static str, pas u16
7. **io::atomic::write_fail** ne cree PAS les parent dirs → create_dir_all avant
8. **CAS filenames** = hash ONLY, pas d'extension (fix dedup .jpeg/.jpg)
9. **Hash prefix** = "blake3:af1349..." (pas juste "af1349...")
10. **make_task_result** puis .with_media() — PAS TaskResult::success() direct

## Deps a Ajouter (Cargo.toml)

```toml
# 2 genuinely new
blake3 = { version = "1.8", features = ["mmap"] }
infer = "0.19"

# 4 zero-cost promotes (already transitive)
base64 = "0.22"
mime_guess = "2.0"
mime = "0.3"
bytes = "1"
```

## Fichiers Cles (line numbers v0.30.8)

```
src/mcp/types.rs:367        ContentBlock struct (→ enum)
src/mcp/rmcp_adapter.rs:373 filter_map bottleneck (→ exhaustive match)
src/runtime/executor/verbs.rs:929  tool_result.text() (→ media processing)
src/runtime/executor/mod.rs:40     TaskExecutor fields
src/store/run_context.rs:40        TaskResult (→ add media field)
src/event/log.rs:142               EventKind (→ add 4 media events)
src/error.rs:481                   NIKA-250 (251-259 FREE)
src/io/atomic.rs:172               write_fail (CAS dedup)
src/runtime/runner.rs:809          make_task_result (→ splice with_media)
src/lib.rs:44                      pub mod mcp (→ add pub mod media after)
src/main.rs:148                    Commands enum
```

## Verification Finale (apres les 16 commits)

```bash
cargo check                    # Clean
cargo test --lib               # 5,219+ pass, 0 fail
cargo clippy --lib             # Max 2 warnings
# Puis verifier manuellement :
# - ContentBlock serde roundtrip 5 variants
# - CAS dedup (same content = same file)
# - Empty base64 → rejected
# - 101MB base64 → rejected
# - Hash prefix "blake3:..."
# - EventKind count = 36
```

## GO

Commence par lire le plan PR1 complet, puis attaque Phase A, Commit 1.
