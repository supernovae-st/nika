// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Accessibility tree traits — L0.5 sealed DTOs + async trait.
//!
//! Phase 2 M1 PR 3 (sprint plan `2026-05-14-nika-phase-2-m1-kernel-
//! modules-sprint-plan.md` §4 · LARGEST module · ~180 LOC). Reserved
//! error codes NIKA-1060..1079 per
//! `docs/architecture/forward-compat-invariants.md` Gate 12.
//!
//! ADR-006 monolithic-kernel-spirit (single trait + DTOs · no proc
//! macro · no async runtime dep). ADR-016 ISP discipline (1 trait =
//! 1 capability · AX is 1 concern · cross-platform tree query · L1
//! impls split per OS backend · macOS `AXUIElement` · Windows UIA ·
//! Linux AT-SPI). ADR-037 bottom-up layer (L0.5 traits-only · zero
//! tokio · zero accessibility-API crates · those land in L1 at Phase
//! 2 M2-M3).
//!
//! Cross-PR dep · `AxNode::bbox: Option<Rect>` reuses the screen-capture
//! `Rect` DTO shipped at PR 1 (M1.1 · canonical pixel-coordinate
//! rectangle). Single canonical geometry type across capture + OCR +
//! a11y · cohérent `no-legacy-no-back-compat.md` Class 1.
//!
//! `AxNode::attributes: BTreeMap<String, String>` per sprint plan §10
//! Risks · workspace `clippy.toml` `disallowed-types` bans `HashMap` ·
//! `BTreeMap` provides serde-determinism + ordered iteration · zero
//! extra dep (std).

use crate::io::screen::Rect;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Accessibility role · canonical taxonomy across macOS · Windows · Linux.
///
/// `#[non_exhaustive]` on the enum keeps the variant set extensible per
/// `no-legacy-no-back-compat.md` (NUKE-LEGACY · breaking changes ship
/// on MINOR until 1.0). 13 variants cover the high-frequency
/// AX roles · further variants land additively when an L1 impl surfaces
/// a platform-specific role that doesn't map cleanly into `Unknown`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum AxRole {
    /// Button · clickable action element.
    Button,
    /// Link · navigational element.
    Link,
    /// Text-input field · keyboard-typeable.
    TextField,
    /// Image · static visual element.
    Image,
    /// Group · container for related children.
    Group,
    /// Window · top-level OS window.
    Window,
    /// Menu · drop-down or popup menu.
    Menu,
    /// Static text · non-interactive label.
    StaticText,
    /// List · ordered or unordered collection.
    List,
    /// List item · child of a `List`.
    ListItem,
    /// Heading · structural section title.
    Heading,
    /// Form · input-collection container.
    Form,
    /// Unknown · L1 impl could not map the platform role into a canonical variant.
    Unknown,
}

/// Accessibility tree node · single element with children + attributes.
///
/// `id` is a stable per-session reference · canonical shape `@e1..@eN`
/// assigned by the L1 impl when walking the platform AX tree. Callers
/// resolve a node back from its ref via `AccessibilityTree::resolve_ref`.
///
/// `bbox: Option<Rect>` shares the canonical pixel-coordinate `Rect`
/// from `io::screen` (cross-PR dep · M1.1) · single geometry type
/// across capture + OCR + a11y per `no-legacy-no-back-compat.md` Class 1.
///
/// `attributes: BTreeMap<String, String>` carries platform-specific
/// metadata (e.g. macOS `AXRole` raw string · Windows `AutomationId`)
/// per sprint plan §10 Risks decision · `BTreeMap` enforces serde
/// determinism + ordered iteration · `HashMap` is workspace-banned by
/// `clippy.toml` `disallowed-types`.
///
/// `PartialEq` derives recursively over `Vec<AxNode>` children · the
/// canonical equality is structural (cohérent `Frame` `PartialEq` pattern
/// at `io::screen::Frame`). `Eq` is sound here · no `f32`/`f64` fields ·
/// `Rect` is integer-only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct AxNode {
    /// Stable per-session ref · canonical `@e<N>` shape.
    pub id: String,
    /// Canonical AX role.
    pub role: AxRole,
    /// Visible label (e.g. button text · ARIA label).
    pub label: Option<String>,
    /// Value content (e.g. text-field input · selected option).
    pub value: Option<String>,
    /// Bounding box in physical-pixel coordinates · `None` when off-screen
    /// or unrenderable.
    pub bbox: Option<Rect>,
    /// Child nodes in document/AX-tree order.
    pub children: Vec<AxNode>,
    /// Platform-specific attribute map · `BTreeMap` for serde determinism.
    pub attributes: BTreeMap<String, String>,
}

impl AxNode {
    /// Construct a new AX-tree node.
    ///
    /// Per Invariant #19 · every `#[non_exhaustive]` struct ships a
    /// `new()` constructor so downstream code never field-literals.
    #[must_use]
    pub fn new(
        id: String,
        role: AxRole,
        label: Option<String>,
        value: Option<String>,
        bbox: Option<Rect>,
        children: Vec<AxNode>,
        attributes: BTreeMap<String, String>,
    ) -> Self {
        Self {
            id,
            role,
            label,
            value,
            bbox,
            children,
            attributes,
        }
    }
}

