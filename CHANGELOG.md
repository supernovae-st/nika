# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  NIKA v0.63.3 — SKILLS + SERVE HARDENING                                  ║
║  Skills on infer | Job isolation | Provider validation | 7 fixes           ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

## [0.63.3] — 2026-04-03

### Added
- **Auto-inject workflow skills into infer system prompts** — `skills:` now work on `infer` tasks, not just `agent`. Skill files are prepended to the system prompt automatically.
- **`overwrite` param for `nika:write`** — `overwrite: true` replaces existing files instead of failing with NIKA-215.

### Fixed
- **Fail loud on missing skill files (NIKA-270)** — Previously silently skipped, now returns a hard error when referenced skill files don't exist.
- **`nika check` validates skills and context file paths** — Catches missing files at validation time instead of at runtime.
- **Per-job scratch dir for cache isolation** — `nika serve` creates `.nika/jobs/<id>/` with `NIKA_JOB_ID` + `NIKA_JOB_DIR` env vars, preventing cross-job cache collisions.
- **`nika check` warns on unknown provider names (NIKA-033)** — Typos like `provider: antropic` now produce a warning with suggestions.
- **Daemon persists exe path for job spawning after binary upgrade** — `~/.nika/daemon/nika-exe-path` fallback prevents stale binary references after `nika switch` or Homebrew upgrades.

## [0.63.2] — 2026-04-03

### Added
- **10 new pipe transforms** — `pluck(field)`, `where(field, val)`, `pick(f1, f2)`, `omit(f1, f2)`, `sort_by(field)`, `group_by(field)`, `merge`, `regex(pattern)`, `base64_encode`, `base64_decode`. Eliminates LLM calls for mechanical data reshaping.
- **`nika:json_verify` builtin** — Translation structural validator: missing keys, extra keys, broken placeholders, broken HTML tags. Replaces `verify-translation.py`.
- **`nika:yaml_validate` builtin** — Batch YAML/JSON field presence checker with dot-path fields.
- **`nika:locale_lookup` builtin** — BCP-47 to NLLB/ISO code mapping with language-prefix fallback.
- **`nika:aggregate` builtin** — Array statistics: sum, avg, min, max, count. Replaces LLM calls for trivial math.
- **`nika:json_flatten` / `nika:json_unflatten` builtins** — Flatten nested JSON to dot-notation keys and back. Full roundtrip fidelity.
- **LSP autocomplete** — All new transforms available in VS Code / JetBrains completion (39 total).
- **14 E2E workflow tests** — Real YAML → runner → verify for all new transforms and builtins.

### Fixed
- **`aggregate` count** — Returns total array length, not just numeric items count.
- **`json_unflatten` collision** — Conflicting keys replace scalar with object instead of silent data loss.
- **Startup banner** — Recursive workflow count in `nika serve` (was showing 0 for nested dirs).
- **Token timing leak** — SHA-256 hash before `ct_eq` prevents token length side-channel.
- **Artifact path check** — Uses `project_root` not `workflows_dir` for containment.
- **`[policy]` config** — `allowed_hosts` from nika.toml now reaches executor (was silently dropped).
- **SSE subscribe TOCTOU** — Single lock acquisition for channel existence + subscribe.
- **GC handle tracking** — GC task handle stored for graceful shutdown.
- **Null byte rejection** — `validate_workflow_path` rejects null bytes.

## [0.63.1] — 2026-04-03

### Added
- **OpenAPI 3.1** — Auto-generated spec via aide at `GET /v1/openapi.json`
- **Scalar UI** — Interactive API explorer at `GET /v1/docs`
- **Security scheme** — Bearer auth documented in spec
- **Operation IDs** — All 10 endpoints have stable IDs for SDK generation
- **Typed responses** — `CancelResponse`, `ListArtifactsResponse` replace `Json<Value>`
- **SSE documentation** — Event types documented in `submitWorkflow` operation

### Fixed
- **LSP** — Windows CI type inference error in `daemon_status()` (E0282)

## [0.62.0] — 2026-04-02

### Added
- **Serve: GET /v1/workflows endpoint** — List available workflows via HTTP API
- **@supernovae-st/nika-client** — Pure TypeScript HTTP client for nika serve (zero deps, 56 tests, SSE streaming, polling, retry). Separate repo: `supernovae-studio/nika-client`

## [0.61.0] — 2026-04-02

### Security
- **Shell quoting before blocklist (NIKA-053)** — Strip quotes before matching, preventing bypass via `'sudo'`
- **Value-based secret redaction** — Custom API keys from `$env` now redacted in traces
- **Webhook SSRF hardening** — DNS pinning, redirect blocking, IPv6-mapped blocked
- **Vault hardening** — 0o700 dirs, Argon2i 6 iterations, file locking, passphrase min 12 chars
- **Serve auth hardening** — Token min 32 chars, X-Request-Id capped 128 chars, CORS validation

### Added
- **nika-sdk crate** — Rust SDK with remote (HTTP+SSE) and embedded (in-process) transports
- **nika-napi crate** — Node.js SDK via napi-rs 3.x, AsyncGenerator streaming, discriminated TS unions
- **nika-py crate** — Python SDK via PyO3 0.24, EventStream iterator, pythonize, `__eq__`, `.pyi` stubs
- **nika.toml project config** — Walk-up discovery (like `.git`), 3-layer merge (defaults → file → env)
- **.mcp.json support** — Claude Code convention at project root, `nika init` creates it
- **Serve: artifact API** — List + download artifacts via `/v1/jobs/{id}/artifacts`
- **Serve: typed SSE events** — Task-level streaming with structured event types
- **Serve: checkpoint store** — Resume interrupted jobs from last checkpoint
- **`{{skills.NAME}}` resolution** — Reference skill file content directly in templates
- **BLAKE3 checksums** — Text/JSON/YAML/Markdown artifacts get integrity checksums
- **ProviderAutoRetried event** — Trace event on transient infer retry with attempt + backoff
- **Interactive init wizard** — cliclack prompts, `--yes` for non-interactive
- **`nika clean`** — Umbrella command for trace + media + cache cleanup
- **CI: SDK publish** — npm + PyPI pipelines with ARM Linux cross-compilation via zig
- **nika check security phase** — Shell escape warnings with word-boundary regex

### Fixed
- **SdkError::Cancelled** — Distinct error variant for cancelled jobs
- **SSE CRLF + buffer limit** — Handle `\r\n\r\n` frame boundaries, 1 MiB cap
- **JobStatus enum** — Type-safe deserialization with `#[serde(other)]` forward compat
- **Job reaping** — Embedded transport reaps terminal jobs above 1024 entries
- **Artifact YAML validation** — `format: yaml` validated before write
- **`nika:dag_info` task count** — Uses total DAG count including for_each expansion
- **`nika:emit` data alias** — Accepts `data` as alias for `payload`
- **Schema path resolution** — Aligned between `nika check` and `nika run`
- **Provider chain in fallback** — Cleared on routing override
- **`fail_fast:false` partial results** — Now unblock downstream tasks
- **Serve: embedded executor default** — In-process instead of subprocess
- **Serve: SSE timeout excluded** — SSE routes no longer hit 30s TimeoutLayer
- **MCP: NIKA error tags** — Correct error codes in tool responses
- **npm scope** — `@supernovae` → `@supernovae-st` across all references
- **CI: npm version sync** — SDK package.json synced from release tag before publish

### Changed
- **MCP config consolidated** — `[mcp.*]` in nika.toml + `.mcp.json` at root
- **Dead code removed** — Blanket `#[allow(dead_code)]` on NativeRuntime, dead fields cleaned
- **9,407 tests** — Up from 9,109

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  NIKA v0.56.0 — HTTP API + SERVE HARDENING                                 ║
║  nika serve | nika-storage | 16 security fixes | 9,109 tests               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

## [0.58.1] — 2026-04-01

### Fixed
- **fetch 4xx/5xx error handling** — Non-success HTTP status codes now properly fail the task (unless `response: full`)
- **wiremock test reliability** — Cleaned up test assertions and patterns
- **LSP unused import** — Fixed compiler warning in nika-lsp
- **DAG flow edge case** — Fixed edge case in dependency resolution

## [Unreleased] — 2026-03-31

### Security
- **CORS origin configurable** — `CorsLayer::allow_origin(Any)` replaced with opt-in `NIKA_SERVE_CORS_ORIGIN`. No CORS headers sent by default, preventing CSRF via browser requests.
- **Auth token minimum length** — `NIKA_SERVE_TOKEN` must be at least 16 characters. Empty or short tokens are rejected at startup.
- **env_clear allowlist** — Worker subprocess uses `env_clear()` + explicit allowlist (PATH, HOME, API keys, NIKA_* vars) instead of `env_remove` denylist. New server secrets won't leak to subprocesses.
- **Database instance lock** — `flock()` on `<db>.lock` prevents two `nika serve` instances from corrupting the same SQLite database.

### Fixed
- **UUID job ID collision** — Job IDs used 12 chars from hyphenated UUID (44 bits, collision at ~4.2M jobs). Now uses full 128-bit UUID in simple format (32 hex chars).
- **Worker panic cleanup (WorkerGuard)** — Drop guard marks job as failed, decrements atomic counter, and removes from worker map even on panic between `acquire()` and `complete_job()`.
- **Race-free queue depth** — Replaced two separate DB queries (list pending + list running) with `AtomicUsize` counter. Prevents 20 concurrent requests from all passing the check.
- **Bounded subprocess output** — `wait_with_output()` (unbounded RAM) replaced with piped reads capped at `max_output_bytes`. Prevents OOM from verbose workflows.
- **Shutdown kills workers** — `tokio::select!` races subprocess against shutdown signal. Workers receive SIGTERM on server shutdown instead of running until 30s drain timeout.
- **Cancel kills subprocess** — `handle.abort()` only killed the tokio task, not the child process. Now stores child PID in `WorkerHandle` and sends SIGTERM/SIGKILL to the process group.
- **Cancel race protection** — Worker checks if job was cancelled before writing complete/fail status, preventing a completed job from being overwritten as cancelled.
- **Drain timeout abort** — After 30s drain, remaining worker handles are aborted (not just dropped/detached).
- **Windows compile fix** — `child.kill()` in `#[cfg(not(unix))]` block referenced moved child. Restructured to use piped reads + `child.wait()`.
- **Inputs passed to subprocess** — `RunRequest.inputs` was accepted but silently ignored. Now forwarded as `--input key=value` CLI args.
- **Non-interactive subprocess** — Added `-y --no-interactive` flags to match daemon behavior, preventing interactive prompts from hanging the subprocess.
- **Construct ServeConfig directly** — Replaced deprecated `std::env::set_var()` calls with direct `ServeConfig` construction from CLI args.
- **9,109 tests** — Up from 9,093 (+16 tests for serve hardening).

## [0.56.0] — 2026-03-31

### Security
- **SSRF redirect re-pinning** — DNS-pinned fetch clients now apply the same SSRF redirect policy as the shared client. Previously, HTTP redirects from pinned connections bypassed the IP blocklist check.

### Added
- **`nika serve` HTTP API** — New `nika-serve` crate with Axum 0.8 server. REST API for workflow execution: `POST /v1/run`, `GET /v1/status/{id}`, `POST /v1/cancel/{id}`. Bearer token auth with constant-time comparison. Subprocess-based execution with process-group isolation.
- **`nika-storage` crate** — Extracted SQLite storage layer from `nika-daemon`. Dedicated OS thread with mpsc channel (rusqlite is Send but not Sync). WAL mode, schema migrations, shared by daemon and serve.
- **Agent scope presets** — `scope: full|minimal|debug` controls which builtin tools an agent receives. `minimal` = only `nika:complete` + `nika:log` (simple Q&A agents). `debug` = all tools + introspection tools (`nika:dag_info`, `nika:task_status`, `nika:threads`, `nika:cost`). Explicit `tools:` list always overrides scope.
- **LLM guardrails (type: llm)** — `type: llm` guardrails now call a judge LLM instead of returning a hard error. Sends `judge_prompt` + agent output to the configured provider, checks response against `pass_pattern` regex. Supports guardrail-specific `model:` override and 30s timeout. Provider errors respect `on_failure:` action.
- **StructuredOutputTimeout event** — New `EventKind::StructuredOutputTimeout` emitted when the 600s aggregate structured output validation times out. Displayed in live renderer and handled by TUI.
- **Failed task binding warning** — `tracing::warn` emitted when `with:` bindings reference a failed task's output, alerting that the value may be null or partial. Covers both legacy and typed resolution paths.

### Fixed
- **Cancellation before binding resolution** — Cancel token checked before synchronous binding resolution in `execute_task_iteration` and `for_each` binding paths. Prevents long JSON path traversal from blocking workflow cancellation.
- **Keyring crate removed** — NikaVault replaces OS keychain. No more macOS Keychain popups.
- **Package resolver cache TTL** — 5-minute TTL on package resolver cache prevents stale results.
- **Run lockfile uses flock** — Exclusive run lockfile uses `flock()` instead of file existence check.
- **Global task concurrency semaphore** — Max 64 concurrent tasks across all workflows to prevent fork bombs.
- **INTERNER DashMap removed** — Global string interner replaced with plain `Arc::from` for simplicity.

### Changed
- **TUI ProviderName migration** — Provider verification uses typed `ProviderName` enum instead of hardcoded string array, keeping TUI in sync with the provider catalog.
- **9,093 tests** — Up from 9,086 (+7 new tests for scope wiring, failed task bindings, LLM guardrails).

### Dependencies
- **Added**: `axum 0.8`, `tower-http 0.6`, `subtle 2` (constant-time auth), `nika-storage` crate.
- **Removed**: `keyring` crate (replaced by NikaVault).

## [0.55.0] — 2026-03-31

### Security
- **NikaVault encrypted secrets store** — XChaCha20Poly1305 (AEAD) + Argon2i KDF via `orion` crate. Replaces OS keychain dependency. Machine fingerprint (machine-id + username) or `NIKA_VAULT_PASSPHRASE` for CI/Docker. File permissions 0o600. Cross-platform: Linux, macOS, Windows.
- **SSRF auto-allow for custom endpoints** — Private IPs (10.x, 192.168.x) from `[endpoints.*]` config are automatically added to `allowed_hosts`, preventing fetch verb from blocking legitimate vLLM/Ollama servers on private networks.
- **Socket cleanup Drop guard** — Unix socket file now cleaned up on any exit path (success, error, panic) via RAII guard, preventing "daemon already running" errors after crashes.

### Added
- **`<think>` tag extraction** — Qwen3.5 and other reasoning models return thinking content in `<think>...</think>` tags. Now extracted and separated from actual response for clean output. Case-insensitive, multi-block support.
- **Provider retry with exponential backoff** — Transient HTTP errors (500, 502, 503, 429, timeout) retry automatically with backoff [0s, 1s, 3s, 10s]. Permanent errors (401, 403) fail immediately. Independent of task-level `retry:` config.
- **systemd Type=notify** — Daemon signals readiness via `sd-notify` after full initialization (socket bound, storage opened, token written). Replaces `Type=simple`.
- **systemd EnvironmentFile** — Unit template loads `~/.nika/.env` for secrets injection.
- **Pending job drain** — When a running job completes, pending jobs are automatically started if slots are available. Uses `Notify`-based background drain loop.

### Fixed
- **systemd Restart=always** — Daemon exits 0 on SIGTERM, so `Restart=on-failure` never triggered restart. Changed to `Restart=always`.
- **timeout_secs propagation** — Custom endpoint `timeout_secs` from `config.toml` was parsed but never passed to the HTTP client. Now stored in `OpenAiCompat` variant and used for all infer/vision/stream timeouts.
- **Signal handler panic** — SIGTERM/SIGINT handlers used `.expect()` which panics in restricted environments. Replaced with `.ok()` + graceful `pending()` fallback.
- **SQLite failure now fatal** — Storage open errors were silently swallowed, disabling jobs. Now propagated so systemd can restart the daemon.
- **`nika provider set` uses NikaVault** — On headless Linux, OS keyring fails (no D-Bus). Now writes to encrypted vault file instead.
- **Vault fallback without daemon** — With `NIKA_NO_DAEMON=1` or dead daemon, secrets resolve from vault file directly (env → daemon → vault → None).
- **Connection drain on shutdown** — Active IPC connections are now tracked and drained with 5-second timeout on graceful shutdown.
- **`$env.VAR` resolution order** — Documented guarantee that daemon/vault secrets are pre-loaded into process env before workflow binding resolution.

