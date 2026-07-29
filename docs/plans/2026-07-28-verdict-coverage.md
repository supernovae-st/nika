# Verdict coverage · the false-green class

> Status: partially executed. Six findings verified on 2026-07-28
> against the 0.106.0 binary. Every repro below was run; nothing here is
> inferred from reading code alone.
>
> **Shipped since** (each verified by repro, not by reading):
>
> | Item | What landed |
> |---|---|
> | F1 step 1 | `nika check` hints when a `capture: structured` exit code is read by nobody |
> | F3 interim | the `TYPES` line narrowed to what it covers — see the correction below |
> | F4 partial | a `nika:prompt` with no `default:` now names its headless cost at check |
> | (new) | `--native-strict` wired into the two hooks, the `/nika:check` command and the `nika-author` subagent |
>
> **Correction to F3 as written below.** "Nothing — no builtin declares
> an output shape" understates the scan. `schema_typing.rs` is sound by
> construction: it validates deep references against the shapes tasks
> DO declare (`schema:` on infer/agent, `output:` bindings on jq) and
> resolves an opaque shape to "unknown — no finding", never a guess.
> The real gap is narrower and structural — there is no `output_schema`
> mechanism in the catalog at all, so a builtin CANNOT declare a shape
> and references into builtin output are unchecked. The rendered line
> was the dishonest part, and it is the part that was repaired.
>
> **The `--native-strict` wiring, and why it is not in the six.** The
> operator reported agents reaching for Python glue whenever a builtin
> refused them. The enforcement already existed and already hard-failed
> (`exec python3 helper.py` → rc=2; `exec git` passes, ledger or not) —
> nothing ran it. It is now the posture of every surface that checks on
> the author's behalf. One trap avoided: a hook that judges with the
> flag must NAME the flag in its message, or the reader re-checks with
> the bare form, reads a green the gate does not accept, and loops.

## The law this plan exists to restore

Six independent bugs turned out to be one defect wearing six coats:

> **A component reported on a domain it did not observe.**

| Verdict | What it claimed | What it actually observed |
|---|---|---|
| `TYPES` | every deep output reference fits its declared shape | nothing — no builtin declares an output shape |
| `PERMITS` | the body fits the declared boundary | literal bounds only — a `const:` bound is invisible |
| scope analysis | `item` is in scope | the TASK carries `for_each:` — not which SURFACE is being scanned |
| exec settle | the task succeeded | the capture MODE — never the exit code |
| the agent kit | this is the language | itself — it was never confronted with the engine |

The remedy generalises, and it is the acceptance criterion for every fix
below:

> **A verdict must either COVER its claim, or NARROW its claim to what it
> covers.** A green that means less than it says is worse than no green:
> it spends the reader's trust and returns nothing.

The engine already owns the right instrument, scoped to one domain — the
check ⇔ run equivalence oracle that proves the two agree on permit
decisions (0.106 · F-O6 · NEP-0007). Generalising that oracle is the
structural end-state; the items below are the path to it.

---

## F1 · `capture: structured` reports success on a failed command

**Severity: P0.** A run reported 23/23 ✔ with four failed tasks. The
error surfaced three waves later on an unrelated `jq`, so the diagnosis
pointed at the wrong task entirely.

Repro, verified:

```
capture: structured   run exit 0   ✔  s  exec · /usr/bin/false
capture: stdout       run exit 1   ✖  NIKA-EXEC-001
```

Same command, same permits; only `capture:` differs.

**This is a designed split, not an oversight.** `dispatch.rs:623-629`:
*"under `structured` a non-zero exit is DATA (the task succeeds ·
`exit_code` is the branch), under the text modes it fails the task"*.
The reasoning is coherent in isolation and indefensible in practice: it
produces a silent failure with no signal at all.

Fix, in the order that keeps every step shippable on its own:

1. **The warning first** (additive, breaks nothing, kills the silence):
   `nika check` emits a hint when a task carries `capture: structured`
   with no `nika:assert` on `exit_code` downstream. Ship this even if
   step 2 waits.
2. **The opt-in field**: `exec: { allow_nonzero: true }` — schema,
   validation, docs, tests.
3. **Flip the default** once (2) exists: non-zero fails, `exit_code`
   stays in the output for inspection. Without (2) this silently breaks
   every workflow that legitimately branches on the code.
4. Storyboard: an exec whose `exit_code != 0` deserves a distinct glyph
   even when the task is green.

Anchor: `crates/nika-runtime/src/dispatch.rs` · `settle_exec_out` returns
`Dispatched::ok` unconditionally under `ExecValue::Structured`.

---

## F2 · SEC-009 rewards leaving the native path

