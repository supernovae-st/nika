<p align="center">
  <a href="https://nika.sh">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="https://nika.sh/brand/nika-logo-dark.svg">
      <img src="https://nika.sh/brand/nika-logo-light.svg" alt="Nika" width="220">
    </picture>
  </a>
</p>

<h1 align="center">Repeat useful AI work. Keep the plan.</h1>

<p align="center">
  Nika turns repeatable AI work into files you can inspect, run and share.<br>
  Your instructions, tools and rules stay in a readable <code>.nika.yaml</code> file.
</p>

<p align="center">
  <a href="https://github.com/supernovae-st/nika/releases/latest"><img src="https://img.shields.io/github/v/release/supernovae-st/nika?label=release" alt="Latest release"></a>
  <a href="https://github.com/supernovae-st/nika/actions/workflows/diamond-ci.yml"><img src="https://github.com/supernovae-st/nika/actions/workflows/diamond-ci.yml/badge.svg?branch=main" alt="CI status"></a>
  <a href="https://github.com/supernovae-st/nika-spec"><img src="https://img.shields.io/badge/spec-open-8b8cf8.svg" alt="Open specification"></a>
</p>

## See the idea in one minute

“Help more customers finish checkout. Read our feedback and sales data, compare
three competitors, propose an improvement, then ask me before sharing it.”

The plan gathers CSV, Markdown and Linear context, processes three competitors
in parallel, and waits for approval. The result is a saved brief, a GitHub issue,
Telegram and Slack updates, and an updated Linear issue.

<p align="center">
  <a href="https://github.com/supernovae-st/nika/raw/refs/heads/main/media/videos/intent-to-impact.mp4">
    <img src="media/gifs/intent-to-impact.optimized.gif" alt="One intention becomes reviewable YAML, a checked graph, a bounded parallel run and five concrete results after approval" width="960">
  </a>
</p>

[Watch or download the 60-second MP4](https://github.com/supernovae-st/nika/raw/refs/heads/main/media/videos/intent-to-impact.mp4).

*Illustrative product film, not a recording of the CLI. Fictional data and
integrations are shown; no live messages are sent. The current starting point
is a terminal and a workflow file, not a visual drag-and-drop editor.*

## Start here

**Turn your meeting notes into action items with owners and deadlines.**
Use the AI access you already have. Get a real file you can review and import,
not an echo or a simulated answer.

**1. Install Nika** on macOS or Linux:

```sh
curl -LsSf https://nika.sh/install.sh | sh
```

**2. Take the ready-to-use meeting workflow:**

```sh
mkdir meeting-follow-up
cd meeting-follow-up
nika new meeting-actions
```

**3. Give it your notes.** Open `examples/fixtures/meeting-transcript.txt` in
your editor and replace the sample with your meeting transcript. Keeping that
filename means the workflow's read permission already matches. You do not need
to learn YAML before getting a result.

**4. Check and run with your existing OpenAI API access:**

```sh
nika check --model openai/gpt-4.1-mini
nika run --model openai/gpt-4.1-mini --access api
```

This uses `OPENAI_API_KEY` from your terminal environment. It makes a real,
billable model call and sends the transcript to your selected provider. Use
notes you are allowed to process there. No credentials go in the workflow.

**Your result is `out/action-items.json`:** one entry per commitment, with an
owner, a task and a deadline when one was stated. Open that file. Review the
extraction before acting on it; a valid JSON shape does not guarantee accuracy.

Next meeting: replace the transcript and run the same command again. The
workflow stays yours; no prompt chain to rebuild and no manual copy-and-paste
between the model's answer and the output file.

<details>
<summary><strong>Already signed into Codex instead of using an API key?</strong></summary>

Keep the same workflow and transcript. Use a model available on your Codex seat:

```sh
nika check --model openai/gpt-5.2
nika run --model openai/gpt-5.2 --access codex
```

The run uses the installed, signed-in Codex CLI and its subscription quota.
It does not require an OpenAI API key. If this model is unavailable on your
account, select one your seat supports in both commands.

`nika doctor` reports detected access and setup guidance. Detection does not
prove that a login is valid; the run checks that. Harness support is task-specific:
this structured `infer` example uses the Codex adapter, not an interchangeable
promise for every agent CLI. A refused access path never silently becomes a mock.

</details>

## Make it yours

Open `meeting-actions.nika.yaml` when you want to change what gets extracted.
Its four steps read the transcript, ask the model for structured action items,
save the JSON and report the output path. Review a diff, keep a history in Git,
or send a teammate the file.

Other API providers and local models use the same workflow with a different
`--model`. See the [model setup documentation](https://docs.nika.sh).

For a new job, run `nika new` in a terminal and follow the guided questions.
It selects a starting example or template; it does not magically implement
every arbitrary request. Review the file, fill any marked slots, configure
the needed tools, then follow its printed check and run commands.

## Why keep the plan in a file?

- **Understand it before running it.** Task references define the dependency
  graph, also called a DAG. Independent steps can run in parallel.
- **Keep AI inside explicit boundaries.** Declare allowed access, output shapes,
  concurrency and approval gates. Checks report what they cover and what must
  be decided at runtime; a green check does not guarantee that AI content is true.
- **Improve it with your team.** Share the procedure, review changes and run a
  version again. The plan can stay the same while external data or AI answers change.
- **Keep a record.** Runs leave a journal under `.nika/traces/`. Verification
  checks record integrity, not the truth of an AI answer.

This is **Intent as Code**: the contract is the plan, not a disposable chat.

<details>
<summary><strong>The four building blocks</strong></summary>

| Verb | What it does |
|---|---|
| `infer` | Ask a model to produce an answer |
| `invoke` | Call a native tool, MCP tool or another workflow |
| `exec` | Run a command |
| `agent` | Let a model use allowed tools for a bounded number of turns |

A workflow only needs the verbs its job requires. The film uses `invoke`
and `infer`; it does not add a shell or an agent loop just to fill the diagram.

</details>

<details>
<summary><strong>Other installation options</strong></summary>

With Homebrew: `brew install supernovae-st/tap/nika`.

See the [install guide](https://nika.sh/install) or download a
[release archive](https://github.com/supernovae-st/nika/releases/latest).
Windows users can use WSL2; native Windows binaries are not shipped yet.

</details>

<details>
<summary><strong>Before sharing a workflow or a run</strong></summary>

Share the workflow, not credentials or private data. Review file paths and
tool access. Keep secrets out of source control.

Run journals can contain the data and outputs the workflow processed.
The `.nika/traces/` directory is a data-at-rest surface: it inherits the sensitivity
of everything the run read. Exclude it from Git unless you deliberately intend
to publish those records. Treat them with the same care as their source data.

After a run, `nika trace verify` verifies the latest journal. Compare its head
with the one printed by the run. This checks the record, not the AI's judgement.

</details>

## Go further

[Examples](examples/README.md) ·
[Documentation](https://docs.nika.sh) ·
[Editor extension](https://marketplace.visualstudio.com/items?itemName=supernovae.nika-lang) ·
[Open specification](https://github.com/supernovae-st/nika-spec) ·
[Roadmap](https://github.com/orgs/supernovae-st/projects/3)

Nika is usable today and pre-1.0. The engine is
[AGPL-3.0-or-later](LICENSE); the specification is Apache-2.0.
