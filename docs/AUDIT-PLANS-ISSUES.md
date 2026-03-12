# Audit Report: docs/plans/ Inconsistencies

**Date:** 2026-03-12
**Auditor:** Claude
**Current Nika Version:** v0.27.0 (as of Cargo.toml)
**Status:** CRITICAL ISSUES FOUND

---

## Summary

The `docs/plans/` directory contains **multiple outdated plan documents** that reference:
- **Removed features** (Ollama, old TUI architectures)
- **Wrong version references** (plans for v0.9.x, v0.10.x targeting features already in v0.22+)
- **Completed work marked as "in progress"** (TUI v0.22 Runner refactor is actually COMPLETE)
- **Contradictory view counts** (8 views → 5 views → actual 4 views in v0.22+)
- **Orphaned v0.9.1/ subdirectory** (32 files planning features from 2+ years of work ago)

---

## Critical Issues

### Issue #1: TUI View Count Mismatch

**Severity:** HIGH
**Files affected:**
- `/Users/thibaut/dev/supernovae/nika/docs/plans/v0.9.1/README.md` (lines 52-113)
- `/Users/thibaut/dev/supernovae/nika/docs/plans/2026-03-05-tui-restructuration-5-views.md` (lines 15-68)

**Problem:**
- v0.9.1/README.md describes 6-VIEW architecture: Explorer, Chat, Editor, Runner, Scheduler, Settings
- tui-restructuration-5-views.md (dated 2026-03-05) plans to reduce from 8 views to 5 views
- **Actual code in v0.27.0 has 4 VIEWS:**
  ```
  - Studio (src/tui/views/studio.rs)
  - Monitor/Runner (src/tui/views/monitor.rs)
  - Chat (src/tui/views/chat/)
  - Settings (src/tui/views/settings.rs)
  - Help (src/tui/views/help.rs)
  - Home (src/tui/views/home.rs)
  - Split (src/tui/views/split.rs) - LEGACY
  - Wizard (src/tui/views/wizard.rs)
  ```

**Explanation:**
The plans reference a 6-view future architecture (Explorer/Scheduler) that was never implemented. Current implementation settled on 4 main views (Studio/Runner/Chat/Settings) with legacy views still in codebase but not exposed in main routing.

**Action:**
- [ ] Delete or archive v0.9.1/ subdirectory (32 orphaned files)
- [ ] Update tui-restructuration-5-views.md to reflect actual 4-view architecture
- [ ] Remove references to planned Scheduler and Explorer views

---

### Issue #2: TUI v0.22 Runner Refactor Status

**Severity:** MEDIUM
**File:** `/Users/thibaut/dev/supernovae/nika/docs/plans/2026-03-08-tui-v0.22-runner-refactor.md`

**Problem:**
Line 3: **Status: 🟡 IN PROGRESS**

But the implementation is **COMPLETE**:
```
✅ Verified files exist (created 2026-03-08):
  - src/tui/widgets/panels/mod.rs (50 lines)
  - src/tui/widgets/panels/browser.rs (16,780 bytes)
  - src/tui/widgets/panels/task_list.rs (9,276 bytes)
  - src/tui/widgets/panels/task_flow.rs (13,312 bytes)
  - src/tui/widgets/panels/info.rs (14,625 bytes)
✅ MonitorView refactored to use shared panels (Phase 5)
✅ StudioView updated to use BrowserPanel (Phase 6)
✅ Tests wired (Phase 7)
```

**Action:**
- [ ] Change line 3 to: `**Status:** ✅ COMPLETED`
- [ ] Add completion date: `**Completed:** 2026-03-08`
- [ ] Move to `docs/completed/` or mark as archived

---

### Issue #3: Removed Feature References (Ollama)

**Severity:** HIGH
**Files affected:**
- `/Users/thibaut/dev/supernovae/nika/docs/plans/v0.9.1/2026-02-24-builtin-tools-spec.md` (line: unclear)
- Multiple files in v0.9.1/ subdirectory

**Problem:**
Ollama was **removed in v0.27.0** (replaced with native inference via mistral.rs).

v0.27.0 CLAUDE.md clearly states:
```
- **Ollama Removed** — Use `provider: native` with mistral.rs instead
```

Yet v0.9.1 plans still reference it as active provider. Example from CLAUDE.md v0.27.0:
```
Ollama (ollama) removed in v0.27 — use `provider: native` with local GGUF models instead.
```

**Action:**
- [ ] Delete v0.9.1/ subdirectory (32 files, all outdated)
- [ ] Any modern plans should reference `provider: native` for local inference, NOT Ollama

---

### Issue #4: Version References to v0.8.x, v0.9.x in 2025-2026 Plans

**Severity:** MEDIUM
**Files affected:**
- `/Users/thibaut/dev/supernovae/nika/docs/plans/2025-02-24-agent-steps-wiring-plan.md` (line 1: `**Version:** v0.8.1`)
- `/Users/thibaut/dev/supernovae/nika/docs/plans/2025-02-24-agent-streaming-design.md` (line 1: `**Version:** v0.8.1`)
- `/Users/thibaut/dev/supernovae/nika/docs/plans/2025-02-24-agent-streaming-implementation.md` (line 1: `**Version:** v0.8.1`)

