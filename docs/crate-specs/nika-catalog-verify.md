# Crate spec — `nika-catalog-verify`

| | |
|---|---|
| Status | Phase 1 — catalog tooling (admitted 2026-04-14 by parallel session) |
| Layer | L2 — binary with I/O (network probes) |
| Design | CLI tool: probes npm/pypi/oci registries + remote MCP endpoints, reports drift |
| LOC budget | ≤1,500 src (actual ~600) |
| File cap | ≤1,500 LOC each |
| Function cap | ≤100 lines each |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |

---

## 1. Purpose

`nika-catalog-verify` is an **online verifier** for the static data in
`nika-catalog`. Where `nika-catalog` answers "what do we know?" in O(1) from
compile-time data, `nika-catalog-verify` answers "is what we know still true?"
by probing real package registries and MCP endpoints.

It is a binary (not a library), invoked from CI nightly or on-demand. Runs
parallel probes via `tokio` + `reqwest`, produces a JSON drift report, and
exits non-zero when drift is detected.

## 2. Responsibility boundary

- **IN scope**: network I/O against package registries, MCP remotes, health
  endpoints. Drift detection + reporting.
- **OUT of scope**: fixing drift (caller's job). Writing to the catalog
  (that's `nika-catalog`'s `build.rs`).

## 3. Public API surface

Binary only — no library exports expected. Entrypoint is `main()` in `main.rs`
with clap-parsed args (`--format json|table`, `--only mcp|npm|pypi`, etc.).

## 4. Dependencies

- `nika-catalog` (path) — source of truth to verify against
- `reqwest` (rustls) — HTTP probes
- `tokio` (rt-multi-thread) — concurrent probes
- `clap` — CLI args
- `tracing` — structured logs

## 5. Gates status at admission

Filled retroactively by parallel session that admitted this crate.
TODO: populate LOC, tests, mutation%, clippy, from CI artifact of commit
`a977e35b1`.

## 6. Exemptions

- Gate 5 (MUTATION ≥90%): binary crate with I/O, mutation testing on network
  code produces low-signal mutations. Exemption: target ≥75% for logic code
  only. Justification: network-bound mutations are tautological.
- Gate 10 (PARITY LEGACY): no legacy equivalent — this is new tooling.

## 7. Why admitted before nika-schema

Catalog drift has a higher real-world cost than schema-ast progress. Having
this tool running nightly gives us a truthful catalog as the registry evolves.
Decision taken by parallel session 2026-04-14.
