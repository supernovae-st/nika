# nika-runtime-laws — crate spec

| Field | Value |
|---|---|
| Status | **ADMITTED member** of the `nika-runtime` unit (ADR-127 · the size-cap member split · D-2026-07-09-N1: one architectural unit in two workspace members). Never a new unit. |
| Layer | **L3 — runtime** (the same row as `nika-runtime`) · `publish = false` · one public surface re-exported by the operator crate at every historical path. |
| Sub-tier | L3-laws — what a run obeys before and after it executes; nothing here dispatches a task or folds a definition. |
| Design | Existing law modules: `errors` (the one-voice `RuntimeError`) · `contract` (the typed `outputs:` contract) · `compat_record` (the public record mirror) · `origins` (input origins) · `identity` (the engine identity + the build-support pins) · `integrity` (the record integrity law · `ValueTaint`) · `secret` (the secret resolver seam · the redacting sink · the payload field list) · `sandbox_select` (the sandbox verdict for a command) · `witness` · `stamp` (the event stamp seams) · `resume_fields` (the resume projection's payload field names) · `retry` (pure backoff arithmetic) · `image_room` (received-image custody: the admitted project held by descriptor, one finite rooted room per store operation, dropped operations joined on demand) · `stack` (a nested run's same-thread stack mechanics, imported privately by the child call). |
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

### Received-image custody

`image_room::ImageRoom` is the `BlobStoreDyn` the runtime attaches to a seated
harness. `open` holds the admitted root through `nika_fs::OwnedDir` (refusing a
symlinked component) and writes nothing. Every operation clones that descriptor
into a fresh `RootedFs` with its own finite `EffectLedger` (one image of at most
`IMAGE_MAX_BYTES`, its sidecar and created directories), runs `FsBlobStore`
below `.nika/blobs`, then seals and drains before answering: no sealed ledger
outlives an operation, so a runtime runs any number of times. A dropped
operation is sealed synchronously in its lease's `Drop`, and its join (a pinned
future) is listed. A join always has exactly one owner: `drain_dropped` takes
the listed joins, polls them without holding the lock across a wait, and hands
the unfinished ones back if it is abandoned, so the next call resumes them
(`pending_drains` counts what is listed). The runtime drains at the end of each
wave and before `run` returns. It takes received bytes only, never a
peer-reported path. It dispatches nothing and composes nothing: seating stays in
`nika-runtime`. The edges are L3 → L1 (`nika-fs`, `nika-blob`).

### Native input capability

The shared engine identity advertises `inputsLiteral` for the CLI's bounded
`run --inputs-json -` producer. This additive token does not rename Serve's
`jobInputs` envelope capability or grant authority beyond declared inputs.

### Native compile capability

`supportedCapabilities` here is the NATIVE door's list; `nika-serve` projects
its own, so neither door can advertise a route only the other serves. The
additive `compile` token means exactly "`nika compile --json` speaks the
`compile_version` 1 foundation wire" (exact-skeleton CREATE, constant EDIT,
literal answers). It promises no authoring cognition beyond what that
document's provenance states, and it grants no authority.

### Terminal output projection and retry arithmetic

`secret::output_fields` serializes the resolved map through a capped writer:
whole compact JSON within `OUTPUTS_KEPT`, otherwise its exact byte count, or
a whole-map withheld marker if it cannot be encoded or represented. The
redacting sink examines terminal output JSON structurally: a secret key or
any changed value withholds the whole map instead of publishing rewritten
JSON. Absence, an empty object, truncation and withholding stay distinct.

`retry::{delay_ms, rand_unit}` owns the existing pure ramp, clamp and seeded
jitter arithmetic. Runtime keeps retry admission, attempts and injected-clock
sleep; moving the arithmetic grants no new execution authority.

### Nested-run stack mechanics

`stack::grown` is same-thread stack mechanics for the runtime's child call
(spec 14 · `NIKA-SEC-003` stays the only nesting limit). It creates, polls and
destroys the one borrowed, non-`Send` child future with at least a 4 MiB red
zone of native stack: in place when the thread has it, otherwise on an 8 MiB
`stacker` segment of the same thread, unmapped when the call returns. It
dispatches nothing, folds no definition, spawns nothing and offers no product
option; `nika-runtime` imports it privately (no re-export, no historical path).
The constants come from a measured unoptimized level (934 KiB on aarch64) and
the 8 MiB a CLI root run already has. The edge is L3 → `stacker` (MIT OR
Apache-2.0 · a safe API over `psm`).
