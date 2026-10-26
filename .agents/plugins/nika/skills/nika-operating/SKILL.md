---
name: nika-operating
description: "Prepare existing Nika workflows for unattended operation: budgets, permits, secrets, model changes, CI and trace export. Use for operational changes to a working workflow."
---

# Operating Nika workflows

Operating prepares a workflow for unattended use: review spend exposure,
declared effects, secret destinations, model choice and available traces.

The check reports readiness; it does not authorize unattended execution.
Carry out execution already authorized by the user within its effects and
spending scope, through the normal engine and host gates. Ask only for a
decision still missing; permission to run does not supply a human-gate answer.
Judge `clean` (the exit's verdict under the flags you passed; under
`--native-strict` the refusal is a `findings[]` row and `native_strict_clean`
repeats `clean`), `paid_ready` and resolved-child coverage separately, with
the engine/spec identity that produced the report.

## Spend (the envelope is part of the contract)

- `nika check <file>` estimates output-token cost BEFORE any token: `≤ $X`
  bounds that output estimate, not prompt/input cost or the entire invoice.
  `≥ $X FLOOR` means at least one task is unbounded — fix
  the reason (a missing `max_tokens`, an uncataloged model, an
  expression fan-out). Before unattended use, state the exposure and
  applicable controls; unpriced compute must not be called a bounded USD cost.
- Cap the run: `nika run <file> --max-cost-usd <n>` refuses a known
  over-budget floor before execution. Crossing the metered budget during
  execution stops new admissions; already-started calls finish and count.
  A concurrent wave can overshoot, and unpriced work has no measured USD
  bound. Choose concurrency and model limits with that exposure in mind.
- A local model is **unpriced compute, not free** — say "unpriced",
  never "$0".

## Permits (the boundary is mandatory)

**Absent is not the unconfined floor — it is ZERO authority.** A body
carrying any effect with no `permits:` block refuses `NIKA-AUTH-006`
at check, before a token is spent, and the runtime gates refuse before
any spawn. A pure-compute body states the zero explicitly:
`permits: {}`.

```
nika check <file> --infer-permits
```

proposes a `permits:` block and unresolved review notes. Reconcile them with
the intended effects before applying the justified grants. From then on the boundary is default-deny: a new host,
path or tool must be added consciously, in a reviewable diff. Permits
are data, not config — they travel with the file through PR review.

Three refusals to expect while tightening: a bound that is an
interpolation instead of a literal (`NIKA-AUTH-007` — a self-serve
boundary is no boundary) · a `*.` subdomain wildcard
(`NIKA-AUTH-010` — it hands the boundary to the zone operator; name
exact hosts) · a `permits: { env: … }` entry naming a dangerous-floor
variable the engine strips unconditionally (`NIKA-AUTH-009` — an
inert dead grant).

A spawned child inherits NOTHING from the engine: its environment is
composed from a cleared slate — the runner floor ∪ the names in
`permits: { env: [NAME] }` ∪ the task's own `env:` map. A workflow
that leaned on an ambient variable must now name it.

## File creation and uncertain results

Use `nika:write` with `overwrite: false` to preserve an occupied destination,
including one created concurrently before publication. An existing destination
returns `NIKA-BUILTIN-WRITE-002`; a backend unable to publish exclusively refuses
instead of falling back to replacement. Keep the required filesystem permits.
After a lost response or cancellation, inspect the file and trace before retry:
publication may already have completed. Atomic visibility does not promise
fsync durability or that a detached write has stopped.

## Stop and resume

- Ctrl-C (or SIGTERM) stops a run cleanly: work already in flight
  completes and is counted, tasks not yet started are cancelled, the
  trace is sealed with `workflow_cancelled`, and the run exits 130. A
  second Ctrl-C aborts mid-flight and leaves the trace incomplete.
- `nika run <file> --resume <trace>` reuses every task the trace
  completed (`cache hit (resume)`) and runs the rest. A task in flight
  at the Stop completed: reconcile what it did before anything
  downstream of it runs again.
- A run paused on a human gate exits 4 and resumes the same way, with
  the person's answer: `--resume <trace> --answer <task>=<value>`.

## Secrets (masked, declared, sunk)

- Every credential rides `${{ secrets.X }}`, declared in the
  `secrets:` block (`source: env` + `key: VAR_NAME`) with its
  `egress:` sinks — the engine masks it in logs and refuses to send
  it anywhere but the declared sinks.
