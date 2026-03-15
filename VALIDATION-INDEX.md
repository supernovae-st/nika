# Spec Validator Report Index

**Generated:** 2026-02-25
**Validator:** Claude Code (Haiku 4.5)
**Project:** Nika v0.9.x (Chat-as-DAG Architecture)

---

## Documents (Read in Order)

### 1. START HERE → ALIGNMENT-EXECUTIVE-SUMMARY.md

**High-level overview** (5 min read)
- Status summary
- Key findings
- Timeline and risks
- Next steps

**Use this to:** Get the big picture before diving into details.

---

### 2. DETAILED ANALYSIS → SPEC-ALIGNMENT-REPORT.md

**Comprehensive validation** (30 min read)
- All 23 alignment issues detailed
- File-by-file checklist
- Architecture decisions
- Critical path blockers

**Use this to:** Understand what exists, what's missing, and why.

---

### 3. IMPLEMENTATION GUIDE → IMPLEMENTATION-CHECKLIST.md

**Practical per-phase checklist** (reference document)
- Pre-implementation tasks
- Phase-by-phase requirements
- Test criteria
- Git workflow

**Use this to:** Execute implementation systematically.

---

## Quick Links by Role

### For Architects (Thibaut)

1. **Read First:** ALIGNMENT-EXECUTIVE-SUMMARY.md (section "Key Architectural Decisions")
2. **Review:** SPEC-ALIGNMENT-REPORT.md (sections 4–5)
3. **Decide:** Dag replacement strategy (create ADR-????)
4. **Action:** Review pre-implementation blockers (section 6)

### For Engineers (Implementation Team)

1. **Read First:** ALIGNMENT-EXECUTIVE-SUMMARY.md (entire)
2. **Reference:** IMPLEMENTATION-CHECKLIST.md (per-phase)
3. **Deep Dive:** SPEC-ALIGNMENT-REPORT.md (as needed)
4. **Test:** Use WIRING checkpoints from checklist

### For Code Reviewers

1. **Read:** ALIGNMENT-EXECUTIVE-SUMMARY.md (section "Risk Assessment")
2. **Reference:** SPEC-ALIGNMENT-REPORT.md (section 4 "Import Verification")
3. **Verify:** Each phase against IMPLEMENTATION-CHECKLIST.md

---

## Critical Issues Requiring Decision

### Issue 1: Dag Migration Strategy

**Location:** SPEC-ALIGNMENT-REPORT.md, Section 3.A

**Decision Needed:** Replace vs Parallel Implementation
- **Option A (Replace):** Modify src/dag/flow.rs to use StableGraph
  - Pros: Simpler, one source of truth
  - Cons: Breaking change, affects all existing code using Dag
- **Option B (Parallel):** Create src/dag/stable.rs alongside flow.rs
  - Pros: Non-breaking, gradual migration
  - Cons: Code duplication, migration complexity later

**Who Decides:** Thibaut
**Deadline:** Before v0.9.0 starts
**Impact:** High - architectural decision

---

### Issue 2: EventLog Thread Safety Model

**Location:** SPEC-ALIGNMENT-REPORT.md, Section 4.C

**Question:** Is EventLog designed for concurrent write access from:
- Executor thread
- Builtin tool threads (v0.9.3)
- ChatWorkflow thread (v0.9.1)

**Current:** Executor passes `&mut EventLog` to tasks
**Planned:** Builtin tools need `&EventLog` (read access)

**Action Needed:** Document concurrency model or wrap with Mutex

**Who Decides:** Architecture team
**Deadline:** Before v0.9.3 starts
**Impact:** Medium - affects builtin tool design

---

### Issue 3: rig-core v0.31 API Verification

**Location:** SPEC-ALIGNMENT-REPORT.md, Section 4.B

**Verification:** Plans assume rig-core's `ToolDyn` trait has this signature:
```rust
pub trait ToolDyn: Send + Sync {
    fn name(&self) -> &str;
    fn definition(&self) -> ToolDefinition;
    fn call(&self, args: String) -> BoxFuture<'_, Result<String, ToolError>>;
}
```

**Action:** Check rig-core v0.31 documentation or tests

**Who Does It:** Engineer assigned to v0.9.3
**Deadline:** Before v0.9.3 starts
**Impact:** Critical - builtin tools depend on this

---

## What Exists vs What's Missing

### Existing (Foundation)

```
✅ src/dag/flow.rs              - Custom FxHashMap implementation
✅ src/event/log.rs             - Event sourcing system (22 variants)
✅ src/store/mod.rs             - RunContext for task outputs
✅ src/binding/mod.rs           - Data binding system (entry, resolve, template)
✅ src/tui/chat_agent.rs        - LLM interface (3,000+ LOC)
✅ src/tui/views/chat.rs        - Chat TUI view
✅ src/error.rs                 - Error enum (NIKA-0 through NIKA-119)
✅ src/runtime/executor.rs      - Task execution engine
✅ Cargo.toml                    - rig-core v0.31 + 25 other deps
```

