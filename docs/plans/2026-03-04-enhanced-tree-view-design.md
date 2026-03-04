# Enhanced Tree View Design for Nika TUI

**Version:** v0.20.0
**Date:** 2026-03-04
**Status:** Design Phase

## Overview

Design a VS Code-like tree view for Nika's Browse view with advanced features including icons, ecosystem file highlighting, animations, and superior UX.

## Research Summary

### Available Crates

| Crate | Version | Downloads | Key Features |
|-------|---------|-----------|--------------|
| `tui-tree-widget` | v0.24.0 | 507k | TreeItem, TreeState, customizable symbols, hjkl navigation |
| `ratatui-explorer` | v0.2.1 | 54k | File explorer, Theme customization, input handling |

**Recommendation:** Use `tui-tree-widget` as base, extend with custom rendering for animations and ecosystem highlighting.

---

## Design Goals

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  🎯 DESIGN GOALS                                                              ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  1. VS Code-like UX     │ Familiar navigation patterns, keyboard shortcuts   ║
║  2. Ecosystem Awareness │ Highlight .nika, .son, pkg: files prominently      ║
║  3. Visual Polish       │ Icons, colors, animations on hover/selection       ║
║  4. Performance         │ Lazy loading, cached renders, no frame lag         ║
║  5. Nika Coherence      │ Solarized theme, 🦋 branding, verb colors          ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

---

## Visual Design

### File Tree Structure

```
┌─ 📁 Browse ──────────────────────────────────────────────────────────────────┐
│                                                                              │
│  🦋 nika-project/                                              [master]     │
│  │                                                                           │
│  ├─▸ 📂 .nika/                              ← Ecosystem folder (teal glow)  │
│  │   ├── 📋 config.toml                                                     │
│  │   ├── 📂 sessions/                                                       │
│  │   └── 📂 cache/                                                          │
│  │                                                                           │
│  ├─▸ 📂 workflows/                          ← Primary workflow location     │
│  │   ├── ✨ deploy.nika.yaml                ← .nika.yaml files (gold glow)  │
│  │   ├── ✨ review-pr.nika.yaml                                             │
│  │   └── ✨ generate-content.nika.yaml                                      │
│  │                                                                           │
│  ├─▾ 📂 agents/                             ← Expanded folder               │
│  │   ├── 🐔 research-agent.son              ← .son files (purple glow)      │
│  │   ├── 🐔 code-reviewer.son                                               │
│  │   └── 📄 README.md                                                       │
│  │                                                                           │
│  ├─▸ 📂 skills/                             ← Skills directory              │
│  │                                                                           │
│  ├── 📄 CLAUDE.md                           ← Documentation                 │
│  ├── 📄 README.md                                                           │
│  └── ⚙️ Cargo.toml                          ← Config files                  │
│                                                                              │
│  ────────────────────────────────────────────────────────────────────────── │
│  [↑↓] Navigate  [←→] Collapse/Expand  [Enter] Open  [Space] Preview         │
│  [/] Search     [H] Toggle Hidden     [R] Refresh   [?] Help                │
└──────────────────────────────────────────────────────────────────────────────┘
```

