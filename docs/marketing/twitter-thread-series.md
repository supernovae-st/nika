# Twitter/X Thread Series -- Nika

> 5 thread series for the Nika launch campaign.
> Each tweet is max 280 chars. Threads are designed for sequential reading with standalone value per tweet.

---

## Thread 1: Launch Announcement (15 tweets)

### Tweet 1
```
Today we're open-sourcing Nika -- a 451K-line Rust engine that turns YAML into AI workflows.

5 verbs. 22 providers. 24 media tools. One binary.

No Python. No Docker. No SDK.

Just YAML.

Thread on why we built it and what it does.
```

### Tweet 2
```
The problem: AI workflow tools force a choice.

Visual builders (Dify, n8n): great for demos, break when you need real logic.

Python SDKs (LangChain, LangGraph): powerful but 200+ lines for a simple pipeline.

We wanted a third option.
```

### Tweet 3
```
Nika's answer: 5 semantic verbs.

infer: -- call any LLM
exec: -- run shell commands
fetch: -- make HTTP requests
invoke: -- call MCP tools
agent: -- run multi-turn loops

Every AI workflow task maps to exactly one verb.
```

### Tweet 4
```
Here's a complete AI pipeline in 15 lines:

schema: nika/workflow@0.12
tasks:
  - id: data
    fetch:
      url: https://api.example.com/metrics
      extract: jsonpath
      selector: "$.results"
  - id: analyze
    with: { data: $data }
    infer: "Summarize: {{with.data}}"
  - id: save
    with: { report: $analyze }
    exec:
      command: 'echo "{{with.report}}" > report.md'
```

### Tweet 5
```
Dependencies? Automatic.

Nika builds a DAG from your `with:` bindings. Tasks without dependencies run in parallel. No explicit ordering needed.

Write the what. Nika figures out the when.
```

### Tweet 6
```
22 LLM providers, one syntax:

Claude, GPT-4o, Gemini, Mistral, Groq, DeepSeek, xAI, Perplexity + local GGUF models.

Switch models by changing one line. Mix providers in the same workflow. Route cheap tasks to fast models, complex tasks to powerful ones.
```

### Tweet 7
```
Structured output built in:

structured:
  schema:
    type: object
    properties:
      trends:
        type: array
        items: { type: string }
    required: [trends]

JSON Schema validation on every LLM response. No post-processing, no prayer-based parsing.
```

### Tweet 8
```
24 built-in media tools:

nika:thumbnail -- SIMD resize
nika:convert -- format conversion
nika:chart -- charts from JSON
nika:pdf_extract -- PDF to text
nika:provenance -- C2PA signing
nika:qr_validate -- QR scanning

All via invoke: nika:* -- zero external dependencies.
```

### Tweet 9
```
The learning path that teaches itself:

nika init --course

12 levels. 44 exercises. Progressive hints (3 tiers). Auto-validation on file save. A constellation progress map.

From first workflow to multi-agent orchestration. The course IS the documentation.
```

### Tweet 10
```
MCP-native from day one.

Nika speaks Model Context Protocol natively. Connect to any MCP server with invoke:.

Paired with NovaNet (our knowledge graph), you get entity-aware AI workflows with semantic context.

Brain + Body architecture. No Cypher in Nika. Pure MCP.
```

### Tweet 11
```
Why Rust?

- Single binary. cargo install nika. Done.
- 7,784+ tests run in seconds
- Zero clippy warnings across 451K lines
- SIMD-accelerated media processing
- tokio async for parallel DAG execution
- No GC pauses during LLM streaming
```

### Tweet 12
```
The TUI is its own project: 92K lines of ratatui.

3 views:
- Studio: live DAG visualization
- Command: streaming output
- Control: system overview

It's like watching your AI pipeline execute in real-time. Cost tracking, token counts, parallel task progress.
```

### Tweet 13
```
Security is not an afterthought:

- 28-pattern command blocklist for exec:
- Content-addressable storage for media (no path traversal)
- Environment variable validation
- Policy enforcement engine
- SVG sanitization before parsing
- File size pre-checks (50MB limit)
```

### Tweet 14
```
AGPL-3.0 licensed.

Use Nika freely for anything. The AGPL only matters if you modify the source and offer it as a service.

Open source means open source. No "community edition" tricks. No features locked behind a paywall. All 451K lines are yours.
```

### Tweet 15
```
Get started:

cargo install nika
nika init --minimal   # 5 example workflows
nika init --course    # 44 exercises
nika showcase list    # 200+ ready workflows

GitHub: https://github.com/supernovae-st/nika

Built by @SuperNovae_st

What would you build with 5 verbs?
```

