# Handoff: Daemon ↔ LSP Bridge + UX Polish

**Date**: 2026-03-27
**For**: Next Claude session
**Context**: This session shipped 15 commits (parser NIKA-163, security fixes, CI unblock, e2e harness, validation parity). The daemon ↔ LSP bridge and UX polish remain.

## Current State

- **Branch**: main (clean)
- **Version**: 0.49.0
- **Tests**: 722 (nika-core) + 374 (nika-lsp-core) + 155 (nika-daemon) + 52 (nika-lsp) + 14 e2e (ignored)
- **CI release**: Fixed (secrets in if: conditions), not yet verified with a tag push
- **Extension**: v0.42.0 on marketplace (stale), local build installed

## What was done this session

1. NIKA-163 unknown workflow key detection (parser.rs) — `tsks:` → "did you mean tasks?"
2. Task-level unknown key detection WITH verb present — `dependson:` flagged
3. LSP crash fix (template_validation.rs .unwrap())
4. Security: SHA256 on extension download, ZIP size limit, root guard, key length limit
5. Validation parity: empty tasks warning (NIKA-145), provider key warning (NIKA-031)
6. E2e test harness: 14 JSON-RPC protocol tests
7. CI fix: secrets in job/step if: conditions
8. Auto-setup: daemon-driven post-install (install.sh, npm, brew)
9. Instant editor detection (quick_editor_scan bypass cooldown for new editors)
10. macOS app bundle CLI resolution (resolve_editor_cli)

## What remains — YOUR mission

### Priority 1: Daemon ↔ LSP Bridge

**Plan**: `docs/plans/2026-03-27-daemon-lsp-bridge-design.md`

#### Step 1: Extend daemon protocol (protocol.rs)
Add 4 new DaemonRequest variants:
- `ListProviderStatus` → Vec<ProviderInfo { name, has_key, source, category }>
- `EstimateCost { provider, model, input_tokens, output_tokens }` → CostEstimate { usd }
- `GetWorkflowRunHistory { workflow_path }` → Vec<RunInfo { status, duration, cost }>
- `GetDaemonCapabilities` → DaemonCaps { version, uptime, cache_entries, active_jobs }

Files: `tools/nika-daemon/src/protocol.rs`, `tools/nika-daemon/src/server.rs`

#### Step 2: Create LSP daemon bridge (daemon_bridge.rs)
New file: `tools/nika-lsp/src/daemon_bridge.rs` (~250 lines)
- `DaemonBridge::connect()` → Option<Self> (graceful if daemon not running)
- Provider status cache with 60s TTL
- All methods return Option<T> (never crash LSP)
- Reconnect with exponential backoff

Add to NikaBackend: `daemon: Option<DaemonBridge>` field

#### Step 3: Wire daemon data into handlers
- **Completions** (completion.rs): show `(✓ key)` / `(no key)` next to providers
- **Hover** (hover.rs): show workflow run history on `workflow:` line
- **Inlay hints** (inlay_hints.rs): replace static cost table with daemon live data
- **Code lens** (code_lens.rs): show last run status `✓ 2.3s, $0.004`
- **Diagnostics** (diagnostics.rs): warn on missing provider key (daemon-aware, not just env)

#### Step 4: Event subscription
Subscribe to daemon EventBus for:
- `WatchTriggered { path }` → revalidate changed .nika.yaml
- `JobStateChanged` → refresh code lens

### Priority 2: UX Polish

#### Rename support (NEW handler)
New file: `tools/nika-lsp-core/src/handlers/rename.rs`
- Reuse existing `references.rs` to find all task ID references
- `prepare_rename()` → verify cursor is on renameable identifier
- `rename()` → return TextEdit for all occurrences
Register: `rename_provider: Some(OneOf::Left(true))` in backend.rs capabilities

#### Last-valid-AST caching
In backend.rs, add to DocumentState:
- `last_valid_ast: Option<Arc<RawWorkflow>>`
- `last_valid_analyzed: Option<Arc<AnalyzedWorkflow>>`
- Update on successful parse, use as fallback on parse failure

#### Status bar (extension.ts)
- Create StatusBarItem: `🦋 Nika: LSP ✓ | Daemon ✓`
- Poll daemon status via custom LSP request `nika/daemonStatus`
- Click opens output channel

#### New snippets
Add to `editors/vscode/snippets/nika.code-snippets`:
- `artifact`, `limits`, `context`, `imports`, `content-vision`, `for-each-fetch`, `fan-out-fan-in`

#### Output channel logging
In extension.ts, add structured logging:
- Activation events, binary discovery, LSP start/fail, daemon connection

### Priority 3: Testing

- 6+ protocol tests for new daemon requests
- 4+ daemon bridge tests (connect, graceful failure, cache TTL, reconnect)
- 4+ e2e tests for daemon-powered features
- Test rename handler (6+ tests)

### Priority 4: CI Release

- Bump to v0.50.0 (or appropriate version)
- Tag + push → verify 7 platforms publish
- Manual verify: brew, npx, cargo install, VS Code marketplace, Open VSX, Docker, GitHub releases

