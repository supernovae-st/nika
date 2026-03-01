# Nika Specification Validation - Document Index

## Overview

This directory contains a comprehensive validation of the Nika specification against the current v0.15.1 codebase. The validation revealed critical gaps between specification and implementation.

**Overall Status:** CRITICAL - Spec is 13 months out of date

---

## Report Documents

### 1. SPEC_VALIDATION_REPORT.md (Primary Report)
**Length:** ~500 lines | **Read time:** 20-30 min

Complete technical validation with:
- Executive summary (Status: FAIL, Score: 3.5/10)
- 6 critical issues identified
- Structure validation (2/6 PASS)
- Completeness validation (0/5 PASS)
- Consistency validation (2/4 PASS)
- Quality validation (2/3 PASS)
- Detailed findings organized by issue
- Recommendations (Priority 1-3)
- Effort estimate (20.5 hours)

**Start here if you want:** Complete technical validation

### 2. SPEC_VALIDATION_SUMMARY.txt (Executive Summary)
**Length:** ~150 lines | **Read time:** 5-10 min

Condensed executive summary with:
- Final score (3.5/10)
- 6 critical gaps (bullets only)
- Validation checklist results
- Scope of changes needed
- Effort estimate
- Impact on users
- Bottom-line recommendation

**Start here if you want:** Quick overview (5 min read)

### 3. SPEC_GAPS_DETAILED.md (Implementation Guide)
**Length:** ~400 lines | **Read time:** 20-25 min

Detailed guide showing exactly what needs to be added:
- Missing verb #1: `invoke:` (MCP tools)
- Missing verb #2: `agent:` (multi-turn loops)
- Missing feature #1: `for_each` parallelism
- Missing feature #2: `context:` file loading
- Missing feature #3: `include:` DAG fusion
- Missing feature #4: `decompose:` runtime expansion
- Missing feature #5: `lazy:` deferred bindings
- Missing feature #6: Security (shell-free execution)
- Missing providers (4 out of 7)
- Complete error code reference

Each section includes:
- YAML examples
- Parameter tables
- Error codes
- Implementation details

**Start here if you want:** To write the new spec sections

---

## Key Statistics

### Validation Scores
| Component | Score | Status |
|-----------|-------|--------|
| Structure completeness | 2/6 (33%) | FAIL |
| Completeness validation | 0/5 (0%) | FAIL |
| Consistency validation | 2/4 (50%) | PARTIAL |
| Quality validation | 2/3 (67%) | PARTIAL |
| **OVERALL** | **3.5/10** | **FAIL** |

### Gaps Identified
| Category | Spec | Code | Gap |
|----------|------|------|-----|
| Semantic verbs | 3 | 5 | 2 missing |
| Error codes | 41 | 192 | 151 missing (78%) |
| Providers | 3 | 7 | 4 missing |
| Schema versions | 1 (@0.1) | 9 (@0.1-@0.9) | 8 missing |
| Advanced features | 0 | 6+ | All missing |

### Effort Estimate
| Task | Effort | Impact |
|------|--------|--------|
| Critical fixes (5 items) | 8.5 hours | Unblocks users |
| High-impact additions (4 sections) | 7.5 hours | Enables modern features |
| Medium priority (3 sections) | 4.5 hours | Completes picture |
| **TOTAL** | **20.5 hours** | **Full rewrite** |

---

## Critical Issues at a Glance

### Issue #1: Schema Version Mismatch
- **Spec says:** v0.1 only (`nika/workflow@0.1`)
- **Code has:** v0.1 through v0.9
- **Gap:** 8 schema versions undocumented
- **Impact:** Users cannot use 95% of features

### Issue #2: Missing Semantic Verbs
- **Spec documents:** 3 verbs (infer, exec, fetch)
- **Code implements:** 5 verbs (+invoke, agent)
- **Gap:** 2 critical verbs completely undocumented
- **Impact:** Cannot use MCP tools or agent loops

### Issue #3: Error Code Coverage
- **Spec documents:** 41 error codes
- **Code has:** 192 error codes
- **Gap:** 151 codes (78%) missing
- **Impact:** No way to understand failures

### Issue #4: Provider Support
- **Spec lists:** 3 providers (Claude, OpenAI, Mock)
- **Code supports:** 7 providers (+Mistral, Groq, DeepSeek, Ollama, Gemini)
- **Gap:** 4 providers missing
- **Impact:** Users cannot discover alternate providers

