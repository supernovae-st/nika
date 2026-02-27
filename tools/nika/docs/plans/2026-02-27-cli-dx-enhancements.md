# CLI DX Enhancements - Implementation Plan

**Date:** 2026-02-27
**Version:** v0.13.1+
**Author:** Claude + Nika

## Overview

Terminal-first CLI improvements inspired by cargo/git/gh patterns identified via research.

---

## P1: Quick Wins (7h total)

### 1.1 Global Flags (2h)

Add `--verbose/-v`, `--quiet/-q`, `--color` to all commands.

**Files to modify:**
- `src/main.rs` - Add flags to Cli struct
- `src/core/output.rs` - Respect color/verbosity settings

**Implementation:**
```rust
// In Cli struct
#[arg(short, long, action = ArgAction::Count, global = true)]
verbose: u8,

#[arg(short, long, global = true)]
quiet: bool,

#[arg(long, default_value = "auto", global = true, value_enum)]
color: ColorChoice,
```

**Verbosity levels:**
- 0 (default): Normal output
- 1 (-v): Info messages
- 2 (-vv): Debug messages
- 3+ (-vvv): Trace messages

### 1.2 Completion Command (2h)

Shell completion for bash/zsh/fish/powershell.

**Files to modify:**
- `src/main.rs` - Add Completion command variant
- `Cargo.toml` - Add clap_complete dependency

**Implementation:**
```rust
// New command variant
Completion {
    #[arg(value_enum)]
    shell: clap_complete::Shell,
}

// Handler
Commands::Completion { shell } => {
    clap_complete::generate(
        shell,
        &mut Cli::command(),
        "nika",
        &mut std::io::stdout(),
    );
}
```

**Usage:**
```bash
# Bash
nika completion bash > ~/.local/share/bash-completion/completions/nika

# Zsh
nika completion zsh > ~/.zfunc/_nika

# Fish
nika completion fish > ~/.config/fish/completions/nika.fish
```

### 1.3 Config Command (3h)

Manage .nika/config.toml via CLI.

**Files to modify:**
- `src/main.rs` - Add Config command with subcommands
- `src/commands/config.rs` - New file for config logic

**Subcommands:**
```rust
#[derive(Subcommand)]
enum ConfigAction {
    /// List all configuration values
    List {
        #[arg(long)]
        json: bool,
    },
    /// Get a specific config value
    Get {
        key: String,
    },
    /// Set a config value
    Set {
        key: String,
        value: String,
    },
    /// Open config in editor
    Edit,
    /// Show config file path
    Path,
}
```

**Usage:**
```bash
nika config list              # Show all settings
nika config get editor.theme  # Get specific value
nika config set editor.theme dark
nika config edit              # Open $EDITOR
nika config path              # Print path
```

---

## P2: Boot & Policy (9h total)

### 2.1 Boot Sequence (5h)

6-phase startup with progress reporting.

**Phases:**
1. Config discovery (find .nika/)
2. Config validation (parse config.toml)
3. Memory loading (load memory files)
4. MCP server startup (launch configured servers)
5. Provider validation (check API keys)
6. Ready state

**Files to create:**
- `src/runtime/boot.rs` - Boot sequence logic
- `src/runtime/boot_phase.rs` - Phase enum and states

### 2.2 Policy Enforcer (4h)

Allow/block rules for commands and resources.

**Config:**
```toml
[policy]
allow_exec = true           # Allow exec: verb
allow_network = true        # Allow fetch: verb
blocked_commands = ["rm -rf", "sudo"]
max_token_spend = 10000
```

**Files to create:**
- `src/runtime/policy.rs` - Policy enforcement
- Update executor.rs to check policies

---

## P3: Operations (14h total)

### 3.1 Heartbeat/Cron (12h)

Background task scheduling.

**Deferred to v0.14+** - Complex feature requiring daemon mode.

### 3.2 Doctor Command (2h)

System health check.

**Checks:**
- Config file valid
- API keys present (without exposing)
- MCP servers reachable
- Disk space for traces
- Rust/cargo version

**Usage:**
```bash
nika doctor
# ✓ Config valid (.nika/config.toml)
# ✓ Claude API key configured
# ✓ NovaNet MCP server responding
# ✓ Trace directory writable (1.2GB free)
# ✓ Rust 1.83.0 detected
```

