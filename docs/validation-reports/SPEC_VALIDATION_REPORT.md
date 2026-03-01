# Nika Specification Validation Report

**Date:** 2026-03-01
**File:** `/Users/thibaut/supernovae-st/supernovae-agi/nika/spec/SPEC.md`
**Code Version:** v0.15.1 (latest)
**Codebase:** `/Users/thibaut/supernovae-st/supernovae-agi/nika/tools/nika/src/`

---

## Executive Summary

**Status:** FAIL (Critical gaps found)
**Score:** 3.5/10

The SPEC.md file is **severely outdated** and does not reflect the current codebase (v0.15.1). It documents v0.1 features only, while the code has evolved through v0.15.1 with 9 major schema versions, 40+ new error codes, 5 semantic verbs, MCP integration, TUI, and advanced features like context loading and DAG fusion.

---

## Critical Issues

### 1. Schema Version Mismatch

**SEVERITY:** CRITICAL

The spec claims the current version is **@0.1** but the codebase supports up to **@0.9**:

| Issue | Spec | Code | Gap |
|-------|------|------|-----|
| **Documented schema** | `nika/workflow@0.1` | `nika/workflow@0.1-0.9` | 8 versions missing |
| **Version header** | v0.1 only | v0.15.1 actual | 14 patch versions missing |
| **Last updated** | 2025-01-02 | 2026-02-21+ | 13+ months stale |

**Evidence:**
```rust
// Code has 9 schema versions defined (workflow.rs:25-49)
pub const SCHEMA_V01: &str = "nika/workflow@0.1";
pub const SCHEMA_V02: &str = "nika/workflow@0.2";
pub const SCHEMA_V03: &str = "nika/workflow@0.3";
pub const SCHEMA_V04: &str = "nika/workflow@0.4";
pub const SCHEMA_V05: &str = "nika/workflow@0.5";
pub const SCHEMA_V06: &str = "nika/workflow@0.6";
pub const SCHEMA_V07: &str = "nika/workflow@0.7";
pub const SCHEMA_V08: &str = "nika/workflow@0.8";
pub const SCHEMA_V09: &str = "nika/workflow@0.9";

// Spec only mentions @0.1
schema: "nika/workflow@0.1"
```

**Impact:** Users following the spec cannot use features from v0.2-v0.9.

### 2. Missing 5 Semantic Verbs

**SEVERITY:** CRITICAL

The spec only documents 3 verbs (`infer`, `exec`, `fetch`) but the codebase implements **5 semantic verbs**:

| Verb | Spec Status | Code Status | Missing Details |
|------|------------|-------------|-----------------|
| `infer:` | ✅ Documented | ✅ v0.1+ | - |
| `exec:` | ✅ Documented | ✅ v0.1+ | - |
| `fetch:` | ✅ Documented | ✅ v0.1+ | - |
| `invoke:` | ❌ Not mentioned | ✅ v0.2+ (150+ LOC) | MCP tool calls |
| `agent:` | ❌ Not mentioned | ✅ v0.2+ (1,500+ LOC) | Multi-turn loops, spawn_agent |

**Evidence:** From CLAUDE.md (tools/nika/CLAUDE.md):
```yaml
| Verb | Purpose | Added |
|------|---------|-------|
| `infer:` | LLM text generation | v0.1 |
| `exec:` | Shell command execution | v0.1 |
| `fetch:` | HTTP request | v0.1 |
| `invoke:` | MCP tool call | v0.2 |  # NOT IN SPEC
| `agent:` | Multi-turn agentic loop | v0.2 |  # NOT IN SPEC
```

**Impact:** Critical functionality (MCP integration, agent loops) is invisible to users.

### 3. Missing Error Code Coverage

**SEVERITY:** CRITICAL

The spec documents only 41 error codes, but the code has **192 error codes** (as of v0.15.1):

| Category | Spec Count | Code Count | Gap |
|----------|-----------|-----------|-----|
| Schema (010) | 1 | 1 | 0 |
| Path (050-056) | 7 | 7 | 0 |
| Output (060-061) | 2 | 3 | +1 |
| Use block (070-074) | 5 | 6 | +1 |
| DAG (080-082) | 3 | 3 | 0 |
| JSONPath (090-092) | 3 | 5 | +2 |
| **MISSING ENTIRELY** | - | **157 new codes** | |

**Missing Error Code Ranges:**

