# Display Refactor — Shared Formatting + Summary Extraction

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Eliminate LiveRenderer/CliRenderer divergence by extracting shared formatting, make render_summary a free function, add borrow-based events_since, and bring output preview + sparklines to LiveRenderer.

**Architecture:** Extract `format_event.rs` with free functions that return formatted strings. Both renderers call them, then emit via their own mechanism (println vs multi.println). Extract `render_summary` from CliRenderer into `summary.rs` as a free function taking `&RunStats`. Add `with_events_since()` to EventLog for zero-copy event iteration.

**Tech Stack:** Rust, indicatif, colored, nika-event, nika-engine

---

### Task 1: Extract render_summary as free function in summary.rs

**Files:**
- Modify: `nika-engine/src/display/summary.rs`
- Modify: `nika-engine/src/display/renderer.rs:1101-1514`
- Modify: `nika-engine/src/display/live.rs` (render_summary delegation)
- Modify: `nika-engine/src/display/mod.rs` (re-export)

Move `CliRenderer::render_summary()` and `render_quiet_summary()` logic into free functions in `summary.rs`. Both renderers call the free functions directly.

### Task 2: Add borrow-based with_events_since to EventLog

**Files:**
- Modify: `nika-event/src/log.rs:1049-1057`
- Modify: `nika-engine/src/runtime/runner.rs:2165,2314`

Add `with_events_since()` method that passes `&[Event]` to a closure instead of cloning.

### Task 3: Extract shared event formatting (top divergent events)

**Files:**
- Create: `nika-engine/src/display/format_event.rs`
- Modify: `nika-engine/src/display/renderer.rs` (CliRenderer)
- Modify: `nika-engine/src/display/live.rs` (LiveRenderer)
- Modify: `nika-engine/src/display/mod.rs`

Extract ForEachCompleted, ExecCompleted, PolicyBlocked handlers + stat accumulation for ProviderResponded into shared functions.

### Task 4: Add output preview + sparklines to LiveRenderer

**Files:**
- Modify: `nika-engine/src/display/live.rs` (TaskCompleted + ProviderResponded handlers)
- Modify: `nika-engine/src/display/renderer.rs` (extract render_output_preview to pub)

Add output preview box and sparkline rendering to LiveRenderer's scrolling log area.

---
