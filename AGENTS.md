# Nika

Semantic YAML workflow engine for AI tasks. Schema `nika/workflow@0.12` | 64 transforms | 63 builtin tools | [QR Code AI](https://qrcode-ai.com)

## 5 Verbs

| Verb | Purpose |
|------|---------|
| `infer:` | LLM generation |
| `exec:` | Shell command |
| `fetch:` | HTTP request |
| `invoke:` | MCP tool call |
| `agent:` | Multi-turn loop |

## Project Structure — The .git Principle

Nika imposes ZERO directory names. Only `nika.toml` + `.nika/` are Nika's territory.

```
project/
├── nika.toml                 ← Config (versioned, root marker)
├── .nika/                    ← Runtime only (gitignored)
├── *.nika.yaml               ← Workflows — anywhere in project
├── artifacts/                ← Default output dir (configurable)
├── AGENTS.md                 ← AI context (nika init)
└── (user's own structure)    ← No imposed dirs
```

- **Root detection**: walk up from CWD to find `nika.toml`
- **Workflow discovery**: by `*.nika.yaml` extension (recursive scan)
- **Skills/context**: referenced by path in each workflow, not by convention dir
- **MCP config**: `.mcp.json` at project root (Claude Code convention, NOT in nika.toml)
- **Secrets**: NikaVault only (`~/.nika/secrets/vault.enc`), never in `nika.toml`

## Workflow Syntax

`with:` for bindings, `{{with.alias}}` for templates, `.nika.yaml` extension.

## Integration with NovaNet

Nika connects to NovaNet via MCP only (Zero Cypher rule). Use `invoke:` verb.

## TUI Views

`1/s` Studio | `2/c` Command | `3/x` Control

## Commands

```bash
# Workflows
nika run workflow.nika.yaml            # Execute workflow
nika run workflow.nika.yaml --resume   # Re-run, skip completed tasks
nika run workflow.nika.yaml --no-live  # Force classic append-only output
nika run https://example.com/wf.nika.yaml  # Execute remote workflow
nika test workflow.nika.yaml           # Test with mock provider
nika test wf.nika.yaml --golden snap.json  # Compare output to golden file
nika test wf.nika.yaml --golden snap.json --update-snapshot  # Update golden
nika eval wf.nika.yaml --dataset data.json  # Evaluate against assertions
nika eval wf.nika.yaml --dataset d.json --provider anthropic --format json
nika lint workflow.nika.yaml           # Best-practice linting (10 rules)
nika explain workflow.nika.yaml        # Human-readable summary
nika check workflow.nika.yaml          # Validate syntax + DAG
nika check workflow.nika.yaml --strict # + test MCP connections
nika new my-flow --verb infer          # Create new workflow
nika graph flow.nika.yaml             # Visualize DAG

# Direct verbs
nika infer "Explain AI"                # Quick LLM call
nika fetch https://blog.com --extract article  # HTTP + extraction
nika invoke nika:dimensions photo.jpg  # Builtin tool
nika agent "Research AI" --turns 5     # Multi-turn agent

# Interactive
nika ui                                # TUI
nika chat                              # Chat mode
nika studio                            # Studio editor

# Models & providers
nika model list                        # Cloud models + pricing
nika model info claude-sonnet-4-6      # Model details
nika model recommend                   # Smart recommendation
nika provider list                     # API key status
nika keys set anthropic            # Store key in encrypted vault
nika mcp list                          # MCP server connections

# Learning
nika init --course                     # 12-level course (44 exercises)
nika course status                     # Constellation progress map
nika course next                       # Open next exercise
nika showcase list                     # Browse 115 showcase workflows
nika showcase extract <name>           # Extract showcase to current dir

# Project
nika init                              # Interactive project setup
nika config list                       # Show configuration
nika pkg list                          # Package management
nika media stats                       # Media store stats

# System
nika version                           # Version, channel, build info
nika env                               # Environment debug view
nika doctor --fix                      # System health + auto-repair
nika daemon status                     # Background daemon (Unix)
nika cache stats                       # LLM response cache (Unix)
nika setup                             # API key setup wizard
nika bench                             # Provider benchmarking
nika switch dev|release                # Channel management
nika features                          # Compiled feature flags
nika completion zsh                    # Shell completions
nika trace list                        # Execution traces
nika help                              # Full command reference
nika help verbs                        # Deep-dive: 5 semantic verbs
```

## Structured Output — 5-Layer Defense (Killer Feature)

`structured:` enforces schema-validated JSON with automatic retry and repair.
The prompt MUST be natural language. NEVER mention JSON or the schema in the prompt.
The 5 layers handle extraction automatically:

```yaml
tasks:
  - id: extract
    # Prompt NATUREL — jamais mentionner JSON
    infer: "Parle-moi d'Alice, 30 ans, developpeuse Rust et Python"
    structured:
      schema:
        type: object
        properties:
          name: { type: string }
          age: { type: number, minimum: 0 }
          skills: { type: array, items: { type: string }, minItems: 1 }
        required: [name, age, skills]
      enable_repair: true
      max_retries: 3
```

Layers: L0 tool injection (provider-native) → L2 extract+validate → L3 retry with feedback → L4 LLM repair.
Result: valid JSON matching the schema. Same result on ALL 7 providers. No exceptions.

## Secrets Architecture

Resolution order: env vars → daemon IPC → NikaVault encrypted file → None.
NikaVault uses XChaCha20Poly1305 + Argon2i KDF. Key from machine fingerprint or `NIKA_VAULT_PASSPHRASE`.
- `nika keys set` writes to vault (no OS keychain, no popups).
- Daemon reads vault at `~/.nika/secrets/vault.enc`.
- Engine fallback reads vault directly when daemon is unavailable (`NIKA_NO_DAEMON=1`).

## Testing Philosophy

Tests must be INTELLIGENT, not superficial:
- Validate output programmatically (type, enum, range, constraints) — not just `!is_empty()`
- Same test on ALL providers — if one fails, it's an ENGINE bug
- Prompts in tests must be NATURAL — never mention JSON format
- Check EventLog for correct events (StructuredOutputSuccess, ProviderResponded)
- E2E: parse YAML → analyze → run → validate output → verify events
