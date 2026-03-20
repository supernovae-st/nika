# Research Report: Evangelion x Cosmic x Hacker TUI Aesthetic

**Date**: 2026-03-20
**Purpose**: Visual design reference for Nika TUI redesign
**Methodology**: Analysis of Evangelion UI design, terminal UI showcase projects, anime-inspired interfaces, and cosmic color theory
**Confidence**: High (well-documented design domain with extensive community analysis)

---

## 1. Evangelion NERV Interface Design

### What Makes NERV HQ UI Iconic

The NERV headquarters interface (designed by Ikuto Yamashita and the Gainax/Khara art team) is one of the most referenced sci-fi UI designs in history. Its power comes from **information density married to emotional tension**.

### Core Design Principles

1. **Layered information hierarchy** -- Multiple overlapping data panels at different opacities, creating depth
2. **Asymmetric layouts** -- Panels are never evenly distributed; heavy weighting to one side creates urgency
3. **Persistent status indicators** -- Sync rates, power levels, neural connections always visible
4. **Warning escalation through color** -- Green (normal) -> Yellow (caution) -> Orange (alert) -> Red (critical) -> Flashing red (catastrophic)
5. **Japanese + English bilingual labels** -- Technical terms in English, status/warnings in Japanese kanji, creating a "foreign technology" feel
6. **Monospaced data, proportional labels** -- Raw telemetry in monospace; UI chrome labels in a clean sans-serif

### NERV Color Palette

| Role | Color | Hex | Usage |
|------|-------|-----|-------|
| Background (deep) | Near-black blue | `#0A0E1A` | Primary background |
| Background (panel) | Dark navy | `#111827` | Panel backgrounds |
| Grid lines | Dim blue-gray | `#1E2A3A` | Subtle grid overlays |
| Primary text | Amber/Orange | `#FF8C00` | Main readouts, active data |
| Warning text | Bright orange | `#FF6B00` | Alerts, sync warnings |
| Critical/danger | NERV red | `#CC0000` | AT Field breach, angel detection |
| Critical flash | Bright red | `#FF0000` | Emergency, self-destruct |
| Success/nominal | Muted green | `#00CC66` | Systems nominal, sync stable |
| Accent/highlight | Electric cyan | `#00D4FF` | Selected items, cursor |
| Secondary text | Steel blue | `#7B8DA4` | Labels, inactive panels |
| NERV logo accent | Blood red on black | `#8B0000` | Organizational identity |
| Entry plug status | Purple | `#6B21A8` | Eva unit connection |

### Typography Patterns

- **Primary display font**: Matisse EB (used in the actual anime) -- a high-contrast Japanese serif with sharp terminals
- **Technical readouts**: Monospaced, slightly condensed (think OCR-A or Inconsolata vibes)
- **TUI equivalent**: Use your terminal's monospace font but vary **weight** (bold for active, dim for background) and **color** to create hierarchy
- **ALL CAPS for system status** -- "PATTERN BLUE DETECTED", "SYNCHRONIZATION RATE: 41.3%"
- **Mixed case for descriptions** -- "Entry plug inserted", "Initiating contact"

### Layout Patterns (Translatable to TUI)

```
+--[NERV]--+---[STATUS]----+--[ALERT]--+
|           |               |           |
| Main      | Center        | Side      |
| Readout   | Viewport      | Panels    |
| (tall)    | (dominant)    | (narrow)  |
|           |               |           |
+-----------+--[TELEMETRY]--+-----------+
| Bottom bar: scrolling status messages  |
+----------------------------------------+
```

Key characteristics:
- The **center viewport** dominates (60%+ of screen)
- Side panels are **narrow and dense** with scrolling data
- Bottom ticker with scrolling status/log messages
- Top bar shows org identity + global status
- Panels have **visible borders with labeled headers**
- Corner decorations: small triangles or brackets at panel corners

### Warning System Design

The NERV warning system is a masterclass in escalation UX:

1. **Level 0 (Normal)**: All text amber/orange on dark. Calm.
2. **Level 1 (Caution)**: Yellow highlight on specific panels. Soft pulse.
3. **Level 2 (Alert)**: Orange bars appear. "WARNING" in English. Panel borders brighten.
4. **Level 3 (Danger)**: Red text replaces orange. Panels flash. Japanese kanji warnings.
5. **Level 4 (Critical)**: Screen-wide red overlay. Alternating red/black. Alarm sound.
6. **Level 5 (Catastrophic)**: Full red screen. Giant text. Everything else dims.

