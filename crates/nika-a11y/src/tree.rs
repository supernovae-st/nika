// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Accessibility backend — implements the L0.5 `AccessibilityTree` trait.
//!
//! **B.2 skeleton.** `snapshot` / `find` / `resolve_ref` return the
//! `BackendNotWired` placeholder; B.3 wires the macOS `accessibility`
//! (`AXUIElement`) walk inside `tokio::task::spawn_blocking` (the handle is
//! `!Send` · stays worker-local · same pattern as `nika-screen`'s `xcap`).
//!
//! **Mandatory Guard 3 (ADR-081 · AX-secure-field redaction) is headless-
//! complete at B.2** — it is a PURE recursive tree-transform
//! ([`redact_secure_fields`]) that strips `value` from any secure-text node
//! ([`is_secure_field`]) so passwords NEVER leave the crate. B.3 applies it to
//! every `snapshot`/`find`/`resolve_ref` result before return. The OS walk
//! that builds the raw tree is the only Rule-2-exempt (FFI) residue.

use std::sync::Mutex;

use nika_kernel::io::a11y::{AccessibilityTree, AxNode, AxQuery, AxRole};

use crate::error::A11yError;

/// Accessibility backend (macOS `AXUIElement` via the safe `accessibility`
/// crate). Caches the last redacted snapshot as the per-session
/// ref-resolution store for `resolve_ref`.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct AxBackend {
    /// Last redacted snapshot · the `@e<N>` → node resolution cache (best-
    /// effort · populated on every `snapshot`/`find`).
    last_snapshot: Mutex<Option<AxNode>>,
}

impl AxBackend {
    /// Construct an accessibility backend bound to the focused application.
    ///
    /// # Errors
    /// None today · B.4 may add a macOS Accessibility-trust pre-check
    /// (`AXIsProcessTrusted`) → [`A11yError::PermissionDenied`].
    pub fn new() -> Result<Self, A11yError> {
        Ok(Self::default())
    }

    /// Walk the focused window, apply the MANDATORY Guard 3 redaction, cache
    /// the result for `resolve_ref`, and return it.
    ///
    /// The sync OS walk runs in `spawn_blocking` — the `!Send` `AXUIElement`
    /// handle is created + dropped worker-local (kernel CANCEL SAFETY: a
    /// dropped future abandons the read with no caller-visible state · the
    /// blocking task finishes on the pool and its result is discarded).
    async fn redacted_snapshot(&self) -> std::io::Result<AxNode> {
        let raw = tokio::task::spawn_blocking(walk_focused_tree)
            .await
            .map_err(|e| A11yError::TaskJoinFailed {
                reason: e.to_string(),
            })??;
        let tree = redact_secure_fields(raw);
        if let Ok(mut cache) = self.last_snapshot.lock() {
            *cache = Some(tree.clone());
        }
        Ok(tree)
    }

    /// Seed the ref-resolution cache directly (test-only · lets `resolve_ref`
    /// be exercised headlessly without a real `AXUIElement` walk).
    #[cfg(test)]
    fn seed_cache_for_test(&self, tree: AxNode) {
        if let Ok(mut cache) = self.last_snapshot.lock() {
            *cache = Some(tree);
        }
    }
}

/// Map a platform AX role string to the canonical [`AxRole`] — **pure** ·
/// headless-testable. Unmapped roles fall back to [`AxRole::Unknown`]
/// (extended additively as more macOS roles surface).
fn ax_role_from_str(raw: &str) -> AxRole {
    match raw {
        "AXButton" => AxRole::Button,
        "AXLink" => AxRole::Link,
        "AXTextField" | "AXTextArea" => AxRole::TextField,
        "AXImage" => AxRole::Image,
        "AXGroup" => AxRole::Group,
        "AXWindow" => AxRole::Window,
        "AXMenu" => AxRole::Menu,
        "AXStaticText" => AxRole::StaticText,
        "AXList" => AxRole::List,
        "AXRow" | "AXCell" | "AXMenuItem" => AxRole::ListItem,
        "AXHeading" => AxRole::Heading,
        _ => AxRole::Unknown,
    }
}

