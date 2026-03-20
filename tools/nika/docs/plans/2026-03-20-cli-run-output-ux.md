# CLI Output UX — Implementation Plan (`nika run` + `nika check`)

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Overhaul both `nika run` and `nika check` CLI output with the "Cosmic" icon palette, rich telemetry, and distinct visual identities. `nika run` gets an append-only event stream (replacing buggy LiveDag). `nika check` gets a pre-flight checklist with the existing advanced DAG boxes upgraded.

**Architecture:** Split `display.rs` (764 lines) into a `display/` module with dedicated renderers: header, event stream, output preview, summary, check checklist, and upgraded DAG boxes. The runner emits events to a new `CliRenderer` that formats and prints them as append-only lines (no ANSI cursor movement). Verbosity is controlled by `--detail` flag (max/default/min/json).

**Tech Stack:** `colored` 2.1 (already dep), `unicode-width` 0.2 (move from optional to always-on), `terminal_size` 0.4 (move from optional to always-on). No new crates.

**Two distinct philosophies:**

| | `nika run` | `nika check` |
|--|-----------|-------------|
| **Feeling** | Mission control — streaming telemetry | Pre-flight checklist — instant validation |
| **Output** | Append-only event stream (seconds/minutes) | Instant checklist + DAG boxes (milliseconds) |
| **Data** | 41 event types, tokens, cost, Gantt | 6 validation phases, task properties, MCP params |
| **DAG** | Static one-line DAG (compact, printed once) | Advanced box DAG with ╔═╗ borders + metadata |
| **Summary** | Rich: timeline, provider table, token bars | Compact: pass/fail + error codes |
| **Colors** | Mixed verb colors, sparklines, gradients | Binary green/red, verb colors in DAG boxes |

---

## Icon Palette — "Cosmic" (Palette B)

```
VERBS                              STATUS
✧  infer    bright magenta         ○  pending     dimmed
⎈  exec     bright yellow          ●  running     bright white
☄  fetch    bright cyan            ✓  success     bright green
⊛  invoke   bright green           ✗  failed      bright red
❋  agent    bright red             ⊘  skipped     dimmed

SUBSYSTEMS
△  provider    blue                ◎  artifact    cyan
⊞  mcp         green               ▣  media       magenta
⛨  guardrail   yellow              ⬡  structured  blue
◐  vision      purple              ⇄  http        cyan
↯  retry       yellow              ◈  agent meta  red
▪  log/custom  dimmed
```

## Verbosity Levels

| Flag | Name | What's shown |
|------|------|-------------|
| `--detail max` | Maximum (default) | Everything: all events, previews, sparklines, full summary |
| `--detail default` | Default | Task lifecycle + provider + MCP + errors. No sub-events like TemplateResolved |
| `--detail min` | Minimal | Task ✓/✗ lines only + summary |
| `--detail json` | JSON | Raw NDJSON events to stdout (for piping) |
| `--quiet` | Quiet | Final output only (unchanged) |

> **Note:** `--detail max` is the DEFAULT for `nika run` in this design. The old behavior (LiveDag boxes) is removed entirely.

## Visual Elements Reference

1. **Sparkline bars** `▁▂▃▄▅▆▇█` — token volume per provider call
2. **Budget bar** `░▓` — context budget usage (green <70%, yellow 70-90%, red >90%)
3. **Cost bar** `▪` — relative cost per task in summary
4. **Gantt timeline** `█░` — parallel execution visualization
5. **Token bars** `█░` — in/out/cache ratio in summary
6. **Color swatches** `████` — truecolor dominant colors from media
7. **Output preview** `╭╌╌╌╌╮` — dashed box with syntax-highlighted JSON/Markdown
8. **Layer separators** `─ ─ ─ layer N ─ ─ ─` — between DAG layers in event stream
9. **Provider breakdown table** — per-call token/cost audit
10. **Truecolor image preview** — half-block pixels `▀▄█` (with graceful fallback)

---

## Task 1: Split `display.rs` into `display/` module

**Files:**
- Rename: `src/display.rs` → `src/display/legacy.rs`
- Create: `src/display/mod.rs`
- Create: `src/display/icons.rs`
- Create: `src/display/colors.rs`
- Modify: `src/lib.rs` (display module declaration unchanged — Rust resolves `display/mod.rs`)

**Step 1: Create `src/display/mod.rs` that re-exports everything from legacy**

```rust
//! CLI display — header, event stream, summary renderers.
//!
//! ## Module structure
//! - `legacy` — Original display functions (to be gradually replaced)
//! - `icons` — Cosmic icon palette (verb, status, subsystem)
//! - `colors` — Color constants and helpers

mod legacy;
pub mod icons;
pub mod colors;

// Re-export legacy API so nothing breaks
pub use legacy::*;
```

**Step 2: Create `src/display/icons.rs`**

```rust
//! Cosmic icon palette — Unicode icons for verbs, status, and subsystems.
//!
//! Design: Every icon is exactly 1 terminal column wide (Unicode category).
//! Colors are applied via the `colored` crate at call sites.

use colored::{ColoredString, Colorize};

// ═══════════════════════════════════════════
// VERB ICONS
// ═══════════════════════════════════════════

/// Verb icon with its signature color.
pub fn verb(v: &str) -> ColoredString {
    match v {
        "infer" => "\u{2727}".magenta(),  // ✧ four-pointed star
        "exec"  => "\u{2388}".yellow(),   // ⎈ helm
        "fetch" => "\u{2604}".cyan(),     // ☄ comet
        "invoke"=> "\u{229B}".green(),    // ⊛ circled asterisk
        "agent" => "\u{274B}".red(),      // ❋ heavy eight teardrop-spoked propeller
        _       => "\u{25CF}".white(),    // ● fallback
    }
}

/// Verb icon without color (for DAG rendering).
pub fn verb_plain(v: &str) -> &'static str {
    match v {
        "infer"  => "\u{2727}", // ✧
        "exec"   => "\u{2388}", // ⎈
        "fetch"  => "\u{2604}", // ☄
        "invoke" => "\u{229B}", // ⊛
        "agent"  => "\u{274B}", // ❋
        _        => "\u{25CF}", // ●
    }
}

// ═══════════════════════════════════════════
// STATUS ICONS
// ═══════════════════════════════════════════

pub fn pending()  -> ColoredString { "\u{25CB}".dimmed()      } // ○
pub fn running()  -> ColoredString { "\u{25CF}".white().bold() } // ●
pub fn success()  -> ColoredString { "\u{2713}".green().bold() } // ✓
pub fn failed()   -> ColoredString { "\u{2717}".red().bold()   } // ✗
pub fn skipped()  -> ColoredString { "\u{2298}".dimmed()       } // ⊘

// ═══════════════════════════════════════════
// SUBSYSTEM ICONS
// ═══════════════════════════════════════════

pub fn provider()   -> ColoredString { "\u{25B3}".blue()    } // △
pub fn mcp()        -> ColoredString { "\u{229E}".green()   } // ⊞
pub fn guardrail()  -> ColoredString { "\u{26E8}".yellow()  } // ⛨
pub fn artifact()   -> ColoredString { "\u{25CE}".cyan()    } // ◎
pub fn media()      -> ColoredString { "\u{25A3}".magenta() } // ▣
pub fn structured() -> ColoredString { "\u{2B21}".blue()    } // ⬡
pub fn vision()     -> ColoredString { "\u{25D0}".purple()  } // ◐
pub fn http()       -> ColoredString { "\u{21C4}".cyan()    } // ⇄
pub fn retry()      -> ColoredString { "\u{21AF}".yellow()  } // ↯
pub fn agent_meta() -> ColoredString { "\u{25C8}".red()     } // ◈
pub fn log()        -> ColoredString { "\u{25AA}".dimmed()   } // ▪
```

**Step 3: Create `src/display/colors.rs`**

```rust
//! Color helpers — formatting, syntax highlighting, sparklines.

use colored::{ColoredString, Colorize};

/// Format elapsed time with color based on duration.
/// - < 1s: green (fast)
/// - 1-5s: yellow (moderate)
/// - > 5s: red (slow)
pub fn duration(secs: f32) -> ColoredString {
    let text = if secs < 0.001 {
        format!("{:.0}µs", secs * 1_000_000.0)
    } else if secs < 1.0 {
        format!("{:.0}ms", secs * 1000.0)
    } else if secs < 60.0 {
        format!("{:.1}s", secs)
    } else {
        let m = (secs / 60.0).floor() as u32;
        let s = secs % 60.0;
        format!("{}m{:.1}s", m, s)
    };
    if secs < 1.0 {
        text.green()
    } else if secs < 5.0 {
        text.yellow()
    } else {
        text.red()
    }
}

/// Format a token count: 842 → "842", 1200 → "1.2k", 15000 → "15k"
pub fn tokens(n: u64) -> String {
    if n < 1000 {
        n.to_string()
    } else if n < 10_000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else {
        format!("{}k", n / 1000)
    }
}

/// Sparkline bar for token volume: maps value to ▁▂▃▄▅▆▇█
pub fn sparkline(value: u64, max: u64) -> ColoredString {
    const CHARS: &[char] = &['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let ratio = if max == 0 { 0.0 } else { value as f64 / max as f64 };
    let idx = (ratio * 7.0).round().min(7.0) as usize;
    let bar: String = (0..8).map(|i| if i <= idx { CHARS[idx] } else { '░' }).collect();
    bar.blue()
}

/// Budget bar: shows context budget usage with color thresholds.
/// Returns something like: ░░░░▓▓▓▓▓▓▓░░░░░░░ 72%
pub fn budget_bar(pct: f32, width: usize) -> String {
    let filled = ((pct / 100.0) * width as f32).round() as usize;
    let empty = width.saturating_sub(filled);
    let bar = format!("{}{}", "▓".repeat(filled), "░".repeat(empty));
    let colored_bar = if pct < 70.0 {
        bar.green()
    } else if pct < 90.0 {
        bar.yellow()
    } else {
        bar.red()
    };
    let pct_str = format!("{}%", pct.round() as u32);
    let colored_pct = if pct < 70.0 {
        pct_str.green()
    } else if pct < 90.0 {
        pct_str.yellow()
    } else {
        pct_str.red()
    };
    format!("{} {}", colored_bar, colored_pct)
}

/// Format cost in USD: $0.0042
pub fn cost(usd: f64) -> ColoredString {
    if usd < 0.001 {
        format!("${:.4}", usd).dimmed()
    } else if usd < 0.01 {
        format!("${:.3}", usd).yellow()
    } else {
        format!("${:.2}", usd).yellow().bold()
    }
}

/// TTFT with color: green <200ms, yellow 200-500ms, red >500ms
pub fn ttft(ms: u64) -> ColoredString {
    let text = format!("{}ms", ms);
    if ms < 200 {
        text.green()
    } else if ms < 500 {
        text.yellow()
    } else {
        text.red()
    }
}

/// Syntax-highlight a JSON string (first line only, truncated).
/// Keys in blue, strings in green, numbers in yellow, booleans in magenta.
pub fn json_preview(json: &str, max_chars: usize) -> String {
    let truncated = if json.len() > max_chars {
        format!("{}…", &json[..max_chars])
    } else {
        json.to_string()
    };
    // Simple state machine for JSON colorization
    let mut result = String::with_capacity(truncated.len() * 2);
    let mut in_key = false;
    let mut in_string = false;
    let mut after_colon = false;

    for ch in truncated.chars() {
        match ch {
            '"' if !in_string && !after_colon => {
                in_key = true;
                in_string = true;
                result.push_str(&format!("\x1b[34m\"")); // blue for key
            }
            '"' if !in_string && after_colon => {
                in_string = true;
                result.push_str(&format!("\x1b[32m\"")); // green for value string
            }
            '"' if in_string => {
                result.push('"');
                result.push_str("\x1b[0m");
                in_string = false;
                if in_key {
                    in_key = false;
                }
                after_colon = false;
            }
            ':' if !in_string => {
                after_colon = true;
                result.push_str(&format!("\x1b[0m:"));
            }
            ',' | '{' | '}' | '[' | ']' if !in_string => {
                after_colon = false;
                result.push_str(&format!("\x1b[0m{}", ch));
            }
            c if !in_string && (c.is_ascii_digit() || c == '.' || c == '-') => {
                result.push_str(&format!("\x1b[33m{}\x1b[0m", c)); // yellow for numbers
            }
            _ => result.push(ch),
        }
    }
    result.push_str("\x1b[0m"); // reset
    result
}

/// Markdown preview: bold headers, dimmed body.
pub fn markdown_preview(md: &str, max_lines: usize) -> Vec<String> {
    md.lines()
        .take(max_lines)
        .map(|line| {
            if line.starts_with('#') {
                format!("{}", line.bold().white())
            } else {
                line.to_string()
            }
        })
        .collect()
}
```

