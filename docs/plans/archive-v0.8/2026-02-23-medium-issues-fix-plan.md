# Fix Plan: 5 MEDIUM Issues (v0.8.0 Code Review)

**Date:** 2026-02-23
**Status:** In Progress
**Severity:** MEDIUM (non-blocking, code quality)

---

## Executive Summary

Code review identified 5 MEDIUM issues in Nika v0.8.0 TUI modules:
- 2 in edit_history.rs (performance)
- 2 in session.rs (correctness + architecture)
- 1 in config.rs (error handling)
- 1 in client.rs (robustness)

All issues are non-blocking but should be fixed for production quality.

---

## Issue 1: Vec::remove(0) Performance (MEDIUM)

**File:** `tools/nika/src/tui/edit_history.rs`
**Lines:** 119-121, 137-139

### Problem
```rust
// Line 119-121
if self.undo_stack.len() > self.max_size {
    self.undo_stack.remove(0);  // O(n) - shifts all elements
}

// Line 137-139
if self.undo_stack.len() > self.max_size {
    self.undo_stack.remove(0);  // Same issue
}
```

`Vec::remove(0)` is O(n) because it shifts all remaining elements left.

### Root Cause
`Vec` is optimized for push/pop at the end. Removing from the front causes full array copy.

### Fix
Use `VecDeque` which provides O(1) operations at both ends:

```rust
use std::collections::VecDeque;

pub struct EditHistory {
    undo_stack: VecDeque<EditState>,  // Was: Vec<EditState>
    redo_stack: VecDeque<EditState>,  // Was: Vec<EditState>
    max_size: usize,
    // ... rest unchanged
}

// Line 119-121 fixed
if self.undo_stack.len() > self.max_size {
    self.undo_stack.pop_front();  // O(1) now
}

// Line 137-139 fixed
if self.undo_stack.len() > self.max_size {
    self.undo_stack.pop_front();  // O(1) now
}
```

### Impact
- **Before:** O(n) per size check × n operations = O(n²) worst case
- **After:** O(1) per size check × n operations = O(n) total

### Test Strategy
- Existing 19 tests should pass after migration
- Add benchmark: 10,000 edits with max_size=100

---

## Issue 2: Naive Date Formatting (MEDIUM)

**File:** `tools/nika/src/tui/session.rs`
**Lines:** 259-280

### Problem
```rust
fn format_system_time(time: &SystemTime) -> String {
    match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => {
            let secs = duration.as_secs();
            let days_since_epoch = secs / 86400;

            // PROBLEM: Ignores leap years (should be ~365.25)
            let year = 1970 + (days_since_epoch / 365);
            let day_of_year = days_since_epoch % 365;

            // PROBLEM: Assumes 30-day months
            let month = (day_of_year / 30) + 1;
            let day = (day_of_year % 30) + 1;

            // ... time calculations ...
        }
        Err(_) => "Unknown".to_string(),
    }
}
```

### Root Cause
Hand-rolled date arithmetic that ignores:
- Leap years (adds ~0.25 days/year error)
- Variable month lengths (28-31 days)

### Fix
Use `chrono` crate (already in Cargo.toml):

```rust
use chrono::{DateTime, Utc, Local};

fn format_system_time(time: &SystemTime) -> String {
    let datetime: DateTime<Local> = (*time).into();
    datetime.format("%Y-%m-%d %H:%M:%S").to_string()
}
```

Or if you need "time ago" display:

```rust
fn format_time_ago(time: &SystemTime) -> String {
    let now = SystemTime::now();
    match now.duration_since(*time) {
        Ok(duration) => {
            let secs = duration.as_secs();
            if secs < 60 {
                format!("{}s ago", secs)
            } else if secs < 3600 {
                format!("{}m ago", secs / 60)
            } else if secs < 86400 {
                format!("{}h ago", secs / 3600)
            } else {
                // Use chrono for accurate date
                let datetime: DateTime<Local> = (*time).into();
                datetime.format("%Y-%m-%d").to_string()
            }
        }
        Err(_) => "Unknown".to_string(),
    }
}
```

### Impact
- Dates will be accurate (currently ~1 day/year drift)
- Month boundaries correct (not always 30th)

### Test Strategy
- Test with known dates: 2024-02-29 (leap year), 2023-01-31
- Test Unix epoch edge cases

---

## Issue 3: Validation Layer Placement (MEDIUM)

**File:** `tools/nika/src/tui/session.rs`
**Lines:** 148-154

### Problem
```rust
pub fn save_session(state: &ChatOverlayState) -> io::Result<ChatSession> {
    // Don't save empty sessions (only system message)
    if state.messages.len() <= 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Cannot save empty session",
        ));
    }
    // ... persistence logic ...
}
```

Business validation (empty session check) is in the persistence layer.

### Root Cause
Quick implementation mixed concerns:
- **UI Layer:** Should decide whether to show "Save" button
- **Persistence Layer:** Should only handle I/O

