# Top 30 Common Mistakes

> Every mistake new Nika users make, why it happens, and how to fix it. Each one includes the relevant NIKA error code when applicable.

---

## YAML Syntax Mistakes

### 1. Wrong file extension

**The mistake**: Saving workflows as `.yaml` instead of `.nika.yaml`.

**Why it happens**: Habit from other YAML tools. Nika requires `.nika.yaml` to distinguish workflow files from configuration files.

**The fix**: Always use `.nika.yaml`:
```
my-workflow.nika.yaml     # Correct
my-workflow.yaml           # Wrong -- Nika will not recognize it
```

**Error**: The file will not be recognized by `nika course check` or the showcase system.

---

### 2. Missing schema declaration

**The mistake**: Forgetting the `schema:` line at the top of the workflow file.

**Why it happens**: It seems like boilerplate. But the schema declaration is how Nika knows which features are available and how to validate the file.

**The fix**: Always start with:
```yaml
schema: "nika/workflow@0.12"
```

**Error**: `NIKA-002` Invalid schema version / `NIKA-004` Validation error

---

### 3. Incorrect YAML indentation

**The mistake**: Mixing tabs and spaces, or using the wrong indentation level.

```yaml
# Wrong -- mixed indentation
tasks:
  - id: hello
      exec: echo "hi"    # Extra indent breaks parsing
```

**Why it happens**: YAML is whitespace-sensitive. Most editors default to tabs, but YAML requires spaces.

**The fix**: Use 2-space indentation consistently. Configure your editor:
```yaml
# Correct
tasks:
  - id: hello
    exec: echo "hi"
```

**Error**: `NIKA-001` Failed to parse workflow

---

### 4. Unquoted special characters

**The mistake**: Not quoting strings that contain YAML special characters (`{`, `}`, `:`, `#`).

```yaml
# Wrong -- the {{ triggers YAML parsing issues
exec: echo "Hello {{with.name}}"
```

**Why it happens**: YAML interprets `{` as the start of a flow mapping.

**The fix**: Quote strings containing special characters:
```yaml
exec: 'echo "Hello {{with.name}}"'
# Or use the full form:
exec:
  command: 'echo "Hello {{with.name}}"'
  shell: true
```

**Error**: `NIKA-001` Failed to parse workflow / `NIKA-095` YAML parse error

---

### 5. Using `schema:` instead of `nika:` in older docs

**The mistake**: Using `nika: workflow@0.12` instead of `schema: "nika/workflow@0.12"`.

**Why it happens**: The schema format changed. Older examples may use different syntax.

**The fix**: The current format is:
```yaml
schema: "nika/workflow@0.12"
```

**Error**: `NIKA-002` Invalid schema version

---

## Binding and Template Mistakes

### 6. Forgetting the `$` prefix in `with:` bindings

**The mistake**: Referencing a task output without the `$` prefix.

```yaml
# Wrong
with:
  data: fetch_api    # Missing $ prefix
```

**Why it happens**: The `$` prefix is a Nika convention, not a standard YAML feature.

**The fix**: Always use `$` when referencing task outputs:
```yaml
# Correct
with:
  data: $fetch_api
```

**Error**: `NIKA-080` with.alias references unknown task

---

### 7. Using `with:` without `depends_on:`

**The mistake**: Binding a task's output without declaring it as a dependency.

```yaml
# Wrong -- no depends_on
- id: process
  with:
    result: $fetch_data
  exec: echo "{{with.result}}"
```

**Why it happens**: It seems like `with:` should imply a dependency. But Nika requires explicit ordering because the DAG scheduler needs to know the execution order.

**The fix**: Always pair `with:` with `depends_on:`:
```yaml
# Correct
- id: process
  depends_on: [fetch_data]
  with:
    result: $fetch_data
  exec: echo "{{with.result}}"
```

**Error**: `NIKA-081` with.alias is not upstream of task

---

