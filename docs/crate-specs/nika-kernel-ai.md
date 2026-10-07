# Crate spec — `nika-kernel-ai`

| | |
|---|---|
| Status | Admitted (kernel 4-way split · census 2026-06-10) |
| Layer | L0.5 (TRAITS ONLY · zero I/O · zero impl) |
| Design | AI sibling — provider · memory · vision · context · genai |
| LOC budget | ≤5,000 src |
| License | `AGPL-3.0-or-later` |

## 1. Purpose

The AI sibling of the 4-way kernel split
(`docs/architecture/kernel-split-census-2026-06-10.md`) · 14 traits ·
`provider` (Provider supertrait + 5 ISP sub-traits) · `memory` (6 ·
the Connectome contract surface) · `vision` (VisionModel) · `context`
(ContextCompressor) · `genai` (OTel GenAI attribute types).

Modules flattened from `nika-kernel/src/ai/` (`crate::ai::genai` →
`crate::genai`). Depends on `nika-kernel-core` (vision traits use
`io::screen`/`io::ocr` types · sealed bounds use `core::sealed`).

## 2. Gate exemptions (documented per Rule 2)

Same as `nika-kernel-core` — mechanical move of 12-gate-admitted code ·
MUTATION inherited · BENCHMARKS/CANARY/PARITY N/A.

## 3. Invariants

- Depends ONLY on `nika-kernel-core` (+ external workspace deps).
- New AI traits (L2 verb admission cohort) land HERE.

## Admission and usage completeness

`InferResponse.usage_completeness` defaults to `UsageCompleteness::Unknown`.
`Complete` means all tariff-relevant token counts and subset relations were
validated; `usage_reported` alone does not establish completeness or billing.
The additive nontransient `ProviderError::AdmissionDenied` maps to NIKA-339 and
means a subsequent paid request was locally refused. Transport cancellation
cannot establish a no-charge outcome; provider billing can remain unknown.

## Explicit reasoning evidence

`InferRequest.reasoning_effort` optionally names a closed `ReasoningEffort`
(`low`, `high`, `max`). Absence preserves the route default; an output-token
cap does not imply a level. `InferResponse.reasoning_wire` optionally records
the thinking and effort words read back from the serialized request body by
the adapter. It is transmitted-configuration evidence, not a server attestation
of internal reasoning effort. An absent read-back stays unobserved.

## Harness image observations

`HarnessImage` carries a peer-local tool-call id, optional received decoded bytes,
MIME, received size and SHA-256, and an optional **reported** saved path. The bytes
are transient `Bytes`; after the verb persists them, `stored_blob` contains the
local CAS metadata and the observer carries no raw data. `observation()` emits a
small `nika/harness-image-observation@1` receipt, never inline image bytes. This
is received-content evidence, not a receipt for the peer's filesystem, an image-
generation model identity, invoice or permission. `HarnessEvent::ImageActivityObserved`
precedes `ImageObserved`; `HarnessOutcome.images` retains the received facts.
Paths must not be opened, copied or fetched merely because the peer named them.

`HarnessImage::storage()` (and the observation's `storage` key) names what
happened to received bytes: `none` (path-only), `stored` (`stored_blob`),
`failed` (`storage_failure`, the store's refusal) or `unconfirmed` (received,
no store answer observed — pending or cancelled; a blob may or may not exist).

`HarnessOutcome.observed_model_source` says how `observed_model` was learned:
`ModelProvenance::{SessionConfig, ConfirmedSelection, AcceptedRequest}`
(`session_config` · `confirmed_selection` · `accepted_request`). None of these
is a response attestation: ACP prompt results name no model. `None` means the
source is unspecified and is never treated as attested.
