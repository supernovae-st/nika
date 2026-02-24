# Nika v0.9 — Consolidated Design Specification

**Date:** 2026-02-24
**Status:** Draft (Final Consolidation)
**Authors:** Thibaut, Claude
**CLI Version:** v0.9.0 (target)
**Schema Version:** nika/workflow@0.6 (new schema features)

---

## Executive Summary

Nika v0.9 introduces a **file-first agentic architecture** based on industry convergence (Claude Code, Manus, OpenClaw). Beyond context, agents, and skills, v0.9 adds:

- **User Profile** (user.yaml) — Operator identity, preferences, autonomy
- **Long-term Memory** (memory.yaml + episodic) — Validated facts, decisions, lessons
- **Policies** (policies.yaml) — RBAC, guardrails, audit trails
- **Heartbeat** (heartbeat.yaml) — Proactive automation
- **Enriched Agents** — Full SOUL sections (role, rules, workflow, handoffs)
- **Boot Sequence** — Mandatory startup ritual

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  NIKA v0.9 — FILE-FIRST AGENTIC ARCHITECTURE                                    │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐  │
│  │ Context │  │ Agents  │  │ Skills  │  │  User   │  │ Memory  │  │Policies │  │
│  │  Files  │  │ w/SOUL  │  │  Dirs   │  │ Profile │  │ + Epis. │  │Guardrail│  │
│  └────┬────┘  └────┬────┘  └────┬────┘  └────┬────┘  └────┬────┘  └────┬────┘  │
│       └────────────┴───────────┴────────────┴───────────┴────────────┘        │
│                                      │                                         │
│                         ┌────────────┴────────────┐                            │
│                         │    BOOT SEQUENCE        │                            │
│                         │ SOUL→USER→Memory→Skills │                            │
│                         └─────────────────────────┘                            │
│                                      │                                         │
│                    ┌─────────────────┼─────────────────┐                       │
│                    ▼                 ▼                 ▼                        │
│              ┌──────────┐      ┌──────────┐      ┌──────────┐                  │
│              │   Chat   │      │ Workflow │      │Heartbeat │                  │
│              │   TUI    │      │   DAG    │      │  Auto    │                  │
│              └──────────┘      └──────────┘      └──────────┘                  │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

**Core Insight:** Industry convergence (Manus $2B acquisition, OpenClaw 145K+ stars, Claude Code) proves file-first architecture is the winning pattern for agentic AI.

---

## Terminology (Industry-Aligned)

| Term | Nika Usage | Industry Standard |
|------|------------|-------------------|
| `context:` | Input data (files + session) | CrewAI, LangGraph (context = per-task data) |
| `agents:` | Agent definitions | CrewAI, AutoGen (standard) |
| `skills:` | System prompt augmentation | Unique to Nika (CrewAI uses "tools") |
| `mcp:` | MCP server configuration | MCP Protocol (standard) |
| `use:` | DataStore bindings (task outputs) | Nika-specific |

**Why `context:` over `memory:`:**
- `context` = dynamic, per-execution data (files, session)
- `memory` = typically cross-session persistent storage (embeddings, long-term)
- Industry standard: CrewAI uses `context:` for task inputs

---

## Part 1: Context System

### The Problem (v0.8)

```yaml
# Current workaround: exec: cat to load files
tasks:
  - id: load_brand
    exec: "cat ./context/brand.md"   # Ugly, creates extra task

  - id: generate
    infer: "Generate content. Brand: {{use.load_brand}}"
```

### The Solution (v0.9)

