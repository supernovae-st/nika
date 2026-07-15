# 11 · Decision

> A typed, bounded, secured, traced workflow can still *decide* by
> fuzzy rules. Constraining the OUTPUT (a closed enum) is not defining
> the DECISION (why `high` rather than `medium`). This chapter makes
> the decision a **contract**: a portable, versioned **Decision
> Bundle** owned by the consuming organization — Nika is a *reference
> implementation* of the contract, never its prison.
>
> Three algebras, never fused:
> [`permits:`](./10-authority.md) is *authority* (required ⊆ permitted)
> · [`policy:`](./10-authority.md) is *law* (the shape of allowed
> action) · a **decision** is *judgment* —
> `D = Evaluate(Bundle, EvidenceSnapshot)` — deterministic, explained,
> appealable. **The model never decides** — the law is this chapter's:
> `infer:`/`agent:` may produce closed, cited semantic *facts*; the
> decision kernel applies the rubric.

---

## The Decision Bundle · *a portable artifact · `decision_bundle_format: 1`*

A bundle is ONE JSON document (produced by hand or by tooling — it is
**data**, never code) with six mandatory regions:

| Region | Carries |
|---|---|
| `manifest` | `id` · `version` (semver) · `owner` · `license` · `valid_until` · the bundle's own `digest` (blake3 of the canonical JSON of everything below) |
| `evidence_schema` | the evidence keys the rules may read: required/optional · a [09-types](./09-types.md) type per key · authorized `sources` · `ttl` · minimum `integrity` (the lattice below) · what missingness means |
| `transforms` | named, **bounded** feature transforms (`clamp` · `linear` · `bucket`) — integer-only by construction (a log-shaped curve is an explicit `bucket` table: honest, and byte-equal across evaluators) — every transform is total on its declared input range and refuses outside it |
| `rules` | the decision rules: named **dimensions**, fixed-point weights, thresholds, dominance, abstention (the Decision IR below) |
| `governance` | `never_automatic: [outcome…]` · `override` (append-only, reason required) · `appeal` · `human_required` triggers · expiration |
| `fixtures` | golden cases the bundle MUST ship: positive · negative · **ambiguous** · **contradictory** · adversarial — a bundle without a contradictory fixture cannot prove its `Conflict` handling |

`explanations` (reason-code templates) is optional but recommended:
the explanation is the FORMULA (term-by-term contributions), never a
post-hoc narrative.

**Ownership law.** The bundle format is Apache-2.0 and
engine-independent. The proof is executable (§the second evaluator):
a stdlib-Python interpreter, living beside the bundle, must produce
**byte-equal** decision JSON against the engine on the bundle's own
fixtures. The `portable` label is *earned by that proof*, never
declared.

## Evidence IR (normative · G14)

One evidence item is

```
e = (key, value, source, observed_at, digest,
     confidentiality, integrity,
     quality { freshness · completeness · independence_group })
```

- **Two orthogonal lattices, never fused** — confidentiality
  (`public ⊑ internal ⊑ confidential ⊑ secret` — the flow lattice of
  [10 §secret flow](./10-authority.md)) × **integrity**
  (`untrusted ⊑ observed ⊑ verified ⊑ authoritative`). The content of
  a PR is a *datum to analyze*, never an instruction — structural
  anti-prompt-injection: rules read evidence VALUES; no evidence value
  ever becomes rule text.
- `completeness` is a closed enum: `complete · paginated_partial ·
  source_down · permission_denied` — a partial read is a FACT the
  rules may weigh, never silently promoted to complete.
- `independence_group` names the correlation group; redundant signals
  share a group and a **contribution cap** (anti-Goodhart: ten mirrors
  of one signal never outvote one independent signal).

An **EvidenceSnapshot** is the frozen set at time `t`:
`Snapshot(E, t)` carries `t`, source cursors, digests, and the list of
*missing* required keys. `D = Evaluate(B, Snapshot(E, t))` — a rerun
at `t'` is a NEW decision with a new receipt, append-only, never a
rewrite.

## Four-valued logic (normative · Belnap)

Rule predicates evaluate in `{True, False, Unknown, Conflict}`:

- **Unknown ≠ False ≠ 0.** A missing/expired evidence key makes
  predicates over it `Unknown`; arithmetic over it contributes an
  **interval**, never an invented zero.
- **Conflict** arises when two sources at `authoritative` integrity
  disagree on one key. A Conflict on a *required* key forces the
  `human_required` outcome and carries a **witness** (both values,
  both sources, both digests). The contradiction classes
  (freshness · source · semantic · unresolved) ride the receipt.

## Decision IR (normative · G15)

The rules region is a **pure, total, bounded, typed** program over the
snapshot — no I/O, no clock, no randomness, no unbounded recursion,
and **no binary floats**:

- **Fixed-point everywhere**: weights and thresholds are integer
  basis-points (`8735` = 87.35%) — the `usd_micros` precedent of the
  [cost certificate](./05-errors.md), generalized. Two evaluators can
  only be byte-equal when no float ever rounds.
- **Scores are VECTORS** of named dimensions (e.g. `priority` ·
  `review_burden` · `change_risk` · `evidence_quality`), each mapping
  to a lane — never one universal scalar. Contributions are exposed
  term-by-term (the explanation IS the formula).
- **Declared monotonicity** per (dimension, evidence key):
  `increases · decreases · none`. A bundle whose fixtures violate a
  declared monotonicity is **refused at publication** — the law is
  property-tested, not prose.
