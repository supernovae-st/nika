# Product Hunt Launch Kit -- Nika

> Complete Product Hunt launch preparation for Nika v0.42.
> Every asset, copy variant, and timing detail needed for a successful launch day.

---

## 1. Core Copy

### Tagline (60 chars max)

**Primary:** `AI workflows in YAML. 5 verbs. Infinite power.` (50 chars)

**Alternatives:**
- `The Rust engine that turns YAML into AI pipelines` (51 chars)
- `5 verbs to orchestrate any AI workflow` (39 chars)
- `Declare your AI workflows. Nika runs them.` (44 chars)

### Short Description (260 chars max)

Nika is a semantic YAML workflow engine for AI tasks, written in 451K lines of Rust. 5 verbs (infer, exec, fetch, invoke, agent) compose into DAG-scheduled pipelines with 22 LLM providers, 24 media tools, a built-in learning course, and full MCP integration.

### Detailed Description

**Nika is the workflow engine for the AI era.**

Most AI tools force you into one of two extremes: drag-and-drop builders that break the moment you need real logic, or Python frameworks that demand you learn a new SDK just to chain two LLM calls together.

Nika takes a different path. You write YAML. Five semantic verbs -- `infer:`, `exec:`, `fetch:`, `invoke:`, `agent:` -- are all you need to build anything from a simple summarizer to a multi-model content pipeline that costs 60% less than single-model approaches.

**Why developers choose Nika:**

- **Declarative, not imperative.** Your workflow IS the documentation. No hidden state, no callback hell, no "what does this lambda do again?"
- **Written in Rust.** 451K lines of zero-unsafe, zero-clippy-warning Rust. The engine compiles to a single binary with no runtime dependencies. It starts in milliseconds and processes workflows with the efficiency only a systems language can provide.
- **22 LLM providers, one syntax.** Claude, GPT-4o, Gemini, Mistral, Groq, DeepSeek, xAI, Perplexity -- plus local GGUF models via mistral.rs. Switch providers by changing one line.
- **MCP-native.** First-class Model Context Protocol integration means Nika connects to any MCP server -- including its companion knowledge graph, NovaNet.
- **24 built-in media tools.** Image processing, PDF extraction, chart generation, C2PA content credentials, QR code validation -- all accessible via `invoke: nika:*` with zero external dependencies.
- **A real learning path.** `nika init --course` generates a 12-level, 44-exercise interactive course that teaches you everything from your first `infer:` to building multi-agent orchestration systems. Progressive hints, auto-validation, constellation progress maps.
- **AGPL-3.0 licensed.** Open source means open source. The AGPL ensures Nika stays free for the community, forever. No cloud provider can take your work and close the door behind them.

**The 5 verbs paradigm:**

```yaml
tasks:
  - id: research
    fetch:
      url: https://api.example.com/data
      extract: markdown

  - id: analyze
    with: { data: $research }
    infer:
      model: claude-sonnet-4-20250514
      prompt: "Analyze this data: {{with.data}}"
      structured:
        schema:
          type: object
          properties:
            findings: { type: array, items: { type: string } }

  - id: notify
    with: { report: $analyze }
    fetch:
      url: https://hooks.slack.com/services/XXX
      method: POST
      json:
        text: "{{with.report}}"
```

Three tasks. Three verbs. A complete pipeline from data collection through AI analysis to notification -- with structured output, automatic DAG scheduling, and zero boilerplate.

**Built for real production:**
- DAG-scheduled parallel execution with dependency resolution
- Structured output with JSON Schema validation
- Agent guardrails (length, regex, custom) with automatic retry
- Security: 28-pattern command blocklist, env validation, path traversal protection
- Event sourcing: 39 event types with NDJSON trace output
- Terminal UI with 3 views (Studio, Command, Control) -- 92K lines of ratatui
- LSP for IDE integration (VS Code, Neovim, etc.)
- 8,100+ tests, zero clippy warnings

---

## 2. Five Key Features

