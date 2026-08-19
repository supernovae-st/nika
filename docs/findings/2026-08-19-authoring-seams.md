# Authoring seams · 2026-08-19

Empirical class from a real OpenAI `nika run` of a 40+ task workflow
(structured extract → jq law → builtins). No product names. The
failures are engine/authoring seams, not domain bugs.

## What burned tokens

1. **String enum of digits.** `type: string, enum: ["0","1","3"]`.
   Models emit JSON `3`. Provider constrained decoding can reject the
   call *before* Nika's coerce stringifies. The task errors, `on_error:
   recover` swallows it, the scorer sees `null` → the wrong default.
   Prefer `type: integer`. `nika check` now hints `digit-string-enum`.
2. **`anyOf` fence vs coerce.** Authors wrote `anyOf: [integer, string]`
   to accept both forms. Coerce treated any `anyOf` as opaque and
   skipped *every* repair on that node. Scalar anyOf now flattens.
3. **`nika:hash` `content:` string-only.** Interpolating a task object
   (a roster) refused `content: (string) is required`. Hash now
   serializes non-strings as compact JSON. Strings stay raw.
4. **`nika:validate` `schema:` from `nika:read`.** A `.json` file is a
   string. `validator_for` then said the schema "is not of types
   boolean, object". String schemas are parsed as JSON, then YAML.
5. **`nika:inspect` is catalogued, unwired.** Every view returns
   `{ available: false }` (ADR-088: dispatcher composed before the
   run, no `WorkflowIntrospect`). `nika check` now hints
   `inspect-unwired`. Read the trace for cost/DAG.
6. **`--resume` + `for_each` infer.** A body that navigates
   `item.field` (`item.stem` · `item.text`) used to drop the stamp
   (`None` — the string stand-in cannot satisfy CEL `.field`). A later
   `--from <downstream>` then re-ran the whole paid infer wave. The
   collection is now the input identity; the stand-in is *shaped* from
   it so `item.field` stays eligible. Resume still skips the **whole
   fan** (one task), not a single iteration — a mid-wave crash still
   replays every item (ADR-099 per-iteration remains open).
7. **A failed last assert quarantines writes.** `out/` artifacts from
   a red `nika:assert` land in `.nika/quarantine/<trace>/`. Look there
   before assuming the write never happened. `nika check` hints
   `assert-quarantine`.
8. **A markdown glob eats README.** `held/*.md` also matches
   `held/README.md`. The extract infer then classifies the table of
   contents. `nika check` hints `glob-readme` unless `exclude`
   mentions README.
9. **jq `. as $c` then bare `map(`.** After `. as $c`, `map(...)`
   maps the *current* value (often a `[cards, receipt]` pair), not
   `$c`. Write `($c | map(...))`. `nika check` hints `jq-as-map`.

## Authoring order that would have been cheaper

1. Check `--native-strict`.
2. Probe every new builtin with `mock/echo` (one-task file) *before*
   wiring it after a paid infer.
3. Fix the extract schema type (integer vs string) before adding
   retry passes, anyOf, or more infer.
4. Never put a README in the input glob (`exclude: "**/README.md"`).
5. After a file change, `--resume` now cache-hits a `for_each` infer
   whose *collection and definition* did not change (including
   `item.field` prompts). A definition edit of the infer itself still
   re-runs — that is the law.

## Shipped in this arc

- `nika-builtin` · hash accepts structured `content:` · validate
  parses string schemas
- `nika-verb-infer` · scalar `anyOf` flattens into coerce
- `nika-check` · hints `digit-string-enum` · `inspect-unwired` ·
  `glob-readme` · `jq-as-map` · `assert-quarantine`
- `nika-runtime` · `for_each` + `item.field` is resume-eligible
  (collection is the input identity · shaped stand-in)

## Follow-up 2026-08-19

- `nika-builtin` ToolDef · `nika:hash` `content:` is no longer
  `type: string`. An object-shaped task output hashes as compact JSON;
  check/tools no longer teach `| tojson`.
- `nika:inspect` still returns `{ available: false }`. Wiring is not a
  builtin-only injection (see next patch).

## Still open

- Per-*iteration* resume keys (ADR-099: a mid-wave crash still
  replays every item; the task-level stamp only skips the whole fan)
- Runtime injection of `WorkflowIntrospect` into `nika:inspect`

## Next patch · WorkflowIntrospect

`BuiltinDispatcher` is composed once before `Runtime::run` and shared
(`Arc`) across concurrent tasks. Live DAG, settling `records`, and
running cost exist only inside the settle pass. Next honest patch:
a `RunState` cell (`Arc<Mutex<…>>` or `RwLock`) the runtime writes as
tasks settle, inject that as `W: WorkflowIntrospect` at composition,
drop `NoWorkflow` from `ProdDispatcher`, retire `inspect-unwired`.
Do not re-compose the dispatcher mid-run and do not serve zeros.
