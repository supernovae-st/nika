// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Input device traits — L0.5 sealed DTOs + async trait gated by type-state
//! consent.
//!
//! Phase 2 M1 PR 4 (sprint plan `2026-05-14-nika-phase-2-m1-kernel-modules-
//! sprint-plan.md` §4 · ~200 LOC · ⚠️ **HIGHEST REVIEW SCRUTINY** ·
//! type-state `ConsentProof` is load-bearing). Reserved error codes
//! NIKA-1300..1399 (`Category::Input` · per ADR-081) per
//! `docs/architecture/forward-compat-invariants.md`
//! Gate 12.
//!
//! ADR-006 monolithic-kernel-spirit (single trait + DTOs · no proc macro ·
//! no async runtime dep). ADR-016 ISP discipline (1 trait = 1 capability ·
//! synthetic input is 1 concern · cross-platform pointer + keyboard event
//! synthesis · L1 impls split per OS backend · macOS `CGEvent` · Windows
//! `SendInput` · Linux `uinput` / wayland virtual-keyboard). ADR-037
//! bottom-up layer (L0.5 traits-only · zero tokio · zero OS-API crates ·
//! those land in L1 at Phase 2 M2 `nika-input`).
//!
//! Type-state pattern (per rust-architect 09 enrichment · simplified v1
//! single-axis · 2-phantom-axes ratchet candidate Phase 2 M2 L1
//! `nika-input` when macOS `AccessibilityEnabled` + `InputMonitoring`
//! split surfaces empirically) ·
//!
//! ```text
//! ConsentState (trait · type-level marker)
//!   ├── Unconfirmed   (Granted = std::convert::Infallible · no value can be constructed · trait methods un-callable at compile time)
//!   └── Authorized    (Granted = ConsentProof · holds granted_at_ns + ttl_ns)
//! ```
//!
//! `InputDevice<S: ConsentState>` is generic on the consent state · each
//! mutating method requires `&S::Granted` (the consent proof). With
//! `S = Unconfirmed`, `S::Granted = Infallible` so no caller can supply
//! the argument · the type system enforces that consent MUST be acquired
//! (state transitioned to `Authorized`) before any synthetic input event
//! is dispatched. Anti-`unwrap` discipline applies to the L1 impl ·
//! consent expiry (`ttl_ns`) is checked at the call site · returns
//! `InputError::ConsentExpired` (NIKA-1302) on stale proof.
//!
//! Cross-PR dep · `Point` is defined inline (M1.4 standalone per sprint
//! plan §5) · the screen-capture `Rect` already defines `x: i32, y: i32`
//! origin · same integer-coordinate canon (cohérent
//! `no-legacy-no-back-compat.md` Class 1 single geometry type · `Point`
//! is the cursor-position sibling of `Rect`).
//!
//! `KeyCode` enum exhaustivity decision (sprint plan §10 Risks +
//! Open-Q #1) · ~66 variants ship inline (A-Z + 0-9 + F1-F12 + arrows +
//! 6 control + 8 modifier) · covers 99% real-world key surface ·
//! single-canonical-enum per `no-legacy-no-back-compat.md` Class 1 ·
//! `#[non_exhaustive]` preserves additive ratchet on MINOR for
//! platform-specific keys (e.g. media keys · function-row F13+ on
//! extended keyboards) per `olympus-error-codes-canon.md` §2 contract
//! item 2 spirit applied to public enum forward-compat. The 66-variant
//! count exceeds sprint plan « ~30 common keys » estimate (Open-Q #1
//! resolved by going wider · pragmatic for 1-source-of-truth keyboard
//! surface · `clippy::large_enum_variant` N/A · all variants zero-size).

use serde::{Deserialize, Serialize};

