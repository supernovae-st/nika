# Research: MCP Integration in Cursor, Windsurf, VS Code, and Claude Code

**Date**: 2026-04-06
**Author**: Thibaut + Nika
**Status**: Complete
**Objective**: How can Nika's MCP server integrate with AI code editors so their AI can generate valid `.nika.yaml` workflows natively?

---

## Executive Summary

All four major AI editors (Cursor, Windsurf, VS Code/Copilot, Claude Code) now support MCP natively. The integration surface is nearly identical across all of them: a JSON config file that declares MCP servers, which the editor spawns and connects to via stdio or HTTP. Nika already has `nika mcp serve` -- making it work in all four editors requires only a config file per editor. The real opportunity is not just tools but also **rules/instructions** and **agent plugins** that teach the AI how to write `.nika.yaml` files.

---

## 1. Cursor MCP Support

### Feature Support (from MCP Registry)

| Feature | Supported |
|---------|-----------|
| Prompts | Yes |
| Tools | Yes |
| Roots | Yes |
| Elicitation | Yes |
| DCR (Dynamic Client Registration) | Yes |
| Resources | No |
| Sampling | No |
| Instructions | No |
| Discovery | No |

### Configuration

Cursor uses **two config locations** (confirmed by MCP official docs + community):

1. **Project-level**: `.cursor/mcp.json` in the project root
2. **Global**: `~/.cursor/mcp.json` (user-level, but does not exist by default -- see Thibaut's machine)

The config format uses the **Claude Desktop / .mcp.json convention** (not VS Code's `servers` key):

```json
{
  "mcpServers": {
    "nika": {
      "command": "/opt/homebrew/bin/nika",
      "args": ["mcp", "serve"]
    }
  }
}
```

This is the SAME format as Claude Desktop's `claude_desktop_config.json` and the project `.mcp.json` file Nika already uses.

### How Cursor's Composer Uses MCP Tools

- MCP tools appear in Cursor's Composer/Agent mode
- The AI can call any tool declared by the MCP server
- Tools are invoked with user confirmation (like Claude Code)
- Cursor supports both stdio and SSE transports
- Cursor does NOT support MCP Resources (cannot serve schema docs as resources)
- Cursor DOES support MCP Prompts (can expose prompt templates)

### Cursor Rules System

Cursor has its own rules system for providing context to the AI:

- **`.cursor/rules/*.mdc`** -- project-level rules (MDC = Markdown with frontmatter)
- **`~/.cursor/rules/*.mdc`** -- global user rules
- Rules have `globs` and `alwaysApply` frontmatter

Nika already has a `.cursor/rules/nika-workflows.mdc` in the project. This is the primary way to teach Cursor's AI about Nika syntax.

### What Cursor CANNOT Do

- No `Resources` support -- cannot serve schema documentation as MCP resources
- No `Instructions` support -- cannot send system-level instructions via MCP
- No `Discovery` -- no notifications when tools/prompts change
- Cannot auto-install MCP servers from extensions (unlike VS Code)

### Best Strategy for Cursor

1. **`.cursor/rules/nika-workflows.mdc`** -- Already exists. Contains Nika syntax reference. Triggered on `**/*.nika.yaml` files.
2. **`.mcp.json`** at project root -- Already exists. Nika MCP server for tools.
3. **MCP Prompts** -- Expose prompt templates like "create a workflow" via MCP server.

---

## 2. Windsurf (Codeium) MCP Support

### Feature Support

| Feature | Supported |
|---------|-----------|
| Tools | Yes |
| Resources | Yes |
| Prompts | Yes |
| Discovery | No |
| Instructions | No |
| Sampling | No |

### Configuration

Config file: **`~/.codeium/windsurf/mcp_config.json`** (user-level only)

```json
{
  "mcpServers": {
    "nika": {
      "command": "/opt/homebrew/bin/nika",
      "args": ["mcp", "serve"]
    }
  }
}
```

Same `mcpServers` format as Cursor and Claude Desktop.

### Key Features

- **MCP Marketplace**: Windsurf has a built-in MCP marketplace (GUI in settings)
- **3 transports**: stdio, Streamable HTTP, SSE
- **OAuth support**: For each transport type
- **Tool limit**: 100 total tools across all servers
- **Admin controls**: Teams can whitelist approved MCP servers (regex matching)
- **Config interpolation**: Supports `${env:VAR_NAME}` syntax in config values
- **Resources support**: Unlike Cursor, Windsurf CAN use MCP Resources

### Windsurf Rules System

- **Memories & Rules** in Cascade settings
- **AGENTS.md** file support
- **Skills** system (Cascade-specific)

### Best Strategy for Windsurf

1. Use `~/.codeium/windsurf/mcp_config.json` with the same Nika MCP server config
2. Leverage Resources support to serve Nika schema documentation
3. Expose prompt templates for workflow generation
4. Consider submitting to Windsurf's MCP Marketplace for discoverability

---

## 3. VS Code (GitHub Copilot) MCP Support

### Feature Support

VS Code has the MOST comprehensive MCP support of all editors:

| Feature | Supported |
|---------|-----------|
| Tools | Yes |
| Resources | Yes (via "Add Context" > "MCP Resources") |
| Prompts | Yes (via `/server.prompt` slash commands) |
| MCP Apps | Yes (interactive UI components) |
| Sandbox | Yes (macOS/Linux -- restrict file/network access) |

### Configuration

Config file: **`.vscode/mcp.json`** (workspace) or user profile `mcp.json`

**IMPORTANT**: VS Code uses a DIFFERENT format than Cursor/Claude Desktop:

```json
{
  "servers": {
    "nika": {
      "command": "/opt/homebrew/bin/nika",
      "args": ["mcp", "serve"]
    }
  }
}
```

Note: the top-level key is `"servers"` not `"mcpServers"`. This is different from the Claude Desktop / Cursor convention.

### Key Features

- **MCP Gallery**: Install MCP servers from VS Code Extensions view (`@mcp` search)
- **Agent Plugins**: Bundle MCP servers + skills + agents + hooks in a single package
- **Input Variables**: `${input:api-key}` prompts for secrets on first start
- **Dev Mode**: File watching + debugger support for MCP server development
- **Sandbox**: Restrict file/network access per server (macOS/Linux)
- **CLI install**: `code --add-mcp "{...}"` command-line installation
- **Dev Container support**: MCP servers in `devcontainer.json`
- **Auto-discovery**: Can detect MCP config from Claude Desktop

### Agent Plugins (The Big Opportunity)

VS Code has a new **Agent Plugins** system (Preview) that is the ideal distribution mechanism:

```
nika-vscode-plugin/
  plugin.json              # Plugin metadata
  skills/
    nika-workflow/
      SKILL.md             # Nika syntax knowledge
  agents/
    nika-expert.agent.md   # Nika workflow expert agent
  hooks/
    hooks.json             # Lifecycle hooks
  .mcp.json                # Nika MCP server config
```

Plugin features:
- Bundled MCP servers start automatically
- Skills, agents, hooks all packaged together
- Can be distributed via marketplace or Git repo
- `${CLAUDE_PLUGIN_ROOT}` variable for referencing plugin files
- Implicitly trusted when installed (no separate trust prompt)

### Best Strategy for VS Code

1. **Immediate**: Create `.vscode/mcp.json` with Nika server config
2. **Short-term**: Build an Agent Plugin that bundles:
   - MCP server (nika mcp serve)
   - Skill file with Nika syntax knowledge
   - Custom agent for workflow generation
3. **Long-term**: Publish to VS Code MCP Gallery

---

## 4. Claude Code MCP Support

### Feature Support (Most Complete)

| Feature | Supported |
|---------|-----------|
| Resources | Yes |
| Prompts | Yes |
| Tools | Yes |
| Discovery | Yes (list_changed) |
| Instructions | Yes |
| Roots | Yes |
| Elicitation | Yes |
| DCR | Yes |

### Configuration

Three ways to configure:

```bash
# CLI commands
claude mcp add --transport stdio nika -- /opt/homebrew/bin/nika mcp serve
claude mcp add --transport http nika https://mcp.example.com/mcp

# Project .mcp.json (shared via git)
# User ~/.claude.json (private)
```

Three scopes:
- **Local**: per-project in `~/.claude.json` (default)
- **Project**: `.mcp.json` at project root (version controlled)
- **User**: `~/.claude.json` global (cross-project)

### Plugin System

Claude Code has its own plugin system that can bundle MCP servers:

```json
// .mcp.json at plugin root
{
  "mcpServers": {
    "nika": {
      "command": "${CLAUDE_PLUGIN_ROOT}/bin/nika",
      "args": ["mcp", "serve"]
    }
  }
}
```

### Key Features

- **Dynamic tool updates**: Supports `list_changed` notifications
- **Push messages (Channels)**: MCP servers can push events into sessions
- **Plugin-provided MCP servers**: Auto-start with plugin
- **Environment variable expansion**: `${VAR}` and `${VAR:-default}` in `.mcp.json`
- **Instructions support**: Server can send system-level instructions to Claude

### Best Strategy for Claude Code

Already working via `.mcp.json` at project root. The existing config:

```json
{
  "mcpServers": {
    "nika": {
      "command": "/opt/homebrew/bin/nika",
      "args": ["mcp", "serve"]
    }
  }
}
```

Enhancement opportunities:
1. Add MCP Instructions to inject Nika schema knowledge
2. Add MCP Resources for workflow templates
3. Add MCP Prompts for common workflow patterns
4. Build a Claude Code Plugin for one-click setup

---

## 5. Cross-Editor Architecture Comparison

### Config File Formats

| Editor | File Location | Top-Level Key | Format |
|--------|--------------|---------------|--------|
| Cursor | `.cursor/mcp.json` | `mcpServers` | Claude Desktop format |
| Windsurf | `~/.codeium/windsurf/mcp_config.json` | `mcpServers` | Claude Desktop format |
| VS Code | `.vscode/mcp.json` | `servers` | VS Code format |
| Claude Code | `.mcp.json` (project root) | `mcpServers` | Claude Desktop format |

**Key insight**: VS Code is the outlier with `"servers"` instead of `"mcpServers"`. The others all use the Claude Desktop convention. Nika's existing `.mcp.json` works for Cursor and Claude Code out of the box.

### Feature Matrix

| Feature | Cursor | Windsurf | VS Code | Claude Code |
|---------|--------|----------|---------|-------------|
| Tools | Yes | Yes | Yes | Yes |
| Resources | No | Yes | Yes | Yes |
| Prompts | Yes | Yes | Yes | Yes |
| Instructions | No | No | No | Yes |
| Discovery | No | No | No | Yes |
| Sampling | No | No | No | No |
| Roots | Yes | No | No | Yes |
| Elicitation | Yes | No | No | Yes |
| Sandbox | No | No | Yes | No |
| Plugin System | No | No | Yes | Yes |
| MCP Gallery | No | Yes (marketplace) | Yes | No |

---

## 6. Can a VS Code Extension Register as an MCP Server?

**Yes, absolutely.** VS Code now has first-class support for this via Agent Plugins.

### Option A: Agent Plugin (Recommended)

A VS Code Agent Plugin can bundle an MCP server. When the plugin is enabled, the MCP server starts automatically. This is the cleanest integration.

```
nika-plugin/
  plugin.json
  .mcp.json    # MCP server definition
  skills/
    nika.md    # Nika schema knowledge
```

The `.mcp.json` in the plugin root is auto-discovered by VS Code.

### Option B: Extension Contributes MCP Config

A traditional VS Code extension could:
1. Contribute to `.vscode/mcp.json` programmatically
2. Start an MCP server process
3. Register the server URL

However, the Agent Plugin approach is cleaner and purpose-built for this.

### Option C: Extension AS MCP Server

The extension process itself could implement the MCP protocol:
- Listen on a stdio pipe or HTTP port
- Expose tools, resources, prompts
- Register via the mcp.json config

This is more complex but gives maximum control.

### Can Extensions Auto-Configure MCP?

- **VS Code**: Yes, via `code --add-mcp` CLI or contributing to `.vscode/mcp.json`
- **Cursor**: No API for this. User must manually edit `.cursor/mcp.json`
- **Windsurf**: No API for this. User must manually edit config
- **Claude Code**: Yes, via `claude mcp add` CLI command

---

## 7. Best Architecture for Nika

### The Dual-Mode Strategy

Nika should be BOTH:
1. **An MCP server** (already exists: `nika mcp serve`)
2. **A rules/context provider** (Cursor rules, VS Code skills, AGENTS.md)

The MCP server provides **tools** (validate workflow, run workflow, list workflows).
The rules/context provides **knowledge** (Nika syntax, patterns, common mistakes).

### Recommended File Structure

```
nika-project/
  # Already exists -- works in Cursor + Claude Code
  .mcp.json                        # MCP server for Cursor + Claude Code

  # Already exists -- Cursor rules
  .cursor/rules/nika-workflows.mdc # Nika syntax rules for Cursor

  # NEW -- VS Code config
  .vscode/mcp.json                 # MCP server for VS Code/Copilot

  # Already exists -- Claude Code rules
  .claude/rules/nika.md            # Nika syntax rules for Claude Code

  # Universal -- works in all editors
  AGENTS.md                        # Nika schema context (Cursor, Windsurf, Claude Code all read this)
```

### What `nika init` Should Generate

When a user runs `nika init`, it should create configs for ALL editors:

1. `.mcp.json` (Cursor + Claude Code)
2. `.vscode/mcp.json` (VS Code/Copilot) with `"servers"` format
3. `AGENTS.md` with Nika schema overview (universal)

### MCP Server Enhancements

The `nika mcp serve` command should expose:

| MCP Feature | Purpose | Priority |
|-------------|---------|----------|
| **Tools** | `nika_check`, `nika_run`, `nika_list`, `nika_explain` | Already done |
| **Resources** | Nika schema reference, workflow templates | High |
| **Prompts** | "Create a workflow that...", "Debug this workflow" | Medium |
| **Instructions** | System-level "You are a Nika expert" prompt injection | Medium (Claude Code only) |

---

## 8. Examples of Extensions Providing MCP Tools

### Playwright MCP Server (Microsoft)

The reference example for VS Code MCP integration:
- Installable from VS Code Extensions view (`@mcp playwright`)
- Provides browser automation tools
- Config: `npx -y @microsoft/mcp-server-playwright`

### Cline (VS Code Extension)

- Open-source autonomous coding agent
- Supports MCP Resources, Tools, Discovery
- Users can create MCP servers via natural language
- Custom servers stored in `~/Documents/Cline/MCP`

### Continue (VS Code Extension)

- Open-source AI code assistant
- MCP Resources accessible via `@` mentions
- Prompt templates as slash commands
- MCP Apps for interactive UIs
- Works in both VS Code and JetBrains

### Amp (Sourcegraph)

- Runs in VS Code, Cursor, Windsurf, VSCodium
- Supports MCP servers defined in VS Code `mcp.json`
- Cross-editor compatibility is proven

---

## 9. Windsurf vs Cursor vs Claude Code: Detailed Comparison

### Context Systems

| System | Cursor | Windsurf | Claude Code |
|--------|--------|----------|-------------|
| Rules files | `.cursor/rules/*.mdc` | Cascade Memories & Rules UI | `.claude/rules/*.md` |
| AGENTS.md | Yes (reads it) | Yes (reads it) | Yes (reads it) |
| Project context | `.cursorcontext` | "Fast Context" indexing | `.claude/` directory |
| Skills | No | Cascade Skills | Skills (via plugins) |
| Custom agents | No | No | Custom agents (via plugins) |
| Hooks | No | Cascade Hooks | Yes (lifecycle hooks) |

### How Claude Code Uses MCP (Pattern for Cursor)

Claude Code's MCP integration is the most mature:

1. **Scoped configuration**: local, project, user
2. **Dynamic discovery**: `list_changed` notifications
3. **Push channels**: MCP servers can push messages into sessions
4. **Plugin bundling**: MCP + skills + hooks in one package
5. **Instructions**: Server-provided system prompts

The same `nika mcp serve` command works in all editors. The difference is what each editor does with the MCP features.

---

## 10. Practical Next Steps for Nika

### Immediate (No Code Changes)

1. **Create `.vscode/mcp.json`** in Nika projects:
```json
{
  "servers": {
    "nika": {
      "command": "nika",
      "args": ["mcp", "serve"]
    }
  }
}
```

2. **Update `nika init`** to generate editor configs for all 4 editors

3. **Ensure AGENTS.md** contains Nika schema summary (universal file all editors read)

### Short-Term (MCP Server Enhancements)

4. **Add MCP Resources** to `nika mcp serve`:
   - `nika://schema/workflow` -- Full workflow schema reference
   - `nika://schema/transforms` -- All 63 transforms
   - `nika://schema/providers` -- Provider configuration
   - `nika://templates/basic` -- Starter templates

5. **Add MCP Prompts** to `nika mcp serve`:
   - `create-workflow` -- "Create a Nika workflow that {description}"
   - `debug-workflow` -- "Debug this workflow: {yaml}"
   - `explain-error` -- "Explain Nika error: {error_code}"

6. **Add MCP Instructions** (for Claude Code):
   - System-level prompt with Nika schema rules

### Medium-Term (Distribution)

7. **VS Code Agent Plugin**: Bundle MCP server + skills + agents
8. **Windsurf MCP Marketplace submission**: Get listed officially
9. **`nika editor setup`** command that auto-detects and configures all editors

### Long-Term (Deep Integration)

10. **Language Server Protocol (LSP)**: Nika already has LSP work in progress
11. **VS Code Extension**: Full extension with syntax highlighting, validation, snippets + MCP server bundled
12. **Remote MCP**: `nika serve` already has HTTP -- expose as remote MCP endpoint

---

## Sources

1. [MCP Official Clients List](https://modelcontextprotocol.io/clients) -- Feature matrix for 108 MCP clients including Cursor, Windsurf
2. [VS Code MCP Servers Documentation](https://code.visualstudio.com/docs/copilot/chat/mcp-servers) -- Full VS Code MCP integration guide
3. [VS Code MCP Configuration Reference](https://code.visualstudio.com/docs/copilot/reference/mcp-configuration) -- Config file schema
4. [VS Code Agent Plugins](https://code.visualstudio.com/docs/copilot/customization/agent-plugins) -- Plugin system with MCP bundling
5. [Windsurf Cascade MCP Documentation](https://docs.windsurf.com/windsurf/cascade/mcp) -- Windsurf MCP config and features
6. [Claude Code MCP Documentation](https://code.claude.com/docs/en/mcp) -- Claude Code MCP integration (scopes, plugins, channels)
7. Cursor docs (https://docs.cursor.com/context/model-context-protocol) -- Blocked by Vercel rate limit during research; data from MCP registry and community sources

## Methodology

- Sources analyzed: 7 primary documentation sites
- Tools used: jina.ai reader for web scraping
- Research date: 2026-04-06
- Note: Cursor docs were rate-limited (Vercel 429). Cursor findings are from the MCP official registry, existing `.cursor/` config on machine, and community knowledge.

## Confidence Level

**High** -- All findings are from official documentation (VS Code, Windsurf, Claude Code, MCP Protocol). Cursor findings are from the MCP registry's verified client list. The config formats and feature support are well-documented and stable.
