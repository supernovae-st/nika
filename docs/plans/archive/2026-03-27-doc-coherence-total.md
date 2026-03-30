# Plan: Documentation Total Coherence — v0.49.0

**Date**: 2026-03-27
**Scope**: All docs, dx skills/rules/commands, README, explanations (NO code changes)
**Goal**: Every file in the repo accurately reflects Nika v0.49.0 reality

---

## The Single Source of Truth

Every doc must agree on these facts:

### Workspace (12 crates)
| Crate | Size | Role |
|-------|------|------|
| `nika` | ~2k | CLI binary |
| `nika-engine` | ~135k | Execution engine (embeddable) |
| `nika-daemon` | ~5k | Background daemon — IS A CRATE, not a feature flag |
| `nika-init` | ~21k | Project scaffolding + course |
| `nika-core` | ~23k | AST, types, catalogs — zero I/O |
| `nika-event` | ~4k | EventLog, TraceWriter |
| `nika-mcp` | ~9k | MCP client, rmcp |
| `nika-media` | ~13k | CAS store, processor |
| `nika-cli` | ~8k | CLI subcommands |
| `nika-tui` | ~86k | Terminal UI (ratatui) |
| `nika-lsp-core` | ~9k | LSP intelligence |
| `nika-lsp` | ~2.5k | LSP binary |

### TUI: 3 Views (NOT 4)
```
1/s  Studio   — Workflow browser + YAML editor + DAG preview
2/c  Command  — Execution monitor + chat
3/x  Control  — Provider config + theme + preferences
```

### Providers (9 total: 7 cloud + native + mock)
| Provider | Env Var | Default Model |
|----------|---------|---------------|
| `anthropic` | `ANTHROPIC_API_KEY` | `claude-sonnet-4-20250514` |
| `openai` | `OPENAI_API_KEY` | `gpt-4o` |
| `mistral` | `MISTRAL_API_KEY` | `mistral-large-latest` |
| `groq` | `GROQ_API_KEY` | `llama-4-maverick` ← NOT llama-3.3-70b |
| `deepseek` | `DEEPSEEK_API_KEY` | `deepseek-chat` |
| `gemini` | `GEMINI_API_KEY` ← NOT GOOGLE_API_KEY | `gemini-2.5-flash` ← NOT 2.0 |
| `xai` | `XAI_API_KEY` | `grok-3` |
| `native` | — | Local GGUF via mistral.rs |
| `mock` | — | Testing only |

### Workflow Syntax (must be consistent everywhere)
```yaml
schema: "nika/workflow@0.12"   # always
# ...
tasks:
  - id: step1
    infer:
      prompt: "..."

  - id: step2
    depends_on: [step1]        # ALWAYS array
    with:
      data: $step1             # $ prefix REQUIRED
    infer:
      prompt: "{{with.data}}"  # always with. prefix

  - id: loop
    for_each:                  # NESTED object form
      items: "{{with.data}}"
      as: item
      concurrency: 3
    infer: "{{with.item}}"     # loop var uses with. prefix too

# MCP env vars: $VAR_NAME (NOT ${VAR}, NOT "{{$env.VAR}}")
# MCP config: mcp: server_name: (NO servers: wrapper)
# Timeout: 30 = 30 seconds (NOT milliseconds)
# Transforms: upper, lower (NOT uppercase, lowercase)
# Extension: .nika.yaml (NOT .yaml)
```

### CLI Commands (real ones only)
```bash
nika check workflow.nika.yaml          # validate
nika run workflow.nika.yaml            # run (auto-detects live/classic)
nika run workflow.nika.yaml --dry-run  # validate without executing
nika run workflow.nika.yaml --no-live  # force classic output
nika ui                                # TUI
nika trace list/show/export            # trace (singular, NOT traces)
nika provider list                     # API key status
nika init                              # interactive wizard
nika init --course                     # generate 12-level course
nika course status/next/check/hint/run/info/reset/watch
nika showcase list/extract <name>      # 115 showcase workflows
nika model                             # model management
nika daemon                            # daemon management
nika lsp                               # LSP server
```

### Phantom Commands (REMOVE from all docs)
- `nika setup` — removed in v0.39 DX refactor
- `nika init --minimal` — never existed
- `nika traces` — correct is `nika trace` (singular)
- `nika chat` — never existed (use TUI Command view)
- `nika studio` — never existed (use TUI Studio view)
- `nika new` / `nika workflow` — never existed
- `cargo run --` — never use in docs, use `nika` commands

### Testing
```bash
cargo nextest run --lib          # CORRECT — avoids Keychain popup
cargo nextest run --all-features # WRONG — triggers Keychain popup
# Test count: 8,300+ (NOT 6846, NOT 2220)
```