**Step 4: Move `src/display.rs` → `src/display/legacy.rs`**

```bash
mkdir -p src/display
mv src/display.rs src/display/legacy.rs
```

**Step 5: Run tests to verify nothing breaks**

```bash
cargo test --lib -- display 2>&1 | head -20
cargo check 2>&1 | tail -5
```

Expected: All tests pass. `cargo check` succeeds because `mod.rs` re-exports everything via `pub use legacy::*`.

**Step 6: Commit**

```bash
git add src/display/
git commit -m "refactor(display): split display.rs into display/ module with icons + colors

Introduce Cosmic icon palette (✧⎈☄⊛❋) and color helpers.
Legacy display functions re-exported for backward compatibility.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## Task 2: Make `unicode-width` and `terminal_size` always-on deps

**Files:**
- Modify: `Cargo.toml` (lines 23, 186-187)

**Step 1: Move deps from optional to required**

In `Cargo.toml`, change:

```toml
# FROM (optional, TUI-only):
unicode-width = { version = "0.2", optional = true }
terminal_size = { version = "0.4", optional = true }

# TO (always available):
unicode-width = "0.2"
terminal_size = "0.4"
```

Remove `dep:unicode-width` and `dep:terminal_size` from the `tui` feature list (line 23).

**Step 2: Verify build**

```bash
cargo check 2>&1 | tail -5
```

Expected: Succeeds.

**Step 3: Commit**

```bash
git add Cargo.toml
git commit -m "chore: make unicode-width and terminal_size always-on deps

Needed for CLI event stream output width calculations.
Previously only available with --features tui.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## Task 3: Create `DetailLevel` enum and CLI flag

**Files:**
- Create: `src/display/detail.rs`
- Modify: `src/display/mod.rs`
- Modify: `src/main.rs` (lines 131-141, ~486)

**Step 1: Write tests for DetailLevel parsing**

Add to `src/display/detail.rs`:

```rust
//! Verbosity control for CLI output.

use std::fmt;
use std::str::FromStr;

/// Controls how much telemetry is displayed during `nika run`.
///
/// Levels (most → least verbose):
/// - `max`: All events, previews, sparklines, full summary (DEFAULT)
/// - `default`: Task lifecycle + provider + MCP + errors
/// - `min`: Task ✓/✗ lines only + compact summary
/// - `json`: Raw NDJSON events to stdout
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailLevel {
    Max,
    Default,
    Min,
    Json,
}

impl DetailLevel {
    /// Whether to show sub-events (provider calls, context assembly, etc.)
    pub fn show_sub_events(&self) -> bool {
        matches!(self, Self::Max | Self::Default)
    }

    /// Whether to show output preview boxes
    pub fn show_previews(&self) -> bool {
        matches!(self, Self::Max)
    }

    /// Whether to show sparklines and budget bars
    pub fn show_sparklines(&self) -> bool {
        matches!(self, Self::Max)
    }

    /// Whether to show the full summary with timeline + provider table
    pub fn show_full_summary(&self) -> bool {
        matches!(self, Self::Max | Self::Default)
    }

    /// Whether to show layer separators in event stream
    pub fn show_layer_separators(&self) -> bool {
        matches!(self, Self::Max)
    }

    /// Whether to output raw NDJSON instead of formatted text
    pub fn is_json(&self) -> bool {
        matches!(self, Self::Json)
    }

    /// Whether to show TemplateResolved events (very verbose)
    pub fn show_template_events(&self) -> bool {
        matches!(self, Self::Max)
    }
}

impl Default for DetailLevel {
    fn default() -> Self {
        Self::Max
    }
}

impl fmt::Display for DetailLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Max => write!(f, "max"),
            Self::Default => write!(f, "default"),
            Self::Min => write!(f, "min"),
            Self::Json => write!(f, "json"),
        }
    }
}

impl FromStr for DetailLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "max" => Ok(Self::Max),
            "default" => Ok(Self::Default),
            "min" => Ok(Self::Min),
            "json" => Ok(Self::Json),
            _ => Err(format!(
                "invalid detail level '{}': expected max, default, min, or json",
                s
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detail_level_from_str() {
        assert_eq!(DetailLevel::from_str("max").unwrap(), DetailLevel::Max);
        assert_eq!(DetailLevel::from_str("default").unwrap(), DetailLevel::Default);
        assert_eq!(DetailLevel::from_str("min").unwrap(), DetailLevel::Min);
        assert_eq!(DetailLevel::from_str("json").unwrap(), DetailLevel::Json);
        assert_eq!(DetailLevel::from_str("MAX").unwrap(), DetailLevel::Max);
        assert!(DetailLevel::from_str("invalid").is_err());
    }

    #[test]
    fn test_default_is_max() {
        assert_eq!(DetailLevel::default(), DetailLevel::Max);
    }

    #[test]
    fn test_visibility_max() {
        let d = DetailLevel::Max;
        assert!(d.show_sub_events());
        assert!(d.show_previews());
        assert!(d.show_sparklines());
        assert!(d.show_full_summary());
        assert!(d.show_layer_separators());
        assert!(d.show_template_events());
        assert!(!d.is_json());
    }

    #[test]
    fn test_visibility_min() {
        let d = DetailLevel::Min;
        assert!(!d.show_sub_events());
        assert!(!d.show_previews());
        assert!(!d.show_sparklines());
        assert!(!d.show_full_summary());
        assert!(!d.show_layer_separators());
        assert!(!d.show_template_events());
        assert!(!d.is_json());
    }

    #[test]
    fn test_visibility_json() {
        let d = DetailLevel::Json;
        assert!(!d.show_sub_events());
        assert!(d.is_json());
    }
}
```

**Step 2: Register in `src/display/mod.rs`**

Add `pub mod detail;` and `pub use detail::DetailLevel;`.

**Step 3: Add `--detail` flag to CLI**

In `src/main.rs`, around line 137 (after `quiet`), add:

```rust
    /// Detail level for run output: max (default), default, min, json
    #[arg(long, default_value = "max")]
    detail: crate::display::DetailLevel,
```

And pass it through `run_workflow()` function signature.

**Step 4: Run tests**

```bash
cargo test --lib -- detail 2>&1 | head -20
cargo check 2>&1 | tail -5
```

**Step 5: Commit**

```bash
git add src/display/detail.rs src/display/mod.rs src/main.rs
git commit -m "feat(display): add DetailLevel enum with --detail max|default|min|json flag

Default is 'max' showing all telemetry. Controls visibility of
sub-events, previews, sparklines, summaries, and layer separators.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## Task 4: Create `CliRenderer` — the event stream engine

**Files:**
- Create: `src/display/renderer.rs`
- Modify: `src/display/mod.rs`

This is the core engine. It receives `Event` structs and prints formatted lines.

**Step 1: Create `CliRenderer` struct**

```rust
//! CliRenderer — append-only event stream renderer.
//!
//! Receives Event structs from the runner and prints formatted lines.
//! NO ANSI cursor movement. Every print is a simple println!().

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use colored::Colorize;
use serde_json::Value;

use crate::display::{colors, icons, DetailLevel};
use crate::event::{AgentTurnMetadata, EventKind};

/// Accumulated stats for the summary.
#[derive(Debug, Default)]
pub struct RunStats {
    pub task_count: usize,
    pub tasks_passed: usize,
    pub tasks_failed: usize,
    pub tasks_skipped: usize,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_tokens: u64,
    pub total_cost: f64,
    pub ttft_values: Vec<u64>,
    pub mcp_calls: u32,
    pub mcp_retries: u32,
    pub mcp_errors: u32,
    pub media_stored: u32,
    pub media_bytes: u64,
    pub media_dedup: u32,
    pub artifacts_count: u32,
    pub artifacts_bytes: u64,
    pub guardrails_passed: u32,
    pub guardrails_failed: u32,
    pub guardrails_escalations: u32,
    pub structured_attempts: u32,
    pub structured_success_layer: Option<u8>,
    /// Per-task timing: (task_id, verb, start_offset_ms, duration_ms)
    pub task_timeline: Vec<(String, String, u64, u64)>,
    /// Per-provider call: (task_id, in, out, cache, ttft_ms, cost)
    pub provider_calls: Vec<ProviderCallStat>,
}

#[derive(Debug)]
pub struct ProviderCallStat {
    pub task_id: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_tokens: u64,
    pub ttft_ms: Option<u64>,
    pub cost: f64,
}

pub struct CliRenderer {
    detail: DetailLevel,
    start: Instant,
    stats: RunStats,
    /// Track which DAG layer each task belongs to (for layer separators)
    task_layers: HashMap<Arc<str>, usize>,
    /// Current layer being displayed
    current_layer: usize,
    /// Terminal width for layout
    term_width: u16,
    /// Track task start times for timeline
    task_starts: HashMap<String, u64>,
    /// Workflow start timestamp for offset calculation
    workflow_start_ms: u64,
}

impl CliRenderer {
    pub fn new(detail: DetailLevel) -> Self {
        let term_width = terminal_size::terminal_size()
            .map(|(w, _)| w.0)
            .unwrap_or(80);

        Self {
            detail,
            start: Instant::now(),
            stats: RunStats::default(),
            task_layers: HashMap::new(),
            current_layer: 0,
            term_width,
            task_starts: HashMap::new(),
            workflow_start_ms: 0,
        }
    }

    /// Set task-to-layer mapping (called after DAG analysis).
    pub fn set_task_layers(&mut self, layers: HashMap<Arc<str>, usize>) {
        self.task_layers = layers;
    }

    /// Format timestamp offset from workflow start.
    fn ts(&self) -> String {
        let elapsed = self.start.elapsed().as_secs_f32();
        format!("{:>6}", format!("+{:.1}s", elapsed)).dimmed().to_string()
    }

