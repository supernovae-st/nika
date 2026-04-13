# Conference Talk: Beyond LangChain -- Why We Built a 1.56M-Line Rust Workflow Engine

> 30-minute conference talk with slide deck
> Target audience: RustConf / AI engineering conferences / open-source summits
> Format: 50+ slides with speaker notes, 3 embedded demo segments
> Speaker: Thibaut Melen, SuperNovae Studio

---

## Talk Structure

```
Part 1: The Problem          (0:00 - 5:00)   -- Why existing tools fail
Part 2: The Design           (5:00 - 12:00)  -- Five verbs, three phases, one DAG
Part 3: Demo 1               (12:00 - 16:00) -- Live: write and run a workflow
Part 4: The Engine           (16:00 - 22:00) -- Deep technical content
Part 5: Demo 2               (22:00 - 25:00) -- Live: media pipeline + TUI
Part 6: The Ecosystem        (25:00 - 28:00) -- Course, NovaNet, roadmap
Part 7: Close                (28:00 - 30:00) -- Philosophy, CTA
```

---

## PART 1: THE PROBLEM (0:00 - 5:00)

### SLIDE 1 -- Title

[SLIDE] Conference branding. Title: "Beyond LangChain: Why We Built a 1.56M-Line Rust Workflow Engine." Speaker: Thibaut Melen, SuperNovae Studio. The Nika butterfly logo in the corner.

**Speaker Notes:**
Thank you. I am Thibaut, and I build workflow engines in Rust at SuperNovae Studio. Today I want to talk about why we built Nika instead of using what already existed, what we learned from 1.56 million lines of Rust, and why YAML plus a compiler pipeline might be the right abstraction for AI workflows.

---

### SLIDE 2 -- The State of AI Tooling

[SLIDE] Screenshot collage: LangChain docs, CrewAI configuration, LangGraph examples. All visually dense.

**Speaker Notes:**
Let me paint the landscape. If you want to build an AI workflow today, your options are Python frameworks. LangChain for chains and agents. LangGraph for stateful multi-step workflows. CrewAI for multi-agent orchestration. They are all Python. They all require learning a framework-specific API. They all discover errors at runtime.

---

### SLIDE 3 -- The Five Problems

[SLIDE] Five bullet points, appearing one at a time:

```
1. Runtime errors for configuration mistakes
2. No type safety at the workflow level
3. Python's concurrency model limits parallel execution
4. Framework lock-in (every tool has its own DSL)
5. Memory overhead makes scaling expensive
```

**Speaker Notes:**
Five problems that are not bugs -- they are architectural limitations. First: configuration errors surface at runtime, possibly after burning API tokens. Second: there is no type system checking that your data flows are valid before execution. Third: Python's GIL and asyncio model create artificial bottlenecks for I/O-heavy AI workloads. Fourth: every framework has its own vocabulary, its own abstractions, its own way of doing things. Fifth: the memory overhead of a Python runtime plus framework plus dependencies makes scaling expensive.

---

### SLIDE 4 -- What We Wanted

[SLIDE] Five matching requirements:

```
1. Validate before execute
2. Type-checked data flow
3. Native parallel execution
4. Declarative, serializable workflows
5. Minimal resource footprint
```

**Speaker Notes:**
We wanted the opposite. Validate before execution -- like a compiler. Type-checked data flow -- like a statically typed language. Native parallel execution -- like Tokio, not asyncio. Declarative workflows that you can serialize, version-control, and share. And a minimal footprint -- single binary, megabytes not gigabytes.

---

### SLIDE 5 -- The Question

[SLIDE] Large text, centered: "What if your workflow engine was a compiler?"

**Speaker Notes:**
The core insight: what if we treated workflow definitions like source code? What if we parsed, analyzed, validated, and lowered them before execution -- the way rustc handles Rust? That question led to Nika.

---

## PART 2: THE DESIGN (5:00 - 12:00)

### SLIDE 6 -- Introducing Nika

[SLIDE] Nika butterfly logo, full screen. "Semantic YAML Workflow Engine for AI Tasks."

