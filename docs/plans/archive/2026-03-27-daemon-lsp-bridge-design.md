# Daemon ↔ LSP Bridge + UX Polish — Design Document

**Date**: 2026-03-27
**Status**: DESIGN — Ready for implementation
**Priority**: HIGH — Differentiator feature
**Estimated**: 5 sessions, ~15h total

## Vision

The daemon is Nika's background brain — secrets, cache, jobs, file watch.
The LSP is Nika's editor presence — completions, hover, diagnostics.
Today they don't talk. Connecting them creates the most intelligent
workflow editor on the market.

```
                    ┌─────────────────────────────────┐
                    │         VS Code / Cursor         │
                    │  ┌───────────────────────────┐   │
                    │  │   .nika.yaml editor       │   │
                    │  │   ├── completion: smart    │   │
                    │  │   ├── hover: live cost     │   │
                    │  │   ├── diagnostic: keys     │   │
                    │  │   ├── inlay: ~$0.03        │   │
                    │  │   ├── code lens: job ✓     │   │
                    │  │   └── status bar: 🦋 ✓     │   │
                    │  └───────────┬───────────────┘   │
                    └──────────────│────────────────────┘
                                   │ stdio (JSON-RPC)
                    ┌──────────────▼────────────────────┐
                    │         nika lsp (tower-lsp)       │
                    │  ┌─────────────────────────────┐   │
                    │  │   DaemonBridge (optional)    │   │
                    │  │   ├── provider status cache  │   │
                    │  │   ├── cost catalog live      │   │
                    │  │   ├── event subscriber       │   │
                    │  │   └── 60s TTL local cache    │   │
                    │  └─────────────┬───────────────┘   │
                    └────────────────│────────────────────┘
                                     │ Unix socket (~/.nika/daemon/nika.sock)
                    ┌────────────────▼────────────────────┐
                    │         nika daemon (tokio)          │
                    │  ├── SecretService (19 providers)    │
                    │  ├── CacheService (1024 entries)     │
                    │  ├── JobService (SQLite + cron)      │
                    │  ├── WatchService (fsevents/inotify) │
                    │  └── EventBus (streaming)            │
                    └─────────────────────────────────────┘
```

---

## Part 1: Daemon Protocol Extension

### Task 1.1: Add LSP query requests to protocol.rs

**File**: `nika-daemon/src/protocol.rs`

Add 4 new request variants to `DaemonRequest`:

```rust
/// LSP queries — read-only, no auth required
ListProviderStatus,
// → Vec<ProviderInfo { name, has_key, source: Env|Keychain|None, category: Llm|Mcp|Local }>

EstimateCost { provider: String, model: String, input_tokens: u64, output_tokens: u64 },
// → CostEstimate { usd: f64, input_rate: f64, output_rate: f64 }

GetWorkflowRunHistory { workflow_path: String },
// → Vec<RunInfo { job_id, status, duration_secs, tokens_used, cost_usd, timestamp }>

GetDaemonCapabilities,
// → DaemonCaps { version, uptime_secs, cache_entries, active_jobs, watch_active }
```

Add corresponding `DaemonResponse` variants.

**Why these 4**: They cover the exact data the LSP needs without adding complexity.
No MCP tool discovery yet (future — requires rmcp introspection).

### Task 1.2: Implement daemon handlers

**File**: `nika-daemon/src/server.rs`

Route the 4 new requests:

1. `ListProviderStatus` → iterate 19 known providers, call `secret_service.has(provider)` for each, tag category (Llm/Mcp/Local)
2. `EstimateCost` → use `nika_engine::provider::cost::estimate_cost()` (already exists)
3. `GetWorkflowRunHistory` → query `storage.list_jobs_for_workflow(path)` (add new storage method)
4. `GetDaemonCapabilities` → aggregate from all services

### Task 1.3: Add storage method for workflow history

**File**: `nika-daemon/src/storage.rs`

