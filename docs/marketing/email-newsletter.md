# Email Newsletter Series -- Nika Launch

> 3-email launch sequence: Teaser, Launch Day, Follow-Up.
> Each email includes subject line A/B variants, full body, and CTA.

---

## Email 1: Teaser (T-7 days)

### Subject Lines (A/B/C)

**A:** `5 YAML verbs. 22 LLM providers. 451K lines of Rust. Next week.`
**B:** `We wrote 451K lines of Rust so you'd never write another AI SDK call`
**C:** `What if your AI workflow was just YAML?`

### Preview Text
`The AI workflow engine we've been building for 2 years is ready.`

### Body

---

**Subject:** 5 YAML verbs. 22 LLM providers. 451K lines of Rust. Next week.

Hi {{first_name}},

Next week, we're open-sourcing something we've been building for two years.

**Nika** is a semantic YAML workflow engine for AI tasks. Written in 451K lines of Rust. Compiled to a single binary. Zero runtime dependencies.

Here's the idea: every AI workflow you've ever built is made of 5 operations.

```yaml
infer:   # Call an LLM
exec:    # Run a command
fetch:   # Make an HTTP request
invoke:  # Call an MCP tool
agent:   # Run a multi-turn loop
```

That's it. Write your workflow in YAML using these 5 verbs. Nika handles DAG scheduling, parallel execution, multi-provider routing, structured output validation, and a media pipeline with 24 built-in tools.

**No Python runtime. No Docker. No SDK.**

A few numbers:

- **22 LLM providers** -- Claude, GPT-4o, Gemini, Groq, DeepSeek, and more
- **24 media tools** -- thumbnails, charts, PDF extraction, C2PA signing
- **44-exercise course** -- built-in interactive learning (nika init --course)
- **200+ showcase workflows** -- ready to extract and use
- **8,100+ tests** -- zero clippy warnings, zero unsafe code
- **AGPL-3.0** -- open source that stays open source

We're launching on **[Launch Date]** on Product Hunt and GitHub.

Want to be the first to try it? Reply to this email, and we'll send you early access to the repo before the public launch.

Talk soon,
**Thibaut**
SuperNovae Studio

---

### CTA Button
**Text:** `Reply for Early Access`
**Action:** Reply-to email

---

## Email 2: Launch Day (T-0)

### Subject Lines (A/B/C)

**A:** `Nika is live. 451K lines of Rust. 5 verbs. Your AI workflows, simplified.`
**B:** `We just open-sourced our AI workflow engine (451K lines of Rust)`
**C:** `cargo install nika -- your AI pipelines will never be the same`

### Preview Text
`Install in one command. Build your first AI workflow in 60 seconds.`

### Body

---

**Subject:** Nika is live. 451K lines of Rust. 5 verbs. Your AI workflows, simplified.

Hi {{first_name}},

Today is the day. **Nika is live.**