/// Find a node by its stable `@e<N>` id in a cached tree — **pure** ·
/// headless-testable. Returns a clone (the cache stays owned).
fn find_by_id(node: &AxNode, id: &str) -> Option<AxNode> {
    if node.id == id {
        return Some(node.clone());
    }
    node.children.iter().find_map(|c| find_by_id(c, id))
}

/// Assemble an [`AxNode`] from the raw strings a backend read off a platform
/// element — **pure** (no FFI · headless-testable · mutation-killable). Maps
/// the role, drops empty `title`/`subrole`, and records a non-empty subrole in
/// `attributes["AXSubrole"]` (the secure-field marker Guard 3 reads). `bbox` is
/// `None` (frame→`Rect` is a later refinement).
fn assemble_node(
    id: String,
    role_str: &str,
    title: Option<String>,
    value: Option<String>,
    subrole: Option<String>,
    children: Vec<AxNode>,
) -> AxNode {
    use std::collections::BTreeMap;

    let role = ax_role_from_str(role_str);
    let label = title.filter(|s| !s.is_empty());
    let mut attributes = BTreeMap::new();
    if let Some(sub) = subrole.filter(|s| !s.is_empty()) {
        attributes.insert("AXSubrole".to_string(), sub);
    }
    AxNode::new(id, role, label, value, None, children, attributes)
}

/// Walk the focused window's accessibility tree into an [`AxNode`].
///
/// macOS · `AXUIElement::system_wide().focused_window()` rooted recursive walk
/// (role/label/value/subrole/children). The `value` of secure fields is
/// stripped downstream by [`redact_secure_fields`] (Guard 3). Non-macOS ·
/// [`A11yError::BackendUnavailable`] until AT-SPI / UIA backends land (§4).
#[cfg(target_os = "macos")]
fn walk_focused_tree() -> Result<AxNode, A11yError> {
    use accessibility::{AXUIElement, AXUIElementAttributes};

    let root = AXUIElement::system_wide()
        .focused_window()
        .map_err(|_| A11yError::NoFocusedApplication)?;
    let mut counter: u32 = 0;
    Ok(build_node(&root, &mut counter, 0))
}

/// Hard recursion cap for the AX walk · the focused app's tree is untrusted
/// input, so a pathologically deep (or cyclic) tree MUST NOT overflow the
/// stack. Generous — native AX trees rarely exceed a few dozen levels;
/// children below the cap are truncated (the node itself is kept).
#[cfg(target_os = "macos")]
const MAX_WALK_DEPTH: u16 = 512;

/// Recursively map one `AXUIElement` to an [`AxNode`] — macOS. Assigns a
/// stable per-walk `@e<N>` id. `bbox` is `None` at B.3 (frame→`Rect` mapping
/// is a later refinement). Attribute-read failures degrade to `None`/empty
/// (a partial tree is better than no tree · the redaction guard still runs).
/// `depth` bounds recursion at [`MAX_WALK_DEPTH`] (untrusted-input safety).
#[cfg(target_os = "macos")]
fn build_node(elem: &accessibility::AXUIElement, counter: &mut u32, depth: u16) -> AxNode {
    use accessibility::AXUIElementAttributes;
    use core_foundation::string::CFString;

    *counter += 1;
    let id = format!("@e{counter}");

    // Raw FFI reads (Rule-2 exempt · need a real AXUIElement). Failures degrade
    // to None — the pure assemble_node + redaction guard still run.
    let role_str = elem.role().map(|s| s.to_string()).unwrap_or_default();
    let title = elem.title().ok().map(|s| s.to_string());
    let value = elem
        .value()
        .ok()
        .and_then(|v| v.downcast::<CFString>())
        .map(|s| s.to_string());
    let subrole = elem.subrole().ok().map(|s| s.to_string());

    let children = if depth >= MAX_WALK_DEPTH {
        Vec::new() // truncate below the cap · keep this node, drop the subtree
    } else {
        elem.children()
            .map(|arr| {
                arr.iter()
                    .map(|c| build_node(&c, counter, depth + 1))
                    .collect()
            })
            .unwrap_or_default()
    };

    assemble_node(id, &role_str, title, value, subrole, children)
}

