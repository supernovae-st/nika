<!-- SPDX-License-Identifier: Apache-2.0 -->
# Conventions · the contract every file in this corpus honours

> This corpus is the surface people learn Nika from. Six agents given only
> the authoring prose took 45 check-fix rounds between them and none was
> green first try; one who read two of these files wrote the next one green
> in zero rounds. **These files outteach the prose.** A wrong permits block
> here is copied. A false comment here becomes a false model in every
> workflow written after it.
>
> Applies to `examples/**` and `templates/**` in this repo — the SOURCE.
> `crates/nika-pack/pack/` in the engine repo is a vendored mirror; never
> edit it directly.

---

## §0 · The law

Every finding of the 2026-07-27/28 audit had one shape: **a component
reported on a domain it did not observe.** The law that came out of it
governs these files too:

> **A claim must either COVER what it says, or NARROW itself to what it covers.**

A permits block is a claim. A header is a claim. Every comment is a claim.

### The standard · a file is done when all seven hold

```
□ check   nika check <f> --native-strict  → rc=0, ZERO findings, ZERO hints
□ run     it actually runs · offline where possible (mock model, committed
          fixtures, no key). If it genuinely cannot, §6 says how to declare it.
□ artifact  parse what it produces · a green run writing malformed JSON is a
          failure
□ permits  the TIGHTEST block that covers the body · never widened to silence
          a message · never a root-level `**`
□ comments  every one is TRUE · measured, or citing a spec line
□ authorities  only inputs: · const: · secrets:
□ verbs   only infer · exec · invoke · agent · native-first
```

**check-green is not run-green.** Every one of the fifteen findings lived in
that gap. A file that has not been RUN is not finished.

Verify:

```bash
export NIKA_BIN=/path/to/engine/target/debug/nika-cli
"$NIKA_BIN" check examples/og-images.nika.yaml --native-strict   # rc=0, no hints
"$NIKA_BIN" run   examples/og-images.nika.yaml                   # rc=0, artifacts land
```

---

## §1 · The file header

Every file opens with the same block. It is a contract, not decoration:
a reader decides from it whether they can run the file *before* they read
the body.

