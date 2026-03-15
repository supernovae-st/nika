# Spec-Code Alignment: Executive Summary

**Date:** 2026-02-25
**Status:** VALIDATED ✓ with 23 resolvable issues

---

## Overview

The v0.9.x plans (Chat-as-DAG architecture) are **well-designed and architecturally sound**. The current codebase provides a solid foundation. However, the plans reference several modules and dependencies that don't yet exist.

**Bottom Line:** No blocking architectural conflicts. All issues are **resolvable in normal development workflow** by:
1. Adding 3 missing dependencies to Cargo.toml
2. Creating 8 new modules following spec templates
3. Adding error codes and updating imports

---

## The Good News

✅ **Current architecture is clean:**
- Modular structure (`ast/`, `dag/`, `runtime/`, `binding/`, `tui/`)
- Clear separation of concerns
- Async/await patterns established
- Error handling pattern (NikaError) in place

✅ **Foundation already exists for all v0.9.x features:**
- `EventLog` (src/event/log.rs) exists for event sourcing
- `RunContext` (src/store/mod.rs) exists for data flow
- `ChatAgent` (src/tui/chat_agent.rs) provides LLM interface
- rig-core v0.31 already in Cargo.toml (supports 6 providers)

✅ **Plans are internally consistent:**
- ROADMAP-v09x.md aligns with detailed specs
- Version breakdown (v0.9.0–v0.9.5) is logical
- WIRING checkpoints defined for integration
- Test counts specified (168 new tests)

---

## The Issues (All Resolvable)

### Critical (3) — Add to Cargo.toml

```toml
petgraph = "0.6"        # StableGraph for stable NodeIndex (v0.9.0)
humantime = "2.1"       # Duration parsing for nika:sleep (v0.9.3)
evalexpr = "11.0"       # Expression evaluation for nika:assert (v0.9.3)
```

**Effort:** 2 minutes

### High (8) — Create New Modules

| Phase | Module | LOC | Complexity |
|-------|--------|-----|-----------|
| v0.9.0 | `src/dag/stable.rs` or refactor `flow.rs` | 400 | Medium |
| v0.9.1 | `src/runtime/chat_workflow.rs` | 300 | Medium |
| v0.9.2 | `src/binding/mention.rs` | 400 | Medium |
| v0.9.3 | `src/runtime/builtin/` (7 files) | 870 | Medium |
| v0.9.4 | `src/tui/widgets/{chat_dag_panel,node_box,edge_line}.rs` | 850 | High |

**Total New Code:** ~3,700 LOC across 16 files

**Effort:** ~3 weeks of focused development (spec templates provided)

### Medium (4) — Verify/Clarify

1. **rig-core v0.31 API** - Verify ToolDyn trait signature
2. **EventLog thread safety** - Confirm Arc<Mutex<>> compatibility
3. **Dag migration** - Replace vs parallel implementation (choose one)
4. **@mention precedence** - Context (@entity:) vs message (@1) system design

**Effort:** 2–4 hours (mostly clarification, no code changes needed yet)

### Low (8) — Update Existing Files

- Add error codes NIKA-260..265 and NIKA-200..299 to `src/error.rs`
- Update `mod.rs` files with new module exports
- Verify imports in existing code

**Effort:** 1–2 hours

---

## Key Architectural Decisions

### 1. Dag → StableGraph Migration

**Decision:** Replace custom `FxHashMap`-based `Dag` with `petgraph::StableGraph`

**Why:** Enables stable `NodeIndex` for @mention references. Chat messages will be stable even after deletion.

**Trade-off:** ~5-10% performance regression (acceptable for this feature)

**Timeline:** v0.9.0 (1 session)

---

### 2. ChatWorkflow as Separate Struct

**Decision:** Create `ChatWorkflow` (new) alongside existing `ChatAgent`

**Architecture:**
```
ChatView
├── ChatAgent (LLM interface) ← existing
└── ChatWorkflow (message DAG) ← new
```

**Why:** Separates concerns. ChatAgent handles LLM, ChatWorkflow handles message structure.

**No conflicts** - clean coexistence.

---

### 3. Two Mention Systems

**Context Mentions** (current):
- `@entity:qr-code` - NovaNet entities
- `@file:README.md` - Local files
- `@locale:fr-FR` - Locale context
- Handled by `MentionSystem` widget

**Message Mentions** (planned):
- `@1` - First message in chat
- `@last` - Last message
- `@all` - All messages
- `@1..3` - Range
- Handled by `MentionParser` (new)

**Decision:** Both coexist. Parser precedence: explicit prefix (@entity:) before implicit message index.

---

### 4. Builtin Tools as Internal ToolDyn

**Decision:** Implement 6 tools (`nika:sleep`, `nika:log`, etc.) as rig::ToolDyn

**Router:** Prefix routing:
- `nika:*` → `BuiltinToolRouter` (internal)
- `server:*` → `McpClient` (external)

**Why:** Preserves ADR-001 (5 semantic verbs only). Tools are implementation detail of `invoke:`.

---

## Timeline Estimate

