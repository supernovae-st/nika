# Color System Unification + Dead Code Purge + studio.rs Split

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Unify 3 overlapping color systems into 1 Theme struct. Delete compat.rs + 3 solarized duplicates. Split studio.rs (4206 lines) into 12 modules. Purge 10 dead items + fix 13 false #[allow(dead_code)].

**Architecture:** Single source of truth: `SemanticColors` (Catppuccin Mocha) → `TokenResolver` → `Theme` (expanded to ~75 fields) → ALL widgets via `&Theme`. Zero `compat::`, zero inline `Color::Rgb()`, zero `solarized::` in widgets.

**Tech Stack:** Rust, ratatui 0.30, Catppuccin Mocha palette

---

## Phase 0: Dead Code Purge (5 min)

### Task 0.1: Remove 10 truly dead items

**Files to modify:**

1. `src/tui/views/studio.rs` — Remove `SyntaxColors::DEFAULT` const
2. `src/tui/widgets/tree/render.rs` — Remove `TreeIcon::SPACER`, `FlatNode::depth` field, `FlatNode::is_last` field
3. `src/tui/views/chat/mod.rs` — Remove `SEPARATOR_20` and `SEPARATOR_20_ASCII`
4. `src/tui/views/chat/types.rs` — Remove `FailedMcpCall`, `LinePosition` structs
5. `src/tui/views/chat/mode_config.rs` — Remove duplicate `char_to_byte_offset()`

Build: `cargo build --features tui`
Commit: `refactor(tui): remove 10 truly dead items`

### Task 0.2: Remove 13 false #[allow(dead_code)] annotations

These items ARE used but suppressed — remove the annotation only:

1. `widgets/mod.rs` — Remove `#[allow(dead_code)]` from: `mod activity_stack`, `mod dag_edge`, `mod dag_layout`, `mod dag_node_box`, `mod matrix_decrypt`, `mod provider_selector`, `mod sparkline`, `pub mod micro`
2. `views/studio.rs` — Remove from `current_line()`, `current_col()`
3. `state/mod.rs` — Remove from `mod chat_overlay`
4. `app/types.rs` — Remove from `enum Action`
5. `theme.rs` — Remove from `pub mod solarized`
6. `views/wizard.rs` — Remove from `struct EditorItem`

Build: `cargo build --features tui`
Commit: `refactor(tui): remove 13 stale #[allow(dead_code)]`

---

## Phase 1: Expand Theme to ~75 fields (30 min)

### Task 1.1: Add missing fields to Theme struct

**File:** `src/tui/theme.rs`

Add these fields to the `Theme` struct:

```rust
// DIAGNOSTICS
pub diagnostic_error: Color,
pub diagnostic_warning: Color,
pub diagnostic_hint: Color,

// POPUP / OVERLAY
pub popup_bg: Color,
pub popup_border: Color,
pub popup_selected: Color,

// VERB COLORS (move from VerbColor enum)
pub verb_infer: Color,
pub verb_exec: Color,
pub verb_fetch: Color,
pub verb_invoke: Color,
pub verb_agent: Color,

// VERB GLOW (brighter for active/hover)
pub verb_infer_glow: Color,
pub verb_exec_glow: Color,
pub verb_fetch_glow: Color,
pub verb_invoke_glow: Color,
pub verb_agent_glow: Color,

// TREE WIDGET
pub tree_directory: Color,
pub tree_file: Color,
pub tree_hidden: Color,
pub tree_ecosystem: Color,
pub tree_ecosystem_glow: Color,
pub tree_indent_guide: Color,

// SYNTAX HIGHLIGHTING
pub syntax_keyword: Color,
pub syntax_string: Color,
pub syntax_number: Color,
pub syntax_comment: Color,
pub syntax_verb: Color,
pub syntax_template: Color,
pub syntax_mcp_server: Color,
pub syntax_task_id: Color,
```

### Task 1.2: Wire fields in from_resolver()

**File:** `src/tui/cosmic_theme.rs` — `Theme::from_resolver()`

