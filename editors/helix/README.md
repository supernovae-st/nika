# Nika for Helix

Helix editor support for [Nika](https://github.com/supernovae-st/nika) YAML workflows (`.nika.yaml`).

## Features

- **Syntax Highlighting** -- Nika-aware highlighting for verbs, task fields, template expressions, dollar references, provider names, tool names, and all 5 semantic verbs
- **LSP Integration** -- Full Language Server Protocol via `nika-lsp`: completions, diagnostics, hover docs, go-to-definition, code actions, semantic tokens, inlay hints, code lens, rename, folding
- **Text Objects** -- Structural selection: select around/inside tasks (`maf`/`mif`), workflow (`mac`/`mic`), key-value pairs (`maa`/`mia`)
- **Auto-Indentation** -- 2-space YAML indentation rules
- **Dual LSP** -- Runs both `nika-lsp` (Nika intelligence) and `yaml-language-server` (general YAML validation) simultaneously

## Prerequisites

Install the Nika LSP server:

```bash
# From crates.io (recommended)
cargo install nika-lsp

# From Homebrew
brew install supernovae-st/tap/nika-lsp

# From source
git clone https://github.com/supernovae-st/nika
cd nika/tools/nika-lsp
cargo install --path .
```

Verify:

```bash
nika-lsp --version
```

Optionally install `yaml-language-server` for additional YAML validation:

```bash
npm install -g yaml-language-server
```

## Installation

### Quick Setup (recommended)

```bash
# From the nika repository root:
cd editors/helix

# Copy language config (merge manually if you have an existing languages.toml)
cp languages.toml ~/.config/helix/languages.toml

# Copy query files
mkdir -p ~/.config/helix/runtime/queries/nika
cp queries/nika/*.scm ~/.config/helix/runtime/queries/nika/
```

### Manual Setup

If you already have a `~/.config/helix/languages.toml`, merge the following:

```toml
[[language]]
name = "nika"
scope = "source.nika"
injection-regex = "nika"
file-types = [{ glob = "*.nika.yaml" }]
roots = ["nika.toml", ".nika"]
language-servers = ["nika-lsp", "yaml-language-server"]
indent = { tab-width = 2, unit = "  " }
comment-token = "#"
auto-format = false
grammar = "yaml"

[language-server.nika-lsp]
command = "nika-lsp"

[language-server.yaml-language-server]
command = "yaml-language-server"
args = ["--stdio"]

[language-server.yaml-language-server.config.yaml]
schemas = { "https://nika.sh/schema/workflow.json" = "*.nika.yaml" }
validate = true
hover = true
completion = true
```

Then copy the query files:

```bash
mkdir -p ~/.config/helix/runtime/queries/nika
cp queries/nika/*.scm ~/.config/helix/runtime/queries/nika/
```

### Using `nika lsp` Instead of `nika-lsp`

If you have the `nika` CLI with the `lsp` feature (check with `nika features`), you can use it directly instead of installing the standalone `nika-lsp` binary:

```toml
[language-server.nika-lsp]
command = "nika"
args = ["lsp", "--stdio"]
```

## Verification

Check that Helix recognizes the language and LSP:

```bash
hx --health nika
```

You should see:

```
Language  nika
LSP       nika-lsp ✓
```

Open any `.nika.yaml` file:

```bash
hx my-workflow.nika.yaml
```

## What You Get

### Syntax Highlighting

| Element | Highlight | Example |
|---------|-----------|---------|
| Schema declaration | `@keyword` | `schema:` |
| 5 Semantic verbs | `@keyword.control` | `infer:`, `exec:`, `fetch:`, `invoke:`, `agent:` |
| Top-level keys | `@keyword` | `tasks:`, `inputs:`, `skills:` |
| Task ID | `@function` | `id: my_task` |
| Task fields | `@type` | `with:`, `depends_on:`, `for_each:` |
| Verb sub-fields | `@variable.other.member` | `prompt:`, `url:`, `command:` |
| Dollar references | `@variable` | `$task_id`, `$inputs.locale` |
| Env variables | `@variable.builtin` | `$env.API_KEY` |
| Provider names | `@constant` | `anthropic`, `openai`, `gemini` |
| Tool names | `@function` | `nika:import`, `server::tool` |
| Booleans | `@constant.builtin.boolean` | `true`, `false` |
| Numbers | `@constant.numeric` | `0.7`, `1000` |

### Text Objects

| Keymap | Selects |
|--------|---------|
| `maf` | Around task (full sequence item) |
| `mif` | Inside task (task body mapping) |
| `mac` | Around workflow (entire document) |
| `mic` | Inside workflow (document body) |
| `maa` | Around key-value pair |
| `mia` | Inside key-value pair (value only) |

### LSP Features (from nika-lsp)

| Feature | Description |
|---------|-------------|
| Completions | 5 verbs, task fields, MCP tools, `{{with.*}}` templates |
| Diagnostics | NIKA-XXX error codes with fix suggestions |
| Hover | Inline documentation for verbs and parameters |
| Go-to-definition | Navigate to task definitions |
| Code actions | Quick fixes for common errors |
| Semantic tokens | Rich token classification from LSP |
| Inlay hints | Type hints for bindings |
| Code lens | Run/check actions on workflows |
| Rename | Rename task IDs across references |
| Folding | Collapse task blocks |

## File Structure

```
editors/helix/
├── languages.toml           # Language + LSP configuration
├── queries/
│   └── nika/
│       ├── highlights.scm   # Syntax highlighting rules
│       ├── textobjects.scm  # Structural text objects
│       └── indents.scm      # Auto-indentation rules
└── README.md                # This file
```

## Troubleshooting

### LSP not starting

1. Verify `nika-lsp` is on your `$PATH`: `which nika-lsp`
2. Check Helix health: `hx --health nika`
3. Check Helix log: `hx --log-file` (then `tail -f` the path)
4. Test manually: `echo '{}' | nika-lsp` (should not crash)

### Files not recognized as Nika

Nika workflows must use the `.nika.yaml` extension. Plain `.yaml` files will use the standard YAML language.

### Highlighting looks like plain YAML

Ensure query files are copied to the correct location:

```bash
ls ~/.config/helix/runtime/queries/nika/
# Should show: highlights.scm  indents.scm  textobjects.scm
```

### yaml-language-server errors

The `yaml-language-server` is optional. If you do not have it installed, remove it from the `language-servers` list:

```toml
language-servers = ["nika-lsp"]
```

## Related

- [nika-lsp](../../tools/nika-lsp/) -- Language Server Protocol implementation
- [VS Code extension](../vscode/) -- Visual Studio Code support
- [Nika documentation](https://github.com/supernovae-st/nika) -- Full reference

## License

AGPL-3.0-or-later -- see [LICENSE](../../LICENSE) for details.
