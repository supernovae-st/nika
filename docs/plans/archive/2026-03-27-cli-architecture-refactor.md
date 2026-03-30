# CLI Architecture Refactor Plan

**Status**: Ready for implementation
**Priority**: MEDIUM (no bugs, architecture improvements)
**Prerequisite**: 8457 tests pass, zero clippy warnings
**Estimated scope**: 4 independent workstreams, can be done in any order

---

## Overview

Four architecture improvements identified by code-reviewer, rust-pro, rust-architect, and coherence-checker agents during the CLI help system refactor session (2026-03-27).

| # | Workstream | Risk | Scope |
|---|-----------|------|-------|
| A | help.rs derive from clap | Medium | ~150 LOC rewrite |
| B | provider.rs move to nika-cli | Low | File move + import fixup |
| C | quiet propagation | Low | 9 handler signatures + 9 dispatch lines |
| D | Init/Invoke delegation | Low | ~30 LOC extract |

---

## A. Derive help.rs from clap Commands enum

### Problem

`help.rs` hardcodes every command name, alias, description, and example as string literals. This creates a parallel shadow of the `Commands` enum in `main.rs`. Any rename, alias change, or new command requires updating BOTH files. Currently:
- `help.rs` shows 24 commands
- `Commands` enum has 32 variants
- `exec` is listed in help.rs but is NOT a CLI command (workflow-only verb)
- `Schema` is in the enum but missing from help.rs

### Solution

Use clap's `Command::get_subcommands()` API to derive help content from the `Commands` enum. Add `#[command(help_heading = "...")]` attributes to group commands.

### Tasks

#### A1. Add `help_heading` to every Commands variant

**File**: `tools/nika/src/main.rs`, lines 114-541

Add `#[command(help_heading = "SECTION")]` to each variant:

```rust
#[command(help_heading = "WORKFLOWS")]
#[command(visible_alias = "r")]
Run { ... },

#[command(help_heading = "5 VERBS")]
#[command(visible_alias = "i")]
Infer { ... },

#[command(help_heading = "INTERACTIVE")]
#[cfg(feature = "tui")]
Ui { ... },

#[command(help_heading = "MODELS & PROVIDERS")]
#[command(visible_alias = "m")]
Model { ... },

#[command(help_heading = "LEARNING")]
#[command(visible_alias = "learn")]
Course { ... },

#[command(help_heading = "PROJECT")]
Init { ... },

#[command(help_heading = "SYSTEM")]
#[command(visible_alias = "d")]
Doctor { ... },
```

Complete mapping (32 variants):

| Variant | Heading |
|---------|---------|
| Run, Check, New, Workflow | WORKFLOWS |
| Infer, Fetch, Invoke, Agent | 5 VERBS |
| Ui, Chat, Studio | INTERACTIVE |
| Model, Provider, Mcp | MODELS & PROVIDERS |
| Course, Showcase | LEARNING |
| Init, Config, Pkg, Media | PROJECT |
| Doctor, Daemon, Cache, Job, Setup, Features, Completion, Trace, Schema | SYSTEM |
| Help | (no heading, or HELP) |
| Cosmic, Lsp | hidden, no heading |

**Verify**: `cargo check`

#### A2. Create a grouping data structure in help.rs

**File**: `tools/nika/src/cli/help.rs`

Replace hardcoded `cmd()` calls with a loop over clap subcommands. Create a section-order array and a per-command metadata map for examples and verb icons:

```rust
use clap::CommandFactory;

/// Extra metadata not available in clap (examples, verb icons).
struct HelpExtra {
    example: &'static str,
    icon: Option<(&'static str, &'static str)>, // (unicode, color)
}

/// Section display order.
const SECTION_ORDER: &[&str] = &[
    "WORKFLOWS", "5 VERBS", "INTERACTIVE",
    "MODELS & PROVIDERS", "LEARNING", "PROJECT", "SYSTEM",
];

fn get_extra(name: &str) -> Option<HelpExtra> {
    // Static map of examples and icons per command name
    match name {
        "run" => Some(HelpExtra { example: "nika run flow.nika.yaml", icon: None }),
        "infer" => Some(HelpExtra { example: "nika infer \"hello\"", icon: Some(("\u{2727}", "magenta")) }),
        // ...
        _ => None,
    }
}
```

Then `print_help()` becomes:

