# TaskBox v0.11 Gap Analysis Report

**Date:** 2026-02-26
**Auditor:** Claude Opus 4.5
**Reference:** `2026-02-26-taskbox-ascii-design-spec.md`
**Implementation Plan:** `2026-02-26-taskbox-v0.11-implementation-plan.md`

---

## Executive Summary

Audit of existing TaskBox and Chat DAG widget implementations against the ASCII design specification reveals:

- **Total existing tests:** 247 widget tests
- **Gaps identified:** 12 major, 8 minor
- **Additional effort required:** ~8 hours
- **Additional tests needed:** ~35 tests

---

## 1. Existing Implementation Status

### 1.1 Chat DAG Widgets (v0.10.x - EXISTS)

| Widget | File | Tests | Status |
|--------|------|-------|--------|
| ChatNodeBox | `chat_node_box.rs` | 23 | ✅ Complete |
| ChatEdgeLine | `chat_edge_line.rs` | 16 | ✅ Complete |
| ChatTaskQueue | `chat_task_queue.rs` | 23 | ✅ Complete |
| ChatDagPanel | `chat_dag_panel.rs` | 27 | ✅ Complete |
| **Subtotal** | | **89 tests** | |

### 1.2 TaskBox Widgets

| Widget | File | Tests | Compact | Expanded | Full |
|--------|------|-------|---------|----------|------|
| InferBox | `infer.rs` | 18 | ❌ | ✅ | ⏳ |
| ExecBox | `exec.rs` | 11 | ❌ | ✅ | ❌ |
| FetchBox | `fetch.rs` | 14 | ✅ | ✅ | ❌ |
| InvokeBox | `invoke.rs` | 21 | ✅ | ✅ | ❌ |
| AgentBox | `agent.rs` | 34 | ❌ | ✅ | ❌ |
| TokenVelocity | `token_velocity.rs` | 10 | - | - | - |
| BoxState | `state.rs` | 10 | - | - | - |
| VerbColor | `colors.rs` | 11 | - | - | - |
| **Subtotal** | | **129 tests** | | | |

**Legend:** ✅ Implemented | ⏳ Partial | ❌ Missing

---

## 2. Major Gaps (12)

### GAP-01: HTTP Method Badge Colors ❌

**Design Spec:**
```
[GET]     → Green   #22c55e
[POST]    → Blue    #3b82f6
[PUT]     → Amber   #f59e0b
[PATCH]   → Purple  #a855f7
[DELETE]  → Red     #ef4444
```

**Current:** No `method_color()` function exists in FetchBox

**Fix Required:**
```rust
// Add to fetch.rs or colors.rs
pub fn method_color(method: &str) -> Color {
    match method.to_uppercase().as_str() {
        "GET" => Color::Rgb(34, 197, 94),    // Green
        "POST" => Color::Rgb(59, 130, 246),  // Blue
        "PUT" => Color::Rgb(245, 158, 11),   // Amber
        "PATCH" => Color::Rgb(168, 85, 247), // Purple
        "DELETE" => Color::Rgb(239, 68, 68), // Red
        _ => Color::Rgb(100, 116, 139),      // Gray
    }
}
```

**Effort:** 1h | **Tests:** 6

---

### GAP-02: ExecBox Compact Mode ❌

**Design Spec:**
```
╭─ 📟 EXEC ─────────────────────────────────────── ✅ 0 ────╮
│ $ npm run build                                    4.2s   │
╰────────────────────────────────────────────────────────────╯
```

**Current:** No `render_compact()` method in ExecBox

**Fix:** Implement Phase 6 from implementation plan

**Effort:** 3h | **Tests:** 5

---

### GAP-03: AgentBox Compact Mode ❌

**Design Spec:**
```
╭─ 🐔 AGENT ────────────────────────────────────── ⏳ ───────╮
│ "Generate landing page"                  turn 3/10 │ 2 tools│
╰────────────────────────────────────────────────────────────╯
```

**Current:** No `render_compact()` method in AgentBox

**Fix Required:** Add render_compact method

**Effort:** 2h | **Tests:** 3

---

### GAP-04: SpawnBox Visual Distinction ⏳

**Design Spec:**
```
╭─ 🐤 SPAWN ────────────────────────────────────── ⏳ ───────╮
│ "Generate header section"              depth 1/3 │ turn 2  │
╰────────────────────────────────────────────────────────────╯
```

**Current:** AgentBox has `is_subagent` and `depth` fields but needs to:
1. Use VerbColor::Spawn (🐤) when `is_subagent == true`
2. Display depth indicator `depth 1/3`

**Fix:** Update render method to check `is_subagent` and use appropriate icon/color

**Effort:** 1h | **Tests:** 4

---

### GAP-05: InferBox Compact Mode ❌

**Design Spec:**
```
╭─ ⚡ INFER ──────────────────────────────────── ✅ 2.3s ────╮
│ "Generate a landing page headline..." 847 tokens           │
╰────────────────────────────────────────────────────────────╯
```

