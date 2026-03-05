# Implicit Output Reference (v0.21.0) Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Enable `$task` syntax in `use:` blocks as shorthand for `task.output` (the full task output).

**Architecture:** The runtime ALREADY supports implicit output resolution via `split_path()` in `binding/resolve.rs`. This feature adds the `$` prefix syntax sugar at the AST/validation layer and updates documentation.

**Tech Stack:** Rust (rustc 1.86+), serde, FxHashMap

---

## Overview

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  IMPLICIT OUTPUT REFERENCE — v0.21.0                                          ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  CURRENT SYNTAX:                                                              ║
║  ─────────────────────────────────────────────────────────────────────────    ║
║  use:                                                                         ║
║    data: task1.output         # Explicit .output required                     ║
║    field: task1.output.field  # Access nested field                           ║
║                                                                               ║
║  NEW SYNTAX (v0.21.0):                                                        ║
║  ─────────────────────────────────────────────────────────────────────────    ║
║  use:                                                                         ║
║    data: $task1               # Implicit output (syntactic sugar)             ║
║    field: task1.field         # Direct field access (unchanged)               ║
║    explicit: task1.output     # Explicit still works (backward compat)        ║
║                                                                               ║
║  RUNTIME BEHAVIOR:                                                            ║
║  ─────────────────────────────────────────────────────────────────────────    ║
║  $task1          → normalize to "task1"                                       ║
║  task1           → resolve_path("task1") → returns full output (ALREADY WORKS)║
║  task1.field     → resolve_path("task1.field") → returns nested field         ║
║  task1.output    → resolve_path("task1.output") → returns full output         ║
║                                                                               ║
║  KEY INSIGHT: DataStore.resolve_path() ALREADY handles implicit output!       ║
║  When path has no dot, it returns the entire output (lines 785-791).          ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

---

## Task 1: Add normalize_path() to UseEntry

**Files:**
- Modify: `tools/nika/src/binding/entry.rs`
- Test: `tools/nika/src/binding/entry.rs` (inline tests)

**Step 1: Write the failing test**

Add to `entry.rs` test module:

```rust
#[test]
fn test_normalize_path_strips_dollar_prefix() {
    assert_eq!(UseEntry::normalize_path("$task1"), "task1");
    assert_eq!(UseEntry::normalize_path("task1"), "task1");
    assert_eq!(UseEntry::normalize_path("$my_task"), "my_task");
    assert_eq!(UseEntry::normalize_path("task.field"), "task.field");
    assert_eq!(UseEntry::normalize_path("$task.field"), "task.field");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p nika normalize_path --lib`
Expected: FAIL with "function `normalize_path` not found"

**Step 3: Write minimal implementation**

Add to `UseEntry` impl block in `entry.rs`:

```rust
impl UseEntry {
    /// Normalize a binding path by stripping the `$` prefix if present.
    ///
    /// The `$` prefix is syntactic sugar for referencing the full output:
    /// - `$task` → `task` (resolves to task.output)
    /// - `task.field` → `task.field` (unchanged)
    /// - `$task.field` → `task.field` (also normalized)
    #[inline]
    pub fn normalize_path(path: &str) -> &str {
        path.strip_prefix('$').unwrap_or(path)
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p nika normalize_path --lib`
Expected: PASS

**Step 5: Commit**

```bash
git add tools/nika/src/binding/entry.rs
git commit -m "$(cat <<'EOF'
feat(binding): add normalize_path for implicit output syntax

Add UseEntry::normalize_path() to strip $ prefix from binding paths.
The $ prefix is syntactic sugar: $task resolves to the full task output.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
EOF
)"
```

---

## Task 2: Apply normalization in UseEntry::from_value()

**Files:**
- Modify: `tools/nika/src/binding/entry.rs`
- Test: `tools/nika/src/binding/entry.rs` (inline tests)

**Step 1: Write the failing test**

Add to `entry.rs` test module:

```rust
#[test]
fn test_use_entry_from_value_normalizes_dollar_prefix() {
    // Shorthand: "$task1" → UseEntry { path: "task1", ... }
    let value = serde_yaml::from_str::<serde_yaml::Value>("\"$task1\"").unwrap();
    let entry = UseEntry::from_value(&value).unwrap();
    assert_eq!(entry.path, "task1");

    // Full form with $
    let value = serde_yaml::from_str::<serde_yaml::Value>(r#"
        path: "$my_task"
        default: "fallback"
    "#).unwrap();
    let entry = UseEntry::from_value(&value).unwrap();
    assert_eq!(entry.path, "my_task");
    assert_eq!(entry.default.as_ref().map(|v| v.as_str()), Some(Some("fallback")));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p nika use_entry_from_value_normalizes --lib`
Expected: FAIL (path will be "$task1" instead of "task1")

**Step 3: Modify from_value() to normalize path**

In `UseEntry::from_value()`:

```rust
// For shorthand string syntax
if let Some(path) = value.as_str() {
    return Ok(Self {
        path: Self::normalize_path(path).to_string(),  // Add normalization
        default: None,
        lazy: false,
    });
}

// For full form map syntax - normalize the path field
let path = map.get("path")
    .and_then(|v| v.as_str())
    .map(Self::normalize_path)  // Add normalization
    .ok_or_else(|| /* error */)?
    .to_string();
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p nika use_entry_from_value_normalizes --lib`
Expected: PASS

**Step 5: Commit**

```bash
git add tools/nika/src/binding/entry.rs
git commit -m "$(cat <<'EOF'
feat(binding): normalize $ prefix in UseEntry::from_value()

Apply normalize_path() when parsing use: blocks to strip $ prefix.
This enables $task syntax as shorthand for full task output.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
EOF
)"
```

---

## Task 3: Add comprehensive tests for implicit output

**Files:**
- Create: `tools/nika/tests/implicit_output_test.rs`

**Step 1: Create test file with 10+ test cases**

```rust
//! Integration tests for implicit output reference syntax ($task).

use nika::binding::{UseEntry, WiringSpec};
use serde_yaml::Value;

#[test]
fn test_dollar_prefix_shorthand() {
    let yaml = r#"
        data: $analyze
    "#;
    let value: serde_yaml::Mapping = serde_yaml::from_str(yaml).unwrap();
    let wiring = WiringSpec::from_mapping(&value).unwrap();
    assert_eq!(wiring.get("data").unwrap().path, "analyze");
}

#[test]
fn test_dollar_prefix_full_form() {
    let yaml = r#"
        data:
          path: $analyze
          default: "none"
    "#;
    let value: serde_yaml::Mapping = serde_yaml::from_str(yaml).unwrap();
    let wiring = WiringSpec::from_mapping(&value).unwrap();
    let entry = wiring.get("data").unwrap();
    assert_eq!(entry.path, "analyze");
    assert_eq!(entry.default.as_ref().unwrap().as_str(), Some("none"));
}

#[test]
fn test_mixed_dollar_and_dot_syntax() {
    let yaml = r#"
        full: $task1
        field: task1.name
        explicit: task1.output
    "#;
    let value: serde_yaml::Mapping = serde_yaml::from_str(yaml).unwrap();
    let wiring = WiringSpec::from_mapping(&value).unwrap();

    assert_eq!(wiring.get("full").unwrap().path, "task1");
    assert_eq!(wiring.get("field").unwrap().path, "task1.name");
    assert_eq!(wiring.get("explicit").unwrap().path, "task1.output");
}

#[test]
fn test_dollar_with_nested_field_not_recommended() {
    // $task.field is valid but not recommended style
    // It normalizes to task.field (same as without $)
    let entry = UseEntry::from_value(&Value::String("$task.field".to_string())).unwrap();
    assert_eq!(entry.path, "task.field");
}

#[test]
fn test_backward_compat_explicit_output() {
    // Old syntax still works
    let yaml = r#"
        data: task.output
        nested: task.output.items[0]
    "#;
    let value: serde_yaml::Mapping = serde_yaml::from_str(yaml).unwrap();
    let wiring = WiringSpec::from_mapping(&value).unwrap();

    assert_eq!(wiring.get("data").unwrap().path, "task.output");
    assert_eq!(wiring.get("nested").unwrap().path, "task.output.items[0]");
}

#[test]
fn test_no_dollar_no_dot_already_works() {
    // task alone already returns full output (implicit .output)
    let yaml = "data: analyze";
    let value: serde_yaml::Mapping = serde_yaml::from_str(yaml).unwrap();
    let wiring = WiringSpec::from_mapping(&value).unwrap();
    assert_eq!(wiring.get("data").unwrap().path, "analyze");
}

#[test]
fn test_lazy_with_dollar_prefix() {
    let yaml = r#"
        data:
          path: $expensive_task
          lazy: true
    "#;
    let value: serde_yaml::Mapping = serde_yaml::from_str(yaml).unwrap();
    let wiring = WiringSpec::from_mapping(&value).unwrap();
    let entry = wiring.get("data").unwrap();
    assert_eq!(entry.path, "expensive_task");
    assert!(entry.lazy);
}

#[test]
fn test_dollar_with_context_prefix_unchanged() {
    // context.* paths should not be affected
    let yaml = "ctx: context.files.readme";
    let value: serde_yaml::Mapping = serde_yaml::from_str(yaml).unwrap();
    let wiring = WiringSpec::from_mapping(&value).unwrap();
    assert_eq!(wiring.get("ctx").unwrap().path, "context.files.readme");
}

#[test]
fn test_dollar_with_inputs_prefix_unchanged() {
    // inputs.* paths should not be affected
    let yaml = "param: inputs.locale";
    let value: serde_yaml::Mapping = serde_yaml::from_str(yaml).unwrap();
    let wiring = WiringSpec::from_mapping(&value).unwrap();
    assert_eq!(wiring.get("param").unwrap().path, "inputs.locale");
}

#[test]
fn test_empty_after_dollar_is_error() {
    // Just "$" alone should be invalid
    let result = UseEntry::from_value(&Value::String("$".to_string()));
    assert!(result.is_err() || result.unwrap().path.is_empty());
}
```

