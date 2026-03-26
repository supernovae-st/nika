# Research Report: Streaming Token Display in AI/LLM CLI Tools

## Summary

This report analyzes how leading AI CLI tools (aichat, mods, and others) display streaming
LLM output in terminals. It covers markdown rendering during streaming, thinking indicators,
token counting, cost display, and UX state machines. The focus is on Rust implementations
and patterns applicable to ratatui-based TUIs.

## Key Findings

---

### 1. aichat (sigoden/aichat) — Streaming Markdown Architecture

aichat is the gold standard for streaming markdown in a Rust CLI. Its architecture has three
key layers:

**Layer 1: Event Gathering with Batching (50ms window)**

The critical insight is `gather_events()` — aichat does NOT render every single SSE token
individually. It collects tokens over a 50ms window and merges them into a single text chunk:

```rust
async fn gather_events(rx: &mut UnboundedReceiver<SseEvent>) -> Vec<SseEvent> {
    let mut texts = vec![];
    let mut done = false;
    tokio::select! {
        _ = async {
            while let Some(reply_event) = rx.recv().await {
                match reply_event {
                    SseEvent::Text(v) => texts.push(v),
                    SseEvent::Done => { done = true; break; }
                }
            }
        } => {}
        _ = tokio::time::sleep(Duration::from_millis(50)) => {}
    };
    let mut events = vec![];
    if !texts.is_empty() {
        events.push(SseEvent::Text(texts.join("")))
    }
    if done { events.push(SseEvent::Done) }
    events
}
```

This prevents flicker and reduces re-render overhead from potentially hundreds of single-char
renders to ~20 batched renders per second.

**Layer 2: Incremental Line Rendering**

aichat maintains a `buffer` of the current incomplete line. When newlines arrive:
- Complete lines go through full `render()` (multi-line markdown)
- The trailing incomplete line goes through `render_line()` (single-line, no state mutation)

This split is critical: `render()` updates the LineType state machine (Normal/CodeBegin/
CodeInner/CodeEnd) while `render_line()` is a read-only speculative render. This prevents
corrupting the code-block detection state on partial lines.

**Layer 3: Terminal Cursor Dance**

The raw terminal output uses crossterm's queue mechanism with precise cursor positioning:

```rust
// Move cursor back to start of buffer
queue!(writer, cursor::MoveTo(0, row + 1 - buffer_rows))?;
// Clear from cursor down (prevents ghosting)
queue!(writer, terminal::Clear(terminal::ClearType::FromCursorDown))?;
// Print new content
print_block(writer, &output, columns)?;
```

The `need_rows()` function handles line wrapping math, accounting for wide characters
via `textwrap::core::display_width`.

**Key Dependencies:**
- `syntect` 5.0 (parsing + regex-onig) — Syntax highlighting with bat's syntax pack
- `crossterm` 0.28 — Terminal control, raw mode, cursor positioning
- `textwrap` 0.16 — Word wrapping respecting terminal width
- `ansi_colours` — True color to ANSI256 fallback conversion

**Source:** `/tmp/aichat-research/src/render/`

---

### 2. mods (charmbracelet/mods) — Glamour + Viewport Pattern

mods takes a fundamentally different approach: accumulate-then-render via Glamour (Go markdown
renderer) with a viewport for scrolling.

**The Accumulate Pattern:**

Every streaming chunk is appended to `m.Output`, then the *entire output* is re-rendered
through Glamour:

```go
func (m *Mods) appendToOutput(s string) {
    m.Output += s
    // Re-render ALL accumulated output through Glamour
    m.glamOutput, _ = m.glam.Render(m.Output)
    m.glamOutput = strings.TrimRightFunc(m.glamOutput, unicode.IsSpace)
    m.glamOutput = strings.ReplaceAll(m.glamOutput, "\t", strings.Repeat(" ", tabWidth))
    m.glamHeight = lipgloss.Height(m.glamOutput)
    // Update viewport
    truncatedGlamOutput := m.renderer.NewStyle().MaxWidth(m.width).Render(m.glamOutput)
    m.glamViewport.SetContent(truncatedGlamOutput)
    // Auto-scroll to bottom if user was at bottom
    if oldHeight < m.glamHeight && wasAtBottom {
        m.glamViewport.GotoBottom()
    }
}
```

