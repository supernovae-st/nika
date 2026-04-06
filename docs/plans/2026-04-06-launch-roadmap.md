# Nika Launch Roadmap — v0.74 to May 5

> Date: 2026-04-06 | Baseline: 10,364 tests | 410K LOC | 20 crates | 77MB binary
> Launch: May 5, 2026 (~29 days)

---

## Current State Assessment

```
DONE                                       HEALTH
- S10 multi-tenant auth (8 phases)         - 10,364 tests GREEN
- VSIX CI binary bundling fix              - 0 clippy warnings
- All S2-S5 quick fixes                    - cargo fmt clean
- All M1-M3 MUST-DOs                       - 1 RUSTSEC (rsa via c2pa, no fix)
                                           - TypeScript compiles clean
CONCERNS
- runner.rs: 8,055 lines (monolith)
- 5,784 unwrap() in prod code
- 77MB release binary
- 6 unmaintained transitive deps
- 31 files > 2,000 lines
```

---

## Phase 0 — Immediate Cleanup (Day 1, ~2h)

### P0.1 — Stale Version References
- `tools/nika-cli/src/machine/install.rs:1910` — v0.62.0 URL
- `tools/nika-cli/src/machine/install.rs:2123` — v0.63.0 URL
- `tools/nika-cli/src/bench.rs:94,1014` — v0.50 references
- `tools/nika-engine/src/runtime/security.rs:30` — v0.59 comment
- **Action**: Update all to current version or remove hardcoded versions

### P0.2 — Dead Code Audit
- 20 `#[allow(dead_code)]` annotations — audit each, remove if genuinely dead
- Check `tools/nika-lsp/src/daemon_bridge.rs` — incrementally wired, may have dead stubs
- **Rule**: Zero dead code (per feedback_v0_no_dead_code.md)

### P0.3 — Panic! Cleanup in keys.rs
- 15 `panic!()` calls in `tools/nika-cli/src/keys.rs` (match guards)
- Convert to `unreachable!()` or `debug_assert!()` — same intent, clearer semantics
- **Why**: `panic!` in CLI code looks like a bug; `unreachable!` is self-documenting

---

## Phase 1 — Runner Decomposition (Days 2-4, ~8h)

### The Problem
`runner.rs` at 8,055 lines is the single largest file. It handles:
- Workflow execution loop
- Task dispatching (5 verbs)
- Binding resolution
- for_each parallelism
- Retry/fallback logic
- Checkpoint/resume
- Event emission
- Context management

### Surgical Plan
Split into focused modules under `runtime/`:

```
runtime/
├── runner.rs          (~2,000 lines — orchestration only)
├── dispatch.rs        (~1,200 lines — verb dispatch + exec/fetch/infer/invoke/agent)
├── bindings.rs        (~800 lines — with: resolution, template context)
├── for_each.rs        (existing — already extracted)
├── retry.rs           (~400 lines — retry + on_error + fallback)
├── checkpoint.rs      (~300 lines — save/load/resume)
├── lifecycle.rs       (~500 lines — task lifecycle: start, complete, fail, skip)
└── mod.rs             (re-exports)
```

### Method
1. **Identify seams**: grep for `fn ` in runner.rs, group by concern
2. **Extract bottom-up**: start with lowest-dependency functions (checkpoint, retry)
3. **Move with tests**: each extraction = move code + move tests + verify
4. **1 extraction = 1 commit**, tests green after each

### Verification
- `cargo test -p nika-engine --lib` after each commit
- No public API changes — only internal restructuring
- `runner.rs` target: < 2,500 lines

---

## Phase 2 — Unwrap Audit (Days 4-6, ~6h)

### Scope
5,784 unwraps in production code. Not all are problems:
- `expect("static string")` on infallible ops → KEEP
- `.unwrap()` on `Mutex::lock` → KEEP (poisoned = unrecoverable)
- `.unwrap()` on user input or IO → FIX (return Result)

### Strategy
1. **Triage by crate** — prioritize:
   - `nika-serve` (user-facing HTTP) — CRITICAL
   - `nika-engine/runtime` — HIGH
   - `nika-cli` — MEDIUM
   - `nika-tui` — LOW (panic = restart TUI)
2. **Fix patterns**:
   - `.unwrap()` on `serde_json::from_str` → `.map_err(NikaError::from)?`
   - `.unwrap()` on file IO → `?` with context
   - `.unwrap()` on channel sends → `.ok()` (fire-and-forget)
