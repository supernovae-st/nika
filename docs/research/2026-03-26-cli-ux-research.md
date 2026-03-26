# Research Report: Beautiful CLI UX in Rust

**Date**: 2026-03-26
**Scope**: `nika run` (CLI display) + `nika ui` (TUI) + general CLI polish
**Current stack**: indicatif 0.18, colored 2.1, ratatui 0.30, terminal_size 0.4

---

## Summary

Nika's display system (7445 LOC across 15 files) is already well-structured with a LiveRenderer/CliRenderer split, cosmic icon palette, color helpers, and sparklines. This report identifies 20 concrete improvements drawn from research across Cargo, gh, Charm.sh tools, Turborepo, Ansible, Terraform, and modern Rust CLI crates. Each improvement targets a specific gap between the current implementation and the state of the art.

---

## Key Findings

### 1. Cargo-Style Right-Aligned Status Verbs

**What**: Cargo's output is scannable because status verbs (`Compiling`, `Downloading`, `Finished`) are right-aligned and bold green, forming a visual column. Everything else flows to the right.

**Current gap**: Nika's LiveRenderer uses `{spinner} {msg}` templates where the verb icon is left-aligned but task names vary in width, breaking alignment.

**Implementation**:
```rust
// Current:  ⠹ ✧ fetch_data       running  +2.3s
// Proposed: ✧ fetch_data ·········· running  2.3s  in:1.2k
//           ⎈ run_script ·········· done     0.1s
//           ☄ get_api ············· running  4.1s  out:3.4k

// Right-pad task names to max_task_name_width, fill with leader dots
const TASK_RUNNING_TEMPLATE: &str =
    "  {spinner:.cyan} {msg}";  // msg is pre-formatted with padding

fn format_task_line(icon: &str, name: &str, status: &str, duration: &str, max_width: usize) -> String {
    let leader = ".".repeat(max_width.saturating_sub(name.len()));
    format!("{} {} {} {} {}", icon, name, leader.dimmed(), status, duration)
}
```

**Applies to**: `nika run` (LiveRenderer)
**Effort**: Small (template + format_event changes)

---

### 2. Phase Transitions with Dynamic Style Switching

**What**: indicatif supports `set_style()` mid-progress to switch between spinner and bar modes. Cargo uses this to transition from "Resolving" (spinner) to "Downloading" (progress bar) to "Compiling" (per-crate bars).

**Current gap**: LiveRenderer uses fixed `style_running` and `style_static` cached at construction. No transitions.

**Implementation**:
```rust
// On WorkflowStarted: show spinner "Preparing..."
// On first TaskStarted: switch to DAG progress view
// On all tasks done: switch to summary style

fn transition_to_phase(&mut self, phase: Phase) {
    match phase {
        Phase::Preparing => {
            self.overall_bar.set_style(self.style_spinner.clone());
            self.overall_bar.set_message("Analyzing DAG...");
        }
        Phase::Executing => {
            self.overall_bar.set_style(self.style_progress.clone());
        }
        Phase::Summarizing => {
            self.overall_bar.finish_with_message("Done");
        }
    }
}
```

**Applies to**: `nika run` (LiveRenderer)
**Effort**: Medium (new Phase enum, transition logic)

---

### 3. Hierarchical Progress for for_each Loops

**What**: indicatif's MultiProgress supports parent + child bar hierarchies. Turborepo shows numbered sub-tasks under a parent task.

**Current gap**: `for_each` iterations appear as flat task bars. No visual nesting.

**Implementation**:
```rust
// Before:
//   ✧ process[0]   running  1.2s
//   ✧ process[1]   running  0.8s
//   ✧ process[2]   done     0.5s

// After:
//   ✧ process       ━━━━━━╸──── 1/3  2.1s
//     ├ [0] image_a  done   0.5s
//     ├ [1] image_b  running 0.8s
//     └ [2] image_c  running 1.2s

// Use insert_after() to place child bars under parent
let parent_bar = self.multi.add(ProgressBar::new(items.len() as u64));
for (i, item) in items.iter().enumerate() {
    let child = self.multi.insert_after(&parent_bar, ProgressBar::new_spinner());
    child.set_style(ProgressStyle::with_template("    {prefix} {msg}").unwrap());
    child.set_prefix(if i == items.len() - 1 { "└" } else { "├" });
}
```

