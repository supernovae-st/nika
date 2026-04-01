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

**Testing command: ALWAYS `cd tools && cd tools && cargo test --workspace --lib`** (never without `--lib` — triggers macOS Keychain popups). The Cargo workspace root is at `tools/Cargo.toml`, NOT at the repo root. Current test count: **2153 tests passing** (as of 2026-04-01).

**Working directory:** Always `cd` to `tools/` before running cargo commands. The repo root has no Cargo.toml.

**Commit style:**
```
type(scope): description

Co-Authored-By: Claude <noreply@anthropic.com>
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

[mcp.novanet]
command = "cargo run -- mcp"
env = { NEO4J_URI = "bolt://localhost:7687" }

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
6. **Phase 7** last (docs, after everything else)

Each phase = 1 commit. `1 fix = 1 commit` rule.

---

## PHASE 1: nika.toml Foundation (CRITICAL PATH)

### Files to Modify

| # | File | Path | Lines | What |
|---|---|---|---|---|
| 1 | boot.rs | `tools/nika-engine/src/runtime/boot.rs` | 826 | Phase 1+2 rewrite, new types |
| 2 | config.rs | `tools/nika-cli/src/config.rs` | 332 | `find_nika_dir()` -> `find_project_root_from()` |
| 3 | init.rs | `tools/nika-cli/src/init.rs` | 187 | Create nika.toml, not .nika/config.toml |
| 4 | paths.rs | `tools/nika-engine/src/core/paths.rs` | 538 | GLOBAL_CONFIG const, path functions |
| 5 | tui config | `tools/nika-tui/src/config.rs` | 200 | Hardcoded path fix |
| 6 | tui startup | `tools/nika-tui/src/startup.rs` | 500 | Test fixtures |
| 7 | error.rs | `tools/nika-engine/src/error.rs` | 2796 | Error messages (grep "config.toml") |
| 8 | error_domains | `tools/nika-engine/src/error_domains.rs` | ? | NIKA-035 message |

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

Co-Authored-By: Claude <noreply@anthropic.com>
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

Add to BootstrapConfig:
```rust
#[serde(default)]
pub mcp: HashMap<String, McpServerConfig>,

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}
```

In boot.rs Phase 5 (MCP Startup): read from nika.toml [mcp.*] first, fallback to .nika/mcp.yaml.

### Commit

```
feat(mcp): read [mcp.*] server config from nika.toml
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

These commits landed AFTER the plan was drafted. The handoff agent must be aware:

1. **`824dbc038` — `nika tools list` + `nika tools info`** — New CLI subcommand. Added `tools/nika-cli/src/tools_cmd.rs`. Shows how to register new commands (model for `nika clean`).

2. **`15a147c76` — Agent file tools use project root** — `rig_agent_loop/mod.rs` now uses process cwd (project root) instead of workflow_base_dir for agent file tools. This is ALIGNED with our Phase 2 (working_dir). The fix already partially implements the "project root" concept for agents.

3. **`92206a7da` — Strip markdown fences from yaml/json artifacts** — Artifact processing changed. Phase 9 (artifact dir) must account for this.

4. **`8d622a68c` — Provider-aware extended thinking** — OpenAI reasoning models get different thinking params. Not directly relevant but shows provider dispatch complexity.

5. **`09136014d` — Path traversal blocked in from_example/schema** — Security hardening in structured output. Shows the security-first approach for path handling.

6. **Path resolution rule (E09/E19)** — Documented in `nika-bugs-and-patterns.md`: all relative paths resolve from PROJECT ROOT (where `nika run` is invoked), NOT from workflow file location. This is the EXISTING behavior and aligns with our `working_dir = "project"` default.

7. **Exec stderr (E29)** — `exec:` captures stdout ONLY. stderr discarded by design. Use `2>&1` redirect. Documented in rules.

8. **`| shell` transform (E30)** — Always use `{{with.data | shell}}` in `shell: true` exec tasks for safety.

## Critical Pitfalls

1. **`~/.config/nika/config.toml`** exists as ANOTHER config location (engine/config.rs, 14K lines). This is a DIFFERENT system from `.nika/config.toml`. Don't confuse them. The engine/config.rs handles global user preferences (model aliases, custom endpoints). Leave it alone for now.

2. **`nika-tui/src/config.rs` line 207** has hardcoded `PathBuf::from(".nika/config.toml")`. Must change to use `find_project_root_from()`.

3. **`paths.rs` line 60** has `pub const GLOBAL_CONFIG: &str = "config.toml"`. This is for `~/.nika/config.toml` (user-level). It stays as-is. Don't rename it to "nika.toml" — that's the project-level name.

4. **boot.rs Phase 1** currently walks up for `.nika/` dir. The new Phase 1 must walk up for `nika.toml` FIRST, then `.nika/` as fallback. Two separate loops (don't try to combine them — nika.toml and .nika/ could be at different levels).

5. **init.rs line 41** checks `nika_dir.join("config.toml").exists()` for idempotency. Change to check `cwd.join("nika.toml").exists()`.

6. **`BootstrapConfig` must NOT have `#[serde(deny_unknown_fields)]`** — unknown sections (like future `[packages]`, `[memory]`) must be silently ignored. Test 6 verifies this.

7. **Artifacts dir** currently defaults to `.nika/artifacts` (inside gitignored dir). Must change to `./artifacts` (visible, also gitignored via .gitignore). This is Phase 9, not Phase 1.

8. **Error messages** — grep `tools/nika-engine/src/error.rs` (2796 lines) for every instance of ".nika/config.toml" or "config.toml" and update to "nika.toml" where it refers to project config. Be careful: `~/.nika/config.toml` references should stay as-is.

9. **Course content** — `nika-init/src/course/missions.rs` line 1144 and 1160 reference `.nika/config.toml`. Update to `nika.toml`.

10. **Tests that create `.nika/config.toml`** — boot.rs tests, paths.rs tests, startup.rs tests all create `.nika/config.toml` in tempdir. Update them to create `nika.toml` instead (or test both for fallback behavior).

---

## Success Criteria

After all phases:
- [ ] `cd tools && cargo test --workspace --lib` passes (2153 existing tests + 43 new)
- [ ] `nika init` creates `nika.toml` (not `.nika/config.toml`)
- [ ] `nika run hello.nika.yaml` works in a fresh project
- [ ] `nika doctor` detects nika.toml and reports project root
- [ ] `nika doctor` detects legacy `.nika/config.toml` and suggests migration
- [ ] `nika clean` removes traces + cache + media
- [ ] `nika config list` shows nika.toml contents
- [ ] `nika` (no args) shows contextual welcome
- [ ] No grep hits for ".nika/config.toml" in user-facing messages (only internal/legacy fallback code)
- [ ] 10 commits, each with tests, each passing CI
