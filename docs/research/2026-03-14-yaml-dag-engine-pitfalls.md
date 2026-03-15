# Research Report: Common Pitfalls in YAML-Based Workflow/DAG Engines

**Date**: 2026-03-14
**Purpose**: Identify common bug patterns in YAML workflow engines, DAG execution,
Rust async runtimes, MCP clients, and LLM frameworks -- then map them to Nika's codebase
for a proactive audit.

---

## Summary

This report catalogs **42 known pitfall patterns** across 5 domains, each evaluated
against Nika v0.27.0. Findings are prioritized as:
- **HIGH**: Likely present or partially mitigated, needs audit
- **MEDIUM**: Possible under edge conditions
- **LOW**: Unlikely given current architecture, but worth awareness

---

## 1. YAML Workflow Engine Common Bugs

### 1.1 The Norway Problem (Boolean Coercion)

**Problem**: YAML 1.1 interprets bare words like `yes`, `no`, `on`, `off`, `NO` as booleans.
Country codes like `NO` (Norway), `FR` (not a boolean, but could be), and field names
become booleans silently. This is YAML's most infamous bug, causing data corruption
in bioinformatics, configuration management, and workflow definitions.

**How it manifests in Nika**: A workflow like:
```yaml
inputs:
  country: NO          # Parsed as boolean false, not string "NO"
  enabled: yes         # Parsed as boolean true, not string "yes"
  shell: on            # Already handled -- see parser.rs get_bool_field()
```

**Nika status**: **MEDIUM risk**. The `node_to_json()` function in `parser.rs` (line 328)
only checks for exact `"true"` and `"false"` strings when converting to JSON booleans.
It does NOT coerce `yes/no/on/off/YES/NO` as booleans in the JSON conversion path.
However, `get_bool_field()` (line 229) explicitly handles `"yes"/"no"/"on"/"off"/"1"/"0"`
for fields that are EXPECTED to be booleans. This split behavior is actually correct
but could surprise users: `shell: yes` works, but `inputs: { enabled: yes }` would
become string `"yes"` in inputs (since inputs go through `node_to_json`).

**Detection**: Add test cases for `node_to_json` with YAML 1.1 boolean-like values.
Verify `marked_yaml` behavior (it may handle this at parse level).

### 1.2 Null Value Handling (Empty String vs Null)

**Problem**: In YAML, `~`, `null`, `Null`, `NULL`, and empty values can all represent null.
An empty mapping value `key:` (with nothing after colon) is null in YAML 1.1.
This creates ambiguity between "field is absent", "field is null", and "field is empty string".

**How it manifests in Nika**: Consider:
```yaml
tasks:
  - id: step1
    description:         # null or empty string?
    infer: ""            # empty prompt -- should this be valid?
```

**Nika status**: **MEDIUM risk**. `node_to_json()` handles `"null"` and `"~"` as
`Value::Null` (line 343). But `extract_string()` (line 129) extracts any scalar as
a string, meaning YAML nulls (`~`, `null`) would become the literal strings `"~"` or
`"null"` when passed through `extract_string`. This is a known divergence point.
The `marked_yaml` library may represent a bare `key:` as a null node rather than
a scalar, which would then hit the non-scalar branch and return an InvalidType error.

**Detection**: Test `parse()` with `description: ~`, `description: null`, `description:`,
and `description: ""` to verify each produces the expected result.

### 1.3 Number vs String Ambiguity

**Problem**: YAML auto-parses numeric-looking strings as numbers. Version strings
like `"3.10"` become float `3.1` (trailing zero dropped). Task IDs that look like
numbers (`id: 123`) become integers instead of strings.

**How it manifests in Nika**: Consider:
```yaml
tasks:
  - id: 123                # Integer or string "123"?
    model: 3.10            # Float 3.1 or string "3.10"?
```

