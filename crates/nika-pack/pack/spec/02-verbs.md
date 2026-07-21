# 02 · The 4 verbs

> Every task in a Nika workflow binds to **exactly one** of 4 verbs.
> A verb is a **distinct native execution model** the engine itself
> implements: call a model, run a process, dispatch a tool, drive an
> agentic loop. That is the whole operation space.
>
> Everything *callable* (fetching a URL, a database query, a file
> write, cognitive recall) is a **tool** reached through `invoke:`.
> Everything about *ordering* (iteration, branching) is a **DAG**
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
> distinct execution model, so it is the `nika:fetch` builtin, reached
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
greet:
    infer:
      prompt: "Say hello in French"
```

### Full

```yaml
research:
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
| `schema` | no | object | raw JSON Schema · structured output validation — the **out-of-core hatch**; the typed door is task-level `returns:` ([09](./09-types.md) · both on one task = `NIKA-TYPE-003`) |
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
build:
    exec:
      command: ["cargo", "build", "--release"]
```

### Full

```yaml
test:
    timeout: "60s"                   # task-level (applies to any verb · Go duration · see 03-dag)
    exec:
      command: ["cargo", "test", "--workspace", "--lib"]
      cwd: "./engine"
      env:
        RUST_LOG: "debug"
      stdin: "${{ inputs.input_data }}"
      capture: structured            # stdout | stderr | structured | combined
