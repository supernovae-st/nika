# Nika Media Kit

> They built walls around intelligence. We compiled a door.
>
> Nika is the open-source AI workflow engine that turns YAML text files into
> multi-provider AI pipelines — no Python, no Docker, no subscriptions. One
> binary. Five verbs. Freedom by default.

All assets for press, social media, conference talks, and partner communications.

---

## About Nika

AI is the new electricity. Six closed labs control it. Chips cost $6M per rack.
Subscriptions run $20-$200/month. And even if you pay, you still need a software
engineer to wire anything useful.

Nika exists to end that. A single Rust binary that gives anyone access to any AI
provider, orchestrated through a text file. No Python. No Docker. No cloud. No
subscription. AGPL-licensed so it stays free forever.

The gap between "AI exists" and "I can use AI" should be zero. Nika is not a
product. It's a movement.

---

## Brand Identity

| Element | Value |
|---------|-------|
| **Name** | Nika |
| **Symbol** | Blue Butterfly — liberation, transformation, the impossible becoming possible |
| **Primary tagline** | Automate AI. No code required. |
| **Manifesto tagline** | The Drums of Liberation. In Rust. |
| **Technical tagline** | 5 verbs. 14 LLM providers. Zero Python. |
| **Mission tagline** | AI shouldn't have a subscription fee. |
| **License** | AGPL-3.0-or-later (dual-licensed with commercial license available) |
| **Website** | https://nika.supernovae.studio |
| **GitHub** | https://github.com/supernovae-st/nika |

### Visual Palette

| Name | Hex | Usage |
|------|-----|-------|
| Solarized Dark | `#002b36` | Primary background — terminal-native aesthetic |
| Electric Blue | `#3b82f6` | Primary accent — links, highlights, the butterfly |
| Indigo | `#6366f1` | Secondary accent — depth, secondary elements |
| Cyan | `#06b6d4` | Tertiary accent — data flow, fetch verb |
| Near White | `#f8fafc` | Text on dark — high contrast |
| Amber | `#f59e0b` | Warning/action — exec verb, alerts |
| Emerald | `#10b981` | Success — completions, passing tests |

**Aesthetic:** Terminal-first. Dark mode. Monospace. Not glassmorphism. Not
gradients. Not purple AI glow.

**Photography style:** Hacker in garage. Terminal at 3am. Real dev environments.
Not stock photos. Not corporate office.

### Typography

| Context | Font | Weight |
|---------|------|--------|
| Headings | Inter | 600 (Semi-Bold), 700 (Bold) |
| Body | Inter | 400 (Regular) |
| Code | JetBrains Mono | 400 (Regular) |
| Terminal screenshots | JetBrains Mono | 400, with ligatures |

### Symbol Usage

- The butterfly represents transformation and liberation
- Always render as a single character when inline
- For logos: use the SVG butterfly mark, not the emoji
- Acceptable pairings: "Nika", "Nika Engine", "SuperNovae / Nika"
- Never: "Nika.ai", "NikaAI", "NIKA"

---

## Images

### Logo

| Asset | Format | Dimensions | Background | File |
|-------|--------|------------|------------|------|
| Logo mark | SVG | Scalable | Transparent | `logo-mark.svg` |
| Logo mark | PNG | 512x512 | Transparent | `logo-mark-512.png` |
| Logo mark | PNG | 1024x1024 | Transparent | `logo-mark-1024.png` |
| Logo + wordmark | SVG | Scalable | Transparent | `logo-wordmark.svg` |
| Logo + wordmark | PNG | 512x128 | Transparent | `logo-wordmark-512.png` |
| Logo dark bg | PNG | 512x512 | Navy #0f172a | `logo-mark-dark-512.png` |
| Logo light bg | PNG | 512x512 | White #f8fafc | `logo-mark-light-512.png` |
| Favicon | ICO | 32x32, 16x16 | Transparent | `favicon.ico` |
| Favicon | SVG | Scalable | Transparent | `favicon.svg` |

### Banners

| Asset | Dimensions | Usage | File |
|-------|------------|-------|------|
| OG Image | 1200x630 | Link previews (Twitter, Slack, Discord) | `og-image.png` |
| Presentation hero | 1920x1080 | Slide decks, conference talks | `hero-1920.png` |
| GitHub social | 1280x640 | GitHub repository social preview | `github-social.png` |
| Product Hunt | 1270x760 | Product Hunt gallery (5 images) | `ph-gallery-*.png` |

