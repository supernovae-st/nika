# Research Report: indicatif v0.18 Advanced Patterns

## Summary

indicatif v0.18.4 (latest as of March 2026, MIT license, by mitsuhiko + djc) is the standard Rust crate for terminal progress bars and spinners. It provides thread-safe `ProgressBar` and `MultiProgress` types that draw to stderr by default, with a powerful template engine for styling, built-in rate limiting (20 fps default), and a rich set of placeholder keys. This report covers all 10 advanced topics requested, drawn directly from the source code at `console-rs/indicatif` on GitHub and docs.rs.

## Key Findings

---

### 1. MultiProgress with Dynamic Add/Remove Bars

`MultiProgress` manages multiple bars rendered together. Bars can be added, inserted at specific positions, and removed at runtime.

**Core API:**

```rust
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

let mp = MultiProgress::new();

// Add at the bottom
let pb1 = mp.add(ProgressBar::new(100));

// Insert at specific index
let pb2 = mp.insert(0, ProgressBar::new(50));  // inserts at top

// Insert relative to another bar
let pb3 = mp.insert_after(&pb1, ProgressBar::new(200));
let pb4 = mp.insert_before(&pb2, ProgressBar::new(75));

// Insert from the back (index from end)
let pb5 = mp.insert_from_back(0, ProgressBar::new(30)); // same as add()

// Remove a bar dynamically
mp.remove(&pb2);

// Clear all bars from screen
mp.clear().unwrap();
```

**Important behaviors:**
- Adding a bar that is already a member of the `MultiProgress` is a no-op.
- When a bar finishes, it becomes a "zombie" -- it stays rendered on screen but gets reaped from tracking on next draw cycle. Consecutive zombie bars at the top of the ordering are reaped first.
- `remove()` checks that the bar belongs to this specific `MultiProgress` via `Arc::ptr_eq`.
- All insertion methods change the bar's draw target to a remote target intercepted by the `MultiProgress`.

**Alignment:**

```rust
use indicatif::MultiProgressAlignment;

// Bars grow downward from top (default)
mp.set_alignment(MultiProgressAlignment::Top);

// Bars grow upward from bottom
mp.set_alignment(MultiProgressAlignment::Bottom);
```

Bottom alignment is useful for chat-like UIs where new items appear at the bottom. Enable it with:
```
cargo run --example multi-tree-ext -- --bottom-alignment
```

**Dynamic tree pattern** (from `multi-tree.rs` and `multi-tree-ext.rs`):

```rust
// Add bars dynamically based on tree structure
let mp = Arc::new(MultiProgress::new());

// Insert at position with indentation via prefix
let pb = mp.insert(item.index + 1, item.progress_bar.clone());
pb.set_message(format!("{}  {}", "  ".repeat(item.indent), item.key));

// Remove completed temporary bars
mp.remove(&item.progress_bar);
```

**Source:** `src/multi.rs` lines 1-250, `examples/multi-tree-ext.rs`

---

### 2. ProgressStyle Template Syntax -- All Placeholders

Templates use `{key:options}` format where options are `<alignment><width><!truncate><.style></alt_style>`.

**Complete placeholder reference (from source `format_state` in `src/style.rs`):**

| Key | Description | Example Output |
|-----|-------------|---------------|
| `{bar}` | Fixed-width progress bar (default 20 chars). Style = filled portion, alt_style = empty portion. | `##########----------` |
| `{wide_bar}` | Fills remaining terminal width. Do NOT combine with `{wide_msg}`. | `################----` |
| `{spinner}` | Current tick string from tick animation. | `*` |
| `{prefix}` | Prefix string (set via `set_prefix()`). | `[1/4]` |
| `{msg}` | Message string (set via `set_message()`). | `downloading...` |
| `{wide_msg}` | Message filling remaining width, with truncation. Do NOT combine with `{wide_bar}`. | `downloading file...` |
| `{pos}` | Current position as raw integer. | `42` |
| `{human_pos}` | Current position with thousands separators. | `1,234,567` |
| `{len}` | Total length as raw integer. | `100` |
| `{human_len}` | Total length with thousands separators. | `1,234,567` |
| `{percent}` | Percentage as integer (0 decimal places). | `42` |
| `{percent_precise}` | Percentage with 3 decimal places. | `42.857` |
| `{bytes}` | Current position as human bytes (binary/IEC: KiB, MiB). | `1.50 MiB` |
| `{total_bytes}` | Total length as human bytes (binary/IEC). | `231.23 MiB` |
| `{decimal_bytes}` | Current position as SI bytes (kB, MB). | `1.57 MB` |
| `{decimal_total_bytes}` | Total length as SI bytes. | `231.23 MB` |
| `{binary_bytes}` | Current position as IEC bytes (KiB, MiB). | `1.50 MiB` |
| `{binary_total_bytes}` | Total length as IEC bytes. | `231.23 MiB` |
| `{elapsed}` | Elapsed time, human-readable short. | `42s`, `1m`, `2h` |
| `{elapsed_precise}` | Elapsed time as `HH:MM:SS`. | `00:01:42` |
| `{eta}` | Estimated time remaining, human-readable short. | `30s` |
| `{eta_precise}` | Estimated time remaining as `HH:MM:SS`. | `00:00:30` |
| `{duration}` | Extrapolated total duration, human-readable. | `2m` |
| `{duration_precise}` | Extrapolated total duration as `HH:MM:SS`. | `00:02:12` |
| `{per_sec}` | Speed in items/second. | `1.23K/s` |
| `{bytes_per_sec}` | Speed in bytes/second (binary). | `1.50 MiB/s` |
| `{decimal_bytes_per_sec}` | Speed in bytes/second (SI). | `1.57 MB/s` |
| `{binary_bytes_per_sec}` | Speed in bytes/second (IEC). | `1.50 MiB/s` |