### Icon System

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  🎨 ICON MAPPING — SUPERNOVAE ECOSYSTEM                                       ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  ═══════════════════════════════════════════════════════════════════════════ ║
║  🚀 SUPERNOVAE ECOSYSTEM (Premium treatment with glow animations)             ║
║  ═══════════════════════════════════════════════════════════════════════════ ║
║                                                                               ║
║  NIKA (Body - Workflow Engine) 🦋                                             ║
║  ─────────────────────────────────────────────────────────────────────────── ║
║  ✨ .nika.yaml    │ Workflow file      │ Gold #f59e0b    │ NF: 󰙨  │ Glow    ║
║  🦋 .nika/        │ Project config     │ Teal #14b8a6    │ NF: 󱂵  │ Glow    ║
║  📂 workflows/    │ Workflows folder   │ Gold #f59e0b    │ NF: 󰙨  │         ║
║  🐔 .son          │ Agent definition   │ Purple #a855f7  │ NF: 󰚩  │ Glow    ║
║  📂 agents/       │ Agents folder      │ Purple #a855f7  │ NF: 󰚩  │         ║
║  📜 .skill.md     │ Skill file         │ Cyan #06b6d4    │ NF: 󰛨  │ Glow    ║
║  📂 skills/       │ Skills folder      │ Cyan #06b6d4    │ NF: 󰛨  │         ║
║                                                                               ║
║  SPN CLI (System Tool) ⚡                                                     ║
║  ─────────────────────────────────────────────────────────────────────────── ║
║  ⚡ ~/.spn/       │ Global SPN config  │ Violet #8b5cf6  │ NF: 󰒓  │ Glow    ║
║  📋 manifest.toml │ System manifest    │ Violet #8b5cf6  │ NF: 󰒓  │         ║
║  🔌 mcp.yaml      │ MCP servers config │ Violet #8b5cf6  │ NF: 󱐋  │         ║
║  📦 packages/     │ Installed packages │ Blue #3b82f6    │ NF: 󰏖  │         ║
║  📄 registry.yaml │ Package registry   │ Blue #3b82f6    │ NF: 󰏖  │         ║
║  🔐 env           │ Environment vars   │ Gray #6b7280    │ NF: 󰌆  │         ║
║  📊 state.json    │ CLI state          │ Gray #6b7280    │ NF: 󰘦  │         ║
║                                                                               ║
║  NOVANET (Brain - Knowledge Graph) 🧠                                         ║
║  ─────────────────────────────────────────────────────────────────────────── ║
║  🧠 .novanet/     │ NovaNet config     │ Sky #0ea5e9     │ NF: 󰠗  │ Glow    ║
║  📂 brain/        │ Schema YAML        │ Sky #0ea5e9     │ NF: 󰠗  │         ║
║  📂 models/       │ Node/Arc classes   │ Sky #0ea5e9     │ NF: 󰠗  │         ║
║  📂 seed/         │ Cypher seeds       │ Sky #0ea5e9     │ NF: 󰆼  │         ║
║                                                                               ║
║  PACKAGE REFERENCES (pkg: URIs)                                               ║
║  ─────────────────────────────────────────────────────────────────────────── ║
║  📦 pkg:@scope/   │ Scoped package     │ Blue #3b82f6    │ NF: 󰏖  │         ║
║  📦 @workflows/   │ Workflow package   │ Gold #f59e0b    │ NF: 󰙨  │         ║
║  📦 @agents/      │ Agent package      │ Purple #a855f7  │ NF: 󰚩  │         ║
║  📦 @skills/      │ Skill package      │ Cyan #06b6d4    │ NF: 󰛨  │         ║
║                                                                               ║
║  ═══════════════════════════════════════════════════════════════════════════ ║
║  📁 STANDARD DIRECTORIES                                                      ║
║  ═══════════════════════════════════════════════════════════════════════════ ║
║                                                                               ║
║  📂 folder (closed)  │ Yellow #fbbf24  │ NF:             │ Fallback: ▸      ║
║  📂 folder (open)    │ Yellow #fbbf24  │ NF:             │ Fallback: ▾      ║
║  📂 src/             │ Source code      │ Green #22c55e   │ NF:             ║
║  📂 tests/           │ Test files       │ Yellow #facc15  │ NF: 󰙨            ║
║  📂 docs/            │ Documentation    │ Blue #3b82f6    │ NF: 󰈙            ║
║  📂 examples/        │ Examples         │ Amber #f59e0b   │ NF: 󰉋            ║
║  📂 .git/            │ Hidden (gray)    │ Gray #6b7280    │ NF:             ║
║  📂 .claude/         │ Claude Code DX   │ Teal #14b8a6    │ NF: 󰚩            ║
║  📂 node_modules/    │ Hidden           │ Gray #6b7280    │ NF: 󰎙            ║
║  📂 target/          │ Hidden           │ Gray #6b7280    │ NF:             ║
║                                                                               ║
║  ═══════════════════════════════════════════════════════════════════════════ ║
║  📄 COMMON FILES                                                              ║
║  ═══════════════════════════════════════════════════════════════════════════ ║
║                                                                               ║
║  📄 .md             │ Markdown         │ Blue #3b82f6    │ NF: 󰍔            ║
║  📄 .yaml/.yml      │ YAML             │ Red #ef4444     │ NF: 󰈙            ║
║  📄 .toml           │ TOML config      │ Gray #9ca3af    │ NF:             ║
║  📄 .json           │ JSON             │ Yellow #fbbf24  │ NF: 󰘦            ║
║  📄 .rs             │ Rust             │ Orange #f97316  │ NF: 󱘗            ║
║  📄 .ts/.js         │ JavaScript       │ Yellow #facc15  │ NF: 󰛦            ║
║  ⚙️  Cargo.toml     │ Rust manifest    │ Orange #f97316  │ NF:             ║
║  📄 CLAUDE.md       │ Claude context   │ Teal #14b8a6    │ NF: 󰚩            ║
║  📖 README.md       │ Documentation    │ Blue #3b82f6    │ NF: 󰍔            ║
║  📋 CHANGELOG.md    │ Version history  │ Green #22c55e   │ NF: 󰋽            ║
║  📋 ROADMAP.md      │ Project roadmap  │ Amber #f59e0b   │ NF: 󰙨            ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### SuperNovae Ecosystem Structure

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  🚀 SUPERNOVAE ECOSYSTEM DIRECTORIES                                          ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  GLOBAL (~/.spn/)                    │ PROJECT-LEVEL                          ║
║  ─────────────────────────────────── │ ───────────────────────────────────── ║
║  ~/.spn/                             │ my-project/                            ║
║  ├── ⚙️ manifest.toml  (bootstrap)   │ ├── 🦋 .nika/                          ║
║  ├── 🔌 mcp.yaml       (MCP config)  │ │   ├── config.toml                   ║
║  ├── 🔐 env            (secrets)     │ │   ├── 📂 sessions/                  ║
║  ├── 📊 state.json     (CLI state)   │ │   └── 📂 cache/                     ║
║  ├── 📄 registry.yaml  (pkg index)   │ │                                      ║
║  ├── 📂 packages/                    │ ├── 📂 workflows/                      ║
║  │   ├── 📂 @scope/name/             │ │   ├── ✨ deploy.nika.yaml           ║
║  │   ├── 📂 workflows/               │ │   └── ✨ review.nika.yaml           ║
║  │   │   └── test-include/           │ │                                      ║
║  │   └── 📂 agents/                  │ ├── 📂 agents/                         ║
║  │       └── test-researcher/        │ │   ├── 🐔 researcher.son             ║
║  └── 📂 cache/                       │ │   └── 🐔 reviewer.son               ║
║                                      │ │                                      ║
║                                      │ ├── 📂 skills/                         ║
║                                      │ │   ├── 📜 seo.skill.md               ║
║                                      │ │   └── 📜 brand.skill.md             ║
║                                      │ │                                      ║
║                                      │ └── 🧠 .novanet/  (if NovaNet proj)   ║
║                                      │     └── config.toml                   ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### Color Scheme (Solarized Extended)