/// Type-state marker · sealed via `ConsentState::Granted` associated type.
///
/// `Unconfirmed` ships `Granted = Infallible` (no value constructible ·
/// method dispatch un-callable at compile time). `Authorized` ships
/// `Granted = ConsentProof` (real value · supplied at call site). The
/// trait is the canonical sealing primitive · L1 impls MUST NOT add
/// new states without an ADR amendment per `no-legacy-no-back-compat.md`
/// Class 1 single-canonical-enum (states are an enum-of-types).
///
/// Per rust-architect 09 enrichment · v1 ships single-axis `ConsentProof` ·
/// 2-phantom-axes ratchet (macOS `AccessibilityEnabled` + `InputMonitoring`
/// distinct entitlements) deferred Phase 2 M2 L1 `nika-input` when
/// platform-specific friction surfaces empirically (NUKE-LEGACY · ship
/// the simplest thing that surfaces the pattern per `socratic-research-
/// discipline.md` §5c Option D).
pub trait ConsentState {
    /// The value carried by an Authorized state · `Infallible` when
    /// Unconfirmed · `ConsentProof` when Authorized.
    ///
    /// `Send + Sync` bound · enables `&S::Granted` proof references to
    /// cross async task boundaries (cohérent CANCEL SAFETY · L1 impls
    /// wrap synchronous OS event syscalls in `spawn_blocking` and pass
    /// the proof reference into the worker). `Infallible` and
    /// `ConsentProof` both satisfy the bound automatically · primitive
    /// fields · `Copy + Send + Sync` by default.
    type Granted: Send + Sync;
}

/// Type-state · consent has not been acquired · `Granted = Infallible`
/// so trait methods cannot be called.
///
/// `#[non_exhaustive]` per Invariant #25 + `olympus-error-codes-canon.md`
/// §2 contract item 2 spirit · downstream code never field-literals
/// (state markers are unit structs · field-add-on-MINOR is moot but
/// the attribute documents the forward-compat intent).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Unconfirmed;

impl ConsentState for Unconfirmed {
    type Granted = std::convert::Infallible;
}

/// Type-state · consent has been acquired · `Granted = ConsentProof`
/// holds the granted-at timestamp + TTL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Authorized;

impl ConsentState for Authorized {
    type Granted = ConsentProof;
}

/// Consent proof DTO · supplied by the L1 impl after the user grants
/// accessibility / input-monitoring entitlements (macOS) · UI Access
/// uiAccess manifest (Windows) · uinput group membership + udev rules
/// (Linux).
///
/// `granted_at_ns` · monotonic wall-clock nanoseconds at which the user
/// granted consent (cohérent `io::clock` canonical timestamp shape ·
/// `u64` for forward-compat ratchet beyond 2554 AD).
///
/// `ttl_ns` · time-to-live in nanoseconds · `0` means « infinite · until
/// system revocation » · non-zero means « expire after N ns since
/// `granted_at_ns` ». L1 impls MUST re-check `ttl_ns` at every dispatch
/// site · stale proof returns `InputError::ConsentExpired` (NIKA-1302).
///
/// `#[non_exhaustive]` preserves forward-compat for future fields (e.g.
/// `scope: ConsentScope` · `entitlements: u8` bitfield) on MINOR per
/// `olympus-error-codes-canon.md` §2 contract item 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ConsentProof {
    /// Monotonic nanoseconds at which consent was granted.
    pub granted_at_ns: u64,
    /// Time-to-live in nanoseconds · `0` = infinite (until system revoke).
    pub ttl_ns: u64,
}

impl ConsentProof {
    /// Construct a new consent proof.
    ///
    /// Per Invariant #19 · every `#[non_exhaustive]` struct ships a
    /// `new()` constructor so downstream code never field-literals.
    #[must_use]
    pub const fn new(granted_at_ns: u64, ttl_ns: u64) -> Self {
        Self {
            granted_at_ns,
            ttl_ns,
        }
    }
}

