# Research Report: Best TUI Wizard & Setup Experiences for CLI Tools

**Date**: 2026-03-22
**Goal**: Build the most beautiful and immersive TUI wizard for Nika's `init` command
**Researcher**: Claude Opus 4.6

---

## Executive Summary

The state of the art in CLI setup wizards has evolved dramatically. The JavaScript ecosystem
(create-astro, create-next-app, SvelteKit) pioneered the "guided journey" pattern, and the Go
ecosystem (Charm.sh) perfected the visual polish. The Rust ecosystem now has `cliclack` -- a
direct port of the `@clack/prompts` paradigm -- which is production-ready and the clear winner
for building a premium wizard in Rust.

**Recommendation**: Use `cliclack` as the prompt framework. It gives Nika the exact visual
language of create-astro/create-svelte (the gold standard) but in pure Rust. Layer it with
`console` for styling, `indicatif` for progress, and `ratatui` if you need any full-screen moments.

---

## 1. JavaScript Wizards (The Gold Standard)

### 1.1 create-next-app (Vercel)

**Stack**: `prompts` library + `picocolors` for color + `commander` for CLI args

**Flow**:
```
What is your project named?          [text input with validation]
Would you like to use TypeScript?    [toggle: Yes/No]
Would you like to use ESLint?        [toggle: Yes/No]
Would you like to use Tailwind CSS?  [toggle: Yes/No]
Would you like to use `src/` dir?    [toggle: Yes/No]
Would you like to use App Router?    [toggle: Yes/No]
What import alias would you like?    [text input, default @/*]
```

**What makes it premium**:
- **Progressive disclosure**: Only asks what it needs, skips with `--yes` flag
- **Saved preferences**: Uses `Conf` to remember choices (never asks twice)
- **Smart defaults**: Detects package manager (`npm`/`pnpm`/`yarn`/`bun`) automatically
- **Graceful abort**: Re-enables cursor on SIGINT (prevents hidden cursor bug)
- **Validation inline**: npm name validation happens as you type
- **Color scheme**: Minimal -- `cyan` for prompts, `green` for success, `red` for errors
- **CI detection**: Uses `ci-info` to skip interactive prompts in CI

**Key insight**: create-next-app is *functional* but not *beautiful*. It's the baseline, not the ceiling.

### 1.2 create-astro (Astro) -- The Visual Masterpiece

**Stack**: `@astrojs/cli-kit` (custom) + `@clack/prompts`-style UI

**Flow**:
```
 astro   Launch sequence initiated.

      Welcome to  astro  v5.x, astronaut!

   Where should we create your project?
   ./my-project

   How would you like to start your new project?
   > Use blog template
     Empty project
     Minimal

   Do you plan to write TypeScript?
   > Yes / No

   How strict should TypeScript be?
   > Strict (recommended)
     Strictest
     Relaxed

   Install dependencies?
   > Yes / No

   Initialize a git repository?
   > Yes / No

  next   Liftoff confirmed. Explore your project!

   Enter your project directory using cd ./my-project
   Run pnpm dev to start the dev server.
   Add frameworks like react or tailwind using astro add.

   Stuck? Join us at https://astro.build/chat
```

**What makes it legendary**:
- **Houston mascot**: ASCII art character that "speaks" to you (with typing animation)
- **Typing animation**: Messages appear character by character with `sleep(100ms)` delays
- **Label badges**: `astro` appears as a styled badge (background color + inverted text)
- **Themed sections**: `intro`, `steps`, `tasks`, `next-steps` -- each visually distinct
- **Task runner**: Shows spinner during install, with success checkmarks
- **Personalization**: Detects git username, greets you by name
- **Color scheme**: Green badges, cyan accents, dim helper text
- **Responsive layout**: Checks `stdout.columns` and adjusts layout for narrow terminals

### 1.3 @clack/prompts (Nate Moore) -- The Design System

This is the library that powers create-svelte, create-astro, and many others.
It is THE reference design for modern CLI wizards.

