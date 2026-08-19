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
6. **`--resume` + `for_each` infer.** Item records often journal
   without a usable resume key. A later `--from <downstream>` after a
   file edit still re-runs the paid infer wave. Do not treat resume as
   a cost save on `for_each` infer until item keys exist (open).
7. **A failed last assert quarantines writes.** `out/` artifacts from
   a red `nika:assert` land in `.nika/quarantine/<trace>/`. Look there
   before assuming the write never happened.

## Authoring order that would have been cheaper

1. Check `--native-strict`.
2. Probe every new builtin with `mock/echo` (one-task file) *before*
   wiring it after a paid infer.
3. Fix the extract schema type (integer vs string) before adding
   retry passes, anyOf, or more infer.
4. Never put a README in the input glob.
5. After a file change, do not `--resume` a `for_each` infer expecting
   a cache hit.

## Shipped in this arc

- `nika-builtin` · hash accepts structured `content:` · validate
  parses string schemas
- `nika-verb-infer` · scalar `anyOf` flattens into coerce
- `nika-check` · hints `digit-string-enum` · `inspect-unwired`

## Still open

- Per-item resume keys for `for_each` infer (the paid-replay class)
- Runtime injection of `WorkflowIntrospect` into `nika:inspect`
- Check-time type of `nika:hash` `content:` when the binding is an
  object-shaped task output (the runtime now accepts it)
