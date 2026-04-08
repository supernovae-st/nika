# Constellation Execution Handoff — SESSION 4

> **Copy-paste this ENTIRE file as context for a fresh Claude Code session.**
> **It contains everything needed to continue the Constellation architecture refactor.**

---

## SITUATION

- **Version:** v0.79.0 (unchanged — no version bump yet)
- **Branch:** main
- **Last commit:** `88fb34a54` fix(security): address P0+P1 findings from code review
- **Tests:** ~10,768 passed, 0 failed (`cargo test --workspace --lib`)
- **Clippy:** Zero warnings (`cargo clippy --workspace -- -D warnings`)
- **Crates:** 24 workspace members (17 original + 7 new)
- **Launch:** May 5, 2026 (J-27)
- **Codename:** Constellation v2.1
- **Working directory:** `tools/nika` (within `/Users/thibaut/dev/supernovae/nika/`)

---

## WHAT WAS DONE (Sessions 1-3, 2026-04-08)

### SESSION 1 — Unblock + Quick Wins (9 commits)

| # | Commit | Type | Description |
|---|--------|------|-------------|
| 1 | `fe0232d86` | fix(build) | ARM64 linker: disable opt-level in test profile |
| 2 | `6134b073e` | fix(runtime) | Dead MPSC receivers: spawned drain tasks in infer.rs |
| 3 | `7af91c350` | fix(security) | Redact McpInvoke params before trace serialization |
| 4 | `e330ba649` | fix(serve) | Release workers Mutex before async storage update |
| 5 | `0de5a3c2f` | refactor(runtime) | `#[must_use]` on 10 Runner builder methods |
| 6 | `574d070cb` | perf(binding) | FxHashSet in template.rs (6 sites) |
| 7 | `b4fa788fa` | refactor(runtime) | OnceLock for workspace_root (write-once invariant) |
| 8 | `671f3fbef` | perf(runtime) | Hoist Arc::from(task_id) out of streaming loops |
| 9 | `f2f10e581` | docs(engine) | ARCHITECTURE.md for nika-engine (matklad rule) |

### SESSION 2 — Kernel + Foundation (7 commits)

| # | Commit | Type | Description |
|---|--------|------|-------------|
| 10 | `1facbe6d3` | docs | Fix nika-daemon missing in ARCHITECTURE.md |
| 11 | `8e1d4f2aa` | feat(kernel) | **nika-kernel** — 10 trait defs, 717 LOC, L0.5 |
| 12 | `36ece3b94` | refactor(core) | rstest pilot: 26→5 parametrized tables (-77 lines) |
| 13 | `4c2af7fe3` | feat(event) | EventEmitter blanket impl for `Arc<T>` |
| 14 | `a98f9409a` | feat(kernel) | **nika-kernel-mock** — 5 mocks, 23 tests |
| 15 | `1d55db887` | refactor(core) | **Split transform.rs** (5570→5 files) |
| 16 | `6390be306` | refactor(engine) | **Split template.rs** (4938→2 files) |

### SESSION 3 — Effect Crates + God File Split + Security Review (4 commits)

| # | Commit | Type | Description |
|---|--------|------|-------------|
| 17 | `98f011a74` | feat(arch) | **5 L1 effect crates** — SystemClock, TokioFs, DiskBlobStore, ReqwestClient, TokioShell |
| 18 | `e4a421a03` | refactor(core) | **Split analyze.rs** (5531→6 files, largest 1109 LOC) |
| 19 | `a020d6b60` | docs | SESSION 4 handoff |
| 20 | `88fb34a54` | fix(security) | **P0+P1 security fixes** from rust-pro code review |

**Code review findings (2 rust-pro agents):**
- 4 P0 SSRF bypasses FIXED (IPv4-mapped IPv6, CGN 100.64.0.0/10, metadata.google.internal)
- 1 P0 blocklist FIXED (ported full 100+ patterns from engine security.rs with NFKC normalization)
- 3 P1 fixes (stat reads 512 bytes not full file, shell-mode blocklist wired, args scanned in non-shell mode)
- 2 P2 kernel DTO improvements (PartialEq+Eq on FileMetadata/BlobMetadata)

---

## COMPLETED PHASES

| Phase | What | Commit(s) |
|-------|------|-----------|
| S1 bugs | ARM64 linker, dead MPSC, redaction, Mutex | 1-4 |
| Quick wins | #[must_use], FxHashSet, OnceLock, Arc hoist | 5-8 |
| Pre-0 | ARCHITECTURE.md | 9-10 |
| 1 | nika-kernel (10 traits, L0.5) | 11 |
| 2 | nika-kernel-mock (5 mocks) | 14 |
| 4 (partial) | rstest pilot | 12 |
| 5.1 | EventEmitter blanket impl | 13 |
| 8a | transform.rs split | 15 |
| 8b | template.rs split | 16 |
| **9+10** | **5 L1 effect crates (75 tests)** | 17, 20 |
| **16 partial** | **analyze.rs split (5531→6 files)** | 18 |

