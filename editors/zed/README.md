# Nika for Zed

Language support for [Nika](https://nika.sh) workflow files (`.nika.yaml`) in the [Zed](https://zed.dev) editor.

## Features

- **Syntax highlighting** -- YAML-aware highlighting with Nika-specific semantic tokens for verbs, task IDs, keywords, template expressions, and dollar references
- **LSP integration** -- Full Language Server Protocol support via `nika-lsp` (diagnostics, completions, hover docs, go-to-definition)
- **Code outline** -- Navigate workflow structure (tasks, inputs, skills) in the outline panel
- **Bracket matching** -- Matching for `{}`, `[]`, `""`, `''`, and `{{}}`
- **Smart indentation** -- Correct indent behavior for YAML block mappings and sequences

## Prerequisites

Install the Nika CLI (which includes the LSP):

```bash
# Homebrew (macOS/Linux)
brew install supernovae-st/tap/nika

# Cargo
cargo install nika-lsp

# Or from source
git clone https://github.com/supernovae-st/nika
cd nika/tools/nika-lsp
cargo install --path .
```

Verify the LSP is available:

```bash
nika-lsp --version
# or
nika lsp --version
```

## Installation

### From Zed Extensions (when published)

1. Open Zed
2. `Cmd+Shift+X` to open Extensions
3. Search for "Nika"
4. Click Install

### From source (development)

1. Clone this repository
2. In Zed, open **Extensions** > **Install Dev Extension**
3. Select the `editors/zed/` directory

## How It Works

The extension registers `.nika.yaml` files as the "Nika" language and uses tree-sitter-yaml for parsing. Nika-specific highlighting queries match YAML keys against known Nika keywords to provide semantic coloring:

| Element | Highlight |
|---------|-----------|
| `schema:` | keyword |
| `workflow:` name | function |
| `infer:`, `exec:`, `fetch:`, `invoke:`, `agent:` | keyword.function (verbs) |
| `id:` value | function (task ID) |
| `with:`, `depends_on:`, `for_each:`, `when:`, etc. | type (task fields) |
| `prompt:`, `url:`, `command:`, `tool:`, etc. | property (verb sub-fields) |
| `provider:` value | constant |
| `true` / `false` | boolean |
| Numbers | number |
| Strings | string |
| Comments | comment |

The LSP binary (`nika-lsp` or `nika lsp --stdio`) is located automatically via PATH lookup, with fallbacks to common install locations (Homebrew, Cargo, `~/.nika/bin`).

## Configuration

The Nika LSP server uses sensible defaults out of the box -- no configuration is
required. All features (validation, completions, hover, diagnostics) are enabled
automatically.

To override server defaults, add `initialization_options` in your Zed `settings.json`:

```json
{
  "lsp": {
    "nika-lsp": {
      "initialization_options": {
        "nika": {
          "validation": { "enabled": false },
          "diagnostics": { "delay": 500 }
        }
      }
    }
  }
}
```

### Custom binary path

If the Nika binary is not on your PATH:

```json
{
  "lsp": {
    "nika-lsp": {
      "binary": {
        "path": "/path/to/nika-lsp"
      }
    }
  }
}
```

## File Structure

```
editors/zed/
  extension.toml          # Extension manifest (grammars, language servers)
  Cargo.toml              # Rust build config (WASM cdylib)
  src/
    lib.rs                # LSP binary discovery and launch
  languages/
    nika/
      config.toml         # Language metadata (suffixes, comments, brackets)
      highlights.scm      # Syntax highlighting queries
      brackets.scm        # Bracket matching queries
      outline.scm         # Code outline/structure queries
      indents.scm         # Indentation rules
      injections.scm      # Language injection (reserved for future use)
  README.md               # This file
```

## Development

```bash
# Build the WASM extension
cd editors/zed
cargo build --target wasm32-wasi

# Test highlighting queries against a sample file
# (use Zed's built-in "Syntax Tree" view: Cmd+Shift+P > "Debug: Show Syntax Tree")
```

To iterate on highlighting:

1. Open a `.nika.yaml` file in Zed
2. Open the syntax tree inspector (`Cmd+Shift+P` > "Debug: Show Syntax Tree")
3. Edit `highlights.scm` -- Zed reloads queries on save for dev extensions

## License

AGPL-3.0-or-later -- see [LICENSE](../../LICENSE) for details.