```yaml
# SPDX-License-Identifier: Apache-2.0
# yaml-language-server: $schema=https://nika.sh/spec/v1/workflow.schema.json
#
# showcase · T2 chain · data → picture → report
#
# A CSV of rows becomes a rendered bar chart plus a one-page markdown
# report — the « paste-the-spreadsheet, get-the-slide » job, fully offline
# and byte-deterministic: the same CSV renders the same SVG forever.
#
# Demonstrates ·
#   - `nika:chart` · a declarative chart → a deterministic SVG artifact
#   - `nika:convert` CSV→JSON + jq `group_by` · the numbers are computed,
#     never guessed
#   - a declared `permits.fs.write` tree · the report + chart land under
#     `out/**` and nowhere else
#
# Needs · ./data/sales.csv (columns · region,revenue).
#
# Run · nika run examples/csv-chart-report.nika.yaml
```

Order, and what each line owes:

| Line | Rule |
|---|---|
| `SPDX` + `yaml-language-server` | verbatim, always, first two lines |
| **one-line identity** | `showcase · T<n> <shape> · <audience>` for showcase · `TEMPLATE · <id> · <one sentence>` for templates · `NN · <title>` for the numbered path |
| **the story** | 2–5 lines of prose. What job this does for a human. Not a feature list |
| **`Demonstrates ·`** | the constructs this file exists to teach, one bullet each. If a construct is not in this list, the file is not the place to show it off |
| **`Needs ·`** | every precondition that is NOT in the repo: fixtures, a git repo, a running daemon, an env var, a real endpoint. **Absent means: needs nothing.** §6 |
| **`Run ·`** | the exact command, `--var`s included |

A header that says `Needs ·` nothing and then dies on a missing file is the
same defect class as a permit that does not cover its body.

---

## §2 · The permits block

`permits:` is the declared capability boundary: **default-deny once present.**
The block is the wall. It cannot interpolate (`NIKA-AUTH-007`) — a bound is
a literal you can read, so a path built from a `const` is written out
literally in the permits even though the body interpolates it.

### Shape and order

Always in this order, and omit any category the body does not use:

```yaml
permits:
  exec: ["git"]                       # programs · or `false` to state the zero out loud
  tools: ["nika:read", "nika:write"]  # alphabetical · the exact ids the body invokes
  net:
    http: ["api.example.com"]         # exact host names · NEVER globs
  fs:
    read: ["./data/sales.csv"]
    write: ["out/**"]
```

- `tools:` lists exactly what the body invokes — no more (a spare entry is
  `NIKA-DRIFT-001`), no less. Sort alphabetically.
- `net.http` entries are **exact host names, never globs**.
- A file that touches nothing declares `permits: {}` — the explicit zero
  (F-O8). Leaving `permits:` out entirely is a different thing: it means
  *no boundary was declared*. Both forms refuse an effecting tool at run,
  with different verdicts — measured on `nika-cli 0.107`: an absent block
  reports `NIKA-AUTH-006` (no boundary to judge against) · `permits: {}`
  reports `NIKA-SEC-004` (outside the boundary you drew). Declaring the
  zero is also what puts the PERMITS and TRIFECTA judges on duty —
  under-declaring quietly stands them down (01-hello teaches this split).

### The wildcard table · measured, 2026-07-28

Globs that used to grant a whole subtree no longer do. These are the exact
semantics, probed against `nika-cli 0.106.0`:

| Grant | Covers | Does NOT cover |
|---|---|---|
| `data/**` | `data`, `data/a.csv`, `data/sub/deep.key` — any descendant, any depth | — |
| `data/*` | `data/a.csv` — exactly ONE level | `data/sub/a.csv` · `*` never crosses `/` |
| `data/*.csv` | `data/a.csv` | `data/sub/secret.key` |
| `data` | `data` itself | **`data/a.csv`** — a directory grant does not cover its children |

The last row is the one that bites. Two shipped fail-opens came from
assuming otherwise (a permit naming CSV files reached a private key three
directories down; a permit naming markdown wrote a shell script into a
subdirectory). Both are fixed. **Wildcards now mean what they say.**

Where the path is a literal, `check` catches a miss (rc=2) and the run
agrees (`NIKA-SEC-004`). Where the path is interpolated, `check` cannot
see it — that is the whole check-green-run-dead gap, and the reason §0
requires a run.

### The boundary is enforced inside agent loops too

An `agent:` grants tools with its own `tools:` list, but every call still
passes the workflow boundary. Measured: an agent granted `nika:read` under
a `permits:` with no `fs:` fails
`NIKA-SEC-004 · agent tool "nika:read" refused by the security boundary`.
An agent that reads files needs a real `permits.fs.read` covering them.

### Comment the non-obvious bounds

When a bound is tighter than a reader expects, say why — the comment is
part of the teaching:

```yaml
  fs:
    # One brief per topic, written DIRECTLY in ./briefs/. `*` matches a single
    # path segment and never crosses `/`, so a topic carrying a slash is
    # refused instead of steering the write into a subtree — which is exactly
    # why this is `./briefs/*.json` and not `./briefs/**`.
    write: ["./briefs/*.json"]
```

---

## §3 · Flow vs capability · the secrets-and-hosts rule

**`egress:` is NOT a network grant.** The spec is explicit
(`spec/01-envelope.md:377`, layer ③): *"`egress:` NARROWS the capability
boundary, never widens it. `host_from_self` (host unknown statically)
degrades to the runtime `permits` check."*

```
egress:            sanctions the FLOW        may this secret go there
permits.net.http   grants the CAPABILITY     may this workflow reach that host
```

**A webhook send needs BOTH.** Measured, all three cases:

| `permits.net.http` | check | run |
|---|---|---|
| absent | rc=0 ✅ | ✖ `NIKA-SEC-004 · hooks.slack.com resolves outside the declared net.http boundary` |
| names a different host | rc=0 ✅ | ✖ `NIKA-SEC-004` — same |
| names the webhook host | rc=0 ✅ | ✔ the send is attempted |

The correct shape — the secret carries the URL, so the host is not knowable
at check time and is judged at RUN against this list:

```yaml
secrets:
  oncall_webhook:
    source: env
    key: ONCALL_WEBHOOK_URL
    egress:                       # sanction the one send · the secret IS the URL
      - to: "nika:notify"
        host_from_self: true
permits:
  tools: ["nika:notify"]
  net:
    # `host_from_self:` above sanctions the FLOW (the secret may be the URL) —
    # it does not grant the capability. The host stays unknown at check, so it
    # is judged at RUN against this bound: name the escalation host here or the
    # send is refused mid-run, after the tokens are already spent.
    http: ["hooks.slack.com"]
```

That comment is the canonical wording. Copy it.

Because this failure is invisible to `check` and only fires when the gate
actually opens, a workflow whose alert path is closed on the offline
rehearsal will look green forever and die the first day it has something
real to say.

---

## §4 · Directory-writing builtins need the directory AND its children

For `nika:image_generate` and its family, `check` judges the `output_dir:`
**argument** while the run gates every **final file path** under it.
Measured:

```yaml
# ✖ check rc=0 · run ✖ NIKA-SEC-004
#   ./out/assets/asset-mock-…-0-f67f1b78.png resolves outside permits.fs.write
  fs: { write: ["./out/assets"] }

# ✔ check rc=0 · run ✔
  fs:
    write:
      # Two entries, and both earn their place: `check` judges the `output_dir:`
      # ARGUMENT (`./out/assets`), while the RUN gates every FINAL file path under
      # it — the asset, its provenance manifest, and manifest.json. Grant only the
      # directory and the file sails through check, then dies at run on the first
      # asset. `*` is one segment and never crosses `/`, which is all this needs:
      # image_generate lands its files flat, so no subtree grant is warranted.
      - "./out/assets"              # keep in step with const.out_dir
      - "./out/assets/*"            # the files that land inside it
```

`./out/assets/**` also works and is what you want when the builtin nests
output. Prefer `*` when the builtin lands files flat — narrower is the
whole point.

---

## §5 · JSON artifacts are built as a VALUE, never typed as text

Never type JSON braces around an interpolation. Measured:

```yaml
# ✖ emits: { "title": a title: with a colon, "n": 2 }   ← unquoted · not JSON
  invoke:
    tool: "nika:write"
    args:
      path: out/by-hand.json
      content: |
        { "title": ${{ const.title }}, "n": 2 }
```

Build the object as a value and let the engine serialize it:

```yaml
# ✔ emits: {"n":2,"title":"a title: with a colon"}      ← valid JSON
  shape:
    invoke:
      tool: "nika:jq"
      args:
        input: { title: "${{ const.title }}", n: 2 }
        expression: "."
  save:
    with:
      shape: ${{ tasks.shape.output }}
    invoke:
      tool: "nika:write"
      args: { path: out/shape.json, content: "${{ with.shape }}" }
```

Rule: **a `nika:write` whose path ends `.json` passes a single `${{ … }}`
as `content:`.** Markdown is the opposite case — a `content: |` block with
interpolations is correct there, because the artifact is prose.

The corpus already honours this in all six of its JSON writes. Keep it that
way.

---

## §6 · Declaring that a file cannot run offline

Offline-runnable is the default expectation. Reach for it in this order:

1. **A mock model** — `model: mock/echo`, or `provider: mock` for media
   builtins. Deterministic, zero keys.
2. **A committed fixture** — the file the workflow reads lives in the repo.
3. **`on_error: recover:`** — the honest dry run. A fetch at a placeholder
   host resolves nowhere, a literal recovery value mirroring the API's shape
   takes over, and the same `extract:` bindings work on both paths.

When an effect genuinely prevents it, the header says **exactly which one**,
in one sentence, on the `Needs ·` line. Not "requires setup" — the effect:

```
# Needs · a git repo (the digest reads YOUR yesterday's commits).
# Needs · ollama running with the three seats pulled (qwen2.5:14b · llama3.2:3b
#   · qwen2.5:0.5b) — per-task `model:` seats ARE the bench, so the CLI
#   --model override deliberately doesn't touch them.
# Needs · a REAL sitemap URL (--var competitor_sitemap=https://…/sitemap.xml
#   — the placeholder domain resolves nowhere).
```

A blocking `nika:prompt` is its own case: the run **pauses** (exit 4, not a
failure). Say so, and give the resume line:

```
# It runs green but PAUSES — `nika run <file> --resume <trace> --answer approve=true`.
```

And a secret with no default cannot resolve: a file whose `secrets:` entry
reads an env var that is unset dies `NIKA-VAR-001 · unresolved template
reference`. If the file needs one, the `Needs ·` line names the variable.

---

## §7 · What a comment in these files may assert

A comment here is a claim, and claims are the thing that propagates. The bar:

> **Assert only what you measured, or what you can cite to a spec line.**

| ✅ Allowed | ❌ Not allowed |
|---|---|
| what a construct DOES, when you ran it | what it "should" do |
| a spec citation (`NEP-0002` · `01-envelope.md:377` · `NIKA-AUTH-007`) | a mechanism you inferred from a name |
| WHY a bound is narrow (§2) | a justification for a bound that does not hold |
| an honest limitation (`the first call pays its load time`) | a reassurance you did not test |
| a designed failure path, labelled (`the RED assert IS the lesson`) | silence about a path that dies |

The measured example of the failure mode: a repair shipped the comment
*"the webhook is NOT listed here: its host is inside the secret, and the
`egress:` above (host_from_self) is what sanctions that one send."* The spec
says the opposite. The file was green, the gate never opened on the offline
rehearsal, and the comment taught a wrong model to everyone who copied it.

If you are about to explain a mechanism, run it first.

---

## §8 · The language surface

**Three authorities. No others.** `vars:` and `env:` are dead and refuse with
`NIKA-VALUES-001` / `NIKA-VALUES-002`; the top-level `config:` block is gone
with the envelope nuke, and a value outside the three refuses with
`NIKA-VALUES-003`.

| Authority | Role |
|---|---|
| `inputs:` | a typed parameter · `required: true` the caller supplies at launch · `required: false` + `default:` is the DEPLOYMENT's knob, unreachable from `--var` |
| `const:` | a fixed value baked in · never caller-supplied |
| `secrets:` | a governed store reference · masked · `egress:`-gated |

**A typed constant is `{ type, value }` — not `{ type, default }`.** The
discriminator is normative (`01-envelope.md:273`): an object carrying BOTH
`type` and `value` IS a typed constant; **an object missing either key is a
bare literal object constant.** So `{ type, default }` is not a declaration
at all — `${{ const.x }}` yields the whole `{type, default, description}`
object. Measured: a `for_each` over one dies
`NIKA-VAR-006 · for_each collection must be an array · got object`.

```yaml
const:
  plain: ["a", "b"]                   # bare literal · the common case
  window:                             # typed constant · BOTH keys
    type: integer
    value: 30
```

**Four verbs. No others.** `infer` · `exec` · `invoke` · `agent`. Everything
callable is a tool under `invoke:` — `fetch`, recall, db, files are tools,
not verbs.

**Native-first.** The work happens in builtins, not in `exec python3 helper.py`.
`curl` is `nika:fetch`; `cat`/`mkdir` are `nika:read`/`nika:write`
(`create_dirs: true`); `jq` is `nika:jq`. The checker says so
(`native-first/001`, `/002`) and it is right.

---

## §9 · Authoring seams (paid-run class · 2026-08-19)

Measured on a real OpenAI `nika run` of a 40+ task extract → jq law →
builtins workflow. The engine now hints or accepts these; do not rediscover
them with tokens. The shape is [`13-extract-then-law`](13-extract-then-law.nika.yaml).
The named bundle is [`14-decide-publish`](14-decide-publish.nika.yaml).

1. **Integer facts, not digit strings.** `enum: ["0","1","3"]` — models emit
   JSON `3`. Prefer `type: integer`. Hint `digit-string-enum`.
2. **The model never writes the verdict.** Extract facts (`infer.schema:`),
   then `nika:jq` (or `nika:decide`) is the law. A second infer to "pick
   the level" is the expensive mistake this lesson exists to unlearn.
   Hint `infer-as-law`. `nika check --json` reports `paid_ready: false`
   until the paid-run family is gone. The MCP `nika_check` oracle fails
   `infer-as-law` and `digit-string-enum` by default.
3. **Probe a new builtin with `mock/echo` first.** `nika:inspect` is
   catalogued and unwired (`available: false`). Hint `inspect-unwired`.
4. **`nika:hash` accepts an object.** Do not pre-`tojson` a roster.
   `nika:validate` parses a string schema from `nika:read`.
5. **Pin the glob.** `held/*.md` includes `README.md`. `exclude:
   "**/README.md"`. Hint `glob-readme`.