```sql
SELECT id, status, started_at, completed_at,
       json_extract(output, '$.tokens_used') as tokens,
       json_extract(output, '$.cost_usd') as cost
FROM jobs
WHERE workflow_path = ?
ORDER BY created_at DESC
LIMIT 10
```

### Task 1.4: Tests for new protocol

6 tests:
- Serialize/deserialize each new request type
- Round-trip test for ProviderInfo struct
- Cost estimate matches static catalog
- Empty history returns empty vec

---

## Part 2: LSP Daemon Bridge

### Task 2.1: Create daemon_bridge.rs

**File**: `nika-lsp/src/daemon_bridge.rs` (NEW, ~250 lines)

```rust
use nika_daemon::{DaemonClient, DaemonRequest, DaemonResponse};
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct DaemonBridge {
    client: Option<ConnectedClient>,
    /// Cached provider status (refreshed every 60s)
    providers: Arc<RwLock<Vec<ProviderInfo>>>,
    /// Last refresh timestamp
    last_refresh: Arc<RwLock<Instant>>,
}

impl DaemonBridge {
    /// Try to connect. Returns None if daemon not running.
    pub async fn connect() -> Option<Self> {
        let socket = nika_daemon::daemon_socket_path();
        match DaemonClient::connect(&socket).await {
            Ok(client) => {
                let bridge = Self {
                    client: Some(client),
                    providers: Arc::new(RwLock::new(Vec::new())),
                    last_refresh: Arc::new(RwLock::new(Instant::now())),
                };
                bridge.refresh_providers().await;
                Some(bridge)
            }
            Err(_) => None,
        }
    }

    /// Get provider status (from cache, refreshes every 60s)
    pub async fn provider_status(&self) -> Vec<ProviderInfo> { ... }

    /// Estimate cost for a model + token count
    pub async fn estimate_cost(&self, provider: &str, model: &str,
                                input: u64, output: u64) -> Option<f64> { ... }

    /// Get last N runs for a workflow file
    pub async fn workflow_history(&self, path: &str) -> Vec<RunInfo> { ... }

    /// Check if daemon is connected
    pub fn is_connected(&self) -> bool { self.client.is_some() }
}
```

**Key design decisions:**
- `Option<ConnectedClient>` — LSP works without daemon (graceful degradation)
- `RwLock<Vec<ProviderInfo>>` — cached, refreshed every 60s
- All methods return `Option<T>` — never block/crash LSP if daemon down
- Reconnect on error with exponential backoff (1s, 2s, 4s, max 30s)

### Task 2.2: Integrate bridge into NikaBackend

**File**: `nika-lsp/src/backend.rs`

```rust
pub struct NikaBackend {
    client: Client,
    documents: DashMap<String, DocumentState>,
    validation_tx: mpsc::Sender<ValidationRequest>,
    daemon: Option<DaemonBridge>,  // ← NEW
}

impl NikaBackend {
    pub fn new(client: Client) -> Self {
        // Spawn daemon connection attempt in background
        let daemon = tokio::spawn(async { DaemonBridge::connect().await });
        // ... rest of init
    }
}
```

Pass `&self.daemon` to handler functions that need it.

### Task 2.3: Event subscription for live updates

**File**: `nika-lsp/src/daemon_bridge.rs`

```rust
impl DaemonBridge {
    /// Subscribe to daemon events. Spawns background task.
    pub async fn subscribe_events(&self, tx: mpsc::Sender<DaemonEvent>) {
        if let Some(ref client) = self.client {
            let stream = client.subscribe().await;
            tokio::spawn(async move {
                while let Some(event) = stream.next().await {
                    match event {
                        DaemonEvent::WatchTriggered { path } => {
                            tx.send(DaemonEvent::WatchTriggered { path }).await.ok();
                        }
                        DaemonEvent::JobStateChanged { .. } => {
                            tx.send(event).await.ok();
                        }
                        _ => {} // Ignore cache/secret events
                    }
                }
            });
        }
    }
}
```

LSP listens for:
- `WatchTriggered` → revalidate the changed file
- `JobStateChanged` → refresh code lens for that workflow

