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
