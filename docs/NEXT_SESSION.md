# NEXT_SESSION — agent orientation

If you are a contributing agent (or a fresh human contributor) opening
this workspace, this file is your single starting point. It points to
the public sources of truth and tells you what to verify in the first
five minutes before touching any code.

## What this repository is

The Diamond rewrite of Nika — a Rust workflow engine for AI. We are
crafting 40-42 crates from scratch on the `nika-diamond` orphan branch.
Each crate must pass 12 gates before it is admitted to the workspace.
Read [docs/MANIFESTO.md](MANIFESTO.md) for the why and
[docs/adr/](adr/) for the how.

## First five minutes — read in this order

1. [docs/MANIFESTO.md](MANIFESTO.md) — what Nika is and is becoming
2. [docs/adr/README.md](adr/README.md) — the 15 ADRs index
3. [docs/architecture/forward-compat-invariants.md](architecture/forward-compat-invariants.md) — 8 patterns + 10 rules every public type must follow
4. [docs/crate-specs/](crate-specs/) — one spec per admitted crate (currently 5: error, catalog, catalog-verify, kernel, kernel-mock)
5. [.claude/CLAUDE.md](../.claude/CLAUDE.md) — the agent rule layer (12 gates, banned patterns, mandatory patterns)
6. [.claude/rules/diamond-discipline.md](../.claude/rules/diamond-discipline.md) — non-negotiable rules (orphan branch, rewrite-not-copy, shadow zones)

## Pre-flight — verify before any code change

```bash
git branch --show-current               # must be nika-diamond
git log --oneline -1                    # current HEAD
cargo check --workspace                 # must compile
cargo test --workspace --lib            # must pass — ALWAYS --lib (no Keychain popup on macOS)
cargo clippy --workspace --all-targets -- -D warnings   # must be 0 warnings
cargo machete                           # must report no unused deps
bash scripts/ci/check-adr-coverage.sh   # all admitted crates covered
```

If any of these fails on a clean tree, **stop and diagnose** before
proposing changes. Diamond invariant: every commit compiles, tests,
clippy 0, machete clean.

## Where to find the live state

| Source | What lives there |
|---|---|
| [docs/adr/](adr/) | 15 architectural decisions (Accepted) |
| [docs/crate-specs/](crate-specs/) | per-crate spec (Gate 1) |
| [docs/architecture/](architecture/) | forward-compat invariants + AI-velocity north star |
| [docs/golden-commits.md](golden-commits.md) | exemplary commits to learn from |
| [`scripts/ci/`](../scripts/ci/) | gate check scripts |
| [`Cargo.toml`](../Cargo.toml) | `[workspace.dependencies]` is the version pin floor |
| [`.claude/commands/`](../.claude/commands/) | `/diamond-gates`, `/diamond-health`, `/legacy-lookup`, `/admit` |
| [`.claude/skills/`](../.claude/skills/) | `crate-admit`, `gate-check` |

## Where the *current session* handoff lives

Per-session execution context (what we are currently building, which
gate is half-green, what to do next) lives in the maintainer's
private session memory and is therefore **not** in this public
repository. If you are an external contributor, read the ROADMAP.md
plus open issues plus recent commits — that is the public surface.

## What to do if you are about to admit a new crate

Use the `/admit` slash command (loads
[`.claude/commands/admit.md`](../.claude/commands/admit.md)) — it walks
the 12 gates sequentially, runs the automated checks, and helps draft
the canonical admission commit message.

## What to do if you are unsure

Stop. Read the relevant ADR. Read [.claude/rules/](../.claude/rules/).
The Diamond rule of last resort: *quality > speed, no deadline*. There
is no penalty for asking — only for shipping a crate that drags 1,500
lines of legacy debt across the orphan boundary.
