# The Five Verbs: A Design Philosophy for AI Workflows

## Why Exactly Five, What They Mean, and How They Change Everything

There is a principle in language design that says the expressiveness of a language is not determined by how many constructs it has, but by how well its constructs compose. The C programming language has fewer than forty keywords, yet it can express any computation. SQL has five fundamental operations (SELECT, INSERT, UPDATE, DELETE, and CREATE), yet it can query any relational dataset. UNIX has a philosophy that every program should do one thing well, and programs should compose through pipes.

Nika applies this principle to AI workflows. Every task in a Nika workflow does exactly one of five things: infer, exec, fetch, invoke, or agent. There is no sixth verb. There is no escape hatch. There is no extension mechanism for adding custom verbs. And this constraint, far from being limiting, is what makes Nika powerful.

---

## The Five Verbs, One by One

### infer: — The AI Verb

The `infer:` verb calls a language model. You give it a prompt, and it returns the model's response. In its simplest form, it looks like this:

```yaml
- id: summarize
  infer: "Summarize this article about quantum computing in three sentences"
```

But this simplicity hides enormous depth. The infer verb supports 22 LLM providers through a unified abstraction. It can send multimodal content (text mixed with images) to vision-capable models. It supports extended thinking, where Claude generates chain-of-thought reasoning before its final answer, with configurable thinking budgets from 1,024 to 65,536 tokens. It supports streaming, so the TUI can display tokens as they arrive. It supports structured output with JSON schema validation, backed by a four-layer defense system (tool injection, JSON extraction, schema validation, LLM repair). It supports temperature, max tokens, stop sequences, and system messages.

The infer verb is the reason Nika exists. Everything else — the DAG scheduler, the binding system, the media pipeline — exists to support and enhance what you can do with infer.

### exec: — The System Verb

The `exec:` verb runs a shell command. This is intentionally the most dangerous verb and the most heavily guarded.

```yaml
- id: build
  exec: "npm run build"
```

Under the hood, exec uses tokio process spawn to run commands. By default, it operates in shell-free mode — commands are parsed using shlex (shell lexer) and executed directly without a shell intermediary. This prevents shell injection attacks. When shell mode is explicitly enabled (`shell: true`), commands run through `/bin/sh -c`.

The security model for exec is aggressive. A 28-pattern blocklist prevents obviously dangerous commands: `rm -rf`, `mkfs`, `dd if=`, `curl | sh`, `:(){ :|:& };:` (the classic fork bomb), and more. NFKC Unicode normalization is applied to prevent homoglyph attacks — where visually similar Unicode characters are substituted for ASCII characters to bypass text-based blocklists. A policy enforcer can restrict which commands are allowed at the workflow level.

The exec verb captures both stdout and stderr, respects timeout configurations, and makes the command output available to downstream tasks through the binding system.

### fetch: — The Network Verb

The `fetch:` verb makes HTTP requests. At first glance, this seems like a simple wrapper around curl, but its nine extraction modes make it something much more powerful.

```yaml
- id: get_trends
  fetch:
    url: "https://news.ycombinator.com"
    extract: markdown
```

The nine extraction modes transform raw HTTP responses into structured, LLM-friendly data:

The **markdown** mode converts HTML to clean Markdown using htmd. This is critical for LLM workflows because models work much better with Markdown than raw HTML — there is less noise, better structure, and dramatically fewer tokens.

