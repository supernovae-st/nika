# Nika IDE — Phase 2b Plan: Embedded Daemon + Live DAG + MCP Prompts

> **Execution guide** for the 4 remaining features after Phase 0-7 completion (17 commits).
> Self-contained. Read top to bottom before starting.

---

## What We're Building

4 features that complete the IDE experience:

1. **nika/workflowGraph** — LSP custom request that parses a workflow and returns `{ nodes, edges }` for the DAG panel
2. **InProcessDaemonProvider** — Run daemon services directly in the LSP process (no separate daemon needed)
3. **MCP Prompts** — Add 3 prompt templates to the MCP server
4. **Event Streaming** — Engine events → LSP notification → webview for live DAG updates

---

## Execution Order

```
Task A (1h)  → nika/workflowGraph LSP request (pure AST, no daemon needed)
Task B (2h)  → InProcessDaemonProvider (boot CacheService + SecretService + estimate_cost)
Task C (30m) → MCP Prompts (rmcp prompt API)
Task D (2h)  → Event streaming pipeline (EventLog → LSP → webview)
```

Tasks A and C are independent. B depends on the DaemonProvider trait (done). D depends on B.

---

## Task A: nika/workflowGraph LSP Custom Request

### What
Parse a .nika.yaml file and return a JSON graph for the DAG webview panel.

### Files
- **Modify**: `tools/nika-lsp/src/backend.rs` — add `workflow_graph` method
- **Modify**: `tools/nika-lsp/src/main.rs` — register custom method

### Implementation

**backend.rs** — Add `workflow_graph()` handler:

```rust
pub async fn workflow_graph(
    &self,
    params: serde_json::Value,
) -> Result<serde_json::Value> {
    let uri_str = params.get("uri")
        .and_then(|v| v.as_str())
        .ok_or_else(|| tower_lsp_server::jsonrpc::Error::invalid_params("missing uri"))?;

    let uri: Uri = uri_str.parse().map_err(|_| {
        tower_lsp_server::jsonrpc::Error::invalid_params("invalid uri")
    })?;

    let content = match self.documents.get(&uri) {
        Some(doc) => doc.content().to_string(),
        None => return Ok(serde_json::json!({ "nodes": [], "edges": [] })),
    };

    // Parse with nika-core AST
    let parsed = crate::ast_integration::parse_workflow(&content);
    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    if let Some(workflow) = &parsed.workflow {
        for task in &workflow.tasks.value {
            let t = &task.value;
            let verb = match &t.action {
                Some(RawTaskAction::Infer(_)) => "infer",
                Some(RawTaskAction::Exec(_)) => "exec",
                Some(RawTaskAction::Fetch(_)) => "fetch",
                Some(RawTaskAction::Invoke(_)) => "invoke",
                Some(RawTaskAction::Agent(_)) => "agent",
                None => "unknown",
            };
            let (line, _) = ast_integration::span_to_line_col(&content, &task.span);

            nodes.push(serde_json::json!({
                "id": t.id.value,
                "label": t.id.value,
                "verb": verb,
                "status": "pending",
                "dependsOn": t.depends_on_ids(),
                "line": line,
            }));

            for dep in t.depends_on_ids() {
                edges.push(serde_json::json!({
                    "id": format!("{}->{}", dep, t.id.value),
                    "source": dep,
                    "target": t.id.value,
                    "isDataEdge": false,
                }));
            }

            // Also add implicit data dependencies from with: bindings
            if let Some(with_block) = &t.with_block {
                for (_, entry) in &with_block.value {
                    if let Some(ref_id) = entry.value.strip_prefix('$') {
                        let base_id = ref_id.split('.').next().unwrap_or(ref_id);
                        if !t.depends_on_ids().contains(&base_id) {
                            edges.push(serde_json::json!({
                                "id": format!("{}->{}:data", base_id, t.id.value),
                                "source": base_id,
                                "target": t.id.value,
                                "isDataEdge": true,
                            }));
                        }
                    }
                }
            }
        }
    }

    let workflow_name = parsed.workflow
        .as_ref()
        .and_then(|w| w.workflow.as_ref())
        .map(|s| s.value.as_str())
        .unwrap_or("workflow");

    Ok(serde_json::json!({
        "workflowName": workflow_name,
        "nodes": nodes,
        "edges": edges,
    }))
}
```

