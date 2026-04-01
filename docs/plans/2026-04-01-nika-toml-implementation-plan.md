# Implementation Plan: nika.toml + Project Structure + UX

> Date: 2026-04-01 | Status: VALIDATED | Scope: v0.59.0
> Research: 5 reports in docs/research/2026-04-01-*.md
> TDD: 43 tests planned, write RED first

## Decision Summary

| Decision | Choice |
|---|---|
| Config file | `nika.toml` at project root (versioned) |
| Runtime dir | `.nika/` (gitignored, unchanged) |
| Philosophy | `.git` principle: zero imposed directory names |
| Discovery | Walk up for `nika.toml` (primary) > `.nika/` (fallback) |
| Merge order | CLI flags > env vars > nika.toml > ~/.nika/config.toml > defaults |
| Backward compat | Zero. `.nika/config.toml` is legacy, doctor detects + migrate |
| New deps | `comfy-table = "7"`, `textwrap = "0.16"` |
| Artifacts dir | `./artifacts` (configurable via `[artifacts] dir`) |

## New Types (defined by TDD tests)

```rust
// boot.rs
pub enum ConfigSource { NikaToml, DotNika, Defaults }
pub struct CliOverrides { provider: Option<String>, model: Option<String> }
pub struct ProjectConfig { name: String, description: Option<String> }

// BootContext gains:
//   project_root: Option<PathBuf>
//   config_source: Option<ConfigSource>

// BootSequence gains:
//   .with_user_config_dir(&Path)
//   .with_cli_overrides(CliOverrides)

// config.rs
pub struct ProjectRoot { root: PathBuf, source: ProjectRootSource }
pub enum ProjectRootSource { NikaToml, DotNika, Fallback }
pub fn find_project_root_from(start: &Path) -> Result<ProjectRoot>

// init.rs
pub async fn init_project_at(root: &Path, permission: &str, migrate_keys: bool) -> Result<()>
```

## Impact Analysis: 35 Files

### TIER 1 — MUST CHANGE (7 files)

| File | Lines | What changes |
|---|---|---|
| `nika-cli/src/config.rs` | 332 | `find_nika_dir()` -> `find_project_root_from()`, walk up for nika.toml |
| `nika-cli/src/init.rs` | 187 | Create `nika.toml` (not `.nika/config.toml`), .gitignore append |
| `nika-cli/src/doctor.rs` | 1315 | Check nika.toml, detect legacy, migration suggestion |
| `nika-engine/src/runtime/boot.rs` | 826 | Phase 1: nika.toml discovery. Phase 2: merge user+project config |
| `nika-engine/src/core/paths.rs` | 538+ | `GLOBAL_CONFIG` const, `global_config_path()`, project paths |
| `nika-tui/src/config.rs` | 200+ | Hardcoded `.nika/config.toml` -> find nika.toml |
| `nika-tui/src/startup.rs` | 500+ | Test fixtures for config loading |

### TIER 2 — SHOULD CHANGE (8 files)

| File | What changes |
|---|---|
| `nika-mcp/src/nika_config.rs` | MCP config from nika.toml `[mcp.*]` sections |
| `nika-init/src/course/generator.rs` | Config references in course content |
| `nika-engine/src/config.rs` | Clarify relationship with ~/.config/nika/config.toml |
| `nika-tui/src/app/mod.rs` | Config loading comments |
| `nika-tui/src/app/routing.rs` | Endpoint wiring from config |
| `nika-cli/src/course.rs` | Course progress path |
| `nika-engine/src/provider/endpoints.rs` | Custom endpoint config |
| `nika-cli/src/provider.rs` | Config references |

### TIER 3 — MIGHT CHANGE (8+ files)

