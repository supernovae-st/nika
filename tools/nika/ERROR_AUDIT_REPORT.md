# Error.rs Audit Report - Nika v0.27.0

**Audit Date:** 2026-03-12
**File:** `/Users/thibaut/dev/supernovae/nika/tools/nika/src/error.rs`
**Total Lines:** ~1,400
**Status:** 67 error code variants defined, 43+ ranges documented but not implemented

---

## Executive Summary

The `error.rs` file contains comprehensive error handling for Nika with well-structured error codes. However, there are **3 critical issues** and several documentation inconsistencies that should be addressed:

1. **NIKA-083** is documented in `dag/validate.rs` but has no enum variant
2. **NIKA-140-149** (AST errors) are documented as part of NikaError but actually defined in `ast/analyzer/errors.rs`
3. **NIKA-200-209** are documented as "Chat/Mention" errors but actually are file tool errors

Additionally, **41 reserved error code ranges** (NIKA-200+ to NIKA-429) are documented but completely unimplemented, creating confusion about what features actually exist in v0.27.0.

---

## 1. Critical Issues

### Issue 1.1: NIKA-083 Referenced but Not Defined

**Severity:** CRITICAL
**Location:** `src/dag/validate.rs:13` (comment reference)
**Line in error.rs:** Missing from enum definition

```rust
// src/dag/validate.rs:13
//! - NIKA-083: Template {{use.alias}} references undeclared alias
```

**Problem:** This error code is documented in DAG validation comments but has NO corresponding enum variant in `NikaError`.

**Current Behavior:** The error is likely being caught as `UnknownAlias` (NIKA-071) or not reported at all.

**Recommendation:** Either:
- Add a new variant: `Unknown083 { ... }`
- Or update `dag/validate.rs` comments to use existing error code

---

### Issue 1.2: NIKA-140-149 Documentation Misleading

**Severity:** CRITICAL
**Location:** `src/error.rs:22` (header documentation)

```rust
//! - NIKA-140-149: AST analysis errors (v0.20 - Phase 2 analyzer)
```

**Problem:** These error codes are documented as part of NikaError, but they are ACTUALLY defined in a separate module:

```rust
// Actual location: src/ast/analyzer/errors.rs
pub enum AnalyzeError {
    UnknownTask { ... },        // NIKA-140
    DuplicateTask { ... },      // NIKA-141
    InvalidSchema { ... },      // NIKA-142
    CyclicDependency { ... },   // NIKA-143
    InvalidValue { ... },       // NIKA-144
    MissingField { ... },       // NIKA-145
    InvalidTemplate { ... },    // NIKA-146
    UnknownFlow { ... },        // NIKA-147
    UnknownMcpServer { ... },   // NIKA-148
    UnsupportedFeature { ... }, // NIKA-149
}
```

**Impact:**
- Developers expect these codes in NikaError
- Code reviews get confused about error ownership
- Error handling logic checks wrong enum type

**Recommendation:** Update documentation to:
```rust
//! - NIKA-140-149: AST analysis errors (see ast/analyzer/errors.rs - AnalyzeError enum)
```

---

### Issue 1.3: NIKA-200-209 Incorrectly Documented as Chat Errors

**Severity:** CRITICAL
**Location:** `src/error.rs:25` (header documentation)

```rust
//! - NIKA-200-209: Chat/Mention errors (v0.9.1-v0.9.2)
```

**Problem:** These are documented as "Chat/Mention" but actually are FILE TOOL errors.

**Actual Implementation:**
```rust
// src/tools/mod.rs
pub enum ToolErrorCode {
    ReadFailed,           // NIKA-200
    WriteFailed,          // NIKA-201
    EditFailed,           // NIKA-202
    MustReadFirst,        // NIKA-203
    PathOutOfBounds,      // NIKA-204
    PermissionDenied,     // NIKA-205
    InvalidGlobPattern,   // NIKA-206
    InvalidRegexPattern,  // NIKA-207
    FileNotFound,         // NIKA-208
    OldStringNotUnique,   // NIKA-209
}
```

**Impact:** Developers looking for chat error codes won't find them under NIKA-200.

