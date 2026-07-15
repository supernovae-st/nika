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
> opened: `nika: v1` in, a signed honest proof out.

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

## `nika.lock` · *the single lock (normative · F7)*

One lockfile pins everything a run resolves, by **digest**:

```yaml
# nika.lock · generated · never hand-edited
lock_format: 1
providers:                      # every model pinned by content digest
  "anthropic/claude-…": { digest: "blake3:…" }
tools:                          # builtin + MCP surface versions
registry:                       # every registry: ref pinned owner/name@version + digest
policy:                         # the resolved policy decisions (10)
model_select:                   # `model: { select: {require, prefer} }` materialized
```

- **Pin by default**: a run resolves ONLY what the lock pins; an
  unpinned dependency is a refusal (`NIKA-LOCK-001`). Nothing floats.
- The lock is generated (`nika lock`), never authored — hand-editing a
  digest is a lie the check catches (the lock's own hash covers it).
- It unifies the prior manifest lock + the pin-by-default rule into one
  file — the local boundary of the same supply chain the gateway (12)
  and the distribution work extend.

## `assert:` · *the author's obligations (normative)*

A task or workflow declares assertions the engine judges — distinct
from `nika:assert` (the single-condition fail-fast builtin): `assert:`
is a **closed vocabulary of properties**, each judged at an honest
level:

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