For a TUI, this translates to:
- Normal: `fg: #FF8C00` on `bg: #0A0E1A`
- Warning: Border color changes to `#FFD700`, header becomes bold
- Error: Text goes `#FF0000`, border flashes (toggle bold/dim per frame)
- Critical: Background changes to `#1A0000`, all text red

### Iconic Visual Elements

- **Hexagonal patterns** in backgrounds (angel detection grids)
- **Concentric circles** for targeting/sync displays
- **Progress bars with percentage** -- always showing numbers, never just a bar
- **Countdown timers** prominently displayed
- **Scan lines** -- subtle horizontal lines creating CRT feel
- **"SOUND ONLY"** panels -- when video unavailable, just a labeled box

---

## 2. Best Terminal UI Designs (2024-2026)

### Ratatui Ecosystem (Rust)

**Ratatui** (fork of tui-rs) is the dominant Rust TUI framework. Notable showcase apps:

#### gitui (extrawurst/gitui)
- Clean panel layout with git diff highlighting
- Tab-based navigation with vim keys
- Color-coded diff view (green adds, red removes)
- Demonstrates: **multi-panel layout with contextual detail**

#### bottom (ClementTsang/bottom)
- System monitor with graphs, sparklines, CPU/memory charts
- Multiple widget types in a dashboard grid
- Color-coded process list
- Demonstrates: **cockpit/dashboard aesthetic**, real-time data

#### lazygit (jesseduffield/lazygit)
- Stacked panel layout
- Contextual popups and modals
- Clean status bar with keybinding hints
- Demonstrates: **information density without clutter**

#### bandwhich (imsnif/bandwhich)
- Network monitor with live bandwidth graphs
- Table + chart combo layout
- Demonstrates: **real-time telemetry display**

#### atuin (atuinsh/atuin)
- Shell history search with fuzzy matching
- Clean, fast search UI
- Demonstrates: **search-first interfaces**

#### harlequin (tconbeer/harlequin)
- SQL IDE in terminal
- Multi-pane: query editor, results table, schema browser
- Demonstrates: **IDE-like layout complexity in TUI**

### Bubbletea Ecosystem (Go)

#### charm.sh suite (Charm)
- Glow, Soft Serve, VHS -- all with beautiful styled output
- Lip Gloss styling library for borders, padding, colors
- Demonstrates: **polished, branded TUI aesthetics**

#### superfile (MHNightCat/superfile)
- File manager with preview pane
- Nerd Font icons throughout
- Multiple themes including cyberpunk-inspired
- Demonstrates: **icon-rich, modern TUI design**

### Cockpit/Dashboard Aesthetic Leaders

#### WTF (wtfutil/wtf)
- Personal dashboard with configurable modules
- Grid layout with independent refresh rates per widget
- Demonstrates: **mission control / cockpit layout**

#### Sampler (sqshq/sampler)
- Real-time metrics dashboard
- Sparklines, gauges, bar charts, ASCII art
- YAML-configured (relevant to Nika)
- Demonstrates: **telemetry dashboard** -- closest to NERV aesthetic

#### Grafana Terminal (not official, community)
- Various projects bringing Grafana-like dashboards to terminal
- Demonstrates: **metrics visualization in TUI**

### Design Patterns from Best TUIs

1. **Status bar at bottom** -- always show mode, keybindings, status
2. **Breadcrumb navigation** -- show where you are in the hierarchy
3. **Contextual help** -- `?` key overlay with available commands
4. **Progressive disclosure** -- summary view -> detail on select
5. **Sparklines for trends** -- small inline charts using Unicode block chars
6. **Dimmed borders** -- panels separated by low-contrast borders, not bright lines
7. **Focus indicators** -- active panel has brighter border / colored header
8. **Unicode box drawing** -- `─ │ ┌ ┐ └ ┘ ├ ┤ ┬ ┴ ┼` for clean panels
9. **Braille dot patterns** -- `⠁⠂⠃⠄⠅⠆⠇⠈` for high-res graphs in terminal

---

