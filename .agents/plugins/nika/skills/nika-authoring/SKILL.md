---
name: nika-authoring
description: Author, check and repair Nika workflows (.nika.yaml files — the workflow language for AI). Use when writing or editing a *.nika.yaml file, converting a repeated AI task or prompt chain into a workflow, or when nika check reports NIKA-XXXX findings to fix.
---

# Authoring Nika workflows

Nika turns repeatable AI work into files: one `.nika.yaml`, four verbs,
audited **before** it runs. You author the file; `nika check` is the
oracle; the human runs it.

## The oracle is evidence, not permission

Use the installed engine's `nika check --json` report, not a second
checker assembled from this prose. Keep its engine/spec identity with the
result. Judge `clean` and `native_strict_clean` separately from `paid_ready`:
a file can be paid-ready and still fail a permits finding. For composition,
inspect `judged` and the children's findings; a report that did not resolve
children cannot certify the tree.

Check readiness never authorizes execution, spend, a wider permit, or a
publication. `--model mock/echo` changes the envelope model, not per-task
model pins, subprocesses, network tools, file writes, or secret sources.
Before an authorized rehearsal, review those effects and use an isolated
workspace with the intended boundary. In an editor, workspace trust is a
separate host decision; an offline model does not bypass it.

## Read the workspace, then two examples before you write

**This file is the map. The examples are the territory.** A map this
detailed is exactly why authors skip the ground, and the ground is where
the shapes live. Nothing below replaces reading two real files.

Join the workspace before adding to it. A local workflow may already own
the job, its conventions and its permits boundary:

```
nika list                       # workflows below this directory
nika explain <candidate>        # waves · cost · touches · run line
nika inspect <candidate>        # tasks · verbs · graph anatomy
nika check <candidate>          # the oracle: clean or an exact repair
```

`nika list` lists candidates; it does not certify them. Reuse or extend a
matching local file only after `nika check` is clean. If nothing local fits,
continue to the embedded shelf; even when one does, the two canonical reads
below remain the cheapest shape check before a structural edit.

The cost of skipping it was measured on 2026-07-28. Six authors each
wrote one workflow from a real intention with this skill loaded and
nothing else. **None reached a green check one-shot: 45 check→fix rounds
between them, 7.5 on average, 11 at worst.** One of those authors spent
**eight** rounds on their file, then read two example files, and wrote their NEXT workflow **one-shot green. Zero
rounds.**

Eight to zero, and the delta was two reads; the measured alternative
is 7.5 check→fix rounds. Reading
them is not diligence, it is the cheapest move on the board.

**The order, every time. Never write first and look second:**

```
nika try                        # the shelf · the path, then the jobs
nika new <slug>                 # take the one matching your intent (table below) — read it
nika new <second-slug>          # take the one covering what the first did not — read it
                                # only now open your file
```

Read for SHAPE, not for prose. Four things, in this order: which verb
each task carries · where the `with:` edges are · what the `permits:`
block ended up containing · how the last task lands the artifact. Those
four are the decisions that cost rounds when guessed instead of copied.

### Which example answers which intent

| Your intent | Read this |
|---|---|
| one model call, nothing around it | `01-hello` |
| independent steps, then a merge | `02-parallel-fanout` |
| shell out to a real binary (git · docker) | `03-exec-pipeline` |
| a model must return JSON fitting a shape | `04-schema-retry` |
| fetch a URL and shape what comes back | `05-fetch-chain` |
| open-ended work, step count unknown up front | `06-code-review` |
| the same task for every item of a collection | `07-for-each-locales` |
| extract facts, then score them without a second infer | `13-extract-then-law` |
| publish or abstain from a Decision Bundle | `14-decide-publish` |
| an agent drafts a file and checks it until valid | `15-compose-self-check` |
| the run reads its own DAG / cost / records | `16-inspect-self` |
| mock TTS that writes a real WAV | `17-tts-self` |
| land a typed artifact on disk | `meeting-actions` |
| poll something, act only when a condition holds | `price-watch` |
| rows in, chart and report out, zero model calls | `csv-chart-report` |
| a batch where bad items must not kill the run | `etl-quarantine` |
| a folder of files, one job per file | `localization-factory` |
| a human signs before an irreversible step | `release-train` |
| a job too big for one file | §Composition below, then `01-hello` for the child |

