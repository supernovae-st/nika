# 01 · Envelope

> Every Nika workflow starts with one header line. That line names the
> language and pins the contract version. Everything else is the
> workflow id, optional defaults, and the task graph.

---

## Minimal envelope

```yaml
nika: v1
workflow:
  id: my-workflow-id

tasks:
  my_task:
    ...
```

The `nika:` header, a `workflow:` object with an `id`, and a non-empty
`tasks:` map. That's the **whole minimum** to be a valid Nika workflow.

---

## Full envelope

```yaml
nika: v1                                # required · language + contract version
workflow:                               # required · the workflow object (W1: the map)
  id: scrape-and-summarize              #   required · kebab-case · unique within file
  description: "Fetch + summarize"      #   optional · human-readable

# Workflow-level default model · any task may override · <provider>/<name>
model: ollama/qwen3.5:4b         # optional · anthropic/claude-sonnet-4-6 for cloud

# Named types · referenced by returns:/outputs: (09-types.md)
types:
  Summary: { object: { title: string, bullets: { array: string } } }

# Typed workflow inputs · supplied by the caller at launch · available as ${{ inputs.<name> }}
inputs:
  topic:                                 # every entry is typed (the full TypeExpr of 09-types)
    type: string
    required: true
    description: "Subject to research"

# Named constants · fixed values baked into the workflow · available as ${{ const.<name> }}
const:
  output_dir: "./output"                 # bare literal

# Non-sensitive runtime config · supplied by the deployment · available as ${{ config.<name> }}
config:
  log_level: { type: string, default: "info" }

# Sensitive values · vault-backed · masked in logs · available as ${{ secrets.<name> }}
secrets:
  api_key:
    source: vault                        # never inline · a reference to a store
    key: prod/anthropic/api-key

# The declared capability boundary · the file IS the blast radius (optional · default-deny once present)
permits:
  net:  { http: ["api.anthropic.com"] }  # only this host · everything else denied
  fs:   { write: ["./output/**"] }       # no reads · writes only under ./output
  exec: false                            # this workflow runs zero shells

# Tasks (the DAG) · map keyed by task id
tasks:
  some_task:
    ...

# What this workflow returns · ${{ tasks.<id>.output }} refs · untyped OR typed
outputs:
  summary: ${{ tasks.summarize.output }}
```

---

## YAML profile (normative)

A workflow file is **YAML 1.2 · core schema**, restricted by the **Nika
YAML profile** (the R11 law set · [`canon/laws/yaml-profile.yaml`](../canon/laws/yaml-profile.yaml)).
Two consequences authors hit ·

- **Anchors & aliases (`&x` / `*x`) are FORBIDDEN**: the profile refuses
  an anchor even when it is never referenced (LAW-GRAMMAR-0101/0102 ·
  dedicated diagnostics `NIKA-YAML-001` / `NIKA-YAML-002`; the reference
  engine already refuses them at parse). What a Nika file shows a reviewer
  IS what the engine sees — aliasing expands invisible bytes into the
  document and opens the reference-bomb class, so repeated blocks are
  repeated in the source, not aliased.
- **Merge keys (`<<:`) are FORBIDDEN too**: YAML 1.2 dropped them (they
  were a 1.1 extension) · parser support varies · a portable workflow
  MUST NOT use them · refused with `NIKA-YAML-003` (LAW-GRAMMAR-0103).

