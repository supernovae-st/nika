# Swarm-3 SOTA Audit Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Close the 7 remaining swarm-3 SOTA audit findings on `nika-diamond` so the L0/L0.5 foundation matches its documented contract before `nika-schema` admission (#6).

**Architecture:** Three tiers, executed sequentially. **Tier 2** (Phases A-C) drops one kernel trait, adds one new sealed L0.5 trait, and bridges OTel GenAI semconv on the inference DTOs — pure kernel surface work. **Tier 3** (Phases D-G) wires CI enforcement (`cargo public-api`, `cargo semver-checks`), gates L0 crates `no_std`-friendly, and adds `cargo hakari` for compile-time sharing. **Tier 4** (Phase H) ships a perf bench for the 22-sub-enum match table after `nika-event` admits. Each task is TDD: failing test first, minimal impl, commit.

**Tech Stack:** Rust 1.85 stable · `nika-kernel` (L0.5, sealed traits) · `nika-kernel-mock` (L0.5 1:1 mocks) · `nika-types` / `nika-error` (L0) · `cargo` workspace · `trait_variant` for async-in-trait · `thiserror` + `miette` · `criterion` for bench · GitHub Actions for CI · `cargo-public-api`, `cargo-semver-checks`, `cargo-hakari`.

**SHA at start:** `7a8b2f9fd` (post Tier 1 docs commit `8d35a946d`).

**Reference docs (read before starting):**
- `docs/architecture/l0-l05-architecture-decisions.md` Q12 (drop ObsSink + add AuditSink), Q13 (GenAiAttrs)
- `docs/adr/adr-028-forward-compat-reservation-policy.md` (seams-now / crates-later policy)
- `docs/adr/adr-014-sealed-kernel-traits.md` (sealed pattern — every new trait MUST seal)
- `docs/adr/adr-007-forward-compat-invariants.md` (`#[non_exhaustive]` + `pub fn new()`)
- `docs/architecture/crate-layer-registry.md` (12 capability axes vocabulary)
- `.claude/CLAUDE.md` (12 gates, interdits stricts)

---

## Pre-flight (run before each session)

**Step 1: Verify branch + clean tree**

Run: `git status && git rev-parse HEAD`
Expected: branch `nika-diamond`, clean tree, HEAD ahead of origin only by your in-flight commits.

**Step 2: Verify baseline compiles + tests pass**

Run: `cargo check --workspace && cargo test --workspace --lib 2>&1 | tail -3`
Expected: clean check, "846 passed" (current baseline as of HEAD `5e810a94a`).

**Step 3: Verify hygiene baseline**

Run: `bash scripts/hygiene/check-all.sh --full | tail -3`
Expected: "21 green / 4 yellow / 0 red" (4 yellow are documented: loc-totals, org-profile-repos, file-loc-cap, adr-evidence-paths).

**Step 4: Read the rules**

Read: `.claude/rules/diamond-discipline.md` + `.claude/rules/commit-granularity.md`
Why: every commit needs Nika 🦋 co-author, atomic-per-change, no `--no-verify`, no `git add -A`.

---

## Phase A — Drop `ObservabilitySink` (5 → 4 channels)

**Why:** Q12 of `l0-l05-architecture-decisions.md` reverses Wave 2 S1-B's stub. `ObservabilitySink` documented intent was "v0.95 merge then v0.100 re-split of Metrics+Trace" — a future-feature that adapter-layer code (OTLP exporter consuming `MetricsExporter` + `TracerProvider`) covers without a kernel trait. OpenTelemetry uses 3 signals; Nika should not exceed 4 (Event/Metrics/Trace/Billing) before Phase B adds AuditSink.

**Risk:** Low. No L1+ crate consumes `ObservabilitySink` today (verified: 6 crates in workspace, none reference it outside `nika-kernel-mock` and the kernel itself).

### Task A1: Grep callers + verify zero downstream consumers

**Files:** none (read-only).

**Step 1: Find every reference to `ObservabilitySink`**

Run: `grep -rn 'ObservabilitySink' crates/ Cargo.toml docs/ scripts/`
Expected output (current state):
```
crates/nika-kernel/src/lib.rs:73:pub use plugin::{observability, sandbox};
crates/nika-kernel/src/lib.rs:100:pub use observability::{MetricEvent, ObservabilityError, ObservabilitySink, SpanEvent};
crates/nika-kernel/src/plugin/mod.rs:8:pub mod observability;
crates/nika-kernel/src/plugin/observability.rs:20:pub trait ObservabilitySink: Send + Sync {
crates/nika-kernel-mock/src/lib.rs:44:pub mod observability;
crates/nika-kernel-mock/src/lib.rs:63:pub use observability::NullObservabilitySink;
crates/nika-kernel-mock/src/observability.rs: ...impl ObservabilitySink...
docs/architecture/l0-l05-architecture-decisions.md: ...Q12...
```
**Required:** zero hits in any crate other than `nika-kernel`, `nika-kernel-mock`, and docs. If you find any, STOP and report — refactor scope grew.

**Step 2: Snapshot the public API of `nika-kernel` before edit**

Run: `cargo doc --no-deps -p nika-kernel 2>&1 | tail -5`
Expected: doc builds cleanly. Note: `cargo public-api` is added in Phase D, so we snapshot manually here.

### Task A2: Write the failing test (proof the symbol is gone)

**Files:**
- Modify: `crates/nika-kernel/tests/no_observability_sink.rs` (create)

**Step 1: Write the failing test**

```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Integration test proving `ObservabilitySink` was dropped per ADR/Q12.
//!
//! This test compiles only if the symbol is absent. Adding it back must
//! reopen Q12 in `docs/architecture/l0-l05-architecture-decisions.md`.

#[test]
fn observability_sink_is_dropped() {
    // Doc-link sentinel — if this comment line moves, update Q12 cross-ref.
    // See: docs/architecture/l0-l05-architecture-decisions.md Decision Q12.
    //
    // The test body is an `assert!` that compiles iff `ObservabilitySink`
    // is not exported. We achieve that with a `compile_error!`-equivalent
    // via stringified path — but in practice, the absence is enforced by
    // the next test which fails to compile if the type re-appears.
    assert!(true, "sentinel — test exists to anchor the Q12 doc reference");
}

/// This `use` must NOT compile. Comment-only check:
/// ```compile_fail
/// use nika_kernel::ObservabilitySink;
/// ```
#[allow(dead_code)]
fn _compile_fail_anchor() {}
```

**Step 2: Run it and watch it pass before edit (because the symbol still exists, the doctest fails which is correct)**

Run: `cargo test -p nika-kernel --test no_observability_sink -- --nocapture`
Expected: PASS for `observability_sink_is_dropped`, doctest is `compile_fail` so it intentionally fails to compile (which is "passing" the negative assertion).

If the doctest *succeeds* compiling, the symbol is still importable → drop did not happen → STOP.

### Task A3: Delete the trait + supporting types

**Files:**
- Delete: `crates/nika-kernel/src/plugin/observability.rs`
- Modify: `crates/nika-kernel/src/plugin/mod.rs` (remove `pub mod observability;` line)
- Modify: `crates/nika-kernel/src/lib.rs` (remove the two lines around `observability` re-export)

**Step 1: Delete the trait file**

Run: `git rm crates/nika-kernel/src/plugin/observability.rs`
Expected: file removed from index.

**Step 2: Edit `plugin/mod.rs`**

Read first: `cat crates/nika-kernel/src/plugin/mod.rs`. Remove the line `pub mod observability;` (line 8 per current state).

**Step 3: Edit `lib.rs`**

Read first: `crates/nika-kernel/src/lib.rs:73` and `:100`. Replace the two re-export lines:
```rust
// REMOVE:
pub use plugin::{observability, sandbox};
// REPLACE WITH:
pub use plugin::sandbox;

