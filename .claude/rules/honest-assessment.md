# Honest Assessment Rules

**No theatre. No optimism. Numbers > narratives.**

## Phantom vs Real

Before claiming a feature "works", verify:

1. **Grep production call sites**: `grep -rn "<function>" tools --include="*.rs" | grep -v test | grep -v '//'`
2. **Count dependants**: `grep -r "use <crate>" tools/*/Cargo.toml`
3. **Check the call path end-to-end**: does production code actually reach this function?

A function that compiles and has tests but **zero production callers** is PHANTOM. It's not "done", it's "scaffolding".

## Phantom Indicators

Flag any of these in session reports and real-state audits:

- Error code defined (`NIKA-XXX`) but never constructed anywhere
- Feature flag referenced in docs but not checked at runtime
- `pub use` re-export with no external users
- Struct field with `#[allow(dead_code)]` and no plan to use it
- Trait method with default `Err(NotImplemented)` that stays that way 3+ sessions
- "Future" / "post-launch" / "coming soon" in a docstring for 5+ sessions
- Crate with 0 workspace dependants

## Numbers-First Reporting

**Every session memo MUST include**:

```
Engine LOC:    <before> → <after>  (<delta>)
Workspace LOC: <before> → <after>  (<delta>)
Tests:         <before> → <after>  (<delta>)
D/A ratio:     <deleted>/<added> = <ratio>
Clippy:        <before> → <after>
Zombies:       <closed> closed, <opened> new
Gates:         G1 <pass/fail>, G2 <pass/fail>, ...
```

**No session memo is valid without these numbers measured, not estimated.**

## When the Plan is Wrong

If you're executing a plan and discover that:

- A file is 5× larger than the handoff said
- A module has 10× more coupling than estimated
- A "simple" extraction requires 3 kernel traits not 1
- A "dead" function turns out to have 47 callers
- An estimate was based on test code, not production code

**STOP. Do not continue with the wrong plan.** Write a discovery memo documenting what you found. Propose 2-3 alternative approaches. Hand off to the next session.

Executing the wrong plan just to claim "headline delivered" is the worst outcome. Better to deliver a discovery than a lie.

## The "Obvious" Trap

When you find yourself writing things like:

- "obviously we can just..."
- "it should be straightforward to..."
- "a simple refactor to..."
- "quickly move X to Y..."

**STOP. That's where estimates go wrong.** Run the actual grep. Count the actual LOC. Read the actual imports. The monolith hides complexity behind familiar-looking code.

## Swarm Review Every Session

After every session, dispatch at least 3 review agents in parallel:

- `spn-nika:code-reviewer` — Nika-specific invariants, AGPL, NikaError patterns
- `spn-rust:rust-pro` — idiomatic Rust, ownership, performance
- `feature-dev:code-reviewer` — general quality, test coverage

For sessions with >500 LOC changes or new kernel traits, add:

- `spn-rust:rust-architect` — dependency direction, layer compliance
- `feature-dev:code-explorer` — mapping what was touched

**Apply findings in the SAME session, not a follow-up.** Fix as you go.

## Document What Wasn't Done

Every session memo MUST have a "Deferred" section explaining what was IN the plan but NOT delivered. If you planned 6 commits and delivered 4, the memo says:

```
## Deferred
- [ ] Phase 5 unwrap migration — ran out of time after 4 commits
  - Why: MockBindingStore test migration took 2x estimate
  - Impact: S24 now must do it + the S24 headline
  - Adjusted: add to zombie backlog if not done by S26
```

**Never write "all phases complete" when they aren't.**

## Trust Numbers Over Docs

If the README says "9 providers" and the code has 14 providers in KNOWN_PROVIDERS, the code is right.

If SECURITY.md says "6-layer defense" and L5 ML detection is never emitted, the code is right.

If CLAUDE.md says "dispatch() orchestration layer" and grep shows 0 production callers, the code is right.

**The code is always more honest than the documentation. When they disagree, update the docs.**
