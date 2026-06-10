// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-browser` — browser-automation L1 effect crate (CDP).
//!
//! Implements the L0.5 [`nika_kernel::io::browser::BrowserAutomationDyn`]
//! trait — the `Send` variant; the local `BrowserAutomation` arrives via the
//! kernel's one-way blanket impl (`Dyn` ⇒ local, never the reverse). Five
//! verbs (`launch` · `navigate` · `dom_snapshot` · `click_selector` ·
//! `screenshot`) give the Olympus cockpit a high-fidelity web arm next to the
//! desktop loop (`nika-screen` → `nika-ocr` → `nika-a11y` → `nika-input`):
//! structured DOM instead of OCR guesses, CDP determinism instead of
//! synthetic coordinates.
//!
//! # MANDATORY ADR-081 Guard 5 — selector clickjacking
//!
//! The threat: between the agent DECIDING to click `sel` (reasoning over an
//! earlier `dom_snapshot`) and the click executing, the page can mutate the
//! DOM — or a hostile page can alias the selector — so the click lands on a
//! DIFFERENT element (the web equivalent of clickjacking). The guard is a
//! verify-before-click contract with a PURE core:
//!
//! - [`SelectorExpectation`] — what the agent believes the selector targets
//!   (tag + attribute pins), captured at decision time from the SAME snapshot
//!   the agent reasoned over.
//! - [`verify_selector_target`] — pure · headless-testable · mutation-killed:
//!   the freshly-resolved element must match the expectation's tag, every
//!   pinned attribute, and be VISIBLE (non-zero bbox). Any mismatch returns
//!   [`BrowserError::SelectorFailed`] (NIKA-1404) — fail CLOSED, never click
//!   a guess.
//! - The B.3 `click_selector` path re-resolves `sel` FRESH via CDP, maps the
//!   element to a [`DomNode`], runs the pure verify, and only then dispatches
//!   the CDP click. Without a registered expectation the STRUCTURAL checks
//!   still apply (exactly-one match · visible · stable double-resolve).
//!
//! # Untrusted-input posture
//!
//! DOM snapshots come from arbitrary web pages — UNTRUSTED. The B.3 mapper is
//! depth-capped ([`MAX_DOM_DEPTH`] · the `nika-a11y` `MAX_WALK_DEPTH`
//! precedent) and the pure [`dom_depth`] measure is the headless-testable
//! core of that cap: a hostile page cannot stack-overflow or OOM the agent.
//!
//! # Backend (B.3 · `chromiumoxide` CDP · async-native)
//!
//! The CDP backend is async-native tokio — NO `spawn_blocking` (contrast the
//! sync-backend M2 crates): trait methods await CDP calls directly. Each
//! `launch` spawns ONE chromium child (`kill_on_drop(true)` per Invariant #11
//! — chromiumoxide sets it at spawn, verified on the 0.9.1 tarball) + ONE
//! owned Handler task pumping the CDP event loop (aborted on backend drop).
//! Chromium is DETECTED on the host (Chrome/Chromium/Edge), never downloaded
//! (the `fetcher` feature stays OFF — sovereignty): a missing binary surfaces
//! as [`BrowserError::BackendUnavailable`] (NIKA-1405) with install-a-browser
//! remediation. Navigation is gated by a pure RFC-3986 check (http/https
//! ONLY — `file://` and `javascript:` fail closed) BEFORE any CDP dispatch.
//!
//! CANCEL SAFETY: `launch` is NOT cancel-safe (child spawn) — chromiumoxide
//! kills the child on the launch future's drop (`kill_on_drop` + its own
//! partial-launch reaping). The query paths (`dom_snapshot` · `screenshot`)
//! are read-only CDP calls; a dropped future abandons the response without
//! page-side effects. `click_selector` is best-effort per the kernel trait
//! doc: the CDP click may already have fired when the future drops.

// Tests assert on Result/Option outcomes; `.unwrap()`/`.expect()` are the
// idiomatic test-failure path (never in `src/` non-test code per Diamond Rule).
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::collections::BTreeMap;

