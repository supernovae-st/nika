# Research Report: Terminal Color Design & CLI UX for `nika keys list`

## Summary

This report synthesizes best practices from terminal color theory, the Catppuccin palette system, Charm.sh's Lip Gloss framework, the Vercel/Stripe/GitHub CLI design patterns, progressive disclosure techniques, accessibility requirements, and interactive CLI patterns -- all specifically applied to designing a "best-in-class" key listing command for Nika. It includes concrete `colored` crate recommendations that are compatible with Nika's existing design system.

## Key Findings

### 1. The `colored` Crate in Rust: How ANSI Colors Work

The `colored` crate provides two layers of color:

**Basic 16 ANSI colors** (what Nika uses today):
```
.black()     .red()       .green()     .yellow()
.blue()      .magenta()   .cyan()      .white()
```
Each has a `.bright_*()` variant (e.g. `.bright_green()`), giving 16 total.

**Extended methods:**
```
.bold()      .dimmed()    .italic()    .underline()
.strikethrough()  .reversed()  .hidden()
```

**TrueColor (24-bit):**
```rust
use colored::Colorize;
"text".truecolor(139, 92, 246)  // Violet-500 (#8b5cf6)
"text".on_truecolor(15, 23, 42) // Slate-900 background
```

**Critical insight**: The `colored` crate respects `NO_COLOR` and `CLICOLOR_FORCE` environment variables. The `colored::control::set_override()` function forces colors on/off. Nika already handles this via `IsTerminal`.

**Nika's current palette usage** (from display code analysis):
- `dimmed` (15x) -- the workhorse for secondary text
- `bold` (11x) -- for emphasis/titles
- `green`/`red`/`yellow` (9x each) -- semantic traffic light
- `cyan` (5x) -- info/links
- `white` (3x) -- titles on dark backgrounds
- `blue` (3x) -- keys in JSON
- `magenta` (2x) -- brand accent (infer verb)

### 2. Terminal Color Palette Design: Dark Mode vs Light Mode

**The fundamental problem**: You cannot know if the user has a dark or light terminal background. The basic 16 ANSI colors are *remapped* by each terminal theme.

**Safe universal colors** (readable on both dark and light backgrounds):

| Purpose | `colored` Method | Why Safe |
|---------|-----------------|----------|
| Primary text | `.bold()` (no color) | Uses terminal's default foreground |
| Secondary text | `.dimmed()` | Fades from whatever the default is |
| Success | `.green()` | Standard green is visible on both |
| Error | `.red()` | Standard red is visible on both |
| Warning | `.yellow()` | Works on dark; borderline on light (but common convention) |
| Info/Link | `.cyan()` | Good on dark, acceptable on light |
| Accent | `.magenta()` | Good contrast on both |
| Muted structure | `.dimmed()` + box chars | Structural but not distracting |

**Dangerous colors to avoid:**
- `.white()` on light backgrounds -- invisible
- `.black()` on dark backgrounds -- invisible
- `.blue()` on dark backgrounds -- many themes make this too dark to read
- `.bright_black()` -- inconsistent across terminals (sometimes invisible)

**Best practice**: Use `.bold()` (inherits terminal fg) instead of `.white()` for primary text. Use `.dimmed()` instead of `.bright_black()` for muted text. These adapt to both light and dark.

**TrueColor exception**: If using `.truecolor()`, you control the exact color but lose theme adaptation. Only use TrueColor for decorative elements (borders, icons) where exact brand color matters and where falling back to a basic color is acceptable.

### 3. Catppuccin: The Most Popular Terminal Color Scheme

Catppuccin (github.com/catppuccin) is the dominant modern terminal palette with 4 flavors:
- **Latte** (light)
- **Frappe** (medium dark)
- **Macchiato** (dark)
- **Mocha** (darkest)

**Design principles from Catppuccin:**
1. **Warm neutrals** -- backgrounds have slight warmth, not pure gray
2. **Pastel accents** -- all accent colors are desaturated/softened, never harsh neon
3. **Consistent luminance** -- accent colors share similar brightness so they feel harmonious
4. **Semantic naming** -- `red`, `green`, `yellow`, `blue` + specialty: `mauve`, `pink`, `teal`, `sky`, `lavender`

**Key Catppuccin Mocha values** (most popular dark theme):

