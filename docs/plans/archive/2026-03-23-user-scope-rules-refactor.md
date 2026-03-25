# User-Scope AI Editor Rules Refactor

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Move AI editor rules from per-project (nika init) to user-scope (nika setup / machine setup). One install, all editors, auto-update on upgrade.

**Architecture:** `machine.rs` becomes the single source of truth for editor detection + rule installation at `~/`. `init_ai.rs` becomes lightweight (AGENTS.md project context + .vscode recommendation only). `machine.toml` tracks installed editors for quick new-editor detection.

**Baseline:** 43 commits on main, all tests passing.

---

## Task 1: Expand machine.toml to track editors

**Files:** `tools/nika-cli/src/machine.rs`

- Add `editors = [...]` field to machine.toml
- Add `detect_editors()` function that returns Vec<String> of detected editor IDs
- Detection: `which code`, `which cursor`, `which windsurf`, `which claude`, check `~/.roo`, check `~/.github`
- Add `detect_new_editors()` that compares current detection vs stored list

---

## Task 2: Move ALL rule constants from init_ai.rs to machine.rs

**Files:** `tools/nika-cli/src/machine.rs`, `tools/nika-cli/src/init_ai.rs`

Move these constants from init_ai.rs to machine.rs:
- CURSOR_SYNTAX_RULE → CURSOR_RULES (merge syntax+patterns+arch+security into 1 comprehensive file)
- COPILOT_INSTRUCTIONS
- WINDSURF_RULE
- ROO_RULE

Keep in init_ai.rs (project-scope):
- AGENTS_MD_CONTENT (but rewrite to lightweight version)
- VSCODE_EXTENSIONS
- VSCODE_SETTINGS
- ROOMODES (project-level config)

---

## Task 3: Expand setup_ai_rules() to install for ALL editors

**Files:** `tools/nika-cli/src/machine.rs`

For each detected editor, install rules at user scope:

| Editor | Path | Format |
|--------|------|--------|
| Claude Code | `~/.claude/rules/nika.md` | Already done |
| Cursor | `~/.cursor/rules/nika.mdc` | .mdc with frontmatter |
| Copilot | `~/.github/copilot/nika.instructions.md` | .md with applyTo frontmatter |
| Windsurf | `~/.windsurf/rules/nika.md` | .md with trigger frontmatter |
| Roo Code | `~/.roo/rules/nika.md` | .md with glob frontmatter |

Logic: if editor detected → create dir → write/overwrite rule file.

Always overwrite (not write_if_absent) so upgrades work.

---

## Task 4: Rewrite AGENTS.md to lightweight project context

**Files:** `tools/nika-cli/src/init_ai.rs`

Replace the 7k full syntax reference AGENTS_MD_CONTENT with a ~1k project-focused version:

```markdown
# [Project Name]

This project uses [Nika](https://github.com/supernovae-st/nika) workflow engine.
Schema: `nika/workflow@0.12` | Extension: `.nika.yaml`

## Workflows

| Directory | Contents |
|-----------|----------|
| `workflows/minimal/` | 5 starter workflows (1 per verb) |
| `workflows/showcase*/` | 60 showcase examples |

## Quick Start

```bash
nika run workflows/minimal/01-exec.nika.yaml   # No API key needed
nika check <workflow.nika.yaml>                  # Validate syntax
nika provider list                               # Check API key status
```

## Nika Syntax (Quick Reference)

5 verbs: `infer:` (LLM), `exec:` (shell), `fetch:` (HTTP), `invoke:` (MCP), `agent:` (loop)
Bindings: `with: { alias: $task_id }` | Templates: `{{with.alias}}`
Schema: `schema: "nika/workflow@0.12"` (required)

For full syntax reference, install nika: `cargo install nika`
```

---

## Task 5: Strip init_ai.rs generate_ai_files()

**Files:** `tools/nika-cli/src/init_ai.rs`

Remove from generate_ai_files():
- .cursor/rules/nika-syntax.mdc
- .cursor/rules/nika-patterns.mdc
- .cursor/rules/nika-architecture.mdc
- .cursor/rules/nika-security.mdc
- .github/copilot/nika.instructions.md
- .windsurf/rules/nika.md
- .roo/rules/nika.md
- .roomodes

Keep:
- AGENTS.md (lightweight version)
- CLAUDE.md symlink
- .vscode/extensions.json
- .vscode/settings.json
- .git/hooks/prepare-commit-msg

Delete the now-unused constants (CURSOR_SYNTAX_RULE, CURSOR_PATTERNS_RULE, etc.)

---

## Task 6: Add quick editor re-scan to main.rs

**Files:** `tools/nika/src/main.rs` or `tools/nika-cli/src/machine.rs`

On every nika command (in main.rs early init):
1. If machine_setup_status() == NeedsUpdate → full re-setup
2. If machine_setup_status() == Ready → quick_editor_scan()
   - Compare detect_editors() vs stored editors list
   - If new editor found → install rules for it, update machine.toml

This adds ~5ms to every command. Acceptable.

---

## Task 7: Update tests

- Update init_ai tests (content assertions for lighter AGENTS.md)
- Update machine.rs tests (new editor detection, rule installation)
- Verify `cargo test --workspace --lib` passes

---

## Task 8: Cleanup old rule constants

Delete from init_ai.rs:
- CURSOR_SYNTAX_RULE (~300 lines)
- CURSOR_PATTERNS_RULE (~300 lines)
- CURSOR_ARCHITECTURE_RULE (~50 lines)
- CURSOR_SECURITY_RULE (~50 lines)
- COPILOT_INSTRUCTIONS (~300 lines)
- WINDSURF_RULE (~300 lines)
- ROO_RULE (~300 lines)
- ROOMODES (~20 lines)

~1600 lines deleted from init_ai.rs. Moved to machine.rs as unified per-editor rules.

---