/// Non-macOS · no accessibility backend yet (§4 macOS-first · Linux AT-SPI /
/// Windows UIA land additively on a consumer signal).
#[cfg(not(target_os = "macos"))]
fn walk_focused_tree() -> Result<AxNode, A11yError> {
    Err(A11yError::BackendUnavailable)
}

/// True when a node is a secure-text field whose `value` MUST be redacted —
/// **pure** (ADR-081 Guard 3). The L1 backend populates the canonical markers
/// while walking: macOS `attributes["AXSubrole"] == "AXSecureTextField"` ·
/// AT-SPI `attributes["sensitive"] == "true"` (`STATE_SENSITIVE`).
fn is_secure_field(node: &AxNode) -> bool {
    node.attributes
        .get("AXSubrole")
        .is_some_and(|s| s == "AXSecureTextField")
        || node
            .attributes
            .get("sensitive")
            .is_some_and(|s| s == "true")
}

/// Recursively strip `value` from every secure-text field in the tree —
/// **pure** · the MANDATORY Guard 3 (ADR-081). Applied to every node before
/// it leaves the crate (B.3) so passwords never reach a caller. The redacted
/// `value` is set to `None` (NOT a masked string · zero leak).
fn redact_secure_fields(node: AxNode) -> AxNode {
    let value = if is_secure_field(&node) {
        None
    } else {
        node.value
    };
    let children = node
        .children
        .into_iter()
        .map(redact_secure_fields)
        .collect();
    AxNode::new(
        node.id,
        node.role,
        node.label,
        value,
        node.bbox,
        children,
        node.attributes,
    )
}

/// True when a node satisfies the query selector — **pure**. `None` fields
/// don't filter; multiple `Some` fields compose via logical AND
/// (case-sensitive substring on `label`/`value`).
fn matches_query(node: &AxNode, query: &AxQuery) -> bool {
    if let Some(role) = query.role
        && node.role != role
    {
        return false;
    }
    if let Some(needle) = &query.label_contains
        && !node.label.as_ref().is_some_and(|l| l.contains(needle))
    {
        return false;
    }
    if let Some(needle) = &query.value_contains
        && !node.value.as_ref().is_some_and(|v| v.contains(needle))
    {
        return false;
    }
    true
}

/// Depth-bounded recursive collect of query matches over an in-memory tree —
/// **pure** (the `find` filter · runs over the redacted snapshot at B.3).
/// `depth` is the current node's depth (root = 0). `max_depth = Some(0)` =
/// root only · `Some(n)` includes depths `0..=n` · `None` = unbounded.
fn collect_matches(node: &AxNode, query: &AxQuery, depth: u16, out: &mut Vec<AxNode>) {
    if matches_query(node, query) {
        out.push(node.clone());
    }
    let recurse = match query.max_depth {
        None => true,
        Some(max) => depth < max,
    };
    if recurse {
        for child in &node.children {
            collect_matches(child, query, depth.saturating_add(1), out);
        }
    }
}

impl AccessibilityTree for AxBackend {
    async fn snapshot(&self) -> std::io::Result<AxNode> {
        // Guard 3 redaction is applied inside redacted_snapshot · no secure
        // value ever leaves the crate.
        self.redacted_snapshot().await
    }

    async fn find(&self, query: &AxQuery) -> std::io::Result<Vec<AxNode>> {
        // Redacted snapshot → depth-bounded filter. The redact step runs
        // BEFORE the filter so secure values never surface.
        let tree = self.redacted_snapshot().await?;
        let mut out = Vec::new();
        collect_matches(&tree, query, 0, &mut out);
        Ok(out)
    }