| Role | Name | Hex | RGB |
|------|------|-----|-----|
| Base | Base | #1e1e2e | (30, 30, 46) |
| Surface | Surface0 | #313244 | (49, 50, 68) |
| Overlay | Overlay0 | #6c7086 | (108, 112, 134) |
| Text | Text | #cdd6f4 | (205, 214, 244) |
| Subtext | Subtext0 | #a6adc8 | (166, 173, 200) |
| Red | Red | #f38ba8 | (243, 139, 168) |
| Green | Green | #a6e3a1 | (166, 227, 161) |
| Yellow | Yellow | #f9e2af | (249, 226, 175) |
| Blue | Blue | #89b4fa | (137, 180, 250) |
| Mauve | Mauve | #cba6f7 | (203, 166, 247) |
| Teal | Teal | #94e2d5 | (148, 226, 213) |
| Lavender | Lavender | #b4befe | (180, 190, 254) |

**Relevance for Nika**: These pastel values inspire the "Cosmic" palette Nika's TUI already uses (Violet-400 `#a78bfa` is very close to Catppuccin's Mauve `#cba6f7`). For CLI output using the basic 16-color `colored` crate, you *cannot* use these exact colors -- they depend on the terminal theme. But you can use TrueColor for specific decorative elements.

### 4. CLI UX Design: Progressive Disclosure

**The principle**: Show the minimum needed information by default. Let users opt into detail.

**Three-tier progressive disclosure model** (used by best CLIs):

| Tier | Flag | What shows | When |
|------|------|-----------|------|
| Compact | (default) | Status icons + names only | Quick glance |
| Normal | (default) | Status + key source + models | Daily use |
| Verbose | `--verbose` / `-v` | Full env var names, key prefixes, test results | Debugging |

**Patterns from industry leaders:**

**Vercel CLI** (`vercel ls`):
- Clean table with aligned columns
- Status dots (green/yellow/red) as first column
- Dimmed metadata (age, URL) as last columns
- Empty state: friendly message + command to fix

**Stripe CLI** (`stripe resources`):
- Grouped by category with section headers
- Count in parentheses after header: `Payments (12)`
- Tree connectors for hierarchy
- Dimmed descriptions after names

**GitHub CLI** (`gh auth status`):
- One line per account with checkmark/cross
- Source in parentheses: `(token)`, `(oauth)`, `(ssh)`
- Scopes listed with dimmed comma separation
- Clear "Logged in to" phrasing (human, not technical)

**Key insight from all three**: They never use more than 3-4 colors simultaneously. The palette is *restrained*. Color carries meaning, not decoration.

### 5. Terminal Output Accessibility & Contrast

**WCAG 2.1 for terminals** (adapted):
- Minimum contrast ratio 4.5:1 for normal text, 3:1 for large/bold text
- Never use color as the ONLY indicator -- always pair with icon or text label
- Support `NO_COLOR` environment variable (colored crate does this automatically)

**Practical rules:**
1. **Green checkmark + "configured"** (not just green dot)
2. **Red cross + "missing"** (not just red text)
3. **Dimmed text must still be legible** -- test on low-contrast monitors
4. **Bold carries weight without color** -- accessible to colorblind users
5. **Icons must have text alternatives in `--no-color` mode**

**Color blindness considerations:**
- 8% of men have red-green color blindness
- Never distinguish states ONLY by red vs green
- Nika already handles this: `StatusIcon::Ok` = checkmark + green, `StatusIcon::Fail` = cross + red (icon shape provides redundancy)

### 6. Charm.sh Lip Gloss: Design Philosophy

Charm.sh (creators of bubbletea, lip gloss, glow) pioneered the "beautiful terminal" movement in Go. Key design principles:

**Layout primitives:**
- `lipgloss.Place()` -- absolute positioning in terminal
- Borders (rounded, thick, double, hidden)
- Padding and margins (measured in terminal cells)
- Horizontal/vertical joining of styled blocks

**Color handling:**
```
lipgloss.Color("205")           // ANSI 256
lipgloss.Color("#FF5733")       // TrueColor
lipgloss.AdaptiveColor{Light: "236", Dark: "248"}  // Theme-aware!
lipgloss.CompleteColor{TrueColor: "#FF5733", ANSI256: "205", ANSI: "1"}
```

**The AdaptiveColor pattern**: Lip Gloss queries the terminal's background color (via `COLORFGBG` env var or OSC 11 escape sequence) and picks a color variant. This is the gold standard but complex to implement in Rust.

**Nika equivalent**: Stick to the basic 16 ANSI colors from `colored` (which adapt automatically via terminal themes) and use `.bold()` / `.dimmed()` for emphasis hierarchy. Reserve TrueColor for the TUI (ratatui), not CLI output.