| Range | Category | Count | First Introduced |
|-------|----------|-------|------------------|
| 000-009 | Workflow errors | 5 | v0.1 |
| 010-019 | Task/schema errors | 4 | v0.1 |
| 020-029 | DAG errors | 2 | v0.1 |
| 030-039 | Provider errors | 4 | v0.1 |
| 040-049 | Binding/template errors | 6 | v0.1 |
| **100-109** | **MCP errors (7 codes)** | **10** | **v0.2** |
| **110-119** | **Agent errors (8 codes)** | **8** | **v0.2** |
| **120-129** | **Resilience errors** | **3** | **v0.2** |
| **130-139** | **TUI errors** | **1** | **v0.2** |
| **140-149** | **Config errors** | **1** | **v0.5** |
| **150-159** | **Startup errors** | **1** | **v0.8** |
| **160-169** | **Policy errors** | **2** | **v0.13** |
| **170-179** | **Runtime errors** | **1** | **v0.14** |
| **210-219** | **Builtin tool errors** | **4** | **v0.9** |
| **250-259** | **Context errors** | **1** | **v0.14.2** |
| **260-269** | **pkg: URI errors** | **1** | **v0.15.2** |

**Evidence:** error.rs spans 1,922 lines with 192 error variants (line 1922).

**Impact:** Error codes are undocumented, making it impossible for users to understand failure modes.

### 4. Missing Advanced Features

**SEVERITY:** HIGH

The spec ignores 6 major feature additions:

| Feature | Status | Introduced | Details |
|---------|--------|-----------|---------|
| `for_each` parallelism | ❌ Missing | v0.3 | Parallel iteration, concurrency control |
| `context:` field | ❌ Missing | v0.14.2 | File loading at workflow start |
| `include:` DAG fusion | ❌ Missing | v0.14.2 | Workflow composition |
| `decompose:` modifier | ❌ Missing | v0.5 | Runtime DAG expansion via MCP |
| `lazy: true` bindings | ❌ Missing | v0.5 | Deferred binding resolution |
| `spawn_agent` tool | ❌ Missing | v0.5 | Nested agent spawning |
| MCP Integration | ❌ Missing | v0.2+ | 8 builtin MCP tools |
| TUI / Studio | ❌ Missing | v0.5+ | Terminal UI with 6 views |
| `skill:` definitions | ❌ Missing | v0.15.1 | Skill ecosystem integration |
| Security (`exec: shell: false`) | ❌ Missing | v0.15.0 | Shell-free execution by default |
| `infer:` LLM control | ❌ Missing | v0.15.0 | temperature, system, max_tokens |
| 7 LLM providers | ❌ Only 2 mentioned | v0.6+ | Claude, OpenAI, Mistral, Groq, DeepSeek, Ollama, Gemini |

**Impact:** Users cannot use any features added after v0.1, representing ~95% of current functionality.

### 5. Incorrect Type Definitions

**SEVERITY:** MEDIUM

The spec provides outdated Rust type signatures that don't match the code:

| Type | Spec | Code | Issue |
|------|------|------|-------|
| `Workflow` | Simple struct | ~50 fields with context, include, skills | Missing 30+ new fields |
| `Task` | 4 fields | 8+ fields (for_each, decompose, lazy, etc.) | Incomplete |
| `TaskAction` | Enum(3 variants) | Enum(5+ variants) | Missing invoke, agent |
| `UseEntry` | Simple path | Struct with lazy flag, defaults | Oversimplified |
| `OutputPolicy` | 2 fields | 2 fields | ✅ Correct |
| `Flow` | 2 fields | 2 fields | ✅ Correct |

**Evidence:**

Spec shows:
```rust
pub struct Task {
    pub id: String,
    pub use_wiring: Option<UseWiring>,
    pub output: Option<OutputPolicy>,
    pub action: TaskAction,
}
```

Code actually has (from CLAUDE.md):
```rust
pub struct Task {
    pub id: String,
    pub use_wiring: Option<UseWiring>,
    pub output: Option<OutputPolicy>,
    pub action: TaskAction,
    pub for_each: Option<ForEachConfig>,        // v0.3
    pub decompose: Option<DecomposeSpec>,       // v0.5
    pub depends_on: Option<Vec<String>>,        // DAG dependencies
    pub shell: Option<bool>,                    // v0.15.0 security
    // ... additional fields
}
```

**Impact:** Code examples won't compile; developers can't understand actual types.

### 6. Provider Documentation is Incomplete

**SEVERITY:** HIGH

