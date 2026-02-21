# EXTERNAL CRATES RESEARCH

**Date:** 2026-02-21
**Context:** Nika v0.6.0 dependency audit and improvement analysis

## Executive Summary

After researching crates.io and analyzing download/maintenance metrics, this document recommends specific crate upgrades and additions across 10 categories, plus 5 new capabilities to add.

**Priority Changes:**
1. ADD `miette` for fancy error reporting (user-facing YAML errors)
2. ADD `nucleo` for fuzzy file search (helix-quality matching)
3. ADD `syntect` for YAML syntax highlighting in TUI
4. ADD `indicatif` for better progress bars alongside TUI spinners
5. ADD `rstest` for cleaner test fixtures

---

## Current Dependencies Analysis

```toml
# From Cargo.toml - what we already have
clap = "4.5"           # CLI parsing - KEEP (best in class)
tokio = "1.49"         # Async runtime - KEEP
tracing = "0.1"        # Logging - KEEP (extend with OTel)
thiserror = "1.0"      # Error types - KEEP (add miette for display)
notify = "8"           # File watching - KEEP (upgrade to 9.0)
ratatui = "0.30"       # TUI framework - KEEP
```

---

## CATEGORY: Token Counting

**Current:** None (tokens come from rig-core/API responses)

| Crate | Downloads | Last Update | Pros | Cons |
|-------|-----------|-------------|------|------|
| `tiktoken-rs` | 4.3M | 2025-11 | OpenAI's BPE, most accurate | Heavy deps, 10MB+ binary size |
| `llm-tokenizer` | 53K | 2026-02 | Multi-model support, lightweight | Newer, less battle-tested |
| `kitoken` | 28K | 2024-12 | Pure Rust, fast | Limited model support |

**Recommendation:** SKIP - Token counts come from LLM API responses via rig-core. Adding local tokenization would only be useful for prompt truncation, which is better done at the provider level.

---

## CATEGORY: File Watching

**Current:** `notify = "8"` (feature-gated for TUI)

| Crate | Downloads | Last Update | Pros | Cons |
|-------|-----------|-------------|------|------|
| `notify` | 78M | 2026-02 (v9.0-rc.2) | Industry standard, all platforms | Need debouncer for practical use |
| `notify-debouncer-full` | 7.8M | 2026-01 | Best debouncer, handles renames | Extra dependency |
| `watchexec` | 4.3M | 2025-05 | CLI-grade, glob support | Heavier, designed for CLI apps |

**Recommendation:** UPGRADE to `notify = "9"` + `notify-debouncer-full = "0.7"`

```toml
# Upgrade path
notify = { version = "9", optional = true }
notify-debouncer-full = { version = "0.7", optional = true }
```

The debouncer handles:
- File renames (CREATE+DELETE -> RENAME)
- Rapid saves (coalesce within 200ms)
- Cross-platform edge cases

---

## CATEGORY: Syntax Highlighting

**Current:** None (YAML shown as plain text in TUI)

| Crate | Downloads | Last Update | Pros | Cons |
|-------|-----------|-------------|------|------|
| `syntect` | 10.9M | 2025-09 | bat/delta use it, Sublime themes | Heavy deps (onig/fancy-regex) |
| `tree-sitter` + `tree-sitter-yaml` | 13M + 787K | 2026-02 | Incremental, accurate AST | More complex integration |
| `synoptic` | 460K | 2024-11 | Lightweight, no deps | Limited language support |

**Recommendation:** ADD `syntect = "5.3"` for TUI syntax highlighting

```rust
// Usage in TUI YAML editor
use syntect::parsing::SyntaxSet;
use syntect::highlighting::{ThemeSet, Style};

let ps = SyntaxSet::load_defaults_newlines();
let ts = ThemeSet::load_defaults();
let syntax = ps.find_syntax_by_extension("yaml").unwrap();
let theme = &ts.themes["base16-ocean.dark"];
```

For TUI integration, convert `Style` to `ratatui::style::Style`:
```rust
fn syntect_to_ratatui(style: syntect::highlighting::Style) -> ratatui::style::Style {
    ratatui::style::Style::default()
        .fg(Color::Rgb(style.foreground.r, style.foreground.g, style.foreground.b))
}
```

---

## CATEGORY: Fuzzy Search

**Current:** None (file browser uses exact glob matching)

| Crate | Downloads | Last Update | Pros | Cons |
|-------|-----------|-------------|------|------|
| `nucleo` | 416K | 2024-04 | Helix editor quality, UTF-8 aware | API slightly complex |
| `fuzzy-matcher` | 13M | 2020-10 | skim uses it, battle-tested | Older, basic API |
| `sublime_fuzzy` | 1.7M | 2020-12 | Simple API, good scoring | No recent updates |
| `rust-fuzzy-search` | 3.9M | 2021-07 | Most downloads | Basic Levenshtein only |