**Pros:** Perfect markdown rendering (full context available). Handles complex structures
like tables and nested lists correctly since the full document is always re-parsed.

**Cons:** O(n) re-render on every chunk. For long outputs this becomes expensive.

**The Cycling Characters Animation (Pre-TTFT):**

mods has the most visually impressive "thinking" animation in the CLI space. Before the first
token arrives, it shows cycling random characters with a gradient ramp:

```go
type cyclingChar struct {
    finalValue   rune    // if < 0 cycle forever
    currentValue rune
    initialDelay time.Duration
    lifetime     time.Duration
}
```

Each character has three states: `charInitialState` (dot), `charCyclingState` (random rune),
`charEndOfLifeState` (reveals to final value). The label text ("Generating...") decrypts
through the cycling chars while a gradient ramp cycles colors at 5fps.

The gradient uses true color BlendLuv interpolation between `#F967DC` and `#6B50FF`:

```go
func makeGradientRamp(length int) []lipgloss.Color {
    for i := 0; i < length; i++ {
        step := start.BlendLuv(end, float64(i)/float64(length))
        c[i] = lipgloss.Color(step.Hex())
    }
}
```

After all label characters reach end-of-life, an ellipsis spinner starts after a 220ms pause.

**State Machine:**

```
startState -> configLoadedState -> requestState -> responseState -> doneState
                                     |
                                     v
                                  errorState
```

The View() function branches on state:
- `requestState` -> show animation (cycling chars)
- `responseState` -> show glamour-rendered markdown (or viewport if tall)
- `doneState` -> empty (Bubble Tea exits)

**Source:** `/tmp/mods-research/`

---

### 3. Patterns for Bounded/Contained Streaming Areas

Three proven patterns exist for showing streaming text in a bounded area:

**Pattern A: Viewport with Auto-Scroll (mods approach)**

The full output is rendered into a `viewport.Model` widget. The viewport tracks scroll
position and auto-scrolls to bottom when new content arrives, but only if the user was
already at the bottom:

```go
wasAtBottom := m.glamViewport.ScrollPercent() == 1.0
// ... update content ...
if oldHeight < m.glamHeight && wasAtBottom {
    m.glamViewport.GotoBottom()
}
```

**Pattern B: Last-N-Lines Window (Nika inline pattern)**

The Nika TUI's InferStream box shows only the last 3 lines of streaming content:

```rust
let content_lines: Vec<&str> = data.content.lines().collect();
let start = content_lines.len().saturating_sub(3);
for line in content_lines.iter().skip(start) {
    let display = truncate_str(line, 50);
    // render into box
}
```

This creates a "tail -f" effect within a fixed-height widget. Ideal for task boxes
where you want a preview without consuming the full panel.

**Pattern C: Raw Mode Rewrite (aichat approach)**

aichat enters raw mode, tracks the exact row count of its buffer output, then on each
update: moves cursor back to the start, clears everything below, and reprints. This gives
pixel-perfect control but only works for full-terminal-width output, not bounded widgets.

---

### 4. Truncated Preview of Streaming Tokens

**Last N Characters:**

```rust
fn truncated_preview(content: &str, max_chars: usize) -> String {
    let chars: Vec<char> = content.chars().collect();
    if chars.len() <= max_chars {
        return content.to_string();
    }
    // Show ellipsis + last N chars
    format!("...{}", chars[chars.len() - max_chars..].iter().collect::<String>())
}
```

**Last N Lines (Nika pattern):**

The Nika InferStream box uses this approach: collect all lines, skip to the last 3,
and truncate each line to a max width. This gives a "terminal window into the stream"
effect.

**Tail with Scroll Context:**