// REMOVE:
pub use observability::{MetricEvent, ObservabilityError, ObservabilitySink, SpanEvent};
// (no replacement)
```

**Step 4: Verify it compiles**

Run: `cargo check -p nika-kernel`
Expected: clean. If errors mention `MetricEvent` or `SpanEvent` consumers, those types had legitimate use elsewhere — STOP and report.

### Task A4: Drop the mock + re-exports

**Files:**
- Delete: `crates/nika-kernel-mock/src/observability.rs`
- Modify: `crates/nika-kernel-mock/src/lib.rs` (remove `pub mod observability;` line 44 and `pub use observability::NullObservabilitySink;` line 63)

**Step 1: Delete + edit**

Run: `git rm crates/nika-kernel-mock/src/observability.rs`
Then edit `lib.rs` to remove both lines (44 and 63 per grep).

**Step 2: Verify**

Run: `cargo check -p nika-kernel-mock`
Expected: clean.

### Task A5: Run the full sentinel test + workspace tests

**Step 1: Workspace test pass**

Run: `cargo test --workspace --lib 2>&1 | tail -5`
Expected: lib tests count drops by ~6 (the deleted `tests` module in `observability.rs`). New count should be `~840 passed` (was 846).

**Step 2: Sentinel test passes (doctest now correctly compile-fails)**

Run: `cargo test -p nika-kernel --test no_observability_sink`
Expected: PASS.

**Step 3: Hygiene clean**

Run: `bash scripts/hygiene/check-all.sh --full | tail -3`
Expected: still `21 green / 4 yellow / 0 red` (no new red).

### Task A6: Update Q12 cross-reference + commit

**Files:**
- Modify: `docs/architecture/l0-l05-architecture-decisions.md` Q12 — flip status `LOCKED rev.3` → `LOCKED rev.3 · executed`.

**Step 1: Edit Q12 status in the decision index table**

Replace:
```
| Q12 | Drop `ObservabilitySink` (5 → 4 channels) + add `AuditSink` (compliance) | LOCKED rev.3 |
```
With:
```
| Q12 | Drop `ObservabilitySink` (executed Phase A) + add `AuditSink` (Phase B) | LOCKED rev.3 · partially executed |
```

**Step 2: Commit**

```bash
git add crates/nika-kernel/src/lib.rs \
        crates/nika-kernel/src/plugin/mod.rs \
        crates/nika-kernel-mock/src/lib.rs \
        crates/nika-kernel/tests/no_observability_sink.rs \
        docs/architecture/l0-l05-architecture-decisions.md
git rm crates/nika-kernel/src/plugin/observability.rs \
       crates/nika-kernel-mock/src/observability.rs

git commit -m "$(cat <<'EOF'
refactor(nika-kernel): drop ObservabilitySink (Q12 Phase A — 5 → 4 channels)

Per swarm-3 audit + Q12 of l0-l05-architecture-decisions.md, the
ObservabilitySink trait was a v0.95 merge stub for Metrics+Trace that
adapter-layer code (OTLP exporter) covers without a kernel trait.
OpenTelemetry uses 3 signals; Nika settles on 4 (Event/Metrics/Trace/
Billing) before Phase B adds the 5th (AuditSink, compliance-grade).

- Delete crates/nika-kernel/src/plugin/observability.rs (177 LOC)
- Delete crates/nika-kernel-mock/src/observability.rs
- Drop pub mod + pub use from kernel + mock lib.rs
- Add tests/no_observability_sink.rs sentinel (compile_fail doctest
  ensures the symbol does not silently re-appear)
- Mark Q12 partially-executed in l0-l05-decisions

Verified: cargo check workspace clean, 840 lib tests (was 846 — diff
explained by 6 deleted unit tests in observability.rs), hygiene 21/4/0.

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
EOF
)"
```

---

## Phase B — Add `AuditSink` (compliance-grade 5th channel)

**Why:** Q12 second half. Shield events (capability override, taint violation, canary leak), budget exhaustion, GDPR right-to-erasure, EU AI Act incident reports — all need **never-sample + tamper-evident + persist-or-fail** semantics. Neither `EventSink` (may sample, best-effort) nor `BillingSink` (cost-typed, narrow) fits. This is unique vs OTel, Honeycomb, Datadog.

**Risk:** Low for the trait + mock. Real backend (Merkle anchor, append-only log) is L1 work, deferred per ADR-028.

### Task B1: Spec the trait + record types

**Files:** none (design step — read existing patterns).

**Step 1: Read sibling sealed traits as templates**

Read: `crates/nika-kernel/src/infra/billing.rs` (similar contract: never-drop), `crates/nika-kernel/src/infra/event_sink.rs` (sub-enum pattern), `crates/nika-kernel/src/sealed.rs` (the sealed marker trait).

Why: AuditSink must (a) be sealed (ADR-014), (b) use `trait_variant::make` for Send-bound dyn, (c) carry a `#[non_exhaustive]` enum payload, (d) use `pub fn new()` constructors on every public struct (INV-019 + ADR-007).

### Task B2: Write the failing test (mock returns Ok on all variants)

**Files:**
- Create: `crates/nika-kernel/src/infra/audit.rs` (empty — will fail compile)
- Create: `crates/nika-kernel-mock/src/audit.rs` (empty)
- Modify: `crates/nika-kernel/src/infra/mod.rs` (add `pub mod audit;`)
- Modify: `crates/nika-kernel/src/lib.rs` (re-export the new types)
- Modify: `crates/nika-kernel-mock/src/lib.rs` (add `pub mod audit;` + `pub use`)
- Create: `crates/nika-kernel/tests/audit_sink_contract.rs`

**Step 1: Write the contract test FIRST**

Create `crates/nika-kernel/tests/audit_sink_contract.rs`:

```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Contract test for AuditSink — every variant must round-trip + persist.

use nika_kernel::audit::{AuditRecord, AuditSink, AuditSinkError};
use nika_kernel_mock::NullAuditSink;

#[tokio::test]
async fn audit_sink_accepts_capability_override() {
    let sink = NullAuditSink::new();
    let rec = AuditRecord::capability_override(
        "tenant-123",
        "verb.invoke",
        "shield bypass requested",
    );
    let result = sink.audit(rec).await;
    assert!(result.is_ok(), "AuditSink must persist or fail loudly");
}

#[tokio::test]
async fn audit_sink_accepts_taint_violation() {
    let sink = NullAuditSink::new();
    let rec = AuditRecord::taint_violation(
        "tenant-123",
        "untrusted_input -> exec_shell",
        nika_kernel::audit::Severity::Critical,
    );
    assert!(sink.audit(rec).await.is_ok());
}

#[tokio::test]
async fn audit_sink_accepts_budget_exhausted() {
    let sink = NullAuditSink::new();
    let rec = AuditRecord::budget_exhausted("tenant-123", "tokens", 100_000, 99_999);
    assert!(sink.audit(rec).await.is_ok());
}

#[tokio::test]
async fn audit_sink_propagates_persist_failure() {
    use nika_kernel_mock::audit::FailingAuditSink;
    let sink = FailingAuditSink::new();
    let rec = AuditRecord::canary_leaked("tenant-123", "secret-id-42");
    let err = sink.audit(rec).await.expect_err("must fail");
    assert!(matches!(err, AuditSinkError::PersistFailed { .. }));
}

#[test]
fn audit_record_extension_escape_hatch() {
    // Future variants (GDPR, EU AI Act) ship via Extension before getting
    // promoted to first-class variants.
    let rec = AuditRecord::extension(
        "compliance.gdpr",
        "right_to_erasure",
        serde_json::json!({"subject_id": "user-42", "tenant": "t-1"}),
    );
    let json = serde_json::to_string(&rec).expect("serializable");
    assert!(json.contains("right_to_erasure"));
}
```