## 3. Anime-Inspired Terminal Interfaces

### Ghost in the Shell

The Standalone Complex and Arise series feature iconic terminal/hacking UIs:
- **Green-on-black** with kanji overlays
- Cascading data streams (precursor to Matrix rain)
- Circular menu systems for cyberspace navigation
- Network topology visualizations with node-link diagrams

Color palette:
| Role | Hex |
|------|-----|
| Primary | `#00FF41` (hacker green) |
| Background | `#000000` |
| Accent | `#00BFFF` |
| Warning | `#FF4500` |

### The Matrix

- Classic: `#00FF41` on `#000000` (the "Matrix green")
- Modern interpretations add depth: `#003B00` for dim text, `#00FF41` for bright
- **cmatrix** -- classic terminal screensaver
- **neo** (github.com/st3w/neo) -- Matrix rain with customization
- **unimatrix** -- more advanced Matrix effect with varied characters

### Cyberpunk 2077 / Edgerunners

Cyberpunk terminal aesthetic:
| Role | Hex |
|------|-----|
| Background | `#0D0221` (deep purple-black) |
| Primary | `#FF2A6D` (hot pink) |
| Secondary | `#05D9E8` (electric cyan) |
| Accent | `#D1F7FF` (ice white) |
| Warning | `#FF6B00` |

### Serial Experiments Lain

- Pioneered the "lonely terminal" aesthetic
- Brown/amber monochrome on dark
- Sparse, centered text with large margins
- Error messages as narrative

### Psycho-Pass (Sibyl System)

- Clean, corporate-dystopian UI
- Blue-white primary with red threat indicators
- Circular HUDs and radial menus
- Threat coefficient displayed as number with color scale

### Notable Projects

#### cool-retro-term
- CRT terminal emulator with scanlines, bloom, curvature
- Customizable phosphor colors (amber, green, white)
- Demonstrates: **nostalgic display effects**

#### edex-ui (GitSquared/edex-ui)
- Full-screen sci-fi terminal
- Network map, system stats, file browser, terminal -- all in one
- Heavy TRON/sci-fi aesthetic
- **Most relevant reference** for your aesthetic goals
- Color scheme: cyan/teal on dark with orange accents

#### blessed-contrib (yaronn/blessed-contrib)
- Node.js TUI dashboard widgets
- Maps, donuts, sparklines, log viewers
- "Mission control" aesthetic

#### Hack the Planet projects
- Various "Hollywood hacking" terminal UIs
- genact -- pretend to be busy
- Hollywood -- fill terminal with fake hacking

### Anime Terminal Themes (Community)

Several Evangelion-specific themes exist:
- **evangelion.vim** -- Vim colorscheme based on Eva Unit-01 (purple/green)
- **Tokyo Night** -- Not anime-specific but inspired by Tokyo cityscape; extremely popular for IDEs and terminals
- **Catppuccin** -- Pastel theme with warm tones; has been adapted with Eva colors
- **eva-01-theme** -- VS Code theme matching Unit-01 colors

---

## 4. Blue-Violet + Orange Color Palettes for Dark UIs

### The Science Behind This Combination

Blue-violet and orange are **complementary colors** (opposite on the color wheel). This creates:
- Maximum visual contrast
- Natural focal point hierarchy (warm orange pops on cool background)
- Cosmic association (nebulae, space photography uses these exact tones)
- Emotional tension (cool calm vs warm urgency)

### Cosmic / Space Theme Palette

#### Primary Palette: "Supernova"

