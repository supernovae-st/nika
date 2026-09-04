# ADR-134 · Trust is a rung, never a word

- Status: accepted
- Date: 2026-09-04
- Deciders: the one-door program (OD-F8)
- Related: ADR-128 (settlement · unknown cost ≠ zero), ADR-132 (identity · idempotency · protocol), #1253

## Context

The access plan names the path that serves a model (`access` · `chosen` · `billing` · `pinned` · the rejected and outranked candidates) and every door carries the same row. What no row said was how far that path's IDENTITY is proven. A harness seat is admitted by PATH presence (#1253 measured an eight-line fake `codex` believed with full user rights); an API key in the environment is a key, not a working one until something dials; a keyless local server is a profile line until it is pinged. The human lines said « seat present » and « key present · not validated » — true words, in prose, on one surface. The machine rows said nothing, and a reader could take « the seat is present » for « the seat is who it says ».

## Decision

1. **A rung on the plan.** `nika_types::access::Trust { Declared, Discovered, Observed }` (ordered · `#[non_exhaustive]`) on `AccessPlan.trust`, defaulting to the floor (`Declared`) and raised only by evidence: `Trust::from_evidence(class, configured, reachable)` gives `Observed` to a probe that answered (`doctor --ping`) and to the in-crate mock, `Discovered` to a key or a seat found on this machine, `Declared` to a keyless local path nobody pinged and to anything unconfigured.
2. **The candidate carries it, the plan inherits it.** The resolver never invents a rung: the chosen candidate's `trust` becomes the plan's; a pinned seat is `discovered` (the pin refuses when the binary is absent, and nothing dialed it).
3. **Every wire row says it.** `lane_rows` adds `trust` beside `billing` — `check --json` · `run --dry-run --json` · the trace boot manifest · the resident's check summary — and the two human lines (`check`'s access rung · `run`'s announce) print the rung word in the same parenthesis as the class and the billing.
4. **`attested` is reserved, not named.** No path in this build proves an identity (a handshake, a hash, a signed seat); a variant nobody can reach would be a word above the proof. When #1253's confinement half lands, the enum grows additively.

## Consequences

- A reader of any door can rank paths by proof and never mistake presence for identity; the wording tests pin that no rung is claimed above its evidence.
- `AccessPlan` and `AccessCandidate` gain a field (both `#[non_exhaustive]` · construction goes through `new` + `with_trust`); the SDK's lane type learns `trust` in the 0.118 client (OD-F9).
- The confinement of seats (#1253's execution half) stays out of scope: this ADR makes the plan honest about what it knows, it does not make a fake seat harmless.
