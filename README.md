<p align="center">
  <a href="https://nika.sh">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="https://nika.sh/brand/nika-logo-dark.svg">
      <img src="https://nika.sh/brand/nika-logo-light.svg" alt="Nika" width="220">
    </picture>
  </a>
</p>

<h1 align="center">Say it once.<br>Keep it as a file.</h1>

<p align="center">
  <b>Checked before it runs. Run with the model you choose. Proven afterwards.</b><br>
  Nika turns the AI work you repeat into a readable workflow that stays yours. Fully open source.
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

<p align="center">
  <a href="media/gifs/first-session.optimized.gif">
    <img src="media/gifs/first-session.optimized.gif" alt="Nika's Session in four moments: a sentence typed in plain words, a checked workflow file saved only after yes, one run under a $0.25 ceiling that keeps the two paid rows, and /proof with its chain, permit checks and what it does not prove" width="960">
  </a>
</p>
<p align="center"><sub>
  “Read ./data/orders.csv, keep only the rows whose status is paid, and write them to ./out/paid.csv.”<br>
  One sentence becomes a checked file; <code>yes</code> saves it, <code>run it</code> keeps the two paid rows, and
  <code>/proof</code> shows what the run recorded. The real Session of nika 0.123.0, restyled; no AI model is involved.
</sub></p>

## What is Nika?

Nika turns repeatable AI work into a small file you keep. Say what you
want done, like *"every Monday, pull the action items out of my meeting
notes"*, and Nika writes it as a readable `.nika` workflow. Before
anything runs, its check shows what the workflow will do, which models
and tools it uses, what it is allowed to touch and what it can cost,
without calling a model. A workflow that could read your private data,
take in untrusted content and send data out is refused until it asks a
person first. You run it when you decide, with the model you choose, on
your machine or through a provider's API, and every run leaves a
tamper-evident record you can verify. One Rust binary with a terminal
Session and a command line, plus a TypeScript SDK for your applications;
local-first and fully open source (AGPL-3.0).

| 1 · Say it | 2 · Check it | 3 · Run it | 4 · Prove it |
|:---:|:---:|:---:|:---:|
| Describe the job; Nika writes a `.nika` file (or `nika compile` from a skeleton) | `nika check` audits it before any model is called | `nika run` with the model you choose | `nika trace verify` checks the run's record |

<p align="center">
  <a href="#see-it-in-four-steps"><b>See it</b></a> ·
  <a href="#you-stay-in-control"><b>Control</b></a> ·
  <a href="#it-refuses-the-lethal-trifecta-before-a-token-is-spent"><b>Trifecta</b></a> ·
  <a href="#try-it-in-two-minutes"><b>Try it</b></a> ·
  <a href="#what-you-can-hand-to-nika"><b>Examples</b></a> ·
  <a href="#why-a-file-and-not-a-chat"><b>Why a file</b></a> ·
  <a href="#works-where-you-already-work"><b>Where it works</b></a> ·
  <a href="#know-the-limits"><b>Limits</b></a> ·
  <a href="https://docs.nika.sh"><b>Docs</b></a>
</p>

## See it in four steps

### 1 · Say it

Describe the job in plain words. Nika writes it as a `.nika` file, shows
you what it will do and what it can touch, and saves it only when you say
`yes`. Nothing runs until you ask. The clip at the top is that Session;
this is its first answer, as the plain view prints it:

```text
nika › Read ./data/orders.csv, keep only the rows whose status is paid, and write them to ./out/paid.csv.
✓ recorded 3 requirements
Nika proposes `compiled-workflow.nika`:
Does
  1. read_source · reads a file
  …
  8. write_output · writes a file
Runs
  when you ask (« run it ») · no schedule was asked
Can touch
  external effects · none
  human approval at run · none
Changes
  + `compiled-workflow.nika` · 97 lines · new
Needs
  nothing more from you
Nothing has run yet · `yes` saves these exact bytes and checks them · running is its own line (« run it »)
```

### 2 · Check it

Before anything runs, `nika check` reads the file and reports what it
will do, what it may touch and what it can cost. It calls no model and
spends nothing.

<p align="center">
  <a href="media/gifs/static-check-fix.optimized.gif">
    <img src="media/gifs/static-check-fix.optimized.gif" alt="nika check finds two defects in a pull-request review workflow, the fix is applied, and the re-check comes back clean; nothing runs and no token is spent" width="860">
  </a>