| Role | Name | Hex | RGB | Usage |
|------|------|-----|-----|-------|
| bg-deep | Void | `#06060F` | 6,6,15 | Deepest background |
| bg-base | Deep Space | `#0C0E1A` | 12,14,26 | Main background |
| bg-surface | Nebula Dark | `#141829` | 20,24,41 | Panel backgrounds |
| bg-overlay | Cosmic Dust | `#1C2038` | 28,32,56 | Popups, overlays |
| border-dim | Starfield | `#2A2E4A` | 42,46,74 | Inactive borders |
| border-active | Constellation | `#4A3F8F` | 74,63,143 | Active panel borders |
| text-muted | Distant Star | `#5B6178` | 91,97,120 | Disabled, comments |
| text-secondary | Moonlight | `#8B92A8` | 139,146,168 | Labels, secondary |
| text-primary | Starlight | `#C8CDE0` | 200,205,224 | Primary readable text |
| text-bright | White Dwarf | `#E8ECF5` | 232,236,245 | Headings, emphasis |
| accent-violet | Nebula Core | `#7C5CFC` | 124,92,252 | Primary accent |
| accent-blue | Event Horizon | `#4B6EF5` | 75,110,245 | Links, info |
| accent-indigo | Warp Drive | `#6366F1` | 99,102,241 | Selected state |
| accent-orange | Solar Flare | `#FF8C00` | 255,140,0 | Primary CTA, active data |
| accent-amber | Supernova | `#FFAA2A` | 255,170,42 | Highlights, values |
| accent-red | Red Giant | `#EF4444` | 239,68,68 | Errors, critical |
| accent-green | Aurora | `#22C55E` | 34,197,94 | Success, nominal |
| accent-cyan | Pulsar | `#06B6D4` | 6,182,212 | Info, telemetry |
| accent-pink | Quasar | `#EC4899` | 236,72,153 | Special, rare |

#### Evangelion-Specific Overlay

For NERV-mode or alert states, shift the palette warmer:

| Role | Name | Hex | Usage |
|------|------|-----|-------|
| eva-bg | NERV Dark | `#0A0808` | Alert background (warm black) |
| eva-primary | Entry Plug | `#FF8C00` | Main text during alerts |
| eva-warning | AT Field | `#FF6B00` | Warning state |
| eva-danger | Impact | `#CC0000` | Critical alerts |
| eva-unit01-purple | Unit-01 | `#6B21A8` | Eva unit / process identity |
| eva-unit01-green | Berserk | `#22C55E` | Active / running state |
| eva-unit02-red | Unit-02 | `#DC2626` | Secondary process |
| eva-unit00-blue | Unit-00 | `#2563EB` | Tertiary process |

### Color Application Rules

```
HIERARCHY (most to least prominent):
1. Orange (#FF8C00)  -- Active data, current values, user focus
2. Violet (#7C5CFC)  -- Structural elements, active borders, selections
3. Cyan (#06B6D4)    -- Informational, telemetry, metadata
4. White (#E8ECF5)   -- Headings, titles
5. Light gray (#C8CDE0) -- Body text
6. Medium gray (#8B92A8) -- Labels, secondary info
7. Dark gray (#5B6178)  -- Disabled, background info
8. Border (#2A2E4A)  -- Panel separation

SEMANTIC COLORS:
- Green (#22C55E) = Success, nominal, running
- Orange (#FF8C00) = Active, current, attention
- Red (#EF4444) = Error, failed, critical
- Amber (#FFAA2A) = Warning, caution, pending
- Cyan (#06B6D4) = Info, metadata, telemetry
- Violet (#7C5CFC) = Special, selected, UI chrome
```

### Existing Palettes for Reference

#### Dracula (close family)
- Background: `#282A36`, Purple: `#BD93F9`, Orange: `#FFB86C`
- Shares the violet + orange DNA but lighter/more pastel

#### Tokyo Night (close family)
- Background: `#1A1B26`, Purple: `#BB9AF7`, Blue: `#7AA2F7`
- Excellent blue-violet base, lacks strong orange

#### Kanagawa (close family)
- Inspired by Hokusai's Great Wave
- Background: `#1F1F28`, Old white: `#DCD7BA`, Wave blue: `#7E9CD8`
- Japanese aesthetic alignment

#### Suggested Hybrid: "NERV Cosmic"

Take Tokyo Night's depth + Dracula's orange + Evangelion's urgency:

```
bg:           #0C0E1A  (deeper than Tokyo Night)
surface:      #141829
border:       #2A2E4A
text:         #C8CDE0
text-bright:  #E8ECF5
violet:       #7C5CFC  (brighter than both)
orange:       #FF8C00  (more saturated than Dracula)
cyan:         #06B6D4
red:          #EF4444
green:        #22C55E
```

---

## 5. Synthesis: Design System for Nika TUI

### Recommended Approach

Combine these four research streams into a cohesive design:

#### Layout: NERV Mission Control

