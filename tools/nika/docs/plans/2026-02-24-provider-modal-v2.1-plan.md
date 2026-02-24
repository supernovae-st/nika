# Provider Modal v2.1 WOW Cosmic Edition - Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task.

**Goal:** Fix critical bugs in Provider Modal and implement WOW Cosmic visual effects.

**Architecture:** TDD-first with immediate bug fixes, then incremental WOW features.

**Tech Stack:** Rust, ratatui, tokio async, parking_lot::Mutex

---

## Phase 1: Critical Bug Fixes (MUST DO FIRST)

### Task 1: Fix Infinite "Checking..." State

**Files:**
- Modify: `src/tui/app.rs:3610-3613`

**Bug:** When provider is not configured, no event is sent, status stays `Checking` forever.

**Step 1: Write failing test**

```rust
// In app.rs tests or separate test file
#[tokio::test]
async fn test_unconfigured_provider_sends_not_configured_event() {
    // Setup: Provider without API key
    // Action: Trigger verification
    // Assert: ProviderNotConfigured event is received
}
```

**Step 2: Implement fix**

```rust
// src/tui/app.rs line 3610-3613, change from:
None => {
    tracing::debug!(provider = %provider_id, "Provider not configured");
}

// To:
None => {
    tracing::debug!(provider = %provider_id, "Provider not configured");
    // v0.8.9: Send NotConfigured event to clear Checking state
    let _ = tx
        .send(StreamChunk::ProviderNotConfigured {
            provider: provider_id,
        })
        .await;
}
```

**Step 3: Add StreamChunk variant**

```rust
// In src/provider/rig.rs around line 530
pub enum StreamChunk {
    // ... existing variants ...
    /// v0.8.9: Provider not configured (no API key)
    ProviderNotConfigured { provider: String },
}
```

**Step 4: Handle event in app.rs**

```rust
// In src/tui/app.rs handle_stream_chunk, add case:
StreamChunk::ProviderNotConfigured { provider } => {
    tracing::debug!(provider = %provider, "Provider not configured");
    use super::widgets::provider_modal::ConnectionStatus;
    self.chat_view
        .provider_modal
        .set_provider_status_by_name(&provider, ConnectionStatus::NotConfigured);
}
```

**Step 5: Commit**
```bash
git add src/tui/app.rs src/provider/rig.rs
git commit -m "fix(tui): send NotConfigured event to clear infinite Checking state"
```

---

### Task 2: Add Per-Provider Verification Timeout

**Files:**
- Modify: `src/tui/app.rs:3496-3615`

**Bug:** Individual provider verification can hang indefinitely if network stalls.

**Step 1: Write failing test**

```rust
#[tokio::test]
async fn test_provider_verification_times_out_after_10_seconds() {
    // Setup: Mock slow provider
    // Action: Start verification with 10s timeout
    // Assert: ProviderVerifyFailed with "timeout" error after 10s
}
```

**Step 2: Implement fix**

```rust
// Wrap the verification in a timeout
use tokio::time::timeout;
const SINGLE_PROVIDER_TIMEOUT: Duration = Duration::from_secs(10);

self.spawn_tracked(async move {
    // ... provider_opt creation ...

    match provider_opt {
        Some(provider) => {
            // v0.8.9: Add per-provider timeout
            match timeout(SINGLE_PROVIDER_TIMEOUT, provider.verify()).await {
                Ok(Ok(result)) => {
                    // ... existing success handling ...
                }
                Ok(Err(e)) => {
                    // ... existing error handling ...
                }
                Err(_) => {
                    // Timeout!
                    let _ = tx
                        .send(StreamChunk::ProviderVerifyFailed {
                            provider: provider_id,
                            error: "Verification timeout (10s)".to_string(),
                        })
                        .await;
                }
            }
        }
        None => {
            // ... NotConfigured handling from Task 1 ...
        }
    }
});
```

**Step 3: Commit**
```bash
git commit -m "fix(tui): add 10s timeout per provider verification"
```

---

### Task 3: Fix Unbounded Vec Growth

**Files:**
- Modify: `src/tui/widgets/provider_modal/state.rs:431-437`

**Bug:** `set_provider_status()` can grow vector indefinitely.

