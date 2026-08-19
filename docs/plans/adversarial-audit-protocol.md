# The adversarial audit protocol · and the yield curve

> **The question this instrument exists to answer.** On 2026-07-28 a single
> day's sweep produced fifteen numbered findings against the check/gate
> surface. That is either a system shedding known debt — in which case the
> rate falls on the next pass and the day was a one-off harvest — or a class
> that regenerates, in which case the rate stays flat and we are at a wall.
>
> **Nobody knows which.** One point is not a curve. This document is the
> instrument that makes the second point comparable to the first, so that the
> third one means something.

This is not an audit. It is the harness: how a domain is picked, what counts
as one finding, what must be measured rather than read, and how a run is
recorded so its count can be set beside another run's.

---

## §0 · Calibrate the oracle · BEFORE any probe

A finding is a statement about a binary. If you cannot name which binary
answered, you have measured nothing. This step is mandatory and it is cheap.

**The version string is not the oracle identity.** Measured 2026-07-29, on
the two binaries this repo has on disk:

```
target/debug/nika-cli   reports "nika 0.106.0"   → CARRIES the F11 fix
/opt/homebrew/bin/nika  reports "nika 0.106.1"   → CARRIES the fail-open
```

The *higher* version number carries the bug. A debug build's version string
tracks the last tag, not the tree. Two binaries whose strings differ by a
patch level differ here by a security boundary.

**Nor is the build timestamp.** The tree binary was built at `00:40:35`; the
fix landed at `01:33:25`, fifty-three minutes later. Inferring from those two
numbers that the binary predates the fix is wrong — the fix was in the working
tree, uncommitted, when the build ran. That inference was made while writing
this document and killed by the probe below. It is recorded because it is the
protocol's own first rule failing to be obeyed by its own author.

### The procedure

Identify the oracle by a **discriminating probe**: an input on which the
fixed and unfixed builds give different answers. For the fs-permit lane the
probe is three lines and takes ten seconds:

```yaml
nika: oracle-cal
permits:
  tools: ["nika:read"]
  fs:
    read: ["data/*"]          # NOT data/*.csv — see the trap below
tasks:
  peek:
    invoke: { tool: "nika:read", args: { path: "data/sub/deeper/private.key" } }
```

```
fixed    rc=2   ✖ PERMITS [NIKA-SEC-004 · fs] … is outside permits.fs.read
unfixed  rc=0   ✔ PERMITS body fits the declared boundary · 0 hints
```

**The trap, and why the probe uses `data/*` and not `data/*.csv`.** The
obvious probe — the exact repro from the F11 record — does **not**
discriminate. Both builds refuse it, for opposite reasons: the fixed one
because the pattern genuinely excludes the path, the unfixed one because its
matcher treated any non-final `*` as matching nothing at all, so the grant was
inert and everything fell outside it. A probe that returns the same verdict
from both builds for different reasons is worse than no probe: it reads as
confirmation. **Verify a discriminator discriminates before trusting it.**

### Two shell rules that void results silently

Both bit this session, and one of them bit again during this calibration.

```
✗  nika check f.yaml | tail -8 ; echo "rc=$?"     # rc is tail's. Always 0.
✓  nika check f.yaml > out.log 2>&1 ; echo "rc=$?" ; grep -E "…" out.log
```

**A gated operation is the LAST command in its chain.** A pipe, a trailing
`echo`, a `git log` after a `git commit` — each returns its own status over
the failure it hides. This produced two false "green" readings on 2026-07-28
and one during §0 calibration on 2026-07-29.

Second: `check` is a **static** audit. An expectation phrased "at run" cannot
be tested with `check`, and conflating the two produced a retracted claim of
49 broken conformance fixtures (real number after re-measurement: 25 + 24).
Name which gate you are probing, in every row.

---

## §1 · Picking a domain

A domain is a **surface with one enforcement seam** — not a crate, not a file,
not a feature. The test of a well-formed domain: a fail-open anywhere inside
it has the same shape and the same cost.

```
✓  "the fs permit boundary"     one seam · static matcher + runtime confines
✓  "the cost ceiling"           one seam · what check prices vs what runs
✓  "the trace/attestation chain" one seam · what the chain proves
✗  "nika-check"                 a crate · contains four unrelated seams
✗  "security"                   a theme · fs, net, secrets and flow are four
                                domains with four different fail-open costs
```

