# Keynote: Nika -- AI Workflows, Liberated

> 10-minute keynote presentation
> Target audience: developers, AI engineers, open-source enthusiasts
> Format: slide deck with speaker notes, live demo segment

---

## Pre-Show Technical Requirements

- Terminal with Nika TUI running (dark theme, 120x40 minimum)
- VS Code with `.nika.yaml` file open (LSP active, completions visible)
- Split screen capability for live demo
- Backup video recordings of each demo segment

---

## SLIDE 1 -- Title Card

[SLIDE] Black screen. Butterfly silhouette slowly materializes in deep purple.

**Visual:** The Nika butterfly logo, centered. Below it: "AI Workflows, Liberated." Below that, smaller: "nika/workflow@0.12 -- v0.42"

**Speaker Notes:**
Good morning. I want to start with a question. How many of you have written an AI workflow this week? And how many of you actually enjoyed the experience?

[PAUSE 3 seconds]

---

## SLIDE 2 -- The Problem

[SLIDE] Split screen. Left: a wall of Python code (LangChain boilerplate). Right: a wall of JavaScript (CrewAI config).

**Visual:** Both sides are visually overwhelming. Red error highlights scattered throughout. Line count indicators show 200+ lines on each side. A subtle counter in the corner: "Just to call two APIs and summarize the results."

**Speaker Notes:**
This is what AI workflow tooling looks like today. Two hundred lines of Python to chain two API calls. Import statements that look like a dependency graveyard. Configuration objects nested six levels deep. And when something breaks -- and it always breaks -- you are staring at a stack trace from a framework you did not write, in a language that was not designed for concurrency, running on a runtime that was not built for reliability.

---

## SLIDE 3 -- The Cost of Complexity

[SLIDE] Three statistics appear one at a time, large bold text:

1. "73% of LLM apps never make it past prototype" -- [source attribution]
2. "Average setup time for a multi-model pipeline: 4 hours"
3. "Most AI frameworks: 0 type safety at the workflow level"

**Speaker Notes:**
The tooling problem is not abstract. It has real costs. Most LLM applications die in prototype because the framework friction is higher than the application complexity. Setting up a multi-model pipeline -- something that should take minutes -- takes hours of dependency management and configuration juggling. And none of these frameworks give you type safety where it matters most: at the workflow level, where your data flows between tasks.

---

## SLIDE 4 -- What If

[SLIDE] Clean black background. Single line of white text, centered:

```
What if a workflow engine respected your intelligence?
```

**Speaker Notes:**
What if there were a workflow engine that did not assume you needed training wheels? One that was as expressive as code, as readable as configuration, and as reliable as a compiled program? One built from the ground up for the AI era -- not bolted onto a web framework or a notebook?

---

## SLIDE 5 -- Introducing Nika

[ANIMATION] The butterfly logo unfolds from the center. Title appears letter by letter: "Nika"

**Visual:** Full-screen Nika logo with the tagline "Semantic YAML Workflow Engine for AI Tasks." Below: "12 crates. 451K lines. 8,300+ tests. Zero compromises."

**Speaker Notes:**
This is Nika. A semantic YAML workflow engine for AI tasks, written entirely in Rust. Twelve workspace crates. Four hundred and fifty-one thousand lines of production code. Over eight thousand three hundred tests. And a zero-warnings policy enforced by CI on every commit. Nika is not a prototype. It is not a wrapper around somebody else's library. It is a ground-up engineering effort to build the workflow engine that AI development deserves.

---

## SLIDE 6 -- The Name

[SLIDE] Split visual: left side shows the Nika butterfly symbol; right side shows the Sun God Nika from One Piece mythology.

**Visual:** Subtle animation connecting the two -- threads of light connecting the mythological figure to the butterfly.

**Speaker Notes:**
The name Nika comes from the Sun God Nika in One Piece -- a figure whose power is limited only by imagination. That is the design philosophy. Nika the engine is limited only by the YAML you write. It is the body that executes. Its partner, NovaNet, is the brain that knows. Together they form the SuperNovae architecture: knowing and doing, connected by the MCP protocol.