**Speaker Notes:**
Nika. Named after the Sun God Nika from One Piece -- a figure whose power is limited only by imagination. A semantic YAML workflow engine for AI tasks, written in Rust. Twelve Cargo workspace crates. Schema nika/workflow@0.12. AGPL-3.0-or-later.

---

### SLIDE 7 -- Five Verbs

[SLIDE] The five verbs as a table:

```
| Verb      | Purpose          | Key Features                              |
|-----------|------------------|-------------------------------------------|
| infer:    | LLM generation   | Vision, thinking, structured output       |
| exec:     | Shell command    | Blocklist, NFKC, timeout                  |
| fetch:    | HTTP request     | 9 extraction modes                        |
| invoke:   | MCP tool call    | Schema validation, retry, pooling         |
| agent:    | Multi-turn loop  | Guardrails, completion, HITL              |
```

**Speaker Notes:**
The entire Nika vocabulary is five verbs. Not five hundred API methods. Five verbs that compose. Let me walk through each one.

`infer:` calls an LLM. Any of nine providers. Vision, extended thinking, structured output with a five-layer defense, guardrails -- all configured in YAML.

`exec:` runs a shell command. With a security blocklist, NFKC Unicode normalization to prevent homoglyph attacks, and configurable timeouts.

`fetch:` makes HTTP requests. But it is not just curl-in-YAML. It has nine extraction modes -- markdown, article, jsonpath, feed, and more. Post-processing built into the verb.

`invoke:` calls MCP tools. Any MCP-compatible server. Schema validation before the call. Connection pooling. Retry with backoff.

`agent:` creates multi-turn agentic loops. The LLM decides which tools to call and when to stop. Guardrails enforce quality. Completion modes define success criteria.

---

### SLIDE 8 -- A Complete Workflow

[SLIDE] YAML code:

```yaml
schema: nika/workflow@0.12

tasks:
  - id: research
    fetch:
      url: https://news.ycombinator.com
      extract: article

  - id: summarize
    depends_on: [research]
    with: { article: $research }
    infer:
      model: claude/claude-haiku-3-5-20241022
      prompt: "Summarize: {{with.article}}"
      output:
        format: json
        schema:
          type: object
          properties:
            summary: { type: string }
            topics: { type: array, items: { type: string } }
          required: [summary, topics]
```

**Speaker Notes:**
A complete workflow. Fetch a web page, extract the article content, summarize it with Claude, and enforce a JSON output schema. Eleven lines of meaningful YAML. No imports, no boilerplate, no framework initialization. The schema line gives you LSP completions. The `depends_on` creates the DAG. The `with:` binding creates the data flow. The `output:` block guarantees structure.

---

### SLIDE 9 -- The Three-Phase AST

[SLIDE] Three boxes with arrows:

```
Phase 1: Raw                Phase 2: Analyzed           Phase 3: Lowered
-------------------------   -------------------------   -------------------------
YAML -> spans               Validation passes           Runtime types
All fields Optional         Task ID interning           Optimized for execution
No validation               Binding resolution          No more validation
Source location tracking     Dependency checking         Types encode correctness
```

**Speaker Notes:**
Here is where it gets interesting for Rust developers. The AST has three phases, inspired by rustc.

Phase 1: raw parsing. marked-yaml plus serde-saphyr with bomb protection. Every field is Option. We record source spans for error reporting. Zero validation.

Phase 2: analysis. Nine categories of passes. Schema version, task IDs get interned to Arc<str>, verb exclusivity, dependency resolution, binding validation, model references, duplicates, output schemas, security policies. After analysis, Options are gone. Every required field exists. The type system changes -- AnalyzedTask is a different type than RawTask. You cannot pass a raw task where an analyzed one is expected.

Phase 3: lowering. Analyzed types become runtime types. InferParams, ExecParams, FetchParams. Defaults applied, fields flattened, ready for execution. After lowering, no validation ever runs again. The runtime trusts the types.

---

### SLIDE 10 -- Why Three Phases Matter

[SLIDE] Error comparison:

```
LangChain error:
  TypeError: 'NoneType' object has no attribute 'content'
  (at runtime, after 3 API calls, $0.12 spent)

Nika error:
  NIKA-075: Binding '$summarize' references task 'sumamrize'
            which does not exist. Did you mean 'summarize'?
            --> workflow.nika.yaml:12:18
  (at validation time, $0.00 spent)
```

