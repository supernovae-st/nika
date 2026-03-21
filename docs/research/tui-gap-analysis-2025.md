# TUI Gap Analysis: Nika vs. Best-in-Class Terminal Apps (2025-2026)

**Date:** 2026-03-21
**Scope:** Features missing from Nika TUI that the top terminal editors and TUI apps ship today
**Method:** Perplexity web research on Helix, lazygit, bottom, ratatui ecosystem, workflow engines

---

## What Nika Already Has

Before the gaps, credit where due. Nika TUI already ships:

- 3-view architecture (Studio / Command / Control)
- Tree-sitter syntax highlighting
- DiagnosticsEngine with gutter icons + underline styles + severity levels
- Command palette (Cmd+K fuzzy search)
- Which-key popup (leader key hints)
- Undo/redo with coalescing
- Selection model (char/word/line)
- DAG ASCII visualization
- Notification system (Info/Warning/Alert/Success/Error levels)
- Session context bar (tokens, cost, MCP status)
- Matrix rain + decrypt animations
- Help overlay
- Sparkline + gauge widgets
- Browser panel with tree widget and git status
- Provider modal + selector

---

## Priority 1 -- High Impact, YAML Editor Specific

### 1.1 Inline Diagnostics with Virtual Text

**What Helix shipped (25.01):** Diagnostics rendered as virtual text at end-of-line or between lines, not just gutter icons. Configurable per-severity: cursor-line shows warnings+errors, other-lines show only errors. This prevents noise on large files while keeping critical info visible.

**Nika gap:** DiagnosticsEngine has gutter icons and underline styles but no virtual text rendering. Errors appear in the status bar (`first_error()`) or gutter only. The user must mentally map `NIKA-141` to the problem.

**What to build:**
- End-of-line diagnostic text: `  -- [NIKA-141] duplicate task id` rendered in dim red after the offending line
- Cursor-line expansion: when cursor is on an error line, show the full message below the line as a virtual text block
- Severity filter config: `end_of_line_min_severity: Warning` in TuiConfig
- Helix uses `cursor-line` vs `other-lines` distinction -- adopt the same pattern

**Effort:** Medium (extend StudioView render loop to inject virtual lines)
**Impact:** Highest. This is the single biggest UX win for a YAML editor. Users should never have to look away from their code to understand errors.

**Sources:**
- https://helix-editor.com/news/release-25-01-highlights/
- https://www.seanh.cc/2025/10/13/helix-lsp-virtualenv/

---

### 1.2 Suggested Fixes (Code Actions)

**What Helix/LSP editors ship:** When cursor is on a diagnostic, a lightbulb or `[a]` hint appears. Pressing it shows a menu of fixes: "Add missing `depends_on`", "Change schema to 0.12", "Remove duplicate task".

**Nika gap:** DiagnosticsEngine detects errors but has zero fix suggestions. The NIKA error codes are well-structured (each AnalyzeError has a code + message) -- the infrastructure is there, the "what to do about it" layer is not.

**What to build:**
- `SuggestedFix` struct: `{ description: String, replacement: String, range: (line,col)..(line,col) }`
- Map each NIKA-1xx error to 0-N suggested fixes
- On cursor-line with diagnostic: show `[Space+a] Fix` in which-key
- Apply fix = replace text in editor buffer + re-analyze
- Start with the 5 most common errors: duplicate id, missing schema, unknown verb, broken depends_on, invalid template syntax

**Effort:** Medium-High
**Impact:** Very high for YAML beginners. Turns error messages into one-keypress solutions.

---

### 1.3 Breadcrumb Navigation (Structural Location)

**What Combobulate/Helix show:** A statusline or bar displaying `workflow > tasks[2] > infer > prompt` as you move through the file. Tree-sitter powers this by walking the AST up from cursor position.

**Nika gap:** No structural breadcrumb. The editor knows cursor position (line, col) but does not map it to the YAML/workflow AST path. Nika already has tree-sitter integration for highlighting -- extending it to breadcrumbs is natural.

**What to build:**
- `breadcrumb_at(line, col) -> Vec<String>` function using tree-sitter node traversal
- Render as a thin bar above the editor: `workflow > tasks > step1 > infer > prompt`
- Clickable segments (if mouse enabled) to jump to that YAML node
- Special icons per verb: infer gets a brain icon, exec gets a terminal icon, etc.

**Effort:** Low-Medium (tree-sitter already loaded, just walk ancestors)
**Impact:** High. YAML files get deeply nested. Knowing "where am I" without scrolling up is essential.

