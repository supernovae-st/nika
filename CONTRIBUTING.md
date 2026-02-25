# Contributing to Nika

Thank you for your interest in contributing to Nika! This document provides guidelines and workflows for contributing.

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  🏴‍☠️ ARMADA — Nika Contribution Guide                                         ║
║  ───────────────────────────────────────────────────────────────────────────  ║
║  "All ships must pass the checkpoint"                                         ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

## Table of Contents

- [Version Lock Policy](#version-lock-policy)
- [ARMADA Quality System](#armada-quality-system)
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

See [ARMADA Design](docs/plans/2025-02-25-nika-fortress-design.md) for details.

## ARMADA Quality System

Every contribution passes through the **10-station ARMADA checkpoint**:

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  🏴‍☠️ ARMADA — 10 QUALITY STATIONS                                             ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║   Station 1: 🔧 Format         cargo fmt --check                              ║
║   Station 2: 📎 Lint           cargo clippy -- -D warnings                    ║
║   Station 3: 🧪 Tests          cargo nextest run                              ║
║   Station 4: 📊 Coverage       cargo llvm-cov (>80%)                          ║
║   Station 5: 📖 Docs           cargo doc --no-deps                            ║
║   Station 6: 🔒 Security       cargo audit + cargo deny                       ║
║   Station 7: 🤖 CodeRabbit     AI review (general patterns)                   ║
║   Station 8: 🧠 Claude AI      AI review (Nika-specific)                      ║
║   Station 9: 📝 Conventional   commitlint validation                          ║
║   Station 10: ⚓ Version Lock  0.x.x enforcement (NEVER 1.0.0)                ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### Running Locally

```bash
# Quick check (minimum before push)
cargo fmt --check && cargo clippy -- -D warnings && cargo nextest run

# Full ARMADA check
cargo fmt --check
cargo clippy -- -D warnings
cargo nextest run --all-features
cargo llvm-cov --all-features
cargo doc --no-deps --all-features
cargo audit && cargo deny check
```

## Development Workflow

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  🏴‍☠️ ARMADA — DEVELOPER FLOW                                                  ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  ┌─────────┐     ┌─────────┐     ┌─────────┐     ┌─────────┐     ┌─────────┐ ║
║  │WORKTREE │────▶│ DEVELOP │────▶│ COMMIT  │────▶│  PUSH   │────▶│   PR    │ ║
║  └─────────┘     └─────────┘     └─────────┘     └─────────┘     └─────────┘ ║
║       │               │               │               │               │      ║
║       ▼               ▼               ▼               ▼               ▼      ║
║  git worktree    cargo test     Conventional    Feature branch   ARMADA CI   ║
║  add             cargo clippy   Commits         created          10 stations ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
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

### Step 5: Wait for ARMADA

- All 10 stations must pass
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
Co-Authored-By: Nika <agent@nika.sh>
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
chore(ci): update rust version to 1.75
```

## Pull Request Process

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  🏴‍☠️ PR LIFECYCLE                                                             ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║   1. Create feature branch from main                                          ║
║   2. Make changes with conventional commits                                   ║
║   3. Run quality gates locally                                                ║
║   4. Push and open PR                                                         ║
║   5. Fill out PR template completely                                          ║
║   6. Wait for ARMADA CI (all 10 stations)                                     ║
║   7. Address review feedback                                                  ║
║   8. Squash merge when approved                                               ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
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
- [ ] All ARMADA stations pass locally

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

- Rust 1.75+ (rustup recommended)
- Git 2.40+
- cargo-nextest (for testing)

### Setup

```bash
# Clone the repository
git clone https://github.com/SuperNovae-studio/nika.git
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
