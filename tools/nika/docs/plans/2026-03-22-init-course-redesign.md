# nika init + nika course — Complete Redesign

**Date**: 2026-03-22
**Status**: Approved
**Scope**: Replace `nika init` (30 wf / 6 tiers) with minimal scaffold + interactive course

## Decision Summary

| Aspect | Old | New |
|--------|-----|-----|
| `nika init` | 30 workflows, 6 tiers, overwhelming | cliclack wizard → minimal scaffold (5 wf) |
| `nika init --course` | N/A | Generates 12-level course (~44 wf) at DEST |
| `nika course *` | N/A | Interactive: status, next, check, hint, reset, run, info |
| Templates | 365 KB in 6 tier files | Minimal (5 wf) + Course module (44 wf) |
| Provider | Static | Auto-detected, workflows adapted |
| Progress | None | .nika/course-progress.toml + scoring |
| Theme | N/A | "Liberation" — hacker/freedom/pirate, customizable |
| Wizard | None | cliclack guide-rail pattern (like create-astro) |

## Architecture

### CLI Commands

```
nika init                       cliclack wizard (provider detect → mode select → generate)
nika init --minimal             Scaffold only (.nika/ + 5 examples)
nika init --course [DEST]       Generate course at DEST (default: ./nika-course/)
nika init --yes                 Non-interactive, all defaults

nika course status              Constellation map + progression
nika course next                Open next incomplete exercise
nika course check [LEVEL]       Validate exercises (nika check + assertions)
nika course hint [EXERCISE]     Progressive hints (1/3 → 2/3 → solution)
nika course reset [LEVEL]       Reset a level
nika course run [EXERCISE]      Execute an exercise
nika course info [LEVEL]        Display MISSION.md
nika course watch               Auto-detect changes, re-validate (rustlings-style)
```

### Rust Module Structure

```
nika-engine/src/
├── init/
│   ├── mod.rs              Refactored: generate_minimal(), generate_course()
│   ├── minimal.rs          5 workflows (1/verb), config.toml, policies
│   ├── course/
│   │   ├── mod.rs          CourseGenerator, CourseConfig, CourseTheme
│   │   ├── levels.rs       12 levels (YAML embedded as const &str)
│   │   ├── missions.rs     MISSION.md per level (Markdown embedded)
│   │   ├── hints.rs        3 hints per exercise
│   │   ├── checks.rs       Assertions per exercise (CourseCheck trait)
│   │   └── progress.rs     CourseProgress (serde TOML)
│   ├── context.rs          Context files (reused)
│   └── schemas.rs          JSON schemas (reused)

nika-cli/src/
├── init.rs                 Refactored: --minimal / --course / wizard dispatch
└── course.rs               NEW: status, next, check, hint, reset, run, info, watch

nika/src/main.rs
└── Commands::Course        NEW: CourseSubcommand enum

Cargo.toml (nika binary)
└── cliclack = "0.x"       NEW: wizard UI framework
```

### DELETED (old init)

- `tier1.rs` through `tier6.rs` (30 workflows, ~300 KB)
- `partials.rs` (5 partial workflows)
- Old `init()` function in nika-cli

### Kept

- `context.rs` (5 context files)
- `schemas.rs` (6 JSON schemas)
- `nika new` command (15 templates + wizard) — unchanged

## The 12 Levels — "Liberation" Theme

Each level = you free a new capability. Names are ballsy, mocking, powerful.

| # | Codename | Tagline | WF | Features |
|---|----------|---------|----|----|
| 01 | **Jailbreak** | "They said AI was for them. You just broke out." | 5 | exec, fetch, infer, providers, nika check |
| 02 | **Hot Wire** | "Data flows where you tell it. Not where they sell it." | 4 | with: bindings, transforms (27), ??, $env |
| 03 | **Fork Bomb** | "One task? Cute. Try a thousand." | 4 | DAG patterns, for_each, concurrency, fail_fast |
| 04 | **Root Access** | "Their walled gardens? Your open fields." | 3 | context, imports (DAG fusion), inputs |
| 05 | **Shapeshifter** | "Chaos is just structure that hasn't met you yet." | 3 | structured output, JSON Schema, artifacts |
| 06 | **Pay-Per-Dream** | "7 providers. 0 lock-in. Their worst nightmare." | 3 | 7 cloud + native GGUF/Vision, extended_thinking |
| 07 | **Swiss Knife** | "12 tools. No subscription. No terms of service." | 3 | 12 builtin tools, 5 file tools, nika:run |
| 08 | **Gone Rogue** | "You don't run prompts anymore. Your agents do." | 3 | agent verb, skills, guardrails, limits |
| 09 | **Data Heist** | "The web is a buffet. You just got a plate." | 4 | 9 fetch extract modes, response modes, retry |
| 10 | **Open Protocol** | "They built walls. You built bridges." | 3 | MCP protocol, invoke, 113 aliases, NovaNet |
| 11 | **Pixel Pirate** | "Every pixel they locked up? Yours now." | 4 | Media pipeline (26 tools), CAS, vision |
| 12 | **SuperNovae** | "You are the SuperNovae. Ship it." | 5 | BOSS — everything combined |
| | | **Total** | **44** | **100% of v0.38 features** |