**Recommendation:** ADD `nucleo = "0.5"` (used by Helix editor)

```rust
use nucleo::{Nucleo, Config};
use nucleo::pattern::{Pattern, CaseMatching};

// Create matcher
let nucleo: Nucleo<String> = Nucleo::new(Config::DEFAULT, Arc::new(|| {}), None, 1);

// Add items
for path in workflow_files {
    nucleo.injector().push(path, |s, cols| {
        cols[0] = s.as_str().into();
    });
}

// Query
let pattern = Pattern::parse("test", CaseMatching::Smart);
nucleo.pattern.reparse(&pattern, CaseMatching::Smart);

// Get results (already ranked)
let snapshot = nucleo.snapshot();
for item in snapshot.matched_items(..10) {
    println!("{}", item.data);
}
```

---

## CATEGORY: Progress Indicators

**Current:** Custom spinners in `src/tui/widgets/spinner.rs`

| Crate | Downloads | Last Update | Pros | Cons |
|-------|-----------|-------------|------|------|
| `indicatif` | 118M | 2026-02 | Most popular, rich features | Not TUI-native |
| `spinners` | 5.5M | 2023-11 | 80+ spinner styles | Standalone only |
| `throbber-widgets-tui` | 461K | 2025-12 | Ratatui-native spinners | Limited styles |

**Recommendation:**
- KEEP custom TUI spinners (already ratatui-native)
- ADD `indicatif = "0.18"` for CLI-mode progress (non-TUI runs)

```rust
// For CLI mode (no TUI)
use indicatif::{ProgressBar, ProgressStyle};

let pb = ProgressBar::new(task_count as u64);
pb.set_style(ProgressStyle::default_bar()
    .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} {msg}")
    .progress_chars("#>-"));
```

For TUI mode, continue using our custom `ROCKET_SPINNER`, `ORBIT_SPINNER`, etc.

---

## CATEGORY: Error Handling

**Current:** `thiserror = "1.0"` with NikaError enum

| Crate | Downloads | Last Update | Pros | Cons |
|-------|-----------|-------------|------|------|
| `thiserror` | 771M | 2026-01 | Industry standard, zero-cost | No fancy display |
| `miette` | 40.5M | 2025-04 | Fancy diagnostics, source spans | Extra derive macro |
| `error-stack` | 2.9M | 2025-08 | Attachments, stack traces | Different paradigm |
| `eyre` | 72.6M | 2024-01 | Dynamic errors, good backtraces | Less type-safe |
| `snafu` | 75.8M | 2025-09 | Context selectors | Verbose |

**Recommendation:** KEEP `thiserror`, ADD `miette = "7.6"` for user-facing errors

```rust
use miette::{Diagnostic, SourceSpan};
use thiserror::Error;

#[derive(Error, Diagnostic, Debug)]
#[error("Invalid workflow syntax")]
#[diagnostic(
    code(nika::parse::invalid_yaml),
    help("Check YAML indentation and key names")
)]
pub struct ParseError {
    #[source_code]
    src: String,
    #[label("error occurred here")]
    span: SourceSpan,
}
```

This gives beautiful error output:
```
Error: nika::parse::invalid_yaml

  x Invalid workflow syntax
   ,-[workflow.nika.yaml:5:1]
 5 |   infer "missing colon"
   :   ^^^^^^^^^^^^^^^^^^^^^ error occurred here
   `----
  help: Check YAML indentation and key names
```

---

## CATEGORY: Tracing/Logging

**Current:** `tracing = "0.1"` + `tracing-subscriber = "0.3"`

| Crate | Downloads | Last Update | Pros | Cons |
|-------|-----------|-------------|------|------|
| `tracing-opentelemetry` | 118M | 2026-01 | OTel export, industry standard | Complex setup |
| `console-subscriber` | 29M | 2025-10 | tokio-console debugging | Dev only |
| `tracing-appender` | 56M | 2025-11 | File rotation, async writing | Need for production logs |

**Recommendation:** ADD for production observability

```toml
[dependencies]
tracing-opentelemetry = { version = "0.32", optional = true }
opentelemetry = { version = "0.31", optional = true }
opentelemetry_sdk = { version = "0.31", optional = true }

[dev-dependencies]
console-subscriber = "0.5"

