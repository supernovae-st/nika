# Hacker News Launch Kit -- Nika

> Show HN post strategy, anticipated comments, and response playbook.
> HN values: technical depth, intellectual honesty, no marketing fluff.

---

## 1. Title Options

### Option A (Recommended)
```
Show HN: Nika -- A 451K-line Rust engine for declarative AI workflows (YAML, 5 verbs, 9 providers)
```

### Option B (Concise)
```
Show HN: Nika -- Orchestrate AI workflows with 5 YAML verbs (Rust, AGPL-3.0)
```

### Option C (Technical hook)
```
Show HN: I wrote 451K lines of Rust to replace AI workflow SDKs with 5 YAML verbs (9 providers, AGPL)
```

---

## 2. Post Body

```
Nika is a semantic YAML workflow engine for AI tasks. You declare what you
want using 5 verbs, and it handles DAG scheduling, parallel execution,
multi-provider routing, and structured output validation.

The 5 verbs:

  - infer: LLM generation (9 providers, structured output, vision)
  - exec: shell commands (28-pattern security blocklist)
  - fetch: HTTP requests (9 extract modes including article, markdown, RSS)
  - invoke: MCP tool calls (24 built-in media tools + any MCP server)
  - agent: multi-turn agentic loops (guardrails, cost limits, tool calling)

Example -- a pipeline that fetches data, analyzes it with an LLM, and posts results:

  schema: nika/workflow@0.12
  tasks:
    - id: data
      fetch:
        url: https://api.example.com/metrics
        extract: jsonpath
        selector: "$.results[*]"
    - id: analyze
      with: { metrics: $data }
      infer:
        model: claude-sonnet-4-20250514
        prompt: "Analyze these metrics: {{with.metrics}}"
        structured:
          schema:
            type: object
            properties:
              insights: { type: array, items: { type: string } }
    - id: notify
      with: { report: $analyze }
      fetch:
        url: https://hooks.slack.com/XXX
        method: POST
        json: { text: "{{with.report}}" }

Dependencies are inferred from `with:` bindings -- no explicit ordering needed.
Tasks without dependencies run in parallel automatically.

Technical details:

  - 451K lines of Rust across 10 workspace crates
  - 8,300+ tests, zero clippy warnings, zero unsafe
  - Single binary (cargo install nika), no runtime deps
  - 2-phase AST: YAML -> Raw AST (with source spans) -> Analyzed AST (validated, interned)
  - DAG scheduler with cycle detection, topological sort, parallel execution via tokio
  - Content-addressable storage for media (24 built-in tools: thumbnail, chart, PDF, C2PA)
  - MCP client (rmcp v0.16) with retry, reconnect, and builtin tool routing
  - TUI with 3 views (92K lines of ratatui): live DAG, streaming output, cost tracking
  - LSP server for VS Code and Neovim
  - Interactive course: 12 levels, 44 exercises (nika init --course)

The engine is embeddable -- nika-engine is a library crate that can be
integrated into other Rust projects without the CLI or TUI.

Why YAML instead of Python/TypeScript:

  1. Workflows ARE documentation. No hidden state, no abstractions to learn.
  2. Version-controllable. Put workflows in PRs, diff them, review them.
  3. Constrained expressiveness. 5 verbs prevent the "framework spaghetti" problem.
  4. Portable. No runtime dependency on Python/Node.js/Docker.
  5. Generated. Workflows can be generated programmatically (including by LLMs).

Why Rust:

  1. Single binary. cargo install nika -- done.
  2. Performance. SIMD-accelerated media tools (Lanczos3 resize, oxipng).
  3. Correctness. The type system catches integration errors at compile time.
  4. Concurrency. tokio async runtime with JoinSet for parallel task execution.
  5. No GC pauses. Predictable latency for streaming LLM responses.

The project is AGPL-3.0 licensed. Using Nika as a CLI tool has no restrictions.
The AGPL only applies if you modify the source and offer it as a hosted service.

This is a companion to NovaNet, a knowledge graph that Nika connects to via MCP.
Together they form a "brain + body" architecture: NovaNet knows things (entities,
relationships, knowledge atoms), Nika does things (workflows, LLM calls, pipelines).

Source: https://github.com/supernovae-st/nika
Install: cargo install nika
Learn: nika init --course
```

---

## 3. Anticipated Comments and Responses

### "Why not just use LangChain?"

**Response:**
```
Different design philosophy. LangChain is an imperative Python SDK -- you write
Python code that calls LLMs. Nika is a declarative YAML engine -- you describe
what you want and it figures out execution.

The tradeoff is explicit: LangChain gives you maximum flexibility (write any
Python), Nika gives you maximum clarity (read any workflow in 30 seconds).

For teams where multiple people touch AI workflows, the "workflow IS the
documentation" property matters a lot. You can review a Nika workflow in a PR
without knowing Rust or any SDK.
```