    /// Main entry point: render a single event.
    pub fn render(&mut self, event: &crate::event::Event) {
        if self.detail.is_json() {
            // JSON mode: print raw NDJSON
            if let Ok(json) = serde_json::to_string(event) {
                println!("{}", json);
            }
            return;
        }

        match &event.kind {
            // ═══════════════════════════════════════
            // WORKFLOW LEVEL
            // ═══════════════════════════════════════
            EventKind::WorkflowStarted { .. } => {
                self.workflow_start_ms = event.timestamp_ms;
                // Header already printed by main.rs
            }
            EventKind::WorkflowPaused => {
                println!("{} {} paused", self.ts(), "⏸".yellow());
            }
            EventKind::WorkflowResumed => {
                println!("{} {} resumed", self.ts(), "▶".green());
            }
            EventKind::WorkflowAborted { reason, running_tasks, .. } => {
                println!("{} {} {}", self.ts(), "⚠".red().bold(), "ABORTED".red().bold());
                println!("{}   {} {}", " ".repeat(6), "reason:".dimmed(), reason.red());
                if !running_tasks.is_empty() {
                    let names: Vec<&str> = running_tasks.iter().map(|s| s.as_ref()).collect();
                    println!("{}   {} {}", " ".repeat(6), "running:".dimmed(), names.join(", ").yellow());
                }
            }

            // ═══════════════════════════════════════
            // TASK LEVEL
            // ═══════════════════════════════════════
            EventKind::TaskScheduled { task_id, dependencies } => {
                // Check if we need a layer separator
                if self.detail.show_layer_separators() {
                    if let Some(&layer) = self.task_layers.get(task_id) {
                        if layer > self.current_layer && self.current_layer > 0 {
                            println!();
                            let label = format!(" layer {} ", layer + 1);
                            let dash = "─ ".dimmed();
                            let half = (self.term_width as usize / 4).saturating_sub(label.len() / 2);
                            println!(
                                "{}{}{}{}{}",
                                " ".repeat(14),
                                dash.to_string().repeat(half / 2),
                                label.dimmed(),
                                dash.to_string().repeat(half / 2),
                                ""
                            );
                            println!();
                        }
                        self.current_layer = layer;
                    }
                }

                let deps_str = if dependencies.is_empty() {
                    "—".dimmed().to_string()
                } else {
                    dependencies.iter().map(|d| d.as_ref()).collect::<Vec<_>>().join(", ")
                };
                // Look up verb for this task — will be filled by TaskStarted
                println!(
                    "{}  {} {} {:<14} {} {}",
                    self.ts(),
                    icons::pending(),
                    " ".normal(), // placeholder — verb not known yet at schedule time
                    task_id.bold(),
                    "scheduled".dimmed(),
                    format!("deps: {}", deps_str).dimmed()
                );
            }

            EventKind::TaskStarted { task_id, verb, .. } => {
                self.task_starts.insert(task_id.to_string(), event.timestamp_ms);
                println!(
                    "{}  {} {} {:<14} {}",
                    self.ts(),
                    icons::running(),
                    icons::verb(verb),
                    task_id.bold(),
                    "running".white()
                );
            }

            EventKind::TaskCompleted { task_id, output, duration_ms } => {
                self.stats.tasks_passed += 1;
                let dur_secs = *duration_ms as f32 / 1000.0;

                // Record timeline
                if let Some(start) = self.task_starts.get(task_id.as_ref()) {
                    // We'd need verb here — stored from TaskStarted
                    self.stats.task_timeline.push((
                        task_id.to_string(),
                        String::new(), // verb filled later or from a lookup
                        start - self.workflow_start_ms,
                        *duration_ms,
                    ));
                }

                println!(
                    "{}  {} {:<16} {}",
                    self.ts(),
                    icons::success(),
                    task_id.bold(),
                    colors::duration(dur_secs)
                );

                // Output preview
                if self.detail.show_previews() {
                    self.render_output_preview(output);
                }
            }

            EventKind::TaskFailed { task_id, error, duration_ms } => {
                self.stats.tasks_failed += 1;
                let dur_secs = *duration_ms as f32 / 1000.0;
                println!(
                    "{}  {} {:<16} {}",
                    self.ts(),
                    icons::failed(),
                    task_id.bold().red(),
                    colors::duration(dur_secs)
                );
                println!(
                    "{}  {} {} {}",
                    " ".repeat(6),
                    "│".dimmed(),
                    "error".red(),
                    error.red()
                );
            }

            // ═══════════════════════════════════════
            // FINE-GRAINED
            // ═══════════════════════════════════════
            EventKind::TemplateResolved { task_id, template, result } => {
                if self.detail.show_template_events() {
                    println!(
                        "{}     {} {} {} → {}",
                        " ".repeat(6),
                        "│".dimmed(),
                        "tmpl".dimmed(),
                        template.dimmed(),
                        result.dimmed()
                    );
                }
            }

            EventKind::ProviderCalled { task_id, provider, model, prompt_len } => {
                if self.detail.show_sub_events() {
                    println!(
                        "{}     {} {} {}/{} {} {} chars",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::provider(),
                        provider.dimmed(),
                        model.white(),
                        "· prompt:".dimmed(),
                        prompt_len
                    );
                }
            }

            EventKind::ProviderResponded {
                task_id, input_tokens, output_tokens,
                cache_read_tokens, ttft_ms, finish_reason, cost_usd, ..
            } => {
                // Accumulate stats
                self.stats.total_input_tokens += input_tokens;
                self.stats.total_output_tokens += output_tokens;
                self.stats.total_cache_tokens += cache_read_tokens;
                self.stats.total_cost += cost_usd;
                if let Some(t) = ttft_ms {
                    self.stats.ttft_values.push(*t);
                }
                self.stats.provider_calls.push(ProviderCallStat {
                    task_id: task_id.to_string(),
                    input_tokens: *input_tokens,
                    output_tokens: *output_tokens,
                    cache_tokens: *cache_read_tokens,
                    ttft_ms: *ttft_ms,
                    cost: *cost_usd,
                });

                if self.detail.show_sub_events() {
                    let ttft_str = ttft_ms
                        .map(|t| format!(" · ttft:{}", colors::ttft(t)))
                        .unwrap_or_default();
                    println!(
                        "{}     {} {} {} in:{} out:{} cache:{}{}",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::provider(),
                        "←".dimmed(),
                        colors::tokens(*input_tokens).dimmed(),
                        colors::tokens(*output_tokens).white(),
                        colors::tokens(*cache_read_tokens).dimmed(),
                        ttft_str
                    );

                    if self.detail.show_sparklines() {
                        let max_tok = (*input_tokens).max(*output_tokens);
                        println!(
                            "{}     {}    tok {} cost {}",
                            " ".repeat(6),
                            "│".dimmed(),
                            colors::sparkline(*output_tokens, max_tok),
                            colors::cost(*cost_usd)
                        );
                    }
                }
            }

            // ═══════════════════════════════════════
            // CONTEXT
            // ═══════════════════════════════════════
            EventKind::ContextAssembled {
                task_id, sources, total_tokens,
                budget_used_pct, truncated, ..
            } => {
                if self.detail.show_sub_events() {
                    let warn = if *budget_used_pct > 90.0 { " ⚠".red().to_string() } else { String::new() };
                    println!(
                        "{}     {} {} {} src · {} tok · {}{}",
                        " ".repeat(6),
                        "│".dimmed(),
                        "ctx".dimmed(),
                        sources.len(),
                        colors::tokens(*total_tokens),
                        colors::budget_bar(*budget_used_pct, 25),
                        warn
                    );
                }
            }

            // ═══════════════════════════════════════
            // MCP
            // ═══════════════════════════════════════
            EventKind::McpConnected { server_name } => {
                if self.detail.show_sub_events() {
                    println!(
                        "{}     {} {} connected {}",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::mcp(),
                        server_name.green()
                    );
                }
            }

            EventKind::McpError { server_name, error } => {
                self.stats.mcp_errors += 1;
                println!(
                    "{}     {} {} {} {}",
                    " ".repeat(6),
                    "│".dimmed(),
                    icons::mcp(),
                    format!("{} ✗", server_name).red(),
                    error.red()
                );
            }

            EventKind::McpInvoke { task_id, call_id, mcp_server, tool, resource, .. } => {
                self.stats.mcp_calls += 1;
                if self.detail.show_sub_events() {
                    let target = tool.as_deref().or(resource.as_deref()).unwrap_or("?");
                    println!(
                        "{}     {} {} {} → {} {}",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::mcp(),
                        mcp_server.dimmed(),
                        target.white(),
                        format!("call:{}", call_id).dimmed()
                    );
                }
            }

            EventKind::McpResponse { call_id, output_len, duration_ms, cached, is_error, .. } => {
                if self.detail.show_sub_events() {
                    let cache_tag = if *cached { " cached".green().to_string() } else { String::new() };
                    let err_tag = if *is_error { " ✗".red().to_string() } else { String::new() };
                    println!(
                        "{}     {} {} {} {} · {}{}{}",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::mcp(),
                        format!("call:{}", call_id).dimmed(),
                        "←".dimmed(),
                        format_bytes(*output_len as u64),
                        format!(" · {}ms", duration_ms).dimmed(),
                        format!("{}{}", cache_tag, err_tag)
                    );
                }
            }

            EventKind::McpRetry { task_id, server_name, operation, attempt, max_attempts, error } => {
                self.stats.mcp_retries += 1;
                println!(
                    "{}     {} {} {} {}/{} · {}",
                    " ".repeat(6),
                    "│".dimmed(),
                    icons::retry(),
                    format!("retry {}", operation).yellow(),
                    attempt.to_string().yellow(),
                    max_attempts,
                    error.dimmed()
                );
            }

            // ═══════════════════════════════════════
            // AGENT
            // ═══════════════════════════════════════
            EventKind::AgentStart { task_id, max_turns, mcp_servers } => {
                if self.detail.show_sub_events() {
                    let servers = mcp_servers.join(", ");
                    println!(
                        "{}     {} {} {} max_turns:{} · mcp:[{}]",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::agent_meta(),
                        "agent".dimmed(),
                        max_turns,
                        servers.green()
                    );
                }
            }

            EventKind::AgentTurn { task_id, turn_index, kind, metadata } => {
                if self.detail.show_sub_events() {
                    println!(
                        "{}     {} {} turn {}/…  {}",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::agent_meta(),
                        (turn_index + 1).to_string().white(),
                        kind.dimmed()
                    );

                    // If metadata available, show tool_use or end_turn
                    if let Some(meta) = metadata {
                        if meta.stop_reason == "tool_use" {
                            println!(
                                "{}     {} {} tool_use",
                                " ".repeat(6),
                                "│".dimmed(),
                                "↳".dimmed()
                            );
                        }
                    }
                }
            }

            EventKind::AgentComplete { task_id, turns, stop_reason } => {
                if self.detail.show_sub_events() {
                    println!(
                        "{}     {} {} {} {} turns · {}",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::agent_meta(),
                        "done".green(),
                        turns,
                        stop_reason.dimmed()
                    );
                }
            }

            EventKind::AgentSpawned { parent_task_id, child_task_id, depth } => {
                if self.detail.show_sub_events() {
                    println!(
                        "{}     {} {} spawned {} depth:{}",
                        " ".repeat(6),
                        "│".dimmed(),
                        "⤋".magenta(),
                        child_task_id.white(),
                        depth
                    );
                }
            }

            // ═══════════════════════════════════════
            // GUARDRAILS
            // ═══════════════════════════════════════
            EventKind::GuardrailPassed { guardrail_type, description, .. } => {
                self.stats.guardrails_passed += 1;
                if self.detail.show_sub_events() {
                    println!(
                        "{}     {} {} {} {}",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::guardrail(),
                        icons::success(),
                        format!("{} · {}", guardrail_type, description).dimmed()
                    );
                }
            }

            EventKind::GuardrailFailed { guardrail_type, message, .. } => {
                self.stats.guardrails_failed += 1;
                println!(
                    "{}     {} {} {} {}",
                    " ".repeat(6),
                    "│".dimmed(),
                    icons::guardrail(),
                    icons::failed(),
                    format!("{} · {}", guardrail_type, message).red()
                );
            }

            EventKind::GuardrailEscalation { guardrail_type, severity, message, .. } => {
                self.stats.guardrails_escalations += 1;
                println!(
                    "{}     {}   {} {} · {}",
                    " ".repeat(6),
                    "│".dimmed(),
                    icons::retry(),
                    format!("escalation · {}", severity).yellow(),
                    message.dimmed()
                );
            }

            // ═══════════════════════════════════════
            // BUILTIN
            // ═══════════════════════════════════════
            EventKind::Log { level, message, .. } => {
                let level_colored = match level.as_str() {
                    "error" => level.red(),
                    "warn"  => level.yellow(),
                    "info"  => level.green(),
                    "debug" => level.dimmed(),
                    "trace" => level.dimmed(),
                    _       => level.normal(),
                };
                println!(
                    "{}  {} {} · {}",
                    self.ts(),
                    icons::log(),
                    level_colored,
                    message
                );
            }

            EventKind::Custom { name, payload, .. } => {
                if self.detail.show_sub_events() {
                    let preview = serde_json::to_string(payload)
                        .unwrap_or_default();
                    let short = if preview.len() > 60 {
                        format!("{}…", &preview[..60])
                    } else {
                        preview
                    };
                    println!(
                        "{}  {} {} · {}",
                        self.ts(),
                        icons::log(),
                        name.cyan(),
                        short.dimmed()
                    );
                }
            }

            // ═══════════════════════════════════════
            // ARTIFACTS
            // ═══════════════════════════════════════
            EventKind::ArtifactWritten { task_id, path, size, format, .. } => {
                self.stats.artifacts_count += 1;
                self.stats.artifacts_bytes += size;
                if self.detail.show_sub_events() {
                    println!(
                        "{}     {} {} {} {}",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::artifact(),
                        format!("→ {}", path).cyan(),
                        format!("{} · {}", format_bytes(*size), format).dimmed()
                    );
                }
            }

            EventKind::ArtifactFailed { path, reason, .. } => {
                println!(
                    "{}     {} {} {} {}",
                    " ".repeat(6),
                    "│".dimmed(),
                    icons::artifact(),
                    format!("✗ {}", path).red(),
                    reason.dimmed()
                );
            }

            // ═══════════════════════════════════════
            // MEDIA
            // ═══════════════════════════════════════
            EventKind::MediaExtracted { task_id, block_count, content_types } => {
                if self.detail.show_sub_events() {
                    println!(
                        "{}     {} {} {} blocks · types: [{}]",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::media(),
                        block_count,
                        content_types.join(", ").magenta()
                    );
                }
            }

            EventKind::MediaStored { task_id, hash, path, size_bytes, verified, deduplicated, pipeline_ms } => {
                self.stats.media_stored += 1;
                self.stats.media_bytes += size_bytes;
                if *deduplicated { self.stats.media_dedup += 1; }

                if self.detail.show_sub_events() {
                    let short_hash = if hash.len() > 16 { &hash[..16] } else { hash };
                    println!(
                        "{}     {} {} {} · {} · {}…",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::media(),
                        format_bytes(*size_bytes),
                        path.dimmed(),
                        short_hash.dimmed()
                    );
                    if self.detail.show_previews() {
                        let dedup = if *deduplicated { "yes".yellow() } else { "no".dimmed() };
                        let verif = if *verified { "yes".green() } else { "no".red() };
                        println!(
                            "{}     {}   dedup:{} · verified:{} · pipeline:{}ms",
                            " ".repeat(6),
                            "│".dimmed(),
                            dedup, verif, pipeline_ms
                        );
                    }
                }
            }

            EventKind::MediaProcessed { .. } => {
                // Grouped into MediaStored line — no separate output
            }

            EventKind::MediaStoreFailed { hash, reason, .. } => {
                println!(
                    "{}     {} {} {} {}",
                    " ".repeat(6),
                    "│".dimmed(),
                    icons::media(),
                    "✗".red(),
                    reason.red()
                );
            }

            EventKind::MediaIntegrityCheck { checked, warnings } => {
                // Stored for summary
            }

            EventKind::MediaCleanup { removed, bytes_freed, dry_run } => {
                // Stored for summary
            }

            // ═══════════════════════════════════════
            // STRUCTURED OUTPUT
            // ═══════════════════════════════════════
            EventKind::StructuredOutputAttempt { layer, layer_name, success, error, .. } => {
                self.stats.structured_attempts += 1;
                if self.detail.show_sub_events() {
                    let status = if *success { icons::success() } else { icons::failed() };
                    let err_msg = error.as_deref().map(|e| format!(" {}", e.dimmed())).unwrap_or_default();
                    println!(
                        "{}     {} {} L{}: {} {}{}",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::structured(),
                        layer,
                        layer_name,
                        status,
                        err_msg
                    );
                }
            }

            EventKind::StructuredOutputSuccess { layer, layer_name, total_attempts, .. } => {
                self.stats.structured_success_layer = Some(*layer);
            }

            // ═══════════════════════════════════════
            // VISION
            // ═══════════════════════════════════════
            EventKind::VisionContentResolved { image_count, total_bytes, resolve_ms, .. } => {
                if self.detail.show_sub_events() {
                    println!(
                        "{}     {} {} {} images · {} · resolved {}ms",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::vision(),
                        image_count,
                        format_bytes(*total_bytes),
                        resolve_ms
                    );
                }
            }

            // ═══════════════════════════════════════
            // HTTP
            // ═══════════════════════════════════════
            EventKind::HttpRequest { method, url, .. } => {
                if self.detail.show_sub_events() {
                    println!(
                        "{}     {} {} → {} {}",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::http(),
                        method.cyan(),
                        url.underline()
                    );
                }
            }

            EventKind::HttpResponse { status_code, content_type, content_length, elapsed_ms, .. } => {
                if self.detail.show_sub_events() {
                    let status_colored = if *status_code < 300 {
                        status_code.to_string().green()
                    } else if *status_code < 400 {
                        status_code.to_string().yellow()
                    } else {
                        status_code.to_string().red()
                    };
                    let ct = content_type.as_deref().unwrap_or("?");
                    let cl = content_length.map(|l| format_bytes(l)).unwrap_or_default();
                    println!(
                        "{}     {} {} ← {} · {} · {} · {}ms",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::http(),
                        status_colored,
                        ct.dimmed(),
                        cl,
                        elapsed_ms
                    );
                }
            }

            // Catch-all for WorkflowCompleted/WorkflowFailed (handled by summary)
            _ => {}
        }
    }