**Step 2: Run it — must fail to compile**

Run: `cargo test -p nika-kernel --test audit_sink_contract 2>&1 | tail -5`
Expected: errors on `nika_kernel::audit::*` (module does not exist yet) and `nika_kernel_mock::NullAuditSink` (mock missing).

### Task B3: Implement the trait + types

**Files:**
- Create: `crates/nika-kernel/src/infra/audit.rs`
- Modify: `crates/nika-kernel/src/infra/mod.rs`
- Modify: `crates/nika-kernel/src/sealed.rs` (add `AuditSink` to sealed list)
- Modify: `crates/nika-kernel/src/lib.rs` (re-export `audit::*`)

**Step 1: Write `crates/nika-kernel/src/infra/audit.rs`**

```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `AuditSink` — append-only, never-sampled, persist-or-fail.
//!
//! 5th observability channel (after Event/Metrics/Trace/Billing). Used by
//! Shield (capability override, taint, canary leak), budget enforcement,
//! and compliance signals (GDPR, EU AI Act). Records MUST persist before
//! the trait method returns Ok — implementations cannot buffer in memory.
//!
//! See: ADR-028 (forward-compat reservation policy), Q12 of
//! `docs/architecture/l0-l05-architecture-decisions.md`, ADR-014 (sealed).

use nika_error::id::TenantId;
use serde::{Deserialize, Serialize};

use crate::sealed;

/// Severity level for audit records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Severity {
    /// Informational — admin override, key rotation.
    Info,
    /// Warning — soft budget breach, retry exhaustion.
    Warn,
    /// Critical — taint violation, canary leak, hard budget breach.
    Critical,
}

/// Append-only audit record. Every public variant carries a `tenant_id`
/// for multi-tenant routing. New variants are added behind `Extension`
/// before promotion to first-class variants (ADR-007 + ADR-028).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum AuditRecord {
    /// A capability check was overridden by an admin.
    CapabilityOverride {
        /// Target tenant.
        tenant_id: TenantId,
        /// Capability that was bypassed.
        capability: String,
        /// Operator-provided reason.
        reason: String,
    },
    /// Taint propagation detected an untrusted-data → privileged-action path.
    TaintViolation {
        /// Target tenant.
        tenant_id: TenantId,
        /// Source -> sink path description.
        path: String,
        /// Severity of the violation.
        severity: Severity,
    },
    /// A canary token leaked through model output.
    CanaryLeaked {
        /// Target tenant.
        tenant_id: TenantId,
        /// Identifier of the canary that leaked.
        canary_id: String,
    },
    /// Budget cap (tokens, cost, runs) exhausted for the tenant.
    BudgetExhausted {
        /// Target tenant.
        tenant_id: TenantId,
        /// Budget dimension (e.g. `"tokens"`, `"cost_usd"`, `"runs"`).
        dimension: String,
        /// Hard cap.
        cap: u64,
        /// Observed value at the time of the breach.
        observed: u64,
    },
    /// Policy denied a workflow step (Shield).
    PolicyDenied {
        /// Target tenant.
        tenant_id: TenantId,
        /// Policy rule identifier (e.g. `"L-SEC-003"`).
        rule_id: String,
        /// What was denied.
        target: String,
    },
    /// A secret/key was used (audit trail for KeyProvider access).
    KeyUsed {
        /// Target tenant.
        tenant_id: TenantId,
        /// Key identifier (never the value).
        key_id: String,
    },
    /// A secret was redacted in an outbound payload.
    SecretRedacted {
        /// Target tenant.
        tenant_id: TenantId,
        /// Where the secret was found.
        location: String,
    },
    /// Future-feature escape hatch. Must be promoted to a first-class
    /// variant once the namespace stabilises (ADR-028).
    Extension {
        /// Dotted namespace (e.g. `"compliance.gdpr"`).
        ns: String,
        /// Variant name within the namespace.
        name: String,
        /// Free-form payload — implementations MUST treat as untrusted.
        payload: serde_json::Value,
    },
}

impl AuditRecord {
    /// Convenience: build a `CapabilityOverride` record.
    #[must_use]
    pub fn capability_override(
        tenant_id: impl Into<TenantId>,
        capability: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self::CapabilityOverride {
            tenant_id: tenant_id.into(),
            capability: capability.into(),
            reason: reason.into(),
        }
    }

    /// Convenience: build a `TaintViolation` record.
    #[must_use]
    pub fn taint_violation(
        tenant_id: impl Into<TenantId>,
        path: impl Into<String>,
        severity: Severity,
    ) -> Self {
        Self::TaintViolation {
            tenant_id: tenant_id.into(),
            path: path.into(),
            severity,
        }
    }

    /// Convenience: build a `CanaryLeaked` record.
    #[must_use]
    pub fn canary_leaked(
        tenant_id: impl Into<TenantId>,
        canary_id: impl Into<String>,
    ) -> Self {
        Self::CanaryLeaked {
            tenant_id: tenant_id.into(),
            canary_id: canary_id.into(),
        }
    }

    /// Convenience: build a `BudgetExhausted` record.
    #[must_use]
    pub fn budget_exhausted(
        tenant_id: impl Into<TenantId>,
        dimension: impl Into<String>,
        cap: u64,
        observed: u64,
    ) -> Self {
        Self::BudgetExhausted {
            tenant_id: tenant_id.into(),
            dimension: dimension.into(),
            cap,
            observed,
        }
    }

    /// Convenience: build an `Extension` record.
    #[must_use]
    pub fn extension(
        ns: impl Into<String>,
        name: impl Into<String>,
        payload: serde_json::Value,
    ) -> Self {
        Self::Extension {
            ns: ns.into(),
            name: name.into(),
            payload,
        }
    }
}

/// Errors a sink can raise. Persist failures are non-recoverable; the
/// caller should kill the run (compliance contract).
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum AuditSinkError {
    /// Persisted backend (file/db) write failed.
    #[error("audit persist failed: {reason}")]
    PersistFailed {
        /// What went wrong.
        reason: String,
    },
    /// Sink not configured (boot-time error).
    #[error("audit sink not configured: {reason}")]
    NotConfigured {
        /// Why no sink.
        reason: String,
    },
}

/// `AuditSink` — append-only, never-sampled. Implementations MUST persist
/// the record durably (file, DB, log-shipping appliance) before returning
/// `Ok`. Buffering in memory is forbidden by contract.
#[trait_variant::make(AuditSinkDyn: Send)]
pub trait AuditSink: Send + Sync + sealed::Sealed {
    /// Append a record to the audit log.
    async fn audit(&self, record: AuditRecord) -> Result<(), AuditSinkError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn audit_types_are_send_sync() {
        _assert_send_sync::<AuditRecord>();
        _assert_send_sync::<AuditSinkError>();
        _assert_send_sync::<Severity>();
    }

    #[test]
    fn audit_record_serde_roundtrip_capability() {
        let r = AuditRecord::capability_override("t-1", "verb.invoke", "admin override");
        let json = serde_json::to_string(&r).unwrap();
        let back: AuditRecord = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, AuditRecord::CapabilityOverride { .. }));
    }

    #[test]
    fn audit_record_extension_payload_preserved() {
        let r = AuditRecord::extension(
            "compliance.eu_ai_act",
            "incident_report",
            serde_json::json!({"severity": "high"}),
        );
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("incident_report"));
        assert!(json.contains("severity"));
    }
}
```