### Source Paths (crate-qualified, not bare src/)
```
nika-core/src/catalogs/          # providers, models, mcp_aliases
nika-core/src/ast/raw/parser.rs  # YAML → Raw AST
nika-engine/src/runtime/runner.rs
nika-engine/src/runtime/executor/
nika-engine/src/runtime/rig_agent_loop/
nika-mcp/src/client.rs
```

### Counts & Versions
- Schema version: `0.12` (workflow@0.12)
- Showcase workflows: **115** (NOT 200+, 65, 55+)
- Course levels: **12** (NOT 10, 11)
- Course level names: Jailbreak, Hot Wire, Fork Bomb, Root Access, Shapeshifter, Pay-Per-Dream, Swiss Knife, Gone Rogue, Data Heist, Open Protocol, Pixel Pirate, SuperNovae
- Builtin tools: **24** (5 always-on + 6 media-core + 13 opt-in)
- Error code ranges: 000-009, 010-019, 020-029, 030-039, 040-049, 050-059, 060-069, 070-089, 090-099, 100-109, 110-119, 120-129, 130-139, 140-151, 160-164, 170-179, 200-214, 215-219, 250, 251-259, 260-269, 270-279, 280-285, 290-297, 300-309, 310-319

### Error Handling (Nika-specific rule)
```rust
// CORRECT in Nika crates:
use nika_engine::NikaError;  // NikaError with NIKA-XXX codes

// WRONG in Nika crates:
use anyhow::Error;           // never anyhow in libraries
// color-eyre ONLY in nika binary main.rs
```

---

## File Audit Scope (30 agents)

### Batch A: dx/.claude/ (6 agents)
| Agent | Files |
|-------|-------|
| A1 | `skills/nika/nika-bug-hunter/SKILL.md` + `skills/nika/nika-deep-audit/SKILL.md` |
| A2 | `skills/novanet/novanet-terminology.md` + `rules/novanet.md` + `rules/novanet-terminology.md` |
| A3 | `skills/shared/novanet-architecture.md` + `skills/shared/novanet-mcp.md` + `skills/shared/novanet-tui.md` |
| A4 | `skills/shared/mcp-tool-selection.md` + `skills/shared/novanet-sync.md` + `skills/shared/security-audit.md` |
| A5 | `commands/novanet-arch.md` + `commands/novanet-sync.md` + `commands/schema-add-node.md` + `commands/schema-edit-node.md` + `commands/schema.md` |
| A6 | `rules/arc-design-guide.md` + `rules/mcp-tool-selection.md` + `rules/git-workflow.md` |

### Batch B: docs/content/technical/ (4 agents)
| Agent | Files |
|-------|-------|
| B1 | `tui-architecture.md` + `event-system.md` + `ast-pipeline.md` + `ast-three-phase-pipeline.md` |
| B2 | `runtime-architecture.md` + `dag-execution-model.md` + `binding-template-system.md` |
| B3 | `five-verbs-deep-dive.md` + `yaml-schema-reference.md` + `configuration-reference.md` |
| B4 | `mcp-integration.md` + `media-cas-architecture.md` + `lsp-architecture.md` + `design-decisions.md` + `error-codes-reference.md` |

### Batch C: content-suite/01-technical-bible/ (3 agents)
| Agent | Files |
|-------|-------|
| C1 | `03-yaml-schema-reference.md` + `04-five-verbs-deep-dive.md` + `05-ast-pipeline.md` |
| C2 | `06-provider-system.md` + `07-error-codes-reference.md` + `08-media-pipeline.md` + `09-binding-template-system.md` + `10-security-model.md` |
| C3 | `12-cli-commands-reference.md` — MAJOR REWRITE (phantom commands everywhere) |

### Batch D: content-suite/02-architecture-deep-dive/ (3 agents)
| Agent | Files |
|-------|-------|
| D1 | `01-crate-dependency-graph.md` + `02-ast-three-phase-pipeline.md` + `03-dag-execution-model.md` + `04-runtime-architecture.md` |
| D2 | `05-provider-abstraction.md` + `06-mcp-integration.md` + `08-event-system.md` + `09-media-cas-architecture.md` + `10-lsp-architecture.md` |
| D3 | `07-tui-architecture.md` (MAJOR REWRITE) + `11-security-architecture.md` + `12-design-decisions.md` |

### Batch E: content-suite/03-user-guide/ (4 agents)
| Agent | Files |
|-------|-------|
| E1 | `02-your-first-workflow.md` + `04-workflow-patterns.md` |
| E2 | `05-infer-verb-guide.md` + `06-fetch-verb-guide.md` + `07-exec-invoke-agent-guide.md` |
| E3 | `09-tui-guide.md` — MAJOR REWRITE (entire 4-view model must become 3-view) |
| E4 | `08-media-pipeline-guide.md` + `10-course-guide.md` + `11-showcase-guide.md` + `12-troubleshooting.md` + `13-faq.md` |