### Theme System (future)

```
nika init --course --theme liberation   (default — hacker/freedom)
nika init --course --theme onepiece     (East Blue → Laugh Tale)
nika init --course --theme minimal      (Level 1, Level 2, Level 3...)
nika init --course --theme custom       (names from config)
```

### Constellation Map (nika course status)

```
🦋 Nika Course — Your Liberation Journey

  ★ Jailbreak ━━ ✦ Hot Wire ━━ ✦ Fork Bomb
                                    ╲
                                ✦ Root Access
                                    ╲
      ✦ Pay-Per-Dream ━━ ✦ Shapeshifter
         ╲
      ✦ Swiss Knife ━━ ✦ Gone Rogue
                            ╲
          ✦ Data Heist ━━ ✦ Open Protocol
                                ╲
                            ✦ Pixel Pirate
                                ╲
                             ☆ SUPERNOVAE

  ★ completed  ✦ unlocked  ○ locked  ☆ boss

  Progress: 1/12 levels  |  5/44 exercises  |  Score: 95
  Next: nika course next
```

## Interactive Experience

### Init Wizard (cliclack)

Uses cliclack crate (Rust port of @clack/prompts) for the guide-rail pattern:

```
 nika   v0.38.0

|
*  Welcome! Let's set you up.
|
*  Detected providers:
|  ✓ Claude (ANTHROPIC_API_KEY)
|  ✓ OpenAI (OPENAI_API_KEY)
|  ✗ Groq (not set)
|
*  What do you want to do?
|  > 🏴 Start a project
|    🦋 Learn Nika (interactive course)
|    ⚡ Quick scaffold
|
*  Permission mode?
|  > Plan (review before execute)
|
◆  Setting up... done (0.3s)
|
 done   Created 12 files. Ready!

  Next steps:
  cd my-project && nika run workflows/01-hello.nika.yaml
```

### Progress Tracking (.nika/course-progress.toml)

```toml
[course]
started_at = "2026-03-22T14:30:00Z"
provider = "claude"
model = "claude-sonnet-4-6"
theme = "liberation"
current_level = 3

[levels.jailbreak]
status = "completed"
completed_at = "2026-03-22T15:00:00Z"
exercises = { "01" = true, "02" = true, "03" = true, "04" = true, "05" = true }
hints_used = 1
score = 95

[levels.hot-wire]
status = "completed"
exercises = { "01" = true, "02" = true, "03" = true, "04" = true }
hints_used = 0
score = 100

[levels.fork-bomb]
status = "in_progress"
exercises = { "01" = true, "02" = false, "03" = false, "04" = false }
```

### Exercise Model (inspired by rustlings + exercism)

Each exercise has two forms:
- `XX-name.nika.yaml` — Exercise with TODOs and comments guiding the user
- `.solutions/XX-name.nika.yaml` — Hidden complete solution

Exercise files contain `# TODO:` markers:
```yaml
tasks:
  - id: greet
    # TODO: Add an infer: verb that generates a greeting
    # Hint: use prompt: with a template
```

### Check System

`nika course check [LEVEL]` runs assertions:
1. `nika check` (schema validation, DAG acyclic, bindings resolve)
2. Custom assertions per exercise (CourseCheck trait):
   - Has specific verb type
   - Has depends_on edges
   - Has for_each with concurrency
   - Uses specific transforms
   - Produces valid JSON output schema
3. Bonus checks for elegance (optional, awards extra stars)

### Hint System

3 progressive hints per exercise (never penalized — tracked as bonus):
1. **Conceptual** — "This exercise needs a for_each to iterate over items"
2. **Specific** — "Add `for_each:` with `as: item` and `concurrency: 3`"
3. **Solution** — Full YAML reveal

