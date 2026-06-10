// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Browser automation traits — L0.5 sealed DTOs + async trait.
//!
//! Phase 2 M1 PR 5 (sprint plan `2026-05-14-nika-phase-2-m1-kernel-
//! modules-sprint-plan.md` §4). Reserved error codes NIKA-1100..1149
//! per `docs/architecture/forward-compat-invariants.md` Gate 12.
//!
//! ADR-006 monolithic-kernel-spirit (single trait + DTOs · no proc
//! macro · no async runtime dep). ADR-016 ISP discipline (1 trait =
//! 1 capability · browser automation is 1 concern · CDP-shaped surface
//! · L1 impls split per backend · `headless_chrome` · `chromiumoxide` ·
//! playwright sidecar). ADR-037 bottom-up layer (L0.5 traits-only ·
//! zero tokio · zero CDP-protocol crates · those land in L1 at Phase
//! 2 M2-M3).
//!
//! Cross-PR dep · `DomNode::bbox: Option<Rect>` reuses the screen-
//! capture `Rect` DTO shipped at PR 1 (M1.1 · canonical pixel-coord
//! rectangle). `BrowserAutomation::screenshot` returns the `Frame`
//! DTO from `io::screen` (single canonical capture-frame type across
//! display capture + browser screenshot · cohérent
//! `no-legacy-no-back-compat.md` Class 1).
//!
//! URL field decision (sprint plan §7 Risks · pre-flight verify) ·
//! the workspace does NOT declare `url` as a dependency · scope-lock
//! per `concurrent-session-conflict.md` Pattern 1 forbids workspace
//! Cargo.toml structural change in this batch. `BrowserSession::url`
//! and `BrowserAutomation::navigate` therefore carry `String` with
//! documented format expectation (RFC 3986 absolute URI · scheme +
//! authority + path · validated at L1 impl boundary). When `url`
//! lands in workspace via a future scope-amend, the type rotates to
//! `url::Url` per `no-legacy-no-back-compat.md` (additive · no shim).
//!
//! `DomNode::attributes: BTreeMap<String, String>` per workspace
//! `clippy.toml` `disallowed-types` ban on `HashMap` (DEV-3 lesson ·
//! `BTreeMap` provides serde-determinism + ordered iteration · zero
//! extra dep · std-only).

use crate::io::screen::{Frame, Rect};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Browser session handle · opaque CDP session identifier.
///
/// `id` is the canonical Chrome `DevTools` Protocol session id assigned
/// by the L1 impl at `launch()` time. `url` carries the last-known
/// navigation target (RFC 3986 absolute URI · validated at L1) · `None`
/// when the session is freshly launched and `navigate()` has not been
/// called yet OR when the L1 impl cannot resolve the current target.
///
/// `#[non_exhaustive]` keeps the struct extensible per `no-legacy-no-
/// back-compat.md` (NUKE-LEGACY · breaking changes ship on MINOR until
/// forever-v0.x). Future fields land additively (e.g. `cookies` ·
/// `viewport` · `user_agent`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct BrowserSession {
    /// CDP session identifier · stable per session lifetime.
    pub id: String,
    /// Last-known navigation target · RFC 3986 absolute URI · `None`
    /// when freshly launched.
    pub url: Option<String>,
}

impl BrowserSession {
    /// Construct a new browser session record.
    ///
    /// Per Invariant #19 · every `#[non_exhaustive]` struct ships a
    /// `new()` constructor so downstream code never field-literals.
    #[must_use]
    pub fn new(id: String, url: Option<String>) -> Self {
        Self { id, url }
    }
}

/// Browser launch profile · configuration handed to `launch()`.
///
/// `user_data_dir: Option<PathBuf>` controls Chrome's user-data
/// directory (cookies · localStorage · session state) · `None` means
/// the L1 impl picks an ephemeral temp dir (clean session). `headless:
/// bool` toggles headless mode · `false` shows a visible window
/// (development · debugging). `viewport: Option<(u32, u32)>` sets the
/// initial viewport (width · height) in CSS pixels · `None` uses the
/// L1 impl default (typically 1920×1080).
///
/// `Default::default()` produces `{ user_data_dir: None, headless:
/// false, viewport: None }` so callers can spread-update individual
/// fields ergonomically post-`new()` if needed.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub struct BrowserProfile {
    /// User-data directory · `None` = ephemeral temp dir.
    pub user_data_dir: Option<PathBuf>,
    /// Headless mode toggle · `false` = visible window.
    pub headless: bool,
    /// Initial viewport `(width, height)` in CSS pixels · `None` = L1 default.
    pub viewport: Option<(u32, u32)>,
}

