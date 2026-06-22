// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Accessibility backend — implements the L0.5 `AccessibilityTree` trait,
//! **cross-platform** per ADR-083 (macOS + Linux prio · Windows + all).
//!
//! Architecture · a backend-agnostic pure core (DTO assembly · the Guard 3
//! redaction · `find`/query · `@e<N>` ref-cache · role + secure-marker logic)
//! + a per-OS raw-tree walk dispatched by [`build_raw_tree`]:
//! - **macOS** (verified) · sync `accessibility` `AXUIElement` walk in
//!   `tokio::task::spawn_blocking` (the `!Send` handle stays worker-local).
//! - **Linux** (⚠️ CI-pending) · async `atspi` AT-SPI2 D-Bus walk · NOT
//!   compiled on the darwin dev host · primary-source-grounded · see the
//!   Linux backend banner below.
//! - **other** · `A11yError::BackendUnavailable`.
//!
//! **Mandatory Guard 3 (ADR-081 · secure-field redaction) is a PURE recursive
//! tree-transform** ([`redact_secure_fields`]) that strips `value` from any
//! secure-text node ([`is_secure_field`]) so passwords NEVER leave the crate —
//! applied to every `snapshot`/`find`/`resolve_ref` result on every OS. Each
//! backend sets the canonical secure marker (macOS `AXSubrole ==
//! AXSecureTextField` · Linux `Role::PasswordText` → `secure = true`). The OS
//! walks that build the raw tree are the only Rule-2-exempt (FFI) residue.

use std::sync::Mutex;

use nika_kernel::io::a11y::{AccessibilityTreeDyn, AxNode, AxQuery, AxRole};

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

    /// Walk the focused window via the per-OS backend ([`build_raw_tree`]),
    /// apply the MANDATORY Guard 3 redaction, cache the result for
    /// `resolve_ref`, and return it.
    ///
    /// The OS walk is backend-specific (macOS · sync `AXUIElement` in
    /// `spawn_blocking` · Linux · async AT-SPI2 D-Bus) but the redaction +
    /// cache are backend-agnostic — Guard 3 runs on every result regardless of
    /// OS, so no secure value ever leaves the crate on any platform.
    async fn redacted_snapshot(&self) -> Result<AxNode, A11yError> {
        let raw = build_raw_tree().await?;
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
#[cfg(target_os = "macos")]
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
#[cfg(target_os = "macos")]
fn assemble_node(
    id: String,
    role_str: &str,
    title: Option<String>,
    value: Option<String>,
    subrole: Option<String>,
    subrole_read_failed: bool,
    children: Vec<AxNode>,
) -> AxNode {
    use std::collections::BTreeMap;

    let role = ax_role_from_str(role_str);
    let label = title.filter(|s| !s.is_empty());
    let mut attributes = BTreeMap::new();
    if let Some(sub) = subrole.filter(|s| !s.is_empty()) {
        attributes.insert("AXSubrole".to_string(), sub);
    } else if subrole_read_failed && value.as_ref().is_some_and(|v| !v.is_empty()) {
        // Guard 3 fail-CLOSED (ADR-081): the secure-marker (AXSubrole) read ERRORED
        // on a node that carries content (e.g. focus churn / app teardown mid-walk).
        // We cannot PROVE this is not a secure field, so mark it secure — redaction
        // then drops the value rather than risk leaking a password whose subrole
        // read happened to fail. A successful read of a non-secure subrole does NOT
        // trip this (we proved it safe); an empty-value node is not over-redacted.
        attributes.insert("secure".to_string(), "true".to_string());
    }
    AxNode::new(id, role, label, value, None, children, attributes)
}

/// Map a `spawn_blocking` join failure to a payload-free reason. NEVER
/// `e.to_string()`: tokio's `JoinError` Display embeds the panic PAYLOAD, which
/// could carry a secure-field value the AX walk read (Guard 3) if a backend
/// panicked with it. Distinguish the cause; carry no payload.
#[cfg(target_os = "macos")]
fn join_panic_reason(e: &tokio::task::JoinError) -> String {
    if e.is_cancelled() {
        "a11y task cancelled"
    } else {
        "a11y task panicked (payload suppressed)"
    }
    .to_owned()
}

/// Walk the focused window's accessibility tree into an [`AxNode`] — **macOS**.
///
/// `AXUIElement::system_wide().focused_window()` rooted recursive walk
/// (role/label/value/subrole/children). The `value` of secure fields is
/// stripped downstream by [`redact_secure_fields`] (Guard 3). Sync · driven
/// from [`build_raw_tree`] inside `spawn_blocking`.
#[cfg(target_os = "macos")]
fn walk_focused_tree() -> Result<AxNode, A11yError> {
    use accessibility::{AXUIElement, AXUIElementAttributes};

    let root = AXUIElement::system_wide()
        .focused_window()
        .map_err(|_| A11yError::NoFocusedApplication)?;
    let mut counter: u32 = 0;
    Ok(build_node(&root, &mut counter, 0))
}

/// Hard recursion cap for the OS walk · the focused app's tree is untrusted
/// input, so a pathologically deep (or cyclic) tree MUST NOT overflow the
/// stack. Generous — native AX/AT-SPI trees rarely exceed a few dozen levels;
/// children below the cap are truncated (the node itself is kept). Shared by
/// the macOS `AXUIElement` walk + the Linux AT-SPI walk.
#[cfg(any(target_os = "macos", target_os = "linux"))]
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
    // Keep the subrole read RESULT, not just `.ok()` — assemble_node fails closed
    // (Guard 3) when the secure-marker read errors on a node that has a value.
    let subrole_result = elem.subrole();
    let subrole_read_failed = subrole_result.is_err();
    let subrole = subrole_result.ok().map(|s| s.to_string());

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

    assemble_node(
        id,
        &role_str,
        title,
        value,
        subrole,
        subrole_read_failed,
        children,
    )
}