**Visual Language** (Unicode symbols):
```
 Symbols (with ASCII fallbacks):
   Active step:    (or *)
   Submit step:    (or o)
   Cancel step:    (or x)
   Error step:     (or x)
   Bar:            (or |)
   Bar end:        (or -)
   Radio active:   (or >)
   Radio inactive: (or  )
   Checkbox:       /  (or [+]/[ ])
   Password mask:  (or bullet)
```

**State-based coloring**:
```
  initial/active  = cyan
  submit          = green
  cancel          = red
  error           = yellow
```

**Layout pattern** (the "guide rail"):
```
       Step title
       Option 1
       Option 2
       Option 3

```

The vertical bar acts as a "guide rail" connecting all steps into one visual flow.
This is the single most important design pattern in modern CLI wizards.

**Key features**:
- **Guide rail**: Continuous `|` bar connecting all prompts vertically
- **State transitions**: Symbol changes from `*` (active) to `o` (done) to `x` (cancelled)
- **Spinner with timer**: Shows elapsed time `[3s]` during long operations
- **Unicode detection**: Falls back to ASCII on unsupported terminals
- **Signal handling**: Graceful cleanup on SIGINT/SIGTERM/uncaughtException
- **Screen wrapping**: `wrapAnsi` for long text that respects terminal width

---

## 2. Go Ecosystem (Charm.sh) -- The Visual Perfectionists

### 2.1 Bubble Tea (charmbracelet/bubbletea)

**Architecture**: The Elm Architecture (TEA) for terminals.

```
Model -> Init() -> Update(msg) -> View() -> string
```

**What makes it special**:
- **Cell-based renderer**: High-performance differential rendering
- **Color downsampling**: Truecolor -> 256 -> 16 -> 1-bit automatically
- **Mouse support**: Full mouse event handling
- **Clipboard**: Native clipboard integration
- **Alt screen**: Full-screen mode when needed

**Key insight**: Bubble Tea is a *framework*, not a prompt library. It's what you'd use
to build something like Nika's TUI (which already uses ratatui). For a wizard, you want
the higher-level abstractions built ON TOP of it.

### 2.2 Huh? (charmbracelet/huh) -- The Form Library

This is Charm's equivalent of @clack/prompts but for Go.

**Field types**: Input, Text, Select, MultiSelect, Confirm

**What makes it legendary**:
- **Groups = Pages**: Form is divided into groups; each group is a "page" of the wizard
- **5 built-in themes**: Charm, Dracula, Catppuccin, Base 16, Default
- **Dynamic forms**: Options/titles can be functions that recompute based on other selections
- **Accessibility mode**: Drops TUI rendering for screen readers (`ACCESSIBLE=true`)
- **Lip Gloss integration**: CSS-like styling for every component
- **Spinner companion**: Standalone spinner package for post-form processing
- **Keyboard**: j/k navigation, `/` to filter, vim mode

**Theme architecture**:
```go
type Theme struct {
    Form           lipgloss.Style
    Group          lipgloss.Style
    FieldSeparator lipgloss.Style
    Blurred        FieldStyles
    Focused        FieldStyles
    Help           HelpStyles
}
```

Each theme controls: borders, colors, padding, margins, cursor style, prompt prefix,
checkbox/radio symbols, selected/unselected styles, error styles.

### 2.3 Gum (charmbracelet/gum) -- Shell-Script Beauty

Gum makes Bubble Tea components available as standalone CLI commands.

**Available commands**:
| Command | Purpose |
|---------|---------|
| `gum choose` | Select from list |
| `gum confirm` | Yes/No |
| `gum filter` | Fuzzy filter list |
| `gum input` | Single-line text |
| `gum write` | Multi-line text |
| `gum spin` | Spinner |
| `gum style` | Style text |
| `gum table` | Render table |
| `gum file` | File picker |
| `gum pager` | Scroll content |
| `gum format` | Format text |
| `gum log` | Log messages |
| `gum join` | Join text blocks |

**Spinner types**: line, dot, minidot, jump, pulse, points, globe, moon, monkey, meter, hamburger

**What matters for Nika**: Gum proves that individual prompts composed together
create a premium experience. The key is consistent styling across all components.

### 2.4 Lip Gloss (charmbracelet/lipgloss) -- The CSS of Terminals