### Scoring (research-backed: score outcome, not process)

- ⭐ Correctness: all checks pass (base requirement)
- ⭐ Elegance: uses idiomatic features (transforms, proper bindings)
- ⭐ Bonus: first-try + no-hints (additive, never penalizing)
- Level score = stars earned / stars possible × 100
- Hints are FREE — "solved without hints" is a bonus star, not a penalty

### Watch Mode (rustlings-style)

`nika course watch` — monitors exercise files, auto-runs check on save:
```
🦋 Watching for changes...

  Level 03: Fork Bomb
  Exercise: 02-for-each-basic.nika.yaml

  ✗ Missing for_each: directive
  ✗ Missing concurrency: control

  Hint: nika course hint 03-02

  [Ctrl+C to exit]
```

## Execution Plan

### Phase 1: Foundations (1 session)
- [ ] Delete tier1.rs → tier6.rs, partials.rs (~300 KB removed)
- [ ] Create init/minimal.rs (5 workflows, 1 per verb)
- [ ] Refactor init.rs CLI (--minimal + --course + --yes flags)
- [ ] Add `cliclack` dependency to nika binary
- [ ] Add Commands::Course in main.rs (stub handlers)
- [ ] Tests: nika init --minimal works, old init tests updated

### Phase 2: Course Engine (1-2 sessions)
- [ ] Create init/course/ module structure
- [ ] CourseProgress struct (serde TOML read/write)
- [ ] CourseGenerator (generate 12 level directories)
- [ ] CourseTheme enum (Liberation/OnePiece/Minimal)
- [ ] Auto-detection + {{COURSE_PROVIDER}} placeholder substitution
- [ ] course.rs in nika-cli (status, next, check, hint, reset, info)
- [ ] CourseCheck trait + basic assertion framework
- [ ] Tests: generate + status + progress tracking

### Phase 3: Content Levels 1-6 (1-2 sessions)
- [ ] Adapt prototype nika-test-034 levels 1-6 with new names
- [ ] Write MISSION.md per level (liberation tone, bilingual)
- [ ] Create exercise versions (incomplete, with TODO markers)
- [ ] Create .solutions/ per level
- [ ] Write hints (3 per exercise, 3 levels of specificity)
- [ ] Write CourseCheck assertions per exercise

### Phase 4: Content Levels 7-12 (1-2 sessions)
- [ ] Levels 7-10: adapt + enrich from prototype
- [ ] Level 11 (Pixel Pirate): NEW, 4 media workflows from scratch
- [ ] Level 12 (SuperNovae): 5 BOSS workflows combining everything
- [ ] Tests: all 44 workflows pass nika check

### Phase 5: Wizard + Watch (1 session)
- [ ] cliclack wizard flow (Welcome → Detect → Mode → Permission → Generate)
- [ ] Provider auto-detection with colored status
- [ ] nika course watch (file watcher + auto-check)
- [ ] Integration with nika init dispatch

### Phase 6: Polish (1 session)
- [ ] Constellation map rendering (colored, responsive to terminal width)
- [ ] Scoring system (3-star model)
- [ ] nika course run integration (execute + track progress)
- [ ] nika course info (MISSION.md display with styled output)
- [ ] Complete test suite + documentation
- [ ] Update CLAUDE.md with new commands

**Total: 6-9 sessions**

## Research Sources

- **rustlings**: watch mode, fill-in-the-blanks, `I AM NOT DONE` markers
- **exercism**: concept vs practice exercises, track progression, mentoring
- **cliclack**: guide-rail pattern, branded intro/outro, spinners (Rust)
- **create-astro**: gold standard TUI wizard, animated mascot, smart defaults
- **Charm.sh**: bubbletea/lipgloss, color downsampling, form pages
- **Gamification research**: earned capability unlocks, composite scoring, constellation maps
- **Branding research**: butterfly metamorphosis lifecycle, liberation narrative, Nika symbolism

## Source of Truth

- Prototype: `/Users/thibaut/Desktop/nika-test-034/course/` (10 levels, 38 wf)
- Current init: `nika-engine/src/init/` (30 wf, 6 tiers — TO DELETE)
- Current new: `nika-engine/src/new/` (15 templates — KEEP)
- Research: `docs/research/2026-03-22-tui-wizard-research.md`
- Research: `docs/research-liberation-branding-course-levels.md`
