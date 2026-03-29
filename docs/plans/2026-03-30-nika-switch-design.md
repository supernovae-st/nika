# `nika switch` — Dual Channel Management (dev / release)

> Design doc — 2026-03-30
> Status: **APPROVED** (brainstorming complete)

## Goal

Two versions of Nika on the same machine, switchable with one command:

- **release** — Homebrew (`brew install supernovae-st/tap/nika`)
- **dev** — Auto-built from repo on every git commit

```
$ nika switch dev

   release   v0.49.0          homebrew
              ○
              ┃
              ┃  ✦
              ┃
              ●━━━━━━━ active
   dev        v0.51.0-dev      abc1234 · built 2min ago

   ╭──────────────────────────────────────────╮
   │  ✦ Switched to dev                       │
   │  v0.51.0-dev @ abc1234 · built 2min ago  │
   ╰──────────────────────────────────────────╯
```

## Architecture

```
~/.nika/
├── bin/
│   ├── nika          -> symlink (dev binary OR homebrew binary)
│   └── nika-dev      # latest dev build from repo
├── channel           # "dev" or "release" (1 word)
└── builds/
    ├── dev.json      # { version, hash, built_at, status }
    └── last.log      # cargo build stdout+stderr

PATH priority (in .zshrc):
  ~/.nika/bin  >  /opt/homebrew/bin  >  ~/.cargo/bin
```

### Symlink targets

| Channel   | Symlink `~/.nika/bin/nika` points to     |
|-----------|------------------------------------------|
| `release` | `/opt/homebrew/bin/nika` (Homebrew)       |
| `dev`     | `~/.nika/bin/nika-dev` (local build)      |

### Build metadata (embedded via build.rs)

Each binary embeds at compile time:

| Field      | Source                         | Example                    |
|------------|--------------------------------|----------------------------|
| `version`  | `CARGO_PKG_VERSION`            | `0.51.0`                   |
| `channel`  | `NIKA_BUILD_CHANNEL` env       | `dev` or `release`         |
| `git_hash` | `git rev-parse --short HEAD`   | `abc1234`                  |
| `built_at` | `chrono::Utc::now()`           | `2026-03-30T14:23:00Z`     |

Displayed via `nika --version`:
```
nika 0.51.0-dev (abc1234, built 2min ago)    # dev channel
nika 0.49.0 (release, homebrew)              # release channel
```

## CLI Commands

```
nika switch              # Status — show both channels
nika switch dev          # Switch to dev channel
nika switch release      # Switch to release channel
nika switch --setup      # One-time bootstrap
nika switch --build      # Force rebuild dev now
```

## Animation UX (Solarized palette)

### Colors

| Element  | Color                          |
|----------|--------------------------------|
| release  | Cyan `#2aa198`                 |
| dev      | Magenta `#d33682`              |
| active   | Green bold `#859900`           |
| dim      | Base01 `#586e75`               |
| banner ✦ | Yellow `#b58900`               |
| error    | Red `#dc322f`                  |

### Status view (`nika switch`)

```
                   .  *  .
                *  NIKA  *
                   '  *  '

  ┌─────────────────────────────────────────────────────┐
  │                                                     │
  │   release   v0.49.0          homebrew               │
  │              ○                                      │
  │              │                                      │
  │              ●━━━━━━━ active                        │
  │              │                                      │
  │              ○                                      │
  │   dev        v0.51.0-dev      abc1234 · 2min ago    │
  │                                                     │
  └─────────────────────────────────────────────────────┘
```

- Active channel: GREEN BOLD + ● filled dot
- Inactive channel: DIM gray + ○ empty dot
- "active" label: pulses once (0.5s fade)

### Transition animation (`nika switch dev`)

4 frames over 500ms:

```
Frame 1 (0ms)     release  v0.49.0  ●━━━━ active
                   dev      v0.51.0  ○

Frame 2 (150ms)   release  v0.49.0  ○
                                     ┃  ✦
                   dev      v0.51.0  ○

Frame 3 (300ms)   release  v0.49.0  ○
                   dev      v0.51.0  ●━━━━ active

Frame 4 (500ms)   ╭──────────────────────────────────────────╮
                   │  ✦ Switched to dev                       │
                   │  v0.51.0-dev @ abc1234 · built 2min ago  │
                   ╰──────────────────────────────────────────╯
```