### 8. Misspelling template variables

**The mistake**: Typos in `{{with.alias}}` expressions that do not match any declared alias.

```yaml
with:
  data: $api_call
exec: echo "{{with.dta}}"    # Typo: 'dta' instead of 'data'
```

**Why it happens**: Template variables are strings, not type-checked identifiers.

**The fix**: Double-check alias names. The error message will tell you which alias was not found.

**Error**: `NIKA-071` Unknown alias '{{with.alias}}' - not declared in with: block

---

### 9. Accessing non-existent JSON paths

**The mistake**: Using dot notation to access fields that do not exist in the task output.

```yaml
with:
  response: $api_call
exec: echo "{{with.response.data.users[0].nonexistent_field}}"
```

**Why it happens**: The task output structure may not match your expectations.

**The fix**: Test the raw output first, then narrow down the path:
```yaml
# First, see what the raw output looks like:
exec: echo "{{with.response}}"

# Then add specific paths once you know the structure
```

**Error**: `NIKA-073` Cannot traverse 'segment' on value_type / `NIKA-052` Path not found

---

### 10. Template syntax in non-template contexts

**The mistake**: Using `{{...}}` in YAML keys or in places where templates are not processed.

```yaml
# Wrong -- template in task id
- id: "{{inputs.name}}_task"
  exec: echo "hi"
```

**Why it happens**: Templates work in values (prompts, commands, URLs, etc.) but not in structural fields like task IDs.

**The fix**: Use templates only in value positions:
```yaml
# Correct
- id: my_task
  exec:
    command: echo "Hello {{inputs.name}}"
    shell: true
```

**Error**: `NIKA-055` Invalid task ID / `NIKA-041` Template error

---

## Provider and Model Mistakes

### 11. Missing API key

**The mistake**: Running a workflow with `infer:` or `agent:` without setting the provider's API key.

**Why it happens**: API keys must be set as environment variables before running.

**The fix**: Set the environment variable:
```bash
export ANTHROPIC_API_KEY="sk-ant-..."
# Or
export OPENAI_API_KEY="sk-..."
```

Verify with: `nika provider list`

**Error**: `NIKA-032` Missing API key for provider

---

### 12. Wrong provider/model format

**The mistake**: Using incorrect provider names or model IDs.

```yaml
# Wrong
provider: "claude"        # Not a valid provider name
model: "sonnet"           # Not a valid model ID
```

**Why it happens**: Provider names and model IDs are specific strings that must match exactly.

**The fix**: Use correct names:
```yaml
# Correct
provider: anthropic
model: claude-sonnet-4-20250514

# Or the combined format:
model: anthropic/claude-sonnet-4-20250514
```

**Error**: `NIKA-030` Provider not configured / `NIKA-033` Invalid configuration

---

### 13. Using `native` provider without a model file

**The mistake**: Specifying `provider: native` without downloading a GGUF model first.

**The fix**: Download a model first:
```bash
nika model pull mistral-7b
# Or specify the path directly:
model: "~/.nika/models/mistral-7b-instruct-v0.2.Q4_K_M.gguf"
```

**Error**: `NIKA-030` Provider not configured

---

### 14. GGUF model for vision tasks

**The mistake**: Trying to use a GGUF model for vision (image) tasks. GGUF models are text-only.

**The fix**: Use `NativeModelKind::VisionHf` with a HuggingFace model:
```bash
nika model vision Qwen/Qwen2.5-VL-7B-Instruct --isq Q4K
```

Or use a cloud provider that supports vision: Claude, OpenAI, Gemini, Mistral, Groq, xAI.

**Error**: VisionNotSupported error

---

## DAG and Dependency Mistakes

### 15. Circular dependencies

**The mistake**: Creating a dependency cycle where Task A depends on Task B and Task B depends on Task A.

```yaml
tasks:
  - id: a
    depends_on: [b]
    exec: echo "A"
  - id: b
    depends_on: [a]
    exec: echo "B"
```

