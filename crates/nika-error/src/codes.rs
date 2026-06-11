// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! NIKA-XXX code registry — dual wire format + typed struct.
//!
//! Each error code is a [`NikaCode`] with a numeric id, [`Category`],
//! [`Severity`], and a kebab-case slug for documentation URLs.
//!
//! Wire format: `"NIKA-001"` (Display impl, stable across versions).

use std::fmt;

/// Structured error code combining numeric id, category, severity, and slug.
///
/// # Display
///
/// Formats as `"NIKA-{num:03}"` for wire stability:
/// ```
/// use nika_error::codes::NIKA_001;
/// assert_eq!(format!("{NIKA_001}"), "NIKA-001");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NikaCode {
    /// Numeric identifier (1..=9999). Unique across the entire registry.
    pub num: u16,
    /// Functional category for grouping and routing.
    pub category: Category,
    /// Severity level.
    pub severity: Severity,
    /// Kebab-case slug for documentation URLs (e.g. `"validation-failed"`).
    pub slug: &'static str,
}

impl fmt::Display for NikaCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NIKA-{:03}", self.num)
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for NikaCode {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for NikaCode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = <&str>::deserialize(deserializer)?;
        lookup(s).ok_or_else(|| serde::de::Error::custom(format!("unknown code: {s}")))
    }
}

/// Functional category for error grouping and routing.
///
/// Numeric ranges are a convention (not enforced at type level). The
/// authoritative allocation lives in `nika-kernel::errors` (the hub):
/// Core 001-049, Shell 050-099, `FileIo` 100-139, Http 140-189,
/// Auth 190-229, Mcp 230-279, Schema 280-329, Provider 330-379,
/// Shield 380-429 (reserved · crate not yet admitted), Verb 430-479,
/// Runtime 480-529, Memory 600-649, `WasmPlugin` 700-749,
/// Sandbox 750-799, Observability 800-819, Screen 1000-1099,
/// Ocr 1100-1199, A11y 1200-1299 (M2 computer-use L1 ranges per ADR-081).
/// `Binding` is a reserved category variant with no allocated range yet
/// (its original 330-379 slot was reassigned to Provider on 2026-05-11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[non_exhaustive]
pub enum Category {
    Core,
    Shell,
    FileIo,
    Http,
    Auth,
    Mcp,
    Schema,
    Binding,
    Provider,
    Verb,
    Runtime,
    /// Memory subsystem (600-649).
    Memory,
    /// WASM plugin host execution (700-749).
    WasmPlugin,
    /// Capability-based sandbox (750-799).
    Sandbox,
    /// Observability/telemetry sinks (800-819).
    Observability,
    /// Screen capture (1000-1099 · M2.1 nika-screen · ADR-081).
    Screen,
    /// OCR text extraction (1100-1199 · M2.2 nika-ocr · ADR-081).
    Ocr,
    /// Accessibility-tree query (1200-1299 · M2.3 nika-a11y · ADR-081).
    A11y,
    /// Synthetic input dispatch (1300-1399 · M2.4 nika-input · ADR-081).
    Input,
    /// Browser automation (1400-1499 · nika-browser · ADR-081).
    Browser,
    /// Vision inference (NIKA-1500..1599 · `VisionModel` · `nika-vision-local` M2.6).
    Vision,
    /// Audio inference (NIKA-1600..1699 · stt/tts/vad · `ai::audio` seam R6).
    Audio,
}

/// Severity level for an error code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[non_exhaustive]
pub enum Severity {
    Error,
    Warning,
}

// ─── Phase 1 codes (L0 only) ──────────────────────────────────────────────

/// NIKA-001: Input validation failed.
pub const NIKA_001: NikaCode = NikaCode {
    num: 1,
    category: Category::Core,
    severity: Severity::Error,
    slug: "validation-failed",
};

/// NIKA-002: Referenced item not found.
pub const NIKA_002: NikaCode = NikaCode {
    num: 2,
    category: Category::Core,
    severity: Severity::Error,
    slug: "not-found",
};

/// NIKA-003: Feature not supported.
pub const NIKA_003: NikaCode = NikaCode {
    num: 3,
    category: Category::Core,
    severity: Severity::Error,
    slug: "unsupported",
};

// ─── Catalog validation codes (010-019) ───────────────────────────────────
//
// Originally assigned as NIKA-230..235 (Session 4a A1), but 230-233 collide
// with the MCP/tool codes in nika-kernel (tool-not-found, tool-timeout,
// tool-exec-failed, tool-not-available). Renumbered to Core 010-015 to
// stay within Core 001-049 range and avoid the MCP 230-279 range.

/// NIKA-010: Catalog TOML parse failure.
pub const NIKA_010: NikaCode = NikaCode {
    num: 10,
    category: Category::Core,
    severity: Severity::Error,
    slug: "catalog-toml-parse",
};

/// NIKA-011: Catalog schema version mismatch.
pub const NIKA_011: NikaCode = NikaCode {
    num: 11,
    category: Category::Core,
    severity: Severity::Error,
    slug: "catalog-schema-mismatch",
};

/// NIKA-012: Conflicting capability rules.
pub const NIKA_012: NikaCode = NikaCode {
    num: 12,
    category: Category::Core,
    severity: Severity::Error,
    slug: "capability-rule-conflict",
};

/// NIKA-013: Pricing axis value out of range.
pub const NIKA_013: NikaCode = NikaCode {
    num: 13,
    category: Category::Core,
    severity: Severity::Error,
    slug: "pricing-axis-out-of-range",
};

/// NIKA-014: Context window invariant violated (`max_output` > `context_window`).
pub const NIKA_014: NikaCode = NikaCode {
    num: 14,
    category: Category::Core,
    severity: Severity::Error,
    slug: "context-window-invariant",
};

/// NIKA-015: Unrecognised JSON mode value.
pub const NIKA_015: NikaCode = NikaCode {
    num: 15,
    category: Category::Core,
    severity: Severity::Error,
    slug: "json-mode-unknown",
};

