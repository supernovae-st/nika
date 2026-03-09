# Nika Bug Fixes — TDD Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Date:** 2026-03-09
**Scope:** Fix all 7 documented bugs from BUG-REPORT-2026-03-09.md
**Strategy:** TDD (Red-Green-Refactor) + Ralph Wiggum verification

---

## Executive Summary

| Bug | Severity | Fix Type | Estimated Effort |
|-----|----------|----------|------------------|
| #8  | CRITICAL | Wire existing code | 1 task |
| #2  | HIGH | Template string fix | 1 task |
| #3  | HIGH | Template string fix | 1 task |
| #5  | HIGH | Template string fix | 1 task |
| #1/#4 | MEDIUM | Runtime code change | 2 tasks |

**Total:** 6 tasks, ~30 test cases

---

## Bug #8: StructuredOutputEngine NOT Wired (CRITICAL)

### Root Cause Analysis

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  CURRENT STATE (BUG)                                                            │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  executor.rs:run_infer()                                                        │
│  ├── build_json_schema_instruction()  ← Text injection only!                    │
│  ├── provider.infer_stream()          ← No schema validation                    │
│  └── return stream_result.text        ← Raw output, no retry                    │
│                                                                                 │
│  PROBLEM:                                                                       │
│  - StructuredOutputEngine EXISTS (856 lines, 30+ tests)                         │
│  - BUT executor.rs does NOT import or use it                                    │
│  - Schema injection is just text, no validation/retry/repair                    │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────────────┐
│  REQUIRED STATE (FIX)                                                           │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  executor.rs:run_infer()                                                        │
│  ├── if output_policy.is_structured() {                                         │
│  │     let engine = StructuredOutputEngine::new(...);                           │
│  │     return engine.extract(prompt, schema).await;                             │
│  │   }                                                                          │
│  └── else { existing flow... }                                                  │
│                                                                                 │
│  4-LAYER DEFENSE:                                                               │
│  Layer 2: Provider-Native (schema injection) → ~95%                             │
│  Layer 3: Retry with Feedback → ~99%                                            │
│  Layer 4: LLM Repair → ~99.99%                                                  │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Task 8.1: Wire StructuredOutputEngine in executor.rs

**Files:**
- Modify: `tools/nika/src/runtime/executor.rs`
- Test: `tests/structured_output_wiring_test.rs`

#### Step 1: Write failing integration test (RED)

Create `tests/structured_output_wiring_test.rs`:

```rust
//! Tests that StructuredOutputEngine is properly wired in executor

use nika::ast::{OutputFormat, OutputPolicy, SchemaRef, TaskAction, InferParams};
use nika::runtime::TaskExecutor;
use nika::event::EventLog;
use serde_json::json;
use std::sync::Arc;

/// Test that infer task with output.schema actually validates the output
#[tokio::test]
async fn test_infer_with_schema_validates_output() {
    // This test should FAIL initially because StructuredOutputEngine is not wired

    let event_log = EventLog::new();
    let executor = TaskExecutor::new("mock", None, None, event_log.clone());

    let schema = json!({
        "type": "object",
        "properties": {
            "name": { "type": "string" },
            "age": { "type": "integer", "minimum": 0 }
        },
        "required": ["name", "age"]
    });

    let output_policy = OutputPolicy {
        format: OutputFormat::Json,
        schema: Some(SchemaRef::Inline(schema)),
        structured: Some(true),  // Enable structured output
        ..Default::default()
    };

    let infer = InferParams {
        prompt: "Generate a person with name Alice and age 30".to_string(),
        ..Default::default()
    };

    // Execute infer with schema
    let result = executor.run_infer_with_policy(
        &Arc::from("test_task"),
        &infer,
        &Default::default(),
        &Default::default(),
        Some(&output_policy),
    ).await;

    // Should succeed with valid JSON
    assert!(result.is_ok(), "Infer with schema should succeed");

    // Check events for StructuredOutputSuccess
    let events = event_log.all();
    let has_structured_event = events.iter().any(|e| {
        matches!(&e.kind, nika::event::EventKind::StructuredOutputSuccess { .. })
    });
    assert!(has_structured_event, "Should emit StructuredOutputSuccess event");
}

/// Test that invalid schema output triggers retry
#[tokio::test]
async fn test_infer_with_schema_retries_on_invalid() {
    // This requires a provider that returns invalid JSON first, then valid
    // For now, verify the retry event is emitted
    // TODO: Implement with mock provider that returns invalid first
}
```

#### Step 2: Run test to verify it fails

