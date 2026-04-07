# AI Rules Architecture — Progressive Discovery

**Date**: 2026-04-07
**Status**: Design complete, ready to implement
**Problem**: 5 monolithic 500-line rule files → AI absorbs maybe 30%
**Solution**: Multi-layer progressive discovery with live MCP tools

## The Principle: Progressive Disclosure

```
┌─────────────────────────────────────────────────────────────┐
│                                                             │
│  Layer 0: IDENTITY (always loaded, <20 lines)               │
│  "This is a Nika project. 5 verbs. Schema @0.12."          │
│  "For details: call nika MCP tools or read the references." │
│                                                             │
│  Layer 1: SYNTAX (loaded when editing *.nika.yaml)          │
│  The 5 verbs with 1 example each. Data flow basics.        │
│  ~100 lines. Focused. Actionable.                           │
│                                                             │
│  Layer 2: REFERENCE (loaded on demand / by request)         │
│  Transforms list. Error table. Provider matrix.             │
│  ~80 lines per topic. AI requests when needed.              │
│                                                             │
│  Layer 3: LIVE (MCP + LSP, real-time)                       │
│  nika_schema → full schema on demand                        │
│  nika_check → validate after writing                        │
│  nika_error_lookup → explain any NIKA-XXX code              │
│  LSP → completions, hover, diagnostics                      │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

## File Structure

### Source files (tools/nika-cli/rules/)

```
tools/nika-cli/rules/
├── shared/                          ← Content modules (reused across all AI tools)
│   ├── identity.md                  ← 15 lines: what is Nika, schema, install
│   ├── verbs.md                     ← 80 lines: 5 verbs with examples
│   ├── data-flow.md                 ← 60 lines: with, depends_on, templates, transforms
│   ├── structured-output.md         ← 40 lines: 5-layer defense, schema validation
│   ├── common-mistakes.md           ← 50 lines: the error table
│   ├── providers.md                 ← 30 lines: 16 providers, slash syntax
│   └── advanced.md                  ← 60 lines: for_each, agent, scheduling, on_error
│
├── claude/                          ← Claude Code specific
│   ├── identity.md                  ← Layer 0 (alwaysApply equivalent)
│   ├── syntax.md                    ← Layer 1 (combines verbs + data-flow)
│   └── reference.md                 ← Layer 2 (transforms, errors, providers)
│
├── cursor/                          ← Cursor specific
│   ├── nika-project.mdc             ← Layer 0 (alwaysApply: true, 15 lines)
│   ├── nika-syntax.mdc              ← Layer 1 (globs: *.nika.yaml, ~100 lines)
│   └── nika-reference.mdc           ← Layer 2 (Agent Requested, ~80 lines)
│
├── copilot/                         ← GitHub Copilot specific
│   ├── nika.instructions.md         ← .github/copilot-instructions.md (general)
│   └── nika-yaml.instructions.md    ← .github/instructions/ (applyTo: *.nika.yaml)
│
├── windsurf/                        ← Windsurf specific
│   ├── nika-project.md              ← Always on (<6000 chars)
│   └── nika-syntax.md               ← Glob targeted
│
├── roo/                             ← Roo Code specific
│   ├── nika-project.md              ← General rules
│   └── nika-syntax.md               ← Workflow-specific
│
├── gemini/                          ← NEW: Gemini CLI
│   └── GEMINI.md                    ← .gemini/GEMINI.md
│
├── amazonq/                         ← NEW: Amazon Q
│   └── nika.rule.md                 ← .amazonq/rules/nika.rule.md
│
├── jetbrains/                       ← NEW: JetBrains AI
│   └── nika.md                      ← .aiassistant/rules/nika.md
│
└── cline/                           ← NEW: Cline
    └── clinerules                   ← .clinerules
