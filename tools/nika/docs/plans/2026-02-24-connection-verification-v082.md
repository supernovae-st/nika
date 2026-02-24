# Connection Verification v0.8.2 Enhancement Plan

**Date:** 2026-02-24
**Status:** In Progress
**Target:** Full verification UX with caching, refresh, MCP display, startup badge

## Overview

Enhance the connection verification system added in commit `7e00f76` with:
1. **Cache** - TTL-based caching to avoid redundant API calls
2. **Refresh** - Manual refresh via Cmd+R in provider selector
3. **MCP Display** - Show MCP servers in provider popup
4. **Startup Badge** - Auto-verify on launch with status bar badge

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  Connection Verification v0.8.2                                             │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌──────────────────┐     ┌─────────────────┐     ┌──────────────────┐     │
│  │ VerificationCache│◄────│ App             │────►│ ProviderSelector │     │
│  │                  │     │                 │     │                  │     │
│  │ providers: Map   │     │ spawn_*_verif() │     │ providers[]      │     │
│  │ mcp_servers: Map │     │ startup_verify()│     │ mcp_servers[]    │     │
│  │ ttl: 30s         │     │ refresh_verif() │     │ selected_section │     │
│  └──────────────────┘     └─────────────────┘     └──────────────────┘     │
│           │                       │                       │                │
│           ▼                       ▼                       ▼                │
│  ┌──────────────────────────────────────────────────────────────────┐     │
│  │                     SessionContextBar                             │     │
│  │  🔗 3/6 ✓ providers │ 🔌 2/2 ✓ MCP │ claude-sonnet │ 1.2K tokens  │     │
│  └──────────────────────────────────────────────────────────────────┘     │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Phase 1: VerificationCache Module

**New file:** `src/tui/verification.rs`

### Data Structures

```rust
/// Cached verification result
#[derive(Debug, Clone)]
pub struct VerificationEntry {
    pub status: VerifyStatus,
    pub latency: Option<Duration>,
    pub tool_count: Option<usize>,  // MCP only
    pub model: Option<String>,       // Provider only
    pub verified_at: Instant,
}

/// TTL-based verification cache
pub struct VerificationCache {
    providers: FxHashMap<String, VerificationEntry>,
    mcp_servers: FxHashMap<String, VerificationEntry>,
    ttl: Duration,
}
```

### TDD Tests

1. `test_cache_new_empty` - New cache has no entries
2. `test_cache_set_get_provider` - Set/get provider entry
3. `test_cache_set_get_mcp` - Set/get MCP entry
4. `test_cache_ttl_valid` - Entry within TTL is valid
5. `test_cache_ttl_expired` - Entry past TTL is invalid
6. `test_cache_invalidate_all` - Clear all entries

## Phase 2: Integrate Cache into App

### Changes to `src/tui/app.rs`

```rust
// Add field to App
verification_cache: Arc<Mutex<VerificationCache>>,

// Modify spawn_provider_verification
fn spawn_provider_verification(&self) {
    // Check cache first
    let cache = self.verification_cache.lock();
    if let Some(entry) = cache.get_provider(&provider_id) {
        if cache.is_valid(entry) {
            // Skip - use cached result
            continue;
        }
    }
    // ... spawn verification task
}
```

### TDD Tests

1. `test_spawn_uses_cache` - Verification checks cache first
2. `test_skip_if_cached` - Valid cache entry skips API call
3. `test_verify_if_expired` - Expired entry triggers new verification

## Phase 3: Refresh Shortcut

### Key Handler

```rust
// In ChatView key handler when selector visible
KeyCode::Char('r') if modifiers.contains(KeyModifiers::NONE) => {
    ViewAction::RefreshVerification
}

// New ViewAction variant
ViewAction::RefreshVerification
```

### App Handler

```rust
ViewAction::RefreshVerification => {
    self.verification_cache.lock().invalidate_all();
    self.spawn_provider_verification();
    self.spawn_mcp_verification();
    self.set_status("🔄 Refreshing connections...");
    Action::Continue
}
```

### TDD Tests

1. `test_r_key_triggers_refresh` - R key returns RefreshVerification
2. `test_refresh_invalidates_cache` - Cache is cleared
3. `test_refresh_spawns_verification` - New verification tasks spawned

## Phase 4: MCP Section in Provider Popup

### UI Layout

```
┌────────────────────────────────────────────────┐
│ 🎛️ Select Provider                             │
├────────────────────────────────────────────────┤
│ 🧠 Anthropic Claude          ⚡  ✓ 380ms       │
│ 🤖 OpenAI                    ⚡  ✓ 420ms       │
│ ...                                            │
├────────────────────────────────────────────────┤
│ 🔌 MCP Servers                                 │
│   novanet                    12  ✓ 45ms        │
│   firecrawl                   8  ✗ Error       │
├────────────────────────────────────────────────┤
│ ↑↓ Select • Tab Section • R Refresh • Esc     │
└────────────────────────────────────────────────┘
```

### State Changes

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectorSection {
    Providers,
    McpServers,
}

pub struct ProviderSelectorState {
    // existing fields...
    pub mcp_servers: Vec<McpServerDisplay>,
    pub selected_section: SelectorSection,
    pub selected_mcp: usize,
}
```

### TDD Tests

1. `test_mcp_section_displays` - MCP servers render in popup
2. `test_tab_switches_section` - Tab key toggles section
3. `test_navigation_in_mcp_section` - Up/Down in MCP section

## Phase 5: Startup Verification Badge

### Trigger Location

```rust
// In App::run_unified(), after init_mcp_clients()
self.spawn_startup_verification();
```

### Badge Rendering

```rust
// In SessionContextBar compact mode
fn render_verification_badge(&self) -> String {
    let providers_ok = self.providers.iter()
        .filter(|p| p.verify_status == VerifyStatus::Verified)
        .count();
    let mcp_ok = self.mcp_servers.iter()
        .filter(|s| s.status == McpStatus::Connected)
        .count();

    format!("🔗 {}/{} │ 🔌 {}/{}",
        providers_ok, self.providers.len(),
        mcp_ok, self.mcp_servers.len())
}
```

### TDD Tests

1. `test_startup_calls_verification` - run_unified triggers verification
2. `test_badge_format_all_verified` - "🔗 6/6 │ 🔌 2/2"
3. `test_badge_format_partial` - "🔗 3/6 │ 🔌 1/2"
4. `test_badge_format_none` - "🔗 0/6 │ 🔌 0/2"

## Implementation Order

| Phase | Files | Tests | Deps |
|-------|-------|-------|------|
| 1 | verification.rs (new) | 6 | None |
| 2 | app.rs | 3 | Phase 1 |
| 3 | chat.rs, views/mod.rs, app.rs | 3 | Phase 2 |
| 4 | provider_selector.rs | 3 | None |
| 5 | app.rs, session_context.rs | 4 | Phase 1,2 |

**Total:** 19 tests, 5 files

## Success Criteria

- [ ] All 19 new tests pass
- [ ] Cache prevents redundant API calls (verified via logs)
- [ ] Cmd+R refreshes and shows "Refreshing..." status
- [ ] MCP servers visible in provider popup
- [ ] Badge shows on startup and updates in real-time
- [ ] Zero clippy warnings
