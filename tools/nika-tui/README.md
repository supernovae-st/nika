# nika-tui

Terminal UI for Nika — built with ratatui.

## 3-View Architecture

```
+------------------------------------------------------------------+
| 1/s Studio    | 2/c Command   | 3/x Control                     |
|------------------------------------------------------------------+
| Browser | Editor | DAG    | Chat / Monitor  | System dashboard   |
+------------------------------------------------------------------+
```

- **Studio** — 3-panel: file browser, YAML editor (with LSP), DAG visualizer
- **Command** — Chat mode (agent interaction) or Monitor mode (workflow execution)
- **Control** — System dashboard, provider status, MCP connections

## Key Features

- Embedded LSP (nika-lsp-core) for hover, completion, go-to-definition
- Tree-sitter syntax highlighting
- Vi-mode keybindings with which-key popup
- Live DAG rendering during workflow execution
- Tailwind color theme system (60 fields)

## Dependencies

Depends on `nika-engine` only (not `nika` binary) — clean one-way layering.

## License

AGPL-3.0-or-later