**Nika status**: **MEDIUM risk**. Task IDs go through `extract_string()` which
calls `.to_string()` on the scalar node. If `marked_yaml` preserves the original
string representation (which it does for scalars), then `"123"` stays as string `"123"`.
But `node_to_json()` does try `parse::<i64>()` first (line 333), so JSON params
with version-like strings could lose precision.

**Detection**: Add test: `id: 123` should parse as string `"123"`. Add test:
`params: { version: 3.10 }` should warn or preserve as string.

### 1.4 Multi-Document YAML Files

**Problem**: YAML supports multiple documents separated by `---`. If a workflow file
accidentally contains multiple documents, only the first is parsed and the rest is
silently ignored.

**Nika status**: **LOW risk**. `marked_yaml::parse_yaml` returns a single `Node`.
If the file has `---` separators, behavior depends on the library. Worth a defensive test.

### 1.5 Anchor/Alias Reference Cycles

**Problem**: YAML anchors (`&anchor`) and aliases (`*alias`) can create reference
cycles that cause infinite loops or stack overflows in parsers.

**Nika status**: **LOW risk**. `marked_yaml` handles anchors safely. The
`LoadError::UnexpectedAnchor` variant in `extract_span_from_load_error` (line 96)
suggests anchors may not even be supported, preventing this class of bugs entirely.

### 1.6 Large YAML Document Denial of Service

**Problem**: Deeply nested YAML or YAML with millions of nodes can exhaust memory
or stack space. Known as "billion laughs" attack with anchors.

**Nika status**: **LOW risk** for anchors (see above), but no explicit depth or
size limits on the YAML document itself. A workflow with 10,000 tasks would be
accepted and could cause memory issues during DAG construction.

**Detection**: Test with large workflow (1000+ tasks) to verify graceful behavior.

### 1.7 Template Injection via User-Controlled Values

**Problem**: If task outputs contain template syntax (`{{use.secret}}`), and those
outputs are used in subsequent prompts, the template engine might resolve them,
leaking data from unintended sources.

**Nika status**: **MITIGATED**. The template module documentation (line 40-48)
explicitly addresses this: "template markers in VALUES are NOT re-evaluated".
The 3-pass architecture prevents cross-source injection. The module header claims
injection tests exist. This is well-handled.

### 1.8 Duplicate Task IDs

**Problem**: Two tasks with the same ID silently overwrite each other, causing
one to be lost.

**Nika status**: **MITIGATED**. The `TaskTable::try_insert()` method (line 138,
ids.rs) returns `None` for duplicates, and the analyzer's `build_task_table`
function detects duplicates with proper error reporting (NIKA-141 DuplicateTask).

### 1.9 Implicit Flow Dependencies vs Explicit

**Problem**: When tasks reference each other via `with:` bindings, implicit
dependency edges should be created. If only explicit `depends_on:` is considered,
tasks may execute before their data dependencies are ready.

**Nika status**: **MITIGATED**. The analyzer comment (line 4 of analyze.rs)
explicitly mentions "Extracts implicit dependencies from WithEntry task references".
The analyzer converts `with:` references into dependency edges.

---

## 2. DAG Execution Race Conditions (Rust/Tokio)

### 2.1 Task Completion Notification Race

**Problem**: When using `JoinSet` or channels to collect task results, there's a
window between a task completing and the orchestrator processing the completion.
If a dependent task is scheduled during this window, it may try to read data
that hasn't been stored yet.

**How it manifests in Nika**: With `for_each` and `concurrency > 1`, multiple
tasks complete near-simultaneously. If the runner processes completions one at a
time but downstream tasks check `RunContext` before all predecessors are recorded:

```
Task A completes → result stored → Task C (depends on A+B) triggered
Task B completes → result being stored
Task C starts → reads B's result → NOT YET AVAILABLE
```

