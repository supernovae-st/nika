# Crate spec — `nika-registry-client`

| | |
|---|---|
| Status | **ADMITTED 2026-07-12** — Gate 1 authored at the split (a descent, not a greenfield: the resolver landed test-first inside `nika-cli`'s registry seam and descended under the size cap — issue #512; this spec joins one arc later, closing the vector the descent left open). |
| Layer | L2 — domain service (registry refs → verified local artifacts) |
| Design | ONE law, one crate: a `registry:` ref resolves to a digest-pinned, oracle-verified cache file or it refuses with a teaching error — never a half-trusted artifact. `is_registry_ref` owns the ref grammar; `RegistryClient<H>` is generic over the kernel HTTP seam (the whole network path mock-proven; the only production transport is `nika-http` — rustls · SSRF floor); `Resolved::describe` speaks the receipt the CLI prints. Verification is structural: sha256 + the spec oracle re-check ride INSIDE resolution (D-2026-07-06 · every registry entry CI-re-proven). |
| LOC budget | the 15k-prod workspace ratchet governs (≤1,500/file · ≤100/fn as everywhere) — admitted under ~1k prod src; headroom deliberate (~950 LOC at the descent) |
| File cap | ≤1,500 LOC each |
| Function cap | ≤100 lines each |
| Crate version | tracks workspace (`0.99.0` at admission) |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |
| Publish | `false` — internal L2 service crate, same stance as `nika-verb-*` |
| Extraction source | `crates/nika-cli/src/registry_args.rs`'s resolution half (git-mv, history preserved). `nika-cli` keeps the thin arg-routing seam (`registry_args::{check_verb, registry_then_run}`) — the L4 edge the descent killed was `nika-cli` reaching the network directly. Per D-2026-07-09-N1 the descent is ONE architectural unit in TWO members. Precedent: `nika-models` · `nika-onboard` (2026-07-12) · `nika-display` (2026-07-10), the same wall. |
| NIKA codes | **none of its own** — refusals surface through `RegistryError` (`code()` maps onto the existing NIKA-REG-* family owned upstream; the one-voice model stays with the error crate) |

---

## 1. Purpose

`nika-registry-client` is the **registry resolution unit**: everything
between a `registry:` ref in a CLI argument and a verified artifact on
disk the run can trust. Ref grammar, cache layout, fetch, digest pin,
oracle re-verification — one place, injected transport, no CLI types.

## 2. Why a crate (and why now)

The resolver crossed `nika-cli`'s 15k prod wall the day the registry
grew resume + verification (#512) — and an L4 binary crate talking to
the network directly was the layering smell. As an L2 service behind
the kernel HTTP traits it is mock-proven end to end, and the CLI keeps
only the seam that names files.
