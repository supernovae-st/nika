# 04 -- Five Verbs Deep Dive

Every task in a Nika workflow performs exactly one of five verbs. This document details each verb's implementation, parameters, runtime behavior, and edge cases.

---

## 1. infer: -- LLM Text Generation

### Purpose

Send a prompt to a large language model and receive generated text. Supports all 7 cloud providers and native local inference.

### Parameters

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `prompt` | string | Yes* | -- | The prompt text (* optional when `content:` present) |
| `system` | string | No | -- | System prompt override |
| `temperature` | float | No | Provider default | Sampling temperature (0.0 - 2.0) |
| `max_tokens` | integer | No | Provider default | Maximum tokens to generate |
| `extended_thinking` | boolean | No | false | Enable extended thinking (Claude only) |
| `thinking_budget` | integer | No | -- | Token budget for thinking |
| `response_format` | string | No | "text" | Expected format: text, json, markdown |
| `content` | array | No | -- | Multimodal content parts (vision) |
| `guardrails` | array | No | -- | Output validation rules |

### Runtime Flow

1. **Validate params**: Check prompt non-empty, temperature range
2. **Resolve templates**: `{{with.alias}}` substitution in prompt and system
3. **Validate resolved prompt**: Ensure non-empty after template resolution (skip if content present)
4. **Inject JSON schema instruction**: If output policy requires JSON with schema, append schema instruction to prompt
5. **Emit events**: `TemplateResolved`, `ContextAssembled`
6. **Resolve provider**: Task-level override > workflow default > "claude"
7. **Provider dispatch**: Route to appropriate rig-core provider
8. **Structured output**: If output schema configured, apply 5-layer defense system
9. **Return**: Generated text

### Provider Resolution

The `RigProvider` enum wraps all supported providers via rig-core:

```rust
pub enum RigProvider {
    Claude(anthropic::Client),
    OpenAI(openai::Client),
    Mistral(mistral::Client),
    Groq(groq::Client),
    DeepSeek(deepseek::Client),
    Gemini(gemini::Client),
    XAi(xai::Client),
    #[cfg(feature = "native-inference")]
    Native(NativeRuntime),
}
```

Provider names resolve through `core::find_provider()`. Aliases are supported: "claude" resolves to "anthropic", "gpt" to "openai", "grok" to "xai".

### Mock Provider

When `provider: mock` is specified, no API call is made. A deterministic JSON response is generated with common test fields (`title`, `summary`, `items`). For vision content, mock responses include content metadata. This enables testing without API keys.

### Vision (Multimodal Content)

The `content:` field enables vision capabilities:

```yaml
infer:
  content:
    - type: image
      source: "{{with.photo.media[0].hash}}"
      detail: high
    - type: text
      text: "Describe this image in detail"
```

Content part resolution:
- **CAS hash sources**: Automatically resolved to base64 via `CasStore`. The hash is loaded from `.nika/media/store/`, decoded via `detect_image_media_type()` (PNG, JPEG, GIF, WebP magic bytes), and sent as base64 to the LLM API.
- **Image URLs**: Sent directly to the provider.
- **Text parts**: Sent as-is.

Provider support matrix:

| Provider | Vision | Notes |
|----------|--------|-------|
| Claude | Yes | Native multimodal support |
| OpenAI | Yes | GPT-4V and later |
| Mistral | Yes | Pixtral models |
| Groq | Yes | Selected models |
| Gemini | Yes | All Gemini models |
| xAI | Yes | Grok Vision |
| DeepSeek | No | Returns `VisionNotSupported` error |
| Native | Yes* | `NativeModelKind::VisionHf` only (HuggingFace + ISQ) |

*GGUF models are text-only. Native vision requires `VisionModelBuilder` + ISQ from safetensors.*

### Structured Output (5-Layer Defense)

When `output.format: json` with a `schema:` is configured, Nika applies a 5-layer defense system for ~99.99% JSON compliance:

1. **Layer 0: DynamicSubmitTool** -- Injects a `submit_result` tool with the JSON Schema, forcing the LLM to use tool calling for structured output (provider-native)
2. **Layer 1: Raw extraction** -- Attempts to extract JSON from the raw LLM response
3. **Layer 2: Extract + Validate** -- Parses JSON and validates against schema
4. **Layer 3: Retry** -- Re-prompts the LLM with validation errors
5. **Layer 4: LLM Repair** -- Sends the broken JSON to the LLM with the schema for repair

