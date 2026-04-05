# S6 Instructions — Cleanup, Docs, Telemetry, Rust Quality

> **Baseline**: 10,083 tests, 534K LOC, 17 crates, HEAD = `bbe4c5bc7`
> **Previous**: S5 extracted nika-display (303 tests), split runner.rs, 18 CliRenderer arms
> **Goal**: Stale deps + docs accuracy + telemetry gaps + Rust quality cleanup
> **Skills**: `/spn-rust:rust-core`, `/spn-powers:verification-before-completion`

---

## Pre-Session Research

Before writing any code, verify these assumptions (line numbers shift between sessions):

```bash
cd tools

# 1. Confirm test baseline
cargo test --workspace --lib 2>&1 | grep "^test result" | awk '{sum+=$4} END{print sum}'
# Expected: 10,083

# 2. Verify stale deps still unused
grep -r "indicatif" nika-engine/src/ --include="*.rs" | grep -v display | head -5
grep -r "unicode_width" nika-engine/src/ --include="*.rs" | grep -v display | head -5
grep -r "terminal_size" nika-engine/src/ --include="*.rs" | grep -v display | head -5
grep -r "chrono" nika-display/src/ --include="*.rs" | head -5
grep -r "dirs::" nika-mcp/src/ --include="*.rs" | head -5
grep -r "tokio_stream" nika-serve/src/ --include="*.rs" | head -5
grep -r "tokio_stream" nika-sdk/src/ --include="*.rs" | head -5
# ALL should return empty — if any match, DO NOT remove that dep

# 3. Verify EventKind variant count
grep -c "^    [A-Z]" nika-event/src/log.rs
# Expected: ~81 variants

# 4. Find exhaustive EventKind matches (these break on new variants)
grep -rn "EventKind::" nika-tui/src/state/event_handler/mod.rs | head -5
# Confirm TUI handler still has exhaustive match
```

---

## Phase 1: Remove 8 Stale Dependencies (15 min, 1 commit)

### What & Why

After nika-display extraction (S5), 3 deps in nika-engine are only used by
nika-display. 5 others are unused across the workspace. Confirmed by grep audit.

### Exact Changes

**`tools/nika-engine/Cargo.toml`** — remove 3 lines:
```
indicatif = { workspace = true }       ← moved to nika-display
unicode-width = { workspace = true }   ← moved to nika-display
terminal_size = { workspace = true }   ← moved to nika-display
```
NOTE: `colored` STAYS — used in `secrets/key_utils.rs:109` and `runtime/runner.rs:14`

**`tools/nika-display/Cargo.toml`** — remove 1 line:
```
chrono = { workspace = true }          ← never used in display module
```

**`tools/nika-mcp/Cargo.toml`** — remove 1 line:
```
dirs = { workspace = true }            ← unused
```

**`tools/nika-serve/Cargo.toml`** — remove 1 line:
```
tokio-stream = { workspace = true }    ← unused
```

**`tools/nika-sdk/Cargo.toml`** — remove 1 line:
```
tokio-stream = { workspace = true }    ← unused
```

### Verification
```bash
cargo check --workspace              # MUST compile
cargo test --workspace --lib          # MUST pass (count unchanged)
```

