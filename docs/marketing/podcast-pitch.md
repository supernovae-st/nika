# Nika Podcast Outreach Materials

> Narrative reference: `docs/vision/podcast-drums-of-liberation.md` — the full
> "Drums of Liberation" podcast script (in French). Draw from it for conviction,
> but lead with the English-language talking points below.

Everything needed to pitch, prepare for, and follow up on podcast guest appearances.

---

## Core Pitch Angle: "AI Is Electricity — Who Controls the Grid?"

Two hundred years ago, only a handful of engineers understood electricity. Kings
and industrialists had private generators. Everyone else lived by candlelight.
Then came distribution networks — and within two generations, electricity became
a right, not a privilege.

In 2026, the resource is not electricity. It is intelligence. Six labs control
frontier AI. Chips cost $6M per rack (sold out for two years). Subscriptions run
$20-$200/month. The frameworks that promise "democratization" charge $49/mo to
run automations on THEIR servers, with THEIR limits, under THEIR terms.

Nika is the distribution network. A single open-source binary that gives anyone
access to any AI provider, orchestrated through a text file. No Python. No
Docker. No cloud. No subscription. AGPL-licensed so it stays free forever.

**The question for the host:** Who should control the grid?

---

## 30-Second Elevator Pitch

> "AI is the new electricity — but right now, six labs control the grid and you
> need a software engineer to flip the switch. I built Nika to fix that. It's an
> open-source engine where you write AI workflows in a text file — 5 verbs, any
> provider, one Rust binary, 5x less memory than Python frameworks. AGPL
> licensed, free forever. Because the gap between 'AI exists' and 'I can use AI'
> should be zero."

**Variants:**

*For non-technical audiences:*
> "You know how you copy-paste between ChatGPT tabs — feeding one answer into
> the next prompt, manually, over and over? Nika automates that. You write the
> steps in a text file, hit run, and it chains everything together. No coding,
> no subscriptions. AI should be like electricity — you shouldn't need to be an
> electrician to turn on the lights."

*For Rust/systems audiences:*
> "Nika is a 15 MB Rust binary that replaces Python AI orchestration frameworks.
> Three-phase AST, DAG-based parallel execution, 22 provider backends, structured
> output validation — all with zero dependencies for the end user. 4ms cold start.
> 28 MB RSS. 7,800+ tests. AGPL licensed. Performance is not a luxury.
> Performance is freedom."

*For open-source/ethics audiences:*
> "In 1984, they controlled the words. In 2026, they control the tokens. Every
> major AI tool is either proprietary or VC-funded with an inevitable rug pull.
> Nika is AGPL-licensed — if you use it as a service, you contribute back. No
> cloud lock-in, no telemetry, no premium tier. MIT and Apache are gifts to
> corporations. AGPL breaks that pattern. We chose the community contract."

---

## Episode Outline (Guest Appearance — 30 Minutes)

### 1. The Problem: Gatekept Intelligence (5 min)

**Talking points:**
- The electricity parallel: 200 years ago, only engineers had generators. Today, only engineers have AI pipelines. The resource changed, the gatekeeping didn't.
- The two gates: code (you need Python to chain two API calls) and money ($20-200/month per provider, $49/mo for basic automation platforms, $6M per GPU rack)
- The Orwell parallel: in 1984, controlling vocabulary limited what people could think. In 2026, controlling access to LLMs limits cognitive augmentation. Not limiting words — limiting tokens.
- The person with GPT-5 and Claude thinks faster, codes quicker, analyzes deeper. That is not an economic gap. It is a cognitive gap.
- 57% of internet content is now AI-generated, but most humans cannot access the tools that generate it

**Sound bite:** "In 1984, they controlled the words. In 2026, they control the tokens. Not anymore."

### 2. The Response: 5 Verbs and a Text File (5 min)

**Talking points:**
- The insight: AI tasks are workflows, not programs. They have inputs, outputs, and dependencies — just like CI/CD pipelines
- YAML is already how ops people describe infrastructure (Kubernetes, Docker Compose, GitHub Actions). It is readable by non-developers. A product manager can read a `.nika.yaml` and understand what it does.
- Five verbs is all you need: `infer:` (think), `exec:` (run), `fetch:` (get), `invoke:` (call a tool), `agent:` (multi-turn loop)
- The five verbs are not a limitation. They are a language. 90% of real-world AI automation fits in a DAG of these 5 operations.
- Workflows are data, not code. They live in git, they diff cleanly, they are reviewable by anyone.