// ─── Kernel subsystem codes (050+) ──────────────────────────────────────
//
// These range placeholders register kernel error categories so that
// `lookup("NIKA-050")` works even before individual codes ship.
// Specific codes within each range are added when owning crates land.

// ─── Range-based help (no concrete placeholders) ────────────────────────
//
// Per Audit-1 P0-1 (2026-04-16): the placeholder constants NIKA_050,
// NIKA_100, NIKA_140, NIKA_230, NIKA_380 used to live here but COLLIDED
// with concrete code definitions in `nika-kernel/src/errors.rs` (same
// numbers, different slugs). `lookup()` returned the placeholder and
// silently mismatched any caller expecting the real slug.
//
// The placeholders are removed. Help text for these ranges is still
// provided by `code_help()` via numeric range matching, so a wire code
// like "NIKA-053" still resolves to actionable shell-error guidance via
// `code_help()` even though `lookup("NIKA-053")` returns None (the
// concrete code lives in nika-kernel and is not visible to nika-error
// without a workspace-level registry crate).
//
// The limitation: cross-crate `lookup()` is not yet implemented. Codes
// owned by nika-kernel (or other downstream crates) need a workspace-
// level registry to be resolvable from nika-error. Tracked as future
// work; not in scope for Wave 1.3.

// ─── MCP/tools concrete codes (230-279 · ai-sibling owned) ──────────────
//
// 230-233 stay kernel-runtime-local (the Audit-1 P0-1 carve-out). 234 is
// registry-owned per the ai-sibling convention (vision/audio/memory
// precedent) so `lookup("NIKA-234")` resolves for `nika explain`.

/// NIKA-234: Tool-definition source unavailable (catalog · MCP `tools/list`).
pub const NIKA_234: NikaCode = NikaCode {
    num: 234,
    category: Category::Mcp,
    severity: Severity::Error,
    slug: "tool-defs-unavailable",
};

// ─── Reserved subsystem codes (600+) ─────────────────────────────────────

/// NIKA-600: Memory subsystem error (range placeholder 600-649).
pub const NIKA_600: NikaCode = NikaCode {
    num: 600,
    category: Category::Memory,
    severity: Severity::Error,
    slug: "memory",
};

// ─── Memory subsystem concrete codes (601-604) ──────────────────────────
//
// Wired Diamond W2.1 (2026-05-12) per Phase 1 entry plan §2 v1.1 amendment.
// Sub-allocation within reserved 600-649 range:
//   - 601..=604  active (this commit · MemoryError variant mapping)
//   - 605..=607  reserved (tenant-quota / consolidation-budget / prune-tombstone)
//   - 610..=619  reserved · nika-hnsw L1 satellite
//   - 620..=629  reserved · nika-bm25 (W3 admission · ADR-038)
//   - 630..=634  reserved · nika-rrf (W4 admission)
//   - 635..=639  reserved · nika-fsrs
//   - 640..=644  reserved · nika-rdfs-reasoner / graph-algos / temporal / autodesc
//   - 645..=649  reserved · nika-memory L2 orchestrator
// See dx/.claude/plans/active/sprint/2026-05-11-diamond-memory-phase-1-entry-plan.md §2.

/// NIKA-601: Memory store unavailable (provider misconfigured · runtime down).
pub const NIKA_601: NikaCode = NikaCode {
    num: 601,
    category: Category::Memory,
    severity: Severity::Error,
    slug: "memory-unavailable",
};

/// NIKA-602: Memory fact not found (deterministic miss · not transient).
pub const NIKA_602: NikaCode = NikaCode {
    num: 602,
    category: Category::Memory,
    severity: Severity::Error,
    slug: "memory-not-found",
};

/// NIKA-603: Embedding provider failed (transient · retry-eligible).
pub const NIKA_603: NikaCode = NikaCode {
    num: 603,
    category: Category::Memory,
    severity: Severity::Error,
    slug: "embedding-failed",
};

/// NIKA-604: Memory storage layer error (`Oxigraph` / `RocksDB` / IO · transient).
pub const NIKA_604: NikaCode = NikaCode {
    num: 604,
    category: Category::Memory,
    severity: Severity::Error,
    slug: "memory-storage",
};

/// NIKA-700: WASM plugin error (range placeholder 700-749).
pub const NIKA_700: NikaCode = NikaCode {
    num: 700,
    category: Category::WasmPlugin,
    severity: Severity::Error,
    slug: "wasm-plugin",
};

/// NIKA-750: Sandbox error (range placeholder 750-799).
pub const NIKA_750: NikaCode = NikaCode {
    num: 750,
    category: Category::Sandbox,
    severity: Severity::Error,
    slug: "sandbox",
};

/// NIKA-800: Observability error (range placeholder 800-819).
pub const NIKA_800: NikaCode = NikaCode {
    num: 800,
    category: Category::Observability,
    severity: Severity::Error,
    slug: "observability",
};

// ─── Verb codes · 430-479 (s9 nika-verb-infer claims 430-439 · consumed
// by the verb-crate NikaErrorCode impls · same registry-owned pattern as
// the M2 computer-use ranges below) ─────────────────────────────────────