---

## SLIDE 7 -- Five Verbs

[ANIMATION] Five cards fly in from different angles, landing in a row:

```
infer:     exec:     fetch:     invoke:     agent:
```

**Visual:** Each card has an icon and a one-word description:
- infer: brain icon -- "Generate"
- exec: terminal icon -- "Execute"
- fetch: globe icon -- "Request"
- invoke: plug icon -- "Connect"
- agent: loop icon -- "Orchestrate"

**Speaker Notes:**
Everything in Nika starts with five verbs. That is it. Five verbs to express any AI workflow. `infer:` for LLM generation -- with vision, extended thinking, structured output, and guardrails built in. `exec:` for shell commands -- with security blocklists and timeout enforcement. `fetch:` for HTTP requests -- with nine extraction modes from raw HTML to clean Markdown. `invoke:` for MCP tool calls -- connecting to any external system through the Model Context Protocol. And `agent:` for multi-turn agentic loops -- with guardrails, completion modes, and human-in-the-loop support.

Five verbs. Not fifty helper classes. Not a hundred utility functions. Five verbs that compose.

---

## SLIDE 8 -- The Simplest Workflow

[SLIDE] A clean YAML block appears, syntax-highlighted:

```yaml
schema: nika/workflow@0.12

tasks:
  - id: greet
    exec:
      command: echo "Hello from Nika"
```

**Visual:** Four lines of YAML. Clean syntax highlighting. No boilerplate. A small annotation arrow points to the schema line: "Type-safe from line 1."

**Speaker Notes:**
This is a complete Nika workflow. Four lines. A schema declaration that gives you LSP completions and validation. A task with an ID. An exec verb with a command. No imports. No setup. No framework initialization. You write YAML, Nika executes it. And that schema declaration is not cosmetic -- it powers a full Language Server Protocol implementation with completions, diagnostics, and hover documentation in your editor.

---

## SLIDE 9 -- Composition

[SLIDE] A more complex workflow appears, building from the previous:

```yaml
schema: nika/workflow@0.12

tasks:
  - id: fetch_news
    fetch:
      url: https://news.ycombinator.com
      extract: article

  - id: summarize
    depends_on: [fetch_news]
    with: { article: $fetch_news }
    infer:
      model: claude/claude-sonnet-4-20250514
      prompt: |
        Summarize this article in 3 bullet points:
        {{with.article}}
```

**Visual:** Arrows animate between the two tasks showing data flow. The `depends_on` and `with:` bindings light up to show the connection.

**Speaker Notes:**
Composition is where Nika starts to shine. Here we fetch a web page, extract its article content using Readability-style parsing, then pass it to Claude for summarization. The `depends_on:` declaration creates a DAG edge. The `with:` binding creates a typed data flow. The `{{with.article}}` template resolves at runtime. This is not string concatenation -- it is a validated, type-checked data pipeline. Nika builds a Directed Acyclic Graph from your dependencies, runs Kahn's topological sort, and executes tasks in maximum-parallelism order. If two tasks have no dependency between them, they run concurrently. Automatically.

---

## SLIDE 10 -- Architecture Overview

[ANIMATION] The full architecture diagram builds itself piece by piece, from top to bottom:

```
.nika.yaml
    |
    v
Three-Phase AST: Raw -> Analyzed -> Lowered
    |
    v
IndexedDag (Kahn's topological sort)
    |
    v
Executor (parallel task execution)
    |
    |-- infer  -> 8 LLM providers via rig-core
    |-- exec   -> Shell (blocklist + NFKC normalization)
    |-- fetch  -> HTTP (9 extraction modes)
    |-- invoke -> MCP Client (rmcp 0.16)
    |-- agent  -> Agentic Loop (guardrails + completion)
    |
    v
RunContext (task results + CAS media)
```

**Visual:** Each box animates in with a subtle glow. Lines draw themselves between boxes. The five verb branches fan out simultaneously.

