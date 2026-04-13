# Nika Editor Extensions

Editor integration for the Nika workflow language (`.nika.yaml` files). The single
supported editor is VS Code, which uses the shared `nika lsp` server for diagnostics,
completions, hover docs, and go-to-definition on top of a TextMate grammar.

## Supported Editors

| Editor | Syntax | LSP | Status |
|--------|--------|-----|--------|
| **VS Code** | TextMate grammar | `nika lsp --stdio` | Full (marketplace) |

Zed, Neovim and Helix scaffolds were speculative and were removed in the Month A
deletion bloc. They can be rebuilt clean against the diamond runtime if user demand
emerges.

## Architecture

```
  VS Code UI Layer (syntax, snippets, commands, DAG webview)
  ────────────────────────────────────────────────────────
  TypeScript extension
  tmLanguage.json grammar
  ────────────────────────────────────────────────────────
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

### VS Code specific

TextMate grammar, snippets, DAG webview, sidebar tree, run/check commands.

## Keyword Categories

The grammar highlights the same keyword set that `nika check` validates, extracted
from the Rust source:

| Category | Source of Truth | Count |
|----------|----------------|-------|
| **Verbs** | `nika-core/src/ast/raw/action.rs` | 5 |
| **Transforms** | `nika-core/src/binding/transform.rs` (`KNOWN_TRANSFORM_NAMES`) | 65 |
| **Builtin tools** | `nika-core/src/catalogs/builtins.rs` (`KNOWN_BUILTIN_TOOLS`) | 63 |
| **Workflow keys** | `nika-core/src/ast/raw/parser.rs` (`known_workflow_keys`) | 20 |
| **Task keys** | `nika-core/src/ast/raw/parser.rs` (`KNOWN_TASK_KEYS`) | 35 |

## Sync System

When a new transform, builtin tool, or field is added to the Nika engine, the VS
Code grammar must be updated. The sync script automates this.

```bash
# Check for drift (CI-friendly — exits 1 if drift detected)
./editors/sync-editors.sh

# Auto-fix VS Code grammar
./editors/sync-editors.sh --fix

# Machine-readable output
./editors/sync-editors.sh --json

# Verbose (show extracted keyword lists)
./editors/sync-editors.sh --verbose
```

The script extracts canonical keyword lists from `tools/nika-core/src/**` via
`editors/shared/extract-keywords.py`, then patches
`editors/vscode/syntaxes/nika.tmLanguage.json`.

## Directory Structure

```
editors/
  sync-editors.sh          Automated sync script (VS Code only)
  README.md                This file
  shared/
    extract-keywords.py    Source-of-truth keyword extractor
    nika-keywords.json     Generated cache
  vscode/
    syntaxes/nika.tmLanguage.json   TextMate grammar
    snippets/nika.code-snippets     VS Code snippets
    language-configuration.json     Bracket pairs, comments
    package.json                    Extension manifest
    src/                            TypeScript extension code
```