```rust
// In src/tui/theme/tree_colors.rs

pub struct TreeColors {
    // Ecosystem files - Premium treatment with glow
    pub nika_workflow: Color,      // #f59e0b (gold/amber)
    pub son_agent: Color,          // #a855f7 (purple)
    pub nika_folder: Color,        // #14b8a6 (teal)
    pub pkg_reference: Color,      // #3b82f6 (blue)

    // Standard file types
    pub folder: Color,             // #fbbf24 (yellow)
    pub markdown: Color,           // #3b82f6 (blue)
    pub yaml: Color,               // #ef4444 (red)
    pub rust: Color,               // #f97316 (orange)
    pub config: Color,             // #9ca3af (gray)

    // States
    pub selected: Color,           // Inverse or bright
    pub hover: Color,              // Subtle highlight
    pub hidden: Color,             // Dimmed gray

    // Git status
    pub modified: Color,           // #f59e0b (amber)
    pub added: Color,              // #22c55e (green)
    pub deleted: Color,            // #ef4444 (red)
    pub untracked: Color,          // #8b5cf6 (violet)
}
```

---

## Animation System

### Hover Effects

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  ✨ HOVER ANIMATIONS                                                          ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  1. GLOW PULSE (ecosystem files)                                              ║
║  ─────────────────────────────────────────────────────────────────────────── ║
║  Frame 0: ✨ deploy.nika.yaml                    (base color)                 ║
║  Frame 1: ✨ deploy.nika.yaml                    (10% brighter)               ║
║  Frame 2: ✨ deploy.nika.yaml                    (20% brighter)               ║
║  Frame 3: ✨ deploy.nika.yaml                    (10% brighter)               ║
║  Loop: 500ms cycle, sine easing                                               ║
║                                                                               ║
║  2. FOLDER EXPAND (smooth transition)                                         ║
║  ─────────────────────────────────────────────────────────────────────────── ║
║  Frame 0: ▸ 📂 workflows/                                                     ║
║  Frame 1: ▹ 📂 workflows/                        (chevron rotating)          ║
║  Frame 2: ▾ 📂 workflows/                        (fully expanded)            ║
║            ├── ✨ deploy.nika.yaml               (children fade in)          ║
║  Duration: 150ms, ease-out                                                    ║
║                                                                               ║
║  3. SELECTION SWEEP (ASCII border effect)                                     ║
║  ─────────────────────────────────────────────────────────────────────────── ║
║  Normal:   ✨ deploy.nika.yaml                                                ║
║  Selected: ┃ ✨ deploy.nika.yaml ◂               (border + indicator)        ║
║  Animation: Border sweeps from left to right on selection                     ║
║                                                                               ║
║  4. PREVIEW SHIMMER (loading state)                                           ║
║  ─────────────────────────────────────────────────────────────────────────── ║
║  ░░░░░░░░░░░░░░░░░ → ▒▒▒▒▒░░░░░░░░░░░░ → ████▒▒▒░░░░░░░░░░                   ║
║  Shimmer effect while loading file preview                                    ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### Animation Implementation