**Nika status**: **HIGH risk** (needs audit). The `for_each` implementation uses
`tokio::spawn` with `JoinSet`. The ordering of result collection and storage into
`RunContext` relative to downstream task scheduling is critical. If the runner uses
a single loop to process completions and schedule new tasks, this is safe. If
completions trigger scheduling asynchronously, there could be a race.

**Detection**: Stress test with `concurrency: 50` on tasks with tight data
dependencies. Check if `RunContext` uses `DashMap` or `RwLock` for thread safety.

### 2.2 Semaphore Fairness and Starvation

**Problem**: When using `tokio::sync::Semaphore` for concurrency control, long-running
tasks hold permits while short tasks queue up. If `fail_fast` cancellation races
with permit acquisition, tasks may be cancelled while they're already executing.

**Nika status**: **MITIGATED** (partially). The v0.24.0 changelog mentions
"Use `tokio::select!` to race semaphore acquisition against cancellation", which
is the correct pattern. However, `select!` has known subtleties:
- The `select!` macro evaluates branches in random order by default
- If cancellation and semaphore both resolve simultaneously, either could win
- A task may start executing between semaphore acquisition and cancellation check

**Detection**: Test `fail_fast: true` with a slow task (sleep 5s) and a fast
failing task. Verify the slow task is actually cancelled, not just abandoned.

### 2.3 Deadlock in DAG with Shared Resources

**Problem**: If tasks compete for shared resources (e.g., MCP connections, semaphore
permits) while having mutual data dependencies, deadlock is possible. Classic case:
Task A holds resource R1, needs R2. Task B holds R2, needs R1.

**Nika status**: **LOW risk**. MCP clients are shared via `Arc` and are inherently
thread-safe (the `McpClient` uses `DashMap` for caching and `AtomicBool` for state).
Semaphore is per-for_each, not global. However, if multiple `for_each` blocks share
the same MCP connection, and the MCP server has limited capacity, backpressure
could cause effective deadlock.

**Detection**: Run two concurrent `for_each` blocks that both invoke the same MCP server
with limited connection capacity.

### 2.4 JoinSet Task Panic Propagation

**Problem**: When a tokio task panics (as opposed to returning `Err`), the panic
is captured by `JoinHandle`. If the orchestrator doesn't check for `JoinError::Panic`,
the panic is silently swallowed, and the task appears to have just disappeared.

**Nika status**: **MEDIUM risk**. The `for_each` implementation spawns tasks via
`tokio::spawn`. If a task panics (e.g., due to a bug in template resolution or
JSON parsing), the `JoinSet` returns a `JoinError`. The question is whether
the runner distinguishes `JoinError::Cancelled` from `JoinError::Panic`.

**Detection**: Create a workflow that triggers a panic in a spawned task (e.g.,
stack overflow in deep recursion) and verify the error is reported clearly.

### 2.5 Async Drop and Resource Leaks

**Problem**: Rust's `Drop` trait is synchronous, but resources like MCP connections
need async cleanup (sending shutdown messages, waiting for process termination).
Dropping an `McpClient` in a sync context skips async cleanup.

**Nika status**: **MEDIUM risk**. The `McpClient` struct (line 1386) has a comment
"Drop is handled by RmcpClientAdapter which cleans up the child process". The
`RmcpClientAdapter` likely uses a sync `Drop` to kill the child process, which
works but may not send a graceful MCP shutdown message. If the MCP server writes
persistent state, this could cause corruption.

**Detection**: Check `RmcpClientAdapter`'s Drop implementation. Verify that MCP
servers receive proper shutdown signals.

### 2.6 Tokio Runtime Blocking

**Problem**: Calling blocking operations inside async tasks (file I/O, synchronous
HTTP, CPU-intensive work) blocks the tokio worker thread, reducing throughput.
Common culprits: `std::fs::read`, synchronous keychain access, template regex
compilation.

**Nika status**: **LOW risk** for regex (uses `LazyLock` in template.rs).
**MEDIUM risk** for file operations in context loading and artifact writing.
The template module uses pre-compiled regex via `LazyLock` (correct pattern).
But `context_loader.rs` may do synchronous file reads inside async context.

