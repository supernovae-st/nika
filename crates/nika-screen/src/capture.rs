// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Screen-capture backend — implements the L0.5 `ScreenCapture` trait.
//!
//! **B.3 single-shot capture WIRED (behind the `xcap` feature).**
//! `list_displays` / `capture_full` / `capture_region` drive the real
//! cross-platform `xcap` backend (display enumeration + RGBA8 frame capture ·
//! macOS CoreGraphics · Linux X11/Wayland portal · Windows DXGI) when built
//! with `--features xcap`. `xcap` encapsulates all OS FFI internally (objc2 /
//! x11 / windows crates), so this crate stays `unsafe_code = forbid`-clean.
//!
//! The `xcap` calls are synchronous, so each method runs them inside
//! [`tokio::task::spawn_blocking`] — a dropped future surrenders the
//! worker promptly (the kernel CANCEL SAFETY contract). The OS handle
//! (`xcap::Monitor`, possibly `!Send` on macOS) is created, used, and
//! dropped entirely inside the blocking closure; only `Send` results
//! (`Frame`, `Vec<DisplayInfo>`) cross the `.await` boundary.
//!
//! `capture_stream` (B.4) drives a bounded `tokio::mpsc` channel fed by a
//! `spawn_blocking` capture worker; dropping the returned stream closes the
//! channel and the worker exits (drop-stop · no `CancellationToken` needed).
//! Per `skeleton-option-a-pattern.md` §5 the B.2 `BackendNotWired` placeholder
//! is now fully CLOSED across all four `ScreenCapture` methods.
//!
//! # Stub backend (default · no `xcap` feature)
//!
//! Without the `xcap` feature the crate compiles WITHOUT pulling `xcap` (and
//! its Wayland/X11/PipeWire/DRM/EGL system libs), so `cargo build --workspace`
//! works on a headless box with no display stack. The ADR-081 consent (guard 7)
//! and LED (guard 6) guards are STILL enforced — only the OS-call portion is
//! replaced by a [`ScreenError::BackendInit`] "backend not built" error.
//! Rebuild with `--features xcap` for real OS capture.

use std::sync::Arc;

#[cfg(feature = "xcap")]
use std::pin::Pin;
#[cfg(feature = "xcap")]
use std::task::{Context, Poll};
#[cfg(feature = "xcap")]
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(feature = "xcap")]
use bytes::Bytes;
#[cfg(feature = "xcap")]
use futures_core::Stream;
use nika_kernel::io::screen::{DisplayId, DisplayInfo, Frame, FrameStream, Rect, ScreenCaptureDyn};
#[cfg(feature = "xcap")]
use xcap::Monitor;
#[cfg(feature = "xcap")]
use xcap::image::RgbaImage;

use crate::error::ScreenError;
use crate::guards::{ConsentGate, LedIndicator};

/// Cross-platform screen-capture backend (driven by `xcap`).
///
/// Composes the ADR-081 capture guards · a fail-closed [`ConsentGate`]
/// (guard 7) and a shared [`LedIndicator`] (guard 6). `xcap` enumerates
/// monitors per call, so the only persistent state is the guard state.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct ScreenBackend {
    /// Capture-consent gate (ADR-081 guard 7) · fail-closed · session-scoped.
    /// `Arc` so the stream worker can re-check consent per frame (revoke).
    consent: Arc<ConsentGate>,
    /// Capture-LED indicator (ADR-081 guard 6) · shared so a stream worker can
    /// hold the engaged scope for the stream's whole lifetime.
    led: Arc<LedIndicator>,
}

impl ScreenBackend {
    /// Construct a new screen-capture backend (consent fail-closed · LED off).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Grant capture consent for the session (guard 7).
    ///
    /// Driven by the OS consent-dialog result at the deferred FFI layer; the
    /// pure-Rust gate records the session-scoped grant.
    pub fn grant_consent(&self) {
        self.consent.grant();
    }

    /// Revoke capture consent (guard 7) · single-shot captures are denied and a
    /// running stream stops (surfacing `ConsentRevoked`) at its next frame.
    pub fn revoke_consent(&self) {
        self.consent.revoke();
    }

