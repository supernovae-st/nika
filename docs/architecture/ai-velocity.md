# AI Velocity — Why Diamond

The single number that drove the rewrite: **context window fit**.

## The problem with legacy

The legacy v0.79 codebase grew to 322k LOC across 31 crates. The largest
single crate, `nika-engine`, was 138,724 LOC on its own. That's roughly
**750,000 tokens** — 75% of a 1 million token context window — just to
read one crate.

Ask an AI assistant to refactor `nika-engine`? It reads the crate at
75% context saturation. No headroom for the task. It skims. It guesses.
It hallucinates.

We measured this. Three months of refactoring attempts on legacy produced
27 patches. 19 of them reintroduced bugs that had been fixed the previous
quarter. The assistants weren't malicious, they were **starved of context**.
They could see one function at a time, not the system.

## The diamond constraint

Every crate MUST fit in an AI context window with room to think. This
cashed out as:

- **15,000 LOC max per crate** (hard cap, enforced by CI)
- **1,500 LOC max per file** (hard cap, CI blocks)
- **100 LOC max per function** (warning)

The math:

| Metric | Legacy worst case | Diamond cap |
|---|---|---|
| Largest crate | 138,724 LOC | 15,000 LOC |
| Tokens (estimate) | ~750k | ~70k |
| % of 1M context | 75% | **7%** |
| Reasoning headroom | ~250k tokens | ~930k tokens |

**10x more thinking space** on the same task. That's the number.

## Why not just refactor legacy

Refactoring a 322k codebase with 1,276 `.unwrap()` calls and 47 files above
1,500 LOC is not a craft activity — it's damage control. Each of those
unwraps could panic in production. Each of those files is unreadable. The
total cost of fixing them in-place, while keeping main shippable, is higher
than starting fresh.

We chose fresh. Orphan branch `nika-diamond` (since renamed `main`) from commit zero. No
inheritance. Legacy main is a read-only reference: `git show brouillon:<path>`
to look something up, then rewrite it clean. Never copy-paste.

## The 12 gates

Once a crate exists, admitting it to the diamond workspace requires all 12
gates green. The gates are not negotiable:

1. SPEC — `docs/crate-specs/nika-X.md` with purpose, layer, LOC budget, API
2. TDD — tests written before implementation (RED then GREEN commit order)
3. IMPL — compiles, all tests pass, no `# TEMP` without removal plan
4. CLIPPY 0 — `cargo clippy --workspace --all-targets -- -D warnings`
5. MUTATION ≥ 90% — `cargo mutants -p nika-X`
6. PROPERTY — proptest for parsers, security, encoding
7. BENCHMARKS — `benches/` with criterion if hot path
8. DOCS — `cargo doc --no-deps` with zero warnings; every `pub` item documented
9. CANARY E2E — `tests/canary-X.nika.yaml` workflow passes
10. PARITY LEGACY — golden test vs `git show brouillon:...` output
11. REVIEW SWARM — 3 agents in parallel (patterns / idioms / architecture)
12. ATOMIC COMMIT — one commit, `feat(nika-X): admit to workspace — all 12 gates passed`

A crate that fails any gate does not enter the workspace. Period. Exemptions
(when genuinely applicable — e.g., benchmarks for pure-types crate) go in
the spec doc with a paragraph of justification.

## Forever v0.x

Because of this standard, Nika ships in v0.x releases indefinitely. Each
release is diamond-grade for its declared scope. Features arrive as they
pass their 12 gates.

SQLite 1.0 did not have WAL, FTS, JSON1, or window functions. Those came
across 3.x releases over 20 years. Each release was diamond-grade. The
product was complete AT EACH RELEASE for what it claimed to do.

v0.90 ships when 42 crates pass 12 gates and 7 shadow zones are green.
v0.95 ships the Connectome and agent-v2. v0.100 ships WASM plugins and keys
subsystem. Each release is a complete chrysalis stage, not an intermediate
build.

## Cost

~2-3 hours per crate for spec + TDD + impl. ~30 min for clippy + mutation +
docs. ~30 min for canary + parity + review swarm. ~10 min for atomic commit
and MEMORY update.

Call it 4 hours per crate, 42 crates, 168 hours of craft work. Against an
11-12 month self-paced schedule, that's 15 hours of craft per week — well
under the 70/20/10 rhythm (craft / docs / social) we committed to.

The rest of the time goes to weekly dev logs, monthly blog posts, quarterly
podcasts, and the hygiene stack that keeps the ecosystem from drifting.

🦋 Craft, not extraction. Quality, not speed. Forever, not launch.