### Extended Thinking

Claude supports extended thinking where the model reasons step-by-step before responding:

```yaml
infer:
  prompt: "Solve this complex math problem..."
  extended_thinking: true
  thinking_budget: 10000
```

The thinking content is captured and emitted as a `ThinkingContent` event but not included in the task output.

### Guardrails

Post-generation validation rules:

```yaml
infer:
  prompt: "Generate a JSON config"
  guardrails:
    - type: regex
      pattern: "^\\{.*\\}$"
      message: "Response must be valid JSON"
    - type: contains
      value: "version"
      message: "Must include version field"
```

If guardrails fail, the task returns `NIKA-112 GuardrailViolation`.

---

## 2. exec: -- Shell Command Execution

### Purpose

Execute shell commands with security validation, output capture, and environment control.

### Parameters

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `command` | string | Yes | -- | Command to execute |
| `shell` | boolean | No | false | Execute via `sh -c` |
| `cwd` | string | No | workflow dir | Working directory |
| `env` | object | No | -- | Environment variables |
| `timeout` | integer | No | 120s | Timeout in seconds |

### Runtime Flow

1. **Resolve templates** in command, cwd, and env values
2. **Validate command** against blocklist
3. **Check policy** (PolicyEnforcer)
4. **Execute**: If `shell: false`, tokenize via `shlex` and spawn directly. If `shell: true`, run `sh -c "<command>"`.
5. **Capture output**: stdout as task result. If exit code != 0, return `NikaError::Execution`.
6. **Timeout enforcement**: Kill process after deadline.

### Security Model

The exec verb has the most aggressive security validation in Nika.

#### Command Blocklist (Always Active)

```
rm -rf /          rm -rf /*         rm -rf ~
| bash            |bash             | sh              |sh
eval              mkfifo            nc -e             nc -c
; rm              && rm             | rm
:(){ :|:& };:    python -c "import socket
sudo              doas              pkexec
chmod 777         dd if=            perl -e
ruby -e           node -e           env
su
```

#### Shell-Mode Blocklist (Only When `shell: true`)

```
$(                `
```

#### Unicode Confusable Protection

All commands are normalized via NFKC (Compatibility Decomposition + Canonical Composition) before blocklist checking. This prevents bypass via:
- Fullwidth characters: `ｒｍ` (U+FF52, U+FF4D)
- Math bold/italic: `𝘀𝘂𝗱𝗼` (U+1D600 range)
- Combining characters with zero-width joiners

#### Control Character Detection

Commands are scanned for control characters (null bytes, escape sequences) which are blocked unconditionally.

### Environment Variables

```yaml
exec:
  command: python3 script.py
  env:
    API_KEY: "{{with.key}}"
    DATABASE_URL: "postgres://localhost/db"
```

Environment variables are merged with the process environment. Template resolution happens before execution.

---

## 3. fetch: -- HTTP Requests

### Purpose

Make HTTP requests with optional response extraction. Supports 9 extraction modes for HTML processing, JSON querying, and feed parsing.

### Parameters

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `url` | string | Yes | -- | Request URL |
| `method` | string | No | "GET" | HTTP method |
| `headers` | object | No | -- | HTTP headers |
| `body` | string | No | -- | Request body |
| `json` | object | No | -- | JSON request body |
| `timeout` | integer | No | 60s | Timeout in seconds |
| `follow_redirects` | boolean | No | true | Follow redirects |
| `response` | string | No | -- | Response mode: full, binary |
| `extract` | string | No | -- | Extraction mode |
| `selector` | string | No | -- | CSS selector or JSONPath |

### Runtime Flow

1. **Resolve templates** in URL, headers, body
2. **Check SSRF blocklist**: Cloud metadata endpoints (169.254.169.254, metadata.google.internal, 100.100.100.200) and loopback addresses are always blocked
3. **Build request**: Method, headers, body, timeout, redirect policy
4. **Execute request**: Via shared `reqwest::Client` with connection pooling
5. **Process response**: Based on `response:` and `extract:` modes
6. **Return**: Extracted content or raw body

### SSRF Protection

The following hosts are unconditionally blocked, regardless of user configuration:

- `169.254.169.254` (AWS metadata)
- `metadata.google.internal` (GCP metadata)
- `100.100.100.200` (Alibaba Cloud metadata)
- `localhost`, `127.0.0.1`, `::1`, `0.0.0.0`

### Extraction Modes (9 Total)

#### `extract: markdown`
Converts HTML to clean Markdown via the `htmd` crate. Removes scripts, styles, and navigation. Feature: `fetch-markdown`.

#### `extract: article`
Extracts the main article content using Readability algorithm via `dom_smoothie`. Returns clean text content, stripping boilerplate, sidebars, and navigation. Feature: `fetch-article`.

#### `extract: text`
Extracts visible text from HTML. When `selector:` is provided, filters to matching elements first. Feature: `fetch-html`.

#### `extract: selector`
Returns raw HTML of elements matching the CSS selector. Requires `selector:` field. Feature: `fetch-html`.

```yaml
fetch:
  url: "https://example.com"
  extract: selector
  selector: "article.main h2"
