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
  <a href="https://www.npmjs.com/package/@supernovae-st/nika"><img src="https://img.shields.io/npm/v/@supernovae-st/nika?label=npm" alt="npm package"></a>
  <a href="https://docs.nika.sh"><img src="https://img.shields.io/badge/docs-docs.nika.sh-8b8cf8.svg" alt="Documentation"></a>
  <a href="https://github.com/supernovae-st/nika-spec"><img src="https://img.shields.io/badge/spec-open-8b8cf8.svg" alt="Open specification"></a>
</p>

<p align="center">
  <a href="https://scorecard.dev/viewer/?uri=github.com/supernovae-st/nika"><img src="https://api.scorecard.dev/projects/github.com/supernovae-st/nika/badge" alt="OpenSSF Scorecard"></a>
  <a href="https://github.com/supernovae-st/nika/actions/workflows/codeql.yml"><img src="https://github.com/supernovae-st/nika/actions/workflows/codeql.yml/badge.svg?branch=main" alt="CodeQL"></a>
  <a href="https://github.com/supernovae-st/nika/releases/latest"><img src="https://slsa.dev/images/gh-badge-level3.svg" alt="SLSA 3 provenance on every release"></a>
  <a href="https://archive.softwareheritage.org/browse/origin/?origin_url=https://github.com/supernovae-st/nika"><img src="https://archive.softwareheritage.org/badge/origin/https://github.com/supernovae-st/nika/" alt="Archived by Software Heritage"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-AGPL--3.0--or--later-blue.svg" alt="AGPL-3.0-or-later"></a>
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

**2. Create the offline hello lesson at an explicit destination:**

```sh
mkdir first-workflow
cd first-workflow
nika compile hello hello.nika.yaml
```

Compile uses the same stateless core for this lesson and exact skeletons. Hello
always uses `mock/echo`, including when provider keys are present. It does not
call a model, run the workflow, or choose access on your behalf.

**3. Check and run the file:**

```sh
nika check hello.nika.yaml
nika run hello.nika.yaml
```

The greeting is a mock echo: this proves the workflow runs without a provider.
The file stays yours to inspect, review in Git, and share. Run records its local
trace under `.nika/traces/`; creation adds that directory to `.gitignore`.

## Make it yours

`nika compile --list` lists exact skeletons. Preview one with
`nika compile <slug> --json`, then answer its stable questions with repeatable
`--answer KEY=JSON_LITERAL`. An incomplete result includes its candidate and
questions; it does not write a file. Only a Ready candidate plus an explicit
destination writes. Existing destinations require `--force`.

For an accepted source, a conservative constant edit uses the same core:

```sh
nika compile --base workflow.nika.yaml --change 'Set const.topic to "new topic"' --json
```

Add `--output edited.nika.yaml` to materialize a Ready edit. The base source
remains explicit. Unsupported natural language stays incomplete without a
substitute workflow. Full natural-language authoring and Graph editing are not
implemented by this bounded CLI.

Compile's Check preview judges source only. `nika check` and `nika run` judge
the actual environment separately. For a real model, choose the provider and
access explicitly; [model setup documentation](https://docs.nika.sh) explains
local, API and supported harness choices. `nika try` remains the example gallery.

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

From npm, the same binary plus a TypeScript client:
`npm install @supernovae-st/nika` (the `nika` command lands in
`node_modules/.bin`).

See the [install guide](https://nika.sh/install) or download a
[release archive](https://github.com/supernovae-st/nika/releases/latest);
every release ships SLSA provenance you can verify. A Nix flake is in the
repository. Windows users can use WSL2; native Windows binaries are not
shipped yet.

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

<!-- city:map -->
## The city · where this repo sits

```text
📜 nika-spec ──── language law and conformance
    │
    ▼
⚙️ nika ───────── this repo: the engine · admission, execution, receipts and schedules
    │
    ▼
🔌 doors ──────── npm @supernovae-st/nika · Homebrew · VS Code · plugins · gh nika · the CI action
    │
    ▼
🧩 your workflows
```

This repository is the one executable: it parses, admits, runs and traces
every workflow. The language is defined in the specification; the doors
consume this engine and add nothing to its authority.

All the buildings: [nika-spec](https://github.com/supernovae-st/nika-spec) ·
[nika](https://github.com/supernovae-st/nika) ·
[nika.sh](https://nika.sh) ·
[nika-docs](https://github.com/supernovae-st/nika-docs) ·
[nika-client](https://github.com/supernovae-st/nika-client) ·
[nika-vscode](https://github.com/supernovae-st/nika-vscode) ·
[nika-plugins](https://github.com/supernovae-st/nika-plugins) ·
[gh-nika](https://github.com/supernovae-st/gh-nika) ·
[homebrew-tap](https://github.com/supernovae-st/homebrew-tap) ·
[nika-action](https://github.com/supernovae-st/nika-action) ·
[nika-actions-starter](https://github.com/supernovae-st/nika-actions-starter) ·
[nika-registry](https://github.com/supernovae-st/nika-registry) ·
[nika-estate](https://github.com/supernovae-st/nika-estate).
<!-- /city:map -->

## Go further

[Examples](examples/README.md) ·
[Documentation](https://docs.nika.sh) ·
[TypeScript SDK](https://www.npmjs.com/package/@supernovae-st/nika) ·
[Editor extension](https://marketplace.visualstudio.com/items?itemName=supernovae.nika-lang) ·
[Open specification](https://github.com/supernovae-st/nika-spec) ·
[Registry](https://github.com/supernovae-st/nika-registry) ·
[Roadmap](https://github.com/orgs/supernovae-st/projects/3)

Nika is usable today and pre-1.0. The engine is
[AGPL-3.0-or-later](LICENSE); the specification is Apache-2.0.
