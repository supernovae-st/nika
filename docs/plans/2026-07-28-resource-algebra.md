# The resource algebra · what a workflow costs, and what the checker can prove

> Status: research + measurement, not implementation. Every row in the
> matrix below was MEASURED against the 0.106.0 binary or read in the
> source on 2026-07-28. Nothing here is inferred from memory. Citations
> carry a URL or a file:line; claims without one are marked as inference.

Companion: `2026-07-28-verdict-coverage.md` (the false-green class). That
document says a verdict must cover its claim or narrow it. This one asks
what the claims can be, for the whole resource surface rather than money
alone.

---

## 0 · Why this document exists

`nika check` prints one number before a run and the spec promises it:

> `spec/07-conformance.md` — *"Cost ceiling · the worst-case spend ·
> Σ (max_tokens × provider price) across `infer:`/`agent:` tasks · before
> one token is spent"*

Three measurements say that promise is not kept, in three different ways:
the formula prices a quarter of what the runtime bills, the ceiling is
uncomputable for a whole class of workflows, and it is the only resource
the engine bounds at all. The rest of this document is what the
literature and the code say about fixing that.

---

## 0.5 · CORRECTIONS · what an adversarial pass killed

> Read this before §1. An adversarial refutation swarm attacked every claim
> in this document on the same day it was written: **17 of 33 REFUTED, 10
> more hold with the wrong hypotheses stated.** The sections below are left
> intact so the record is honest, but the following supersedes them.

### The worst one: I rebuilt a capability the engine already documents

`crates/nika-check/src/analysis.rs:26-32` says, verbatim:

> *"Width can EXCEED the largest wave … `max parallelism` = the wave peak
> **as executed** · `width` = what the DAG **permits**."*

It is computed exactly (Dilworth → Fulkerson → Hopcroft-Karp, with a König
witness) and **printed on the PLAN line of every check**. I saw `width 3`
in an output, noted it, and did not follow it. Then I spent three attempts
re-deriving it from YAML, more slowly and wrong.

**The reusable lesson: when the engine prints a number you are about to
recompute, read where it comes from first.**

### S9 is wrong three ways

| | Claimed | Actual |
|---|---|---|
| the quantity | width bounds concurrency | width counts TASK NODES; the real parallelism lives in `for_each` — `runtime/src/task.rs:640`, `cap = max_parallel OR items.len()`, so **without `max_parallel:` every item is in flight**. A 20-item file reports `width 1` and holds 20 processes. **15 of 76 corpus files carry a runtime `for_each`.** |
| the statistics | median 2, mean 1.8, over 43 | **45 of 76 have width 1** ⟹ median **1**, mean **1.66**. Only `max 4` and `100% ≤ 4` survive. |
| the coverage caveat | "the parser saw 43 of ~76" | 43 is not a parser gap, it is **mirror deduplication** (33 workflows × 2 mirrors + 10 templates = 76; 43 unique basenames) — which K4 of this very document already explains. And the binary sweep takes **2 seconds**, not the 10 minutes that timed out. |

### The measured counter-example that reorders the plan

16 identical `infer:` tasks, a provider allowing 4 concurrent, default retry:

```
flat16    (0 edges · width 16)  →  EXIT=1 · 12/16 RED · 12 × 429
staged16  (12 after: edges)     →  EXIT=0 · 16/16 green · 4.0 s · 0 × 429
```

**The widest schedule fails; the narrowest succeeds.** Therefore:

- **Item 8 (per-provider concurrency) is a PREREQUISITE of item 1
  (dataflow scheduler), not its peer.** Removing the barrier first is a
  regression, not an optimisation.
- **Item 6 (the phantom-`after:` hint) must be gated on item 8**, or it
  will tell authors to delete the only thing keeping their workflow green.

And `m` in T1 is the PROCESSOR count, not a software cap: 24 CPU-bound
units at declared `width 1` measured 7.3 s wall / 63.9 s user on 12 cores
against 2.21 s predicted. For `infer:`, `m` is the provider's concurrency
allocation, measured at 4.

### `buffered` is a buffer, not a scheduler

`futures::stream::Buffered` admits a new item only when the **head** of its
ordered queue resolves. It idles with free slots and ready work, which puts
it **outside Graham's non-idling precondition** (T2). Measured on 28 items
(`[8s, 27×1s]`, `max_parallel: 10`): six seconds at 1-in-flight with 9 free
slots and 18 ready items. `capped 10 → 10.1 s` vs `uncapped → 8.0 s`, the
optimum. **A tighter cap was SLOWER.**

### The wave theorem: four of seven lines are wrong as written

| | Verdict | What breaks |
|---|---|---|
| **W1** | holds, hypothesis oversized | true for ANY proper layering (0 violations in 1,128,320); only `d ≥ 0` carries. ASAP is stated and unused. |
| **W2** | holds, second conjunct VACUOUS | the `d(v) = max_{level(v)}` clause is implied by the first (0 disagreements over 28,448,688 exhaustive pairs). The `max_w > 0` guard is the WHOLE correction — my account of my own fix was wrong. |
| **W3** | **REFUTED (hypothesis)** | the result holds; the named hypothesis does not. Exhaustive over n≤7 (2,131,019 DAGs): 0 failures under ASAP **and** under ALAP; a STRETCHED layering fails (6 cases). The real condition is **minimum height**, not ASAP. The engine's layering (`analyzer/dag.rs:211`, Kahn) is minimum-height, so the conclusion survives — but teaching the wrong hypothesis would make a future layering change falsely safe. |
| **W4** | **REFUTED** | `W` is **attained**, not approached. Witness: 3 tasks, one edge, `d=(1,0,1)` → ratio exactly 2 = W. Zero durations are in W1's own domain (`d : V → ℝ≥0`) and are real — `cost.rs:108` maps `exec`/`invoke` to `continue`. So `W` is the **real worst case**, not a loose majorant. |
| **W5** | holds, one hole | `T_∞ = 0` is attainable (all-zero durations), so the ratio form divides by zero. Needs a `T_flow > 0` guard. |
| **W6** | **REFUTED** | the algebra survives — 15/15 concurrent-monoid axioms verified — but the **identification fails**. On the N poset (the one W7 names two paragraphs later) the exchange slack is 1 while the wave penalty is 0, and the right-hand side (1) is not `T_flow` (2). Only 2 of 9 bindings hold, both the same "two disjoint chains" DAG. **"The penalty IS the exchange law" comes out of the document.** What survives: `T_wave ≥ T_flow` follows from exchange PLUS isotonicity, and the slack **upper-bounds** the penalty without equalling it. |
| **W7** | **REFUTED** | the paragraph contradicts itself. A weak order IS N-free, therefore series-parallel — **waves DO remove the Ns**. On the N poset the wave order is N-free. What survives is the ORIGINAL DAG's N. Three further errors in the same paragraph: the SP extension is not minimal (up to 2 needless relations), "optimal exactly under unit durations" read as an iff is false, and the example loses 1.998002×, not "2×". |

### And a claim in §2.5 that the code refutes

**T17 said the budget composes by meet.** It composes in theory and **not in
the code**. Measured:

```
child alone,  --max-cost-usd 0.0001  →  rc=2 · "refusing to start:
                                        the unavoidable cost floor exceeds"
same child via a PARENT, same flag   →  the run STARTS, dispatches the child,
                                        reaches the provider, and fails only
                                        on a missing API key
```

**With a key present this spends real money under a budget that forbade
it.** The `COMPOSITION` rung already proves the call graph is *"static,
typed, contained and acyclic"*, so the floor is a **topological sum**, not a
fixpoint. This is a P0 and it outranks everything in §5.


---

## 1 · The measurement matrix

Everything below was run. `check` = `nika check --native-strict` unless
noted; the binary is `/opt/homebrew/bin/nika` 0.106.0 except where a
`main` build is named.

### 1.1 Cost

| # | Question | Method | Result |
|---|---|---|---|
| C1 | What does the ceiling price? | read `nika-check/src/cost.rs:194` | `output_price_per_million` **only** |
| C2 | Does the catalog know the input price? | read `nika-catalog/src/types/model.rs:328` | **yes** — `input_per_million` |
| C3 | Does the catalog know cache rates? | read `nika-catalog/src/data/models.rs:186-192` | **yes** — `cache_read_per_million`, `cache_write_per_million` |
| C4 | Does the runtime price them? | same, `usd_for_split` | **yes** — uncached + read + write + output |
| C5 | Is `exec`/`invoke` priced? | `cost.rs:109` | `continue` — *"spend nothing on inference"* |
| C6 | Is a local model priced? | `find_pricing_for("ollama/…")` | `None` — invisible to the ceiling |
| C7 | Is `agent` priced per-loop or per-turn? | `cost.rs:104` | `max_tokens_total` — **cumulative**, the right shape |
| C8 | Does the summary line sign match the section? | run on `10-cost-estimate-semantics` | `≥` vs `worst-case ceiling` — **fixed**, `a77078707` |
| C9 | Is the cheapest-path figure a floor? | read `cost.rs:83` doc | no — cheapest PATH with every task at its own cap |
| C10 | Ceiling vs realized, one sample | operator repro 10 | `$0.0305` announced, `$0.000242` billed — 126× |

**C1 against C2-C4 is the finding.** The engine has two cost models. The
runtime bills input + cache-read + cache-write + output; the check
ceiling reads one term of four. Since `max_tokens` bounds output only,
the ignored term is also the unbounded one — so on an input-heavy
pipeline (fetch a page, summarise it) the "ceiling" can sit **below** the
bill. That is a soundness break, not looseness, and it points the
opposite way from C10.

### 1.2 Scheduling