```rust
pub fn print_help() {
    print_banner();

    let app = super::super::Cli::command(); // access the clap Command
    let subs: Vec<&clap::Command> = app.get_subcommands().collect();

    for section_name in SECTION_ORDER {
        section(section_name);
        for sub in &subs {
            if sub.get_help_heading().map(|h| h.to_string()).as_deref() == Some(section_name) {
                let name = sub.get_name();
                let alias = sub.get_visible_aliases().next();
                let about = sub.get_about().map(|s| s.to_string()).unwrap_or_default();
                let extra = get_extra(name);
                // ...print with cmd() or verb_cmd()
            }
        }
        sep();
    }
    // ...deep dive, flags, footer
}
```

**Key point**: `Cli::command()` requires importing from the binary crate, so this stays in `nika/src/cli/help.rs` (not nika-cli). The circular reference is avoided because `help.rs` is IN the binary crate.

**Verify**: `cargo test -p nika --lib -- help`

#### A3. Remove exec from help (it's not a CLI command)

`exec` is a workflow YAML verb, not a CLI subcommand. It should NOT appear in `print_help()`. It can be mentioned in `nika help verbs` (topic_verbs) with a note "(workflow YAML only, no CLI command)".

**Verify**: `./target/debug/nika help` shows 4 verbs in "5 VERBS" section, not 5. The topic `nika help verbs` still shows all 5.

#### A4. Add `Schema` to help

`Schema` is an unhidden variant in `Commands` but not in help.rs. Add it to the SYSTEM section (the dynamic approach from A2 handles this automatically).

**Verify**: `./target/debug/nika help` shows `schema` in SYSTEM.

---

## B. Move provider.rs to nika-cli

### Problem

`provider.rs` lives in the binary crate (`tools/nika/src/cli/provider.rs`) but has zero binary-only dependencies. All its imports (`nika-core`, `nika-engine`, `nika-daemon`) are already available in `nika-cli`. The stated rule is "TUI-dependent handlers stay in the binary crate" — `provider.rs` has no TUI dependency.

### Dependencies (all satisfied in nika-cli)

```
provider.rs imports:
  clap::Subcommand           → in nika-cli
  colored::Colorize          → in nika-cli
  nika::display::*           → use nika_engine::display (already in nika-cli)
  nika::error::NikaError     → use nika_engine::error (already in nika-cli)
  nika::core::*              → use nika_engine::core (already in nika-cli)
  nika::secrets::*           → use nika_engine::secrets (already in nika-cli)
  nika::provider::rig        → use nika_engine::provider::rig (already in nika-cli)
  nika_engine::config        → already in nika-cli
  nika_daemon (#[cfg(unix)]) → already in nika-cli Cargo.toml line 73
```

### Cross-dependency

`onboarding.rs` (line 9) imports `super::provider::detect_provider_from_key`. If both move together, this becomes a sibling module reference — no change needed.

### Tasks

#### B1. Move provider.rs

1. Copy `tools/nika/src/cli/provider.rs` → `tools/nika-cli/src/provider.rs`
2. Update imports: `nika::display` → `nika_engine::display`, `nika::error` → `nika_engine::error`, etc.
3. Add `pub mod provider;` to `tools/nika-cli/src/lib.rs`
4. In `tools/nika/src/cli/mod.rs`: replace `pub mod provider;` with `pub use nika_cli::provider;`
5. **Verify**: `cargo check`

#### B2. Move onboarding.rs (optional, recommended)

Same pattern: copy, update imports, re-export. The `detect_provider_from_key` cross-reference becomes `super::provider::detect_provider_from_key` (same path, now in nika-cli).

If NOT moved: change import in `onboarding.rs` from `super::provider` to `nika_cli::provider`.

**Verify**: `cargo test --workspace --lib`

---

## C. Standardize quiet propagation

### Problem

13 handlers accept `quiet: bool`, 9 do not. There is no consistent contract. Users passing `-q` expect all output suppressed, but 9 commands ignore it.

### Handlers missing `quiet`

| Handler | File | Line |
|---------|------|------|
| `handle_job_command` | `jobs.rs` | 71 |
| `handle_trace_command` | `trace.rs` | 47 |
| `handle_cache_command` | `cache_cmd.rs` | 18 |
| `handle_mcp_command` | `mcp.rs` | 109 |
| `handle_pkg_command` | `pkg.rs` | 92 |
| `handle_course_command` | `course.rs` | 61 |
| `handle_daemon_command` | `daemon.rs` | 67 |
| `handle_provider_command` | `provider.rs` | 98 |
| `handle_setup_command` | `onboarding.rs` | 172 |

### Tasks

#### C1. Add `quiet: bool` parameter to all 9 handlers

For each handler:
1. Add `quiet: bool` as the last parameter
2. Guard user-facing `println!` with `if !quiet { ... }` (only informational output, not errors)
3. Update the dispatch line in `main.rs` to pass `quiet`