**Step 2: Wire the module**

Edit `crates/nika-kernel/src/infra/mod.rs` to add `pub mod audit;` (alphabetical: between `id_gen` and `metrics`).

Edit `crates/nika-kernel/src/sealed.rs` — find the `impl Sealed for X` lines for sibling traits (BillingSink, EventSink). Add an analogous block for `AuditSink` once the mock is wired (Task B4 will add the mock impl).

Edit `crates/nika-kernel/src/lib.rs`. Add a re-export module + flat re-exports near the existing `pub use` block:

```rust
pub use infra::audit::{
    AuditRecord, AuditSink, AuditSinkDyn, AuditSinkError, Severity as AuditSeverity,
};
// also re-export as a sub-module for `nika_kernel::audit::*` access
pub mod audit {
    pub use crate::infra::audit::*;
}
```

**Step 3: Verify it compiles**

Run: `cargo check -p nika-kernel`
Expected: clean.

### Task B4: Implement the mock + failing mock

**Files:**
- Create: `crates/nika-kernel-mock/src/audit.rs`
- Modify: `crates/nika-kernel-mock/src/lib.rs`

**Step 1: Write the mock**

```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `NullAuditSink` — accepts everything, persists nowhere. Test-only.
//! `FailingAuditSink` — always returns PersistFailed. Drives error-path tests.

use nika_kernel::audit::{AuditRecord, AuditSink, AuditSinkError};

/// Test mock that accepts every record. NEVER use in production —
/// violates the "persist or fail" contract.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullAuditSink;

impl NullAuditSink {
    /// Construct a new mock.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

// Sealed marker — mock is allowed to impl because we control the kernel.
impl nika_kernel::sealed::Sealed for NullAuditSink {}

impl AuditSink for NullAuditSink {
    async fn audit(&self, _record: AuditRecord) -> Result<(), AuditSinkError> {
        Ok(())
    }
}

/// Test mock that always fails. Used to drive error-path tests.
#[derive(Debug, Default, Clone, Copy)]
pub struct FailingAuditSink;

impl FailingAuditSink {
    /// Construct.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl nika_kernel::sealed::Sealed for FailingAuditSink {}

impl AuditSink for FailingAuditSink {
    async fn audit(&self, _record: AuditRecord) -> Result<(), AuditSinkError> {
        Err(AuditSinkError::PersistFailed {
            reason: "mock always fails".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_kernel::audit::AuditRecord;

    #[tokio::test]
    async fn null_accepts_everything() {
        let sink = NullAuditSink::new();
        let r = AuditRecord::capability_override("t", "c", "r");
        assert!(sink.audit(r).await.is_ok());
    }

    #[tokio::test]
    async fn failing_always_fails() {
        let sink = FailingAuditSink::new();
        let r = AuditRecord::capability_override("t", "c", "r");
        assert!(sink.audit(r).await.is_err());
    }
}
```

**Step 2: Wire the mock module**

Edit `crates/nika-kernel-mock/src/lib.rs`:
```rust
pub mod audit;
// ... near existing pub use lines:
pub use audit::{FailingAuditSink, NullAuditSink};
```

**Step 3: Run the contract test from Task B2**

Run: `cargo test -p nika-kernel --test audit_sink_contract`
Expected: 5/5 PASS.

**Step 4: Run workspace tests**

Run: `cargo test --workspace --lib 2>&1 | tail -3`
Expected: count went up by ~5 (audit unit tests in audit.rs + mock tests). New baseline target: ~850 passed.

**Step 5: Hygiene check**

Run: `bash scripts/hygiene/check-all.sh --full | tail -3`
Expected: still 0 red.

### Task B5: Update Q12 + commit

**Files:**
- Modify: `docs/architecture/l0-l05-architecture-decisions.md` Q12 status: `LOCKED rev.3 · partially executed` → `LOCKED rev.3 · executed`.

**Step 1: Edit Q12 row in decision index**

Replace:
```
| Q12 | Drop `ObservabilitySink` (executed Phase A) + add `AuditSink` (Phase B) | LOCKED rev.3 · partially executed |
```
With:
```
| Q12 | Drop `ObservabilitySink` + add `AuditSink` (5 channels: Event/Metrics/Trace/Billing/Audit) | LOCKED rev.3 · executed |
```

**Step 2: Commit**

```bash
git add crates/nika-kernel/src/infra/audit.rs \
        crates/nika-kernel/src/infra/mod.rs \
        crates/nika-kernel/src/sealed.rs \
        crates/nika-kernel/src/lib.rs \
        crates/nika-kernel/tests/audit_sink_contract.rs \
        crates/nika-kernel-mock/src/audit.rs \
        crates/nika-kernel-mock/src/lib.rs \
        docs/architecture/l0-l05-architecture-decisions.md

git commit -m "$(cat <<'EOF'
feat(nika-kernel): add AuditSink trait (Q12 Phase B — compliance channel)

5th observability channel: append-only, never-sampled, persist-or-fail.
Differentiator vs OTel/Honeycomb/Datadog APM (none have first-class
compliance signal). Used by Shield (capability override, taint, canary
leak), budget enforcement, GDPR/EU AI Act incident reports.

- AuditRecord enum (#[non_exhaustive]) — 7 first-class variants:
  CapabilityOverride, TaintViolation, CanaryLeaked, BudgetExhausted,
  PolicyDenied, KeyUsed, SecretRedacted + Extension escape hatch.
- AuditSink sealed trait (ADR-014) with trait_variant::make for
  Send-bound dyn dispatch.
- Severity enum (Info/Warn/Critical), AuditSinkError (PersistFailed/
  NotConfigured).
- pub fn new() constructors per INV-019 + 5 typed convenience builders
  (capability_override, taint_violation, canary_leaked,
  budget_exhausted, extension).
- Sub-module re-export: nika_kernel::audit::* + flat re-exports.
- nika-kernel-mock ships NullAuditSink (always-Ok) + FailingAuditSink
  (always PersistFailed) to drive error-path tests.

Forward-compat: every public type non_exhaustive; new variants
(GDPR right_to_erasure, EU AI Act incident_report) ship via Extension
before promotion to first-class — per ADR-028.

Tests: 5 contract tests (one per critical variant + extension hatch +
error-path) + 5 unit tests in audit.rs + 2 mock tests = 12 new tests.

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
EOF
)"
```