- **The taint FLOWS**: the output of a task that used a secret is
  secret-derived, and every downstream sink it reaches needs its own
  `egress:` entry. An authed fetch whose output feeds an `infer:`
  declares both:

  ```yaml
  secrets:
    gh_token:
      source: env
      key: GITHUB_TOKEN
      egress:
        - to: "nika:fetch"
        - to: "infer"
  ```

  The checker names the exact chain when one is missing
  (`secrets.X → tasks.A.output → tasks.B.output`).
- For a `host:`-scoped egress finding, use the checker's actual diagnostic.
  A literal host can remain provable when the path or query is interpolated.
  If the authorized destination is known, express it with that host fixed
  and re-check; if it is unknown, request the destination, not the secret.
  Never remove or widen `host:` merely to pass a check. A broader destination
  policy requires explicit authorization for that change. Preserve secret
  egress, `permits:` and runtime re-gating; an unresolved refusal still blocks.
- Use the declared secret source at run time; CI can inject an environment
  source. Never put a credential literal in YAML. Public redaction is not
  journal confidentiality: raw task outputs can contain sensitive data,
  so protect trace storage and review evidence before sharing it.
- `nika doctor` audits the machine: binary, PATH, provider env vars.

## Models (a one-line swap, both directions)

- Models are `provider/name`. `nika catalog` is the embedded registry:
  providers · models · capabilities · which env var each needs.
- Shape with `mock/echo` (deterministic, zero keys, no model call). Prove
  structure first, spend later. The CLI override changes the envelope
  model only: inspect per-task model pins before assuming a rehearsal
  calls no model. Tools, writes, subprocesses and secret sources remain
  real: a fetch still reaches its host, and `nika try` names those hosts
  before it runs.
- Local models: `--model ollama/<model>` runs on the machine — same
  file, same check, same trace. Give local providers `timeout: "300s"`+.
- The file does not hardcode a vendor: swapping cloud↔local is a
  `--model` flag or one line in the YAML, never a rewrite.

## CI (the check is the gate, the golden is the pin)

- Gate every PR: `nika check <file> --json` — exit 0 clean · 2
  findings · 3 broken oracle. Parse `clean`, `conformance[]`,
  `pricing`, `models_resolve` from the payload.
- Pin behavior: `nika test <file> --update` writes
  `<file>.golden.json` from a mock run; `nika test <file>` replays
  and compares — deterministic, zero model keys, CI-safe.
- `nika test` uses the simulated plane and refuses network, subprocess
  and write effects. Golden only workflows that fit that plane. For an
  effecting rehearsal, use `nika run <file> --model mock/echo` in scratch:
  that flag mocks the model, not the tools, so declared effects and secret
  sources remain real. Inspect those artifacts and the trace separately.
- `--native-strict` in CI keeps `exec:` honest: any shell task an
  embedded builtin covers fails the gate (the exec ledger documents
  the survivors).
