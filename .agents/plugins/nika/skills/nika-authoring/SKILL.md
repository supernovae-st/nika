---
name: nika-authoring
description: Author, check and repair .nika.yaml workflows and static NIKA findings. Use migration for existing automation ports and debugging for run failures.
---

# Authoring Nika workflows

Deliver a workflow that expresses the requested inputs, outputs and effects,
with a clean check of the exact final file. Continue through diagnosis and
repair within the user's scope; include execution and artifact inspection when
requested and authorized. If blocked, return the artifact and concrete finding,
not a claim of readiness. These instructions apply across models and clients;
installed engine capabilities and host tools determine what is available.

## Start from the task

For a small edit, inspect the named file and affected dependencies. For a new
job, `nika list` lists candidates; it does not certify them. Use
`nika explain <candidate>` to understand an existing owner before creating a
replacement. `nika try` and `nika new '?'` expose examples and templates when
an unfamiliar structure needs one. Read the relevant example, not a fixed quota.
Use `nika --version` and the installed schema/catalog for exact names; preserve
the user's model and do not invent an identifier from a marketing name.

## Read the reference that resolves the uncertainty

| Task | Reference |
|---|---|
| Find a local owner or example for a new graph | [Workspace and examples](references/workspace-and-examples.md) |
| Envelope, values, modifiers, verbs, permits or CEL syntax | [Language](references/language.md) |
| Check findings, execution, rehearsal, traces or cost reporting | [Validation and execution](references/validation-and-execution.md) |
| Choose native tools, retain an exec, build JSON or prove deterministic rules | [Native tools and artifacts](references/native-and-artifacts.md) |
| Paid inference, extraction schemas or paid-readiness blockers | [Paid inference](references/paid-inference.md) |
| Parent/child calls, containment, child outputs or budgets | [Composition](references/composition.md) |

Consult only the needed references. When loaded as a Nika runtime skill, linked
files are not automatically injected: the host must supply an authorized read
tool and read grants, or explicitly supply the required files as context.

## Shared contract

- Envelope: `nika: <id>` and a `tasks:` map, exactly one of `infer`, `exec`,
  `invoke`, `agent` per task. Use `nika spec --schema` and `nika catalog --tools`.
- Values belong in `inputs:`, `const:`, `secrets:`. Bind task data with `with:`;
  `after:` orders without data. Secrets use declared stores and `egress:` sinks.
- `permits:` bounds intended effects. Missing grants mean zero authority
  (`NIKA-AUTH-006`); pure compute declares `permits: {}`. Review
  `nika check <file> --infer-permits` proposals against the requested scope.
  Do not widen grants merely to silence findings. For `lift:` (`law: taint` or
  `law: data-as-code`), read the language reference; a lift grants no permit.
- `nika check <file> --json --native-strict` validates the final candidate.
  Inspect the exit status, `clean`, `native_strict_clean`, composition coverage
  and, before paid inference, `paid_ready`. `--fix` applies mechanical repairs;
  review its diff. `nika explain NIKA-XXXX` teaches an unresolved finding.
- A check is evidence, not permission or proof of effects. Preserve existing
  authorization, caps and human gates. `mock/echo` simulates envelope inference;
  task model pins, tools, writes and network effects remain real.
- The estimate covers output tokens; unknown input billing or local compute
  remains unpriced. Metered caps can overshoot for already admitted calls.

## Finish at the requested outcome

For authoring, report the file, actual check verdict, effect scope and remaining
blockers. For an authorized run, retain its session and exact trace, inspect
outputs against the task's expected result, and report the exit status and
verified evidence tier. Reconcile partial effects before any retry. A golden
is suitable only for the simulated plane that supports the workflow; a green
static check alone proves neither behavior nor parity.
