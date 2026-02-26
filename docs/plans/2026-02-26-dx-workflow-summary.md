# Nika DX Workflow Summary

**Date:** 2026-02-26
**Version:** v0.12.0
**Status:** Production Ready

## Overview

This document summarizes the Developer Experience (DX) workflow established for Nika, including the ARMADA quality system, Claude Code integration, and automated shipping.

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  NIKA DX ARCHITECTURE (v0.12.0)                                                 │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐    ┌──────────────┐  │
│  │   Claude     │    │    Skills    │    │    Hooks     │    │   ARMADA     │  │
│  │   Code       │───▶│   (11)       │───▶│   (18)       │───▶│   CI         │  │
│  └──────────────┘    └──────────────┘    └──────────────┘    └──────────────┘  │
│         │                   │                   │                   │          │
│         ▼                   ▼                   ▼                   ▼          │
│  .claude/             skills/              hooks/             workflows/       │
│  settings.json        INDEX.md             compact            armada.yml       │
│                       ship/                git-safety         release.yml      │
│                       armada/              rust-detect        version-lock.yml │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

## Quick Commands

| Command | Description |
|---------|-------------|
| `/ship` | Auto-ship changes: branch → commit → push → PR → CI → merge |
| `/armada` | Show ARMADA status and quality gates |
| `/armada check` | Run all 10 quality stations locally |
| `/fortress` | Alias for `/armada` (version lock focus) |

## The /ship Workflow

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  🚀 /ship — ONE-COMMAND SHIPPING                                              ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  Step 1: Detect changes (git status)                                          ║
║  Step 2: Create branch if on main (feat/ship-<timestamp>)                     ║
║  Step 3: Stage and commit (conventional commit format)                        ║
║  Step 4: Push to origin                                                       ║
║  Step 5: Create PR via gh                                                     ║
║  Step 6: Wait for CI (optional --wait)                                        ║
║  Step 7: Enable auto-merge (squash)                                           ║
║  Step 8: Cleanup local branch after merge                                     ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

## ARMADA 10-Station Quality System

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  🏴‍☠️ ARMADA — 10 QUALITY STATIONS                                             ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║   Station 1: 🔧 Format         cargo fmt --check                              ║
║   Station 2: 📎 Lint           cargo clippy -- -D warnings                    ║
║   Station 3: 🧪 Tests          cargo nextest run (2,997 tests)                ║
║   Station 4: 📊 Coverage       cargo llvm-cov (>70%)                          ║
║   Station 5: 📖 Docs           cargo doc --no-deps                            ║
║   Station 6: 🔒 Security       cargo audit + cargo deny                       ║
║   Station 7: 🤖 CodeRabbit     AI review (general patterns)                   ║
║   Station 8: 🧠 Claude AI      AI review (Nika-specific) [placeholder]        ║
║   Station 9: 📝 Conventional   Commit message validation                      ║
║   Station 10: ⚓ Version Lock  0.x.x enforcement (NEVER 1.0.0)                ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

## Claude Code Configuration

### Skills (11 total)

| Skill | Trigger | Purpose |
|-------|---------|---------|
| ship | `/ship` | Auto-ship changes |
| armada | `/armada` | Quality gates |
| nika-yaml | `/nika-yaml` | YAML authoring guide |
| nika-arch | `/nika-arch` | Architecture diagram |
| nika-run | `/nika-run` | Run workflows |
| nika-diagnose | `/nika-diagnose` | Systematic diagnosis |
| nika-debug | `/nika-debug` | Debug with traces |
| nika-binding | `/nika-binding` | Data binding syntax |
| workflow-validate | `/workflow-validate` | Validate YAML/DAG |
| nika-spec | `/nika-spec` | Specification reference |

### Hooks (18 active)

**PreToolUse hooks:**
- `session-start:compact` — Session initialization
- `git-safety:check` — Branch protection
- `rust-project-detected` — Cargo project detection
- `version-lock:enforce` — Block v1.x.x changes

**PostToolUse hooks:**
- `test-verification` — Verify test results
- `cargo-toml:version-check` — Validate version changes

### Rules (27 total)

Key rule files in `.claude/rules/`:
- `testing.md` — TDD workflow
- `error-handling.md` — NikaError patterns
- `PERFORMANCE.md` — TUI optimization rules
- `adr/*.md` — Architecture Decision Records (6 ADRs)

## Version Lock (Captain's Orders)

**Nika will NEVER be version 1.0.0.** Enforced at:

1. **Rust tests** — `tests/version_lock_test.rs`
2. **CI workflow** — Station 10 in `armada-checkpoints.yml`
3. **Claude hooks** — `PreToolUse` blocks v1.x pushes
4. **release-plz** — Configured for 0.x.x SemVer

## File Locations

```
nika/
├── .claude/
│   ├── settings.json        # Hooks configuration
│   ├── .nika-status         # Health status (v0.12.0)
│   └── skills/
│       ├── INDEX.md         # Skills inventory
│       ├── ship/            # /ship skill
│       ├── armada/          # /armada skill
│       └── nika-*/          # Workflow skills
├── .github/
│   └── workflows/
│       ├── armada-checkpoints.yml  # 10-station CI
│       ├── release-plz.yml         # Release automation
│       └── version-lock.yml        # Version enforcement
├── tools/nika/
│   ├── CLAUDE.md            # Claude Code context
│   └── Cargo.toml           # Version source of truth
└── docs/
    └── plans/
        └── 2026-02-26-dx-workflow-summary.md  # This file
```

## Daily Workflow

### Starting Work

```bash
# 1. Check current status
git status
cargo test --lib  # Quick test run

# 2. Make changes
# ... edit files ...

# 3. Ship when ready
/ship  # One command does everything!
```

### Before Major Changes

```bash
# Run full ARMADA check locally
/armada check

# Or manually:
cargo fmt --check
cargo clippy -- -D warnings
cargo nextest run --all-features
cargo doc --no-deps
```

### Troubleshooting CI

If ARMADA fails:

1. Check which station failed in GitHub Actions
2. Fix the issue locally
3. Run `/ship` again (creates new commit on same PR)
4. CI re-runs automatically

## Metrics

| Metric | Value |
|--------|-------|
| Version | v0.12.0 |
| Tests | 2,997 passing |
| Coverage | ~70% |
| Clippy warnings | 0 |
| Skills | 11 |
| Hooks | 18 |
| Rules | 27 |
| ARMADA stations | 10 |

## References

- **CLAUDE.md** — `tools/nika/CLAUDE.md`
- **Skills INDEX** — `.claude/skills/INDEX.md`
- **ARMADA Design** — `docs/plans/2025-02-25-nika-fortress-design.md`
- **CI Workflows** — `.github/workflows/`
