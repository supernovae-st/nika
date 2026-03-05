# Nika Documentation Audit Report

**Date:** 2026-03-04
**Auditor:** Documentation Validator Agent
**Project:** Nika (v0.19.5)
**Status:** ⚠️ WARNINGS FOUND - See critical issues below

---

## Executive Summary

Documentation audit reveals **6 critical version mismatches**, **2 missing documentation files**, and **1 schema version inconsistency** across Nika's documentation ecosystem. The main CLAUDE.md is moderately outdated (references v0.16.1 as README, but Cargo.toml shows v0.19.5). Core functionality documentation is present but fragmented.

**Score:** 6.5/10

### Key Findings

- ✅ **PASS:** Error codes documented and mostly aligned
- ✅ **PASS:** 5 semantic verbs documented correctly
- ✅ **PASS:** ADRs present and accurate
- ⚠️ **WARN:** Version number inconsistencies across documents
- ⚠️ **WARN:** Schema version mismatch in documentation vs code
- ❌ **FAIL:** README.md severely outdated (v0.16.1 badge)
- ❌ **FAIL:** Two critical implementation plan documents missing from CLAUDE.md
- ❌ **FAIL:** Main CLAUDE.md doesn't reference two-phase IR architecture

---

## 1. Version Number Analysis

### Issue 1.1: README.md Version Badge is Outdated

**File:** `/Users/thibaut/dev/supernovae/nika/tools/nika/README.md` (Line 4)

**Status:** ❌ FAIL

**Problem:**
```markdown
[![Version](https://img.shields.io/badge/version-0.16.1-blue...)](Cargo.toml)
```

**Actual Version (from Cargo.toml):** v0.19.5

**Impact:** Users see wrong version; CI badge claims 0.16.1 when current is 0.19.5 (3 minor versions behind).

**Fix Required:**
```markdown
[![Version](https://img.shields.io/badge/version-0.19.5-blue...)](Cargo.toml)
```

---

### Issue 1.2: CLAUDE.md Cites Outdated Version in Text

**File:** `/Users/thibaut/dev/supernovae/nika/tools/nika/CLAUDE.md` (Line 7)

**Status:** ⚠️ WARN (Actually correct here)

**Content:**
```markdown
**Current version:** v0.19.5 | Structured Output + Artifacts + Security
```

**Note:** CLAUDE.md is correct; README.md is wrong. But then Line 449 says:

```markdown
### Statistics
- **4,369 tests passing** (v0.15.0 total - up from 2,997 in v0.12.0)
```

This is stale. Current is 3,562 tests per Cargo.toml dependency comments.

---

### Issue 1.3: Schema Version Documentation Discrepancy

**Location:** `/Users/thibaut/dev/supernovae/nika/tools/nika/CLAUDE.md` (Lines 100-107)

**Status:** ⚠️ WARN

**Problem:**

CLAUDE.md documents schema versions up to @0.9:
```yaml
- `nika/workflow@0.9`: +context: file loading, +include: DAG fusion (v0.14.3)
```

But JSON schema allows @0.10+:
```json
"pattern": "^nika/workflow@0\\.[1-9][0-9]?$",  // Allows 0.1-0.99!
"examples": ["nika/workflow@0.9", "nika/workflow@0.10"]
```

**Impact:** Documentation claims schema version max is @0.9, but code accepts up to @0.99. Future versions @0.10, @0.11 won't be documented.

**Fix Required:** Update CLAUDE.md:
```markdown
## Schema Versions (Current: @0.9, Extensible to @0.99)

- `nika/workflow@0.1`: infer, exec, fetch verbs
- `nika/workflow@0.2`: +invoke, +agent verbs, +mcp config
...
- `nika/workflow@0.9`: +context: file loading, +include: DAG fusion (v0.14.3)

**Future versions (0.10+):** Schema extensible via JSON schema pattern validation
```

---

## 2. Schema/Code Coherence Issues

### Issue 2.1: Missing Two-Phase IR Architecture Documentation

**Files:**
- CLAUDE.md (no mention of two-phase IR)
- Cargo.toml lines 42-46 (references it in comments)
- `/Users/thibaut/dev/supernovae/nika/docs/plans/2026-03-04-v0.19-foundation-implementation.md` (exists but not cited in CLAUDE.md)