| Phase | Version | Effort | Tests | Blocker |
|-------|---------|--------|-------|---------|
| Pre-work | — | 1 day | — | Add Cargo.toml deps |
| Phase 1 | v0.9.0 | 1 session | 25 | StableGraph migration |
| Phase 2 | v0.9.1 | 1 session | 20 | ChatWorkflow struct |
| Phase 3 | v0.9.2 | 2 sessions | 35 | @mention parser |
| Phase 4 | v0.9.3 | 2 sessions | 45 | Builtin tools |
| Phase 5 | v0.9.4 | 2 sessions | 25 | TUI widgets |
| Phase 6 | v0.9.5 | 1 session | 18 | Polish |
| **Total** | — | **10 sessions** | **168** | — |

**Assumption:** 1 session = 1 focused day (6–8 hours)

---

## Risk Assessment

### Low Risk (Proceed Immediately)

- ✅ Module creation (spec templates provided)
- ✅ Error code additions (straightforward)
- ✅ Import updates (mechanical)
- ✅ Testing framework (existing patterns)

### Medium Risk (Requires Verification)

- ⚠️ Dag replacement (breaking change, but scoped to v0.9.0)
- ⚠️ ChatWorkflow integration (new async patterns, covered by WIRING tests)
- ⚠️ Builtin tool dispatch (needs rig-core API confirmation)

### Mitigation

1. **Pre-work verification** - Confirm rig-core API before Phase 4
2. **WIRING checkpoints** - Integration tests after each phase prevent cascading failures
3. **Comprehensive tests** - 168 new tests provide safety net
4. **Code review** - Feature-dev:code-reviewer skill for quality gates

---

## Success Criteria

Phase passes when:

1. ✅ All new tests pass (`cargo test`)
2. ✅ Zero clippy warnings (`cargo clippy -- -D warnings`)
3. ✅ WIRING checkpoint passes (integration test)
4. ✅ Live test succeeds (manual TUI verification)
5. ✅ Code review approved

---

## Next Steps

### For Thibaut (Architect)

1. **Decide:** Replace vs parallel Dag implementation (create ADR)
2. **Verify:** rig-core v0.31 ToolDyn API (10 minutes)
3. **Review:** SPEC-ALIGNMENT-REPORT.md for any concerns
4. **Approve:** Go/no-go for v0.9.0 start

### For Development Team

1. **Add dependencies** to Cargo.toml (5 min)
2. **Read** IMPLEMENTATION-CHECKLIST.md
3. **Follow** TDD pattern (test first, then code)
4. **Run** WIRING checkpoints after each phase
5. **Commit** frequently with clear messages

---

## Documents

This validation package includes:

1. **SPEC-ALIGNMENT-REPORT.md** (detailed, 11 sections)
   - File-by-file analysis
   - Architecture assumptions
   - Critical path blockers

2. **IMPLEMENTATION-CHECKLIST.md** (practical, per-phase)
   - Pre-implementation tasks
   - File creation/modification lists
   - Test requirements
   - Git workflow

3. **ALIGNMENT-EXECUTIVE-SUMMARY.md** (this file)
   - High-level overview
   - Timeline and risk
   - Next steps

---

## Appendix: File Locations

**Report Files (New):**
- `/Users/thibaut/supernovae-st/supernovae-agi/nika/SPEC-ALIGNMENT-REPORT.md`
- `/Users/thibaut/supernovae-st/supernovae-agi/nika/IMPLEMENTATION-CHECKLIST.md`
- `/Users/thibaut/supernovae-st/supernovae-agi/nika/ALIGNMENT-EXECUTIVE-SUMMARY.md`

**Plan Files (Reference):**
- `/Users/thibaut/supernovae-st/supernovae-agi/nika/docs/plans/v0.9.1/README.md`
- `/Users/thibaut/supernovae-st/supernovae-agi/nika/docs/plans/v0.9.1/ROADMAP-v09x.md`
- `/Users/thibaut/supernovae-st/supernovae-agi/nika/docs/plans/v0.9.1/2026-02-24-stablegraph-migration-spec.md`
- `/Users/thibaut/supernovae-st/supernovae-agi/nika/docs/plans/v0.9.1/2026-02-24-builtin-tools-spec.md`

**Current Code:**
- `/Users/thibaut/supernovae-st/supernovae-agi/nika/tools/nika/Cargo.toml`
- `/Users/thibaut/supernovae-st/supernovae-agi/nika/tools/nika/src/dag/flow.rs` (432 lines)
- `/Users/thibaut/supernovae-st/supernovae-agi/nika/tools/nika/src/runtime/mod.rs`
- `/Users/thibaut/supernovae-st/supernovae-agi/nika/tools/nika/src/binding/mod.rs`

---

## Recommendation

**Status:** ✅ **APPROVED FOR DEVELOPMENT**

The v0.9.x specification is well-founded and ready for implementation. No architectural blockers. Proceed with:

1. Pre-work (add Cargo.toml deps)
2. Phase-by-phase implementation following IMPLEMENTATION-CHECKLIST.md
3. WIRING checkpoint verification after each phase
4. Full test suite validation at end

**Timeline:** 10 focused sessions (2–3 weeks)

---

**Report Prepared By:** Claude Code (Specification Validator)
**Date:** 2026-02-25
**Confidence Level:** High (detailed analysis, no showstoppers)