**Why it happens**: Complex workflows with many dependencies make cycles easy to introduce accidentally.

**The fix**: Draw the dependency graph on paper first. Dependencies must form a DAG (directed acyclic graph) -- no loops.

**Error**: `NIKA-020` Cycle detected in DAG

---

### 16. Duplicate task IDs

**The mistake**: Using the same `id:` for two different tasks.

```yaml
tasks:
  - id: fetch_data
    fetch: "https://api.example.com/users"
  - id: fetch_data      # Duplicate!
    fetch: "https://api.example.com/products"
```

**The fix**: Every task ID must be unique within a workflow.

**Error**: `NIKA-022` Duplicate task ID

---

### 17. Referencing non-existent task IDs in `depends_on:`

**The mistake**: Misspelling a task ID in the `depends_on:` list.

```yaml
- id: process
  depends_on: [fech_data]    # Typo: 'fech' instead of 'fetch'
```

**The fix**: Double-check task IDs. Use `nika check` to catch these before running.

**Error**: `NIKA-021` Missing dependency: task depends on unknown task

---

## Execution Mistakes

### 18. Timeout in wrong units

**The mistake**: Thinking `timeout: 30` means 30 milliseconds.

**Why it happens**: Many systems use milliseconds. Nika uses seconds for the `timeout:` field.

**The fix**: `timeout: 30` means 30 seconds. The parser converts to milliseconds internally.

```yaml
exec:
  command: "long-running-process"
  timeout: 30    # 30 seconds, not 30ms
```

---

### 19. Forgetting `shell: true` for pipes and chaining

**The mistake**: Using shell features (`|`, `&&`, `$VAR`) without enabling the shell.

```yaml
# Wrong -- pipes need shell: true
exec: "cat file.txt | grep 'pattern'"
```

**Why it happens**: By default, `exec:` runs commands directly without a shell interpreter.

**The fix**: Add `shell: true`:
```yaml
exec:
  command: "cat file.txt | grep 'pattern'"
  shell: true
```

**Error**: `NIKA-096` Execution error (command not found)

---

### 20. Running `cargo test` instead of `cargo test --lib`

**The mistake**: Running `cargo test` which triggers contract tests that open macOS Keychain popups.

**Why it happens**: `cargo test` runs all tests including integration tests by default.

**The fix**: Always use `cargo test --lib` for safe testing (8,300+ tests safe).

```bash
cargo test --workspace --lib    # Safe -- no keychain (8,300+ tests)
cargo test --lib                # nika binary tests only
```

---

## Security Mistakes

### 21. Hardcoding API keys in YAML

**The mistake**: Putting secrets directly in workflow files.

```yaml
# NEVER DO THIS
tasks:
  - id: call_api
    fetch:
      url: "https://api.example.com/data"
      headers:
        Authorization: "Bearer sk-abc123..."
```

**The fix**: Use environment variable bindings:
```yaml
tasks:
  - id: call_api
    with:
      token: $env.API_TOKEN
    fetch:
      url: "https://api.example.com/data"
      headers:
        Authorization: "Bearer {{with.token}}"
```

---

### 22. Using blocked commands

**The mistake**: Trying to run dangerous commands like `rm -rf /` in `exec:`.

**Why it happens**: The command blocklist prevents destructive operations.

**The fix**: Use safe alternatives. If you need to delete files, be specific about paths.

**Error**: `NIKA-053` Command blocked

---

### 23. Path traversal in media imports

**The mistake**: Importing files with `../` paths that escape the workflow directory.

**The fix**: Use `validate_import_path()` (handled automatically by the engine). Keep imports within the workflow directory.

**Error**: `NIKA-290` Media security error

---

## Agent Mistakes

### 24. No safety limits on agents

**The mistake**: Running an agent without `max_turns`, `token_budget`, or `max_cost_usd`.