    /// Whether the capture-LED indicator is currently lit (guard 6) · the
    /// consuming UI renders this.
    #[must_use]
    pub fn led_is_engaged(&self) -> bool {
        self.led.is_engaged()
    }
}

// The SEND variant is the impl target — the kernel's one-way blanket impl
// derives the local `ScreenCapture` from it (never the reverse), so
// `T: ScreenCaptureDyn` consumers (the documented downstream bound) and
// `tokio::spawn` both work. Bodies are Send: xcap runs in `spawn_blocking`.
impl ScreenCaptureDyn for ScreenBackend {
    async fn list_displays(&self) -> Result<Vec<DisplayInfo>, ScreenError> {
        // No consent check (enumeration is metadata-only). The stub returns the
        // "backend not built" error directly.
        #[cfg(feature = "xcap")]
        {
            let infos = tokio::task::spawn_blocking(list_displays_sync)
                .await
                .map_err(|e| join_err(&e))??;
            Ok(infos)
        }
        #[cfg(not(feature = "xcap"))]
        {
            Err(backend_unavailable())
        }
    }

    async fn capture_full(&self, display: DisplayId) -> Result<Frame, ScreenError> {
        self.consent.check()?; // guard 7 · fail-closed (before any OS call)
        let _led = LedIndicator::engage(&self.led); // guard 6 · RAII · off on drop
        #[cfg(feature = "xcap")]
        {
            let frame = tokio::task::spawn_blocking(move || capture_full_sync(display))
                .await
                .map_err(|e| join_err(&e))??;
            Ok(frame)
        }
        #[cfg(not(feature = "xcap"))]
        {
            let _ = display;
            Err(backend_unavailable())
        }
    }

    async fn capture_region(&self, display: DisplayId, region: Rect) -> Result<Frame, ScreenError> {
        self.consent.check()?; // guard 7 · fail-closed (before any OS call)
        let _led = LedIndicator::engage(&self.led); // guard 6 · RAII · off on drop
        #[cfg(feature = "xcap")]
        {
            let frame = tokio::task::spawn_blocking(move || capture_region_sync(display, region))
                .await
                .map_err(|e| join_err(&e))??;
            Ok(frame)
        }
        #[cfg(not(feature = "xcap"))]
        {
            let _ = display;
            let _ = region;
            Err(backend_unavailable())
        }
    }

    async fn capture_stream(&self, display: DisplayId) -> Result<FrameStream, ScreenError> {
        self.consent.check()?; // guard 7 · fail-closed (re-checked per frame below)
        #[cfg(feature = "xcap")]
        {
            // Fail fast · verify the display exists before spawning the worker
            // (keeps the !Send Monitor inside the blocking probe).
            tokio::task::spawn_blocking(move || find_monitor_sync(display.0).map(|_| ()))
                .await
                .map_err(|e| join_err(&e))??;

            // Bounded channel · backpressure paces the worker to the consumer.
            let (tx, rx) = tokio::sync::mpsc::channel::<Result<Frame, ScreenError>>(STREAM_BUFFER);

            // Long-lived capture worker on the tokio blocking pool (the sanctioned
            // tokio-first primitive · holds one pool slot for the stream's lifetime).
            // Drop-stop cancellation · when the consumer drops the returned stream
            // the channel closes, the next `blocking_send` fails, the worker exits
            // and frees the slot. A `CancellationToken` would add nothing — a sync
            // `capture_image` mid-flight cannot be interrupted regardless.
            let consent = Arc::clone(&self.consent);
            let led = Arc::clone(&self.led);
            tokio::task::spawn_blocking(move || stream_worker(display, &consent, &led, &tx));

            Ok(Box::pin(FrameRx { rx }))
        }
        #[cfg(not(feature = "xcap"))]
        {
            let _ = display;
            Err(backend_unavailable())
        }
    }
}

/// The "screen-capture backend not built" error returned by every stub method
/// when the crate is compiled WITHOUT the `xcap` feature. Reached only AFTER the
/// ADR-081 consent + LED guards (so guard behavior is identical in both configs).
#[cfg(not(feature = "xcap"))]
fn backend_unavailable() -> ScreenError {
    ScreenError::BackendInit {
        reason: "screen-capture backend not built — rebuild nika-screen with --features xcap"
            .to_owned(),
    }
}

