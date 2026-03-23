# Research Report: MCP as Universal Bridge Between AI Coding Tools and Nika

**Date**: 2026-03-23
**Author**: Claude Opus 4.6 + Thibaut
**Confidence**: High (primary sources: MCP spec, rmcp 0.16 source, NovaNet MCP server reference implementation, tool config files on disk)

---

## Summary

MCP (Model Context Protocol) is now the universal integration standard for AI coding tools. Every major AI coding tool -- Claude Code, Cursor, Windsurf, Copilot, Roo Code, Continue, Zed, Amazon Q, and others -- supports MCP servers via near-identical JSON config. Nika currently operates as an MCP **client** (consuming external MCP servers via `invoke:` verb). The highest-leverage integration for Nika would be to also become an MCP **server**, exposing its workflow schema, validation, examples, and execution capabilities to all AI coding tools simultaneously. This is a 1-server-many-clients architecture that requires minimal effort thanks to `rmcp 0.16` already being in the workspace.

---

## 1. AI Tools That Support MCP (and How They Configure It)

### Full Support Matrix

| Tool | Config Location | Format | Transport | Status |
|------|----------------|--------|-----------|--------|
| **Claude Code** | `~/.claude/settings.json` (global) or `.claude/settings.json` (project) | `mcpServers: {}` | stdio, SSE | Full support (tools, resources, prompts) |
| **Cursor** | `.cursor/mcp.json` (project) or `~/.cursor/mcp.json` (global) | `mcpServers: {}` | stdio, SSE | Full support since v0.43+ |
| **Windsurf (Codeium)** | `~/.codeium/windsurf/mcp_config.json` | `mcpServers: {}` | stdio, SSE | Full support (formerly Cascade) |
| **GitHub Copilot** | `.github/copilot/mcp.json` or VS Code `settings.json` | `mcpServers: {}` | stdio, SSE | Full support in Copilot Chat agent mode |
| **Roo Code** | `.roo/mcp.json` (project) | `mcpServers: {}` | stdio, SSE | Full support |
| **Continue** | `~/.continue/config.json` | `mcpServers: []` | stdio, SSE | Full support |
| **Zed** | `~/.config/zed/settings.json` | `context_servers: {}` | stdio | Full support (different key name) |
| **Amazon Q Developer** | `~/.aws/amazonq/mcp.json` | `mcpServers: {}` | stdio, SSE | Full support |
| **Cline** | `.cline/mcp_settings.json` or VS Code settings | `mcpServers: {}` | stdio, SSE | Full support |
| **JetBrains AI** | IDE settings UI | `mcpServers: {}` | stdio, SSE | Support in 2025.1+ |
| **Sourcegraph Cody** | `~/.sourcegraph/cody.json` | `mcpServers: {}` | stdio, SSE | Partial support |

### Config Format (Near-Universal)

All tools except Zed use the same config shape:

```json
{
  "mcpServers": {
    "server-name": {
      "command": "path/to/binary",
      "args": ["--flag", "value"],
      "env": {
        "API_KEY": "value"
      }
    }
  }
}
```

**Zed** uses `context_servers` as the key name but the inner shape is the same.

### Config File Locations (Exact Paths)

```
Claude Code:
  Global:  ~/.claude/settings.json
  Project: .claude/settings.json

Cursor:
  Global:  ~/.cursor/mcp.json
  Project: .cursor/mcp.json

Windsurf:
  Global:  ~/.codeium/windsurf/mcp_config.json

Copilot:
  Project: .github/copilot/mcp.json
  VS Code: .vscode/settings.json (under github.copilot.chat.mcp)

Roo Code:
  Project: .roo/mcp.json

Continue:
  Global:  ~/.continue/config.json

Zed:
  Global:  ~/.config/zed/settings.json

Amazon Q:
  Global:  ~/.aws/amazonq/mcp.json

Cline:
  Project: .cline/mcp_settings.json
```

### Real Example from This Machine

Found in `~/.claude/settings.json`:
```json
{
  "mcpServers": {
    "novanet": {
      "command": "cargo",
      "args": ["run", "--manifest-path", "/path/to/Cargo.toml", "--release"],
      "env": { "NOVANET_MCP_NEO4J_PASSWORD": "..." }
    }
  }
}
```