| File | What changes |
|---|---|
| `nika-engine/src/io/security.rs` | DEFAULT_ARTIFACT_DIR -> `./artifacts` |
| `nika-engine/src/io/writer.rs` | ArtifactWriter reads [artifacts] from config |
| `nika-engine/src/runtime/artifact_processor.rs` | resolve_artifact_dir() from nika.toml |
| `nika-tui/src/session.rs` | SESSION_DIR stays `.nika/sessions` |
| `nika-engine/src/runtime/context_loader.rs` | Session path references |
| `nika/src/main.rs` | NIKA_SERVE_* env var comments, fallback to nika.toml |
| `nika-serve/src/config.rs` | Read [serve] from nika.toml, env vars override |
| `nika-engine/src/error.rs` + `error_domains.rs` | Error messages referencing config.toml |

### DOCS & STRINGS (5+ files)

- `nika/src/cli/help.rs` — doc comments
- `nika-init/src/course/missions.rs` — config references in course content
- `nika-init/src/course/exercises.rs` — artifact path docs
- `nika-init/src/course/progress.rs` — progress path
- `nika/tests/contracts/*.rs` — test assertions

## Phases

### Phase 1: nika.toml Foundation (CRITICAL PATH)

**Files:** boot.rs, config.rs, init.rs, paths.rs, nika-tui/config.rs, startup.rs
**TDD:** 22 tests (RED first)
**Effort:** Large (1-2 sessions)

Tests to write first:

| # | Test | File | Asserts |
|---|---|---|---|
| 1 | `config_discovery_finds_nika_toml_walking_up` | boot.rs | Walk up, find nika.toml, set project_root |
| 2 | `config_discovery_fallback_to_dot_nika` | boot.rs | No nika.toml -> .nika/ fallback |
| 3 | `config_discovery_neither_uses_defaults` | boot.rs | Empty dir -> defaults |
| 4 | `nika_toml_parsing_all_sections` | boot.rs | Full nika.toml parsed |
| 5 | `nika_toml_minimal_project_only` | boot.rs | [project]-only, rest defaults |
| 6 | `nika_toml_unknown_sections_ignored` | boot.rs | [banana] doesn't break parsing |
| 7 | `user_defaults_merge_under_project_config` | boot.rs | ~/.nika/config.toml < nika.toml |
| 8 | `cli_flags_override_nika_toml` | boot.rs | --provider/--model wins |
| 9 | `project_root_is_parent_of_nika_toml` | boot.rs | project_root != nika_dir |
| 10 | `boot_context_has_config_source` | boot.rs | ConfigSource enum |
| 11 | `find_project_root_nika_toml_in_current_dir` | config.rs | Current dir discovery |
| 12 | `find_project_root_nika_toml_3_levels_up` | config.rs | Walk 3 levels |
| 13 | `find_project_root_fallback_dot_nika` | config.rs | .nika/ fallback |
| 14 | `find_project_root_returns_start_dir` | config.rs | Nothing found -> start dir |
| 15 | `init_creates_nika_toml_at_project_root` | init.rs | nika.toml created with [project]+[tools] |
| 16 | `init_creates_dot_nika_directory` | init.rs | .nika/ dir for runtime |
| 17 | `init_creates_hello_workflow` | init.rs | hello.nika.yaml with schema |
| 18 | `init_creates_agents_md` | init.rs | AGENTS.md non-empty |
| 19 | `init_appends_to_existing_gitignore` | init.rs | Preserves + appends |
| 20 | `init_creates_gitignore_with_defaults` | init.rs | .nika/ + artifacts/ |
| 21 | `init_fails_if_nika_toml_exists` | init.rs | Idempotency guard |
| 22 | `init_does_not_create_legacy_config` | init.rs | No .nika/config.toml |

Implementation order:
1. Add types: ConfigSource, ProjectConfig, CliOverrides, ProjectRoot, ProjectRootSource
2. Add BootContext fields: project_root, config_source
3. Add BootSequence builders: with_user_config_dir, with_cli_overrides
4. Implement find_project_root_from() in config.rs
5. Update boot.rs Phase 1: walk up for nika.toml first
6. Update boot.rs Phase 2: read nika.toml, merge with user config
7. Rewrite init.rs: init_project_at() creates nika.toml
8. Update paths.rs: GLOBAL_CONFIG constant
9. Update nika-tui/config.rs: use find_project_root
10. Update error messages: .nika/config.toml -> nika.toml

