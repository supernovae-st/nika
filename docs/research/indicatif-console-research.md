# Research Report: indicatif, console, and comfy-table Crates

## Summary

`indicatif` (v0.18.4) is the de facto standard for progress bars and spinners in Rust CLIs. It is built on top of the `console` crate (v0.16.3) for terminal abstraction, styling, and ANSI handling. `comfy-table` (v7.2.2) handles structured table output. All three are mature, actively maintained, and production-ready. This report covers their APIs in depth, including advanced patterns like tree-like multi-progress displays, custom trackers, and terminal integration.

---

## 1. MultiProgress -- Parallel Task Display

### Core API

```rust
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

let m = MultiProgress::new();

// Add bars (appended at bottom)
let pb1 = m.add(ProgressBar::new(100));
let pb2 = m.add(ProgressBar::new(200));

// Insert at specific positions
let pb3 = m.insert(0, ProgressBar::new(50));           // at index 0
let pb4 = m.insert_after(&pb1, ProgressBar::new(75));   // after pb1
let pb5 = m.insert_before(&pb2, ProgressBar::new(75));  // before pb2
let pb6 = m.insert_from_back(1, ProgressBar::new(30));  // 1 from end

// Remove a bar
m.remove(&pb3);

// Print a log line above all bars
m.println("Starting download...").unwrap();

// Temporarily hide all bars, run code, redraw
m.suspend(|| {
    println!("Some output that would conflict with bars");
});

// Clear all bars
m.clear().unwrap();
```

### Key Properties

- `MultiProgress` is `Send + Sync + Clone` -- safe to share across threads via `Arc`
- Adding a bar changes its draw target to the `MultiProgress` (overrides any custom target)
- Default draw target: stderr at 15 fps (yes, MultiProgress defaults to 15, not 20)
- Adding a bar that is already a member is a no-op
- Vertical alignment: `MultiProgressAlignment::Top` (default) or `Bottom`
  - `Top`: when a bar is removed, bars below shift up
  - `Bottom`: when a bar is removed, bars above shift down

### Thread Pattern (from `multi.rs` example)

```rust
let m = MultiProgress::new();
let sty = ProgressStyle::with_template(
    "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}"
).unwrap().progress_chars("##-");

let mut handles = vec![];
for i in 0..4 {
    let pb = m.add(ProgressBar::new(100));
    pb.set_style(sty.clone());
    handles.push(thread::spawn(move || {
        for j in 0..100 {
            pb.inc(1);
            thread::sleep(Duration::from_millis(20));
        }
        pb.finish_with_message("done");
    }));
}
for h in handles { h.join().unwrap(); }
m.clear().unwrap();
```

### Zombie Bars

When a bar finishes and is the first in the visual order, indicatif "reaps" it -- the bar's final state stays on screen but is no longer managed. This prevents finished bars from being cleared when the MultiProgress redraws. Internal `is_zombie` tracking handles deferred reaping for non-first bars.

---

## 2. ProgressBar with Spinner

### Creation

```rust
// Bounded progress bar
let pb = ProgressBar::new(1000);

// Unbounded spinner
let pb = ProgressBar::new_spinner();

// No length (can set later)
let pb = ProgressBar::no_length();

// Hidden (responds to API but draws nothing)
let pb = ProgressBar::hidden();
```

### Builder Pattern

```rust
let pb = ProgressBar::new(100)
    .with_style(my_style)
    .with_message("downloading")
    .with_prefix("[1/4]")
    .with_position(50)
    .with_elapsed(Duration::from_secs(30))
    .with_finish(ProgressFinish::AndLeave)
    .with_tab_width(4);
```

### Spinner Auto-Tick

```rust
// Tick every 120ms in a background thread
pb.enable_steady_tick(Duration::from_millis(120));

// Stop auto-ticking
pb.disable_steady_tick();
```

The background `Ticker` thread:
- Uses a `Condvar` for clean shutdown (no busy-waiting)
- Stops when the bar is finished or dropped
- Holds a `Weak` reference to avoid preventing bar drop

### Custom Tick Strings

