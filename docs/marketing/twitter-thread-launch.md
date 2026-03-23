# Nika Launch Thread — Twitter/X

**Author:** @SuperNovae_st
**Posting:** Tuesday or Wednesday, 9:00 AM PST / 6:00 PM CET
**Format:** 12-tweet thread

---

## Tweet 1 — Hook (stop the scroll)

> LangChain: 200 lines of Python.
> Nika: 10 lines of YAML.
> Same result. 5x less RAM.
>
> We just open-sourced the Ansible for AI. 🧵

**Chars:** 131
[IMAGE 1 — see "Image Generation Prompts" section below]

---

## Tweet 2 — The problem

> You copy from ChatGPT, paste somewhere, ask it again, copy again.
>
> Now imagine writing those steps ONCE in a text file.
> It runs all of them. In parallel. For 50 articles.
>
> That's Nika.

**Chars:** 199
[IMAGE 2 — see "Image Generation Prompts" section below]

---

## Tweet 3 — Show the code

> Scrape Hacker News. Summarize with AI. Eight lines.
>
> That's a real workflow. Not pseudocode.
>
> (attached: code screenshot)

**Chars:** ~90 (text only — the code is rendered as an image attachment)

**Screenshot layout:** Dark terminal background (#0f172a). Top half: syntax-highlighted YAML of the `hn-digest` workflow (8 lines). Bottom half: simulated output showing "Fetching... Done. Summarizing... Done." with green checkmarks. Font: JetBrains Mono or SF Mono, 14pt. Window chrome: minimal macOS-style rounded corners, no title bar buttons. The YAML keywords (`name:`, `fetch:`, `infer:`, `with:`) highlighted in electric blue (#3b82f6), string values in white (#f8fafc), comments in muted grey.

[IMAGE 3 — see "Image Generation Prompts" and "Image Alternatives" sections below]

---

## Tweet 4 — Why not Python?

> "Why not just write a Python script?"
>
> Manual ChatGPT:
> — 47 clicks, 12 min, one article at a time
>
> Python script:
> — 200 lines, pip install hell, breaks on update
>
> Nika:
> — 10 lines YAML, single binary, runs forever
>
> Pick one.

**Chars:** 236
[IMAGE 4 — see "Image Generation Prompts" section below]

---

## Tweet 5 — Benchmarks

> We benchmarked against everything.
>
> Nika (Rig/Rust): 1.0 GB RAM, 4ms cold start
> LangChain: 5.7 GB RAM, 62ms cold start
> LangGraph: 5.5 GB RAM, 10,155ms cold start
> CrewAI: excluded — 44% failure rate
>
> Numbers don't lie.

**Chars:** 229
[IMAGE 5 — see "Image Generation Prompts" section below]

---

## Tweet 6 — Multi-provider freedom

> Change ONE WORD. Your workflow runs on a different AI.
>
> provider: claude
> provider: openai
> provider: mistral
> provider: groq
> provider: gemini
>
> 22 providers. No code changes. No vendor lock-in.
> Your choice.

**Chars:** 209
[IMAGE 6 — see "Image Generation Prompts" section below]

---

## Tweet 7 — The 5 verbs

> The entire API is 5 words:
>
> infer — ask AI
> exec — run a command
> fetch — get a webpage
> invoke — call a tool
> agent — autonomous loop
>
> Five verbs. Any automation. That's the whole thing.

**Chars:** 192
[IMAGE 7 — see "Image Generation Prompts" section below]

---

## Tweet 8 — Built-in course

> We didn't write docs. We built a course INTO the tool.
>
> nika init --course
>
> 12 levels. 44 exercises. From "hello world" to autonomous agents.
>
> No tutorials to google. No videos to watch.
> Open your terminal and learn by doing.

**Chars:** 230
[IMAGE 8 — see "Image Alternatives" section below (real screenshot preferred)]

---

## Tweet 9 — The manifesto

> AI is the new electricity. Six labs decide who gets the switch.
>
> Nika is AGPL open source. Runs on your machine. Works with any provider.
>
> Your automations. Your data. Your rules.
>
> We ship in Rust because freedom should be fast.

**Chars:** 232
[IMAGE 9 — see "Image Generation Prompts" section below]

---

## Tweet 10 — Media pipeline

> It also has a built-in media pipeline.
>
> Import, resize, optimize, extract metadata, generate alt-text with vision AI.
>
> 24 tools. Content-addressable storage. Zero external dependencies.
>
> One binary does it all.

**Chars:** 219
[IMAGE 10 — see "Image Generation Prompts" section below]

---

## Tweet 11 — AI-native DX

> Every AI coding tool already understands Nika.
>
> Claude Code, Cursor, Copilot, Windsurf — 43 tool configs generated in one command:
>
> nika setup
>
> Your AI assistant writes Nika workflows for you.
> Recursive AI. We're here.

**Chars:** 233
[IMAGE 11 — see "Image Alternatives" section below (real screenshot preferred)]

---

## Tweet 12 — CTA

> Star us: github.com/supernovae-st/nika
> Install: brew install supernovae-st/tap/nika
> Learn: nika init --course
>
> Liberate your AI. 🦋
>
> #OpenSource #AI #Rust #BuildInPublic

**Chars:** 186
[IMAGE 12 — see "Image Generation Prompts" section below]

---

# Alternative Hooks (A/B Testing)

## Hook A — Frustration angle

> Every AI framework wants you to learn THEIR Python SDK.
>
> What if the entire interface was a text file?
>
> 10 lines of YAML. Any AI provider. Single binary. No dependencies.
>
> Meet Nika. 🧵

**Chars:** 197

## Hook B — Speed angle

> Cold start benchmarks:
>
> Nika: 4ms
> LangChain: 62ms
> LangGraph: 10,155ms
>
> One of these is written in Rust.
> One of these is open source today. 🧵

**Chars:** 163

## Hook C — Simplicity angle

> Your entire AI automation stack, explained in 30 seconds:
>
> 1. Write a YAML file
> 2. nika run file.nika.yaml
> 3. There is no step 3
>
> We just open-sourced a 10 MB binary that replaces LangChain, LangGraph, and CrewAI. 🧵

**Chars:** 226

---

# Posting Strategy

**Day:** Tuesday or Wednesday
**Time:** 9:00 AM PST / 6:00 PM CET (peak US West + Europe evening overlap)
**Hashtags:** Only on final tweet — #OpenSource #AI #Rust #BuildInPublic
**Thread label:** Use 🧵 on tweet 1 only

## Engagement tips

- Reply to tweet 1 with tweet 3 (the code) — it acts as a visual anchor
- Quote-tweet the benchmarks (tweet 5) separately 24h later for second wave
- Pin tweet 1 to profile before posting
- Prepare a reply for "but YAML sucks" — lean into it: "YAML is readable by humans AND machines. That's the point."
- Prepare a reply for "Rust is overkill" — "1 GB vs 5.7 GB RAM. 4ms vs 10 seconds. You tell me."
