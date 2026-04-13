# Nika — `nika-diamond` branch

This branch is the fresh diamond-architecture rewrite of Nika started
**Constellation Phase 0** (2026-04-13).

It is an **orphan branch** — no shared history with `main`. The stable
`main` branch keeps shipping the existing engine while this branch is
constructed block-by-block against a strict 35-crate diamond target.

## Status

**Phase 0** — scaffolding. Workspace root, CI ratchets, lint policy.
Zero crates yet; first crates arrive in Phase 1 (copy the 9
EXTRACT-READY crates from `main`).

## Plan

Canonical in `memory/`:

- **CONSTELLATION_PLAN.md** — phases, done criteria, target
- **CRATE_CATALOG.md** — spec per crate
- **RUST_ENFORCEMENT.md** — CI ratchet stack + invariants

## Ratchets

Nine blocking CI scripts live in `scripts/ci/`. They use `git ls-files`
so untracked legacy content from `main` does not affect them:

| Script | Rule |
| --- | --- |
| `check-loc-limits.sh` | file ≤ 1500 LOC |
| `check-crate-size.sh` | crate ≤ 15k LOC |
| `check-fn-length.sh` | fn ≤ 100 lines |
| `check-unwrap.sh` | 0 `.unwrap()` in `src/` |
| `check-expect.sh` | 0 `.expect(` in `src/` |
| `check-dead-code.sh` | 0 `#[allow(dead_code)]` in `src/` |
| `check-clippy.sh` | `cargo clippy -D warnings` |
| `check-tests.sh` | `cargo test --workspace --lib` |
| `check-no-default-features.sh` | `cargo check --no-default-features` |

## License

AGPL-3.0-or-later. Commercial CLA planned pre-v1.0 (Grafana model).
