# Display & Telemetry v2 — Consolidated Agent Findings

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix correctness bugs, eliminate remaining renderer divergence, render 12 swallowed events, and extract stat accumulation into a shared path — based on findings from 4 review agents (code-reviewer, rust-pro, rust-architect, explorer).

**Architecture:** Three waves. Wave 1 fixes correctness bugs (stripped_len, division-by-zero, type mismatches, dead code). Wave 2 completes the format_event extraction (3 missed formatters + 12 new formatters for swallowed events). Wave 3 extracts `RunStats::apply_event()` to eliminate stat accumulation duplication. Each wave is independently shippable.

**Tech Stack:** Rust, unicode-width (already in deps), colored, indicatif, nika-event, nika-engine

---

## Wave 1: Correctness Fixes (6 tasks, ~45 min)

### Task 1: Fix `stripped_len` to count terminal columns, not chars

**Files:**
- Modify: `nika-engine/src/display/colors.rs:58-75`
- Test: `nika-engine/src/display/tests.rs`

**Why:** CJK characters and emoji occupy 2 terminal columns. Current code counts chars, breaking summary box alignment when task IDs or error messages contain wide characters. `unicode-width` is already a dependency (used by `dag_render.rs`).

**Step 1: Write the failing test**

Add to `nika-engine/src/display/tests.rs`:

```rust
#[test]
fn stripped_len_cjk_double_width() {
    // CJK characters are 2 columns wide
    assert_eq!(super::colors::stripped_len("你好"), 4);
    assert_eq!(super::colors::stripped_len("AB你好CD"), 8);
}

#[test]
fn stripped_len_emoji_double_width() {
    assert_eq!(super::colors::stripped_len("✓"), 1); // narrow
    assert_eq!(super::colors::stripped_len("🎉"), 2); // wide emoji
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p nika-engine --lib -- display::tests::stripped_len_cjk`
Expected: FAIL — `stripped_len("你好")` returns 2, not 4

**Step 3: Fix `stripped_len` to use `unicode_width`**

In `colors.rs:58-75`, replace `len += 1;` with:

```rust
use unicode_width::UnicodeWidthChar;
// ... inside the else branch:
len += UnicodeWidthChar::width(ch).unwrap_or(0);
```

Add `use unicode_width::UnicodeWidthChar;` at top of `colors.rs`.

**Step 4: Run tests**

Run: `cargo test -p nika-engine --lib -- display::tests::stripped_len`
Expected: ALL PASS

**Step 5: Commit**

```
fix(display): stripped_len counts terminal columns not chars (CJK/emoji)
```

---

### Task 2: Guard TTFT average against division by zero

**Files:**
- Modify: `nika-engine/src/display/summary.rs:326`

**Why:** The outer `if !stats.ttft_values.is_empty()` guard protects today, but the bare division is fragile — any refactor that moves the code outside the guard causes a panic.

**Step 1: Apply defensive fix**

In `summary.rs:326`, replace:

```rust
let avg_ttft = stats.ttft_values.iter().sum::<u64>() / stats.ttft_values.len() as u64;
```

with:

```rust
let count = stats.ttft_values.len() as u64;
let avg_ttft = if count > 0 {
    stats.ttft_values.iter().sum::<u64>() / count
} else {
    0
};
```

**Step 2: Run tests**

Run: `cargo test -p nika-engine --lib -- display`
Expected: PASS (113 tests)

**Step 3: Commit**

```
fix(display): guard TTFT average against division by zero
```

---

### Task 3: Fix `hash[..16]` potential panic on non-ASCII

**Files:**
- Modify: `nika-engine/src/display/format_event.rs:335`

**Why:** `&hash[..16]` panics if slicing at a non-char boundary. Blake3 hex hashes are ASCII, but defensive coding prevents future issues.

**Step 1: Fix the slice**

In `format_event.rs`, function `fmt_media_stored`, replace:

```rust
let short_hash = if hash.len() > 16 { &hash[..16] } else { hash };
```

with:

```rust
let short_hash = if hash.len() > 16 {
    &hash[..super::colors::floor_char_boundary(hash, 16)]
} else {
    hash
};
```

**Step 2: Run tests**

Run: `cargo test -p nika-engine --lib -- display`
Expected: PASS

**Step 3: Commit**

```
fix(display): use floor_char_boundary for hash truncation (panic safety)
```

---