```yaml
# Dangerous -- no limits
agent:
  prompt: "Research everything about AI."
  tools: [builtin]
```

**Why it happens**: It seems optional. But without limits, an agent can loop indefinitely and consume unlimited tokens.

**The fix**: Always set safety limits:
```yaml
agent:
  prompt: "Research AI trends."
  tools: [builtin]
  max_turns: 10
  token_budget: 8000
  completion:
    mode: explicit
```

**Error**: No error, but your API bill will be the error.

---

### 25. Using `natural` completion for complex tasks

**The mistake**: Using `completion: mode: natural` for tasks that require specific output formatting.

**Why it happens**: `natural` seems simpler. But the agent may stop before producing the required output.

**The fix**: Use `explicit` mode with `nika:complete` for any task that needs specific output:
```yaml
completion:
  mode: explicit
  signal:
    tool: nika:complete
    fields:
      required: [result]
```

---

### 26. Not providing enough context in agent prompts

**The mistake**: Giving vague instructions to agents.

```yaml
# Too vague
agent:
  prompt: "Do something useful."
  tools: [builtin]
```

**The fix**: Be specific about the mission, the tools to use, and the expected output:
```yaml
agent:
  prompt: |
    You are a code review agent. Your mission:
    1. Read the file at "src/main.rs" using nika_read
    2. Log any issues found using nika_log
    3. Call nika_complete with a structured review
  tools: [builtin]
```

---

## Output and Artifact Mistakes

### 27. Using `format: json` when you need `format: json_schema`

**The mistake**: Using `json` (any valid JSON) when you need a specific structure.

**Why it happens**: `json` seems like it should work. But it only guarantees valid JSON, not the specific shape you need.

**The fix**: Use `json_schema` with an explicit schema:
```yaml
output:
  format: json_schema
  schema:
    type: object
    properties:
      name: { type: string }
    required: [name]
```

**Error**: `NIKA-061` Schema validation failed (only with `json_schema`)

---

### 28. Forgetting `enable_retry: true` for structured output

**The mistake**: Not enabling retry when using `json_schema` output. The first LLM attempt may fail validation.

**The fix**: Always enable retry for schema-validated output:
```yaml
output:
  format: json_schema
  schema: { ... }
  enable_retry: true
  max_retry_attempts: 3
```

---

## Media Pipeline Mistakes

### 29. Using `image::load_from_memory()` directly

**The mistake**: In custom code, loading images without size limits or validation.

**The fix**: Always use `decode_image_safe()` from `media/safety.rs`, which applies size limits and prevents decompression bombs.

**Error**: `NIKA-290` Media security error

---

### 30. SVG without sanitization

**The mistake**: Parsing SVG files without running them through the sanitizer first.

**Why it happens**: SVG files can contain embedded scripts, external references, and other dangerous content.

**The fix**: Always call `sanitize_svg()` before `usvg` parsing. This is handled automatically by `nika:svg_render`.

**Error**: `NIKA-297` Media security error

---

## Quick Reference: Error Code to Mistake

| Error | Mistake # | Category |
|-------|-----------|----------|
| NIKA-001 | 3, 4 | YAML syntax |
| NIKA-002 | 2, 5 | Schema |
| NIKA-020 | 15 | DAG |
| NIKA-021 | 17 | DAG |
| NIKA-022 | 16 | DAG |
| NIKA-030 | 12, 13 | Provider |
| NIKA-032 | 11 | Provider |
| NIKA-041 | 10 | Template |
| NIKA-053 | 22 | Security |
| NIKA-055 | 10 | Task ID |
| NIKA-061 | 27 | Output |
| NIKA-071 | 8 | Binding |
| NIKA-073 | 9 | Binding |
| NIKA-080 | 6 | Binding |
| NIKA-081 | 7 | DAG |
| NIKA-096 | 19 | Execution |

---

*"The fastest way to learn is to make every mistake once. The smartest way is to read this page first."*
