# @supernovae/nika

Thin npm wrapper for the [Nika CLI](https://github.com/supernovae-st/nika) -- a semantic YAML workflow engine for AI tasks.

This package downloads the pre-built Nika binary for your platform during `npm install`.

## Install

```bash
# Global install
npm install -g @supernovae/nika

# Or run directly
npx @supernovae/nika

# Or as a project dependency
npm install @supernovae/nika
```

## Usage

```bash
nika run workflow.nika.yaml      # Execute a workflow
nika check workflow.nika.yaml    # Validate syntax + DAG
nika ui                          # Terminal UI
nika provider list               # Check API key status
nika init                        # Interactive project setup
nika course next                 # Start the learning course
```

## Supported Platforms

| OS      | Architecture |
|---------|-------------|
| macOS   | arm64 (Apple Silicon) |
| macOS   | x64 (Intel) |
| Linux   | x64 |
| Linux   | arm64 |

## License

AGPL-3.0-or-later

## Links

- [GitHub Repository](https://github.com/supernovae-st/nika)
- [SuperNovae Studio](https://supernovae.studio)