Second column pinned to the pack by the engine's own test (every slug
this table names resolves through `nika_pack::example`). Any slug works
with or without its `showcase/` prefix and with or without the
`.nika.yaml` extension. `nika new <slug>` makes one yours;
`nika new <name>` does the same from the template side
(`nika new '?'` prints that set).

## The loop (always)

1. **Start from the example you just read**, never from a blank file.
   The section above is not advice, it is step zero: `nika try` ·
   `nika new <slug>` (take the lesson — the file is the read) ·
   `nika new <template> <file>.nika.yaml`. An author who writes
   first and reads second pays the measured 7.5 rounds.
2. **Write the file.** The envelope is `nika: <id>` (kebab-case — the
   workflow id lives ON the tag since 2026-08-12; that one key carries
   BOTH the mark and the name, and `description:` died with the
   `workflow:` object, which is no longer an envelope key at all) + a
   `tasks:` MAP keyed by task id —
   the key IS the identity, never a `- id:` sequence. Pick
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
   not pass `nika check --native-strict` AND `paid_ready: true`.**
   `--native-strict` is the run-gate bar (an `exec:` a builtin covers).
   `.paid_ready` is the paid-infer bar
   (`nika check --json <file> | jq .paid_ready`).
   A green exit with leftover `infer-as-law` / `digit-string-enum` /
   `glob-readme` / `jq-as-map` / `unproven-law` is
   legal, not the one-way. The MCP `nika_check` oracle fails
   `infer-as-law` and `digit-string-enum` by default.
   The exec ledger does NOT buy an exemption (measured: a `.py` wrapper
   fails with a complete ledger) — it documents intent for a reviewer.
   What passes is an `exec:` of a real tool (`git`, `docker`); what
   fails is an `exec:` of a `.py`/`.mjs`/`.sh` wrapper, ledger or not.
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
7. Pin it for CI **only when the mock run needs no network, subprocess,
   or write effect**: `nika test <file> --update` writes
   `<file>.golden.json`; `nika test <file>` replays and compares —
   deterministic, zero keys. The simulated test plane refuses those effects
   deliberately. For an effecting workflow, rehearse with
   `nika run <file> --model mock/echo` in scratch, inspect the artifacts, and
   verify its trace; never promise a golden that cannot run.
8. **Prove a run that mattered**: execution journals are enabled by default
   under `.nika/traces/`; a refusal before execution, disabled recording or
   lost ownership can leave no complete trace. `nika trace verify <trace>` climbs a
   four-tier ladder and reports the highest tier honestly attained —
   chain OK · **SEALED** (the run signature verifies against a custody
   key) · **ANCHORED** (the detached transparency-log sidecar verifies
   fully offline) · **REPLAYED** (`--replay <fresh-trace>` compares a fresh run;
   verify never re-executes). `nika trace show <trace>` reads the card;
   `nika trace evidence <trace>` exports the pack an auditor reads without
   trusting your summary. Verification never creates or seals the journal;
   an internally consistent chain does not itself prove producer honesty.
   Cite the trace and the actual proof tier, never a memory of the run.

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

## The one way (take the default, and the checker goes quiet)

Every authoring decision has a default. Take it unless the job forces
otherwise, in this order:

1. **Shape before content.** Two example reads (take them:
   `nika new <slug>`), then the file (§Read two examples · measured 8 rounds → 0). Never a blank
   file. The outer shape decides the task graph before a single prompt
   is written, and copying a shape is free where guessing it is not.
2. **One job, one task, one verb.** If a task needs an "and then", it is
   two tasks. The verb IS the key.
