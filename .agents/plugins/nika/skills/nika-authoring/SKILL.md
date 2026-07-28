---
name: nika-authoring
description: Author, check and repair Nika workflows (.nika.yaml files — the workflow language for AI). Use when writing or editing a *.nika.yaml file, converting a repeated AI task or prompt chain into a workflow, or when nika check reports NIKA-XXXX findings to fix.
---

# Authoring Nika workflows

Nika turns repeatable AI work into files: one `.nika.yaml`, four verbs,
audited **before** it runs. You author the file; `nika check` is the
oracle; the human runs it.

## The loop (always)

1. **Start from a template or example**, never from scratch:
   `nika examples list` · `nika examples show <slug>` ·
   `nika new --from <template> <file>.nika.yaml`
2. **Write the file.** The envelope is `nika: v1` + a `workflow:` OBJECT
   (`id:` kebab-case · optional `description:`) + a `tasks:` MAP keyed
   by task id — the key IS the identity, never a `- id:` sequence. Pick
   models and builtins from the embedded catalogs — `nika catalog`
   (providers · models · capabilities · which env var each needs) and
   `nika catalog --tools` (the `nika:*` builtins an `invoke` reaches
   without MCP); before a run, `nika inspect <file>` shows the anatomy:
   tasks · waves · the cost floor.
3. **Check it**: `nika check <file>` (exit 0 = clean · 2 = findings),
   then `nika check --native-strict <file>` — it fails on any
   `native-first` hint (an `exec:` a builtin covers).
4. **Repair**: `nika check <file> --fix` applies the machine-applicable
   repairs first (typo'd fields · tools · args · `after:` targets ·
   `${{ }}` references — typed did-you-mean only, ambiguity is skipped
   with a note, never guessed) and re-audits; repair what remains from
   the diagnostics — they name the exact task, reference and fix.
   Unknown code? `nika explain NIKA-XXXX`.
5. Repeat 3–4 until clean. **Never hand a file to the human that does
   not pass `nika check`** — and pass `--native-strict` too, unless
   every remaining `exec:` is in the exec ledger (below).
6. The human (or CI) runs it: `nika run <file>`. Preview offline with
   `--model mock/echo`; run locally with `--model ollama/<model>` —
   or fully in-binary: `nika model pull <owner/repo-GGUF>` then
   `nika model serve --model <id>` (qwen3-family GGUFs today; the
   serve banner prints the exact env + `model:` line workflows use).
   Inputs ride `--var key=value` (repeatable · the flag names an
   `inputs:` declaration · unknown keys refused); a run paused on a
   `nika:prompt` resumes with
   `nika run <file> --resume <trace> --answer <task>=<value>`
   (confirm gates take booleans: `--answer approve=true`).
7. Pin it for CI: `nika test <file> --update` writes
   `<file>.golden.json` from an offline mock run; `nika test <file>`
   replays and compares — deterministic, zero keys.
8. **Prove a run that mattered**: every run writes a hash-chained
   journal to `.nika/traces/`. `nika trace verify <trace>` climbs a
   four-tier ladder and reports the highest tier honestly attained —
   chain OK · **SEALED** (the run signature verifies against a custody
   key) · **ANCHORED** (the detached transparency-log sidecar verifies
   fully offline) · **REPLAYED** (`--replay` compares a fresh run;
   verify never re-executes). `nika trace show <trace>` reads the card;
   `nika evidence <trace>` exports the pack an auditor reads without
   trusting you. Cite the trace, never a memory of the run.

## The envelope: four value authorities, one boundary

Every value a workflow depends on is DECLARED, and the family is closed:

| Authority | What it holds |
|---|---|
| `inputs:` | typed parameters a caller supplies (`--var key=value`) |
| `config:` | typed configuration a deployment supplies |
| `const:` | fixed values baked into the file |
| `secrets:` | governed store references (`source: env` + `key:`) |

`vars:` and `env:` are dead envelope fields (`NIKA-VALUES-001` ·
`NIKA-VALUES-002`); any other namespace is `NIKA-VALUES-003`. Classify
by ROLE, never bulk-rename: a caller's parameter is an `inputs:` entry,
a baked value is a `const:`, a credential is a `secrets:` entry, and a
name a child process must SEE is `permits: { env: [NAME] }`. `config:`
resolves ONLY against the declared block — the engine never falls back
to the OS environment, so every value the file depends on is visible in
the file. `nika check --fix` migrates the `vars:` half mechanically;
`env:` has no mechanical repair, because that classification is yours.

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