/// NIKA-430: Provider call failed during `infer` verb execution.
pub const NIKA_430: NikaCode = NikaCode {
    num: 430,
    category: Category::Verb,
    severity: Severity::Error,
    slug: "infer-provider-call",
};
/// NIKA-431: Structured output failed schema validation after retries.
pub const NIKA_431: NikaCode = NikaCode {
    num: 431,
    category: Category::Verb,
    severity: Severity::Error,
    slug: "infer-schema-validation",
};
/// NIKA-432: Invalid `infer` parameter (empty prompt · temperature out of 0-2).
pub const NIKA_432: NikaCode = NikaCode {
    num: 432,
    category: Category::Verb,
    severity: Severity::Error,
    slug: "infer-invalid-param",
};
/// NIKA-433: Model string failed to resolve to a provider profile.
pub const NIKA_433: NikaCode = NikaCode {
    num: 433,
    category: Category::Verb,
    severity: Severity::Error,
    slug: "infer-model-resolution",
};
/// NIKA-440: Command exited non-zero in a default capture mode.
pub const NIKA_440: NikaCode = NikaCode {
    num: 440,
    category: Category::Verb,
    severity: Severity::Error,
    slug: "exec-non-zero-exit",
};
/// NIKA-441: Shell execution failed (spawn · blocklist · timeout).
pub const NIKA_441: NikaCode = NikaCode {
    num: 441,
    category: Category::Verb,
    severity: Severity::Error,
    slug: "exec-shell-failure",
};
/// NIKA-442: Invalid `exec` parameter (empty command).
pub const NIKA_442: NikaCode = NikaCode {
    num: 442,
    category: Category::Verb,
    severity: Severity::Error,
    slug: "exec-invalid-param",
};
/// NIKA-450: The `invoke` tool id did not resolve (bad namespace · `mcp:`
/// missing the slash · unknown builtin/server).
pub const NIKA_450: NikaCode = NikaCode {
    num: 450,
    category: Category::Verb,
    severity: Severity::Error,
    slug: "invoke-unresolvable-tool",
};
/// NIKA-451: The tool ran but reported an error (`is_error: true`).
pub const NIKA_451: NikaCode = NikaCode {
    num: 451,
    category: Category::Verb,
    severity: Severity::Error,
    slug: "invoke-tool-reported-error",
};
/// NIKA-452: Tool dispatch failed (timeout · execution · unavailable).
pub const NIKA_452: NikaCode = NikaCode {
    num: 452,
    category: Category::Verb,
    severity: Severity::Error,
    slug: "invoke-dispatch-failure",
};
/// NIKA-460: The agent loop hit `max_turns` without completing.
pub const NIKA_460: NikaCode = NikaCode {
    num: 460,
    category: Category::Verb,
    severity: Severity::Error,
    slug: "agent-max-turns",
};
/// NIKA-461: The agent loop exhausted `max_tokens_total`.
pub const NIKA_461: NikaCode = NikaCode {
    num: 461,
    category: Category::Verb,
    severity: Severity::Error,
    slug: "agent-max-tokens",
};
/// NIKA-462: The model requested a tool outside the agent whitelist.
pub const NIKA_462: NikaCode = NikaCode {
    num: 462,
    category: Category::Verb,
    severity: Severity::Error,
    slug: "agent-whitelist-violation",
};
/// NIKA-463: The provider call failed mid-loop.
pub const NIKA_463: NikaCode = NikaCode {
    num: 463,
    category: Category::Verb,
    severity: Severity::Error,
    slug: "agent-inference",
};
/// NIKA-464: The agent's final message failed `schema:` validation.
pub const NIKA_464: NikaCode = NikaCode {
    num: 464,
    category: Category::Verb,
    severity: Severity::Error,
    slug: "agent-schema-validation",
};
/// NIKA-465: An `agent` parameter is invalid (empty prompt · temp range).
pub const NIKA_465: NikaCode = NikaCode {
    num: 465,
    category: Category::Verb,
    severity: Severity::Error,
    slug: "agent-invalid-param",
};
/// NIKA-466: The tool-definition source failed (catalog · MCP `tools/list`).
pub const NIKA_466: NikaCode = NikaCode {
    num: 466,
    category: Category::Verb,
    severity: Severity::Error,
    slug: "agent-tool-defs-unavailable",
};

/// NIKA-999: Internal error (catch-all).
pub const NIKA_999: NikaCode = NikaCode {
    num: 999,
    category: Category::Core,
    severity: Severity::Error,
    slug: "internal",
};

// ─── M2 computer-use L1 codes (ADR-081 ranges · consumed by the
// nika-screen / nika-ocr / nika-a11y NikaErrorCode impls) ──────────────

/// NIKA-1000: Screen-capture backend not wired (skeleton placeholder).
pub const NIKA_1000: NikaCode = NikaCode {
    num: 1000,
    category: Category::Screen,
    severity: Severity::Error,
    slug: "screen-backend-not-wired",
};

/// NIKA-1001: Requested display id not found.
pub const NIKA_1001: NikaCode = NikaCode {
    num: 1001,
    category: Category::Screen,
    severity: Severity::Error,
    slug: "screen-display-not-found",
};

/// NIKA-1002: No displays connected / enumerable.
pub const NIKA_1002: NikaCode = NikaCode {
    num: 1002,
    category: Category::Screen,
    severity: Severity::Error,
    slug: "screen-no-displays",
};

/// NIKA-1003: OS capture call failed.
pub const NIKA_1003: NikaCode = NikaCode {
    num: 1003,
    category: Category::Screen,
    severity: Severity::Error,
    slug: "screen-capture-failed",
};

/// NIKA-1004: Requested sub-region outside display bounds.
pub const NIKA_1004: NikaCode = NikaCode {
    num: 1004,
    category: Category::Screen,
    severity: Severity::Error,
    slug: "screen-region-out-of-bounds",
};

/// NIKA-1005: Captured frame had unexpected pixel format / size.
pub const NIKA_1005: NikaCode = NikaCode {
    num: 1005,
    category: Category::Screen,
    severity: Severity::Error,
    slug: "screen-invalid-frame-format",
};

/// NIKA-1006: Capture attempted without user consent (guard 7).
pub const NIKA_1006: NikaCode = NikaCode {
    num: 1006,
    category: Category::Screen,
    severity: Severity::Error,
    slug: "screen-consent-denied",
};

/// NIKA-1007: Consent revoked mid-capture — stream torn down.
pub const NIKA_1007: NikaCode = NikaCode {
    num: 1007,
    category: Category::Screen,
    severity: Severity::Error,
    slug: "screen-consent-revoked",
};

