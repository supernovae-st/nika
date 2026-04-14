# Nika Diamond — Architecture Decision Records

ADRs document **why** we chose a specific architectural path. They are the
public, permanent memory of non-obvious decisions — so that six months from
now, a contributor (or a future version of you) can reconstruct the reasoning
without having to re-derive it.

## When to write an ADR

Write an ADR for any decision that matches ≥2 of:

- Crosses a **crate or layer boundary** (affects >1 crate, or defines a layer contract)
- Introduces a **new invariant** that downstream code must respect
- **Locks a public API** (trait, struct, enum that other crates depend on)
- Makes a **non-reversible** choice (can't be undone without breaking users)
- Trades off a **quality attribute** against another (perf vs correctness, simplicity vs flexibility)
- Replaces or supersedes a prior ADR

Do **not** write an ADR for:

- Single-file refactors
- Bug fixes (commit message suffices)
- Dependency bumps
- Renames without behavior change
- Style / formatting decisions

## How to write one

1. Pick the next sequential number (`ls docs/adr/ | grep -E '^adr-[0-9]{3}'`)
2. Copy `TEMPLATE.md` to `adr-NNN-<short-kebab-title>.md`
3. Fill the sections. Be specific. Prefer grep-verified evidence over narrative.
4. Set Status to `Proposed` while discussing, `Accepted` once committed
5. Commit with message: `docs(adr): ADR-NNN add <title>` (co-author Nika 🦋)
6. If the ADR supersedes a prior one, update the prior ADR's status to
   `Superseded by ADR-NNN` in the same commit

## Enforcement

- `scripts/ci/check-adr-coverage.sh` — hygiene check: every admitted workspace
  member should be mentioned in at least one ADR (warn-only for now).
- Future: integrate into the `crate-admit` skill as a soft-gate.

## Index (updated on each new ADR)

| # | Title | Status | Date |
|---|-------|--------|------|
| [ADR-001](adr-001-diamond-orphan-branch.md) | Diamond orphan branch rewrite | Accepted | 2026-04-13 |
| [ADR-002](adr-002-forever-v0x.md) | Forever v0.x release model | Accepted | 2026-04-13 |
| [ADR-003](adr-003-12-gate-admission.md) | 12-gate crate admission protocol | Accepted | 2026-04-13 |
| [ADR-004](adr-004-context-window-sized-crates.md) | Context-window-sized crate architecture (≤15k LOC, cap 100) | Accepted | 2026-04-13 |
| [ADR-005](adr-005-error-hierarchy.md) | Trait-based error hierarchy (`NikaErrorCode` + `Box<dyn>`) | Accepted | 2026-04-13 |
| [ADR-006](adr-006-layered-kernel-isp-traits.md) | Layered architecture + kernel ISP atomic traits + `trait_variant` | Accepted | 2026-04-13 |
| [ADR-007](adr-007-forward-compat-invariants.md) | Forward-compat invariants (`#[non_exhaustive]` + `::new()` + pre-planted hooks) | Accepted | 2026-04-13 |
| [ADR-008](adr-008-toml-driven-catalog.md) | TOML-driven catalog with build-time codegen | Accepted | 2026-04-14 |
| [ADR-009](adr-009-adr-process.md) | ADR process + hook discipline (meta) | Accepted | 2026-04-14 |
| [ADR-010](adr-010-miette-diagnostic-layer.md) | miette as the L4 diagnostic presentation layer | Accepted | 2026-04-14 |
| [ADR-011](adr-011-cargo-xtask.md) | `cargo xtask` as canonical automation surface (spec) | Accepted | 2026-04-14 |
| [ADR-012](adr-012-typestate-runtime.md) | Typestate for `nika-runtime` workflow lifecycle (spec) | Accepted | 2026-04-14 |
| [ADR-013](adr-013-loom-concurrency-verification.md) | Loom-based concurrency verification (spec) | Accepted | 2026-04-14 |
| [ADR-014](adr-014-sealed-kernel-traits.md) | Sealed kernel traits with explicit adapter registration | Accepted | 2026-04-14 |
| [ADR-015](adr-015-expect-test-inline-snapshots.md) | `expect-test` for inline snapshot assertions on rendered output | Accepted | 2026-04-15 |

## Pre-Diamond ADRs (legacy reference)

Nika v0.1 → v0.27 had 8 ADRs that were superseded by the Diamond big-bang
rewrite. They are archived at:

```
supernovae-hq (private monorepo) → archive/nika-v0.79/adr/
```

Read them for historical rationale on the 5-semantic-verb DSL, YAML-first,
MCP-only principles — concepts retained conceptually but re-implemented
from scratch in Diamond.

## Further reading

- Michael Nygard, [Documenting Architecture Decisions](https://cognitect.com/blog/2011/11/15/documenting-architecture-decisions) (2011)
- ThoughtWorks [Lightweight Architecture Decision Records](https://www.thoughtworks.com/insights/blog/architecture-decision-records)
- [adr.github.io](https://adr.github.io/) — community ADR patterns