### Missing (To Create)

```
❌ src/dag/stable.rs                           - StableGraph wrapper (v0.9.0)
❌ src/runtime/chat_workflow.rs                - ChatWorkflow struct (v0.9.1)
❌ src/binding/mention.rs                      - @mention parser (v0.9.2)
❌ src/runtime/builtin/mod.rs                  - Builtin tools module (v0.9.3)
❌ src/runtime/builtin/{router,sleep,...}     - 7 builtin tool files (v0.9.3)
❌ src/tui/widgets/chat_dag_panel.rs           - DAG visualization (v0.9.4)
❌ src/tui/widgets/{node_box,edge_line}.rs     - Widget components (v0.9.4)
❌ Cargo.toml: petgraph, humantime, evalexpr   - 3 new deps needed
```

---

## Validation Checklist

### Before Development Starts

- [ ] Add 3 dependencies to Cargo.toml (5 min)
- [ ] Verify rig-core v0.31 API (30 min)
- [ ] Clarify EventLog thread safety (30 min)
- [ ] Decide Dag migration strategy (1 hour)
- [ ] Add error codes to src/error.rs (30 min)

**Effort:** ~3 hours

### Per Phase (v0.9.0–v0.9.5)

- [ ] Create new modules per checklist
- [ ] Write tests first (TDD)
- [ ] Pass WIRING checkpoint
- [ ] Pass live test
- [ ] Zero clippy warnings

**Effort:** 10 sessions (2–3 weeks)

---

## Confidence Assessment

| Aspect | Confidence | Notes |
|--------|------------|-------|
| Architecture | ✅ High | Plans are well-designed, no conflicts |
| Feasibility | ✅ High | All required dependencies exist or easily added |
| Scope | ✅ High | Clear phase breakdown, test counts provided |
| Risk | ⚠️ Medium | Dag migration + thread safety need decisions |
| Timeline | ⚠️ Medium | Assumes focused 10-day effort, may extend with code review |

---

## File References

### Validation Documents (New)

1. **ALIGNMENT-EXECUTIVE-SUMMARY.md** (this repo, 3 KB)
   - Strategic overview
   - Risk/benefit analysis
   - Timeline estimates

2. **SPEC-ALIGNMENT-REPORT.md** (this repo, 30 KB)
   - Detailed findings (23 issues)
   - File-by-file validation
   - Architecture analysis

3. **IMPLEMENTATION-CHECKLIST.md** (this repo, 25 KB)
   - Per-phase tasks
   - Test requirements
   - Git workflow

4. **VALIDATION-INDEX.md** (this file, 8 KB)
   - Navigation guide
   - Critical decisions
   - Quick reference

### Plan Documents (Reference)

- `/nika/docs/plans/v0.9.1/README.md`
- `/nika/docs/plans/v0.9.1/ROADMAP-v09x.md`
- `/nika/docs/plans/v0.9.1/2026-02-24-stablegraph-migration-spec.md`
- `/nika/docs/plans/v0.9.1/2026-02-24-builtin-tools-spec.md`
- `/nika/docs/plans/v0.9.1/v0.9.2-MentionBindings.md`

### Source Code (Current)

- `/nika/tools/nika/Cargo.toml`
- `/nika/tools/nika/src/dag/flow.rs` (432 lines)
- `/nika/tools/nika/src/runtime/mod.rs`
- `/nika/tools/nika/src/binding/mod.rs`
- `/nika/tools/nika/src/event/log.rs`
- `/nika/tools/nika/src/error.rs`

---

## How to Use These Documents

### Scenario 1: "I need the executive summary"
→ Read **ALIGNMENT-EXECUTIVE-SUMMARY.md** (5 min)

### Scenario 2: "I'm starting v0.9.0 implementation"
→ Read **IMPLEMENTATION-CHECKLIST.md** (Pre-Implementation section)
→ Follow checklist for v0.9.0: StableGraph Foundation

### Scenario 3: "I found an issue during implementation"
→ Cross-reference with **SPEC-ALIGNMENT-REPORT.md** (Section 2)
→ Check **IMPLEMENTATION-CHECKLIST.md** (WIRING Checkpoints)

### Scenario 4: "I need to understand a specific alignment issue"
→ Use **SPEC-ALIGNMENT-REPORT.md** (detailed, searchable)

### Scenario 5: "I'm code reviewing v0.9.x work"
→ Use **IMPLEMENTATION-CHECKLIST.md** (Quality Gates section)
→ Reference **SPEC-ALIGNMENT-REPORT.md** (Import Verification)

---

## Success Criteria (Overall)

✅ All alignment issues resolved
✅ All new modules created following spec
✅ All 168 new tests passing
✅ WIRING checkpoints all pass
✅ Zero clippy warnings
✅ Chat-as-DAG fully operational (v0.9.5)

---

**Last Updated:** 2026-02-25
**Next Review:** After v0.9.0 implementation begins
**Contact:** Claude Code (Spec Validator Agent)
