# Plan Corrections — Amendments After Independent Review

> **Status:** Active amendments to docs 00-11. Apply alongside the original documents.
> **Source:** [`12-review-synthesis.md`](12-review-synthesis.md) — independent review by `feature-dev:code-reviewer`, 2026-04-10.

## Amendment policy

This document is **additive**. Docs 00-11 are NOT rewritten. Instead, each amendment below specifies (a) the affected document, (b) the contradiction/gap, (c) the authoritative correction.

**Conflict rule:** where this document contradicts docs 00-11, **this document wins.**

---

## AMEND-1 — Golden e2e test suite is a mandatory Session 12 deliverable

**Affected docs:** 00, 06, 08, 09, 10
**Status:** BLOCKING for S13 deletion commits

**Change:** add a new commit `S12-F11` at the end of Session 12's foundation phase:

### S12-F11 — `test(runtime): golden e2e regression tests for all 5 verbs`

**Files:**
- Create: `tools/nika/tests/golden_verbs.rs` (new integration test file)
- Create: `tools/nika/tests/fixtures/golden/*.nika.yaml` (5 workflow fixtures, one per verb)
- Create: `tools/nika/tests/fixtures/golden/*.expected.json` (5 output snapshots)

**Strategy:** use `insta` for snapshot testing. Each test runs a small workflow through `Runner::run` with `provider: mock`, asserts output content, and asserts event sequence via `EventLog::snapshot`.

**Example fixture:**

```yaml
# tools/nika/tests/fixtures/golden/exec_hello.nika.yaml
schema: "nika/workflow@0.12"
workflow: golden-exec-hello
tasks:
  - id: greet
    exec:
      command: "echo hello golden"
```

**Example test:**

```rust
// tools/nika/tests/golden_verbs.rs
use nika_engine::runtime::runner::Runner;

#[tokio::test]
async fn golden_exec_hello() {
    let runner = Runner::test_default().await.expect("runner init");
    let result = runner.run_file("tests/fixtures/golden/exec_hello.nika.yaml")
        .await
        .expect("run success");
    insta::assert_yaml_snapshot!("exec_hello_output", result.task_output("greet"));
    insta::assert_yaml_snapshot!("exec_hello_events", result.event_snapshot());
}
```

**5 verbs × 1 fixture each = 5 new tests. Expected count after S12-F11: ~10,800 tests.**

**TDD flow:**
1. Write the test referencing a `Runner::test_default` that does not exist
2. Compile fails (RED)
3. Add `Runner::test_default` as a cfg-test constructor in `nika-engine::runtime::runner`
4. Write the first fixture + assertion
5. Run, capture the initial snapshot with `cargo insta accept`
6. Commit snapshot files alongside code

**Verification:**
```bash
cargo test -p nika --test golden_verbs 2>&1 | tail -10
# EXPECTED: 5 passed
```

**Commit message:**
```
test(runtime): golden e2e regression tests for all 5 verbs (S12-F11)

Creates the regression oracle for Sessions 13/14 verb extraction. Each
of the 5 verbs (exec, fetch, infer, invoke, agent) has a minimal
workflow fixture executed through Runner::run with provider:mock.
Tests assert output content and EventLog event sequence via insta
snapshots.

This is the safety net for verb extraction: any deletion commit in S13
or S14 that alters behavior will fail these tests.

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

**Impact on Session 12 plan:** commit count 10 → **11**. Total S12 budget 4h → **5h**.

---

## AMEND-2 — Per-verb Caps structs live in nika-kernel, not nika-runtime

**Affected docs:** 06, 07, 01 (architecture vision), 03 (ADR-002)
**Status:** BLOCKING for S13 start

**The contradiction:**
- Doc 06 defines `ExecCaps` in `nika-kernel/src/caps.rs` with `policy` as `&dyn PolicyChecker` (trait object)
- Doc 07 redefines `ExecCaps` in `nika-runtime/src/capabilities.rs` with `policy` as `&PolicyEnforcer` (concrete)

**Authoritative resolution:** the doc 06 definition wins. Per-verb Caps structs live in **`nika-kernel/src/caps.rs`** with **trait object** fields for capabilities.

### Canonical ExecCaps definition

```rust
// tools/nika-kernel/src/caps.rs

use std::path::Path;
use tokio_util::sync::CancellationToken;
use crate::shell::ShellExecutor;
use crate::policy::PolicyChecker;
use crate::clock::Clock;

#[non_exhaustive]
pub struct ExecCaps<'a> {
    pub shell: &'a dyn ShellExecutor,
    pub policy: &'a dyn PolicyChecker,
    pub events: &'a nika_event::EventLog,
    pub clock: &'a dyn Clock,
    pub shield: &'a nika_shield::ShieldContext,
    pub cancel: &'a CancellationToken,
    pub workflow_base_dir: &'a Path,
    pub default_cwd: Option<&'a Path>,
}
```

### VerbCapabilities accessor construction (in nika-runtime)

The exec_caps accessor constructs a borrowed slice from the run-scoped bundle:

```rust
// tools/nika-runtime/src/capabilities.rs — accessor shape