3. **Pick the verb by execution model, not convenience.** `invoke:` when
   something callable already does it · `infer:` when a model must
   produce judgement or language · `agent:` when the number of steps
   cannot be known in advance and must be bounded · `exec:` only when
   the first three genuinely cannot.
4. **Classify every value before writing it.** Caller-supplied →
   `inputs:` · deployment-supplied → an `inputs:` entry with
   `required: false` and a `default:` · fixed here → `const:` ·
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
   body is tight; one written from intent is wishful. Expect a
   `NIKA-AUTH-006` the moment you save the permit-less draft (the
   write-time hook checks on your behalf): that finding is this step
   working, not a mistake to patch around. Read the review notes the
   inference prints and supply the paths it says it cannot compute.
9. **Fail on purpose.** Transient failure → `retry:` · expected absence
   → `on_error: on_codes + recover:` · cleanup that must always happen →
   an ordinary task you name, declaring `after: { producer: unwind }`.
   Swallowing an error is never the plan.
10. **Prove it before handing it over.** `nika check` clean, then
    `--native-strict`, then a golden pin if the workflow is hermetic.
    Only then does the human get the run line.

## Paid infer (the order that is cheaper than tokens)

Measured on a 40+ task OpenAI extract → law run. Do not rediscover
this with a paid seat.

1. `nika check --json --native-strict` until `clean` and `paid_ready`
   are both true (zero findings, zero paid-run hints).
2. Probe every new builtin in a one-task file on `mock/echo` *before*
   wiring it after a paid `infer:` (`nika:inspect` is live — lesson
   `16-inspect-self` asserts `available` at run start).
3. Freeze the extract schema type. Numeric facts are `type: integer`
   with a numeric `enum`. `enum: ["0","1","3"]` is the shape models do
   not emit (JSON `3` — hint `digit-string-enum`).
4. Pin the glob. `held/*.md` includes `README.md`. `exclude:
   "**/README.md"` (hint `glob-readme`).
5. **The model extracts facts. `nika:jq` or `nika:decide` is the law.**
   A second infer to "pick the level" is the expensive mistake.
   The shape is `13-extract-then-law`. Prove the law on const fixtures
   (`unproven-law`) — `14-decide-publish` is the named bundle.
6. Then, and only then, swap `model:` to a paid seat.

`for_each` + `item.field` is resume-eligible as a **whole fan** when
the collection and definition did not change. A mid-wave crash still
replays every item. After `. as $c` in jq, write `($c | map(...))`
(hint `jq-as-map`). A red last `nika:assert` quarantines `out/`
(`.nika/quarantine/<trace>/` — hint `assert-quarantine`).

## After valid: is there a better one-way? (not optional)

`nika check --native-strict` green means the file is *legal*. It does
not mean it is the cheapest, most native, or most honest file.
`.paid_ready` reports the paid-infer blockers the checker knows about,
not execution consent or a complete safety proof. A runnable handoff
also needs a clean native-strict check and the intended authority. Each
question has a command or a file. Do not reason from memory.

Zhang, Kraska, Khattab 2026 (arXiv:2512.24601, Recursive Language
Models): the prompt is an *environment* you inspect and decompose,
not a blob you swallow. In Nika that environment is the file + two
examples + `nika catalog --tools`. Recursion is `invoke: { workflow: }`
or `for_each:` over items — never one giant infer. Verification is
`nika:jq` / `nika:decide`, never a second infer that names the verdict.

0. **Is `.paid_ready` true?** `nika check --json <file> | jq .paid_ready`.
   `false` → repair `.next` (kind · task · advice) first, then the rest
   of `.paid_blockers[]`; also inspect `.compiled` and the findings.
   Do not swap off `mock/` while a paid blocker remains.
1. **Did I read two examples first?** `nika try` then `nika new <slug>`
   twice. Skipping this is the measured 7.5-round tax.
