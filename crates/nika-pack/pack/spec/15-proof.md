# 15 · Proof

> Everything before this chapter made a workflow *checkable*. This
> chapter makes a run *provable and portable*: a **canonical semantic
> identity** (the hash of what the workflow MEANS, not how it is
> spelled), a **single lock** that pins every dependency by digest,
> **assertions** the author writes and the engine judges at an honest
> level, and **one receipt** into which the certificate, the trace
> verdict, the assertions, and the lock digest all fold — so the
> Decision Receipt (11) and the registry certificate become
> *instances* of one shape, never three.
>
> This is the last pre-1.0 chapter. It closes the loop the language
> opened: a `nika:` file in, a signed honest proof out.

---

## The semantic hash (normative · G13)

A workflow's identity is the hash of its **desugared, versioned
Semantic IR** — never the source YAML text, never a reordered map:

```
H_semantic = H( domain ‖ format_version ‖ JCS(SemanticIR) )
```

- **The Semantic IR** is the source after: parsing finished · sugar
  lowered · normative defaults expanded · types normalized (chapter 09
  canonical forms) · graphs made explicit (the derived edges of 03) ·
  references resolved · units normalized (durations → ns, sizes →
  bytes) · Unicode NFC · keys sorted · spans and comments removed. Two
  files that MEAN the same workflow lower to the **same** IR; two files
  that mean different workflows do not.
- **JCS** is RFC 8785 (JSON Canonicalization Scheme) — the one byte
  encoding. It fixes number handling explicitly (JCS alone serializes
  numbers as ES6 doubles, which would collapse distinct integers; the
  IR carries integers as strings where that matters, the chapter-09
  and chapter-05 precedent).
- **Domain separation is strict.** Every hash names its domain so a
  value can never be reinterpreted across roles:
  `source · canonical · semantic · plan · trace · artifact · receipt`.
  A trace-domain hash never collides usefully with a semantic-domain
  hash even over identical bytes.
- **Merkle by task** — each task's semantic subtree hashes
  independently (seed: the ResumeKey's JCS+blake3 definition hash,
  generalized); the workflow hash commits to the task hashes, and a
  composed child (14) folds in as a subtree. A proof of the whole
  contains a proof of each part.
- **`canonical()` is idempotent**: `canonical(canonical(x)) ==
  canonical(x)` — property-pinned.
- **The correct property** (never overstated): semantically-different
  programs produce different canonical encodings. Collision resistance
  of the underlying hash is a **cryptographic assumption**, stated as
  such — not a promise this spec makes.

Cache and resume are **re-keyed semantically**: a result is reused iff
the semantic identity matches — the same law the composition cache
(14 §law 10) was waiting on.

**The versioned rate table is part of the run's semantic pin**
(normative) · a cost bound means nothing without the table it was
computed against, so the table's version rides the pin the way the lock
digest does. A replay reads cost against the **pinned** table and
yields the identical budget verdict. A run whose pin lacks the table,
or names a table version this engine does not know, **refuses the
cost-meaning replay** — it never silently re-prices against today's
numbers, which would rewrite yesterday's verdict and call it
verification. The replay of EFFECTS is untouched: it is the budget
claim, and only that, which needs the table.

## `nika.lock` · *the single lock (normative · F7)*

One lockfile pins everything a run resolves, by **digest**:

```yaml
# nika.lock · generated · never hand-edited
lock_format: 1
providers:                      # every model pinned by content digest
  "anthropic/claude-…": { digest: "blake3:…" }
tools:                          # builtin + MCP surface versions
registry:                       # every registry: ref pinned owner/name@version + digest
```

> The `policy:` row died with the envelope block it materialized
> (2026-08-12): the author surface and its lock materialization die
> together — a lock field pinning « the resolved policy decisions » of a
> block that cannot be written is a column with no source. The
> unconditional laws ([10](./10-authority.md)) need no pin: nothing
> declares them, so nothing about them can drift between runs.

- **Pin by default**: a run resolves ONLY what the lock pins; an
  unpinned dependency is a refusal (`NIKA-LOCK-001`). Nothing floats.