    /// Render the output preview box with syntax highlighting.
    fn render_output_preview(&self, output: &Value) {
        let text = match output {
            Value::String(s) => s.clone(),
            _ => serde_json::to_string_pretty(output).unwrap_or_default(),
        };

        if text.is_empty() || text == "null" {
            return;
        }

        let is_json = text.starts_with('{') || text.starts_with('[');
        let is_markdown = text.starts_with('#') || text.contains("\n## ");

        let max_width = (self.term_width as usize).min(72).saturating_sub(16);
        let dashes = "╌".repeat(max_width);
        let size_label = format!("{} ch", text.len());
        let padding = max_width.saturating_sub(size_label.len() + 1);

        println!("{}     {} {}{}{}", " ".repeat(6), "│".dimmed(),
            "╭╌".dimmed(), dashes.dimmed(), "╮".dimmed());

        let preview_lines: Vec<String> = if is_json {
            // Take first line of compact JSON, syntax-highlighted
            vec![colors::json_preview(&text.replace('\n', " "), max_width)]
        } else if is_markdown {
            colors::markdown_preview(&text, 4)
                .into_iter()
                .map(|l| {
                    if l.len() > max_width {
                        format!("{}…", &l[..max_width - 1])
                    } else {
                        l
                    }
                })
                .collect()
        } else {
            // Plain text
            text.lines()
                .take(2)
                .map(|l| {
                    if l.len() > max_width {
                        format!("{}…", &l[..max_width - 1])
                    } else {
                        l.to_string()
                    }
                })
                .collect()
        };

        for line in &preview_lines {
            let pad = max_width.saturating_sub(stripped_len(line));
            println!("{}     {} {} {}{} {}",
                " ".repeat(6), "│".dimmed(), "│".dimmed(),
                line, " ".repeat(pad), "│".dimmed());
        }

        println!("{}     {} {}{} {} {}",
            " ".repeat(6), "│".dimmed(),
            "╰╌".dimmed(), "╌".repeat(padding).dimmed(),
            size_label.dimmed(), "╌╯".dimmed());
    }

    /// Render the full summary footer.
    pub fn render_summary(&self, total_duration_ms: u64, trace_path: Option<&str>) {
        if self.detail.is_json() {
            return;
        }

        let dur_secs = total_duration_ms as f32 / 1000.0;

        println!();

        // ── Summary box ──
        let w = (self.term_width as usize).min(72);
        let border = "─".repeat(w);
        println!("╭{}╮", border.dimmed());
        println!("│{}│", " ".repeat(w));

        // Done line
        let done = format!(
            "  {}  D O N E                                              {}",
            icons::success(),
            colors::duration(dur_secs)
        );
        println!("│{}│", pad_right(&done, w));
        println!("│{}│", " ".repeat(w));

        // Tasks
        let passed = self.stats.tasks_passed.to_string().green();
        let total = (self.stats.tasks_passed + self.stats.tasks_failed + self.stats.tasks_skipped).to_string();
        println!("│{}│",
            pad_right(&format!("  Tasks    {}/{} passed", passed, total), w));
        println!("│{}│", " ".repeat(w));

        // ── Tokens ──
        if self.stats.total_input_tokens > 0 && self.detail.show_full_summary() {
            println!("│{}│",
                pad_right(&format!("  {} Tokens {}", "──".dimmed(), "─".repeat(w - 16).dimmed()), w));
            let max_tok = self.stats.total_input_tokens
                .max(self.stats.total_output_tokens)
                .max(self.stats.total_cache_tokens);
            println!("│{}│",
                pad_right(&format!("    in {} {}",
                    token_bar(self.stats.total_input_tokens, max_tok, 30, "blue"),
                    colors::tokens(self.stats.total_input_tokens)
                ), w));
            println!("│{}│",
                pad_right(&format!("   out {} {}",
                    token_bar(self.stats.total_output_tokens, max_tok, 30, "magenta"),
                    colors::tokens(self.stats.total_output_tokens)
                ), w));
            if self.stats.total_cache_tokens > 0 {
                println!("│{}│",
                    pad_right(&format!("    $↻ {} {} saved",
                        token_bar(self.stats.total_cache_tokens, max_tok, 30, "green"),
                        colors::tokens(self.stats.total_cache_tokens)
                    ), w));
            }
            println!("│{}│", " ".repeat(w));
        }

        // ── Cost ──
        if self.stats.total_cost > 0.0 && self.detail.show_full_summary() {
            println!("│{}│",
                pad_right(&format!("  {} Cost {}", "──".dimmed(), "─".repeat(w - 14).dimmed()), w));
            // Per-task cost breakdown using ▪ blocks
            let mut cost_parts = Vec::new();
            // Group provider_calls by task_id
            let mut task_costs: Vec<(String, f64)> = Vec::new();
            for call in &self.stats.provider_calls {
                if let Some(existing) = task_costs.iter_mut().find(|(t, _)| *t == call.task_id) {
                    existing.1 += call.cost;
                } else {
                    task_costs.push((call.task_id.clone(), call.cost));
                }
            }
            for (task, c) in &task_costs {
                let blocks = ((c / self.stats.total_cost) * 20.0).round() as usize;
                cost_parts.push(format!("{} {}", task.dimmed(), "▪".repeat(blocks.max(1))));
            }
            println!("│{}│",
                pad_right(&format!("  {} {}", colors::cost(self.stats.total_cost), cost_parts.join("  ")), w));
            println!("│{}│", " ".repeat(w));
        }

        // ── Performance ──
        if !self.stats.ttft_values.is_empty() && self.detail.show_full_summary() {
            println!("│{}│",
                pad_right(&format!("  {} Performance {}", "──".dimmed(), "─".repeat(w - 20).dimmed()), w));
            let avg_ttft = self.stats.ttft_values.iter().sum::<u64>() / self.stats.ttft_values.len() as u64;
            let min_ttft = self.stats.ttft_values.iter().min().copied().unwrap_or(0);
            let max_ttft = self.stats.ttft_values.iter().max().copied().unwrap_or(0);
            let throughput = if dur_secs > 0.0 {
                (self.stats.total_output_tokens as f32 / dur_secs).round() as u64
            } else { 0 };

            println!("│{}│",
                pad_right(&format!("  TTFT     avg {} · min {} · max {}",
                    colors::ttft(avg_ttft), colors::ttft(min_ttft), colors::ttft(max_ttft)), w));
            println!("│{}│",
                pad_right(&format!("  Throughput  {} tok/s", throughput), w));
            println!("│{}│", " ".repeat(w));
        }

        // ── Infrastructure ──
        if self.detail.show_full_summary() {
            println!("│{}│",
                pad_right(&format!("  {} Infrastructure {}", "──".dimmed(), "─".repeat(w - 24).dimmed()), w));
            if self.stats.mcp_calls > 0 {
                println!("│{}│",
                    pad_right(&format!("  MCP      {} calls · {} retries · {} errors",
                        self.stats.mcp_calls,
                        self.stats.mcp_retries.to_string().yellow(),
                        self.stats.mcp_errors.to_string().if_then(self.stats.mcp_errors > 0, |s| s.red(), |s| s.green())
                    ), w));
            }
            if self.stats.media_stored > 0 {
                println!("│{}│",
                    pad_right(&format!("  Media    {} stored · {} · {} dedup · ✓ integrity",
                        self.stats.media_stored,
                        format_bytes(self.stats.media_bytes),
                        self.stats.media_dedup
                    ), w));
            }
            if self.stats.artifacts_count > 0 {
                println!("│{}│",
                    pad_right(&format!("  Output   {} artifacts · {} total",
                        self.stats.artifacts_count,
                        format_bytes(self.stats.artifacts_bytes)
                    ), w));
            }
            if self.stats.guardrails_passed + self.stats.guardrails_failed > 0 {
                println!("│{}│",
                    pad_right(&format!("  Guards   {} passed · {} failed · {} escalations",
                        self.stats.guardrails_passed.to_string().green(),
                        self.stats.guardrails_failed.to_string().yellow(),
                        self.stats.guardrails_escalations
                    ), w));
            }
            println!("│{}│", " ".repeat(w));
        }

        // ── Timeline (Gantt) ──
        if !self.stats.task_timeline.is_empty() && self.detail.show_full_summary() {
            println!("│{}│",
                pad_right(&format!("  {} Timeline {}", "──".dimmed(), "─".repeat(w - 18).dimmed()), w));

            let total_ms = total_duration_ms;
            let bar_width = 38;

            for (task_id, verb, start_ms, dur_ms) in &self.stats.task_timeline {
                let start_pct = *start_ms as f64 / total_ms as f64;
                let dur_pct = *dur_ms as f64 / total_ms as f64;
                let start_col = (start_pct * bar_width as f64).round() as usize;
                let dur_col = (dur_pct * bar_width as f64).round().max(1.0) as usize;
                let end_col = (start_col + dur_col).min(bar_width);

                let mut bar = String::new();
                for i in 0..bar_width {
                    if i >= start_col && i < end_col {
                        bar.push('█');
                    } else {
                        bar.push('░');
                    }
                }
                // Color the bar based on verb
                let dur_secs = *dur_ms as f32 / 1000.0;
                println!("│{}│",
                    pad_right(&format!("  {:<12} {} {:>5}",
                        task_id.dimmed(), bar, colors::duration(dur_secs)), w));
            }

            // Time axis
            let axis = format!("  {:12} 0s{:>12}{:>12} {:.1}s",
                "", "", "", total_ms as f64 / 1000.0);
            println!("│{}│", pad_right(&axis.dimmed().to_string(), w));
            println!("│{}│", " ".repeat(w));
        }

        // ── Provider Breakdown Table ──
        if !self.stats.provider_calls.is_empty() && self.detail.show_full_summary() {
            println!("│{}│",
                pad_right(&format!("  {} Provider Breakdown {}", "──".dimmed(), "─".repeat(w - 27).dimmed()), w));
            println!("│{}│",
                pad_right(&format!("  {}   {}      {}    {}   {}    {}",
                    "#".dimmed(), "Task".dimmed(), "In".dimmed(),
                    "Out".dimmed(), "Cache".dimmed(), "Cost".dimmed()), w));
            for (i, call) in self.stats.provider_calls.iter().enumerate() {
                let ttft_str = call.ttft_ms
                    .map(|t| format!("{}ms", t))
                    .unwrap_or_else(|| "—".to_string());
                println!("│{}│",
                    pad_right(&format!("  {}   {:<12} {:>5}  {:>5}  {:>5}   {}",
                        i + 1,
                        call.task_id,
                        colors::tokens(call.input_tokens),
                        colors::tokens(call.output_tokens),
                        colors::tokens(call.cache_tokens),
                        colors::cost(call.cost)
                    ), w));
            }
            // Totals row
            println!("│{}│",
                pad_right(&format!("  {}",
                    "─".repeat(w - 4).dimmed()), w));
            println!("│{}│",
                pad_right(&format!("  Σ   {:12} {:>5}  {:>5}  {:>5}   {}",
                    "",
                    colors::tokens(self.stats.total_input_tokens),
                    colors::tokens(self.stats.total_output_tokens),
                    colors::tokens(self.stats.total_cache_tokens),
                    colors::cost(self.stats.total_cost)
                ), w));
            println!("│{}│", " ".repeat(w));
        }

        // Trace path
        if let Some(path) = trace_path {
            println!("│{}│",
                pad_right(&format!("  trace {}", path.dimmed()), w));
        }

        println!("│{}│", " ".repeat(w));
        println!("╰{}╯", border.dimmed());
    }
}

