# Troubleshooting Guide

When things go wrong with Nika, error messages include NIKA-XXX codes that point to the exact problem category. This guide covers common errors, their causes, and how to fix them.

## Quick Diagnostics

Before diving into specific errors, run these diagnostic commands:

```bash
# Check system health
nika doctor

# Full diagnostics (including slow MCP checks)
nika doctor --full

# Check compiled features
nika features

# Check provider status
nika provider list

# Validate a workflow without running
nika check workflow.nika.yaml
```

## Error Code Reference

Nika errors follow the pattern `[NIKA-XXX]` where XXX is a three-digit code. Here is the complete reference organized by category.

### Workflow Errors (000-009)

**NIKA-001: Failed to parse workflow**

```
[NIKA-001] Failed to parse workflow: expected key at line 5, column 3
```

**Cause:** Invalid YAML syntax.

**Fix:**
- Check indentation (YAML uses spaces, not tabs)
- Ensure strings with special characters are quoted
- Use a YAML linter or `nika check` to find the exact location

```yaml
# WRONG: unquoted string with special characters
exec: echo "hello: world"

# CORRECT: properly quoted
exec: 'echo "hello: world"'
```

**NIKA-002: Invalid schema version**

```
[NIKA-002] Invalid schema version: 0.12
```

**Cause:** The `schema:` field is missing or incorrect.

**Fix:** Always use the full schema string:

```yaml
# WRONG
schema: 0.12

# CORRECT
schema: nika/workflow@0.12
```

**NIKA-003: Workflow file not found**

```
[NIKA-003] Workflow file not found: path/to/workflow.yaml
```

**Cause:** The file path does not exist or has the wrong extension.

**Fix:**
- Check the file path
- Ensure the extension is `.nika.yaml` (not `.yaml` or `.yml`)

**NIKA-004: Workflow validation failed**

```
[NIKA-004] Workflow validation failed: tasks list is empty
```

**Cause:** Structural validation failed.

**Fix:** Check that the workflow has a non-empty `tasks:` list and all required fields.

### Schema Errors (010-019)

**NIKA-013: Schema file not found**

```
[NIKA-013] Schema file not found for task 'extract': ./schemas/output.json
```

**Cause:** A `structured:` or `output.schema_ref:` references a file that does not exist.

**Fix:** Create the schema file at the referenced path, relative to the workflow file.

### DAG Errors (020-029)

**NIKA-020: Cycle detected in DAG**

```
[NIKA-020] Cycle detected in DAG: task_a -> task_b -> task_a
```

**Cause:** Tasks have circular dependencies. Task A depends on B, and B depends on A.

**Fix:** Restructure the DAG to remove the cycle. Common patterns:
- Extract shared logic into a separate task that both depend on
- Use `depends_on:` only for genuine sequential dependencies

```yaml
# WRONG: circular dependency
tasks:
  - id: a
    depends_on: [b]
  - id: b
    depends_on: [a]

# CORRECT: use a shared source
tasks:
  - id: source
    exec: "echo 'shared data'"
  - id: a
    depends_on: [source]
  - id: b
    depends_on: [source]
```

**NIKA-021: Missing dependency**

```
[NIKA-021] Missing dependency: task 'consumer' depends on unknown 'prodcer'
```

**Cause:** A task references a dependency that does not exist (usually a typo).

**Fix:** Check the task ID spelling in `depends_on:` and `with:` blocks. Task IDs are case-sensitive.

**NIKA-022: Duplicate task ID**

```
[NIKA-022] Duplicate task ID: 'process' appears multiple times in workflow
```

**Cause:** Two tasks have the same `id:`.

**Fix:** Give each task a unique identifier.

**NIKA-026: Dependency chain failed**

```
[NIKA-026] Dependency chain failed: 3 task(s) blocked by failed dependencies
```

**Cause:** An upstream task failed, causing all downstream tasks to be skipped.

**Fix:** Fix the root failing task. The error message includes the root failure cause.

### Provider Errors (030-039)

**NIKA-030: Provider not configured**

```
[NIKA-030] Provider 'anthropic' not configured
```

**Cause:** The provider name is not recognized.

**Fix:** Use a valid provider ID: `anthropic`, `openai`, `mistral`, `groq`, `deepseek`, `gemini`, `xai`, `native`.

**NIKA-031: Provider API error**

```
[NIKA-031] Provider API error: 401 Unauthorized
```

**Cause:** The API returned an error.

**Fix:**
- Check that your API key is valid and not expired
- Verify you have sufficient credits/quota
- Check the model name is correct for the provider
- Try `nika provider test <provider>` to diagnose

**NIKA-032: Missing API key**

```
[NIKA-032] Missing API key for provider 'anthropic'
```