3. **Target**: < 500 unwraps in serve + engine runtime
4. **Skip**: test code, TUI rendering, static initialization

---

## Phase 3 — Binary Size Optimization (Days 6-7, ~4h)

### Current: 77MB release binary

### Actions
1. **Strip debug info**: `[profile.release] strip = true` (if not already)
2. **LTO**: `lto = "thin"` for balance of compile time vs size
3. **codegen-units**: `codegen-units = 1` for maximum optimization
4. **panic = abort**: `panic = "abort"` (no unwinding in CLI)
5. **Feature audit**: check which `media-*` features are default — move expensive ones to opt-in
6. **Dependency weight**: `cargo bloat --release --crates` to find heavy deps
7. **Target**: < 50MB release binary

### Profile
```toml
[profile.release]
lto = "thin"
codegen-units = 1
strip = true
panic = "abort"
opt-level = "s"  # size-optimize (test if perf acceptable)
```

---

## Phase 4 — Integration Test Suite (Days 7-10, ~8h)

### Goal: Real workflow execution tests, not just unit tests

### 4.1 — Golden Test Workflows
Create 10 canonical workflows that exercise every verb + major feature:

```
tests/golden/
├── 01-hello-infer.nika.yaml          # Basic infer
├── 02-structured-output.nika.yaml    # 5-layer defense
├── 03-fetch-extract.nika.yaml        # 9 extract modes
├── 04-exec-pipeline.nika.yaml        # Shell commands + pipes
├── 05-for-each-parallel.nika.yaml    # Concurrency + fail_fast
├── 06-agent-loop.nika.yaml           # Multi-turn agent
├── 07-on-error-fallback.nika.yaml    # Error routing
├── 08-multi-provider.nika.yaml       # 3 providers fan-out
├── 09-invoke-builtins.nika.yaml      # nika:jq, nika:map, etc.
├── 10-context-skills.nika.yaml       # File context + skills
```

### 4.2 — Serve Stress Test
- Concurrent requests (10 parallel `nika run` via HTTP)
- Token rotation (add/revoke while serving)
- SSE streaming reliability
- Job GC under load

### 4.3 — Mock Provider E2E
- `nika test` on all 10 golden workflows with `--provider mock`
- `nika test --golden` snapshot comparison
- `nika eval` with assertion datasets

---

## Phase 5 — Crate Architecture Polish (Days 10-14, ~8h)

### 5.1 — Large File Splits (beyond runner.rs)

| File | Lines | Action |
|------|-------|--------|
| `template.rs` | 4,935 | Split: parser, resolver, pipe_transforms |
| `transform.rs` | 5,574 | Split: string_transforms, array_transforms, numeric_transforms |
| `analyze.rs` | 5,462 | Split: phase2_validate, phase2_resolve, phase2_analyze |
| `log.rs` | 4,460 | Split: event_types, event_log, event_serialize |
| `storage/lib.rs` | 2,818 | Split: jobs.rs, schedules.rs, tokens.rs, schema.rs |

### 5.2 — Dependency Audit
- `cargo deny check` — resolve RUSTSEC-2023-0071 (rsa)
  - Option A: Pin c2pa version, document risk, add `[advisories] ignore`
  - Option B: Feature-gate `media-provenance` (c2pa is opt-in anyway)
- Check for duplicated transitive deps: `cargo tree -d`
- Remove unused workspace deps

### 5.3 — Compile Time
- Current: ~80s full check, ~60s incremental
- Target: < 45s incremental
- Actions:
  - Audit `#[derive(Serialize, Deserialize)]` — remove from internal types that don't need it
  - Check for unnecessary `syn` full-feature usage in proc macros
  - Profile with `cargo build --timings`

---

## Phase 6 — Security Hardening (Days 14-16, ~4h)

### 6.1 — Serve Security
- Rate limit per IP (in addition to per-token)
- Request body size validation on all endpoints
- CORS: verify restrictive by default (already done)
- TLS documentation (recommend reverse proxy)

### 6.2 — Exec Hardening
- Audit `exec:` blocklist completeness (SEC-1 full scan)
- Verify `| shell` enforcement in all tests
- Test NIKA-053 with adversarial inputs

### 6.3 — Supply Chain
- Pin all CI action versions to SHA (not tags)
- Enable SLSA provenance on all release artifacts
- Verify Homebrew SHA256 checksums match

---

## Phase 7 — Distribution & IDE (Days 16-20, ~8h)

