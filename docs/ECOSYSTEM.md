# Nika Ecosystem

> The master organization of the Nika project across repos, tools, tracking
> systems, and rituals. Living document — updated quarterly.
>
> Authority: this doc describes the ecosystem. For rules and gates, see
> [`.claude/CLAUDE.md`](../.claude/CLAUDE.md) + [`ROADMAP.md`](../ROADMAP.md).

## Ecosystem map

```
                           ┌─────────────────────────┐
                           │  VISITOR / USER         │
                           │  → nika.sh (entry)       │
                           └────────────┬─────────────┘
                                        │
              ┌─────────────────────────┼─────────────────────────┐
              ▼                         ▼                         ▼
      ┌──────────────┐         ┌──────────────┐         ┌──────────────┐
      │  nika.sh     │         │ docs.nika.sh │         │ atlas.nika.sh│
      │  Astro/DO    │         │  Mintlify    │         │ Gource+snaps │
      │  marketing   │         │  reference   │         │ evolution    │
      └──────┬───────┘         └──────┬───────┘         └──────┬───────┘
             │                        │                         │
             ▼                        ▼                         ▼
      ┌──────────────┐         ┌──────────────────────────────────────┐
      │ repo:        │         │ repo: nika (AGPL-3.0)                │
      │ nika.sh      │         │ branch: nika-diamond (default)       │
      │ (Astro src)  │         │ ├── Cargo.toml    ◄── SSoT crates    │
      └──────────────┘         │ ├── docs/         ◄── SSoT arch      │
                               │ ├── CHANGELOG.md  ◄── SSoT narrative │
                               │ └── .github/      ◄── CI + releases  │
                               └──┬──────────┬──────────────┬─────────┘
                  ┌───────────────┴──┐   ┌───┴──────┐   ┌──┴───────────┐
                  ▼                  ▼   ▼          ▼   ▼              ▼
          ┌──────────────┐  ┌──────────────┐ ┌──────────────┐ ┌──────────────┐
          │ nika-client  │  │ nika-registry│ │ homebrew-nika│ │ nika-design- │
          │ (TS SDK)     │  │ (workflows)  │ │ (brew tap)   │ │ skill        │
          └──────────────┘  └──────────────┘ └──────────────┘ └──────────────┘


  TRACKING PLANE (private):
  ┌───────────────────────────────────────────────────────────────────┐
  │ Linear (SPN team)      — 2 initiatives, issues mirror crate DAG   │
  │ ~/.claude/.../memory/  — Thibaut's working memory (MEMORY.md)     │
  │ supernovae-hq/docs/    — strategic (launch, brand, research)      │
  └───────────────────────────────────────────────────────────────────┘
```

## Single source of truth

| Fact | Authoritative | Downstream (sync) |
|---|---|---|
| Crates admitted | `Cargo.toml` `members` | ROADMAP.md table, MEMORY Quick State |
| HEAD / latest commit | `git rev-parse HEAD` | MEMORY Quick State (Fri) |
| Released versions | Git tags `v0.*` | GH Releases, CHANGELOG (git-cliff auto) |
| What shipped (narrative) | `CHANGELOG.md` | Blog (monthly), social (Fri), Discord |
| Current work item | Linear "In Progress" | Git branch `nika-<crate>`, commit msg |
| Public architecture | `docs/architecture/*.md` | docs.nika.sh |
| Private architecture | `memory/POST_AUDIT_REVISIONS.md` | distilled → public ADRs when safe |
| Public roadmap | `ROADMAP.md` | docs.nika.sh `/roadmap`, GH Milestones |
| Private-extended roadmap | `memory/NIKA_COMPLETE_ROADMAP.md` | never surfaced publicly |
| Shadow zones | `memory/PRE_LAUNCH_GATES.md` | public only once green |

**Rule**: a fact appearing in two places MUST have one marked `# Derived from <source>`.

## Personas + entry points

