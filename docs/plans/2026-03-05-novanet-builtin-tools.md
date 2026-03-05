# NovaNet Builtin Tools Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Transform 14 NovaNet MCP tools into native Nika builtin tools (`novanet:*`) for ~25x performance improvement by eliminating MCP spawn/stdio overhead.

**Architecture:** Two-tier approach: (1) Fix JSON schema to allow `nika:*` and `novanet:*` tools in `invoke:` without requiring `mcp:`/`server:` field, (2) Add `novanet:*` builtin tools with direct `neo4rs` connection to Neo4j, feature-gated behind `--features novanet-builtin`.

**Tech Stack:** Rust, neo4rs (async Neo4j driver), moka (caching), serde_json, JSON Schema

---

## Overview

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  CURRENT: MCP Protocol Overhead (~50ms per call)                              ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  Nika Workflow                                                                ║
║       │                                                                       ║
║       ▼                                                                       ║
║  invoke: novanet_describe                                                     ║
║       │                                                                       ║
║       ├── Spawn MCP Server process (~30ms)                                    ║
║       ├── JSON-RPC via stdio (~10ms)                                          ║
║       ├── NovaNet MCP parses, calls Neo4j                                     ║
║       ├── JSON-RPC response via stdio (~10ms)                                 ║
║       └── Parse response                                                      ║
║                                                                               ║
║  TOTAL: ~50-100ms per tool call                                               ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝

╔═══════════════════════════════════════════════════════════════════════════════╗
║  TARGET: Direct Neo4j Connection (~2ms per call)                              ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  Nika Workflow                                                                ║
║       │                                                                       ║
║       ▼                                                                       ║
║  invoke: novanet:describe                                                     ║
║       │                                                                       ║
║       ├── BuiltinToolRouter.is_builtin("novanet:describe") → true             ║
║       ├── Dispatch to NovanetDescribeTool                                     ║
║       ├── Direct neo4rs query (connection pooled)                             ║
║       └── Return result                                                       ║
║                                                                               ║
║  TOTAL: ~2-5ms per tool call (25x faster!)                                    ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

---

## Phase 1: Quick Win - Fix JSON Schema (v0.21.0)

Allow `invoke: { tool: "nika:*" }` and `invoke: { tool: "novanet:*" }` without requiring `mcp:` or `server:`.

### Task 1.1: Update JSON Schema InvokeParams

**Files:**
- Modify: `schemas/nika-workflow.schema.json:438-468`

**Step 1: Read current schema**

```bash
cat schemas/nika-workflow.schema.json | jq '.definitions.InvokeParams'
```

Expected: See current `oneOf` with 4 alternatives (all require mcp/server)

**Step 2: Edit InvokeParams to add 5th alternative**

In `schemas/nika-workflow.schema.json`, find the `InvokeParams` definition and update `oneOf`:

```json
"InvokeParams": {
  "type": "object",
  "additionalProperties": false,
  "properties": {
    "mcp": {
      "type": "string",
      "description": "MCP server name (must match a key in workflow's mcp config)"
    },
    "server": {
      "type": "string",
      "description": "MCP server name (legacy alias for 'mcp', deprecated)"
    },
    "tool": {
      "type": "string",
      "description": "Tool name to call (mutually exclusive with resource)"
    },
    "params": {
      "type": "object",
      "description": "Parameters to pass to the tool"
    },
    "resource": {
      "type": "string",
      "description": "Resource URI to read (mutually exclusive with tool)"
    }
  },
  "oneOf": [
    { "required": ["mcp", "tool"] },
    { "required": ["mcp", "resource"] },
    { "required": ["server", "tool"] },
    { "required": ["server", "resource"] },
    {
      "required": ["tool"],
      "properties": {
        "tool": {
          "type": "string",
          "pattern": "^(nika:|novanet:)",
          "description": "Builtin tool (nika:* or novanet:*) - no MCP server required"
        }
      }
    }
  ]
}
```

**Step 3: Validate schema syntax**

```bash
cd /Users/thibaut/dev/supernovae/nika
python3 -c "import json; json.load(open('schemas/nika-workflow.schema.json'))" && echo "✅ Valid JSON"
```

Expected: `✅ Valid JSON`

**Step 4: Commit**

