# Master Handoff — 3 Chantiers IDE / LSP / Distribution

> **Date**: 2026-04-06 | **Version**: v0.73.0 (v0.74.0 ready to tag)
> **Tests**: 10,315 GREEN | **Crates**: 22 | **Launch**: May 5, 2026

## State of the World

### Just Shipped (v0.73→v0.74)
- Model/provider refactor COMPLETE (base_url removed, slash syntax, endpoints in nika.toml)
- Scheduling SHIPPED (cron loop, overlap policies, serve reconciliation, 24h timeline)
- native-inference bundled in default features
- 8 production bug fixes from nk-jungo VPS

### Remaining Before Launch
- S9 finish: trigger command + interactive wizard (~1h)
- S10: Multi-tenant auth (~4h, blueprint ready)
- S11: PostgreSQL store (~3h, blocked by S10)
- S12: Final polish (BUG-10, parse_yaml, docs)
- **3 Big Chantiers** (this document)

### Working Tree
- Branch: `main`, up to date with origin
- Clean (two phantom-dirty files with zero diff)
- Untracked: `tools/nika-vscode/` (scaffold) + 3 plan docs

---

## 3 Chantiers — Execution Order

```
                                     +-> Handoff B: IDE Phase 0 (30min)
                                     |     fix 8 bugs + activate features
                                     |
Handoff A: LSP Decoupling (1-2d) ---+-> Handoff B: IDE Phases 1-7 (10-12d)
  compile 90s -> 15s                 |     DAG webview, sidebar, MCP
  drop nika-engine dep               |
                                     +-> Handoff C: Distribution (2-3d)
                                           fix npm, VSIX bundling, version sync
```

**Order**: A first (unblocks everything), then B + C in parallel.

---

## Handoff A: LSP & Crate Decoupling

**File**: `docs/plans/2026-04-06-handoff-a-lsp-decoupling.md`
**Effort**: 1-2 days
**Impact**: Compile time 90s → 15s, unblocks VSIX bundling

### Why First
- Smaller nika-lsp binary = smaller VSIX (Plan C)
- Faster iteration for IDE features (Plan B)
- Clean separation for DaemonProvider trait (Plan B Phase 2)

### Key Update Since Plans
- `base_url` field removed from AST → fewer types to worry about
- analyze.rs simplified → decoupling is EASIER than expected
- All model/provider logic properly split between AST (validation) and runtime (resolution)

---

## Handoff B: IDE / VSCode Mega Execution

**File**: `docs/plans/2026-04-06-handoff-b-ide-execution.md`
**Effort**: 12-15 days total
**Impact**: Full IDE experience (code lens, DAG, sidebar, Cursor support)

### Phase 0 (30 min — Can start immediately)
Fix 8 bugs + activate 6 hidden features. No dependency on Plan A.

### Phases 1-7 (after Plan A)
Binary bundling, DaemonProvider, DAG webview, sidebar, MCP expansion.

### Key Update Since Plans
- Extension still at v0.51.0 (no changes)
- Model default now `claude-sonnet-4-20250514` (correct)
- native-inference bundled → binary is bigger (~50MB) but users get everything

---

## Handoff C: Distribution Simplification

**File**: `docs/plans/2026-04-06-handoff-c-distribution.md`
**Effort**: 2-3 days
**Impact**: All 9 channels in sync, 1 download = everything

### Critical Fixes
1. npm v0.46.1 → v0.73.0+ (27 versions behind!)
2. VS Code v0.51.0 → v0.73.0+ (22 behind!)
3. Platform-specific VSIX with bundled binary
4. Version sync automation

### Key Update Since Plans
- native-inference now bundled by default → "1 download = everything" is TRUE
- No more feature flag decisions for users
- Homebrew at v0.72.0 (will auto-update on next tag)

---

## How to Use These Handoffs

Each handoff document is **self-contained**. Copy-paste into a new Claude Code session:

```
# New session — start fresh
Read docs/plans/2026-04-06-handoff-a-lsp-decoupling.md
Then execute following the plan.
```

### Skills to Load
- `test-driven-development` — every task
- `verification-before-completion` — before "done"
- `spn-rust:rust-core` — Rust code
- `spn-rust:rust-async` — DaemonProvider (Plan B Phase 2)
- `executing-plans` — batch execution with checkpoints

### Guardrails (run in EVERY session)
```bash
cd tools && cargo test --workspace --lib  # 10,315+ tests
cd editors/vscode && npm run compile      # Extension builds
```

### Commit Convention
```
type(scope): description

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

**NEVER** use Claude/Anthropic co-author. **ALWAYS** Nika 🦋.

---

## Priority Matrix

| Handoff | Effort | Impact | Blocks | Priority |
|---------|--------|--------|--------|----------|
| A (LSP) | 1-2d | Compile 6x faster | B Phase 6, C Phase 3 | **P0** |
| B Phase 0 (bugs) | 30min | 6 features visible | Nothing | **P0** |
| C Phase 1-2 (npm/version) | 1d | Fix broken channels | Nothing | **P1** |
| B Phases 1-7 (IDE) | 10-12d | Full IDE | A | **P1** |
| C Phase 3 (VSIX) | 1d | Binary in extension | A | **P1** |
| C Phase 4-5 (docs/scripts) | 1d | Automation | Nothing | **P2** |

---

## Files Reference

| Document | Purpose |
|----------|---------|
| `docs/plans/2026-04-06-plan-1-ide-vscode-mega-execution.md` | Full IDE plan (800 lines) |
| `docs/plans/2026-04-06-plan-2-lsp-crate-decoupling.md` | LSP decoupling plan |
| `docs/plans/2026-04-06-plan-3-distribution-single-download.md` | Distribution plan |
| `docs/plans/2026-04-06-handoff-a-lsp-decoupling.md` | Session handoff: LSP |
| `docs/plans/2026-04-06-handoff-b-ide-execution.md` | Session handoff: IDE |
| `docs/plans/2026-04-06-handoff-c-distribution.md` | Session handoff: Distribution |
| `docs/research/2026-04-06-vscode-*.md` | 4 research reports |
| `docs/research/2026-04-06-provider-model-syntax-research.md` | Provider refactor research |