### Task 4: Fix `fmt_mcp_response` type mismatch + `budget_used_pct` round-trip

**Files:**
- Modify: `nika-engine/src/display/format_event.rs:106` (signature)
- Modify: `nika-engine/src/display/format_event.rs:56` (signature)
- Modify: `nika-engine/src/display/renderer.rs:491` (call site)
- Modify: `nika-engine/src/display/live.rs:715` (call site)

**Why:** `McpResponse.output_len` is `usize` but `fmt_mcp_response` takes `u64`. Also `budget_used_pct` is `f32` in the event but gets cast `f32→f64→f32` for no reason.

**Step 1: Fix `fmt_mcp_response` signature**

Change `output_len: u64` to `output_len: usize` in `format_event.rs`. Remove `as u64` casts at both call sites.

**Step 2: Fix `fmt_context_assembled` signature**

Change `budget_used_pct: f64` to `budget_used_pct: f32` in `format_event.rs`. Remove `as f64` casts at both call sites, and `as f32` inside the function.

**Step 3: Run tests + clippy**

Run: `cargo test -p nika-engine --lib -- display && cargo clippy -p nika-engine -- -D warnings`
Expected: PASS, zero warnings

**Step 4: Commit**

```
fix(display): align format_event param types with EventKind fields
```

---

### Task 5: Remove dead code (`task_count`, `_ttft_str`, duplicate doc comment)

**Files:**
- Modify: `nika-engine/src/display/renderer.rs:19` (remove `task_count`)
- Modify: `nika-engine/src/display/summary.rs:532-535` (remove `_ttft_str`)
- Modify: `nika-engine/src/display/live.rs:58-59` (remove duplicate doc comment)
- Modify: `nika-engine/src/display/live.rs:441` (remove `task_count` increment)

**Why:** `task_count` is written by LiveRenderer but never read. `_ttft_str` is computed but unused. Duplicate doc comment on `task_token_acc`.

**Step 1: Remove `task_count` from RunStats**

In `renderer.rs`, remove `pub task_count: usize,` from `RunStats`. In `live.rs`, remove `self.stats.task_count += 1;` from `TaskScheduled` handler.

**Step 2: Remove `_ttft_str` dead variable**

In `summary.rs:532-535`, delete:

```rust
let _ttft_str = call
    .ttft_ms
    .map(|t| format!("{}ms", t))
    .unwrap_or_else(|| "\u{2014}".to_string());
```

**Step 3: Remove duplicate doc comment**

In `live.rs:58-59`, remove one of the two identical lines:

```rust
/// Per-task token accumulator for O(1) lookup in TaskCompleted.
```

**Step 4: Run tests + clippy**

Run: `cargo test -p nika-engine --lib -- display && cargo clippy -p nika-engine -- -D warnings`
Expected: PASS

**Step 5: Commit**

```
refactor(display): remove dead code (task_count, _ttft_str, dup comment)
```

---

### Task 6: Cache terminal width in LiveRenderer + pass to summary

**Files:**
- Modify: `nika-engine/src/display/live.rs:510` (use cached `self.term_width`)

**Why:** `terminal_size()` is a syscall. LiveRenderer already stores `term_width` but re-queries at line 510 for output preview.

**Step 1: Replace syscall with cached value**

In `live.rs`, inside `TaskCompleted` handler (~line 510), replace:

```rust
let tw = terminal_size::terminal_size()
    .map(|(w, _)| w.0)
    .unwrap_or(80);
```

with:

```rust
let tw = self.term_width;
```

Note: LiveRenderer must have a `term_width: u16` field. Check if it exists — if not, add it in `new()`.

**Step 2: Run tests**

Run: `cargo test -p nika-engine --lib -- display`
Expected: PASS

**Step 3: Commit**

```
perf(display): use cached terminal width in LiveRenderer (avoid syscall)
```

---

## Wave 2: Complete Format Extraction + Render Swallowed Events (3 tasks, ~60 min)

### Task 7: Migrate `TemplateResolved` and `ProviderCalled` to shared formatters

**Files:**
- Modify: `nika-engine/src/display/format_event.rs` (add `fmt_provider_called`)
- Modify: `nika-engine/src/display/renderer.rs:357-392` (delegate to shared)

**Why:** Code reviewer found 2 event handlers still inline in CliRenderer while LiveRenderer uses the shared formatter. Maintenance trap.