The complete closed profile (duplicate keys · custom tags · non-string
keys · NaN/Infinity · depth and size caps · UTF-8 NFC · no BOM) is the
R11 law set above; its dedicated prose chapter ships with the
law-projection wave. Until then the law file is normative and
[05-errors](./05-errors.md#error-code-namespaces) allocates the
`NIKA-YAML` namespace.

(YAML 1.2 core also kills the 1.1 traps: `no` is a string, not `false` ·
`3:22` is a string, not sexagesimal. The quoted-duration rule of
[03 §timeout](./03-dag.md#timeout--optional--task-level-timeout-go-duration-string)
adds the belt to those suspenders.)

---

## Field-by-field

### `nika` · **required · the contract version**

```yaml
nika: v1
```

The first line of every workflow. The key `nika` declares « this is a
Nika workflow »; the value `v1` pins the **language contract version**.

`v1` is the only value for the entire lifetime of the v1 contract. Minor
additions to the language (a new optional field, a new builtin) are
**additive** and never change this value. **There is no `nika: v2` — ever**:
while the reference engine is pre-1.0, deep grammar changes happen INSIDE
`v1` (per the [pre-1.0 stability contract](./00-overview.md#pre-10-stability-contract));
after engine 1.0.0, changes are additive only. (This is the **language**
version, independent of any engine version.)

**Anti-pattern** · do not write `nika: v1.0` · `nika: "1"` · or
`nika: 1.0`. The value is exactly `v1`.

> **Why one field, not `apiVersion` + `schema`?** Earlier drafts used a
> Kubernetes-style `apiVersion: nika.sh/v1` (the superseded ADR-021 form)
> plus a separate `schema: nika/workflow@v1` (superseded too). That is two version-ish fields and
> ceremony a workflow file does not need. Modern specs converge on a
> single version marker: OpenAPI writes `openapi: 3.1.0`, Docker
> Compose dropped its `version:` field entirely. Nika takes the
> middle, proven path: **one field, the language name as the key, the
> contract version as the value.** The engine's internal canonical URI
> stays `https://nika.sh/spec/v1` for RDF / conformance tooling, but
> the author never types a URL.

### `workflow` · **required · an OBJECT · `{id, description}`**

```yaml
workflow:
  id: scrape-and-summarize
  description: "Fetch article, summarize in 3 bullets, write to disk"
```

The workflow object — a stable home for the file's identity and metadata.

- **`id`** · required · kebab-case (`^[a-z][a-z0-9-]*$`) · unique within
  file · used in journal events, traces and error messages.
- **`description`** · optional · free-form text · not used by the engine
  for execution · useful for `nika ls` listings + LSP hover hints.

The presence of `workflow:` is also the **document-type discriminator**:
it marks this file as a workflow. Future Nika document types (if any ever
ship) would use their own top-level key; there is no separate `kind:`
field in v1.

**Dead forms (rejected with a migration teaching · W1 « the map »)** ·
a scalar `workflow: some-id` is `NIKA-PARSE-020`; a top-level
`description:` is `NIKA-PARSE-021` — both moved INTO the object. No alias
survives: the old spellings left the parser in the same window that
introduced the object.

### `model` · *optional · default model · `<provider>/<name>`*

```yaml
model: ollama/qwen3.5:4b               # local
# model: anthropic/claude-sonnet-4-6    # cloud · same shape
```

Default model for any `infer:` or `agent:` verb in this workflow, as a single
**`<provider>/<name>`** string (the LiteLLM / OpenRouter / Vercel convention:
there is no separate `provider:` field). The provider prefix selects the
backend and decides local-vs-cloud (`ollama/` · `lmstudio/` = local · the rest
= cloud). See [stdlib/providers-v0.1.md](../stdlib/providers-v0.1.md) for the
<!-- canon:providers -->17<!-- /canon -->-provider catalog.

A task may override this. If absent · each `infer:`/`agent:` task must specify
its own `model:`.

### `types` · *optional · named type declarations*

```yaml
types:
  Summary:
    object:
      title: string
      bullets: { array: string }
```

Named, PascalCase, acyclic type declarations — referenced by task
`returns:` and typed `outputs:`. The grammar, the subtyping lattice and
the JSON-Schema lowering are the whole of [09-types.md](./09-types.md);
the envelope only owns the block's position. Unknown names are
`NIKA-TYPE-001` · recursion is `NIKA-TYPE-002`.

### `inputs` · *optional · typed workflow inputs*

```yaml
inputs:
  topic:
    type: string                 # the full TypeExpr of 09-types.md · named types · unions · shapes · refinements
    required: true               # default false · the caller MUST supply a value
    default: "Rust async 2026"   # used when the caller omits it · MUST conform to type:
    description: "Subject to research"
```

Inputs available in every task via `${{ inputs.<name> }}` substitution.

Every entry is a **typed declaration** (`type:` required). The type speaks
the **full TypeExpr** of [09-types.md](./09-types.md) — the flat 6-enum
(`string` · `number` · `integer` · `boolean` · `array` · `object`) is dead
and `bool` is the one boolean spelling (R3b · LAW-GRAMMAR-0211). An unknown
type name or a form outside the grammar refuses `NIKA-TYPE-001`; a
`default:` that does not conform to its `type:` refuses at check
(`NIKA-DEFAULT-001` · the P0 soundness hole is closed). Typed `inputs:` let
the engine validate what a caller passes and **generate a callable schema**:
this is what powers `nika.run_workflow` over MCP (a caller like an agent
host sees the typed inputs and knows exactly what to pass) and UI
generation. Typed `inputs:` are the **input** half of that callable
contract; typed [`outputs:`](#outputs--optional--the-workflows-return-value--untyped-or-typed)
(below) are the **output** half.

**Supplying values at launch** · the caller provides inputs when starting
the run; how is an engine CLI concern. The reference engine's surface ·
`nika run flow.nika.yaml --var topic="Rust async 2026"` (repeatable · one
`--var key=value` per input). A supplied value **overrides** a declared
`default:` and **satisfies** a `required: true` input (conformance rule 5
below · a missing required input rejects before execution). A value parses
as JSON when it parses (`--var limit=5` is a number · `--var deep=true` a
boolean) · else it rides as a string. An **unknown key is refused** before
the run, with the declared set listed: a typo that silently did nothing
would be the worst outcome. The file stays the contract · every input a
caller can pass is declared in `inputs:`.

### `config` · *optional · non-sensitive runtime config*

```yaml
config:
  log_level: { type: string, default: "info" }
  region:    { type: string }                     # no default · the deployment supplies it
```

Non-sensitive configuration available via `${{ config.<name> }}`, supplied
by the deployment or environment (engine launch concern · `default:` is the
declared fallback). Each entry is a typed declaration (`type:` required ·
full TypeExpr · a `default:` MUST conform · `NIKA-DEFAULT-001`). Values may
appear in logs and traces. For anything secret, use `secrets:` instead.

**Declared-only · no ambient OS fallback** · a `${{ config.X }}` read
resolves ONLY against this block (an undeclared entry is `NIKA-VAR-001`):
the engine never silently reads the OS environment — every value a workflow
depends on is visible in the file (sovereignty + portability).

### `const` · *optional · named constants*

```yaml
const:
  output_dir: "./output"              # bare literal · any YAML value
  retries: 3
  window:                             # typed constant · object carrying BOTH type and value
    type: integer
    value: 30                         # MUST conform to type: (NIKA-DEFAULT-001)
```

Fixed values baked into the workflow, available via `${{ const.<name> }}`.
Either a **bare literal** or a **typed constant** `{ type, value }` (the
`type:` speaks the full TypeExpr; the `value:` MUST conform to it).
The discriminator (normative) · an object carrying BOTH `type` and `value`
keys IS a typed constant; an object missing either key is a bare literal
object constant — so a literal that legitimately contains a `type` key
(`settings: { type: "custom" }`) is never misread. Constants are immutable
across the run and never caller-supplied: a value the caller may override
is an `inputs:` declaration, not a constant.

See [04-variables.md](./04-variables.md) for the full substitution grammar.

> **Dead forms (rejected with a classification teaching · the E-split ·
> R3a).** The pre-flip `vars:` and `env:` envelope fields are dead:
> `vars:` refuses `NIKA-VALUES-001`, `env:` refuses `NIKA-VALUES-002`.
> Classify each old use into the authority its role commands — a typed
> parameter is an `inputs:` declaration, a fixed value is a `const:` entry,
> non-sensitive runtime configuration is a `config:` declaration, a
> governed store reference is a `secrets:` entry (classify-not-rename ·
> no alias survives · LAW-SURFACE-0201/0202 · LAW-GRAMMAR-0201/0202).

### `secrets` · *optional · vault-backed · masked*

```yaml
secrets:
  api_key:
    source: vault                 # vault | env | file · never an inline value
    key: prod/anthropic/api-key
  github_token:
    source: env                   # read from the OS env var below · still masked
    key: GITHUB_TOKEN
  signing_pem:
    source: file                  # read file contents at resolve time · masked
    path: ~/.keys/signing.pem
```

The shape is **discriminated by `source:`** (required) · `vault` and `env`
require `key:` · `file` requires `path:` · an entry with neither (or an
inline literal value) is rejected (`NIKA-PARSE` · `validation_error`).

Sensitive values available via `${{ secrets.<name> }}`. A secret is always
a **reference to a store**, never an inline literal. The engine **masks**
resolved secret values in logs, traces, and journal events.

**`source` · closed enum** (the only three v0.1 values) ·

| `source` | `key` means | Use |
|---|---|---|
| `vault` (default) | path in the local `nika-vault` | the sovereign default |
| `env` | name of an OS environment variable | 12-factor / CI secrets |
| `file` | path to a file holding the value | Docker / k8s mounted secrets |

The `config` / `secrets` split is the modern secure-workflow default: non-sensitive
config in `config:` (appears in logs), masked references in `secrets:` (never
logged). Note `source: env` reads a *secret* from an OS env var and still masks
it, which is different from the plain `config:` block.

#### `egress` · *optional · sanctioned destinations (declassification)*

By default, a `secrets.<name>` value reaching ANY effect, an `exec:` or
`invoke:` sink, OR an `infer:`/`agent:` **prompt** (`prompt:` + `system:`),
is a **blocking leak**: the engine masks its own output but cannot follow a
secret a subprocess, a tool, or a third-party provider re-emits (`nika check`
refuses the workflow — `NIKA-SEC-006`, the taint path in the diagnostic;
a tainted value reaching `outputs:` is `NIKA-SEC-007` ·
[10 §secret flow](./10-authority.md#secret-flow-refusals-carry-their-codes-normative)). An `infer:`/`agent:` prompt is a
provider-egress sink like any other: a secret in it LEAVES the run to the
provider, so it needs an explicit sanction (`egress: [{ to: "infer" }]` /
`{ to: "agent" }`) exactly as a tool sink does.

The ONE carve-out is the infer/agent **output**, NOT the prompt: a model
response is not a verbatim echo of its prompt, so `tasks.<id>.output` of an
`infer:`/`agent:` task is never tainted by a prompt secret (it does not
re-leak downstream).

But legitimate workflows MUST send a secret to other sinks: a webhook-URL
secret to `nika:notify`, an API key in a `nika:fetch` header. The optional
`egress:` list on a secret **sanctions** exactly those destinations · the
author declassifies their OWN secret, in-file, statically checkable.

```yaml
secrets:
  stripe_key:
    source: env
    key: STRIPE_API_KEY
    egress:
      - to: "nika:fetch"        # the SPECIFIC sink · a tool id or "exec"
        host: "api.stripe.com"  # the static-literal destination host
  slack_webhook:
    source: env
    key: SLACK_WEBHOOK_URL
    egress:
      - to: "nika:notify"
        host_from_self: true    # the secret value IS the URL (host unknown statically)
```

**Default-deny.** An absent / empty `egress:` is the current behavior,
unchanged: NO sanctioned egress, every `exec:`/`invoke:` reach is a leak.

**Semantics (normative) · a secret→sink edge is sanctioned iff ALL hold** ·

| Layer | Rule |
|---|---|
| **① confidentiality** | the sink's id equals an `egress[].to:`: a tool id (`nika:fetch` · `mcp:<server>/<tool>`), `exec`, or the provider-egress sinks `infer` / `agent` (an infer/agent prompt). `to:` is SPECIFIC: a clearance for `nika:fetch` never authorizes `exec` (no cross-tool laundering). |
| **② integrity** | for a network sink, either `host:` equals the effect's **static-literal** destination host (a templated `${{ }}`-derived host does NOT sanction · it stays the runtime check), OR `host_from_self: true` AND the destination arg is **exactly** `${{ secrets.<this> }}` (not concatenated) AND **no other secret co-occurs** in the same effect payload. A sink with no addressable host (`{ to }` alone) clears this layer. |
| **③ capability** | when a [`permits:`](#permits--optional--the-declared-capability-boundary) block is present and the host is statically known, the host MUST ALSO be in `permits.net.http`: `egress:` NARROWS the capability boundary, never widens it. `host_from_self` (host unknown statically) degrades to the runtime `permits` check. |

`to:` is required. `host:` and `host_from_self:` are mutually exclusive (a host
is a literal you can check OR the secret itself, never both). Sanctioning the
egress clears the **send**, not the **capture**: a sanctioned `exec:`/tool can
still embed the secret in its captured output, so that output stays tainted and
re-leaks if it reaches an unsanctioned sink downstream.

**The workflow boundary is a sink too — `to: "outputs"`.** Because the
capture stays tainted, the return value of an authenticated call
(`tasks.<id>.output` of a fetch that sent an API key) taints every
`outputs:` entry it reaches — and `outputs:` is where a result LEAVES the
run, so the check reports it as an egress. That over-approximation is
deliberate (the provider saw the key; its response is not provably
clean), and so is the release valve: the secret's owner declassifies the
workflow boundary itself, in-file, exactly like any other sink —

```yaml
secrets:
  API_KEY:
    source: env
    key: EXAMPLE_API_KEY
    egress:
      - to: "nika:fetch"   # the send
      - to: "outputs"      # the return value derived from the response
```

`to: "outputs"` is sink-only (no `host:` — the workflow boundary has no
address) and as SPECIFIC as every other rule: it clears the `outputs:`
report for taints originating from THIS secret and nothing else — it
never authorizes a send, and a `nika:fetch` clearance never authorizes
the boundary. Absent the rule, the report stands (default-deny).

The general untrusted→decision integrity lattice lands surface by surface:
the permit-parameterization taint (untrusted values under a present
`permits:` block) is normative in [10](./10-authority.md) §the
permit-parameterization taint (NEP-0004); the remaining decision surfaces
stay a documented follow-up covered by the static guards above.

### `permits` · *optional · the declared capability boundary*

```yaml
permits:                          # the workflow's entire blast radius, declared in-file
  fs:   { read: ["./data/**"], write: ["./out/**"] }   # path globs (gitignore-style)
  net:  { http: ["api.example.com", "*.github.com"] }  # host allowlist — the SSRF boundary, in-file
  exec: false                     # may this workflow run shells? false | true | ["git", "cargo"] (program allowlist)
  tools: ["nika:read", "nika:write", "mcp:browser/*"]  # the builtin/MCP surface it may invoke
  env:  ["CI_COMMIT_SHA"]         # engine env names passed through to child processes · exact names · composed, never inherited
```

`permits:` makes the **file itself the security boundary**: an auditable
property of the workflow, not a runtime flag. The key stays **optional in
the grammar** (LAW-AUTH-0320 · no new field, no version marker); an absent
block is not a free pass: **an absent `permits:` block declares zero
authority** (NEP-0003 · LAW-AUTH-0324 · fail-closed). DeclaredPermits :=
∅, so every effect the body requires is refused at `check`
(`NIKA-AUTH-006` · before any token · the diagnostic carries the inferred
block inline, ready to paste), and any effect attempted at run time fails
the task (`NIKA-SEC-004` · defense in depth for the dynamic cases a static
judge cannot see). A workflow whose required set is empty (pure compute)
passes, with an informational hint naming the explicit form; `permits: {}`
is the authored spelling of "I touch nothing". Under composition the
absence is a wall, the zero wall: a parent's grants never flow down to a
child implicitly (`NIKA-COMP-002`).

**Semantics (normative) · once `permits:` is present, every category is
DEFAULT-DENY unless listed, and every bound in the block is a LITERAL —
interpolation never reaches this block (an interpolated host/glob/program
is a hard refusal, `NIKA-AUTH-007` · untrusted values under the block are
re-gated on their canonical resolved form, [10](./10-authority.md) §the
permit-parameterization taint)** ·

| Category | When listed | When the `permits:` block is present but this category is omitted |
|---|---|---|
| `fs.read` / `fs.write` | only the matching path globs are allowed | **no** filesystem read / write |
| `net.http` | only the listed hosts (**exact names only** · an entry carrying a `*` anywhere but as the whole bare-`*` entry is refused at check, `NIKA-AUTH-010` · the bare `*` stays the explicit, visible escape) · tightens the engine SSRF floor — the one loosening is the exact-loopback declassification below | **no** outbound network |
| `exec` | `false` = no shells · `true` = any (still blocklist-gated) · array = only those program names (argv `command[0]`) | treated as `false` · **no** `exec:` |
| `tools` | only the matching `nika:` / `mcp:` ids (globs ok) | **no** `invoke:` of any tool |
| `env` | only the named engine variables pass through to child processes (exact names · the runner env floor rides beneath) | **no** passthrough · a child sees the runner env floor plus the task `env:` map only |

**A program allowlist verifies the argv form only** (normative) · when
`exec:` is an array of program names, a task whose `command:` is a shell
STRING is refused under that allowlist — at check and at run. A
leading-token heuristic is unsound (`"git log; rm -rf /"` leads with `git`);
the array `command:` form is the shape an allowlist can actually verify.

**A path grant names an effective path identity, re-judged at dispatch**
(normative · NEP-0009) · an `fs.read` / `fs.write` entry is not a string
prefix the sandbox mounts blindly: at dispatch the engine resolves the
grant's literal prefix to its **effective form** (the longest existing
ancestor canonicalized, the not-yet-existing tail carried lexically) and
re-judges that identity against the declared set ·

1. **Escape = refusal, never rewrite** — a grant whose effective target
   escapes the declared set (a planted symlink pointing out of the judged
   tree) is REFUSED before spawn (`NIKA-SEC-004` class) and the refusal is
   attested in the run journal (`fs.path_mismatch`, carrying the judged
   prefix and the resolved target). The engine MUST NOT silently mount
   the resolved target under the judged name — the receipt never lies:
   judged = mounted.
2. **Platform parity** — the same verdict holds on every enforcement arm
   (the kernel path-walk arm and the mount-projection arm alike); a
   conformance pair asserts it, so `check ≡ run ≡ jail` descends to
   symlinks.
3. **A write grant whose target does not exist yet stays legal** — the
   identity is judged on the longest existing ancestor.
4. **The residual window is declared** — a parallel task of the same run
   swapping the symlink between the dispatch re-gate and the mount is a
   documented residual; the fd-pin projection (`--bind-fd`, bwrap ≥
   0.10.0) closes it and is a named follow-on, not v1.

**Exact-loopback declassification** (normative) · an **exact loopback
literal** in `net.http` — the bare `localhost` name, a `127.x.y.z` v4
literal, or the v6 loopback `::1` (the bracketed `[::1]` authority
spelling is accepted) — is the author's **declassification of the SSRF
floor for that host only** (the [secrets `egress:`](#egress--optional--sanctioned-destinations-declassification)
precedent: the owner's explicit act, co-located with the boundary). The
clearing is **exact-host** (never a prefix, subnet, or resolution: a
permitted `localhost` does not clear `127.0.0.1`, and a public DNS name
resolving to loopback stays refused) and **host-level** (ports do not
participate in permits). It **never** extends past loopback: a glob entry
(refused outright at check, `NIKA-AUTH-010`), the `*.localhost` family,
and RFC1918 / link-local / CGN / metadata targets stay floor-blocked even
when named — an exact entry naming one is an inert dead grant, flagged at
check under the floor parity (`NIKA-SEC-005`). DNS-rebinding stays covered: the engine
re-checks every resolved address and every redirect hop; a permitted
loopback name admits only its own loopback resolution, and a redirect
to any un-permitted floor host still refuses (`NIKA-SEC-005`). An
engine MUST NOT auto-write a loopback grant (e.g. from permits
inference) — the explicit act stays the author's.

**The sandboxed egress proxy is the permit's exact projection** (normative
· NEP-0008) · when a sandboxed `exec` child runs under a `net.http`
allowlist, the engine mediates every outbound connection through a
loopback egress proxy plus an OS fence that refuses every other
destination. Four laws hold ·

1. **DNS is resolved by the proxy, never by the client** — the OS fence
   refuses the confined client's own resolver (fail-closed), and the
   proxy re-checks **every resolved address** against the SSRF floor
   inside the dial loop (no resolve-then-connect window: a permitted
   name that resolves to a blocked address is refused at dial,
   `NIKA-SEC-005`).
2. **The port floor is normative** — the engine's dangerous-egress-port
   denylist rides beneath every permit; **no permit overrides it** (a
   `net.http` grant can never admit ssh/smtp/docker/kube/database
   ports).
3. **One run, one allowlist** — a run projecting two distinct `net.http`
   sets refuses fail-closed at the second projection; there is no
   ambient merge of egress boundaries inside one run.
4. **A floor-blocked entry is a dead grant** — an exact entry naming a
   private / link-local / CGN / metadata / otherwise non-public address
   can never take effect and is refused at check under the floor parity
   (`NIKA-SEC-005`); the only declassifiable target is the exact loopback
   literal of the paragraph above.

The proxy is **TLS-blind by design**: it observes the CONNECT authority
(host · port) and relayed byte counts only, never the content — and every
verdict (allowed · refused · bytes relayed) is journaled.

**The environment category** (normative · NEP-0005 · LAW-AUTH-0326) · a
child process environment (the `exec` subprocess · a stdio `mcp:*` tool
server) is **composed, never inherited**: the runner env floor ∪ the
declared `env:` passthrough (resolved from the engine's environment at
spawn) ∪ the task's explicit `env:` map (authored values · applied after
the passthrough, so an authored entry wins on the same name), minus the
dangerous-name floor. The **runner env floor** is the fixed list `PATH`,
`HOME`, `TMPDIR`, `LANG`, `LC_ALL`, `TZ`, `USER`, `LOGNAME` · a normative
MAXIMUM: an engine MUST NOT pass any undeclared name beyond it and MAY
pass fewer. An `env:` entry is an exact POSIX name
(`[A-Za-z_][A-Za-z0-9_]*` · no globs) and a permit BOUND: an interpolated
entry is `NIKA-AUTH-007` (a bound MUST be a literal · [10](./10-authority.md)).
The **dangerous-name floor** (dynamic-linker injection · shell startup
sourcing · tool command hooks · interpreter pre-exec hooks · `IFS` · the
engine's canonical `DANGEROUS_ENV_VARS` list) is never passable: an
`env:` entry naming one is an inert dead grant, flagged at check
(`NIKA-AUTH-009` · the SSRF dead-grant teaching applied to the env
plane), and a task-map entry naming one is stripped last. A declared name
absent from the engine environment passes nothing (no error · no
empty-string synthesis). Under composition ([14](./14-composition.md))
the effective category is the exact-name intersection child ∩ parent.
`env:` is **not inferable**: permit inference MUST NOT invent the list (a
subprocess's environment reads are opaque) · the undeclared-read failure
mode is the child tool's own missing-variable error, and the repair is
one declared line (`env: [NAME]`).

So `permits: {}` is a workflow provably limited to pure compute (`infer:` +
CEL + `nika:jq`): zero fs, zero net, zero shell, zero tools, zero env
passthrough (its children see the runner env floor plus their task `env:`
maps, nothing else). That property is checkable BEFORE the run.

**The engine MUST enforce `permits:` on BOTH surfaces** ·
1. **Statically** (`nika check`) · a `nika:write ./etc/x` outside `fs.write`,
   a `nika:fetch` to an unlisted host, an `exec:` under `exec: false`, an
   `invoke:` of an unlisted tool → a **lint error** (the run is refused
   before it starts). The SSRF floor is checked statically too: a literal
   URL — or a `net.http` entry — naming a target the floor always refuses
   (loopback · private · link-local/metadata · `NIKA-SEC-005` · 05-errors)
   is flagged at `check`, with or without a `permits:` block; outside the
   exact-loopback declassification above, no grant can admit such a
   target, so blessing it would be a false green (the run could never
   succeed). A DECLASSIFIED loopback host is the mirrored truth: the
   check stops flagging exactly where the run stops refusing (the entry
   is live, not a dead grant), and the clearing is stated informationally
   rather than silently un-flagged.
2. **At runtime** · any effect escaping the declared set fails the task
   `NIKA-SEC-004` (`security_error` · never fed back to an `agent:` model:
   a capability boundary is not negotiation material). This catches the
   dynamic cases a static check cannot (a host computed at run time —
   including one that resolves to a floor-blocked address, `NIKA-SEC-005`).

`permits.net.http` and the agent `tools:` whitelist compose: the agent
whitelist scopes ONE task's tools; `permits.tools` scopes the WHOLE workflow
(the union ceiling). An agent may never be granted a tool outside `permits`.

### `run` · *optional · the run's entropy + clock declaration*

```yaml
run:
  entropy: ambient          # none | ambient | { seeded: <u64> }
  clock: system             # system | virtual
```

Every source of randomness and time a run consumes is **declared, never
ambient** (normative · NEP-0010). The block is optional; absent, the run
behaves exactly as `entropy: ambient` + `clock: system` (the status quo).
The key set is closed (`{entropy, clock}` only — a typo'd declaration is
refused in both parse modes, like `permits:`).

- **`entropy: { seeded: N }`** forces the deterministic seams and pins the
  run's seed: two runs of the same file with the same `N` produce
  **byte-identical journals**. `entropy: none` demands strict determinism
  (any structural randomness is a finding). `entropy: ambient` is the
  honest status quo — legal, and named as what it is.
- **`clock: virtual`** substitutes the engine's injected virtual clock for
  wall time: a `timeout:` budget reads against the declared clock, and
  time becomes a contract input.
- **The dimensions couple** (normative): byte-identical journals require
  deterministic TIME as much as deterministic randomness, so the only
  legal pairs are `ambient × system` (the status quo) and
  `none | seeded × virtual` (the deterministic states). A declared
  contradiction (`ambient × virtual` · `none | seeded × system`) is
  refused at parse (`NIKA-PARSE-026` · `NIKA-PARSE-027`). The
  contradiction is judged on the DECLARED pair: `clock: virtual` alone
  (entropy left implicit) stays legal — a test configuration that makes
  no determinism claim. And `entropy: none` with a structural randomness
  source consumed (a live retry jitter · `nika:uuid`) is refused at
  check (`NIKA-PARSE-028`).
- **One run, one clock**: the declaration lives at the envelope, never
  per task — a run has exactly one entropy source and one clock, so the
  composition stays auditable.

### `tasks` · **required · a non-empty MAP · the key IS the identity**

```yaml
tasks:
  task_1:
    ...
  task_2:
    ...
```

The DAG. Each key names one task (snake_case · `^[a-z][a-z0-9_]*$` ·
CEL-safe); duplicate keys are rejected loudly at parse. Source order is
**presentation only** — the graph alone determines scheduling. See
[03-dag.md](./03-dag.md) for the task model.

**Dead forms (rejected with a migration teaching · W1 « the map »)** ·
a `tasks:` SEQUENCE is `NIKA-PARSE-022`; an `id:` field inside a task is
`NIKA-PARSE-023` — the map key replaced both. No alias survives.

### `outputs` · *optional · the workflow's return value · untyped OR typed*

```yaml
outputs:
  # Untyped form — just a reference to a task output
  summary: ${{ tasks.synthesize.output }}

  # Typed form — declares the return shape · powers the callable-workflow output schema
  report:
    value: ${{ tasks.write_report.output }}
    type: string                # the full TypeExpr of 09-types.md (R3b · one type language on both halves)
    description: "The final markdown brief"
```

`outputs:` declares **what the workflow returns**, the symmetric twin of
`inputs:` (what it takes in). Each entry is a name bound to a
`${{ tasks.<id>.output }}` reference (or any `${{ ... }}` expression), in the
**untyped form** (bare reference) or the **typed form**
(`{ value, type, description }`).

This single block serves three consumers ·

- **`nika run`**: prints this object as the workflow result (without `outputs:`,
  the CLI result is engine-defined and implicit).
- **`nika.run_workflow` over MCP**: a caller (agent host · parent workflow)
  receives exactly this shape. Together with typed `inputs:` it forms the
  **complete callable contract** · typed in, typed out.
- **Schema generation**: typed outputs generate the *output half* of the
  callable schema (typed `inputs:` generate the input half).

If `outputs:` is omitted, the workflow still runs; its result is
engine-defined (a reusable/callable workflow SHOULD declare `outputs:`). The
referenced task ids must exist (parse-time validated).

> **`outputs:` (envelope · plural) ≠ `output:` (task · singular).** The
> workflow-level `outputs:` is the *return contract*; the task-level `output:`
> ([04-variables.md](./04-variables.md#output-binding--output)) defines *named
> jq bindings* on one task. Plural-at-the-top, singular-per-task: the
> same split GitHub Actions uses for `workflow_call.outputs` vs step `outputs`.

### What leaves a run · the export contract (normative)

Exactly **three** things cross the run boundary · nothing else is promised ·

1. **The `outputs:` object**: when declared, a conformant engine's run
   command prints it as a **single JSON object on stdout** (and returns the
   same shape over MCP). Diagnostics · progress · logs go to **stderr** ·
   never interleaved into the stdout JSON.
2. **The exit code**: maps 1:1 to the workflow final state
   ([05 §workflow-level semantics](./05-errors.md#workflow-level-error-semantics)) ·
   `success` → **0** · `failure` → **non-zero** · `cancelled` → **non-zero**
   (distinct from failure's code · engine-documented). Parse/validation
   rejection (the run never started) is also non-zero.
3. **Tool side effects**: whatever the workflow's tools wrote (files ·
   notifications · MCP calls). The language does not track these; they are
   the workflow's business.

A schema'd **machine run report** (per-task statuses · timings · costs ·
ProofChain-compatible provenance) is deferred: see
[08 §Horizon postures H10](./08-out-of-scope.md#horizon-postures--the--did-you-think-of-x--table-2026-06-10) ·
until then `nika:inspect` exposes run introspection *inside* the run, and
the completion event (05) is the engine-side hook.

---

## YAML conventions · no traps

A Nika file is **YAML 1.2** (which is a strict superset of JSON: every Nika
workflow can also be written as JSON). YAML 1.2 is mandated specifically to
avoid the classic YAML 1.1 footguns that bite generated configs:

| Trap (YAML 1.1) | What happens | The rule |
|---|---|---|
| `region: no` | parsed as boolean `false` (the « Norway problem ») | YAML 1.2 keeps it a string · still, **quote** bare words that look boolean (`no` · `yes` · `on` · `off`) |
| `id: 0755` | parsed as octal `493` | **quote** numbers with leading zeros · `"0755"` |
| `at: 12:30` | parsed as sexagesimal `750` | **quote** colon-bearing scalars |
| `v: 1.10` | parsed as float `1.1` (trailing zero lost) | **quote** version-like strings · `"1.10"` |

**One rule that removes all of them** · when a scalar *could* be misread as a
number, boolean, or date, **quote it**. When in doubt, quote.

**Expressions** · a bare `${{ … }}` reference is a safe plain scalar
(`prompt: ${{ inputs.topic }}` is fine). But **quote** any expression that
contains `:` `#` `[` `{` `,` or `>` so YAML does not misparse it ·

```yaml
when: "${{ tasks.x.status == 'ok' && tasks.y.count > 3 }}"   # quoted · contains > and :
prompt: ${{ inputs.topic }}                                  # bare ok · no special chars
```

A conformant engine parses YAML 1.2. Authoring tools (and the AI writing
these files) should quote-by-default for the four ambiguous-scalar cases above.

---

## What the envelope is NOT

- It is NOT a place to inline credentials. Use `secrets:` with a `source` reference.
- It is NOT a place for engine runtime config (global timeouts · concurrency limits). Those live in engine config files, out of scope of the spec.
- It is NOT a place for imports / includes. v1 is single-file workflows. (Static composition is a candidate for a later additive minor: see [08-out-of-scope.md](./08-out-of-scope.md).)

---

## File naming (normative)

- **Canonical filename** · `<name>.nika.yaml`. Every tool that CREATES a
  workflow file (`nika new` · `nika init` · scaffolds · templates) MUST
  emit this form, and every teaching surface writes it.
- **`.nika.yml`** · accepted by matchers (editors · schema catalogs ·
  hooks) so no real file is ever orphaned — and taught against: a tool
  that notices it SHOULD flag « non-canonical filename · rename to
  `.nika.yaml` » (a dedicated profile diagnostic may be allocated by a
  future law; the convention is normative today). Tools MUST NOT emit it.
- **Bare `.nika`** · RESERVED. Never emitted, never claimed by tooling.
- **Media type** · `application/vnd.nika+yaml` is the reserved media type
  for workflow documents (vendor-tree registration per RFC 6838 is a
  post-1.0 gesture). Do not invent alternatives.

One suffix, one grammar: a split-suffix ecosystem (`.yml` and `.yaml`
both canonical) fragments globs, schema catalogs and CI matchers
forever — that lesson is upstream, and this door closes pre-1.0.

---

## Examples

### Minimal

```yaml
nika: v1
workflow:
  id: hello

tasks:
  greet:
    infer:
      prompt: "Hello"
      model: anthropic/claude-haiku-4-5
```

### Full · with typed inputs (callable over MCP)

```yaml
nika: v1
workflow:
  id: research-pipeline
  description: "Research a topic and write a markdown brief"

model: anthropic/claude-sonnet-4-6
inputs:
  topic:
    type: string
    required: true
    description: "Subject to research"
  output_path:
    type: string
    default: "./brief.md"

tasks:
  research:
    infer:
      prompt: "Research the topic · ${{ inputs.topic }} · in 5 paragraphs"

  write:
    with:
      content: ${{ tasks.research.output }}
    invoke:
      tool: "nika:write"
      args:
        path: "${{ inputs.output_path }}"
        content: "${{ with.content }}"
```

---

## Conformance

A v0.1-compliant engine MUST ·

1. Reject any workflow missing `nika:` or `workflow:` with a clear error
2. Accept exactly `nika: v1` · reject any other value (`v1.0` · `1` · `v2` …) with a clear error
3. Validate `workflow` identifier kebab-case format
4. Make workflow-level `model`, `inputs`, `config`, `const`, `secrets` available to all tasks as defaults
5. Validate typed `inputs` (type + required) before execution · reject missing required inputs · refuse every declared `default:` / typed `const:` value that does not conform to its declared `type:` (`NIKA-DEFAULT-001`)
6. Validate each typed `outputs` value against its declared `type:` at run end · a value that does not match its declared type fails the run (`NIKA-VAR-009` · `validation_error`): the callable contract is enforced on BOTH halves (typed in via `inputs`, typed out via `outputs`) · symmetric with rule 5
7. Mask resolved `secrets` values in all logs · traces · journal events
8. Enforce a declared `permits:` block on both surfaces: refuse statically-detectable escapes at check time, and fail any runtime effect outside the boundary with `NIKA-SEC-004` · once `permits:` is present every category is default-deny unless listed
9. Compose every child process environment (§permits env · NEP-0005): the runner env floor ∪ the declared `env:` passthrough ∪ the task `env:` map, minus the dangerous-name floor · never inherit the engine environment

---

🦋 *Next · [02 · The 4 verbs](./02-verbs.md)*

---

### Multi-line strings · canonical `|`

For multi-line `prompt:` · `system:` · `command:` · or any free-form text
field · use the **literal block** indicator `|` (keeps newlines verbatim) ·

```yaml
prompt: |
  Line 1 keeps its newline
  Line 2 keeps its newline
  Line 3 ends with a trailing newline
```

**YAML multi-line forms · ranked for Nika** ·

| Form | Newlines | Trailing newline | Verdict |
|---|---|---|---|
| `|` | preserved | preserved | **✅ canonical** · use for prompts · system · command · long strings |
| `|-` | preserved | STRIPPED | ✅ alternative · for compact prompts without trailing newline |
| `>` | folded to SPACES | preserved | **❌ forbidden in prompts** · whitespace-collapses · corrupts LLM intent |
| `>-` | folded to SPACES | STRIPPED | **❌ forbidden in prompts** · same whitespace issue |

**Why forbid `>` and `>-` in prompts** · they collapse newlines into
spaces · which often changes LLM behavior (intended paragraphs become
one long line). Engines MAY warn or reject `>` / `>-` for prompt/system/
command fields at parse time.

**Single-line strings** · prefer **unquoted** when no special chars · use
**double-quoted** `"..."` when escaping is needed (`\n` · `\t` · etc.) ·
use **single-quoted** `'...'` for literal strings with quotes.

```yaml
# Unquoted (preferred)
prompt: Say hello in French.

# Double-quoted (when escaping needed)
prompt: "Line 1\nLine 2 (escaped newline)"

# Single-quoted (literal · no escapes)
prompt: 'He said "hi"'

# Multi-line literal (most common · use this for any prompt > 1 line)
prompt: |
  You are a helpful assistant.
  Answer the user's question in 3 sentences.
```

