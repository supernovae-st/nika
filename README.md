# Nika

> **Intent as Code.** The workflow language for AI: one file, 4 verbs,
> one binary.

[![License](https://img.shields.io/badge/engine-AGPL--3.0--or--later-blue.svg)](LICENSE)
[![Spec](https://img.shields.io/badge/spec-Apache--2.0-brightgreen.svg)](https://github.com/supernovae-st/nika-spec)
[![Rust](https://img.shields.io/badge/built_in-Rust-orange.svg)](Cargo.toml)
[![CI](https://github.com/supernovae-st/nika/actions/workflows/diamond-ci.yml/badge.svg?branch=main)](https://github.com/supernovae-st/nika/actions/workflows/diamond-ci.yml)

Useful AI work shouldn't disappear into chats. **Nika turns repeatable AI
work into files you can run, review, diff and share.** If you do the same
AI task twice, make it a workflow.

A Nika workflow is just a file: readable, portable, verifiable. It runs
locally, on whichever LLM you choose, with no cloud required. The language
is an open [Apache-2.0 spec](https://github.com/supernovae-st/nika-spec);
this repo is the reference engine, a single Rust binary (AGPL-3.0). The
way SQL pairs with PostgreSQL, or the Dockerfile with Docker.

<p align="center">
  <img src="media/gifs/chat-to-workflow.optimized.gif" alt="A repeated chat prompt becomes meeting-actions.nika.yaml; nika check audits it, then a real local run (ollama/llama3.2:3b) writes typed action items" width="900" />
</p>

## Does it run today?

Yes.

```sh
brew install supernovae-st/tap/nika    # or: curl -LsSf https://nika.sh/install.sh | sh
nika examples run 01-hello --model mock/echo            # zero setup: no key, no model server
nika examples run 01-hello --model ollama/qwen3.5:4b    # got Ollama? the same run, real + local
```

![nika check audits the workflow (plan, cost, secrets, types), then nika run executes it locally](media/nika-hero.gif)

Nika audits a workflow **before a single token is spent** (plan, cost
ceiling, secret flows, types, tool args), then runs it:

```text
$ nika check brief.nika.yaml
 ✔ PLAN     2 wave(s) · 2 task(s) · max parallelism 1
 ✔ SECRETS  no information-flow escapes
 ✔ TYPES    every deep output reference fits its declared shape
 ✔ TOOLS    every nika: tool names a canonical builtin
 ✔ ARGS     every invoke arg key is declared + every required arg is present
 ✔ SCHEMA   every authored schema: is satisfiable
 ✔ clean — audited before a single token was spent

$ nika run brief.nika.yaml
  🦋 nika · daily-brief · 2 tasks
  ✔  fetch_notes  exec · cat
  ✔  brief        infer · ollama/llama3.2:3b
  ── 2/2 done · $0.000 · elapsed 16.2s ───────────────────────────
```

## What a workflow looks like

```yaml
# review.nika.yaml: read a PR diff, judge its risk, comment only when it's high.
nika: v1
workflow: pr-risk-review
model: ollama/qwen3.5:9b             # local by default. swap to any provider

tasks:
  - id: diff                          # exec: a read-only shell command
    exec:
      command: "git diff origin/main...HEAD"
      capture: structured

  - id: assess                        # infer: structured LLM judgment
    with: { patch: ${{ tasks.diff.output.stdout }} }
    infer:
      prompt: "Risk-assess this diff (secrets, breaking changes, missing tests). Be terse.\n${{ with.patch }}"
      schema:
        type: object
        required: [risk]
        properties:
          risk: { type: string, enum: [low, medium, high] }

  - id: comment                       # invoke: the only write, gated on the verdict
    when: ${{ tasks.assess.output.risk == 'high' }}
    invoke:
      tool: "mcp:github/pr-comment"
      args: { body: ${{ tasks.assess.output }} }
```

## Check before it runs

`nika check` is a static audit. It catches broken references, missing
dependencies, schema and permission problems **before any model is called**
— and when something is off, it points at the exact fix:

![nika check catches a missing depends_on and a typo'd task reference, shows the three-line fix, then the same audit passes clean](media/gifs/static-check-fix.optimized.gif)

The two fixtures behind this capture live in
[`scripts/media/fixtures/`](scripts/media/fixtures/), gated in both
directions by `scripts/media/validate-media.sh`: the broken one must keep
failing `nika check`, the fixed one must stay clean.

The same audit holds the workflow's **declared blast radius**. A `permits:`
block makes the file itself the security boundary — hosts, paths, programs,
tools, all default-deny once declared. A task that reaches beyond it is
caught statically, with the machine-applicable fix, before anything runs:

![A permits block declares GitHub-only network access; one task fetches another host; nika check flags the escape with the exact fix line; the boundary widens on purpose in review and the audit passes clean](media/gifs/permits-audit.optimized.gif)

And failure handling is part of the file, not an ops runbook. When a task
dies, `on_error: recover:` degrades to a declared fallback — the run
completes, and the output says what it is:

![The live source is missing so the task fails; on_error recover degrades to the cached snapshot; the run exits 0 and the published file says stale: true](media/gifs/on-error-recover.optimized.gif)

## Images are workflow citizens

`nika:image_generate` renders through the same discipline as everything
else — a declared `permits:` boundary gates every save, the run ledger
meters real spend, and provenance is structural, not a promise:

![One workflow renders a real image and speaks a real line — permits gate the saves, the ledger shows $0.02 exact, the gallery frame carries the actual renders and a waveform drawn from the real MP3 bytes, and the manifest panel shows the provenance nobody else keeps](media/gifs/image-pipeline.gif)

- **Local-first** — `provider: local` speaks the OpenAI-images wire any
  self-hosted server exposes (LocalAI · Ollama · stable-diffusion.cpp ·
  SGLang · vLLM-Omni). The base URL is engine config, never workflow data.
  Clouds when you choose: `openai` · `gemini` · `xai` — and `mock` renders
  real, decodable PNG files offline for CI.
- **Provenance survives `cp`** — every saved PNG carries a `nika` tEXt
  chunk (tool · engine · provider · model · prompt · seed), the practice
  ComfyUI and InvokeAI standardized and no other workflow engine ships.
  The sidecar manifest adds sha256, resolved request and timing.
- **Honesty is enforced** — magic bytes beat declared MIME types, lossy
  provider mappings warn loudly, a provider returning fewer images than
  asked is a visible `count_shortfall:`, result URLs are **never**
  fetched, and base64 never rides workflow outputs — assets, not blobs.
- **Real spend in the ledger** — a render's exact cost (xAI bills in
  ticks) lands on the task line and the run total, the same honest-spend
  channel `infer:` uses.

## Daily commands

```sh
nika inspect flow.nika.yaml          # anatomy · tasks · waves · cost floor
nika check flow.nika.yaml            # the audit · exit 0 clean · 2 findings
nika explain NIKA-VAR-001            # any code · cause · category · fix-form
nika run flow.nika.yaml --var topic=rust   # launch inputs · repeatable
nika test flow.nika.yaml --update    # pin the golden · then `nika test` = offline CI
nika run flow.nika.yaml --task hero    # regenerate ONE task + its upstream cone
nika run flow.nika.yaml --resume .nika/traces/<run>.ndjson   # skip journaled successes
nika run flow.nika.yaml --resume <trace> --answer approve=true  # re-arm a paused gate
nika trace show .nika/traces/<run>.ndjson   # re-render any past run
nika doctor --ping                   # are the local servers actually listening?
```

Every run writes its own journal to `.nika/traces/` (opt out per run with
`--no-trace-file`, globally with `NIKA_NO_TRACE_FILE`) — `--resume` and
`nika trace` read it directly. A paused run exits `4` (a blocking
`nika:prompt` journals its question); cache hits on resume are always
visible — nothing is skipped silently.

## Pick a workflow

The binary embeds a versioned pack of runnable examples. Browse with
`nika examples list`, read one with `nika examples show <slug>`, preview any
of them with `--model ollama/qwen3.5:4b` (or offline with `--model mock/echo`):

| I want to… | Run | For |
|---|---|---|
| Review a PR before merging | [`examples/pr-risk-review.nika.yaml`](examples/pr-risk-review.nika.yaml) | developers |
| Turn meeting notes into owned actions | `nika examples run showcase/t1-meeting-actions` | everyone |
| Digest a week of standups | `nika examples run showcase/t1-standup-digest` | teams |
| Draft release notes from commits | `nika examples run showcase/t2-release-notes` | maintainers |
| Triage a support inbox | `nika examples run showcase/t2-support-triage` | support · ops |
| Chase unpaid invoices politely | `nika examples run showcase/t2-invoice-chaser` | founders |
| Track competitors weekly | `nika examples run showcase/t3-competitor-radar` | founders |
| Screen resumes against a role | `nika examples run showcase/t3-resume-screener` | hiring |
| Build a Monday operating brief | `nika examples run showcase/t4-ceo-monday-brief` | founders |

The full gallery (27 workflows + 6 templates) lives in
[`examples/`](examples/): foundation patterns, business showcases, and the
skeletons `nika new --from <template>` instantiates.

Shared workflows live on [**nika-registry**](https://github.com/supernovae-st/nika-registry) —
every entry pinned to a full commit + sha256 and re-proven by CI (the
conformance oracle + this engine's static certificate: exec · tools ·
cost, visible before anything runs). `nika add` is on the roadmap; today
the registry's `get.py` does resolve → verify → audit in one command.

## The model

Four verbs, and nothing else. A small core that composes into arbitrary
real-world workflows. The Unix and SQL discipline of "small surface, large
composition."

| Verb | What it does |
|---|---|
| `infer` | Call an LLM. Any provider, local or hosted |
| `exec` | Run a shell command |
| `invoke` | Call a tool or MCP server (an HTTP fetch, GitHub, a builtin…) |
| `agent` | Run an autonomous loop with tools, until the task is done |

Everything sits under one frozen, versioned envelope, `nika: v1`, that won't
break. Three properties hold across every workflow:

- **Provider-agnostic, local-first.** Local Ollama or LM Studio, or any API.
  Your workflow doesn't change when the model does.
- **Safe by construction.** A read-XOR-write capability model. A step that
  reads cannot silently write; every effect is explicit and gated.
- **Reproducible.** The file and its execution trace are an auditable,
  re-runnable record.

```mermaid
flowchart LR
    F["workflow.nika.yaml<br/><i>portable · readable · verifiable</i>"] --> E["<b>nika</b><br/>single Rust binary"]
    E -->|infer| L["LLMs<br/>Ollama · LM Studio · any API"]
    E -->|exec| S["shell"]
    E -->|invoke| T["tools · MCP"]
    E -->|agent| A["autonomous loop"]
```

Dependencies make every workflow a graph: independent tasks run in
parallel, an `agent` step fans out, joins wait for every branch — and the
whole plan is known, costed and audited before execution starts:

![The embedded pr-review-fanout workflow as its real execution graph: seven tasks, six waves, all four verbs in one file](media/gifs/dag-execution.optimized.gif)

## Why Nika

The closest analogues aren't products. They're **standards**. SQL. The
Dockerfile. A portable specification with a reference engine. The language is
the contribution, not a product to sell.

As AI agents start acting on the real world, the interface where they act
can't be free text (too vague) or raw code (too risky). It has to be a
**verifiable action language**: one an AI writes, a human reviews and approves,
and a machine runs deterministically. Kept open and sovereign, not locked
inside one vendor's cloud.

What no existing workflow tool offers together: a single Rust binary · portable
declarative YAML · local-first · read-XOR-write capability security · AGPL ·
no cloud required · bring-your-own-LLM.

## Status

Nika is built in the open.

The **language** (the `nika: v1` envelope and its four verbs) is stable and
won't break. The **engine** is a strict, modular Rust workspace. The latest
tagged public release is **v0.91.0**; `main` moves immediately to the next
`-dev` version after each release so local contributor binaries cannot be
confused with Homebrew assets. The 1.0.0 launch remains gated by the release
checklist, not by a date. The code, the
[spec](https://github.com/supernovae-st/nika-spec), and the
[example workflows](examples/) are all readable, and development happens on
`main` in the open.

The `nika: v1` language envelope is frozen forever. It is a separate axis from the
engine version. Every release is complete for its declared scope; no
half-features parked behind a future version.

## Get started

Install (macOS · Linux):

```sh
# Homebrew (macOS · Linux): on your PATH immediately
brew install supernovae-st/tap/nika
nika --version

# …or, without Homebrew: the install script. It downloads the verified release
# binary into ~/.nika/bin and prints the single PATH line to add to your shell
# profile (then reopen the terminal, or `source` it, and `nika --version` works).
curl -LsSf https://nika.sh/install.sh | sh
```

Prefer a guided page? Every install path, step by step: [nika.sh/install](https://nika.sh/install).

> Fully manual / air-gapped? Download the platform tarball + `SHA256SUMS` from the
> [latest release](https://github.com/supernovae-st/nika/releases/latest), verify
> with `sha256sum -c SHA256SUMS --ignore-missing`, then move `nika` onto your `PATH`.

Your first workflow runs with **zero setup**: no model, no API key:

```sh
cat > hello.nika.yaml <<'YAML'
nika: v1
workflow: hello
tasks:
  - id: greet
    exec:
      command: "echo hello from nika"
YAML

nika check hello.nika.yaml   # static audit, before a single token is spent
nika run hello.nika.yaml     # execute locally
```

Adding an AI step? Point it at a local model and nothing leaves your machine:

```yaml
model: ollama/llama3.2:3b    # local · or mistral/..., anthropic/..., any provider
tasks:
  - id: greet
    infer:
      prompt: "Say hello in one sentence."
```

(No model handy? The built-in `mock/echo` previews any workflow offline.)

For real inference, run a local model (Ollama / LM Studio) or set a provider
key, then see what's wired:

```sh
nika doctor                  # provider keys + local servers, with the exact fix
nika init                    # schema wiring + AGENTS.md for this repo
nika wire cursor             # explicit MCP wiring · also: vscode · windsurf · claude · codex · all
nika examples list           # browse the embedded examples
nika examples run 01-hello --model ollama/llama3.2:3b   # a real local run
```

From source (contributors): `git clone https://github.com/supernovae-st/nika.git && cd nika && cargo test --workspace --lib`. End-user docs: [docs.nika.sh](https://docs.nika.sh).

## Work with your agents

Nika is built to be **written by agents and reviewed by you**. `nika init`
drops the schema wiring, `AGENTS.md`, the Cursor rule and a repo-level
[agent skill](https://agentskills.io) into your repo so Claude Code, Cursor,
Codex and friends author valid workflows on the first try. `nika wire
<cursor|vscode|windsurf|claude|codex|all>` points each client's MCP config at
the engine — idempotent, and it preserves your other servers. `nika mcp`
exposes a read-only oracle — 8 tools, `nika_check` and `nika_explain` through
`nika_catalog` and `nika_tools` — any MCP client can call. `nika lsp` speaks LSP to every editor.

Or install everything as a plugin — this repo hosts one plugin (the
`nika-authoring` skill + the MCP oracle) for both ecosystems:

```sh
codex plugin marketplace add supernovae-st/nika-agents && codex plugin add nika@nika
claude plugin marketplace add supernovae-st/nika-agents && claude plugin install nika@nika
```

Agents discover a working prompt chain once; Nika keeps it as a file your
team can check, run and replay.

## Send us a workflow

Do you repeat an AI task every week, in ChatGPT, Claude, Cursor, Codex, or
scripts? Describe it at [nika.sh/convert](https://nika.sh/convert) or
[open a "convert my workflow" issue](https://github.com/supernovae-st/nika/issues/new/choose).
We convert the best ones into runnable `.nika.yaml` examples, credited to you.

## Editor support

This repo is the **engine**. It ships the language server (`nika lsp`, over stdio).
The VS Code / Cursor / Windsurf / VSCodium **extension** lives in its own repo and
is published as [`supernovae.nika-lang`](https://marketplace.visualstudio.com/items?itemName=supernovae.nika-lang)
(and on [Open VSX](https://open-vsx.org/extension/supernovae/nika-lang) for Cursor / Windsurf / VSCodium):

- Install it from your editor's marketplace. It auto-downloads the matching `nika`
  release binary on first use (or reuses the `nika` already on your `PATH`).
- Source + issues: [supernovae-st/nika-vscode](https://github.com/supernovae-st/nika-vscode).
- Any other editor: `nika lsp` speaks LSP over stdio. Wire it into any LSP client.

## Documentation

- **Language spec**: [supernovae-st/nika-spec](https://github.com/supernovae-st/nika-spec) (Apache-2.0), the runtime-agnostic Nika language.
- **End-user docs**: [docs.nika.sh](https://docs.nika.sh).
- **Website**: [nika.sh](https://nika.sh).
- **Examples**: [`examples/`](examples/), the embedded gallery (also `nika examples list`).

**Building Nika?** The engine is crafted under a strict workspace discipline:
context-window-sized crates, a per-crate admission checklist, zero `.unwrap()`
in `src/` (CI-enforced), downward-only layering. The design lives in
[`docs/architecture/`](docs/architecture/) and the decisions in
[`docs/adr/`](docs/adr/README.md); the roadmap is in [`ROADMAP.md`](ROADMAP.md).

## License

The **engine** is AGPL-3.0-or-later (see [`LICENSE`](LICENSE)): modify it and
run it as a hosted service, and users of that service get the source. The
**spec** is [Apache-2.0](https://github.com/supernovae-st/nika-spec), maximally
permissive for a standard.

A commercial license (Grafana model) is available for organizations that can't
accept AGPL's network clause. Contact `contact@supernovae.studio`. Security
reports: `security@supernovae.studio`.

---

© 2024–2026 [SuperNovae Studio](https://supernovae.studio) · 🦋 Nika, the
butterfly on the SuperNovae flag. *Prompt once. Run forever.*