Found in `.cursor/mcp.json`:
```json
{
  "mcpServers": {
    "firecrawl": {
      "command": "npx",
      "args": ["-y", "firecrawl-mcp"],
      "env": { "FIRECRAWL_API_KEY": "${FIRECRAWL_API_KEY}" }
    }
  }
}
```

---

## 2. Nika's Current MCP Status

### What Exists: MCP Client (Consumer)

Nika is currently an MCP **client** via the `nika-mcp` crate:

- **`invoke:` verb**: Calls tools on external MCP servers
- **`agent:` verb**: Uses MCP tools within agent loops
- **`nika mcp add/remove/list/test/tools/aliases`**: CLI management of MCP servers
- **114 MCP aliases**: Short names for common npm MCP packages
- **rmcp 0.16**: Already in workspace with `client` and `transport-child-process` features
- **3-level config merge**: workflow `mcp:` section + `.nika/mcp.yaml` (project) + `~/.nika/mcp.yaml` (global)

### What Does NOT Exist: MCP Server (Provider)

There is **no** `nika mcp serve` command. Nika cannot currently expose its capabilities to AI coding tools.

### Key Architecture Insight

The `rmcp 0.16` crate supports both client AND server modes. Adding server support requires:
1. Adding `server` + `transport-io` features to rmcp dependency
2. Implementing `ServerHandler` trait (exact pattern exists in NovaNet MCP server)
3. Adding a `nika mcp serve` subcommand that launches stdio transport

The NovaNet MCP server (`/Users/thibaut/dev/supernovae/novanet/tools/novanet-mcp/`) is a **perfect reference implementation** in the same monorepo, using identical rmcp 0.16 patterns.

---

## 3. MCP as the Ultimate Integration: Nika as MCP Server

### The Vision

Instead of relying on static rules files (CLAUDE.md, .cursorrules, etc.) that AI tools passively read, Nika could expose **active intelligence** via MCP:

```
                           MCP Protocol (stdio)
Claude Code  ─────────────────┐
Cursor       ─────────────────┤
Windsurf     ─────────────────┼──── nika mcp serve
Copilot      ─────────────────┤    (one server, all tools)
Roo Code     ─────────────────┘
```

### Proposed Tool Surface (12 tools)

Based on analysis of Nika's existing capabilities:

#### Schema & Validation (3 tools)

| Tool | Description | Annotations |
|------|-------------|-------------|
| `nika_schema` | Get Nika workflow schema reference (verbs, fields, types). Use FIRST to understand .nika.yaml format. | read_only, idempotent |
| `nika_validate` | Validate a .nika.yaml workflow file. Returns errors with NIKA-XXX codes and fix suggestions. | read_only, idempotent |
| `nika_examples` | Get example workflows by verb (infer, exec, fetch, invoke, agent) or pattern (dag, media, vision). | read_only, idempotent |

#### Execution (3 tools)

| Tool | Description | Annotations |
|------|-------------|-------------|
| `nika_run` | Execute a .nika.yaml workflow file. Returns task results. | destructive: false |
| `nika_check` | Check a workflow without executing (dry-run validation + DAG analysis). | read_only, idempotent |
| `nika_provider_list` | List available LLM providers and their status (API keys configured). | read_only, idempotent |

#### Project Setup (3 tools)

| Tool | Description | Annotations |
|------|-------------|-------------|
| `nika_init_workflow` | Scaffold a new .nika.yaml workflow from description. Generates valid YAML. | destructive: false |
| `nika_mcp_list` | List MCP servers available to Nika workflows (global + project config). | read_only, idempotent |
| `nika_mcp_aliases` | List all 114 MCP server aliases (short names like 'neo4j' -> npm package). | read_only, idempotent |

#### Media Pipeline (3 tools)

| Tool | Description | Annotations |
|------|-------------|-------------|
| `nika_media_import` | Import a file into Nika's content-addressable storage. Returns CAS hash. | destructive: false |
| `nika_media_info` | Get metadata for a CAS-stored file (dimensions, format, EXIF). | read_only, idempotent |
| `nika_builtin_tools` | List all 38 builtin nika:* tools (12 core + 26 media) with descriptions. | read_only, idempotent |

### Tool Description Best Practices (Learned from NovaNet)

