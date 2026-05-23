// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Screen-capture backend — implements the L0.5 `ScreenCapture` trait.
//!
//! **B.2 skeleton.** Every method returns [`ScreenError::BackendNotWired`]
//! (boxed into `io::Error`); the real cross-platform impl lands at B.3 via
//! `xcap` (display enumeration + RGBA frame capture). Per
//! `skeleton-option-a-pattern.md` §3 the placeholder is allowed for a single
//! cascade window and is CLOSED at B.3 same-commit (§5 closure ceremony).
//!
//! `unimplemented!()` is deliberately NOT used — the workspace promotes the
//! `clippy::unimplemented` warning to an error under `-D warnings`, so the
//! skeleton returns a real typed error instead.

use nika_kernel::io::screen::{DisplayId, DisplayInfo, Frame, FrameStream, Rect, ScreenCapture};

use crate::error::ScreenError;

/// Cross-platform screen-capture backend.
///
/// B.3 wires `xcap` (which encapsulates the OS FFI internally, keeping this
/// crate `unsafe_code = forbid`-clean). `#[non_exhaustive]` leaves room for
/// the B.5 guard handles (consent gate + LED indicator) to land as fields.
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
        Err(ScreenError::BackendNotWired.into())
    }

    async fn capture_full(&self, _display: DisplayId) -> std::io::Result<Frame> {
        Err(ScreenError::BackendNotWired.into())
    }

    async fn capture_region(&self, _display: DisplayId, _region: Rect) -> std::io::Result<Frame> {
        Err(ScreenError::BackendNotWired.into())
    }

    async fn capture_stream(&self, _display: DisplayId) -> std::io::Result<FrameStream> {
        Err(ScreenError::BackendNotWired.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The skeleton backend satisfies `ScreenCapture` and every method returns
    /// the `BackendNotWired` placeholder (NIKA-1000) — proves the trait is
    /// implemented end-to-end before B.3 wires the real `xcap` impl.
    #[tokio::test]
    async fn skeleton_backend_returns_backend_not_wired() {
        let backend = ScreenBackend::new();

        let displays = backend.list_displays().await;
        assert!(displays.is_err(), "skeleton list_displays errors");

        let full = backend.capture_full(DisplayId::new(0)).await;
        assert!(full.is_err(), "skeleton capture_full errors");

        let region = backend
            .capture_region(DisplayId::new(0), Rect::new(0, 0, 100, 100))
            .await;
        assert!(region.is_err(), "skeleton capture_region errors");

        let stream = backend.capture_stream(DisplayId::new(0)).await;
        assert!(stream.is_err(), "skeleton capture_stream errors");

        // The boxed io::Error preserves the NIKA-1000 source.
        let io = full.expect_err("err present");
        let src = io.into_inner().expect("boxed source");
        let screen_err = src.downcast::<ScreenError>().expect("ScreenError source");
        assert_eq!(screen_err.code(), "NIKA-1000");
    }
}
