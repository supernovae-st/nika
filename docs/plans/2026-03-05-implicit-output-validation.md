# Implicit Output v0.21 - Validation Plan

> **For Claude:** Use superpowers:executing-plans to execute task-by-task.

**Goal:** Comprehensive validation of the implicit output feature and all Nika binding/workflow functionality.

**Architecture:** Multi-agent parallel validation with Rust experts, code reviewers, and integration testing.

---

## Phase 1: Code Quality Review (Parallel Agents)

### Agent A: Rust Architecture Review
- Review `binding/entry.rs` for idiomatic Rust patterns
- Verify `normalize_path()` implementation efficiency
- Check error handling and edge cases
- Validate test coverage completeness

### Agent B: Code Security Review
- Check for path traversal vulnerabilities
- Verify no injection attacks possible via `$` prefix
- Review template interpolation safety
- Validate input sanitization

### Agent C: Integration Patterns Review
- Verify DAG binding resolution order
- Check `use:` block parsing correctness
- Validate cross-task data flow
- Review artifact integration points

---

## Phase 2: Workflow Test Suite

### Test Category 1: Basic Binding Tests
```yaml
# test-binding-basic.nika.yaml
- $task shorthand works
- Explicit task reference works
- Both forms are equivalent
- Nested field access ($task.field)
```

### Test Category 2: Advanced Binding Tests
```yaml
# test-binding-advanced.nika.yaml
- Multiple $ prefixes ($$task → $task)
- Empty path handling
- Path with dots ($task.nested.field)
- Default values ($task ?? "fallback")
```

### Test Category 3: Workflow Composition
```yaml
# test-workflow-composition.nika.yaml
- include: with $task references
- context: file loading with bindings
- Cross-workflow data flow
- DAG fusion with prefixed tasks
```

### Test Category 4: Artifact Integration
```yaml
# test-artifact-binding.nika.yaml
- Artifact output with $task reference
- Template variables in artifact paths
- JSON/YAML artifact formatting
- Artifact manifest generation
```

### Test Category 5: All 5 Verbs with Bindings
```yaml
# test-verbs-binding.nika.yaml
- infer: with $task context
- exec: with $task templating
- fetch: with $task URL params
- invoke: with $task MCP params
- agent: with $task tool context
```

---

## Phase 3: Edge Case Validation

### Edge Case Matrix
| Input | Expected | Test |
|-------|----------|------|
| `$task` | `task` | ✅ |
| `$$task` | `$task` | ✅ |
| `task` | `task` | ✅ |
| `$` | `` (empty) | ✅ |
| `$task.field` | `task.field` | ✅ |
| `path$var` | `path$var` | ✅ (no change) |

---

## Phase 4: Regression Testing

### Run Full Test Suite
```bash
cargo test --all-features
cargo test binding
cargo test normalize
cargo test workflow
```

### Benchmark Performance
```bash
cargo bench binding
```

---

## Phase 5: Documentation Validation

- [ ] CLAUDE.md updated with examples
- [ ] CHANGELOG.md has feature entry
- [ ] Example workflow is valid
- [ ] JSON Schema updated (if needed)

---

## Success Criteria

1. All 3,800+ tests pass
2. Zero clippy warnings
3. All edge cases covered
4. Example workflow executes successfully
5. No security vulnerabilities
6. Performance unchanged
