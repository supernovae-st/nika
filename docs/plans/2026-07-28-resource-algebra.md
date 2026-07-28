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

### 1.7 The MCP catalog

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

---

## 4 · What is not tested

Stated plainly so nothing here is over-trusted.

- **The renderer (F).** `pyte` is absent; the frame-duplication repro was
  never run. It is the one operator finding I have not reproduced.
- **Retry amplification `k^N`** under parent/child composition. My probe
  was malformed (no child file); the class is not cleared.
- **DAG width across the corpus.** Two attempts timed out. Until it is
  measured, "the cap never binds in practice" is an assumption, not a
  fact — and it is the hypothesis T1 rests on.
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