### Build animation (`nika switch --build`)

```
⣾ Building nika-dev from main...
████████████████████░░░░  78%  nika-engine

╭──────────────────────────────────────────╮
│  ✦ Dev build complete                    │
│  v0.51.0-dev @ def5678 · just now        │
│  compiled in 47s                         │
╰──────────────────────────────────────────╯
```

### Setup animation (`nika switch --setup`)

```
✦ Setting up Nika channels...
✓ Created ~/.nika/bin/
✓ Found release: /opt/homebrew/bin/nika (v0.49.0)
⣾ Building dev from main @ abc1234...
✓ Dev ready: v0.51.0-dev (47s)
✓ Git hook installed
✓ Active channel: dev

Add to ~/.zshrc:
  export PATH="$HOME/.nika/bin:$PATH"
```

## Git Post-Commit Hook

Auto-installed by `nika switch --setup` at `<repo>/.git/hooks/post-commit`:

```sh
#!/bin/sh
# Nika auto-build — background rebuild on commit
NIKA_DIR="$HOME/.nika"
REPO_DIR="$(git rev-parse --show-toplevel)"
LOG="$NIKA_DIR/builds/last.log"
LOCK="$NIKA_DIR/builds/build.lock"

# Debounce: skip if build started < 10s ago
if [ -f "$LOCK" ]; then
  lock_age=$(( $(date +%s) - $(cat "$LOCK") ))
  [ "$lock_age" -lt 10 ] && exit 0
fi

# Write lock timestamp
date +%s > "$LOCK"

nohup sh -c '
  cd "$1/tools/nika"
  cargo build --release > "$2" 2>&1
  if [ $? -eq 0 ]; then
    cp "$1/tools/target/release/nika" "$3/bin/nika-dev"
    hash=$(git -C "$1" rev-parse --short HEAD)
    version=$(grep "^version" "$1/tools/nika/Cargo.toml" | head -1 | cut -d\" -f2)
    printf "{\"version\":\"%s\",\"hash\":\"%s\",\"built_at\":\"%s\",\"status\":\"ok\"}" \
      "$version" "$hash" "$(date -u +%%FT%%TZ)" > "$3/builds/dev.json"
  else
    printf "{\"status\":\"failed\",\"built_at\":\"%s\"}" \
      "$(date -u +%%FT%%TZ)" > "$3/builds/dev.json"
  fi
  rm -f "$3/builds/build.lock"
' _ "$REPO_DIR" "$LOG" "$NIKA_DIR" &
```

Key properties:
- **Background** — `nohup ... &` — commit returns instantly
- **Debounce** — lock file with timestamp, skip if < 10s since last build
- **Incremental** — `cargo build` (not clean), so subsequent commits are fast
- **Fail-safe** — writes `status: "failed"` to dev.json, `nika switch` shows the error

## Implementation Plan

### Task 0: Create `build.rs` for version metadata

**File:** `tools/nika/build.rs` (NEW)

Embeds at compile time:
- `NIKA_GIT_HASH` — `git rev-parse --short HEAD`
- `NIKA_BUILD_TIMESTAMP` — UTC ISO 8601
- `NIKA_BUILD_CHANNEL` — from env var `NIKA_BUILD_CHANNEL` (default: `"dev"`)

```rust
use std::process::Command;

fn main() {
    // Git hash
    let hash = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default()
        .trim()
        .to_string();
    println!("cargo:rustc-env=NIKA_GIT_HASH={hash}");

    // Build timestamp
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    println!("cargo:rustc-env=NIKA_BUILD_TIMESTAMP={now}");

    // Channel (set by hook or CI, default "dev")
    let channel = std::env::var("NIKA_BUILD_CHANNEL").unwrap_or_else(|_| "dev".into());
    println!("cargo:rustc-env=NIKA_BUILD_CHANNEL={channel}");

    // Only rerun on git HEAD change or env change
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-env-changed=NIKA_BUILD_CHANNEL");
}
```

