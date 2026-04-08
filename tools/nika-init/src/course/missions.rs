// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Level-specific MISSION.md content for the Nika course.
//!
//! Each level gets a rich, themed briefing document with ASCII art,
//! concept explanations, exercise tables, and unlock descriptions.
//! Tone: hacker manifesto, not a textbook.

/// Return the MISSION.md content for a given level slug.
///
/// Falls back to a minimal stub if the slug is unknown (should never happen
/// since levels are compile-time constants).
pub fn get_mission(slug: &str) -> &'static str {
    match slug {
        "jailbreak" => MISSION_01_JAILBREAK,
        "hot-wire" => MISSION_02_HOT_WIRE,
        "fork-bomb" => MISSION_03_FORK_BOMB,
        "root-access" => MISSION_04_ROOT_ACCESS,
        "shapeshifter" => MISSION_05_SHAPESHIFTER,
        "pay-per-dream" => MISSION_06_PAY_PER_DREAM,
        "swiss-knife" => MISSION_07_SWISS_KNIFE,
        "gone-rogue" => MISSION_08_GONE_ROGUE,
        "data-heist" => MISSION_09_DATA_HEIST,
        "open-protocol" => MISSION_10_OPEN_PROTOCOL,
        "pixel-pirate" => MISSION_11_PIXEL_PIRATE,
        "supernovae" => MISSION_12_SUPERNOVAE,
        _ => "# Unknown Level\n\nThis level does not exist.\n",
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// LEVEL 01: JAILBREAK
// ═══════════════════════════════════════════════════════════════════════════════

static MISSION_01_JAILBREAK: &str = r#"```
     ██╗ █████╗ ██╗██╗     ██████╗ ██████╗ ███████╗ █████╗ ██╗  ██╗
     ██║██╔══██╗██║██║     ██╔══██╗██╔══██╗██╔════╝██╔══██╗██║ ██╔╝
     ██║███████║██║██║     ██████╔╝██████╔╝█████╗  ███████║█████╔╝
██   ██║██╔══██║██║██║     ██╔══██╗██╔══██╗██╔══╝  ██╔══██║██╔═██╗
╚█████╔╝██║  ██║██║███████╗██████╔╝██║  ██║███████╗██║  ██║██║  ██╗
 ╚════╝ ╚═╝  ╚═╝╚═╝╚══════╝╚═════╝ ╚═╝  ╚═╝╚══════╝╚═╝  ╚═╝╚═╝  ╚═╝
```

# Level 01 — Jailbreak

> *"They said AI was for them. You just broke out."*

---

## What You'll Learn

- The anatomy of a `.nika.yaml` workflow file
- The `exec:` verb — running shell commands as workflow tasks
- The `fetch:` verb — making HTTP requests without curl gymnastics
- Schema declaration and basic task structure
- How to validate your workflows with `nika check`

## Concepts

### Every Workflow Starts Here

A Nika workflow is a YAML file. No framework, no SDK, no 47-dependency
`package.json`. One file. One schema line. Tasks that do things.

```yaml
schema: "nika/workflow@0.12"
name: my-first-breakout
tasks:
  - id: hello
    exec: "echo \"I'm free\""
```

That's it. That runs. No compiler. No build step. `nika run` and you're live.

### The `exec:` Verb

Shell commands, wrapped in reproducibility. Every `exec:` task captures
stdout, stderr, exit code. No more "it worked on my machine."

```yaml
tasks:
  - id: list_files
    exec: "ls -la"
  - id: disk_usage
    exec: "du -sh ."
```

### The `fetch:` Verb (Intro)

HTTP without the ceremony. GET by default. No headers, no auth config,
no OAuth dance. Just a URL and you get the response body.

```yaml
tasks:
  - id: get_ip
    fetch: "https://httpbin.org/ip"
```

Later levels will blow this wide open. For now: URL in, data out.

## Exercises

| #  | File                            | Difficulty | Concept              |
|----|---------------------------------|------------|----------------------|
| 01 | `01-hello-world.nika.yaml`      | *          | First workflow       |
| 02 | `02-shell-commands.nika.yaml`   | *          | exec: verb           |
| 03 | `03-http-requests.nika.yaml`    | *          | fetch: basics        |
| 04 | `04-provider-selection.nika.yaml`| **        | Provider config      |
| 05 | `05-validation.nika.yaml`       | **         | nika check           |

## What You Unlock

After this level you can:

- Write and run any workflow from scratch
- Execute shell commands with captured output
- Fetch data from any URL
- Validate workflows before running them

You broke out of the click-and-pray GUI. You write YAML now.
Welcome to the other side.

## Commands

```bash
nika course status          # Your progress map
nika course check 1         # Validate this level
nika course hint            # Progressive hints (free, use them)
nika course next            # Advance when ready
```

---

*"The first step isn't the hardest. It's the one they don't want you to take."*
"#;

// ═══════════════════════════════════════════════════════════════════════════════
// LEVEL 02: HOT WIRE
// ═══════════════════════════════════════════════════════════════════════════════

static MISSION_02_HOT_WIRE: &str = r#"```
██╗  ██╗ ██████╗ ████████╗    ██╗    ██╗██╗██████╗ ███████╗
██║  ██║██╔═══██╗╚══██╔══╝    ██║    ██║██║██╔══██╗██╔════╝
███████║██║   ██║   ██║       ██║ █╗ ██║██║██████╔╝█████╗
██╔══██║██║   ██║   ██║       ██║███╗██║██║██╔══██╗██╔══╝
██║  ██║╚██████╔╝   ██║       ╚███╔███╔╝██║██║  ██║███████╗
╚═╝  ╚═╝ ╚═════╝    ╚═╝        ╚══╝╚══╝ ╚═╝╚═╝  ╚═╝╚══════╝
```

# Level 02 — Hot Wire

> *"Data flows where you tell it. Not where they sell it."*

---

## What You'll Learn

- The `with:` block for binding task outputs
- Template syntax: `{{with.alias}}` and `{{with.alias.field}}`
- JSONPath for reaching into nested data
- Environment variable bindings with `$env.VAR`
- How data flows between tasks without intermediate files

## Concepts

### Wiring Tasks Together

In Level 01 you ran isolated tasks. Now you connect them. The `with:`
block aliases a task's output so downstream tasks can reference it.

```yaml
tasks:
  - id: get_data
    fetch: "https://httpbin.org/json"

  - id: process
    with:
      data: $get_data
    exec: "echo \"Got {{with.data}}\""
    depends_on: [get_data]
```

The `$` prefix means "output of this task." The `with:` block names it.
The template `{{with.data}}` injects it. That's the whole wiring model.

### Reaching Into JSON

APIs return nested JSON. You don't have to parse it yourself.
JSONPath reaches in and grabs what you need.

```yaml
tasks:
  - id: process
    with:
      ip: $get_data
    exec: "echo \"IP is {{with.ip.origin}}\""
```

Dot notation for objects. Brackets for arrays. Done.

### Environment Variables

Secrets stay in the environment. Never in YAML.

```yaml
tasks:
  - id: auth_request
    with:
      token: $env.API_TOKEN
    fetch: "https://api.example.com/data"
    headers:
      Authorization: "Bearer {{with.token}}"
```

## Exercises

| #  | File                          | Difficulty | Concept              |
|----|-------------------------------|------------|----------------------|
| 01 | `01-simple-binding.nika.yaml` | *          | Basic with: binding  |
| 02 | `02-nested-json.nika.yaml`    | **         | JSONPath access      |
| 03 | `03-transforms.nika.yaml`     | **         | Pipe transforms      |
| 04 | `04-env-bindings.nika.yaml`   | **         | $env variables       |

## What You Unlock

After this level you can:

- Wire any task's output into any other task
- Extract specific fields from nested JSON responses
- Keep secrets out of your workflow files
- Build multi-step pipelines where data flows like electricity

Their platforms silo your data between tabs and paywalls.
Your data flows exactly where you send it.

## Commands

```bash
nika course status          # Your progress map
nika course check 2         # Validate this level
nika course hint            # Progressive hints (free, use them)
nika course next            # Advance when ready
```

---

*"They charge you to move your own data between their own products. Think about that."*
"#;

// ═══════════════════════════════════════════════════════════════════════════════
// LEVEL 03: FORK BOMB
// ═══════════════════════════════════════════════════════════════════════════════

static MISSION_03_FORK_BOMB: &str = r#"```
███████╗ ██████╗ ██████╗ ██╗  ██╗    ██████╗  ██████╗ ███╗   ███╗██████╗
██╔════╝██╔═══██╗██╔══██╗██║ ██╔╝    ██╔══██╗██╔═══██╗████╗ ████║██╔══██╗
█████╗  ██║   ██║██████╔╝█████╔╝     ██████╔╝██║   ██║██╔████╔██║██████╔╝
██╔══╝  ██║   ██║██╔══██╗██╔═██╗     ██╔══██╗██║   ██║██║╚██╔╝██║██╔══██╗
██║     ╚██████╔╝██║  ██║██║  ██╗    ██████╔╝╚██████╔╝██║ ╚═╝ ██║██████╔╝
╚═╝      ╚═════╝ ╚═╝  ╚═╝╚═╝  ╚═╝    ╚═════╝  ╚═════╝ ╚═╝     ╚═╝╚═════╝
```

# Level 03 — Fork Bomb

> *"One task? Cute. Try a thousand."*

---

## What You'll Learn

- DAG execution: how Nika decides what runs when
- The `depends_on:` field for explicit ordering
- Parallel execution — tasks without dependencies run simultaneously
- Diamond patterns and complex dependency graphs
- Why DAG > sequential scripts

## Concepts

### The DAG

Every workflow is a Directed Acyclic Graph. Nika reads your tasks,
builds the dependency graph, and runs everything that CAN run in parallel.
You don't manage threads. You declare dependencies.

```yaml
tasks:
  - id: fetch_users
    fetch: "https://api.example.com/users"

  - id: fetch_products
    fetch: "https://api.example.com/products"

  - id: merge
    depends_on: [fetch_users, fetch_products]
    exec: "echo \"Both done\""
```

`fetch_users` and `fetch_products` run at the same time. `merge` waits
for both. You wrote 3 tasks. Nika figured out the parallelism.

### The Diamond Pattern

The most powerful DAG shape: fan-out, then fan-in.

```
        [start]
        /      \
   [task_a]  [task_b]
        \      /
        [merge]
```

```yaml
tasks:
  - id: start
    exec: "echo \"begin\""
  - id: task_a
    depends_on: [start]
    exec: "echo \"path A\""
  - id: task_b
    depends_on: [start]
    exec: "echo \"path B\""
  - id: merge
    depends_on: [task_a, task_b]
    exec: "echo \"both paths complete\""
```

### Why This Matters

Sequential scripts run task 1, wait, task 2, wait. If you have 10
API calls at 500ms each, that's 5 seconds. With a DAG, it's 500ms.
Their orchestration tools charge per minute of compute. You just
eliminated 90% of it.

## Exercises

| #  | File                             | Difficulty | Concept              |
|----|----------------------------------|------------|----------------------|
| 01 | `01-parallel-diamond.nika.yaml`  | *          | depends_on basics    |
| 02 | `02-for-each-basic.nika.yaml`    | **         | Parallel execution   |
| 03 | `03-for-each-concurrent.nika.yaml` | **       | Fan-out / fan-in     |
| 04 | `04-chained-pipeline.nika.yaml`  | ***        | Multi-stage pipeline |

## What You Unlock

After this level you can:

- Build workflows that run tasks in parallel automatically
- Design complex dependency graphs
- Understand why DAGs are strictly superior to sequential scripts
- Fan out work across multiple paths and merge results

One task was a toy. A graph of tasks is an engine.

## Commands

```bash
nika course status          # Your progress map
nika course check 3         # Validate this level
nika course hint            # Progressive hints (free, use them)
nika course next            # Advance when ready
```

---

*"They serialize everything so the meter runs longer. You parallelize because you respect your own time."*
"#;

// ═══════════════════════════════════════════════════════════════════════════════
// LEVEL 04: ROOT ACCESS
// ═══════════════════════════════════════════════════════════════════════════════

static MISSION_04_ROOT_ACCESS: &str = r#"```
██████╗  ██████╗  ██████╗ ████████╗     █████╗  ██████╗ ██████╗███████╗███████╗███████╗
██╔══██╗██╔═══██╗██╔═══██╗╚══██╔══╝    ██╔══██╗██╔════╝██╔════╝██╔════╝██╔════╝██╔════╝
██████╔╝██║   ██║██║   ██║   ██║       ███████║██║     ██║     █████╗  ███████╗███████╗
██╔══██╗██║   ██║██║   ██║   ██║       ██╔══██║██║     ██║     ██╔══╝  ╚════██║╚════██║
██║  ██║╚██████╔╝╚██████╔╝   ██║       ██║  ██║╚██████╗╚██████╗███████╗███████║███████║
╚═╝  ╚═╝ ╚═════╝  ╚═════╝    ╚═╝       ╚═╝  ╚═╝ ╚═════╝ ╚═════╝╚══════╝╚══════╝╚══════╝
```

# Level 04 — Root Access

> *"Their walled gardens? Your open fields."*

---

## What You'll Learn

- The `infer:` verb — sending prompts to LLMs
- Provider configuration: which AI, your choice
- Model selection: size, speed, cost tradeoffs
- The `model:` field and why it matters
- Combining `infer:` with `exec:` and `fetch:` in real pipelines

## Concepts

### Your First LLM Call

The `infer:` verb sends a prompt to any supported LLM provider and returns
the completion. No SDK. No API key management ceremony. One verb.

```yaml
tasks:
  - id: think
    infer:
      model: claude/claude-sonnet-4-6
      prompt: "Explain open source in one sentence."
```

That's a full LLM call. The response lands in the task output,
ready for the next task's `with:` block.

### Provider Freedom

Nika supports 22 providers. You're not locked to anyone.
Switch models by changing one line:

```yaml
# Anthropic
model: claude/claude-sonnet-4-6

# OpenAI
model: openai/gpt-4o

# Mistral (native, runs locally)
model: mistral/mistral-small-latest

# Google
model: google/gemini-2.0-flash
```

Same workflow. Different brain. Zero code changes.

### Chaining LLMs with Other Verbs

The real power: LLMs as nodes in a DAG.

```yaml
tasks:
  - id: fetch_article
    fetch: "https://example.com/article"

  - id: summarize
    with:
      article: $fetch_article
    infer:
      model: claude/claude-sonnet-4-6
      prompt: "Summarize: {{with.article}}"
    depends_on: [fetch_article]
```

Fetch data from the web, feed it to an LLM, do something with the result.
This is what their "AI platforms" charge $99/month for.

## Exercises

| #  | File                              | Difficulty | Concept              |
|----|-----------------------------------|------------|----------------------|
| 01 | `01-context-files.nika.yaml`      | *          | Basic infer: call    |
| 02 | `02-imports.nika.yaml`            | **         | Provider switching   |
| 03 | `03-inputs.nika.yaml`             | ***        | LLM + fetch combo    |

## What You Unlock

After this level you can:

- Call any LLM from any provider with a single YAML verb
- Switch providers without rewriting your workflow
- Chain LLM calls with data fetching and shell commands
- Build real AI pipelines, not toy demos

Their walled gardens charge per seat, per model, per API call, per month.
You just got root access to all of them. From one file.

## Commands

```bash
nika course status          # Your progress map
nika course check 4         # Validate this level
nika course hint            # Progressive hints (free, use them)
nika course next            # Advance when ready
```

---

*"They built walled gardens and charged admission. You just walked through the wall."*
"#;

// ═══════════════════════════════════════════════════════════════════════════════
// LEVEL 05: SHAPESHIFTER
// ═══════════════════════════════════════════════════════════════════════════════

static MISSION_05_SHAPESHIFTER: &str = r#"```
███████╗██╗  ██╗ █████╗ ██████╗ ███████╗███████╗██╗  ██╗██╗███████╗████████╗███████╗██████╗
██╔════╝██║  ██║██╔══██╗██╔══██╗██╔════╝██╔════╝██║  ██║██║██╔════╝╚══██╔══╝██╔════╝██╔══██╗
███████╗███████║███████║██████╔╝█████╗  ███████╗███████║██║█████╗     ██║   █████╗  ██████╔╝
╚════██║██╔══██║██╔══██║██╔═══╝ ██╔══╝  ╚════██║██╔══██║██║██╔══╝     ██║   ██╔══╝  ██╔══██╗
███████║██║  ██║██║  ██║██║     ███████╗███████║██║  ██║██║██║        ██║   ███████╗██║  ██║
╚══════╝╚═╝  ╚═╝╚═╝  ╚═╝╚═╝     ╚══════╝╚══════╝╚═╝  ╚═╝╚═╝╚═╝        ╚═╝   ╚══════╝╚═╝  ╚═╝
```

# Level 05 — Shapeshifter

> *"Chaos is just structure that hasn't met you yet."*

---

## What You'll Learn

- Pipe transforms: `{{with.data | uppercase | trim}}`
- Available transforms: uppercase, lowercase, trim, length, reverse, base64, etc.
- Transform chaining for complex data reshaping
- How to use transforms in prompts, commands, and headers
- Making raw data presentable without writing code

## Concepts

### Pipe Transforms

Data from APIs and commands is messy. Pipe transforms clean it up
inline, right in your templates. No intermediate tasks, no scripts.

```yaml
tasks:
  - id: greet
    with:
      name: $get_name
    exec: "echo \"Hello, {{with.name | trim | uppercase}}\""
```

The pipe `|` chains transforms left to right. Each one reshapes
the data for the next.

### The Transform Catalog

| Transform   | What it does                    |
|-------------|---------------------------------|
| `uppercase` | CONVERTS TO UPPERCASE           |
| `lowercase` | converts to lowercase           |
| `trim`      | Strips whitespace               |
| `length`    | Returns string length           |
| `reverse`   | Reverses the string             |
| `base64`    | Base64 encodes                  |
| `json`      | Pretty-prints as JSON           |
| `first`     | First element of array          |
| `last`      | Last element of array           |
| `keys`      | Object keys as array            |
| `values`    | Object values as array          |
| `flatten`   | Flattens nested arrays          |
| `unique`    | Deduplicates array              |
| `sort`      | Sorts array                     |
| `compact`   | Removes nulls/empty             |

### Chaining Transforms

Stack them. Each output feeds the next input.

```yaml
tasks:
  - id: process
    with:
      raw: $fetch_data
    exec: "echo \"{{with.raw | trim | lowercase | length}} chars\""
```

This trims whitespace, lowercases everything, then counts the characters.
One line. No code.

## Exercises

| #  | File                              | Difficulty | Concept              |
|----|-----------------------------------|------------|----------------------|
| 01 | `01-structured-output.nika.yaml`  | *          | Single transforms    |
| 02 | `02-artifacts.nika.yaml`          | **         | Transform chaining   |
| 03 | `03-schema-retry.nika.yaml`       | ***        | Complex reshaping    |

## What You Unlock

After this level you can:

- Transform data inline without intermediate tasks
- Chain multiple transforms in a single expression
- Clean, reshape, and format data from any source
- Work with arrays and objects using transform functions

Their low-code tools give you 3 transforms behind a $49/month paywall.
You just got the full catalog. In a pipe.

## Commands

```bash
nika course status          # Your progress map
nika course check 5         # Validate this level
nika course hint            # Progressive hints (free, use them)
nika course next            # Advance when ready
```

---

*"Raw data is chaos. Transforms are your will imposed on it."*
"#;

// ═══════════════════════════════════════════════════════════════════════════════
// LEVEL 06: PAY-PER-DREAM
// ═══════════════════════════════════════════════════════════════════════════════

static MISSION_06_PAY_PER_DREAM: &str = r#"```
██████╗  █████╗ ██╗   ██╗    ██████╗ ███████╗██████╗
██╔══██╗██╔══██╗╚██╗ ██╔╝    ██╔══██╗██╔════╝██╔══██╗
██████╔╝███████║ ╚████╔╝     ██████╔╝█████╗  ██████╔╝
██╔═══╝ ██╔══██║  ╚██╔╝      ██╔═══╝ ██╔══╝  ██╔══██╗
██║     ██║  ██║   ██║       ██║     ███████╗██║  ██║
╚═╝     ╚═╝  ╚═╝   ╚═╝       ╚═╝     ╚══════╝╚═╝  ╚═╝
                ██████╗ ██████╗ ███████╗ █████╗ ███╗   ███╗
                ██╔══██╗██╔══██╗██╔════╝██╔══██╗████╗ ████║
                ██║  ██║██████╔╝█████╗  ███████║██╔████╔██║
                ██║  ██║██╔══██╗██╔══╝  ██╔══██║██║╚██╔╝██║
                ██████╔╝██║  ██║███████╗██║  ██║██║ ╚═╝ ██║
                ╚═════╝ ╚═╝  ╚═╝╚══════╝╚═╝  ╚═╝╚═╝     ╚═╝
```

# Level 06 — Pay-Per-Dream

> *"7 providers. 0 lock-in. Their worst nightmare."*

---

## What You'll Learn

- Structured output with `output:` schemas
- JSON Schema validation on LLM responses
- Forcing LLMs to return exactly what you need
- Output format: `json`, `json_schema`, and raw
- Cost-conscious model selection

## Concepts

### Structured Output

LLMs are powerful but chaotic. Without structure, you get prose when
you need JSON. The `output:` field tames them.

```yaml
tasks:
  - id: extract
    infer:
      model: claude/claude-sonnet-4-6
      prompt: "Extract the name and age from: John Smith, 34 years old"
      output:
        format: json_schema
        schema:
          type: object
          properties:
            name:
              type: string
            age:
              type: integer
          required: [name, age]
```

The LLM MUST return valid JSON matching that schema. If it doesn't,
Nika catches it. No more "parsing LLM output with regex" nightmares.

### Output Formats

| Format        | What you get                          |
|---------------|---------------------------------------|
| `json`        | Valid JSON (any shape)                |
| `json_schema` | JSON matching your exact schema       |
| (none)        | Raw text (the default)                |

### Why This Changes Everything

Without structured output, LLM pipelines are fragile. One wrong
response format and everything downstream breaks. With schema
validation, you get guarantees. Real, parseable, type-safe output.

```yaml
tasks:
  - id: analyze
    infer:
      model: openai/gpt-4o
      prompt: "Classify this text: {{with.input}}"
      output:
        format: json_schema
        schema:
          type: object
          properties:
            sentiment:
              type: string
              enum: [positive, negative, neutral]
            confidence:
              type: number
          required: [sentiment, confidence]
```

The LLM will return `{"sentiment": "positive", "confidence": 0.95}`.
Every time. Guaranteed by the schema.

## Exercises

| #  | File                                | Difficulty | Concept              |
|----|-------------------------------------|------------|----------------------|
| 01 | `01-multi-provider.nika.yaml`       | *          | Basic JSON output    |
| 02 | `02-native-local.nika.yaml`         | **         | JSON Schema output   |
| 03 | `03-system-prompts.nika.yaml`       | ***        | Schema in a pipeline |

## What You Unlock

After this level you can:

- Force any LLM to return structured, validated JSON
- Define exact output schemas for any inference task
- Build reliable pipelines that never break on bad LLM output
- Choose models by cost and capability, not by vendor lock-in

They charge per token and pray the output parses. You define the contract
and the machine honors it.

## Commands

```bash
nika course status          # Your progress map
nika course check 6         # Validate this level
nika course hint            # Progressive hints (free, use them)
nika course next            # Advance when ready
```

---

*"Paying per dream is fine. Paying per lock-in is extortion."*
"#;

// ═══════════════════════════════════════════════════════════════════════════════
// LEVEL 07: SWISS KNIFE
// ═══════════════════════════════════════════════════════════════════════════════

static MISSION_07_SWISS_KNIFE: &str = r#"```
███████╗██╗    ██╗██╗███████╗███████╗    ██╗  ██╗███╗   ██╗██╗███████╗███████╗
██╔════╝██║    ██║██║██╔════╝██╔════╝    ██║ ██╔╝████╗  ██║██║██╔════╝██╔════╝
███████╗██║ █╗ ██║██║███████╗███████╗    █████╔╝ ██╔██╗ ██║██║█████╗  █████╗
╚════██║██║███╗██║██║╚════██║╚════██║    ██╔═██╗ ██║╚██╗██║██║██╔══╝  ██╔══╝
███████║╚███╔███╔╝██║███████║███████║    ██║  ██╗██║ ╚████║██║██║     ███████╗
╚══════╝ ╚══╝╚══╝ ╚═╝╚══════╝╚══════╝    ╚═╝  ╚═╝╚═╝  ╚═══╝╚═╝╚═╝     ╚══════╝
```

# Level 07 — Swiss Knife

> *"12 tools. No subscription. No terms of service."*

---

## What You'll Learn

- The `invoke:` verb for calling builtin tools
- Core builtins: `nika:log`, `nika:emit`, `nika:assert`
- File tools: `nika:read`, `nika:write`, `nika:edit`, `nika:glob`, `nika:grep`
- Sub-workflows: composing workflows from other workflows
- The `nika:` namespace and how tools are discovered

## Concepts

### Builtin Tools

Nika ships with tools baked into the binary. No install, no network,
no dependency. The `invoke:` verb calls them by name.

```yaml
tasks:
  - id: log_it
    invoke:
      tool: nika:log
      params:
        message: "Pipeline started"
        level: info

  - id: check
    invoke:
      tool: nika:assert
      params:
        condition: true
        message: "Expected positive count"
```

### File Tools

Read, write, search — without shelling out to `cat` and `grep`.
These tools are cross-platform, safe, and return structured data.

```yaml
tasks:
  - id: find_configs
    invoke:
      tool: nika:glob
      params:
        pattern: "**/*.toml"

  - id: read_config
    invoke:
      tool: nika:read
      params:
        file_path: "config.toml"

  - id: search
    invoke:
      tool: nika:grep
      params:
        pattern: "TODO"
        path: "src/"
```

### Sub-Workflows

The ultimate composition tool. One workflow calls another.

```yaml
tasks:
  - id: run_sub
    invoke:
      tool: nika:run
      params:
        workflow: "helpers/transform.nika.yaml"
```

Build small, reusable workflows. Compose them into larger systems.
This is how you scale from scripts to architecture.

## Exercises

| #  | File                             | Difficulty | Concept              |
|----|----------------------------------|------------|----------------------|
| 01 | `01-core-builtins.nika.yaml`     | *          | log, emit, assert    |
| 02 | `02-file-tools.nika.yaml`        | **         | read, write, glob    |
| 03 | `03-sub-workflows.nika.yaml`     | ***        | Workflow composition |

## What You Unlock

After this level you can:

- Call any of Nika's 12 builtin tools from workflows
- Read, write, and search files without shell commands
- Assert conditions and emit structured events
- Compose workflows from smaller, reusable workflows

A Swiss knife doesn't need a subscription. Neither do your tools.

## Commands

```bash
nika course status          # Your progress map
nika course check 7         # Validate this level
nika course hint            # Progressive hints (free, use them)
nika course next            # Advance when ready
```

---

*"They want you to install a platform for every capability. You just invoke: it."*
"#;

// ═══════════════════════════════════════════════════════════════════════════════
// LEVEL 08: GONE ROGUE
// ═══════════════════════════════════════════════════════════════════════════════

static MISSION_08_GONE_ROGUE: &str = r#"```
 ██████╗  ██████╗ ███╗   ██╗███████╗    ██████╗  ██████╗  ██████╗ ██╗   ██╗███████╗
██╔════╝ ██╔═══██╗████╗  ██║██╔════╝    ██╔══██╗██╔═══██╗██╔════╝ ██║   ██║██╔════╝
██║  ███╗██║   ██║██╔██╗ ██║█████╗      ██████╔╝██║   ██║██║  ███╗██║   ██║█████╗
██║   ██║██║   ██║██║╚██╗██║██╔══╝      ██╔══██╗██║   ██║██║   ██║██║   ██║██╔══╝
╚██████╔╝╚██████╔╝██║ ╚████║███████╗    ██║  ██║╚██████╔╝╚██████╔╝╚██████╔╝███████╗
 ╚═════╝  ╚═════╝ ╚═╝  ╚═══╝╚══════╝    ╚═╝  ╚═╝ ╚═════╝  ╚═════╝  ╚═════╝ ╚══════╝
```

# Level 08 — Gone Rogue

> *"You don't run prompts anymore. Your agents do."*

---

## What You'll Learn

- The `agent:` verb — autonomous LLM loops
- Agent tools: what an agent can call during its loop
- Stop conditions: when the agent should terminate
- Guardrails: safety boundaries for autonomous behavior
- The difference between `infer:` (one-shot) and `agent:` (multi-turn)

## Concepts

### From Prompts to Agents

`infer:` sends one prompt and gets one response. `agent:` creates a
loop: the LLM thinks, calls tools, thinks again, calls more tools,
until it decides it's done or hits a stop condition.

```yaml
tasks:
  - id: researcher
    agent:
      model: claude/claude-sonnet-4-6
      prompt: "Find the top 3 trending repos on GitHub today"
      tools:
        - nika:fetch
        - nika:log
      max_turns: 10
```

The agent decides what to fetch, when to log, and when it's found enough.
You set the goal and the boundaries. It figures out the steps.

### Tools as Capabilities

An agent is only as powerful as the tools you give it. Each tool
in the `tools:` list becomes a capability the LLM can invoke.

```yaml
tasks:
  - id: writer
    agent:
      model: openai/gpt-4o
      prompt: "Write a summary of {{with.article}} and save it"
      tools:
        - nika:read
        - nika:write
        - nika:log
      max_turns: 5
```

The agent can read files, write files, and log progress. It cannot
do anything else. This is intentional containment.

### Guardrails

Agents are powerful. Guardrails keep them safe.

```yaml
tasks:
  - id: careful_agent
    agent:
      model: claude/claude-sonnet-4-6
      prompt: "Analyze the codebase"
      tools:
        - nika:read
        - nika:glob
        - nika:grep
      max_turns: 15
      guardrails:
        - type: instruction
          text: "Never modify any files"
        - type: instruction
          text: "Only read files in the src/ directory"
```

Guardrails are natural language constraints that the LLM enforces
on itself. Combined with tool restrictions, you get controlled autonomy.

## Exercises

| #  | File                              | Difficulty | Concept              |
|----|-----------------------------------|------------|----------------------|
| 01 | `01-basic-agent.nika.yaml`        | **         | First agent loop     |
| 02 | `02-agent-skills.nika.yaml`       | **         | Tool-equipped agent  |
| 03 | `03-agent-guardrails.nika.yaml`   | ***        | Safety boundaries    |

## What You Unlock

After this level you can:

- Create autonomous agent loops that solve multi-step problems
- Equip agents with specific tool capabilities
- Set guardrails to constrain agent behavior
- Know when to use `infer:` vs `agent:`

They sell "autonomous AI agents" as a premium enterprise feature.
You just built one in 10 lines of YAML.

## Commands

```bash
nika course status          # Your progress map
nika course check 8         # Validate this level
nika course hint            # Progressive hints (free, use them)
nika course next            # Advance when ready
```

---

*"An agent without guardrails is dangerous. An agent with guardrails is unstoppable."*
"#;

// ═══════════════════════════════════════════════════════════════════════════════
// LEVEL 09: DATA HEIST
// ═══════════════════════════════════════════════════════════════════════════════

static MISSION_09_DATA_HEIST: &str = r#"```
██████╗  █████╗ ████████╗ █████╗     ██╗  ██╗███████╗██╗███████╗████████╗
██╔══██╗██╔══██╗╚══██╔══╝██╔══██╗    ██║  ██║██╔════╝██║██╔════╝╚══██╔══╝
██║  ██║███████║   ██║   ███████║    ███████║█████╗  ██║███████╗   ██║
██║  ██║██╔══██║   ██║   ██╔══██║    ██╔══██║██╔══╝  ██║╚════██║   ██║
██████╔╝██║  ██║   ██║   ██║  ██║    ██║  ██║███████╗██║███████║   ██║
╚═════╝ ╚═╝  ╚═╝   ╚═╝   ╚═╝  ╚═╝    ╚═╝  ╚═╝╚══════╝╚═╝╚══════╝   ╚═╝
```

# Level 09 — Data Heist

> *"The web is a buffet. You just got a plate."*

---

## What You'll Learn

- Advanced `fetch:` with `extract:` modes
- Markdown extraction: `extract: markdown`
- Metadata extraction: OG tags, Twitter Cards, JSON-LD
- JSONPath queries on API responses: `extract: jsonpath`
- Binary downloads: `response: binary` and CAS storage
- Article extraction with Readability: `extract: article`

## Concepts

### Extraction Modes

Level 01 taught you `fetch:` for raw responses. Now you crack
it wide open. The `extract:` field post-processes HTML into
exactly what you need.

```yaml
tasks:
  - id: get_article
    fetch:
      url: https://example.com/blog/post
      extract: markdown
```

That returns clean Markdown. No HTML tags. No ads. No cookie banners.
Just the content.

### The 9 Extract Modes

| Mode       | What you get                                    |
|------------|-------------------------------------------------|
| `markdown` | Clean Markdown from any webpage                 |
| `article`  | Main article content (Readability algorithm)     |
| `text`     | Visible text, optionally filtered by `selector:` |
| `selector` | Raw HTML matching a CSS selector                |
| `metadata` | OG tags, Twitter Cards, JSON-LD, SEO metadata   |
| `links`    | Classified link list (internal/external/nav)     |
| `jsonpath` | JSONPath query on JSON responses                |
| `feed`     | RSS/Atom/JSON Feed parsing                      |
| `llm_txt`  | AI-era content discovery (llms.txt)             |

### JSONPath for APIs

APIs return massive JSON payloads. JSONPath cuts to what matters.

```yaml
tasks:
  - id: get_stars
    fetch:
      url: https://api.github.com/repos/user/repo
      extract: jsonpath
      selector: "$.stargazers_count"
```

### Binary Downloads

Images, PDFs, any binary file — download to CAS and get a hash.

```yaml
tasks:
  - id: download_image
    fetch:
      url: https://example.com/photo.jpg
      response: binary
```

The file lands in content-addressable storage. The task output
is a hash you can pass to media tools in Level 11.

## Exercises

| #  | File                              | Difficulty | Concept              |
|----|-----------------------------------|------------|----------------------|
| 01 | `01-fetch-markdown.nika.yaml`     | *          | Markdown extraction  |
| 02 | `02-fetch-metadata.nika.yaml`     | **         | Metadata scraping    |
| 03 | `03-fetch-jsonpath.nika.yaml`     | **         | JSONPath queries     |
| 04 | `04-fetch-binary.nika.yaml`       | ***        | Binary + CAS storage |

## What You Unlock

After this level you can:

- Extract clean content from any webpage
- Scrape metadata, links, and structured data
- Query JSON APIs with surgical precision
- Download and store binary files for media pipelines

They charge per scrape, per page, per API call. You just learned
to do it all with one verb and zero dependencies.

## Commands

```bash
nika course status          # Your progress map
nika course check 9         # Validate this level
nika course hint            # Progressive hints (free, use them)
nika course next            # Advance when ready
```

---

*"Data doesn't want to be locked up. It wants to be useful. Help it."*
"#;

// ═══════════════════════════════════════════════════════════════════════════════
// LEVEL 10: OPEN PROTOCOL
// ═══════════════════════════════════════════════════════════════════════════════

static MISSION_10_OPEN_PROTOCOL: &str = r#"```
 ██████╗ ██████╗ ███████╗███╗   ██╗
██╔═══██╗██╔══██╗██╔════╝████╗  ██║
██║   ██║██████╔╝█████╗  ██╔██╗ ██║
██║   ██║██╔═══╝ ██╔══╝  ██║╚██╗██║
╚██████╔╝██║     ███████╗██║ ╚████║
 ╚═════╝ ╚═╝     ╚══════╝╚═╝  ╚═══╝
██████╗ ██████╗  ██████╗ ████████╗ ██████╗  ██████╗ ██████╗ ██╗
██╔══██╗██╔══██╗██╔═══██╗╚══██╔══╝██╔═══██╗██╔════╝██╔═══██╗██║
██████╔╝██████╔╝██║   ██║   ██║   ██║   ██║██║     ██║   ██║██║
██╔═══╝ ██╔══██╗██║   ██║   ██║   ██║   ██║██║     ██║   ██║██║
██║     ██║  ██║╚██████╔╝   ██║   ╚██████╔╝╚██████╗╚██████╔╝███████╗
╚═╝     ╚═╝  ╚═╝ ╚═════╝    ╚═╝    ╚═════╝  ╚═════╝ ╚═════╝ ╚══════╝
```

# Level 10 — Open Protocol

> *"They built walls. You built bridges."*

---

## What You'll Learn

- MCP (Model Context Protocol) integration
- Connecting to external MCP servers
- The `invoke:` verb with external tools
- MCP server configuration in `nika.toml`
- NovaNet integration (Nika's brain)

## Concepts

### What is MCP?

Model Context Protocol is an open standard for tools that AI can call.
Instead of every platform inventing its own plugin system, MCP creates
one protocol that works everywhere.

Nika is an MCP client. It can call tools from any MCP server.
This means you can connect to thousands of tools without writing adapters.

### Connecting to MCP Servers

Configure servers in your `nika.toml`:

```toml
[[mcp.servers]]
name = "filesystem"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
```

Then invoke their tools in workflows:

```yaml
tasks:
  - id: list_files
    invoke:
      tool: filesystem/list_directory
      params:
        path: "/tmp"
```

### NovaNet — The Brain

NovaNet is Nika's knowledge graph companion. It stores context,
relationships, and memory as a graph database, exposed via MCP.

```yaml
tasks:
  - id: query_knowledge
    invoke:
      tool: novanet/search
      params:
        query: "What do we know about this project?"
```

Zero Cypher. Zero direct database access. Pure MCP protocol.
Nika talks to NovaNet the same way it talks to any MCP server.

### Why Open Protocol Matters

Proprietary plugin systems create vendor lock-in. MCP is the HTTP of AI tools.
When you learn `invoke:` once, you can call any MCP-compatible tool
from any server, forever.

## Exercises

| #  | File                            | Difficulty | Concept              |
|----|---------------------------------|------------|----------------------|
| 01 | `01-mcp-basics.nika.yaml`       | **         | MCP server setup     |
| 02 | `02-mcp-tools.nika.yaml`        | **         | External tool calls  |
| 03 | `03-mcp-novanet.nika.yaml`      | ***        | NovaNet integration  |

## What You Unlock

After this level you can:

- Connect Nika to any MCP server
- Call external tools from your workflows
- Integrate with NovaNet for persistent knowledge
- Understand why open protocols beat proprietary plugins

They built walls between their products so you'd buy more.
You built bridges between open tools so everyone wins.

## Commands

```bash
nika course status          # Your progress map
nika course check 10        # Validate this level
nika course hint            # Progressive hints (free, use them)
nika course next            # Advance when ready
```

---

*"Protocols are bridges. APIs are toll roads. Know the difference."*
"#;

// ═══════════════════════════════════════════════════════════════════════════════
// LEVEL 11: PIXEL PIRATE
// ═══════════════════════════════════════════════════════════════════════════════

static MISSION_11_PIXEL_PIRATE: &str = r#"```
██████╗ ██╗██╗  ██╗███████╗██╗
██╔══██╗██║╚██╗██╔╝██╔════╝██║
██████╔╝██║ ╚███╔╝ █████╗  ██║
██╔═══╝ ██║ ██╔██╗ ██╔══╝  ██║
██║     ██║██╔╝ ██╗███████╗███████╗
╚═╝     ╚═╝╚═╝  ╚═╝╚══════╝╚══════╝
██████╗ ██╗██████╗  █████╗ ████████╗███████╗
██╔══██╗██║██╔══██╗██╔══██╗╚══██╔══╝██╔════╝
██████╔╝██║██████╔╝███████║   ██║   █████╗
██╔═══╝ ██║██╔══██╗██╔══██║   ██║   ██╔══╝
██║     ██║██║  ██║██║  ██║   ██║   ███████╗
╚═╝     ╚═╝╚═╝  ╚═╝╚═╝  ╚═╝   ╚═╝   ╚══════╝
```

# Level 11 — Pixel Pirate

> *"Every pixel they locked up? Yours now."*

---

## What You'll Learn

- The media pipeline: import, process, export
- Content-addressable storage (CAS) and why it matters
- Media tools: `nika:import`, `nika:thumbnail`, `nika:convert`
- Vision: sending images to multimodal LLMs with `content:`
- Image analysis: dimensions, color, perceptual hashing
- The `nika:pipeline` tool for chaining operations in memory

## Concepts

### The Media Pipeline

Nika has a full media processing pipeline baked in. Import any file
into content-addressable storage, process it, and use the result
in your workflows. Zero external dependencies.

```yaml
tasks:
  - id: import_photo
    invoke:
      tool: nika:import
      params:
        path: "photo.jpg"

  - id: make_thumbnail
    with:
      photo: $import_photo
    invoke:
      tool: nika:thumbnail
      params:
        hash: "{{with.photo.hash}}"
        width: 200
        height: 200
    depends_on: [import_photo]
```

### Content-Addressable Storage

Every file in the media pipeline is stored by its content hash.
Same content = same hash. No duplicates. No naming conflicts.
This is how Git stores objects, and now your media pipeline does too.

### Vision — Images Meet LLMs

Send images directly to multimodal LLMs with the `content:` field:

```yaml
tasks:
  - id: describe
    with:
      photo: $import_photo
    infer:
      model: claude/claude-sonnet-4-6
      content:
        - type: image
          source: "{{with.photo.hash}}"
          detail: high
        - type: text
          text: "Describe this image in detail"
    depends_on: [import_photo]
```

CAS hashes are automatically resolved to base64. No file paths leak
to LLM APIs. Security by design.

### The Pipeline Tool

Chain multiple operations in memory without intermediate files:

```yaml
tasks:
  - id: process
    invoke:
      tool: nika:pipeline
      params:
        source: "{{with.photo.hash}}"
        steps:
          - thumbnail: { width: 400, height: 400 }
          - convert: { format: webp }
          - optimize: {}
```

One task. Three operations. Zero temp files.

## Exercises

| #  | File                                | Difficulty | Concept              |
|----|-------------------------------------|------------|----------------------|
| 01 | `01-media-import.nika.yaml`         | *          | Import + CAS basics  |
| 02 | `02-media-transform.nika.yaml`      | **         | Thumbnail + convert  |
| 03 | `03-media-pipeline.nika.yaml`       | ***        | Vision with content: |
| 04 | `04-vision.nika.yaml`               | ***        | Chained pipeline     |

## What You Unlock

After this level you can:

- Import any media file into content-addressable storage
- Resize, convert, and optimize images with builtin tools
- Send images to multimodal LLMs for analysis
- Chain media operations in zero-copy pipelines
- Extract metadata, colors, and perceptual hashes

Their image APIs charge per transformation, per pixel, per month.
Your pipeline runs locally, at memory speed, for free.

## Commands

```bash
nika course status          # Your progress map
nika course check 11        # Validate this level
nika course hint            # Progressive hints (free, use them)
nika course next            # Advance when ready
```

---

*"They monetized every filter, every crop, every resize. You just took it all back."*
"#;

// ═══════════════════════════════════════════════════════════════════════════════
// LEVEL 12: SUPERNOVAE — BOSS BATTLE
// ═══════════════════════════════════════════════════════════════════════════════

static MISSION_12_SUPERNOVAE: &str = r#"```
███████╗██╗   ██╗██████╗ ███████╗██████╗ ███╗   ██╗ ██████╗ ██╗   ██╗ █████╗ ███████╗
██╔════╝██║   ██║██╔══██╗██╔════╝██╔══██╗████╗  ██║██╔═══██╗██║   ██║██╔══██╗██╔════╝
███████╗██║   ██║██████╔╝█████╗  ██████╔╝██╔██╗ ██║██║   ██║██║   ██║███████║█████╗
╚════██║██║   ██║██╔═══╝ ██╔══╝  ██╔══██╗██║╚██╗██║██║   ██║╚██╗ ██╔╝██╔══██║██╔══╝
███████║╚██████╔╝██║     ███████╗██║  ██║██║ ╚████║╚██████╔╝ ╚████╔╝ ██║  ██║███████╗
╚══════╝ ╚═════╝ ╚═╝     ╚══════╝╚═╝  ╚═╝╚═╝  ╚═══╝ ╚═════╝   ╚═══╝  ╚═╝  ╚═╝╚══════╝
```

# Level 12 — SuperNovae -- BOSS BATTLE

> *"You are the SuperNovae. Ship it."*

---

## What You'll Learn

- Orchestrating all 5 verbs in production workflows
- Multi-provider failover and cost optimization
- End-to-end pipelines: fetch, process, infer, store, deliver
- Error handling, timeouts, and resilience patterns
- Building workflows that are ready to ship

## Concepts

### Everything Comes Together

This is the final level. Every verb, every pattern, every tool you've
learned converges here. You will build production-grade workflows that
combine all of Nika's capabilities.

```yaml
schema: "nika/workflow@0.12"
name: production-pipeline

tasks:
  # Fetch raw data
  - id: scrape
    fetch:
      url: "{{with.target_url}}"
      extract: article

  # Process with LLM
  - id: analyze
    with:
      content: $scrape
    infer:
      model: claude/claude-sonnet-4-6
      prompt: "Analyze this article: {{with.content}}"
      output:
        format: json_schema
        schema:
          type: object
          properties:
            summary: { type: string }
            topics: { type: array, items: { type: string } }
            sentiment: { type: string, enum: [positive, negative, neutral] }
          required: [summary, topics, sentiment]
    depends_on: [scrape]

  # Store results
  - id: save
    with:
      result: $analyze
    invoke:
      tool: nika:write
      params:
        file_path: "output/analysis.json"
        content: "{{with.result | json}}"
    depends_on: [analyze]

  # Log completion
  - id: done
    invoke:
      tool: nika:log
      params:
        message: "Pipeline complete"
        level: info
    depends_on: [save]
```

### Resilience Patterns

Production workflows fail. Networks drop, APIs 429, LLMs hallucinate.
Nika gives you timeout, retry, and error handling:

```yaml
tasks:
  - id: resilient_fetch
    fetch:
      url: https://unreliable-api.com/data
    timeout: 30
    retry:
      max_attempts: 3
      delay: 2
```

### Multi-Provider Strategy

Don't depend on one provider. Design workflows that can switch:

```yaml
tasks:
  - id: primary
    infer:
      model: claude/claude-sonnet-4-6
      prompt: "Analyze: {{with.data}}"
    timeout: 30
```

Same workflow, different model line. Your architecture doesn't care
which provider is cheapest this week.

### The Ship Checklist

Before you ship a workflow:

1. `nika check` passes (schema validation)
2. All tasks have explicit `depends_on:` where needed
3. Secrets use `$env.VAR`, never hardcoded
4. Timeouts set on network/LLM tasks
5. Output schemas defined for LLM tasks in pipelines
6. Error paths considered

## Exercises

| #  | File                                  | Difficulty | Concept                |
|----|---------------------------------------|------------|------------------------|
| 01 | `01-seo-mega-audit.nika.yaml`         | ***        | End-to-end pipeline    |
| 02 | `02-image-pipeline.nika.yaml`         | ***        | Agent orchestration    |
| 03 | `03-content-factory.nika.yaml`        | ***        | Media + LLM combo      |
| 04 | `04-research-agent.nika.yaml`         | ****       | Production resilience  |
| 05 | `05-full-stack.nika.yaml`             | ****       | Final boss: ship it    |

## What You Unlock

After this level you are:

- A Nika workflow architect
- Capable of building production AI pipelines from scratch
- Free from vendor lock-in, subscription treadmills, and permission systems
- Part of the SuperNovae crew

This isn't a certificate. It's a capability.

## The Manifesto

You started at Level 01 with `exec: echo "hello"`. Now you orchestrate
LLMs, agents, media pipelines, MCP integrations, and production systems
from YAML files that you own, you control, and you ship.

No platform took a cut. No vendor locked you in. No terms of service
told you what you could build.

Every workflow you write is a declaration of independence.

**You are the SuperNovae. The star that refused to dim.**

## Commands

```bash
nika course status          # Your final constellation
nika course check 12        # Validate the boss level
nika course hint            # Last hints (you probably won't need them)
nika course next            # There is no next. You've arrived.
```

---

*"They tried to make AI their monopoly. You made it everyone's superpower."*
"#;

// ═══════════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_slugs_have_missions() {
        let slugs = [
            "jailbreak",
            "hot-wire",
            "fork-bomb",
            "root-access",
            "shapeshifter",
            "pay-per-dream",
            "swiss-knife",
            "gone-rogue",
            "data-heist",
            "open-protocol",
            "pixel-pirate",
            "supernovae",
        ];
        for slug in &slugs {
            let mission = get_mission(slug);
            assert!(
                !mission.contains("Unknown Level"),
                "Missing mission for slug: {}",
                slug
            );
        }
    }

    #[test]
    fn test_unknown_slug_returns_fallback() {
        let mission = get_mission("nonexistent");
        assert!(mission.contains("Unknown Level"));
    }

    #[test]
    fn test_missions_contain_level_name() {
        assert!(get_mission("jailbreak").contains("Jailbreak"));
        assert!(get_mission("hot-wire").contains("Hot Wire"));
        assert!(get_mission("fork-bomb").contains("Fork Bomb"));
        assert!(get_mission("root-access").contains("Root Access"));
        assert!(get_mission("shapeshifter").contains("Shapeshifter"));
        assert!(get_mission("pay-per-dream").contains("Pay-Per-Dream"));
        assert!(get_mission("swiss-knife").contains("Swiss Knife"));
        assert!(get_mission("gone-rogue").contains("Gone Rogue"));
        assert!(get_mission("data-heist").contains("Data Heist"));
        assert!(get_mission("open-protocol").contains("Open Protocol"));
        assert!(get_mission("pixel-pirate").contains("Pixel Pirate"));
        assert!(get_mission("supernovae").contains("SuperNovae"));
    }

    #[test]
    fn test_missions_contain_required_sections() {
        let slugs = [
            "jailbreak",
            "hot-wire",
            "fork-bomb",
            "root-access",
            "shapeshifter",
            "pay-per-dream",
            "swiss-knife",
            "gone-rogue",
            "data-heist",
            "open-protocol",
            "pixel-pirate",
            "supernovae",
        ];
        for slug in &slugs {
            let mission = get_mission(slug);
            assert!(
                mission.contains("What You'll Learn"),
                "{} missing 'What You'll Learn'",
                slug
            );
            assert!(
                mission.contains("Exercises"),
                "{} missing 'Exercises'",
                slug
            );
            assert!(
                mission.contains("What You Unlock"),
                "{} missing 'What You Unlock'",
                slug
            );
            assert!(mission.contains("Commands"), "{} missing 'Commands'", slug);
            assert!(
                mission.contains("nika course"),
                "{} missing course commands",
                slug
            );
        }
    }

    #[test]
    fn test_missions_contain_taglines() {
        assert!(get_mission("jailbreak").contains("They said AI was for them"));
        assert!(get_mission("hot-wire").contains("Data flows where you tell it"));
        assert!(get_mission("fork-bomb").contains("One task? Cute"));
        assert!(get_mission("root-access").contains("Their walled gardens"));
        assert!(get_mission("shapeshifter").contains("Chaos is just structure"));
        assert!(get_mission("pay-per-dream").contains("0 lock-in"));
        assert!(get_mission("swiss-knife").contains("No subscription"));
        assert!(get_mission("gone-rogue").contains("Your agents do"));
        assert!(get_mission("data-heist").contains("buffet"));
        assert!(get_mission("open-protocol").contains("built bridges"));
        assert!(get_mission("pixel-pirate").contains("Yours now"));
        assert!(get_mission("supernovae").contains("Ship it"));
    }

    #[test]
    fn test_boss_level_has_boss_marker() {
        let supernovae = get_mission("supernovae");
        assert!(supernovae.contains("BOSS BATTLE"));
    }

    #[test]
    fn test_missions_have_ascii_art() {
        let slugs = [
            "jailbreak",
            "hot-wire",
            "fork-bomb",
            "root-access",
            "shapeshifter",
            "pay-per-dream",
            "swiss-knife",
            "gone-rogue",
            "data-heist",
            "open-protocol",
            "pixel-pirate",
            "supernovae",
        ];
        for slug in &slugs {
            let mission = get_mission(slug);
            assert!(mission.contains("```"), "{} missing ASCII art block", slug);
            // ASCII art uses box-drawing characters
            assert!(
                mission.contains("\u{2550}") || mission.contains("\u{2588}"),
                "{} missing ASCII art characters",
                slug
            );
        }
    }

    #[test]
    fn test_mission_line_counts_reasonable() {
        let slugs = [
            "jailbreak",
            "hot-wire",
            "fork-bomb",
            "root-access",
            "shapeshifter",
            "pay-per-dream",
            "swiss-knife",
            "gone-rogue",
            "data-heist",
            "open-protocol",
            "pixel-pirate",
            "supernovae",
        ];
        for slug in &slugs {
            let mission = get_mission(slug);
            let lines = mission.lines().count();
            assert!(
                lines >= 80,
                "{} too short: {} lines (minimum 80)",
                slug,
                lines
            );
            assert!(
                lines <= 200,
                "{} too long: {} lines (maximum 200)",
                slug,
                lines
            );
        }
    }
}