**Step 1: Write failing test**

```rust
#[test]
fn test_set_provider_status_rejects_invalid_index() {
    let mut state = ProviderModalState::default();
    state.set_provider_status(1000, ConnectionStatus::Connected { latency_ms: 100 });
    // Should NOT grow to 1001 elements
    assert!(state.provider_statuses.len() <= 6);
}
```

**Step 2: Implement fix**

```rust
pub fn set_provider_status(&mut self, index: usize, status: ConnectionStatus) {
    // v0.8.9: Guard against invalid index
    if index >= 6 {
        tracing::warn!("Invalid provider index {}, max is 5", index);
        return;
    }
    while self.provider_statuses.len() <= index {
        self.provider_statuses.push(ConnectionStatus::Unknown);
    }
    self.provider_statuses[index] = status;
}
```

**Step 3: Commit**
```bash
git commit -m "fix(tui): guard set_provider_status against unbounded growth"
```

---

### Task 4: Cache cloud_tab_label to Avoid Allocation in Render

**Files:**
- Modify: `src/tui/widgets/provider_modal/state.rs`

**Bug:** `cloud_tab_label()` allocates new String every frame.

**Step 1: Write failing test**

```rust
#[test]
fn test_cloud_tab_label_is_cached() {
    let mut state = ProviderModalState::default();
    state.set_active_model("claude-sonnet-4");

    let label1 = state.cloud_tab_label();
    let label2 = state.cloud_tab_label();

    // Should return same reference (cached)
    assert!(std::ptr::eq(label1.as_ptr(), label2.as_ptr()));
}
```

**Step 2: Implement caching**

```rust
// Add field to ProviderModalState
pub cached_cloud_label: Option<String>,

// Modify set_active_model to invalidate cache
pub fn set_active_model(&mut self, model: impl Into<String>) {
    self.active_model = Some(model.into());
    self.cached_cloud_label = None; // Invalidate
}

// Return cached label
pub fn cloud_tab_label(&mut self) -> &str {
    if self.cached_cloud_label.is_none() {
        let label = if let Some(ref model) = self.active_model {
            let short = if model.len() > 20 {
                format!("{}...", &model[..17])
            } else {
                model.clone()
            };
            format!("☁️  CLOUD [{}]", short)
        } else {
            "☁️  CLOUD".to_string()
        };
        self.cached_cloud_label = Some(label);
    }
    self.cached_cloud_label.as_ref().unwrap()
}
```

**Step 3: Commit**
```bash
git commit -m "perf(tui): cache cloud_tab_label to avoid allocation per frame"
```

---

### Task 5: Fix Drop Race Condition

**Files:**
- Modify: `src/tui/widgets/provider_modal/loader.rs:171-178`

**Bug:** `try_send` in Drop can fail silently.

**Step 1: Implement fix** (simple, no test needed)

```rust
impl Drop for ModalLoader {
    fn drop(&mut self) {
        if self.handle.is_some() {
            // v0.8.9: Use blocking_send to ensure Stop is delivered
            // This is safe in Drop since we're shutting down anyway
            if let Ok(rt) = tokio::runtime::Handle::try_current() {
                let tx = self.cmd_tx.clone();
                rt.spawn(async move {
                    let _ = tx.send(LoaderCommand::Stop).await;
                });
            } else {
                // No runtime - try_send is best effort
                let _ = self.cmd_tx.try_send(LoaderCommand::Stop);
            }
        }
    }
}
```

**Step 2: Commit**
```bash
git commit -m "fix(tui): improve ModalLoader drop to reliably send Stop"
```

---

## Phase 2: WOW Cosmic Visual Effects

### Task 6: Create AnimatedHeader Component

**Files:**
- Create: `src/tui/widgets/provider_modal/components/animated_header.rs`
- Modify: `src/tui/widgets/provider_modal/components/mod.rs`

**Design:**
```
✧ ･ﾟ: *✧･ﾟ:*   ◆ PROVIDER COMMAND CENTER   *:･ﾟ✧*:･ﾟ✧
```

**Step 1: Write tests**