**Speaker Notes:**
Under the hood, Nika runs a compiler-grade pipeline. Your YAML goes through a three-phase AST transformation inspired by the Rust compiler itself: Raw parsing with source spans, then Analysis with semantic validation, then Lowering to runtime types. The analyzed AST feeds into an immutable DAG. Once constructed, that graph is frozen -- enabling safe concurrent execution via Tokio. The executor dispatches to the five verbs, each optimized for its domain. And results land in the RunContext -- a DashMap-backed concurrent store that tasks can reference through bindings.

---

## SLIDE 11 -- Twelve Crates

[SLIDE] Visual of the workspace structure as interconnected nodes:

```
nika (2k)  ->  nika-engine (134k)  ->  nika-core (23k)
                    |                        |
                    v                        v
              nika-tui (92k)          nika-event (4k)
              nika-mcp (9k)           nika-media (3.5k)
              nika-cli (8k)           nika-daemon (5k)
              nika-init (21k)
              nika-lsp-core (9k)  ->  nika-lsp (2.5k)
```

**Visual:** Each crate is a circle with its name and line count. Lines show dependencies. The whole structure pulses gently to suggest a living system.

**Speaker Notes:**
Nika is not one monolithic binary. It is twelve workspace crates with clear boundaries. `nika-core` is zero-I/O -- pure types and AST definitions. `nika-engine` is the embeddable runtime -- you can integrate it into any Rust application. `nika-tui` is a full terminal UI with three views and forty-plus widgets. `nika-mcp` wraps the rmcp client for MCP protocol communication. `nika-media` provides content-addressable storage with blake3 hashing and zstd compression. `nika-daemon` handles background services. `nika-init` handles project scaffolding. And the LSP crates give you editor intelligence without running the full engine. Every boundary is intentional. Every dependency arrow has a reason.

---

## SLIDE 12 -- Live Demo: Title Card

[SLIDE] "LIVE DEMO" text appears with a terminal cursor blinking.

**Visual:** Clean black background. "Let's build a workflow." in monospace font.

**Speaker Notes:**
Enough slides. Let me show you Nika in action.

---

## SLIDE 13 -- Live Demo: Writing the Workflow

[SCREEN] Switch to VS Code with an empty `.nika.yaml` file.

**Demo Script:**
1. Type `schema: nika/workflow@0.12` -- [PAUSE] show LSP validation checkmark appears
2. Type `tasks:` -- show completion dropdown offering task structure
3. Add first task:
```yaml
  - id: get_weather
    fetch:
      url: https://wttr.in/Paris?format=j1
      extract: jsonpath
      selector: "$.current_condition[0]"
```
4. [ZOOM] on the `extract: jsonpath` -- explain 9 extraction modes in one field
5. Add second task:
```yaml
  - id: describe
    depends_on: [get_weather]
    with: { weather: $get_weather }
    infer:
      model: claude/claude-haiku-3-5-20241022
      prompt: |
        Current weather in Paris: {{with.weather}}
        Write a poetic one-line description.
```
6. [ZOOM] on `with:` binding -- explain the `$task_id` reference syntax

**Speaker Notes:**
Watch the LSP working as I type. Schema validation happens in real-time. Now I am adding a fetch task -- it will hit the weather API and extract just the current conditions using JSONPath. Nine extraction modes available: markdown, article, text, selector, metadata, links, jsonpath, feed, and llm_txt. One field, one choice. Now the second task depends on the first -- Nika will schedule them in order automatically. The `with:` binding creates a typed reference. Dollar-sign prefix for task IDs. Double-brace templates for resolution. This is not stringly-typed glue code -- it is a validated data pipeline.

---

## SLIDE 14 -- Live Demo: Running

[SCREEN] Switch to terminal.

**Demo Script:**
1. Run `nika check demo.nika.yaml` -- show validation output (green checkmarks)
2. Run `nika run demo.nika.yaml` -- show execution output
3. [ZOOM] on the timing information, task ordering, and result output

**Speaker Notes:**
First, `nika check` validates the workflow without executing it. Schema validation, DAG cycle detection, binding resolution -- all checked ahead of time. Green across the board. Now `nika run` -- watch the execution order. The fetch task runs first, the infer task waits for its result, and we get a poetic weather description. Total execution time: under two seconds. No cold start. No framework initialization overhead. Rust binary, straight to execution.