---

## Thread 2: "Why We Chose Rust" (10 tweets)

### Tweet 1
```
We wrote 451K lines of Rust for an AI workflow engine.

Not Python. Not TypeScript. Not Go.

Rust.

Here's why -- and what we learned.
```

### Tweet 2
```
Reason 1: Single binary deployment.

cargo install nika

One command. One binary. No Python venv, no node_modules, no Docker. Works on macOS and Linux. Starts in milliseconds.

This is the deployment story every developer tool should have.
```

### Tweet 3
```
Reason 2: The type system catches bugs at compile time.

Our AST pipeline: Raw YAML -> Analyzed AST -> Runtime types. Each phase is a separate type. You literally cannot pass unvalidated YAML to the executor -- the compiler won't let you.

2-phase AST > hope-based validation.
```

### Tweet 4
```
Reason 3: SIMD-accelerated media processing.

24 built-in media tools. Lanczos3 image resampling. oxipng lossless optimization. resvg SVG rasterization.

Try doing SIMD resize in Python without calling C. In Rust, it's just a crate.
```

### Tweet 5
```
Reason 4: Fearless concurrency.

DAG scheduling means parallel task execution. tokio JoinSet + CancellationToken + DashMap.

In Python: GIL + threading hacks + asyncio.
In Rust: the compiler proves your concurrent code is correct.

7,784 tests. Zero data races. Not by discipline -- by type system.
```

### Tweet 6
```
Reason 5: No garbage collection.

Streaming LLM responses token-by-token. The TUI updates at 60fps. Media tools process megabyte images.

GC pauses would be visible. With Rust's ownership model, memory is freed deterministically. No pauses. Ever.
```

### Tweet 7
```
What surprised us:

The TUI is 92K lines. ratatui is phenomenal -- Elm-style architecture in Rust. State management, widgets, layout.

Writing a TUI in Python would have been "easier" but the result would be slower and less reliable.

ratatui + crossterm = terminal UI that feels native.
```

### Tweet 8
```
What was hard:

Async Rust. Lifetimes in closures. The borrow checker fighting our DAG mutability patterns.

The learning curve is real. We won't pretend otherwise.

But: every fight with the compiler was a bug we didn't ship to users.
```

### Tweet 9
```
The ecosystem that made it possible:

- rig-core: multi-provider LLM abstraction
- rmcp: MCP client library
- ratatui: terminal UI framework
- mistral.rs: local model inference
- marked_yaml: YAML with source spans
- insta: snapshot testing

Rust's AI/ML ecosystem is small but growing fast.
```

### Tweet 10
```
Would we choose Rust again?

Absolutely. For a developer tool that ships as a binary, processes media, streams LLM responses, and runs concurrent DAG pipelines -- Rust is the right language.

The 20% tax on development speed pays back 10x in reliability and performance.

https://github.com/supernovae-st/nika
```

---

## Thread 3: "5 Verbs Is All You Need" (10 tweets)

### Tweet 1
```
Every AI workflow you've ever built uses 5 operations:

1. Call an LLM
2. Run a command
3. Make an HTTP request
4. Call a tool
5. Run a multi-turn loop

That's it. 5 verbs. Everything else is composition.

Let me show you.
```

### Tweet 2
```
Verb 1: infer: -- Call any LLM

- id: summarize
  infer:
    model: claude-sonnet-4-20250514
    prompt: "Summarize: {{with.data}}"
    structured:
      schema:
        type: object
        properties:
          summary: { type: string }

22 providers. Structured output. Vision. Extended thinking.
```

### Tweet 3
```
Verb 2: exec: -- Run a command

- id: build
  exec:
    command: "cargo test --lib"
    timeout: 60

Shell commands with a 28-pattern security blocklist. Timeout enforcement. shlex parsing (no shell injection by default).
```

### Tweet 4
```
Verb 3: fetch: -- HTTP requests

- id: scrape
  fetch:
    url: https://news.ycombinator.com
    extract: article

9 extract modes: markdown, article, text, selector, metadata, links, jsonpath, feed, llm_txt.

Web scraping without a browser.
```

### Tweet 5
```
Verb 4: invoke: -- Call MCP tools

- id: resize
  invoke:
    tool: nika:thumbnail
    params:
      hash: "{{with.image}}"
      width: 800

24 built-in tools + any external MCP server. Knowledge graphs, databases, APIs -- all through one verb.
```