**Applies to**: `nika run` (LiveRenderer)
**Effort**: Medium (for_each event handling, child bar management)

---

### 4. Streaming LLM Output with Live Markdown

**What**: Tools like Claude Code and Aider render streaming LLM tokens with live markdown formatting. The `streamdown` crate provides a Rust streaming markdown parser + renderer.

**Current gap**: `infer:` task output is shown as raw text in the summary, not streamed live.

**Implementation approach**:
- Add `streamdown-parser` + `streamdown-render` as optional dependencies behind a `live-markdown` feature
- During `infer:` execution, stream tokens through the parser and render above the task bars via `multi.println()`
- Fallback: batch render with `termimad` or custom `json_preview`-style renderer for code blocks

**Crate options**:
| Crate | Purpose | Size |
|-------|---------|------|
| `streamdown-parser` | Incremental markdown parsing | Light |
| `termimad` | Full markdown-to-ANSI rendering | ~30KB |
| `pulldown-cmark` | Event-based markdown parsing | Standard |
| `syntect` | Syntax highlighting for code blocks | Heavy (~2MB) |

**Applies to**: `nika run` (LiveRenderer verbose mode)
**Effort**: Large (new subsystem, feature-gated)

---

### 5. OSC 8 Clickable Hyperlinks for File Paths

**What**: Modern terminals (iTerm2, Kitty, WezTerm, Alacritty 0.13+) support OSC 8 escape sequences for clickable URLs. File paths in error messages, artifact paths, and source references become clickable.

**Current gap**: No hyperlink support. Paths are plain text.

**Implementation**:
```rust
// In display/colors.rs or a new display/hyperlink.rs

/// Emit an OSC 8 hyperlink if terminal supports it.
pub fn hyperlink(url: &str, text: &str) -> String {
    if supports_hyperlinks() {
        format!("\x1b]8;;{}\x1b\\{}\x1b]8;;\x1b\\", url, text)
    } else {
        text.to_string()
    }
}

/// File path as clickable file:// link
pub fn file_link(path: &std::path::Path) -> String {
    let abs = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    hyperlink(&format!("file://{}", abs.display()), &path.display().to_string())
}

fn supports_hyperlinks() -> bool {
    // Check TERM_PROGRAM for known supporters
    std::env::var("TERM_PROGRAM")
        .map(|t| matches!(t.as_str(), "iTerm.app" | "WezTerm" | "kitty"))
        .unwrap_or(false)
}
```

Use in error messages:
```
NIKA-011: Schema version mismatch in workflow.nika.yaml:3
                                       ^^^^^^^^^^^^^^^^^^ (clickable)
```

**Applies to**: `nika run`, `nika check`, error output
**Effort**: Small (helper function + integration points)

---

### 6. Adaptive Layout Based on Terminal Width

**What**: Responsive CLI output that adapts to terminal width, like CSS media queries. Comfy-table auto-queries width and truncates/pads columns.

**Current gap**: LiveRenderer uses hardcoded `{bar:30.cyan/dim}` width. No adaptation.

**Implementation**:
```rust
use terminal_size::{terminal_size, Width};

enum LayoutMode {
    Compact,   // < 80 cols: minimal, no sparklines
    Standard,  // 80-120 cols: default layout
    Wide,      // > 120 cols: full stats, sparklines, token breakdown
}

fn detect_layout() -> LayoutMode {
    match terminal_size() {
        Some((Width(w), _)) if w < 80 => LayoutMode::Compact,
        Some((Width(w), _)) if w > 120 => LayoutMode::Wide,
        _ => LayoutMode::Standard,
    }
}

// Adjust templates per mode
fn overall_template(mode: &LayoutMode) -> &'static str {
    match mode {
        LayoutMode::Compact => "  {bar:15.cyan/dim} {pos}/{len} {elapsed}",
        LayoutMode::Standard => "  {bar:30.cyan/dim} {pos}/{len}  {elapsed_precise}  {msg}",
        LayoutMode::Wide => "  {bar:40.cyan/dim} {pos}/{len}  {elapsed_precise}  {msg}  {per_sec}",
    }
}
```

