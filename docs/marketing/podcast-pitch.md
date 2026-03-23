# Nika Podcast Outreach Materials

Everything needed to pitch, prepare for, and follow up on podcast guest appearances.

---

## 30-Second Elevator Pitch

> "I built an open-source tool that lets anyone automate AI tasks without writing code. You describe the steps in a YAML text file — fetch this webpage, summarize it with Claude, translate it to French — and Nika runs them all automatically. It works with any AI provider, it's a single Rust binary, and it uses 5x less memory than LangChain. We believe AI should be like electricity — accessible to everyone. The tool is called Nika, it's AGPL licensed, and it's free forever."

**Variants:**

*For non-technical audiences:*
> "Right now, if you want AI to do a series of tasks for you — like research a topic, write a summary, and translate it — you either do it manually, tab by tab, or you hire a developer. Nika is a free tool that lets you write those steps in plain English inside a simple text file, and it runs them all for you. No coding. No subscriptions. Just describe what you want, and it happens."

*For Rust/systems audiences:*
> "Nika is a 15 MB Rust binary that replaces Python AI orchestration frameworks. Three-phase AST, DAG-based parallel execution, 22 provider backends, structured output validation — all with zero dependencies for the end user. It cold-starts in 50 milliseconds and idles at 8 MB of RAM. AGPL licensed."

*For open-source/ethics audiences:*
> "Every major AI tool is either proprietary or VC-funded with an inevitable rug pull. Nika is an AGPL-licensed AI workflow engine — if you use it, you contribute back. No cloud lock-in, no telemetry, no premium tier. The name comes from One Piece's Sun God — the one who liberates. We think AI should be a public utility, and we're building the tools to make that real."

---

## Episode Outline (Guest Appearance — 30 Minutes)

### 1. The Problem (5 min)

**Talking points:**
- AI is incredibly powerful but locked behind two gates: code and subscriptions
- The "copy-paste workflow": most people use AI by switching between ChatGPT tabs, manually feeding output from one prompt into another
- Developer frameworks (LangChain, CrewAI) solve this — but only for Python developers, and they bring 150+ dependencies
- The gap: non-developers can't automate AI, and developers are over-served with complexity

**Sound bite:** "Right now, using AI for anything beyond a single prompt is either manual labor or a software engineering project. There's nothing in between. That's the gap Nika fills."

### 2. The "Aha Moment" — Why YAML? (3 min)

**Talking points:**
- YAML is already how ops people describe infrastructure (Kubernetes, Docker Compose, GitHub Actions)
- The key insight: AI tasks are workflows, not programs — they have inputs, outputs, and dependencies, just like CI/CD pipelines
- Five verbs is all you need: `infer:` (think), `exec:` (run), `fetch:` (get), `invoke:` (call a tool), `agent:` (multi-turn loop)
- "Why not just Python?" — because 90% of AI automation doesn't need Turing completeness, it needs clarity

**Sound bite:** "If your AI workflow needs a for-loop, you probably need YAML. If it needs recursion, you probably need a developer. Nika handles the first case, which is 90% of real-world use."

### 3. Live Demo (5 min)

**Setup:** Have a terminal ready. Pre-write but don't pre-run.

**Demo recipe:**
1. Start with a blank file: `demo.nika.yaml`
2. Write 3 tasks live: fetch a blog post, summarize it, generate tweet thread
3. Run it: `nika run demo.nika.yaml`
4. Show output streaming in real-time
5. Modify one line (change provider from Claude to GPT) and re-run
6. Show the TUI briefly: `nika ui`

**For audio-only podcasts:** Describe what's happening on screen. "I'm typing 10 lines of YAML... now I run it... and in 4 seconds I have a 3-tweet thread summarizing a 2000-word blog post."

**Sound bite:** "That's it. Ten lines. Four seconds. And I can swap the AI provider by changing one word."

### 4. Architecture — Why Rust? (5 min)

**Talking points:**
- Single binary distribution: `brew install` and you're done, no Python, no Node, no Docker
- Performance: 50ms cold start, 8 MB idle memory, compared to 3+ seconds and 180 MB for Python frameworks
- The three-phase AST: catches errors before execution, not during — YAML is validated like a compiled language
- DAG-based execution: automatic parallelism, Nika figures out what can run at the same time
- 22 provider backends: Claude, GPT, Gemini, Mistral, Groq, xAI, DeepSeek, local GGUF models
- The "single binary philosophy": your tool should be simpler than your problem

**Sound bite:** "Python AI frameworks are like bringing a shipping container to move a couch. Nika is the friend with a pickup truck."

### 5. The Mission — Open Source and AGPL (5 min)

