# Language

Read this reference for the matching task; return to [the skill](../SKILL.md) for scope and completion.

## The envelope: three value authorities, one boundary

Every value a workflow depends on is DECLARED, and the family is closed:

| Authority | What it holds |
|---|---|
| `inputs:` | typed parameters a caller supplies (`--var key=value`), and typed configuration a deployment supplies — the latter carries `required: false` and a `default:` |
| `const:` | fixed values baked into the file |
| `secrets:` | governed store references (`source: env` + `key:`) |

`vars:` and `env:` are dead envelope fields (`NIKA-VALUES-001` ·
`NIKA-VALUES-002`); any other namespace is `NIKA-VALUES-003`. `config:`
was a fourth authority and is not one now — it is not a field at all,
so it refuses `NIKA-PARSE-005` rather than teaching a migration.
Classify by ROLE, never bulk-rename: a caller's parameter is an
`inputs:` entry, a baked value is a `const:`, a credential is a
`secrets:` entry, and a name a child process must SEE is
`permits: { env: [NAME] }`. `inputs:` resolves ONLY against the
declared block — the engine never falls back to the OS environment, so
every value the file depends on is visible in the file. `nika check
--fix` migrates the `vars:` half mechanically; `env:` has no mechanical
repair, because that classification is yours.

**`permits:` is the boundary, and ABSENT MEANS ZERO AUTHORITY:** any
effect under no block refuses `NIKA-AUTH-006` at check, before a token
is spent. A pure-compute body states the zero explicitly as
`permits: {}`. `nika check --infer-permits <file>` prints the tightest
block — paste it in, and from then on the boundary is default-deny: a
new host, path or tool must be added consciously, in a reviewable diff.
A permit bound is always a literal, never an interpolation
(`NIKA-AUTH-007`), and `*.example.com` is refused — a subdomain
wildcard hands the boundary to the zone operator; name exact hosts
(`NIKA-AUTH-010`).

**Static coverage and runtime boundaries are different claims.**

- **Constant interpolation is not a blanket static blind spot.** The
  checker resolves statically known values: a fetch through
  `${{ const.api_url }}` with no permits is refused before execution.
  Genuinely runtime-dependent values still require runtime re-gating.
  Read `permits.notes` and `permits.partial` in the report; a deferred
  path or host is not proof of zero effects.
- **A host permit does not cover the host it redirects to.**
  `net: { http: ["www.rust-lang.org"] }` is green at check and refused
  at run: `NIKA-SEC-004 · rust-lang.org resolves outside the declared
  net.http boundary`. The redirect target is not knowable statically.
  Declare both hosts, or point at the final one.
- **An unresolved path needs a reviewed finite boundary.** Inferred
  permits and their review notes are a starting point, not permission
  to invent an unrestricted path. Declare only the directories/files
  the job needs, and test both an allowed value and an out-of-bound
  value in scratch. Do not widen the boundary merely to silence a refusal.
- **The axes are conjunctive, and `fs` bounds take globs while hosts do
  not.** `tools: ["nika:write"]` with no `fs.write` authorizes nothing:
  the write refuses on the `fs` axis at check and at run. You need the
  tool AND the path. `fs` bounds may be globs (`./docs/**/*.md` is
  accepted); host bounds may not (`*.example.com` is `NIKA-AUTH-010`).
  That asymmetry is deliberate and it is not guessable, so do not
  reason from one to the other.

A spawned child inherits NOTHING from the engine: its environment is
composed from a cleared slate — the runner floor ∪ the names declared
in `permits: { env: [NAME] }` ∪ the task's own `env:` map. A variable
the child needs must be named.

## The whole surface (nothing else exists)

Nine envelope keys, one verb per task, and a fixed set of modifiers.
`nika spec --schema` is the machine truth; this is the map.

**Envelope** · `nika` · `model` · `inputs` · `const` · `secrets` ·
`permits` · `run` · `tasks` · `outputs`. Nothing else parses: a key
outside this nine refuses `NIKA-PARSE-005`. `workflow:`, `types:`,
`config:`, `policy:` and `assert:` were envelope keys and are not one
now — `workflow:` survives only INSIDE `invoke:`, and a
deployment-supplied value is an `inputs:` entry with `required: false`
and a `default:`.

