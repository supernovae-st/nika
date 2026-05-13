// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Screen capture traits — L0.5 sealed DTOs + async trait.
//!
//! Phase 2 M1 PR 1 (sprint plan `2026-05-14-nika-phase-2-m1-kernel-
//! modules-sprint-plan.md` §4). Reserved error codes NIKA-1000..1019
//! per `docs/architecture/forward-compat-invariants.md` Gate 12.
//!
//! ADR-006 monolithic-kernel-spirit (single trait + DTOs · no proc
//! macro · no async runtime dep). ADR-016 ISP discipline (1 trait =
//! 1 capability). ADR-037 bottom-up layer (L0.5 traits-only · zero
//! tokio · zero reqwest). GUI-ready Day-1 per BLUEPRINT §5 ·
//! `bytes::Bytes` zero-copy RGBA8 frame payloads · `serde` derives
//! on every DTO so the projection layer (S33+) and cockpit IPC bridge
//! can ship the frame across process boundaries without re-encoding.

use bytes::Bytes;
use serde::{Deserialize, Serialize};

/// Stable display identifier — opaque integer assigned by the host OS.
///
/// Hash + Eq + Ord enable use as a key in maps + sorted iteration.
/// `#[non_exhaustive]` on the tuple-struct keeps the field private to
/// the constructor + accessor while preserving forward-compat on the
/// inner-type ratchet (per Invariant #19 + Gate 12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[non_exhaustive]
pub struct DisplayId(pub u32);

/// Display geometry + identity metadata.
///
/// All fields are public-read via destructuring `let DisplayInfo { .. } = ...`
/// but the `#[non_exhaustive]` ratchet forbids exhaustive pattern matches
/// from downstream consumers · they MUST use the wildcard `..` arm. New
/// fields land additively on MINOR per `no-legacy-no-back-compat.md`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct DisplayInfo {
    /// Stable display identifier.
    pub id: DisplayId,
    /// Human-readable display name (e.g. "Built-in Retina").
    pub name: String,
    /// Width in physical pixels.
    pub width: u32,
    /// Height in physical pixels.
    pub height: u32,
    /// DPI scale factor (1.0 = standard · 2.0 = Retina · etc.).
    pub scale_factor: f32,
    /// Whether this is the primary / main display.
    pub is_primary: bool,
}

impl DisplayInfo {
    /// Construct a new display-info record.
    ///
    /// Per Invariant #19 · every `#[non_exhaustive]` struct ships a
    /// `new()` constructor so downstream code never field-literals.
    #[must_use]
    pub fn new(
        id: DisplayId,
        name: String,
        width: u32,
        height: u32,
        scale_factor: f32,
        is_primary: bool,
    ) -> Self {
        Self {
            id,
            name,
            width,
            height,
            scale_factor,
            is_primary,
        }
    }
}

/// Rectangular region in physical-pixel coordinates.
///
/// Origin top-left (cohérent macOS Quartz + Windows GDI · NOT OpenGL
/// bottom-left). Used to drive partial-screen capture regions and
/// dirty-rect propagation when the projection layer ships diff frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Rect {
    /// X offset from display origin (top-left).
    pub x: i32,
    /// Y offset from display origin (top-left).
    pub y: i32,
    /// Width in physical pixels.
    pub width: u32,
    /// Height in physical pixels.
    pub height: u32,
}

impl Rect {
    /// Construct a new rectangle.
    #[must_use]
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

/// Captured frame — zero-copy RGBA8 payload + frame metadata.
///
/// `pixels: bytes::Bytes` is the canonical zero-copy carrier per
/// BLUEPRINT §5 · arc-shared + cheap clone + serde via the workspace
/// `bytes = { features = ["serde"] }` feature flip (sprint plan §7
/// Risks). Frame DTO ships across IPC + projection + cockpit without
/// re-encoding.
///
/// `PartialEq` only (no `Eq`) because `scale_factor: f32` lacks total
/// equality. Frame comparison in tests + integration uses structural
/// `==` which is sufficient for the projection-layer diff semantics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Frame {
    /// Frame width in physical pixels.
    pub width: u32,
    /// Frame height in physical pixels.
    pub height: u32,
    /// DPI scale factor (matches `DisplayInfo::scale_factor` at capture).
    pub scale_factor: f32,
    /// Zero-copy RGBA8 pixel buffer · `width * height * 4` bytes.
    pub pixels: Bytes,
    /// Source display identifier.
    pub display_id: DisplayId,
    /// Monotonic capture timestamp (milliseconds since UNIX epoch).
    pub captured_at_ms: u64,
}

impl Frame {
    /// Construct a new frame.
    ///
    /// Per Invariant #19 · ergonomic constructor for `#[non_exhaustive]`.
    /// Callers SHOULD pre-size `pixels` to `width * height * 4` · this
    /// constructor does NOT validate the invariant at runtime (zero-cost
    /// L0.5 discipline · validation belongs in L1 impls).
    #[must_use]
    pub fn new(
        width: u32,
        height: u32,
        scale_factor: f32,
        pixels: Bytes,
        display_id: DisplayId,
        captured_at_ms: u64,
    ) -> Self {
        Self {
            width,
            height,
            scale_factor,
            pixels,
            display_id,
            captured_at_ms,
        }
    }
}