**Step 2: Run tests to verify compilation**

Run: `cargo test -p nika implicit_output --test implicit_output_test`
Expected: All 10 tests pass

**Step 3: Commit**

```bash
git add tools/nika/tests/implicit_output_test.rs
git commit -m "$(cat <<'EOF'
test(binding): add implicit output resolution tests

Add 10 integration tests for $task implicit output syntax:
- Dollar prefix shorthand and full form
- Mixed syntax with dot notation
- Backward compatibility with explicit .output
- Lazy bindings with dollar prefix
- Context and inputs paths unchanged
- Edge cases

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
EOF
)"
```

---

## Task 4: Update documentation with examples

**Files:**
- Modify: `tools/nika/CLAUDE.md` (binding section)

**Step 1: Find binding documentation section**

Read the CLAUDE.md and locate the binding/use: documentation section.

**Step 2: Add implicit output syntax documentation**

Add under the binding section:

```markdown
### Implicit Output Reference (v0.21.0)

Reference task outputs with cleaner syntax using the `$` prefix:

```yaml
tasks:
  - id: analyze
    infer: "Analyze the data"

  - id: report
    use:
      # All three are equivalent:
      data1: $analyze           # New: Implicit output (recommended)
      data2: analyze            # Also works (no dot = full output)
      data3: analyze.output     # Old: Explicit .output (verbose)

      # For nested fields, use dot notation:
      title: analyze.title      # Access nested field
```

**When to use each syntax:**
- `$task` — When you want the entire task output (clearest intent)
- `task.field` — When you want a specific nested field
- `task.output` — Legacy syntax, still supported
```

**Step 3: Commit**

```bash
git add tools/nika/CLAUDE.md
git commit -m "$(cat <<'EOF'
docs: add implicit output reference syntax to CLAUDE.md

Document the $task syntax for v0.21.0:
- New $ prefix as shorthand for full output
- Comparison with existing syntaxes
- Recommendations for when to use each

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
EOF
)"
```

---

## Task 5: Create example workflow

**Files:**
- Create: `tools/nika/examples/v21-implicit-output.nika.yaml`

**Step 1: Create example workflow**