**Applies to**: `nika run` (LiveRenderer), `nika check`
**Effort**: Medium (layout detection + template variants)

---

### 7. Ansible-Style Task Headers with Bold Phase Labels

**What**: Ansible's output is exceptionally scannable because PLAY and TASK headers are bold, colored, and visually separated. Terraform uses `+`/`-`/`~` diff symbols.

**Current gap**: Nika's log output blends together without strong visual separation between workflow phases.

**Implementation**:
```
  ╭──────────────────────────────────────────╮
  │  nika run  research-and-summarize        │
  │  provider: anthropic  model: sonnet-4    │
  ╰──────────────────────────────────────────╯

  TASK [research] ✧ infer ─────────────────────
    prompt: "Research the following topic..."
    temperature: 0.7

  TASK [summarize] ✧ infer ────────────────────
    depends_on: [research]
    prompt: "Create a concise summary..."
```

Use heavy box drawing (`━`) for the workflow header (already done) and medium (`─`) for task headers. Color the `TASK` label with the verb color.

**Applies to**: `nika run` (header.rs, format_event.rs)
**Effort**: Small (new format functions in format_event.rs)

---

### 8. Terraform-Style Diff Output for Workflow Changes

**What**: Terraform's `+`/`-`/`~` diff format is instantly recognizable. Apply this to `nika check` when comparing workflow versions or showing what changed.

**Implementation**:
```
  Plan: 3 tasks to run, 1 cached, 1 skipped

  + research      ✧ infer   (new)
  ~ summarize     ✧ infer   (prompt changed)
  = format_output ⎈ exec    (cached)
  - old_step      ☄ fetch   (removed)
```

**Applies to**: `nika check`, future `nika diff` command
**Effort**: Medium (new diff renderer)

---

### 9. Pulumi-Style Tree Output for DAG Visualization

**What**: Pulumi renders stack resources as nested trees with Unicode tree characters. This is superior to flat lists for showing dependencies.

**Current gap**: `dag_render.rs` (461 LOC) exists but could be enhanced with live status coloring during execution.

**Implementation**:
```
  Workflow: research-and-summarize
  ├── research        ✧ infer    ✓ 3.2s  $0.003
  │   └── summarize   ✧ infer    ● 1.8s  ...
  └── format_output   ⎈ exec     ○ pending
```

Tree characters:
```rust
const TREE_BRANCH: &str = "├──";
const TREE_LAST: &str = "└──";
const TREE_PIPE: &str = "│  ";
const TREE_SPACE: &str = "   ";
```

**Applies to**: `nika run` (summary), `nika check --tree`
**Effort**: Medium (tree layout algorithm for DAG with shared deps)

---

### 10. Rate-Limited Redraws for High-Frequency Updates

**What**: indicatif's `set_draw_rate()` throttles redraws. Critical for `for_each` with high concurrency or `agent:` loops with many tool calls.

**Current gap**: No explicit draw rate limiting. Each event triggers a redraw.

**Implementation**:
```rust
// In LiveRenderer::new()
overall_bar.set_draw_rate(15); // 15 fps max — smooth without CPU waste

// For task bars in hot loops
task_bar.set_draw_rate(10);
```

Also: batch token count updates instead of per-event:
```rust
// Instead of updating on every StreamChunk event:
if self.last_token_update.elapsed() > Duration::from_millis(100) {
    self.update_token_display(task_id);
    self.last_token_update = Instant::now();
}
```

**Applies to**: `nika run` (LiveRenderer)
**Effort**: Small (3-5 lines)

---

### 11. Sound Notification on Workflow Completion

**What**: Terminal BEL character (`\x07`) for audible notification when long workflows finish. Common in CI tools and build systems.

**Implementation**:
```rust
// In summary.rs, after printing the summary box
fn notify_completion(success: bool) {
    use std::io::IsTerminal;
    if std::io::stdout().is_terminal() {
        print!("\x07"); // BEL
    }
}
```

Guard with a `--quiet` / `--no-bell` flag or `NIKA_NO_BELL=1` env var.

**Applies to**: `nika run`
**Effort**: Trivial (2 lines + flag)

---

### 12. Gradient Text for Headers and Branding

