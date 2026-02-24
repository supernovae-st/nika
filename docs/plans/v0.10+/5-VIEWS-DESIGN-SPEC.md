# Nika TUI — 5-Views Design Specification

> **⚠️ SUPERSEDED:** This document is replaced by [6-Views Design](./2026-02-24-v010-v012-6-views-design.md).
> Settings view is now included as view #6 in standby mode (opens existing Provider Modal until v0.11.1).

**Version:** 1.0.0
**Date:** 2026-02-24
**Authors:** Thibaut, Claude
**Status:** SUPERSEDED (see 6-Views Design)

---

## Executive Summary

Refonte complète du TUI Nika avec **5 vues** (EXPLORER, CHAT, EDITOR, RUNNER, SCHEDULER), architecture Explorer-first (VS Code style), et design system unifié avec emojis Nika.

**Note:** La vue SETTINGS est supprimée — les popups existantes (Provider Modal, MCP Config) font le travail.

---

## Table of Contents

1. [Version Roadmap](#version-roadmap)
2. [Crate Dependencies](#crate-dependencies)
3. [Design System](#design-system)
4. [View 1: EXPLORER](#view-1-explorer)
5. [View 2: CHAT](#view-2-chat)
6. [View 3: EDITOR](#view-3-editor)
7. [View 4: RUNNER](#view-4-runner)
8. [View 5: SCHEDULER](#view-5-scheduler)
9. [Shared Widgets](#shared-widgets)
10. [Navigation System](#navigation-system)
11. [Implementation Phases](#implementation-phases)
12. [Success Criteria](#success-criteria)

---

## Version Roadmap

| Version | Milestone | Key Features | Est. Tasks |
|---------|-----------|--------------|------------|
| **v0.10.0** | Explorer + Editor | 5-view navigation, Explorer 6-panel, Editor DAG sync | 15 |
| **v0.10.1** | Chat-as-DAG | Live DAG, YAML preview, @mentions, // fork syntax | 12 |
| **v0.11.0** | Runner | Animated execution, streaming output, MCP log | 10 |
| **v0.11.1** | Scheduler | Timeline view, cron editor, priority queue | 12 |
| **v0.12.0** | Polish | 60fps animations, minimap, breadcrumbs, performance | 8 |

**Total:** ~57 tasks across 5 releases

---

## Crate Dependencies

### Core TUI

| Crate | Version | Purpose | Context7 ID |
|-------|---------|---------|-------------|
| **ratatui** | 0.29+ | TUI framework | `/ratatui/ratatui` |
| **ratatui-widgets** | 0.3+ | Chart, Sparkline, Gauge | `/websites/rs_ratatui-widgets_0_3_0` |
| **crossterm** | 0.28+ | Terminal backend | - |

### Tree & Navigation

| Crate | Version | Purpose | Context7 ID |
|-------|---------|---------|-------------|
| **tui-tree-widget** | 0.21+ | File tree with TreeItem/TreeState | `/websites/rs_tui-tree-widget_tui_tree_widget` |
| **nucleo-matcher** | 0.3+ | Fuzzy search (faster than fzf) | `/websites/rs_nucleo-matcher_0_3_1` |

### Scheduler

| Crate | Version | Purpose | Context7 ID |
|-------|---------|---------|-------------|
| **cron** | 0.12+ | Cron expression parser | `/zslayton/cron` |
| **chrono** | 0.4+ | DateTime handling | `/websites/rs_chrono_chrono` |

### Syntax Highlighting

| Crate | Version | Purpose | Context7 ID |
|-------|---------|---------|-------------|
| **syntect** | 5.2+ | YAML syntax highlighting | `/trishume/syntect` |

### Graph

| Crate | Version | Purpose | Context7 ID |
|-------|---------|---------|-------------|
| **petgraph** | 0.6+ | DAG data structure (StableGraph) | `/websites/rs_petgraph` |

### Cargo.toml Additions

```toml
[dependencies]
# TUI
ratatui = { version = "0.29", features = ["all-widgets"] }
tui-tree-widget = "0.21"

# Fuzzy search
nucleo-matcher = "0.3"

# Scheduler
cron = "0.12"
chrono = { version = "0.4", features = ["serde"] }

# Syntax highlighting (optional, feature-gated)
syntect = { version = "5.2", optional = true, default-features = false, features = ["parsing", "default-themes"] }

[features]
syntax-highlight = ["syntect"]
```

---

## Design System

### Emoji Taxonomy (Nika Brand)

#### Core Icons

| Emoji | Meaning | Usage |
|-------|---------|-------|
| 🦋 | **Nika brand** | Logo, status, loading heartbeat |
| 🚂 | **Workflow** | Fichiers `*.nika.yaml` |
| 🪽 | **Nika folder** | Dossier `.nika/` |
| 📁 | **Folder** | Dossiers standards |
| 📄 | **File** | Fichiers génériques |
| 🐔 | **Agent** | Fichiers `*.agent.yaml` |
| 🎯 | **Skill** | Fichiers `*.skill.yaml` |
| 📝 | **Context** | Fichiers contexte |

#### Verb Icons (DAG Nodes)

| Emoji | Verb | Color (Hex) | Tailwind |
|-------|------|-------------|----------|
| ⚡ | `infer:` | #8B5CF6 | violet-500 |
| 📟 | `exec:` | #F59E0B | amber-500 |
| 🛰️ | `fetch:` | #06B6D4 | cyan-500 |
| 🔌 | `invoke:` | #10B981 | emerald-500 |
| 🐔 | `agent:` | #F43F5E | rose-500 |
| 🐤 | subagent | #FB7185 | rose-400 |

#### Status Icons

| Icon | Meaning | Color |
|------|---------|-------|
| ✓ | Completed | Green |
| ◐ | Running/Streaming | Blue (animated) |
| ⏳ | Queued/Pending | Gray |
| ✗ | Failed | Red |
| ⏸ | Paused | Yellow |

#### Spinner Pattern (Heartbeat)

```rust
pub const NIKA_SPINNER: &[&str] = &[
    "🦋", "🌌", "🦋", "🦀", "🦋", "🪐", "🦋", "🦖", "🦋", "🐦‍🔥"
];
// Interval: 120ms per frame
// Usage: Loading states, background tasks
```

### Color Palette (Solarized-Compatible)

```rust
pub struct NikaColors {
    // Backgrounds
    pub bg_base:    Color::Rgb(0, 43, 54),    // #002b36 (dark)
    pub bg_light:   Color::Rgb(253, 246, 227), // #fdf6e3 (light)

    // Foregrounds
    pub fg_primary: Color::Rgb(131, 148, 150), // #839496
    pub fg_muted:   Color::Rgb(88, 110, 117),  // #586e75

    // Accents
    pub accent_blue:    Color::Rgb(38, 139, 210),  // #268bd2
    pub accent_cyan:    Color::Rgb(42, 161, 152),  // #2aa198
    pub accent_green:   Color::Rgb(133, 153, 0),   // #859900
    pub accent_yellow:  Color::Rgb(181, 137, 0),   // #b58900
    pub accent_orange:  Color::Rgb(203, 75, 22),   // #cb4b16
    pub accent_red:     Color::Rgb(220, 50, 47),   // #dc322f
    pub accent_magenta: Color::Rgb(211, 54, 130),  // #d33682
    pub accent_violet:  Color::Rgb(108, 113, 196), // #6c71c4
}
```

### Typography

| Element | Style |
|---------|-------|
| Headers | Bold, accent color |
| Body text | Regular, fg_primary |
| Muted text | Regular, fg_muted |
| Code | Monospace (terminal default) |
| Shortcuts | `[Key]` with border |

---

## View 1: EXPLORER

### Purpose

Entry point par défaut. Browse files, preview workflows, access recent sessions.

### Layout (6 Panels)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              HEADER (3%)                                    │
├────────────────┬───────────────────────────┬────────────────────────────────┤
│                │                           │                                │
│   PANEL A      │       PANEL B             │         PANEL C                │
│   Tree View    │       DAG Preview         │         YAML Preview           │
│                │                           │                                │
│    25%         │         35%               │           40%                  │  72%
│                │                           │                                │
│                │                           │                                │
├────────────────┼───────────────────────────┼────────────────────────────────┤
│   PANEL D      │       PANEL E             │         PANEL F                │
│   Quick Access │       Actions + Stats     │         Metadata               │  22%
│    25%         │         35%               │           40%                  │
├────────────────┴───────────────────────────┴────────────────────────────────┤
│                            STATUS BAR (3%)                                  │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Panel Specifications

#### Panel A: Tree View (25% × 72%)

**Component:** `tui-tree-widget::Tree<FileIdentifier>`

**Data Structure:**
```rust
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct FileIdentifier {
    pub path: PathBuf,
    pub kind: FileKind,
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub enum FileKind {
    Directory,
    NikaFolder,      // .nika/
    Workflow,        // *.nika.yaml
    Agent,           // *.agent.yaml
    Skill,           // *.skill.yaml
    Context,         // context files
    Regular,         // other files
}

impl FileKind {
    pub fn icon(&self) -> &'static str {
        match self {
            FileKind::Directory => "📁",
            FileKind::NikaFolder => "🪽",
            FileKind::Workflow => "🚂",
            FileKind::Agent => "🐔",
            FileKind::Skill => "🎯",
            FileKind::Context => "📝",
            FileKind::Regular => "📄",
        }
    }

    pub fn priority(&self) -> u8 {
        // Higher = shown first
        match self {
            FileKind::NikaFolder => 100,
            FileKind::Workflow => 90,
            FileKind::Agent => 80,
            FileKind::Skill => 70,
            FileKind::Context => 60,
            FileKind::Directory => 50,
            FileKind::Regular => 10,
        }
    }
}
```

**TreeItem Construction:**
```rust
use tui_tree_widget::{TreeItem, TreeState};

fn build_tree_items(dir: &Path) -> Vec<TreeItem<'static, FileIdentifier>> {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .collect();

    // Sort by kind priority, then alphabetically
    entries.sort_by(|a, b| {
        let kind_a = FileKind::from_path(a.path());
        let kind_b = FileKind::from_path(b.path());
        kind_b.priority().cmp(&kind_a.priority())
            .then_with(|| a.file_name().cmp(&b.file_name()))
    });

    entries.into_iter().map(|entry| {
        let path = entry.path();
        let kind = FileKind::from_path(&path);
        let id = FileIdentifier { path: path.clone(), kind: kind.clone() };
        let text = format!("{} {}", kind.icon(), entry.file_name().to_string_lossy());

        if path.is_dir() {
            let children = build_tree_items(&path);
            TreeItem::new(id, text, children).unwrap()
        } else {
            TreeItem::new_leaf(id, text)
        }
    }).collect()
}
```

**Visual Hierarchy:**
```rust
// Nika files: 100% opacity
// Other files: 60% opacity (dimmed)
fn style_for_kind(kind: &FileKind) -> Style {
    match kind {
        FileKind::Workflow | FileKind::Agent | FileKind::Skill | FileKind::NikaFolder => {
            Style::default().fg(Color::White)
        }
        _ => Style::default().fg(Color::DarkGray)
    }
}
```

**Box Drawing:**
```
├── Non-last sibling
└── Last sibling
│   Vertical continuation
    Empty (parent was last)
▶ Collapsed folder (prefix)
▼ Expanded folder (prefix)
```

#### Panel B: DAG Preview (35% × 72%)

**Component:** `DagPanel` (shared widget, see [Shared Widgets](#shared-widgets))

**Behavior:**
- Shows DAG when a `.nika.yaml` file is selected
- Empty state: "Select a workflow to preview DAG"
- Auto-layout with topological sort
- Read-only (no interaction)

#### Panel C: YAML Preview (40% × 72%)

**Component:** `Paragraph` with optional `syntect` highlighting

**Features:**
- Line numbers (right-aligned, dimmed)
- Syntax highlighting for YAML (if `syntax-highlight` feature enabled)
- Scroll with `▲█░▼` indicators
- Word wrap disabled (horizontal scroll)

**Syntax Highlighting (Optional):**
```rust
#[cfg(feature = "syntax-highlight")]
fn highlight_yaml(content: &str) -> Vec<Spans<'static>> {
    use syntect::easy::HighlightLines;
    use syntect::parsing::SyntaxSet;
    use syntect::highlighting::ThemeSet;

    let ps = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    let syntax = ps.find_syntax_by_extension("yaml").unwrap();
    let theme = &ts.themes["Solarized (dark)"];

    let mut h = HighlightLines::new(syntax, theme);

    content.lines().enumerate().map(|(i, line)| {
        let line_num = Span::styled(
            format!("{:4} ", i + 1),
            Style::default().fg(Color::DarkGray)
        );

        let ranges = h.highlight_line(line, &ps).unwrap();
        let spans: Vec<Span> = ranges.into_iter().map(|(style, text)| {
            Span::styled(text.to_string(), convert_style(style))
        }).collect();

        Spans::from([vec![line_num], spans].concat())
    }).collect()
}

#[cfg(not(feature = "syntax-highlight"))]
fn highlight_yaml(content: &str) -> Vec<Spans<'static>> {
    // Fallback: line numbers only
    content.lines().enumerate().map(|(i, line)| {
        Spans::from(vec![
            Span::styled(format!("{:4} ", i + 1), Style::default().fg(Color::DarkGray)),
            Span::raw(line.to_string()),
        ])
    }).collect()
}
```

#### Panel D: Quick Access (25% × 22%)

**Content:**
- ★ Pinned workflows (max 5)
- ◇ Recent workflows (max 5)

**Data Structure:**
```rust
pub struct QuickAccess {
    pub pinned: Vec<PathBuf>,    // User-pinned (persisted)
    pub recent: VecDeque<PathBuf>, // Auto-tracked (max 10)
}

impl QuickAccess {
    pub fn display_items(&self) -> Vec<QuickAccessItem> {
        let mut items = Vec::new();

        for path in &self.pinned {
            items.push(QuickAccessItem {
                icon: "★",
                name: path.file_stem().unwrap().to_string_lossy().to_string(),
                path: path.clone(),
                pinned: true,
            });
        }

        for path in self.recent.iter().filter(|p| !self.pinned.contains(p)).take(5) {
            items.push(QuickAccessItem {
                icon: "◇",
                name: path.file_stem().unwrap().to_string_lossy().to_string(),
                path: path.clone(),
                pinned: false,
            });
        }

        items
    }
}
```

#### Panel E: Actions + Stats (35% × 22%)

**Actions Row:**
```
[▶ Run] [✏ Edit] [👁 Preview] [📋 Copy Path]
```

**Stats Row (for workflows):**
```
Tasks: 5 │ MCP: 2 │ Depth: 3
Verbs: ⚡×2 📟×1 🔌×1 🐔×1
```

**Stats Calculation:**
```rust
pub struct WorkflowStats {
    pub task_count: usize,
    pub mcp_server_count: usize,
    pub max_depth: usize,
    pub verb_counts: HashMap<VerbKind, usize>,
}

impl WorkflowStats {
    pub fn from_workflow(workflow: &Workflow) -> Self {
        let mut verb_counts = HashMap::new();
        for task in &workflow.tasks {
            let kind = task.action.verb_kind();
            *verb_counts.entry(kind).or_insert(0) += 1;
        }

        Self {
            task_count: workflow.tasks.len(),
            mcp_server_count: workflow.mcp.as_ref().map(|m| m.servers.len()).unwrap_or(0),
            max_depth: calculate_dag_depth(&workflow.tasks, &workflow.flows),
            verb_counts,
        }
    }

    pub fn format_verbs(&self) -> String {
        let order = [VerbKind::Infer, VerbKind::Exec, VerbKind::Fetch, VerbKind::Invoke, VerbKind::Agent];
        order.iter()
            .filter_map(|v| self.verb_counts.get(v).map(|c| format!("{}×{}", v.icon(), c)))
            .collect::<Vec<_>>()
            .join(" ")
    }
}
```

#### Panel F: Metadata (40% × 22%)

**Content:**
```
Created: 2026-02-20 14:30
Modified: 2026-02-24 10:15
Size: 2.4 KB
Schema: v0.6 │ Valid: ✓
```

**Data Source:** `fs::metadata()` + YAML parsing

### Keyboard Navigation

| Key | Action | Scope |
|-----|--------|-------|
| `Tab` | Cycle panels (A→B→C→D→E→F→A) | Global |
| `Shift+Tab` | Cycle panels reverse | Global |
| `↑↓` / `jk` | Navigate within panel | Panel |
| `←→` / `hl` | Collapse/expand folder | Panel A |
| `Enter` | Open in Editor / Expand folder | Panel A, D |
| `Space` | Toggle preview | Panel A |
| `r` / `R` | Run workflow | Panel A (on workflow) |
| `p` | Pin/unpin to Quick Access | Panel A |
| `Ctrl+P` | Fuzzy file search | Global |
| `Ctrl+C` | Copy content | Panel C |
| `1-5` | Direct view switch | Global |
| `?` | Help overlay | Global |

### Fuzzy Search (Ctrl+P)

**Component:** `nucleo-matcher` overlay

```rust
use nucleo_matcher::{Matcher, Config};
use nucleo_matcher::pattern::{Pattern, CaseMatching, Normalization};

pub struct FuzzySearch {
    matcher: Matcher,
    query: String,
    results: Vec<SearchResult>,
    selected: usize,
}

pub struct SearchResult {
    pub path: PathBuf,
    pub score: u32,
    pub indices: Vec<u32>, // Match positions for highlighting
}

impl FuzzySearch {
    pub fn new() -> Self {
        Self {
            matcher: Matcher::new(Config::DEFAULT),
            query: String::new(),
            results: Vec::new(),
            selected: 0,
        }
    }

    pub fn search(&mut self, files: &[PathBuf]) {
        if self.query.is_empty() {
            self.results.clear();
            return;
        }

        let pattern = Pattern::parse(&self.query, CaseMatching::Ignore, Normalization::Smart);

        self.results = files.iter()
            .filter_map(|path| {
                let name = path.file_name()?.to_string_lossy();
                let haystack = nucleo_matcher::Utf32String::from(name.as_ref());
                let mut indices = Vec::new();

                pattern.indices(haystack.slice(..), &mut self.matcher, &mut indices)
                    .map(|score| SearchResult {
                        path: path.clone(),
                        score,
                        indices,
                    })
            })
            .collect();

        // Sort by score descending
        self.results.sort_by(|a, b| b.score.cmp(&a.score));
        self.results.truncate(20); // Max 20 results
        self.selected = 0;
    }

    pub fn render_highlighted(&self, result: &SearchResult) -> Spans<'static> {
        let name = result.path.file_name().unwrap().to_string_lossy();
        let mut spans = Vec::new();
        let mut last_idx = 0;

        for &idx in &result.indices {
            let idx = idx as usize;
            if idx > last_idx {
                spans.push(Span::raw(name[last_idx..idx].to_string()));
            }
            spans.push(Span::styled(
                name[idx..idx+1].to_string(),
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            ));
            last_idx = idx + 1;
        }

        if last_idx < name.len() {
            spans.push(Span::raw(name[last_idx..].to_string()));
        }

        Spans::from(spans)
    }
}
```

**UI Overlay:**
```
╭─────────────────────────────────────────────╮
│ 🔍 seo                                      │
├─────────────────────────────────────────────┤
│ ▶ 🚂 [seo]-pipeline.nika.yaml               │
│   🚂 re[seo]arch.nika.yaml                  │
│   🎯 [seo].skill.yaml                       │
╰─────────────────────────────────────────────╯
```

### Breadcrumb Navigation (v0.12)

**Sticky header at top of Panel A:**
```
╭────────────────────────────────────────────────────────────────╮
│  📁 qrcode-ai / 🪽 .nika / 📁 agents / 🐔 researcher.agent    │
╰────────────────────────────────────────────────────────────────╯
```

**Implementation:**
```rust
pub struct Breadcrumb {
    segments: Vec<BreadcrumbSegment>,
}

pub struct BreadcrumbSegment {
    name: String,
    kind: FileKind,
    path: PathBuf,
}

impl Breadcrumb {
    pub fn from_path(path: &Path, root: &Path) -> Self {
        let relative = path.strip_prefix(root).unwrap_or(path);
        let segments = relative.ancestors()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .skip(1) // Skip empty root
            .map(|p| {
                let name = p.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                let kind = FileKind::from_path(&root.join(p));
                BreadcrumbSegment { name, kind, path: root.join(p) }
            })
            .collect();

        Self { segments }
    }

    pub fn render(&self) -> Spans<'static> {
        let mut spans = Vec::new();

        for (i, seg) in self.segments.iter().enumerate() {
            if i > 0 {
                spans.push(Span::styled(" / ", Style::default().fg(Color::DarkGray)));
            }
            spans.push(Span::raw(format!("{} {}", seg.kind.icon(), seg.name)));
        }

        Spans::from(spans)
    }
}
```

### Minimap Sidebar (v0.12)

**2-char wide visual scroll indicator:**
```
┌──┬────────────────────────────────┐
│▓▓│  📁 qrcode-ai/                 │  ← Viewport top
│░░│  ├─ 🪽 .nika/                  │
│░░│  │  ├─ 📁 agents/              │
│██│  │  │  └─ 🐔 researcher ← sel  │  ← Current selection
│░░│  │  ├─ 📁 skills/              │
│░░│  │  └─ 📁 context/             │
│░░│  ├─ 📁 workflows/              │
│░░│  │  ├─ 🚂 seo-pipeline         │
│░░│  │  └─ 🚂 multi-agent          │
│░░│  └─ 📄 package.json            │
└──┴────────────────────────────────┘
     ▲
     │
   Minimap (2 chars)
```

**Legend:**
- `▓▓` = Visible viewport
- `██` = Current selection
- `░░` = Scrollable content (outside viewport)

---

## View 2: CHAT

### Purpose

Conversational AI interface avec **Chat-as-DAG** integration. Chaque message devient un Task node dans un DAG live.

### Layout (4 Panels)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              HEADER (3%)                                    │
├─────────────────────────────────────────┬───────────────────────────────────┤
│                                         │                                   │
│           PANEL A                       │          PANEL B                  │
│       CONVERSATION                      │      MISSION CONTROL              │
│                                         │  ┌───────────────────────────┐    │
│           55%                           │  │    LIVE DAG (50%)         │    │  72%
│                                         │  ├───────────────────────────┤    │
│                                         │  │   YAML PREVIEW (50%)      │    │
│                                         │  └───────────────────────────┘    │
│                                         │           45%                     │
├─────────────────────────────────────────┼───────────────────────────────────┤
│           PANEL C                       │          PANEL D                  │
│       INPUT FIELD                       │      QUICK ACTIONS                │  22%
├─────────────────────────────────────────┴───────────────────────────────────┤
│                            STATUS BAR (3%)                                  │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Chat-as-DAG Semantics

**Core Concept:** Each chat message becomes a `ChatTask` node in a `StableGraph`.

```rust
use petgraph::stable_graph::{StableGraph, NodeIndex};

pub struct ChatWorkflow {
    pub graph: StableGraph<ChatTask, ChatEdge>,
    pub message_to_node: HashMap<MessageId, NodeIndex>,
    pub root: Option<NodeIndex>,
}

#[derive(Clone)]
pub struct ChatTask {
    pub id: MessageId,
    pub verb: VerbKind,
    pub input: String,
    pub output: Option<String>,
    pub status: TaskStatus,
    pub metadata: TaskMetadata,
}

#[derive(Clone)]
pub struct ChatEdge {
    pub kind: EdgeKind,
}

#[derive(Clone)]
pub enum EdgeKind {
    Sequential,   // Normal message (depends on previous)
    Reference,    // @N reference
    Fork,         // // parallel fork
    FanIn,        // Multiple @N references converge
}
```

**Input Patterns:**

| Pattern | Meaning | DAG Behavior |
|---------|---------|--------------|
| Normal text | Sequential | Edge from previous message |
| `// text` | Fork | No edge from previous (parallel) |
| `@1` | Reference msg-001 | Edge from msg-001 |
| `@last` | Reference previous | Edge from previous |
| `@all` | Fan-in all | Edges from all previous |
| `@1 @3` | Multi-ref | Edges from msg-001 and msg-003 |

**Parsing:**
```rust
pub struct MessageParser;

impl MessageParser {
    pub fn parse(input: &str) -> ParsedMessage {
        let trimmed = input.trim();

        // Check for fork prefix
        let (is_fork, content) = if trimmed.starts_with("//") {
            (true, trimmed[2..].trim())
        } else {
            (false, trimmed)
        };

        // Extract @references
        let re = Regex::new(r"@(\d+|last|all|prev)").unwrap();
        let references: Vec<Reference> = re.captures_iter(content)
            .map(|cap| {
                match &cap[1] {
                    "last" | "prev" => Reference::Last,
                    "all" => Reference::All,
                    n => Reference::Index(n.parse().unwrap()),
                }
            })
            .collect();

        // Determine verb (default: infer)
        let verb = if content.starts_with("/exec") {
            VerbKind::Exec
        } else if content.starts_with("/fetch") {
            VerbKind::Fetch
        } else if content.starts_with("/invoke") {
            VerbKind::Invoke
        } else if content.starts_with("/agent") {
            VerbKind::Agent
        } else {
            VerbKind::Infer
        };

        ParsedMessage {
            is_fork,
            references,
            verb,
            content: content.to_string(),
        }
    }
}

pub enum Reference {
    Index(usize),
    Last,
    All,
}
```

### Panel A: Conversation (55% × 72%)

**Message Box (NodeBox) — 5-line compact:**
```
╭─────────────────────────────────────────────╮
│ ⚡ msg-001          ✓ 3.2s    📊 1.2K→0.8K │
│ 🧠 claude-sonnet                           │
│ 💬 "Describe QR Code entity"               │
│ 📤 "QR codes are 2D barcodes that..."      │
│ 🔗 use.prev: null                          │
╰─────────────────────────────────────────────╯
```

**NodeBox Structure:**
```rust
pub struct NodeBox {
    pub task: ChatTask,
    pub expanded: bool, // Toggle full output
}

impl NodeBox {
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(self.border_style());

        let inner = block.inner(area);
        block.render(area, buf);

        // Line 1: ID + Status + Time + Tokens
        let line1 = format!(
            "{} {}          {} {}    📊 {}→{}",
            self.task.verb.icon(),
            self.task.id,
            self.task.status.icon(),
            self.format_duration(),
            self.task.metadata.input_tokens,
            self.task.metadata.output_tokens.unwrap_or(0),
        );

        // Line 2: Provider
        let line2 = format!("🧠 {}", self.task.metadata.model);

        // Line 3: Input (truncated)
        let line3 = format!("💬 \"{}\"", truncate(&self.task.input, 35));

        // Line 4: Output (truncated or streaming)
        let line4 = if let Some(output) = &self.task.output {
            format!("📤 \"{}\"", truncate(output, 35))
        } else if self.task.status == TaskStatus::Running {
            format!("📤 ▌ {}...", self.task.metadata.streaming_text.as_deref().unwrap_or(""))
        } else {
            "📤 —".to_string()
        };

        // Line 5: Bindings
        let line5 = format!("🔗 {}", self.format_bindings());

        // Render lines
        for (i, line) in [line1, line2, line3, line4, line5].iter().enumerate() {
            let y = inner.y + i as u16;
            if y < inner.bottom() {
                buf.set_string(inner.x, y, line, Style::default());
            }
        }
    }

    fn border_style(&self) -> Style {
        match self.task.status {
            TaskStatus::Completed => Style::default().fg(Color::Green),
            TaskStatus::Running => Style::default().fg(Color::Blue),
            TaskStatus::Failed => Style::default().fg(Color::Red),
            TaskStatus::Pending => Style::default().fg(Color::DarkGray),
        }
    }
}
```

### Panel B: Mission Control (45% × 72%)

Split vertically 50/50:
- **Top (50%):** Live DAG
- **Bottom (50%):** YAML Preview

**Live DAG:**
```rust
pub struct LiveDag {
    workflow: ChatWorkflow,
    layout: DagLayout,
    animation: AnimationState,
}

impl LiveDag {
    pub fn on_message_added(&mut self, node_idx: NodeIndex) {
        // Recalculate layout
        self.layout.recompute(&self.workflow.graph);

        // Trigger animation
        self.animation.node_enter(node_idx);
    }

    pub fn on_status_change(&mut self, node_idx: NodeIndex, status: TaskStatus) {
        match status {
            TaskStatus::Running => self.animation.pulse_start(node_idx),
            TaskStatus::Completed => self.animation.checkmark_flash(node_idx),
            TaskStatus::Failed => self.animation.error_shake(node_idx),
            _ => {}
        }
    }
}
```

**YAML Preview:**
Auto-generated from chat messages in real-time.

```rust
impl ChatWorkflow {
    pub fn to_yaml(&self) -> String {
        let mut output = String::new();

        output.push_str("schema: \"nika/workflow@0.6\"\n");
        output.push_str(&format!("workflow: chat-{}\n", self.session_id));
        output.push_str("description: \"Auto-generated from chat session\"\n\n");

        output.push_str("tasks:\n");

        for node_idx in self.graph.node_indices() {
            let task = &self.graph[node_idx];
            output.push_str(&format!("  - id: {}\n", task.id));

            match task.verb {
                VerbKind::Infer => {
                    output.push_str(&format!("    infer: \"{}\"\n", escape_yaml(&task.input)));
                }
                VerbKind::Exec => {
                    output.push_str(&format!("    exec: \"{}\"\n", escape_yaml(&task.input)));
                }
                // ... other verbs
            }

            // Add needs: if there are incoming edges
            let deps: Vec<_> = self.graph
                .neighbors_directed(node_idx, petgraph::Direction::Incoming)
                .map(|n| self.graph[n].id.clone())
                .collect();

            if !deps.is_empty() {
                output.push_str(&format!("    needs: [{}]\n", deps.join(", ")));
            }

            output.push_str("\n");
        }

        output
    }
}
```

### Panel C: Input Field (55% × 22%)

**Component:** Single-line input with hints

```
┌─────────────────────────────────────────────┐
│ 🦋 > Combine @1 and @2 into landing page▌  │
└─────────────────────────────────────────────┘
HINTS: // fork │ @N ref │ Enter send │ Esc clear
```

**Features:**
- Cursor with blinking `▌`
- Auto-complete for @references
- Command palette trigger with `/`

### Panel D: Quick Actions (45% × 22%)

```
[💾 Export] [🔄 Reset] [📋 Copy] [⏸ Pause]
```

| Action | Shortcut | Description |
|--------|----------|-------------|
| Export | `Ctrl+S` | Save as `.nika.yaml` |
| Reset | `Ctrl+R` | Clear conversation |
| Copy | `Ctrl+C` | Copy YAML to clipboard |
| Pause | `Ctrl+Z` | Pause current task |

### Input Commands

| Command | Verb | Example |
|---------|------|---------|
| (default) | `infer:` | `Generate a headline` |
| `/exec` | `exec:` | `/exec npm run build` |
| `/fetch` | `fetch:` | `/fetch GET https://api.example.com` |
| `/invoke` | `invoke:` | `/invoke novanet_describe entity:qr-code` |
| `/agent` | `agent:` | `/agent Research QR code market` |

### Export Flow (Chat → Editor)

```
┌──────────────┐  /export  ┌──────────────┐  confirm  ┌──────────────┐
│   CHAT       │──────────▶│   DIALOG     │──────────▶│   EDITOR     │
│  [DAG Live]  │           │  Save as...  │           │  [Loaded]    │
└──────────────┘           └──────────────┘           └──────────────┘
                                  │
                                  ▼
                           .nika/exports/
                           chat-<session>.nika.yaml
```

---

## View 3: EDITOR

### Purpose

YAML editing avec syntax highlighting et DAG preview synchronisé.

### Layout (2 Panels Split)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              HEADER (3%)                                    │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌────────────────────────────────┬────────────────────────────────────┐    │
│  │      YAML EDITOR (60%)         │         DAG PREVIEW (40%)          │    │
│  │                                │                                    │    │  72%
│  │  Line numbers + Syntax         │     Real-time graph rendering      │    │
│  │                                │                                    │    │
│  └────────────────────────────────┴────────────────────────────────────┘    │
│                                                                             │
├─────────────────────────────────────────────────────────────────────────────┤
│                      DIAGNOSTICS + ACTIONS (22%)                            │
├─────────────────────────────────────────────────────────────────────────────┤
│                            STATUS BAR (3%)                                  │
└─────────────────────────────────────────────────────────────────────────────┘
```

### YAML Editor (60%)

**Features:**
- Line numbers (4-char width, right-aligned)
- Syntax highlighting (syntect with Solarized theme)
- Cursor tracking with blinking
- Selection highlighting
- Undo/Redo (v0.8.0 EditHistory)
- Auto-indent on Enter
- Bracket matching

**Verb Icon Display:**
```rust
// Replace verb keywords with icons in display
fn render_line_with_icons(line: &str) -> Spans<'static> {
    let mut spans = Vec::new();

    // Detect verb patterns
    if let Some(idx) = line.find("infer:") {
        spans.push(Span::raw(line[..idx].to_string()));
        spans.push(Span::styled("⚡infer:", Style::default().fg(Color::Magenta)));
        spans.push(Span::raw(line[idx + 6..].to_string()));
    } else if let Some(idx) = line.find("exec:") {
        spans.push(Span::raw(line[..idx].to_string()));
        spans.push(Span::styled("📟exec:", Style::default().fg(Color::Yellow)));
        spans.push(Span::raw(line[idx + 5..].to_string()));
    } else if let Some(idx) = line.find("fetch:") {
        spans.push(Span::raw(line[..idx].to_string()));
        spans.push(Span::styled("🛰️fetch:", Style::default().fg(Color::Cyan)));
        spans.push(Span::raw(line[idx + 6..].to_string()));
    } else if let Some(idx) = line.find("invoke:") {
        spans.push(Span::raw(line[..idx].to_string()));
        spans.push(Span::styled("🔌invoke:", Style::default().fg(Color::Green)));
        spans.push(Span::raw(line[idx + 7..].to_string()));
    } else if let Some(idx) = line.find("agent:") {
        spans.push(Span::raw(line[..idx].to_string()));
        spans.push(Span::styled("🐔agent:", Style::default().fg(Color::Red)));
        spans.push(Span::raw(line[idx + 6..].to_string()));
    } else {
        spans.push(Span::raw(line.to_string()));
    }

    Spans::from(spans)
}
```

### DAG Preview (40%)

**Component:** `DagPanel` (shared widget)

**Cursor-DAG Sync:**
```rust
impl EditorView {
    pub fn on_cursor_move(&mut self, line: usize) {
        // Find which task the cursor is on
        if let Some(task_id) = self.line_to_task.get(&line) {
            self.dag_panel.highlight_node(task_id);
        } else {
            self.dag_panel.clear_highlight();
        }
    }

    pub fn on_dag_click(&mut self, task_id: &str) {
        // Jump to task in editor
        if let Some(line) = self.task_to_line.get(task_id) {
            self.cursor.line = *line;
            self.ensure_cursor_visible();
        }
    }
}
```

**Real-time Update (Debounced):**
```rust
impl EditorView {
    pub fn on_content_change(&mut self) {
        // Debounce: only reparse after 100ms of no typing
        self.reparse_timer = Some(Instant::now() + Duration::from_millis(100));
    }

    pub fn tick(&mut self) {
        if let Some(timer) = self.reparse_timer {
            if Instant::now() >= timer {
                self.reparse_timer = None;
                self.reparse_workflow();
            }
        }
    }

    fn reparse_workflow(&mut self) {
        match parse_workflow_yaml(&self.content) {
            Ok(workflow) => {
                self.dag_panel.update_from_workflow(&workflow);
                self.diagnostics.clear_errors();
                self.line_to_task = build_line_mapping(&workflow);
            }
            Err(e) => {
                // Keep last valid DAG, show error
                self.diagnostics.add_error(e);
            }
        }
    }
}
```

### Diagnostics Panel (22%)

**Content:**
```
📊 DIAGNOSTICS
──────────────────────────────────────────────────────────────────────
⚠ Warning (line 25): agent: without max_turns may run indefinitely.
ℹ Info: 4 tasks detected │ MCP servers: novanet │ Estimated tokens: ~8K

[💾 Save] [▶ Run] [✓ Validate] [↩ Undo] [↪ Redo] │ Ln 10, Col 5 │ UTF-8
```

**Diagnostic Levels:**
```rust
pub enum DiagnosticLevel {
    Error,   // ✗ Red - Invalid schema, parse error
    Warning, // ⚠ Yellow - Recommendations
    Info,    // ℹ Blue - Statistics
}

pub struct Diagnostic {
    pub level: DiagnosticLevel,
    pub line: Option<usize>,
    pub message: String,
    pub suggestion: Option<String>,
}
```

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Ctrl+S` | Save file |
| `Ctrl+Z` | Undo |
| `Ctrl+Y` | Redo |
| `F5` | Run workflow |
| `F6` | Validate schema |
| `Ctrl+F` | Find in file |
| `Ctrl+G` | Go to line |
| `Ctrl+\` | Toggle DAG panel |
| `Ctrl+Shift+F` | Format YAML |

---

## View 4: RUNNER

### Purpose

Workflow execution monitor avec DAG animé et streaming output.

### Layout (4 Panels)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              HEADER (3%)                                    │
├────────────────────────────────┬────────────────────────────────────────────┤
│                                │                                            │
│         PANEL A                │           PANEL B                          │
│      ANIMATED DAG              │        TASK OUTPUT                         │  55%
│          50%                   │            50%                             │
│                                │                                            │
├────────────────────────────────┼────────────────────────────────────────────┤
│        PANEL C                 │              PANEL D                       │
│     MCP CALL LOG               │           METRICS + CONTROLS               │  39%
├────────────────────────────────┴────────────────────────────────────────────┤
│                            STATUS BAR (3%)                                  │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Panel A: Animated DAG (50% × 55%)

**Animation States:**

| State | Border | Fill | Animation |
|-------|--------|------|-----------|
| Pending (⏳) | Gray dashed | None | Static |
| Running (◐) | Blue solid | Blue 20% | Pulsing glow (500ms) |
| Streaming (◐) | Blue solid | Blue 20% | Spinner rotation |
| Completed (✓) | Green solid | Green 10% | Checkmark flash (200ms) |
| Failed (✗) | Red solid | Red 10% | Shake animation (300ms) |

**Animation Implementation:**
```rust
pub struct AnimationState {
    pub nodes: HashMap<NodeIndex, NodeAnimation>,
    pub edges: HashMap<(NodeIndex, NodeIndex), EdgeAnimation>,
    pub frame: u64,
}

pub struct NodeAnimation {
    pub kind: AnimationKind,
    pub started_at: Instant,
    pub duration: Duration,
}

pub enum AnimationKind {
    None,
    Pulse,          // Running state
    Spinner,        // Streaming state
    CheckmarkFlash, // Completion
    ErrorShake,     // Failure
    FadeIn,         // Node appears
}

impl AnimationState {
    pub fn tick(&mut self) {
        self.frame = self.frame.wrapping_add(1);

        // Clean up finished animations
        self.nodes.retain(|_, anim| {
            anim.started_at.elapsed() < anim.duration || anim.kind == AnimationKind::Pulse
        });
    }

    pub fn render_node(&self, node_idx: NodeIndex, base_style: Style) -> Style {
        let Some(anim) = self.nodes.get(&node_idx) else {
            return base_style;
        };

        let elapsed = anim.started_at.elapsed();
        let progress = (elapsed.as_millis() as f64) / (anim.duration.as_millis() as f64);

        match anim.kind {
            AnimationKind::Pulse => {
                // Sine wave opacity between 0.5 and 1.0
                let alpha = 0.5 + 0.5 * (self.frame as f64 * 0.1).sin();
                base_style.fg(Color::Rgb(
                    (66.0 + alpha * 100.0) as u8,
                    (153.0 + alpha * 50.0) as u8,
                    (225.0) as u8,
                ))
            }
            AnimationKind::CheckmarkFlash => {
                // Flash bright green then fade
                let brightness = 255.0 * (1.0 - progress);
                base_style.fg(Color::Rgb(brightness as u8, 255, brightness as u8))
            }
            AnimationKind::ErrorShake => {
                // Handled in position, not style
                base_style.fg(Color::Red)
            }
            _ => base_style,
        }
    }

    pub fn position_offset(&self, node_idx: NodeIndex) -> (i16, i16) {
        let Some(anim) = self.nodes.get(&node_idx) else {
            return (0, 0);
        };

        if anim.kind == AnimationKind::ErrorShake {
            let elapsed = anim.started_at.elapsed().as_millis() as f64;
            let shake = (elapsed * 0.1).sin() * 2.0;
            return (shake as i16, 0);
        }

        (0, 0)
    }
}
```

**Edge Animation (Data Flow):**
```rust
pub struct EdgeAnimation {
    pub active: bool,
    pub progress: f32, // 0.0 to 1.0
}

impl DagPanel {
    fn render_edge(&self, from: (u16, u16), to: (u16, u16), anim: &EdgeAnimation, buf: &mut Buffer) {
        let style = if anim.active {
            Style::default().fg(Color::Blue)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        // Draw line from `from` to `to`
        // If active, show progress marker
        if anim.active && anim.progress > 0.0 && anim.progress < 1.0 {
            let marker_x = from.0 + ((to.0 - from.0) as f32 * anim.progress) as u16;
            let marker_y = from.1 + ((to.1 - from.1) as f32 * anim.progress) as u16;
            buf.get_mut(marker_x, marker_y).set_char('●').set_style(Style::default().fg(Color::Yellow));
        }
    }
}
```

### Panel B: Task Output (50% × 55%)

**Streaming Display:**
```rust
pub struct TaskOutput {
    pub tasks: Vec<TaskOutputEntry>,
    pub selected: Option<usize>,
    pub scroll: u16,
}

pub struct TaskOutputEntry {
    pub task_id: String,
    pub verb: VerbKind,
    pub status: TaskStatus,
    pub duration: Duration,
    pub output: OutputContent,
    pub expanded: bool,
}

pub enum OutputContent {
    Text(String),
    Streaming { current: String, cursor: bool },
    Json(serde_json::Value),
    Error(String),
}

impl TaskOutputEntry {
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        // Header line
        let header = format!(
            "┌─ {} {} ──────────────────── {} {} ───┐",
            self.verb.icon(),
            self.task_id,
            self.status.icon(),
            format_duration(self.duration),
        );
        buf.set_string(area.x, area.y, &header, Style::default().fg(Color::Cyan));

        // Content
        match &self.output {
            OutputContent::Text(text) => {
                let lines: Vec<_> = text.lines().take(if self.expanded { 100 } else { 3 }).collect();
                for (i, line) in lines.iter().enumerate() {
                    let y = area.y + 1 + i as u16;
                    if y < area.bottom() - 1 {
                        buf.set_string(area.x + 1, y, line, Style::default());
                    }
                }
            }
            OutputContent::Streaming { current, cursor } => {
                let display = if *cursor {
                    format!("{}▌", current)
                } else {
                    current.clone()
                };
                buf.set_string(area.x + 1, area.y + 1, &display, Style::default().fg(Color::Blue));
            }
            OutputContent::Json(value) => {
                let formatted = serde_json::to_string_pretty(value).unwrap();
                for (i, line) in formatted.lines().take(5).enumerate() {
                    let y = area.y + 1 + i as u16;
                    if y < area.bottom() - 1 {
                        buf.set_string(area.x + 1, y, line, Style::default().fg(Color::Green));
                    }
                }
            }
            OutputContent::Error(msg) => {
                buf.set_string(area.x + 1, area.y + 1, msg, Style::default().fg(Color::Red));
            }
        }

        // Footer
        let footer = "└───────────────────────────────────────────┘";
        buf.set_string(area.x, area.bottom() - 1, footer, Style::default().fg(Color::DarkGray));
    }
}
```

**Progress Bar:**
```
Progress: ████████████░░░░ 75% (3/4 tasks)
```

```rust
pub fn render_progress(completed: usize, total: usize, width: u16) -> Spans<'static> {
    let pct = (completed as f64 / total as f64 * 100.0) as u8;
    let filled = (width as f64 * completed as f64 / total as f64) as u16;
    let empty = width - filled;

    Spans::from(vec![
        Span::raw("Progress: "),
        Span::styled("█".repeat(filled as usize), Style::default().fg(Color::Green)),
        Span::styled("░".repeat(empty as usize), Style::default().fg(Color::DarkGray)),
        Span::raw(format!(" {}% ({}/{} tasks)", pct, completed, total)),
    ])
}
```

### Panel C: MCP Call Log (50% × 39%)

**Format:**
```
🔌 MCP CALL LOG
──────────────────────────────────────────────────────────────
14:32:01 │ novanet_generate │ params: entity=qr-code, locale=en
14:32:03 │ ← response │ 1.2KB │ 200ms
14:32:03 │ novanet_describe │ entity: qr-code
14:32:04 │ ◐ awaiting response...
```

**Data Structure:**
```rust
pub struct McpCallLog {
    pub entries: VecDeque<McpLogEntry>,
    pub max_entries: usize,
}

pub struct McpLogEntry {
    pub timestamp: DateTime<Local>,
    pub kind: McpLogKind,
}

pub enum McpLogKind {
    Request { tool: String, params: String },
    Response { size: usize, duration: Duration },
    Pending { tool: String },
    Error { message: String },
}

impl McpLogEntry {
    pub fn render(&self) -> Spans<'static> {
        let time = self.timestamp.format("%H:%M:%S").to_string();

        match &self.kind {
            McpLogKind::Request { tool, params } => {
                Spans::from(vec![
                    Span::styled(time, Style::default().fg(Color::DarkGray)),
                    Span::raw(" │ "),
                    Span::styled(tool, Style::default().fg(Color::Cyan)),
                    Span::raw(" │ params: "),
                    Span::raw(truncate(params, 40)),
                ])
            }
            McpLogKind::Response { size, duration } => {
                Spans::from(vec![
                    Span::styled(time, Style::default().fg(Color::DarkGray)),
                    Span::raw(" │ "),
                    Span::styled("← response", Style::default().fg(Color::Green)),
                    Span::raw(format!(" │ {} │ {}ms", format_bytes(*size), duration.as_millis())),
                ])
            }
            McpLogKind::Pending { tool } => {
                Spans::from(vec![
                    Span::styled(time, Style::default().fg(Color::DarkGray)),
                    Span::raw(" │ "),
                    Span::styled("◐", Style::default().fg(Color::Blue)),
                    Span::raw(format!(" awaiting {}...", tool)),
                ])
            }
            McpLogKind::Error { message } => {
                Spans::from(vec![
                    Span::styled(time, Style::default().fg(Color::DarkGray)),
                    Span::raw(" │ "),
                    Span::styled("✗ error", Style::default().fg(Color::Red)),
                    Span::raw(format!(": {}", truncate(message, 40))),
                ])
            }
        }
    }
}
```

### Panel D: Metrics + Controls (50% × 39%)

**Metrics:**
```
Tokens: 3.2K in │ 2.1K out │ $0.18
Time: 6.8s elapsed │ ~4s remaining
Tasks: 2 ✓ │ 1 ◐ │ 1 ⏳
```

**Controls:**
```
[⏸ Pause] [⏹ Stop] [🔄 Restart] [📋 Copy]
```

**Data Structure:**
```rust
pub struct ExecutionMetrics {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub estimated_cost: f64,
    pub elapsed: Duration,
    pub eta: Option<Duration>,
    pub tasks_completed: usize,
    pub tasks_running: usize,
    pub tasks_pending: usize,
    pub tasks_failed: usize,
}

impl ExecutionMetrics {
    pub fn render(&self) -> Vec<Spans<'static>> {
        vec![
            Spans::from(format!(
                "Tokens: {} in │ {} out │ ${:.2}",
                format_tokens(self.input_tokens),
                format_tokens(self.output_tokens),
                self.estimated_cost,
            )),
            Spans::from(format!(
                "Time: {:.1}s elapsed │ {}",
                self.elapsed.as_secs_f64(),
                self.eta.map(|d| format!("~{:.0}s remaining", d.as_secs_f64()))
                    .unwrap_or_else(|| "calculating...".to_string()),
            )),
            Spans::from(vec![
                Span::raw("Tasks: "),
                Span::styled(format!("{} ✓", self.tasks_completed), Style::default().fg(Color::Green)),
                Span::raw(" │ "),
                Span::styled(format!("{} ◐", self.tasks_running), Style::default().fg(Color::Blue)),
                Span::raw(" │ "),
                Span::styled(format!("{} ⏳", self.tasks_pending), Style::default().fg(Color::DarkGray)),
                if self.tasks_failed > 0 {
                    Span::styled(format!(" │ {} ✗", self.tasks_failed), Style::default().fg(Color::Red))
                } else {
                    Span::raw("")
                },
            ]),
        ]
    }
}
```

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Space` | Pause/Resume |
| `Ctrl+C` | Abort execution |
| `Tab` | Cycle panels |
| `↑↓` | Scroll output/log |
| `Enter` | Expand/collapse task |
| `r` | Restart workflow |

---

## View 5: SCHEDULER

### Purpose

Cron job management pour workflow automation avec timeline visuelle.

### Layout (4 Panels)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              HEADER (3%)                                    │
├───────────────────────────────────┬─────────────────────────────────────────┤
│                                   │                                         │
│         PANEL A                   │              PANEL B                    │
│     SCHEDULE LIST                 │           TIMELINE VIEW                 │  47%
│          40%                      │               60%                       │
│                                   │                                         │
├───────────────────────────────────┼─────────────────────────────────────────┤
│         PANEL C                   │              PANEL D                    │
│      RUN HISTORY                  │          CRON EDITOR                    │  47%
│          40%                      │               60%                       │
├───────────────────────────────────┴─────────────────────────────────────────┤
│                            STATUS BAR (3%)                                  │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Panel A: Schedule List (40% × 47%)

**Data Structure:**
```rust
use cron::Schedule;
use chrono::{DateTime, Local, Utc};
use std::str::FromStr;

pub struct ScheduledWorkflow {
    pub id: String,
    pub workflow_path: PathBuf,
    pub cron_expression: String,
    pub schedule: Schedule,
    pub enabled: bool,
    pub priority: Priority,
    pub timeout: Duration,
    pub last_run: Option<RunResult>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Critical = 4,
    High = 3,
    Normal = 2,
    Low = 1,
}

impl Priority {
    pub fn color(&self) -> Color {
        match self {
            Priority::Critical => Color::Red,
            Priority::High => Color::Rgb(255, 165, 0), // Orange
            Priority::Normal => Color::Blue,
            Priority::Low => Color::DarkGray,
        }
    }
}

pub struct RunResult {
    pub timestamp: DateTime<Utc>,
    pub duration: Duration,
    pub status: RunStatus,
    pub cost: f64,
}

pub enum RunStatus {
    Success,
    Failed(String),
    Timeout,
    Cancelled,
}

impl ScheduledWorkflow {
    pub fn next_run(&self) -> Option<DateTime<Local>> {
        if !self.enabled {
            return None;
        }
        self.schedule.upcoming(Local).next()
    }

    pub fn time_until_next(&self) -> Option<Duration> {
        self.next_run().map(|next| {
            let now = Local::now();
            if next > now {
                (next - now).to_std().unwrap_or_default()
            } else {
                Duration::ZERO
            }
        })
    }
}
```

**Schedule Card:**
```
┌─ 🚂 seo-pipeline ─────────────────────┐
│ 📆 Every 6h  │ */6 * * * *             │
│ ▶ Next: 14:00:00 (in 1h 23m)          │
│ ✓ Last: 08:00:00 (success)            │
└───────────────────────────────────────┘
```

```rust
impl ScheduledWorkflow {
    pub fn render_card(&self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(format!("🚂 {}", self.workflow_path.file_stem().unwrap().to_string_lossy()))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded);

        let inner = block.inner(area);
        block.render(area, buf);

        // Line 1: Schedule
        let cron_human = self.humanize_cron();
        buf.set_string(
            inner.x,
            inner.y,
            format!("📆 {}  │ {}", cron_human, self.cron_expression),
            Style::default(),
        );

        // Line 2: Next run
        if let Some(next) = self.next_run() {
            let until = self.time_until_next().unwrap();
            buf.set_string(
                inner.x,
                inner.y + 1,
                format!("▶ Next: {} (in {})", next.format("%H:%M:%S"), format_duration_human(until)),
                Style::default().fg(Color::Green),
            );
        } else {
            buf.set_string(
                inner.x,
                inner.y + 1,
                "⏸ Paused",
                Style::default().fg(Color::Yellow),
            );
        }

        // Line 3: Last run
        if let Some(last) = &self.last_run {
            let status_icon = match &last.status {
                RunStatus::Success => "✓",
                RunStatus::Failed(_) => "✗",
                RunStatus::Timeout => "⏱",
                RunStatus::Cancelled => "⏹",
            };
            let style = match &last.status {
                RunStatus::Success => Style::default().fg(Color::Green),
                _ => Style::default().fg(Color::Red),
            };
            buf.set_string(
                inner.x,
                inner.y + 2,
                format!("{} Last: {} ({})", status_icon, last.timestamp.format("%H:%M:%S"),
                    match &last.status {
                        RunStatus::Success => "success".to_string(),
                        RunStatus::Failed(e) => format!("failed: {}", truncate(e, 20)),
                        RunStatus::Timeout => "timeout".to_string(),
                        RunStatus::Cancelled => "cancelled".to_string(),
                    }
                ),
                style,
            );
        }
    }

    fn humanize_cron(&self) -> String {
        // Common patterns
        match self.cron_expression.as_str() {
            "* * * * *" => "Every minute".to_string(),
            "0 * * * *" => "Every hour".to_string(),
            "0 0 * * *" => "Daily at midnight".to_string(),
            s if s.starts_with("*/") => {
                let interval: u32 = s[2..].split_whitespace().next()
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(1);
                format!("Every {}h", interval)
            }
            s if s.contains("* * 1-5") => "Weekdays".to_string(),
            _ => "Custom".to_string(),
        }
    }
}
```

### Panel B: Timeline View (60% × 47%)

**Timeline Rendering (Using ratatui Canvas):**

```rust
use ratatui::widgets::canvas::{Canvas, Line, Rectangle};

pub struct TimelineView {
    pub schedules: Vec<ScheduledWorkflow>,
    pub view_start: DateTime<Local>,
    pub view_end: DateTime<Local>,
    pub now_marker: bool,
}

impl TimelineView {
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        let hours = (self.view_end - self.view_start).num_hours() as f64;
        let width = area.width as f64;

        // Draw time axis
        for hour in 0..=hours as i64 {
            let x = (hour as f64 / hours * width) as u16;
            let time = self.view_start + chrono::Duration::hours(hour);
            buf.set_string(
                area.x + x,
                area.y,
                time.format("%H:%M").to_string(),
                Style::default().fg(Color::DarkGray),
            );

            // Vertical tick
            buf.get_mut(area.x + x, area.y + 1).set_char('│');
        }

        // Draw schedule bars
        for (i, schedule) in self.schedules.iter().enumerate() {
            let y = area.y + 2 + (i as u16 * 2);

            // Get upcoming occurrences in view
            for next in schedule.schedule.upcoming(Local).take(10) {
                if next >= self.view_end {
                    break;
                }
                if next < self.view_start {
                    continue;
                }

                let offset = (next - self.view_start).num_minutes() as f64
                    / (self.view_end - self.view_start).num_minutes() as f64;
                let x = area.x + (offset * width) as u16;

                // Draw bar (fixed width representing estimated duration)
                let bar_width = 8;
                for dx in 0..bar_width {
                    if x + dx < area.right() {
                        buf.get_mut(x + dx, y)
                            .set_char('█')
                            .set_style(Style::default().fg(schedule.priority.color()));
                    }
                }

                // Label
                let name = schedule.workflow_path.file_stem().unwrap().to_string_lossy();
                buf.set_string(x, y + 1, truncate(&name, 10), Style::default().fg(Color::White));
            }
        }

        // Draw "now" marker
        if self.now_marker {
            let now = Local::now();
            if now >= self.view_start && now < self.view_end {
                let offset = (now - self.view_start).num_minutes() as f64
                    / (self.view_end - self.view_start).num_minutes() as f64;
                let x = area.x + (offset * width) as u16;

                for y in area.y..area.bottom() {
                    buf.get_mut(x, y)
                        .set_char('│')
                        .set_style(Style::default().fg(Color::Red));
                }
                buf.set_string(x.saturating_sub(2), area.bottom() - 1, "◄─now", Style::default().fg(Color::Red));
            }
        }
    }
}
```

**Alternative: Sparkline for Quick View:**
```rust
// Show next 24 hours as sparkline
pub fn render_sparkline(&self, area: Rect, buf: &mut Buffer) {
    // Count jobs per hour
    let now = Local::now();
    let mut counts = [0u64; 24];

    for schedule in &self.schedules {
        if !schedule.enabled {
            continue;
        }
        for next in schedule.schedule.upcoming(Local).take(100) {
            let hours_from_now = (next - now).num_hours();
            if hours_from_now >= 0 && hours_from_now < 24 {
                counts[hours_from_now as usize] += 1;
            }
        }
    }

    let sparkline = Sparkline::default()
        .data(&counts)
        .max(counts.iter().max().copied().unwrap_or(1))
        .style(Style::default().fg(Color::Cyan));

    sparkline.render(area, buf);
}
```

### Panel C: Run History (40% × 47%)

**Format:**
```
📜 RUN HISTORY
──────────────────────────────────────────────
Today:
│ 08:00 │ seo-pipeline    │ ✓ 45s │ $0.12
│ 06:00 │ novanet-sync    │ ✓ 12s │ $0.02
│ 02:00 │ seo-pipeline    │ ✓ 43s │ $0.11
Yesterday:
│ 20:00 │ seo-pipeline    │ ✓ 44s │ $0.12
│ 16:00 │ analytics-report│ ✗ err │ $0.03
```

**Data Structure:**
```rust
pub struct RunHistory {
    pub entries: Vec<HistoryEntry>,
}

pub struct HistoryEntry {
    pub workflow_name: String,
    pub result: RunResult,
}

impl RunHistory {
    pub fn grouped_by_day(&self) -> Vec<(String, Vec<&HistoryEntry>)> {
        let mut groups: HashMap<String, Vec<&HistoryEntry>> = HashMap::new();

        for entry in &self.entries {
            let day = entry.result.timestamp.format("%Y-%m-%d").to_string();
            groups.entry(day).or_default().push(entry);
        }

        let mut sorted: Vec<_> = groups.into_iter().collect();
        sorted.sort_by(|a, b| b.0.cmp(&a.0)); // Newest first

        // Convert to human-readable labels
        let today = Utc::now().format("%Y-%m-%d").to_string();
        let yesterday = (Utc::now() - chrono::Duration::days(1)).format("%Y-%m-%d").to_string();

        sorted.into_iter().map(|(day, entries)| {
            let label = if day == today {
                "Today".to_string()
            } else if day == yesterday {
                "Yesterday".to_string()
            } else {
                day
            };
            (label, entries)
        }).collect()
    }
}
```

### Panel D: Cron Editor (60% × 47%)

**Cron Syntax Helper:**

```rust
pub struct CronEditor {
    pub expression: String,
    pub cursor: usize,
    pub parsed: Option<Schedule>,
    pub error: Option<String>,
    pub preview: Vec<DateTime<Local>>,
}

impl CronEditor {
    pub fn parse_and_preview(&mut self) {
        match Schedule::from_str(&self.expression) {
            Ok(schedule) => {
                self.parsed = Some(schedule.clone());
                self.error = None;
                self.preview = schedule.upcoming(Local).take(5).collect();
            }
            Err(e) => {
                self.parsed = None;
                self.error = Some(e.to_string());
                self.preview.clear();
            }
        }
    }

    pub fn humanize(&self) -> String {
        // Enhanced humanization
        let parts: Vec<&str> = self.expression.split_whitespace().collect();
        if parts.len() < 5 {
            return "Invalid expression".to_string();
        }

        let [sec, min, hour, day, month, dow] = match parts.as_slice() {
            [s, m, h, d, mo, dw] => [*s, *m, *h, *d, *mo, *dw],
            [m, h, d, mo, dw] => ["0", *m, *h, *d, *mo, *dw],
            _ => return "Invalid expression".to_string(),
        };

        // Build human description
        let mut desc = String::new();

        // Frequency
        if min.starts_with("*/") {
            let interval: u32 = min[2..].parse().unwrap_or(1);
            desc.push_str(&format!("Every {} minute(s)", interval));
        } else if hour.starts_with("*/") {
            let interval: u32 = hour[2..].parse().unwrap_or(1);
            desc.push_str(&format!("Every {} hour(s)", interval));
        } else if min == "0" && hour != "*" {
            desc.push_str(&format!("Daily at {}:00", hour));
        } else if min != "*" && hour != "*" {
            desc.push_str(&format!("At {}:{}", hour, min));
        }

        // Day of week
        if dow != "*" {
            desc.push_str(&format!(" on {}", humanize_dow(dow)));
        }

        // Day of month
        if day != "*" {
            desc.push_str(&format!(" on day {}", day));
        }

        // Month
        if month != "*" {
            desc.push_str(&format!(" in {}", humanize_month(month)));
        }

        desc
    }
}

fn humanize_dow(dow: &str) -> String {
    let days = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    dow.split(',')
        .filter_map(|d| d.parse::<usize>().ok())
        .filter_map(|d| days.get(d))
        .map(|d| d.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn humanize_month(month: &str) -> String {
    let months = ["", "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    month.split(',')
        .filter_map(|m| m.parse::<usize>().ok())
        .filter_map(|m| months.get(m))
        .map(|m| m.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}
```

**Cron Syntax Reference:**
```
╭───────────────────────────────────────────────────────────────╮
│  CRON SYNTAX                                                  │
├───────────────────────────────────────────────────────────────┤
│  ┌───────────── minute (0-59)                                 │
│  │ ┌─────────── hour (0-23)                                   │
│  │ │ ┌───────── day of month (1-31)                           │
│  │ │ │ ┌─────── month (1-12)                                  │
│  │ │ │ │ ┌───── day of week (0-6, 0=Sun)                      │
│  │ │ │ │ │                                                    │
│  * * * * *                                                    │
├───────────────────────────────────────────────────────────────┤
│  EXAMPLES:                                                    │
│  */5 * * * *     Every 5 minutes                              │
│  0 * * * *       Every hour                                   │
│  0 9 * * *       Daily at 9:00 AM                             │
│  0 9 * * 1-5     Weekdays at 9:00 AM                          │
│  0 0 1 * *       First of every month                         │
╰───────────────────────────────────────────────────────────────╯
```

### Priority Queue

**Widget:**
```
QUEUE (3 pending):
1. 🚂 seo-pipeline     │ 14:00 │ Priority: HIGH
2. 🚂 novanet-sync     │ 14:30 │ Priority: NORMAL
3. 🚂 analytics-report │ 16:00 │ Priority: LOW
```

```rust
pub struct PriorityQueue {
    jobs: BinaryHeap<QueuedJob>,
}

#[derive(PartialEq, Eq)]
pub struct QueuedJob {
    pub workflow_id: String,
    pub scheduled_time: DateTime<Local>,
    pub priority: Priority,
}

impl Ord for QueuedJob {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Higher priority first, then earlier time
        other.priority.cmp(&self.priority)
            .then_with(|| self.scheduled_time.cmp(&other.scheduled_time))
    }
}

impl PartialOrd for QueuedJob {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
```

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Enter` | Edit selected schedule |
| `n` / `N` | New schedule |
| `p` | Toggle pause/resume |
| `d` / `Delete` | Delete schedule |
| `r` | Run now (manual trigger) |
| `Tab` | Cycle panels |
| `↑↓` | Navigate list |

---

## Shared Widgets

### DagPanel

**Shared across:** EXPLORER (preview), CHAT (live), EDITOR (sync), RUNNER (animated)

```rust
pub struct DagPanel {
    pub graph: StableGraph<DagNode, DagEdge>,
    pub layout: DagLayout,
    pub selected: Option<NodeIndex>,
    pub highlight: Option<NodeIndex>,
    pub animation: Option<AnimationState>,
    pub config: DagPanelConfig,
}

#[derive(Clone)]
pub struct DagNode {
    pub id: String,
    pub label: String,
    pub verb: VerbKind,
    pub status: TaskStatus,
    pub position: (u16, u16), // Computed by layout
}

#[derive(Clone)]
pub struct DagEdge {
    pub kind: EdgeKind,
}

pub struct DagPanelConfig {
    pub show_labels: bool,
    pub show_status: bool,
    pub show_verb_icon: bool,
    pub interactive: bool,       // Allow selection
    pub animated: bool,          // Enable animations
    pub compact: bool,           // Smaller nodes
}

impl Default for DagPanelConfig {
    fn default() -> Self {
        Self {
            show_labels: true,
            show_status: true,
            show_verb_icon: true,
            interactive: true,
            animated: false,
            compact: false,
        }
    }
}
```

**Layout Algorithm (Sugiyama-style):**
```rust
pub struct DagLayout {
    pub layers: Vec<Vec<NodeIndex>>,
    pub positions: HashMap<NodeIndex, (u16, u16)>,
}

impl DagLayout {
    pub fn compute(graph: &StableGraph<DagNode, DagEdge>, area: Rect) -> Self {
        // 1. Topological sort
        let topo = petgraph::algo::toposort(graph, None).unwrap_or_default();

        // 2. Assign layers (longest path from source)
        let mut layers: Vec<Vec<NodeIndex>> = Vec::new();
        let mut node_layer: HashMap<NodeIndex, usize> = HashMap::new();

        for node in &topo {
            let layer = graph.neighbors_directed(*node, petgraph::Direction::Incoming)
                .filter_map(|pred| node_layer.get(&pred))
                .max()
                .map(|l| l + 1)
                .unwrap_or(0);

            node_layer.insert(*node, layer);

            while layers.len() <= layer {
                layers.push(Vec::new());
            }
            layers[layer].push(*node);
        }

        // 3. Position nodes within layers
        let mut positions = HashMap::new();
        let layer_height = area.height / layers.len().max(1) as u16;

        for (layer_idx, layer) in layers.iter().enumerate() {
            let layer_width = area.width / layer.len().max(1) as u16;

            for (node_idx, node) in layer.iter().enumerate() {
                let x = area.x + (node_idx as u16 * layer_width) + layer_width / 2;
                let y = area.y + (layer_idx as u16 * layer_height) + layer_height / 2;
                positions.insert(*node, (x, y));
            }
        }

        Self { layers, positions }
    }
}
```

**Rendering:**
```rust
impl DagPanel {
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        // 1. Draw edges
        for edge in self.graph.edge_indices() {
            let (source, target) = self.graph.edge_endpoints(edge).unwrap();
            let from = self.layout.positions[&source];
            let to = self.layout.positions[&target];

            self.draw_edge(from, to, &self.graph[edge], buf);
        }

        // 2. Draw nodes
        for node_idx in self.graph.node_indices() {
            let node = &self.graph[node_idx];
            let pos = self.layout.positions[&node_idx];

            let is_selected = self.selected == Some(node_idx);
            let is_highlighted = self.highlight == Some(node_idx);

            self.draw_node(node, pos, is_selected, is_highlighted, buf);
        }
    }

    fn draw_node(&self, node: &DagNode, pos: (u16, u16), selected: bool, highlighted: bool, buf: &mut Buffer) {
        let (x, y) = pos;

        // Node dimensions
        let width = if self.config.compact { 8 } else { 14 };
        let height = if self.config.compact { 1 } else { 3 };

        // Determine style
        let border_style = if selected {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else if highlighted {
            Style::default().fg(Color::Cyan)
        } else {
            node.status.border_style()
        };

        // Draw box
        if !self.config.compact {
            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(border_style)
                .border_type(if node.status == TaskStatus::Pending {
                    BorderType::Rounded
                } else {
                    BorderType::Double
                });

            let node_area = Rect::new(x.saturating_sub(width/2), y.saturating_sub(height/2), width, height);
            block.render(node_area, buf);

            // Content
            let content = if self.config.show_verb_icon {
                format!("{} {}", node.verb.icon(), truncate(&node.label, 8))
            } else {
                truncate(&node.label, 10).to_string()
            };

            buf.set_string(node_area.x + 1, node_area.y + 1, content, Style::default());

            // Status indicator
            if self.config.show_status {
                buf.set_string(
                    node_area.right() - 2,
                    node_area.y,
                    node.status.icon(),
                    node.status.style(),
                );
            }
        } else {
            // Compact mode: just icon and abbreviated label
            let content = format!("{}{}", node.verb.icon(), &node.label.chars().take(3).collect::<String>());
            buf.set_string(x.saturating_sub(2), y, content, border_style);
        }
    }

    fn draw_edge(&self, from: (u16, u16), to: (u16, u16), edge: &DagEdge, buf: &mut Buffer) {
        let style = match edge.kind {
            EdgeKind::Sequential => Style::default().fg(Color::DarkGray),
            EdgeKind::Reference => Style::default().fg(Color::Cyan),
            EdgeKind::Fork => Style::default().fg(Color::Yellow),
            EdgeKind::FanIn => Style::default().fg(Color::Magenta),
        };

        // Simple line drawing (vertical then horizontal)
        let mid_y = (from.1 + to.1) / 2;

        // Vertical from source
        for y in from.1.min(mid_y)..=from.1.max(mid_y) {
            buf.get_mut(from.0, y).set_char('│').set_style(style);
        }

        // Horizontal
        for x in from.0.min(to.0)..=from.0.max(to.0) {
            buf.get_mut(x, mid_y).set_char('─').set_style(style);
        }

        // Vertical to target
        for y in mid_y.min(to.1)..=mid_y.max(to.1) {
            buf.get_mut(to.0, y).set_char('│').set_style(style);
        }

        // Arrow at target
        buf.get_mut(to.0, to.1.saturating_sub(1)).set_char('▼').set_style(style);

        // Corners
        if from.0 != to.0 {
            buf.get_mut(from.0, mid_y).set_char(if from.1 < mid_y { '└' } else { '┌' }).set_style(style);
            buf.get_mut(to.0, mid_y).set_char(if to.1 > mid_y { '┐' } else { '┘' }).set_style(style);
        }
    }
}
```

### StatusBar

**Shared component at bottom of all views:**

```rust
pub struct StatusBar {
    pub left: Vec<StatusItem>,
    pub center: Vec<StatusItem>,
    pub right: Vec<StatusItem>,
}

pub enum StatusItem {
    Text(String),
    KeyHint { key: String, action: String },
    Separator,
    MpcStatus { name: String, connected: bool },
    ProviderStatus { name: String, model: String },
    ViewIndicator { current: usize, total: usize },
}

impl StatusBar {
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        let style = Style::default().bg(Color::Rgb(0, 43, 54)).fg(Color::Rgb(131, 148, 150));

        // Fill background
        for x in area.x..area.right() {
            buf.get_mut(x, area.y).set_style(style);
        }

        // Render left items
        let mut x = area.x + 1;
        for item in &self.left {
            let rendered = item.render();
            buf.set_spans(x, area.y, &rendered, area.width);
            x += rendered.width() as u16 + 1;
        }

        // Render center (centered)
        let center_text = self.center.iter()
            .map(|i| i.render())
            .collect::<Vec<_>>();
        let center_width: u16 = center_text.iter().map(|s| s.width() as u16).sum();
        let center_x = area.x + (area.width - center_width) / 2;
        let mut x = center_x;
        for spans in center_text {
            buf.set_spans(x, area.y, &spans, area.width);
            x += spans.width() as u16;
        }

        // Render right (right-aligned)
        let right_text = self.right.iter()
            .map(|i| i.render())
            .collect::<Vec<_>>();
        let right_width: u16 = right_text.iter().map(|s| s.width() as u16).sum();
        let mut x = area.right().saturating_sub(right_width + 1);
        for spans in right_text {
            buf.set_spans(x, area.y, &spans, area.width);
            x += spans.width() as u16;
        }
    }
}

impl StatusItem {
    pub fn render(&self) -> Spans<'static> {
        match self {
            StatusItem::Text(t) => Spans::from(t.clone()),
            StatusItem::KeyHint { key, action } => Spans::from(vec![
                Span::styled(format!("[{}]", key), Style::default().fg(Color::Yellow)),
                Span::raw(format!(" {}", action)),
            ]),
            StatusItem::Separator => Spans::from(" │ "),
            StatusItem::MpcStatus { name, connected } => {
                let icon = if *connected { "●" } else { "○" };
                let color = if *connected { Color::Green } else { Color::Red };
                Spans::from(vec![
                    Span::styled(icon, Style::default().fg(color)),
                    Span::raw(format!(" {}", name)),
                ])
            }
            StatusItem::ProviderStatus { name, model } => Spans::from(vec![
                Span::raw("🧠 "),
                Span::styled(name, Style::default().fg(Color::Cyan)),
                Span::raw(format!(" ({})", model)),
            ]),
            StatusItem::ViewIndicator { current, total } => Spans::from(vec![
                Span::styled(
                    (1..=*total).map(|i| if i == *current { "●" } else { "○" }).collect::<String>(),
                    Style::default().fg(Color::DarkGray),
                ),
            ]),
        }
    }
}
```

---

## Navigation System

### View Enum

```rust
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TuiView {
    Explorer = 1,
    Chat = 2,
    Editor = 3,
    Runner = 4,
    Scheduler = 5,
}

impl TuiView {
    pub fn from_key(key: char) -> Option<Self> {
        match key {
            '1' | 'e' => Some(Self::Explorer),
            '2' | 'c' => Some(Self::Chat),
            '3' | 'd' => Some(Self::Editor),
            '4' | 'r' => Some(Self::Runner),
            '5' | 's' => Some(Self::Scheduler),
            _ => None,
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Explorer => Self::Chat,
            Self::Chat => Self::Editor,
            Self::Editor => Self::Runner,
            Self::Runner => Self::Scheduler,
            Self::Scheduler => Self::Explorer,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::Explorer => Self::Scheduler,
            Self::Chat => Self::Explorer,
            Self::Editor => Self::Chat,
            Self::Runner => Self::Editor,
            Self::Scheduler => Self::Runner,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Explorer => "EXPLORER",
            Self::Chat => "CHAT",
            Self::Editor => "EDITOR",
            Self::Runner => "RUNNER",
            Self::Scheduler => "SCHEDULER",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Self::Explorer => "📁",
            Self::Chat => "💬",
            Self::Editor => "✏️",
            Self::Runner => "▶️",
            Self::Scheduler => "📅",
        }
    }
}
```

### Global Key Handling

```rust
pub fn handle_global_key(key: KeyEvent, app: &mut App) -> bool {
    match key.code {
        // View navigation
        KeyCode::Tab if key.modifiers.is_empty() => {
            app.view = app.view.next();
            true
        }
        KeyCode::BackTab => {
            app.view = app.view.prev();
            true
        }
        KeyCode::Char(c @ '1'..='5') => {
            if let Some(view) = TuiView::from_key(c) {
                app.view = view;
                return true;
            }
            false
        }
        KeyCode::Char(c @ ('e' | 'c' | 'd' | 'r' | 's')) if key.modifiers.is_empty() => {
            // Only when not in input mode
            if !app.is_input_focused() {
                if let Some(view) = TuiView::from_key(c) {
                    app.view = view;
                    return true;
                }
            }
            false
        }

        // Global actions
        KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.show_fuzzy_search();
            true
        }
        KeyCode::Char('h') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.show_help_overlay();
            true
        }
        KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.should_quit = true;
            true
        }

        _ => false,
    }
}
```

### Header Rendering

```rust
pub fn render_header(view: TuiView, context: &str, area: Rect, buf: &mut Buffer) {
    let style = Style::default().bg(Color::Rgb(7, 54, 66)).fg(Color::White);

    // Fill background
    for x in area.x..area.right() {
        buf.get_mut(x, area.y).set_style(style);
    }

    // Left: Icon + Title
    let title = format!("🦋 NIKA {}", view.title());
    buf.set_string(area.x + 1, area.y, &title, style.add_modifier(Modifier::BOLD));

    // Center: Context (filename, session, etc.)
    let context_x = area.x + (area.width - context.len() as u16) / 2;
    buf.set_string(context_x, area.y, context, style);

    // Right: View indicators
    let indicators: String = (1..=5).map(|i| {
        if i == view as usize {
            format!("[{}]", i)
        } else {
            format!(" {} ", i)
        }
    }).collect();
    buf.set_string(area.right() - indicators.len() as u16 - 1, area.y, &indicators, style);
}
```

---

## Implementation Phases

### Phase 1: Foundation (v0.10.0)

**Tasks:**
1. [ ] Add new crate dependencies (tui-tree-widget, nucleo-matcher, cron)
2. [ ] Implement `TuiView` enum with 5 views
3. [ ] Implement global navigation (Tab, 1-5, letters)
4. [ ] Create `DagPanel` shared widget
5. [ ] Create `StatusBar` shared widget
6. [ ] Implement EXPLORER view (6 panels)
7. [ ] Implement `FileKind` enum with icons
8. [ ] Implement `TreeState` for file browser
9. [ ] Implement fuzzy search with nucleo-matcher
10. [ ] Implement EDITOR view (split with DAG sync)
11. [ ] Add cursor-to-DAG tracking
12. [ ] Add real-time YAML validation

**Tests:** 40 new tests

### Phase 2: Chat-as-DAG (v0.10.1)

**Tasks:**
1. [ ] Implement `ChatWorkflow` struct with StableGraph
2. [ ] Implement message parsing (@mentions, // fork)
3. [ ] Implement `LiveDag` with real-time updates
4. [ ] Implement YAML preview generation
5. [ ] Implement export flow (Chat → Editor)
6. [ ] Add NodeBox (5-line message format)
7. [ ] Wire Chat panel navigation

**Tests:** 30 new tests

### Phase 3: Execution (v0.11.0)

**Tasks:**
1. [ ] Implement RUNNER view (4 panels)
2. [ ] Implement DAG animation states
3. [ ] Implement streaming output display
4. [ ] Implement MCP call log
5. [ ] Implement execution metrics
6. [ ] Add pause/stop/restart controls

**Tests:** 25 new tests

### Phase 4: Scheduler (v0.11.1)

**Tasks:**
1. [ ] Add `cron` crate integration
2. [ ] Implement `ScheduledWorkflow` struct
3. [ ] Implement SCHEDULER view (4 panels)
4. [ ] Implement timeline visualization
5. [ ] Implement cron editor with validation
6. [ ] Implement priority queue
7. [ ] Implement run history

**Tests:** 30 new tests

### Phase 5: Polish (v0.12.0)

**Tasks:**
1. [ ] Implement breadcrumb navigation
2. [ ] Implement minimap sidebar
3. [ ] Add 60fps animation loop
4. [ ] Performance optimization (see PERFORMANCE.md)
5. [ ] Add syntect highlighting (feature-gated)
6. [ ] Comprehensive integration tests

**Tests:** 20 new tests

---

## Success Criteria

### Core UX
- [ ] 5 views navigable via Tab and 1-5 keys
- [ ] Explorer as default view
- [ ] Consistent emoji taxonomy across all views
- [ ] Scroll in all panels with indicators
- [ ] Copy from any panel (Ctrl+C)
- [ ] 60 FPS smooth animations

### Chat-as-DAG
- [ ] Messages become DAG nodes
- [ ] `//` fork creates parallel branches
- [ ] `@N` references create edges
- [ ] Live YAML preview updates in real-time
- [ ] Export produces valid `.nika.yaml`

### Editor
- [ ] Cursor-to-DAG sync works
- [ ] Real-time validation shows errors
- [ ] Undo/Redo preserved from v0.8.0
- [ ] F5 runs workflow

### Runner
- [ ] Animated DAG shows task progress
- [ ] Streaming output displays live
- [ ] Token/cost metrics accurate
- [ ] Pause/Stop/Restart work

### Scheduler
- [ ] Cron expressions parse correctly
- [ ] Timeline shows next 24 hours
- [ ] Priority queue orders correctly
- [ ] Run history persists

---

## Appendix A: File Structure Changes

```
src/tui/
├── mod.rs                  # TuiView enum (5 views)
├── app.rs                  # App state, global handlers
├── views/
│   ├── mod.rs              # View trait, exports
│   ├── explorer.rs         # NEW: 6-panel explorer
│   ├── chat.rs             # UPDATED: Mission Control
│   ├── editor.rs           # UPDATED: DAG sync (was studio.rs)
│   ├── runner.rs           # NEW: Execution monitor
│   └── scheduler.rs        # NEW: Cron management
├── widgets/
│   ├── mod.rs              # Widget exports
│   ├── dag_panel.rs        # NEW: Shared DAG widget
│   ├── status_bar.rs       # NEW: Shared status bar
│   ├── node_box.rs         # NEW: Chat message box
│   ├── tree_view.rs        # NEW: File tree wrapper
│   ├── fuzzy_search.rs     # NEW: nucleo overlay
│   ├── timeline.rs         # NEW: Scheduler timeline
│   ├── cron_editor.rs      # NEW: Cron input
│   └── ...existing...
└── theme.rs                # Color palette, styles
```

---

## Appendix B: Crate Usage Examples

### tui-tree-widget

```rust
use tui_tree_widget::{Tree, TreeItem, TreeState};

let items = vec![
    TreeItem::new("root", "📁 project", vec![
        TreeItem::new("nika", "🪽 .nika", vec![
            TreeItem::new_leaf("agent", "🐔 researcher.agent.yaml"),
        ]).unwrap(),
        TreeItem::new_leaf("workflow", "🚂 seo-pipeline.nika.yaml"),
    ]).unwrap(),
];

let mut state = TreeState::default();
state.select(vec!["root", "nika"]);

let tree = Tree::new(&items)
    .expect("unique identifiers")
    .block(Block::bordered().title("Files"));

frame.render_stateful_widget(tree, area, &mut state);
```

### nucleo-matcher

```rust
use nucleo_matcher::{Matcher, Config};
use nucleo_matcher::pattern::{Pattern, CaseMatching, Normalization};

let mut matcher = Matcher::new(Config::DEFAULT);
let pattern = Pattern::parse("seo", CaseMatching::Ignore, Normalization::Smart);

let files = vec!["seo-pipeline.nika.yaml", "research.nika.yaml", "seo.skill.yaml"];
let mut results = pattern.match_list(files, &mut matcher);
results.sort_by(|a, b| b.1.cmp(&a.1));
// results: [("seo-pipeline.nika.yaml", 95), ("seo.skill.yaml", 90), ...]
```

### cron

```rust
use cron::Schedule;
use chrono::Local;
use std::str::FromStr;

let expression = "0 */6 * * *"; // Every 6 hours
let schedule = Schedule::from_str(expression).unwrap();

println!("Next 5 occurrences:");
for datetime in schedule.upcoming(Local).take(5) {
    println!("  {}", datetime);
}
```

### syntect

```rust
use syntect::easy::HighlightLines;
use syntect::parsing::SyntaxSet;
use syntect::highlighting::ThemeSet;
use syntect::util::as_24_bit_terminal_escaped;

let ps = SyntaxSet::load_defaults_newlines();
let ts = ThemeSet::load_defaults();
let syntax = ps.find_syntax_by_extension("yaml").unwrap();
let mut h = HighlightLines::new(syntax, &ts.themes["Solarized (dark)"]);

let yaml = "schema: nika/workflow@0.6\nworkflow: example";
for line in yaml.lines() {
    let ranges = h.highlight_line(line, &ps).unwrap();
    let escaped = as_24_bit_terminal_escaped(&ranges, true);
    println!("{}", escaped);
}
```

---

**Document Status:** COMPLETE
**Last Updated:** 2026-02-24
**Next Review:** After v0.10.0 implementation
