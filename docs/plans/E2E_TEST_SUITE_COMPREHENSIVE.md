# Nika E2E Test Suite — Comprehensive Feature Checklist

A complete inventory of every testable feature in Nika v0.12 workflows with YAML snippets.
Total: **54 test scenarios** covering all 5 verbs, data flow, control flow, resilience, security, and advanced features.

---

## 1. INFER VERB (9 tests)

### 1.1 — Infer: Short Form with Default Model
```yaml
- id: infer_short
  infer: "Reply: HELLO"
```
**Tests:** Basic string prompt, model/provider from workflow header.

### 1.2 — Infer: Full Form with System Prompt
```yaml
- id: infer_system
  infer:
    prompt: "What is AI?"
    system: "Be concise. Max 2 sentences."
    temperature: 0.5
    max_tokens: 100
```
**Tests:** System prompt, temperature, max_tokens, full form syntax.

### 1.3 — Infer: Response Format JSON
```yaml
- id: infer_json_format
  infer: "Return JSON: {name: 'Nika', version: 30}"
  output:
    format: json
```
**Tests:** `output.format: json` (formatting only, no validation).

### 1.4 — Infer: Structured Output with Schema
```yaml
- id: infer_structured
  infer: "Extract product: name, price, in_stock."
  structured:
    schema:
      type: object
      properties:
        name: { type: string }
        price: { type: number }
        in_stock: { type: boolean }
      required: [name, price]
    enable_repair: true
    max_retries: 2
    repair_model: claude-haiku-4-5
```
**Tests:** Schema validation, repair, retry on violation, repair model override.

### 1.5 — Infer: Extended Thinking (Claude)
```yaml
- id: infer_thinking
  infer: "Solve: √2 + π = ?"
  extended_thinking: true
  thinking_budget: 5000
  provider: anthropic
```
**Tests:** Extended thinking, thinking budget allocation, Claude-specific feature.

### 1.6 — Infer: Multimodal/Vision Content
```yaml
- id: infer_vision
  infer:
    content:
      - type: image
        source: "{{with.photo_hash}}"
        detail: high
      - type: text
        text: "Describe this image in detail"
  provider: anthropic
```
**Tests:** Multimodal `content:` array, image binding from CAS hash, vision support.

### 1.7 — Infer: Guardrails on Output (Length)
```yaml
- id: infer_guardrails
  infer: "Write exactly 5 bullet points."
  guardrails:
    - type: length
      min_words: 50
      max_words: 500
```
**Tests:** Guardrail validation, length constraints on infer output.

### 1.8 — Infer: Provider Override at Task Level
```yaml
- id: infer_provider_override
  infer: "Hello"
  provider: mistral
  model: mistral-large-latest
```
**Tests:** Task-level provider override, model selection.

### 1.9 — Infer: Template Interpolation in Prompt
```yaml
- id: infer_template
  depends_on: [source_task]
  with:
    data: $source_task
  infer: "Summarize: {{with.data}}"
```
**Tests:** Template binding in prompt, task data flow.

---

## 2. EXEC VERB (8 tests)

### 2.1 — Exec: Short Form (No Shell)
```yaml
- id: exec_basic
  exec: "echo hello"
```
**Tests:** Basic command, default no-shell mode.

### 2.2 — Exec: Full Form with Shell=true
```yaml
- id: exec_shell
  exec:
    command: "echo 'test' | grep 'es'"
    shell: true
```
**Tests:** `shell: true` enables pipes and redirects.

### 2.3 — Exec: Environment Variables
```yaml
- id: exec_env
  with:
    home: $env.HOME
  exec:
    command: "echo HOME={{with.home}}"
    shell: true
    env:
      CUSTOM: "value"
```
**Tests:** `$env.*` binding, env override in exec.

### 2.4 — Exec: Working Directory (cwd)
```yaml
- id: exec_cwd
  exec:
    command: "pwd"
    cwd: "/tmp"
```
**Tests:** Working directory change, cwd field.

