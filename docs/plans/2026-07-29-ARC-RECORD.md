# The arc, end to end · 2026-07-28/29

What was asked, what was built, what was only written down, and what is still
open. Written for someone who was not here. Companion to
`2026-07-29-HANDOFF.md` (what to do next) and
`2026-07-28-verdict-coverage.md` (every repro).

**The honesty rule this document follows**: a section marked SHIPPED has code
and a passing test behind it. A section marked RECORDED has a document and no
implementation. The distinction is the whole value of the file — the arc
produced a great deal of both, and conflating them would be the exact defect
the arc spent two days removing.

---

## §1 · How it started, and how the subject changed

It began as an authoring problem. A Cursor session was writing Nika workflows
and reaching for Python glue whenever a builtin refused it. The ask was: make
the engine SOTA, be maximally ambitious, research deeply.

Four things were asked for in sequence, and all four were pursued:

1. **The resource algebra** — price, but also time, hardware, server, operator,
   energy. "Keep all your calculations and theorems, keep a good history."
2. **Our own catalog** — rich, maintained, verifiable, refreshed by a Nika
   workflow (dogfooding), published with an arena / benchmark / leaderboard.
3. **A permanent doctrine** — friction writing a Nika workflow anywhere in the
   ecosystem must feed back into the spec and engine.
4. **Rebuild the example corpus to SOTA** — nuke and redo, nothing outdated,
   nothing badly taught.

**The subject changed on day two**, and it was an adversarial research pass
that changed it. Sent to VALIDATE a fix, it found the fix was addressing the
wrong bug: a shipped fail-open in the filesystem permit boundary was one
function away, in a crate already open on screen. Everything after that
reordered around verdict soundness.

---

## §2 · SHIPPED · code, tests, committed

### 2.1 · The security fix · `3746c9a37`

Measured on the **published** `nika 0.106.1` — Homebrew, npm, Docker — with no
attacker, no symlink, no traversal:

```
permits.fs.read:  ["data/*.csv"]   read  data/sub/deeper/private.key
permits.fs.write: ["out/*.md"]     wrote out/sub/pwned.sh
```

Both green at check with `0 hints`, both green at run, and the read's content
landed in the **signed trace**. A permit naming CSV files read a private key
three directories down; a permit naming markdown wrote a shell script into a
subdirectory it never named.

**Cause**: `literal_root()` walked the glob, stopped at the first component
containing `*`, and returned `(prefix, true)`. The pattern was discarded, and
`confines()` admitted anything under the resolved prefix. `data/*.csv` meant
`data/**`. Its own comment said so, which is why a unit test asserted the
behaviour rather than catching it.

**Fix**: one predicate, `nika_cap::glob_admits`, segment-aware, `*` never
crossing `/`, used by BOTH check and runtime — the arrangement hosts always had
via `nika_types::net::host_glob_matches`. `nika-cap` moved from a
dev-dependency to a real one.

**Why the guard that exists for this did not fire**: a differential proptest
compared the two implementations "because they share no code, so a common bug
would have to be born twice". It ran green because its generator emitted only
`<segs>/**` and literal paths, excluding mid-pattern globs on the grounds that
they are "a KNOWN non-decidability". That theorem is true and it is about
containment between two PATTERNS; both sides match a CONCRETE path against ONE
pattern, which is ordinary glob matching. **A correct theorem waived a proof
obligation it did not govern, and the fail-open lived in the gap.**

### 2.2 · The verdicts stopped overclaiming · `761892858`, coverage lane

Every rung's sentence was read against its implementation and narrowed where
the two differed. Measured examples:

```
COST     "worst-case spend" while pricing max_tokens, which the spec defines
         as max OUTPUT tokens. On fetch-a-document-and-summarise: 818k input
         tokens, $2.4563 real, under a green line reading $0.0075. 328×.
         → "worst-case OUTPUT ceiling · prompts unpriced"

TYPES    "every deep reference fits its declared shape" while no builtin can
         declare an output shape.
         → "…builtin output has none"

PERMITS  "the body fits the declared boundary" while judging literal args only.
         → "literal + const: args fit the boundary · computed + symlinks at run"

TOOLS    → "every named nika: tool is canonical · globs + mcp: not checked"
SCHEMA   → "no known-unsatisfiable form · $ref opaque"
GATES    → "no task proven dead" (absence of a proof of death is not proof of life)
SECRETS  → "no declared secret reaches an effect · model echo untracked"
```

### 2.3 · The const lane · `cab7e996b`, and the staged F16/F13

`const:` is the one authority a run cannot move (`--var` satisfies `inputs:`
and nothing else, measured). Reading it as dynamic let an author delete a
grant, keep the effect, and pass a gate the runtime then refused — and made
`--infer-permits` print a draft its own `check` rejected.