**Speaker Notes:**
This is why three phases matter. LangChain gives you a Python TypeError at runtime, after you have already spent money on API calls. Nika gives you NIKA-075 with the exact line number, a suggestion for what you probably meant, and zero dollars spent. Validation-time errors with source spans. That is the compiler advantage.

---

### SLIDE 11 -- The DAG

[SLIDE] DAG visualization with execution layers:

```
[fetch_a]  [fetch_b]  [fetch_c]    Layer 0: max parallel
    \        |         /
     \       |        /
      [  merge_all  ]              Layer 1: fan-in
           |
      [  analyze   ]              Layer 2: sequential
       /         \
  [report_md]  [report_json]      Layer 3: fan-out
```

**Speaker Notes:**
Dependencies create a DAG. The DAG determines execution order and parallelism. Layer 0: three fetches run in parallel, no dependencies between them. Layer 1: merge waits for all three. Layer 2: analysis runs sequentially after merge. Layer 3: two report formats fan out in parallel.

The DAG is immutable after construction. Once built, it cannot be modified. This makes concurrent access safe without locks. The runtime uses Tokio's JoinSet -- tasks spawn as their dependencies complete. No thread pool configuration, no concurrency limits. The DAG IS the concurrency control.

---

### SLIDE 12 -- Data Flow

[SLIDE] Detailed binding flow:

```yaml
with: { article: $research }
#       ^alias    ^task_id ($ prefix = task reference)

prompt: "{{with.article}}"
#        ^template resolution at execution time
```

```
At analysis time:
  1. Check '$research' exists as a task ID ---- NIKA-075 if not
  2. Check 'research' is in depends_on --------- NIKA-076 if not
  3. Check 'article' is not a reserved word ---- NIKA-078 if so

At execution time:
  4. Look up TaskResult in RunContext (DashMap)
  5. Resolve {{with.article}} to output string
  6. Apply pipe transforms if present
```

**Speaker Notes:**
Data flow is explicit and validated. The dollar-sign prefix in `with:` means "reference a task result." At analysis time, three checks: does the task exist? Is it in depends_on? Is the alias valid? At execution time: look up the result in the RunContext -- a DashMap, lock-free concurrent hashmap -- and resolve templates. Pipe transforms like `| uppercase | trim` are available in all template contexts. Twenty-seven transforms built in.

---

### SLIDE 13 -- Structured Output: Five-Layer Defense

[SLIDE] Five layers as a stack:

```
Layer 4: LLM Repair     -- Ask the model to fix its own output
Layer 3: Retry           -- Retry with enhanced prompt
Layer 2: Extract+Valid   -- Extract JSON + validate against schema
Layer 1: Extractor       -- Extract JSON from mixed text
Layer 0: Provider-native -- Use provider's structured output API
```

**Speaker Notes:**
Structured output is hard. LLMs are probabilistic -- they do not always produce valid JSON. Nika has five escalation layers. Layer 0 tries the provider's native structured output, like Claude's tool_use mode. If that fails or is not available, layer 1 extracts JSON from the response text. Layer 2 validates against your schema. Layer 3 retries with an enhanced prompt that includes the validation errors. Layer 4 asks the LLM to repair its own output based on the schema violations. Automatic escalation. Zero configuration from the workflow author.

---

### SLIDE 14 -- Security Model

[SLIDE] Four pillars:

```
1. NFKC normalization     (prevents Unicode homoglyph attacks on exec:)
2. Command blocklist       (rejects known destructive patterns)
3. Path traversal defense  (validates all file paths before access)
4. PolicyEnforcer          (boot-time workspace security validation)
```

**Speaker Notes:**
Security deserves its own slide because most AI frameworks treat it as an afterthought. NFKC normalization catches Unicode tricks where a Cyrillic character looks like a Latin one. The command blocklist rejects known destructive patterns. Path traversal defense prevents `../../etc/passwd` in file imports. The PolicyEnforcer runs at boot, before any task executes, validating environment variables and workspace policies. SVG files are sanitized before parsing. Media imports enforce size limits.

---

### SLIDE 15 -- Nine Providers

