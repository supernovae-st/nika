# Crate spec — `nika-kernel-plugin`

| | |
|---|---|
| Status | Admitted (kernel 4-way split · census 2026-06-10) |
| Layer | L0.5 (TRAITS ONLY · zero I/O · zero impl) |
| Design | Plugin sibling — WASM host + sandbox (ADR-020) |
| LOC budget | ≤3,000 src |
| License | `AGPL-3.0-or-later` |

## 1. Purpose

The plugin sibling of the 4-way kernel split
(`docs/architecture/kernel-split-census-2026-06-10.md`) · 6 traits ·
`wasm` (WasmPluginHost · WasmPluginLifecycle · PluginEnv · PluginFs ·
PluginHttp) + `sandbox` (Sandbox) · all OPEN (unsealed) per ADR-020
community-backend posture.

Modules flattened from `nika-kernel/src/plugin/`. Depends on
`nika-kernel-core` (`cancel::CancelCtx` in host lifecycle).

## 2. Gate exemptions (documented per Rule 2)

Same as `nika-kernel-core` — mechanical move of 12-gate-admitted code ·
MUTATION inherited · BENCHMARKS/CANARY/PARITY N/A.

## 3. Invariants

- Depends ONLY on `nika-kernel-core` (+ external workspace deps).
- Plugin-boundary trait growth lands HERE (ADR-020 addenda).