**Styling primitives** (the complete set):
```
Inline:   Bold, Italic, Faint, Blink, Strikethrough, Underline, Reverse
          Underline styles: Single, Double, Curly, Dotted, Dashed
          Hyperlinks (clickable in supporting terminals)
Block:    Padding (top/right/bottom/left), Margin, Width, Height
          Alignment (Left, Center, Right)
Borders:  Normal, Rounded, Thick, Double, Hidden, Custom
          Gradient borders (multi-color)
Colors:   ANSI 16, ANSI 256, TrueColor, auto-downsampling
          Darken, Lighten, Complementary, Alpha
```

**Key insight for Nika**: Lip Gloss is for Go, but the *concepts* apply directly.
In Rust, use the `console` crate for styling + `crossterm` for raw terminal control.

---

## 3. Rust Ecosystem

### 3.1 cliclack -- THE WINNER for Nika

**What**: Direct Rust port of `@clack/prompts`. Same visual language, same guide rail pattern.

**Available prompts**:
```rust
cliclack::intro("title")?;              // Session start
cliclack::input("prompt").interact()?;   // Text input with validation
cliclack::password("prompt").interact()?; // Masked input
cliclack::select("prompt")              // Single select
    .item("val", "Label", "hint")
    .interact()?;
cliclack::multiselect("prompt")         // Multi select
    .item("val", "Label", "hint")
    .interact()?;
cliclack::confirm("prompt").interact()?; // Yes/No
cliclack::spinner();                     // Progress spinner
cliclack::note("title", "body")?;       // Info box
cliclack::outro("message")?;            // Session end
cliclack::log::info("message")?;        // Styled log
cliclack::log::warning("message")?;
cliclack::log::error("message")?;
cliclack::log::success("message")?;
```

**Features**:
- Theme support (custom themes possible)
- Progress bars and multi-progress
- Autocomplete on text inputs
- Validation with error messages
- Uses `console` crate for styling under the hood
- Unicode detection with ASCII fallbacks
- Ctrl-C handling (graceful abort)

**Example** (the complete wizard pattern):
```rust
use console::style;

cliclack::clear_screen()?;
cliclack::intro(style(" create-app ").on_cyan().black())?;

let path: String = cliclack::input("Where should we create your project?")
    .placeholder("./sparkling-solid")
    .validate(|input: &String| {
        if input.is_empty() { Err("Please enter a path.") }
        else { Ok(()) }
    })
    .interact()?;

let kind = cliclack::select("Pick a project type")
    .initial_value("ts")
    .item("ts", "TypeScript", "")
    .item("js", "JavaScript", "")
    .interact()?;

let tools = cliclack::multiselect("Select additional tools")
    .initial_values(vec!["prettier", "eslint"])
    .item("prettier", "Prettier", "recommended")
    .item("eslint", "ESLint", "recommended")
    .interact()?;

let install = cliclack::confirm("Install dependencies?").interact()?;

if install {
    let spinner = cliclack::spinner();
    spinner.start("Installing via pnpm");
    // do work...
    spinner.stop("Installed via pnpm");
}

cliclack::note("Next steps", "cd project\npnpm dev")?;
cliclack::outro("You're all set!")?;
```

**Why cliclack wins for Nika**:
1. Same visual language as the best JS wizards (Astro, SvelteKit, T3)
2. Pure Rust, zero JS dependency
3. Minimal API surface -- easy to learn, hard to misuse
4. Theme support for Nika branding
5. Works with `console` crate (already in Rust ecosystem)
6. Active maintenance, growing adoption

### 3.2 inquire -- The Feature-Rich Alternative

**Prompt types**: Text, Editor, DateSelect, Select, MultiSelect, Confirm, CustomType, Password

**Unique strengths**:
- **DateSelect**: Interactive calendar widget (unique among all tools)
- **Autocompletion**: Built-in autocomplete with suggestion lists
- **Fuzzy matching**: SkimV2 fuzzy search on Select/MultiSelect
- **Derive macros**: `#[derive(Selectable)]` for enums
- **RenderConfig**: Deep customization of every visual element
- **Three backends**: crossterm (default), termion, console