### "451K lines seems like a lot for a workflow engine"

**Response:**
```
Fair point. The breakdown:

  nika-tui: 92K (Terminal UI -- 3 views with live DAG, streaming, widgets)
  nika-engine: 134K (core engine + 24 media tools + init/course system)
  nika-core: 23K (AST types, parser, analyzer -- zero I/O)
  nika-mcp: 9K (MCP client)
  nika-lsp-core: 9K (Language Server Protocol)
  nika-cli: 8K (CLI subcommands)
  rest: ~10K (event, media, lsp binary)

The TUI alone is 92K lines -- it's a full ratatui application with 42 widgets.
If you're using Nika headless (nika run), you only need ~180K lines. Still a lot,
but that includes 24 media tools, a 44-exercise course generator, 200+ showcase
workflows, and comprehensive error handling (319 error codes).

The test count (8,100+) accounts for a significant portion of the total.
```

### "YAML is terrible for programming"

**Response:**
```
I agree that YAML is terrible for general programming. That's actually the
point -- Nika constrains you to 5 verbs on purpose.

The failure mode of LangChain/LangGraph is "framework spaghetti" -- 200 lines
of Python with callbacks, chains, memory adapters, and output parsers that no
one can read 6 months later.

Nika's failure mode is "you can't do that in YAML." When you hit that wall,
you use exec: to call a shell script, invoke: to call an MCP tool, or agent:
to let an LLM figure it out with tool calling. The workflow stays readable.

YAML is also uniquely LLM-friendly. An LLM can generate valid .nika.yaml
workflows because the format is simple and well-constrained. That matters
for autonomous orchestration.
```

### "Why AGPL?"

**Response:**
```
AGPL protects the commons. Without it, any cloud provider can fork Nika,
add proprietary features, and sell it as "Nika Cloud" without contributing
anything back.

For individual developers and companies using Nika as a tool (nika run,
nika ui), the AGPL has zero practical impact. You can use it for any
purpose.

The AGPL only matters if you modify Nika's source code and offer it as a
hosted service. In that case, you share your modifications -- which is fair,
since you built on 451K lines of community code.

We considered MIT/Apache but decided the risk of cloud exploitation was too
high. The AGPL aligns our incentives with the community.
```

### "How does this compare to Temporal/Prefect?"

**Response:**
```
Different layers. Temporal and Prefect are workflow orchestration platforms --
they handle durability, retries, state management for general-purpose workflows.

Nika is purpose-built for AI workflows. It has native LLM support (22
providers), agent loops with guardrails, structured output validation,
MCP integration, and a media pipeline. You wouldn't use Temporal to call
Claude with structured output and pipe the result through a vision model.

They can complement each other: Nika for the AI-specific orchestration,
Temporal/Prefect for the infrastructure-level durability.
```

### "No Python support?"

**Response:**
```
No. Nika is Rust all the way down, and workflows are YAML.

The exec: verb lets you call Python scripts if needed:

  - id: analyze
    exec:
      command: "python3 analyze.py --input {{with.data}}"
      shell: true

But the engine itself is Rust. This is intentional -- a single binary with
no Python/Node.js runtime dependency is the simplest deployment story.
```

### "What about error handling?"

**Response:**
```
Nika has comprehensive error codes (NIKA-XXX format) covering every failure
mode: schema validation, DAG cycles, provider errors, template resolution,
security violations, media pipeline failures, MCP connection issues, etc.

Each error includes the source span (file, line, column) so you get
actionable diagnostics. The LSP server shows these inline in your editor.

For runtime errors, you get event traces (39 event types) in NDJSON format
for observability. The TUI shows errors in real-time with stack context.
```

### "Is this production-ready?"

**Response:**
```
v0.49.0 is the current version. It's used in production by SuperNovae
Studio for QR Code AI (https://qrcode-ai.com).

That said, Nika stays at 0.x.x permanently -- not because it's unstable,
but because it's evolving fast and we don't want semver to slow us down.
The schema version (nika/workflow@0.12) is the stability contract.
```

### "Why not use JSON Schema for the workflow format?"

**Response:**
```
The workflow format IS validated against a JSON Schema
(nika-workflow.schema.json). YAML is just the surface syntax because it's
more readable for humans than JSON. The schema ensures workflows are valid
at parse time, before any execution happens.

The 2-phase AST (Raw -> Analyzed) catches everything from missing fields
to DAG cycles to unresolved template variables before a single LLM call
is made.
```

---

## 4. Technical Differentiators to Highlight

### On HN, lead with these:

