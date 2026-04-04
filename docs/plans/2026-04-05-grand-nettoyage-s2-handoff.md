# Grand Nettoyage — S2+ Handoff

> Generated 2026-04-05 from 17-commit session + 9-agent deep audit.
> Use this as the opening prompt for the next session.

---

## CURRENT SESSION: S4+ — Provider Polish, LSP, Remaining Coverage

**Previous**: S1 Security (10 commits) + S2 for_each (-544 LOC) + S3 Stabilization (8 commits)
**Total commits this sprint**: 25
**Tests**: 9,847 → 9,909 (+62)
**Runner LOC**: 8,252 → 7,708

### S3 Session Results (8 commits)
- **W1**: Fix help text 29→52 transforms, fix 14 verbs test failures (vault+mutex), accept snapshot drift
- **W2**: Emit PolicyBlocked events in exec.rs (3 locations) and fetch.rs (3: SSRF, 2x CRLF). 14 "dead" variants verified ALIVE (earlier audit was wrong)
- **W3**: RigProvider::Mock enum variant added, mock provider in catalog (requires_key: false)
- **W4**: Budget size limit enforced before YAML parse (DoS fix), 3 unwrap() eliminated via @ binding
- **W5**: nika-napi/nika-py removed from workspace (no more --exclude), 6 infer.rs unit tests added

---

## What's DONE (S1 + S2)

### S1 Security Hardening (COMPLETE)
- Shell quote-breakout fix (SEC-2b)
- +10 blocklist entries (command, builtin, nohup, nice, timeout, strace, source, coproc, /dev/tcp, /dev/udp)
- BASH_ENV/ENV blocked in env vars
- +6 shell injection patterns (&&, ||, ;, >, >>, |)
- Percent-encoded traversal blocked in serve
- Artifact symlink → validate_canonicalized_boundary
- KDF documented (audit was WRONG — 64 MiB not KB)
- auth.rs: 0 → 11 tests
- Edge case tests for contains_unquoted, basename, dequoting

### S2 Engine Decomposition (PARTIAL)
- for_each.rs extracted: 4 binding formats → 1 unified resolver (-544 LOC from runner.rs)
- Dead code cleanup: incorrect #[allow(dead_code)] removed, RAII fields use _ prefix
- **REMAINING**: scheduler extraction, DAG unification (deferred — lower priority than stabilization)

---

## What's LEFT (prioritized for May 5 launch)

### P0 — Ship Blockers

#### ~~Fix Integration Test Drift (NIKA-034)~~ ✅ S3
Snapshot drift accepted (2 snapshots: improved error messages).

#### ~~Fix Pre-existing CLI Test Failures~~ ✅ S3
14 verbs test failures fixed (vault leak + mutex poison cascade).

### P1 — Stability & Observability

#### Telemetry Gaps (26 findings from agent audit)
**Emit events BEFORE returning errors** in these paths:
- Template resolution failures (infer.rs, fetch.rs) — no event, only TaskFailed after
- CRLF injection detection (fetch.rs:337-351) — security block with no event
- Domain rate limiter delays (fetch.rs:208-215) — no event to explain slowness
- MCP tool size limit violations (invoke.rs:231-239) — rejected without event
- Schema file loading failures (infer.rs:157-186) — invisible in traces

**Add missing event types:**
- `SecurityBlocked` — for all NIKA-053 blocks (currently only logged via tracing)
- `RateLimitDelayed` — for domain rate limiter
- `ProviderInitFailed` — for bootstrap failures
- `TaskRetry` — exists but NEVER emitted

**Fix dead event type:**
- `StructuredOutputTimeout` — defined but never emitted anywhere

#### Test Coverage (highest risk gaps)
| File | LOC | Tests | Risk |
|------|-----|-------|------|
| `runtime/executor/infer.rs` | 1,798 | 0 | CRITICAL — retry logic, structured output |
| `display/renderer.rs` | 1,593 | 0 | HIGH — rendering bugs invisible |
| `rig_agent_loop/providers.rs` | 746 | 0 | HIGH — provider fallback |
| `rig_agent_loop/streaming.rs` | 676 | 0 | HIGH — stream handling |
| CLI: workflow.rs, mcp.rs, pkg.rs, schema.rs | ~2,350 | 0 | MEDIUM |

### P2 — Architecture & Polish