**main.rs** — Register alongside daemonStatus:

```rust
.custom_method("nika/workflowGraph", backend::NikaBackend::workflow_graph)
```

### Verification
```bash
cd tools && cargo check -p nika-lsp
cargo test -p nika-lsp
```

### Commit
```
feat(lsp): add nika/workflowGraph custom request for DAG visualization
```

---

## Task B: InProcessDaemonProvider

### What
Run key daemon services (secrets, cache, cost estimation) directly in the LSP process.
No Unix socket, no separate daemon process. The LSP IS the daemon.

### Architecture

From the daemon agent research:
- **CacheService**: Clone (Arc<CacheInner> + DashMap). Can boot with `CacheService::new(storage)`.
- **SecretService**: NOT Clone. Wraps NikaVault. Can boot standalone.
- **JobService**: Clone (Arc). Needs SQLite storage + nika binary path. Skip for now (complex).
- **EventBus**: NOT Clone. broadcast::Sender. Easy to create.
- **Storage**: SQLite via nika-storage. Has in-memory option for tests.

### Scope (MVP)
For the embedded LSP, we only need:
1. **estimate_cost()** — ALREADY works without daemon (static KNOWN_PRICING table in nika-core)
2. **provider_status()** — Check env vars for API key presence (no vault needed for MVP)
3. **capabilities()** — Return synthetic capabilities (version, uptime)
4. **workflow_history()** — Return empty (no job tracking in embedded mode)

### Files
- **Create**: `tools/nika-daemon/src/provider_embedded.rs`
- **Modify**: `tools/nika-daemon/src/lib.rs` — add module
- **Modify**: `tools/nika-lsp/src/backend.rs` — add `--embedded-daemon` flag support

### Implementation

```rust
// provider_embedded.rs
pub struct EmbeddedDaemonProvider {
    started_at: Instant,
}

impl EmbeddedDaemonProvider {
    pub fn new() -> Self {
        Self { started_at: Instant::now() }
    }
}

#[async_trait]
impl DaemonProvider for EmbeddedDaemonProvider {
    fn is_connected(&self) -> bool { true } // Always connected — we ARE the daemon

    async fn provider_status(&self) -> Vec<ProviderStatusInfo> {
        // Check env vars directly (no vault in embedded mode)
        nika_core::catalogs::providers::all_providers()
            .map(|p| ProviderStatusInfo {
                id: p.id.to_string(),
                name: p.name.to_string(),
                has_key: std::env::var(p.env_var).is_ok(),
                source: if std::env::var(p.env_var).is_ok() { KeySource::Env } else { KeySource::NotFound },
                category: p.category,
                env_var: p.env_var.to_string(),
            })
            .collect()
    }

    async fn estimate_cost(&self, _prov: &str, model: &str, input: u64, output: u64) -> Option<CostEstimate> {
        nika_core::catalogs::estimate_cost(model, input, output)
    }

    async fn workflow_history(&self, _workflow: &str) -> Vec<WorkflowRunInfo> {
        Vec::new() // No job tracking in embedded mode
    }

    async fn capabilities(&self) -> Option<DaemonCapabilities> {
        Some(DaemonCapabilities {
            version: env!("CARGO_PKG_VERSION").into(),
            uptime_secs: self.started_at.elapsed().as_secs(),
            cache_entries: 0,
            cache_hit_rate: 0.0,
            active_jobs: 0,
            watch_active: false,
            total_cost_saved: 0.0,
        })
    }
}
```

### Backend Integration

In `backend.rs`, add `--embedded-daemon` flag:

```rust
pub fn new_embedded(client: Client) -> Self {
    // Same as new() but uses EmbeddedDaemonProvider
    let daemon: Arc<dyn DaemonProvider> = Arc::new(EmbeddedDaemonProvider::new());
    // ... rest of construction
}
```

In `main.rs`:
```rust
let embedded = std::env::args().any(|a| a == "--embedded-daemon")
    || std::env::var("NIKA_LSP_EMBEDDED_DAEMON").is_ok();

let builder = if embedded {
    LspService::build(|client| NikaBackend::new_embedded(client))
} else {
    LspService::build(NikaBackend::new)
};
```

### Extension Wire-Up

In `extension.ts`, pass `--embedded-daemon` by default:

```typescript
args: ['lsp', '--embedded-daemon', ...extraArgs],
```

