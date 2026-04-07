## Common Mistakes

| Wrong | Right |
|-------|-------|
| `timeout: 30000` (ms) | `timeout: 30` (always seconds) |
| `use: { data: step1 }` | `with: { data: $step1 }` ($ prefix required) |
| `{{data}}` | `{{with.data}}` (always `with.` prefix) |
| `{{item}}` in for_each | `{{with.item}}` (loop var uses `with.` prefix) |
| `retry: 3` | `retry: { max_attempts: 3, delay_ms: 2000 }` |
| `.yaml` extension | `.nika.yaml` extension |
| `shell: bash` | `shell: true` (boolean, not shell name) |
| Missing `schema:` line | Always start with `schema: "nika/workflow@0.12"` |
| `depends_on: task_id` | `depends_on: [task_id]` (always array) |
| `tool: "server/tool"` | `tool: "server::tool"` (double colon `::`) |
| `output: { format: json }` | `structured: { schema: ... }` for validated JSON |
| `{{with.results.field}}` after for_each | `{{with.results[0].field}}` (for_each = array) |
| `retry:` inside `invoke:` | `retry:` is task-level — place alongside `invoke:` |
| `body: {...}` for JSON | Use `json: {...}` (auto-serializes; `body:` is strings only) |
| `invoke: { tool: "...", input: {...} }` | `invoke: { tool: "...", params: {...} }` |
| `model: haiku` inside `infer:` | `model: claude-haiku-4-5` at task level |
| `echo {{with.val}}` with `shell: true` | `echo {{with.val \| shell}}` (NIKA-053) |
| `provider: native` for vision | GGUF is text-only — use cloud provider |
| `provider: deepseek` for vision | DeepSeek doesn't support vision |
| `retry: { max_retries: N }` | `retry: { max_attempts: N }` — `max_retries` is for `structured:` only |

## Key Error Codes

| Code | Meaning |
|------|---------|
| NIKA-010 | Schema validation error |
| NIKA-020 | DAG cycle detected |
| NIKA-026 | Dependency chain failed (upstream task failed) |
| NIKA-041 | Template resolution error |
| NIKA-045 | Fetch error (SSRF blocked, timeout) |
| NIKA-053 | Blocked command (security) |
| NIKA-071 | Unknown alias in `{{with.alias}}` |
| NIKA-072 | Null value at path — guard with `default()` |
| NIKA-100 | MCP connection error |
| NIKA-112 | Agent guardrail violation |
| NIKA-270 | Skill file not found |
| NIKA-300 | Structured output validation failed |