**Lip Gloss design patterns that transfer to `colored` in Rust:**
1. **Consistent padding**: Always 2-space left margin (`"  "`)
2. **Section borders**: Box-drawing characters (Nika already uses `╭╰├└`)
3. **Information density**: Aligned columns, no wasted vertical space
4. **Color as hierarchy**: Bold white > Normal > Dimmed (3 levels)

### 7. Box Drawing & Unicode Patterns for Modern Terminals

**Nika's current box drawing** (from `cli_format.rs`):
```
╭─────────────╮    Rounded corners (U+256D-U+2570)
│  Content    │    Vertical bars (U+2502)
├─────────────┤    Horizontal separator (U+251C + U+2524)
│  More       │
╰─────────────╯
```

**Tree connectors** (already in Nika):
```
├── branch item     (U+251C + U+2500)
└── last item       (U+2514 + U+2500)
│   continuation    (U+2502)
```

**Additional useful box-drawing characters:**

| Character | Unicode | Name | Use |
|-----------|---------|------|-----|
| `─` | U+2500 | Horizontal | Separators |
| `│` | U+2502 | Vertical | Tree pipes |
| `┆` | U+2506 | Light triple dash vertical | Subtle separator |
| `╌` | U+254C | Light double dash horizontal | Light separator |
| `▪` | U+25AA | Black small square | Bullet points |
| `▸` | U+25B8 | Right-pointing small triangle | Expandable items |
| `◆` | U+25C6 | Black diamond | Configured item |
| `◇` | U+25C7 | White diamond | Unconfigured item |
| `●` | U+25CF | Black circle | Active/running |
| `○` | U+25CB | White circle | Inactive |
| `⏺` | U+23FA | Record button | Vault source |
| `🔑` | U+1F511 | Key | AVOID (wide emoji, alignment issues) |

**Rule**: Never use emoji in CLI output. They have unpredictable widths (1 or 2 cells depending on terminal). Stick to Unicode symbols from the Geometric Shapes, Box Drawing, and Miscellaneous Symbols blocks.

### 8. Best Developer CLI Onboarding: Empty States

The best CLIs turn "nothing configured" into a guided experience.

**Pattern from Vercel/Railway/Fly.io:**
```
  No providers configured yet.

  Get started:
    1. Get an API key from https://console.anthropic.com
    2. Run: nika keys set anthropic

  Or try without an API key:
    nika run workflow.nika.yaml --provider mock
```

**Anti-pattern** (what most CLIs do):
```
Error: No providers configured
```

**Design principles for empty states:**
1. **Acknowledge the state warmly** -- "No keys configured yet" (not "Error: no keys")
2. **Provide the next action** -- exact command to run
3. **Offer the easiest path** -- link to API key page
4. **Show an alternative** -- mock provider for testing
5. **Match the brand voice** -- Nika's "cosmic" personality

### 9. Interactive CLI Patterns (cliclack, dialoguer)

Nika already uses `cliclack` for interactive prompts. Key patterns:

**cliclack visual style:**
```
◇  Which provider?
│  ○ anthropic  Claude -- recommended for reasoning & code
│  ● openai     GPT-4o, GPT-4.1, o3, o4-mini
│  ○ mistral    Mistral Large, Small, Codestral
└
```

**When to use interactive vs static:**
- `nika keys list` should be STATIC (non-interactive) -- it's a read command
- `nika keys set` should be INTERACTIVE -- it needs user input
- Use `--json` flag for machine-readable output (piping to jq)

### 10. Concrete Design Recommendation for `nika keys list`

Based on all research, here is the recommended design.

---

## The Design: `nika keys list`

### Default Output (Normal Mode)

```
  Keys                                           3/7 configured
  ──────────────────────────────────────────────────────────────

  LLM Providers

  ├── ✓ anthropic     sk-ant-***Dk   vault    claude-sonnet-4-6, claude-haiku-4-5
  ├── ✓ openai        sk-proj-***9f  env      gpt-4.1, gpt-4o, o4-mini
  ├── ✗ mistral                               nika keys set mistral
  ├── ✓ groq          gsk_***2w      vault    llama-3.3-70b, mixtral-8x7b
  ├── ✗ deepseek                              nika keys set deepseek
  ├── ✗ gemini                                nika keys set gemini
  └── ✗ xai                                   nika keys set xai

  Always Available
  ├── ◆ mock          deterministic test responses
  └── ◇ native        nika model pull <name>

  Add a key     nika keys set <name>
  Test a key    nika provider test <name>
```

### Verbose Output (`--verbose`)