**Severity: P0 — the worst of the six, because it teaches the wrong
reflex to every agent that meets it.**

A pipeline of `fetch metadata → write a template → CLI renders → CLI
verifies its own output` cannot be written natively: the CLI must read
the artifact it just wrote, `fs.read` on the workflow's own scratch
counts as "private read", and the trifecta completes. Eleven findings.

The perverse incentive is the finding: **replacing `nika:fetch` with
`exec curl` makes SEC-009 disappear**, because `exec` ingress is not
counted as untrusted. The gate pushes authors off the native path — the
exact inverse of native-first.

**Confirmed in the source, 2026-07-28.** `content_flow.rs:140` —
`RawAction::Exec(_) => (false, true)`: an exec is `writes_fs`, never
`born_ingress`. The compensating rule two frames up
(`content_flow.rs:80`) re-taints an exec that reads a file a tainted
writer already produced, under a declared `fs.read` — the file-mediated
channel argv cannot see. It does NOT make `exec curl` an ORIGIN of
untrusted content, so leg ① of the trifecta simply does not hold and
SEC-009 goes quiet.

**The incentive is closed; the classification is not.** Measured on the
same pair of workflows:

| | bare `check` | `--native-strict` |
|---|---|---|
| `nika:fetch` (native) | 0 | **0** |
| `exec curl` (the bypass) | **0 — SEC-009 silent** | **2 · native-first/001** |

So under `--native-strict` the native form passes and the bypass fails:
the payoff points the right way again. Since that flag is now the
posture of both hooks, the `/nika:check` command and the `nika-author`
subagent, an author cannot reach the bypass through any surface that
checks on their behalf — and cannot run the file either, because the
run gate uses the same flag.

This is a fix to the INCENTIVE, not to the classification. An author
who never passes the flag still gets a silent bypass, so the two
repairs below stand. It does buy the time to make them carefully: a
security-semantics change that adds findings to workflows already in
the field is exactly the kind that should not ship unannounced.

Two independent repairs, both needed:

- A path that appears in `fs.write` is the workflow's own scratch and
  must not count as a private read. Either infer it, or give it a name
  (`fs.scratch:`) so the intent is authored rather than guessed.
- Count `exec` ingress (curl/wget and friends) as untrusted, so the
  bypass stops paying.

And the diagnostic must offer BOTH doors — the human gate AND the
scratch classification — instead of only "gate the egress path".

Secondary, same family: declaring `fs.write` alone silently sets
`fs.read` to deny-all. CLIs read what they write; the refusal should say
so.

---

## F3 · `TYPES` is true by vacuity

**Severity: P0 (architectural).** `nika check` prints *"every deep output
reference fits its declared shape"*. Measured: **0 of 28 builtins declare
an output shape**. Nothing can fail to fit what nothing declares.

Consequence, verified: `tasks.bill.output.total_usd` passes check and
dies at run, because `nika:inspect view: cost` answers
`{available: false, reason, view}` until the runtime exposes live cost.
Our own showcase taught that field and the stdlib promised the shape;
both are repaired on main, but the class is not.

The fix is one rule, and it closes the class rather than the instance:

> **A builtin declares its output shape, and the TYPES layer validates
> deep references against it.**

`nika:inspect view: cost` is the case that proves the design: its honest
shape is a UNION (`{available:false,…}` | `{total_usd,…}`), which makes
`.total_usd` refuse without a guard — exactly right. This mirrors the
`outputSchema` MCP standardised for the same problem.

Until it lands, `TYPES` must narrow its claim: say that deep references
into builtin output are unchecked, rather than implying they passed.

---

## F4 · The human gate is unusable headless on a first run

**Severity: P1.** `nika run --answer confirm=true` is refused with
"--resume required", so a gated workflow needs two runs; stdin piping
fails with `NIKA-BUILTIN-PROMPT-001`; and giving the prompt a `default:`
disqualifies it as a dominating gate, which brings SEC-009 back. Net
effect: **a gated workflow cannot run in CI in one pass.**

Fix: accept `--answer <task>=<value>` on an initial run as a
pre-answer, or an explicit `--assume` recorded in the journal. The
answer must stay visible in the trace either way — a gate that can be
satisfied invisibly is not a gate.

Related, cheap, and worth doing with it: the dominance diagnostic names
the egress task, not the undominated PATH. An operator had to hang
`after: {confirm: success}` on a `nika:date` task with no effects to
silence the last finding. Name the path (`stamp → pack`), not the
symptom.

---

## F5 · Failure reporting points at the wrong task

**Severity: P1.** The run summary cites the LAST error, not the causal
one, so a run with two failures does not say which one started it.
Depth-first — the earliest failure in the DAG — is the honest default.