### Fix Option A: Move to UI (Recommended)
```rust
// In UI layer (e.g., app.rs or chat_overlay.rs)
fn handle_save(&mut self) {
    if self.state.messages.len() <= 1 {
        self.show_toast("Nothing to save");
        return;
    }
    match save_session(&self.state) {
        Ok(session) => self.show_toast(&format!("Saved: {}", session.id)),
        Err(e) => self.show_error(&format!("Save failed: {}", e)),
    }
}

// In session.rs - remove the check
pub fn save_session(state: &ChatOverlayState) -> io::Result<ChatSession> {
    // Trust caller did validation - this is I/O layer
    // ... persistence logic only ...
}
```

### Fix Option B: Return Option instead of Error
```rust
pub fn save_session(state: &ChatOverlayState) -> io::Result<Option<ChatSession>> {
    if state.messages.len() <= 1 {
        return Ok(None);  // Nothing to save, not an error
    }
    // ... persistence logic ...
    Ok(Some(session))
}
```

### Impact
- Cleaner separation of concerns
- UI can provide better feedback (button disabled vs error message)
- Persistence layer becomes more reusable

### Test Strategy
- UI tests for empty session handling
- Persistence tests assume valid input

---

## Issue 4: Error Conversion Pattern (MEDIUM)

**File:** `tools/nika/src/tui/config.rs`
**Line:** 199

### Problem
```rust
pub fn save(&self) -> Result<(), ConfigError> {
    let path = Self::config_path()?;
    let content = toml::to_string_pretty(self)?;
    atomic_write(&path, content.as_bytes())
        .map_err(|e| ConfigError::ReadError(std::io::Error::other(e)))?;
    //          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    // Double-wrapping: io::Error → io::Error::other() → ConfigError
    Ok(())
}
```

The error is being double-wrapped:
1. `atomic_write` returns `io::Error`
2. `io::Error::other(e)` wraps it in another `io::Error`
3. `ConfigError::ReadError` wraps that

### Fix
Direct conversion (assuming `atomic_write` returns `io::Error`):

```rust
atomic_write(&path, content.as_bytes())
    .map_err(ConfigError::WriteError)?;  // Direct - no io::Error::other()
```

If `ConfigError` has only `ReadError`:
```rust
// Option 1: Add WriteError variant
pub enum ConfigError {
    ReadError(io::Error),
    WriteError(io::Error),  // Add this
    // ...
}

// Option 2: Use existing variant (less semantic but works)
atomic_write(&path, content.as_bytes())
    .map_err(ConfigError::ReadError)?;
```

### Impact
- Cleaner error chain (no double-wrapping)
- Better error messages in logs

### Test Strategy
- Test save failures (permission denied, disk full)
- Verify error messages are clear

---

## Issue 5: Cache Key Panic Risk (MEDIUM)

**File:** `tools/nika/src/mcp/client.rs`
**Line:** 154

### Problem
```rust
fn cache_key(tool: &str, params: &Value) -> String {
    let mut hasher = FxHasher::default();
    let params_str = serde_json::to_string(params).unwrap_or_default();
    //                                            ^^^^^^^^^^^^^^^^^
    // Silent failure: returns "" on serialization error
    params_str.hash(&mut hasher);
    format!("{}:{:016x}", tool, hasher.finish())
}
```

### Root Cause
`unwrap_or_default()` hides JSON serialization failures:
- All failed params would hash to same empty string
- Cache collisions between different failed params

### Fix Option A: Log and use default (minimal change)
```rust
fn cache_key(tool: &str, params: &Value) -> String {
    let mut hasher = FxHasher::default();
    let params_str = match serde_json::to_string(params) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("Failed to serialize params for cache key: {}", e);
            // Include raw debug representation for uniqueness
            format!("{:?}", params)
        }
    };
    params_str.hash(&mut hasher);
    format!("{}:{:016x}", tool, hasher.finish())
}
```

### Fix Option B: Return Result (propagate error)
```rust
fn cache_key(tool: &str, params: &Value) -> Result<String, serde_json::Error> {
    let mut hasher = FxHasher::default();
    let params_str = serde_json::to_string(params)?;
    params_str.hash(&mut hasher);
    Ok(format!("{}:{:016x}", tool, hasher.finish()))
}

// Caller handles:
let key = cache_key(tool, params).unwrap_or_else(|_| {
    format!("{}:uncacheable", tool)
});
```

### Impact
- No hidden cache collisions
- Better debugging when JSON fails

### Test Strategy
- Test with non-serializable Value (shouldn't happen in practice)
- Test cache key uniqueness

---

## Implementation Order

| Priority | Issue | File | Effort | Risk |
|----------|-------|------|--------|------|
| 1 | Vec::remove(0) | edit_history.rs | Low | Low |
| 2 | Date formatting | session.rs | Low | Low |
| 3 | Error conversion | config.rs | Low | Low |
| 4 | Cache key | client.rs | Low | Low |
| 5 | Validation layer | session.rs | Medium | Low |

**Recommended approach:** Fix 1-4 first (simple), then 5 (requires UI changes).

---

## Verification Checklist

After fixes:
- [ ] `cargo test` passes (1,902 tests)
- [ ] `cargo clippy -- -D warnings` clean
- [ ] `cargo fmt --check` passes
- [ ] Manual TUI test: Edit History undo/redo
- [ ] Manual TUI test: Session save/load

---

**Last Updated:** 2026-02-23
**Author:** Claude Opus 4.5