Example for `handle_mcp_command`:

```rust
// Before (mcp.rs:109)
pub async fn handle_mcp_command(action: McpAction) -> Result<(), NikaError> {

// After
pub async fn handle_mcp_command(action: McpAction, quiet: bool) -> Result<(), NikaError> {
```

```rust
// Before (main.rs:1089)
Some(Commands::Mcp { action }) => cli::mcp::handle_mcp_command(action).await,

// After
Some(Commands::Mcp { action }) => cli::mcp::handle_mcp_command(action, quiet).await,
```

#### C2. Decide quiet semantics

Document the contract: `quiet` suppresses informational output (status messages, hints, progress). It does NOT suppress:
- Error messages (always shown)
- Data output (JSON, lists — these are the command's purpose)
- Interactive prompts (user must answer)

**Verify**: `cargo test --workspace --lib`, then `./target/debug/nika -q provider list` should show data, not status messages.

---

## D. Delegate Init and Invoke inline logic

### Problem

Two match arms in `main.rs` contain multi-line business logic instead of delegating to handlers:
- `Commands::Init` — 24 lines of course generation logic inline (lines 1042-1074)
- `Commands::Invoke` — 14 lines of tool/list branching inline (lines 983-1004)

### Tasks

#### D1. Extract Init course path into handler

**File**: `tools/nika-cli/src/init.rs`

Add a new function:

```rust
/// Handle `nika init --course` — generate interactive course files.
pub fn init_course() -> Result<(), NikaError> {
    use nika_engine::init::course::generator::{generate_course, CourseConfig};

    let config = CourseConfig {
        dest: std::path::PathBuf::from("nika-course"),
        ..CourseConfig::default()
    };

    match generate_course(&config) {
        Ok(result) => {
            println!(
                "\n  {} Course generated! {} levels, {} exercises\n  Provider: {} (auto-detected)\n  Location: {}\n  Run: cd {} && nika course status\n",
                nika_engine::display::StatusIcon::Ok,
                result.levels, result.exercises, result.provider,
                result.root.display(), result.root.display(),
            );
            Ok(())
        }
        Err(e) => {
            eprintln!("{} Course generation failed: {e}", "Error:".red().bold());
            Err(e.into())
        }
    }
}
```

Then in `main.rs`:

```rust
Some(Commands::Init { permission, migrate_keys, course }) => {
    if course {
        cli::init::init_course()
    } else {
        cli::init::init_project(&permission, migrate_keys).await
    }
}
```

**Verify**: `cargo test -p nika-cli --lib -- init`

#### D2. Change handle_invoke signature to accept `Option<String>`

**File**: `tools/nika-cli/src/verbs.rs`, line 372

```rust
// Before
pub async fn handle_invoke(
    tool: String,           // caller forced to pass String::new() for --list
    ...
) -> Result<(), NikaError>

// After
pub async fn handle_invoke(
    tool: Option<String>,   // None = --list mode or missing tool
    ...
) -> Result<(), NikaError> {
    if list_tools {
        return invoke_list_tools().await;
    }
    let tool = tool.ok_or_else(|| NikaError::ValidationError {
        reason: "Tool name required. Use: nika invoke nika:dimensions file.jpg\nOr: nika invoke --list".to_string(),
    })?;
    // ...rest unchanged
}
```

Then in `main.rs`:

```rust
Some(Commands::Invoke { tool, file, params, mcp, timeout, list }) => {
    cli::verbs::handle_invoke(tool, file, params, mcp, timeout, list, quiet).await
}
```

**Verify**: `cargo test -p nika-cli --lib -- invoke`

---

## Execution Order

These are independent. Recommended order by risk/reward:

1. **D** (Init/Invoke delegation) — smallest change, immediate code quality win
2. **C** (quiet propagation) — mechanical, low risk, high consistency win
3. **B** (provider.rs move) — file move, medium risk from import changes
4. **A** (help.rs derive) — largest rewrite, highest maintenance payoff

Each is a single commit. Run `cargo test --workspace --lib` after each.

---

## Verification Checklist

- [ ] `cargo check --workspace` — zero errors
- [ ] `cargo clippy --workspace -- -D warnings` — zero warnings
- [ ] `cargo test --workspace --lib` — 8457+ tests pass
- [ ] `./target/debug/nika help` — all commands present, grouped correctly
- [ ] `./target/debug/nika -q provider list` — data shown, status suppressed
- [ ] `./target/debug/nika model list` — works
- [ ] `./target/debug/nika cosmic` — still works
- [ ] `./target/debug/nika init --course` — still works
- [ ] `./target/debug/nika invoke --list` — still works