```bash
cd tools/nika && cargo test structured_output_wiring -- --nocapture
```

Expected: FAIL (StructuredOutputSuccess event not found)

#### Step 3: Implement the fix (GREEN)

Modify `src/runtime/executor.rs`:

**Add import at top:**

```rust
use crate::runtime::structured_output::{StructuredOutputEngine, StructuredOutputResult};
use crate::ast::structured::StructuredMode;
```

**Modify `run_infer()` — insert BEFORE the mock provider check (around line 663):**

```rust
    async fn run_infer(
        &self,
        task_id: &Arc<str>,
        infer: &InferParams,
        bindings: &ResolvedBindings,
        datastore: &DataStore,
        output_policy: Option<&OutputPolicy>,
    ) -> Result<String, NikaError> {
        // ... existing validation code (lines 608-630) ...

        // v0.21.1: Check if structured output is enabled (BEFORE mock provider)
        if let Some(policy) = output_policy {
            if policy.is_structured() {
                return self.run_infer_structured(
                    task_id,
                    infer,
                    bindings,
                    datastore,
                    policy,
                ).await;
            }
        }

        // ... rest of existing code ...
    }

    /// Execute infer with StructuredOutputEngine (v0.21.1)
    async fn run_infer_structured(
        &self,
        task_id: &Arc<str>,
        infer: &InferParams,
        bindings: &ResolvedBindings,
        datastore: &DataStore,
        policy: &OutputPolicy,
    ) -> Result<String, NikaError> {
        // Resolve prompt template
        let prompt = template_resolve(&infer.prompt, bindings, datastore)?.into_owned();

        if prompt.trim().is_empty() {
            return Err(NikaError::ValidationError {
                reason: format!(
                    "Resolved prompt is empty (task: {}). Check your template bindings.",
                    task_id
                ),
            });
        }

        // Get schema
        let schema = match &policy.schema {
            Some(SchemaRef::Inline(v)) => v.clone(),
            Some(SchemaRef::File(path)) => {
                let content = tokio::fs::read_to_string(path).await
                    .map_err(|e| NikaError::SchemaLoadFailed {
                        path: path.clone(),
                        reason: e.to_string(),
                    })?;
                serde_json::from_str(&content)
                    .map_err(|e| NikaError::SchemaLoadFailed {
                        path: path.clone(),
                        reason: format!("Invalid JSON: {}", e),
                    })?
            }
            None => {
                return Err(NikaError::ValidationError {
                    reason: "structured: true requires output.schema".to_string(),
                });
            }
        };

        // Get provider
        let provider_name = infer.provider.as_deref().unwrap_or(&self.default_provider);
        let provider = self.get_rig_provider(provider_name)?;

        // Get config from policy
        let config = policy.structured_config().unwrap_or_default();

        // Create and run engine
        let engine = StructuredOutputEngine::new(provider, task_id.as_ref())
            .with_config(config)
            .with_event_log(self.event_log.clone());

        let result = engine.extract(&prompt, &schema).await?;

        // Return validated JSON as string
        Ok(serde_json::to_string(&result.value).unwrap_or_default())
    }
```

#### Step 4: Run test to verify it passes

```bash
cd tools/nika && cargo test structured_output_wiring -- --nocapture
```

Expected: PASS

#### Step 5: Commit

```bash
git add tools/nika/src/runtime/executor.rs tests/structured_output_wiring_test.rs
git commit -m "fix(executor): wire StructuredOutputEngine for infer tasks (BUG #8)

- Add run_infer_structured() method for output.structured: true
- Check policy.is_structured() before normal infer flow
- Load schema from inline or file
- Create engine with config from policy
- Emit StructuredOutputSuccess/Attempt events
- Integration test for wiring verification

Fixes: BUG #8 (output.schema silently ignored)

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## Bugs #2, #3, #5: Template String Fixes (HIGH)

These are simple string replacements in `src/init/tier*.rs`.

### Task T.1: Fix BUG #2 — Add shell: true to exec templates

**Files:**
- Modify: `tools/nika/src/init/tier1.rs`
- Modify: `tools/nika/src/init/tier2.rs`
- Modify: `tools/nika/src/init/tier3.rs`
- Modify: Any tier*.rs using exec with shell operators

#### Step 1: Find affected templates

```bash
cd tools/nika && grep -r "exec:" src/init/ | grep -E "\||&&|>" | head -20
```

#### Step 2: Fix each template

Change:
```yaml
exec:
  command: "date > output.txt && echo Done"