/// Mouse button · canonical 3-button taxonomy.
///
/// `#[non_exhaustive]` preserves additive ratchet for extended buttons
/// (e.g. `Back` · `Forward` · `Side1..N` on gaming mice) on MINOR.
/// Today's L1 ships the 3 canonical buttons covering 99% real-world
/// surface · per `socratic-research-discipline.md` §5c Option D ship
/// minimal viable subset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum MouseButton {
    /// Left primary button.
    Left,
    /// Right secondary button.
    Right,
    /// Middle button (scroll-wheel click).
    Middle,
}

/// Keyboard modifier set · 4 canonical modifiers in field-order
/// `cmd · shift · alt · ctrl`.
///
/// Field order matches macOS canonical event-record layout (cmd-first
/// per Apple HIG) · serde-determinism preserved by struct-field iteration.
/// `Default` derive ships the all-`false` state (no modifiers pressed) ·
/// canonical for the « bare keypress » call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[non_exhaustive]
#[allow(
    clippy::struct_excessive_bools,
    reason = "canonical keyboard modifier set · 4 mods is the keyboard reality \
              (cmd · shift · alt · ctrl) · per `no-legacy-no-back-compat.md` \
              Class 1 single canonical shape · NOT a flag-set anti-pattern"
)]
pub struct KeyMods {
    /// Command / Meta / Super / Windows key.
    pub cmd: bool,
    /// Shift key (either side).
    pub shift: bool,
    /// Alt / Option key (either side).
    pub alt: bool,
    /// Control key (either side).
    pub ctrl: bool,
}

impl KeyMods {
    /// Construct a new key-modifier set.
    ///
    /// Per Invariant #19 · every `#[non_exhaustive]` struct ships a
    /// `new()` constructor.
    #[must_use]
    #[allow(
        clippy::fn_params_excessive_bools,
        reason = "constructor for canonical KeyMods · 4 bools mirror keyboard \
                  reality · cohérent struct shape per Invariant #19"
    )]
    pub const fn new(cmd: bool, shift: bool, alt: bool, ctrl: bool) -> Self {
        Self {
            cmd,
            shift,
            alt,
            ctrl,
        }
    }
}

/// Screen-coordinate point · canonical pixel-integer pair.
///
/// `x: i32, y: i32` per `io::screen::Rect` canonical origin (cohérent
/// `no-legacy-no-back-compat.md` Class 1 single geometry type · integer-
/// only · no `f32`/`f64` to preserve `Eq` derive · pixel-perfect
/// addressing). Origin top-left (cohérent `CGImage` + Win32 GDI + X11/
/// Wayland canonical · L1 impls handle the macOS bottom-left CG flip at
/// the syscall boundary).
///
/// `#[non_exhaustive]` preserves additive ratchet for future fields
/// (e.g. `display_id: Option<u32>` for multi-monitor disambiguation) on
/// MINOR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Point {
    /// Horizontal pixel offset from origin (top-left).
    pub x: i32,
    /// Vertical pixel offset from origin (top-left).
    pub y: i32,
}

impl Point {
    /// Construct a new point.
    ///
    /// Per Invariant #19 · every `#[non_exhaustive]` struct ships a
    /// `new()` constructor.
    #[must_use]
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// Keyboard key · canonical ~66-variant subset covering 99% real-world
/// surface · letter A-Z + digit 0-9 + function F1-F12 + 4 arrows + 6
/// control + 8 modifiers.
///
/// `#[non_exhaustive]` preserves additive ratchet for platform-specific
/// keys (media keys · F13+ extended-function · numeric-keypad · IME
/// keys) on MINOR per `no-legacy-no-back-compat.md` Class 1 single-
/// canonical-enum discipline.
///
/// `Copy + Eq + Hash` derives are sound (zero-size variants · no `f32`/
/// `f64` payloads · all carriers are pure tags). `serde` round-trips
/// canonical variant names (`PascalCase` per Rust convention · L1 impls
/// map to platform key-code u16/u32 via lookup table).
///
/// Pragmatic decision · sprint plan §4 PR 4 cited « ~30 common keys » ·
/// shipping 66 covers the full keyboard surface in one canonical place ·
/// avoids parallel-taxonomy violation (Class 1) when L1 impls need media
/// or extended-function keys later. `clippy::too_many_variants` triggers
/// at default threshold 200 · 66 is well under.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum KeyCode {
    // Letters · A-Z
    /// A
    A,
    /// B
    B,
    /// C
    C,
    /// D
    D,
    /// E
    E,
    /// F
    F,
    /// G
    G,
    /// H
    H,
    /// I
    I,
    /// J
    J,
    /// K
    K,
    /// L
    L,
    /// M
    M,
    /// N
    N,
    /// O
    O,
    /// P
    P,
    /// Q
    Q,
    /// R
    R,
    /// S
    S,
    /// T
    T,
    /// U
    U,
    /// V
    V,
    /// W
    W,
    /// X
    X,
    /// Y
    Y,
    /// Z
    Z,