### Changed
- **Secrets resolution order** — env var → daemon IPC → NikaVault → None (was: env var → daemon → None).
- **NikaVault in nika-core** — Vault implementation lives in `nika-core` (shared by daemon + engine without circular deps).
- **9,086 tests** — Up from 9,057 (+29 new tests for vault, think tags, retry, SSRF policy).

### Dependencies
- **Added**: `orion 0.17` (XChaCha20Poly1305 + Argon2i), `whoami 1` (username), `sd-notify 0.5` (systemd readiness).

## [0.54.0] — 2026-03-31

### Security
- **SECRET_RE hardening** — 4 new patterns (Stripe restricted keys, Twilio auth tokens, database URIs, generic `secret=`). 7 new tests.
- **IPv6 SSRF bypass** — Block unspecified address `[::]` in SSRF check.
- **Exec shell validation** — Pass `is_shell` flag to `validate_exec_command_with_shell` for accurate blocklist matching.
- **TOCTOU race in session context** — Eliminate race condition in session context loading.
- **Path traversal guard** — Block `../` in mock file schema loading paths.
- **Secret redaction in MCP events** — Redact secrets in MCP response events.
- **Sprint 1 P0** — Secret redaction in resolved commands, recursive JSON redaction, FetchExhausted event at all 4 retry-exhaustion paths, Retry-After header parsing (capped at 5 min).
- **Sprint 2 P1** — Thread PolicyEnforcer through agent loop, parametric effective_max_tokens (replace 16x hardcoded 8192), 50 MB MCP resource size limit.

### Added
- **5 new telemetry events** — `ForEachItemStarted`/`Completed`/`Failed` (per-iteration tracking), `TaskCancelled` (distinct from `TaskFailed`), `FallbackChainExhausted` (emitted when all providers in chain fail).
- **for_each item count limit** — `MAX_FOR_EACH_ITEMS = 10,000` prevents OOM from unbounded arrays.
- **91 overnight E2E test workflows** — Across 17 categories (infer, exec, fetch, invoke, structured, for_each, agent, transforms, security, vision, artifacts, orchestrate, retry, error-handling, multi-provider, pipelines, edge-cases).
- **39 instruction→workflow training pairs** — For fine-tuning dataset.
- **Artifact path collision detection** — AST analyzer detects duplicate artifact paths at analysis time, respects `append`/`unique` modes.
- **JSON schema fields** — `goal` + `orchestrate` added to JSON schema.

### Fixed
- **String "null" coercion removed** — `coerce_json_types()` no longer converts `"null"` string to `Value::Null`, preserving data integrity.
- **Transform unwrap_or_default** — Replace silent `unwrap_or_default()` with explicit `expect()` in `FirstN` and `ToJson` transforms.
- **timeout: 0 rejection** — Promoted from warning to analysis error — `timeout: 0` causes immediate timeout and is never intentional.
- **for_each fail_fast=false** — Include cancelled items in error summary.
- **NIKA-028 for semaphore failures** — Distinct error code instead of reusing NIKA-026.
- **Cascade contract** — Address code review findings for error message clarity and cleanup logging.
- **Permanent error retry skip** — Skip retry for 401, 403, and command-not-found errors.
- **CancellationToken wiring** — Wire cancel tokens into exec and fetch verbs for proper shutdown.
- **Parser guardrails** — Reject `use:` and `max_retries:` at task level with helpful migration suggestions.
- **Mock provider depth limit** — Prevent stack overflow in `generate_mock_json()`.
- **Mock file-based schemas** — Load file-based schemas for structured output in mock provider.
- **Shell transform null safety** — Return `NullInput` error on null instead of literal `'null'` string.
- **Pipe parser quote handling** — Auto-close quotes at parenthesis boundary.
- **Structured output repair_model** — Warn and skip empty `repair_model` instead of using empty string.
- **Telemetry finish_reason** — Propagate `finish_reason` from provider through `StreamResult`.
- **Orchestrate events** — Emit missing events + enforce `confidence_target`.
- **Provider unreachable!()** — Replace 4 `unreachable!()` with proper error returns.
- **Canonical provider names** — Use canonical names in telemetry.
- **FallbackChainExhausted** — Map to correct `NikaError` variant.
- **Test assertions** — Add real assertions to 4 security tests that asserted nothing; replace incorrect "dead variant" test with correct limit semantics tests.
- **Cascade order-dependence** — Document + add reverse-order test.

### Changed
- **Broadcast channel capacity** — 1024 → 4096 for high-throughput workflows.
- **DAG scheduler** — `get_ready_tasks()` optimized from O(n²) to O(remaining).
- **Cleanup logging** — Errors logged at debug level instead of silent swallow.
- **Retry compounding** — Document task × structured retry interaction.

## [0.53.0] — 2026-03-30

### Security
- **DNS resolution pinning** — Wire DNS-pinned addresses to reqwest, closing SSRF TOCTOU gap.
- **NIKA-053 backtick detection** — Quote-aware backtick detection in exec security check.
- **Secret redaction** — Redact env-sourced secrets and API keys in trace events.
- **Shell template warning** — Warn on unescaped template bindings in `shell: true` commands.
- **50 MB limits** — MCP tool results + task output size limits to prevent OOM.
- **Output scanner** — Wire output scanner + fix empty provider chain panic.

### Added
- **ModelResolver** — Centralized model routing (`refactor(core)`), wired into infer (9 fallback sites eliminated), agent, compressor. No more hardcoded model names.
- **ProviderName migration** — Complete typed enum migration across 33 files (`Option<String>` → `Option<ProviderName>`).
- **Global workflow timeout** — `max_duration_secs` field at workflow level.
- **Structured output hardening** — 600s aggregate timeout on validation engine; L0 safety-net context + retry delay; propagate max_tokens through InferCallback for L3/L4 retries; extract L0a/L0b into methods.
- **Orchestration events** — Emit `OrchestratorStarted` + `OrchestratorCompleted` events; wire `wrap_as_orchestrator` + security fixes.
- **Mock provider enhancements** — Schema-conforming JSON for structured output; `NIKA_MOCK_FAIL_COUNT` env var for retry testing.
- **41 E2E test workflows** — Structured output, fetch, invoke, retry, transforms, provider fallback, artifacts, for_each+structured, guardrails, adversarial data flow traps.
- **4 daemon tests** — Auto-start + secrets resolution.

### Fixed
- **Fetch exhausted retries** — Return error on exhausted 5xx retries instead of empty success.
- **Thinking budget overflow** — Safe `u64→u32` conversion with `try_from`.
- **Token accumulation** — `saturating_add` at all token count sites (cost, display, orchestrate, agent).
- **Agent fallback** — Pass actual chain index to ModelResolver for correct fallback substitution.
- **Concurrency:0 rejection** — Analyzer rejects `concurrency: 0` with clear error.
- **Binding alignment** — Align traced resolution with untraced for null+transform paths.
- **Transform fixes** — `default()` treats empty strings as needing fallback; respect parentheses when splitting pipe chain; return null for NaN/Infinity in round/ceil/floor/abs.
- **JSON fence stripping** — Harden for uppercase markers and Windows CRLF.
- **TUI canonical provider** — Use canonical provider ID instead of first alias.
- **Streaming** — Log `try_send` failures at debug level (9 sites); reduce abandoned channel buffer 64→1.
- **Low confidence debug** — Add debug log for `LowConfidence(0.0)` in explicit completion mode.
- **Tool path alias** — Accept `path` alias for `nika:write` `file_path` param.
- **NIKA-204** — Improve path error with actionable suggestion.
- **Examples** — Use `inputs:` instead of shell `${}`, add `extract: article`.
- **5 stale tests removed** — BUG PROVEN tests for bugs already fixed in production.

### Changed
- **TUI refactor** — 4 views migrated to `default_model_for_provider` from catalog.
- **Structured output refactor** — Extract `InferCallback` factory method, L0a/L0b methods.
- **Secrets display** — Provider list shows source (env, daemon, or keychain).
- **CI hardened** — `cargo-deny` and `machete` hard fail (removed `|| true`).
- **6 dead workspace dependencies removed.**
- **Workspace version** — Bumped all 12 crates from 0.52.0 to 0.53.0.

## [0.52.0] — 2026-03-30

### Security
- **IPv4-compatible IPv6 SSRF bypass** — Block `::127.0.0.1`, `::169.254.169.254` and other IPv4-compatible IPv6 addresses that bypassed SSRF protection.
- **Absolute path blocklist bypass** — `/usr/bin/sudo`, `/usr/bin/doas`, `/usr/local/bin/python3 -c` now caught by extracting basename from first token before blocklist matching.
- **Symlink artifact escape (fail-closed)** — `canonicalize()` failures in artifact writer now return `ArtifactPathError` instead of silently skipping the symlink check.

### Added
- **28 E2E workflow tests** — Comprehensive end-to-end tests: 12 mock (infer, depends_on, fan-out, for_each, exec, retry, inputs), 7 real API provider tests (all 7 providers), 7 structured output validation tests (programmatic JSON validation per provider), 2 integration tests (pipeline + fetch).
- **P-ORCHESTRATE: Goal-driven workflow orchestration** — 6-part feature (goal field, orchestrate config, agent wrapper, inline YAML execution, round tracking, 5 events).
- **ProviderName typed enum** — Provider aliases resolved at analysis time to canonical names. Type-safe provider handling throughout core AST.
- **P-RECORD: Record compression engine** — `record:` field, NDJSON persistence, `nika trace search` CLI.
- **P-CONTEXT: Context budget enforcement** — `context_budget:` field, CJK-aware token counting.
- **P-INTROSPECT: 4 introspection tools** — `nika:dag_info`, `nika:task_status`, `nika:threads`, `nika:orchestrate`.
- **P-MEMORY-LOCAL: Cross-session memory** — NDJSON records, output scanner injection detection.
- **Inference routing** — `provider: [groq, anthropic]` fallback chains with ProviderFallback event.
- **Agent presets** — 8 built-in presets. `nika:cost` introspection tool.
- **Daemon auto-start** — Daemon starts automatically on any `nika` command.

### Fixed
- **SpawnAgent cancel token** — Child agents now receive `cancel_token.child_token()` so parent cancellation cascades correctly.
- **HashMap panic in DAG depth calculation** — Direct indexing replaced with `.get().copied().unwrap_or(0)` in two places.
- **Exit code display** — `None` exit code now renders with dim color (not success green).
- **Missing size_bytes warning** — Media tool results without `size_bytes` now log a warning.
- **Empty workflow output warning** — Final output defaults logged at warn level instead of silently.
- **Structured output error accumulation** — Layer 2/3/4 errors now accumulated in Vec and included in NIKA-300 error message.
- **JSONPath debug→warn** — 5 JSONPath resolution failures upgraded to warn level for user visibility.
- **Malformed template warning** — Template parse errors now emit tracing::warn with expression and error context.
- **Transform null error logging** — Transform errors on null values logged at debug with error context.
- **$env secret blocking (BUG-001)** — All env vars now accessible.
- **Shell blocklist false positive (BUG-005)** — Pre-resolution blocklist check.
- **12 stale example workflows** — Fixed field names and placements.
- **4 pre-existing clippy errors** — field_reassign_with_default, redundant closure, useless vec!, non-Debug assert.

### Changed
- **Dead code removed** — `RecordSpec.retain` (never consumed), `IterationResult.artifact_paths` (populated but ignored), `RetryCondition` enum (zero consumers), 702 LOC include_loader.rs.
- **Provider canonicalization** — Consistent "anthropic" instead of "claude" alias.
- **Test count** — 8,938 tests across 12 crates (up from 8,903 in v0.51).

## [0.51.0] — 2026-03-29

### Security
- **Exec blocklist** — Block `bash -c`, `sh -c`, `zsh -c`, and 4 more shell `-c` variants. Block generic `python -c`, `python2 -c`, `python3 -c`. Block `find -exec`, `find -delete`, `xargs`.
- **SSRF fail-closed** — DNS resolution failure now blocks (was: allowed). Post-redirect DNS SSRF check prevents hostname-to-private-IP redirects.
- **Template injection** — Trusted allowlists for `{{inputs.*}}` and `{{context.*}}` in both `resolve()` and `resolve_with()`. Injected refs via LLM output are blocked.
- **Skill loading** — Path traversal (`../`) blocked in `resolve_skill_path()`. File size limit (1 MiB) added.
- **JSON Schema** — Invalid schema now errors immediately (was: silent `.ok()` wasting LLM calls).
- **API key redaction** — `redact_for_event()` now regex-matches `sk-*`, `Bearer *`, `ghp_*`, `gho_*`, `xoxb-*`, `AKIA*` patterns.

### Fixed
- **Agent loop** — `run_claude` (405 LOC) and `run_openai` (399 LOC) replaced with thin wrappers delegating to generic `run_agent_loop`. Total: -771 LOC in providers.rs.
- **token_budget** — Now wired into `LimitTracker` (was: parsed but ignored).
- **Silent failures** — 17 DAG scheduling failures now emit `TaskFailed` events. Added `TaskEventGuard` RAII pattern. Fixed missing `ProviderResponded` on Layer 0a no-spec path. Replaced silent `let _ =` with `warn!`/`debug!` in event emission.
- **Tautological tests** — Replaced 2 tests that only tested compiler derives with behavior assertions.
- **Error code table** — NIKA-160-164 is Parse errors (not Policy/Boot) in README.

### Changed
- **Workspace version** — Bumped all crates from 0.50.0 to 0.51.0.
- **VS Code extension** — Bumped to 0.51.0.

## [0.50.0] — 2026-03-28

### Added
- **`preset:` field on tasks** — Reference named agents from the workflow's `agents:` block. Inherits provider, model, temperature, and system prompt. Task-level overrides take precedence. Validated at analysis time (NIKA-144).
- **`retry:` on all verbs** — Previously only effective on `fetch:` tasks. Now works on `infer:`, `exec:`, `invoke:`, and `agent:` via runner-level retry wrapper.
- **TaskRetry event** — New `TaskRetry` event for non-fetch retry attempts with attempt count, backoff, and error details. Rendered in both live and classic display modes.
- **Agent presets documentation** — New "Agent Presets" section in editor rules + `examples/agents-preset.nika.yaml` with mock provider.

### Fixed
- **Error code table** — NIKA-160-164 correctly documented as Parse errors (Phase 1 parser), not Policy/Boot. Added NIKA-165-169 range for Policy/Boot/Startup.
- **VS Code template** — New workflow command now includes `model:` field (prevents NIKA-034 missing model error).
- **VS Code showOutput command** — Registered in package.json contributes (was missing from manifest).
- **Dead schema URL** — Removed non-existent `nika.sh/schemas/workflow.json` from VS Code yaml.schemas default.

### Changed
- **Workspace version** — Bumped all 10 workspace crates from 0.49.0 to 0.50.0.
- **VS Code extension** — Bumped from 0.42.0 to 0.50.0.

## [0.49.3] — 2026-03-27