**Note:** `bytes` is an alias for `binary_bytes`, and `total_bytes` is an alias for `binary_total_bytes`.

**Template option syntax:**

```text
{key:<alignment><width><!truncate><.style></alt_style>}

alignment:  < (left), ^ (center), > (right)
width:      positive integer
truncate:   ! character enables truncation
style:      dotted style string (see section 7)
alt_style:  after / separator, dotted style string
```

**Examples:**

```rust
// Right-aligned position, 7 chars wide
"{pos:>7}"

// Bar, 40 chars, cyan filled / blue empty
"{bar:40.cyan/blue}"

// Wide bar with colors
"{wide_bar:.cyan/blue}"

// Truncated message, 20 chars, left-aligned
"{msg:<20!}"

// Bold dim prefix, right-aligned, 12 chars
"{prefix:>12.bold.dim}"

// Center-aligned wide message
"{wide_msg:^}"
```

**Custom keys via `with_key()`:**

```rust
use std::fmt::Write;
use indicatif::{ProgressStyle, ProgressState};

let style = ProgressStyle::with_template("{spinner} [{wide_bar}] {bytes}/{total_bytes} ({eta})")
    .unwrap()
    .with_key("eta", |state: &ProgressState, w: &mut dyn Write| {
        write!(w, "{:.1}s", state.eta().as_secs_f64()).unwrap()
    })
    .progress_chars("#>-");
```

**Stateful custom keys via `ProgressTracker` trait:**

```rust
use indicatif::style::ProgressTracker;

#[derive(Debug, Clone)]
struct RateTracker { /* state */ }

impl ProgressTracker for RateTracker {
    fn clone_box(&self) -> Box<dyn ProgressTracker> { Box::new(self.clone()) }
    fn tick(&mut self, state: &ProgressState, now: Instant) { /* update state */ }
    fn reset(&mut self, _state: &ProgressState, _now: Instant) { /* reset */ }
    fn write(&self, state: &ProgressState, w: &mut dyn fmt::Write) {
        write!(w, "{:.2}/s", self.rate).unwrap();
    }
}
```

**Literal braces:** Use `{{` and `}}` to escape braces in templates.

**Source:** `src/lib.rs` lines 118-193, `src/style.rs` lines 267-400

---

### 3. Custom tick_strings for Spinners -- Best Unicode Spinner Sets

**Two methods to set spinners:**

```rust
// Method 1: tick_chars() -- each char is one frame, last char = finished state
.tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ")

// Method 2: tick_strings() -- each &str is one frame (multi-char), last = finished state
.tick_strings(&[
    "▹▹▹▹▹",
    "▸▹▹▹▹",
    "▹▸▹▹▹",
    "▹▹▸▹▹",
    "▹▹▹▸▹",
    "▹▹▹▹▸",
    "▪▪▪▪▪",  // <-- last string = finished state
])
```

**Rule:** At least 2 entries required (1 animation frame + 1 final state). The last entry is always the finished state and is never used during animation.

**Default spinner** (from source):
```
⠁⠁⠉⠙⠚⠒⠂⠂⠒⠲⠴⠤⠄⠄⠤⠠⠠⠤⠦⠖⠒⠐⠐⠒⠓⠋⠉⠈⠈
```
(Braille dots pattern, space as final state)

**Best Unicode spinner sets** (curated from cli-spinners + indicatif examples):