And `nika trace peek <trace> <task>` on a SKIPPED task lists every task
that has an output, when the one thing wanted is the REASON for the skip
(which `on_error` arm fired, and the original error code).

---

## F6 · The banner repeats ~50× in an interactive terminal

**Severity: P2.** Reported in iTerm/zsh, not reproducible through a
pipe; the capture shows `[?2026h` / `[28A` / `[0J` sequences, so a
re-render that fails to clear. Suspect a resize during the run.

---

## Order of work

The sequence is chosen so each step is independently shippable and
nothing depends on a decision that has not been made:

| # | Work | Blast radius | Gate |
|---|---|---|---|
| 1 | F1 step 1 — the `capture: structured` hint | additive | none |
| 2 | F3 — narrow the `TYPES` claim in the report | wording | none |
| 3 | F2 — scratch classification + count `exec` ingress | check semantics | needs the scratch shape decided |
| 4 | F1 steps 2-3 — `allow_nonzero:` then flip the default | **breaking** | operator |
| 5 | F3 — output shapes on 28 builtins + TYPES enforcement | large, additive | operator |
| 6 | F4 — headless pre-answer | CLI surface | operator |
| 7 | F5 — causal-first failure, skip reason | reporting | none |

Steps 1, 2 and 7 need no decision and can start immediately. Step 4 must
not ship before its opt-in exists, or it silently breaks every workflow
that branches on `exit_code`.

## The end-state

Generalise the check ⇔ run equivalence oracle beyond permits: for a
corpus of workflows, assert that no run outcome contradicts its check
verdict. Every finding above is an instance of that contradiction, and
a corpus-level oracle is what makes the seventh one impossible to ship
unnoticed — which is the only guarantee worth having, since all six of
these shipped past reviewers who were reading carefully.

---

# F7 · the cost ceiling prices half the bill

Found 2026-07-28, after the six above, while checking a claim about
`max_turns` that turned out to be wrong. The correction found this.

## What was claimed, and refuted

I asserted that `07-conformance.md`'s cost ceiling was false because an
`agent:` loop is unbounded. **Refuted at two lines:**

- `spec/02-verbs.md:328` — `max_turns` carries a **default of 10**.
- `crates/nika-check/src/cost.rs:141` — an `agent:` with no
  `max_tokens_total` resolves to `UnboundedReason::NoTokenLimit`, and
  the report says `est unbounded`. The engine was honest.

The claim was wrong. Measuring it found the real one.

## The defect

`crates/nika-check/src/cost.rs:144-148` computes

```rust
let per_call = (tokens as f64) * price / 1_000_000.0;
```

where `price` comes from `output_price_per_million`. And
`spec/02-verbs.md:93` defines the field it multiplies:

> `max_tokens` — Max **output** tokens

**`input_per_million` has zero occurrences in `crates/nika-check/src/`.**
The prompt is not in the ceiling. Not underweighted — absent.

## The measurement

```yaml
grab:      invoke: { tool: "nika:fetch", args: { url: models.dev/api.json, mode: raw } }
summarise: infer:  { model: anthropic/claude-sonnet-4-5, max_tokens: 500,
                     prompt: "Summarise this.\n\n${{ with.body }}" }
```

| | |
|---|---|
| document | 3 275 066 bytes |
| ≈ input tokens | 818 766 |
| `input_per_million` for that model | **3.0** — in the catalog, four lines above the output price the ceiling reads |
| real input cost | **$2.4563** |
| what check prints | `✔ COST  $0.0075 – $0.0075 worst-case ceiling` |
| ratio | **328×** |

The line carries a ✔ and the words *worst-case*.

## The second defect in the same repro

818 766 tokens against a 200 000-token context window: the call is 4.1×
over and would be refused by the provider. Nothing in the report says
so. `limit.context` is available for 653/653 of our models upstream and
is not read either.

## Why it is structural, not an oversight

The ceiling is defined over what the AUTHOR declares (`max_tokens`), and
the input size is a property of what the workflow FETCHES — a runtime
value. Pricing it requires a static estimate of interpolated content,
which is genuinely hard. Three honest options:

1. **Narrow the claim.** The line says *output ceiling*, not
   *worst-case spend*. Zero new machinery, and the promise stops lying.
   Per the governing law this is always available and always correct.
2. **Bound what is boundable.** A literal prompt has a known token
   count; only interpolated content is unknown. Report
   `output ceiling $X · input ≥ $Y (literal) · unbounded (2 interpolations)`.
3. **Refuse the unbounded case.** An `infer:` whose prompt interpolates
   an unbounded source declares a `max_input_tokens:`, or the ceiling
   reports unbounded — the same discipline `max_tokens_total` already
   imposes on the agent loop, applied to the other half.

