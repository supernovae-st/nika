# Nika Workflow Language - VS Code Extension

Language support for [Nika](https://github.com/SuperNovae-studio/nika) `.nika.yaml` workflow files.

## Features

- **Syntax Highlighting** - TextMate grammar for all Nika constructs (verbs, fields, templates, task IDs)
- **Semantic Tokens** - LSP-powered token classification for context-aware coloring
- **Completions** - Context-aware completions for verbs, fields, task references, template variables
- **Hover Documentation** - Rich markdown docs for all 5 verbs, fields, and template expressions
- **Go to Definition** - Jump to task definitions from `with:` bindings and `{{with.alias}}` templates
- **Code Actions** - Quick fixes for unknown task references, missing fields, schema version updates
- **Document Symbols** - Outline view with workflow structure, tasks, and verbs
- **Diagnostics** - Real-time error reporting from the Nika analyzer (Phase 2 AST)

## Requirements

The `nika` binary must be installed and accessible in your PATH.

```bash
# Install from source
cd nika/tools/nika
cargo install --path . --features lsp

# Verify
nika lsp --help
```

## Extension Settings

| Setting | Default | Description |
|---------|---------|-------------|
| `nika.server.path` | `"nika"` | Path to the nika binary |
| `nika.server.extraArgs` | `[]` | Extra arguments for `nika lsp` |
| `nika.trace.server` | `"off"` | Trace LSP communication |

## Supported Syntax

```yaml
schema: nika/workflow@0.12
workflow: example
provider: anthropic

tasks:
  - id: generate
    infer: "Generate a title for {{with.topic}}"

  - id: process
    with:
      title: $generate
    exec: "echo {{with.title}}"
    depends_on: [generate]
```

### 5 Semantic Verbs

| Verb | Icon | Purpose |
|------|------|---------|
| `infer:` | lightning | LLM generation |
| `exec:` | terminal | Shell command |
| `fetch:` | satellite | HTTP request |
| `invoke:` | plug | MCP tool call |
| `agent:` | chicken | Agentic loop |

## Development

```bash
cd editors/vscode
npm install
npm run compile
# Press F5 in VS Code to launch Extension Development Host
```

## License

MIT
