# Migration Guide: `nika pkg` → `spn` CLI

**Version:** v0.15.3 → v0.16.0
**Status:** Deprecation in v0.15.3, Removal in v0.16.0

---

## Overview

Starting with Nika v0.15.3, all `nika pkg` commands are **deprecated** in favor of
the new standalone `spn` CLI (SuperNovae Package Manager). This change improves:

- **Separation of concerns**: Package management is decoupled from workflow execution
- **Unified tooling**: One CLI for all SuperNovae ecosystem packages (schemas, skills, workflows, MCP servers)
- **Better DX**: Dedicated tool with focused functionality

---

## Command Migration Reference

| Old Command | New Command | Notes |
|-------------|-------------|-------|
| `nika pkg install @scope/name` | `spn install @scope/name` | Identical syntax |
| `nika pkg install @scope/name@1.2.0` | `spn install @scope/name@1.2.0` | Version pinning works the same |
| `nika pkg list` | `spn list` | Lists installed packages |
| `nika pkg list --registry` | `spn list --registry` | Shows registry packages |
| `nika pkg search <query>` | `spn search <query>` | Search functionality preserved |
| `nika pkg update` | `spn update` | Updates all packages |
| `nika pkg update @scope/name` | `spn update @scope/name` | Update specific package |
| `nika pkg remove @scope/name` | `spn remove @scope/name` | Remove installed package |

---

## Installation

### Install `spn` CLI

```bash
# Via cargo (recommended)
cargo install --git https://github.com/supernovae-st/supernovae-cli

# Or clone and build locally
git clone https://github.com/supernovae-st/supernovae-cli
cd supernovae-cli
cargo install --path .
```

### Verify Installation

```bash
spn --version
# spn 0.2.0

spn --help
# Shows all available commands
```

---

## Migration Steps

### Step 1: Install `spn` CLI

Follow the installation instructions above.

### Step 2: Verify Package Directory

Both `nika pkg` and `spn` use the same package directory:

```
~/.spn/packages/
├── @scope/
│   └── package-name/
│       └── 1.0.0/
│           ├── manifest.yaml
│           └── ...
└── registry.yaml
```

No migration of packages is needed - `spn` reads the same directory structure.

### Step 3: Update Your Scripts

Replace `nika pkg` with `spn` in your scripts:

```bash
# Before
nika pkg install @supernovae/nika-core-skills@1.0.0
nika pkg list

# After
spn install @supernovae/nika-core-skills@1.0.0
spn list
```

### Step 4: Update CI/CD Pipelines

If you have CI/CD pipelines using `nika pkg`:

```yaml
# Before (GitHub Actions)
- name: Install packages
  run: nika pkg install @supernovae/workflows

# After
- name: Install spn CLI
  run: cargo install --git https://github.com/supernovae-st/supernovae-cli

- name: Install packages
  run: spn install @supernovae/workflows
```

---

## New Features in `spn`

The `spn` CLI includes additional features not available in `nika pkg`:

### Interactive Help System

```bash
spn topic install    # Detailed help for install command
spn topic workflow   # Workflow authoring guide
spn topic providers  # LLM provider configuration
```

### Package Initialization

```bash
spn init             # Initialize a new package
spn init --skill     # Initialize a skill package
spn init --workflow  # Initialize a workflow package
```

### MCP Server Management

```bash
spn mcp list         # List MCP servers
spn mcp start        # Start MCP servers
spn mcp status       # Check server status
```

---

## Timeline

| Version | Date | Status |
|---------|------|--------|
| v0.15.3 | March 2026 | Deprecation warnings added |
| v0.16.0 | April 2026 | `nika pkg` module removed |

---

## Troubleshooting

### "Command not found: spn"

Ensure `~/.cargo/bin` is in your PATH:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

### "Package directory not found"

Initialize the package registry:

```bash
spn init --registry
# Creates ~/.spn/packages/registry.yaml
```

### "Version conflict"

Both `nika pkg` and `spn` use semantic versioning. If you have conflicts:

```bash
spn remove @scope/name
spn install @scope/name@specific-version
```

---

## Questions?

- **Documentation**: https://github.com/supernovae-st/supernovae-cli
- **Issues**: https://github.com/supernovae-st/supernovae-cli/issues
- **Nika Docs**: https://github.com/supernovae-st/nika