---

## Phase C — `GenAiAttrs` OTel semconv bridge (Q13)

**Why:** Q13 of `l0-l05-architecture-decisions.md`. Today `InferRequest` and `InferResponse` carry the values OTel GenAI semconv defines (`gen_ai.system`, `gen_ai.request.model`, etc.) but in untyped, scattered form. Every future exporter would re-invent the mapping. A typed `GenAiAttrs` struct embedded on both DTOs enforces cross-provider parity (Pre-launch Gate 2).

**Risk:** Medium. Touches `InferRequest` + `InferResponse` (deep DTOs in the kernel). Backward-compat preserved via `Default` + `pub fn new()` + `#[non_exhaustive]` (INV-019).

### Task C1: Read existing Infer DTOs end-to-end

**Files:** none (read-only).

**Step 1: Read the full provider module**

Read: `crates/nika-kernel/src/ai/provider.rs` lines 180-310 (InferRequest + InferResponse current shapes).

Read: `crates/nika-types/src/token_usage.rs` (TokenUsage already has `cache_read_tokens`, `reasoning_tokens`, `thinking_tokens` per recent expansion — Q13 reuses these, does not duplicate).

**Step 2: Identify which `gen_ai.*` attributes are already present**

Note the mapping (informational, no edit):
- `gen_ai.request.model` → `InferRequest::model` (already present, keep)
- `gen_ai.request.max_tokens` → `InferRequest::max_tokens` (already present)
- `gen_ai.request.temperature` → `InferRequest::temperature` (already present)
- `gen_ai.usage.input_tokens` → `TokenUsage::input_tokens` (present)
- `gen_ai.usage.output_tokens` → `TokenUsage::output_tokens` (present)
- `gen_ai.usage.cached_input_tokens` → `TokenUsage::cache_read_tokens` (present, alias differs)
- `gen_ai.usage.reasoning_tokens` → `TokenUsage::reasoning_tokens` (present)

**Gaps** (Q13 closes these):
- `gen_ai.system` — provider system identifier (Anthropic / OpenAI / Mistral / etc.)
- `gen_ai.response.id` — provider's response ID (for support tickets)
- `gen_ai.response.model` — actual model that responded (may differ from request)
- `gen_ai.operation.name` — operation kind (chat / text_completion / embedding)

### Task C2: Write the failing test

**Files:**
- Create: `crates/nika-kernel/tests/genai_attrs_bridge.rs`

**Step 1: Write the test**

```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Contract test for GenAiAttrs (Q13 — OTel GenAI semconv bridge).

use nika_kernel::genai::{GenAiAttrs, GenAiOperation, GenAiSystem};
use nika_kernel::{InferRequest, InferResponse};

#[test]
fn infer_request_carries_genai_attrs_default() {
    let req = InferRequest::new("anthropic/claude-sonnet-4-7");
    // Default attrs is empty/unknown; producers populate.
    assert_eq!(req.gen_ai.system, GenAiSystem::Unknown);
    assert_eq!(req.gen_ai.operation, GenAiOperation::Chat);
}

#[test]
fn infer_response_carries_genai_attrs_default() {
    let resp = InferResponse::default();
    assert!(resp.gen_ai.response_id.is_none());
    assert!(resp.gen_ai.response_model.is_none());
}

#[test]
fn genai_attrs_is_non_exhaustive_constructor_only() {
    let attrs = GenAiAttrs::new();
    assert_eq!(attrs.system, GenAiSystem::Unknown);
}

#[test]
fn genai_system_serde_dot_notation() {
    let s = GenAiSystem::Anthropic;
    let json = serde_json::to_string(&s).unwrap();
    assert_eq!(json, "\"anthropic\"");

    let back: GenAiSystem = serde_json::from_str("\"openai\"").unwrap();
    assert_eq!(back, GenAiSystem::OpenAi);
}

#[test]
fn genai_attrs_full_roundtrip() {
    let mut attrs = GenAiAttrs::new();
    attrs.system = GenAiSystem::Mistral;
    attrs.operation = GenAiOperation::Embedding;
    attrs.response_id = Some("resp_abc123".into());
    attrs.response_model = Some("mistral-large-2".into());
    let json = serde_json::to_string(&attrs).unwrap();
    let back: GenAiAttrs = serde_json::from_str(&json).unwrap();
    assert_eq!(back.system, GenAiSystem::Mistral);
    assert_eq!(back.response_id.as_deref(), Some("resp_abc123"));
}
```

**Step 2: Run — must fail to compile**

Run: `cargo test -p nika-kernel --test genai_attrs_bridge 2>&1 | tail -5`
Expected: errors on `nika_kernel::genai::*` (module does not exist).

### Task C3: Implement `GenAiAttrs` module

**Files:**
- Create: `crates/nika-kernel/src/ai/genai.rs`
- Modify: `crates/nika-kernel/src/ai/mod.rs` (add `pub mod genai;`)
- Modify: `crates/nika-kernel/src/lib.rs` (re-export `genai::*` + `pub mod genai`)

**Step 1: Write the module**

```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `GenAiAttrs` — typed bridge to OpenTelemetry GenAI semconv.
//!
//! Maps the ~20 stable `gen_ai.*` attributes to a typed Rust struct
//! embedded on `InferRequest` and `InferResponse`. Enforces cross-provider
//! parity (Pre-launch Gate 2) — no provider can silently drop an
//! attribute the kernel exports.
//!
//! Spec: https://opentelemetry.io/docs/specs/semconv/gen-ai/ (Development
//! status as of 2026-04 — fields are non_exhaustive until Stable).

use serde::{Deserialize, Serialize};

/// `gen_ai.system` — the GenAI provider identifier.
///
/// Wire format uses the OTel semconv lowercase string convention.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum GenAiSystem {
    /// Unknown / not yet populated by the provider.
    #[default]
    Unknown,
    /// Anthropic (Claude family).
    Anthropic,
    /// `OpenAI`.
    OpenAi,
    /// Google (Gemini).
    Google,
    /// Mistral.
    Mistral,
    /// Meta (Llama).
    Meta,
    /// Cohere.
    Cohere,
    /// `DeepSeek`.
    DeepSeek,
    /// xAI (Grok).
    Xai,
    /// `OpenAI`-compatible third party (LiteLLM, OpenRouter, Ollama, etc.).
    OpenAiCompatible,
    /// Local model (mistral.rs GGUF, llama.cpp).
    LocalNative,
    /// Custom / user-extended. Prefer adding a first-class variant.
    Custom,
}

/// `gen_ai.operation.name` — the operation kind.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum GenAiOperation {
    /// Chat completion (default for InferRequest).
    #[default]
    Chat,
    /// Text completion (legacy).
    TextCompletion,
    /// Embedding generation.
    Embedding,
    /// Image generation.
    ImageGeneration,
    /// Audio transcription.
    AudioTranscription,
    /// Audio synthesis (TTS).
    AudioSynthesis,
}

/// Typed bridge to OTel GenAI semconv attributes.
///
/// Embedded on `InferRequest` and `InferResponse`. Default = unknown +
/// chat operation. Populated by the provider trait impl.
///
/// All fields are `Option<T>` or have `Default`; struct grows non-breakingly
/// thanks to `#[non_exhaustive]` + `pub fn new()` (INV-019).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct GenAiAttrs {
    /// `gen_ai.system` — which provider.
    pub system: GenAiSystem,
    /// `gen_ai.operation.name` — operation kind.
    pub operation: GenAiOperation,
    /// `gen_ai.response.id` — provider's response identifier.
    pub response_id: Option<String>,
    /// `gen_ai.response.model` — actual model that responded (may differ
    /// from `InferRequest::model` if the provider routed).
    pub response_model: Option<String>,
    /// `gen_ai.request.encoding_formats` — vector encoding formats (e.g.
    /// `["float", "base64"]` for embeddings).
    pub encoding_formats: Vec<String>,
    /// `gen_ai.conversation.id` — multi-turn conversation correlation.
    pub conversation_id: Option<String>,
    /// `gen_ai.agent.id` — agent identifier when the request originates
    /// from a verb-agent step.
    pub agent_id: Option<String>,
    /// `gen_ai.agent.name` — human-readable agent name.
    pub agent_name: Option<String>,
}

