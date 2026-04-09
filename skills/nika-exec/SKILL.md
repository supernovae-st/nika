---
name: nika-exec
description: >-
  Expert at the Nika exec: verb for shell commands in .nika.yaml workflows.
  Covers shell: true with mandatory | shell escaping, NIKA-053 blocked
  commands, cwd/env/timeout config, stdout-only capture, and safe data
  injection patterns. Use when building exec: tasks or debugging NIKA-053
  security errors in Nika workflows (schema nika/workflow@0.12).
globs:
  - "**/*.nika.yaml"
---

# Nika exec: Verb Expert

The `exec:` verb runs a shell command and captures stdout as the task output.

## Syntax

### Short Form (command only)

```yaml
- id: build
  exec: "cargo build --release"
```

### Long Form (all options)

```yaml
- id: process
  exec:
    command: "python3 scripts/transform.py --input {{with.path | shell}}"
    shell: true          # boolean, NOT a shell name
    cwd: "./src"         # relative to project root, NOT workflow file location
    timeout: 60          # SECONDS (not milliseconds)
    env:
      NODE_ENV: production
      API_URL: "{{with.endpoint | shell}}"
```

## Critical: `shell: true` + `| shell` Transform

**When `shell: true`, EVERY `{{with.*}}` binding in the command MUST use `| shell`.**

```yaml
# ❌ BLOCKED — NIKA-053, even for "safe" values
exec:
  command: "echo {{with.message}}"
  shell: true

# ✅ CORRECT — | shell escapes shell metacharacters
exec:
  command: "echo {{with.message | shell}}"
  shell: true

# ✅ ALSO CORRECT — single quotes exempt the binding
exec:
  command: "jq --arg x '{{with.val}}' '.data'"
  shell: true

# ✅ NO shell: true needed for simple commands
exec:
  command: "cargo test"     # No template bindings, shell: true unnecessary
```

The `| shell` transform wraps values in single quotes with proper escaping — it does NOT execute anything.

## What `shell: true` Enables

Without `shell: true`, `command:` is executed directly as `argv[0..n]` — no pipes, redirects, or shell features.

```yaml
# Requires shell: true for pipes, redirects, &&, ||
exec:
  command: "grep -r 'error' logs/ | sort | uniq > errors.txt"
  shell: true

# Does NOT require shell: true
exec:
  command: "cargo test --workspace --lib"
```

## Blocked Patterns (NIKA-053)

These are always blocked to prevent injection:

| Pattern | Why blocked |
|---------|------------|
| `$()` in raw templates | Command substitution |
| Backticks `` `cmd` `` | Command substitution |
| `<(...)` process substitution | Shell process sub |
| `rm -rf /`, `sudo`, fork bombs | Command blocklist |

```yaml
# ❌ BLOCKED — $( in raw YAML template
exec:
  command: "jq --argjson data \"$(cat file.json)\" ..."
  shell: true

# ✅ CORRECT — jq with multiple file args, no substitution
exec:
  command: "jq -s '.[0] as $a | .[1] as $b | ...' file1.json file2.json"
  shell: true
```

**Exception:** `$()` in task DATA (already resolved) is allowed — only blocked in raw template text.

## Stdout Only

`exec:` captures **stdout only**. Use `2>&1` to include stderr:

```yaml
exec:
  command: "cargo build 2>&1"
  shell: true
```

## Data Injection — Use infer instead of Python

```yaml
# ❌ FRAGILE — French apostrophes break shell quoting
exec:
  command: "python3 -c \"data = '''{{with.script}}'''\""
  shell: true

# ✅ CORRECT — use infer for data transformation
- id: transform
  with:
    data: $fetch_result
  infer:
    prompt: "Merge these arrays into a flat list: {{with.data | to_json}}"
    temperature: 0.0
  structured:
    schema:
      type: object
      properties:
        items: { type: array }

# ✅ CORRECT — exec only for actual CLI tools, no data injection
- id: run_tool
  exec:
    command: "ffmpeg -y -i input.mp4 -c:a libmp3lame output.mp3"
    shell: true
```

## Path Resolution

All relative paths resolve from **project root** (where `nika run` is invoked), NOT from the workflow file's location.

```yaml
context:
  files:
    readme: ./README.md    # relative to <project_root>/README.md
```

## Common Mistakes

| Mistake | Fix |
|---------|-----|
| `shell: bash` | `shell: true` — it's a boolean |
| `timeout: 30000` (ms) | `timeout: 30` — always seconds |
| `{{with.val}}` in shell: true | `{{with.val \| shell}}` — mandatory escaping |
| `$(cat file)` in template | Use `jq -s` with file args instead |
| Expecting stderr in output | Add `2>&1` to command |
| `exec` for data transformation | Use `infer:` + `structured:` instead |
| Path relative to workflow file | Paths resolve from project root |

## Error Codes

| Code | Meaning | Fix |
|------|---------|-----|
| NIKA-053 | Blocked command (security) | Add `\| shell` to all template bindings |
| NIKA-028 | Exec timeout | Increase `timeout:` value |
| NIKA-029 | Non-zero exit code | Check command, add `\|\| true` if acceptable |

## Related Skills

- `/nika-workflow-syntax` — all 5 verbs quick reference
- `/nika-security` — Shell injection defense, NIKA-053 deep dive, trust levels
- `/nika-transforms` — The `| shell` transform and all 65 transforms
