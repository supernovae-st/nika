# Changelog

All notable changes to Nika are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Nika Diamond is a ground-up rewrite on an orphan branch (`nika-diamond`).
Legacy main sits at v0.79.3. Diamond starts at v0.80.0, targeting v0.90.0.
No public releases yet -- internal tracking only.

## [Unreleased]

### Added

- **nika-catalog** -- L0 static catalogs crate admitted to workspace (55a451695).
  Hybrid lookup: phf + unicase for providers/MCP (O(1)), sorted arrays for
  builtins/transforms (O(log n)). 16 providers, 113 MCP aliases, 63 builtins,
  65 transforms, 61 pricing entries. 2,235 LOC, 85 tests, 94.7% mutation killed.
- **nika-error** -- L0 error infrastructure crate admitted to workspace (42909b1c7).
  Strategy C+: trait `NikaErrorCode` + `NikaError(Box<dyn>)` + `CoreError`.
  `NikaCode` dual wire format (Display `"NIKA-001"`, serde roundtrip).
  1,013 LOC, 44 tests, 100% mutation killed.

## [0.80.0-alpha.0] - 2026-04-13

Workspace scaffold. Orphan branch `nika-diamond` created from scratch --
no code inherited from main.

### Added

- Orphan branch initialized with workspace Cargo.toml (edition 2024, Rust 1.91).
- Workspace-level lint policy: `clippy::unwrap_used = "deny"`,
  `clippy::expect_used = "warn"`, `clippy::panic = "deny"`.
- `.gitignore` excluding all 32 legacy crate directories and tool output.
- `DIAMOND.md` architectural vision with complete 34-crate catalog.
- `README.md` rewritten for Phase 1 state.
- Claude Code environment, DX rules, session discipline, and hooks.
- Linear setup guide for diamond project tracking.

[Unreleased]: https://github.com/supernovae-st/nika/compare/v0.80.0-alpha.0...HEAD
[0.80.0-alpha.0]: https://github.com/supernovae-st/nika/commits/v0.80.0-alpha.0