/// NIKA-1008: Capture-LED indicator could not be engaged (guard 6).
pub const NIKA_1008: NikaCode = NikaCode {
    num: 1008,
    category: Category::Screen,
    severity: Severity::Error,
    slug: "screen-indicator-unavailable",
};

/// NIKA-1009: Capture backend failed to initialize.
pub const NIKA_1009: NikaCode = NikaCode {
    num: 1009,
    category: Category::Screen,
    severity: Severity::Error,
    slug: "screen-backend-init",
};

/// NIKA-1101: OCR model file not found.
pub const NIKA_1101: NikaCode = NikaCode {
    num: 1101,
    category: Category::Ocr,
    severity: Severity::Error,
    slug: "ocr-model-not-found",
};

/// NIKA-1102: OCR model failed to load / parse.
pub const NIKA_1102: NikaCode = NikaCode {
    num: 1102,
    category: Category::Ocr,
    severity: Severity::Error,
    slug: "ocr-model-load-failed",
};

/// NIKA-1103: OCR engine failed to initialize.
pub const NIKA_1103: NikaCode = NikaCode {
    num: 1103,
    category: Category::Ocr,
    severity: Severity::Error,
    slug: "ocr-engine-init",
};

/// NIKA-1104: OCR region outside frame bounds.
pub const NIKA_1104: NikaCode = NikaCode {
    num: 1104,
    category: Category::Ocr,
    severity: Severity::Error,
    slug: "ocr-region-out-of-bounds",
};

/// NIKA-1105: Frame rejected by the OCR input stage.
pub const NIKA_1105: NikaCode = NikaCode {
    num: 1105,
    category: Category::Ocr,
    severity: Severity::Error,
    slug: "ocr-invalid-frame-format",
};

/// NIKA-1106: Frame-to-tensor preparation failed.
pub const NIKA_1106: NikaCode = NikaCode {
    num: 1106,
    category: Category::Ocr,
    severity: Severity::Error,
    slug: "ocr-prepare-input-failed",
};

/// NIKA-1107: Text detection pass failed.
pub const NIKA_1107: NikaCode = NikaCode {
    num: 1107,
    category: Category::Ocr,
    severity: Severity::Error,
    slug: "ocr-detection-failed",
};

/// NIKA-1108: Text recognition pass failed.
pub const NIKA_1108: NikaCode = NikaCode {
    num: 1108,
    category: Category::Ocr,
    severity: Severity::Error,
    slug: "ocr-recognition-failed",
};

/// NIKA-1109: `spawn_blocking` OCR task failed to join.
pub const NIKA_1109: NikaCode = NikaCode {
    num: 1109,
    category: Category::Ocr,
    severity: Severity::Error,
    slug: "ocr-task-join-failed",
};

/// NIKA-1201: OS accessibility permission denied.
pub const NIKA_1201: NikaCode = NikaCode {
    num: 1201,
    category: Category::A11y,
    severity: Severity::Error,
    slug: "a11y-permission-denied",
};

/// NIKA-1202: No focused application to query.
pub const NIKA_1202: NikaCode = NikaCode {
    num: 1202,
    category: Category::A11y,
    severity: Severity::Error,
    slug: "a11y-no-focused-application",
};

/// NIKA-1203: Accessibility attribute read failed.
pub const NIKA_1203: NikaCode = NikaCode {
    num: 1203,
    category: Category::A11y,
    severity: Severity::Error,
    slug: "a11y-attribute-error",
};

/// NIKA-1204: Accessibility tree walk failed.
pub const NIKA_1204: NikaCode = NikaCode {
    num: 1204,
    category: Category::A11y,
    severity: Severity::Error,
    slug: "a11y-tree-walk-failed",
};

/// NIKA-1205: Accessibility backend unavailable on this OS.
pub const NIKA_1205: NikaCode = NikaCode {
    num: 1205,
    category: Category::A11y,
    severity: Severity::Error,
    slug: "a11y-backend-unavailable",
};

/// NIKA-1206: `spawn_blocking` accessibility task failed to join.
pub const NIKA_1206: NikaCode = NikaCode {
    num: 1206,
    category: Category::A11y,
    severity: Severity::Error,
    slug: "a11y-task-join-failed",
};

// ─── Synthetic input · 1300-1399 (M2.4 nika-input · ADR-081) ───────────────
// NIKA-1300 reserved (skeleton placeholder slot · retired at admission).

/// NIKA-1301: Synthetic input attempted without OS consent grant.
pub const NIKA_1301: NikaCode = NikaCode {
    num: 1301,
    category: Category::Input,
    severity: Severity::Error,
    slug: "input-consent-denied",
};
/// NIKA-1302: The `ConsentProof` TTL expired before dispatch.
pub const NIKA_1302: NikaCode = NikaCode {
    num: 1302,
    category: Category::Input,
    severity: Severity::Error,
    slug: "input-consent-expired",
};
/// NIKA-1303: Posting the synthetic OS event failed.
pub const NIKA_1303: NikaCode = NikaCode {
    num: 1303,
    category: Category::Input,
    severity: Severity::Error,
    slug: "input-event-post-failed",
};
/// NIKA-1304: No synthetic-input backend on this platform.
pub const NIKA_1304: NikaCode = NikaCode {
    num: 1304,
    category: Category::Input,
    severity: Severity::Error,
    slug: "input-backend-unavailable",
};
/// NIKA-1305: `spawn_blocking` input-dispatch task failed to join.
pub const NIKA_1305: NikaCode = NikaCode {
    num: 1305,
    category: Category::Input,
    severity: Severity::Error,
    slug: "input-task-join-failed",
};

// ─── Browser automation · 1400-1499 (nika-browser · ADR-081) ───────────────
// NIKA-1400 reserved (skeleton placeholder slot · retired at admission).