Some tools show "42 lines above" as a header, then the last N visible lines. This gives
context about total output while keeping the view bounded.

---

### 5. Live Token Count Display

**Nika's Approach (Best-in-Class):**

The Nika TUI tracks tokens at multiple levels simultaneously:

1. **Turn Metrics** (`TurnMetrics`): input_tokens, output_tokens, cost_usd per inference call
2. **Session Metrics** (`SessionMetrics`): cumulative across all turns
3. **Pro Status Bar**: Two-line display showing model, cost, token usage with gauge:
   ```
   | 🔢 1.2k/200k (0.6%) ▓▓░░░░░░░░ |
   ```

Token updates happen via delta accumulation:
```rust
pub fn update_turn_metrics(&mut self, input_tokens: u64, output_tokens: u64, cost_usd: f64) {
    let input_delta = input_tokens.saturating_sub(self.turn_metrics.input_tokens);
    let output_delta = output_tokens.saturating_sub(self.turn_metrics.output_tokens);
    let cost_delta = cost_usd.max(0.0) - self.turn_metrics.cost_usd.max(0.0);
    // Update both turn and session
    self.turn_metrics.input_tokens = input_tokens;
    self.session_metrics.output_tokens += output_delta;
    ...
}
```

**aichat's approach:** Does not show live token counts during streaming. Tokens are only
counted after completion.

**Inline Display Pattern:**

The Nika InferStream box shows: `📊 {tokens_in} in -> {tokens_out} out`

---

### 6. Token Velocity Sparkline

Nika has a unique `TokenVelocity` tracker that maintains a ring buffer of tokens/sec samples:

```rust
pub struct TokenVelocity {
    samples: VecDeque<f32>,  // Ring buffer of tokens/sec
    capacity: usize,         // 30 samples default (~0.5s at 60fps)
}
```

It generates sparkline characters using Unicode block elements:
```rust
const SPARKLINE_CHARS: &[char] = &['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
```

This shows a visual history of streaming speed, making throughput drops or bursts visible.

---

### 7. "Thinking" Indicator (Pre-TTFT)

**aichat's Spinner:**

Uses braille spinner characters at 50ms intervals with configurable message:

```rust
const DATA: [&'static str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

fn step(&mut self) -> Result<()> {
    let frame = Self::DATA[self.index % Self::DATA.len()];
    let dots = ".".repeat((self.index / 5) % 4);
    let line = format!("{frame}{}{:<3}", self.message, dots);
    queue!(writer, cursor::MoveToColumn(0), style::Print(line))?;
    if self.index == 0 {
        queue!(writer, cursor::Hide)?;
    }
    writer.flush()?;
    self.index += 1;
    Ok(())
}
```

The spinner runs on its own tokio task and is killed when the first token arrives:
```rust
let mut spinner = Some(spawn_spinner("Generating"));
// ... on first text event:
if let Some(spinner) = spinner.take() {
    spinner.stop();
}
```

**mods' Cycling Chars:**

As described above: Matrix-style character cycling with gradient colors, transitioning to
an ellipsis after the label reveals.

**Nika's Agent Phase System:**

Nika has the most granular pre-TTFT indication with a full phase state machine:

```
Idle -> Syncing -> Planning -> Routing -> Invoking -> Processing -> Inferring -> Streaming -> Idle
```

Each phase has a dedicated color and animated indicator. The `AgentPhaseIndicator` widget
shows the current phase with a Matrix decrypt animation. The transition from Inferring
to Streaming marks the TTFT moment.

**Nika's Braille Spinner:**

```rust
pub const BRAILLE_SPINNER: &[char] = &['⣾', '⣽', '⣻', '⢿', '⡿', '⣟', '⣯', '⣷'];
```

Used in `BoxState::Running` for task boxes.

---

### 8. Real-Time Cost Estimation

**Nika's InferBox:**