---

### 1.4 Token/Cost Calculator Preview

**What no TUI ships yet:** No terminal tool does pre-send token/cost estimation well. This is greenfield -- Nika could be first.

**Nika gap:** SessionContextBar already tracks tokens and cost during execution. But there is no pre-execution estimate: "if I run this workflow, approximately how many tokens will it consume and what will it cost?"

**What to build:**
- `EstimatePanel` widget: shows per-task token estimates based on prompt length + model context
- Use tiktoken-rs (or simple word-count heuristic: ~750 words = 1000 tokens) for input estimation
- Pull per-model pricing from `provider/cost.rs` (already exists)
- Output estimation: use model's `max_tokens` or a configurable default
- Total cost = sum of (input_tokens * input_rate + estimated_output_tokens * output_rate) per task
- Show in the DAG preview panel: each node annotated with `~2.1k tokens / ~$0.003`
- Keybinding: `Space+$` or integrate into the existing DAG panel

**Effort:** Medium (token counting is simple, pricing data exists, UI is a panel overlay)
**Impact:** High. This is a unique differentiator. No other workflow tool shows this in-terminal.

**Sources:**
- https://evalics.com/blog/how-to-calculate-token-costs-for-an-ai-project
- https://token-calculator.net

---

### 1.5 Workflow Dry Run / Execution Preview

**What Airflow/Prefect/Temporal ship:**
- `airflow dags test` -- simulates execution without side effects
- `prefect flow run --dry-run` -- shows task tree with estimated timing
- Nextflow `-preview` -- renders DAG + resource estimates without running

**Nika gap:** `nika run` executes immediately. `nika check` validates YAML but does not show an execution plan. There is no "what would happen" mode.

**What to build:**
- `nika dry-run workflow.nika.yaml` CLI command + `Space+d` in TUI
- Output: DAG with execution order numbers, parallelism groups, estimated timing
- Show which providers will be called, which MCP servers will be invoked
- Flag potential issues: "task X has no timeout", "task Y depends on unresolved template"
- In TUI: render as an annotated DAG overlay (reuse DagAscii widget, add annotations)
- Color-code: green = ready, yellow = needs provider key, red = will fail (missing dep)

**Effort:** Medium (DAG already built by runtime, just need to render without executing)
**Impact:** High. Essential for workflows with real costs (LLM calls, API calls).

**Sources:**
- https://workflowengine.io/documentation/execution
- https://temporal.io/blog/workflow-engine-principles

---

## Priority 2 -- Important UX Polish

### 2.1 File Picker Dialog

**What Helix ships:** Built-in fuzzy file picker. Type partial filename, see matches, preview content, press Enter to open.

**What ratatui ecosystem offers:** `ratatui-explorer` (basic tree nav), `tui-file-explorer` (two-pane, filtering, sorting). Neither has fuzzy finding.

**Nika gap:** BrowserPanel shows a tree of files, but there is no fuzzy-find file picker overlay. To open a different `.nika.yaml`, the user must navigate the tree manually.

**What to build:**
- `FilePicker` overlay widget (floating, centered, like command palette)
- Scan for `*.nika.yaml` files in project root
- Fuzzy match on filename as user types (use `fuzzy-matcher` or `nucleo` crate)
- Preview pane: show first 10 lines of selected file
- Keybinding: `Space+f` or `Ctrl+p`
- Return selected path to Studio view, which loads it into the editor

**Effort:** Medium
**Impact:** Medium-High. Critical for projects with multiple workflow files.

---

### 2.2 Search and Replace

**What Helix/Scooter ship:**
- `/` for incremental regex search with live highlighting
- `:s/pattern/replacement/` with capture group support
- Scooter: TUI-native search/replace with per-match toggle, whole-word, case options

**Nika gap:** No search in the editor. No search across files. The editor has selection and cursor movement but zero find/replace.

**What to build:**
- Phase 1: `Ctrl+f` in-file search with incremental highlighting
- Phase 2: `Ctrl+h` search and replace (confirm per match or replace all)
- Phase 3: `Ctrl+Shift+f` cross-file search (grep all `.nika.yaml` in project)
- Use `regex` crate for pattern matching
- Highlight all matches in the editor, scroll to current match
- Status bar: `3/17 matches`

**Effort:** Medium-High (Phase 1 is quick, Phase 3 is more work)
**Impact:** Medium-High. Any editor without search feels broken.

