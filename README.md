# Nika CLI

> ⚠️ **Private Repository** - Proprietary closed-source CLI for Nika workflow orchestration.

The [Nika Specification](https://github.com/supernovae-studio/nika) is open source (Apache 2.0).
This CLI is closed source and distributed as a binary.

## Requirements

- Rust 1.75+ (install via [rustup](https://rustup.rs/))
- Claude Code CLI (for workflow execution)

## Development

```bash
# Build
cargo build

# Run
cargo run -- --help

# Test
cargo test

# Release build
cargo build --release
```

## Commands

```bash
nika run <workflow>      # Run a .wf.yaml workflow
nika validate [path]     # Validate workflow files
nika init [name]         # Initialize new .nika project
nika add <package>       # Install community package
nika publish <file>      # Publish to registry
nika auth login          # Authenticate with registry
nika tui                 # Launch TUI dashboard
```

## Architecture

```
src/
├── main.rs             # CLI entry point (clap)
├── lib.rs              # Library exports
├── auth.rs             # Registry authentication
├── publish.rs          # Package publishing
├── workflow.rs         # Workflow execution
├── validator.rs        # Schema validation
├── validators.rs       # Validation rules
├── rules.rs            # Paradigm rules
├── custom_nodes.rs     # Custom node handling
└── errors.rs           # Error types
```

## Dependencies

- **clap** - CLI argument parsing
- **ratatui** - Terminal UI framework
- **crossterm** - Cross-platform terminal library
- **serde_yaml** - YAML parsing
- **jsonschema** - JSON Schema validation
- **tokio** - Async runtime

## Related

- [Nika Specification](https://github.com/supernovae-studio/nika) (Apache 2.0)
- [Documentation](https://nika.dev/docs)

## License

Proprietary - © 2025 SuperNovae Studio. All rights reserved.