**Problem:**
These are dated 2025-02-24 but reference v0.8.1. Current version is **v0.27.0** (v0.19 higher than v0.8.1).

Unclear status:
- Were these plans completed and the code moved to v0.27.0?
- Are these outdated plans that should be deleted?
- Should they be updated to reference v0.27.0?

**Action:**
- [ ] Review and update version references OR
- [ ] Move to `docs/archived/` with clear reasoning why they're outdated

---

### Issue #5: Obsolete TUI Architecture Documentation

**Severity:** MEDIUM
**Directory:** `/Users/thibaut/dev/supernovae/nika/docs/plans/v0.9.1/` (32 files)

**Files in this directory:**
```
- 2026-02-24-builtin-tools-spec.md
- 2026-02-24-chat-dag-implementation-plan.md
- 2026-02-24-chat-workflow-conversion.md
- 2026-02-24-cosmic-theme-wiring-plan.md
- 2026-02-24-gap-analysis.md
- 2026-02-24-memory-and-agents-design.md
- 2026-02-24-nika-project-structure.md
- 2026-02-24-v091-consolidated-design.md
- 2026-02-24-v091-master-plan.md
- 2026-02-24-v091-stable-graph-migration-spec.md
- 2026-02-24-v091-unstable-arc-design.md
- 2026-02-24-v091-vs-v090-deltas.md
- 2026-02-25-cosmic-theme-wiring-plan.md
- ASYNC-PATTERNS-QUICK-REF.md
- ASYNC-REVIEW.md
- README-ASYNC-REVIEW.md
- README.md
- VIEW-ARCHITECTURE.md
- v0.9.1-ChatWorkflow.md
- v0.9.2-MentionBindings.md
- [+ 12 more files]
```

**Problem:**
All dated 2026-02-24 but reference **v0.9.1 architecture** which was:
- Never fully implemented (6-view planned, not built)
- Superseded by v0.22+ (4-view actual implementation)
- Contains plans for features like `StableGraph` Chat DAG that don't match current architecture

**Evidence of supersession:**
- v0.9.1/README.md talks about "Chat-as-DAG using StableGraph"
- Current v0.27.0 uses `petgraph::StableGraph` in some modules but not for Chat DAG
- Actual Chat is in `src/tui/views/chat/` (not as DAG nodes)

**Action:**
- [ ] Delete entire `/Users/thibaut/dev/supernovae/nika/docs/plans/v0.9.1/` directory
- [ ] If historical value exists, move to `docs/archived/v0.9-planning/` with README explaining "These plans were precursors to v0.22+ architecture"

---

### Issue #6: Binding System v2 Status Contradiction

**Severity:** LOW
**File:** `/Users/thibaut/dev/supernovae/nika/docs/plans/2026-03-05-binding-system-v2-master.md`

**Problem:**
Line 6: "Enhance Nika's binding system with implicit output references, agent namespaces, and persistent datastores across **3 minor releases**."

But checking current code status:
- **v0.21.0 (Feature A - Implicit Output Reference)**: Lines 20-26 show `$task` → `task.output` syntax
  - **Status unclear** — need to verify if this is in v0.27.0 codebase
  - v0.27.0 CLAUDE.md doesn't mention `$task` syntax changes

- **v0.22.0 (Feature C - Agent Output Namespaces)**: Not mentioned in v0.27.0 CLAUDE.md

- **v0.23.0 (Feature B - Persistent Datastore)**: Not in v0.27.0

**Recommendation:**
- [ ] Verify if `$task` syntax is actually implemented in current codebase
- [ ] If implemented, update plan status to ✅ COMPLETED with version notes
- [ ] If NOT implemented, either:
  - Defer to future release and update version target, OR
  - Remove from planning (deprioritized)

---

### Issue #7: v0.4 Deprecated Removal Plan Status

**Severity:** MEDIUM
**File:** `/Users/thibaut/dev/supernovae/nika/docs/plans/2026-02-19-v04-deprecated-removal.md`

**Problem:**
Line 4: **Status: In Progress**

But current v0.27.0 codebase shows:
```
✅ COMPLETED: All files marked for deletion in v0.4 are actually GONE:
  - src/provider/claude.rs — DELETED
  - src/provider/openai.rs — DELETED
  - src/provider/types.rs — DELETED
  - src/runtime/agent_loop.rs — DELETED
✅ RigAgentLoop fully migrated and in use
✅ Test suite updated
```

**Verification:**
v0.27.0 CLAUDE.md v0.4 section explicitly lists these as "Removed in v0.4":
```rust
pub enum ClaudeProvider deleted
pub enum OpenAIProvider deleted
runtime/agent_loop.rs (717 lines) deleted
```

