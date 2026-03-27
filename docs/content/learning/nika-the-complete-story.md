# Nika: The Complete Story

## How a Solo Developer Built a 451,000-Line Rust Workflow Engine to Democratize AI

In 2026, six closed labs control frontier AI. Chips cost $6 million per rack. LLM subscriptions run $20 to $200 a month. And even if you pay, you still need a software engineer to wire anything useful together. The technology that should empower billions is gatekept by a handful of corporations.

A French developer named Thibaut Melen looked at this landscape and asked a heretical question: what if AI workflows could be declared in a text file and run from a single binary? What if the entire thing was written in Rust so it compiled to a zero-dependency binary that ran on any machine? What if the gap between "AI exists" and "I can use AI" could be reduced to zero?

That question became Nika. This is the story of how that project came to be, what it does, why it matters, and where it is going.

---

## The Problem: AI Orchestration is Broken

To understand why Nika exists, you need to understand the state of AI workflow tooling in 2025 and 2026. The AI revolution brought extraordinary capabilities — language models that can write code, analyze images, reason about complex problems, and generate creative content. But chaining these capabilities together into reliable, reproducible pipelines remained surprisingly difficult.

If you wanted to build a workflow that scrapes a website, sends the content to an LLM for analysis, generates a report, and posts the results to an API, you had roughly five options. You could write a Python script using LangChain, which meant hundreds of lines of imperative code that were difficult to version-control, impossible to audit, and fragile to maintain. You could use a visual builder like Dify or n8n, which required a server, a database, and Docker. You could reach for a data pipeline tool like Prefect or Airflow, designed for ETL jobs and awkwardly retrofitted for AI tasks. You could write raw API calls with requests and openai, producing throwaway scripts with no structure. Or you could use a multi-agent framework like CrewAI or AutoGen, which meant Python, more Python, and a lot of boilerplate Python.

Every single major AI orchestration tool in 2025 required either Python, a server, Docker, or Kubernetes. Often several of these at once. The barrier to entry was enormous, and the barrier to maintainability was worse. A LangChain workflow that worked last Tuesday might break next Tuesday because a library updated, a prompt format changed, or a chain was deprecated.

Thibaut looked at this landscape and asked a heretical question: what if AI workflows could be declared in YAML and run from a single binary, the same way you run `git` or `cargo`? What if the workflow definition was the documentation, the specification, and the executable, all in one file? What if the entire thing was written in Rust so it compiled to a zero-dependency binary that ran on any machine without Python, Node.js, Docker, or the cloud?

That question became Nika.

---

## What Nika Actually Is

At its core, Nika is a semantic YAML workflow engine for AI tasks. You write a `.nika.yaml` file that describes a series of tasks, and Nika parses it, validates it, constructs a directed acyclic graph (DAG) of dependencies, and executes the tasks in the correct order — automatically parallelizing tasks that have no dependencies on each other. The results of each task flow into the next through a typed binding system that uses template expressions like `{{with.previous_result}}`.

Here is the simplest possible Nika workflow:

```yaml
schema: "nika/workflow@0.12"
workflow: hello-world

tasks:
  - id: greet
    infer: "Say hello to the world"
```

That is a complete, valid, executable workflow. It declares the schema version, names the workflow, defines one task that calls a language model with a prompt, and that is it. Run `nika run hello-world.nika.yaml` and Nika will detect which LLM provider you have configured (checking for API keys in order: Anthropic, OpenAI, Mistral, Groq, DeepSeek, Gemini), call the model, and display the result.

But Nika scales from that trivial example to workflows with dozens of tasks, multiple LLM providers, MCP tool integrations, structured JSON output validation, image processing pipelines, and autonomous agent loops — all declared in the same YAML format.

---

## The Five Verbs: Nika's Grammar

The design philosophy of Nika crystallizes in what the project calls "the five verbs." Every task in a Nika workflow does exactly one of five things:

**infer:** calls a language model. This is the AI verb — you give it a prompt, optionally a system message, temperature, max tokens, and other parameters, and it returns the LLM's response. It supports 9 LLM providers through rig-core, including Claude, GPT-4, Mistral, Groq, DeepSeek, Gemini, xAI, and local models via mistral.rs. It handles vision (multimodal content with images), extended thinking (Claude's chain-of-thought), streaming, and structured output with JSON schema validation.