---

## Part 3: Smart Completions (Daemon-Powered)

### Task 3.1: Filter providers by key availability

**File**: `nika-lsp-core/src/handlers/completion.rs`

Currently: suggests ALL 8 providers regardless of key status.
After: suggestions show key status as detail text.

```
provider: |
  ├── anthropic     (API key configured ✓)
  ├── openai        (API key configured ✓)
  ├── mistral       (no API key — set MISTRAL_API_KEY)
  ├── groq          (no API key)
  ├── deepseek      (no API key)
  ├── gemini        (no API key)
  ├── xai           (no API key)
  ├── native        (local — no key needed)
  └── mock          (testing — no key needed)
```

Implementation: pass `Option<&[ProviderInfo]>` from daemon bridge to completion handler.
If daemon not connected → show all providers without status (current behavior).

### Task 3.2: Filter models by provider

Currently: suggests ALL models from all providers.
After: only suggest models for the configured provider.

```yaml
provider: anthropic
model: |  # Only shows:
  ├── claude-opus-4-20250514      ($15/$75 per 1M tokens)
  ├── claude-sonnet-4-20250514    ($3/$15 per 1M tokens)
  └── claude-haiku-4-5            ($0.80/$4 per 1M tokens)
```

### Task 3.3: Cost in completion detail

Each model completion item shows cost in the `detail` field:
```
claude-sonnet-4-20250514    Anthropic · $3/$15 per 1M tokens
```

This already partially exists in inlay hints — reuse the cost lookup.

---

## Part 4: Live Diagnostics (Daemon-Powered)

### Task 4.1: Provider key warning with daemon data

**File**: `nika-lsp/src/diagnostics.rs`

Current (env var only): checks `ANTHROPIC_API_KEY` etc.
After (daemon + env): also checks daemon keychain via `ListProviderStatus`.

```rust
// In validate_document(), after parse:
if let Some(ref daemon) = daemon_bridge {
    let providers = daemon.provider_status().await;
    if let Some(provider_name) = &raw_workflow.provider {
        let info = providers.iter().find(|p| p.name == provider_name.value);
        match info {
            Some(p) if !p.has_key => {
                // Warning: no key in env OR keychain
                diagnostics.push(warning("NIKA-031", ...));
            }
            None => {
                // Warning: unknown provider
                diagnostics.push(warning("NIKA-032", ...));
            }
            _ => {} // Key available, no warning
        }
    }
}
```

### Task 4.2: Workflow history in hover

When hovering over `workflow:` name, show last 3 runs:

```
## my-workflow

Last runs:
  ✓ 12s ago — 2.3s, 1,240 tokens, ~$0.004
  ✗ 1h ago  — failed after 8.1s (NIKA-031: missing key)
  ✓ 3h ago  — 1.9s, 890 tokens, ~$0.003
```

Data source: `GetWorkflowRunHistory` from daemon.

---

## Part 5: Inlay Hints (Daemon-Powered)

### Task 5.1: Live cost estimates

Currently: static cost table hardcoded in inlay_hints.rs.
After: daemon provides live pricing + actual cost from last run.

```yaml
  - id: analyze
    infer:                          # ~$0.02 (estimated) | $0.018 (last run)
      prompt: "Analyze this text"
      model: claude-sonnet-4-20250514    # Anthropic · $3/$15
      max_tokens: 1000                   # ≈ 1K output tokens
```

### Task 5.2: Cache hit indicator

When a task has been run before and the result is cached:

```yaml
  - id: research                    # ⚡ cached (saved $0.03)
    infer: "Research {{inputs.topic}}"
```

Data source: `CacheGet` with prompt hash.

---

## Part 6: Code Lens (Daemon-Powered)

### Task 6.1: Job status in code lens

Currently: "▶ Run Workflow" and "✓ Validate" buttons.
After: also show last run status.

```
▶ Run  ✓ Validate  ✓ Last: 2.3s, $0.004 (12s ago)
```

