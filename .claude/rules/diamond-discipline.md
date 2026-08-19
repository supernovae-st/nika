# Diamond Discipline — non-negotiable rules

## Rule 1 — Authority chain

If you're unsure about anything, consult this chain :

1. `~/.claude/.../memory/POST_AUDIT_REVISIONS.md` — supreme authority
2. `~/.claude/.../memory/PRE_LAUNCH_GATES.md` — shadow zones
3. `~/.claude/.../memory/HANDOFF_PHASE_1_REVISED.md` — current task
4. `.claude/CLAUDE.md` (this repo)
5. `.claude/rules/*.md`

If two docs contradict, **higher in the list wins**.

## Rule 2 — 12 gates, no exceptions

A crate admission to `Cargo.toml` `members = [...]` **requires** all 12 gates
green in the same PR. No "we'll fix gate X later." No "the spec is obvious."

If a gate is genuinely not applicable (e.g., benchmarks for a pure-types
crate), document the exemption in `docs/crate-specs/nika-X.md` with a
1-paragraph justification.

## Rule 3 — Brouillon is read-only

`brouillon` branch contains legacy code at HEAD `830aa6154` (or user
hotfixes). This branch (`main` · production · renamed 2026-05-06 from
`nika-diamond`) **NEVER** modifies brouillon. Brouillon is referenced
via `git show brouillon:path/to/file.rs` only.

If you need to copy something from brouillon : read it, understand it,
REWRITE it propre (not copy-paste). Legacy has 1,276 unwraps in
nika-core alone — inherit those = inherit the bugs.

## Rule 4 — Rewrite, not verbatim copy

Diamond rewrite mandate :
- Every unwrap in legacy → `?` propagation
- Every file >1500 LOC → split into modules
- Every `#[allow(dead_code)]` → delete or pub(crate)
- Every unclear API → documented + tested via property testing

User validation 2026-04-13 : "le meilleur truc le plus propre possible,
refaire les fonctionnalités qui ne marchent pas".

## Rule 5 — Shadow zones are pre-launch gates

7 shadow zones identified by audit. They are NOT optional :
- Gate 1 : nika serve input trust (P0) — résolu 2026-08-19 (W5 · ADR-116) :
  `nika serve` reads ONLY the project's `nika.yaml` (the vocab's shape and the
  cadence grammar judge it BEFORE any firing) and its own `.nika/arm/`
  sidecar — no socket, no port, no network read, no external argument
  (`--once`/`--dry` are the whole public surface; `--now`/`--until` stay
  hidden replay hooks). Pinned by
  `serve_has_no_input_but_the_registry_and_its_state`
  (`crates/nika-cli/tests/serve.rs`).
- Gate 2 : cross-provider structured output parity (P0)
- Gate 3 : binding/template/mod.rs 7,243 LOC (auto-résolu Phase 1 nika-binding)
- Gate 4 : L1 taint runtime
- Gate 5 : for_each per-element spotlight
- Gate 6 : NikaError Display parity (auto-résolu Phase 1 nika-error)
- Gate 7 : provider parity matrix

`git tag v0.90.0` is blocked until all 7 are green.

## Rule 6 — Timeline honesty

Constellation total = 11-12 mois honnête. NOT 14 semaines, NOT 9 mois.
If you feel pressure to compress, re-read this rule. Quality > speed.

No per-phase deadline estimates. Each phase takes what it takes.

Ship cadence : 1 crate admission = 1 commit = 1 tag increment `v0.90.0-alpha.N`.
Every 4 weeks : blog post or dev log entry. Public accountability.

## Rule 7 — Hallucination forbidden

Every factual claim about code state MUST be backed by a grep command
that produced it. Memory of prior sessions is NOT trustworthy.

Before claiming "this function has 0 callers" :
```bash
grep -rn 'function_name' tools --include="*.rs" | grep -v test | grep -v '//'
```

Before claiming LOC numbers :
```bash
find tools -name '*.rs' | xargs wc -l | tail -1
```

The code is authoritative. MEMORY.md is a compressed reference, not truth.

## Rule 8 — Anti-patterns banned

- Creating `.md` files during work sessions (except approved handoffs/memos)
- Spawning review swarms without purpose
- Rewriting canonical docs without user consent (POST_AUDIT_REVISIONS, etc.)
- Pivoting strategy mid-session ("actually let's do it differently")
- Suggesting "maybe we should reconsider X" (if X is locked, it stays locked)

If you catch yourself doing one of these, STOP. Reread the authority chain.
Reread the current handoff. Execute.

## Rule 9 — Skills usage

Mandatory per work type :
- Rust refactor → spn-rust:rust-pro + spn-rust:rust-architect (parallel review)
- Nika feature → spn-nika:code-reviewer before commit
- Deletion session → 4-agent swarm per deletion-first.md
- Any commit → skills check via `/spn-powers:system:yo`

## Rule 10 — Budget restart = 0

User has used 5 restarts previously. The 6th is forbidden.

Pivots are allowed (gate-review quarterly). Resets are not.

If you feel we should restart, that's a gate-review moment. Escalate to
user. Do not act unilaterally.

🦋