[features]
otel = ["dep:tracing-opentelemetry", "dep:opentelemetry", "dep:opentelemetry_sdk"]
```

---

## CATEGORY: Config Management

**Current:** `dotenvy = "0.15"` + `toml = "0.8"` (manual loading)

| Crate | Downloads | Last Update | Pros | Cons |
|-------|-----------|-------------|------|------|
| `config` | 69.6M | 2025-11 | Multi-source merge, popular | API verbose |
| `figment` | 20.7M | 2024-05 | Rocket uses it, composable | Less active |
| `toml` (manual) | - | - | Full control, minimal deps | More code |

**Recommendation:** KEEP current approach (dotenvy + manual toml)

Nika's config is simple:
- API keys from env vars (dotenvy handles .env)
- Workflow files are self-contained YAML
- No complex layered config needed

If config grows complex, consider `config` crate later.

---

## CATEGORY: CLI Parsing

**Current:** `clap = "4.5"` with derive

| Crate | Downloads | Last Update | Pros | Cons |
|-------|-----------|-------------|------|------|
| `clap` | 678M | 2026-02 | Most features, best maintained | Compile time |
| `argh` | 11M | 2026-02 | Google, fast compile | Less features |
| `bpaf` | 4.3M | 2026-02 | Minimal, good derive | Smaller community |
| `pico-args` | 41M | 2022-06 | Zero deps, fastest | No help generation |

**Recommendation:** KEEP `clap = "4.5"` - no benefit to switching

Clap is the right choice:
- Subcommands (run, check, chat, studio)
- Derive macros reduce boilerplate
- Shell completions out of the box
- Best documentation

---

## CATEGORY: Testing

**Current:** `proptest`, `insta`, `pretty_assertions`, `tempfile`, `wiremock`

| Crate | Downloads | Last Update | Pros | Cons |
|-------|-----------|-------------|------|------|
| `rstest` | 64.4M | 2025-07 | Fixtures, parametrized tests | Learning curve |
| `fake` | 10.4M | 2025-08 | Random data generation | Need with proptest |
| `mockall` | 101M | 2025-11 | Auto-mock traits | Macro complexity |
| `test-case` | 33.3M | 2024-12 | Simple parametrized | Less flexible than rstest |

**Recommendation:** ADD `rstest = "0.26"` + `fake = "4.4"`

```rust
use rstest::*;
use fake::{Fake, Faker};
use fake::faker::name::en::*;

#[fixture]
fn workflow() -> Workflow {
    Workflow {
        schema: "nika/workflow@0.5".to_string(),
        name: "test-workflow".to_string(),
        tasks: vec![],
        ..Default::default()
    }
}

#[rstest]
#[case("infer", TaskAction::Infer(_))]
#[case("exec", TaskAction::Exec(_))]
fn test_parse_verb(#[case] verb: &str, #[case] expected: TaskAction, workflow: Workflow) {
    // workflow fixture injected automatically
}

// Generate random test data
let random_id: String = (8..16).fake::<String>();
```

---

## NEW CAPABILITIES TO ADD

### 1. Tree-sitter for YAML (Semantic Analysis)

**Crate:** `tree-sitter = "0.26"` + `tree-sitter-yaml = "0.7"`

**What it enables:**
- Incremental YAML parsing (only reparse changed regions)
- Accurate syntax error locations
- Semantic queries (find all `infer:` tasks)
- Potential LSP foundation

```rust
use tree_sitter::{Parser, Language};

extern "C" { fn tree_sitter_yaml() -> Language; }

let mut parser = Parser::new();
parser.set_language(unsafe { tree_sitter_yaml() })?;

let tree = parser.parse(yaml_source, None)?;
let root = tree.root_node();

// Query for all task IDs
let query = Query::new(
    unsafe { tree_sitter_yaml() },
    "(block_mapping_pair key: (flow_node) @key (#eq? @key \"id\") value: (_) @value)"
)?;
```

**Recommendation:** Consider for v0.7 - enables LSP server and better IDE integration.

---

### 2. LSP Server for `.nika.yaml` Files

**Crate:** `tower-lsp = "0.20"` (3.7M downloads, but outdated API)
**Better:** `tower-lsp-server = "0.23"` (350K, more recent)

**What it enables:**
- Autocomplete for verbs (`infer:`, `exec:`, etc.)
- Hover documentation
- Go-to-definition for `use:` bindings
- Real-time validation

**Recommendation:** Phase 2 feature (post-v1.0). Requires tree-sitter foundation first.

---

### 3. OpenTelemetry for Production Tracing

**Crates:**
- `tracing-opentelemetry = "0.32"` (118M downloads)
- `opentelemetry = "0.31"` (153M downloads)
- `opentelemetry_sdk = "0.31"` (119M downloads)

**What it enables:**
- Export traces to Jaeger, Honeycomb, Datadog
- Distributed tracing across MCP calls
- Production debugging

```rust
use tracing_opentelemetry::OpenTelemetryLayer;
use opentelemetry::global;
use opentelemetry_sdk::trace::TracerProvider;

