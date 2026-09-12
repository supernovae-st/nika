# Paid inference

Read this reference for the matching task; return to [the skill](../SKILL.md) for scope and completion.

## Paid infer (the order that is cheaper than tokens)

Apply these checks when the workflow actually uses the corresponding
construct. A small prompt edit does not require rebuilding or reproving
unaffected parts of the workflow.

1. `nika check <file> --json --native-strict` until `clean` and `paid_ready`
   are both true (zero findings, zero paid-run hints).
2. Probe an unfamiliar builtin with known-answer inputs when its behavior
   affects correctness, within isolated authorized effect boundaries, before
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
6. Proceed to authorized paid execution after the applicable checks and
   budget are ready. Keeping the intended `model:` during static checking
   spends no inference tokens; do not replace a user-selected model as a ritual.

`for_each` + `item.field` is resume-eligible as a **whole fan** when
the collection and definition did not change. A mid-wave crash still
replays every item. After `. as $c` in jq, write `($c | map(...))`
(hint `jq-as-map`). A red last `nika:assert` quarantines `out/`
(`.nika/quarantine/<trace>/` — hint `assert-quarantine`).

## Review applicable paid-inference findings

`nika check --native-strict` green means the file is *legal*. It does
not mean it is the cheapest, most native, or most honest file.
`.paid_ready` reports the paid-infer blockers the checker knows about,
not execution consent or a complete safety proof. A runnable handoff
also needs a clean native-strict check and the intended authority. Each
question has a command or a file. Do not reason from memory.

Inspect the workflow, relevant examples and `nika catalog --tools` as
needed. Use `invoke.workflow` for child workflows and `for_each` for
collections. Test deterministic rules with known answers; a schema or
an anchor substring check does not establish that extracted facts are true.

0. **Is `.paid_ready` true?** `nika check --json <file> | jq .paid_ready`.
   `false` → repair `.next` (kind · task · advice) first, then the rest
   of `.paid_blockers[]`; also inspect `.compiled` and the findings.
   Do not perform paid execution while a paid blocker remains; preserve the
   user's selected model during static validation.
1. **Is an unfamiliar shape still unresolved?** Read the relevant
   `nika try` / `nika new <slug>` example, then check the actual file.
2. **Is every `exec:` a real tool?** `nika check <file> --native-strict`. A
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
   and `nika new "the job in plain words"`. Adapt a closer shape only when it materially improves the requested result;
   preserve working content and avoid restarting for stylistic conformity.
8. **Did `nika explain <file>` stay honest?** Waves · cost (FLOOR ≠ $0)
   · touches · the **before a paid model** panel. If a paid-run hint
   remains, the file is not done.
9. **Is the law proven on known answers?** Hint `unproven-law`. A
   jq/decide that scores an infer needs a const-fixture `nika:assert`.

Before paid execution, resolve the paid blockers as well as the findings.
If a dependency or decision is unavailable, hand over the checked artifact
with its exact blocker and remaining action; do not call it ready or run it.
Explain remaining non-paid hints without treating that explanation as an
exemption from native-first admission (CONVENTIONS §10).