The NovaNet MCP server's tool descriptions follow a proven pattern:

```
"EMOJI VERB -- One-line summary. WHEN: Use this tool when [situation].
INPUTS: param1 (type), param2 (type|type), optional param3.
RETURNS: JSON with [key fields].
NOT FOR: [anti-pattern] (use [other_tool] instead)."
```

This pattern works because:
1. **WHEN** tells the LLM when to pick this tool over others
2. **INPUTS** gives a quick parameter reference without needing schema lookup
3. **RETURNS** sets expectations for output parsing
4. **NOT FOR** prevents misuse and suggests alternatives

---

## 4. MCP Tool Design for AI: Best Practices

### What Makes Good MCP Tools

Based on analysis of successful MCP servers (NovaNet, GitHub, Prisma, Supabase):

#### 1. Short, Actionable Names
- Good: `nika_validate`, `nika_run`, `nika_schema`
- Bad: `nika_workflow_yaml_validation_checker`, `get_nika_schema_reference_docs`

#### 2. Rich Descriptions with Usage Guidance
- Include WHEN/WHY not just WHAT
- Include parameter summary in description (LLMs read descriptions before schemas)
- Include "NOT FOR" section to prevent misuse
- Keep under 500 chars (some clients truncate)

#### 3. Strong JSON Schema for Parameters
```rust
#[derive(Deserialize, JsonSchema)]
pub struct ValidateParams {
    /// Path to .nika.yaml file to validate
    pub path: String,
    /// Schema version to validate against (default: latest @0.12)
    #[serde(default)]
    pub schema_version: Option<String>,
    /// Return fix suggestions for errors
    #[serde(default = "default_true")]
    pub suggest_fixes: bool,
}
```

#### 4. Tool Annotations (rmcp 0.16 / MCP 2025-03-26 spec)
```rust
#[tool(
    name = "nika_validate",
    description = "...",
    annotations(
        read_only_hint = true,
        destructive_hint = false,
        idempotent_hint = true,
        open_world_hint = false
    )
)]
```

- `read_only_hint`: Can tool modify state? (safe to call freely if true)
- `destructive_hint`: Can tool delete/overwrite? (requires confirmation if true)
- `idempotent_hint`: Same input = same output? (safe to retry if true)
- `open_world_hint`: Can tool interact with external world? (network, files)

#### 5. Structured Output
- Always return JSON
- Include `next_action` suggestions (like NovaNet does)
- Include error codes, not just messages
- Include `hints` for common mistakes

---

## 5. MCP Resources vs Tools: When to Use Which

### Resources = Static/Semi-Static Data

MCP Resources are for data that AI tools can **passively access** without invoking a tool call:

```
resources/read  -> Returns content
resources/list  -> Enumerates available resources
resources/subscribe -> Watch for changes
```

### What Nika Should Expose as Resources

| Resource URI | Content | Update Frequency |
|-------------|---------|-----------------|
| `nika://schema` | Complete schema@0.12 reference (all verbs, fields, types) | Per-version |
| `nika://schema/verbs/{verb}` | Specific verb reference (infer, exec, fetch, invoke, agent) | Per-version |
| `nika://providers` | Available LLM providers + configured status | Session |
| `nika://errors` | Error code reference (NIKA-000 through NIKA-319) | Per-version |
| `nika://transforms` | Available pipe transforms for templates | Per-version |
| `nika://mcp-aliases` | All 114 MCP server aliases | Per-version |
| `nika://project/config` | Project-level .nika/ configuration | File changes |

### Resources vs Tools Decision Matrix

| Need | Use Resource | Use Tool |
|------|-------------|----------|
| Schema reference docs | Yes | No |
| Validate a workflow | No | Yes |
| Error code lookup | Yes | No |
| Execute a workflow | No | Yes |
| List providers | Either (resource for status, tool for testing) | Either |
| Generate a workflow | No | Yes |

### Implementation Pattern (from NovaNet)

```rust
// Resource templates
RawResourceTemplate {
    uri_template: "nika://schema/verbs/{verb}".into(),
    name: "Verb Reference".into(),
    description: Some("Schema reference for a specific Nika verb".into()),
    mime_type: Some("text/markdown".into()),
}
```

---

## 6. MCP Prompts: Reusable Templates for AI Tools

