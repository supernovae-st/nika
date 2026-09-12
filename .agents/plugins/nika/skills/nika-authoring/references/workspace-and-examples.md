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
nika try                        # the shelf · the path, then the jobs
nika new <slug>                 # take the one matching your intent (table below) — read it
nika new <second-slug>          # optional: a different shape still needed
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
| a job too big for one file | [composition](composition.md), then `01-hello` for the child |

Second column pinned to the pack by the engine's own test (every slug
this table names resolves through `nika_pack::example`). Any slug works
with or without its `showcase/` prefix and with or without the
`.nika.yaml` extension. `nika new <slug>` makes one yours;
`nika new <name>` does the same from the template side
(`nika new '?'` prints that set).