Option 1 ships today and is a prerequisite of the others: whatever the
machinery becomes, a verdict must not claim the whole bill while
covering one side of it. Option 3 is the end-state, and it is the same
shape as every other bound in this language — declared in the file,
checked before the run.

## Rank

**P0, above every previously ordered item.** It is money, it is a ✔, and
the number it prints is off by two orders of magnitude on a shape
(fetch a document, summarise it) that is the single most common thing a
person writes first.

---

# F8 · the security gate rewards under-declaring

Found by the catalog swarm, reproduced here end to end. **This outranks
F7 and everything before it.** It is not a verdict that covers less than
it claims — it is a verdict that inverts the incentive it exists to
create.

## The repro

Two files. The `tasks:` block is byte-for-byte identical: read a local
pin with `nika:read`, fetch upstream with `nika:fetch`, write a verdict
with `nika:write`. The only difference is one line deleted from
`permits:` — the `fs.read` grant.

```
honest         nika check --native-strict → rc=2
               ✔ PERMITS  body fits the declared boundary
               ✖ TRIFECTA [NIKA-SEC-009] lethal trifecta complete · human gate required

underdeclared  nika check --native-strict → rc=0
               ✔ PERMITS  body fits the declared boundary
               ✔ TRIFECTA no lethal trifecta without a dominating human gate
               ✔ audited · 3 tasks · 2 waves · permits declared · est ≤$0.0000 · 0 hints
```

The author who declares the truth is blocked. The author who declares
less than the body does gets a green audited card.

## And the pass is worthless

```
underdeclared  nika run → rc=1
               ✖ pin  invoke · nika:read  3ms
               ✖ NIKA-SEC-004 · `./crates/nika-catalog/data/model-pricing.toml`
                 resolves outside the declared permits.fs.read boundary
```

The workflow that passes the security gate **cannot run at all**. It
dies on its first task. So the gate is not trading safety for
convenience — it is passing something broken and blocking something
that works.

## The two defects, and their single root

> **CORRECTION, same session, before this shipped.** The first version of
> this section read *"PERMITS does not statically resolve literal paths
> against the declared globs."* **That is wrong, and measuring it is what
> found the real defect.** With a genuine literal the module works exactly
> as its own header documents:
>
> ```
> path: "./crates/nika-catalog/data/model-pricing.toml"
> → ✖ PERMITS [NIKA-SEC-004 · fs] task `pin` · `nika:read` path
>   `./crates/…` is outside permits.fs.read
>   fix: add "./crates/…" to permits.fs.read
> ```
>
> My repro wrote the path as `${{ const.pin }}`, which is not a literal to
> this scanner. The claim below is the corrected one, and it is narrower
> and far more tractable than what I first wrote.

**(1) A `${{ const.x }}` path is treated as DYNAMIC when it is provably
STATIC.** `permits_fit.rs` says so in its own header — *"A path/host built
from a `${{ }}` value is dynamic and stays the runtime `NIKA-SEC-004`
check"* — and for `inputs:` or a task output that is correct. For
`const:` it is not: `const` is a static authority, its value is in the
file, and no runtime input can change it. The checker can resolve it.

**The precedent is already in this codebase.** `cost.rs`
(`static_vars_array_len`) resolves exactly this shape — a bare
`${{ <authority>.<name> }}` over a literal — to decide whether a
`for_each` count is statically known. One rung resolves const-backed
expressions; the other defers them to runtime. The asymmetry is the bug.

Measured in both directions: deleting `fs.write` while keeping a
`nika:write` to a const-backed path passes identically.

**(2) TRIFECTA reads leg ① off the DECLARATION, not the BODY.** Private
read is taken from `permits.fs.read`. Delete the grant and the leg
disappears, while the `nika:read` stays in the body doing the same
thing.

They compose into the inversion: (2) makes under-declaring profitable,
and (1) makes it free **for the shape everyone actually writes** — a path
in `const:`, referenced once, which is what every template and every
example teaches.

**One fix closes both, and it is small.** Resolve
`${{ <static-authority>.<name> }}` in `permits_fit` the way `cost.rs`
already resolves it, then the under-declared file fails PERMITS and can
never reach TRIFECTA with a short leg. Note what the literal case above
shows: it fails PERMITS while TRIFECTA still reports `✔ no lethal
trifecta` — the file is blocked, but by a different rung than the one
that should have caught it. The gate holds by accident there, and not at
all when the path goes through `const:`.

Fixing (2) as well — reading leg ① off the BODY rather than the
declaration — is the belt to that suspender, and is the general law
below.