let tracer = TracerProvider::builder()
    .with_simple_exporter(opentelemetry_jaeger::new_agent_pipeline())
    .build();

tracing_subscriber::registry()
    .with(OpenTelemetryLayer::new(tracer))
    .init();
```

**Recommendation:** ADD behind feature flag for production deployments.

---

### 4. Better Async Patterns

**Crate:** `async-stream = "0.3"` (194M downloads)

**What it enables:**
- Cleaner streaming responses
- Generator-style async iterators

```rust
use async_stream::stream;

fn token_stream(prompt: &str) -> impl Stream<Item = String> {
    stream! {
        for token in provider.stream(prompt).await {
            yield token;
        }
    }
}
```

**Already have:** `futures = "0.3"` covers most needs. `async-stream` is nice-to-have.

---

### 5. TUI Ecosystem Widgets

**Crates to ADD:**

| Crate | Downloads | Purpose |
|-------|-----------|---------|
| `ratatui-macros` | 1.1M | `line!`, `span!` macros for cleaner code |
| `tui-tree-widget` | 489K | File tree browser |
| `tui-scrollview` | 250K | Scrollable content areas |
| `tui-big-text` | 282K | ASCII art banners |
| `tui-logger` | 1.3M | In-TUI log viewer |
| `throbber-widgets-tui` | 461K | Additional spinner styles |

**Recommendation:** ADD `ratatui-macros` (immediate DX win), consider others as needed.

```rust
// Before
Paragraph::new(vec![
    Line::from(vec![
        Span::styled("Status: ", Style::default().bold()),
        Span::styled("Running", Style::default().fg(Color::Green)),
    ])
])

// After with ratatui-macros
use ratatui_macros::{line, span};
Paragraph::new(line![
    span!["Status: "].bold(),
    span!["Running"].green(),
])
```

---

## Summary: Recommended Changes

### Immediate (v0.6.1)

```toml
# Cargo.toml additions

[dependencies]
miette = { version = "7.6", features = ["fancy"] }  # Error display
nucleo = "0.5"                                       # Fuzzy search
ratatui-macros = "0.7"                               # TUI DX

[dependencies.notify]
version = "9"                                        # UPGRADE from 8
optional = true

[dependencies.notify-debouncer-full]
version = "0.7"
optional = true

[dev-dependencies]
rstest = "0.26"
fake = { version = "4.4", features = ["derive"] }
```

### Near-term (v0.7)

```toml
[dependencies]
syntect = { version = "5.3", optional = true }
indicatif = { version = "0.18", optional = true }
tree-sitter = { version = "0.26", optional = true }
tree-sitter-yaml = { version = "0.7", optional = true }

[features]
syntax-highlight = ["dep:syntect"]
progress = ["dep:indicatif"]
tree-sitter = ["dep:tree-sitter", "dep:tree-sitter-yaml"]
```

### Production Features (v1.0)

```toml
[dependencies]
tracing-opentelemetry = { version = "0.32", optional = true }
opentelemetry = { version = "0.31", optional = true }
opentelemetry_sdk = { version = "0.31", optional = true }

[features]
otel = ["dep:tracing-opentelemetry", "dep:opentelemetry", "dep:opentelemetry_sdk"]
```

---

## Utility Crates Worth Adding

These small, high-value crates improve code quality:

| Crate | Downloads | Purpose | Add? |
|-------|-----------|---------|------|
| `strum` | 357M | Enum iteration, Display | YES |
| `derive_more` | 260M | Extra derives (From, Into) | CONSIDER |
| `bon` | 20.8M | Builder pattern macro | CONSIDER |
| `humantime` | 305M | Duration formatting | YES |
| `humansize` | 38.9M | Byte size formatting | YES |

```toml
# Small utilities to add
strum = { version = "0.27", features = ["derive"] }
humantime = "2.3"
humansize = "2.1"
```

---

## What NOT to Change

| Current | Keep Because |
|---------|--------------|
| `clap` | Best CLI parser, no real alternative |
| `tokio` | Only mature async runtime |
| `serde_yaml` | Standard YAML parsing |
| `thiserror` | Keep for internal errors (miette for display) |
| `tracing` | Extend, don't replace |
| `dashmap` | Best concurrent map |
| `camino` | Already using best path handling |
| `ignore` | Already using ripgrep author's crate |

---

## References

- crates.io API queries: 2026-02-21
- Download counts as of snapshot date
- Last update times from crate metadata
