# 10 · Authority

> A workflow's authority is **declared in the file and judged before a
> token is spent** — three blocks, three questions, never conflated:
> [`permits:`](./01-envelope.md#permits--optional--the-declared-capability-boundary)
> says what the workflow **may touch** (capability) · the **unconditional
> laws** say in what **order and shape** it may act (this chapter) ·
> [`secrets:` + `egress:`](./01-envelope.md#egress--optional--sanctioned-destinations-declassification)
> say where a sensitive value **may go** (flow). The three compose
> without overlap; none is negotiation material for a model.
>
> One law binds them: **required ⊆ permitted**. What a task's body
> requires (computed statically, never declared by hand) must fit inside
> what the file grants. A violation is a check refusal with a **witness**
> — the task, the effect, and (for flows) the exact taint path.

---

## The effect vocabulary (normative · closed)

An **effect** is a class of interaction with the world outside the run.
The v1 vocabulary is exactly the capability surface of
[01 §permits](./01-envelope.md#permits--optional--the-declared-capability-boundary)
— **five categories** (this table is the one vocabulary the checker, the
inference, the certificate, and the unconditional laws all speak — one voice,
never a parallel list):

| Effect category | Carried by | Granted by |
|---|---|---|
| `fs` (read · write) | `nika:read` · `nika:grep` · `nika:write` · `nika:edit` · file-writing media builtins | `permits.fs.read` / `permits.fs.write` (path globs) |
| `net` | `nika:fetch` · `nika:notify` (webhook) · any URL-reaching builtin | `permits.net.http` (host allowlist · SSRF floor beneath) |
| `exec` | the `exec:` verb | `permits.exec` (`false` · `true` · program allowlist) |
| `tools` | the `invoke:` verb surface | `permits.tools` (id globs) |
| `env` | the names a child process inherits — composed, never inherited (see [01 §permits](./01-envelope.md#permits--optional--the-declared-capability-boundary)) | `permits.env` (exact names · **never inferred**: a subprocess's environment reads are opaque) |

> **Why this table said `four` until 2026-08-11 (D-2026-08-11-N22).** `permits.env`
> has shipped in the envelope since the block existed, `NIKA-AUTH-007` counts its
> entries among the bounds (NEP-0005), and [02 §exec](./02-verbs.md) reaches a
> child's environment *only through a declared `permits.env:` name*. The table
> that calls itself **one voice, never a parallel list** did not know the key —
> so the spec carried exactly the parallel list it forbids. Admitting `env` is
> **not adding a category, it is documenting one that ships**: a category with no
> key would change the language; a key with no category made four downstream
> surfaces (inference · certificate · the `<effect-class>` vocabulary · `NIKA-SEC-004`)
> unable to name it.

Two derived facts the engine computes and no author writes:

- **A task's required effects** — from its verb and its arguments (the
  builtin classification table is normative for the stdlib: each
  catalog entry declares which category it carries and which argument
  names its target). `infer:` with no tools is **pure compute**: zero
  required effects (provider egress is governed by the secrets flow
  rules, not by a capability category).
- **The workflow's needed boundary** — the union of its tasks' required
  effects, as a `permits:` block (`nika check --infer-permits` prints
  it; the round-trip law holds: the inferred block re-checks clean).

**Required ⊆ permitted is judged twice** (per 01 §permits): statically
at `nika check` (an escape = refusal before any token) and at run time
(`NIKA-SEC-004` — the dynamic cases a static check cannot see).

## The unconditional laws · *normative · the `policy:` block is dead*

`permits:` bounds capability; nothing bounded **order and shape** — «
no shell after an untrusted fetch », « a human signs before anything
irreversible ». A `policy:` block used to name those laws next to the
workflow. **It is dead (2026-08-12), and the reason is the shape of the
enforcement, not the shape of the syntax.**

### Why a declared law was not a law

Measured on the corpus before the ruling ·

- **Zero usage in real work.** The 12 `policy:` blocks in the corpus were
  all exactly `policy:` + `endorsement: solo`. Six of the seven families
  had fan-out **zero**.
- **The soft families were never recorded.** `prefer:` and `optimize:`
  claimed to be « parsed and recorded »; `check --json` came back
  **byte-identical at the sha256** with opposite values in them. Nothing
  read them.
- **`limits.max_tasks` asserted a bound on a literal in the same file.**
  A number checking a number it can see.
- **And the whole block was FAIL-OPEN.** A human gate with **no**
  `policy:` block passed. The **same** gate carrying an unrelated clause
  refused. Corpus tally: **26 gates spared, 8 punished** — and the 8 were
  the ones that had declared something else entirely. A law that binds
  only the file that opts into it binds nothing.

The two codes that served the block are **retired**, and the whole
`NIKA-POLICY` namespace with them. No retired code is ever reused.

### What survived, and where it went

Two of the families read the **transitive graph** — they say something a
reader cannot see by looking at one task — so they did not die with the
key. They moved to the shelf where the lethal-trifecta law and the
affirmative-consent law already live: **laws that fire with no
declaration at all.**

**`forbid.exec_after` became the order law, unconditional.** No `exec:`
task may sit transitively downstream of a net-effecting task
(`nika:fetch` · `nika:notify`) over the derived graph (`with:` data edges
∪ `after:` control edges). Content the workflow did not author must not
reach a shell. Fixed at `[net]`, the one parameterization it was ever
declared with. The diagnostic names the **path**, which is the witness
(`fetch_page → act`). Its code is in [05](./05-errors.md).

> **The trifecta does NOT subsume it, and this was measured.** The
> trifecta needs leg ① — a non-empty `permits.fs.read`. A file that
> fetches and then shells, with no private read declared, reads **GREEN**
> under the trifecta and **RED** under this law. That file is the
> conformance fixture `core/order/001-net-before-exec-violation`.
>
> **Cost, measured BEFORE the ruling** · 194 `exec:` tasks across the
> shipped corpus, **1** refused — and that one
> (`conformance/envelope/secrets-two-sinks-one-sanctioned`) is already a
> declared `check-reject`. Zero green files paid for it.

**`require.human_gate_before` did NOT make the same move, and the residue
is owed rather than guessed.** Its unconditional substance — *a human
gate must dominate every route by which untrusted content reaches an
egress* — is what the trifecta law and the affirmative-consent law
already carry, with no declaration. What is **not** covered is the
narrower claim *a human gate before `exec:`, absent any private read*.
Turning that into an unconditional law at its one declared
parameterization `[exec]` would refuse **187 of the 194** `exec:` tasks in
the corpus: that is not a law, it is a ban on `exec:`. The
parameterization a real unconditional gate rule would need has **no
empirical signal** — no corpus file ever declared the family — so it is
named here as **OWED**, and left undecided rather than invented.

**`allow.providers` and `limits.max_tasks` are simply gone.** Neither
reads the graph; both were a declared preference the file could have
enforced by being written differently. A provider allowlist is a
deployment concern, not a property of the workflow text.

### The two judges that remain

`permits:` and the unconditional laws are orthogonal judges over one
body: permits answers *may this task touch X at all*, the unconditional
laws answer *may it do so here, in this order, unattended*. A body must
satisfy **both**, and **neither is opt-in**.

**The approval is a bounded ticket (normative · NEP-0013)** · the
consent a pause collects is not a bare answer: it is a **ticket**
· (1) **content-bound** — the ticket binds the hash of the canonical
rendering of what is shown (the message · the gated action's identity ·
the effect classes in play · never an LLM summary); an answer whose
resolved content hash differs halts (`approval.content_mismatch` at the
receipt) · (2) **scoped and TTL'd** — this run × this step × this
content hash, with a bounded TTL; an expired ticket re-prompts, a
cross-run replay is refused · (3) **rate-limited** — at most N=5
approvals per run; identical prompts dedup (the same content hash rides
one ticket, attested `dedup`); a heterogeneous batch is refused at
check (one prompt gates one action of one class); the N+1th prompt is a
typed HALT (`security_error`), never a queue · (4) **attested** — every
decision emits a hash-chained `approval_decided` event (ticket digest ·
shown hash · decision · remaining TTL · scope), and the digests rise to
the receipt · (5) **revocable before execution only** — never after:
the receipt shows what executed under which authority.

## The affirmative-consent law · *normative · NEP-0020*

A REFUSED confirm-mode `nika:prompt` settles the task **success with
value `false`** — the Deny lives in the approval attestation
(NEP-0013), never in the task status. So a gate that nothing consumes
is a rubber stamp: a bare `after: { ask: success }` edge admits the
refusal, a `when:` that never reads the answer lets it through, and a
`when:` provably true on `false`
(`with.go == true || with.go == false`) blocks nothing. The law: for
every confirm-mode gate (`mode:` absent or the literal `confirm`) and
every egress-capable task (`exec:` · a net or fs-**write** builtin ·
`mcp:*` · an `agent:` whose whitelist admits an egress tool), judged on
the derived graph, **every route from the gate to the task must be
closed** — by an affirmative gate, by `when: false`, or by a closer
confirm gate (the nearest gate owns its closure). **`false` triggers
exactly zero effects.**

**The affirmative gate (normative · the decidable fragment).** A `when:`
is affirmative when it evaluates to FALSE under the **refusal
substitution**: `tasks.<gate>.output` → `false` · `tasks.<gate>.status`
→ `"success"` (a refusal settles success — a status read is NOT
consent, and it is decidable exactly), the exact single-island `with:`
binding carrying its target's value, Kleene-3 over the fragment
(boolean literals · `==`/`!=`/`in` on resolved literals · `!`/`&&`/`||`
/ternary). Three verdicts, three fates —

- **FALSE** — the route is closed (the human-gated-ship pattern:
  `with: { go: "${{ tasks.ask.output }}" }` + `when: ${{ with.go == true }}`).
- **TRUE** — the gate is PROVEN open under the refusal; the route stands.
- **Unknown** — the gate cannot be decided statically (a nested template
  binding · an expression beyond the fragment): the defect is unproven,
  and an unproven defect is the advisory hint's ground, **never** a
  refusal (sound — no false red).

A violation is **`NIKA-SEC-014`** (`security_error` · check-time ·
before any token) — the diagnostic names the gate AND the sink and
teaches the affirmative pattern. The law binds with **no declaration at
all** — it always did, which is why it survived the death of the declared
block untouched — and `mode: choice` is out of scope — its
answer is a string and the lane claims nothing there (silence, never
wrong).

## Secret flow refusals carry their codes (normative)

The flow rules themselves live in
[01 §egress](./01-envelope.md#egress--optional--sanctioned-destinations-declassification)
and are unchanged. This chapter names their wire codes — the two
refusal classes were report-only until W4:

| Code | Class | Witness (in the diagnostic) |
|---|---|---|
| `NIKA-SEC-006` | a `secrets.<name>` value reaches an **unsanctioned sink** (an `exec:` argument · an `invoke:` payload · an `infer:`/`agent:` prompt) | the **taint path**, source-first (`secrets.api_key → with.tok → tasks.call.output → exec`) + the exact `egress: [{ to: "<sink>" }]` clause that would sanction it |
| `NIKA-SEC-007` | a tainted value reaches the **workflow boundary** (`outputs:` — where a result leaves the run) | the taint path into the `outputs:` entry |

Both are `security_error`, check-time, blocking. The taint path is not
decoration: it is the **minimal witness** that lets an author decide
sanction-or-fix without re-deriving the flow by eye. The secret's
*value* never appears in any diagnostic.

## The permit-parameterization taint (normative)

The fit above proves `Required ⊆ Declared` on **categories**; it says
nothing about the resolved VALUES flowing under a present block. Every
value carries an integrity label — **Integ ∈ {trusted, untrusted}** —
orthogonal to Conf (the secrets axis): literals, `const.*`, and
`secrets.*` are trusted; `inputs.*` (caller-supplied at launch),
`config.*` (untrusted by DECLARATION, not by provenance — see below),
fetch/tool results,
and anything derived from them are untrusted, with monotone propagation
(one untrusted operand taints the whole interpolation). Two rules bind
untrusted values under a `permits:` block (NEP-0004 · LAW-AUTH-0325):

| Code | Class | Witness (in the diagnostic) |
|---|---|---|
| `NIKA-AUTH-007` | an interpolation reaches a **permit bound** (a host, glob, program, or env-name literal inside `permits:` · NEP-0005 counts `env:` entries among the bounds) | the bound's own path (`net.http[0]`) — a bound MUST be a literal: the boundary would be self-serve, there is nothing left to canonicalize against |
| `NIKA-AUTH-008` | an untrusted value reaches a permitted verb's **argument** and its canonical resolved form escapes the step's permit | the **taint path**, source-first (`inputs.p → args.path`) + the resolved value, its canonical form, and the bound it escaped |

Both are `security_error`, check-time, blocking. The **re-gate** never
matches raw strings: the engine canonicalizes the RESOLVED value first,
per plane — fs paths are lexically normalized (`.`/`..` resolved,
separators collapsed) before the glob match, net hosts are lowercased
(IDNA→punycode, trailing dot and default port stripped), and for exec
argv the program is argv[0] while re-entry-class tokens (`--exec`, `-c`,
`eval`…) are never covered unless the permit lists them explicitly.
`datasets/../datasets/q3.csv` is INSIDE `datasets/**`;
`../../etc/passwd` is not — a prefix matcher cannot tell them apart,
the canonical form can.

An untrusted value not resolvable at check (no default) **defers**: the
file stays valid, the run-time re-gate is mandatory (an escape fails the
task `NIKA-SEC-004` — the defense-in-depth twin of NEP-0003 law 3), and
the check report SHOULD list the deferred re-gates informationally so CI
sees the attack surface before launch.

The only door is the authored one — a `lift:` entry naming the **taint**
law (see [§the authored doors](#the-authored-doors-normative) for the
construct):

```yaml
tasks:
  load:
    invoke:
      tool: nika:read
      args: { path: "${{ inputs.p }}" }
    lift:
      - law: taint
        from: inputs.p            # one binding
        because: "vendor inventory path, deployment-controlled, reviewed at release time"
```

A `law: taint` entry MUST name `from:` (one binding) and a non-empty
`because:`; the receipt records it (taint path · because · value
digest). It lifts the TAINT law only — the value is then matched like a
literal and must still sit inside the declared boundary. It is never a
permit bypass, and there is no implicit declassification in v1.

## The data-as-code sink (normative · NEP-0006 · LAW-AUTH-0327)

The contract distinguishes an INERT read from a CODE-BEARING read.
Some artifact classes execute at load: the serialized-executable
family (the deserializer runs code · `.pkl` `.pickle` `.dill`
`.joblib` `.pt` `.pth` `.ckpt`), scripts and notebooks (`.py` `.sh`
`.bash` `.zsh` `.ps1` `.bat` `.cmd` `.rb` `.pl` `.php` `.js` `.mjs`
`.ipynb`), and executable binaries/modules (`.exe` `.dll` `.so`
`.dylib` `.wasm` `.jar`). The three classes are CLOSED and normative
(only a NEP amends them · the deliberate exclusions and their reasons
live in NEP-0006). A `nika:fetch` whose RESOLVED URL path names one ·
matched case-insensitively on the path's final extension, the query
carries no verdict · is refused at check (`NIKA-SEC-008` ·
security_error · the diagnostic names the class and both repairs) when
the URL is literal or resolvable through the taint rules above; the
unresolvable DEFERS to the run-time twin (`NIKA-SEC-004` · the same
class refused on the resolved URL · defense in depth). The honest door
is a `lift:` entry naming the **data-as-code** law (non-empty `because:`
· greppable): it lifts THIS law only · never the `net.http` boundary,
never the SSRF floor, never the taint re-gate. The repair in the other
direction is to model the acquisition as the `exec` it feeds, under a
program permit review can see.

```yaml
    lift:
      - law: data-as-code
        because: "vendor checkpoint archived to cold storage, never loaded"
```

## The authored doors (normative)

`lift:` is the **single** construct through which an author opens a
named law. There is no second spelling, and there will not be one when a
third law earns a door.

```yaml
lift:
  - law: taint                    # REQUIRES from:
    from: inputs.p
    because: "<why this is safe, in words a reviewer can judge>"
  - law: data-as-code             # from: is FORBIDDEN here
    because: "<why this artifact is never loaded>"
```

The contract, for every entry ·

1. **`law:` is a closed enum.** v1 knows `taint` and `data-as-code`. A
   law that has no door cannot be lifted at all — that is the default,
   and it is the common case: **24 error-bearing laws exist; 2 have a
   door.**
2. **`because:` is mandatory and non-empty.** A lift with no reason is a
   parse error, not a warning. The reason is what review reads.
3. **A lift moves exactly ONE law.** It never widens `permits:`, never
   touches the `net.http` boundary, never lowers the SSRF floor.
4. **Every lift is recorded** in the run receipt and projected in the
   check certificate. A lift that review cannot see would defeat its own
   purpose.
5. **`from:` is law-specific** — required by `taint`, forbidden
   elsewhere. The schema enforces the discrimination; a `from:` on the
   wrong law is a parse error.
6. **A lift that lifts nothing is an ERROR** (`NIKA-AUTH-011` ·
   `validation_error`), never a silent no-op. If the named law would not
   have fired on this task, the entry is refused and the diagnostic says
   so.

> **Rule 6 is borrowed from a regression, and the regression is the
> argument.** Terraform's `nonsensitive()` is the same shape: an authored
> trapdoor that lowers one label, with the responsibility transfer stated
> in its own docs (*« you are declaring to Terraform that you have done
> all that is necessary … that's a bug in your module and not a bug in
> Terraform itself »*). In `v1.5.0` a redundant call was an **error**
> — `if args[0].IsKnown() && !args[0].HasMark(marks.Sensitive) { return …
> "this call is redundant" }`. On `main` that guard is **gone**: the mark
> is deleted unconditionally, so a redundant declassification is a silent
> no-op. Their own documentation page has not caught up — its prose says
> no-op, its example still shows the error.
>
> **The cost is not the wasted line. It is that the trapdoor stops being
> greppable.** A reviewer auditing « every declassification in this repo
> carries its weight » can do that under the strict rule and cannot under
> the lax one, because dead lifts accumulate indistinguishably from live
> ones. A trapdoor whose whole value is being **countable and reviewable**
> must refuse to be written when it does nothing.

**Why one construct and not one field per law.** A door per law grows
the language linearly in laws, and each language feature an author uses
carries a measured **+18.9% odds of workflow failure** (regressed over
13,915 workflows · see [08 §antivalues](./08-out-of-scope.md) for the
population and its limits). The provider is
a parameter of `infer:`, not a verb; the law is a parameter of `lift:`,
not a field. The predecessors — a task-level `declassify:` list and a
task-level `inert: "<because>"` string — are **dead**: same job, two
spellings, and the third law would have made three.

## The certificate names its effects (normative)

`nika check --json` already emits a resource certificate
([05](./05-errors.md) · attempts · llm calls · effect calls · spend).
W4 adds the **authority projection** — `certificate.effects`:

```json
{
  "effects": {
    "boundary_declared": true,
    "needed": { "fs": { "read": ["./data/**"] }, "exec": ["git"] },
    "escapes": 0
  }
}
```

`needed` is the inferred boundary (the same object `--infer-permits`
prints) · `boundary_declared` says whether the file carries a
`permits:` block · `escapes` is the count of required-outside-permitted
violations (0 in any clean report — the field exists so a certificate
consumer never has to re-derive it). The certificate stays a
## Errors (the order law · new in this chapter)

The order law's code, its category and its meaning live in the one error
catalogue ([05-errors](./05-errors.md)) — this chapter names the law, never
its wire spelling.

The secret-flow and permit-taint codes join the existing `NIKA-SEC` and
`NIKA-AUTH` namespaces, as does the affirmative-consent law. The
`NIKA-POLICY` namespace is **retired** with the block it served.

## One obvious way (normative for linters)

- A capability boundary is spelled `permits:` — nothing else re-spells a
  permits grant.
- A human gate is `invoke: { tool: "nika:prompt" }` — the pause IS the
  consent mechanism; a `when:` on an input flag is not a gate.
- A law that reads the graph is UNCONDITIONAL — there is no block to
  declare it in, and none to disable it from. A rule an author can opt
  out of by saying nothing is not a rule (measured: 26 gates spared, 8
  punished, and the 8 were the ones that had declared something else).

## What `permits:` is, formally — *and what it is not*

`permits:` is **not a capability system**, and this chapter says so
plainly rather than borrowing a word it has not earned.

In the capability model as Dennis and Van Horn defined it (CACM 9(3),
1966), a reference is a pair `[i, a]` where `i` is an index **into the
holder's own C-list**, and each C-list entry *"locates by means of a
pointer some computing object"*. A computation cannot name an object it
was not handed. **Authority comes from the indirection.**

`permits.fs.write: ["./out/**"]` is a **name**, resolved at use against
an ambient filesystem the engine process can already reach in full. Two
paths therefore exist through the system: the designation, which travels
in `args`, and the authority, which travels in `permits`. They are
recombined at the moment of access by a reference monitor. Miller, Yee
and Shapiro (SRL2003-02, 2003) call the absence of that split *Property
A · No Designation Without Authority*, and state that **no ACL system
can have it**. `permits:` does not have it.

The shape is closer than the label suggests — `permits:` is per-subject
and lists objects, which is a C-list's shape rather than an access
list's. But a C-list of **strings** is not a C-list. The accurate name
is **a per-subject permission manifest over a shared namespace**: the
family of Android manifests and Deno flags, not the family of seL4
capabilities.

The known cost is stated with it. A manifest is declared once, before
the work exists, so it must be written wide enough for every call the
file could ever make — *"you often do not know in advance what
authorities the program actually needs"* (Miller, 2006, §3.1). That is
authority granted **just-in-case**, where the principle of least
authority asks for **just-in-time**.

What v1 buys instead is the thing none of those systems attempts: **the
bound is judged before the program runs.** A capability is checked when
it is invoked — which is to say after the tokens are spent and the
request is already out. `nika check` reads the whole declared boundary
against the whole graph and refuses ahead of the first effect, and
`--infer-permits` lets the machine derive the boundary so the author is
not the one who has to be exhaustive. That is a different guarantee, not
a weaker version of the same one, and it is the one this file is built
to give.

> **Citation note.** The law usually quoted as *"No Ambient Authority"*
> is Property D of SRL2003-02, **not** of Miller's 2006 dissertation:
> the word `ambient` does not occur in the dissertation text.

## What v1 deliberately does not do

- **No solver, and no soft constraints at all.** `prefer:`/`optimize:`
  died with the block that held them: measured, nothing read them (the
  `check --json` sha256 was identical with opposite values). A
  constraint that cannot be judged must never look judged, and the
  honest way to not judge one is to not accept it.
- **No runtime law.** Every v1 rule is decidable at `nika check` on
  the graph; rules whose truth needs runtime data are out of scope,
  deliberately (the check stays the pre-token audit).
- **No new verbs, no `approve:`.** The human gate rides the existing
  `nika:prompt` pause; a dedicated approval surface (delegation ·
  quorum · `via:`) is future work with its own chapter.
- **No law inheritance.** A file's law is the file's — composition
  across `invoke: workflow:` calls judges the callee by the same
  unconditional laws (the ceiling algebra is reserved, unwired).
- **No memory category.** Recall is a **tool**, reached through `invoke:`
  like every other callable ([08 §H9](./08-out-of-scope.md) · today
  `mcp:memory-server/recall`, tomorrow the reserved `nika:connectome/*`
  group — same verb, zero workflow-shape change). It is therefore
  governed by `permits.tools`, and **never by an ambient implicit
  memory**: a recall a reader cannot see in the file does not exist.
  Whether a store-scoped grant deserves its own effect category — the
  `net` precedent, where `nika:fetch` is a tool and yet hosts need
  bounding — is an open question for a future proposal, not a v1 gap.

## Related

- [01 · Envelope](./01-envelope.md) — `permits:` (capability) ·
  `secrets:`/`egress:` (flow) — the two blocks this chapter composes with
- [03 · DAG](./03-dag.md) — the derived graph every order rule reads
- [05 · Errors](./05-errors.md) — the error registry (POLICY + SEC rows)
- [09 · Types](./09-types.md) — `returns:`/`types:` (the value contract;
  orthogonal to authority)