### Social Cards

| Asset | Dimensions | Platform | File |
|-------|------------|----------|------|
| Square card | 1200x1200 | Twitter/X, Instagram | `social-square.png` |
| Landscape card | 1200x630 | LinkedIn, Facebook | `social-landscape.png` |
| Story format | 1080x1920 | Instagram/Twitter Stories | `social-story.png` |

### Terminal Screenshots

All screenshots taken with a clean terminal (JetBrains Mono, navy background, no window chrome) using a screenshot tool that produces clean PNGs with padding.

| Screenshot | Command | Description | File |
|------------|---------|-------------|------|
| Run output | `nika run demo.nika.yaml` | Colored task execution with timing + DAG progress | `screenshot-run.png` |
| Doctor | `nika doctor` | System health check with provider status | `screenshot-doctor.png` |
| Check | `nika check workflow.nika.yaml` | Validation output with green checkmarks | `screenshot-check.png` |
| Provider list | `nika provider list` | All 14 LLM providers with API key status | `screenshot-providers.png` |
| Showcase | `nika showcase list` | Browse 115 pre-built workflow templates | `screenshot-showcase.png` |
| VS Code LSP | (editor screenshot) | Inline syntax validation + DAG webview | `screenshot-vscode-lsp.png` |

### Diagrams

| Diagram | Description | Format | File |
|---------|-------------|--------|------|
| 5 Verbs | The five verbs with icons and descriptions | SVG | `diagram-5-verbs.svg` |
| Architecture | Engine internals (AST, DAG, runtime) | SVG | `diagram-architecture.svg` |
| Data flow | `with:` bindings and template resolution | SVG | `diagram-data-flow.svg` |
| Provider map | All 14 LLM + 11 MCP providers with logos | SVG | `diagram-providers.svg` |
| Benchmark chart | RAM comparison vs Python frameworks | SVG | `chart-benchmark.svg` |

---

## Videos

### 30-Second Teaser

| Property | Value |
|----------|-------|
| **Duration** | 30 seconds |
| **Audio** | Silent with background music option |
| **Subtitles** | Burned-in, white on semi-transparent black |
| **Resolution** | 1920x1080 (16:9) + 1080x1080 (1:1 crop) |
| **Platform** | Twitter/X, LinkedIn, Product Hunt |
| **File** | `teaser-30s.mp4` |

**Storyboard:**

| Time | Visual | Subtitle |
|------|--------|----------|
| 0-5s | Logo reveal with butterfly animation | "Nika" |
| 5-12s | YAML file being typed, line by line | "Write AI workflows in YAML" |
| 12-18s | Terminal running `nika run`, tasks completing | "Run them anywhere" |
| 18-24s | Split: Claude, GPT, Mistral, local model logos | "Any provider. No lock-in." |
| 24-28s | RAM benchmark bar chart animating | "5x less memory than Python" |
| 28-30s | Logo + tagline + GitHub URL | "Automate AI. No code required." |

### 2-Minute Demo

| Property | Value |
|----------|-------|
| **Duration** | 2 minutes |
| **Audio** | Narrated voiceover |
| **Resolution** | 1920x1080 |
| **Platform** | YouTube, Product Hunt, landing page |
| **File** | `demo-2min.mp4` |

**Script Outline:**

| Time | Section | Content |
|------|---------|---------|
| 0:00-0:15 | Hook | "What if you could automate any AI task in 8 lines of YAML?" |
| 0:15-0:45 | Problem | Show Python boilerplate for a simple summarizer (~60 lines). Show the equivalent Nika YAML (~8 lines). |
| 0:45-1:15 | Live demo | Run the HN scraper workflow. Show output streaming in real time. |
| 1:15-1:35 | Features | Quick montage: swap providers (1 line change), VS Code LSP validation, 115 showcase workflows. |
| 1:35-1:50 | Technical | Single binary, 28 MB RAM, <50ms startup, 8,600+ tests, AGPL. |
| 1:50-2:00 | CTA | "brew install supernovae-st/tap/nika — `nika init` to scaffold a project." |

### 5-Minute Deep Dive

| Property | Value |
|----------|-------|
| **Duration** | 5 minutes |
| **Audio** | Narrated with terminal recordings |
| **Resolution** | 1920x1080 |
| **Platform** | YouTube, dev conferences, meetup talks |
| **File** | `deepdive-5min.mp4` |