impl VerbCapabilities {
    pub fn exec_caps(&self) -> ExecCaps<'_> {
        ExecCaps {
            shell: &*self.shell,
            policy: &*self.policy_enforcer,
            events: &self.events,
            clock: &*self.clock,
            shield: &self.shield,
            cancel: &self.cancel_token,
            workflow_base_dir: &self.workflow_base_dir,
            default_cwd: self.default_cwd.as_deref(),
        }
    }
}
```

**Policy lock safety note:** `self.policy_enforcer` should be `Arc<PolicyEnforcer>` (not `Arc<RwLock<PolicyEnforcer>>`) because mutation of `allowed_hosts` only happens during construction. If runtime mutation is needed, use `Arc<RwLock<...>>` but verb bodies MUST NOT hold the read guard across any `.await` — `parking_lot::RwLockReadGuard` is not `Send`.

### Apply this correction:

- **Doc 06 Commit S12-F9:** keep as-is (defines the Caps structs in nika-kernel with trait objects). No change.
- **Doc 07 commits referencing ExecCaps:** replace "in nika-runtime/src/capabilities.rs" with "re-exported from nika-kernel via `pub use nika_kernel::caps::*;`". Verb crates `use nika_kernel::caps::ExecCaps`.
- **Doc 01 architecture vision:** the VerbCapabilities sketch already uses trait objects — no change needed. Just clarify that Caps types live in nika-kernel and accessor methods live on VerbCapabilities in nika-runtime.

---

## AMEND-3 — ADR-004 call site count corrected: 107, not 15

**Affected docs:** 05 (ADR-004), 00 (mega plan), 08 (Session 14), 09 (risk register)
**Status:** BLOCKING for S14 Wave C budget

**Verified count (2026-04-10 via grep):**

| File | TaskExecutor constructor calls |
|---|---|
| `nika-engine/src/runtime/executor/tests.rs` | 88 |
| `nika-engine/src/runtime/runner/mod.rs` | 2 |
| `nika-engine/src/runtime/executor/tests_wiremock.rs` | 2 |
| `nika-engine/src/runtime/executor/tests_shield_e2e.rs` | 1 |
| `nika-engine/src/runtime/executor/tests_shield_spotlight.rs` | 1 |
| `nika-engine/src/runtime/executor/tests_shield_agent_restrict.rs` | 1 |
| `nika-engine/src/runtime/executor/tests_shield_canary.rs` | 1 |
| `nika/tests/executor_exec_errors_test.rs` | 1 |
| `nika/tests/executor_fetch_errors_test.rs` | 1 |
| `nika/tests/executor_infer_errors_test.rs` | 2 |
| `nika/tests/decompose_test.rs` | 1 |
| `nika/tests/fetch_wiremock_test.rs` | 1 |
| `nika/tests/builtin_integration_test.rs` | 2 |
| `nika-cli/src/bench.rs` | 1 |
| `nika-cli/src/verbs.rs` | 1 |
| `nika/CHANGELOG.md` | 1 (documentation mention, not code) |
| **Total code sites** | **~106** |

### Corrections to ADR-004

Replace the "~15 call sites" claim with:

> **Call site audit (verified 2026-04-10):** 106 production + test sites across 15 code files. Breakdown:
> - **Production code (4 sites):** `nika-engine::runtime::runner::mod.rs` (2), `nika-cli::bench.rs` (1), `nika-cli::verbs.rs` (1)
> - **Engine tests (94 sites):** `executor/tests.rs` (88), `tests_wiremock.rs` (2), `tests_shield_*.rs` (4)
> - **Integration tests in `nika` crate (8 sites):** across 6 test files in `nika/tests/`
>
> The 4 production sites are the true "migration" work. The 102 test sites require a separate test-migration strategy.

### Corrections to Session 14 plan (doc 08)

**Wave C budget:** was 2-3 hours, revised to **5-8 hours** (absorbing the test migration prerequisite).

Add a new prerequisite commit **W14-A0** at the start of Wave A:

#### W14-A0 — `refactor(tests): migrate TaskExecutor-direct tests to Runner-based`

**Scope:** 102 test sites across 12 test files.

**Strategy:** introduce a `TestRunner` helper in `nika-engine::runtime::runner::test_helpers`. Each test replaces direct TaskExecutor construction with `TestRunner::new`, which internally builds a `Runner` with mock capabilities.

**Migration mechanics:**
1. Add `TestRunner` helper in a new file `tools/nika-engine/src/runtime/runner/test_helpers.rs`
2. For each test file, replace direct TaskExecutor construction with `TestRunner::new` via find-and-replace
3. Verify each test still asserts the same behavior
4. Run `cargo test --workspace --lib` after each test file migration

**Estimated effort:** 3-4 hours of mechanical work. Budget this separately from Wave A kernel trait enrichment.

**Revised Session 14 budget:**

| Wave | Commits | Old budget | New budget |
|---|---|---|---|
| W14-A0 (NEW) — test migration prerequisite | 1 | — | 3-4h |
| Wave A — kernel prerequisites | 4 | 3-4h | 3-4h |
| Wave B1 — verb-infer | 5 | 4-5h | 4-5h |
| Wave B2 — verb-agent | 4 | 4-5h | 4-5h |
| Wave C — dissolution | 5 | 2-3h | 2-3h |
| Wave D — close | 2 | 1h | 1h |
| **Total** | **21** (was 20) | **14-18h** | **17-22h** |

**Revised Session 14 window:** 2026-04-14 → **2026-04-14/15** (spill into day 2 expected).

---

## AMEND-4 — dispatch function is parallel, not live, during Session 13

**Affected docs:** 07 (Session 13), 09 (risk register)
**Status:** ADVISORY, affects test gating strategy

**Authoritative answer:** during Session 13, **`nika-engine::task_dispatch` remains the live code path**. `nika-runtime::dispatch` is constructed in parallel but is NOT called by the Runner yet. The wiring switch happens in Session 14 Wave C commit W14-D1.

### Apply to doc 07:

Add a callout box at the top of Session 13 Part 1:

> **Dispatch strategy during Session 13:**
> - `nika-runtime::dispatch` is created with 5 arms but is NOT the live code path during S13.
> - The existing `nika-engine::task_dispatch` continues to call TaskExecutor verb methods.
> - Each verb method on TaskExecutor becomes a bridge that delegates to the corresponding `nika_verb_*::run` function (as the verb crates are extracted).
> - Session 14 Wave C commit W14-D1 is where Runner stops using TaskExecutor and starts calling `nika_runtime::dispatch` directly.
> - This means `todo!` arms in `nika-runtime::dispatch` during S13 are safe — they are never called.

### Apply to doc 09 risk register:

Add a new risk entry:

> **R13-6 — dispatch stub arms silently diverge from TaskExecutor bridge** (P2 × M)
>
> **Description:** Session 13 builds `nika-runtime::dispatch` in parallel with the bridge pattern. If the dispatch stub arms for Infer and Agent are filled in S13 (out of scope, error), they could diverge from what TaskExecutor is doing.
>
> **Mitigation:** Session 13 dispatch arms for Exec/Invoke/Fetch are filled. Infer and Agent remain `todo!` until Session 14 Wave B. This is explicitly documented in doc 07.

---

## AMEND-5 — extract.rs purity verified

**Affected docs:** 06 (S12 commit F7), 11 (kernel audit)
**Status:** RESOLVED — no action needed except documentation

**Verification (2026-04-10 via grep):**

```
grep -c "use crate::" tools/nika-engine/src/runtime/executor/extract.rs
# Result: 2

