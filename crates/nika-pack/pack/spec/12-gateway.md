# 12 · Gateway

> Nika is the semantic execution kernel that **connects** agentic
> standards without reinventing them: a skill, an agent profile, or a
> workflow from any ecosystem enters Nika, is understood, typed,
> secured, locked, executed and attested — without losing its
> semantics and **without gaining invisible authority**.
>
> The contracts in this chapter are **backend-neutral by law**: no
> enforcement backend, agent runtime, or vendor is named anywhere in
> this spec, and the entry fixture for any conforming abstraction is
> *« the same plan, two backends, comparable reports »*. Nika stays
> the semantic truth; a backend enforces, it never redefines.

---

## Five categories, never confused (normative)

| Category | Is | Is NOT |
|---|---|---|
| **Tool** | one atomic capability (builtin · MCP · API) | a place to hide a workflow |
| **Workflow** | a typed, composable computation (the callable contract) | a tool with steps |
| **Skill** | procedural KNOWLEDGE — explains *how* | an authority grant of any kind |
| **Agent profile** | portable agentic configuration | a permission, a budget, or a memory |
| **Pack** | a signed distributable unit {workflows · skills · agents · schemas · assets · manifest · lock · attestations · fixtures} | a running thing |

A skill **never** grants authority. Disclosure of a tool changes the
model's *context*, never its *authority* — the four sets are ordered
and the order is the law (`NIKA-PORT` class when violated):

```
T_presented ⊆ T_discoverable ⊆ T_authorized ⊆ T_installed
```

## The Deployment Bundle · *`deployment_bundle_format: 1`*

A **compiled, portable artifact** (a projection of a Pack — never
authored by hand): everything a host needs to run a workflow the way
its author proved it.

| Region | Carries |
|---|---|
| `manifest` | id · version · owner · the semantic hashes of every workflow it ships |
| `lock` | every dependency **pinned by digest** (spec pin · tool versions · provider catalog snapshot · schemas) — nothing resolves at run time |
| `authority` | the DECLARED boundary: the `permits:`/`policy:` blocks verbatim + the inferred needed-effects projection (10 §certificate) |
| `contracts` | the callable contracts (types · returns · inputs/outputs) of every entry workflow |
| `backend_requirements` | the MINIMUM capability set a backend must attest before this bundle runs (see below) — with per-requirement `on_absent: refuse \| degrade_declared` |
| `attestations` | reserved (W6 — signatures · provenance · SBOM refs) |
| `lifecycle` | reserved (W5 — DeploymentLifecycleIR) |

The bundle is DATA. Signing it proves *origin*, never *safety*
(`signature ≠ safety` — the separation laws below).

## ExecutionBackend · the enforcement contract (normative)

An execution backend is anything that can *enforce* a compiled
authority boundary around a run (a sandbox · a container profile · an
OS policy layer). The contract has four verbs, and every one of them
returns **evidence**, not vibes:

```
capabilities()  → BackendCapabilities
lower(plan)     → EnforcementPlan + PolicyLoweringReport
load(plan)      → EffectivePolicySnapshot        (the READBACK)
observe(run)    → BackendReceiptFragment          (reserved W6)
```

**BackendCapabilities** — for every capability the kernel may ask
(fs-path confinement · network host allowlists · exec program
allowlists · process isolation · secret masking), the backend answers:

```
support ∈ { exact · partial · absent · unknown }
× guarantee (the G27 strength enum — unchanged, never extended here)
× scope (what exactly is covered)
× assumptions (kernel versions · privileges · platform)
× evidence (how the claim was verified — a doc pin · a probe result)
```

`unknown` is an honest answer and is **never promoted** to anything
else. A backend that cannot attest a `backend_requirements` entry
refuses the bundle (or degrades **declaredly** when the bundle allows
it) — a silent downgrade is the one forbidden move.

**The five policy states** (never conflated · the readback is
mandatory):

```
Requested → Compiled → Loaded → Effective → Observed
```

What the author wrote (Requested) is compiled to a backend plan
(Compiled), loaded into the mechanism (Loaded), read **back** from the
mechanism as ground truth (Effective), and finally compared against
what actually happened (Observed · W6). A divergence between any two
adjacent states is a **typed diff with a witness** — decision and
receipt, never a shrug.