### How MCP Prompts Work

MCP servers can provide **prompt templates** -- pre-built conversation starters that AI tools can offer to users:

```
prompts/list -> Returns available prompt templates
prompts/get  -> Returns rendered prompt with arguments filled
```

### What Nika Should Expose as Prompts

| Prompt Name | Description | Arguments |
|-------------|-------------|-----------|
| `workflow_from_description` | Generate a .nika.yaml workflow from a natural language description | `description` (required), `verbs` (optional filter) |
| `debug_workflow` | Analyze a failing workflow and suggest fixes | `path` (required), `error` (optional NIKA-XXX code) |
| `convert_script_to_workflow` | Convert a shell script or Python script to a Nika workflow | `script` (required), `language` (optional) |
| `media_pipeline` | Design a media processing pipeline using nika:* tools | `input_format` (required), `operations` (required) |
| `agent_loop_design` | Design an agent: task with MCP tool access | `goal` (required), `mcp_servers` (optional) |

### How AI Tools Use Prompts

- **Claude Code**: Prompts appear in the slash command menu (e.g., `/nika:workflow_from_description`)
- **Cursor**: Prompts can be invoked via the prompt palette
- **Others**: Most tools surface prompts as conversation starters or templates

### Implementation Pattern

```rust
fn list_prompts(&self, ...) -> ListPromptsResult {
    vec![Prompt {
        name: "workflow_from_description".into(),
        description: Some("Generate a .nika.yaml workflow from a description".into()),
        arguments: Some(vec![
            PromptArgument {
                name: "description".into(),
                description: Some("What the workflow should do".into()),
                required: Some(true),
            },
        ]),
    }]
}
```

---

## 7. Real MCP Servers: Patterns from the Wild

### GitHub MCP Server (@modelcontextprotocol/server-github)

**Tools exposed (22)**:
- `create_or_update_file`, `search_repositories`, `create_repository`
- `get_file_contents`, `push_files`, `create_issue`, `create_pull_request`
- `fork_repository`, `create_branch`, `list_commits`, `list_issues`

**Pattern**: CRUD operations on GitHub resources. Each tool maps 1:1 to a GitHub API endpoint.

### Prisma MCP Server

**Tools exposed (4)**:
- `prisma_schema_validate` -- Validate a Prisma schema file
- `prisma_schema_format` -- Format a Prisma schema file
- `prisma_schema_introspect` -- Introspect database and update schema
- `prisma_migrate` -- Run database migrations

**Pattern**: Developer workflow tools. Schema-first, validation-heavy. Very similar to what Nika should do.

### Supabase MCP Server

**Tools exposed (8+)**:
- `execute_sql`, `list_tables`, `list_extensions`
- `create_migration`, `apply_migration`
- `get_logs`, `get_project_url`

**Pattern**: Database + project management. Combines read-only introspection with write operations.

### Key Patterns That Work

1. **Start-here tool**: One tool that bootstraps understanding (NovaNet's `novanet_describe`, Nika's `nika_schema`)
2. **Validate-before-write**: Dry-run validation before executing (NovaNet's `dry_run=true`, Prisma's `validate`)
3. **Error guidance**: Return structured errors with fix suggestions, not just error messages
4. **Next action hints**: Tell the LLM what to do next after each tool call
5. **Batch operations**: Allow multiple operations in one call to reduce round-trips

---

## 8. Config Formats: Side-by-Side Comparison

### Claude Code (`.claude/settings.json`)

```json
{
  "mcpServers": {
    "nika": {
      "command": "nika",
      "args": ["mcp", "serve"],
      "env": {}
    }
  }
}
```

### Cursor (`.cursor/mcp.json`)

```json
{
  "mcpServers": {
    "nika": {
      "command": "nika",
      "args": ["mcp", "serve"],
      "env": {}
    }
  }
}
```

### Windsurf (`~/.codeium/windsurf/mcp_config.json`)

```json
{
  "mcpServers": {
    "nika": {
      "command": "nika",
      "args": ["mcp", "serve"],
      "env": {}
    }
  }
}
```

### Copilot (`.github/copilot/mcp.json`)

```json
{
  "mcpServers": {
    "nika": {
      "command": "nika",
      "args": ["mcp", "serve"],
      "env": {}
    }
  }
}
```

