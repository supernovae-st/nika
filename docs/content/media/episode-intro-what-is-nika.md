# Episode 1: What is Nika and Why Does the World Need Another Workflow Engine?

## Metadata

| Field | Value |
|-------|-------|
| **Series** | Building Nika -- A Rust AI Engine from Scratch |
| **Episode** | 01 |
| **Duration** | ~30 minutes |
| **Topics** | Introduction, problem space, 5 verbs, naming, ecosystem |
| **Guest Suggestions** | Thibaut Melen (creator), a Rust systems engineer, an AI workflow practitioner |
| **Audience** | Developers building AI applications, Rust enthusiasts, workflow engine users |
| **Prerequisites** | Basic understanding of YAML, REST APIs, and LLMs |

---

## Cold Open (30 seconds)

[MUSIC: Energetic, slightly mysterious intro -- think Nujabes meets lo-fi tech]

**Host:** Imagine you could describe an entire AI pipeline -- scraping the web, calling Claude, processing images, orchestrating agents -- in a single YAML file, and a Rust binary executes it in parallel, automatically. No Python glue code. No framework lock-in. No 47 dependencies to debug at 2 AM.

[PAUSE]

That is Nika. And today, we are going to tell you why it exists, what it does differently, and why a semantic YAML workflow engine written in 1.56 million lines of Rust might be exactly what the AI world has been missing.

[MUSIC FADES]

---

## Intro (1 minute)

**Host:** Welcome to "Building Nika -- A Rust AI Engine from Scratch." I am [Host Name], and this is a podcast series for developers who are tired of duct-taping AI tools together with Python scripts and hoping nothing breaks in production.

Over the next eight episodes, we are going to deep-dive into Nika -- an open-source workflow engine that takes a radically different approach to AI orchestration. We will cover its five-verb architecture, its Rust internals, its media pipeline, security model, learning system, MCP integration, and its vision for the future of open-source AI.

But first -- episode one. The big question: why does the world need another workflow engine?

Let us start with the problem.

---

## Segment 1: The Problem -- AI Workflows Are a Mess (8 minutes)

**Host:** Let me paint you a picture. It is 2026. You are a developer at a startup, and your boss walks in and says:

"Hey, I need you to build a pipeline. It should scrape Hacker News every morning, use an LLM to analyze trends, generate a report, create a thumbnail image, and send it to Slack. Oh, and make it reliable. And cheap. And can we switch from OpenAI to Claude next month without rewriting everything?"

[PAUSE]

So you start building. And you hit the wall. The same wall every AI developer hits.

**Problem 1: The Glue Code Nightmare**

You write a Python script. It calls the OpenAI API. Then it calls requests for scraping. Then Pillow for image processing. Then the Slack SDK. Each step has its own error handling, its own retry logic, its own authentication. Your "pipeline" is really just a linear script with try-except blocks every three lines.

[EMPHASIS] And here is the thing -- every step depends on the previous one, but you have no formal way to express that. If the scraping fails, does the LLM step still run? If you add a new step, do you have to restructure the entire script? The answer is usually "yes, painfully."

**Problem 2: Vendor Lock-In**

You chose OpenAI because GPT-4o was the best at the time. Now Claude is better for your use case. But switching means rewriting every API call, every prompt format, every response parser. Your pipeline is not a pipeline -- it is an OpenAI client with extra steps.

[CODE EXAMPLE]
```python
# This is what most "AI pipelines" look like in 2026
import openai  # locked in
response = openai.chat.completions.create(
    model="gpt-4o",  # hardcoded
    messages=[{"role": "user", "content": prompt}]
)
# Now do 47 more lines of parsing, error handling, and prayer
```

**Problem 3: No Parallelism by Default**

Your scraping step and your image processing step do not depend on each other. They could run in parallel. But in a Python script, everything runs sequentially unless you manually add asyncio or threading. And even then, you are managing concurrency yourself -- a task that professional distributed systems engineers get wrong.

**Problem 4: Observability is an Afterthought**