---

## 5 L1 EFFECT CRATES — DETAIL

Each crate implements one nika-kernel trait with production-quality code:

### nika-clock (L1)
- **Trait:** `Clock` (now, sleep, elapsed)
- **Impl:** `SystemClock` — zero-size type wrapping `Instant::now()` + `tokio::time::sleep()`
- **Tests:** 5 (monotonicity, sleep advances, elapsed, Send+Sync, zero-sized)
- **LOC:** ~80

### nika-fs (L1)
- **Trait:** `Filesystem` (read, write, metadata, glob, canonicalize, etc.)
- **Impl:** `TokioFs` — zero-size type wrapping `tokio::fs` + `globset` for pattern matching
- **Tests:** 13 (read/write roundtrip, binary, nested dirs, metadata, remove, glob recursive, canonicalize)
- **LOC:** ~220
- **Known limitation:** glob skips hidden dirs (`.git/`, `.nika/`) unconditionally; no symlink loop protection (P1, documented)

### nika-blob (L1)
- **Trait:** `BlobStore` (put, get, exists, stat, delete)
- **Impl:** `DiskBlobStore` — blake3 content-addressable storage with sharded directory layout (`{root}/{hash[0..2]}/{hash[2..]}`)
- **Tests:** 13 (roundtrip, dedup, exists, stat, delete, not-found, empty rejection, deterministic hash, shard path)
- **LOC:** ~280
- **Security:** Rejects empty blobs, 500MB max size, atomic write via temp+rename

### nika-http (L1)
- **Trait:** `HttpClient` (send)
- **Impl:** `ReqwestClient` — wraps `reqwest::Client` with configurable SSRF protection
- **SSRF defense:** IPv4 private ranges, IPv6 loopback/link-local/unique-local, IPv4-mapped IPv6, CGN 100.64.0.0/10, cloud metadata hostnames
- **Tests:** 16 (request builders, SSRF blocks for localhost/private/CGN/IPv6-mapped/cloud-metadata, public URLs allowed)
- **LOC:** ~180 + ssrf.rs ~180
- **Known limitation:** follow_redirects field ignored (reqwest is client-level); no DNS rebinding protection (P1, documented)

### nika-exec-runner (L1)
- **Trait:** `ShellExecutor` (run)
- **Impl:** `TokioShell` — wraps `tokio::process::Command` with full security blocklist
- **Blocklist:** 100+ patterns ported from engine security.rs, NFKC normalization, zero-width stripping, shell-quote bypass detection, basename resolution, shell-mode patterns
- **Tests:** 28 (echo, shell pipes, exit codes, timeout+kill, not-found, env vars, stdin, rm-rf blocking, sudo, reverse shells, pipe-to-shell, interpreter execution, NFKC bypass, zero-width bypass, quote bypass, full command scan)
- **LOC:** ~280 + blocklist.rs ~230

---

## DEFERRED PHASES (with reasons)

### Phase 5.2: EventSink hot site wiring → Phase 14
30+ structs hold EventLog by value. Cascades through TaskExecutor → Runner → 25+ child structs.
**When:** Each verb crate takes `Arc<dyn EventEmitter>` during extraction.

### Phase 6: error_domains big-bang → Dedicated session
180+ call sites, miette `#[diagnostic]` delegation, ExecutionError(String) 70 sites.
**Start with:** DagError (0 constructor sites), then ProviderError (15), BindingError (8), ExecutionError (70).

### Phase 7: LSP absorption → Dedicated session
**Analysis completed S3:** engine/lsp (11,982 LOC) and nika-lsp-core (11,885 LOC) are NOT duplicates:
- Engine = AST-aware handlers (model_intel, fuzzy matching, semantic diagnostics)
- Core = pure functions (CursorContext-based, error recovery, 4 extra handlers: references/rename/document_links/folding_ranges)
- Already partially integrated (engine delegates to core for some handlers)
- **Blocker:** Handler signature mismatch (position-based vs CursorContext-based)
- **Strategy:** Enrich nika-lsp-core with optional AST parameter, move engine-unique capabilities in, delete engine copy

### Phase 8c: runner/mod.rs → Phase 14
2344 LOC. Tight coupling; meaningful split requires VerbExecutor restructure.

---

## REMAINING GOD FILES