**exec:** runs a shell command. This is the system verb — you give it a command string, and Nika runs it via tokio process spawn, captures the output, and makes it available to downstream tasks. It includes a 28-pattern security blocklist that prevents dangerous commands like `rm -rf /` or `curl | bash`, and it uses NFKC Unicode normalization to prevent homoglyph attacks.

**fetch:** makes an HTTP request. This is the network verb — you specify a URL, method, headers, and body, and Nika performs the request and returns the response. It supports nine extraction modes for post-processing HTML responses: markdown conversion, article extraction via readability, text extraction, CSS selector matching, metadata extraction (OpenGraph, Twitter Cards, JSON-LD), link classification, JSONPath queries on JSON APIs, RSS/Atom feed parsing, and AI-era content discovery via llms.txt. It also supports binary mode with content-addressable storage for downloading images and files.

**invoke:** calls an MCP tool. This is the integration verb — you specify an MCP server and tool name, along with arguments, and Nika calls the tool through the Model Context Protocol. This is how Nika connects to NovaNet (the knowledge graph), external databases, code execution environments, and any other tool that speaks MCP. The MCP client uses rmcp 0.16 with retry and reconnection logic.

**agent:** runs an autonomous multi-turn loop. This is the agentic verb — you define an agent with a system prompt, available tools, guardrails, completion conditions, and cost limits, and Nika runs a multi-turn conversation loop where the LLM can call tools, process results, and decide when it is done. Agents can spawn sub-agents with configurable depth limits, use extended thinking, and maintain chat history across turns.

The decision to have exactly five verbs — no more, no fewer — was deliberate and controversial. Many users' first instinct is to ask for more verbs. Why not a `code:` verb for running Python? Why not a `transform:` verb for data manipulation? Why not a `wait:` verb for delays?

The answer is that the five verbs are primitive operations that compose to express any workflow. A `code:` verb is just `exec: "python3 -c '...'"`. A `transform:` is handled by the pipe transform system built into bindings (31 chained operations including sort, unique, filter, map, group_by, and regex). A `wait:` is `invoke: nika:sleep`. The constraint of five verbs forces clarity. Every task has a single, unambiguous purpose. You can look at any task in any workflow and instantly understand what kind of operation it performs.

This design principle — radical simplicity through constraint — runs through every aspect of Nika's architecture.

---

## The Three-Phase AST: Why Most YAML Tools Are Wrong

Most YAML-based tools parse YAML into a data structure and immediately try to execute it. This is the equivalent of running a script without compiling it first, and it leads to the same class of problems: errors discovered at runtime instead of authoring time, vague error messages, no IDE support, and brittle execution.

Nika takes a fundamentally different approach. It treats `.nika.yaml` files the way a compiler treats source code, processing them through a three-phase pipeline inspired by rustc itself.

Phase 1, the raw parse, uses the marked-yaml crate to parse YAML into a Raw AST where every single value carries a source span — the exact file, line, and column where it appeared. This means error messages can point to the exact character that caused the problem. No "error somewhere in your YAML" — Nika tells you exactly where.

Phase 2, the analysis phase, is where the real work happens. The analyzer validates the schema version, interns task IDs (converting string names to u32 integers for O(1) comparison), parses binding expressions, detects dependency cycles via depth-first search, resolves MCP server references, and extracts implicit dependencies. Critically, this phase collects ALL errors in a single pass rather than stopping at the first one. This means an IDE or the CLI can show you every problem in your workflow at once, just like a modern compiler.

Phase 3, the lowering phase, converts the validated analyzed AST into runtime-optimized types. Spans are stripped, FxHashMap replaces HashMap for faster non-cryptographic hashing, and tasks are wrapped in Arc for zero-copy sharing across Tokio task spawns.

This three-phase architecture is why Nika can have a Language Server Protocol (LSP) implementation that provides real-time completion, hover information, go-to-definition, and diagnostics as you type. The same analysis pipeline that catches errors at runtime also powers the editor experience. Very few YAML-based tools have anything approaching this level of tooling.

---

## The Connection to NovaNet: Brain and Body

