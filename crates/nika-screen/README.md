# nika-screen

Screen-capture **L1 effect crate** for the Nika diamond — the first
computer-use L1. Implements the L0.5 `nika_kernel::io::screen::ScreenCapture`
trait: display enumeration, full-display + sub-region capture, and a
continuous `FrameStream`, on macOS / Linux / Windows.

## Layer & deps

- **Layer L1** (effect impl, async). Depends only on L0 / L0.5.
- OS FFI is encapsulated by [`xcap`](https://crates.io/crates/xcap) (wired
  at B.3), so this crate stays `unsafe_code = forbid`-clean.

## Security (ADR-081)

Owns 2 of the 7 forever L1 security guards (MANDATORY-at-admission):

| Guard | What | Type |
|---|---|---|
| 6 | Capture-LED indicator | `guards::LedIndicator` |
| 7 | User consent UX (per-app / session-scoped, revocable) | `guards::ConsentGate` |

The OS-native LED polling (e.g. macOS `AVCaptureDevice`) is deferred — it
requires `unsafe` FFI, which this crate forbids; the indicator instead
exposes capture **state** the consuming UI renders (ADR-081 guard 6's
"explicit UI indicator otherwise" fallback).

## Error codes

`NIKA-1000..1099` reserved sub-range (`error::ScreenError`, accessor
`ScreenError::code()`). See `docs/adr/adr-081-l1-effect-crate-guard-contract.md`.

## Status

**Alpha · ADMITTED (M2.1 · 12-gate closed).** The 6-batch admission dispatch:

- ✅ B.1 — `capture_stream` additive kernel trait method
- ✅ B.2 — crate skeleton + `ScreenError` (NIKA-1000..1009) + guard skeletons
- ✅ B.3 — `xcap` single-shot capture impl (`spawn_blocking` · zero-copy RGBA8)
- ✅ B.4 — `Stream<Frame>` impl + cancel-safety (mpsc · drop-stop)
- ✅ B.5 — guards 6 + 7 (RAII LED indicator + fail-closed consent · enforced)
- ✅ B.6 — 12-gate close (all 12 green). **GAP-3 `From<ScreenPoint>` shim
  carried forward** — `ScreenPoint` is a `cockpit_overlay` (Olympus) type;
  a `From` impl here would be a Nika→Olympus dependency (cross-flow
  D-2026-05-08-N1 violation), and the conversion is an `io::input` (cursor)
  concern, not `io::screen`. It lands on the Olympus consumer side (where
  `cockpit-input-injection` already mirrors it) per M2.4 `nika-input`.