/// NIKA-1401: Launching the browser session failed.
pub const NIKA_1401: NikaCode = NikaCode {
    num: 1401,
    category: Category::Browser,
    severity: Severity::Error,
    slug: "browser-launch-failed",
};
/// NIKA-1402: Navigating to a URL failed.
pub const NIKA_1402: NikaCode = NikaCode {
    num: 1402,
    category: Category::Browser,
    severity: Severity::Error,
    slug: "browser-navigation-failed",
};
/// NIKA-1403: The referenced browser session was not found / already closed.
pub const NIKA_1403: NikaCode = NikaCode {
    num: 1403,
    category: Category::Browser,
    severity: Severity::Error,
    slug: "browser-session-not-found",
};
/// NIKA-1404: A DOM selector did not resolve / interaction failed.
pub const NIKA_1404: NikaCode = NikaCode {
    num: 1404,
    category: Category::Browser,
    severity: Severity::Error,
    slug: "browser-selector-failed",
};
/// NIKA-1405: No browser-automation backend on this platform.
pub const NIKA_1405: NikaCode = NikaCode {
    num: 1405,
    category: Category::Browser,
    severity: Severity::Error,
    slug: "browser-backend-unavailable",
};
/// NIKA-1406: `spawn_blocking` browser task failed to join.
pub const NIKA_1406: NikaCode = NikaCode {
    num: 1406,
    category: Category::Browser,
    severity: Severity::Error,
    slug: "browser-task-join-failed",
};

// ─── Vision inference · 1500-1599 (VisionModel · nika-vision-local M2.6) ────

/// NIKA-1501: The requested vision model is unavailable on this host.
pub const NIKA_1501: NikaCode = NikaCode {
    num: 1501,
    category: Category::Vision,
    severity: Severity::Error,
    slug: "vision-model-unavailable",
};
/// NIKA-1502: The vision input failed validation (frame dimensions/buffer).
pub const NIKA_1502: NikaCode = NikaCode {
    num: 1502,
    category: Category::Vision,
    severity: Severity::Error,
    slug: "vision-input-invalid",
};
/// NIKA-1503: The vision inference run failed.
pub const NIKA_1503: NikaCode = NikaCode {
    num: 1503,
    category: Category::Vision,
    severity: Severity::Error,
    slug: "vision-inference-failed",
};
/// NIKA-1504: No vision backend on this platform.
pub const NIKA_1504: NikaCode = NikaCode {
    num: 1504,
    category: Category::Vision,
    severity: Severity::Error,
    slug: "vision-backend-unavailable",
};
/// NIKA-1505: `spawn_blocking` vision task failed to join.
pub const NIKA_1505: NikaCode = NikaCode {
    num: 1505,
    category: Category::Vision,
    severity: Severity::Error,
    slug: "vision-task-join-failed",
};

// ─── Audio inference · 1600-1699 (stt/tts/vad · ai::audio seam R6) ──────────

/// NIKA-1601: The requested audio model/voice is unavailable on this host.
pub const NIKA_1601: NikaCode = NikaCode {
    num: 1601,
    category: Category::Audio,
    severity: Severity::Error,
    slug: "audio-model-unavailable",
};
/// NIKA-1602: The audio input failed validation (rate/channels/length).
pub const NIKA_1602: NikaCode = NikaCode {
    num: 1602,
    category: Category::Audio,
    severity: Severity::Error,
    slug: "audio-input-invalid",
};
/// NIKA-1603: The audio inference/synthesis run failed.
pub const NIKA_1603: NikaCode = NikaCode {
    num: 1603,
    category: Category::Audio,
    severity: Severity::Error,
    slug: "audio-inference-failed",
};
/// NIKA-1604: No audio backend on this platform.
pub const NIKA_1604: NikaCode = NikaCode {
    num: 1604,
    category: Category::Audio,
    severity: Severity::Error,
    slug: "audio-backend-unavailable",
};
/// NIKA-1605: `spawn_blocking` audio task failed to join.
pub const NIKA_1605: NikaCode = NikaCode {
    num: 1605,
    category: Category::Audio,
    severity: Severity::Error,
    slug: "audio-task-join-failed",
};

/// All registered codes within nika-error's own ranges + the M2
/// computer-use L1 ranges (Screen/Ocr/A11y · ADR-081 · the impls live
/// in their L1 crates, the CONSTANTS are registry-owned here so
/// `lookup()` + `code_help()` resolve them).
///
/// **Scope**: other downstream-owned codes (nika-kernel siblings,
/// nika-runtime, etc.) are NOT enumerated here. `lookup()` therefore
/// returns None for codes outside this registry — see Audit-1 P0-1
/// (2026-04-16). Verb codes (430-479) joined the registry-owned set
/// with s9 `nika-verb-infer` (same pattern as the M2 ranges).
///
/// A workspace-level registry crate would unify all codes, but landing
/// it requires settling the cross-crate registry pattern (Phase D
/// candidate).
pub const ALL: &[NikaCode] = &[
    NIKA_001, NIKA_002, NIKA_003, NIKA_010, NIKA_011, NIKA_012, NIKA_013, NIKA_014, NIKA_015,
    NIKA_234, NIKA_430, NIKA_431, NIKA_432, NIKA_433, NIKA_440, NIKA_441, NIKA_442, NIKA_450,
    NIKA_451, NIKA_452, NIKA_460, NIKA_461, NIKA_462, NIKA_463, NIKA_464, NIKA_465, NIKA_466,
    NIKA_600, NIKA_601, NIKA_602, NIKA_603, NIKA_604, NIKA_700, NIKA_750, NIKA_800, NIKA_999,
    NIKA_1000, NIKA_1001, NIKA_1002, NIKA_1003, NIKA_1004, NIKA_1005, NIKA_1006, NIKA_1007,
    NIKA_1008, NIKA_1009, NIKA_1101, NIKA_1102, NIKA_1103, NIKA_1104, NIKA_1105, NIKA_1106,
    NIKA_1107, NIKA_1108, NIKA_1109, NIKA_1201, NIKA_1202, NIKA_1203, NIKA_1204, NIKA_1205,
    NIKA_1206, NIKA_1301, NIKA_1302, NIKA_1303, NIKA_1304, NIKA_1305, NIKA_1401, NIKA_1402,
    NIKA_1403, NIKA_1404, NIKA_1405, NIKA_1406, NIKA_1501, NIKA_1502, NIKA_1503, NIKA_1504,
    NIKA_1505, NIKA_1601, NIKA_1602, NIKA_1603, NIKA_1604, NIKA_1605,
];