### 2.4 · The tooling stopped lying · `aafbc277f`

The edit hook resolved `${NIKA_BIN:-nika}`, so unset it judged with the PATH
build — one release behind the tree, on exactly the class being fixed. Three
agents were working under it simultaneously.

The first repair printed the version. **That inverted the safety ordering**:
the tree's debug build reports `0.106.0` and carries the fix; the brew build
reports `0.106.1` and carries the fail-open. A debug build's version tracks the
last tag, not the tree. The second repair made the PATH the identity and
labelled the tag as a tag. Caught by an agent auditing something else.

### 2.5 · The corpus · spec `07dedd0` · 67 files · 3585 lines

**Yes, the corpus was genuinely rebuilt, not patched.** 46 examples and
templates, against `examples/CONVENTIONS.md` derived from the files that
already worked rather than invented, plus committed fixtures so the files run
for a stranger.

What was actually broken, none of it visible to `check`:

- **6 of 10 templates were RUN-DEAD in a fresh scaffold directory** — the exact
  files `nika new --from` hands a beginner, every one check-green. All 10 now
  run green in an empty dir offline.
- **`agent-loop` taught a false model.** It claimed the done-contract belongs
  in the prompt or a live model finishes in prose and fails. Measured false —
  and the prescribed instruction CAUSES the failure it warns about. Reproduced
  twice.
- **`etl-state`'s diff never diffed.** `nika:read` returns TEXT,
  `nika:json_diff` compares VALUES, so every run emitted one replace-everything
  op — for the template whose entire purpose is "only what changed".
- **A `# SLOT:` line under a block scalar is prompt CONTENT** and was being
  sent to the model verbatim. Four files.

**What was deliberately NOT done**: 25+ refusals to widen a boundary, each with
a recorded reason. One agent planted a private key beside an invoices fixture
to measure what a subtree grant would expose. Another verified at run that a
one-segment `*` correctly refuses a nested path when a topic carries a slash.

**One file stays RED on purpose**: `examples/showcase/t2-release-radar.nika.yaml`
hits `NIKA-SEC-009` by declaring an honest boundary. No blocking prompt was
added to silence it — that would be ceremony on the file people copy.

### 2.6 · The pack re-vendored · `8980176af`

`nika-onboard` 62/62 — red the whole arc, because the gates read the VENDORED
mirror and the corpus rebuild could not reach them until `sync-pack.sh` ran.

### 2.7 · The two laws, in the code · `c81b258e9`

`crates/nika-check/src/lib.rs` module doc (what an author reads before adding a
rung) and `AGENTS.md` hard rules (what a reviewer reads). Ratcheted after four
instances cleared the house stress-to-ratchet threshold.

### 2.8 · The dogfood doctrine · monorepo

The `nika-first-automation` rule (monorepo-internal), §4bis — the reciprocal law. The
atelier runs the product it ships (§0-§4); §4bis says friction writing a
workflow REMOVES upward into the spec and engine. 7 triggers, a 5-step gesture,
4 anti-patterns, and the empirical proof: one workflow, twenty minutes, three
defects found without looking for them.

---

## §3 · RECORDED, NOT IMPLEMENTED · this is the honest half

Substantial research landed on disk and stopped there. **None of the following
is in the engine.** Each is real work someone can pick up; none of it should be
described as done.

### 3.1 · The resource algebra · `2026-07-28-resource-algebra.md` · 1535 lines

- **§1 measurement matrix** — ~57 measured facts about the engine's actual
  cost, scheduling and concurrency behaviour.
- **§2 · 42 theorems.** Including one original result, the **wave penalty
  theorem**: `T_wave ≥ T_flow`, with equality iff a path touches the maximum of
  every positive wave; equality is forced when durations are constant; and the
  ratio is UNBOUNDED with the wave count held tight. Sandwiched by
  `T_∞ ≤ T_wave ≤ T_1`.
- **§3 · the 14-dimension resource vector** with composition rules under `;`
  (sequence) and `‖` (parallel) — duration, hardware, operator attention,
  energy, and the rest beyond money.
- **§0.5 · 17 refuted claims**, leading the document rather than buried. An
  adversarial pass killed half the original 33, including my SQL-optimizer
  analogy (the DAG is the QUERY, not the plan) and my series-parallel framing
  (refuted by a 4-node counterexample).
- **§8 · four operator ratifications with their reasoning**, including one
  where the premise I asked on was later refuted and the entry says so.

**Status**: a research document. The dimensions are not in the spec, the
scheduler is unchanged, nothing is measured at runtime.

