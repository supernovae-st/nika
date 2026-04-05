# 12 -- CLI Commands Reference

## Global Flags

Every command accepts these global flags:

| Flag | Short | Description |
|------|-------|-------------|
| `--verbose` | `-v` | Increase verbosity (-v info, -vv debug, -vvv trace) |
| `--quiet` | `-q` | Suppress non-error output |
| `--color <mode>` | -- | Color output: auto (default), always, never |
| `--detail <level>` | -- | Detail level: max (default), default, min, json |

---

## Direct Workflow Execution

```bash
nika <file.nika.yaml>
```

Run a workflow directly. Equivalent to `nika run <file>`. Streaming output is displayed in the terminal.

---

## nika run

Run a workflow file headlessly (no TUI).

```bash
nika run <file.nika.yaml> [options]
```

| Option | Short | Description |
|--------|-------|-------------|
| `--provider <name>` | `-p` | Override default provider |
| `--model <name>` | `-m` | Override default model |

**Aliases:** `r`

**Examples:**

```bash
nika run workflow.nika.yaml
nika run workflow.nika.yaml --provider openai
nika run workflow.nika.yaml -p claude -m claude-sonnet-4-6
```

---

## nika check

Validate workflow syntax, DAG structure, and bindings.

```bash
nika check <file.nika.yaml> [options]
```

| Option | Description |
|--------|-------------|
| `--strict` | Connect to MCP servers and validate invoke params |

**Aliases:** `validate`, `v`

**What it checks:**
1. YAML syntax (Phase 1 parse)
2. Schema version
3. Task ID uniqueness
4. Dependency resolution
5. Cycle detection
6. With: binding validation
7. Template reference validation
8. (strict) MCP server connectivity and tool schema validation

---

## nika ui

Launch the interactive Terminal UI.

```bash
nika ui [options] [workflow]
```

| Option | Description |
|--------|-------------|
| `[workflow]` | Optional workflow file to load |

**Views (3 total):**

| Key | View | Description |
|-----|------|-------------|
| `1/s` | Studio | File browser + YAML editor + DAG preview |
| `2/c` | Command | Real-time execution monitoring + chat |
| `3/x` | Control | Provider config, theme, preferences |

**Navigation:**
- `1-3` to switch views (normal mode)
- `?` for help overlay
- `Ctrl+C` (2x) to quit

---

## nika init

Initialize a new Nika project in the current directory.

```bash
nika init [options]
```

| Option | Short | Description |
|--------|-------|-------------|
| `--permission <mode>` | `-p` | Permission mode: deny, plan, accept-edits, accept-all |
| `--no-example` | -- | Skip creating example workflow |
| `--migrate-keys` | -- | Migrate API keys from env to keychain |
| `--course` | -- | Generate interactive course (12 levels, 44 exercises) |

**What it creates:**
- `.nika/` directory with `config.toml`
- `workflows/minimal/` with 5 starter workflows (1 per verb)
- `workflows/context/` with reusable context files
- `workflows/schemas/` with JSON schemas
- (with `--course`) `workflows/course/` with 44 exercises across 12 levels

---

## nika course

Interactive learning course management.

```bash
nika course <action>
```

**Aliases:** `learn`

### Subcommands

| Command | Description |
|---------|-------------|
| `nika course status` | Show constellation progress map |
| `nika course next` | Open next exercise |
| `nika course check [level]` | Validate exercises |
| `nika course hint [exercise]` | Progressive hints (3 tiers) |
| `nika course run <exercise>` | Run a course exercise |
| `nika course info [level]` | Show course/level details |
| `nika course reset <level>` | Reset a level's progress |
| `nika course watch` | Auto-check on file save |

### Course Levels (12)

| # | Name | Exercises | Theme |
|---|------|-----------|-------|
| 01 | Jailbreak | 5 | Break free -- exec: basics |
| 02 | Hot Wire | 4 | Network -- fetch: HTTP |
| 03 | Fork Bomb | 4 | Multiply -- DAG patterns |
| 04 | Root Access | 3 | Unlock LLM -- infer: |
| 05 | Shapeshifter | 3 | Transform -- with: bindings |
| 06 | Pay-Per-Dream | 3 | Structured output |
| 07 | Swiss Knife | 3 | Builtin tools -- invoke: |
| 08 | Gone Rogue | 3 | Autonomous -- agent: |
| 09 | Data Heist | 4 | Extraction -- fetch: extract |
| 10 | Open Protocol | 3 | MCP integration |
| 11 | Pixel Pirate | 4 | Media pipeline |
| 12 | SuperNovae | 5 | Boss -- full orchestration |

Total: 44 exercises following a "Liberation" narrative arc.

---