    // Digits · 0-9 (D-prefix · `D` avoids reserved-keyword collision with
    // Rust integer literal lexer ambiguity in future macro contexts).
    /// 0
    D0,
    /// 1
    D1,
    /// 2
    D2,
    /// 3
    D3,
    /// 4
    D4,
    /// 5
    D5,
    /// 6
    D6,
    /// 7
    D7,
    /// 8
    D8,
    /// 9
    D9,

    // Function keys · F1-F12
    /// F1
    F1,
    /// F2
    F2,
    /// F3
    F3,
    /// F4
    F4,
    /// F5
    F5,
    /// F6
    F6,
    /// F7
    F7,
    /// F8
    F8,
    /// F9
    F9,
    /// F10
    F10,
    /// F11
    F11,
    /// F12
    F12,

    // Arrow keys
    /// Arrow up.
    ArrowUp,
    /// Arrow down.
    ArrowDown,
    /// Arrow left.
    ArrowLeft,
    /// Arrow right.
    ArrowRight,

    // Control keys
    /// Enter / Return.
    Enter,
    /// Escape.
    Escape,
    /// Tab.
    Tab,
    /// Space.
    Space,
    /// Backspace.
    Backspace,
    /// Delete (forward delete).
    Delete,

    // Modifier keys (left / right discriminated · L1 impls coalesce as
    // needed · canonical taxonomy preserves side info for accessibility
    // tooling that needs to distinguish e.g. left-shift vs right-shift).
    /// Left shift.
    LShift,
    /// Right shift.
    RShift,
    /// Left control.
    LCtrl,
    /// Right control.
    RCtrl,
    /// Left alt / option.
    LAlt,
    /// Right alt / option.
    RAlt,
    /// Left command / meta / super / Windows.
    LCmd,
    /// Right command / meta / super / Windows.
    RCmd,
}

/// Synthetic-input errors — the typed boundary of the `InputDevice` trait
/// (Pattern A · FCI-023bis). `#[non_exhaustive]`; `NikaErrorCode` impl + reserved
/// range (`Category::Input` · NIKA-1301..1305) live in `crate::errors`. The error
/// type is orthogonal to the `ConsentState` type-state — a type-state-generic
/// trait is no exception to the typed-error convention (Pattern A · FCI-023bis).
///
/// SECURITY (ADR-081 Guard 1): an `InputError` MUST NEVER carry the raw typed
/// text — a synthetic-input error that logged what it typed would leak passwords.
/// Variants carry only structural detail (reason strings · never key content).
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum InputError {
    /// Synthetic input attempted without an OS consent grant.
    #[error("NIKA-1301 · synthetic input consent denied (grant Accessibility / Input Monitoring)")]
    ConsentDenied,
    /// The `ConsentProof` TTL expired before this dispatch (re-acquire consent).
    #[error("NIKA-1302 · synthetic input consent expired (proof TTL elapsed)")]
    ConsentExpired,
    /// Posting the synthetic OS event failed.
    #[error("NIKA-1303 · synthetic event post failed: {reason}")]
    EventPostFailed {
        /// Backend-reported reason (NEVER the typed content · Guard 1).
        reason: String,
    },
    /// No synthetic-input backend is compiled for this platform.
    #[error("NIKA-1304 · no synthetic-input backend on this platform")]
    BackendUnavailable,
    /// A `spawn_blocking` input-dispatch task panicked or was cancelled.
    #[error("NIKA-1305 · synthetic input task join failed: {reason}")]
    TaskJoinFailed {
        /// Join failure detail.
        reason: String,
    },
}

