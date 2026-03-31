# Desktop Automation Agents & OS-Level Control: 2025-2026 Landscape

> Research date: 2026-04-01
> Methodology: 8 Perplexity searches, cross-referenced across sources
> Confidence: HIGH for architecture patterns and major projects, MEDIUM for benchmark numbers (evolving rapidly)

## Summary

The field of AI agents that control desktop applications, browsers, and local software has exploded
in 2025-2026. Five distinct architectural patterns have emerged: pure vision (screenshot-based),
accessibility tree/API, hybrid vision+tree, set-of-marks prompting, and MCP tool integration.
None achieve more than ~35% success on the OSWorld benchmark for complex multi-step desktop tasks,
but the technology is advancing rapidly. The landscape spans from proprietary solutions (Anthropic
Computer Use, OpenAI Operator) to open-source frameworks (trycua/cua, UI-TARS, Agent-S) and
infrastructure projects (screenpipe, browser-use).

---

## 1. Architectural Patterns

### 1.1 Pure Vision (Screenshot + Coordinate Prediction)

**How it works**: Agent captures a screenshot, sends it to a vision-language model (VLM), the
model predicts pixel coordinates for clicks/actions, executes via OS-level input simulation,
then captures another screenshot to verify.

**Loop**: screenshot -> VLM analysis -> coordinate prediction -> action execution -> verify -> repeat

**Pros**: Works on ANY application without integration. OS-agnostic. No app cooperation needed.
**Cons**: Slow (full image encode per step). High token cost. ~20-30% coordinate error rate.
Fails on animations, overlapping elements, dynamic UIs. Resolution-sensitive.

**Projects using this**: Anthropic Computer Use, OpenAI Operator/CUA, early UFO agents

**OSWorld benchmark**: ~17% success rate (pure vision baselines)

### 1.2 Accessibility Tree/API Approach

**How it works**: Agent reads the OS accessibility tree (a structured, DOM-like representation
of all UI elements with labels, roles, and actions). Plans actions against this tree. Executes
via accessibility APIs.

**APIs per platform**:
- **macOS**: NSAccessibility / AXUIElement API
- **Windows**: UI Automation (UIA) / MSAA
- **Linux**: AT-SPI2 (via D-Bus)

**Pros**: Fastest approach (text parsing, no image encoding). Lowest cost. High accuracy on
supported apps (95%+ element identification). Precise element targeting by name/role.
**Cons**: Not all apps expose good accessibility trees. Custom UIs, Electron apps, and games
are poorly represented. Platform-specific code needed. No support for purely visual elements.

**Projects using this**: AppAgent, Windows Agent Arena baselines, FlaUI, pywinauto

**OSWorld benchmark**: ~23% success rate

### 1.3 Hybrid Vision + Accessibility

**How it works**: Uses accessibility tree when available (faster, cheaper, more reliable).
Falls back to vision when the tree is incomplete or missing. May fuse both signals.

**Pros**: Best of both worlds. Highest benchmark scores. Adapts to gaps in either approach.
**Cons**: Complex fusion logic. Needs both vision model AND accessibility integration.

**Projects using this**: SeeAct, AgentOS, MobileAgent, Agent-S

**OSWorld benchmark**: ~27.5% success rate (leading approach)

### 1.4 Set-of-Marks Prompting

**How it works**: Before sending a screenshot to the VLM, overlay numbered/labeled visual
markers on detected UI elements. The VLM then references marks by number instead of predicting
raw coordinates. Reduces coordinate hallucination significantly.

**Pros**: +15% accuracy over pure vision. Visual anchors persist across frames. Reduces
ambiguity for the model.
**Cons**: Extra processing step for mark rendering. Still vision-dependent. Overhead for
mark detection.

**Projects using this**: SeeAct, UFO-Mark, ScreenSpotter

### 1.5 MCP Tool Integration

