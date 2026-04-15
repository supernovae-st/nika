---
id: ADR-010
title: "miette as the L4 diagnostic presentation layer"
status: accepted
date: "2026-04-14"
phase: "Phase D (post-Session 2a retrofit)"
deciders: ["@ThibautMelen"]
tags: ["diagnostics", "miette", "error-rendering", "l4"]
affects_crates: ["nika-error"]
affects_layers: ["L0", "L4"]
supersedes: []
superseded_by: []
related: ["ADR-005", "ADR-007", "ADR-015"]
requires: ["ADR-005"]
enables: ["ADR-015"]
amends: []
fci: []
inv: []
shadow_zones: []
nika_codes: []
timeline: "v0.80.0, Phase D"
follow_ups: ["test-miette-render.rs golden test when nika-cli lands"]
---

# ADR-010: miette as the L4 diagnostic presentation layer

## Context

ADR-005 established `NikaErrorCode` trait + `Box<dyn>` unification at API boundaries — correct for routing, retry (`is_transient()`), and fingerprinting. It does NOT, however, solve *rendering*: when a user runs `nika check workflow.nika.yaml` and a template parse fails at line 14, they want source-span highlighting, a `--explain NIKA-384`, and coloured severity — not a single-line `Error: NIKA-384: ...` string.

The Rust 2026 ecosystem has converged on **miette** for this role (ruff, uv, cargo-dist, rye all use it or miette-class renderers). `miette 7.6` is already pinned in `[workspace.dependencies]` and `NikaError` already carries `impl miette::Diagnostic`. This ADR locks the pattern and scope so downstream L4 crates inherit it consistently.

## Decision

- `nika-error` **keeps** the `NikaErrorCode` trait as the structural / routing contract (ADR-005 unchanged).
- `nika-error` **additionally** implements `miette::Diagnostic` for `NikaError` — this is the rendering contract.
- L4 crates (`nika-cli`, `nika-lsp`, `nika-serve`, `nika-catalog-verify`) render all user-visible errors via `miette::GraphicalReportHandler` (or equivalent).
- Source spans, severity, help strings, related diagnostics — all flow through miette's `Diagnostic` API.
- `--explain NIKA-NNN` CLI subcommand uses the registered help string per `NikaCode`.
- Library crates (L0, L0.5, L1, L2, L3) **do NOT** depend on miette at runtime — their errors implement only `NikaErrorCode`; miette is purely a presentation concern.

## Consequences

### Positive
- User-facing errors are best-in-class (source underlines, colour, hyperlinked error codes).
- Library crates stay lean — no miette surface in kernel/catalog/error public types beyond the optional `Diagnostic` impl on `NikaError`.
- `--explain` works by registry lookup — zero per-error maintenance cost.

### Negative
- L0 `nika-error` carries a miette dep (already paid — feature-gated via `diagnostic`).
- Two contracts per error type (`NikaErrorCode` + `Diagnostic`). Mitigated by derive helpers.

### Neutral
- If the community pivots to ariadne or a successor, we swap in `nika-error` only — library crates untouched.

## Evidence

- `tools/nika-error/Cargo.toml:14` — `miette = { workspace = true, features = ["fancy-no-backtrace"] }`
- `tools/nika-error/src/nika_error.rs:97` — `impl miette::Diagnostic for NikaError`
- `tools/nika-error/src/codes.rs:128` — per-code help strings
- ADR-005 — the structural error hierarchy this extends

## Alternatives considered

### Alt A — `anyhow` / `eyre` at CLI
Rejected. Both produce flat strings — no spans, no `--explain`.

### Alt B — Custom renderer
Rejected. Reinvents miette. Not valuable.

### Alt C — `error-stack` (HASH) with attachments
Watching. Good for structured observability context but not focused on user-facing rendering. Revisit if miette stagnates.

## Related

- ADR-005 — trait-based error hierarchy (structural contract this rendering layer wraps)
- ADR-007 — forward-compat invariants (`NikaError` is `#[non_exhaustive]`)
- ADR-015 — `expect-test` inline snapshots (regression coverage for the rendering this ADR locks)

## Notes

When `nika-cli` is admitted, add a `test-miette-render.rs` golden test that pins the rendered output of 3 representative error codes. Prevents accidental UX regression.