- **Interval propagation + robust dominance**: Unknown-bearing inputs
  propagate as intervals; `A > B` only when `inf(A) > sup(B)`,
  otherwise the pair is *« incomparable with the available evidence »*
  — never a false total order.
- **Bounded discontinuities**: any step function (a secret leak, a
  release workflow, a DB migration) is explicit in the rules and
  carries its own fixture; everything else is locally Lipschitz (the
  bundle declares the bound; fixtures probe it).
- **Identity counterfactual invariance**: the same patch under a
  different author name/tenure yields the same *technical* decision —
  identity keys are declared `identity: true` in the evidence schema
  and MUST NOT feed technical dimensions (fixture-enforced).

## Outcomes and abstention (normative · G16)

The outcome enum is closed:
`recommend · defer · human_required · opted_out · overridden`.

**Abstention is a safety property, first-class**: `defer` (not enough
evidence *yet* — TTL/completeness driven) and `human_required`
(Conflict, never-automatic outcome, or a governance trigger) are
successful evaluations, never errors. `overridden` is append-only with
a mandatory reason and the original decision preserved. Actions listed
in `governance.never_automatic` can never ride a bare `recommend`.

## The Decision Receipt (normative)

Every evaluation emits a receipt:

```
bundle {id · version · digest} + snapshot {t · cursors · digests · missing}
+ per-dimension contributions (term by term) + intervals
+ outcome + conflicts/witnesses + override (if any)
+ determination provenance
```

**Determination provenance** names the commitments that *decided*
between admissible outcomes (which threshold, which dominance test,
which governance trigger) — on top of data provenance. Receipts come
in a PUBLIC and a PRIVATE form linked by digests: publishing the
decision never forces publishing sensitive evidence.

## `nika:decide` · *the deterministic kernel as a builtin*

Decision evaluation reaches a workflow as a **pure builtin** (a tool
under `invoke:` — [no fifth verb, ever](./02-verbs.md)):

```yaml
tasks:
  triage:
    invoke:
      tool: "nika:decide"
      args:
        bundle: "./decisions/pr-triage.bundle.json"   # path or inline object
        evidence: "${{ tasks.collect.output }}"       # the snapshot items
    returns: DecisionResult   # the receipt shape · typed via 09-types
```

- The builtin is **pure compute**: zero required effects (the bundle
  path read is declared like any `fs.read`; an inline bundle needs no
  filesystem at all). Collection stays `invoke:`/`exec:`; semantic
  facts stay `infer:` (closed, cited, uncertainty-carrying); the
  kernel applies the rubric. **The LLM never decides.**
- Arg-shape violations are `NIKA-BUILTIN`-class like every builtin.
  Bundle-law violations are their own namespace:

| Code | Category | Meaning |
|---|---|---|
| `NIKA-DECIDE-001` | `validation_error` | the bundle is malformed or violates its own laws (bad fixed-point, undeclared evidence key in rules, monotonicity fixture failure, missing contradictory fixture) |
| `NIKA-DECIDE-002` | `validation_error` | the evidence snapshot does not satisfy the bundle's evidence schema (type misfit · unauthorized source · integrity below the declared floor) |

Both are deterministic refusals — same inputs, same refusal, both
evaluators.

## The second evaluator (normative · G18)

The reference interpreter is **stdlib Python, engine-free**, designed
to live *in the bundle owner's repo*:
`conformance/decision_core.py` in this spec is the canonical copy.
Conformance for any engine:

```
for every bundle fixture:  JSON_engine == JSON_reference   (byte-equal)
```

The comparison is on **canonical JSON** (sorted keys, no spaces, raw
UTF-8) of the full receipt. This proof is what makes the semantics
belong to the BUNDLE, not to a hidden engine detail.

## The validation protocol (normative for bundle publication · G17)

A bundle that claims production quality ships its protocol *inside
the bundle*: an annotated corpus (≥2 human annotators) · metrics per
output type (binary → confusion + Cohen's κ · ordinal → weighted-κ +
MAE · rankings → Kendall τ · multi-annotator → Krippendorff α) ·
**coverage AND abstention rate always reported beside any score** ·
drift watched by re-running the corpus at every bundle bump ·
promotion thresholds written IN the bundle. The spec defines the
protocol; executing it belongs to each bundle's owners — an engine
never fakes it.

## One obvious way (normative for linters)

- A decision is `nika:decide` over a bundle — never an `infer:` whose
  prompt says « decide » (the closed-enum output would be a decision
  with no contract; linters hint toward the bundle).
- Money-like weights are basis-points integers — a float weight in a
  bundle is `NIKA-DECIDE-001`, teaching the fixed-point spelling.

## What v1 deliberately does not do

- **No workflow-level declaration block.** How a bundle is declared
  first-class in a workflow (`decisions:`?) is an authoring-surface
  decision — it goes to the operator tournament (witness #6); the
  builtin path/inline forms are complete without it.
- **No solver, no learning.** Soft-constraint solving stays LAB (10 §);
  selective/conformal prediction layers are LAB — calibrated research,
  never the deterministic kernel, never presented as a guarantee.
- **No cross-bundle composition.** One evaluation, one bundle;
  delegation/composition of decisions is W-COMP ground.

## Related

- [10 · Authority](./10-authority.md) — permits (authority) · policy
  (law) — the two algebras this one completes
- [09 · Types](./09-types.md) — evidence value types · `returns:` on
  the decide task
- [05 · Errors](./05-errors.md) — the registry (DECIDE rows)
- [02 · Verbs](./02-verbs.md) — four verbs forever; `nika:decide` is a
  tool under `invoke:`