### Tweet 6
```
Verb 5: agent: -- Multi-turn loops

- id: coder
  agent:
    prompt: "Fix the failing tests"
    tools: [nika:read, nika:edit, nika:grep]
    max_turns: 10
    guardrails:
      - type: length
        max: 50000
    limits:
      max_cost_usd: 0.50

Agents with guardrails and cost limits.
```

### Tweet 7
```
Composition: the 5 verbs combine with `with:` bindings.

- id: data
  fetch: { url: "https://api.example.com" }
- id: analyze
  with: { raw: $data }
  infer: "Analyze: {{with.raw}}"
- id: save
  with: { report: $analyze }
  exec:
    command: 'echo "{{with.report}}" > out.md'

Dependencies are automatic. Parallel execution is automatic.
```

### Tweet 8
```
What about conditionals? Loops? Complex logic?

You don't need them.

Conditionals -> Use agent: (the LLM decides)
Loops -> Use agent: with max_turns
Complex logic -> Use exec: to call a script
Parallel fan-out -> Dependencies handle it automatically

5 verbs + DAG scheduling > Turing completeness
```

### Tweet 9
```
The constraint IS the feature.

LangChain lets you do anything -> framework spaghetti
Nika lets you do 5 things -> readable workflows

When your coworker opens your workflow, they understand it in 30 seconds. No SDK knowledge needed. No Python debugging. Just YAML.

Constraints breed clarity.
```

### Tweet 10
```
Try the 5 verbs:

cargo install nika
nika init --minimal

You get 5 example workflows -- one per verb.

Or start the interactive course:
nika init --course

44 exercises teaching every verb in depth.

https://github.com/supernovae-st/nika

What would you build with 5 verbs?
```

---

## Thread 4: "Open Source Is Liberation" (10 tweets)

### Tweet 1
```
We licensed Nika under AGPL-3.0.

Not MIT. Not Apache. Not "open core."

AGPL.

This is a deliberate choice about what we believe open source should be. Here's why.
```

### Tweet 2
```
The pattern we keep seeing:

1. Small team builds amazing open source tool (MIT licensed)
2. Cloud provider forks it, adds features, hosts it
3. Cloud provider captures 99% of the value
4. Original team can't compete with their own creation
5. Project dies or goes "open core" (read: closed core)

This is broken.
```

### Tweet 3
```
Examples:

- Elasticsearch -> AWS OpenSearch (Elastic switched to SSPL)
- Redis -> AWS ElastiCache (Redis switched to dual license)
- MongoDB -> DocumentDB (MongoDB created SSPL)
- Terraform -> OpenTofu (HashiCorp switched to BSL)

The pattern is so common it has a name: "strip-mining open source."
```

### Tweet 4
```
The AGPL prevents this.

If you use Nika as a tool: no restrictions. Use it for anything -- personal, commercial, whatever.

If you modify Nika and offer it as a service: share your modifications. That's it.

You benefit from 451K lines of community code. The community benefits from your improvements.
```

### Tweet 5
```
"But AGPL scares companies away!"

Good. It scares away companies that want to take without giving back.

It welcomes companies that want to build ON Nika (no AGPL obligations -- your workflows are yours) and companies that want to build WITH Nika (contribute back, everyone wins).
```

### Tweet 6
```
The name Nika comes from the Greek goddess of victory.

In the Nika Riots of 532 AD, the crowd chanted "Nika! Nika!" -- "Win! Win!"

It was a populist uprising. A demand for change.

Open source is our Nika moment. Technology should serve everyone, not just the companies that can afford to close it.
```

### Tweet 7
```
Our butterfly symbol represents metamorphosis.

A creature that breaks free from confinement and gains the ability to fly.

That's what good tools do: they free you from vendor lock-in, from proprietary formats, from cloud dependency.

Nika workflows are YAML files. They belong to you.
```

### Tweet 8
```
Practical implications:

Your .nika.yaml workflows: YOURS. No license concerns.
Nika CLI as a tool: FREE. Use it however you want.
Nika as embedded library: AGPL applies to your wrapper.
Modified Nika as SaaS: Share modifications (AGPL).

The engine is open. Your work on top is yours.
```

### Tweet 9
```
We could have chosen MIT for more GitHub stars.

We could have chosen "open core" for venture funding.

We chose AGPL because we want Nika to exist in 10 years, still open, still community-owned, still free.

The license IS the long-term strategy.
```

### Tweet 10
```
If you believe open source should stay open:

Star: https://github.com/supernovae-st/nika
Try: cargo install nika
Learn: nika init --course
Build: nika init --minimal

451K lines of Rust. AGPL-3.0. Zero compromises.

Victory belongs to everyone.
```