```
┌─[NIKA]─────────────────────────────────────────────────────────┐
│ ▸ Studio  │  Runner  │  Chat  │  Settings          ⬡ NOMINAL  │
├───────────┼──────────────────────────────────┼─────────────────┤
│           │                                  │ ┌─ TELEMETRY ─┐ │
│  WORKFLOW │     MAIN VIEWPORT                │ │ Steps: 4/12 │ │
│  TREE     │                                  │ │ Time: 1.2s  │ │
│           │     (step detail, logs,          │ │ Mem: 42MB   │ │
│  ▸ fetch  │      chat, editor)               │ │ Tokens: 1.2k│ │
│    infer  │                                  │ │ ████░░ 67%  │ │
│    exec   │                                  │ └─────────────┘ │
│           │                                  │ ┌─ STATUS ────┐ │
│           │                                  │ │ ● Provider OK│ │
│           │                                  │ │ ● MCP: 3/3  │ │
│           │                                  │ │ ○ Queue: 0  │ │
│           │                                  │ └─────────────┘ │
├───────────┴──────────────────────────────────┴─────────────────┤
│ [LOG] 14:32:01 fetch:api-call ✓ 200 OK (340ms)    ▸ RUNNING  │
└────────────────────────────────────────────────────────────────┘
```

#### Color Mapping to Nika Concepts

| Nika Concept | Color | Hex | Reason |
|--------------|-------|-----|--------|
| Active step / current verb | Solar Flare (orange) | `#FF8C00` | NERV primary readout |
| Step success | Aurora (green) | `#22C55E` | Universal success |
| Step failure | Red Giant (red) | `#EF4444` | NERV critical |
| Step pending | Moonlight (gray) | `#8B92A8` | Inactive |
| Step running | Pulsar (cyan) | `#06B6D4` | Active telemetry |
| Panel borders (inactive) | Starfield | `#2A2E4A` | Recedes |
| Panel borders (focused) | Nebula Core (violet) | `#7C5CFC` | Selection accent |
| Verb: infer | Nebula Core (violet) | `#7C5CFC` | AI = cosmic |
| Verb: fetch | Pulsar (cyan) | `#06B6D4` | Network = data |
| Verb: exec | Aurora (green) | `#22C55E` | Shell = system |
| Verb: invoke | Accent blue | `#4B6EF5` | MCP = connection |
| Verb: agent | Quasar (pink) | `#EC4899` | Special, autonomous |
| Workflow title | White Dwarf | `#E8ECF5` | Prominent |
| Timestamps | Distant Star | `#5B6178` | Background info |
| Log messages | Starlight | `#C8CDE0` | Readable |
| Keybinding hints | Supernova (amber) | `#FFAA2A` | Discoverable |

#### Animation / Transition Patterns

From Evangelion and best TUIs:

1. **Spinner for running steps** -- Use Braille spinner chars: `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏`
2. **Progress with percentage** -- Always show `████░░░░ 43%` never just a bar
3. **Warning flash** -- Toggle border between `#EF4444` and `#2A2E4A` at 2Hz
4. **Success pulse** -- Brief green flash on border, then return to normal
5. **Scrolling log ticker** -- Bottom bar scrolls latest log entries left-to-right
6. **Focus transition** -- When switching panels, new panel border brightens over 2 frames
7. **Startup sequence** -- Panels appear one by one (top-left to bottom-right) like NERV boot sequence

#### Typography Rules (Terminal Constraints)

Since we are in a terminal (monospace only):

- **UPPERCASE** for panel headers: `─ TELEMETRY ─`
- **Bold** (via ANSI) for active items, current step
- **Dim** (via ANSI) for timestamps, metadata, inactive panels
- **Italic** (via ANSI, if terminal supports) for descriptions, help text
- **Underline** -- avoid (looks bad in most terminals)
- **Nerd Font icons** for verbs: `󰖟` (fetch), `󰧑` (infer), `󰆍` (exec), `󰌷` (invoke), `󰚩` (agent)
- **Unicode symbols** for status: `●` (active), `○` (inactive), `✓` (done), `✗` (failed), `⠿` (running)

#### Border Styles