```

To:
```yaml
exec:
  command: "date > output.txt && echo Done"
  shell: true
```

#### Step 3: Commit

```bash
git add tools/nika/src/init/
git commit -m "fix(init): add shell: true to templates with shell operators (BUG #2)

Templates using |, &&, >, or other shell operators now explicitly
set shell: true as required by v0.15.0 security hardening.

Affected templates:
- tier1: [list affected]
- tier2: [list affected]
- tier3: [list affected]

Fixes: BUG #2

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

### Task T.2: Fix BUG #3 — Correct MCP tool name

**Files:**
- Modify: `tools/nika/src/init/tier6.rs` (WORKFLOW_21_MORNING_BRIEFING)

#### Step 1: Find the wrong tool name

```bash
cd tools/nika && grep -r "perplexity_search_web" src/init/
```

#### Step 2: Replace

Change: `perplexity_search_web` → `perplexity_search`

#### Step 3: Commit

```bash
git add tools/nika/src/init/tier6.rs
git commit -m "fix(init): correct MCP tool name in morning-briefing (BUG #3)

perplexity_search_web → perplexity_search

The MCP tool is named 'perplexity_search' in the Perplexity MCP server.

Fixes: BUG #3

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

### Task T.3: Fix BUG #5 — Correct binding syntax

**Files:**
- Modify: `tools/nika/src/init/tier5.rs` (multiple templates)

#### Step 1: Find wrong syntax

```bash
cd tools/nika && grep -r "for_each: \"\$[^i]" src/init/
# or
cd tools/nika && grep -r 'for_each:.*\$[a-z]' src/init/ | grep -v '\$inputs'
```

#### Step 2: Replace all occurrences

Change: `for_each: "$targets"` → `for_each: "$inputs.targets"`

Affected templates:
- WORKFLOW_12_CONTENT_LOCALIZATION
- WORKFLOW_17_PR_REVIEW_BOT
- WORKFLOW_23_COMPETITOR_ANALYSIS
- WORKFLOW_30_NEWSLETTER_CURATOR

#### Step 3: Commit

```bash
git add tools/nika/src/init/tier5.rs
git commit -m "fix(init): use \$inputs.targets binding syntax (BUG #5)

for_each: \"\$targets\" causes deadlock because 'targets' is not resolved.
The correct syntax is \"\$inputs.targets\" to reference workflow inputs.

Affected templates:
- WORKFLOW_12_CONTENT_LOCALIZATION
- WORKFLOW_17_PR_REVIEW_BOT
- WORKFLOW_23_COMPETITOR_ANALYSIS
- WORKFLOW_30_NEWSLETTER_CURATOR

Fixes: BUG #5

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## Bugs #1/#4: HYBRID Path Validation (MEDIUM)

### Design: HYBRID Approach

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  HYBRID PATH VALIDATION                                                         │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  RULE 1: Accept relative paths (resolve against workdir)                        │
│  ./test.txt → /Users/x/project/test.txt                                         │
│  output/result.json → /Users/x/project/output/result.json                       │
│                                                                                 │
│  RULE 2: Accept /tmp/ as allowlisted prefix                                     │
│  /tmp/nika-test/file.txt → allowed                                              │
│  /tmp/anything/deep/path.txt → allowed                                          │
│                                                                                 │
│  RULE 3: Reject other absolute paths outside workdir                            │
│  /etc/passwd → NIKA-204 error                                                   │
│  /home/user/secret.txt → NIKA-204 error                                         │
│                                                                                 │
│  RULE 4: Prevent traversal attacks                                              │
│  ../../../etc/passwd → NIKA-204 error (canonicalize then check)                 │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Task P.1: Update path validation in security.rs

**Files:**
- Modify: `tools/nika/src/core/security.rs`
- Test: Add new tests

#### Step 1: Write failing tests (RED)