```bash
git add schemas/nika-workflow.schema.json
git commit -m "feat(schema): allow builtin tools in invoke: without mcp: field

- Add 5th oneOf alternative for nika:* and novanet:* tools
- Pattern ^(nika:|novanet:) validates builtin tool prefix
- Enables: invoke: { tool: 'nika:write', params: {...} }
- Backward compatible: existing workflows still validate"
```

---

### Task 1.2: Create Test Workflow for Builtin invoke:

**Files:**
- Create: `examples/test-invoke-builtin-tools.nika.yaml`

**Step 1: Write test workflow**

```yaml
# Test: Builtin tools via invoke: (should work!)
# This tests that nika:* tools work in invoke: tasks without mcp: field
schema: nika/workflow@0.10
workflow: test-invoke-builtin-tools
description: "Verify builtin tools work in invoke: tasks without MCP server"

tasks:
  # Test 1: Write a file via invoke:
  - id: write_file
    invoke:
      tool: nika:write
      params:
        file_path: /tmp/nika-invoke-test.txt
        content: "Hello from invoke: task!"

  # Test 2: Read the file back via invoke:
  - id: read_file
    invoke:
      tool: nika:read
      params:
        file_path: /tmp/nika-invoke-test.txt

  # Test 3: Glob to find the file
  - id: glob_files
    invoke:
      tool: nika:glob
      params:
        pattern: "nika-invoke-*.txt"
        path: /tmp

  # Test 4: Use nika:log
  - id: log_result
    use:
      content: read_file
    invoke:
      tool: nika:log
      params:
        level: info
        message: "Read content: {{use.content}}"

flows:
  - source: write_file
    target: read_file
  - source: read_file
    target: glob_files
  - source: glob_files
    target: log_result
```

**Step 2: Validate workflow syntax**

```bash
cargo run -- check examples/test-invoke-builtin-tools.nika.yaml
```

Expected: `✅ Workflow valid` (no schema errors)

**Step 3: Run workflow**

```bash
cargo run -- run examples/test-invoke-builtin-tools.nika.yaml
```

Expected: All 4 tasks complete successfully

**Step 4: Commit**

```bash
git add examples/test-invoke-builtin-tools.nika.yaml
git commit -m "test: add workflow for builtin tools in invoke: tasks"
```

---

### Task 1.3: Update BuiltinToolRouter.is_builtin()

**Files:**
- Modify: `tools/nika/src/runtime/builtin/router.rs:45-50`

**Step 1: Read current implementation**

```bash
grep -A5 "fn is_builtin" tools/nika/src/runtime/builtin/router.rs
```

Expected: Only checks `nika:` prefix

**Step 2: Update is_builtin() to include novanet: prefix**

```rust
/// Check if a tool name is a builtin tool (nika:* or novanet:*)
#[inline]
pub fn is_builtin(tool_name: &str) -> bool {
    tool_name.starts_with("nika:") || tool_name.starts_with("novanet:")
}

/// Extract tool name from prefixed builtin tool
/// Returns None if not a builtin tool
pub fn extract_name(tool_name: &str) -> Option<(&str, &str)> {
    if let Some(name) = tool_name.strip_prefix("nika:") {
        Some(("nika", name))
    } else if let Some(name) = tool_name.strip_prefix("novanet:") {
        Some(("novanet", name))
    } else {
        None
    }
}
```

**Step 3: Add tests for novanet: prefix**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_builtin_nika_prefix() {
        assert!(BuiltinToolRouter::is_builtin("nika:sleep"));
        assert!(BuiltinToolRouter::is_builtin("nika:read"));
        assert!(BuiltinToolRouter::is_builtin("nika:write"));
    }

    #[test]
    fn test_is_builtin_novanet_prefix() {
        assert!(BuiltinToolRouter::is_builtin("novanet:describe"));
        assert!(BuiltinToolRouter::is_builtin("novanet:query"));
        assert!(BuiltinToolRouter::is_builtin("novanet:traverse"));
    }

    #[test]
    fn test_is_builtin_external_tools() {
        assert!(!BuiltinToolRouter::is_builtin("perplexity_search"));
        assert!(!BuiltinToolRouter::is_builtin("custom_tool"));
        assert!(!BuiltinToolRouter::is_builtin("mcp:something"));
    }

    #[test]
    fn test_extract_name_nika() {
        assert_eq!(
            BuiltinToolRouter::extract_name("nika:sleep"),
            Some(("nika", "sleep"))
        );
    }

    #[test]
    fn test_extract_name_novanet() {
        assert_eq!(
            BuiltinToolRouter::extract_name("novanet:describe"),
            Some(("novanet", "describe"))
        );
    }

    #[test]
    fn test_extract_name_external() {
        assert_eq!(BuiltinToolRouter::extract_name("external_tool"), None);
    }
}
```

**Step 4: Run tests**

```bash
cargo test --package nika -- router::tests --nocapture
```

Expected: All new tests pass

**Step 5: Commit**

```bash
git add tools/nika/src/runtime/builtin/router.rs
git commit -m "feat(builtin): add novanet: prefix support to is_builtin()