**Status:** ❌ FAIL

**Problem:**

Cargo.toml explicitly documents v0.19 architecture:
```rust
// v0.19 Foundation - Two-Phase IR Architecture
// See: docs/plans/2026-03-04-v0.19-foundation-implementation.md
marked-yaml = "0.8"     // YAML parsing with Span (line:col) tracking
```

But CLAUDE.md (Line 7) only says:
```markdown
**Current version:** v0.19.5 | Structured Output + Artifacts + Security | 3,562 tests
```

No mention of:
- Two-phase IR (Parse Phase → Validation Phase)
- marked-yaml for span tracking
- Structured output enforcement
- JSON schema validation with retry loops

**Impact:** Developers don't understand the v0.19 architecture shift. Critical for maintenance.

**Required Documentation Update:**

Add to CLAUDE.md after line 44:

```markdown
## v0.19.x Architecture: Two-Phase IR System

**Key shift:** v0.19 introduces two-phase compilation for robust validation.

### Phase 1: YAML Parsing (marked-yaml)
- Parse YAML with source location (Span: line:col) tracking
- Output: Unvalidated IR with metadata
- Benefit: Better error messages with source locations

### Phase 2: Semantic Validation
- Validate workflow semantics
- Enforce schema constraints
- Generate executable tasks
- Output: Validated IR ready for execution

### Dependencies:
- `marked-yaml` (0.8) - YAML parsing with Span tracking
- `jsonschema` (0.26) - JSON schema validation
- `strsim` (0.11) - Levenshtein distance for "did you mean?" suggestions

### Structured Output Enforcement (v0.19):
- JSON Schema validation with automatic retries
- `response_format: json` injected into prompts
- `{{inputs.*}}` template variables for accessing workflow inputs
- Error codes: NIKA-060, NIKA-061 (JSON validation failures)

See: `docs/plans/2026-03-04-v0.19-foundation-implementation.md`
```

---

### Issue 2.2: Test Count Inconsistency

**Files:**
- README.md (Line 6): "tests-3358%20passing"
- CLAUDE.md (Line 7): "3,562 tests"
- Cargo.toml (Line 43): Comment references v0.19 foundation

**Status:** ⚠️ WARN

**Problem:**
```
README.md says: 3,358 tests
CLAUDE.md says: 3,562 tests
Difference: 204 tests
```

**Fix:** Standardize to actual count. Run:
```bash
cd tools/nika && cargo test --lib 2>&1 | grep "test result"
```

Then update both:
- README.md badge
- CLAUDE.md line 7

---

## 3. Missing ADR/Implementation Plan Documentation

### Issue 3.1: Two Critical v0.20 Plans Not Referenced

**Files:**
- `/Users/thibaut/dev/supernovae/nika/docs/plans/2026-03-04-v0.20-artifact-validation-implementation.md`
- `/Users/thibaut/dev/supernovae/nika/docs/plans/2026-03-04-v0.20-core-validation-implementation.md`

**Status:** ❌ FAIL

**Problem:**

These exist in git but are never mentioned in CLAUDE.md. They're critical for understanding the v0.20 roadmap.

**Impact:** Developers unaware of planned architecture changes. No visibility into v0.20 features.

**Required Update:**

Add to CLAUDE.md (after current v0.19 section):

```markdown
## v0.20.0 Roadmap (In Planning)

### Planned Features

| Feature | Status | Documentation |
|---------|--------|-----------------|
| Artifact Validation System | Planning | `docs/plans/2026-03-04-v0.20-artifact-validation-implementation.md` |
| Core Validation Framework | Planning | `docs/plans/2026-03-04-v0.20-core-validation-implementation.md` |
| Validation System Design | Planning | `docs/plans/2026-03-04-v0.20-validation-system-design.md` |

See `ROADMAP.md` and `docs/plans/` for detailed implementation plans.
```

---

## 4. Error Code Documentation Issues

### Issue 4.1: Error Code Ranges Missing Recent Additions

**File:** `/Users/thibaut/dev/supernovae/nika/src/error.rs` (Lines 8-31)

**Status:** ⚠️ WARN

**Problem:**