Add to `src/core/security.rs` test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // BUG #4: Relative paths should work
    #[test]
    fn test_relative_path_accepted() {
        let workdir = PathBuf::from("/Users/x/project");
        let result = validate_file_path("./test.txt", &workdir);
        assert!(result.is_ok(), "Relative path ./test.txt should be accepted");
    }

    #[test]
    fn test_relative_nested_path_accepted() {
        let workdir = PathBuf::from("/Users/x/project");
        let result = validate_file_path("output/results/data.json", &workdir);
        assert!(result.is_ok(), "Nested relative path should be accepted");
    }

    // BUG #1: /tmp/ should be allowed
    #[test]
    fn test_tmp_path_accepted() {
        let workdir = PathBuf::from("/Users/x/project");
        let result = validate_file_path("/tmp/nika-test/file.txt", &workdir);
        assert!(result.is_ok(), "/tmp/ paths should be allowed");
    }

    #[test]
    fn test_tmp_nested_path_accepted() {
        let workdir = PathBuf::from("/Users/x/project");
        let result = validate_file_path("/tmp/deep/nested/path/file.txt", &workdir);
        assert!(result.is_ok(), "Nested /tmp/ paths should be allowed");
    }

    // Security: Outside paths rejected
    #[test]
    fn test_etc_passwd_rejected() {
        let workdir = PathBuf::from("/Users/x/project");
        let result = validate_file_path("/etc/passwd", &workdir);
        assert!(result.is_err(), "/etc/passwd should be rejected");
    }

    #[test]
    fn test_traversal_rejected() {
        let workdir = PathBuf::from("/Users/x/project");
        let result = validate_file_path("../../../etc/passwd", &workdir);
        assert!(result.is_err(), "Traversal attacks should be rejected");
    }

    // Absolute paths within workdir accepted
    #[test]
    fn test_absolute_within_workdir_accepted() {
        let workdir = PathBuf::from("/Users/x/project");
        let result = validate_file_path("/Users/x/project/subdir/file.txt", &workdir);
        assert!(result.is_ok(), "Absolute paths within workdir should be accepted");
    }
}
```

#### Step 2: Run tests to verify they fail

```bash
cd tools/nika && cargo test validate_file_path -- --nocapture
```

Expected: Some tests FAIL

#### Step 3: Implement HYBRID validation (GREEN)

```rust
/// Allowlisted path prefixes (system temp directories)
const PATH_ALLOWLIST: &[&str] = &[
    "/tmp/",
    "/var/tmp/",
    "/private/tmp/",  // macOS
];

/// Validate a file path for builtin tools (v0.21.1 HYBRID approach)
///
/// Rules:
/// 1. Relative paths → resolve against workdir, verify within workdir
/// 2. /tmp/* paths → allowed (allowlisted)
/// 3. Absolute within workdir → allowed
/// 4. Absolute outside workdir → rejected (NIKA-204)
/// 5. Traversal attacks → rejected (canonicalize then check)
pub fn validate_file_path(path: &str, workdir: &Path) -> Result<PathBuf, NikaError> {
    let input_path = Path::new(path);

    // Check allowlist first (before any other validation)
    for prefix in PATH_ALLOWLIST {
        if path.starts_with(prefix) {
            // Allowlisted path - just ensure it doesn't escape via traversal
            let canonical = if input_path.exists() {
                input_path.canonicalize().map_err(|e| NikaError::PathError {
                    path: path.to_string(),
                    reason: e.to_string(),
                })?
            } else {
                // For non-existent paths, just normalize
                normalize_path(input_path)
            };
            // Verify still under allowlist prefix after canonicalization
            if canonical.to_string_lossy().starts_with(prefix) {
                return Ok(canonical);
            }
            // Traversal detected (e.g., /tmp/../etc/passwd)
            return Err(NikaError::PathTraversalDetected {
                path: path.to_string(),
            });
        }
    }

    // Resolve relative paths against workdir
    let resolved = if input_path.is_relative() {
        workdir.join(input_path)
    } else {
        input_path.to_path_buf()
    };

    // Canonicalize to handle .. and symlinks
    let canonical = if resolved.exists() {
        resolved.canonicalize().map_err(|e| NikaError::PathError {
            path: path.to_string(),
            reason: e.to_string(),
        })?
    } else {
        // For non-existent paths, normalize and verify parent exists
        let parent = resolved.parent().ok_or_else(|| NikaError::PathError {
            path: path.to_string(),
            reason: "No parent directory".to_string(),
        })?;
        if parent.exists() {
            let canonical_parent = parent.canonicalize().map_err(|e| NikaError::PathError {
                path: path.to_string(),
                reason: e.to_string(),
            })?;
            canonical_parent.join(resolved.file_name().unwrap_or_default())
        } else {
            // Parent doesn't exist - just normalize the path
            normalize_path(&resolved)
        }
    };

    // Canonicalize workdir for comparison
    let canonical_workdir = workdir.canonicalize().unwrap_or_else(|_| workdir.to_path_buf());

    // Check if path is within workdir
    if canonical.starts_with(&canonical_workdir) {
        return Ok(canonical);
    }

    // Path is outside workdir and not in allowlist
    Err(NikaError::PathOutsideWorkdir {
        path: path.to_string(),
        workdir: canonical_workdir.to_string_lossy().to_string(),
    })
}

