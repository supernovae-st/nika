// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-render-js` · JS-rendering [`HttpClient`] for the Nika engine.
//!
//! Wraps `chromiumoxide` 0.9.1 (`["rustls"]`) to fetch pages that need
//! client-side rendering (React/Vue/Next/Nuxt SPAs). v0 is GET-only, headless,
//! and serializes one page at a time (`MAX_CONCURRENT_PAGES = 1`). The async
//! runtime comes from the caller (workspace tokio) — chromiumoxide 0.9.1 has
//! no runtime feature.
//!
//! # Layer
//!
//! L1 effect crate · sister to [`nika-http`](../nika_http) · same kernel
//! trait surface ([`nika_kernel::http::HttpClient`]). The ONLY `nika-*`
//! runtime dependency is `nika-kernel` (cross-flow asymmetric D-2026-05-08-N1).
//!
//! # Cancellation & lifecycle discipline
//!
//! - Every render races a `CancellationToken`; a fired token returns
//!   [`RenderError::Cancelled`] before any further `.await`.
//! - The browser tab is closed on EVERY render exit (success/error/cancel) —
//!   chromiumoxide 0.9.1 does not auto-close tabs on `Page` drop.
//! - **`close()` over `Drop`**: tear the client down via the async
//!   `BrowserHandle::close`. `Drop` is a best-effort safety net only — it
//!   cannot `.await`, so it merely cancels the pump token and aborts the task.
//!   Long-lived daemons MUST call `close()` to avoid orphaned Chrome processes.
//!
//! # Round 1 scope (this scaffold)
//!
//! Batch A ships the error taxonomy ([`RenderError`]) and render config
//! ([`RenderOptions`]). Batch B adds `BrowserHandle` (lifecycle) and
//! `ChromiumClient` (the [`HttpClient`] impl). Batch C wires the `render: js`
//! backend into `nika-verb-fetch`.

#![forbid(unsafe_code)]

mod error;
mod page;

pub use error::RenderError;
pub use page::RenderOptions;
