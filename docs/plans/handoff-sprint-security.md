# Handoff: Security Sprint (~4h)

> Copy this file as first message in a new Claude Code session.

## Context
v0.54.0 has 10 security fixes from the overnight session. These are the REMAINING security items.

## Codebase
```
cd /Users/thibaut/dev/supernovae/nika
cargo test --workspace --lib  # 9057 pass, 0 fail
```

## Items

### 1. SEC-AGENT-01: Agent bypasses security policies (~4h)
**File:** `nika-engine/src/runtime/rig_agent_loop/mod.rs`
**Bug:** Child agents spawned via `spawn_agent` don't inherit parent's PolicyEnforcer. A workflow with `allow_exec: false` can still exec via agent tool calls.
**Fix:** Thread `PolicyEnforcer` through `RigAgentLoop::new()` and check it before every tool call in the agent loop.
**Test:** Agent with `allow_exec: false` policy → agent tries `nika:read` → should succeed. Agent tries `exec` via tool → should fail.

### 2. MCP resource read size limit (~30min)
**File:** `nika-engine/src/runtime/executor/invoke.rs:211`
**Bug:** `MAX_MCP_RESULT_SIZE` (50MB) is only checked for tool calls, not resource reads. A malicious MCP server could return a 500MB resource.
**Fix:** Apply same 50MB check to resource read results.

### 3. Unicode blocklist bypass (~2h)
**File:** `nika-engine/src/runtime/security.rs`
**Bug:** Fullwidth Unicode confusables (ｓｕｄｏ) and zero-width spaces (s​u​d​o) can bypass the command blocklist.
**Fix:** NFKC normalization is already applied via `normalize_for_blocklist()` — verify it handles fullwidth. Add zero-width char stripping. Add tests from companion doc section 1.5.

### 4. SSRF redirect DNS re-pinning (~1h)
**File:** `nika-engine/src/runtime/policy.rs`
**Bug:** After DNS pinning, HTTP redirects to a new hostname don't re-check SSRF. An attacker could use `safe.com → 302 → http://169.254.169.254/`.
**Fix:** reqwest `redirect::Policy::custom()` that re-checks each redirect hop.

## Verification
```bash
cargo test --workspace --lib
# Run G01-G07: ALL must fail
./tools/target/debug/nika run tests/e2e-overnight/G01-ssrf-private-ip.nika.yaml --no-live
```