### Declare before probing

Write these five lines **before the first probe**, and do not edit them
afterwards. A domain that grows to fit what you found makes the count
incomparable to every other run.

```
DOMAIN      the seam, one sentence
ORACLE      binary path + discriminating-probe result (§0)
SURFACES    the enumerated files/verbs in scope · a closed list
DENOMINATOR the effort budget: agent-hours, or probes planned
EXCLUDED    what is deliberately out, and why
```

The denominator is the part everyone skips and it is the part the curve needs.
Fifteen findings in a 13-hour session with 23 dispatched agents and 15 in a
two-hour solo pass are not the same measurement, and without the denominator
nothing distinguishes them.

### Same domain or new domain — never pooled

```
SAME domain, re-swept after fixes   → measures DECAY      "is the class closed?"
NEW  domain, first sweep            → measures BREADTH    "how much is left?"
```

These answer different questions and their counts must never be added. A
combined number falls when the class closes and rises when a new surface is
opened, which means it moves for two opposite reasons and reports neither.

---

## §2 · What counts as one finding

### The unit

> **One finding = one defect that requires one repair.**

Not one symptom, not one repro, not one error message. Three consequences:

**Faces are not findings.** F11 produced two proofs (a read and a write), two
wrong implementations (static matcher and runtime `confines`), and one green
differential test. That is one finding: one predicate, one repair.

**A symptom closed by another's fix is not a separate finding.** F12 (the
contradictory-advice pair) was recorded, re-measured after F11's predicate
landed, and needed no repair of its own. It is counted as a **symptom**, in
its own column, never in the defect count.

**A defect with a teaching face is one finding, not two** — unless the
teaching artifact needs a repair the engine fix does not deliver. F13's false
comment about `host_from_self` lives in a skill; the engine repair does not
touch it. That is one finding with a noted secondary repair, and the choice is
recorded rather than hidden, because the opposite choice is defensible.

### The reproduction bar

> **A finding is REPRODUCED or it is REPORTED. The two never share a column.**

REPRODUCED means: the author ran the repro, against a named oracle, and read
the output. Everything else — an agent's claim, an operator's report, a
plausible reading of the source — is REPORTED and sits in a separate table
until someone reproduces it.

This is not pedantry. On 2026-07-28 a healthy product was publicly accused of
being broken (`audit-workflow`), and the accusation was withdrawn: the sweep's
own `skills:` paths resolved against the CWD, so the same file exited 0 from
inside the repo and 2 from its parent. The bar exists because it was crossed.

Of the fifteen findings below, **one (F6) was never reproduced** and is
therefore excluded from the defect count while remaining on the record.

### De-duplication across runs

Before opening a finding, grep the prior runs' tables for the seam, not the
symptom. Two different error codes over one predicate are one finding. If a
prior run's finding recurs after a shipped fix, it is **not** a new finding —
it is a **regression**, tracked in its own column, and a regression is the
strongest possible signal about the curve's shape.

---

## §3 · The taxonomy

Six classes. The first three are about what a verdict does; the last three are
about what it says.

| Class | Definition | Severity rule |
|---|---|---|
| **fail-open** | a boundary admits what it should refuse | **always P0** — outranks everything |
| **fail-closed** | a boundary refuses what it should admit | P1 · **P0** if it blocks all legitimate use of a surface |
| **false-green** | a verdict claims more than it covers · includes check-green/run-refused | **P0** on money or security · P1 elsewhere |
| **self-contradiction** | two surfaces of one tool give opposite answers on one input | P1 — it destroys the tool's authority wholesale |
| **teaching defect** | a shipped example, doc, template or comment teaches a form the engine refuses, or a model the spec contradicts | **inherits the severity of what it teaches**, because it replicates |
| **misattribution** | a verdict true in aggregate that names the wrong subject | P1 if it misdirects a diagnosis · P2 otherwise |

### Why fail-open outranks everything

A fail-closed finding costs the author rounds. A fail-open costs the user
secrets, and it costs them silently and irreversibly. On 2026-07-28 the author
spent the session on the fail-closed side of the ledger; the fail-open was one
function away in a crate already open, and the pass that found it had been
dispatched to check a *fix*, not to look for it.

> **Send the adversarial pass before the fix feels finished, not after.**

### The two classes that are easy to miss