When something fails in production at 3 AM, you get a stack trace. Maybe. If you remembered to add logging. You have no structured trace of what ran, what succeeded, what failed, how long each step took, or how many tokens you burned. You are flying blind.

[PAUSE]

**Host:** Now, I just described four problems. And if you are an experienced developer, you are probably thinking: "So use Airflow. Or Prefect. Or LangGraph. Or CrewAI."

And you would be half right. These tools solve some of these problems. But they introduce new ones.

Let me show you where Nika sits on the map.

[CODE EXAMPLE -- Competitive landscape]
```
Declarative + Complex  <-- Nika sits here. Alone.

                         Declarative
                            ^
                            |
                  Dify      |      NIKA
                            |
    Simple <----------------+----------------> Complex
                            |
                 CrewAI      |     LangGraph
                            |
                         Imperative
```

Dify is declarative but simple -- it is a visual drag-and-drop builder. Great for non-developers, but limited when you need real complexity. LangGraph is complex but imperative -- you write Python state graphs, which gives you power but zero portability. CrewAI is in between -- role-based agents with YAML configuration, but the execution is still Python underneath.

[EMPHASIS] Nika occupies a unique position: declarative AND complex. You write YAML, but the YAML is expressive enough to handle production-grade AI workflows. The engine is Rust, so execution is fast, parallel, and memory-safe. And the YAML is version-controllable, diffable, and auditable -- because it is just text files.

---

## Segment 2: The Solution -- Five Verbs and Semantic YAML (8 minutes)

**Host:** So what does Nika actually look like? Let me show you the simplest possible workflow.

[CODE EXAMPLE]
```yaml
# hello.nika.yaml
schema: nika/workflow@0.12

tasks:
  - id: greet
    infer: "Say hello in the style of a pirate captain"
```

That is it. One file. One task. One verb: `infer:`. Nika reads this YAML, resolves which LLM provider you have configured (it checks your environment variables in priority order: Anthropic, OpenAI, Mistral, Groq, DeepSeek, Gemini), sends the prompt, and prints the result.

[PAUSE]

But here is where it gets interesting. Nika has exactly five verbs. Not three. Not ten. Five.

[EMPHASIS] And the number five is not arbitrary. It is the result of careful design work -- mapping every action an AI workflow might need to perform into the smallest possible set of orthogonal operations.

Let me walk through them.

**Verb 1: `infer:`** -- LLM generation. This is your text generation, your reasoning, your structured output. It supports 22 LLM providers, multimodal vision with images, extended thinking for Claude, temperature control, and structured JSON output with a five-layer validation defense. We will go deep on this in Episode 2.

**Verb 2: `exec:`** -- Shell commands. Run any command on the host machine. But -- and this is critical -- with a security blocklist that prevents command injection, fork bombs, privilege escalation, and reverse shells. It even normalizes Unicode to prevent confusable character bypass attacks. More on security in Episode 5.

**Verb 3: `fetch:`** -- HTTP requests. Make any HTTP call. But Nika goes further with nine extraction modes: you can pull clean Markdown from any webpage, extract article content using Readability, parse RSS feeds, extract metadata, classify links, query JSON APIs with JSONPath, and even discover AI-era content via llm.txt files. It is not just "fetch a URL" -- it is "fetch and understand."

**Verb 4: `invoke:`** -- MCP tool calls. This is how Nika talks to external tools using the Model Context Protocol. It can call 24 built-in tools (image processing, file operations, logging) and any external MCP server. With 100+ pre-configured aliases for popular services like GitHub, Slack, and Notion.

**Verb 5: `agent:`** -- Multi-turn autonomous loops. This is the agentic layer. An agent gets tools, a goal, guardrails, and runs in a loop until it achieves its objective or hits a limit. It supports spawning sub-agents, extended thinking, streaming, and quality gates.

[PAUSE]

**Host:** Now, here is the key insight. With these five verbs, you can compose any AI workflow. Let me show you a real-world example -- that morning report pipeline the boss asked for.