/// Screen-capture capability — async trait over a single display.
///
/// CANCEL SAFETY: the design contract MUST be evaluated per-method
/// because OS capture APIs differ on cancellation behavior (CoreGraphics
/// `CGDisplayCreateImage` is sync · macOS `ScreenCaptureKit` is push-async
/// · Wayland portals queue requests). See individual method docs.
///
/// `#[trait_variant::make(ScreenCaptureDyn: Send)]` generates a `Send`
/// companion trait `ScreenCaptureDyn` for generic constraints (cohérent
/// ADR-006 + `io::fs::FsRead` pattern). Downstream code uses
/// `T: ScreenCaptureDyn` for monomorphized hot paths. Per `trait_variant`
/// 0.1 semantics · the companion uses `impl Future + Send` returns
/// which are NOT dyn-compatible · this is intentional (kernel traits
/// stay zero-cost · L1 impls wrap via `Arc<T>` not `Arc<dyn _>`).
#[trait_variant::make(ScreenCaptureDyn: Send)]
pub trait ScreenCapture: Send + Sync {
    /// Enumerate connected displays.
    ///
    /// CANCEL SAFETY: cancel-safe (read-only OS query · no side
    /// effects). Drop semantics · the future abandons the syscall.
    async fn list_displays(&self) -> std::io::Result<Vec<DisplayInfo>>;

    /// Capture a full-display frame.
    ///
    /// CANCEL SAFETY: best-effort cancel-safe at the syscall boundary ·
    /// L1 impls SHOULD wrap the OS call in a `spawn_blocking` + cancel
    /// token shim so a dropped future surrenders the worker promptly.
    /// Partial frame data MUST NOT leak to a caller on cancel (L1 impl
    /// invariant · enforced by integration tests at Phase 3 M3).
    async fn capture_full(&self, display: DisplayId) -> std::io::Result<Frame>;

    /// Capture a sub-region of a display.
    ///
    /// CANCEL SAFETY: same contract as `capture_full` · partial regional
    /// reads MUST NOT surface partial frames. `region` coordinates are
    /// physical-pixel relative to the display origin (top-left).
    async fn capture_region(&self, display: DisplayId, region: Rect) -> std::io::Result<Frame>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Frame DTO round-trips through serde JSON · proves the `bytes`
    /// workspace flip (sprint plan §7 Risks) is wired correctly.
    #[test]
    fn frame_dto_serialize_roundtrip() {
        let frame = Frame::new(
            1920,
            1080,
            1.0,
            Bytes::from_static(&[0u8; 16]),
            DisplayId(0),
            1000,
        );
        let json = serde_json::to_string(&frame).expect("serialize frame");
        let back: Frame = serde_json::from_str(&json).expect("deserialize frame");
        assert_eq!(back, frame);
    }

    /// `DisplayId` is `Hash` + `Eq` · usable as map key.
    ///
    /// Uses `BTreeMap` instead of `HashMap` per workspace `clippy.toml`
    /// `disallowed-types` (`HashMap` → `rustc_hash::FxHashMap` for prod
    /// · `BTreeMap` is fine for tests · zero new dep).
    #[test]
    fn display_id_hash_eq() {
        use std::collections::BTreeMap;
        let mut m: BTreeMap<DisplayId, &'static str> = BTreeMap::new();
        m.insert(DisplayId(0), "primary");
        m.insert(DisplayId(1), "secondary");
        assert_eq!(m.get(&DisplayId(0)), Some(&"primary"));
        assert_eq!(m.get(&DisplayId(1)), Some(&"secondary"));
        assert_eq!(DisplayId(0), DisplayId(0));
        assert_ne!(DisplayId(0), DisplayId(1));
    }

    /// `ScreenCapture` + `ScreenCaptureDyn` (the `Send` companion
    /// generated by `trait_variant::make`) are well-formed as generic
    /// bounds. Per `io::fs::FsRead` canonical pattern · kernel async
    /// traits ship `impl Future + Send` returns (NOT dyn-compatible
    /// by design · zero-cost L0.5 discipline · L1 impls use `Arc<T>`
    /// for trait-object polymorphism).
    #[test]
    fn screen_capture_trait_object_safe() {
        fn _accepts_screen_capture<T: ScreenCapture>() {}
        fn _accepts_screen_capture_dyn<T: ScreenCaptureDyn>() {}
        // Pure compile-time check · no runtime path. If this compiles
        // the trait bounds are well-formed.
    }

    /// Rect constructor round-trips fields.
    #[test]
    fn rect_new_fields() {
        let r = Rect::new(10, 20, 800, 600);
        assert_eq!(r.x, 10);
        assert_eq!(r.y, 20);
        assert_eq!(r.width, 800);
        assert_eq!(r.height, 600);
    }

    /// `DisplayInfo` constructor round-trips fields.
    #[test]
    fn display_info_new_fields() {
        let d = DisplayInfo::new(
            DisplayId(2),
            "Studio Display".to_string(),
            5120,
            2880,
            2.0,
            true,
        );
        assert_eq!(d.id, DisplayId(2));
        assert_eq!(d.name, "Studio Display");
        assert_eq!(d.width, 5120);
        assert_eq!(d.height, 2880);
        assert!((d.scale_factor - 2.0).abs() < f32::EPSILON);
        assert!(d.is_primary);
    }

    /// All public DTOs are Send + Sync (cohérent CANCEL SAFETY · types
    /// cross task boundaries in async impls).
    #[test]
    fn dtos_are_send_sync() {
        fn _assert<T: Send + Sync>() {}
        _assert::<DisplayId>();
        _assert::<DisplayInfo>();
        _assert::<Rect>();
        _assert::<Frame>();
    }
}
