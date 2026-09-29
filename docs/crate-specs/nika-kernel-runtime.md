# Crate spec — `nika-kernel-runtime`

| | |
|---|---|
| Status | Admitted (kernel 4-way split · census 2026-06-10) |
| Layer | L0.5 (TRAITS ONLY · zero I/O · zero impl) |
| Design | Runtime sibling — tool execution · agent loop · checkpoint |
| LOC budget | ≤4,000 src |
| License | `AGPL-3.0-or-later` |

## 1. Purpose

The runtime sibling of the 4-way kernel split
(`docs/architecture/kernel-split-census-2026-06-10.md`) · 3 traits
(`ToolExecutor` + `ToolExecute` + `ToolBatch`) + the agent-loop and
checkpoint TYPE surfaces (`agent` · `checkpoint` · 0 traits today ·
the L2/L3 trait growth bucket).

Modules flattened from `nika-kernel/src/runtime/` + root
`checkpoint.rs`. Depends on `nika-kernel-core` (DAG discipline · no
internal imports today · base types tomorrow).

## 2. Gate exemptions (documented per Rule 2)

Same as `nika-kernel-core` — mechanical move of 12-gate-admitted code ·
MUTATION inherited · BENCHMARKS/CANARY/PARITY N/A.

## 3. Invariants

- Depends ONLY on `nika-kernel-core` (+ external workspace deps).
- Agent/checkpoint trait growth (L2 verb admission) lands HERE.
- A failed `ToolResult` carries its tool's own `ToolErrorMeta` (spec code,
  retry class) and, since 2026-09-28, optional typed `ToolErrorDetails`:
  the final HTTP `status_code` the call received and the `accepted` set it
  declared. The type is closed: no free-form data or provider text crosses
  the seam, empty details stay absent, and `to_value()` projects only the
  facts present under the spec's names (the error object's `details`).