/// Query selector for `AccessibilityTree::find` · structural filter.
///
/// All fields are `Option` · `None` means « don't filter on this axis » ·
/// the L1 impl walks the cached tree and returns matching nodes. Multiple
/// `Some` fields compose via logical AND.
///
/// `max_depth: Option<u16>` caps recursion · `Some(0)` means « root only » ·
/// `None` means « unbounded » (caller responsibility to bound).
///
/// `#[non_exhaustive]` preserves forward-compat · future query axes (e.g.
/// `role_in: Vec<AxRole>` · `attribute_match: BTreeMap<String, String>`)
/// land additively on MINOR.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub struct AxQuery {
    /// Filter by exact role match.
    pub role: Option<AxRole>,
    /// Filter by substring match on `label` (case-sensitive · L1 impl decides).
    pub label_contains: Option<String>,
    /// Filter by substring match on `value` (case-sensitive · L1 impl decides).
    pub value_contains: Option<String>,
    /// Cap recursion depth · `None` = unbounded.
    pub max_depth: Option<u16>,
}

impl AxQuery {
    /// Construct a new AX query selector.
    #[must_use]
    pub fn new(
        role: Option<AxRole>,
        label_contains: Option<String>,
        value_contains: Option<String>,
        max_depth: Option<u16>,
    ) -> Self {
        Self {
            role,
            label_contains,
            value_contains,
            max_depth,
        }
    }
}

/// Accessibility backend errors — the typed boundary of the `AccessibilityTree`
/// trait (Pattern A · FCI-023bis). `#[non_exhaustive]`; `NikaErrorCode` impl +
/// reserved range (`Category::A11y` · NIKA-1201..1206) live in `crate::errors`.
/// Moved here from `nika-a11y` 2026-06-10 so the typed NIKA taxonomy survives the
/// trait boundary instead of being erased by `io::Error`.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum A11yError {
    /// The process lacks the OS accessibility grant (macOS Accessibility
    /// trust · `AXIsProcessTrusted` false) — the operator must grant it.
    #[error("NIKA-1201 · accessibility permission denied (grant Accessibility access)")]
    PermissionDenied,
    /// No focused/active application to snapshot.
    #[error("NIKA-1202 · no focused application")]
    NoFocusedApplication,
    /// Reading an `AXUIElement` attribute failed.
    #[error("NIKA-1203 · accessibility attribute error: {reason}")]
    AttributeError {
        /// Backend-reported reason.
        reason: String,
    },
    /// Walking the accessibility tree failed mid-traversal.
    #[error("NIKA-1204 · accessibility tree walk failed: {reason}")]
    TreeWalkFailed {
        /// Backend-reported reason.
        reason: String,
    },
    /// No accessibility backend is compiled for this platform (non-macOS
    /// until Linux AT-SPI / Windows UIA backends land · §4 macOS-first).
    #[error("NIKA-1205 · no accessibility backend on this platform")]
    BackendUnavailable,
    /// A `spawn_blocking` walk task panicked or was cancelled.
    #[error("NIKA-1206 · accessibility task join failed: {reason}")]
    TaskJoinFailed {
        /// Join failure detail.
        reason: String,
    },
}

/// Accessibility-tree capability · async trait over the active window.
///
/// CANCEL SAFETY: every method is cancel-safe at the syscall boundary ·
/// AX operations are read-only platform queries and produce no side
/// effects. L1 impls SHOULD wrap the OS call in a `spawn_blocking` +
/// cancel token shim because macOS `AXUIElement` · Windows UIA · Linux
/// AT-SPI APIs are synchronous. Partial tree data MUST NOT leak to a
/// caller on cancel (L1 impl invariant · enforced by integration tests
/// at Phase 3 M3).
///
/// `#[trait_variant::make(AccessibilityTreeDyn: Send)]` generates a
/// `Send` companion trait `AccessibilityTreeDyn` for generic constraints
/// (cohérent `io::screen::ScreenCaptureDyn` + `io::ocr::OcrEngineDyn`
/// pattern · ADR-006 + `io::fs::FsRead` canonical shape). Downstream
/// code uses `T: AccessibilityTreeDyn` for monomorphized hot paths. Per
/// `trait_variant` 0.1 semantics · the companion uses `impl Future +
/// Send` returns which are NOT dyn-compatible · this is intentional
/// (kernel traits stay zero-cost · L1 impls wrap via `Arc<T>` not
/// `Arc<dyn _>`).
#[trait_variant::make(AccessibilityTreeDyn: Send)]
pub trait AccessibilityTree: Send + Sync {
    /// Snapshot the full AX tree rooted at the active window.
    ///
    /// CANCEL SAFETY: cancel-safe (read-only platform query · no side
    /// effects · L1 impls wrap sync AX APIs in `spawn_blocking`).
    async fn snapshot(&self) -> Result<AxNode, A11yError>;