### 2.5 — Exec: Timeout (Seconds)
```yaml
- id: exec_timeout
  exec:
    command: "sleep 100"
    timeout: 5
```
**Tests:** Timeout in seconds, command termination.

### 2.6 — Exec: Intentional Failure (Error Independence)
```yaml
- id: exec_fail
  exec: "exit 1"

- id: exec_independent
  exec: "echo OK"
```
**Tests:** Task failures don't block independent tasks (DAG independence).

### 2.7 — Exec: Template in Command
```yaml
- id: exec_template
  depends_on: [upstream]
  with:
    data: $upstream | trim
  exec: "echo '{{with.data}}'"
```
**Tests:** Template interpolation in exec command.

### 2.8 — Exec: Security Blocklist (Blocked Command)
```yaml
- id: exec_blocked
  exec: "rm -rf /"
  # Should fail with NIKA-053 (Blocked command)
```
**Tests:** Command blocklist (rm -rf, sudo, fork bombs).

---

## 3. FETCH VERB (11 tests)

### 3.1 — Fetch: Simple GET
```yaml
- id: fetch_get
  fetch:
    url: "https://httpbin.org/get?test=1"
    method: GET
```
**Tests:** Basic GET, URL interpolation.

### 3.2 — Fetch: POST with JSON Body
```yaml
- id: fetch_post
  fetch:
    url: "https://httpbin.org/post"
    method: POST
    headers:
      Authorization: "Bearer TOKEN"
    json:
      key: value
      nested:
        field: data
```
**Tests:** POST, JSON body auto-serialization, headers.

### 3.3 — Fetch: POST with Raw String Body
```yaml
- id: fetch_body_string
  fetch:
    url: "https://httpbin.org/post"
    method: POST
    body: "raw text body"
```
**Tests:** Raw string body (vs json auto-serialization).

### 3.4 — Fetch: Response Full (JSON with Headers)
```yaml
- id: fetch_full_response
  fetch:
    url: "https://httpbin.org/get"
    response: full
  # Returns: { status, headers, body, url }
```
**Tests:** Full response envelope with metadata.

### 3.5 — Fetch: Extract Mode — Markdown
```yaml
- id: fetch_markdown
  fetch:
    url: "https://example.com/article"
    extract: markdown
```
**Tests:** HTML to Markdown extraction via htmd.

### 3.6 — Fetch: Extract Mode — Article (Readability)
```yaml
- id: fetch_article
  fetch:
    url: "https://news.site.com/story"
    extract: article
```
**Tests:** Main article extraction via Readability algorithm.

### 3.7 — Fetch: Extract Mode — JSONPath
```yaml
- id: fetch_jsonpath
  fetch:
    url: "https://api.example.com/data"
    extract: jsonpath
    selector: "$.results[0].name"
```
**Tests:** JSONPath query on JSON response.

### 3.8 — Fetch: Extract Mode — Selector (CSS)
```yaml
- id: fetch_selector
  fetch:
    url: "https://example.com"
    extract: selector
    selector: "div.content p"
```
**Tests:** CSS selector extraction, raw HTML.

### 3.9 — Fetch: Extract Mode — Metadata
```yaml
- id: fetch_metadata
  fetch:
    url: "https://example.com"
    extract: metadata
  # Returns: OG, Twitter Cards, JSON-LD, SEO tags
```
**Tests:** OG/Twitter/JSON-LD metadata extraction.

### 3.10 — Fetch: Response Binary (Media Pipeline)
```yaml
- id: fetch_binary
  fetch:
    url: "https://example.com/image.png"
    response: binary
  # Returns CAS hash for media tools
```
**Tests:** Binary response, CAS storage, media pipeline integration.

### 3.11 — Fetch: Timeout and Redirect Settings
```yaml
- id: fetch_timeout
  fetch:
    url: "https://httpbin.org/delay/10"
    timeout: 5
    follow_redirects: true
```
**Tests:** HTTP timeout, redirect following.

---

## 4. INVOKE VERB (8 tests)