**What**: Charm.sh tools use gradient text effects. The `owo-colors` crate supports zero-allocation coloring. Custom RGB gradients make headers feel premium.

**Implementation**:
```rust
/// Render text with a horizontal gradient between two RGB colors.
fn gradient(text: &str, from: (u8, u8, u8), to: (u8, u8, u8)) -> String {
    let len = text.chars().count().max(1);
    text.chars()
        .enumerate()
        .map(|(i, ch)| {
            let t = i as f32 / (len - 1).max(1) as f32;
            let r = lerp(from.0, to.0, t);
            let g = lerp(from.1, to.1, t);
            let b = lerp(from.2, to.2, t);
            format!("\x1b[38;2;{};{};{}m{}", r, g, b, ch)
        })
        .collect::<String>()
        + "\x1b[0m"
}

fn lerp(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t).round() as u8
}

// Usage in header:
// gradient("nika", (138, 43, 226), (0, 191, 255))  // purple -> cyan
```

Guard behind truecolor detection. Fallback to `colored` for 16-color terminals.

**Applies to**: `nika run` header, `nika --version`, `nika ui` splash
**Effort**: Small (new function in colors.rs)

---

### 13. Contextual Cost Coloring with Budget Bars

**What**: Show cumulative API cost as a visual budget bar alongside the overall progress. Bun-style speed metrics ("2.1x faster, $0.003 spent").

**Current gap**: Cost is shown in the summary only, not live during execution.

**Implementation**:
```rust
// Add to overall bar message:
// ━━━━━━━━━╸─────────────── 3/6  +5.2s  $0.004  ▓▓▓░░░░░ 12%

fn format_overall_msg(&self) -> String {
    let cost = colors::cost(self.stats.total_cost);
    let budget = if let Some(max) = self.budget_limit {
        let pct = (self.stats.total_cost / max * 100.0) as f32;
        format!(" {}", colors::budget_bar(pct, 8))
    } else {
        String::new()
    };
    format!("{}{}", cost, budget)
}
```

**Applies to**: `nika run` (LiveRenderer)
**Effort**: Small (already have `budget_bar` in colors.rs, wire it up)

---

### 14. Interactive Error Output with Code Snippets

**What**: Cargo and rustc use `annotate-snippets`-style error formatting with underlines, arrows, and contextual code. Nika errors reference YAML line numbers but show no snippet.

**Implementation**:
```
  error[NIKA-011]: Invalid schema version
    --> research.nika.yaml:1:9
     |
   1 | schema: "nika/workflow@0.11"
     |         ^^^^^^^^^^^^^^^^^^^^^ expected "nika/workflow@0.12"
     |
   = help: Update the schema line to: schema: "nika/workflow@0.12"
```

Use the `annotate-snippets` crate (same as rustc) or build a minimal version:
```rust
fn annotated_error(
    file: &str,
    line: usize,
    col: usize,
    source_line: &str,
    message: &str,
    help: Option<&str>,
) -> String {
    let line_num = format!("{}", line);
    let pad = " ".repeat(line_num.len());
    let underline = format!("{}{}", " ".repeat(col), "^".repeat(source_line.len() - col));
    let mut out = format!(
        "  {} {}\n  {} |\n  {} | {}\n  {} | {} {}\n",
        "-->".blue(),
        format!("{}:{}:{}", file, line, col).dimmed(),
        pad,
        line_num.blue(),
        source_line,
        pad,
        underline.red(),
        message.red(),
    );
    if let Some(h) = help {
        out += &format!("  {} = {}: {}\n", pad, "help".cyan(), h);
    }
    out
}
```

**Applies to**: `nika check`, `nika run` error output
**Effort**: Medium (source span tracking + renderer)

---

### 15. NO_COLOR and Color Mode Support

**What**: The no-color.org standard. Many CI environments set `NO_COLOR`. Professional CLIs respect it.

**Current gap**: Relies on `colored` crate's built-in detection but no explicit `--color` flag.

**Implementation**:
```rust
// In CLI args (clap)
#[arg(long, value_enum, default_value = "auto")]
color: ColorMode,

#[derive(ValueEnum, Clone)]
enum ColorMode { Auto, Always, Never }

// Apply early in main()
match args.color {
    ColorMode::Never => colored::control::set_override(false),
    ColorMode::Always => colored::control::set_override(true),
    ColorMode::Auto => {
        if std::env::var("NO_COLOR").is_ok() {
            colored::control::set_override(false);
        }
    }
}
```