**Detection**: Grep for `std::fs::` calls inside async functions. Verify they use
`tokio::fs` or `spawn_blocking`.

### 2.7 Channel Backpressure and Memory Growth

**Problem**: Unbounded channels for event emission can grow without limit if the
consumer (TUI) falls behind the producer (runtime). This causes memory growth
proportional to execution time.

**Nika status**: **MEDIUM risk**. The `EventLog` likely uses a channel or `Vec`
internally. With streaming LLM responses generating many events per second,
and the TUI rendering at 60fps, the event buffer could grow unboundedly during
long agent runs.

**Detection**: Monitor memory during a long-running agent task. Check if `EventLog`
uses bounded or unbounded channels.

---

## 3. YAML Parsing Edge Cases (serde/Rust)

### 3.1 Marked YAML vs Serde YAML Behavior Differences

**Problem**: `marked_yaml` and `serde_yaml` have different parsing behaviors for
edge cases. Nika uses `marked_yaml` for span tracking, but may use `serde_yaml`
elsewhere (e.g., for test fixtures or config files). Inconsistent parsing between
the two can cause "works in tests but fails in production" bugs.

**Nika status**: **MEDIUM risk**. The parser exclusively uses `marked_yaml` for
workflow parsing, which is good. But if any other part of the codebase uses
`serde_yaml` for YAML operations, behavior differences could emerge.

**Detection**: Grep for `serde_yaml` usage. Ensure all YAML parsing goes through
the same path.

### 3.2 Unicode in Task IDs and Field Names

**Problem**: YAML allows Unicode in keys. A task ID containing Unicode characters
(emoji, accented characters) may pass parsing but cause issues in file paths
(artifact names), regex matching, or JSON serialization.

**Nika status**: **LOW risk**. Task IDs are validated as strings but there's no
explicit character restriction. The template regex `USE_RE` (template.rs line 88)
uses `\w+` which matches `[a-zA-Z0-9_]` in Rust's regex, excluding Unicode
word characters by default. So `{{use.etape_1}}` works but `{{use.etape}}` would
work too. Task IDs with hyphens (`step-1`) may fail template matching since `\w`
doesn't include `-`.

**Detection**: Test task ID `step-1` with template `{{with.data}}` where data
references that task. Verify hyphens work or are rejected with a clear error.

### 3.3 Excessively Long Strings

**Problem**: YAML has no built-in string length limit. A prompt field containing
megabytes of text would be stored in memory, potentially causing OOM during
template resolution (which may create copies).

**Nika status**: **LOW risk**. Template resolution uses `Cow<str>` for zero-copy
when no substitution is needed (template.rs line 64). But when substitution occurs,
the full string is cloned and modified.

### 3.4 JSON Value Conversion Precision Loss

**Problem**: The `node_to_json()` function (parser.rs line 328) converts YAML
scalars to JSON values with type inference. This can cause precision loss:
- `3.10` becomes `3.1` (trailing zero dropped in f64)
- `00123` becomes `123` (leading zeros dropped in i64)
- `1e100` becomes a very large number (potential overflow)

**Nika status**: **HIGH risk** for `params:` blocks. When users write MCP tool
parameters like `params: { version: 3.10 }`, the version becomes float `3.1`
instead of string `"3.10"`. This is a real-world bug pattern.

**Detection**: Test `node_to_json` with `"3.10"`, `"00123"`, `"1e999"`,
`"9999999999999999"` (exceeds i64 precision). Add explicit quoting guidance
in documentation.

### 3.5 Empty Mapping and Sequence Handling

**Problem**: Empty mappings `{}` and sequences `[]` are valid YAML but can cause
issues in code that assumes non-empty collections.

**Nika status**: **LOW risk**. The parser handles `tasks: []` by returning an
empty task list (parser.rs line 1179). Most collection operations use iterators
which handle empty gracefully.

