# Chat-as-DAG: Async/Tokio Review — Complete Index

**Date:** 2026-02-24
**Reviewer:** Claude (rust-async-expert)
**Status:** Complete Review + Implementation Guide

---

## 📋 Document Overview

This folder contains a complete async/Tokio pattern review for the Chat-as-DAG implementation (v0.9.1).

### Quick Links

| Document | Purpose | Length | Time |
|----------|---------|--------|------|
| **ASYNC-SUMMARY.md** | Start here (findings + action items) | 5 pages | 15 min |
| **ASYNC-REVIEW.md** | Complete audit (16 sections) | 20 pages | 45 min |
| **ASYNC-IMPLEMENTATION-GUIDE.md** | Code examples + templates | 25 pages | 90 min |
| **ASYNC-PATTERNS-QUICK-REF.md** | Cheat sheet (while coding) | 8 pages | 10 min |

---

## 🎯 Reading Paths

### Path 1: Decision Maker (30 min)
1. This index (5 min)
2. ASYNC-SUMMARY.md (15 min)
3. Review § Risk Assessment

**Outcome:** Understand critical issues + timeline

### Path 2: Engineer (2 hours)
1. ASYNC-SUMMARY.md (20 min)
2. ASYNC-REVIEW.md § 1-10 (40 min)
3. ASYNC-IMPLEMENTATION-GUIDE.md § Part A (30 min)
4. ASYNC-PATTERNS-QUICK-REF.md § 1-5 (15 min)

**Outcome:** Ready to code Phase 1

### Path 3: Code Reviewer (1.5 hours)
1. ASYNC-REVIEW.md § 13-15 (code review checklist)
2. ASYNC-PATTERNS-QUICK-REF.md (all sections)
3. ASYNC-IMPLEMENTATION-GUIDE.md § Part C (testing)

**Outcome:** Ready to PR review

### Path 4: Complete Deep-Dive (4 hours)
Read all documents front-to-back in order:
1. This index
2. ASYNC-SUMMARY.md
3. ASYNC-REVIEW.md (complete)
4. ASYNC-IMPLEMENTATION-GUIDE.md (complete)
5. ASYNC-PATTERNS-QUICK-REF.md (complete)

**Outcome:** Mastery of patterns + implementation details

---

## 🔑 Key Findings at a Glance

### Critical Issues (Fix Before Phase 1)

| Issue | Severity | File | Fix | Time |
|-------|----------|------|-----|------|
| ChatWorkflow not thread-safe | HIGH | chat_workflow.rs | Add Arc<Mutex<>> + AtomicU32 | 30 min |
| Task ID collisions under concurrency | HIGH | chat_agent.rs | Use AtomicU32 for counter | 15 min |
| Locks held across .await | MEDIUM | chat_agent.rs | Release lock before execute | 20 min |

### Medium Issues (Fix Before Phase 3)

| Issue | Severity | File | Fix | Time |
|-------|----------|------|-----|------|
| DAG event queue unbounded | MEDIUM | event/log.rs | Add broadcast channel (1000 events) | 30 min |
| No DAG event subscription | MEDIUM | event/log.rs | Add subscribe() method | 20 min |
| Event backpressure unhandled | MEDIUM | chat.rs | Handle RecvError::Lagged | 25 min |

### Low Issues (Fix Before Release)

| Issue | Severity | File | Fix | Time |
|-------|----------|------|-----|------|
| Session save not atomic | LOW | session.rs | Use atomic_write() | 10 min |
| No concurrent infer test | LOW | tests/ | Add stress test | 45 min |

---

## ✅ Already Good (No Changes Needed)

| Pattern | File | Status |
|---------|------|--------|
| JoinSet for parallelism | runner.rs | Reuse as-is ✅ |
| DashMap + OnceCell caching | executor.rs | Reuse as-is ✅ |
| Mention parser (sync) | mention_parser.rs | Keep sync ✅ |
| Binding converter (pure) | mention_binding.rs | Keep pure ✅ |
| MCP timeout protection | executor.rs | Reuse as-is ✅ |

---

## 📊 Risk Summary

### Before Fixes
```
CRITICAL: 3 issues (task IDs, deadlock, concurrent access)
HIGH:     2 issues (locks across await, memory safety)
MEDIUM:   4 issues (backpressure, persistence, testing)
LOW:      2 issues (minor optimizations)
```

### After Proposed Fixes (Phase 1)
```
CRITICAL: 0 ✅
HIGH:     1 (performance - deferred to Phase 5)
MEDIUM:   2 (backpressure, testing)
LOW:      2 (optimizations)
```

---

## 🚀 Implementation Timeline

| Phase | Focus | Duration | Files | Tests |
|-------|-------|----------|-------|-------|
| **1** | Thread-safety + Mutex + Atomic | 2h | 3 | +5 |
| **2** | Mention parsing | 1h | 2 | +10 |
| **3** | DAG panel + broadcast | 3h | 4 | +8 |
| **4** | NodeBox enhancement | 1h | 1 | +5 |
| **5** | Polish + persistence | 2h | 3 | +45 |
| **Total** | | **9 hours** | **13** | **+73** |