**Applies to**: All CLI output
**Effort**: Small (clap arg + 5 lines)

---

### 16. Concurrently-Style Prefixed Output for Parallel Tasks

**What**: The `concurrently` npm tool prefixes each parallel process's output with a colored short label. Makes multiplexed output scannable.

**Current gap**: When `--verbose` shows task output, parallel tasks interleave without clear attribution.

**Implementation**:
```
  [research  ] Calling anthropic/claude-sonnet-4...
  [summarize ] Waiting for dependency: research
  [research  ] Received 1,234 tokens (3.2s)
  [research  ] ✓ Done
  [summarize ] Calling anthropic/claude-sonnet-4...
```

Each task gets a fixed-width label colored by its verb, ensuring alignment:
```rust
fn task_prefix(task_id: &str, verb: &str, max_width: usize) -> String {
    let label = format!("{:width$}", task_id, width = max_width);
    let colored_label = match verb {
        "infer" => label.magenta(),
        "exec" => label.yellow(),
        "fetch" => label.cyan(),
        "invoke" => label.green(),
        "agent" => label.red(),
        _ => label.white(),
    };
    format!("[{}]", colored_label)
}
```

**Applies to**: `nika run --verbose` (LiveRenderer println output)
**Effort**: Small (format function + integration)

---

### 17. Summary Box with Sparklines per Task

**What**: The `bottom` (btm) tool uses sparklines for CPU/memory per process. Nika can show token-per-task sparklines in the final summary.

**Current gap**: Summary box shows totals only, no per-task visual comparison.

**Implementation**:
```
  ╭─ Summary ──────────────────────────────────────────────────╮
  │  ✓ 4 tasks  3.2s  $0.012  in:4.2k out:8.1k               │
  │                                                            │
  │  research     ✧ 1.8s ▂▅▇█▆▃▁  $0.008  in:1.2k out:4.3k  │
  │  summarize    ✧ 1.1s ▁▃▆█▅▂▁  $0.003  in:3.4k out:2.1k  │
  │  format       ⎈ 0.2s           $0.000                     │
  │  publish      ☄ 0.1s           $0.001  out:1.7k           │
  ╰────────────────────────────────────────────────────────────╯
```

Use the existing `sparkline()` function but feed it per-task token streaming data (sampled at intervals during execution).

**Applies to**: `nika run` (summary.rs)
**Effort**: Medium (need to accumulate token samples during execution)

---

### 18. Rounded Corners for Boxes and Panels