### 3.6 YAML Tag Handling

**Problem**: YAML tags like `!!str`, `!!int`, `!!binary` explicitly specify types.
If the parser doesn't handle them, tagged values may be silently misinterpreted.

**Nika status**: **LOW risk**. `marked_yaml`'s `LoadError::UnexpectedTag` variant
(parser.rs line 97) suggests tags are explicitly rejected. This is the safest
approach for a workflow engine.

---

## 4. MCP Protocol Client Implementation Pitfalls

### 4.1 Stdio Transport Deadlock

**Problem**: MCP uses JSON-RPC over stdio (stdin/stdout of a child process).
If the client writes a large request to the server's stdin while the server's
stdout buffer is full (because the client isn't reading), both processes block:
classic pipe deadlock.

**How it manifests in Nika**: When calling an MCP tool with large parameters
(e.g., a long context string), if the response is also large, the OS pipe
buffers (typically 64KB on Linux, 16KB on macOS) can fill up in both directions.

**Nika status**: **MEDIUM risk**. The `RmcpClientAdapter` handles the actual
stdio communication. If rmcp uses async I/O for both reading and writing
(which it should, being a Rust async library), this is mitigated. But if any
synchronous I/O is involved, deadlock is possible.

**Detection**: Test with a large MCP tool call (>64KB parameters AND >64KB response).

### 4.2 Server Process Zombie Accumulation

**Problem**: If MCP server processes aren't properly reaped after disconnection,
zombie processes accumulate. This is especially problematic when the workflow
engine crashes or is killed with SIGKILL.

**Nika status**: **MEDIUM risk**. The `Drop` implementation on `RmcpClientAdapter`
should kill the child process, but SIGKILL of the parent means Drop never runs.
Over time, zombie MCP servers accumulate.

**Detection**: Run Nika, force-kill it (SIGKILL), check for orphaned MCP processes.

### 4.3 Server Startup Timeout

**Problem**: MCP servers started via stdio may take variable time to initialize
(downloading npm packages, compiling, connecting to databases). If the client
has a fixed connect timeout that's too short, intermittent failures occur.

**Nika status**: **MITIGATED**. The `ping()` method uses a 10-second timeout
(client.rs line 618). The `connect()` method delegates to the adapter which
likely has its own timeout. But `npx` cold starts (which download packages)
can take 30+ seconds on slow connections.

**Detection**: Test MCP connection with `npx -y <uncached-package>` to simulate
cold start.

### 4.4 JSON-RPC Message Framing

**Problem**: MCP uses newline-delimited JSON over stdio. If a message contains
embedded newlines (in JSON string values), naive line-based parsing breaks the
protocol framing.

**Nika status**: **LOW risk**. The rmcp library handles framing. But if Nika
constructs JSON-RPC messages manually anywhere (bypassing rmcp), this could be
an issue.

### 4.5 Tool Schema Caching Staleness

**Problem**: MCP servers can update their available tools at runtime (via
`notifications/tools/list_changed`). If the client caches tool definitions
and doesn't handle this notification, stale schemas cause validation failures
for new tools or incorrect parameter validation.

**Nika status**: **MEDIUM risk**. The `McpClient` has a tool definition cache
with TTL (the `is_tool_cache_fresh()` method, client.rs line 1331, and the
`invalidate_tool_cache()` method). The TTL-based approach works but doesn't
react to server notifications. If an MCP server adds a new tool mid-workflow,
Nika won't discover it until the cache expires.

**Detection**: Test with an MCP server that adds tools dynamically. Verify
Nika discovers new tools within a reasonable time.

### 4.6 Concurrent Tool Calls on Same Server

**Problem**: The MCP specification (as of 2025) doesn't mandate that servers
handle concurrent requests. Some servers process requests serially, meaning
concurrent `call_tool()` invocations effectively serialize at the server.
Worse, some servers crash or return errors under concurrent access.

**Nika status**: **MEDIUM risk**. The `McpClient` supports concurrent calls
(tested in `test_concurrent_calls`, client.rs line 1414). But the test uses
a mock client. Real MCP servers may not handle concurrent calls gracefully.
When `for_each` with `concurrency: 10` all call the same MCP server, the
server may fail.

**Detection**: Test `for_each` with high concurrency against a real MCP server.
Consider adding per-server concurrency limits.

### 4.7 Error Response Content Parsing

**Problem**: MCP tool call responses can contain both `content` and `isError: true`.
The error content may be a text block, a JSON object, or empty. Inconsistent
parsing of error responses leads to lost error messages or type errors.

**Nika status**: **LOW risk**. The `ToolCallResult::is_error` field (used in
cache.rs line 280 to avoid caching errors) and the `ContentBlock` type suggest
structured error handling.

### 4.8 Server-Side State Mutation via Idempotent-Looking Tools

**Problem**: Caching MCP tool responses assumes tools are idempotent. But tools
like `novanet_write` mutate state. If a write tool's response is cached, subsequent
calls with the same params return the old result instead of executing the write.

**Nika status**: **MITIGATED** (partially). Error responses are not cached
(client.rs line 280). But successful write operations ARE cached if they have
the same parameters. The cache should exclude mutation tools.

**Detection**: Verify that mutation tools (`novanet_write`, `novanet_check`) are
not cached. Consider a cache exclusion list based on tool name patterns.

---

## 5. rig-core Rust LLM Framework Issues

### 5.1 Token Tracking with Tools Returns Zero

**Problem**: rig-core's `agent.prompt()` method (used when tools are present)
returns a `String` response without token usage metadata. The `model.stream()`
method (used without tools) provides token counts via `GetTokenUsage`.

**Nika status**: **KNOWN LIMITATION**. Documented in CLAUDE.md:
"When MCP tools are provided, `run_*()` methods return 0 tokens".
`chat_continue_*()` methods also return 0 tokens. This affects cost tracking
and rate limit estimation.

**Detection**: Already documented and tested. Impact is observability, not correctness.

### 5.2 Provider-Specific Error Handling

**Problem**: Different LLM providers return errors in different formats. rig-core
abstracts this, but provider-specific errors (rate limits, content filtering,
model not found) may be poorly categorized, making retry logic unreliable.

**How it manifests in Nika**: A rate limit error from OpenAI vs Anthropic has
different HTTP status codes and response bodies. If Nika retries on all errors
equally, it may waste retries on permanent failures (wrong API key) or not
retry enough on transient failures (rate limits).

**Nika status**: **MEDIUM risk**. The retry logic uses `McpRetryConfig` for
MCP calls, but LLM provider retry logic depends on rig-core's error types.
If rig-core doesn't distinguish retryable from non-retryable errors, Nika can't
either.

**Detection**: Test each provider with an expired API key and verify the error
is classified as non-retryable. Test with a very high request rate and verify
rate limit errors are retried.

### 5.3 Streaming Cancellation and Resource Cleanup

**Problem**: When streaming a response from an LLM, if the task is cancelled
(e.g., `fail_fast`), the HTTP connection must be properly closed. If not, the
connection stays open, consuming server resources and potentially counting
tokens for the abandoned response.

**Nika status**: **MEDIUM risk**. The `tokio::select!` pattern for `fail_fast`
should drop the streaming future when cancelled, which should close the HTTP
connection via Drop. But if rig-core uses a background task for streaming
(rather than a simple Future), cancellation may not propagate immediately.

**Detection**: Start a streaming infer task, then cancel it via `fail_fast`.
Verify no lingering HTTP connections (using `netstat` or `lsof`).

### 5.4 Chat History Memory Growth

**Problem**: Multi-turn conversations accumulate message history. For long agent
runs (many turns), the history can exceed the model's context window, causing
API errors or silent truncation.

**Nika status**: **MEDIUM risk**. The `agent:` verb supports `max_turns` to
limit iterations, but there's no explicit token budget for history. If each
turn adds a large response, the history can exceed context limits before
reaching `max_turns`.

**Detection**: Run an agent with `max_turns: 50` and large responses per turn.
Verify graceful handling when context window is exceeded.

### 5.5 Model Name Validation

**Problem**: rig-core accepts model names as strings. If a user specifies a
non-existent model (typo like `claude-sonnet-4-6` instead of `claude-sonnet-4-20250514`),
the error comes from the API at runtime, not at parse time.

**Nika status**: **LOW risk** for correctness (the API returns a clear error),
but **MEDIUM risk** for user experience. The analyzer could validate model
names against a known list at parse time.

**Detection**: Test with an invalid model name and verify the error message
is actionable.

### 5.6 System Prompt Handling Across Providers

**Problem**: Different providers handle system prompts differently:
- Anthropic: Separate `system` parameter
- OpenAI: First message with `role: "system"`
- Some providers: Don't support system prompts at all

rig-core should abstract this, but edge cases exist.

**Nika status**: **LOW risk**. The `InferOptions` struct passes `system` through
to rig-core, which handles provider-specific formatting. But if a provider
doesn't support system prompts, the behavior is undefined (silently ignored?
prepended to user prompt? error?).