#### Provider Layer (S3)
- Mock is string sentinel `if provider_name == "mock"` — make it a proper enum variant
- OpenAiCompat raw HTTP logic copied 3x — extract helper
- ModelResolver bypassed in agent loop — wire it everywhere
- Cost estimation in CLI uses fixed $0.003/infer — use ModelPricing

#### Structured Output (S4)
- L1 ghost: `enable_extractor` silently accepted, never implemented → NIKA-010 error
- Schema/validator cache: clear-all on cap exceeded → LRU
- L0 fallback: transport error skips L0b → should try L0b
- L0b token counts: uses estimate_tokens() → real provider counts

#### CLI UX (S6)
- ~~Help text says "29 transforms" → actual is 52~~ ✅ S3
- `bench`, `explain`, `switch`, `vault`, `clean` missing from get_short_desc/get_example
- config get/set uses raw TOML string manipulation → typed struct
- `curl` subprocess for remote download → reqwest

#### LSP (from 5-agent audit)
- **P0: No diagnostics handler** — `textDocument/publishDiagnostics` unimplemented. error_ranges from parser never converted to LSP Diagnostics. Create `handlers/diagnostics.rs`.
- **P0: YAML bomb size limit** — Budget exists but not enforced before tree-sitter parse. DoS risk. Call `Budget::from_str()` before parse, 1 MB default.
- **P1: 150+ unwrap() in parser** — `nika-core/src/ast/raw/parser.rs` lines 1908, 1930, 1948. Create `safe_get_required()` helper.
- **P1: Generic error messages** — "expected string" with no suggestion. Add `suggestion` field to ParseError.
- **P2: Dual LSP implementations** — nika-lsp-core (9K) + embedded (12K) = 24K LOC maintenance burden.
- **NOTE: AST sync is CORRECT** — nika-engine properly re-exports from nika-core, no divergence found.
- **NOTE: 12/13 handlers fully implemented** with 180+ test cases. Only diagnostics + formatting missing.

#### Dead Code (S11)
- ~~nika-napi, nika-py still in workspace (link failures)~~ ✅ S3 — removed from workspace
- 4 unused deps: tokio-stream (nika-sdk, nika-serve), dirs (nika-mcp), serde (nika-napi)
- .nika/config.toml legacy fallback in 5 files
- JsonQueryTool deprecated but still registered

---

## 9-Agent Audit Findings (4 from S1 + 5 from S2)

### LSP + AST (S2 agent)
- **P0**: No `textDocument/publishDiagnostics` handler — error_ranges from parser never converted to LSP Diagnostics
- **P0**: YAML bomb size limit not enforced before tree-sitter parse (DoS risk, 1 MB default needed)
- **P1**: 150+ `unwrap()` in `nika-core/src/ast/raw/parser.rs` — crash risk on malformed YAML
- **P1**: Generic error messages ("expected string") without suggestions
- **P2**: Dual LSP implementations (nika-lsp-core 9K + embedded 12K = 24K LOC maintenance burden)
- **OK**: AST sync is CORRECT — nika-engine properly re-exports from nika-core
- **OK**: 12/13 handlers implemented with 180+ tests. Only diagnostics + formatting missing.

### Telemetry (S1+S2 agents — COMPLETE, 85 EventKind variants audited)
- **14 dead event variants** (defined but never emitted): BindingDefaultApplied, BindingEnvResolved, BindingTransformApplied, BindingVaultResolved, BudgetExceeded, BudgetOk, FallbackTriggered, McpConnected, McpError, McpRetry, NativeModelLoaded, PresetApplied, WorkflowPaused, WorkflowResumed
- **7 silent error paths** (return Err without event): SSRF redirect block (fetch.rs:516), domain rate limit (fetch.rs:208), URL validation (fetch.rs:66), stdout truncation (exec.rs:352), shell injection bindings (exec.rs:67,82), shell data injection (exec.rs:98), vision resolution (infer.rs:1570)
- **CRITICAL**: SSRF blocks emit NO PolicyBlocked event (fetch.rs:271-280, 516-521) — invisible to TUI
- **CRITICAL**: Shell injection detection emits NO event (exec.rs:67,82,98) — security blocks invisible
- **Verb asymmetry**: TemplateResolved emitted by exec/fetch/infer but NOT invoke/agent. PolicyBlocked emitted by exec/fetch but NOT invoke/agent. McpRetry variant exists but never emitted.
- **Suggested new variants**: `RateLimitApplied` (domain fetch), `OutputTruncated` (exec stdout)
- **Fix pattern**: Emit `PolicyBlocked` event BEFORE every `return Err(BlockedCommand/PolicyViolation)` in executor paths
- **Coverage**: 71/85 variants emitted (84%), 12/19 error paths have events (63%)