**Talking points:**
- The open-source capture playbook: Redis, Elasticsearch, MongoDB all started open, got acquired, re-licensed
- MIT/Apache let corporations take without giving back — that's legal, but it kills communities
- AGPL means: use it freely, modify it freely, but if you deploy it as a service, share your changes
- "AI should be like electricity": a utility infrastructure, not a luxury product
- No venture capital, no premium tier, no "enterprise edition" planned
- Sustainability model: community contributions, sponsorships, consulting

**Sound bite:** "MIT license is a gift to corporations. AGPL is a contract with the community. We chose the contract."

### 6. The One Piece Connection (2 min)

**Talking points:**
- Nika is named after the Sun God Nika from One Piece — the mythical figure who liberates slaves and brings joy
- The butterfly symbol: transformation, lightness, freedom
- One Piece themes that map perfectly: freedom vs. control, community vs. empire, the impossible dream
- "The age of AI piracy": big tech hoards models behind APIs, open source crews sail freely
- Not just branding — it's a genuine philosophical alignment

**Sound bite:** "In One Piece, the World Government hoards power and calls freedom dangerous. Sound familiar? We put a butterfly on our flag and set sail anyway."

### 7. What's Next (3 min)

**Talking points:**
- Upcoming: package registry (share and reuse workflows), visual editor, native model support
- The course: 12 levels, 44 exercises, built into the CLI — learn by doing
- Community: how to contribute (workflows, providers, documentation, bug reports)
- The 1.0 vision: Nika as the standard CLI for AI task automation
- Call to action: star on GitHub, try the course, share a workflow

**Sound bite:** "The roadmap is simple: make AI automation as normal as running a shell script. We're maybe 40% of the way there. Come help us build the other 60%."

---

## Pitch Email Template

**Subject:** Show pitch: Open-source tool that replaces LangChain with 10 lines of YAML

---

Hi [Host Name],

I'm Thibaut, creator of Nika — an open-source AI workflow engine that lets you automate multi-step AI tasks in 10 lines of YAML instead of 200 lines of Python. It's a single Rust binary (15 MB, zero dependencies), works with 22 AI providers, and cold-starts in 50 milliseconds. We're AGPL licensed because we believe AI tools should stay open — permanently.

I think your audience would find this interesting because [customize per podcast]:
- **For dev podcasts:** The Rust architecture story is unusual — three-phase AST, DAG-based parallelism, and a terminal UI, all in one binary. Plus the benchmarks against LangChain/CrewAI are dramatic.
- **For open-source podcasts:** The AGPL licensing decision is deliberately controversial and worth discussing. Why we chose community contract over corporate gift.
- **For AI/future podcasts:** The "AI as electricity" thesis — why the current model of AI access (subscriptions, APIs, code) is unsustainable, and what the alternative looks like.

I can do a live demo in terminal (works great for video podcasts) or walk through the concepts with clear analogies (works for audio-only). Happy to adapt to your format.

GitHub: [link]
30-second demo: [link to teaser video]
Personal background: [1 sentence]

Would love to chat about a guest spot. What does your booking process look like?

Best,
Thibaut

---

## Podcast Target List

### Tier 1 — High Priority

| Podcast | Why | Angle |
|---------|-----|-------|
| **Changelog** | Largest open-source podcast | AGPL decision + Rust architecture |
| **Syntax.fm** | Huge dev audience, casual tone | "Replace Python with YAML" demo |
| **Ship It!** | CI/CD audience, workflow-native | YAML workflow engine = their language |
| **Rustacean Station** | Rust community | Architecture deep dive, single binary |
| **Practical AI** | AI practitioners | Benchmarks + real-world workflows |

### Tier 2 — Strong Fit

| Podcast | Why | Angle |
|---------|-----|-------|
| **CoRecursive** | Story-driven, dev audience | The journey from idea to 270k lines |
| **Software Engineering Daily** | Technical depth | Full architecture walkthrough |
| **FOSS Weekly** | Open source community | AGPL philosophy + community model |
| **The AI Breakdown** | AI news audience | "AI as electricity" thesis |
| **Indie Hackers** | Solo builders | Building without VC, AGPL sustainability |

### Tier 3 — Niche but Valuable

| Podcast | Why | Angle |
|---------|-----|-------|
| **Console.dev** | Developer tools newsletter + podcast | Tool showcase |
| **devtools.fm** | Developer tools specific | CLI UX + TUI design |
| **Open Source Startup** | OS business models | AGPL as business strategy |
| **Whiskeycast** (French tech) | French dev community | Franglais crossover appeal |

---

## Preparation Checklist

- [ ] Test demo workflow on the day of recording (APIs can break)
- [ ] Have backup demo that works offline (local GGUF model)
- [ ] Prepare 3 sound bites and practice saying them naturally
- [ ] Know the podcast's format: length, style, audience level
- [ ] Have GitHub link, install command, and website ready to share
- [ ] Send host a pre-read with the elevator pitch and 3 bullet points
- [ ] Follow up within 24 hours with links and a thank-you