**Cause:** No API key found for the specified provider.

**Fix:**
```bash
# Set via environment variable
export ANTHROPIC_API_KEY="sk-ant-..."

# Or store in system keychain
nika keys set anthropic
```

### Template and Binding Errors (040-049)

**NIKA-041: Template error**

```
[NIKA-041] Template error in '{{with.data}}': could not be resolved
```

**Cause:** A template reference points to a binding that does not exist or has not been defined.

**Fix:** Ensure every `{{with.X}}` has a corresponding `with: { X: $source }` declaration.

**NIKA-042: Binding not found**

```
[NIKA-042] Binding 'data' not found
```

**Cause:** Task reference is missing the `$` prefix or the alias does not exist.

**Fix:**
```yaml
# WRONG
with:
  data: source_task

# CORRECT
with:
  data: $source_task
```

### Path and Security Errors (050-059)

**NIKA-050: Invalid path syntax**

**Cause:** A binding path has invalid syntax.

**Fix:** Check path expressions in `with:` blocks and template references.

**NIKA-052: Path not found**

**Cause:** A referenced path does not exist (task may not have JSON output).

**Fix:** Check paths relative to the workflow file location and verify upstream task output.

**NIKA-053: Command blocked**

```
[NIKA-053] Command blocked: 'rm -rf /' - matches security blocklist
```

**Cause:** A shell command matches Nika's security blocklist.

**Fix:** Nika blocks dangerous commands by default (e.g., `rm -rf /`). If your command is legitimate, restructure it to avoid matching the blocklist pattern.

### Output Errors (060-069)

**NIKA-060: Output validation failed**

```
[NIKA-060] Output validation failed: expected JSON, got plain text
```

**Cause:** Task output does not match the expected format.

**Fix:** Ensure the LLM produces output matching the `output: { format: json }` or `structured:` specification. Add `max_retries` for automatic retries.

### With Block Validation (070-089)

**NIKA-071: Unknown alias in with: block**

**Cause:** A template references an alias not declared in the `with:` block.

**Fix:** Check the `with:` block syntax:
```yaml
with:
  alias: $task_id              # Simple reference
  alias: $task_id | transform  # With transform
  alias: $task_id.field        # With JSONPath
  alias: $env.VAR_NAME         # Environment variable
```

### JSONPath and Execution Errors (090-099)

**NIKA-090: JSONPath error**

```
[NIKA-090] JSONPath '$.nonexistent.field' returned null
```

**Cause:** The JSONPath expression does not match any value in the data.

**Fix:**
- Check that the upstream task produces the expected JSON structure
- Use the `?? default` fallback operator: `$task.data.field ?? "default"`
- Run the upstream task independently to verify its output

**NIKA-096: Execution error**

```
[NIKA-096] Execution error: command exited with status 1
```

**Cause:** A shell command failed (non-zero exit code).

**Fix:**
- Run the command manually to see the full error output
- Check that required tools are installed
- Verify file paths and permissions

### MCP Errors (100-109)

**NIKA-100: MCP connection failed**

```
[NIKA-100] MCP connection failed: could not connect to server 'github'
```

**Cause:** The MCP server could not be started or connected to.

**Fix:**
- Check that the MCP server command is installed (`npx`, etc.)
- Verify environment variables in the `mcp:` block
- Test the connection: `nika mcp test workflow.yaml server_name`

**NIKA-101: MCP tool not found**

```
[NIKA-101] MCP tool not found: 'github::nonexistent_tool'
```

**Cause:** The specified tool does not exist on the MCP server.

**Fix:**
- List available tools: `nika mcp tools workflow.yaml server_name`
- Check tool name spelling (case-sensitive)

### Agent Errors (110-119)

**NIKA-115: Agent execution failed**

```
[NIKA-115] Agent execution failed for task 'researcher': exceeded max_turns (20)
```

**Cause:** The agent used all allowed iterations without completing.

**Fix:**
- Increase `max_turns`
- Simplify the agent's goal
- Add clearer stop conditions with `completion: { on_tool: done }`

**NIKA-112: Guardrail violation**

```
[NIKA-112] Guardrail violation: output has 45 words, minimum is 50
```

**Cause:** The agent's output failed a guardrail check.

**Fix:**
- Adjust the prompt to be more specific about requirements
- Adjust guardrail thresholds
- Set `on_failure: retry` to let the agent self-correct

### Resilience Errors (120-129)

**NIKA-120: Max retries exceeded**

```
[NIKA-120] Max retries exceeded (3 attempts) for task 'api_call'
```

**Cause:** A task failed all retry attempts.

**Fix:**
- Increase `max_attempts` in the retry configuration
- Increase `timeout` for slow operations
- Check if the external service is down

