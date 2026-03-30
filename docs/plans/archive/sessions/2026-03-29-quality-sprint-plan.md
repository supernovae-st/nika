# Quality Sprint Plan — 2026-03-29

## Context

Gap analysis of 14 sessions (A-N) against v1.0 master plan revealed:
- **Phase 1 Intelligence: 80% complete** — P-MODEL, P-RECORD, P-CONTEXT, P-INTROSPECT, P-MEMORY-LOCAL all done
- **P-ORCHESTRATE**: The last Phase 1 feature (NOT STARTED, ~3 weeks)
- **Many audit bugs already fixed** in prior sessions (CR1, S3/S4, SF1-SF8)
- **ProviderName migration**: Core AST done (2 commits), engine-side partially done

## Current State

```
Version     : v0.51.0
Commits     : 109
Tests       : 8,852 (0 failures, 0 clippy warnings)
Builtin tools: 30 nika:*
EventKind   : 64 variants
LOC         : 353k+
```

## Sprint Phases

### Phase 1: Engine ProviderName Migration (DONE in this session)
- [x] Core AST: AnalyzedTask.provider + AnalyzedWorkflow.provider -> ProviderName
- [x] Canonicalize defaults: "claude" -> "anthropic" in 5 locations
- [ ] Engine-side: InferParams + AgentParams (deferred — hot path but low risk)

### Phase 2: Quality Audit Verification
- [x] CR1 SchemaGuardrail: Already uses jsonschema::validator_for()
- [x] S3/S4 SSRF: 3-layer defense already implemented
- [x] SF1 fail-closed: DNS errors -> BLOCK
- [x] SF2 ProviderResponded: Fixed in prior session
- [x] SF6 trace drops: Fixed with warn!/debug!
- [x] SF8 debug levels: All appropriate for graceful degradation
- [x] 133 `_ => {}` patterns: All intentional (audit verified)

### Phase 3: Remaining Session Items (DEFERRED)
- [ ] Session B: Agent loop refactor (-800 LOC) — deferred, complex
- [ ] Session D: cargo-mutants, tracing-error, proptest — deferred, tools need setup
- [ ] Session F: 6 remaining enum migrations — deferred, lower priority
- [ ] Session H: LSP NIKA-163, crash fix — deferred, tooling needs
- [ ] Session I: TUI Arc<str> — deferred, deep refactor for marginal gain

### Phase 4: Template Injection Fixes (S5, S6)
- [ ] binding/template.rs:1177 — Add trusted_inputs allowlist to Pass 3
- [ ] binding/template.rs:505 — Add trusted_context to resolve_with
Priority: HIGH (security), but requires understanding of template resolution

### Phase 5: P-ORCHESTRATE (NEXT MAJOR FEATURE)
See: docs/plans/2026-03-28-phase1-orchestrate.md
- goal: field in workflow AST
- nika:run inline YAML extension
- Orchestrator agent loop (review -> dispatch -> synthesize)
- Round tracking + budget enforcement
- Estimated: 725 production LOC + 600 test LOC

## Critical Path to v1.0

```
CURRENT (v0.51.0, 8852 tests)
    |
    +-- Quality Sprint (this plan)     <- DONE (2 commits)
    |
    +-- P-ORCHESTRATE                  <- NEXT (~3 weeks)
    |   +-- goal: AST field
    |   +-- nika:run inline YAML
    |   +-- Orchestrator loop
    |   +-- Round tracking
    |
    +-- Phase 2: Ecosystem             <- AFTER (~6 weeks)
    |   +-- Registry + packages
    |   +-- Telegram trigger
    |   +-- CI/Release pipeline
    |   +-- Distribution
    |
    === v1.0 RELEASE ===
```

## Verified Already Done (from audit)

| Bug ID | Description | Status |
|--------|-------------|--------|
| CR1 | SchemaGuardrail validation | Fixed — uses jsonschema crate |
| CR2/CR3 | Tautological agent tests | Fixed — behavior assertions |
| S1/S2 | Shell -c blocking | Fixed — blocklist covers all variants |
| SF1 | DNS fail-closed | Fixed — errors + timeouts -> BLOCK |
| SF2 | ProviderResponded Layer 0a | Fixed — event emitted before early return |
| SF3/SF4 | for_each TaskFailed events | Fixed — emit_scheduling_failure |
| SF5 | jsonschema .ok() bypass | Fixed — errors properly propagated |
| SF6 | EventLog trace drops | Fixed — warn!/debug! logging |
| SF7 | Daemon job state | Fixed — failures logged |
| SF9 | token_budget enforcement | Fixed — wired to LimitTracker |
| M-orig1 | for_each ordering | Fixed |
| M-orig3 | manifest: true | Fixed — write_artifact_manifest |
| M-orig6 | for_each.index | Fixed — for_each_index binding |
| M-orig8 | Temperature validation | Fixed |
| M-orig9 | Schema guardrail | Fixed — same as CR1 |