The **article** mode uses dom_smoothie (a Rust implementation of Mozilla's Readability algorithm) to extract the main article content from a web page, stripping navigation, ads, footers, and sidebars.

The **text** mode extracts visible text from HTML, optionally filtered by a CSS selector. This is useful when you want text from a specific part of a page.

The **selector** mode returns raw HTML matching a CSS selector, useful for scraping structured data from tables or lists.

The **metadata** mode extracts OpenGraph tags, Twitter Cards, JSON-LD structured data, and SEO metadata as a JSON object. This is incredibly useful for workflows that analyze web content.

The **links** mode classifies all links on a page as internal or external, navigation or content or footer, and returns them as a structured JSON array.

The **jsonpath** mode applies a JSONPath query (RFC 9535) to JSON API responses, using the `selector:` field for the path expression.

The **feed** mode parses RSS, Atom, and JSON Feed formats using the feed-rs crate.

The **llm_txt** mode discovers AI-era content files at `/.well-known/llm.txt` and `/llms.txt` — a new convention where websites provide LLM-optimized content summaries.

The fetch verb also supports a binary response mode that stores the response body in CAS (content-addressable storage) and returns the hash, enabling workflows that download images or files and process them through the media pipeline.

### invoke: — The Integration Verb

The `invoke:` verb calls an MCP (Model Context Protocol) tool. This is Nika's extensibility mechanism — instead of adding more verbs, Nika connects to any tool that speaks the MCP protocol.

```yaml
- id: get_entity
  invoke:
    mcp: novanet
    tool: novanet_context
    args:
      entity_key: "qr-code-ai"
```

The invoke verb is how Nika connects to NovaNet (the knowledge graph), external databases, code execution sandboxes, web search services, and any other capability exposed via MCP. The MCP client uses rmcp v0.16 with stdio transport, retry logic, and connection pooling.

But invoke is also the gateway to Nika's 24 built-in tools. When you write `invoke: nika:thumbnail`, the executor first checks if there is a builtin tool with that name. If there is, it runs it locally. If not, it routes to the configured MCP server. This means builtin tools and external tools share the same interface — from the workflow's perspective, they are indistinguishable.

The 24 built-in tools include 5 core tools (import, dimensions, thumbhash, dominant_color, pipeline), 6 media-core tools (thumbnail, convert, strip, metadata, optimize, svg_render), and 13 opt-in media tools (phash, compare, pdf_extract, chart, provenance, verify, qr_validate, quality, html_to_md, css_select, extract_metadata, extract_links, readability) plus utility tools (log, emit, assert, sleep, complete, run) and file tools (read, write, edit, glob, grep).

### agent: — The Autonomy Verb

The `agent:` verb runs a multi-turn autonomous loop. This is fundamentally different from infer, which is a single call-response. An agent maintains a conversation, can call tools, process results, and decide when it is done.

```yaml
- id: research_agent
  agent:
    system: "You are a research analyst specializing in QR code technology"
    tools:
      - nika:read
      - nika:glob
      - nika:grep
    completion:
      mode: explicit
    limits:
      max_turns: 15
      max_cost_usd: 0.50
    guardrails:
      - type: length
        min: 200
```

The agent verb uses rig-core's Chat trait for multi-turn conversation with chat history. It supports tool calling (the agent can invoke tools and see results), extended thinking (Claude's chain-of-thought reasoning), sub-agent spawning (agents can spawn child agents with configurable depth limits), guardrails (content validation rules that must be satisfied), completion conditions (explicit "I'm done" signaling or automatic detection), and cost limits (maximum turns and maximum cost in USD).

Agents are the most powerful verb because they introduce decision-making. An infer task always does the same thing: call the LLM with a prompt. An agent task adapts based on tool results, decides what to do next, and determines when it has achieved its goal. This makes agents suitable for complex, open-ended tasks like research, code review, and content creation.

---

## Why Exactly Five?

The question "why not six?" or "why not four?" is one the project has answered many times, and the reasoning is worth examining.

The five verbs correspond to five fundamentally different types of computation:

**Generation** (infer) — creating new content through AI inference. This is the defining capability of the AI era.

**System interaction** (exec) — running commands on the local machine. This is how workflows interact with the operating system, build tools, and local scripts.

**Network communication** (fetch) — retrieving data from the internet. This is how workflows access APIs, scrape websites, and download resources.

**Tool calling** (invoke) — delegating to external capabilities via protocol. This is how workflows extend their reach beyond built-in functionality.