- is_builtin() now returns true for novanet:* tools
- extract_name() returns (prefix, name) tuple
- Prepares for Phase 2: novanet builtin tools
- 6 new tests for prefix handling"
```

---

### Task 1.4: Update CLAUDE.md Documentation

**Files:**
- Modify: `tools/nika/CLAUDE.md`

**Step 1: Find the "Builtin Tools" section**

```bash
grep -n "Builtin Tools" tools/nika/CLAUDE.md
```

**Step 2: Update documentation**

Add/update the builtin tools section to reflect that:
1. `nika:*` tools work in both `invoke:` and `agent:` tasks
2. `novanet:*` prefix is reserved for future builtin tools
3. Remove outdated "file tools only in agent:" statement

**Key changes:**
```markdown
### Builtin Tools (11 + future novanet:*)

Nika provides builtin tools via `BuiltinToolRouter`. These work in **both** `invoke:` and `agent:` tasks without requiring an MCP server.

**Core tools (6):**
| Tool | Description | Works In |
|------|-------------|----------|
| `nika:sleep` | Pause execution | invoke:, agent: |
| `nika:log` | Emit log event | invoke:, agent: |
| `nika:emit` | Custom event | invoke:, agent: |
| `nika:assert` | Validate condition | invoke:, agent: |
| `nika:prompt` | HITL user input | invoke:, agent: |
| `nika:run` | Execute sub-workflow | invoke:, agent: |

**File tools (5):**
| Tool | Description | Works In |
|------|-------------|----------|
| `nika:read` | Read file | invoke:, agent: |
| `nika:write` | Create/overwrite file | invoke:, agent: |
| `nika:edit` | Modify file | invoke:, agent: |
| `nika:glob` | Find files by pattern | invoke:, agent: |
| `nika:grep` | Search content | invoke:, agent: |

**Usage in invoke: (NEW in v0.21.0):**
```yaml
# No MCP server needed for builtin tools!
tasks:
  - id: write_result
    invoke:
      tool: nika:write
      params:
        file_path: ./output.txt
        content: "Hello!"
```

**Reserved prefix - novanet:* (future):**
The `novanet:` prefix is reserved for future NovaNet builtin tools that will provide direct Neo4j access without MCP overhead.
```

**Step 3: Commit**

```bash
git add tools/nika/CLAUDE.md
git commit -m "docs: update builtin tools documentation for v0.21.0

- Clarify nika:* tools work in invoke: AND agent: tasks
- Remove outdated 'file tools only in agent:' statement
- Document reserved novanet:* prefix for future builtin tools
- Add usage example for invoke: with builtin tools"
```

---

### Task 1.5: Run Full Test Suite

**Step 1: Run all tests**

```bash
cd /Users/thibaut/dev/supernovae/nika/tools/nika
cargo test --all-features
```

Expected: All 3,808+ tests pass

**Step 2: Run clippy**

```bash
cargo clippy -- -D warnings
```

Expected: Zero warnings

**Step 3: Commit version bump**

```bash
# Update Cargo.toml version to 0.21.0
git add Cargo.toml
git commit -m "chore: bump version to v0.21.0"
```

---

## Phase 2: NovaNet Builtin Module (v0.22.0)

Add 14 `novanet:*` builtin tools with direct Neo4j connection.

### Task 2.1: Add neo4rs Dependency

**Files:**
- Modify: `tools/nika/Cargo.toml`

**Step 1: Add feature-gated dependency**

```toml
[features]
default = ["tui"]
tui = ["ratatui", "crossterm", "tui-tree-widget"]
novanet-builtin = ["neo4rs", "moka"]  # NEW

[dependencies]
# ... existing deps ...

