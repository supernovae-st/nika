# Nika Video Demo Scripts

Three formats, one message: **Automate AI. No code required.**

---

## 1. 30-Second Teaser (Twitter/X — Silent, Subtitled)

**Format:** 1080x1080 or 9:16 vertical, no audio, large subtitles, dark terminal aesthetic.

| Time | Frame | Visual | Subtitle |
|------|-------|--------|----------|
| 0-5s | 1 | Screen recording: frantically switching between ChatGPT tabs, copy-pasting text, waiting for responses. Cursor moves chaotically. | **"You copy-paste between ChatGPT tabs."** |
| 5-10s | 2 | Smooth transition: a clean `.nika.yaml` file appears in a code editor. Camera slowly zooms in on the 10 lines. Syntax highlighting glows. | **"What if you could write the steps once?"** |
| 10-20s | 3 | Terminal: `nika run recipe.nika.yaml`. The TUI lights up — tasks execute in parallel, progress bars fill simultaneously, green checkmarks cascade. | **"And run them all. In parallel."** |
| 20-25s | 4 | Split screen: left shows the 10-line YAML, right shows the structured JSON output. Provider logos flash: Claude, GPT, Gemini, Mistral. | **"10 lines. Any AI. Open source."** |
| 25-30s | 5 | Black screen. Nika butterfly logo fades in. GitHub URL below. Stars counter animating up. | **"Nika. Automate AI. No code required."** |

**Production notes:**
- Use a real workflow, not a mock — authenticity matters
- Terminal font: JetBrains Mono or Berkeley Mono
- Color scheme: match Nika TUI theme (dark background, accent colors)
- End card: GitHub URL + `brew install supernovae/tap/nika`
- Music: none (silent format), but design for rhythm — cuts on beats

---

## 2. Two-Minute Demo (YouTube / Product Hunt — Narrated)

**Format:** 16:9, 1080p minimum. Screen recording with voiceover. No face cam needed (optional).
**Tone:** Casual, confident. Like a dev showing a friend something cool at a coffee shop.

### 0:00-0:15 — The Hook

[SCREEN] Quick montage: LangChain boilerplate code scrolling, pip install commands, dependency errors, Python tracebacks.

[NARRATION] "Every AI framework wants you to write code. Set up environments. Install dependencies. Import twelve modules just to summarize a webpage. What if you didn't have to?"

### 0:15-0:30 — The Install

[SCREEN] Clean terminal. Type: `brew install supernovae/tap/nika`. Installation completes in 3 seconds. Type: `nika --version`. Shows version.

[NARRATION] "Nika is a single binary. One command to install. No Python, no Node, no Docker. It's written in Rust and it's about 15 megabytes."

### 0:30-1:00 — Write a Workflow

[SCREEN] Open a new file: `demo.nika.yaml`. Type the workflow live (or use a sped-up recording). Show a 3-task recipe: fetch a webpage, summarize it with Claude, translate to French.

[NARRATION] "Here's what a Nika workflow looks like. Three tasks. Fetch a webpage — that's the `fetch:` verb. Summarize it — that's `infer:` with Claude. Translate to French — another `infer:`, this time with GPT. Each task says what it needs from the previous one. That's it. No imports, no classes, no boilerplate."

```yaml
nika: workflow@0.12
name: demo
tasks:
  scrape:
    fetch: https://example.com/article
    extract: article

  summarize:
    model: claude/claude-sonnet-4-20250514
    infer: "Summarize this article in 3 bullet points: {{with.page}}"
    with:
      page: $scrape

  translate:
    model: openai/gpt-4o
    infer: "Translate to French: {{with.summary}}"
    with:
      summary: $summarize
```

### 1:00-1:20 — Run It

[SCREEN] Terminal: `nika run demo.nika.yaml`. Tasks execute. Output streams in real-time. Green checkmarks appear. Final output displayed.

[NARRATION] "Now we run it. Nika builds a dependency graph, figures out what can run in parallel, and executes everything. You see each task complete in real-time. And there's our French summary."

### 1:20-1:35 — The TUI

[SCREEN] Type `nika ui`. The TUI opens — Studio view with workflow list, then switch to Command view. Navigate between views with keyboard shortcuts.

[NARRATION] "Nika also has a terminal UI. You can browse your workflows, run them, inspect outputs, see execution traces — all without leaving the terminal. Three views: Studio for browsing, Command for execution, Control for settings."

### 1:35-1:50 — The Course

[SCREEN] Type `nika init --course`. Show the constellation map with `nika course status`. Open one exercise. Run it.

[NARRATION] "And if you're new to this, there's a built-in interactive course. Twelve levels, forty-four exercises. It teaches you everything from basic inference to building autonomous agents. All inside your terminal."

### 1:50-2:00 — The Close

[SCREEN] GitHub page. Star count. The README. Then fade to Nika logo + butterfly.

[NARRATION] "Nika is open source, AGPL licensed, and free forever. Five verbs, any AI provider, one binary. Link in the description. Star us on GitHub if this is useful to you."

[SCREEN] End card:
- GitHub: `github.com/supernovae-st/nika`
- Install: `brew install supernovae/tap/nika`
- Website: `qrcode-ai.com`

---

## 3. Five-Minute Deep Dive (Dev Audience)

