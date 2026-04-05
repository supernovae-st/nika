# Master Handoff Prompt: nika.toml Implementation

> Copy this entire document as prompt for a new Claude session.
> It contains ALL context, decisions, file paths, TDD tests, and UX specs.

---

## Mission

Implement the `nika.toml` project structure migration for Nika v0.59.0. This is a validated design — all decisions are final, zero ambiguity. Execute with TDD (RED-GREEN-REFACTOR), using subagent-driven development for independent phases.

## Methodology

Use these superpowers skills IN ORDER:

1. `/spn-powers:test-driven-development` — For EVERY phase: write tests FIRST (RED), implement (GREEN), refactor
2. `/spn-powers:subagent-driven-development` — Dispatch independent phases as parallel subagents
3. `/spn-powers:verification-before-completion` — After each phase: `cd tools && cargo test --workspace --lib`, verify output
4. `/spn-powers:requesting-code-review` — After Phase 1 (critical path), request code review before continuing

**Testing command: ALWAYS `cd tools && cargo test --workspace --lib`** (never without `--lib` — triggers macOS Keychain popups). The Cargo workspace root is at `tools/Cargo.toml`, NOT at the repo root. Current test count: **2153 tests passing** (as of 2026-04-01).

**Working directory:** Always `cd` to `tools/` before running cargo commands. The repo root has no Cargo.toml.

**Commit style:**
```
type(scope): description

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```
Types: feat, fix, refactor. Scopes: runtime, cli, tui, serve.

**Zero backward compat.** Zero users. Break everything. Only `nika/workflow@0.12` matters.

## What Changed and Why

**Before:** Config lives inside `.nika/config.toml` (gitignored). Team members can't share project config. Anti-pattern — every tool in the industry does config-at-root.

**After:** `nika.toml` at project root (versioned, committed). `.nika/` is runtime-only (traces, cache, media — gitignored). The `.git` principle: Nika imposes ZERO directory names on the user's project. Workflows found by `*.nika.yaml` extension, not by convention directory.

**Research basis:** 20+ tools analyzed (Cargo, Pulumi, Prefect, Mise, Dagger, LangGraph, CrewAI, Prompt Flow). Universal pattern: config-at-root + runtime-dir-gitignored. Reports at `docs/research/2026-04-01-*.md`.

## Full Implementation Plan

Read `docs/plans/2026-04-01-nika-toml-implementation-plan.md` for the complete plan. Below is the execution guide.

## Architecture: Two Scopes

```
USER SCOPE (~/.nika/)                    PROJECT SCOPE (project/)
Created by: nika setup                  Created by: nika init
─────────────────────────               ──────────────────────────
~/.nika/                                project/
├── config.toml  ← user defaults        ├── nika.toml     ← PROJECT CONFIG (NEW)
├── daemon/      ← socket, PID          ├── .nika/        ← runtime (gitignored)
├── secrets/vault.enc ← API keys        │   ├── traces/
├── packages/    ← nika pkg             │   ├── media/store/
├── registry.yaml                       │   ├── cache/
├── memory.grafeo  ← future             │   ├── sessions/
└── memory-meta.db ← future             │   └── serve.db
                                        ├── .mcp.json     ← MCP servers (convention Claude Code)
                                        ├── *.nika.yaml   ← workflows (anywhere)
                                        ├── artifacts/    ← output (configurable)
                                        ├── AGENTS.md
                                        └── .gitignore
```

**Config merge order:** CLI flags > env vars > nika.toml (project) > ~/.nika/config.toml (user) > hardcoded defaults

## nika.toml Full Schema

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

# MCP config is NOT in nika.toml — see .mcp.json (separate file, convention standard)

[packages]
# "@supernovae/seo-audit" = "^1.0"

[memory]
# enabled = true
# auto_extract = true
```

**Key:** `#[serde(deny_unknown_fields)]` must be OFF — unknown sections ignored gracefully (forward-compatible).

## Execution Plan

### Dependency Graph

```
Phase 1 (nika.toml foundation) ─┬──> Phase 2 (working_dir)
                                 ├──> Phase 3 (serve)
                                 ├──> Phase 5 (doctor)
                                 ├──> Phase 6 (MCP)
                                 ├──> Phase 0 (welcome)
                                 ├──> Phase 8 (init wizard UX)
                                 └──> Phase 9 (artifacts dir)

Phase 4 (nika clean) ──────────────── (independent, can run in parallel)

All phases ──────────────────────────> Phase 7 (documentation, LAST)
```

### Execution Strategy

1. **Start Phase 1 + Phase 4 in parallel** (Phase 4 is independent)
2. After Phase 1 passes: **request code review** (`/spn-powers:requesting-code-review`)
3. **Dispatch Phases 2, 3, 5, 9 as parallel subagents** (all small, independent after Phase 1)
4. **Phase 6** after Phase 1 (medium, touches MCP subsystem)
5. **Phases 0, 8** after Phase 1 (UX work, can be parallel)
6. **Phase 10** after Phase 1 (CLI UX polish — verbs + config list + comfy-table)
7. **Phase 7** LAST (docs, after everything else)

Each phase = 1 commit. `1 fix = 1 commit` rule.

```
Phase 1 (nika.toml) ─┬──> Phase 2 (working_dir)
                      ├──> Phase 3 (serve)
                      ├──> Phase 5 (doctor)
                      ├──> Phase 6 (MCP)
                      ├──> Phase 0 (welcome)
                      ├──> Phase 8 (init wizard)
                      ├──> Phase 9 (artifacts)
                      └──> Phase 10 (CLI UX polish)

Phase 4 (clean) ──────────> (independent)

All phases ───────────────> Phase 7 (docs)
```

