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

---

# Image Generation Prompts

Detailed prompts for DALL-E 3 or Midjourney. Brand colors: dark navy `#0f172a`, electric blue `#3b82f6`, white `#f8fafc`.

## IMAGE 1 — Hero: Code Comparison (Tweet 1)

**Aspect ratio:** 16:9
**Style:** Minimal tech marketing, Vercel/Linear aesthetic, clean and modern

**Prompt:**
> Split-screen image on a dark navy (#0f172a) background. Left side labeled "LangChain" in muted grey at the top: a dense wall of Python code, blurred and overwhelming, ~40 lines of tiny monospace text with import statements and class definitions, tinted slightly red/orange to feel chaotic. Right side labeled "Nika" in electric blue (#3b82f6) at the top: exactly 10 lines of clean YAML code, large readable monospace font, with keywords highlighted in electric blue and values in white (#f8fafc). The right side has generous whitespace and breathing room. A thin vertical divider line in electric blue separates the two halves. Bottom center: the text "Same result. 5x less RAM." in white, 16pt sans-serif. No logos, no decorations. Flat design, no shadows or gradients. Aspect ratio 16:9.

## IMAGE 2 — Pipeline Diagram (Tweet 2)

**Aspect ratio:** 16:9
**Style:** Minimal diagram, dark background, thin line art

**Prompt:**
> Dark navy (#0f172a) background. Top half labeled "Before" in muted grey: a chaotic loop diagram showing a stick figure at a computer, arrows going back and forth between a browser (labeled "ChatGPT"), a text editor, and a clipboard icon. The arrows are tangled, messy, and form a confusing web. Red "x12 per article" annotation. Bottom half labeled "After" in electric blue (#3b82f6): a single clean horizontal flow. A small YAML file icon on the left, a right-pointing arrow in electric blue, a gear icon labeled "Nika", another arrow, and a stack of 50 document icons on the right labeled "50 articles, done." Clean sans-serif font. Thin lines, no fills, minimal flat design. Aspect ratio 16:9.

## IMAGE 3 — Code Screenshot (Tweet 3)

**Aspect ratio:** 1:1
**Style:** Terminal screenshot aesthetic, code-focused

**Prompt:**
> Square image. A macOS-style terminal window with rounded corners, no window control buttons, floating on a dark navy (#0f172a) background with a very subtle radial gradient (slightly lighter at center). The terminal has a slightly lighter dark background (#1e293b). Top section: 8 lines of YAML code in monospace font (JetBrains Mono style). Keywords like "name:", "tasks:", "fetch:", "infer:", "with:" in electric blue (#3b82f6). String values in white (#f8fafc). Structure indentation characters in muted grey. Below the code, a thin horizontal separator line, then 3 lines of simulated output: green checkmark + "Fetching news.ycombinator.com... done", green checkmark + "Summarizing with claude... done", green checkmark + "Output saved to hn-digest.md". The green is a soft terminal green (#4ade80). Aspect ratio 1:1.

## IMAGE 4 — Comparison Table (Tweet 4)

**Aspect ratio:** 1:1
**Style:** Clean data visualization, infographic

**Prompt:**
> Square image on dark navy (#0f172a) background. Three columns side by side, each a rounded rectangle card with a slightly lighter background (#1e293b). Left card header: "Manual ChatGPT" in muted grey. Middle card header: "Python Script" in muted grey. Right card header: "Nika" in electric blue (#3b82f6) with a subtle blue glow behind the card. Each card contains 3 rows of stats, left-aligned with small icons. Row 1 (effort): left "47 clicks", middle "200 lines", right "10 lines" in electric blue. Row 2 (time): left "12 min/article", middle "setup hell", right "instant". Row 3 (durability): left "manual every time", middle "breaks on update", right "runs forever" in electric blue. The Nika column values are in white/blue, the other columns in muted grey. Clean sans-serif font. No decorations. Aspect ratio 1:1.

## IMAGE 5 — Benchmark Chart (Tweet 5)

**Aspect ratio:** 16:9
**Style:** Data visualization, clean bar chart

**Prompt:**
> Dark navy (#0f172a) background. Two grouped horizontal bar charts stacked vertically. Top chart labeled "RAM Usage" in white: four horizontal bars. "Nika" bar is short and electric blue (#3b82f6) labeled "1.0 GB". "LangChain" bar is very long in muted grey (#475569) labeled "5.7 GB". "LangGraph" bar is long in muted grey labeled "5.5 GB". "CrewAI" bar has a red strikethrough with "44% failure rate" annotation. Bottom chart labeled "Cold Start" in white: "Nika" bar is a tiny sliver in electric blue labeled "4ms". "LangChain" bar is medium in grey labeled "62ms". "LangGraph" bar extends far right in grey labeled "10,155ms" (this bar should be dramatically longer). Clean sans-serif font, left-aligned labels. No grid lines, no axes, just the bars and labels. Aspect ratio 16:9.

## IMAGE 6 — Provider Grid (Tweet 6)

**Aspect ratio:** 1:1
**Style:** Connection diagram, node graph aesthetic

**Prompt:**
> Square image on dark navy (#0f172a) background. Center: a small YAML file icon with a butterfly silhouette watermark, glowing faintly in electric blue (#3b82f6). Around it in a circular arrangement: 8 rounded square tiles representing AI providers. Each tile is a slightly lighter dark (#1e293b) with the provider name in white text inside: "Claude", "OpenAI", "Mistral", "Groq", "Gemini", "xAI", "DeepSeek", "Ollama". Thin electric blue lines connect each tile to the center YAML icon, like spokes of a wheel. Below the circle: the text "22 providers. One line change." in white sans-serif. The lines have a subtle glow effect. Minimal, flat design. Aspect ratio 1:1.

## IMAGE 7 — Five Verbs Typography (Tweet 7)

**Aspect ratio:** 1:1
**Style:** Typography poster, minimal iconography

**Prompt:**
> Square image on dark navy (#0f172a) background. Five rows, vertically centered, each with generous spacing. Each row contains: a small minimal line icon on the left in electric blue (#3b82f6), the verb in large bold monospace font in white (#f8fafc), and a short description in smaller muted grey text on the right. Row 1: brain/sparkle icon, "infer:", "ask AI". Row 2: terminal/bracket icon, "exec:", "run a command". Row 3: globe/arrow icon, "fetch:", "get a webpage". Row 4: plug/socket icon, "invoke:", "call a tool". Row 5: loop/infinity icon, "agent:", "autonomous loop". The verbs are in a larger font size than the descriptions. All text left-aligned. At the bottom, small text: "Five verbs. Any automation." in electric blue. Clean, no decorations, pure typography. Aspect ratio 1:1.

## IMAGE 9 — Manifesto Quote (Tweet 9)

**Aspect ratio:** 1:1
**Style:** Quote card, stark and powerful

**Prompt:**
> Square image on a solid dark navy (#0f172a) background. Large centered text in white (#f8fafc) sans-serif font, bold weight: "Your automations. Your data. Your rules." Each sentence on its own line, centered. Above the quote, a small butterfly silhouette in electric blue (#3b82f6), about 40px, delicate and minimal. Below the quote, a thin horizontal line in electric blue, and beneath it in smaller muted grey text: "AGPL open source. Runs on your machine. Works with any provider." At the very bottom, tiny text: "nika" in electric blue monospace. No other decorations. The image should feel like a manifesto poster. Aspect ratio 1:1.

## IMAGE 10 — Media Pipeline Flow (Tweet 10)

**Aspect ratio:** 16:9
**Style:** Flow diagram, processing pipeline

**Prompt:**
> Dark navy (#0f172a) background. A horizontal pipeline flow from left to right. On the far left: a photo icon (representing an input image) with a small label "photo.jpg" in white. Five right-pointing arrows in electric blue (#3b82f6) branch out from it to five output nodes arranged in a fan pattern on the right side. Each output node is a rounded rectangle (#1e293b) with an icon and label: "thumbnail" (small image icon), "metadata" (tag icon), "optimized" (compress icon), "alt-text" (AI sparkle icon), "provenance" (shield/checkmark icon). Above the pipeline: "24 tools. Zero dependencies." in white sans-serif. Below: "Content-addressable storage" in muted grey, smaller font. All connections are thin electric blue lines with small arrow tips. Clean, flat, minimal diagram style. Aspect ratio 16:9.

## IMAGE 12 — GitHub Card (Tweet 12)

**Aspect ratio:** 16:9
**Style:** Social card, call-to-action

**Prompt:**
> Dark navy (#0f172a) background. Center: a large rounded rectangle card (#1e293b) with padding. Inside the card, top line: a small butterfly icon in electric blue (#3b82f6) next to "supernovae-st/nika" in white bold sans-serif. Second line: "Semantic YAML workflow engine for AI tasks" in muted grey. Third line: small icons with counts in white: star icon "0" (placeholder), fork icon "0", license icon "AGPL-3.0". Below the card: a terminal-style code block with a dark background showing: "$ brew install supernovae-st/tap/nika" in monospace, the command highlighted in electric blue. Below that: "$ nika init --course" in white monospace. At the very bottom: "Liberate your AI." in white with a small butterfly emoji. Aspect ratio 16:9.

---

# Image Alternatives

For tweets where a real terminal screenshot is more authentic and effective than a generated image. Use a terminal with dark theme (background close to #0f172a), JetBrains Mono or SF Mono font.

## IMAGE 3 — Code Screenshot (Tweet 3)

A real screenshot works better than AI-generated for code credibility.

**Option A: Terminal recording**
```bash
# 1. Create the example workflow
cat > /tmp/hn-digest.nika.yaml << 'EOF'
name: hn-digest
tasks:
  - id: scrape
    fetch:
      url: https://news.ycombinator.com
      extract: article
  - id: summarize
    infer:
      prompt: "Summarize: {{with.page}}"
    with: { page: $scrape }
EOF

# 2. Run `nika check` to show validation (safe, no API calls)
nika check /tmp/hn-digest.nika.yaml
```

**Option B: VS Code screenshot**
Open the YAML in VS Code with a dark theme (One Dark Pro or GitHub Dark). Take a screenshot cropped tightly to the editor pane. Use the YAML language mode for syntax highlighting. Overlay simulated terminal output below using a design tool.

## IMAGE 8 — Course Output (Tweet 8)

A real screenshot is far more convincing than a generated image here.

```bash
# Option 1: Show the course status constellation map
# First init a course if needed:
mkdir -p /tmp/nika-course-demo && cd /tmp/nika-course-demo
nika init --course

# Then capture the status:
nika course status

# Option 2: Show course info for a specific level
nika course info 1
```

**Screenshot tips:** Resize terminal to ~100 columns wide. Make sure at least 3-4 levels are visible in the constellation map. If possible, mark 1-2 exercises as completed first so the progress feels real. Use a terminal with dark background.

## IMAGE 11 — Setup Output (Tweet 11)

Real `nika setup` output showing detected AI tools is the strongest proof.

```bash
# Run in a directory where Claude Code / Cursor configs exist:
cd ~/dev/supernovae/nika
nika setup

# Or for a clean demo:
mkdir -p /tmp/nika-setup-demo && cd /tmp/nika-setup-demo
nika setup
```

**Screenshot tips:** Make sure the output shows multiple detected tools (Claude Code, Cursor, etc.). Crop to show just the tool detection and config generation lines. Terminal width ~100 columns, dark background.

## IMAGE 5 — Benchmark Chart (Tweet 5)

As an alternative to a generated image, create a real chart:

```bash
# Use nika's built-in chart tool in a workflow:
# Create a workflow that generates the benchmark bar chart
nika run benchmark-chart.nika.yaml
# (where the workflow uses invoke: nika:chart with the benchmark data)
```

Or use any charting tool (Datawrapper, Observable Plot) with the exact numbers from the tweet, exported as a dark-themed PNG.

---

# Thread Visuals Checklist

- [ ] Hero banner (tweet 1) — code comparison side-by-side, 16:9, generated
- [ ] Pipeline diagram (tweet 2) — manual vs automated flow, 16:9, generated
- [ ] Code screenshot (tweet 3) — terminal with syntax highlighting + output, 1:1, real screenshot preferred
- [ ] Comparison table (tweet 4) — 3-column clean card design, 1:1, generated
- [ ] Benchmark chart (tweet 5) — horizontal bar chart, green vs grey, 16:9, generated or real chart
- [ ] Provider grid (tweet 6) — logos in circle connected to YAML file, 1:1, generated
- [ ] 5 verbs typography (tweet 7) — clean icons + monospace verbs, 1:1, generated
- [ ] Course screenshot (tweet 8) — actual `nika course status` output, 1:1, real screenshot
- [ ] Manifesto quote (tweet 9) — text on dark background with butterfly, 1:1, generated
- [ ] Pipeline diagram (tweet 10) — image processing flow, fan-out, 16:9, generated
- [ ] Setup terminal (tweet 11) — `nika setup` output showing detected tools, 1:1, real screenshot
- [ ] GitHub card (tweet 12) — repo card + brew install command, 16:9, generated