mod cdp;

pub use nika_kernel::io::browser::BrowserError as Error;
use nika_kernel::io::browser::{BrowserError, BrowserProfile, BrowserSession, DomNode};
use nika_kernel::io::screen::Frame;

/// Maximum DOM tree depth the snapshot mapper will descend (untrusted-input
/// cap — a hostile page nesting thousands of elements cannot stack-overflow
/// the agent). Matches the `nika-a11y` `MAX_WALK_DEPTH` discipline.
///
/// The cap protects TWO recursion surfaces: the mapper's descent AND the
/// mapped tree's later life — `DomNode`'s derive glue (Drop · `PartialEq` ·
/// serde) recurses over `children`, so an uncapped hostile tree would
/// overflow the stack the moment it is dropped or compared (empirically: a
/// 10 000-deep chain aborts a test thread in Drop). Capping at map time
/// bounds every downstream consumer.
pub const MAX_DOM_DEPTH: u16 = 512;

/// What the agent believes a selector targets — captured at DECISION time,
/// from the same `dom_snapshot` the agent reasoned over (Guard 5 · ADR-081).
///
/// `tag` pins the element name (lowercased, the [`DomNode`] normal form).
/// `attributes` pins any subset of HTML attributes the agent anchored on
/// (e.g. `id` · `name` · `type`) — EVERY pin must match exactly.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct SelectorExpectation {
    /// Expected lowercased element name (e.g. `"button"`).
    pub tag: String,
    /// Attribute pins · every entry must match the resolved element exactly.
    pub attributes: BTreeMap<String, String>,
}

impl SelectorExpectation {
    /// Construct a new selector expectation.
    ///
    /// Per Invariant #19 · every `#[non_exhaustive]` struct ships a `new()`.
    #[must_use]
    pub fn new(tag: String, attributes: BTreeMap<String, String>) -> Self {
        Self { tag, attributes }
    }
}

/// Guard 5 (ADR-081 · MANDATORY) · does the freshly-resolved element match
/// what the agent decided to click — **pure** · headless-testable ·
/// mutation-killed.
///
/// Three checks, ALL required (fail CLOSED on the first miss):
/// 1. **Tag** — `resolved.tag` equals the expectation's tag (both are the
///    lowercased [`DomNode`] normal form; comparison is case-insensitive to
///    survive a non-normalized caller).
/// 2. **Attribute pins** — every pinned `(name, value)` matches the resolved
///    element's attributes exactly. A missing attribute or different value is
///    a mismatch (the element the agent saw is not the element about to be
///    clicked).
/// 3. **Visibility** — the resolved element has a bbox with non-zero area.
///    A `display:none` / off-screen / zero-size element is the classic
///    clickjacking decoy: never click what the user cannot see.
///
/// # Errors
/// [`BrowserError::SelectorFailed`] (NIKA-1404) naming the first failed check
/// (tag · attribute · visibility). The error carries the EXPECTED shape, never
/// page-controlled content beyond the resolved tag name (a hostile page must
/// not inject arbitrary text into our error/journal surface).
pub fn verify_selector_target(
    resolved: &DomNode,
    expectation: &SelectorExpectation,
) -> Result<(), BrowserError> {
    if !resolved.tag.eq_ignore_ascii_case(&expectation.tag) {
        return Err(BrowserError::SelectorFailed {
            reason: format!(
                "guard-5 tag mismatch: expected <{}>, resolved a different element",
                expectation.tag
            ),
        });
    }
    for (name, want) in &expectation.attributes {
        if resolved.attributes.get(name) != Some(want) {
            return Err(BrowserError::SelectorFailed {
                reason: format!("guard-5 attribute pin mismatch on '{name}'"),
            });
        }
    }
    if !is_visible(resolved) {
        return Err(BrowserError::SelectorFailed {
            reason: "guard-5 visibility: resolved element has no renderable area".to_owned(),
        });
    }
    Ok(())
}