---

## Execution Order

1. **P1.1** Global flags (foundation for other features)
2. **P1.2** Completion command (quick win, high value)
3. **P1.3** Config command (enables P2 features)
4. **P2.1** Boot sequence (uses config)
5. **P2.2** Policy enforcer (uses config + boot)
6. **P3.2** Doctor command (validates everything)
7. **P3.1** Heartbeat (deferred)

---

## Testing Strategy

Each feature requires:
- Unit tests in implementation file
- Integration test in tests/
- Manual CLI verification

**Test files:**
- `tests/cli_global_flags_test.rs`
- `tests/cli_completion_test.rs`
- `tests/cli_config_test.rs`

---

## Version Plan

- **v0.13.1**: P1 features (completion, config, global flags)
- **v0.13.2**: P2 features (boot, policy)
- **v0.14.0**: P3.2 doctor + stability
- **v0.15.0**: P3.1 heartbeat (if needed)

---

## Section 14: Critical Dependency Updates (Pre-v0.14 Requirement)

**Status:** ✅ COMPLETED (2026-02-27)
**Blocker:** Must be resolved before any v0.14 work

### 14.1 serde_yaml Deprecation

**Problem:** `serde_yaml` v0.9.34 is deprecated (repository archived).

**Solution:** Migrate to `serde-saphyr` v0.0.20 (active fork maintained by saphyr-rs team).

**Changes Applied:**

1. **Cargo.toml:**
   ```toml
   # Before
   serde_yaml = "0.9"

   # After
   serde-saphyr = "0.0.20"
   ```

2. **src/lib.rs** (crate-level alias for minimal code changes):
   ```rust
   pub use serde_saphyr as serde_yaml;
   ```

3. **All source files:** Added `use crate::serde_yaml;` where needed in test modules.

4. **serde_yaml::Value** (not available in serde-saphyr):
   - Replaced with `serde_json::Value` in validation contexts
   - File: `src/tui/views/studio.rs`

5. **Error handling:** Added explicit error mapping for `serde_saphyr::ser::Error`:
   - File: `src/main.rs:940`
   - Uses `NikaError::SerializationError` variant

### 14.2 Security Advisories (tui-textarea)

**Problem:** `tui-textarea` v0.7 pulls in vulnerable dependencies:
- RUSTSEC-2024-0436: `paste` crate is unmaintained
- RUSTSEC-2026-0002: `lru` crate has unsound API

**Solution:** Migrate to `tui-textarea-2` v0.10 (maintained fork with ratatui 0.30+ support).

**Changes Applied:**

```toml
# Before
tui-textarea = { version = "0.7", optional = true }

# After
tui-textarea = { package = "tui-textarea-2", version = "0.10", optional = true }
```

### 14.3 tui-input Update

**Problem:** `tui-input` v0.11 is outdated.

**Solution:** Update to v0.15.

```toml
# Before
tui-input = { version = "0.11", features = ["crossterm"], optional = true }

# After
tui-input = { version = "0.15", features = ["crossterm"], optional = true }
```

### 14.4 Verification Results

```bash
# Compilation
cargo check  # ✅ Passes

# Security audit
cargo audit  # ✅ Zero advisories

# Tests
cargo test --lib  # ✅ 3169 tests passing
```

### 14.5 Migration Impact

| Metric | Before | After |
|--------|--------|-------|
| Security advisories | 2 | 0 |
| Deprecated dependencies | 1 | 0 |
| Tests passing | 3169 | 3169 |
| Breaking changes | N/A | None (alias approach) |

### 14.6 Files Modified

**Core changes:**
- `Cargo.toml` - Dependency updates
- `src/lib.rs` - Added serde_yaml alias
- `src/main.rs` - Import + error handling
- `src/tui/mod.rs` - Added import
- `src/tui/views/studio.rs` - Value type fix
- `src/error.rs` - SerializationError fix_suggestion

**Test imports (42 files):**
- All `tests/*.rs` files - Added `use nika::serde_yaml;`
- All `src/**/*.rs` test modules - Added `use crate::serde_yaml;`