/// Build the raw (un-redacted) accessibility tree via the per-OS backend.
///
/// Backend dispatch (ADR-083 cross-platform doctrine · macOS + Linux prio) ·
/// - **macOS** · the sync `AXUIElement` walk in `spawn_blocking` (the `!Send`
///   handle stays worker-local · kernel CANCEL SAFETY · a dropped future
///   abandons the read, the blocking task finishes on the pool + is discarded).
/// - **Linux** · the async AT-SPI2 D-Bus walk (`walk_focused_tree_async`,
///   `#[cfg(target_os = "linux")]`) — no `spawn_blocking` (natively async).
/// - **other** · [`A11yError::BackendUnavailable`] (NIKA-1205).
///
/// Guard 3 redaction is applied by the caller ([`AxBackend::redacted_snapshot`])
/// on the result, so it is backend-agnostic.
#[cfg(target_os = "macos")]
async fn build_raw_tree() -> Result<AxNode, A11yError> {
    let raw = tokio::task::spawn_blocking(walk_focused_tree)
        .await
        .map_err(|e| A11yError::TaskJoinFailed {
            reason: join_panic_reason(&e),
        })??;
    Ok(raw)
}

#[cfg(target_os = "linux")]
async fn build_raw_tree() -> Result<AxNode, A11yError> {
    let raw = walk_focused_tree_async().await?;
    Ok(raw)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
async fn build_raw_tree() -> Result<AxNode, A11yError> {
    Err(A11yError::BackendUnavailable)
}

// ─────────────────────────────────────────────────────────────────────────
// Linux AT-SPI2 backend (ADR-083 · M2.3-bis) · ⚠️ CI-PENDING.
//
// This `#[cfg(target_os = "linux")]` module is NOT compiled or tested on the
// darwin dev host (aarch64-apple-darwin only). It is primary-source-grounded
// against the `atspi` 0.29 API (docs.rs · `AccessibilityConnection` ·
// `root_accessible_on_registry` · `get_children` · `get_role` → `Role` enum ·
// `into_accessible_proxy`) BUT MUST be compiled + its Guard 3 path tested on a
// real Linux / AT-SPI host before the Linux backend is trusted. Known
// CI-verify points are flagged inline (`CI-VERIFY`).
//
// SECURITY (Guard 3) · the secure-field signal on AT-SPI is the
// **`Role::PasswordText`** role — NOT `State::Sensitive` (which means
// "enabled / interactive", the opposite of secret · ADR-083). A
// `PasswordText` node is marked `attributes["secure"] = "true"`, which the
// pure [`is_secure_field`] / [`redact_secure_fields`] then redact. If the
// `Role::PasswordText` variant name is wrong, Guard 3 silently fails →
// password leak · this is the #1 CI-VERIFY item.
// ─────────────────────────────────────────────────────────────────────────

/// Map an `atspi::Role` to the canonical [`AxRole`] — **Linux** · pure.
/// Unmapped roles fall back to [`AxRole::Unknown`]. The `_` arm is mandatory
/// (`atspi::Role` is `#[non_exhaustive]`).
///
/// CI-VERIFY · the `atspi::Role` variant names below are from the AT-SPI2
/// `ROLE_*` standard (`PascalCase`) · confirm each against `atspi` 0.29 on the
/// Linux build (a wrong name is a compile error here, caught by Linux CI).
#[cfg(target_os = "linux")]
fn atspi_role_to_ax(role: atspi::Role) -> AxRole {
    use atspi::Role;
    match role {
        Role::Button | Role::ToggleButton => AxRole::Button,
        Role::Link => AxRole::Link,
        // PasswordText also maps to TextField for the role; the secure marker
        // is set separately at walk time (see build_node_atspi).
        Role::Entry | Role::Text | Role::PasswordText => AxRole::TextField,
        Role::Image => AxRole::Image,
        Role::Panel | Role::Filler => AxRole::Group,
        Role::Frame | Role::Window | Role::Dialog => AxRole::Window,
        Role::Menu => AxRole::Menu,
        Role::Label | Role::Static => AxRole::StaticText,
        Role::List => AxRole::List,
        Role::ListItem | Role::TableRow | Role::TableCell => AxRole::ListItem,
        Role::Heading => AxRole::Heading,
        _ => AxRole::Unknown,
    }
}

/// Walk the active desktop's AT-SPI2 tree into an [`AxNode`] — **Linux** ·
/// async D-Bus. Connects to the session a11y bus, roots at the registry, and
/// recursively maps each accessible. ⚠️ CI-PENDING (see module banner).
#[cfg(target_os = "linux")]
async fn walk_focused_tree_async() -> Result<AxNode, A11yError> {
    use atspi::AccessibilityConnection;
    use atspi::connection::set_session_accessibility;

    let atspi_conn =
        AccessibilityConnection::new()
            .await
            .map_err(|e| A11yError::TreeWalkFailed {
                reason: format!("a11y bus connect: {e}"),
            })?;
    // CI-VERIFY · ensures the session AT-SPI bus is enabled (no-op if already).
    set_session_accessibility(true)
        .await
        .map_err(|e| A11yError::TreeWalkFailed {
            reason: format!("enable session a11y: {e}"),
        })?;
    let conn = atspi_conn.connection();
    let root = atspi_conn
        .root_accessible_on_registry()
        .await
        .map_err(|_| A11yError::NoFocusedApplication)?;
    let mut counter: u32 = 0;
    Ok(build_node_atspi(root, conn, &mut counter, 0).await)
}

/// Recursively map one AT-SPI `AccessibleProxy` to an [`AxNode`] — **Linux** ·
/// `Box::pin` async recursion (depth-bounded by [`MAX_WALK_DEPTH`]). `value` is
/// `None` at v1 (AT-SPI `Text`-interface reads are a later refinement · secure
/// fields would be redacted regardless). ⚠️ CI-PENDING · the `AccessibleProxy`
/// lifetime + `&mut counter` reborrow across `.await` are the CI-VERIFY items.
#[cfg(target_os = "linux")]
fn build_node_atspi<'a>(
    proxy: atspi::proxy::accessible::AccessibleProxy<'a>,
    // atspi 0.29 only re-exports zbus behind its `zbus` feature, so `atspi::zbus`
    // is absent by default — depend on `zbus` directly (matches atspi's `zbus ^5.5`).
    conn: &'a zbus::Connection,
    counter: &'a mut u32,
    depth: u16,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = AxNode> + Send + 'a>> {
    use atspi::proxy::accessible::ObjectRefExt;
    use std::collections::BTreeMap;

    Box::pin(async move {
        *counter += 1;
        let id = format!("@e{counter}");

        // One role read · derive both the AxRole and the secure marker from it.
        // Guard 3 fail-CLOSED (ADR-081): on AT-SPI the `PasswordText` ROLE is the
        // sole secrecy signal, so a FAILED role read cannot prove the node is not a
        // password field — treat read-failure as secure. (value is None at v1, so
        // this only sets the marker today, but it ships the correct posture the
        // instant Text-interface value reads land.)
        let role_result = proxy.get_role().await;
        let role_read_failed = role_result.is_err();
        let raw_role = role_result.ok();
        let secure = matches!(raw_role, Some(atspi::Role::PasswordText)) || role_read_failed;
        let role = raw_role.map_or(AxRole::Unknown, atspi_role_to_ax);
        let label = proxy.name().await.ok().filter(|s| !s.is_empty());

        let mut attributes = BTreeMap::new();
        if secure {
            // Guard 3 marker · is_secure_field reads attributes["secure"].
            attributes.insert("secure".to_string(), "true".to_string());
        }

        let children = if depth >= MAX_WALK_DEPTH {
            Vec::new()
        } else {
            match proxy.get_children().await {
                Ok(refs) => {
                    let mut out = Vec::new();
                    for child_ref in refs {
                        if child_ref.is_null() {
                            continue;
                        }
                        if let Ok(child_proxy) = child_ref.into_accessible_proxy(conn).await {
                            out.push(build_node_atspi(child_proxy, conn, counter, depth + 1).await);
                        }
                    }
                    out
                }
                Err(_) => Vec::new(),
            }
        };

        // value None at v1 (Text-interface read deferred · secure ⇒ redacted).
        AxNode::new(id, role, label, None, None, children, attributes)
    })
}

