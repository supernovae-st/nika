// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Screen-capture backend — implements the L0.5 `ScreenCapture` trait via `xcap`.
//!
//! **B.3 single-shot capture WIRED.** `list_displays` / `capture_full` /
//! `capture_region` now drive the real cross-platform `xcap` backend
//! (display enumeration + RGBA8 frame capture · macOS CoreGraphics ·
//! Linux X11/Wayland portal · Windows DXGI). `xcap` encapsulates all OS
//! FFI internally (objc2 / x11 / windows crates), so this crate stays
//! `unsafe_code = forbid`-clean.
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

use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bytes::Bytes;
use futures_core::Stream;
use nika_kernel::io::screen::{DisplayId, DisplayInfo, Frame, FrameStream, Rect, ScreenCapture};
use xcap::Monitor;
use xcap::image::RgbaImage;

use crate::error::ScreenError;

/// Cross-platform screen-capture backend (driven by `xcap`).
///
/// A zero-sized handle — `xcap` enumerates monitors per call, so there is no
/// persistent OS state to hold. `#[non_exhaustive]` leaves room for the B.5
/// guard handles (consent gate + LED indicator) to land as fields.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct ScreenBackend;

impl ScreenBackend {
    /// Construct a new screen-capture backend.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl ScreenCapture for ScreenBackend {
    async fn list_displays(&self) -> std::io::Result<Vec<DisplayInfo>> {
        let infos = tokio::task::spawn_blocking(list_displays_sync)
            .await
            .map_err(|e| join_err(&e))??;
        Ok(infos)
    }

    async fn capture_full(&self, display: DisplayId) -> std::io::Result<Frame> {
        let frame = tokio::task::spawn_blocking(move || capture_full_sync(display))
            .await
            .map_err(|e| join_err(&e))??;
        Ok(frame)
    }

    async fn capture_region(&self, display: DisplayId, region: Rect) -> std::io::Result<Frame> {
        let frame = tokio::task::spawn_blocking(move || capture_region_sync(display, region))
            .await
            .map_err(|e| join_err(&e))??;
        Ok(frame)
    }

    async fn capture_stream(&self, display: DisplayId) -> std::io::Result<FrameStream> {
        // Fail fast · verify the display exists before spawning the worker
        // (keeps the !Send Monitor inside the blocking probe).
        tokio::task::spawn_blocking(move || find_monitor_sync(display.0).map(|_| ()))
            .await
            .map_err(|e| join_err(&e))??;

        // Bounded channel · backpressure paces the worker to the consumer.
        let (tx, rx) = tokio::sync::mpsc::channel::<std::io::Result<Frame>>(STREAM_BUFFER);

        // Long-lived capture worker on the tokio blocking pool (the sanctioned
        // tokio-first primitive · holds one pool slot for the stream's lifetime).
        // Drop-stop cancellation · when the consumer drops the returned stream the
        // channel closes, the next `blocking_send` fails, the worker exits and
        // frees the slot. A `CancellationToken` would add nothing — a sync
        // `capture_image` mid-flight cannot be interrupted regardless.
        tokio::task::spawn_blocking(move || stream_worker(display, &tx));

        Ok(Box::pin(FrameRx { rx }))
    }
}

// --- sync xcap helpers (run inside spawn_blocking · keep !Send Monitor local) ---

/// Map a `spawn_blocking` join failure (panic / cancel) into a transient
/// capture error.
fn join_err(e: &tokio::task::JoinError) -> ScreenError {
    ScreenError::CaptureFailed {
        reason: format!("capture task join failed: {e}"),
    }
}

/// Map an `xcap` backend error into a transient `CaptureFailed`.
fn capture_failed(e: impl std::fmt::Display) -> ScreenError {
    ScreenError::CaptureFailed {
        reason: e.to_string(),
    }
}

/// Monotonic capture timestamp — nanoseconds since the UNIX epoch
/// (saturating · clock-skew + overflow tolerant · never panics).
fn now_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// Resolve a connected `xcap::Monitor` by its kernel `DisplayId`.
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
fn rgba_to_frame(img: RgbaImage, scale: f32, display: DisplayId) -> Frame {
    let width = img.width();
    let height = img.height();
    let pixels = Bytes::from(img.into_raw());
    Frame::new(width, height, scale, pixels, display, now_ns())
}