- The lock is generated (`nika lock`), never authored — hand-editing a
  digest is a lie the check catches (the lock's own hash covers it).
- It unifies the prior manifest lock + the pin-by-default rule into one
  file — the local boundary of the same supply chain the gateway (12)
  and the distribution work extend.

## `assert:` · ⚰️ *NOT in the envelope of v0.1 (removed 2026-08-11)*

> **The envelope key is gone. `nika:assert`, the single-condition fail-fast
> builtin, stays and is untouched** — that half works and is what an author
> actually reaches for today.
>
> **Three measurements forced the subtraction, and any one of them would have.**
> ① The published JSON Schema carries **9 envelope keys and `assert` is not
> among them** — so a feature the engine accepted *failed the validator the
> project ships*, and since [07 §unknown key](./07-conformance.md) made refusal
> strict, that contradiction became fatal rather than untidy. ② It judged
> **nothing**: an assertion naming an ordering over two tasks that do not exist
> was accepted with a `clean · risk low` verdict, where the same mistake one
> field away (`after:`) is `NIKA-DAG-002`. ③ Across **661 workflow files**,
> exactly **one** carried the block — and that file is the probe written to
> demonstrate this very defect. *The only user of the field was the witness of
> its flaw.*
>
> **Why removal rather than repair.** A vocabulary of obligations that accepts
> anything and checks nothing is worse than its absence: absence sends an
> author looking for a real mechanism, silence tells them a guarantee is held.
> Making it merely *refuse* would have kept an envelope key that expresses
> nothing an engine can act on. **The subtraction is the fix, and it costs one
> reserved slot back.**
>
> **It returns as an ADDITION, not as a rescue.** The vocabulary and the three
> honesty levels below are kept in this chapter as the design of record. The
> day `nika trace verify` can judge a property, that property comes back —
> one at a time, each moving from *absent* to *judged*, never from *silent* to
> *judged*. A closed language always permits an addition; that is the whole
> point of closing it.

The design of record — a **closed vocabulary of properties**, each judged at an
honest level — is preserved below and is **not implementable surface today**:

```yaml
assert:
  - no_secret_egress                       # no secret reaches an unsanctioned sink (10)
  - eventually: { task: deploy, state: success }
  - before: { first: gate, second: deploy }
  - bounded: { task: crawl, max_iterations: 100 }
  - resource: { cost_usd: { max: 5.00 } }
```

| Property | What it claims |
|---|---|
| `no_secret_egress` | the flow laws of [10](./10-authority.md) hold across the whole run |
| `eventually{task,state}` | the named task reaches the named terminal state (the Outcome of [13](./13-outcomes.md)) |
| `before{first,second}` | an ordering law on the derived graph (03) |
| `bounded{task,max_iterations}` | a `for_each`/agent loop stays under its cap |
| `resource{cost_usd:{max}}` | the symbolic certificate's cost bound (05) holds |

**The three levels (claim ≤ evidence · normative)**:

- **`StaticProof`** — decidable at `nika check` on the graph/IR (an
  ordering law, a static bound). The strongest, and only claimable when
  the check genuinely decides it.
- **`TraceVerified`** — decided by `nika trace verify` against a
  completed run's trace (13 · the Outcome IR). What only the trace can
  see is judged there, never optimistically promoted to `StaticProof`.
- **`Unknown`** — honestly unresolved (a property no static check and
  no available trace settles). Never dressed up.

`nika trace verify` learns to judge assertions: it reports each
assertion with its achieved level, and a `StaticProof` claim that the
IR cannot actually decide is itself a refusal (`NIKA-ASSERT-001` — an
assertion mis-leveled). Bounded/statistical assertions stay LAB
(calibrated research · never presented as a guarantee).

### An assertion the engine cannot judge is REFUSED (normative · D-2026-08-11-N34)

⚰️ **Superseded the same day by the removal above.** This clause hardened an
envelope key into refusing what it could not judge; the key is now absent, so
there is nothing to harden. The clause is kept because its *reason* survives
the key and generalises — it is the law any future property must satisfy
before it re-enters ·

An engine MUST refuse (`NIKA-ASSERT-001`) any property it does not judge. It
MUST NOT parse it, accept it, and stay silent.

**This is written because the opposite was measured on a shipped engine,
2026-08-11.** An `assert:` naming a `before:` ordering over **two tasks that do
not exist** was accepted and the audit reported `clean · risk low` — no lane,
no finding, nothing. Compare `after:` pointing at an undeclared task, which is
`NIKA-DAG-002`. The same mistake, one field apart, once fatal and once
invisible.

That silence contradicts [10 · Authority](./10-authority.md) in its own words —
*a constraint that cannot be judged must never look judged* — and this block
is where an author states an **obligation**, the one place in the language
where a human writes what the file is supposed to guarantee. A vocabulary of
obligations that accepts anything and checks nothing is worse than its
absence: absence teaches the author to look elsewhere, silence teaches them
that a guarantee is held.

**The properties land one at a time**, each moving from *refused* to *judged*,
never from *silent* to *judged*. An author is never surprised in the direction
that matters: a file that checked clean does not later reveal it was carrying
an unjudged claim.

⚠️ **The cost was measured before this was written.** Across 661 workflow
files in the shipped and internal corpora, exactly **one** carries an
`assert:` block — and that file is the probe written to demonstrate this very
defect. **The only user of the field is the witness of the flaw it has.** It
uses `before:`, the most decidable of the five, so starting there breaks
nothing real.

## `receipt_format: 1` · *the one receipt (normative)*

A run's receipt folds four things into one shape:

```
receipt = (
  certificate     # the check certificate (05 · attempts · effects · cost bound)
  trace_verdict   # the trace-verify result (13 Outcome + chain integrity)
  assertions      # each assert: judged with its level
  lock_digest     # the nika.lock digest this run resolved under
)
```

- The [Decision Receipt](./11-decision.md) and the registry certificate
  become **instances** of this shape — one voice, three surfaces.
- Receipts come in a PUBLIC and a PRIVATE form linked by digests (the
  [11 §receipt](./11-decision.md) discipline): a proof can be shown
  without exposing sensitive evidence.
- The receipt is domain-separated (`receipt` domain) and Merkle-linked
  to the semantic hash it proves: given a receipt you can verify it
  proves *this* workflow and no other.

**Every receipt field carries a readable projection** (normative) · the
schema itself holds, per field, the stable human text that renders it;
a new field WITHOUT one refuses the schema. The ratchet is the point —
a proof surface grows only in forms a human can read, so the readable
half can never fall behind the machine half.

**And the projection is never the evidence.** Reading a receipt and
verifying one are two acts at two trust levels: the rendered text is a
convenience, the verification is the proof. A surface that lets the
first pass for the second has quietly made prose authoritative.

## The verifier is a fortress (normative · reading untrusted proof)

The trace, the receipt and the seal exist to be **verified** — which
makes the verifier the one component guaranteed to parse bytes an
attacker chose. Everything it reads is untrusted input, including
artifacts this engine produced a moment ago: an attacker who can write
the file is the threat, not the producer.

**The decode bounds are spec constants** — not engine folklore, so a
second implementation inherits them instead of re-deriving them. Each
one names **the surface it guards**: the two reading surfaces are the
JOURNAL (a chained NDJSON walk) and the ARTIFACT (a receipt · a sidecar
· an anchor token — one JSON document decoded whole), and a bound that
guards one does not guard the other ·

| Bound | Value | Surface | What it stops |
|---|---|---|---|
| line length | 1 MiB | journal, per line | one line beyond this is refused before the JSON parser sees it |
| file size | 256 MiB | journal, whole | past this it is not a run this engine produced |
| artifact size | 1 MiB | artifact | a receipt is kilobytes; the rest is a denial of service |
| JSON nesting depth | 32 | artifact | unbounded recursion at decode |
| proof-node count | 64 | artifact | a proof flood |
| identifier length | 256 bytes | artifact | an identifier used to overflow a render |

A THIRD reading surface has its own bounds and is not governed here:
the workflow file an author writes, capped by the YAML profile
([01 §YAML profile](./01-envelope.md) · depth and size caps · the
`NIKA-YAML` namespace). Three readers, three bound sets — the authoring
parser, the journal walk, the artifact decoder — and none of them
inherits another's. A bound quoted without its reader is a bound
enforced somewhere it does not belong.

The split is not an accident of implementation, it is the shape of the
two readers judged here. A journal is walked **line by line** and each line is
bounded before it is parsed, so a hostile line costs one bound-check and
nothing else; the structural bounds (depth · proof nodes · identifier
length) belong to the reader that decodes a document **whole**, where
recursion and fan-out are the attack. Stating a bound without its
surface invites a second implementation to enforce it in the wrong
place — which is both a false refusal and a real hole.

The law (MUST) ·

1. **Refusal is total.** An artifact past any bound is refused with a
   **typed** class — `Oversized` · `TooDeep` · `Malformed` ·
   `ProofFlood` · `IdOverflow` — never truncated, never partially
   decoded, never repaired-and-continued. The order is fixed (size →
   depth scan → parse → structural scan) so two implementations refuse
   the *same* artifact for the *same* reason.
2. **Bounds are code, on every build profile.** A bound carried by a
   debug assertion is absent in the build that meets the attacker.
3. **Recognize, never sanitize.** A malicious artifact is classified
   and refused. A verifier that repairs hostile input into
   acceptability becomes the vulnerability it was meant to catch — the
   shotgun-parser diagnosis. Refusal classes are proven against a
   golden corpus that lives with the implementation and runs on every
   change; a crash, a hang or an overflow on that corpus is a defect of
   the highest class, by definition.
4. **The terminal is escaped, the machine surface is not.** Every
   artifact-derived string rendered to a TTY has its C0, C1, DEL and
   OSC/CSI sequences escaped: a title injection, a clipboard write or
   hidden text is an attack that lands on the human, not on the
   pipeline. `--json` and non-TTY output stay **byte-exact** — escaping
   them would break the exactness the proofs depend on.
5. **Two decoders, one verdict.** The reference decoder and an engine
   decoder MUST render the same verdict over the same artifact. A
   divergence is a defect of this specification until proven otherwise
   — the discipline `nika.lock` already lives by, applied to the
   reading side.

Within bounds, nothing changes: an artifact that verified before
verifies identically. Beyond them, behavior that was never a promise
becomes a **named** refusal.

## Distribution (normative note · the local boundary · G35)

A distributable pack ships as an OCI content-addressed artifact with
Sigstore signatures, in-toto/SLSA attestations, an SBOM, an admission
policy, and transparency — one voice with the gateway's backend
attestations ([12](./12-gateway.md)). **`nika.lock` and the local
receipt are the LOCAL ends of that same chain**; the OCI/signature
machinery is the distribution window's own work. Signing proves origin,
never safety ([12](./12-gateway.md) separation laws).

## Errors (the `NIKA-LOCK` / `NIKA-ASSERT` namespaces · new here)

| Code | Category | Meaning |
|---|---|---|
| `NIKA-LOCK-001` | `validation_error` | a dependency resolved that the lock does not pin, or a hand-edited lock digest does not match (pin-by-default · the lock's own hash catches the edit) |
| `NIKA-ASSERT-001` | `validation_error` | an `assert:` claims a level the evidence does not support (a `StaticProof` the IR cannot decide · a mis-leveled obligation) |

## One obvious way (normative for linters)

- A cache/resume key is the semantic hash — never the file path, never
  the source bytes (14 §law 10, generalized here).
- A dependency is pinned in `nika.lock` — a floating version in a
  workflow is a refusal, not a warning.
- An assertion states the level it earns — `StaticProof` is a claim
  the check must be able to keep.

## What v1 deliberately does not do

- **No OCI/signature machinery in the language.** Distribution is a
  dedicated window; the spec pins the local boundary (lock + receipt).
- **No statistical/bounded assertion levels shipped.** They stay LAB —
  a calibrated confidence is never a proof.
- **No hand-authored lock.** The lock is generated; editing it is a
  refusal.

## Related

- [05 · Errors](./05-errors.md) — the certificate the receipt folds in
- [10 · Authority](./10-authority.md) — `no_secret_egress` · the pin chain
- [11 · Decision](./11-decision.md) — the Decision Receipt, now an instance
- [13 · Outcomes](./13-outcomes.md) — the Outcome `nika trace verify` reads
- [14 · Composition](./14-composition.md) — the semantic cache this hash unblocks