[SLIDE] Provider grid:

```
Cloud (via rig-core 0.32):          Local:
  Claude (Anthropic)                  Native (mistral.rs 0.7)
  OpenAI                              - GGUF models (text)
  Mistral                             - VisionHf (HuggingFace + ISQ)
  Groq
  DeepSeek
  Gemini
  xAI
  (8 cloud providers)
```

**Speaker Notes:**
Nine providers total. Eight cloud through rig-core, which gives us a unified completion API. One native through mistral.rs for local GGUF models -- completely offline. Vision support across all providers that support it. The native provider even supports vision through HuggingFace models with in-situ quantization. Switch providers by changing one YAML line. No code changes.

---

## PART 3: DEMO 1 (12:00 - 16:00)

### SLIDE 16 -- Demo Title

[SLIDE] "DEMO: From Zero to AI Pipeline" with terminal cursor blinking.

**Speaker Notes:**
Let me show you this in action.

---

### Demo Script (12:00 - 16:00)

[SCREEN] Terminal, clean prompt.

**Step 1 (12:00 - 12:30): Scaffold**

[TYPE] `mkdir conference-demo && cd conference-demo`
[TYPE] Create `pipeline.nika.yaml` in editor

**Voice-over:**
Empty directory. New file. No project setup, no virtual environment, no package.json.

**Step 2 (12:30 - 13:30): Write the workflow**

[TYPE] Write a three-task workflow:
```yaml
schema: nika/workflow@0.12

tasks:
  - id: fetch_repo
    fetch:
      url: https://api.github.com/repos/supernovae-st/nika
      extract: jsonpath
      selector: "$.description"

  - id: fetch_stats
    fetch:
      url: https://api.github.com/repos/supernovae-st/nika
      extract: jsonpath
      selector: "$.stargazers_count"

  - id: generate_badge
    depends_on: [fetch_repo, fetch_stats]
    with: { desc: $fetch_repo, stars: $fetch_stats }
    infer:
      model: claude/claude-haiku-3-5-20241022
      prompt: |
        Repository: {{with.desc}}
        Stars: {{with.stars}}

        Write a one-paragraph project pitch for a conference talk.
      output:
        format: json
        schema:
          type: object
          properties:
            pitch: { type: string }
            tagline: { type: string, maxLength: 50 }
          required: [pitch, tagline]
```

**Voice-over:**
Three tasks. Two fetch calls to the GitHub API -- running in parallel, no dependency between them. One infer call that depends on both, with structured output. Watch the LSP completions as I type.

**Step 3 (13:30 - 14:00): Validate**

[TYPE] `nika check pipeline.nika.yaml`

[SCREEN] Show validation output -- all green.

**Voice-over:**
`nika check` validates everything before execution. Schema, DAG, bindings, model references. All pass.

**Step 4 (14:00 - 14:30): Run**

[TYPE] `nika run pipeline.nika.yaml`

[SCREEN] Show execution output -- two fetches in parallel, then the infer task.

**Voice-over:**
Two fetches started simultaneously -- no dependency between them. Both completed in under a second. Then the infer task ran with both results as bindings. Structured JSON output, validated against the schema. Total time: about two seconds.

**Step 5 (14:30 - 15:00): Introduce an error**

[SCREEN] Edit the workflow -- misspell a task reference in `depends_on`.

[TYPE] `nika check pipeline.nika.yaml`

[SCREEN] Show error output with source span:
```
NIKA-075: Dependency 'fetch_reop' not found.
          Did you mean 'fetch_repo'?
          --> pipeline.nika.yaml:18:21
```

**Voice-over:**
Now watch what happens when I introduce a typo. NIKA-075 with the exact line and column. A suggestion for what I probably meant. Zero API calls, zero dollars spent. This is the compiler advantage.

**Step 6 (15:00 - 16:00): Show the TUI**

[TYPE] `nika ui`

[SCREEN] Show the TUI, navigate to the workflow, run it from the Monitor view.

**Voice-over:**
And here is the TUI. Home view for browsing, Studio for editing with syntax highlighting, Monitor for watching execution in real-time. DAG nodes lighting up green as tasks complete. Token counts, timing, cost tracking -- all live.