    async fn resolve_ref(&self, ref_id: &str) -> std::io::Result<Option<AxNode>> {
        // Resolve against the last cached (already-redacted) snapshot · `None`
        // when the ref is stale or no snapshot has run yet.
        let cache = self
            .last_snapshot
            .lock()
            .map_err(|_| A11yError::AttributeError {
                reason: "snapshot cache poisoned".to_string(),
            })?;
        Ok(cache.as_ref().and_then(|tree| find_by_id(tree, ref_id)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_kernel::io::a11y::AxRole;
    use nika_kernel::io::screen::Rect;
    use proptest::prelude::*;
    use std::collections::BTreeMap;

    /// Build a leaf node with the given role + optional value + attributes.
    fn leaf(id: &str, role: AxRole, value: Option<&str>, attrs: &[(&str, &str)]) -> AxNode {
        let mut map = BTreeMap::new();
        for (k, v) in attrs {
            map.insert((*k).to_string(), (*v).to_string());
        }
        AxNode::new(
            id.to_string(),
            role,
            None,
            value.map(str::to_string),
            Some(Rect::new(0, 0, 10, 10)),
            vec![],
            map,
        )
    }

    // --- is_secure_field + redact_secure_fields (Guard 3 · mutation-killing) ---

    #[test]
    fn is_secure_field_detects_macos_subrole_and_atspi_sensitive() {
        assert!(is_secure_field(&leaf(
            "@e1",
            AxRole::TextField,
            Some("hunter2"),
            &[("AXSubrole", "AXSecureTextField")]
        )));
        assert!(is_secure_field(&leaf(
            "@e2",
            AxRole::TextField,
            Some("hunter2"),
            &[("sensitive", "true")]
        )));
        assert!(!is_secure_field(&leaf(
            "@e3",
            AxRole::TextField,
            Some("plain"),
            &[]
        )));
        // A non-secure subrole must NOT trip the guard.
        assert!(!is_secure_field(&leaf(
            "@e4",
            AxRole::TextField,
            Some("plain"),
            &[("AXSubrole", "AXSearchField")]
        )));
    }

    #[test]
    fn redact_strips_secure_value_keeps_plain_value() {
        let secure = leaf(
            "@e1",
            AxRole::TextField,
            Some("hunter2"),
            &[("AXSubrole", "AXSecureTextField")],
        );
        let redacted = redact_secure_fields(secure);
        assert_eq!(redacted.value, None, "password value stripped to None");

        let plain = leaf("@e2", AxRole::TextField, Some("visible"), &[]);
        let kept = redact_secure_fields(plain);
        assert_eq!(kept.value.as_deref(), Some("visible"), "plain value kept");
    }

    #[test]
    fn redact_recurses_into_nested_secure_children() {
        let secure_child = leaf(
            "@e2",
            AxRole::TextField,
            Some("topsecret"),
            &[("sensitive", "true")],
        );
        let plain_child = leaf("@e3", AxRole::StaticText, Some("label"), &[]);
        let parent = AxNode::new(
            "@e1".to_string(),
            AxRole::Group,
            None,
            None,
            None,
            vec![secure_child, plain_child],
            BTreeMap::new(),
        );
        let redacted = redact_secure_fields(parent);
        assert_eq!(redacted.children[0].value, None, "nested secure redacted");
        assert_eq!(
            redacted.children[1].value.as_deref(),
            Some("label"),
            "sibling plain value preserved"
        );
    }

    // --- matches_query + collect_matches (pure find filter · mutation-killing) ---

    #[test]
    fn matches_query_empty_matches_everything() {
        let n = leaf("@e1", AxRole::Button, Some("x"), &[]);
        assert!(matches_query(&n, &AxQuery::default()));
    }

    #[test]
    fn matches_query_role_label_value_compose_and() {
        let mut n = leaf("@e1", AxRole::Button, Some("submit-now"), &[]);
        n.label = Some("Submit".to_string());
        // role match
        assert!(matches_query(
            &n,
            &AxQuery::new(Some(AxRole::Button), None, None, None)
        ));
        assert!(!matches_query(
            &n,
            &AxQuery::new(Some(AxRole::Link), None, None, None)
        ));
        // label substring
        assert!(matches_query(
            &n,
            &AxQuery::new(None, Some("ubmi".into()), None, None)
        ));
        assert!(!matches_query(
            &n,
            &AxQuery::new(None, Some("Cancel".into()), None, None)
        ));
        // value substring
        assert!(matches_query(
            &n,
            &AxQuery::new(None, None, Some("now".into()), None)
        ));
        // AND: role+label both must hold
        assert!(matches_query(
            &n,
            &AxQuery::new(Some(AxRole::Button), Some("Submit".into()), None, None)
        ));
        assert!(!matches_query(
            &n,
            &AxQuery::new(Some(AxRole::Link), Some("Submit".into()), None, None)
        ));
    }

    #[test]
    fn collect_matches_respects_max_depth() {
        // root(Group) > child(Button) > grandchild(Button)
        let grandchild = leaf("@e3", AxRole::Button, None, &[]);
        let child = AxNode::new(
            "@e2".into(),
            AxRole::Button,
            None,
            None,
            None,
            vec![grandchild],
            BTreeMap::new(),
        );
        let root = AxNode::new(
            "@e1".into(),
            AxRole::Group,
            None,
            None,
            None,
            vec![child],
            BTreeMap::new(),
        );
        let q = |max| AxQuery::new(Some(AxRole::Button), None, None, max);

        let mut root_only = vec![];
        collect_matches(&root, &q(Some(0)), 0, &mut root_only);
        assert_eq!(root_only.len(), 0, "depth 0 = root only · root is a Group");

        let mut depth1 = vec![];
        collect_matches(&root, &q(Some(1)), 0, &mut depth1);
        assert_eq!(depth1.len(), 1, "depth 1 reaches the child Button");

        let mut unbounded = vec![];
        collect_matches(&root, &q(None), 0, &mut unbounded);
        assert_eq!(unbounded.len(), 2, "unbounded reaches child + grandchild");
    }

    // --- ax_role_from_str (pure · mutation-killing) ---

    #[test]
    fn ax_role_from_str_maps_every_arm_and_unknown() {
        // Every mapped arm (pins each match arm against deletion).
        assert_eq!(ax_role_from_str("AXButton"), AxRole::Button);
        assert_eq!(ax_role_from_str("AXLink"), AxRole::Link);
        assert_eq!(ax_role_from_str("AXTextField"), AxRole::TextField);
        assert_eq!(ax_role_from_str("AXTextArea"), AxRole::TextField);
        assert_eq!(ax_role_from_str("AXImage"), AxRole::Image);
        assert_eq!(ax_role_from_str("AXGroup"), AxRole::Group);
        assert_eq!(ax_role_from_str("AXWindow"), AxRole::Window);
        assert_eq!(ax_role_from_str("AXMenu"), AxRole::Menu);
        assert_eq!(ax_role_from_str("AXStaticText"), AxRole::StaticText);
        assert_eq!(ax_role_from_str("AXList"), AxRole::List);
        assert_eq!(ax_role_from_str("AXRow"), AxRole::ListItem);
        assert_eq!(ax_role_from_str("AXCell"), AxRole::ListItem);
        assert_eq!(ax_role_from_str("AXMenuItem"), AxRole::ListItem);
        assert_eq!(ax_role_from_str("AXHeading"), AxRole::Heading);
        // Unmapped + empty → Unknown.
        assert_eq!(ax_role_from_str("AXSomethingNovel"), AxRole::Unknown);
        assert_eq!(ax_role_from_str(""), AxRole::Unknown);
    }

    // --- find_by_id (pure ref-cache resolution · mutation-killing) ---

    #[test]
    fn find_by_id_finds_root_nested_and_misses() {
        let grandchild = leaf("@e3", AxRole::Button, Some("ok"), &[]);
        let child = AxNode::new(
            "@e2".into(),
            AxRole::Group,
            None,
            None,
            None,
            vec![grandchild],
            BTreeMap::new(),
        );
        let root = AxNode::new(
            "@e1".into(),
            AxRole::Window,
            None,
            None,
            None,
            vec![child],
            BTreeMap::new(),
        );
        assert_eq!(find_by_id(&root, "@e1").map(|n| n.id), Some("@e1".into()));
        assert_eq!(find_by_id(&root, "@e3").map(|n| n.id), Some("@e3".into()));
        assert_eq!(find_by_id(&root, "@e99"), None);
    }

    // --- assemble_node (pure node assembly · mutation-killing) ---

    #[test]
    fn assemble_node_maps_role_filters_empty_and_records_subrole() {
        // Non-empty title + non-empty secure subrole.
        let n = assemble_node(
            "@e1".into(),
            "AXTextField",
            Some("Password".into()),
            Some("hunter2".into()),
            Some("AXSecureTextField".into()),
            vec![],
        );
        assert_eq!(n.role, AxRole::TextField);
        assert_eq!(n.label.as_deref(), Some("Password"));
        assert_eq!(n.value.as_deref(), Some("hunter2"));
        assert_eq!(
            n.attributes.get("AXSubrole").map(String::as_str),
            Some("AXSecureTextField")
        );
        assert!(is_secure_field(&n), "assembled secure field trips Guard 3");

        // Empty title + empty subrole are dropped (NOT recorded).
        let m = assemble_node(
            "@e2".into(),
            "AXUnknownThing",
            Some(String::new()),
            None,
            Some(String::new()),
            vec![],
        );
        assert_eq!(m.role, AxRole::Unknown);
        assert_eq!(m.label, None, "empty title ⇒ no label");
        assert!(
            !m.attributes.contains_key("AXSubrole"),
            "empty subrole ⇒ no attribute"
        );
        // None subrole ⇒ no attribute.
        let p = assemble_node("@e3".into(), "AXGroup", None, None, None, vec![]);
        assert!(!p.attributes.contains_key("AXSubrole"));
    }

    // --- resolve_ref on a fresh backend (empty cache · headless · NIKA-N/A) ---

    #[tokio::test]
    async fn resolve_ref_empty_cache_none_then_seeded_hit_and_miss() {
        let backend = AxBackend::new().expect("new");
        // Empty cache ⇒ Ok(None).
        assert_eq!(
            backend.resolve_ref("@e1").await.expect("empty cache ok"),
            None,
            "no snapshot cached yet ⇒ Ok(None)"
        );
        // Seed a redacted tree, then resolve hits + misses against the cache.
        let child = leaf("@e2", AxRole::Button, Some("ok"), &[]);
        let root = AxNode::new(
            "@e1".into(),
            AxRole::Window,
            None,
            None,
            None,
            vec![child],
            BTreeMap::new(),
        );
        backend.seed_cache_for_test(root);
        assert_eq!(
            backend.resolve_ref("@e2").await.expect("hit").map(|n| n.id),
            Some("@e2".into()),
            "seeded ref resolves to the cached node"
        );
        assert_eq!(
            backend.resolve_ref("@e404").await.expect("miss"),
            None,
            "unknown ref ⇒ Ok(None)"
        );
    }

    // --- real-walk smoke (needs AX permission + a focused window) ---

    #[tokio::test]
    #[ignore = "needs macOS Accessibility grant + a focused window · run: cargo test -p nika-a11y -- --ignored"]
    async fn snapshot_smoke_real_focused_window() {
        let backend = AxBackend::new().expect("new");
        let tree = backend.snapshot().await.expect("focused-window snapshot");
        // The root window walk yields at least itself; secure fields (if any)
        // are already redacted by the time the tree returns.
        assert!(!tree.id.is_empty());
    }

    proptest! {
        /// Guard 3 invariant · after redaction NO secure node retains a value
        /// (pins the recursive transform against headless mutation).
        #[test]
        fn redact_clears_every_secure_value(
            n_children in 0usize..8,
            secure_mask in 0u32..256,
        ) {
            let children: Vec<AxNode> = (0..n_children)
                .map(|i| {
                    let secure = (secure_mask >> (i % 8)) & 1 == 1;
                    let attrs: &[(&str, &str)] =
                        if secure { &[("sensitive", "true")] } else { &[] };
                    leaf(&format!("@e{i}"), AxRole::TextField, Some("secret"), attrs)
                })
                .collect();
            let root = AxNode::new(
                "@root".into(), AxRole::Group, None, None, None, children, BTreeMap::new(),
            );
            let redacted = redact_secure_fields(root);
            for child in &redacted.children {
                if is_secure_field(child) {
                    prop_assert_eq!(&child.value, &None);
                }
            }
        }
    }
}
