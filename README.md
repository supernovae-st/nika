<p align="center">
  <a href="https://nika.sh">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="https://nika.sh/brand/nika-logo-dark.svg">
      <img src="https://nika.sh/brand/nika-logo-light.svg" alt="Nika" width="220">
    </picture>
  </a>
</p>

# Nika

> **Intent as Code.** Repeatable AI work as a file you can check, run, diff and share.

[![npm](https://img.shields.io/npm/v/@supernovae-st/nika-client?label=npm)](https://www.npmjs.com/package/@supernovae-st/nika-client)
[![Release](https://img.shields.io/github/v/release/supernovae-st/nika?label=release)](https://github.com/supernovae-st/nika/releases/latest)
[![CI](https://github.com/supernovae-st/nika/actions/workflows/diamond-ci.yml/badge.svg?branch=main)](https://github.com/supernovae-st/nika/actions/workflows/diamond-ci.yml)
[![License](https://img.shields.io/badge/engine-AGPL--3.0--or--later-blue.svg)](LICENSE)
[![Spec](https://img.shields.io/badge/spec-Apache--2.0-brightgreen.svg)](https://github.com/supernovae-st/nika-spec)
[![OpenSSF Scorecard](https://api.scorecard.dev/projects/github.com/supernovae-st/nika/badge)](https://scorecard.dev/viewer/?uri=github.com/supernovae-st/nika)
[![SWH](https://archive.softwareheritage.org/badge/origin/https://github.com/supernovae-st/nika/)](https://archive.softwareheritage.org/browse/origin/?origin_url=https://github.com/supernovae-st/nika)

**Do the same AI task twice? Make it a workflow.** A `.nika.yaml` file
declares model calls, tools and commands as a graph. Nika audits that file
before a token is spent (cost ceiling · permissions · secret flows · types),
runs it on the model you choose, local first, and leaves every run a
hash-chained receipt you can verify later. One binary, four verbs, an open
spec.

<p align="center">
  <img src="media/gifs/dag-execution.optimized.gif" alt="A workflow is a graph, not a prompt: the YAML on the left, its real execution graph on the right" width="820">
</p>

## Install

**npm** gives you the `nika` command and the TypeScript SDK in one package.
The verified release binary for your platform arrives as an optional
dependency (macOS and Linux · arm64 and x64).

```sh
npm install -g @supernovae-st/nika-client
nika --version
```

Project-local works the same way: `npm install @supernovae-st/nika-client`,
then `npx nika …` and `import { Nika } from '@supernovae-st/nika-client'`.

Prefer another door? Every path installs the same verified binary.

| Door | Command |
|---|---|
| Homebrew (macOS · Linux) | `brew install supernovae-st/tap/nika` |
| Shell | `curl -LsSf https://nika.sh/install.sh \| sh` |
| cargo | `cargo binstall --git https://github.com/supernovae-st/nika nika-cli` (prebuilt, no compile · symlink `nika-cli` as `nika`) |
| nix | `nix run github:supernovae-st/nika` |
| docker | `docker run --rm -v "$PWD:/work" -w /work ghcr.io/supernovae-st/nika check hello.nika.yaml` |
| VS Code · Cursor · Windsurf | the [`supernovae.nika-lang`](https://marketplace.visualstudio.com/items?itemName=supernovae.nika-lang) extension fetches the matching engine on first use |

Air-gapped: the platform tarball and `SHA256SUMS` on the
[latest release](https://github.com/supernovae-st/nika/releases/latest),
`sha256sum -c SHA256SUMS --ignore-missing`, then `nika` onto your `PATH`.
Every path, step by step: [nika.sh/install](https://nika.sh/install).

## Sixty seconds to a first run

```sh
nika try 01-hello                              # a rehearsal · no key, no model server, nothing spent
nika new hello my-first.nika.yaml               # a real file, ingredients included
nika check my-first.nika.yaml                   # the audit · plan, cost, permits, secrets, types
nika run my-first.nika.yaml --model mock/echo --max-cost-usd 0.01   # offline, capped
nika run my-first.nika.yaml --model ollama/qwen3.5:4b               # got Ollama? real and local
```

Every run writes its journal under `.nika/traces/`; `nika trace verify`
recomputes the chain and reports how far the proof reaches. `nika` alone on
a terminal opens the concierge card; `nika welcome` says what this machine
can drive, offline.

## What a workflow looks like

```yaml
# review.nika.yaml: read a PR diff, judge its risk, comment only when it is high.
nika: pr-risk-review
model: ollama/qwen3.5:9b             # local by default · swap to any provider

permits:                              # the blast radius, declared in the file
  exec: ["git"]
  tools: ["mcp:github/pr-comment"]

tasks:
  diff:                               # exec · a read-only shell command
    exec:
      command: ["git", "diff", "origin/main...HEAD"]
      capture: structured

  assess:                             # infer · a structured model judgment
    with:                             # the binding IS the edge · no separate wiring
      patch: ${{ tasks.diff.output.stdout }}
    infer:
      prompt: "Risk-assess this diff (secrets, breaking changes, missing tests). Be terse.\n${{ with.patch }}"
      max_tokens: 300
      schema:
        type: object
        required: [risk]
        properties:
          risk: { type: string, enum: [low, medium, high] }

  comment:                            # invoke · the only write, gated on the verdict
    with:
      risk: ${{ tasks.assess.output.risk }}
      verdict: ${{ tasks.assess.output }}
    when: ${{ with.risk == 'high' }}
    invoke:
      tool: "mcp:github/pr-comment"
      args:
        body: ${{ with.verdict }}
```

Independent tasks run in parallel, joins wait for every branch, and the
whole plan is known, costed and audited before execution starts.

## The audit, before a token is spent

`nika check` reads the file and names every fix, with its `NIKA-XXX` code and
the exact source span:

| The mistake | What `nika check` says |
|---|---|
| A reference to a task that does not exist | `NIKA-VAR-001` · unresolved reference `tasks.digets` · *did you mean `tasks.digest`?* |
| Reading another task from a verb body | `NIKA-VAR-021` · hoist it into `with:` and read `${{ with.stats }}` |
| A typo in a tool name | `nika:wrte` is not a canonical builtin · *did you mean `nika:write`?* |
| A task reaching beyond its `permits:` | the escape, with the exact line that widens the boundary |
| A secret flowing where it should not | the information-flow escape, statically |
| Unbounded spend | a hard ceiling when priceable, an honest `UNBOUNDED` when not · never a fake `$0` |
| An output used where its shape cannot fit | every deep reference checked against the declared schema |

Then the run streams live, and the receipt is a hash-chained journal
`nika trace verify` re-proves. Tamper-evident, local, zero services. The
journal keeps task outputs verbatim so a run can be replayed, so
`.nika/traces/` inherits the sensitivity of whatever the workflow read: on a
shared or CI machine it is a second data-at-rest surface, to be treated like
the files the run opened.

<p align="center">
  <img src="media/gifs/full-loop.optimized.gif" alt="nika check catches a typo'd task reference and a typo'd tool, each with a did-you-mean fix; the run then streams and seals its trace" width="820">
</p>

## Four verbs, one envelope

| Verb | What it does |
|---|---|
| `infer` | Call a model · any provider, local or hosted · structured output with `schema:` |
| `exec` | Run a command · argv by default, no shell unless you ask |
| `invoke` | Call a builtin (`nika:read`, `nika:fetch`, `nika:write`, `nika:decide` …), an MCP tool, or another workflow |
| `agent` | A governed multi-turn loop with tools, a turn cap and a token cap |

Every file sits under one nine-key envelope · `nika` · `model` · `inputs` ·
`const` · `secrets` · `permits` · `run` · `tasks` · `outputs` · and the
envelope is frozen: a workflow written today keeps running. Three properties
hold across every workflow:

- **Local first, provider-agnostic.** Ollama, LM Studio, llama.cpp, vLLM, or
  any API; the file does not change when the model does.
- **Safe by construction.** An absent `permits:` block is zero authority.
  Every effect is declared, every secret's path is traced, a lethal trifecta
  (private read + untrusted input + external write) needs a human gate.
- **Reproducible.** The file and its journal are an auditable, replayable
  record. `nika test` pins a run's outputs as a golden.

## Bring your own model

```sh
nika catalog                 # every provider and model this binary speaks, with prices
nika doctor                  # which keys and local servers this machine actually has
nika run brief.nika.yaml --model ollama/qwen3.5:4b          # local
nika run brief.nika.yaml --model mistral/mistral-small-latest   # a key in the environment
nika run brief.nika.yaml --model mock/echo                    # a rehearsal · nothing spent
```

Local servers first (Ollama · LM Studio · llama.cpp · LocalAI · vLLM), then
Mistral, Hugging Face, OpenAI, xAI, Anthropic, Gemini, DeepSeek, Groq,
OpenRouter, NVIDIA and more. `--max-cost-usd` caps a run before it starts; an
unpriced model never reads as free. Already paying for a coding agent? A
signed-in `codex`, `gemini-cli`, `qwen-code` or `kimi-code` seat drives a
workflow with `--access`, on the plan you already have.

## Use it from code

The same package you installed is the SDK. It transports the engine's facts
and never parses YAML or reconstructs proof in TypeScript.

```ts
import { Nika } from '@supernovae-st/nika-client';

const nika = new Nika({ cwd: process.cwd() });

const report = await nika.check('my-first.nika.yaml', { nativeStrict: true });
if (!report.clean) throw new Error('the workflow did not pass nika check');

const run = await nika.run('my-first.nika.yaml', { maxCostUsd: 0.25 });
for await (const event of nika.events(run)) console.log(event.kind);

const result = await run.done;             // the sole terminal result
if (result.status !== 'succeeded') process.exit(1);
console.log(result.outputs, result.receipt.trace_path);
```

The same client talks to a remote `nika serve` over HTTP with a bearer token.
The full contract lives in the
[SDK README](https://github.com/supernovae-st/nika-client#readme).

## In your editor, with your agents

- **Extension** · [`supernovae.nika-lang`](https://marketplace.visualstudio.com/items?itemName=supernovae.nika-lang)
  for VS Code, and on [Open VSX](https://open-vsx.org/extension/supernovae/nika-lang)
  for Cursor, Windsurf and VSCodium: diagnostics as you type, the live
  execution graph, one-key runs. Any other editor: `nika lsp` speaks LSP over
  stdio.
- **Agents** · `nika init` drops the schema wiring, an `AGENTS.md`, a Cursor
  rule and a repo-level [agent skill](https://agentskills.io) so Claude Code,
  Cursor, Codex and friends author valid workflows on the first try.
  `nika wire <cursor|vscode|windsurf|claude|codex|zed|all>` points each
  client's MCP config at the engine. `nika mcp` is a read-only oracle any MCP
  client can call.
- **Plugins** · one plugin for three ecosystems: the authoring skill, a
  `nika-author` subagent, three commands, a check-on-edit hook and the oracle.

```sh
codex plugin marketplace add supernovae-st/nika-plugins && codex plugin add nika@nika
claude plugin marketplace add supernovae-st/nika-plugins && claude plugin install nika@nika
# Cursor: search "nika" in Settings → Plugins · one Add installs the bundle
```

## Why Nika

The closest analogues are not products, they are standards: SQL, the
Dockerfile. A portable specification with a reference engine. As agents start
acting on the real world, the place where they act cannot be free text (too
vague) or raw code (too risky). It has to be a verifiable action language:
one an AI writes, a human reviews, and a machine runs deterministically,
kept open and sovereign rather than locked inside one vendor's cloud.

What no other workflow tool offers together: a single Rust binary · portable
declarative YAML · local first · an audit before the spend · declared
permissions with secret-flow tracing · a hash-chained receipt · AGPL · no cloud
required · bring your own model.

Coming from Airflow, Dagster, LangGraph, Temporal or GitHub Actions? The docs
keep an honest [how Nika compares](https://docs.nika.sh/concepts/how-nika-compares)
page, including when *not* to use it.

## Learn more

- **Docs** · [docs.nika.sh](https://docs.nika.sh) · installation, the first
  workflow, the language reference, every error code, cost and energy honesty.
- **Examples** · [`examples/`](examples/) in this repo, and `nika try` shows the
  same shelf from the binary: eighteen path steps and thirty-five jobs, each
  runnable offline.
- **Templates** · `nika new '?'` lists the fourteen skeletons; `nika new "describe the job"`
  routes plain words to the closest one.
- **The spec** · [nika-spec](https://github.com/supernovae-st/nika-spec) (Apache-2.0),
  the law the engine follows, with its conformance corpus and its
  machine-verified [timeline](https://nika.sh/timeline).
- **Send us a workflow** · repeat an AI task every week? Describe it at
  [nika.sh/convert](https://nika.sh/convert) or open a
  [« convert my workflow » issue](https://github.com/supernovae-st/nika/issues/new/choose);
  the best ones become runnable examples, credited to you.
- **Everything around the engine** · the spec, the docs, the extension, the
  plugins, the SDK, the registry, the GitHub Action, the Homebrew tap: one map
  at [docs.nika.sh/integrations/everywhere](https://docs.nika.sh/integrations/everywhere).

**Building Nika?** The engine is a strict Rust workspace: context-window-sized
crates, a per-crate admission checklist, zero `.unwrap()` in `src/`
(CI-enforced), downward-only layering. Start at
[`docs/architecture/`](docs/architecture/), the decisions in
[`docs/adr/`](docs/adr/README.md) and the roadmap in [`ROADMAP.md`](ROADMAP.md).

## Status

Nika is built in the open. The **language** (the nine-key envelope and its
four verbs) is stable and will not break. The **engine** is a strict, modular
Rust workspace. The latest tagged release is whatever the badge at the top of
this page says, always [the releases page](https://github.com/supernovae-st/nika/releases/latest),
never a number typed here. `main` moves to the next `-dev` version right after
each release. The 1.0.0 launch is gated by the release checklist, not by a
date.

## License

The **engine** is AGPL-3.0-or-later (see [`LICENSE`](LICENSE)): modify it and
run it as a hosted service, and users of that service get the source. The
**spec** is [Apache-2.0](https://github.com/supernovae-st/nika-spec), maximally
permissive for a standard.

A commercial license (Grafana model) is available for organizations that
cannot accept AGPL's network clause. Contact `contact@supernovae.studio`.
Security reports: `security@supernovae.studio`.

---

© 2024–2026 [SuperNovae Studio](https://supernovae.studio) · 🦋 Nika, the
butterfly on the SuperNovae flag. *Prompt once. Run forever.*
