# Crate spec — `nika-chart`

| | |
|---|---|
| Status | **CANDIDATE** — Gate 1 (this document) authored 2026-07-09. Crafted standalone (89 tests · clippy 0 under workspace pedantic · rustdoc 0) in a private playground tree, moved in whole per CHT master plan W2. |
| Layer | L0 — pure, zero I/O, zero async |
| Design | Deterministic chart compiler+renderer: rows + semantic spec → byte-identical artifacts. Two-stage pure pipeline (`compile → render`), 5 chart types, 5 surfaces (SVG · Vega-Lite JSON · TTY · PNG · terminal-inline escapes). The artifact's sha256 is the trace-chain receipt (« your workflow draws its own receipts »). |
| LOC budget | ≤6,000 src prod (at authoring ~5.3k total incl. cfg(test) — prod well under) · ≤15,000 hard cap |
| File cap | ≤1,500 LOC each (max file ~450) |
| Function cap | ≤100 lines each (max ~80) |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 (workspace-inherited) |
| Publish | `false` — foundation crate, never on crates.io |
| Dependencies | **ZERO** (operator lock 2026-07-09 · CHT G-gates): sha256 (FIPS 180-4), PNG/deflate (RFC 1950/1951 + W3C PNG-3), base64 (RFC 4648), palettes (Okabe-Ito · viridis · Moreland), tick labeling (Talbot-Lin-Hanrahan 2010, density erratum `2−max` per the R reference) — all implemented in-crate from primary sources |
| NIKA codes | `NIKA-BUILTIN-CHART-001..007` — designed as the builtin sub-namespace tail; `error.rs::ChartError::code()` emits the 3-digit tails (wired at the `nika-builtin` dispatch, W2 §2) |

---

## 1. Purpose

`nika-chart` renders **attestable charts** from run-shaped data: identical
`(spec, rows)` produce byte-identical artifacts on every platform (proven
executed: wasm32-wasip1 ≡ aarch64, SVG and PNG). The sha256 of the artifact
joins the run's trace chain; `attest::verify_artifact` re-renders from the
embedded self-describing manifest and byte-compares — a tampered chart is a
detectable advisory, not a belief.

Consumers (in dependency order): the `nika:chart` builtin (`nika-builtin`
dispatch, stdlib §Media graduate #3 — `chart` is ALREADY NAMED in
`stdlib/builtins-v0.1.md` §Media-deferred), `nika report` (`report::
report_html` / `report_tty` — pure fns the CLI folds traces into), the
VS Code run panel (same crate via wasm, byte-parity proven), terminal
inline images (`term_img::{kitty, iterm2}`).

It does **not** own: file I/O (the builtin's save path rides the kernel fs
seam + permits boundary in `nika-builtin`, image_generate precedent) ·
trace folding (CLI) · general-purpose viz (46-type ambitions route to the
`flint-chart-mcp` catalog escape hatch, master plan §7 guards).

## 2. Determinism contract (the design center)

Enforced **by construction**, then by tests:

- integer half-pixel grid — no float `Display` in any artifact path
- no transcendentals (POW10 table · clippy `disallowed-methods` bans
  `ln`/`powf`/… — std documents them platform/version-varying)
- no `HashMap`/`HashSet` (crate `clippy.toml` `disallowed-types`) ·
  `BTreeMap`/`Vec` + `total_cmp`
- no clock · no rand · no ids · no PNG ancillary chunks · LF only
- embedded font metrics (never a system font lookup)

Proof battery (`judge.sh`, one command): double-render byte-eq · golden
sha256 pins + tamper detector · 10k-case deterministic fuzz corpus with the
attest property (`verify_artifact == Match` corpus-wide, hostile-title
pool) · cross-arch parity run · official vega-lite compiler judge (6/6) ·
python-zlib + sips PNG decoders (pixel-exact) · HTML5 balance judge.

## 3. Public surface

`compile(spec, rows) -> ChartArtifact { svg, sha256, data_sha256,
vega_lite, warnings }` · `verify(spec, rows, sha) -> bool` ·
`attest::verify_artifact(svg, rows) -> Verdict` (total: Match ·
DataMismatch · ByteMismatch · NoManifest · BadManifest) ·
`report::{report_html, report_tty, report_with_hash}` ·
`render_png::{bar, line, area_band, scatter, heatmap}` ·
`tty::{bars, sparkline}` · `term_img::{kitty, iterm2, base64}` ·
`decimate::lttb` · `lint::advisories` (§3ter crate-computable subset).

Semantic types (closed v1 + `#[non_exhaustive]`): `usd · duration_ms ·
tokens · count · delta · percent · timestamp · category` — presentation
semantics ⊥ JSON-Schema correctness; they drive formatting, palettes
(delta ⇒ diverging anchored 0) and the TLH « Formats » scoring natively.

## 4. Tests

89 at authoring: unit + property (TLH integer-mode · LTTB anchors/spike/
monotone · escape round-trips) + goldens (byte pins) + fuzz-200 (in-tree) +
judges kept as examples (`fuzz_deep` 10k · `parity` · `vl_all` ·
`png_edge` · `verify_all`). Mutation floor: run `check-mutation-floor.sh`
at admission.

## 5. Non-goals / guards

No 5th verb, no chart DSL (charts are tools under `invoke`,
D-2026-05-22-N18) · no JS/flint dependency in-engine (pattern-steal only) ·
no PNG raster of SVG (`svg_render` stays deferred) · SVG is THE attestation
surface (PNG/TTY/VL are viewing projections — SQ-L canon).