A spawned child inherits NOTHING from the engine: its environment is
composed from a cleared slate — the runner floor ∪ the names declared
in `permits: { env: [NAME] }` ∪ the task's own `env:` map. A variable
the child needs must be named.

## The whole surface (nothing else exists)

Thirteen envelope keys, one verb per task, and a fixed set of modifiers.
`nika spec --schema` is the machine truth; this is the map.

**Envelope** · `nika` · `workflow` · `model` · `types` · `inputs` ·
`config` · `const` · `secrets` · `permits` · `run` · `policy` ·
`tasks` · `outputs`. Two are easy to miss:

- `types:` — named type declarations (PascalCase · acyclic), so a shape
  is declared once and referenced everywhere.
- `policy:` — named workflow law. The HARD families (`require` ·
  `forbid` · `allow` · `limits`) are judged at check
  (`NIKA-POLICY-001`); the SOFT families (`prefer` · `optimize`) are
  recorded and never judged.

**Task modifiers**, beside the one verb:

| Field | What it does |
|---|---|
| `with:` | the DATA edge — bind another task's output, body reads `${{ with.alias }}` |
| `after:` | the CONTROL edge — `success` · `failure` · `skipped` · `terminal` |
| `when:` | a CEL boolean gate (`size()` is the only function) |
| `for_each:` | fan out over a collection · `max_parallel:` caps concurrency (1 = sequential) · `fail_fast:` aborts on the first error (default true) |
| `retry:` | `max_attempts` · `backoff_ms` · `backoff_strategy` · `backoff_max_ms` · `jitter` · `on_codes` — transient failures only; a wrong prompt never heals by retry |
| `on_error:` | exactly ONE action — `recover:` · `skip:` (preserves the original error at `tasks.X.error`) · `fail_workflow:` — with an optional `on_codes:` filter |
| `on_finally:` | cleanup mini-tasks that ALWAYS run (success · failure · timeout · cancel) · sequential · best-effort |
| `output:` | named jq bindings → `${{ tasks.X.<name> }}` |
| `returns:` | the task's output contract — exclusive with a verb-level `schema:` (`NIKA-TYPE-003`) |
| `timeout:` | a quoted Go duration |
| `inert:` | declares a `nika:fetch` payload code-bearing but never loaded — the non-empty string IS the justification. Lifts the data-as-code sink law ONLY, never the net boundary |
| `declassify:` | the one door through the permit-parameterization taint · raises ONE binding from untrusted to trusted, check-visible and receipt-recorded. Never a permit bypass — the value is still matched against the declared boundary |

## The one way (take the default, and the checker goes quiet)

Every authoring decision has a default. Take it unless the job forces
otherwise, in this order:

1. **Shape before content.** Route to a template or an example; never
   start from a blank file. The outer shape decides the task graph
   before a single prompt is written.
2. **One job, one task, one verb.** If a task needs an "and then", it is
   two tasks. The verb IS the key.
3. **Pick the verb by execution model, not convenience.** `invoke:` when
   something callable already does it · `infer:` when a model must
   produce judgement or language · `agent:` when the number of steps
   cannot be known in advance and must be bounded · `exec:` only when
   the first three genuinely cannot.
4. **Classify every value before writing it.** Caller-supplied →
   `inputs:` · deployment-supplied → `config:` · fixed here → `const:` ·
   credential → `secrets:`. If you cannot name the class, you do not yet
   know what the value is.
5. **Bind, never reach.** A task needing another's output binds it in
   `with:`. Reaching for `tasks.*` anywhere else is `NIKA-VAR-021`.
6. **Order only when no data flows.** `after:` is pure sequencing; if
   data flows the `with:` binding already IS the edge. Never both.
7. **Bound the spend where it is spent.** Every `infer:` carries
   `max_tokens`; every `agent:` carries `max_turns` and
   `max_tokens_total`. A ceiling the checker can compute beats a cap
   someone has to remember to pass.
8. **Declare the boundary LAST, from the body.** Write the tasks, then
   `nika check --infer-permits` and paste. A boundary derived from the
   body is tight; one written from intent is wishful.
9. **Fail on purpose.** Transient failure → `retry:` · expected absence
   → `on_error: on_codes + recover:` · cleanup that must always happen →
   `on_finally:`. Swallowing an error is never the plan.
10. **Prove it before handing it over.** `nika check` clean, then
    `--native-strict`, then a golden pin if the workflow is hermetic.
    Only then does the human get the run line.

## Cost honesty (never hide unknown spend)

