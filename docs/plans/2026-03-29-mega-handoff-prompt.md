# v0.50.0 Mega Handoff — Security Fixes + Release + Phase 1 Prep

**Date**: 2026-03-29
**Version**: v0.50.0 (workspace bumped, NOT tagged yet)
**Tests**: 4509+ passing, 0 failures
**Branch**: main

---

## What's Done (this session)

- `preset:` field — full pipeline (parser → analyzer → lower → runner), 6/6 E2E PASS
- `retry:` on all verbs + TaskRetry event + display rendering
- 22 workflows fixed ($prefix, model, to_yaml, typos)
- VS Code extension: snippets fixed, schema URL removed, model in template, showOutput registered
- JSON schema: preset added, both copies synced
- validate() preset validation added (was missing)
- o3 pricing synced ($10→$2)
- Newline injection blocked (CRITICAL security fix)
- 4 gate tests for preset
- CHANGELOG v0.50.0

---

## 5 Security Fixes Before Release Tag

### Fix 1: api_key Debug leak (10 min)
**File**: `tools/nika-engine/src/provider/endpoints.rs`
**Lines**: 15, 48
**What**: Replace `#[derive(Debug)]` on `CustomEndpointConfig` and `ResolvedEndpoint` with manual Debug impl that masks api_key using existing `mask_api_key()` from `secrets/keyring.rs:228`.
```rust
impl fmt::Debug for ResolvedEndpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResolvedEndpoint")
            .field("name", &self.name)
            .field("base_url", &self.base_url)
            .field("api_key", &"***")
            .field("default_model", &self.default_model)
            .finish()
    }
}
```

### Fix 2: IPv6 SSRF in endpoint validation (15 min)
**File**: `tools/nika-engine/src/provider/endpoints.rs:70-112`
**What**: Add IPv6-mapped metadata check. Currently only checks IPv4. Add:
```rust
// After Ipv4Addr check, add:
if let Ok(v6) = host.parse::<std::net::Ipv6Addr>() {
    // Check IPv4-mapped: ::ffff:169.254.169.254
    if let Some(v4) = v6.to_ipv4_mapped() {
        if v4.is_link_local() { return Err(...) }
    }
    // Block link-local fe80::/10
    if (v6.segments()[0] & 0xffc0) == 0xfe80 { return Err(...) }
    // Block ULA fc00::/7
    if (v6.segments()[0] & 0xfe00) == 0xfc00 { return Err(...) }
}
```

### Fix 3: DNS rebinding SSRF (30 min)
**File**: `tools/nika-engine/src/runtime/policy.rs` + `executor/mod.rs`
**What**: reqwest doesn't have IP-level DNS callback. Two options:
- **Option A (recommended)**: Use `trust-dns-resolver` to pre-resolve hostname, check resolved IP against SSRF blocklist BEFORE sending request. ~50 LOC.
- **Option B (minimal)**: Document as known limitation. Add comment in policy.rs. Not a fix but honest.
- **Option C**: Use reqwest's `hickory-dns` feature + custom resolver that checks IPs.

Best approach: Option A with a `resolve_and_check_ssrf()` helper called before `http_client.execute()` in fetch.rs.

### Fix 4: Response size streaming limit (30 min)
**File**: `tools/nika-engine/src/runtime/executor/fetch.rs`
**What**: Replace `response.text().await` (buffers entire body) with streaming reader:
```rust
async fn read_response_with_limit(response: Response, max_bytes: u64) -> Result<String, NikaError> {
    let mut stream = response.bytes_stream();
    let mut buffer = Vec::with_capacity(max_bytes.min(1_048_576) as usize);
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| NikaError::FetchError { reason: e.to_string() })?;
        if buffer.len() as u64 + chunk.len() as u64 > max_bytes {
            return Err(ExecutionError::FetchFailed {
                reason: format!("Response exceeded {} byte limit", max_bytes),
            }.into());
        }
        buffer.extend_from_slice(&chunk);
    }
    String::from_utf8(buffer).map_err(|e| NikaError::FetchError { reason: e.to_string() })
}
```
Replace 3 calls: lines 331, 493, 517. Also add `use futures::StreamExt;`.

### Fix 5: on_limit_reached.action (CRITICAL, 30 min)
**File**: `tools/nika-engine/src/runtime/rig_agent_loop/providers.rs`
**What**: `LimitsConfig.on_limit_reached.action` is parsed but NEVER read. All limit-exceeded paths unconditionally return partial result. Need to:
1. Read `self.limit_tracker` config in each limit-exceeded branch
2. Match on `LimitAction::CompletePartial` → current behavior (return partial)
3. Match on `LimitAction::Fail` → return `Err(NikaError::AgentLimitExceeded { ... })`
4. Match on `LimitAction::Escalate` → emit escalation event, then fail