**Step 1: Add `fmt_provider_called` to `format_event.rs`**

```rust
pub fn fmt_provider_called(provider: &str, model: &str, prompt_len: usize) -> String {
    sub(format!(
        "{} {}/{} {} {} chars",
        icons::provider(),
        provider.dimmed(),
        model.white(),
        "· prompt:".dimmed(),
        prompt_len
    ))
}
```

**Step 2: Migrate CliRenderer**

In `renderer.rs`, replace inline `TemplateResolved` handler (lines 357-372) with:

```rust
EventKind::TemplateResolved { task_id: _, template, result } => {
    if self.detail.show_template_events() {
        println!("{}", super::format_event::fmt_template_resolved(template, result));
    }
}
```

Replace inline `ProviderCalled` handler (lines 374-392) with:

```rust
EventKind::ProviderCalled { task_id: _, provider, model, prompt_len } => {
    if self.detail.show_sub_events() {
        println!("{}", super::format_event::fmt_provider_called(provider, model, *prompt_len));
    }
}
```

**Step 3: Migrate LiveRenderer's `ProviderCalled`**

In `live.rs:584-602`, replace inline formatting with:

```rust
self.log(&super::format_event::fmt_provider_called(provider, model, *prompt_len));
```

**Step 4: Run tests + clippy**

Run: `cargo test -p nika-engine --lib -- display && cargo clippy -p nika-engine -- -D warnings`
Expected: PASS

**Step 5: Commit**

```
refactor(display): migrate TemplateResolved + ProviderCalled to shared formatters
```

---

### Task 8: Add `fmt_media_stored_detail` to LiveRenderer (feature parity)

**Files:**
- Modify: `nika-engine/src/display/live.rs` (MediaStored handler)

**Why:** Code reviewer + architect both found this: CliRenderer renders media detail (dedup/verified/pipeline_ms) at Max detail, LiveRenderer silently drops these fields.

**Step 1: Extract missing fields from pattern match**

In `live.rs`, `MediaStored` handler, change `..` to extract `verified`, `pipeline_ms`:

```rust
EventKind::MediaStored {
    hash, path, size_bytes, deduplicated, verified, pipeline_ms, ..
} => {
```

**Step 2: Add detail rendering after `fmt_media_stored`**

After the existing `self.log(&super::format_event::fmt_media_stored(...))` call, add:

```rust
if self.detail.show_previews() {
    self.log(&super::format_event::fmt_media_stored_detail(
        *deduplicated, *verified, *pipeline_ms,
    ));
}
```

**Step 3: Run tests**

Run: `cargo test -p nika-engine --lib -- display`
Expected: PASS

**Step 4: Commit**

```
fix(display): LiveRenderer renders media stored detail at Max level (parity)
```

---

### Task 9: Add formatters for 12 swallowed EventKind variants

**Files:**
- Modify: `nika-engine/src/display/format_event.rs` (add 12 new `fmt_*` functions)
- Modify: `nika-engine/src/display/renderer.rs:790` (replace `_ => {}`)
- Modify: `nika-engine/src/display/live.rs:988` (replace `_ => {}`)
- Test: `nika-engine/src/display/tests.rs`

**Why:** 12 EventKind variants are actively emitted by the runtime but silently swallowed by `_ => {}`. Users never see fetch retries, boot phases, decompose expansion, builtin tool calls, or binding fallbacks.

**Step 1: Add formatters to `format_event.rs`**