**How it works**: Instead of direct screen manipulation, AI agents call structured tools
(screenshot capture, element finding, mouse/keyboard control) exposed via Model Context
Protocol (MCP) servers. Tools can wrap accessibility APIs, PyAutoGUI, or other backends.

**Pros**: Standardized, observable, cacheable. Reusable tool endpoints. Composable with
other MCP tools (filesystem, database, web). Lower per-action cost.
**Cons**: Nascent ecosystem. Requires MCP server setup. Tool quality varies.
Not all desktop interactions are easily tool-ified.

**Projects using this**: trycua/cua MCP server, Windows Desktop Control MCP, GUI Automation
for Windows MCP, Claude Desktop Extensions

**OSWorld benchmark**: ~25% (early results with MCP+Claude)

### Pattern Comparison Matrix

```
Pattern             | Accuracy | Speed    | Cost   | Reliability | Cross-OS
--------------------|----------|----------|--------|-------------|----------
Pure Vision         | Medium   | Slow     | High   | Low         | YES
Accessibility Tree  | High*    | Fastest  | Low    | Medium      | No (per-OS)
Hybrid              | Highest  | Fast     | Medium | High        | Partial
Set-of-Marks        | High     | Medium   | Med-Hi | High        | YES
MCP Tool            | Varies   | Fast     | Low-Med| High        | Via tools

* On supported apps only
```

---

## 2. Major Projects: Commercial

### 2.1 Anthropic Computer Use

- **Status**: Beta API, available on Claude 3.5 Sonnet and later models
- **Architecture**: Pure vision. Screenshot-analyze-plan-execute feedback loop.
- **How it works**:
  1. Captures high-res screenshot (recommended 1024x768 XGA)
  2. Claude vision detects elements, reads text, interprets UI hierarchy
  3. Model "counts pixels" from screen edges for exact (x,y) coordinates
  4. Executes mouse/keyboard actions via tools (computer, text_editor, bash)
  5. New screenshot captured for verification
  6. Loop repeats until task complete
- **Isolation**: Runs in local Docker container
- **Limitations**: Beta-stage resolution sensitivity. Higher res degrades accuracy. No built-in
  sensitive data protection. Struggles with dynamic UIs and small elements.
- **Reference impl**: `anthropic/anthropic-quickstarts` (computer-use-demo)
- **URL**: https://docs.anthropic.com/en/docs/agents-and-tools/computer-use

### 2.2 OpenAI Operator / CUA (Computer Using Agent)

- **Status**: Limited research preview / ChatGPT Pro feature (as of early 2026)
- **Architecture**: Vision-based, primarily browser-focused
- **How it works**: GPT-4o-powered. Observes screen as image, predicts actions
  (clicks, typing via coordinates), executes via browser control. Uses RLHF for
  action accuracy. Reports progress in natural language chat interface.
- **Benchmark**: ~32.6% success on OSWorld 50-step tasks
- **Limitations**: Primarily web/browser. Not yet general desktop. High compute cost.
  Safety guardrails constrain capabilities.
- **URL**: https://openai.com/operator (product), API in ChatGPT Responses API

### 2.3 Google Project Mariner / Gemini Computer Use

- **Status**: Generally available in Gemini Advanced (Q1 2026)
- **Architecture**: Hybrid vision + structured action space
- **How it works**: Screen diffing (pixel changes), HTML parsing, low-level controls.
  "Reason-act" loop with trajectory optimization. Model outputs JSON actions
  (e.g., `{"action": "click", "x": 420, "y": 300}`). Trained on synthetic UI
  trajectories for robustness to layout shifts.
- **Benchmark**: ~85% success on WebArena (web tasks)
- **Focus**: Chrome/web-first, expanding to desktop

### 2.4 Apple Intelligence App Control

