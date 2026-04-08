# Contributing to Nika

Thank you for your interest in contributing to Nika! Whether you're filing a bug, proposing a feature, improving docs, or submitting code, every contribution matters.

Nika is an open-source project licensed under **AGPL-3.0-or-later**. By submitting a pull request, you agree to our [Contributor License Agreement (CLA)](https://cla-assistant.io/supernovae-st/nika). The CLA lets us offer Nika under dual licensing (AGPL + commercial) while you retain full copyright of your work.

## Table of Contents

- [Getting Started](#getting-started)
- [Project Structure](#project-structure)
- [Development Workflow](#development-workflow)
- [Testing](#testing)
- [Commit Convention](#commit-convention)
- [Pull Request Process](#pull-request-process)
- [Issue Guidelines](#issue-guidelines)
- [Version Lock Policy](#version-lock-policy)
- [Contributor License Agreement](#contributor-license-agreement)
- [Code of Conduct](#code-of-conduct)

## Getting Started

### Prerequisites

- **Rust 1.86+** (via [rustup](https://rustup.rs/))
- **Git 2.40+**
- **cargo-nextest** (recommended test runner): `cargo install cargo-nextest`

### Setup

```bash
# Clone the repository
git clone https://github.com/supernovae-st/nika.git
cd nika

# Build from the workspace root
cd tools/nika
cargo build

# Run tests (IMPORTANT: always use --lib to avoid macOS Keychain popups)
cargo test --workspace --lib

# Or with nextest (parallel, faster)
cargo nextest run --workspace --lib

# Run the binary
cargo run -- --help
```

### Quick Validation (run before every push)

```bash
cd tools/nika
cargo fmt --all --check
cargo clippy --workspace -- -D warnings
cargo test --workspace --lib
```

## Project Structure

Nika is a **17-crate Cargo workspace** under `tools/`:

```
tools/
├── nika/           CLI binary (entry point)
├── nika-engine/    Execution engine (largest crate)
├── nika-core/      AST, types, catalogs (zero I/O)
├── nika-tui/       Terminal UI (ratatui)
├── nika-daemon/    Background daemon (secrets, jobs, cache)
├── nika-init/      Project scaffolding + course
├── nika-cli/       CLI subcommands
├── nika-event/     EventLog, TraceWriter
├── nika-mcp/       MCP client (rmcp)
├── nika-media/     Content-addressable store, image processing
├── nika-serve/     HTTP API server
├── nika-storage/   SQLite persistence
├── nika-sdk/       Rust SDK
├── nika-napi/      Node.js bindings (N-API)
├── nika-py/        Python bindings
├── nika-lsp/       Language Server binary
└── nika-lsp-core/  LSP intelligence
```

The workspace root is `tools/nika/Cargo.toml`. All `cargo` commands run from `tools/nika`.

## Development Workflow

### 1. Pick or Create an Issue

Check [existing issues](https://github.com/supernovae-st/nika/issues) first. If your idea isn't there, open one to discuss before writing code.

### 2. Create a Branch

```bash
# Feature branches from main
git checkout -b feat/my-feature main

# Or use worktrees for isolation (recommended for larger work)
git worktree add ../.worktrees/nika-my-feature -b feat/my-feature
cd ../.worktrees/nika-my-feature/tools/nika
```

### Branch Naming

```
feat/     New functionality
fix/      Bug fixes
docs/     Documentation only
refactor/ Code refactoring
test/     Test improvements
chore/    Tooling, CI, dependencies
perf/     Performance improvements
```

### 3. Develop

```bash
cd tools/nika

# Run specific crate tests during development
cargo test -p nika-engine --lib
cargo test -p nika-core --lib

# Check lints
cargo clippy --workspace -- -D warnings

# Format code
cargo fmt --all
```

### 4. Validate Before Pushing

All of these must pass:

```bash
cargo fmt --all --check
cargo clippy --workspace -- -D warnings
cargo test --workspace --lib
```

### 5. Push and Open a PR

```bash
git push -u origin feat/my-feature
```

Then open a Pull Request on GitHub. Fill out the PR template completely.

### 6. CI + Review

- CI runs automatically (format, clippy, tests across Ubuntu/macOS/Windows, coverage, security audit)
- Automated code review runs
- A maintainer will review your PR

### 7. Merge

- Squash merge to main
- release-plz auto-creates Release PRs for versioned changes

### Cleanup

```bash
# If you used a worktree
git worktree remove ../.worktrees/nika-my-feature

# Or just delete the branch
git branch -d feat/my-feature
```

## Testing

### Running Tests

```bash
cd tools/nika

# All workspace tests (9,000+)
cargo test --workspace --lib

# Specific crate
cargo test -p nika-engine --lib          # Engine (4,170+ tests)
cargo test -p nika-tui --lib             # TUI (2,150+ tests)
cargo test -p nika-daemon --lib          # Daemon (164 tests)
cargo test -p nika-core --lib            # Core (AST, transforms)

# With nextest (parallel, recommended)
cargo nextest run --workspace --lib

# Specific test pattern
cargo test -p nika-engine --lib -- display
```

**IMPORTANT**: Always use `--lib`. Running `cargo test` without `--lib` triggers integration tests that may cause macOS Keychain popups.

### Writing Tests

- **Location**: In-module `#[cfg(test)]` blocks (unit tests) or `tests/` directories (integration)
- **Snapshots**: We use the `insta` crate for snapshot testing
- **Naming**: `test_<function>_<scenario>_<expected_outcome>`
- **Philosophy**: Tests must validate behavior programmatically, not just check `!is_empty()`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_workflow_valid_yaml_returns_workflow() {
        // Arrange
        let yaml = r#"
            schema: "nika/workflow@0.12"
            tasks:
              - id: hello
                infer: "Say hello"
        "#;

        // Act
        let result = parse_workflow(yaml);

        // Assert
        assert!(result.is_ok());
        let wf = result.unwrap();
        assert_eq!(wf.tasks.len(), 1);
        assert_eq!(wf.tasks[0].id, "hello");
    }
}
```

## Commit Convention

We use [Conventional Commits](https://www.conventionalcommits.org/):

```
type(scope): concise description

[optional body]

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

### Types

| Type | Description |
|------|-------------|
| `feat` | New feature |
| `fix` | Bug fix |
| `docs` | Documentation |
| `style` | Formatting (no logic change) |
| `refactor` | Code refactoring |
| `test` | Adding/updating tests |
| `chore` | Tooling, CI, dependencies |
| `perf` | Performance improvement |

### Scopes

Common scopes: `engine`, `tui`, `ast`, `runtime`, `mcp`, `provider`, `dag`, `event`, `binding`, `media`, `cli`, `serve`, `daemon`, `core`, `lsp`

### Examples

```
feat(engine): add structured output repair with cheaper model
fix(mcp): handle timeout in tool calls
docs(readme): update installation section
refactor(runtime): simplify executor dispatch
test(binding): add lazy resolution edge cases
chore(ci): update Rust toolchain to 1.86
perf(media): SIMD-accelerate thumbnail generation
```

### One Fix = One Commit

Each logical change gets its own commit. Don't batch unrelated fixes. Exception: tightly coupled changes (rename + usages, feature + tests, bugfix + regression test).

## Pull Request Process

1. **Create a feature branch** from `main`
2. **Make changes** with conventional commits
3. **Run quality gates** locally (fmt, clippy, test)
4. **Push** and open a PR
5. **Fill out the PR template** completely
6. **Wait for CI.** All jobs must pass
7. **Address review feedback**
8. **Squash merge** when approved

### PR Title

Follow the same convention as commits:

```
feat(tui): add streaming inference widget
fix(runtime): prevent infinite recursion in spawn_agent
```

### What Makes a Good PR

- **Small and focused**: One concern per PR
- **Tests included**: New functionality has tests, bug fixes include a regression test
- **Docs updated**: If you changed behavior, update relevant docs
- **CHANGELOG entry**: For user-facing changes, add an entry to CHANGELOG.md

## Issue Guidelines

### Bug Reports

Use the [Bug Report template](https://github.com/supernovae-st/nika/issues/new?template=bug_report.yml). Include:

- Nika version (`nika --version`)
- Your OS and provider
- The workflow file (sanitized, remove API keys)
- Full error output
- Steps to reproduce

### Feature Requests

Use the [Feature Request template](https://github.com/supernovae-st/nika/issues/new?template=feature_request.yml). Include:

- The problem you're solving
- Proposed workflow syntax (if applicable)
- Alternatives you considered

### Security Issues

**Do NOT report security vulnerabilities through public GitHub issues.** Email **security@supernovae.studio** instead. See [SECURITY.md](.github/SECURITY.md).

## Version Lock Policy

```
Nika will NEVER be version 1.0.0 or higher.
Valid versions: 0.0.1 through 0.99.99
```

This is by design:

- Perpetual 0.x.x enables continuous evolution
- SemVer 0.x allows breaking changes without drama
- Follows Rust ecosystem norms (many crates stay 0.x forever)

PRs that bump the version to 1.0.0+ will be automatically rejected by CI.

## Error Handling

Nika uses `NikaError` with `NIKA-XXX` codes, not `anyhow`. When adding error cases:

- Pick the right error code range (see `tools/nika/CLAUDE.md` for the full table)
- Include a `FixSuggestion` when possible
- Never expose internal paths or secrets in error messages

## Code Style

- **Formatting**: `cargo fmt` (default rustfmt config)
- **Linting**: Zero clippy warnings (`-D warnings`)
- **Errors**: `NikaError` with NIKA-XXX codes
- **AST pipeline**: Always Raw -> Analyzed -> Lower (never skip phases)
- **Logging**: `tracing` macros
- **Extensions**: `.nika.yaml` for workflows
- **License header**: All `.rs` files carry `// SPDX-License-Identifier: AGPL-3.0-or-later` (automated)

See [CONVENTIONS.md](CONVENTIONS.md) for workflow authoring conventions.

## AI-Assisted Development

We use AI tools (Claude, Copilot, etc.) to build Nika. You can too.

**What we expect from AI-assisted PRs:**

- **You reviewed every line.** AI-generated code that you haven't read is not a contribution. It's a liability. If you can't explain why a line exists, don't submit it.
- **You tested it.** `cargo test --workspace --lib` passes. Not "it should work." It works.
- **You made the design decisions.** AI can write a function. You decide whether that function should exist at all.
- **Use the co-author convention.** If AI helped write the code, add a co-author line in your commit. We do it on every commit (`Co-Authored-By: Nika 🦋 <nika@supernovae.studio>`) and we expect the same honesty from contributors.

**What we reject:**

- AI slop: unreviewed, untested, generated-and-pasted code with no human judgment
- PRs where the contributor can't answer questions about their own code
- "I asked ChatGPT to fix this" without understanding what changed

**The bottom line:** AI is a power tool, like cargo-clippy or rust-analyzer. The value is in the decisions: what to build, what to reject, how things compose. Use AI to go faster. But own the result.

## Contributor License Agreement

When you open your first pull request, the CLA-assistant bot will ask you to sign our [CLA](https://cla-assistant.io/supernovae-st/nika). This is a one-time process. Comment "I have read the CLA Document and I hereby sign the CLA" on the PR and you're done.

The CLA grants SuperNovae a license to distribute your contributions under AGPL and commercial licenses. **You retain full copyright.** See the [full CLA text](https://cla-assistant.io/supernovae-st/nika) for details.

## Code of Conduct

Read our [Code of Conduct](.github/CODE_OF_CONDUCT.md). The short version: be kind, show your ugly work, discuss before you build, and remember that quality is not negotiable here.

## Questions?

- **Bugs or features**: [Open an issue](https://github.com/supernovae-st/nika/issues)
- **Discussion before PRs**: [GitHub Discussions](https://github.com/supernovae-st/nika/discussions)
- **Security**: Email security@supernovae.studio

---

Built with care by [SuperNovae Studio](https://supernovae.studio), Paris.
