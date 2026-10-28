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
> reason a DB query (`invoke: { tool: "mcp:postgres/query" }`) or a file
> write (`invoke: { tool: "nika:write" }`) is not its own verb. See
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
| `agent:` | the loop's **final message** · string | `schema:` set → **object** matching it · `images:` set → `{ message, images }` ([§Harness images](#harness-images--images-normative)) |

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
      model: mistral/mistral-large-latest  # override default · <provider>/<name>
      temperature: 0.3
      max_tokens: 16384                  # room for the thinking budget and the answer
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
        - source: file                   # a read · permits.fs.read admits the path
          path: "./diagram.png"
```

### Fields

| Field | Required | Type | Notes |
|---|---|---|---|
| `prompt` | yes | string, or `{ file: <path> }` | User prompt · may use `${{ ... }}` substitution · or the text of a project file ([§Project files](#project-files---file--normative)) |
| `system` | no | string, or `{ file: <path> }` | System prompt · or the text of a project file ([§Project files](#project-files---file--normative)) |
| `model` | no | string | Override workflow default · `<provider>/<name>` · see stdlib/providers-v0.1.md |
| `temperature` | no | number 0-2 | Sampling temperature |
| `max_tokens` | no | integer | Max output tokens · provider-dependent default · an answer the provider cuts at this cap fails (`NIKA-INFER-005`) · a model that reasons before it answers draws its reasoning from this budget: give it room (16,384), or a low `run.reasoning.effort` its route documents; too little returns an empty answer (`NIKA-INFER-004`) |
| `schema` | no | object, or `{ file: <path> }` | raw JSON Schema, inline or kept in a project file ([§Project files](#project-files---file--normative)) · structured output validation — the **out-of-core hatch**; the typed door is task-level `returns:` ([09](./09-types.md) · both on one task = `NIKA-TYPE-003`). Numeric facts (`0\|1\|3`, counts, coded levels) are `type: integer` with a numeric `enum` — never `enum: ["0","1","3"]`. Models emit JSON numbers; a string-digit enum can reject the call before coerce (reference engine hint `digit-string-enum`). The model extracts facts; `nika:jq` or `nika:decide` is the law ([11](./11-decision.md) · `examples/13-extract-then-law.nika`) |
| `thinking` | no | object | Extended thinking · `{ enabled, budget_tokens }` |
| `vision` | no | array | Image inputs · each `{ source: file, path: <path> }`, a read inside `permits.fs.read`, or `{ source: url, url: <url> }`, fetched by the provider and never by Nika ([§Vision inputs](#vision-inputs--vision-normative)) |

### Project files · `{ file: }` (normative)

A prompt or a schema kept beside the workflow is part of the program when the
field names it. `prompt:` and `system:` (of `infer:` and of `agent:`) take
either a string or the closed mapping `{ file: "<path>" }`, whose text is the
field's value. `schema:` (of `infer:` and of `agent:`) takes either a JSON
Schema written inline or the same mapping, whose file holds the schema.

```yaml
tasks:
  brief:
    infer:
      system: { file: "./prompts/editor.md" }
      prompt: { file: "./prompts/brief.md" }   # may use ${{ inputs.topic }} like inline text
      schema: { file: "./schemas/brief.json" } # one JSON Schema, judged as if written inline
```

```yaml
permits:
  fs: { read: ["./prompts/**", "./schemas/**"] } # the files reach the model · reads inside the boundary