```rust
// Braille dots (default, smooth rotation)
.tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ")

// Braille spinner (classic)
.tick_chars("⣾⣽⣻⢿⡿⣟⣯⣷ ")

// Block bounce
.tick_strings(&["▹▹▹▹▹", "▸▹▹▹▹", "▹▸▹▹▹", "▹▹▸▹▹", "▹▹▹▸▹", "▹▹▹▹▸", "▪▪▪▪▪"])

// Dots
.tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ ")

// Moon phases
.tick_chars("🌑🌒🌓🌔🌕🌖🌗🌘 ")

// Clock
.tick_strings(&["🕛","🕐","🕑","🕒","🕓","🕔","🕕","🕖","🕗","🕘","🕙","🕚","*"])

// Box drawing
.tick_chars("┤┘┴└├┌┬┐ ")

// Line drawing
.tick_chars("|/-\\ ")

// ASCII spinner (from cargowrap.rs)
.tick_chars("/|\\- ")

// Arrow
.tick_strings(&["←","↖","↑","↗","→","↘","↓","↙","*"])

// Growing dots
.tick_strings(&["   ",".  ",".. ","...",".. ",".  ","   ","*"])

// Bouncing bar
.tick_strings(&["[    ]","[=   ]","[==  ]","[=== ]","[ ===]","[  ==]","[   =]","[    ]","[   =]","[  ==]","[ ===]","[====]","[=== ]","[==  ]","[=   ]","[done]"])
```

