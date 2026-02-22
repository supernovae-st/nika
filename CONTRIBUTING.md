# Contributing to Nika

Thank you for your interest in contributing to Nika! This document provides guidelines for contributing.

## Getting Started

### Prerequisites

- Rust 1.86+ (`rustup update`)
- Git

### Setup

```bash
git clone https://github.com/supernovae-studio/nika.git
cd nika
cargo build
cargo test
```

## Development Workflow

### 1. Create a Branch

```bash
git checkout -b feat/your-feature
# or
git checkout -b fix/your-bugfix
```

### 2. Make Changes

Follow the code style:
- Run `cargo fmt` before committing
- Run `cargo clippy -- -D warnings`
- Add tests for new functionality
- Update documentation as needed

### 3. Test

```bash
# Run all tests
cargo test

# Run specific crate tests
cargo test -p nika-core
cargo test -p nika-runtime

# Run with coverage
cargo llvm-cov nextest
```

### 4. Commit

Use [Conventional Commits](https://www.conventionalcommits.org/):

```bash
git commit -m "feat(runtime): add lazy binding support"
git commit -m "fix(mcp): handle timeout correctly"
git commit -m "docs: update README examples"
```

### 5. Push and Create PR

```bash
git push origin feat/your-feature
```

Then create a Pull Request on GitHub.

## Code Organization

```
nika/
├── crates/
│   ├── nika-core/     # Core types, AST, DAG
│   ├── nika-mcp/      # MCP client
│   ├── nika-provider/ # LLM providers
│   ├── nika-runtime/  # Execution engine
│   ├── nika-tui/      # Terminal UI
│   └── nika-cli/      # CLI binary
├── examples/          # Example workflows
└── docs/              # Documentation
```

## Guidelines

### Error Handling

Use `NikaError` with proper error codes:

```rust
// Good
return Err(NikaError::ParseError {
    source: e.to_string(),
    line: Some(10),
});

// Bad
return Err(anyhow::anyhow!("parse error"));
```

### Testing

- Write tests for new functionality
- Use `insta` for snapshot testing
- Use `proptest` for parser fuzzing

### Documentation

- Add doc comments to public APIs
- Update README for user-facing changes
- Keep CHANGELOG up to date

## Need Help?

- Open an issue for bugs or feature requests
- Check existing issues before creating new ones
- Join discussions in existing PRs

## License

By contributing, you agree that your contributions will be licensed under AGPL-3.0.