// ═══════════════════════════════════════
// HELPERS
// ═══════════════════════════════════════

/// Format bytes: 1234 → "1.2 KB", 1234567 → "1.2 MB"
fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// Get the display width of a string, stripping ANSI escape codes.
fn stripped_len(s: &str) -> usize {
    // Simple ANSI stripper — removes \x1b[...m sequences
    let mut len = 0;
    let mut in_escape = false;
    for ch in s.chars() {
        if ch == '\x1b' {
            in_escape = true;
        } else if in_escape && ch == 'm' {
            in_escape = false;
        } else if !in_escape {
            len += 1;
        }
    }
    len
}

/// Pad a string to width, accounting for ANSI escape codes.
fn pad_right(s: &str, width: usize) -> String {
    let visible = stripped_len(s);
    if visible >= width {
        s.to_string()
    } else {
        format!("{}{}", s, " ".repeat(width - visible))
    }
}

/// Generate a token bar: █ for filled, ░ for empty.
fn token_bar(value: u64, max: u64, width: usize, color: &str) -> String {
    let ratio = if max == 0 { 0.0 } else { value as f64 / max as f64 };
    let filled = (ratio * width as f64).round() as usize;
    let empty = width.saturating_sub(filled);
    let bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty));
    match color {
        "blue"    => bar.blue().to_string(),
        "magenta" => bar.magenta().to_string(),
        "green"   => bar.green().to_string(),
        _         => bar,
    }
}
```

**Step 2: Register in `src/display/mod.rs`**

```rust
pub mod renderer;
pub use renderer::CliRenderer;
```

**Step 3: Run `cargo check`**

```bash
cargo check 2>&1 | tail -10
```

Fix any compilation errors. This is a large file — expect iterative fixes.

**Step 4: Commit**

```bash
git add src/display/renderer.rs src/display/mod.rs
git commit -m "feat(display): add CliRenderer — append-only event stream engine

Renders all 41 EventKind variants as colored, formatted terminal lines.
Supports DetailLevel (max/default/min/json).
Includes: sparklines, budget bars, JSON syntax highlighting,
output previews, and full summary with timeline + provider table.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## Task 5: Create `HeaderRenderer` — the new workflow header

**Files:**
- Create: `src/display/header.rs`
- Modify: `src/display/mod.rs`

**Step 1: Create header with rounded corners and static DAG**

```rust
//! Header renderer — workflow info box + static DAG.

use colored::Colorize;

use crate::display::icons;

/// Print the new rounded-corner header box.
///
/// ```text
/// ╭───────────────────────────────────────────────────────────╮
/// │                                                           │
/// │  N I K A                                        v0.35.0   │
/// │                                                           │
/// │  seo-pipeline                                             │
/// │  △ claude / sonnet-4                   6 tasks · 3 layers │
/// │  gen:7f3a2b                                               │
/// │                                                           │
/// ╰───────────────────────────────────────────────────────────╯
/// ```
pub fn print_header(
    name: Option<&str>,
    provider: &str,
    model: &str,
    task_count: usize,
    layer_count: usize,
    version: &str,
    generation_id: &str,
) {
    let w = terminal_size::terminal_size()
        .map(|(tw, _)| tw.0 as usize)
        .unwrap_or(80)
        .min(72);

    let inner = w - 2; // account for │ on each side
    let border = "─".repeat(inner);

    println!("╭{}╮", border.dimmed());
    println!("│{}│", " ".repeat(inner));

    // Title line
    let title = "N I K A";
    let ver = format!("v{}", version);
    let pad = inner.saturating_sub(title.len() + ver.len() + 4);
    println!("│  {}{}{}  │", title.bold().white(), " ".repeat(pad), ver.dimmed());
    println!("│{}│", " ".repeat(inner));

    // Workflow name
    let display_name = name.unwrap_or("(unnamed)");
    println!("│  {}{}│",
        display_name.bold(),
        " ".repeat(inner.saturating_sub(display_name.len() + 2)));

    // Provider + task count
    let info = format!("{} {} / {}",
        icons::provider(), provider, model);
    let tasks = format!("{} tasks · {} layers", task_count, layer_count);
    let pad = inner.saturating_sub(stripped_display_len(&info) + tasks.len() + 4);
    println!("│  {}{}{} │", info, " ".repeat(pad), tasks.dimmed());

    // Generation ID
    let gen = format!("gen:{}", &generation_id[..generation_id.len().min(8)]);
    println!("│  {}{}│",
        gen.dimmed(),
        " ".repeat(inner.saturating_sub(gen.len() + 2)));

    println!("│{}│", " ".repeat(inner));
    println!("╰{}╯", border.dimmed());
    println!();
}

/// Rough display width ignoring ANSI codes.
fn stripped_display_len(s: &str) -> usize {
    let mut len = 0;
    let mut in_escape = false;
    for ch in s.chars() {
        if ch == '\x1b' { in_escape = true; }
        else if in_escape && ch == 'm' { in_escape = false; }
        else if !in_escape { len += 1; }
    }
    len
}
```

**Step 2: Register and commit**

```bash
git add src/display/header.rs src/display/mod.rs
git commit -m "feat(display): add rounded-corner header renderer

Modern header with NIKA branding, provider info, task count,
layer count, and generation ID. Uses ╭╮╰╯ rounded corners.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## Task 6: Create `DagRenderer` — static DAG (print once)

**Files:**
- Create: `src/display/dag.rs`
- Modify: `src/display/mod.rs`

**Step 1: Create simplified static DAG renderer**

The static DAG is printed once after the header. It uses the new Cosmic icons and shows the flow without boxes (lighter than the old LiveDag).

```rust
//! Static DAG renderer — printed once, no ANSI cursor movement.
//!
//! Shows task names with verb icons connected by arrows.
//! Groups tasks by topological layer (parallel tasks on same line).

use colored::Colorize;

use crate::display::icons;

pub struct DagTask {
    pub id: String,
    pub verb: String,
    pub layer: usize,
}

pub struct DagEdge {
    pub from: String,
    pub to: String,
}

/// Print a compact static DAG.
///
/// ```text
///   ✧ research ──┐   ⎈ scrape ──┐   ☄ fetch_api
///                ├─▸ ❋ analyze ◂─┘       │
///                │       │               │
///                │       ▾               │
///                │  ✧ summarize ◂────────┘
///                │       │
///                └──▸ ⊛ publish
/// ```
pub fn print_static_dag(tasks: &[DagTask], edges: &[DagEdge]) {
    // Group by layer
    let max_layer = tasks.iter().map(|t| t.layer).max().unwrap_or(0);

    let summary = format!(
        "DAG  {} tasks · {} layers · {} edges",
        tasks.len(), max_layer + 1, edges.len()
    );
    println!("  {}", summary.dimmed());
    println!();

    // Simple rendering: one line per layer with task names
    for layer_idx in 0..=max_layer {
        let layer_tasks: Vec<&DagTask> = tasks.iter()
            .filter(|t| t.layer == layer_idx)
            .collect();

        let line: Vec<String> = layer_tasks.iter()
            .map(|t| format!("{} {}", icons::verb_plain(&t.verb), t.id))
            .collect();

        println!("   {}", line.join("       "));

        // Print arrows to next layer if not last
        if layer_idx < max_layer {
            // Find edges from this layer to next
            let connecting: Vec<&DagEdge> = edges.iter()
                .filter(|e| {
                    tasks.iter().any(|t| t.id == e.from && t.layer == layer_idx)
                })
                .collect();

            if !connecting.is_empty() {
                // Simple vertical connector
                println!("   {:>width$}", "│".dimmed(), width = 8);
                println!("   {:>width$}", "▾".dimmed(), width = 8);
            }
        }
    }
    println!();
}
```

**Step 2: Register and commit**

```bash
git add src/display/dag.rs src/display/mod.rs
git commit -m "feat(display): add static DAG renderer — no cursor movement

Simple topological DAG printed once with Cosmic verb icons.
Zero ANSI cursor tricks — pure append-only output.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## Task 7: Wire `CliRenderer` into the Runner

**Files:**
- Modify: `src/runtime/runner.rs` (lines 1085-1121, 1822-1837, 2070)
- Modify: `src/main.rs` (lines 486, 691-753)

This is the integration task — replacing LiveDag usage with CliRenderer.

**Step 1: Add `CliRenderer` to Runner struct**

In `runner.rs`, add a field:

```rust
/// CLI event stream renderer (None when quiet or TUI mode)
cli_renderer: Option<crate::display::CliRenderer>,
```

Initialize in `Runner::new()` or via a builder method:

```rust
pub fn with_detail_level(mut self, detail: crate::display::DetailLevel) -> Self {
    if !self.quiet {
        self.cli_renderer = Some(crate::display::CliRenderer::new(detail));
    }
    self
}
```

**Step 2: Replace LiveDag creation (lines 1085-1121)**

Replace the LiveDag block with:

```rust
// Compute task layers for the renderer
if let Some(ref mut renderer) = self.cli_renderer {
    let layers = compute_task_layers(&self.dag, &self.workflow);
    renderer.set_task_layers(layers);

    // Print static DAG (once)
    let dag_tasks = /* build from workflow tasks */;
    let dag_edges = /* build from deps */;
    crate::display::dag::print_static_dag(&dag_tasks, &dag_edges);
    println!("{}", "╌".repeat(69).dimmed());
    println!();
}
```

**Step 3: Replace LiveDag update calls (lines 1822-1837)**

Replace `dag.update_task()` + `dag.redraw()` with:

```rust
if let Some(ref mut renderer) = self.cli_renderer {
    renderer.render(&event);
}
```

**Step 4: Replace print_done_summary call (line 2070)**

Replace with:

```rust
if let Some(ref renderer) = self.cli_renderer {
    renderer.render_summary(total_duration_ms, trace_path.as_deref());
}
```

**Step 5: Update `main.rs` to pass DetailLevel**

In `run_workflow()` (~line 741):

```rust
let runner = Runner::new(workflow, config, event_log)
    .with_detail_level(detail);
```

**Step 6: Remove all `println!()` calls in runner.rs that print task progress**

Search for and remove/guard all direct `println!()` calls in the execution loop that are now handled by the renderer.

**Step 7: Run tests**

```bash
cargo test --lib 2>&1 | tail -20
cargo check 2>&1 | tail -5
```

**Step 8: Commit**

```bash
git add src/runtime/runner.rs src/main.rs
git commit -m "feat(runtime): wire CliRenderer into runner, remove LiveDag

Replace ANSI cursor-movement LiveDag with append-only CliRenderer.
Events are rendered as they happen — no more terminal glitches.
DetailLevel controls verbosity via --detail flag.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## Task 8: Update legacy display functions

**Files:**
- Modify: `src/display/legacy.rs`

**Step 1: Update `verb_icon()` and `verb_emoji()` to use Cosmic palette**

Replace the match arms in both functions to use the new Unicode characters:

```rust
pub fn verb_icon(verb: &str) -> colored::ColoredString {
    crate::display::icons::verb(verb)
}

pub fn verb_emoji(verb: &str) -> &'static str {
    crate::display::icons::verb_plain(verb)
}
```

**Step 2: Deprecate `LiveDag` struct**

Add `#[deprecated(note = "Use CliRenderer instead")]` to the LiveDag struct and its methods.

**Step 3: Delegate `format_duration` to new module**

```rust
pub fn format_duration(secs: f32) -> colored::ColoredString {
    crate::display::colors::duration(secs)
}
```

**Step 4: Run tests, commit**