Total: 11 phases, 50 TDD tests, 40+ files, ~12K lines affected.

---

## PHASE 1: nika.toml Foundation (CRITICAL PATH)

### Files to Modify

| # | File | Path | Lines | What |
|---|---|---|---|---|
| 1 | boot.rs | `tools/nika-engine/src/runtime/boot.rs` | 826 | Phase 1+2 rewrite, new types |
| 2 | config.rs | `tools/nika-cli/src/config.rs` | 332 | `find_nika_dir()` -> `find_project_root_from()` |
| 3 | init.rs | `tools/nika-cli/src/init.rs` | 187 | Create nika.toml, not .nika/config.toml |
| 4 | paths.rs | `tools/nika-engine/src/core/paths.rs` | 549 | GLOBAL_CONFIG const, path functions |
| 5 | tui config | `tools/nika-tui/src/config.rs` | 200 | Hardcoded path fix |
| 6 | tui startup | `tools/nika-tui/src/startup.rs` | 500 | Test fixtures |
| 7 | error.rs | `tools/nika-engine/src/error.rs` | 2797 | Error messages (grep "config.toml") |
| 8 | error_domains | `tools/nika-engine/src/error_domains.rs` | ? | NIKA-035 message |
| 9 | main.rs | `tools/nika/src/main.rs` | 4045 | CLI enum, onboarding, check --strict |

### New Types to Add (in boot.rs)

```rust
/// Which configuration source was used during boot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConfigSource {
    NikaToml,   // nika.toml found (new standard)
    DotNika,    // .nika/config.toml found (legacy fallback)
    Defaults,   // Nothing found, using built-in defaults
}

/// CLI overrides that take highest precedence.
#[derive(Debug, Clone, Default)]
pub struct CliOverrides {
    pub provider: Option<String>,
    pub model: Option<String>,
}

/// [project] section in nika.toml.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}
```

Add to `BootContext`:
```rust
pub project_root: Option<PathBuf>,
pub config_source: Option<ConfigSource>,
```

Add to `BootstrapConfig`:
```rust
#[serde(default)]
pub project: Option<ProjectConfig>,
```

Add to `BootSequence`:
```rust
user_config_dir: Option<PathBuf>,
cli_overrides: Option<CliOverrides>,

pub fn with_user_config_dir(mut self, dir: &Path) -> Self { ... }
pub fn with_cli_overrides(mut self, overrides: CliOverrides) -> Self { ... }
```