- `nika check` prints the cost ceiling BEFORE any token: `≤ $X` is a
  ceiling · `≥ $X FLOOR` means at least one task is unbounded — name
  the reason (a missing `max_tokens`, an uncataloged model, an
  expression fan-out), never round it to $0.
- A local model (`ollama/…`) is **unpriced compute, not « free »** —
  say "unpriced", never "$0" or "free".
- A spend cap rides the run: `nika run <file> --max-cost-usd <n>`
  blocks BEFORE the call that would cross the cap.
- `nika explain <file>` narrates all of this (waves · cost · touches ·
  how to run) — use it before handing a workflow to a human.

## The four verbs (exactly one per task)

- `infer:` — an LLM call (`prompt`, `schema?` for typed output,
  `max_tokens?`)
- `exec:` — a subprocess · `command:` is argv (`["git", "status"]` —
  one token per element, run via execve, so an interpolated value can
  never break out) · no implicit shell: pipes, redirects and globs go
  in `shell:` explicitly · `capture: stdout|stderr|combined|structured`
  · **last resort**: run the native-first interrogation first (below)
- `invoke:` — a tagged union carrying EXACTLY ONE of `tool:` or
  `workflow:`, plus `args:`. `tool:` reaches a builtin or an MCP tool
  (HTTP fetch is `tool: "nika:fetch"`, a tool, not a verb);
  `workflow:` calls a whole other workflow (below). Both, or neither,
  is a parse error — two targets is two meanings
- `agent:` — a bounded multi-turn loop (`prompt`, `tools` allowlist,
  `max_turns`, `max_tokens_total`)

## Composition (a workflow is callable)

A job too big for one file becomes a parent that calls children. The
child is a normal workflow; the parent reaches it through the verb it
already knows:

```yaml
tasks:
  audit:
    invoke:
      workflow: "./audits/site-audit.nika.yaml"
      args:
        url: "${{ inputs.target }}"
    returns: AuditReport
```

Two laws make the call graph drawable before anything runs:

- The target is STATIC — a literal path, or a pinned
  `registry:owner/name@version`. A `${{ }}`-templated target refuses
  `NIKA-COMP-001`: a call graph you cannot draw before the run is a
  call graph you cannot bound.
- `invoke:` carries exactly one target. `tool:` and `workflow:`
  together is the same refusal class as two verbs on one task.

The child's typed `outputs:` compose through `returns:` — declared
once in the child, never re-declared in the parent. Effects, budgets
and the permits boundary are inherited by the child, so a parent
cannot widen what its children may touch by calling them.

Reach for composition when a workflow has two audiences (a reusable
audit any project can call) or when one file stops fitting in a
reviewer's head. Do NOT reach for it to avoid writing a task.

## Native-first (the law)

The order is `invoke: nika:*` → `invoke: mcp:<server>/<tool>` →
`exec:`. Before writing ANY `exec:`, answer in your head:

1. **Which builtin replaces it?** The embedded set spans SIX families.
   Assume one exists before assuming it does not — most `exec:` lines
   written by agents are a builtin the author never looked for.

   | Family | Every builtin in it |
   |---|---|
   | CORE | `nika:log` · `nika:emit` · `nika:assert` · `nika:prompt` · `nika:done` · `nika:wait` |
   | FILE | `nika:read` · `nika:write` · `nika:edit` · `nika:glob` · `nika:grep` |
   | DATA | `nika:jq` · `nika:json_diff` · `nika:json_merge_patch` · `nika:validate` · `nika:convert` · `nika:uuid` · `nika:date` · `nika:hash` · `nika:decide` |
   | NETWORK | `nika:fetch` · `nika:notify` |
   | INTROSPECTION | `nika:compose` · `nika:inspect` |
   | MEDIA | `nika:chart` · `nika:image_generate` · `nika:image_fx` · `nika:tts_generate` |

   The NAMES above are canon — that is the whole set. The argument
   CONTRACTS are not: read them from `nika catalog --tools`
   (`--json` for the model-facing JSON Schemas) before calling one,
   and never guess an arg name.

   The reflexes worth memorising: HTTP (curl/wget/helper fetch) →
   `nika:fetch` · file plumbing (cat/tee/cp/mkdir) →
   `nika:read`/`nika:write` (`create_dirs: true`) · JSON shaping
   (jq/sed) → `nika:jq` or an `output:` binding · in-place edits →
   `nika:edit` · finding files (`find`/`ls`) → `nika:glob` · searching
   them (`grep`/`rg`) → `nika:grep` · `date`/`uuidgen`/`shasum` →
   `nika:date`/`nika:uuid`/`nika:hash` · format conversion →
   `nika:convert` · schema checks → `nika:validate` · image styling
   (ImageMagick / PIL / dither scripts) → `nika:image_fx`
   (deterministic — same input+args = same bytes, the artifact sha256
   joins the trace chain).