---

## 6. Nika-Specific Cross-Cutting Concerns

### 6.1 Binding Resolution Ordering

**Problem**: The 3-pass template resolution (use -> context -> inputs) means
that a `{{context.files.x}}` reference inside a `use:` value would NOT be
resolved (correct security behavior). But it also means that a context file's
content cannot reference workflow inputs -- each pass is independent.

**Nika status**: **BY DESIGN** but may surprise users. Document this behavior.

### 6.2 Regex Denial of Service in Template Matching

**Problem**: Complex template strings with many nested `{{` could cause regex
backtracking. The `USE_RE` regex `\{\{\s*(?:use|with)\.(\w+(?:\.\w+)*)...\}\}`
uses nested quantifiers (`\w+(?:\.\w+)*`) which is theoretically safe but
worth verifying.

**Nika status**: **LOW risk**. The regex crate in Rust uses a guaranteed
linear-time NFA engine, so catastrophic backtracking is impossible. This is
a Rust advantage over other languages.

### 6.3 Cache Key Collision (FxHasher)

**Problem**: The MCP response cache uses `FxHasher` (client.rs line 247),
a non-cryptographic hash. While fast, it has higher collision rates than
SipHash. If two different tool+params combinations hash to the same key,
one response overwrites the other.

**Nika status**: **LOW risk**. FxHasher is used for the hash component of the
cache key, but the key format is `"tool:hash"` which includes the tool name.
Collisions would require the same tool name with different params hashing
to the same 64-bit value, which is astronomically unlikely for reasonable
workloads.