Nika is half of a two-part system. The other half is NovaNet, a knowledge graph engine built on Neo4j. The relationship between them is captured in what the project calls "The Golden Rule":

NovaNet is the brain — it knows things. It manages entities, locales, semantic relationships, and knowledge atoms in a graph database with 59 node classes and over 200 locale definitions. Nika is the body — it does things. It executes workflows, calls LLMs, fetches data, and produces artifacts.

The two communicate exclusively through the Model Context Protocol (MCP). Nika never touches the Neo4j database directly — there is zero Cypher in the entire Nika codebase. When a Nika workflow needs to look up an entity, retrieve knowledge, or store results, it uses `invoke:` to call NovaNet's MCP tools. This clean separation means either system can evolve independently, and Nika can be used without NovaNet at all.

This architecture was inspired by Dr. Vegapunk from One Piece — the scientist who externalized his brain into satellite workers. In the manga, Vegapunk's body is on Egghead Island while his knowledge lives in the Punk Records system and his satellites handle specialized tasks. In the SuperNovae architecture, NovaNet is Punk Records (the externalized knowledge), Nika is the body (the execution engine), and the MCP protocol is the communication channel between brain and body.

---

## The Vegapunk Naming System: When Manga Meets Software Architecture

One of the most distinctive aspects of Nika is its naming system, which draws deeply from the One Piece manga — specifically from the Egghead Island arc and Dr. Vegapunk's satellite system. This is not superficial branding. The One Piece parallels encode genuine architectural decisions.

In the manga, Dr. Vegapunk is a genius scientist who externalized his brain into six satellite bodies, each representing a different aspect of his personality: Shaka (wisdom), Lilith (evil/defense), Edison (thinking/invention), Pythagoras (logic), Atlas (violence/force), and York (greed/desire). Each satellite operates independently but shares knowledge through the Punk Records system.

In the SuperNovae architecture, this mapping is remarkably precise. The orchestrator (the component that dispatches tasks to the right handler) maps to Shaka, the wisest satellite who makes strategic decisions. The primary creative model slot maps to Edison, the inventive satellite. The deep reasoning model slot maps to Pythagoras, the logical satellite. The fast execution model slot maps to Atlas, the forceful satellite. The search and retrieval model slot maps to York, the resource-gathering satellite. The security layer maps to Lilith, the defensive satellite.

Even the in-memory task result store was originally named "Egghead" after Vegapunk's island laboratory — a temporary research space that is destroyed when the arc ends, just as the RunContext is destroyed when a workflow completes.

The naming evolved over time from pure One Piece references to descriptive presets (edison became "default," pythagoras became "think," atlas became "lite," york became "search"), because the original names created onboarding friction — new users should not need to know One Piece lore to use a workflow engine. But the architectural mapping remains: the system's structure mirrors the manga's satellite architecture because both solve the same problem — distributing specialized cognitive tasks across specialized workers under a coordinating intelligence.

The project expanded beyond the original four Vegapunk slots to eight presets covering all functional roles: default (primary creative work), lite (fast execution), think (deep reasoning), search (retrieval), vision (visual analysis), judge (quality evaluation), coder (code generation), and summary (compression and extraction). This expansion was validated through research across six frameworks and four academic papers.

---

## The Event System: Observability as a First-Class Concern

Most workflow tools treat logging as an afterthought — you add print statements or configure a logging framework. Nika treats observability as a core architectural concern, with 41 event types that record every significant action during workflow execution.

The event system writes NDJSON (Newline-Delimited JSON) trace files that capture the complete execution history: when the workflow started, when each task began and ended, what prompts were sent to LLMs, what responses came back, what tools were called, what errors occurred, how many tokens were consumed, and what the total cost was. These trace files are machine-readable, which means they can be ingested by monitoring systems, analyzed by debugging tools, and used for cost tracking.

The event types tell the story of a workflow execution: WorkflowStarted, TaskStarted, InferenceStarted, InferenceCompleted, StreamTokenReceived, ToolCallStarted, ToolCallCompleted, AgentTurnStarted, AgentTurnCompleted, TaskCompleted, TaskFailed, WorkflowCompleted. Each event carries a timestamp, the task ID, and event-specific metadata.