### Phase 2: Wire tools.working_dir

**Files:** exec.rs, context.rs
**TDD:** 4 tests
**Blocked by:** Phase 1
**Effort:** Small

| # | Test | Asserts |
|---|---|---|
| 23 | `exec_working_dir_project` | Uses project_root as cwd |
| 24 | `exec_working_dir_workflow` | Uses workflow parent (current) |
| 25 | `exec_working_dir_none` | Uses process cwd |
| 26 | `tool_context_respects_working_dir` | ToolContext reads config |

### Phase 3: nika serve reads nika.toml

**Files:** nika-serve/src/config.rs
**TDD:** 4 tests
**Blocked by:** Phase 1
**Effort:** Small

| # | Test | Asserts |
|---|---|---|
| 27 | `serve_config_loads_from_nika_toml` | [serve] section parsed |
| 28 | `serve_env_vars_override_nika_toml` | NIKA_SERVE_* wins |
| 29 | `serve_missing_section_uses_defaults` | No [serve] -> defaults |
| 30 | `serve_workflows_default_is_dot` | "." recursive, not "./workflows" |

### Phase 4: nika clean (independent)

**Files:** NEW clean.rs, main.rs (register command)
**TDD:** 4 tests
**Effort:** Small

| # | Test | Asserts |
|---|---|---|
| 31 | `clean_runs_all_three` | trace + media + cache |
| 32 | `clean_dry_run_no_delete` | Reports but doesn't delete |
| 33 | `clean_all_includes_serve_db` | --all removes serve.db + sessions |
| 34 | `clean_reports_bytes_freed` | Per-category size reporting |

### Phase 5: nika doctor project checks

**Files:** doctor.rs (modify existing)
**TDD:** 5 tests
**Blocked by:** Phase 1
**Effort:** Medium

| # | Test | Asserts |
|---|---|---|
| 35 | `doctor_detects_nika_toml` | Reports project root |
| 36 | `doctor_warns_gitignore_missing_nika` | Missing .nika/ in .gitignore |
| 37 | `doctor_warns_gitignore_missing_artifacts` | Missing artifacts/ |
| 38 | `doctor_detects_legacy_config` | .nika/config.toml -> migration |
| 39 | `doctor_counts_workflows` | Recursive *.nika.yaml count |

### Phase 6: MCP in nika.toml

**Files:** nika-mcp/src/nika_config.rs, boot.rs Phase 5
**TDD:** 4 tests
**Blocked by:** Phase 1
**Effort:** Medium

| # | Test | Asserts |
|---|---|---|
| 40 | `mcp_section_parses_correctly` | [mcp.novanet] with command |
| 41 | `mcp_config_with_env_field` | env = { KEY = "val" } |
| 42 | `mcp_legacy_ignored_when_nika_toml` | .nika/mcp.yaml skipped |
| 43 | `mcp_fallback_to_dot_nika` | No [mcp] -> read .nika/mcp.yaml |

### Phase 0: Smart Welcome Screen

**Files:** NEW or modify main.rs default handler
**Blocked by:** Phase 1
**Effort:** Medium

Three modes:
1. No setup + no project: Welcome + nika init/setup suggestions
2. Setup done + no project: Status + suggestions
3. In project: Overview (name, workflows, provider, last run)

### Phase 8: Init Wizard UX

**Files:** init.rs (rewrite output)
**Blocked by:** Phase 1
**Effort:** Medium

Interactive flow with cliclack:
1. Project name (auto-detect, editable)
2. Provider select (with descriptions)
3. Permission select (with descriptions)
4. Spinner during file creation
5. Summary panel
6. Next steps
7. Merge with setup if no API key configured

### Phase 9: Artifact Dir Config