```rust
// In src/tui/widgets/tree/animation.rs

pub struct TreeAnimation {
    /// Current animation state per node
    node_states: HashMap<NodeId, NodeAnimationState>,
    /// Global animation ticker (shared with app)
    ticker: AnimationTicker,
}

pub struct NodeAnimationState {
    /// Glow intensity (0.0 - 1.0)
    glow: f32,
    /// Expand progress (0.0 = collapsed, 1.0 = expanded)
    expand: f32,
    /// Selection highlight progress
    selection: f32,
    /// Hover state for effects
    hovered: bool,
    /// Animation direction
    direction: AnimationDirection,
}

impl TreeAnimation {
    pub fn tick(&mut self, dt: Duration) {
        for state in self.node_states.values_mut() {
            // Glow pulse for ecosystem files
            if state.hovered {
                state.glow = (state.glow + dt.as_secs_f32() * 4.0).sin().abs();
            }

            // Smooth expand/collapse
            let target = if state.is_expanded { 1.0 } else { 0.0 };
            state.expand = lerp(state.expand, target, dt.as_secs_f32() * 10.0);
        }
    }

    pub fn glow_color(&self, base: Color, glow: f32) -> Color {
        // Brighten color based on glow intensity
        Color::Rgb(
            (base.r as f32 * (1.0 + glow * 0.3)).min(255.0) as u8,
            (base.g as f32 * (1.0 + glow * 0.3)).min(255.0) as u8,
            (base.b as f32 * (1.0 + glow * 0.3)).min(255.0) as u8,
        )
    }
}
```

---

## Architecture

### Module Structure

```
src/tui/
├── widgets/
│   └── tree/
│       ├── mod.rs           # Public API
│       ├── node.rs          # TreeNode struct with metadata
│       ├── state.rs         # TreeState with selection/expansion
│       ├── render.rs        # Rendering with icons and colors
│       ├── animation.rs     # Hover/expand animations
│       ├── icons.rs         # Icon mapping (NerdFont + fallback)
│       ├── theme.rs         # Color scheme per file type
│       └── events.rs        # Keyboard/mouse handling
│
├── views/
│   └── browse.rs            # Browse view using tree widget
│
└── theme/
    └── tree_colors.rs       # Solarized color definitions
```

### Key Types

