# 11 -- Configuration Reference

## Configuration Files

Nika uses multiple configuration files at different scopes:

| File | Scope | Purpose |
|------|-------|---------|
| `~/.config/nika/config.toml` | Global | API keys, defaults |
| `.nika/config.toml` | Project | Project-specific settings |
| `.mcp.json` | Project | MCP server definitions |
| `~/.nika/mcp.yaml` | Global | Global MCP server definitions |
| `.nika/course-progress.json` | Project | Course progress tracking |

---

## Global Config (~/.config/nika/config.toml)

### Structure

```toml
[api_keys]
anthropic = "sk-ant-..."    # Anthropic Claude
openai = "sk-proj-..."      # OpenAI

[defaults]
provider = "claude"          # Default LLM provider
model = "claude-sonnet-4-6"  # Default model
```

### API Key Priority

1. Environment variables (highest priority)
2. System keychain (`native-keychain` feature)
3. Nika daemon (`nika-daemon` feature)
4. This config file (lowest priority)

### Config Commands

```bash
nika config list              # Show all config values
nika config get defaults.provider   # Get specific value
nika config set defaults.provider openai  # Set value
nika config edit              # Open in $EDITOR
nika config path              # Show config file path
```

---

## Project Config (.nika/config.toml)

### Bootstrap Configuration

```toml
[session]
provider = "claude"
model = "claude-sonnet-4-6"

[editor]
theme = "dark"
tab_size = 2
word_wrap = true

[tools]
permission = "plan"    # deny, plan, accept-edits, accept-all

[policy]
allowed_hosts = ["api.github.com", "*.openai.com"]
blocked_hosts = []
allowed_commands = ["curl", "jq", "python3"]
blocked_commands = []
token_budget = 100000

[trace]
max_traces = 50
retention_days = 30

[provider]
default = "claude"
```

### Permission Modes

| Mode | Description |
|------|-------------|
| `deny` | All file tool operations denied |
| `plan` | Ask before each operation |
| `accept-edits` | Auto-approve edits, ask for creates |
| `accept-all` | Auto-approve everything |

---

## Environment Variables

### LLM Provider Keys

| Variable | Provider | Key Prefix |
|----------|----------|------------|
| `ANTHROPIC_API_KEY` | Anthropic Claude | `sk-ant-` |
| `OPENAI_API_KEY` | OpenAI | `sk-` |
| `MISTRAL_API_KEY` | Mistral AI | -- |
| `GROQ_API_KEY` | Groq | `gsk_` |
| `DEEPSEEK_API_KEY` | DeepSeek | `sk-` |
| `GEMINI_API_KEY` | Google Gemini | -- |
| `XAI_API_KEY` | xAI (Grok) | -- |

### MCP Provider Keys

| Variable | Provider |
|----------|----------|
| `NEO4J_PASSWORD` | Neo4j |
| `GITHUB_TOKEN` | GitHub |
| `SLACK_BOT_TOKEN` | Slack |
| `PERPLEXITY_API_KEY` | Perplexity |
| `FIRECRAWL_API_KEY` | Firecrawl |
| `SUPADATA_API_KEY` | Supadata |

### System Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `NIKA_MODEL_PATH` | `~/.nika/models/` | Native inference model directory |
| `NIKA_HOME` | `~/.nika/` | Nika home directory |
| `NIKA_LOG` | -- | Log level override (trace, debug, info, warn, error) |

---

## Provider Setup

### Interactive Setup

```bash
nika keys set anthropic     # Prompts for API key, stores in keychain
nika keys set openai        # Same for OpenAI
nika provider test anthropic    # Verify connection works
nika provider list              # Show all providers and key status
nika provider migrate           # Move env vars to keychain
```

### Provider Test Output

```
Provider: Anthropic Claude
  Status: ✅ Connected
  Model: claude-sonnet-4-6
  API Key: sk-ant-...xxxx (masked)
  Source: environment variable
```

---

## MCP Server Configuration

### Workflow-Level (mcp: block)

```yaml
mcp:
  novanet:
    command: cargo
    args: ["run", "--", "mcp"]
    cwd: ../novanet
    env:
      NEO4J_URI: bolt://localhost:7687
```

### Project-Level (.mcp.json)

```json
{
  "servers": {
    "novanet": {
      "command": "cargo",
      "args": ["run", "--", "mcp"],
      "cwd": "../novanet",
      "env": {
        "NEO4J_URI": "bolt://localhost:7687"
      }
    },
    "github": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-github"],
      "env": {
        "GITHUB_TOKEN": "$GITHUB_TOKEN"
      }
    }
  }
}
```

### Global MCP Config (~/.nika/mcp.yaml)

Same format as project-level. Project-level overrides global for the same server name.

### MCP Aliases (100 Predefined)

Nika ships with 100 predefined MCP server aliases. These can be used in workflows without explicit configuration:

```yaml
mcp:
  github:     # Resolves to npx @modelcontextprotocol/server-github
    env:
      GITHUB_TOKEN: $GITHUB_TOKEN
```

**MCP Management Commands:**

```bash
nika mcp list -w workflow.yaml    # List servers in workflow
nika mcp test workflow.yaml srv   # Test server connection
nika mcp tools workflow.yaml srv  # List available tools
```

---

## TUI Configuration

### Theme

```toml
[editor]
theme = "dark"       # dark, light, cosmic
```

The TUI supports three themes: dark (default), light, and cosmic (SuperNovae branded).

### TUI Settings

```toml
[tui]
startup_view = "studio"    # studio, command, control
animation = true
show_line_numbers = true
word_wrap = true
tab_size = 2
```

### Chat Settings

```toml
[chat]
provider = "claude"
model = "claude-sonnet-4-6"
system_prompt = "You are a helpful assistant."
max_tokens = 4096
temperature = 0.7
streaming = true
```

---

## Trace Configuration

```toml
[trace]
max_traces = 50          # Maximum stored traces
retention_days = 30       # Auto-delete after N days
```

Traces are stored in `.nika/traces/` as NDJSON files. Each workflow execution produces a trace file named by generation ID.

### Trace Commands

```bash
nika trace list              # List execution traces
nika trace show <id>         # Show trace details
nika trace export <id>       # Export to JSON/YAML
```

---

## Model Configuration (Native Inference)

### Model Directory

Models are stored in `~/.nika/models/` (or `$NIKA_MODEL_PATH`).

### Model Management

```bash
nika model pull mistral-7b              # Download GGUF model
nika model list                          # List installed models
nika model info mistral-7b              # Show model details
nika model vision Qwen/Qwen2.5-VL-7B-Instruct --isq Q4K  # Vision model
```

### Model Selection in Workflows

```yaml
provider: native
model: mistral-7b-instruct-v0.3

# Or for vision:
provider: native
model: Qwen/Qwen2.5-VL-7B-Instruct
```

---

## Package Registry

### Package Installation

```bash
nika pkg install @nika/skills@1.0       # Install package
nika pkg list                            # List installed packages
nika pkg remove @nika/skills             # Remove package
```

Packages are stored in `~/.nika/packages/`. They can contain workflows, skills, schemas, and other resources.

### Package URIs

Format: `pkg:@scope/name@version/path`

```yaml
imports:
  - path: pkg:@nika/core@1.0/seo.nika.yaml

skills:
  analysis: pkg:@nika/skills@1.0/analysis.md
```

---

## Schema Configuration

```bash
nika schema list               # List known schema versions
nika schema validate file.yaml # Validate against schema
```

The current and only supported schema version is `nika/workflow@0.12`.