### 4.1 — Invoke: Builtin Tool Short Form
```yaml
- id: invoke_builtin_short
  invoke: "nika:dimensions"
  params:
    image: "path/to/image.png"
```
**Tests:** Short form `invoke: "nika:tool"`, builtin tool access.

### 4.2 — Invoke: Builtin Tool Full Form
```yaml
- id: invoke_builtin_full
  invoke:
    tool: nika:thumbnail
    params:
      input: "source.jpg"
      width: 300
      height: 300
    timeout: 30
```
**Tests:** Full form with timeout, media tool invocation.

### 4.3 — Invoke: MCP Tool with Double Colon
```yaml
- id: invoke_mcp_doublecolon
  invoke:
    tool: "novanet::novanet_search"
    params:
      query: "AI workflow"
      limit: 10
```
**Tests:** MCP server notation with `::`, tool parameters.

### 4.4 — Invoke: MCP Tool with Explicit mcp: Field
```yaml
- id: invoke_mcp_explicit
  invoke:
    tool: search
    mcp: novanet
    params:
      query: "test"
```
**Tests:** Split form: `mcp: server` + `tool: name`.

### 4.5 — Invoke: Resource URI (Alternative to tool:)
```yaml
- id: invoke_resource
  invoke:
    resource: "novanet://entity/123"
    params:
      depth: 2
```
**Tests:** Resource URI syntax as alternative to tool name.

### 4.6 — Invoke: Timeout Setting
```yaml
- id: invoke_timeout
  invoke:
    tool: "novanet::novanet_search"
    params:
      query: "test"
    timeout: 10
```
**Tests:** Per-invoke timeout override.

### 4.7 — Invoke: With Binding from Upstream
```yaml
- id: invoke_binding
  depends_on: [upstream_task]
  with:
    data: $upstream_task
  invoke:
    tool: "nika:thumbnail"
    params:
      input: "{{with.data}}"
```
**Tests:** Template interpolation in invoke params.

### 4.8 — Invoke: Retry at Task Level
```yaml
- id: invoke_retry
  retry:
    max_attempts: 3
    delay_ms: 1000
    backoff: 2.0
  invoke:
    tool: "novanet::novanet_search"
    params:
      query: "test"
```
**Tests:** Task-level retry (applies to all verbs), exponential backoff.

---

## 5. AGENT VERB (10 tests)

### 5.1 — Agent: Basic with Builtin Tools
```yaml
- id: agent_basic
  agent:
    prompt: "Help me. Call nika_complete with result='done'."
    tools: [builtin]
    max_turns: 3
    max_tokens: 200
```
**Tests:** Basic agent, builtin tool access, completion via tool call.

### 5.2 — Agent: Completion Mode — Explicit (Default)
```yaml
- id: agent_explicit
  agent:
    prompt: "Say hello. Then call nika_complete with message='hello'."
    tools: [builtin]
    max_turns: 2
    completion:
      mode: explicit
```
**Tests:** Explicit completion mode (must call nika:complete tool).

### 5.3 — Agent: Completion Mode — Natural
```yaml
- id: agent_natural
  agent:
    prompt: "Answer: Is Rust compiled? Then stop."
    tools: [builtin]
    max_turns: 5
    completion:
      mode: natural
```
**Tests:** Natural completion (stops when no more tool calls).

### 5.4 — Agent: Guardrails — Length
```yaml
- id: agent_guardrail_length
  agent:
    prompt: "Write 5 bullet points about Rust."
    tools: [builtin]
    max_turns: 3
    guardrails:
      - type: length
        min_words: 50
        max_words: 500
        on_failure: retry
```
**Tests:** Length guardrails, retry on failure.

### 5.5 — Agent: Guardrails — Schema
```yaml
- id: agent_guardrail_schema
  agent:
    prompt: "Extract: {name, age, city}. Call nika_complete with JSON."
    tools: [builtin]
    max_turns: 3
    guardrails:
      - type: schema
        json_schema:
          type: object
          properties:
            name: { type: string }
            age: { type: integer }
        required: [name]
        on_failure: escalate
```
**Tests:** Schema validation guardrail, escalate on failure.