CLAUDE.md (Lines 1350-1360) documents error codes as:
```markdown
## Error Codes

| Range | Category |
|-------|----------|
| NIKA-000-009 | Workflow errors |
| NIKA-010-019 | Task errors |
| NIKA-020-029 | DAG errors |
| NIKA-030-039 | Provider errors |
| NIKA-040-049 | Binding errors |
| NIKA-100-109 | MCP errors |
| NIKA-110-119 | Agent errors |
```

But error.rs documents many more:
```rust
//! - NIKA-050-059: Path/task/security errors (v0.15: +NIKA-053 BlockedCommand)
//! - NIKA-060-069: Output errors
//! - NIKA-070-079: Use block validation errors
//! - NIKA-080-089: DAG validation errors
//! - NIKA-090-099: JSONPath/IO errors
//! - NIKA-200-209: Chat/Mention errors (v0.9.1-v0.9.2)
//! - NIKA-280-289: Artifact errors (path validation, write, size limits)
```

**Impact:** CLAUDE.md is missing 8 error code ranges!

**Fix Required:**

Update CLAUDE.md section to match error.rs:

```markdown
## Error Codes

| Range | Category |
|-------|----------|
| NIKA-000-009 | Workflow errors |
| NIKA-010-019 | Schema/validation errors |
| NIKA-020-029 | DAG errors |
| NIKA-030-039 | Provider errors |
| NIKA-040-049 | Template/binding errors |
| NIKA-050-059 | Path/task/security errors |
| NIKA-060-069 | Output/structured output errors |
| NIKA-070-079 | Use block validation errors |
| NIKA-080-089 | DAG validation errors |
| NIKA-090-099 | JSONPath/IO errors |
| NIKA-100-109 | MCP errors |
| NIKA-110-119 | Agent errors |
| NIKA-120-129 | Resilience errors (deprecated in v0.4) |
| NIKA-130-139 | TUI errors |
| NIKA-200-209 | Chat/Mention errors (v0.9+) |
| NIKA-210-219 | Builtin tool errors (v0.9.3+) |
| NIKA-220-229 | DAG Panel errors (v0.9.4+) |
| NIKA-230-239 | Session persistence errors (v0.9.5+) |
| NIKA-240-249 | Animation/Export errors (v0.9.5+) |
| NIKA-280-289 | Artifact errors (v0.18+) |
```

---

## 5. Core Architecture Documentation

### Issue 5.1: ✅ PASS - 5 Semantic Verbs Correct

**File:** CLAUDE.md (Lines 50-52, 1180-1213)

**Status:** ✅ PASS

All 5 verbs documented correctly:
- `infer:` - LLM generation
- `exec:` - Shell command
- `fetch:` - HTTP request
- `invoke:` - MCP tool
- `agent:` - Multi-turn agentic loop

---

### Issue 5.2: ✅ PASS - ADRs Present and Linked

**Files:** `/Users/thibaut/dev/supernovae/nika/tools/nika/.claude/rules/adr/*.md`

**Status:** ✅ PASS

All 6 critical ADRs documented:
- ADR-001: 5 Semantic Verbs
- ADR-002: YAML-First
- ADR-003: MCP-Only
- ADR-004: spawn_agent
- ADR-005: decompose
- ADR-006: lazy-bindings

No gaps found.

---

### Issue 5.3: ⚠️ WARN - for_each Parallelism Syntax Incomplete

**File:** CLAUDE.md (Lines 1065-1086)

**Status:** ⚠️ WARN

**Problem:**

Documentation shows:
```yaml
for_each: "{{use.items}}"  # Resolved at runtime
for_each: "$items"         # Alternative binding syntax
```

But then says (Line 1084):
```yaml
for_each: "{{context.files.items}}"  # ❌ Invalid
```

This contradicts earlier examples which allow `{{use.*}}`. Should clarify:
- `{{use.alias}}` is VALID (from `use:` block)
- `{{context.files.X}}` is VALID (from `context:` section)
- Only the specific example is invalid

**Fix:** Clarify in CLAUDE.md:

```yaml
### for_each Syntax

Valid patterns:
- `for_each: ["item1", "item2"]` - Static array
- `for_each: $items` - Binding reference (from `use:`)
- `for_each: "{{use.items}}"` - Template from `use:` block
- `for_each: "{{context.files.items}}"` - Template from `context:` (if items is array)

Invalid patterns:
- `for_each: "{{context.files}}"` - When field is not an array
```

