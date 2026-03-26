# Plan: v0.47.0 — Performance + Architecture Optimization

> Results from 18 review agents across 2 swarms.
> 4 HIGH, 10 MEDIUM, 8 LOW findings. Estimated: ~600 LOC changes.

## HIGH Impact — ✅ ALL DONE

- [x] **H1**: TUI chat message items cloned every frame → extend pattern (`3f461d2`)
- [x] **H2**: TUI Clear+set_style overwrites all cells → removed (`7be97fa`)
- [x] **H3**: jsonschema recompiled every retry → pre-compile (`d573406`)
- [x] **H4**: Validator compiled per call → VALIDATOR_CACHE DashMap (`83dcf49`)

## MEDIUM Impact — ✅ 9/10 DONE

- [x] **M1**: for_each task.clone() → Arc<AnalyzedTask> (`b7c2055`)
- [x] **M2**: compute_layers() called twice → cached_depths (`7247702`)
- [x] **M3**: event.clone() on every emit → conditional clone (`f7e2a6f`)
- [x] **M4**: lower_action() clones → borrow params (`76d4844`)
- [x] **M5**: Template value.clone() → pass &value (`9983fc3`)
- [x] **M6**: MCP call history Vec → VecDeque O(1) eviction (`cbc59f1`)
- [x] **M7**: Star animation → 5-second idle decay (`3cb1b3f`)
- [x] **M8**: DAG layout cached via static hash → eliminates O(N³) Sugiyama per frame (`761810f`)
- [ ] **M9**: TUI DAG nodes/edges cloned every frame (needs ChatDagPanel ownership restructure)
- [x] **M10**: NOT FEASIBLE — intermediate JSON parsing creates temporaries incompatible with Cow

## LOW Impact — ✅ 7/8 DONE

- [x] **L1**: DAG O(V*E) → O(V+E) Kahn's algorithm (`e359f93`)
- [x] **L2**: Cost HashMap → FxHashMap for pricing tables (`dc516f3`)
- [x] **L3**: ProviderKind::parse to_lowercase → eq_ignore_ascii_case (`dc516f3`)
- [ ] **L4**: Template TransformExpr re-parsed per match (LOW ROI, needs arch refactor)
- [x] **L5**: TUI Vec::remove(0) → VecDeque across 8 locations (`3385739`)
- [x] **L6**: Event Vec pre-allocated with capacity 256 (committed with M3)
- [x] **L7**: Runner format! buffer reuse in for_each loop (`71e4653`)
- [x] **L8**: Runner dag_edges Vec pre-allocated (`71e4653`)

## Architecture Fixes — ✅ 2/2 DONE

- [x] **A1**: Provider command behind TUI feature gate → removed (`02c6848`)
- [x] **A2**: Layer 0 tool injection for from_example → fixed (`11ef88a`)

## Architecture — 2/5 DONE

- [x] **ARCH-4**: Seal nika-engine public API → NO-OP: all pub modules used externally
- [x] **ARCH-5**: Extract ContentBlock to nika-core (`2147ccc`) — enables ARCH-2

### Remaining (multi-session refactors):

- [ ] **ARCH-1**: Extract nika-init crate (20.9k LOC, 22 files → new crate, ~13% compile savings)
- [ ] **ARCH-2**: Move media tools to nika-media (11k LOC, depends on ARCH-5 ✓, ~7% compile)
- [ ] **ARCH-3**: Split NikaError (91 variants → per-crate errors, highest risk)

## Summary

| Category | Done | Total | Progress |
|----------|------|-------|----------|
| HIGH | 4 | 4 | 100% |
| MEDIUM | 9 | 10 | 90% |
| LOW | 7 | 8 | 88% |
| Arch fixes | 2 | 2 | 100% |
| ARCH | 2 | 5 | 40% |
| **Total** | **24** | **29** | **83%** |
