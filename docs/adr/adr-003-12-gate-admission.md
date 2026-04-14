# ADR-003: 12-gate crate admission protocol

**Status:** Accepted
**Date:** 2026-04-13
**Phase:** Phase 0 (process)
**Deciders:** @ThibautMelen + 16-agent Rust Council (post-audit)

## Context

The Diamond rewrite (ADR-001) is valuable only if the new crates don't accumulate the same debt as legacy (1,276 unwraps, 47 oversized files, silent rot). Code review alone failed in the legacy codebase — reviews happened, yet a P0 audit discovered 29 broken MCP aliases that had lived for months. The enforcement must be **mechanical**, not social.

Five crates have been admitted to date (`nika-error`, `nika-catalog`, `nika-catalog-verify`, `nika-kernel`, `nika-kernel-mock`). Each carries the commit body listing all 12 gate results. The process works under load.

## Decision

Every crate passes **12 sequential gates** before its path is added to `Cargo.toml` members:

| # | Gate | Enforces |
|---|------|----------|
| 1 | SPEC doc | crate purpose + API surface documented in `docs/crate-specs/<name>.md` |
| 2 | TDD red/green | failing tests written before implementation |
| 3 | IMPL compiles | workspace builds with the new crate added |
| 4 | Zero Clippy warnings | `cargo clippy -- -D warnings` clean |
| 5 | Mutation ≥90% | `cargo-mutants` kills ≥90% of mutants (or documented exemption) |
| 6 | Property tests | `proptest` on security/parser/boundary code |
| 7 | Benchmarks | `criterion` on hot paths if applicable |
| 8 | Zero `cargo doc` warnings | `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` clean |
| 9 | Canary E2E | 1 real workflow exercises the crate end-to-end (or exempted for infra crates) |
| 10 | Golden parity test | output matches legacy where applicable |
| 11 | 3-agent review swarm | parallel review by fresh agents, all findings resolved |
| 12 | Atomic commit | single commit with full admission body |

Exemptions (e.g., Gate 9 N/A for `nika-error`) require written justification in the crate spec. No gate skipping without explicit record.

Admission commit body format is mandatory (see `.claude/rules/commit-granularity.md`).

## Consequences

### Positive
- Every admitted crate starts at a correctness floor legacy never achieved: 0 unwraps, 0 clippy warnings, ≥90% mutation killed, documented public API.
- Regression detection front-loaded, not retrofitted.
- Proven on 5 crates: `nika-error` 100% mutation, `nika-kernel` 100% mutation, `nika-kernel-mock` 95.7%.
- Bisect-friendly history: one crate = one atomic commit.

### Negative
- Admission is expensive per crate (a full session per admission in some cases).
- Gate 5 (`cargo-mutants`) is slow on larger crates — up to ~20 minutes on `nika-catalog`.
- Gate 9 (E2E) is N/A for infrastructure crates, requiring explicit documented exemptions.
- Velocity-over-Safety tension: an under-pressure solo dev might rationalize skipping a gate. The process must be socially enforced via memory + reminders.

### Neutral
- Agent review (Gate 11) requires ~3 parallel sub-agents per admission — acceptable cost given error detection rate.

## Evidence

- `.claude/CLAUDE.md` — 12-gates summary
- `.claude/rules/commit-granularity.md` — mandatory commit body format
- `CHANGELOG.md` lines 196–216 — `nika-kernel` admission with all 12 gates confirmed, commit `ef8804371`
- `scripts/ci/` — 10 check scripts (one per Gate approximately): `check-clippy.sh`, `check-crate-size.sh`, `check-dead-code.sh`, `check-expect.sh`, `check-fn-length.sh`, `check-loc-limits.sh`, `check-no-default-features.sh`, `check-tests.sh`, `check-unwrap.sh`, `check-adr-coverage.sh` (see ADR-009)
- `.github/workflows/diamond-ci.yml` — CI matrix running the gates
- memory: `crate-admit` skill + `gate-check` skill — automation entrypoints

## Alternatives considered

### Alt A — Code review only
Rejected empirically — legacy had reviews, still accumulated 1,276 unwraps.

### Alt B — 8-gate protocol
Earlier draft. Rejected because mutation coverage, property tests, and the agent review swarm each caught real issues in admission sessions. Downgrading to 8 would re-open those failure modes.

### Alt C — Gates as PR-level CI only (not admission commit)
Rejected because CI failure on a PR that has already touched 6 crates is expensive to unwind. Gates run *before* admission commit lands on nika-diamond.

## Related

- ADR-001 — orphan rewrite (the reason this process exists)
- ADR-004 — context-window sizing (why we have ~40 admissions to run)
- ADR-009 — ADR process (Gate 12 invokes ADR check)
- `crate-admit` skill — guided admission workflow
- `gate-check` skill — run gates against an in-flight crate

## Notes

If a future gate proves redundant (e.g., Gate 10 golden parity becomes meaningless post-v0.95), revisit and supersede. Gate count is not sacred — enforcement floor is.