## nika provider

Manage LLM provider API keys.

```bash
nika provider <action>
```

### Subcommands

| Command | Description |
|---------|-------------|
| `nika provider list` | Show providers and API key status |
| `nika keys set <provider>` | Store key in system keychain |
| `nika provider test <provider>` | Test provider connection |
| `nika provider migrate` | Move env vars to keychain |

---

## nika mcp

Manage MCP server connections.

```bash
nika mcp <action>
```

### Subcommands

| Command | Description |
|---------|-------------|
| `nika mcp list -w <workflow>` | List servers in workflow |
| `nika mcp test <workflow> <server>` | Test server connection |
| `nika mcp tools <workflow> <server>` | List available tools |

---

## nika model

Manage local LLM models (requires `native-inference` feature).

```bash
nika model <action>
```

**Aliases:** `m`

### Subcommands

| Command | Description |
|---------|-------------|
| `nika model pull <model>` | Download GGUF model |
| `nika model list` | List installed models |
| `nika model info <model>` | Show model details |
| `nika model remove <model>` | Delete model |
| `nika model vision <model> --isq <quant>` | Download vision model |

---

## nika pkg

Manage installed packages.

```bash
nika pkg <action>
```

**Aliases:** `p`

### Subcommands

| Command | Description |
|---------|-------------|
| `nika pkg install <spec>` | Install package from registry |
| `nika pkg list` | List installed packages |
| `nika pkg remove <name>` | Remove package |
| `nika pkg info <name>` | Show package details |

---

## nika media

Manage the media store (CAS).

```bash
nika media <action>
```

### Subcommands

| Command | Description |
|---------|-------------|
| `nika media list` | List CAS entries |
| `nika media stats` | Storage statistics |
| `nika media clean` | Garbage collect unused blobs |
| `nika media clean --force` | Override lockfile protection |

---

## nika trace

Manage execution traces.

```bash
nika trace <action>
```

### Subcommands

| Command | Description |
|---------|-------------|
| `nika trace list` | List execution traces |
| `nika trace show <id>` | Show trace details |
| `nika trace export <id>` | Export to JSON/YAML |

---

## nika config

Manage Nika configuration.

```bash
nika config <action>
```

### Subcommands

| Command | Description |
|---------|-------------|
| `nika config list` | Show all config values |
| `nika config get <key>` | Get specific value |
| `nika config set <key> <value>` | Set value |
| `nika config edit` | Open in $EDITOR |
| `nika config path` | Show config file path |

---

## nika schema

Manage schema versions and migrations.

```bash
nika schema <action>
```

### Subcommands

| Command | Description |
|---------|-------------|
| `nika schema list` | List known schema versions |
| `nika schema validate <file>` | Validate against schema |

---

## nika showcase

Browse and extract showcase workflows.

```bash
nika showcase <action>
```

### Subcommands

| Command | Description |
|---------|-------------|
| `nika showcase list` | Browse 115 showcase workflows |
| `nika showcase extract <name>` | Extract a showcase to current dir |

The showcase contains 115 workflows organized in multiple categories.

---

## nika doctor

Check system health and diagnose issues.

```bash
nika doctor [options]
```

**Aliases:** `d`

| Option | Description |
|--------|-------------|
| `--full` | Run all checks including slow ones (MCP connectivity) |
| `--format <mode>` | Output format: text (default), json |

Checks performed:
- Nika version and configuration
- Provider API key availability
- MCP server configuration
- File system permissions
- (full) MCP server connectivity

---

## nika features

Show compiled feature flags and capabilities.

```bash
nika features
```

Displays all feature flags (tui, native-inference, media-*, fetch-*, lsp) with their compile-time status.

---

## nika lsp

Start Language Server Protocol server (requires `lsp` feature).

```bash
nika lsp [options]
```

| Option | Default | Description |
|--------|---------|-------------|
| `--mode <mode>` | `stdio` | Communication: stdio or tcp |
| `--port <port>` | `9257` | TCP port (only with --mode tcp) |

Provides IDE integration:
- Diagnostics (syntax errors, validation errors)
- Completions (verbs, fields, task references)
- Hover documentation
- Go to definition
- Code actions (quick fixes)

---

## nika completion

Generate shell completions.

```bash
nika completion <shell>
```

Supported shells: `bash`, `zsh`, `fish`, `powershell`, `elvish`.

```bash
# Install completions
nika completion bash > ~/.local/share/bash-completion/completions/nika
nika completion zsh > ~/.zfunc/_nika
nika completion fish > ~/.config/fish/completions/nika.fish
```

---

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error |
| 2 | Validation error (check command) |
| 3 | Execution error (run command) |
| 4 | Configuration error |