```

#### `extract: metadata`
Extracts structured metadata as JSON: Open Graph tags, Twitter Cards, JSON-LD, SEO meta tags, canonical URLs. Feature: `fetch-html`.

#### `extract: links`
Classifies and extracts all links with rich metadata: internal/external, navigation/content/footer, anchor text, rel attributes. Feature: `fetch-html`.

#### `extract: jsonpath`
Applies a JSONPath expression (RFC 9535) to JSON API responses. The `selector:` field specifies the JSONPath. Zero additional dependencies (uses `serde_json_path`).

```yaml
fetch:
  url: "https://api.github.com/repos/owner/repo"
  extract: jsonpath
  selector: "$.stargazers_count"
```

#### `extract: feed`
Parses RSS, Atom, and JSON Feed formats via `feed-rs`. Returns structured feed data with entries, titles, dates, and content. Feature: `fetch-feed`.

#### `extract: llm_txt`
AI-era content discovery. Checks `/.well-known/llm.txt` and `/llms.txt` for machine-readable content descriptions.

### Response Modes

#### Default (no `response:` field)
Returns raw body text.

#### `response: full`
Returns JSON with complete response metadata:
```json
{
  "status": 200,
  "headers": { "content-type": "text/html" },
  "body": "...",
  "url": "https://final-url-after-redirects.com"
}
```

#### `response: binary`
Stores response body in CAS (Content-Addressable Storage). Returns the blake3 hash for use in the media pipeline. Used for downloading images, PDFs, etc.

---

## 4. invoke: -- MCP Tool Calls

### Purpose

Call tools on MCP servers or use Nika's 24 builtin tools. This is Nika's primary extensibility mechanism.

### Parameters

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `tool` | string | Yes* | -- | Tool name (* or `resource:`) |
| `resource` | string | No | -- | MCP resource URI (alternative to tool) |
| `params` | object | No | `{}` | Tool parameters |
| `mcp` | string | No | auto | MCP server name |
| `timeout` | integer | No | 60s | Timeout in seconds |

### Tool Name Resolution

1. **`nika:*` prefix**: Routes to `BuiltinToolRouter`. No MCP server needed.
2. **`server::tool_name`**: Explicitly specifies the MCP server.
3. **`tool_name`** (bare): Auto-resolved to first connected MCP server that advertises this tool.

### Runtime Flow

1. **Resolve templates** in params
2. **Classify tool**: Builtin (`nika:*`) vs MCP
3. **For builtin tools**: Route through `BuiltinToolRouter`
4. **For MCP tools**: Get or create client from `McpClientPool`, send JSON-RPC call
5. **Process response**: Extract text content, handle binary content blocks
6. **Timeout**: Race against cancellation token + `INVOKE_TASK_DEADLINE`
7. **Return**: Tool output as string

### Builtin Core Tools (7)

| Tool | Description | Parameters |
|------|-------------|-----------|
| `nika:sleep` | Pause execution | `{ duration_ms: 1000 }` |
| `nika:log` | Emit log event | `{ level: "info", message: "..." }` |
| `nika:emit` | Emit custom event | `{ event: "name", payload: {...} }` |
| `nika:assert` | Validate condition | `{ condition: true, message: "..." }` |
| `nika:prompt` | HITL user input | `{ question: "...", options: [...] }` |
| `nika:run` | Execute sub-workflow | `{ workflow: "path.nika.yaml" }` |
| `nika:complete` | Signal agent completion | `{ result: "...", confidence: 0.9 }` |

### Builtin File Tools (5)

| Tool | Description | Parameters |
|------|-------------|-----------|
| `nika:read` | Read file with line numbers | `{ file_path: "/abs/path", offset: 0, limit: 100 }` |
| `nika:write` | Create new file (fails if exists) | `{ file_path: "/abs/path", content: "..." }` |
| `nika:edit` | Edit existing file | `{ file_path: "/abs/path", old_string: "...", new_string: "..." }` |
| `nika:glob` | Find files by pattern | `{ pattern: "**/*.rs", path: "/dir" }` |
| `nika:grep` | Search with regex | `{ pattern: "TODO", path: "/dir", type: "rs" }` |

File tools enforce security boundaries: absolute paths only, within working directory. Permission modes: Deny, Plan, AcceptEdits, YoloMode.

### Builtin Media Tools (24)

See [08-media-pipeline.md](./08-media-pipeline.md) for the complete media tool reference.

### MCP Client Pool

The `McpClientPool` manages MCP server connections:

- Lazy initialization (connect on first use)
- Per-server deduplication via `DashMap + OnceCell`
- Event logging for all connections and calls
- Graceful shutdown with process termination
- Timeouts: connect 20s, call 60s, reconnect 30s

---

## 5. agent: -- Multi-Turn Autonomous Loops

### Purpose

Give an LLM agent a prompt and tools, then let it work autonomously over multiple turns until it signals completion or reaches a limit.

### Parameters

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `prompt` | string | Yes | -- | The agent's objective |
| `system` | string | No | -- | System prompt (persona) |
| `from` | string | No | -- | Reference to `agents:` definition |
| `tools` | array | No | all core | Available tools |
| `skills` | array | No | -- | Skills to inject |
| `max_turns` | integer | No | 10 | Max turns (1-100) |
| `max_tokens` | integer | No | provider default | Tokens per response |
| `temperature` | float | No | provider default | Sampling temperature |
| `token_budget` | integer | No | -- | Total token budget |
| `provider` | string | No | workflow default | Provider override |
| `model` | string | No | workflow default | Model override |
| `mcp` | array | No | -- | MCP servers for tools |
| `extended_thinking` | boolean | No | false | Extended thinking (Claude) |
| `thinking_budget` | integer | No | -- | Thinking budget tokens |
| `depth_limit` | integer | No | 3 | Max spawn recursion depth |
| `tool_choice` | string | No | "auto" | auto, required, none |
| `stop_sequences` | array | No | -- | Stop generation sequences |
| `scope` | string | No | "full" | Preset: full, minimal, debug |
| `guardrails` | array | No | -- | Output validation |
| `completion` | object | No | -- | Completion behavior config |
| `limits` | object | No | -- | Execution cost limits |

### Architecture

```
RigAgentLoop
  +-- Creates rig::Agent via AgentBuilder
  +-- Converts MCP tools to NikaMcpTool (ToolDyn)
  +-- Adds builtin nika:* tools
  +-- Runs agent.chat() for multi-turn execution
  +-- Emits events to EventLog for observability
  +-- Tracks limits (cost, tokens, duration)
