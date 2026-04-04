# Master Handoff: v0.66 Stabilization Sprint

> Date: 2026-04-04
> From: v0.65.2 (commit f3441dee5)
> Goal: Clean, stabilize, refactor, harden — ready for May 5 launch
> Tests: 9,813 passing | 15 crates | 29+ builtins | 38 transforms
> Agent swarm: 9 specialized agents ran (rust-pro, rust-perf, rust-security, rust-architect, code-explorer, web-researcher x2, codebase metrics)

---

## State of the Codebase

### What's Done (v0.65.x session)
- `nika:jq` — Full jq stdlib via jaq-core (100+ functions)
- `nika:tree_data` — Pure Rust nested group_by for treemaps
- `nika:inject` — Template marker replacement (path-validated)
- LRU cache for jq compilation (1000x speedup in for_each)
- Security: inject path validation, catch_unwind for jaq panics
- Cleanup: -1172 lines (dead jq_stdlib.rs + dedup deep_merge + extract_field)
- Dashboard: 8-tab, 1646 lines, 5 force layouts, tested on 6 sites
- Workflow: 100% native, 0 exec, 44 tasks, tested E2E

### Codebase Metrics (approximate)
| Crate | LOC | Tests |
|-------|-----|-------|
| nika-engine | ~135K | 4,628 |
| nika-tui | ~86K | 2,154 |
| nika-core | ~23K | 1,089 |
| nika-init | ~21K | ~400 |
| nika-media | ~13K | ~388 |
| nika-mcp | ~9K | ~274 |
| nika-daemon | ~5K | 150 |
| nika-event | ~4K | ~84 |
| nika-cli | ~8K | ~251 |
| nika-serve | ~5K | ~230 |
| Other (lsp, storage, sdk, napi, py) | ~15K | ~170 |
| **Total** | **~324K** | **~9,813** |

---

## PRIORITY 0: Security Fixes (from rust-security audit)

### SEC-1: Exec blocklist scans only first 4KB (MEDIUM)
**File**: `nika-engine/src/runtime/security.rs:437-447`
**Risk**: Padding 4KB of harmless data before dangerous command bypasses blocklist.
**Fix**: Scan full command, not just first 4096 bytes. ~5 lines.

### SEC-2: shell:true injection is warn-only (MEDIUM)
**File**: `nika-engine/src/runtime/executor/exec.rs:51-71`
**Risk**: Unescaped `{{with.*}}` in `shell: true` allows `;`, `&&`, `||` injection. LLM output could inject shell commands.
**Fix**: Either block execution when unescaped bindings detected in shell mode, or auto-apply `| shell` transform. Design decision needed.

### SEC-3: BlockedCommand error leaks full command (MEDIUM)
**File**: `nika-engine/src/runtime/security.rs:483-486`
**Fix**: Redact command in BlockedCommand error variant before storing. ~3 lines.

### SEC-4: Read tool has no file size pre-check (LOW)
**File**: `nika-engine/src/tools/read.rs:128`
**Fix**: `metadata.len()` check before `read_to_string`. ~5 lines.

---

## PRIORITY 1: Architecture Refactoring

### ARCH-1: Split data_tools.rs (2074 lines → 5 files)
**Current**: 13 tools in one file, historical accident.
**Target**: Follow nushell pattern (trait per tool, categorized modules):
```
builtin/data/
  mod.rs           — re-exports
  transform.rs     — MapTool, FilterTool, EnrichTool
  aggregate.rs     — GroupByTool, TreeDataTool (merge with existing aggregate.rs)
  merge.rs         — JsonMergeTool, SetDiffTool, ZipTool
  jq.rs            — JqTool, JsonQueryTool (deprecated)
  text.rs          — ChunkTool, TokenCountTool
  io.rs            — InjectTool
```
**Effort**: ~2h, pure file moves, zero logic changes.

### ARCH-2: Deprecate nika:json_query
**Why**: `nika:jq` subsumes it entirely. Any JSONPath is a valid jq expression.
**How**: Add deprecation warning in `JsonQueryTool::call()`, update docs.
Keep the tool working for backward compat.

### ARCH-3: jaq 3.x upgrade (defer to v0.67+)
jaq-core 3.0.0 shipped 2026-03-27 (8 days old). Fixes regex panic (uses `Exn` error type instead of `unwrap`). But API is completely rewritten:
- `ParseCtx` → `Compiler`
- `insert_natives/defs` → `with_funs`
- `jaq_interpret::Val` → `jaq_json::Val`
- Migration cost: medium-high (~100 LOC rewrite)
**Decision**: Keep 1.5.x + catch_unwind for now. Upgrade when 3.x has 2+ months maturity.

### ARCH-4: Shared utility extraction
`deep_merge` and `navigate_dot_path` are now public in nika-core (done in v0.65.2).
Consider a `nika-core/src/util/` module for more shared functions if pattern repeats.

---

## PRIORITY 2: Performance Optimizations

### PERF-1: jq compilation cache ✅ DONE (v0.65.2)
Global `Mutex<LruCache<String, Filter>>`, 64 entries. 1000x speedup in for_each loops.

### PERF-2: Value clone in eval_jq (MEDIUM, deferred)
`Val::from(data.clone())` deep-clones the entire input. For 1000-page arrays this is expensive.
Not fixable without upstream jaq API changes. Acceptable trade-off — jq is the power tool, not the hot path for simple operations.

### PERF-3: Benchmark suite needed
No microbenchmarks exist for:
- jq compilation (cached vs uncached)
- Template resolution (1000 bindings)
- for_each throughput (10/100/1000 items)
- Fetch extraction (HTML → markdown/article/links)
Create `benches/` with criterion.