| File | LOC | Action | When |
|------|-----|--------|------|
| `nika/src/main.rs` | 5,530 | Migrate to nika-cli/verbs/ | Phase 15 |
| `nika-engine/src/error.rs` | 2,874 | error_domains promotion | Phase 6 |
| `nika-engine/src/runtime/runner/mod.rs` | 2,344 | Restructure with VerbExecutor | Phase 14 |
| `nika-engine/src/binding/template/mod.rs` | 2,053 | Tightly coupled | Phase 14+ |

---

## CODE REVIEW — REMAINING P1/P2 FINDINGS (not yet fixed)

### P1: nika-fs — Symlink loop protection
`collect_glob_matches` recurses without tracking visited inodes. Circular symlinks cause infinite I/O loop.
**Fix:** Use `ignore` crate (already declared as dep) or track visited paths.

### P1: nika-http — follow_redirects silently ignored
Field exists on `HttpRequest` but reqwest sets policy at Client level.
**Fix:** Create separate `Client` with `redirect(Policy::none())` when false.

### P1: nika-http — No DNS rebinding protection
New SSRF checks are string-level only. Attacker domains resolving to 127.0.0.1 bypass.
**Fix:** Resolve DNS and re-check IPs (requires async resolution step).

### P1: nika-http — No SSRF check on redirect targets
Server at `evil.com` can 302-redirect to `169.254.169.254`.
**Fix:** Custom redirect policy that re-checks each hop.

### P1: nika-exec-runner — Timeout path deadlock risk
stdout/stderr read AFTER wait() instead of concurrently — process can block on pipe write.
**Fix:** Use `tokio::try_join!` to read stdout, stderr, and wait concurrently.

### P2: nika-http — Response headers lose duplicates (HashMap)
HTTP headers can have multiple values; HashMap keeps last only.

### P2: nika-exec-runner — Stdin write error silently ignored
`let _ = stdin.write_all(...)` drops the error.

### P2: nika-blob — MIME type inconsistency
`put()` returns caller-provided mime_type; `stat()` re-detects from content.

---

## NEXT PHASES — SESSION 4

### Priority 1: Phase 11 — Provider trait cutover (THE KEYSTONE)

**Current state:**
- `RigProvider` enum: 9 variants in `nika-engine/src/provider/rig/mod.rs:164-207`
- `dispatch_rig!` macro: 5 call sites in `inference.rs` and `provider_streaming.rs`
- 26 files reference RigProvider across the codebase
- Kernel `Provider` trait with DTOs already in `nika-kernel/src/provider.rs`