// --- sync xcap helpers (run inside spawn_blocking · keep !Send Monitor local) ---
// Gated behind the `xcap` feature · the OS-coupled backend internals.

/// Map a `spawn_blocking` join failure (panic / cancel) into a transient
/// capture error.
#[cfg(feature = "xcap")]
fn join_err(e: &tokio::task::JoinError) -> ScreenError {
    // NEVER `format!("{e}")`: tokio's JoinError Display embeds the panic
    // PAYLOAD, which could carry captured pixel data if a backend panicked with
    // it. Distinguish the cause structurally; carry no payload.
    ScreenError::CaptureFailed {
        reason: if e.is_cancelled() {
            "capture task cancelled"
        } else {
            "capture task panicked (payload suppressed)"
        }
        .to_owned(),
    }
}

/// Map an `xcap` backend error into a transient `CaptureFailed`.
#[cfg(feature = "xcap")]
fn capture_failed(e: impl std::fmt::Display) -> ScreenError {
    ScreenError::CaptureFailed {
        reason: e.to_string(),
    }
}

/// Monotonic capture timestamp — nanoseconds since the UNIX epoch
/// (saturating · clock-skew + overflow tolerant · never panics).
#[cfg(feature = "xcap")]
fn now_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// Resolve a connected `xcap::Monitor` by its kernel `DisplayId`.
#[cfg(feature = "xcap")]
fn find_monitor_sync(id: u32) -> Result<Monitor, ScreenError> {
    let monitors = Monitor::all().map_err(|e| ScreenError::BackendInit {
        reason: e.to_string(),
    })?;
    if monitors.is_empty() {
        return Err(ScreenError::NoDisplaysFound);
    }
    for m in monitors {
        if m.id().map_err(capture_failed)? == id {
            return Ok(m);
        }
    }
    Err(ScreenError::DisplayNotFound { id })
}

/// Enumerate connected displays into kernel `DisplayInfo` records.
#[cfg(feature = "xcap")]
fn list_displays_sync() -> Result<Vec<DisplayInfo>, ScreenError> {
    let monitors = Monitor::all().map_err(|e| ScreenError::BackendInit {
        reason: e.to_string(),
    })?;
    let mut infos = Vec::with_capacity(monitors.len());
    for m in monitors {
        let id = m.id().map_err(capture_failed)?;
        let name = m.friendly_name().map_err(capture_failed)?;
        let width = m.width().map_err(capture_failed)?;
        let height = m.height().map_err(capture_failed)?;
        let scale = m.scale_factor().map_err(capture_failed)?;
        let is_primary = m.is_primary().map_err(capture_failed)?;
        infos.push(DisplayInfo::new(
            DisplayId::new(id),
            name,
            width,
            height,
            scale,
            is_primary,
        ));
    }
    Ok(infos)
}

/// Convert an `xcap` RGBA image into a kernel `Frame` (zero-copy payload).
#[cfg(feature = "xcap")]
fn rgba_to_frame(img: RgbaImage, scale: f32, display: DisplayId) -> Frame {
    let width = img.width();
    let height = img.height();
    let pixels = Bytes::from(img.into_raw());
    Frame::new(width, height, scale, pixels, display, now_ns())
}

/// Capture a full-display frame (sync · runs inside `spawn_blocking`).
#[cfg(feature = "xcap")]
fn capture_full_sync(display: DisplayId) -> Result<Frame, ScreenError> {
    let monitor = find_monitor_sync(display.0)?;
    let scale = monitor.scale_factor().map_err(capture_failed)?;
    let img = monitor.capture_image().map_err(capture_failed)?;
    Ok(rgba_to_frame(img, scale, display))
}