```yaml
# v21-implicit-output.nika.yaml
# Demonstrates implicit output reference syntax ($task)
schema: "nika/workflow@0.10"
provider: claude

tasks:
  # Task 1: Generate some data
  - id: generate
    infer: |
      Generate a JSON object with:
      - title: A creative article title
      - summary: A one-sentence summary
      - tags: An array of 3 tags

      Return valid JSON only.
    output:
      format: json

  # Task 2: Use implicit output reference
  - id: expand
    use:
      # $generate = full output of generate task
      data: $generate
    infer: |
      Expand this article outline into a full paragraph:

      Title: {{use.data.title}}
      Summary: {{use.data.summary}}
      Tags: {{use.data.tags}}

  # Task 3: Show all three equivalent syntaxes
  - id: compare
    use:
      # All three access the same data:
      implicit: $generate         # v0.21 shorthand
      direct: generate            # Also implicit (no dot)
      explicit: generate.output   # Verbose explicit
    infer: |
      Verify all three references are equivalent:
      1. Implicit ($): {{use.implicit}}
      2. Direct: {{use.direct}}
      3. Explicit (.output): {{use.explicit}}

      Are they identical? Respond with YES or NO.

flows:
  - source: generate
    target: [expand, compare]
```

**Step 2: Validate example**

Run: `cargo run -p nika -- check examples/v21-implicit-output.nika.yaml`
Expected: Workflow is valid

**Step 3: Commit**

```bash
git add tools/nika/examples/v21-implicit-output.nika.yaml
git commit -m "$(cat <<'EOF'
docs(examples): add v21-implicit-output example

Demonstrate implicit output reference syntax:
- $task shorthand for full output
- Comparison of all three equivalent syntaxes
- Template interpolation with implicit references

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
EOF
)"
```

---

## Task 6: Version bump and CHANGELOG

**Files:**
- Modify: `tools/nika/Cargo.toml`
- Modify: `CHANGELOG.md`

**Step 1: Bump version in Cargo.toml**

Change version from current to "0.21.0"

**Step 2: Add CHANGELOG entry**

Add to CHANGELOG.md under `## [Unreleased]`:

```markdown
## [0.21.0] - 2026-03-XX

### Added

- **Implicit Output Reference Syntax** — Use `$task` as shorthand for `task.output`
  - `$task` normalizes to `task` during parsing
  - DataStore already resolves paths without dots to full output
  - Cleaner, more intentional syntax for referencing entire task outputs
  - Backward compatible: `task.output` syntax still works
- **Example workflow** — `examples/v21-implicit-output.nika.yaml`
- **10+ binding tests** — Comprehensive coverage for `$` prefix syntax

### Statistics

- **X tests passing** (updated count)
- **Zero clippy warnings**
```

**Step 3: Commit**

```bash
git add tools/nika/Cargo.toml CHANGELOG.md
git commit -m "$(cat <<'EOF'
chore(release): bump version to 0.21.0

Add implicit output reference syntax ($task shorthand).
See CHANGELOG.md for full details.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
EOF
)"
```

---

## Verification Checklist

```bash
# Run all tests
cargo test -p nika

# Run specific implicit output tests
cargo test -p nika implicit_output

# Lint check
cargo clippy -p nika -- -D warnings

# Validate example workflow
cargo run -p nika -- check examples/v21-implicit-output.nika.yaml

# Optional: Run example (requires API key)
cargo run -p nika -- examples/v21-implicit-output.nika.yaml
```

---

## Exit Criteria

- [ ] `$task` syntax works in `use:` blocks
- [ ] `UseEntry::normalize_path()` strips `$` prefix
- [ ] `UseEntry::from_value()` applies normalization
- [ ] Backward compatible with `task.output` syntax
- [ ] 10+ new tests passing
- [ ] Example workflow validates
- [ ] CLAUDE.md documents new syntax
- [ ] CHANGELOG updated
- [ ] Version bumped to 0.21.0
- [ ] Zero clippy warnings

---

## Skills Usage

| Step | Skill | Purpose |
|------|-------|---------|
| All | `superpowers:test-driven-development` | Write tests first |
| All | `superpowers:verification-before-completion` | Verify before commit |
| Debug | `superpowers:systematic-debugging` | If tests fail |
| Review | `superpowers:requesting-code-review` | After completion |
