# Nika × Zed — Deep Integration Plan

**Date**: 2026-04-07
**Status**: Research complete, ready to implement
**Goal**: Best-in-class Nika integration in Zed — 4 layers, Rust-native

## Why Zed Matters

Both Nika and Zed are Rust. Both target the same audience: developers who want
fast, modern, no-bullshit tools. Zed is growing fast in the Rust community —
our exact target. A killer Zed extension is not a "nice-to-have", it's a
competitive advantage nobody else will have.

## Architecture: 4 Layers

```
┌─────────────────────────────────────────────────┐
│                    Zed Editor                     │
├──────────┬──────────┬───────────┬────────────────┤
│ Layer 1  │ Layer 2  │ Layer 3   │ Layer 4        │
│ LSP      │ MCP      │ Tree-sit  │ Tasks          │
│          │ Context  │ Queries   │                │
│ nika lsp │ nika mcp │ .scm      │ .zed/tasks.json│
│ --stdio  │          │ files     │                │
└──────────┴──────────┴───────────┴────────────────┘
```

### Layer 1: Language Server (DONE)

The Zed extension already registers `nika-lsp` via the `Extension` trait.
Binary discovery: `nika-lsp` (dedicated) > `nika lsp --stdio` (fallback).

**Capabilities via LSP:**
- Diagnostics (NIKA-XXX error codes)
- Completions (verbs, fields, transforms, builtins, providers, models)
- Hover documentation
- Go-to-definition
- Semantic tokens (verbs, task IDs, templates)
- Code lens (Run, Validate)
- Inlay hints (timeout, cost, model)
- References, rename, document links, folding

### Layer 2: MCP Context Server (NEW — KEY FEATURE)

**This is the game-changer.** Nika registers as a MCP server in Zed's AI Agent
Panel. The AI agent can then use Nika tools to help users write and debug
workflows.

```toml
# extension.toml
[context_servers.nika-mcp]
```

```rust
fn context_server_command(
    &mut self,
    _id: &ContextServerId,
    _project: &zed::Project,
) -> Result<zed::Command> {
    let binary = self.find_binary()?;
    Ok(zed::Command {
        command: binary,
        args: vec!["mcp".to_string()],
        env: worktree.shell_env(),
    })
}
```

**What the AI agent can do with Nika MCP:**
- `nika_check` — validate a workflow from the Agent Panel
- `nika_schema` — get the full schema for authoring
- `nika_error_lookup` — explain any NIKA-XXX error
- `nika_list_workflows` — discover project workflows
- `nika_generate_task` — scaffold tasks from description
- `nika_dag_visualization` — get workflow DAG as Mermaid

**User experience:**
> "Hey, add a translation task to my workflow that uses DeepL for French"
> → Agent calls `nika_generate_task` → gets validated YAML → inserts it

> "Why is task 'scrape' failing with NIKA-045?"
> → Agent calls `nika_error_lookup(code: "NIKA-045")` → explains SSRF block

### Layer 3: Tree-sitter Queries (DONE)

Already created: `highlights.scm`, `brackets.scm`, `outline.scm`, `indents.scm`.
Unified with Neovim and Helix for feature parity.

**Next step: `runnables.scm`** — detect workflow definitions and show inline
"Run ▶" buttons next to `workflow:` declarations:

```scheme
; runnables.scm — inline run buttons for workflows
(block_mapping_pair
  key: (flow_node (plain_scalar (string_scalar) @_key))
  value: (flow_node (plain_scalar (string_scalar) @run))
  (#eq? @_key "workflow"))
```

### Layer 4: Task Templates (NEW)

Ship a `.zed/tasks.json` template that users can copy:

```json
[
  {
    "label": "Nika: Run workflow",
    "command": "nika run $ZED_FILE",
    "tags": ["nika-run"],
    "save": "current"
  },
  {
    "label": "Nika: Check workflow",
    "command": "nika check $ZED_FILE",
    "tags": ["nika-check"]
  },
  {
    "label": "Nika: Lint workflow",
    "command": "nika lint $ZED_FILE",
    "tags": ["nika-lint"]
  },
  {
    "label": "Nika: Test (mock)",
    "command": "nika test $ZED_FILE",
    "tags": ["nika-test"]
  },
  {
    "label": "Nika: Explain workflow",
    "command": "nika explain $ZED_FILE"
  },
  {
    "label": "Nika: Dry run",
    "command": "nika run $ZED_FILE --dry-run"
  }
]
```

## Implementation Plan

### Phase 1: MCP Context Server (2h)

1. Add `[context_servers.nika-mcp]` to `extension.toml`
2. Implement `context_server_command()` in `lib.rs`
3. Add `context_server_configuration()` with install instructions + settings schema
4. Test: open Zed, open Agent Panel, verify Nika MCP tools are available

### Phase 2: Runnables (1h)

1. Create `languages/nika/runnables.scm`
2. Detect `workflow:` declarations → inline "Run" button
3. Map to task tag `nika-run`
4. Test: open a `.nika.yaml`, verify ▶ button appears

### Phase 3: Task Templates (30min)

1. Create `editors/zed/tasks.json.example`
2. Document in README how to copy to `.zed/tasks.json`
3. Add keybinding recommendations

### Phase 4: Binary Distribution (1h)

1. Implement `download_file` for auto-installing `nika` binary
2. Check GitHub releases for platform-appropriate binary
3. Cache in extension data directory
4. Fallback: clear error with install instructions

### Phase 5: Publish to Zed Extensions Registry

1. Fork `zed-industries/extensions`
2. Add `editors/zed/` as extension entry
3. Submit PR
4. License: extension code as GPLv3 (accepted by Zed), binary is AGPL

## Key Research Findings

| Feature | Available? | Notes |
|---------|-----------|-------|
| LSP | YES | Full support, stdio |
| MCP Context Server | YES | `nika mcp` as context server |
| Slash commands | REMOVED | Replaced by MCP servers |
| Tasks | JSON only | No extension API, ship template |
| Inline completions | NO | Edit Prediction not extensible |
| Terminal control | NO | Use tasks instead |
| Runnables (inline run) | YES | Via `runnables.scm` |
| Semantic tokens | YES | Configurable mapping |
| Process spawning | YES | `process::Command` (capability-gated) |
| HTTP requests | YES | Full HTTP client with streaming |
| File download | YES | For binary distribution |

## WASM Extension Capabilities

**CAN do**: spawn processes, HTTP requests, download files, read worktree,
check PATH, shell env, GitHub API, npm install

**CANNOT do**: inline ghost text, register tasks programmatically, terminal
control, editor buffer access (only via LSP), custom UI panels, keybindings,
notifications, clipboard

## License Note

Zed extensions accept: Apache 2.0, BSD, MIT, Unlicense, zlib, CC BY 4.0,
GPLv3, LGPLv3. The extension code should be GPLv3. The `nika` binary it
downloads/locates is AGPL-3.0 — that's fine, the extension doesn't link it.