### 7.1 — CI Release Pipeline
- [ ] Verify VSIX extraction works end-to-end (tag a test release)
- [ ] Test npm publish flow (platform binaries + wrapper)
- [ ] Verify Homebrew tap formula update
- [ ] Test Docker multi-arch build
- [ ] Verify crates.io publish order (dependency chain)
- [ ] Test AUR + Scoop publish

### 7.2 — IDE Extension Polish
- [ ] Wire `nikaProviders` tree view (declared but no TreeDataProvider)
- [ ] Fix MCP `nika_dag_visualization` block-style `depends_on:` parsing
- [ ] Test extension on VS Code, Cursor, Windsurf
- [ ] Verify binary auto-download on first install

### 7.3 — Documentation
- [ ] Update CHANGELOG.md for v0.75.0
- [ ] Update README badges (test count, version)
- [ ] Verify `nika init --course` works (44 exercises)
- [ ] Check `nika showcase list` (115 workflows)

---

## Phase 8 — Final Stabilization (Days 20-25, ~6h)

### 8.1 — Full Test Suite Verification
```bash
cargo test --workspace --lib                    # 10,364+ tests
cargo clippy --workspace -- -D warnings         # 0 warnings
cargo fmt --all --check                         # clean
cd editors/vscode && npm run compile            # builds
cargo deny check                                # security
```

### 8.2 — Performance Benchmarks
```bash
nika bench                                      # Provider latency
nika run hello.nika.yaml --provider mock        # Cold start < 100ms
time nika check complex.nika.yaml               # Validation < 500ms
time nika graph complex.nika.yaml               # DAG render < 200ms
```

### 8.3 — Smoke Tests on All Platforms
- macOS ARM64 (primary dev)
- macOS x64 (CI)
- Linux x64 (Docker + bare metal)
- Linux ARM64 (Scaleway VPS)
- Windows x64 (CI only)

---

## Phase 9 — Launch Prep (Days 25-29)

### 9.1 — Version Bump
```bash
# Tag v0.75.0 (or whatever version ships)
git tag v0.75.0 && git push --tags
```

### 9.2 — Release Verification
- [ ] GitHub Release created with AI-generated notes
- [ ] Homebrew formula updated
- [ ] npm packages published (6 packages)
- [ ] crates.io published (13 crates)
- [ ] Docker images pushed (amd64 + arm64)
- [ ] VS Code Marketplace VSIX (5 platforms + universal)
- [ ] Open VSX published
- [ ] AUR + Scoop updated

### 9.3 — Smoke Test Post-Release
```bash
# Fresh install paths
brew install supernovae-st/tap/nika
npm install -g nika
cargo install nika
docker run ghcr.io/supernovae-st/nika version
```

---

## Priority Matrix

```
                    IMPACT
              LOW         HIGH
         ┌──────────┬──────────┐
    LOW  │ P0.1     │ P3       │  EFFORT
         │ versions │ binary   │
         ├──────────┼──────────┤
    HIGH │ P5.1     │ P1       │
         │ splits   │ runner   │
         └──────────┴──────────┘

CRITICAL PATH (blocks launch):
  P4 (integration tests) → P7 (distribution) → P8 (stabilize) → P9 (launch)

PARALLEL WORK (improves quality, doesn't block):
  P0 (cleanup) | P1 (runner split) | P2 (unwrap audit) | P3 (binary size)
  P5 (crate polish) | P6 (security)
```

---

## Post-Launch Backlog (v0.76+)

| Item | Effort | Priority |
|------|--------|----------|
| PostgreSQL backend (feature-gated) | 2 weeks | Medium |
| Observability UI (htmx + uPlot) | 1 week | Medium |
| `on_error` depth > 1 | 3 days | Low |
| NikaError 103-variant flattening | 1 week | Low |
| YAML anchor bomb protection | 2 days | Medium |
| Nika Memory (Egghead) | 6 phases, 8750 LOC | Major feature |
| Schedule cost projection | 2 days | Low |
| `nika help cron` topic | 1 day | Low |
| Schedule-aware lint rules (L100+) | 3 days | Low |

---

## Rules

- `cargo test --workspace --lib` GREEN after EVERY commit
- 1 concern = 1 commit (no batching)
- Co-author: `Nika 🦋 <nika@supernovae.studio>`
- No new features — stabilize what exists
- If a refactor breaks > 3 tests, stop and reassess
- TDD on integration tests (write test → watch fail → implement)
