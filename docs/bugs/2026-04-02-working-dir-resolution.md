# BUG: Working Directory Resolution — exec/nika:read/from_example ignore nika.toml

**Date**: 2026-04-02
**Severity**: P0 — blocks standard project layouts
**Discovered in**: nk-jungo (i18n translation project, 216 locales)
**Nika version**: v0.62.0
**Reporter**: Thibaut Melen

---

## Problem

When a workflow lives in `workflows/translate.nika.yaml`, all relative paths in
`exec:`, `invoke: nika:read`, and `structured: from_example:` resolve from the
**workflow file's directory** (`workflows/`), not the **project root** (where `nika.toml` lives).

This forces users to either:
1. Put all workflows at the project root (breaks convention)
2. Use `../` everywhere (fragile, and blocked by nika:read + from_example security)
3. Duplicate workflows at root AND in workflows/ with different paths

The `nika.toml` setting `[tools] working_dir = "project"` exists but is **never read by the CLI**.

## Reproduction

```
my-project/
├── nika.toml              # [tools] working_dir = "project"
├── src/en-US/ui.json
├── locales/fr-fr/...
└── workflows/
    └── translate.nika.yaml
```

```yaml
# workflows/translate.nika.yaml
tasks:
  - id: source
    exec:
      command: "cat ./src/en-US/ui.json"   # ← FAILS (resolves from workflows/)
      shell: true

  - id: read
    invoke:
      tool: "nika:read"
      params:
        file_path: "./src/en-US/ui.json"   # ← FAILS (NIKA-208, resolves from workflows/)

  - id: read_parent
    invoke:
      tool: "nika:read"
      params:
        file_path: "../src/en-US/ui.json"  # ← FAILS (NIKA-204, path traversal blocked)

  - id: translate
    structured:
      from_example: ./src/en-US/ui.json    # ← FAILS (path traversal blocked)
```

**Expected**: All `./` paths resolve from project root (where `nika.toml` is) when
`[tools] working_dir = "project"` is set.

**Actual**: All `./` paths resolve from `workflows/` (the workflow file's parent directory).

## Root Cause

### 1. CLI doesn't pass project_root to the runner

`tools/nika/src/main.rs:2692`:
```rust
let mut runner = Runner::new(workflow)?
    .with_base_path(base_path.to_path_buf())  // ← workflow file's parent dir
    .with_permission_mode(perm_mode);
// MISSING: .with_project_root(project_root)
// MISSING: .with_working_dir_mode("project")
```

The runner has `with_project_root()` and `with_working_dir_mode()` methods (used in tests at
`nika-engine/src/runtime/tests_e2e_workflow.rs:1422-1423`) but the CLI never calls them.

### 2. nika:read/from_example use base_path for security boundary

`nika:read` validates that the file is within `working_dir`. Since `working_dir` defaults to
`base_path` (workflow dir), any file outside that dir is blocked with NIKA-204.

`from_example` uses the same boundary check and rejects `../` paths.

### 3. exec CWD is set to base_path

The exec command spawns a shell with CWD = `base_path`. When `working_dir = "project"` should
make CWD = project root instead.

## Proposed Fix

In `tools/nika/src/main.rs`, in the `run_workflow()` function:

```rust
// After line 2692, add:
let project_root = cli::config::find_project_root_from(
    &std::env::current_dir().unwrap_or_default()
).ok();

let mut runner = Runner::new(workflow)?
    .with_base_path(base_path.to_path_buf())
    .with_permission_mode(perm_mode);

// NEW: wire project root + working_dir mode from nika.toml
if let Some(ref proj) = project_root {
    runner = runner.with_project_root(proj.root.clone());
    if let Some(ref config) = bootstrap_config {
        if let Some(ref wd) = config.tools.working_dir {
            runner = runner.with_working_dir_mode(wd.clone());
        }
    }
}
```

This makes:
- `exec:` CWD = project root (when `working_dir = "project"`)
- `nika:read` security boundary = project root
- `from_example` path resolution = project root

## Impact

Every nika project with workflows in a `workflows/` subdirectory is affected.
The `nika init` command creates `workflows/hello.nika.yaml` by default — which
means the default project layout hits this bug immediately if the workflow needs
to read files from the project root.

## Workarounds

1. **Put workflows at project root** (not in `workflows/`). Ugly but works.
2. **Use `exec: cat` instead of `nika:read`**. Shell exec respects the shell's CWD
   which can be set via `cd` before `nika run`. But nika:read is supposed to be the
   safe alternative.
3. **Use absolute paths**. Works but non-portable between machines.

## Related

- NIKA-204: Path traversal blocked in nika:read
- NIKA-208: File not found (because wrong base directory)
- BUG-010: fetch json: body loses array type (forces exec: curl workaround)
- `nika-engine/src/runtime/tests_e2e_workflow.rs:1401-1439`: Tests exist for
  `working_dir = "project"` — the engine supports it, the CLI just doesn't wire it.

## Tests to Add

```rust
#[tokio::test]
async fn cli_run_workflow_respects_working_dir_project() {
    // Create project with nika.toml at root
    // Put workflow in workflows/ subdir
    // Set [tools] working_dir = "project" in nika.toml
    // Run workflow that does exec: "cat ./src/file.txt"
    // Assert: file is found (CWD = project root, not workflows/)
}
```