---

## PART 4: THE ENGINE (16:00 - 22:00)

### SLIDE 17 -- Workspace Architecture

[SLIDE] Crate map with line counts:

```
nika-core (23k)    -- Zero I/O, pure types + AST
nika-engine (134k) -- Embeddable runtime
nika-tui (92k)     -- Terminal UI (ratatui)
nika-mcp (9k)      -- MCP client (rmcp 0.16)
nika-media (3.5k)  -- CAS store
nika-event (4k)    -- Event log + NDJSON traces
nika-cli (8k)      -- CLI subcommands
nika-lsp-core (9k) -- LSP intelligence
nika-lsp (2.5k)    -- LSP server
nika (2k)          -- Binary entry point
```

**Speaker Notes:**
Ten crates. Each boundary is an architectural invariant. nika-core has zero I/O -- it compiles without touching a file system. You can use it for analysis without the runtime. nika-engine is embeddable -- you can integrate it into any Rust application. nika-tui is optional -- the engine does not know it exists. The engine has 134,000 lines. The TUI has 92,000. They are independent.

---

### SLIDE 18 -- The RunContext

[SLIDE] DashMap-based concurrent store:

```rust
pub struct RunContext {
    results: DashMap<Arc<str>, TaskResult>,
}
```

```
Write: O(1) amortized, shard-level locking
Read:  O(1), multiple readers per shard
No global lock. Tasks write in parallel without blocking.
```

**Speaker Notes:**
The RunContext is the runtime's shared memory. DashMap gives us sharded concurrent access. Multiple tasks writing results simultaneously -- no contention. Arc<str> keys from the interner means comparison is a pointer check, not a string comparison. Each TaskResult stores the output, duration, token usage, cost, and media references.

---

### SLIDE 19 -- Event Sourcing

[SLIDE] Event types and trace format:

```rust
pub enum EventKind {
    WorkflowStarted,
    TaskStarted { task_id: Arc<str> },
    TaskCompleted { task_id: Arc<str>, duration: Duration },
    InferStream { task_id: Arc<str>, chunk: String },
    // ... 41 total variants
}
```

```ndjson
{"kind":"TaskStarted","task_id":"fetch_repo","ts":"2026-03-23T10:14:22Z"}
{"kind":"FetchComplete","task_id":"fetch_repo","status":200,"ms":342}
```

**Speaker Notes:**
Forty-one event types. Append-only log. NDJSON trace files. Every action in the runtime emits a structured event. The TUI subscribes for real-time display. The trace writer records for offline analysis. Events are structured data, not log strings -- you can query, filter, and replay them.

---

### SLIDE 20 -- Error Design

[SLIDE] NikaError design principles:

```
1. Every error has a unique NIKA-XXX code (000-319)
2. Every error carries a source span when applicable
3. NikaError, not anyhow -- type preservation matters
4. Categorized ranges for searchability
5. Suggestions included (Did you mean...?)
```

**Speaker Notes:**
Over three hundred error codes. Categories by range: 000 for workflow, 020 for DAG, 100 for MCP, 300 for structured output. Every error with a source span points to the exact line in your YAML. We use NikaError, not anyhow -- because anyhow erases type information. When an error crosses a crate boundary, it carries its full context. And many errors include suggestions.

---

### SLIDE 21 -- Fetch Extraction

[SLIDE] Nine modes with their backing libraries:

```
markdown     htmd              Clean Markdown from any HTML
article      dom_smoothie      Readability-style article extraction
text         built-in          Visible text with optional CSS selector
selector     scraper           CSS selector on raw HTML
metadata     built-in          OG, Twitter, JSON-LD, SEO tags
links        built-in          Classified link inventory
jsonpath     built-in          JSONPath queries on JSON APIs
feed         feed-rs           RSS, Atom, JSON Feed parsing
llm_txt      built-in          AI-era content discovery
```

**Speaker Notes:**
The fetch verb ships with nine extraction modes. Not as plugins -- as built-in post-processing. `markdown` converts any web page to clean Markdown using htmd. `article` uses a Readability-style algorithm to extract the main content. `jsonpath` queries JSON APIs without dependencies. Each mode has a dedicated library chosen for correctness, not convenience.

