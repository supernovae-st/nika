# The Nika Course -- Interactive Learning

Nika includes a built-in interactive course with 12 progressive levels and 44 hands-on exercises. The course teaches you every aspect of Nika workflows through practice, starting with simple shell commands and building up to full production pipelines.

## Getting Started

### Generating the Course

```bash
mkdir my-nika-course
cd my-nika-course
nika init --course
```

This creates the full course structure with exercise files, progress tracking, and configuration.

### Course Structure

After initialization, your project looks like:

```
my-nika-course/
├── .nika/
│   ├── config.toml
│   └── course-progress.toml      # Your progress tracker
├── 01-jailbreak/
│   ├── exercise-01.nika.yaml
│   ├── exercise-02.nika.yaml
│   └── ...
├── 02-hot-wire/
│   ├── exercise-01.nika.yaml
│   └── ...
├── ...
└── 12-supernovae/
    ├── exercise-01.nika.yaml
    └── ...
```

Each level is a directory containing exercise files. Exercises are workflow files with TODO comments that you need to complete.

## The 12 Levels

The course uses a "Liberation" theme -- each level name represents a step toward mastering workflow automation.

| Level | Name | Focus | Exercises |
|-------|------|-------|:---------:|
| 1 | **Jailbreak** | Break free from manual commands. Learn `exec:` and basic workflows. | 5 |
| 2 | **Hot Wire** | Hot-wire the network. Master `fetch:` for HTTP requests and APIs. | 4 |
| 3 | **Fork Bomb** | Multiply your power. DAG patterns, `depends_on`, parallel execution. | 4 |
| 4 | **Root Access** | Unlock the LLM. First `infer:` prompts with provider setup. | 3 |
| 5 | **Shapeshifter** | Transform data with `with:` bindings and pipe transforms. | 3 |
| 6 | **Pay-Per-Dream** | Structured output, JSON schemas, and output validation. | 3 |
| 7 | **Swiss Knife** | Builtin tools via `invoke:` -- `nika:log`, `nika:emit`, `nika:assert`. | 3 |
| 8 | **Gone Rogue** | Autonomous agents with `agent:`, tools, and stop conditions. | 3 |
| 9 | **Data Heist** | Advanced `fetch:` extraction -- markdown, article, metadata, links. | 4 |
| 10 | **Open Protocol** | MCP integration -- `invoke:` external tools and servers. | 3 |
| 11 | **Pixel Pirate** | Media pipeline -- import, thumbnail, vision, CAS workflows. | 4 |
| 12 | **SuperNovae** | Final boss. Orchestrate everything -- full production workflows. | 5 |

**Total: 44 exercises across 12 levels.**

### Level Progression

- Levels 1-3 require no API keys (pure `exec:` and `fetch:`)
- Level 4 introduces `infer:` (requires at least one LLM API key)
- Level 12 (SuperNovae) is a boss level that requires mastery of all previous levels

## Course Commands

### Check Your Progress

```bash
nika course status
```

This shows a constellation progress map with completed, in-progress, and locked levels:

```
  Nika Course -- Liberation Path

  ★ Jailbreak          5/5 ████████████████████ COMPLETE
  ★ Hot Wire           3/4 ███████████████░░░░░ 75%
  ○ Fork Bomb          0/4 ░░░░░░░░░░░░░░░░░░░ LOCKED
  ○ Root Access        0/3 ░░░░░░░░░░░░░░░░░░░ LOCKED
  ...

  Progress: 8/44 exercises (18%)
```

### Find Your Next Exercise

```bash
nika course next
```

Shows the next exercise to work on and opens it in your editor.

### Check an Exercise

Validate that your solution is correct:

```bash
# Check a specific exercise
nika course check 2     # Check all exercises in level 2

# Run a specific exercise
nika course run 2-3     # Run exercise 3 of level 2
```

The checker validates:
- YAML syntax is correct
- Required schema version is present
- The correct verb is used
- Task dependencies are valid
- TODO comments are replaced with actual implementations
- Bindings work correctly

### Get Hints

Stuck? Get progressive hints (3 tiers):

```bash
nika course hint 2-3
```

Hints are progressive:
1. **Tier 1** -- A gentle nudge in the right direction
2. **Tier 2** -- More specific guidance with key concepts
3. **Tier 3** -- Nearly the solution, with only small gaps

Each time you run `hint`, it reveals the next tier.

### Run an Exercise

Execute your solution to see the output:

```bash
nika course run 2-3
```

This runs the exercise workflow and shows the output, helping you verify that your implementation works correctly.

### Get Level Info

See details about a specific level or the whole course:

```bash
# Overview of the whole course
nika course info

# Details about level 3
nika course info 3
nika course info fork-bomb     # By slug
nika course info "Fork Bomb"   # By name
```

### Reset a Level

Start a level over from scratch:

```bash
nika course reset 2
```

This regenerates the exercise files for that level, resetting your solutions.

### Watch Mode

Auto-check exercises when you save:

```bash
nika course watch
```

This watches exercise files for changes and automatically runs validation when you save. Ideal for a workflow where you edit in one terminal and watch results in another.

## Working Through an Exercise

Here is a typical exercise workflow:

### 1. Open the exercise

```bash
nika course next
```

You see something like:

```
  Next Exercise: 01-03 (Jailbreak, Exercise 3)

  Goal: Create a workflow that captures system info and formats it

  File: 01-jailbreak/exercise-03.nika.yaml
```

### 2. Read the exercise file

Open the file. You will find a skeleton with TODO comments:

```yaml
schema: nika/workflow@0.12
workflow: system-info

tasks:
  # TODO: Create a task that captures the hostname
  - id: hostname
    exec: # YOUR CODE HERE

  # TODO: Create a task that captures the current date
  - id: current_date
    exec: # YOUR CODE HERE

  # TODO: Create a task that combines both outputs
  - id: report
    depends_on: [hostname, current_date]
    # TODO: Add with: bindings for hostname and current_date
    exec: # YOUR CODE HERE - format a report string
```

### 3. Write your solution

Replace the TODOs with actual implementations:

```yaml
schema: nika/workflow@0.12
workflow: system-info

tasks:
  - id: hostname
    exec: "hostname"

  - id: current_date
    exec: "date '+%Y-%m-%d'"

  - id: report
    depends_on: [hostname, current_date]
    with:
      host: $hostname | trim
      date: $current_date | trim
    exec: "echo 'Host: {{with.host}} | Date: {{with.date}}'"
```

### 4. Check your solution

```bash
nika course check 1
```

```
  Level 1: Jailbreak
  ─────────────────────

  ✓ Exercise 01: Basic echo          PASS
  ✓ Exercise 02: Multi-step chain    PASS
  ✓ Exercise 03: System info         PASS
  ✗ Exercise 04: Environment vars    TODO
  ✗ Exercise 05: JSON output         TODO

  Progress: 3/5
```

### 5. Run it to verify

```bash
nika course run 1-3
```

```
  ✓ hostname ─── 0.01s
    MacBook-Pro.local

  ✓ current_date ─── 0.01s
    2026-03-23

  ✓ report ─── 0.01s
    Host: MacBook-Pro.local | Date: 2026-03-23
```

## Level-by-Level Guide

### Level 1: Jailbreak (5 exercises)

Your entry point. No API keys needed.

- **01-01**: Write a basic `exec:` task
- **01-02**: Chain two tasks with `depends_on:`
- **01-03**: Pass data between tasks with `with:` bindings
- **01-04**: Use environment variables in commands
- **01-05**: Produce JSON output with `output: { format: json }`

### Level 2: Hot Wire (4 exercises)

Master HTTP requests. No API keys needed for public APIs.

- **02-01**: Basic GET request with `fetch:`
- **02-02**: POST request with JSON body
- **02-03**: Extract markdown from a webpage
- **02-04**: Parse JSON API response with JSONPath

### Level 3: Fork Bomb (4 exercises)

DAG mastery. Learn parallel execution patterns.

- **03-01**: Diamond pattern (fan-out and merge)
- **03-02**: Wide parallel execution (5+ concurrent tasks)
- **03-03**: `for_each` iteration over arrays
- **03-04**: Complex dependency chains (10+ tasks)

### Level 4: Root Access (3 exercises)

Your first LLM calls. Requires an API key.

- **04-01**: Basic `infer:` with a prompt
- **04-02**: System prompts and temperature control
- **04-03**: Chain fetch and infer (web + AI)

### Level 5: Shapeshifter (3 exercises)

Data transformation deep dive.

- **05-01**: String transforms (`trim`, `upper`, `lower`)
- **05-02**: Collection transforms (`sort`, `unique`, `first`)
- **05-03**: Complex transform chains and `default()`

### Level 6: Pay-Per-Dream (3 exercises)

Structured output and JSON schemas.

- **06-01**: Basic structured output with inline schema
- **06-02**: Schema file references
- **06-03**: Output validation with retries

### Level 7: Swiss Knife (3 exercises)

Builtin tools via `invoke:`.

- **07-01**: `nika:log` and `nika:emit`
- **07-02**: `nika:assert` for validation
- **07-03**: Multiple builtin tools in one workflow

### Level 8: Gone Rogue (3 exercises)

Agent loops.

- **08-01**: Basic agent with max_turns
- **08-02**: Agent with tools and stop conditions
- **08-03**: Agent guardrails (length, regex)

### Level 9: Data Heist (4 exercises)

Advanced fetch extraction.

- **09-01**: Article extraction (Readability)
- **09-02**: Metadata and link extraction
- **09-03**: RSS feed parsing
- **09-04**: Multi-source data aggregation

### Level 10: Open Protocol (3 exercises)

MCP integration.

- **10-01**: Configure an MCP server
- **10-02**: Call external tools via `invoke:`
- **10-03**: Agent with MCP tools

### Level 11: Pixel Pirate (4 exercises)

Media pipeline.

- **11-01**: Import and get dimensions
- **11-02**: Thumbnail generation and format conversion
- **11-03**: Vision (image + LLM)
- **11-04**: Pipeline chaining

### Level 12: SuperNovae (5 exercises)

The final boss. Orchestrate everything.

- **12-01**: Multi-provider comparison workflow
- **12-02**: Full data pipeline (fetch + process + analyze + report)
- **12-03**: Content production pipeline with artifacts
- **12-04**: Agent-driven research with media
- **12-05**: Production-ready workflow with retries, guardrails, and structured output

## Tips for Success

1. **Start with Level 1** even if you are experienced -- the early exercises establish patterns used everywhere
2. **Use `nika course hint`** freely -- hints are designed to teach, not just give answers
3. **Run exercises after completing them** to see actual output
4. **Read error messages carefully** -- NIKA-XXX codes are descriptive and include fix suggestions
5. **Levels 1-3 need no API keys** -- perfect for getting started without any accounts
6. **Use `nika course watch`** for rapid iteration -- edit and see results instantly