```

### Fields

| Field | Required | Type | Notes |
|---|---|---|---|
| `command` | one of | array | **argv** — `["prog", "arg", …]` → direct `execve`, **NO shell** · each element substituted independently — an interpolated `${{ }}` value can never break out of its argument (see Security). Exactly one of `command` \| `shell`. |
| `shell` | one of | string | **The explicit shell door** — one line run via `/bin/sh -c` · pipes · redirects · globs. The blocklist applies HERE; interpolating untrusted values here is the author's own risk, visibly. Exactly one of `command` \| `shell`. *(D1 · #75: semantics never fork on a YAML type — the field name carries the meaning. The old string `command:` is rejected at parse.)* |
| `cwd` | no | string | Working directory · default = engine's cwd |
| `env` | no | object | OS environment variables for **this subprocess** · key→value map |
| `stdin` | no | string | Stdin data · may use `${{ ... }}` |
| `capture` | no | enum | `stdout` (default) · `stderr` · `combined` · `structured` (= `{ stdout, stderr, exit_code }`) — the **source** |
| `decode` | no | enum | `text` (default) · `json` · `jsonl` · `bytes` — how the captured **string** becomes a value ([09 §decode](./09-types.md#decode--how-exec-bytes-become-a-value-normative)) · illegal with `capture: structured` (`NIKA-PARSE-025` — that capture already IS an object) · a non-parsing stream settles the task `failure` inside `on_error:` scope |

> **`exec.env` ≠ the envelope `config:`**: different scopes, neighboring words
> (the one overlap to know · the pre-flip envelope `env:` block is dead —
> `NIKA-VALUES-002`). The envelope `config:` is *workflow config* read via
> `${{ config.* }}`; `exec.env` is the *OS environment of this subprocess*. They
> are NOT auto-connected. To pass a workflow value into the process, do it
> explicitly: `env: { API_BASE: "${{ config.API_BASE }}" }`.

> `timeout` and `retry` are **task-level** fields (see [03-dag.md](./03-dag.md)): they apply uniformly to every verb, so they are not repeated inside `exec:`.

### Security

A v0.1-compliant engine MUST ·

- **Honor the two exec bodies** · `command:` (array) runs through `execve` with NO shell: `command[0]` is the program, the rest are argv passed verbatim, each element substituted independently. `shell:` (string) runs through `/bin/sh -c` — the shell-feature path (pipes · redirects · globs). **Exactly one** of the two; a string `command:` (the pre-0.103 implicit-shell form) is rejected at parse (`NIKA-PARSE` · `validation_error`).
- Implement a shell **blocklist** for dangerous commands on `shell:` (see reference impl `nika-policy` for canonical list · 100+ patterns including `rm -rf /` · `chmod 777` · `curl … | sh` · etc.)
- Reject blocklist matches with a clear error
- Honor `timeout` with a hard kill (Go-duration string · see 03-dag)
- Sandbox `cwd` if configured (engine-specific)

> **The argv form is the STRUCTURAL fix for command injection: prefer it.**
> A `${{ }}`-interpolated value in the STRING form is shell-parsed:
> `shell: "process ${{ item }}"` with `item == "; rm -rf /"` is a classic
> injection, and the blocklist is a *detector* (best-effort), not a guarantee.
> The ARRAY form removes the shell entirely: `command: ["process", "${{ item }}"]`
> passes `item` as ONE argv element no matter what it contains; there is no
> shell to parse it. **Rule of thumb · any command carrying a task output, a
> `var`, or any value not authored inline → use the array form.** `nika check`
> warns (`one-obvious-way`) when an interpolated value lands in a string
> `command`. The string form stays first-class for genuine shell pipelines
> (`"cat x | grep y > z"`). That is what `/bin/sh -c` is for.

### Conformance

The engine MUST ·

- Run the STRING command via the OS shell (`/bin/sh -c` on Unix · `cmd /c` on Windows, OR a sandboxed equivalent) · run the ARRAY command via `execve` with no shell
- Capture stdout/stderr as configured
- Return exit code in `structured` capture mode
- **Fail the task on non-zero exit** (`NIKA-EXEC-001` · `process_error`) in
  `stdout` / `stderr` / `combined` capture modes (unless `on_error:` recovers ·
  [05](./05-errors.md)), **EXCEPT `capture: structured`**, where a non-zero
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

The `mcp:` form **requires the slash**: `mcp:postgres` alone (no tool) is a
parse error (`NIKA-PARSE` · `validation_error`) · server segment
`[a-z][a-z0-9-]*` · tool segment `[A-Za-z0-9_-]+` (tool names are the MCP
server's to define).

One rule everywhere · the **colon** marks the namespace boundary (exactly once),
the **slash** separates the path within it. No `::`, no mixed `.`/`:`. Globs are
clean · `nika:*` · `nika:connectome/*` · `mcp:browser/*`.

**The namespace set is CLOSED at v1** · `nika:` and `mcp:` are the only two ·
any other namespace (`custom:thing` · `x:tool`) is rejected at parse time
(`NIKA-PARSE` · `validation_error`). A third namespace would be an additive
spec minor. It never appears silently.

### Builtin call

```yaml
read_config:
    invoke:
      tool: "nika:read"
      args:
        path: "./config.yaml"
```

See [stdlib/builtins-v0.1.md](../stdlib/builtins-v0.1.md) for the canonical builtin list (<!-- canon:builtins -->28<!-- /canon --> builtins in v0.1).

> **Grouped paths are grammar-legal · v0.1 ships none.** The reference
> grammar admits `nika:<group>/<tool>` (the `nika:connectome/recall`
> illustration above is the reserved FUTURE shape), but the v0.1 builtin
> set contains only flat names · a grouped `nika:` path is rejected against
> the closed 23 set today (`NIKA-INVOKE-001`). The seam exists so the
> Connectome tools land additively · zero workflow-shape change.

### MCP call

```yaml
query_db:
    invoke:
      tool: "mcp:postgres/query"
      args:
        sql: "SELECT * FROM users WHERE id = $1"
        params: ["${{ inputs.user_id }}"]
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
research:
    agent:
      system: "You are a research assistant."
      prompt: "Research the topic · ${{ inputs.topic }}"
      tools: ["nika:fetch"]             # default-deny · grant explicitly
```

### Full

```yaml
research:
    agent:
      system: "You are a research assistant. Use tools to gather info."
      prompt: "Research the topic · ${{ inputs.topic }} · and produce a markdown brief"
      model: mistral/mistral-large
      tools:                            # whitelist · default-deny (no tools if omitted)
        - "nika:fetch"
        - "nika:write"
        - "mcp:browser/*"               # all tools from browser MCP server
      skills:                           # Agent Skills · SKILL.md paths (agentskills.io shape)
        - ".agents/skills/nika-authoring/SKILL.md"
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
| `skills` | no | array | [Agent Skill](https://agentskills.io) file paths (each an agentskills.io-shape `SKILL.md`) · **explicit static paths only** — no globs, no `${{ }}` templates (the `permits:` explicitness law) · loaded at **compose time** and injected into the system context (§Agent Skills below) |
| `max_turns` | no | integer | Loop limit · default 10 |
| `max_tokens_total` | no | integer | Cumulative token budget · default engine-configurable |
| `temperature` | no | number 0-2 | Sampling temperature |
| `schema` | no | object | raw JSON Schema · validates the agent's **final message** as structured output (same contract as `infer.schema:` · the out-of-core hatch — the typed door is `returns:` · [09](./09-types.md)) · `.output` becomes the matching object |

### Loop semantics

The agent loops · model response → if tool calls present, execute tools → feed results back to model → repeat. The loop terminates when ·

1. Model returns a final response with no tool calls, OR
2. `max_turns` reached, OR
3. `max_tokens_total` exhausted, OR
4. A tool returns the canonical completion sentinel `nika:done` (the builtin tool · see [stdlib/builtins-v0.1.md](../stdlib/builtins-v0.1.md))

`nika:done` is **valid only inside an `agent:` loop's tool whitelist**: it is
the loop-completion sentinel. Calling `nika:done` from a standalone `invoke:`
task (outside any agent loop) is an error (`NIKA-BUILTIN-DONE-001`). The
sentinel has no meaning without a loop to terminate. `nika:done` accepts an
optional **`result:`** arg (any JSON value) · when present the task's
`.output` is that value (and `schema:` validates IT) · when absent `.output`
is the final assistant message (string).

The agent may also grant itself **`nika:compose`**, a second loop-only
builtin (also valid ONLY inside an `agent:` whitelist · standalone is
`NIKA-BUILTIN-COMPOSE-001`). It lets the model **self-check a workflow it is
drafting**: pass a `workflow_yaml` string, get the full `nika check` verdict
back (conformance + secret-flow + permits + the termination/cost certificate)
as the tool result. It **never executes** the draft: verification yields an
artifact + its certificate, and running it stays a separate, gated decision.
See [stdlib/builtins-v0.1.md](../stdlib/builtins-v0.1.md) §`nika:compose`.

**Termination outcomes (normative)** ·

- Case 1 or 4 (natural completion · `nika:done`) → `status: success`.
- Case 2 (`max_turns` reached) → **`status: failure`** · `NIKA-AGENT-001` ·
  `budget_error` · the last assistant message is preserved at
  `error.details.partial_output` (a budget that runs out did NOT produce the
  asked-for result: failing loudly beats returning an unfinished answer ·
  recover the partial explicitly via `on_error:` if it is acceptable).
- Case 3 (`max_tokens_total` exhausted) → same shape · `NIKA-AGENT-002`.

**Tool-call errors inside the loop are fed back, not fatal** · a failing tool
call returns its typed error to the MODEL as the tool result (the standard
agentic convention: the model sees the failure and adapts) · the loop
continues against its budgets. The ONE exception · a `security_error`
(whitelist violation `NIKA-SEC-002` · blocklist) **fails the task
immediately**: security boundaries are not negotiation material for a model.

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

### Agent Skills · `skills:`

The `skills:` field makes the agent a **consumer** of the
[Agent Skills](https://agentskills.io) format — the same `SKILL.md` shape
`nika init` already produces. Each entry is an **explicit file path** to a
skill file: a markdown document opening with a YAML frontmatter block
(`---` fences) that carries a non-empty `name` and `description`; the
markdown body after the closing fence is the skill's instructions. Other
frontmatter keys (`license` · `metadata` · client-specific fields) are the
skill author's surface — a consumer MUST tolerate them.

```yaml
review:
    agent:
      system: "You are a code reviewer."
      prompt: "Review the diff · ${{ tasks.diff.output }}"
      skills:
        - ".agents/skills/code-review/SKILL.md"
        - "docs/skills/security-pass/SKILL.md"
      tools: ["nika:read"]
```

**Resolution (normative)** · paths are **static** — no globs, no
`${{ }}` templates (the same explicitness law as `permits:`; a template
in a skill path is a parse error). Relative paths resolve from the
working directory, like every other relative path in the language. The
files are read at **compose time** — before the run starts, by the
composition layer, never mid-loop by the runtime — so skill reads are
**outside `permits.fs`** (exactly like the workflow file itself: what
the agent will carry is fixed and auditable before any effect).

**Injection (normative)** · the resolved skills join the agent's system
context as ONE deterministic section, appended after the authored
`system:` (or standing alone when `system:` is absent) ·

```text
<authored system>

## Skills

### <frontmatter name>

<frontmatter description>

<markdown body · trimmed>
```

— one `###` block per skill, in `skills:` source order. Same inputs,
same bytes (provider-cache-friendly · reproducible transcripts).

**check≡run** · `skills:` is fully validated statically. `nika check`
fails a workflow whose skill paths do not resolve (`NIKA-AGENT-003` ·
the file is missing/unreadable) or whose files are not valid Agent
Skills (`NIKA-AGENT-004` · no/unterminated/non-mapping frontmatter ·
missing/empty `name` or `description`) — see
[05-errors.md](./05-errors.md). A run refuses on the same findings
before any token is spent. The skill TEXT also joins the referencing
task's resume identity: editing a `SKILL.md` re-runs the task (the same
law as an edited prompt — a resume never serves output produced under a
different skill).

### Conformance

The engine MUST ·

- Honor the `tools` whitelist · reject tool calls not in the whitelist · **default-deny when `tools:` is absent** (the agent gets no tools · least-privilege)
- Enforce `max_turns` and `max_tokens_total` budgets · terminate on exhaustion
- Detect the `nika:done` completion sentinel and exit gracefully
- Return the final model response as task output
- Resolve `skills:` at compose time · inject the `## Skills` section in source order exactly as specified above · fail statically (`NIKA-AGENT-003`/`NIKA-AGENT-004`) on a missing or invalid skill file — never start a run with a half-composed context

---

## Forward-compat

The 4 verb names are **ratified by the [v1 constitution](../design/v1-constitution.md)**, and the count is **4, absolute** — under the constitutional hierarchy of **three atomic calls and one bounded controller**: call a model once (`infer`), apply a typed callable once (`invoke`), run a host process once (`exec`), and orchestrate a bounded sequence of the atomic calls (`agent`). Every other capability is either an **invoke-able callable** (an HTTP fetch → `invoke: nika:fetch` · a database query → `invoke: mcp:postgres/query` · a file write → `invoke: nika:write` · cognitive recall → `invoke: nika:connectome/recall`) or a **DAG control-flow construct** (iteration → `for_each` · branching → `when`). There is no `nika: v2` — ever: while the reference engine is pre-1.0 a verb change would be a break INSIDE `v1` (per the [pre-1.0 stability contract](./00-overview.md#pre-10-stability-contract)), and after engine 1.0.0 the verb set is frozen with the rest of the grammar, so a fifth verb is effectively never.

### The closure argument · why no case forces a 5th verb

Every candidate operation decomposes along **two orthogonal axes** ·

- **WHO executes**: the execution model. There are exactly four · model
  inference (`infer`) · OS process (`exec`) · dispatch-a-call-and-await-its-
  result (`invoke`) · model-driven loop over tools (`agent`). Anything with a
  request/response shape (however exotic the backend) is by construction
  the `invoke` model.
- **WHEN it runs**: ordering. Edges (`with:` data · `after:` control) ·
  conditions (`when`) · iteration (`for_each`) · time-bounds (`timeout`) ·
  recovery (`retry` · `on_error` · `on_finally`). Ordering never executes
  anything: it is DAG-side, or host-side when it concerns *starting* a run
  at all.

The recurring « doesn't X need a verb? » cases, stress-tested ·

| Candidate | Axis | Resolution |
|---|---|---|
| **wait-for-human** (approval gate) | WHO · a call that resolves when a human answers | a **tool** · request/response with a long latency · `invoke:` model · **already shipped** as `nika:prompt` (blocking confirm · [stdlib/builtins-v0.1.md](../stdlib/builtins-v0.1.md)) |
| **sleep / delay** inside a run | WHO · a call that resolves after a duration | a **tool** · **already shipped** as `nika:wait` (relative `duration:` XOR absolute `until:`) · NOT a verb · nothing is executed, a result (elapsed) is awaited |
| **cron / schedule** (start a run at time T) | neither · it precedes the run | **host concern** · a workflow describes a run, not its triggering · explicitly out-of-scope ([08](./08-out-of-scope.md)) |
| **streaming between tasks** | WHEN · changes what an *edge* delivers | a **DAG-edge semantic**, not an execution model · out-of-scope v0.1 · an additive edge attribute later · never a verb |
| **sub-workflow call** | WHO · dispatch-and-await a run | a **tool** (workflow-as-tool · see [08](./08-out-of-scope.md) §composition) · the `invoke` model again |

A 5th verb would have to name an execution model that is none of
call-with-result, process, inference, or loop, and every concrete candidate
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