**Action:**
- [ ] Change status to ✅ COMPLETED
- [ ] Add note: "Completed in v0.4. v0.27.0 has zero deprecated code remaining."
- [ ] Move to `docs/completed/`

---

### Issue #8: Conflicting View Count Planning

**Severity:** MEDIUM
**Files:**
- `v0.9.1/README.md` — plans for 6 views (lines 52-68)
- `2026-03-05-tui-restructuration-5-views.md` — plans to go from 8→5 views (lines 15-68)
- `2026-03-08-tui-v0.22-runner-refactor.md` — actually implements 4-view architecture

**Timeline of confusion:**
```
v0.9.1 (2026-02-24): Plan for 6-view architecture
  ↓
tui-restructuration-5-views (2026-03-05): Plan to consolidate to 5 views
  ↓
tui-v0.22-runner-refactor (2026-03-08): Actually implemented as 4-view + refactored components
```

**What's actually in v0.27.0 code:**
```
- HomeView (deprecated, legacy)
- ChatView (primary for conversational agent)
- StudioView (primary for editing)
- MonitorView/RunnerView (primary for execution)
- SettingsView (primary for config)
- HelpView (utility)
- SplitView (legacy, unused)
- WizardView (utility for setup)
```

Main navigation exposes: **4 views** (Studio, Runner, Chat, Settings)

**Action:**
- [ ] Consolidate all TUI planning docs into single master plan
- [ ] Document actual 4-view architecture as v0.22+ baseline
- [ ] Remove contradictory "5-view" and "6-view" planning documents

---

## Summary of Actions

| Priority | Category | Count | Action |
|----------|----------|-------|--------|
| 🔴 CRITICAL | Removed features | 1 | Delete v0.9.1/ (32 orphaned files) |
| 🔴 CRITICAL | False status | 2 | Mark v0.22 runner + v0.4 removal as COMPLETED |
| 🟡 HIGH | Version references | 3 | Update or archive v0.8.x-era plans |
| 🟡 HIGH | Ollama refs | Multiple | Remove Ollama references (replaced by native inference) |
| 🟠 MEDIUM | Contradictions | 2 | Consolidate conflicting TUI architecture docs |
| 🟠 MEDIUM | Unclear status | 1 | Verify `$task` binding syntax implementation |

---

## Recommendations

### Immediate (v0.27.0)
1. Delete `/Users/thibaut/dev/supernovae/nika/docs/plans/v0.9.1/` entirely
   - 32 files, all outdated, no value in codebase
   - Create historical archive if needed

2. Update status in `2026-03-08-tui-v0.22-runner-refactor.md`:
   - Line 3: Change to `**Status:** ✅ COMPLETED (2026-03-08)`

3. Update status in `2026-02-19-v04-deprecated-removal.md`:
   - Line 4: Change to `**Status:** ✅ COMPLETED (v0.4)`

4. Delete or archive `2026-03-05-tui-restructuration-5-views.md`:
   - This plan was superseded by actual v0.22 implementation
   - If keeping for historical reference, move to `docs/archived/`

### Short-term (Next Release)
1. Verify `$task` syntax in current codebase
   - If implemented: Update `2026-03-05-binding-system-v2-master.md` to mark v0.21 as completed
   - If not: Deprioritize or update target version

2. Review and update all v0.8.x-era plans (2025-02-24):
   - Either update to v0.27.0 context or archive
   - Clarify if these were completed (features absorbed into v0.27.0)

3. Create `docs/PLANS.md` master index:
   - List active plans with status
   - List archived/completed plans
   - Prevent confusion in future

### Long-term
1. Establish plan review process:
   - Quarterly audit of docs/plans/
   - Mark completed plans immediately
   - Remove plans older than 3 versions (currently would remove v0.24 and earlier)

2. Migrate planning to issue tracker:
   - Some of these plans are large enough to warrant tracking
   - Could use GitHub Projects/Issues instead of standalone markdown

---

## Files to Delete

```
/Users/thibaut/dev/supernovae/nika/docs/plans/v0.9.1/  (entire 32-file directory)
```

## Files to Update

```
/Users/thibaut/dev/supernovae/nika/docs/plans/2026-03-08-tui-v0.22-runner-refactor.md (line 3)
/Users/thibaut/dev/supernovae/nika/docs/plans/2026-02-19-v04-deprecated-removal.md (line 4)
```

## Files to Archive/Delete

```
/Users/thibaut/dev/supernovae/nika/docs/plans/2026-03-05-tui-restructuration-5-views.md
/Users/thibaut/dev/supernovae/nika/docs/plans/2025-02-24-agent-steps-wiring-plan.md
/Users/thibaut/dev/supernovae/nika/docs/plans/2025-02-24-agent-streaming-design.md
/Users/thibaut/dev/supernovae/nika/docs/plans/2025-02-24-agent-streaming-implementation.md
```

---

**Report Generated:** 2026-03-12
**Auditor:** Claude
**Status:** READY FOR REVIEW