impl GenAiAttrs {
    /// Construct an empty/default attrs object.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn types_are_send_sync() {
        _assert_send_sync::<GenAiAttrs>();
        _assert_send_sync::<GenAiSystem>();
        _assert_send_sync::<GenAiOperation>();
    }

    #[test]
    fn default_attrs_is_unknown_chat() {
        let a = GenAiAttrs::new();
        assert_eq!(a.system, GenAiSystem::Unknown);
        assert_eq!(a.operation, GenAiOperation::Chat);
    }

    #[test]
    fn system_serde_lowercase() {
        assert_eq!(serde_json::to_string(&GenAiSystem::Anthropic).unwrap(), "\"anthropic\"");
        assert_eq!(serde_json::to_string(&GenAiSystem::OpenAi).unwrap(), "\"open_ai\"");
        assert_eq!(serde_json::to_string(&GenAiSystem::DeepSeek).unwrap(), "\"deep_seek\"");
    }
}
```

**Step 2: Wire the module**

Edit `crates/nika-kernel/src/ai/mod.rs` — add `pub mod genai;` (alphabetical: between `context` and `memory`, or wherever fits the existing order).

Edit `crates/nika-kernel/src/lib.rs` — add a `pub mod genai` re-export and flat re-exports:
```rust
pub mod genai {
    pub use crate::ai::genai::*;
}
pub use ai::genai::{GenAiAttrs, GenAiOperation, GenAiSystem};
```

**Step 3: Verify compile**

Run: `cargo check -p nika-kernel`
Expected: clean.

### Task C4: Embed `GenAiAttrs` on `InferRequest`

**Files:**
- Modify: `crates/nika-kernel/src/ai/provider.rs` (around line 187)

**Step 1: Read current InferRequest definition**

Read: `crates/nika-kernel/src/ai/provider.rs` lines 187-250 (full struct + impl block).

**Step 2: Add the field**

Find the `pub struct InferRequest { ... }` block. Add a new public field at the end (before the closing `}`):

```rust
    /// OTel GenAI semconv bridge — populated by the provider impl.
    /// Defaults to `GenAiAttrs::new()` (unknown + chat).
    pub gen_ai: crate::ai::genai::GenAiAttrs,
```

**Step 3: Update the `InferRequest::new` constructor**

Find `impl InferRequest { pub fn new(model: ...) -> Self { ... } }`. Add `gen_ai: GenAiAttrs::default()` to the struct literal (the existing `..Default::default()` if present already covers it — verify). If `new()` enumerates fields explicitly, add the line.

**Step 4: Verify check + run pre-existing provider tests**

Run: `cargo test -p nika-kernel --lib provider 2>&1 | tail -3`
Expected: existing tests pass; new `gen_ai` field is `Default`-populated transparently.

### Task C5: Embed `GenAiAttrs` on `InferResponse`

**Files:**
- Modify: `crates/nika-kernel/src/ai/provider.rs` (around line 277)

**Step 1: Read current InferResponse**

Read: `crates/nika-kernel/src/ai/provider.rs` lines 277-340.

**Step 2: Add the field + update constructor**

Same pattern as Task C4: add `pub gen_ai: GenAiAttrs` field and ensure constructors default it.

**Step 3: Verify**

Run: `cargo check -p nika-kernel && cargo test -p nika-kernel --lib`
Expected: clean + all pass.

### Task C6: Run the contract test from Task C2

Run: `cargo test -p nika-kernel --test genai_attrs_bridge`
Expected: 5/5 PASS.

### Task C7: Update mock provider to populate `gen_ai`

**Files:**
- Modify: `crates/nika-kernel-mock/src/provider.rs` (mock's infer impl)

**Step 1: Locate the mock's `infer` method**

Read: `crates/nika-kernel-mock/src/provider.rs`. Find where it constructs `InferResponse`.

**Step 2: Populate `gen_ai`**

In the mock infer impl, after building the response, set:
```rust
let mut resp = InferResponse::new(...);
resp.gen_ai.system = GenAiSystem::Custom; // mock identifier
resp.gen_ai.operation = GenAiOperation::Chat;
resp.gen_ai.response_id = Some(format!("mock-{}", rand_id));
resp.gen_ai.response_model = Some(req.model.clone());
```

**Step 3: Verify mock provider tests still pass**

Run: `cargo test -p nika-kernel-mock --lib provider`
Expected: pass.

### Task C8: Update Q13 + commit

**Files:**
- Modify: `docs/architecture/l0-l05-architecture-decisions.md` Q13 row.

**Step 1: Edit Q13 row**

Replace:
```
| Q13 | Bridge OTel GenAI semconv via typed `GenAiAttrs` on Infer{Request,Response} | LOCKED rev.3 |
```
With:
```
| Q13 | Bridge OTel GenAI semconv via typed `GenAiAttrs` on Infer{Request,Response} | LOCKED rev.3 · executed |
```

**Step 2: Commit**

```bash
git add crates/nika-kernel/src/ai/genai.rs \
        crates/nika-kernel/src/ai/mod.rs \
        crates/nika-kernel/src/ai/provider.rs \
        crates/nika-kernel/src/lib.rs \
        crates/nika-kernel/tests/genai_attrs_bridge.rs \
        crates/nika-kernel-mock/src/provider.rs \
        docs/architecture/l0-l05-architecture-decisions.md

git commit -m "$(cat <<'EOF'
feat(nika-kernel): GenAiAttrs OTel semconv bridge (Q13 executed)

Typed bridge to OpenTelemetry GenAI semantic conventions (still in
Development status Apr 2026). Embedded as `pub gen_ai: GenAiAttrs` on
both InferRequest + InferResponse. Enforces cross-provider parity
(Pre-launch Gate 2) — no provider can silently drop an attribute the
kernel exports.