---

## 📝 Document Structure

### ASYNC-SUMMARY.md
- Key findings (critical vs medium vs low)
- Risk assessment
- What to do NOW
- Code changes summary
- FAQ

**Read when:** You need quick overview or decision input

### ASYNC-REVIEW.md (Main Audit Document)

| Section | Topic | Pages |
|---------|-------|-------|
| 1 | Executive Summary | 2 |
| 2 | JoinSet (GOOD ✅) | 2 |
| 3 | DataStore (NEEDS REVIEW ⚠️) | 3 |
| 4 | Real-Time DAG Updates (BACKPRESSURE ⚠️) | 3 |
| 5 | MCP Client Caching (EXCELLENT ✅) | 2 |
| 6 | EventLog (CORRECT ✅) | 2 |
| 7 | ChatWorkflow Builder (GOOD ✅) | 1 |
| 8 | Mention Parser (SYNC ONLY ✅) | 1 |
| 9 | Concurrent Infer (DEADLOCK RISK ❌) | 2 |
| 10 | Session Persistence (RACE ⚠️) | 1 |
| 11 | Full Async Checklist | 1 |
| 12 | Recommended Changes | 3 |
| 13 | Performance Considerations | 2 |
| 14 | Race Conditions Audit | 1 |
| 15 | Deadlock Audit | 1 |
| 16 | Summary: Action Items | 1 |

**Read when:** You need complete technical analysis

### ASYNC-IMPLEMENTATION-GUIDE.md (Code Examples)

| Section | Topic | Pages |
|---------|-------|-------|
| **A** | Phase 1 Implementation | 8 |
| A1 | ChatWorkflow: Thread-Safe Counter | 3 |
| A2 | ChatAgent: Arc<Mutex<ChatWorkflow>> | 4 |
| A3 | Export EventLog | 1 |
| **B** | Phase 3 Implementation | 5 |
| B1 | Real-Time DAG with Broadcast | 3 |
| B2 | ChatDagPanel with Events | 2 |
| **C** | Testing Strategy | 4 |
| C1 | Concurrency Tests | 2 |
| C2 | Debugging Guide | 2 |
| **D** | Performance Targets | 1 |
| **E** | Checklist | 2 |

**Read when:** You're ready to code (copy-paste ready)

### ASYNC-PATTERNS-QUICK-REF.md (Cheat Sheet)

| Section | Topic |
|---------|-------|
| 1 | Lock Ordering: GOOD vs BAD |
| 2 | Atomic Operations |
| 3 | Broadcast Channel |
| 4 | DashMap + OnceCell |
| 5 | JoinSet |
| 6 | CancellationToken |
| 7 | Timeouts |
| 8 | Arc vs Box |
| 9 | Mutex Ordering |
| 10 | Error Propagation |
| 11 | Testing Concurrency |
| 12 | Debugging |
| **Quick Decision Tree** | |
| **Copy-Paste Boilerplate** | |

**Read when:** You need quick reference while coding

---

## 🎓 Learning Outcomes

### After Reading ASYNC-SUMMARY.md
- [ ] Understand 3 critical issues
- [ ] Know why Arc<Mutex<>> is needed
- [ ] Understand risk level
- [ ] Know what to implement first

### After Reading ASYNC-REVIEW.md
- [ ] Deep understanding of each pattern
- [ ] Know why patterns are good/bad
- [ ] Understand race conditions
- [ ] Understand deadlock risks
- [ ] Know performance targets

### After Reading ASYNC-IMPLEMENTATION-GUIDE.md
- [ ] Ready to implement Phase 1
- [ ] Understand testing strategy
- [ ] Know how to debug issues
- [ ] Have working code examples

### After Reading ASYNC-PATTERNS-QUICK-REF.md
- [ ] Quick reference for common mistakes
- [ ] Copy-paste templates ready
- [ ] Decision tree for pattern selection

---

## 🔍 Key Concepts Explained

### Arc<Mutex<T>>
**What:** Shared mutable state with async-safe locking
**Why:** Multiple tasks need to access ChatWorkflow
**Example:** `Arc<Mutex<ChatWorkflow>>`
**See:** ASYNC-IMPLEMENTATION-GUIDE.md § A2

### AtomicU32
**What:** Lock-free counter
**Why:** No contention on message ID generation
**Example:** `message_counter: Arc<AtomicU32>`
**See:** ASYNC-IMPLEMENTATION-GUIDE.md § A1

### Broadcast Channel
**What:** Multi-subscriber event queue
**Why:** DAG panel + chat view both need updates
**Example:** `broadcast::channel(1000)`
**See:** ASYNC-IMPLEMENTATION-GUIDE.md § B1

### JoinSet
**What:** Efficient task collection
**Why:** for_each with concurrency control
**Example:** Reuse from runner.rs
**See:** ASYNC-REVIEW.md § 1