### New Types in config.rs

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectRoot {
    pub root: PathBuf,
    pub source: ProjectRootSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectRootSource {
    NikaToml,
    DotNika,
    Fallback,
}

/// Walk up from start to find project root.
/// Priority: nika.toml > .nika/ > start_dir
pub fn find_project_root_from(start: &Path) -> Result<ProjectRoot, NikaError> {
    // Walk up looking for nika.toml FIRST
    let mut dir = start;
    loop {
        if dir.join("nika.toml").exists() {
            return Ok(ProjectRoot { root: dir.to_path_buf(), source: ProjectRootSource::NikaToml });
        }
        match dir.parent() { Some(p) => dir = p, None => break }
    }
    // Second pass: walk up for .nika/ (legacy)
    let mut dir = start;
    loop {
        let nika_dir = dir.join(".nika");
        if nika_dir.exists() && nika_dir.is_dir() {
            return Ok(ProjectRoot { root: dir.to_path_buf(), source: ProjectRootSource::DotNika });
        }
        match dir.parent() { Some(p) => dir = p, None => break }
    }
    // Nothing found
    Ok(ProjectRoot { root: start.to_path_buf(), source: ProjectRootSource::Fallback })
}
```

### TDD Tests for Phase 1 (22 tests, write ALL before implementing)

**File: boot.rs** — append to existing `#[cfg(test)] mod tests`

Test 1-3: Config discovery (walk-up for nika.toml, fallback .nika/, defaults)
Test 4-6: Parsing (all sections, minimal, unknown sections ignored)
Test 7: User defaults merge under project config (with_user_config_dir)
Test 8: CLI overrides win (with_cli_overrides)
Test 9: project_root = parent of nika.toml, nika_dir = .nika/ inside it
Test 10: config_source enum on BootContext for all 3 cases

**File: config.rs** — append to existing tests

Test 11: find_project_root_from() finds nika.toml in current dir
Test 12: find_project_root_from() finds nika.toml 3 levels up
Test 13: Fallback to .nika/ parent
Test 14: Returns start dir + Fallback source when nothing found

**File: init.rs** — NEW test module

Test 15: init_project_at() creates nika.toml with [project] + [tools]
Test 16: Creates .nika/ directory
Test 17: Creates hello.nika.yaml with schema declaration
Test 18: Creates AGENTS.md (non-empty)
Test 19: Appends to existing .gitignore (preserves content)
Test 20: Creates .gitignore with .nika/ + artifacts/
Test 21: Fails if nika.toml already exists (idempotency)
Test 22: Does NOT create .nika/config.toml (zero legacy)

**Test patterns:**
- Use `tempfile::tempdir()` for isolation
- Use `#[serial]` for any env var mutations
- Use `init_project_at(path, ...)` not `init_project(...)` (path-explicit for testability)
- `cd tools && cargo test --workspace --lib` to verify

### Implementation Order (after tests are RED)

1. Add types to boot.rs (ConfigSource, ProjectConfig, CliOverrides)
2. Add fields to BootContext (project_root, config_source)
3. Add builders to BootSequence (with_user_config_dir, with_cli_overrides)
4. Implement `find_project_root_from()` in config.rs
5. Rewrite `phase_config_discovery()` in boot.rs — walk up for nika.toml first, then .nika/ fallback
6. Rewrite `phase_config_validation()` in boot.rs — read nika.toml, merge with user config
7. Rewrite `init_project()` in init.rs — create nika.toml (not .nika/config.toml), add `init_project_at()` for testability
8. Update paths.rs: `GLOBAL_CONFIG` const from "config.toml" to "config.toml" (user-level stays same), add project-level path functions
9. Update nika-tui/config.rs: `.nika/config.toml` -> use `find_project_root` then read nika.toml
10. Grep error.rs + error_domains.rs for ".nika/config.toml" -> replace with "nika.toml"
11. Run `cd tools && cargo test --workspace --lib` — all 22 tests GREEN

### Commit

```
feat(runtime): nika.toml project config — walk-up discovery, 3-layer merge

Replace .nika/config.toml with nika.toml at project root.
Discovery: nika.toml (primary) > .nika/ (legacy fallback) > defaults.
Merge: CLI flags > env vars > nika.toml > ~/.nika/config.toml > defaults.

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

---

## PHASE 4: nika clean (INDEPENDENT — can run parallel with Phase 1)

### New File: `tools/nika-cli/src/clean.rs`

Create umbrella command wrapping:
- `nika trace clean --keep N` (N from TraceConfig.max_traces)
- `nika media clean --older-than 1h`
- `nika cache clear`

CLI args:
- `nika clean` — runs all 3
- `nika clean --dry-run` — preview only
- `nika clean --all` — also removes serve.db + sessions/

Register in `tools/nika/src/main.rs` CLI enum (or wherever subcommands are registered — check clap setup).

### UX Output

```
  Traces:  removed 42 files                         14.7 MB
  Cache:   cleared                                    8.2 MB
  Media:   removed 3 orphaned blobs                   2.4 MB

  ✓ Freed 25.3 MB
```

For `--dry-run`: use `⊘` icon instead of `✓`, "Would remove" instead of "Removed".

### TDD: 4 Tests

Write in clean.rs `#[cfg(test)] mod tests` block. Use tempdir to create fake .nika/ with trace files, test that clean removes them. Test --dry-run doesn't delete.

### Commit

```
feat(cli): nika clean umbrella command — trace + media + cache cleanup
```

---

## PHASE 2: Wire tools.working_dir

### Files

- `tools/nika-engine/src/runtime/executor/exec.rs` (~line 124-146, cwd resolution)
- `tools/nika-engine/src/tools/context.rs` (~line 166-188, ToolContext::new)

### What

The field `tools.working_dir` exists in BootstrapConfig but is NEVER consumed. Wire it:
- `"project"` → use `project_root` (from BootContext) as cwd
- `"workflow"` → use parent of the .nika.yaml file (current behavior)
- `"none"` → no sandboxing, use process cwd

This unblocks Nicolas for nk-jungo (he needs exec commands to run from project root, not workflow dir).

### Commit

```
feat(runtime): wire tools.working_dir — project | workflow | none
```

---

## PHASE 3: nika serve reads nika.toml

### File: `tools/nika-serve/src/config.rs` (133 lines)

Currently 100% env vars. Add nika.toml as fallback layer:

```rust
impl ServeConfig {
    pub fn load() -> Result<Self, ServeError> {
        // 1. Try reading [serve] from nika.toml (find_project_root)
        // 2. Env vars override nika.toml values
        // 3. Defaults for anything not set
    }
}
```

Change workflows default from `"./workflows"` to `"."` (recursive scan).
Keep `NIKA_SERVE_TOKEN` as env-var-only (secrets never in nika.toml).

### Commit

```
feat(serve): read [serve] from nika.toml, env vars override
```

---

## PHASE 5: nika doctor project checks

### File: `tools/nika-cli/src/doctor.rs` (1315 lines)

Add checks to the existing diagnostic flow:
1. Check `nika.toml` exists → show project root path
2. Check `.gitignore` includes `.nika/`
3. Check `.gitignore` includes artifacts dir (from [artifacts].dir)
4. Detect legacy `.nika/config.toml` → suggest `nika init --migrate`
5. Count `*.nika.yaml` files recursively → show workflow count
6. Summary line: "X checks: Y passed, Z warnings, W failed"

### Commit

```
feat(cli): doctor project structure checks + legacy migration detection
```

---

## PHASE 6: MCP config in nika.toml

### File: `tools/nika-mcp/src/nika_config.rs` (942 lines)

Add `[mcp.*]` section parsing to BootstrapConfig:

```toml
[mcp.novanet]
command = "cargo run -- mcp"
args = ["-y"]
env = { NEO4J_URI = "bolt://localhost:7687" }
```

**DESIGN CHANGE (security audit):** MCP config is NOT in nika.toml. It uses `.mcp.json` at project root, following the Claude Code / Cursor convention.

**Why:** Putting MCP commands (`command = "cargo run -- mcp"`) in versioned nika.toml = arbitrary code execution via `git clone` + `nika run`. This was flagged as CRITICAL in the security audit. The `.mcp.json` convention is already understood by developers (same trust model as Claude Code).

### Project MCP: `.mcp.json` (versioned, at project root)

```json
{
  "mcpServers": {
    "novanet": {
      "command": "cargo",
      "args": ["run", "--manifest-path", "../novanet/Cargo.toml", "--", "mcp"],
      "env": {
        "NEO4J_URI": "bolt://localhost:7687"
      }
    }
  }
}
```

### User MCP: `~/.nika/mcp.yaml` (user-scoped, never committed — keeps current format)

### Boot Phase 5 changes:
1. Look for `.mcp.json` at project root (from `find_project_root`)
2. Parse with `serde_json::from_str` (not TOML)
3. Merge with global `~/.nika/mcp.yaml`
4. Fallback: legacy `.nika/mcp.yaml` (project-level, for migration)

### Files to modify:
- `tools/nika-mcp/src/nika_config.rs` (942 lines) — add `.mcp.json` reader alongside YAML
- `tools/nika-engine/src/runtime/boot.rs` — Phase 5 reads `.mcp.json` from project_root
- `tools/nika-cli/src/init.rs` — optionally create `.mcp.json` during init

### TDD Tests:
| # | Test | Asserts |
|---|---|---|
| 40 | `mcp_json_parsed_correctly` | .mcp.json with mcpServers map |
| 41 | `mcp_json_with_env_field` | env vars passed to server |
| 42 | `mcp_json_preferred_over_legacy_yaml` | .mcp.json wins over .nika/mcp.yaml |
| 43 | `mcp_fallback_to_global_yaml` | No .mcp.json -> read ~/.nika/mcp.yaml |

### Commit

```
feat(mcp): read .mcp.json (Claude Code convention) for project MCP servers
```

---

## PHASE 9: Artifact Dir Config

### Files

- `tools/nika-engine/src/io/security.rs` — `DEFAULT_ARTIFACT_DIR` constant
- `tools/nika-engine/src/runtime/artifact_processor.rs` — `resolve_artifact_dir()`
- `tools/nika-engine/src/io/writer.rs` — ArtifactWriter

Change DEFAULT_ARTIFACT_DIR from `".nika/artifacts"` to `"./artifacts"`.
Read `[artifacts].dir` from nika.toml if available.

### Commit

```
feat(runtime): artifact dir defaults to ./artifacts, configurable via nika.toml
```

---

## PHASE 0: Smart Welcome Screen

### File: Modify default handler in `tools/nika/src/main.rs` (or wherever `nika` with no args is handled)

Three modes based on detection:

**Mode 1: No setup, no project**
```
  ╭────────────────────────────────────────╮
  │  N I K A                    v0.59.0    │
  │  One file. Any AI.                     │
  ╰────────────────────────────────────────╯

  ⚠ No API keys configured
  ⚠ No project found

  Get started
  ──────────────────────────
    1. nika setup         Configure your first LLM provider
    2. nika init          Initialize a project here
    3. nika init --course Learn with 44 guided exercises
```

**Mode 2: Setup done, no project**
```
  N I K A v0.59.0

  ✓ anthropic   ✓ gemini   ✗ 4 more

  Not in a project directory.
    nika init      Initialize a project here
    nika infer     Quick LLM call
```

**Mode 3: In a project**
```
  N I K A v0.59.0                       ~/dev/my-project

  Provider:   anthropic
  Workflows:  7 files
  Last run:   ✓ Done (12 min ago)

    nika run <file>   Execute a workflow
    nika ui           Open TUI
```

Use existing display primitives: `panel()`, `StatusIcon`, `section_header()`, `key_value()`.

### Commit

```
feat(cli): smart welcome screen — contextual nika (no args) output
```

---

## PHASE 8: Init Wizard UX

### File: `tools/nika-cli/src/init.rs` (rewrite)

Interactive flow using cliclack (already a dependency):

```rust
cliclack::intro("Initialize a new Nika project")?;

let name = cliclack::input("Project name")
    .default_input(dir_name)
    .interact()?;

let provider = cliclack::select("Default LLM provider")
    .item("anthropic", "anthropic", "Claude — best for reasoning")
    .item("openai", "openai", "GPT-4o — versatile")
    .item("groq", "groq", "Llama 4 — free, fast")
    // ...
    .interact()?;

let permission = cliclack::select("Permission mode")
    .item("plan", "plan", "Show plan, ask before running")
    // ...
    .interact()?;

let spinner = cliclack::spinner();
spinner.start("Creating project...");
// create files
tokio::time::sleep(Duration::from_millis(150)).await; // feels intentional
spinner.stop("Project created");

cliclack::outro("Next: nika run hello.nika.yaml")?;
```

Keep `init_project_at()` as the core function (testable). The cliclack flow calls it.
Add `--yes` flag for non-interactive mode (expert users, CI).

### Commit

```
feat(cli): interactive init wizard with cliclack prompts
```

---

## PHASE 7: Documentation (LAST)

After all phases pass:
1. Re-read `nika/CLAUDE.md` — verify Project Structure section is accurate
2. Update `nika/README.md` — add project structure section
3. Update AGENTS.md template in `init.rs` (the `include_str!` content)
4. Grep all error messages for "config.toml" → ensure they say "nika.toml"
5. Update course content (missions.rs, exercises.rs) if they reference .nika/config.toml

### Commit

```
docs: update all references from .nika/config.toml to nika.toml
```

---

## Testing Infrastructure Reference

### Crates Available

| Crate | Version | Use |
|---|---|---|
| tempfile | 3.27 | Isolated temp dirs |
| insta | 1.34 | Snapshot testing |
| pretty_assertions | 1.4 | Colored diffs |
| serial_test | 3 | `#[serial]` for env vars |
| rstest | 0.25 | Parametrized tests |
| proptest | 1.4 | Fuzzing |

### Existing Test Patterns

**Config test pattern (tempdir + toml roundtrip):**
```rust
let temp = tempfile::tempdir().unwrap();
std::fs::write(temp.path().join("nika.toml"), toml_content).unwrap();
let boot = BootSequence::new(temp.path());
let ctx = boot.run(None).await.unwrap();
assert_eq!(ctx.config.as_ref().unwrap().provider.default, "anthropic");
```

**Env var isolation pattern:**
```rust
#[serial]
#[tokio::test]
async fn test_env_override() {
    let old = std::env::var("KEY").ok();
    std::env::set_var("KEY", "value");
    // test
    match old { Some(v) => std::env::set_var("KEY", v), None => std::env::remove_var("KEY") }
}
```

**Display output test pattern:**
```rust
fn strip_ansi(s: &str) -> String {
    let mut result = String::new();
    let mut in_escape = false;
    for ch in s.chars() {
        if ch == '\x1b' { in_escape = true; }
        else if in_escape && ch == 'm' { in_escape = false; }
        else if !in_escape { result.push(ch); }
    }
    result
}
```

### Display Module Reference

Key utilities already available in `tools/nika-engine/src/display/`:
- `cli_format.rs`: `panel()`, `panel_with_content()`, `section_header()`, `key_value()`, `status_line()`, `tree_connector()`, `separator()`, `hint()`, `terminal_width()`
- `icons.rs`: `StatusIcon { Ok, Fail, Warn, Info, Skip, Download, Hint }`, verb icons (`verb("infer")` → ✧), subsystem icons
- `colors.rs`: `tokens()`, `format_bytes()`, `duration()`, `sparkline()`, `budget_bar()`, `cost()`, `ttft()`
- `spinner.rs`: `TICK_STRINGS` (braille), `TICK_INTERVAL` (80ms), progress templates

### Cargo Workspace Layout

**CRITICAL: Workspace root is `tools/Cargo.toml`, not repo root.** Always `cd tools/` before cargo commands.

```
tools/                   ← Cargo workspace root (Cargo.toml here)
├── nika/                # CLI binary (2k lines, main.rs, clap)
├── nika-cli/            # CLI subcommand handlers (8k — init, config, doctor, tools_cmd, clean...)
├── nika-engine/         # Execution engine (135k — embeddable runtime)
├── nika-core/           # AST, types, catalogs (23k — zero I/O)
├── nika-tui/            # Terminal UI (86k — ratatui)
├── nika-serve/          # HTTP server (axum)
├── nika-daemon/         # Background daemon (5k)
├── nika-mcp/            # MCP client (9k — rmcp)
├── nika-init/           # Project scaffolding + course (21k)
├── nika-event/          # EventLog, TraceWriter (4k)
├── nika-media/          # CAS store, processor (13k)
├── nika-lsp-core/       # LSP intelligence (9k)
└── nika-lsp/            # LSP binary (2.5k)
```

### CLI Command Registration

New commands (like `nika clean`, `nika tools`) are registered in `tools/nika/src/main.rs`:
- Add variant to `enum Commands` (line ~160, uses `#[derive(Subcommand)]`)
- Add handler in `match` block (line ~1200+)
- Implementation in `tools/nika-cli/src/` (e.g., `clean.rs`)
- Register module in `tools/nika-cli/src/lib.rs`

Recent example: `nika tools list` added in commit `824dbc038` — new file `tools/nika-cli/src/tools_cmd.rs` (134 lines), 5 files changed total.

---

## Recent Changes to Account For (since plan was written)

### v0.58.1 released — 3 of our v0.59 issues already FIXED

These v0.59 plan issues were implemented BEFORE our nika.toml work begins:

| Issue | Commit | Status |
|---|---|---|
| fetch 404 returns exit 0 | `0f92a2ad3` | **FIXED** — 4xx now errors (except response: full) |
| fail_fast:false partial results blocked | `3f770efcd` | **FIXED** — partial results unblock downstream |
| $env.MISSING fails before default() | `891489a16` | **FIXED** — default() now fires |
| {{skills.NAME}} not resolved | `cef6f6b0c` | **FIXED** — template resolution added |

**These 4 issues are NO LONGER part of the nika.toml plan.** They are done.

### New commits impacting our plan

1. **`824dbc038` — `nika tools list` + `nika tools info`** — New CLI subcommand. Added `tools/nika-cli/src/tools_cmd.rs`. Shows how to register new commands (model for `nika clean`).

2. **`15a147c76` — Agent file tools use project root** — `rig_agent_loop/mod.rs` now uses process cwd (project root) instead of workflow_base_dir for agent file tools. ALIGNED with our Phase 2.

3. **`eba8c8771` — Serve default executor switched to embedded** — `ExecutorMode::Embedded` is now the default (was `Subprocess`). Phase 3 (serve reads nika.toml) must account for this.

4. **`46f6e2400` — Default to first configured provider** — No longer hardcoded anthropic. The runtime picks the first provider with an API key. Affects Phase 1 (BootstrapConfig defaults).

5. **`ebc201514` — Onboarding checks vault + NIKA_NO_ONBOARDING** — New `skip_onboarding()` function. Affects Phase 0 (smart welcome) and Phase 8 (init wizard).

6. **`945173167` — `--no-interactive` and `--quiet` for nika infer** — CLI args pattern. main.rs now at **4045 lines** (was 3200).

7. **`ed0b7e0cf` — Shell quoting stripped before blocklist check** — Security fix for NIKA-053. Not directly relevant but shows security patterns.

8. **5 serve fixes** — metrics auth, SSE validation, shutdown signaling, env allowlist, active_jobs decrement. Phase 3 must read the current serve/config.rs (133 lines).

### Rules documented since plan

- **Path resolution (E09/E19)** — All relative paths resolve from PROJECT ROOT (where `nika run` is invoked). Aligns with `working_dir = "project"` default.
- **Exec stderr (E29)** — stdout ONLY. stderr discarded by design. Use `2>&1`.
- **`| shell` transform (E30)** — Always use in `shell: true` exec tasks.

## Critical Pitfalls

1. **`~/.config/nika/config.toml`** exists as ANOTHER config location (engine/config.rs, 14K lines). This is a DIFFERENT system from `.nika/config.toml`. Don't confuse them. The engine/config.rs handles global user preferences (model aliases, custom endpoints). Leave it alone for now.

2. **`nika-tui/src/config.rs` line 207** has hardcoded `PathBuf::from(".nika/config.toml")`. Must change to use `find_project_root_from()`.

3. **`paths.rs` line 60** has `pub const GLOBAL_CONFIG: &str = "config.toml"`. This is for `~/.nika/config.toml` (user-level). It stays as-is. Don't rename it to "nika.toml" — that's the project-level name.

4. **boot.rs Phase 1** currently walks up for `.nika/` dir. The new Phase 1 must walk up for `nika.toml` FIRST, then `.nika/` as fallback. Two separate loops (don't try to combine them — nika.toml and .nika/ could be at different levels).

4b. **Default provider is no longer hardcoded anthropic** (commit `46f6e2400`). The runtime picks the first configured provider. The `BootstrapConfig::default()` still says `"anthropic"` as the TOML default, but the runtime may override. TDD test 3 should expect `"anthropic"` from the default config struct, not from what the runtime actually uses.

5. **init.rs line 41** checks `nika_dir.join("config.toml").exists()` for idempotency. Change to check `cwd.join("nika.toml").exists()`.

6. **`BootstrapConfig` must NOT have `#[serde(deny_unknown_fields)]`** — unknown sections (like future `[packages]`, `[memory]`) must be silently ignored. Test 6 verifies this.

7. **Artifacts dir** currently defaults to `.nika/artifacts` (inside gitignored dir). Must change to `./artifacts` (visible, also gitignored via .gitignore). This is Phase 9, not Phase 1.

8. **Error messages** — grep `tools/nika-engine/src/error.rs` (2796 lines) for every instance of ".nika/config.toml" or "config.toml" and update to "nika.toml" where it refers to project config. Be careful: `~/.nika/config.toml` references should stay as-is.

9. **Course content** — `nika-init/src/course/missions.rs` line 1144 and 1160 reference `.nika/config.toml`. Update to `nika.toml`.

10. **Tests that create `.nika/config.toml`** — boot.rs tests, paths.rs tests, startup.rs tests all create `.nika/config.toml` in tempdir. Update them to create `nika.toml` instead (or test both for fallback behavior).

## Security Findings (from rust-security audit)

### RESOLVED: MCP commands no longer in nika.toml

The CRITICAL finding (MCP commands = arbitrary code execution from versioned config) is resolved by moving MCP config to `.mcp.json` (separate file, Claude Code convention). This follows the same trust model developers already accept.

### HIGH — Policy weakening via git commit

`[policy] allow_exec = true, blocked_commands = []` in versioned nika.toml can disable security guardrails. **Mitigation:** Policy from nika.toml can only RESTRICT further than user defaults, never RELAX. User-level `~/.nika/config.toml` sets the security floor. Implementation: during merge, take the MORE restrictive value for security fields (`allow_exec = project AND user`, blocked_commands = project UNION user).

### HIGH — Secret detection in config values

Nothing prevents `[serve] auth_token = "sk-ant-XXX"` in versioned nika.toml. **Mitigation:** Add `validate_no_secrets_in_config()` using existing `SECRET_RE` patterns from `util/mod.rs`. Run during `nika check` and `nika init`. Emit NIKA-XXX error (not warning) when secret-like values found.

### MEDIUM — Walk-up depth limit

Cap walk-up at 20 levels or stop at `$HOME`. Prevents hijacking via `/Users/nika.toml` or `/tmp/nika.toml` on shared systems.

### MEDIUM — Path traversal in config values

`[artifacts] dir = "../../../etc/"` is not validated. **Mitigation:** Reject absolute paths and paths that escape project root after normalization. Reuse `normalize_path()` from security.rs.

### LOW — TOML file size limit

Check file size before parsing: reject nika.toml > 1 MB.

## Architecture Findings (from rust-architect audit)

### HIGH — Three config systems with overlapping schemas

`NikaConfig` (engine/config.rs, ~/.config/nika/), `BootstrapConfig` (boot.rs, nika.toml), and `NikaMcpConfig` (mcp_config.rs, mcp.yaml) are independent systems. `nika infer` reads `NikaConfig` directly (bypasses boot), while `nika run` uses `BootstrapConfig`. They can return different provider/model defaults.

**For this sprint:** Do NOT unify. Document the duality. Ensure `nika infer` respects nika.toml by calling `find_project_root` and reading nika.toml in verbs.rs. Full unification is a separate sprint.

### MEDIUM — merge_config_layers cannot detect explicit vs default values

If nika.toml has no `[provider]` section, serde fills `default = "anthropic"`. The merge logic cannot tell this from an explicit `default = "anthropic"`. User-level `openai` default gets overridden by the serde default.

**For this sprint:** Option-wrap fields that participate in merging. `None` = not set (use parent), `Some(x)` = explicitly set.

### MEDIUM — Walk-up logic duplicated in 3 files

boot.rs, config.rs, nika-mcp/nika_config.rs all have walk-up loops. **Fix:** Extract `find_project_root_from()` to `nika-core` (zero-dep crate, all crates depend on it). Use NIKA_PROJECT_CONFIG constant.

### LOW — syntect too heavy, use colored + regex instead

Avoid syntect (15-30s added to clean builds). For TOML/JSON highlighting in CLI output, use colored + simple regex patterns. Tree-sitter is already in the dep tree if richer highlighting needed later.

## Rust Code Quality Findings (from rust-pro audit)

### P0 — Add NIKA_PROJECT_CONFIG constant

`"nika.toml"` is hardcoded as string literal in 3+ files. Add to paths.rs:
```rust
pub const NIKA_PROJECT_CONFIG: &str = "nika.toml";
```

### P0 — Single find_project_root in nika-core

Three implementations of walk-up logic. Extract to `nika-core` or `nika-engine/core/paths.rs`.

### P1 — Atomic config writes

`fs::write` truncates then writes. If process crashes mid-write, config is corrupted. Use:
```rust
let tmp = config_path.with_extension("toml.tmp");
fs::write(&tmp, new_content)?;
fs::rename(&tmp, &config_path)?;
```

### P1 — Remove async from sync boot phases

4 of 7 boot phases are `async fn` with zero `.await` calls. Unnecessary Future state machine allocation.

### P2 — 2 unnecessary PhaseResult clones

Reorder emit/push/check to avoid cloning phase results in phases 1-2.

### P2 — PhaseResult name collision

`boot::PhaseResult` and `display::check::PhaseResult` are different structs with same name. Rename to `BootPhaseResult`.

### P2 — Option-wrap BootstrapConfig for merge correctness

All fields use serde defaults. Cannot distinguish "not set" from "set to default value". Wrap mergeable fields in `Option<T>`.

## Performance Findings (from rust-perf audit)

### CRITICAL — BootSequence is dead code

`BootSequence` (boot.rs, 826 lines) is NEVER called in production. `nika run` does ad-hoc boot in `main.rs:2415` (run_workflow). `nika infer` uses `one_shot_executor()` in verbs.rs which bypasses boot entirely. Only tests use BootSequence.

**For this sprint:** Wire BootSequence into the actual code paths. Otherwise all the nika.toml discovery logic is dead code.

### HIGH — 500ms daemon auto-start sleep

`secrets/fallback.rs:38-51` has a `tokio::time::sleep(500ms)` after daemon auto-start. First `nika` command after reboot/crash pays 500ms penalty.

**Mitigation:** Replace with 50ms polling loop (max 3 retries = 150ms). Or make daemon startup non-blocking (proceed, fall through to vault if IPC fails).

### MEDIUM — Sync reads in async boot

`tokio::fs::read_to_string` in boot.rs Phase 2 adds spawn_blocking overhead (~5us). No concurrent I/O during boot. Use `std::fs::read_to_string` instead.

### LOW — Combine walk-up passes

Two walk-up loops (nika.toml then .nika/) can be combined into one pass. Saves ~50us. Worth doing since we're rewriting the function anyway.

### INFORMATIONAL — Verb overhead is fine

`one_shot_executor()` costs ~2-8ms (reqwest client + TLS init). LLM calls are 500-5000ms. <1% overhead.

### INFORMATIONAL — indicatif spinner is well-designed

80ms tick via tokio task (not OS thread). No optimization needed.

### NOTE — Skip daemon for mock-provider verbs

`nika fetch` and `nika invoke` (mock provider) don't need secrets. Skip `load_from_daemon_or_fallback()` for these. Saves 0-500ms.

---

## PHASE 10: CLI UX Polish — Inline Verbs + Command Consistency

> This phase can run independently or after Phase 1. It covers the inline CLI experience
> for all 5 verbs and fixes the 10 inconsistencies found in the 26-command audit.

### The Problem

The display system has a two-tier quality gap:
- `nika run` (workflow execution): LiveRenderer, spinners, sparklines, timeline — excellent
- `nika infer/fetch/invoke/agent` (inline verbs): raw println, no spinner, no progress — minimal

All 4 inline verb handlers live in ONE file: `tools/nika-cli/src/verbs.rs` (635 lines).
They share the same minimal pattern: `print_header()` then `println!("{output}")` then `print_footer()`.

Current verb display:
```
  [dim]|--[/dim] claude-sonnet-4-6 via anthropic
  The LLM response goes here as raw text
  [dim]|__[/dim] 523ms [dim]|[/dim] 1200 tokens [dim]|[/dim] $0.0045
```

### Research Basis

Report: `docs/research/2026-04-01-cli-ai-output-ux-patterns.md` (analyzed Ollama, llm, aichat, mods, httpie, xh, bat, glow, sgpt, gh copilot).

Key findings:
- `syntect` (v5.3) is THE library for JSON + Markdown highlighting (used by bat, aichat, xh) — one dep covers all needs
- Streaming architecture (aichat 50ms batch) is gold standard but SEPARATE sprint — not in scope here
- Thinking state: Ollama's "Thinking... / ...done thinking." grey+dimmed pattern is cleanest
- Nika's metadata footer (tokens/cost/TTFT) is already best-in-class vs competition
- TTY vs pipe: stdout = pure output, metadata on stderr — Nika already correct

### Design Principles for Inline Verbs

1. TTY = rich, pipe = raw. Already implemented via `is_terminal()` check. Preserve this.
2. No streaming in this sprint. Streaming infer/agent is a separate project (needs tokio channels, aichat-style 50ms batch architecture). Instead: add spinner during inference, dump result at once.
3. Pretty-print JSON with syntect. When output is JSON (structured, invoke, fetch metadata), syntax-highlight with embedded Monokai theme on TTY.
4. Show TTFT. Extract from EventLog `ProviderResponded` event. Show in footer.
5. Spinner during LLM calls. Use existing indicatif braille spinner for infer/agent while waiting.
6. Cost always visible. Use cost color from colors.rs (green cheap, yellow moderate, red expensive).
7. Extended thinking indicator. When `extended_thinking: true`, show "Thinking..." (dimmed) during thinking phase, "...done thinking." when response starts (Ollama pattern).

### Verb Improvements

#### `nika infer` (verbs.rs:180)

Improvements:
- Add indicatif spinner before `run_infer()` call, clear after
- Add verb icon (magenta infer star) to header
- Pretty-print JSON via `serde_json::to_string_pretty()` + colorize keys/values when structured
- Extract TTFT from EventLog (ProviderResponded.ttft_ms) and show in footer
- Use `textwrap::wrap()` for text output at terminal width
- Show structured output layer info (L0/L2/L3/L4) from EventLog
- Footer format: `TTFT 187ms | 523ms total | 1.2k tokens | $0.004 | L0 tool-injected`

#### `nika fetch` (verbs.rs:309)

Improvements:
- Add verb icon (cyan comet) to header
- Show HTTP status code in header line
- Show compression ratio (extracted bytes / original bytes) in footer
- For JSON extract modes (metadata, jsonpath, feed): pretty-print with colors
- For links mode: use comfy-table for tabular display (type, count, examples)

#### `nika invoke` (verbs.rs:414)

Improvements:
- Add verb icon (green circled asterisk) + file context in header
- Detect JSON output, format as key-value pairs (top-level fields) on TTY
- Truncate to top N fields with "+N more" hint for complex results
- `--json` flag bypasses formatting, gives raw JSON (for piping)
- Show tool description in header from BuiltinToolRouter metadata

#### `nika agent` (verbs.rs:540)

Improvements:
- Add verb icon (red propeller) to header
- Subscribe to EventLog events DURING execution (not just after)
- Print tool calls as they happen (ToolCall + ToolResult events)
- Show turn separators with turn number
- Show tool name + brief result for each invocation
- Final turn labeled "(final)"
- Footer: actual turn count (not max), total tokens across all turns

### `nika config list` — Upgrade from Raw TOML Dump

Since we rewrite config loading (Phase 1), upgrade the display:
- Parse nika.toml into BootstrapConfig struct
- Display each section with [section] header (bold)
- Key-value pairs with key_value_width() helper
- Provider: show key status (check/cross icon)
- MCP: show connection status
- File path in subtitle

### Command Consistency Fixes (touch ONLY when already modifying the file)

| Command | File | Fix | When |
|---|---|---|---|
| `config list` | config.rs | Pretty-print nika.toml with section colors | Phase 1 |
| `provider list` | provider.rs | Use section_header() consistently | If touching |
| `model list` | model_cloud.rs | Switch double-line to single-line separator | Optional |
| `invoke --list` | verbs.rs | Add StatusIcon for tiers | Phase 10 |
| `features` | main.rs | Use section_header() | Optional |
| `trace list` | trace.rs | Add --json flag | Optional |
| `new` | new_cmd.rs | Show created file summary panel | Phase 8 |

### Display Style Guide (add as doc comment in display module)

```
HEADERS: section_header() for lists, panel() for major ops (init, run, bench)
SEPARATOR: single dash line. Never double-line except bench.
ICONS: StatusIcon enum (Ok, Fail, Warn, Info, Skip) + verb icons (infer star, exec helm, fetch comet, invoke circled-asterisk, agent propeller)
COLORS: green=success/fast/cheap, yellow=warning/medium, red=error/slow/expensive, cyan=info/links, magenta=infer, dimmed=secondary
LAYOUT: 2-space indent, key_value_width() for labels, tree_connector() for hierarchy, comfy-table for tables
OUTPUT MODES: TTY=rich, pipe=raw, --json=machine, --quiet=minimal
FOOTER: TTFT Xms | Yms | Z tokens | $cost | extra_info (cost uses semantic color)
```

### Files to Modify (Phase 10)

| File | Changes |
|---|---|
| `nika-cli/src/verbs.rs` (635 lines) | Spinner, pretty-print, TTFT, verb icons, agent turns |
| `nika-cli/src/config.rs` (332 lines) | config list structured display (touched in Phase 1) |
| `nika-engine/src/display/cli_format.rs` (299 lines) | Add verb_header(), verb_footer() helpers |
| `nika-engine/src/display/colors.rs` (150 lines) | Add json_highlight() for pretty JSON |
| Cargo.toml (workspace) | Add comfy-table = "7", textwrap = "0.16", syntect = "5.3" |

### TDD Tests (Phase 10)

| # | Test | Asserts |
|---|---|---|
| 44 | `verb_header_includes_icon` | Correct icon per verb type |
| 45 | `verb_footer_shows_ttft` | TTFT extracted from EventLog |
| 46 | `json_output_pretty_printed_on_tty` | Indented, keys colored |
| 47 | `json_output_raw_when_piped` | No ANSI, no indentation |
| 48 | `config_list_shows_sections` | [project], [provider], [tools] sections |
| 49 | `config_list_shows_provider_status` | Check/cross for key presence |
| 50 | `invoke_result_formatted_as_kv` | JSON to key-value pairs on TTY |

---

## Build Notes

If you encounter linker errors or temp dir issues, run `cargo clean` in `tools/` first. The build cache can get corrupted across sessions.

```bash
cd tools && cargo clean && cargo test --workspace --lib
```

## Success Criteria

After all phases:
- [ ] `cd tools && cargo test --workspace --lib` passes (2153 existing + 50 new tests)
- [ ] `nika init` creates `nika.toml` (not `.nika/config.toml`)
- [ ] `nika run hello.nika.yaml` works in a fresh project
- [ ] `nika doctor` detects nika.toml and reports project root
- [ ] `nika doctor` detects legacy `.nika/config.toml` and suggests migration
- [ ] `nika clean` removes traces + cache + media with size summary
- [ ] `nika config list` shows structured nika.toml (not raw TOML dump)
- [ ] `nika` (no args) shows contextual welcome (3 modes)
- [ ] `nika infer` shows spinner + TTFT + verb icon
- [ ] `nika invoke` pretty-prints JSON results as key-value on TTY
- [ ] No grep hits for ".nika/config.toml" in user-facing messages
- [ ] 11 commits (Phases 0-10), each with tests, each passing CI