### Issue #5: Advanced Features
- **Spec shows:** Basic workflow only (v0.1)
- **Code includes:** 6+ advanced features (v0.3+)
- **Examples:** for_each, context:, include:, decompose:, lazy:, spawn_agent
- **Impact:** Cannot express modern patterns

### Issue #6: Outdated Type Signatures
- **Task struct:** Missing 4+ fields
- **UseEntry:** Missing lazy flag
- **Workflow:** Missing context, include, skills fields
- **Impact:** Type definitions don't match code

---

## Recommended Actions

### Phase 1: Quick Wins (Today) - 2 hours
1. [ ] Update spec header to v0.15.1, schema @0.9
2. [ ] Add schema version history table (v0.1-v0.9)
3. [ ] Add provider reference table (all 7)
4. [ ] Fix "Last Updated" date

### Phase 2: Critical Sections (This week) - 8.5 hours
1. [ ] Document `invoke:` verb completely
2. [ ] Document `agent:` verb completely
3. [ ] Document all 192 error codes (organized by range)
4. [ ] Update all Rust type definitions
5. [ ] Add MCP integration section

### Phase 3: Advanced Features (Next week) - 7.5 hours
1. [ ] Add for_each documentation
2. [ ] Add context: and include: sections
3. [ ] Add decompose: and lazy: sections
4. [ ] Add spawn_agent tool documentation
5. [ ] Add security section (shell: false, path protection)

### Phase 4: Polish (Following week) - 4.5 hours
1. [ ] Add TUI/Studio documentation
2. [ ] Refresh all example workflows (use v0.9 schema)
3. [ ] Add advanced use cases
4. [ ] Add troubleshooting section

---

## Files Affected

**To be rewritten:**
- `/Users/thibaut/supernovae-st/supernovae-agi/nika/spec/SPEC.md` (MAIN FILE)

**Source of truth for rewrite:**
- `/Users/thibaut/supernovae-st/supernovae-agi/nika/tools/nika/src/` (code)
- `/Users/thibaut/supernovae-st/supernovae-agi/nika/tools/nika/CLAUDE.md` (context)
- `/Users/thibaut/supernovae-st/supernovae-agi/nika/CHANGELOG.md` (history)
- `/Users/thibaut/supernovae-st/supernovae-agi/nika/README.md` (overview)

---

## How to Use This Validation

### For Quick Review (10 min)
1. Read: SPEC_VALIDATION_SUMMARY.txt
2. Skim: "Critical Issues at a Glance" section above

### For Detailed Analysis (30 min)
1. Read: SPEC_VALIDATION_SUMMARY.txt
2. Read: SPEC_VALIDATION_REPORT.md (sections 1-6)
3. Skim: SPEC_GAPS_DETAILED.md examples

### For Implementing Fixes (Full effort)
1. Read: SPEC_VALIDATION_REPORT.md (Recommendations section)
2. Use: SPEC_GAPS_DETAILED.md (copy examples for each section)
3. Reference: SPEC_VALIDATION_SUMMARY.txt (effort estimates)

### For Stakeholders/Management
1. Read: SPEC_VALIDATION_SUMMARY.txt (5 min)
2. Show: "Final Score: 3.5/10" and "Critical Issues" sections
3. Action: Approve 20.5-hour rewrite effort

---

## Next Steps

**Immediate (1 day):**
1. Review this validation with the team
2. Decide on rewrite vs. incremental fix (RECOMMEND: full rewrite)
3. Schedule 20.5 hours for Phase 1-4

**Short-term (1 week):**
1. Complete Phase 1 (quick wins)
2. Block Phase 2 (critical sections) - most important
3. Get review/approval

**Medium-term (2 weeks):**
1. Complete Phase 2 and 3
2. Update CI/CD to validate spec against code
3. Add spec version to spec header with release notes

**Long-term (ongoing):**
1. Keep spec and code synchronized
2. Add CI check: spec schema version must match code
3. Update spec with each code release

---

## Questions?

For detailed explanations of each gap, see:
- **SPEC_GAPS_DETAILED.md** for "what's missing" with examples
- **SPEC_VALIDATION_REPORT.md** for "why it matters" with impact analysis
- Code references in `/tools/nika/src/` for implementation details

---

**Generated:** 2026-03-01
**Status:** Final Validation Report
**Confidence:** High (verified against code)
**Action Required:** YES - Urgent rewrite needed