2. **Is every `exec:` a real tool?** `nika check --native-strict`. A
   `.py`/`.sh` wrapper is not a tool.
3. **Does any infer name the verdict?** Hint `infer-as-law`. Extract
   integer facts; `nika:jq` or `nika:decide` is the law
   (`13-extract-then-law`). A second infer whose schema is a language
   enum (BCP-47 · sentiment) is language, not this hint.
4. **Is every numeric enum `type: integer`?** Hint `digit-string-enum`.
5. **Does a markdown glob include README?** Hint `glob-readme`.
6. **Did I probe every new builtin on `mock/echo`?** One-task file,
   then wire it. `nika:inspect` is live (`16-inspect-self`).
7. **Would a closer template have given this graph?** `nika new "?"`
   and `nika new "the job in plain words"`. If a skeleton is closer
   than what I wrote, start over from it.
8. **Did `nika explain <file>` stay honest?** Waves · cost (FLOOR ≠ $0)
   · touches · the **before a paid model** panel. If a paid-run hint
   remains, the file is not done.
9. **Is the law proven on known answers?** Hint `unproven-law`. A
   jq/decide that scores an infer needs a const-fixture `nika:assert`.

A green check with `paid_ready: false` is not a handoff. Repair the
blocker. Re-run 0–8 until findings are empty and `paid_ready` is true,
or every remaining *non-paid* hint has a one-line reason in the file
header (the honest red class, CONVENTIONS §10).

## Cost honesty (never hide unknown spend)

- `nika check` prints the cost ceiling BEFORE any token: `≤ $X` is a
  ceiling · `≥ $X FLOOR` means at least one task is unbounded — name
  the reason (a missing `max_tokens`, an uncataloged model, an
  expression fan-out), never round it to $0.
- **The ceiling covers OUTPUT tokens only. The prompt is not in it.**
  `max_tokens` is the max OUTPUT tokens, and that is what the sum
  prices; `input_per_million` has no reader in the checker. Measured
  2026-07-28: a workflow that fetches a 3.2 MB document and
  interpolates it into one prompt reports `$0.0075` and would bill
  about `$2.46` in input alone. **When a prompt interpolates fetched or
  file content, say so at handoff and do not quote the ceiling as the
  bill** — quote it as the output half, and name the unpriced input.
  (The same repro is 4x over that model's context window, which nothing
  reports either: check the window yourself when a prompt carries a
  document.)
- A local model (`ollama/…`) is **unpriced compute, not « free »** —
  say "unpriced", never "$0" or "free".
- **A `FLOOR` on a CLOUD model is not a cheap model, it is a missing
  price row, and the short catalog id is often the reason.** Measured
  2026-07-28: `mistral/small` (the id `nika catalog` prints) reports
  `$0.0000 FLOOR · no catalog price (local/unknown model)` while
  `mistral/mistral-small-latest`, the same model, reports a
  `$0.0024 worst-case output ceiling`. The wording says "local/unknown" about
  a cataloged cloud model. Try the full model string before believing a
  cloud floor; if it still floors, hand the human the word "unpriced"
  and never a number.
- A spend cap rides the run: `nika run <file> --max-cost-usd <n>`
  refuses a known over-budget floor before execution. During execution,
  crossing the metered budget stops new admissions; already-started calls
  finish and count, so a concurrent wave can overshoot. Unpriced calls
  have no measured USD bound. Do not promise a hard invoice ceiling.
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
already knows. **The form is `workflow:` INSIDE `invoke:`, a sibling of
`tool:`, never a tool name.** This is a complete parent. Check is green
only when the child sits at that relative path — the next law:

```yaml
nika: site-audit-parent
inputs:
  target:
    type: string
permits: {}
tasks:
  audit:
    invoke:
      workflow: "./audits/site-audit.nika.yaml"   # sibling of tool:, not a tool
      args:
        url: "${{ inputs.target }}"
```