```bash
cargo test --lib 2>&1 | tail -20
git add src/display/legacy.rs
git commit -m "refactor(display): delegate legacy functions to new Cosmic modules

verb_icon/verb_emoji now use Cosmic palette.
LiveDag deprecated in favor of CliRenderer.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## Task 9: Add unit tests for all renderers

**Files:**
- Create: `src/display/tests.rs`
- Modify: `src/display/mod.rs`

**Step 1: Test icons render correctly**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_verb_icons_are_single_width() {
        for verb in &["infer", "exec", "fetch", "invoke", "agent"] {
            let plain = icons::verb_plain(verb);
            // Each icon should be a single Unicode character
            assert_eq!(plain.chars().count(), 1, "verb '{}' icon should be 1 char", verb);
        }
    }

    #[test]
    fn test_cosmic_palette_characters() {
        assert_eq!(icons::verb_plain("infer"), "✧");
        assert_eq!(icons::verb_plain("exec"), "⎈");
        assert_eq!(icons::verb_plain("fetch"), "☄");
        assert_eq!(icons::verb_plain("invoke"), "⊛");
        assert_eq!(icons::verb_plain("agent"), "❋");
    }

    #[test]
    fn test_token_formatting() {
        assert_eq!(colors::tokens(42), "42");
        assert_eq!(colors::tokens(842), "842");
        assert_eq!(colors::tokens(1200), "1.2k");
        assert_eq!(colors::tokens(15000), "15k");
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(renderer::format_bytes(500), "500 B");
        assert_eq!(renderer::format_bytes(1536), "1.5 KB");
        assert_eq!(renderer::format_bytes(1_500_000), "1.4 MB");
    }

    #[test]
    fn test_json_preview_truncation() {
        let json = r#"{"key":"value","long":"very long string here"}"#;
        let preview = colors::json_preview(json, 20);
        // Should contain ANSI color codes and be truncated
        assert!(preview.contains("key"));
    }

    #[test]
    fn test_budget_bar_thresholds() {
        let low = colors::budget_bar(30.0, 20);
        assert!(low.contains("30%"));

        let mid = colors::budget_bar(75.0, 20);
        assert!(mid.contains("75%"));

        let high = colors::budget_bar(95.0, 20);
        assert!(high.contains("95%"));
    }

    #[test]
    fn test_detail_level_visibility() {
        // Max shows everything
        assert!(DetailLevel::Max.show_sub_events());
        assert!(DetailLevel::Max.show_previews());
        assert!(DetailLevel::Max.show_sparklines());

        // Default shows sub-events but not previews
        assert!(DetailLevel::Default.show_sub_events());
        assert!(!DetailLevel::Default.show_previews());

        // Min shows nothing extra
        assert!(!DetailLevel::Min.show_sub_events());

        // JSON is special
        assert!(DetailLevel::Json.is_json());
    }
}
```

**Step 2: Run tests, commit**

```bash
cargo test --lib -- display 2>&1 | head -30
git add src/display/tests.rs src/display/mod.rs
git commit -m "test(display): add unit tests for Cosmic icons, colors, and renderer

Tests cover: icon width, palette characters, token formatting,
byte formatting, JSON preview, budget bar thresholds, detail levels.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## Task 10: Integration test — manual verification

**Step 1: Build and test with a real workflow**

```bash
cargo build 2>&1 | tail -5
```

**Step 2: Create a test workflow**

```bash
cat > /tmp/test-output.nika.yaml << 'EOF'
name: output-test
tasks:
  - id: greet
    infer:
      prompt: "Say hello in one sentence"
    output:
      write: /tmp/nika-test-greet.txt
EOF
```

**Step 3: Run with each detail level**

```bash
# Max (default)
cargo run -- run /tmp/test-output.nika.yaml --detail max

# Default
cargo run -- run /tmp/test-output.nika.yaml --detail default

# Min
cargo run -- run /tmp/test-output.nika.yaml --detail min

# JSON
cargo run -- run /tmp/test-output.nika.yaml --detail json
```

**Step 4: Verify output visually**

Check:
- [ ] Header renders with rounded corners
- [ ] Cosmic icons appear correctly (✧ for infer)
- [ ] Timestamps show +X.Xs format
- [ ] Provider info shows with sparkline
- [ ] Output preview box renders with syntax highlighting
- [ ] Summary footer shows tokens, cost, timeline
- [ ] `--detail min` only shows task ✓/✗ + compact summary
- [ ] `--detail json` outputs NDJSON
- [ ] No ANSI cursor movement artifacts
- [ ] Colors render correctly

**Step 5: Final commit**

```bash
git add -A
git commit -m "feat(cli): complete nika run output UX overhaul

New Cosmic icon palette (✧⎈☄⊛❋). Append-only event stream
replaces buggy ANSI cursor-movement LiveDag. Full telemetry:
41 event types rendered with sparklines, budget bars, syntax-
highlighted previews, Gantt timeline, and provider breakdown.
Parameterizable via --detail max|default|min|json.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## Summary

| Task | Description | Files | Estimated Commits |
|------|-------------|-------|-------------------|
| 1 | Split display.rs → display/ module + icons + colors | 4 new, 1 renamed | 1 |
| 2 | Make unicode-width + terminal_size always-on | 1 modified | 1 |
| 3 | DetailLevel enum + --detail CLI flag | 2 new, 1 modified | 1 |
| 4 | CliRenderer — event stream engine (core) | 1 new | 1 |
| 5 | HeaderRenderer — rounded-corner header | 1 new | 1 |
| 6 | DagRenderer — static DAG (print once) | 1 new | 1 |
| 7 | Wire into Runner — replace LiveDag | 2 modified | 1 |
| 8 | Update legacy display functions | 1 modified | 1 |
| 9 | Unit tests | 1 new | 1 |
| 10 | Integration test + polish | — | 1 |
| **Total** | | **11 new/modified** | **10 commits** |

### Event Coverage Verification

All 41 EventKind variants are handled in `CliRenderer::render()`:

| # | Event | Display | Detail Level |
|---|-------|---------|-------------|
| 1 | WorkflowStarted | Header box | all |
| 2 | WorkflowCompleted | Summary footer | all |
| 3 | WorkflowFailed | ✗ error line | all |
| 4 | WorkflowAborted | ⚠ abort + running tasks | all |
| 5 | WorkflowPaused | ⏸ paused | all |
| 6 | WorkflowResumed | ▶ resumed | all |
| 7 | TaskScheduled | ○ scheduled + deps | ≥ min |
| 8 | TaskStarted | ● running | ≥ min |
| 9 | TaskCompleted | ✓ + duration + preview | ≥ min (preview: max) |
| 10 | TaskFailed | ✗ + error | all |
| 11 | TemplateResolved | tmpl line | max only |
| 12 | ProviderCalled | △ provider/model | ≥ default |
| 13 | ProviderResponded | △ ← tokens + sparkline | ≥ default (sparkline: max) |
| 14 | ContextAssembled | ctx + budget bar | ≥ default |
| 15 | McpConnected | ⊞ connected | ≥ default |
| 16 | McpError | ⊞ ✗ error | all |
| 17 | McpInvoke | ⊞ server → tool | ≥ default |
| 18 | McpResponse | ⊞ ← size + duration | ≥ default |
| 19 | McpRetry | ↯ retry n/max | all |
| 20 | AgentStart | ◈ agent + config | ≥ default |
| 21 | AgentTurn | ◈ turn n/… | ≥ default |
| 22 | AgentComplete | ◈ done | ≥ default |
| 23 | AgentSpawned | ⤋ spawned child | ≥ default |
| 24 | GuardrailPassed | ⛨ ✓ | ≥ default |
| 25 | GuardrailFailed | ⛨ ✗ | all |
| 26 | GuardrailEscalation | ↯ escalation | all |
| 27 | Log | ▪ level + message | all |
| 28 | Custom | ▪ name + payload | ≥ default |
| 29 | ArtifactWritten | ◎ → path + size | ≥ default |
| 30 | ArtifactFailed | ◎ ✗ | all |
| 31 | MediaExtracted | ▣ blocks + types | ≥ default |
| 32 | MediaProcessed | (grouped into MediaStored) | — |
| 33 | MediaStored | ▣ size + path + hash | ≥ default |
| 34 | MediaStoreFailed | ▣ ✗ | all |
| 35 | MediaIntegrityCheck | summary footer | ≥ default |
| 36 | MediaCleanup | summary footer | ≥ default |
| 37 | StructuredOutputAttempt | ⬡ Layer N ✓/✗ | ≥ default |
| 38 | StructuredOutputSuccess | (last attempt shows ✓) | ≥ default |
| 39 | VisionContentResolved | ◐ images + size | ≥ default |
| 40 | HttpRequest | ⇄ → METHOD url | ≥ default |
| 41 | HttpResponse | ⇄ ← status + size | ≥ default |

---

## Part 2: `nika check` — Pre-Flight Checklist

### Visual Mockup — `nika check workflow.nika.yaml`

```
╭───────────────────────────────────────────────────────────────────────╮
│                                                                       │
│  N I K A  C H E C K                                         v0.35.0   │
│                                                                       │
│  seo-pipeline.nika.yaml                                               │
│                                                                       │
╰───────────────────────────────────────────────────────────────────────╯

  ✓  schema          YAML valid against @0.12                      1ms
  ✓  parse           6 tasks · provider: claude · model: sonnet-4  3ms
  ✓  includes        0 expanded                                    0ms
  ✓  dag             3 layers · 5 edges · 0 cycles                 1ms
  ✓  bindings        12 refs resolved · 0 dangling                 0ms
  ✓  schemas         2 files validated                             1ms

  DAG 6 tasks · 3 layers · 5 edges

    ╔═══════════════╗  ╔══════════════╗  ╔════════════════╗
    ║ ✧ research    ║  ║ ⎈ scrape     ║  ║ ☄ fetch_api   ║
    ║   structured  ║  ║   timeout:30 ║  ║   extract:json ║
    ╚═══════════════╝  ╚══════════════╝  ╚════════════════╝
           │                  │                  │
           └──────────┬───────┘                  │
                      ▼                          │
             ╔════════════════╗                   │
             ║ ❋ analyze     ║                   │
             ║   guardrails:2║                   │
             ║   mcp:novanet ║                   │
             ╚════════════════╝                   │
                      │                          │
                      ▼                          │
             ╔════════════════╗◂──────────────────┘
             ║ ✧ summarize   ║
             ║   structured  ║
             ║   vision:2img ║
             ╚════════════════╝
                      │
                      ▼
             ╔════════════════╗
             ║ ⊛ publish     ║
             ║   mcp:novanet ║
             ╚════════════════╝

╭───────────────────────────────────────────────────────────────────────╮
│                                                                       │
│  ✓  V A L I D                                                 6ms    │
│                                                                       │
│  6 tasks · 5 edges · 3 layers · 2 schemas · 0 warnings               │
│                                                                       │
╰───────────────────────────────────────────────────────────────────────╯
```

### Visual Mockup — `nika check --strict` (MCP validation + DAG badges)

```
╭───────────────────────────────────────────────────────────────────────╮
│                                                                       │
│  N I K A  C H E C K  ─ ─  S T R I C T                       v0.35.0  │
│                                                                       │
│  seo-pipeline.nika.yaml                                               │
│                                                                       │
╰───────────────────────────────────────────────────────────────────────╯

  ✓  schema          YAML valid against @0.12                      1ms
  ✓  parse           6 tasks · provider: claude · model: sonnet-4  3ms
  ✓  includes        0 expanded                                    0ms
  ✓  dag             3 layers · 5 edges · 0 cycles                 1ms
  ✓  bindings        12 refs resolved · 0 dangling                 0ms
  ✓  schemas         2 files validated                             1ms

  ── MCP Validation ─────────────────────────────────────────────────

  ⊞ novanet
  │ connected · 47 tools available                            320ms
  │
  │ ✓ analyze     → novanet_search        params valid
  │ ✓ analyze     → novanet_context       params valid
  │ ✗ publish     → novanet_write         2 errors
  │   │ [params.resource]  must be one of: Entity, SEOKeyword, ...
  │   │ [params.format]    required field missing
  │
  │ 2/3 calls valid

  DAG 6 tasks · 3 layers · 5 edges

    ╔═✓═════════════╗  ╔═✓════════════╗  ╔═✓══════════════╗
    ║ ✧ research    ║  ║ ⎈ scrape     ║  ║ ☄ fetch_api   ║
    ║   structured  ║  ║   timeout:30 ║  ║   extract:json ║
    ╚═══════════════╝  ╚══════════════╝  ╚════════════════╝
           │                  │                  │
           └──────────┬───────┘                  │
                      ▼                          │
             ╔═✓══════════════╗                   │
             ║ ❋ analyze     ║                   │
             ║   guardrails:2║                   │
             ║   mcp:novanet ║                   │
             ╚════════════════╝                   │
                      │                          │
                      ▼                          │
             ╔═✓══════════════╗◂──────────────────┘
             ║ ✧ summarize   ║
             ║   structured  ║
             ║   vision:2img ║
             ╚════════════════╝
                      │
                      ▼
             ╔═✗═════════════╗
             ║ ⊛ publish     ║
             ║   mcp:novanet ║
             ╚═══════════════╝

╭───────────────────────────────────────────────────────────────────────╮
│                                                                       │
│  ✗  I N V A L I D                                           326ms    │
│                                                                       │
│  6 tasks · 5 edges · 3 layers · 2 schemas                             │
│  strict: 2/3 MCP calls valid · 2 param errors                        │
│                                                                       │
╰───────────────────────────────────────────────────────────────────────╯
```