```
Standard panel (inactive):
┌─ PANEL NAME ──────────────────┐
│                                │
└────────────────────────────────┘

Focused panel (active):  [border color: #7C5CFC]
╔═ PANEL NAME ══════════════════╗
║                                ║
╚════════════════════════════════╝

Alert panel:  [border color: #EF4444]
┏━ WARNING ━━━━━━━━━━━━━━━━━━━━┓
┃                                ┃
┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛

Alternatively, keep single-line borders but change COLOR:
- Inactive: #2A2E4A (barely visible)
- Active: #7C5CFC (violet glow)
- Warning: #FF8C00 (amber)
- Error: #EF4444 (red)
```

### Ratatui Implementation Notes

In ratatui, the color system maps directly:

```rust
use ratatui::style::Color;

// Cosmic palette as ratatui Colors
const BG_DEEP: Color = Color::Rgb(6, 6, 15);
const BG_BASE: Color = Color::Rgb(12, 14, 26);
const BG_SURFACE: Color = Color::Rgb(20, 24, 41);
const BG_OVERLAY: Color = Color::Rgb(28, 32, 56);
const BORDER_DIM: Color = Color::Rgb(42, 46, 74);
const BORDER_ACTIVE: Color = Color::Rgb(74, 63, 143);
const TEXT_MUTED: Color = Color::Rgb(91, 97, 120);
const TEXT_SECONDARY: Color = Color::Rgb(139, 146, 168);
const TEXT_PRIMARY: Color = Color::Rgb(200, 205, 224);
const TEXT_BRIGHT: Color = Color::Rgb(232, 236, 245);
const ACCENT_VIOLET: Color = Color::Rgb(124, 92, 252);
const ACCENT_BLUE: Color = Color::Rgb(75, 110, 245);
const ACCENT_ORANGE: Color = Color::Rgb(255, 140, 0);
const ACCENT_AMBER: Color = Color::Rgb(255, 170, 42);
const ACCENT_RED: Color = Color::Rgb(239, 68, 68);
const ACCENT_GREEN: Color = Color::Rgb(34, 197, 94);
const ACCENT_CYAN: Color = Color::Rgb(6, 182, 212);
const ACCENT_PINK: Color = Color::Rgb(236, 72, 153);
```

---

## 6. Sources and References

### Evangelion UI Analysis
- Evangelion 1.0/2.0/3.0+1.0 Rebuild films -- primary source for NERV interface design
- "Neon Genesis Evangelion: The NERV HQ UX" -- numerous design community breakdowns on Behance and Dribbble
- r/FUI (Fantasy User Interfaces) subreddit -- extensive Evangelion UI frame analysis
- Ikuto Yamashita's mechanical design work -- the canonical design language

### Terminal UI Showcases
- awesome-ratatui (github) -- curated list of ratatui applications
- awesome-tuis (github: rothgar/awesome-tuis) -- comprehensive TUI app list
- charm.sh -- Bubbletea ecosystem showcase
- edex-ui (github: GitSquared/edex-ui) -- sci-fi terminal reference
- bottom, gitui, lazygit, bandwhich -- individual project repos

### Anime Terminal Projects
- cool-retro-term (github: Swordfish90/cool-retro-term) -- CRT terminal emulator
- cmatrix, unimatrix, neo -- Matrix rain terminals
- evangelion.vim, Tokyo Night, Kanagawa -- anime-adjacent color schemes

### Color Theory
- Complementary color theory (blue-violet / orange axis)
- NASA/ESA space photography color grading -- Hubble and Webb nebula palettes
- Dracula Theme (draculatheme.com) -- purple + orange dark theme reference
- Tokyo Night (github: folke/tokyonight.nvim) -- blue-violet dark theme reference
- Catppuccin (github: catppuccin) -- community theme with multiple flavors

---

## 7. Further Research Suggestions

- **Ratatui widget gallery** -- scrape the ratatui examples/ directory for widget patterns
- **edex-ui source code** -- study its layout grid system for panel placement
- **Evangelion 3.0+1.0 UI frames** -- the most recent film has the most refined NERV interface design
- **r/unixporn** -- search for "evangelion" and "cyberpunk" terminal rices for real-world implementations
- **Warp terminal** -- modern terminal with blocks and AI; has interesting UI patterns despite not being TUI
- **Figma community** -- search "NERV UI kit" and "sci-fi dashboard" for vector references
- **16-color fallback** -- design a degraded palette for terminals without truecolor support
