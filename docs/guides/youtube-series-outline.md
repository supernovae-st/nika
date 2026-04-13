# YouTube Series: Learn Nika (12 Episodes)

> 12-episode educational YouTube series aligned with the 12 course levels
> Target audience: developers wanting to learn Nika progressively
> Format: screen recording + voice-over + animated diagrams
> Each episode: 15-25 minutes
> Total series watch time: ~4 hours

---

## Series Branding

**Series Title:** "Learn Nika: AI Workflows, Liberated"
**Channel:** SuperNovae Studio
**Playlist Description:** Master the Nika workflow engine in 12 episodes. From your first shell command to full agent orchestration. Each episode maps to one course level with hands-on exercises.

**Thumbnail Template:**
- Dark background (#0a0e1a)
- Episode number large on the left (gradient purple-pink)
- Level name in bold white text
- Small Nika butterfly logo in the corner
- A relevant icon or screenshot on the right third

**Intro Sequence (5 seconds):**
- Nika butterfly animation
- "Learn Nika" text
- Episode number and title
- SuperNovae Studio logo

**Outro Sequence (10 seconds):**
- Next episode preview card
- Subscribe + bell icon reminder
- Playlist link
- GitHub repo link

---

## EPISODE 1: Jailbreak

**Course Level:** 1 -- Jailbreak
**Duration:** 15 minutes
**Exercises:** 5

### Metadata

**Title:** "Learn Nika #1: Jailbreak -- Your First Workflow in 60 Seconds"
**SEO Description:** Learn Nika, a Rust-powered YAML workflow engine for AI tasks. Episode 1: install Nika, write your first exec: workflow, and break free from manual commands. No AI experience required. 5 hands-on exercises included.
**Tags:** nika, yaml workflow, rust, ai workflow engine, tutorial, beginner, exec command, cli tool
**Thumbnail Concept:** Terminal screen with green text "JAILBREAK" breaking through chains. Episode "01" in large purple gradient.

### Content Outline

```
0:00  Cold open: "One command. One file. Your first AI workflow."
0:15  Series intro: what is Nika, what you will learn in 12 episodes
1:00  Installation (cargo install nika, verify)
2:00  The schema line: nika/workflow@0.12
3:00  Your first exec: task (echo "Hello")
4:00  nika check -- validation before execution
5:00  nika run -- execution and output
6:00  Exercise 1: Single command workflow
7:00  Multi-task workflows (3 exec tasks)
8:00  Parallel execution explained (no depends_on = parallel)
9:00  Exercise 2: System info collector
10:00  Timeouts and error handling
11:00  Exercise 3: Command with timeout
12:00  Exercise 4-5: Challenge exercises
13:00  Recap: schema, tasks, exec, check, run
14:00  Next episode preview: "Hot Wire -- mastering HTTP"
14:30  Outro
```

### Key Teaching Points

- Every workflow starts with `schema: nika/workflow@0.12`
- Every task has a unique `id:`
- The `exec:` verb runs shell commands
- `nika check` validates without executing
- Tasks without dependencies run in parallel

---

## EPISODE 2: Hot Wire

**Course Level:** 2 -- Hot Wire
**Duration:** 18 minutes
**Exercises:** 4

### Metadata

**Title:** "Learn Nika #2: Hot Wire -- HTTP Requests and Data Extraction"
**SEO Description:** Master the fetch: verb in Nika. Make HTTP requests, extract data with 9 modes (jsonpath, markdown, article, metadata), and build data collection workflows. 4 exercises.
**Tags:** nika, fetch, http, api, jsonpath, web scraping, data extraction, yaml workflow
**Thumbnail Concept:** Network cables connecting to a globe icon. "HOT WIRE" text with orange/amber gradient. Episode "02".

### Content Outline

```
0:00  Cold open: "The internet has data. Let's go get it."
0:15  Recap: exec verb from Episode 1
0:45  Introducing fetch: -- HTTP requests in YAML
1:30  Basic fetch: URL only (raw response)
3:00  Exercise 1: Fetch a public API
4:00  Extract modes overview (9 modes)
5:00  extract: jsonpath -- query JSON APIs
7:00  Exercise 2: JSONPath extraction
8:00  extract: markdown -- HTML to clean Markdown
9:30  extract: article -- Readability-style extraction
10:30  extract: metadata -- OG tags, Twitter Cards
11:30  extract: feed -- RSS/Atom parsing
12:30  Exercise 3: Multiple extraction modes
13:30  Response modes: full, binary, default
14:30  Headers, method, timeout configuration
15:30  Exercise 4: Complete data pipeline
16:30  Recap: 9 extraction modes, response modes
17:30  Next preview: "Fork Bomb -- DAG patterns"
18:00  Outro
```

### Key Teaching Points

- `fetch:` makes HTTP requests with minimal configuration
- 9 extraction modes turn raw HTML/JSON into useful data
- `extract: jsonpath` queries JSON with `selector:` for the path
- `extract: markdown` converts web pages to clean Markdown
- Response modes control what fetch returns (raw body, full response, binary)

---

## EPISODE 3: Fork Bomb

**Course Level:** 3 -- Fork Bomb
**Duration:** 20 minutes
**Exercises:** 4

### Metadata

**Title:** "Learn Nika #3: Fork Bomb -- DAGs, Dependencies, and Parallel Power"
**SEO Description:** Understand DAG-based execution in Nika. Learn depends_on for task ordering, parallel execution patterns, fan-out/fan-in, and how Nika schedules tasks with Kahn's algorithm. 4 exercises.
**Tags:** nika, dag, depends_on, parallel execution, workflow orchestration, kahn algorithm, topological sort
**Thumbnail Concept:** A forking graph structure with glowing nodes. "FORK BOMB" in green gradient. Episode "03".

### Content Outline

```
0:00  Cold open: DAG visualization animation (nodes lighting up in parallel)
0:30  Why DAGs matter for AI workflows
1:30  depends_on: syntax and semantics
3:00  Drawing the DAG: sequential chain
4:00  Exercise 1: Three-task sequential pipeline
5:30  Parallel execution: tasks without dependencies
7:00  Fan-out pattern: one task feeds many
8:00  Fan-in pattern: many tasks feed one
9:00  Diamond pattern: fork and join
10:00  Exercise 2: Fan-out/fan-in weather dashboard
12:00  How Nika schedules: Kahn's topological sort
13:30  Maximum parallelism: the engine figures it out
14:30  nika check --dag: visualize the dependency graph
15:30  Exercise 3: Complex DAG with multiple layers
17:00  Common mistakes: circular dependencies (NIKA-020)
18:00  Exercise 4: Debug a broken DAG
19:00  Recap: depends_on, parallel, fan-out, fan-in, diamond
19:30  Next preview: "Root Access -- unlocking the LLM"
20:00  Outro
```

### Key Teaching Points

- `depends_on: [task_id]` creates DAG edges
- Tasks without dependencies run in parallel automatically
- Four patterns: sequential, fan-out, fan-in, diamond
- Kahn's algorithm determines execution order
- Cycles are caught at validation time (NIKA-020)

---

## EPISODE 4: Root Access

**Course Level:** 4 -- Root Access
**Duration:** 20 minutes
**Exercises:** 3

### Metadata

**Title:** "Learn Nika #4: Root Access -- Your First LLM Call"
**SEO Description:** Unlock LLM generation with the infer: verb. Connect to Claude, OpenAI, Groq, or local models. Configure prompts, system messages, temperature, and max_tokens. 3 exercises.
**Tags:** nika, infer, llm, claude, openai, groq, ai generation, prompting, yaml ai
**Thumbnail Concept:** A lock being opened with a key made of code. "ROOT ACCESS" in electric blue. Episode "04".

### Content Outline

```
0:00  Cold open: "Time to talk to the machines."
0:30  Setting up an API key (ANTHROPIC_API_KEY)
1:30  nika provider list -- checking available providers
2:30  The infer: verb basics
3:30  model: provider/model-name syntax
4:30  Your first LLM call (haiku for speed)
5:30  Exercise 1: Hello from Claude
6:30  System prompts with system:
7:30  Temperature and max_tokens
8:30  Multi-line prompts with YAML |
9:30  Exercise 2: Expert system with system prompt
11:00  Switching providers (Claude -> OpenAI -> Groq)
12:30  Native provider: local GGUF models (offline)
14:00  Combining infer: with fetch: (data -> LLM)
15:30  Exercise 3: Fetch + analyze pipeline
17:00  Cost awareness: which models for which tasks
18:30  Recap: infer, model, system, temperature, providers
19:30  Next preview: "Shapeshifter -- data flow mastery"
20:00  Outro
```

### Key Teaching Points

- `infer:` calls LLMs with `model: provider/model-name`
- `system:` sets the LLM's role/persona
- `temperature:` controls creativity (0 = deterministic, 1 = creative)
- Eight providers: Claude, OpenAI, Mistral, Groq, DeepSeek, Gemini, xAI, Native
- Cost varies by model -- use Haiku/mini for simple tasks, Sonnet/GPT-4o for complex ones

---

## EPISODE 5: Shapeshifter

**Course Level:** 5 -- Shapeshifter
**Duration:** 20 minutes
**Exercises:** 3

### Metadata

**Title:** "Learn Nika #5: Shapeshifter -- Data Bindings and Pipe Transforms"
**SEO Description:** Master Nika's data flow system. Learn with: bindings, dollar-sign task references, double-brace templates, and 27 pipe transforms for inline data processing. 3 exercises.
**Tags:** nika, data flow, bindings, templates, transforms, pipe operator, yaml workflow
**Thumbnail Concept:** Data transforming between shapes (cube to sphere to star). "SHAPESHIFTER" in gradient purple. Episode "05".

### Content Outline

```
0:00  Cold open: "Data without flow is just noise."
0:30  Recap: depends_on creates order; now we need data
1:30  The with: block explained
2:30  Dollar-sign prefix: $task_id references
3:30  Template syntax: {{with.alias}}
4:30  Three-part validation: exists, in depends_on, valid alias
5:30  Exercise 1: Two-task data pipeline
7:00  Multiple bindings: with: { a: $t1, b: $t2 }
8:00  Accessing nested data: {{with.data.field}}
9:00  Pipe transforms introduction: {{with.data | uppercase}}
10:00  Common transforms: uppercase, lowercase, trim, reverse
11:00  Chaining: {{with.data | trim | uppercase | truncate(50)}}
12:00  The full catalog: 27 transforms
13:00  Exercise 2: Transform pipeline
14:30  Real-world pattern: fetch -> transform -> infer
16:00  Exercise 3: Multi-source synthesis
18:00  Error messages: NIKA-075 (bad reference), NIKA-076 (missing dep)
19:00  Recap: with:, $, {{}}, pipes, 27 transforms
19:30  Next preview: "Pay-Per-Dream -- structured output"
20:00  Outro
```

### Key Teaching Points

- `with: { alias: $task_id }` creates data bindings
- Dollar-sign prefix references task results
- Double-brace templates resolve at runtime
- 27 pipe transforms for inline processing
- Validation catches misspelled references with suggestions

---

## EPISODE 6: Pay-Per-Dream

**Course Level:** 6 -- Pay-Per-Dream
**Duration:** 22 minutes
**Exercises:** 3

### Metadata

**Title:** "Learn Nika #6: Pay-Per-Dream -- Structured Output and JSON Schemas"
**SEO Description:** Guarantee LLM output format with Nika's 5-layer structured output defense. Define JSON schemas, enforce types, and build reliable AI data pipelines. 3 exercises.
**Tags:** nika, structured output, json schema, llm reliability, output validation, ai pipeline
**Thumbnail Concept:** A golden dream cloud with JSON brackets floating inside. "PAY-PER-DREAM" in gold. Episode "06".

### Content Outline

```
0:00  Cold open: "LLMs are creative. Sometimes too creative."
0:30  The problem: unstructured LLM output
1:30  output: block syntax
2:30  format: json
3:00  Defining a schema: type, properties, required
4:30  Exercise 1: Basic structured output
6:00  The 5-layer defense explained
7:30  [ANIMATION] Layer escalation visualization
9:00  Complex schemas: nested objects, arrays, enums
10:30  Exercise 2: Complex schema with nested types
12:30  Schema validation errors (what happens when it fails)
13:30  Using structured output with data flow
14:30  Pattern: fetch -> infer (structured) -> save
16:00  Exercise 3: End-to-end structured pipeline
18:00  Best practices: keep schemas simple, use descriptions
19:00  Cost impact: structured output and token usage
20:00  Recap: output, format, schema, 5 layers
21:00  Next preview: "Swiss Knife -- builtin tools"
22:00  Outro
```

### Key Teaching Points

- `output: { format: json, schema: {...} }` enforces structure
- JSON Schema syntax: type, properties, required, enum, minimum/maximum
- Five-layer defense: provider-native -> extract -> validate -> retry -> repair
- Descriptions in schema fields improve LLM compliance
- Structured output enables reliable downstream data processing

---

## EPISODE 7: Swiss Knife

**Course Level:** 7 -- Swiss Knife
**Duration:** 18 minutes
**Exercises:** 3

### Metadata

**Title:** "Learn Nika #7: Swiss Knife -- 24+ Builtin Tools"
**SEO Description:** Explore Nika's builtin tool ecosystem. Use invoke: with nika:log, nika:emit, nika:assert, and discover 24+ tools for logging, validation, media, and more. 3 exercises.
**Tags:** nika, invoke, builtin tools, nika:log, nika:assert, mcp, tool calling
**Thumbnail Concept:** A multi-tool (Swiss Army knife style) with code symbols on each tool. "SWISS KNIFE" in red. Episode "07".

### Content Outline

```
0:00  Cold open: "Five verbs compose. But tools multiply."
0:30  The invoke: verb for nika: builtin tools
1:30  nika:log -- structured logging within workflows
2:30  nika:emit -- emit custom events
3:30  nika:assert -- runtime assertions
4:30  Exercise 1: Logging and assertions
6:00  Overview of 24+ builtin tools
7:00  File tools: nika:read, nika:write, nika:edit
8:30  Exercise 2: File operations workflow
10:00  Utility tools overview
11:00  Combining tools with other verbs
12:00  Exercise 3: Multi-tool pipeline
14:00  Tool error handling (NIKA-200 to NIKA-214)
15:00  Preview: media tools (covered in Episode 11)
16:00  Recap: invoke, nika: prefix, 24+ tools
17:00  Next preview: "Gone Rogue -- autonomous agents"
18:00  Outro
```

### Key Teaching Points

- `invoke: { tool: nika:*, input: {...} }` calls builtin tools
- nika:log for structured workflow logging
- nika:emit for custom event emission
- nika:assert for runtime validation
- File tools for reading, writing, editing files
- Error codes NIKA-200 to NIKA-214 for tool-specific issues

---

## EPISODE 8: Gone Rogue

**Course Level:** 8 -- Gone Rogue
**Duration:** 25 minutes
**Exercises:** 3

### Metadata

**Title:** "Learn Nika #8: Gone Rogue -- Autonomous AI Agents"
**SEO Description:** Build autonomous AI agents with the agent: verb. Configure goals, tools, guardrails, completion modes, and max_turns. Learn agent loop architecture and stop conditions. 3 exercises.
**Tags:** nika, agent, autonomous ai, agentic loop, guardrails, tool calling, multi-turn
**Thumbnail Concept:** A robot breaking free from a terminal window. "GONE ROGUE" in gold with red accents. Episode "08".

### Content Outline

```
0:00  Cold open: "What if the AI decided what to do next?"
0:30  The agent: verb -- multi-turn autonomous loops
2:00  Agent anatomy: goal, skills, max_turns, completion
3:30  Your first agent (simple goal, nika:log tool)
5:00  Exercise 1: Basic research agent
7:00  [ANIMATION] Agent loop flow diagram
8:00  Completion modes: natural, explicit, pattern
9:30  Guardrails: length, schema, regex, LLM validation
11:00  Exercise 2: Agent with guardrails
13:00  stop_sequences and token_budget
14:00  Multi-tool agents (giving agents multiple skills)
15:30  Exercise 3: Multi-tool research and report agent
18:00  Agent + infer pipeline (agent researches, infer synthesizes)
19:30  Safety: max_turns as a hard limit
20:00  Cost management with agents (budget tracking)
21:00  Human-in-the-loop (HITL) concepts
22:00  Debugging agents: event log, TUI monitor
23:00  Recap: agent, goal, skills, completion, guardrails
24:00  Next preview: "Data Heist -- advanced extraction"
25:00  Outro
```

### Key Teaching Points

- `agent:` creates autonomous multi-turn loops
- The LLM decides which tools to call and when to stop
- `completion.mode: natural` lets the agent self-terminate
- Guardrails enforce quality constraints on each turn
- `max_turns` is a safety net, not a performance target

---

## EPISODE 9: Data Heist

**Course Level:** 9 -- Data Heist
**Duration:** 22 minutes
**Exercises:** 4

### Metadata

**Title:** "Learn Nika #9: Data Heist -- Advanced Fetch Extraction"
**SEO Description:** Deep dive into Nika's 9 fetch extraction modes. Build web scrapers, RSS readers, and metadata extractors. Master article, links, metadata, feed, and llm_txt modes. 4 exercises.
**Tags:** nika, fetch, web scraping, extraction, rss, metadata, css selector, readability
**Thumbnail Concept:** A vault door opening to reveal data streams. "DATA HEIST" in amber/orange. Episode "09".

### Content Outline

```
0:00  Cold open: "The web is a goldmine. If you have the right tools."
0:30  Recap: basic fetch from Episode 2
1:00  Deep dive: all 9 extraction modes
2:00  extract: article -- Readability algorithm (dom_smoothie)
3:30  extract: text -- visible text with CSS selectors
5:00  extract: selector -- raw HTML from CSS matches
6:00  extract: metadata -- OG, Twitter, JSON-LD, SEO
7:30  Exercise 1: Metadata extraction pipeline
9:00  extract: links -- classified link inventory
10:00  extract: feed -- RSS/Atom/JSON Feed parsing
11:30  Exercise 2: RSS aggregator
13:00  extract: llm_txt -- AI-era content discovery
14:00  Response modes: full (headers, status), binary (CAS)
15:00  Exercise 3: Full response analysis
16:30  Chaining extraction with LLM analysis
17:30  Exercise 4: News intelligence pipeline
19:30  Performance: concurrent fetches with DAG
20:30  Recap: 9 modes, response modes, real patterns
21:30  Next preview: "Open Protocol -- MCP integration"
22:00  Outro
```

### Key Teaching Points

- Each extraction mode has a specific use case and backing library
- `extract: article` is ideal for blog posts and news sites
- `extract: metadata` returns structured SEO data
- `extract: feed` handles RSS, Atom, and JSON Feed formats
- `response: full` includes status codes and headers for debugging

---

## EPISODE 10: Open Protocol

**Course Level:** 10 -- Open Protocol
**Duration:** 20 minutes
**Exercises:** 3

### Metadata

**Title:** "Learn Nika #10: Open Protocol -- MCP Integration"
**SEO Description:** Connect Nika to external systems with MCP (Model Context Protocol). Use invoke: for external tool servers, understand NovaNet integration, and build cross-system workflows. 3 exercises.
**Tags:** nika, mcp, model context protocol, invoke, novanet, external tools, integration
**Thumbnail Concept:** Two terminals connected by a glowing bridge. "OPEN PROTOCOL" in purple. Episode "10".

### Content Outline

```
0:00  Cold open: "One protocol to connect them all."
0:30  What is MCP? (Model Context Protocol by Anthropic)
2:00  invoke: with external MCP servers
3:30  MCP server configuration in workflow
5:00  Schema validation for tool inputs
6:00  Exercise 1: Call an MCP tool
8:00  NovaNet integration (Brain + Body architecture)
9:30  The Zero Cypher rule explained
10:30  invoke: with NovaNet tools
11:30  Exercise 2: NovaNet knowledge query
13:00  Connection pooling and retry
14:00  Multiple MCP servers in one workflow
15:00  Exercise 3: Multi-server orchestration
17:00  Error handling: NIKA-100 to NIKA-109
18:00  MCP aliases for common servers
19:00  Recap: invoke, MCP, NovaNet, Zero Cypher
19:30  Next preview: "Pixel Pirate -- media pipeline"
20:00  Outro
```

### Key Teaching Points

- `invoke:` calls any MCP-compatible tool server
- MCP is the standard protocol for AI tool communication
- NovaNet is Nika's knowledge graph partner (Brain + Body)
- Zero Cypher: workflows never contain database queries
- Connection pooling and retry are automatic

---

## EPISODE 11: Pixel Pirate

**Course Level:** 11 -- Pixel Pirate
**Duration:** 25 minutes
**Exercises:** 4

### Metadata

**Title:** "Learn Nika #11: Pixel Pirate -- The Media Pipeline"
**SEO Description:** Master Nika's 24 media tools. Import images into CAS, generate thumbnails, extract metadata, create charts, sign with C2PA provenance, and chain operations with nika:pipeline. 4 exercises.
**Tags:** nika, media pipeline, image processing, cas, content addressable, thumbnail, c2pa, provenance
**Thumbnail Concept:** A pirate flag with a pixel art butterfly. "PIXEL PIRATE" in teal/cyan. Episode "11".

### Content Outline

```
0:00  Cold open: "24 media tools. Zero ImageMagick."
0:30  Media architecture: content-addressable storage (CAS)
2:00  blake3 hashing, zstd compression, atomic writes
3:00  nika:import -- bringing files into CAS with validation
4:30  Exercise 1: Import and inspect
6:00  Tier 1 tools: import, dimensions, thumbhash, dominant_color, pipeline
7:30  Tier 2: thumbnail (SIMD Lanczos3), convert, strip, metadata, optimize, svg_render
9:00  Exercise 2: Image processing pipeline
11:00  nika:pipeline -- chaining operations in-memory (zero temp files)
12:30  Tier 3: phash, compare, chart, provenance, verify, qr_validate, quality, pdf_extract, html_to_md, css_select, extract_metadata, extract_links, readability
14:00  Exercise 3: Advanced media workflow
16:00  Vision integration: CAS hash in infer: content blocks (auto base64)
17:30  C2PA provenance signing (EU AI Act compliance)
19:00  Exercise 4: Full pipeline with AI analysis
21:00  Security: decode_image_safe (no pixel floods), SVG sanitization (no XXE)
22:00  Performance: parallel media tasks in DAG, MediaBudget (500MB per run)
23:00  Recap: CAS, 3 tiers, 24 tools, vision, provenance
24:00  Next preview: "SuperNovae -- the final boss"
25:00  Outro
```

### Key Teaching Points

- Content-addressable storage: import once, reference by hash (immutable + deduplicated)
- Three tiers of media tools (always-on, default, opt-in)
- `nika:pipeline` chains operations without intermediate files
- Vision: CAS hashes auto-resolve to base64 in `infer:` content blocks
- C2PA provenance for content authenticity and EU AI Act compliance
- Security: decode_image_safe limits prevent pixel floods, SVG sanitization prevents XXE

---

## EPISODE 12: SuperNovae

**Course Level:** 12 -- SuperNovae (Boss Level)
**Duration:** 25 minutes
**Exercises:** 5

### Metadata

**Title:** "Learn Nika #12: SuperNovae -- The Final Boss"
**SEO Description:** The capstone episode. Combine all 5 verbs, media tools, agents, structured output, and MCP integration into production-quality AI workflows. 5 exercises. Boss level.
**Tags:** nika, capstone, production workflow, orchestration, ai pipeline, boss level, supernovae
**Thumbnail Concept:** An explosion of light (supernova) with the butterfly emerging from it. "SUPERNOVAE" in white with golden glow. Episode "12" with a crown icon.

### Content Outline

```
0:00  Cold open: "Everything you learned. One workflow."
0:30  Series recap: 11 levels, 39 exercises, 5 verbs
2:00  Production workflow patterns
3:00  Exercise 1: Multi-source data aggregation
5:30  Error resilience: retry, timeout, fail_fast
7:00  Exercise 2: Resilient pipeline with error handling
9:00  Agent + media + structured output combination
10:30  Exercise 3: AI content factory
13:00  The full architecture review (TUI demo)
14:30  Exercise 4: Dashboard workflow with TUI monitoring
17:00  Exercise 5: The Boss -- complete production pipeline
20:00  Solutions walkthrough
22:00  What's next: roadmap (model routing, orchestration, Punk Records)
23:00  The course: nika init --course (the full 44 exercises)
24:00  Community: GitHub, contributing, AGPL philosophy
24:30  Closing: "Limited only by the YAML you write."
25:00  Outro (special extended version with highlights reel)
```

### Key Teaching Points

- All five verbs compose into complex workflows
- Production patterns: error handling, retries, timeouts
- The TUI is your debugging and monitoring companion
- The course has 44 total exercises for continued practice
- Nika is open source (AGPL-3.0) and welcomes contributors

---

## Series Production Schedule

| Week | Episodes | Status |
|------|----------|--------|
| 1 | Ep 1-2 (Jailbreak, Hot Wire) | Scripting |
| 2 | Ep 3-4 (Fork Bomb, Root Access) | Scripting |
| 3 | Ep 1-2 Recording | Recording |
| 4 | Ep 5-6 (Shapeshifter, Pay-Per-Dream) | Scripting |
| 5 | Ep 3-4 Recording, Ep 1-2 Editing | Recording + Post |
| 6 | Ep 7-8 (Swiss Knife, Gone Rogue) | Scripting |
| 7 | Ep 5-6 Recording, Ep 3-4 Editing | Recording + Post |
| 8 | Ep 9-10 (Data Heist, Open Protocol) | Scripting |
| 9 | Ep 7-8 Recording, Ep 5-6 Editing | Recording + Post |
| 10 | Ep 11-12 (Pixel Pirate, SuperNovae) | Scripting |
| 11 | Ep 9-10 Recording, Ep 7-8 Editing | Recording + Post |
| 12 | Ep 11-12 Recording, Ep 9-10 Editing | Recording + Post |
| 13 | Final edits, Ep 11-12 Editing | Post |
| 14 | Series launch (all 12 published) | Launch |

**Release Cadence:** 2 episodes per week for 6 weeks (after initial batch production).

---

## YouTube SEO Strategy

### Channel Keywords

```
nika workflow engine, yaml ai, rust ai tool, ai automation,
workflow automation, llm orchestration, mcp protocol,
claude api, openai api, structured output, agent loop
```

### Per-Episode SEO Checklist

- [ ] Title under 60 characters with primary keyword
- [ ] Description: 2-3 paragraphs with timestamps and links
- [ ] Tags: 8-12 relevant tags
- [ ] Custom thumbnail following template
- [ ] End screen: next episode + subscribe
- [ ] Cards: link to course exercise at the relevant timestamp
- [ ] Chapters: auto-generated from timestamps in description
- [ ] Pinned comment: exercise starter files + solution link

### Description Template

```
Learn Nika Episode [N]: [Level Name]
[One-sentence description]

In this episode you will learn:
- [Point 1]
- [Point 2]
- [Point 3]

TIMESTAMPS:
[Generated from content outline]

RESOURCES:
Source code: github.com/supernovae-st/nika
Course: nika init --course
Showcase: nika showcase list
Episode exercises: [link]

INSTALL:
cargo install nika
nika --version

#nika #yamlworkflow #rustlang #ai #llm
```

---

## Cross-Promotion Opportunities

| Platform | Content | Timing |
|----------|---------|--------|
| GitHub README | Link to playlist | On series launch |
| Twitter/X | Episode clip (60s) | Each release |
| Reddit r/rust | Announcement post | Ep 1 + Ep 12 |
| Hacker News | Show HN | Ep 1 launch |
| Dev.to | Written companion post | Ep 1, 6, 12 |
| Discord | Each episode discussion | Each release |
| LinkedIn | Professional article | Ep 1 + series wrap |