| Persona | Lands on | First 5 minutes |
|---|---|---|
| Curious first-timer | nika.sh | Hero + 30s demo + `brew install` + quickstart + Discord |
| Rust contributor | github.com/supernovae-st/nika | Badges + `nika-diamond` context + CONTRIBUTING + `good-first-issue` |
| Nika user | docs.nika.sh | Quickstart + 5 verbs cheatsheet + registry + SDK examples |
| Journalist / investor | nika.sh/about → supernovae.studio | North star + team + roadmap + press kit + contact |
| Future Thibaut (6mo) | `memory/MEMORY.md` Quick State | HEAD + phase + handoff + Linear "In Progress" |

## Weekly operating rhythm

```
MON   Triage hygiene issue + Linear inbox (30m) → craft gates 1-3 (6h)
TUE   Craft gates 4-5 (4h) + gates 6-8 (2h) + spec doc (2h)
WED   Craft gates 9-10 (3h) + review swarm (2h) + fix P0/P1 (2h)
THU   Polish + final passes (3h) + admit crate + gate 12 commit (1h)
      + update STATE.md + MEMORY + ROADMAP (1h) + buffer (2h)
FRI   Dev log write (1h) + cross-post (1h)
      + Friday ritual: hygiene + Linear groom (1h) + brainstorm next (2h)
      17:00 SHIP — close week, commit MEMORY drift fixes

SAT/SUN  REST. No Nika.

MONTHLY W4 Friday     blog deep-dive (3h)
QUARTERLY              podcast episode + gate-review (1 day)
```

Cadence in steady state = **1 crate/week**. Heavy crates (schema ~13k LOC, cli ~20k LOC) get 2 weeks.

## Keep / cut / add

**Keep (load-bearing)**:
- 12 gates per crate — non-negotiable
- Friday ritual + dev log — the public accountability engine
- MEMORY.md Quick State — future-Thibaut onboarding doc
- `cargo-public-api` + `cargo-semver-checks` — forward-compat enforcement
- Mintlify for reference docs

**Cut (over-engineered for current stage)**:
- 18 Linear milestones → collapse to 6 (one per version: v0.80, v0.85, v0.90, v0.95, v0.100, v0.110)
- 5 Linear initiatives → keep 2 (Diamond craft + Launch)
- 15 hygiene vectors → review annually; current load is fine

**Add (missing for polished OSS)**:
- `CONTRIBUTING.md` — 12 gates abridged, DCO/CLA statement
- `CODE_OF_CONDUCT.md` — Contributor Covenant 2.1 verbatim
- `SECURITY.md` — email `security@supernovae.studio`, 90-day disclosure, GPG key
- Triage labels + auto-labeling bot (by crate path)
- Release signing — `cosign` + `cargo-dist` multi-platform + SBOM via `cargo-cyclonedx`
- `nika-atlas` repo — evolution viz (Gource monthly + screenshots + SVG dep graph)

## Tool roles

| Tool | Role | When used |
|---|---|---|
| Cargo workspace | source of truth for crates | always |
| Git tags | source of truth for releases | on admission |
| git-cliff | auto-CHANGELOG from Conventional Commits | every tag push |
| cargo-public-api / semver-checks | breaking change detection | every PR |
| cargo-mutants | mutation testing | per-crate admission |
| scripts/hygiene | 15 drift vectors | commit + nightly CI |
| Claude Code hooks | diamond discipline enforcement | every tool call |
| Linear | execution tracker | daily |
| Mintlify | user docs | on push to docs/mintlify/ |
| GitNexus (MCP) | graph-aware dev (review swarm only) | Thu review, contributor onboarding |
| Gource | git history visualization | monthly MP4 in nika-atlas |

**GitNexus**: MCP-only install, never public-facing source of truth. `Cargo.toml` + git remain authoritative. If GitNexus disappears tomorrow, zero data loss.

## Closing principles

1. **One authoritative source per fact** — if duplicated, mark `# Derived from`.
2. **Private stays private** — `supernovae-hq/docs/` + `~/.claude/memory/` never leak to public repos.
3. **Friday ships or Friday explains why** — zero skipped rituals.
4. **Cut before adding** — every new tool/repo must retire something.
5. **Future-Thibaut is the primary user** — docs written for him-in-6-months, not for strangers.

🦋
