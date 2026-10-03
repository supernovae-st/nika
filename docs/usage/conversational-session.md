# Describe work in the Session

Run `nika` with no arguments in a terminal to open the Session. You describe an
outcome in your own words. Nika turns it into a `.nika` workflow file, and you
decide what happens next. Saving and running are separate decisions, and runs
leave records you can inspect.

| Step | You type | What happens |
|---|---|---|
| Describe | a sentence | Nika reads the request and the files it names |
| Clarify | an answer | Nika asks when a value it needs is unclear |
| Review | `/show`, `/meaning`, or a change in words | The proposal shows its steps, reach and check result |
| Save | `yes` | The exact reviewed bytes are written and checked; nothing runs |
| Run | `run it` | The saved workflow runs once, under an announced ceiling |
| Inspect | `/proof`, `/details` | What the run produced and recorded, and how the workflow was built |

This guide describes the engine in this repository. An installed release older
than this source does not have every feature described here: compare
`nika --version` with the [changelog](../../CHANGELOG.md). What this source
carries beyond the latest release is described one file per change in
[`changelog.d/`](../../changelog.d/).

## Before you start

- [Install Nika](../../README.md#try-it-in-two-minutes), then run `nika` from the folder
  you want to work in. That folder is the Session's project root. Nika works
  only under it: it lists the workflows there, reads the files your request
  names, and proposes new files inside it.
- On an interactive terminal, `nika` opens the full-screen workspace with the
  current project, workflow inspection and conversation. At narrow sizes the
  conversation stays in focus. `F6` moves between regions, `Esc` returns to the
  composer, and `Alt+Enter` breaks a line. Opening a workflow from the project
  list shows Source, Plan, Graph and Check for the same observed bytes; it does
  not attach the workflow to your message or run it. The check covers the parent
  file only and keeps imported dependencies and run readiness unknown.
  `NIKA_TUI=inline nika` selects the inline view with finished blocks in the
  terminal's scrollback; `Ctrl+T` switches presentation. `nika --plain` (or `NIKA_TUI=0`) opens the same
  Session as plain lines, for screen readers, recorders and scripts. When the
  terminal cannot host the terminal view, the plain view opens instead. When
  its input or output is not a terminal,
  such as a pipe, `nika` prints a welcome card and does not open a Session.
- `/help` lists the commands. `/quit` closes the Session.

## Choose who writes the workflow

The Session opens on *What do you want to automate?* without asking you to
choose an AI. Nika's deterministic compiler settles some requests without any
model call. The first request that needs a model shows a numbered choice.
Nika needs one to write a workflow the compiler cannot settle, or to answer
a question in words:

| Answer | What it uses |
|---|---|
| `1` | An AI app you are signed into, when Nika can get an answer through it |
| `2` | An API provider, with your own key (metered by the provider) |
| `3` | A local engine on this machine |
| `4` | No AI: engine facts and deterministic compilation only |

A number can be followed by a model: `2 deepseek/deepseek-flash`. Nika keeps the
choice in `~/.nika/session-intelligence.json` for later Sessions. `/intelligence`
asks again, and `/status` shows the project root, the chosen AI and where your
context goes. The request that triggered the choice continues once you answer.

### DeepSeek

Nika reads provider keys from the environment. It has no store for provider
credentials: `nika key` manages the key that signs run records, not provider
keys. To keep a DeepSeek key for every new terminal, export it from your shell
startup file:

```sh
# in ~/.zshrc or ~/.bashrc
export DEEPSEEK_API_KEY='your-key'
```

`NIKA_DEEPSEEK_API_KEY` is read too. Open a new terminal, run `nika`, and when
Nika asks which AI to use, answer:

```text
2 deepseek/deepseek-flash
```

Naming the model keeps it fixed. With `2 deepseek` alone, Nika uses the
provider's default model (`deepseek-flash`). When that model cannot settle a
request, Nika tries once more with `deepseek/deepseek-v4-pro`, unless you
stated a spending amount.

The same environment variable serves workflow authoring in the Session and any
DeepSeek model step when a saved workflow runs. A workflow's run-time model is
written in the file. When your request leaves it open, Nika may ask which
model to use.

Check a provider's data policy before you send it private material.

### Other choices

- Other API providers work the same way. `nika doctor` names the variable each
  provider reads.
- A local engine (`3`, for example `3 ollama/<model>`) keeps inference on your
  machine. Nika does not price local computation: it is unpriced, not free.
- Signed-in apps (`1`) can answer questions when Nika can attest them. Writing
  workflows through them is limited and outside the qualified path. For
  example, authoring through Codex is refused.
- With `4`, a request the deterministic compiler cannot settle stops with an
  explanation and nothing is written.

## A result you can reproduce: filter a CSV

This walkthrough needs no API key. The deterministic compiler settles the
request, so neither authoring nor the run calls a model.

Create a folder with a small file and open the Session there:

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

Type the request:

```text
Read ./data/orders.csv, keep only the rows whose status is paid, and write them to ./out/paid.csv.
```

Nika proposes a new workflow file, here `compiled-workflow.nika`. This excerpt
comes from the plain view:

```text
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
…
  check of these bytes · `compiled-workflow.nika` · clean ✔
  when it runs:
    · reads ./data/orders.csv
    · writes ./out/paid.csv
    · tools nika:assert · nika:convert · nika:jq · nika:read · nika:write
    · model output estimate · $0 · no direct model task in these checked bytes
```

`/show` prints the proposal's exact bytes. The filter is an ordinary jq
expression you can read and change:

```yaml
expression: '[.records[] | select(.status == "paid")]'
```

Type `yes`. Nika writes exactly the reviewed bytes, checks the file on disk and
stops there:

```text
applied · wrote `compiled-workflow.nika`
  check · `compiled-workflow.nika` · clean ✔
Saved · checked · not active · nothing has run
  say « run it » to run it once (a ceiling is announced first)
```

Type `run it`:

```text
running `compiled-workflow.nika` once · ceiling $0.25
…
  produced · ./out/paid.csv (92 B)
  read · ./data/orders.csv
  cost · no model usage recorded
```

`out/paid.csv` keeps the header and the two paid rows:

```text
order_id,customer,status,amount
1001,Juniper Books,paid,18.00
1003,Linden Studio,paid,13.50
```

With the same Nika version, the same request and the same input file, this path
gives the same workflow and the same output bytes. The same compiler works
without the Session, and its file differs only in the name it takes from the
destination (`nika: paid-orders`):

```sh
nika compile 'Read ./data/orders.csv, keep only the rows whose status is paid, and write them to ./out/paid.csv.' paid-orders.nika
nika check paid-orders.nika
nika run paid-orders.nika
nika trace verify
```

Ask for more, for example the paid rows plus their total in a second file, and
the deterministic compiler no longer settles the request alone. Nika then asks
you to choose an AI and writes the workflow with it. Review that proposal the
same way: the saved workflow can still compute its results without a model.

## A result that depends on the model: summarize notes

Add a notes file to the same folder:

```sh
mkdir -p notes
cat > notes/launch.md <<'EOF'
# Launch notes
- The beta opens to 40 teams.
- Support answers within one business day during the beta.
- Pricing stays the same until general availability.
EOF
```

Then ask:

```text
Summarize ./notes/launch.md in three bullet points and write them to ./out/summary.md.
```

The compiler settles this request's structure without an authoring model, but
the workflow itself calls a model every time it runs. So Nika asks one
question: which model should run it. Answer with a model you have a key for,
for example `deepseek/deepseek-flash`. (`nika compile` asks the same `model`
question for this request.)

The proposal then contains a model step (`infer:`). `/show` displays its model
and prompt. The generated prompt asks the model to use only the notes and to
anchor each claim on a verbatim span of them. That helps review; it does not
guarantee fidelity. The review shows a model output estimate.

After `yes` and `run it`, the run calls the model and needs the key. Compare
`out/summary.md` with the notes: each bullet should state a fact from the
source. Wording can change from one run to the next and from one model to
another. Nika records the run and its usage; it does not certify that a summary
is faithful.

## Change a proposal before saving

While a proposal waits, a sentence that is neither a yes, a no nor a question
is read as a change:

```text
write the paid rows to ./out/confirmed.csv instead
```

Nika revises the pending proposal from its exact bytes, your original request
and your change. The new proposal replaces the old one and needs its own `yes`.
If the change cannot be settled, the previous proposal keeps waiting and Nika
says so. A change can need the chosen AI, like the original request. `no`
discards the proposal, and `/meaning` lists what Nika kept of your request,
clause by clause.

## Save and Run are separate

- `yes` writes only the reviewed bytes and checks them. Saving never runs a
  workflow, and a new save clears the previous run status.
- `run it` runs the saved workflow once, and only when the file on disk checks
  clean. You can name a file and a ceiling:
  `run paid-orders.nika with a ceiling of 0.05`.
- The ceiling is announced before the run starts. It is the amount you state,
  otherwise the `ceiling:` in the project's `nika.yaml`, otherwise USD 0.25.
  It bounds the run's metered spend as the runtime estimates it. It does not
  cover authoring and it is not an invoice.
- After reopening a history whose saved spending constraint cannot be proved,
  state a fresh Run ceiling. No default replaces that missing evidence.
- When a workflow declares a required input without a default, the Session asks
  for it before the run.
- When a run pauses at an approval step, the Session shows that step's
  question and your answer resumes the same run. Nothing answers for you.
  Answer a waiting approval before you ask for another run.
- When a run's cost cannot be estimated, the terminal view asks a separate
  `yes / no / details` question that approves that one run only.
- The qualified sequence is review, `yes`, then a separate `run it`. It does not
  cover stating a run ceiling while a proposal still waits for its `yes`.

## Inspect the result

In the workspace, the run object has Run, Outputs, Files and Proof faces. Focus
that region with `F6`, use Left/Right to change face, and `r` to read again.
Outputs are the values carried by the run's settlement. Files shows up to eight
files the run reported writing, read now (at most 1 MiB each). Those current
bytes are not certified as the bytes written by the run.

Proof captures up to 8 MiB of the journal and verifies those bytes. It shows
separately whether execution, source and receipt match the selected run. A
missing or conflicting identity stays unbound; a seal declared by the run is
not a verified signature. The verdict checks the record, not business success.
A refused or unavailable read stays visible; use `nika trace verify` for a
journal above the workspace's interactive cap.
Custody key files must themselves be regular files; a linked key file is
refused as unavailable, even when its parent directory is a link.

After a run, the Session lists what the run produced and read, the model usage
it recorded, and the path of its trace.

- `/proof` shows what the trace records (chain, seal, boundary, task hashes)
  and what it does not prove.
- `/details` shows how the last workflow was built: backend and model, calls,
  tokens and time, strategy, decision model, and engine identity.
- Outside the Session, `nika trace verify` checks the latest trace's hash chain
  and says whether the run was signed. It checks the record, not whether an AI
  answer is true.

| Where | What |
|---|---|
| `<project>/<name>.nika` | the saved workflow |
| `<project>/.nika/traces/` | run traces |
| `<project>/.nika/session-state.json` | the Session's record for this project |
| `<project>/.nika/consents.ndjson` | which files each `yes` wrote |
| `~/.nika/sessions/<project-digest>/` | the conversation history ([details](../architecture/session-history.md)) |
| `~/.nika/session-intelligence.json` | your AI choice |

Traces and history can contain data the workflow read. Keep them out of version
control unless you intend to publish them.

## Close and come back

`/quit` (or `/exit`) closes the Session. A proposal that was still waiting for
your answer is kept. The next `nika` in the same folder offers `/restore`,
which shows the original request and proposes the kept draft again, checked
against the project as it is now. It calls no AI and writes nothing until you
type `yes`.

A kept authoring question takes priority when `/restore` finds one. It replays
its recorded plan against the recorded observation, without a model call or a
fresh observation; a changed edit base is held for resolution. Restoring a draft
instead proposes it again against the current project, as described above.
Neither path restores consent, Run authority or a spending allowance.

Reopening restores the conversation's goal, decisions, open questions and
recent turns. The workspace repaints retained turns as history and reopens the
last observed run as evidence. Its outputs, file list and task rows are not
restored; Proof is verified again when opened. Nothing is replayed. Earlier approvals and approval answers do not carry over, and
neither does a ceiling agreed for a saved file. To run a workflow saved in an
earlier Session, name its ceiling:
`run compiled-workflow.nika with a ceiling of 0.25`.

## Spending

- With no amount stated, the Session puts no allowance on authoring. On routes
  the catalog prices exactly, such as DeepSeek's native endpoints, each
  authoring call is recorded with its catalog cost estimate. Other routes are
  not observed that way.
- To bound authoring, state an amount in the request, for example
  `… budget 0.50 dollars`. On an exactly priced route, each call reserves its
  worst case at the catalog price, and the Session sends nothing the amount
  cannot cover. Routes it cannot price that way are refused before any paid
  call: other providers, custom gateways, streaming, tools, subscriptions and
  unpriced local computation.
- Authoring with a provider model is bounded per call: 180 seconds, an initial
  limit of 16,384 output tokens that can rise to 32,768 after a truncation, and
  at most three repairs. A call whose delivery is uncertain is not replayed.
- Catalog estimates are not invoices. Your provider's bill is the reference.
- The run ceiling is separate from authoring (see above).

**Run provider limits in 0.121.0.** A price shown in the model catalog is not
enough to admit an OpenAI-compatible API route automatically. The qualified
DeepSeek direct route is admitted. Other routes, including default OpenAI and
Mistral endpoints, require a fresh Run cost review if their exact route and
model are not admitted. Scripts and CI invocations without a review channel
refuse before dispatch. The production Serve backend also refuses those routes:
HTTP jobs are queued normally, then settle as `failed` / `admission_refused`
before any model or tool effect. Resident schedule fires use the same gate. The terminal Session and interactive local
`nika run` can ask for this choice; an explicit `--cost-review-stdio` host must
implement the review exchange. A workflow's saved ceiling is not that choice.

Unknown-price review is limited to bounded HTTPS OpenAI-compatible text calls.
HTTP API overrides and native Anthropic/Gemini gateway overrides refuse even
interactively. Catalog-priced native models at their default endpoints and
explicit local-provider lanes follow separate admission rules. These limits
apply to Run, independently of which provider authored the workflow.

## Optional operator settings

The Session reads these from the environment when it opens. Set them in the same
startup file as your key. The walkthroughs above need none of them.

| Variable | Effect |
|---|---|
| `NIKA_AUTHORING_STRATEGY` | When the model writes the workflow itself: `escalate` (default), `only`, `sketch` or `off` |
| `NIKA_KNOWLEDGE` | An optional Foundry release root; the environment carries no trusted identity, so a named root is refused with `ADMISSION_UNTRUSTED`. Unset uses the embedded release; the exact word `off` disables knowledge |
| `NIKA_KNOWLEDGE_EXCLUDE` | A corpus whose examples are never recalled from the selected release, including the embedded default |
| `NIKA_SESSION_DECISION_MODEL` | An optional decision model; only `typesafe/<model>` (Jev) is supported, with `TYPESAFE_API_KEY` |

**Knowledge releases.** By default, the Session admits the release embedded in
its binary against the build's trusted identity and pins it when it opens.
`/status` names that identity and its embedded origin. The release contains three
blocks linked to three patterns. Matching words in an intention select patterns
and their blocks as reference material; an intention with no lexical match adds
none. The release contains no examples or repair principles. The record
distinguishes composed references from those presented in an authoring
instruction; neither proves a better result.
Deterministic authoring does not present knowledge to a model.

`NIKA_KNOWLEDGE=off` disables knowledge. With authoring strategy `off` and no
source named, the choice is unread and composes nothing; naming a source under
that strategy is refused. A host embedding the Session can name a disk release
with the trusted identity from its own release record. The environment cannot
supply that identity, so a root named only by `NIKA_KNOWLEDGE` is refused with
`ADMISSION_UNTRUSTED`. A named release that lacks trust or fails admission never
falls back to the embedded release. A release that changes after it was pinned
is also refused before authoring. `/details` records what was actually presented,
separately from the release available to the Session.

**Jev.** The decision model is consulted only when the compiler faces a finite
choice it cannot settle alone. For example: does a clause ask for a search, a
lookup or neither? The compiler checks the answer against the options it
offered. A compile makes at most three such calls, 20 seconds each, with no
retry. They happen only under an API authoring model with no stated spending
amount. Their cost is unknown and recorded separately, never counted as zero.
Jev does not select knowledge, write workflows or grant permissions. TypeSafe
is a third-party service with its own account and terms.

## Limits

- Nika is pre-1.0. The conversational path was qualified on six synthetic
  journeys with one installed macOS build, recorded in the
  [qualification note](../qa/delivery-a-2026-09.md). That installation also
  used a knowledge snapshot and Jev; this historical note does not qualify the
  current embedded knowledge release. It does not establish correctness for
  arbitrary requests, platforms or providers.
- Some requests cannot be expressed yet. Nika then says what stopped it and
  what could help, and writes nothing.
- A clean check describes the file's structure and boundary. It does not prove
  that the workflow does what you meant or that model output is true. Review
  each proposal, and compare model-written text with its sources.

## Upgrading and going back

- Session history written by a newer Nika can contain records an older
  executable cannot read. For example, v0.120.3 cannot read the separate record
  of a closed Run request. The older executable refuses that history instead of
  guessing. Keep the history, and reopen the project with the newer executable.
- Reinstalling an older binary does not convert history back; there is no
  reverse migration. Deleting history does not reset what earlier spending
  might have been.