# NovaNet builtin tools (optional)
neo4rs = { version = "0.8", optional = true }
moka = { version = "0.12", optional = true, features = ["future"] }
```

**Step 2: Verify compilation**

```bash
cargo check --features novanet-builtin
```

Expected: Compiles without errors

**Step 3: Commit**

```bash
git add tools/nika/Cargo.toml
git commit -m "feat(deps): add neo4rs and moka for novanet builtin tools

- neo4rs v0.8 for direct Neo4j connection
- moka v0.12 for async caching
- Feature-gated: --features novanet-builtin
- No impact on default builds"
```

---

### Task 2.2: Create NovanetClient Module

**Files:**
- Create: `tools/nika/src/novanet/mod.rs`
- Create: `tools/nika/src/novanet/client.rs`

**Step 1: Create module structure**

```rust
// src/novanet/mod.rs
#[cfg(feature = "novanet-builtin")]
mod client;
#[cfg(feature = "novanet-builtin")]
mod tools;

#[cfg(feature = "novanet-builtin")]
pub use client::NovanetClient;
#[cfg(feature = "novanet-builtin")]
pub use tools::*;
```

**Step 2: Implement NovanetClient**

```rust
// src/novanet/client.rs
use neo4rs::{Graph, Query};
use moka::future::Cache;
use std::sync::Arc;
use std::time::Duration;

use crate::error::NikaError;

/// Direct Neo4j client for novanet:* builtin tools
pub struct NovanetClient {
    graph: Arc<Graph>,
    cache: Cache<String, serde_json::Value>,
}

impl NovanetClient {
    /// Create new client from connection URI
    pub async fn new(uri: &str, user: &str, password: &str) -> Result<Self, NikaError> {
        let graph = Graph::new(uri, user, password)
            .await
            .map_err(|e| NikaError::McpError {
                server: "novanet-builtin".into(),
                reason: format!("Failed to connect to Neo4j: {}", e),
            })?;

        let cache = Cache::builder()
            .max_capacity(1000)
            .time_to_live(Duration::from_secs(300)) // 5 min TTL
            .build();

        Ok(Self {
            graph: Arc::new(graph),
            cache,
        })
    }

    /// Execute read-only Cypher query
    pub async fn query(&self, cypher: &str, params: Option<serde_json::Value>) -> Result<Vec<serde_json::Value>, NikaError> {
        let mut query = Query::new(cypher.to_string());

        if let Some(p) = params {
            if let serde_json::Value::Object(map) = p {
                for (key, value) in map {
                    query = query.param(&key, value);
                }
            }
        }

        let mut result = self.graph.execute(query).await.map_err(|e| {
            NikaError::McpToolError {
                tool: "novanet:query".into(),
                reason: format!("Cypher execution failed: {}", e),
            }
        })?;

        let mut rows = Vec::new();
        while let Some(row) = result.next().await.map_err(|e| {
            NikaError::McpToolError {
                tool: "novanet:query".into(),
                reason: format!("Failed to fetch row: {}", e),
            }
        })? {
            // Convert row to JSON
            let json_row = row_to_json(&row)?;
            rows.push(json_row);
        }

        Ok(rows)
    }

    /// Get cached result or execute query
    pub async fn query_cached(&self, cache_key: &str, cypher: &str, params: Option<serde_json::Value>) -> Result<serde_json::Value, NikaError> {
        if let Some(cached) = self.cache.get(cache_key).await {
            return Ok(cached);
        }

        let rows = self.query(cypher, params).await?;
        let result = serde_json::Value::Array(rows);

        self.cache.insert(cache_key.to_string(), result.clone()).await;

        Ok(result)
    }

    /// Invalidate cache
    pub async fn invalidate_cache(&self) {
        self.cache.invalidate_all();
    }
}

fn row_to_json(row: &neo4rs::Row) -> Result<serde_json::Value, NikaError> {
    // Implementation converts neo4rs::Row to serde_json::Value
    // Handle Node, Relationship, primitive types
    todo!("Implement row_to_json conversion")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires running Neo4j
    async fn test_client_connection() {
        let client = NovanetClient::new(
            "bolt://localhost:7687",
            "neo4j",
            "password"
        ).await.unwrap();

        let result = client.query("RETURN 1 as n", None).await.unwrap();
        assert_eq!(result.len(), 1);
    }
}
```

**Step 3: Add to lib.rs**

```rust
// In src/lib.rs
#[cfg(feature = "novanet-builtin")]
pub mod novanet;
```

**Step 4: Commit**

```bash
git add tools/nika/src/novanet/
git add tools/nika/src/lib.rs
git commit -m "feat(novanet): add NovanetClient with connection pooling and caching

