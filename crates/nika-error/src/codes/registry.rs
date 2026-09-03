// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The code REGISTRY rows — every `NIKA-*` const plus the `ALL_CODES`
//! table, family by family.
//!
//! Split from `codes.rs` 2026-07-10 at 1489/1500 LOC (eleven lines from
//! the file cap, and every feature adds codes): the MACHINERY
//! (`NikaCode` · `Category` · `Severity` · the help fns · `lookup` · the
//! tests) stays in `codes.rs`; the DATA lives here. The module is
//! PRIVATE with a glob re-export, so every `codes::NIKA_*` path — and
//! the public API surface the baseline locks — is unchanged.

use super::{Category, NikaCode, Severity};

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

/// NIKA-016: A canonical schedule declaration was semantically refused.
pub const NIKA_016: NikaCode = NikaCode {
    num: 16,
    category: Category::Core,
    severity: Severity::Error,
    slug: "schedule-finding",
};

/// NIKA-017: The pure schedule planner could not produce an authoritative plan.
pub const NIKA_017: NikaCode = NikaCode {
    num: 17,
    category: Category::Core,
    severity: Severity::Error,
    slug: "schedule-plan-refused",
};

/// NIKA-018: Durable resident schedule state refused recovery or mutation.
pub const NIKA_018: NikaCode = NikaCode {
    num: 18,
    category: Category::Core,
    severity: Severity::Error,
    slug: "schedule-store",
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

// ─── Memory subsystem concrete codes (601-605) ──────────────────────────
//
// Wired Diamond W2.1 (2026-05-12) per Phase 1 entry plan §2 v1.1 amendment.
// Sub-allocation within reserved 600-649 range:
//   - 601..=604  active (this commit · MemoryError variant mapping)
//   - 605        active (F-P8 · nika-store signed-envelope StoreError)
//   - 606..=608  reserved (tenant-quota / consolidation-budget / prune-tombstone)
//   - 610..=619  reserved · nika-hnsw L1 satellite
//   - 620..=629  reserved · nika-bm25 (W3 admission · ADR-038)
//   - 630..=634  reserved · nika-rrf (W4 admission)
//   - 635..=639  reserved · nika-fsrs
//   - 640..=644  reserved · nika-rdfs-reasoner / graph-algos / temporal / autodesc
//   - 645..=649  reserved · nika-memory L2 orchestrator
// Ranges reserved by the memory-phase entry plan (private DX surface).

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

/// NIKA-605: Signed-memory store error (F-P8 · `nika-store` envelope IO /
/// serialize / dir-layout · the signed-write/verified-recall substrate).
pub const NIKA_605: NikaCode = NikaCode {
    num: 605,
    category: Category::Memory,
    severity: Severity::Error,
    slug: "memory-store",
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
/// NIKA-434: The backend omitted the usage block on a priced model —
/// the ledger cannot bill the call honestly (fail-closed, wire
/// `NIKA-INFER-003`).
pub const NIKA_434: NikaCode = NikaCode {
    num: 434,
    category: Category::Verb,
    severity: Severity::Error,
    slug: "infer-usage-unmetered",
};
/// NIKA-435: The provider spent tokens yet the visible `infer` answer is
/// empty — a thinking model ate the budget on its reasoning trace (fail-
/// closed, wire `NIKA-INFER-004` · #651).
pub const NIKA_435: NikaCode = NikaCode {
    num: 435,
    category: Category::Verb,
    severity: Severity::Error,
    slug: "infer-empty-answer",
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
/// NIKA-463: The provider call failed mid-loop (internal identity —
/// the wire speaks the shared spec class `NIKA-INFER-001` · #468).
pub const NIKA_463: NikaCode = NikaCode {
    num: 463,
    category: Category::Verb,
    severity: Severity::Error,
    slug: "agent-inference",
};
/// NIKA-464: The agent's final message failed `schema:` validation
/// (internal identity — the wire speaks `NIKA-INFER-002` · #468).
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
/// NIKA-467: The agent loop stalled — identical actions with identical
/// observations repeated past the stall threshold (no progress; further
/// turns would spend budget for nothing · ADR-096).
pub const NIKA_467: NikaCode = NikaCode {
    num: 467,
    category: Category::Verb,
    severity: Severity::Error,
    slug: "agent-stalled",
};
/// NIKA-468: A whitelisted tool was refused by the security boundary
/// mid-loop (`permits:` capability boundary · the SSRF floor — internal
/// identity; the wire speaks the boundary's own spec code `NIKA-SEC-004`
/// / `NIKA-SEC-005`, one voice with the runtime's `security_err`).
pub const NIKA_468: NikaCode = NikaCode {
    num: 468,
    category: Category::Verb,
    severity: Severity::Error,
    slug: "agent-security-boundary",
};
/// NIKA-469: The backend omitted the usage block on a priced model —
/// the budget cannot meter the turn (fail-closed, wire `NIKA-AGENT-005`).
pub const NIKA_469: NikaCode = NikaCode {
    num: 469,
    category: Category::Verb,
    severity: Severity::Error,
    slug: "agent-usage-unmetered",
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

// ─── Runtime orchestration · 1700-1799 (nika-runtime L3 · s17) ──────────

/// NIKA-1700: A dirty `CheckReport` was handed to the runtime
/// (audit-before-run violated · a dirty workflow never executes).
pub const NIKA_1700: NikaCode = NikaCode {
    num: 1700,
    category: Category::Runtime,
    severity: Severity::Error,
    slug: "runtime-dirty-report",
};
/// NIKA-1701: A wave index fell outside the task list (the
/// checker/runtime schedule contract was breached).
pub const NIKA_1701: NikaCode = NikaCode {
    num: 1701,
    category: Category::Runtime,
    severity: Severity::Error,
    slug: "runtime-wave-out-of-bounds",
};
/// NIKA-1702: A rendered string still carries `${{` after
/// interpolation (unknown reference · the silent-literal guard).
pub const NIKA_1702: NikaCode = NikaCode {
    num: 1702,
    category: Category::Runtime,
    severity: Severity::Error,
    slug: "runtime-unresolved-template",
};
/// NIKA-1703: A `when:` expression is outside the v0 gate subset
/// (`<ref> == '<lit>'` · `<ref> != '<lit>'` · bare `<ref>`).
pub const NIKA_1703: NikaCode = NikaCode {
    num: 1703,
    category: Category::Runtime,
    severity: Severity::Error,
    slug: "runtime-when-unsupported",
};
/// NIKA-1704: The run crossed its operator budget (`--max-cost-usd`) —
/// in-flight work completed and was counted; unstarted tasks were
/// cancelled. The detail carries spent vs budget.
pub const NIKA_1704: NikaCode = NikaCode {
    num: 1704,
    category: Category::Runtime,
    severity: Severity::Error,
    slug: "runtime-budget-exceeded",
};
/// NIKA-1705: An exec `decode:` pipeline failure (spec 09 §decode) —
/// the captured bytes did not decode (strict-UTF-8 text violation ·
/// unparseable JSON/JSONL). Task-stage (`on_error:` scope) · engine
/// wire form until the spec registers a dedicated NIKA-EXEC row.
pub const NIKA_1705: NikaCode = NikaCode {
    num: 1705,
    category: Category::Runtime,
    severity: Severity::Error,
    slug: "runtime-decode-failure",
};
/// NIKA-1706: A run-time contract violation — the decoded value does
/// not fit the task's `returns:` type (spec 09 · the wire form is the
/// SPEC-PLANE `NIKA-TYPE-101`; this is its engine-internal identity).
pub const NIKA_1706: NikaCode = NikaCode {
    num: 1706,
    category: Category::Runtime,
    severity: Severity::Error,
    slug: "runtime-contract-violation",
};
/// NIKA-1707: The report's boundary lanes do not match the workflow
/// bytes — the run-start re-derivation (permits-fit · trifecta) found
/// something a clean report was credited with not having (audit-before-
/// run violated · a clean report over DIFFERENT bytes is not clean).
pub const NIKA_1707: NikaCode = NikaCode {
    num: 1707,
    category: Category::Runtime,
    severity: Severity::Error,
    slug: "runtime-report-mismatch",
};
/// NIKA-1708: A `required: true` input reached `run` with neither a
/// declared `default:` nor an operator `--var` override — the admission
/// preflight refuses the launch BEFORE the prologue (issue #603 · zero
/// events, zero spend; the mid-DAG NIKA-VAR-001 at the first read was
/// the bug).
pub const NIKA_1708: NikaCode = NikaCode {
    num: 1708,
    category: Category::Runtime,
    severity: Severity::Error,
    slug: "runtime-missing-required-input",
};
/// NIKA-1709: The run's unavoidable cost floor already exceeds the budget
/// it was launched under (`--max-cost-usd`, or an inherited
/// `min(parent remaining, child declared)` under composition · spec 14
/// law 6). The ADMISSION form of the budget law — refuse BEFORE the
/// prologue (zero events, zero spend) for every embedder; the mid-run
/// crossing stays NIKA-1704's (the ledger sees what the static floor
/// cannot).
pub const NIKA_1709: NikaCode = NikaCode {
    num: 1709,
    category: Category::Runtime,
    severity: Severity::Error,
    slug: "runtime-budget-floor",
};

/// NIKA-1710: The sandbox policy requires OS confinement the host cannot
/// provide (#889 · ADR-080 Q4.B amended): a workflow declaring `permits:`
/// under `NIKA_SANDBOX=auto|require` on a host with no Seatbelt/bwrap —
/// the composition root refuses BEFORE the prologue (zero events, zero
/// spend), naming the exact per-OS fix and the witnessed opt-out
/// (`NIKA_SANDBOX=off`, attested on the journal's opening frame). The
/// launch-refusal class (the NIKA-1708/1709 precedent): run-abort, never
/// a task failure — no `on_codes` ladder exists for it by design.
pub const NIKA_1710: NikaCode = NikaCode {
    num: 1710,
    category: Category::Runtime,
    severity: Severity::Error,
    slug: "runtime-sandbox-required",
};

/// NIKA-1711: The `NIKA_SANDBOX` knob held anything but
/// `auto | require | off` (#889) — a typo'd security knob refuses the
/// launch BEFORE the prologue (fail-closed; a loud default would be the
/// fail-open class the policy exists to kill). The launch-refusal class
/// (the NIKA-1708..1710 precedent): run-abort, never a task failure.
pub const NIKA_1711: NikaCode = NikaCode {
    num: 1711,
    category: Category::Runtime,
    severity: Severity::Error,
    slug: "runtime-sandbox-policy-invalid",
};

/// NIKA-1800: No access path survives admission for the requested
/// model (D-2026-08-04-N1) — every enumerated candidate was rejected,
/// each with its witness (dimension + layer + teaching line · A-8).
/// The TOTAL refusal form of the resolver; the single-candidate
/// dimensions keep their own codes below.
pub const NIKA_1800: NikaCode = NikaCode {
    num: 1800,
    category: Category::Access,
    severity: Severity::Error,
    slug: "access-no-path",
};
/// NIKA-1801: An explicit `--access` pin is unsatisfied — the pinned
/// path is absent or inadmissible here. A pin is a pin: refusal,
/// never a silent substitute (A-4).
pub const NIKA_1801: NikaCode = NikaCode {
    num: 1801,
    category: Category::Access,
    severity: Severity::Error,
    slug: "access-pin-unsatisfied",
};
/// NIKA-1802: The `--access` token names neither an access class
/// (`local` · `api` · `harness` · `oauth` · `mock`) nor a known
/// agentic CLI (`claude-code` · `codex` · `gemini-cli` · `kimi-code`
/// · `qwen-code`) — refused before any resolution runs, so a typo
/// can never read as an empty-candidate refusal.
pub const NIKA_1802: NikaCode = NikaCode {
    num: 1802,
    category: Category::Access,
    severity: Severity::Error,
    slug: "access-unknown-token",
};
/// NIKA-1803: A known agentic CLI token cannot run here — binary
/// absent, ACP speaker missing, or this nika was built without
/// adapters. Dummy-readable: install the CLI or pick Nika local /
/// Nika Cloud.
pub const NIKA_1803: NikaCode = NikaCode {
    num: 1803,
    category: Category::Access,
    severity: Severity::Error,
    slug: "access-harness-unavailable",
};
/// NIKA-1804: The harness session died mid-run (process exit · wire
/// breakdown) — the one TRANSIENT row of the family.
pub const NIKA_1804: NikaCode = NikaCode {
    num: 1804,
    category: Category::Access,
    severity: Severity::Error,
    slug: "access-harness-session",
};
/// NIKA-1805: The harness itself refused (auth absent on ITS side ·
/// unsupported capability) — its own words ride verbatim.
pub const NIKA_1805: NikaCode = NikaCode {
    num: 1805,
    category: Category::Access,
    severity: Severity::Error,
    slug: "access-harness-refused",
};
/// NIKA-1806: The harness asked for authority the workflow's `permits:`
/// grants do not cover — the run PAUSES for the operator (the durable
/// human gate · ADR-099's harness twin), never an auto-grant. The gate
/// question rides the pause verbatim; `--resume --answer <task>=true`
/// grants it once, `false` denies.
pub const NIKA_1806: NikaCode = NikaCode {
    num: 1806,
    category: Category::Access,
    severity: Severity::Error,
    slug: "access-harness-gate",
};

/// NIKA-1807: A resume would switch access silently — the trace rode
/// one path for a model and this machine now resolves another; the
/// resume refuses unless `--access` keeps the recorded path or names
/// the change (One Door · « resume cannot switch access silently »).
pub const NIKA_1807: NikaCode = NikaCode {
    num: 1807,
    category: Category::Access,
    severity: Severity::Error,
    slug: "access-resume-moved",
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
    NIKA_016, NIKA_017, NIKA_018, NIKA_234, NIKA_430, NIKA_431, NIKA_432, NIKA_433, NIKA_434,
    NIKA_435, NIKA_440, NIKA_441, NIKA_442, NIKA_450, NIKA_451, NIKA_452, NIKA_460, NIKA_461,
    NIKA_462, NIKA_463, NIKA_464, NIKA_465, NIKA_466, NIKA_467, NIKA_468, NIKA_469, NIKA_600,
    NIKA_601, NIKA_602, NIKA_603, NIKA_604, NIKA_605, NIKA_700, NIKA_750, NIKA_800, NIKA_999,
    NIKA_1000, NIKA_1001, NIKA_1002, NIKA_1003, NIKA_1004, NIKA_1005, NIKA_1006, NIKA_1007,
    NIKA_1008, NIKA_1009, NIKA_1101, NIKA_1102, NIKA_1103, NIKA_1104, NIKA_1105, NIKA_1106,
    NIKA_1107, NIKA_1108, NIKA_1109, NIKA_1201, NIKA_1202, NIKA_1203, NIKA_1204, NIKA_1205,
    NIKA_1206, NIKA_1301, NIKA_1302, NIKA_1303, NIKA_1304, NIKA_1305, NIKA_1401, NIKA_1402,
    NIKA_1403, NIKA_1404, NIKA_1405, NIKA_1406, NIKA_1501, NIKA_1502, NIKA_1503, NIKA_1504,
    NIKA_1505, NIKA_1601, NIKA_1602, NIKA_1603, NIKA_1604, NIKA_1605, NIKA_1700, NIKA_1701,
    NIKA_1702, NIKA_1703, NIKA_1704, NIKA_1705, NIKA_1706, NIKA_1707, NIKA_1708, NIKA_1709,
    NIKA_1710, NIKA_1711, NIKA_1800, NIKA_1801, NIKA_1802, NIKA_1803, NIKA_1804, NIKA_1805,
    NIKA_1806, NIKA_1807,
];
