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

use nika_kernel::io::a11y::{AccessibilityTree, AxNode, AxQuery};

use crate::error::A11yError;

/// Accessibility backend (macOS `AXUIElement` via the safe `accessibility`
/// crate at B.3). The B.2 skeleton holds no state; B.3 adds the per-session
/// `@e<N>` ref cache + the focused-app target.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct AxBackend {}

impl AxBackend {
    /// Construct an accessibility backend bound to the focused application.
    ///
    /// B.2 skeleton · always succeeds. B.3 adds the macOS Accessibility-trust
    /// check (`AXIsProcessTrusted`) → [`A11yError::PermissionDenied`].
    ///
    /// # Errors
    /// B.3 · [`A11yError::PermissionDenied`] when the process is not trusted.
    pub fn new() -> Result<Self, A11yError> {
        Ok(Self::default())
    }

    /// Walk the focused application's accessibility tree into an [`AxNode`].
    ///
    /// B.2 placeholder · returns [`A11yError::BackendNotWired`]. B.3 wires the
    /// macOS `AXUIElement` `TreeWalker` (role string → [`AxNode`] · subrole →
    /// secure flag · frame → `Rect`) inside `spawn_blocking` (the `!Send`
    /// handle stays worker-local · kernel CANCEL SAFETY contract).
    #[expect(
        clippy::unused_self,
        reason = "B.3 reads &self (focused-app target + @e<N> ref cache) for the real AXUIElement walk"
    )]
    fn walk_tree(&self) -> Result<AxNode, A11yError> {
        Err(A11yError::BackendNotWired)
    }
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
        // walk_tree is the B.2 placeholder (returns BackendNotWired) · the
        // MANDATORY Guard 3 redaction wraps it so no secure value ever leaves.
        // B.3 makes walk_tree the real AXUIElement walk inside spawn_blocking.
        let tree = self.walk_tree()?;
        Ok(redact_secure_fields(tree))
    }

    async fn find(&self, query: &AxQuery) -> std::io::Result<Vec<AxNode>> {
        // B.3 = redacted snapshot → depth-bounded collect_matches. The redact
        // step runs BEFORE the filter so secure values never surface.
        let tree = redact_secure_fields(self.walk_tree()?);
        let mut out = Vec::new();
        collect_matches(&tree, query, 0, &mut out);
        Ok(out)
    }

    async fn resolve_ref(&self, _ref_id: &str) -> std::io::Result<Option<AxNode>> {
        // B.2 placeholder · B.3 looks up the @e<N> ref cache → redact → return.
        let _ = self.walk_tree()?;
        Ok(None)
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

    // --- skeleton trait methods (B.2 placeholder · NIKA-1200) ---

    #[tokio::test]
    async fn snapshot_hits_placeholder() {
        let backend = AxBackend::new().expect("new");
        let io = backend
            .snapshot()
            .await
            .expect_err("B.2 placeholder denies");
        let ae = io
            .into_inner()
            .expect("boxed")
            .downcast::<A11yError>()
            .expect("A11yError");
        assert_eq!(ae.code(), "NIKA-1200");
    }

    #[tokio::test]
    async fn find_and_resolve_ref_hit_placeholder() {
        let backend = AxBackend::new().expect("new");
        let f = backend
            .find(&AxQuery::default())
            .await
            .expect_err("placeholder");
        let r = backend.resolve_ref("@e1").await.expect_err("placeholder");
        for io in [f, r] {
            let ae = io
                .into_inner()
                .expect("boxed")
                .downcast::<A11yError>()
                .expect("A11yError");
            assert_eq!(ae.code(), "NIKA-1200");
        }
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