impl BrowserProfile {
    /// Construct a new browser profile.
    #[must_use]
    pub fn new(
        user_data_dir: Option<PathBuf>,
        headless: bool,
        viewport: Option<(u32, u32)>,
    ) -> Self {
        Self {
            user_data_dir,
            headless,
            viewport,
        }
    }
}

/// DOM tree node · recursive HTML element representation.
///
/// `tag` is the lowercased element name (e.g. `"div"` · `"button"` ·
/// `"a"`) · the L1 impl normalizes case at snapshot time. `attributes:
/// BTreeMap<String, String>` carries the element's HTML attributes
/// (e.g. `"id" → "submit-btn"` · `"href" → "/login"`) · `BTreeMap`
/// enforces serde-determinism + ordered iteration per `clippy.toml`
/// `disallowed-types` ban on `HashMap` (DEV-3 lesson · cohérent
/// `io::a11y::AxNode::attributes` shape).
///
/// `children: Vec<DomNode>` carries child elements in document order ·
/// `bbox: Option<Rect>` shares the canonical pixel-coord `Rect` from
/// `io::screen` (cross-PR dep · M1.1) · `None` when the element is
/// `display: none` · off-screen · or otherwise unrenderable.
///
/// `PartialEq` + `Eq` derive recursively over `Vec<DomNode>` children ·
/// the canonical equality is structural (cohérent `AxNode` shape at
/// `io::a11y`). `Eq` is sound · no `f32`/`f64` fields · `Rect` is
/// integer-only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct DomNode {
    /// Lowercased HTML element name (e.g. `"div"`).
    pub tag: String,
    /// HTML attributes · `BTreeMap` for serde determinism.
    pub attributes: BTreeMap<String, String>,
    /// Child elements in document order.
    pub children: Vec<DomNode>,
    /// Bounding box in physical-pixel coordinates · `None` when off-screen
    /// or unrenderable.
    pub bbox: Option<Rect>,
}

impl DomNode {
    /// Construct a new DOM tree node.
    ///
    /// Per Invariant #19 · every `#[non_exhaustive]` struct ships a
    /// `new()` constructor so downstream code never field-literals.
    #[must_use]
    pub fn new(
        tag: String,
        attributes: BTreeMap<String, String>,
        children: Vec<DomNode>,
        bbox: Option<Rect>,
    ) -> Self {
        Self {
            tag,
            attributes,
            children,
            bbox,
        }
    }
}

/// Browser-automation errors — the typed boundary of the `BrowserAutomation`
/// trait (Pattern A · FCI-023bis). `#[non_exhaustive]`; `NikaErrorCode` impl +
/// reserved range (`Category::Browser` · NIKA-1401..1406) live in `crate::errors`.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum BrowserError {
    /// Launching the browser session (process spawn / CDP handshake) failed.
    #[error("NIKA-1401 · browser launch failed: {reason}")]
    LaunchFailed {
        /// Backend-reported reason.
        reason: String,
    },
    /// Navigating to a URL failed (malformed URL, load error, timeout).
    #[error("NIKA-1402 · browser navigation failed: {reason}")]
    NavigationFailed {
        /// Backend-reported reason.
        reason: String,
    },
    /// The referenced browser session was not found / already closed.
    #[error("NIKA-1403 · browser session not found: {session}")]
    SessionNotFound {
        /// The session id that did not resolve.
        session: String,
    },
    /// A DOM selector did not resolve, or the interaction failed.
    #[error("NIKA-1404 · browser selector failed: {reason}")]
    SelectorFailed {
        /// Backend-reported reason (e.g. "selector matched no element").
        reason: String,
    },
    /// No browser-automation backend is compiled for this platform.
    #[error("NIKA-1405 · no browser-automation backend on this platform")]
    BackendUnavailable,
    /// A `spawn_blocking` / driver task panicked or was cancelled.
    #[error("NIKA-1406 · browser task join failed: {reason}")]
    TaskJoinFailed {
        /// Join failure detail.
        reason: String,
    },
}