### DashMap + OnceCell
**What:** Lock-free cache + atomic initialization
**Why:** MCP clients initialized once, shared forever
**Example:** Already in executor.rs
**See:** ASYNC-REVIEW.md § 5

---

## 🛠️ Tools Provided

### Templates
- [ ] ChatAgent struct template
- [ ] EventLog with broadcast
- [ ] Concurrent task loop
- [ ] Event loop with backpressure

**Location:** ASYNC-PATTERNS-QUICK-REF.md § Copy-Paste Boilerplate

### Tests
- [ ] Concurrent ID generation
- [ ] DAG broadcast
- [ ] Concurrent infers
- [ ] Stress test (50+ concurrent)

**Location:** ASYNC-IMPLEMENTATION-GUIDE.md § Part C

### Debugging Code
- [ ] Lock contention detection
- [ ] Event queue monitoring
- [ ] Tracing instrumentation

**Location:** ASYNC-IMPLEMENTATION-GUIDE.md § Part D

---

## 📦 What's NOT Covered

These are outside scope of async review:

- Visual design of DAG panel (design doc)
- UX copy/messaging (PM)
- Session storage format (schema doc)
- Export format (spec doc)
- Provider-specific APIs (rig-core docs)

**For those:** See linked documents in references

---

## ✋ When to Stop Reading

### If you're a...

**Project Manager:**
- Stop after ASYNC-SUMMARY.md
- You understand the risk + timeline

**Architect:**
- Read ASYNC-REVIEW.md
- You understand the patterns + tradeoffs

**Engineer (Implementing):**
- Read ASYNC-IMPLEMENTATION-GUIDE.md + ASYNC-PATTERNS-QUICK-REF.md
- Keep QUICK-REF.md open while coding

**Code Reviewer:**
- Read ASYNC-REVIEW.md § 13-15
- Keep QUICK-REF.md for review checklist

---

## 🤔 FAQ

### Q: Which document do I read first?
**A:** ASYNC-SUMMARY.md (15 min overview), then decide your path above.

### Q: Can I just skip to ASYNC-IMPLEMENTATION-GUIDE.md?
**A:** No. You'll miss critical context about **why** patterns are needed. Read ASYNC-REVIEW.md first.

### Q: Where's the TUI/UI code?
**A:** Not here. This is async patterns only. UI code is separate.

### Q: Will this pass code review?
**A:** Yes, if you follow the patterns and use the checklist in ASYNC-REVIEW.md § 16.

### Q: What if I find a bug in the docs?
**A:** File an issue referencing the document + section.

### Q: Can I use other async patterns?
**A:** Only if you fully understand the trade-offs. See ASYNC-REVIEW.md § 14 (deadlock audit).

---

## 📞 Support

### Questions About...

**Async patterns:**
→ See ASYNC-PATTERNS-QUICK-REF.md

**Implementation details:**
→ See ASYNC-IMPLEMENTATION-GUIDE.md

**Why a pattern is needed:**
→ See ASYNC-REVIEW.md

**Risk assessment:**
→ See ASYNC-REVIEW.md § 14-15

**Code review process:**
→ See ASYNC-REVIEW.md § 16

**Testing strategy:**
→ See ASYNC-IMPLEMENTATION-GUIDE.md § Part C

---

## ✅ Verification Checklist

Before implementing, verify:

- [ ] You've read ASYNC-SUMMARY.md
- [ ] You understand the 3 critical issues
- [ ] You know why Arc<Mutex<>> is needed
- [ ] You can explain AtomicU32 vs Mutex<u32>
- [ ] You understand broadcast channel backpressure
- [ ] You can point to working examples

---

## 📚 References

### In This Folder
1. ASYNC-SUMMARY.md (overview)
2. ASYNC-REVIEW.md (complete audit)
3. ASYNC-IMPLEMENTATION-GUIDE.md (code examples)
4. ASYNC-PATTERNS-QUICK-REF.md (cheat sheet)
5. ASYNC-INDEX.md (this file)

### In Codebase
- `src/runtime/runner.rs` — JoinSet usage
- `src/runtime/executor.rs` — DashMap + OnceCell pattern
- `src/event/log.rs` — EventLog structure
- `src/store/datastore.rs` — DataStore

### External
- Tokio Tutorial: https://tokio.rs/tokio/tutorial
- Tokio Sync Docs: https://docs.rs/tokio/1/tokio/sync/
- The Rustonomicon (async): https://doc.rust-lang.org/nomicon/
- Tokio Patterns: https://tokio.rs/tokio/topics

---

## 🎯 One-Liner Summary

**Chat-as-DAG needs Arc<Mutex<ChatWorkflow>> + AtomicU32 + broadcast channel to safely execute concurrent messages while keeping DAG real-time synchronized.**

---

**Status:** ✅ Complete Review
**Action:** Proceed to implementation (ready to code)
**Timeline:** 9 hours (1 engineer)
**Risk:** Medium → Low (with fixes)