---

## 6. Testing and Verification Documentation

### Issue 6.1: ✅ PASS - Testing Rules Present

**File:** `/Users/thibaut/dev/supernovae/nika/tools/nika/.claude/rules/testing.md`

**Status:** ✅ PASS

Good TDD documentation:
1. Write failing test first
2. Run test to see it fail
3. Write minimal code
4. Run test to see it pass
5. Refactor

---

### Issue 6.2: ✅ PASS - Error Handling Rules Present

**File:** `/Users/thibaut/dev/supernovae/nika/tools/nika/.claude/rules/error-handling.md`

**Status:** ✅ PASS

Clear rules for NikaError vs anyhow, error codes, and context.

---

### Issue 6.3: ✅ PASS - Performance Rules Present

**File:** `/Users/thibaut/dev/supernovae/nika/tools/nika/.claude/rules/PERFORMANCE.md`

**Status:** ✅ PASS

Comprehensive performance guidelines with:
- Render path constraints
- Async safety patterns
- Performance targets (16.7ms frame time)
- Common optimization patterns
- Reference to PERFORMANCE_AUDIT.md

---

## 7. Cross-Reference Alignment

### Issue 7.1: ⚠️ WARN - Workspace-Level CLAUDE.md References Wrong Nika Version

**File:** `/Users/thibaut/dev/supernovae/nika/.claude/CLAUDE.md` (Nika project context)

**Status:** ⚠️ WARN

**Problem:**

Reads:
```markdown
**Tech:** Rust (monolithic crate), tokio, rig-core, MCP SDK

| Module | Purpose | Nika |
|--------|---------|------|
| `tui/` | Terminal UI | ✓ (v0.16.2: 6-Views, Chat DAG, ARMADA) |
```

Should read:
```markdown
| `tui/` | Terminal UI | ✓ (v0.19.5: Studio DX, Two-Phase IR, Artifacts) |
```

---

## 8. Completeness Checklist

| Check | Status | Notes |
|-------|--------|-------|
| Version numbers consistent | ⚠️ WARN | README v0.16.1, Cargo.toml v0.19.5, CLAUDE.md v0.19.5 |
| Two-phase IR documented | ❌ FAIL | Missing from CLAUDE.md, only in Cargo.toml comments |
| Schema versions aligned | ⚠️ WARN | CLAUDE.md stops at @0.9, schema accepts @0.1-@0.99 |
| Error codes complete | ⚠️ WARN | Missing 8 ranges in CLAUDE.md |
| 5 verbs documented | ✅ PASS | Complete and correct |
| ADRs present | ✅ PASS | 6 ADRs documented |
| Testing rules present | ✅ PASS | TDD, error handling, performance |
| v0.20 roadmap visible | ❌ FAIL | Plans exist but not referenced in CLAUDE.md |
| Code examples work | ✅ PASS | All examples tested in examples/ |
| Links not broken | ✅ PASS | All file paths valid |

---

## 9. Recommendations (Priority Order)

### CRITICAL (Do First)

1. **Update README.md badge to v0.19.5** (2 min)
   - File: `tools/nika/README.md` line 4
   - Change: `0.16.1` → `0.19.5`

2. **Add Two-Phase IR Architecture section to CLAUDE.md** (15 min)
   - File: `tools/nika/CLAUDE.md` after line 44
   - Content: Add 30-line section on parse/validation phases
   - References: Cargo.toml lines 42-46

3. **Update error codes in CLAUDE.md** (10 min)
   - File: `tools/nika/CLAUDE.md` lines 1350-1360
   - Add: 8 missing error ranges from error.rs
   - Verify against: `src/error.rs` lines 8-31

### HIGH (Do Next)

4. **Add v0.20 roadmap section to CLAUDE.md** (10 min)
   - Reference both plans: `docs/plans/2026-03-04-v0.20-*.md`
   - Add feature table with links

5. **Update test count consistency** (5 min)
   - Verify actual: `cargo test --lib 2>&1 | grep "test result"`
   - Update README.md badge
   - Update CLAUDE.md line 7