**Current:** No `render_compact()` method in InferBox

**Fix Required:** Add render_compact method

**Effort:** 2h | **Tests:** 3

---

### GAP-06: ChatNodeKind Color Alignment ⚠️

**Design Spec:**
| Kind | Design Color | Current Code |
|------|--------------|--------------|
| User | Blue #3b82f6 | Cyan |
| Assistant | Green #22c55e | Green ✓ |
| ToolCall | Amber #f59e0b | Magenta ❌ |
| System | Gray #64748b | Yellow ❌ |

**Fix:** Update ChatNodeKind::color() to use Tailwind colors

**Effort:** 30m | **Tests:** 4

---

### GAP-07: InvokeBox RetryBadge ⏳

**Design Spec:**
```
╭─ 🔌 INVOKE ───────────────────────────────────── ❌ ──────╮
│ novanet::novanet_generate                    🔄 retry 2/3 │
╰────────────────────────────────────────────────────────────╯

Error history:
├─ Attempt 1: -32602 Invalid params (0.1s)
└─ Attempt 2: -32602 Invalid params (0.1s)
```

**Current:** InvokeBox has `retries` field but no:
- RetryBadge struct
- Error history tracking
- Visual retry indicator

**Fix:** Implement Phase 12 from plan (RetryBadge struct)

**Effort:** 2h | **Tests:** 6

---

### GAP-08: Full Render Mode (All Widgets) ❌

**Design Spec shows Full mode with:**
- Extended thinking section (InferBox)
- All headers/response bodies (FetchBox)
- Complete turn history (AgentBox)
- Full param display (InvokeBox)

**Current:** No `render_full()` methods implemented

**Fix:** Add render_full() to all 5 TaskBox widgets

**Effort:** 4h | **Tests:** 5

---

### GAP-09: Turn History Rendering (AgentBox) ⏳

**Design Spec:**
```
│  TURN 1                                               0.8s    │
│  ┊ 💭 I need to get entity context first...                  │
│  ┊ 🔌 novanet_generate(entity: "qr-code", locale: "fr-FR")   │
│  ┊ ✅ Retrieved entity context with 3 forms                  │
```

**Current:** AgentBox has `children: Vec<TaskBox>` but no turn-by-turn history with:
- Thinking display (💭)
- Tool calls inline
- Turn timing
- Turn separators

**Fix:** Add TurnEntry struct and render_turn_history() method

**Effort:** 3h | **Tests:** 4

---

### GAP-10: Streaming Cursor Animation ⏳

**Design Spec:**
```
│  ┊ Créez des QR codes intelligents qui                    │
│  ┊ transforment votre marketing█                          │  ← cursor blinks
```

**Current:** InferBox has `streaming_cursor: bool` but no:
- Actual cursor blink animation (timer-based)
- Cursor character rendering at end of text

**Fix:** Integrate with AnimationTicker for cursor blink

**Effort:** 1h | **Tests:** 2

---

### GAP-11: Response Preview with Line Count ⏳

**Design Spec:**
```
│  ┊ <!DOCTYPE html>                                            │
│  ┊ <html lang="fr">                                           │
│  ┊ <head>...                                                  │
│  ┊ [+142 lines]                                               │
```

**Current:** Long responses are truncated but no `[+N lines]` indicator

**Fix:** Add line count indicator for truncated content

**Effort:** 30m | **Tests:** 2

---

### GAP-12: ChatTaskQueue Hot/Warm/Queued Categories ⏳

**Design Spec:**
```
│  🔥 HOT (currently executing)                                 │
│  ├─ ⚡ infer: "Generate headline"              [▓▓▓▓░░] 67%  │
│                                                               │
│  🌡️ WARM (ready to execute)                                   │
│  ├─ 🔌 invoke: novanet_generate                 ⏳ waiting    │
│                                                               │
│  📋 QUEUED (dependencies pending)                             │
│  ├─ 🛰️ fetch: api.example.com                   deps: [1,2]  │
```

**Current:** ChatTaskQueue has Pending/Running/Complete/Failed but no:
- Hot/Warm/Queued categorization
- Progress bars for running tasks
- Dependency display `deps: [1,2]`

**Fix:** Add task categorization and progress display

**Effort:** 2h | **Tests:** 4

---

## 3. Minor Gaps (8)

| ID | Description | Effort | Tests |
|----|-------------|--------|-------|
| MIN-01 | Provider badge truncation in InferBox footer | 30m | 1 |
| MIN-02 | Cost formatting ($0.0042 vs $0.00) precision | 15m | 1 |
| MIN-03 | CWD truncation with `...` prefix in ExecBox | 30m | 1 |
| MIN-04 | PID display alignment in ExecBox footer | 15m | 1 |
| MIN-05 | Response size formatting (KB/MB) in FetchBox | 30m | 2 |
| MIN-06 | TTFB display format (ms vs s) consistency | 15m | 1 |
| MIN-07 | Node reference display `refs: @N1, @N2` formatting | 30m | 1 |
| MIN-08 | Edge Bezier curve rendering (currently straight lines) | 2h | 3 |