### 5.6 — Agent: Guardrails — Regex Pattern
```yaml
- id: agent_guardrail_regex
  agent:
    prompt: "Start with ## Summary"
    tools: [builtin]
    max_turns: 2
    guardrails:
      - type: regex
        pattern: "^## Summary"
        message: "Must start with ## Summary"
        on_failure: retry
```
**Tests:** Regex pattern matching, retry on mismatch.

### 5.7 — Agent: Guardrails — LLM Judge
```yaml
- id: agent_guardrail_llm
  agent:
    prompt: "Say something true about Rust."
    tools: [builtin]
    max_turns: 3
    guardrails:
      - type: llm
        judge_prompt: "Is this factually correct? Reply PASS/FAIL."
        pass_pattern: "^PASS"
        on_failure: retry
```
**Tests:** LLM-based quality judgment guardrail.

### 5.8 — Agent: Cost Limits
```yaml
- id: agent_cost_limit
  agent:
    prompt: "Count to 100."
    tools: [builtin]
    max_turns: 50
    limits:
      max_cost_usd: 1.0
      duration_seconds: 60
```
**Tests:** Cost USD limit, duration limit, graceful stop on limit.

### 5.9 — Agent: Token Budget and Extended Thinking
```yaml
- id: agent_tokens_thinking
  agent:
    prompt: "Solve a hard problem."
    tools: [builtin]
    max_turns: 5
    max_tokens: 2000
    token_budget: 100000
    extended_thinking: true
    thinking_budget: 10000
  provider: anthropic
```
**Tests:** Token budget, extended thinking, multi-turn tokens.

### 5.10 — Agent: With Upstream Binding
```yaml
- id: agent_binding
  depends_on: [data_task]
  with:
    context: $data_task
  agent:
    prompt: "Given: {{with.context}}, analyze it. Call nika_complete."
    tools: [builtin]
    max_turns: 3
```
**Tests:** Agent binding from upstream task, template in prompt.

---

## 6. DATA FLOW (10 tests)

### 6.1 — With Binding: Simple Task Reference
```yaml
- id: source
  exec: "echo DATA"

- id: consumer
  depends_on: [source]
  with:
    data: $source
  exec: "echo {{with.data}}"
```
**Tests:** `$` prefix for task reference, with binding.

### 6.2 — With Binding: Nested Field Access (JSONPath)
```yaml
- id: api_call
  fetch:
    url: "https://api.example.com/user"
    response: full

- id: extract_name
  depends_on: [api_call]
  with:
    name: $api_call.body.user.profile.name
  exec: "echo {{with.name}}"
```
**Tests:** JSONPath field access via dot notation.

### 6.3 — With Binding: Fallback Operator (??)
```yaml
- id: fallback_test
  depends_on: [upstream]
  with:
    safe: $upstream.missing_field ?? "DEFAULT"
  exec: "echo {{with.safe}}"
```
**Tests:** `??` null coalescing operator, safe field access.

### 6.4 — With Binding: Environment Variable
```yaml
- id: env_binding
  with:
    api_key: $env.API_SECRET
    home_dir: $env.HOME
  infer: "API_KEY={{with.api_key}}"
```
**Tests:** `$env.VAR_NAME` binding to environment variables.

### 6.5 — Pipe Transforms: String Operations
```yaml
- id: string_transforms
  depends_on: [source]
  with:
    upper: $source | upper
    lower: $source | lower
    trimmed: $source | trim
    length: $source | length
  exec: "echo U={{with.upper}} L={{with.lower}}"
```
**Tests:** `upper`, `lower`, `trim`, `length` transforms.