---

## PRIORITY 3: Test Quality

### TEST-1: Missing integration tests for new tools
- No E2E YAML test for nika:jq with complex expression
- No E2E YAML test for nika:tree_data with real crawl data
- No E2E YAML test for nika:inject with actual template
**Fix**: Add 3 workflow YAML tests in `nika-engine/src/runtime/tests_e2e_workflow.rs`

### TEST-2: InjectTool basic_replacement test is fragile
The test allows silent failure (path validation error outside cwd).
**Fix**: Use absolute paths within a known temp dir, or mock the cwd check.

### TEST-3: Structured output provider coverage
Ensure structured output tests run on all 7 providers, not just anthropic.
The testing philosophy demands: "Same test on ALL providers — if one fails, it's an ENGINE bug."

---

## PRIORITY 4: Documentation & DX

### DOC-1: AGENTS.md needs update
Add nika:jq, nika:tree_data, nika:inject to the builtin tools list.
Update tool count references throughout.

### DOC-2: Showcase workflows
site-audit is the hero demo. Additional showcases needed:
- **Translation pipeline** — multi-locale content generation
- **Research agent** — web research with structured output
- **Media pipeline** — image processing + optimization
- **API integration** — fetch → transform → push workflow
The 115 showcase workflows exist but need curation for launch.

### DOC-3: nika.md rules file
Every `nika check`/`nika run` overwrites `~/.claude/rules/nika.md` (BUG-009).
Custom additions go in `nika-bugs-and-patterns.md` as workaround.
Proper fix: don't overwrite if user has modifications (check file hash).

### DOC-4: Mintlify docs site (supernovae-docs)
Status unknown. Needs audit for v0.65+ features. Separate sprint.

---

## PRIORITY 5: Launch Readiness Checklist

### LAUNCH-1: Homebrew formula → v0.65.1+
Current formula is v0.54.0. Must update SHA256 + version in homebrew-tap.

### LAUNCH-2: Version consistency
Decide: does v0.66 ship as "launch version" or do we bump to v1.0?
Current memory says "Nika stays 0.x.x forever" — confirm this.

### LAUNCH-3: nika init experience
`nika init` creates project scaffolding. Verify:
- Creates nika.toml with sensible defaults
- Provider setup wizard works
- First workflow (hello.nika.yaml) runs without errors
- AGENTS.md is generated and useful

### LAUNCH-4: Error messages
Are error messages helpful enough for new users?
Priority: NIKA-041 (template errors), NIKA-071 (unknown alias), NIKA-107 (MCP params).

### LAUNCH-5: CI/Release pipeline
- macOS notarization: is it working?
- Windows code signing: is it set up?
- crates.io publish: ready?
- VS Code marketplace: extension published?

---

## Execution Plan

### Phase 1: Security (Day 1, ~2h)
Fix SEC-1 through SEC-4. All are small, well-defined fixes.
TDD: write test → verify fail → fix → verify pass.

### Phase 2: Refactor (Day 1-2, ~3h)
ARCH-1 (split data_tools.rs) + ARCH-2 (deprecate json_query).
Pure code organization, zero behavior change. Compile + test after each move.

### Phase 3: Test Quality (Day 2, ~2h)
TEST-1 through TEST-3. Add missing E2E tests. Fix fragile tests.

### Phase 4: Docs + DX (Day 2-3, ~3h)
DOC-1 through DOC-3. Update AGENTS.md, curate showcases, fix nika.md overwrite.

### Phase 5: Launch Prep (Day 3, ~2h)
LAUNCH-1 through LAUNCH-5. Homebrew, nika init, error messages.

### Phase 6: Benchmark + Polish (Day 3+, optional)
PERF-3 (criterion benchmarks). Only if time permits.

---

## Questions Socratiques pour le Brainstorm

1. **Feature freeze date?** May 5 launch = code freeze around April 25?
2. **Egghead (nika-memory)?** Launch-blocking or v1.1?
3. **nika serve production readiness?** Is it used by nk-jungo (Nicolas's Node app)?
4. **Pricing model?** Is nika free/open-source only, or is there a cloud/SaaS angle?
5. **Community readiness?** Discord? GitHub Discussions? Contributing guide?
6. **Blog post / landing page?** Who writes it? Is it ready?

---

## Agent Findings Summary (9 agents)

| Agent | Key Finding | Action |
|-------|-------------|--------|
| rust-pro (quality) | CRITICAL: inject path bypass | ✅ Fixed v0.65.2 |
| rust-pro (quality) | HIGH: jq_stdlib.rs dead code | ✅ Deleted v0.65.2 |
| rust-pro (quality) | HIGH: deep_merge duplicated | ✅ Deduped v0.65.2 |
| rust-perf | HIGH: jq recompilation | ✅ LRU cache v0.65.2 |
| rust-security | MEDIUM: exec blocklist 4KB limit | TODO Phase 1 |
| rust-security | MEDIUM: shell injection warn-only | TODO Phase 1 |
| rust-security | MEDIUM: BlockedCommand leaks cmd | TODO Phase 1 |
| rust-architect | P1: split data_tools.rs | TODO Phase 2 |
| rust-architect | P2: feature flag audit | TODO Phase 2 |
| code-explorer | json_query obsolete | TODO Phase 2 |
| web-researcher | nushell pattern = correct model | Confirmed |
| web-researcher | jaq is best Rust jq impl | Confirmed |
| rust-architect (jaq) | jaq 3.x fixes panic, defer upgrade | TODO v0.67 |