### Roo Code (`.roo/mcp.json`)

```json
{
  "mcpServers": {
    "nika": {
      "command": "nika",
      "args": ["mcp", "serve"],
      "env": {}
    }
  }
}
```

### Zed (`~/.config/zed/settings.json`)

```json
{
  "context_servers": {
    "nika": {
      "command": {
        "path": "nika",
        "args": ["mcp", "serve"]
      }
    }
  }
}
```

### Continue (`~/.continue/config.json`)

```json
{
  "mcpServers": [
    {
      "name": "nika",
      "command": "nika",
      "args": ["mcp", "serve"]
    }
  ]
}
```

**Key observation**: The config is 95% identical across all tools. A single `nika setup mcp` command could generate all of them.

---

## 9. Auto-Discovery: Can AI Tools Find MCP Servers?

### Current State: No Standard Auto-Discovery

There is no standard mechanism for AI tools to automatically discover project-level MCP servers. Each tool reads its own config file.

### Emerging Patterns

1. **`.well-known/mcp.json`**: Some projects propose a well-known file at the project root
2. **`package.json` / `Cargo.toml` metadata**: Embedding MCP server info in existing project metadata
3. **`.mcp/` directory convention**: A project-level directory containing server configs (emerging)

### What Nika Could Do

Since there is no standard, Nika can create its own convention:

```yaml
# .nika/mcp.yaml (already exists for client configs)
# Could be extended with a server section:
serve:
  enabled: true
  tools: [schema, validate, examples, run, check, provider_list]
```

And `nika setup mcp` could detect installed AI tools and write configs automatically.

---

## 10. `nika setup mcp`: Auto-Configure All AI Tools

### Detection Logic

```rust
fn detect_ai_tools() -> Vec<AiTool> {
    let mut tools = vec![];

    // Claude Code
    if home_dir().join(".claude").exists() {
        tools.push(AiTool::ClaudeCode {
            global: home_dir().join(".claude/settings.json"),
            project: project_root().join(".claude/settings.json"),
        });
    }

    // Cursor
    if home_dir().join(".cursor").exists() || project_root().join(".cursor").exists() {
        tools.push(AiTool::Cursor {
            global: home_dir().join(".cursor/mcp.json"),
            project: project_root().join(".cursor/mcp.json"),
        });
    }

    // Windsurf
    let windsurf = home_dir().join(".codeium/windsurf/mcp_config.json");
    if windsurf.parent().map_or(false, |p| p.exists()) {
        tools.push(AiTool::Windsurf { config: windsurf });
    }

    // Copilot
    if project_root().join(".github").exists() {
        tools.push(AiTool::Copilot {
            config: project_root().join(".github/copilot/mcp.json"),
        });
    }

    // Roo Code
    if project_root().join(".roo").exists() {
        tools.push(AiTool::RooCode {
            config: project_root().join(".roo/mcp.json"),
        });
    }

    // Zed
    let zed = home_dir().join(".config/zed/settings.json");
    if zed.exists() {
        tools.push(AiTool::Zed { config: zed });
    }

    // Amazon Q
    let q = home_dir().join(".aws/amazonq/mcp.json");
    if q.parent().map_or(false, |p| p.exists()) {
        tools.push(AiTool::AmazonQ { config: q });
    }

    tools
}
```

### UX Flow

```
$ nika setup mcp

Nika MCP Server Setup
----------------------

Detected AI tools:
  [x] Claude Code  (~/.claude/settings.json)
  [x] Cursor       (.cursor/mcp.json)
  [ ] Windsurf     (not installed)
  [ ] Copilot      (not configured)
  [x] Roo Code     (.roo/mcp.json)

Configure Nika MCP for selected tools? [Y/n]

  + Added nika to Claude Code   (~/.claude/settings.json)
  + Added nika to Cursor        (.cursor/mcp.json)
  + Added nika to Roo Code      (.roo/mcp.json)

Nika MCP server configured for 3 tools.
Run 'nika mcp serve' to start, or AI tools will auto-launch it.
```

### Config Snippet Injected

```json
{
  "nika": {
    "command": "nika",
    "args": ["mcp", "serve"],
    "env": {}
  }
}
```