```

### Deployed structure (after nika init)

```
my-project/
├── nika.toml
├── AGENTS.md                        ← Cross-tool (identity + syntax, ~120 lines)
├── CLAUDE.md → AGENTS.md            ← Symlink
├── .mcp.json                        ← Pre-populated with nika MCP server
│
├── .claude/
│   ├── rules/
│   │   └── nika-workflows.md        ← Layer 1+2 (syntax + reference, ~180 lines)
│   └── settings.json                ← Hooks (auto-validate .nika.yaml) + permissions
│
├── .cursor/
│   ├── rules/
│   │   ├── nika-project.mdc         ← Layer 0 (alwaysApply, 15 lines)
│   │   ├── nika-syntax.mdc          ← Layer 1 (globs, ~100 lines)
│   │   └── nika-reference.mdc       ← Layer 2 (Agent Requested, ~80 lines)
│   └── mcp.json                     ← Nika MCP server
│
├── .github/
│   ├── copilot-instructions.md      ← General instructions
│   └── instructions/
│       └── nika.instructions.md     ← Path-specific (applyTo: *.nika.yaml)
│
├── .windsurf/
│   └── rules/
│       └── nika.md                  ← Combined (max 6000 chars)
│
├── .gemini/
│   └── GEMINI.md                    ← Gemini CLI context
│
└── .gitignore                       ← Includes .cursor/mcp.json if sensitive
```

### Home directory (after nika setup)

```
~/
├── .claude/rules/nika.md            ← Global Claude rules
├── .cursor/rules/nika.mdc           ← Global Cursor rules  
├── .windsurf/rules/nika.md          ← Global Windsurf rules
└── .roo/rules/nika.md               ← Global Roo rules
```

## The Content Modules

### identity.md (~15 lines) — ALWAYS loaded

```markdown
# Nika Project

This project uses Nika — a YAML workflow engine for AI tasks.

- Schema: `nika/workflow@0.12`
- Extension: `.nika.yaml`
- 5 verbs: `infer:` (LLM), `exec:` (shell), `fetch:` (HTTP), `invoke:` (tools), `agent:` (loop)
- Validate: `nika check workflow.nika.yaml`
- Execute: `nika run workflow.nika.yaml`
- MCP tools available: call nika_schema for full reference

When writing .nika.yaml files, ALWAYS validate with `nika check` before committing.
```

This is what every AI loads first. 15 lines. The essentials.

### verbs.md (~80 lines) — loaded when editing .nika.yaml

One complete example per verb. No walls of text. Just code.

### data-flow.md (~60 lines) — loaded when editing .nika.yaml

`with:`, `depends_on:`, `$task_id`, `{{with.alias | transform}}`, `for_each:`

### common-mistakes.md (~50 lines) — loaded on demand

The error table. Wrong vs right. The most impactful 15 mistakes.

### providers.md (~30 lines) — loaded on demand

16 providers. Slash syntax. Auto-infer. Env vars.

## The Live Layer (MCP)

The key insight: **don't dump the full reference into static files.
Let the AI CALL nika to learn.**

```
AI: "What transforms are available?"
  → calls nika_schema MCP tool
  → gets live, always-current list of 64 transforms

AI: "Why is this failing with NIKA-045?"
  → calls nika_error_lookup(code: "NIKA-045")
  → gets explanation + fix suggestion

AI: "Show me the workflow structure"
  → calls nika_dag_visualization(file: "workflow.nika.yaml")
  → gets Mermaid DAG