/// Browser automation capability · async trait over a CDP-shaped surface.
///
/// CANCEL SAFETY: evaluated per-method · CDP commands differ in
/// idempotency. `launch()` is NOT cancel-safe (spawns a child process ·
/// dropped futures must NOT leak the child · L1 impls SHOULD use
/// `tokio::process::Command::kill_on_drop(true)` per Invariant #11).
/// `navigate()` · `dom_snapshot()` · `screenshot()` are cancel-safe
/// (CDP `Page.navigate` is idempotent · DOM/screenshot are read-only).
/// `click_selector()` is best-effort cancel-safe (CDP `Input.dispatch
/// MouseEvent` may have already fired by the time the future is
/// dropped · L1 impls document the race).
///
/// `#[trait_variant::make(BrowserAutomationDyn: Send)]` generates a
/// `Send` companion trait `BrowserAutomationDyn` for generic constraints
/// (cohérent `io::screen::ScreenCaptureDyn` · `io::ocr::OcrEngineDyn` ·
/// `io::a11y::AccessibilityTreeDyn` · `io::input::InputDeviceDyn`
/// pattern · ADR-006 + `io::fs::FsRead` canonical shape). Downstream
/// code uses `T: BrowserAutomationDyn` for monomorphized hot paths.
/// Per `trait_variant` 0.1 semantics · the companion uses `impl Future
/// + Send` returns which are NOT dyn-compatible · this is intentional
/// (kernel traits stay zero-cost · L1 impls wrap via generic `Arc` over
/// `T: BrowserAutomation` not over trait objects).
#[trait_variant::make(BrowserAutomationDyn: Send)]
pub trait BrowserAutomation: Send + Sync {
    /// Launch a new browser session.
    ///
    /// CANCEL SAFETY: NOT cancel-safe · spawns a child browser process ·
    /// dropped futures MUST NOT leak the child. L1 impls SHOULD wrap the
    /// process spawn in `tokio::process::Command::kill_on_drop(true)` per
    /// Invariant #11 · partial-launch state (process up · CDP handshake
    /// not yet complete) MUST be reaped on drop.
    async fn launch(&self, profile: &BrowserProfile) -> Result<BrowserSession, BrowserError>;

    /// Navigate session to URL.
    ///
    /// CANCEL SAFETY: cancel-safe · CDP `Page.navigate` is idempotent ·
    /// the future may be safely dropped before the navigation settles
    /// (the page lifecycle continues server-side · subsequent `dom_
    /// snapshot` will observe whichever state landed first). `url` is
    /// an RFC 3986 absolute URI string · L1 impls validate format and
    /// return [`BrowserError::NavigationFailed`] (NIKA-1402) on malformed
    /// input.
    async fn navigate(&self, session: &BrowserSession, url: &str) -> Result<(), BrowserError>;

    /// Capture a snapshot of the current DOM tree.
    ///
    /// CANCEL SAFETY: cancel-safe · read-only CDP query (`DOM.getDocument`
    /// + recursive traversal) · partial tree data MUST NOT leak to a
    /// caller on cancel (L1 impl invariant · enforced by integration
    /// tests at Phase 3 M3).
    async fn dom_snapshot(&self, session: &BrowserSession) -> Result<DomNode, BrowserError>;

    /// Click element by CSS selector.
    ///
    /// CANCEL SAFETY: best-effort · CDP `Input.dispatchMouseEvent` may
    /// have already fired by the time the future is dropped · L1 impls
    /// document the race window. Returns [`BrowserError::SelectorFailed`]
    /// (NIKA-1404) when the selector matches no element.
    async fn click_selector(&self, session: &BrowserSession, sel: &str)
    -> Result<(), BrowserError>;

    /// Capture a screenshot of the current page.
    ///
    /// CANCEL SAFETY: cancel-safe · read-only CDP `Page.captureScreen
    /// shot` · returns the canonical `Frame` DTO from `io::screen`
    /// (cross-PR dep · M1.1) · zero-copy RGBA8 payload. Partial frame
    /// data MUST NOT leak on cancel.
    async fn screenshot(&self, session: &BrowserSession) -> Result<Frame, BrowserError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `BrowserSession` DTO round-trips through serde JSON · proves the
    /// canonical shape (id + optional url) serializes cleanly. Per
    /// sprint plan §4 PR 5 test 1.
    #[test]
    fn browser_session_serde_roundtrip() {
        let session = BrowserSession::new(
            "session-abc-123".to_string(),
            Some("https://example.com/login".to_string()),
        );
        let json = serde_json::to_string(&session).expect("serialize browser session");
        let back: BrowserSession =
            serde_json::from_str(&json).expect("deserialize browser session");
        assert_eq!(back, session);
        assert_eq!(back.id, "session-abc-123");
        assert_eq!(back.url.as_deref(), Some("https://example.com/login"));
    }