</p>
<p align="center"><sub>Two mistakes caught before anything runs, the fix, and the clean re-check. Every line is captured from the real CLI.</sub></p>

### 3 · Run it

You run it when you decide, with the model you choose. Here a local
model, through Ollama, writes a meeting's action items as typed JSON.

<p align="center">
  <a href="media/gifs/nika-hero.optimized.gif">
    <img src="media/gifs/nika-hero.optimized.gif" alt="nika check audits a meeting-actions workflow, then nika run executes it on a local model through Ollama and writes the meeting's action items to a typed JSON file" width="860">
  </a>
</p>
<p align="center"><sub>The audit first, then a real run on a local model (<code>ollama/llama3.2:3b</code>); the clip names the release each part was captured with. Nothing leaves the machine.</sub></p>

### 4 · Prove it

Every run leaves a hash-chained record. `nika trace verify` reads it
back, and when a single byte changes it names the line where the chain
breaks.

Runs are also signed once this machine has a run-signing key. Mint it once
with `NIKA_KEYCHAIN=off nika key init`: the key lands in `~/.nika/keys/`
(mode `0600`), where terminal runs, scripts, CI, the TypeScript SDK and
`nika serve` all read it, and later runs report `SEALED` in
`nika trace verify` and `/proof`. Without the variable, a terminal on a Linux
desktop can keep the key in the Secret Service, which only terminal runs open
(`nika key align` copies it to the files), and on a Mac whose keychain holds a
key from Nika 0.122 or older, `nika key init` refuses: 0.123.0 and later never
open the macOS keychain.

<p align="center">
  <a href="media/gifs/trace-proof.optimized.gif">
    <img src="media/gifs/trace-proof.optimized.gif" alt="A run's trace drawn as five hash-linked blocks: nika trace verify reports the chain intact, then one byte of line 4 changes in a copy and verify stops at line 5: BROKEN, exit 2" width="860">
  </a>
</p>
<p align="center"><sub>The chain read back intact, then one changed byte refused at the next line. A <code>mock/echo</code> rehearsal captured from the real CLI; the scan is an illustration.</sub></p>

## You stay in control

### It refuses the lethal trifecta before a token is spent