/// True when a node is a secure-text field whose `value` MUST be redacted —
/// **pure** (ADR-081 Guard 3). The L1 backend populates ONE canonical marker
/// while walking, per OS:
/// - macOS · `attributes["AXSubrole"] == "AXSecureTextField"` (the secure
///   subrole reported by `AXTextField` password inputs).
/// - AT-SPI (Linux) · `attributes["secure"] == "true"`, set by the atspi
///   backend when the element's **role is `Role::PasswordText`**.
///
/// ⚠️ Correctness note (ADR-083 · 2026-05-26) · AT-SPI `State::Sensitive` is
/// **NOT** a secrecy signal — it means "enabled / interactive / not greyed-out"
/// (true for almost every usable field). The earlier `attributes["sensitive"]
/// == "true"` marker was a misreading that would BOTH over-redact ordinary
/// fields AND miss real password inputs. The canonical AT-SPI password signal
/// is the **`PasswordText` role**, mapped to the `secure` marker at walk time.
fn is_secure_field(node: &AxNode) -> bool {
    node.attributes
        .get("AXSubrole")
        .is_some_and(|s| s == "AXSecureTextField")
        || node.attributes.get("secure").is_some_and(|s| s == "true")
}

/// Recursively strip `value` from every secure-text field in the tree —
/// **pure** · the MANDATORY Guard 3 (ADR-081). Applied to every node before
/// it leaves the crate (B.3) so passwords never reach a caller. A redacted
/// `value` is set to `None` (NOT a masked string · zero leak).
fn redact_secure_fields(node: AxNode) -> AxNode {
    if is_secure_field(&node) {
        // A secure field scrubs its ENTIRE subtree, not just its own node:
        // `value` is the typed secret; `label` is cleared because some toolkits
        // mirror the content into the accessible name/title; and a DESCENDANT
        // (e.g. a child AXStaticText the widget exposes the text through) could
        // mirror it too. The guard must be complete by construction, never
        // reliant on a backend not doing that. (A node nested under a secure
        // field carries no caller-useful non-secret data — scrubbing is safe.)
        return scrub_subtree(node);
    }
    let children = node
        .children
        .into_iter()
        .map(redact_secure_fields)
        .collect();
    AxNode::new(
        node.id,
        node.role,
        node.label,
        node.value,
        node.bbox,
        children,
        node.attributes,
    )
}