### Feature 1: Five Semantic Verbs
**Title:** 5 Verbs Is All You Need
**Description:** Every AI workflow task maps to one of five verbs: `infer:` for LLM generation, `exec:` for shell commands, `fetch:` for HTTP requests, `invoke:` for MCP tool calls, and `agent:` for multi-turn agentic loops. No SDKs, no abstractions -- just YAML and intent.

### Feature 2: 22 LLM Providers, One Syntax
**Title:** Every Model, One YAML
**Description:** Claude, GPT-4o, Gemini, Mistral, Groq, DeepSeek, xAI, Perplexity, and local GGUF models through mistral.rs. Multi-model workflows route different tasks to different providers for optimal cost and quality. Change `provider: claude` to `provider: groq` -- everything else stays the same.

### Feature 3: Built-in Media Pipeline
**Title:** 24 Media Tools, Zero Dependencies
**Description:** Import images into content-addressable storage, generate thumbnails with SIMD acceleration, extract PDF text, create charts from JSON, sign content with C2PA credentials, validate QR codes. All via `invoke: nika:thumbnail` -- no external services, no API keys, no Docker containers.

### Feature 4: Interactive Learning Course
**Title:** Learn by Doing: 44 Exercises, 12 Levels
**Description:** `nika init --course` generates a complete learning journey -- from first workflow to multi-agent orchestration. Each level unlocks new capabilities. Progressive hints (3 tiers), auto-validation on file save, constellation progress maps. The course IS the documentation.

### Feature 5: MCP-Native Architecture
**Title:** First-Class MCP Integration
**Description:** Nika speaks Model Context Protocol natively. Connect to any MCP server -- databases, APIs, knowledge graphs -- with `invoke:`. The companion project NovaNet provides a knowledge graph accessible via MCP, enabling entity-linked workflows with semantic context.

---

## 3. Maker Comment

**Posted by:** Thibaut / @ThibautMelen

> Hey Product Hunt! I'm Thibaut, the creator of Nika.
>
> I built Nika because I was frustrated with the state of AI workflow tools. On one side, you have visual builders like Dify that look great in demos but can't handle real logic. On the other, you have Python frameworks like LangChain that require learning an entire SDK to do what should be a YAML config.
>
> Nika's insight is simple: **5 verbs are enough**. `infer:` generates text. `exec:` runs commands. `fetch:` makes HTTP requests. `invoke:` calls MCP tools. `agent:` runs multi-turn loops. Compose them in YAML with DAG dependencies, and you can build anything.
>
> Why Rust? Because AI workflows should start in milliseconds, not seconds. Because a single binary with zero dependencies is the best deployment story. Because 8,100 tests should run in under 5 seconds. And because when you're processing media (24 built-in tools for images, PDFs, charts, QR codes), you want SIMD-accelerated Lanczos3 resampling, not Python's PIL.
>
> The part I'm most proud of is the course system. Run `nika init --course` and you get 12 levels of progressive exercises -- from "what is a workflow" to building multi-agent orchestration systems. Every level has progressive hints, auto-validation, and a constellation progress map. We believe the best documentation teaches you to fish, not just shows you the fish.
>
> Nika is AGPL-3.0 licensed because open source should mean open source. No "open core" tricks. No "community edition" that's missing the good parts. The full engine, all 451K lines, is yours.
>
> I'd love to hear what you think. What would you build with 5 verbs?

---

## 4. First Comment Strategy

### First Comment (posted by maker immediately after launch)

> Quick start in 30 seconds:
>
> ```bash
> # Install
> cargo install nika
>
> # Create your first workflow
> cat > hello.nika.yaml << 'EOF'
> schema: nika/workflow@0.12
> tasks:
>   - id: greet
>     infer: "Write a haiku about Rust programming"
> EOF
>
> # Run it
> nika run hello.nika.yaml
> ```
>
> Or try the interactive course:
> ```bash
> mkdir learn-nika && cd learn-nika
> nika init --course
> nika course next
> ```
>
> Full docs: https://github.com/supernovae-st/nika
> TUI demo: `nika ui`

