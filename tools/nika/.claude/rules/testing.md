# Testing Rules for Nika

## TDD Workflow

1. **Write failing test first** - Always
2. **Run test to see it fail** - Verify error message makes sense
3. **Write minimal code** - Only what's needed to pass
4. **Run test to see it pass** - Verify green
5. **Refactor** - Only if needed
6. **Commit** - Atomic commits per feature

## Test File Location

- Unit tests: Same file as implementation in `#[cfg(test)]` module
- Integration tests: `tests/` directory
- Snapshot tests: Use insta with `.snap` files in `tests/snapshots/`

## Test Naming

```rust
#[test]
fn test_<function>_<scenario>_<expected_outcome>() {
    // arrange
    // act
    // assert
}

// Examples:
fn test_parse_workflow_valid_yaml_returns_workflow()
fn test_parse_workflow_missing_schema_returns_error()
fn test_execute_task_infer_calls_provider()
```

## Assertions

- Use `pretty_assertions` for struct comparisons
- Use `insta::assert_yaml_snapshot!` for complex outputs
- Use `proptest` for parser fuzzing

## Mocking

- Prefer real implementations over mocks
- If mocking needed, use `mockall` or manual test doubles
- Never mock what you don't own