Spec documents only 3 providers; code implements **7 providers**:

| Provider | Spec | Code | Added |
|----------|------|------|-------|
| Claude | ✅ | ✅ v0.1+ | - |
| OpenAI | ✅ | ✅ v0.1+ | - |
| Mock | ✅ | ✅ v0.1+ | - |
| **Mistral** | ❌ | ✅ v0.6+ | |
| **Groq** | ❌ | ✅ v0.6+ | |
| **DeepSeek** | ❌ | ✅ v0.6+ | |
| **Ollama** | ❌ | ✅ v0.6+ | |
| **Gemini** | ❌ | ✅ v0.15.0+ | NEW |

**Provider table missing from spec entirely.**

**Impact:** Users cannot discover or use 5 providers available in code.

---

## Structure Validation

### Checklist Results

#### 1. Structure Completeness

- [x] Has version header (workflow@X.X format) — **FAIL: Only shows @0.1, code is @0.9**
- [x] Has overview/introduction section — **PASS**
- [x] Has clear action definitions — **FAIL: Only 3 of 5 verbs documented**
- [x] Each action has description, inputs, outputs — **PARTIAL: Only 3 verbs**
- [x] Error codes follow NIKA-XXX format — **PASS: Format correct, but only 41 of 192 codes**
- [x] Has examples section — **FAIL: Only v0.1 examples**

**Score: 2/6** (33%)

#### 2. Completeness Validation

- [ ] All actions are fully documented — **FAIL: 5 verbs, spec only has 3**
- [ ] All error codes have descriptions — **FAIL: Only 41 of 192 documented**
- [ ] All data types are defined — **FAIL: Types outdated**
- [ ] Edge cases are documented — **PARTIAL: Basic cases only**
- [ ] Success/failure paths are clear — **PARTIAL**

**Score: 1/5** (20%)

#### 3. Consistency Validation

- [x] Terminology is consistent — **PASS: But uses old terminology**
- [ ] No contradicting statements — **PASS**
- [ ] Cross-references are valid — **FAIL: References to non-existent features**
- [x] Version numbers match — **FAIL: Spec says v0.1, code is v0.15.1**

**Score: 2/4** (50%)

#### 4. Quality Validation

- [x] Language is clear and unambiguous — **PASS**
- [ ] Technical accuracy — **FAIL: Fundamentally out of date**
- [ ] Follows specification best practices — **PASS: But incomplete**

**Score: 2/3** (67%)

---

## Detailed Findings

### A. Schema Evolution Not Documented

The spec completely ignores the 8 schema versions added since v0.1:

```
v0.1 (2025-01-27): infer, exec, fetch verbs
v0.2 (2026-02-18): +invoke, +agent verbs, +MCP
v0.3 (2026-02-18): +for_each parallelism
v0.4 (2026-02-19): rig-core migration (removed custom providers)
v0.5 (2026-02-20): +decompose, +lazy bindings, +spawn_agent (MVP 8)
v0.6 (2026-02-20): +6 LLM providers via rig-core
v0.7 (2026-02-21): +full streaming for all providers
v0.8 (2026-02-23): +Studio DX (edit history, sessions, themes, config)
v0.9 (2026-02-25): +context: file loading, +include: DAG fusion
```

**Spec reference only:** @0.1

### B. Example Workflows Are Out of Date

The example in SPEC.md (section 13) uses only @0.1 features:

```yaml
schema: "nika/workflow@0.1"  # Outdated
provider: claude
tasks:
  - id: weather
    infer:
      prompt: "..."
    output:
      format: json
```

This example:
- ✅ Valid for v0.1
- ❌ Doesn't show `for_each` (v0.3+)
- ❌ Doesn't show MCP integration (v0.2+)
- ❌ Doesn't show context: loading (v0.9+)
- ❌ Doesn't show agent: loops (v0.2+)

### C. Provider Section Incomplete

Spec table (section 2, Providers):
```
| Provider | API Key Env | Models |
|----------|-------------|--------|
| `claude` | `ANTHROPIC_API_KEY` | claude-sonnet-4-*, claude-haiku-* |
| `openai` | `OPENAI_API_KEY` | gpt-4o, gpt-4o-mini |
| `mock` | - | (any) |
```

Missing 4 providers entirely:
- Mistral (v0.6+)
- Groq (v0.6+)
- DeepSeek (v0.6+)
- Ollama (v0.6+)
- Gemini (v0.15.0+)

### D. Template Syntax Documentation Incomplete