### 6.4 Task ID Naming Conflicts with Reserved Words

**Problem**: If a task is named `context`, `inputs`, `env`, or `item`, the
binding system could confuse it with a reserved namespace. For example:
```yaml
tasks:
  - id: context           # Task named "context"
  - id: step2
    with:
      data: context       # Task ref or reserved namespace?
```

**Nika status**: **HIGH risk** (needs audit). The `BindingPath::parse()` function
(types.rs line 140) checks reserved namespaces BEFORE task references. So `$context`
is always a context reference, never a task reference. A task named `context` would
be unreferenceable via bindings.

**Detection**: Test creating a task named `context`, `inputs`, or `env` and verify
a clear error message (or document that these names are reserved).

---

## Prioritized Audit Checklist

### HIGH Priority (Likely Impact)

| # | Finding | File(s) | Test Strategy |
|---|---------|---------|---------------|
| 1 | `node_to_json` precision loss for version strings | `parser.rs:328-361` | Unit test: `version: 3.10` |
| 2 | Reserved task names (`context`, `inputs`, `env`) | `types.rs:192-237` | Unit test: task named `context` |
| 3 | DAG completion race with concurrent tasks | `executor.rs`, `runner.rs` | Stress test: 50-way concurrency with deps |
| 4 | MCP cache for mutation tools | `client.rs:278-292` | Test: cache `novanet_write` result |

