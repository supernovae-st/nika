# Phase 5: Interop & Final Integration

## Overview

**Goal**: Deprecate spn CLI and finalize nika as the unified tool.
**Lines**: ~200
**Types**: 2
**Tests**: 4

---

## Deprecation Strategy

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  SPN DEPRECATION TIMELINE                                                      ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  v0.27.0   spn shows deprecation warning, suggests nika equivalents           ║
║     │                                                                         ║
║     │      $ spn provider list                                                ║
║     │      ⚠️  spn is deprecated. Use: nika provider list                     ║
║     │      [output continues normally]                                        ║
║     │                                                                         ║
║  v0.28.0   spn removed from release, only nika published                      ║
║     │                                                                         ║
║     │      $ spn                                                              ║
║     │      ❌ spn has been removed. Please use: nika                          ║
║     │                                                                         ║
║  v0.29.0   spn crate archived (security fixes only)                           ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

---

## Command Mapping

| spn Command | nika Equivalent | Notes |
|-------------|-----------------|-------|
| `spn provider list` | `nika provider list` | Identical |
| `spn provider set <p>` | `nika provider set <p>` | Identical |
| `spn provider get <p>` | `nika provider get <p>` | Identical |
| `spn provider test <p>` | `nika provider test <p>` | Identical |
| `spn provider migrate` | `nika provider migrate` | Identical |
| `spn model list` | `nika model list` | Identical |
| `spn model pull <m>` | `nika model pull <m>` | Identical |
| `spn mcp add <s>` | `nika mcp add <s>` | Identical |
| `spn mcp remove <s>` | `nika mcp remove <s>` | Identical |
| `spn mcp list` | `nika mcp list` | Identical |
| `spn mcp test <s>` | `nika mcp test <s>` | Identical |
| `spn sync` | `nika sync` | Identical |
| `spn setup` | `nika setup` | Identical |
| `spn daemon start` | `nika daemon start` | Identical |
| `spn daemon stop` | `nika daemon stop` | Identical |
| `spn daemon status` | `nika daemon status` | Identical |
| `spn doctor` | `nika doctor` | 🆕 Added |
| `spn nk <args>` | `nika <args>` | No longer needed |
| `spn nv <args>` | `novanet <args>` | Separate tool |

---

## Deprecation Warning Implementation

### In spn main.rs

```rust
// supernovae-cli/crates/spn/src/main.rs

fn main() {
    // Show deprecation warning for ALL commands
    show_deprecation_warning();

    // Continue with normal execution
    let result = run();

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

fn show_deprecation_warning() {
    use std::io::{stderr, Write};
    use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

    let mut stderr = StandardStream::stderr(ColorChoice::Auto);

    // Yellow warning color
    stderr.set_color(ColorSpec::new().set_fg(Some(Color::Yellow))).ok();
    write!(stderr, "⚠️  ").ok();
    stderr.reset().ok();

    writeln!(
        stderr,
        "spn is deprecated and will be removed in v0.28.0. Use 'nika' instead."
    ).ok();

    // Show equivalent command
    let args: Vec<String> = std::env::args().skip(1).collect();
    if !args.is_empty() {
        let nika_cmd = format!("nika {}", args.join(" "));
        writeln!(stderr, "   Equivalent: {}", nika_cmd).ok();
    }

    writeln!(stderr).ok();
}
```

### Mapping Helper

```rust
// supernovae-cli/crates/spn/src/deprecation.rs

/// Map spn command to nika equivalent
pub fn map_to_nika(args: &[String]) -> String {
    if args.is_empty() {
        return "nika".to_string();
    }

    let cmd = &args[0];
    let rest = args[1..].join(" ");

    match cmd.as_str() {
        "nk" => format!("nika {}", rest),
        "nv" => format!("novanet {}", rest),
        _ => format!("nika {} {}", cmd, rest).trim().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_map_provider() {
        let args = vec!["provider".into(), "list".into()];
        assert_eq!(map_to_nika(&args), "nika provider list");
    }

    #[test]
    fn test_map_nk_shortcut() {
        let args = vec!["nk".into(), "chat".into()];
        assert_eq!(map_to_nika(&args), "nika chat");
    }

    #[test]
    fn test_map_empty() {
        let args: Vec<String> = vec![];
        assert_eq!(map_to_nika(&args), "nika");
    }
}
```

---

## spn-client Library Maintenance

The `spn-client` crate remains as a library for backward compatibility:

```
supernovae-cli/
├── crates/
│   ├── spn-client/        # KEEP: Library for IPC
│   │   ├── Cargo.toml     # Version 0.2.x (maintenance mode)
│   │   └── src/
│   │       ├── lib.rs     # DaemonClient, SpnPaths
│   │       └── protocol.rs # IPC message types
│   │
│   ├── spn-core/          # KEEP: Zero-dep types
│   │   └── ...
│   │
│   ├── spn-keyring/       # KEEP: Keychain wrapper
│   │   └── ...
│   │
│   └── spn/               # DEPRECATE: CLI binary
│       └── ...
```

### Update Cargo.toml