### Anticipated Questions and Prepared Responses

**Q: "How does this compare to LangChain?"**
> LangChain is a Python SDK -- you write Python code that happens to call LLMs. Nika is a YAML engine -- you declare what you want and Nika figures out execution order, parallelism, and provider routing. Different philosophies: LangChain gives you maximum flexibility at the cost of boilerplate. Nika gives you maximum clarity at the cost of... writing YAML.

**Q: "Why YAML instead of a visual builder?"**
> YAML is version-controllable, diffable, reviewable, and composable. You can put a Nika workflow in a PR and your team can review it. You can copy-paste workflows between projects. You can generate them programmatically. Visual builders are great for demos -- YAML is great for production.

**Q: "Why Rust? Isn't that overkill for a workflow engine?"**
> Three reasons: (1) Single binary, zero dependencies -- `cargo install nika` and you're done. (2) Performance -- the engine processes workflows in milliseconds with SIMD-accelerated media tools. (3) Correctness -- 451K lines with zero clippy warnings and 8,100 tests means fewer surprises in production.

**Q: "What's the AGPL mean for me?"**
> If you use Nika as a CLI tool to run your workflows -- nothing changes, use it freely. The AGPL only kicks in if you modify Nika's source code and offer it as a service to others. Then you need to share your modifications. This is intentional: it prevents cloud providers from taking Nika, adding features, and selling it as a closed product.

---

## 5. Gallery Image Descriptions