[Star on GitHub](https://github.com/supernovae-st/nika) | [Product Hunt](https://producthunt.com/posts/nika) | [Install now](#install)

### Install in one command

```bash
cargo install nika
```

### Build your first workflow in 60 seconds

```yaml
# hello.nika.yaml
schema: nika/workflow@0.12

tasks:
  - id: research
    fetch:
      url: https://en.wikipedia.org/wiki/Rust_(programming_language)
      extract: article

  - id: summarize
    with: { content: $research }
    infer: "Summarize in 3 bullet points: {{with.content}}"
```

```bash
nika run hello.nika.yaml
```

That's it. Two tasks, two verbs (`fetch:` + `infer:`), automatic dependency resolution.

### What makes Nika different

**5 verbs, not 50 abstractions.** `infer:`, `exec:`, `fetch:`, `invoke:`, `agent:`. Every AI task maps to one verb. Your workflow IS the documentation.

**22 providers, one syntax.** Mix Claude and Groq in the same workflow. Route cheap tasks to fast models. Cut costs by 60%.

**24 media tools, zero services.** Thumbnails, charts, PDF extraction, QR validation, C2PA signing -- all built into the binary.

**A course, not a README.** `nika init --course` generates 12 levels of interactive exercises. Learn by doing, not by reading.

**Rust, not Python.** Single binary. 8,100 tests. Starts in milliseconds. The TUI renders at 60fps.

**AGPL, not MIT.** Open source that stays open. No cloud provider can take your work and close the door.

### Three ways to start

1. **Quick start:** `nika init --minimal` -- 5 example workflows, one per verb
2. **Learn step by step:** `nika init --course` -- 44 exercises across 12 levels
3. **Browse examples:** `nika showcase list` -- 200+ ready-made workflows

### Support the launch

If Nika resonates with you, here's how you can help:

- [Star the repo on GitHub](https://github.com/supernovae-st/nika) -- it helps more people find the project
- [Upvote on Product Hunt](https://producthunt.com/posts/nika) -- we're launching today
- Share this email with a developer friend who might find it useful
- Try building something and tell us what you think

### What's next

Nika v0.42 is just the beginning. The roadmap includes multi-model routing presets, record compression for bounded context, orchestration mode (dynamic workflow generation from goals), and persistent memory via our NovaNet knowledge graph.

The 5 verbs are the foundation. Everything else builds on top.

Thank you for being part of this,
**Thibaut**
SuperNovae Studio

P.S. -- What would you build with 5 verbs? Reply to this email. I read every response.

---

### CTA Buttons
**Primary:** `Star on GitHub` -> https://github.com/supernovae-st/nika
**Secondary:** `Upvote on Product Hunt` -> [PH link]
**Tertiary:** `Install: cargo install nika`

---

## Email 3: Follow-Up (T+7 days)

### Subject Lines (A/B/C)

**A:** `Week 1 results: what {{x}} developers built with Nika`
**B:** `"I replaced 200 lines of LangChain with 20 lines of YAML" -- Week 1 update`
**C:** `What we learned from launching a 451K-line Rust project`

### Preview Text
`Launch stats, community feedback, and what's coming in v0.42.`

### Body

---

**Subject:** Week 1 results: what developers built with Nika

Hi {{first_name}},

One week since we launched Nika. Here's what happened.

### By the numbers

- **{{stars}} GitHub stars** (from 0 to {{stars}} in 7 days)
- **{{installs}} installs** via cargo install
- **{{ph_rank}}** on Product Hunt
- **{{hn_points}} points** on Hacker News
- **{{issues}} GitHub issues** filed (and {{closed}} already resolved)

### What people built

We've seen some incredible workflows this week:

**Content pipelines.** Several developers built multi-source research -> analysis -> publishing pipelines that mix Groq (for fast research) with Claude (for quality writing). One team reported 60% cost savings compared to their previous single-model approach.

**Image processing.** The media pipeline found its audience fast. Teams are using `nika:thumbnail` + `nika:optimize` + `nika:thumbhash` for automated image processing without any external services.

**Code agents.** The `agent:` verb with file tools (`nika:read`, `nika:edit`, `nika:grep`) is being used for automated code review with guardrails and cost limits.

### What we learned

**The course is a hit.** `nika init --course` has been the most-mentioned feature in feedback. People love learning by doing rather than reading docs. We're expanding it in v0.42.

**YAML skeptics became converts.** The most common feedback: "I was skeptical about YAML, but after seeing how readable the workflows are, I get it." The constraint IS the feature.

**AGPL sparked good conversations.** Most developers understand the license once we explain: use Nika as a tool freely, only share mods if you offer it as a service. The community appreciates the protection.

### Top community questions

**Q: Windows support?**
Not yet. macOS and Linux for now. Cross-compilation is on the roadmap -- Rust makes this feasible.

**Q: Can I use Nika with my existing Python tools?**
Yes -- `exec: command: "python3 my_script.py"` works. Nika orchestrates your existing tools via shell commands, HTTP, or MCP.

**Q: What about streaming output?**
Streaming works for all providers. The TUI shows token-by-token output in real-time. CLI mode (`nika run`) will support streaming in v0.42.

### What's coming in v0.42

Based on your feedback, here's what we're prioritizing:

1. **Model routing presets** -- Named model slots (default, lite, think, search) for per-task provider selection
2. **Record compression** -- Compressed task results so downstream tasks get summaries, not raw data
3. **Streaming CLI output** -- Real-time token output in `nika run` mode
4. **More course levels** -- Community-contributed exercises and advanced patterns

### How to get involved

- **Contribute:** PRs welcome. Check the "good first issue" label.
- **Report bugs:** GitHub issues. Include the NIKA-XXX error code if you have one.
- **Share workflows:** Built something cool? Open a PR to the showcase.
- **Join the conversation:** [Discord link] | [GitHub Discussions link]

### One more thing

We're planning a live coding session next week where we'll build a complete multi-model content pipeline from scratch using Nika. Follow [@SuperNovae_st](https://twitter.com/SuperNovae_st) for the announcement.

Thank you for making this first week incredible. Open source is a team sport, and we're glad you're on the team.

**Thibaut**
SuperNovae Studio

P.S. -- If you built something with Nika this week, reply with a link or a screenshot. We'd love to feature it.

---

### CTA Buttons
**Primary:** `Star on GitHub` -> https://github.com/supernovae-st/nika
**Secondary:** `Join Discord` -> [Discord invite]

---

---

## Email Design Appendix: Key Talking Points

### For Subject Line Testing

**Hooks that work for developer audiences:**
- Numbers ("451K lines", "5 verbs", "22 providers")
- Contrasts ("never write another SDK call")
- Questions ("What if your AI workflow was just YAML?")
- Concrete actions ("cargo install nika")

**Hooks to avoid:**
- Superlatives ("Revolutionary", "Game-changing")
- Vague claims ("10x productivity")
- Urgency ("Last chance", "Don't miss") -- developers see through this
- Emoji in subject lines -- reduces open rate for technical audiences

### Content Block Library

These are reusable content blocks that can be mixed into any email:

**Block: The 5-Verb Pitch**
```
Every AI workflow is 5 operations:
infer: -- call an LLM
exec: -- run a command
fetch: -- make an HTTP request
invoke: -- call a tool
agent: -- run a multi-turn loop
Write YAML. Nika does the rest.
```

**Block: The Cost Story**
```
Multi-model routing: route simple tasks to Groq ($0.06/1M tokens),
complex tasks to Claude. Same workflow, 60% cheaper.
```

**Block: The Single Binary Story**
```
cargo install nika. One command. One binary.
No Python. No Docker. No node_modules.
Starts in milliseconds. Runs anywhere.
```

**Block: The Course Story**
```
nika init --course: 12 levels, 44 exercises, 3-tier hints.
From "what is a workflow" to multi-agent orchestration.
The documentation that teaches itself.
```

**Block: The AGPL Story**
```
AGPL-3.0: use Nika freely for anything.
Only share modifications if you offer them as a service.
Open source that stays open source.
```

**Block: The Media Pipeline Story**
```
24 built-in tools: thumbnails, charts, PDF extraction,
C2PA signing, QR validation. All in the binary.
No external services. No API keys. No Docker.
```

### Personalization Variables

| Variable | Source | Example |
|----------|--------|---------|
| `{{first_name}}` | Signup form | "Thibaut" |
| `{{stars}}` | GitHub API | "247" |
| `{{installs}}` | crates.io API | "89" |
| `{{ph_rank}}` | Product Hunt | "#3 Product of the Day" |
| `{{hn_points}}` | HN API | "312" |
| `{{issues}}` | GitHub API | "17" |
| `{{closed}}` | GitHub API | "11" |

### Send Time Optimization

| Audience | Best Day | Best Time (ET) |
|----------|----------|----------------|
| US developers | Tuesday-Thursday | 9-10 AM |
| EU developers | Tuesday-Wednesday | 2-3 PM (8-9 AM ET) |
| Mixed global | Tuesday | 10 AM ET |

### Unsubscribe Copy

> You're receiving this because you signed up for SuperNovae Studio updates.
> [Unsubscribe]({{unsubscribe_url}}) | [Update preferences]({{preferences_url}})
> SuperNovae Studio, Paris, France

---

## Email Design Notes

### Consistent Elements

- **From name:** Thibaut @ SuperNovae
- **Reply-to:** thibaut@supernovae.studio (personal, not noreply)
- **Footer:** GitHub | Twitter | Unsubscribe
- **Tone:** Technical but warm. Developer-to-developer, not brand-to-consumer.
- **Code blocks:** Monospaced font with dark background. YAML syntax highlighting where possible.
- **Mobile:** All code blocks should be readable on mobile (max 60 chars per line)

### Segmentation

| Segment | Email 1 | Email 2 | Email 3 |
|---------|---------|---------|---------|
| Early access requesters | Full teaser | Launch + "you got early access!" note | Results + exclusive beta features |
| Newsletter subscribers | Full teaser | Launch | Results |
| Product Hunt followers | Skip | Launch (PH-specific CTA) | Results |
| GitHub stargazers | Skip | Skip (they already know) | Results + contributor CTA |

### A/B Testing Plan

- **Email 1:** Test all 3 subject lines. Send winning variant to remaining 50% after 4 hours.
- **Email 2:** Test A vs B. C is backup if opens are low.
- **Email 3:** Personalize subject line with actual metrics (stars, installs).

---

*Prepared for SuperNovae Studio. Last updated 2026-03-23.*