```toml
# spn-client/Cargo.toml
[package]
name = "spn-client"
version = "0.2.3"
description = "IPC client for nika daemon (formerly spn daemon)"
# Note: This library is in maintenance mode. New features go in nika.
```

---

## nika Daemon Socket Path

During transition, nika daemon uses the same socket as spn:

```rust
// nika/tools/nika/src/daemon/paths.rs

/// Daemon socket path.
///
/// Uses ~/.spn/daemon.sock for backward compatibility with spn-client.
/// Will migrate to ~/.nika/daemon.sock in v0.29.0.
pub fn socket_path() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".spn").join("daemon.sock"))
        .unwrap_or_else(|| PathBuf::from("/tmp/nika-daemon.sock"))
}

/// Future socket path (v0.29.0+)
pub fn socket_path_v29() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".nika").join("daemon.sock"))
        .unwrap_or_else(|| PathBuf::from("/tmp/nika-daemon.sock"))
}
```

---

## Migration Guide (docs/guides/spn-to-nika.md)

```markdown
# Migrating from spn to nika

## Why the Change?

`spn` (SuperNovae Package Manager) and `nika` (workflow engine) have been
unified into a single tool. This simplifies the user experience:

- **One tool** instead of two
- **Unified configuration** (nika.yaml instead of spn.yaml)
- **Single daemon** (nika daemon instead of spn daemon)

## Quick Migration

### 1. Update Your Shell Aliases

```bash
# Old
alias spn='spn'

# New
alias spn='nika'  # If you want backward compatibility
```

### 2. Update Configuration Files

```bash
# Move config
mv ~/.spn/config.yaml ~/.nika/config.yaml

# Update project files
mv ./spn.yaml ./nika.yaml  # Or merge into existing nika.yaml
```

### 3. Restart Daemon

```bash
# Stop old daemon
spn daemon stop

# Start new daemon
nika daemon start
```

### 4. Update Scripts

Replace all `spn` calls with `nika`:

```bash
# Old
spn provider list
spn mcp add neo4j

# New
nika provider list
nika mcp add neo4j
```

## Command Reference

| Old (spn) | New (nika) |
|-----------|------------|
| `spn provider list` | `nika provider list` |
| `spn model pull <m>` | `nika model pull <m>` |
| `spn mcp add <s>` | `nika mcp add <s>` |
| `spn sync` | `nika sync` |
| `spn setup` | `nika setup` |
| `spn daemon start` | `nika daemon start` |
| `spn nk <args>` | `nika <args>` |

## Troubleshooting

### "spn: command not found"

The `spn` binary is no longer installed. Use `nika` instead.

### "Connection refused" from daemon

The daemon socket location changed. Restart with `nika daemon start`.

### MCP servers not connecting

Check your config file location: `~/.nika/config.yaml` or `./nika.yaml`.
```

---

## Release Checklist

### v0.27.0 Release

- [ ] Add deprecation warning to spn
- [ ] Add mapping helper for commands
- [ ] Document migration guide
- [ ] Update README files
- [ ] Announce deprecation in CHANGELOG

### v0.28.0 Release

- [ ] Remove spn binary from release artifacts
- [ ] Keep spn crate for `cargo install` (shows error)
- [ ] Update installation docs
- [ ] Archive spn in Cargo.toml

### v0.29.0 Release

- [ ] Migrate socket path to ~/.nika/
- [ ] Archive spn-client (security fixes only)
- [ ] Final cleanup

---

## TDD Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deprecation_message_shown() {
        // Capture stderr
        let output = std::process::Command::new("spn")
            .arg("--help")
            .output()
            .expect("failed to run spn");

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("deprecated"));
        assert!(stderr.contains("nika"));
    }

    #[test]
    fn test_command_mapping() {
        assert_eq!(
            map_to_nika(&["provider".into(), "list".into()]),
            "nika provider list"
        );
        assert_eq!(
            map_to_nika(&["nk".into(), "studio".into()]),
            "nika studio"
        );
    }

    #[test]
    fn test_socket_path_compatibility() {
        let path = socket_path();
        assert!(path.to_string_lossy().contains(".spn"));
    }

    #[test]
    fn test_nika_commands_work() {
        // Verify all migrated commands work
        let commands = [
            vec!["provider", "list"],
            vec!["model", "list"],
            vec!["mcp", "list"],
            vec!["config", "files"],
        ];

        for cmd in commands {
            let output = std::process::Command::new("nika")
                .args(&cmd)
                .output()
                .expect("failed to run nika");

            assert!(
                output.status.success(),
                "nika {} failed: {}",
                cmd.join(" "),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
```

---

## Estimated Effort

| Task | Hours |
|------|-------|
| Deprecation warning | 0.5 |
| Command mapping | 0.5 |
| Migration guide | 1 |
| Socket path handling | 0.5 |
| Tests | 1 |
| Documentation | 1 |
| **Total** | **~4.5 hours** |

---

## Validation Checklist

- [ ] spn shows deprecation warning
- [ ] spn commands still work
- [ ] nika has all spn functionality
- [ ] Migration guide is accurate
- [ ] Socket path compatible
- [ ] Tests pass
- [ ] CHANGELOG updated
- [ ] README updated