```rust
// In src/tui/widgets/tree/node.rs

#[derive(Debug, Clone)]
pub struct TreeNode {
    /// Unique identifier
    pub id: NodeId,
    /// Display name
    pub name: String,
    /// Full path
    pub path: PathBuf,
    /// Node type for icon/color selection
    pub kind: NodeKind,
    /// Git status (if available)
    pub git_status: Option<GitStatus>,
    /// Children (lazily loaded)
    pub children: Option<Vec<TreeNode>>,
    /// Is this node expanded?
    pub expanded: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    // ═══════════════════════════════════════════════════════════════════
    // SUPERNOVAE ECOSYSTEM (Premium treatment with glow animations)
    // ═══════════════════════════════════════════════════════════════════

    // NIKA (Body - Workflow Engine) 🦋
    NikaWorkflow,        // .nika.yaml - Gold glow
    NikaFolder,          // .nika/ - Teal glow
    SonAgent,            // .son - Purple glow
    SkillFile,           // .skill.md - Cyan glow
    WorkflowsFolder,     // workflows/
    AgentsFolder,        // agents/
    SkillsFolder,        // skills/

    // SPN CLI (System Tool) ⚡
    SpnFolder,           // ~/.spn/ - Violet glow
    SpnManifest,         // manifest.toml
    SpnMcpConfig,        // mcp.yaml
    SpnPackages,         // packages/
    SpnRegistry,         // registry.yaml
    SpnEnv,              // env (secrets)
    SpnState,            // state.json

    // NOVANET (Brain - Knowledge Graph) 🧠
    NovanetFolder,       // .novanet/ - Sky glow
    BrainFolder,         // brain/
    ModelsFolder,        // models/
    SeedFolder,          // seed/

    // CLAUDE CODE DX
    ClaudeFolder,        // .claude/
    ClaudeMd,            // CLAUDE.md

    // ═══════════════════════════════════════════════════════════════════
    // STANDARD DIRECTORIES
    // ═══════════════════════════════════════════════════════════════════
    Directory,
    SrcFolder,           // src/
    TestsFolder,         // tests/
    DocsFolder,          // docs/
    ExamplesFolder,      // examples/

    // ═══════════════════════════════════════════════════════════════════
    // COMMON FILES
    // ═══════════════════════════════════════════════════════════════════
    File,
    Markdown,
    Yaml,
    Toml,
    Json,
    Rust,
    JavaScript,
    TypeScript,
    Readme,              // README.md
    Changelog,           // CHANGELOG.md
    Roadmap,             // ROADMAP.md
    CargoToml,           // Cargo.toml
    PackageJson,         // package.json

    // ═══════════════════════════════════════════════════════════════════
    // HIDDEN (dimmed, collapsed by default)
    // ═══════════════════════════════════════════════════════════════════
    GitFolder,
    NodeModules,
    TargetFolder,
}

impl NodeKind {
    pub fn from_path(path: &Path) -> Self {
        let name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        // ═══════════════════════════════════════════════════════════════════
        // SUPERNOVAE ECOSYSTEM DETECTION (Premium treatment)
        // ═══════════════════════════════════════════════════════════════════

        // NIKA (Body) 🦋
        if name.ends_with(".nika.yaml") {
            return Self::NikaWorkflow;
        }
        if name.ends_with(".son") {
            return Self::SonAgent;
        }
        if name.ends_with(".skill.md") {
            return Self::SkillFile;
        }
        if name == ".nika" && path.is_dir() {
            return Self::NikaFolder;
        }

        // SPN CLI ⚡
        if name == ".spn" && path.is_dir() {
            return Self::SpnFolder;
        }
        if name == "manifest.toml" && path.parent().map(|p| p.ends_with(".spn")).unwrap_or(false) {
            return Self::SpnManifest;
        }
        if name == "mcp.yaml" {
            return Self::SpnMcpConfig;
        }
        if name == "registry.yaml" {
            return Self::SpnRegistry;
        }

        // NOVANET (Brain) 🧠
        if name == ".novanet" && path.is_dir() {
            return Self::NovanetFolder;
        }
        if name == "brain" && path.is_dir() {
            return Self::BrainFolder;
        }

        // CLAUDE CODE DX
        if name == ".claude" && path.is_dir() {
            return Self::ClaudeFolder;
        }

        // ═══════════════════════════════════════════════════════════════════
        // SPECIAL FOLDERS
        // ═══════════════════════════════════════════════════════════════════
        if path.is_dir() {
            return match name {
                // Ecosystem folders
                "workflows" => Self::WorkflowsFolder,
                "agents" => Self::AgentsFolder,
                "skills" => Self::SkillsFolder,
                "packages" => Self::SpnPackages,
                "models" => Self::ModelsFolder,
                "seed" => Self::SeedFolder,

                // Standard folders
                "src" => Self::SrcFolder,
                "tests" => Self::TestsFolder,
                "docs" => Self::DocsFolder,
                "examples" => Self::ExamplesFolder,

                // Hidden folders
                ".git" => Self::GitFolder,
                "node_modules" => Self::NodeModules,
                "target" => Self::TargetFolder,

                _ => Self::Directory,
            };
        }

        // ═══════════════════════════════════════════════════════════════════
        // SPECIAL FILES
        // ═══════════════════════════════════════════════════════════════════
        match name {
            "CLAUDE.md" => Self::ClaudeMd,
            "README.md" => Self::Readme,
            "CHANGELOG.md" => Self::Changelog,
            "ROADMAP.md" => Self::Roadmap,
            "Cargo.toml" => Self::CargoToml,
            "package.json" => Self::PackageJson,
            "env" => Self::SpnEnv,
            "state.json" => Self::SpnState,
            _ => {
                // By extension
                match path.extension().and_then(|e| e.to_str()) {
                    Some("md") => Self::Markdown,
                    Some("yaml" | "yml") => Self::Yaml,
                    Some("toml") => Self::Toml,
                    Some("json") => Self::Json,
                    Some("rs") => Self::Rust,
                    Some("ts" | "tsx") => Self::TypeScript,
                    Some("js" | "jsx") => Self::JavaScript,
                    _ => Self::File,
                }
            }
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            // ═══════════════════════════════════════════════════════════════
            // SUPERNOVAE ECOSYSTEM (Premium icons)
            // ═══════════════════════════════════════════════════════════════

            // NIKA 🦋
            Self::NikaWorkflow => "✨",      // Gold sparkle
            Self::NikaFolder => "🦋",        // Butterfly
            Self::SonAgent => "🐔",          // Space chicken
            Self::SkillFile => "📜",         // Scroll
            Self::WorkflowsFolder => "⚡",   // Workflows
            Self::AgentsFolder => "🐔",      // Agents
            Self::SkillsFolder => "📚",      // Skills

            // SPN CLI ⚡
            Self::SpnFolder => "⚡",         // SPN
            Self::SpnManifest => "📋",       // Manifest
            Self::SpnMcpConfig => "🔌",      // MCP
            Self::SpnPackages => "📦",       // Packages
            Self::SpnRegistry => "📦",       // Registry
            Self::SpnEnv => "🔐",            // Secrets
            Self::SpnState => "📊",          // State

            // NOVANET 🧠
            Self::NovanetFolder => "🧠",     // Brain
            Self::BrainFolder => "🧠",       // Brain
            Self::ModelsFolder => "📐",      // Models
            Self::SeedFolder => "🌱",        // Seed

            // CLAUDE CODE DX
            Self::ClaudeFolder => "🤖",      // Claude
            Self::ClaudeMd => "🤖",          // Claude

            // ═══════════════════════════════════════════════════════════════
            // STANDARD DIRECTORIES
            // ═══════════════════════════════════════════════════════════════
            Self::Directory => "📂",
            Self::SrcFolder => "📂",
            Self::TestsFolder => "🧪",
            Self::DocsFolder => "📚",
            Self::ExamplesFolder => "💡",

            // ═══════════════════════════════════════════════════════════════
            // COMMON FILES
            // ═══════════════════════════════════════════════════════════════
            Self::Readme => "📖",
            Self::Changelog => "📋",
            Self::Roadmap => "🗺️",
            Self::CargoToml => "⚙️",
            Self::PackageJson => "📦",
            Self::Markdown => "📄",
            Self::Yaml => "📄",
            Self::Toml => "⚙️",
            Self::Json => "📄",
            Self::Rust => "🦀",
            Self::TypeScript => "📄",
            Self::JavaScript => "📄",
            Self::File => "📄",

            // ═══════════════════════════════════════════════════════════════
            // HIDDEN
            // ═══════════════════════════════════════════════════════════════
            Self::GitFolder => "📂",
            Self::NodeModules => "📦",
            Self::TargetFolder => "📂",
        }
    }

    pub fn color(&self, colors: &TreeColors) -> Color {
        match self {
            // NIKA ecosystem - warm colors
            Self::NikaWorkflow | Self::WorkflowsFolder => colors.nika_workflow, // Gold
            Self::SonAgent | Self::AgentsFolder => colors.son_agent,            // Purple
            Self::NikaFolder => colors.nika_folder,                             // Teal
            Self::SkillFile | Self::SkillsFolder => colors.skill,               // Cyan

            // SPN ecosystem - violet
            Self::SpnFolder | Self::SpnManifest | Self::SpnMcpConfig => colors.spn, // Violet
            Self::SpnPackages | Self::SpnRegistry => colors.pkg,                    // Blue

            // NovaNet ecosystem - sky
            Self::NovanetFolder | Self::BrainFolder | Self::ModelsFolder | Self::SeedFolder =>
                colors.novanet, // Sky

            // Claude ecosystem - teal
            Self::ClaudeFolder | Self::ClaudeMd => colors.claude, // Teal

            // Standard directories
            Self::Directory | Self::SrcFolder | Self::TestsFolder |
            Self::DocsFolder | Self::ExamplesFolder => colors.folder,

            // File types
            Self::Markdown | Self::Readme => colors.markdown,
            Self::Yaml => colors.yaml,
            Self::Rust | Self::CargoToml => colors.rust,
            Self::TypeScript | Self::JavaScript | Self::PackageJson => colors.javascript,
            Self::Changelog => colors.changelog,

            // Hidden (dimmed)
            Self::GitFolder | Self::NodeModules | Self::TargetFolder => colors.hidden,

            // Default
            _ => colors.file,
        }
    }

    /// Returns true if this node should get glow animation on hover
    pub fn is_ecosystem(&self) -> bool {
        matches!(self,
            // NIKA
            Self::NikaWorkflow |
            Self::NikaFolder |
            Self::SonAgent |
            Self::SkillFile |
            // SPN
            Self::SpnFolder |
            // NOVANET
            Self::NovanetFolder |
            // CLAUDE
            Self::ClaudeFolder
        )
    }

    /// Returns true if this node should be hidden by default
    pub fn is_hidden(&self) -> bool {
        matches!(self,
            Self::GitFolder |
            Self::NodeModules |
            Self::TargetFolder
        )
    }
}
```