**Tip:** Reference [sindresorhus/cli-spinners](https://github.com/sindresorhus/cli-spinners/blob/master/spinners.json) for 80+ spinner designs. The indicatif `long-spinner.rs` example links to this directly.

**Source:** `src/style.rs` lines 129-165, `examples/long-spinner.rs`

---

### 4. MultiProgress::println() -- Log Lines Above Fixed Bars

`MultiProgress::println()` emits a line of text above all progress bars. The bars are redrawn below the new text, maintaining their position.

```rust
use indicatif::{MultiProgress, ProgressBar};

let mp = MultiProgress::new();
let pb1 = mp.add(ProgressBar::new(100));
let pb2 = mp.add(ProgressBar::new(200));

// Print log line above all bars
mp.println("starting!").unwrap();

// From another thread, clone the MultiProgress
let mp_clone = mp.clone();
std::thread::spawn(move || {
    // This also works from threads
    mp_clone.println("pb3 is done!").unwrap();
});
```

**Individual bar println:**

```rust
// ProgressBar also has println -- if attached to a MultiProgress,
// the line appears above ALL bars in the MultiProgress
let pb = ProgressBar::new(100);
for i in 0..100 {
    pb.println(format!("[+] finished #{i}"));
    pb.inc(1);
}
pb.finish_with_message("done");
```

**Behavior details:**
- If the draw target is hidden (non-terminal), `println()` is a no-op.
- Empty strings still trigger a newline.
- Multi-line messages are split and each line is tracked separately for correct re-rendering.
- When called, all zombie lines are cleared first (zombie = finished bars still on screen), then the new text is drawn, then all active bars are redrawn below.

**Cargo-style pattern** (from `examples/cargo.rs`):

```rust
let green_bold = Style::new().green().bold();

// Print compilation status above progress bar
let line = format!(
    "{:>12} {} {}",
    green_bold.apply_to("Compiling"),
    name,
    version
);
pb.println(line);
pb.inc(1);
```

**Source:** `src/multi.rs` `println()`, `src/state.rs` `println()`, `examples/log.rs`, `examples/cargo.rs`

---

### 5. ProgressBar::suspend() for Temporary Output

`suspend()` temporarily hides the progress bar(s), runs your closure, then redraws everything.

```rust
let pb = ProgressBar::new(100);

// Hide bar, print, redraw bar
pb.suspend(|| {
    println!("This log line won't overlap with the bar");
    eprintln!("This works for stderr too");
});
```

**Key differences from `println()`:**

| Feature | `println()` | `suspend()` |
|---------|-------------|-------------|
| Draw target hidden | No-op (silent) | **Still executes `f`** |
| Output placement | Above bars | Wherever you print |
| Use case | Structured log lines | External code that writes to stdout |
| Lock held | Brief | **Entire duration of `f`** |
| MultiProgress | Prints above all bars | Suspends entire MultiProgress |

**With MultiProgress:**

```rust
let mp = MultiProgress::new();
let pb = mp.add(ProgressBar::new(100));

// Both work:
mp.suspend(|| {
    println!("All bars hidden temporarily");
});

// Or via a bar attached to the MultiProgress:
pb.suspend(|| {
    println!("Also suspends the entire MultiProgress");
});
```

**Warning:** The internal lock is held while `f` executes. Other threads trying to update any bar in the same `MultiProgress` will block. Keep `f` short.

**Source:** `src/progress_bar.rs` `suspend()`, `src/multi.rs` `suspend()`

---

### 6. Drawing Modes: stderr vs stdout, Hidden for Tests

**Default: stderr** (since most programs write actual output to stdout)

```rust
use indicatif::{ProgressBar, ProgressDrawTarget};

// Default -- draws to stderr at 20 fps
let pb = ProgressBar::new(100);

// Explicit stderr
let pb = ProgressBar::with_draw_target(Some(100), ProgressDrawTarget::stderr());

// Draw to stdout instead
let pb = ProgressBar::with_draw_target(Some(100), ProgressDrawTarget::stdout());

// Custom refresh rate (frames per second)
let pb = ProgressBar::with_draw_target(
    Some(100),
    ProgressDrawTarget::stderr_with_hz(30),  // 30 fps
);
let pb = ProgressBar::with_draw_target(
    Some(100),
    ProgressDrawTarget::stdout_with_hz(10),  // 10 fps
);
```

**Hidden mode (critical for tests):**

```rust
// Completely hidden -- no rendering, no output, but still tracks state
let pb = ProgressBar::hidden();

// Check if hidden
assert!(pb.is_hidden());

// MultiProgress hidden
let mp = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
assert!(mp.is_hidden());
```

**Auto-hiding behavior:**
- If output is not a TTY (piped to file), bars are automatically hidden.
- If `NO_COLOR` environment variable is set, bars are hidden.
- If `TERM` is unset or set to `dumb`, bars are hidden.

**Custom draw target (for testing or custom renderers):**

```rust
use indicatif::TermLike;

// Implement TermLike for custom output
struct TestOutput { /* buffer */ }

impl TermLike for TestOutput {
    fn width(&self) -> u16 { 80 }
    fn move_cursor_up(&self, n: usize) -> std::io::Result<()> { Ok(()) }
    fn move_cursor_down(&self, n: usize) -> std::io::Result<()> { Ok(()) }
    fn move_cursor_right(&self, n: usize) -> std::io::Result<()> { Ok(()) }
    fn clear_line(&self) -> std::io::Result<()> { Ok(()) }
    fn write_line(&self, s: &str) -> std::io::Result<()> { Ok(()) }
    fn write_str(&self, s: &str) -> std::io::Result<()> { Ok(()) }
    fn flush(&self) -> std::io::Result<()> { Ok(()) }
}

// Use it
let target = ProgressDrawTarget::term_like_with_hz(Box::new(TestOutput {}), 20);
```

**In-memory terminal (feature-gated):**

```rust
// Cargo.toml: indicatif = { version = "0.18", features = ["in_memory"] }
use indicatif::InMemoryTerm;

let term = InMemoryTerm::new(10, 80); // 10 rows, 80 cols
let pb = ProgressBar::with_draw_target(
    Some(100),
    ProgressDrawTarget::term_like(Box::new(term.clone())),
);
// Can read back what was rendered:
let output = term.contents();
```

**MultiProgress draw target:**

```rust
// Set draw target for the whole MultiProgress
let mp = MultiProgress::with_draw_target(ProgressDrawTarget::stderr_with_hz(15));

// Default refresh: MultiProgress = 15 fps, ProgressBar = 20 fps
```

**Source:** `src/draw_target.rs` lines 1-200, `src/progress_bar.rs` `new()`, `hidden()`

---

### 7. Color Styling in Templates -- The Dotted Syntax

Colors use the `console` crate's `Style::from_dotted_str()` syntax. Styles are specified as dot-separated attributes after the width in a template placeholder.

**Format:** `{key:width.style1.style2/alt_style1.alt_style2}`

**Available style attributes:**

| Attribute | Effect |
|-----------|--------|
| `bold` | Bold text |
| `dim` | Dimmed text |
| `italic` | Italic text |
| `underlined` | Underlined text |
| `blink` | Blinking text |
| `reverse` | Reversed colors |
| `strikethrough` | Struck-through text |
| `black` | Black foreground |
| `red` | Red foreground |
| `green` | Green foreground |
| `yellow` | Yellow foreground |
| `blue` | Blue foreground |
| `magenta` | Magenta foreground |
| `cyan` | Cyan foreground |
| `white` | White foreground |
| `on_black` | Black background |
| `on_red` | Red background |
| `on_green` | Green background |
| `on_yellow` | Yellow background |
| `on_blue` | Blue background |
| `on_magenta` | Magenta background |
| `on_cyan` | Cyan background |
| `on_white` | White background |
| `bright_black` | Bright black (gray) foreground |
| `bright_red` | Bright red foreground |
| `bright_green` | etc. |
| `bright_yellow` | etc. |
| `bright_blue` | etc. |
| `bright_magenta` | etc. |
| `bright_cyan` | etc. |
| `bright_white` | etc. |
| `on_bright_*` | Bright background variants |
| `color256(N)` | 256-color foreground (0-255) |
| `on_color256(N)` | 256-color background |

**Examples in templates:**

```rust
// Green spinner
"{spinner:.green}"

// Bold dim prefix
"{prefix:.bold.dim}"

// Cyan bar with blue background for unfilled
"{bar:40.cyan/blue}"
// Equivalent to: filled portion in cyan, unfilled in blue

// Red on blue foreground, green on cyan alt (for wide_bar)
"{wide_bar:.red.on_blue/green.on_cyan}"

// Bold cyan prefix, right-aligned 12 chars
"{prefix:>12.cyan.bold}"

// Green/yellow bar
"{bar:40.green/yellow}"

// Blue spinner
"{spinner:.blue}"

// Green spinner with dim bold
"{spinner:.dim.bold}"
```

**In the alt_style context (after `/`):**
The alt_style is primarily used for `{bar}` and `{wide_bar}` -- it colors the unfilled/remaining portion of the bar.

```rust
// Filled = cyan, Remaining = blue
ProgressStyle::with_template("{wide_bar:.cyan/blue}")
```

**Programmatic coloring (using `console` crate directly):**

```rust
use console::{style, Style, Emoji};

// Inline colored strings (not in templates)
let green_bold = Style::new().green().bold();
println!("{}", green_bold.apply_to("Compiling"));

// Or using the style() shorthand
println!("{}", style("Building").bold().dim());
```

**Source:** `src/style.rs` `from_dotted_str` usage, `console` crate docs, `examples/yarnish.rs`, `examples/cargo.rs`

---

### 8. Handling 20+ Parallel Bars Without Flickering

**Problem:** Many simultaneous bars cause visible flickering because each update clears and redraws all bars.

**Solution 1: `set_move_cursor(true)`**

```rust
let mp = MultiProgress::new();

// Use cursor movement instead of clearing lines
// REDUCES FLICKERING but do NOT use if you add/remove bars dynamically
mp.set_move_cursor(true);
```

When enabled, the draw target uses cursor movement (`\x1b[A` up, `\x1b[B` down) instead of clearing entire lines. This is much faster but only works when the number of bars is stable.

**Solution 2: Lower the refresh rate**

```rust
// Default MultiProgress: 15 fps
// For 20+ bars, consider 10 fps
let mp = MultiProgress::with_draw_target(ProgressDrawTarget::stderr_with_hz(10));
```

**Solution 3: Rate-limited increments via `AtomicPosition`**

indicatif already has built-in rate limiting. The `inc()` method uses `AtomicPosition::allow()` to skip draws when calls are too frequent:

```rust
// From ProgressBar::inc() in the source:
pub fn inc(&self, delta: u64) {
    self.pos.inc(delta);
    let now = Instant::now();
    if self.pos.allow(now) {   // <-- rate limiter
        self.tick_inner(now);
    }
}
```

The internal rate limiter ensures at most ~20 draws/second per bar regardless of how often you call `inc()`.

**Solution 4: Use `ProgressBar::hidden()` for non-visible work**

```rust
// Only show top-N active bars, hide the rest
let visible_bars: Vec<_> = (0..20).map(|_| mp.add(ProgressBar::new(100))).collect();
// Tasks beyond 20 use hidden bars
let hidden_bar = ProgressBar::hidden();
```

**Solution 5: Batch updates**

```rust
// Instead of calling inc(1) in a tight loop, batch:
let pb = ProgressBar::new(1_000_000);
let mut count = 0;
for item in items {
    process(item);
    count += 1;
    if count % 100 == 0 {
        pb.inc(100);
    }
}
```

**Solution 6: Use `enable_steady_tick()` instead of manual ticking**

```rust
// For spinners, don't tick manually -- use steady tick
let pb = mp.add(ProgressBar::new_spinner());
pb.enable_steady_tick(Duration::from_millis(100)); // 10 fps
pb.set_message("Processing...");
// Just update the message, the drawing happens in background
pb.set_message("Still processing...");
```

**Architecture recommendation for 20+ bars:**

```rust
// 1. Use MultiProgress with lower Hz
let mp = MultiProgress::with_draw_target(ProgressDrawTarget::stderr_with_hz(10));

// 2. If bar count is stable, enable move_cursor
mp.set_move_cursor(true);

// 3. Keep a "header" bar + limited visible bars
let header = mp.add(ProgressBar::new(total_tasks as u64));
header.set_style(ProgressStyle::with_template(
    "{prefix:.bold} [{bar:50.green/yellow}] {pos}/{len} ({eta})"
).unwrap());
header.set_prefix("Overall");

// 4. Pool of N worker bars (reuse them)
let worker_bars: Vec<_> = (0..8).map(|i| {
    let pb = mp.add(ProgressBar::new_spinner());
    pb.set_style(spinner_style.clone());
    pb.set_prefix(format!("[{}]", i + 1));
    pb
}).collect();

// 5. Completed tasks go through println
mp.println(format!("  {} task_name", style("done").green())).unwrap();
```

**Source:** `src/multi.rs` `set_move_cursor()`, `src/draw_target.rs` `RateLimiter`, `src/progress_bar.rs` `inc()`

---

### 9. Performance Considerations

**Draw overhead and rate limiting:**

The internal rate limiter (`RateLimiter` in `draw_target.rs`) prevents excessive redraws:
- `ProgressBar::new()` defaults to `stderr()` which is 20 fps
- `MultiProgress::new()` defaults to `stderr()` which is 15 fps
- `ProgressDrawTarget::stderr_with_hz(N)` lets you set custom fps

**`inc()` is nearly free:**

```rust
// inc() uses atomic operations and only triggers a draw when the rate limiter allows
pub fn inc(&self, delta: u64) {
    self.pos.inc(delta);           // atomic add -- nanoseconds
    let now = Instant::now();
    if self.pos.allow(now) {       // checks if enough time elapsed
        self.tick_inner(now);      // only redraws if rate allows
    }
}
```

**Benchmark insight (from `examples/fastbar.rs`):**

The `fastbar` example demonstrates that even with 1M iterations, the overhead is dominated by the 20 fps draw rate, not the `inc()` calls. The atomic counter is essentially free.

```rust
// From fastbar.rs: 1<<20 = 1,048,576 iterations
// The progress bar adds negligible overhead because draws are rate-limited
let pb = ProgressBar::new(n);
for i in 0..n {
    sum += 2 * i + 3;
    pb.inc(1);     // ~nanoseconds per call (atomic add + time check)
}
```

**When to use `enable_steady_tick()`:**

```rust
// For slow tasks where progress updates are infrequent
let pb = ProgressBar::new_spinner();
pb.enable_steady_tick(Duration::from_millis(100));

// The spinner ticks in a background thread at fixed intervals
// Your code only needs to call set_message() -- no tick() calls needed
```

**Warning:** When `enable_steady_tick()` is active, manual `tick()` calls are ignored (the ticker thread owns ticking).

**`force_draw()` for critical updates:**

```rust
// Bypasses rate limiter for one draw
pb.force_draw();
```

Use sparingly -- only when a state change absolutely must be visible immediately.

**Performance tips summary:**

1. **Do nothing special for most cases** -- the 20 fps rate limiter handles it.
2. **For tight loops (>10K iterations/sec):** `inc()` is already fast; no batching needed.
3. **For 20+ bars:** Lower Hz to 10 on the `MultiProgress`.
4. **For spinners:** Use `enable_steady_tick()` rather than manual `tick()`.
5. **For tests:** Use `ProgressBar::hidden()` to skip all rendering.
6. **`is_hidden()` guard:** Check `pb.is_hidden()` before expensive message formatting:
   ```rust
   if !pb.is_hidden() {
       pb.set_message(format!("Processing {}: {}", expensive_name(), status));
   }
   ```

**Source:** `src/draw_target.rs` `RateLimiter`, `src/state.rs` `AtomicPosition`, `examples/fastbar.rs`

---

### 10. Complete Working Example: DAG Task Runner Display

```rust
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use console::style;
use indicatif::{
    HumanDuration, MultiProgress, MultiProgressAlignment, ProgressBar, ProgressDrawTarget,
    ProgressFinish, ProgressStyle,
};
use rand::Rng;

/// Represents a task in the DAG
#[derive(Clone)]
struct Task {
    id: String,
    duration_ms: u64,
    depends_on: Vec<String>,
}

fn main() {
    let started = std::time::Instant::now();

    // --- Define DAG ---
    let tasks = vec![
        Task { id: "parse".into(),     duration_ms: 800,  depends_on: vec![] },
        Task { id: "validate".into(),  duration_ms: 600,  depends_on: vec![] },
        Task { id: "analyze".into(),   duration_ms: 1200, depends_on: vec!["parse".into()] },
        Task { id: "optimize".into(),  duration_ms: 1500, depends_on: vec!["analyze".into(), "validate".into()] },
        Task { id: "codegen_a".into(), duration_ms: 900,  depends_on: vec!["optimize".into()] },
        Task { id: "codegen_b".into(), duration_ms: 700,  depends_on: vec!["optimize".into()] },
        Task { id: "codegen_c".into(), duration_ms: 1100, depends_on: vec!["optimize".into()] },
        Task { id: "link".into(),      duration_ms: 2000, depends_on: vec!["codegen_a".into(), "codegen_b".into(), "codegen_c".into()] },
        Task { id: "strip".into(),     duration_ms: 400,  depends_on: vec!["link".into()] },
        Task { id: "package".into(),   duration_ms: 600,  depends_on: vec!["strip".into()] },
    ];
    let total = tasks.len();

    // --- Setup MultiProgress ---
    let mp = MultiProgress::with_draw_target(ProgressDrawTarget::stderr_with_hz(15));
    mp.set_alignment(MultiProgressAlignment::Top);

    // Header bar (overall progress)
    let header_style = ProgressStyle::with_template(
        "{prefix:.bold.cyan} [{bar:40.green/dim}] {pos}/{len} tasks ({elapsed})"
    ).unwrap().progress_chars("=>-");

    let header = mp.add(ProgressBar::new(total as u64));
    header.set_style(header_style);
    header.set_prefix("DAG");
    header.tick(); // force initial render

    // Separator
    mp.println("").unwrap();

    // Spinner style for running tasks
    let spinner_style = ProgressStyle::with_template(
        "  {spinner:.yellow} {prefix:.bold} {wide_msg}"
    ).unwrap().tick_strings(&[
        ">>>  ",
        " >>> ",
        "  >>>",
        "   >>",
        "    >",
        "     ",
        ">    ",
        ">>   ",
        " done",
    ]);

    // Finished style
    let done_style = ProgressStyle::with_template(
        "  {prefix:.bold.green} {msg:.dim}"
    ).unwrap();

    // --- Task execution state ---
    let completed: Arc<Mutex<HashMap<String, bool>>> = Arc::new(Mutex::new(HashMap::new()));
    let active_bars: Arc<Mutex<HashMap<String, ProgressBar>>> = Arc::new(Mutex::new(HashMap::new()));

    // Initialize all tasks as not completed
    for task in &tasks {
        completed.lock().unwrap().insert(task.id.clone(), false);
    }

    // --- Scheduler loop ---
    let tasks_arc = Arc::new(tasks);
    let mut handles = vec![];

    loop {
        let mut launched_any = false;
        let ready: Vec<Task>;

        {
            let comp = completed.lock().unwrap();
            let active = active_bars.lock().unwrap();

            // Find tasks whose dependencies are all met and that haven't started
            ready = tasks_arc.iter()
                .filter(|t| !comp[&t.id] && !active.contains_key(&t.id))
                .filter(|t| t.depends_on.iter().all(|dep| comp.get(dep).copied().unwrap_or(false)))
                .cloned()
                .collect();
        }

        for task in ready {
            launched_any = true;
            let task_id = task.id.clone();

            // Create and add spinner for this task
            let pb = mp.add(ProgressBar::new_spinner());
            pb.set_style(spinner_style.clone());
            pb.set_prefix(format!("{:<12}", task_id));
            pb.enable_steady_tick(Duration::from_millis(80));
            pb.set_message("running...");

            active_bars.lock().unwrap().insert(task_id.clone(), pb.clone());

            // Spawn worker thread
            let completed = Arc::clone(&completed);
            let active_bars = Arc::clone(&active_bars);
            let header = header.clone();
            let mp = mp.clone();
            let done_style = done_style.clone();

            let handle = thread::spawn(move || {
                let mut rng = rand::rng();
                let steps = 10;
                let step_ms = task.duration_ms / steps;

                for i in 0..steps {
                    thread::sleep(Duration::from_millis(
                        step_ms + rng.random_range(0..step_ms / 2)
                    ));
                    let pct = ((i + 1) as f64 / steps as f64 * 100.0) as u64;
                    pb.set_message(format!("{}%", pct));
                }

                // Mark as done
                pb.set_style(done_style);
                pb.set_message(format!("completed in {}ms", task.duration_ms));
                pb.finish();

                // Log completion above bars
                mp.println(format!(
                    "  {} {} ({}ms)",
                    style("OK").green().bold(),
                    task.id,
                    task.duration_ms
                )).unwrap();

                // Update shared state
                completed.lock().unwrap().insert(task.id.clone(), true);
                active_bars.lock().unwrap().remove(&task.id);
                header.inc(1);
            });

            handles.push(handle);
        }

        // Check if all done
        {
            let comp = completed.lock().unwrap();
            if comp.values().all(|&v| v) {
                break;
            }
        }

        if !launched_any {
            thread::sleep(Duration::from_millis(50));
        }
    }

    // Wait for all threads
    for h in handles {
        let _ = h.join();
    }

    header.finish();
    mp.clear().unwrap();

    println!(
        "\n  {} Built {} tasks in {}",
        style("DONE").green().bold(),
        total,
        HumanDuration(started.elapsed())
    );
}
```

**What this demonstrates:**
- `MultiProgress` with dynamic bar creation/removal
- Header bar tracking overall progress
- Spinner bars for active tasks with `enable_steady_tick()`
- `mp.println()` for logging completions above bars
- Style swapping on completion (spinner -> done)
- DAG dependency scheduling (tasks wait for dependencies)
- Thread-safe state sharing via `Arc<Mutex<>>`
- Custom tick strings for the spinner animation
- Color styling (`.bold.cyan`, `.green`, `.yellow`, `.dim`)
- Rate-limited draw target at 15 fps

**Cargo.toml for this example:**

```toml
[dependencies]
indicatif = "0.18"
console = "0.16"
rand = "0.9"
```

---

## Sources

1. [indicatif docs.rs/0.18.0](https://docs.rs/indicatif/0.18.0/indicatif/) -- Official API documentation
2. [console-rs/indicatif GitHub main](https://github.com/console-rs/indicatif) -- Source code, version 0.18.4
3. `src/style.rs` -- Template engine, all placeholders, ProgressTracker trait
4. `src/progress_bar.rs` -- ProgressBar API, suspend, println, tick
5. `src/multi.rs` -- MultiProgress, add/insert/remove, alignment
6. `src/draw_target.rs` -- DrawTarget, rate limiter, stderr/stdout/hidden
7. `src/state.rs` -- AtomicPosition, BarState, rate limiting in inc()
8. `src/lib.rs` -- Template key documentation (lines 118-193)
9. `examples/multi.rs` -- MultiProgress with insert_after, println
10. `examples/multi-tree.rs` -- Dynamic bar insertion with tree structure
11. `examples/multi-tree-ext.rs` -- Dynamic add/remove, bottom alignment
12. `examples/yarnish.rs` -- Yarn-style build output, spinner styles
13. `examples/cargo.rs` -- Cargo-style compilation progress
14. `examples/finebars.rs` -- Fine-grained progress_chars, color per bar
15. `examples/download.rs` -- Custom eta key via with_key()
16. `examples/download-speed.rs` -- bytes_per_sec placeholder
17. `examples/long-spinner.rs` -- tick_strings with Unicode, steady tick
18. `examples/fastbar.rs` -- Performance benchmark (1M iterations)
19. `examples/cargowrap.rs` -- ASCII spinner, steady tick, wrapping real process output
20. `examples/log.rs` -- ProgressBar::println pattern
21. [sindresorhus/cli-spinners](https://github.com/sindresorhus/cli-spinners/blob/master/spinners.json) -- 80+ spinner designs

## Methodology

- **Tools used:** Direct HTTP fetch of source files from GitHub raw content, docs.rs HTML
- **Files analyzed:** 20+ source and example files from the indicatif repository
- **Version:** 0.18.4 (latest on main branch as of 2026-03-26)
- **Approach:** Source code reading over documentation, since the source is the authoritative reference for template keys, rate limiter behavior, and draw target internals

## Confidence Level

**High** -- All findings are drawn directly from the source code of indicatif v0.18.4 on the main branch. Template placeholders were extracted from the `format_state()` match arms in `src/style.rs`. API signatures were taken from the actual struct implementations. Examples were fetched verbatim from the repository. The DAG runner example in section 10 synthesizes patterns from multiple official examples.

## Quick Reference Card

```
TEMPLATE FORMAT:  {key:<alignment><width><!truncate><.style></alt_style>}
STYLE SYNTAX:     bold.cyan.on_blue   (dot-separated, from console crate)
PROGRESS CHARS:   .progress_chars("=>-")   filled, current, empty
TICK CHARS:       .tick_chars("/|\\- ")    last char = finished state
TICK STRINGS:     .tick_strings(&["a","b","done"])   last = finished
DEFAULT FPS:      ProgressBar=20, MultiProgress=15
DEFAULT TARGET:   stderr (hidden if not a TTY)
WIDE ELEMENTS:    Only ONE of {wide_bar} or {wide_msg} per template
CUSTOM KEYS:      .with_key("name", |state, w| write!(w, "..."))
```