### Transform Errors (150-153)

**NIKA-151: Transform parse error**

```
[NIKA-151] Transform parse error in 'unknown_transform': unknown transform
```

**Cause:** Invalid transform name in a pipe expression.

**Fix:** Use valid transform names. See the [complete catalog](04-workflow-patterns.md#complete-transform-catalog).

**NIKA-152: Type mismatch**

```
[NIKA-152] Transform 'upper' failed: expected string, got number
```

**Cause:** A transform was applied to the wrong data type.

**Fix:** Use `to_string` before string transforms, or check upstream data types:
```yaml
with:
  text: $number_task | to_string | upper
```

**NIKA-153: Null input**

```
[NIKA-153] Transform 'upper' received null — use default() to handle
```

**Cause:** A transform received a null value.

**Fix:** Add `default()` before the failing transform:
```yaml
with:
  safe: $maybe_null | default("fallback") | upper
```

### Media Errors (251-297)

**NIKA-251: Invalid MIME type**

**Cause:** The imported file has an unrecognized format.

**Fix:** Check the file is a supported format (JPEG, PNG, WebP, SVG, PDF, etc.).

**NIKA-290: Media tool error**

**Cause:** A media tool operation failed.

**Fix:** Check:
- The hash references a valid CAS file
- Required feature flags are compiled in (`nika features`)
- The file is not corrupted

**NIKA-291: Invalid format**

**Cause:** Unsupported target format for conversion.

**Fix:** Use supported formats: `png`, `jpeg`, `webp`.

### Structured Output Errors (300-309)

**NIKA-300: Schema validation failed**

```
[NIKA-300] Structured output validation failed: missing required field 'name'
```

**Cause:** The LLM output does not match the JSON Schema.

**Fix:**
- Enable repair: `enable_repair: true`
- Increase retries: `max_retries: 3`
- Simplify the schema
- Make the prompt more explicit about required fields

### Course Errors (310-319)

**NIKA-310: Not inside a Nika project**

**Cause:** Course commands run outside a project directory.

**Fix:** Navigate to a directory with `.nika/` or `01-jailbreak/`, or run `nika init --course`.

## Common Scenarios

### "My workflow works locally but fails in CI"

- Ensure API keys are set as CI/CD environment variables
- Check that the CI environment has network access for `fetch:` tasks
- Use `--provider mock` for testing workflow structure without real API calls
- Add `timeout:` fields for operations that may take longer in CI

### "LLM output is inconsistent"

- Lower `temperature` for more deterministic output
- Use `structured:` for reliable JSON output
- Add guardrails for output validation
- Set `max_tokens` to prevent cut-off responses

### "MCP server won't connect"

```bash
# Test the connection
nika mcp test workflow.yaml server_name

# List available tools
nika mcp tools workflow.yaml server_name

# Check strict mode
nika check workflow.yaml --strict
```

Verify:
- The server command is installed (run it manually)
- Environment variables are correctly set
- No firewall is blocking connections

### "Workflow is slow"

- Check which tasks are sequential vs parallel (use DAG visualization)
- Add `concurrency` limits to `for_each` to control parallelism
- Use `timeout` to fail-fast on hung operations
- Consider using faster providers (Groq) for simple tasks
- Use `--detail min` to reduce output rendering overhead

### "Template not resolving"

```yaml
# Check that the binding exists
with:
  data: $source_task    # $ prefix required

# Check that the template uses the right alias
exec: "echo '{{with.data}}'"    # Must match the with: key
```

### "Task output is empty or has extra whitespace"

Shell commands often include trailing newlines. Use `| trim`:

```yaml
with:
  clean: $shell_task | trim
```

## Verbose Logging

Increase log verbosity to see what Nika is doing internally:

```bash
# Info level
nika -v run workflow.nika.yaml

# Debug level (shows template resolution, DAG decisions)
nika -vv run workflow.nika.yaml

# Trace level (shows everything including MCP messages)
nika -vvv run workflow.nika.yaml
```

## Execution Traces

Every workflow run produces a trace file in `.nika/traces/`. Inspect traces for post-mortem debugging:

```bash
# List recent traces
nika trace list

# Show trace details
nika trace show <trace-id>

# Export to JSON for analysis
nika trace export <trace-id> --format json
```

Traces include:
- Task start/end times
- Input bindings (resolved values)
- Output values
- Error details
- Token usage (for LLM tasks)

## Getting Help

If you cannot resolve an issue:

1. Run `nika doctor --full` and review the output
2. Check the error code in this guide
3. Use `nika -vvv run workflow.yaml` for maximum debug info
4. Check the [FAQ](13-faq.md) for common questions
5. File an issue at https://github.com/supernovae-st/nika/issues