```

### Runtime Flow

1. **Validate params**: Non-empty prompt, valid max_turns (1-100)
2. **Build tools from MCP clients**: Each MCP tool becomes a `NikaMcpTool` implementing rig's `ToolDyn`
3. **Add spawn_agent tool**: If `depth_limit` allows (current_depth < max_depth)
4. **Add builtin tools**: Core + file tools based on `tools:` filter
5. **Inject skills**: Load skill files, prepend to system prompt
6. **Build system prompt**: Combine persona, tool routing guide, completion instructions
7. **Create rig Agent**: Via `AgentBuilder` with preamble, model, tools
8. **Execute multi-turn loop**: `agent.chat()` until completion signal or max_turns
9. **Track limits**: Cost, tokens, duration per turn
10. **Collect media**: Drain binary content blocks from staging
11. **Return**: Final agent output

### Tool Selection

The `tools:` field controls which tools are available:

```yaml
# All core builtin tools (default when tools: omitted)
agent:
  prompt: "..."

# Only specific tools
agent:
  prompt: "..."
  tools: ["nika:read", "nika:write", "nika:grep"]

# All builtin tools (core + file)
agent:
  prompt: "..."
  tools: ["builtin"]

# MCP tools + specific builtins
agent:
  prompt: "..."
  mcp: [novanet]
  tools: ["nika:complete", "nika:log"]
