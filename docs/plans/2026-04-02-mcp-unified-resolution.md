# MCP Unified Resolution — Design Document

> Date: 2026-04-02 | Status: APPROVED | Author: Thibaut + Claude

## Problem

MCP server configuration is fragmented across 4 disconnected systems:

1. `.mcp.json` at project root (Claude Code convention) — read by CLI only
2. `.nika/mcp.yaml` (legacy project) — read by CLI as fallback
3. `~/.nika/mcp.yaml` (global) — read by CLI
4. Workflow inline `mcp:` block — read by runtime

Users must redeclare MCP servers inline in every workflow, even when they're already configured in `.mcp.json`. No way to reference a server by name.

## Solution

Add `from:` field to workflow MCP declarations. Servers are resolved from config files at lowering time (before execution) with deep merge of overrides.

```yaml
mcp:
  neo4j:
    from: config                    # resolve from .mcp.json > global
    env:                            # field-level override
      NEO4J_URI: bolt://staging:7687
  custom-scraper:
    command: ./bin/scraper           # full inline (no from:)
    args: [--headless]
```

## Architecture

```
Priority (highest wins):

  Layer 3: WORKFLOW (from: + overrides, or full inline)
     ↑
  Layer 2: PROJECT (.mcp.json at root, or .nika/mcp.yaml legacy)
     ↑
  Layer 1: GLOBAL (~/.nika/mcp.yaml)
```

Single canonical config module: `nika-engine/src/core/mcp_config.rs`.

## from: Values

| Value | Resolves from | v0 |
|-------|---------------|-----|
| `config` | .mcp.json > .nika/mcp.yaml > ~/.nika/mcp.yaml | YES |
| `project` | .mcp.json > .nika/mcp.yaml only | YES |
| `global` | ~/.nika/mcp.yaml only | YES |
| `@pkg/name` | Package registry (future) | NO — NIKA-109 |
| `https://...` | Remote URL (future) | NO — NIKA-109 |
| `./path` | Local file (future) | NO — NIKA-109 |

## Detection Rules (Analyzer)

| Has `command:` | Has `from:` | Result |
|----------------|-------------|--------|
| YES | NO | Inline (existing behavior) |
| NO | YES | Reference (resolve from config) |
| YES | YES | ERROR NIKA-110 |
| NO | NO | ERROR NIKA-111 |

## Field-Level Override (Deep Merge)

When `from:` resolves a base config, workflow fields override selectively:

| Field | Merge behavior |
|-------|---------------|
| `env:` | MERGED — workflow keys win, config keys preserved |
| `args:` | REPLACED — workflow args win entirely if provided |
| `cwd:` | REPLACED — workflow cwd wins if provided |
| `command:` | NOT ALLOWED with `from:` (error NIKA-110) |

## Error Codes

| Code | When | Message |
|------|------|---------|
| NIKA-108 | Lowering | Server 'X' not found in config (from: config) |
| NIKA-109 | Analysis | Unknown from: source 'X' |
| NIKA-110 | Analysis | Server has both from: and command: |
| NIKA-111 | Analysis | Server missing both from: and command: |

All errors caught BEFORE execution starts (lowering/analysis time).

## Runtime Flow

```
Workflow YAML
  ↓ parse_mcp_config() [parser.rs]
RawMcpServer { command?, from?, args?, env?, cwd? }
  ↓ analyze_mcp_server() [analyze.rs]
AnalyzedMcpServer { from: Option<McpFromSource>, command?, ... }
  ↓ lower_mcp_servers(servers, &resolver) [lower.rs]
  │   ├── from: Some → resolver.resolve(name, source) → base config
  │   │     └── deep_merge(base, workflow_overrides) → McpConfigInline
  │   └── from: None → McpConfigInline directly (unchanged)
FxHashMap<String, McpConfigInline>
  ↓ TaskExecutor::new()
McpClientPool::with_configs()
  ↓ get_or_connect()
McpClient (connected)
```

## McpConfigResolver

```rust
pub struct McpConfigResolver {
    project: Option<McpConfig>,  // .mcp.json or .nika/mcp.yaml
    global: Option<McpConfig>,   // ~/.nika/mcp.yaml
}

impl McpConfigResolver {
    pub fn new() -> Self {
        let project = load_project_config().ok().flatten();
        let global = load_global_config().ok().flatten();
        Self { project, global }
    }

    pub fn resolve(&self, name: &str, source: McpFromSource)
        -> Result<McpServer, NikaError>
    {
        match source {
            McpFromSource::Config => {
                self.project.as_ref()
                    .and_then(|c| c.servers.get(name))
                    .or_else(|| self.global.as_ref()
                        .and_then(|c| c.servers.get(name)))
                    .cloned()
                    .ok_or_else(|| nika_error!(108, ...))
            }
            McpFromSource::Project => { ... }
            McpFromSource::Global => { ... }
        }
    }
}
```

## Serve Integration

Both serve modes get `from:` support for free:

- **Embedded**: calls `Runner::with_event_log()` which creates `McpConfigResolver` internally. `.mcp.json` auto-discovered via `find_project_root()`.
- **Subprocess**: spawns `nika run` which has its own Runner. Same auto-discovery via CWD walk-up.

Zero changes to serve code.

## Dead Code Nuke List

| What | Where | Lines |
|------|-------|-------|
| NikaMcpConfigManager + all types | nika-mcp/src/nika_config.rs | ~1178 (full delete) |
| lib.rs re-exports | nika-mcp/src/lib.rs:24-28 | 5 |
| McpServerConfig struct | boot.rs:162-169 | 8 |
| BootstrapConfig.mcp field | boot.rs:194 | 3 |
| BootContext.mcp_servers field | boot.rs:144 | 1 |
| config list [mcp] display | cli/config.rs | ~10 |
| Mintlify stale paths | 8 docs files | 12 refs |

Net: ~1200 lines removed, ~400 added.

## Implementation Phases (TDD)

| Phase | What | Files | Tests |
|-------|------|-------|-------|
| 1 | Parser: add `from:` to RawMcpServer | raw/mcp.rs, parser.rs | 3 |
| 2 | Analyzer: validate from: vs command: | analyze.rs, workflow.rs | 4 |
| 3 | McpConfigResolver | core/mcp_config.rs | 4 |
| 4 | Lowering: resolve + deep merge | lower.rs | 5 |
| 5 | Serve E2E verification | (test only) | 2 |
| 6 | Dead code nuke | nika_config.rs, boot.rs, config.rs | update existing |
| 7 | Docs: Mintlify + CHANGELOG + skills | 8+ docs files | — |
| 8 | Final 10-agent audit | — | — |

## Migration

Zero breaking changes — existing workflows with `command:` continue to work unchanged. The `from:` field is purely additive.
