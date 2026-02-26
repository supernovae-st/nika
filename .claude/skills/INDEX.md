# Nika Skills Index

**Nika Version:** v0.9.0 | **5 verbs** | Full streaming + VS Code TUI | rig-core v0.31 | 6 LLM providers

## Available Skills

| Skill | Command | Purpose |
|-------|---------|---------|
| **ship** | `/ship` | Auto-ship changes: branch → commit → PR → merge (ARMADA workflow) |
| **armada** | `/armada` | 10-station quality gates, releases, version lock (0.x.x forever) |
| **nika-yaml** | `/nika-yaml` | Complete YAML authoring guide (verbs, for_each, bindings) |
| **nika-arch** | `/nika-arch` | Architecture diagram and module structure |
| **nika-run** | `/nika-run` | Run workflows with validation |
| **nika-diagnose** | `/nika-diagnose` | Systematic workflow diagnosis |
| **nika-debug** | `/nika-debug` | Debug with traces and logging |
| **nika-binding** | `/nika-binding` | Data binding syntax reference |
| **workflow-validate** | `/workflow-validate` | Validate YAML syntax and DAG |
| **nika-spec** | `/nika-spec` | Workflow specification reference |

## ARMADA Quality System

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  🏴‍☠️ ARMADA — 10 QUALITY STATIONS                                             ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║   Station 1: 🔧 Format       | Station 6: 🔒 Security                          ║
║   Station 2: 📎 Lint         | Station 7: 🤖 CodeRabbit                        ║
║   Station 3: 🧪 Tests        | Station 8: 🧠 Claude AI                         ║
║   Station 4: 📊 Coverage     | Station 9: 📝 Conventional                      ║
║   Station 5: 📖 Docs         | Station 10: ⚓ Version Lock                     ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### ARMADA Commands
- `/armada` — Show status and available commands
- `/armada check` — Run all 10 stations locally
- `/armada release` — Prepare release with full validation
- `/armada worktree <name>` — Create isolated git worktree

### Captain's Orders
**Nika will NEVER be version 1.0.0.** Version lock enforced at:
- Rust tests (`tests/version_lock_test.rs`)
- CI workflow (`.github/workflows/version-lock.yml`)
- Claude hooks (`.claude/settings.json`)
- release-plz (`release-plz.toml`)

## v0.7.x Features

Features in v0.7.x (MVP 8 complete + TUI navigation refresh):

| Feature | Documentation |
|---------|--------------|
| `decompose:` modifier | `CLAUDE.md` -> "Decompose Modifier" |
| Lazy bindings | `CLAUDE.md` -> "Lazy Bindings" |
| `spawn_agent` tool | `CLAUDE.md` -> "Nested Agents" |
| Shorthand syntax | `CLAUDE.md` -> "Verb Shorthand Syntax" |
| Event sourcing | `CLAUDE.md` -> "Event Sourcing" |

## v0.3-0.4 Features (Reference)

Features from earlier versions:

| Feature | Documentation |
|---------|--------------|
| `invoke:` verb (MCP) | `CLAUDE.md` -> "MCP Integration" |
| `agent:` verb (agentic loop) | `CLAUDE.md` -> "rig-core Integration" |
| `for_each:` parallelism | `CLAUDE.md` -> "for_each Parallelism" |
| Token tracking fix | `CLAUDE.md` -> "v0.4.1 Changes" |

## Quick Reference

### CLI Commands (v0.9.0)

```bash
# TUI Home view (default)
nika

# TUI Chat mode
nika chat

# TUI Studio editor
nika studio

# Run workflow directly
nika workflow.nika.yaml

# Explicit run command
nika run workflow.nika.yaml

# Validate without executing
nika check workflow.nika.yaml

# Initialize project
nika init
```

### Debugging & Observation

```bash
# Verbose logging with run
RUST_LOG=debug nika run workflow.nika.yaml

# View event trace (NDJSON)
cat .nika/trace.ndjson | jq .

# Check syntax and DAG
nika check workflow.nika.yaml
```

## Skill Categories

### Shipping & Release
- `/ship` — Auto-ship changes (branch → PR → merge)
- `/armada` — 10-station quality system
- `/armada check` — Run all stations locally
- `/armada release` — Prepare release with validation
- `/armada worktree` — Create isolated git worktree

### Development
- `/nika-arch` — Understand the codebase
- `/nika-binding` — Data flow between tasks

### Execution
- `/nika-run` — Run workflows
- `/workflow-validate` — Validate before running

### Debugging
- `/nika-diagnose` — Systematic checklist
- `/nika-debug` — Traces and logging

### Reference
- `/nika-spec` — Full workflow specification
