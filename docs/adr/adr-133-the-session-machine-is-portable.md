# ADR-133 · The session machine is portable: typed outcomes, identities, hosts

- Status: accepted
- Date: 2026-09-04
- Deciders: the one-door program (OD-F6)
- Related: ADR-126 (project changes from the session), ADR-127 (the native session), ADR-132 (identity, idempotency and protocol)

## Context

The native session (`nika-session`) is a machine — `turn` · `choose` · `consent` · `answer_gate` · `observe_run` — with no I/O of its own: it requests runs as data (`RunRequested` · `ResumeRequested`) and the terminal door executes them through the same path `nika run` owns. That shape is already the portable one. Two things were not: a refusal was a `String` (a host that is not a terminal could only print it), and a consent or a gate answer named nothing (the host answered "whatever waits", which is right at a keyboard and wrong on a reconnecting wire — a second `yes` after a network retry could land on the NEXT proposal).

## Decision

1. **A refusal has a class.** `TurnOutcome::Refusal(Refusal { class, text })` · the classes are the ones a host acts on: `NoIntelligence` (the facts still answer), `IntelligenceRefused` (the previous choice stands), `NotAllowed` (outside what the session may write), `WrongState` (nothing pending · no gate waiting · no census), `StaleRevision` (another proposal or gate waits, or the file changed since the preview), `AlreadyConsumed` (decided once, never twice), `EmptyAnswer`, `Io`. The text is for a human; the class is for the host.
2. **A proposal and a gate have an identity.** `Proposal { id: ProposalId, preview }` — the witness of the preview's bytes — and `GateAsk { id: GateId { trace, task }, question }`. `Held` carries the same proposal id.
3. **A consent or an answer may name what it decides.** `consent_to(&id, answer)` and `answer_gate_for(&id, line)`: stale when another one waits (nothing applied, the waiting one still waits), already consumed when that one was decided, wrong state when none is pending. `consent` and `answer_gate` keep answering whatever waits — the keyboard's contract — and record what they decided, so the duplicate after them is `AlreadyConsumed`, not a new effect.
4. **The machine owns no port a host must implement.** The intelligence is already a trait (`SessionReasoner`); the run door is data the host executes; the facts are the engine's own. A remote host (the resident, an SDK app) drives `SessionRuntime` with the same calls the terminal door makes, and judges outcomes by class and identity.

## Consequences

- The terminal door renders `Refusal` through `Display` and reads the same variants; no behavior changes at the keyboard.
- The public API of `nika-session` grows by the outcome types and the two identity doors; the tuple variants `Proposal` · `Held` · `GateAsk` become struct variants (a consumer names the field it reads).
- A wire for the session (the resident's session door, the SDK) can be built on these types without a second machine; that wire is out of this ADR's scope.