### Visual Mockup — Error cases

**Cycle detected:**
```
  ✓  schema          YAML valid against @0.12                      1ms
  ✓  parse           4 tasks · provider: claude · model: sonnet-4  2ms
  ✓  includes        0 expanded                                    0ms
  ✗  dag             CYCLE DETECTED                                0ms
     │
     │ step_a → step_b → step_c → step_a
     │
     │ ╭╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╮
     │ │ Remove one dependency to break the cycle.            │
     │ │ Common fix: use with: binding instead of depends_on. │
     │ ╰╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╯
  ⊘  bindings        skipped (DAG invalid)
  ⊘  schemas         skipped (DAG invalid)

  ⚠ DAG cannot be rendered (cycle detected)
```

**Binding errors:**
```
  ✗  bindings        2 errors                                      0ms
     │
     │ task summarize
     │   with.data: $nonexistent — task not found
     │   with.result: $research.output — missing depends_on: [research]
```

---

## Task 11: Create `CheckRenderer` — validation checklist

**Files:**
- Create: `src/display/check.rs`
- Modify: `src/display/mod.rs`

**Step 1: Write tests for CheckRenderer**

Add to `src/display/check.rs`:

```rust
//! CheckRenderer — pre-flight validation checklist for `nika check`.
//!
//! Displays validation phases as a checklist with pass/fail status,
//! timing, and inline error details. Uses the Cosmic icon palette.

use colored::Colorize;

use crate::display::icons;

/// Result of a single validation phase.
pub struct PhaseResult {
    pub name: &'static str,
    pub passed: bool,
    pub detail: String,
    pub duration_ms: u64,
    /// If failed, optional error context lines
    pub errors: Vec<String>,
    /// If failed, optional hint box lines
    pub hints: Vec<String>,
}

/// Print the check header with rounded corners.
///
/// ```text
/// ╭───────────────────────────────────────────────────────────╮
/// │                                                           │
/// │  N I K A  C H E C K                             v0.35.0   │
/// │                                                           │
/// │  workflow.nika.yaml                                       │
/// │                                                           │
/// ╰───────────────────────────────────────────────────────────╯
/// ```
pub fn print_check_header(file: &str, strict: bool, version: &str) {
    let w = terminal_size::terminal_size()
        .map(|(tw, _)| tw.0 as usize)
        .unwrap_or(80)
        .min(72);

    let inner = w - 2;
    let border = "─".repeat(inner);

    println!("╭{}╮", border.dimmed());
    println!("│{}│", " ".repeat(inner));

    let title = if strict {
        "N I K A  C H E C K  ─ ─  S T R I C T"
    } else {
        "N I K A  C H E C K"
    };
    let ver = format!("v{}", version);
    let pad = inner.saturating_sub(title.len() + ver.len() + 4);
    println!("│  {}{}{}  │", title.bold().white(), " ".repeat(pad), ver.dimmed());
    println!("│{}│", " ".repeat(inner));

    // File name
    let pad = inner.saturating_sub(file.len() + 2);
    println!("│  {}{}│", file.bold(), " ".repeat(pad));

    println!("│{}│", " ".repeat(inner));
    println!("╰{}╯", border.dimmed());
    println!();
}

/// Print a single validation phase line.
///
/// ```text
///   ✓  schema          YAML valid against @0.12                      1ms
///   ✗  dag             CYCLE DETECTED                                0ms
///   ⊘  bindings        skipped (DAG invalid)
/// ```
pub fn print_phase(result: &PhaseResult) {
    let icon = if result.passed {
        icons::success()
    } else {
        icons::failed()
    };

    let dur = format!("{}ms", result.duration_ms);

    println!(
        "  {}  {:<16} {:<50} {}",
        icon,
        result.name,
        result.detail,
        dur.dimmed()
    );

    // Error details (indented under the phase)
    for err in &result.errors {
        println!("     {}",  "│".dimmed());
        println!("     {} {}", "│".dimmed(), err.red());
    }

    // Hint box (dashed border)
    if !result.hints.is_empty() {
        println!("     {}",  "│".dimmed());
        let max_w = result.hints.iter().map(|h| h.len()).max().unwrap_or(40);
        let dashes = "╌".repeat(max_w + 2);
        println!("     {} ╭{}╮", "│".dimmed(), dashes.dimmed());
        for hint in &result.hints {
            let pad = max_w.saturating_sub(hint.len());
            println!("     {} │ {}{} │", "│".dimmed(), hint, " ".repeat(pad));
        }
        println!("     {} ╰{}╯", "│".dimmed(), dashes.dimmed());
    }
}

/// Print a skipped phase (dependency failed).
pub fn print_phase_skipped(name: &str, reason: &str) {
    println!(
        "  {}  {:<16} {}",
        icons::skipped(),
        name,
        format!("skipped ({})", reason).dimmed()
    );
}

/// Print the MCP validation section for --strict mode.
///
/// ```text
///   ── MCP Validation ─────────────────────────────────────────────────
///
///   ⊞ novanet
///   │ connected · 47 tools available                            320ms
///   │
///   │ ✓ analyze     → novanet_search        params valid
///   │ ✗ publish     → novanet_write         2 errors
///   │   │ [params.resource]  must be one of: ...
///   │
///   │ 2/3 calls valid
/// ```
pub struct McpCheckResult {
    pub server_name: String,
    pub tool_count: usize,
    pub connect_ms: u64,
    pub validations: Vec<McpCallValidation>,
}

pub struct McpCallValidation {
    pub task_id: String,
    pub tool_name: String,
    pub valid: bool,
    pub errors: Vec<McpParamError>,
}

pub struct McpParamError {
    pub path: String,
    pub message: String,
}

pub fn print_mcp_validation(results: &[McpCheckResult]) {
    let w = terminal_size::terminal_size()
        .map(|(tw, _)| tw.0 as usize)
        .unwrap_or(80)
        .min(72);

    println!();
    let label = "── MCP Validation ";
    let fill = "─".repeat(w.saturating_sub(label.len() + 2));
    println!("  {}{}", label.dimmed(), fill.dimmed());
    println!();

    for result in results {
        println!("  {} {}", icons::mcp(), result.server_name.green().bold());
        println!(
            "  {} {} · {} tools available{:>w$}",
            "│".dimmed(),
            "connected".green(),
            result.tool_count,
            format!("{}ms", result.connect_ms).dimmed(),
            w = w.saturating_sub(40)
        );
        println!("  {}", "│".dimmed());

        let mut valid_count = 0u32;
        let total = result.validations.len() as u32;

        for v in &result.validations {
            if v.valid {
                valid_count += 1;
                println!(
                    "  {} {} {:<14}→ {:<24} {}",
                    "│".dimmed(),
                    icons::success(),
                    v.task_id,
                    v.tool_name,
                    "params valid".dimmed()
                );
            } else {
                println!(
                    "  {} {} {:<14}→ {:<24} {}",
                    "│".dimmed(),
                    icons::failed(),
                    v.task_id.red(),
                    v.tool_name,
                    format!("{} errors", v.errors.len()).red()
                );
                for err in &v.errors {
                    println!(
                        "  {}   {} {}  {}",
                        "│".dimmed(),
                        "│".dimmed(),
                        format!("[{}]", err.path).yellow(),
                        err.message.dimmed()
                    );
                }
            }
        }

        println!("  {}", "│".dimmed());
        let summary = format!("{}/{} calls valid", valid_count, total);
        let summary_colored = if valid_count == total {
            summary.green()
        } else {
            summary.yellow()
        };
        println!("  {} {}", "│".dimmed(), summary_colored);
        println!();
    }
}

/// Print the check summary footer.
///
/// ```text
/// ╭───────────────────────────────────────────────────────────╮
/// │                                                           │
/// │  ✓  V A L I D                                      6ms   │
/// │                                                           │
/// │  6 tasks · 5 edges · 3 layers · 2 schemas · 0 warnings   │
/// │                                                           │
/// ╰───────────────────────────────────────────────────────────╯
/// ```
pub fn print_check_summary(
    valid: bool,
    total_ms: u64,
    task_count: usize,
    edge_count: usize,
    layer_count: usize,
    schema_count: u32,
    strict_info: Option<(u32, u32, u32)>, // (valid_calls, total_calls, param_errors)
    error_codes: &[(&str, &str)],         // (code, message) for NIKA-XXX errors
) {
    let w = terminal_size::terminal_size()
        .map(|(tw, _)| tw.0 as usize)
        .unwrap_or(80)
        .min(72);

    let inner = w - 2;
    let border = "─".repeat(inner);

    println!("╭{}╮", border.dimmed());
    println!("│{}│", " ".repeat(inner));

    // Status line
    let (icon, label) = if valid {
        (icons::success(), "V A L I D".green().bold())
    } else {
        (icons::failed(), "I N V A L I D".red().bold())
    };
    let dur = format!("{}ms", total_ms);
    let status_line = format!("  {}  {}", icon, label);
    let pad = inner.saturating_sub(stripped_len(&status_line) + dur.len() + 2);
    println!("│{}{}{}  │", status_line, " ".repeat(pad), dur.dimmed());
    println!("│{}│", " ".repeat(inner));

    // Stats line
    let mut stats_parts = vec![
        format!("{} tasks", task_count),
        format!("{} edges", edge_count),
        format!("{} layers", layer_count),
    ];
    if schema_count > 0 {
        stats_parts.push(format!("{} schemas", schema_count));
    }
    let stats = stats_parts.join(" · ");
    let pad = inner.saturating_sub(stats.len() + 2);
    println!("│  {}{}│", stats, " ".repeat(pad));

    // Strict info
    if let Some((valid_calls, total_calls, param_errors)) = strict_info {
        let strict_line = format!(
            "strict: {}/{} MCP calls valid · {} param errors",
            valid_calls, total_calls, param_errors
        );
        let pad = inner.saturating_sub(strict_line.len() + 2);
        println!("│  {}{}│", strict_line, " ".repeat(pad));
    }

    // Error codes
    for (code, msg) in error_codes {
        let err_line = format!("{}: {}", code, msg);
        let pad = inner.saturating_sub(err_line.len() + 2);
        println!("│  {}{}│", err_line.red(), " ".repeat(pad));
    }

    println!("│{}│", " ".repeat(inner));
    println!("╰{}╯", border.dimmed());
}

/// Strip ANSI escape codes for width calculation.
fn stripped_len(s: &str) -> usize {
    let mut len = 0;
    let mut in_escape = false;
    for ch in s.chars() {
        if ch == '\x1b' { in_escape = true; }
        else if in_escape && ch == 'm' { in_escape = false; }
        else if !in_escape { len += 1; }
    }
    len
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phase_result_pass() {
        let result = PhaseResult {
            name: "schema",
            passed: true,
            detail: "YAML valid against @0.12".to_string(),
            duration_ms: 1,
            errors: vec![],
            hints: vec![],
        };
        // Should not panic
        print_phase(&result);
    }

    #[test]
    fn test_phase_result_fail_with_hints() {
        let result = PhaseResult {
            name: "dag",
            passed: false,
            detail: "CYCLE DETECTED".to_string(),
            duration_ms: 0,
            errors: vec!["step_a → step_b → step_c → step_a".to_string()],
            hints: vec![
                "Remove one dependency to break the cycle.".to_string(),
                "Common fix: use with: binding instead of depends_on.".to_string(),
            ],
        };
        print_phase(&result);
    }

    #[test]
    fn test_stripped_len() {
        assert_eq!(stripped_len("hello"), 5);
        assert_eq!(stripped_len("\x1b[32mhello\x1b[0m"), 5);
        assert_eq!(stripped_len("\x1b[1m\x1b[32m✓\x1b[0m"), 1);
    }
}
```

**Step 2: Register in `src/display/mod.rs`**

```rust
pub mod check;
pub use check::{
    print_check_header, print_check_summary, print_phase,
    print_phase_skipped, print_mcp_validation,
    PhaseResult, McpCheckResult, McpCallValidation, McpParamError,
};
```

**Step 3: Run tests**

```bash
cargo test --lib -- check 2>&1 | head -20
cargo check 2>&1 | tail -5
```

**Step 4: Commit**

```bash
git add src/display/check.rs src/display/mod.rs
git commit -m "feat(display): add CheckRenderer — pre-flight validation checklist