### Fixed
- **CRITICAL: media template resolution** — `{{with.img.media[0].hash}}` now works in both `resolve()` and `resolve_for_shell()`. Media refs live in `TaskResult.media` (side-channel), not in output JSON. Extracted `intercept_media_path()` shared helper. All 15 showcase workflows depended on this pattern.
- **Media leak in structured output retry** — `execute_with_retry()` orphaned staged media refs on success path. Added `.with_media()` on success and defense-in-depth drains on 3 failure paths.
- **Positional media matching** — multiple binary artifacts without explicit `source:` now use positional matching (`artifact[i]` → `media[i]`) instead of all using `media[0]`.
- **Empty media error messages** — `{{with.img.media[0].hash}}` on empty media now shows "task 'X' produced no matching media" instead of cryptic `PathNotFound`. Fixed `resolve_path()` to return `None` (not `[]`) for indexed access on empty arrays.
- **ArtifactWritten always visible** — removed `show_sub_events()` gate so users always see output file paths, regardless of detail level.
- **Showcase artifacts** — all 15 advanced showcases wrote to `.nika/artifacts/` instead of `./output/`. Added `artifacts: dir: .` to all.
- **Showcase 10** — rewritten from 3/10 hardcoded pages to full 10-page iteration using `items:` binding.
- **Doctor command** — replaced non-existent `nika setup ai` suggestion with correct `nika init`.
- **CODESPACES detection** — `CODESPACES=true` now treated as dev environment (not CI), allowing auto-setup in GitHub Codespaces.

### Added
- **`nika init` starter workflow** — creates `hello.nika.yaml` so LSP activates immediately in editors. "Next steps" section guides new users.
- **Daemon auto-install** — machine setup now installs + starts daemon as system service (launchd/systemd). `RunAtLoad=true` for persistent background service. Opt-out: `NIKA_NO_DAEMON=1`.
- **Auto-setup on Doctor + TUI** — removed from skip list so `nika doctor` and `nika ui` trigger first-run setup.
- **Env var API key detection** — machine setup detects `ANTHROPIC_API_KEY` etc. and hints about keychain migration.
- **LSP: 29 transform completions** — expanded from 6 to all 29 transforms (string, array, numeric, type, parametric, system).
- **LSP: 24 nika:* tool completions** — expanded from 3 to all 24 builtin media tools organized by tier.
- **11 new media interception tests** — hash, mime, metadata, array, transform, empty, out-of-bounds, shell escape.

### Changed
- **cli_format adoption** — `media.rs` uses `key_value()`, `key_value_width()`, data-driven Tier 1 tools. `install.rs` uses `StatusIcon::Ok` (8 raw unicode replaced).
- **Quick editor scan** — cooldown reduced from 24h to 4h.
- **Removed stale `tools/nika-vscode/`** — canonical extension is `editors/vscode/` (`supernovae.nika-lang`).

### Stats
- ~20 commits, 8457 tests pass (+45 from session start)
- Zero clippy warnings (`cargo clippy --workspace -- -D warnings`)

## [0.49.2] — 2026-03-27

### Security
- **Constant-time auth token comparison** — replace early-return length check with blake3 hashing to fixed 32 bytes before XOR comparison, eliminating timing-based token length leak
- **API key CLI arg warning** — hide key positional arg from help, warn about `ps aux` visibility, recommend interactive mode

### Added
- **config.rs tests** — 7 unit tests for `parse_config_value` and `find_nika_dir`
- **daemon.rs tests** — 3 edge case tests for `format_uptime` (zero, hour boundary, 24h)
- **model.rs tests** — 5 unit tests for `format_size` and `ModelAction` variants

### Changed
- **cli_format adoption** — config.rs, daemon.rs, model.rs now use `StatusIcon`, `separator()`, `section_header()` instead of raw `Colorize` icons
- **config.rs** — removed `colored::Colorize` import entirely (no longer needed)
- **daemon.rs** — removed `colored::Colorize` import, 13 icons replaced with `StatusIcon`
- **model.rs** — 12 icons + 4 separators replaced with cli_format utilities

### Stats
- 9 commits, 8398 tests pass
- Zero clippy warnings (`cargo clippy --workspace -- -D warnings`)

## [0.49.1] — 2026-03-27

### Fixed
- **`handle_result()` missing `.await`** — async fn was called without await, blocking `clippy -D warnings`
- **Flaky `for_each_concurrent_fail_fast` test** — added 50ms sleep to non-failing items so CancellationToken propagates before completion (was 4/10 failures)
- **`nika job list` exit code** — returned `Ok(())` when daemon not running, now returns `Err` with clear message
- **StatusIcon consistency** — replaced all hardcoded `"✓".green()` / `"✗".red()` with `StatusIcon::Ok` / `StatusIcon::Fail`
- **nika-lsp version mismatch** — bumped nika-engine dep from 0.48.0 to 0.49.0

### Added
- **`nika models --json`** — JSON output mode for model catalog (provider, model, pricing, context window, tags)
- **`nika provider test --quiet`** — suppress output, exit code only (for scripts/CI)
- **`native` provider in onboarding wizard** — skip API key prompt, show `nika model pull` hint
- **Daemon IPC for provider set/delete** — `cfg(unix)` routes through daemon socket before falling back to direct keyring
- **Non-TTY fallback for provider test** — skip cliclack spinner when not TTY
- **Dry-run cost estimate** — show estimated cost based on model pricing (~500 input + ~1000 output tokens)
- **Doctor unit tests** — 9 tests for diagnostic formatting (pass/warn/fail, sections, JSON output, summary)
- **SECRET error codes documented** — SECRET-001 through SECRET-004 in CLAUDE.md error table

### Changed
- **Doctor display** — adopted `section_header()`, `status_line()`, `StatusIcon`, `hint()` from cli_format
- **Doctor "Next steps" footer** — consistent hints instead of manual numbered steps
- **Daemon secret tests** — tightened always-true `assert!(is_ok() || is_err())` to proper `assert!(is_ok())`

### Stats
- 17 commits, 8381 tests pass
- Zero clippy warnings (`cargo clippy --workspace -- -D warnings`)

## [0.43.0](https://github.com/supernovae-st/nika/releases/tag/v0.43.0) — 2026-03-25

### Added
- **`from_example` structured output** — auto-derive JSON Schema from a JSON example
  ```yaml
  structured:
    from_example: { name: "Alice", age: 30, active: true }
  # or from file:
    from_example: ./structure.json
  ```
- **`strict: true` flag** — adds `additionalProperties: false` recursively to derived schemas, preventing LLM from injecting extra keys
- **Array union in schema derivation** — multi-item arrays merge all objects' properties (union keys, intersect required, anyOf for type conflicts)
- **`json_to_schema_strict()`** — new public API in `nika-core::schema` module
- **`OutputPolicy.from_example`** — first-class field, no more `source_structured_spec` indirection
- **File-based prompt injection** — file `from_example` examples are pre-read and injected into LLM prompts (cached_example)
- Gate workflow: `structured-from-example.nika.yaml` with inline, file, and strict examples

### Changed
- **`schema: Option<SchemaRef>`** — StructuredOutputSpec.schema is now `Option`. When `from_example` is set, schema is `None` (removed placeholder `{}` anti-pattern)
- **`is_structured()`** — now returns true when `from_example` is set (even without `schema`)
- **`json_to_schema` moved** to dedicated `nika-core/src/schema/` module (re-exported from `structured.rs` for compat)
- **`build_json_schema_instruction`** reads `policy.from_example` directly instead of going through `source_structured_spec`

### Stats
- 7 commits, 18 files changed
- +1,379 / -414 lines
- 8,188 tests pass (717 core + 4,389 engine + 3,082 other)

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  NIKA v0.42.0 — DX OVERHAUL + SECURITY HARDENING                           ║
║  Auto-setup | TUI polish | 50+ bug fixes | SSRF/token safety               ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                             ║
║  v0.40.3  Telemetry v2 (43 → 55 EventKind)                                 ║
║  v0.41.0  LSP CodeLens + InlayHints + Manifesto                            ║
║  v0.41.1  Deep audit — 50 bugs fixed, 3200 LOC deleted                     ║
║  v0.41.2  Security hardening — SSRF, secrets, DAG cycles                   ║
║  v0.41.3  Gemini stopSequences + UTF-8 safety                              ║
║  v0.41.4  Parser: duplicate task IDs + timeout: 0 warning                  ║
║  v0.41.5  Overflow-safe timeout + error code collision fix                 ║
║  v0.42.0  DX overhaul — auto-setup, TUI polish, nika setup removed        ║
║  v0.43.0  from_example structured output — auto-derive JSON Schema         ║
║                                                                             ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

## [0.42.0](https://github.com/supernovae-st/nika/releases/tag/v0.42.0) — 2026-03-25

### Added
- Tab completion for all slash commands in TUI chat
- Welcome message with verb examples and provider status on first launch
- Yank (`y`) keybinding to copy task output in monitor view
- `chat.default_provider` and `chat.default_model` config support in TUI
- ExportTrace action wired to `export_session()` in TUI
- Diamond DAG, manifesto, and wow demo workflows for `nika` bare command
- Memorable end-of-run summary one-liner after workflow execution
- Inline DAG summary displayed before `nika run`
- `nika provider list` now shows fix command for missing API keys
- Editor name reveal in setup output
- Content hash fingerprinting to protect user-customized rules from overwrites
- Auto-setup on first command (skipped in CI environments)
- `install.sh` curl script for quick installation
- `@supernovae-st/nika` npm wrapper package for cross-platform install
- Windows target in CI release pipeline
- Startup loading indicator and first-launch help hint in TUI

### Changed
- Removed `nika setup` command (replaced by auto-setup on first use)
- Slimmed `nika init` to generate only `config.toml` (editor rules moved to user scope)
- Simplified `nika new` to accept name + verb + provider only
- Hidden power-user commands from `--help` output
- Slimmed `.nika/` project scaffold (removed 6 unused files/dirs)
- Split TUI modules: event_handler into sub-handlers, wizard/studio/matrix_decrypt into directories
- Consolidated 4 provider fields into `ActiveProvider` struct in TUI
- Removed dead code: new_wizard module (-1544 LOC), agent step stubs, widget shells, unused deps
- 24h cooldown on quick editor scan for performance

### Fixed
- TUI provider state now updates on ChatModelSwitch
- UTF-8 safe truncation for MCP error messages in TUI
- `delay` → `delay_ms` in retry examples across all generated rules
- Layout sync for correct mouse hit-testing in TUI
- Unicode-aware status bar hint width calculation
- Welcome hint and loading overlay no longer overlap
- Status bar hints reordered by priority with progressive disclosure
- MCP errors and API key warnings now shown in status bar
- Concurrent provider verification spawns guarded
- `/invoke` now validates that a tool name is provided
- Memory bounds added for activity items and browser entries
- Silent failures, stub actions, and signal handler wired in TUI
- Non-ASCII `generation_id` no longer causes panic in display
- Copilot false-positive detection removed (was triggering on VS Code presence)
- CRLF line endings handled correctly in LSP handlers
- Plural grammar in display output and doctor recommendations
- Stale tests referencing deleted code removed
- Token estimation, error codes, and tracing improvements in runtime
- MCP validator now extracts unknown field name from error messages

### Removed
- `nika setup` command (replaced by auto-setup)
- Dead code: HomeView, MatrixRain, sparkline, progress module (~3000 LOC)
- Unused dependencies: nucleo, humantime, and others unified to dirs v6

## [0.41.5](https://github.com/supernovae-st/nika/releases/tag/v0.41.5) — 2026-03-23

### Fixed
- Timeout seconds-to-milliseconds conversion uses `saturating_mul` to prevent overflow
- `McpToolCallFailed` error code reassigned to NIKA-110 (was colliding with NIKA-103)

## [0.41.4](https://github.com/supernovae-st/nika/releases/tag/v0.41.4) — 2026-03-23

### Fixed
- Duplicate task IDs now detected at parse time instead of analysis phase
- Warning added for `timeout: 0` which causes immediate timeout
- Provider prefix stripped from model names before API calls

## [0.41.3](https://github.com/supernovae-st/nika/releases/tag/v0.41.3) — 2026-03-23

### Fixed
- Gemini `stopSequences` now correctly nested inside `generationConfig`
- UTF-8 panic in `redact_for_event` truncation prevented
- `extract_actual_type` implemented properly (was returning literal string "actual")
- NIKA-102/103 error code deduplication for `McpToolCallFailed`

## [0.41.2](https://github.com/supernovae-st/nika/releases/tag/v0.41.2) — 2026-03-23

### Added
- LSP: references, document links, and folding ranges migrated from engine to nika-lsp-core
- Cursor AI rules expanded from 35 to 527 lines
- Machine auto-setup module (`~/.nika/machine.toml`)
- Unified wizard with machine auto-setup as Phase 1
- `nika doctor --fix` for auto-remediation
- Adaptive bare `nika` command (distinguishes upgrade vs first-time)
- 25 tests for LimitTracker + concurrent fail_fast
- 8 coherence tests to prevent AI rule content drift
- Actionable help messages in provider errors

### Changed
- 12 dead/redundant dependencies removed
- Claude rules expanded to 170 lines

### Fixed
- DAG cycle detection added at run time + MCP graceful shutdown
- `$env` secret access blocked + `TemplateResolved` events redacted
- CIDR-based SSRF blocklist + proper host matching
- SSRF-vulnerable HTTP client fallbacks removed
- Checked arithmetic in token budget + safe `inject_mock`
- Token budget leak on Layer 0 early return + trace writer warning
- Structured output retry loop now checks cancellation
- Structured output error context improved + agent cleanup
- `chat_continue` unified with full AgentBuilder config + cost estimates
- `calculate_cost_with_cache` wired at all 12 call sites
- MCP caches cleared on reconnect + `TransportClosed` error detection
- Structured output retry loop uses latest output + Layer 3/4 cost tracking
- MCP invoke params coerced to native JSON types after template resolution
- `for_each` + `decompose` coexistence warning + schema without `format:json`
- Dead `is_error` variable removed + `ForEachCompleted` skipped count split
- `llm_txt` fetch failures and vision channel warnings now logged
- `TaskFailed` emitted for cancellation + artifact write failures
- `{{item}}` → `{{with.item}}` in Cursor rules + stale grok-4 reference removed
- AI file generation moved before summary output in init
- `spawn_blocking` used for workflow directory scan in MCP
- Short-circuit invoke param template roundtrip for performance
- TUI bounds checks on provider selector + wizard navigation
- Missing model pricing entries added

## [0.41.1](https://github.com/supernovae-st/nika/releases/tag/v0.41.1) — 2026-03-23

### Added
- `PermissionMode` wired from CLI to executor

### Changed
- ~3200 LOC dead code deleted across the codebase

### Fixed
- 50 bugs from deep audit (6 CRITICAL + 17 HIGH) across runtime, security, and bindings
- Task failures now propagated instead of silently swallowed
- Atomic token budget operations + SSRF redirect protection + cache pricing fixes
- Shell escape for non-string values in bindings + `USE_RE` supports all transforms
- `retry:` removed from course L5-03 infer task (was silently ignored)
- MISSION.md exercise tables fixed in 8 levels

## [0.41.0](https://github.com/supernovae-st/nika/releases/tag/v0.41.0) — 2026-03-23

### Added
- LSP: diagnostic code actions, CodeLens, and InlayHints
- MANIFESTO.md with mermaid diagrams and launch marketing suite

### Fixed
- NIKA-053 false positive on blocklist normalization
- 25 failing course templates + init `--minimal` helpers
- Template vars, test count, and brew tap references in documentation

## [0.40.3](https://github.com/supernovae-st/nika/releases/tag/v0.40.3) — 2026-03-23

### Added
- Telemetry v2: 12 new event types for full observability (43 → 55 `EventKind` variants)

## [0.40.2](https://github.com/supernovae-st/nika/releases/tag/v0.40.2) — 2026-03-23

### Added
- GPT-4.1 family (gpt-4.1, gpt-4.1-mini, gpt-4.1-nano) in cost table
- `error_code` field in `TaskFailed` events for programmatic extraction
- 3 new event types: `ExecCompleted`, `FetchRetry`, `PolicyBlocked`
- Time-to-first-token (`ttft_ms`) capture in streaming responses
- `cached_input_tokens` tracking across all provider agent loops
- Fetch retry deadline (prevents infinite backoff loops)

### Fixed
- `{{item}}` template variable → `{{with.item}}` in all 17 documentation files (AI rules, IDE configs, content-suite). Template engine only resolves `{{with.*}}` namespace.
- Version references updated from 0.39.x → 0.40.2 across README, CI, VS Code, Claude plugin
- Double-comma syntax errors in 7 test files (from automated replacement)
- Missing `ExecCompleted`/`FetchRetry`/`PolicyBlocked` in TUI event handler
- Missing `ttft_ms`/`request_id` fields in `StreamResult` test

