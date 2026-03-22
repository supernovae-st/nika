# nika init + nika course — Complete Redesign

**Date**: 2026-03-22
**Status**: Approved
**Scope**: Replace `nika init` (30 wf / 6 tiers) with minimal scaffold + interactive course

## Decision Summary

| Aspect | Old | New |
|--------|-----|-----|
| `nika init` | 30 workflows, 6 tiers, overwhelming | Wizard TUI → minimal scaffold (5 wf) |
| `nika init --course` | N/A | Generates 12-level One Piece course (~44 wf) |
| `nika course *` | N/A | Interactive: status, next, check, hint, reset, run, info |
| Templates | 365 KB in 6 tier files | Minimal (5 wf) + Course module (44 wf) |
| Provider | Static | Auto-detected, workflows adapted |
| Progress | None | .nika/course-progress.toml + scoring |

## Architecture

### CLI Commands

```
nika init                       TUI wizard (provider detect → mode select → generate)
nika init --minimal             Scaffold only (.nika/ + 5 examples)
nika init --course [DEST]       Generate course at DEST (default: ./nika-course/)

nika course status              ASCII art One Piece map + progression
nika course next                Open next incomplete exercise
nika course check [LEVEL]       Validate exercises (nika check + assertions)
nika course hint [EXERCISE]     Progressive hints (1/3 → 2/3 → solution)
nika course reset [LEVEL]       Reset a level
nika course run [EXERCISE]      Execute an exercise
nika course info [LEVEL]        Display MISSION.md
```

### Rust Module Structure

```
nika-engine/src/
├── init/
│   ├── mod.rs              Refactored: generate_minimal(), generate_course()
│   ├── minimal.rs          5 workflows (1/verb), config.toml, policies
│   ├── course/
│   │   ├── mod.rs          CourseGenerator, CourseConfig
│   │   ├── levels.rs       12 levels (YAML embedded as const &str)
│   │   ├── missions.rs     MISSION.md per level (Markdown embedded)
│   │   ├── hints.rs        3 hints per exercise
│   │   ├── checks.rs       Assertions per exercise (CourseCheck trait)
│   │   └── progress.rs     CourseProgress (serde TOML)
│   ├── context.rs          Context files (reused)
│   └── schemas.rs          JSON schemas (reused)

nika-cli/src/
├── init.rs                 Refactored: --minimal / --course / wizard dispatch
└── course.rs               NEW: status, next, check, hint, reset, run, info

nika-tui/src/
└── init_wizard/            NEW: TUI wizard for nika init
    ├── mod.rs              WizardApp, WizardState
    ├── views.rs            Welcome, ProviderDetect, ModeSelect, Confirm
    └── render.rs           Styled rendering with colors

nika/src/main.rs
└── Commands::Course        NEW: CourseSubcommand enum
```

### DELETED (old init)

- `tier1.rs` through `tier6.rs` (30 workflows)
- `partials.rs` (5 partial workflows)
- Old `init()` function

### Kept

- `context.rs` (5 context files)
- `schemas.rs` (6 JSON schemas)
- `nika new` command (15 templates + wizard) — unchanged

## The 12 Levels

| # | Arc | WF | Features |
|---|-----|----|----|
| 01 | East Blue | 5 | exec, fetch, infer, providers, nika check |
| 02 | Baratie | 4 | with: bindings, transforms (27), ??, $env |
| 03 | Arlong Park | 4 | DAG patterns, for_each, concurrency, fail_fast |
| 04 | Grand Line | 3 | context, imports (DAG fusion), inputs |
| 05 | Alabasta | 3 | structured output, JSON Schema, artifacts (4 modes) |
| 06 | Skypiea | 3 | 7 cloud providers, native GGUF/Vision, extended_thinking |
| 07 | Water 7 | 3 | 12 builtin tools, 5 file tools, nika:run |
| 08 | Enies Lobby | 3 | agent verb, skills, guardrails, limits, completion modes |
| 09 | Thriller Bark | 4 | 9 fetch extract modes, response modes, retry |
| 10 | Sabaody | 3 | MCP protocol, invoke, 113 aliases, NovaNet |
| 11 | New World | 4 | Media pipeline (26 tools), CAS, vision, pipeline chains |
| 12 | Laugh Tale | 5 | BOSS: SEO Audit, Image Pipeline, Content Factory, Research Agent, Full Stack |
| | **Total** | **44** | **100% of v0.38 features** |

## Interactive Experience

### Progress Tracking (.nika/course-progress.toml)

```toml
[course]
started_at = "2026-03-22T14:30:00Z"
provider = "claude"
current_level = 3

[levels.east-blue]
status = "completed"
completed_at = "2026-03-22T15:00:00Z"
exercises = { "01" = true, "02" = true, "03" = true, "04" = true, "05" = true }
hints_used = 1
score = 95
```

### Exercise Model

Each exercise has two forms:
- `XX-name.nika.yaml` — Exercise (incomplete, user fills in)
- `.solutions/XX-name.nika.yaml` — Hidden solution

Check validates behavior (assertions), not exact code match.

### Hint System

3 progressive hints per exercise:
1. Conceptual hint (what to do)
2. Specific hint (which field/syntax)
3. Solution reveal (exact YAML)

### Scoring

- Base: 100 points per exercise
- -10 per hint used
- -5 per check failure before pass
- Level score = average of exercise scores

## TUI Wizard (nika init)

Full ratatui view with:
- Color scheme matching nika-tui theme
- Auto-detected providers with status icons
- Mode selection: Project / Learn / Minimal
- Animated transitions
- Provider recommendations
- Preview of what will be generated

## Execution Plan

### Phase 1: Foundations (1 session)
- Delete tier1.rs → tier6.rs, partials.rs
- Create init/minimal.rs (5 workflows)
- Refactor init.rs CLI (--minimal + --course flags)
- Add Commands::Course in main.rs (stub)
- Tests: nika init --minimal works

### Phase 2: Course Engine (1-2 sessions)
- Create init/course/ module
- CourseProgress (serde TOML)
- CourseGenerator (12 levels)
- Auto-detection + placeholder substitution
- course.rs in nika-cli (status, next, check, hint, reset)
- Tests: generate + status + progress

### Phase 3: Content Levels 1-6 (1-2 sessions)
- Adapt prototype nika-test-034 levels 1-6
- MISSION.md bilingual per level
- Create exercises (incomplete versions) + solutions
- Write hints (3 per exercise) + checks

### Phase 4: Content Levels 7-12 (1-2 sessions)
- Levels 7-10: adapt + enrich from prototype
- Level 11 (media): NEW, 4 workflows from scratch
- Level 12 (boss): 5 workflows + "Full Stack"
- Tests: all workflows pass nika check

### Phase 5: TUI Wizard (1 session)
- init_wizard/ in nika-tui
- Welcome → ProviderDetect → ModeSelect → Confirm
- Colors, animations, styled rendering
- Integration with nika init dispatch

### Phase 6: Polish (1 session)
- ASCII art status map
- Scoring system refinement
- nika course run integration
- nika course info (MISSION.md display)
- Complete test suite + documentation

**Total: 6-9 sessions**

## Source of Truth

- Prototype: `/Users/thibaut/Desktop/nika-test-034/course/` (10 levels, 38 wf)
- Current init: `nika-engine/src/init/` (30 wf, 6 tiers — TO DELETE)
- Current new: `nika-engine/src/new/` (15 templates — KEEP)