**false-green includes check-green/run-refused.** If `check` passes a file the
run refuses, the check verdict claimed more than it covered — regardless of
which side is "correct". F13 is this shape, and so is every `${{ const.x }}`
path the static gate deferred.

**teaching defect inherits severity.** A wrong comment in a canonical example
is not cosmetic: it is copied. The severity is that of the thing it teaches,
which is why a comment claiming `egress: host_from_self` grants network
capability is not a P3 typo — it teaches an author to omit a permit, and the
spec says the opposite at `01-envelope.md:377`.

### Marking

Every finding carries one primary class. A finding with two faces (F8 is
fail-open for the dishonest author and fail-closed for the honest one) takes
the **higher-severity** class as primary and names the second in its row.

---

## §4 · What must be MEASURED, not read

This is the load-bearing half of the method. On 2026-07-28 an adversarial pass
killed **17 of 33** of the author's own claims, and the session recorded **20
further retractions** the algebra document does not carry. Four of those were
verdicts an agent rendered on files it had never opened.

### The rule

> **Cite a `file:line`, a URL, or a command with its actual output. Otherwise
> mark it INFERENCE, in the row, in the artifact.**

### The five failure modes, each with the incident that taught it

**① A "CLEAN" answers the question you asked, not the one you meant.** A sweep
for dead syntax and a sweep for "does this file pass its own check" are
different tests. The first one was believed for hours. The second found the
starters, then everything else. *This is the session's own law applied to its
method: a verdict that does not cover what it announces.*

**② A gate that has never failed is not proven — probe BOTH directions.** A
ratchet must be run against a deliberately-wrong fixture (it catches) *and*
against the real tree (it stays quiet). Running only the second is how a gate
that silently matches nothing passes review. The kit ratchet was validated 6
caught / 0 false, and that protocol immediately exposed a form it was missing.

**③ Measure a gate's repair against the WHOLE corpus before keeping it.** The
`fn-length` fix looked right on its case and turned 1 false positive into 5
across the repo's 10 000+ functions. **Reverted.** A half-corrected heuristic
is worse than a documented one.

**④ When the engine already prints the number, read where it comes from before
recomputing it.** `analysis.rs` computes DAG width exactly — Dilworth →
Fulkerson → Hopcroft-Karp with a König witness — and prints it. Three hours
went into a slower, wrong reconstruction of a number that had already been on
screen.

**⑤ The release is the truth for everything users touch.** Prompts and
annotations were proven on a locally-built binary and refuted by an agent
probing the *published* one (`prompts/list` → `-32601`). A push is not a
release.

### The waiver rule

The single sharpest lesson of run 1, and it belongs in every audit:

> **When a proof obligation is waived as undecidable, NAME the decision
> procedure it would have needed. If you can write it down, it was decidable
> and the waiver is a hole.**

F11's differential proptest ran green because its generator excluded
mid-pattern globs, citing a true theorem about containment between two
*patterns*. Both sides of that differential match a *concrete path* against
*one pattern* — ordinary glob matching, entirely decidable. A correct theorem
waived an obligation it did not govern, and the fail-open lived in the gap.

Used as a tool rather than learned twice, this becomes the audit's most
productive question. F13: "which host?" is undecidable inside a secret; **"is
there ANY host granted?"** is decidable, and an empty `net.http` makes the run
a guaranteed failure. One engine repair closes the class; seven file repairs
would grow back.

**Audit move: for every "we can't check that statically", write the
sub-question that IS decidable.**

---

## §5 · Recording a run

One table per run, one row per finding, in the run's own file. The columns are
fixed so that two runs can be set side by side.

```
ID · DOMAIN · CLASS · SEVERITY · STATE · ORACLE · REPRO · SEAM
```

`STATE` ∈ `reproduced` | `reported` | `symptom` | `regression` | `open-question`.
`SEAM` is the `file:line` or verb where the repair lands — it is the
de-duplication key for every later run.

And the run's header carries the five declared lines from §1 plus:

```
COUNT   reproduced defects requiring their own repair       ← THE number
        + symptoms · + reported-unreproduced · + regressions  (separate)
EFFORT  the denominator, as declared
```

**The headline count is reproduced-defects-requiring-repair.** Everything else
is reported beside it and never folded in.

---

## §6 · Run 1 · the baseline

