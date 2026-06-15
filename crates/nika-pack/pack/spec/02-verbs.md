# 02 · The 4 verbs

> Every task in a Nika workflow binds to **exactly one** of 4 verbs.
> A verb is a **distinct native execution model** the engine itself
> implements — call a model, run a process, dispatch a tool, drive an
> agentic loop. That is the whole operation space.
>
> Everything *callable* — fetching a URL, a database query, a file
> write, cognitive recall — is a **tool** reached through `invoke:`.
> Everything about *ordering* — iteration, branching — is a **DAG**
> construct (`for_each` · `when`). The 4 verbs never change; tools and
> the stdlib grow freely.

---

## The <!-- canon:verbs -->4<!-- /canon --> verbs · summary

| Verb | What it does | Stdlib it consumes |
|---|---|---|
| `infer:` | Single LLM call · text · structured · vision · thinking | providers |
| `exec:` | Shell command with sandboxing | (none · pure effect) |
| `invoke:` | Call a builtin tool OR an MCP tool | builtins · MCP servers |
| `agent:` | Multi-turn agentic loop with tool calls | providers · builtins · MCP |

A task **must** specify exactly one of these. Multiple verbs on a single task is a validation error.

> **Where did `fetch` go?** Fetching a URL is *calling a tool*, not a
> distinct execution model — so it is the `nika:fetch` builtin, reached
> through `invoke:` (the extract modes become its `mode` argument). Same
> reason a DB query (`invoke: mcp:postgres/query`) or a file write
> (`invoke: nika:write`) is not its own verb. See
> [stdlib/builtins-v0.1.md](../stdlib/builtins-v0.1.md).

### What `${{ tasks.<id>.output }}` holds · per verb