**The decoy, and it is live.** `nika:compose` exists as a builtin, so
`tool: compose` is the move an author reaches for. Both spellings fail,
and both diagnostics point AWAY from the answer. Measured 2026-07-28:

| You write | You get | Why it misleads |
|---|---|---|
| `tool: compose` | `NIKA-PARSE-019` · "expected `nika:<path>` or `mcp:<server>/<tool>`" | reads as "prefix it" · which lands you on the decoy |
| `tool: "nika:compose"` | `NIKA-BUILTIN-001` · compose is the agent-loop sub-workflow spawner, valid ONLY inside an `agent:` tools whitelist | correct refusal, and it never names `invoke: { workflow: … }` |
| the same, again | `ARGS` · "no `workflow` arg · did you mean `workflow_yaml`?" | a second wrong turn · toward inlining a whole YAML string |

Neither diagnostic teaches the right form today. That is why this
section exists: `nika:compose` is for an AGENT to check a draft it
wrote, not for a parent to call a child.

**Four laws, all judged at check.**

- **STATIC target.** A literal path or a pinned
  `registry:owner/name@version`. A `${{ }}`-templated target refuses
  `NIKA-COMP-001`: a call graph you cannot draw before the run is a
  call graph you cannot bound. A relative path resolves from the PARENT
  FILE's directory, not the shell's cwd.
- **The child is READ at check time.** A path that does not resolve is
  `NIKA-COMP-001` too, so a parent cannot pass check without its
  children present on disk.
- **One target per `invoke:`.** `tool:` and `workflow:` together is the
  same refusal class as two verbs on one task.
- **Containment, and the parent declares it (`NIKA-COMP-002`).** The
  child's boundary must be a SUBSET of the parent's, and the parent does
  not inherit anything by calling: a `permits: {}` parent calling a
  child that writes a file is refused at check AND at the run gate, once
  per effect: `child fs write <path> is outside the parent boundary` and
  `child tool nika:write is outside the parent boundary` (spec 14 laws 3
  and 4). The parent's `permits:` must be the union of what every child
  touches. `nika check --infer-permits` will NOT compute that half for
  you: it prints a review note saying the child's boundary is resolved
  by the composition lane, never inferred. **Read that message with
  suspicion**: `child tool X is outside the parent boundary` also fires
  when the parent grants X and the CHILD's own `permits:` is the block
  missing it. Check the child alone before widening the parent.

**What the parent reads back.** `${{ tasks.<id>.output }}` is the
child's whole `outputs:` map, and `${{ tasks.<id>.output.<name> }}`
reaches one entry (measured: a child declaring `greeting:` yields
`{"greeting": …}` and the deep reference passes TYPES). Add `returns:`
to pin the shape, with one trap: the shape is written INLINE — there is
no envelope `types:` block to declare a name in (it refuses
`NIKA-PARSE-005`) — and the grammar is Nika's own, not JSON Schema, so
it is
`{ object: { greeting: string } }` and never `{ type: object,
properties: … }` (that spelling is `NIKA-TYPE-001` too, on the
constructor). `schema:` on an `infer:` IS JSON Schema. Two type
languages in one file, and only one of them takes `type:`.

**What crosses the boundary, and what the claim covers.**

- The composed checker folds children's output-token estimates into
  the parent's `cost.composed` and totals, including calling-task
  multipliers. Verify `judged.composition` and `judged.children`, not
  only the parent's direct inference-task count. The reader-less Rust
  checker is not a substitute for the resolved CLI check.
- The child receives the parent's remaining metered budget at call
  time. This is not an atomic reservation across concurrent children:
  already-admitted calls can overshoot, and static estimates still
  exclude input-token cost. Tighten concurrency and per-call limits;
  never call the parent's cap a hard invoice ceiling.