### Crate Structure (S2 agent — COMPLETE)
- **OK**: cargo machete found ZERO unused deps (earlier audit was wrong about tokio-stream/dirs)
- **P1**: 3x duplicate `find_project_root` — nika-cli, nika-engine/mcp_config, nika-tui → extract to nika-core
- **P1**: 4x duplicate `format_duration` in nika-tui → extract to nika-tui/src/util/format.rs
- **P1**: deny.toml only in nika/ crate, not workspace root — move to tools/
- **P1**: RUSTSEC-2024-0436 advisory ignore needs upstream link + review date
- **P2**: nika-engine 168K LOC (60% of workspace) — consider nika-builtins + nika-display extraction
- **P2**: media-chart, media-pdf could move to Tier 3 (opt-in) for faster builds

### CLI UX (S2 agent — COMPLETE)
- **P0**: Help says "29 transforms" → actual is **56** (`help.rs:203,488`)
- **P1**: 10 subcommands missing from get_short_desc/get_example (explain, bench, vault, clean, tools, help, switch, serve, cosmic, lsp)
- **P1**: 57 error variants have #[diagnostic] but NO help() text — add actionable suggestions
- **P1**: 8 CLI subcommand files with 0 tests (pkg, schema, workflow, init, jobs, new_cmd, cache_cmd, tools_cmd)
- **P2**: Cost estimation uses fixed $0.003/infer → use ModelPricing (2-5x variance)
- **P2**: No fuzzy matcher for mistyped commands ("nika workkflow" → no suggestion)

### Provider + Structured Output (S2 agent — COMPLETE)
- **P1**: OpenAiCompat raw HTTP duplicated 2x (text path vs tools path) → extract helper
- **P1**: Mock is string sentinel `if provider_name == "mock"` → add `RigProvider::Mock` variant
- **P1**: Agent loop hardcodes model defaults (run_mistral, etc.) bypassing ModelResolver
- **P1**: `RigInferError` too coarse — uses string parsing for 401/429/5xx → align with `ProviderVerifyError`
- **P1**: XAi provider variant has no dedicated test
- **P2**: L1 ghost: `enable_extractor` never referenced → remove from docs or implement
- **P2**: L0 transport error fallback to L0b not documented
- **OK**: Cost tracking correctly handles cached responses (no double-counting)
- **OK**: LRU cache not needed — schema compilation is <1ms per call

### Test Coverage (S1 agent)
| File | LOC | Tests | Priority |
|------|-----|-------|----------|
| `runtime/executor/infer.rs` | 1,798 | 0 | **P0** |
| `display/renderer.rs` | 1,593 | 0 | P1 |
| `rig_agent_loop/providers.rs` | 746 | 0 | P1 |
| `rig_agent_loop/streaming.rs` | 676 | 0 | P1 |
| `rig_agent_loop/thinking.rs` | 624 | 0 | P1 |
| CLI: workflow/mcp/pkg/schema | ~2,350 | 0 | P2 |

### Security (S1 agent — remaining gaps, intentionally deferred)
- `exec ` (bash builtin) — too many false positives (docker exec, kubectl exec)
- `. ` (dot-source) — too broad (matches period-space everywhere)
- `trap ` — potential deferred execution but common word
- `<<` (heredoc) — too many false positives (bit shifts), covered by existing blocklist

---

## Workflow Per Session

```
1. Read this handoff
2. For each fix:
   a. Read the target file(s)
   b. Write test FIRST (TDD)
   c. Run test → verify it FAILS
   d. Implement the fix
   e. Run test → verify it PASSES
   f. cargo test --workspace --lib --exclude nika-py
   g. git add <specific files>
   h. git commit
3. After all fixes: cargo clippy --workspace --exclude nika-py
4. Push: git push
5. Update this handoff
```

## Commit Format

```
type(scope): concise description

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

## Rules

- Test BEFORE commit. No exceptions.
- 1 fix = 1 commit.
- `cargo test --workspace --lib --exclude nika-py` (always --lib, exclude nika-py link failure)
- Zero backward compat (v0 = 0 users)
- AGPL-3.0-or-later on all crates
- 10 pre-existing nika-cli verbs test failures — ignore unless fixing them specifically