**Format:** 16:9, 1080p. Screen recording with voiceover. Optional face cam in corner.
**Tone:** Technical but approachable. Conference talk energy.
**Target:** r/rust, Hacker News, dev YouTube channels.

### Section 1: Architecture Overview (0:00-0:30)

[SCREEN] Animated architecture diagram (can be ASCII or simple motion graphics):

```
YAML File
   |
   v
Parser (Raw AST)
   |
   v
Analyzer (Validated AST)
   |
   v
Lower (Runtime Types)
   |
   v
DAG Builder (dependency graph, cycle detection)
   |
   v
Executor (parallel, per-task timeout, retry)
   |
   v
Provider Router (Claude, GPT, Gemini, Mistral, local GGUF...)
```

[NARRATION] Cover: three-phase AST pipeline, DAG-based execution, provider abstraction. Emphasize: no runtime reflection, everything validated before execution.

**Key points:**
- Three-phase AST: Raw, Analyzed, Lowered — catches errors before execution
- DAG scheduler: automatic parallelism, cycle detection, dependency resolution
- Provider router: single interface to 22+ providers
- Single Rust binary: no garbage collector, no runtime overhead

### Section 2: Live Coding a 3-Task Recipe (0:30-2:00)

[SCREEN] Code editor + terminal split. Build from scratch, explaining each line.

**The recipe:** Fetch a Hacker News thread, extract top comments, generate a summary with sentiment analysis, output as structured JSON.

[NARRATION] Walk through:
1. The `nika: workflow@0.12` header and schema versioning
2. The `fetch:` verb with `extract: text` and `selector:` for CSS targeting
3. The `infer:` verb with `output:` for structured JSON schema
4. Data flow with `with:` bindings and `{{template}}` syntax
5. The `depends_on:` relationship (implicit via `with:`)

**Show:** Run the workflow. Inspect the JSON output. Point out that structured output was validated against the schema.

### Section 3: for_each Parallel Demo (2:00-3:00)

[SCREEN] Modify the workflow to process 5 URLs in parallel using `for_each:`.

```yaml
  analyze_all:
    for_each: $urls
    model: claude/claude-sonnet-4-20250514
    infer: "Analyze sentiment: {{item}}"
```

[NARRATION] Explain:
- `for_each:` fans out automatically — each item runs as a parallel sub-task
- Show the TUI execution view with 5 tasks running simultaneously
- Compare wall-clock time: sequential (25s) vs parallel (6s)
- Mention: `max_parallel:` for rate-limit-aware throttling

### Section 4: Agent with Guardrails (3:00-4:00)

[SCREEN] Show an agent workflow with tools and guardrails.

```yaml
  research:
    model: claude/claude-sonnet-4-20250514
    agent: "Research this topic and write a report"
    tools:
      - nika:read_file
      - nika:write_file
      - nika:glob
    guardrails:
      max_iterations: 10
      max_tool_calls: 25
      forbidden_patterns: ["rm -rf", "sudo"]
```

[NARRATION] Explain:
- `agent:` verb gives the LLM a tool loop — it can call tools, observe results, iterate
- Guardrails: iteration limits, tool call caps, forbidden command patterns
- Show the agent executing: tool calls logged in real-time, reasoning visible
- Point out: this is NOT autonomous — it's bounded, auditable, reproducible

### Section 5: Benchmark Comparison (4:00-4:30)

[SCREEN] Show benchmark table (from AutoAgents or internal benchmarks):

| Metric | LangChain | CrewAI | Nika |
|--------|-----------|--------|------|
| Install size | 380 MB | 220 MB | 15 MB |
| Cold start | 3.2s | 2.1s | 0.05s |
| Memory (idle) | 180 MB | 120 MB | 8 MB |
| Simple chain | 4.8s | 3.5s | 1.2s |
| Dependencies | 147 | 89 | 0 |

[NARRATION] "These numbers aren't a gotcha — Python frameworks do more things. But if all you need is reliable AI task execution, you're paying a 20x overhead tax for features you'll never use. Nika does one thing and does it fast."

### Section 6: Community + Manifesto (4:30-5:00)

[SCREEN] Show the manifesto excerpt. Show the GitHub contributors page. Show the course constellation map.

[NARRATION] "Nika is named after Nika, the Sun God from One Piece — the one who liberates. We believe AI should be like electricity: a utility, not a luxury. That's why Nika is AGPL licensed — not MIT. If you build on it, you contribute back. That's the deal. Star us on GitHub, try the course, build something. The age of accessible AI starts with tools that anyone can use."

[SCREEN] Final card with GitHub link, install command, and butterfly logo.

---

## Production Checklist

- [ ] Record all terminal sessions with real workflows (no fakes)
- [ ] Use consistent terminal theme across all videos
- [ ] Ensure all provider API calls succeed (pre-test before recording)
- [ ] Add captions/subtitles to all formats (accessibility)
- [ ] Include install command in every video description
- [ ] Thumbnail: terminal screenshot with butterfly logo overlay
- [ ] Upload 30s teaser natively to Twitter (not as YouTube link)
- [ ] Product Hunt launch: use 2-minute demo as primary video
- [ ] YouTube: 5-minute deep dive with chapters in description