/// Validate a capture region against display bounds — **pure** (no OS call ·
/// display-local top-left coordinates). Returns the unsigned `(x, y)` origin,
/// or [`ScreenError::RegionOutOfBounds`] for a negative offset · a zero extent
/// · or an out-of-bounds extent (strictly `> display`; the exact boundary
/// `rx + width == display_w` is IN bounds).
///
/// Extracted from [`capture_region_sync`] so the bounds logic is unit- and
/// mutation-tested headless — the OS-coupled capture call stays `#[ignore]`
/// (needs a real display + TCC). ADR-003 gate 5/6 coverage of the pure path.
#[cfg(feature = "xcap")]
fn validate_region(
    region: Rect,
    display_w: u32,
    display_h: u32,
) -> Result<(u32, u32), ScreenError> {
    let oob = || ScreenError::RegionOutOfBounds {
        x: region.x,
        y: region.y,
        width: region.width,
        height: region.height,
        display_w,
        display_h,
    };
    let (Ok(rx), Ok(ry)) = (u32::try_from(region.x), u32::try_from(region.y)) else {
        return Err(oob());
    };
    if region.width == 0
        || region.height == 0
        || rx.saturating_add(region.width) > display_w
        || ry.saturating_add(region.height) > display_h
    {
        return Err(oob());
    }
    Ok((rx, ry))
}

/// Capture a sub-region frame (sync · runs inside `spawn_blocking`).
///
/// The region is validated against the display bounds by [`validate_region`]
/// (a negative offset or an out-of-bounds extent yields
/// [`ScreenError::RegionOutOfBounds`] · structural · non-transient).
#[cfg(feature = "xcap")]
fn capture_region_sync(display: DisplayId, region: Rect) -> Result<Frame, ScreenError> {
    let monitor = find_monitor_sync(display.0)?;
    let scale = monitor.scale_factor().map_err(capture_failed)?;
    let display_w = monitor.width().map_err(capture_failed)?;
    let display_h = monitor.height().map_err(capture_failed)?;
    let (rx, ry) = validate_region(region, display_w, display_h)?;
    let img = monitor
        .capture_region(rx, ry, region.width, region.height)
        .map_err(capture_failed)?;
    Ok(rgba_to_frame(img, scale, display))
}

// --- B.4 continuous capture stream (gated behind the `xcap` feature) ---

/// Bounded frame buffer · backpressure paces the worker to the consumer.
#[cfg(feature = "xcap")]
const STREAM_BUFFER: usize = 4;

/// Fixed ~30fps capture cadence for the B.4 stream. A configurable
/// frame-rate / change-detection policy is a later-round refinement.
#[cfg(feature = "xcap")]
const STREAM_FRAME_INTERVAL: Duration = Duration::from_millis(33);

/// Continuous capture loop · runs on a tokio blocking-pool thread.
///
/// Captures one full-display frame per `STREAM_FRAME_INTERVAL` and pushes it
/// (Ok or per-frame Err) to the consumer. Per the kernel contract a transient
/// per-frame error surfaces WITHOUT tearing down the stream — the consumer
/// decides whether to keep polling or drop. `blocking_send` returning `Err`
/// means the consumer dropped the receiver → the worker exits (drop-stop).
#[cfg(feature = "xcap")]
fn stream_worker(
    display: DisplayId,
    consent: &Arc<ConsentGate>,
    led: &Arc<LedIndicator>,
    tx: &tokio::sync::mpsc::Sender<Result<Frame, ScreenError>>,
) {
    // Guard 6 · the indicator stays lit for the whole stream (the scope drops
    // when this loop exits → disengage).
    let _led = LedIndicator::engage(led);
    loop {
        // Guard 7 · per-frame consent re-check · a mid-stream revoke surfaces
        // ConsentRevoked then tears the stream down.
        if let Err(e) = consent.check() {
            let _ = tx.blocking_send(Err(e));
            break;
        }
        let item = capture_full_sync(display);
        if tx.blocking_send(item).is_err() {
            break;
        }
        std::thread::sleep(STREAM_FRAME_INTERVAL);
    }
}

/// `Stream` adapter over the worker's bounded `mpsc::Receiver`.
///
/// Dropping this (the `FrameStream`) closes the channel; the worker's next
/// `blocking_send` fails and the capture thread exits (drop-stop · no separate
/// cancellation token needed). `futures_core::Stream` keeps this consistent
/// with the kernel `FrameStream` type alias.
#[cfg(feature = "xcap")]
struct FrameRx {
    rx: tokio::sync::mpsc::Receiver<Result<Frame, ScreenError>>,
}