Map all new fields from SemanticColors:

```rust
// Diagnostics
diagnostic_error: resolver.status_error(),
diagnostic_warning: resolver.status_warning(),
diagnostic_hint: resolver.status_info(),

// Popup
popup_bg: resolver.bg_secondary(),
popup_border: resolver.border_default(),
popup_selected: resolver.bg_hover(),

// Verbs (already in SemanticColors)
verb_infer: resolver.semantic().verb_infer,
verb_exec: resolver.semantic().verb_exec,
// ... etc

// Verb glow — lighter variant (compute from base)
verb_infer_glow: lighten(resolver.semantic().verb_infer, 0.3),
// ... etc
```

Add a `lighten()` helper that increases RGB values by a factor.

### Task 1.3: Add verb_glow + syntax fields to SemanticColors

**File:** `src/tui/tokens/semantic.rs`

Add verb_glow and syntax fields to SemanticColors struct. Populate in cosmic_dark():

```rust
// Catppuccin Mocha syntax
syntax_keyword: MAUVE,
syntax_string: GREEN,
syntax_number: PEACH,
syntax_comment: OVERLAY0,
syntax_verb: MAUVE,
syntax_template: TEAL,
syntax_mcp_server: SAPPHIRE,
syntax_task_id: YELLOW,
```

Build + test: `cargo build --features tui && cargo test --lib -- tui`
Commit: `feat(tui): expand Theme to 75 fields with diagnostics/verb/syntax colors`

---

## Phase 2: Migrate widgets from compat:: to theme (1h)

### Task 2.1: Delete tokens/compat.rs

**Step 1:** Run `grep -rn "compat::" src/tui/ | grep -v test | grep -v "mod compat"` to list ALL usages.

**Step 2:** For each file, replace every `compat::X` with the semantic `theme.field`:

Mapping table:
```
compat::GREEN_500    → theme.status_success
compat::RED_500      → theme.status_error
compat::AMBER_500    → theme.status_warning
compat::VIOLET_500   → theme.accent_primary (or theme.verb_infer)
compat::CYAN_500     → theme.accent_secondary (or theme.verb_fetch)
compat::BLUE_500     → theme.status_info
compat::GRAY_500     → theme.text_muted
compat::GRAY_700     → theme.border_normal
compat::SLATE_500    → theme.text_muted
compat::EMERALD_500  → theme.verb_invoke
compat::ROSE_500     → theme.verb_agent
compat::PINK_500     → theme.accent_tertiary
```

**Step 3:** Some widgets don't have `&Theme` access. For those, add `theme: &Theme` parameter and thread it from the parent.

**Step 4:** Delete `src/tui/tokens/compat.rs` and remove `pub mod compat;` from `tokens/mod.rs`.

Build: `cargo build --features tui`
Commit: `refactor(tui): delete compat.rs — migrate 275 usages to Theme`

### Task 2.2: Delete solarized modules (3 copies)

1. `theme.rs` — Remove `pub mod solarized` block. Keep only `accent_at()` and `pulse_intensity()` but rewrite to use Theme colors.
2. `widgets/tree/colors.rs` — Remove local `pub mod solarized`. `from_theme()` already uses Theme fields.
3. `highlight/theme.rs` — Remove local `mod colors`. Wire from Theme's new syntax fields.

Build + test.
Commit: `refactor(tui): delete 3 solarized module duplicates`

### Task 2.3: Migrate inline Color::Rgb in widgets

For each widget file with hardcoded `Color::Rgb()`:

Pattern:
```rust
// BEFORE:
let bg = Color::Rgb(24, 24, 37); // what is this?
// AFTER:
let bg = theme.popup_bg; // clear semantic meaning
```

Priority files (by count):
1. `views/studio.rs` (32) — popups, diagnostics
2. `widgets/provider_modal/` (30+) — cards, tabs
3. `views/chat/messages.rs` (8) — content colors
4. `widgets/pro_status_bar.rs` (10)
5. `widgets/mission_control.rs` (9)
6. `widgets/session_context.rs` (14)
7. `widgets/dag_node_box.rs` (11)
8. `diagnostics.rs` (6) — severity colors