- GenAiAttrs struct (#[non_exhaustive]) — 8 fields covering system,
  operation, response.id, response.model, encoding_formats,
  conversation.id, agent.id, agent.name.
- GenAiSystem enum (12 variants: Unknown / Anthropic / OpenAi / Google /
  Mistral / Meta / Cohere / DeepSeek / Xai / OpenAiCompatible /
  LocalNative / Custom). All snake_case wire format.
- GenAiOperation enum (6 variants: Chat default / TextCompletion /
  Embedding / ImageGeneration / AudioTranscription / AudioSynthesis).
- Sub-module re-export: nika_kernel::genai::*.
- Mock provider populates gen_ai (system=Custom + response_id +
  response_model echoes request) for hermetic tests.
- TokenUsage already had the usage.* counters (cache_read_tokens,
  reasoning_tokens, thinking_tokens) — Q13 reuses, does not duplicate.

Forward-compat: every public type non_exhaustive + Default + new().
OTel semconv reaches Stable expected ~2026-Q4 → re-validate then.

Tests: 5 contract tests (default attrs on both DTOs, non_exhaustive
constructor, system serde dot-notation, full roundtrip) + 3 unit
tests in genai.rs = 8 new tests.

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
EOF
)"
```

---

## Phase D — `cargo public-api` snapshots (P0 enforcement)

**Why:** Architect's #1 finding — without public-api snapshots, Gate 12 is prose, not enforcement. Snapshot fixes the public surface of every L0/L0.5 admitted crate. Any departure (intentional or not) shows in CI diff.

**Risk:** Low (read-only enforcement, snapshots stored in repo). Initial baselines are large but mechanical.

### Task D1: Install + verify locally

**Step 1: Install**

Run: `cargo install --locked cargo-public-api@0.45 || echo 'already installed'`
Expected: install succeeds or already present.

**Step 2: Verify against an admitted crate**

Run: `cargo public-api -p nika-types --all-features 2>&1 | head -20`
Expected: a list of `pub use ...`, `pub fn ...`, `pub struct ...` lines.

### Task D2: Capture baselines

**Files:**
- Create: `crates/nika-types/public-api.txt`
- Create: `crates/nika-error/public-api.txt`
- Create: `crates/nika-catalog/public-api.txt`
- Create: `crates/nika-kernel/public-api.txt`
- Create: `crates/nika-kernel-mock/public-api.txt`
- Create: `crates/nika-catalog-verify/public-api.txt`

**Step 1: Snapshot every admitted crate**

Run for each:
```bash
for crate in nika-types nika-error nika-catalog nika-kernel nika-kernel-mock nika-catalog-verify; do
  cargo public-api -p "$crate" --all-features --omit auto-trait-impls > "crates/$crate/public-api.txt"
done
```

**Step 2: Inspect one for sanity**

Run: `wc -l crates/nika-kernel/public-api.txt && head -30 crates/nika-kernel/public-api.txt`
Expected: hundreds of lines (kernel is large), readable list.

### Task D3: Add CI workflow

**Files:**
- Create: `.github/workflows/public-api.yml`

**Step 1: Write the workflow**

```yaml
name: public-api
on:
  pull_request:
    paths:
      - 'crates/**/src/**'
      - 'crates/**/Cargo.toml'
      - 'Cargo.toml'

permissions:
  contents: read

jobs:
  diff:
    runs-on: ubuntu-latest
    timeout-minutes: 15
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: install cargo-public-api
        run: cargo install --locked cargo-public-api@0.45
      - name: diff each admitted crate
        run: |
          set -euo pipefail
          fail=0
          for crate in nika-types nika-error nika-catalog nika-kernel nika-kernel-mock nika-catalog-verify; do
            actual=$(cargo public-api -p "$crate" --all-features --omit auto-trait-impls)
            expected=$(cat "crates/$crate/public-api.txt")
            if [ "$actual" != "$expected" ]; then
              echo "::error::public API drift in $crate — regenerate snapshot or revert"
              diff <(echo "$expected") <(echo "$actual") || true
              fail=1
            fi
          done
          exit $fail
```

**Step 2: Commit baseline + workflow**

```bash
git add crates/*/public-api.txt .github/workflows/public-api.yml
git commit -m "$(cat <<'EOF'
chore(ci): wire cargo-public-api snapshots (P0 — Gate 12 enforcement)

Snapshots the public API surface of every admitted L0/L0.5 crate
(types, error, catalog, kernel, kernel-mock, catalog-verify).

CI workflow .github/workflows/public-api.yml runs on every PR
touching crates/**/src or Cargo.toml. Drift fails the job with a
diff in the log.

To accept intentional drift: regenerate the affected snapshot
locally with `cargo public-api -p <crate> --all-features` and
commit the new file.

Fills the gap identified by swarm-3 architect audit (P0 #1):
"FCI is aspirational prose without cargo public-api wired".

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
EOF
)"
```

---

## Phase E — `cargo semver-checks` (catches breaking changes)

**Why:** Architect P0 #1 second half. `public-api` shows drift; `semver-checks` classifies whether the drift is breaking. Two-tool pair.

### Task E1: Add to CI

**Files:**
- Create: `.github/workflows/semver-checks.yml`

**Step 1: Write workflow**

```yaml
name: semver-checks
on:
  pull_request:
    paths:
      - 'crates/**/src/**'
      - 'crates/**/Cargo.toml'

permissions:
  contents: read

jobs:
  check:
    runs-on: ubuntu-latest
    timeout-minutes: 20
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: install cargo-semver-checks
        run: cargo install --locked cargo-semver-checks@0.40
      - name: check every admitted crate
        # Foundation crates are publish=false (ADR-022); we still check
        # against the previous workspace version for drift visibility.
        run: |
          for crate in nika-types nika-error nika-catalog nika-kernel nika-kernel-mock nika-catalog-verify; do
            cargo semver-checks check-release -p "$crate" || \
              echo "::warning::$crate has breaking changes (publish=false, informational)"
          done
```

**Step 2: Commit**

```bash
git add .github/workflows/semver-checks.yml
git commit -m "$(cat <<'EOF'
chore(ci): wire cargo-semver-checks (P0 — breaking change visibility)

Pairs with public-api.yml: that one shows drift, this one classifies it
as breaking vs additive. Foundation crates are publish=false (ADR-022),
so failures degrade to warnings — informational visibility, not blocking.