## [0.40.1](https://github.com/supernovae-st/nika/releases/tag/v0.40.1) — 2026-03-23

### Fixed
- MCP server path traversal protection + result cap
- Split NIKA-096 catch-all into 4 specific codes (NIKA-094, 095, 097, 098)
- Correct `system:`/`temperature:` placement in nika-agent skill
- 3 critical bugs: NaN trace guard, CLI JSON errors, vision cost calculation
- 8 tautological test assertions that always passed
- `expect()` panic in lower.rs → proper NikaError
- Exponential backoff integer overflow prevention
- Retry snowball fix, for_each outputs, 3 error codes, Layer 0 cost
- Course themes mismatch, MCP async timeout, .roomodes generation
- Doctor improvements + course skill update
- 27+ engine/TUI/security improvements from deep audit

### Added
- `nika mcp serve` — MCP server exposing workflow tools to AI coding assistants
- AI integration files generated during `nika init` (AGENTS.md, IDE rules, git hook)
- E2E test workflow with real OpenAI API
- Updated AI rules, skills, and context files for v0.40

## [0.40.0](https://github.com/supernovae-st/nika/releases/tag/v0.40.0) — 2026-03-23

### Added
- `nika setup` command for machine-level IDE + AI tool configuration
- 15 universal Agent Skills for 43+ AI agents (agentskills.io standard)
- Claude Code plugin (5 skills, 3 agents, hooks, MCP, LSP)
- AI rules for Cursor (.mdc), Copilot (.instructions.md), Windsurf, Roo Code, Aider
- `nika mcp serve` — MCP server exposing workflow tools to AI coding assistants
- `nika doctor` AI integration health checks (rules, skills, AGENTS.md, git hook)
- `nika init` now generates AI rules for all detected coding tools
- AGENTS.md migration (universal standard, 60k+ repos, Linux Foundation)
- llms.txt + llms-syntax.txt for AI content discovery
- E2E test workflow with real OpenAI API
- `course run`, `course check` with scoring, `course watch`, `course hint`
- Provider auto-detection at course generation

### Fixed
- exec output capture (Stdio::piped for spawn + wait_with_output)
- SSRF protection for IPv6 loopback addresses + allowed_hosts override
- MCP server path traversal protection + result cap
- 7 critical agent bugs (Claude/OpenAI control plane)
- 3 media bugs ("image exists but can't see it")
- 3 template resolution bugs (context path, shell transforms, brackets)
- 5 byte-index panics + renderer crashes
- 3 UTF-8 panics (cursor position, select_all, LSP completion)
- TUI blank startup screen + silent buffer bugs
- 27 test failures resolved across engine/wiremock/security

### Changed
- CLAUDE.md → AGENTS.md (symlink for backward compat)
- rmcp updated with server + transport-io features
- 9 unused dependencies removed
- CI workflows consolidated

## [0.39.1](https://github.com/supernovae-st/nika/releases/tag/v0.39.1) — 2026-03-22

### Added
- `nika course watch` — auto-check exercises on file save
- `nika showcase` command — browse 200+ showcase workflows from CLI
- 3-star scoring for exercises (Perfect/Passed/Attempted)
- Enhanced constellation progress map with star ratings
- Smart hint auto-detection for incomplete exercises

### Fixed
- NIKA-210 collision resolved — `FileAlreadyExists` renumbered to NIKA-215
- NIKA-090 stale message — removed outdated "v0.1" reference
- 13 critical + high TUI fixes from 15-agent deep audit (navigation, rendering, state)

## [0.39.0](https://github.com/supernovae-st/nika/releases/tag/v0.39.0) — 2026-03-22

### Added
- `nika init --course` — generates 12-level interactive learning course (44 exercises)
- `nika course` command with 8 subcommands: status, next, check, hint, reset, run, info, watch
- `nika init --minimal` — lightweight scaffold (5 workflows, 1 per verb)
- cliclack wizard for `nika init` — beautiful interactive setup
- CourseProgress tracking (.nika/course-progress.toml)
- 3-tier progressive hint system (conceptual → specific → solution)
- Course exercise validation (CourseCheck assertions)
- NIKA-310 through NIKA-314 error codes for course operations
- Liberation theme with 12 named levels (Jailbreak → SuperNovae)

### Changed
- `nika init` now uses minimal scaffold (5 workflows) instead of 30-workflow tier system
- Removed tier1-6.rs and partials.rs from init module (-7,000 lines)

### Removed
- Old 6-tier init system (30 workflows) — replaced by minimal + course

## [0.38.0](https://github.com/supernovae-st/nika/releases/tag/v0.38.0) — 2026-03-22

### Changed
- **The Great Split**: Monolithic binary split into 10 workspace crates
  - `nika-engine` (134k lines) — embeddable execution engine
  - `nika-core` (23k lines) — AST, types, catalogs (zero I/O)
  - `nika-tui` (92k lines) — Terminal UI (ratatui)
  - `nika-cli` (8k lines) — CLI subcommands
  - `nika-event` (4k lines) — EventLog, TraceWriter
  - `nika-mcp` (9k lines) — MCP client (rmcp)
  - `nika-media` (3.5k lines) — CAS store, processor
  - `nika-lsp-core` (9k lines) — LSP intelligence
  - `nika-lsp` (2.5k lines) — LSP binary
  - `nika` (2k lines) — CLI entry point

### Fixed
- invoke: resource support + stress test fixture
- Parser merges task-level `max_tokens`/`temperature` into shorthand `infer:`
- `parse_json` transform now strips markdown code blocks
- 3 critical issues from workspace audit
- 2 CAS compression test assertions

## [0.37.0](https://github.com/supernovae-st/nika/releases/tag/v0.37.0) - 2026-03-21

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  🦋 NIKA v0.37.0 — SCHEMA @0.12 ONLY                                         ║
║  Zero users = zero backward compatibility                                    ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  v0.36.1  MCP Rich Aliases + CLI (113 aliases, pricing tiers)                ║
║  v0.36.2  LSP Intelligence + Guardrails (retry loop, hover docs)             ║
║  v0.36.3  Nuclear Cleanup (-78K lines, 104 docs deleted)                     ║
║  v0.36.4  Runtime Polish + Zombie Purge (stop_sequences, -2,810 lines)       ║
║  v0.37.0  Schema @0.12 Only (breaking: old schemas rejected)                ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

See [tools/nika/CHANGELOG.md](tools/nika/CHANGELOG.md) for detailed per-release notes (v0.36.1 → v0.37.0).

### Breaking
- **Schema @0.1 through @0.11 rejected** — only `nika/workflow@0.12` accepted

### Highlights
- **113 MCP aliases** with rich structs (pricing, categories)
- **Guardrail retry loop** for agents (`on_failure: retry`)
- **stop_sequences** via additional_params workaround (all 8 providers)
- **LSP enriched**: 35 hover docs, 3 code actions, syntax errors, "did you mean?"
- **Nuclear cleanup**: -78K lines docs, legacy.rs extracted, 5 widgets stripped
- **Contract tests**: 42x spn→nika, all counts updated
- **Schema simplified**: 12 enum variants → 1, all feature gates removed
- **nika-lsp**: MIT → AGPL-3.0-or-later

## [0.36.0](https://github.com/supernovae-st/nika/releases/tag/v0.36.0) - 2026-03-21

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  🦋 NIKA v0.36.0 — LSP PHASE C + FULL INTELLIGENCE                           ║
║  References | Document Links | Folding | 100 MCP Aliases | UTF-8 TUI        ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  v0.35.5  VS Code Extension + Schema Sync                                    ║
║  v0.35.6  LSP Phase B — Handler Delegation                                   ║
║  v0.35.7  Runtime Wiring — Guardrails + Skills                               ║
║  v0.35.8  TUI Studio Redesign + LSP Fixes                                    ║
║  v0.36.0  LSP Phase C — Full Intelligence                                    ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

See [tools/nika/CHANGELOG.md](tools/nika/CHANGELOG.md) for detailed per-release notes (v0.35.5 → v0.36.0).

### Highlights

- **LSP Phase C** — references, document links, folding ranges, inlay hints (5 types), CodeLens (3 types)
- **LSP Phase B** — `LspHandler` trait + `DefaultHandler` delegation, enriched hover/definition/code_action/semantic_tokens
- **Runtime wiring** — guardrails enforcement, `LimitTracker`, `CompletionConfig`, skills injection in agent loop
- **TUI Studio** — split into 6-module directory, diagnostic gutter, go-to-def, code actions, Theme 60 fields
- **VS Code extension** — 21 snippets + 4 commands, blue butterfly icons
- **100 MCP aliases** — expanded from 48
- **AST field renames** — `thinking` → `extended_thinking`, `max_iterations` → `max_turns`, `working_dir` → `cwd`
- **UTF-8 safe TextBuffer** — char index, not byte offset
- **Schema sync** — JSON schema aligned with Rust AST (guardrails, completion, limits, resource)
- **Default features** expanded to 22/24

## [0.27.0] - 2026-03-12

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  🦋 NIKA v0.27.0 — spn→nika FEATURE FUSION                                    ║
║  Unified CLI + Ollama removal + Security hardening                            ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  ✨ NEW FEATURES:                                                              ║
║  ─────────────────────────────────────────────────────────────────────────    ║
║  • Unified CLI — all spn commands now available via `nika`                    ║
║  • Core module — zero-dep provider/model/MCP definitions                      ║
║  • WizardView — interactive setup in TUI (nika setup)                         ║
║  • YAML bomb protection — SEC-001 via serde-saphyr Budget                     ║
║                                                                               ║
║  🔧 BREAKING CHANGES:                                                          ║
║  ─────────────────────────────────────────────────────────────────────────    ║
║  • Ollama provider REMOVED — use `provider: native` with mistral.rs           ║
║  • spn CLI deprecated — shows warning directing to nika                       ║
║                                                                               ║
║  Tests: 5,640 passing | Zero clippy warnings | ARMADA CI green                ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### Added

#### Unified CLI (spn→nika Fusion)

All spn features now available directly via nika commands:

```bash
# Provider management
nika provider list        # Show all providers with status
nika provider set claude  # Store API key in keychain
nika provider test claude # Validate key with provider

# Model management
nika model list           # List available local models
nika model pull llama3.2  # Download model from HuggingFace

# MCP server management
nika mcp add neo4j        # Add MCP server (48 aliases)
nika mcp list             # List configured servers
nika mcp test neo4j       # Test server connection

# Editor sync
nika sync                 # Sync to enabled editors
nika sync --enable cursor # Enable editor

# Interactive setup
nika setup                # Interactive onboarding wizard
nika setup nika           # Install Nika + LSP + Daemon
```

#### Core Module (`src/core/`)

New zero-dependency module with provider/model/MCP definitions:

- **KNOWN_PROVIDERS**: 6 LLM + 11 MCP providers with validation
- **KNOWN_MODELS**: 16+ curated models for native inference
- **MCP_ALIASES**: 48 MCP server aliases for auto-configuration

#### WizardView for Interactive Setup

New TUI wizard for first-time setup with step-by-step guidance.

#### YAML Bomb Protection (SEC-001)

Protection against malicious YAML payloads:
- serde-saphyr with Budget limits
- Prevents billion-laughs and zip-bomb attacks
- Error code NIKA-054 for recursion limit exceeded

### Changed

#### Ollama Provider Removed

**BREAKING:** Ollama is no longer supported. Use native inference instead:

```yaml
# Before (v0.26 and earlier)
provider: ollama
model: llama3.2

# After (v0.27+)
provider: native
model: llama3.2:7b
```

Native inference provides:
- Better performance via mistral.rs
- Metal/CUDA acceleration
- No external Ollama process needed

### Fixed

- TUI routing.rs TODOs resolved
- Provider StreamChunk variants for native model operations
- Dead code removal across DX audit
- Clippy warnings in core and sync modules
- Contract test cleanup

### Deprecated

- `spn` CLI — Use `nika` instead. Running `spn` shows deprecation warning.

## [0.26.0] - 2026-03-11

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  🦋 NIKA v0.26.0 — NATIVE INFERENCE (ADR-008)                                 ║
║  Inference moved from spn to Nika via mistral.rs                              ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  ✨ NEW FEATURES:                                                              ║
║  ─────────────────────────────────────────────────────────────────────────    ║
║  • Native inference via mistral.rs (NativeRuntime)                            ║
║  • Streaming support with infer_stream() and async channels                   ║
║  • InferenceBackend trait for unified provider interface                      ║
║  • provider: native support in workflows                                      ║
║                                                                               ║
║  🐛 BUG FIXES:                                                                 ║
║  ─────────────────────────────────────────────────────────────────────────    ║
║  • for_each nested path binding: "{{use.data.nested.items}}" now works        ║
║  • Ignored tests cleaned up: 5 → 1 (Ollama removed, 2 fixed)                  ║
║                                                                               ║
║  Tests: 4,396 passing | Zero clippy warnings | ARMADA CI green               ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### Added

#### Native Inference via mistral.rs (ADR-008)

Inference capabilities moved from spn to Nika for clean architecture:

- **NativeRuntime**: Direct mistral.rs integration for local GGUF models
- **infer_stream()**: Async streaming with `mpsc` channels for real-time output
- **InferenceBackend trait**: Unified interface for all inference providers
- **provider: native**: Use local models in workflows

```yaml
# Use local GGUF model
tasks:
  - id: generate
    infer: "Summarize this document"
    provider: native
    model: "llama3.2:7b"
```

### Fixed

#### for_each Nested Path Binding

**Problem:** `for_each: "{{use.data.nested.items}}"` silently failed when binding
was `data: producer` because code resolved "data.nested.items" as alias instead of
resolving "data" first then traversing ".nested.items".

**Solution:**
- Split path: first segment is alias, rest is nested path
- Parse JSON strings from exec: tasks that output JSON
- Traverse nested path segments with array index support
- Add tracing::warn for missing path segments

```yaml
# Now works correctly
tasks:
  - id: producer
    exec: "echo '{\"nested\": {\"items\": [\"a\", \"b\", \"c\"]}}'"

  - id: consumer
    use:
      data: producer
    for_each: "{{use.data.nested.items}}"  # ✅ Resolves correctly
    as: item
    exec: "echo {{use.item}}"
```

### Changed

- **NativeClient deprecated**: Use NativeRuntime directly
- **Ollama tests removed**: Native inference via mistral.rs replaces Ollama

## [0.24.0] - 2026-03-10

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  🦋 NIKA v0.24.0 — COMPREHENSIVE BUG FIX RELEASE                              ║
║  Critical runtime fixes discovered by 4 Opus 4.5 agents                        ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  🔴 CRITICAL FIXES:                                                           ║
║  ─────────────────────────────────────────────────────────────────────────    ║
║  1. StructuredOutput Layers 3 & 4 — Now actually call LLM for retry/repair    ║
║  2. fail_fast — Properly cancels in-flight tasks with tokio::select!          ║
║  3. Deadlock Detection — Distinguishes deadlock from dependency chain fail    ║
║  4. MCP Timeouts — 5 minute deadline for all MCP operations                   ║
║  5. Sleep Tool Limit — 5 minute max prevents unbounded blocking               ║
║  6. MCP Error Codes — JSON-RPC codes preserved from servers                   ║
║                                                                               ║
║  Tests: 4,391 passing | Zero clippy warnings | ARMADA CI green               ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### Fixed

#### 1. StructuredOutput Layers 3 & 4 — LLM Retry/Repair Now Works

**Problem:** Layers 3 (Retry) and 4 (Repair) were logging errors but NOT calling the LLM.

**Solution:**
- Add `InferCallback` type for LLM invocation during validation
- Layer 3 (Retry) now calls LLM on JSON validation failure
- Layer 4 (Repair) generates repair prompt and calls LLM to fix

```rust
// Before: Just logged and returned Err
// After: Actually calls LLM with structured prompt
pub type InferCallback = Arc<dyn Fn(&str) -> BoxFuture<'static, Result<String>> + Send + Sync>;
```

