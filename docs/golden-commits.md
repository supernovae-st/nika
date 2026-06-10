# Golden commits — exemplary patterns

A curated, grep-verified list of commits that exemplify the commit
discipline laid out in [`.claude/rules/commit-granularity.md`](../.claude/rules/commit-granularity.md).
When in doubt about scope, message format, or atomic boundaries, read
one of these and imitate the shape.

The list is intentionally short. If a commit pattern needs more than
3 examples, the rule itself probably needs sharpening, not the catalog.

## Crate admissions (the most important commit type)

| SHA | Crate | Why exemplary |
|---|---|---|
| `42909b1c7` | `nika-error` | First admission — established the 12-gate body format |
| `55a451695` | `nika-catalog` | Larger crate (4,690 LOC + build.rs codegen); body cites mutation %, file budget, exemption justifications |
| `ef8804371` | `nika-kernel` + `nika-kernel-mock` | Twin admission in **one** commit because the trait + mock are inseparable; explicit Gate 11 swarm log |

Inspect with: `git show --stat <SHA>` — note the single-purpose diff
(only the new crate + a one-line `members = […]` insertion in
`Cargo.toml`).

## DX tooling commits (chore + dx scope)

| SHA | What | Why exemplary |
|---|---|---|
| `7beb24dcb` | miri + cargo-hack CI jobs + tokio layer bans | One coherent CI capability, body explains *which* 2026 SOTA gap closed |
| `31128e9a2` | machete + semver-checks + typos CI + unused dep removals | Multi-tool but **one logical capability** (lockfile hygiene); body lists every false-positive justification with reasoning |

## ADR commits (docs scope)

| SHA | What | Why exemplary |
|---|---|---|
| `4cac646e9` | ADR-001..009 inaugural batch | One commit because the 9 ADRs are interdependent and shipped as one decision package |
| `f0e032bd3` | ADR-010..014 (5 SOTA improvement decisions) | Same logic — coherent batch from one audit pass |
| `199119e94` | Bidirectional cross-references audit | **No body content changed**, only Related: bullets — the message is explicit about what did and did not change |

## Submodule-bump commits (parent monorepo only)

These live in the **private** parent monorepo, not this repo, but the
shape is worth knowing: a `chore(submodules): bump <path>` commit only
ever changes a single gitlink line and lists every Diamond commit (branch renamed `main` 2026-05)
it pulls in. See `c14654ef` and `509a52ba` in the parent for the
canonical shape.

## What none of these have

- `--no-verify` (hooks always run)
- `--amend` after push (every change is a new commit)
- Mixed scopes (no `docs+chore+feat` shotgun commits)
- Vague messages ("fix stuff", "wip", "address review")
- Stray files (no `.DS_Store`, no editor swap files)
- Co-author other than `Nika 🦋 <nika@supernovae.studio>`

## When to add to this list

Add a commit here when **and only when** it is the best example of its
type AND replaces a prior example that became stale (e.g. a spec
referenced has since changed). Three or four examples per category is
enough — more becomes a wall of references nobody reads.

## How to verify a candidate before adding

```bash
git show --stat <SHA>            # diff is single-purpose?
git log --format=%B -1 <SHA>     # body explains *why*, not just *what*?
git log --format=%(trailers) -1 <SHA>   # Co-Authored-By: Nika 🦋 ?
```

If any of those produce a “no”, don’t add it.
