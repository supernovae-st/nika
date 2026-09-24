# Crate spec — `nika-compile`

| | |
|---|---|
| Status | **MEMBER** (size-cap split of the admitted `nika-onboard` unit · ADR-137 · D-2026-07-09-N1 · 2026-09-21) |
| Layer | L4 — a library surface; lateral L4→L4 edges `nika-onboard → nika-compile`, `nika-compile → nika-compile-reader` (ADR-138) and `nika-compile → nika-compile-fidelity` (ADR-141), never back |
| Design | the stateless Compile core: one `CompileRequest` in, one `CompileOutcome` out — deterministic HOT admission, native record application, the assembler, the Check preview, the recorded-plan replay across answer rounds; the deterministic reader (frozen: a safety floor) and the private typed semantic plan (operations · effects · obligations · constraints · typed computations) live in `nika-compile-reader` since ADR-138 |
| IMPL | measured by `scripts/crate-metrics.sh nika-compile` at each freeze; the crate carries what `nika-onboard::compile` carried on 2026-09-21 minus the reader and the plan, descended to `nika-compile-reader` the same day (ADR-138 · the gate's own counter: 9,778 prod LOC after the split · 52 unit tests · 15 integration suites) |
| LOC budget | ≤15k crate · ≤1500/file · ≤100/fn (the frozen reader's tables and their `lookup-table` LOC-EXEMPT live in `nika-compile-reader` since ADR-138) |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |
| Publish | `false` — member of the `nika-onboard` unit |
| NIKA codes | none minted here — compile diagnostics speak through `CompileOutcome`; machinery failures keep the canonical `nika-error` voice |

## 1. Purpose

`nika-onboard` reached **17,318 prod LOC against the 15,000 cap** during the compiler war
room (2026-09-20/21): the compile module alone was 16k lines of the crate's 23k. Per
D-2026-07-09-N1 a size-cap split is ONE architectural unit in TWO workspace members: the
Compile core descends here, the onboarding surface (`nika init`, the gallery, the briefs)
stays in `nika-onboard`, which re-exports this crate at its historical path:

```rust
// nika_onboard::compile explicitly re-exports the supported core API and:
// Cognition, NoProvider, compile_with_cognition, compile_with_provider, decide.
// The member-only surface and outcome-building helpers remain in nika_compile.
```

Every caller keeps writing `nika_onboard::compile::…` (`nika-cli-host`, `nika-serve`, the
MCP integration). Existing explicit facade imports remain available. ADR-140 adds a
shared Rust surface and moves direct core seat exports to `nika_compile_cognition`; see
the current member boundary below. This is not a claim of an identical public API.

## 2. The boundary, measured

| direction | edges before the split |
|---|---:|
| `nika-onboard` (surface) → `compile` | 1 (`lib.rs` declares the module) |
| `compile` → the rest of `nika-onboard` | 2 (`intent::STOPWORDS`, `banner::sentence`) |

The two borrowed helpers moved into `nika_compile::text` (`STOPWORDS`, `banner_sentence`,
`banner_lines`, `is_label`) and `nika-onboard` re-imports them. The member never depends
back on the surface.

## 3. Contracts kept

- The split retains the machine wire projection and recorded-plan format. The Rust
  request surface grows under ADR-140; it is not byte-identical API text. Zero-call replay,
  candidate semantics and provenance remain contracts to verify on the composed source
  with the compiler and transport regression suites.
- The deterministic reader is frozen: no cue or head is added; every new law is a structural
  one (path boundaries, anaphora, the shape of a human gate, the carrier of a constraint) or
  lives in the typed semantic plan.
- The 12 ADR-003 gates were passed by `nika-onboard` at its admission; this member inherits
  them as the second half of the same unit (the ADR-115 precedent). Mutation and property
  attestations for the compile core are owed as pending evidence, tracked with the season-2
  debt wave, never claimed.

## 4. Current member boundary (ADR-140)

`doors` owns deterministic HOT admission, plan/native replay and record application.
Record application completes a native candidate's boundary only from its answers: an
answered endpoint's host (`permits.net.http`), and an answered bare `${{ const.<slug> }}`
path, which replaces the seat's empty `permits.fs` placeholder (`[""]`, flow or block
`- ""`) in the direction the capability inference derives — never a path that escapes the
workspace, never a glob, never a direction the seat declared with any other entry.
A replayed HOT record receives the reader's unnamed-destination floor again (idempotent), so a
record written before that law asks the path of the write it lacked (« … dans un fichier »).
The assembler's `const.output_path` question stays open when the answer names no file:
prose, like a directory, a glob or a placeholder, is refused and asked again, never dropped.
`assemble`, `bindings`, `approval`, `laws`, `ledger`, `network`, `realize`, `support`,
`trigger` and `writes` implement deterministic compilation. `edit`, `edit_source`,
`materialize`, `retrieve`, `types`, `pattern` and `wire` retain their existing roles.
`surface` states the contracts used by the cognition member, including the one spec pin
reader and the assembler laws its tests compare against.

The COLD composer, proposal decoder, predicates, decision seats and native/sketch/transform/
knowledge authoring now live above this crate in `nika-compile-cognition`. Core depends on
Reader for the frozen reading/plan and on Fidelity for deterministic candidate laws
(ADR-141); it never depends on cognition in production. Kernel/Tokio are development
dependencies for the unchanged compiler suites. The supported onboarding facade combines
core and cognition; the direct core crate's seat exports move to the cognition member.

The split is implemented under the authorized consolidation. Compilation, complete suite
execution and canonical API qualification remain integration checks; the inherited
mutation/property attestations remain pending. Historical counts above describe their
recorded revision, not current test or size results.

## Exact schedule requirement

`requested_trigger.cadence` remains a coarse label. Additive nullable `cron`
contains the exact five fields of the existing cadence grammar when a bounded
FR/EN trigger phrase is complete. The original `source_hint` stays evidence;
a `trigger.cadence` answer is projected by the same reader and survives plan
replay. Neither field is a grant, timezone choice, project row or execution.
Older machine documents lacking `cron` remain readable by tolerant consumers;
absence must never be reconstructed from the coarse label for activation.

Supported exact forms are daily/weekday/single named weekday with an explicit
clock, and every N hours/minutes where N divides 24/60 (digits, plus one/two in
FR/EN). Examples: every Tuesday at 09:15, chaque vendredi à 18h30, toutes les
deux heures. The interval phase is zero on the local clock, not elapsed time;
the activation review displays it and the canonical scheduler owns DST. Missing
time/weekday, conflicting periods/clocks, mixed periods, monthly/alternate-week
recurrence and non-divisor intervals retain their words and yield null `cron`.
This does not claim unrestricted natural-language cadence understanding or add
another cron parser. `nika-cadence` alone validates/executes the bound expression.

## Observed fields and pending transformations

Source observation distinguishes absent, unreadable, empty, unknown and observed
material. An observed field choice is grounded in that source; a partial sample
is not a complete schema. A missing field asks a closed clarification rather than
silently selecting another key or returning an empty result.

A pending transformation preserves intent, plan and observed source identity,
field choices and bounded attempt lineage across answer rounds. Record replay
rejects changed or malformed context as `PendingTransformError`, rendered through
the existing authoring outcome diagnostic. A verified transform is reusable only
for the same captured context. This record is neither an executable grant nor
permission for a new source or Run. If the host supplies no fresh observation,
replay can validate only its captured source, not assert current filesystem identity.

Each unnamed output keeps a stable path question across rounds. A single unnamed
output retains `const.output_path`; several use numbered keys in plan order. One
answer cannot fill two different outputs, and an answered file already assigned
to another output is refused. Content fidelity checks data edges separately from
ordering or guard edges; this structural floor is not a general proof of semantic
correspondence between every producer and every requested result.

The assembler owns one compute binding. Independent typed computations feeding
several files leave that binding unresolved instead of reparsing their joined
descriptions into one partial rule. The normal authoring escalation can generate
the complete native task graph. A typed rule for the whole detail retains its
deterministic path, as does a single-output pipeline whose parsed rule retains
every recorded typed stage. This structural boundary does not prove arbitrary
model-generated computations semantically correct.

The lexical reader supplies hypotheses for effects it cannot settle: an indirect negation
does not become a ban, and an undecided effect is not an obligation to execute. Native
authoring sees those open readings separately from settled constraints; any realized
uncertain effect is stated in review. Literal-targeted bans retain their own scope,
including relative paths and referenced constant destinations. These readings grant
no permissions: Check, exact-byte review, consent and runtime admission still apply.
