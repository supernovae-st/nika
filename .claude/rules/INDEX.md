# Rules INDEX — nika/engine

Route-finding map for Diamond enforcement rules. Agents should load this first, then `@`-reference only the specific rule(s) needed for the current task. Keeps context lean.

| Rule | Path | Tags | When to load |
|---|---|---|---|
| Diamond discipline | [diamond-discipline.md](diamond-discipline.md) | diamond, 12-gates, main-read-only | **Before ANY session start, ANY commit** — non-negotiable rules |
| Nika invariants | [nika-invariants.md](nika-invariants.md) | invariants, crate-count, layers, naming | Before crate admission, ADR write, structural change |
| Commit granularity | [commit-granularity.md](commit-granularity.md) | commit, conventional, atomic, subject-len | Before every commit |
| Session discipline | [session-discipline.md](session-discipline.md) | session, pre-flight, end-of-session, anti-patinage | Beginning + end of every session |
| Evolution | [evolution.md](evolution.md) | archive, memory, freshness, update-cadence | When touching memory/ files or canonical docs |

## Quick decision tree

```
User asks you to …                    → Load rule
─────────────────────────────────────  ─────────────────────────────────
"admit a new crate"                    diamond-discipline + nika-invariants + commit-granularity
"make a commit"                        commit-granularity
"starting a session"                   session-discipline + diamond-discipline
"update memory / MEMORY.md"            evolution
"rename a crate / symbol"              nika-invariants + diamond-discipline
"write an ADR"                         nika-invariants + evolution
"skip a gate"                          diamond-discipline (NO exceptions rule)
```

## Authority chain (from `../CLAUDE.md`)

Higher in this list wins when two docs contradict:

1. `~/.claude/projects/-Users-thibaut-dev-supernovae-hq/memory/POST_AUDIT_REVISIONS.md` — supreme authority
2. `~/.claude/projects/-Users-thibaut-dev-supernovae-hq/memory/PRE_LAUNCH_GATES.md` — 7 shadow zones
3. `~/.claude/projects/-Users-thibaut-dev-supernovae-hq/memory/HANDOFF_PHASE_1_REVISED.md` — current execution plan
4. `.claude/rules/*.md` (this directory) — project-specific enforcement
5. `~/.claude/projects/-Users-thibaut-dev-supernovae-hq/memory/project_ai_velocity_north_star.md` — WHY diamond (decision filter)

## Loading philosophy

- **Start small**: load this INDEX.md first (≤400 tokens)
- **`@`-reference**: cite specific rules via `@.claude/rules/<rule>.md`
- **Don't preload all**: 5 rules total ~6000 tokens — too much for every task
- **Update this index** when adding a new rule (file + tags + trigger)

## Related

- `../CLAUDE.md` — Diamond rules entry point (authority hierarchy, 12 gates, interdits stricts)
- Root monorepo rules at `../../../dx/.claude/rules/INDEX.md` (hygiene, submodule, naming, security, root-structure)