- The child writes **its own trace file**. A parent run leaves two
  chains in `.nika/traces/`; the parent's failure line names the child's
  trace id, and that is the one to `nika trace show`.

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
   | INTROSPECTION | `nika:compose` (agent-loop only · calling a child is `invoke: { workflow: … }` · §Composition) · `nika:inspect` |
   | MEDIA | `nika:chart` · `nika:image_generate` · `nika:image_fx` · `nika:tts_generate` |

   The NAMES above are canon — that is the whole set. The argument
   CONTRACTS are not: read them from `nika catalog --tools`
   (`--json` for the model-facing JSON Schemas) before calling one,
   and never guess an arg name.

   The reflexes worth memorising: HTTP (curl/wget/helper fetch) →
   `nika:fetch` · file plumbing (cat/tee/cp/mkdir) →
   `nika:read`/`nika:write` (`create_dirs: true`) · JSON shaping
   (jq/sed) → `nika:jq` or an `extract:` binding · in-place edits →
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

**`NIKA-SEC-004` — an untrusted value reached an effect argument.** An
`inputs:`-supplied or fetched value that check cannot resolve DEFERS to
a mandatory run-time re-gate; escaping that re-gate is SEC-004. The
diagnostic talks about the capability boundary, so the reflex is to
widen `permits:` — **that reflex is the trap, and it dead-ends.**

**The door is `lift:`** — a task-level list, the ONLY sanctioned lift
(spec 10 §the authored doors). One construct, two laws: `taint` and
`data-as-code`; the law is a PARAMETER of the door, never a second
spelling (`declassify:` and `inert:` were those spellings, and are dead).
This is a complete nine-key file (checked on 0.109 · rc=0):

```yaml
nika: load-reviewed-path
inputs:
  p:
    type: string
permits:
  tools: ["nika:read"]
  fs:
    read: ["./reviewed"]
tasks:
  load:
    invoke: { tool: nika:read, args: { path: "${{ inputs.p }}" } }
    lift:
      - law: taint              # the law this task opens
        from: inputs.p          # ONE binding
        because: "deployment-controlled path, reviewed at release time"
```

`law:` and `because:` are required on every entry, `from:` on `taint`
only (forbidden on `data-as-code`); `because:` must be non-empty — it is
recorded in the receipt with the taint path and the value digest. It
lifts the TAINT law only: the value is still matched against the
declared boundary, so this is never a permit bypass. A lift that would
not have fired is refused (`NIKA-AUTH-011`), so dead lifts cannot
accumulate.

**Why the staging recipe is the wrong first move.** Landing the value
in a file with `nika:write` and passing the PATH as argv looks safe, and
it is — until the CLI has to READ that file back. That read adds
`fs.read`, which completes the lethal trifecta, which makes a dominating
human gate mandatory. Measured in a real session: the chain runs shim →
`fs.read` → trifecta → mandatory gate → a gate that cannot be answered
(see the run notes on `nika:prompt`). Reach for `lift:` first.
Staging remains correct where the value genuinely must not touch a
command line AND nothing reads the file back inside the same workflow.

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

The ledger is for the REVIEWER, not for the checker — it never silences
a finding. `--native-strict` judges the SHAPE of the exec: a real tool
passes, a script wrapper fails, and a row in the ledger changes neither.
If the wrapper is genuinely unavoidable, the honest move is to say so to
the human at handoff, not to expect a green.

## Discipline

- References: `${{ inputs.x }}` · `${{ const.x }}` ·
  `${{ secrets.X }}` · `${{ tasks.<id>.output }}`
  · `${{ with.alias }}` (never inline a credential) · and inside a
  `for_each:` body only, the loop-scoped `${{ item }}` · `${{ index }}`.
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
  Numeric facts are `type: integer` + a numeric `enum`. The model
  extracts facts; `nika:jq` / `nika:decide` is the law
  (`13-extract-then-law`).
- Auth rides `headers: { x-api-key: "${{ secrets.KEY }}" }` (masked ·
  declared in `secrets:` with its `egress:` sink) — never `exec: curl`
  for the sake of a header.

### Writing JSON: build the VALUE, never the braces