```rust
#[test]
fn test_animated_header_cycles_stars() {
    let mut header = AnimatedHeader::new();
    let frame0 = header.render_to_string(80);
    header.tick();
    let frame1 = header.render_to_string(80);
    assert_ne!(frame0, frame1); // Should animate
}
```

**Step 2: Implement**

```rust
pub struct AnimatedHeader {
    frame: u8,
}

const STAR_FRAMES: &[&[&str]] = &[
    &["✧", "･", "ﾟ", ":", "*", "✧", "･", "ﾟ", ":", "*"],
    &["✦", "*", ":", "ﾟ", "･", "✦", "*", ":", "ﾟ", "･"],
    &["★", "ﾟ", "･", "*", ":", "★", "ﾟ", "･", "*", ":"],
];

impl AnimatedHeader {
    pub fn tick(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        let stars = STAR_FRAMES[(self.frame as usize / 4) % STAR_FRAMES.len()];
        // ... render with gradient colors ...
    }
}
```

---

### Task 7: Enhanced Tab Bar with Live Info

**Files:**
- Modify: `src/tui/widgets/provider_modal/mod.rs` (tab bar rendering)

**Design:**
```
☁️ CLOUD [claude-sonnet-4] │ 🦙 OLLAMA (3) │ 🔐 KEYS ✓✓✓✗ │ ⚙️ CONFIG
════════════════════════════
```

**Features:**
- Active model name in Cloud tab
- Model count in Ollama tab
- Key status indicators (✓/✗) in Keys tab
- Animated underline on active tab

---

### Task 8: Sparkline in Provider Cards

**Files:**
- Modify: `src/tui/widgets/provider_modal/components/provider_card.rs`
- Use existing: `src/tui/widgets/sparkline.rs`

**Design:**
```
╭───────────────────────╮
│ 🧠 Claude    ★ IN USE │
│ claude-sonnet-4       │
│ ▁▂▃▅▇▅▃▂▁  ● 89ms    │  ← Mini sparkline
│ 🧠 👁️ 🔧   200K ctx   │
╰───────────────────────╯
```

---

### Task 9: Footer Stats Bar

**Files:**
- Create: `src/tui/widgets/provider_modal/components/footer_stats.rs`

**Design:**
```
╠═══════════════════════════════════════════════════════════════╣
║  ⌨️ hjkl Navigate │ Enter Select │ r Refresh │ Esc Close     ║
║  ✦ Active: Claude • Tokens: 1.2k/200k • MCP: ● 3/3 • $0.42  ║
╚═══════════════════════════════════════════════════════════════╝
```

---

### Task 10: Matrix Verification Effect

**Files:**
- Use existing: `src/tui/widgets/matrix_decrypt.rs`

**Design:** When verifying, show MatrixDecrypt effect on provider name:
```
⠹ Claude        a@#$ng...  ░░░░░░░░░░
```
Then reveal:
```
● Claude        Connected  ████████████  89ms
```

---

## Testing Summary

| Phase | Task | Tests |
|-------|------|-------|
| 1 | Infinite Checking fix | 1 integration |
| 1 | Per-provider timeout | 1 integration |
| 1 | Unbounded Vec guard | 1 unit |
| 1 | Cached label | 1 unit |
| 1 | Drop fix | Manual |
| 2 | AnimatedHeader | 2 unit |
| 2 | Tab bar | 3 unit |
| 2 | Sparkline cards | 2 unit |
| 2 | Footer stats | 2 unit |
| 2 | Matrix effect | 1 unit |

**Total: 14 new tests minimum**

---

## Execution Order

1. **Phase 1 (Critical):** Tasks 1-5 in sequence (dependencies)
2. **Phase 2 (WOW):** Tasks 6-10 can be parallelized after Phase 1

## Estimated Commits

```
fix(tui): send NotConfigured event to clear infinite Checking state
fix(tui): add 10s timeout per provider verification
fix(tui): guard set_provider_status against unbounded growth
perf(tui): cache cloud_tab_label to avoid allocation per frame
fix(tui): improve ModalLoader drop to reliably send Stop
feat(tui): add AnimatedHeader cosmic component
feat(tui): enhance tab bar with live model info
feat(tui): add sparkline to provider cards
feat(tui): add footer stats bar with session metrics
feat(tui): matrix decrypt effect during verification
```
