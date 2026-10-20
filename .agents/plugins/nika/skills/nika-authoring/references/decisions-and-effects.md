# Decisions and effects

Read this reference for the matching task; return to [the skill](../SKILL.md) for scope and completion.

## Ask, default or gate

Three different forms answer « what if this value is missing »:

| Form | Who decides, and when | Use it for |
|---|---|---|
| A question while authoring | the person, once, before the file exists | a fact that changes the result or an effect and that the person has neither given, nor kept in the project, nor delegated: a recipient, a destination, a zone, a ceiling, a missed-slot rule. Every such fact goes in one question |
| An optional `inputs:` entry with its `default:` | the person, once, for every run | a deployment value the person gave or delegated to you (say the value you chose); never a guess written down to avoid asking |
| A `nika:prompt` gate | a human, at run time, on every run | a decision taken when the run reaches it, above all before an irreversible effect |

- A fact the person gave, the project keeps or the person delegated is not an open question:
  use it, and never ask for it again.
- Never invent the answer to an open question, and never bake a guess as a `default:`.
- Headless, a gate with no `default:` pauses the run (exit 4) and names its resume line:
  `nika run <file> --resume <trace> --answer <task>=<value>` continues with the person's
  answer. Nobody else answers it for them.
- A `default:` on a gate makes it unattended. That changes the workflow's approval policy and
  needs the person's explicit choice; it is never the repair of a pause.

```yaml
nika: send-after-approval
permits:
  tools: ["nika:prompt", "nika:log"]
tasks:
  approve:
    invoke:
      tool: "nika:prompt"
      args: { message: "Send the drafted reply to the customer?", mode: confirm }
  send:
    after: { approve: success }
    invoke: { tool: "nika:log", args: { message: "sending" } }
```

## Exact identifiers

When the request names a record by its identifier (a customer, an account, a channel, an
invoice), carry that exact identifier in the file, as a `const:` or an `inputs:` value, and
select by it. Two records can share a name, so a selection by name must refuse more than one
match, never take the first. Never substitute a similar identifier, and when a component or a
template is rebound, bind the identifiers the person gave.

```yaml
nika: one-customer-by-id
const:
  customer_id: "CUST-00417"
permits:
  tools: ["nika:read", "nika:jq", "nika:assert"]
  fs:
    read: ["./data/customers.json"]
tasks:
  load:
    invoke: { tool: "nika:read", args: { path: "./data/customers.json" } }
  pick:
    with: { rows: "${{ tasks.load.output }}" }
    invoke:
      tool: "nika:jq"
      args:
        expression: ".id as $want | .rows | fromjson | map(select(.id == $want))"
        input: { rows: "${{ with.rows }}", id: "${{ const.customer_id }}" }
  one:
    with: { matches: "${{ tasks.pick.output }}" }
    invoke: { tool: "nika:jq", args: { expression: "length == 1", input: "${{ with.matches }}" } }
  exactly_one:
    with: { holds: "${{ tasks.one.output }}" }
    invoke:
      tool: "nika:assert"
      args: { condition: "${{ with.holds }}", message: "the identifier does not name exactly one record" }
```

## Read back what an effect did

A clean exit says the run settled, not that the artifact holds what it should. After an effect
that matters, read it back and assert what it must contain: read the file a task wrote, or
fetch the record it created, then `nika:assert` the expected values. After a lost response,
reconcile with the destination before any retry: the write may already have happened.

```yaml
nika: write-then-read-back
const:
  report: "Weekly total: 1250 EUR"
permits:
  tools: ["nika:write", "nika:read", "nika:jq", "nika:assert"]
  fs:
    write: ["./out/summary.txt"]
    read: ["./out/summary.txt"]
tasks:
  save:
    invoke:
      tool: "nika:write"
      args: { path: "./out/summary.txt", content: "${{ const.report }}", create_dirs: true, overwrite: false }
  read_back:
    after: { save: success }
    invoke: { tool: "nika:read", args: { path: "./out/summary.txt" } }
  same:
    with: { kept: "${{ tasks.read_back.output }}" }
    invoke:
      tool: "nika:jq"
      args: { expression: ".kept == .want", input: { kept: "${{ with.kept }}", want: "${{ const.report }}" } }
  holds:
    with: { same: "${{ tasks.same.output }}" }
    invoke:
      tool: "nika:assert"
      args: { condition: "${{ with.same }}", message: "the file does not hold the report" }
```

## When a write is refused

- `nika:write` with `overwrite: false` refuses an existing destination
  (`NIKA-BUILTIN-WRITE-002`) and leaves the file as it was. Write to a new path, or set
  `overwrite: true` only when the person asked to replace that file.
- A write outside the declared `fs.write` is refused at check and at run. Grant the exact path
  the request names, nothing wider.
- Never route a refused write through `exec:` (a shell redirect, `cp`, `tee`) or another tool:
  the refusal is the boundary working.
- A draft the person asked for is written or created as a draft. Sending it is another effect
  that needs its own request.

## Keep the model steps the request needs

A checker hint can read as « remove this model step ». It never means that when the request
asks for what the step contributes:

- `dead-spend` (« no task or output consumes `tasks.<id>.output` … consume it or remove the
  task »): consume it, by binding it into the task that uses it or into `outputs:`. Remove the
  task only when the request does not need its text.
- `infer-as-law`: the model is asked to decide a law (a score, a level, a threshold). Let it
  extract the facts and let `nika:jq` or `nika:decide` apply the law; the model keeps the
  language.
- `unproven-law`: pin the law with a `nika:assert` on known answers.

Never replace a requested model step with a constant, never delete it to quiet a hint, and
never swap its model without the person's choice.