[CODE EXAMPLE]
```yaml
# morning-report.nika.yaml
schema: nika/workflow@0.12

tasks:
  - id: scrape_hn
    fetch:
      url: https://hacker-news.firebaseio.com/v0/topstories.json
      method: GET

  - id: scrape_reddit
    fetch:
      url: https://www.reddit.com/r/artificial/top.json?t=week
      method: GET
      headers:
        User-Agent: "NikaBot/1.0"

  - id: analyze
    depends_on: [scrape_hn, scrape_reddit]
    with:
      hn: "$scrape_hn"
      reddit: "$scrape_reddit"
    infer: |
      Analyze these sources for AI trends:
      HackerNews: {{with.hn}}
      Reddit: {{with.reddit}}
      Extract: top 5 topics, sentiment, emerging themes.
    structured:
      schema:
        type: object
        properties:
          topics: { type: array, items: { type: string } }
          sentiment: { type: string }
        required: [topics, sentiment]

  - id: generate_report
    depends_on: [analyze]
    with:
      data: "$analyze"
    infer: "Write a morning briefing based on: {{with.data | to_json}}"

  - id: create_thumbnail
    depends_on: [analyze]
    invoke:
      tool: nika:chart
      args:
        type: bar
        data: "{{with.data.topics}}"

  - id: send_to_slack
    depends_on: [generate_report, create_thumbnail]
    fetch:
      url: https://hooks.slack.com/services/YOUR/WEBHOOK
      method: POST
      body:
        text: "{{with.report}}"
```

[EMPHASIS] Notice several things here. First: `scrape_hn` and `scrape_reddit` have no `depends_on:`. They run in parallel automatically. Nika builds a DAG (Directed Acyclic Graph) from the dependencies and executes independent tasks concurrently using Tokio.

Second: the `with:` block creates typed bindings between tasks. `$scrape_hn` means "the output of the task with id scrape_hn." And you can apply transforms with pipe syntax: `{{with.data | to_json}}`, `{{with.data | uppercase | trim}}`.

Third: `generate_report` and `create_thumbnail` both depend on `analyze` but not on each other. So they also run in parallel.

Fourth: structured output with JSON schema validation. The `analyze` task will not just give you free-form text -- it gives you a validated JSON object with the exact fields you specified. And if the LLM gets it wrong, Nika has a five-layer repair cascade to fix it.

This entire pipeline is one file. You run it with `nika run morning-report.nika.yaml`. You diff it in Git. You review it in a PR. You deploy it with confidence.

---

## Segment 3: The Name, The Philosophy, The Ecosystem (8 minutes)

**Host:** So why is it called Nika? And what is this "SuperNovae" thing?

[PAUSE]

This is where the story gets interesting. And a little nerdy. Bear with me.

In the manga One Piece -- which, if you are not familiar, is the best-selling manga in history with over 500 million copies -- there is a mythical figure called the Sun God Nika. Nika represents liberation, imagination, and the power to make the impossible possible. The hero of the story, Luffy, discovers that his Devil Fruit is actually the Hito Hito no Mi, Model: Nika -- the fruit of the Sun God. And when he awakens this power, he becomes "limited only by his imagination."

[EMPHASIS] The Nika workflow engine carries this philosophy. Its tagline could be: "Limited only by the YAML you write." Five verbs, infinite compositions. The engine does not constrain what you can build -- it gives you the primitives and gets out of the way.

But it goes deeper than just a name. The entire SuperNovae architecture maps onto One Piece's world:

- **Nika** is the body -- the runtime that executes. It does things.
- **NovaNet** is the brain -- a knowledge graph built on Neo4j that remembers things. It stores entities, relationships, locales, and accumulated intelligence.
- **MCP** (Model Context Protocol) is the nervous system that connects them. Nika never touches the database directly. It communicates with NovaNet exclusively through MCP tool calls. This is called the "Zero Cypher Rule" -- zero raw database queries in the workflow engine.

[PAUSE]

