# Contributing to Nika

Thank you for your interest in contributing to Nika! This document provides guidelines and workflows for contributing.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Version Lock Policy](#version-lock-policy)
- [Getting Started](#getting-started)
- [Development Workflow](#development-workflow)
- [Commit Convention](#commit-convention)
- [Pull Request Process](#pull-request-process)
- [Quality Gates](#quality-gates)
- [Testing](#testing)
- [Documentation](#documentation)

## Code of Conduct

Be respectful, inclusive, and constructive. We're building something great together.

## Version Lock Policy

**⚠️ CRITICAL:** Nika will **NEVER** be version 1.0.0 or higher.

This is intentional, not a bug:
- Perpetual 0.x.x enables continuous evolution
- SemVer 0.x allows breaking changes without drama
- See [FORTRESS Design](docs/plans/2025-02-25-nika-fortress-design.md)

PRs that bump the version to 1.0.0+ will be automatically rejected.

## Getting Started

### Prerequisites

- Rust 1.75+ (rustup recommended)
- Git 2.40+

### Setup

```bash
# Clone the repository
git clone https://github.com/SuperNovae-studio/nika.git
cd nika

# Build
cargo build

# Run tests
cargo test

# Run the TUI
cargo run
```

## Development Workflow

### Git Worktree (Recommended)

For isolated feature development, we recommend git worktrees:

```bash
# Create a worktree for your feature
git worktree add -b feature/my-feature ../nika-my-feature main

# Work in isolation
cd ../nika-my-feature

# When done, clean up
cd ../nika
git worktree remove ../nika-my-feature
```

### Branch Naming

```
feature/  → New functionality
fix/      → Bug fixes
docs/     → Documentation only
refactor/ → Code refactoring
test/     → Test improvements
chore/    → Tooling, CI, dependencies
```

Examples:
- `feature/lazy-bindings`
- `fix/mcp-timeout`
- `docs/update-readme`

## Commit Convention

We use [Conventional Commits](https://www.conventionalcommits.org/):

```
type(scope): description

[optional body]

[optional footer]
```

### Types

| Type | Description |
|------|-------------|
| `feat` | New feature |
| `fix` | Bug fix |
| `docs` | Documentation |
| `style` | Formatting (no code change) |
| `refactor` | Code refactoring |
| `test` | Adding/updating tests |
| `chore` | Tooling, CI, dependencies |
| `perf` | Performance improvement |

### Examples

```bash
feat(tui): add command palette widget
fix(mcp): handle timeout in tool calls
docs(readme): update installation guide
refactor(runtime): simplify executor logic
test(binding): add lazy resolution tests
chore(ci): update rust version to 1.75
```

### Co-Authors

Include AI assistants in commits:

```
feat(tui): add dark mode support

Implemented theme switching with system detection.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <agent@nika.sh>
```

## Pull Request Process

1. **Create a feature branch** from `main`
2. **Make your changes** with appropriate commits
3. **Run quality gates** locally (see below)
4. **Push your branch** and open a PR
5. **Fill out the PR template** completely
6. **Wait for CI** to pass all checks
7. **Address review feedback** if any
8. **Merge** once approved

### PR Title Format

Follow the same convention as commits:

```
feat(tui): add streaming inference widget
fix(runtime): prevent infinite recursion in spawn_agent
```

## Quality Gates

All PRs must pass these checks (enforced by CI):

### Required Checks

```bash
# Format check
cargo fmt --check

# Lint check (must be warning-free)
cargo clippy -- -D warnings

# All tests must pass
cargo test

# Version must remain 0.x.x
# (Automatically enforced)
```

### Running Locally

```bash
# Run all quality gates
cargo fmt --check && cargo clippy -- -D warnings && cargo test
```

## Testing

### Test Structure

- **Unit tests**: In-module `#[cfg(test)]` blocks
- **Integration tests**: `tests/` directory
- **Snapshot tests**: Using `insta` crate

### Writing Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feature_behavior() {
        // Arrange
        let input = "test";

        // Act
        let result = my_function(input);

        // Assert
        assert_eq!(result, expected);
    }
}
```

### Test Naming

```
test_<function>_<scenario>_<expected_outcome>
```

Examples:
- `test_parse_workflow_valid_yaml_returns_workflow`
- `test_execute_task_missing_binding_returns_error`

## Documentation

### When to Update

- New features → Update CLAUDE.md and README
- API changes → Update docstrings
- Bug fixes → Add test documenting the fix
- Breaking changes → Update CHANGELOG.md

### Docstring Style

```rust
/// Brief one-line description.
///
/// More detailed explanation if needed.
///
/// # Arguments
///
/// * `param` - Description of the parameter
///
/// # Returns
///
/// Description of return value
///
/// # Errors
///
/// When and why this function returns an error
///
/// # Examples
///
/// ```rust
/// let result = my_function("input");
/// assert!(result.is_ok());
/// ```
pub fn my_function(param: &str) -> Result<Output, Error> {
    // ...
}
```

## Questions?

- Open an issue for bugs or feature requests
- Check existing issues before creating new ones
- For security issues, see [SECURITY.md](.github/SECURITY.md)

---

**Thank you for contributing to Nika!** 🚀