**Sound bite:** "If your AI workflow fits in 5 verbs and a DAG, Nika is faster to write, faster to run, and easier to maintain than any Python framework."

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

### 4. Why Rust — Performance Is Freedom (5 min)

**Talking points:**
- The benchmarks (measured, not marketing): 4ms cold start vs 62ms Python. 28 MB RSS vs 140 MB LangChain. 15 MB binary vs 150+ pip packages.
- When your tool is lightweight, it goes everywhere. Raspberry Pi. $5 VPS. CI pipeline. Offline. That is not optimization — that is reach. That is access. That is the mission.
- Single binary distribution: `brew install` and you're done. No Python, no Node, no Docker. The tool should be simpler than your problem.
- The three-phase AST: catches errors before execution, not during. YAML is validated like a compiled language.
- DAG-based execution: automatic parallelism. Nika figures out what can run at the same time.
- 451K lines of Rust. 10 crates. 7,800+ tests. Zero clippy warnings.

**Sound bite:** "Performance is not a luxury. Performance is freedom. When your binary is 15 MB, it runs on a $5 VPS in Lagos as easily as a $6M rack in San Francisco."

### 5. Why AGPL — The License Is the Mission (5 min)

**Talking points:**
- The open-source capture playbook: Redis, Elasticsearch, MongoDB all started open, got acquired, re-licensed. The pattern is documented and repeating.
- MIT and Apache are gifts to corporations. Legal, but they kill communities. Amazon builds a competing service with your code and your usage data, and you can do nothing.
- AGPL means: use it freely, modify it freely, but if you deploy it as a service, share your changes. Your private YAML workflows are yours — the license applies to the engine.
- No venture capital. No premium tier. No "enterprise edition" planned. Sustainability comes from community, sponsorships, consulting.
- This is a deliberate, controversial choice. Worth the cost in corporate adoption because the alternative — getting absorbed — is worse.

**Sound bite:** "MIT license is a gift to corporations. AGPL is a contract with the community. We chose the contract."

### 6. The Name and the Symbol (2 min)

**Talking points:**
- The butterfly symbol: transformation, lightness, freedom. One butterfly is powerful. A swarm is a climax.
- The name resonates with hundreds of millions of manga readers — but you do not need to know the reference. The code works regardless.
- The course built into the CLI is called "Liberation." 12 levels, 44 exercises, learn by doing.
- Not just branding — a genuine philosophical alignment with liberation over control

**Sound bite:** "The name means 'smile' in Japanese. We put a butterfly on the flag and set sail."

### 7. What's Next (3 min)

**Talking points:**
- The 1.0 vision: make AI automation as normal as running a shell script
- Upcoming: package registry (share and reuse workflows), native local model support, visual editor
- The community: how to contribute (workflows, providers, documentation, bug reports)
- NovaNet: the knowledge graph that connects to Nika via MCP — brain and body, separate but coordinated
- Call to action: star on GitHub, try `nika init --course`, share a workflow

**Sound bite:** "We're maybe 40% of the way there. Come help us build the other 60%. Open Source must win. We wrote the weapon in Rust."

---

## Pitch Email Template

**Subject:** Podcast pitch: "AI is electricity — who controls the grid?" (open-source Rust engine, benchmarks, AGPL)

---

Hi [Host Name],

Two hundred years ago, only engineers had generators. Today, only engineers have AI pipelines. The resource changed, the gatekeeping didn't.

I built Nika to fix that — an open-source AI workflow engine that replaces 200 lines of Python with 10 lines of YAML. Single Rust binary, 15 MB, zero dependencies, 4ms cold start, 22 AI providers, AGPL-licensed forever. The benchmarks against LangChain/CrewAI are dramatic: 5x less RAM, 15x faster startup.

I think your audience would find this interesting because [customize per podcast]:
- **For dev podcasts:** The Rust architecture is unusual — three-phase AST, DAG-based parallelism, 7,800+ tests, and a terminal UI, all in one binary. The "why Rust for an AI tool" story writes itself.
- **For open-source podcasts:** The AGPL licensing choice is deliberately controversial. MIT is a gift to corporations. AGPL is a contract with the community. That decision, and the reasoning behind it, is worth 20 minutes alone.
- **For AI/future podcasts:** The "AI as electricity" thesis — the Orwell parallel (controlling tokens is the new Newspeak), the $6M GPU racks, why the current model of AI access is creating a cognitive class divide, and what the alternative looks like.

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
| **CoRecursive** | Story-driven, dev audience | The journey from idea to 451K lines |
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