**v2.1 big-bang strategy:**
1. `impl Provider for RigProvider` that bridges kernel DTOs ↔ rig-core types
2. Flip `TaskExecutor` to use `Arc<dyn Provider>` instead of direct `RigProvider`
3. Keep `dispatch_rig!` INSIDE the trait impl (connect, don't delete)
4. Mock and Native get separate `impl Provider` blocks

**Key files to read:**
- `tools/nika-engine/src/provider/rig/mod.rs` — RigProvider enum + dispatch_rig! macro
- `tools/nika-engine/src/provider/rig/construction.rs` — factory methods
- `tools/nika-engine/src/provider/rig/inference.rs` — non-streaming inference (5 dispatch sites)
- `tools/nika-engine/src/provider/rig/provider_streaming.rs` — streaming inference
- `tools/nika-engine/src/runtime/executor/mod.rs` — TaskExecutor (rig_provider_cache field)
- `tools/nika-engine/src/runtime/executor/infer.rs` — how providers are resolved and called
- `tools/nika-kernel/src/provider.rs` — the target Provider trait + DTOs

**Challenge:** Bridging rig-core types ↔ kernel DTOs (Message, ContentBlock, InferResponse).
The rig-core types are deeply integrated through the inference pipeline.

### Priority 2: Phase 12 — nika-builtin extraction
Create `nika-builtin` crate with 63 builtin tools + sealed `BuiltinTool` trait.

### Priority 3: Phase 15 — main.rs migration
Move 5530 LOC from `nika/src/main.rs` to `nika-cli/src/verbs/`.

---

## KEY NUMBERS

| Metric | v0.79.0 | After S1+2 | After S3 |
|--------|---------|------------|----------|
| Workspace crates | 17 | 19 | **24** |
| Tests | 10,666 | 10,693 | **~10,768** |
| God files (>1500 LOC source) | 5 | 3 | **2** |
| Traits defined | 0 | 10 | 10 |
| Production trait impls | 0 | 0 | **5** |
| Mock implementations | 0 | 5 | 5 |
| Security test cases (effect crates) | 0 | 0 | **~30** |
| Clippy warnings | 0 | 0 | 0 |

---

## CURRENT LAYERING (post SESSION 3)

```
L0    nika-core (23k)         Pure types, AST, catalogs. Zero I/O.
L0.5  nika-kernel (717)       10 trait defs. Zero impls.
      nika-kernel-mock (744)  5 hand-written mocks. Dev-dep.
L1    nika-clock              SystemClock (tokio::time, ZST)         NEW S3
      nika-fs                 TokioFs (tokio::fs + globset, ZST)     NEW S3
      nika-blob               DiskBlobStore (blake3 CAS)             NEW S3
      nika-http               ReqwestClient (SSRF: IPv4/v6/CGN/meta) NEW S3
      nika-exec-runner        TokioShell (100+ pattern blocklist)    NEW S3
      nika-event (4.5k)       EventLog + EventEmitter blanket impl
      nika-lsp-core (12k)     LSP intelligence (pure functions)
L2    nika-engine (160k)      MONOLITH — extraction source for ~15 new crates
      nika-media (14k)        CAS store, image ops
      nika-mcp (9k)           MCP client (rmcp)
      nika-vault (1.2k)       Encrypted secrets
      nika-storage (1k)       Storage abstraction
      nika-display (13k)      CLI renderers
L3    nika-daemon (7k)
L4    nika-cli (8k), nika-tui (88k), nika-serve (4k), nika-lsp (2.5k),
      nika-sdk (3k), nika-init (21k)
L5    nika (5.5k)             Binary entry point
```

### Target (Constellation v2.1 complete)

```
L0    nika-core
L0.5  nika-kernel, nika-kernel-mock
L1    nika-clock, nika-fs, nika-http, nika-exec-runner, nika-blob,
      nika-event, nika-macros, nika-shield, nika-lsp-core
L2    nika-provider, nika-builtin, nika-mcp, nika-media, nika-storage,
      nika-vault, nika-verb-{infer,exec,fetch,invoke,agent}
L3    nika-runtime, nika-daemon, nika-cache
L4    nika-cli, nika-tui-{widgets,core,views,app}, nika-lsp, nika-serve,
      nika-sdk, nika-init, nika-display
L5    nika (<500 LOC)
```

---

## MANDATORY READS (in order)

1. `nika/CLAUDE.md` — project overview, 5 verbs, Shield, testing
2. `tools/nika/CLAUDE.md` — crate architecture, error codes, testing rules
3. `tools/nika-engine/ARCHITECTURE.md` — engine module map, invariants
4. `docs/plans/2026-04-08-constellation-v2-mega-plan.md` — **THE PLAN** (sections 5, 8, 17, 18)
5. Memory: `project_constellation_session3.md` — S3 full log
6. Memory: `project_constellation_session1_session2.md` — S1+S2 full log
7. Memory: `project_constellation_findings_log.md` — running findings/non-issues/debt log

---

## RULES — NON-NEGOTIABLE

### Build
- `cargo test --workspace --lib` after EVERY commit (from `tools/nika/`)
- `cargo clippy --workspace -- -D warnings` — zero warnings
- Never `cargo test` without `--lib` (macOS Keychain popups)

### Git
- 1 logical change = 1 commit
- Format: `refactor(arch): <what>` or `fix(scope): <what>` or `feat(scope): <what>`
- Co-author ALWAYS: `Nika 🦋 <nika@supernovae.studio>` (NEVER Claude/Anthropic)
- Do NOT push unless explicitly asked
- Stage specific files, never `git add -A`

### Architecture
- **CONNECT, DO NOT DELETE.** Dead scaffolding gets wired, not removed.
- Big-bang cutovers OK (v0 = zero users)
- No crate > 15k LOC source (excluding tests)
- No file > 1500 LOC (target; template/mod.rs at 2053 is a known exception)
- Every side effect behind a trait
- Tests move WITH their code

### What You May NOT Touch
- The 5 verbs (infer, exec, fetch, invoke, agent) — NEVER change
- Schema version `nika/workflow@0.12` — NEVER change
- AGPL license — NEVER change
- Shield files (merged, stable) — only if directly needed

---

## VERIFICATION

```bash
cd /Users/thibaut/dev/supernovae/nika/tools/nika
cargo test --workspace --lib 2>&1 | grep "test result"
cargo clippy --workspace -- -D warnings 2>&1 | tail -5
```

Expected: `~10,768 passed; 0 failed` + clean clippy.

---

**Start with Phase 11 (Provider trait cutover) — it's the keystone that unlocks nika-provider + nika-runtime.**
**The kernel traits are ready. The effect crates are ready. Now wire the providers.**
