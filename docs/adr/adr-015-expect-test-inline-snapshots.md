---
id: ADR-015
title: "expect-test for inline snapshot assertions on rendered output"
status: accepted
date: "2026-04-15"
phase: "Phase D (post-Session 2a DX retrofit)"
deciders: ["@ThibautMelen"]
tags: ["testing", "snapshots", "expect-test", "dx"]
affects_crates: ["nika-error"]
affects_layers: ["L0"]
supersedes: []
superseded_by: []
related: ["ADR-005", "ADR-007", "ADR-010"]
requires: ["ADR-007", "ADR-010"]
enables: []
amends: []
fci: []
inv: []
shadow_zones: []
nika_codes: []
timeline: "v0.80.0, Phase D"
follow_ups: ["Use expect-test for --explain NIKA-NNN golden test in nika-cli"]
---

# ADR-015: `expect-test` for inline snapshot assertions on rendered output

## Context

ADR-010 locked `miette` as the L4 presentation layer for user-visible errors. The natural follow-on question is: *how do we test that rendering is stable?* Today we have two patterns living side-by-side:

- `assert_eq!(format!("{}", x), "literal")` — fine for one-line outputs but the literal is buried in code, no diff on failure, no easy refresh.
- `insta` — already pinned in `[workspace.dependencies]` (1.42, `yaml` feature). Excellent for medium/large structured payloads (full catalog snapshots, error-tree YAML, generated code dumps) but the snapshot file lives elsewhere, which adds a hop when reviewing PR diffs that change the rendered output.

The Rust 2026 ecosystem (rust-analyzer, ruff-equivalent crates, hashbrown, rustls test suites) overwhelmingly uses **`expect-test`** for the third gap: short-to-medium textual outputs that you want pinned *next to the assertion*, with a one-shot refresh via `UPDATE_EXPECT=1 cargo test`. It's a 200-LOC dep with zero macros at runtime and no proc-macro compile cost.

## Decision

- Add `expect-test = "1.5"` to `[workspace.dependencies]` (dev-only line).
- Crate `[dev-dependencies]` opt in via `expect-test = { workspace = true }`.
- **Use `expect-test` for**: inline assertions on rendered text where the snapshot is ≤50 lines and lives best next to the test (Display/Debug output, miette rendering, generated lints, single-error formatting).
- **Use `insta` for**: large structured payloads (≥50 lines, YAML/JSON serialization, full catalog dumps), redactions, multi-snapshot test cases, anything the reviewer wants to scan in a dedicated `*.snap` file.
- **Plain `assert_eq!` stays** for trivial one-liners with no risk of churn (e.g. `assert_eq!(NikaCode::FAILED_PRECONDITION.to_string(), "NIKA-001")`).
- Refresh workflow is `UPDATE_EXPECT=1 cargo test --workspace --lib` (mirrors `cargo insta accept`). Documented in the testing section of the agent self-serve docs (Phase E).

## Consequences

### Positive

- Closes the "buried literal" gap without introducing a new tool — `expect-test` is the de-facto rust-analyzer-tier choice.
- Reviewers see the exact expected text in the same diff hunk as the code change.
- Refresh is one env var, no extra binary install (unlike `cargo insta`).
- Zero runtime cost; zero proc-macro overhead at build time.
- Diff failures print colourised side-by-side, matches developer ergonomics.

### Negative

- One more dev-dep to keep in sync (offset by removing ad-hoc `assert!(s.contains(...))` clusters that grew because `assert_eq!` felt too rigid).
- Two snapshot crates in the tree (`insta` + `expect-test`) — small risk of "which one do I use" friction, mitigated by the rule above.
- `UPDATE_EXPECT=1` requires reviewer discipline (don't bless without reading the diff).

### Neutral

- If `expect-test` ever stagnates the swap surface is tiny — every call site is `expect![[…]].assert_eq(&actual)`, mechanically rewritable to `insta::assert_snapshot!` or plain `assert_eq!`.

## Evidence

- `Cargo.toml:90` — `expect-test = "1.5"` pinned in `[workspace.dependencies]` dev block.
- `crates/nika-error/src/nika_error.rs` — first use site: `display_format_pinned_inline` test pinning the `NIKA-002: not found: task foo` rendering.
- `crates/nika-error/Cargo.toml` — `[dev-dependencies] expect-test.workspace = true`.

## Alternatives considered

### Alt A — Keep only `insta`

Rejected. `insta` shines on large payloads but inflates the diff burden when the snapshot is a single line. Rust ecosystem leaders co-use both for that exact reason.

### Alt B — `goldenfile` / `k9` / `pretty_assertions` only

Rejected. `goldenfile` duplicates `insta`. `k9` brings snapshotting but with heavier macros and weaker bless workflow. `pretty_assertions` improves diff output but doesn't solve the buried-literal or refresh problem.

### Alt C — Roll our own

Rejected. The pattern is 200 LOC and three macros. Reinventing it costs more than the dep.

## Related

- ADR-010 — miette as the L4 presentation layer (this ADR pins regression coverage *of* that layer).
- ADR-007 — forward-compat invariants (snapshot tests are how we detect when `#[non_exhaustive]` types grow new variants that change rendered output).
- `crates/nika-catalog/src/data/snapshots/` — existing `insta` snapshots for the larger structured cases (pattern preserved).

## Notes

When `nika-cli` lands (Phase F+), the `--explain NIKA-NNN` golden test mentioned in ADR-010 §Notes should use `expect-test` for the per-code rendering and `insta` for the multi-code `--explain --all` dump. That split is the canonical example of the rule.