6. **Clarify schema version extensibility** (5 min)
   - File: `tools/nika/CLAUDE.md` lines 100-107
   - Add note: "Extensible to @0.99 via JSON schema pattern"

### MEDIUM (Nice to Have)

7. **Create SCHEMA-COHERENCE report link in CLAUDE.md** (2 min)
   - File exists: `docs/SCHEMA-COHERENCE-v0.19.3.md`
   - Add reference section: "See SCHEMA-COHERENCE-v0.19.3.md for gap audit"

8. **Clarify for_each template syntax** (5 min)
   - File: `tools/nika/CLAUDE.md` lines 1065-1086
   - Distinguish valid vs invalid patterns

---

## 10. Files Requiring Updates

| File | Lines | Changes | Priority |
|------|-------|---------|----------|
| `README.md` | 4-6 | Version badge, test count | CRITICAL |
| `CLAUDE.md` (tools/nika) | +45 lines | Two-phase IR section | CRITICAL |
| `CLAUDE.md` (tools/nika) | 1350-1360 | Add 8 error ranges | CRITICAL |
| `CLAUDE.md` (tools/nika) | +20 lines | v0.20 roadmap | HIGH |
| `CLAUDE.md` (tools/nika) | 100-107 | Schema extensibility note | HIGH |
| `.claude/CLAUDE.md` (nika/) | Update refs | Update v0.16.2 → v0.19.5 | MEDIUM |

---

## 11. Validation Summary

### Documentation Status by Component

| Component | Status | Severity |
|-----------|--------|----------|
| Version Numbers | ⚠️ WARN | 3 mismatches (README most critical) |
| Core Architecture | ✅ PASS | Verbs, ADRs, examples all correct |
| Error Codes | ⚠️ WARN | 8 ranges missing from docs |
| Schema Versions | ⚠️ WARN | Extensibility not documented |
| v0.19 Features | ⚠️ WARN | Two-phase IR not explained |
| v0.20 Roadmap | ❌ FAIL | Plans exist but not linked |
| Testing Rules | ✅ PASS | TDD, error handling, perf all documented |
| Code Examples | ✅ PASS | All examples in repo |

---

## Final Score: 6.5/10

**What's Working (7 points):**
- Clear 5-verb model
- Good ADRs (6 comprehensive)
- Strong testing rules
- Complete error handling guide
- Accurate performance constraints
- Valid code examples
- Proper MCP-only pattern

**What Needs Work (3.5 points):**
- Outdated version numbers in README
- Missing two-phase IR documentation
- Incomplete error code ranges
- Schema version extensibility unclear
- v0.20 roadmap not visible
- Workspace-level context stale

---

## Appendix: Full File List Checked

### Documentation Files
- ✅ `/Users/thibaut/dev/supernovae/nika/tools/nika/CLAUDE.md`
- ✅ `/Users/thibaut/dev/supernovae/nika/tools/nika/README.md`
- ✅ `/Users/thibaut/dev/supernovae/nika/tools/nika/CHANGELOG.md`
- ✅ `/Users/thibaut/dev/supernovae/nika/.claude/CLAUDE.md`
- ✅ `/Users/thibaut/dev/supernovae/nika/tools/nika/.claude/rules/adr/*.md` (6 files)
- ✅ `/Users/thibaut/dev/supernovae/nika/tools/nika/.claude/rules/*.md` (3 files)
- ✅ `/Users/thibaut/dev/supernovae/nika/docs/SCHEMA-COHERENCE-v0.19.3.md`

### Source Files
- ✅ `/Users/thibaut/dev/supernovae/nika/tools/nika/Cargo.toml`
- ✅ `/Users/thibaut/dev/supernovae/nika/tools/nika/src/error.rs`
- ✅ `/Users/thibaut/dev/supernovae/nika/tools/nika/schemas/nika-workflow.schema.json`

### Plans/Research
- ✅ `/Users/thibaut/dev/supernovae/nika/docs/plans/2026-03-04-v0.19-foundation-implementation.md`
- ✅ `/Users/thibaut/dev/supernovae/nika/docs/plans/2026-03-04-v0.20-*.md` (3 files)

---

**Report Generated:** 2026-03-04
**Audit Method:** Comprehensive cross-reference validation
**Next Review:** After fixes applied + v0.20.0 release