1. **2-phase AST with source spans.** Not just "parse YAML and run it." The engine builds a proper AST with span tracking, validates it semantically (cycle detection, binding resolution, provider validation), then lowers it to runtime types. This is compiler engineering, not string templating.

2. **DAG scheduling with automatic parallelism.** Tasks that don't depend on each other run in parallel automatically via tokio JoinSet. No explicit `parallel:` blocks needed -- the engine infers the execution graph from `with:` bindings.

3. **Embeddable engine.** `nika-engine` is a library crate. You can integrate the workflow execution engine into your own Rust application without the CLI or TUI. This is the path to Nika-as-a-library.

4. **Content-addressable storage for media.** All media operations (thumbnail, convert, import) go through a CAS layer. Files are addressed by content hash, never by path. This prevents path traversal attacks and enables deduplication.

5. **Zero unsafe, zero clippy warnings.** 451K lines of safe Rust. The entire workspace passes `clippy -- -D warnings`.

6. **319 structured error codes.** Not just "something went wrong." Every error has a code (NIKA-000 through NIKA-319), a category, and source span information. Debugging a Nika workflow should never require reading the engine source.

7. **The course system.** `nika init --course` generates 44 exercises across 12 levels (Liberation constellation). Each exercise is a partially-complete `.nika.yaml` file with `# TODO` markers. Validation checks that the completed workflow produces the expected output. This isn't documentation -- it's executable teaching.

---

## 5. What NOT to Say on HN

| Avoid | Why | Instead |
|-------|-----|---------|
| "Revolutionary" / "Game-changing" | HN hates superlatives | State what it does, let readers judge |
| "10x productivity" | Unverifiable claims | Show a before/after code comparison |
| "Better than LangChain" | Tribal / competitive | "Different approach: declarative vs imperative" |
| "AI agent framework" | Overloaded buzzword | "Workflow engine with agent loops" |
| "Enterprise-ready" | MBA-speak | "Used in production for X" |
| "Democratizing AI" | Empty buzzword | "Single binary, 5 verbs, no SDK" |
| Emoji in the post | HN culture | Plain text only |
| Marketing copy tone | Instant downvotes | Technical writing tone |
| Talking about funding | Not relevant | Talk about technical decisions |
| Dismissing criticism | Arrogant | "Fair point, here's the tradeoff..." |

### Tone Calibration

HN readers value:
- **Intellectual honesty.** Admit limitations. "YAML has real downsides: no conditionals, no loops, verbose for complex logic."
- **Technical depth.** Explain WHY decisions were made, not just WHAT was built.
- **Comparative clarity.** Don't bash competitors -- explain where Nika fits and where it doesn't.
- **Responsiveness.** Answer every genuine question. HN rewards engaged makers.
- **Concreteness.** Code examples > adjectives. Benchmarks > "fast."

---

## 6. Response Templates

### For "Why should I care?"

```
If you're building AI pipelines and want:
- Workflows that are readable by non-engineers (YAML, not Python)
- Multi-model routing without rewriting your pipeline
- A single binary with no Docker/Python/Node dependency
- 24 media tools built into the engine (no external services)
- An interactive course instead of docs you'll never read

Then Nika might be worth 30 minutes of your time. If you're happy with
LangChain/Python, there's no reason to switch.
```

### For "Cool but I'd rather use Python"

```
Totally valid. Python is the lingua franca of AI and has the largest
ecosystem. Nika is for people who find that:

1. Their AI workflows are 80% configuration and 20% logic
2. They want non-ML-engineers to read/review their pipelines
3. They want a single binary without Python version/venv management
4. They want version-controllable, diffable workflow definitions

If your workflows are 80% custom Python logic, LangChain is the right tool.
```

### For constructive technical criticism

```
That's a great point. [Acknowledge the specific issue]. We've considered
[alternative approach] but chose [current approach] because [concrete reason].
The tradeoff is [honest assessment of downsides]. Happy to discuss further
-- this is the kind of feedback that makes the project better.
```

---

## 7. Timing and Cross-Posting

### Optimal HN Post Time
- **Tuesday or Wednesday, 8-10 AM ET** -- highest visibility window
- Avoid weekends (lower traffic) and Mondays (crowded)

### Cross-Posting Strategy
1. Post on HN first (Show HN)
2. Wait 2-4 hours for traction
3. If it gains momentum, share on X/Twitter with the HN link
4. Post on r/rust and r/programming later in the day (don't spam)
5. LinkedIn post the following day with HN discussion link

### Engagement Protocol
- Check HN comments every 15-30 minutes for the first 4 hours
- Respond to every genuine technical question
- Upvote constructive criticism (yes, even negative)
- Do NOT ask friends to upvote (HN penalizes vote rings)
- Do NOT post from throwaway accounts

---

*Prepared for SuperNovae Studio. Last updated 2026-03-23.*
