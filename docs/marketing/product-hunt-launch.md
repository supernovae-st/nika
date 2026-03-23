# Product Hunt Launch Plan

## Tagline (60 chars)

> Automate AI in YAML — open source, any provider, no code

## Description (260 chars)

> Write steps in a simple text file. Nika runs them — fetch webpages, summarize with AI, translate, process images, run autonomous agents. Works with Claude, GPT, Mistral, Gemini, or local models. Single Rust binary. AGPL open source.

---

## Maker Comment (~200 words)

> Hi Product Hunt! I'm Thibaut, and I built Nika because I believe AI should be
> accessible to everyone — not just people who can write Python glue code.
>
> Most AI automation tools today require you to be a developer. You install
> packages, manage virtual environments, wrangle API clients, and write hundreds
> of lines of boilerplate just to chain two LLM calls together. I thought: what
> if you could describe what you want in a simple text file, and a fast binary
> just... runs it?
>
> That's Nika. You write YAML workflows with 5 verbs — infer, fetch, exec,
> invoke, agent — and Nika handles the rest. It works with any provider: Claude,
> GPT-4, Mistral, Gemini, Groq, xAI, DeepSeek, or fully local models via GGUF.
> No lock-in, ever.
>
> What makes it different: it's a single Rust binary (~15 MB). No runtime, no
> dependencies, no Docker. It uses 5x less RAM than equivalent LangChain setups
> because there's no Python interpreter, no GC, no framework overhead.
>
> Honest state: Nika is pre-1.0 (schema @0.12). It has 7,800+ tests and is
> battle-tested on real workflows, but APIs may still evolve. The license is AGPL
> because I believe open source AI tools should stay open.
>
> Want to try it? Run `nika init --course` — it generates a 12-level interactive
> course with 44 exercises that teach you the entire engine, from hello-world to
> autonomous agents. No API key needed for the first levels.
>
> I'd love your feedback. What workflows would you automate first?

---

## Gallery (5 Screenshots)

| # | Title | Description | What to Show |
|---|-------|-------------|--------------|
| 1 | **Hero** | YAML workflow + terminal output side by side | Left: a clean 12-line `.nika.yaml` file in VS Code (dark theme). Right: terminal showing `nika run` with colored output — task names in blue, AI responses in white, timing in gray. The workflow fetches HN, summarizes with Claude, and outputs a markdown file. |
| 2 | **TUI** | Nika UI showing a workflow running | Full-screen `nika ui` in Studio view (hotkey 1/s). Left panel: workflow tree with task statuses (green checkmarks, spinning blue for in-progress). Right panel: live streaming output from an `infer:` task. Bottom bar: keybindings and elapsed time. |
| 3 | **Course** | Interactive learning constellation | `nika course status` output showing the 12-level constellation map. Stars represent levels, connected by lines. Completed levels glow green, current level pulses blue, locked levels are gray. Progress bar at bottom: "Level 5/12 — 18/44 exercises". |
| 4 | **5 Verbs** | The verb architecture with provider logos | Clean diagram showing the 5 verbs as colored cards: `infer:` (purple), `fetch:` (green), `exec:` (orange), `invoke:` (blue), `agent:` (red). Below each card, provider logos where applicable. Arrows show data flow between verbs via `with:` bindings. |
| 5 | **Benchmark** | RAM comparison chart | Horizontal bar chart comparing peak RSS for an equivalent workflow: Nika (~28 MB), LangChain (~140 MB), AutoGen (~180 MB), CrewAI (~160 MB). Clean design, navy background, blue bars. Subtitle: "Same task, same model, measured with /usr/bin/time -v". |

---

## Topics

- AI
- Developer Tools
- Open Source
- Rust
- Automation

## Pricing

**Free** — Open source, AGPL-3.0-or-later

---

## Launch Checklist

- [ ] Product Hunt listing created and scheduled
- [ ] All 5 gallery images produced (1270x760 recommended)
- [ ] Maker comment drafted and reviewed
- [ ] GitHub README polished with PH badge
- [ ] Landing page live with PH redirect
- [ ] Social posts queued (Twitter, LinkedIn, Mastodon)
- [ ] First-day responses prepared (FAQ below)
- [ ] Team hunters notified
- [ ] Upvote CTA ready (not spammy — genuine ask to community)

## Prepared Responses (PH Comments)

### "How is this different from n8n / Make / Zapier?"

> Those are visual workflow tools for connecting SaaS apps. Nika is a code-first
> engine specifically designed for AI tasks — LLM inference, autonomous agents,
> media processing. It runs locally as a CLI, not in a browser. Think of it as
> "Makefile for AI" rather than "Zapier for AI".

### "Can I use it without knowing YAML?"

> The `nika init --course` command generates a 12-level interactive course that
> teaches you everything step by step. If you can edit a text file, you can use
> Nika. We also have 200+ showcase workflows you can extract and modify.

### "Why not a GUI?"

> The TUI (`nika ui`) gives you a terminal-based visual interface. A web GUI is
> on the roadmap but not the priority — we believe the CLI-first approach gives
> you composability (pipe, cron, CI/CD) that GUIs can't match.

### "Will you add provider X?"

> Nika already supports 22+ providers via rig-core. If your provider has an
> OpenAI-compatible API, it works today via the `openai-compatible` provider
> config. For native integrations, open an issue on GitHub.