## Why the shape recurs

This is the fourth instance in this record of the same mechanism:
a verdict computed over what the author DECLARED rather than what the
body DOES.

- `05-fetch-chain` and `t3-localization-factory` — two shipped examples
  pass `--native-strict` claiming *pure compute · nothing escapes*, then
  die at run with `NIKA-SEC-004`, because a `url:` or `path:` written as
  `${{ const.x }}` hides the effect from the static gate while a literal
  would not.
- F7 above — the ceiling prices `max_tokens` (declared) and not the
  prompt (what the body sends).
- F8 here.

The generalisation is worth stating as a law of this checker:

> **Every gate must read the BODY. A gate that reads the declaration is
> gating the author's honesty, not the workflow.**

## Rank

**P0, top.** It is a security gate, it is inverted, and the two shipped
examples sitting in the quiet half of the same hole mean we are teaching
the shape.

---

# F11 · the declared boundary granted more than it declared · SHIPPED

Found by an adversarial research swarm sent to check whether F9 was the right
fix. It was not the right fix, and it was not the important bug. **This one
outranks every finding in this document.**

Measured on the published `nika 0.106.1` — Homebrew, npm, Docker — with no
attacker, no symlink, and no path traversal.

## The two proofs

```
permits.fs.read:  ["data/*.csv"]     args.path → data/sub/deeper/private.key
  nika check → ✔ PERMITS body fits the declared boundary · ✔ audited · 0 hints
  nika run   → ✔ 1/1 done
  and the file's contents are in the SIGNED TRACE:
  .nika/traces/2026-07-28T22-26-47Z-e3ef.ndjson

permits.fs.write: ["out/*.md"]       args.path → out/sub/pwned.sh
  nika check → ✔ PERMITS · ✔ audited · 0 hints
  nika run   → ✔ 1/1 done
  ls out/sub → pwned.sh, on disk
```

A permit naming CSV files read a private key three directories down. A permit
naming markdown at the root of `out/` wrote a shell script into a subdirectory
it never mentioned.

## The cause, at the line

`crates/nika-builtin/src/permits.rs` · `literal_root()` walked the glob's
components, stopped at the first one containing `*`, and returned
`(literal_prefix, true)`. The `true` was the whole of what survived: **the
pattern itself was discarded.** `confines()` then admitted anything under the
resolved prefix. `data/*.csv` meant `data/**`, and the extension was
decoration.

Its own comment stated the behaviour plainly —
`// <root>/** · <root>/* etc. — any descendant of the real root` — which is
why a unit test asserted it rather than caught it:

```rust
assert_eq!(literal_root("/var/log/*.log"), ("/var/log".to_owned(), true),
           "a *.ext segment ends the literal prefix");
```

The static side was wrong differently: `glob_matches` was a trailing-star
prefix test, so `data/*` also crossed `/`, and any glob whose star was not
final (`*.csv`, `data/*.md`) matched nothing at all — a silently inert grant.
Two implementations, two different wrong answers.

## Why the guard that exists for exactly this did not fire

A differential proptest sits in the same file, comparing the static predicate
against the runtime enforcer on the grounds that "they share no code, so a
common bug would have to be born twice." It ran green. Its generator emitted
only `<segs>/**` and literal paths, and its doc comment gave the reason:

> Mid-pattern globs like `a/*/b` are a KNOWN, documented non-decidability ·
> `crate::effect` states "glob-pattern ⊆ permits-glob inclusion is not soundly
> decidable" · the runtime uses prefix-containment there · so this differential
> is scoped to the forms that DO decide.

**The theorem is true and it is about a different question.** Deciding whether
one PATTERN is contained in another is genuinely hard. Both sides here match a
CONCRETE resolved path against ONE pattern, which is ordinary glob matching and
entirely decidable. A correct theorem was used to justify a shortcut on a
problem it does not govern, and the exclusion it justified is exactly where the
fail-open lived.

That is the transferable lesson, and it is sharper than the bug:

> **When a proof obligation is waived as undecidable, name the decision
> procedure it would have needed. If you can write it down, it was decidable
> and the waiver is a hole.**

## The fix

One predicate, `nika_cap::glob_admits`, segment-aware, `*` never crossing `/`,
used by BOTH sides — the arrangement hosts have always had via
`nika_types::net::host_glob_matches`. `nika-cap` moves from a dev-dependency of
`nika-builtin` to a real one. `literal_root` returns the tail instead of a
bool; `confines` resolves the prefix against the filesystem (unchanged — that
is the part that legitimately differs) and then re-applies the tail to the
remainder. The differential generator now emits `*`, `*.csv`, `*/x` and `*/**`.

