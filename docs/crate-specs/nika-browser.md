# nika-browser — crate spec (Gate 1)

| Field | Value |
|---|---|
| Crate | `nika-browser` |
| Milestone | **M2.5** — fifth computer-use L1 effect crate · the browser-automation arm |
| Layer | **L1** (effect) |
| Sub-tier | L1-effect — browser automation behind the L0.5 `BrowserAutomation`/`BrowserAutomationDyn` trait pair (launch · navigate · dom_snapshot · click_selector · screenshot). **Cross-platform** backend via `chromiumoxide` (CDP · macOS + Linux + Windows · pure-Rust async · zero unsafe in src) |
| Design | Async-native adapter over **`chromiumoxide` 0.9.1** (MIT OR Apache-2.0 · tokio-runtime feature · CDP over WebSocket · `kill_on_drop(true)` on the chromium child per Invariant #11 — verified in the 0.9.1 tarball `src/async_process.rs:23` + `src/browser/mod.rs:186`). UNLIKE the sync-backend M2 crates (`xcap`/`ocrs`/`AXUIElement`/`enigo` → `spawn_blocking`), chromiumoxide is tokio-async end-to-end: the trait methods await CDP calls DIRECTLY — no blocking workers, no per-call handle. The CDP event-loop `Handler` runs as ONE owned `tokio::spawn` task per session (Invariant #19 `new()` + held `JoinHandle`, aborted on drop) |
| LOC budget | ~2,500 src (adapter + Guard 5 + DOM mapper + PNG→Frame decode + session registry) |
| Public API | `ChromiumBrowser` (impl `BrowserAutomationDyn`) · re-export `nika_kernel::io::browser::BrowserError as Error` · Guard-5 pure core `verify_selector_target` + `SelectorExpectation` |
| Error range | **NIKA-1400..1499** — kernel-owned typed `BrowserError` (Pattern A · FCI-023bis · NO crate-local enum · NO thiserror dep) · LaunchFailed(1401) · NavigationFailed(1402) · SessionNotFound(1403) · SelectorFailed(1404) · BackendUnavailable(1405) · TaskJoinFailed(1406) |
| ADRs | ADR-003 (12 gates) · ADR-081 (**Guard 5 · selector clickjacking · MANDATORY-at-admission**) · ADR-083 (cross-platform doctrine — chromiumoxide covers macOS + Linux + Windows day-1) |
| Reference | [`chromiumoxide`](https://docs.rs/chromiumoxide) 0.9.1 (tarball-verified 2026-06-11: edition 2024 · MSRV 1.85 ≤ workspace 1.91 · `Browser::launch` · `Page::{goto,find_element,screenshot}` · `Element::{click,clickable_point}` · 0 `unsafe` in src) · `nika_kernel::io::browser` (L0.5 sealed trait + `BrowserSession`/`BrowserProfile`/`DomNode` DTOs) |

## 1. Purpose

Close the M2 computer-use surface with the BROWSER arm: the Olympus cockpit
(and later L2 verb crates) drive a real Chromium — navigate, read the DOM,
click, screenshot — through the same sealed-trait discipline as the desktop
arms. The desktop loop (screen→ocr→a11y→input) acts on ANY app; `nika-browser`
is the high-fidelity path for web targets (structured DOM instead of OCR
guesses, CDP determinism instead of synthetic coordinates).

## 2. Public API sketch

```rust
//! `nika-browser` · browser-automation L1 effect crate (CDP via chromiumoxide).

/// CDP browser backend. One chromium child per `launch`; sessions tracked in
/// an internal registry (SessionId → Page handle + Handler JoinHandle).
/// Async-native: NO spawn_blocking — chromiumoxide rides the same tokio
/// runtime (contrast the sync-backend M2 crates).
#[derive(Debug)]   // no Clone (owns child-process registry) · no Default
#[non_exhaustive]
pub struct ChromiumBrowser { /* session registry · Mutex<HashMap<…>> */ }

impl ChromiumBrowser {
    /// Hermetic constructor (no chromium spawned · no I/O). Invariant #19.
    pub fn new() -> Result<Self, BrowserError>;
}

impl nika_kernel::io::browser::BrowserAutomationDyn for ChromiumBrowser {
    // launch: spawns chromium (kill_on_drop) + Handler task; headless default;
    //         BrowserProfile maps to chromiumoxide BrowserConfig (user_agent ·
    //         viewport · headless).
    // navigate: RFC-3986 validate FIRST (pure) → NavigationFailed(1402) on
    //         malformed input · then CDP Page.navigate.
    // dom_snapshot: CDP DOM.getDocument → map to the kernel DomNode tree
    //         (pure mapper · depth-capped per untrusted-input discipline,
    //         same MAX_WALK_DEPTH pattern as nika-a11y).
    // click_selector: GUARD 5 path — see §5b. Resolve → verify → click.
    // screenshot: CDP Page.captureScreenshot (PNG) → decode → RGBA8 Frame
    //         (png crate · pure decode · width/height from IHDR).
}
```

The local `BrowserAutomation` trait arrives via the kernel's one-way blanket
impl (`Dyn` ⇒ local · the canonical M2 pattern — uniform across ALL L1 effect
crates since 2026-06-10 `9c455e2cd`).

## 3. Layer discipline

- **L1 effect** — implements one L0.5 trait pair. Depends only on
  `nika-kernel` (L0.5) + permissive externals (`chromiumoxide` MIT/Apache ·
  `tokio` · `png` for the screenshot decode · `url` for RFC-3986 validation).
  NO `thiserror` (Pattern A — kernel-owned error enum, re-exported).
- `tokio` layer-legal at L1 (deny.toml wrappers allowlist · add `nika-browser`
  + the chromiumoxide transitive consumers as needed).
- Zero `nika-*` cross-deps beyond `nika-kernel`. No upward imports.
- `chromiumoxide` is a normal (non-target-gated) dep — CDP is OS-agnostic;
  the chromium binary is located at runtime (system Chrome/Chromium/Edge) ·
  a missing binary surfaces as `BackendUnavailable`(1405) with remediation,
  NEVER a panic. The `fetcher` feature (auto-download chromium) stays OFF —
  sovereignty: we never silently download a 150 MB binary; the user installs
  their browser.

## 4. Backend scope — RESOLVED · `chromiumoxide` (primary-source verified)

| Candidate | Verdict | Why |
|---|---|---|
| **✅ `chromiumoxide` 0.9.1** (chosen) | CDP native · async tokio | MIT OR Apache-2.0 · 2.1 M downloads · updated 2026-02 · edition 2024 / MSRV 1.85 · `kill_on_drop(true)` built-in (Invariant #11) · 0 unsafe in src · async-native = no spawn_blocking ceremony · find_element/click/screenshot map 1:1 to the kernel trait |
| ❌ `headless_chrome` 1.0.21 | sync API | every call would need spawn_blocking + the crate trails CDP features; LGPL-free but the sync model fights our async traits |
| ❌ `fantoccini` 0.22 / `thirtyfour` 0.37 | WebDriver | needs a SEPARATE driver binary (chromedriver/geckodriver) running — heavier operational footprint + version-matching fragility; WebDriver protocol is also slower for DOM snapshots |
| ❌ raw CDP (tungstenite + serde) | DIY | re-implements chromiumoxide's generated protocol layer (~100k LOC of codegen) for zero gain |

## 5. Mandatory Guard 5 — selector clickjacking guard (ADR-081 · MANDATORY-at-admission)

Per ADR-081 §matrix line 5: *"verify selector points to expected DOM tree
shape · prevent malicious resolver"*. The threat: between the agent DECIDING
to click `sel` (based on an earlier `dom_snapshot`) and the click executing,
the page can mutate the DOM (or a malicious page can alias the selector) so
the click lands on a DIFFERENT element — the web equivalent of clickjacking.

### 5b. Guard 5 design — double-resolve + shape pin (pure core + CDP residue)

```rust
/// What the caller believes the selector targets (captured at decision time,
/// from the SAME dom_snapshot the agent reasoned over).
#[non_exhaustive]
pub struct SelectorExpectation {
    pub tag: String,                       // weak shape pin
    pub attributes: BTreeMap<String,String>, // weak shape pins (id · name · …)
    pub node_ref: Option<u64>,             // STRONG pin · the snapshot DomNode.node_ref
}

/// Guard 5 PURE core — does the freshly-resolved element match what the
/// agent decided to click? Headless-testable · mutation-killed.
pub fn verify_selector_target(
    resolved: &DomNode,
    expectation: &SelectorExpectation,
) -> Result<(), BrowserError>;   // SelectorFailed(1404) on mismatch
```

The `click_selector` dispatch path: (1) PEEK the expectation (clone · never
burned by a page-induced failure), (2) `cdp::verify_unique_match` (exactly
one), (3) `cdp::verify_stable_resolve` (same backend node twice), (4) map the
fresh element to a `DomNode` carrying its live `node_ref`, (5) `guard5_gate`
— the STRONG node-identity pin (`node_ref`) defeats structural look-alike
swaps that tag+attribute shape pins alone cannot, (6) **OCCLUSION hit-test**
(`cdp::verify_click_point_hits_target` · the SOTA "receives events"
actionability check · Playwright/Puppeteer model): hit-test the element's
actual click point via the protocol-level `DOM.getNodeForLocation` (NOT
page-side `elementsFromPoint`, which would need JS injection a hostile page
could poison) and require the topmost node there to be the target or a
descendant — a transparent overlay intercepting the click fails CLOSED,
(7) dispatch the CDP click, (8) consume the expectation only AFTER the click
succeeds. The kernel
trait signature carries no expectation param — the L1 crate exposes the
expectation via a `ChromiumBrowser::set_click_expectation` session-scoped
API (additive, crate-level), and `click_selector` WITHOUT a registered
expectation runs the STRUCTURAL checks only: selector resolves to EXACTLY
ONE element · element is visible (non-zero box · not `display:none`) ·
resolution is stable across a double-resolve (two CDP queries, same node id).
Mismatch / instability / multi-match → `SelectorFailed`(1404) — fail CLOSED,
never click a guess.

## 6. Batch plan (per M2 precedent)

- **B.1** this spec · backend research RESOLVED (chromiumoxide tarball-verified).
- **B.2** ✅ SHIPPED `eb6f07e0d` — crate skeleton + Guard-5 pure core
  (`verify_selector_target` + `SelectorExpectation`) + headless tests. The
  security core shipped before any CDP wiring (the nika-input precedent).
- **B.3** ✅ SHIPPED — chromiumoxide backend wired: launch (kill_on_drop +
  one owned Handler task per session) · navigate (pure RFC-3986 http/https
  gate BEFORE session lookup) · dom_snapshot (depth-capped pure mapper —
  the cap protects DomNode's own recursive derive glue, not just the
  mapper) · click_selector (Guard-5: exactly-one match + stable
  double-resolve on backend_node_id + live-bbox visibility + expectation
  pins) · screenshot (PNG→RGBA8 Frame · pure decode). Real-chromium
  `#[ignore]` smoke PASSES (launch/snapshot/screenshot/guard-5 refusals).
- **B.4** mutation ≥90 % headless + Rule-2 CDP-residue exemption + 3-lens
  review swarm + 12-gate close + admission.

## 7. Gate status — ADR-003 canonical 12 gates

| # | Gate | Status | Evidence |
|---|------|--------|----------|
| 1 | SPEC | ✅ | this file |
| 2 | TDD | ✅ | Guard-5 pure core headless-first (B.2) |
| 3 | IMPL | ✅ | chromiumoxide 0.9.1 · API tarball-verified pre-write (B.3) |
| 4 | CLIPPY | ✅ | **measured 2026-06-11** workspace `-D warnings` 0 |
| 5 | MUTATION ≥ 90% | ✅ | **re-measured 2026-06-11 post-occlusion** `cargo mutants -p nika-browser -- --lib` · 76 mutants · **60/65 viable caught = 92.3 %** · 11 unviable · 5 documented exemptions (`GATE5-EXEMPT` below) |
| 6 | PROPERTY | ✅ | proptest `guard5_tag_swap_never_verifies` |
| 7 | BENCHMARKS | ⚪ N/A | CDP round-trip is network/browser-bound (Rule 2) |
| 8 | DOCS | ✅ | **measured 2026-06-11** cargo doc 0 warnings |
| 9 | CANARY E2E | ⚪ N/A | L1 effect · `#[ignore]` smoke (real chromium) PASSES |
| 10 | PARITY | ⚪ N/A | the brouillon `feat/p5-chromium-render` branch is paged out to the private legacy repo · no public parity target |
| 11 | REVIEW SWARM | ✅ | 3-lens adversarial swarm (28 agents) · 3 P1 + P2s on Guard 5 ALL folded before B.4 (node-identity pin · no failure-downgrade · pure structural gates) |
| 12 | ATOMIC COMMIT | ✅ | the admission commit (B.4) |

### 7.1 Gate 5 exemption budget (ADR-003 Rule 2 · re-verified 2026-06-11 post-occlusion)

<!-- GATE5-EXEMPT: 5 -->

Re-measured after the occlusion hardening: 76 mutants · **60/65 viable
caught = 92.3 %** · 11 unviable. The pure surface (mappers · URL gate ·
PNG decode · the 3 Guard-5 gates · the 3 occlusion fns
`verify_click_point_hits_target`/`collect_backend_ids`/`coord_to_i64` ·
`backend_ref` · `bbox_to_rect` boundary · `epoch_now_ns` · `consume`) is
fully mutation-killed. The five survivors are TRUE residue/equivalents:

1. `<impl Drop>::drop with ()` — aborts the per-session Handler tasks; the
   effect (no orphan task/child) is only observable with a LIVE session, so
   the `#[ignore]` real-chromium smoke exercises it, not headless CI.
2. `dom_snapshot delete - in depth(-1)` — flips the CDP `GetDocument` query
   from full-tree to depth-1; only a live multi-level DOM distinguishes it
   (smoke-only).
3. `click_selector delete - in depth(-1)` — the SAME class for the occlusion
   path's full-depth `DescribeNode` (the subtree query); a live deep DOM is
   needed to observe the difference (the `#[ignore]` overlay smoke covers it).
4. `<impl Debug for SessionHandle>::fmt` — the cosmetic
   `finish_non_exhaustive` Debug body; no behavior rides it.
5. `bbox_to_rect: > with >=` (the `dim` closure's `v > 0.0`) — a TRUE
   EQUIVALENT: at the only distinguishing input `v == 0.0`, the then-branch
   computes `0.0.round() as u32 == 0`, identical to the else-branch's `0`,
   so no test can separate `>` from `>=`.

Survivors 1-3 are CDP-residue (live-browser-only · the planned Rule-2
exemption · the occlusion fns themselves are pure + killed, only their
live-CDP wiring residue survives); 4 is cosmetic; 5 is a mathematical
equivalent. None is a logic gap.

## 8. Security (ADR-081)

Guard 5 is THE mandatory guard (§5b). Additional posture:
- `BrowserProfile` never carries credentials — cookie/auth injection is out
  of scope for M2.5 (a future L2 concern with its own consent design).
- DOM snapshots are UNTRUSTED INPUT: bounded on BOTH axes — depth
  (`MAX_DOM_DEPTH`) caps recursion + a total node budget (`MAX_DOM_NODES`)
  caps memory, so a hostile tree shape cannot stack-overflow or OOM the
  agent. (Per-attribute VALUE size is an unbounded residual · follow-up.)
- The chromium child is always `kill_on_drop` — no orphan browsers (#11);
  the per-session Handler task is aborted on launch failure + at drop.
- Guard-5 click safety is TWO-layer: geometric visibility (non-zero box) +
  OCCLUSION hit-test (the click point must hit-test to the target or a
  descendant via protocol-level `DOM.getNodeForLocation` · shipped
  2026-06-11 · the SOTA actionability model · catches transparent-overlay
  clickjacks the bbox check alone cannot · e2e-verified on real chromium).
- Headless follows the kernel `BrowserProfile` DTO (its `Default` is
  headful-VISIBLE — the transparency posture, consistent with the Guard-6
  LED-visibility spirit); agent callers opt INTO headless explicitly.