## Key files

| File | What | Lines |
|------|------|-------|
| `tools/nika-daemon/src/protocol.rs` | IPC types, wire format | 845 |
| `tools/nika-daemon/src/client.rs` | Client API, ConnectedClient | 647 |
| `tools/nika-daemon/src/server.rs` | Request routing | ~400 |
| `tools/nika-lsp/src/backend.rs` | LSP server, NikaBackend struct | ~800 |
| `tools/nika-lsp/src/diagnostics.rs` | validate_document, 5 phases | ~350 |
| `tools/nika-lsp-core/src/handlers/completion.rs` | 60+ completions, 31 transforms | 1599 |
| `tools/nika-lsp-core/src/handlers/inlay_hints.rs` | 6 hint types, static cost table | 227 |
| `tools/nika-lsp-core/src/handlers/code_lens.rs` | Run/Validate/TaskCount | 89 |
| `tools/nika-lsp/tests/e2e_harness.rs` | JSON-RPC test client | 928 |
| `editors/vscode/src/extension.ts` | Extension, auto-download, commands | ~600 |

## Architecture constraints

- `nika-daemon` depends ONLY on `nika-core` (no circular deps)
- `nika-lsp` can depend on `nika-daemon` (client module only)
- LSP must work WITHOUT daemon (graceful degradation)
- All daemon queries are read-only (no auth needed)
- Unix socket at `~/.nika/daemon/nika.sock`
- Wire format: 4-byte BE length + JSON, max 16MB

## Testing commands

```bash
cargo test -p nika-core --lib                    # 374+ tests
cargo test -p nika-lsp-core --lib                # 722+ tests
cargo test -p nika-daemon --lib                  # 155+ tests
cargo test -p nika-lsp                           # 52+ tests
cargo test -p nika-lsp --test e2e_harness -- --ignored  # 14 e2e tests
cargo clippy --workspace -- -D warnings          # Zero warnings
cd editors/vscode && npm run compile             # Extension builds
```

## Bugs Found by Swarm Audit (FIX THESE FIRST)

### CRITICAL — Fix before daemon bridge work

1. **Cost pattern "o1" matches "o1-mini"** → 5x surcoût
   - File: `tools/nika-core/src/catalogs/cost.rs`
   - Pattern "o1" (15.0/60.0) matches "o1-mini" (should be 3.0/12.0)
   - Same issue: "gemini-2.5-flash" matches "gemini-1.5-flash" (2x)
   - Fix: use more specific patterns or ordered matching (longest first)

2. **Daemon secrets service only knows 7/19 providers**
   - File: `tools/nika-daemon/src/services/secrets.rs:18-26`
   - PROVIDERS array hardcodes only LLM providers
   - Missing: neo4j, github, slack, firecrawl, supadata, dataforseo, ahrefs, postgres, filesystem, memory, perplexity, native
   - Fix: expand PROVIDERS to include all 19 from providers.rs catalog

3. **ConnectedClient doesn't support streaming (EventSubscribe)**
   - File: `tools/nika-daemon/src/client.rs`
   - `request()` method expects request → response pairs
   - EventSubscribe keeps connection open for streaming
   - Fix: add `subscribe_events() -> EventStream` method to ConnectedClient
   - EventStream wraps the reader half, yields DaemonEvent

### HIGH — Fix during daemon bridge work

4. **KnownModel has no `provider` field** → can't filter models by provider
   - File: `tools/nika-core/src/catalogs/models.rs`
   - Fix: add `provider: &'static str` field, populate for all models

5. **Duplicate "Vision Content" snippet**
   - File: `editors/vscode/snippets/nika.code-snippets` lines 306-318
   - Fix: delete the duplicate (keep lines 214-227)

6. **E2e tests hardcode 300ms sleep** → flaky
   - File: `tools/nika-lsp/tests/e2e_harness.rs`
   - 8 tests use `sleep(300ms)` instead of waiting for publishDiagnostics
   - Fix: use `read_until()` to wait for diagnostics before requesting completion/hover

7. **3 handlers already accept daemon data but get None**
   - File: `tools/nika-lsp-core/src/handler.rs:118,122,157`
   - `completions(..., None)`, `hover(..., None)`, `code_lenses(..., None)`
   - These are READY — just need to pass real DaemonBridge data

### MEDIUM — Fix during UX polish

8. **Output channel created twice** → 2 "Nika Language Server" channels
   - File: `editors/vscode/src/extension.ts:466 + 543`
   - Fix: remove one, use the other consistently

9. **@ts-ignore for StatusBarAlignment**
   - File: `editors/vscode/src/extension.ts:548`
   - Fix: import StatusBarAlignment properly

10. **Cost fallback returns $0.00 silently** for unknown models
    - File: `tools/nika-core/src/catalogs/cost.rs`
    - Fix: return Option<f64> instead of 0.0, let callers decide

## Commit conventions

```
type(scope): description

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

Types: feat, fix, refactor, test, docs, chore, perf, security
Scopes: daemon, lsp, lsp-core, parser, cli, tui, extension