grep -n "reqwest" tools/nika-engine/src/runtime/executor/extract.rs
# Result: empty

grep -n ".get(" tools/nika-engine/src/runtime/executor/extract.rs
# Result: all matches are serde_json Value .get in tests, not HTTP GETs
```

**Conclusion:** `extract.rs` is genuinely pure. No `reqwest` calls. No secondary HTTP. Only 2 `use crate::` imports (minimal, mechanical fix during the move). The verbatim extraction to `nika-extract` L2 crate in Session 12 commit F7 is safe.

No doc changes required. This amendment is informational only.

---

## Summary of amendments

| # | Amendment | Severity | Doc(s) affected | Status |
|---|---|---|---|---|
| AMEND-1 | Golden tests as S12-F11 commit | BLOCKING | 00, 06, 08, 09, 10 | Action required |
| AMEND-2 | Caps structs in nika-kernel with trait objects | BLOCKING | 01, 03, 06, 07 | Action required |
| AMEND-3 | 107 call sites (not 15); W14-A0 prerequisite | BLOCKING | 05, 00, 08, 09 | Action required |
| AMEND-4 | dispatch is parallel during S13 | ADVISORY | 07, 09 | Documentation update |
| AMEND-5 | extract.rs purity verified | RESOLVED | 06, 11 | None (informational) |

## Total session budget with amendments

| Session | Original budget | With amendments | Reason |
|---|---|---|---|
| Session 12 | 4h / 10 commits | **5h / 11 commits** | +S12-F11 golden tests |
| Session 13 | 10-12h / 15-18 commits | **10-12h / 15-18 commits** | No change |
| Session 14 | 14-18h / 20 commits | **17-22h / 21 commits** | +W14-A0 test migration |
| **Total** | **28-34h / 45-48 commits** | **32-39h / 47-50 commits** | |

**Launch gate impact:** Session 14 window slips from 2026-04-14 to 2026-04-14/15 (+1 day into Phase 15 buffer). Still ships before 2026-05-05 launch gate with ~18 days of polish time.

---

## Execution gate

Session 12 commits S12-F1 through S12-F11 are cleared to proceed **after this amendment document is committed** alongside the original 12 docs. The plan is ready for execution with these corrections applied.
