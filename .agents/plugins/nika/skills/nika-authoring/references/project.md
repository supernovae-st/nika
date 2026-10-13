# Project

Read this reference for the matching task; return to [the skill](../SKILL.md) for scope and completion.

## What a project holds

The unit of work is the project. An optional `nika.yaml` at its root is the project file: its
name (`nika:`), a default per-run ceiling (`ceiling:`), the beats (`arm:`), and the trace and
registry policy (`traces:`, `registry:`). Its grammar is closed: `nika spec --schema --project`
prints it, and a key outside it is refused (`project.unknown-key`). A document without
`tasks:` is read as a project, never as a workflow.

Beside it:

- **Workflows**, one job per `.nika` file, each with its own `permits:`.
- **Supporting files** the workflows read: data, prompt texts, templates. A path written in a
  prompt is only text: a model sees a file's content when a task reads it (`nika:read` under a
  declared `fs.read`) and binds that output into the prompt.
- **Child workflows**, called with `invoke: { workflow: "./child.nika" }`
  ([composition](composition.md)).
- **`.nika/`**, the engine's runtime state (traces, firing ledgers). It is never source.

## Two path bases

- A child's `workflow:` path resolves from the parent file's directory.
- Every builtin path (`nika:read`, `nika:write` …) and every `fs` permit resolves from the
  directory the command starts in. Check, run and arm from the project root. Checked from a
  subdirectory, a workflow that reads `./data/in.txt` gets the hint « reads `./data/in.txt`
  which does not exist here », and a run started there fails `NIKA-BUILTIN-READ-001`.

## Children

- The parent's `permits:` must contain every effect of every child (`NIKA-COMP-002`).
- `--model` on the parent's run line does not reach a child that pins its own `model:`. That
  child runs its own model, or fails `NIKA-1800` when its model has no ready access path on the
  machine. Change the child's pin explicitly when the person asks for another model.
- A child input is either required (`required: true` and no `default:`, so the parent must pass
  it) or optional (`required: false` with a `default:`). Both at once checks clean and teaches a
  contradiction: choose one.

## Beats: zone, ceiling, missed slots, activation

A recurring request becomes a beat in `nika.yaml`: the workflow, the cadence with its IANA zone
inside (`TZ=Europe/Paris 0 9 * * 1`), the ceiling of one firing (`plafond:`, in USD) and what a
missed slot becomes (`manqué:`). Neither `plafond` nor `manqué` has a default.

- Take the zone, the ceiling and the missed-slot rule from what the person already gave: the
  request, an earlier answer, the project (another beat's zone, the per-run `ceiling:`), or a
  choice they delegated to you (then choose, and say what you chose). Ask only for one still
  missing, all of them in one question, saying what each choice changes. Never write a value
  the person neither gave nor delegated.
- Write the `manqué` value the targeted binary fires; `nika arm --json` on that binary is the
  judge. 0.122.0 and the 0.123 previews fire `sauter` and `rattraper-une-fois`, read
  `rattraper` UNREADY (« not supported by this firer ») and refuse the bounded
  `{ rattraper: N }` (`project.bad-value`). Write the bounded catch-up only for a binary whose
  `nika arm --json` reads it READY.
- Saving the project, running a workflow and activating a beat are three distinct effects, and
  one scoped instruction can cover them: « save it and run it » covers the Save and that run,
  once; « … and run it every Monday » also covers activating the beat. Carry out what the
  instruction covers without asking again; an effect it does not name waits for the person.
  Writing a beat arms nothing by itself: activation is its own effect (an OS unit from
  `nika arm --emit launchd|systemd --write`, then loaded, or a resident `nika serve`). A saved
  revision of a workflow never silently changes what an active beat fires.
- `nika check nika.yaml` judges the file. `nika arm --json` reads every beat and reports READY,
  UNREADY with its `blockers`, or DORMANT (suspended); it writes nothing. An unknown alone never
  makes a beat UNREADY: a beat whose workflow calls a child reads READY and names the unknown
  « a nested workflow's routes are judged only at its own dispatch » (`nika serve --once --dry`
  says what would fire). Activate a READY beat; `nika arm --emit` refuses an UNREADY one, and a
  unit written for an earlier revision of the schedule refuses to fire until it is emitted again.

Every cadence form, the suspension triple and the activation commands are in the
[Projects and schedules](https://docs.nika.sh/guides/projects-and-schedules) guide and the
[arm reference](https://docs.nika.sh/reference/arm).

## A checked project

The project file:

```yaml nika.yaml
nika: weekly-report
ceiling: 0.50
arm:
  - workflow: workflows/weekly-report.nika
    cadence: "TZ=Europe/Paris 0 9 * * 1"
    plafond: 0.10
    manqué: rattraper-une-fois
```

The workflow reads two supporting files, calls a child and writes once:

```yaml workflows/weekly-report.nika
nika: weekly-report-run
permits:
  tools: ["nika:read", "nika:write"]
  fs:
    read: ["./prompts/report-style.txt", "./notes/week.txt"]
    write: ["./out/weekly-report.md"]
tasks:
  style:
    invoke: { tool: "nika:read", args: { path: "./prompts/report-style.txt" } }
  notes:
    invoke: { tool: "nika:read", args: { path: "./notes/week.txt" } }
  summarize:
    with:
      style: ${{ tasks.style.output }}
      notes: ${{ tasks.notes.output }}
    invoke:
      workflow: "./children/summarize.nika"
      args: { style: "${{ with.style }}", notes: "${{ with.notes }}" }
    returns: { object: { summary: string } }
  save:
    with:
      summary: ${{ tasks.summarize.output.summary }}
    invoke:
      tool: "nika:write"
      args: { path: "./out/weekly-report.md", content: "${{ with.summary }}", overwrite: false }
```

Its child, beside it in `children/`:

```yaml workflows/children/summarize.nika
nika: summarize-notes
model: ollama/qwen3.5:4b
inputs:
  notes: { type: string, required: true }
  style: { type: string, required: false, default: "Plain, five bullet points." }
permits: {}
tasks:
  brief:
    infer:
      prompt: "Summarize these notes. Style: ${{ inputs.style }}\n\n${{ inputs.notes }}"
      max_tokens: 4096
outputs:
  summary: ${{ tasks.brief.output }}
```

With `prompts/report-style.txt` and `notes/week.txt` present, the three files check clean from
the root, and `nika arm --json` reads the beat READY, naming that nested-workflow unknown.