---

## Thread 5: "What You Can Build" Showcase (12 tweets)

### Tweet 1
```
"What can you actually build with a YAML workflow engine?"

Here are 10 real workflows you can build with Nika, each under 30 lines of YAML.

All of these are in our showcase: nika showcase list
```

### Tweet 2
```
1. Content Pipeline

Fetch a webpage -> extract the article -> summarize with Claude -> translate with Groq -> post to Slack.

4 tasks, 3 verbs (fetch, infer, fetch), 2 providers, ~20 lines.

Cost: ~$0.005 per run.
```

### Tweet 3
```
2. Image Processing Pipeline

Import image -> generate thumbnail (800px) -> extract metadata -> create thumbhash placeholder -> optimize PNG.

All built-in tools. Zero external services.

invoke: nika:import -> nika:thumbnail -> nika:metadata -> nika:thumbhash -> nika:optimize
```

### Tweet 4
```
3. Code Review Agent

Read PR diff -> analyze with Claude (extended thinking) -> check for security issues -> post review comment via API.

The agent: verb gives the LLM access to nika:read, nika:grep, and nika:glob for deep code analysis. Guardrails prevent hallucinated file paths.
```

### Tweet 5
```
4. RSS Monitor + Digest

Fetch RSS feed (extract: feed) -> filter for keywords -> summarize with DeepSeek ($0.001) -> format as HTML -> send via SendGrid.

Schedule with cron. Cost: ~$0.01/day for 10 feeds.

The fetch: verb's extract: feed mode handles RSS/Atom/JSON Feed natively.
```

### Tweet 6
```
5. Multi-Model Research Pipeline

Research with Groq (fast, cheap) -> deep analysis with Claude (quality) -> fact-check with Gemini (grounding) -> compile report.

Multi-model = each task uses the best provider for its job. One workflow, three providers.

Cost savings vs single-model: ~60%.
```

### Tweet 7
```
6. QR Code Validation Pipeline

Import QR image -> decode + scan score (nika:qr_validate) -> check branding guidelines with vision LLM -> generate report.

Combines media tools with LLM vision. The scan score (0-100) predicts real-world readability.
```

### Tweet 8
```
7. Documentation Generator

Glob source files -> read each file -> extract functions/types with agent -> generate Markdown docs -> write to docs/ directory.

The agent uses nika:read, nika:glob, nika:write as tools. Structured output ensures consistent doc format.
```

### Tweet 9
```
8. Web Scraper + Knowledge Base

Fetch page (extract: markdown) -> extract metadata (extract: metadata) -> extract links (extract: links) -> store via MCP.

9 extract modes means you can scrape anything without a headless browser. Links are auto-classified (internal/external, nav/content/footer).
```

### Tweet 10
```
9. C2PA Content Provenance

Import image -> sign with C2PA credentials (nika:provenance) -> verify manifest (nika:verify) -> generate EU AI Act compliance report.

Content authenticity built into the workflow. No external signing services.
```

### Tweet 11
```
10. Learning Course Generator

nika init --course generates:
- 12 levels with progressive difficulty
- 44 exercises with TODO markers
- Auto-validation (nika course check)
- Progressive hints (3 tiers)
- Constellation progress map

The course system itself is a Nika feature -- meta!
```

### Tweet 12
```
All of these are in the showcase:

nika showcase list              # Browse 200+ workflows
nika showcase extract <name>    # Extract to your project

Start building:
cargo install nika
nika init --minimal

What would YOU build with 5 verbs?

https://github.com/supernovae-st/nika
```

---

## Posting Strategy

### Schedule

| Day | Thread | Platform |
|-----|--------|----------|
| Launch Day (Monday) | Thread 1: Launch Announcement | X/Twitter |
| Tuesday | Thread 3: 5 Verbs | X/Twitter |
| Wednesday | Thread 2: Why Rust | X/Twitter |
| Thursday | Thread 5: What You Can Build | X/Twitter |
| Friday | Thread 4: Open Source Liberation | X/Twitter |

### Tips

- Post thread at 9 AM ET for maximum visibility
- Pin Thread 1 to profile during launch week
- Quote-tweet individual tweets with additional context
- Respond to every reply within 2 hours
- Cross-post Thread 2 to r/rust, Thread 4 to r/opensource
- Use alt text on all images/screenshots

---

*Prepared for SuperNovae Studio. Last updated 2026-03-23.*