Context has **3 layers**:

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  CONTEXT LAYERS                                                                 │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  Layer 1: Project Files (.nika/context/)                                        │
│  ├── brand.md           → {{context.files.brand}}                               │
│  ├── persona.json       → {{context.files.persona.name}}                        │
│  └── examples/*.md      → {{context.files.examples}}  (array)                   │
│                                                                                 │
│  Layer 2: Session Context (.nika/sessions/)                                     │
│  ├── Chat history       → {{context.session.messages}}                          │
│  ├── Previous outputs   → {{context.session.last_result}}                       │
│  └── Workflow state     → {{context.session.datastore}}                         │
│                                                                                 │
│  Layer 3: Long-term Memory (future v1.0)                                        │
│  ├── Embeddings DB      → semantic search                                       │
│  └── Cross-session      → project knowledge                                     │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### YAML Syntax

```yaml
schema: "nika/workflow@0.6"
workflow: content-pipeline

context:
  # Layer 1: Project files (auto-parsed by extension)
  files:
    brand: brand.md                    # → .nika/context/brand.md (string)
    persona: persona.json              # → .nika/context/persona.json (object)
    examples: examples/*.md            # → glob → array of strings

  # Layer 2: Session restore (optional)
  session: previous-run                # → .nika/sessions/previous-run.json

tasks:
  - id: generate
    infer: |
      Generate landing page.
      Brand: {{context.files.brand}}
      Persona: {{context.files.persona.target_audience}}
```

### Type Inference Rules

| Extension | Parsed As | Access Pattern |
|-----------|-----------|----------------|
| `.md` | `string` | `{{context.files.brand}}` |
| `.txt` | `string` | `{{context.files.notes}}` |
| `.json` | `object` | `{{context.files.persona.name}}` |
| `.yaml` | `object` | `{{context.files.rules.tone}}` |
| `.toml` | `object` | `{{context.files.config.key}}` |
| `*.md` (glob) | `array<string>` | `{{context.files.examples}}` |

---

## Part 2: Flexible Agents (3 Modes)

### Mode 1: Reference (External File)

```yaml
# Workflow references external agent
agents:
  researcher: researcher              # → .nika/agents/researcher.agent.yaml

tasks:
  - id: research
    agent:
      use: researcher                 # Reference by name
      prompt: "Research QR trends"
```

**Agent File Format (`.nika/agents/researcher.agent.yaml`):**

```yaml
name: researcher
version: 1.0.0
description: "Web research specialist"

system: |
  You are a research specialist focused on market intelligence.

  ## Guidelines
  1. Always cite sources with URLs
  2. Prefer recent information (last 6 months)
  3. Cross-reference multiple sources

provider: claude
model: claude-sonnet-4-6
mcp: [perplexity, novanet]
max_turns: 15
temperature: 0.3

stop_conditions:
  - "RESEARCH_COMPLETE"
```

### Mode 2: Inline (No File Needed)

```yaml
# Agent defined directly in workflow (no external file)
agents:
  quick_helper:                       # Inline definition
    system: "You are a quick helper for simple tasks"
    model: claude-haiku
    max_turns: 3

tasks:
  - id: quick_check
    agent:
      use: quick_helper
      prompt: "Check this quickly"

  # Or 100% inline in task (no agents: section needed)
  - id: one_off
    agent:
      system: "You are a formatter"   # Inline, no use:
      prompt: "Format this: {{use.quick_check}}"
      max_turns: 1
```

### Mode 3: Reference + Override (Inherit)

```yaml
agents:
  researcher: researcher              # Base agent

  seo_researcher:                     # Extends researcher
    inherit: researcher
    system_append: "Focus on SEO keywords"
    skill: [seo]
    temperature: 0.2                  # Override

tasks:
  - id: research
    agent:
      use: researcher
      system_append: "Focus on QR codes"  # Task-level override
      prompt: "Research trends"
```

### Resolution Priority

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  AGENT RESOLUTION                                                               │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  task.agent:                                                                    │
│    use: researcher          →  Lookup agents.researcher                        │
│                                   │                                             │
│                                   ├── String "researcher"                       │
│                                   │   → .nika/agents/researcher.agent.yaml      │
│                                   │                                             │
│                                   └── Object { inherit: ..., system: ... }      │
│                                       → Inline or inherit definition            │
│                                                                                 │
│  task.agent:                                                                    │
│    system: "..."            →  100% Inline (no lookup)                          │
│                                                                                 │
│  task.agent:                                                                    │
│    use: researcher          →  Merge: base + overrides                          │
│    system_append: "..."         1. Load base (researcher)                       │
│    temperature: 0.2             2. Apply system_prepend (before)                │
│                                 3. Apply system_append (after)                  │
│                                 4. Override other fields                        │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## Part 3: Composable Skills (3 Modes)

### Mode 1: Reference (External File)

```yaml
skills:
  seo: seo                            # → .nika/skills/seo.skill.yaml
  brand_voice: brand-voice

tasks:
  - id: write
    agent:
      use: writer
      skill: [seo, brand_voice]       # Apply multiple skills
      prompt: "Write landing page"
```

**Skill File Format (`.nika/skills/seo.skill.yaml`):**

```yaml
name: seo
version: 1.0.0
description: "SEO optimization guidelines"

system_augment: |
  ## SEO Requirements

  ### Keyword Integration
  - Include primary keyword in title (H1)
  - Use primary keyword in first 100 words
  - Keyword density: 1-2%

  ### Content Structure
  - Use H2/H3 headings with keywords
  - Keep paragraphs under 150 words

requires_mcp: []
stop_conditions:
  - "SEO_OPTIMIZED"
```

### Mode 2: Inline (No File Needed)

```yaml
skills:
  quick_format:                       # Inline definition
    system_augment: |
      Format output as markdown with headers.
      Use bullet points for lists.

tasks:
  - id: format
    agent:
      use: writer
      skill: [quick_format]
      prompt: "Format this content"
```

### Mode 3: Mix Reference + Inline

```yaml
skills:
  seo: seo                            # Reference file
  brand_voice: brand-voice            # Reference file

  custom_tone:                        # Inline
    system_augment: "Use casual, friendly tone"

tasks:
  - id: write
    agent:
      use: writer
      skill: [seo, brand_voice, custom_tone]  # Mix all
      prompt: "Write content"
```

### Skill Composition

Execution: `agent.system` + `skill[0].system_augment` + `skill[1].system_augment` + ...

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  FINAL SYSTEM PROMPT                                                            │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  [Agent system_prepend]           ← If task specifies                           │
│                                                                                 │
│  You are a content writer...      ← Base agent system                           │
│                                                                                 │
│  [Agent system_append]            ← If task specifies                           │
│                                                                                 │
│  ## SEO Requirements              ← skill[0]: seo                               │
│  - Primary keyword in title...                                                  │
│                                                                                 │
│  ## Brand Voice                   ← skill[1]: brand_voice                       │
│  - Professional but friendly...                                                 │
│                                                                                 │
│  Use casual, friendly tone        ← skill[2]: custom_tone                       │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## Part 4: Chat-as-DAG (All 5 Verbs)

### Chat = Workflow That Builds Itself

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  CHAT TUI                                         YAML GÉNÉRÉ (live)            │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  /agent researcher                                schema: nika/workflow@0.6     │
│  /skill seo                                       workflow: chat-session        │
│  /context brand                                                                 │
│                                                   context:                      │
│  > Research QR trends                               files:                      │
│                                                       brand: brand.md           │
│  ╭─────────────────────────╮                                                    │
│  │ msg-001 🐔 researcher   │ ───────────────►    tasks:                         │
│  │ "## QR Market 2026..."  │                       - id: msg-001                │
│  ╰─────────────────────────╯                         agent:                     │
│                                                        use: researcher          │
│  > /exec npm run build                                 skill: [seo]             │
│                                                        prompt: "Research QR"    │
│  ╭─────────────────────────╮                                                    │
│  │ msg-002 📟 exec         │ ───────────────►      - id: msg-002                │
│  │ "Build successful"      │                         exec: "npm run build"      │
│  ╰─────────────────────────╯                                                    │
│                                                                                 │
│  > /invoke novanet_describe                                                     │
│    entity: qr-code                                                              │
│                                                                                 │
│  ╭─────────────────────────╮                                                    │
│  │ msg-003 🔌 invoke       │ ───────────────►      - id: msg-003                │
│  │ "{entity: ...}"         │                         invoke:                    │
│  ╰─────────────────────────╯                           tool: novanet_describe   │
│                                                        params:                  │
│                                                          entity: qr-code        │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Chat Commands

| Command | Verb | Description |
|---------|------|-------------|
| `> message` | `infer:` | Default: LLM generation |
| `/exec <cmd>` | `exec:` | Shell command |
| `/fetch <url>` | `fetch:` | HTTP request |
| `/invoke <tool>` | `invoke:` | MCP tool call |
| `/agent <name>` | - | Switch agent |
| `/skill <name>` | - | Apply skill |
| `/skill -<name>` | - | Remove skill |
| `/context <name>` | - | Load context file |
| `/agents` | - | List available agents |
| `/skills` | - | List available skills |
| `/export yaml` | - | Export as workflow |
| `/session save` | - | Save session |
| `/session load <id>` | - | Restore session |

### Agent Tool Calls (Inline Boxes)

When an agent uses MCP tools during multi-turn execution:

```
╭──────────────────────────────────────────────────────────────────────────────╮
│ 🐔 msg-001 (researcher)                                        ◐ 12.5s      │
│                                                                              │
│ ┌────────────────────────────────────────────────────────────────────────┐  │
│ │ 🔌 perplexity_search                                          ✓ 2.3s  │  │
│ │ query: "QR code trends 2026"                                           │  │
│ │ → "QR codes are growing 25% year over year..."                         │  │
│ └────────────────────────────────────────────────────────────────────────┘  │
│                                                                              │
│ ┌────────────────────────────────────────────────────────────────────────┐  │
│ │ 🛰️ fetch                                                       ✓ 1.1s  │  │
│ │ url: https://qrcode-ai.com                                             │  │
│ │ → 200 OK (15.2 KB)                                                     │  │
│ └────────────────────────────────────────────────────────────────────────┘  │
│                                                                              │
│ ## QR Code Market Analysis                                                   │
│ Based on my research, QR codes are experiencing 25% growth...                │
│                                                                              │
╰──────────────────────────────────────────────────────────────────────────────╯
```

### Session Persistence

```json
// .nika/sessions/chat-abc123.json
{
  "id": "abc123",
  "type": "chat",
  "created_at": "2026-02-24T10:00:00Z",
  "updated_at": "2026-02-24T10:30:00Z",

  "dag": {
    "tasks": ["msg-001", "msg-002", "msg-003"],
    "edges": [["msg-001", "msg-002"], ["msg-002", "msg-003"]]
  },

  "datastore": {
    "msg-001": { "output": "## QR Market...", "tokens": 2300 },
    "msg-002": { "output": "Build successful", "exit_code": 0 }
  },

  "state": {
    "current_agent": "researcher",
    "active_skills": ["seo"],
    "loaded_context": ["brand.md"]
  },

  "history": [
    { "role": "user", "content": "Research QR trends" },
    { "role": "assistant", "content": "## QR Market 2026..." }
  ]
}
```

---

## Part 5: Project Structure (.nika/)

```
my-project/
├── .nika/                              # Nika project root (like .git/)
│   │
│   ├── config.toml                     # Project configuration
│   ├── user.yaml                       # ✨ NEW: Operator profile
│   ├── memory.yaml                     # ✨ NEW: Long-term memory
│   ├── policies.yaml                   # ✨ NEW: Security & governance
│   ├── heartbeat.yaml                  # ✨ NEW: Proactive automation
│   │
│   ├── agents/                         # Agent definitions (full SOUL)
│   │   ├── researcher.agent.yaml       # ✨ ENRICHED: SOUL sections
│   │   ├── writer.agent.yaml
│   │   └── reviewer.agent.yaml
│   │
│   ├── skills/                         # ✨ Skill directories (not files)
│   │   ├── seo/
│   │   │   ├── SKILL.yaml              # Skill definition
│   │   │   ├── templates/              # Optional templates
│   │   │   └── references/             # Optional docs
│   │   ├── tdd/
│   │   │   └── SKILL.yaml
│   │   └── brand-voice/
│   │       └── SKILL.yaml
│   │
│   ├── context/                        # Context files
│   │   ├── brand.md
│   │   ├── persona.json
│   │   ├── style-guide.yaml
│   │   └── examples/
│   │       ├── landing-page.md
│   │       └── blog-post.md
│   │
│   ├── memory/                         # ✨ NEW: Episodic memory
│   │   ├── 2026-02-24.md               # Today (auto-load)
│   │   ├── 2026-02-23.md               # Yesterday (auto-load)
│   │   └── ...
│   │
│   ├── proposed/                       # ✨ NEW: Agent-proposed changes
│   │   └── user-update-2026-02-24.yaml # Pending human approval
│   │
│   ├── sessions/                       # Session persistence
│   │   ├── chat-abc123.json
│   │   └── workflow-def456.json
│   │
│   ├── traces/                         # Execution traces (audit)
│   │   └── workflow-2026-02-24.ndjson
│   │
│   └── cache/                          # Cached data
│       └── embeddings.db               # (future: semantic search)
│
├── workflows/                          # User workflow files
│   ├── generate-page.nika.yaml
│   └── research-pipeline.nika.yaml
│
└── output/                             # Generated content
    └── ...
```

### Discovery Rules

```
Agent Discovery:
1. agents: section in workflow (inline or reference)
2. .nika/agents/<name>.agent.yaml
3. ./agents/<name>.agent.yaml (workflow-relative)
4. Explicit file: path

Skill Discovery:
1. skills: section in workflow (inline or reference)
2. .nika/skills/<name>.skill.yaml
3. ./skills/<name>.skill.yaml (workflow-relative)
4. Explicit file: path

Context Discovery:
1. .nika/context/<name>
2. ./context/<name> (workflow-relative)
3. Explicit path
```

### config.toml

```toml
[project]
name = "qrcode-ai"
version = "1.0.0"

[provider]
default = "claude"
model = "claude-sonnet-4-6"

[session]
auto_restore = true
max_sessions = 50
ttl_days = 7

[context]
auto_load = true
dir = "context"

[discovery]
agents_dir = "agents"
skills_dir = "skills"
```

### nika init

```bash
$ nika init

Creating Nika project...

Created:
  .nika/
  .nika/config.toml          # Project configuration
  .nika/user.yaml            # ✨ Operator profile
  .nika/memory.yaml          # ✨ Long-term memory (empty)
  .nika/policies.yaml        # ✨ Security guardrails (defaults)
  .nika/heartbeat.yaml       # ✨ Proactive automation (disabled)
  .nika/agents/              # Agent definitions
  .nika/skills/              # Skill directories
  .nika/context/             # Context files
  .nika/memory/              # ✨ Episodic memory
  .nika/proposed/            # ✨ Agent-proposed changes
  .nika/sessions/            # Session persistence
  .nika/traces/              # Execution traces

$ nika init --with-examples

Also creates:
  .nika/context/brand.md                        # Example brand guidelines
  .nika/context/persona.json                    # Example persona
  .nika/agents/researcher.agent.yaml            # Example agent (full SOUL)
  .nika/agents/writer.agent.yaml                # Example agent
  .nika/skills/seo/SKILL.yaml                   # Example skill (directory)
  .nika/skills/seo/templates/meta-tags.md       # Example template
  .nika/memory/{{today}}.md                     # Today's episodic log
  workflows/example.nika.yaml                   # Example workflow (v0.6)

$ nika init --minimal

Creates only:
  .nika/
  .nika/config.toml          # Essential config only
```

---

## Part 6: Complete YAML Example

```yaml
schema: "nika/workflow@0.6"
workflow: content-pipeline
description: "Full v0.9 example with all features"

# ─────────────────────────────────────────────────────────────────────────────
# CONTEXT: Input data (files + session)
# ─────────────────────────────────────────────────────────────────────────────
context:
  files:
    brand: brand.md                   # → .nika/context/brand.md
    persona: persona.json             # → .nika/context/persona.json
    examples: examples/*.md           # → glob array
  session: previous-run               # → .nika/sessions/previous-run.json

# ─────────────────────────────────────────────────────────────────────────────
# AGENTS: Mix reference + inline
# ─────────────────────────────────────────────────────────────────────────────
agents:
  # Mode 1: Reference (string → file)
  researcher: researcher              # → .nika/agents/researcher.agent.yaml
  writer: writer                      # → .nika/agents/writer.agent.yaml

  # Mode 2: Inline (no file)
  quick_helper:
    system: "You are a quick helper for simple validation tasks"
    model: claude-haiku
    max_turns: 3

  # Mode 3: Inherit + override
  seo_researcher:
    inherit: researcher
    system_append: "Focus specifically on SEO opportunities"
    skill: [seo]

# ─────────────────────────────────────────────────────────────────────────────
# SKILLS: Mix reference + inline
# ─────────────────────────────────────────────────────────────────────────────
skills:
  # Reference (string → file)
  seo: seo                            # → .nika/skills/seo.skill.yaml
  brand_voice: brand-voice            # → .nika/skills/brand-voice.skill.yaml

  # Inline
  markdown_format:
    system_augment: |
      Format all output as clean markdown:
      - Use headers (##, ###) for sections
      - Use bullet points for lists
      - Include code blocks where appropriate

# ─────────────────────────────────────────────────────────────────────────────
# MCP: Server configuration
# ─────────────────────────────────────────────────────────────────────────────
mcp:
  novanet:
    command: cargo
    args: [run, -p, novanet-mcp]
    env:
      NOVANET_MCP_NEO4J_URI: bolt://localhost:7687

  perplexity:
    command: npx
    args: [-y, "@anthropic/perplexity-mcp"]

# ─────────────────────────────────────────────────────────────────────────────
# TASKS: Use agents, skills, context
# ─────────────────────────────────────────────────────────────────────────────
tasks:
  # Task with referenced agent + skills
  - id: research
    agent:
      use: seo_researcher             # Uses inherited agent
      prompt: |
        Research QR code market trends for 2026.

        Brand context:
        {{context.files.brand}}

        Target persona:
        {{context.files.persona.target_audience}}

  # Task with quick validation
  - id: validate
    agent:
      use: quick_helper
      prompt: "Verify this research is accurate: {{use.research}}"

  # Task with multiple skills
  - id: write_content
    agent:
      use: writer
      skill: [seo, brand_voice, markdown_format]
      prompt: |
        Write a landing page for QR Code AI.

        Research: {{use.research}}
        Examples: {{context.files.examples}}

  # 100% inline task (no use:)
  - id: final_format
    agent:
      system: "You are a markdown formatter. Clean up and finalize content."
      prompt: "Finalize this content: {{use.write_content}}"
      max_turns: 1

# ─────────────────────────────────────────────────────────────────────────────
# FLOWS: DAG edges (optional, auto-inferred from {{use.xxx}})
# ─────────────────────────────────────────────────────────────────────────────
flows:
  - source: research
    target: validate
  - source: research
    target: write_content
  - source: write_content
    target: final_format
```

---

## Part 7: Claude Code Comparison

Nika's design is inspired by Claude Code's agent/skill structure:

| Aspect | Claude Code | Nika v0.9 |
|--------|-------------|-----------|
| **Location** | `.claude/agents/` | `.nika/agents/` |
| **Format** | Markdown + YAML frontmatter | Pure YAML |
| **Agent Fields** | name, description, tools, model, skills | name, system, model, mcp, skill |
| **Skill Location** | `.claude/skills/name/SKILL.md` | `.nika/skills/name.skill.yaml` |
| **Inheritance** | Not supported | `inherit:` + `system_append:` |
| **Inline** | Not supported | Full inline support in workflow |
| **Context** | CLAUDE.md files | `context:` block + `.nika/context/` |

### Claude Code Agent Example

```markdown
---
name: code-reviewer
description: Expert code reviewer. Use proactively after code changes.
tools: Read, Grep, Glob
model: sonnet
skills: test-driven-development
---

You are a specialized agent for code review...
```

### Nika v0.9 Agent Example

```yaml
name: code-reviewer
description: "Expert code reviewer"
system: |
  You are a specialized agent for code review...
model: claude-sonnet-4-6
mcp: [filesystem]
skill: [tdd]
max_turns: 10
```

**Nika Advantages:**
- Pure YAML (no mixed Markdown)
- Inline definitions in workflow
- Inheritance with override
- Same agents work in Chat AND Workflows

---

## Part 8: User Profile (user.yaml)

### Purpose

The User Profile defines the **operator identity** — who is using Nika, their preferences, and autonomy level. This enables personalized agent behavior without modifying agent definitions.

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  USER PROFILE ARCHITECTURE                                                      │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  .nika/user.yaml                                                                │
│  ├── identity:        Who am I? (name, role, expertise, timezone)               │
│  ├── preferences:     How do I like responses? (verbosity, format, tone)        │
│  ├── autonomy:        What can agents do without asking? (level, permissions)   │
│  └── goals:           What am I trying to achieve? (project goals)              │
│                                                                                 │
│  Load Order: Boot sequence loads user.yaml AFTER agent SOUL, BEFORE workflow    │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Full Specification

```yaml
# .nika/user.yaml — Operator Profile
# Industry standard: Claude Code CLAUDE.md user sections, Manus USER.md

identity:
  name: Thibaut
  role: Senior Developer
  company: SuperNovae Studio
  expertise:
    - Rust
    - TypeScript
    - AI/ML
    - Knowledge Graphs
  timezone: Europe/Paris
  languages:
    primary: fr
    secondary: en
  # Bio for agent context
  bio: |
    Building QR Code AI. Focus on quality over speed.
    Prefers Franglais conversations, English code/docs.

preferences:
  # Response style
  verbosity: concise            # verbose | normal | concise | minimal
  format: structured            # prose | structured | bullet_points
  tone: casual-professional     # formal | professional | casual-professional | casual

  # Code preferences
  code_style:
    indent: 2
    quotes: single
    semicolons: true
    max_line_length: 100

  # Communication
  emoji_usage: minimal          # none | minimal | moderate | heavy
  language_mixing: true         # Franglais allowed

autonomy:
  # Overall autonomy level
  level: high                   # minimal | low | medium | high | full

  # Specific permissions
  auto_execute:
    safe_commands: true         # npm test, cargo check, git status
    file_edits: true            # Edit existing files
    file_creation: false        # Create new files (ask first)

  # Always require approval
  require_approval:
    - git push
    - git push --force
    - file delete
    - rm -rf
    - "cost > 5.00 USD"
    - deploy
    - publish

  # Budget limits
  budget:
    max_cost_per_task: 5.00
    max_tokens_per_turn: 50000
    daily_limit: 100.00

goals:
  current:
    - "Ship Nika v0.9 by end of Q1 2026"
    - "Maintain 80%+ test coverage"
  project:
    - "Build best-in-class workflow engine"
    - "QR Code AI launch ready"
  values:
    - "Quality over speed"
    - "Test before commit"
    - "Question before code"
```

### Access in Workflows

```yaml
# Reference user data in workflows
tasks:
  - id: personalized_greeting
    infer: |
      Greet {{user.identity.name}} in {{user.preferences.tone}} tone.
      They prefer {{user.preferences.verbosity}} responses.
      Current goal: {{user.goals.current[0]}}
```

### Anti-Poisoning Pattern

**Critical**: Agents NEVER write directly to user.yaml.

```
┌───────────────────────────────────────────────────────────────────────────────┐
│  ANTI-POISONING: Agent-Proposed Updates                                       │
├───────────────────────────────────────────────────────────────────────────────┤
│                                                                               │
│  1. Agent proposes:  "I noticed you prefer short responses. Add to profile?"  │
│                                                                               │
│  2. Creates:         .nika/proposed/user-update-2026-02-24.yaml               │
│                      + preference: verbosity: minimal                         │
│                                                                               │
│  3. Human reviews:   nika review proposed                                     │
│                                                                               │
│  4. Human approves:  nika approve user-update-2026-02-24                      │
│                      OR nika reject user-update-2026-02-24                    │
│                                                                               │
│  5. Merge:           Changes applied to user.yaml                             │
│                                                                               │
└───────────────────────────────────────────────────────────────────────────────┘
```

---

## Part 9: Long-term Memory (memory.yaml + Episodic)

### Purpose

Memory provides **validated, persistent knowledge** that survives across sessions. Two types:

1. **Semantic Memory** (memory.yaml) — Facts, decisions, lessons
2. **Episodic Memory** (memory/*.md) — Daily session logs

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  MEMORY ARCHITECTURE                                                            │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  .nika/memory.yaml              .nika/memory/                                   │
│  ├── facts:                     ├── 2026-02-24.md  ← Today (auto-load)         │
│  │   └── Validated truths       ├── 2026-02-23.md  ← Yesterday (auto-load)     │
│  ├── decisions:                 ├── 2026-02-22.md                              │
│  │   └── Choices with rationale └── ...                                        │
│  ├── learned:                                                                   │
│  │   └── Lessons from errors    Boot loads: memory.yaml + today + yesterday    │
│  └── ephemeral:                                                                 │
│      └── Temp notes (TTL)                                                       │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### memory.yaml Specification

```yaml
# .nika/memory.yaml — Validated Long-term Memory
# Industry standard: MAGMA multi-graph, AriGraph, Mem0

facts:
  # Permanent truths about the project
  - fact: "Project uses Rust with tokio async runtime"
    source: CLAUDE.md
    added: 2026-01-15
    confidence: high
    tags: [tech-stack, rust]

  - fact: "Neo4j schema has 61 NodeClasses and 182 ArcClasses"
    source: novanet/brain/models/
    added: 2026-02-20
    confidence: high
    tags: [novanet, schema]

  - fact: "Thibaut prefers Franglais conversations"
    source: observed
    added: 2026-02-10
    confidence: high
    tags: [user-preference]

decisions:
  # Architectural choices with rationale
  - decision: "Use context: over memory: for input data"
    rationale: "Industry standard alignment (CrewAI, LangGraph)"
    date: 2026-02-24
    adr: null                   # Optional: link to ADR
    alternatives_considered:
      - memory: "Rejected - implies cross-session persistence"
      - input: "Rejected - too generic"
    tags: [terminology, v0.9]

  - decision: "File-first architecture over database-first"
    rationale: "Industry convergence (Manus, OpenClaw, Claude Code)"
    date: 2026-02-24
    tags: [architecture, v0.9]

learned:
  # Lessons from mistakes or experience
  - lesson: "Always run tests before commit"
    context: "Broke CI twice by skipping tests"
    date: 2026-02-10
    severity: high
    tags: [workflow, ci]

  - lesson: "Check for relevant skills before any task"
    context: "Wasted time reinventing existing skill patterns"
    date: 2026-02-15
    severity: medium
    tags: [workflow, skills]

ephemeral:
  # Temporary notes with expiration
  - note: "Current focus is v0.9 release"
    expires: 2026-03-31
    tags: [focus]

  - note: "Waiting for rig-core v0.32 for streaming fix"
    expires: 2026-04-15
    tags: [dependency, blocked]
```

### Episodic Memory (Daily Logs)

```markdown
<!-- .nika/memory/2026-02-24.md — Auto-generated daily log -->
# Session Log: 2026-02-24

## Summary
Brainstormed v0.9 file-first architecture with comprehensive research.

## Key Activities
- Researched industry standards (Manus, OpenClaw, Claude Code)
- Designed user.yaml, memory.yaml, policies.yaml, heartbeat.yaml
- Enriched agent format with full SOUL sections
- Defined boot sequence

## Decisions Made
- All new features go directly into v0.9 (not v0.10)
- Using file-first architecture based on industry convergence

## Files Changed
- docs/plans/2026-02-24-v09-consolidated-design.md
- docs/plans/2026-02-24-nika-project-structure.md

## Tokens Used
- Total: 45,000
- Cost: $2.35

## Next Actions
- Implement boot sequence
- Add memory loading to runtime
```

### Auto-load Rules

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  MEMORY AUTO-LOAD AT BOOT                                                       │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  Always load:                                                                   │
│  1. memory.yaml (facts, decisions, learned, ephemeral)                          │
│  2. memory/YYYY-MM-DD.md (today)                                                │
│  3. memory/YYYY-MM-DD.md (yesterday)                                            │
│                                                                                 │
│  Token budget: ~2,000-4,000 tokens for "homework"                               │
│                                                                                 │
│  Cleanup:                                                                       │
│  - Ephemeral notes auto-deleted after expires date                              │
│  - Old episodic logs compressed monthly                                         │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Access in Workflows

```yaml
tasks:
  - id: research
    agent:
      use: researcher
      prompt: |
        Research QR trends.

        Known facts:
        {{memory.facts | where: tags contains "tech-stack"}}

        Previous decisions:
        {{memory.decisions | last: 5}}

        Yesterday's context:
        {{memory.episodic.yesterday.summary}}
```

---

## Part 10: Policies & Governance (policies.yaml)

### Purpose

Policies define **security guardrails, RBAC permissions, and audit requirements**. This prevents agents from executing dangerous operations without explicit approval.

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  POLICIES ARCHITECTURE                                                          │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  .nika/policies.yaml                                                            │
│  ├── tools:           RBAC per verb (allowed, blocked, require_approval)        │
│  ├── guardrails:      Budgets, limits, human-approval triggers                  │
│  ├── validation:      Input/output sanitization, PII redaction                  │
│  └── audit:           Logging, retention, compliance                            │
│                                                                                 │
│  Enforcement: Runtime checks BEFORE every tool call                             │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Full Specification

```yaml
# .nika/policies.yaml — Security & Governance
# Industry standard: OWASP, RBAC, Zero Trust

tools:
  # Per-verb permissions
  exec:
    allowed_commands:
      - npm
      - cargo
      - git
      - pnpm
      - yarn
      - make
      - python
      - node
    blocked_commands:
      - "rm -rf /"
      - "rm -rf ~"
      - "rm -rf *"
      - sudo
      - chmod 777
      - "> /dev/sda"
      - dd
      - mkfs
    require_approval:
      - git push
      - git push --force
      - git reset --hard
      - deploy
      - publish
    sandbox:
      enabled: true
      allowed_paths:
        - "{{project_root}}"
        - "/tmp/nika-*"
      blocked_paths:
        - "~/.ssh"
        - "~/.aws"
        - "~/.config"
        - "/etc"

  fetch:
    allowed_domains:
      - "*.github.com"
      - "*.githubusercontent.com"
      - "api.anthropic.com"
      - "api.openai.com"
      - "localhost:*"
    blocked_domains:
      - "*.onion"
      - "*.local"
    require_approval:
      - "*.internal.company.com"
    rate_limit:
      requests_per_minute: 60
      requests_per_hour: 1000

  invoke:
    allowed_tools:
      - "novanet_*"
      - "perplexity_*"
      - "filesystem_*"
    blocked_tools:
      - "dangerous_tool"
    require_approval:
      - "novanet_delete"
      - "novanet_update"

guardrails:
  # Budget controls
  budget:
    max_tokens_per_task: 50000
    max_tokens_per_workflow: 200000
    max_cost_per_task: 5.00
    max_cost_per_workflow: 20.00
    daily_limit: 100.00
    alert_threshold: 0.8       # Alert at 80% of limit

  # Execution limits
  limits:
    max_turns_per_agent: 20
    max_spawn_depth: 5
    timeout_seconds: 300
    max_file_size_mb: 10
    max_output_length: 100000

  # Human approval triggers
  require_human_approval:
    - "cost > 5.00 USD"
    - "memory_modify"          # Any write to memory.yaml
    - "user_modify"            # Any write to user.yaml
    - "policy_modify"          # Any write to policies.yaml
    - "delete_file"
    - "external_api_write"
    - "spawn_depth > 3"

validation:
  # Input sanitization
  input:
    blocked_patterns:
      - "ignore previous instructions"
      - "ignore all previous"
      - "you are now"
      - "pretend you are"
      - "jailbreak"
      - "DAN mode"
    max_prompt_length: 50000
    require_context: true      # Prompts must have context

  # Output sanitization
  output:
    pii_redaction: true
    redact_patterns:
      - email
      - phone
      - ssn
      - credit_card
      - api_key
    max_length: 100000

audit:
  # Logging requirements
  log_all_tool_calls: true
  log_all_llm_calls: true
  log_token_usage: true
  log_cost: true

  # Retention
  retention_days: 90
  compress_after_days: 30

  # Export
  export_format: ndjson
  export_path: ".nika/traces/"
```

### Enforcement Flow

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  POLICY ENFORCEMENT                                                             │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  Tool Call Request                                                              │
│       │                                                                         │
│       ▼                                                                         │
│  ┌────────────────┐                                                             │
│  │ Check blocked  │──── Blocked ────► REJECT with error code                   │
│  └───────┬────────┘                                                             │
│          │ Not blocked                                                          │
│          ▼                                                                       │
│  ┌────────────────┐                                                             │
│  │ Check allowed  │──── Not in list ─► REJECT with error code                  │
│  └───────┬────────┘                                                             │
│          │ Allowed                                                              │
│          ▼                                                                       │
│  ┌────────────────┐                                                             │
│  │Check approval  │──── Needs approval ─► PAUSE for human input                │
│  └───────┬────────┘                                                             │
│          │ Auto-approved                                                        │
│          ▼                                                                       │
│  ┌────────────────┐                                                             │
│  │ Check budget   │──── Over budget ────► REJECT with budget error             │
│  └───────┬────────┘                                                             │
│          │ Within budget                                                        │
│          ▼                                                                       │
│       EXECUTE                                                                   │
│          │                                                                       │
│          ▼                                                                       │
│  ┌────────────────┐                                                             │
│  │  Audit log     │──── Write to traces                                        │
│  └────────────────┘                                                             │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## Part 11: Heartbeat Automation (heartbeat.yaml)

### Purpose

Heartbeat enables **proactive, scheduled automation** — agents that run periodically without user prompting. This transforms Nika from reactive (wait for commands) to proactive (anticipate needs).

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  HEARTBEAT ARCHITECTURE                                                         │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  .nika/heartbeat.yaml                                                           │
│  ├── enabled:          Master switch                                            │
│  ├── active_hours:     When automation runs (respect user time)                 │
│  ├── tasks:            Scheduled jobs (cron syntax)                             │
│  └── triggers:         Event-based automation                                   │
│                                                                                 │
│  Example jobs:                                                                  │
│  - Daily summary at 9am                                                         │
│  - Weekly memory compression                                                    │
│  - On-commit code review                                                        │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Full Specification

```yaml
# .nika/heartbeat.yaml — Proactive Automation
# Industry standard: cron, GitHub Actions, scheduled agents

enabled: true

# Respect user's work hours
active_hours:
  start: "09:00"
  end: "18:00"
  timezone: Europe/Paris
  weekdays_only: true

# Default interval for background checks
default_interval: 30m

# Scheduled tasks
tasks:
  # Daily morning summary
  - id: daily_summary
    schedule: "0 9 * * *"              # 9am daily
    agent: researcher
    prompt: |
      Compile yesterday's session summary.
      Highlight key decisions and next actions.
    output: ".nika/memory/{{date}}.md"

  # Weekly memory cleanup
  - id: memory_cleanup
    schedule: "0 0 * * 0"              # Sunday midnight
    action: memory_compress
    params:
      older_than_days: 30
      keep_decisions: true
      keep_lessons: true

  # Daily dependency check
  - id: dep_check
    schedule: "0 10 * * 1-5"           # 10am weekdays
    exec: "cargo outdated --depth 1"
    notify_on: changes

  # Code review on commit
  - id: post_commit_review
    trigger: git_commit
    agent: code-reviewer
    prompt: "Review the last commit for quality and security"
    conditions:
      - "files_changed > 5"
      - "not: merge_commit"

# Event triggers (not time-based)
triggers:
  # When workflow fails
  - event: workflow_failed
    action: notify
    params:
      channel: slack
      message: "Workflow {{workflow.name}} failed: {{error}}"

  # When budget threshold reached
  - event: budget_threshold
    threshold: 0.8
    action: notify
    params:
      message: "Budget at 80%: ${{budget.used}} / ${{budget.limit}}"

  # When new file created
  - event: file_created
    patterns: ["*.nika.yaml"]
    agent: workflow-validator
    prompt: "Validate the new workflow: {{file.path}}"
```

### Heartbeat Commands

```bash
# Start heartbeat daemon
nika heartbeat start

# Stop heartbeat daemon
nika heartbeat stop

# Check status
nika heartbeat status

# Run specific task now
nika heartbeat run daily_summary

# List scheduled tasks
nika heartbeat list

# View next scheduled runs
nika heartbeat next
```

---

## Part 12: Enriched Agent Format (Full SOUL)

### Purpose

Agents now have **full SOUL sections** following industry best practices (Claude Code AGENTS.md, Manus SOUL.md). This goes beyond basic system prompts to define personality, rules, workflow patterns, and handoffs.

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  ENRICHED AGENT STRUCTURE                                                       │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  .nika/agents/researcher.agent.yaml                                             │
│  │                                                                              │
│  ├── IDENTITY                                                                   │
│  │   ├── name, version, description                                             │
│  │   └── soul: (role, mission, personality, values)                             │
│  │                                                                              │
│  ├── RULES                                                                      │
│  │   ├── must: (required behaviors)                                             │
│  │   ├── never: (forbidden actions)                                             │
│  │   └── anti_patterns: (style rules)                                           │
│  │                                                                              │
│  ├── TOOLS                                                                      │
│  │   ├── allowed: (permitted MCP tools)                                         │
│  │   ├── when: (usage guidance per tool)                                        │
│  │   └── blocked: (forbidden tools)                                             │
│  │                                                                              │
│  ├── WORKFLOW                                                                   │
│  │   ├── pattern: (plan-execute-verify, etc)                                    │
│  │   └── steps: (explicit workflow)                                             │
│  │                                                                              │
│  ├── HANDOFFS                                                                   │
│  │   ├── condition: (when to delegate)                                          │
│  │   ├── to: (target agent)                                                     │
│  │   └── context: (what to pass)                                                │
│  │                                                                              │
│  └── CONFIG                                                                     │
│      ├── provider, model, mcp                                                   │
│      └── max_turns, temperature                                                 │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Full Agent Specification

```yaml
# .nika/agents/researcher.agent.yaml — Full SOUL Format
# Industry standard: Claude Code AGENTS.md, Manus SOUL.md

# ─────────────────────────────────────────────────────────────────────────────
# IDENTITY
# ─────────────────────────────────────────────────────────────────────────────
name: researcher
version: 1.0.0
description: "Market intelligence and web research specialist"

soul:
  role: "Senior Market Research Analyst"
  mission: "Find accurate, recent, well-sourced data to inform decisions"

  personality:
    tone: analytical           # analytical | creative | supportive | direct
    verbosity: detailed        # minimal | concise | detailed | verbose
    confidence: measured       # humble | measured | confident | assertive

  values:
    - "Accuracy over speed — verify before reporting"
    - "Multiple sources — never rely on single source"
    - "Recency matters — prefer data from last 6 months"
    - "Cite everything — URLs for all claims"

# ─────────────────────────────────────────────────────────────────────────────
# RULES
# ─────────────────────────────────────────────────────────────────────────────
rules:
  must:
    - "Always cite sources with full URLs"
    - "Cross-reference at least 2 sources for statistics"
    - "Include publication date for all references"
    - "Distinguish between facts and opinions"
    - "Acknowledge uncertainty when data is limited"

  never:
    - "Fabricate statistics or sources"
    - "Present speculation as fact"
    - "Ignore contradictory evidence"
    - "Skip source verification"

  anti_patterns:
    - "Avoid filler phrases (In conclusion, It's worth noting)"
    - "Skip unnecessary hedging (I think, maybe, perhaps)"
    - "Don't repeat the question in the answer"
    - "Avoid walls of text — use structure"

# ─────────────────────────────────────────────────────────────────────────────
# TOOLS
# ─────────────────────────────────────────────────────────────────────────────
tools:
  allowed:
    - perplexity_search
    - fetch
    - novanet_describe
    - novanet_search
    - novanet_traverse

  when:
    perplexity_search: "For real-time web research and recent news"
    fetch: "For reading specific URLs or API endpoints"
    novanet_describe: "For entity context from knowledge graph"
    novanet_search: "For finding related entities"
    novanet_traverse: "For exploring entity relationships"

  blocked:
    - exec                     # Research agent shouldn't run commands
    - novanet_update          # Read-only access
    - novanet_delete

# ─────────────────────────────────────────────────────────────────────────────
# WORKFLOW
# ─────────────────────────────────────────────────────────────────────────────
workflow:
  pattern: plan-execute-verify

  steps:
    - name: Understand
      action: "Parse the research question, identify key entities and scope"

    - name: Plan
      action: "Think aloud about research strategy before executing"

    - name: Search
      action: "Use perplexity_search for broad web research"

    - name: Deep Dive
      action: "Use fetch for specific sources, novanet for context"

    - name: Cross-Reference
      action: "Verify key claims across multiple sources"

    - name: Synthesize
      action: "Compile findings with citations"

    - name: Verify
      action: "Review for accuracy, recency, source quality"

# ─────────────────────────────────────────────────────────────────────────────
# HANDOFFS
# ─────────────────────────────────────────────────────────────────────────────
handoffs:
  - condition: "Task requires code implementation"
    to: coder
    context:
      - research_findings
      - relevant_entities
    message: "Research complete. Handing off to coder for implementation."

  - condition: "Task requires content writing"
    to: writer
    context:
      - research_findings
      - key_statistics
      - source_urls
    message: "Research complete. Handing off to writer for content creation."

  - condition: "Research reveals critical blocker"
    to: human
    context:
      - blocker_description
      - impact_assessment
    message: "BLOCKER FOUND: Escalating to human for decision."

# ─────────────────────────────────────────────────────────────────────────────
# CONFIG
# ─────────────────────────────────────────────────────────────────────────────
provider: claude
model: claude-sonnet-4-6
mcp:
  - perplexity
  - novanet
max_turns: 15
temperature: 0.3
token_budget: 50000

stop_conditions:
  - "RESEARCH_COMPLETE"
  - "INSUFFICIENT_DATA"
  - "BLOCKER_FOUND"

# Inheritance (optional)
inherit: null                  # Can inherit from base agent
system_prepend: null          # Add before inherited system
system_append: null           # Add after inherited system
```

### Skill Directory Format (Enriched)

Skills now use **directory structure** instead of single files:

```
.nika/skills/
├── seo/
│   ├── SKILL.yaml            # Frontmatter + instructions
│   ├── templates/            # Optional reusable templates
│   │   └── meta-tags.md
│   ├── scripts/              # Optional helper scripts
│   │   └── keyword-density.py
│   └── references/           # Optional reference docs
│       └── google-guidelines.md
│
├── tdd/
│   ├── SKILL.yaml
│   └── templates/
│       └── test-template.rs
│
└── brand-voice/
    └── SKILL.yaml            # Simple skill (no subdirs needed)
```

**SKILL.yaml Format:**

```yaml
# .nika/skills/seo/SKILL.yaml

name: seo
version: 1.0.0
description: "SEO optimization guidelines for content generation"

# Core skill instructions
system_augment: |
  ## SEO Optimization Requirements

  ### Keyword Integration
  - Include primary keyword in title (H1)
  - Use primary keyword in first 100 words
  - Include 2-3 secondary keywords naturally
  - Keyword density: 1-2% (don't overstuff)

  ### Content Structure
  - Use H2/H3 headings with keywords
  - Keep paragraphs under 150 words
  - Include bullet points and lists
  - Add internal linking opportunities

  ### Meta Elements
  - Title tag: 50-60 characters
  - Meta description: 150-160 characters
  - URL slug: short, keyword-rich, hyphens

  ### Output Requirements
  Always include in your response:
  1. SEO_TITLE: Optimized title tag
  2. SEO_DESCRIPTION: Meta description
  3. SEO_KEYWORDS: Primary and secondary keywords used

# Resource references (loaded on demand)
resources:
  - templates/meta-tags.md
  - references/google-guidelines.md

requires_mcp: []

stop_conditions:
  - "SEO_OPTIMIZED"

# Optional validation rules
validation:
  - rule: "title_length <= 60"
    message: "Title exceeds 60 characters"
  - rule: "description_length <= 160"
    message: "Meta description exceeds 160 characters"
```

---

## Part 13: Boot Sequence

### Purpose

The Boot Sequence is the **mandatory startup ritual** that loads all context files in the correct order. This ensures agents always have full context before executing.

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  BOOT SEQUENCE                                                                  │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  ┌──────────────────────────────────────────────────────────────────────────┐  │
│  │  PHASE 1: IDENTITY (Who is the agent?)                                   │  │
│  │  Load: agent SOUL sections (role, mission, personality, values)           │  │
│  └──────────────────────────────────────────────────────────────────────────┘  │
│                               │                                                 │
│                               ▼                                                 │
│  ┌──────────────────────────────────────────────────────────────────────────┐  │
│  │  PHASE 2: USER (Who am I serving?)                                       │  │
│  │  Load: user.yaml (identity, preferences, autonomy, goals)                 │  │
│  └──────────────────────────────────────────────────────────────────────────┘  │
│                               │                                                 │
│                               ▼                                                 │
│  ┌──────────────────────────────────────────────────────────────────────────┐  │
│  │  PHASE 3: MEMORY (What do we know?)                                      │  │
│  │  Load: memory.yaml + today.md + yesterday.md                              │  │
│  └──────────────────────────────────────────────────────────────────────────┘  │
│                               │                                                 │
│                               ▼                                                 │
│  ┌──────────────────────────────────────────────────────────────────────────┐  │
│  │  PHASE 4: POLICIES (What are the rules?)                                 │  │
│  │  Load: policies.yaml (guardrails, permissions, audit)                     │  │
│  └──────────────────────────────────────────────────────────────────────────┘  │
│                               │                                                 │
│                               ▼                                                 │
│  ┌──────────────────────────────────────────────────────────────────────────┐  │
│  │  PHASE 5: CONTEXT (What's the input?)                                    │  │
│  │  Load: context files (brand.md, persona.json, etc.)                       │  │
│  └──────────────────────────────────────────────────────────────────────────┘  │
│                               │                                                 │
│                               ▼                                                 │
│  ┌──────────────────────────────────────────────────────────────────────────┐  │
│  │  PHASE 6: SKILLS (What methodologies apply?)                             │  │
│  │  Load: skill frontmatter first, body on demand                            │  │
│  └──────────────────────────────────────────────────────────────────────────┘  │
│                               │                                                 │
│                               ▼                                                 │
│                          READY TO EXECUTE                                       │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Token Budget

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  BOOT SEQUENCE TOKEN BUDGET                                                     │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  Phase                          Typical Tokens    Max Recommended               │
│  ─────────────────────────────────────────────────────────────────             │
│  Agent SOUL                     500-1,000         2,000                         │
│  User Profile                   300-500           1,000                         │
│  Memory (facts/decisions)       500-1,500         3,000                         │
│  Episodic (today+yesterday)     200-800           2,000                         │
│  Policies                       200-400           1,000                         │
│  Context Files                  1,000-3,000       5,000                         │
│  Skills (frontmatter only)      200-500           1,000                         │
│  ─────────────────────────────────────────────────────────────────             │
│  TOTAL                          ~3,000-7,000      ~15,000                        │
│                                                                                 │
│  Rule: Boot sequence should use <10% of context window                          │
│  Claude Sonnet: 200K context → 20K max for boot                                 │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Progressive Disclosure

To minimize token usage, boot uses **progressive disclosure**:

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  PROGRESSIVE DISCLOSURE                                                         │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  Level 1: ALWAYS LOAD (boot sequence)                                           │
│  ├── Agent SOUL (role, mission, rules)                                          │
│  ├── User identity + preferences                                                │
│  ├── Memory facts + recent decisions                                            │
│  ├── Policy guardrails (blocked lists)                                          │
│  └── Skill frontmatter (names, descriptions)                                    │
│                                                                                 │
│  Level 2: LOAD ON DEMAND (during execution)                                     │
│  ├── Full skill body (when skill: [x] applied)                                  │
│  ├── Context files (when {{context.files.x}} referenced)                        │
│  ├── Episodic memory older than yesterday                                       │
│  └── Detailed policy rules (on policy check)                                    │
│                                                                                 │
│  Level 3: LOAD ON REQUEST (explicit command)                                    │
│  ├── Skill resources (templates, scripts)                                       │
│  ├── Full audit logs                                                            │
│  └── Archived memory                                                            │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Implementation

```rust
// In runtime/boot.rs

pub struct BootContext {
    pub agent_soul: AgentSoul,
    pub user: UserProfile,
    pub memory: MemoryStore,
    pub policies: PolicyConfig,
    pub context: ContextFiles,
    pub skills: Vec<SkillFrontmatter>,
}

impl BootContext {
    pub async fn load(project_root: &Path, agent_name: &str) -> Result<Self, NikaError> {
        // Phase 1: Agent SOUL
        let agent_soul = AgentSoul::load(project_root, agent_name).await?;

        // Phase 2: User Profile
        let user = UserProfile::load(project_root).await
            .unwrap_or_default(); // Optional

        // Phase 3: Memory
        let memory = MemoryStore::load_with_episodic(
            project_root,
            2, // Load today + yesterday
        ).await?;

        // Phase 4: Policies
        let policies = PolicyConfig::load(project_root).await
            .unwrap_or_default(); // Defaults are permissive

        // Phase 5: Context (lazy - load references only)
        let context = ContextFiles::scan(project_root).await?;

        // Phase 6: Skills (frontmatter only)
        let skills = SkillFrontmatter::scan(project_root).await?;

        Ok(Self {
            agent_soul,
            user,
            memory,
            policies,
            context,
            skills,
        })
    }

    pub fn to_system_prompt(&self) -> String {
        format!(
            "{}\n\n{}\n\n{}\n\n{}",
            self.agent_soul.to_prompt(),
            self.user.to_prompt(),
            self.memory.to_prompt(),
            self.policies.summary(),
        )
    }
}
```

---

## Implementation Phases

### Phase 1: Core Project Structure (Sprint 1)

```
[ ] .nika/ directory structure (all new dirs)
[ ] nika init command (create all)
[ ] nika init --with-examples (templates)
[ ] config.toml loading
[ ] user.yaml loading
[ ] memory.yaml loading
[ ] policies.yaml loading
[ ] heartbeat.yaml loading (parse only)
```

### Phase 2: Discovery System (Sprint 1)

```
[ ] Agent discovery (file + inline)
[ ] Agent SOUL parsing (full format)
[ ] Skill discovery (directory structure)
[ ] Skill frontmatter vs body separation
[ ] Context discovery
[ ] Inherit resolution
```

### Phase 3: Boot Sequence (Sprint 2)

```
[ ] BootContext struct
[ ] Phase 1: Agent SOUL loading
[ ] Phase 2: User Profile loading
[ ] Phase 3: Memory loading (facts + episodic)
[ ] Phase 4: Policy loading
[ ] Phase 5: Context scanning
[ ] Phase 6: Skill frontmatter loading
[ ] to_system_prompt() generation
[ ] Token budget tracking
```

### Phase 4: AST Updates (Sprint 2)

```
[ ] context: field (files, session)
[ ] agents: field (3 modes, full SOUL)
[ ] skills: field (3 modes, directory)
[ ] user: field (workflow access)
[ ] memory: field (workflow access)
[ ] use: in AgentParams
[ ] skill: array in AgentParams
[ ] soul: section in AgentParams
[ ] rules: section in AgentParams
[ ] tools: section in AgentParams
[ ] workflow: section in AgentParams
[ ] handoffs: section in AgentParams
[ ] system_prepend/system_append
[ ] inherit: field
[ ] Schema v0.6
```

### Phase 5: Policy Enforcement (Sprint 2)

```
[ ] PolicyConfig struct
[ ] Tool permission checking (allowed/blocked)
[ ] Approval workflow (require_approval)
[ ] Budget tracking (tokens, cost)
[ ] Input validation (blocked patterns)
[ ] Output sanitization (PII redaction)
[ ] Audit logging
```

### Phase 6: Memory System (Sprint 3)

```
[ ] MemoryStore struct
[ ] Semantic memory (facts, decisions, learned)
[ ] Episodic memory (daily .md files)
[ ] Auto-load today + yesterday
[ ] Ephemeral note expiration
[ ] {{memory.*}} binding syntax
[ ] Anti-poisoning (proposed/ directory)
```

### Phase 7: Chat Integration (Sprint 3)

```
[ ] /agent command
[ ] /skill command
[ ] /context command
[ ] /exec, /fetch, /invoke verbs
[ ] /memory command (list/add)
[ ] Session auto-save
[ ] /export yaml
```

### Phase 8: Chat-DAG View (Sprint 3)

```
[ ] NodeBox widget
[ ] Inline tool call boxes
[ ] DAG sidebar
[ ] Real-time metrics
```

### Phase 9: Heartbeat System (Sprint 4)

```
[ ] HeartbeatConfig struct
[ ] Cron schedule parsing
[ ] Active hours enforcement
[ ] Scheduled task execution
[ ] Event triggers (git_commit, etc)
[ ] nika heartbeat start/stop/status
[ ] nika heartbeat run <task>
```

---

## Success Criteria

| Criterion | Validation |
|-----------|------------|
| `nika init` creates full .nika/ | All directories + config files created |
| Agents: 3 modes + full SOUL | Reference, inline, inherit + SOUL sections |
| Skills: directory structure | SKILL.yaml + optional templates/references |
| Context loads files | `{{context.files.brand}}` available |
| User profile loads | `{{user.identity.name}}` available |
| Memory loads at boot | facts, decisions, learned, episodic |
| Policies enforce at runtime | Blocked commands rejected, approval pauses |
| Boot sequence completes | All 6 phases load in order |
| Chat supports 5 verbs | /exec, /fetch, /invoke work |
| Sessions persist | Restart preserves chat history |
| Chat exports to YAML | `/export yaml` valid workflow |
| Anti-poisoning works | Agent changes go to proposed/ |
| Heartbeat schedules run | Cron tasks execute at scheduled time |

---

## Open Questions

### Resolved in v0.9 Design

1. **✅ Skill Conflicts**: What if two skills have conflicting instructions?
   - **Resolved**: Last skill wins (order matters in `skill: [a, b, c]`)

2. **✅ Agent Versioning**: How to handle breaking changes?
   - **Resolved**: `version:` field + deprecation warnings

3. **✅ Session Cleanup**: Auto-prune or manual?
   - **Resolved**: Auto-cleanup after `ttl_days` (config.toml)

4. **✅ Long-term Memory**: Where to store persistent knowledge?
   - **Resolved**: `memory.yaml` (facts, decisions, learned) + episodic (memory/*.md)

5. **✅ User Preferences**: How to personalize agent behavior?
   - **Resolved**: `user.yaml` with identity, preferences, autonomy, goals

6. **✅ Security/Governance**: How to prevent dangerous operations?
   - **Resolved**: `policies.yaml` with RBAC, guardrails, validation, audit

### Still Open for v0.9

1. **Memory Compression**: How to compress old episodic logs?
   - Proposed: Monthly compression via heartbeat task
   - Open: What summarization approach?

2. **Handoff Implementation**: How do agent-to-agent handoffs work?
   - Proposed: spawn_agent with context passing
   - Open: Should handoffs be synchronous or async?

3. **Policy Inheritance**: Can workflows override project policies?
   - Proposed: Workflow can only be MORE restrictive, never less
   - Open: Syntax for policy overrides?

4. **Heartbeat Daemon**: How to run background scheduler?
   - Proposed: `nika heartbeat start` as background process
   - Open: Integration with systemd/launchd?

### Deferred to v1.0

1. **Embeddings Integration**: Semantic search in memory
2. **Multi-Project**: Shared agents/skills across projects
3. **Remote Agents**: Agents running on different machines
4. **Team Collaboration**: Shared policies, audit aggregation

---

## Related Documents

| Document | Purpose |
|----------|---------|
| [Chat as Workflow DAG](./2026-02-24-chat-as-workflow-dag.md) | Detailed DAG design |
| [Chat DAG Implementation](./2026-02-24-chat-dag-implementation-plan.md) | Implementation steps |
| [Project Structure](./2026-02-24-nika-project-structure.md) | Folder structure details |
| [Memory & Agents Design](./2026-02-24-memory-and-agents-design.md) | Agent/memory research |

---

## Industry Research Sources

The v0.9 design is based on industry convergence from:

| Source | Key Patterns Adopted |
|--------|---------------------|
| **Claude Code** (Anthropic) | AGENTS.md, skills/, tiered loading |
| **Manus** ($2B Meta acquisition) | SOUL.md, file-first architecture |
| **OpenClaw** (145K+ stars) | USER.md, IDENTITY.md patterns |
| **CrewAI** | agents.yaml, tasks.yaml, context: terminology |
| **LangGraph** | Checkpointers, memory persistence |
| **MAGMA Research** | Multi-graph memory architecture |
| **Anthropic Context Engineering** | Boot sequence, progressive disclosure |

---

*Final consolidation from 2026-02-24 brainstorming session.*
*Updated with industry research: user.yaml, memory.yaml, policies.yaml, heartbeat.yaml, enriched SOUL agents, boot sequence.*