---

## Keyboard Navigation

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  ⌨️ KEYBOARD SHORTCUTS                                                        ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  NAVIGATION                                                                   ║
║  ─────────────────────────────────────────────────────────────────────────── ║
║  ↑ / k        │ Move up                                                      ║
║  ↓ / j        │ Move down                                                    ║
║  ← / h        │ Collapse folder / Go to parent                               ║
║  → / l        │ Expand folder / Enter folder                                 ║
║  Home / gg    │ Go to first item                                             ║
║  End / G      │ Go to last item                                              ║
║  PageUp       │ Page up                                                      ║
║  PageDown     │ Page down                                                    ║
║                                                                               ║
║  ACTIONS                                                                      ║
║  ─────────────────────────────────────────────────────────────────────────── ║
║  Enter        │ Open file in Studio / Toggle folder                          ║
║  Space        │ Preview file (split pane)                                    ║
║  o            │ Open in external editor                                      ║
║  y            │ Copy path to clipboard                                       ║
║  d            │ Delete file (with confirmation)                              ║
║  r            │ Rename file                                                  ║
║  n            │ New file                                                     ║
║  N            │ New folder                                                   ║
║                                                                               ║
║  FILTERING                                                                    ║
║  ─────────────────────────────────────────────────────────────────────────── ║
║  /            │ Search/filter (fuzzy)                                        ║
║  H            │ Toggle hidden files (.git, node_modules)                     ║
║  W            │ Show only .nika.yaml files                                   ║
║  A            │ Show only .son agent files                                   ║
║  Esc          │ Clear filter                                                 ║
║                                                                               ║
║  VIEW                                                                         ║
║  ─────────────────────────────────────────────────────────────────────────── ║
║  R            │ Refresh tree                                                 ║
║  z            │ Collapse all                                                 ║
║  Z            │ Expand all                                                   ║
║  ?            │ Show keyboard shortcuts                                      ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