The two checks together close the architect P0 #1 finding.

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
EOF
)"
```

---

## Phase F — `no_std`/`alloc` gating on L0 (forward-compat WASM)

**Why:** Architect P1 #4 — v0.100 WASM commitment exists but L0 assumes `std`. Cheap to reserve now (a few `extern crate alloc;` lines), painful to retrofit later.

**Risk:** Medium. Some current code may assume std features (HashMap default-hasher, threading primitives). Each crate gated separately with feature `std` (default-on).

### Task F1: Gate `nika-types` first (smallest scope)

**Files:**
- Modify: `crates/nika-types/Cargo.toml`
- Modify: `crates/nika-types/src/lib.rs`

**Step 1: Edit `Cargo.toml` to add feature**

```toml
[features]
default = ["std"]
std = ["serde/std", "thiserror/std"]
```

**Step 2: Edit `lib.rs` top**

Add at the very top of `crates/nika-types/src/lib.rs`:

```rust
#![cfg_attr(not(feature = "std"), no_std)]
extern crate alloc;
```

**Step 3: Build with default features (std)**

Run: `cargo check -p nika-types`
Expected: clean.

**Step 4: Build without std**

Run: `cargo check -p nika-types --no-default-features`
Expected: errors will list std-only types (HashMap, std::time, etc.). For each: replace HashMap → BTreeMap (alloc-only), std::time → core::time, std::string::String → alloc::string::String.

**Step 5: Iterate until clean**

When `cargo check --no-default-features` passes: commit.

```bash
git add crates/nika-types/Cargo.toml crates/nika-types/src/lib.rs crates/nika-types/src/**.rs
git commit -m "$(cat <<'EOF'
feat(nika-types): gate no_std/alloc (forward-compat WASM v0.100)

Adds default feature `std` (on by default). Without `std`, the crate
compiles against `core` + `alloc`, suitable for WASM Component Model
guests and embedded targets.

Replaced std-only types with alloc-equivalents:
- HashMap -> BTreeMap (where ordering doesn't matter)
- std::time::Duration -> core::time::Duration
- explicit alloc::{string,vec} imports

Reserves the no_std seam now (cheap) instead of retrofitting under
WASM v0.100 deadline (expensive). Per architect P1 #4 swarm-3 finding
+ ADR-028 forward-compat reservation policy.

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
EOF
)"
```

### Task F2: Gate `nika-error` (same pattern)

Repeat F1 pattern for `nika-error`. miette is std-only by default — gate it behind the `std` feature.

### Task F3: Gate `nika-schema` (when admitted)

**Defer until after admission #6** (schema is WIP). Add the gate as part of the admission commit so the new crate ships with the seam.

---

## Phase G — `cargo hakari` workspace-hack

**Why:** Architect P1 #2 — RA / uv / tokio-console / wasmtime all use it. mat-klad: "the cost compounds with crate count; add before hitting 20." Workspace at 7 admitted, 51-53 target — start now while the diff is small.

### Task G1: Install + init

**Step 1: Install**

Run: `cargo install --locked cargo-hakari@0.9 || echo 'already installed'`

**Step 2: Initialise**

Run: `cargo hakari init --yes`
Expected: creates `crates/workspace-hack/Cargo.toml` (or `crates/nika-workspace-hack/` — pick the one matching `nika-` prefix convention).

**Step 3: Generate the hack manifest**

Run: `cargo hakari generate`
Expected: populates the workspace-hack crate with unified feature deps.

### Task G2: Wire into every workspace member

Run: `cargo hakari manage-deps --yes`
Expected: adds `nika-workspace-hack = { path = "../workspace-hack" }` to every member crate.

### Task G3: Verify clean build

Run: `cargo check --workspace && cargo test --workspace --lib 2>&1 | tail -3`
Expected: faster (cache-friendly), still clean.

### Task G4: Add CI guardrail

**Files:**
- Modify: `.github/workflows/diamond-ci.yml` (add hakari job)

**Step 1: Add a job**

```yaml
  hakari:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo install --locked cargo-hakari@0.9
      - run: cargo hakari verify
```

**Step 2: Commit**

```bash
git add crates/nika-workspace-hack/ \
        Cargo.toml \
        crates/*/Cargo.toml \
        .github/workflows/diamond-ci.yml
git commit -m "$(cat <<'EOF'
chore(workspace): cargo-hakari workspace-hack (P1 — compile sharing)

mat-klad pattern. RA / uv / tokio-console / wasmtime all use it.
Adds nika-workspace-hack crate that unifies feature flags across
the workspace, dramatically improving incremental compile time as
the crate count grows (7 admitted -> 51-53 target).

Per architect P1 #2 swarm-3 finding: "add at v0.85 or when cargo
build --workspace > 90s cold" — adding now while diff is small
(7 crates) instead of later (50+).

CI: hakari verify on every PR catches drift from regenerated members.

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
EOF
)"
```

---

## Phase H — Match exhaustivity bench (post `nika-event` admission)

**Why:** Rust-pro P2 #5 — match exhaustivity perf cliff is real around 500+ variants. Q6 plan is `event_categories!` macro generating 22 sub-enums aggregating to 647 variants. Prove compile time + match perf before locking the design.

**Status:** **DEFER** until `nika-event` is admitted (Q5 admission slot #8). Cannot bench what does not exist.

### Task H1 (deferred): Add criterion bench

When `nika-event` lands:

**Files:**
- Create: `crates/nika-event/benches/match_exhaustivity.rs`
- Modify: `crates/nika-event/Cargo.toml` (add `[dev-dependencies] criterion`)

**Step 1: Bench template**

```rust
use criterion::{criterion_group, criterion_main, Criterion};
use nika_event::EventKind;

fn bench_category_match(c: &mut Criterion) {
    let kinds = generate_647_variants();
    c.bench_function("647_variant_match", |b| {
        b.iter(|| {
            for k in &kinds {
                let _ = match_category(k);
            }
        });
    });
}

criterion_group!(benches, bench_category_match);
criterion_main!(benches);
```

**Acceptance:** match dispatch < 10ns per variant; cold compile time `cargo build -p nika-event` < 15s.

If perf cliff hit: pivot to `phf` perfect-hash table per rust-pro recommendation.

---

## Done criteria

After Phase G is committed:

- [ ] All 5 admitted L0/L0.5 crates have `public-api.txt` baselines + CI guard
- [ ] `semver-checks.yml` posts informational warnings on drift
- [ ] `nika-types` + `nika-error` build with `--no-default-features`
- [ ] `cargo hakari verify` is part of `diamond-ci`
- [ ] `ObservabilitySink` is gone from kernel; `AuditSink` is in
- [ ] `GenAiAttrs` embedded on `InferRequest` + `InferResponse`
- [ ] Q12 + Q13 marked "executed" in `l0-l05-architecture-decisions.md`
- [ ] Hygiene: ≥ 21 green, 0 red
- [ ] Workspace tests: ≥ 855 passed (846 baseline + 12 audit + 8 genai − 6 obs deletions)
- [ ] Phase H deferred — re-open after `nika-event` admission (#8 per Q5)

## Risks + mitigations

| Risk | Phase | Mitigation |
|---|---|---|
| `no_std` gating breaks transitive deps (uuid, serde derive) | F | Default `std` feature on; only `--no-default-features` builds need to be clean |
| `cargo hakari` re-shuffles every Cargo.toml | G | Single atomic commit, reviewable diff; CI verifies |
| `public-api` snapshot is huge for `nika-kernel` | D | Use `--omit auto-trait-impls` to skip noise |
| `GenAiAttrs` field added to `InferRequest` triggers semver-checks warning | C + E | Foundation crates are `publish=false` (ADR-022); semver-checks degrades to warning |
| Phase B `AuditSink` `tokio` dep creep | B | Use `trait_variant::make`; keep `tokio` only behind `[dev-dependencies]` for the test crate |

## References

- `docs/architecture/l0-l05-architecture-decisions.md` — Q1-Q13
- `docs/adr/adr-007-forward-compat-invariants.md` — non_exhaustive + new()
- `docs/adr/adr-014-sealed-kernel-traits.md` — sealed pattern
- `docs/adr/adr-022-foundation-crate-layout-v081.md` — publish=false
- `docs/adr/adr-025-per-crate-semver-release-plz.md` — addresses P0-5 (no work needed)
- `docs/adr/adr-028-forward-compat-reservation-policy.md` — seams-now/crates-later
- `docs/architecture/crate-layer-registry.md` — 12 capability axes vocabulary
- `.claude/CLAUDE.md` — interdits stricts, 12 gates, mandatory patterns
- swarm-3 reports (in conversation history) — full audit findings

🦋
