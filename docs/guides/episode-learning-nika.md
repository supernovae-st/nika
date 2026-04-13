# Episode 6: Learning AI Workflows -- The 12-Level Liberation Course

## Metadata

| Field | Value |
|-------|-------|
| **Series** | Building Nika -- A Rust AI Engine from Scratch |
| **Episode** | 06 |
| **Duration** | ~25 minutes |
| **Topics** | Course design, progressive disclosure, Liberation theme, showcase workflows, learning paths |
| **Guest Suggestions** | An education technology designer, a developer advocacy lead, a learn-by-doing advocate |
| **Audience** | Developers learning Nika, educators building developer courses, DevRel professionals |
| **Prerequisites** | None (this episode is accessible to newcomers) |

---

## Cold Open (30 seconds)

[MUSIC: Adventure game soundtrack -- discovery and progression]

**Host:** Level 1: Jailbreak. You write your first shell command in a YAML workflow. Level 4: Root Access. You unlock the LLM for the first time. Level 8: Gone Rogue. You create your first autonomous agent. Level 12: SuperNovae. You orchestrate everything -- fetch, infer, invoke, agent -- into a production-grade pipeline.

[PAUSE]

44 exercises. 12 levels. One theme: Liberation. This is how Nika teaches you to build AI workflows -- not with documentation you skim and forget, but with exercises you write, run, break, fix, and master.

[MUSIC FADES]

---

## Intro (1 minute)

**Host:** Episode 6 of "Building Nika." We have spent five episodes talking about what Nika does and how it works. Today, we are talking about how you learn it.

Most developer tools have documentation. Good ones have tutorials. Great ones have interactive courses. Nika ships with a built-in 12-level learning path that generates exercises directly into your project, validates your solutions, gives you progressive hints, and tracks your progress with a constellation map.

And the design philosophy behind this course is worth understanding even if you never use Nika -- because the principles apply to any developer education system.

Let us start with the command.

---

## Segment 1: The Course System -- How It Works (8 minutes)

**Host:** Setting up the course is one command:

[CODE EXAMPLE]
```bash
nika init --course
```

This generates a project structure with 44 exercise files organized into 12 levels:

[CODE EXAMPLE]
```
my-project/
  levels/
    01-jailbreak/
      01-hello-world.nika.yaml
      02-shell-commands.nika.yaml
      03-http-requests.nika.yaml
      04-provider-selection.nika.yaml
      05-validation.nika.yaml
    02-hot-wire/
      01-simple-binding.nika.yaml
      02-nested-json.nika.yaml
      ...
    ...
    12-supernovae/
      01-full-pipeline.nika.yaml
      ...
```

Each exercise file is a template with TODO markers that you need to complete:

[CODE EXAMPLE]
```yaml
# levels/01-jailbreak/01-hello-world.nika.yaml
schema: nika/workflow@0.12

# TODO: Create your first task!
# A task needs:
#   - id: a unique identifier
#   - a verb: exec: to run a shell command
#
# Your goal: create a task that prints "Hello, Nika!" to the terminal
#
# Hint: Use exec: with the echo command.

tasks:
  # TODO: Write your task here
```

You edit the file, then validate your solution:

[CODE EXAMPLE]
```bash
# Check a specific exercise
nika course check 1

# Check a specific level
nika course check --level 1

# Run an exercise to see the output
nika course run levels/01-jailbreak/01-hello-world.nika.yaml

# Get a hint if you are stuck
nika course hint 01-jailbreak/01-hello-world

# Check your overall progress
nika course status

# Open the next uncompleted exercise
nika course next

# Auto-check on file save (watch mode)
nika course watch
```

[EMPHASIS] The course system is not a separate documentation website. It is built into the Nika binary itself, powered by 14 Rust source files totaling over 2,500 lines. The exercises are embedded as static string templates in the binary -- no network request, no file download, no package to install.

**Progress Tracking**

Progress is stored in `.nika/course-progress.toml`:

[CODE EXAMPLE]
```toml
[metadata]
version = 1
started_at = "2026-03-23T10:00:00Z"
last_activity = "2026-03-23T14:30:00Z"
total_hints_used = 7

[levels.1]
status = "completed"
hints_used = 2

[levels.1.exercises]
1 = "passed"
2 = "passed"
3 = "passed"
4 = "passed"
5 = "perfect"  # Solved without hints

[levels.2]
status = "in_progress"
hints_used = 3
```

Four exercise statuses: NotStarted, Attempted, Passed, Perfect (solved without hints). Four level statuses: Locked, Unlocked, InProgress, Completed.

Level 1 starts unlocked. Each subsequent level unlocks when the previous one is completed. Level 12 (SuperNovae) is a boss level that gates the final achievement -- it requires mastery of everything that came before.

**Progressive Hints**

Each exercise has three tiers of hints. If you are stuck, you ask for a hint, and you get a gentle nudge. Ask again, and you get a more specific pointer. Ask a third time, and you get a near-solution walkthrough. This graduated approach teaches problem-solving skills alongside Nika skills.

The trade-off is explicit: using hints means your exercise status is "Passed" instead of "Perfect." There is no punishment -- Perfect is just a bonus for those who want the challenge.

---

## Segment 2: The 12 Levels -- A Liberation Journey (8 minutes)

**Host:** The 12 levels follow a deliberate pedagogical arc. Let me walk you through each one, because the progression tells you a lot about how Nika is designed to be learned.

[EMPHASIS] And notice the naming theme. Every level is named after a liberation concept -- breaking free from limitations, gaining new powers, expanding what you can do.

**Level 1: Jailbreak** (5 exercises)
"Break free from manual commands. Learn exec: and basic workflows."

This is where you learn that a workflow is just a YAML file with tasks. Your first exercise prints "Hello, Nika!" using `exec: "echo Hello, Nika!"`. By exercise 5, you are validating workflows with `nika check`.

[PAUSE] Notice: you do NOT start with LLMs. You start with shell commands. This is a deliberate pedagogical choice -- exec: has no API key requirement, no network dependency, no cost. You can learn the YAML structure in a completely local, free, instant environment.

**Level 2: Hot Wire** (4 exercises)
"Hot-wire the network. Master fetch: for HTTP requests and APIs."

Now you make HTTP requests. GET a public API, parse JSON responses, use headers. Still no LLM -- just fetch and data flow.

**Level 3: Fork Bomb** (4 exercises)
"Multiply your power. DAG patterns, depends_on, and parallel execution."

This level teaches the DAG -- how `depends_on:` creates relationships between tasks, how independent tasks run in parallel, and how data flows between tasks. The name "Fork Bomb" is cheeky -- you are not actually creating fork bombs, you are multiplying your tasks.

**Level 4: Root Access** (3 exercises)
"Unlock the LLM. First infer: prompts with provider setup."

[EMPHASIS] This is the first level that requires an API key. You make your first LLM call. You learn about provider auto-detection, temperature, and system prompts.

**Level 5: Shapeshifter** (3 exercises)
"Transform data with with: bindings and pipe transforms."

The binding system: `with:` blocks, `$task_id` references, `{{with.alias}}` templates, pipe transforms like `| uppercase | trim`. This is where data flow becomes explicit and powerful.

**Level 6: Pay-Per-Dream** (3 exercises)
"Structured output, JSON schemas, and output validation."

The name is perfect -- every LLM call costs money, and structured output makes sure you get exactly the data structure you paid for. You learn JSON schema definitions, the `structured:` block, and how the five-layer validation cascade guarantees valid output.

**Level 7: Swiss Knife** (3 exercises)
"Builtin tools via invoke: -- nika:log, nika:emit, nika:assert."

The `invoke:` verb with built-in tools. Log messages, emit events, assert conditions. This prepares you for MCP tool calls.

**Level 8: Gone Rogue** (3 exercises)
"Autonomous agents with agent:, tools, and stop conditions."