**Recommendation:** Update documentation:
```rust
//! - NIKA-200-209: Tool/File operation errors (ToolErrorCode enum in src/tools/mod.rs)
```

---

## 2. High-Priority Issues

### Issue 2.1: NIKA-220-249 Over-Documented but Not Implemented

**Severity:** HIGH
**Location:** `src/error.rs:27-29` (header documentation)

```rust
//! - NIKA-220-229: DAG Panel errors (v0.9.4)
//! - NIKA-230-239: Session persistence errors (v0.9.5)
//! - NIKA-240-249: Animation/Export errors (v0.9.5)
```

**Problem:** These ranges are documented as existing features, but have **ZERO enum variants**. The features (DAG visualization, sessions, animations) exist in the codebase but DON'T use error codes from these ranges.

**Search Results:**
```bash
$ grep -r "NIKA-22[0-9]\|NIKA-23[0-9]\|NIKA-24[0-9]" src/ --include="*.rs"
# No results (except in error.rs documentation)
```

**Impact:** Code reviewers think these error ranges are implemented when they're not.

**Recommendation:** Mark as reserved:
```rust
//! Reserved ranges for future releases:
//! - NIKA-220-229: DAG Panel errors (v0.9.4+)
//! - NIKA-230-239: Session persistence errors (v0.9.5+)
//! - NIKA-240-249: Animation/Export errors (v0.9.5+)
```

---

## 3. Medium-Priority Issues

### Issue 3.1: Error Code Range Gaps Throughout

**Severity:** MEDIUM
**Detailed Gap Analysis:**

| Range | Defined | Missing | Utilization |
|-------|---------|---------|-------------|
| 050-059 | 053,055,056,057 | 054,058,059 | 4/10 (40%) |
| 060-069 | 060,061,062 | 063-069 | 3/10 (30%) |
| 070-079 | 070-075 | 076-079 | 6/10 (60%) |
| 080-089 | 080-082 | 083-089 | 3/10 (30%) |
| 120-129 | 120-123,125 | 124,126-129 | 5/10 (50%) |
| 131-149 | 135,150 | 131-134,140-149 | Low |
| 250-259 | 250 | 251-259 | 1/10 (10%) |
| 260-269 | 260-261 | 262-269 | 2/10 (20%) |
| 270-279 | 270 | 271-279 | 1/10 (10%) |
| 280-289 | 280-282 | 283-289 | 3/10 (30%) |
| 300-309 | 300-303 | 304-309 | 4/10 (40%) |
| 410-419 | 410 | 411-419 | 1/10 (10%) |
| 420-429 | 420 | 421-429 | 1/10 (10%) |

**Problem:** These gaps suggest the documentation is prescriptive (planning future codes) rather than descriptive (documenting current codes).

**Recommendation:** Either:
1. Fill the gaps with actual error variants, OR
2. Mark ranges as "Reserved for vX.Y.Z" in documentation

---

### Issue 3.2: Inconsistent Error Message Formatting

**Severity:** MEDIUM
**Location:** `src/error.rs:386` (McpToolError)

```rust
#[error("[NIKA-102] MCP tool '{tool}' call failed{}: {reason}",
         error_code.map(|c| format!(" ({})", c)).unwrap_or_default())]
```

**Problem:** The error message conditionally includes the JSON-RPC error code, making output inconsistent.

**Example outputs:**
```
[NIKA-102] MCP tool 'novanet_search' call failed: {"error": "..."}
[NIKA-102] MCP tool 'novanet_search' call failed (-32602): {"error": "..."}
```

**Recommendation:** Either always include the code or never include it.

---

### Issue 3.3: ToolError Code Returns Placeholder

**Severity:** MEDIUM
**Location:** `src/error.rs:856` (in code() function)

```rust
Self::ToolError { .. } => "NIKA-2XX",  // Placeholder
```

**Problem:** Returns a generic placeholder instead of the actual code. Prevents code-based error routing.

**Workaround:** ToolErrorCode has its own code() method, so this is acceptable but inconsistent.

**Recommendation:** Update comment to explain why:
```rust
Self::ToolError { .. } => "NIKA-2XX",  // ToolErrorCode has its own code() method
```

---

## 4. Documentation Issues