**RenderConfig customization**:
```rust
RenderConfig {
    prompt_prefix,          // Symbol before prompt text
    answered_prompt_prefix, // Symbol after answering
    highlighted_option_prefix, // Symbol for current selection
    selected_checkbox,      // Checked checkbox symbol
    unselected_checkbox,    // Unchecked checkbox symbol
    error_message,          // Error styling
    default_value,          // Default value styling
    help_message,           // Help text styling
    answer,                 // Submitted answer styling
    // ... and more
}
```

**Why not inquire**: It's feature-rich but doesn't have the "guide rail" visual flow.
Each prompt is visually independent. For a *wizard* (connected flow), cliclack's
vertical bar pattern is superior.

### 3.3 dialoguer -- The Minimal Classic

**Part of**: console-rs family (`console` + `dialoguer` + `indicatif`)

**Prompt types**: Input, Password, Confirm, Select, MultiSelect, Sort, FuzzySelect, Editor

**Themes**: `ColorfulTheme` (default) or custom

**Example**:
```rust
let selection = Select::with_theme(&ColorfulTheme::default())
    .with_prompt("Pick your flavor")
    .default(0)
    .items(&["Ice Cream", "Vanilla Cupcake", "Chocolate Muffin"])
    .interact()?;
```

**Why not dialoguer**: Functional but visually plain. No guide rail, no intro/outro,
no spinner integration. It's a prompt library, not a wizard framework.

### 3.4 indicatif -- Progress Bars for Rust

**Key for wizards**: The post-prompt phase (creating files, installing deps) needs progress.

**Features**:
- Single and multi-progress bars
- Spinners with custom styles
- Yarn/npm-style output
- Integration with `tracing` via `tracing-indicatif`
- Integration with `log` via `indicatif-log-bridge`

**Note**: cliclack has its own spinner and progress bar. For simple wizard needs,
cliclack's built-in is sufficient. Use indicatif only for complex multi-step progress.

### 3.5 ratatui -- Full TUI Framework

Already used by Nika for the TUI. For the *wizard*, ratatui is overkill -- you don't
need a full-screen application for a setup flow. However, ratatui could provide a
brief "splash screen" moment before dropping to the cliclack prompt flow.

### 3.6 cargo-generate -- Template System

Uses `dialoguer` for prompts. The flow is minimal:
```
Project Name: my-project
[template-specific questions from cargo-generate.toml]
```

**Interesting pattern**: Template authors define prompts in `cargo-generate.toml`:
```toml
[placeholders.project-name]
type = "string"
prompt = "What is the project name?"

[placeholders.use-serde]
type = "bool"
prompt = "Include serde?"
default = true
```

**Key insight for Nika**: Nika's init could similarly use a declarative prompt
definition for different project "templates" (web app, AI pipeline, automation, etc.).

---

## 4. Terminal Capability Detection

### How the best tools detect capabilities

**Color support**:
| Method | Library |
|--------|---------|
| `$COLORTERM=truecolor` | True color detection |
| `$TERM` contains `256color` | 256-color detection |
| `$NO_COLOR` env var | Disable all color |
| `$FORCE_COLOR` env var | Force color output |
| `isatty()` check | Is stdout a terminal? |
| Windows: `ENABLE_VIRTUAL_TERMINAL_PROCESSING` | ANSI support on Windows |

**Unicode support**:
| Method | Library |
|--------|---------|
| `is-unicode-supported` (JS) / equivalent check | Clack, cliclack |
| `$TERM` / locale / `$LC_ALL` / `$LANG` checks | console crate |
| Fallback to ASCII symbols | All good tools |

**Terminal size**:
| Method | Library |
|--------|---------|
| `stdout.columns` (JS) | Node.js built-in |
| `crossterm::terminal::size()` | Rust/crossterm |
| `console::Term::size()` | Rust/console |
| Responsive layout adjustment | create-astro, Lip Gloss |

**CI detection**:
| Method | Library |
|--------|---------|
| `$CI=true` | Most tools |
| `ci-info` package (JS) | create-next-app |
| Skip interactive prompts in CI | All good tools |

