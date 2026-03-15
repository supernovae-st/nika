# nika-lsp

Language Server Protocol (LSP) implementation for Nika YAML workflows.

[![Version](https://img.shields.io/crates/v/nika-lsp?style=flat-square&color=7c3aed)](https://crates.io/crates/nika-lsp)
[![License](https://img.shields.io/crates/l/nika-lsp?style=flat-square)](LICENSE)
[![Docs](https://img.shields.io/docsrs/nika-lsp?style=flat-square)](https://docs.rs/nika-lsp)

## Features

- **Syntax Validation** — Real-time YAML validation for `.nika.yaml` workflows
- **Autocompletion** — 5 semantic verbs (`infer`, `exec`, `fetch`, `invoke`, `agent`)
- **MCP Tool Discovery** — Completion for available MCP tools
- **Task Dependency Validation** — Validates `with:` references and DAG structure
- **Template Variable Completion** — `{{with.*}}` and `{{inputs.*}}` variables
- **Go-to-Definition** — Navigate to task definitions
- **Hover Documentation** — Inline docs for verbs and parameters
- **Diagnostics** — NIKA error codes with actionable suggestions

## Installation

### From crates.io (recommended)

```bash
cargo install nika-lsp
```

### From Homebrew

```bash
brew install supernovae-st/tap/nika-lsp
```

### From source

```bash
git clone https://github.com/SuperNovae-studio/nika
cd nika/tools/nika-lsp
cargo install --path .
```

### Verify installation

```bash
nika-lsp --version
```

## Editor Setup

### VS Code

1. Install the [YAML extension](https://marketplace.visualstudio.com/items?itemName=redhat.vscode-yaml)
2. Add to your `settings.json`:

```json
{
  "yaml.customTags": ["!include scalar"],
  "yaml.schemas": {
    "https://nika.sh/schema/workflow.json": "*.nika.yaml"
  },
  "yaml.languageserver.enabled": false,
  "[yaml]": {
    "editor.defaultFormatter": null
  }
}
```

3. Configure the LSP in VS Code:

   - Install [LSP client extension](https://marketplace.visualstudio.com/items?itemName=jeanp413.open-remote-ssh) or configure manually
   - Add nika-lsp as a custom language server

### Neovim (nvim-lspconfig)

Add to your Neovim configuration:

```lua
local lspconfig = require('lspconfig')
local configs = require('lspconfig.configs')

-- Define nika-lsp if not already defined
if not configs.nika_lsp then
  configs.nika_lsp = {
    default_config = {
      cmd = { 'nika-lsp' },
      filetypes = { 'yaml' },
      root_dir = function(fname)
        return lspconfig.util.root_pattern('.nika', 'nika.yaml', '.git')(fname)
      end,
      settings = {},
    },
  }
end

-- Enable nika-lsp
lspconfig.nika_lsp.setup({
  on_attach = function(client, bufnr)
    -- Your on_attach function here
  end,
})
```

### Helix

Add to `~/.config/helix/languages.toml`:

```toml
[[language]]
name = "yaml"
language-servers = ["nika-lsp", "yaml-language-server"]

[language-server.nika-lsp]
command = "nika-lsp"
```

### Zed

Add to your Zed settings:

```json
{
  "lsp": {
    "nika-lsp": {
      "binary": {
        "path": "nika-lsp"
      }
    }
  },
  "languages": {
    "YAML": {
      "language_servers": ["nika-lsp", "yaml-language-server"]
    }
  }
}
```

### Sublime Text (LSP package)

Add to LSP settings:

```json
{
  "clients": {
    "nika-lsp": {
      "command": ["nika-lsp"],
      "selector": "source.yaml",
      "schemes": ["file"]
    }
  }
}
```

### Emacs (lsp-mode)

Add to your Emacs configuration:

```elisp
(with-eval-after-load 'lsp-mode
  (add-to-list 'lsp-language-id-configuration
    '(yaml-mode . "yaml"))
  (lsp-register-client
    (make-lsp-client
      :new-connection (lsp-stdio-connection '("nika-lsp"))
      :activation-fn (lsp-activate-on "yaml")
      :server-id 'nika-lsp)))
```

## Usage

The LSP server starts automatically when your editor opens a `.nika.yaml` file. Features include:

### Completions

Type a verb and get intelligent suggestions:

```yaml
tasks:
  - id: my_task
    # Type 'inf' and get 'infer:' completion with documentation
```

### Diagnostics

Get real-time error feedback:

```yaml
tasks:
  - id: task1
    with:
      data: nonexistent_task  # Error: Unknown task 'nonexistent_task'
    infer: "Process {{with.data}}"
```

### Hover Documentation

Hover over verbs for inline documentation:

```yaml
tasks:
  - id: research
    agent:  # Hover for: "Multi-turn agentic loop with MCP tools"
      prompt: "Research AI papers"
```

## Configuration

The LSP respects the following settings (when supported by your editor):

| Setting | Default | Description |
|---------|---------|-------------|
| `nika.validation.enabled` | `true` | Enable/disable validation |
| `nika.completion.providers` | `true` | Show provider completions |
| `nika.completion.mcpTools` | `true` | Show MCP tool completions |
| `nika.diagnostics.delay` | `300` | Debounce delay in ms |

## Supported Schema Versions

- `nika/workflow@0.9` (current)
- `nika/workflow@0.8`
- `nika/workflow@0.5`
- `nika/workflow@0.3`
- `nika/workflow@0.1`

## Development

```bash
# Build
cargo build

# Test
cargo test

# Run with logging
RUST_LOG=debug nika-lsp
```

## Architecture

```
nika-lsp/
├── src/
│   ├── main.rs           # LSP server entry point
│   ├── backend.rs        # Language server implementation
│   ├── completion.rs     # Completion provider
│   ├── diagnostics.rs    # Validation diagnostics
│   ├── hover.rs          # Hover documentation
│   └── document.rs       # Document management
└── Cargo.toml
```

The LSP integrates with the main `nika` crate for:

- **AST parsing** — Uses `nika::ast::parse_yaml()` for accurate parsing
- **Validation** — Leverages `nika::analyzer` for semantic validation
- **Error codes** — Surfaces NIKA-xxx error codes with fix suggestions

## Related

- [nika](https://github.com/SuperNovae-studio/nika) — Main CLI and runtime
- [nika.sh](https://nika.sh) — Documentation
- [JSON Schema](https://nika.sh/schema/workflow.json) — Workflow schema

## License

MIT License — see [LICENSE](../../LICENSE) for details.