---

## SLIDE 15 -- Live Demo: The TUI

[SCREEN] Run `nika ui` and navigate to a more complex workflow.

**Demo Script:**
1. Launch `nika ui` -- show the Home view with file browser
2. Navigate to a multi-task workflow
3. Switch to Studio view (`1/s`) -- show the YAML editor with syntax highlighting
4. Switch to Monitor view -- show the live DAG visualization
5. [ZOOM] on the DAG node boxes showing task status, timing, token counts

**Visual Description for backup video:**
- Home view: file tree on left, workflow preview on right, status bar at bottom
- Studio view: split-pane YAML editor with syntax highlighting and LSP inline
- Monitor view: animated DAG with colored nodes (green=done, blue=running, gray=pending)

**Speaker Notes:**
This is the Nika TUI. Three views for three workflows. Home for browsing and selecting. Studio for editing -- that is a full syntax-highlighted YAML editor with LSP integration running in your terminal. And Monitor for watching execution in real-time. See the DAG visualization? Each node is a task. Green means completed, blue means running, gray means pending. Token counts, timing, cost tracking -- all live. Forty-plus custom widgets. Ninety-two thousand lines of ratatui code. This is not a toy dashboard -- it is a mission control cockpit.

---

## SLIDE 16 -- Nine Providers

[SLIDE] Grid of provider logos with connection lines to Nika:

```
Claude    OpenAI    Mistral    Groq
DeepSeek  Gemini    xAI        Native (local GGUF)
```

**Visual:** Each logo animates in. The "Native" entry has a special glow -- it represents local execution.

**Speaker Notes:**
Nika supports nine LLM providers out of the box. Eight cloud providers through rig-core -- Claude, OpenAI, Mistral, Groq, DeepSeek, Gemini, and xAI. And a native provider through mistral.rs for running GGUF models locally, completely offline. Vision support across all providers that support it. Extended thinking for Claude. Streaming for all of them. Structured output with a five-layer defense system. One workflow, any provider. Switch models by changing one line.

---

## SLIDE 17 -- Structured Output

[SLIDE] The five-layer defense diagram:

```
Layer 0: Provider-native (DynamicSubmitTool)
Layer 1: Extractor
Layer 2: Extract + Validate
Layer 3: Retry
Layer 4: LLM Repair
```

**Visual:** Five horizontal bars stacking up like a shield. Each layer lights up as it is mentioned. An arrow tries to pass through -- representing an invalid output -- and gets caught at different layers.

**Speaker Notes:**
Structured output is where most frameworks fail silently. Nika has a five-layer defense system. Layer zero tries the provider's native structured output. If that fails, layer one extracts JSON from the response. Layer two validates against your schema. Layer three retries with a better prompt. Layer four asks the LLM to repair its own output. Five layers. Automatic escalation. Zero configuration. You define a JSON schema, Nika guarantees conformance.

---

## SLIDE 18 -- Media Pipeline

[SLIDE] Visual of the 24 media tools organized in three tiers:

```
Tier 1 (Always-on):  import, dimensions, thumbhash, dominant_color, pipeline
Tier 2 (Default):    thumbnail, convert, strip, metadata, optimize, svg_render
Tier 3 (Opt-in):     phash, compare, pdf_extract, chart, provenance, verify,
                     qr_validate, quality, html_to_md, css_select, ...
```

**Visual:** Three horizontal bands in different shades. Tools appear as small icons within each band. Lines connect them showing pipeline flow.

**Speaker Notes:**
Nika ships with twenty-four builtin media tools -- and they are not wrappers around ImageMagick. SIMD-accelerated thumbnail generation with Lanczos3. Perceptual image hashing for deduplication. C2PA content provenance for EU AI Act compliance. QR code validation with scan scoring. All backed by a content-addressable storage system using blake3 hashing and zstd compression. Import a file once, reference it by hash forever, chain operations without intermediate files.