Why this separation? Because the body should not need to know how the brain stores information. Just like how you do not need to understand neuroscience to pick up a cup of coffee. The body sends an intent ("remember this entity"), the brain handles the storage, and the protocol ensures they speak the same language.

The data architecture has three tiers, inspired by Vegapunk's Punk Records from the Egghead arc:

[CODE EXAMPLE]
```
HOT:  RunContext (DashMap in RAM)
      Lifetime: one workflow run
      Content: task results, bindings, inputs

WARM: Records (NDJSON on disk)
      Lifetime: configurable TTL (7-90 days)
      Content: compressed run summaries

COLD: NovaNet (Neo4j via MCP)
      Lifetime: permanent
      Content: promoted entities, knowledge atoms
```

This means a workflow run lives in hot memory while it executes. When it finishes, key results are compressed into local NDJSON files. And the most valuable insights get promoted to the permanent knowledge graph in NovaNet, where they can be queried across projects, locales, and time.

**Host:** Let me address the elephant in the room: why Rust?

[PAUSE]

Three reasons.

**One: Performance.** Nika uses Tokio for async execution, which means hundreds of concurrent tasks with zero overhead. The DAG uses Kahn's topological sort with a compact Vec-based adjacency list where 95% of tasks use stack-allocated SmallVec for their dependencies. No heap allocation in the common case. The content-addressable store uses blake3 hashing. Image processing is SIMD-accelerated with Lanczos3 resampling. This is not a wrapper around a Python runtime -- it is a real systems-level engine.

**Two: Safety.** Rust's type system catches entire categories of bugs at compile time. The three-phase AST pipeline (Raw, Analyzed, Lowered) uses Rust's type system to make invalid states unrepresentable. You cannot skip the validation phase because the types literally will not let you. The zero-I/O core (nika-core) has no file system access, no network calls -- it is pure data transformation. This is the kind of guarantee you cannot get in a dynamically typed language.

**Three: Distribution.** Nika ships as a single binary. No runtime dependencies. No virtual environment. No `pip install` dance. You download the binary, you run it. This matters enormously for developer experience and for deployment.

The codebase today is 12 workspace crates totaling 1.56M lines of Rust:

[CODE EXAMPLE]
```
nika           (2K)    CLI entry point
nika-engine    (134K)  Execution engine -- embeddable runtime
nika-core      (23K)   AST, types, catalogs -- zero I/O
nika-event     (4K)    EventLog, TraceWriter
nika-mcp       (9K)    MCP client, rmcp
nika-media     (3.5K)  CAS store, media processor
nika-cli       (8K)    CLI subcommands
nika-tui       (92K)   Terminal UI -- ratatui
nika-lsp-core  (9K)    LSP intelligence
nika-lsp       (2.5K)  LSP binary
```

[EMPHASIS] The TUI alone is 92 thousand lines. That is not a typo. The terminal interface has three views -- Studio, Command, and Control -- with 40+ widgets, a DAG visualizer, and real-time task status. Developer experience is a major component of the Nika codebase.

And the test suite has 8,300+ tests. With zero clippy warnings. Zero.

**Host:** Now, I want to be transparent about something. As of today, Nika has zero users. This is a pre-launch project. The creator, Thibaut Melen, has deliberately chosen to focus on engineering quality over marketing. The philosophy is: build something genuinely excellent, then release it to the world. Not the other way around.

But that also means something important: there is zero backward compatibility baggage. Only schema @0.12 matters. The version stays 0.x.x intentionally -- there is no "1.0" milestone. This is a rare luxury in software development, and Nika uses it to prioritize correctness and evolution over stability. The codebase is what you get when you optimize for quality without the constraint of not breaking existing users.

---

## Segment 4: What is a Nika Workflow, Really? (4 minutes)

**Host:** Before we wrap up, let me give you a mental model for what a Nika workflow actually is under the hood.

A `.nika.yaml` file goes through three phases:

**Phase 1 -- Raw AST.** The YAML is parsed into a raw abstract syntax tree. Everything is optional. Source spans are preserved for error messages. This is the "accept anything that looks like YAML" phase.

**Phase 2 -- Analyzed AST.** The raw AST is validated and transformed. Task IDs are interned for O(1) comparison. Dependencies are resolved. Provider configurations are checked. Bindings are verified. This is where Nika catches errors like circular dependencies, missing tasks, invalid provider names. If your workflow has a problem, it fails here, before any code runs, with a precise error message pointing to the exact line in your YAML.

**Phase 3 -- Lowered.** The analyzed AST is converted into runtime types. This is the representation the executor actually uses. It is optimized for execution, not for human readability.

Then the executor builds an IndexedDag using Kahn's algorithm, computes topological order and depth, and runs tasks in parallel using Tokio's JoinSet with CancellationToken support for fail-fast behavior.

[EMPHASIS] This three-phase design is why you can trust Nika with production workflows. The validation phase catches problems that other tools only discover at runtime. It is the same design philosophy as a real compiler -- parse, analyze, lower, execute.

---

## Wrap-up & Preview (2 minutes)

**Host:** Let me summarize what we covered today.

Nika is a semantic YAML workflow engine for AI tasks, written in Rust. It solves four fundamental problems with AI workflows: glue code, vendor lock-in, missing parallelism, and poor observability.

It does this with five verbs -- infer, exec, fetch, invoke, agent -- that compose into DAG-scheduled workflows with typed bindings, structured output, and full event tracing.

It is part of a larger ecosystem: Nika (the body that executes), NovaNet (the brain that remembers), and MCP (the protocol that connects them).

And it is named after the Sun God of liberation from One Piece, because the philosophy is the same: give people the primitives and get out of their way.

[PAUSE]

In the next episode, we are going to go deep on the five verbs. We will look at real YAML examples for each one, explore multimodal vision with `infer:`, the nine extraction modes of `fetch:`, the 24 built-in tools of `invoke:`, and the autonomous agent loop. We will also answer the design question: why exactly five verbs? Why not three? Why not ten?

Until then, you can find Nika at [github.com/supernovae-st](https://github.com/supernovae-st) and [qrcode-ai.com](https://qrcode-ai.com).

[MUSIC: Outro theme]

**Host:** This has been "Building Nika -- A Rust AI Engine from Scratch." See you next episode.

---

## Show Notes

### Links
- Nika GitHub: [github.com/supernovae-st](https://github.com/supernovae-st)
- QR Code AI: [qrcode-ai.com](https://qrcode-ai.com)
- Model Context Protocol (MCP): [modelcontextprotocol.io](https://modelcontextprotocol.io)
- One Piece (Nika / Sun God): [onepiece.fandom.com/wiki/Nika](https://onepiece.fandom.com/wiki/Nika)

### Key Concepts Mentioned
- **Semantic YAML** -- YAML that carries meaning, not just configuration
- **DAG (Directed Acyclic Graph)** -- Dependency graph that enables automatic parallelism
- **5 Verbs** -- infer, exec, fetch, invoke, agent
- **Three-Phase AST** -- Raw, Analyzed, Lowered
- **MCP (Model Context Protocol)** -- Standard protocol for AI tool communication
- **CAS (Content-Addressable Storage)** -- File storage indexed by content hash
- **rig-core** -- Rust LLM framework used by Nika for provider abstraction

### Competitive Landscape Referenced
- LangGraph (Python, imperative state graphs)
- CrewAI (role-based multi-agent, YAML config)
- Dify (visual workflow builder)
- AutoGen (conversational agents)
- Slate by Random Labs (thread-based context management)

### Tech Stack
- **Language:** Rust
- **Async Runtime:** Tokio
- **LLM Framework:** rig-core v0.32
- **MCP Client:** rmcp v0.16
- **TUI Framework:** ratatui
- **LSP:** tower-lsp-server 0.23
- **Hashing:** blake3
- **Testing:** insta (snapshot testing)