For development builds (not in PATH):
```json
{
  "nika": {
    "command": "cargo",
    "args": ["run", "--manifest-path", "/abs/path/to/tools/nika/Cargo.toml", "--", "mcp", "serve"],
    "env": {}
  }
}
```

---

## Implementation Plan: `nika mcp serve`

### Phase 1: Core Server (1 session)

**Add to `nika-mcp/Cargo.toml`:**
```toml
rmcp = { workspace = true, features = ["server", "transport-io", "macros"] }
```

**New file: `tools/nika-mcp/src/server/mod.rs`**

Implement `NikaServerHandler` with:
- `get_info()` -- Server metadata + instructions
- 6 initial tools: `nika_schema`, `nika_validate`, `nika_check`, `nika_examples`, `nika_provider_list`, `nika_builtin_tools`
- 3 resources: `nika://schema`, `nika://errors`, `nika://transforms`
- 2 prompts: `workflow_from_description`, `debug_workflow`

**New CLI subcommand:**
```rust
McpAction::Serve => {
    let handler = NikaServerHandler::new();
    handler.serve(rmcp::transport::stdio()).await?;
}
```

### Phase 2: Execution Tools (1 session)

Add `nika_run`, `nika_init_workflow`, `nika_mcp_list`, `nika_mcp_aliases`.

### Phase 3: `nika setup mcp` (1 session)

Auto-detect and configure all AI tools.

### Phase 4: Media Tools (1 session)

Add `nika_media_import`, `nika_media_info`, `nika_media_pipeline`.

### Estimated Effort

- Phase 1: ~3h (rmcp server pattern is well-understood from NovaNet)
- Phase 2: ~2h (execution tools are thin wrappers around existing functions)
- Phase 3: ~2h (config detection + JSON merging)
- Phase 4: ~2h (media tools already exist, just need MCP wrappers)

**Total: ~9h / 2-3 sessions**

---

## Key References

### Source Files Analyzed

| File | Purpose |
|------|---------|
| `tools/nika-mcp/src/lib.rs` | Current MCP client module |
| `tools/nika-mcp/src/rmcp_adapter.rs` | rmcp 0.16 client adapter |
| `tools/nika-cli/src/mcp.rs` | CLI subcommands (add/remove/list/test/tools/aliases) |
| `tools/nika-core/src/catalogs/mcp_aliases.rs` | 114 MCP server aliases |
| `novanet/tools/novanet-mcp/src/server/handler.rs` | Reference: rmcp 0.16 server implementation |
| `novanet/tools/novanet-mcp/src/server/mod.rs` | Reference: stdio transport setup |
| `novanet/tools/novanet-mcp/src/prompts/mod.rs` | Reference: MCP prompts implementation |
| `~/.claude/settings.json` | Claude Code MCP config (live) |
| `.cursor/mcp.json` | Cursor MCP config (live) |
| `rmcp-0.16.0/tests/common/calculator.rs` | rmcp server example |
| `rmcp-0.16.0/src/handler/server/router.rs` | rmcp Router (tools + prompts) |

### External References

- MCP Specification: https://spec.modelcontextprotocol.io
- rmcp (Rust SDK): https://github.com/modelcontextprotocol/rust-sdk
- MCP Server Registry: https://github.com/modelcontextprotocol/servers

---

## Conclusion: Why This Is the Most Powerful Integration

1. **One server, all clients**: Write the MCP server once, every AI tool connects. No per-tool integration work.

2. **Active > Passive**: Static rules files (CLAUDE.md) are read once. MCP tools are called on demand -- they can validate, execute, and guide in real time.

3. **Schema as a living API**: Instead of AI tools guessing at .nika.yaml syntax, they call `nika_schema` and get the exact specification. When the schema changes, the tool returns the new spec automatically.

4. **Error guidance loop**: `nika_validate` returns NIKA-XXX error codes with fix suggestions. The AI tool can iteratively fix and re-validate until the workflow is correct.

5. **Zero-config dream**: `nika setup mcp` detects all installed AI tools and configures them in one command. From that point, every AI tool knows how to work with Nika.

6. **Already have the blueprint**: NovaNet's MCP server is a proven pattern using the exact same rmcp version. Copy the architecture, adapt the tools.

7. **Competitive moat**: No other workflow engine exposes itself as an MCP server. First-mover advantage in the AI-native tooling space.