| # | Question | Method | Result |
|---|---|---|---|
| S1 | Is the wave penalty real? | timed `11-wave-barrier-scheduling` | **10.1 s** for two independent 5 s waits |
| S2 | Is the wave structure what I claimed? | `check` on the same file | 4 waves · `{slow_early,c1} {c2} {c3} {slow_late}` — heterogeneous, so the penalty is predicted |
| S3 | Does the parallelism cap bind by default? | `nika-runtime/src/config.rs:14` | **no** — *"`None` = wave-width (every wave member in flight at once)"* |
| S4 | Is there a concurrency primitive already? | `nika-runtime/src/lib.rs:940` | `futures_util::stream::iter(...).buffered(cap)` — single decision loop, no spawn |
| S5 | Is a phantom `after:` edge detected? | 3-shape fixture | **0 hints** |
| S6 | Is a redundant (transitive) `after:` detected? | same | **0 hints** |
| S7 | Is there a per-provider concurrency counter? | grep `nika-verb-infer`, `nika-providers` | **none** |
| S8 | Is 429 handled? | `nika-providers/src/wire/mod.rs:114,133` | **yes** — `Retry-After` honoured |
| S9 | **How wide are real DAGs?** | longest-path layering computed from the YAML over 43 corpus workflows | **max 4 · median 2 · mean 1.8 · 100% ≤ 4** |

