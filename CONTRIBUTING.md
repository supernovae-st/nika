# Contributing to Nika

Thank you for your interest in contributing to Nika! This document provides guidelines and workflows for contributing.

## Table of Contents

- [Version Lock Policy](#version-lock-policy)
- [CI Quality Gates](#ci-quality-gates)
- [Development Workflow](#development-workflow)
- [Commit Convention](#commit-convention)
- [Pull Request Process](#pull-request-process)
- [Testing](#testing)
- [Documentation](#documentation)

## Version Lock Policy

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  ⚓ CAPTAIN'S ORDERS: VERSION LOCK                                            ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  Nika will NEVER be version 1.0.0 or higher.                                  ║
║                                                                               ║
║  Valid versions: 0.0.1 through 0.99.99                                        ║
║                                                                               ║
║  Why?                                                                         ║
║  - Perpetual 0.x.x enables continuous evolution                               ║
║  - SemVer 0.x allows breaking changes without drama                           ║
║  - Follows Rust ecosystem norms (many crates stay 0.x forever)                ║
║                                                                               ║
║  PRs that bump the version to 1.0.0+ will be automatically rejected.          ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

See `.github/workflows/ci.yml` for details.

## CI Quality Gates

Every contribution passes through 8 jobs in ci.yml:

```
check → test → test-features → coverage
security → semver → validate → summary
```

| Job | What it checks |
|-----|---------------|
| `check` | cargo fmt + clippy + doc + version lock |
| `test` | cargo nextest --lib on ubuntu + macos |
| `test-features` | no-default-features + all-features compatibility |
| `coverage` | cargo-llvm-cov nextest → Codecov |
| `security` | cargo audit + cargo deny + cargo machete |
| `semver` | Breaking change detection |
| `validate` | nika check on all examples |
| `summary` | PR comment with all results |

### Running Locally

```bash
# Quick check (minimum before push)
cd tools/nika
cargo fmt --check && cargo clippy --workspace -- -D warnings && cargo nextest run --workspace --lib

# Full local check
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace --lib
cargo test --workspace --doc
cargo audit && cargo deny check
```

## Development Workflow

```
WORKTREE → DEVELOP → COMMIT → PUSH → PR → CI → MERGE
  │           │         │        │      │    │
  │           │         │        │      │    └─ 8 jobs pass
  │           │         │        │      └───── CodeRabbit + review
  │           │         │        └──────────── Feature branch
  │           │         └───────────────────── Conventional Commits
  │           └─────────────────────────────── cargo nextest --lib
  └─────────────────────────────────────────── git worktree add
```

### Step 1: Create Worktree (Recommended)

```bash
# Create a worktree for your feature
git worktree add ../.worktrees/nika-my-feature -b feat/my-feature

# Work in isolation
cd ../.worktrees/nika-my-feature
```

### Step 2: Develop

```bash
# Run tests continuously
cargo test

# Check lints
cargo clippy -- -D warnings

# Validate before commit
cargo fmt --check && cargo clippy -- -D warnings && cargo nextest run
```

### Step 3: Commit (Conventional)

```bash
git add .
git commit -m "feat(tui): add new widget"
```

### Step 4: Push & PR

```bash
git push -u origin feat/my-feature
gh pr create --title "feat(tui): add new widget" --body "..."
```

### Step 5: Wait for CI

- All 8 jobs must pass (ci.yml)
- CodeRabbit + Claude AI review automatically
- Human review required

### Step 6: Merge

- Squash merge to main
- release-plz auto-creates Release PR
- Merge Release PR -> auto-tag v0.x.x

### Step 7: Cleanup

```bash
cd /path/to/nika
git worktree remove ../.worktrees/nika-my-feature
```

### Branch Naming

```
feat/     -> New functionality
fix/      -> Bug fixes
docs/     -> Documentation only
refactor/ -> Code refactoring
test/     -> Test improvements
chore/    -> Tooling, CI, dependencies
```

## Commit Convention

We use [Conventional Commits](https://www.conventionalcommits.org/):

```
type(scope): description

[optional body]

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
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
| `ci` | CI/CD changes |

### Examples

```bash
feat(tui): add command palette widget
fix(mcp): handle timeout in tool calls
docs(readme): update installation guide
refactor(runtime): simplify executor logic
test(binding): add lazy resolution tests
chore(ci): update rust version to 1.86
```

## Pull Request Process

```
1. Create feature branch from main
2. Make changes with conventional commits
3. Run quality gates locally
4. Push and open PR
5. Fill out PR template completely
6. Wait for CI (all 8 jobs)
7. Address review feedback
8. Squash merge when approved
```

### PR Title Format

Follow the same convention as commits:

```
feat(tui): add streaming inference widget
fix(runtime): prevent infinite recursion in spawn_agent
```

### PR Checklist

- [ ] Code follows project style guidelines
- [ ] Tests added for new functionality
- [ ] Documentation updated if needed
- [ ] CHANGELOG.md updated for significant changes
- [ ] All CI quality gates pass locally

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

- New features -> Update CLAUDE.md and README
- API changes -> Update docstrings
- Bug fixes -> Add test documenting the fix
- Breaking changes -> Update CHANGELOG.md

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

## Getting Started

### Prerequisites

- Rust 1.86+ (rustup recommended)
- Git 2.40+
- cargo-nextest (for testing)

### Setup

```bash
# Clone the repository
git clone https://github.com/supernovae-st/nika.git
cd nika/tools/nika

# Build
cargo build

# Run tests
cargo nextest run

# Run the TUI
cargo run
```

## Questions?

- Open an issue for bugs or feature requests
- Check existing issues before creating new ones
- For security issues, see [SECURITY.md](.github/SECURITY.md)

---

**Thank you for contributing to Nika!** 🏴‍☠️