### 3.2 · The scheduling work

Established and NOT built: the dataflow scheduler is worth ~2× and is optimal
under `P∞`; **per-provider concurrency is a PREREQUISITE, not a peer** —
measured, a 16-wide flat fan-out fails 12/16 with rate limits while a staged
one passes 16/16. The phantom-edge hint must be gated on it or it tells authors
to delete what keeps them green.

### 3.3 · The catalog

**Shipped**: workflows that measure the catalog (`workflows/catalog-*`),
proving drift between the vendored pin and live upstream.

**Recorded, not shipped**: the generator carrying `limit.output`,
`limit.context`, `open_weights` (100% upstream coverage, uncommitted in
`crates/nika-catalog*`); the finding that **10 of 11 hand-written token limits
in `llm-providers.toml` are WRONG and rendered in VS Code hover** — a haiku
ceiling 8× too low, taught to authors today; a proptest passing vacuously
(schema @1.0 vs @1.1, all 200 cases dying at the gate, comparing two identical
errors).

**Ratified, not built**: measured data stays LOCAL-ONLY first; the arena ships
as a REPRODUCIBLE HARNESS, not a leaderboard (we measure providers we compete
with — "run it yourself" is not contestable, "trust us" is).

**Decided**: no new repo. `nika-registry` already exists, public, and its
canon already specifies the architecture and names `conformance-as-trust` as
the differentiator.

### 3.4 · The oracle

**Shipped**: a check⇔run equivalence oracle over the corpus, with a tiering
that prints what it could not cover.

**Its declared residual, which corrects the framing I gave it**: an equivalence
oracle can only see check and run DISAGREEING. It is structurally blind to the
class where both agree and both are wrong — which is exactly the shipped
fail-open. The oracle and the differential are complements; neither subsumes
the other. My claim that generalising the oracle makes the class unshippable
was too strong.

---

## §4 · Is anything still outdated or badly taught?

Asked directly, and measured rather than asserted.

**Dead forms**: zero occurrences across the corpus of `vars:`, `env:`,
`apiVersion`, `tool: compose`, `nika:compose`.

**The corpus against the engine**: 36 files with zero findings and zero hints ·
6 carrying a hint whose argument is written in the file's own header · 1
deliberately red with its reason and a pointer.

**Known-stale surfaces OUTSIDE the corpus**, each flagged and not fixed:

- `media/raw/*.txt` and `transcripts.json` (the GIF capture sources) still show
  nine sentences the engine no longer prints. The re-shoot is its own session
  and should happen AFTER this lands.
- `.agents/plugins/nika/skills/nika-authoring/SKILL.md:176` quotes a PERMITS
  sentence the binary no longer prints. It narrates a real past measurement, so
  rewriting it would falsify the record; it belongs to the kit mirror re-sync
  (task 7).
- `crates/nika-check/examples/check/render.rs` is a second renderer duplicating
  nine report sentences. Its header says it is scaffolding until step 19 ships.
  Step 19 shipped. It wants deleting or wiring to the real renderer.

---

## §5 · The method, and what it actually produced

Five times, what corrected this session was **someone sent to contradict it**,
not a test.

```
the "purely lexical" characterisation      true of the static rung only
"two independent probes agree"             one probe, read twice
"nine shipped examples are broken"         did not reproduce on the released build
a regression I introduced                  the diagonal moved from ** to *
a build piped behind grep                  the timeout was invisible
my own SLOT scanner finding 7 phantoms     an hour after writing the rule against it
a probe counting 43 reds                   the binary had been deleted mid-run
```

The last three are the same defect: **a probe that returns the right-looking
verdict for the wrong reason.** The rule that catches it — *a probe must be
SHOWN to discriminate before it is trusted* — was written during this arc and
then broken twice by its own author. That is worth recording plainly rather
than tidying away.

---

## §6 · What this says about the product

Not one of the 23 verified defects was an EXECUTION defect. Every one was in
the JUDGMENT layer: the engine did what it said, but said more than it knew.

That layer is the differentiator, so finding bugs there is finding bugs in the
thing that makes Nika worth having. Kubernetes, Deno and Docker have all
shipped this same class of CVE. The difference is not that they lack the bug —
**it is that they have no gate that could find it.**

What changed is the definition of ready. Before: 1.0 is ready when the features
are there. After: **1.0 is ready when the oracle runs over the corpus and finds
no contradiction.**

The unknown is honest and unresolved: **the yield curve.** 23 defects in two
days is either a system shedding known debt (the rate falls) or a class that
regenerates (the rate holds). The instrument to measure it exists; run 2, on a
NEW domain, has not happened. That is the single most valuable thing left.