```rust
// ── Fetch retry ─────────────────────────────────────────────────────

pub fn fmt_fetch_retry(url: &str, attempt: u32, max_attempts: u32, status_code: Option<u16>, backoff_ms: u64) -> String {
    let status = status_code
        .map(|c| format!(" HTTP {}", c))
        .unwrap_or_default();
    sub(format!(
        "{} {} {}/{} · {}ms backoff{}",
        icons::retry(),
        format!("fetch retry").yellow(),
        attempt.to_string().yellow(),
        max_attempts,
        backoff_ms,
        status
    ))
}

// ── Boot ────────────────────────────────────────────────────────────

pub fn fmt_boot_phase(phase: &str, success: bool, duration_ms: u64, warnings: &[String]) -> String {
    let status = if success { icons::success() } else { icons::failed() };
    let warn = if warnings.is_empty() {
        String::new()
    } else {
        format!(" · {} warnings", warnings.len())
    };
    format!(
        "  {} boot {} {}ms{}",
        status,
        phase.dimmed(),
        duration_ms,
        warn.yellow()
    )
}

pub fn fmt_native_model_loaded(model: &str, kind: &str, duration_ms: u64, is_vision: bool) -> String {
    let vision = if is_vision { " +vision" } else { "" };
    sub(format!(
        "{} {} ({}) loaded {}ms{}",
        icons::provider(),
        model.white(),
        kind.dimmed(),
        duration_ms,
        vision.cyan()
    ))
}

// ── Binding ─────────────────────────────────────────────────────────

pub fn fmt_binding_default(alias: &str, path: &str) -> String {
    sub(format!(
        "{} {} → default (was null: {})",
        "??".yellow(),
        alias.white(),
        path.dimmed()
    ))
}

pub fn fmt_binding_transform(alias: &str, transform_chain: &str) -> String {
    sub(format!(
        "{} {} | {}",
        "⤳".dimmed(),
        alias.white(),
        transform_chain.cyan()
    ))
}

pub fn fmt_binding_env(var_name: &str, found: bool) -> String {
    let status = if found { icons::success() } else { "✗".red().to_string() };
    sub(format!(
        "{} $env.{}{}",
        "env".dimmed(),
        var_name.white(),
        format!(" {}", status)
    ))
}

// ── DAG orchestration ───────────────────────────────────────────────

pub fn fmt_decompose_started(task_id: &str, strategy: &str) -> String {
    sub(format!(
        "{} decompose {} → {}",
        "⊕".magenta(),
        task_id.dimmed(),
        strategy.cyan()
    ))
}

pub fn fmt_decompose_completed(task_id: &str, item_count: usize, duration_ms: u64) -> String {
    sub(format!(
        "{} decompose {} → {} items · {}ms",
        icons::success(),
        task_id.dimmed(),
        item_count.to_string().white(),
        duration_ms
    ))
}

pub fn fmt_for_each_started(task_id: &str, item_count: usize, concurrency: usize) -> String {
    sub(format!(
        "{} for_each {} · {} items · concurrency:{}",
        "⤸".cyan(),
        task_id.dimmed(),
        item_count,
        concurrency
    ))
}

// ── Provider lifecycle ──────────────────────────────────────────────

pub fn fmt_provider_initialized(provider: &str, model: &str, cached: bool) -> String {
    let cache_tag = if cached { " (cached)".green().to_string() } else { String::new() };
    sub(format!(
        "{} init {}/{}{}",
        icons::provider(),
        provider.dimmed(),
        model.white(),
        cache_tag
    ))
}

pub fn fmt_builtin_tool_invoked(tool_name: &str, duration_ms: u64, success: bool) -> String {
    let status = if success { icons::success() } else { icons::failed() };
    sub(format!(
        "{} {} {} · {}ms",
        "⚙".dimmed(),
        tool_name.cyan(),
        status,
        duration_ms
    ))
}

pub fn fmt_extract_applied(mode: &str, input_len: usize, output_len: usize) -> String {
    sub(format!(
        "{} extract:{} · {} → {} bytes",
        "⊳".dimmed(),
        mode.cyan(),
        input_len,
        output_len
    ))
}
```

**Step 2: Replace `_ => {}` catch-all in BOTH renderers**

In `renderer.rs`, replace `_ => {}` (line 790) with explicit match arms:

```rust
EventKind::FetchRetry { url, attempt, max_attempts, status_code, backoff_ms, .. } => {
    println!("{}", super::format_event::fmt_fetch_retry(url, *attempt, *max_attempts, *status_code, *backoff_ms));
}
EventKind::BootPhaseCompleted { phase, success, duration_ms, warnings } => {
    println!("{}", super::format_event::fmt_boot_phase(phase, *success, *duration_ms, warnings));
}
EventKind::NativeModelLoaded { model, kind, duration_ms, is_vision, .. } => {
    if self.detail.show_sub_events() {
        println!("{}", super::format_event::fmt_native_model_loaded(model, kind, *duration_ms, *is_vision));
    }
}
EventKind::BindingDefaultApplied { alias, path, .. } => {
    if self.detail.show_sub_events() {
        println!("{}", super::format_event::fmt_binding_default(alias, path));
    }
}
EventKind::BindingTransformApplied { alias, transform_chain, .. } => {
    if self.detail.show_sub_events() {
        println!("{}", super::format_event::fmt_binding_transform(alias, transform_chain));
    }
}
EventKind::BindingEnvResolved { var_name, found, .. } => {
    if self.detail.show_sub_events() {
        println!("{}", super::format_event::fmt_binding_env(var_name, *found));
    }
}
EventKind::DecomposeStarted { task_id, strategy } => {
    if self.detail.show_sub_events() {
        println!("{}", super::format_event::fmt_decompose_started(task_id, strategy));
    }
}
EventKind::DecomposeCompleted { task_id, item_count, duration_ms, .. } => {
    if self.detail.show_sub_events() {
        println!("{}", super::format_event::fmt_decompose_completed(task_id, *item_count, *duration_ms));
    }
}
EventKind::ForEachStarted { task_id, item_count, concurrency, .. } => {
    if self.detail.show_sub_events() {
        println!("{}", super::format_event::fmt_for_each_started(task_id, *item_count, *concurrency));
    }
}
EventKind::ProviderInitialized { provider, model, cached } => {
    if self.detail.show_sub_events() {
        println!("{}", super::format_event::fmt_provider_initialized(provider, model, *cached));
    }
}
EventKind::BuiltinToolInvoked { tool_name, duration_ms, success, .. } => {
    if self.detail.show_sub_events() {
        println!("{}", super::format_event::fmt_builtin_tool_invoked(tool_name, *duration_ms, *success));
    }
}
EventKind::ExtractApplied { mode, input_len, output_len, .. } => {
    if self.detail.show_sub_events() {
        println!("{}", super::format_event::fmt_extract_applied(mode, *input_len, *output_len));
    }
}
// Handled elsewhere or no display needed
_ => {}
```

Apply identical arms to `live.rs` (line 988), using `self.log(&...)` instead of `println!`.

**Step 3: Write tests**

Add to `tests.rs`:

```rust
#[test]
fn format_event_fetch_retry_renders() {
    let s = super::format_event::fmt_fetch_retry("https://api.example.com", 2, 3, Some(429), 2000);
    let stripped = strip_ansi(&s);
    assert!(stripped.contains("fetch retry"));
    assert!(stripped.contains("2/3"));
    assert!(stripped.contains("2000ms"));
}

#[test]
fn format_event_boot_phase_renders() {
    let s = super::format_event::fmt_boot_phase("config_validation", true, 42, &[]);
    let stripped = strip_ansi(&s);
    assert!(stripped.contains("boot"));
    assert!(stripped.contains("config_validation"));
    assert!(stripped.contains("42ms"));
}

#[test]
fn format_event_binding_default_renders() {
    let s = super::format_event::fmt_binding_default("data", "$step1.missing");
    let stripped = strip_ansi(&s);
    assert!(stripped.contains("??"));
    assert!(stripped.contains("data"));
}
```

**Step 4: Run tests + clippy**

Run: `cargo test -p nika-engine --lib -- display && cargo clippy -p nika-engine -- -D warnings`
Expected: PASS

**Step 5: Commit**

```
feat(display): render 12 previously swallowed event types

FetchRetry, BootPhaseCompleted, NativeModelLoaded, 3x Binding events,
DecomposeStarted/Completed, ForEachStarted, ProviderInitialized,
BuiltinToolInvoked, ExtractApplied — all now visible in CLI output.
```

---

## Wave 3: Structural Improvement (2 tasks, ~45 min)

### Task 10: Extract `RunStats::apply_event()` to eliminate stat duplication

**Files:**
- Modify: `nika-engine/src/display/renderer.rs` (RunStats impl + CliRenderer)
- Modify: `nika-engine/src/display/live.rs` (LiveRenderer)

**Why:** Both renderers have identical stat accumulation logic for: `ProviderResponded` (tokens, cost, ttft, provider_calls), `McpError` (mcp_errors++), `McpInvoke` (mcp_calls++), `McpRetry` (mcp_retries++), `GuardrailPassed/Failed/Escalation`, `ArtifactWritten`, `MediaStored`, `StructuredOutputAttempt/Success`. This is the largest remaining duplication — ~80 LOC duplicated across both renderers.

**Step 1: Add `apply_event()` method to RunStats**

In `renderer.rs`, add:

```rust
impl RunStats {
    /// Apply stat accumulation from an event. Called by both renderers.
    pub fn apply_event(&mut self, event: &crate::event::Event) {
        match &event.kind {
            EventKind::ProviderResponded {
                input_tokens, output_tokens, cache_read_tokens,
                ttft_ms, cost_usd, ..
            } => {
                self.total_input_tokens += input_tokens;
                self.total_output_tokens += output_tokens;
                self.total_cache_tokens += cache_read_tokens;
                self.total_cost += cost_usd;
                if let Some(t) = ttft_ms {
                    self.ttft_values.push(*t);
                }
                self.provider_calls.push(ProviderCallStat {
                    task_id: event.kind.task_id().unwrap_or("?").to_string(),
                    input_tokens: *input_tokens,
                    output_tokens: *output_tokens,
                    cache_tokens: *cache_read_tokens,
                    ttft_ms: *ttft_ms,
                    cost: *cost_usd,
                });
            }
            EventKind::McpError { .. } => { self.mcp_errors += 1; }
            EventKind::McpInvoke { .. } => { self.mcp_calls += 1; }
            EventKind::McpRetry { .. } => { self.mcp_retries += 1; }
            EventKind::GuardrailPassed { .. } => { self.guardrails_passed += 1; }
            EventKind::GuardrailFailed { .. } => { self.guardrails_failed += 1; }
            EventKind::GuardrailEscalation { .. } => { self.guardrails_escalations += 1; }
            EventKind::ArtifactWritten { size, .. } => {
                self.artifacts_count += 1;
                self.artifacts_bytes += size;
            }
            EventKind::MediaStored { size_bytes, deduplicated, .. } => {
                self.media_stored += 1;
                self.media_bytes += size_bytes;
                if *deduplicated { self.media_dedup += 1; }
            }
            EventKind::StructuredOutputAttempt { .. } => { self.structured_attempts += 1; }
            EventKind::StructuredOutputSuccess { layer, .. } => {
                self.structured_success_layer = Some(*layer);
            }
            _ => {}
        }
    }
}
```

**Step 2: Replace inline stat accumulation in both renderers**

In `CliRenderer::render()`, at the top of the method (before the match on `event.kind`), add:

```rust
self.stats.apply_event(event);
```

Then remove all inline stat accumulation from individual match arms. Keep only the display logic.

Do the same in `LiveRenderer::render()`. Note: LiveRenderer also has `task_token_acc` accumulation in `ProviderResponded` — keep that (it's LiveRenderer-specific for the task bar).

**Step 3: Run full test suite**

Run: `cargo test -p nika-engine --lib && cargo clippy -p nika-engine -- -D warnings`
Expected: ALL PASS

**Step 4: Commit**

```
refactor(display): extract RunStats::apply_event() — DRY stat accumulation
```

---

### Task 11: Deduplicate `LiveRenderer::hidden()` constructor

**Files:**
- Modify: `nika-engine/src/display/live.rs:82-163`

**Why:** `hidden()` (test-only) is a near-copy of `new()` — 37 lines of identical code. Adding a field means updating both.

**Step 1: Extract shared builder**

Add a private `fn build(multi: MultiProgress, detail: DetailLevel) -> Self` that both `new()` and `hidden()` call.

**Step 2: Refactor `new()` and `hidden()`**

```rust
pub fn new(detail: DetailLevel) -> Self {
    Self::build(MultiProgress::new(), detail)
}

#[cfg(test)]
pub fn hidden(detail: DetailLevel) -> Self {
    Self::build(
        MultiProgress::with_draw_target(ProgressDrawTarget::hidden()),
        detail,
    )
}

fn build(multi: MultiProgress, detail: DetailLevel) -> Self {
    // ... existing constructor logic using `multi` parameter
}
```

**Step 3: Run tests**

Run: `cargo test -p nika-engine --lib -- display`
Expected: PASS

**Step 4: Commit**

```
refactor(display): deduplicate LiveRenderer::hidden() via shared builder
```

---

## Summary

| Wave | Tasks | LOC change | Impact |
|------|-------|------------|--------|
| 1: Correctness | 1-6 | ~+20/-30 | Fixes CJK alignment, panics, type mismatches, dead code |
| 2: Completeness | 7-9 | ~+250/-50 | 12 events visible, 34/34 formatters shared |
| 3: Structure | 10-11 | ~+60/-120 | Stat accumulation DRY, constructor DRY |

**Total: 11 tasks, ~2.5 hours estimated**