**Modify:** `tools/nika/src/main.rs` — update `#[command(version)]` to use long_version:

```rust
#[command(version, long_version = long_version())]
```

```rust
fn long_version() -> &'static str {
    concat!(
        env!("CARGO_PKG_VERSION"),
        " (", env!("NIKA_BUILD_CHANNEL"),
        ", ", env!("NIKA_GIT_HASH"),
        ", built ", env!("NIKA_BUILD_TIMESTAMP"), ")"
    )
}
```

**Tests:** Unit test that `long_version()` contains expected fields.

**Verify:** `cargo build && ./target/debug/nika --version` shows extended info.

---

### Task 1: Create `switch.rs` handler module

**File:** `tools/nika-cli/src/switch.rs` (NEW, ~350 lines)

#### 1a. Data types

```rust
use clap::Subcommand;
use serde::{Deserialize, Serialize};

#[derive(Subcommand)]
pub enum SwitchAction {
    /// Switch to dev channel
    Dev,
    /// Switch to release channel
    Release,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BuildMeta {
    pub version: String,
    pub hash: String,
    pub built_at: String,
    pub status: String,
}

#[derive(Debug)]
pub struct ChannelInfo {
    pub name: String,           // "dev" or "release"
    pub version: String,        // "0.51.0"
    pub detail: String,         // "abc1234 · 2min ago" or "homebrew"
    pub binary_path: PathBuf,   // actual binary location
    pub available: bool,        // binary exists and is executable
}
```

#### 1b. Channel detection

```rust
fn detect_channels() -> (Option<ChannelInfo>, Option<ChannelInfo>) {
    let release = detect_release();  // find /opt/homebrew/bin/nika, run --version
    let dev = detect_dev();          // read ~/.nika/builds/dev.json + check binary
    (release, dev)
}

fn detect_release() -> Option<ChannelInfo> {
    // 1. which::which("nika") in /opt/homebrew/bin/
    // 2. Run it with --version, parse output
    // 3. Build ChannelInfo { name: "release", detail: "homebrew", ... }
}

fn detect_dev() -> Option<ChannelInfo> {
    // 1. Check ~/.nika/bin/nika-dev exists
    // 2. Read ~/.nika/builds/dev.json for metadata
    // 3. Compute relative time from built_at
    // 4. Build ChannelInfo { name: "dev", detail: "abc1234 · 2min ago", ... }
}

fn active_channel() -> String {
    // Read ~/.nika/channel, default "release"
    std::fs::read_to_string(nika_dir().join("channel"))
        .unwrap_or_else(|_| "release".into())
        .trim().to_string()
}
```

#### 1c. Switch logic

```rust
fn do_switch(target: &str) -> Result<(), NikaError> {
    let nika_dir = dirs::home_dir().unwrap().join(".nika");
    let symlink_path = nika_dir.join("bin/nika");
    let current = active_channel();

    if current == target {
        // Already on this channel — show status instead
        return render_status();
    }

    let target_binary = match target {
        "dev" => nika_dir.join("bin/nika-dev"),
        "release" => find_homebrew_nika()?,
        _ => return Err(NikaError::validation("Unknown channel")),
    };

    if !target_binary.exists() {
        return Err(NikaError::validation(format!(
            "{target} channel not available. Run `nika switch --setup`"
        )));
    }

    // Atomic swap: remove old symlink, create new one
    let _ = std::fs::remove_file(&symlink_path);
    std::os::unix::fs::symlink(&target_binary, &symlink_path)?;

    // Write channel file
    std::fs::write(nika_dir.join("channel"), target)?;

    // Animate transition
    animate_transition(&current, target)?;

    Ok(())
}
```

#### 1d. Setup logic

```rust
pub async fn do_setup(quiet: bool) -> Result<(), NikaError> {
    // 1. Create ~/.nika/bin/ and ~/.nika/builds/
    // 2. Detect homebrew binary
    // 3. Build dev (cargo build --release in repo)
    // 4. Copy to ~/.nika/bin/nika-dev
    // 5. Create symlink
    // 6. Install git post-commit hook
    // 7. Write channel file
    // 8. Prompt for PATH addition
}
```