```
  Keys                                           3/7 configured
  ──────────────────────────────────────────────────────────────

  LLM Providers

  ├── ✓ anthropic     sk-ant-api03-***Dk   vault    ANTHROPIC_API_KEY
  │                   Models: claude-opus-4, claude-sonnet-4-6, claude-haiku-4-5
  │                   Features: vision, structured output, extended thinking
  │
  ├── ✓ openai        sk-proj-***9f        env      OPENAI_API_KEY
  │                   Models: gpt-4.1, gpt-4o, o3, o4-mini
  │                   Features: vision, structured output, function calling
  │                   Warning: env var lost on reboot -- use vault instead
  │
  ├── ✗ mistral                                     MISTRAL_API_KEY
  │                   Models: mistral-large, mistral-small, codestral
  │                   Get key: https://console.mistral.ai/api-keys
  │
  ...
```

### Empty State (No Keys)

```
  Keys                                           0/7 configured
  ──────────────────────────────────────────────────────────────

  No API keys configured yet.

  Get started in 30 seconds:

    1. Get a free API key:
       Anthropic   https://console.anthropic.com/settings/keys
       OpenAI      https://platform.openai.com/api-keys
       Groq        https://console.groq.com/keys        (free tier)

    2. Store it securely:
       nika keys set anthropic

    3. Try it:
       nika infer "Hello, world!"

  Or explore without an API key:
    nika run my-workflow.nika.yaml --provider mock
```

### All Configured (Happy State)

```
  Keys                                           7/7 all configured
  ──────────────────────────────────────────────────────────────────

  LLM Providers

  ├── ✓ anthropic     sk-ant-***Dk   vault    claude-sonnet-4-6, claude-haiku-4-5
  ├── ✓ openai        sk-proj-***9f  vault    gpt-4.1, gpt-4o, o4-mini
  ├── ✓ mistral       ***4a2         vault    mistral-large, mistral-small
  ├── ✓ groq          gsk_***2w      vault    llama-3.3-70b, mixtral-8x7b
  ├── ✓ deepseek      sk-***8f       vault    deepseek-chat, deepseek-reasoner
  ├── ✓ gemini        AI***Bg        vault    gemini-2.5-pro, gemini-2.5-flash
  └── ✓ xai           xai-***3n      vault    grok-3, grok-3-mini

  All providers ready. You have access to 20+ models.
```

---

## Color Mapping for Implementation

### Semantic Color Assignment

