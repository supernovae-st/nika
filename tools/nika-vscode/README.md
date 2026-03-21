# Nika — VS Code Extension

Language support for `.nika.yaml` workflow files, powered by the Nika LSP.

## Requirements

The `nika` binary must be installed and available on your `PATH`:

```bash
# From source
cargo install --path tools/nika

# Or via brew
brew install supernovae-studio/tap/nika
```

## Features

- **Diagnostics** — Real-time validation with NIKA-XXX error codes
- **Completion** — Verbs, keys, providers, models, `nika:*` builtins, MCP tools
- **Hover** — Documentation for verbs, keys, providers, error codes
- **Go to Definition** — Jump to task definitions from `depends_on` and `$task_ref`
- **Semantic Tokens** — Context-aware highlighting for verbs, templates, references
- **Syntax Highlighting** — TextMate grammar for `.nika.yaml` files

## Configuration

| Setting | Default | Description |
|---------|---------|-------------|
| `nika.lsp.path` | `nika` | Path to the `nika` binary |
| `nika.lsp.enabled` | `true` | Enable/disable the language server |

## Install from VSIX

```bash
cd tools/nika-vscode
npm install && npm run compile
npx @vscode/vsce package
code --install-extension nika-0.35.3.vsix
```

## License

AGPL-3.0-or-later