**Sources:**
- https://github.com/thomasschafer/scooter (Rust TUI search/replace reference)

---

### 2.3 Split Editor (Side-by-Side)

**What Helix ships:** `:vsplit` and `:hsplit` to view two files simultaneously. Essential for comparing a workflow with its output, or editing two related workflows.

**Nika gap:** Studio view has 3 panels (Browser | Editor | DAG) but no way to show two editor panes. The layout module (`ResponsiveLayout`) handles responsive sizing but not user-initiated splits.

**What to build:**
- `Ctrl+\` vertical split: Editor left | Editor right
- Each pane independently scrollable, independently editable
- Shared diagnostics engine (each pane has its own DiagnosticsEngine)
- Close split: `Ctrl+w` then `q`
- Use case: edit workflow on left, see the output template on right
- Layout: modify the existing 3-panel layout to support a 4th "split editor" region

**Effort:** High (significant layout refactoring)
**Impact:** Medium. Power users want it, casual users never split.

---

### 2.4 Toast Notifications with Animation

**What ratatui-notifications ships:** Toasts with slide/fade/expand animations, 9 anchor positions, level-based styling, smart stacking, auto-dismiss, max-concurrent with overflow policy.

**Nika gap:** `NotificationLevel` and `Notification` types exist in `state/notification.rs`, and `StatusMessageWidget` exists. But no animated toasts, no stacking, no auto-dismiss with visual transition.

**What to build:**
- Either integrate `ratatui-notifications` crate or build a minimal toast renderer
- Anchor: top-right corner of the editor area
- Auto-dismiss: Info 3s, Warning 5s, Error stays until dismissed
- Stack max 3 visible, discard oldest on overflow
- Slide-in animation (frame-based, reuse existing `AnimationState`)
- Use for: "File saved", "Workflow validated", "Provider connected", "MCP server timeout"

**Effort:** Low-Medium (crate does most of the work)
**Impact:** Medium. Polish that makes the app feel professional.

**Sources:**
- https://lib.rs/crates/ratatui-notifications

---

## Priority 3 -- Nice to Have

### 3.1 Lazygit-Style Contextual Panels

**What lazygit does:** Right panel changes content based on left panel selection. Panels are non-overwhelming (few, fixed). Inline notifications warn about divergence without modals.

**Nika already does this partially:** Studio view has Browser (left) that affects Editor (center) which affects DAG (right). But the DAG panel is static -- it does not react to cursor position in the editor.

**What to build:**
- DAG panel highlights the current task when cursor moves in the editor
- If cursor is on `depends_on: [task_a]`, highlight the dependency edge in the DAG
- If cursor is on `invoke: mcp_tool`, show MCP server status in the DAG panel area

**Effort:** Low-Medium
**Impact:** Medium. Ties the panels together into a cohesive experience.

---

### 3.2 Bottom-Style Panel Zoom

**What bottom does:** Press a key to expand any widget to full screen, press again to restore. Simple toggle.

**Nika gap:** No panel zoom. If the DAG is complex, the small right panel is too cramped.

**What to build:**
- `Space+z` or `Enter` on focused panel: zoom to full screen
- Same key: restore to normal layout
- Works on any panel: Browser, Editor, DAG, Chat
- Reuse existing `ResponsiveLayout` but with a "zoomed panel" override

**Effort:** Low
**Impact:** Medium. Especially useful for complex DAGs and long chat sessions.

---

### 3.3 Undo System Feedback (Lazygit Pattern)

**What lazygit ships:** `z` for undo/redo with visual feedback of what was undone. Works across operations, not just text edits.

**Nika gap:** EditHistory handles text undo/redo but there is no visual feedback ("Undid: delete 3 lines") and no operation-level undo (e.g., undo "moved task up in the list").

**What to build:**
- Toast notification on undo: "Undone: deleted line 15"
- Toast on redo: "Redone: inserted 'depends_on: [step1]'"
- Consider operation-level undo for structural edits (move task, rename task id)

**Effort:** Low (just add toast on undo/redo events)
**Impact:** Low-Medium.

---

### 3.4 Accessibility: Semantic Colors Only

**What Ghostty/modern terminals emphasize:** Respect terminal theme colors. Do not hardcode RGB values. Use the 16 ANSI colors that the user's terminal theme defines.

**Nika gap:** DiagnosticSeverity hardcodes Solarized RGB values (`Color::Rgb(220, 50, 47)`). This breaks on terminals with different themes.

**What to build:**
- Already have a `tokens/` module with `SemanticColors` and `ColorPalette`
- Migrate all hardcoded RGB in diagnostics.rs to use the token system
- Ensure high contrast in both light and dark terminal themes
- Test with at least: Solarized Dark, Catppuccin Mocha, Tokyo Night, default macOS Terminal

**Effort:** Low
**Impact:** Low but important for accessibility.

---

## Summary: Ranked Feature Roadmap

| Rank | Feature | Impact | Effort | Unique to Nika? |
|------|---------|--------|--------|-----------------|
| 1 | Inline diagnostics (virtual text) | Highest | Medium | No (Helix has it) |
| 2 | Token/cost calculator preview | High | Medium | YES -- no TUI does this |
| 3 | Workflow dry run preview | High | Medium | Partially (Airflow CLI, but not in-TUI) |
| 4 | Breadcrumb navigation | High | Low-Med | Rare in TUIs |
| 5 | Suggested fixes (code actions) | Very High | Med-High | No (LSP editors) but rare in YAML |
| 6 | File picker with fuzzy find | Med-High | Medium | No (Helix) |
| 7 | In-file search | Med-High | Medium | Table stakes |
| 8 | Toast notifications animated | Medium | Low-Med | Common pattern |
| 9 | Contextual DAG highlighting | Medium | Low-Med | Unique combo |
| 10 | Panel zoom | Medium | Low | Common (bottom) |
| 11 | Split editor | Medium | High | Common (Helix, Vim) |
| 12 | Search and replace | Med-High | Med-High | Table stakes |
| 13 | Cross-file search | Medium | High | Power feature |

---

## Recommended Implementation Order

**Phase A -- "Make errors disappear" (2-3 weeks)**
1. Inline diagnostics with virtual text (1.1)
2. Breadcrumb navigation (1.3)
3. Migrate hardcoded colors to token system (3.4)

**Phase B -- "Unique differentiators" (2-3 weeks)**
4. Token/cost calculator preview (1.4)
5. Workflow dry run preview (1.5)
6. Contextual DAG highlighting (3.1)

**Phase C -- "Editor completeness" (3-4 weeks)**
7. In-file search with highlighting (2.2 Phase 1)
8. File picker with fuzzy find (2.1)
9. Suggested fixes for top 5 errors (1.2)
10. Toast notifications (2.4)

**Phase D -- "Power user" (2-3 weeks)**
11. Search and replace (2.2 Phase 2)
12. Panel zoom (3.2)
13. Split editor (2.3)
14. Cross-file search (2.2 Phase 3)

---

## Key Crates to Evaluate

| Crate | Purpose | Stars/Downloads |
|-------|---------|-----------------|
| `ratatui-notifications` | Toast system | New but well-documented |
| `ratatui-explorer` | File browser widget | Basic but functional |
| `nucleo` | Fuzzy matching (Helix uses this) | Battle-tested |
| `tiktoken-rs` | Token counting | OpenAI official Rust port |
| `fuzzy-matcher` | Simpler fuzzy matching | Stable |

---

## Sources

1. Helix 25.01 inline diagnostics -- https://helix-editor.com/news/release-25-01-highlights/
2. Helix features overview -- https://blacksquiddotink.wordpress.com/2025/07/20/explore-helix-a-modern-terminal-text-editor/
3. Helix 25.07 release -- https://helix-editor.com/news/release-25-07-highlights/
4. Lazygit UX patterns -- https://bwplotka.dev/2025/lazygit/
5. Bottom TUI patterns -- https://github.com/clementtsang/bottom
6. ratatui-notifications -- https://lib.rs/crates/ratatui-notifications
7. Scooter (Rust TUI search/replace) -- https://github.com/thomasschafer/scooter
8. ratatui-explorer -- https://github.com/tatounee/ratatui-explorer
9. Combobulate breadcrumbs -- https://www.masteringemacs.org/article/combobulate-intuitive-structured-navigation-treesitter
10. Workflow engine patterns -- https://workflowengine.io/documentation/execution
11. Temporal principles -- https://temporal.io/blog/workflow-engine-principles
12. Helix inline diagnostics config -- https://www.seanh.cc/2025/10/13/helix-lsp-virtualenv/
13. Innovative TUI apps 2025 -- https://matthewsanabria.dev/posts/tools-worth-changing-to-in-2025/
14. awesome-tuis -- https://github.com/rothgar/awesome-tuis
