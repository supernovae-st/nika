# nika-chart

Deterministic chart compiler+renderer for the Nika engine — data + semantic
spec in, **byte-identical artifacts** out. The chart's sha256 joins the
workflow trace chain: *your workflow draws its own receipts, and the receipts
are part of the proof.*

**Zero dependencies.** `[dependencies]` is empty — sha256, PNG/deflate,
base64, palettes, fonts and tick labeling are implemented in-crate from
primary sources (FIPS 180-4 · RFC 1950/1951 · W3C PNG-3 · RFC 4648 ·
Talbot-Lin-Hanrahan 2010).

## Five surfaces, one recipe

The same `compile(spec, rows)` projects to every surface — the recipe is the
asset, surfaces are projections:

| Surface | Module | Notes |
|---|---|---|
| SVG | `svg` · `render` | theme-adaptive (`prefers-color-scheme` embedded · SAME bytes render light and dark) · integer half-pixel grid · no float formatting in the artifact |
| Vega-Lite v6 | `vega` | compile target for rich surfaces · provenance in `usermeta` |
| TTY | `tty` | eighth-block bars `│█▋ 141ms` + sparklines `▁▃▅█` |
| PNG | `png` · `raster` · `render_png` | hand-rolled encoder (fixed-Huffman deflate · filter Up) · integer Bresenham · embedded 8×8 public-domain font |
| Terminal inline | `term_img` | kitty graphics protocol + iTerm2 OSC 1337 escapes |

## Chart types (v1 five — each pinned to a run-surface consumer)

`bar` (per-node duration) · `line` (run-over-run cost) · `area_band`
(forecast p50/p90 — dashed predictions, solid observations) · `scatter`
(cost vs duration) · `heatmap` (step × run flakiness · diverging anchored 0).

## The determinism contract

Identical `(spec, rows)` ⇒ identical bytes, across platforms, re-runs and
wasm. Enforced **by construction**, not by testing alone:

- integer-grid coordinates (no float `Display` in any artifact path)
- no transcendentals (POW10 table · clippy `disallowed-methods` bans
  `ln`/`powf`/… — std documents them non-deterministic)
- no `HashMap` (clippy `disallowed-types`) · `BTreeMap`/`Vec` + `total_cmp`
- no clock · no rand · no ids · no ancillary PNG chunks · LF only
- embedded font metrics (no system font lookup — ever)

Verified by: double-render byte-eq tests · golden sha256 pins · a 200-case
deterministic fuzz corpus · cross-decoder PNG validation · `verify()`.

## Philosophy (master plan CHT §2bis · the 6 laws)

1. Receipts, not decoration.
2. Predictions dashed · observations solid.
3. Zero honesty (bars anchor zero · signs never dropped).
4. Semantics drive presentation (`usd` · `duration_ms` · `tokens` · `delta`…).
5. The fingerprint is visible (`nika · data <hash8>` + embedded manifest).
6. No-data is explicit, never silent.

## Prove it

```bash
cargo test                                   # the suite: goldens · fuzz · properties
cargo clippy --all-targets                   # 0 warnings · det laws as lints
cargo build --target wasm32-unknown-unknown  # same renderer, byte-parity
cargo run --example demo                     # 5 SVGs + PNG + VL JSON + TTY
cargo run --release --example bench          # µs/chart table (sha256 included)
cargo run --release --example fuzz_deep      # 10k-case deterministic corpus
cargo run --example vl_all                   # emit VL for all types (judge-ready)
cargo run --release --example parity         # cross-arch hashes (run native + wasip1)
```

## License

AGPL-3.0-or-later · © SuperNovae Studio. Embedded font8x8 data is Public
Domain (Daniel Hepper / Marcel Sondaar-IBM lineage).