**Files:** io/security.rs, artifact_processor.rs, writer.rs
**Blocked by:** Phase 1
**Effort:** Small

Change DEFAULT_ARTIFACT_DIR from `.nika/artifacts` to `./artifacts`.
Read `[artifacts] dir` from nika.toml.

### Phase 7: Documentation (LAST)

**Blocked by:** ALL other phases
**Effort:** Small

- nika/CLAUDE.md: verify accuracy
- nika/README.md: project structure section
- AGENTS.md template in init.rs
- Error messages audit
- Course content updates

## nika.toml Schema

```toml
[project]
name = "my-project"
description = "Optional"

[provider]
default = "anthropic"
model = "claude-sonnet-4-6"

[tools]
permission = "plan"               # deny | plan | accept-edits | yolo
working_dir = "project"           # project | workflow | none

[policy]
allow_exec = true
allow_network = true
blocked_commands = []
allowed_hosts = []
blocked_hosts = []
max_token_spend = 100000

[artifacts]
dir = "./artifacts"

[trace]
retention_days = 7
max_traces = 100

[serve]
bind = "127.0.0.1:3000"
workflows = "."
max_concurrent = 6
timeout = 300

[mcp.novanet]
command = "cargo run -- mcp"
env = { NEO4J_URI = "bolt://localhost:7687" }

[packages]
# "@supernovae/seo-audit" = "^1.0"

[memory]
# enabled = true
# auto_extract = true
```

## UX Guidelines

### Crate Stack (existing + 2 new)

| Crate | Use |
|---|---|
| `colored` | ANSI color output (keep) |
| `indicatif` | Progress bars + spinners (keep) |
| `cliclack` | Interactive prompts (keep) |
| `miette` | Fancy error diagnostics (keep) |
| `terminal_size` | Width detection (keep) |
| `unicode-width` | CJK/emoji handling (keep) |
| `comfy-table` | **NEW** — proper tables for list commands |
| `textwrap` | **NEW** — width-aware wrapping |

### DO

- Named ANSI colors (`.green()`, `.dimmed()`) — adapts to terminal themes
- `N I K A` spaced header in rounded box — this IS the brand
- 2-space indent baseline
- Semantic colors: green=success, yellow=warning, red=error, cyan=info, magenta=infer
- `panel()` with rounded corners for headers
- `tree_connector()` for hierarchical lists
- Sleep 100-200ms between wizard steps (feels intentional)

### DON'T

- ASCII art butterfly logo (the header box IS the brand)
- Gradient text (fails on light terminals)
- Typewriter effects (except nika chat)
- owo-colors (churn for zero gain)
- comfy-table for tiny lists (manual alignment is fine for <5 items)
- `.dimmed()` on critical info (fragile on light themes)

## Testing Infrastructure

### Patterns to Use

| Pattern | When | Crate |
|---|---|---|
| `tempfile::tempdir()` | Config, init, paths tests | tempfile |
| `#[serial]` | Env var mutation tests | serial_test |
| `insta::assert_snapshot!()` | Display output regression | insta |
| `pretty_assertions` | Struct comparison | pretty_assertions |
| Pure function tests | Display formatters | (none) |
| Strip ANSI + assert | Colored output | (manual) |
| `init_project_at(path)` | Init tests (no cwd dependency) | (none) |
| `with_user_config_dir()` | User default tests | (none) |

### Test Command

```bash
cd tools && cargo test --workspace --lib    # Always --lib (no keychain popups)
```

## Execution Order

```
Phase 1 (foundation) ─┬──> Phase 2 (working_dir)
                       ├──> Phase 3 (serve)
                       ├──> Phase 5 (doctor)
                       ├──> Phase 6 (MCP)
                       ├──> Phase 0 (welcome)
                       ├──> Phase 8 (init UX)
                       └──> Phase 9 (artifacts)

Phase 4 (clean) ──────────> (independent)

All phases ───────────────> Phase 7 (docs)
```

Total: 10 phases, 43 TDD tests, 35 files, ~10K lines affected.