This level of observability is unusual in the workflow engine space. Airflow has logging. Prefect has event streams. But neither has the structured, typed, machine-readable trace format that Nika produces. The trace files are the forensic record of what happened, and they enable post-mortem debugging, cost analysis, and performance optimization in ways that unstructured logs cannot.

---

## The MCP Revolution and Nika's Role In It

The Model Context Protocol (MCP) is a standard created by Anthropic for connecting AI agents to external tools. It defines a JSON-RPC 2.0 interface that any tool can implement, allowing AI systems to discover and call tools without hardcoded integrations. MCP was first supported by Claude Desktop, then adopted by Claude Code, ChatGPT, Gemini, Cursor, VS Code, Windsurf, and even Windows 11.

Nika holds a unique position in the MCP ecosystem. It is the first non-IDE, non-assistant CLI tool to implement the MCP client protocol for workflow automation. Every other MCP client is an AI assistant (Claude, ChatGPT), an IDE (Cursor, VS Code), or an operating system (Windows 11). Nika is a workflow engine — it uses MCP not for interactive conversation but for automated, reproducible tool calling within DAG-scheduled workflows.

This distinction matters because it demonstrates that MCP is not just a protocol for chatbots — it is a general-purpose integration protocol for AI systems. A Nika workflow can call any MCP server, which means any tool that exposes an MCP interface is automatically available to any Nika workflow. As the MCP ecosystem grows (and it is growing rapidly, with thousands of MCP servers already available), Nika's capability surface grows with it.

The MCP client implementation uses rmcp v0.16 with stdio transport, which means MCP servers are spawned as child processes and communication happens through standard input and output. This is the same transport used by Claude Code and most other MCP clients. The client manages connection pooling, retry logic, tool discovery, and argument serialization.

Nika also defines MCP aliases — short names that map to full MCP tool paths. For example, `novanet` maps to the NovaNet MCP server's tools, and the various provider aliases map to their respective MCP tool paths. These aliases make workflow YAML more readable and reduce the boilerplate of specifying full server and tool name combinations.

---

## Why AGPL: The Open Source Philosophy

Nika is licensed under AGPL-3.0-or-later, and this is a deeply intentional choice. The AGPL is the strongest copyleft license in common use — it requires that anyone who runs AGPL software over a network must make the source code available to users. This is specifically designed to prevent what is called "cloud exploitation" — the practice where a cloud provider takes open source software, runs it as a service, and never contributes back.

The creator of Nika is an open source activist who views the current AI landscape through the lens of the Great Pirate Era from One Piece. In this framing, open source AI projects are the pirates fighting for freedom against the "World Government" of closed-source big tech. The AGPL license is the weapon that ensures Nika remains free — anyone can use it, modify it, and build on it, but they cannot privatize it.

This philosophy extends to the project's visual identity. The project's symbol is a blue butterfly — representing transformation, courage, renewal, and new beginnings. The butterfly appears on Nika's flag alongside the SuperNovae logo and the five verbs carved into the hull of the flagship.

---

## The Solo Developer Story: 450K+ Lines of Rust

One of the most remarkable aspects of Nika is its scale. As of version 0.49.0, the project contains over 450,000 lines of Rust source code organized into 12 workspace crates. The largest crate, nika-engine, contains the execution runtime. The TUI (terminal user interface) is a major subsystem. Multiple specialized crates handle media processing, MCP integration, CLI, and LSP.

This is the work of a solo developer, Thibaut Melen, working with AI assistance. The project maintains a zero clippy warnings policy, uses structured error codes (NIKA-001 through NIKA-319+), and has a comprehensive test suite exceeding 8,300 tests. The codebase includes snapshot testing via insta, property-based testing via proptest, and a strict pre-commit workflow that requires all tests, linting, and type-checking to pass before any commit.

The choice of Rust was not accidental. Rust provides memory safety without garbage collection, which means Nika can process images, manage concurrent network connections, and run multiple LLM calls simultaneously without the overhead of a runtime garbage collector. The release profile uses thin LTO (link-time optimization) and single codegen unit for maximum binary optimization, with debug symbols stripped. The result is a single, self-contained binary that runs anywhere Rust compiles.