    /// Query the tree by selector · returns flat list of matching nodes.
    ///
    /// CANCEL SAFETY: cancel-safe · same contract as `snapshot`.
    /// `query.max_depth = None` means unbounded recursion · caller
    /// responsibility to bound for large trees (browser tabs · IDE
    /// workspaces).
    async fn find(&self, query: &AxQuery) -> Result<Vec<AxNode>, A11yError>;

    /// Resolve a stable `@e<N>` ref back to its node.
    ///
    /// Returns `None` when the ref is stale (window closed · element
    /// destroyed) · `Some` when the node is still live. L1 impls maintain
    /// a per-session ref → node cache.
    ///
    /// CANCEL SAFETY: cancel-safe · single-lookup operation.
    async fn resolve_ref(&self, ref_id: &str) -> Result<Option<AxNode>, A11yError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `AxNode` DTO round-trips through serde JSON · proves the canonical
    /// shape (id + role + label + value + bbox + children + attributes)
    /// serializes cleanly · cross-PR dep with `io::screen::Rect`.
    #[test]
    fn ax_node_serde_roundtrip() {
        let node = AxNode::new(
            "@e1".to_string(),
            AxRole::Button,
            Some("Submit".to_string()),
            None,
            Some(Rect::new(100, 200, 80, 32)),
            vec![],
            BTreeMap::new(),
        );
        let json = serde_json::to_string(&node).expect("serialize ax node");
        let back: AxNode = serde_json::from_str(&json).expect("deserialize ax node");
        assert_eq!(back, node);
    }

    /// `AxRole` enum exhibits the canonical variant subset · verifies
    /// 3 high-signal variants (Button · Link · Unknown) are present and
    /// distinct under equality. Cohérent `#[non_exhaustive]` discipline ·
    /// downstream code never field-matches without `_` wildcard arm.
    #[test]
    fn ax_role_enum_exhaustivity_subset() {
        assert_ne!(AxRole::Button, AxRole::Link);
        assert_ne!(AxRole::Button, AxRole::Unknown);
        assert_ne!(AxRole::Link, AxRole::Unknown);
        assert_eq!(AxRole::Button, AxRole::Button);
    }

    /// `AxQuery::default()` produces an all-`None` selector · the
    /// canonical « match everything » query · proves `Default` derive
    /// + structural equality on the empty selector.
    #[test]
    fn ax_query_default_empty() {
        let q = AxQuery::default();
        assert_eq!(q.role, None);
        assert_eq!(q.label_contains, None);
        assert_eq!(q.value_contains, None);
        assert_eq!(q.max_depth, None);
    }

    /// `AxNode` with nested children round-trips through serde JSON ·
    /// proves recursive serde derive walks the `Vec<AxNode>` field
    /// correctly (no infinite-loop · no missing fields).
    #[test]
    fn ax_node_with_children_serde() {
        let mut attrs = BTreeMap::new();
        attrs.insert("AXRole".to_string(), "AXButton".to_string());
        let child = AxNode::new(
            "@e2".to_string(),
            AxRole::StaticText,
            Some("OK".to_string()),
            None,
            Some(Rect::new(110, 210, 60, 20)),
            vec![],
            attrs.clone(),
        );
        let parent = AxNode::new(
            "@e1".to_string(),
            AxRole::Group,
            None,
            None,
            Some(Rect::new(100, 200, 80, 32)),
            vec![child.clone()],
            attrs,
        );
        let json = serde_json::to_string(&parent).expect("serialize parent");
        let back: AxNode = serde_json::from_str(&json).expect("deserialize parent");
        assert_eq!(back, parent);
        assert_eq!(back.children.len(), 1);
        assert_eq!(back.children[0], child);
    }

    /// `AccessibilityTree` + `AccessibilityTreeDyn` (the `Send` companion
    /// generated by `trait_variant::make`) are well-formed as generic
    /// bounds.
    ///
    /// Per DEV-2 M1.1 lesson · `trait_variant` 0.1 returns
    /// `impl Future + Send` which is NOT dyn-compatible.
    /// Use a generic-bound test (`T: AccessibilityTreeDyn`) NOT a
    /// trait-object test. Pure compile-time check · no runtime path.
    /// If this compiles · the trait bounds are well-formed.
    #[test]
    fn accessibility_tree_generic_bound_compile_check() {
        fn _accepts_accessibility_tree<T: AccessibilityTree>() {}
        fn _accepts_accessibility_tree_dyn<T: AccessibilityTreeDyn>() {}
    }

    /// All public DTOs are Send + Sync · cohérent CANCEL SAFETY · types
    /// cross task boundaries in async L1 impls (`spawn_blocking` wraps).
    #[test]
    fn dtos_are_send_sync() {
        fn _assert<T: Send + Sync>() {}
        _assert::<AxRole>();
        _assert::<AxNode>();
        _assert::<AxQuery>();
    }
}
