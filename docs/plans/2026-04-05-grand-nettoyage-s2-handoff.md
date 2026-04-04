# Grand Nettoyage — S2+ Handoff

> Generated 2026-04-05 from 17-commit session + 9-agent deep audit.
> Use this as the opening prompt for the next session.

---

## CURRENT SESSION: S3+ — Stabilization, Telemetry, Polish

**Previous**: S1 Security (10 commits) + S2 for_each extraction (-544 LOC) + dead code cleanup
**Total commits this sprint**: 17
**Tests**: 9,847 → 9,907 (+60)
**Runner LOC**: 8,252 → 7,708

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

#### Fix Integration Test Drift (NIKA-034)
~20 integration tests fail because `model:` is now required on infer/agent tasks.
Fix: add `model:` field to test fixtures, or use `provider: mock`.
```bash
cd tools && cargo test --test comprehensive_tests 2>&1 | grep FAILED
cargo insta review  # Accept snapshot drift
```

#### Fix Pre-existing CLI Test Failures
10 nika-cli verbs tests fail (env detection). Fix the tests, not the code.

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
- Help text says "29 transforms" → actual is 50
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
- nika-napi, nika-py still in workspace (link failures)
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

### Telemetry (S1+S2 agents, 26 findings)
- **Silent error paths**: Template resolution failures, CRLF injection, domain rate limiting — no events emitted
- **Missing event types**: `SecurityBlocked`, `RateLimitDelayed`, `ProviderInitFailed`
- **Dead event**: `StructuredOutputTimeout` defined but never emitted
- **`TaskRetry`**: exists in EventKind but never emitted anywhere
- **Verb asymmetry**: exec has duration_ms, FetchRetry has backoff_ms, McpRetry has NO duration field
- **Security events**: NIKA-053 blocks only logged via tracing, not emitted as events (TUI/serve can't display)
- **Fix pattern**: Emit event BEFORE `return Err(...)` in executor paths

### Crate Structure (S2 agent — pending full report)
- **P0**: nika-napi, nika-py still in workspace members (link failures on macOS)
- **P1**: 4 unused deps — tokio-stream (nika-sdk, nika-serve), dirs (nika-mcp), serde (nika-napi)
- **P1**: 3x duplicate `find_project_root` — nika-cli, nika-engine/mcp_config, nika-tui
- **P2**: nika-engine is 44% of codebase (168K LOC) — target for nika-builtins + nika-display extraction

### CLI UX (S2 agent — pending full report)
- **P1**: Help text says "29 transforms" → actual is 50 (`nika/src/cli/help.rs:496`)
- **P1**: `bench`, `explain`, `switch`, `vault`, `clean` missing from get_short_desc/get_example
- **P2**: Cost estimation uses fixed $0.003/infer → use ModelPricing tables
- **P2**: config get/set uses raw TOML string manipulation → typed struct
- **P2**: `curl` subprocess for remote workflow download → reqwest

### Provider + Structured Output (S2 agent — pending full report)
- **P1**: Mock is string sentinel `if provider_name == "mock"` → proper enum variant
- **P1**: OpenAiCompat raw HTTP logic 3x duplicated → extract helper (-200 LOC)
- **P1**: ModelResolver bypassed in agent loop → wire everywhere
- **P2**: L1 ghost: `enable_extractor` silently accepted → NIKA-010 error
- **P2**: Schema/validator cache clear-all-on-cap → LRU
- **P2**: L0 fallback: transport error skips L0b entirely → should try L0b

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