---

## SLIDE 19 -- The Course

[SLIDE] The 12-level course visualized as a constellation map:

```
 1. Jailbreak          7. Swiss Knife
 2. Hot Wire           8. Gone Rogue
 3. Fork Bomb          9. Data Heist
 4. Root Access       10. Open Protocol
 5. Shapeshifter      11. Pixel Pirate
 6. Pay-Per-Dream     12. SuperNovae (Boss)
```

**Visual:** Stars connected by lines forming a constellation. Completed levels glow. The final "SuperNovae" star is larger and brighter. 44 exercises total shown as dots along the paths.

**Speaker Notes:**
Learning Nika is not reading documentation and hoping for the best. We built a twelve-level interactive course with forty-four exercises. Each level has a liberation theme -- from Jailbreak, where you learn basic exec commands, all the way to SuperNovae, the final boss level where you orchestrate everything into a production workflow. Progressive hints. Auto-checking on file save. A constellation progress map. `nika init --course` generates the entire thing. `nika course next` opens your next exercise. `nika course watch` validates as you code. This is not a tutorial -- it is a training program.

---

## SLIDE 20 -- MCP Integration

[SLIDE] Diagram showing Nika connecting to NovaNet through MCP:

```
Nika Workflow --invoke:--> NovaNet MCP --Cypher--> Neo4j
```

**Visual:** Three boxes with arrows. The middle box (MCP) glows as a bridge. A "Zero Cypher" badge appears over the Nika box.

**Speaker Notes:**
Nika connects to the outside world through the Model Context Protocol. The invoke verb calls any MCP tool -- and our primary integration is NovaNet, a knowledge graph powered by Neo4j. But here is the critical design decision: zero Cypher in Nika. Your workflows never write database queries. They invoke semantic tools. NovaNet handles the graph operations. Nika handles execution. Clean boundaries. The MCP protocol is the contract between them. This architecture means you can swap NovaNet for any MCP-compatible backend without changing a single workflow.

---

## SLIDE 21 -- Security

[SLIDE] Four security pillars appear as shield icons:

```
1. Command Blocklist + NFKC normalization
2. Path traversal validation for all file operations
3. Import size limits (50 MB default)
4. SVG sanitization before parsing
```

**Speaker Notes:**
Security is not an afterthought. The exec verb runs a command blocklist before any shell execution, with Unicode NFKC normalization to prevent homoglyph attacks. Every file import validates paths against traversal attacks. Media imports enforce size limits -- fifty megabytes by default. SVG files are sanitized before parsing to prevent XXE and script injection. The PolicyEnforcer runs at boot time, before your first task even starts. Security is a compile-time guarantee, not a runtime hope.

---

## SLIDE 22 -- Event Sourcing

[SLIDE] An NDJSON log scrolling in real time, showing 41 event types:

```json
{"kind":"TaskStarted","task_id":"fetch_news","timestamp":"..."}
{"kind":"FetchComplete","task_id":"fetch_news","status":200,"duration_ms":342}
{"kind":"TaskStarted","task_id":"summarize","timestamp":"..."}
{"kind":"InferStream","task_id":"summarize","chunk":"The article..."}
```

**Visual:** Log lines appear one at a time, scrolling upward. Each event kind is color-coded. A sidebar shows the count: "41 event types."

**Speaker Notes:**
Every action in Nika emits events. Forty-one event types. Append-only event log. NDJSON trace files for full replay capability. When a workflow fails at three in the morning, you do not guess what happened -- you replay the event stream. Task started, fetch completed with status code and duration, inference streaming chunk by chunk, agent loop iterations, guardrail evaluations. Full observability. Zero instrumentation burden on workflow authors.

---

## SLIDE 23 -- Error Codes

[SLIDE] The error code ranges displayed as a spectrum:

```
NIKA-000 to NIKA-319
  000-009: Workflow       100-109: MCP
  010-019: Schema         110-119: Agent
  020-029: DAG            200-214: File tools
  030-039: Provider       251-259: Media
  050-059: Security       300-309: Structured output
  060-069: Output         310-319: Course
```