/// Capture a full-display frame (sync · runs inside `spawn_blocking`).
fn capture_full_sync(display: DisplayId) -> Result<Frame, ScreenError> {
    let monitor = find_monitor_sync(display.0)?;
    let scale = monitor.scale_factor().map_err(capture_failed)?;
    let img = monitor.capture_image().map_err(capture_failed)?;
    Ok(rgba_to_frame(img, scale, display))
}

/// Capture a sub-region frame (sync · runs inside `spawn_blocking`).
///
/// The region is validated against the display bounds in display-local
/// top-left coordinates; a negative offset or an out-of-bounds extent
/// yields [`ScreenError::RegionOutOfBounds`] (structural · non-transient).
fn capture_region_sync(display: DisplayId, region: Rect) -> Result<Frame, ScreenError> {
    let monitor = find_monitor_sync(display.0)?;
    let scale = monitor.scale_factor().map_err(capture_failed)?;
    let display_w = monitor.width().map_err(capture_failed)?;
    let display_h = monitor.height().map_err(capture_failed)?;

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

    let img = monitor
        .capture_region(rx, ry, region.width, region.height)
        .map_err(capture_failed)?;
    Ok(rgba_to_frame(img, scale, display))
}

// --- B.4 continuous capture stream ---

/// Bounded frame buffer · backpressure paces the worker to the consumer.
const STREAM_BUFFER: usize = 4;

/// Fixed ~30fps capture cadence for the B.4 stream. A configurable
/// frame-rate / change-detection policy is a later-round refinement.
const STREAM_FRAME_INTERVAL: Duration = Duration::from_millis(33);

/// Continuous capture loop · runs on a dedicated OS thread.
///
/// Captures one full-display frame per `STREAM_FRAME_INTERVAL` and pushes it
/// (Ok or per-frame Err) to the consumer. Per the kernel contract a transient
/// per-frame error surfaces WITHOUT tearing down the stream — the consumer
/// decides whether to keep polling or drop. `blocking_send` returning `Err`
/// means the consumer dropped the receiver → the worker exits (drop-stop).
fn stream_worker(display: DisplayId, tx: &tokio::sync::mpsc::Sender<std::io::Result<Frame>>) {
    loop {
        let item = capture_full_sync(display).map_err(std::io::Error::from);
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
struct FrameRx {
    rx: tokio::sync::mpsc::Receiver<std::io::Result<Frame>>,
}

impl Stream for FrameRx {
    type Item = std::io::Result<Frame>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // `FrameRx` is `Unpin` (Receiver is Unpin) · safe to project via get_mut.
        self.get_mut().rx.poll_recv(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
                let src = e.into_inner().expect("boxed source");
                let se = src.downcast::<ScreenError>().expect("ScreenError source");
                assert_ne!(
                    se.code(),
                    "NIKA-1000",
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
        if let Err(e) = backend.capture_stream(DisplayId::new(0)).await {
            let src = e.into_inner().expect("boxed source");
            let se = src.downcast::<ScreenError>().expect("ScreenError source");
            assert_ne!(
                se.code(),
                "NIKA-1000",
                "B.4 CLOSES the capture_stream skeleton"
            );
        }
    }

    /// Real continuous-capture smoke test — requires a connected display AND
    /// OS screen-recording permission (macOS TCC). `#[ignore]` keeps the
    /// default `--lib` suite headless-safe; run with `-- --ignored`.
    #[tokio::test]
    #[ignore = "requires a display + OS screen-recording permission (TCC)"]
    async fn capture_stream_yields_frames() {
        let backend = ScreenBackend::new();
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
    /// run locally with `cargo test -p nika-screen -- --ignored`.
    #[tokio::test]
    #[ignore = "requires a display + OS screen-recording permission (TCC)"]
    async fn capture_full_real_smoke() {
        let backend = ScreenBackend::new();
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
}