Verified after, check and run agreeing on every row:

```
  data/*.csv  data/sub/deeper/private.key   refused / refused
  data/*      data/sub/deeper/private.key   refused / refused
  data/*.csv  data/sales.csv                admitted / allowed
  data/*      data/sales.csv                admitted / allowed
  data/**     data/sub/deeper/private.key   admitted / allowed
  out/*.md    out/sub/pwned.sh              refused / refused · nothing written
```

## What this says about F8, F9 and F10

They were real and they are fixed, but every one of them was **fail-closed** —
friction, refusing what should pass. I spent the session on the side of the
ledger that costs authors rounds, and the side that costs users secrets was one
function away in a crate I had already opened. The research pass that found it
was sent to check my fix, not to look for this.

**Send the adversarial pass before the fix feels finished, not after.**

---

## F12 · the contradictory-advice pair · CLOSED by F11, verified

The research pass flagged a third defect alongside the two fail-opens: on one
input the report printed two instructions that contradicted each other.

```
✖ PERMITS  [NIKA-SEC-004 · fs] path `./data/sales.csv` is outside permits.fs.read
           · fix: add "./data/sales.csv" to permits.fs.read
↳ HINT     [NIKA-DRIFT-001 · drift] `permits.fs.read` entry `data/**` matches no
           path the body reads — remove the entry
```

One line said widen the boundary; the next said delete the grant that was
already correct. Both were downstream of the matcher: `data/**` genuinely did
not match `./data/sales.csv`, so SEC-004 fired AND the drift detector correctly
observed that the entry matched nothing.

Re-measured after the shared predicate landed, same input:

```
✔ PERMITS  body fits the declared boundary
✔ audited · 1 task · 1 wave · permits declared · est ≤$0.0000 · 0 hints
```

No repair of its own was needed. Worth recording because the failure mode is
not "a bad message" — it is a matcher defect surfacing as ADVICE, and the
advice pointed at widening a security boundary. A diagnostic is only as sound
as the predicate underneath it, and a wrong predicate does not stay quiet.

---

# F13 · a `nika:notify` send passes check and is refused at run

Reported by a repair agent mid-swarm, reproduced here.

```yaml
secrets:
  hook: { source: env, key: H, egress: [{ to: "nika:notify", host_from_self: true }] }
permits:
  tools: ["nika:notify"]        # and NO net.http at all
tasks:
  send: { invoke: { tool: "nika:notify", args: { channel: webhook, target: "${{ secrets.hook }}", message: "x" } } }
```

```
CHECK : ✔ PERMITS  body fits the declared boundary
RUN   : ✖ NIKA-SEC-004 · `hooks.slack.com` resolves outside the declared net.http boundary
```

Measured across all three arms — `net.http` absent, empty, and naming a
DIFFERENT host — check is green and run is refused in every one. Seven
showcase files in the spec are in this state.

## The decidable half

The naive reading is that this is unknowable statically: the host lives inside
a secret, so `check` cannot know it. That is true of one question and not the
other.

```
"which host?"         UNDECIDABLE at check · the secret carries it
"is there ANY host?"  DECIDABLE · an empty or absent net.http means every
                      host is outside it, so the run CANNOT succeed
```

The first two arms are certainties, not guesses. A net-effecting builtin
invoked under a `net.http` that grants nothing is a guaranteed run failure and
belongs in PERMITS as a finding. The third arm stays a hint, because there the
static pass genuinely does not know.

This is the F11 lesson used as a tool rather than learned again: when a check
is waived as undecidable, look for the sub-question that IS decidable and write
its procedure down. One engine repair kills the class; seven file repairs would
grow back.

## The teaching defect underneath it

One repair, caught by a sibling agent before it shipped, added this comment:

> The webhook is NOT listed here: its host is inside the secret, and the
> `egress:` above (`host_from_self`) is what sanctions that one send.

**False**, and the spec says so at `01-envelope.md:377`: *"`egress:` NARROWS the
capability boundary, never widens it. `host_from_self` (host unknown
statically) degrades to the runtime `permits` check."*

Two blocks, two questions:

```
egress:            sanctions the FLOW       may this secret go there
permits.net.http   grants the CAPABILITY    may this workflow reach that host at all
```

A webhook send needs BOTH. There is no configuration in which naming the host
is optional. This sentence goes in the authoring skill — a wrong model taught
in an example people copy costs more than a wrong file.

Cheap verification without a real webhook: point the env var at a host outside
`net.http` and run. `NIKA-SEC-004` means the boundary refused it;
`NIKA-BUILTIN-NOTIFY-002` (DNS) means it got through.

---

# F14 · the edit hook judged with the wrong binary, silently

`.agents/plugins/nika/scripts/check-on-edit.sh` resolved `NIKA="${NIKA_BIN:-nika}"`.
With the variable unset that is the PATH build — which, mid-session, was one
release behind the tree and still deferred `${{ const.x }}` paths to run time.
Same file, same flag:

```
brew 0.106.1  ✔ PERMITS  body fits the declared boundary
engine main   ✖ NIKA-SEC-004 ./secret/keys.txt is outside permits.fs.read
```

The hook fires on its own after every edit. So agents working ON the engine
were handed a green from the release they were in the middle of fixing, on
exactly the class the fix addresses, with no way to see which binary answered.

**Fixed structurally, not by asking people to remember an env var.** The hook
now names its oracle in every finding, and breaks its silence in exactly one
configuration: a build exists in this tree and the hook is not using it.
Verified both ways — inside the engine tree with `NIKA_BIN` unset it prints the
export line; outside any such tree it stays quiet even on a clean verdict.

This is the governing law applied to the tooling rather than the workflows: a
verdict that does not name the oracle behind it claims more than it covers.

---

# F15 · `--infer-permits` and `check` disagree about the same file

Mine, and a consequence of scoping the const-resolution lane to `permits_fit`
and not `permits_infer`. In one binary:

```
check           calls a `${{ const.x }}` path "a literal path" and judges it
--infer-permits calls the same path "too dynamic to pin statically"
```

So the block `--infer-permits` prints never round-trips clean on any file that
routes a path or url through `const:` — which is every file the templates teach.
The tool contradicts itself, and the half that is wrong is the half authors
paste. Wiring `judgeable_arg` into `permits_infer` closes it.

---

# OPEN · the lethal-trifecta gate may over-approximate, and two agents hit it independently

Not a finding: a question that needs an owner, with the evidence attached.

Declaring an HONEST boundary can now complete the trifecta on files that were
green only because they declared nothing. An empty `permits:` block switches off
legs ① and ③, so under-declaration was DISABLING the security judge. Fixing the
under-declaration turns it on — which is correct in general and looks wrong on
these two files.

Both agents refused to add a blocking `nika:prompt` to silence it, and both were
right to: adding ceremony to quiet a security message is the failure mode, and on
a canonical example it teaches the reflex to everyone who copies it.

The argument that the firing is an over-approximation, as the agents made it:

- **Leg ①** is satisfied by a workflow reading its OWN state file. NEP-0002 says
  so itself: *"v2 refinement: a sensitivity classification over read paths; v1
  treats any declared read as ①."*
- **Leg ③** is satisfied only via `net.http`, whose sole consumer is an INBOUND
  `nika:fetch`. Probed: three legs plus a fetch with no write sink comes back
  clean, so a fetch is never itself the egress witness. Yet the witness chosen is
  an in-workspace `nika:write` — which leg ③'s own text excludes
  (*"a `permits.fs.write` glob ESCAPES the declared workspace"*). The workflow
  has no path by which data leaves the machine.

Three options, and this is an operator decision because it changes what the gate
means:

```
(a) apply leg ③'s workspace test to witness selection
    release-radar goes green with zero file changes · agents recommend this
(b) land the NEP-0002 v2 sensitivity classification over read paths
    the principled fix · larger
(c) accept the gate and add the human prompt
    turns an unattended weekly radar into a two-command ceremony
```

Left RED pending the decision. A red gate that reports something true is the
honest state; a green one bought with ceremony is not.

---

# DECIDED · SEC-009 keeps its semantics · its MESSAGE stops hiding the approximation

Two repair agents hit the lethal-trifecta gate independently by declaring an
honest boundary, both refused to silence it with ceremony, and both recommended
narrowing the gate. I verified their probe and then read the spec, and the
verdict splits: **the mechanism they describe is real, the conclusion is not.**

## What the probe actually shows

```
① fs.read + ② nika:fetch + NO write sink          → ✔ TRIFECTA clean
① fs.read + ② nika:fetch + ③ write to ./out/**     → ✖ NIKA-SEC-009
```

So a `nika:fetch` is never itself the egress witness, and the witness selected
is an in-workspace write. That much is exactly as reported.

## Why the recommended narrowing is wrong

`NEP-0002:59-60` defines leg ③ as a disjunction over the DECLARED BOUNDARY:

> **③ external egress**: `permits.net.http` is non-empty, OR a
> `permits.fs.write` glob escapes the declared workspace, OR `permits.exec`
> is enabled.

Leg ③ IS satisfied in the probe — by the FIRST disjunct. `net.http` is
non-empty, so the workflow can reach the network. The agents read the second
disjunct ("escapes the workspace") as the definition and concluded the witness
was illegal. It is not: the capability comes from `net.http`, and the write is
the task the tainted content reaches.

Their sharper point survives that correction: **a fetch with a LITERAL url
cannot carry data out.** The capability is inbound-only in that shape. So the
gate over-approximates — not because the witness is illegal, but because
`net.http` non-empty is a coarse proxy for "can send", and a literal-url fetch
is not a send.

## The decision, and it is none of the three options offered

Not (a) narrow the witness selection · not (b) land the v2 classification now ·
not (c) accept it silently.

> **The gate keeps its semantics. Its MESSAGE stops hiding which
> approximation produced the finding.**

Today it says *"private read + untrusted ingress + external egress are all
permitted"* — a sentence that names none of the three disjuncts and does not
say which task was picked as the witness or why. An author cannot see the
over-approximation, so their only move is ceremony.

The message must name:

- **which disjunct** satisfied leg ③ (`net.http non-empty` · `an escaping
  fs.write glob` · `exec enabled`), and
- **the witness task**, with the fact that a non-escaping write was selected
  because the boundary carries egress capability from elsewhere.

Then the author reads *"leg ③ via net.http, and every fetch in this file has a
literal url"* and knows precisely what they are looking at.

## Why not narrow the gate

Because I verified the argument in one afternoon, on one shape, on a product
approaching 1.0 — and that is the exact shape of the mistake that put the
fail-open in `literal_root`: a plausible local argument justifying a narrower
check, pinned by a test, shipped. Making a security gate fire LESS is the
dangerous direction, and the burden there is a proof, not a probe.

The refinement is real and belongs in the spec, where it gets the scrutiny a
security semantics change deserves. It is now written down with its probe
attached, which is more than it had this morning.

**Consequence for the corpus**: files that hit this stay RED and carry an
in-file note pointing here. A red gate reporting something true is the honest
state; a green one bought with a prompt nobody will answer is not.

---

# THE CLASS · a decidable question waived as undecidable

Three occurrences in two days, found independently, in three different rungs.
That is a class, not a run of bad luck, and it now gets a name and a rule.

## The three

**F11 · the fs boundary.** The differential proptest that exists to catch
check-run drift excluded mid-pattern globs, citing *"glob-pattern ⊆ permits-glob
inclusion is not soundly decidable"*. True — about containment between two
PATTERNS. Both sides matched a CONCRETE path against ONE pattern, which is
ordinary glob matching. **The waiver was where the shipped fail-open lived.**

**F13 · the notify host.** A `nika:notify` whose target is a secret passes check
and dies at run. The host is genuinely unknowable statically, so the whole
question looked closed. It is not:

```
"which host?"         UNDECIDABLE · the secret carries it
"is there ANY host?"  DECIDABLE · an empty net.http means every host is
                      outside it, so the run CANNOT succeed
```

**Oracle finding #27 · a tool-authority conjunct.** Check drops a decidable
conjunct when the argument is dynamic — the same move, in a third rung, found
by the oracle built to find exactly this.

## The rule

> **When a proof obligation is waived as undecidable, NAME the decision
> procedure it would have needed. If you can write it down, the waiver is a
> hole.**

And its corollary, which is where the leverage is:

> **An undecidable question almost always contains a decidable one. Find the
> weaker claim that IS decidable and make THAT the check.** "Which host" is
> beyond a static pass; "is any host granted" is a set-emptiness test.

## Why it keeps happening, and it is not carelessness

Every one of the three was written by someone who understood the theory. The
theorem cited in F11 is correctly stated and correctly attributed. The failure
is not ignorance — it is that **a true impossibility result feels like a
complete answer**, so the search stops there. Nobody asks what weaker statement
survives, because the strong one is settled.

That is why this is structural rather than a matter of care. The prompt that
finds these is not "be careful", it is a question:

> *What is the strongest claim I CAN decide here, and does the code make it?*

## The ratchet

Per the house stress-to-ratchet ladder (amend a rule · new rule · hygiene
vector), three occurrences in one cycle clears the threshold. The lightest
surface that fits:

1. **Now** · this section, cited from the authoring and review surfaces.
2. **Next** · any code comment or doc that waives a check as undecidable must
   name the sub-question it considered and why that one does not survive
   either. A waiver with no named alternative is reviewable as incomplete.
3. **Later, if it recurs** · a lint over the source for the waiver vocabulary
   ("not decidable", "cannot be determined statically", "runtime concern")
   that requires an adjacent justification line. Mechanical, and only worth it
   if 2 proves insufficient.

The pattern to watch for in review: a comment that explains why something
cannot be checked, with no sentence about what can.