- **Status**: iOS 19+ / macOS 16+ (rolling out 2026)
- **Architecture**: Semantic intents (NOT pixel-level automation)
- **How it works**: Developers expose structured APIs via App Intents framework.
  Siri parses natural language -> matches to intents -> executes on-device via
  Swift concurrency. Visual Intelligence uses on-device Foundation Models (3B params)
  for grounding. Controls apps via private Accessibility APIs + IntentKit for chaining.
- **Key difference**: No pixel-level mouse/keyboard. Relies on semantic intents
  (~90% coverage of common tasks). Falls back to cloud for rare cases.
- **Privacy**: On-device processing, no cloud dependency for most operations.

---

## 3. Major Projects: Open Source

### 3.1 trycua/cua -- The Computer Use Agent Platform

- **GitHub**: https://github.com/trycua/cua
- **Stars**: ~13.3k
- **Status**: Active (YC X25 company), v0.1.7+ MCP server
- **Architecture**: Full-stack CUA infrastructure
- **Components**:
  - **Loom CLI**: High-performance macOS VMs via Apple Virtualization Framework
  - **Computer SDK**: Screenshots, mouse/keyboard, shell, file I/O, Playwright
  - **Agent SDK**: Observe-reason-act loops, trajectory logging, budget limits
  - **Sandboxes**: Cloud desktops, local headless VMs, VNC sessions
  - **MCP server** (cua-mcp-server): Integrates with Claude Desktop, Cursor
- **Supports**: macOS, Linux, Windows, Android
- **Key value**: Gives every agent a sandboxed cloud desktop. Built for Claude Code,
  Codex, and computer-use agents.

### 3.2 ByteDance UI-TARS / UI-TARS-desktop

- **GitHub**: https://github.com/bytedance/UI-TARS
- **GitHub**: https://github.com/bytedance/UI-TARS-desktop
- **Status**: Active, v0.2.x (cross-platform desktop app)
- **Architecture**: VLM (UI-TARS-34B) + skill library
- **How it works**: Encode screen -> predict action tokens (discrete + continuous
  coords) -> execute via OS accessibility APIs (WinAPI, AppleScript). Fine-tuned
  on 100k+ human demonstrations. Chain-of-action prompting for complex tasks.
- **Backbone**: Doubao-1.5-Pro vision model
- **Benchmark**: ~70%+ on OSWorld (claimed, needs verification)
- **Supports**: Windows, macOS, Linux, multi-monitor

### 3.3 simular-ai/Agent-S (Agent S2)

- **GitHub**: https://github.com/simular-ai/Agent-S
- **Architecture**: Hybrid vision + accessibility, open-source
- **Benchmark**: 34.5% on OSWorld (edging out OpenAI Operator)
- **Key feature**: Customizable via scripting for custom PC setups
- **Privacy**: Self-hostable

### 3.4 OpenInterpreter/open-interpreter

- **GitHub**: https://github.com/OpenInterpreter/open-interpreter
- **Stars**: ~50k+ (one of the most starred AI agent projects)
- **Architecture**: Hybrid -- code interpretation + vision (screenshots/UI analysis)
- **Status**: Active but pivoting. Original focus was natural-language code execution.
  Added "OS mode" for desktop interaction via screenshots.
- **How it works**: Natural language -> code generation -> local execution.
  Vision mode captures screenshots, analyzes with VLM, generates PyAutoGUI/
  AppleScript commands. Runs entirely on-device.
- **Limitations**: OS mode was experimental and fragile. Project has pivoted
  toward "01" (their hardware device) and lighter agent patterns.

### 3.5 OthersideAI/self-operating-computer

- **GitHub**: https://github.com/OthersideAI/self-operating-computer
- **Architecture**: Pure vision (screenshots + GPT-4V coordinate prediction)
- **Status**: Early demo/prototype, limited maintenance
- **How it works**: Takes screenshots -> sends to GPT-4V -> gets click coordinates
  -> executes via PyAutoGUI. Simple observe-act loop.