/// Synthetic input device · async trait gated by type-state consent.
///
/// CANCEL SAFETY · async methods are BEST-EFFORT cancel-safe with one
/// exception · `type_text` may produce PARTIAL INPUT if cancelled
/// mid-string (multi-character strings emit one synthetic key event per
/// code point · cancel mid-stream leaves the target with a partial
/// substring). See per-method CANCEL SAFETY blocks for exact semantics.
///
/// Other methods (`move_cursor` · `click` · `key_press`) are best-effort ·
/// once the synthetic event is queued at the kernel layer · dropping the
/// future abandons the post-queue synchronous wait but the event itself
/// proceeds. L1 impls SHOULD wrap the OS call in a `spawn_blocking` +
/// cancel token shim because macOS `CGEvent` · Windows `SendInput` ·
/// Linux `uinput` APIs are synchronous. Once the OS has queued the
/// event (post-syscall) cancel becomes a no-op · the synthetic event is
/// committed to the input pipeline · this is documented per-method.
///
/// `#[trait_variant::make(InputDeviceDyn: Send)]` generates a `Send`
/// companion trait `InputDeviceDyn` for generic constraints (cohérent
/// `io::screen::ScreenCaptureDyn` + `io::ocr::OcrEngineDyn` +
/// `io::a11y::AccessibilityTreeDyn` pattern · ADR-006 + canonical L0.5
/// shape). Downstream code uses `T: InputDeviceDyn<S>` for monomorphized
/// hot paths. Per `trait_variant` 0.1 semantics · the companion uses
/// `impl Future + Send` returns which are NOT dyn-compatible · this is
/// intentional (kernel traits stay zero-cost · L1 impls wrap via
/// `Arc<T>` not `Arc<dyn _>`).
///
/// Type-state generic `S: ConsentState` · each mutating method bounds
/// `S: ConsentState<Granted = ConsentProof>` · with `S = Unconfirmed`
/// the bound fails (`Granted = Infallible`) and the method is
/// un-callable at compile time. Caller acquires consent via the L1
/// impl's state-transition method (e.g. `request_consent()` → returns
/// `InputDevice<Authorized>`) before any synthetic event is dispatched.
///
/// Per rust-architect 09 enrichment · 2-phantom-axes ratchet candidate
/// Phase 2 M2 L1 `nika-input` when macOS `AccessibilityEnabled` vs
/// `InputMonitoring` distinct entitlements surface empirically · v1
/// ships single-axis `ConsentProof` per `socratic-research-discipline.md`
/// §5c Option D minimal viable subset.
#[trait_variant::make(InputDeviceDyn: Send)]
pub trait InputDevice<S: ConsentState>: Send + Sync {
    /// Move the cursor to absolute screen-coordinate `to`.
    ///
    /// CANCEL SAFETY: best-effort · once the synthetic move event is
    /// queued OS-side it commits regardless of cancel. Cancel before
    /// the syscall completes is safe (no event emitted).
    ///
    /// `proof` MUST be a non-expired `ConsentProof` (per `Authorized`
    /// state) · L1 impls re-check `ttl_ns` at call site · returns
    /// `InputError::ConsentExpired` (NIKA-1302) on stale proof.
    async fn move_cursor(&self, to: Point, proof: &S::Granted) -> Result<(), InputError>
    where
        S: ConsentState<Granted = ConsentProof>;