/// Structural visibility — the element renders with non-zero area. The
/// clickjacking decoy classes (`display:none` · zero-size · unrenderable) all
/// surface as `bbox: None` or a zero dimension in the [`DomNode`] mapping.
#[must_use]
pub fn is_visible(node: &DomNode) -> bool {
    node.bbox
        .as_ref()
        .is_some_and(|b| b.width > 0 && b.height > 0)
}

/// Measure a DOM tree's depth — the pure core of the [`MAX_DOM_DEPTH`]
/// untrusted-input cap (the B.3 mapper truncates beyond it; this measure is
/// what tests + the mapper share). Iterative (explicit stack): measuring a
/// hostile deep tree must not itself stack-overflow.
#[must_use]
pub fn dom_depth(node: &DomNode) -> u32 {
    let mut max = 0u32;
    let mut stack: Vec<(&DomNode, u32)> = vec![(node, 1)];
    while let Some((n, d)) = stack.pop() {
        max = max.max(d);
        for child in &n.children {
            stack.push((child, d.saturating_add(1)));
        }
    }
    max
}

/// One live CDP session: the chromium child (killed on drop · Invariant #11
/// via chromiumoxide's `kill_on_drop(true)`), its page, and the owned Handler
/// task that pumps the CDP event loop (aborted on session/backend drop).
struct SessionHandle {
    /// Kept alive for the child-process lifetime — dropping it kills chromium.
    _browser: chromiumoxide::Browser,
    page: chromiumoxide::Page,
    handler_task: tokio::task::JoinHandle<()>,
}

impl std::fmt::Debug for SessionHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionHandle").finish_non_exhaustive()
    }
}

/// CDP browser backend (cross-platform via `chromiumoxide`). Owns the session
/// registry; each `launch` spawns one chromium child (`kill_on_drop(true)` ·
/// Invariant #11) + one CDP Handler task (aborted on drop).
///
/// Deliberately NOT `Clone`/`Copy`/`Default`: the registry owns child-process
/// lifetimes (no aliased owners), and a session-capable handle must only come
/// from [`ChromiumBrowser::new`] (the nika-input no-`Default` precedent —
/// derives are part of the security surface).
#[derive(Debug)]
#[non_exhaustive]
pub struct ChromiumBrowser {
    /// Live sessions · keyed by session id.
    sessions: std::sync::Mutex<BTreeMap<String, SessionHandle>>,
    /// Per-session click expectations (Guard 5) · keyed by session id.
    expectations: std::sync::Mutex<BTreeMap<String, SelectorExpectation>>,
    /// Monotonic session-id counter.
    next_id: std::sync::atomic::AtomicU64,
}

impl Drop for ChromiumBrowser {
    fn drop(&mut self) {
        // Abort every Handler task; each Browser drop kills its chromium
        // child (kill_on_drop · Invariant #11 — no orphan browsers).
        if let Ok(mut sessions) = self.sessions.lock() {
            for (_, handle) in std::mem::take(&mut *sessions) {
                handle.handler_task.abort();
            }
        }
    }
}

impl ChromiumBrowser {
    /// Construct the backend — hermetic (no chromium spawned · no I/O ·
    /// `cargo test --lib` never needs a browser). Chromium is located and
    /// spawned per `launch` call.
    ///
    /// # Errors
    /// None today (the `Result` shape is the forward-compat seam every
    /// kernel-facing constructor keeps).
    pub fn new() -> Result<Self, BrowserError> {
        Ok(Self {
            sessions: std::sync::Mutex::new(BTreeMap::new()),
            expectations: std::sync::Mutex::new(BTreeMap::new()),
            next_id: std::sync::atomic::AtomicU64::new(1),
        })
    }

    /// Clone the live page handle for `session_id` (chromiumoxide `Page` is a
    /// cheap `Arc` clone — the registry lock is never held across an await).
    fn page(&self, session_id: &str) -> Result<chromiumoxide::Page, BrowserError> {
        let sessions = self.sessions.lock().map_err(|_| poisoned())?;
        sessions
            .get(session_id)
            .map(|h| h.page.clone())
            .ok_or_else(|| BrowserError::SessionNotFound {
                session: session_id.to_owned(),
            })
    }