#### 1e. Force build logic

```rust
pub async fn do_build(quiet: bool) -> Result<(), NikaError> {
    // 1. Find repo root (walk up from cwd looking for tools/nika/Cargo.toml)
    // 2. Show spinner: "Building nika-dev from main..."
    // 3. Run cargo build --release
    // 4. Copy binary to ~/.nika/bin/nika-dev
    // 5. Write dev.json metadata
    // 6. Show banner with version + build time
}
```

**Tests:**
- `test_detect_channels_missing_dir` — no ~/.nika/ returns None/None
- `test_active_channel_default` — missing file returns "release"
- `test_build_meta_parse` — deserialize dev.json

**Verify:** `cargo test -p nika-cli --lib -- switch`

---

### Task 2: Animation module

**File:** `tools/nika-cli/src/switch.rs` (bottom section, or separate `switch_ui.rs`)

Uses: `colored`, `indicatif`, `std::thread::sleep`

#### 2a. Solarized colors for `colored` crate

```rust
use colored::{Color, Colorize, CustomColor};

const CYAN: CustomColor = CustomColor { r: 42, g: 161, b: 152 };
const MAGENTA: CustomColor = CustomColor { r: 211, g: 54, b: 130 };
const GREEN: CustomColor = CustomColor { r: 133, g: 153, b: 0 };
const DIM: CustomColor = CustomColor { r: 88, g: 110, b: 117 };
const YELLOW: CustomColor = CustomColor { r: 181, g: 137, b: 0 };
const RED: CustomColor = CustomColor { r: 220, g: 50, b: 47 };
```

#### 2b. Status renderer

```rust
fn render_status() -> Result<(), NikaError> {
    let (release, dev) = detect_channels();
    let active = active_channel();

    // Header with star sparkle
    println!();
    println!("{}", "               .  *  .".custom_color(DIM));
    println!("{}", "            *  NIKA  *".custom_color(YELLOW).bold());
    println!("{}", "               '  *  '".custom_color(DIM));
    println!();

    // Channel list
    render_channel_line("release", &release, &active);
    render_connector(&active);
    render_channel_line("dev", &dev, &active);
    println!();

    Ok(())
}
```

#### 2c. Transition animation

```rust
fn animate_transition(from: &str, to: &str) -> Result<(), NikaError> {
    let (release, dev) = detect_channels();

    // Frame 1: current state (150ms)
    print_frame_current(from, &release, &dev);
    std::thread::sleep(Duration::from_millis(150));
    clear_lines(3);

    // Frame 2: spark moving (150ms)
    print_frame_spark(from, to);
    std::thread::sleep(Duration::from_millis(150));
    clear_lines(4);

    // Frame 3: new state (150ms)
    print_frame_current(to, &release, &dev);
    std::thread::sleep(Duration::from_millis(200));
    clear_lines(3);

    // Frame 4: confirmation banner
    let target = if to == "dev" { &dev } else { &release };
    print_banner(to, target);

    Ok(())
}

fn clear_lines(n: usize) {
    for _ in 0..n {
        print!("\x1b[A\x1b[2K"); // cursor up + clear line
    }
}
```

#### 2d. Build progress

```rust
fn show_build_progress(repo_dir: &Path) -> Result<Duration, NikaError> {
    let spinner = indicatif::ProgressBar::new_spinner();
    spinner.set_style(
        indicatif::ProgressStyle::default_spinner()
            .tick_strings(&["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"])
            .template("{spinner:.magenta} {msg}")
            .unwrap(),
    );
    spinner.set_message("Building nika-dev from main...");

    let start = Instant::now();

    // Spawn cargo build, pipe stderr for crate names
    let mut child = Command::new("cargo")
        .args(["build", "--release"])
        .current_dir(repo_dir.join("tools/nika"))
        .stderr(Stdio::piped())
        .spawn()?;

    // Parse stderr for "Compiling crate_name vX.Y.Z" lines
    // Update spinner message with current crate
    // ...

    let status = child.wait()?;
    let duration = start.elapsed();
    spinner.finish_and_clear();

    if !status.success() {
        return Err(NikaError::execution("cargo build failed"));
    }

    Ok(duration)
}
```