**Emoji support**:
| Method | Library |
|--------|---------|
| `wants_emoji()` | console crate |
| macOS: always yes | Platform check |
| Windows: check Windows version | Platform check |
| Linux: check locale | Locale check |

### Recommended detection for Nika:
```rust
// Use `console` crate (already available via cliclack dependency)
use console::Term;

let term = Term::stdout();
let features = term.features();

// Is this a real terminal?
let is_interactive = features.is_attended();

// Color support
let has_colors = features.colors_supported();
let has_truecolor = features.true_colors_supported();

// Emoji support
let has_emoji = features.wants_emoji();

// Terminal size
let (rows, cols) = term.size();

// Unicode (from cliclack internals, or is-terminal + locale check)
let has_unicode = is_unicode_supported(); // check $LANG, $LC_ALL, etc.

// CI mode
let is_ci = std::env::var("CI").map(|v| v == "true").unwrap_or(false);
```

---

## 5. Design Patterns That Make Wizards Feel "Premium"

### 5.1 The Guide Rail Pattern (MUST HAVE)

The single most important visual innovation in modern CLI wizards.
A continuous vertical bar (`|`) connects all prompts into one visual flow:

```
|
*  Project name
|  ./my-project
|
*  Template
|  > Blog
|    Empty
|    Minimal
|
*  TypeScript?
|  Yes
|
   Done!
```

**Status**: cliclack implements this out of the box.

### 5.2 Progressive Disclosure

Only show questions that matter. Skip what can be auto-detected.

```
Detected: pnpm (from lockfile)
Detected: TypeScript (from tsconfig.json)
Detected: git (initialized)

Only asking what we can't detect:
  Which template? [select]
  Permission mode? [select]
```

### 5.3 Branded Intro/Outro

```
  nika   v0.38.0

   Welcome to Nika, Thibaut!
   Let's set up your workflow project.

   [... wizard steps ...]

  done   Project initialized!
   Run `nika run workflows/hello.nika.yaml` to get started.
```

The badge-style labels (`nika`, `done`) use inverted colors (background + foreground swap).

### 5.4 Smart Defaults with Override

Every question should have a sensible default. Power users can skip through with Enter.
The `--yes` flag should use all defaults non-interactively.

### 5.5 Validation Feedback

Errors appear inline, not after submission:
```
*  Project name
|  my project
|  Project name cannot contain spaces
```

### 5.6 Spinner for I/O Operations

After the wizard questions, use a spinner for file creation:
```
   Creating project structure...
   Created 47 files in 0.3s
```

### 5.7 Grouped Next Steps

End with actionable, copy-pasteable commands:
```
  Next steps:

  cd ./my-project
  nika keys set anthropic
  nika run workflows/tier-2-llm/04-infer-basics.nika.yaml

  Problems? https://github.com/supernovae-st/nika/issues
```

### 5.8 ASCII Art (Optional, Sparingly)

Astro uses Houston the mascot. Most tools use minimal branding.
For Nika, a butterfly (`🦋`) is the brand symbol -- use it in the banner, not as ASCII art.

### 5.9 Color Palette

The most successful wizards use a minimal palette:
| Color | Usage |
|-------|-------|
| **Cyan** | Active prompts, interactive elements, brand accents |
| **Green** | Success, completed steps, submit confirmation |
| **Red** | Errors, cancellation |
| **Yellow** | Warnings |
| **Dim/Gray** | Helper text, hints, inactive options |
| **Bold** | Important text, selected values |
| **Inverted** | Badge labels (brand name, section headers) |

### 5.10 Graceful Degradation

```
Full terminal:     Unicode symbols, colors, guide rail
Dumb terminal:     ASCII fallbacks, no colors
CI environment:    Non-interactive, defaults only, plain output
Piped output:      No prompts, no colors, machine-readable
```

---

## 6. Comparison Matrix