Cost is calculated per-model with hardcoded rates:
```rust
pub fn calculate_cost(input_tokens: u64, output_tokens: u64, model: &str) -> f64 {
    let (input_rate, output_rate) = match model {
        m if m.contains("claude") => (3.0, 15.0),  // $3/M in, $15/M out
        m if m.contains("gpt-4") => (5.0, 15.0),
        m if m.contains("mistral") => (2.0, 6.0),
        _ => (1.0, 3.0),
    };
    (input_tokens as f64 / 1_000_000.0) * input_rate
    + (output_tokens as f64 / 1_000_000.0) * output_rate
}
```

Updated on every token metric update and displayed in the Pro Status Bar as `💰 $0.42`.

The cost delta tracking prevents double-counting:
```rust
let cost_delta = cost_usd.max(0.0) - self.turn_metrics.cost_usd.max(0.0);
self.session_metrics.cost_usd += cost_delta;
```

**Best Practice:** Show both turn cost and session cost. Turn cost resets per inference,
session cost accumulates. Format as `$0.001` for small amounts, `$1.23` for larger.

---

### 9. Markdown Rendering in Terminals During Streaming

**Three approaches in production:**

| Approach | Tool | Library | Streaming? | Quality |
|----------|------|---------|------------|---------|
| Line-by-line syntax highlighting | aichat | syntect | Yes (incremental) | Good |
| Full document re-render | mods | glamour | Yes (accumulate+rerender) | Best |
| Convert to ratatui Text | tui-markdown | pulldown-cmark | Batch only | Good |

**aichat's syntect approach (best for streaming):**

Uses a `LineType` state machine to track whether we're inside a code block:

```rust
enum LineType { Normal, CodeBegin, CodeInner, CodeEnd }
```

Completed lines: rendered with full syntax highlighting via syntect's `HighlightLines`.
Code block detection via `` ``` `` prefix. Language-specific syntax loaded from bat's
syntax pack (shipped as embedded binary).

Inline markdown (headers, bold, links): highlighted via the "md" syntax definition.

Color conversion handles truecolor and ANSI256 fallback:
```rust
fn convert_color(c: SyntectColor, truecolor: bool) -> Color {
    if truecolor {
        Color::Rgb { r: c.r, g: c.g, b: c.b }
    } else {
        Color::AnsiValue((c.r, c.g, c.b).to_ansi256())
    }
}
```

**tui-markdown (for ratatui/bounded widgets):**

Converts markdown to `ratatui::text::Text` using pulldown-cmark. Supports code highlighting
via syntect. The API is simple:

```rust
let text: Text = tui_markdown::from_str(markdown_content);
frame.render_widget(text, area);
```

This is the right approach for a bounded widget in a ratatui TUI, but it does a full
re-parse on every call. For streaming, you'd want to cache the parsed output and only
re-parse when new content arrives.

---

### 10. UX State Machine: Spinner -> Streaming -> Complete

**The Universal Pattern:**

```
[User sends message]
    |
    v
THINKING (spinner/animation)  <-- Show immediately
    |  (TTFT: first token arrives)
    v
STREAMING (tokens appearing)  <-- Transition: kill spinner, start text
    |  (Done event)
    v
COMPLETE (full rendered output)  <-- Transition: finalize, show stats
```

**aichat implementation:**
```
spawn_spinner("Generating")
  -> on first SseEvent::Text: spinner.take().stop()
  -> text renders with markdown highlighting
  -> on SseEvent::Done: break, disable_raw_mode()
```

**mods implementation:**
```
configLoadedState: show cycling chars animation
  -> requestState: continue animation
  -> responseState: switch to glamour-rendered markdown
  -> doneState: tea.Quit()
```

**Nika implementation (most granular):**
```
Idle
  -> Syncing (agent init)
  -> Planning (turn begins)
  -> Invoking (MCP tool call)
  -> Processing (MCP response)
  -> Inferring (LLM call sent)          <- "Thinking" indicator here
  -> Streaming (first token)            <- Matrix decrypt effect starts
  -> Idle (done)                        <- reveal_all(), finalize_thinking()