### Commit
```
chore(deps): remove 8 unused dependencies

Post nika-display extraction: indicatif, unicode-width, terminal_size
moved to nika-display. chrono never used in display. dirs unused in
nika-mcp. tokio-stream unused in nika-serve and nika-sdk.

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

---

## Phase 2: README.md Update (15 min, 1 commit)

### Exact Edits

**Line 15** — crate badge count:
```
OLD: [![Crates](https://img.shields.io/badge/crates-16-3b82f6?style=flat-square)]
NEW: [![Crates](https://img.shields.io/badge/crates-17-3b82f6?style=flat-square)]
```

**Line ~700** — contributing section:
```
OLD: cargo build                       # build all 16 crates
NEW: cargo build                       # build all 17 crates
```

**Footer (last section):**
```
OLD: **Nika v0.71.0** · Schema `nika/workflow@0.12` · Rust 1.86+ · 16 crates · 10,000+ tests
NEW: **Nika v0.71.0** · Schema `nika/workflow@0.12` · Rust 1.86+ · 17 crates · 10,000+ tests
```

**Crate list** (~line 590): already has `nika-display/` but is missing `nika-vault/`.
Add after `nika-sdk/`:
```
  nika-vault/       Encrypted credential store
```

### Commit
```
docs: update README — 17 crates, add nika-vault to list

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

---

## Phase 3: ARCHITECTURE.md Update (20 min, 1 commit)

### Exact Edits

**Line 3** — header:
```
OLD: Schema `nika/workflow@0.12` | v0.52.0 | 12 crates | 8,888+ tests
NEW: Schema `nika/workflow@0.12` | v0.71.0 | 17 crates | 10,000+ tests
```

**Crate table** (~lines 53-66) — add 5 rows after existing 12:
```markdown
| `nika-display` | ~12k | CLI display renderers |
| `nika-sdk` | ~3k | Rust SDK |
| `nika-serve` | ~4k | HTTP server |
| `nika-storage` | ~1k | Storage abstraction |
| `nika-vault` | ~1.2k | Encrypted credential store |
```

**Key Dependencies table** (~line 188):
```
OLD: | `rig-core` | 0.32 | LLM provider abstraction |
NEW: | `rig-core` | 0.33 | LLM provider abstraction |
```

Also update tokio version if stale:
```
OLD: | `tokio` | 1.49 | Async runtime |
NEW: | `tokio` | 1.50 | Async runtime |
```
(verify actual version in `tools/Cargo.toml` first)

### Commit
```
docs: update ARCHITECTURE — v0.71, 17 crates, rig-core 0.33

19 versions behind (was v0.52). Added nika-display, nika-sdk,
nika-serve, nika-storage, nika-vault to crate table.

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

---

## Phase 4: CHANGELOG v0.69 + v0.70 (30 min, 1 commit)

### Format Reference

The CHANGELOG uses Keep a Changelog format:
- `## [x.y.z] — YYYY-MM-DD` or `## [x.y.z] — YYYY-MM-DD — MARKER`
- Sections: `### Added`, `### Fixed`, `### Changed`, `### Architecture`
- Each entry: `- **Bold name** — Description`
- Latest entry is v0.68.0

### Content

Insert AFTER the `## [0.68.0]` block:

```markdown
---

## [0.69.0] — 2026-04-05

### Added

- **11 new transforms** — `replace`, `truncate`, `add`, `min`, `max`, `not`, `min_by`, `max_by`, `sum`, `avg`, `has` (52→63 total)
- **`when:` conditional execution** — Wired through analyzer and schema for runtime conditional evaluation
- **`nika test`** — Run workflows with mock provider. `--golden` for snapshot comparison, `--update-snapshot` to update
- **`--resume` flag** — Re-run workflows skipping completed tasks (reads latest trace NDJSON)
- **`nika lint`** — Best-practice linting with 8 rules (L001-L070)
- **`nika version`, `nika env`, `nika graph`** — Version info, debug view, DAG visualization
- **7 OpenAI-compatible providers** — OpenRouter, Together AI, Fireworks AI, Cerebras, SambaNova, Cohere, AI21 Labs
- **ModelResolver catalog** — Centralized model routing eliminates hardcoded fallback sites

### Fixed

- **NaN/Infinity** in min/max transforms — Guard with safe f64_to_json_number()
- **when: analyzer bug** — Fixed setting `when: None` instead of copying from raw AST
- **YAML bomb DoS** — Pre-parse 2 MiB budget check
- **Shell injection** — Extended blocklist patterns, BASH_ENV/ENV blocking
- **PolicyBlocked events** — Security blocks now emit events before returning errors

---

## [0.70.0] — 2026-04-05

### Added

- **`nika eval`** — Dataset-driven evaluation with assertion types: output_contains, output_min_words, output_max_words, output_matches_schema
- **`POST /v1/batch/run`** — Submit up to 50 workflows atomically (two-pass validation)
- **`GET /v1/jobs`** — List jobs with state/workflow/tag filters and pagination
- **Job tags** — Arbitrary key:value metadata on jobs, filtered via `?tag=key:value`
- **Lint rules L080 + L090** — Conditional gap detection, duplicate task name detection (10 total)

### Fixed

- **`nika test --golden`** — Proper output capture, normalization, and field-by-field diff
- **sum transform** — Restricted to numbers only (was polymorphic)
- **Batch validation** — All-or-nothing (prevents partial failures)
```

### Commit
```
docs(changelog): add v0.69 and v0.70 entries

v0.69: 11 transforms, when:, nika test/lint/version/env/graph, 7 providers
v0.70: nika eval, batch/run, job tags, lint L080+L090

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

---

## Phase 5: Telemetry Gap Fixes (4-6h, 4 commits)

### Context: EventKind Architecture

**File**: `nika-event/src/log.rs`
**Current**: 81 variants, `#[serde(tag = "type", rename_all = "snake_case")]`
**Field convention**: `Arc<str>` for task_id, `String` for text, `Arc<Value>` for JSON

**3 EXHAUSTIVE matches that BREAK on new variants** (must ALL be updated):

| Location | File | Purpose |
|----------|------|---------|
| `task_id()` method | `nika-event/src/log.rs:1069-1156` | Returns task_id from any variant |
| `CliRenderer::render()` | `nika-display/src/renderer.rs:405-1546` | CLI event rendering |
| `TuiState::handle_event()` | `nika-tui/src/state/event_handler/mod.rs:29-518` | TUI event handling |

**2 non-exhaustive matches** (safe, have `_ =>` fallback):
- `RunStats::apply_event()` in renderer.rs:190-309
- `spawn_event_forwarder()` in nika-serve/src/executor.rs:281-361

### Corrected Gap Analysis

| # | Gap | File:Line | Verified | New Variant |
|---|-----|-----------|----------|-------------|
| 1 | Template resolution fails | `infer.rs:83-86` | ACTIVE | `TemplateResolutionFailed` |
| 2 | CRLF injection | `fetch.rs:343-370` | FIXED S1 | — |
| 3 | Domain rate limiter | `fetch.rs:210-222` | ACTIVE | `RateLimitDelay` |
| 4 | MCP tool size limit | `invoke.rs:229-240` | FIXED S3 | — |
| 5 | Schema file loading | `infer.rs:617-664` | ACTIVE | `SchemaLoadFailed` |
| 6 | Vision CAS/format fails | `infer.rs:1503-1539` | ACTIVE | `VisionContentFailed` |
| 7 | Shell injection blocks | `exec.rs:36-140` | FIXED S1 | — |

### Commit 1: Add 4 EventKind variants

**Add to `nika-event/src/log.rs`** inside the EventKind enum (after the last variant):

```rust
/// Template binding resolution failed before task execution.
TemplateResolutionFailed {
    task_id: Arc<str>,
    template: String,
    error: String,
},

/// Domain rate limiter delayed request.
RateLimitDelay {
    task_id: Arc<str>,
    domain: String,
    delay_ms: u64,
},

/// Schema file could not be loaded or parsed.
SchemaLoadFailed {
    task_id: Arc<str>,
    schema_path: String,
    error: String,
},

/// Vision content resolution failed (CAS read, unsupported format, etc.).
VisionContentFailed {
    task_id: Arc<str>,
    source: String,
    stage: String,
    error: String,
},
```

**Update `task_id()` method** in same file (~line 1069):
Add 4 arms returning `Some(task_id)` for each new variant.

**Update CliRenderer::render()** in `nika-display/src/renderer.rs`:
Add 4 arms in the "INTENTIONAL NO-OPS" section (or with actual rendering):

```rust
EventKind::TemplateResolutionFailed { task_id, error, .. } => {
    println!(
        "{} {} {}: template error: {}",
        self.ts(),
        "✗".red(),
        task_id,
        error
    );
}
EventKind::RateLimitDelay { task_id, domain, delay_ms } => {
    if self.detail.show_sub_events() {
        println!(
            "{} {} {}: rate limited on {} ({}ms)",
            self.ts(),
            "⏱".yellow(),
            task_id,
            domain,
            delay_ms
        );
    }
}
EventKind::SchemaLoadFailed { task_id, schema_path, error } => {
    println!(
        "{} {} {}: schema load failed: {} — {}",
        self.ts(),
        "✗".red(),
        task_id,
        schema_path,
        error
    );
}
EventKind::VisionContentFailed { task_id, stage, error, .. } => {
    println!(
        "{} {} {}: vision {}: {}",
        self.ts(),
        "✗".red(),
        task_id,
        stage,
        error
    );
}
```

**Update LiveRenderer** in `nika-display/src/live.rs`:
Add 4 arms (same pattern but using `self.log(&format!(...))` instead of println).

**Update TuiState::handle_event()** in `nika-tui/src/state/event_handler/mod.rs`:
Add 4 arms. Group with the existing no-op block (~line 476-516).
For template/schema/vision failures: push to error pane.
For rate limit: no-op (debug noise in TUI).

**Tests** — Add to `nika-event/src/log.rs` test module:
```rust
#[test]
fn test_template_resolution_failed_serde() {
    let event = EventKind::TemplateResolutionFailed {
        task_id: Arc::from("my_task"),
        template: "{{with.missing}}".to_string(),
        error: "unknown alias 'missing'".to_string(),
    };
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains(r#""type":"template_resolution_failed""#));
    let rt: EventKind = serde_json::from_str(&json).unwrap();
    assert_eq!(event, rt);
}
// Repeat for RateLimitDelay, SchemaLoadFailed, VisionContentFailed
```

**Commit**:
```
feat(event): add 4 telemetry event variants

TemplateResolutionFailed, RateLimitDelay, SchemaLoadFailed,
VisionContentFailed. All 3 exhaustive matches updated:
log.rs::task_id(), CliRenderer, TuiState.

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

### Commit 2: Emit TemplateResolutionFailed + SchemaLoadFailed

**`nika-engine/src/runtime/executor/infer.rs:83-86`**:
```rust
// BEFORE:
let mut prompt = template_resolve(&infer.prompt, bindings, datastore)?.into_owned();

// AFTER:
let mut prompt = match template_resolve(&infer.prompt, bindings, datastore) {
    Ok(resolved) => resolved.into_owned(),
    Err(e) => {
        self.event_log.emit(EventKind::TemplateResolutionFailed {
            task_id: Arc::clone(task_id),
            template: infer.prompt.clone(),
            error: e.to_string(),
        });
        return Err(e);
    }
};
```

Same pattern for system prompt at ~line 85.

**`infer.rs:617-664`** — schema loading:
```rust
// BEFORE:
tokio::fs::read_to_string(&resolved_path).await.map_err(|e| NikaError::SchemaFailed { ... })?;

// AFTER:
let content = match tokio::fs::read_to_string(&resolved_path).await {
    Ok(c) => c,
    Err(e) => {
        self.event_log.emit(EventKind::SchemaLoadFailed {
            task_id: Arc::clone(task_id),
            schema_path: resolved_path.clone(),
            error: e.to_string(),
        });
        return Err(NikaError::SchemaFailed { details: format!("Failed to read schema '{}': {}", resolved_path, e) });
    }
};
```

Same pattern for JSON parse error at ~line 626.

**Commit**:
```
fix(infer): emit TemplateResolutionFailed + SchemaLoadFailed events

Template resolution and schema file loading failures now emit
diagnostic events before returning errors. Previously only TaskFailed
was emitted, making debugging invisible.

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

### Commit 3: Emit RateLimitDelay

**`nika-engine/src/runtime/executor/fetch.rs:210-222`**:
```rust
// BEFORE:
limiter.acquire(domain).await;

// AFTER:
let rl_start = std::time::Instant::now();
limiter.acquire(domain).await;
let delay_ms = rl_start.elapsed().as_millis() as u64;
if delay_ms > 50 {
    self.event_log.emit(EventKind::RateLimitDelay {
        task_id: Arc::clone(task_id),
        domain: domain.to_string(),
        delay_ms,
    });
}
```

Only emit when delay > 50ms (avoid noise for instant acquisitions).

**Commit**:
```
fix(fetch): emit RateLimitDelay event for domain throttling

Domain rate limiter.acquire() was a silent await. Now emits
RateLimitDelay when blocking > 50ms, visible in TUI and traces.

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

### Commit 4: Emit VisionContentFailed

**`infer.rs:1503-1509`** — CAS read failure:
```rust
// BEFORE:
result.map_err(|e| ProviderError::ApiError { message: format!("Vision: CAS read '{}': {}", ...) })?

// AFTER: emit event, then return error
Err(e) => {
    self.event_log.emit(EventKind::VisionContentFailed {
        task_id: Arc::clone(task_id),
        source: resolved_source.clone(),
        stage: "cas_read".to_string(),
        error: e.to_string(),
    });
    return Err(ProviderError::ApiError { message: format!(...) }.into());
}
```

**`infer.rs:1531-1539`** — unsupported format:
```rust
// Add event emission before the return Err
self.event_log.emit(EventKind::VisionContentFailed {
    task_id: Arc::clone(task_id),
    source: resolved_source.clone(),
    stage: "unsupported_format".to_string(),
    error: "Supported: PNG, JPEG, GIF, WebP".to_string(),
});
```

**Commit**:
```
fix(infer): emit VisionContentFailed for CAS read + format errors

Vision content resolution failures (CAS missing, unsupported format)
now emit diagnostic events. Success path already had VisionContentResolved.

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

---

## Phase 6: Rust Quality Cleanup (30 min, 1 commit)

### From Audit Findings

**1. Double Arc wrap** — `infer.rs:536` (verify current line):
```rust
// BEFORE (wasteful):
Arc::new(self.event_log.clone())

// AFTER (correct):
// If event_log is already Arc<EventLog>, use Arc::clone
// If event_log is EventLog directly, this is fine — check type first
```

**2. Stale lockfile comment** — `runner.rs` in finalize_run():
```rust
// BEFORE:
// Lockfile is removed automatically when `_lockfile_guard` drops
// (at function exit -- normal return, error, or panic).

// This comment is now WRONG — lockfile drops in run(), not finalize_run().
// REMOVE the comment from finalize_run.
```

**3. `_completed` counter** — `runner.rs` in run():
```rust
// `let mut _completed = 0;` + `_completed += 1;` in the loop
// This counter is NEVER READ — it's dead code.
// Remove both the declaration and the increment.
```

### Commit
```
refactor(engine): remove dead _completed counter + stale comment

_completed was incremented but never read. Lockfile comment was
stale after finalize_run() extraction (guard lives in run()).

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

---

## Summary: 9 commits total

| # | Phase | Commit | Time |
|---|-------|--------|------|
| 1 | Stale deps | `chore(deps): remove 8 unused dependencies` | 15 min |
| 2 | README | `docs: update README — 17 crates, add nika-vault` | 10 min |
| 3 | ARCHITECTURE | `docs: update ARCHITECTURE — v0.71, 17 crates` | 15 min |
| 4 | CHANGELOG | `docs(changelog): add v0.69 and v0.70 entries` | 30 min |
| 5 | EventKind variants | `feat(event): add 4 telemetry event variants` | 90 min |
| 6 | Template + Schema | `fix(infer): emit TemplateResolutionFailed + SchemaLoadFailed` | 45 min |
| 7 | Rate limit | `fix(fetch): emit RateLimitDelay event` | 20 min |
| 8 | Vision | `fix(infer): emit VisionContentFailed` | 30 min |
| 9 | Rust quality | `refactor(engine): remove dead code + stale comment` | 15 min |

**Total**: ~4.5h | **Expected tests**: 10,083 + ~10 new ≈ 10,093+

---

## Verification Checklist

After ALL phases:
```bash
cd tools
cargo test --workspace --lib                    # ALL pass
cargo clippy --workspace                        # ZERO warnings
cargo check --workspace 2>&1 | grep warning     # ZERO warnings
git log --oneline -10                           # 9 clean commits
```

---

## Rules

- `cargo test --workspace --lib` green after EVERY commit
- 1 fix = 1 commit (never batch unrelated changes)
- Co-author: ONLY `Nika 🦋 <nika@supernovae.studio>` (NEVER Claude/Anthropic)
- AGPL-3.0-or-later on new files
- Do NOT touch `tools/nika-cli/src/lint.rs` (WIP local)
- Verify line numbers BEFORE editing (they shift between sessions)
- `Arc<str>` for task_id fields in EventKind, `String` for everything else
- Serde: `tag = "type"`, `rename_all = "snake_case"` — new variants auto-named
- Use `/spn-rust:rust-core` skill for Rust patterns
- Use `/spn-powers:verification-before-completion` before each commit

---

## Key File Paths

| What | Path | Key Lines |
|------|------|-----------|
| EventKind enum | `nika-event/src/log.rs` | 147-1065 (81 variants) |
| EventKind::task_id() | `nika-event/src/log.rs` | 1069-1156 (exhaustive) |
| CliRenderer::render() | `nika-display/src/renderer.rs` | 405-1546 (exhaustive) |
| LiveRenderer::handle_event() | `nika-display/src/live.rs` | 762-1619 (exhaustive) |
| TuiState::handle_event() | `nika-tui/src/state/event_handler/mod.rs` | 29-518 (exhaustive) |
| RunStats::apply_event() | `nika-display/src/renderer.rs` | 190-309 (fallback) |
| Executor infer | `nika-engine/src/runtime/executor/infer.rs` | 83, 617, 1503 |
| Executor fetch | `nika-engine/src/runtime/executor/fetch.rs` | 210-222 |
| README | `README.md` | 15, ~590, ~700, footer |
| ARCHITECTURE | `docs/ARCHITECTURE.md` | 3, 53-66, ~188 |
| CHANGELOG | `tools/nika/CHANGELOG.md` | after line 8 |
| Engine Cargo.toml | `tools/nika-engine/Cargo.toml` | 106, 138, 139 |
| Display Cargo.toml | `tools/nika-display/Cargo.toml` | 19 |