    /// Click a mouse button at the current cursor position.
    ///
    /// CANCEL SAFETY: best-effort · once the synthetic click event is
    /// queued OS-side it commits regardless of cancel. L1 impls SHOULD
    /// emit `ButtonPress` + `ButtonRelease` as a single atomic pair.
    ///
    /// `proof` MUST be a non-expired `ConsentProof` · same contract as
    /// `move_cursor`.
    async fn click(&self, button: MouseButton, proof: &S::Granted) -> Result<(), InputError>
    where
        S: ConsentState<Granted = ConsentProof>;

    /// Type the UTF-8 text via synthetic key events.
    ///
    /// CANCEL SAFETY: PARTIAL-INPUT risk on cancel · multi-character
    /// strings emit one synthetic key event per code point · cancel
    /// mid-stream leaves the target with a partial substring. Callers
    /// SHOULD treat this as best-effort and verify post-state (or wrap
    /// the call in a transactional UI primitive). This is documented
    /// behavior · not a bug · per sprint plan §4 PR 4 CANCEL SAFETY
    /// risk-acknowledged.
    ///
    /// `proof` MUST be a non-expired `ConsentProof` · same contract as
    /// `move_cursor`.
    async fn type_text(&self, text: &str, proof: &S::Granted) -> Result<(), InputError>
    where
        S: ConsentState<Granted = ConsentProof>;

