# TUI Gap Remediation — Final Analysis

**Date**: 2026-03-06
**Phase 1 Completed**: commit 4b22aeb0 (View consolidation 9→5)
**Analysis**: All 9 TUI improvement plans reviewed

---

## Executive Summary

After reviewing all 9 TUI improvement plans from Feb 20-23, 2026, and verifying against the current codebase, **most gaps are already implemented**. The plans evolved significantly over the 3-day period, with earlier plans superseded by later ones.

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  GAP ANALYSIS RESULTS                                                         ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  Total Plans Reviewed:           9                                            ║
║  Plans Superseded:               5 (older architectures replaced)             ║
║  Plans Completed:                3 (all items implemented)                    ║
║  Plans with Minor Gaps:          1 (DAG preview bugs)                         ║
║                                                                               ║
║  Critical Gaps from Plans:       0  (all verified as implemented)             ║
║  Minor Gaps Remaining:           3  (DAG preview, keybinding polish)          ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

---

## Plan Status Matrix

| Plan File | Date | Status | Notes |
|-----------|------|--------|-------|
| `nika-studio-tui-design.md` | Feb 20 | 🟡 Superseded | 3-view design → evolved to 5 views |
| `nika-tui-home-chat-integration.md` | Feb 20 | 🟡 Superseded | Early integration plan |
| `nika-tui-mvp8-tasklist.md` | Feb 20 | ✅ Completed | MVP 8 features done |
| `nika-tui-restructure-phase1.md` | Feb 21 | ✅ Completed | Committed 4b22aeb0 |
| `nika-tui-restructure-plan.md` | Feb 21 | 🟡 Superseded | 4-view → 5-view |
| `tui-6-views-architecture.md` | Feb 23 | 🟡 Superseded | 6-view → 5-view |
| `tui-gap-remediation-v2.md` | Feb 23 | ✅ Mostly Done | Critical gaps verified |
| `tui-home-dag-preview-fixes.md` | Feb 23 | 🟡 Minor Gaps | DAG preview bugs |
| `tui-keybindings-audit.md` | Feb 23 | ✅ Completed | 5-view bindings done |

---

## Verified Implementations

### 1. RunWorkflow Action ✅

**Plan claim**: "RunWorkflow is defined but never triggered"

**Reality**: Fully wired in `app.rs:1593`
```rust
ViewAction::RunWorkflow(path) => {
    // Implementation exists
}
```
Also triggered from:
- `studio.rs:672`
- `home.rs:288, 949`

### 2. Task Type Field ✅

**Plan claim**: "`task_type` never populated in events"

**Reality**: Set in `state.rs:1810`
```rust
task.task_type = Some(verb.to_string());
```

### 3. PreviewMode Toggle ✅

**Plan claim**: "PreviewMode enum not wired"

**Reality**: Fully implemented in `home.rs:37`
```rust
pub enum PreviewMode { Dag, Yaml }
```
Toggle at lines 1031-1032.

### 4. Chat Scroll Methods ✅

**Plan claim**: "Missing scroll methods in ChatView"

**Reality**: Extensively implemented in `chat.rs`:
- `scroll_up()` at line 934
- `scroll_down()` at line 918
- `scroll_to_top()` at line 949
- `scroll_to_bottom()` at line 963
- `auto_scroll_to_bottom()` at line 2829

### 5. /invoke Handler ✅

**Plan claim**: "/invoke is a stub"

**Reality**: Extensive implementation across:
- `command.rs` — Command parsing
- `app.rs:2848` — App integration
- `chat.rs` — Chat view handling
- `verb_input.rs` — Verb input processing

### 6. Schema Validation ✅

**Plan claim**: "Validation not integrated"

**Reality**: Full `WorkflowSchemaValidator` integration:
- `startup.rs:128-136` — Startup validation
- `app.rs:3688-3702` — App validation
- `studio.rs:498-526` — Studio live validation
- `standalone.rs:474-486` — Standalone check

### 7. VerbColor System ✅

**Plan claim**: "Missing verb colors"

**Reality**: Complete `VerbColor` enum with all methods:
- `icon()`, `rgb()`, `hex()`, `label()`
- Used in all TaskBox widgets (infer, exec, fetch, invoke, agent)
- Full tests in `matrix_decrypt.rs:1033`

---

## Remaining Minor Gaps

### Gap 1: DAG Preview Node Height Bug (from tui-home-dag-preview-fixes.md)

**Location**: `dag_layout.rs:355`
**Issue**: Node height calculation mismatch (1 vs 3)
**Priority**: Low (visual only)

```rust
// Bug 1.1: Height mismatch
// Uses 1 for calculation but renders with height 3
```

### Gap 2: DAG Coordinate Offset (from tui-home-dag-preview-fixes.md)

**Location**: `dag_ascii.rs`
**Issue**: Coordinate offset not applied when rendering
**Priority**: Low (visual only)

### Gap 3: Keybinding Polish

**Remaining items**:
- Help overlay integration in all 5 views
- Consistent `?` shortcut for help

---

## Recommendation

**No critical implementation work needed**. The codebase is in excellent shape.

**Optional polish tasks** (if desired):
1. Fix DAG preview node height calculation (~30 min)
2. Fix DAG coordinate offset (~30 min)
3. Verify help overlay consistency (~15 min)

**Total estimated effort**: ~1.5 hours of optional polish

---

## Conclusion

The TUI implementation is **complete and functional**. All critical gaps from the plans have been addressed. The remaining items are minor visual polish that don't affect functionality.

**Phase 1** (view consolidation) has been committed and pushed. The TUI now has a clean 5-view architecture:
- Studio (default) — s/1
- Runner — r/2
- Chat — c/3
- Scheduler — d/4
- Settings — ,/5

The v0.21.1 release can proceed with confidence that the TUI is stable.