- **Limitations**: Proof of concept quality. No sophisticated planning or error
  recovery. Fragile on complex tasks.

### 3.6 OS-Copilot/OS-Copilot

- **GitHub**: https://github.com/OS-Copilot/OS-Copilot
- **Architecture**: Hybrid (accessibility APIs + vision + code execution)
- **Status**: Research project, aligned with Microsoft ecosystem
- **Key feature**: Fara-7B (7B parameter model) for on-device agent tinkering

### 3.7 mediar-ai/screenpipe

- **GitHub**: https://github.com/mediar-ai/screenpipe
- **Stars**: ~12k+
- **Architecture**: Continuous screen + audio capture infrastructure
- **How it works**:
  1. Continuously records screen content
  2. Extracts text via OCR (runs locally)
  3. Transcribes audio (Whisper-based)
  4. Streams structured data as context for AI agents
  5. Stores everything in local SQLite for retrieval
- **Key value**: Not an agent itself, but INFRASTRUCTURE for agents.
  Provides the "memory" and "perception" layer that other agents can consume.
  Think of it as "always-on context" for any AI agent.
- **Use case**: Paired with LLM agents for desktop understanding without
  requiring screenshot-per-action.

### 3.8 Skyvern (skyvern-ai/skyvern)

- **GitHub**: https://github.com/skyvern-ai/skyvern
- **License**: AGPL-3.0
- **Architecture**: Vision + LLM for browser automation (NOT desktop-level)
- **How it works**: Vision-based element identification (no CSS selectors).
  Multi-agent swarm for task decomposition. Playwright browser control underneath.
  Pluggable LLMs (OpenAI, Anthropic, etc.).
- **Key value**: AI layer on top of Playwright that handles layout changes
  gracefully. API-first architecture.
- **Status**: YC-backed, actively maintained, commercial cloud offering

### 3.9 browser-use/browser-use

- **GitHub**: https://github.com/browser-use/browser-use
- **Architecture**: Browser extension + "Smart DOM" technology
- **How it works**: Makes websites accessible for AI agents via vision + HTML
  extraction. Multi-tab management. LangChain integration.
  Turns browser into API endpoint via MCP server.
- **Status**: Active, popular in the LangChain ecosystem

---

## 4. CLI Tools for OS-Level GUI Automation

These are the low-level building blocks that AI agents use to actually execute actions.

### 4.1 macOS

| Tool | Type | Capabilities |
|------|------|-------------|
| **cliclick** | CLI binary | Mouse clicks, drags, key presses via CLI args (`cliclick c:100,100`) |
| **AppleScript/osascript** | Built-in | Script any app GUI (windows, menus, keystrokes). Most powerful on macOS. |
| **macOS Shortcuts** | Built-in | CLI invocation: `shortcuts run "MyShortcut"`. Chainable with AI. |
| **Hammerspoon** | Lua scripting | Deep macOS automation (windows, hotkeys, events) |

### 4.2 Linux

| Tool | Display | Capabilities |
|------|---------|-------------|
| **xdotool** | X11 only | Mouse, keyboard, window management (`xdotool click 1`) |
| **ydotool** | Wayland | Modern replacement via uinput. Needs root/socket setup. |
| **kdotool** | Both | Lightweight, libinput-based. Broader input support. |
| **xclip/xsel** | X11 | Clipboard automation |
| **wl-clipboard** | Wayland | Clipboard for Wayland |

### 4.3 Windows

| Tool | Capabilities |
|------|-------------|
| **PowerShell UIAutomation** | Full UI element tree access, clicks, text input. Built-in. |
| **wtype** | Keyboard string typing via SendInput API |
| **AutoIt** | Dedicated scripting language for Windows GUI (keystrokes, mouse, windows) |
| **pywinauto** | Python lib for Windows GUI automation (MS UIA + Win32API) |

### 4.4 Cross-Platform