- Recurring work is a beat in the project's `nika.yaml`, never a cron
  line, a CI schedule or a hand-written timer (they skip the firer: no
  missed-slot rule, no overlap lock, no ceiling per firing, no firing
  record). One `arm:` entry names the workflow, the cadence with its
  zone inside (`TZ=Europe/Paris 30 7 * * 1-5`, an IANA zone name), the
  ceiling of one firing (`plafond:`) and what a missed slot becomes
  (`manqué:`); neither has a default: take each from what the person
  gave, what the project keeps or what they delegated, and ask, once, only
  for one still missing. Write the `manqué` value the targeted binary fires:
  0.122.0 and 0.123.0 fire `sauter` and `rattraper-une-fois` and read
  `rattraper` UNREADY; 0.123.0 reads the bounded `{ rattraper: N }` UNREADY
  too, where 0.122.0 refused it; `nika arm --json` on that binary is the
  judge. Per-beat `inputs:` give
  the workflow's declared inputs their values. `nika check nika.yaml`
  judges the file; `nika arm --json` reads every beat and reports READY,
  UNREADY with its `blockers`, or DORMANT, writing nothing. One scoped
  instruction can cover saving, running and, when it says so, activating,
  each once: carry out what it covers without asking again. Activate a
  READY beat: `nika arm --emit launchd|systemd --write`, then loading the
  unit, or a resident `nika serve`. `--emit` refuses an UNREADY beat: no
  unit arms a run that cannot start. An unknown alone never makes a beat
  UNREADY: a beat whose workflow calls a child reads READY and names the
  unknown « a nested workflow's routes are judged only at its own
  dispatch »; `nika serve --once --dry` says what a firing would do. A
  unit carries the revision of the schedule it was written for: after a
  cadence change, emit and load it again (the firer refuses a stale wake
  and names that re-emit). A suspension is the triple `actif: false`,
  `raison:` and `jusqu_au: YYYY-MM-DD`; it ends at midnight after that
  date, in the cadence's zone. Deleting the line does not disarm on its
  own: the next `--emit … --write` retires that beat's unit, removing its
  file and printing the unload command to run.
  Reference: https://docs.nika.sh/reference/arm
  (the project file: https://docs.nika.sh/sdk/project/nika-yaml)

## MCP servers and definition drift

A configured MCP server that changes its tool definitions after you
approved them is the rug pull. Nika pins every tool on first contact
(TOFU) into `.nika/mcp_pins.json` beside a reviewable snapshot: first
contact enrolls loudly, a match proceeds silently, ANY drift fails
closed with a diff naming the CHANGED field and returns no tools. A
hand-edited lockfile is `NIKA-MCP-004`, never a silent re-TOFU.
Re-pin after human review: `nika mcp approve <server>`.

A pin detects changes relative to the enrolled definitions. It does not
prove those descriptions are truthful, the server is harmless, or an effect
was delivered. Keep the intended permissions and review the actual result;
re-pinning is a review decision, not an automatic fix for drift.

## Observability (the journal is exportable, not captive)

- `nika trace export <trace>` projects the journal to OTLP/JSON
  lines — drag into Jaeger UI (≥1.60) or POST to any OTLP/HTTP
  endpoint. Local file, zero collector, zero vendor.
- `nika trace ls` shows the store; retention never collects the `★`
  newest trace of each workflow. `nika trace rm --older-than <dur>`
  prunes deliberately.
- Audits cite `nika trace verify <trace>`, which reports the highest
  tier honestly attained: chain OK · SEALED (the `run_sealed`
  signature verifies against a custody key) · ANCHORED (the detached
  sidecar verifies fully offline) · REPLAYED (`--replay <fresh-trace>` compares a
  fresh run). A journal that never reached a terminal frame verifies
  INCOMPLETE. Never a log screenshot.
- `nika trace evidence <trace>` exports the auditor's pack — journal +
  manifest + receipt + a `VERIFY.md` naming the exact commands. Hand
  THAT over, not a summary you wrote.
- `nika trace anchor <trace>` notarizes the journal head OUTSIDE the
  journal (public transparency log + an RFC 3161 timestamp, written
  to a detached sidecar). An explicit NETWORK act — the verb IS the
  opt-in, never a default.
- Author-binding: `nika sign <file>` mints a detached
  `<file>.minisig` (`--check` verifies) and
  `nika run --require-signature` refuses an unsigned or
  invalidly-signed workflow at exit 2. `nika key` is the run-signing
  key lifecycle (mint · TOFU fingerprint · rotate — old public halves
  stay verifiable).

## Production checklist

1. `nika check <file>` — clean, ceiling not floor.
2. `nika check <file> --native-strict` — exec ledger complete.
3. `permits:` declared — absent is ZERO authority, not a floor; a
   pure-compute body still says `permits: {}`.
4. Secrets in `secrets:` with sinks — env-injected, never literal.
5. Hermetic workflow: golden pinned (`nika test <file> --update`,
   committed). Effecting workflow: isolated authorized rehearsal and
   artifact assertions instead; the simulated golden plane refuses effects.
6. Spend cap on the run line (`--max-cost-usd`).
7. Trace store known (`.nika/traces/`) — export wired if anyone
   watches dashboards.
8. For a run someone will audit: the workflow signed (`nika sign` binds
   the file to its author), the journal sealed (only a run key from
   `nika key init` seals a run; `nika sign` never does), verified to its
   highest honest tier (`nika trace verify`), packed
   (`nika trace evidence`).

## Completion

For the requested operational change, validate the final file and affected
integration, preserving unrelated settings. Complete an already authorized
rehearsal or rollout and inspect its artifacts; otherwise return the checked
configuration and precise remaining decision. Report which readiness checks,
effect assertions and evidence tiers actually passed. A production checklist
is conditional on the workflow's features, not a demand to configure every
capability for a narrow model or budget change.