### 6.6 — Pipe Transforms: Array Operations
```yaml
- id: array_transforms
  depends_on: [list_source]
  with:
    first: $list_source | first
    last: $list_source | last
    reversed: $list_source | reverse
    unique: $list_source | unique
    count: $list_source | length
  exec: "echo FIRST={{with.first}} COUNT={{with.count}}"
```
**Tests:** `first`, `last`, `reverse`, `unique`, `flatten`, `compact` transforms.

### 6.7 — Pipe Transforms: Type Conversions
```yaml
- id: type_transforms
  with:
    as_string: $number | to_string
    as_number: $string | to_number
    as_bool: $string | to_bool
    as_json: $object | to_json
    type_name: $data | type_of
  exec: "echo TYPE={{with.type_name}}"
```
**Tests:** `to_string`, `to_number`, `to_bool`, `to_json`, `parse_json`, `type_of`.

### 6.8 — Pipe Transforms: Numeric Operations
```yaml
- id: numeric_transforms
  with:
    rounded: $float_val | round(2)
    absolute: $number | abs
    ceiling: $number | ceil
    floor: $number | floor
  exec: "echo ROUNDED={{with.rounded}}"
```
**Tests:** `round`, `abs`, `ceil`, `floor` numeric transforms.

### 6.9 — Pipe Transforms: Chain Multiple Operations
```yaml
- id: chain_transforms
  depends_on: [source]
  with:
    processed: $source | trim | upper | length
    safe_value: $maybe_null | default("fallback") | upper | trim
  exec: "echo {{with.processed}}"
```
**Tests:** Chaining multiple transforms with `|`, null safety with `default()`.

### 6.10 — Context Files: Loading and Binding
```yaml
context:
  files:
    readme: ./README.md
    style: ./docs/style-guide.md

tasks:
  - id: use_context
    infer: |
      Style: {{context.style}}
      Readme: {{context.readme}}
      Write a summary following the style.
```
**Tests:** Context file loading, template binding with `{{context.filename}}`.

---

## 7. CONTROL FLOW (8 tests)

### 7.1 — Depends_on: Ordering Without Data Flow
```yaml
- id: step1
  exec: "echo STEP1"

- id: step2
  exec: "echo STEP2"

- id: step3
  depends_on: [step1, step2]
  exec: "echo STEP3_AFTER_BOTH"
```
**Tests:** `depends_on` for ordering only (no data binding).

### 7.2 — For_Each: Basic Iteration
```yaml
- id: items
  exec: 'echo "[\"a\",\"b\",\"c\"]"'
  output:
    format: json

- id: each_item
  depends_on: [items]
  for_each: $items
  as: item
  exec: "echo ITEM_{{with.item}}"
```
**Tests:** `for_each` over array, `as:` loop variable, `{{with.item}}`.

### 7.3 — For_Each: Concurrency Control
```yaml
- id: concurrent_loop
  depends_on: [items]
  for_each: $items
  as: item
  concurrency: 3
  infer: "Process {{with.item}}"
```
**Tests:** `concurrency:` limit on parallel iterations.

### 7.4 — For_Each: Fail Fast vs Continue
```yaml
- id: fail_fast_true
  depends_on: [items]
  for_each: $items
  as: item
  fail_fast: true
  exec: "cmd {{with.item}}"

- id: fail_fast_false
  depends_on: [items]
  for_each: $items
  as: item
  fail_fast: false
  exec: "cmd {{with.item}}"
```
**Tests:** `fail_fast: true` stops on error, `false` continues all.

### 7.5 — For_Each: Output is Array (Fan-In)
```yaml
- id: loop
  for_each: $items
  as: item
  infer: "Process {{with.item}}"

- id: consume_array
  depends_on: [loop]
  with:
    results: $loop
    count: $loop | length
    first_result: $loop | first
  infer: "Processed {{with.count}} items. First: {{with.first_result}}"
```
**Tests:** `for_each` output is array, requires array access (`[0]`, `first`, `length`).