6. **jq `. as $c` then `($c | map(...))`.** A bare `map(` after `. as $c`
   maps the current value (often a pair). Hint `jq-as-map`.
7. **A red last `nika:assert` quarantines `out/`.** Look in
   `.nika/quarantine/<trace>/`. Hint `assert-quarantine`.
8. **`--resume` on a `for_each` infer whose prompt uses `item.field`
   now cache-hits** when the collection and definition are unchanged.
   The skip is the *whole fan*, not one item.
9. **Prove the law on known answers.** `infer` → `nika:jq` /
   `nika:decide` without a const-fixture `nika:assert` is hint
   `unproven-law` · `paid_ready: false`. Shape: lesson 13 `prove` +
   lesson 14.
10. **`nika:compose` is loop-only.** Grant it on `agent.tools` after
    `nika:done`. The model drafts YAML, gets the full `nika check`
    JSON, iterates until `valid`. A standalone `invoke:` is
    `NIKA-BUILTIN-COMPOSE-001`. Checking never executes the draft.
    Shape: lesson 15. Parent→child calls stay lesson 10.

Order that is cheaper: `nika check --json --native-strict` until
`paid_ready: true` → one-task mock probe of every new builtin → freeze
the extract schema type → then wire paid infer.

---

## §10 · The one honest red · NIKA-SEC-009