[EMPHASIS] Your first autonomous agent. You define a goal, give it tools, set guardrails, and let it run. This is the level where Nika goes from "workflow engine" to "agentic platform."

**Level 9: Data Heist** (4 exercises)
"Advanced fetch: extraction -- markdown, article, metadata, links."

The nine extraction modes of `fetch:`. You learn to turn raw web pages into structured data that LLMs can work with.

**Level 10: Open Protocol** (3 exercises)
"MCP integration -- invoke: external tools and NovaNet."

External MCP servers. You connect to real services via the Model Context Protocol, use aliases, and learn the invoke: verb's full power.

**Level 11: Pixel Pirate** (4 exercises)
"Media pipeline -- import, thumbnail, vision, CAS workflows."

The media pipeline from Episode 4. Import images, generate thumbnails, use vision models, chain operations with `nika:pipeline`.

**Level 12: SuperNovae** (5 exercises)
"Final boss. Orchestrate everything -- full production workflows."

[EMPHASIS] The boss level. Five exercises that combine everything: multi-provider workflows, structured output feeding into agents, media processing pipelines, MCP integrations, and full DAG orchestration. If you complete Level 12, you can build production-grade AI workflows.

[PAUSE]

**Host:** The total exercise count is 44. Across 12 levels, the progression goes:

```
Levels  1-3: No LLM needed (13 exercises) -- Learn the structure
Levels  4-6: Basic LLM usage (9 exercises) -- Learn the intelligence
Levels  7-9: Tools and extraction (10 exercises) -- Learn the capabilities
Levels 10-12: Integration and mastery (12 exercises) -- Learn the orchestration
```

This four-phase structure means you can learn 30% of Nika without spending a single API token. That is important for accessibility -- not everyone has API keys ready when they start learning.

---

## Segment 3: Showcase Workflows and Learning Philosophy (6 minutes)

**Host:** Beyond the structured course, Nika ships with 200+ showcase workflows. These are complete, runnable examples that demonstrate specific features:

[CODE EXAMPLE]
```bash
# Browse available showcases
nika showcase list

# Extract a showcase to your current directory
nika showcase extract content-pipeline
nika showcase extract vision-analysis
nika showcase extract multi-agent-research
```

The showcases are organized by category:
- **exec** -- Shell command patterns, data processing
- **builtin** -- Built-in tool demonstrations
- **llm** -- LLM-powered workflows with various providers

Each showcase is a complete `.nika.yaml` file that you can run immediately. They serve as both documentation and starting points -- you extract a showcase, modify it, and build from there.

The showcase system is powered by four generator modules in the engine: `showcase.rs` for the core framework, `showcase_exec.rs` for shell command patterns, `showcase_builtin.rs` for built-in tool demonstrations, and `showcase_llm.rs` for LLM-powered workflows. Each generator produces valid, runnable YAML with comments explaining every feature used. This means the showcases are not just static examples -- they are generated from Rust code that stays in sync with the engine's capabilities as features are added.

**The Showcase Workflows**

For developers who want to learn by example, Nika includes 115 showcase workflows:

[CODE EXAMPLE]
```bash
nika showcase list
# Browse and extract from 115 ready-to-run workflows
nika showcase extract vision-analysis
nika showcase extract multi-agent-research
```

Learn by examining and running complete production patterns.

**The Interactive Wizard**

`nika init` without flags runs an interactive wizard that asks about your project and generates an appropriate scaffold. This is progressive disclosure in action -- beginners get the course, intermediate users get the minimal scaffold, advanced users get the wizard.

[PAUSE]

**Host:** Let me talk about the learning philosophy behind all of this, because I think it is worth understanding even if you never use Nika.

**Principle 1: Start without the expensive part.**

The course starts with `exec:` and `fetch:` -- free, local, instant feedback. You do not need an API key to learn how workflows, DAGs, and bindings work. The LLM comes in Level 4, after you already understand the structure. This removes the "I cannot start because I do not have an API key" barrier.

**Principle 2: Every exercise is runnable.**