/// Unconditionally clear `value` + `label` on a node and EVERY descendant —
/// the complete redaction applied to a secure field's whole subtree (pure).
fn scrub_subtree(node: AxNode) -> AxNode {
    let children = node.children.into_iter().map(scrub_subtree).collect();
    AxNode::new(
        node.id,
        node.role,
        None,
        None,
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

// The SEND variant is the impl target — the kernel's one-way blanket impl
// derives the local `AccessibilityTree` from it (never the reverse). Bodies
// are Send: the AXUIElement walk runs in `spawn_blocking`.
impl AccessibilityTreeDyn for AxBackend {
    async fn snapshot(&self) -> Result<AxNode, A11yError> {
        // Guard 3 redaction is applied inside redacted_snapshot · no secure
        // value ever leaves the crate.
        self.redacted_snapshot().await
    }

    async fn find(&self, query: &AxQuery) -> Result<Vec<AxNode>, A11yError> {
        // Redacted snapshot → depth-bounded filter. The redact step runs
        // BEFORE the filter so secure values never surface.
        let tree = self.redacted_snapshot().await?;
        let mut out = Vec::new();
        collect_matches(&tree, query, 0, &mut out);
        Ok(out)
    }

    async fn resolve_ref(&self, ref_id: &str) -> Result<Option<AxNode>, A11yError> {
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
    fn is_secure_field_detects_macos_subrole_and_atspi_secure_marker() {
        assert!(is_secure_field(&leaf(
            "@e1",
            AxRole::TextField,
            Some("hunter2"),
            &[("AXSubrole", "AXSecureTextField")]
        )));
        // AT-SPI backend sets `secure=true` when role is `PasswordText`.
        assert!(is_secure_field(&leaf(
            "@e2",
            AxRole::TextField,
            Some("hunter2"),
            &[("secure", "true")]
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
        // `sensitive=true` (AT-SPI "interactive / not greyed-out") must NOT
        // trip the guard — it is NOT a secrecy signal (ADR-083 correctness).
        assert!(!is_secure_field(&leaf(
            "@e5",
            AxRole::TextField,
            Some("plain-but-interactive"),
            &[("sensitive", "true")]
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
    fn redact_clears_label_too_for_secure_node() {
        // Defense-in-depth (Guard 3): a secure node's `label` is cleared as well as
        // its `value`, in case a backend mirrors the secret into the accessible
        // name. A plain node keeps both.
        let mut attrs = BTreeMap::new();
        attrs.insert("secure".to_string(), "true".to_string());
        let secure = AxNode::new(
            "@e1".to_string(),
            AxRole::TextField,
            Some("hunter2-as-name".to_string()), // label mirrors the secret
            Some("hunter2".to_string()),
            None,
            vec![],
            attrs,
        );
        let redacted = redact_secure_fields(secure);
        assert_eq!(redacted.value, None, "secure value stripped");
        assert_eq!(
            redacted.label, None,
            "secure label stripped (defense-in-depth)"
        );

        let plain = leaf("@e2", AxRole::StaticText, Some("v"), &[]);
        let plain = AxNode::new(
            plain.id,
            plain.role,
            Some("Visible Label".to_string()),
            plain.value,
            plain.bbox,
            plain.children,
            plain.attributes,
        );
        let kept = redact_secure_fields(plain);
        assert_eq!(
            kept.label.as_deref(),
            Some("Visible Label"),
            "plain label kept"
        );
    }

    #[test]
    fn redact_recurses_into_nested_secure_children() {
        let secure_child = leaf(
            "@e2",
            AxRole::TextField,
            Some("topsecret"),
            &[("secure", "true")],
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

    #[test]
    fn redact_scrubs_the_secure_fields_entire_subtree() {
        // The descendant-mirror leak (hardened 2026-06-11): some widgets expose
        // the typed text through a CHILD node (e.g. an AXStaticText under the
        // secure field). The old code cleared only the secure node itself —
        // the child's value walked out unredacted. Guard 3 must scrub the
        // WHOLE subtree rooted at a secure field, depth included.
        let mirror_child = leaf("@e3", AxRole::StaticText, Some("hunter2"), &[]);
        let mut deep_mirror = leaf("@e4", AxRole::StaticText, Some("hunter2"), &[]);
        deep_mirror.label = Some("hunter2".to_string());
        let mid = AxNode::new(
            "@e2b".to_string(),
            AxRole::Group,
            None,
            None,
            None,
            vec![deep_mirror],
            BTreeMap::new(),
        );
        let mut secure = leaf(
            "@e2",
            AxRole::TextField,
            Some("hunter2"),
            &[("AXSubrole", "AXSecureTextField")],
        );
        secure.children = vec![mirror_child, mid];
        // A plain sibling OUTSIDE the secure subtree must keep its value.
        let plain_sibling = leaf("@e5", AxRole::StaticText, Some("public"), &[]);
        let root = AxNode::new(
            "@e1".to_string(),
            AxRole::Group,
            None,
            None,
            None,
            vec![secure, plain_sibling],
            BTreeMap::new(),
        );
        let redacted = redact_secure_fields(root);
        let sec = &redacted.children[0];
        assert_eq!(sec.value, None, "secure node scrubbed");
        assert_eq!(sec.children[0].value, None, "direct child mirror scrubbed");
        assert_eq!(
            sec.children[1].children[0].value, None,
            "deep descendant mirror scrubbed"
        );
        assert_eq!(
            sec.children[1].children[0].label, None,
            "deep descendant label scrubbed too"
        );
        assert_eq!(
            redacted.children[1].value.as_deref(),
            Some("public"),
            "plain sibling outside the secure subtree untouched"
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

    #[cfg(target_os = "macos")]
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

    #[cfg(target_os = "macos")]
    #[test]
    fn assemble_node_maps_role_filters_empty_and_records_subrole() {
        // Non-empty title + non-empty secure subrole (read succeeded).
        let n = assemble_node(
            "@e1".into(),
            "AXTextField",
            Some("Password".into()),
            Some("hunter2".into()),
            Some("AXSecureTextField".into()),
            false,
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

        // Empty title + empty subrole are dropped (NOT recorded · read succeeded).
        let m = assemble_node(
            "@e2".into(),
            "AXUnknownThing",
            Some(String::new()),
            None,
            Some(String::new()),
            false,
            vec![],
        );
        assert_eq!(m.role, AxRole::Unknown);
        assert_eq!(m.label, None, "empty title ⇒ no label");
        assert!(
            !m.attributes.contains_key("AXSubrole"),
            "empty subrole ⇒ no attribute"
        );
        // None subrole, read succeeded ⇒ no attribute (proven non-secure).
        let p = assemble_node("@e3".into(), "AXGroup", None, None, None, false, vec![]);
        assert!(!p.attributes.contains_key("AXSubrole"));
    }

    // --- assemble_node Guard 3 fail-closed (subrole read ERRORED) ---

    #[cfg(target_os = "macos")]
    #[test]
    fn assemble_node_fail_closed_marks_secure_when_subrole_read_failed_with_value() {
        // subrole read errored (None + read_failed) but the node HAS a value:
        // we cannot prove it is non-secure, so it must be marked secure.
        let n = assemble_node(
            "@e1".into(),
            "AXTextField",
            Some("Email".into()),
            Some("hunter2".into()),
            None, // subrole unreadable
            true, // ...because the read ERRORED
            vec![],
        );
        assert!(
            is_secure_field(&n),
            "fail-closed: unreadable subrole + value ⇒ secure"
        );
        // ...and redaction then drops the value (no leak).
        let redacted = redact_secure_fields(n);
        assert_eq!(redacted.value, None, "fail-closed value is redacted");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn assemble_node_no_overredact_when_read_failed_but_no_value() {
        // subrole read errored but the node has NO content: nothing to protect,
        // so do NOT over-redact (mark) it.
        let n = assemble_node(
            "@e1".into(),
            "AXGroup",
            Some("Container".into()),
            None, // no value
            None,
            true, // read errored
            vec![],
        );
        assert!(
            !is_secure_field(&n),
            "no value ⇒ nothing to protect ⇒ not over-redacted"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn assemble_node_proven_nonsecure_is_not_marked_even_with_value() {
        // subrole read SUCCEEDED and returned a non-secure subrole: we proved it
        // is safe, so a value must pass through (read_failed = false).
        let n = assemble_node(
            "@e1".into(),
            "AXTextField",
            None,
            Some("public-handle".into()),
            Some("AXSearchField".into()),
            false,
            vec![],
        );
        assert!(
            !is_secure_field(&n),
            "proven-non-secure subrole ⇒ value kept"
        );
        let kept = redact_secure_fields(n);
        assert_eq!(kept.value.as_deref(), Some("public-handle"));
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
        // SKIP-on-precondition, never false-FAIL: "no focused GUI window" (a
        // terminal-only / headless CI run) is an ENVIRONMENT precondition, not a
        // code assertion — the crate correctly returns NoFocusedApplication and
        // the test should pass-through, not fail.
        let result = backend.snapshot().await;
        if matches!(result, Err(A11yError::NoFocusedApplication)) {
            return; // no GUI window in this environment → skip clean
        }
        // Any OTHER error is a real bug, surfaced via the test-idiomatic
        // `expect` on the runtime Result. A real walk yields a non-empty root
        // id (secure fields, if any, are already redacted by the time it
        // returns).
        let tree = result.expect("focused-window snapshot failed (real bug)");
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
                        if secure { &[("secure", "true")] } else { &[] };
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