```

This is infinitely better than a static 500-line file because:
1. Always up to date (from the binary, not a stale text file)
2. On demand (doesn't waste context window)
3. Precise (answers the exact question, not "here's everything")

## The Hook Layer (Auto-validation)

```json
// .claude/settings.json
{
  "permissions": {
    "allow": ["mcp__nika__*", "Bash(nika *)"]
  },
  "hooks": {
    "PostToolUse": [{
      "matcher": "Edit|Write",
      "hooks": [{
        "type": "command",
        "command": "FILE=$(cat | jq -r '.tool_input.file_path // empty'); case \"$FILE\" in *.nika.yaml) nika check \"$FILE\" 2>&1 | head -20 ;; esac; exit 0",
        "timeout": 10000
      }]
    }]
  }
}
```

Every time Claude writes a `.nika.yaml` → auto-validates via `nika check`.
Errors injected as context → Claude self-corrects immediately.

## Build System

### Compile-time embedding (existing, keep)
```rust
// init.rs
const IDENTITY: &str = include_str!("../rules/shared/identity.md");
const VERBS: &str = include_str!("../rules/shared/verbs.md");
const DATA_FLOW: &str = include_str!("../rules/shared/data-flow.md");
// ... assemble per-tool from modules
```

### Generation (new)
```rust
fn generate_claude_rules() -> String {
    format!("{}\n\n{}\n\n{}", IDENTITY, VERBS, DATA_FLOW)
}

fn generate_cursor_project_mdc() -> String {
    format!("---\ndescription: Nika project identity\nalwaysApply: true\n---\n\n{}", IDENTITY)
}

fn generate_cursor_syntax_mdc() -> String {
    format!("---\ndescription: Nika workflow syntax\nglobs: [\"**/*.nika.yaml\"]\n---\n\n{}\n\n{}", VERBS, DATA_FLOW)
}
```

Modules are composed, not duplicated. One source of truth, N outputs.

## Daemon + Doctor Integration

### Daemon: proactive rule freshness

The daemon should detect CLI version changes and auto-update rules:
```
daemon startup → compare binary_version vs stored_rule_version
  → if different: fast_rule_update() → xxhash per-file → deploy silently
  → log: "AI rules updated for v0.78"
```

Location: `nika-daemon/src/lib.rs` — add `check_rule_freshness()` to startup.
Uses existing `fast_rule_update()` from `install.rs` (already has xxhash tracking).

### Doctor: verify AI ecosystem

New checks for `nika doctor`:
1. **Rules freshness** — xxhash of deployed rules vs embedded rules
2. **MCP config** — .mcp.json has nika server entry?
3. **Cursor MCP** — .cursor/mcp.json has nika server?
4. **AGENTS.md** — exists and version matches?
5. **Editor extensions** — detect installed editors, check for nika extension
6. **Hooks** — .claude/settings.json has PostToolUse validation hook?
7. **LSP binary** — nika-lsp or nika lsp --stdio available?

`--fix` auto-repairs: regenerate stale rules, create missing MCP configs,
install missing AGENTS.md, add hooks to .claude/settings.json.

Location: `nika-cli/src/doctor.rs` — add `check_ai_ecosystem()` section.

### nika init: smart detection

`nika init` should detect installed AI tools and editors:
- Check for ~/.claude/ → generate .claude/ rules
- Check for ~/.cursor/ → generate .cursor/ rules + mcp.json
- Check for `code` in PATH → suggest VS Code extension
- Check for `zed` in PATH → suggest Zed extension
- Only generate files for detected tools (no pollution)

### The Lifecycle

```
INSTALL     nika setup → deploy global rules to ~/
CREATE      nika init  → detect tools, generate project files
VERIFY      nika doctor → check everything, --fix auto-repairs
MAINTAIN    nika daemon → watch version changes, auto-update rules
UPDATE      brew upgrade → daemon triggers fast_rule_update()
```

No stale files. No missing configs. No invisible tools.
The cycle is closed.

## Migration Path

1. Create `shared/` modules from the existing claude.md (extract, don't rewrite)
2. Create per-tool assemblers in init.rs
3. Update nika init to generate multi-file structure
4. Update nika setup to deploy multi-file
5. Update xxhash fingerprinting for per-file tracking
6. Add 4 new AI tools (Gemini, Amazon Q, JetBrains, Cline)
7. Pre-populate .mcp.json and .cursor/mcp.json
8. Add .claude/settings.json with hooks + permissions