2. **Which MCP tool replaces it?** A product API deserves an MCP
   server, never a helper script.
3. **Neither?** Name the exact gap — then `exec:` is legitimate
   (build tools · git · a product CLI with no MCP surface yet) and
   goes in the ledger.

Never write a helper script (`node bin/helper.mjs …`, `python3
bin/thing.py …`) that wraps HTTP/files/JSON — that is
`native-first/005`, the exact failure class this law exists for.

### When the boundary pushes back (the reason glue gets written)

Two refusals send authors reaching for a scripting language. Neither
one wants a script; both have a native recipe.

**`NIKA-SEC-004` — a tainted value cannot ride `exec` argv.** An
`inputs:`-supplied or fetched value on a command line is a shell-shaped
injection surface, so the boundary refuses it. The move is NOT a reader
script. **Stage the value to a file, and pass the PATH as argv:**

1. shape it with `nika:jq` if it needs a form,
2. land it with `nika:write` (a `const:` path),
3. call the CLI with that path — `exec: { command: [cli, render, "./tmp/job.yaml"] }`.

Every argv element is now a literal the boundary can verify, and the
tainted value never touches a command line. Most CLIs already have this
door: a template, a config file, a `@file` argument. Prefer it over any
flag that takes the value inline.

**`NIKA-SEC-009` — the trifecta.** Untrusted input, private data and an
egress in one task is refused as a shape, not as an accident. The move
is to keep the trifecta INCOMPLETE rather than to smuggle a leg through
a subprocess: take the fetched value as `nika:fetch` metadata or text
and do NOT add an `fs.read` of local content in the same flow.

If a genuine gap survives both recipes, `exec:` is legitimate — name the
exact missing capability in the ledger. A helper that exists to dodge a
refusal is the refusal winning.

## Exec ledger (mandatory when any exec remains)

Every surviving `exec:` gets a row in the workflow's header comment:

```
# EXEC LEDGER ·
# | task | command | why no native path | unlock that removes it |
```

`--native-strict` + a complete ledger = a reviewable workflow.

## Discipline

- References: `${{ inputs.x }}` · `${{ const.x }}` ·
  `${{ config.KEY }}` · `${{ secrets.X }}` · `${{ tasks.<id>.output }}`
  · `${{ with.alias }}` (never inline a credential).
- Quote any scalar that STARTS with `${{` inside a FLOW mapping —
  `with: { body: "${{ tasks.a.output }}" }` — or YAML reads the `{{`
  as a nested map (`NIKA-PARSE-001`). In block style the quotes are
  optional.
- In `outputs:` bind `${{ tasks.<id>.output }}` — never the bare
  `${{ tasks.<id> }}`: that binds the ENVELOPE (status + timestamps),
  so `nika test` goldens drift red on every run. `nika check` teaches
  this as `[envelope-output]`; fix the binding, never re-baseline
  around it.
- A task that reads another task's output binds it in `with:` —
  `with: { alias: "${{ tasks.<id>.output }}" }` — and the body reads
  `${{ with.alias }}` (the binding IS the edge; `tasks.*` anywhere
  else is NIKA-VAR-021). Pure ordering is `after: { <id>: success }`,
  and the predicate set is closed: `success` · `failure` · `skipped` ·
  `terminal` (`NIKA-DAG-005`).
- Models are `provider/name` (`ollama/llama3.2:3b` local-first ·
  `mock/echo` offline preview).
- Timeouts are quoted Go-durations (`timeout: "7m"`) — give local
  providers ≥300s: thinking models routinely think past 30s.
- Determinism is declared, not hoped: `run:` carries `entropy:`
  (`ambient` — the default when `run:` is absent — · `none` ·
  `{ seeded: <n> }`) and `clock:` (`system` · `virtual`). A
  contradictory declared pair refuses at parse; an `entropy: none`
  that still consumes randomness refuses at check.
- Structured output: give `infer:` a `schema:`; add
  `additionalProperties: false` for a deterministic shape.
- Auth rides `headers: { x-api-key: "${{ secrets.KEY }}" }` (masked ·
  declared in `secrets:` with its `egress:` sink) — never `exec: curl`
  for the sake of a header.