    /// Press a keyboard key with the given modifier set.
    ///
    /// CANCEL SAFETY: best-effort · once the synthetic key event is
    /// queued OS-side it commits regardless of cancel. L1 impls SHOULD
    /// emit `KeyDown` + `KeyUp` as a single atomic pair · modifiers
    /// applied first (e.g. cmd-down · then key-down · then key-up ·
    /// then cmd-up).
    ///
    /// `proof` MUST be a non-expired `ConsentProof` · same contract as
    /// `move_cursor`.
    async fn key_press(
        &self,
        key: KeyCode,
        modifiers: KeyMods,
        proof: &S::Granted,
    ) -> Result<(), InputError>
    where
        S: ConsentState<Granted = ConsentProof>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `ConsentProof::new` constructs with `ttl_ns = 0` (infinite TTL ·
    /// the canonical « no expiry » shape) · proves the constructor
    /// surface + field-shape per Invariant #19.
    #[test]
    fn consent_proof_default_ttl_zero() {
        let proof = ConsentProof::new(1_000, 0);
        assert_eq!(proof.granted_at_ns, 1_000);
        assert_eq!(proof.ttl_ns, 0);
    }

    /// `KeyMods::default()` produces the all-`false` state · canonical
    /// « no modifiers pressed » shape · proves `Default` derive matches
    /// the field-zero contract.
    #[test]
    fn key_mods_default_all_false() {
        let mods = KeyMods::default();
        assert!(!mods.cmd);
        assert!(!mods.shift);
        assert!(!mods.alt);
        assert!(!mods.ctrl);
    }

    /// `Point::new(0, 0)` exhibits canonical equality + hashability ·
    /// proves `Eq + Hash` derive (sound · integer-only fields per
    /// `no-legacy-no-back-compat.md` Class 1 single geometry type).
    /// Hash bound is asserted via a zero-cost generic-bound helper ·
    /// no `HashMap` use (banned workspace-wide by `clippy.toml`
    /// `disallowed-types` per DEV-3 lesson · `BTreeMap` is canonical).
    #[test]
    fn point_eq_hash() {
        fn _assert_hash<T: std::hash::Hash>() {}
        _assert_hash::<Point>();
        let a = Point::new(0, 0);
        let b = Point::new(0, 0);
        let c = Point::new(1, 0);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    /// `Authorized` type-state ships `Granted = ConsentProof` · compile-
    /// time check via a generic function that requires the bound. If
    /// this compiles · the type-state contract is well-formed for the
    /// trait-method `where S: ConsentState<Granted = ConsentProof>`
    /// clauses. Pure compile-time check · no runtime path.
    #[test]
    fn authorized_type_state_phantom() {
        fn _accepts_authorized_granted<S: ConsentState<Granted = ConsentProof>>() {}
        _accepts_authorized_granted::<Authorized>();
    }

    /// `InputDevice` + `InputDeviceDyn` (the `Send` companion generated
    /// by `trait_variant::make`) are well-formed as generic bounds
    /// parameterized on the consent state.
    ///
    /// Per DEV-2 M1.1 lesson · `trait_variant` 0.1 returns
    /// `impl Future + Send` which is NOT dyn-compatible. Use a
    /// generic-bound test (`T: InputDeviceDyn<S>`) NOT a trait-object
    /// test. Pure compile-time check · no runtime path. If this
    /// compiles · the trait bounds are well-formed with both type-state
    /// markers.
    #[test]
    fn input_device_generic_bound_compile_check() {
        fn _accepts_input_device<S: ConsentState, T: InputDevice<S>>() {}
        fn _accepts_input_device_dyn<S: ConsentState, T: InputDeviceDyn<S>>() {}
    }

    /// `ConsentProof` + DTOs round-trip through serde JSON · proves the
    /// canonical shape serializes cleanly (downstream consumers in
    /// `io::journal` + projection layers need stable JSON form).
    #[test]
    fn dtos_serde_roundtrip() {
        let proof = ConsentProof::new(123_456_789, 60_000_000_000);
        let json = serde_json::to_string(&proof).expect("serialize consent proof");
        let back: ConsentProof = serde_json::from_str(&json).expect("deserialize consent proof");
        assert_eq!(back, proof);

        let mods = KeyMods::new(true, false, true, false);
        let json = serde_json::to_string(&mods).expect("serialize key mods");
        let back: KeyMods = serde_json::from_str(&json).expect("deserialize key mods");
        assert_eq!(back, mods);

        let point = Point::new(42, -17);
        let json = serde_json::to_string(&point).expect("serialize point");
        let back: Point = serde_json::from_str(&json).expect("deserialize point");
        assert_eq!(back, point);
    }

    /// `KeyCode` exhibits canonical variant distinctness · 5 high-signal
    /// variants (`A` · `D0` · `F1` · `ArrowUp` · `LCmd`) verified distinct under
    /// equality + hashable for use as map keys.
    #[test]
    fn key_code_variant_distinctness() {
        assert_ne!(KeyCode::A, KeyCode::D0);
        assert_ne!(KeyCode::A, KeyCode::F1);
        assert_ne!(KeyCode::F1, KeyCode::ArrowUp);
        assert_ne!(KeyCode::ArrowUp, KeyCode::LCmd);
        assert_ne!(KeyCode::LCmd, KeyCode::RCmd);
        // Round-trip serde to prove serialization shape stable.
        let key = KeyCode::F12;
        let json = serde_json::to_string(&key).expect("serialize key code");
        let back: KeyCode = serde_json::from_str(&json).expect("deserialize key code");
        assert_eq!(back, key);
    }

    /// `MouseButton` exhibits canonical 3-variant distinctness + serde
    /// round-trip · proves `Copy + Eq + Hash + Serialize + Deserialize`
    /// derive consistency.
    #[test]
    fn mouse_button_variants() {
        assert_ne!(MouseButton::Left, MouseButton::Right);
        assert_ne!(MouseButton::Right, MouseButton::Middle);
        let btn = MouseButton::Middle;
        let json = serde_json::to_string(&btn).expect("serialize mouse button");
        let back: MouseButton = serde_json::from_str(&json).expect("deserialize mouse button");
        assert_eq!(back, btn);
    }

    /// All public DTOs are Send + Sync · cohérent CANCEL SAFETY · types
    /// cross task boundaries in async L1 impls (`spawn_blocking` wraps).
    #[test]
    fn dtos_are_send_sync() {
        fn _assert<T: Send + Sync>() {}
        _assert::<Unconfirmed>();
        _assert::<Authorized>();
        _assert::<ConsentProof>();
        _assert::<MouseButton>();
        _assert::<KeyMods>();
        _assert::<Point>();
        _assert::<KeyCode>();
    }
}