Or on failure:
```
▶ Run  ✓ Validate  ✗ Last: FAILED — NIKA-031 (1h ago)
```

### Task 6.2: Task-level run button

Each task gets its own "▶ Run from here" lens:

```yaml
  - id: step1          ▶ Run from step1
    infer: "..."

  - id: step2          ▶ Run from step2 (skips step1)
    depends_on: [step1]
```

Command: `nika run workflow.nika.yaml --from step2`

---

## Part 7: Status Bar (Extension)

### Task 7.1: Status bar item

**File**: `editors/vscode/src/extension.ts`

```
🦋 Nika: LSP ✓ | Daemon ✓ | 3 providers | 2 jobs running
```

States:
- `🦋 Nika: LSP ✓ | Daemon ✓` — everything connected
- `🦋 Nika: LSP ✓ | Daemon ✗` — LSP works, daemon not running
- `🦋 Nika: LSP ✗` — LSP failed to start (show binary missing message)

Click → opens output channel "Nika Language Server".

### Task 7.2: Daemon status in status bar

Poll daemon every 30s via LSP custom request:
```typescript
// Extension sends custom request to LSP
const status = await client.sendRequest('nika/daemonStatus');
// { connected: true, providers: 3, activeJobs: 2, cacheHitRate: 0.87 }
```

LSP handles this as a custom method:
```rust
// In backend.rs
async fn handle_custom_request(&self, method: &str, params: Value) -> Result<Value> {
    match method {
        "nika/daemonStatus" => {
            if let Some(ref daemon) = self.daemon {
                let caps = daemon.capabilities().await;
                Ok(json!({ "connected": true, ...caps }))
            } else {
                Ok(json!({ "connected": false }))
            }
        }
        _ => Err(...)
    }
}
```

---

## Part 8: UX Polish

### Task 8.1: Rename support (task ID refactoring)

**File**: `nika-lsp-core/src/handlers/rename.rs` (NEW)

Leverage existing `references.rs` — it already finds all references to a task ID.

```rust
pub fn prepare_rename(text: &str, offset: usize) -> Option<RenameRange> {
    // Check if cursor is on a task ID (in `- id: xxx` or `$xxx` or depends_on)
    // Return the range of the identifier
}

pub fn rename(text: &str, offset: usize, new_name: &str) -> Vec<TextEdit> {
    // Find all references using existing references handler
    // Return TextEdit for each occurrence
    // Covers: id definition, depends_on refs, with $refs, template {{$refs}}
}
```

Register in backend.rs:
```rust
rename_provider: Some(OneOf::Left(true)),
```

### Task 8.2: Last-valid-AST caching

**File**: `nika-lsp/src/backend.rs`

When user is typing (broken YAML), hover/goto-def stop working.
Cache last successful parse for fallback:

```rust
pub struct DocumentState {
    rope: Rope,
    version: i32,
    last_valid_ast: Option<Arc<RawWorkflow>>,  // ← NEW
    last_valid_analyzed: Option<Arc<AnalyzedWorkflow>>,  // ← NEW
}
```

On successful parse → update cache.
On failed parse → use cached AST for hover/goto-def/completion.
On document close → clear cache.

### Task 8.3: Improved snippets

**File**: `editors/vscode/snippets/nika.code-snippets`

Add missing snippets:
- `artifact` → artifact block with path, format, mode
- `limits` → limits block with max_cost_usd, max_duration_secs
- `context` → context files block
- `imports` → imports block with path, prefix
- `content-vision` → content array with image + text parts
- `for-each-fetch` → complete fetch fan-out pattern
- `fan-out-fan-in` → full parallel processing pipeline (3 tasks)

### Task 8.4: Output channel logging

**File**: `editors/vscode/src/extension.ts`

Add structured logging to the output channel:

```typescript
const output = window.createOutputChannel('Nika Language Server');

function log(level: string, msg: string) {
    output.appendLine(`[${new Date().toISOString()}] [${level}] ${msg}`);
}

// On activation:
log('INFO', `Nika extension v${version} activating`);
log('INFO', `Binary: ${nikaPath}`);
log('INFO', `Platform: ${process.platform}/${process.arch}`);

// On LSP start:
log('INFO', 'Language server started');

// On error:
log('ERROR', `LSP failed: ${err.message}`);
```

---

## Part 9: CI Verification

### Task 9.1: Tag v0.50.0

```bash
# Bump workspace version
sed -i '' 's/version = "0.49.0"/version = "0.50.0"/' tools/Cargo.toml
cargo check --workspace  # Verify

# Update CHANGELOG
# ... add v0.50.0 section

# Commit, tag, push
git add -A && git commit -m "chore: bump to v0.50.0"
git tag v0.50.0
git push origin main --tags
```

### Task 9.2: Monitor CI release pipeline

Watch all 9 jobs:
1. `preflight` — version coherence ✓
2. `build` — 7 targets compile ✓
3. `docker-publish` — ghcr.io + Docker Hub ✓
4. `release` — GitHub release created ✓
5. `vscode-publish` — marketplace + Open VSX ✓
6. `npm-publish` — @supernovae/nika ✓
7. `crates-publish` — 10 crates ✓
8. `update-homebrew` — formula bump ✓
9. `post-release-smoke-test` — all platforms visible ✓

### Task 9.3: Manual verification

After CI completes:
- `brew install supernovae-st/tap/nika && nika --version` → 0.50.0
- `npx @supernovae/nika --version` → 0.50.0
- VS Code: Extensions → search "Nika" → version 0.50.0
- Cursor: same
- `cargo install nika && nika --version` → 0.50.0
- `docker run --rm ghcr.io/supernovae-st/nika:0.50.0 --version` → 0.50.0

---

## Execution Schedule

```
Session A — CI RELEASE (1h):
  Task 9.1: Bump version + tag v0.50.0
  Task 9.2: Monitor CI pipeline
  Task 9.3: Manual verification on all 7 platforms

Session B — DAEMON PROTOCOL (2h):
  Task 1.1: Add 4 new request types to protocol.rs
  Task 1.2: Implement daemon handlers
  Task 1.3: Add storage method for workflow history
  Task 1.4: Protocol tests (6)

Session C — LSP BRIDGE + SMART FEATURES (3h):
  Task 2.1: Create daemon_bridge.rs
  Task 2.2: Integrate into NikaBackend
  Task 2.3: Event subscription
  Task 3.1: Provider-filtered completions
  Task 3.2: Model-filtered completions
  Task 3.3: Cost in completion detail
  Task 4.1: Provider key warning with daemon
  Task 4.2: Workflow history in hover

Session D — INLAY + CODE LENS + STATUS BAR (3h):
  Task 5.1: Live cost estimates
  Task 5.2: Cache hit indicator
  Task 6.1: Job status in code lens
  Task 6.2: Task-level run button
  Task 7.1: Status bar item
  Task 7.2: Daemon status polling

Session E — UX POLISH (2h):
  Task 8.1: Rename support
  Task 8.2: Last-valid-AST caching
  Task 8.3: Improved snippets (7 new)
  Task 8.4: Output channel logging
```

## Success Criteria

- [ ] v0.50.0 published on all 7 platforms
- [ ] `provider: anthropic` without key → warning squiggly WITH daemon check
- [ ] Completion shows only models for selected provider
- [ ] Model completion shows cost: `claude-sonnet-4 · $3/$15`
- [ ] Hover on `workflow:` shows last 3 run results
- [ ] Inlay hint shows `~$0.02` estimated cost
- [ ] Code lens shows `✓ Last: 2.3s` or `✗ FAILED`
- [ ] Status bar: `🦋 Nika: LSP ✓ | Daemon ✓ | 3 providers`
- [ ] Rename task ID → all references updated
- [ ] Broken YAML → hover still works (AST cache fallback)
- [ ] 7 new snippets available
- [ ] Output channel shows structured logs