**What**: Rounded corners (```) feel modern and softer than sharp corners (``). Charm.sh tools and Zellij use them extensively.

**Current gap**: Summary box uses sharp corners. Could alternate: rounded for info boxes, sharp for error boxes.

**Implementation**:
```rust
// Box drawing character sets
struct BoxChars {
    tl: &'static str, // top-left
    tr: &'static str, // top-right
    bl: &'static str, // bottom-left
    br: &'static str, // bottom-right
    h: &'static str,  // horizontal
    v: &'static str,  // vertical
}

const ROUNDED: BoxChars = BoxChars {
    tl: "\u{256D}", tr: "\u{256E}",  // ╭ ╮
    bl: "\u{2570}", br: "\u{256F}",  // ╰ ╯
    h: "\u{2500}", v: "\u{2502}",    // ─ │
};

const HEAVY: BoxChars = BoxChars {
    tl: "\u{250F}", tr: "\u{2513}",  // ┏ ┓
    bl: "\u{2517}", br: "\u{251B}",  // ┗ ┛
    h: "\u{2501}", v: "\u{2503}",    // ━ ┃
};
```

Use `ROUNDED` for success summaries, `HEAVY` for error boxes.

**Applies to**: `nika run` (summary.rs, header.rs)
**Effort**: Small (character constant swap)

---

### 19. TUI: Live DAG Widget with Animated Node Status

**What**: ratatui's `StatefulWidget` trait enables a custom DAG widget where nodes pulse/animate based on execution state. Similar to Nx's task graph visualization.

**Current gap**: `render_dag.rs` in nika-tui (461 LOC) exists but is static. Nodes don't animate during execution.

**Implementation approach**:
```rust
struct DagWidget;

impl StatefulWidget for DagWidget {
    type State = DagState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        for node in &state.nodes {
            let style = match node.status {
                Status::Pending => Style::default().fg(Color::DarkGray),
                Status::Running => {
                    // Pulse effect: alternate between bright and dim
                    let brightness = ((state.frame as f64 / 15.0).sin() + 1.0) / 2.0;
                    let g = (brightness * 255.0) as u8;
                    Style::default().fg(Color::Rgb(0, g, 255))
                }
                Status::Done => Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                Status::Failed => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            };
            // Render node box with status-colored border
            let block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(style)
                .title(Span::styled(&node.id, style));
            block.render(node.rect, buf);
        }
        // Render edges between nodes
        for edge in &state.edges {
            draw_edge(buf, edge.from, edge.to, Color::DarkGray);
        }
    }
}
```

**Applies to**: `nika ui` (Studio view DAG panel)
**Effort**: Large (full widget implementation, layout engine for node positioning)

---

### 20. TTFT (Time-to-First-Token) Display in Live Progress

**What**: LLM-specific metric. Show TTFT inline during `infer:` execution so users know if the model is responding.

**Current gap**: `ttft()` function exists in colors.rs but is only used in summary.

**Implementation**:
```
  ⠹ ✧ research      running  ttft:182ms  tokens:0
  ... (after first token arrives)
  ⠹ ✧ research      running  ttft:182ms  tokens:847  $0.003