| Feature | cliclack | inquire | dialoguer | huh (Go) | @clack |
|---------|----------|---------|-----------|----------|--------|
| Guide rail | YES | no | no | no | YES |
| Intro/Outro | YES | no | no | no | YES |
| Select | YES | YES | YES | YES | YES |
| MultiSelect | YES | YES | YES | YES | YES |
| Text input | YES | YES | YES | YES | YES |
| Password | YES | YES | YES | no | YES |
| Confirm | YES | YES | YES | YES | YES |
| Spinner | YES | no | no | YES | YES |
| Progress bar | YES | no | indicatif | no | YES |
| Fuzzy filter | no | YES | YES | YES | no |
| Date picker | no | YES | no | no | YES |
| Autocomplete | YES | YES | no | no | YES |
| Theming | YES | YES | YES | YES | limited |
| Accessibility | no | no | no | YES | no |
| Unicode fallback | YES | YES | no | YES | YES |
| CI detection | partial | no | no | no | YES |
| Vim keys | no | YES | no | YES | no |

---

## 7. Recommended Architecture for Nika Init Wizard

### Dependencies

```toml
[dependencies]
cliclack = "0.3"      # Core wizard framework
console = "0.15"      # Styling + terminal detection
```

### Proposed Flow

```
 nika   v0.38.0

   Welcome! Let's set up your Nika project.

   Where should we create your project?
   . (current directory)

   What kind of project?
   > AI Workflows     Full setup with 30 example workflows
     Minimal           Just .nika/ config, no examples
     Agent-focused     Focus on agent definitions
     Pipeline          Media/data processing workflows

   Default LLM provider?
   > Claude (Anthropic)
     OpenAI
     Mistral
     Groq
     DeepSeek
     Native (local GGUF)
     Skip (configure later)

   Permission mode for file tools?
   > Plan           Agent proposes, you approve
     Deny           No file access
     Accept Edits   Auto-approve file edits
     YOLO           Accept everything (development only)

   Include example workflows?
   Yes

   Initialize git repository?
   Yes

   Setting up project...
   Created .nika/ configuration (8 files)
   Created workflows/ (30 examples across 6 tiers)
   Created context/, schemas/, output/
   Initialized git repository

  done   Project initialized!

  Next steps:

  # Works immediately (no API key needed)
  nika run workflows/tier-1-no-deps/01-exec-basics.nika.yaml

  # Set up your provider
  nika keys set anthropic

  # Then try LLM workflows
  nika run workflows/tier-2-llm/04-infer-basics.nika.yaml

  Docs: https://nika.dev | Issues: https://github.com/supernovae-st/nika
```

### Implementation Skeleton

```rust
use cliclack::{intro, outro, select, confirm, input, spinner, note, log};
use console::style;

pub fn init_wizard() -> Result<(), NikaError> {
    let term = console::Term::stdout();

    // Non-interactive mode (CI or --yes flag)
    if !term.features().is_attended() || args.yes {
        return init_project_defaults();
    }

    // Banner
    cliclack::clear_screen()?;
    intro(style(" nika ").on_magenta().black())?;

    // Step 1: Project location
    let path: String = input("Where should we create your project?")
        .default_input(".")
        .interact()?;

    // Step 2: Project template
    let template = select("What kind of project?")
        .item("full", "AI Workflows", "Full setup with 30 example workflows")
        .item("minimal", "Minimal", "Just .nika/ config, no examples")
        .item("agent", "Agent-focused", "Focus on agent definitions")
        .item("pipeline", "Pipeline", "Media/data processing workflows")
        .interact()?;

    // Step 3: Provider (skip if already configured)
    let provider = if env_has_provider_key() {
        log::info(format!("Detected provider: {}", detected_provider()))?;
        detected_provider()
    } else {
        select("Default LLM provider?")
            .item("claude", "Claude (Anthropic)", "")
            .item("openai", "OpenAI", "")
            .item("mistral", "Mistral", "")
            .item("groq", "Groq", "")
            .item("deepseek", "DeepSeek", "")
            .item("native", "Native (local GGUF)", "")
            .item("skip", "Skip", "configure later")
            .interact()?
    };

    // Step 4: Permission mode
    let permission = select("Permission mode for file tools?")
        .initial_value("plan")
        .item("plan", "Plan", "Agent proposes, you approve")
        .item("deny", "Deny", "No file access")
        .item("accept-edits", "Accept Edits", "Auto-approve file edits")
        .item("yolo", "YOLO", "Accept everything (development only)")
        .interact()?;

    // Step 5: Examples
    let include_examples = template != "minimal"
        && confirm("Include example workflows?")
            .initial_value(true)
            .interact()?;

    // Step 6: Git
    let init_git = confirm("Initialize git repository?")
        .initial_value(true)
        .interact()?;

    // Execute
    let s = spinner();
    s.start("Setting up project...");

    // ... create files ...

    s.stop("Project created!");

    // Summary
    note("Next steps", format!(
        "cd {path}\n\
         nika run workflows/tier-1-no-deps/01-exec-basics.nika.yaml\n\
         \n\
         # Set up your provider\n\
         nika keys set {provider}\n\
         \n\
         # Then try LLM workflows\n\
         nika run workflows/tier-2-llm/04-infer-basics.nika.yaml"
    ))?;

    outro(format!(
        "Problems? {}",
        style("https://github.com/supernovae-st/nika/issues").cyan().underlined()
    ))?;

    Ok(())
}
```