- Direct neo4rs connection to Neo4j
- moka cache with 5min TTL and 1000 entry limit
- query() for read-only Cypher execution
- query_cached() for cached queries
- Feature-gated: novanet-builtin"
```

---

### Task 2.3: Implement novanet:describe Tool

**Files:**
- Create: `tools/nika/src/novanet/tools/describe.rs`
- Modify: `tools/nika/src/novanet/tools/mod.rs`

**Step 1: Create tool implementation**

```rust
// src/novanet/tools/describe.rs
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::error::NikaError;
use crate::novanet::NovanetClient;
use crate::runtime::builtin::BuiltinTool;

#[derive(Debug, Deserialize)]
pub struct DescribeParams {
    pub describe: DescribeTarget,
    pub entity_key: Option<String>,
    pub category_key: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DescribeTarget {
    Schema,
    Entity,
    Category,
    Relations,
    Locales,
    Stats,
}

#[derive(Debug, Serialize)]
pub struct DescribeResult {
    pub target: String,
    pub data: serde_json::Value,
    pub token_estimate: usize,
}

pub struct NovanetDescribeTool {
    client: Arc<NovanetClient>,
}

impl NovanetDescribeTool {
    pub fn new(client: Arc<NovanetClient>) -> Self {
        Self { client }
    }
}

impl BuiltinTool for NovanetDescribeTool {
    fn name(&self) -> &'static str {
        "describe"
    }

    fn description(&self) -> &'static str {
        "Self-description of NovaNet schema, entities, categories, relations, locales, or stats"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "describe": {
                    "type": "string",
                    "enum": ["schema", "entity", "category", "relations", "locales", "stats"],
                    "description": "What to describe"
                },
                "entity_key": {
                    "type": "string",
                    "description": "Entity key (for entity target)"
                },
                "category_key": {
                    "type": "string",
                    "description": "Category key (for category target)"
                }
            },
            "required": ["describe"]
        })
    }

    fn call<'a>(
        &'a self,
        args: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, NikaError>> + Send + 'a>> {
        Box::pin(async move {
            let params: DescribeParams = serde_json::from_str(&args).map_err(|e| {
                NikaError::McpToolError {
                    tool: "novanet:describe".into(),
                    reason: format!("Invalid params: {}", e),
                }
            })?;

            let (cypher, cache_key) = match params.describe {
                DescribeTarget::Schema => (
                    "CALL apoc.meta.schema() YIELD value RETURN value",
                    "describe:schema".to_string(),
                ),
                DescribeTarget::Stats => (
                    "CALL apoc.meta.stats() YIELD nodeCount, relCount, labels, relTypes RETURN *",
                    "describe:stats".to_string(),
                ),
                DescribeTarget::Locales => (
                    "MATCH (l:Locale) RETURN l.code as code, l.name as name ORDER BY l.code",
                    "describe:locales".to_string(),
                ),
                DescribeTarget::Entity => {
                    let key = params.entity_key.ok_or_else(|| NikaError::McpToolError {
                        tool: "novanet:describe".into(),
                        reason: "entity_key required for entity target".into(),
                    })?;
                    (
                        &format!(
                            "MATCH (e:Entity {{key: $key}}) RETURN e {{ .*, labels: labels(e) }}",
                        ),
                        format!("describe:entity:{}", key),
                    )
                }
                // ... other cases
                _ => todo!("Implement other describe targets"),
            };

            let data = self.client.query_cached(&cache_key, cypher, None).await?;

            let result = DescribeResult {
                target: format!("{:?}", params.describe).to_lowercase(),
                data,
                token_estimate: estimate_tokens(&data),
            };

            serde_json::to_string(&result).map_err(|e| NikaError::McpToolError {
                tool: "novanet:describe".into(),
                reason: format!("Failed to serialize result: {}", e),
            })
        })
    }
}

fn estimate_tokens(value: &serde_json::Value) -> usize {
    // Rough estimate: 4 chars per token
    value.to_string().len() / 4
}
```

**Step 2: Commit**

```bash
git add tools/nika/src/novanet/tools/
git commit -m "feat(novanet): implement novanet:describe builtin tool