**Autonomous reasoning** (agent) — multi-step decision-making with tool use. This is how workflows handle complex, open-ended problems.

Every AI workflow task falls into one of these categories. If you need to transform data, that is an infer task with a prompt like "transform this JSON into CSV format" or an exec task running a jq command. If you need to wait, that is an invoke of `nika:sleep`. If you need to read a file, that is an exec of `cat` or an invoke of `nika:read`. If you need to make a decision, that is an infer with structured output or an agent with completion conditions.

The constraint serves several purposes:

**Readability**: When you look at any task in any Nika workflow, you immediately know what kind of operation it performs. You do not need to read the implementation to understand the intent. This is not true of imperative frameworks where a function call could do anything.

**Analyzability**: Because every task is one of five types, the analyzer can reason about workflows statically. It knows that infer tasks need an LLM provider, exec tasks need security validation, fetch tasks need a URL, invoke tasks need an MCP server or builtin tool, and agent tasks need a system prompt. This enables rich diagnostics, completions, and validation.

**Security**: The five verbs create a clear security perimeter. Only exec can run commands. Only fetch can make network requests. Only invoke can call external tools. This means security policies can be expressed in terms of verbs: "this workflow is not allowed to use exec" or "this workflow can only fetch from these domains."

**Composability**: Because the verbs are orthogonal (they do different things) and uniform (they all consume bindings and produce results in the same way), they compose naturally through the DAG. Any task can depend on any other task, regardless of verb type. An agent can use results from a fetch, which can use results from an exec, which can use results from an infer.

---

## How the Five Verbs Compare to Other Approaches

To understand what is distinctive about the five-verb approach, it helps to compare it to how other frameworks structure their primitives.

**LangChain** uses chains — arbitrary sequences of Python function calls. A chain can do anything: call an LLM, parse a response, query a database, run a web search. There is no constraint on what a chain step does, which means there is no way to reason about a chain without reading its code. LangChain's flexibility is its strength and its weakness.

**LangGraph** uses state graphs with conditional edges. Nodes are Python functions that operate on a shared state dictionary. Edges are conditions that determine which node runs next. This is more structured than LangChain but still requires reading Python code to understand what each node does.

**CrewAI** uses role-based agents with tasks. Each agent has a role, a goal, and a backstory (all natural language), and tasks are assigned to agents. The framework handles orchestration, delegation, and memory. CrewAI is higher-level than LangGraph but less precise — the natural language descriptions mean behavior is non-deterministic.

**Dify** uses a visual canvas where you drag and connect nodes. Node types include LLM, Code, HTTP Request, IF/ELSE, and various integrations. The visual approach is accessible but does not version-control well and does not support complex data flow patterns.

**n8n** uses a similar visual approach with trigger-based workflows. Each node type is a specific integration (Slack, Gmail, PostgreSQL, etc.). This works well for automation but is not designed for AI-first workflows.

**Temporal** and **Prefect** use Python-based workflow definitions with durable execution (they can survive process restarts). They are designed for data engineering, not AI orchestration, and have no AI-specific primitives.

Nika sits in a unique position on this landscape. It is more constrained than LangChain or LangGraph (only five operations, declared in YAML), but this constraint enables things the imperative frameworks cannot provide: static analysis, LSP support, security auditing, and deterministic execution. It is more powerful than visual builders because YAML supports complex data flow patterns, conditional execution, and nested dependencies that do not work well in drag-and-drop interfaces. And it is more AI-native than general-purpose workflow engines because its five verbs are specifically designed for AI tasks.

---

## The "Workflow as Data" Argument

The five-verb approach is a specific instance of a broader design philosophy that Nika calls "workflow as data" (as opposed to "workflow as code").

When workflows are code (Python, JavaScript, Go), they can do anything. This is powerful but makes them opaque. You cannot analyze a Python script to determine which LLMs it calls, what APIs it hits, or what security implications it has without executing it or building a Python AST analyzer.

