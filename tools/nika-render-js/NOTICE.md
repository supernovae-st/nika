# nika-render-js · third-party attribution

> Per `dx/.claude/rules/brouillon-lift-pattern.md` §4 — SHA-pinned upstream
> attribution. This crate is a NEW production impl (NOT a code lift); it
> *depends on* chromiumoxide as a Cargo dependency. The attribution below
> records the pinned dependency, its license, and the primary-source
> verification done at admission time.

## chromiumoxide

| Field | Value |
|---|---|
| Crate | `chromiumoxide` |
| Version pin | `=0.9.1` (exact · no caret) |
| License | `MIT OR Apache-2.0` (dual) |
| Repository | <https://github.com/mattsse/chromiumoxide> |
| max_stable verified | `0.9.1` (cratesio API · 2026-02-25 · 1,846,811 downloads) |
| Verify date | 2026-05-20 (cratesio primary-source · phantom-feature-recheck §3 Step 3) |
| Feature enabled | `rustls` (pure-Rust TLS for the auto-download fetcher) |

> **Phantom-feature caught** (phantom-feature-recheck §3 Step 2 · context7
> verify 2026-05-20) · the `tokio-runtime` feature does NOT exist in
> chromiumoxide 0.9.1 (available features · `bytes · chromiumoxide_fetcher ·
> default · fetcher · native-tls · rustls · zip0 · zip8`). The async runtime
> is determined by the calling context (`#[tokio::main]` / workspace tokio
> rt-multi-thread) · no Cargo feature selects it. `rustls` chosen over
> `native-tls` for sovereign pure-Rust TLS.

### Why exact pin

chromiumoxide wraps the Chrome DevTools Protocol; minor releases (0.10+) have
historically changed the `Browser` / `Page` / `Handler` surface. An exact `=0.9.1`
pin keeps the integration deterministic. Bumping to 0.10+ requires an ADR per
`no-legacy-no-back-compat.md` (forever-v0.x · breaking changes ship on MINOR with
review).

### License compatibility

`MIT OR Apache-2.0` is in the workspace `cargo deny` allowlist and compatible with
the engine's `AGPL-3.0-or-later` (permissive → copyleft direction is sound). The
Chromium binary itself is downloaded at runtime by chromiumoxide and is BSD-3-Clause
+ other permissive licenses (Google Chrome OSS) · not redistributed by this crate.

## Sovereignty

Per `supernovae-alignment.md` Rule 1 — rendering is 100% local. The headless Chrome
process runs on the operator's machine; no page content, telemetry, or rendering
state leaves the host. No cloud rendering service is contacted.
