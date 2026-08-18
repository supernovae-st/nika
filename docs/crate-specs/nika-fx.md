# `nika-fx` — crate spec

| Field | Value |
|---|---|
| Layer | **L1** (pure-algo · zero I/O · consumed by `nika-builtin` L1.5) |
| Dependencies | **ZERO** runtime deps (the determinism + supply-chain contract) |
| LOC budget | ≤15,000/crate · ≤1,500/file · ≤100/fn (Gate 4) — ~4.3k src today |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |
| Publish | `true` — publishable on crates.io (nika-bm25 class · pure-algo external value) |
| Errors | `NIKA-BUILTIN-IMAGE_FX-001..006` (assigned by the `nika-builtin` wiring · the crate's own `FxError` is the typed source) |
| Reference | FX master plan · private DX surface (§12-B pins every algorithm) |

---

## 1. Purpose

`nika-fx` is the **deterministic artistic image-effects substrate** behind the
`nika:image_fx` builtin (stdlib §Media graduate #3 · the `image editing` deferred
row). PNG in → styled artifact out, **byte-identical forever** for identical
`(input bytes, args)`.

The determinism contract IS the product: every artifact's sha256 joins the run's
hash-chained trace, re-render is the tamper check (`verify()`), and the full
recipe (contract tag `image_fx/v1` · input sha256 · seed · ops) rides the
artifact itself as a PNG `nika` tEXt chunk (no timestamp). Receipts, not
decoration — the incumbent stack (ImageMagick et al.) is documented
non-reproducible by default; effects-as-integer-code can be byte-exact.

## 2. Public surface

- `transform(input_png, args, engine_tag) -> Result<FxOutput, FxError>` — the pipeline.
- `verify(input_png, args, engine_tag, artifact) -> Result<bool, FxError>` — re-render tamper check.
- `FxArgs::new(ops, seed)` · `Op` (15 op families · `#[non_exhaustive]`) · palettes/enums.
- `CONTRACT_TAG: &str = "image_fx/v1"` — the stable algorithm-contract tag.
- `codec::png::{decode, encode}` · `det::{sha256_hex, Pcg32}` — the hand-rolled substrate.

## 3. Determinism laws (each enforced by construction + CI)

1. **Zero std float transcendentals** — `clippy::disallowed-methods` bans
   `ln`/`log10`/`powf`/`exp`/`sqrt`/`cbrt`/`sin`/`cos`/`tan`/`atan2`; the
   deterministic seams (sRGB LUT · Oklab Q16 + fixed-iteration Newton cbrt · IGN
   u32 fractions · vignette rational cos⁴) live in `det.rs` + `tables.rs`.
2. **No hash-order collections** — `clippy::disallowed-types` bans
   `HashMap`/`HashSet`; `Vec`/`BTreeMap` in documented order.
3. **Seeded, never random** — one PCG32 stream per run (grain · glitch); the seed
   IS the style.
4. **Offline-generated tables carry a re-derivation test** (`det.rs::tests`).
5. **Every integer division's rounding rule is pinned** (goldens).
6. **wasm32 byte-parity** — the crate builds `wasm32-unknown-unknown`/`wasip1`
   unchanged; artifacts are byte-identical across aarch64 ↔ wasm (proven).

## 4. Security / resource contract

- Input = **PNG v1 only** (depth 8 · color 0/2/4/6 · no Adam7) · non-PNG → typed
  reject with the honest upstream hint.
- **Decoded-pixel budget** ≤2²⁶ + per-side ≤16384, gated from IHDR **before** any
  inflate (the decompression-bomb defense) · the ascii `png` emit re-checks the
  derived raster against the same budget (the output side).
- **Never panics** on hostile bytes — 10k-case seeded byte-mangling fuzz + typed
  rejects; no `.unwrap()`/`.expect()`/indexing in `src/` (lint-enforced).
- Pixel work runs on the blocking pool in the wiring (the jq precedent).

## 5. The op vocabulary (v1 · closed · `#[non_exhaustive]`)

`resize · crop · levels · grayscale · palette_map · dither · duotone · pixelate ·
halftone · grain · vignette · chromatic_aberration · scanlines · glitch · ascii`.
Dither modes: `bayer2/4/8 · blue_noise · ign · floyd_steinberg · atkinson · jjn`.
Branching/fan-out is the WORKFLOW's job (`with:`/`after:` edges · `for_each`) — `ops` is a
linear pipeline by design (never a graph-in-a-builtin · D-2026-05-22-N18).

## 6. Admission (12 gates · status)

Currently `wip = ["nika-fx"]` — pre full Gate-12 atomic admission. Green today:
Gate 1 (this spec) · Gate 3 (impl · 48 lib tests) · Gate 4 (clippy 0 `-D
warnings` · caps respected) · Gate 8 (doc build). Pending for admission: Gate 5
(mutation ≥90%) · Gate 6 (proptest — the double-render byte-eq is in place) ·
Gate 10 (no legacy parity — CRAFT, N/A) · Gate 11 (review swarm — done, wave 2).