Checklist-style output for nika check with pass/fail phases,
error details, hint boxes, MCP validation section, and
compact summary with NIKA-XXX error codes.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## Task 12: Upgrade DAG boxes with Cosmic icons + task metadata

**Files:**
- Modify: `src/display/legacy.rs` (lines 486-764 — the v3 box rendering)

The advanced DAG box renderer already handles:
- Double-line borders ╔═╗║╚═╝
- Status badges in top border ╔═✓═══╗
- Edge routing (straight snap, horizontal fills, corners └┘, arrows ▼)
- Layer computation (topological sort)

We upgrade it with:
1. Cosmic verb icons (✧⎈☄⊛❋) instead of emojis (⚡📟🛰️🔌🐔)
2. Task metadata lines (structured, mcp, timeout, guardrails, vision, extract)
3. `unicode-width` for accurate display width calculation

**Step 1: Replace emoji icons with Cosmic icons in `render_v3_boxes`**

In `src/display/legacy.rs`, replace the icon match in `render_v3_boxes()` (lines 492-499):

```rust
// BEFORE:
let icon = match task.verb.as_str() {
    "infer" => "⚡",
    "exec" => "📟",
    "fetch" => "🛰️",
    "invoke" => "🔌",
    "agent" => "🐔",
    _ => "●",
};

// AFTER:
let icon = crate::display::icons::verb_plain(&task.verb);
```

**Step 2: Same replacement in `compute_box_centers`**

In `compute_box_centers()` (lines 748-755):

```rust
// BEFORE:
let icon = match verb {
    "infer" => "⚡",
    "exec" => "📟",
    "fetch" => "🛰️",
    "invoke" => "🔌",
    "agent" => "🐔",
    _ => "●",
};

// AFTER:
let icon = crate::display::icons::verb_plain(verb);
```

**Step 3: Replace `display_width` with `unicode-width` crate**

Replace the heuristic `display_width()` function (lines 725-736):

```rust
// BEFORE (heuristic):
fn display_width(s: &str) -> usize {
    let mut w = 0;
    for ch in s.chars() {
        if ch.len_utf8() >= 3 { w += 2; } else { w += 1; }
    }
    w
}

// AFTER (accurate):
fn display_width(s: &str) -> usize {
    use unicode_width::UnicodeWidthStr;
    UnicodeWidthStr::width(s)
}
```

**Step 4: Add metadata lines to DagTask**

Extend the `DagTask` struct to support multiple metadata lines:

```rust
pub struct DagTask {
    pub id: String,
    pub verb: String,
    pub status: DagTaskStatus,
    /// Optional metadata (duration, tokens, error) — line 1
    pub meta: Option<String>,
    /// Additional property tags for check mode (structured, mcp, guardrails, etc.)
    pub tags: Vec<String>,
}
```

Update `render_v3_boxes` to render tag lines between meta and bottom border:

```rust
// After the meta line, render tag lines
for tag in &task.tags {
    let tag_display = format!("{}{}{}", " ".repeat(BOX_PAD), tag, " ".repeat(pad));
    let content = format!("║{}║", tag_display);
    tag_lines.push_str(&colorize(&content, task.status));
}
```

**Step 5: Build task tags from workflow analysis**

Create a helper that extracts tags from a workflow task:

```rust
/// Extract display tags from a workflow task for DAG boxes.
pub fn task_tags(task: &crate::ast::analyzed::Task) -> Vec<String> {
    let mut tags = Vec::new();

    // Structured output
    if task.structured.is_some() ||
       task.output.as_ref().and_then(|o| o.schema.as_ref()).is_some() {
        tags.push("structured".to_string());
    }
    // MCP server
    if let TaskAction::Invoke { invoke: params } = &task.action {
        if let Some(mcp) = &params.mcp {
            tags.push(format!("mcp:{}", mcp));
        }
    }
    // Agent MCP servers
    if let TaskAction::Agent { agent: params } = &task.action {
        if !params.mcp.is_empty() {
            tags.push(format!("mcp:{}", params.mcp.join(",")));
        }
    }
    // Timeout
    if let Some(t) = task.timeout {
        tags.push(format!("timeout:{}", t));
    }
    // Guardrails count
    if let Some(ref guards) = task.guardrails {
        if !guards.is_empty() {
            tags.push(format!("guardrails:{}", guards.len()));
        }
    }
    // Vision content
    if let TaskAction::Infer { infer: params } = &task.action {
        if let Some(ref content) = params.content {
            let img_count = content.iter()
                .filter(|c| c.content_type() == "image")
                .count();
            if img_count > 0 {
                tags.push(format!("vision:{}img", img_count));
            }
        }
    }
    // Fetch extract mode
    if let TaskAction::Fetch { fetch: params } = &task.action {
        if let Some(ref extract) = params.extract {
            tags.push(format!("extract:{}", extract));
        }
    }

    tags
}
```

**Step 6: Run tests**

```bash
cargo test --lib -- display 2>&1 | head -20
cargo check 2>&1 | tail -5
```

**Step 7: Commit**

```bash
git add src/display/legacy.rs
git commit -m "feat(display): upgrade DAG boxes with Cosmic icons + task metadata tags

Replace emoji verb icons with Cosmic Unicode palette.
Use unicode-width for accurate display width calculation.
Add metadata tag lines (structured, mcp, timeout, guardrails,
vision, extract) inside DAG boxes for nika check.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## Task 13: Wire CheckRenderer into `main.rs`

**Files:**
- Modify: `src/main.rs` (lines 755-835 `validate_workflow`, lines 871-1026 `validate_workflow_strict`)

**Step 1: Replace `validate_workflow()` output with CheckRenderer**

Replace the current `println!()` calls (lines 794-831) with:

```rust
if !quiet {
    // Header
    crate::display::check::print_check_header(
        file,
        false,
        env!("CARGO_PKG_VERSION"),
    );

    // Phase 1: Schema
    crate::display::check::print_phase(&PhaseResult {
        name: "schema",
        passed: true,
        detail: format!("YAML valid against @{}", SCHEMA_VERSION),
        duration_ms: schema_elapsed.as_millis() as u64,
        errors: vec![],
        hints: vec![],
    });

    // Phase 2: Parse
    crate::display::check::print_phase(&PhaseResult {
        name: "parse",
        passed: true,
        detail: format!(
            "{} tasks · provider: {} · model: {}",
            workflow.tasks.len(),
            workflow.provider,
            workflow.model.as_deref().unwrap_or("(default)")
        ),
        duration_ms: parse_elapsed.as_millis() as u64,
        errors: vec![],
        hints: vec![],
    });

    // Phase 3: Includes
    // Phase 4: DAG
    // Phase 5: Bindings
    // Phase 6: Schemas
    // ... same pattern for each phase

    // Advanced DAG boxes with Cosmic icons + tags
    if workflow.tasks.len() > 1 {
        let dag_tasks: Vec<DagTask> = workflow.tasks.iter().map(|t| {
            DagTask {
                id: t.id.clone(),
                verb: t.action.verb_name().to_string(),
                status: DagTaskStatus::Pending, // No execution status in check
                meta: None,
                tags: crate::display::legacy::task_tags(t),
            }
        }).collect();

        let mut deps_map = HashMap::new();
        for task in &workflow.tasks {
            if let Some(ref task_deps) = task.depends_on {
                deps_map.insert(task.id.clone(), task_deps.clone());
            }
        }

        render_dag(&dag_tasks, &deps_map);
    }

    // Summary footer
    crate::display::check::print_check_summary(
        true, total_elapsed.as_millis() as u64,
        workflow.tasks.len(), workflow.flow_count(),
        layer_count, schema_count,
        None, &[],
    );
}
```

**Step 2: Replace `validate_workflow_strict()` output similarly**

Same pattern but with:
- `print_check_header(file, true, ...)` — shows "STRICT" in title
- `print_mcp_validation(&mcp_results)` — MCP section
- DAG boxes with `DagTaskStatus::Success` or `DagTaskStatus::Failed` based on MCP results
- `print_check_summary(valid, ..., Some((valid_calls, total, errors)), &error_codes)`

**Step 3: Add timing instrumentation**

Wrap each validation phase with `Instant::now()` / `.elapsed()`:

```rust
let t = std::time::Instant::now();
validator.validate_yaml(&yaml)?;
let schema_elapsed = t.elapsed();
```

**Step 4: Handle error cases gracefully**

When a phase fails, show remaining phases as skipped:

```rust
match flow_graph.detect_cycles() {
    Ok(()) => print_phase(&PhaseResult { name: "dag", passed: true, ... }),
    Err(e) => {
        // Show DAG error with cycle path and hints
        print_phase(&PhaseResult {
            name: "dag",
            passed: false,
            detail: "CYCLE DETECTED".to_string(),
            errors: vec![e.cycle_path()],
            hints: vec![
                "Remove one dependency to break the cycle.".to_string(),
                "Common fix: use with: binding instead of depends_on.".to_string(),
            ],
            ..
        });
        // Skip remaining phases
        print_phase_skipped("bindings", "DAG invalid");
        print_phase_skipped("schemas", "DAG invalid");
        // Still show summary with error code
        print_check_summary(false, ..., &[("NIKA-020", "Circular dependency detected")]);
        return Err(e);
    }
}
```

**Step 5: Run tests**

```bash
cargo test --lib 2>&1 | tail -20
cargo check 2>&1 | tail -5
```

**Step 6: Commit**

```bash
git add src/main.rs
git commit -m "feat(cli): wire CheckRenderer into nika check + strict mode

Replace println! calls with CheckRenderer phases, MCP validation
section, upgraded DAG boxes with task tags, and summary footer.
Error cases show skipped phases, hint boxes, and NIKA-XXX codes.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## Task 14: Integration test — `nika check` manual verification

**Step 1: Test with a valid multi-task workflow**

```bash
cargo run -- check tests/fixtures/multi-task.nika.yaml
```

Check:
- [ ] Rounded-corner header shows "N I K A  C H E C K"
- [ ] 6 validation phases with ✓ and timing
- [ ] Advanced DAG boxes with Cosmic icons (✧⎈☄⊛❋)
- [ ] Task metadata tags inside boxes (structured, mcp, timeout, etc.)
- [ ] Rounded-corner summary shows "V A L I D"

**Step 2: Test with --strict**

```bash
cargo run -- check tests/fixtures/invoke-workflow.nika.yaml --strict
```

Check:
- [ ] Header shows "S T R I C T"
- [ ] MCP Validation section with ⊞ icons
- [ ] DAG boxes show ✓/✗ badges based on MCP validation
- [ ] Summary shows strict info

**Step 3: Test error cases**

```bash
# Cycle detection
cargo run -- check tests/fixtures/cycle-workflow.nika.yaml

# Bad bindings
cargo run -- check tests/fixtures/bad-bindings.nika.yaml

# Invalid YAML
cargo run -- check /dev/null
```

Check:
- [ ] Failed phase shows ✗ in red
- [ ] Error details indented under failed phase
- [ ] Hint box with fix suggestions
- [ ] Remaining phases show ⊘ skipped
- [ ] Summary shows "I N V A L I D" with NIKA-XXX codes

**Step 4: Commit**

```bash
git add -A
git commit -m "feat(cli): complete nika check output UX overhaul

Pre-flight checklist with 6 validation phases, advanced DAG boxes
with Cosmic icons and task metadata tags, MCP strict validation,
error hints, and compact summary with NIKA-XXX codes.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## Updated Summary

| Task | Description | Files | Commits |
|------|-------------|-------|---------|
| **Part 1: `nika run`** | | | |
| 1 | Split display.rs → display/ module + icons + colors | 4 new, 1 renamed | 1 |
| 2 | Make unicode-width + terminal_size always-on | 1 modified | 1 |
| 3 | DetailLevel enum + --detail CLI flag | 2 new, 1 modified | 1 |
| 4 | CliRenderer — event stream engine (core) | 1 new | 1 |
| 5 | HeaderRenderer — rounded-corner header | 1 new | 1 |
| 6 | DagRenderer — static DAG for run (print once) | 1 new | 1 |
| 7 | Wire into Runner — replace LiveDag | 2 modified | 1 |
| 8 | Update legacy display functions | 1 modified | 1 |
| 9 | Unit tests for run | 1 new | 1 |
| 10 | Integration test — nika run | — | 1 |
| **Part 2: `nika check`** | | | |
| 11 | CheckRenderer — validation checklist | 1 new | 1 |
| 12 | Upgrade DAG boxes — Cosmic icons + task metadata | 1 modified | 1 |
| 13 | Wire into main.rs — check + strict | 1 modified | 1 |
| 14 | Integration test — nika check | — | 1 |
| **Total** | | **15 new/modified** | **14 commits** |