```

- **The shape is closed.** The mapping carries exactly one key, `file`, a
  non-empty string. Another key is `NIKA-PARSE-005`; any other shape is the
  field's type error. Under `schema:`, a mapping that holds `file` is the file
  form, never an inline schema, so `file` beside any other key is refused
  there too. Written inline, `{ file }` alone would constrain nothing (an
  unknown keyword), so no meaningful schema is lost.
- **The path law.** The path is static (no `${{ }}`, no glob) and relative
  (no leading `/`, no `\`, no drive letter, no URI scheme). It resolves from
  the directory of the workflow file that names it, like a `skills:` path and
  a composed child's target ([14](./14-composition.md)), never from the
  working directory. A leading `./` is allowed; an empty segment, any other
  `.` segment and a hidden name (a segment beginning with `.` other than
  `..`, such as `.env` or `.ssh`) are not.
- **Containment by the project root.** Joined to the naming file's folder
  and normalized lexically, the path stays inside the **project root**: the
  folder holding the project file (`nika.yaml`) that governs the run, or the
  root the run admitted, or, when no project file governs it, the root
  workflow's own folder; a registry package's root is its package. A `..`
  segment is allowed while the path stays inside, so a child in
  `workflows/` names `../prompts/brief.md` at the project root, and a `..`
  that climbs past the root is refused. Resolved, inside the boundary, no
  component between the project root and the file is a symbolic link. A
  broken path law is `NIKA-FILE-001`.
- **The authority law.** The file's content reaches the model provider (a
  text is sent, a schema shapes the reply it asks for), so the read is the
  class of `nika:read` and of a `skills:` file: the path as the naming
  workflow writes it must be admitted by that workflow's declared
  `permits.fs.read` boundary (a child that names `../prompts/brief.md`
  grants that path), judged
  **before** the read (`NIKA-AUTH-006` under an absent block · `NIKA-SEC-004`
  outside a declared one), exactly as [§Agent Skills](#agent-skills--skills)
  states.
- **The read law.** The file is read at **compose time**, before the run and
  never mid-run. A missing, unreadable or non-regular file is
  `NIKA-FILE-002`. Its bytes MUST be valid UTF-8 and at most **65,536**;
  otherwise `NIKA-FILE-003`.
- **One templating law.** A prompt or system file's text is interpolated
  exactly as the same text written inline in the field: the same `${{ … }}`
  scopes and the same VAR laws ([04](./04-variables.md)), judged statically at
  check. A `${{` inside such a file is never literal, and a secret reaching the
  provider through a file meets the same secret-flow laws as one written
  inline ([10](./10-authority.md)).
- **The schema content law.** A schema file holds exactly one JSON Schema:
  JSON text, or YAML under the profile of
  [01](./01-envelope.md#yaml-profile-normative), whose value is a mapping. Every
  `$ref` it holds is a fragment of the file itself (`#…`): a reference to
  another file or to a URL would reach content the read boundary never judged.
  A schema file that parses as neither, holds no mapping, or holds a `$ref`
  that leaves it is `NIKA-TYPE-007`. The schema it holds is then judged by
  every law the same schema written inline meets, and like an inline schema it
  is never interpolated: a `${{` in it is text.
- **Program bytes, bound by their digest.** The content is the field's value
  for every purpose: the Semantic IR carries it resolved (a text as text, a
  schema as the schema), so the semantic identity and the task's resume
  identity commit to it ([15](./15-proof.md)) and editing the file re-runs the
  task; the trace records the path and the digest of the bytes read; a consent
  preview shows the content as it shows the inline field; a run bound to an
  admitted world refuses a project file whose digest moved, as it refuses a
  moved child.
- **A door that reads no files says so.** A checker that judges a source
  without its files (an editor buffer, a source string) still judges the
  field's shape and path law, names each project file it did not read, and
  never reports their read, authority, templating or content laws as held.
- **Registry packages carry their project files.** A registry child's
  project file resolves inside its own package, pinned by the package digest,
  never in the caller's tree.

Order of judgment: the shape, the path as written and its containment,
then the authority law, then the resolution (no symbolic link) and the read
law, then the templating law over a text or the content law over a schema.
An ungranted path is never opened.

**A private key in a prompt is flagged (normative for linters ·
advisory).** A conformant linter warns `egress/001 private-key-in-prompt`
when a `prompt:` or `system:` text, inline or the text of a prompt file,
holds a PEM private-key block (a line `-----BEGIN … PRIVATE KEY-----`): a
credential bound for the model provider. The rule judges inline text and
file text alike, never one more strictly than the other, and refuses
nothing; a secret the provider needs belongs in `secrets:`, whose egress
laws apply ([10](./10-authority.md)).

### Vision inputs · `vision:` (normative)

`vision:` lists the images sent with the prompt. Each entry is
`{ source: file, path: <path> }`, an image file the engine reads, or
`{ source: url, url: <url> }`, an image the provider fetches.

```yaml
tasks:
  describe:
    infer:
      prompt: "Describe the flow this diagram shows"
      vision:
        - source: file
          path: "./diagrams/flow.png"            # read by the engine · its bytes reach the provider
        - source: url
          url: "https://example.com/legend.png"  # fetched by the provider · never by Nika
```

```yaml
permits:
  fs: { read: ["./diagrams/**"] }   # the image reaches the provider · a read inside the boundary
```

**A vision file is a read inside `permits.fs.read`.** The engine reads the
file and sends its bytes to the model provider with the request: a read
whose bytes reach a destination, the class of `nika:read`, of a `skills:`
file ([§Agent Skills](#agent-skills--skills)) and of a project file
([§Project files](#project-files---file--normative)). Its path must be
admitted by the workflow's declared `permits.fs.read` boundary, matched as
the `path:` of a `nika:read` is, and the boundary is judged **before** the
read: an ungranted path is never opened.

| the workflow declares | a `source: file` entry is | code |
|---|---|---|
| no `permits:` block at all | an effect under an absent block · zero authority | `NIKA-AUTH-006` |
| a `permits:` block whose `fs.read` does not admit the path | outside the declared boundary | `NIKA-SEC-004` |
| a boundary that admits the path | read when the task runs · its bytes ride the request | — |
| a boundary that admits the path, and no file there can be read | the read's own failure, before any provider call | `NIKA-BUILTIN-READ-001` (no file at the path) · `NIKA-BUILTIN-READ-002` (any other read failure) |

**When the path is judged.** A literal path, or a whole bare `const`
reference to a declared string, is judged at check, before any token. Any
other form is judged at run on its rendered value, in the lexical canonical
form every fs effect is matched in (`.` and `..` resolved, separators
collapsed), before the file is opened: an escape fails the task
`NIKA-SEC-004`. Whatever the form, whether the boundary admits any read at
all is known before the run: an entry under no `permits:` block
(`NIKA-AUTH-006`), or under a block whose `fs.read` admits no path
(`NIKA-SEC-004`), is refused at check, since no rendered value can fit an
empty grant. The inferred boundary (`nika check --infer-permits`) lists
every workspace path judged at check in `fs.read`, so the block it prints
re-checks clean; a path that escapes the workspace (absolute, `~`, or a
leading `..` after lexical normalization) is named for review and never
printed as a grant, and its task stays refused until the author points it
inside the workspace or grants the host path deliberately.

**A URL is not an fs effect.** A `source: url` entry hands its URL to the
provider, which fetches the image. Nika never fetches it, so the entry needs
no `fs` grant and no `net.http` grant, and `permits: {}` admits it.

**The entries are part of the request.** The URL is part of what the request
sends the provider, and a file path chooses the bytes it sends: both are
provider egress, governed by the secret-flow rules ([01
§egress](./01-envelope.md#egress--optional--sanctioned-destinations-declassification)
· [10](./10-authority.md)), not by a capability category. A secret in a
`vision:` entry is the `infer` sink's leak (`NIKA-SEC-006`) unless its
`egress:` sanctions `{ to: "infer" }`, exactly as a secret in the prompt is.
For the content-taint rules the entries are part of the task's effect
surface: untrusted content that picks a path or a URL picks what the model
sees, so the task's answer carries it (`NIKA-SEC-009`), as a `nika:read`
whose path it picks does.

### Conformance

A v0.1-compliant engine MUST ·

- Call the configured provider with the given prompt + system + parameters
- Return the model's response as the task output
- Validate the response against `schema` if present · MAY auto-retry validation failures internally before surfacing `NIKA-INFER-002` (engine-configurable · the same rule as [05 §structured output](./05-errors.md#structured-output-validation))
- **Fail an answer cut at the output cap** · when the provider reports that it stopped at `max_tokens` (Anthropic `max_tokens` · OpenAI-compatible `length` · Gemini `MAX_TOKENS`), the task fails with `NIKA-INFER-005` (`budget_error` · not transient): the visible text received is preserved at `error.details.partial_output` and the stop reason at `error.details.stop_reason`. A cut answer is never the task output and is never validated against `schema:` (it would fail `NIKA-INFER-002` and hide its cause); an empty visible answer stays `NIKA-INFER-004`. On a route that reasons by default the cap also pays for the reasoning. A partial answer that is acceptable is recovered explicitly through `on_error:`
- **Reject** any unknown field with a clear error (`NIKA-PARSE-005`) — the choice this line used to leave open is closed by [07 §unknown key](./07-conformance.md#an-unknown-key-is-an-error-at-every-level-normative--d-2026-08-11-n20): a key can REMOVE authority, so accept-and-warn fails open
- Resolve a `{ file: }` prompt, system or schema at compose time under [§Project files](#project-files---file--normative) · the path law (`NIKA-FILE-001`), the read boundary before the read (`NIKA-AUTH-006` · `NIKA-SEC-004`), the read law (`NIKA-FILE-002` · `NIKA-FILE-003`), then the inline templating law over a text or the schema content law over a schema (`NIKA-TYPE-007`) · never start a run with an unread or unjudged project file
- Judge every `vision:` file against the declared `permits.fs.read` boundary BEFORE opening it ([§Vision inputs](#vision-inputs--vision-normative)) · at check for a literal or whole bare `const` path (`NIKA-AUTH-006` under an absent block · `NIKA-SEC-004` outside a declared one), and for any form under a boundary that admits no read · at run for any other form (`NIKA-SEC-004`) · fail a granted file that cannot be read with the read's own code (`NIKA-BUILTIN-READ-001` · `-002`) before any provider call · treat every `vision:` entry as a provider-egress sink of the task (`NIKA-SEC-006` · `NIKA-SEC-009`) · never fetch a `source: url` image

### Subscription harness access (normative)

An engine MAY execute `infer:` through a user's authenticated CLI seat
instead of a provider API key, but only after an **infer-grade meet** proves
all four properties for that adapter and this task:

1. `single_turn` — one request produces one terminal answer;
2. `no_implicit_tools` — an accepted answer contains no tool, command, file,
   network, or permission event;
3. `structured_output` — the task's need (plain text, JSON, or JSON Schema)
   is met, by the adapter's own level or, for JSON and JSON Schema, by the
   engine validating the terminal answer under the `schema` law of
   [§Conformance](#conformance): the answer that does not validate is never
   the task output (`NIKA-INFER-002`), and each repair the engine spends is a
   new request that meets all four properties again;
4. `model_identity` — the requested model identity is observable at command
   construction and is recorded without being relabelled as the model that
   answered.

The meet is evidence, not a product-name allowlist. A seat with no attestation,
or one failing any conjunct, MUST be refused before inference with a witness
that names the failed property. It MUST NOT fall back to a native provider or
to the seat's agentic loop. In particular, an ACP-capable agent seat is not
thereby infer-grade.

For an accepted seat run, the receipt MUST record the seat id and the
**requested** model. It MUST leave the responding model absent unless the
adapter reports that fact explicitly; request identity is never promoted into
response identity. Subscription inference has no numeric token count and no
numeric price in the Nika receipt or budget. Its budget lane is
`subscription quota`, never `0 USD`. An adapter may read a terminal usage
object as protocol-completeness evidence, but MUST NOT project its numeric
meters into the Nika receipt.

A non-executing preview that accepts an access pin MUST apply the same access
admission meet as a live run and record the requested pin plus each requested
model. The same invalid or unattested pin MUST refuse with the same access
code. For an admitted subscription seat, the preview MUST name
`subscription quota` and MUST NOT display a numeric USD or token meter.

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
| `env` | no | object | OS environment variables for **this subprocess** · key→value map · applied over the COMPOSED environment ([01 §permits](./01-envelope.md#permits--optional--the-declared-capability-boundary) env · never inherited) |
| `stdin` | no | string | Stdin data · may use `${{ ... }}` |
| `capture` | no | enum | `stdout` (default) · `stderr` · `combined` · `structured` (= `{ stdout, stderr, exit_code }`) — the **source** |
| `decode` | no | enum | `text` (default) · `json` · `jsonl` · `bytes` — how the captured **string** becomes a value ([09 §decode](./09-types.md#decode--how-exec-bytes-become-a-value-normative)) · illegal with `capture: structured` (`NIKA-PARSE-025` — that capture already IS an object) · a non-parsing stream settles the task `failure` inside `on_error:` scope |

> **`exec.env` is a subprocess field, never an envelope block**: no top-level
> key of that family survives — the pre-flip envelope `env:` block is dead
> (`NIKA-VALUES-002`) and the top-level `config:` block died with the envelope
> nuke, so `exec.env` is the *OS environment of this subprocess* and nothing
> else. Nothing is auto-connected. To pass a workflow value into the process,
> do it explicitly: `env: { API_BASE: "${{ inputs.api_base }}" }`. The subprocess
> environment itself is COMPOSED, never inherited (NEP-0005 ·
> [01 §permits](./01-envelope.md#permits--optional--the-declared-capability-boundary)):
> the runner env floor ∪ the `permits.env:` passthrough ∪ this map, minus
> the dangerous-name floor · an ambient engine variable reaches the child
> only through a declared `permits.env:` name.

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
  workflow's to branch on (bind `with: { code: "${{ tasks.test.output.exit_code }}" }`,
  then `when: "${{ with.code != 0 }}"`) ·
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
`[a-z0-9][a-z0-9-]*` (a digit-initial name such as `21st` is legal) · tool
segment `[A-Za-z0-9_-]+` (tool names are the MCP server's to define).

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

See [stdlib/builtins-v0.1.md](../stdlib/builtins-v0.1.md) for the canonical builtin list (<!-- canon:builtins -->31<!-- /canon --> builtins in v0.1).

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
      model: mistral/mistral-large-latest
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
| `system` | no | string, or `{ file: <path> }` | System prompt · or the text of a project file ([§Project files](#project-files---file--normative)) |
| `prompt` | yes | string, or `{ file: <path> }` | Initial user message (same field name and forms as `infer:` · consistent) |
| `model` | no | string | Override workflow default · `<provider>/<name>` |
| `tools` | no | array | Tool whitelist · glob patterns · **default-deny** · if absent the agent gets NO tools (pure conversation · least-privilege). Grant explicitly. |
| `skills` | no | array | [Agent Skill](https://agentskills.io) file paths (each an agentskills.io-shape `SKILL.md`) · **explicit static paths only** — no globs, no `${{ }}` templates (the `permits:` explicitness law) · loaded at **compose time**, **inside `permits.fs.read`** (the text reaches the model), and injected into the system context (§Agent Skills below) |
| `max_turns` | no | integer | Loop limit · default 10 |
| `max_tokens_total` | no | integer | Cumulative token budget · default engine-configurable |
| `temperature` | no | number 0-2 | Sampling temperature |
| `schema` | no | object, or `{ file: <path> }` | raw JSON Schema, inline or kept in a project file ([§Project files](#project-files---file--normative)) · validates the agent's **final message** as structured output (same contract as `infer.schema:` · the out-of-core hatch — the typed door is `returns:` · [09](./09-types.md)) · `.output` becomes the matching object |
| `images` | no | object | `{ save?: <path> }` · makes the images an agent application delivered during its turn a value of the task, and with `save:` writes them in the project ([§Harness images](#harness-images--images-normative)) · a task that runs on a seat (`run.access.protocol: acp`) only · `.output` becomes `{ message, images }` |

### Loop semantics

The agent loops · model response → if tool calls present, execute tools → feed results back to model → repeat. The loop terminates when ·

1. Model returns a final response with no tool calls **and the effective
   whitelist does not grant `nika:done`**, OR
2. `max_turns` reached, OR
3. `max_tokens_total` exhausted, OR
4. A tool returns the canonical completion sentinel `nika:done` (the builtin tool · see [stdlib/builtins-v0.1.md](../stdlib/builtins-v0.1.md))

When the effective whitelist grants `nika:done` (including through a glob,
after applying exclusions), a text-only response MUST NOT complete the task.
The engine MUST preserve that assistant turn and ask the model to continue
using tools or call `nika:done` to finish. The next request MUST fit the
existing turn and cumulative-token budgets; exhausted budgets retain their
`NIKA-AGENT-001` / `NIKA-AGENT-002` failures and partial output. This
continuation is not a schema-repair attempt. A task without the sentinel
grant retains natural text completion, including at a budget boundary.

Runtime fixtures: `agent/004-text-with-done-exhausts-turns`,
`agent/005-text-without-done-completes`, and
`agent/006-text-with-done-exhausts-tokens`.

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
drafting**: pass a `workflow_yaml` string and receive the in-memory static
report (Core conformance, secret-flow and capability-escape counts, and a
termination/cost certificate summary). `valid` reports parsing and Core
conformance only: child files are not read, composition is not resolved, and
execution is not admitted. Check the saved file and its children through the
file-aware `nika check` before claiming it is ready to run. The intrinsic
**never executes** the draft; running it remains a separate, gated decision.
The teaching shape is
[`examples/15-compose-self-check.nika`](../examples/15-compose-self-check.nika)
(`nika:done` first so `mock/echo` closes at turn one). See
[stdlib/builtins-v0.1.md](../stdlib/builtins-v0.1.md) §`nika:compose`.
Do not confuse this loop intrinsic with parent→child composition
(`invoke: { workflow: "./child.nika" }` · [14](./14-composition.md) · lesson 10).

**Termination outcomes (normative)** ·

- Case 1 or 4 (natural completion · `nika:done`) → `status: success`.
- Case 2 (`max_turns` reached) → **`status: failure`** · `NIKA-AGENT-001` ·
  `budget_error` · the last assistant message is preserved at
  `error.details.partial_output` (a budget that runs out did NOT produce the
  asked-for result: failing loudly beats returning an unfinished answer ·
  recover the partial explicitly via `on_error:` if it is acceptable).
- Case 3 (`max_tokens_total` exhausted) → same shape · `NIKA-AGENT-002`.
- A final response (case 1) that the provider cut at the turn's output cap (`max_tokens`) is not a completion → same shape · `NIKA-AGENT-006` · the cut message at `error.details.partial_output` (the rule of `NIKA-INFER-005`).

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
    with:
      diff: ${{ tasks.diff.output }}    # the binding is the edge · the body reads the local name
    agent:
      system: "You are a code reviewer."
      prompt: "Review the diff · ${{ with.diff }}"
      skills:
        - ".agents/skills/code-review/SKILL.md"
        - "docs/skills/security-pass/SKILL.md"
      tools: ["nika:read"]
```

**Resolution (normative)** · paths are **static** — no globs, no
`${{ }}` templates (the same explicitness law as `permits:`; a template
or a glob in a skill path is a parse-time `NIKA-AGENT-003` — it can
never resolve). Relative paths resolve from the **directory of the
workflow file that names them**, exactly like a composed `workflow:`
child target ([14](./14-composition.md)) — never from the working
directory (`nika check sub/wf.nika` run from the repo root reads
`sub/skills/…`, whatever the operator's cwd). The files are read at
**compose time** — before the run starts, by the composition layer,
never mid-loop by the runtime.

**Skill reads are INSIDE `permits.fs.read`.** A skill file is not
program text the way a composed child is: a child's *effects* are
re-judged under the parent's boundary and its text goes nowhere, while
a skill's *text* is carried into the model's context — a file read whose
bytes reach a destination, the same class as `nika:read`. So the path
as written must be admitted by the workflow's declared read boundary
(the lexical `permits.fs.read` match every fs effect uses):

| the workflow declares | a `skills:` entry is | code |
|---|---|---|
| no `permits:` block at all | an effect under an absent block · zero authority | `NIKA-AUTH-006` |
| a `permits:` block whose `fs.read` does not admit the path | outside the declared boundary | `NIKA-SEC-004` |
| a boundary that admits the path · the file is missing or unreadable | unresolvable at compose time | `NIKA-AGENT-003` |
| a boundary that admits the path · the file is not a valid Agent Skill | resolvable, malformed | `NIKA-AGENT-004` |
| a boundary that admits the path · a well-shaped `SKILL.md` | admitted · injected | — |

The boundary is judged **before** the read: an ungranted path is never
opened, so the refusal cannot leak whether the file exists or how its
first line is shaped. (Measured on the shipped 0.108.0, 2026-08-12:
under `permits: {}` — the declared zero — a skill path of `/etc/hosts`
was read and the engine reported on its content while the permits and
trifecta rungs stayed green. The earlier text of this paragraph, «
skill reads are outside `permits.fs` », was the sentence that
authorized it; it is retired here.)

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
fails a workflow whose skill paths lie outside the declared read
boundary (`NIKA-AUTH-006` under an absent block · `NIKA-SEC-004` under
one that does not admit the path), do not resolve (`NIKA-AGENT-003` ·
the file is missing/unreadable · a template or glob) or whose files are
not valid Agent Skills (`NIKA-AGENT-004` · no/unterminated/non-mapping
frontmatter · missing/empty `name` or `description`) — see
[05-errors.md](./05-errors.md). A run refuses on the same findings
before any token is spent. The skill TEXT also joins the referencing
task's resume identity: editing a `SKILL.md` re-runs the task (the same
law as an edited prompt — a resume never serves output produced under a
different skill).

### Harness images · `images:` (normative)

An `agent:` task that runs on an agent application's seat
(`run.access.protocol: acp` · [01 §run](./01-envelope.md)) can receive the
images the application generates during its turn. `images:` makes those images
a value of the task, and `save:` writes them in the project, so a later task
reads the file the engine wrote instead of asking the application to copy one ·

```yaml
banner:
    agent:
      prompt: "Draw a 16:9 release banner for ${{ inputs.product }}, no text"
      images:
        save: "./out/release/banner.png"     # optional · the engine writes the received bytes
halftone:
    with:
      banner: ${{ tasks.banner.output.images[0].path }}
    invoke:
      tool: "nika:image_fx"
      args: { input: "${{ with.banner }}", out: "./out/release/banner-halftone.png", ops: [{ halftone: {} }] }
```

- **The value.** With `images:` set, `.output` is the object
  `{ message, images }`. `message` holds what `.output` holds without
  `images:` (the final message, or the object matching `schema:`). `images`
  lists the images in the order the session delivered them, each
  `{ index, path, sha256, size_bytes, mime_type, width, height }`: `index`
  counts from 0, `path` is the project-relative path the engine wrote (`null`
  without `save:`), `sha256` digests the bytes received, and `width` and
  `height` come from the image header.
- **This session's bytes only.** An entry is an image whose bytes the engine
  received on this task's own session, during this attempt. An update
  addressed to another session is never this task's. A path the application
  reports (where it says it saved a copy, a folder of generated images that
  other sessions share) is a claim: the engine never opens it, never lists it
  as a value and never saves from it.
- **`save:`** · a path, `${{ }}` allowed. The engine writes the first image's
  received bytes at that path, and image *n* (2 ≤ *n* ≤ 4) at the path with
  `-n` inserted before its extension (`banner.png`, then `banner-2.png`): a
  save names at most four files, and a turn that delivers a fifth image (an
  image whose bytes arrived · a reported path is not one) fails the task
  before any file is written. Every path is judged against
  `permits.fs.write` before any byte is written to it (`NIKA-AUTH-006` under
  an absent block, `NIKA-SEC-004` outside a declared one). At check: all four
  for a literal path or a whole bare `const` reference to a declared string,
  since how many images arrive is known only at run, and a save of any form
  under a boundary whose `fs.write` admits no path. At run, before the first
  write, every path the run will write, a rendered one included, is fitted
  against the declared boundary in the lexical form the check uses: a grant
  that holds the first path and not a sibling refuses the delivery with
  nothing written. Each path is then judged on its resolved form before its
  own write, so a save spelled in a form the lexical fit cannot judge (an
  absolute or `~` path) meets that judgment write by write, and a refused
  sibling can follow an image already written. For the
  same-path law ([10](./10-authority.md#the-effect-vocabulary-normative--closed))
  the save mutates all four paths. A write replaces an existing file
  atomically. A task whose session delivered no image bytes fails
  `NIKA-AGENT-007`: the declared file is never left unwritten behind a
  success.
- **A seat's field.** On a route without a seat (the engine's own loop over an
  API or local model) `images:` is refused before the first request, and
  `check` says so for the route it resolves: that loop's images are its tools'
  outputs (`nika:image_generate`).
- **Evidence.** Every received image keeps its trace observation
  ([17 §Harness image observations](./17-trace.md#harness-image-observations-additive));
  a saved one also records there the project-relative `saved_path`, so a run's
  artifact list carries the file with the digest of the bytes received.

### Conformance

The engine MUST ·

- Honor the `tools` whitelist · reject tool calls not in the whitelist · **default-deny when `tools:` is absent** (the agent gets no tools · least-privilege)
- Enforce `max_turns` and `max_tokens_total` budgets · terminate on exhaustion
- Detect the `nika:done` completion sentinel and exit gracefully
- Return the final model response as task output
- Resolve `skills:` at compose time · judge every path against the declared `permits.fs.read` boundary BEFORE opening it (`NIKA-AUTH-006` under an absent block · `NIKA-SEC-004` outside a declared one) · inject the `## Skills` section in source order exactly as specified above · fail statically (`NIKA-AGENT-003`/`NIKA-AGENT-004`) on a missing or invalid skill file — never start a run with a half-composed context
- Under `images:` · make the images received on the task's own session, and only those, the `images` list of `.output` · never open a path the application reports · write each received image at its `save:` path inside `permits.fs.write` before the task succeeds, fitting every path it writes before the first write and judging each before its own write, or fail `NIKA-AGENT-007` when none was received · fail a turn that delivers a fifth image to a save before any write · refuse `images:` on a route without a seat before the first request

---

## Forward-compat

The 4 verb names are **ratified by the [v1 constitution](../design/v1-constitution.md)**, and the count is **4, absolute** — under the constitutional hierarchy of **three atomic calls and one bounded controller**: call a model once (`infer`), apply a typed callable once (`invoke`), run a host process once (`exec`), and orchestrate a bounded sequence of the atomic calls (`agent`). Every other capability is either an **invoke-able callable** (an HTTP fetch → `invoke: { tool: "nika:fetch" }` · a database query → `invoke: { tool: "mcp:postgres/query" }` · a file write → `invoke: { tool: "nika:write" }` · cognitive recall → `invoke: { tool: "nika:connectome/recall" }`, the reserved future shape) or a **DAG control-flow construct** (iteration → `for_each` · branching → `when`). There is no `nika: v2` — ever: while the reference engine is pre-1.0 a verb change would be a break INSIDE `v1` (per the [pre-1.0 stability contract](./00-overview.md#pre-10-stability-contract)), and after engine 1.0.0 the verb set is frozen with the rest of the grammar, so a fifth verb is effectively never.

### The closure argument · why no case forces a 5th verb

Every candidate operation decomposes along **two orthogonal axes** ·

- **WHO executes**: the execution model. There are exactly four · model
  inference (`infer`) · OS process (`exec`) · dispatch-a-call-and-await-its-
  result (`invoke`) · model-driven loop over tools (`agent`). Anything with a
  request/response shape (however exotic the backend) is by construction
  the `invoke` model.
- **WHEN it runs**: ordering. Edges (`with:` data · `after:` control) ·
  conditions (`when`) · iteration (`for_each`) · time-bounds (`timeout`) ·
  recovery (`retry` · `on_error` · an `unwind` task). Ordering never executes
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

Field additions to each verb are **additive** within `v1` (feature-detected · no version marker in the file at all). Field removal NEVER happens at v1.

A v0.1-compliant engine that encounters an unknown field on a verb may ·

- **Reject** with a clear error (strict mode · default for tests)
- **Warn + ignore** the field (lenient mode · default for production)

Engine config picks which mode.

---

🦋 *Next · [03 · DAG shape](./03-dag.md)*