**Task modifiers**, beside the one verb:

| Field | What it does |
|---|---|
| `with:` | the DATA edge — bind another task's output, body reads `${{ with.alias }}` |
| `after:` | the CONTROL edge — `success` · `failure` · `skipped` · `terminal` · `unwind` |
| `when:` | a CEL boolean gate · closed callables: `size()` · `has()` · `.size()` · `.contains()` · `.startsWith()` · `.endsWith()` |
| `for_each:` | fan out over a collection · a BLOCK, never a scalar — `items:` carries the collection and is required, `max_parallel:` caps concurrency (1 = sequential), `fail_fast:` aborts on the first error (default true), and all three live INSIDE the block · a bare `for_each: <expr>` refuses `NIKA-PARSE-019`, and `max_parallel:`/`fail_fast:` at task level are retired spellings · `items:` reads a prior task through `with:` like every other reference · the body reads the current element as `${{ item }}` and its position as `${{ index }}` (loop-scoped locals, NOT a fourth value authority · `item.field` reaches into an object element) · the task's `.output` is the ARRAY of per-iteration outputs, in input order |
| `retry:` | `max_attempts` · `backoff_ms` · `backoff_strategy` · `backoff_max_ms` · `jitter` · `on_codes` — transient failures only; a wrong prompt never heals by retry |
| `on_error:` | exactly ONE action — `recover:` · `skip:` (preserves the original error at `tasks.X.error`) — with an optional `on_codes:` filter · the default (no `on_error:`) IS failure, and there is no keyword for saying so (`fail_workflow:` is dead · a YAML comment says it) |
| `extract:` | named jq bindings → `${{ tasks.X.<name> }}` |
| `returns:` | the task's output contract — exclusive with a verb-level `schema:` (`NIKA-TYPE-003`) |
| `timeout:` | a quoted Go duration |
| `lift:` | the ONE authored door, a list · each entry opens exactly one named law with a non-empty `because:` (check-visible · receipt-recorded) · `{law: taint, from: <binding>, because: "…"}` raises ONE binding through the permit-parameterization taint — never a permit bypass, the value is still matched against the declared boundary · `{law: data-as-code, because: "…"}` declares a `nika:fetch` payload code-bearing but never loaded — lifts that sink law ONLY, never the net boundary (`from:` is forbidden here) · a lift that would not have fired refuses `NIKA-AUTH-011` · `declassify:` and `inert:` are dead spellings of the same door |

One shape a table cannot carry, because its whole defect is nesting:

```yaml
nika: fan-out-shape
const:
  targets: ["alpha", "beta"]
permits:
  tools: [nika:log]
tasks:
  each:
    for_each:
      items: ${{ const.targets }}   # REQUIRED · the collection
      max_parallel: 4               # inside the block · at task level it is refused
      fail_fast: true               # inside the block · this is the default
    invoke:
      tool: nika:log
      args:
        message: "${{ item }} at ${{ index }}"
outputs:
  lines: .each
```

Checked and RUN against the shipped binary before it was written here ·
`clean` · `compiled` · `paid_ready` · zero hints · prints `alpha at 0`
then `beta at 1`. An example that only checks is half an example.

## The four verbs (exactly one per task)

- `infer:` — an LLM call (`prompt`, `schema?` for typed output,
  `max_tokens?`)
- `exec:` — a subprocess · `command:` is argv (`["git", "status"]` —
  one token per element, run via execve, so an interpolated value can
  never break out) · no implicit shell: pipes, redirects and globs go
  in `shell:` explicitly · `capture: stdout|stderr|combined|structured`
  · **last resort**: run the [native-first guidance](native-and-artifacts.md)
- `invoke:` — a tagged union carrying EXACTLY ONE of `tool:` or
  `workflow:`, plus `args:`. `tool:` reaches a builtin or an MCP tool
  (HTTP fetch is `tool: "nika:fetch"`, a tool, not a verb);
  `workflow:` calls a whole other workflow (below). Both, or neither,
  is a parse error — two targets is two meanings
- `agent:` — a bounded multi-turn loop (`prompt`, `tools` allowlist,
  `max_turns`, `max_tokens_total`)