**Visual:** A horizontal bar divided into colored segments, each representing an error code range. The bar fills from left to right as ranges are mentioned.

**Speaker Notes:**
When something goes wrong, Nika tells you exactly what. Three hundred and twenty distinct error codes organized by category. NIKA-020 through 029 for DAG issues. NIKA-110 through 119 for agent problems. NIKA-251 through 259 for media pipeline errors. Every error includes the source span -- the exact line in your YAML where the problem originated. No "something went wrong." No generic stack traces. Structured, searchable, debuggable error codes.

---

## SLIDE 24 -- The Showcase

[SLIDE] A scrolling gallery of 115 showcase workflows:

**Visual:** Tile grid showing workflow categories: fetch patterns, LLM workflows, media pipelines, infrastructure automation, advanced patterns. Each tile has a preview snippet.

**Speaker Notes:**
Not sure where to start? Nika ships with one hundred and fifteen showcase workflows. `nika showcase list` to browse. `nika showcase extract` to pull one into your project. Fetch patterns, LLM workflows, media pipelines, infrastructure automation -- curated examples for every use case. They are not documentation examples -- they are working, tested workflows you can run today.

---

## SLIDE 25 -- Performance

[SLIDE] Benchmark comparisons (Nika vs Python frameworks):

```
Startup time:     Nika: <10ms      LangChain: ~2s     CrewAI: ~1.5s
Memory (idle):    Nika: ~8MB       LangChain: ~120MB  CrewAI: ~90MB
Parallel tasks:   Nika: native     LangChain: asyncio CrewAI: threads
Type safety:      Nika: compile    LangChain: runtime CrewAI: runtime
```

**Visual:** Bar charts for each metric. Nika's bars are dramatically smaller/faster than competitors.

**Speaker Notes:**
Let me talk about performance. Nika starts in under ten milliseconds. Not seconds -- milliseconds. The idle memory footprint is around eight megabytes. Parallel task execution is native Tokio -- not asyncio, not threads, not processes. Type safety is enforced at compile time and at YAML analysis time -- before your workflow ever runs. This is what happens when you build a workflow engine in Rust instead of wrapping Python libraries.

---

## SLIDE 26 -- Open Source

[SLIDE] The AGPL-3.0 license badge, large and centered.

**Visual:** The AGPL logo with the text: "Free as in freedom. AGPL-3.0-or-later."

**Speaker Notes:**
Nika is licensed under AGPL-3.0-or-later. Not MIT. Not Apache. AGPL. Because open source means protecting the commons, not donating your work to cloud platforms that will wrap it in a proprietary service and sell it back to you. If you use Nika, you contribute back. If you build on Nika, you share your improvements. That is the deal. Freedom for everyone, not just corporations.

---

## SLIDE 27 -- The Ecosystem

[SLIDE] The full SuperNovae architecture:

```
NovaNet (Brain)         MCP Protocol          Nika (Body)
+-- Knowledge Graph  <------------------->  +-- YAML Workflows
+-- NodeClasses                              +-- 5 Verbs
+-- ArcClasses                               +-- DAG Execution
+-- MCP Tools                                +-- Inference Backends
```

**Visual:** Two large circles connected by a glowing bridge (MCP). Left circle: brain imagery with graph nodes. Right circle: butterfly imagery with workflow arrows.

**Speaker Notes:**
Nika does not exist in isolation. It is one half of the SuperNovae architecture. NovaNet is the brain -- a knowledge graph with fifty-nine node classes, storing entities, relationships, and semantic knowledge in Neo4j. Nika is the body -- executing workflows, calling LLMs, processing media. The MCP protocol is the nervous system connecting them. Know something? NovaNet. Do something? Nika. This separation is not accidental -- it is the core architectural decision that makes the whole system composable.

---

## SLIDE 28 -- What is Coming

[SLIDE] Roadmap preview:

```
Wave 1: Model routing (4-preset system)
Wave 2: Orchestration mode + context budgeting
Wave 3: Punk Records (3-tier memory: HOT -> WARM -> COLD)
```