**Retrospective classification of the F-series from
`docs/plans/2026-07-28-verdict-coverage.md`. Every row traces to that record.**

```
DOMAIN      the verdict/gate surface — what `nika check` and `nika run` claim
            about a workflow, and whether they observed it
ORACLE      MIXED — see the caveat below
SURFACES    check verdict lines (TYPES · PERMITS · COST · TRIFECTA) · run
            settle + reporting · the fs/net permit seams · the checking
            surfaces that judge on an author's behalf (hooks, --infer-permits)
DENOMINATOR ~15h (08:52 2026-07-28 → 01:57 2026-07-29) · 23 individually
            dispatched agents + 5 dynamic swarms, MOST OF WHICH SERVED A
            DIFFERENT ARC (see §6.3)
EXCLUDED    declared retroactively — see §6.4
```

### §6.1 · The findings

| ID | Domain | Class | Sev | State | Seam |
|---|---|---|---|---|---|
| **F11** | fs permits | **fail-open** | **P0** | reproduced · on published 0.106.1 | `nika-builtin/src/permits.rs` `literal_root`/`confines` + `nika-check/src/permits_fit.rs` |
| **F8** | fs permits + trifecta | **fail-open** (2nd face fail-closed) | **P0** | reproduced | `permits_fit.rs` const-resolution · trifecta leg ① reads the declaration |
| **F2** | flow analysis · SEC-009 | **fail-open** | **P0** | reproduced | `content_flow.rs:140` — `Exec(_) => (false, true)` |
| **F7** | cost model | **false-green** | **P0** | reproduced · 328× | `nika-check/src/cost.rs:144-148` — `input_per_million` has no reader |
| **F3** | type layer | **false-green** | **P0** | reproduced | no `output_schema` mechanism in the catalog at all |
| **F1** | exec settle | **false-green** | **P0** | reproduced | `nika-runtime/src/dispatch.rs` `settle_exec_out` |
| **F14** | tooling · edit hook | **false-green** | unranked | reproduced | `check-on-edit.sh` — `NIKA="${NIKA_BIN:-nika}"` |
| **F13** | net permits + secrets | **false-green** (2nd face teaching) | unranked | reproduced · all 3 arms | check misses the decidable "is there ANY host" |
| **F4** | human gate · CLI | **fail-closed** | P1 | reproduced | `--answer` refused without `--resume` |
| **F9** | *unrecovered* | **fail-closed** | unranked | attested only | — |
| **F10** | *unrecovered* | **fail-closed** | unranked | attested only | — |
| **F15** | check surfaces | **self-contradiction** | unranked | reproduced | `permits_infer` lacks `judgeable_arg` |
| **F5** | run reporting | **misattribution** | P1 | reproduced | summary cites last error, not causal |
| **F12** | diagnostics | **self-contradiction** | — | **symptom** · closed by F11, no repair | downstream of the matcher |
| **F6** | renderer | rendering | P2 | **reported · never reproduced** | `pyte` absent, repro never run |
| **OPEN** | trifecta | potential over-approx | — | **open question** · operator | leg ③'s workspace test vs witness selection |

### §6.2 · The count, decomposed

```
15   numbered entries in the record
−1   F12   symptom · closed by F11's predicate · required no repair of its own
−1   F6    reported, never reproduced
−1   OPEN  explicitly "not a finding: a question that needs an owner"
────
12   reproduced defects requiring their own repair   ← run 1's headline number
     of which F9/F10 have unrecovered content (see §6.5)
```

By class, over the 12 + the symptom:

```
fail-open           3    F2 · F8 · F11        all P0, by rule
false-green         5    F1 · F3 · F7 · F13 · F14
fail-closed         3    F4 · F9 · F10
self-contradiction  2    F12 (symptom) · F15
misattribution      1    F5
teaching defect     0 primary · 2 secondary faces (F13 · F2)
```

**No finding in the F-series is a primary teaching defect** — and that is a
fact about the *scope*, not about the product. The session's teaching-defect
arc was large and was never F-numbered (§6.3).

### §6.3 · What run 1 is NOT · the denominator problem

> **Run 1 is not "the session". It is one domain-scoped sweep inside a session
> that also ran several others.**

The same 15 hours produced these separately-measured results, in **different
domains**, none of which are F-numbered. They are recorded here so nobody
later adds them to twelve:

| Sweep | Domain | Measured |
|---|---|---|
| `intent-to-workflow` (swarm 6314) | authoring experience | 6 workflows, **zero one-shot**, 45 round-trips · **78 frictions**: 47 teaching · **24 engine** (16 defects + **8 false-greens**) · 7 own slips · 40 learnable nowhere |
| `theorem-refutation` (swarm 6144) | the session's own claims | **17 refuted · 10 false hypotheses · of 33** |
| session record §7.3 | engine debt, author-verified | **15 rows** · 3 are the same finding as F1 · F3 · F7, and a 4th (a permit bound behind `const:` escaping `AUTH-006`) shares **F8's seam** under §2's de-duplication rule — different error code, one predicate → **11 additional verified defects** |
| session record §7.4 | reported, never author-verified | 7 rows · kept separate by rule |
| Phase A (kit/plugins/MCP/starters) | teaching surfaces | 5 dead syntactic forms shipped in `nika init`; starters failing their own check; the marketplace serving 0.105 |

So the session's *verified distinct engine defects* is **23** (12 F-series + 11
unique to §7.3), not 15 — and the *false-green* class alone was independently
re-measured at **8** by swarm 6314 in a domain the F-series never touched.

*(The 15 → 11 adjustment is §2's de-duplication rule applied to this very
table, and it was caught while verifying the count rather than while writing
it. Recorded because the rule is worth more than the number.)*

**The consequence for the curve.** Run 1's headline of 12 is a **lower bound
with an unnormalised denominator**: the domain was not declared in advance, it
grew as the sweep went, and the effort attributable specifically to the
F-series cannot be separated from the 23 agents mostly serving the resource-
algebra arc. Run 2 must declare its five lines up front, or the two points do
not lie on the same axis.

### §6.4 · Retroactively declared exclusions

Stated because §1 requires them and run 1 did not have them:

- **Excluded**: the authoring/teaching surfaces (measured separately, §6.3),
  the resource-algebra research claims (a different artifact), and the five
  never-tested items in `resource-algebra.md` §4.
- **Not excluded but not swept**: everything in §7. The F-series went where
  the day's reports pointed, which is a **reactive** sweep, not a systematic
  one. A systematic sweep of the same domain would likely find more, which is
  itself a reason run 2's decay reading needs care.

### §6.5 · The honest gap · F9 and F10

**Their content could not be recovered.** Both are referenced exactly twice in
the record and nowhere else — `2026-07-28-verdict-coverage.md:485` and `:580`
— and neither has a section. Searched: both plan documents, the full engine
`git log --all` since 2026-07-27 including commit bodies, and the monorepo
journal. No commit names them.

What the record **does** attest, at `:580-586`:

> "They were real and they are fixed, but every one of them was **fail-closed**
> — friction, refusing what should pass."

Plus, at `:485`: F9 was a **fix** whose correctness an adversarial swarm was
dispatched to check — and "it was not the right fix, and it was not the
important bug."

**Classification chosen: fail-closed, on the record's explicit word, content
unrecovered, severity unranked.** They are counted in the 12 because the
record states they were real and required repairs. The alternative — dropping
them — would make the count *look* cleaner while covering less than it claims.

*(A plausible reading is that F9/F10 are the const-resolution lane in
`permits_fit` and the ceiling/floor card repair at `a77078707`. It is not
written anywhere, so it is not recorded as fact.)*

---

## §7 · Run 2 · the named domains

Two runs are owed, and they answer different questions. **Run them as separate
records; never pool their counts.**

### 7a · DECAY · re-sweep the verdict/gate domain

This is the run that answers the operator's question. Same domain as run 1,
after F11/F13/F15 ship, executed **systematically** under §1 rather than
reactively.

```
if the count falls sharply    → run 1 was a first-audit harvest. The debt was
                                real, finite, and is being shed.
if the count holds flat       → the class regenerates. The seam that produces
                                false-greens is structural and needs the
                                corpus-level check⇔run oracle, not more fixes.
if a prior finding recurs     → REGRESSION column. This is the strongest
                                signal available and outranks the raw count.
```

**Caveat that must be written into run 2's header:** run 1 was reactive and
run 2 will be systematic. A systematic sweep of a domain finds more per hour
than a reactive one, so a *flat* count between them is evidence of **decay**,
not of stasis. Say so before measuring, not after.

### 7b · BREADTH · the new domains, ranked