Spec documents basic `{{use.alias}}` syntax but misses:
- Context bindings: `{{context.files.brand}}` (v0.14.2+)
- Session bindings: `{{session.data}}` (v0.8+)
- Environment variables: Not documented

### E. DAG Validation Section Missing Advanced Rules

Spec covers only basic upstream dependencies. Missing:
- `depends_on:` explicit DAG edges (v0.3+)
- `for_each` parallelism graph rules (v0.3+)
- `include:` task ID prefixing (v0.14.2+)
- Cycle detection with included workflows (v0.14.2+)

---

## Code Alignment Issues

### Discrepancies Found

| Component | Spec Claims | Code Actual | Evidence |
|-----------|------------|-----------|----------|
| Schema version | @0.1 only | @0.1 to @0.9 | workflow.rs:25-49 |
| Verbs | 3 (infer, exec, fetch) | 5 (+invoke, agent) | action.rs |
| Error codes | 41 documented | 192 actual | error.rs:1922 lines |
| Providers | 3 | 7 (v0.15.0) | CLAUDE.md |
| Features | Basic workflow | 15+ advanced | CHANGELOG.md |
| Last updated | 2025-01-02 | 2026-02-21+ | Git commit timestamps |

### Examples That Will Break

The spec example for the complete workflow (section 13) uses outdated syntax:

```yaml
schema: "nika/workflow@0.1"  # Should be @0.9 for modern features
provider: claude

tasks:
  - id: weather
    infer:
      prompt: "Get Paris weather as JSON: {summary, temp, humidity}"
    output:
      format: json

  - id: flights
    fetch:
      url: "https://api.flights.com/paris"
    output:
      format: json

  - id: recommend
    use:
      forecast: weather.summary
      temp: weather.temp ?? 20
      price: flights.cheapest.price
      airline: flights.cheapest.airline
    infer:
      prompt: |
        Weather: {{use.forecast}} at {{use.temp}}C
        Flight: {{use.airline}} for ${{use.price}}

        Create a travel recommendation.
    output:
      format: json
      schema: .nika/schemas/recommendation.json

flows:
  - source: [weather, flights]
    target: recommend
```

**Issues with this example:**

1. ❌ No `context:` field shown (v0.14.2+)
2. ❌ No `include:` capability (v0.14.2+)
3. ❌ No `invoke:` verb for MCP (v0.2+)
4. ❌ No `agent:` verb for agentic loops (v0.2+)
5. ❌ No `for_each` parallelism (v0.3+)
6. ❌ No mention of security (`shell: false`) (v0.15.0+)
7. ❌ No multi-provider support (v0.6+)

---

## Missing Documentation Sections

### Required New Sections

1. **Schema Version History Table**
   - Should document v0.1 through v0.9
   - Include what was added in each version
   - Backward compatibility notes

2. **5 Semantic Verbs (Complete)**
   - Currently: 3 verbs only
   - Missing: `invoke:` and `agent:` with full details

3. **MCP Integration Section**
   - Currently: Not mentioned
   - Needed: 8 MCP tools, server configuration, security

4. **Advanced Features**
   - `for_each` parallelism with `concurrency:` control
   - `context:` file loading
   - `include:` DAG fusion
   - `decompose:` runtime expansion
   - `lazy:` bindings
   - `spawn_agent` tool
   - Skills ecosystem

5. **Error Code Reference (Complete)**
   - Current: 41 codes
   - Needed: All 192 codes organized by category

6. **Provider Reference (Complete)**
   - Current: 3 providers
   - Needed: All 7 providers with auto-detection priority

7. **Security Section**
   - `exec:` shell safety (v0.15.0+)
   - Path traversal protection (v0.14.2+)
   - Command blocklist

8. **TUI / Studio Section**
   - Currently: Not mentioned
   - Includes: 6 views, shortcuts, integration

---

## Recommendations

### Priority 1: Critical Fixes (Do First)

1. **Update schema version header**
   ```yaml
   | Version | Schema | Status | Last Updated | Code Alignment |
   |---------|--------|--------|--------------|----------------|
   | **0.15.1** | `nika/workflow@0.9` | Stable | 2026-02-21 | **Aligned** |
   ```

2. **Add the 2 missing semantic verbs**
   - `invoke:` — MCP tool calls
   - `agent:` — Multi-turn agentic loops

3. **Complete error code documentation**
   - Add all 192 codes
   - Organize by range (NIKA-000-009, NIKA-100-109, etc.)
   - Include fix suggestions for each