Locations to modify (all in providers.rs):
- Claude: ~line 168 (MaxTurnsReached), ~line 180 (TokenBudgetExceeded)
- OpenAI: ~line 505, ~line 517
- Generic: ~line 960, ~line 972

Add to `limit_tracker.rs`:
```rust
pub fn on_limit_action(&self) -> &LimitAction {
    &self.config.on_limit_reached.action
}
```

---

## Preset Unit Tests (10 tests)

### Parser (nika-core/src/ast/raw/parser.rs)
1. `test_parse_task_with_preset` — YAML with `preset: assistant` parses
2. Update `test_known_task_keys_with_verb_no_error` — add preset to YAML

### Analyzer (nika-core/src/ast/analyzer/analyze.rs)
3. `test_analyze_valid_preset_reference` — preset: assistant with agents: block passes
4. `test_analyze_invalid_preset_undefined` — preset: ghost → NIKA-144
5. `test_analyze_preset_missing_agents_block` — preset without agents → NIKA-144
6. `test_analyze_preset_exempts_missing_model` — infer+preset but no model passes
7. `test_analyze_multiple_tasks_with_presets` — 2 tasks, 2 presets

### Runner (nika-engine/src/runtime/runner.rs)
8. `test_preset_resolves_provider_model` — mock workflow with preset
9. `test_preset_task_override_wins` — task provider beats preset
10. `test_preset_injects_system_temperature` — system + temp from preset

---

## Release v0.50.0 Checklist

### Pre-tag
- [ ] 5 security fixes committed
- [ ] 10 preset unit tests passing
- [ ] cargo check + clippy + test = clean
- [ ] CHANGELOG.md updated

### Secrets to verify (GitHub Settings → Secrets)
- VSCE_PAT (may need renewal — Azure DevOps PATs expire yearly)
- OVSX_PAT (Open VSX for Cursor)
- NPM_TOKEN
- CARGO_REGISTRY_TOKEN
- HOMEBREW_TAP_TOKEN
- APPLE_* secrets (macOS notarization)
- DOCKERHUB_USERNAME + DOCKERHUB_TOKEN

### Tag
```bash
git tag v0.50.0
git push origin v0.50.0
```

### Monitor (pipeline takes ~15-20 min)
- GitHub Actions → release.yml
- VS Code Marketplace: nika-lang
- npm: @supernovae/nika
- Docker Hub: supernovae/nika

---

## Phase 1 Status (from plan analysis)

| Phase | Status | Blocker |
|-------|--------|---------|
| P-MODEL (preset:) | **80% done** | Unit tests + fallback chains |
| P-RECORD | 0% | Needs P-MODEL complete |
| P-ORCHESTRATE | 0% | Needs P-RECORD |
| P-CONTEXT | 0% | Needs P-ORCHESTRATE |
| P-MEMORY | 0% | Needs P-CONTEXT |

### Key insight from plan review:
- `preset:` field IS the P-MODEL foundation — it's done
- Next P-MODEL work: provider fallback chains (`provider: [groq, anthropic]`), `nika:cost` tool
- P-RECORD, P-ORCHESTRATE, P-CONTEXT are sequential — 3 weeks each
- Total Phase 1: ~10-12 weeks (on track with master plan's 10-week estimate)

### Agent verb known limitations (document, don't fix for v0.50):
- LLM guardrails (`type: llm`) silently skipped — `run_guardrails_async` never implemented
- `extended_thinking: true` is single-turn only (no tools, no multi-turn loop)
- `on_limit_reached.action` ignored (fix in Wave 1 above)
- Quadratic prompt growth on retries (truncate in v0.51)

---

## 78-Bug Summary (from 20 agents across 2 waves)

| Severity | Total | Fixed | Remaining |
|----------|-------|-------|-----------|
| CRITICAL | 3 | 2 | 1 (#3 on_limit_reached) |
| HIGH | 9 | 0 | 9 (5 in Wave 1 above) |
| MEDIUM | 23 | 3 | 20 |
| LOW | 29 | 0 | 29 |

---

## Commit Strategy

```
fix(security): mask api_key in Debug derives for endpoint structs
fix(security): block IPv6-mapped metadata endpoints in SSRF validation
fix(security): add streaming response size limit to fetch verb
fix(runtime): respect on_limit_reached.action in agent limit checks
test(preset): add 10 unit tests for preset: field across parser/analyzer/runner
```

Then: `git tag v0.50.0 && git push origin v0.50.0`