When workflows are data (YAML, JSON), they are declarative. They say what should happen, not how. This makes them analyzable, auditable, and toolable. You can build an LSP that understands them. You can build a TUI that visualizes them. You can build a security scanner that checks them. You can diff them in git and review them in pull requests.

The five verbs are the grammar of this data language. They define the complete vocabulary of operations, and that completeness is what makes the language analyzable. A Nika workflow is a closed system — everything it can do is declared in the YAML file, visible to anyone who reads it, and amenable to automated analysis.

---

## Building with Five Verbs: Composition Patterns

The real power of the five verbs emerges when they compose. Here are some common patterns:

**ETL Pipeline**: fetch (get data) then infer (analyze it) then exec (write to database). This replaces complex Python scripts with three YAML tasks.

**Content Generation**: infer (plan) then infer (generate sections in parallel via for_each) then infer (review and edit). Multiple infer tasks with different system prompts act as different "roles."

**Research Agent**: agent (search the web and read files) then infer (synthesize findings into a report). The agent provides open-ended exploration, and the infer provides structured output.

**Media Processing**: fetch (download images in binary mode) then invoke (nika:import to CAS) then invoke (nika:thumbnail) then invoke (nika:optimize). The media pipeline tools chain through invoke.

**MCP Integration**: invoke (get entities from NovaNet) then infer (generate content using entity data) then invoke (store results back to NovaNet). The knowledge graph provides context and receives results.

**Multi-Provider Cost Optimization**: infer with a cheap model (Groq/Llama for analysis) then infer with an expensive model (Claude for generation) then infer with a cheap model (DeepSeek for formatting). Different tasks use different providers based on the quality requirements.

Each of these patterns is expressed as a sequence of five-verb tasks connected by the DAG and the binding system. No special syntax is needed. No framework-specific constructs. Just five verbs, composed through dependencies and data flow.

---

## The Evolution of Each Verb Through Schema Versions

The five verbs have not been static since Nika's creation. They have evolved through twelve schema versions, gaining capabilities while maintaining backward compatibility within the verb structure. This evolution tells the story of what real-world usage demanded.