### 7.6 — Diamond DAG: Merge Multiple Branches
```yaml
- id: source
  exec: "echo DATA"

- id: branch_a
  depends_on: [source]
  with: { d: $source }
  infer: "Process A: {{with.d}}"

- id: branch_b
  depends_on: [source]
  with: { d: $source }
  infer: "Process B: {{with.d}}"

- id: merge
  depends_on: [branch_a, branch_b]
  with: { a: $branch_a, b: $branch_b }
  infer: "Merge: {{with.a}} and {{with.b}}"
```
**Tests:** Diamond dependency pattern, multiple upstream bindings.

### 7.7 — DAG Cycle Detection (Should Fail)
```yaml
- id: task_a
  depends_on: [task_b]
  exec: "echo a"

- id: task_b
  depends_on: [task_a]
  exec: "echo b"
  # Should fail with NIKA-020 (DAG cycle)
```
**Tests:** Cycle detection, DAG validation.

### 7.8 — Dependency Chain Failure (NIKA-026)
```yaml
- id: fails
  exec: "exit 1"

- id: blocked
  depends_on: [fails]
  exec: "echo THIS_BLOCKED"
  # Should fail with NIKA-026 (upstream failed)
```
**Tests:** Upstream task failure blocks dependent tasks.

---

## 8. RESILIENCE & RETRY (4 tests)

### 8.1 — Retry: Max Attempts with Backoff
```yaml
- id: retry_task
  retry:
    max_attempts: 3
    delay_ms: 500
    backoff: 2.0
  fetch:
    url: "https://flaky-api.example.com/data"
```
**Tests:** Task-level retry, exponential backoff (500ms → 1s → 2s).

### 8.2 — Retry: On All Verbs
```yaml
- id: exec_retry
  retry:
    max_attempts: 2
  exec: "flaky_command"

- id: infer_retry
  retry:
    max_attempts: 2
  infer: "Generate data"
```
**Tests:** Retry applies to exec, infer, fetch, invoke, agent.

### 8.3 — Structured Output: Enable Repair
```yaml
- id: repair_enabled
  infer: "Extract JSON with name, age"
  structured:
    schema:
      type: object
      properties:
        name: { type: string }
        age: { type: integer }
    enable_repair: true
    max_retries: 2
    repair_model: claude-haiku-4-5
```
**Tests:** Structured output repair, cheaper repair model.

### 8.4 — Max Turns Graceful Stop
```yaml
- id: agent_max_turns
  agent:
    prompt: "Keep calling nika_iterate."
    tools: [builtin]
    max_turns: 2
    # Stops at turn 2 with partial result, not error
```
**Tests:** Agent gracefully stops at max_turns (not an error).

---

## 9. OUTPUT & ARTIFACTS (6 tests)

### 9.1 — Artifact: Text Format
```yaml
- id: generate
  infer: "Write a summary"

- id: save_text
  depends_on: [generate]
  with:
    content: $generate
  artifact:
    path: "summary.txt"
    format: text
```
**Tests:** Text artifact, file persistence.

### 9.2 — Artifact: JSON Format
```yaml
- id: save_json
  depends_on: [structured_task]
  with:
    data: $structured_task
  artifact:
    path: "data.json"
    format: json
```
**Tests:** JSON artifact, auto-serialization.

### 9.3 — Artifact: YAML Format
```yaml
- id: save_yaml
  artifact:
    path: "config.yaml"
    format: yaml
    source: config_data
```
**Tests:** YAML artifact, source binding (saves upstream data, not task output).

### 9.4 — Artifact: Binary Format (Media)
```yaml
- id: convert_image
  invoke:
    tool: nika:convert
    params: { input: "source.png", format: webp }
  artifact:
    path: "image.webp"
    format: binary
```
**Tests:** Binary artifact, CAS media storage, format: binary.

### 9.5 — Artifact: Mode — Unique vs Overwrite
```yaml
- id: report
  artifact:
    path: "report.md"
    format: markdown
    mode: unique  # report-1.md, report-2.md, ...
```
**Tests:** Mode `unique` creates numbered files, `overwrite` replaces.