    /// Register the Guard-5 expectation for the NEXT `click_selector` on
    /// `session` — captured at decision time from the same snapshot the agent
    /// reasoned over. Consumed (removed) by the click path; one expectation
    /// guards one click.
    ///
    /// # Errors
    /// [`BrowserError::SessionNotFound`] is NOT raised here (the registry is
    /// session-id-keyed and the id is validated at click time) — reserved for
    /// B.3 when live sessions exist to validate against.
    pub fn set_click_expectation(
        &self,
        session: &BrowserSession,
        expectation: SelectorExpectation,
    ) -> Result<(), BrowserError> {
        let mut map = self
            .expectations
            .lock()
            .map_err(|_| BrowserError::SelectorFailed {
                reason: "guard-5 expectation registry poisoned".to_owned(),
            })?;
        map.insert(session.id.clone(), expectation);
        Ok(())
    }

    /// Take (consume) the registered expectation for `session`, if any.
    fn take_click_expectation(&self, session_id: &str) -> Option<SelectorExpectation> {
        self.expectations
            .lock()
            .ok()
            .and_then(|mut map| map.remove(session_id))
    }
}

/// Registry-lock poisoning = a panicked sibling thread — an internal fault
/// surfaced on the transient channel (retry-able · NIKA-1406).
fn poisoned() -> BrowserError {
    BrowserError::TaskJoinFailed {
        reason: "session registry poisoned (a holder thread panicked)".to_owned(),
    }
}

/// Epoch-nanosecond timestamp for screenshot frames (ns canon · EC-4).
/// Observability metadata only — NOT a security gate (contrast nika-input's
/// monotonic consent clock).
fn epoch_now_ns() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX))
}

/// Map a chromiumoxide error on the SELECTOR path. The selector string is
/// agent-authored (safe), but CDP error payloads can embed page-derived
/// detail — forward the protocol message only (no DOM content rides
/// chromiumoxide error Display for find/describe failures).
fn selector_err(e: &chromiumoxide::error::CdpError) -> BrowserError {
    BrowserError::SelectorFailed {
        reason: format!("selector resolution failed: {e}"),
    }
}