**Script Outline:**

| Time | Section | Content |
|------|---------|---------|
| 0:00-0:30 | Intro | Who, what, why. The "AI is electricity" thesis. |
| 0:30-1:30 | The 5 Verbs | Walk through each verb with a live example. |
| 1:30-2:30 | DAG & Bindings | Show parallel execution, `with:` bindings, pipe transforms. |
| 2:30-3:30 | Agent Loop | Demo an autonomous agent with tool use (MCP integration). |
| 3:30-4:15 | Architecture | AST pipeline, provider abstraction, media tools. |
| 4:15-4:45 | Provider strategy | 14 LLM providers + 11 MCP integrations + native GGUF. Live provider swap (Anthropic → Groq → DeepSeek) with single line YAML change. |
| 4:45-5:00 | CTA | GitHub, docs, community links. |

---

## Audio

### Podcast Elevator Pitch (30 seconds)

> "Nika is an open-source engine that lets you automate AI tasks by writing YAML
> files instead of Python scripts. You describe what you want — fetch a webpage,
> summarize it with Claude or GPT, translate it, process images — and Nika runs
> the whole pipeline. It ships as a single Rust binary, uses a fraction of the
> memory of Python frameworks, and works with any LLM provider. It's AGPL
> licensed because we believe AI tools should stay open."

### Podcast Episode Outline (Guest Appearance)

**Target**: Developer-focused podcasts (Changelog, Syntax, devtools.fm, Rustacean Station)

| Segment | Duration | Topics |
|---------|----------|--------|
| Intro | 2 min | Background, SuperNovae, the butterfly symbol |
| The Problem | 5 min | AI automation today: Python boilerplate, dependency hell, provider lock-in |
| The Solution | 8 min | YAML workflows, 5 verbs, DAG execution, provider abstraction |
| Technical Deep Dive | 10 min | Rust choice, AST pipeline, rig-core integration, MCP protocol |
| Open Source Philosophy | 5 min | Why AGPL, the "AI is electricity" thesis, community-first approach |
| Showcase & Onboarding | 5 min | 115 showcase workflows, `nika init` scaffolding, VS Code LSP integration, MCP-first development |
| Future | 5 min | Roadmap, Nika Memory engine (ADR-004), distribution, public arXiv paper Q4 2026 |
| Q&A / Close | 5 min | Where to find it, how to contribute, community links |

---

## Copy

### One-Liner (15 words)

> Nika automates AI tasks in YAML — any provider, single binary, open source.

### Elevator Pitch (30 seconds / 75 words)

> Nika is an open-source workflow engine that automates AI tasks using simple YAML
> files. Instead of writing Python boilerplate to chain LLM calls, you describe
> your workflow with five verbs — infer, fetch, exec, invoke, agent — and Nika
> runs it. It works with 14 LLM providers (Claude, GPT, Mistral, Gemini, Groq,
> DeepSeek, xAI, and 7 OpenAI-compat), ships as a single 15 MB Rust binary, and
> uses 5x less RAM than Python alternatives. AGPL licensed. No lock-in, ever.

### Blog Post Pitch (1 paragraph)

> We just open-sourced Nika, a workflow engine that lets anyone automate AI tasks
> by writing YAML instead of Python. It supports 14 LLM providers plus 11 MCP
> integrations (GitHub, Slack, Neo4j, Perplexity, Firecrawl, etc.), runs as a
> single Rust binary with zero dependencies, and ships with a 115-workflow
> showcase library to learn from real examples. We built it because we believe AI
> automation shouldn't require a CS degree — if you can edit a text file, you can
> orchestrate Claude, GPT, and Mistral to do real work. Nika is pre-1.0 but
> battle-tested with 8,600+ tests, and the AGPL license ensures it stays open.
> We'd love to share the story of why we chose YAML over Python, Rust over Node,
> and AGPL over MIT.

### Press Release Draft

---

**FOR IMMEDIATE RELEASE**

**SuperNovae Studio Launches Nika: Open-Source AI Workflow Engine That Replaces Python Boilerplate with YAML**

*Single Rust binary automates AI tasks across 14 LLM providers and 11 MCP integrations with 5x less memory than Python frameworks*

**Paris, May 5, 2026** — SuperNovae Studio today announced the public release of
Nika, an open-source workflow engine that enables anyone to automate AI tasks by
writing simple YAML configuration files instead of Python code.