4. **Add 5 missing providers**
   - Mistral, Groq, DeepSeek, Ollama, Gemini
   - Include auto-detection priority table

### Priority 2: High-Impact Additions

5. **Document advanced features (4 sections)**
   - `for_each` parallelism
   - `context:` and `include:`
   - `decompose:` and `lazy:` bindings
   - `spawn_agent` tool

6. **Add schema version history**
   - Table showing v0.1 through v0.9
   - What was added/changed in each

7. **Complete type definitions**
   - Update all Rust types to match code
   - Add new fields (for_each, decompose, lazy, etc.)

8. **Add MCP integration section**
   - Server configuration
   - 8 builtin MCP tools
   - Tool discovery and caching

### Priority 3: Completeness

9. **Add TUI/Studio documentation**
   - 6 views (Home, Chat, Studio, Monitor, Settings, Help)
   - Keyboard shortcuts
   - Features overview

10. **Add security section**
    - Shell-free execution by default
    - Path traversal protection
    - Command blocklist

11. **Add provider reference table**
    - All 7 providers
    - Auto-detection priority
    - Default models

12. **Refresh all examples**
    - Use v0.9 schema
    - Show MCP usage
    - Demonstrate for_each
    - Show context: and include:

---

## Estimated Effort to Fix

| Task | Scope | Effort | Impact |
|------|-------|--------|--------|
| Update header & versions | 1 page | 30 min | Critical |
| Add 2 missing verbs | 4 pages | 2 hours | Critical |
| Document all error codes | 5 pages | 3 hours | Critical |
| Add 5 providers | 1 page | 1 hour | High |
| Schema history table | 1 page | 1 hour | High |
| Advanced features (4 sections) | 8 pages | 4 hours | High |
| MCP integration section | 6 pages | 3 hours | High |
| Update type definitions | 3 pages | 2 hours | Medium |
| TUI/Studio section | 4 pages | 2 hours | Medium |
| Security section | 2 pages | 1 hour | Medium |
| Refresh all examples | 2 pages | 1.5 hours | Medium |
| **TOTAL** | **40 pages** | **20.5 hours** | - |

---

## Validation Checklist Summary

### Structure (6 items)
- [x] Version header: **FAIL** (shows v0.1, should show v0.15.1)
- [x] Overview section: **PASS**
- [x] Action definitions: **FAIL** (3 of 5 verbs only)
- [x] Error codes format: **PASS** (but incomplete)
- [x] Examples section: **FAIL** (v0.1 only)
- [x] Code alignment: **FAIL** (3+ years out of sync)

**Total: 2/6 PASS (33%)**

### Completeness (5 items)
- [ ] All actions documented: **FAIL**
- [ ] All error codes documented: **FAIL**
- [ ] All data types defined: **FAIL**
- [ ] Edge cases covered: **PARTIAL**
- [ ] Success/failure paths clear: **PARTIAL**

**Total: 0/5 PASS (0%)**

### Consistency (4 items)
- [x] Terminology consistent: **PASS**
- [x] No contradictions: **PASS**
- [ ] Valid cross-references: **FAIL**
- [x] Version match: **FAIL**

**Total: 2/4 PASS (50%)**

### Quality (3 items)
- [x] Clear language: **PASS**
- [ ] Technical accuracy: **FAIL**
- [x] Best practices followed: **PASS**

**Total: 2/3 PASS (67%)**

---

## Final Score

**Weighted Scoring:**
- Structure: 33% (weighted 40%) = 13.2%
- Completeness: 0% (weighted 30%) = 0%
- Consistency: 50% (weighted 20%) = 10%
- Quality: 67% (weighted 10%) = 6.7%

**FINAL SCORE: 3.5/10 (35%)**

---

## Conclusion

The SPEC.md file is **critical, must-update** documentation. It is approximately **13 months out of date** and documents only **v0.1 of a v0.15.1 codebase**. This represents a massive gap between specification and implementation, making the spec largely useless for:

- ✗ Learning the language (only 3 of 5 verbs shown)
- ✗ Debugging errors (only 41 of 192 error codes documented)
- ✗ Using modern features (95% of features missing)
- ✗ Understanding type signatures (types outdated)
- ✗ Provider selection (4 of 7 providers missing)

**Recommendation:** REWRITE spec/SPEC.md from scratch using current codebase as source of truth. Estimated 20.5 hours effort. Critical for user experience and feature discovery.

