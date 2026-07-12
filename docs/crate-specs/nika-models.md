# Crate spec — `nika-models`

| | |
|---|---|
| Status | **ADMITTED 2026-07-12** — Gate 1 authored at the split (a descent, not a greenfield: the unit landed test-first in `nika-cli` the same arc — issue #146 — and descended the day it was born). |
| Layer | L4 — interface crate (the local-models unit: store · Hub acquisition · sidecar launch glue) |
| Design | ONE law, one crate: the canonical models dir (`~/.nika/models/<owner>/<repo>/`). `store` owns the dir (by-id resolution for `serve --model` · `list` · `rm` · the `owner/repo[:QUANT]` grammar); `pull` acquires from the Hugging Face Hub over the injected kernel http seam (tree metadata FIRST — sizes before bytes · `Q4_K_M` default quant · ≥2 GiB confirm · `.part` + `Range:` resume · `HF_TOKEN` for gated repos · `tokenizer.json` rides along); `serve` is the candle sidecar boot glue (ADR-091/093 · feature-gated `local-infer`). The downloader and the resolver share the root by construction — the brouillon-era pull/load two-dir mismatch cannot re-happen. Outcomes are plain strings (`Ok` receipt · `Err` teaching refusal); the parent's thin adapters own the exit contract. |
| LOC budget | the 15k-prod workspace ratchet governs (≤1,500/file · ≤100/fn as everywhere) — admitted at ~1.5k prod src; the parent `nika-cli` returns under the wall it crossed at 16,298 |
| File cap | ≤1,500 LOC each (max at admission: `pull.rs`) |
| Function cap | ≤100 lines each |
| Crate version | tracks workspace (`0.99.0` at admission) |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |
| Publish | `false` — internal L4 interface crate, same stance as `nika-cli` |
| Extraction source | `crates/nika-cli/src/verbs/model/{store,pull,mod→serve}.rs` (git-mv, history preserved). `nika-cli` keeps thin `VerbOutput` adapters at the OLD public paths (`verbs::model::{serve, store::{resolve_serve_model,list,rm}, pull::run}` + `DEFAULT_PORT`) — the public-api baseline lines stay true verbatim. Per D-2026-07-09-N1 the descent is ONE architectural unit in TWO members — this crate spec names the parentage; the unit stays `nika-cli`'s. Precedent: `nika-onboard` (2026-07-12) · `nika-display` (2026-07-10) · `nika-dap` (2026-07-09), the same wall. |
| NIKA codes | **none** — an acquisition/store surface: refusals are CLI teaching texts (exit `3` via the parent's adapter), never workflow-error codes (the one-voice model stays upstream) |

---

## 1. Purpose

`nika-models` is the **local-models unit**: everything between a
Hugging Face repo id and a GGUF the candle sidecar can serve. The store
logic is pure disk (temp-root tested); the Hub client is generic over
the kernel `HttpGetDyn`/`HttpPostDyn` traits (the whole network path is
mock-proven — resume, restart, short-stream, auth); the only production
transport is `nika-http` (rustls · SSRF floor · per-hop redirect
vetting — no second HTTP stack, the reason `hf-hub` was refused).

## 2. Why a crate (and why now)

`nika-cli` sat at **14,958/15,000 prod LOC** when issue #146 landed its
+1,340: the wall the `nika-onboard`/`nika-display`/`nika-dap` descents
each hit. The model unit was the newest coherent surface and the least
entangled cut: three modules, one law, zero inverse edges (the parent
consumes; nothing here reads the parent). The `local-infer`/`metal`
features forward through the parent unchanged.

## 3. Public API (the adapter's consumption surface)

- `store::{models_root, resolve_serve_model, list, rm, human_size}`
- `pull::run`
- `serve::{serve, DEFAULT_PORT}`
- `ModelsProbe` + `models_probe()` (the `nika doctor` models row)

Everything else is `pub(crate)` — the grammar, the walk, the quant
parse, the Puller and its refusal builders stay internal.

## 4. Tests

38 lib tests at admission: the ref grammar (+12 malformed-teach rows) ·
quant parsing · by-id resolution (passthrough / single / quant-pick /
ambiguity / miss-teaches-list / path-shaped flow-through per the
bin_smoke-pinned #482 contract) · list/rm on temp roots (repo rm ·
quant rm · last-gguf sweep) · tree parse · GGUF choice (default / tag /
menu refusals) · the confirm gate · the streamed download over an
injected seam double (happy · resume-`Range` · 200-restart ·
short-stream-keeps-part · 401/403 teach · bearer on/off). The serve
boot path keeps its per-axis tests (`local-infer` on/off).
