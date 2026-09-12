# Diamond discipline

The common contributor contract is `AGENTS.md`. For architecture changes, use
`docs/architecture/forward-compat-invariants.md` and the crate-layer registry.
Optional historical handoffs help locate evidence; they do not override the
current request or the public engine contract.

## Crate admission

Adding a crate to workspace membership requires all 12 gates from
`docs/adr/adr-003-12-gate-admission.md` in the same PR. Use the `crate-admit`
skill for implementation and `gate-check` for an evidence audit. Document a
contract-supported N/A in the crate spec with its reason; a missing test,
reviewer or tool is pending evidence, never an exemption.

The gates are spec, test-first evidence, implementation, warning-free clippy,
mutation floor, property coverage, applicable benchmarks, documentation,
canary, legacy parity, three independent review perspectives and atomic commit.
Do not repeat admission gates for unrelated documentation edits. Fix a gate's
cause rather than bypassing a hook or broadening the verdict.

## Legacy and source evidence

Read `brouillon` only with `git show` or `git grep`; never check it out or
modify it. Understand the behavior and implement it with the current traits,
error propagation and layer discipline. Retain no legacy defect for parity.

Verify factual claims against source, manifests or executed tools at the named
revision. A search result, test count, clean build and successful runtime test
are different evidence. When a check cannot decide a whole claim, state the
strongest covered claim and the remaining limitation.

## Scope and release

Follow current `ROADMAP.md` and release policy, not historical dates, phase
names or alpha-tag recipes. Readiness depends on the required gates. Continue
authorized changes and publication; ask only about a decision or effect that
still lacks authority. The number of files touched does not by itself make an
authorized cross-cutting refactor out of scope.