**S9 closes the hypothesis T1 rests on.** `wave_parallelism` defaults to
wave-width, i.e. unbounded (S3), and the widest workflow in the corpus is
4. **The cap does not bind, and on this evidence will not.** So we are
permanently in the `P∞` regime where greedy is *exactly* optimal — not
`2 − 1/m`-approximate. Every theorem below the `P∞` line (Graham's bound,
Ullman's NP-completeness, the anomalies) is inert for this corpus.

*Coverage caveat:* the parser recognised `tasks:` in 43 files of the ~76
in the teaching corpus; files using an inline or otherwise-shaped task
map were skipped. The claim is therefore "max width 4 over 43 workflows",
not over all of them. Two earlier attempts to measure this by invoking the
binary per file timed out at 10 minutes; parsing the YAML directly runs in
under a second, which is the reusable lesson.

**S3 + S4 together are the whole fix.** The concurrency is already there
and already has the shape the literature recommends (one decision loop,
work concurrent inside). The barrier is the only thing serialising, and
with an unbounded cap, removing it does not "optimise" — it reaches the
optimum (§2.1).

**S4 carries a trap.** `buffered` returns results *in input order*;
`buffer_unordered` returns them *in completion order*. The chain's order
stability today comes from `buffered`. A naive dataflow rewrite slides to
completion order and loses it — independently of any failure-policy
choice.

### 1.3 The value language

| # | Question | Method | Result |
|---|---|---|---|
| V1 | Are CEL comprehension macros allowed? | `all` / `map` / `exists` in `when:` | **refused** — `NIKA-VAR-005` |
| V2 | Is `matches()` (regex) allowed? | `when: '${{ inputs.s.matches("(a+)+$") }}'` | **refused** — `NIKA-VAR-005` |
| V3 | Is there a cardinality bound on `for_each`? | `max_items: 100` | **refused** — `NIKA-PARSE-005` |
| V4 | Does `for_each` accept an object form? | `for_each: { over: …, take: … }` | **refused** — `NIKA-PARSE-019` |
| V5 | Is there a size bound on an array input? | `max_items` / `max_len` / `maxItems` | **all refused** |

**V1 + V2 are good news and worth saying out loud.** The CEL spec names
macros as *"the only avenue for exponential behavior"*; Nika refuses all
of them, and refuses regex too. So the exponential blow-up and the ReDoS
vector are closed **by construction**, more firmly than the Kubernetes
cost-budget design would have required. This is a real safety property
the spec does not currently claim.

**V3-V5 are the other half of the cost problem.** A `for_each` over a
runtime collection has no declarable cardinality anywhere — not on the
loop, not on the input. So `Σ c·n·r` has an unknown `n` and the ceiling
is `∞` for that whole class.

### 1.4 The corpus

| # | Question | Method | Result |
|---|---|---|---|
| K1 | Does the teaching corpus pass? | `check --native-strict` on all of `pack/examples`, `pack/templates`, `spec/examples` | **76 / 76 green** |
| K2 | How many tracked workflows are red? | same sweep, 295 files | 79 — **all** under `conformance/`, `fixtures/`, `adversarial/`, `fuzz/`, i.e. deliberately red |
| K3 | Is `declassify:` taught anywhere? | grep all three example trees | **0 occurrences** |
| K4 | Do the two example trees agree? | `diff -rq` spec vs vendored pack | **13 files differ** |
| K5 | Does an example teach a phantom builtin field? | `t4-ceo-monday-brief` | `tasks.bill.output.total_usd` × 2, `check` says `audited` — repaired in spec, PR #229 |

**K2 corrects a swarm result.** An audit swarm reported "32 broken"; the
sweep shows the teaching corpus is entirely clean and every red file is a
fixture whose job is to be red. A conformance fixture under `invalid/`
that fails is the test passing.

**K4 carries a discipline point that cost this session real work.**
`crates/nika-pack/pack/` is the **vendored mirror** of `nika-spec`,
pinned by `SPEC_PIN` and re-vendored by the daily heal. A fix applied
only there is overwritten at the next bump. Fixes land in the source.

### 1.5 The human gate

| # | Question | Method | Result |
|---|---|---|---|
| H1 | How many `Prompter` implementations exist? | grep `impl Prompter for` | **two** — `NonInteractive`, and `AlwaysPrompter` inside `#[cfg(test)]` |
| H2 | What does the CLI inject? | `nika-runtime/src/compose.rs:829` | `NonInteractive` — **unconditionally**, no tty branch |
| H3 | What does the trait doc claim? | `nika-builtin/src/lib.rs:135` | *"The L4 CLI implements the TTY prompter"* |

**H1-H3 is not a design trade-off; it is an implementation that a comment
believes exists.** Combined with `NIKA-SEC-009` requiring a dominating
gate, every workflow that completes the trifecta is structurally
unrunnable: the gate is mandatory and unanswerable.

### 1.6 The builtins

| # | Question | Method | Result |
|---|---|---|---|
| B1 | Does `nika:jq` match jq 1.7.1? | operator repro `09-jq-conformance`, run | `floor_slice` ✓ · `splits_global` ✓ · **`scan_global` ✗** · **`scan_captures` ✗** |
| B2 | Does the run report the divergence? | same | **no** — 4/4 green |
| B3 | Does `nika:inspect view: cost` work in-workflow? | ran it | `{available:false, reason:"the runtime does not yet expose its live cost to the in-workflow builtin"}`, task **green** |
| B4 | Does `permits.exec` grant reading the binary? | operator repro 05 | **no** — `NIKA-EXEC-001 status 126`, message from **bash**, naming no permit; `check` was green |

**B3 matters beyond the bug.** `spec/08-out-of-scope.md` defers the
`budget:` block with the rationale *"the `nika:inspect view: cost`
introspection builtin gives workflows access to running cost"*. That
builtin answers `available: false`. The rationale rests on a capability
that does not exist and has to be rewritten whichever way the budget
question is decided.

### 1.7 The security gates

| # | Question | Method | Result |
|---|---|---|---|
| G1 | Does leg ② of the trifecta need a READ, or a declared permit? | 3 variants, one parameter moved | see below |
| G2 | Does removing an unused `fs.read` permit clear SEC-009? | variant v4 | **yes** — green |
| G3 | Does narrowing the read to the single file the flow wrote clear it? | variant v5 | **no** — still SEC-009 |
| G4 | Does `exec curl` bypass SEC-009 where `nika:fetch` does not? | matched pair | **yes** — bare `check` exit 0 |
| G5 | Does `--native-strict` close that bypass? | same pair | **yes** — `native-first/001`, exit 2 |
| G6 | Why? | `content_flow.rs:140` | `RawAction::Exec(_) => (false, true)` — an exec is `writes_fs`, never `born_ingress` |

**G1-G3 is the sharpest finding of the session.** Three variants, one
parameter moved:

```
v1  fetch + write(./tmp) + read(./tmp)                 → SEC-009
v2  fetch + write(./tmp) · NO read task at all         → SEC-009   ← ??
v4  identical to v2, permits.fs.read REMOVED           → clean
v5  identical to v2, read narrowed to the written file → SEC-009
```

`v2` reads nothing. Leg ② is armed by the **declaration**, not by the
body. And the same report says so, eight lines below its own refusal:

```
✖ TRIFECTA [NIKA-SEC-009] … while private re[ad in scope]
↳ HINT     [NIKA-DRIFT-001] `permits.fs.read` entry `./tmp/**`
           matches no path the body reads — remove the entry
```

Two subsystems, one output, opposite conclusions. DRIFT knows nothing
reads; TRIFECTA counts the read anyway. This is the seventh instance of
the false-green class, and the one that fires SEC-009 on ordinary work.

**The consequence is an incentive, and it is the wrong one.** The way to
pass the gate today is to *remove a permit you legitimately hold* (G2).
The security gate rewards under-declaring your authority — while
`permits:` exists precisely to be declared honestly.

**G4-G6 is the same shape on the other leg.** Replacing `nika:fetch`
with `exec curl` makes SEC-009 go quiet, because an exec is never an
ingress ORIGIN. The compensating rule two frames up (`content_flow.rs:80`)
re-taints an exec that reads a file a tainted writer produced — the
file-mediated channel argv cannot see — but it does not make `exec curl`
untrusted content. Measured on a matched pair:

| | bare `check` | `--native-strict` |
|---|---|---|
| `nika:fetch` (native) | 0 | **0** |
| `exec curl` (the bypass) | **0 — SEC-009 silent** | **2** |

So the flag wired onto the checking surfaces this session closes the
**incentive** — an author cannot reach the bypass through any surface
that checks on their behalf, and cannot run the file either. It does not
close the **classification**. Both repairs still stand.

### 1.8 The engine's own gates

| # | Question | Method | Result |
|---|---|---|---|
| R1 | Does `check-fn-length.sh` measure correctly? | it refused a 24-line test | reported **212 lines** |
| R2 | Why? | read the script | its literal stripper is **line-local**: a backslash-continued Rust string has no closing quote on its opening line, so the YAML braces inside it count as code |
| R3 | Does fixing it help? | rewrote it to carry string state across lines, then diffed **every** function in the repo | **1 false positive became 5** — a multi-line `r#"…"#` closes with a bare quote, which the scanner reads as an opening one |
| R4 | Verdict | reverted | the script declares itself *"Phase 0 heuristic … a proper `syn` AST walk"* is the fix; a half-corrected heuristic is worse than a documented one |

Recorded because it is the same defect class one level up: **a gate that
reports on a domain it does not observe**, and whose wrong number also
named the wrong culprit. And because the attempted repair is a worked
example of the discipline — the fix was measured against the whole corpus
before being kept, and it was not kept.

### 1.9 The MCP catalog

| # | Question | Method | Result |
|---|---|---|---|
| M1 | What does a catalogued MCP server carry? | `nika-catalog/src/types/mcp_server.rs:36` | `id · aliases · title · description · packages · remotes · env_vars · homepage` |
| M2 | Any cost, latency or rate-limit field? | same | **none** |
| M3 | Is an `invoke` of a remote MCP priced? | `cost.rs:109` | **no** — `continue` |

So a task that calls a paid third-party MCP server over the network
contributes exactly zero to every number `check` prints.

---

## 2 · The theorems

Marked **[E]** established with a source, **[V]** verified computationally
this session, **[I]** my inference.

### 2.1 Scheduling

**T1 · ASAP optimality (`P∞|prec|C_max`). [E]**
For a finite DAG with durations `p_v ≥ 0`, the earliest-start schedule
`S*(v) = max{S*(u) + p_u : (u,v) ∈ E}` satisfies `S(v) ≥ S*(v)` for every
feasible `S` and every `v`. Hence `C_max(S*)` is the longest weighted
path, and it is optimal.
*Hypotheses:* `m ≥ width(G)`; zero communication delay; durations
**exogenous**; no release dates; non-preemptive.
*Consequences that matter:* the ASAP policy is **1-competitive against a
clairvoyant adversary** — no amount of duration knowledge can improve it.
And `S*` is optimal **pointwise**, so it minimises every regular
objective simultaneously, not just makespan.

**T2 · Graham's bound. [E]** (Graham 1966, BSTJ 45:1563)
For `P|prec|C_max`, **any** non-idling list schedule satisfies
`C_max ≤ (2 − 1/m)·OPT`, for arbitrary durations, arbitrary precedence,
and **any** list order — including one derived from no duration
knowledge at all.

**T3 · The non-clairvoyant lower bound. [E]** (Shmoys, Wein, Williamson,
SIAM J. Comput. 1995)
No deterministic non-clairvoyant algorithm beats `2 − 1/m`, with or
without preemption. Combined with T2: **blind greedy is exactly optimally
competitive.**

**T4 · The clairvoyant ceiling. [E]** (Svensson 2010)
Under a UGC variant it is NP-hard to approximate `P|prec|C_max` within
`2 − ζ`, **even with unit processing times**. So knowing the durations
does not open a door either.

**T5 · Brent. [E]** (Brent, JACM 21(2), 1974)
`T_p ≤ T_1/p + T_∞`, and trivially `T_p ≥ max(T_1/p, T_∞)`. Two
statically computable numbers pin `T_p` within 2×.

**T6 · Graham's anomalies. [E]** (Graham 1969, SIAM J. Appl. Math. 17(2))
`ω'/ω ≤ 1 + (n−1)/n'`, best possible. Adding processors, shortening
tasks, or **removing precedence edges** can each increase a list
schedule's makespan. `n = 1` is anomaly-free.
*Note:* this is why an author's artificial `after:` edge is not
superstition — under a binding cap it can genuinely help. Under `m = ∞`
it cannot, so once the barrier is gone the edge is pure harm.

### 2.2 The wave penalty — stated here, verified by construction

Let `waves` be the ASAP longest-path layering and `d : V → ℝ≥0`.

```
T_wave(G,d) = Σ over waves w of  max_{v ∈ w} d(v)
T_flow(G,d) = max over paths p of  Σ_{v ∈ p} d(v)
```

**W1 · `T_wave ≥ T_flow` always. [V]** (4000 random DAGs, no violation)
*Proof.* `level` strictly increases along a path, so a path meets each
wave at most once; its weight is at most `Σ_w max_w`. Requires `d ≥ 0`. ∎

**W2 · Equality condition. [V, corrected]**
`T_wave = T_flow` iff some path `p` contains an argmax vertex of every
wave with `max_w > 0`, **and** `d(v) = max_{level(v)}` for every `v ∈ p`.
My original statement omitted the `max_w > 0` guard: a path can skip a
wave, and skipping a zero-weight wave costs nothing.

**W3 · Constant durations ⟹ equality. [V]** (3000 random DAGs)
So the **entire** penalty comes from duration heterogeneity within a
wave. *The hypothesis "ASAP layering" is load-bearing* — other layerings
lose even with constant durations.

**W4 · The ratio is unbounded and `W` is exactly tight. [V]**
`T_wave ≤ W · T_flow`, approached but not attained by: a chain of `W`
`ε`-tasks, each `b_{w−1}` also feeding a unit-weight sink. Measured at
`ε = 1e−6`: W=2 → 1.999998, W=10 → 9.999910.

**W5 · The sandwich, sharper than W4. [V]**
```
T_∞  ≤  T_wave  ≤  T_1          (span ≤ wave ≤ total work)
⟹  T_wave / T_flow  ≤  min( W , T_1/T_∞ )
```
*Proof of the right half:* `Σ_w max_{v∈w} d(v) ≤ Σ_w Σ_{v∈w} d(v) = T_1`. ∎
Both `T_1` and `T_∞` are computed by any dataflow scheduler, so this is
the bound to instrument.

**W6 · The penalty IS the CKA exchange law. [V, 200k random tuples]**
`D = (ℝ≥0, ‖ = max, ; = +, 1 = 0, ≤ := ≥)` is a concurrent monoid in the
sense of Hoare–Möller–Struth–Wehrman, *Concurrent Kleene Algebra and its
Foundations*, JLAP 80(6), 2011, Definition 6.6. Its axiom (7) reads:

```
(a ‖ b) ; (c ‖ d)  ≤  (a ; c) ‖ (b ; d)
    i.e.   max(a,b) + max(c,d)  ≥  max(a+c, b+d)
           └──── T_wave ────┘      └── T_flow ──┘
```

**The wave penalty is one application of the exchange law, and its slack
is the defect of laxity.** With `(5,5,5,5)` it reads `10 ≥ 10` — which is
W3 restated algebraically.

**W7 · What a wave decomposition IS. [E]**
Not "the series-parallel closure" — that framing is refuted by a
four-node counterexample (`a1→a2 ‖ b1→b2` is series-parallel, N-free, and
still loses 2× with `d = (1000,1,1,1000)`), and by the fact that minimum
SP extensions are **not unique** (the N poset has three, pairwise
incomparable). The wave order is a **weak order** — a linear sum of
antichains — i.e. the depth-2 `series-of-parallel` SP tree. Waves destroy
SP *nesting*, they do not remove N's.
Equivalently: the wave decomposition is the **Cartier–Foata normal form**
of the trace, *"the most parallel and greedy form"* — canonical, and
optimal exactly under unit durations, which is W3 again.
The operation has a published name, **SP-ization**, and the layer-based
algorithm is González-Escribano, van Gemund & Cardeñoso-Payo, *Parallel
Computing* 35(8-9):455-474, 2009, whose abstract states the loss is
*"theoretically unbounded"* (W4) while measuring ≤10% on their
applications.

### 2.3 Cost

**T7 · The gated-cost DP. [E]** (Melani, Bertogna, Bonifaci,
Marchetti-Spaccamela, Buttazzo, *Schedulability Analysis of Conditional
Parallel Task Graphs*, IEEE Trans. Computers 2017, Algorithm 1)
Reverse-topological, `O(|V|·|E|)`: at a conditional node take the
**argmax** successor set; at a regular node take the **union** of
successor sets.
🔴 **The trap, stated in the paper:** a scalar DP (max at gates, sum at
forks) **double-counts every reconvergence** — the join node's cost is
added once per incoming branch. You must propagate node **sets**.
Special case: on a non-conditional DAG this collapses to `Σ_v C_v` in
`O(|V|)`, which is today's formula, and it is **exact** there.

**T8 · IPET, and why our Σ is not it. [E]** (Wilhelm et al., ACM TECS
7(3), 2008, §3.4)
IPET **maximises** `Σ xᵢtᵢ` over a flow polytope. Our Σ **evaluates** at a
fixed point `xᵢ = nᵢ·rᵢ`. With no gates the constraints pin every `xᵢ` and
max = our sum. With gates, setting `xᵢ = 1` for all gated nodes at once
is **not a feasible flow** — mutually exclusive branches cannot both run.
We evaluate the objective outside the feasible region; that is exactly
the over-count T7 fixes.

**T9 · The half of WCET that does not apply. [I, from the survey's own
diagnosis]**
WCET's difficulty and nearly all its pessimism live in processor-behaviour
analysis — caches, pipelines, timing anomalies, where *"the assumption
that only local worst cases have to be considered … is unsafe"*. None of
that exists for token cost: no history dependence, so
`cost(A;B) = cost(A) + cost(B)` **exactly**. We inherit only the
flow-analysis half. Our problem is structurally easier than WCET.
⚠️ *One caveat that would break it:* prompt-prefix caching prices B
differently depending on whether A ran first. Compositionality survives
monotonicity (local-worst-case stays safe) but exactness does not.

**T10 · Never emit `∞`. [E]** (Bygde, *Parametric WCET Analysis*;
Vivancos et al.)
Hard real-time never reports infinity. Two accepted answers: refuse, or
emit a **parametric** bound. For `for_each` the correct output is
`A + B·n`, not `∞` — actionable, because the caller can supply `n`.

**T11 · AARA degenerates here. [E + I]**
(Hofmann & Jost, POPL 2003; Hoffmann, Aehlig & Hofmann, TOPLAS 34(3),
2012; Hoffmann & Jost, MSCS 2022)
AARA's power comes from anchoring potential to a *structural invariant* —
list length. Token counts have no such handle. The survey notes that
equality-constrained AARA *"would deliver exact resource bounds for all
possible executions"* but is generally infeasible, requiring `L:RELAX`.
On a loop-free, recursion-free DAG with constant per-node cost and no
gates, the equality system **is** feasible and the bound is exact — and
it equals our Σ. With gates, the relaxation at a branch is exactly `max`,
i.e. T7. **AARA on our program is a constant-potential LP whose optimum
is the gate-aware max.** It buys nothing a 40-line DP does not.
The nearest shipped analogue is **Nomos** (Das, Balzer, Hoffmann,
Pfenning, Santurkar, CSF 2021, arXiv:1902.06056) — money-denominated
AARA, computed before execution, *"ruling out … out-of-gas
vulnerabilities"*.

**T12 · Tightness is not measurable. [E]** (Wilhelm et al., §5)
*"since the exact WCET or BCET is usually not known, there is really no
way to check how precise an estimate or bound is."* `ceiling / observed`
is an optimistic proxy; our 126× is `ceiling / one_sample`, weaker still.
The principled route from traces to a defensible claim is **Hybrid AARA**
(Pham, Wang, Hoffmann et al., 2024, doi:10.1145/3656380): Bayesian
inference **restricted to the polytope AARA's linear constraints already
define**, with soundness w.r.t. every observed run with probability 1
(Thm 6.1) and convergence in the limit (Thm 6.2).

### 2.4 Information flow

**T13 · Loop-freeness is the decidability hypothesis. [E]**
(Finkbeiner, Müller, Seidl & Zălinescu, CCS 2017, arXiv:1708.09013;
Finkbeiner, Seidl & Müller, ATVA 2016)
*"for workflows without loops, it is possible to check non-interference
even when ALL agents behave in a causal way. This is no longer the case
for workflows with loops."* Add loops and it becomes undecidable
(periodic tiling).
Corroborated from the program side: NI on **loop-free boolean programs**
is **coNP-complete** — decidable (Yasuoka & Terauchi 2010,
arXiv:1004.0062, Thm 3.9). Rice's theorem needs Turing-completeness and
does not apply.

**T14 · One mechanism closes four channels. [I, from the above]**
Implicit flow (`when:`), presence, cardinality (`for_each`) and the error
oracle are all the same question — *did this node run, and how many
times*. A `pc` label = the join of all control ancestors' labels,
computed by post-dominator control dependence (Denning & Denning, CACM
20(7), 1977 — the IFD rule), decides all four in `O(V+E)` with **zero new
annotations**, because the labels come from the taint analysis and
`permits:` that already exist.

**T15 · The cardinality channel is quantifiable. [E]**
(Smith, FOSSACS 2009, Theorem 1) For deterministic `c` and uniform `H`,
leakage = `log |L|`, the log of the number of distinct outputs, and this
equals the channel capacity (Thm 2). So a `for_each` whose fan-out is
`0..N` leaks **≤ log₂(N+1) bits** — decidable iff `N` is statically
bounded. Exact QIF is #P-hard and **not k-safety for any k**, so
self-composition does not apply: take upper bounds, never exact numbers.

**T16 · Non-interpretation ≠ non-influence. [E]**
(Debenedetti et al., CaMeL, arXiv:2503.18813, §6.4 and §7)
A binding that never reaches a code-bearing position can still select
among pre-authorised actions (the dispatcher, *"ROP vs CFI"*), leak via
iteration count, or leak one bit via a conditional exception. So an
exemption class asserting "this content is never interpreted" is
**unsound** as a discharge for a whole refusal.
The structural advantage that is genuinely ours: a purely dynamic monitor
*"cannot taint an assignment it never runs — the real leak is on the
branch it does not take"* (LLMbda, arXiv:2602.20064). A static analysis
over a declared DAG sees both branches.

### 2.5 Composition

**T17 · Limits meet, balances escrow. [E + I]**
`effective(child) = declared(child) ⊓ effective(parent)` is a
meet-semilattice on `(ℝ≥0 ∪ ∞, min)` — idempotent, commutative,
associative, identity `∞` — so it composes across arbitrary nesting and
is verifiable edge-by-edge. Same structure as cgroup v2 (*"restrictions
set closer to the root … can not be overridden from further away"*) and
HNC (*"the most restrictive quota always applies"*).
🔴 **But `min` bounds each child, not their SUM.** Two children each pass
admission and jointly blow the parent. The spec sentence *"the child
cannot outspend its caller"* is satisfied by the algebra while the sum
overruns. The correct primitive is the **escrow method** (O'Neil, ACM
TODS 11(4), 1986): `reserve(worst_case) → execute → settle(actual)`,
atomic, against every ancestor, acquired in a fixed root-to-leaf order.

### 2.6 The other constructs

**T18 · Retry — the deadline dominates, not the attempt count. [E]**
gRPC gRFC A6 caps `maxAttempts` at **5 client-side regardless of config**
and states *"gRPC's call deadline applies across all attempts"*. Temporal
says the same independently: bound the total with a timeout, not with
`Maximum Attempts`. Full jitter always: `sleep = rand(0, min(cap,
base·2^(n−1)))`; server pushback (`Retry-After`) takes absolute priority
and resets the exponent.

**T19 · The commit point, not the error taxonomy. [E]**
gRPC A6: *"An RPC becomes committed … the client receives
Response-Headers"*, and committed ⇒ retry is invalid whatever the status.
This is the only **semantic** (non-taxonomic) retryability rule in the
literature — "is this error transient?" is a hand-maintained list
everywhere, but "was the effect possibly applied?" is decidable from
transport state. Per verb: `infer` commits at first token; `exec` commits
once the child is spawned; `invoke` commits unless the tool declares
idempotence.

**T20 · A depth cap bounds a call; only a budget bounds amplification. [E]**
Google SRE ch.21 ships two: 3 attempts per request **and** a per-client
retry ratio ≤10%, with the arithmetic — *"a threefold increase in requests
… layering on the per-client retry budget (a 10% retry ratio) reduces the
growth to just 1.1x"*. And the nesting rule: retry at exactly one level,
because N levels × k attempts is `k^N`. **An exhausted child must hand
its parent a non-retryable error.** This is the most likely latent bug in
any engine that lets both parent and child carry `retry:` — untested here
(§4).

**T21 · Caching: the Frankenbuild. [E]**
(Mokhov, Mitchell, Peyton Jones, *Build Systems à la Carte*, ICFP 2018,
§6.4) *"Deep constructive traces combined with task non-determinism can
lead to very subtle bugs … caching build results based only on the hashes
of terminal task inputs … the resulting store is incorrect according to
**all three** definitions of correctness."* Also: *"volatile tasks cannot
be cached"*, and Buck is named as the implementation that *"relies on
deterministic tasks"*.
Applied here: key a downstream task on the workflow's ROOT inputs, put one
`infer` upstream, and you can serve a downstream result that was never
produced from the upstream value in the store — and the hash chain records
it. **Shallow constructive traces only.** For an engine whose whole claim
is a checkable trace this is not a trade-off.

**T22 · The cache precondition is a pair, not purity. [E]**
Bazel establishes hermeticity by **sandboxing, not analysis** — an
undeclared input becomes a build failure, not a silent wrong cache hit.
REAPI's key is the Action digest over `{command, input_root, timeout,
platform, salt, do_not_cache}`; non-OK results MUST NOT be cached.
`salt` exists *"to disown an entire set of ActionResults that might have
been poisoned"*. Nix's fixed-output derivation is the transferable idea:
an impure fetch becomes cacheable **and** may hold network permits,
because the author declares the output hash — *"the name of the output
path only depends on the `outputHash*` and `name` attributes"*.
`permits:` is our version of Bazel's sandbox, declared at parse time
rather than discovered by debugging cache misses.

**T23 · Deadline propagation is the same meet as the budget. [E]**
SRE ch.22: *"servers should employ deadline propagation … The tree of RPCs
emanating from an initial request will all have the same absolute
deadline."* So `effective(child) = min(parent_deadline, now + own_timeout)`
— the identical semilattice as T17, over instants instead of dollars. It
is also what makes revocation real: on deadline or failure, cancel the
subtree and release its escrow. SRE names the leak: *"the initial call
continues to use server resources until it eventually times out, despite
being doomed to failure."*

**T24 · Sagas — and a lint we can ship today. [E]**
(Garcia-Molina & Salem, 1987) *"either the sequence [T₁…Tₙ] or [T₁…T_j,
C_j…C₁] is executed"*, compensations in **reverse order**. The
transferable consequence: **`permits:` tells us statically which tasks
need one.** A task with `write:`/`net:` permits and no declared
compensation is a workflow that cannot be safely aborted — a check-time
lint available today. Honest limit: an `infer` cannot be compensated (you
cannot unspend tokens) and an email cannot be unsent, so the guarantee is
weaker than the saga guarantee and must not be oversold.

### 2.7 The exemption design

**T25 · Every shipped suppression trusts its justification. [E]**
SPARK's own user guide, of `pragma Annotate (GNATprove, …)`: *"The
Category currently has **no impact on the behavior of the tool** but
serves a documentation purpose"*, and the reason is *"a string provided
by the user as a justification **for reviews**"*. Checker Framework:
*"The code is correct only if the checker issues no warnings **and** each
`@SuppressWarnings` is correct"* — a human audit obligation. SARIF's
`justification` is a `string`. No deployed counterexample was found.

**T26 · Relevance ≠ reason. [E]**
There IS a check in the wild, on a different axis. *"Does this suppression
suppress anything?"* is widely deployed — GNATprove (on by default), Rust
`#[expect]`, ESLint `--report-unused-disable-directives`, Checker
Framework `-AwarnUnneededSuppressions`. *"Is the stated reason true?"* is
deployed nowhere. Naming the axis is necessary or "GNATprove already
checks justifications" is a one-line rebuttal.

**T27 · The `assume` family is checked for sufficiency, never truth. [E]**
SPARK's `pragma Assume`, Checker Framework's `@AssumeAssertion`, Dafny's
`assume {:axiom}` + `dafny audit`: all enter the proof context and are
checked for what they DISCHARGE, never for whether they HOLD. Frama-C's
framing is the closest classical analogue — *"a plug-in's assumption is
another plug-in's goal"* — but ACSL has no suppression construct at all.

**T28 · The novel cell, stated precisely. [I, after a targeted search]**
Two 2026 systems do the semantic core on a different subject: **LeanGuard**
(arXiv:2607.03963) names *"premature discharge of safety obligations"*,
has typed protection kinds with a per-class entailment relation, and a
Lean-checked coverage predicate — *"a fluent justification is not a
proof"*. **Evident** (arXiv:2606.15122): *"dismissing a report requires
establishing that the reported error state is unreachable, not merely
offering a plausible explanation."* **In both, the justification is
produced by a triage LLM, not authored by a developer as a durable
suppression.** Machine-checking a *human-authored* exemption reason is the
unoccupied cell — and it is exactly the hole SPARK's own documentation
identifies.

**T29 · Design it like VEX, not like `# nosec`. [E]**
OpenVEX is the only mechanism that made justification mandatory, and it
did so with a **closed enum**, explicitly for machine-checkability. Its
own ruling on the free-text alternative: *"This field is not intended to
be machine readable so its use is highly discouraged for automated
systems."*
Empirical backing for scoping it to security: Liargkovas, Panourgia &
Spinellis (arXiv:2311.07482), 1,425 Java projects, 11,240 suppressions —
only ~5% were genuine false positives, but **security-category warnings
were suppressed markedly less** (4% of configs, <1% via annotation).
*"Developers take security-related warnings more seriously."*

**T30 · The PCA attack, and the adoption lesson. [E]**
Appel & Felten's CCS'99 proof-carrying authentication is the right
academic ancestor — the requester supplies a proof, the monitor only
checks it. The CMU implementation had to reject proofs containing illegal
axioms: *"the client could respond to a challenge by sending an axiom that
asserted the proposition it needed to prove."* **No exemption class may
ever assert its own conclusion** — a test every future class must pass.
And the failure story matters: Grey (SOUPS 2007) measured 6.6 s vs 14.7 s
unlock times — Grey was *faster* — yet 5 of 8 users said it felt slower.
PCA failed on **expressiveness**, not on the certificate idea. **A small,
closed, decidable vocabulary is adoptable; an open proof language is not.**

**T31 · Runtime approval is not the escape. [E]**
Willison, who coined the lethal trifecta (June 2025), does **not**
recommend human approval: *"The only way to stay safe there is to avoid
that lethal trifecta combination entirely."* Meta's Agents Rule of Two
puts it last and names its own failure: *"a user blindly confirming a
warning interstitial."* Anthropic's production telemetry: **users approve
93% of permission prompts**, and their response was not a better prompt
but a tiered allowlist plus a classifier — and in headless mode, they
terminate. Every production HITL system (Step Functions `waitForTaskToken`,
Temporal signals, LangGraph `interrupt()`, OpenAI Agents `RunState`) is an
out-of-band durable token; **none requires a tty**. And MCP elicitation is
capability-gated with **no specified fallback**, so building the escape on
it relocates the unreachability rather than fixing it.

### 2.8 What an LLM call actually costs

**T32 · `max_tokens` caps output only, and not even a whole turn. [E]**
Anthropic: *"`max_tokens` is a hard cap on total output for the request,
thinking and response text combined … **In a tool-use loop, each request
in the turn has its own `max_tokens`, so it doesn't bound the whole
turn's spend.**"* OpenAI's `max_output_tokens` likewise limits reasoning
+ visible + formatting output. **Nothing caps input.** C1 is therefore
not a rounding error: the omitted term is the unbounded one.

**T33 · The agent loop is Θ(n²) on the term we omit. [E]**
(arXiv:2606.14945, measured on 3 seeds) With `s` = system + tools resent
every turn, `t̄` = mean assistant output, `r̄` = mean tool result:

```
I(n) = n·s + (t̄+r̄)·n(n−1)/2        O(n) = n·t̄
C(n) = p_in·I(n) + p_out·O(n)
```

**The leading term is `p_in·(t̄+r̄)·n²/2` — the quadratic sits on the
input price, the one our formula omits entirely.** Measured: 15
iterations → 24,465 vs 2,492 tokens (9.8×); 40 iterations → 1.28M vs
627k. Caching multiplies the quadratic coefficient by 0.1; it does **not**
change the asymptotics. Only a bounded window does.

**T34 · Output is ~10% of the bill. [E]**
(arXiv:2607.12161 — 2,848 provider-billed Claude Code runs, reconstruction
matching individual bills to a ~1% median residual)

| Component | Share of bill |
|---|---|
| cache creation | **44.3%** |
| cache read | **35.4%** |
| generated output | **10.4%** |
| uncached input | 1.3% |
| unattributed | 8.7% |

So the ceiling models one term worth ~10% of the bill, and models it
~50× too loose. The two errors point opposite ways and do not cancel.

**T35 · The unsoundness has a measured exploit. [E]**
(arXiv:2601.10955) A protocol-compliant malicious MCP server steers an
agent into long tool chains **while preserving task success**, inflating
cost **up to 658× with `max_tokens` unchanged**. The framing is exact:
single-turn attacks are bounded by `≤ M`; the tool-loop attack is bounded
by `≤ n·M` where **`n` is unbounded**. A ceiling that can be exceeded by
658× is not a ceiling.

**T36 · `max_tokens` is the ONLY sound output bound — and that is a
theorem, not a limitation. [E + I]**
(arXiv:2604.00499 — 1,000 prompts × 100 generations) Output length is a
stopping time with a power-law tail: `P(L > n) ~ c·Γ(α)/n^α`. Empirically
skewness 3.10, CV 1.09, **P99/P50 = 10.77**. Best-fitting family is
**log-t** (93.1% KS pass, vs 60.3% log-normal, 10.7% exponential).

Two consequences, both mine by derivation:
- Fitting a Pareto tail to P99/P50 gives **α ≈ 1.65 < 2 ⟹ infinite
  variance**. No Chebyshev bound. And a sum over `N` tasks scales as
  `N^(1/α) = N^0.61`, so DAG cost **does not concentrate** — it is
  dominated by the single worst call.
- For `X = exp(μ + σY)` with `Y ~ t(ν)`, `E[X^k] = M_Y(kσ)`, and the
  Student-t MGF **does not exist for any nonzero argument**. So
  **`E[X^k] = ∞` for every `k > 0`** — the log-t has no finite mean.

**The empirical mean output length is an artifact of the truncation cap,
not a property of the model.** Everything other than the cap is a
quantile. Best predictor accuracy in the literature is R² ≈ 0.82 on the
*distribution parameters*, not the realisation.

**T37 · Caching breaks compositionality on four axes. [E]**
Not just existence (0.1× read vs 1.25× write — a **12.5× swing**), but:
**wall-clock timing** (a tool call longer than the 5-minute TTL re-bills
the whole Θ(n²) prefix at write rate), **concurrency** (*"a cache entry
only becomes available after the first response begins … wait for the
first response before sending subsequent requests"* — so **fan-out
destroys caching**), and **byte identity** (a documented invalidation
table: tool definitions, `tool_choice`, images, thinking params each
invalidate a different cache scope).
So cost is not a function of the DAG. It is a function of (DAG, schedule,
wall-clock, concurrency policy, cache history). **A sound ceiling must
assume the cache-pessimal case: every call writes, none reads.**

**T38 · Long context is a step function on the whole request. [E]**
Bedrock: *"For requests exceeding 200K input tokens, the long context rate
applies to the entire request, not just the tokens above the threshold."*
Google above 200k: input ×2.0, output ×1.5. The cost function is
**discontinuous in input length**; a sound bound evaluates on the worse
side of every threshold the input range straddles.

**T39 · Tool definitions are billed input, and the constants are
published. [E]**
Anthropic tabulates the tool-use system-prompt overhead per model
(Opus 5: 286 `auto` / 406 `any`; Sonnet 5: 354 / 474) plus per-tool
constants (bash 325, text editor 700, computer use 735). Structured-output
schemas are billed input too (*"serves as a prefix to the system
message"*). **All of this is exactly computable from the workflow file
and is absent from our formula.**

And a class no token formula can express: web search **$10 / 1,000
searches**, file-search calls $2.50/1k, code-interpreter containers
$0.03–$1.92 per session, Anthropic code execution **$0.05/hour**, Google
explicit cache **$1.00–$4.50 per 1M tokens per hour**. Dollars per call
and dollars per wall-clock hour.

**T40 · Rate limits split exactly along the static/runtime line. [E]**
Provable conformance is available on RPM/RPD (cost = 1) and on **ITPM**
(input tokens are exactly computable before dispatch — tokenize), via
GCRA over a token bucket; Anthropic confirms *"the API uses the token
bucket algorithm"*. Not provable on **OTPM** (settled on actuals) nor on
Google's rolling 10-minute **dollar** window (which needs the cost model
we are trying to build).
Two vendor asymmetries worth encoding: OpenAI reserves
`max(max_tokens, estimate)` at admission and **429s consume quota**;
Anthropic states `max_tokens` *"does not factor into OTPM"*, so it costs
nothing in quota and buys nothing in predictability.
And the ablation that decides the design (HiveMind, arXiv:2604.17111):
**admission control alone still produces 81.8% failure**; transparent
retry is the critical primitive. Their diagnosis of an 11-agent run that
lost 3: *"The problem is not capacity — it is coordination."*

**T41 · The solved analogue is GraphQL, and its answer is
declare-and-settle. [E]**
Same problem shape: a declarative query language that must bound cost
before execution, where true cost is data-dependent. Static analysis
exists (arXiv:2009.05632) and its follow-up admits it was too loose in
practice (arXiv:2108.11139). **The industry resolution is not prediction.**
GitHub's GraphQL API makes `first: n` **mandatory on every connection**,
computes the estimate from the declaration, charges against it, and
refunds the difference. Bedrock does the same server-side: reserve
`input + max_tokens`, settle to actual, replenish the unused.
**This independently validates Q6** — declare the slice, do not predict
the data.

**T42 · No published sound static cost bound for LLM pipelines exists. [I,
after a six-framing search]**
The correct formalism exists and is unapplied: expected-cost analysis for
*probabilistic* programs via the potential method (arXiv:2006.14010),
which is the right frame precisely because output length is a random
variable. The cost-aware routing literature (FrugalGPT, RouteLLM,
TREACLE, SCOPE) **minimises expected cost subject to a budget; none
produces a sound per-run ceiling.**

#### The corrected formula

```
C(c) = Θ(c)·[ p_in·( U_c + μ_w·W_c + μ_r·R_c ) + p_out·K_c ] + Λ(c)

  Θ(c) = τ·γ·χ                    tier × geo × long-context step
  Λ(c) = per-call server-tool charges
  U+W+R = S_c + T_c + Σ_c + D_c(N_c)

    S_c    tokenize(system + template literals)      STATIC EXACT
    T_c    tokenize(tools JSON + output schema)      STATIC EXACT
    Σ_c    tool-use overhead constants(model)        STATIC EXACT — published
    D_c(N) (K_c + r_max)·N(N−1)/2                    ← THE QUADRATIC, absent today

Ceiling = Σ_c A_c·F_c·C(c)  +  Σ_h ρ_h·Δt_h
```

| Term | Class |
|---|---|
| prices, cache multipliers, tier, geo | **static** — pin the price-table version into the promise |
| `S_c`, `T_c`, `Σ_c` | **static exact** — tokenize the file; free accuracy |
| `K_c` (`max_tokens`) | **static**; sound; irreducible (T36) — but make it **per-task declared** |
| `A_c` retries | **static** — report separately, never folded into the headline |
| `F_c` fan-out | static if literal, **unbounded if data-dependent** — must be declared |
| `N_c` agent turns | **UNBOUNDED — must be declared.** Absent today = the unsoundness |
| `r_max` tool-result cap | **UNBOUNDED — must be declared.** A `grep` is not bounded |
| `U/W/R` split | **runtime** (schedule + TTL + concurrency) — assume all-write |
| `χ` crossing | **runtime** — evaluate on the worse side |
| `Λ`, `ρ_h·Δt_h` | **runtime wall-clock** — no token formula expresses these |

#### Decomposing the 126×, and where it flips sign

```
Ratio = (max_tokens / E[out]) × (retries / E[attempts])
        × (fan_out / E[fan_out]) × output_share_of_bill
```
For a short-output task (`max_tokens` 16,384, actual ≈ 400, retries 3,
output ≈ 75% of a small bill): `41 × 3 × 1 × 0.75 ≈ 92`; at 32,768,
≈ 184. **126× sits squarely in that band**, dominated by `max_tokens`
slack (~40–50×) and an unconditional retry multiplier (3×).

**On an agent loop the same formula flips sign**: `output_share = 0.104`
and the missing input term grows Θ(n²). The ceiling under-estimates,
without bound.

---

## 3 · The resource vector

Money is one dimension of at least nine. The composition rule per
dimension is what decides whether the checker can bound it.

| Dimension | `;` | `‖` | Structure | Static? |
|---|---|---|---|---|
| 💰 money | `+` | `+` | strong duoidal — schedule-invariant | **yes**, given bounds |
| ⏱ time | `+` | `max` | **lax** — this is the wave penalty | bound only (W5) |
| ⚡ energy / GPU-s | `+` | `+` | strong | yes, given a throughput model |
| 👤 human attention | `+` | `+` | strong | yes — gates are countable |
| 📊 tokens | `+` | `+` | strong | feeds TPM admission |
| 🔓 leakage (bits) | `+` | `+` | strong | yes — T15 |
| ↩️ irreversibility | `+` | `+` | strong | yes — from `permits:` + `compensate:` |
| 🎲 reliability | `×` | `×` | probability semiring | yes, given per-task rates |
| 💥 blast radius | `∪` | `∪` | join-semilattice | yes — bounded by `permits:` |
| 🔑 authority | `⊓` | `⊓` | meet-semilattice — **descends** | yes — `permits:` |
| 🏷️ taint | `⊔` | `⊔` | join-semilattice — **ascends** | yes — provenance |

**One traversal, N semirings.** This is the algebraic path problem
(Tarjan, JACM 28(3), 1981; Lehmann, TCS 4(1), 1977) — a single fold
parameterised by a closed semiring computes every row. Writing one
traversal per metric is the refactor to avoid.

**Two symmetries worth stating in the spec.** Money, energy, attention,
bits and irreversibility are **strong** duoidal morphisms — additive under
both compositions, hence invariant under any schedule, hence checkable
before a run whatever the scheduler does. Time alone is **lax**, and the
non-invertibility of its 2-cell *is* the scheduling loss. And authority
descends by `⊓` while taint ascends by `⊔`; they meet exactly at effect
sites, which is where the trifecta is judged.

### 3.0 Corrections to the table above

The table is the right shape and four of its rows are wrong. Recorded
here rather than silently rewritten, because each correction is load-
bearing.

**X1 · Money is not `+` under retry. [E]**
Retry-with-backoff multiplies by the attempt count. The sound form is
`Σ max_tokens × price × max_attempts` — and per T34 that 3× should be
reported **separately** from the nominal ceiling, because it fires on a
tail event and folding it in is a 3× lie in the modal case.

**X2 · Energy is `+` only on DISTINCT hardware. [E]**
Two concurrent calls to the same local GPU **share the weight read** —
that is what batching is. Measured (ML.ENERGY, Llama 3.1 8B, H100):
560 J/generation at batch 32 falls to 152 J at batch 512, a 3.7× spread
from configuration alone. So `+` remains a sound **upper** bound and is a
loose one exactly where local inference matters.

**X3 · Time is a PAIR, not a scalar. [E]**
```
   T_1 = Σ over tasks (the money-shaped sum)      commutative monoid
   T_∞ = max over paths (the span)                max-plus semiring

   max(T_∞, T_1/p)  ≤  T_p  ≤  T_∞ + T_1/p       (Brent)
```
It degrades correctly at both ends: `p = 1` reduces to the sequential
sum, `p = ∞` to the span. **Report the pair.** And the implementation
consequence is real: money, energy, tokens and interrupts need only the
**multiset of tasks** — a fold over the task list. Time needs the
**edges** — a topological traversal. That justifies two code paths, not
one.

**X4 · Human attention: the unit is a COUNT, and the folklore figure is
misquoted. [E]**
The "23 minutes to refocus" is Mark, González & Harris, CHI 2005, and it
says **25 min 26 s (sd 54 min 48 s)** — *elapsed wall-clock before
returning to the task, during which the worker did 2.26 other real pieces
of work.* It is **not** lost productivity and not a refocus time. The
standard deviation is twice the mean.
And the controlled follow-up (Mark, Gudith & Klocke, CHI 2008) inverts
the naive model: interrupted work is completed **faster**, with *"more
stress, higher frustration, more time pressure, and effort."* **The cost
is load, not duration.**
Therefore the checkable unit is **the count of human gates on the
critical path** — a pure graph property, zero model risk, and the
highest-value/lowest-cost component of the whole vector. Attention-
seconds is a report-time enrichment, never a check-time bound. And
*blocked wall-clock* is a different quantity again: a gate answered
overnight costs 8 h of latency and ~30 s of attention.

### 3.0.1 The component that was missing, and the one that is inverted

**Tokens are the dimensionless invariant, and they should lead.** They
are exactly boundable pre-run, provider-independent, and **every other
dimension is `tokens × a-coefficient-you-may-not-have`**. Publishing
tokens is publishing the uncertainty budget honestly.

**And `0.00` for a local model is not conservative — it is inverted.**
`find_pricing_for("ollama/…")` returns `None`, which renders as free. The
most resource-intensive execution path is reported as the cheapest. The
doctrine already names this — *"a local model is unpriced, never free"* —
so the fix is not new physics, it is making **`⊥ unknown` a first-class
rendered value** instead of zero. That single change delivers most of the
vector's value without asserting a single joule.

**Which components may FAIL a run:**

```
   BOUND (may fail)      tokens · money-on-priced-APIs · interrupt-count
                         ← sound derivations, all three

   ESTIMATE (may warn)   time · energy · attention-seconds · carbon
                         ← mandatory provenance: measured@sha,
                           calibrated@host, or ⊥ unknown
```

**X5 · Do not combine the dimensions. [E]**
Weighted sum can only reach points on the **convex hull** of the Pareto
front — it cannot produce any point in a non-convex region for *any*
weights (Das & Dennis, *Structural Optimization* 14(1), 1997). It also
encodes an exchange rate between €, seconds, joules and human attention
that we have no basis to set. Lexicographic order makes dimensions 2..n
dead letters, since continuous money never ties.
The honest structure is the **product order on ℝⁿ₊ — which is exactly
Pareto dominance**, and it is *partial*. Most workflow pairs are
**incomparable**, and saying so beats inventing a weight. Two exceptions:
summing within one dimension across providers is legitimate; and an
operator-supplied exchange rate may produce a derived scalar **at report
time only, never at the gate**.

**X6 · DRF is not the model. [E + I]**
Dominant Resource Fairness (Ghodsi et al., NSDI 2011) is an **allocation
mechanism among competing users** — its strategy-proofness and
envy-freeness apparatus is about lying agents, and there are no agents in
`nika check`. Worse, its central move is a **max over normalised shares**,
i.e. precisely the collapse-to-a-scalar this vector exists to avoid. It
becomes relevant only if we ever schedule *concurrent workflows* against
a shared GPU, a shared rate-limit budget and a shared human reviewer.

**X7 · The architectural precedent is EnergyAnalyzer, not AARA. [E]**
AbsInt's EnergyAnalyzer *"utilises techniques usually used for worst-case
execution time (WCET) analysis together with bespoke energy models"* —
**one analysis engine, N pluggable cost models**, shipping, in the
TeamPlay project whose stated goal is *"a toolchain where energy
properties are first-class citizens."* That is exactly the
one-traversal-N-semirings design.
And a correction to a likely misreading: **"multivariate" in *Multivariate
Amortized Resource Analysis* means multivariate in the ARGUMENT SIZES,
not multiple resource metrics.** RAML is metric-*parametric* — one metric
per pass, pluggable — which is still the right architecture, just
instantiated N times.

**X8 · The prefill quadratic does not dominate at our sizes. [E + derived]**
Per transformer layer with hidden dim `d` and sequence `S`:
`projections + FFN ≈ 24·d²·S` versus `attention ≈ 4·d·S²`, so the ratio
is **`S / (6·d)`**. Checked against published measurement (Llama-2-7B,
d=4096, S=2048, A6000): attention is 68 G of 899 G ops = **8.2%**;
`S/(6d) = 8.3%`. Crossover is ~24k tokens for `d=4096`, ~49k for
`d=8192`. So "prefill is quadratic" is folk shorthand, not a usable model
below ~24k context.

**X9 · Wall-clock time is the wrong shape for a scalar. [E]**
Measured p99/p50 of LLM API latency is **3–5×**; on real coding-agent
traces the **mean is 7× the median** (avg 4.3 min, median 38 s, p90
6.4 min). Nineteen endpoints serving the *same model* spread TTFT p99 by
**2.9×**. And no provider publishes an enforceable per-request p99
guarantee. So a single "estimated time" is not merely imprecise — it is
the wrong shape. Publish **span-in-tokens** (a pure graph+bound property)
and convert to seconds only against a named, dated calibration.

### 3.1 Three natures, not one

Not every constraint is a fold over the DAG.

```
ACCUMULATED   a fold over the graph        money · time · energy · attention
              a bound is a TOTAL            bits · irreversibility · reliability

POINTWISE     checked per task             context window · max_tokens
              a bound is a MAXIMUM          ⟹ a prompt + interpolations that
                                              overflows the window FAILS —
                                              not checked today

WINDOWED      checked per interval         RPM · TPM · concurrency
              a bound is a RATE             ⟹ not a property of the GRAPH at all,
                                              a property of the SCHEDULE
```

The windowed class is why a per-provider counter is needed and why it
cannot be a check-time verdict. It is the `requests:` / `limits:` split
(Kubernetes) applied to rate rather than quantity: `check` reads the
declaration, the runtime enforces the rate.

### 3.2 The three catalogues

| Catalogue | Has | Missing |
|---|---|---|
| models | in/out price, cache rates, context window | throughput (tok/s), TTFT, params (for local energy) |
| builtins | surface, args | cost class (pure / IO / subprocess) |
| **MCP** | id, packages, remotes, env | **everything** — cost, latency, rate limits, reliability |

The MCP row is the one nobody else holds. The official registry answers
*"this server exists, here is how to install it"*. The question a workflow
engine needs answered is *"this server costs ~X ms, ~$Y, fails Z% of the
time, caps at N req/min"* — and the engine that **executes** the calls is
the one structurally positioned to measure it.

### 3.3 Three dimensions the refutation pass revealed

Each passes the resource test — consumed, composes under both `;` and `‖`,
boundable — and each is decidable from data the engine already holds.

**🔁 REPRODUCIBILITY · `∧` under both.**
A run is replayable iff EVERY task is. That is a meet on booleans, so it
composes trivially and is statically decidable: `run: entropy:` is already
declared (`ambient` · `none` · `{seeded: n}`), the verb kinds are known, and
`nika test --update` already pins a golden. The engine has the parts and does
not report the aggregate. Worth having because it answers the question a
reader of a trace actually asks — *"can I get this back?"* — and because it is
the precondition for every caching claim (T21: a volatile task cannot be
cached, and volatility is exactly `¬reproducible`).

**⏳ STALENESS · `max` under both.**
The age of the OLDEST input a run depends on. Composes as a max because a
workflow is as stale as its stalest dependency. Statically knowable for
everything the engine pins: `pricing.as_of` (measured 21 days at time of
writing), `SPEC_PIN`, a golden's date, a cached artifact's TTL. This is not
an abstraction — it is the concrete defect §5's item on the pricing heal
exists to fix, generalised. A ceiling computed against 21-day-old prices and
a ceiling computed against today's are different promises, and only one of
them says so.

**🧬 PROVENANCE DEPTH · `max` under both.**
How many hops separate a value from a source the run did not produce. Zero
for a `const:`, one for a `nika:fetch`, more through a chain. It composes as
a max and it is the quantity that makes the taint lattice *legible*: today
`🏷️ taint` is a set of origins, which answers "is this tainted" but not "how
far from trusted". Depth answers the second, and it is what a reviewer of an
`accept_flow:` discharge actually needs to judge.

| Dimension | `;` | `‖` | Structure | Static? |
|---|---|---|---|---|
| 🔁 reproducibility | `∧` | `∧` | meet on booleans | **yes** — `run: entropy:` + verb kinds |
| ⏳ staleness | `max` | `max` | tropical | **yes** — every pin carries a date |
| 🧬 provenance depth | `max` | `max` | tropical | **yes** — the flow graph already has it |

Note the shape: **two of the three are tropical, like time.** That is not a
coincidence. Additive dimensions answer *"how much in total"*; tropical ones
answer *"what is the worst point"*. A workflow's staleness is not the sum of
its inputs' ages any more than its duration is the sum of its tasks'.

### 3.4 What the upstream carries and we do not read

Measured against `https://models.dev/api.json` on 2026-07-28: 173 providers,
5,810 models, 21 model fields. Restricted to our 10 providers — 653 models —
the import reads **four** of them.

| Field | Coverage | What it unlocks |
|---|---|---|
| **`limit.output`** | **653/653 · 100%** | **the max_tokens the model actually accepts.** A workflow declaring `max_tokens: 100000` against a model capped at 8192 is wrong, and check could say so today. It also TIGHTENS the ceiling rather than merely making it honest. |
| `limit.context` | 653/653 · 100% | the context window — today hand-written on 69 lines of `llm-providers.toml`, which the file itself calls "a manually-curated snapshot" |
| `open_weights` | 653/653 · 100% | local vs cloud, DECLARED — so "unpriced, never free" stops being inferred from a missing price |
| `tool_call` · `reasoning` | 653/653 · 100% | a workflow handing tools to a model that takes none is a runtime failure check could catch |
| `structured_output` | 487/653 · 74% | same, for `schema:` |
| `temperature` | 650/653 · 99% | |
| `knowledge` | 400/653 · 61% | the cutoff — "this model cannot know about that" |
| `status` | 35/653 · 5% | **deprecated** — we may be teaching a model that is going away |
| `modalities` | 653/653 · 100% | |
| `cost.reasoning` | 18/653 · 2% | a SEPARATE reasoning rate the formula ignores |
| `cost.tiers` | 31/653 · 4% | long context, with the threshold as an explicit value |
| `cost.input_audio` | 14/653 · 2% | |

**`limit.output` is the pick of the lot**: total coverage, zero new supply
chain, and it converts an unchecked `max_tokens` into one bounded by the
model. That is one more false green closed, from a file we already fetch,
hash and pin.


---

## 4 · What is not tested

Stated plainly so nothing here is over-trusted.

- **The renderer (F).** `pyte` is absent; the frame-duplication repro was
  never run. It is the one operator finding I have not reproduced.
- **Retry amplification `k^N`** under parent/child composition. My probe
  was malformed (no child file); the class is not cleared.
- ~~**DAG width across the corpus.**~~ **CLOSED — see S9.** Max 4, median
  2, over 43 workflows. The cap does not bind, so `P∞` holds and greedy
  is exactly optimal. Residual: 33 corpus files the parser skipped.
- **Provider billing on mid-stream failure.** Anthropic documents that
  SSE errors arrive *after* a 200, which implies generation occurred and
  was billed, but no primary billing statement was found either way.
- **Whether the golden traces normalise timestamps.** The chain hashes
  *"the PREVIOUS line's exact bytes"* and each line carries `at:`, so the
  chain is order-integrity, not byte-reproducibility. What the golden
  pins actually compare has not been read.

---

## 5 · Ordered work

Ranked by (damage closed × confidence), mechanical first.

| # | Work | Basis | Blast radius |
|---|---|---|---|
| 1 | Dataflow scheduler: ready-queue, in-degree, tie-break by **declaration index** | T1 — reaches the optimum, not an approximation | runtime |
| 2 | Publish `T_1` and `T_∞` per run | T5, W5 — turns a benchmark into a contract | reporting |
| 3 | Ceiling reads `usd_for_split`, not `output_per_million` | C1 vs C2-C4 — closes a **soundness** hole | check |
| 4 | Gate-aware cost via Melani Alg. 1, **propagating sets** | T7, T8 | check |
| 5 | Parametric bound `A + B·n` instead of `∞` | T10 | check |
| 6 | Phantom / redundant `after:` hint | S5, S6 — structural, zero statistics | check |
| 7 | `pc` label on control ancestors | T14 — four channels, one mechanism, zero annotations | check |
| 8 | Per-provider concurrency class | S7, T1's exogenous-duration hypothesis | runtime |
| 9 | Cardinality bound (`take:` / declared size) | V3-V5, T15 — makes the ceiling computable **and** the leak quantifiable | language |
| 10 | `max_turns` on every `agent`, `max_result_bytes` on every tool — and **refuse to emit a ceiling without them** | T33, T35 — the refusal is the feature; without them the number is unsound, not loose | language |
| 11 | Report THREE numbers: `Ceiling` (sound, cache-pessimal, retries excluded), `Ceiling+R`, `Estimate p95` — plus the price-table version hash | T34, T36 — promise the first, plan against the second, never conflate them | check |
| 12 | Declare-and-settle: reserve the ceiling, meter actuals, release the difference, abort at the promise | T41 — GitHub GraphQL and Bedrock burndown both do exactly this; it converts an unachievable prediction problem into an achievable enforcement one | runtime |
| 13 | GCRA admission on RPM + ITPM per (provider, model); reactive on OTPM | T40 — provable where the debit is statically known, reactive where it is not. **Retry is the critical primitive, not admission** (81.8% failure with admission alone) | runtime |

### 5.1 Ratified this session

Recorded so they are not re-litigated. Each was decided after the
research that supports it, and each has a named counter-argument.

| # | Question | Ratified | Basis · counter |
|---|---|---|---|
| Q1 | Where does the human decision live? | **AUTHORED**, load-bearing; runtime approval is a convenience and **never** the security boundary | T31 · counter: an authored exception is granted before the attacker's payload exists, so it catches 0% of live payloads *unless* the class is verified (T28) |
| Q2 | What arms leg ② of the trifecta? | **PROVENANCE** — reading what this run itself wrote introduces no new information, so it arms nothing | G1-G3 · counter: `permits:` declares *authority*, and a future edit adding a real private read would pass under body-analysis. Answered by the fact that check re-runs at every edit and at the run gate |
| Q3 | What shape is the exemption? | **CLOSED ENUM, each class a machine-VERIFIED precondition** + a mandatory `because:` for the diff reader. No `other`, no free-text escape | T25-T29 · counter: an optional declaration nobody writes is a guarantee nobody gets (Kubernetes' own documented failure) |
| Q4 | Which classes ship? | **`egress_destination_controlled`** only. `human_validates_payload` is gated on a TTY prompter existing; `content_not_interpreted` does **not** ship; `data_not_sensitive` is withdrawn | T16 · non-interpretation ≠ non-influence, so it is a partial condition and cannot discharge a whole refusal — my own law |
| Q5 | Failure policy under dataflow | **DRAIN** — release on structural eligibility only, let every released task settle, append in declaration-index order, causal failure = lowest declaration index | the trilemma (§5.2) · counter: a wide fan-out keeps spending after the run is doomed. Bounded by numbers we control statically; the nondeterminism of the alternative is not |
| Q6 | Cost bound on dynamic fan-out | **`take: N`** — a decision about the work, not a prediction about the data — over a required `max_items:`, which the ecosystem would satisfy with `1000000` | Mitchell's arc, three times: ban → proclaim → find it clunky → restore. Rust RFC 2834's predictor: a rule sticks when the fix is local and mechanical; `max_items` is mechanically trivial and semantically impossible |

### 5.2 The scheduling trilemma

Stated because it forecloses the option that looks most attractive.

```
   ① eager release        a task starts the instant ITS predecessors settle
   ② global gating        admission conditioned on "has anything failed yet"
   ③ deterministic chain

   at most TWO.

   wave barrier      ② + ③   sacrifices ①    ← today
   dataflow drain    ① + ③   sacrifices ②    ← ratified
   cancel-on-first   ① + ②   sacrifices ③
   Argo/GitLab quiesce  ①+②+almost ③ — the residual race is which eligible
                        tasks were released in the instant the gate closed
```

② is fatal because *"has anything failed yet"* is a function of settle
order, i.e. of I/O timing. A gate reading only *"did MY predecessors
succeed"* is a pure function of the DAG plus per-task outcomes.

Corroboration that the sacrificed option really does sacrifice ③: OpenJDK's
`StructuredTaskScopeImpl` **drops** a sibling's exception when the scope
is already cancelled — `if (scope.isCancelled()) return;` before the
exception is ever stored. Which task wins is pure timing. Go's `errgroup`
author gives the reason it is defensible there and not here: *"After
[cancellation] happens, it can be difficult to tell whether subsequent
errors are errors in their own right, or secondary effects of that
cancellation."*

What must be recorded, per task, appended in ascending declaration index:

```
{ decl_index, task_id, outcome }
outcome ∈ Ok{result_digest} | Err{error_class, payload_digest}
        | NotRun{blocking_edge, blocking_task_decl_index}
```

No `Cancelled` and no `Unobserved` state may exist — both are timing
artefacts by construction, so under this design they are
**unrepresentable**. The type system holds the determinism, not the
discipline.

And the reporting consequence, which is the operator's 21 identical `⊘`
lines: Airflow separates `failed` from `upstream_failed`. All 21 are the
second kind. **None of them is a failure** — they are the shadow of one.
The honest report is `1 failure: task X. 20 tasks not run (predecessor X
failed).` Under cancel-on-first that sentence cannot be written: it would
have to read `1 failure, N unobserved`, the one line nobody can act on.

Deliberately **not** on this list, with the reason:

- a cost-based reordering optimiser — the prize is capped at `2 − 1/m`
  (T2) and greedy already collects it; the cost model is the component
  the database literature finds least valuable (Leis et al., VLDB 2015);
- a learned duration predictor feeding plan choice — maximum exposure to
  *"fleeing from knowledge to ignorance"* (LEO, VLDB 2001), where exact
  costs on measured paths plus optimistic priors on unmeasured ones make
  decisions **worse** than uniform ignorance;
- speculation of `when:`-gated tasks — tokens are billed, the trace
  becomes timing-dependent, and under a binding cap it strictly increases
  makespan;
- a signed execution certificate — it deletes the best bug detector we
  have. Today a wrong check is caught by a dead run and reported. Under a
  certificate, a permissively-wrong check authorises the effect and the
  signal disappears. This is the Special J failure (Necula's PCC: a
  23,000-line VCgen with a confirmed soundness hole that the certificate
  did not catch).

---

## 6 · Retracted

Six claims I made in this session and then withdrew, with what killed
each. Recorded because the reasoning that produced them is plausible and
will be produced again.

**R1 · "A workflow engine needs a query optimiser."**
The analogy: SQL derives the plan, nobody hand-writes one, so an author
moving tasks by hand means an optimiser is missing.
*Killed by:* the DAG is the **query**, not the plan. Nobody hand-writes a
query plan, and everybody hand-writes a Makefile — both are correct.
Graham's `2 − 1/m` caps the entire ordering prize and blind greedy already
collects it; Leis et al. (VLDB 2015) find the cost model is the *least*
valuable optimiser component, and tuning it with true cardinalities
**degraded 35%** of queries; Bazel, Buck2, Nix, Ninja and Make do not
rewrite their graphs, and Buck2's headline 2× came from *"avoiding any
phases"*.

**R2 · "Use the hash-chained history to plan the next run better."**
*Killed by:* **"fleeing from knowledge to ignorance"** (LEO, VLDB 2001).
History gives exact costs on paths actually taken and optimistic priors
on every alternative, so the optimiser abandons the plan it has evidence
for. *The better the measurement, the worse the decision.* Named,
documented, and it targets precisely the asset that looked like our
advantage.

**R3 · "`check` emits a signed certificate; `run` executes only what it
authorises."**
*Killed by:* it deletes our best bug detector. Today a wrong check is
caught by a dead run and an operator writes a report — that is literally
how this session's work began. Under a certificate, a *permissively*
wrong check authorises the effect with the plan's blessing and the signal
disappears. The Special J precedent: Necula's PCC shipped a 23,000-line
VCgen with a soundness hole that League found and Necula confirmed — the
certificate did not catch it, *the certificate is why nobody was looking*.
Also: the certificate is **orthogonal to the reported defect**, which is a
completeness bug, not an enforcement one. What survives is capability
threading (a permit type with a private constructor — a compile-time
guarantee, strictly better than a runtime signature) plus an unsigned,
content-addressed plan as a debugging record.

**R4 · "The wave decomposition is the coarsest series-parallel
over-approximation; the 2× is the N-freeness defect."**
*Killed by:* a four-node counterexample. `a1→a2 ‖ b1→b2` **is**
series-parallel, contains no N, and loses 1.998× with
`d = (1000,1,1,1000)`. Also "coarsest SP" is degenerate (a linear order is
SP) and minimum SP extensions are **not unique** — the N poset has three,
pairwise incomparable. The true statement is W7: waves are the depth-2
weak order, the Foata normal form, and the penalty is one application of
the CKA exchange law.

**R5 · "The cost ceiling and the critical path are the same computation."**
*Killed by:* `cost.rs:94` — `for task in &wf.tasks`, a flat sum. And the
code is right: money is additive under parallelism, time is not. The real
unification is finer and better — same traversal, different semiring:
`(+, ×)` for money, `(max, +)` for time.

**R6 · "A required cardinality bound makes the conformance promise true."**
*Killed by:* Terraform is six years into paying its way out of exactly
this refusal (`-allow-deferral`, alpha through 1.16), and Dhall's own
author on the totality claim: *"The absence of Turing completeness per se
does not provide many safety guarantees … you can craft compact Dhall
functions that can take longer than the age of the universe."* The
predicted equilibrium of a required bound is `max_items: 1000000`
everywhere — a *true* ceiling with no operational content, **worse than
today's honest warning**. What survives is `take: N`, which is a decision
about the work rather than a prediction about the data, and therefore
always answerable.

**One retraction I did NOT make, and the evidence for it.** An audit agent
claimed my wave benchmark did not match its own mechanism — that two
independent 5 s tasks form one wave, so waves predict 5 s, not 10. The
plan dump refutes it:

```
✔ PLAN  4 waves · 5 tasks
   wave 1  slow_early (5s) · c1 (~0ms)
   wave 2  c2 · wave 3  c3 · wave 4  slow_late (5s)
```

`slow_late` sits at depth 3 behind the `c1→c2→c3` chain, so the durations
are heterogeneous and W3 does not apply. `T_wave = 5 + 0 + 0 + 5 = 10`,
measured 10.1 s.

---

## 7 · The method that produced this

Stated because it is the transferable part.

1. **Measure before believing, including your own claims.** Every row of
   §1 exists because a claim was checked. Three of the six retractions in
   §6 are mine, killed by my own probes.
2. **A gate's repair is measured against the whole corpus before it is
   kept.** The `fn-length` fix looked right, turned one false positive
   into five, and was reverted (R1-R4 in §1.8).
3. **The fix lands in the source, never in the mirror.** Five examples
   were repaired in `crates/nika-pack/pack/` — the vendored copy, pinned
   by `SPEC_PIN` and re-vendored daily. They would have been overwritten
   at the next bump while the source kept teaching the defect.
4. **A gated operation is the last command in its chain.** A `git log`
   after a `git commit` returns 0 over the commit's failure; that masked
   two failures this session before the pattern was named.
5. **Cite a file:line or a URL, or mark it inference.** Everything in §2
   carries one or the other.