- DescribeTarget enum: schema, entity, category, relations, locales, stats
- Cached queries with 5min TTL
- Token estimation for LLM context management
- Implements BuiltinTool trait"
```

---

### Task 2.4-2.16: Implement Remaining 13 Tools

Repeat the pattern from Task 2.3 for each tool:

| Task | Tool | Priority |
|------|------|----------|
| 2.4 | `novanet:query` | HIGH - Core Cypher execution |
| 2.5 | `novanet:search` | HIGH - Fulltext/property search |
| 2.6 | `novanet:traverse` | HIGH - Graph traversal |
| 2.7 | `novanet:assemble` | HIGH - Context assembly |
| 2.8 | `novanet:atoms` | MEDIUM - Knowledge atoms |
| 2.9 | `novanet:generate` | HIGH - Complete generation context |
| 2.10 | `novanet:introspect` | MEDIUM - Schema introspection |
| 2.11 | `novanet:batch` | LOW - Bulk operations |
| 2.12 | `novanet:cache_stats` | LOW - Cache monitoring |
| 2.13 | `novanet:cache_invalidate` | LOW - Cache management |
| 2.14 | `novanet:write` | HIGH - Data writes |
| 2.15 | `novanet:check` | MEDIUM - Pre-write validation |
| 2.16 | `novanet:audit` | LOW - Quality audit |

Each tool follows the same pattern:
1. Create `src/novanet/tools/<tool_name>.rs`
2. Implement `BuiltinTool` trait
3. Add to `tools/mod.rs` exports
4. Write tests
5. Commit

---

## Phase 3: Config Integration (v0.22.0)

### Task 3.1: Add NovaNetSettings to TuiConfig

**Files:**
- Modify: `tools/nika/src/tui/config.rs`

**Step 1: Add NovaNetSettings struct**

```rust
/// NovaNet builtin tools configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NovaNetSettings {
    /// Neo4j connection URI (bolt://host:port)
    pub uri: Option<String>,

    /// Neo4j username
    pub user: Option<String>,

    /// Neo4j password (or env var reference like ${NEO4J_PASSWORD})
    pub password: Option<String>,

    /// Enable novanet:* builtin tools
    pub enabled: bool,

    /// Cache TTL in seconds
    pub cache_ttl_secs: u64,

    /// Max cache entries
    pub cache_max_entries: u64,
}

impl Default for NovaNetSettings {
    fn default() -> Self {
        Self {
            uri: None,
            user: None,
            password: None,
            enabled: false,
            cache_ttl_secs: 300,
            cache_max_entries: 1000,
        }
    }
}
```

**Step 2: Add to TuiConfig**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TuiConfig {
    pub tui: TuiSettings,
    pub chat: ChatSettings,
    pub studio: StudioSettings,
    pub paths: PathSettings,
    #[cfg(feature = "novanet-builtin")]
    pub novanet: NovaNetSettings,
}
```

**Step 3: Add tests**

```rust
#[test]
fn test_novanet_settings_default() {
    let settings = NovaNetSettings::default();
    assert!(!settings.enabled);
    assert_eq!(settings.cache_ttl_secs, 300);
}

#[test]
fn test_novanet_config_parse() {
    let toml = r#"
[novanet]
uri = "bolt://localhost:7687"
user = "neo4j"
password = "${NEO4J_PASSWORD}"
enabled = true
"#;
    let config: TuiConfig = toml::from_str(toml).unwrap();
    assert!(config.novanet.enabled);
    assert_eq!(config.novanet.uri, Some("bolt://localhost:7687".to_string()));
}
```

**Step 4: Commit**

```bash
git add tools/nika/src/tui/config.rs
git commit -m "feat(config): add [novanet] section for builtin tools

- NovaNetSettings: uri, user, password, enabled, cache settings
- Feature-gated: novanet-builtin
- Supports env var expansion (${VAR})
- 2 tests for defaults and parsing"
```

---

### Task 3.2: Wire NovanetClient to Router

**Files:**
- Modify: `tools/nika/src/runtime/builtin/router.rs`
- Modify: `tools/nika/src/runtime/executor.rs`

**Step 1: Add novanet tools to router**

```rust
impl BuiltinToolRouter {
    #[cfg(feature = "novanet-builtin")]
    pub fn with_novanet_tools(ctx: Arc<ToolContext>, novanet: Arc<NovanetClient>) -> Self {
        let mut router = Self::with_file_tools(ctx);

        // Register all 14 novanet tools
        router.novanet_tools.insert("describe", Arc::new(NovanetDescribeTool::new(novanet.clone())));
        router.novanet_tools.insert("query", Arc::new(NovanetQueryTool::new(novanet.clone())));
        // ... etc for all 14 tools

        router
    }
}
```