---

### SLIDE 22 -- Media Pipeline

[SLIDE] 24 tools in three tiers:

```
Tier 1 (always): import, dimensions, thumbhash, dominant_color, pipeline
Tier 2 (default): thumbnail (SIMD), convert, strip, metadata, optimize, svg_render
Tier 3 (opt-in): phash, compare, pdf_extract, chart, provenance, verify,
                  qr_validate, quality, html_to_md, css_select, ...
```

**Speaker Notes:**
Twenty-four media tools. SIMD-accelerated thumbnail generation via Lanczos3. Lossless PNG optimization via oxipng. C2PA content provenance for EU AI Act compliance. QR code validation with scan scoring. All backed by content-addressable storage with blake3 and zstd. Import once, reference by hash, chain operations without intermediate files.

---

### SLIDE 23 -- The Agent Loop

[SLIDE] Agent architecture diagram:

```
Initial prompt + tool definitions
         |
         v
    LLM response
         |
    Tool calls?  --yes-->  Execute tools --> Add results --> Loop back
         |
         no
         |
    Check stop conditions:
      - Guardrails (length, schema, regex, LLM validation)
      - Completion mode (explicit, natural, pattern)
      - stop_sequences, confidence, max_turns, token_budget
         |
    Done? --> Return result
```

**Speaker Notes:**
The agent verb creates autonomous loops. The LLM receives tools and decides which to call. After each response, stop conditions are evaluated: guardrails for quality, completion modes for success criteria, hard limits for safety. Guardrails can be regex patterns, JSON schema validation, or even LLM-powered evaluation. The agent keeps iterating until a stop condition is met or the budget is exhausted.

---

### SLIDE 24 -- The LSP

[SLIDE] Two-crate architecture:

```
nika-lsp-core (9k)           nika-lsp (2.5k)
Protocol-agnostic            tower-lsp-server binding
  - Completions              12 LSP handlers
  - Diagnostics
  - Hover docs
  - Semantic tokens
```

**Speaker Notes:**
The LSP is split into two crates. The intelligence is protocol-agnostic -- completions, diagnostics, hover, semantic tokens -- and can be reused for web editors or validation APIs. The server crate is a thin binding to tower-lsp. Twelve handlers cover the feature surface.

---

### SLIDE 25 -- The TUI

[SLIDE] Three-view architecture:

```
Home:     File browser + workflow preview
Studio:   YAML editor + syntax highlighting + LSP inline
Monitor:  Live DAG + streaming output + mission control
```

```
92,000 lines | 40+ custom widgets | ratatui 0.30
tree-sitter YAML highlighting | petgraph DAG visualization
```

**Speaker Notes:**
Ninety-two thousand lines of terminal UI. Three views. Forty-plus custom widgets including DAG rendering with petgraph, syntax highlighting with tree-sitter, inline LLM streaming, matrix-style animation effects, and a full command palette. It is not a log viewer -- it is a cockpit.

---

## PART 5: DEMO 2 (22:00 - 25:00)

### SLIDE 26 -- Demo Title

[SLIDE] "DEMO: Media Pipeline + TUI" with terminal cursor.

---

### Demo Script (22:00 - 25:00)

**Step 1 (22:00 - 23:00): Media workflow**

[SCREEN] Write and run a media workflow:
```yaml
schema: nika/workflow@0.12

tasks:
  - id: import
    invoke:
      tool: nika:import
      input:
        path: ./demo-photo.jpg

  - id: thumb
    depends_on: [import]
    with: { img: $import }
    invoke:
      tool: nika:pipeline
      input:
        hash: "{{with.img.hash}}"
        steps:
          - operation: thumbnail
            width: 400
            height: 300
          - operation: convert
            format: webp
          - operation: strip

  - id: analyze
    depends_on: [import]
    with: { img: $import }
    infer:
      model: claude/claude-sonnet-4-20250514
      content:
        - type: image
          source: "{{with.img.hash}}"
        - type: text
          text: "Describe this image in one sentence."
```

[TYPE] `nika run media-demo.nika.yaml`

