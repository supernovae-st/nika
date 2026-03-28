# Session A: Security Hardening (~2-3h)

## Context
Nika workflow engine. Workspace: `tools/` (12 Rust crates). Main at `b1df0fda7`, 8613 tests.
Master plan: `docs/plans/2026-03-28-v051-master-quality-plan.md` — READ IT FIRST.

## Mission: Fix 8 security vulnerabilities found by deep audit

### Methodology
For EVERY fix: read code → write failing test → fix → verify → commit.
`cargo test --workspace --lib` (always --lib to avoid keychain popups).
1 fix = 1 commit. Conventional commits with co-authors.

---

### Bug 1: S1+S2 — Block shell -c variants in exec blocklist
**File**: `nika-engine/src/runtime/security.rs:28-97` (BLOCKLIST)
**Problem**: `python3 -c` only blocks `import socket`. `bash -c`, `zsh -c` not blocked.
**Fix**: Add generic `-c` patterns: `"python -c"`, `"python2 -c"`, `"python3 -c"`, `"bash -c"`, `"zsh -c"`, `"sh -c"`, `"dash -c"`, `"ksh -c"`, `"csh -c"`
**Tests**: Verify all blocked. Verify `"python3 script.py"` (no -c) is still allowed.

### Bug 2: SF1 — DNS failure defaults to ALLOW (must be BLOCK)
**File**: `nika-engine/src/runtime/policy.rs:105-112`
**Problem**: DNS fail/timeout → `false` (allow). Must be fail-closed.
**Fix**: Return `true` (block) on DNS errors. Upgrade `debug!` to `warn!`.
**Test**: Mock DNS failure, verify fetch is blocked.

### Bug 3: S5 — Template resolve Pass 3 missing trusted_inputs
**File**: `nika-engine/src/binding/template.rs:1177-1244`
**Problem**: No allowlist for inputs — injected `{{inputs.secret}}` resolves if any inputs ref exists.
**Fix**: Build `trusted_inputs: HashSet` from original template. Only resolve trusted paths.
**Test**: Inject `{{inputs.api_key}}` via LLM output → must NOT resolve.

### Bug 4: S6 — resolve_with lacks trusted_context
**File**: `nika-engine/src/binding/template.rs:494-566`
**Problem**: Same injection vector as Bug 3 but for context paths.
**Fix**: Port `trusted_context` pattern from `resolve` (line 1086-1096).

### Bug 5: M-sec1 — Block xargs, find -exec
**File**: `nika-engine/src/runtime/security.rs` BLOCKLIST
**Fix**: Add `"find -exec"`, `"find -delete"`, `"xargs "`
**Test**: Verify blocked.

### Bug 6: S3+S4 — SSRF redirect + DNS rebinding
**File**: `nika-engine/src/runtime/executor/mod.rs:128-151`
**Problem**: Redirect targets string-checked only. DNS rebinding TOCTOU.
**Investigate**: Can `reqwest::ClientBuilder::resolve()` pin IPs? If too complex, document + test.
**This is the hardest. Do last.**

### Bug 7: SF5 — Schema validator silently disabled by .ok()
**File**: `nika-engine/src/runtime/runner.rs:656`
**Problem**: Invalid schema → `.ok()` → None → no validation at all.
**Fix**: Return `NikaError` on invalid schema instead of silently disabling.

### Bug 8: M-sec4 — redact_for_event doesn't redact API key patterns
**File**: `nika-engine/src/runtime/executor/verbs.rs:95-106`
**Fix**: Add regex for `sk-*` and `Bearer *` patterns before truncation.

---

## After All Fixes
1. `cargo test --workspace --lib` — ALL pass
2. `cargo clippy --workspace -- -D warnings` — 0 warnings
3. `git push`

## Commit format
```
fix(security): description

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```