### 9.6 — Artifact: Manifest and Workflow-Level Config
```yaml
artifacts:
  dir: ./output
  format: markdown
  mode: overwrite
  manifest: true
  max_size: 104857600

tasks:
  - id: task
    artifact:
      path: "file.md"
```
**Tests:** Workflow-level artifact defaults, manifest.json generation.

---

## 10. SECURITY (5 tests)

### 10.1 — SSRF Protection: Private IP Blocked
```yaml
- id: ssrf_attempt
  fetch:
    url: "http://192.168.1.1:8080/admin"
    # Should fail with NIKA-045 (SSRF blocked)
```
**Tests:** SSRF protection blocks private IP ranges (127.0.0.0/8, 192.168.0.0/16, etc).

### 10.2 — Exec Command Blocklist
```yaml
- id: blocked_rm
  exec: "rm -rf /"
  # Should fail with NIKA-053 (Blocked command)

- id: blocked_sudo
  exec: "sudo shutdown"
  # Should fail with NIKA-053
```
**Tests:** Command blocklist enforcement (rm -rf, sudo, fork bombs).

### 10.3 — Template Injection Prevention
```yaml
- id: no_injection
  with:
    user_input: "{{__proto__}}"
  exec: "echo {{with.user_input}}"
```
**Tests:** Template variables don't allow code execution, safe interpolation.

### 10.4 — API Keys in Environment Only
```yaml
- id: env_secret
  with:
    key: $env.ANTHROPIC_API_KEY
  infer: "Test"
```
**Tests:** API keys must use `$env.VAR` (never hardcoded).

### 10.5 — Directory Traversal Prevention in Artifacts
```yaml
- id: traversal_attempt
  artifact:
    path: "../../etc/passwd"
    format: text
    # Should fail with validation error
```
**Tests:** Path validation blocks `../` traversal attacks in artifact paths.

---

## 11. ADVANCED FEATURES (5 tests)

### 11.1 — Provider Fallback Pattern
```yaml
- id: primary
  infer: "Solve problem"
  provider: openai

- id: fallback
  infer: "Solve problem"
  provider: mock

- id: select
  depends_on: [primary, fallback]
  with:
    result: $primary ?? $fallback
  infer: "Use: {{with.result}}"
```
**Tests:** Fallback operator for multi-provider resilience, ?? operator.

### 11.2 — Skills Injection (Prompt Augmentation)
```yaml
skills:
  writing: ./skills/writing-guide.md

tasks:
  - id: agent_with_skills
    agent:
      prompt: "Write an article about AI"
      tools: [builtin]
      skills: [writing]
      max_turns: 5
```
**Tests:** Skill files injected into system prompt, augment agent context.

### 11.3 — Presets (Agent Reuse from agents: Header)
```yaml
agents:
  researcher:
    system: "You are a research expert"
    tools: [search, read]
    max_turns: 10

tasks:
  - id: reuse_preset
    agent:
      prompt: "Research AI workflow engines"
      from: researcher
```
**Tests:** `agents:` header definition, `from:` preset reference.

### 11.4 — Inputs with Defaults
```yaml
inputs:
  topic: "default topic"
  limit: 10

tasks:
  - id: use_inputs
    infer: "About {{inputs.topic}}, limit {{inputs.limit}}"
```
**Tests:** Workflow inputs with defaults, template binding.

### 11.5 — Log Level Control
```yaml
workflow: debug-workflow
log: debug

tasks:
  - id: verbose
    log: debug
    infer: "Trace details"

  - id: quiet
    log: info
    infer: "Less output"
```
**Tests:** Workflow and task-level log level override.

---

## 12. BOUNDARY CONDITIONS & ERROR CASES (5 tests)

### 12.1 — Empty Output Handling
```yaml
- id: empty
  infer: ""
  output:
    format: json
```
**Tests:** Empty LLM output, NIKA-300 structured validation.

### 12.2 — Null Value Access with Guard
```yaml
- id: guard_null
  depends_on: [source]
  with:
    safe: $source.missing ?? "default"
  exec: "echo {{with.safe}}"
```
**Tests:** Null safety, `default()` guard, NIKA-072 null value at path.