---

## The Course: Liberation Through Learning

Nika includes a built-in 12-level interactive course called "Liberation," themed after the One Piece narrative of freedom and discovery. The course takes learners from writing their first `exec:` command to orchestrating full production workflows, with 44 exercises organized in progressive difficulty.

The levels are named with a hacker-liberation theme: Jailbreak (basic shell commands), Hot Wire (network requests), Fork Bomb (DAG patterns and parallelism), Root Access (first LLM calls), Shapeshifter (data transformation), Pay-Per-Dream (structured output), Swiss Knife (builtin tools), Gone Rogue (autonomous agents), Data Heist (advanced extraction), Open Protocol (MCP integration), Pixel Pirate (media pipeline), and SuperNovae (the final boss level combining everything).

The course design was informed by deep research into Rustlings, Ziglings, Exercism, Codecrafters, and other interactive learning tools. The golden rule: the exercise file IS the lesson. Each exercise is a `.nika.yaml` file with inline comments explaining the concept, a broken or incomplete workflow to fix, and progressive hints accessible via `nika course hint`. The watch mode (`nika course watch`) automatically re-validates exercises on file save, creating a tight edit-save-feedback loop.

In addition to the course, Nika includes a showcase system with 115 example workflows covering everything from content generation to data analysis to media processing, all extractable via `nika showcase extract`.

---

## Where Nika is Going