impl nika_kernel::io::browser::BrowserAutomationDyn for ChromiumBrowser {
    async fn launch(&self, profile: &BrowserProfile) -> Result<BrowserSession, BrowserError> {
        use chromiumoxide::browser::{Browser, BrowserConfig};
        use futures_util::StreamExt;

        let mut config = BrowserConfig::builder();
        config = if profile.headless {
            config.new_headless_mode() // chrome's `--headless=new` (the maintained mode)
        } else {
            config.with_head() // visible window — the kernel DTO default (transparency posture)
        };
        if let Some(dir) = &profile.user_data_dir {
            config = config.user_data_dir(dir);
        }
        if let Some((w, h)) = profile.viewport {
            config = config.window_size(w, h);
        }
        // Chrome/Chromium/Edge is DETECTED, never downloaded (fetcher OFF —
        // sovereignty). A missing binary is a backend-absence, not a launch
        // fault: remediation = install a browser.
        let config = config.build().map_err(|reason| {
            let _ = reason; // detection detail is host-environment, not actionable here
            BrowserError::BackendUnavailable
        })?;
        let (browser, mut handler) =
            Browser::launch(config)
                .await
                .map_err(|e| BrowserError::LaunchFailed {
                    reason: format!("chromium launch failed: {e}"),
                })?;
        // ONE owned Handler task per session pumps the CDP event loop; it
        // ends when the browser closes (stream None) or on abort at drop.
        let handler_task = tokio::spawn(async move {
            while let Some(event) = handler.next().await {
                // Protocol-level errors are the session's own dispatch errors
                // surfaced elsewhere; the pump just keeps the loop alive.
                let _ = event;
            }
        });
        let page =
            browser
                .new_page("about:blank")
                .await
                .map_err(|e| BrowserError::LaunchFailed {
                    reason: format!("initial page creation failed: {e}"),
                })?;
        let id = format!(
            "cdp-{}",
            self.next_id
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        let mut sessions = self.sessions.lock().map_err(|_| poisoned())?;
        sessions.insert(
            id.clone(),
            SessionHandle {
                _browser: browser,
                page,
                handler_task,
            },
        );
        Ok(BrowserSession::new(id, None))
    }

    async fn navigate(&self, session: &BrowserSession, url: &str) -> Result<(), BrowserError> {
        // Pure RFC-3986 gate BEFORE any CDP dispatch (http/https only —
        // file:// and javascript: fail closed per the cdp module).
        cdp::validate_absolute_url(url)?;
        let page = self.page(&session.id)?;
        page.goto(url)
            .await
            .map_err(|e| BrowserError::NavigationFailed {
                reason: format!("navigation to {url:?} failed: {e}"),
            })?;
        Ok(())
    }

    async fn dom_snapshot(&self, session: &BrowserSession) -> Result<DomNode, BrowserError> {
        use chromiumoxide::cdp::browser_protocol::dom::GetDocumentParams;
        let page = self.page(&session.id)?;
        // depth -1 = full tree in ONE query (the per-node mapper applies the
        // MAX_DOM_DEPTH cap — a hostile page cannot recurse past it).
        let resp = page
            .execute(GetDocumentParams::builder().depth(-1).build())
            .await
            .map_err(|e| selector_err(&e))?;
        cdp::map_document(&resp.result.root)
    }

    async fn click_selector(
        &self,
        session: &BrowserSession,
        sel: &str,
    ) -> Result<(), BrowserError> {
        let page = self.page(&session.id)?;
        // Guard 5 ordering FIXED since B.2: expectation consumed, fresh
        // resolve, PURE verify — only then the CDP click.
        let expectation = self.take_click_expectation(&session.id);

        // Structural check 1 · the selector matches EXACTLY ONE element.
        let matches = page
            .find_elements(sel)
            .await
            .map_err(|e| selector_err(&e))?;
        if matches.len() != 1 {
            return Err(BrowserError::SelectorFailed {
                reason: format!(
                    "guard-5 structural: selector must match exactly one element (matched {})",
                    matches.len()
                ),
            });
        }
        // Structural check 2 · stable double-resolve (same backend node).
        let first = &matches[0];
        let second = page.find_element(sel).await.map_err(|e| selector_err(&e))?;
        if first.backend_node_id != second.backend_node_id {
            return Err(BrowserError::SelectorFailed {
                reason: "guard-5 structural: selector resolution is unstable (DOM mutated \
                         between resolves)"
                    .to_owned(),
            });
        }
        // Build the resolved DomNode (live bbox → visibility).
        let desc = second.description().await.map_err(|e| selector_err(&e))?;
        let bbox = match second.bounding_box().await {
            Ok(b) => Some(cdp::bbox_to_rect(b.x, b.y, b.width, b.height)),
            Err(_) => None, // unrenderable → INVISIBLE to the guard
        };
        let resolved = DomNode::new(
            desc.local_name.to_ascii_lowercase(),
            cdp::attrs_to_map(desc.attributes.as_deref().unwrap_or_default()),
            Vec::new(),
            bbox,
        );
        // Guard 5 · PURE verify (expectation pins) or structural visibility.
        match expectation {
            Some(exp) => verify_selector_target(&resolved, &exp)?,
            None => {
                if !is_visible(&resolved) {
                    return Err(BrowserError::SelectorFailed {
                        reason: "guard-5 visibility: resolved element has no renderable area"
                            .to_owned(),
                    });
                }
            }
        }
        second.click().await.map_err(|e| selector_err(&e))?;
        Ok(())
    }

    async fn screenshot(&self, session: &BrowserSession) -> Result<Frame, BrowserError> {
        use chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat;
        use chromiumoxide::page::ScreenshotParams;
        let page = self.page(&session.id)?;
        let png_bytes = page
            .screenshot(
                ScreenshotParams::builder()
                    .format(CaptureScreenshotFormat::Png)
                    .build(),
            )
            .await
            .map_err(|e| BrowserError::TaskJoinFailed {
                reason: format!("screenshot capture failed: {e}"),
            })?;
        cdp::png_to_frame(&png_bytes, epoch_now_ns())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_kernel::io::browser::BrowserAutomationDyn;
    use nika_kernel::io::screen::Rect;
    use nika_kernel::prelude::{NikaErrorCode, codes};

    fn node(tag: &str, attrs: &[(&str, &str)], bbox: Option<Rect>) -> DomNode {
        DomNode::new(
            tag.to_owned(),
            attrs
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
            Vec::new(),
            bbox,
        )
    }

    fn visible_box() -> Rect {
        Rect::new(10, 10, 120, 40)
    }

    // ─── Guard 5 · verify_selector_target (MANDATORY · pure) ────────────────

    #[test]
    fn guard5_accepts_exact_match() {
        let resolved = node(
            "button",
            &[("id", "submit"), ("type", "submit")],
            Some(visible_box()),
        );
        let exp = SelectorExpectation::new(
            "button".to_owned(),
            [("id".to_owned(), "submit".to_owned())].into(),
        );
        assert!(verify_selector_target(&resolved, &exp).is_ok());
    }

    #[test]
    fn guard5_rejects_tag_swap() {
        // The clickjack: agent decided to click a <button>, page swapped in <a>.
        let resolved = node("a", &[("id", "submit")], Some(visible_box()));
        let exp = SelectorExpectation::new("button".to_owned(), BTreeMap::new());
        let err = verify_selector_target(&resolved, &exp).expect_err("tag swap must fail");
        assert_eq!(err.nika_code(), codes::NIKA_1404);
    }

    #[test]
    fn guard5_tag_compare_is_case_insensitive() {
        // DomNode normal form is lowercase, but a non-normalized caller must
        // not bypass nor false-positive the guard.
        let resolved = node("button", &[], Some(visible_box()));
        let exp = SelectorExpectation::new("BUTTON".to_owned(), BTreeMap::new());
        assert!(verify_selector_target(&resolved, &exp).is_ok());
    }

    #[test]
    fn guard5_rejects_attribute_pin_mismatch_and_absence() {
        let exp = SelectorExpectation::new(
            "button".to_owned(),
            [("id".to_owned(), "submit".to_owned())].into(),
        );
        // Different value.
        let swapped = node("button", &[("id", "cancel")], Some(visible_box()));
        assert_eq!(
            verify_selector_target(&swapped, &exp)
                .expect_err("value mismatch")
                .nika_code(),
            codes::NIKA_1404
        );
        // Attribute absent entirely.
        let stripped = node("button", &[], Some(visible_box()));
        assert_eq!(
            verify_selector_target(&stripped, &exp)
                .expect_err("missing pin")
                .nika_code(),
            codes::NIKA_1404
        );
    }

    #[test]
    fn guard5_rejects_invisible_targets() {
        let exp = SelectorExpectation::new("button".to_owned(), BTreeMap::new());
        // display:none / unrenderable → bbox None.
        let unrendered = node("button", &[], None);
        assert!(verify_selector_target(&unrendered, &exp).is_err());
        // Zero-width decoy.
        let zero_w = node("button", &[], Some(Rect::new(0, 0, 0, 40)));
        assert!(verify_selector_target(&zero_w, &exp).is_err());
        // Zero-height decoy.
        let zero_h = node("button", &[], Some(Rect::new(0, 0, 40, 0)));
        assert!(verify_selector_target(&zero_h, &exp).is_err());
    }

    #[test]
    fn guard5_error_never_echoes_page_controlled_attribute_values() {
        // A hostile page controls the resolved element's attribute VALUES —
        // the guard error must not forward them into our error/journal surface.
        let resolved = node(
            "button",
            &[("id", "<script>alert(1)</script>injected")],
            Some(visible_box()),
        );
        let exp = SelectorExpectation::new(
            "button".to_owned(),
            [("id".to_owned(), "submit".to_owned())].into(),
        );
        let err = verify_selector_target(&resolved, &exp).expect_err("mismatch");
        let shown = err.to_string();
        assert!(!shown.contains("injected"), "page value leaked: {shown}");
        assert!(!shown.contains("script"), "page value leaked: {shown}");
    }

    // ─── visibility + depth (pure) ───────────────────────────────────────────

    #[test]
    fn is_visible_requires_positive_area() {
        assert!(is_visible(&node("div", &[], Some(visible_box()))));
        assert!(!is_visible(&node("div", &[], None)));
        assert!(!is_visible(&node("div", &[], Some(Rect::new(5, 5, 0, 10)))));
        assert!(!is_visible(&node("div", &[], Some(Rect::new(5, 5, 10, 0)))));
    }

    #[test]
    fn dom_depth_measures_beyond_the_cap_iteratively() {
        // The measure must survive depths BEYOND the cap without recursion
        // (the mapper consults it BEFORE truncating). Depth stays bounded
        // here because DomNode's own derive glue (Drop · PartialEq · serde)
        // recurses over children — the empirical reason MAX_DOM_DEPTH exists:
        // a 10 000-deep chain aborts the thread in DROP, not in dom_depth.
        let beyond_cap = u32::from(MAX_DOM_DEPTH) * 2;
        let mut tree = node("div", &[], None);
        for _ in 0..beyond_cap {
            tree = DomNode::new("div".to_owned(), BTreeMap::new(), vec![tree], None);
        }
        assert_eq!(dom_depth(&tree), beyond_cap + 1);
        // Dismantle iteratively — dropping the deep chain recursively is the
        // exact failure mode the cap prevents in production.
        let mut worklist = vec![tree];
        while let Some(mut n) = worklist.pop() {
            worklist.append(&mut n.children);
        }
        assert_eq!(dom_depth(&node("p", &[], None)), 1);
        // Depth is the MAX across branches, not the sum.
        let wide = DomNode::new(
            "div".to_owned(),
            BTreeMap::new(),
            vec![node("a", &[], None), node("b", &[], None)],
            None,
        );
        assert_eq!(dom_depth(&wide), 2);
    }

    // ─── Skeleton dispatch (hermetic — no chromium) ─────────────────────────

    #[tokio::test]
    async fn session_scoped_methods_reject_unknown_sessions_hermetically() {
        // The live backend resolves the SESSION before any CDP dispatch — an
        // unknown id is NIKA-1403 and no browser is ever touched (these tests
        // stay hermetic; real-chromium dispatch is the #[ignore] smoke below).
        let browser = ChromiumBrowser::new().expect("construct");
        let ghost = BrowserSession::new("never-launched".to_owned(), None);
        assert_eq!(
            browser
                .navigate(&ghost, "https://example.org")
                .await
                .expect_err("unknown session")
                .nika_code(),
            codes::NIKA_1403
        );
        assert_eq!(
            browser
                .dom_snapshot(&ghost)
                .await
                .expect_err("unknown session")
                .nika_code(),
            codes::NIKA_1403
        );
        assert_eq!(
            browser
                .click_selector(&ghost, "#submit")
                .await
                .expect_err("unknown session")
                .nika_code(),
            codes::NIKA_1403
        );
        assert_eq!(
            browser
                .screenshot(&ghost)
                .await
                .expect_err("unknown session")
                .nika_code(),
            codes::NIKA_1403
        );
    }

    #[tokio::test]
    async fn navigate_rejects_malformed_urls_before_any_session_lookup_question() {
        // The pure URL gate runs FIRST: a refused scheme errors NIKA-1402 even
        // on an unknown session (never leak session-existence on a bad input).
        let browser = ChromiumBrowser::new().expect("construct");
        let ghost = BrowserSession::new("never-launched".to_owned(), None);
        for bad in ["javascript:alert(1)", "file:///etc/passwd", "not-a-url"] {
            assert_eq!(
                browser
                    .navigate(&ghost, bad)
                    .await
                    .expect_err("refused url")
                    .nika_code(),
                codes::NIKA_1402,
                "{bad:?}"
            );
        }
    }

    #[tokio::test]
    async fn click_expectation_survives_a_failed_session_lookup() {
        // Guard-5 bookkeeping: the expectation is consumed by a click ATTEMPT
        // on a LIVE session only — a failed session lookup must not burn it.
        let browser = ChromiumBrowser::new().expect("construct");
        let session = BrowserSession::new("s-1".to_owned(), None);
        let exp = SelectorExpectation::new("button".to_owned(), BTreeMap::new());
        browser
            .set_click_expectation(&session, exp)
            .expect("register");
        let err = browser
            .click_selector(&session, "#submit")
            .await
            .expect_err("no live session");
        assert_eq!(err.nika_code(), codes::NIKA_1403);
        assert!(
            browser.take_click_expectation("s-1").is_some(),
            "expectation not consumed by a failed lookup"
        );
    }

    /// Real-chromium smoke (B.3 acceptance) — needs an installed
    /// Chrome/Chromium/Edge. Run: `cargo test -p nika-browser -- --ignored`.
    #[tokio::test]
    #[ignore = "spawns a real chromium · launch/snapshot/screenshot/guard-5 round-trip"]
    async fn smoke_real_chromium_round_trip() {
        let browser = ChromiumBrowser::new().expect("construct");
        let profile = BrowserProfile::new(None, true, Some((800, 600)));
        let session = browser.launch(&profile).await.expect("launch headless");
        // about:blank still has a DOM: <html><head/><body/></html>.
        let dom = browser.dom_snapshot(&session).await.expect("snapshot");
        assert_eq!(dom.tag, "html");
        assert!(dom.children.iter().any(|c| c.tag == "body"));
        let frame = browser.screenshot(&session).await.expect("screenshot");
        assert_eq!((frame.width, frame.height), (800, 600));
        assert_eq!(frame.pixels.len(), 800 * 600 * 4, "RGBA8 contract");
        // Guard-5 structural: a selector matching nothing fails CLOSED.
        let err = browser
            .click_selector(&session, "#does-not-exist")
            .await
            .expect_err("no match");
        assert_eq!(err.nika_code(), codes::NIKA_1404);
        // Guard-5 expectation: body resolves but a tag-swap pin refuses to click.
        browser
            .set_click_expectation(
                &session,
                SelectorExpectation::new("button".to_owned(), BTreeMap::new()),
            )
            .expect("register");
        let err = browser
            .click_selector(&session, "body")
            .await
            .expect_err("tag swap pinned");
        assert_eq!(err.nika_code(), codes::NIKA_1404);
    }

    proptest::proptest! {
        /// Guard-5 invariant under arbitrary resolved elements: if the tag
        /// differs (case-insensitively), verification NEVER succeeds — no
        /// attribute/visibility combination can compensate a tag swap.
        #[test]
        fn guard5_tag_swap_never_verifies(
            resolved_tag in "[a-z]{1,12}",
            expected_tag in "[a-z]{1,12}",
            id in "[a-zA-Z0-9_-]{0,24}",
        ) {
            proptest::prop_assume!(!resolved_tag.eq_ignore_ascii_case(&expected_tag));
            let resolved = DomNode::new(
                resolved_tag,
                [("id".to_owned(), id.clone())].into(),
                Vec::new(),
                Some(nika_kernel::io::screen::Rect::new(0, 0, 100, 30)),
            );
            let exp = SelectorExpectation::new(
                expected_tag,
                [("id".to_owned(), id)].into(),
            );
            proptest::prop_assert!(verify_selector_target(&resolved, &exp).is_err());
        }
    }
}