---

## 4. Implementation Plan Alignment

### 4.1 Phases Already Covering Gaps

| Gap | Phase(s) | Coverage |
|-----|----------|----------|
| GAP-02 | Phase 6 | ✅ Full |
| GAP-07 | Phase 12 | ✅ Full |
| GAP-08 | Phases 1, 9 | ⏳ Partial |

### 4.2 Missing from Implementation Plan

| Gap | Description | Add to Phase |
|-----|-------------|--------------|
| GAP-01 | HTTP Method Badge Colors | Phase 8 (extend) |
| GAP-03 | AgentBox Compact Mode | New Phase 10.5 |
| GAP-04 | SpawnBox Visual Distinction | Phase 17 (extend) |
| GAP-05 | InferBox Compact Mode | Phase 4 (extend) |
| GAP-06 | ChatNodeKind Colors | New Phase 18 |
| GAP-09 | Turn History Rendering | Phase 17 (extend) |
| GAP-10 | Streaming Cursor Animation | Phase 4 (extend) |
| GAP-11 | Response Line Count | Phase 9 (extend) |
| GAP-12 | TaskQueue Categories | New Phase 19 |

---

## 5. Updated Test Counts

### Current Tests: 247

| Category | Tests |
|----------|-------|
| Chat DAG widgets | 89 |
| TaskBox widgets | 129 |
| Shared (colors, state) | 29 |

### Gap Tests Needed: 46

| Gap | Tests |
|-----|-------|
| GAP-01 (HTTP colors) | 6 |
| GAP-02 (ExecBox compact) | 5 |
| GAP-03 (AgentBox compact) | 3 |
| GAP-04 (SpawnBox) | 4 |
| GAP-05 (InferBox compact) | 3 |
| GAP-06 (ChatNode colors) | 4 |
| GAP-07 (RetryBadge) | 6 |
| GAP-08 (Full mode) | 5 |
| GAP-09 (Turn history) | 4 |
| GAP-10 (Cursor) | 2 |
| GAP-11 (Line count) | 2 |
| GAP-12 (Queue categories) | 4 |
| Minor gaps | 11 |

### New Total: 293 widget tests

---

## 6. Priority Matrix

### P0 - Critical (Block v0.11 Release)

| Gap | Description | Effort |
|-----|-------------|--------|
| GAP-04 | SpawnBox visual distinction | 1h |
| GAP-07 | InvokeBox RetryBadge | 2h |
| GAP-09 | AgentBox turn history | 3h |

### P1 - High (Strong UX Impact)

| Gap | Description | Effort |
|-----|-------------|--------|
| GAP-01 | HTTP Method Badge Colors | 1h |
| GAP-02 | ExecBox Compact Mode | 3h |
| GAP-05 | InferBox Compact Mode | 2h |
| GAP-10 | Streaming Cursor Animation | 1h |

### P2 - Medium (Polish)

| Gap | Description | Effort |
|-----|-------------|--------|
| GAP-03 | AgentBox Compact Mode | 2h |
| GAP-06 | ChatNodeKind Color Alignment | 30m |
| GAP-08 | Full Render Mode | 4h |
| GAP-11 | Response Line Count | 30m |
| GAP-12 | TaskQueue Categories | 2h |

### P3 - Low (Nice to Have)

| Gap | Description | Effort |
|-----|-------------|--------|
| MIN-01 to MIN-08 | Minor formatting fixes | 5h |

---

## 7. Recommended Implementation Order

```
Week 1 (P0 + P1):
├─ Day 1: GAP-04 SpawnBox + GAP-01 HTTP colors (2h)
├─ Day 2: GAP-07 RetryBadge (2h)
├─ Day 3: GAP-09 Turn History (3h)
├─ Day 4: GAP-02 ExecBox compact + GAP-05 InferBox compact (5h)
└─ Day 5: GAP-10 Cursor animation + integration tests (2h)

Week 2 (P2):
├─ Day 1: GAP-03 AgentBox compact + GAP-06 Colors (2.5h)
├─ Day 2-3: GAP-08 Full render modes (4h)
├─ Day 4: GAP-11 Line count + GAP-12 Queue categories (2.5h)
└─ Day 5: Minor gaps + final polish (5h)
```

**Total Additional Effort:** ~28 hours

---

## 8. References

- **Design Spec:** `2026-02-26-taskbox-ascii-design-spec.md`
- **Implementation Plan:** `2026-02-26-taskbox-v0.11-implementation-plan.md`
- **VerbColor Source:** `src/tui/theme.rs`
- **Chat DAG Plan:** `docs/plans/v0.9.1/2026-02-24-chat-dag-implementation-plan.md`

---

*Report generated by Claude Opus 4.5 on 2026-02-26*
