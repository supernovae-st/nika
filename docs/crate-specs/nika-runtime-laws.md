# nika-runtime-laws — crate spec

| Field | Value |
|---|---|
| Status | **ADMITTED member** of the `nika-runtime` unit (ADR-127 · the size-cap member split · D-2026-07-09-N1: one architectural unit in two workspace members). Never a new unit. |
| Layer | **L3 — runtime** (the same row as `nika-runtime`) · `publish = false` · one public surface re-exported by the operator crate at every historical path. |
| Sub-tier | L3-laws — what a run obeys before and after it executes; nothing here dispatches a task or folds a definition. |
| Design | Ten modules, one law each: `errors` (the one-voice `RuntimeError`) · `contract` (the typed `outputs:` contract) · `compat_record` (the public record mirror) · `origins` (input origins) · `identity` (the engine identity + the build-support pins) · `integrity` (the record integrity law · `ValueTaint`) · `secret` (the secret resolver seam · the redacting sink · the payload field list) · `sandbox_select` (the sandbox verdict for a command) · `witness` · `stamp` (the event stamp seams) · `resume_fields` (the resume projection's payload field names). |
| LOC budget | ≤15k crate · ≤1500/file · ≤100/fn (Diamond caps) — the descent leaves `nika-runtime` at 13 234 lines (1 766 below the wall) and this member ≈ 1.8k. |
| IMPL | live · `scripts/crate-metrics.sh nika-runtime-laws` |
| Crate version | tracks workspace · License `AGPL-3.0-or-later` · Edition 2024 · Publish `false` |
| ADRs | **ADR-127 (this member)** · ADR-110 (the member-split precedent) · ADR-022 / ADR-024 (the size-cap law) |
| Error range | the runtime's (`RuntimeError` lives here and is re-exported by `nika-runtime` unchanged) |
| Reference | the one-door program's wave 7 (the runtime at 14 999 of 15 000 lines) |

---

## What it must NOT own

The wave engine · dispatch · settle · recover · the pause and approval plane · the resume projection (the definition fold · `definition_value` and its helpers) · the boot trust judgement (`trust`) · the semantic IR (`proof::ir`) · the composition root — everything that executes or folds stays in `nika-runtime`.

## The tests that admit it

- every historical path holds: `nika_runtime::{RuntimeError, TaskRecord, TaskStatus, TerminalCause, InputOrigin, input_origins, WorkflowSecretResolver, identity, sandbox_select, resume::fields, EventSink, Stamper, DeterministicStamper, SystemStamper, VecSink}` compile and behave as before (the consumers' batteries: `nika-cli` · `nika-cli-host` · `nika-service-execution` · `nika-serve` · `nika-session` · `nika-dap`);
- the moved modules' own tests run in the member (`cargo test -p nika-runtime-laws --lib`);
- `nika-runtime`'s battery and its integration gates (`budget_gate` · `cancel_gate`) are unchanged;
- the crate-size vector is GREEN for both members; the layering check refuses no edge (L3 → L0..L2 only).

## Boundaries (the seams the operator crate reaches)

`TaskContract{of, lowered, check_fit}` · `decode_bytes` · `ValueTaint{of_task, bare, label}` · `task_integrity` · `scrub_outputs` · `RedactingSink` · `REDACTED` · `resolve_secrets` · `SandboxDecision` · `SandboxVerdict` · `select_command_sandbox` · `PermitWitness` · `PermitDecision` — `pub` here, `pub(crate) use` in `nika-runtime`.
