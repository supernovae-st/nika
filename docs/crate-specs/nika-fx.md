# `nika-fx` — crate spec

| Field | Value |
|---|---|
| Layer | **L1** (pure-algo · zero I/O · consumed by `nika-builtin` L1.5) |
| Dependencies | **ZERO** runtime deps (the determinism + supply-chain contract) |
| LOC budget | ≤25,000/crate · ≤2,500/file · ≤100/fn (Gate 4; ADR-154) — ~4.3k src today |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |
| Publish | `true` — publishable on crates.io (nika-bm25 class · pure-algo external value) |
| Errors | `NIKA-BUILTIN-IMAGE_FX-001..006` and `NIKA-BUILTIN-IMAGE_COMPOSE-001..006` (assigned by the `nika-builtin` wiring · the crate's own `FxError` is the typed source) |
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

Since 2026-10-26 it is also the substrate behind `nika:image_compose` (stdlib
§Media graduate #5): an ordered list of PNGs becomes ONE PNG laid out as a
`grid`, a `row` or a `column` (`compose.rs`), under the same law — integer
geometry, the `resize` op's linear-light ladder for every scaled box, the recipe
(`image_compose/v1` · each input's sha256 and size · the resolved layout) in the
`nika` tEXt chunk, no timestamp and no path.

## 2. Public surface

- `transform(input_png, args, engine_tag) -> Result<FxOutput, FxError>` — the pipeline.
- `verify(input_png, args, engine_tag, artifact) -> Result<bool, FxError>` — re-render tamper check.
- `FxArgs::new(ops, seed)` · `Op` (15 op families · `#[non_exhaustive]`) · palettes/enums.
- `CONTRACT_TAG: &str = "image_fx/v1"` — the stable algorithm-contract tag.
- `codec::png::{decode, encode}` · `det::{sha256_hex, Pcg32}` — the hand-rolled substrate.
- `compose::{compose, grid}` · `ComposeArgs::new()` · `Layout` · `Scale` · `ComposeOutput` ·
  `InputFacts` · `compose::CONTRACT_TAG = "image_compose/v1"` — the composition.
- `codec::png::{header, decode_with_alpha}` — the IHDR-only probe (every cap judged
  before any inflate) and the alpha plane the composer flattens over its background.
- `FxError::Cap` — a per-side size cap (the pixel totals stay `FxError::Budget`).

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
- **Composition caps** — 1..=64 inputs (an argument, `FxError::Args`) · each input
  ≤16384 per side (`FxError::Cap`, for `image_fx` too) · the SUM of the inputs'
  pixels ≤2²⁶ · the canvas (and a `fill` cover box) ≤16384 per side and ≤2²⁶
  pixels — all judged from the IHDRs and the args before any decompression or
  canvas allocation (golden tests feed header-only PNGs: a cap that fired after an
  inflate would fail `Decode` instead).
- **One IHDR, first** — `decode` refuses an IHDR that is not the first chunk or a
  second IHDR (`FxError::Decode`), so the size `header` judged is the size decoded;
  `compose` also refuses a decoded size that differs from its header.

## 5. The op vocabulary (v1 · closed · `#[non_exhaustive]`)

`resize · crop · levels · grayscale · palette_map · dither · duotone · pixelate ·
halftone · grain · vignette · chromatic_aberration · scanlines · glitch · ascii`.
Dither modes: `bayer2/4/8 · blue_noise · ign · floyd_steinberg · atkinson · jjn`.
Branching/fan-out is the WORKFLOW's job (`with:`/`after:` edges · `for_each`) — `ops` is a
linear pipeline by design (never a graph-in-a-builtin · D-2026-05-22-N18).

## 5bis. The composition geometry (v1 · normative in stdlib §`nika:image_compose`)

- `grid` derives ⌈√n⌉ columns and ⌈n ÷ columns⌉ rows (one side given: the
  other is ⌈n ÷ given⌉); `row` is n×1 and `column` 1×n; placement is row-major.
- `none` keeps each image's size; `fit` scales into the cell with the aspect
  kept (`round(w·ch ÷ h) × ch` when `w·ch ≤ h·cw`, else `cw × round(h·cw ÷ w)`);
  `fill` scales to the cover box and center-crops to the cell. `round(a ÷ b)` is
  `⌊(a + ⌊b ÷ 2⌋) ÷ b⌋`, never below 1.
- Tracks are the cell under `fit`/`fill`, the widest/tallest tile per track under
  `none`; each tile is centered at `⌊(track − tile) ÷ 2⌋`; the canvas is
  `2·padding + Σ tracks + gap·(n − 1)` per side, filled with `background`.
- Alpha composites over the background before scaling:
  `⌊(c·a + bg·(255 − a) + 127) ÷ 255⌋` per sRGB channel; the artifact is RGB8.
- Five golden layouts pin the bytes by sha256 (`compose/tests.rs`).

## 6. Admission (12 gates · status)

Currently `wip = ["nika-fx"]` — pre full Gate-12 atomic admission. Green today:
Gate 1 (this spec) · Gate 3 (impl · 48 lib tests) · Gate 4 (clippy 0 `-D
warnings` · caps respected) · Gate 8 (doc build). Pending for admission: Gate 5
(mutation ≥90%) · Gate 6 (proptest — the double-render byte-eq is in place) ·
Gate 10 (no legacy parity — CRAFT, N/A) · Gate 11 (review swarm — done, wave 2).