---

## Implementation Phases

### Phase 1: Core Tree Widget (2-3 hours)
- [ ] Create `tree/` module structure
- [ ] Implement `TreeNode` and `NodeKind` types
- [ ] Implement `TreeState` with selection/expansion
- [ ] Basic rendering without animations
- [ ] Keyboard navigation (hjkl, arrows)

### Phase 2: Icon and Color System (1-2 hours)
- [ ] Implement icon mapping with NerdFont fallbacks
- [ ] Implement `TreeColors` with Solarized palette
- [ ] Ecosystem file detection (.nika.yaml, .son, .nika/)
- [ ] Git status colors

### Phase 3: Animations (2-3 hours)
- [ ] Implement `TreeAnimation` state manager
- [ ] Glow pulse for ecosystem files
- [ ] Smooth expand/collapse transitions
- [ ] Selection sweep effect
- [ ] Preview shimmer loading state

### Phase 4: Advanced Features (2-3 hours)
- [ ] Fuzzy search filtering
- [ ] File preview split pane
- [ ] Git integration (modified/added/deleted indicators)
- [ ] Hidden file toggle
- [ ] Workflow/Agent filter modes

### Phase 5: Polish (1-2 hours)
- [ ] Performance optimization (lazy loading)
- [ ] Edge case handling
- [ ] Tests
- [ ] Documentation