```

Update the task bar message when the first StreamChunk event arrives:
```rust
EventKind::StreamChunk { task_id, .. } => {
    if let Some(start) = self.task_starts.get(task_id) {
        let ttft_ms = event.timestamp_ms - start.0;
        if !self.ttft_recorded.contains(task_id) {
            self.ttft_recorded.insert(task_id.clone());
            // Update task bar to show TTFT
            self.update_task_bar(task_id, |bar| {
                bar.set_message(format!("ttft:{}", colors::ttft(ttft_ms)));
            });
        }
    }
}
```

**Applies to**: `nika run` (LiveRenderer)
**Effort**: Small (event handler + display logic)

---

## Implementation Priority Matrix

| # | Improvement | Effort | Impact | Priority |
|---|------------|--------|--------|----------|
| 1 | Right-aligned status verbs | S | High | P0 |
| 10 | Rate-limited redraws | S | High | P0 |
| 15 | NO_COLOR support | S | High | P0 |
| 5 | OSC 8 hyperlinks | S | Medium | P1 |
| 11 | BEL notification | S | Low | P1 |
| 12 | Gradient text headers | S | Medium | P1 |
| 13 | Live cost budget bar | S | Medium | P1 |
| 16 | Prefixed parallel output | S | High | P1 |
| 18 | Rounded corners | S | Medium | P1 |
| 20 | TTFT live display | S | Medium | P1 |
| 2 | Phase transitions | M | High | P1 |
| 6 | Adaptive terminal width | M | High | P1 |
| 7 | Ansible-style task headers | S | High | P1 |
| 9 | Pulumi-style tree output | M | Medium | P2 |
| 14 | Annotated error snippets | M | High | P2 |
| 17 | Summary sparklines | M | Medium | P2 |
| 3 | Hierarchical for_each progress | M | Medium | P2 |
| 8 | Terraform-style diff | M | Low | P3 |
| 4 | Streaming LLM markdown | L | High | P3 |
| 19 | TUI animated DAG widget | L | High | P3 |

---

## Crate Recommendations

| Need | Crate | Status | Notes |
|------|-------|--------|-------|
| Coloring | `colored` 2.1 | Already used | Keep. Good ergonomics. |
| Progress | `indicatif` 0.18 | Already used | Use `set_draw_rate()`, `insert_after()` |
| TUI | `ratatui` 0.30 | Already used | Add custom `StatefulWidget` for DAG |
| Terminal size | `terminal_size` 0.4 | Already used | Use for adaptive layout |
| Markdown | `termimad` | Not used | Consider for rich summary output |
| Markdown streaming | `streamdown-parser` | Not used | For live LLM output (P3) |
| Error snippets | `annotate-snippets` | Not used | Cargo-quality error output |
| Interactive | `dialoguer` | Not used | For `nika init` wizard improvements |
| Tables | `comfy-table` | Not used | For `nika provider list` output |
| Fast coloring | `owo-colors` | Not used | Zero-alloc, consider for hot paths |

---

## Design Principles Extracted from Research

1. **Verb-led lines**: Every output line starts with a status verb or icon. The eye anchors on the left column.
2. **Color budget**: Max 4-5 colors per screen. Semantic only (green=success, red=error, yellow=warning, cyan=info, magenta=AI).
3. **Progressive disclosure**: Quiet by default, verbose on demand. `--verbose` adds token counts, `--debug` adds full payloads.
4. **Alignment matters**: Right-pad task names, right-align numbers, use leader dots for scannability.
5. **Whitespace is information**: Blank lines between phases. Indentation for hierarchy. Breathing room in boxes.
6. **Test on dark AND light**: Use 16 ANSI colors as baseline. Never hardcode RGB without truecolor detection fallback.
7. **Respect the environment**: `NO_COLOR`, `TERM=dumb`, piped output, CI environments. Degrade gracefully.
8. **Animation is feedback, not decoration**: Spinners mean "working". Progress bars mean "known duration". Static means "waiting" or "done".

---

## Sources

1. indicatif crate documentation (crates.io/docs.rs) - Progress bar patterns, MultiProgress, templates
2. Perplexity search: "rust cli beautiful output indicatif examples" - Cargo, ripgrep, bat patterns
3. Perplexity search: "rust cli animation terminal techniques" - zenity, r3bl_terminal_async, console
4. Perplexity search: "cargo build output design" - Status verb pattern, color hierarchy
5. Perplexity search: "indicatif multi progress custom templates" - Nested progress, rate limiting, tokio
6. Perplexity search: "terminal sparklines charts rust" - textplots, comfy-table, ascii-dag
7. Perplexity search: "cli output design system color palette" - NO_COLOR, semantic colors, responsive
8. Perplexity search: "cli workflow engine output" - Turborepo, Ansible, Terraform, Pulumi
9. Perplexity search: "OSC 8 hyperlinks terminal rust" - osc8 crate, terminal detection
10. Perplexity search: "best CLI UX examples" - Charm.sh, Atuin, Zellij, Just, Mise
11. Perplexity search: "rust daemon cli management" - daemonize, PID files, graceful shutdown
12. Perplexity search: "streaming LLM output terminal" - streamdown crate, termimad, pulldown-cmark
13. Perplexity search: "ratatui animation widget techniques" - Custom widgets, sparklines, DAG rendering
14. Perplexity search: "rust colored terminal crate comparison" - owo-colors, anstyle, dialoguer, inquire
15. Perplexity search: "terminal capability detection rust" - console, terminal-size, concolor
16. Perplexity search: "streamdown crate terminal markdown" - Architecture, partial token handling

---

## Methodology

- Tools used: Perplexity AI (sonar-pro), source code analysis
- Pages analyzed: 16 search queries, 10+ display system source files
- Current codebase: 7445 LOC display system, indicatif 0.18, colored 2.1, ratatui 0.30
- Focus: Actionable improvements with concrete Rust code patterns

## Confidence Level

**High** - Recommendations are grounded in real crate APIs (indicatif 0.18, ratatui 0.30), verified against Nika's current source code, and prioritized by effort/impact. Code snippets use APIs confirmed to exist in the versions already in use.

## Further Research Suggestions

- **A/B test output formats**: Record terminal sessions with `asciinema` to compare before/after
- **Benchmark draw performance**: Profile indicatif draw overhead during high-frequency `for_each` loops
- **Investigate `anstyle`**: Cargo is migrating from `colored` to `anstyle` for zero-alloc styling. May be worth evaluating for hot paths.
- **Explore `ratatui-image`**: For `nika ui` to show generated charts/images inline in supported terminals
- **Study Charm.sh Bubble Tea**: While Go-based, its component model (viewport, spinner, progress, text input) maps well to ratatui widget patterns