Update status bar text from "Daemon $(check)" to "Nika: Ready".

### Verification
```bash
cd tools && cargo test --lib -p nika-daemon -- provider
cargo test -p nika-lsp
```

### Commits
```
feat(daemon): add EmbeddedDaemonProvider for in-process LSP mode
feat(lsp): support --embedded-daemon flag for single-process IDE
feat(vscode): use embedded daemon by default — single process, zero config
```

---

## Task C: MCP Prompts

### What
Add 3 prompt templates so AI editors (Cursor, Windsurf) can generate valid Nika workflows.

### rmcp API
The rmcp crate uses `ServerCapabilities::builder().enable_prompts()` and the
`ServerHandler` trait has `list_prompts()` and `get_prompt()` methods to override.

### Files
- **Modify**: `tools/nika-mcp/src/server.rs`

### Implementation

```rust
// In get_info():
capabilities: ServerCapabilities::builder()
    .enable_tools()
    .enable_prompts()
    .build(),

// Override list_prompts and get_prompt in ServerHandler impl
```

3 prompts:
1. **create-workflow**: `description` param → full workflow scaffold
2. **add-task**: `verb`, `description` params → single task YAML
3. **fix-error**: `code`, `file` params → corrected workflow

### Verification
```bash
cd tools && cargo test --lib -p nika-mcp
```

### Commit
```
feat(mcp): add 3 prompt templates for workflow generation
```

---

## Task D: Event Streaming (Engine → LSP → Webview)

### What
When a workflow runs via the embedded daemon, stream task events to the DAG webview
so nodes animate in real time: pending → running (blue pulse) → success/failed.

### Architecture
```
Engine EventLog ──emit──→ broadcast::Sender<Event>
                              │
                              └──→ LSP spawns background task
                                      │
                                  filters task events (Started/Completed/Failed/Skipped)
                                      │
                                  client.send_notification("nika/executionEvent", json)
                                      │
                                  Extension onNotification handler
                                      │
                                  dagPanel.updateTaskStatus() → webview postMessage
```

### Files
- **Modify**: `tools/nika-lsp/src/backend.rs` — subscribe to EventLog, emit notifications
- **Modify**: `editors/vscode/src/extension.ts` — forward events to webview

### Implementation

**backend.rs**: When using EmbeddedDaemonProvider, subscribe to event broadcasts:

```rust
// After creating the embedded provider, if it exposes an event_rx:
tokio::spawn(async move {
    while let Ok(event) = event_rx.recv().await {
        let notification = match &event.kind {
            EventKind::TaskStarted { task_id, .. } => Some(("running", task_id)),
            EventKind::TaskCompleted { task_id, .. } => Some(("success", task_id)),
            EventKind::TaskFailed { task_id, .. } => Some(("failed", task_id)),
            EventKind::TaskSkipped { task_id, .. } => Some(("skipped", task_id)),
            _ => None,
        };
        if let Some((status, task_id)) = notification {
            let _ = client.send_notification::<serde_json::Value>(
                "nika/executionEvent",
                serde_json::json!({ "taskId": task_id, "status": status }),
            );
        }
    }
});
```

**extension.ts**: Forward to webview:

```typescript
client.onNotification('nika/executionEvent', (event: { taskId: string; status: string }) => {
  dagPanel.updateTaskStatus(event.taskId, event.status as any);
});
```

### Note
Event streaming requires the engine to run IN the LSP process (EmbeddedDaemonProvider
with a real Engine instance). For MVP, we wire the plumbing but actual execution stays
via `nika run` subprocess. Full in-process execution is a future sprint.

### Verification
```bash
cd tools && cargo check -p nika-lsp
cd editors/vscode && npm run compile
```

### Commits
```
feat(lsp): wire event streaming for live DAG updates
feat(vscode): forward execution events to DAG webview
```

---

## Summary

| Task | Files | Time | Key Deliverable |
|------|-------|------|-----------------|
| A | backend.rs, main.rs | 1h | DAG panel gets real graph data from LSP |
| B | provider_embedded.rs, backend.rs, main.rs, extension.ts | 2h | Single process, zero daemon config |
| C | server.rs | 30m | AI editors can use prompt templates |
| D | backend.rs, extension.ts | 2h | Live DAG updates during execution |
| **Total** | | **~5.5h** | **Complete IDE experience** |

### Commit Convention
```
type(scope): description

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```
