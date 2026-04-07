# Nika Editor Extensions

Editor integrations for the Nika workflow language (`.nika.yaml` files). All editors share a single LSP server (`nika lsp`) for diagnostics, completions, hover docs, and go-to-definition, with editor-specific syntax highlighting and UI on top.

## Supported Editors

| Editor | Syntax | LSP | Status |
|--------|--------|-----|--------|
| **VS Code** | TextMate grammar | `nika lsp --stdio` | Full (marketplace) |
| **Zed** | Tree-sitter queries | `nika-lsp` | Full (extension) |
| **Neovim** | Tree-sitter queries | `nika lsp --stdio` via nvim-lspconfig | Full (plugin) |
| **Helix** | YAML grammar + queries | `nika-lsp` | Config only |

## Architecture

```
  Editor UI Layer (syntax, snippets, commands, DAG webview)
  ─────────────────────────────────────────────────────────
  VS Code         Zed          Neovim         Helix
  (TypeScript)    (Rust ext)   (Lua plugin)   (TOML config)
  tmLanguage.json highlights.scm highlights.scm languages.toml
  ─────────────────────────────────────────────────────────
                    Shared LSP
              ┌─────────────────┐
              │  nika lsp       │  (or nika-lsp binary)
              │  nika-lsp-core  │  Rust crate
              └─────────────────┘
              │  nika-core AST  │  Parser, analyzer, catalogs
              └─────────────────┘
```

### Shared (via LSP)

- Diagnostics (syntax errors, unknown fields, DAG cycles)
- Completions (verbs, fields, transforms, builtin tools, $references)
- Hover documentation (field descriptions, transform help)
- Go-to-definition ($task_id references)
- Code actions (quick fixes for common mistakes)
- Inlay hints (timeout annotations, model costs)
- Code lens (Run, Validate, task count)
- Semantic tokens (verbs, fields, templates, references)

### Editor-Specific

- **VS Code**: TextMate grammar, snippets, DAG webview, sidebar tree, run/check commands
- **Zed**: Tree-sitter highlights, brackets, outline, indents, injections
- **Neovim**: Tree-sitter highlights, ftdetect, ftplugin, health check
- **Helix**: Language server config, file type detection

## Keyword Categories

All editors must highlight the same set of keywords, extracted from the Rust source:

| Category | Source of Truth | Count |
|----------|----------------|-------|
| **Verbs** | `nika-core/src/ast/raw/action.rs` | 5 |
| **Transforms** | `nika-core/src/binding/transform.rs` (`KNOWN_TRANSFORM_NAMES`) | 62 |
| **Builtin tools** | `nika-core/src/catalogs/builtins.rs` (`KNOWN_BUILTIN_TOOLS`) | 63 |
| **Workflow keys** | `nika-core/src/ast/raw/parser.rs` (`known_workflow_keys`) | 20 |
| **Task keys** | `nika-core/src/ast/raw/parser.rs` (`KNOWN_TASK_KEYS`) | 35 |

## Sync System

When a new transform, builtin tool, or field is added to the Nika engine, **all editor configs must be updated**. The sync script automates this.

### Usage

```bash
# Check for drift (CI-friendly — exits 1 if drift detected)
./editors/sync-editors.sh

# Auto-fix all editor configs
./editors/sync-editors.sh --fix

# Machine-readable output
./editors/sync-editors.sh --json

# Verbose (show extracted keyword lists)
./editors/sync-editors.sh --verbose
```

### How It Works

1. **Extract** keywords from the Rust source files (the single source of truth)
2. **Parse** each editor's config file format to extract its keyword lists
3. **Compare** source vs editor, reporting missing and extra keywords
4. **Fix** (with `--fix`) by regenerating the keyword sections in each editor config

The script handles each editor's syntax format:

- **VS Code**: JSON TextMate grammar — regex `(keyword1|keyword2)(:)` in match patterns
- **Zed**: Tree-sitter `.scm` — `#match?` with `^(keyword1|keyword2)$` regex patterns
- **Neovim**: Tree-sitter `.scm` — `#any-of?` with `"keyword1" "keyword2"` quoted strings
- **Helix**: TOML config — structural checks only (LSP config, file type detection)

### CI Integration

Add to your CI pipeline:

```yaml
- name: Check editor sync
  run: ./editors/sync-editors.sh
```

The script exits with code 1 if any drift is detected, making it suitable for CI gates.

## Adding a New Editor

1. Create `editors/<editor-name>/` directory
2. Add language config for `.nika.yaml` files
3. Configure the LSP server:
   - Binary: `nika` (with `lsp --stdio` subcommand) or `nika-lsp` standalone
   - Transport: stdio
4. Add syntax highlighting using the keyword lists from `sync-editors.sh --json`
5. Add extraction logic to `sync-editors.sh` for the new editor's format
6. Run `./editors/sync-editors.sh` to verify sync

### LSP Server Setup

The Nika LSP is built into the main `nika` binary:

```bash
# Primary method (built into nika)
nika lsp --stdio

# Alternative (standalone binary, same code)
nika-lsp --stdio
```

Configuration options:
- `nika.server.path`: Path to the `nika` binary (default: from `$PATH`)
- `nika.server.autoDownload`: Auto-download from GitHub releases if not found

## Directory Structure

```
editors/
  sync-editors.sh          Automated sync script
  README.md                This file
  vscode/
    syntaxes/nika.tmLanguage.json   TextMate grammar
    snippets/nika.code-snippets     VS Code snippets
    language-configuration.json     Bracket pairs, comments
    package.json                    Extension manifest
    src/                            TypeScript extension code
  zed/
    extension.toml                  Zed extension manifest
    Cargo.toml                      Rust extension crate
    languages/nika/
      config.toml                   Language config
      highlights.scm                Syntax highlights
      brackets.scm                  Bracket pairs
      outline.scm                   Symbol outline
      indents.scm                   Auto-indentation
      injections.scm                Language injections
  neovim/
    ftdetect/nika.lua               Filetype detection
    ftplugin/yaml.lua               Buffer settings
    lua/nika/health.lua             :checkhealth integration
    after/queries/yaml/highlights.scm  Tree-sitter highlights
  helix/
    languages.toml                  Language + LSP config
```