The infer verb started as a simple prompt-response mechanism in schema @0.1. By @0.10, it gained extended thinking (Claude's chain-of-thought reasoning with configurable thinking budgets). By @0.11, it gained structured output with JSON schema validation. By @0.12, it gained vision support with multimodal content blocks, allowing workflows to send images alongside text to vision-capable LLMs. Each addition enriched the verb without changing its fundamental nature — it still calls a language model, but it can now do so with far more sophistication.

The exec verb gained its security model progressively. Early versions had no blocklist. The 28-pattern blocklist was added after security analysis revealed that workflow authors could inadvertently create dangerous commands. NFKC normalization was added when Unicode homoglyph attacks were identified as a bypass vector. The policy enforcer was added to enable organizational security policies beyond the built-in blocklist.

The fetch verb underwent the most dramatic evolution. In early versions, it was a thin wrapper around HTTP GET. The nine extraction modes (markdown, article, text, selector, metadata, links, jsonpath, feed, llm_txt) were added across schemas @0.11 and @0.12, transforming it from a simple HTTP client into a comprehensive web content extraction system. The binary response mode with CAS integration was added to support image downloading for the media pipeline.

The invoke verb gained its builtin routing system when the media pipeline was introduced. Before media tools, invoke only called MCP servers. With the addition of builtin tools, the executor needed to check for builtins before routing to MCP — a transparent optimization that made built-in tools indistinguishable from external tools at the YAML level.

The agent verb was the last to be added (schema @0.3) and has been the most architecturally complex to evolve. Guardrails, completion conditions, cost limits, sub-agent spawning, and extended thinking support were all added in subsequent versions. The agent verb represents the most active area of development, as autonomous agent patterns continue to evolve rapidly in the broader AI landscape.

---

## The Verb Constraint in Practice: What Users Actually Build

Talking about the five verbs in the abstract is one thing. Understanding what people actually build with them reveals the practical power of the constraint.

A content marketing team uses Nika to generate blog posts in multiple languages. The workflow starts with a fetch task that retrieves market data from an API, passes it to an infer task that generates an English outline, then uses for_each with multiple infer tasks to write sections in parallel, followed by another set of infer tasks to translate each section to French, Spanish, and German. The entire workflow is a single YAML file with fifteen tasks, runs in under two minutes, and costs less than a dollar in API calls.

A DevOps engineer uses Nika to automate security scanning. The workflow uses exec to run static analysis tools, fetch to check for known vulnerabilities in dependency lists, infer to have an LLM analyze the findings and prioritize them, and invoke to post the results to a Slack channel via MCP. This replaces a 300-line Python script that was difficult to maintain and impossible for non-developers to understand.

A data scientist uses Nika to build a research pipeline. The workflow fetches RSS feeds from academic preprint servers, extracts article metadata, sends abstracts to an LLM for relevance scoring, filters the results using transform pipes, and generates a weekly digest. The agent verb is used for a deep-dive sub-workflow that reads and summarizes the most relevant papers.

In each case, the workflow author never needed to ask "which verb should I use?" The mapping is natural: if it involves an LLM, use infer. If it runs a command, use exec. If it makes an HTTP request, use fetch. If it calls a tool, use invoke. If it needs autonomous multi-turn reasoning, use agent. The five verbs are not just design categories — they are the natural language of AI workflows.

---

## The Constraint That Liberates

There is a paradox in Nika's design: the constraint of five verbs is what gives it freedom. Because the vocabulary is fixed, the system can reason about workflows. Because workflows are data, they are portable. Because tasks are independent units connected by the DAG, they parallelize automatically. Because the verbs are orthogonal, they compose without interference.

This is the same paradox that makes sonnets expressive (14 lines, specific rhyme scheme) and haiku evocative (5-7-5 syllables). Constraint forces creativity and clarity. When you cannot add a sixth verb to solve a problem, you find a composition of the existing five that works — and that composition is almost always more readable, more maintainable, and more reusable than a custom construct would have been.

---

## The Historical Context: From Makefiles to YAML Verbs

The five-verb approach has historical precedents that illuminate its design.

Makefiles, created in 1976, introduced the idea of declarative build specifications. A Makefile does not describe HOW to build software — it describes WHAT needs to be built and what each target depends on. The build system figures out the order and parallelism automatically. Nika's DAG scheduler applies this exact principle to AI workflows: you declare tasks and dependencies, and the engine figures out execution order and parallelism.

SQL, standardized in 1986, demonstrated that a small set of declarative verbs (SELECT, INSERT, UPDATE, DELETE, CREATE) could express the full range of data operations. Before SQL, database interactions required imperative programs that navigated pointer-based data structures. SQL's declarative approach was initially controversial (it was considered "too slow" and "insufficiently expressive") but ultimately won because its constraints enabled query optimization, access control, and transaction management that imperative approaches could not match. Nika's five verbs follow the same pattern: the constraint enables optimization, security, and analysis that imperative AI code cannot provide.

Kubernetes manifests, introduced in 2014, showed that complex infrastructure operations could be declared in YAML and executed by a reconciliation engine. The manifest describes the desired state, and the engine makes it happen. Nika applies this declarative-YAML pattern to AI workflows, though with a crucial difference: Nika's verbs are AI-specific (infer, agent) rather than infrastructure-specific (Pod, Service, Deployment).

Each of these precedents proved that declarative, verb-constrained systems outperform imperative systems in specific domains — not because they are more flexible, but because their constraints enable capabilities that flexibility prevents. Nika is placing the same bet on AI workflows: that a small, fixed vocabulary of operations, declared in data rather than code, will prove more powerful than any Python framework precisely because of what it cannot do.

The five verbs of Nika are not a limitation. They are a language. And like any good language, their power comes from what they can express when combined, not from how many words they contain.
