---
id: ADR-113
title: "The envelope's identity moves onto `nika:` — the version slot dies, losslessly"
status: accepted
date: 2026-08-13
phase: ""
deciders: ["@ThibautMelen"]
tags: [envelope, grammar, parser, flag-day, spec-alignment]
affects_crates: [nika-schema, nika-vocab, nika-check, nika-lsp, nika-migrate, nika-cli, nika-cli-host, nika-runtime, nika-display]
affects_layers: [L0, L4]
supersedes: [ADR-082]
superseded_by: []
related: [ADR-001, ADR-021, ADR-082]
requires: []
enables: []
amends: []
fci: []
inv: []
shadow_zones: []
nika_codes: ["NIKA-PARSE-003", "NIKA-PARSE-004", "NIKA-PARSE-005", "NIKA-PARSE-020", "NIKA-PARSE-021"]
timeline: ""
follow_ups:
  - "The remaining LOT 2 grammar (`lift:` · `unwind` · `for_each` block · `extract:` · `group:`) — the vendored pack cannot resync until ALL of it lands"
  - "The identity codemod (`workflow:`/`nika: v1` → one `nika: <id>`, with `description:` prose demoted, not dropped)"
  - "`nika-onboard`'s intent router indexes the `description:` line — the spec's tombstone counted one consumer and this is a second"
---

# ADR-113: The envelope's identity moves onto `nika:`

## Context

`nika-spec` executed its envelope nuke on 2026-08-12 (`d20b139` · « l enveloppe
passe de 13 cles a 9, nika porte l identite »). The engine had not followed.

The gap was invisible because the engine is judged at a pinned spec commit
(`SPEC_PIN`, consumed by `diamond-ci.yml`). At the time of writing that pin sat
**44 commits behind** `nika-spec` `main`, so CI was green against a spec that
predates the change. Pointing the conformance battery at the CURRENT spec made
the real state visible:

```
NIKA_SPEC_DIR=<spec@main> cargo nextest run -p nika-check
  → 4 suites failed · 172 diagnostic emissions · 172 of them NIKA-PARSE-003
```

Every one of 145 named fixtures died on the `nika:` line, before the engine
could reach the construct the fixture actually tests. One defect masked the
entire next layer.

## Decision

`nika:` carries BOTH the mark and the name. The KEY says *this is a Nika file*;
the VALUE is the file's kebab-case id (`^[a-z][a-z0-9-]*$`).

- `parse_nika_version` → `parse_nika_id`. The shape is judged on the NODE, not
  through `get_scalar`: a mapping or sequence would otherwise read as ABSENT and
  the id would go quietly missing instead of loudly wrong.
- `SchemaError::BadNikaVersion` → `BadNikaId`. `NIKA-PARSE-003` survives by
  changing MEANING, and its CATEGORY moves with it — `parse_error` →
  `validation_error`. Judging a version marker was the file's entry contract;
  judging a name is a spec-rule violation in well-formed input. The document
  parses; the name is wrong. (Spec fixture `envelope/003-nika-id-bad-shape`
  pins the category, and it is what caught this.)
- `workflow:` and top-level `description:` leave `TOP_LEVEL_KEYS` and refuse as
  unknown keys (`NIKA-PARSE-005`). `RawWorkflow.workflow` now holds the value of
  `nika:` — so the ~5 downstream readers of the workflow id changed by zero
  lines.
- `NIKA-PARSE-004` (`BadWorkflowId`), `NIKA-PARSE-020` (`W1WorkflowScalar`) and
  `NIKA-PARSE-021` (`W1TopLevelDescription`) are RETIRED. No retired code is
  ever reused (SSOT-2 B.22). `SchemaVersion` is deleted: a field with one legal
  value for the whole lifetime of the contract was never a version.

### W1's envelope half is retired with it

`nika-migrate`'s W1 codemod migrated `workflow: <scalar>` INTO
`workflow: { id, description }` — precisely the object the parser now refuses.
A repair whose output its own checker rejects is worse than no repair, so that
half is removed and its `--fix` ladder arms with it. **The tasks half is
untouched** (sequence → map · `- id:` → the key). The identity codemod is
follow-up work, and it must DEMOTE the `description:` prose (to a comment),
never silently drop it.

## Consequences

Measured, not asserted:

| | before | after |
|---|---|---|
| conformance emissions (spec@main) | 172 | **16** |
| `nika-check` suites failing | 4 | **3** |
| workspace `--lib` | 5757 run | **5747 pass / 4 fail** |

The 16 remaining emissions are the next layer, now visible and small:
`group:` ×7 · `lift:` ×2 · `extract:` ×1 · `for_each` block ×2 · `unwind` ×1 ·
plus two unrelated. **`group:` was not in any plan** — it arrived with spec
`450b476` and only surfaced once the envelope stopped masking it.

### The pack is all-or-nothing (the sequencing constraint this uncovered)

The 4 remaining workspace failures are all consumers of the VENDORED spec pack
(`crates/nika-pack/pack/`). `scripts/sync-pack.sh` moves whole directories with
`rsync --delete`, and the spec's pack uses `lift:` (2 files), `extract:` (9),
`max_parallel`/`fail_fast` under `for_each` (9/8). So the pack can only resync
once the engine speaks the WHOLE new grammar.

**Consequence: LOT 2's slices cannot each land green on their own.** The parser
work slices cleanly; the pack does not. This is a correction to the plan, found
by measurement.

### A second reader of `description:`

The spec's tombstone justified killing `description:` with « one consumer across
five reading surfaces ». There is a second: `nika-onboard`'s intent router
indexes the description line (`intent.rs:594`), and the routing fixtures score
against that exact wording — the query `chase unpaid invoices` IS
`invoice-chaser`'s description. When the pack resyncs, routing quality drops and
3 tests go red. Recorded here rather than patched silently.