The project's roadmap extends through several waves of planned features. The vision includes 4-slot model routing (routing different cognitive tasks to different LLM providers within a single workflow), record compression (LLM-generated summaries at task completion boundaries to manage context window growth), an orchestrate mode (where Nika's orchestrator dynamically writes and executes new workflows to achieve a stated goal), context budget management (preventing the "dumb zone" where LLM performance degrades from too much context), and 3-tier persistent memory (hot in-memory results, warm local disk storage in NDJSON format, and cold promotion to NovaNet's knowledge graph for cross-session learning).

Perhaps the most ambitious planned feature is the orchestrate mode. When given a `goal:` instead of explicit tasks, Nika's orchestrator would plan in YAML — dynamically generating, executing, evaluating, and improving `.nika.yaml` workflows to achieve the goal. No other framework has an orchestrator that plans in its own workflow language. LangGraph's plans are opaque Python, CrewAI's plans are non-deterministic natural language, AutoGen's plans are implicit conversations. Nika's plans would be deterministic, auditable, reusable YAML files that can be saved, version-controlled, and reviewed alongside hand-written workflows.

The integration story is equally ambitious. As of version 0.49.0, Nika integrates with multiple AI coding tools — Claude Code, Cursor, Copilot, and others — through universal Agent Skills that teach these tools how to write `.nika.yaml` workflows. A Claude Code plugin provides full integration with skills, agents, hooks, and MCP connectivity. The vision is simple: when someone installs Nika, AI coding tools should instantly understand how to work with it.

---

## The QR Code AI Connection

Nika and NovaNet are not academic exercises. They are built to serve a real product: QR Code AI, a SaaS platform for AI-powered QR code generation. This is important context because it means every feature in Nika is motivated by a real production use case, not by theoretical elegance.

QR Code AI needs to generate landing pages in multiple languages, analyze QR code scan quality, process images through a media pipeline, integrate with knowledge graphs for entity-specific content, track costs across multiple LLM providers, and produce reproducible results that can be audited and improved over time. These requirements drove features that might otherwise seem academic: the media pipeline exists because QR Code AI processes thousands of images, the structured output exists because marketing copy needs to conform to brand guidelines, the multi-provider support exists because different tasks have different cost-quality tradeoffs, and the event system exists because production workflows need debugging.

The relationship between a product and its tooling is bidirectional. QR Code AI drives Nika's feature development, and Nika's capabilities expand what QR Code AI can do. This virtuous cycle is why Nika has such a comprehensive feature set for a project created by a solo developer — every feature solves a real problem that the creator encountered in production.

---

## The Developer Experience Philosophy

One of the less visible but most important aspects of Nika is its investment in developer experience. The TUI alone accounts for 92,959 lines of code — nearly a third of the total codebase. This is not accidental. The project's philosophy is that tools should be a pleasure to use, not just functional.

The error messages are modeled after rustc's output. Instead of generic "Error: invalid YAML" messages, Nika produces structured diagnostics with the exact source location, an error code (NIKA-XXX), a clear description of what went wrong, and often a "did you mean?" suggestion computed via Jaro-Winkler fuzzy matching. If you write `provder: claude` instead of `provider: claude`, Nika will suggest the correct spelling.

The LSP integration means that developers using VS Code, Cursor, or any editor that supports the Language Server Protocol get real-time feedback as they type. Completions suggest valid task IDs for depends_on references, valid provider names, valid model names, and valid MCP tool names. Hover information shows the type and description of each field. Diagnostics highlight errors before the workflow is even saved.

The CLI is designed with discoverability in mind. `nika --help` shows all subcommands. `nika run --help` shows all run options. `nika provider list` shows which API keys are configured and which providers are available. `nika doctor` checks system configuration and reports issues. `nika check workflow.nika.yaml` validates a workflow without executing it.

This level of polish is unusual for a pre-release project. Most tools at Nika's stage prioritize functionality over usability. The project's creator takes the opposite view: usability IS functionality. A tool that is difficult to use will not be used, regardless of how powerful it is.

---

## The Academic Foundations

Nika's design is informed by academic research in AI agent architectures. The project's vision documents reference six academic papers and seven industry products, and the design decisions are explicitly traced back to their research foundations.

The Slate framework by Random Labs introduced the concept of "thread weaving" — implicit adaptive decomposition where an orchestrator dynamically dispatches one-shot tasks and synthesizes their results. Nika adopted this concept for its planned orchestrate mode, where the orchestrator generates and executes workflows dynamically.

The THREAD paper (arXiv:2405.17402) contributed hierarchical agent decomposition with resource-aware model selection — the idea that different subtasks should use different models based on their cognitive requirements. This directly inspired Nika's model slot architecture.

The Context-Folding paper (arXiv:2510.11967) introduced sub-trajectory compression for reducing context growth in long-running agents. This informed Nika's Record compression system, where task results are summarized at completion boundaries to prevent context window degradation.

The Memory-R1 paper (arXiv:2508.19828) explored reinforcement learning-trained memory policies for agents, including confidence scoring and selective retention. This influenced Nika's planned confidence-based Record promotion system.

The RLM paper (arXiv:2512.24601) from MIT introduced Recursive Language Models with REPL memory — the idea that LLMs can use external working memory through recursive sub-calls. This validated Nika's approach of using the DAG as working memory and Records as compressed recall.

The CodeAct paper (arXiv:2402.01030, ICML 2024) demonstrated that code actions outperform tool-calling for LLM agents, with 451 citations validating the approach. While Nika uses tool-calling rather than code execution, the paper informed the design of the agent verb's tool integration.

These academic foundations give Nika's architecture a rigor that is uncommon in the workflow engine space. Design decisions are not made by intuition alone — they are validated against research findings and traced back to their sources.

---

## Why It Matters

The fundamental argument of Nika is that AI workflows should be data, not code. A YAML workflow file is human-readable, machine-parseable, version-controllable, diffable, auditable, and shareable. It captures not just what the workflow does, but why — through explicit task names, descriptions, dependencies, and structured bindings. When something goes wrong, the NDJSON trace file records every event with 41 event types, creating a complete forensic record of execution.

This matters because AI is becoming infrastructure. The same way web applications are built on HTTP, databases, and file systems, AI-powered applications are being built on LLMs, knowledge graphs, and tool-calling protocols. Nika provides the orchestration layer — the thing that connects all these pieces together and makes them work in concert.

There is a scene in One Piece where Whitebeard, mortally wounded and standing alone against the World Government, roars to the world: "The One Piece does exist!" This declaration changes everything — it confirms that the treasure is real and inspires a new generation of pirates to search for it. Nika makes a similar declaration about AI democratization: it IS possible to have a single binary that orchestrates any AI workflow, accessible to anyone who can write YAML, free and open source forever. The treasure is real. The workflow is the way to find it.

The project's tagline could be borrowed from the Sun God Nika himself: limited only by imagination. Or in YAML terms: limited only by the workflow you write.