| Tool | Language | Notes |
|------|----------|-------|
| **PyAutoGUI** | Python | Mouse, keyboard, screenshots. All 3 OSes. Wayland experimental. |
| **robotjs** | Node.js | Mouse, keyboard, screen. Cross-platform via native bindings. |
| **nut.js** | TypeScript | Desktop automation + image-finder plugins for element location |
| **SikuliX** | Java | OpenCV-based image recognition for GUI interaction |

### How AI Agents Use These

AI agents typically wrap these tools in subprocess calls:
```
Agent VLM -> "click at (100, 200)" -> subprocess.run(["cliclick", "c:100,200"])
Agent VLM -> "type hello" -> subprocess.run(["xdotool", "type", "hello"])
Agent tree -> find element "Save" -> accessibility API -> click
```

The agent loop is: perceive (screenshot or a11y tree) -> reason (VLM/LLM) -> act (CLI tool) -> verify.

---

## 5. MCP-Based Desktop Control

### 5.1 Available MCP Servers

| Server | Platform | Capabilities | GitHub/Source |
|--------|----------|-------------|---------------|
| **cua-mcp-server** | All | Full CUA (screenshots, mouse, keyboard, shell) | trycua/cua |
| **Windows Desktop Control** | Windows | UIAutomation + PyAutoGUI | Jeomon George |
| **GUI Automation for Windows** | Windows | Screenshots, element finding, input | lpmwfx |
| **Computer Control** | Cross | Mouse, keyboard, screenshots, OCR, windows | AB498 |
| **Desktop Commander** | macOS/Linux | Terminal, filesystem, process management | wonderwhy-er |
| **SwiftAutoGUI** | macOS | Mouse, clicks, scrolling via Swift | Nakaoka Rei |
| **PyAutoGUI MCP** | Cross | PyAutoGUI wrapper | He Tao |
| **Screenshot MCP** | Cross | Screen capture + analysis for AI context | m-mcp |

### 5.2 MCP vs Direct Vision Control

```
                   MCP Approach                    Vision Approach
                   ============                    ===============
Perception:        Structured tool calls           Raw screenshots
Action space:      Defined tool params             Free-form coordinates
Cost per action:   Low (text-only tools)           High (image encoding)
Reliability:       High (typed params)             Medium (coord errors)
Generality:        Limited to exposed tools        Works on anything visual
Setup:             MCP server installation          Docker/API setup
Composability:     YES (chain with other MCP)      No (standalone loop)
Debugging:         Tool call logs                   Screenshot sequences
```

### 5.3 MCP Desktop Control Architecture

```
Claude/Agent                MCP Server              OS
==========                  ==========              ==
"Click Save" ──────────>    find_element("Save") ─> Accessibility API
                            screenshot() ──────────> Screen capture
"Type hello" ──────────>    type_text("hello") ───> Input simulation
"Open Terminal" ──────────> launch_app("Terminal") > AppleScript/WinAPI
```

The MCP approach is interesting because it bridges the gap between pure CLI tool usage and
full vision-based agents. An MCP server can internally use accessibility APIs for fast,
reliable element finding, fall back to screenshot+OCR when needed, and expose a clean
tool interface to the agent. The agent never needs to deal with raw pixels.

---

## 6. Claude Code: How It Controls Terminals and Local Apps

Claude Code is NOT a computer-use / vision agent. It is fundamentally different.

### Architecture

- **Runtime**: Node.js process running in user's local terminal
- **UI**: React-based virtual DOM with yoga-layout for terminal rendering
- **Execution**: Direct shell command execution via child processes
- **File access**: Direct filesystem read/write, scoped by settings.json
- **Permission model**: Hierarchical settings (user > project > local)
  with pre-execution static analysis

### How It Works