**Visual:** Three waves washing onto a shore, each carrying feature names. The metaphor: each wave builds on the previous.

**Speaker Notes:**
Where is Nika going? Three waves. Wave one brings four-preset model routing -- default, lite, think, and coder slots that let you assign the right model to the right cognitive task. Wave two introduces orchestration mode -- dynamic satellite dispatch where an LLM decides which tasks to run based on the current state. Wave three delivers Punk Records -- a three-tier memory system from hot in-memory cache through warm NDJSON on disk to cold promotion into NovaNet's knowledge graph. The DAG is the kernel. We are upgrading it.

---

## SLIDE 29 -- Getting Started

[SLIDE] Three commands, large and centered:

```bash
cargo install nika
nika init --course
nika course next
```

**Visual:** Each command appears one at a time with a brief animation. Below each: what it does.

**Speaker Notes:**
Getting started takes three commands. Install from crates.io. Initialize the course. Open your first exercise. Or if you already know what you are doing: `nika init --minimal` gives you five workflows, one per verb, ready to customize. `nika showcase list` gives you two hundred working examples. The documentation is built into the tool.

---

## SLIDE 30 -- Call to Action

[SLIDE] The butterfly logo, full screen, with GitHub URL:

```
github.com/supernovae-st/nika
```

**Visual:** The butterfly logo pulses gently. Below the URL: "Star it. Fork it. Build with it." The AGPL badge in the corner.

**Speaker Notes:**
Nika is open source. It is available today. The code is real, the tests pass, the engine works. We are not selling a vision -- we are shipping a product. Star the repo if this resonates. Open an issue if you find a bug. Submit a PR if you want to contribute. And if you just want to play -- run `nika init --course` and start with Jailbreak. The first level takes five minutes. By the end of the twelve levels, you will be orchestrating multi-model AI pipelines with confidence.

---

## SLIDE 31 -- Closing

[SLIDE] Black background. The butterfly slowly fades in, then transforms into the SuperNovae logo.

**Visual:** Text appears below: "Limited only by the YAML you write."

**Speaker Notes:**
Nika. The Sun God's fruit of execution. Limited only by imagination. Or in our case -- limited only by the YAML you write.

Thank you.

[PAUSE for applause]

---

## SLIDE 32 -- Q&A Card

[SLIDE] "Questions?" with contact information:

```
@supernovae-st on GitHub
@ThibautMelen
supernovae.studio
```

**Visual:** Simple, clean. The butterfly logo in watermark behind the text.

---

## Timing Breakdown

| Segment | Slides | Duration |
|---------|--------|----------|
| Opening hook (The Problem) | 1-4 | 1:30 |
| Introducing Nika | 5-9 | 2:00 |
| Architecture deep dive | 10-11 | 1:00 |
| Live demo | 12-15 | 3:00 |
| Features tour | 16-23 | 2:00 |
| Ecosystem and roadmap | 24-29 | 1:00 |
| Close and CTA | 30-32 | 0:30 |
| **Total** | **32** | **~11:00** |

---

## Backup Slides

### BACKUP A -- Fetch Extraction Modes

```
markdown   article   text       selector
metadata   links     jsonpath   feed     llm_txt
```

Nine modes. One field. Show code example for each.

### BACKUP B -- Agent Loop Detail

```
1. Initial prompt + skills injection
2. LLM response (with tool defs)
3. Tool execution via MCP
4. Stop condition check (guardrails, completion, max_turns)
5. Continue or return
```

### BACKUP C -- Testing Strategy

```
8,100+ tests
- Unit tests: cargo test --lib
- Snapshot tests: insta
- Property tests: proptest
- Zero keychain popups (--lib flag)
```

---

## Production Notes

- All code examples must be verified against v0.42.0 before recording
- Terminal font: JetBrains Mono, 16pt
- Color scheme: dark background, high contrast
- Screen resolution: 1920x1080 minimum, 4K preferred
- Backup video for each demo segment in case of live failure
- Practice run timing: aim for 10:00, never exceed 12:00