Hand-writing JSON punctuation around an interpolation is the one way to
get a green check, a green run, and an unreadable artifact. Both halves
below were measured on 2026-07-28 with a value containing a quote and a
newline. This is a complete nine-key file (checked on 0.109 · rc=0):

```yaml
nika: write-json-value
inputs:
  v:
    type: string
permits:
  tools: ["nika:jq", "nika:write"]
  fs:
    write: ["./out/**"]
tasks:
  # ✗ green everywhere, and the artifact does not parse.
  naive:
    with: { v: "${{ inputs.v }}" }
    invoke:
      tool: "nika:write"
      args: { path: "./out/naive.json", content: '{"value": "${{ with.v }}"}' }
      # writes  {"value": "he said "hi"
      # and a newline"}                       ← malformed. Nothing warned.

  # ✓ build the object as a VALUE, then write the value whole.
  build:
    with: { v: "${{ inputs.v }}" }
    invoke:
      tool: "nika:jq"
      args: { expression: "{ value: .v }", input: { v: "${{ with.v }}" } }
  safe:
    with: { obj: "${{ tasks.build.output }}" }
    invoke:
      tool: "nika:write"
      args: { path: "./out/safe.json", content: "${{ with.obj }}", create_dirs: true }
      # writes  {"value":"he said \"hi\"\nand a newline"}   ← parses.
```

**The law: an interpolation may be a whole JSON value or a whole string,
never a fragment welded between quotes you typed.** A structured value
interpolated into a string argument is serialized as JSON by the engine,
with escaping. Two ways to get one: `nika:jq` (any reshaping), or an
`infer:` carrying a `schema:` (a model's typed output, the shape
`meeting-actions` uses to land its artifact).

**Then validate the artifact twice, because well-formed is not right.**
Parse it, AND spot-check one value against a number you computed by
hand. A green run that emits malformed JSON is a failure with a
checkmark on it; a green run that emits well-formed JSON full of wrong
values is the same failure wearing a better disguise, and `json.loads`
cannot see it.

**Prove `nika:jq` against known answers.** The engine corrects its
upstream `scan` implementation to collect every match, so
`[.s | scan("\\S+")]` on `{"s":"one two three"}` yields all three
words. Do not preserve a workaround for the retired first-match bug.
Keep the actual invariant: assert the expected values, not only valid
JSON or a green exit, including empty input and regex flags where used.

## When the language pushes back, that is a finding

Every friction you meet writing a workflow is a datum about the spec or
the engine. Do not route around it silently — a workaround written once
is a workaround written forever, and the next author meets the same wall
with no trace that you were here.

**The triggers.** Any one of these is enough:

```
  a green `nika check` whose run dies
  a diagnostic that does not name the remedy that WORKS
  a builtin that returns a plausible, wrong answer
  a form you had to guess because no surface teaches it
  a workaround you write "just this once"
  two lines of the SAME output that disagree
  a green run that produces a malformed artifact
```

**The move, in order.**

1. **Reproduce** it small. Never "I think it does X" — the exact file and
   the exact command.
2. **Locate** it: a `file:line` in the engine, or a `§` in the spec.
3. **Write it down** where it survives the session — a local finding
   or plan entry; post an issue only when external reporting is in scope.
4. **Fix at the source.** The spec repo, not a vendored copy of it: a
   mirror is re-vendored and your fix disappears.
5. **Leave a ratchet** — a test, a hint, a gate. Without one the class
   comes back.

**Why this earns its place in an authoring skill.** Writing a five-task
provenance workflow on 2026-07-28 surfaced three defects in twenty
minutes, none of them being looked for: a builtin's required arg missing
from the example that taught it, a `fetch → jq` pipe that hands text to a
tool expecting a value, and a scalar interpolated into literal JSON that
produced an unquoted field — green check, green run, unreadable artifact.

That is the ordinary rate. The language is good and it is not finished,
and the only reason it converges is that the people using it report what
they hit. **A workflow you had to fight is worth more as a bug report
than as a file.**