**Voice-over:**
Import a photo. In parallel: pipeline it to a WebP thumbnail, and send it to Claude for vision analysis. Three tasks, two execution layers. The pipeline chains thumbnail, conversion, and metadata stripping in memory -- zero intermediate files.

**Step 2 (23:00 - 24:00): TUI Monitor**

[SCREEN] Run the same workflow in the TUI Monitor view.

**Voice-over:**
Now watch it in the TUI. Import completes. Two tasks launch in parallel. The pipeline processes the image while Claude analyzes it. Both complete. Look at the DAG -- green nodes, timing information, the streaming Claude response appearing in real-time in the output panel.

**Step 3 (24:00 - 25:00): Course demo**

[TYPE] `nika init --course`
[TYPE] `nika course status`

[SCREEN] Show the constellation progress map.

**Voice-over:**
And if anyone wants to learn all of this, we built a twelve-level interactive course. Forty-four exercises. From Jailbreak to SuperNovae. Progressive hints, auto-checking, a constellation progress map. Built into the binary.

---

## PART 6: THE ECOSYSTEM (25:00 - 28:00)

### SLIDE 27 -- The Brain + Body Architecture

[SLIDE] SuperNovae architecture:

```
NovaNet (Brain)                    Nika (Body)
  Knowledge Graph                    YAML Workflows
  59 NodeClasses                     5 Semantic Verbs
  Neo4j + MCP Tools                  DAG Execution
                                     8 LLM Providers

         <---- MCP Protocol ---->
         Zero Cypher in Nika.
         Knowledge stays in NovaNet.
         Execution stays in Nika.
```

**Speaker Notes:**
Nika is one half of the SuperNovae architecture. NovaNet is the brain -- a knowledge graph with fifty-nine node classes in Neo4j. Nika is the body -- workflow execution. They communicate exclusively through MCP. The zero-Cypher rule: Nika workflows never write database queries. They invoke semantic tools. This separation means you can use Nika without NovaNet, or swap NovaNet for any MCP-compatible backend.

---

### SLIDE 28 -- The Course

[SLIDE] Twelve-level course visualization:

```
Level  1: Jailbreak        -- exec: basics
Level  2: Hot Wire          -- fetch: HTTP
Level  3: Fork Bomb         -- DAG patterns
Level  4: Root Access       -- infer: LLM
Level  5: Shapeshifter      -- with: bindings
Level  6: Pay-Per-Dream     -- structured output
Level  7: Swiss Knife       -- builtin tools
Level  8: Gone Rogue        -- agent: loops
Level  9: Data Heist        -- fetch: extraction
Level 10: Open Protocol     -- invoke: MCP
Level 11: Pixel Pirate      -- media pipeline
Level 12: SuperNovae (Boss) -- full orchestration
```

**Speaker Notes:**
Twelve levels, forty-four exercises. Liberation-themed names because Nika is about freedom -- freedom from framework lock-in, freedom from runtime errors, freedom from complexity. Each level builds on the previous. Level 12 is the boss level: full production workflows using everything you learned.

---

### SLIDE 29 -- Showcase

[SLIDE] "200+ showcase workflows" with category examples:

```
nika showcase list           -- Browse all
nika showcase extract <name> -- Pull to your project

Categories: fetch patterns, LLM workflows, media pipelines,
            infrastructure, advanced patterns
```

**Speaker Notes:**
Over two hundred showcase workflows. Working, tested examples for every use case. `nika showcase extract` pulls one into your project, ready to run. They are not documentation -- they are production patterns.

---

### SLIDE 30 -- Roadmap

[SLIDE] Three waves:

```
Wave 1: 4-preset model routing (default/lite/think/coder)
Wave 2: Orchestration mode + context budgeting
Wave 3: Punk Records (3-tier memory: HOT -> WARM -> COLD)
```

**Speaker Notes:**
Three waves ahead. Model routing with cognitive presets -- the right model for the right task. Orchestration mode where an LLM dynamically dispatches tasks. And Punk Records -- a three-tier memory system from in-memory through local NDJSON to NovaNet's knowledge graph. The DAG is the kernel. We are upgrading it.

---

## PART 7: CLOSE (28:00 - 30:00)

### SLIDE 31 -- Why Rust

[SLIDE] Single slide with Rust logo:

```
Not "Rust is fast" -- Rust is correct.
The type system lets us encode pipeline guarantees.
Raw != Analyzed != Lowered.
The compiler enforces the architecture.
```

**Speaker Notes:**
I want to close with why Rust. Not because it is fast -- though it is. Because Rust's type system lets us encode architectural guarantees. A Raw AST and an Analyzed AST are different types. You cannot pass one where the other is expected. The compiler enforces our pipeline design. In Python, these boundaries are conventions. In Rust, they are types.

---

### SLIDE 32 -- Why AGPL

[SLIDE] License philosophy:

```
AGPL-3.0-or-later

"Open source means protecting the commons,
 not donating your work to cloud platforms."
```

**Speaker Notes:**
AGPL because open source is about freedom for everyone, not just corporations. If you modify Nika and offer it as a service, you share your modifications. The freedom to use comes with the responsibility to contribute.

---

### SLIDE 33 -- Getting Started

[SLIDE] Three commands:

```bash
cargo install nika
nika init --course
nika course next
```

**Speaker Notes:**
Three commands to start. Install, initialize the course, open your first exercise. Or `nika showcase list` for two hundred examples.

---

### SLIDE 34 -- Thank You

[SLIDE] Butterfly logo. GitHub URL. Contact information.

```
github.com/supernovae-st/nika
@supernovae-st | @ThibautMelen
supernovae.studio

"Limited only by the YAML you write."
```

**Speaker Notes:**
Nika. Ten crates. 1.56 million lines. Five verbs. Zero compromises. The source is on GitHub. The course is built in. Go build something.

Thank you.

---

## Q&A Preparation (Anticipated Questions)

### Q: Why YAML instead of a programming language like Python?

**A:** Declarative workflows are validatable before execution. YAML is serializable, diffable, and version-controllable. The five verbs provide the escape hatch for arbitrary logic. We get LSP completions for free from the schema. And the three-phase AST means we can catch errors at analysis time, not runtime.

### Q: How does Nika compare to LangGraph specifically?

**A:** LangGraph gives you graph-based workflows in Python. Nika gives you the same concept in YAML with a Rust runtime. The key differences: validation before execution, native parallelism via Tokio instead of asyncio, structured error codes instead of Python exceptions, and a single static binary instead of a Python environment.

### Q: Can I extend Nika with custom verbs?

**A:** The verb set is intentionally closed. You extend Nika through composition: exec for arbitrary shell commands, invoke for MCP tools, agent for autonomous loops. If you need a custom integration, write an MCP server and invoke it. This keeps the core engine simple and the extension surface well-defined.

### Q: What is the testing strategy?

**A:** 8,300+ tests. Unit tests with `cargo test --lib`. Snapshot tests with insta. Property tests with proptest. We use `--lib` exclusively because regular `cargo test` triggers macOS Keychain popups from integration tests. Zero clippy warnings enforced. Opt-level 1 for test builds to balance speed and compile time.

### Q: Why not use anyhow for errors?

**A:** anyhow erases type information. When an error crosses a crate boundary, you lose the ability to match on it. NikaError preserves the full context: error code, source span, category. Every error is categorized, searchable, and actionable. This matters for user experience and for debugging.

### Q: Is Nika production-ready?

**A:** Nika is at v0.49.0 with 8,300+ tests. It powers QR Code AI's content pipelines. The codebase has been through multiple deep audits. That said, the schema is still @0.12 and we intentionally stay at 0.x.x -- there are no users yet to break backward compatibility for, and we want the freedom to evolve rapidly.

### Q: How does the AGPL license affect commercial use?

**A:** You can use Nika commercially. You can modify it. You can run it on your servers. The AGPL requirement kicks in when you offer a modified version as a network service -- then you must share your modifications. If you use Nika as-is, no obligation beyond attribution.

---

## Technical Requirements

- Projector: 1920x1080 minimum, 16:9
- Terminal font: JetBrains Mono, 18pt (visible from back of room)
- Dark terminal theme with high contrast
- VS Code with Nika LSP extension for demo
- Internet connection for fetch demos (have backup screenshots)
- Backup video of each demo segment
- Presentation timer visible to speaker
- Remote clicker for slides