Declaring a truthful boundary can light up `NIKA-SEC-009` (lethal trifecta ·
private read + untrusted ingress + external egress with no dominating human
gate). An empty `permits:` block switches the leg detectors OFF, so
**under-declaring was disabling the security judge; declaring the truth
turns it on.**

**Do not add a blocking `nika:prompt` to silence it.** That is ceremony
added to quiet a security message, on a file people copy. A gate belongs in
a file only when the job actually wants a human decision — and then it goes
at the ROOT, dominating every path to every egress-capable task, because a
gate placed after the ingress dominates nothing (see
`incident-war-room` and `templates/etl-state`, where the gate is real and
the comment explains the dominance argument).

The gate over-approximates in one known way; the decision is recorded at
`the nika engine repo · docs/plans/2026-07-28-verdict-coverage.md` (§DECIDED · SEC-009).
A file that hits it **stays RED** and carries a one-line in-file note
pointing there. A red gate reporting something true is the honest state.

---

## §11 · Verify a change

```bash
# Judge with the binary the WORK targets, not whatever the PATH serves.
# A released build can be BEHIND the tree, and the version string does not
# say so: measured 2026-07-29, a debug build reporting 0.106.0 carried an
# fs-permit fix that the brew build reporting 0.106.1 did not.
export NIKA_BIN=<your nika checkout>/target/debug/nika-cli

# 1 · check · rc=0 and the tail must read "0 hints"
"$NIKA_BIN" check <file> --native-strict

# 2 · run · in a scratch dir, with the fixtures the header promises
"$NIKA_BIN" run <file> [--model mock/echo] [--var …]

# 3 · artifact · parse what landed
python3 -c "import json,sys; json.load(open('out/thing.json')); print('VALID')"
```

The whole corpus in one pass:

```bash
for f in examples/**/*.nika.yaml templates/*.nika.yaml; do
  "$NIKA_BIN" check "$f" --native-strict | tail -1
done
```