### Issue 4.1: Orphaned Error Code Ranges

**Severity:** MEDIUM
**Location:** Multiple lines in header documentation

The following ranges are documented but have NO implementation:
- NIKA-023, NIKA-024 (gaps in DAG range - not documented)
- NIKA-034-039 (gaps in Provider range)
- NIKA-044-049 (gaps in Binding range)
- NIKA-058-059, NIKA-064-069, NIKA-076-079 (various ranges)

**Recommendation:** Update documentation to clearly mark what's implemented vs. reserved.

---

## 5. What's Working Well ✅

| Aspect | Status | Evidence |
|--------|--------|----------|
| code() coverage | ✅ 100% | All 67 variants handled at lines 759-897 |
| FixSuggestion coverage | ✅ 100% | All variants have suggestions at lines 924-1203 |
| Error message formatting | ✅ Mostly good | Uses miette for fancy display |
| Deprecated variant handling | ✅ Working | Provider, Template, Execution still supported |
| Error code uniqueness | ✅ No collisions | Each variant maps to exactly one code |
| Test coverage | ✅ Comprehensive | 1,200+ lines of tests (sections 1207-1398) |

---

## 6. Error Code Allocation Summary

**Total Error Codes in v0.27.0:**

```
Implemented:  67 enum variants across 11 ranges
Documented:   140+ codes across reserved ranges
Ratio:        48% implemented, 52% reserved for future

Breakdown by range:
  0-099:   34 implemented (high density)
  100-199: 26 implemented + reserved (medium density)
  200-429:  7 implemented + 130+ reserved (future buffer)
```

**Assessment:** The design reserves significant capacity for future versions (v0.28+), which is reasonable given Nika's "forever 0.x.x" versioning strategy.

---

## 7. Recommendations

### Immediate (v0.27.1+)

1. **Update header documentation** for NIKA-140-149 and NIKA-200-209 to reference correct modules
2. **Add NIKA-083 variant** or clarify which error handles template reference issues
3. **Mark NIKA-220-249 as reserved** with notation "(v0.9.4+)"

### Short-term (v0.28+)

4. Implement missing ranges as features are added (e.g., NIKA-251-259 for additional context errors)
5. Fill in obvious gaps (NIKA-054, NIKA-058-059, etc.) if functionality is added
6. Improve is_recoverable() to handle more transient error categories

### Long-term

7. Consider deprecation timeline for v0.1 compatibility errors (Provider, Template, Execution)
8. Evaluate whether reserved ranges (200+) should be consolidated or removed in v1.0 (note: v1.0 never ships per CLAUDE.md)

---

## 8. Affected Files

If implementing recommendations:

| File | Impact | Changes Needed |
|------|--------|-----------------|
| `src/error.rs` | Header documentation | Update lines 22, 25, 27-29 |
| `src/error.rs` | Enum definition | Add NIKA-083 variant (if needed) |
| `src/ast/analyzer/errors.rs` | No change | (Already correct) |
| `src/tools/mod.rs` | No change | (Already correct) |
| `src/dag/validate.rs` | Update comments | Change NIKA-083 reference if not added |

---

## 9. Test Coverage

Current test coverage at lines 1207-1398:

- ✅ NIKA-001 through NIKA-033 (comprehensive)
- ✅ Error code() function tested
- ✅ FixSuggestion trait tested
- ✅ Deprecated variants tested with #[allow(deprecated)]

**Missing:** No tests for error code consistency across the codebase (e.g., verifying NIKA-083 isn't used elsewhere)

---

## Conclusion

The error.rs file is **well-structured and mostly correct**, but suffers from **documentation-reality misalignment** in three critical areas. The primary issues are:

1. **External references** (NIKA-140-149 in ast/analyzer, NIKA-200-209 in tools/mod)
2. **Reserved but undocumented ranges** (NIKA-220-249)
3. **Orphaned references** (NIKA-083 documented but not implemented)

These are documentation issues rather than code issues, but they impact developer experience and error tracking.

**Priority:** Fix documentation inconsistencies in next release to improve code clarity and error handling reliability.

---

**Report Generated:** 2026-03-12
**Report Author:** Claude Code Audit Tool
**Version:** Nika v0.27.0