1. User provides natural language instruction
2. Claude analyzes project context (files, CLAUDE.md, git state)
3. Plans multi-file changes
4. Executes shell commands (npm, cargo, git, etc.) via subprocess
5. Captures stdout/stderr for feedback and self-correction
6. Iterates on errors (exit code != 0 triggers retry logic)
7. Manages context window to avoid token overflow

### Key Differences from Computer Use

| Aspect | Claude Code | Computer Use |
|--------|-------------|--------------|
| Input | Text (files, terminal output) | Screenshots (pixels) |
| Actions | Shell commands, file edits | Mouse clicks, keyboard typing |
| Scope | Developer workflows | Any desktop application |
| Speed | Fast (text-only) | Slow (image encoding per step) |
| Cost | Low (no vision tokens) | High (vision model per screenshot) |
| Reliability | High (structured commands) | Medium (coordinate errors) |
| Extensibility | MCP servers, hooks, slash commands | Docker-contained |

### MCP Extension

Claude Code and Claude Desktop support MCP servers for extending capabilities.
This is how desktop control gets added -- not natively, but via MCP servers like
cua-mcp-server or Desktop Commander that expose GUI automation tools.

---

## 7. Performance and Reliability in Practice

### Benchmark: OSWorld (927 cross-OS desktop tasks)

```
Approach                              Success Rate
==============================================
Hybrid (SeeAct, Agent-S)             27-35%
MCP-integrated (Claude + MCP tools)  ~25%
Accessibility-only baselines         ~23%
OpenAI Operator (vision + browser)   ~32.6%
Pure vision (GPT-4o baselines)       ~17%
```

### Real-World Reliability Issues

1. **Dynamic UIs**: Animations, loading spinners, and transitions break coordinate-based
   approaches. The screen changes between screenshot and action execution.

2. **Resolution sensitivity**: Anthropic recommends 1024x768. Higher resolutions
   degrade accuracy for vision models. But modern apps are designed for HiDPI.

3. **Multi-step fragility**: Each step has ~70-80% success. Over 10 steps,
   cumulative success drops to ~10-20%. Error recovery is the hard problem.

4. **Cost**: Vision-based approaches cost $0.01-0.05 per action (screenshot
   encoding). A 50-step task costs $0.50-2.50 in API calls alone.

5. **Speed**: 2-5 seconds per action for vision approaches. Accessibility-based
   approaches are sub-second. MCP tool calls are 0.7-2.5 seconds.

6. **Security**: Agents that can see screens and click things are powerful.
   All serious implementations use sandboxing (Docker, VMs, cloud desktops).

---

## 8. Emerging Trends (Early 2026)

1. **Infrastructure over agents**: Projects like trycua/cua and screenpipe are
   building the PLATFORM layer (sandboxes, perception) rather than end-to-end agents.
   This lets any agent framework use computer-use capabilities.

2. **MCP as the glue**: MCP is becoming the standard for connecting AI agents to
   desktop control tools. Instead of bespoke integrations, agents call MCP tools.

3. **Hybrid is winning**: Pure vision is too slow/expensive. Pure accessibility is
   too limited. The winning pattern is: accessibility tree first, vision fallback.

4. **Set-of-marks spreading**: Overlaying visual labels before VLM analysis is
   becoming standard practice for reducing coordinate hallucination.

5. **On-device models for perception**: Screenpipe-style always-on OCR + transcription
   gives agents continuous context without per-action screenshot costs.

6. **Sandbox-first**: Cloud desktops (cua, VM-based) are preferred over local
   execution for safety and reproducibility.

---

## 9. How This Compares to MCP-Based Tool Integration (Nika Perspective)

Nika's `invoke:` verb with MCP tools is architecturally aligned with Pattern 5 (MCP Tool
Integration). The key difference is scope:

```
Nika invoke: verb                          Computer-Use Agent
===============                            ==================
Calls structured MCP tools                 Observes screen + acts
Deterministic tool params                  Probabilistic coordinates
DAG-orchestrated workflows                 Single-agent loop
Multi-provider LLM routing                 Single model perception
Artifact pipeline (files, media)           Screenshot sequences
Millisecond tool calls                     2-5 second action cycle
```