Build after each file.
Commit: `style(tui): migrate N inline Color::Rgb to Theme fields`

---

## Phase 3: Split studio.rs (45 min)

### Task 3.1: Create directory structure

```bash
mkdir -p src/tui/views/studio
mv src/tui/views/studio.rs src/tui/views/studio/mod.rs
```

Verify: `cargo build --features tui` (should work unchanged — Rust resolves `mod studio;` to `studio/mod.rs`)

Commit: `refactor(tui): convert studio.rs to studio/ directory module`

### Task 3.2: Extract types.rs (~125 lines)

Move from mod.rs: `EditorMode`, `StudioFocus`, `StudioRatio`, `ValidationResult`

Add to mod.rs:
```rust
mod types;
pub use types::*;
```

Build + test.
Commit: `refactor(tui): extract studio/types.rs`

### Task 3.3: Extract syntax.rs (~140 lines)

Move: `YamlHighlight` struct + `highlight_line()` + `highlight_value()`

Commit: `refactor(tui): extract studio/syntax.rs`

### Task 3.4: Extract buffer.rs (~675 lines)

Move: `TextBuffer` struct + all methods. Change field visibility to `pub(super)` for fields accessed by editor and which-key handler.

Commit: `refactor(tui): extract studio/buffer.rs`

### Task 3.5: Extract lsp.rs (~175 lines)

Move: `CompletionState`, `CompletionEntry`, `HoverState`, `CodeActionState`, `CodeActionDisplay` + trigger/accept methods as `impl YamlEditorPanel` in lsp.rs.

Commit: `refactor(tui): extract studio/lsp.rs`

### Task 3.6: Extract editor.rs (~290 lines)

Move: `YamlEditorPanel` struct + `new()`, `tick()`, `load_file()`, `save_file()`, `undo()`, `redo()`, `validate()`, diagnostics accessors.

Commit: `refactor(tui): extract studio/editor.rs`

### Task 3.7: Extract editor_keys.rs (~470 lines)

Move: `handle_normal_mode()`, `handle_insert_mode()`, standalone `View` impl for YamlEditorPanel.

Commit: `refactor(tui): extract studio/editor_keys.rs`

### Task 3.8: Extract editor_render.rs (~500 lines)

Move: `render_editor()` + completion/hover/code-action popup rendering + cursor positioning.

Commit: `refactor(tui): extract studio/editor_render.rs`

### Task 3.9: Extract remaining modules

Move: `dag.rs` (DAG rendering), `browser.rs` (file browser), `overlays.rs` (palette + which-key), `tests.rs` (all tests).

Commit: `refactor(tui): extract studio/dag.rs, browser.rs, overlays.rs, tests.rs`

---

## CHECKPOINT: Full verification

```bash
cargo build --features tui
cargo test --lib -- tui
cargo clippy --features tui -- -W clippy::all
```

Verify:
- [ ] 2114+ TUI tests pass
- [ ] Zero compat:: imports remaining
- [ ] Zero solarized:: in widget code (only in nika_intro intentionally)
- [ ] studio.rs split into 12 files, each < 700 lines
- [ ] Theme has ~75 fields, all populated
- [ ] Theme cycling (T key) updates ALL widgets

---

## Summary

| Phase | Tasks | Impact | Est. Time |
|-------|-------|--------|-----------|
| 0. Dead code | 2 | -10 dead items, -13 false annotations | 5 min |
| 1. Expand Theme | 3 | 35→75 fields (diagnostics, verbs, syntax) | 30 min |
| 2. Migrate colors | 3 | Delete compat.rs + 3 solarized + 250 inline RGB | 60 min |
| 3. Split studio.rs | 9 | 4206 lines → 12 files (~350 avg) | 45 min |
| **Total** | **17** | **Unified color system + clean architecture** | **~2.5h** |