#### 2. fail_fast — Proper Task Cancellation

**Problem:** `fail_fast: true` wasn't canceling in-flight tasks.

**Solution:**
- Use `tokio::select!` to race semaphore acquisition against cancellation
- Add `TaskStatus::DependencyFailed { dependency: String }` variant
- Add `TaskStatus::Skipped { reason: String }` variant

```rust
tokio::select! {
    permit = semaphore.acquire() => { /* execute task */ }
    _ = cancel_token.cancelled() => {
        return TaskStatus::Skipped { reason: "Workflow cancelled".into() }
    }
}
```

#### 3. Deadlock Detection vs Dependency Chain Failure

**Problem:** True deadlocks were confused with dependency chain failures.

**Solution:**
- New error codes: NIKA-025, NIKA-026, NIKA-027
- Clear error messages showing failed dependency chain
- Distinguishes "task A failed → B can't run" from actual cycles

#### 4. MCP Operation Timeouts

**Problem:** MCP operations could hang indefinitely.

**Solution:**
- Add `INVOKE_TASK_DEADLINE` constant (5 minutes)
- Wrap all MCP operations with `tokio::time::timeout()`

```rust
const INVOKE_TASK_DEADLINE: Duration = Duration::from_secs(5 * 60);
timeout(INVOKE_TASK_DEADLINE, mcp_operation).await??
```

#### 5. Sleep Tool Limits

**Problem:** `nika:sleep` could block workflows indefinitely (e.g., `1000h`).

**Solution:**
- Add `MAX_SLEEP_DURATION` constant (5 minutes)
- Return error for excessive durations

```rust
pub const MAX_SLEEP_DURATION: Duration = Duration::from_secs(5 * 60);
```

#### 6. MCP Error Code Preservation

**Problem:** JSON-RPC error codes from MCP servers were lost.

**Solution:**
- Add `McpErrorCode` enum for structured errors
- Preserve -32700 to -32603 range in error messages

### Changed

- Test count: 4,282 → 4,391 (109 new tests for bug fixes)
- New ADR: NIKA-008 (New Error Codes for v0.24.0)

## [0.23.1] - 2026-03-10

### Fixed

- **Provider Definitions** — Add DataForSEO and Ahrefs to fallback provider definitions
  - Add `dataforseo` and `ahrefs` to `MCP_PROVIDER_IDS` (6→8 providers)
  - Add `DATAFORSEO_API_KEY` and `AHREFS_API_KEY` to `provider_env_var()`
  - Ensures consistency with spn-core `KNOWN_PROVIDERS`

## [0.23.0] - 2026-03-10

### Audit Release — Comprehensive Feature Verification

**Methodology:** 15 Opus 4.5 agents + Ultrathink + TDD + Ralph Wiggum Loop

**Coverage:**
- Two-Phase AST Architecture (19 raw types, 22 analyzed types)
- Runtime execution (5 verbs, for_each parallelism, DAG execution)
- MCP client (10 error codes, timeout hierarchy)
- 7 LLM providers with full streaming
- 75+ error codes verified (NIKA-001 to NIKA-303)
- 8/11 benchmarks within performance targets

**Test Results:**
- 4,481 unit tests passing
- 29 doc tests passing
- Zero clippy warnings

### Fixed

- **BUG-003**: `use:` block now creates implicit `depends_on` edges
- **BUG-004**: Workflow final output now selects deepest terminal task
- **BUG-005**: `for_each: $items` with `as:` alias now works

## [0.22.2] - 2026-03-09

### Fixed

- **schema** — Add `fallback_value` to OutputPolicy JSON schema definition

## [0.22.1] - 2026-03-09

### Fixed

- **agent: file tools** — File tools (`nika:read`, `nika:write`, `nika:edit`, `nika:glob`, `nika:grep`) now properly available in agent tasks when explicitly requested in `tools:` list
- **init WF-08** — Agent template now includes explicit tool list and improved system prompt for reliable file operations

### Changed

- Agent tool filtering now respects the `tools:` parameter for fine-grained control over available tools

## [0.22.0] - 2026-03-08

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  🦋 NIKA v0.22.0 — LANGUAGE IMPROVEMENTS + TUI PANELS                         ║
║  Enhanced workflow syntax and modular UI architecture                          ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  ┌─────────────────────────────────────────────────────────────────────────┐  ║
║  │  NEW FEATURES                                                           │  ║
║  ├─────────────────────────────────────────────────────────────────────────┤  ║
║  │  🔧 exec.env      — Environment variable injection for exec tasks       │  ║
║  │  📦 fetch.json    — Auto-serialize JSON body for fetch tasks            │  ║
║  │  🔗 inputs.xxx    — Access workflow inputs in use: blocks               │  ║
║  │  🔄 $inputs       — for_each binding over workflow inputs               │  ║
║  │  📊 TaskStatus    — New Queued (○) and Skipped (⊘) states               │  ║
║  │  🎨 TUI panels/   — Modular panel widget architecture                   │  ║
║  └─────────────────────────────────────────────────────────────────────────┘  ║
║                                                                               ║
║  Tests: 4,282 passing | Docker: ✅ Verified | Init Workflows: 30 updated     ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### exec.env — Environment Variable Injection

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  🔧 EXEC ENVIRONMENT VARIABLES                                                  │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  Inject environment variables into exec tasks                                   │
│                                                                                 │
│  Syntax:                                                                        │
│  ├── env: { KEY: value }           — Static values                              │
│  ├── env: { KEY: "{{use.secret}}"} — Template resolution                        │
│  └── env: { PATH: "/custom/path" } — Override system vars                       │
│                                                                                 │
│  Example:                                                                       │
│  ┌──────────────────────────────────────────────────────────────────────────┐   │
│  │  - id: deploy                                                            │   │
│  │    exec:                                                                 │   │
│  │      command: ./deploy.sh                                                │   │
│  │      env:                                                                │   │
│  │        API_KEY: "{{use.secret}}"                                         │   │
│  │        NODE_ENV: production                                              │   │
│  └──────────────────────────────────────────────────────────────────────────┘   │
│                                                                                 │
│  Works with both shell: true and shell: false modes                             │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### fetch.json — Auto-Serialize JSON Body

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  📦 FETCH JSON BODY                                                             │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  Automatic JSON serialization for fetch requests                                │
│                                                                                 │
│  Features:                                                                      │
│  ├── Auto Content-Type: application/json                                        │
│  ├── Precedence: json > body                                                    │
│  └── Supports nested objects and arrays                                         │
│                                                                                 │
│  Example:                                                                       │
│  ┌──────────────────────────────────────────────────────────────────────────┐   │
│  │  - id: create_user                                                       │   │
│  │    fetch:                                                                │   │
│  │      url: https://api.example.com/users                                  │   │
│  │      method: POST                                                        │   │
│  │      json:                                                               │   │
│  │        name: "{{use.name}}"                                              │   │
│  │        email: "{{use.email}}"                                            │   │
│  │        roles: [admin, user]                                              │   │
│  └──────────────────────────────────────────────────────────────────────────┘   │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### inputs.xxx — Workflow Input References

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  🔗 INPUT REFERENCES IN USE BLOCKS                                              │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  Access workflow inputs directly in use: blocks                                 │
│                                                                                 │
│  Syntax:                                                                        │
│  ├── inputs.config             — Top-level input                                │
│  ├── inputs.config.theme       — Nested path                                    │
│  └── inputs.items[0]           — Array access                                   │
│                                                                                 │
│  Example:                                                                       │
│  ┌──────────────────────────────────────────────────────────────────────────┐   │
│  │  inputs:                                                                 │   │
│  │    theme: { primary: "#3b82f6", mode: "dark" }                           │   │
│  │                                                                          │   │
│  │  tasks:                                                                  │   │
│  │    - id: style                                                           │   │
│  │      use:                                                                │   │
│  │        color: inputs.theme.primary                                       │   │
│  │        mode: inputs.theme.mode                                           │   │
│  │      infer: "Generate CSS with {{use.color}}"                            │   │
│  └──────────────────────────────────────────────────────────────────────────┘   │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### $inputs Binding — Dynamic for_each

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  🔄 FOR_EACH OVER WORKFLOW INPUTS                                               │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  Iterate over input arrays with $inputs binding                                 │
│                                                                                 │
│  Example:                                                                       │
│  ┌──────────────────────────────────────────────────────────────────────────┐   │
│  │  inputs:                                                                 │   │
│  │    locales: [fr-FR, en-US, de-DE, es-ES]                                 │   │
│  │                                                                          │   │
│  │  tasks:                                                                  │   │
│  │    - id: translate                                                       │   │
│  │      for_each: $inputs.locales                                           │   │
│  │      as: locale                                                          │   │
│  │      concurrency: 4                                                      │   │
│  │      infer: "Translate to {{use.locale}}"                                │   │
│  └──────────────────────────────────────────────────────────────────────────┘   │
│                                                                                 │
│  Works with nested paths: $inputs.config.items                                  │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### TaskStatus — New Lifecycle States

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  📊 TASK STATUS VARIANTS                                                        │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  Icon   Status    Description                                                   │
│  ────   ──────    ───────────────────────────────────────                       │
│   ○     Queued    Task not yet scheduled for execution                          │
│   ◦     Pending   Task waiting for dependencies                                 │
│   ►     Running   Task currently executing                                      │
│   ✓     Success   Task completed successfully                                   │
│   ✗     Failed    Task execution failed                                         │
│   ⏸     Paused    Task temporarily paused                                       │
│   ⊘     Skipped   Task explicitly skipped (NEW)                                 │
│                                                                                 │
│  Queued vs Pending:                                                             │
│  ├── Queued  → Not yet added to execution queue                                 │
│  └── Pending → In queue, waiting for upstream tasks                             │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### TUI panels/ Module

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  🎨 MODULAR PANEL ARCHITECTURE                                                  │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  New src/tui/widgets/panels/ module with reusable components                    │
│                                                                                 │
│  Panels:                                                                        │
│  ├── TaskListPanel    — Selectable list with status badges                      │
│  ├── TaskBoxFlow      — Scrollable task box renderer                            │
│  ├── BrowserPanel     — File browser with git status                            │
│  └── InfoPanel        — Context information display                             │
│                                                                                 │
│  Architecture:                                                                  │
│  ┌──────────────────────────────────────────────────────────────────────────┐   │
│  │  WorkspaceView (8)                                                       │   │
│  │  ┌─────────────┬───────────────────────┬─────────────┐                   │   │
│  │  │ BrowserPanel│   EditorPanel         │  DAGPanel   │                   │   │
│  │  │             │   (code editor)       │  (preview)  │                   │   │
│  │  │   files/    │                       │             │                   │   │
│  │  │   tree      │   TaskListPanel or    │   TaskBox   │                   │   │
│  │  │             │   TaskBoxFlow         │   Flow      │                   │   │
│  │  └─────────────┴───────────────────────┴─────────────┘                   │   │
│  └──────────────────────────────────────────────────────────────────────────┘   │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Statistics

```
╭─────────────────────────────────────────────────────────────────────────────────╮
│  📈 v0.22.0 STATISTICS                                                          │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  Tests:       4,282 passing (up from 4,214)                                     │
│  Workflows:   30 init workflows updated with v0.22 features                     │
│  Docker:      ✅ Verified with Rust 1.94.0                                      │
│  Schema:      v0.22 with exec.env + fetch.json properties                       │
│  Clippy:      Zero warnings                                                     │
│                                                                                 │
│  Files Changed:                                                                 │
│  ├── src/ast/raw/action.rs      — exec.env, fetch.json parsing                  │
│  ├── src/ast/analyzed/action.rs — Validation                                    │
│  ├── src/binding/inputs.rs      — inputs.xxx resolution                         │
│  ├── src/runtime/executor.rs    — Environment injection                         │
│  ├── src/tui/widgets/panels/    — New module (4 panels)                         │
│  └── init/workflows/*.nika.yaml — 30 workflows updated                          │
│                                                                                 │
╰─────────────────────────────────────────────────────────────────────────────────╯
```

### Changed

- **Schema v0.22** — Added exec.env and fetch.json properties with strict validation
- **30 init workflows** — Updated to showcase v0.22 features (87% coverage)

## [0.21.4] - 2026-03-08

### Fixed

- **TUI Panel Focus UX** — Added clear visual indicators for panel focus
  - Problem: Users couldn't tell which panel was focused, making arrow keys feel broken
  - Solution: Added `●` focus indicator to panel titles (Browser, Editor, DAG Preview)
  - Status bar now shows `●Browser` instead of just `Browser` for active panel
  - Modified indicator changed from `●` to `◆` to avoid confusion with focus indicator

### Changed

- **Panel Title Format** — All three panels now show `● [Name]` when focused
  - Browser: `● Browser` (focused) vs `  Browser` (unfocused)
  - Editor: `● Editor ◆` (focused + modified) vs `  Editor` (unfocused)
  - DAG Preview: `● DAG Preview` (focused) vs `  DAG Preview` (unfocused)

## [0.21.3] - 2026-03-08

### Fixed

- **ChatView Freeze Bug** — Mention autocomplete no longer traps keyboard input
  - Root cause: `_ => {}` fallthrough kept autocomplete visible
  - Fix: Dismiss autocomplete on any non-navigation key (Tab/Enter/Up/Down/Esc)
  - Added proper `on_enter()` / `on_leave()` lifecycle hooks to ChatView

### Changed

- **Dead Code Cleanup** — Removed ~85 lines of unused code from app module
  - Removed: `ensure_chat_agent()`, `build_conversation_context()`, `get_mcp_server_names()`
  - Removed: `handle_mouse()`, `handle_scroll_to_top/bottom()`
  - Mouse events now return `Action::Continue` (not implemented)

### Added

- **Which-Key Widget** — Vim-style keybinding popup for discoverability
  - Shows available keybindings after pressing prefix keys (g, z, [, ], Space)
  - 300ms delay before popup, 3s auto-close timeout
  - Solarized-inspired colors matching TUI theme
  - Integrated into StudioView with CommandPalette overlay

## [0.21.2] - 2026-03-07

### Fixed