### Batch F: content-suite/04-workflow-cookbook/ (2 agents)
| Agent | Files |
|-------|-------|
| F1 | `01-04` cookbook files |
| F2 | `05-09` cookbook files |

### Batch G: Remaining content-suite (4 agents)
| Agent | Files |
|-------|-------|
| G1 | `09-developer-learning/` 01 + 03 + 04 + 05 |
| G2 | `09-developer-learning/` 06 + 07 + 08 + 10 |
| G3 | `08-marketing-kit/` 01 + 03 + 04 + 06 |
| G4 | `08-marketing-kit/` 07 + 08 + 09 + 10 |

### Batch H: docs/content/user-guide + cookbook (4 agents)
| Agent | Files |
|-------|-------|
| H1 | `content/user-guide/` 01-06 |
| H2 | `content/user-guide/` 07-13 |
| H3 | `content/cookbook/` 01-06 |
| H4 | `content/cookbook/` 07-11 + `content/learning/` cross-cutting |

---

## Common Error Patterns to Fix

### Category 1: Wrong workflow syntax
- `use: { data: step1 }` → `with: { data: $step1 }`
- `{{data}}` → `{{with.data}}`
- `{{item}}` in for_each → `{{with.item}}`
- `timeout: 30000` → `timeout: 30`
- `retry: 3` → `retry: { max_attempts: 3, delay_ms: 2000 }`
- `depends_on: step1` → `depends_on: [step1]`

### Category 2: Wrong provider info
- `llama-3.3-70b` / `llama-3-70b` → `llama-4-maverick` (groq default)
- `gemini-2.0-flash` / `gemini-pro` → `gemini-2.5-flash` (gemini default)
- `GOOGLE_API_KEY` → `GEMINI_API_KEY`
- `claude-sonnet-4-6` / `claude-3-opus` → proper date-suffixed names
- `22 providers` / `7 providers` → `9 total (7 cloud + native + mock)` or `8 usable`
- Fabricated aliases (chatgpt, mistralai, groqcloud, google-ai) → remove

### Category 3: Wrong architecture facts
- `10 crates` → `12 crates`
- Old crate split version refs → v0.49.0
- nika-daemon feature flag → nika-daemon crate
- `4-view TUI` → `3-view TUI`
- `nika-tui: 92k` → `nika-tui: ~86k`
- `nika-core: 30k` → `nika-core: ~23k`

### Category 4: Phantom commands
- Any reference to `nika setup` → remove
- `nika init --minimal` → remove --minimal flag
- `nika traces` → `nika trace`
- `nika chat` / `nika studio` / `nika new` / `nika workflow` → remove

### Category 5: Wrong counts
- Showcase `200+` / `65` / `55+` → `115`
- Tests `6846` / `2220` → `8,300+`
- Course levels `10` / `11` → `12`

### Category 6: Wrong transforms
- `uppercase` / `lowercase` → `upper` / `lower`

### Category 7: Wrong CLI tool references
- `cargo run -- check` → `nika check`
- `cargo run -- run` → `nika run`
- `cargo run -- tui` → `nika ui`
- `cargo nextest run --all-features` → `cargo nextest run --lib`
- `.yaml` extension on workflows → `.nika.yaml`

### Category 8: Wrong source paths
- `src/core/` → `nika-core/src/catalogs/`
- bare `src/` paths for crate-qualified paths

---

## NovaNet-specific Rules
- Schema v0.24.0 (47 nodes, 153 arcs)
- Arc: `TARGETS_KEYWORD` in `mining` family (NOT `TARGETS`)
- Terminal icon: `"◆"` (NOT `"📁"`)
- Seed source: `"seed:schema:v0.24.0"` (NOT `"seed"`)
- Generation family arcs: GENERATED/GENERATED_FROM/ASSEMBLES (NOT HAS_NATIVE)
- No `brain/**` paths

---

## Definition of Done
- [ ] Zero phantom commands in any doc
- [ ] Zero wrong provider models (groq/gemini)
- [ ] Zero wrong env var names (GEMINI_API_KEY not GOOGLE_API_KEY)
- [ ] Zero wrong crate counts (always 12)
- [ ] Zero 4-view TUI references (always 3)
- [ ] Zero unqualified src/ paths in dx files
- [ ] Zero `cargo run --` in docs (use nika commands)
- [ ] Zero `--all-features` in test commands (use --lib)
- [ ] Zero `uppercase`/`lowercase` transforms (use upper/lower)
- [ ] Zero missing $ prefix in with: bindings in code examples
