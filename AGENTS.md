# AGENTS — Nika Diamond

Guidance for AI coding agents (Claude Code, Cursor, Aider, etc.) working in
this repository. This is the public AGENTS counterpart to the
`.claude/` rules — a single page that any agent can read on entry.

## Repository snapshot

This page embeds **no volatile counts** (crate / test / vector numbers drift
every session — a stale number here would mislead a Cursor/Codex/Aider session
exactly as much as a Claude one). For the live, canonical state:

```bash
bash scripts/refresh-status.sh        # the single source of truth
```

The regenerated block is quoted verbatim in `ROADMAP.md` and `.claude/CLAUDE.md`
(kept in sync by hygiene vector `check-status-claims-sync.sh`). Stable facts:

- **Branch topology**: `main` is the production Diamond orphan branch (renamed
  2026-05-06 from `nika-diamond`). `brouillon` is the read-only legacy v0.79.3
  reference (zero shared history). See `.claude/rules/diamond-discipline.md`.
- **Workspace**: `v0.80.0`, forever-v0.x, 42-crate target (cap 100).

## What to read first

1. `README.md` — user-facing overview + current state.
2. `DIAMOND.md` — the Diamond rewrite philosophy.
3. `docs/architecture/forward-compat-invariants.md` — 8 patterns, 10 rules, non-negotiable.
4. `docs/architecture/crate-layer-registry.md` — L0 to L4 layer discipline.
5. `ROADMAP.md` — forever-v0.x plan, v0.81 seams, v0.90 milestones.
6. `.claude/CLAUDE.md` + `.claude/rules/` — project-specific enforcement.

## Hard rules (non-negotiable)

- `brouillon` branch is **read-only** (legacy v0.79.3 reference). Access legacy code via `git show brouillon:path` only. Never `git checkout brouillon`.
- No `.unwrap()` or `.expect(` in `src/` (use `?` propagation). Enforced by clippy + hygiene.
- No `#[allow(dead_code)]` (delete or make `pub(crate)`).
- Files greater than 1,500 LOC must be split. Crates greater than 15,000 LOC are rejected.
- Every public error enum is `#[non_exhaustive]` from day one.
- Every I/O is behind a kernel trait (ADR-006, ADR-014).
- Commits are atomic: one logical change per commit. Co-authored by `Nika <nika@supernovae.studio>` (never "Claude").
- No `--no-verify`, no `git add -A` / `git add .`, no `cargo test --test` (use `--lib` to avoid macOS Keychain popups).

## Crate admission: 12 gates

No crate enters `Cargo.toml` `members = [...]` without all 12 gates passing in the same PR. Summary:

1. SPEC — `docs/crate-specs/nika-X.md`
2. TDD — tests first, red before green
3. IMPL — minimal, compiles, tests pass
4. CLIPPY — 0 warnings
5. MUTATION — greater than or equal to 90% killed
6. PROPERTY — proptest for parsers, encoders, security paths
7. BENCHMARKS — `benches/` if hot path
8. DOCS — 0 `cargo doc` warnings, all pub items documented
9. CANARY E2E — `tests/canary-X.nika.yaml`
10. PARITY LEGACY — golden test vs `git show brouillon:...` output
11. REVIEW SWARM — 3-agent parallel review
12. ATOMIC COMMIT — 1 commit, `feat(nika-X): admit to workspace — all 12 gates passed`

Full detail: `CONTRIBUTING.md` + `.claude/rules/diamond-discipline.md`.

## Tooling agents should run

```bash
cargo test --workspace --lib              # always --lib, avoids Keychain
cargo clippy --all-targets -- -D warnings
cargo fmt --check
bash scripts/hygiene/check-all.sh         # engine-internal hygiene vectors (incl.
                                          # supply-chain cargo-deny, ADR-081 guard
                                          # presence, error one-voice)
bash scripts/refresh-status.sh            # regenerate canonical status block
```

Admission-tier gates (slow · run per crate when admitting, not in the suite):

```bash
bash scripts/ci/check-mutation-floor.sh <crate>   # real Gate 5 (cargo-mutants ≥90%)
cargo deny check                                   # full supply-chain policy
```

## Code intelligence

Local MCP sidecar is provided by **olympus**. Configured at user scope
in `~/.claude.json`, binary at `olympus/src-tauri/binaries/olympus-*`.
Exposes three tools: `olympus_query`, `olympus_impact`, `olympus_context`.

Never commit `nika/engine/.mcp.json` — this engine is a PUBLIC submodule.

If olympus is unavailable, fall back to `Grep` + `Glob` + `cargo clippy`.

## Scope boundaries

This repository is **PUBLIC** (supernovae-st/nika). Never commit:

- Brand bibles, launch plans, market research, competitive intel.
- `.env*` files with production values, API tokens.
- References to private monorepo paths (see monorepo hygiene vector 1).

Privacy is enforced by the monorepo hygiene boundary (vector 1).

## Getting help

- Bug reports, feature requests: GitHub Issues.
- Security reports: `nika@supernovae.studio` (see `SECURITY.md`).
- Architecture questions: read the ADRs at `docs/adr/` first.

Butterfly on the SuperNovae flag.