**Tests:**
- `test_clear_lines_escape_sequence` — verify ANSI output
- `test_render_status_no_channels` — graceful when nothing available
- `test_render_status_both_channels` — snapshot test with `insta`

**Verify:** Manual — `cargo run -- switch` in terminal, visually confirm colors/layout.

---

### Task 3: Wire into CLI

#### 3a. Register module in lib.rs

**File:** `tools/nika-cli/src/lib.rs`

Add: `pub mod switch;`

#### 3b. Add Commands variant

**File:** `tools/nika/src/main.rs` — in `enum Commands`

```rust
/// Switch between dev and release channels
#[command(next_help_heading = "SYSTEM")]
Switch {
    /// Channel to switch to (dev, release)
    #[command(subcommand)]
    action: Option<cli::switch::SwitchAction>,

    /// One-time setup (create dirs, build dev, install hook)
    #[arg(long)]
    setup: bool,

    /// Force rebuild dev binary now
    #[arg(long)]
    build: bool,
},
```

#### 3c. Add dispatch

**File:** `tools/nika/src/main.rs` — in `match cli.command`

```rust
Some(Commands::Switch { action, setup, build }) => {
    if setup {
        cli::switch::do_setup(quiet).await
    } else if build {
        cli::switch::do_build(quiet).await
    } else {
        cli::switch::handle_switch_command(action, quiet)
    }
}
```

**Tests:** Integration test — `nika switch --help` outputs expected text.

**Verify:** `cargo build && ./target/debug/nika switch --help`

---

### Task 4: Enhanced `--version` output

**File:** `tools/nika/src/main.rs`

Update version display to show channel-aware output:

```rust
fn long_version() -> String {
    let version = env!("CARGO_PKG_VERSION");
    let channel = env!("NIKA_BUILD_CHANNEL");
    let hash = env!("NIKA_GIT_HASH");
    let ts = env!("NIKA_BUILD_TIMESTAMP");

    // Parse timestamp to relative time
    let built_ago = relative_time(ts);

    match channel {
        "dev" => format!("{version}-dev ({hash}, built {built_ago})"),
        "release" => format!("{version} (release, homebrew)"),
        _ => format!("{version} ({channel}, {hash})"),
    }
}
```

**Tests:** `test_long_version_dev`, `test_long_version_release`

**Verify:** `NIKA_BUILD_CHANNEL=dev cargo build && ./target/debug/nika --version`

---

### Task 5: Git hook installer

**File:** `tools/nika-cli/src/switch.rs` (part of `do_setup()`)

```rust
fn install_git_hook(repo_dir: &Path) -> Result<(), NikaError> {
    let hooks_dir = repo_dir.join(".git/hooks");
    let hook_path = hooks_dir.join("post-commit");

    // Check for existing hook — don't overwrite user's hook
    if hook_path.exists() {
        let content = std::fs::read_to_string(&hook_path)?;
        if !content.contains("# Nika auto-build") {
            // Append our hook to existing
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(&hook_path)?;
            writeln!(file, "\n{}", HOOK_SCRIPT)?;
            return Ok(());
        }
        // Already installed
        return Ok(());
    }

    // Write new hook
    std::fs::write(&hook_path, format!("#!/bin/sh\n{HOOK_SCRIPT}"))?;

    // Make executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook_path, std::fs::Permissions::from_mode(0o755))?;
    }

    Ok(())
}
```

**Tests:**
- `test_hook_install_new` — creates hook from scratch
- `test_hook_install_append` — appends to existing hook
- `test_hook_install_idempotent` — doesn't duplicate on second install

**Verify:** Run `nika switch --setup`, check `.git/hooks/post-commit` exists and is executable.

---

### Task 6: E2E tests

**File:** `tools/nika-cli/tests/switch_e2e.rs` (NEW)

