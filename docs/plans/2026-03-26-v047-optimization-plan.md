# Plan: v0.47.0 — Performance + Architecture Optimization

> Results from 18 review agents across 2 swarms.
> 4 HIGH, 10 MEDIUM, 8 LOW findings. Estimated: ~600 LOC changes.

## HIGH Impact — ✅ ALL DONE

- [x] **H1**: TUI chat message items cloned every frame → extend pattern (`3f461d2`)
- [x] **H2**: TUI Clear+set_style overwrites all cells → removed (`7be97fa`)
- [x] **H3**: jsonschema recompiled every retry → pre-compile (`d573406`)
- [x] **H4**: Validator compiled per call → VALIDATOR_CACHE DashMap (`83dcf49`)

## MEDIUM Impact — 7/10 DONE

- [x] **M1**: for_each task.clone() → Arc<AnalyzedTask> (`b7c2055`)
- [x] **M2**: compute_layers() called twice → cached_depths (`7247702`)
- [x] **M3**: event.clone() on every emit → conditional clone (`f7e2a6f`)
- [x] **M4**: lower_action() clones → borrow params (`76d4844`)
- [x] **M5**: Template value.clone() → pass &value (`9983fc3`)
- [x] **M6**: MCP call history Vec → VecDeque O(1) eviction (`cbc59f1`)
- [x] **M7**: Star animation → 5-second idle decay (`3cb1b3f`)
- [ ] **M8**: TUI DAG layout recomputed every frame (needs Widget lifetime refactor)
- [ ] **M9**: TUI DAG nodes/edges cloned every frame (needs ChatDagPanel lifetime param)
- [ ] **M10**: resolve_alias_path clones final Value (needs Cow propagation)

## LOW Impact — 5/8 DONE

- [x] **L1**: DAG O(V*E) → O(V+E) Kahn's algorithm (`e359f93`)
- [x] **L2**: Cost HashMap → FxHashMap for pricing tables (`dc516f3`)
- [x] **L3**: ProviderKind::parse to_lowercase → eq_ignore_ascii_case (`dc516f3`)
- [ ] **L4**: Template TransformExpr re-parsed per match (complex, low impact)
- [ ] **L5**: TUI Vec::remove(0) → VecDeque for 8 history buffers (churn vs impact)
- [x] **L6**: Event Vec pre-allocated with capacity 256 (committed with M3)
- [x] **L7**: Runner format! buffer reuse in for_each loop (`71e4653`)
- [x] **L8**: Runner dag_edges Vec pre-allocated (`71e4653`)

## Architecture Fixes — 2/2 DONE

- [x] **A1**: Provider command behind TUI feature gate → removed (`02c6848`)
- [x] **A2**: Layer 0 tool injection for from_example → fixed (`11ef88a`)

## Architecture (Long-term) — NOT STARTED

These are multi-session refactors:

### ARCH-5: Extract ContentBlock to nika-core (prerequisite for ARCH-2)
Moves ContentBlock + ResourceContent from nika-mcp to nika-core.
Breaks nika-media → nika-mcp dependency. ~100 LOC.

### ARCH-1: Extract nika-init crate (HIGH — saves ~13% compile)
20.9k LOC, 22 files. Only dependency: NikaError. Zero runtime coupling.

### ARCH-4: Seal nika-engine public API
Change pub mod → pub(crate) for: new, display, tools, registry.

### ARCH-2: Move builtin media tools to nika-media (HIGH — saves ~7% compile)
11k LOC. Depends on ARCH-5. Eliminates duplicate deps.

### ARCH-3: Split NikaError (91 variants → per-crate errors)
Highest risk. Fix NIKA-160 collision. Target: EngineError, RuntimeError, InitError.

## Summary

| Category | Done | Total | Progress |
|----------|------|-------|----------|
| HIGH | 4 | 4 | 100% |
| MEDIUM | 7 | 10 | 70% |
| LOW | 5 | 8 | 63% |
| Arch fixes | 2 | 2 | 100% |
| ARCH (long-term) | 0 | 5 | 0% |
| **Total** | **18** | **29** | **62%** |