Nika introduces a five-verb abstraction for AI automation: `infer` for LLM
generation, `fetch` for HTTP requests, `exec` for shell commands, `invoke` for
MCP tool calls, and `agent` for autonomous multi-turn loops. Workflows are
defined in `.nika.yaml` files and executed via a single command-line interface.

"AI is becoming like electricity — everyone uses it, but today you need to be an
electrician to wire it up," said Thibaut Mélen, creator of Nika. "We built Nika
so that describing an AI workflow is as simple as writing a recipe."

**Key features:**

- **Provider agnostic**: Works with 14 LLM providers (Claude, GPT, Mistral,
  Gemini, Groq, DeepSeek, xAI + 7 OpenAI-compatible like OpenRouter, Together,
  Fireworks, Cerebras, SambaNova, Cohere, AI21) plus native GGUF and 11 MCP
  integrations (GitHub, Slack, Neo4j, Perplexity, Firecrawl, Postgres, etc.)
- **Single binary**: Ships as a ~15 MB Rust executable with zero runtime
  dependencies — no Python, no Docker, no Node.js
- **Resource efficient**: Uses ~28 MB RAM for typical workflows vs ~140 MB for
  equivalent Python framework implementations
- **115 showcase workflows**: Pre-built templates for common AI automation
  patterns — extract one with `nika showcase extract <name>`
- **VS Code LSP integration**: Real-time syntax validation, autocomplete, and
  DAG visualization for `.nika.yaml` files
- **Defense-in-depth security**: 6-layer Shield (policy + spotlight + structured
  output + capabilities + canary tokens + audit telemetry) protects against
  prompt injection
- **Structured output**: 4-layer JSON schema enforcement (tool injection,
  extract+validate, retry, LLM repair) — same result on every provider
- **MCP native**: First-class Model Context Protocol support for tool extension
- **AGPL licensed**: Ensures the tool and all derivatives remain open source
  (commercial license available for proprietary use)

Nika is available now on GitHub at https://github.com/supernovae-st/nika and
via Homebrew (`brew install supernovae-st/tap/nika`).

**About SuperNovae Studio**

SuperNovae Studio builds open-source AI infrastructure. The company's flagship
project is Nika (workflow engine), with the upcoming Nika Memory cognitive
memory engine planned for release in Q4 2026.

**Contact**: press@supernovae.studio | https://supernovae.studio

---

## Asset Checklist

### Priority 1 — Launch Day Required

- [ ] Logo SVG + PNG (512, 1024)
- [ ] OG image (1200x630)
- [ ] GitHub social preview (1280x640)
- [ ] 5 Product Hunt gallery images (1270x760)
- [ ] Terminal screenshots (6 total: run, doctor, check, providers, showcase, VS Code LSP)
- [ ] 30-second teaser video
- [ ] Social cards (square + landscape)

### Priority 2 — Launch Week

- [ ] 2-minute demo video
- [ ] Benchmark chart SVG
- [ ] Architecture diagram SVG
- [ ] 5 Verbs diagram SVG
- [ ] Provider map diagram SVG (14 LLM + 11 MCP)
- [ ] Blog post (for company blog + dev.to)
- [ ] Twitter/X thread (10 tweets)
- [ ] LinkedIn announcement post

### Priority 3 — Post-Launch

- [ ] 5-minute deep dive video
- [ ] Podcast pitch + booking
- [ ] Story format assets (1080x1920)
- [ ] Conference talk slide deck
- [ ] Press release distribution
- [ ] Data flow diagram

---

## Distribution Channels

| Channel | Asset Type | Timing |
|---------|-----------|--------|
| Product Hunt | Gallery + description + maker comment | Launch day, 12:01 AM PT |
| Hacker News | Show HN post | Launch day, 8 AM ET |
| Twitter/X | Teaser video + thread | Launch day, 9 AM ET |
| LinkedIn | Announcement + social card | Launch day, 10 AM ET |
| Reddit | r/rust, r/programming, r/artificial | Launch day, staggered |
| Dev.to | Blog post | Launch day or day after |
| GitHub | Release notes + social preview | Pre-launch (ready before) |
| YouTube | 2-min demo | Launch day |
| Newsletters | Pitch to TLDR, Console, Changelog | 1 week before launch |
| Podcasts | Pitch + booking | 2-4 weeks before launch |