Ranked by **what a fail-open here would cost**, which is not the same as how
likely one is.

| # | Domain | Fail-open cost | Why it ranks here |
|---|---|---|---|
| **1** | **trace / attestation chain** | **retroactive** — voids every past green | The chain is order-integrity, not byte-reproducibility, and **what the golden pins actually compare has never been read** (`resource-algebra.md` §4). Every other verdict in this system rests on the trace being what it says. It is the only domain whose fail-open invalidates the *other* audits. And F11 put a private key's contents *into* a signed trace, so the domain is already implicated. |
| **2** | **composition / budget** | real money spent under a budget that forbids it | `--max-cost-usd` is **null one level of composition deeper** — already author-verified P0 and unfixed (§7.3 row 4). A budget that stops binding at depth is a fail-open on money with an existing repro. Retry amplification `k^N` under parent/child is in the untested list (§4), same seam. **Highest confidence of the five.** |
| **3** | **agent-loop budget + MCP pricing** *(one domain — they compose)* | unbounded spend, adversarially reachable | The loop is Θ(n²) on input; `max_tokens` bounds only output. A **conforming but malicious MCP server inflates cost 658×** (measured, swarm 5599). And `cost.rs:109` prices a remote MCP `invoke` at **zero** — `continue` — so that vector contributes nothing to any number `check` prints. Auditing these apart would miss the composition, which is where the damage is. |
| **4** | **secrets flow · `declassify:`** | a secret leaves, irreversibly | `declassify:` is taught **nowhere** — 0 occurrences across all three trees (swarm 4942). `content_not_interpreted` was found **unsound** (CaMeL §6.4/§7) and withdrawn. F13 showed the `egress:` ⊥ `permits.net.http` distinction is already being taught wrong. The escape hatch nobody documents is the one nobody audits. |
| **5** | **never-audited-in-completeness surfaces** | varies · unknown | `nika-registry` · `client-sdk` · `homebrew` · `audit-workflow` · `THREAT-MODEL.md` · `server.json` · `listings.yaml` · the three forked copies of the Hermes skill. Per §7.5 these were declared clean **on dead syntax**, which is a different test (§4 ①). Ranked last only because the cost is unknown, not low — and "unknown" is itself a reason to sweep. |

**Do not start with #1.** Start with **#2**: it has a live repro, a bounded
surface, and a P0 already on the books, so it will calibrate the protocol
against a known answer before the protocol is trusted on #1, where there is no
known answer and the stakes are the highest.

---

## §8 · What this instrument cannot tell you

Stated so the curve is not over-read.

- **Two points do not establish a trend.** Run 2 distinguishes "sharp fall"
  from "flat"; it does not fit a rate. Three runs minimum, and the third must
  be in a domain already swept twice.
- **The count is method-sensitive.** Reactive vs systematic, solo vs swarm,
  check-only vs check-and-run — each moves the number more than the underlying
  defect density does. This is why §1's five declared lines are mandatory and
  why the denominator travels with the count.
- **Severity does not aggregate.** Twelve findings of which three are
  fail-open is not comparable to twelve of which zero are. **Report the class
  histogram beside every count**; a run whose total fell while its fail-open
  count rose got worse.
- **Absence of findings is not absence of defects** — it is evidence about the
  probe. A domain that returns zero should publish its probes so the next
  reader can judge whether they could have failed (§4 ②).

---

## §9 · Related

- `docs/plans/2026-07-28-verdict-coverage.md` — run 1's source record; every
  §6 row traces to it
- `docs/plans/2026-07-28-session-record.md` — §7.3/§7.4 debt tables, §7.5
  unaudited surfaces, §8 the twelve method lessons, §5 the twenty retractions
- `docs/plans/2026-07-28-resource-algebra.md` — §1.7 the security-gate
  measurements, §4 what is not tested, §7 the method
- the `nika-first-automation` rule (monorepo-internal) §4bis — the reciprocal dogfood
  law: an authoring friction is a finding, not an inconvenience

---

> **The law all of this serves.**
>
> A verdict must either COVER its claim, or NARROW its claim to what it
> covers. A green that means less than it says is worse than no green: it
> spends the reader's trust and returns nothing.
>
> It binds this document too. Run 1's headline is **12**, not 15, and its
> denominator is not normalised — stated here rather than discovered later.