For Nika, the relevant integration points would be:
- **MCP servers for desktop control** could be invoked via `invoke:` in workflows
- **screenpipe-style context** could feed into `infer:` prompts as background knowledge
- **browser-use/Skyvern results** could be consumed via `fetch:` or custom MCP tools
- The accessibility-tree pattern maps well to structured MCP tool responses

The MCP ecosystem is making it possible to compose desktop automation into larger
workflow DAGs, which is exactly what Nika is built for.

---

## Sources

1. Anthropic Computer Use docs - https://docs.anthropic.com/en/docs/agents-and-tools/computer-use
2. Anthropic "Developing Computer Use" - https://www.anthropic.com/news/developing-computer-use
3. Anthropic vs OpenAI CUA comparison - https://workos.com/blog/anthropics-computer-use-versus-openais-computer-using-agent-cua
4. trycua/cua GitHub - https://github.com/trycua/cua
5. trycua/acu (awesome-computer-use) - https://github.com/trycua/acu
6. ByteDance UI-TARS-desktop - https://github.com/bytedance/UI-TARS-desktop
7. Skyvern GitHub - https://github.com/skyvern-ai/skyvern
8. How Claude Code is built - https://newsletter.pragmaticengineer.com/p/how-claude-code-is-built
9. Claude Code architecture analysis - https://southbridge-research.notion.site/Dependencies-The-Foundation-of-Claude-Code-s-Architecture-2055fec70db181b3bb72cdfe615fad3c
10. Anthropic Desktop Extensions - https://www.anthropic.com/engineering/desktop-extensions
11. awesome-mcp-servers - https://github.com/wong2/awesome-mcp-servers
12. Building AI Desktop Automation (HackerNoon) - https://hackernoon.com/building-ai-desktop-automation-that-survives-the-real-world
13. o-mega.ai top 10 desktop agents - https://o-mega.ai/articles/top-10-ai-agents-for-desktop-automation-2026-mac-windows
14. Windows Desktop Control MCP - https://skywork.ai/skypage/en/unlock-desktop-ai-control/1977606725996441600
15. GUI Automation for Windows MCP - https://www.pulsemcp.com/servers/lpmwfx-gui

## GitHub Repository Index

| Project | URL | Stars (approx) | Focus |
|---------|-----|-----------------|-------|
| trycua/cua | https://github.com/trycua/cua | 13.3k | CUA infrastructure/sandboxes |
| Open Interpreter | https://github.com/OpenInterpreter/open-interpreter | 50k+ | NL code execution + OS mode |
| mediar-ai/screenpipe | https://github.com/mediar-ai/screenpipe | 12k+ | Continuous screen/audio context |
| ByteDance UI-TARS | https://github.com/bytedance/UI-TARS | -- | VLM for desktop UI |
| UI-TARS-desktop | https://github.com/bytedance/UI-TARS-desktop | -- | Desktop app for UI-TARS |
| simular-ai/Agent-S | https://github.com/simular-ai/Agent-S | -- | Hybrid CUA, 34.5% OSWorld |
| OthersideAI/self-operating-computer | https://github.com/OthersideAI/self-operating-computer | -- | Vision-based desktop agent |
| OS-Copilot/OS-Copilot | https://github.com/OS-Copilot/OS-Copilot | -- | Hybrid research agent |
| skyvern-ai/skyvern | https://github.com/skyvern-ai/skyvern | -- | AI browser automation |
| browser-use/browser-use | https://github.com/browser-use/browser-use | -- | Browser-as-API for agents |
| wong2/awesome-mcp-servers | https://github.com/wong2/awesome-mcp-servers | -- | MCP server catalog |
| trycua/acu | https://github.com/trycua/acu | -- | Awesome Computer Use list |