/// Returns an actionable help message for a given code.
///
/// Every registered code has a help string. This is used by miette's
/// Help text for the Verb range (430-479 · infer 430-439 · exec 440-449 ·
/// invoke 450-459 · agent 460-469). Split out of `code_help` to keep it under the 100-line
/// function cap as the verb crates land.
fn verb_help(num: u16) -> &'static str {
    match num {
        430 => {
            "The provider call failed during `infer`. Check the provider error chained below (credentials, rate limits, connectivity)."
        }
        431 => {
            "The model output never satisfied the task `schema:` within the retry budget. Simplify the schema, raise max_tokens, or pick a schema-capable model."
        }
        432 => {
            "An `infer` parameter is invalid. Prompt must be non-empty; temperature must be within 0-2."
        }
        433 => {
            "The `model:` string did not resolve. Use `provider/model` with a provider from the canonical catalog and ensure its API key is configured."
        }
        440 => {
            "The command exited non-zero. Inspect stderr, or use capture: structured to branch on the exit code instead of failing."
        }
        441 => {
            "Shell execution failed before or during the run. Check the command exists, is not blocklisted, and completes within the timeout."
        }
        442 => "An `exec` parameter is invalid. Command must be a non-empty string.",
        450 => {
            "The `invoke` tool id did not resolve. Use `nika:<tool>` or `mcp:<server>/<tool>`; check the builtin name or MCP server registry."
        }
        451 => {
            "The tool ran but reported an error. Inspect the tool's output content for the failure detail."
        }
        452 => {
            "Tool dispatch failed (timeout, execution error, or the tool system is unavailable). Check the MCP server or builtin availability."
        }
        460 => {
            "The agent hit max_turns without completing. Raise max_turns, tighten the prompt toward the nika:done sentinel, or reduce tool round-trips."
        }
        461 => {
            "The agent exhausted max_tokens_total. Raise the budget or reduce turn/context size; the last assistant message rides error.details.partial_output."
        }
        462 => {
            "The model requested a tool outside the agent whitelist. Security boundaries are not model-negotiable: add the tool to `tools:` if intended."
        }
        463 => {
            "A provider call failed mid-loop. Check the chained provider error (credentials, rate limits, connectivity)."
        }
        464 => {
            "The agent's final message never satisfied the task `schema:`. Simplify the schema or instruct the model to answer via nika:done result."
        }
        465 => {
            "An `agent` parameter is invalid. Prompt must be non-empty; temperature must be within 0-2."
        }
        466 => {
            "The tool-definition source failed (builtin catalog or MCP tools/list). Check the MCP server availability for mcp:* whitelist entries."
        }
        _ => "Verb execution failed. Check the task definition against the spec for this verb.",
    }
}

/// `help()` diagnostic and by the display layer for user-facing output.
#[must_use]
pub fn code_help(code: NikaCode) -> &'static str {
    match code.num {
        1 => "Check your workflow YAML syntax and field values.",
        2 => "Referenced item not found in catalogs or task outputs.",
        3 => "Feature not supported in current configuration.",
        10 => "Catalog TOML is malformed. Check syntax near the reported line and column.",
        11 => {
            "Catalog schema version does not match the expected version. Update the `schema` field."
        }
        12 => {
            "Two capability rules conflict for the same scope. Check rule ordering in model-capabilities.toml."
        }
        13 => {
            "A pricing axis value is out of valid range. Ensure rates are non-negative and finite."
        }
        14 => "max_output_tokens exceeds context_window_tokens. Fix the model capability rule.",
        15 => "Unrecognised json_mode value. Valid values: none, object, schema.",
        50..=99 => {
            "Shell/process execution failed. Check command path, permissions, and timeout settings."
        }
        100..=139 => {
            "File I/O or blob operation failed. Check file paths, permissions, and storage availability."
        }
        140..=189 => {
            "HTTP request failed. Check endpoint URL, network connectivity, and SSRF allowlist."
        }
        234 => {
            "The tool-definition source could not enumerate (builtin catalog unloadable or MCP tools/list unreachable). Check the backing source."
        }
        230..=279 => {
            "MCP tool call failed. Check tool name, parameters, and MCP server availability."
        }
        280..=329 => {
            "Schema/workflow validation failed. Check the `.nika.yaml` envelope, task ids, verbs, and field values against the spec."
        }
        330..=379 => {
            "AI provider error. Check the model name, API credentials, rate limits, and provider connectivity."
        }
        380..=429 => {
            "Shield security policy blocked the operation. Check trust levels, capability grants, and injection/canary guards."
        }
        430..=479 => verb_help(code.num),
        601 => {
            "Memory store unavailable. Verify the configured backend (Oxigraph / RocksDB / runtime) is initialised and reachable."
        }
        602 => {
            "Memory fact not found. The id does not exist in the store; verify the id is correct and not yet evicted."
        }
        603 => {
            "Embedding provider failed. Transient — retry-eligible. Check provider connectivity and credentials."
        }
        604 => {
            "Memory storage layer error. Transient — IO / cache / RocksDB-level failure. Retry-eligible after a brief backoff."
        }
        600 | 605..=649 => {
            "Memory subsystem reported an error. Check store availability, embedding provider, and tenant quotas."
        }
        1000..=1099 => {
            "Screen capture failed. Check display connectivity, capture consent (ConsentState · guard 7), and the OS screen-recording permission."
        }
        1100..=1199 => {
            "OCR failed. Check the model files (with_models path), frame format, and region bounds."
        }
        1200..=1299 => {
            "Accessibility-tree query failed. Check the OS accessibility permission and that a focused application exists."
        }
        1300..=1399 => {
            "Synthetic input failed. Check input/accessibility consent (ConsentProof TTL) and the OS input-monitoring permission."
        }
        1400..=1499 => {
            "Browser automation failed. Check the browser session, the target URL/selector, and the automation backend."
        }
        1500..=1599 => {
            "Vision inference failed. Check the model is available, the frame is valid RGBA8, and a vision backend is installed."
        }
        1600..=1699 => {
            "Audio inference failed. Check the model/voice is available, the clip format (PCM s16le), and an audio backend is installed."
        }
        700..=749 => {
            "WASM plugin host reported an error. Check plugin manifest and capability grants."
        }
        750..=799 => "Sandbox denied or failed. Verify capability allowlist and platform support.",
        800..=819 => "Observability sink rejected the event. Check exporter configuration.",
        999 => "Internal error. Please report at github.com/supernovae-st/nika/issues",
        _ => "Unknown error code. Check documentation for details.",
    }
}