### Image 1: Hero Shot
**Title:** Nika -- 5 Verbs to Orchestrate AI
**Description:** Split screen showing a clean YAML workflow on the left (with syntax-highlighted `infer:`, `fetch:`, and `exec:` verbs) and the Nika TUI on the right showing a running DAG with parallel task execution, live token streaming, and cost tracking. Dark theme with purple (#7c3aed) accents.

### Image 2: The 5 Verbs
**Title:** Everything Maps to 5 Verbs
**Description:** Infographic showing the 5 verbs as a horizontal flow: `infer:` (brain icon) -> `exec:` (terminal icon) -> `fetch:` (globe icon) -> `invoke:` (plug icon) -> `agent:` (loop icon). Below each verb, a one-line YAML example. Clean design on dark background.

### Image 3: Provider Ecosystem
**Title:** 22 Providers, One Syntax
**Description:** Grid of provider logos (Claude, GPT-4o, Gemini, Mistral, Groq, DeepSeek, xAI, Perplexity, native/GGUF) with a single YAML snippet showing `provider: claude` being swapped to `provider: groq`. Emphasis on how the rest of the workflow remains unchanged.

### Image 4: Course System
**Title:** Learn by Doing -- 12 Levels, 44 Exercises
**Description:** Screenshot of the constellation progress map in the terminal, showing completed levels glowing and upcoming levels dimmed. Inset showing a course exercise file with progressive hints. Title: "The documentation that teaches itself."

### Image 5: Media Pipeline
**Title:** 24 Built-in Media Tools
**Description:** Visual showing a pipeline: image import -> thumbnail generation -> metadata extraction -> chart creation -> C2PA signing. Each step shows the `invoke: nika:*` YAML syntax. No external services required.

### Image 6: Architecture
**Title:** Brain + Body Architecture
**Description:** Architecture diagram showing NovaNet (Knowledge Graph) connected to Nika (Workflow Engine) via MCP Protocol. Below: the DAG execution flow from YAML source through 2-phase AST to parallel runtime. Clean technical diagram.

---

## 6. Social Proof Suggestions

### Metrics to Highlight
- 451K lines of Rust
- 10 workspace crates
- 8,100+ tests passing
- 22 LLM providers supported
- 24 built-in media tools
- 200+ showcase workflows
- 44 interactive course exercises
- Zero clippy warnings
- Zero unsafe code
- Single binary deployment

### Testimonial Angles to Seek
- "First AI workflow tool where my YAML IS the documentation"
- "Switched from LangChain -- went from 200 lines of Python to 20 lines of YAML"
- "The course system taught me more about AI workflows than any tutorial"
- "Finally, an AI tool that respects open source with AGPL"

### Community Signals
- GitHub stars count at launch
- Number of showcase workflows (200+)
- Course exercise count (44)
- Crates.io download count
- Discord member count

---

## 7. Launch Day Timeline

### Pre-Launch (T-7 days)
- [ ] Finalize Product Hunt page copy
- [ ] Upload all 6 gallery images
- [ ] Schedule social media posts
- [ ] Brief ambassadors / early testers
- [ ] Ensure `cargo install nika` works flawlessly
- [ ] Record 60-second demo GIF for hero image
- [ ] Test-run the course system end-to-end

### Launch Day (T-0)

| Time (PT) | Action |
|-----------|--------|
| 12:01 AM | Product Hunt page goes live |
| 12:05 AM | Post maker comment |
| 12:10 AM | Post first comment (quick start guide) |
| 12:15 AM | Tweet launch announcement thread |
| 12:30 AM | Post on Hacker News (Show HN) |
| 6:00 AM | Post Dev.to article |
| 7:00 AM | Send launch day email to newsletter |
| 8:00 AM | Engage with all PH comments (check every 30 min) |
| 9:00 AM | Share on Reddit (r/rust, r/artificial, r/opensource) |
| 10:00 AM | LinkedIn post |
| 12:00 PM | Mid-day engagement check -- respond to all comments |
| 3:00 PM | Post "What you can build" showcase thread on X |
| 6:00 PM | Evening engagement round |
| 9:00 PM | Thank you post on X + PH |
| 11:59 PM | End of voting period -- final push |

### Post-Launch (T+1 to T+7)
- [ ] Respond to every remaining PH comment
- [ ] Publish follow-up blog post with launch results
- [ ] Send follow-up email to newsletter
- [ ] Update GitHub README with PH badge
- [ ] Analyze traffic and conversion data
- [ ] Plan next feature based on feedback

---

## 8. Hashtags and Keywords

### Product Hunt
- AI Workflow Engine
- YAML
- Rust
- Open Source
- Developer Tools
- Automation
- MCP
- LLM

### Social Media Hashtags
`#opensource` `#rust` `#rustlang` `#ai` `#llm` `#devtools` `#yaml` `#workflow` `#automation` `#mcp` `#agpl` `#buildinpublic`

---

## 9. Key Differentiators to Emphasize

| What Others Do | What Nika Does |
|----------------|----------------|
| Python SDK (learn their abstractions) | YAML declaration (learn 5 verbs) |
| Single provider per project | 22 providers, mix within one workflow |
| No media processing | 24 built-in media tools |
| README + API docs | 44-exercise interactive course |
| MIT/Apache (cloud-exploitable) | AGPL-3.0 (community-protected) |
| JavaScript/Python runtime | Rust single binary, zero deps |
| Separate orchestration layer | DAG scheduling built into the engine |
| Imperative tool calling | Declarative MCP integration |

---

## 10. Competitor Positioning (for comments/responses)

**vs Dify:** "Dify is a visual builder -- great for non-developers. Nika is a YAML engine -- great for developers who want version control, CI/CD, and code review on their workflows."

**vs LangChain:** "LangChain gives you Python building blocks. Nika gives you a complete engine. Write YAML, get DAG scheduling, parallel execution, structured output, and 22 providers out of the box."

**vs n8n:** "n8n is a general automation platform. Nika is purpose-built for AI workflows -- with native LLM support, agent loops, MCP integration, and a media pipeline."

**vs Temporal:** "Temporal is infrastructure for durable workflows. Nika is a developer tool for AI-specific workflows. They can actually complement each other -- Nika for AI orchestration, Temporal for durability."

---

*Prepared for SuperNovae Studio. Last updated 2026-03-23.*