/// Normalize a path without requiring it to exist
fn normalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                components.pop();
            }
            std::path::Component::CurDir => {}
            c => components.push(c),
        }
    }
    components.iter().collect()
}
```

#### Step 4: Run tests

```bash
cd tools/nika && cargo test validate_file_path -- --nocapture
```

Expected: All PASS

#### Step 5: Commit

```bash
git add tools/nika/src/core/security.rs
git commit -m "fix(security): implement HYBRID path validation (BUG #1, #4)

- Accept relative paths (resolve against workdir)
- Accept /tmp/, /var/tmp/, /private/tmp/ (allowlist)
- Accept absolute paths within workdir
- Reject absolute paths outside workdir+allowlist
- Prevent traversal attacks via canonicalization
- PATH_ALLOWLIST constant for easy extension
- 7 unit tests for all cases

Fixes: BUG #1 (path validation rejects /tmp/)
Fixes: BUG #4 (builtin tools require absolute paths)

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

### Task P.2: Wire new validation in builtin tools

**Files:**
- Modify: `tools/nika/src/tools/read_tool.rs`
- Modify: `tools/nika/src/tools/write_tool.rs`
- Modify: `tools/nika/src/tools/edit_tool.rs`
- Modify: `tools/nika/src/tools/glob_tool.rs`
- Modify: `tools/nika/src/tools/grep_tool.rs`

Update each tool to use the new `validate_file_path()` function.

---

## Final Verification: Ralph Wiggum Loop

After all fixes are applied, run the codebase audit:

```bash
# Run /codebase-audit skill
```

### Verification Checklist

- [ ] All 4,200+ existing tests pass
- [ ] New tests for BUG #8 pass
- [ ] `cargo clippy -- -D warnings` clean
- [ ] Run manual test of bug reproduction cases
- [ ] Update CHANGELOG.md

### Manual Verification Tests

```bash
# BUG #8: output.schema validation
cd /tmp && mkdir -p nika-verify && cd nika-verify
cat > schema-test.nika.yaml << 'EOF'
schema: nika/workflow@0.10
workflow: schema-verify

tasks:
  - id: generate
    infer:
      prompt: "Generate a person with name and age"
    output:
      format: json
      structured: true
      schema:
        type: object
        properties:
          name: { type: string }
          age: { type: integer }
        required: [name, age]
EOF
nika run schema-test.nika.yaml

# BUG #1: /tmp path acceptance
cat > tmp-test.nika.yaml << 'EOF'
schema: nika/workflow@0.10
workflow: tmp-verify

tasks:
  - id: write_tmp
    agent:
      prompt: "Write 'Hello' to /tmp/nika-verify/test.txt using nika:write"
      tools: [nika:write]
EOF
nika run tmp-test.nika.yaml
cat /tmp/nika-verify/test.txt  # Should show "Hello"

# BUG #4: Relative path acceptance
cat > relative-test.nika.yaml << 'EOF'
schema: nika/workflow@0.10
workflow: relative-verify

tasks:
  - id: write_relative
    agent:
      prompt: "Write 'Test' to ./output.txt using nika:write"
      tools: [nika:write]
EOF
nika run relative-test.nika.yaml
cat ./output.txt  # Should show "Test"
```

---

## Summary

| Task | Bug | Type | Files Changed |
|------|-----|------|---------------|
| 8.1 | #8 | Wiring | executor.rs + test |
| T.1 | #2 | Template | tier*.rs |
| T.2 | #3 | Template | tier6.rs |
| T.3 | #5 | Template | tier5.rs |
| P.1 | #1/#4 | Runtime | security.rs + tests |
| P.2 | #1/#4 | Runtime | tools/*.rs |

**Total Commits:** 6
**New Tests:** ~30
**Lines Changed:** ~200-300

---

## Execution Order

1. **Task 8.1** (CRITICAL) — Wire StructuredOutputEngine
2. **Task T.1-T.3** (HIGH) — Fix templates (can be parallelized)
3. **Task P.1-P.2** (MEDIUM) — HYBRID path validation
4. **Ralph Wiggum** — Full codebase audit
5. **Manual verification** — Test all reproduction cases
6. **CHANGELOG + Version bump** — Finalize release

---

## Success Criteria

| Metric | Target |
|--------|--------|
| Existing tests | 4,200+ passing |
| New tests | ~30 passing |
| Clippy warnings | 0 |
| Bug reproduction | All 7 fixed |
| Manual verification | All 5 test cases pass |
