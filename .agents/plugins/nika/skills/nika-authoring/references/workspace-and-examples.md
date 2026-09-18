# Workspace and examples

Read this reference for the matching task; return to [the skill](../SKILL.md) for scope and completion.

## Read the workspace and resolve unfamiliar shapes

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
continue to the embedded shelf. Read an example when its shape resolves an
uncertainty in the task; read a second only for a gap the first leaves.
A small edit to a known workflow does not require creating example files.

```
nika try                        # browse the gallery
nika compile --list             # exact authoring skeletons
nika compile <exact-skeleton> --json # preview source and stable questions
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
| run a confined program and branch on its exit status | `03-exec-pipeline` |
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
| reject an oversized batch before model calls | `18-bounded-batch` |
| validate closed records before computing totals | `19-validate-records` |
| compare finite snapshots using set semantics | `20-snapshot-diff` |
| deduplicate identical records and reject conflicting ids | `21-deduplicate-records` |
| send only allowed fields to inference | `22-project-public-fields` |
| combine independent reviews with a proven boolean rule | `23-parallel-review` |
| recover a missing optional file without hiding other errors | `24-recover-optional-file` |
| aggregate bounded integer amounts by key | `25-aggregate-by-key` |
| a job too big for one file | [composition](composition.md), then `01-hello` for the child |

The second column is pinned to the embedded gallery by an engine test.
Read those examples through MCP `nika_examples`, or rehearse an exact lesson
with `nika try <slug>` under its documented mock boundary. Gallery discovery
is not authoring: Compile accepts its own exact skeleton list and the hello lesson.

## Skeleton or filled precedent

`nika compile --list` lists exact skeleton names. Preview with
`nika compile <name> --json`; answer missing values with explicit
`--answer KEY=JSON_LITERAL`. Only a Ready candidate and an explicit destination
write a file. Unsupported prose stays incomplete without substitution.
Through MCP, `nika_template` with exact `name` and `filled: true` reads the
paired lesson; a missing pair refuses explicitly. Omit `filled` to read the
skeleton. This is read-only discovery, not a second authoring implementation.

## Bound the resource, then test the refusal

- Collection size: use a closed schema with `maxItems`, check its verdict with
  `nika:assert`, and put the fan after that assertion's success. `max_parallel`
  limits simultaneous iterations, not the number of items. A runtime guard
  need not make the checker's static cost estimate finite.
- Payload size: bound strings and record fields too. Validation after a read
  does not prevent the read from allocating a large payload first.
- Model work: cap each infer's output tokens and timeout. Check again with the
  intended model because reasoning consumes the same output-token allowance.
- Repetition: bound retry attempts; make recovery specific with `on_codes`.
  A task timeout applies per attempt. Retrying a write needs its own safe
  repetition contract; the optional-read lesson does not authorize that.
- Agent loops: declare turns, cumulative tokens and cost when relevant, and
  require explicit completion. Exhausting a budget is not completing the job.
- Test zero, exact limit, one over, wrong type and missing authority. Inspect
  traces to prove downstream work did not start after rejection. Parse the
  typed outputs and compare them to independently chosen expected values.

Use `--model mock/echo` to test the wiring without credentials. A mock success
is not an evaluation of model quality or of a real external integration.