---

## Dependencies

```toml
# Cargo.toml additions
[dependencies]
tui-tree-widget = "0.24"

# Optional for enhanced icons
nerd-fonts = { version = "0.1", optional = true }
```

---

## Visual Reference: Complete Browse View

```
╔══════════════════════════════════════════════════════════════════════════════════════════════════╗
║  🦋 Nika Studio                                                            v0.20.0  ⌘K  ?  ║
╠══════════════════════════════════════════════════════════════════════════════════════════════════╣
║ [b]📁Browse  [e]📝Editor  [r]▶Runner  [c]💬Chat  [s]📅Sched  [,]⚙️Set                           ║
╠══════════════════════════════════════════════════════════════════════════════════════════════════╣
║                                                                                                  ║
║  ┌─ 📁 Browse ────────────────────────────────┬─ 👁 Preview ─────────────────────────────────┐  ║
║  │                                            │                                               │  ║
║  │  🦋 nika-project/              [master ✓]  │  # deploy.nika.yaml                          │  ║
║  │  │                                         │                                               │  ║
║  │  ├─▸ 🦋 .nika/                             │  schema: "nika/workflow@0.9"                 │  ║
║  │  │   ├── 📋 config.toml                    │  workflow: deploy                            │  ║
║  │  │   └── 📂 sessions/                      │                                               │  ║
║  │  │                                         │  tasks:                                       │  ║
║  │  ├─▾ 📂 workflows/             [3 files]   │    - id: build                               │  ║
║  │  │   ├── ✨ deploy.nika.yaml   [selected]  │      exec: "npm run build"                   │  ║
║  │  │   ├── ✨ review-pr.nika.yaml            │                                               │  ║
║  │  │   └── ✨ generate.nika.yaml             │    - id: deploy                              │  ║
║  │  │                                         │      use:                                     │  ║
║  │  ├─▸ 🐔 agents/                [2 files]   │        build_output: build                   │  ║
║  │  │                                         │      exec: |                                  │  ║
║  │  ├─▸ 📂 skills/                [4 files]   │        aws s3 sync ./dist s3://bucket       │  ║
║  │  │                                         │                                               │  ║
║  │  ├── 🤖 CLAUDE.md                          │  flows:                                       │  ║
║  │  ├── 📖 README.md                          │    - source: build                           │  ║
║  │  └── ⚙️ Cargo.toml                         │      target: deploy                          │  ║
║  │                                            │                                               │  ║
║  │  ──────────────────────────────────────────│───────────────────────────────────────────── │  ║
║  │  Filter: [          ] 🔍  Showing 12 items │  Lines: 23 │ Schema: @0.9 │ Valid ✓          │  ║
║  └────────────────────────────────────────────┴───────────────────────────────────────────────┘  ║
║                                                                                                  ║
╠══════════════════════════════════════════════════════════════════════════════════════════════════╣
║  [↑↓] Navigate  [←→] Expand/Collapse  [Enter] Open in Editor  [Space] Preview  [/] Filter       ║
║  🧠 claude-sonnet-4  │  📊 12 files  │  🔌 MCP: 2 servers  │  Git: master ✓                      ║
╚══════════════════════════════════════════════════════════════════════════════════════════════════╝
```

---

## Summary

This design creates a premium VS Code-like tree view experience with:

1. **Ecosystem Awareness** - .nika.yaml, .son, .nika/ files get special icons and glow effects
2. **Visual Polish** - NerdFont icons, Solarized colors, smooth animations
3. **Keyboard-First** - Full Vim-style navigation (hjkl) plus VS Code shortcuts
4. **Performance** - Lazy loading, cached renders, no frame lag
5. **Nika Coherence** - 🦋 branding, verb colors, ecosystem highlighting

The implementation uses `tui-tree-widget` as a foundation with custom rendering extensions for animations and ecosystem file highlighting.