| Element | `colored` Method | ANSI Code | Rationale |
|---------|-----------------|-----------|-----------|
| Section title ("Keys") | `.bold()` | ESC[1m | Maximum weight, adapts to theme |
| Count (3/7) | `.green()` / `.yellow()` / `.red()` | 32/33/31 | Traffic light for health |
| Separator line | `.dimmed()` | ESC[2m | Structural, not distracting |
| Tree connectors | `.dimmed()` | ESC[2m | Infrastructure, fade into background |
| ✓ configured icon | `.green().bold()` | ESC[1;32m | Strong positive signal |
| ✗ missing icon | `.red().bold()` | ESC[1;31m | Strong negative signal |
| Provider name (configured) | `.bold()` | ESC[1m | Primary information |
| Provider name (missing) | (no style) | (default) | Lower visual weight |
| Masked key | `.dimmed()` | ESC[2m | Present but not primary |
| Source (vault/env/daemon) | `.dimmed()` | ESC[2m | Metadata |
| Model list | `.dimmed()` | ESC[2m | Secondary detail |
| Action hint | `.dimmed()` | ESC[2m | Guiding but not commanding |
| "env" warning | `.yellow()` | ESC[33m | Attention needed |
| ◆ always available | `.cyan()` | ESC[36m | Distinct from LLM providers |
| Section subtitle | `.dimmed()` | ESC[2m | De-emphasized metadata |
| URLs (verbose) | `.cyan().underline()` | ESC[4;36m | Clickable-looking links |

### Color Budget

**Rule of 4**: Never use more than 4 distinct colors in a single view.

For `nika keys list`:
1. **Green** -- configured/success
2. **Red** -- missing/error
3. **Yellow** -- warning (env var)
4. **Dimmed** -- everything structural

Bold is used for emphasis hierarchy, not as a "color."

### Bold/Dimmed/Normal Hierarchy

```
LEVEL 1: .bold()           Title, provider names, count          LOUDEST
LEVEL 2: (no style)        Status text, descriptions             NORMAL
LEVEL 3: .dimmed()         Separators, hints, metadata           QUIETEST
```

This three-level hierarchy is the most important design decision. It creates visual depth without color noise.

## Box Drawing Reference

### Recommended character set for `nika keys list`:

```
├── branch connector (tree_connector(false))     U+251C U+2500 U+2500
└── last connector   (tree_connector(true))      U+2514 U+2500 U+2500
│   pipe for continuation                        U+2502
─   horizontal separator                         U+2500
```

### Characters to consider adding:

```
◆  filled diamond  -- "always available" items    U+25C6
◇  empty diamond   -- "needs setup" items         U+25C7
·  middle dot      -- column separator             U+00B7
```

## Implementation Recommendations

### 1. Column Alignment

Use fixed column widths for professional alignment:

```rust
// Provider column: 12 chars (longest: "anthropic" = 9)
// Key column: 16 chars (masked key)
// Source column: 8 chars (vault/env/daemon)
// Models: remainder

println!(
    "  {} {} {:<12} {:<16} {:<8} {}",
    connector.dimmed(),
    icon,
    name.bold(),
    masked_key.dimmed(),
    source.dimmed(),
    models.dimmed(),
);
```

### 2. Separator Width

Match separator to terminal width (capped at 70):
```rust
let width = terminal_width().min(70);
println!("  {}", "─".repeat(width - 2).dimmed());
```

### 3. Empty State Detection

```rust
if configured == 0 {
    // Show welcoming empty state with onboarding steps
} else if configured == total {
    // Show celebratory "all ready" message
} else {
    // Show normal list with missing items having action hints
}
```

### 4. JSON Output (`--json`)

Always support machine-readable output:
```json
{
  "providers": [
    {
      "name": "anthropic",
      "configured": true,
      "source": "vault",
      "models": ["claude-sonnet-4-6", "claude-haiku-4-5"]
    }
  ],
  "configured": 3,
  "total": 7
}
```

### 5. Respect NO_COLOR

The `colored` crate handles `NO_COLOR` automatically. But also:
- Icons must still make sense without color (✓ vs ✗)
- Column alignment must not depend on ANSI codes
- Use `stripped_len()` (already in Nika) for width calculations

## Anti-Patterns to Avoid

1. **Color soup** -- Using 6+ colors makes output look like a toy
2. **Emoji in columns** -- Unpredictable widths break alignment
3. **Dense tables** -- CLI is not a spreadsheet; breathing room matters
4. **Technical jargon first** -- "ANTHROPIC_API_KEY not found" vs "anthropic not configured"
5. **Hiding actionable info** -- Always show the command to fix a missing key
6. **Inconsistent indentation** -- Every line must align to the same grid
7. **`.white()` for text** -- Invisible on light terminals. Use `.bold()` instead
8. **`.blue()` for important text** -- Too dark on many dark themes. Use `.cyan()` instead

## Sources & References

1. **colored crate** -- https://docs.rs/colored -- Nika's primary color library
2. **Catppuccin palette** -- https://github.com/catppuccin/catppuccin -- Design inspiration
3. **Charm.sh Lip Gloss** -- https://github.com/charmbracelet/lipgloss -- Layout & color philosophy
4. **Vercel CLI** -- https://vercel.com/docs/cli -- Progressive disclosure reference
5. **Stripe CLI** -- https://github.com/stripe/stripe-cli -- Section grouping patterns
6. **GitHub CLI** -- https://github.com/cli/cli -- Auth status display reference
7. **NO_COLOR standard** -- https://no-color.org -- Accessibility standard
8. **WCAG 2.1 contrast** -- https://www.w3.org/WAI/WCAG21/Understanding/contrast-minimum
9. **Unicode box drawing** -- https://en.wikipedia.org/wiki/Box-drawing_character
10. **cliclack** -- https://github.com/fadeevab/cliclack -- Interactive CLI patterns for Rust

## Methodology

- Analyzed Nika's existing display system (6 source files, ~950 lines)
- Counted color usage frequency across `nika-display` crate
- Reviewed Nika's Tailwind color palette (132 colors, TUI-only)
- Cross-referenced Catppuccin Mocha, Dracula, Nord, and Tokyo Night palettes
- Studied Vercel CLI, Stripe CLI, GitHub CLI, Railway CLI, Fly.io CLI output patterns
- Reviewed WCAG 2.1 contrast requirements adapted for terminal use
- Analyzed Unicode box-drawing character compatibility across major terminals

## Confidence Level

**High** -- The recommendations are grounded in Nika's existing design system (`cli_format.rs`, `icons.rs`, `colors.rs`), industry-standard CLI patterns, and accessibility requirements. The color choices use only proven-safe ANSI methods from the `colored` crate.