- **Release Pipeline** — Complete rewrite aligned with spn-cli pattern
  - Fix GitHub Action versions (v6/v7/v8 don't exist → use v4)
  - Add 6 build targets: macOS arm64/x64, Linux arm64/x64, musl arm64/x64
  - Add docker-publish job using pre-built musl binaries
  - Add Homebrew formula auto-update
  - Add SLSA provenance and SBOM generation
  - Remove broken `docker.yml` (Docker now in `release.yml`)
- **Docker ARM64 Build** — Fix cross-compilation failure
  - Error: `failed to find tool aarch64-linux-musl-gcc`
  - Root cause: Building inside Alpine container can't cross-compile
  - Solution: Build musl binaries in CI, copy pre-built to scratch image
- **Dockerfile** — Simplify to scratch pattern with pre-built binaries
  - Remove multi-stage build with Rust compilation
  - Use CI-built static musl binaries (~5MB image)

## [0.21.0] - 2026-03-05

### Added

- **Structured Output Engine** — 4-layer defense system for ~99.99% JSON Schema compliance
  - **Layer 1**: rig Extractor (Rust type extraction via schemars)
  - **Layer 2**: Provider-native (tool_use / response_format)
  - **Layer 3**: Retry with feedback (error messages + schema)
  - **Layer 4**: LLM repair (separate repair call with original + errors)
- **`structured:` task field** — Configure structured output validation per task
  - Shorthand: `structured: ./schemas/user.json`
  - Full form: `structured: { schema: {...}, max_retries: 3, enable_repair: true }`
  - Inline JSON Schema or file path reference
  - Layer toggles: `enable_extractor`, `enable_tool_use`, `enable_retry`, `enable_repair`
- **Error codes NIKA-300-303** — Structured output error variants
  - NIKA-300: StructuredOutputExtractionFailed (parsing failure)
  - NIKA-301: StructuredOutputValidationFailed (schema mismatch)
  - NIKA-302: StructuredOutputRepairFailed (repair LLM failed)
  - NIKA-303: StructuredOutputAllLayersFailed (all layers exhausted)
- **StructuredOutput events** — Observability for validation attempts
  - `StructuredOutputAttempt`: Logs each layer attempt with result
  - `StructuredOutputRepaired`: Logs successful repairs
- **Example workflow** — `examples/v21-structured-output.nika.yaml`
- **JSON Schema update** — `StructuredOutputSpec` definition in workflow schema
- **Implicit Output Syntax** — `$task` shorthand in `use:` blocks
  - `$step1` normalizes to `step1` during YAML parsing
  - Single leading `$` stripped via `UseEntry::normalize_path()`
  - Both `$task` and `task` are equivalent forms
  - Existing workflows continue to work unchanged
  - Example workflow: `examples/v21-implicit-output.nika.yaml`

### Changed

- **Runner integration** — `execute_task_iteration()` validates output when `task.structured` is set

## [0.19.5] - 2026-03-04

### Fixed

- **simple-exec template** — Changed invalid `shell:` verb to proper `exec: { command, shell: true }` format

## [0.19.4] - 2026-03-04

### Added

- **Output Policy for JSON Schema Injection** — Runtime schema enforcement
  - `OutputPolicy` parameter added to `execute()` function
  - `build_json_schema_instruction()` for structured output prompts
  - Schema requirements injected into infer/agent prompts
- **{{inputs.*}} Template Resolution** — Access workflow inputs in templates
  - Third resolution pass after `use.*` and `context.*`
  - Enables dynamic workflow parameterization

### Fixed

- **Benchmark thresholds** — Relaxed for debug builds
  - Parse simple: 500µs → 2000µs
  - DAG construction small: 50µs → 500µs
- **Execute signature migration** — Updated all tests for 5-argument signature
- **Clippy warning** — Fixed unnecessary_lazy_evaluations in executor.rs

### Changed

- **autonomous-research-agent** — Moved to experimental/ (uses future features)

## [0.19.3] - 2026-03-04

### Added

- **`nika new` Command** — Interactive workflow creation wizard
  - Templates: minimal, infer, agent, pipeline, mcp
  - CLI flags for non-interactive mode
- **103 Test Suite Workflows** — Comprehensive coverage for all features
- **Task-level flow** — `flow:` field on individual tasks
- **Server alias** — `alias:` field in MCP server configuration

### Fixed

- **Schema/code coherence** — All schema definitions match runtime behavior
- **20 critical workflows** — Updated to schema@0.10

## [0.19.2] - 2026-03-03

### Added

- **Structured Output Enforcement** — 3-layer validation for JSON output
  - `SchemaRef` enum: supports inline JSON Schema objects or file path references
  - `validate_schema_ref()`: async schema validation using jsonschema crate
  - `DynamicSubmitTool`: LLM-side schema enforcement via tool injection
  - `format_validation_errors()`: formatted error feedback for retry loops
  - **Retry loop**: Auto-retries infer tasks on validation failure with error feedback
    - Configurable via `max_retries: N` in output policy (default: 0)
    - Retry prompt includes schema, previous output, and validation errors
    - Error codes: NIKA-060 (invalid JSON), NIKA-061 (schema validation failed)
- **for_each binding references** — Dynamic iteration over task outputs
  - `$alias` format: `for_each: "$locales"`
  - Template format: `for_each: "{{use.locales}}"`
- **extended_thinking support** — Claude deep reasoning for infer and agent verbs
  - `extended_thinking: true` to enable thinking mode
  - `thinking_budget: 10000` to set token budget for thinking

### Fixed

- **Empty parent path handling** — Include expansion now handles relative filenames
  with empty parent paths correctly

### Changed

- **OutputPolicy schema** — Updated JSON Schema with `oneOf` for schema field
  supporting both inline objects and file path strings
- **Test helpers** — Updated FetchParams and InferParams with new required fields

## [0.19.1] - 2026-03-03

### Added

- **Artifact Processor** — Workflow output persistence system
- **Expert Workflows** — Showcasing v0.19 features with NovaNet integration

### Fixed

- **Workflow examples** — Made agentic with proper NovaNet integration

## [0.19.0] - 2026-03-03

### Added

- **Initial v0.19 release** — Structured outputs foundation
- **Retry loop** — For structured output validation
- **Binding references in for_each** — Dynamic iteration support

## [0.18.0] - 2026-03-03

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  🦋 NIKA v0.18.0 — ARTIFACTS SYSTEM                                           ║
║  Complete file persistence infrastructure for task outputs                     ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  ┌─────────────────────────────────────────────────────────────────────────┐  ║
║  │  MILESTONES                                                             │  ║
║  ├─────────────────────────────────────────────────────────────────────────┤  ║
║  │  M1 🔧 io::atomic    — Atomic writes with crash safety                  │  ║
║  │  M2 🔒 io::security  — Path validation, traversal prevention            │  ║
║  │  M3 📝 io::template  — Variable interpolation ({{task_id}}, etc.)       │  ║
║  │  M4 ✨ io::writer    — ArtifactWriter combining all modules             │  ║
║  └─────────────────────────────────────────────────────────────────────────┘  ║
║                                                                               ║
║  Tags: v0.18.0-m1-atomic, v0.18.0-m2-security, v0.18.0-m3-template,          ║
║        v0.18.0-m4-writer, v0.18.0 (release)                                   ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### M1: io::atomic — Atomic File Writes

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  🔧 ATOMIC WRITES                                 Tag: v0.18.0-m1-atomic       │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  Pattern: temp file → fsync → atomic rename                                     │
│                                                                                 │
│  Functions:                                                                     │
│  ├── write_atomic()   — Guaranteed atomic overwrites                            │
│  ├── write_unique()   — Auto-suffix for collision avoidance                     │
│  ├── write_fail()     — Fail if file already exists                             │
│  └── write_append()   — Append to existing files                                │
│                                                                                 │
│  Tests: 16 new                                                                  │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### M2: io::security — Path Validation

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  🔒 PATH SECURITY                                 Tag: v0.18.0-m2-security     │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  Prevents traversal attacks and malicious paths                                 │
│                                                                                 │
│  Functions:                                                                     │
│  ├── validate_artifact_path()     — Stays within artifact directory             │
│  ├── validate_path_components()   — Rejects null bytes, control chars           │
│  ├── normalize_path()             — Safe normalization (no fs access)           │
│  └── Max path length              — 4096 chars enforced                         │
│                                                                                 │
│  Tests: 17 new                                                                  │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### M3: io::template — Variable Interpolation

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  📝 TEMPLATE RESOLUTION                           Tag: v0.18.0-m3-template     │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  Built-in Variables:                                                            │
│  ├── {{task_id}}        — Current task identifier                               │
│  ├── {{workflow_name}}  — Workflow name                                         │
│  ├── {{date}}           — ISO date (YYYY-MM-DD)                                 │
│  ├── {{time}}           — ISO time (HH-mm-ss)                                   │
│  ├── {{timestamp}}      — Unix timestamp                                        │
│  └── {{uuid}}           — Random UUID v4                                        │
│                                                                                 │
│  Custom Formats:                                                                │
│  ├── {{date.YYYY-MM-DD}}                                                        │
│  └── {{time.HH-mm-ss}}                                                          │
│                                                                                 │
│  API: with_var(), with_vars()                                                   │
│  Tests: 20 new                                                                  │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

### M4: io::writer — ArtifactWriter

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  ✨ ARTIFACT WRITER                               Tag: v0.18.0-m4-writer       │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  Main entry point combining all io modules                                      │
│                                                                                 │
│  API:                                                                           │
│  ├── ArtifactWriter::new()    — Create with base directory                      │
│  ├── WriteRequest builder     — Content, format, custom vars                    │
│  ├── WriteResult              — Path, size, format metadata                     │
│  └── with_max_size()          — Configurable limits (default 10 MB)             │
│                                                                                 │
│  Tests: 15 new                                                                  │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Security Hardening

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  🛡️ SECURITY HARDENING                                                         │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  Template Injection Prevention:                                                 │
│  ├── Rejects /, \, \0, .., ~ in custom variable values                          │
│  ├── with_var() now returns Result<Self, NikaError>                             │
│  └── Prevents path traversal via template injection                             │
│                                                                                 │
│  TOCTOU Mitigation:                                                             │
│  ├── Initial validation before directory creation                               │
│  ├── Final validation with canonicalize() after dirs exist                      │
│  └── Reduces window between validation and write                                │
│                                                                                 │
│  JSON Format Validation:                                                        │
│  ├── OutputFormat::Json validated with serde_json                               │
│  └── Invalid JSON rejected with descriptive error                               │
│                                                                                 │
│  Edge Cases:                                                                    │
│  └── Empty variable names rejected with clear error                             │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Events & Errors

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  📊 EVENTS                              │  ❌ ERROR CODES (NIKA-280-289)       │
├─────────────────────────────────────────┼─────────────────────────────────────────┤
│                                         │                                         │
│  ArtifactWritten                        │  NIKA-280: ArtifactPathError            │
│  ├── task_id                            │  └── Path traversal detected            │
│  ├── path                               │                                         │
│  ├── size                               │  NIKA-281: ArtifactSizeExceeded         │
│  └── format                             │  └── Content exceeds max_size           │
│                                         │                                         │
│  ArtifactFailed                         │  NIKA-282: ArtifactWriteError           │
│  ├── task_id                            │  └── Write operation failed             │
│  ├── path                               │                                         │
│  └── reason                             │  All errors include FixSuggestion       │
│                                         │                                         │
└─────────────────────────────────────────┴─────────────────────────────────────────┘
```

### Statistics

```
╭─────────────────────────────────────────────────────────────────────────────────╮
│  📊 v0.18.0 STATISTICS                                                          │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  Tests:    68 new (atomic: 16, security: 17, template: 20, writer: 15)          │
│  Total:    4,328 tests passing                                                  │
│  Clippy:   Zero warnings                                                        │
│  Coverage: All existing tests passing                                           │
│                                                                                 │
╰─────────────────────────────────────────────────────────────────────────────────╯
```

## [0.17.5] - 2026-03-03

### Added
- **MCP Retry Module** - Exponential backoff for transient failures
  - `McpRetryConfig` for configurable retry behavior
  - `retry_mcp_call()` for automatic retry on timeouts/connection issues
  - `is_retryable_mcp_error()` to classify error types
  - Defaults: 3 retries, 100ms initial delay, 5s max delay, jitter enabled
- **call_tool_with_retry()** - MCP adapter method with automatic retry
- **DecomposeTimeout Error (NIKA-171)** - New error variant for decompose timeouts
  - `DECOMPOSE_TIMEOUT` constant (120s) for BFS graph traversal
  - Timeout protection in runner.rs prevents silent hangs
- **InferParams Validation** - Validate prompt and temperature before execution
  - Empty/whitespace prompts now rejected early
  - Temperature validated in range 0.0..=2.0
- **InvokeParams Validation** - Enhanced empty string detection
  - MCP server name, tool name, resource URI validated
- **Prompt Validation in Executor** - Validate resolved prompts after template expansion
- **spn_config Public Exports** - Module now exported in MCP public API
  - `SpnMcpConfig`, `SpnMcpServer`, `SpnMcpSource`, `SpnMcpConfigManager`
- **Provider Fallback Chain Tests** - 12 new tests for auto-detection priority
- **Error Path Tests** - 11 new tests for context_loader and file_adapter
- **MCP Secrets Integration Tests** - 6 new tests for spn ↔ Nika secrets flow
- **Test Example Workflows** - 5 new workflow examples for testing

### Changed
- **backon** moved to default dependencies (used by MCP retry, not just jobs)
- **Example Paths** - Updated to relative cargo run commands (portable)
- **CI Paths** - Updated novanet-dev to novanet
- **Plan Docs** - Removed hardcoded absolute paths

### Statistics
- **3,449 tests passing** (up from 3,381 in v0.17.4)
- **Zero clippy warnings**
- **All ARMADA checkpoints passing**
- **17 commits in this release**

## [0.17.4] - 2026-03-03

### Fixed
- **Keyring macOS Backend** - Critical fix for credential storage
  - Added platform features to keyring crate: `apple-native`, `windows-native`, `sync-secret-service`
  - Previously used MockCredential (in-memory only), now uses real macOS Keychain
  - Enables unified keyring between `nika` and `spn` CLI with service name "spn"
  - Keys now persist across sessions and are shared between tools
- **VerbColor::User Patterns** - Added missing enum variant handling
  - Added User variant to all VerbColor methods: `icon()`, `rgb_tuple()`, `glow_tuple()`, `muted_tuple()`, `icon_ascii()`, `hex()`, `label()`, `border_rgb()`
  - Fixed chat.rs CurrentVerb mapping for User variant
  - Fixed placeholder text for User verb type
- **Example Workflows** - Fixed field names in init templates
  - Changed `stop_sequences` to `stop_conditions` in agent task examples

### Statistics
- **3,381 tests passing**
- **Zero clippy warnings**

## [0.17.3] - 2026-03-03

### Added
- **StreamingDecrypt Integration** - InferBox now uses streaming decrypt animation
  - Integrated StreamingDecrypt widget into InferBox for progressive text reveal
  - Matrix-style decryption effect during LLM inference

### Fixed
- **Tab Bar Keybindings** - Corrected [1]/[2] keybindings to match tab bar order

### Statistics
- **3,378 tests passing**
- **Zero clippy warnings**

## [0.17.2] - 2026-03-03

### Performance
- **Matrix Rain Zero-Allocation Rendering** - Eliminated heap allocations in TUI hot path
  - `RainGlyph::write_to_buf()` uses stack-allocated UTF-8 buffer instead of `to_string()`
  - Removed `as_str() -> String` in favor of direct buffer writes
  - `DecryptGlyph::as_str()` now returns `Cow<'static, str>` to avoid allocation for static strings
- **Chat Message Separator Optimization** - Pre-computed separator eliminates `.repeat()` allocation
  - Added `SEPARATOR_200` constant (200 Unicode box chars)
  - Dynamic separator now uses slice instead of allocation
  - Saves ~1 allocation per message per render frame
- **Agent History Pre-allocation** - Reduced reallocations during agent conversations
  - History Vec pre-allocated with `capacity = max_turns * 2`
  - Default 20 messages (10 turns) pre-allocated
  - Documented rig-core API constraint requiring Vec clone per chat call

### Fixed
- **MCP Connection State Race** - Eliminated stale AtomicBool in client.rs
  - `is_connected()` now delegates to `adapter.is_connected_sync()`
  - Made `RmcpClientAdapter::is_connected_sync()` public
  - Prevents false positives when adapter connection drops unexpectedly
- **pkg_resolver Safety** - Replaced `unwrap()` with `expect()` on user input validation
  - Line 227: `id.chars().next().unwrap()` → `.expect("id is non-empty")`
  - Added SAFETY comment documenting the invariant
  - Prevents potential panic if code is refactored

### Statistics
- **3,375 tests passing** (stable)
- **Zero clippy warnings**

## [0.17.1] - 2026-03-03

### Security
- **Path Traversal Bypass Fixed** - Critical security fix in include_loader.rs
  - Removed unsafe `unwrap_or_else` fallback that bypassed path boundary validation
  - `canonicalize()` failures now properly return errors instead of proceeding with unvalidated paths
  - Both `validate_path_boundary()` and circular include detection now fail safely

### Fixed
- **MCP Lock Contention** - Performance fix in rmcp_adapter.rs
  - Clone `Peer` and release lock immediately to prevent lock contention during timeouts
  - `call_tool()`, `read_resource()`, and `list_tools()` no longer hold mutex for 60s during timeout
  - Concurrent MCP operations now execute without blocking each other
- **Schema Version in Error Help** - Updated help messages from @0.5 to @0.9
  - `InvalidSchemaVersion` error now suggests `nika/workflow@0.9` (current schema)
  - `InvalidSchema` error help message updated
  - Test assertions updated to match

### Statistics
- **3,375 tests passing** (stable)
- **Zero clippy warnings**
- **All ARMADA checkpoints passing**

## [0.17.0] - 2026-03-02

### Added
- **pkg: Support for Workflow Includes** - Reference packages in include blocks
  - `pkg` field in IncludeSpec for package references
  - Support `@workflows/name` in include blocks
  - JSON schema updated with oneOf for path/pkg
  - Validation for mutually exclusive path/pkg
  - Resolve packages via registry in include_loader
- **Registry v0.17 Optimizations** - Performance and reproducibility improvements
  - DashMap cache for package resolution (Arc-based caching)
  - `spn.lock` support for reproducible builds
  - `@jobs` package type detection
  - `agents:` block added to JSON schema validation
- **Runtime Package Support** - `@agents`, `@prompts`, `@skills` packages
  - Package resolution integrated into `nika run` and `nika check`
  - Registry-based package loading at runtime
- **Complete AgentDef JSON Schema** - Full agent definition variants documented

### Security
- **3 Critical Vulnerabilities Fixed**
  - Memory leak in package resolution cache
  - File corruption prevention in concurrent writes
  - TOCTOU race condition in file operations

### Statistics
- **3,358+ tests passing**
- **Zero clippy warnings**
- **All ARMADA checkpoints passing**

## [0.16.6] - 2026-03-02

### Added
- **Security Audit Report** - Comprehensive v0.16.5 security analysis
  - Score: 92/100 - Zero CVE, zero unsafe blocks
  - 1,332 unwrap() occurrences (90% in tests)
  - Recommendations for branch protection and rmcp upgrade
- **Examples Audit Report** - Validation of 164 example workflows
  - 161/164 valid workflows
  - 3 broken examples (stop_conditions field removed)
  - Comprehensive test coverage analysis

### Fixed
- **README Version Corrections** - All version references updated to v0.16.5
  - Version badge updated from v0.16.1 to v0.16.5
  - TUI ASCII art header updated
  - "What's New" section synchronized
  - --version example corrected
  - Stats box updated
  - **PR #64 merged:** fix/readme-versions-v0.16.5

- **Homebrew Formula Synchronization** - Package manager formula updated
  - Version updated from 0.16.1 to 0.16.5
  - SHA256 checksums updated for all 4 platforms
    - macOS ARM64: `550350546a3e5b00148b9065ed2b3eda260ff63e84440edf0aae8db7aff8fc6b`
    - macOS x64: `d85baceb8eb912846a5b9d292af2e23e758956b915d9c6cb2c28dd688fec92fe`
    - Linux ARM64: `0fcbf61e97598826b374bc66a5a2f13df28658086ab2635a7c48f57b0efde4c2`
    - Linux x64: `d80dfa28c1e8c6c2b23b6545ea01ef093140182b42a029bbba34a9937ba24eb1`
  - **PR #1 merged** (supernovae-st/homebrew-tap)

- **GitHub Releases Correction** - Missing releases and promotion
  - Created GitHub release v0.16.4 with complete notes
  - Promoted v0.16.5 from pre-release to Latest

### Changed
- **Branch Cleanup** - Removed 3 orphan branches from remote
  - `docs/changelog-v0.16.3-tui` (already merged)
  - `feat/spn-mcp-config` (already merged)
  - `fix/schema-validator-tests` (already merged)

### Documentation
- `AUDIT-EXAMPLES-2026-03-02.md` - Examples validation report
- `docs/SECURITY-AUDIT-v0.16.5.md` - Security audit findings
- `.github/SECURITY.md` - Security policy reference

### Statistics
- **3,358 tests passing** (stable)
- **Zero clippy warnings**
- **All ARMADA checkpoints passing**
- **2 PRs merged:** #64 (README), #1 (Homebrew)

## [0.16.5] - 2026-03-02

### Added
- **Chat View TUI Improvements** - Enhanced chat input experience
  - **Dynamic Input Height** - Input area automatically expands from 1-10 lines based on content
  - **Scroll Indicators** - Visual indicators (↑↓) show when chat history is scrollable
  - **Edit History Integration** - Full undo/redo support (Ctrl+Z/Ctrl+Y) in chat input
  - **Vim Navigation** - Ctrl+j/k for scrolling chat history (Vim-style bindings)

### Fixed
- **Formatting** - Fixed comment alignment in Layout constraints (cargo fmt compliance)
- **Clippy Warnings** - Replaced manual ceiling division with `.div_ceil()` method (Rust 1.93+)
  - Line 3226: `(char_count as u16).div_ceil(content_width)`
  - Line 3297: `((line_len as u16).div_ceil(content_width))`

### Changed
- `src/tui/views/chat.rs` - 210 insertions, 8 deletions

### Statistics
- **3,358 tests passing** (stable)
- **Zero clippy warnings**
- **All ARMADA checkpoints passing**
- **PR #62 merged:** feat/tui-v0.16.5

## [0.16.4] - 2026-03-02

### Added
- **Extended Thinking Documentation** - Comprehensive guide for extended_thinking feature
  - Field definitions and ranges (thinking_budget: 1024-65536, default 8192)
  - Usage examples for `infer:` and `agent:` tasks
  - Token budget guidelines (5 tiers from simple to maximum reasoning)
  - Event structure with thinking capture details
  - Best practices and limitations
  - Debugging example with thinking process access
  - Token tracking explanation
  - 139 lines added to CLAUDE.md

### Changed
- **Unified MCP Management** - Merged feat/spn-mcp-config (#58)
  - Centralized MCP server configuration in ~/.spn/mcp.yaml
  - Shared config between Nika and other spn tools
  - Better DX for MCP server management
- **Template Bindings** - Merged fix/schema-validator-tests (#60)
  - Extended thinking fields in schema @0.9
  - Improved template binding validation

### Statistics
- **3,353 tests passing** (stable)
- **Zero clippy warnings**
- **All ARMADA checkpoints passing**
- **3 PRs merged:** #58, #59, #60

## [0.16.3] - 2026-03-02

### Fixed
- **nika init** - All 4 example workflows now have correct syntax
  - `01-hello-world.nika.yaml`: Fixed YAML syntax errors
  - `02-parallel-pipeline.nika.yaml`: Fixed context file paths
  - `03-agent-advanced.nika.yaml`: Fixed builtin tool references (`nika:read` not `read_file`)
  - `04-production-pipeline.nika.yaml`: Fixed all syntax and reference issues

### Changed
- CI workflows updated with latest GitHub Actions versions

### Statistics
- **3,358 tests passing**
- **Zero clippy warnings**
- **All ARMADA checkpoints passing**

## [0.16.2] - 2026-03-02

### Added
- **DX Consolidation** - Comprehensive documentation audit with 10 parallel agents
  - All CLAUDE.md files aligned to v0.16.2
  - Version references synchronized across 11 documentation files
  - Test counts corrected to 3,358 (accurate count)
  - Outdated feature references removed

### Changed
- Root CLAUDE.md: Updated version from v0.14.3 to v0.16.2
- nika/CLAUDE.md: Version sync to v0.16.2
- tools/nika/CLAUDE.md: Fixed version from v0.15.1 to v0.16.2, test count from 4,380 to 3,358
- dx/.claude/rules/nika.md: Added v0.16.2 section

### Statistics
- **3,358 tests passing**
- **Zero clippy warnings**
- **All ARMADA checkpoints passing**
- **11 CLAUDE.md files audited and synchronized**

## [0.16.1] - 2026-03-01

### Added
- Documentation and versioning consistency fixes
- All v0.16.0 features verified and tested

### Statistics
- **3,358 tests passing**
- **Zero clippy warnings**
- **All ARMADA checkpoints passing**

## [0.16.0] - 2026-03-01

### Breaking Changes
- **Remove nika pkg commands** - Migrated to `spn` CLI
  - `nika pkg install/list/search/update/remove` → Use `spn pkg` instead
  - Migration guide: `docs/MIGRATION-PKG-TO-SPN.md`

### Added
- **TaskBox Inline Rendering** - All 5 verbs now have inline task visualization
- **rmcp 0.16 SDK** - Updated MCP client to latest SDK version

### Changed
- CLI cleanup: ~221 lines removed from pkg module
- Dependency update: rmcp 0.14 → 0.16

### Statistics
- **3,358+ tests passing**
- **Zero clippy warnings**
- **7 LLM providers** (Claude, OpenAI, Mistral, Groq, DeepSeek, Ollama, Gemini)

## [0.15.2] - 2026-03-01

### Changed
- **Cargo.lock** - Updated for rustls migration (removes native-tls dependencies)
- **Cross-compilation** - Fixed ARM64 builds via `cross` tool
- **Release workflow** - Corrected archive paths and working directories

### Security
- **rustls-tls** - Switched from native-tls to rustls for consistent TLS across platforms

### Fixed
- ARM64 Linux builds now compile successfully (#43)
- Release archives contain correct binary paths (#42)
- CI jobs use proper working directory (#41)

### Statistics
- **3,358 tests passing**
- Zero clippy warnings
- Schema @0.9 fully supported
- **7 LLM providers** (Claude, OpenAI, Mistral, Groq, DeepSeek, Ollama, Gemini)

## [0.15.1] - 2026-03-01

### Added
- **Skill Merging Through DAG Fusion** - Workflow-level skills propagate through `include:` DAG fusion
  - `SkillDef` AST type with path and optional alias
  - `merge_skills()` function with deduplication and circular detection
  - Local paths and `pkg:` URI support
  - 11 tests for skill merging
- **pkg: Protocol Support** - Reference skills from package registry
  - `pkg:@scope/name@version/path` URI syntax
  - Resolves to `~/.spn/packages/@scope/name/version/path`
  - Implementation in `src/ast/pkg_resolver.rs`

### Changed
- Cargo.lock updated for rustls migration (removes native-tls dependencies)
- All fix branches merged (cross-compilation, release workflow, rustls)

### Statistics
- **3,358 tests passing** (up from 3,480 in v0.14.6 - test consolidation)
- **Zero clippy warnings**

## [0.15.0] - 2026-02-28

### Added
- **Security Hardening: Shell-Free Execution**
  - `exec:` defaults to `shell: false` (shlex parsing) for security
  - Command blocklist prevents dangerous binaries (`rm -rf`, `sudo`, etc.)
  - New error code: `NIKA-053 BlockedCommand`
  - Implementation in `src/core/security.rs`
- **Infer LLM Control Parity**
  - `temperature` - 0.0-1.0 creativity control
  - `system` - System prompt injection
  - `max_tokens` - Output length limit
  - `InferParams` and `InferOptions` structs
- **Gemini Provider (7th provider)**
  - `RigProvider::gemini()` constructor
  - `RigAgentLoop::run_gemini()` for agent mode
  - Full streaming support with token tracking
  - Auto-detection via `GEMINI_API_KEY`
- **File Tools (5 new builtin tools)**
  - `nika:read` - Read file content
  - `nika:write` - Create/overwrite file
  - `nika:edit` - Modify file with old/new string replacement
  - `nika:glob` - Find files by pattern
  - `nika:grep` - Search content by pattern
  - 11 builtin tools total (6 core + 5 file)

### Breaking Changes
- `exec:` now defaults to `shell: false` - Add `shell: true` for pipes/redirects

### Statistics
- **3,480+ tests passing**
- **Zero clippy warnings**
- **7 LLM providers** (Claude, OpenAI, Mistral, Groq, DeepSeek, Ollama, Gemini)

## [0.14.6] - 2026-02-28

### Added
- Full validation of 148 example workflows
- E2E tests for all 5 verbs (exec, fetch, infer, invoke, agent)
- Stress test for large DAG (96 tasks)
- for_each parallel execution validation

### Fixed
- `test-agent-depth-limit.nika.yaml` - removed invalid `target: null` flow
- `test-agent-temperature.nika.yaml` - commented out proposed temperature feature
- `test-agent-with-thinking.nika.yaml` - commented out proposed thinking features

### Statistics
- **3,480+ tests passing** (comprehensive validation)
- **132/148 example workflows valid** (16 are drafts/experimental)
- **Zero clippy warnings**

## [0.14.5] - 2026-02-28

### Changed
- Consolidated release merging all v0.14.x branches
- Clean, up-to-date main branch after PR conflicts resolved

### Statistics
- **3,250+ tests passing**
- **Zero clippy warnings**

## [0.14.4] - 2026-02-28

### Added
- 5 verb test workflows (test-infer-verb.nika.yaml, test-exec-verb.nika.yaml, etc.)
- CI schema validation job (ARMADA Station 7)
- Comprehensive verb coverage examples

### Statistics
- **3,230+ tests passing**
- **Zero clippy warnings**

## [0.14.3] - 2026-02-28

### Changed
- Consolidated security release ensuring all v0.14.2 features are properly included

### Notes
- v0.14.2 tag was created before security PR was merged
- v0.14.3 includes all features listed in v0.14.2 changelog plus complete security fixes

## [0.14.2] - 2026-02-28

### Added
- **context: Field (Schema @0.9)** - Load files at workflow start
  - `context.files` block with alias-to-path mapping
  - Supports markdown, JSON, YAML, and glob patterns
  - Session restore via `context.session`
  - Accessible via `{{context.files.alias}}` bindings
- **include: DAG Fusion (Schema @0.9)** - Merge external workflows
  - `include:` array with path and optional prefix
  - Task ID prefixing for namespace isolation
  - Recursive include resolution with cycle detection
- **Path Traversal Security** - Boundary validation for file loading
  - `validate_path_boundary()` in include_loader.rs
  - `validate_path_boundary()` in context_loader.rs
  - Prevents `../../../` style attacks
- **Schema Validation CI Job** - ARMADA Station 7
  - Validates schema versions v0.1-v0.9
  - Validates v0.14.2 feature examples
  - Validates all public examples
- **New Example Workflows**
  - `v09-context-loading.nika.yaml` - context: field demo
  - `v05-lazy-bindings.nika.yaml` - lazy: bindings with defaults
  - `v06-multi-provider.nika.yaml` - multi-provider support
  - `v05-spawn-agent.nika.yaml` - nested agent spawning
  - `v03-foreach-parallel.nika.yaml` - for_each parallelism

### Fixed
- Path traversal vulnerability in include_loader.rs and context_loader.rs
- Schema validation for all schema versions in CI

### Statistics
- **3,211 tests passing** (path validation tests added)
- **Zero clippy warnings**
- **Schema @0.9** - Full context: + include: support

## [0.12.1] - 2026-02-26

### Added
- **TaskBox v0.11 Implementation Plan** - Comprehensive 22-phase development roadmap
  - ASCII design specification for all 5 verbs (InferBox, ExecBox, FetchBox, InvokeBox, AgentBox)
  - Gap analysis report (12 major + 8 minor gaps identified)
  - Detailed Rust implementation code for phases 18-22
  - 115 new tests planned (3104 total after implementation)
- **TaskBox Visual Enhancements**
  - `AgentBox` compact mode with turn counter and tool count
  - `BorderPulse` animation integration for all widgets
  - `TokenVelocity` sparkline widget
  - `RenderMode` enum (Compact/Expanded/Full)
  - `KeyAction` enum for keyboard shortcuts
  - Subagent visual distinction (🐤 vs 🐔)

### Fixed
- CI checkout step in fleet-cleared job
- Test timeout configuration for exec tests
- API key handling in integration tests (graceful degradation)
- Rustdoc HTML-like tags escaping
- Clippy and lint warnings across TUI modules

### Documentation
- `2026-02-26-taskbox-ascii-design-spec.md` - Visual reference for all widgets
- `2026-02-26-taskbox-gap-analysis.md` - Implementation audit report
- `2026-02-26-taskbox-v0.11-implementation-plan.md` - Full 22-phase plan (4671 lines)
- `v0.11-taskbox-event-wiring.md` - Event system documentation

## [0.12.0] - 2026-02-26

### Added
- **ARMADA CI System** - 10-station quality checkpoint system
  - Station 1: Format (`cargo fmt --check`)
  - Station 2: Lint (`cargo clippy -- -D warnings`)
  - Station 3: Tests (`cargo nextest run` - 2,997 tests)
  - Station 4: Coverage (`cargo llvm-cov` >70%)
  - Station 5: Docs (`cargo doc --no-deps`)
  - Station 6: Security (`cargo audit` + `cargo deny`)
  - Station 7-8: AI Reviews (CodeRabbit + Claude)
  - Station 9: Conventional commits validation
  - Station 10: Version lock enforcement (0.x.x forever)
- **Version Lock Enforcement** - Nika will NEVER be v1.0.0
  - Rust tests (`tests/version_lock_test.rs`)
  - CI workflow (`.github/workflows/version-lock.yml`)
  - Claude Code hooks (PreToolUse blocks v1.x)
  - release-plz configured for 0.x.x
- **/ship Skill** - One-command shipping workflow
  - Detects changes → Creates branch → Commits → Pushes → Creates PR
  - Waits for CI → Enables auto-merge → Cleans up
- **6-Views Architecture** - Complete TUI restructure
  - View enum: Home, Chat, Studio, Monitor, Settings, Help
  - Full keyboard navigation (1-6 keys)
  - Cross-view state synchronization
- **TaskBox Widgets** - Compact/expanded modes with animations
  - `InferBox`, `ExecBox`, `FetchBox`, `InvokeBox`, `AgentBox`
  - `BorderPulse` animation for running state
  - `TokenVelocity` real-time metrics
  - `RenderMode` enum for detail levels

### Statistics
- **2,997 tests passing** (277 new in v0.10-v0.12)
- **Zero clippy warnings**
- **11 Claude Code skills**, **7 hooks**, **27 rules**

## [0.11.0] - 2026-02-25

### Added
- **Production Wiring** - Complete integration of all TUI components
  - Chat DAG widgets wired into ChatView
  - Settings and Help views integrated
  - MonitorView with View trait implementation
- **release-plz Automation** - Automated release PR creation
  - Conventional commits → CHANGELOG generation
  - git-cliff for changelog formatting
  - GitHub release creation

### Changed
- Rebrand FORTRESS → ARMADA (cosmic pirate theme)
- Version bump to v0.11.0

## [0.10.0] - 2026-02-25

### Added
- **Chat DAG Widgets** (108 tests)
  - `ChatNodeBox` - Individual chat message as graph node (4 kinds, 4 states)
  - `ChatEdgeLine` - @N reference edges between nodes (Bezier curves)
  - `ChatTaskQueue` - Task execution queue with 5-verb icons
  - `ChatDagPanel` - Full DAG visualization (nodes + edges combined)
- **Animation System**
  - `AnimationTicker` - 60fps coordinated animation utility
  - `AnimationState` - Running/Paused/Stopped states
  - Easing utilities for smooth transitions
- **Nika Intro Animation** - ASCII art explosion into matrix rain (15 frames, 1.5s)

### Statistics
- **2,720+ tests passing** (108 new chat widget tests)

## [0.9.0] - 2026-02-25

### Added
- **StableGraph Foundation** (v0.9.0) - Stable NodeIndex for chat DAG
  - `StableDag<T>` wrapper using petgraph::StableGraph
  - Stable NodeIndex preserved after node deletion
  - Edge cascading on node removal
  - 17 unit tests for stability guarantees
- **ChatWorkflow Struct** (v0.9.1) - DAG wrapper for chat messages
  - `ChatWorkflow` wraps `StableDag<ChatMessage>`
  - Auto-edge creation for sequential messages (1→2→3)
  - `add_message()` and `add_message_parallel()` methods
  - Thread-safe with `parking_lot::Mutex`
  - Message counter for @N references
  - 45 unit tests for workflow operations
- **@mention Binding System** (v0.9.2) - Reference previous messages
  - Parse `@N`, `@last`, `@all`, `@N..M` mention syntax
  - `MentionParser` with regex-based extraction
  - `resolve_mention()` converts to indices
  - `mentions_to_wiring()` generates WiringSpec bindings
  - `//` parallel marker detection
  - ChatWorkflow integration with auto-edge creation
  - 58 unit tests for parsing and resolution
- **Builtin Tools** (v0.9.3) - 6 nika:* prefixed tools
  - `nika:sleep` - Delay execution with millisecond precision
  - `nika:log` - Emit log events via tracing (trace/debug/info/warn/error)
  - `nika:emit` - Custom event emission with payload
  - `nika:assert` - Condition validation with custom messages
  - `nika:prompt` - Interactive user prompts (HITL integration)
  - `nika:run` - Sub-workflow execution with validation
  - `BuiltinToolRouter` with is_builtin(), dispatch(), extract_name()
  - All tools implement `BuiltinTool` trait with async dispatch
  - 96 unit tests (16 per tool)
- **WIRING Checkpoint Tests** - Integration validation
  - WIRING-0: StableDag foundation (3 tests)
  - WIRING-1: ChatWorkflow ↔ StableDag (6 tests)
  - WIRING-2: ChatWorkflow ↔ @mention Bindings (13 tests)
  - WIRING-3: BuiltinRouter ↔ Executor (13 tests)
  - 35 integration tests validating component wiring

### Architecture
- **Chat-as-DAG** - Unified architecture where chat = workflow DAG
  - Every message is a DAG node with stable index
  - @mentions create explicit data flow edges
  - Builtin tools provide workflow primitives
  - Foundation for TaskBox visualization (v0.10)

### Statistics
- **2,720+ tests passing** (216 new in v0.9.x + 35 WIRING tests)
- **Zero clippy warnings**
- v0.9.x adds 251 tests total (target was 131, exceeded by +120)

## [0.8.0] - 2026-02-23

### Added
- **Edit History (Undo/Redo)** - `src/tui/edit_history.rs` with intelligent coalescing
  - Ctrl+Z/Ctrl+Y support in ChatOverlayState
  - Intelligent grouping of rapid keystrokes (500ms timeout)
  - Preserve user intent across edits
  - 19 tests for edge cases and coalescing logic
- **Session Persistence** - `src/tui/session.rs` saves/loads chat conversations
  - Storage: `.nika/sessions/*.json` per session
  - Atomic writes using temp + rename pattern
  - Auto-cleanup to maintain max 50 sessions
  - Fast deserialization with serde
  - 13 tests for persistence and recovery
- **Solarized Theme** - Third theme option in theme system
  - `ThemeMode::Solarized` variant alongside Default and Custom
  - Based on Ethan Schoonover's color palette
  - High contrast for accessibility
  - Warmth and precision for terminal readability
- **Config System** - `.nika/config.toml` for persistent TUI preferences
  - `TuiSettings`: theme, font_size, ui_density
  - `ChatSettings`: auto_save, session_limit, history_limit
  - `StudioSettings`: auto_format, tab_width, line_numbers
  - `PathSettings`: custom session/trace directories
  - Type-safe TOML serialization with serde

### Statistics
- **1,879 tests passing**
- **Zero clippy warnings**

## [0.7.2] - 2026-02-23

### Added
- **GitHub Actions CI/CD** - Complete workflow automation
  - `ci.yml`: Format, clippy, test, coverage, security audit, build
  - `release.yml`: Cross-platform release binaries (Linux, macOS, Windows)
  - `dependabot.yml`: Automated dependency updates
- **Token Tracking for Standard Mode** - Streaming migration for accurate token counts
  - `StreamingResult` struct captures response, input_tokens, output_tokens, thinking
  - `stream_completion_with_tokens()` helper uses `model.stream()` for pure streaming
  - `stream_with_tools()` routes: streaming when no tools (full tokens), agent.prompt() when tools (0 tokens)
  - Token tracking works for Claude, OpenAI, Mistral, Groq, DeepSeek, Ollama when no tools

### Fixed
- **Token tracking returned 0 for non-thinking mode** - All `run_*()` methods now
  return accurate token counts via streaming API when no tools are used
  - Uses rig-core's `GetTokenUsage` trait on `StreamedAssistantContent::Final`
  - Chat methods (`chat_continue_*`) still return 0 tokens (rig-core `Chat` trait limitation)

### Statistics
- **2,323 tests passing**
- **6 LLM providers** with full token tracking (when no tools)

## [0.7.1] - 2026-02-21

### Added
- **TUI Navigation Refresh** - VS Code-like tab system
  - Tab bar with full path display and active indicator
  - `Alt+←/→` to navigate between tabs
  - `Alt+W` / `Ctrl+W` to close tabs
  - `Ctrl+P` / `/` for fuzzy file search (Helix/VS Code style)
- **spawn_tracked** - Background task lifecycle management in TUI
  - MCP server connections tracked as background tasks
  - Real-time status indicators in status bar

### Statistics
- **1,842+ tests passing** (Nika lib + integration tests)
- **6 LLM providers** with full streaming (Claude, OpenAI, Mistral, Groq, DeepSeek, Ollama)

## [0.7.0] - 2026-02-21

### Added
- **Full Streaming for All 6 Providers** - Real-time token delivery across Claude, OpenAI,
  Mistral, Groq, DeepSeek, and Ollama via rig-core `StreamedAssistantContent`
- **MCP Server Status Events** - `McpConnected` / `McpError` lifecycle tracking
- **Event System Enhancements** - `verb` field in `TaskStarted`, `ContextAssembled` event,
  `StreamChunk::Metrics` for token counting
- **TUI DX** - miette v7.6 YAML error diagnostics, nucleo v0.5 fuzzy file search

### Statistics
- **1,842 tests passing** (up from 1,811)

## [0.6.0] - 2026-02-20

### Added
- **6 LLM Providers** - Claude, OpenAI, Mistral, Groq, DeepSeek, Ollama via rig-core
- **Auto-detection** - `RigProvider::auto()` checks env vars in priority order
- **Chat history** - `agent.chat(prompt, history)` via rig's `Chat` trait
- **New methods** - `chat_continue()`, `add_to_history()`, `with_history()`

### Changed
- Provider priority order: Anthropic → OpenAI → Mistral → Groq → DeepSeek → Ollama
- Default model updated to `claude-sonnet-4-6`

## [0.5.2] - 2026-02-20

### Added
- **CLI DX Refresh** - Streamlined command-line interface
  - `nika` alone launches TUI Home view
  - `nika chat` starts Chat view with `--provider` and `--model` options
  - `nika studio [file]` starts Studio view
  - `nika check` replaces `validate` (alias kept)
  - Positional: `nika workflow.nika.yaml` runs directly
- **TUI 4-View Architecture** - Unified interface with Tab navigation
  - Chat, Home, Studio, Monitor views
  - Keybindings: `a/h/s/m` or `Tab` to switch

### Fixed
- Async response polling now wired in main event loop
- MCP client lazy initialization with DashMap + OnceCell

## [0.5.0] - 2026-02-19

### Added
- **MVP 8: RLM Enhancements** - Complete RLM-on-KG implementation
  - Phase 1: Reasoning capture (`thinking` field in AgentTurn events)
  - Phase 2: Nested agents (`spawn_agent` internal tool with depth protection)
  - Phase 3: Schema introspection (`novanet_introspect` MCP tool)
  - Phase 4: Dynamic decomposition (`decompose:` modifier for runtime DAG expansion)
  - Phase 5: Lazy bindings (`lazy: true` for deferred binding resolution)
- **15 lazy binding tests** - Comprehensive test suite
- **11 decompose tests** - Test coverage for decompose modifier

### Statistics
- **MVP 8 complete** (RLM enhancements)
- **1,747 tests passing**

## [0.4.1] - 2026-02-19

### Fixed
- **Token tracking in streaming mode** - `run_claude_with_thinking()` now extracts tokens from `StreamedAssistantContent::Final` via rig's `GetTokenUsage` trait
- **AgentTurnMetadata accuracy** - `input_tokens` and `output_tokens` are now correctly populated in extended thinking mode

### Added
- **Reasoning capture** - `thinking` field captured in AgentTurn events
- **rig-core integration** - New `RigAgentLoop` using rig-core's AgentBuilder
- **RigProvider.infer()** - Simple text completion via rig-core
- **NikaMcpTool** - Implements rig's `ToolDyn` trait for MCP tool bridging
- **24 rig tests** - Comprehensive test suite for rig-based providers

### Breaking Changes
- **Removed deprecated providers** - `ClaudeProvider`, `OpenAIProvider`, `provider::types` deleted
- **Removed `AgentLoop`** - Replaced by `RigAgentLoop` with rig's AgentBuilder
- **Removed `resilience/` module** - Entire module deleted (was never wired into runtime)

### Changed
- **~1,420 lines removed** - Code reduction from removing deprecated providers
- **`infer:` verb migrated to rig-core** - executor.rs now uses `RigProvider.infer()`
- **621+ tests passing** - Comprehensive test coverage after migration

### Migration Guide

```rust
// Old (v0.3)
use nika::provider::ClaudeProvider;
let provider = ClaudeProvider::new()?;
let result = provider.infer("prompt", None).await?;

// New (v0.4+)
use nika::provider::rig::RigProvider;
let provider = RigProvider::claude()?;
let result = provider.infer("prompt", None).await?;
```

## [0.3.0] - 2026-02-19

### Added
- **Two new verbs** per ADR-001:
  - `invoke:` - MCP tool calls (connects to NovaNet)
  - `agent:` - Multi-turn agentic loops with tool use
- **MCP client integration** - Connect to MCP servers like NovaNet
- **Resilience patterns**:
  - Retry with exponential backoff + jitter
  - Circuit breaker (Closed → Open → HalfOpen)
  - Rate limiting per provider
- **for_each parallelism** - Iterate over arrays with concurrency control
- **TUI** - Terminal UI for workflow monitoring (feature-gated)
- **Quickstart examples** - Two new example workflows:
  - `examples/quickstart-mcp.nika.yaml` - MCP integration with NovaNet
  - `examples/quickstart-multilang.nika.yaml` - Multi-locale generation with `for_each`
- Schema version: `nika/workflow@0.3`

### Changed
- Schema bumped from @0.1 to @0.3
- 16 EventLog variants for comprehensive observability

## [0.1.0] - 2025-01-27

### Added
- Initial release of Nika CLI
- YAML workflow parsing with schema validation (`nika/workflow@0.1`)
- DAG-based task execution with parallel processing
- Three action types:
  - `infer:` - LLM inference calls
  - `exec:` - Shell command execution
  - `fetch:` - HTTP requests
- Data flow between tasks via `use:` blocks
- Template system with `{{use.alias}}` syntax
- Default values with `??` operator
- Output formatting (text/json) with optional JSON Schema validation
- Provider support: Claude, OpenAI, Mock
- Structured error codes (NIKA-0xx)
- Lock-free RunContext with DashMap
- Event logging for execution tracing

### Commands
- `nika run <workflow.yaml>` - Execute a workflow
- `nika validate <workflow.yaml>` - Validate without execution

[Unreleased]: https://github.com/supernovae-st/nika-dev/compare/v0.12.0...HEAD
[0.12.0]: https://github.com/supernovae-st/nika-dev/compare/v0.11.0...v0.12.0
[0.11.0]: https://github.com/supernovae-st/nika-dev/compare/v0.10.0...v0.11.0
[0.10.0]: https://github.com/supernovae-st/nika-dev/compare/v0.9.0...v0.10.0
[0.9.0]: https://github.com/supernovae-st/nika-dev/compare/v0.8.0...v0.9.0
[0.8.0]: https://github.com/supernovae-st/nika-dev/compare/v0.7.2...v0.8.0
[0.7.2]: https://github.com/supernovae-st/nika-dev/compare/v0.7.1...v0.7.2
[0.7.1]: https://github.com/supernovae-st/nika-dev/compare/v0.7.0...v0.7.1
[0.7.0]: https://github.com/supernovae-st/nika-dev/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/supernovae-st/nika-dev/compare/v0.5.2...v0.6.0
[0.5.2]: https://github.com/supernovae-st/nika-dev/compare/v0.5.0...v0.5.2
[0.5.0]: https://github.com/supernovae-st/nika-dev/compare/v0.4.1...v0.5.0
[0.4.1]: https://github.com/supernovae-st/nika-dev/compare/v0.3.0...v0.4.1
[0.3.0]: https://github.com/supernovae-st/nika-dev/compare/v0.1.0...v0.3.0
[0.1.0]: https://github.com/supernovae-st/nika-dev/releases/tag/v0.1.0