    /// `BrowserSession` with `url: None` round-trips correctly · proves
    /// the freshly-launched case (pre-`navigate()`) serializes cleanly.
    #[test]
    fn browser_session_serde_no_url() {
        let session = BrowserSession::new("session-fresh".to_string(), None);
        let json = serde_json::to_string(&session).expect("serialize fresh session");
        let back: BrowserSession = serde_json::from_str(&json).expect("deserialize fresh session");
        assert_eq!(back, session);
        assert!(back.url.is_none());
    }

    /// `BrowserProfile::default()` produces canonical defaults · `headless:
    /// false` · `viewport: None` · `user_data_dir: None`. Per sprint plan
    /// §4 PR 5 test 2.
    #[test]
    fn browser_profile_default_headless_false() {
        let profile = BrowserProfile::default();
        assert!(!profile.headless);
        assert!(profile.user_data_dir.is_none());
        assert!(profile.viewport.is_none());
    }

    /// `BrowserProfile::new()` round-trips fields and serde-cycles
    /// cleanly · including `viewport: Some((w, h))` tuple shape.
    #[test]
    fn browser_profile_new_and_serde() {
        let profile = BrowserProfile::new(
            Some(PathBuf::from("/tmp/chrome-profile")),
            true,
            Some((1920, 1080)),
        );
        let json = serde_json::to_string(&profile).expect("serialize profile");
        let back: BrowserProfile = serde_json::from_str(&json).expect("deserialize profile");
        assert_eq!(back, profile);
        assert_eq!(back.viewport, Some((1920, 1080)));
        assert!(back.headless);
    }

    /// `DomNode` with nested children round-trips through serde JSON ·
    /// proves recursive serde derive walks `Vec<DomNode>` correctly +
    /// `BTreeMap` attribute serialization is deterministic + cross-PR
    /// `Rect` dep wires cleanly. Per sprint plan §4 PR 5 test 3.
    #[test]
    fn dom_node_recursive_serde() {
        let mut child_attrs = BTreeMap::new();
        child_attrs.insert("type".to_string(), "submit".to_string());
        child_attrs.insert("class".to_string(), "btn primary".to_string());
        let child = DomNode::new(
            "button".to_string(),
            child_attrs,
            vec![],
            Some(Rect::new(100, 200, 80, 32)),
        );

        let mut parent_attrs = BTreeMap::new();
        parent_attrs.insert("id".to_string(), "login-form".to_string());
        let parent = DomNode::new(
            "form".to_string(),
            parent_attrs,
            vec![child.clone()],
            Some(Rect::new(50, 150, 400, 200)),
        );

        let json = serde_json::to_string(&parent).expect("serialize dom parent");
        let back: DomNode = serde_json::from_str(&json).expect("deserialize dom parent");
        assert_eq!(back, parent);
        assert_eq!(back.tag, "form");
        assert_eq!(back.children.len(), 1);
        assert_eq!(back.children[0], child);
        assert_eq!(back.children[0].tag, "button");
    }

    /// `BrowserAutomation` + `BrowserAutomationDyn` (the `Send`
    /// companion generated by `trait_variant::make`) are well-formed
    /// as generic bounds.
    ///
    /// Per DEV-2 M1.1 lesson · `trait_variant` 0.1 returns
    /// `impl Future + Send` which is NOT dyn-compatible.
    /// Use a generic-bound test (`T: BrowserAutomationDyn`) NOT a
    /// trait-object test. Pure compile-time check · no runtime path.
    /// If this compiles · the trait bounds are well-formed. Per sprint
    /// plan §4 PR 5 test 4.
    #[test]
    fn browser_automation_generic_bound_compile_check() {
        fn _accepts_browser_automation<T: BrowserAutomation>() {}
        fn _accepts_browser_automation_dyn<T: BrowserAutomationDyn>() {}
    }

    /// All public DTOs are Send + Sync · cohérent CANCEL SAFETY · types
    /// cross task boundaries in async L1 impls (CDP client wraps tokio
    /// channel + `Arc<Mutex<_>>` state).
    #[test]
    fn dtos_are_send_sync() {
        fn _assert<T: Send + Sync>() {}
        _assert::<BrowserSession>();
        _assert::<BrowserProfile>();
        _assert::<DomNode>();
    }
}
