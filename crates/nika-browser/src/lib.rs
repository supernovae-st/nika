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
//! # Status (B.2 · the pure security core)
//!
//! B.2 ships Guard 5's pure core + the `ChromiumBrowser` skeleton. The
//! `chromiumoxide` CDP backend (async-native tokio — NO `spawn_blocking`,
//! contrast the sync-backend M2 crates; one Handler task per session,
//! `kill_on_drop(true)` on the chromium child per Invariant #11) lands at
//! **B.3**; until then the dispatch methods return
//! [`BrowserError::BackendUnavailable`] (NIKA-1405).

// Tests assert on Result/Option outcomes; `.unwrap()`/`.expect()` are the
// idiomatic test-failure path (never in `src/` non-test code per Diamond Rule).
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::collections::BTreeMap;

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

/// CDP browser backend (cross-platform via `chromiumoxide` at B.3). Owns the
/// session registry; each `launch` spawns one chromium child
/// (`kill_on_drop(true)` · Invariant #11) + one CDP Handler task.
///
/// Deliberately NOT `Clone`/`Copy`/`Default`: the registry owns child-process
/// lifetimes (no aliased owners), and a session-capable handle must only come
/// from [`ChromiumBrowser::new`] (the nika-input no-`Default` precedent —
/// derives are part of the security surface).
#[derive(Debug)]
#[non_exhaustive]
pub struct ChromiumBrowser {
    /// Per-session click expectations (Guard 5) · keyed by session id.
    /// B.3 extends this registry with the live Page handles.
    expectations: std::sync::Mutex<BTreeMap<String, SelectorExpectation>>,
}

impl ChromiumBrowser {
    /// Construct the backend — hermetic (no chromium spawned · no I/O ·
    /// `cargo test --lib` never needs a browser). Chromium is located and
    /// spawned per `launch` call (B.3).
    ///
    /// # Errors
    /// None today (the `Result` shape is the forward-compat seam every
    /// kernel-facing constructor keeps).
    pub fn new() -> Result<Self, BrowserError> {
        Ok(Self {
            expectations: std::sync::Mutex::new(BTreeMap::new()),
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

impl nika_kernel::io::browser::BrowserAutomationDyn for ChromiumBrowser {
    async fn launch(&self, _profile: &BrowserProfile) -> Result<BrowserSession, BrowserError> {
        // B.3 wires chromiumoxide Browser::launch (kill_on_drop child +
        // Handler task) — until then: no backend on this build.
        Err(BrowserError::BackendUnavailable)
    }

    async fn navigate(&self, _session: &BrowserSession, _url: &str) -> Result<(), BrowserError> {
        Err(BrowserError::BackendUnavailable) // B.3 wires CDP Page.navigate
    }

    async fn dom_snapshot(&self, _session: &BrowserSession) -> Result<DomNode, BrowserError> {
        Err(BrowserError::BackendUnavailable) // B.3 wires DOM.getDocument + capped mapper
    }

    async fn click_selector(
        &self,
        session: &BrowserSession,
        _sel: &str,
    ) -> Result<(), BrowserError> {
        // Guard 5 ordering is FIXED here at B.2: the expectation is consumed
        // and verified BEFORE any backend dispatch (B.3 inserts the fresh CDP
        // re-resolve between take and verify — the verify call site does not
        // move). With no live backend the resolved element cannot exist yet,
        // so a registered expectation fails CLOSED and no click happens.
        if let Some(_expectation) = self.take_click_expectation(&session.id) {
            // B.3: resolve sel fresh → map to DomNode → verify_selector_target
            // → only then CDP click. B.2 has no resolver: fail closed.
            return Err(BrowserError::BackendUnavailable);
        }
        Err(BrowserError::BackendUnavailable) // B.3 wires structural checks + click
    }

    async fn screenshot(&self, _session: &BrowserSession) -> Result<Frame, BrowserError> {
        Err(BrowserError::BackendUnavailable) // B.3 wires Page.captureScreenshot → RGBA8 Frame
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
    async fn all_five_methods_stub_backend_unavailable() {
        let browser = ChromiumBrowser::new().expect("construct");
        let session = BrowserSession::new("s-1".to_owned(), None);
        let profile = BrowserProfile::default();
        assert_eq!(
            browser.launch(&profile).await.expect_err("b.2").nika_code(),
            codes::NIKA_1405
        );
        assert_eq!(
            browser
                .navigate(&session, "https://example.org")
                .await
                .expect_err("b.2")
                .nika_code(),
            codes::NIKA_1405
        );
        assert_eq!(
            browser
                .dom_snapshot(&session)
                .await
                .expect_err("b.2")
                .nika_code(),
            codes::NIKA_1405
        );
        assert_eq!(
            browser
                .click_selector(&session, "#submit")
                .await
                .expect_err("b.2")
                .nika_code(),
            codes::NIKA_1405
        );
        assert_eq!(
            browser
                .screenshot(&session)
                .await
                .expect_err("b.2")
                .nika_code(),
            codes::NIKA_1405
        );
    }

    #[tokio::test]
    async fn click_expectation_is_consumed_once_and_fails_closed() {
        let browser = ChromiumBrowser::new().expect("construct");
        let session = BrowserSession::new("s-1".to_owned(), None);
        let exp = SelectorExpectation::new("button".to_owned(), BTreeMap::new());
        browser
            .set_click_expectation(&session, exp)
            .expect("register");
        // First click consumes the expectation; with no backend it fails CLOSED.
        let err = browser
            .click_selector(&session, "#submit")
            .await
            .expect_err("fail closed");
        assert_eq!(err.nika_code(), codes::NIKA_1405);
        // Consumed: the registry is empty again (one expectation = one click).
        assert!(browser.take_click_expectation("s-1").is_none());
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