No pseudo-code. No hypothetical examples. Every exercise template is valid YAML with TODO markers. Every solution passes `nika check`. You can run, modify, break, and fix every exercise.

**Principle 3: Progressive hints, not immediate answers.**

Three tiers of hints teach you to think through problems. The first hint is a conceptual nudge. The second is a structural pointer. The third is nearly the answer. This models how a patient mentor would help -- they do not give you the answer immediately.

**Principle 4: Liberation, not certification.**

The theme is not "achievement" or "mastery." It is "liberation." Each level frees you from a limitation -- manual commands, static data, text-only workflows, isolated tasks. The emotional arc is empowerment, not assessment.

**Principle 5: The tool teaches the tool.**

The course, the showcases, the hints, the progress tracking -- they are all built into the Nika binary. There is no external dependency. `nika init --course` works offline, on any machine, with no account required. The tool you are learning is the tool you are using to learn.

---

## Wrap-up & Preview (2 minutes)

**Host:** Nika's learning system is 44 exercises across 12 levels with a Liberation theme, plus 115 showcase workflows and the course system built into the binary.

The pedagogical arc goes from free local commands to paid LLM calls to autonomous agents to full production orchestration. Progress is tracked in TOML, hints are progressive, and every exercise is runnable.

The key insight is that learning developer tools should follow the same design principles as the tools themselves: start simple, compose complexity, and never require more than you need.

[PAUSE]

Next episode: the brain meets the body. How Nika talks to NovaNet via the Model Context Protocol, what the Zero Cypher rule means, and how a knowledge graph gives AI workflows long-term memory. Episode 7: "The Brain and The Body."

[MUSIC: Outro theme]

---

## Show Notes

### Course Commands
| Command | Description |
|---------|-------------|
| `nika init --course` | Generate 12-level course (44 exercises) |
| `nika init --minimal` | Minimal scaffold (5 workflows, 1 per verb) |
| `nika init` | Interactive project setup wizard |
| `nika course status` | Show constellation progress map |
| `nika course next` | Open next uncompleted exercise |
| `nika course check [level]` | Validate exercises |
| `nika course hint [exercise]` | Progressive hints (3 tiers) |
| `nika course run <exercise>` | Run a course exercise |
| `nika course info [level]` | Show course/level details |
| `nika course reset <level>` | Reset a level |
| `nika course watch` | Auto-check on file save |

### The 12 Levels
| # | Name | Exercises | Focus |
|---|------|-----------|-------|
| 1 | Jailbreak | 5 | exec: basics, YAML structure |
| 2 | Hot Wire | 4 | fetch: HTTP requests |
| 3 | Fork Bomb | 4 | DAG, depends_on, parallelism |
| 4 | Root Access | 3 | infer: first LLM call |
| 5 | Shapeshifter | 3 | with: bindings, transforms |
| 6 | Pay-Per-Dream | 3 | Structured output, schemas |
| 7 | Swiss Knife | 3 | invoke: builtin tools |
| 8 | Gone Rogue | 3 | agent: autonomous loops |
| 9 | Data Heist | 4 | Advanced fetch extraction |
| 10 | Open Protocol | 3 | MCP integration |
| 11 | Pixel Pirate | 4 | Media pipeline |
| 12 | SuperNovae | 5 | Boss level -- full orchestration |

### Learning Design Principles
1. Start without the expensive part (no API keys for Levels 1-3)
2. Every exercise is runnable (no pseudo-code)
3. Progressive hints (3 tiers, not immediate answers)
4. Liberation theme (empowerment, not assessment)
5. The tool teaches the tool (built into the binary)

### Source Files
- Course module: `tools/nika-engine/src/init/course/`
- Levels: `levels.rs` (12 level definitions)
- Exercises: `exercises.rs` + `exercises_advanced.rs` (44 templates + solutions)
- Progress: `progress.rs` (TOML persistence)
- Hints: `hints.rs` (3-tier progressive hints)
- Checks: `checks.rs` (solution validation)
- Showcase: `showcase.rs` + `showcase_*.rs` (200+ workflows)