A workflow that can read your private data, take in content nobody vetted
(a web page, a tool's answer, a command's output) and send data out is one
injected instruction away from a leak. Simon Willison named this the
[lethal trifecta](https://simonwillison.net/2025/Jun/16/the-lethal-trifecta/).
`nika check` reads it from the permits, then follows the untrusted content
from step to step, through a model's answer and into child workflows. When
all three meet, it refuses the file until a person must say yes first.

This workflow reads a note, fetches a public page, asks a local model for a
digest and posts it to Slack:

```yaml
nika: page-digest

model: ollama/llama3.2:3b

permits:
  fs: { read: ["./notes/today.md"] }
  net: { http: ["news.ycombinator.com", "hooks.slack.com"] }
  tools: ["nika:fetch", "nika:read", "nika:notify"]

tasks:
  page:
    invoke:
      tool: "nika:fetch"
      args: { url: "https://news.ycombinator.com/" }

  notes:
    invoke:
      tool: "nika:read"
      args: { path: "./notes/today.md" }

  digest:
    with: { page: "${{ tasks.page.output }}", notes: "${{ tasks.notes.output }}" }
    infer:
      prompt: "Pick the three stories that matter for my notes.\nNotes: ${{ with.notes }}\nPage: ${{ with.page }}"
      max_tokens: 400

  post:
    with: { digest: "${{ tasks.digest.output }}" }
    invoke:
      tool: "nika:notify"
      args: { target: "https://hooks.slack.com/services/T000/B000/XXXX", message: "${{ with.digest }}" }
```

The page never reaches Slack directly, only through the model's digest. The
check follows it anyway and refuses the file with exit code 2:

```text
nika check · page-digest.nika
 ✔ PLAN     3 waves · 4 tasks · max parallelism 2
 …
 ✖ TRIFECTA [NIKA-SEC-009] lethal trifecta complete · human gate required — untrusted content from `page` reaches egress task `post` while private read + untrusted ingress + external egress are all permitted …
```

Drop any one of the three and there is nothing to refuse. To keep all three,
put a question in front of them: a `nika:prompt` with no default, whose
answer the fetch, the read and the post each require.

```yaml
  approve:
    invoke:
      tool: "nika:prompt"
      args: { message: "Read my notes, fetch Hacker News and post a digest to Slack?" }

  page:
    with: { go: "${{ tasks.approve.output }}" }
    when: ${{ with.go == true }}
    invoke:
      tool: "nika:fetch"
      args: { url: "https://news.ycombinator.com/" }
```

With `nika:prompt` added to `permits.tools`, and the same `go` binding and
`when:` line on `notes` and `post`, the check passes with exit code 0:

```text
 ✔ TRIFECTA no lethal trifecta over the declared permits: without a human gate
 layers · valid ✔ · access ready ✔ · capacity fit ✔ · run ready ✔
```

A gate that only looks like one is refused too: a prompt with a `default:`
answers itself and guards nothing (`NIKA-SEC-009` stays), and a step that
still runs after a "no" fails `NIKA-SEC-014`. At a terminal the run asks; in
CI it pauses with exit 4, as [shown below](#it-asks-before-it-acts). Every
verdict here was captured from nika 0.124.0.
[NIKA-SEC-009](https://nika.sh/language/errors/NIKA-SEC-009) explains the
rule; Meta's [Agents Rule of Two](https://ai.meta.com/blog/practical-ai-agent-security/)
states the same principle for agents.

### It tells you the cost before the first token

`nika check` prices a workflow from its model's catalog price and the
output limit each step declares. Set a budget below that ceiling and the
run refuses to start: no model is called and nothing is spent.

<p align="center">
  <a href="media/gifs/cost-ceiling.optimized.gif">
    <img src="media/gifs/cost-ceiling.optimized.gif" alt="nika check caps a Claude Sonnet workflow at ≤ $0.0614 once max_tokens is declared, and nika run with a $0.05 budget refuses to start: the ceiling past the budget line, a REFUSED TO START stamp, $0 spent" width="860">
  </a>
</p>
<p align="center"><sub>Unbounded until one line declares <code>max_tokens</code>, then a hard ceiling of $0.0614; a $0.05 budget refuses the run (exit 2). Prices are catalog estimates, not invoices; no model was called.</sub></p>

### It asks before it acts

A step can wait for a person. At your terminal it asks; in CI, where
nobody can answer, the run pauses safely with exit 4 and prints the one
line that resumes it.

<p align="center">
  <a href="media/gifs/approval-gate.optimized.gif">
    <img src="media/gifs/approval-gate.optimized.gif" alt="A workflow of build, a human gate drawn as a door, then ship: at a terminal Nika asks Ship this build to production? [y/N] and y opens the door; in CI the run pauses with exit 4 and prints a line that, pasted and run, reuses the build and ships" width="860">
  </a>
</p>
<p align="center"><sub>Nothing ships without a yes: asked at a terminal, paused in CI, resumed by the printed line with the finished build reused. Every terminal line is captured from the real CLI; the door is an illustration.</sub></p>

## Try it in two minutes

### 1 · Install

```sh
curl -LsSf https://nika.sh/install.sh | sh
```

<details>
<summary>Homebrew, npm, a container, a release archive, Nix or Windows</summary>

- **Homebrew:** `brew install supernovae-st/tap/nika`
- **npm**, the binary plus the TypeScript SDK:
  `npm install @supernovae-st/nika` (the `nika` command lands in
  `node_modules/.bin`). The package bundles the engine of its own version:
  check `nika --version`.
- **A container** (Linux, amd64 and arm64), built from the same release
  binaries: `docker run --rm ghcr.io/supernovae-st/nika:0.124.1 --version`.
  Mount a project to check or run its files, for example
  `docker run --rm -v "$PWD:/work" -w /work ghcr.io/supernovae-st/nika:0.124.1 check hello.nika`.
- **A release archive:** download one from the
  [latest release](https://github.com/supernovae-st/nika/releases/latest).
  Every release ships a checksum manifest, a GitHub attestation and SLSA
  provenance; [RELEASING.md](RELEASING.md#what-a-user-can-prove) gives the
  three verification commands, for example
  `gh attestation verify nika-macos-arm64-0.124.1.tar.gz --repo supernovae-st/nika`.
- **Nix:** `nix profile install github:supernovae-st/nika/v0.124.1` builds
  the release from the repository's flake. Without the tag, the flake builds
  the default branch, which can be ahead of the latest release.
- **Windows:** use WSL2 and the Linux binary. Native Windows binaries are
  not shipped.

</details>

### 2 · Say what you want

This first job needs no AI account: Nika's deterministic compiler settles
it without calling a model. Make a folder with a small spreadsheet and
open the Session:

```sh
mkdir -p orders-demo/data && cd orders-demo
cat > data/orders.csv <<'EOF'
order_id,customer,status,amount
1001,Juniper Books,paid,18.00
1002,Kestrel Cafe,pending,9.50
1003,Linden Studio,paid,13.50
1004,Moss Supply,refunded,6.00
EOF
nika
```

At the prompt, type:

```text
Read ./data/orders.csv, keep only the rows whose status is paid, and write them to ./out/paid.csv.
```

Nika proposes a `.nika` file and shows what it will do, what it can touch
and the result of its check. **Nothing has run yet.** Type `yes` to save
the file, then `run it`. `out/paid.csv` gets the header and the two paid
rows, and `/proof` shows what the run's record proves, and what it does
not.

> [!TIP]
> Nika works only inside the folder you start it in. `/show` prints the
> proposal's exact bytes, `/meaning` maps your request clause by clause,
> and `no` discards it. `Ctrl+T` switches the view to full screen and back,
> and `nika --plain` gives the same Session as plain lines. The
> [Session guide](docs/usage/conversational-session.md) covers questions,
> changes, spending ceilings and `/restore`.
>
> In a Git repository, the Session leaves `.gitignore` alone: before you
> commit, add `.nika/traces/` (run records can hold file contents and model
> outputs) and the Session's own `.nika/*.ndjson` and
> `.nika/session-state.json`. After a run, the Session reminds you when
> traces are not ignored.

### 3 · Add a model

Most open-ended requests need a model to write the workflow, and
workflows such as summaries call a model each time they run. Nika reads
provider keys from your environment, so keep yours in your shell startup
file. For DeepSeek:

```sh
# in ~/.zshrc or ~/.bashrc, then open a new terminal
export DEEPSEEK_API_KEY='your-key'
```

The first request that needs a model shows a numbered choice: answer
`2 deepseek/deepseek-flash`. Nika keeps the choice for later Sessions, and
`/intelligence` changes it. Other API providers and local engines
(Ollama, llama.cpp, LM Studio, vLLM) work too: `nika catalog` lists them.
Your provider meters and bills these calls; Nika's cost figures are
catalog estimates, not invoices.

### 4 · Use the file directly

A workflow is an ordinary file. The command line compiles, checks, runs
and verifies it. This offline lesson needs no provider:

```sh
nika compile hello hello.nika
nika check hello.nika
nika run hello.nika
nika trace verify
```

▶ [Watch these four commands run](media/gifs/full-loop.optimized.gif), exactly as captured from the CLI.

`hello` always uses `mock/echo`, a stand-in model that echoes the prompt,
even when provider keys are present. It proves that the workflow runs, not
that a model answered. Each run records its trace under `.nika/traces/`,
and creating the file adds that directory to `.gitignore`.
`nika trace verify` checks the latest trace's integrity, not the truth of
an AI answer.

## What you can hand to Nika

| You say | Nika keeps | Watch |
|---|---|---|
| *"Read `./data/orders.csv`, keep only the rows whose status is paid, and write them to `./out/paid.csv`."* | A workflow that reads one file and writes one file. No AI model is involved. | [▶ The Session](media/gifs/first-session.optimized.gif) |
| *"Pull the action items out of my meeting notes."* | A workflow that asks a local model and writes the items as typed JSON. | [▶ A local run](media/gifs/nika-hero.optimized.gif) |
| *"Every Friday, turn `CHANGELOG.md` into release notes."* | A workflow your coding agent writes with the Nika plugin; the check catches its mistake before anything runs. | [▶ With your agent](media/gifs/agent-plugin.optimized.gif) |
| *"Rate the risk of each pull request."* | A workflow the Nika GitHub Action checks whenever a pull request changes it: one comment carries the verdict. | [▶ In your pull requests](media/gifs/pr-check-comment.optimized.gif) |

## Why a file, and not a chat?

A chat answer is gone when the conversation ends, and the next one can
differ. A `.nika` file stays: you read it, check it, run it again and
review its changes like any other file in your project.

- **Say it once.** The request you would retype in a chat every week is
  kept as a file that runs again.
  ▶ [Watch a weekly request become a file](media/gifs/chat-to-workflow.optimized.gif)
- **Read it before it runs.** Every workflow has the same shape: nine
  sections and four kinds of step, in a file you can open and review.
  ▶ [Watch one taken apart](media/gifs/spec-anatomy.optimized.gif)
- **It touches only what it declares.** The file lists what it may read,
  write and reach. The check refuses the step that reaches past it.
  ▶ [Watch the check catch an escape](media/gifs/permits-audit.optimized.gif)
- **Failures are planned for.** A step can name its fallback: the run
  finishes, the output says it is stale, and the record keeps the failure.
  ▶ [Watch a missing feed absorbed](media/gifs/on-error-recover.optimized.gif)
- **Parallel where it can be.** Steps that do not depend on each other
  run side by side, in waves the check plans before the run.
  ▶ [Watch a workflow run in waves](media/gifs/dag-execution.optimized.gif)

Every clip on this page shows real files and terminal lines captured from
the binary, names the release it was captured with, and says what it
illustrates. The [media index](media/README.md#feature-clips) lists them
all and how they are made.

## Works where you already work

Nika comes to the tools you already use. Your coding agent can write the
file for you, and the check keeps it honest:

<p align="center">
  <a href="media/gifs/agent-plugin.optimized.gif">
    <img src="media/gifs/agent-plugin.optimized.gif" alt="A coding agent writes release-notes.nika from one request with the plugin's nika-authoring skill; nika check refuses nika:read_file (NIKA-BUILTIN-001), the agent repairs it to nika:read, the re-check says run ready, and a mock/echo rehearsal writes release-notes.md" width="860">
  </a>
</p>
<p align="center"><sub>Your agent writes it, Nika checks it, you keep it. The agent session is an illustration; the checks and the rehearsal run are captured from the real CLI.</sub></p>

| Where | What you get | Watch |
|---|---|---|
| **Your coding agent** · [nika-plugins](https://github.com/supernovae-st/nika-plugins) | Claude Code, Codex, Cursor and others learn to write a workflow, check it and repair what the check finds. | ▶ [above](media/gifs/agent-plugin.optimized.gif) |
| **Your editor** · `nika lsp` | The language server inside the binary: the check's findings as you type, in any LSP client. The VS Code extension ([nika-vscode](https://github.com/supernovae-st/nika-vscode)) stays at 0.120 and is not qualified with 0.124; point its `nika.server.path` at your 0.124 binary ([known limits](https://docs.nika.sh/changelog/known-limits#the-vs-code-extension-stays-at-0-120)). | ▶ [The audit, as you type](media/gifs/editor-diagnostics.optimized.gif) |
| **Your pull requests** · [nika-action](https://github.com/supernovae-st/nika-action) | One comment with the verdict, before anyone spends a token. | ▶ [Every pull request gets a verdict](media/gifs/pr-check-comment.optimized.gif) |
| **Your app** · [nika-client](https://github.com/supernovae-st/nika-client) | Run a workflow from TypeScript ([the package](https://www.npmjs.com/package/@supernovae-st/nika)), get a typed result and verify its receipt, with the engine the package bundles or against your `nika serve`. | ▶ [Run it from your app, prove what ran](media/gifs/typescript-client.optimized.gif) · ▶ [One client, two doors](media/gifs/sdk-doors.optimized.gif) |

### From your application

`npm install @supernovae-st/nika` brings the TypeScript SDK and the engine of
its own version (Node.js 22 or newer, on macOS or Linux with glibc). Run
`NIKA_KEYCHAIN=off ./node_modules/.bin/nika key init` once, so the engine your
app starts can seal each run's record. Then, beside a `hello.nika`, this is the
package's quick start:

```ts
import { Nika, isNikaRunSucceeded } from '@supernovae-st/nika';

const nika = new Nika({ cwd: process.cwd() });

// The engine checks the file first: a workflow it refuses throws here and never starts.
const run = await nika.run<{ greeting: string }>('hello.nika', { maxCostUsd: 0 });
const result = await run.result();

// A failed run is data you read, not an exception.
if (!isNikaRunSucceeded(result) || !result.receipt) {
  console.error(result.status, result.error?.code, result.error?.message);
  process.exit(1);
}
console.log(result.outputs?.greeting);

// Ask the engine to verify the run's tamper-evident record.
const proof = await nika.traceVerify(result.receipt);
console.log(proof.verified ? 'receipt verified' : `not verified: ${proof.reason}`);
```

Save it as `demo.mts` (top-level `await` needs a module) and run
`node demo.mts`: Node.js 22.18 or newer runs the TypeScript as is. The
[SDK quick start](https://docs.nika.sh/sdk/start/quickstart) gives the
`hello.nika` this program runs.

To reach a `nika serve` instead of the bundled engine, keep the methods and
change the options of two lines:

```ts
const nika = new Nika({ url, token, allowInsecureHttp: true });
const run = await nika.run<{ greeting: string }>('hello.nika', { idempotencyKey });
```

`url` and `token` name the server and its bearer token. Plain `http://` is
accepted only to a loopback host, and only with `allowInsecureHttp: true`.
Over HTTP, `run()` needs an `idempotencyKey` you own (a retry with the same
key returns the same job) and refuses `maxCostUsd`: the server applies its
own per-run ceiling, USD 1 by default. `hello.nika` is then a served name,
the workflow's path from the server's project root.
[nika-client](https://github.com/supernovae-st/nika-client) documents the
whole API.

<p align="center">
  <a href="media/gifs/sdk-doors.optimized.gif">
    <img src="media/gifs/sdk-doors.optimized.gif" alt="Animated diagram of the TypeScript SDK: in the quick start, new Nika() starts the engine bundled with @supernovae-st/nika 0.124.0 as a child process; for nika serve, line 3 takes url, token and allowInsecureHttp, and line 6 takes an idempotencyKey in place of maxCostUsd. A refused list-files.nika gets NIKA-SEC-004 and run() throws, hello.nika runs, its events stream back to the app, its six hash-chained journal lines end in run_sealed, and traceVerify() answers verified locally and sealed from the server" width="860">
  </a>
</p>
<p align="center"><sub>Your app asks, the engine decides, the receipt proves it. The code is the quick start above, which the capture ran, with lines 3 and 6 also shown as its server driver ran them against <code>nika serve</code>. Every event, verdict, hash and version on screen was captured from <code>@supernovae-st/nika</code> 0.124.0, the engine it bundles and <code>nika serve</code> in a <code>mock/echo</code> rehearsal; the cards, lanes, packets and pacing are illustration.</sub></p>

## Know the limits

- **0.124 is the current release line.** Work stays focused on
  maintenance, security and the media capabilities workflows need; native
  video is planned for 0.125. The 0.123.x line is the previous one, and
  there is no hosted service. The known limits are listed in plain words,
  each with a way around it:
  [Known limits in 0.124](https://docs.nika.sh/changelog/known-limits).
- **Some requests cannot be expressed.** Nika then says what stopped it
  and writes nothing.
- **A clean check is not a true answer.** It describes a file's structure
  and boundary; it does not prove that model output is right.
- **The trifecta check judges permits and flow, not words.** It does not
  read a page or a model's answer for injected instructions, and a person
  who answers yes approves what follows. On a harness seat, what `permits:`
  covers is listed per seat ([harness agents](docs/usage/harness-agents.md)).
- **This README describes the released 0.124.1.** Compare `nika --version`
  with the [changelog](CHANGELOG.md), which lists what each release
  changed. Until the next release, each change merged after 0.124.1 is
  described in its own file under [`changelog.d/`](changelog.d/README.md),
  and the release folds those files into the changelog.

<details>
<summary><b>The four building blocks</b></summary>

| Verb | What it does |
|---|---|
| `infer` | Ask a model to produce an answer |
| `invoke` | Call a native tool, MCP tool or another workflow |
| `exec` | Run a command |
| `agent` | Let a model use allowed tools for a bounded number of turns |

A workflow uses only the verbs its job needs. The orders workflow at the
top uses `invoke` alone: eight steps over five builtin tools (`nika:read`,
`nika:convert`, `nika:jq`, `nika:assert` and `nika:write`), with no model
and no shell.

</details>

<details>
<summary><b>Compile from the command line</b></summary>

`nika compile --list` lists exact skeletons. Preview one with
`nika compile <slug> --json`, then answer its stable questions with repeatable
`--answer KEY=JSON_LITERAL`. An incomplete result includes its candidate and
questions; it does not write a file. Only a Ready candidate plus an explicit
destination writes. Existing destinations require `--force`.

For an accepted source, a conservative constant edit uses the same core:

```sh
nika compile --base workflow.nika --change 'Set const.topic to "new topic"' --json
```

Add `--output edited.nika` to materialize a Ready edit. The base source
remains explicit. Unresolved intent stays incomplete without a substitute
workflow. `--authoring-model` seats a model to interpret free intent and
revise text. No request count is imposed by default: creation ends on a
result, a failure, no progress or a cancellation. `--authoring-max-calls N`
sets an explicit ceiling (an ACP harness counts its invocations; its own
requests are unknown), and typed repairs or samples that cannot fit under it
are refused before any request. `--authoring-strategy` selects `escalate`,
`only`, `sketch` or `off`. The compiler checks the proposed source and its
fidelity to the request. This is not a guarantee of arbitrary-language
understanding, and Graph editing is not implemented by this CLI.

Compile's Check preview judges source only. `nika check` and `nika run` judge
the actual environment separately. For a real model, choose the provider and
access explicitly; the [model setup documentation](https://docs.nika.sh)
explains local, API and supported harness choices. On a harness seat an
`agent:` task runs the application's own tools:
[what `permits:` covers there](docs/usage/harness-agents.md), and what it
does not, is listed per seat and printed by `nika check`.

</details>

<details>
<summary><b>Before sharing a workflow or a run</b></summary>

Share the workflow, not credentials or private data. Review file paths and
tool access. Keep secrets out of source control.

Run journals can contain the data and outputs the workflow processed.
The `.nika/traces/` directory is a data-at-rest surface: it inherits the sensitivity
of everything the run read. Exclude it from Git unless you deliberately intend
to publish those records, and treat them with the same care as their source
data.

After a run, `nika trace verify` verifies the latest journal. Compare its
head with the one printed by the run. This checks the record, not the AI's
judgement.

</details>

<!-- city:map -->
## 🦋 The Nika family

| | Repository | What it gives you |
|---|---|---|
| 🦋 | **[nika](https://github.com/supernovae-st/nika)** | **The engine and CLI: write, check, run and verify AI workflows** |
| 📖 | [nika-docs](https://github.com/supernovae-st/nika-docs) | The documentation, live at [docs.nika.sh](https://docs.nika.sh) |
| 📜 | [nika-spec](https://github.com/supernovae-st/nika-spec) | The language specification and the suite that proves an engine follows it |
| 🧩 | [nika-vscode](https://github.com/supernovae-st/nika-vscode) | The editor extension, frozen at 0.120 and not qualified with 0.124: use `nika lsp` and the JSON Schema |
| 🟦 | [nika-client](https://github.com/supernovae-st/nika-client) | Run and verify workflows from TypeScript |
| ✅ | [nika-action](https://github.com/supernovae-st/nika-action) | A GitHub Action that posts a `nika check` verdict on your pull requests |
| 🚀 | [nika-actions-starter](https://github.com/supernovae-st/nika-actions-starter) | A ready template: workflows, editor setup and CI from the first push |
| 📦 | [nika-registry](https://github.com/supernovae-st/nika-registry) | Shareable workflows, pinned and re-verified |
| 🤖 | [nika-plugins](https://github.com/supernovae-st/nika-plugins) | Teaches your coding agent (Claude Code, Codex, Cursor…) to write Nika |
| 🍺 | [homebrew-tap](https://github.com/supernovae-st/homebrew-tap) | `brew install supernovae-st/tap/nika` |
| 🐙 | [gh-nika](https://github.com/supernovae-st/gh-nika) | The Nika CLI as a GitHub CLI extension |
| 🏛️ | [nika-estate](https://github.com/supernovae-st/nika-estate) | Where each file in Nika's core repositories comes from, declared and re-checkable |
<!-- /city:map -->

This repository is the one executable: it parses, checks, runs and traces
every workflow. The language is defined by the specification, and the other
repositories drive this engine without adding to its authority.

## Go further

[Session guide](docs/usage/conversational-session.md) ·
[Examples](examples/README.md) ·
[Documentation](https://docs.nika.sh) ·
[TypeScript SDK](https://www.npmjs.com/package/@supernovae-st/nika) ·
[Editor extension (0.120, not qualified with 0.124)](https://marketplace.visualstudio.com/items?itemName=supernovae.nika) ·
[Open specification](https://github.com/supernovae-st/nika-spec) ·
[Registry](https://github.com/supernovae-st/nika-registry) ·
[Planning archive](https://github.com/orgs/supernovae-st/projects/3) ·
[Website](https://nika.sh)

Nika 0.124 is the current open-source release line, usable today and
maintained. The engine is
[AGPL-3.0-or-later](LICENSE); the specification and the TypeScript SDK are
Apache-2.0.
