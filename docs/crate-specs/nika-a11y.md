# Crate spec — `nika-a11y`

| | |
|---|---|
| Status | **ADMITTED** 2026-05-25 (Phase 2 M2.3 · third L1 effect crate · ADR-003 12 gates · Guard 3 AX-secure-field redaction MANDATORY · mutation 82.9 % + Rule-2 AXUIElement-walk exemption · §7) |
| Layer | L1 — effect implementation · async · `Send + Sync` · depends only on L0 / L0.5 |
| Sub-tier | L1-effect — accessibility-tree query behind the L0.5 `AccessibilityTree` trait. macOS backend via the safe-API `accessibility` crate (encapsulates `AXUIElement` unsafe FFI · so `nika-a11y` stays `unsafe_code = forbid`) |
| Design | macOS-first adapter over **accessibility 0.2.0** (`AXUIElement` · `TreeWalker`/`TreeVisitor` · MIT/Apache · safe Rust API · the unsafe ApplicationServices FFI is encapsulated inside the crate, same posture `nika-screen` got from `xcap` + `nika-ocr` from `ocrs`). Sync `AXUIElement` walk runs inside `tokio::task::spawn_blocking` (the `AXUIElement` handle is `!Send` · stays worker-local · produces the `Send` `AxNode` tree · kernel CANCEL SAFETY contract · same pattern as `nika-screen`'s `xcap::Monitor`). AX tree → `AxNode` mapping → **mandatory AX-secure-field redaction (Guard 3)** → `Vec<AxNode>` |
| LOC budget | ≤1,300 src |
| File cap | ≤1,500 LOC each · Function cap ≤100 lines |
| Crate version | tracks workspace (`0.80.0`) · License `AGPL-3.0-or-later` · Edition 2024 · Publish `false` |
| ADRs | ADR-003 (12-gate admission) · ADR-081 (7 L1 security guards forever · nika-a11y owns **Guard 3 · AX-secure-field redaction · MANDATORY-at-admission**) |
| Error range | **NIKA-1200..1299** (per ADR-081 `nika_codes` matrix · **supersedes** the stale `io/a11y.rs` doc-comment "NIKA-1060..1079" which predates ADR-081 · reconciled here · same pattern as nika-ocr NIKA-1100..1199) |
| Reference | [`accessibility`](https://docs.rs/accessibility/0.2.0) (MIT/Apache · macOS AXUIElement) · `nika-kernel::io::a11y` (L0.5 sealed `AccessibilityTree` trait + `AxNode`/`AxRole`/`AxQuery` DTOs) |

---

## 1. Purpose

`nika-a11y` is the **third computer-use L1 effect crate** (M2.3 · after
`nika-screen` M2.1 + `nika-ocr` M2.2). It implements the L0.5
`nika_kernel::io::a11y::AccessibilityTree` trait — `snapshot()` +
`find(&AxQuery)` + `resolve_ref(&str)` — exposing the active window's
accessibility tree as `AxNode` records (id · role · label · value · bbox ·
children · attributes) so the Olympus cockpit can query UI structure
semantically (find the "Submit" button · read a field's value · locate a
heading) instead of pixel-guessing.

The OS query is delegated to **`accessibility`** (safe Rust API over macOS
`AXUIElement`), so `nika-a11y` itself contains **zero `unsafe`** and honours
`unsafe_code = "forbid"` — the same sovereign / no-hand-written-FFI posture
`nika-screen` got from `xcap` and `nika-ocr` from `ocrs`.

`nika-a11y` reuses the `Rect` DTO from `nika-screen` (M2.1 · `AxNode.bbox`),
so capture → OCR → a11y all share one pixel-coordinate space (single canonical
geometry per `no-legacy-no-back-compat.md` Class 1).

## 2. Public API

```rust
//! `nika-a11y` · accessibility-tree-query L1 effect crate.

/// Accessibility backend (macOS `AXUIElement` via the safe `accessibility`
/// crate). Walks the focused application's tree in `spawn_blocking` (the
/// `AXUIElement` handle is `!Send`) and applies the mandatory secure-field
/// redaction (Guard 3) before exposing any `AxNode`.
#[non_exhaustive]
pub struct AxBackend { /* ref cache: BTreeMap<String, ...> | pid target */ }

impl AxBackend {
    /// Construct a backend bound to the system-wide / focused-app element.
    /// Errors NIKA-1201 if the process lacks macOS Accessibility trust.
    pub fn new() -> Result<Self, A11yError>;
}

impl nika_kernel::io::a11y::AccessibilityTree for AxBackend {
    async fn snapshot(&self) -> io::Result<AxNode>;
    async fn find(&self, query: &AxQuery) -> io::Result<Vec<AxNode>>;
    async fn resolve_ref(&self, ref_id: &str) -> io::Result<Option<AxNode>>;
}

/// Errors · NIKA-1201..1206 · #[non_exhaustive] + code() + is_transient().
/// NIKA-1200 = retired B.2 BackendNotWired placeholder slot (reserved).
#[non_exhaustive]
pub enum A11yError { /* PermissionDenied(1201) .. TaskJoinFailed(1206) */ }
```

## 3. Layer discipline

- **L1 effect** — implements one L0.5 trait (`AccessibilityTree`). Depends only
  on `nika-kernel` (L0.5) + permissive externals (`accessibility` on macOS ·
  `tokio` rt for `spawn_blocking` · `thiserror`).
- `tokio` layer-legal at L1 (deny.toml wrappers allowlist · add `nika-a11y`) ·
  `spawn_blocking` only (sync `AXUIElement` walk · `!Send` handle worker-local).
- Zero `nika-*` cross-deps beyond `nika-kernel`. No upward imports.

## 4. OS-backend scope — architecture decision (DECISION POINT)

Querying *another app's* accessibility tree is **per-OS** — there is no mature
single cross-platform pure-Rust QUERY crate (`accesskit` is the inverse: it
*publishes* an app's own tree to ATs, not query). Three vetted backends exist,
all permissively licensed (verified crates.io 2026-05-25):

| OS | Crate | Version | License | Notes |
|---|---|---|---|---|
| macOS | `accessibility` | 0.2.0 | MIT / Apache-2.0 | safe API over `AXUIElement` · `TreeWalker`/`TreeVisitor` · unsafe encapsulated |
| Linux | `atspi` | 0.30.0 | Apache-2.0 OR MIT | pure-Rust AT-SPI2 D-Bus client (zbus) |
| Windows | `uiautomation` | 0.25.0 | Apache-2.0 | UIA · COM FFI encapsulated |

**RECOMMENDED · macOS-first (Option A).** Ship the macOS `accessibility`
backend now (the Olympus dev + atelier platform is darwin · computer-use tools
ship macOS-first). Linux/Windows are `#[cfg(target_os = ...)]` skeleton
backends returning `A11yError::BackendUnavailable` (NIKA-12NN) until a consumer
signal lands (LOCK-031 spirit). The cross-platform `AxNode` DTO + the **pure
mandatory redaction guard ship all-OS, headless** (§5). Deps stay macOS-gated
(`[target.'cfg(target_os="macos")'.dependencies] accessibility`).

**Alternative (Option B) · full 3-OS now** — adds `atspi` + `uiautomation`
backends + their per-OS attribute→`AxNode` mapping + tri-platform CI. ~3× the
surface · not needed while the only consumer (Olympus cockpit) is darwin.

> **RESOLVED B.3 · Option A (macOS-first)** locked. The macOS `accessibility`
> backend is wired (`system_wide().focused_window()` rooted recursive walk ·
> role/label/value/subrole → `AxNode`); `core-foundation` reads `CFString`/
> `CFType`. Linux/Windows compile to `A11yError::BackendUnavailable`
> (`#[cfg(not(target_os = "macos"))]`) until a consumer signal (LOCK-031).
> The mandatory Guard 3 + DTO mapping + ref-cache stay backend-agnostic.

> **AMENDED 2026-05-26 · ADR-083 cross-platform doctrine** (preserves the B.3
> macOS decision above as historical · per `cross-source-validation.md` §2.7).
> The "macOS-first, defer the rest behind a consumer signal" disposition is
> **superseded** per ADR-083 (L1 computer-use cross-platform · macOS + Linux
> prio · Windows + all). **B.3-bis (2026-05-26) · the Linux `atspi` backend is
> WRITTEN** behind `#[cfg(target_os = "linux")]` (async AT-SPI2 D-Bus · same
> `AccessibilityTree` trait · the DTO + the pure Guard 3 core are backend-
> agnostic + reused · role mapped via `atspi::Role` · secure marker set on
> `Role::PasswordText`) + `build_raw_tree()` is cfg-branched (macOS sync
> `spawn_blocking` · Linux async · else `BackendUnavailable`).
>
> ⚠️ **The Linux backend is CI-PENDING** · the dev host is `aarch64-apple-darwin`
> only, so the `#[cfg(target_os = "linux")]` code is **NOT compiled or tested
> here** (it is primary-source-grounded against `atspi` 0.29). It MUST be
> compiled + its Guard 3 (`Role::PasswordText` → redaction) tested on a real
> Linux / AT-SPI host before the Linux backend is trusted — inline `CI-VERIFY`
> markers flag the variant-name + proxy-lifetime + async-recursion checkpoints.
> The macOS backend + the pure Guard 3 core stay fully verified on darwin (14
> lib tests + clippy 0 + `cargo deny` clean incl the atspi/zbus tree). Windows
> `uiautomation` lands later. macOS is no longer the *permanent* gate · it is
> the first of the macOS + Linux pair.

## 5. Mandatory Guard 3 — AX-secure-field redaction (ADR-081 · MANDATORY-at-admission)

Per ADR-081 §matrix, `nika-a11y` owns **Guard 3 · MANDATORY-at-admission**:
secure-text fields MUST have their `AxNode.value` **stripped** before any node
leaves the crate — passwords never reach a caller. The per-OS secure signal is:
- **macOS** · the `AXSecureTextField` **subrole** (`attributes["AXSubrole"]`).
- **Linux (AT-SPI)** · the **`Role::PasswordText` role** → `attributes["secure"]
  = "true"`.

> ⚠️ **Correctness fix (ADR-083 · 2026-05-26)** · the earlier draft cited AT-SPI
> `STATE_SENSITIVE` as the secure signal. That is **wrong** — AT-SPI
> `State::Sensitive` means *"enabled / interactive / not greyed-out"* (true for
> almost every usable field), the **opposite** of secret. Using it would BOTH
> over-redact ordinary fields AND miss real password inputs. The canonical
> AT-SPI password signal is the **`PasswordText` role**. The `is_secure_field`
> predicate + its unit tests are corrected accordingly (a `sensitive=true`
> attribute now explicitly does **not** trip the guard).

**Design — the guard is a PURE tree-transform** (the security-critical core is
backend-independent + 100 % headless-testable + mutation-killable):

```rust
/// Redact secure-field values across the tree. Pure · headless-testable.
fn redact_secure_fields(node: AxNode) -> AxNode;          // recursive
fn is_secure_field(node: &AxNode) -> bool;                // role/attr predicate
```

`is_secure_field` reads ONE canonical marker the backend populates while
walking: `attributes["AXSubrole"] == "AXSecureTextField"` (macOS) OR
`attributes["secure"] == "true"` (Linux · set when role is `Role::PasswordText`).
When true, the node's `value` is replaced with `None` (NOT a masked string —
zero leak). The guard is applied to **every** `snapshot`/`find`/`resolve_ref`
result before return on **every OS**. Per ADR-081 per-guard contract: ≥3 unit
tests (happy / redacted / nested-secure-child / `sensitive`-is-not-secure) + the
pure transform is the all-OS mandatory gate.

## 6. Batch plan (skeleton-option-A · per nika-screen / nika-ocr precedent)

- **B.1** spec (this file) · backend research done · decision OPEN (§4).
- **B.2** crate skeleton + `A11yError` NIKA-1200..1206 + `AxBackend` skeleton
  + the **pure Guard 3 redaction** (`redact_secure_fields` / `is_secure_field`)
  + pure `find` filter (`matches_query` / `collect_matches`) + headless tests.
  Mandatory guard headless-complete at B.2. ✅
- **B.3** macOS `accessibility` backend wired · `system_wide().focused_window()`
  rooted recursive `build_node` (role/label/value/subrole → `AxNode`) inside
  `spawn_blocking` · `core-foundation` `CFString`/`CFType` reads · `@e<N>`
  ref-cache (`Mutex<Option<AxNode>>` + pure `find_by_id`) for `resolve_ref` ·
  pure `ax_role_from_str` · `bbox` deferred (`None` · frame→`Rect` refinement).
  Non-macOS `#[cfg]` `BackendUnavailable`. Closed the `BackendNotWired`
  placeholder (NIKA-1200 retired). 13 lib tests + 1 `#[ignore]` real-walk
  smoke · clippy/doc/machete/deny green · workspace `--lib` 1169. ✅
- **B.4** mutation (`cargo mutants -p nika-a11y -- --lib`) → **82.9 %** (34/41
  viable) · 100 % of the headless surface (extracted pure `assemble_node` +
  role map + find_by_id + redaction + query-filter) + Rule-2 exemption for the
  7 `AXUIElement`-walk mutants (§7.1) + ADR-003 canonical 12-gate close (§7) +
  Foreman-direct 3-lens review (PE-5.1 · added `MAX_WALK_DEPTH` untrusted-input
  cap) + admission commit. ✅ ADMITTED 2026-05-25 (macOS).
- **B.3-bis** (2026-05-26 · ADR-083 cross-platform) Linux `atspi` backend
  written behind `#[cfg(target_os = "linux")]` (async AT-SPI2 · `atspi_role_to_ax`
  · `build_node_atspi` `Box::pin` recursion · `Role::PasswordText` → `secure`
  marker reusing the pure Guard 3) + `build_raw_tree()` cfg-branch + Guard 3
  semantic fix (`secure` marker · NOT `State::Sensitive`). macOS path re-verified
  green (14 lib tests · clippy 0 · `cargo deny` clean incl atspi/zbus tree).
  ⚠️ **Linux backend CI-PENDING** — `#[cfg(target_os = "linux")]` code NOT
  compiled/tested on the darwin host · primary-source-grounded (`atspi` 0.29) ·
  compile + Guard 3 test required on a Linux/AT-SPI host (inline `CI-VERIFY`).

## 7. Gate status — ADR-003 canonical 12 gates

> **macOS backend** · all 12 gates ✅ (admitted 2026-05-25 · table below). The
> **Linux `atspi` backend** (B.3-bis · 2026-05-26) is gate-tracked SEPARATELY ·
> Gate 3 IMPL / Gate 4 CLIPPY / Gate 5 MUTATION are **⏳ Linux-CI-pending** (the
> darwin dev host cannot compile `#[cfg(target_os = "linux")]`). The pure Guard 3
> core it relies on IS verified on darwin; the atspi walk + its `Role::PasswordText`
> detection MUST be compiled + tested on a Linux/AT-SPI host before trust.

| # | Gate | Status | Evidence |
|---|------|--------|----------|
| 1 | SPEC | ✅ | this file |
| 2 | TDD | ✅ | tests precede impl · 14 lib tests (incl 1 proptest) + 1 `#[ignore]` smoke |
| 3 | IMPL | ✅ | ~330 src LOC · `cargo check` 0 |
| 4 | CLIPPY | ✅ | `clippy --workspace --all-targets -D warnings` 0 |
| 5 | MUTATION | ✅ + exemption | `cargo mutants -p nika-a11y -- --lib` · **34/41 viable caught (82.9 %)** · 100 % of headless-reachable · 7 AXUIElement-walk mutants exempt (§7.1) |
| 6 | PROPERTY | ✅ | proptest · Guard 3 redaction invariant (every secure node loses its `value`) |
| 7 | BENCHMARKS | ⚪ N/A | thin `accessibility` adapter · walk latency is OS-bound, not a Nika hot path (Rule 2) |
| 8 | DOCS | ✅ | `cargo doc --no-deps` 0 warnings · all pub items documented |
| 9 | CANARY E2E | ⚪ N/A | L1 effect crate · no `.nika.yaml` surface · the `#[ignore]` real-walk smoke needs AX grant + a focused window |
| 10 | PARITY | ⚪ N/A | NEW computer-use crate (M2.3) · no v0.79 brouillon a11y equivalent |
| 11 | REVIEW SWARM | ✅ | 3-lens review 2026-05-25 · sub-agents hit the 1M-context credit wall → **Foreman-direct** per `orchestrator-autonomous-v6.md` PE-5.1 · rust-pro + Diamond + bug-hunt · all ADMIT · findings fixed (unbounded-recursion → `MAX_WALK_DEPTH` cap · role-arm test gap · `resolve_ref` cache-seed) |
| 12 | ATOMIC COMMIT | ✅ | the admission commit |

### 7.1 Gate 5 mutation exemption (ADR-003 Rule 2 · macOS AXUIElement walk)

<!-- check-mutation-floor.sh note: the "7 AXUIElement-walk exempt" below is the
     macOS-reachable measurement. A naive `cargo mutants -p nika-a11y -- --lib`
     on macOS reports ~31 survivors because it ALSO mutates the cfg'd-out Linux
     atspi role-mapping arms (platform-inactive, not real test gaps). BUDGET
     mode therefore needs a per-crate mutants.toml exclude_re for the cfg'd-out
     code before the budget marker is reproducible on the CI platform — a
     deferred-with-trigger follow-up (calibrate when a mutants.toml lands). -->

`nika-a11y` is a thin adapter over the synchronous `accessibility` walk. 7
mutants are **exempt** — they live on the `AXUIElement` traversal control-flow,
reachable only with a real macOS Accessibility grant + focused window (exercised
by the `#[ignore]` smoke test, not headless CI):

- `build_node` id-`counter += 1` (×2) + `depth >= MAX_WALK_DEPTH` cap + `depth + 1`
  recursion (×2) — all inside the OS walk
- `walk_focused_tree` non-macOS `BackendUnavailable` stub (cfg'd out on macOS)
- `find` → `Ok(vec![])` (delegates to the walk via `redacted_snapshot`)

All **headless-reachable** logic is at 100 % mutation kill — the MANDATORY
Guard 3 (`is_secure_field` + `redact_secure_fields`), the pure node assembly
(`assemble_node`), `ax_role_from_str` (every arm), `find_by_id` (ref cache),
`matches_query` + depth-bounded `collect_matches`, and the full `A11yError`
surface. Per ADR-003 Rule 2 the AXUIElement-walk residue is documented-exempt,
not skipped. Re-run with a focused window: `cargo test -p nika-a11y -- --ignored`.

## 8. Security (ADR-081)

`nika-a11y` owns **Guard 3 (AX-secure-field redaction) · MANDATORY-at-admission**.
The redaction is a pure, always-on tree-transform applied before any node
exposure (no opt-out · zero password leak by construction). Telemetry-canon §0:
zero cloud · no guard-state egress. Sovereignty Rule 1: no vendor-hosted state.
The remaining 6 ADR-081 guards belong to other L1 crates (input / browser /
vision-local / screen) per the §matrix.