/// Look up a [`NikaCode`] by its wire string (e.g. `"NIKA-001"`).
///
/// Returns `None` if the string doesn't match any registered code.
#[must_use]
pub fn lookup(wire: &str) -> Option<NikaCode> {
    let num_str = wire.strip_prefix("NIKA-")?;
    let num: u16 = num_str.parse().ok()?;
    ALL.iter().copied().find(|c| c.num == num)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verb_codes_have_their_category_and_range() {
        // Verb 430-479 · s9 infer 430-439 · s10 exec 440-449 · s11 invoke 450-459.
        for c in [
            NIKA_430, NIKA_431, NIKA_432, NIKA_433, NIKA_440, NIKA_441, NIKA_442, NIKA_450,
            NIKA_451, NIKA_452,
        ] {
            assert_eq!(c.category, Category::Verb, "{c}");
            assert!((430..=459).contains(&c.num), "{c}");
            assert_eq!(lookup(&c.to_string()), Some(c), "{c} resolvable");
            assert!(!code_help(c).is_empty(), "{c} has help");
        }
    }

    #[test]
    fn computer_use_codes_have_their_categories() {
        // M2 computer-use L1 ranges (ADR-081) · Screen 1000-1099 ·
        // Ocr 1100-1199 · A11y 1200-1299.
        for c in [
            NIKA_1000, NIKA_1001, NIKA_1002, NIKA_1003, NIKA_1004, NIKA_1005, NIKA_1006, NIKA_1007,
            NIKA_1008, NIKA_1009,
        ] {
            assert_eq!(c.category, Category::Screen, "{c}");
            assert!((1000..=1099).contains(&c.num), "{c}");
        }
        for c in [
            NIKA_1101, NIKA_1102, NIKA_1103, NIKA_1104, NIKA_1105, NIKA_1106, NIKA_1107, NIKA_1108,
            NIKA_1109,
        ] {
            assert_eq!(c.category, Category::Ocr, "{c}");
            assert!((1100..=1199).contains(&c.num), "{c}");
        }
        for c in [
            NIKA_1201, NIKA_1202, NIKA_1203, NIKA_1204, NIKA_1205, NIKA_1206,
        ] {
            assert_eq!(c.category, Category::A11y, "{c}");
            assert!((1200..=1299).contains(&c.num), "{c}");
        }
    }

    #[test]
    fn computer_use_codes_lookup_and_help() {
        let c = lookup("NIKA-1003").expect("screen capture-failed registered");
        assert_eq!(c.slug, "screen-capture-failed");
        assert!(!code_help(c).is_empty());
        assert!(lookup("NIKA-1101").is_some(), "ocr registered");
        assert!(lookup("NIKA-1206").is_some(), "a11y registered");
        assert!(
            lookup("NIKA-1200").is_none(),
            "1200 reserved (closed skeleton slot)"
        );
    }

    #[test]
    fn display_format_three_digit_padding() {
        assert_eq!(format!("{NIKA_001}"), "NIKA-001");
        assert_eq!(format!("{NIKA_999}"), "NIKA-999");
    }

    #[test]
    fn display_format_no_extra_padding_above_999() {
        let big = NikaCode {
            num: 1234,
            category: Category::Core,
            severity: Severity::Error,
            slug: "test",
        };
        assert_eq!(format!("{big}"), "NIKA-1234");
    }

    #[test]
    fn nika_001_is_core_validation() {
        assert_eq!(NIKA_001.category, Category::Core);
        assert_eq!(NIKA_001.severity, Severity::Error);
        assert_eq!(NIKA_001.slug, "validation-failed");
    }

    #[test]
    fn all_codes_unique_nums() {
        for (i, a) in ALL.iter().enumerate() {
            for b in ALL.iter().skip(i + 1) {
                assert_ne!(a.num, b.num, "duplicate num: {a} and {b}");
            }
        }
    }

    #[test]
    fn all_codes_unique_slugs() {
        for (i, a) in ALL.iter().enumerate() {
            for b in ALL.iter().skip(i + 1) {
                assert_ne!(a.slug, b.slug, "duplicate slug: {} and {}", a.slug, b.slug);
            }
        }
    }

    #[test]
    fn code_help_returns_non_empty_for_registered() {
        for code in ALL {
            let help = code_help(*code);
            assert!(!help.is_empty(), "empty help for {code}");
            assert!(
                !help.contains("Unknown"),
                "registered code {code} got fallback help — add specific entry to code_help()"
            );
        }
    }

    #[test]
    fn code_help_specific_values() {
        assert!(code_help(NIKA_001).contains("YAML"));
        assert!(code_help(NIKA_002).contains("not found"));
        assert!(code_help(NIKA_003).contains("not supported"));
        assert!(code_help(NIKA_999).contains("Internal"));
    }

    #[test]
    fn code_help_unknown_returns_fallback() {
        let unknown = NikaCode {
            num: 8888,
            category: Category::Core,
            severity: Severity::Error,
            slug: "unknown",
        };
        assert!(code_help(unknown).contains("Unknown"));
    }

    #[test]
    fn code_help_covers_cross_crate_ranges() {
        // Schema (280-329), Provider (330-379) and the reserved Shield range
        // (380-429) carry help via NUMERIC-RANGE arms — `code_help` switches on
        // `code.num` only, so the `category` field below is immaterial (Shield
        // is not even a Category variant yet, its crate is unadmitted). These
        // codes live in sibling crates so they are NOT in `ALL`; the range arms
        // are the only coverage. Regression: Provider moved 380-429 → 330-379,
        // which left Schema + Provider falling through to "Unknown error code".
        let probe = |num: u16| NikaCode {
            num,
            category: Category::Core,
            severity: Severity::Error,
            slug: "probe",
        };
        let schema = code_help(probe(299));
        assert!(
            schema.contains("Schema") && !schema.contains("Unknown"),
            "{schema}"
        );
        let provider = code_help(probe(330));
        assert!(
            provider.contains("provider") && !provider.contains("Unknown"),
            "{provider}"
        );
        let shield = code_help(probe(380));
        assert!(
            shield.contains("Shield") && !shield.contains("Unknown"),
            "{shield}"
        );
    }

    #[test]
    fn lookup_valid_codes() {
        assert_eq!(lookup("NIKA-001"), Some(NIKA_001));
        assert_eq!(lookup("NIKA-999"), Some(NIKA_999));
    }

    #[test]
    fn lookup_invalid_returns_none() {
        assert_eq!(lookup("NIKA-555"), None);
        assert_eq!(lookup("INVALID"), None);
        assert_eq!(lookup(""), None);
    }

    #[test]
    fn lookup_roundtrip_via_display() {
        for code in ALL {
            let wire = format!("{code}");
            let found = lookup(&wire);
            assert_eq!(found, Some(*code), "roundtrip failed for {code}");
        }
    }

    #[cfg(feature = "serde")]
    mod serde_tests {
        use super::*;

        #[test]
        fn nika_code_serializes_as_wire_string() {
            let json = serde_json::to_string(&NIKA_001).expect("serialize");
            assert_eq!(json, "\"NIKA-001\"");
        }

        #[test]
        fn nika_code_deserializes_from_wire_string() {
            let code: NikaCode = serde_json::from_str("\"NIKA-001\"").expect("deserialize");
            assert_eq!(code, NIKA_001);
        }

        #[test]
        fn nika_code_serde_roundtrip() {
            for code in ALL {
                let json = serde_json::to_string(code).expect("serialize");
                let back: NikaCode = serde_json::from_str(&json).expect("deserialize");
                assert_eq!(back, *code);
            }
        }

        #[test]
        fn category_serializes_kebab_case() {
            let json = serde_json::to_string(&Category::FileIo).expect("serialize");
            assert_eq!(json, "\"file-io\"");
        }

        #[test]
        fn unknown_code_deser_fails() {
            let result: Result<NikaCode, _> = serde_json::from_str("\"NIKA-555\"");
            assert!(result.is_err());
        }
    }

    mod proptest_codes {
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn display_always_starts_with_nika(num in 1u16..=9999u16) {
                let code = super::NikaCode {
                    num,
                    category: super::Category::Core,
                    severity: super::Severity::Error,
                    slug: "test",
                };
                let display = format!("{code}");
                prop_assert!(display.starts_with("NIKA-"));
                prop_assert!(display.len() >= 8); // "NIKA-" + at least 3 digits
            }
        }

        #[test]
        fn all_registered_codes_have_unique_nums() {
            let nums: Vec<u16> = super::ALL.iter().map(|c| c.num).collect();
            let mut sorted = nums.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(nums.len(), sorted.len(), "duplicate nums in ALL registry");
        }

        // ─── Registry uniqueness + memory cross-mapping (Diamond W2.3) ───

        #[test]
        fn all_registered_codes_have_unique_slugs() {
            let slugs: Vec<&'static str> = super::ALL.iter().map(|c| c.slug).collect();
            let mut sorted = slugs.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(slugs.len(), sorted.len(), "duplicate slugs in ALL registry");
        }

        proptest! {
            // Every registered slug MUST be lowercase kebab-case (^[a-z][a-z0-9-]*$).
            #[test]
            fn all_slugs_are_kebab_case(idx in 0usize..super::ALL.len()) {
                let slug = super::ALL[idx].slug;
                prop_assert!(!slug.is_empty(), "slug must not be empty");
                let first = slug.chars().next().expect("non-empty checked above");
                prop_assert!(
                    first.is_ascii_lowercase(),
                    "slug must start with a-z, got {first:?}"
                );
                for c in slug.chars() {
                    prop_assert!(
                        c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-',
                        "slug contains invalid char {c:?}"
                    );
                }
            }
        }

        #[test]
        fn code_help_some_for_every_registered_code() {
            // Every code in ALL must return a non-empty, non-Unknown help string.
            for code in super::ALL {
                let help = super::code_help(*code);
                assert!(!help.is_empty(), "empty help for {code}");
                assert!(
                    !help.contains("Unknown"),
                    "registered code {code} got fallback Unknown help"
                );
            }
        }

        #[test]
        fn memory_codes_601_to_604_are_memory_category() {
            for code in [
                super::NIKA_601,
                super::NIKA_602,
                super::NIKA_603,
                super::NIKA_604,
            ] {
                assert_eq!(
                    code.category,
                    super::Category::Memory,
                    "{code} must be Category::Memory"
                );
                assert!(
                    code.num >= 601 && code.num <= 604,
                    "{code} num must be 601..=604 sub-allocation"
                );
            }
        }

        #[test]
        fn memory_codes_lookup_roundtrip() {
            for code in [
                super::NIKA_601,
                super::NIKA_602,
                super::NIKA_603,
                super::NIKA_604,
            ] {
                let wire = format!("{code}");
                let back = super::lookup(&wire);
                assert_eq!(back, Some(code), "memory code roundtrip failed for {code}");
            }
        }
    }
}