**Step 2: Update dispatch logic**

```rust
pub async fn dispatch(&self, tool_name: &str, args: String) -> Result<String, NikaError> {
    if let Some((prefix, name)) = Self::extract_name(tool_name) {
        match prefix {
            "nika" => {
                let tool = self.tools.get(name).ok_or_else(|| {
                    NikaError::McpToolError {
                        tool: tool_name.into(),
                        reason: format!("Unknown nika tool: {}", name),
                    }
                })?;
                tool.call(args).await
            }
            #[cfg(feature = "novanet-builtin")]
            "novanet" => {
                let tool = self.novanet_tools.get(name).ok_or_else(|| {
                    NikaError::McpToolError {
                        tool: tool_name.into(),
                        reason: format!("Unknown novanet tool: {}", name),
                    }
                })?;
                tool.call(args).await
            }
            _ => Err(NikaError::McpToolError {
                tool: tool_name.into(),
                reason: format!("Unknown builtin prefix: {}", prefix),
            })
        }
    } else {
        Err(NikaError::McpToolError {
            tool: tool_name.into(),
            reason: "Not a builtin tool".into(),
        })
    }
}
```

**Step 3: Commit**

```bash
git add tools/nika/src/runtime/builtin/router.rs
git add tools/nika/src/runtime/executor.rs
git commit -m "feat(builtin): wire novanet tools to BuiltinToolRouter

- with_novanet_tools() constructor registers 14 tools
- dispatch() routes by prefix (nika vs novanet)
- Feature-gated: novanet-builtin"
```

---

## Phase 4: Testing & Documentation (v0.22.0)

### Task 4.1: Integration Tests with Neo4j

**Files:**
- Create: `tools/nika/tests/novanet_builtin_test.rs`

```rust
#[cfg(feature = "novanet-builtin")]
mod novanet_builtin {
    use nika::novanet::NovanetClient;
    use std::sync::Arc;

    #[tokio::test]
    #[ignore] // Requires running Neo4j
    async fn test_describe_schema() {
        let client = Arc::new(
            NovanetClient::new("bolt://localhost:7687", "neo4j", "password")
                .await
                .unwrap()
        );

        let tool = nika::novanet::NovanetDescribeTool::new(client);
        let result = tool.call(r#"{"describe": "schema"}"#.to_string()).await.unwrap();

        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(parsed.get("data").is_some());
    }
}
```

### Task 4.2: Performance Benchmarks

**Files:**
- Create: `tools/nika/benches/novanet_builtin_bench.rs`

```rust
use criterion::{criterion_group, criterion_main, Criterion};

#[cfg(feature = "novanet-builtin")]
fn benchmark_describe(c: &mut Criterion) {
    // Compare MCP vs builtin performance
    c.bench_function("novanet:describe via builtin", |b| {
        // Builtin implementation
    });

    c.bench_function("novanet_describe via MCP", |b| {
        // MCP implementation for comparison
    });
}
```

### Task 4.3: Update Documentation

**Files:**
- Modify: `tools/nika/CLAUDE.md`
- Modify: `nika/CLAUDE.md`

Document:
- Feature flag usage
- Performance comparison
- Configuration guide
- Migration guide from MCP to builtin

---

## Execution Summary

| Phase | Tasks | Est. Time | Complexity |
|-------|-------|-----------|------------|
| **Phase 1** | 5 tasks | 1-2 hours | Low |
| **Phase 2** | 16 tasks | 8-12 hours | High |
| **Phase 3** | 2 tasks | 2-3 hours | Medium |
| **Phase 4** | 3 tasks | 2-3 hours | Medium |

**Total:** ~15-20 hours of implementation

---

## Verification Checklist

After each phase, verify:

- [ ] All tests pass: `cargo test --all-features`
- [ ] Zero clippy warnings: `cargo clippy -- -D warnings`
- [ ] Schema validates: `cargo run -- check examples/*.nika.yaml`
- [ ] Documentation updated
- [ ] Committed with conventional commits

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| Neo4j connection failures | Graceful fallback to MCP |
| Cache invalidation issues | Manual invalidate tool + TTL |
| Breaking changes | Feature flag isolation |
| Performance regression | Benchmarks + monitoring |
