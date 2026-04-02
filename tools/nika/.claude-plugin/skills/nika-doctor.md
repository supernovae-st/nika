---
name: nika-doctor
description: Run Nika diagnostics and interpret results. Checks binary installation, API keys, project structure, MCP connectivity, and suggests fixes for each issue. Use when something is not working or for health checks.
user-invocable: true
allowed-tools: Bash, Read, Grep, Glob
argument-hint: "[--full | --fix | component]"
---

# Nika Doctor

> Diagnose and fix Nika installation and project issues.

## Process

### Step 1: Run Built-in Diagnostics

```bash
nika doctor --full 2>&1
```

Parse the output and categorize each check:
- Pass: no action needed
- Warn: explain the risk, suggest fix
- Fail: MUST be fixed, provide exact commands

### Step 2: Extended Diagnostics

Run additional checks that `nika doctor` does not cover:

#### Binary & Version

```bash
# Check nika is installed and accessible
which nika && nika --version

# Check compiled features
nika features 2>/dev/null || nika --features 2>/dev/null
```

#### API Key Availability

Check each provider WITHOUT triggering macOS Keychain popups:

```bash
# Check env vars only (safe, no keychain)
[ -n "$ANTHROPIC_API_KEY" ] && echo "anthropic: set" || echo "anthropic: NOT SET"
[ -n "$OPENAI_API_KEY" ] && echo "openai: set" || echo "openai: NOT SET"
[ -n "$MISTRAL_API_KEY" ] && echo "mistral: set" || echo "mistral: NOT SET"
[ -n "$GROQ_API_KEY" ] && echo "groq: set" || echo "groq: NOT SET"
[ -n "$DEEPSEEK_API_KEY" ] && echo "deepseek: set" || echo "deepseek: NOT SET"
[ -n "$GEMINI_API_KEY" ] && echo "gemini: set" || echo "gemini: NOT SET"
[ -n "$XAI_API_KEY" ] && echo "xai: set" || echo "xai: NOT SET"
```

**WARNING**: NEVER run `nika provider list` or `nika provider test` -- these trigger macOS Keychain popups.

#### Project Structure

```bash
# Check for .nika directory
ls -la .nika/ 2>/dev/null || echo ".nika/ directory not found"

# Check for workflow files
find . -name '*.nika.yaml' -maxdepth 5 2>/dev/null | head -20

# Check for config
cat nika.toml 2>/dev/null || echo "No project config (nika.toml not found)"

# Check for course installation
ls .nika/course-progress.toml 2>/dev/null || echo "No course installed"
```

#### Workspace Validation

```bash
# Validate all workflow files in the project
for f in $(find . -name '*.nika.yaml' -maxdepth 5 2>/dev/null); do
  RESULT=$(nika check "$f" 2>&1)
  RC=$?
  if [ $RC -ne 0 ]; then
    echo "FAIL: $f"
    echo "$RESULT" | head -5
  else
    echo "PASS: $f"
  fi
done
```

#### LSP Status

```bash
# Check if LSP feature is compiled in
nika lsp --help 2>/dev/null && echo "LSP: available" || echo "LSP: not available (compile with --features lsp)"
```

#### MCP Configuration

```bash
# Check project .mcp.json (preferred, Claude Code convention)
cat .mcp.json 2>/dev/null || echo "No .mcp.json at project root"

# Check global MCP config
cat ~/.nika/mcp.yaml 2>/dev/null || echo "No global MCP config"

# Check legacy project MCP config
cat .nika/mcp.yaml 2>/dev/null || echo "No legacy project MCP config"
```

### Step 3: Report

Present findings in this format:

```
Nika Doctor Report
==================

Binary:     nika v0.40.2 at /Users/.../.cargo/bin/nika
Features:   tui, media-core, lsp (3/5 compiled)
Project:    .nika/ found, 12 workflow(s)

Checks:
  [PASS] Binary installation
  [PASS] Anthropic API key
  [WARN] No OpenAI API key -- set OPENAI_API_KEY for multi-provider support
  [FAIL] 2 workflows have validation errors

Issues Found: 1 FAIL, 1 WARN

Suggested Fixes:
  1. Fix workflow validation errors:
     nika check path/to/broken.nika.yaml
     (see NIKA-XXX error code for details)

  2. Set OpenAI key (optional):
     export OPENAI_API_KEY="sk-..."
```

### Step 4: Offer Fixes

For each FAIL or WARN, offer to fix it:

| Issue | Auto-fix |
|-------|----------|
| Missing .nika dir | `nika init --minimal` |
| Workflow validation error | Edit the file to fix NIKA-XXX |
| Missing API key | Guide through `export VAR=...` |
| No MCP config | `nika mcp add <alias>` |
| Stale traces | `find .nika/traces -mtime +7 -delete` |

## Common Error Codes

| Code | Issue | Fix |
|------|-------|-----|
| NIKA-001 | Failed to parse workflow | Check YAML syntax and indentation |
| NIKA-002 | Invalid schema version | Add `schema: nika/workflow@0.12` |
| NIKA-020 | Circular dependency | Remove the cycle in depends_on |
| NIKA-022 | Duplicate task ID | Rename one of the duplicate tasks |
| NIKA-032 | Missing API key | Set an API key env var |
| NIKA-042 | Binding not found | Check with: alias matches a task ID |
| NIKA-053 | Command blocked | Remove dangerous command from exec: |
| NIKA-100 | MCP server not found | Add server to mcp: block |

## Rules

- NEVER trigger macOS Keychain popups (no `nika provider list/test`)
- ALWAYS run `nika doctor --full` first
- ALWAYS explain WHY each issue matters
- OFFER fixes, do not apply without confirmation