#[cfg(feature = "xcap")]
impl Stream for FrameRx {
    type Item = Result<Frame, ScreenError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // `FrameRx` is `Unpin` (Receiver is Unpin) · safe to project via get_mut.
        self.get_mut().rx.poll_recv(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_kernel::prelude::{NikaErrorCode, codes};

    /// B.3 closure proof · `list_displays` no longer returns the B.2
    /// `BackendNotWired` skeleton. On a host with displays it enumerates
    /// real monitors (sane dimensions); on a headless / permission-denied
    /// host the OS call errors — but NEVER with NIKA-1000.
    #[tokio::test]
    async fn list_displays_no_longer_skeleton() {
        let backend = ScreenBackend::new();
        match backend.list_displays().await {
            Ok(displays) => {
                for d in &displays {
                    assert!(d.width > 0 && d.height > 0, "display has sane dimensions");
                }
            }
            Err(e) => {
                let se = &e;
                assert_ne!(
                    se.nika_code(),
                    codes::NIKA_1000,
                    "B.3 CLOSES the list_displays skeleton"
                );
            }
        }
    }

    /// B.4 closure proof · `capture_stream` no longer returns the
    /// `BackendNotWired` skeleton. On a host where the display resolves it
    /// returns `Ok(stream)` (not polled here · polling needs TCC + a display);
    /// otherwise the up-front probe errors — but NEVER with NIKA-1000.
    #[tokio::test]
    async fn capture_stream_no_longer_skeleton() {
        let backend = ScreenBackend::new();
        backend.grant_consent(); // guard 7 · exercise the capture path, not the gate
        if let Err(e) = backend.capture_stream(DisplayId::new(0)).await {
            let se = &e;
            assert_ne!(
                se.nika_code(),
                codes::NIKA_1000,
                "B.4 CLOSES the capture_stream skeleton"
            );
        }
    }

    /// Guard 7 integration · a fail-closed backend denies capture BEFORE any OS
    /// call (deterministic · no display/TCC) and never lights the LED (guard 6).
    #[tokio::test]
    async fn capture_denied_fail_closed_keeps_led_off() {
        let backend = ScreenBackend::new();
        let io = backend
            .capture_full(DisplayId::new(0))
            .await
            .expect_err("fail-closed denies capture");
        let se = &io;
        assert_eq!(
            se.nika_code(),
            codes::NIKA_1006,
            "guard 7 · consent denied fail-closed"
        );
        assert!(!backend.led_is_engaged(), "guard 6 · LED off when denied");
    }

    /// Guard 7 integration · revoking after a grant denies capture (NIKA-1007)
    /// before any OS call.
    #[tokio::test]
    async fn capture_revoked_consent_denies() {
        let backend = ScreenBackend::new();
        backend.grant_consent();
        backend.revoke_consent();
        let io = backend
            .capture_full(DisplayId::new(0))
            .await
            .expect_err("revoked consent denies capture");
        let se = &io;
        assert_eq!(
            se.nika_code(),
            codes::NIKA_1007,
            "guard 7 · consent revoked"
        );
    }

    /// Real continuous-capture smoke test — requires a connected display AND
    /// OS screen-recording permission (macOS TCC). `#[ignore]` keeps the
    /// default `--lib` suite headless-safe; run with `-- --ignored`.
    /// Only built with the `xcap` feature (the stub has no streaming backend).
    #[cfg(feature = "xcap")]
    #[tokio::test]
    #[ignore = "requires a display + OS screen-recording permission (TCC)"]
    async fn capture_stream_yields_frames() {
        let backend = ScreenBackend::new();
        backend.grant_consent(); // guard 7 · grant before pixel capture
        let displays = backend.list_displays().await.expect("enumerate displays");
        let first = displays.first().expect("at least one display");
        let mut stream = backend.capture_stream(first.id).await.expect("open stream");
        let item = std::future::poll_fn(|cx| stream.as_mut().poll_next(cx))
            .await
            .expect("at least one frame");
        let frame = item.expect("frame ok");
        assert_eq!(frame.display_id, first.id, "frame carries source display");
        assert!(!frame.pixels.is_empty(), "frame has pixels");
        drop(stream); // closes the channel · worker thread exits
    }

    /// Real full-display capture smoke test — requires a connected display
    /// AND OS screen-recording permission (macOS TCC). `#[ignore]` so the
    /// default `cargo test --workspace --lib` suite stays headless-safe;
    /// run locally with `cargo test -p nika-screen --features xcap -- --ignored`.
    /// Only built with the `xcap` feature (the stub has no capture backend).
    #[cfg(feature = "xcap")]
    #[tokio::test]
    #[ignore = "requires a display + OS screen-recording permission (TCC)"]
    async fn capture_full_real_smoke() {
        let backend = ScreenBackend::new();
        backend.grant_consent(); // guard 7 · grant before pixel capture
        let displays = backend.list_displays().await.expect("enumerate displays");
        let first = displays.first().expect("at least one display");
        let frame = backend.capture_full(first.id).await.expect("capture full");
        assert_eq!(
            u64::try_from(frame.pixels.len()).unwrap_or(u64::MAX),
            u64::from(frame.width) * u64::from(frame.height) * 4,
            "RGBA8 payload is width*height*4 bytes",
        );
        assert_eq!(
            frame.display_id, first.id,
            "frame carries the source display id"
        );
    }

    /// `now_ns` returns a real epoch-nanosecond timestamp (not a constant) ·
    /// pins it against `-> 0` / `-> 1` mutations. Headless · no display needed.
    /// `now_ns` is part of the xcap backend internals.
    #[cfg(feature = "xcap")]
    #[test]
    fn now_ns_is_a_real_epoch_timestamp() {
        // 2020-01-01T00:00:00Z expressed in nanoseconds since the UNIX epoch.
        const Y2020_NS: u64 = 1_577_836_800_000_000_000;
        assert!(
            now_ns() > Y2020_NS,
            "capture timestamp is a real epoch-ns value, not a constant"
        );
    }

    /// The `FrameRx` stream forwards a sent frame then ends when the channel
    /// closes · pins `poll_next` against `-> Poll::from(None)`. Headless · the
    /// channel plumbing is exercised directly without an OS capture.
    /// `FrameRx` is part of the xcap streaming backend.
    #[cfg(feature = "xcap")]
    #[tokio::test]
    async fn frame_rx_yields_then_ends_on_close() {
        let (tx, rx) = tokio::sync::mpsc::channel::<Result<Frame, ScreenError>>(STREAM_BUFFER);
        let mut stream = FrameRx { rx };
        let frame = Frame::new(
            4,
            2,
            1.0,
            Bytes::from_static(&[0u8; 32]),
            DisplayId::new(7),
            1,
        );
        tx.send(Ok(frame)).await.expect("send frame");
        let got = std::future::poll_fn(|cx| Pin::new(&mut stream).poll_next(cx))
            .await
            .expect("stream yields the sent frame")
            .expect("frame ok");
        assert_eq!(
            got.display_id,
            DisplayId::new(7),
            "poll_next forwards the frame (not a constant None)"
        );
        drop(tx); // close the channel
        let end = std::future::poll_fn(|cx| Pin::new(&mut stream).poll_next(cx)).await;
        assert!(end.is_none(), "a closed channel ends the stream");
    }

    /// Guard 7 · `grant_consent` lets capture PAST the gate · pins it against a
    /// no-op mutation. With consent granted, `capture_full` reaches the OS
    /// layer: on a headless / TCC-denied host it errors with an OS code
    /// (`NoDisplaysFound` / `BackendInit` / `CaptureFailed`) — NEVER 1006/1007.
    /// If `grant_consent` were a no-op the fail-closed gate would deny (1006).
    #[tokio::test]
    async fn grant_consent_lets_capture_past_the_gate() {
        let backend = ScreenBackend::new();
        backend.grant_consent();
        if let Err(io) = backend.capture_full(DisplayId::new(0)).await {
            let se = &io;
            assert_ne!(
                se.nika_code(),
                codes::NIKA_1006,
                "grant_consent must pass the gate, not deny"
            );
            assert_ne!(
                se.nika_code(),
                codes::NIKA_1007,
                "consent was granted, not revoked"
            );
        }
    }

    /// Stub backend · with consent GRANTED, `capture_full` is reached PAST the
    /// guard 7 gate and returns the "backend not built" `BackendInit` (NIKA-1009)
    /// — NOT a consent code (1006/1007). Proves the consent/LED guards run in the
    /// stub config and the OS-call portion is the only thing replaced.
    #[cfg(not(feature = "xcap"))]
    #[tokio::test]
    async fn stub_reaches_backend_not_built_past_the_gate() {
        let backend = ScreenBackend::new();
        backend.grant_consent(); // guard 7 · pass the gate, exercise the backend path
        let io = backend
            .capture_full(DisplayId::new(0))
            .await
            .expect_err("stub backend is not built");
        let se = &io;
        assert_eq!(
            se.nika_code(),
            codes::NIKA_1009,
            "stub reaches BackendInit (NIKA-1009) past the consent gate"
        );
        assert_ne!(
            se.nika_code(),
            codes::NIKA_1006,
            "not a consent-denied code"
        );
        assert_ne!(
            se.nika_code(),
            codes::NIKA_1007,
            "not a consent-revoked code"
        );
        assert!(
            io.to_string().contains("not built"),
            "stub error message points to the missing xcap feature"
        );
    }

    // --- validate_region · PURE bounds logic (headless · mutation-killing) ---
    // The pure bounds logic lives behind the `xcap` feature (it backs
    // `capture_region_sync`, which the stub config does not build).
    #[cfg(feature = "xcap")]
    mod validate_region_tests {
        use super::*;

        /// A well-inside region returns its unsigned origin.
        #[test]
        fn validate_region_accepts_in_bounds() {
            assert_eq!(
                validate_region(Rect::new(10, 20, 100, 50), 1920, 1080).expect("in bounds"),
                (10, 20)
            );
        }

        /// The exact boundary `rx + width == display_w` (and height) is IN bounds
        /// (`>` not `>=`) · pins the `>` comparators against `>=` / `==`.
        #[test]
        fn validate_region_accepts_exact_boundary() {
            assert_eq!(
                validate_region(Rect::new(0, 0, 1920, 1080), 1920, 1080).expect("exact bound"),
                (0, 0)
            );
            assert_eq!(
                validate_region(Rect::new(1820, 980, 100, 100), 1920, 1080).expect("exact bound"),
                (1820, 980)
            );
        }

        /// One pixel past the width bound is rejected · pins `>` against `<` / `==`.
        #[test]
        fn validate_region_rejects_width_overflow_by_one() {
            assert!(validate_region(Rect::new(1, 0, 1920, 100), 1920, 1080).is_err());
        }

        /// One pixel past the height bound is rejected · pins `>` against `<` / `==`.
        #[test]
        fn validate_region_rejects_height_overflow_by_one() {
            assert!(validate_region(Rect::new(0, 1, 100, 1080), 1920, 1080).is_err());
        }

        /// Zero width alone (every other term valid) is rejected · pins the
        /// `width == 0` check + the first `||` against `!=` / `&&`.
        #[test]
        fn validate_region_rejects_zero_width_alone() {
            assert!(validate_region(Rect::new(0, 0, 0, 50), 1920, 1080).is_err());
        }

        /// Zero height alone is rejected · pins `height == 0` + the second `||`.
        #[test]
        fn validate_region_rejects_zero_height_alone() {
            assert!(validate_region(Rect::new(0, 0, 50, 0), 1920, 1080).is_err());
        }

        /// A negative offset (signed→unsigned conversion fails) is rejected.
        #[test]
        fn validate_region_rejects_negative_offset() {
            assert!(validate_region(Rect::new(-1, 0, 10, 10), 1920, 1080).is_err());
            assert!(validate_region(Rect::new(0, -5, 10, 10), 1920, 1080).is_err());
        }
    }
}