```rust
#[test]
fn test_switch_status_no_setup() {
    // Run `nika switch` without setup
    // Expect helpful error: "run nika switch --setup first"
}

#[test]
fn test_switch_setup_creates_dirs() {
    // With temp HOME, run setup
    // Verify ~/.nika/bin/ and ~/.nika/builds/ exist
    // Verify channel file is written
}

#[test]
fn test_switch_dev_creates_symlink() {
    // Setup temp env with fake binaries
    // Switch to dev
    // Verify symlink points to nika-dev
    // Verify channel file reads "dev"
}

#[test]
fn test_switch_release_creates_symlink() {
    // Setup temp env with fake homebrew binary
    // Switch to release
    // Verify symlink points to homebrew path
}

#[test]
fn test_switch_already_on_channel() {
    // Switch to dev when already on dev
    // Should show status, not error
}

#[test]
fn test_switch_missing_dev_binary() {
    // Switch to dev when nika-dev doesn't exist
    // Expect error: "run nika switch --build first"
}

#[test]
fn test_build_meta_deserialization() {
    let json = r#"{"version":"0.51.0","hash":"abc1234","built_at":"2026-03-30T14:23:00Z","status":"ok"}"#;
    let meta: BuildMeta = serde_json::from_str(json).unwrap();
    assert_eq!(meta.version, "0.51.0");
    assert_eq!(meta.status, "ok");
}

#[test]
fn test_build_meta_failed_status() {
    let json = r#"{"status":"failed","built_at":"2026-03-30T14:23:00Z"}"#;
    // Should handle missing version/hash gracefully
}

#[test]
fn test_hook_script_debounce() {
    // Verify hook script contains lock file check
    // Verify 10s debounce threshold
}

#[test]
fn test_relative_time_formatting() {
    // 30s ago -> "just now"
    // 120s ago -> "2min ago"
    // 3600s ago -> "1h ago"
    // 86400s ago -> "1d ago"
}
```

**Verify:** `cargo test -p nika-cli --test switch_e2e`

---

## Task Dependency Graph

```
Task 0 (build.rs)
    │
    ├──> Task 1 (switch.rs handler)
    │        │
    │        ├──> Task 2 (animation)
    │        │
    │        └──> Task 5 (git hook)
    │
    └──> Task 4 (--version)

Task 3 (wire CLI) depends on Task 1

Task 6 (E2E) depends on ALL above
```

Parallel batches:
1. **Batch 1:** Task 0 (build.rs)
2. **Batch 2:** Task 1 + Task 4 (in parallel)
3. **Batch 3:** Task 2 + Task 5 (in parallel)
4. **Batch 4:** Task 3 (wire CLI)
5. **Batch 5:** Task 6 (E2E tests)

## Edge Cases

| Scenario | Behavior |
|----------|----------|
| Homebrew not installed | `"release channel unavailable — brew install supernovae-st/tap/nika"` |
| Dev never built | `"run nika switch --build first"` |
| Build in progress | `"build in progress since 30s ago"` (read lock file) |
| Same version both channels | Display both, git hash differentiates |
| No `~/.nika/` dir | `"run nika switch --setup first"` |
| Last build failed | Show `✗ last build failed (see ~/.nika/builds/last.log)` in red |
| Hook already exists | Append (don't overwrite user's existing post-commit hook) |
| Non-macOS platform | Skip Homebrew detection, use `~/.cargo/bin/nika` as release |
| `--quiet` flag | Skip animation, just switch and print one line |

## Files to Create/Modify

| Action | File | Lines (est.) |
|--------|------|-------------|
| CREATE | `tools/nika/build.rs` | ~25 |
| CREATE | `tools/nika-cli/src/switch.rs` | ~350 |
| CREATE | `tools/nika-cli/tests/switch_e2e.rs` | ~120 |
| MODIFY | `tools/nika-cli/src/lib.rs` | +1 line |
| MODIFY | `tools/nika/src/main.rs` | +20 lines (Commands + dispatch + long_version) |

**Total:** ~520 lines new code, 2 new files, 2 modified files.
**Zero new dependencies** — uses `colored`, `indicatif`, `cliclack`, `serde_json`, `chrono`, `dirs`, `which` already in nika-cli.