**PolicyLoweringReport** — lowering a Nika policy onto a backend
mechanism is classified per rule:

```
exact · restrictive_safe · permissive_unsafe · unsupported · unknown
```

The soundness law is the enforcement twin of the type-side lowering
law: `Behaviors(BackendPolicy) ⊆ Behaviors(NikaPolicy)`. A
`restrictive_safe` lowering (the backend blocks MORE than asked) is
sound and reported; **`permissive_unsafe` is a REFUSAL** (the backend
would allow what the policy forbids — running anyway would make the
file lie); `unknown` is never promoted to safe. Every non-`exact` row
carries the divergence witness.

## AgentRuntimeAdapter · the import/export contract (normative)

An agent runtime adapter connects a foreign agentic ecosystem
(profiles · skills · sub-agent conventions) to Nika. Two artifacts are
mandatory on every import, and they are the whole honesty of the
gateway:

- **FidelityReport** — per imported element, the 6-value fidelity enum
  (`exact · adapted · lossy · unsupported · ambiguous ·
  security_restricted`): what survived translation, what didn't, and
  what was *deliberately* restricted. `lossy` names what was lost;
  `ambiguous` names both readings and refuses to guess.
- **AuthorityDelta** — the authority the element would have had in its
  home runtime vs the authority it has under Nika: ambient credentials
  it can no longer reach · prompt-only conventions that became
  enforced boundaries · default-on capabilities that became
  default-deny. **Import must never *gain* authority**; every loss and
  every hardening is listed, none is silent.

The five delegation notions stay separated (reserved to composition —
the vocabulary is normative now, the mechanics land with workflow
composition): `DelegationPreference` (a prompt hint) ≠
`DelegationAuthority` (an enforced grant) ≠ `DelegationContract` ≠
`DelegationBudget` ≠ `DelegationReceipt`. A runtime whose delegation
control is prompt-guidance-only imports as *preference*, never as
*authority* — the AuthorityDelta says so.

## The separation laws (normative · one voice with 10/11)

```
disclosure        ≠ authorization
delegation pref.  ≠ delegation authority
compaction        ≠ canonical memory
requested policy  ≠ effective policy
logs              ≠ attestation
signature         ≠ safety
backend           ≠ semantic truth
```

And the containment laws every gateway judgment enforces:

```
EffectiveAuthority ⊆ PermittedAuthority
Authority(child)   ⊆ Authority(parent)
Strength(Claim)    ≤ Strength(Evidence)
```

No ambient credentials. No ambient memory. No silent permissive
lowering. No best-effort presented as hard enforcement.

## Errors (the `NIKA-PORT` namespace · new in this chapter)

| Code | Category | Meaning |
|---|---|---|
| `NIKA-PORT-001` | `validation_error` | a gateway artifact (bundle · capabilities report · lowering report · fidelity report · authority delta) is malformed or violates its laws (an unknown promoted · a permissive_unsafe row without refusal · a disclosure set violating the ⊆ chain · a child authority exceeding its parent) |
| `NIKA-PORT-002` | `security_error` | a lowering is `permissive_unsafe` — the backend would allow what the policy forbids; the run is refused with the divergence witness |

## What v1 deliberately does not do

- **No backend is named.** Reference adapters live in engine/ecosystem
  space, pinned by digest at build; the spec never depends on one.
- **No runtime adapter mechanics.** This chapter is the CONTRACT layer;
  sandbox lifecycle, event ingestion, and live adapters are engine
  work gated on their own waves.
- **No delegation mechanics.** The five notions are vocabulary here;
  contracts and budgets land with workflow composition.
- **No attestation.** `attestations` and `observe()` are reserved (the
  proof wave) — a log is not an attestation, and this chapter refuses
  to pretend otherwise.

## Related

- [10 · Authority](./10-authority.md) — the boundary this chapter
  compiles and lowers
- [11 · Decision](./11-decision.md) — the receipt discipline the
  lowering/fidelity reports follow
- [09 · Types](./09-types.md) — the callable contracts a bundle ships
- [06 · Stdlib contract](./06-stdlib-contract.md) — the tool surface
  the disclosure sets order
