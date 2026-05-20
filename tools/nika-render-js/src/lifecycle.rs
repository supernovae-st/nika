// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `BrowserHandle` — owns the launched Chrome process + its event pump.
//!
//! `chromiumoxide::Browser::launch` returns `(Browser, Handler)` where the
//! `Handler` is a `Stream` that MUST be polled continuously or the browser
//! stalls. We pump it in a dedicated tokio task whose shutdown is driven by a
//! [`CancellationToken`].
//!
//! # Shutdown discipline
//!
//! `Drop` cannot `.await`, so it is a best-effort safety net only
//! (`shutdown.cancel()` + `task.abort()`). Callers SHOULD prefer the async
//! [`BrowserHandle::close`] for a graceful, bounded teardown — see the crate
//! root doc comment for the close()-over-Drop rule. chromiumoxide's own
//! `Browser::drop` reaps the Chrome subprocess; we still cancel + abort the
//! pump task explicitly so no detached task survives.

use std::time::Duration;

use chromiumoxide::{Browser, BrowserConfig};
use futures_util::StreamExt;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::error::RenderError;

/// Bounded timeout for the graceful `Browser::close` call.
const BROWSER_CLOSE_TIMEOUT: Duration = Duration::from_secs(5);
/// Bounded timeout for joining the pump task after shutdown.
const HANDLER_JOIN_TIMEOUT: Duration = Duration::from_secs(5);

/// Owns a launched Chromium process, its event-pump task, and the shutdown
/// token coordinating their teardown.
///
/// Construct via [`BrowserHandle::launch`]; tear down via
/// [`BrowserHandle::close`] (preferred) or rely on [`Drop`].
pub struct BrowserHandle {
    /// The live chromiumoxide browser. Reached via [`BrowserHandle::browser`].
    browser: Browser,
    /// The spawned pump task draining the `Handler` stream. `Some` until
    /// [`BrowserHandle::close`] joins it (or `Drop` aborts it).
    handler_task: Option<JoinHandle<()>>,
    /// Cancelling this token makes the pump loop break cleanly.
    shutdown: CancellationToken,
}

impl BrowserHandle {
    /// Launch a headless Chromium and spawn its event-pump task.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::Launch`] if the browser binary cannot be started.
    pub async fn launch(config: BrowserConfig) -> Result<Self, RenderError> {
        let (browser, mut handler) =
            Browser::launch(config)
                .await
                .map_err(|e| RenderError::Launch {
                    source: Box::new(e),
                })?;

        let shutdown = CancellationToken::new();
        let shutdown_child = shutdown.child_token();

        // Cooperative pump: biased select drains the Handler until shutdown OR
        // the stream ends (browser gone). Breaking the loop drops `handler`.
        let handler_task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    biased;
                    () = shutdown_child.cancelled() => break,
                    event = handler.next() => match event {
                        Some(_) => {}  // CDP event/error pumped; ignored in v0.
                        None => break, // stream ended.
                    },
                }
            }
        });

        Ok(Self {
            browser,
            handler_task: Some(handler_task),
            shutdown,
        })
    }

    /// Borrow the live [`Browser`] for page creation.
    #[must_use]
    pub fn browser(&self) -> &Browser {
        &self.browser
    }

    /// Gracefully shut down the browser and its pump task with bounded waits.
    ///
    /// Cancel the pump token, close the browser within
    /// [`BROWSER_CLOSE_TIMEOUT`], then join the pump task within
    /// [`HANDLER_JOIN_TIMEOUT`]. All steps are best-effort; a hung browser
    /// process never blocks the caller longer than the bounds above.
    pub async fn close(mut self) {
        self.shutdown.cancel();

        // Browser::close takes &mut self and asks Chrome to exit. Bounded.
        let _ = tokio::time::timeout(BROWSER_CLOSE_TIMEOUT, self.browser.close()).await;

        // Join the pump task, bounded. Take it so Drop won't re-abort.
        if let Some(task) = self.handler_task.take() {
            let _ = tokio::time::timeout(HANDLER_JOIN_TIMEOUT, task).await;
        }
    }
}

impl Drop for BrowserHandle {
    /// Best-effort safety net only — cannot `.await`. Prefer
    /// [`BrowserHandle::close`]. Cancels the pump token (cooperative exit)
    /// then aborts the task as a fallback if it is still owned.
    fn drop(&mut self) {
        self.shutdown.cancel();
        if let Some(task) = self.handler_task.take() {
            task.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Compile-time proof BrowserHandle is Send + Sync (needed to live in
    // Arc<BrowserHandle> shared across the engine's tokio tasks).
    const _: fn() = || {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<BrowserHandle>();
    };

    // launch shape requires a Chromium binary · guarded behind ignore.
    #[tokio::test]
    #[ignore = "requires Chromium binary"]
    async fn launch_returns_handle_with_live_task() {
        let cfg = BrowserConfig::builder()
            .build()
            .expect("default config builds");
        let handle = BrowserHandle::launch(cfg).await.expect("launch ok");
        assert!(handle.handler_task.is_some(), "pump task spawned");
        assert!(!handle.shutdown.is_cancelled(), "shutdown not yet fired");
        handle.close().await;
    }

    // Drop semantics mirrored on the token/task shape (no real Browser in unit):
    // cancelling a token + aborting a handle never awaits.
    #[tokio::test]
    async fn drop_path_cancels_token_synchronously() {
        let shutdown = CancellationToken::new();
        let child = shutdown.child_token();
        let task: JoinHandle<()> = tokio::spawn(async move {
            child.cancelled().await;
        });
        shutdown.cancel();
        task.abort();
        assert!(shutdown.is_cancelled(), "token cancelled by drop path");
    }

    // The pump loop breaks when its child token is cancelled (cooperative
    // shutdown) — proven without a real Handler.
    #[tokio::test]
    async fn pump_loop_exits_on_shutdown_cancel() {
        let shutdown = CancellationToken::new();
        let child = shutdown.child_token();
        let task: JoinHandle<()> = tokio::spawn(async move {
            loop {
                tokio::select! {
                    biased;
                    () = child.cancelled() => break,
                    () = tokio::time::sleep(Duration::from_millis(50)) => {}
                }
            }
        });
        shutdown.cancel();
        let joined = tokio::time::timeout(Duration::from_secs(1), task).await;
        assert!(joined.is_ok(), "pump task exited after shutdown cancel");
    }

    // close()-shaped graceful path: token cancels then bounded join completes.
    #[tokio::test]
    async fn close_shaped_cancels_then_joins_within_bound() {
        let shutdown = CancellationToken::new();
        let child = shutdown.child_token();
        let mut task: Option<JoinHandle<()>> = Some(tokio::spawn(async move {
            child.cancelled().await;
        }));
        shutdown.cancel();
        if let Some(t) = task.take() {
            let joined = tokio::time::timeout(HANDLER_JOIN_TIMEOUT, t).await;
            assert!(joined.is_ok(), "task joined within bound after cancel");
        }
        assert!(task.is_none(), "task taken so Drop won't re-abort");
    }
}