---

## 8. Advanced Patterns to Consider

### 8.1 Template-Driven Prompts (like cargo-generate)

Define prompts in a TOML/YAML config so new project templates can add their own questions:
```toml
[[prompts]]
key = "db_type"
type = "select"
message = "Database type?"
options = ["PostgreSQL", "SQLite", "None"]
```

### 8.2 Resumable Wizard

Save wizard state to a temp file. If the user Ctrl-C's and re-runs, offer to resume:
```
  Found a previous incomplete setup. Resume? [Y/n]
```

### 8.3 Post-Init Health Check

After setup, run `nika doctor` automatically to verify everything is configured:
```
   Checking setup...
     .nika/config.toml exists
     Provider: Claude (ANTHROPIC_API_KEY set)
     30 example workflows available
     Git initialized
   Everything looks good!
```

### 8.4 Theme Customization

Let users pick a theme during init (or later in config):
```
   Choose your Nika theme:
   > Default (cyan + magenta)
     Catppuccin Mocha
     Dracula
     Nord
```

---

## Sources

1. [create-next-app source](https://github.com/vercel/next.js/tree/canary/packages/create-next-app) - Vercel's wizard implementation
2. [create-astro source](https://github.com/withastro/astro/tree/main/packages/create-astro) - Astro's premium wizard experience
3. [@clack/prompts source](https://github.com/natemoo-re/clack) - The design system powering modern CLI wizards
4. [cliclack source](https://github.com/fadeevab/cliclack) - Rust port of @clack/prompts
5. [Bubble Tea](https://github.com/charmbracelet/bubbletea) - Go TUI framework (Elm Architecture)
6. [Huh?](https://github.com/charmbracelet/huh) - Go form/prompt library with themes
7. [Gum](https://github.com/charmbracelet/gum) - Shell-accessible Bubble Tea components
8. [Lip Gloss](https://github.com/charmbracelet/lipgloss) - Go terminal styling (CSS for terminals)
9. [inquire](https://github.com/mikaelmello/inquire) - Rust interactive prompts (feature-rich)
10. [dialoguer](https://github.com/console-rs/dialoguer) - Rust prompt library (minimal)
11. [console](https://github.com/console-rs/console) - Rust terminal utilities
12. [indicatif](https://github.com/console-rs/indicatif) - Rust progress bars
13. [ratatui](https://github.com/ratatui/ratatui) - Rust full TUI framework
14. [cargo-generate](https://github.com/cargo-generate/cargo-generate) - Rust project templating

## Methodology

- Tools used: Direct source code analysis (GitHub raw files), README documentation review
- Pages analyzed: 25+ source files across 14 repositories
- Ecosystems covered: JavaScript/Node.js, Go, Rust
- Pattern analysis: Visual design, UX flow, technical implementation, capability detection

## Confidence Level

**High** - Based on direct source code analysis of all major tools. The recommendation
of cliclack is based on it being a direct port of the proven @clack/prompts design system,
with active Rust maintenance and growing adoption. The patterns documented here are
extracted from production code in tools used by millions of developers.