```rust
// Single characters (last = final state)
pb.set_style(ProgressStyle::with_template("{spinner:.blue} {msg}")
    .unwrap()
    .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ "));

// Multi-character strings (last = final state)
pb.set_style(ProgressStyle::with_template("{spinner:.green} {msg}")
    .unwrap()
    .tick_strings(&[
        "▹▹▹▹▹",
        "▸▹▹▹▹",
        "▹▸▹▹▹",
        "▹▹▸▹▹",
        "▹▹▹▸▹",
        "▹▹▹▹▸",
        "▪▪▪▪▪",  // final state
    ]));
```

The default tick sequence: `⠁⠁⠉⠙⠚⠒⠂⠂⠒⠲⠴⠤⠄⠄⠤⠠⠠⠤⠦⠖⠒⠐⠐⠒⠓⠋⠉⠈⠈ ` (braille dots).

Reference for hundreds of spinner styles: [cli-spinners](https://github.com/sindresorhus/cli-spinners/blob/master/spinners.json).

### Progress Chars (Bar Fill Styles)

```rust
// 2 chars: filled + empty
.progress_chars("#-")

// 3 chars: filled + current + empty
.progress_chars("#>-")

// Fine-grained with Unicode block elements
.progress_chars("█▉▊▋▌▍▎▏  ")  // smooth fill
.progress_chars("█▇▆▅▄▃▂▁  ")  // vertical blocks
.progress_chars("█▓▒░  ")       // fade effect
.progress_chars("█▛▌▖  ")       // blocky
```

All grapheme clusters must be equal width. More chars = smoother progress animation.

### Finish Behaviors

```rust
pb.finish();                              // Fill to 100%, leave on screen
pb.finish_with_message("done");           // Fill + set message
pb.finish_and_clear();                    // Fill + remove from screen
pb.abandon();                             // Leave at current pos, keep on screen
pb.abandon_with_message("cancelled");     // Leave at current pos + message
pb.finish_using_style();                  // Use behavior from with_finish()
```

`ProgressFinish` enum:
- `AndLeave` (default) -- sets pos to len, leaves visible
- `WithMessage(Cow<'static, str>)` -- sets pos to len + message
- `AndClear` -- sets pos to len, hides
- `Abandon` -- keeps current pos, stays visible
- `AbandonWithMessage(Cow<'static, str>)` -- keeps current pos + message

### Iterators

```rust
use indicatif::ProgressIterator;

// Simple
for item in (0..1000).progress() {
    // ...
}

// With bar
let pb = ProgressBar::new(items.len() as u64);
for item in pb.wrap_iter(items.iter()) {
    // ...
}

// Rayon parallel (feature = "rayon")
use indicatif::ParallelProgressIterator;
v.par_iter().progress_count(v.len() as u64).map(|i| i + 1).collect();

// IO wrapping
io::copy(&mut pb.wrap_read(source), &mut target);
io::copy(&mut source, &mut pb.wrap_write(target));

// Async (feature = "tokio")
io::copy(&mut pb.wrap_async_read(source), &mut target).await;

// Streams (feature = "futures")
let stream = pb.wrap_stream(my_stream);
```

### Querying State

```rust
pb.position();           // current position (u64)
pb.length();             // total length (Option<u64>)
pb.eta();                // estimated remaining time (Duration)
pb.per_sec();            // speed in steps/sec (f64)
pb.duration();           // estimated total duration (Duration)
pb.elapsed();            // elapsed time (Duration)
pb.is_finished();        // bool
pb.is_hidden();          // bool
pb.message();            // String
pb.prefix();             // String
```

---

## 3. ProgressStyle -- Template System

### Template Syntax

```
{key}                    plain placeholder
{key:WIDTH}              fixed width
{key:>WIDTH}             right-aligned, fixed width
{key:^WIDTH}             center-aligned
{key:<WIDTH}             left-aligned (default)
{key:WIDTH!}             truncate if exceeds width
{key:WIDTH.STYLE}        with color style
{key:WIDTH.STYLE/ALT}    with primary and alternate style
```

Style strings use `console::Style::from_dotted_str()` -- see section 4 below.

### All Template Keys

| Key | Description | Example Output |
|-----|-------------|----------------|
| `bar` | Fixed-width progress bar (default 20 chars) | `████████░░░░░░░░░░░░` |
| `wide_bar` | Bar that fills remaining terminal width | (fills space) |
| `spinner` | Current tick string | `⠋` |
| `prefix` | Prefix text (set via `set_prefix`) | `[1/4]` |
| `msg` | Message text (set via `set_message`) | `downloading...` |
| `wide_msg` | Message that fills remaining width (truncated) | (fills space) |
| `pos` | Current position (integer) | `42` |
| `human_pos` | Position with thousands separator | `33,857` |
| `len` | Total length (integer) | `100` |
| `human_len` | Length with thousands separator | `1,000,000` |
| `percent` | Percentage (integer) | `42` |
| `percent_precise` | Percentage (3 decimal places) | `42.857` |
| `bytes` / `binary_bytes` | Position in bytes (power-of-2: KiB, MiB) | `3.00 MiB` |
| `total_bytes` / `binary_total_bytes` | Length in bytes (power-of-2) | `10.00 GiB` |
| `decimal_bytes` | Position in bytes (SI: kB, MB) | `3.15 MB` |
| `decimal_total_bytes` | Length in bytes (SI) | `10.74 GB` |
| `elapsed` | Elapsed time (human) | `42s`, `1m` |
| `elapsed_precise` | Elapsed time (HH:MM:SS) | `00:01:23` |
| `eta` | ETA (human) | `2m` |
| `eta_precise` | ETA (HH:MM:SS) | `00:02:15` |
| `duration` | Extrapolated total duration (human) | `3m` |
| `duration_precise` | Extrapolated total duration (HH:MM:SS) | `00:03:38` |
| `per_sec` | Speed (steps/sec) | `1,234/s` |
| `bytes_per_sec` / `binary_bytes_per_sec` | Speed in bytes/s (power-of-2) | `2.50 MiB/s` |
| `decimal_bytes_per_sec` | Speed in bytes/s (SI) | `2.62 MB/s` |

**Important**: `wide_bar` and `wide_msg` are mutually exclusive -- do not use both in the same template.

### Real-World Template Examples

```rust
// Download progress
"{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({eta})"

// Yarn-like build
"{prefix:.bold.dim} {spinner} {wide_msg}"

// Simple with percentage
"{bar:40.green/yellow} {pos:>4}/{len:4}"

// Tree node
"[{pos:>2}/{len:2}] {prefix}{spinner:.green} {msg}"

// Fancy with custom ETA
"{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({eta})"
```

### Custom Keys via ProgressTracker

Simple closure-based:

```rust
use indicatif::{ProgressState, ProgressStyle};
use std::fmt::Write;

let style = ProgressStyle::with_template("{spinner} [{eta}] {bar:40} {pos}/{len}")
    .unwrap()
    .with_key("eta", |state: &ProgressState, w: &mut dyn Write| {
        write!(w, "{:.1}s", state.eta().as_secs_f64()).unwrap()
    });
```

Full stateful tracker (implements `ProgressTracker` trait):

```rust
pub trait ProgressTracker: Send + Sync {
    fn clone_box(&self) -> Box<dyn ProgressTracker>;
    fn tick(&mut self, state: &ProgressState, now: Instant);
    fn reset(&mut self, state: &ProgressState, now: Instant);
    fn write(&self, state: &ProgressState, w: &mut dyn fmt::Write);
}
```

Closures `Fn(&ProgressState, &mut dyn Write)` auto-implement `ProgressTracker` with no-op `tick`/`reset`.

### Human Formatting Utilities

```rust
use indicatif::{HumanBytes, HumanCount, HumanDuration, HumanFloatCount};

HumanBytes(3 * 1024 * 1024).to_string()     // "3.00 MiB"
HumanDuration(Duration::from_secs(8))        // "8 seconds"
HumanCount(33_857_009)                       // "33,857,009"
HumanFloatCount(33_857_009.1235)             // "33,857,009.1235"
```

---

## 4. Console Integration

indicatif is built directly on top of the `console` crate. Every style string in templates uses `console::Style::from_dotted_str()`.

### Style Dotted String Syntax (used in templates)

The style part in `{bar:40.cyan/blue}` uses this format. Multiple attributes are dot-separated:

| Term | Effect |
|------|--------|
| `black`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan`, `white` | Foreground color |
| `bright` | Bright/bold foreground |
| `on_black`, `on_red`, `on_green`, etc. | Background color |
| `on_bright` | Bright background |
| `bold` | Bold text |
| `dim` | Dim/faint text |
| `underlined` | Underline |
| `blink`, `blink_fast` | Blinking |
| `reverse` | Reverse video |
| `hidden` | Hidden text |
| `strikethrough` | Strikethrough |
| `0`-`255` | 256-color foreground |
| `on_0`-`on_255` | 256-color background |
| `#RRGGBB` | True color foreground (e.g., `#ff5733`) |
| `on_#RRGGBB` | True color background |

**Examples**:
- `cyan` -- cyan text
- `bold.cyan` -- bold cyan text
- `red.on_white` -- red text on white background
- `bold.dim` -- bold + dim (faint bold)
- `#ff5733` -- true color orange
- `bold.#00ff00.on_#000000` -- bold green on black (true colors)

### Console Programmatic Styling

```rust
use console::{style, Style, Emoji};

// Inline styling
println!("This is {} neat", style("quite").cyan());
println!("{}", style("Error!").red().bold());
println!("{}", style("OK").green().on_black());

// Reusable styles
let heading = Style::new().bold().underlined();
let error = Style::new().red().bold();
let dim = Style::new().dim();

println!("{}", heading.apply_to("Section Title"));
println!("{}", error.apply_to("Something failed"));
println!("{}", dim.apply_to("(optional detail)"));

// From dotted string (same as template syntax)
let s = Style::from_dotted_str("bold.cyan.on_black");
println!("{}", s.apply_to("styled text"));

// Stderr-aware styling
let s = Style::new().for_stderr().red();
```

### Emoji Support

```rust
use console::Emoji;

static LOOKING_GLASS: Emoji<'_, '_> = Emoji("🔍  ", "");
static TRUCK: Emoji<'_, '_> = Emoji("🚚  ", "");
static SPARKLE: Emoji<'_, '_> = Emoji("✨ ", ":-)");

println!("[1/4] {}Resolving...", LOOKING_GLASS);
println!("Done! {}", SPARKLE);
```

`Emoji` automatically detects if the terminal supports emoji (`wants_emoji()`). If not, the fallback string is used. Detection is based on `is_attended()` + platform heuristics (macOS = yes, most Linux terminals = yes, Windows cmd = no by default).

### Terminal Width Detection

```rust
use console::Term;

let term = Term::stderr();

// Returns (rows, cols), defaults to (24, 80) if unavailable
let (rows, cols) = term.size();

// Returns None if size cannot be determined
let size = term.size_checked();

// Terminal feature detection
let features = term.features();
features.is_attended();          // true if TTY (isatty)
features.colors_supported();     // true if color terminal
features.true_colors_supported();// true if 24-bit color
features.wants_emoji();          // true if emoji safe
features.family();               // UnixTerm, WindowsConsole, File, Dummy
```

### Text Measurement and Truncation

```rust
use console::{measure_text_width, truncate_str, pad_str, Alignment};

// Width of styled text (strips ANSI codes for measurement)
let width = measure_text_width("\x1b[31mhello\x1b[0m"); // 5

// Truncate with tail
let s = truncate_str("Hello World", 7, "..."); // "Hell..."

// Pad to width
let s = pad_str("hi", 10, Alignment::Center, None); // "    hi    "
let s = pad_str("hi", 10, Alignment::Right, Some(".")); // "........hi"
```

### ANSI Code Handling

```rust
use console::{strip_ansi_codes, AnsiCodeIterator};

// Remove all ANSI codes
let clean = strip_ansi_codes("\x1b[31mred\x1b[0m"); // "red"

// Iterate over text and ANSI segments
for (text, is_ansi) in AnsiCodeIterator::new(styled_text) {
    if is_ansi { /* escape sequence */ }
    else { /* visible text */ }
}
```

### Color Control

```rust
use console::{colors_enabled, set_colors_enabled, colors_enabled_stderr};

// Check if colors enabled (respects CLICOLOR, CLICOLOR_FORCE, NO_COLOR)
if colors_enabled() { /* ... */ }

// Override color detection
set_colors_enabled(false);
```

---

## 5. Live Updating -- Position-Fixed Output

### Single ProgressBar

A `ProgressBar` draws to a single terminal line. When you call `set_message()`, `set_position()`, `inc()`, or `tick()`, it redraws in place. The default refresh rate is 20 fps -- the internal `RateLimiter` suppresses intermediate draws.

```rust
let pb = ProgressBar::new(100);
pb.set_message("Starting...");
for i in 0..100 {
    pb.set_message(format!("Processing item {}", i));
    pb.inc(1);
}
pb.finish_with_message("All done");
```

### Force Redraw

```rust
pb.force_draw(); // bypass rate limiter for immediate redraw
```

### Refresh Rate Control

```rust
use indicatif::ProgressDrawTarget;

// Default: 20 fps
let pb = ProgressBar::new(100);

// Custom refresh rate
pb.set_draw_target(ProgressDrawTarget::stderr_with_hz(10)); // 10 fps
pb.set_draw_target(ProgressDrawTarget::stdout_with_hz(30)); // 30 fps
```

### Printing Above a Progress Bar

```rust
// Single bar
pb.println("Log: something happened");

// MultiProgress
m.println("Log: something happened").unwrap();
```

`println` inserts a line above the progress bar(s) without disrupting the layout. The bars move down to make room.

### Suspend Pattern

```rust
// Temporarily hide bars, run code, redraw
pb.suspend(|| {
    println!("This output won't conflict with the progress bar");
    do_something();
});
```

**Warning**: The internal lock is held during `suspend`. Other threads trying to update the bar will block.

---

## 6. Nesting -- Tree-Like Displays

indicatif does NOT have a built-in tree/nesting API. However, tree-like displays are achieved using `MultiProgress` with indented prefixes and dynamic insertion.

### Pattern: Indented Tree

```rust
let mp = MultiProgress::new();
let sty_main = ProgressStyle::with_template("{bar:40.green/yellow} {pos:>4}/{len:4}").unwrap();
let sty_node = ProgressStyle::with_template(
    "[{pos:>2}/{len:2}] {prefix}{spinner:.green} {msg}"
).unwrap();
let sty_done = ProgressStyle::with_template(
    "[{pos:>2}/{len:2}] {prefix}{msg}"
).unwrap();

// Main overall bar
let pb_main = mp.add(ProgressBar::new(total));
pb_main.set_style(sty_main);

// Add child nodes with indentation via prefix
let child = mp.insert(1, ProgressBar::new(32));
child.set_style(sty_node.clone());
child.set_prefix("  ".repeat(indent_level));
child.set_message("downloading foo");

// On completion, swap style to remove spinner
child.set_style(sty_done.clone());
child.finish_with_message(format!("{} {}", style("✔").green(), "foo"));
```

### Dynamic Tree (from `multi-tree.rs` example)

The official `multi-tree.rs` example shows a pattern where:
1. A main progress bar tracks overall progress
2. Child bars are dynamically added with `mp.insert(index, pb)` at specific positions
3. Children use `"  ".repeat(indent)` as visual indentation
4. On completion, a checkmark replaces the spinner

The extended `multi-tree-ext.rs` demonstrates:
- Adding AND removing bars dynamically
- Bottom alignment (`--bottom-alignment` flag)
- Different styles for in-progress vs. completed items

### Limitations of Tree Display

- No automatic tree-line drawing (you must use prefix strings like `├──`, `└──`, `│` manually)
- No collapsible groups
- Inserting/removing bars while other threads are updating can cause brief visual glitches
- `set_move_cursor(true)` reduces flicker but should NOT be used if you intend to add/remove bars dynamically

---

## 7. Complex Layout Examples

### Example: Yarn-like Build Output

From `yarnish.rs` -- a 4-phase build with final timing:

```rust
use console::{style, Emoji};
use indicatif::{HumanDuration, MultiProgress, ProgressBar, ProgressStyle};

static LOOKING_GLASS: Emoji<'_, '_> = Emoji("🔍  ", "");
static TRUCK: Emoji<'_, '_> = Emoji("🚚  ", "");
static CLIP: Emoji<'_, '_> = Emoji("🔗  ", "");
static PAPER: Emoji<'_, '_> = Emoji("📃  ", "");
static SPARKLE: Emoji<'_, '_> = Emoji("✨ ", ":-)");

let started = Instant::now();
let spinner_style = ProgressStyle::with_template("{prefix:.bold.dim} {spinner} {wide_msg}")
    .unwrap()
    .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ");

// Phase 1-2: Static headers
println!("{} {}Resolving packages...", style("[1/4]").bold().dim(), LOOKING_GLASS);
println!("{} {}Fetching packages...", style("[2/4]").bold().dim(), TRUCK);

// Phase 3: Simple progress bar
println!("{} {}Linking dependencies...", style("[3/4]").bold().dim(), CLIP);
let pb = ProgressBar::new(1232);
for _ in 0..1232 { pb.inc(1); }
pb.finish_and_clear();

// Phase 4: Parallel spinners
println!("{} {}Building fresh packages...", style("[4/4]").bold().dim(), PAPER);
let m = MultiProgress::new();
for i in 0..4 {
    let pb = m.add(ProgressBar::new(count));
    pb.set_style(spinner_style.clone());
    pb.set_prefix(format!("[{}/?]", i + 1));
    thread::spawn(move || { /* work */ });
}

println!("{} Done in {}", SPARKLE, HumanDuration(started.elapsed()));
```

### Example: Download with Custom ETA

```rust
let pb = ProgressBar::new(total_size);
pb.set_style(
    ProgressStyle::with_template(
        "{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({eta})"
    ).unwrap()
    .with_key("eta", |state: &ProgressState, w: &mut dyn Write| {
        write!(w, "{:.1}s", state.eta().as_secs_f64()).unwrap()
    })
    .progress_chars("#>-")
);
```

### Example: Multiple Fine-Grained Bar Styles

```rust
let styles = [
    ("Rough bar:", "█  ", "red"),
    ("Fine bar: ", "█▉▊▋▌▍▎▏  ", "yellow"),
    ("Vertical: ", "█▇▆▅▄▃▂▁  ", "green"),
    ("Fade in:  ", "█▓▒░  ", "blue"),
    ("Blocky:   ", "█▛▌▖  ", "magenta"),
];

let m = MultiProgress::new();
for (label, chars, color) in &styles {
    let pb = m.add(ProgressBar::new(512));
    pb.set_style(
        ProgressStyle::with_template(&format!("{{prefix:.bold}}▕{{bar:.{}}}▏{{msg}}", color))
            .unwrap()
            .progress_chars(chars),
    );
    pb.set_prefix(*label);
}
```

---

## 8. Limitations

### What indicatif CANNOT do

| Limitation | Detail | Alternative |
|-----------|--------|-------------|
| Full TUI | No layout engine, no widgets, no input handling | Use `ratatui` |
| Scrolling content | Cannot scroll within a progress region | Use `ratatui` |
| Side-by-side bars | Only vertical stacking of bars | Custom `TermLike` impl |
| Rich text in messages | Messages are plain text (ANSI codes work but no markup) | Pre-style with `console::style()` |
| Nested groups | No collapsible/expandable tree nodes | Manual prefix-based indentation |
| Interactive | No user input during progress (no key handling) | Use `dialoguer` + `suspend()` |
| Custom drawing | Cannot mix free-form rendering with progress bars easily | `TermLike` trait |
| Multiple columns | Cannot show bars in a grid/table layout | Custom rendering |
| Multiline bar templates | Limited -- `\n` in template works but each line is tracked separately | Keep templates single-line |

### Performance Considerations

- Default 20 fps rate limiting prevents excessive redraws
- `ProgressBar` uses `AtomicU64` for position tracking (lock-free `inc()`)
- The `Mutex<BarState>` lock is only acquired for drawing, not for incrementing
- `MultiProgress` holds a `RwLock` -- concurrent reads are fast, writes serialize
- For very high throughput (>100k inc/s), the rate limiter in `AtomicPosition` throttles draw calls

### When to use something else

- **ratatui**: When you need a full TUI with multiple widgets, input handling, scrolling
- **linya**: Lighter alternative if you only need basic multi-line progress (unmaintained)
- **kdam**: Python tqdm-like alternative with more display options
- **pbr**: Simpler alternative, fewer features
- **Custom TermLike**: When you need to integrate progress bars into a custom rendering pipeline

---

## 9. Recent Versions (2024-2026)

### indicatif 0.18.4 (2026-02-14) -- LATEST

- `NO_COLOR` and `TERM=dumb` now correctly hide progress bars
- WASM support made optional (`wasmbind` feature)
- Exposed `tab_width()` getter
- Fixed duration display after `finish()`
- Seeking heuristic improvements for ETA

### indicatif 0.18.0-0.18.3 (2025-07-04 to 2025-11-11)

- **0.18.0**: Semver bump due to console 0.16 upgrade (API compatible with 0.17.x)
- **0.18.1**: Fixed `wide_bar` width with multiline messages, skip drawing current char if none configured
- **0.18.2**: Fixed `wide_msg` truncation with colored messages
- **0.18.3**: Added `ProgressBar::set_elapsed()`

### indicatif 0.17.8-0.17.11 (2024-2025)

- **0.17.10**: Major performance improvements (lazy tab expansion, deferred width queries), added `dec()` and `dec_length()`
- **0.17.9**: Fixed `move_cursor` flag, `AtomicPosition::reset`, `percent_precise` key, `ProgressBar::no_length()`
- **0.17.8**: `VisualLines` newtype, decimal bytes per sec, fixed `per_sec` after finish

### console 0.16.3 (2026-03-13) -- LATEST

- Dropped `once_cell` dependency (uses `std::sync::OnceLock`)

### console 0.16.0-0.16.2 (2025-2026)

- **0.16.0**: `std` feature introduced (semver bump from 0.15.x)
- **0.16.2**: True color support (`#RRGGBB` and `on_#RRGGBB` in dotted strings), `NO_COLOR` on Windows

### comfy-table 7.2.2 -- LATEST

- Considered "finished" by maintainer, in maintenance mode
- Searching for a new maintainer

### Feature Flags Summary

**indicatif**:
- `default` = `unicode-width` + `wasmbind`
- `rayon` -- parallel iterator support
- `tokio` -- async read/write wrapping
- `futures` -- stream wrapping
- `improved_unicode` -- grapheme segmentation + width
- `in_memory` -- `InMemoryTerm` for testing (vt100 dep)

**console**:
- `default` = `unicode-width` + `ansi-parsing` + `std`
- `unicode-width` -- correct character width calculation
- `ansi-parsing` -- ANSI code stripping/measurement

**comfy-table**:
- `default` = `tty` (crossterm for terminal width)
- `custom_styling` -- ANSI styling in cells (adds `console` + `ansi-str` deps)
- `reexport_crossterm` -- re-export crossterm types

---

## 10. comfy-table for Structured Output

### Basic Usage

```rust
use comfy_table::Table;

let mut table = Table::new();
table
    .set_header(vec!["Task", "Status", "Duration"])
    .add_row(vec!["Build", "OK", "2.3s"])
    .add_row(vec!["Test", "FAIL", "45.1s"]);

println!("{table}");
```

### UTF-8 Styled Tables

```rust
use comfy_table::presets::UTF8_FULL;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::*;

let mut table = Table::new();
table
    .load_preset(UTF8_FULL)
    .apply_modifier(UTF8_ROUND_CORNERS)
    .set_content_arrangement(ContentArrangement::Dynamic)
    .set_width(80)
    .set_header(vec![
        Cell::new("Provider").add_attribute(Attribute::Bold),
        Cell::new("Model").fg(Color::Cyan),
        Cell::new("Cost/1K").set_alignment(CellAlignment::Right),
    ])
    .add_row(vec!["Anthropic", "claude-sonnet-4", "$0.003"])
    .add_row(vec!["OpenAI", "gpt-4o", "$0.005"]);
```

Output:
```
╭───────────┬─────────────────┬─────────╮
│ Provider  ┆ Model           ┆ Cost/1K │
╞═══════════╪═════════════════╪═════════╡
│ Anthropic ┆ claude-sonnet-4 ┆  $0.003 │
├╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌┤
│ OpenAI    ┆ gpt-4o          ┆  $0.005 │
╰───────────┴─────────────────┴─────────╯
```

### Key Features

- `ContentArrangement::Dynamic` -- auto-wraps content to fit terminal width
- `ContentArrangement::Disabled` -- table grows to fit content
- Auto-detects terminal width when `tty` feature is enabled
- Multi-line cell content (newlines in strings)
- Per-cell alignment: `CellAlignment::Left`, `Center`, `Right`
- Per-column alignment via `table.column_mut(n).set_cell_alignment()`
- Cell styling: `.fg(Color)`, `.bg(Color)`, `.add_attribute(Attribute)` (requires `custom_styling` feature)
- Built-in presets: `UTF8_FULL`, `UTF8_FULL_CONDENSED`, `UTF8_BORDERS_ONLY`, `UTF8_NO_BORDERS`, `NOTHING`, `ASCII_FULL`, `ASCII_BORDERS_ONLY`, etc.
- Modifiers: `UTF8_ROUND_CORNERS`, `UTF8_SOLID_INNER_BORDERS`
- Column constraints: `ColumnConstraint::ContentWidth`, `Absolute(n)`, `Percentage(n)`, etc.
- ~30us to build a simple table, ~470us for complex ones

---

## 11. TermLike Trait -- Custom Rendering Targets

For cases where you need to integrate indicatif with a custom rendering pipeline:

```rust
use indicatif::TermLike;

pub trait TermLike: Debug + Send + Sync {
    fn width(&self) -> u16;
    fn height(&self) -> u16;              // default: 20
    fn move_cursor_up(&self, n: usize) -> io::Result<()>;
    fn move_cursor_down(&self, n: usize) -> io::Result<()>;
    fn move_cursor_right(&self, n: usize) -> io::Result<()>;
    fn move_cursor_left(&self, n: usize) -> io::Result<()>;
    fn write_line(&self, s: &str) -> io::Result<()>;
    fn write_str(&self, s: &str) -> io::Result<()>;
    fn clear_line(&self) -> io::Result<()>;
    fn flush(&self) -> io::Result<()>;
}
```

Use with `ProgressDrawTarget::term_like(Box::new(my_impl))` or `term_like_with_hz(impl, 30)`.

The `InMemoryTerm` (feature `in_memory`, backed by `vt100`) implements `TermLike` for testing.

---

## 12. Integration Crates

| Crate | Purpose |
|-------|---------|
| `indicatif-log-bridge` | Prevents `log` crate output from conflicting with progress bars |
| `tracing-indicatif` | Auto-creates progress bars for active `tracing` spans |
| `dialoguer` | User prompts/input (same `console-rs` family) |

---

## Sources

1. [indicatif crate](https://crates.io/crates/indicatif) -- v0.18.4, crates.io
2. [indicatif source](https://github.com/console-rs/indicatif) -- full source code review
3. [console crate](https://crates.io/crates/console) -- v0.16.3, crates.io
4. [console source](https://github.com/console-rs/console) -- full source code review
5. [comfy-table crate](https://crates.io/crates/comfy-table) -- v7.2.2, crates.io
6. [comfy-table source](https://github.com/Nukesor/comfy-table) -- README + Cargo.toml review
7. [indicatif GitHub releases](https://github.com/console-rs/indicatif/releases) -- changelog 0.17.8 through 0.18.4
8. [console GitHub releases](https://github.com/console-rs/console/releases) -- changelog 0.15.12 through 0.16.3
9. [cli-spinners](https://github.com/sindresorhus/cli-spinners) -- referenced spinner database

## Methodology

- Tools used: GitHub raw content API, GitHub releases API, crates.io search
- Files analyzed: 15+ source files (lib.rs, multi.rs, style.rs, progress_bar.rs, draw_target.rs, state.rs, term_like.rs, term.rs, utils.rs, Cargo.toml) + 8 examples
- All code examples verified against source (not generated from memory)

## Confidence Level

**High** -- This is based on direct source code reading of the latest versions of all three crates, cross-referenced with official release notes and examples. All API details were verified against the actual implementations.