### 12.3 — Unknown Alias Detection
```yaml
- id: bad_binding
  with:
    data: $undefined_task
  exec: "echo {{with.data}}"
  # Should fail with NIKA-071 (Unknown alias)
```
**Tests:** NIKA-071 when referencing undefined task.

### 12.4 — Template Resolution Error (NIKA-041)
```yaml
- id: bad_template
  exec: "echo {{with.nonexistent}}"
  # Should fail with NIKA-041 (Template resolution error)
```
**Tests:** NIKA-041 when template variable not in `with:` block.

### 12.5 — Structured Output Validation (NIKA-300)
```yaml
- id: schema_fail
  infer: "Return wrong data"
  structured:
    schema:
      type: object
      properties:
        name: { type: string }
      required: [name]
    enable_repair: false
    # Should fail with NIKA-300 if LLM returns invalid JSON
```
**Tests:** NIKA-300 structured output validation failure.

---

## SUMMARY TABLE

| Category | # | Details |
|----------|---|---------|
| **Infer Verb** | 9 | Short/full, system, JSON, structured, thinking, vision, guardrails, override, templates |
| **Exec Verb** | 8 | Short/full, shell, env, cwd, timeout, failures, templates, blocklist |
| **Fetch Verb** | 11 | GET/POST, JSON/body, full response, 9 extract modes, binary, timeout |
| **Invoke Verb** | 8 | Builtin, MCP, double colon, resource URI, timeout, binding, retry |
| **Agent Verb** | 10 | Basic, 3 completion modes, 4 guardrail types, cost limits, token budget, binding |
| **Data Flow** | 10 | Task refs, JSONPath, fallback, env vars, 8+ transforms, chaining, context files |
| **Control Flow** | 8 | depends_on, for_each, concurrency, fail_fast, array output, diamond, cycles, chains |
| **Resilience** | 4 | Retry + backoff, all verbs, structured repair, max_turns graceful |
| **Output** | 6 | Text/JSON/YAML/binary artifacts, modes, manifest, workflow defaults |
| **Security** | 5 | SSRF, blocklist, injection prevention, env secrets, traversal |
| **Advanced** | 5 | Fallback pattern, skills, presets, inputs, log levels |
| **Errors** | 5 | Empty output, null guards, unknown alias, template errors, schema validation |
| **TOTAL** | **54** | **Comprehensive feature coverage** |

---

## Test Execution Strategy

### Quick Smoke (5 tests, < 1 min)
1. Infer short form (1.1)
2. Exec basic (2.1)
3. Fetch GET (3.1)
4. Invoke builtin (4.1)
5. Agent basic (5.1)

### Provider-Less (30 tests, ~5 min with mock provider)
Run with `provider: mock` to skip API calls. Tests infer, exec, fetch, invoke, agent, data flow, control flow.

### Full Integration (54 tests, ~30 min)
Requires API keys. Covers all features with real LLM calls, vision, structured repair, etc.

### Failure Cases (15 tests, ~5 min)
Security, error codes, blocklist, SSRF, schema violations, null handling.

---

## Key Error Codes Tested

| Code | Test ID | Description |
|------|---------|-------------|
| NIKA-010 | 10.3, 12.5 | Schema validation error |
| NIKA-020 | 7.7 | DAG cycle detected |
| NIKA-026 | 7.8 | Dependency chain failed |
| NIKA-041 | 12.4 | Template resolution error |
| NIKA-045 | 10.1 | SSRF blocked |
| NIKA-053 | 2.8, 10.2 | Blocked command |
| NIKA-071 | 12.3 | Unknown alias |
| NIKA-072 | 12.2 | Null value at path |
| NIKA-112 | 5.4-5.7 | Agent guardrail violation |
| NIKA-300 | 12.5 | Structured output validation |

---

**Last Updated:** 2026-03-29
**Schema Version:** nika/workflow@0.12
**Status:** Complete, ready for implementation