```

**Best Practices Across All Tools:**

1. **Never show a blank screen** — spinner/animation appears immediately on submit
2. **Instant transition at TTFT** — spinner kills within one frame of first token
3. **Auto-scroll follows content** — but stops if user scrolls up manually
4. **Batch render events** — 50ms window (aichat), not per-token
5. **Clear state on completion** — finalize thinking, reset turn metrics, clear activities
6. **Abort support** — Ctrl+C cleanly cancels at any phase
7. **Cost shows incrementally** — not just at the end

---

## Comparative Analysis

| Feature | aichat | mods | Nika TUI |
|---------|--------|------|----------|
| Markdown during streaming | syntect line-by-line | glamour full rerender | Not yet (plain text) |
| Thinking indicator | Braille spinner | Gradient cycling chars | Agent phase state machine |
| Token count display | After completion | Not shown | Live in status bar + box |
| Cost display | Not shown | Not shown | Live per-turn + session |
| Token velocity | Not shown | Not shown | Sparkline ring buffer |
| Bounded preview | Full terminal | Viewport with scroll | Last-3-lines box |
| Event batching | 50ms window | Bubble Tea event loop | ratatui tick |
| Matrix effect | No | Cycling chars pre-TTFT | Full decrypt effect during streaming |

---

## Recommendations for Nika

### Priority 1: Markdown in Chat Messages

**Approach:** Use `pulldown-cmark` with a custom ratatui renderer (like tui-markdown)
for completed messages. For streaming messages, use aichat's line-by-line approach:
maintain a `LineType` state machine, render complete lines through the full parser,
and render the trailing incomplete line speculatively.

**Why not full re-render (mods approach):** In a ratatui TUI with bounded widgets,
re-rendering the full document on every token is too expensive. The list-based message
display means only visible lines need rendering.

### Priority 2: Event Batching

Adopt aichat's 50ms gather window pattern. Instead of handling each streaming token
individually, collect tokens over a 50ms window and process them as a batch. This
dramatically reduces render overhead.

### Priority 3: Enhanced TTFT Indicator

The agent phase system is already excellent. Consider adding the elapsed time since
the user sent the message (TTFT clock) that transitions to tokens/sec once streaming
begins.

### Priority 4: Streaming Markdown Highlighting

For code blocks specifically, integrate syntect for syntax highlighting within the
streaming decrypt effect. The LineType state machine from aichat is well-tested and
handles edge cases like Kitty terminal duplicate lines.

---

## Rust Crates Referenced

| Crate | Version | Purpose |
|-------|---------|---------|
| `syntect` | 5.0 | Syntax highlighting (bat's syntax pack) |
| `pulldown-cmark` | latest | Markdown parsing to events |
| `tui-markdown` | latest | Markdown -> ratatui::Text conversion |
| `crossterm` | 0.28 | Terminal control, raw mode, cursor |
| `textwrap` | 0.16 | Word wrapping with Unicode awareness |
| `ansi_colours` | 1.2 | True color -> ANSI256 fallback |
| `ratatui` | latest | TUI framework (bounded widgets, viewport) |

## Sources

1. sigoden/aichat — `src/render/stream.rs`, `src/render/markdown.rs`, `src/utils/spinner.rs`
2. charmbracelet/mods — `mods.go`, `anim.go`, `stream.go`, `styles.go`
3. joshka/tui-markdown — `tui-markdown/src/lib.rs`
4. Nika TUI — `views/chat/streaming.rs`, `widgets/matrix_decrypt/streaming.rs`,
   `widgets/task_box/infer.rs`, `widgets/task_box/token_velocity.rs`

## Methodology

- Tools used: Git clone + source analysis of 4 codebases
- Files analyzed: ~35 source files across Rust and Go
- Approach: Direct source code reading, not documentation — implementation truth

## Confidence Level

**High** — All findings are based on reading actual production source code, not documentation
or blog posts. The patterns described are in active use by tools with thousands of GitHub stars.