Every task also exposes the full result record (`.status` · `.error` ·
`.started_at` · `.ended_at` · `.duration_ms` · [04 §task output reference](./04-variables.md#-taskxoutput--task-output-reference)).
The `.output` shape depends on the verb (know this before you bind downstream) ·

| Verb | `.output` is | Structured when |
|---|---|---|
| `infer:` | the model's reply · **string** | `schema:` set → **object** matching it |
| `exec:` | **stdout string** (default) | `capture: structured` → `{ stdout, stderr, exit_code }` |
| `invoke:` | the **tool's response** (tool-defined · string · object · or bytes) | per builtin / MCP tool schema · bytes are tool-determined (MCP image content · binary read) · flow opaquely · see 04 §value rendering |
| `agent:` | the loop's **final message** · string | `schema:` set → **object** matching it |

---

## `infer:` · LLM call

A single LLM call. The result is the model's response.

### Minimal

```yaml
- id: greet
  infer:
    prompt: "Say hello in French"
```

### Full

```yaml
- id: research
  infer:
    prompt: "Research Rust async runtimes 2026 in 5 paragraphs"
    system: "You are a senior software architect."
    model: mistral/mistral-large         # override default · <provider>/<name>
    temperature: 0.3
    max_tokens: 2000
    schema:                            # optional · structured output
      type: object
      required: [summary, paragraphs]
      properties:
        summary: { type: string }
        paragraphs: { type: array, items: { type: string } }
    thinking:                          # optional · extended thinking
      enabled: true
      budget_tokens: 4000
    vision:                            # optional · vision input
      - source: file
        path: "./diagram.png"
```

### Fields

| Field | Required | Type | Notes |
|---|---|---|---|
| `prompt` | yes | string | User prompt · may use `${{ ... }}` substitution |
| `system` | no | string | System prompt |
| `model` | no | string | Override workflow default · `<provider>/<name>` · see stdlib/providers-v0.1.md |
| `temperature` | no | number 0-2 | Sampling temperature |
| `max_tokens` | no | integer | Max output tokens · provider-dependent default |
| `schema` | no | object | JSON Schema · structured output validation |
| `thinking` | no | object | Extended thinking · `{ enabled, budget_tokens }` |
| `vision` | no | array | Image inputs · each `{ source: file|url, path|url, … }` |

### Conformance

A v0.1-compliant engine MUST ·

- Call the configured provider with the given prompt + system + parameters
- Return the model's response as the task output
- Validate the response against `schema` if present · MAY auto-retry validation failures internally before surfacing `NIKA-INFER-002` (engine-configurable · the same rule as [05 §structured output](./05-errors.md#structured-output-validation))
- Reject any unknown field with a clear error (forward-compat) · or accept + warn (engine choice)

---

## `exec:` · shell command

Run a shell command. The result is the command's stdout (default) or a structured output.

### Minimal

```yaml
- id: build
  exec:
    command: "cargo build --release"
```

### Full

```yaml
- id: test
  timeout: "60s"                   # task-level (applies to any verb · Go duration · see 03-dag)
  exec:
    command: "cargo test --workspace --lib"
    cwd: "./engine"
    env:
      RUST_LOG: "debug"
    stdin: "${{ vars.input_data }}"
    capture: structured            # stdout | stderr | structured | combined
```

### Fields

| Field | Required | Type | Notes |
|---|---|---|---|
| `command` | yes | string \| array | **String** → run via the OS shell (`/bin/sh -c`) · pipes/redirects work · the shell blocklist applies. **Array** (`["prog", "arg", …]`) → direct `execve`, **NO shell** · each element substituted independently · the one-obvious-way for any command carrying interpolated or untrusted values (see Security). May use `${{ ... }}`. |
| `cwd` | no | string | Working directory · default = engine's cwd |
| `env` | no | object | OS environment variables for **this subprocess** · key→value map |
| `stdin` | no | string | Stdin data · may use `${{ ... }}` |
| `capture` | no | enum | `stdout` (default) · `stderr` · `combined` · `structured` (= `{ stdout, stderr, exit_code }`) |

> **`exec.env` ≠ the envelope `env:`** — different scopes, same word (the one
> overlap to know). The envelope `env:` is *workflow config* read via
> `${{ env.* }}`; `exec.env` is the *OS environment of this subprocess*. They
> are NOT auto-connected — to pass a workflow value into the process, do it
> explicitly: `env: { API_BASE: "${{ env.API_BASE }}" }`.

> `timeout` and `retry` are **task-level** fields (see [03-dag.md](./03-dag.md)) — they apply uniformly to every verb, so they are not repeated inside `exec:`.

### Security

A v0.1-compliant engine MUST ·

- **Honor the two `command` forms** · a **string** runs through `/bin/sh -c` (the shell-feature path · pipes · redirects); an **array** runs through `execve` with NO shell — `command[0]` is the program, the rest are argv passed verbatim, each element substituted independently.
- Implement a shell **blocklist** for dangerous commands on the STRING form (see reference impl `nika-policy` for canonical list · 100+ patterns including `rm -rf /` · `chmod 777` · `curl … | sh` · etc.)
- Reject blocklist matches with a clear error
- Honor `timeout` with a hard kill (Go-duration string · see 03-dag)
- Sandbox `cwd` if configured (engine-specific)

> **The argv form is the STRUCTURAL fix for command injection — prefer it.**
> A `${{ }}`-interpolated value in the STRING form is shell-parsed:
> `command: "process ${{ item }}"` with `item == "; rm -rf /"` is a classic
> injection, and the blocklist is a *detector* (best-effort), not a guarantee.
> The ARRAY form removes the shell entirely — `command: ["process", "${{ item }}"]`
> passes `item` as ONE argv element no matter what it contains; there is no
> shell to parse it. **Rule of thumb · any command carrying a task output, a
> `var`, or any value not authored inline → use the array form.** `nika check`
> warns (`one-obvious-way`) when an interpolated value lands in a string
> `command`. The string form stays first-class for genuine shell pipelines
> (`"cat x | grep y > z"`) — that is what `/bin/sh -c` is for.

### Conformance

The engine MUST ·

- Run the STRING command via the OS shell (`/bin/sh -c` on Unix · `cmd /c` on Windows, OR a sandboxed equivalent) · run the ARRAY command via `execve` with no shell
- Capture stdout/stderr as configured
- Return exit code in `structured` capture mode
- **Fail the task on non-zero exit** (`NIKA-EXEC-001` · `process_error`) in
  `stdout` / `stderr` / `combined` capture modes (unless `on_error:` recovers ·
  [05](./05-errors.md)) — **EXCEPT `capture: structured`**, where a non-zero
  exit is **data, not failure** · the task succeeds and `exit_code` is the
  workflow's to branch on (`when: ${{ tasks.test.output.exit_code != 0 }}`) ·
  the one-obvious-way split · default modes for « must succeed » · structured
  for « inspect the outcome »

---

## `invoke:` · builtin or MCP tool call

Call a builtin (`nika:` namespace) or an MCP tool (`mcp:` namespace). The result is the tool's response.

### Tool reference grammar (canonical)

```
<namespace>:<path>          one colon introduces the namespace · `/` separates the path

nika:fetch                  builtin · HTTP + extraction (used to be a verb)
nika:write                  builtin · flat name
nika:connectome/recall      builtin · grouped path
mcp:browser/navigate        MCP · server `browser` · tool `navigate`
mcp:postgres/query          MCP · server `postgres` · tool `query`
mcp:my-server/do_thing      MCP · server names admit kebab-case · tools admit snake_case
```

The `mcp:` form **requires the slash** — `mcp:postgres` alone (no tool) is a
parse error (`NIKA-PARSE` · `validation_error`) · server segment
`[a-z][a-z0-9-]*` · tool segment `[A-Za-z0-9_-]+` (tool names are the MCP
server's to define).

One rule everywhere · the **colon** marks the namespace boundary (exactly once),
the **slash** separates the path within it. No `::`, no mixed `.`/`:`. Globs are
clean · `nika:*` · `nika:connectome/*` · `mcp:browser/*`.

**The namespace set is CLOSED at v1** · `nika:` and `mcp:` are the only two ·
any other namespace (`custom:thing` · `x:tool`) is rejected at parse time
(`NIKA-PARSE` · `validation_error`). A third namespace would be an additive
spec minor — it never appears silently.

### Builtin call

```yaml
- id: read_config
  invoke:
    tool: "nika:read"
    args:
      path: "./config.yaml"
```

See [stdlib/builtins-v0.1.md](../stdlib/builtins-v0.1.md) for the canonical builtin list (<!-- canon:builtins -->23<!-- /canon --> builtins in v0.1).

> **Grouped paths are grammar-legal · v0.1 ships none.** The reference
> grammar admits `nika:<group>/<tool>` (the `nika:connectome/recall`
> illustration above is the reserved FUTURE shape) — but the v0.1 builtin
> set contains only flat names · a grouped `nika:` path is rejected against
> the closed 23 set today (`NIKA-INVOKE-001`). The seam exists so the
> Connectome tools land additively · zero workflow-shape change.

### MCP call

```yaml
- id: query_db
  invoke:
    tool: "mcp:postgres/query"
    args:
      sql: "SELECT * FROM users WHERE id = $1"
      params: ["${{ vars.user_id }}"]
```

The MCP server `postgres` must be configured in the engine's MCP server registry.

### Fields

| Field | Required | Type | Notes |
|---|---|---|---|
| `tool` | yes | string | Tool identifier · `nika:<path>` OR `mcp:<server>/<tool>` |
| `args` | no | object | Tool arguments · schema is tool-specific |

### Conformance

The engine MUST ·

- Resolve the tool identifier to its implementation (builtin or MCP server)
- Reject unknown tools with a clear error
- Pass `args` to the tool · validate against tool's schema if known
- Return the tool's response as task output

---

## `agent:` · multi-turn agentic loop

Run an agentic loop · the model + a set of tools · iterating until completion or budget exhausted.

### Minimal

```yaml
- id: research
  agent:
    system: "You are a research assistant."
    prompt: "Research the topic · ${{ vars.topic }}"
    tools: ["nika:fetch"]             # default-deny · grant explicitly
```

### Full

```yaml
- id: research
  agent:
    system: "You are a research assistant. Use tools to gather info."
    prompt: "Research the topic · ${{ vars.topic }} · and produce a markdown brief"
    model: mistral/mistral-large
    tools:                            # whitelist · default-deny (no tools if omitted)
      - "nika:fetch"
      - "nika:write"
      - "mcp:browser/*"               # all tools from browser MCP server
    max_turns: 20
    max_tokens_total: 100000
    temperature: 0.3
    schema:                           # optional · validate the final message as structured output
      type: object
      required: [findings]
      properties:
        findings: { type: array, items: { type: string } }
```

### Fields

| Field | Required | Type | Notes |
|---|---|---|---|
| `system` | no | string | System prompt |
| `prompt` | yes | string | Initial user message (same field name as `infer:` · consistent) |
| `model` | no | string | Override workflow default · `<provider>/<name>` |
| `tools` | no | array | Tool whitelist · glob patterns · **default-deny** · if absent the agent gets NO tools (pure conversation · least-privilege). Grant explicitly. |
| `max_turns` | no | integer | Loop limit · default 10 |
| `max_tokens_total` | no | integer | Cumulative token budget · default engine-configurable |
| `temperature` | no | number 0-2 | Sampling temperature |
| `schema` | no | object | JSON Schema · validates the agent's **final message** as structured output (same contract as `infer.schema:`) · `.output` becomes the matching object |

### Loop semantics

The agent loops · model response → if tool calls present, execute tools → feed results back to model → repeat. The loop terminates when ·

1. Model returns a final response with no tool calls, OR
2. `max_turns` reached, OR
3. `max_tokens_total` exhausted, OR
4. A tool returns the canonical completion sentinel `nika:done` (the builtin tool · see [stdlib/builtins-v0.1.md](../stdlib/builtins-v0.1.md))

`nika:done` is **valid only inside an `agent:` loop's tool whitelist** — it is
the loop-completion sentinel. Calling `nika:done` from a standalone `invoke:`
task (outside any agent loop) is an error (`NIKA-BUILTIN-DONE-001`). The
sentinel has no meaning without a loop to terminate. `nika:done` accepts an
optional **`result:`** arg (any JSON value) · when present the task's
`.output` is that value (and `schema:` validates IT) · when absent `.output`
is the final assistant message (string).

The agent may also grant itself **`nika:compose`** — a second loop-only
builtin (also valid ONLY inside an `agent:` whitelist · standalone is
`NIKA-BUILTIN-COMPOSE-001`). It lets the model **self-check a workflow it is
drafting**: pass a `workflow_yaml` string, get the full `nika check` verdict
back (conformance + secret-flow + permits + the termination/cost certificate)
as the tool result. It **never executes** the draft — verification yields an
artifact + its certificate, and running it stays a separate, gated decision.
See [stdlib/builtins-v0.1.md](../stdlib/builtins-v0.1.md) §`nika:compose`.

**Termination outcomes (normative)** ·

- Case 1 or 4 (natural completion · `nika:done`) → `status: success`.
- Case 2 (`max_turns` reached) → **`status: failure`** · `NIKA-AGENT-001` ·
  `budget_error` · the last assistant message is preserved at
  `error.details.partial_output` (a budget that runs out did NOT produce the
  asked-for result — failing loudly beats returning an unfinished answer ·
  recover the partial explicitly via `on_error:` if it is acceptable).
- Case 3 (`max_tokens_total` exhausted) → same shape · `NIKA-AGENT-002`.

**Tool-call errors inside the loop are fed back, not fatal** · a failing tool
call returns its typed error to the MODEL as the tool result (the standard
agentic convention — the model sees the failure and adapts) · the loop
continues against its budgets. The ONE exception · a `security_error`
(whitelist violation `NIKA-SEC-002` · blocklist) **fails the task
immediately** — security boundaries are not negotiation material for a model.

### Tool whitelist · glob semantics

The `tools:` whitelist uses **gitignore-style globs** for matching ·

```yaml
tools:
  - "nika:read"                  # exact match
  - "nika:*"                     # any nika builtin
  - "mcp:browser/*"              # any tool from the `browser` MCP server
  - "mcp:postgres/query"         # exact match · postgres query only
```

Match rules (canonical) ·

- `*` matches any sequence of characters EXCEPT the path separator `/` (so `mcp:browser/*` matches `mcp:browser/navigate` but not `mcp:browser/tab/open`)
- `**` matches any sequence including `/` (rare · use sparingly)
- The namespace colon is never crossed by `*` (so `nika:*` never matches an `mcp:` tool)
- Order matters · later rules override earlier rules
- Negation via `!` prefix · `tools: ["mcp:browser/*", "!mcp:browser/navigate"]`

A v0.1-compliant engine MUST implement these glob semantics canonically · NOT engine-specific. This is a portability invariant.

The task output is the final model response.

### Conformance

The engine MUST ·

- Honor the `tools` whitelist · reject tool calls not in the whitelist · **default-deny when `tools:` is absent** (the agent gets no tools · least-privilege)
- Enforce `max_turns` and `max_tokens_total` budgets · terminate on exhaustion
- Detect the `nika:done` completion sentinel and exit gracefully
- Return the final model response as task output

---

## Forward-compat

The 4 verb names are **immutable forever** — and the count is **4, absolute**. The operation space is complete: call a model (`infer`), run a process (`exec`), call a tool (`invoke`), run an agentic loop (`agent`). Every other capability is either an **invoke-able tool** (an HTTP fetch → `invoke: nika:fetch` · a database query → `invoke: mcp:postgres/query` · a file write → `invoke: nika:write` · cognitive recall → `invoke: nika:connectome/recall`) or a **DAG control-flow construct** (iteration → `for_each` · branching → `when`). A new verb would require a `nika: v2` contract — and per forever-v0.x, that is effectively never.

### The closure argument · why no case forces a 5th verb

Every candidate operation decomposes along **two orthogonal axes** ·

- **WHO executes** — the execution model. There are exactly four · model
  inference (`infer`) · OS process (`exec`) · dispatch-a-call-and-await-its-
  result (`invoke`) · model-driven loop over tools (`agent`). Anything with a
  request/response shape — however exotic the backend — is by construction
  the `invoke` model.
- **WHEN it runs** — ordering. Edges (`depends_on`) · conditions (`when`) ·
  iteration (`for_each`) · time-bounds (`timeout`) · recovery (`retry` ·
  `on_error` · `on_finally`). Ordering never executes anything — it is
  DAG-side, or host-side when it concerns *starting* a run at all.

The recurring « doesn't X need a verb? » cases, stress-tested ·

| Candidate | Axis | Resolution |
|---|---|---|
| **wait-for-human** (approval gate) | WHO · a call that resolves when a human answers | a **tool** · request/response with a long latency · `invoke:` model — **already shipped** as `nika:prompt` (blocking confirm · [stdlib/builtins-v0.1.md](../stdlib/builtins-v0.1.md)) |
| **sleep / delay** inside a run | WHO · a call that resolves after a duration | a **tool** — **already shipped** as `nika:wait` (relative `duration:` XOR absolute `until:`) · NOT a verb — nothing is executed, a result (elapsed) is awaited |
| **cron / schedule** (start a run at time T) | neither · it precedes the run | **host concern** · a workflow describes a run, not its triggering · explicitly out-of-scope ([08](./08-out-of-scope.md)) |
| **streaming between tasks** | WHEN · changes what an *edge* delivers | a **DAG-edge semantic**, not an execution model · out-of-scope v0.1 · an additive edge attribute later · never a verb |
| **sub-workflow call** | WHO · dispatch-and-await a run | a **tool** (workflow-as-tool · see [08](./08-out-of-scope.md) §composition) · the `invoke` model again |

A 5th verb would have to name an execution model that is none of
call-with-result, process, inference, or loop — and every concrete candidate
inspected since the 4-verb lock (D-2026-05-22-N18) has resolved to a tool
(WHO · request/response) or an ordering construct (WHEN). That is the
closure: **the verb set is closed under decomposition into WHO × WHEN.**

Field additions to each verb are **additive** within `nika: v1` (feature-detected · no minor version in the file). Field removal NEVER happens at v1.

A v0.1-compliant engine that encounters an unknown field on a verb may ·

- **Reject** with a clear error (strict mode · default for tests)
- **Warn + ignore** the field (lenient mode · default for production)

Engine config picks which mode.

---

🦋 *Next · [03 · DAG shape](./03-dag.md)*