```

### Completion Detection

Agents signal completion through three mechanisms:

1. **Explicit** (default): Agent calls `nika:complete` tool with result and confidence
2. **Pattern**: Agent's response matches a configurable regex pattern
3. **Natural**: Agent reaches max_turns and final response is used as output

The `completion:` config controls this behavior:

```yaml
agent:
  prompt: "..."
  completion:
    mode: explicit
    confidence:
      threshold: 0.8
      low_action: retry
```

### Nested Agent Spawning

Agents can spawn child agents via the `spawn_agent` tool:

```yaml
agent:
  prompt: "Coordinate a team of specialists"
  depth_limit: 3   # Allow 2 levels of child agents
```

Depth tracking prevents infinite recursion. The root agent runs at depth 1; child agents at depth 2, etc.

### Execution Limits

Cost control for agent loops:

```yaml
agent:
  prompt: "..."
  limits:
    max_cost: 1.00        # Maximum dollar cost
    max_tokens: 50000      # Maximum total tokens
    max_duration: 300       # Maximum seconds
    on_limit_reached:
      action: stop          # stop or warn
```

The `LimitTracker` checks limits after each turn and can terminate the agent loop gracefully.

### Stop Sequences

Provider-specific stop sequences injected via `additional_params`:

```yaml
agent:
  prompt: "..."
  stop_sequences: ["DONE", "END"]
```

Provider key mapping:
- Anthropic/Claude: `stop_sequences`
- Gemini: `stopSequences`
- OpenAI, Mistral, Groq, DeepSeek, xAI: `stop`

### Media Staging

Binary content blocks from MCP tool responses are collected in a shared `AgentMediaStaging` (`Arc<DashMap>`). After the agent loop completes, `drain_media()` collects all staged content blocks. This handles the limitation that rig's `ToolDyn::call()` returns `String` only.

### Skill Injection

Skills defined in `skills:` are loaded from markdown files and prepended to the system prompt:

```yaml
skills:
  writing: ./skills/writing-guide.md

tasks:
  - id: write
    agent:
      prompt: "Write a blog post"
      skills: [writing]
```

The `SkillInjector` uses a `DashMap` cache to avoid re-reading skill files across multiple agent tasks.

### Structured Output for Agents

Unlike `infer:` (which forces `tool_choice: Required`), the agent's `DynamicSubmitTool` is available but not forced. The agent calls `submit_result` when ready:

```yaml
agent:
  prompt: "Analyze this data and produce a structured report"
  output:
    format: json
    schema:
      type: object
      properties:
        title: { type: string }
        findings: { type: array }
      required: [title, findings]
```

---

## Cross-Cutting Concerns

### Template Resolution

All five verbs resolve `{{with.alias}}` templates before execution. The resolution process:

1. Look up alias in `ResolvedBindings`
2. If lazy binding, resolve from `RunContext` datastore
3. Apply pipe transforms (if any)
4. Replace template placeholder with resolved value

### Output Policy

All verbs support `output:` configuration for format enforcement:

```yaml
output:
  format: json
  schema: { ... }
  max_retries: 3
```

For `infer:` and `agent:`, the schema instruction is injected into the prompt. For `exec:` and `fetch:`, the output is validated post-execution.

### Event Emission

Every verb emits events to the `EventLog`:
- `TaskStarted` (before execution)
- `TemplateResolved` (after template substitution)
- `ContextAssembled` (binding sources)
- Verb-specific events (MCP call, agent turn, etc.)
- `TaskCompleted` or `TaskFailed` (after execution)

### Error Handling

All verb errors use `NikaError` with NIKA-XXX codes. Each verb has its own error range:
- infer: NIKA-030-039 (provider), NIKA-060-069 (output)
- exec: NIKA-050-059 (path/task/security)
- fetch: NIKA-090-099 (I/O)
- invoke: NIKA-100-109 (MCP), NIKA-200-219 (builtin/media tools)
- agent: NIKA-110-119 (agent)

See [07-error-codes-reference.md](./07-error-codes-reference.md) for the complete error taxonomy.
