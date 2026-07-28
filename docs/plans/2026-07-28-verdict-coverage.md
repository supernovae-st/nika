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