### MEDIUM Priority (Possible Impact)

| # | Finding | File(s) | Test Strategy |
|---|---------|---------|---------------|
| 5 | Null vs empty string in YAML scalars | `parser.rs:129-139` | Parse `description: ~` |
| 6 | YAML 1.1 boolean values in `node_to_json` | `parser.rs:339-344` | Parse `country: NO` in inputs |
| 7 | Task ID with hyphens in templates | `template.rs:88` | Template: `{{with.step-1}}` |
| 8 | MCP server concurrent call handling | `client.rs` | Real server with concurrency 10 |
| 9 | Stdio pipe deadlock with large payloads | `rmcp_adapter.rs` | 100KB params + 100KB response |
| 10 | EventLog unbounded memory growth | `event/log.rs` | Long agent run monitoring |
| 11 | Zombie MCP processes after SIGKILL | `rmcp_adapter.rs` | Force kill + process check |
| 12 | Chat history exceeding context window | `rig_agent_loop.rs` | 50-turn agent with large responses |
| 13 | Streaming cancellation cleanup | `rig_agent_loop.rs` | Cancel during stream + connection check |
| 14 | Provider-specific retry classification | `provider/rig.rs` | Rate limit vs auth error distinction |
| 15 | Stale tool cache after server updates | `client.rs:1331` | Dynamic tool addition test |
| 16 | Blocking FS calls in async context | `context_loader.rs` | Grep for `std::fs::` in async fns |

### LOW Priority (Unlikely but Aware)

| # | Finding | File(s) | Test Strategy |
|---|---------|---------|---------------|
| 17 | Multi-document YAML | `parser.rs` | Parse file with `---` separator |
| 18 | Unicode in task IDs | `parser.rs` | Task ID with emoji |
| 19 | Excessively large YAML | `parser.rs` | 10,000-task workflow |
| 20 | JoinSet panic propagation | `executor.rs` | Trigger panic in spawned task |

---

## Methodology

- **Sources consulted**: Known issues from GitHub Actions, Argo Workflows, Airflow,
  Temporal, Prefect, Serverless Workflow DSL spec, YAML 1.1/1.2 spec differences,
  MCP specification, rig-core GitHub issues, Tokio documentation on cancellation
  safety, Rust async working group reports on common pitfalls.
- **Codebase files analyzed**: `parser.rs` (1904 lines), `client.rs` (2052 lines),
  `template.rs` (header), `types.rs` (817 lines), `ids.rs` (288 lines),
  `analyze.rs` (header), `entry.rs`, `mod.rs` (binding), `transform.rs` (header).
- **Confidence level**: HIGH for YAML parsing and MCP issues (well-documented
  problem space), MEDIUM for race conditions (requires runtime testing to confirm),
  MEDIUM for rig-core issues (library evolving rapidly).

---

## Further Research Suggestions

1. **Fuzz testing** -- Use `cargo-fuzz` on `parse()` with arbitrary YAML inputs
   to find parser edge cases.
2. **Property-based testing** -- Use `proptest` to generate random task graphs
   and verify DAG invariants hold under concurrent execution.
3. **MCP conformance testing** -- Run the MCP test suite against Nika's client
   to verify protocol compliance.
4. **Memory profiling** -- Use `dhat` or `heaptrack` during long workflow runs
   to identify memory leaks in event logging and caching.
5. **Connection leak testing** -- Use `lsof` monitoring during MCP server
   lifecycle tests to detect file descriptor leaks.
