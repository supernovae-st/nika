# Nika Project Structure — Design Document

**Date:** 2026-02-24
**Status:** Draft
**Authors:** Thibaut, Claude
**Related:**
- [memory-and-agents-design.md](./2026-02-24-memory-and-agents-design.md)
- [chat-as-workflow-dag.md](./2026-02-24-chat-as-workflow-dag.md)

---

## Executive Summary

Définir la structure de projet `.nika/` pour supporter:
- Memory (fichiers context)
- Agents (définitions réutilisables)
- Skills (méthodologies composables)
- Sessions (persistence chat/workflow)
- Config (préférences utilisateur)

---

## Project Structure

```
my-project/
├── .nika/                          # Nika project root (like .git/)
│   ├── config.toml                 # User preferences
│   ├── agents/                     # Agent definitions
│   │   ├── researcher.agent.yaml
│   │   ├── writer.agent.yaml
│   │   └── reviewer.agent.yaml
│   ├── skills/                     # Skill definitions
│   │   ├── tdd.skill.yaml
│   │   ├── seo.skill.yaml
│   │   └── brand-voice.skill.yaml
│   ├── context/                    # Memory files (brand, persona, etc.)
│   │   ├── brand.md
│   │   ├── persona.json
│   │   ├── style-guide.yaml
│   │   └── examples/
│   │       ├── landing-page.md
│   │       └── blog-post.md
│   ├── sessions/                   # Session persistence
│   │   ├── chat-<id>.json
│   │   └── workflow-<id>.json
│   ├── traces/                     # Execution traces (NDJSON)
│   │   └── <workflow>-<timestamp>.ndjson
│   └── cache/                      # Cached data (embeddings, etc.)
│       └── embeddings.db
│
├── workflows/                      # User workflows (or root)
│   ├── generate-page.nika.yaml
│   └── research-pipeline.nika.yaml
│
└── output/                         # Generated content
    └── ...
```

---

## Directory Purposes

### `.nika/config.toml`

```toml
# Project configuration
[project]
name = "qrcode-ai"
version = "1.0.0"

# Default provider for all workflows
[provider]
default = "claude"
model = "claude-sonnet-4-6"

# Editor preferences
[editor]
theme = "solarized"
auto_format = true

# Session settings
[session]
auto_restore = true
max_sessions = 50
ttl_days = 7

# Memory settings
[memory]
auto_load_context = true
context_dir = "context"

# Agent/Skill discovery
[discovery]
agents_dir = "agents"
skills_dir = "skills"
```

### `.nika/agents/`

Reusable agent definitions:

```yaml
# .nika/agents/researcher.agent.yaml
name: researcher
version: 1.0.0
description: "Web research specialist"

system: |
  You are a research specialist...

provider: claude
model: claude-sonnet-4-6
mcp: [perplexity]
max_turns: 15
temperature: 0.3
stop_conditions: ["RESEARCH_COMPLETE"]
```

### `.nika/skills/`

Composable methodology files:

```yaml
# .nika/skills/tdd.skill.yaml
name: tdd
version: 1.0.0
description: "Test-Driven Development"

system_augment: |
  ## TDD Methodology
  You MUST follow RED-GREEN-REFACTOR:
  1. RED: Write failing test first
  2. GREEN: Minimal code to pass
  3. REFACTOR: Clean up
```

### `.nika/context/`

Project memory files:

```
.nika/context/
├── brand.md              # Brand guidelines
├── persona.json          # Target user persona
├── style-guide.yaml      # Writing style rules
├── seo-keywords.txt      # SEO keywords list
└── examples/             # Reference examples
    ├── landing-page.md
    └── email-template.md
```

### `.nika/sessions/`

Persistent session data:

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
    "msg-001": { "output": "Hello!", "tokens": 150 },
    "msg-002": { "output": "...", "tokens": 320 }
  },
  "history": [
    { "role": "user", "content": "Hello" },
    { "role": "assistant", "content": "Hello!" }
  ]
}
```

---

## `nika init` Command

Creates the project structure:

```bash
$ nika init

Creating Nika project...

Created:
  .nika/
  .nika/config.toml          # Default configuration
  .nika/agents/               # Agent definitions
  .nika/skills/               # Skill definitions
  .nika/context/              # Memory/context files
  .nika/sessions/             # Session persistence
  .nika/traces/               # Execution traces

Next steps:
  1. Add context files:     .nika/context/brand.md
  2. Create agents:         .nika/agents/researcher.agent.yaml
  3. Create workflows:      workflows/my-workflow.nika.yaml
  4. Run:                   nika workflows/my-workflow.nika.yaml

$ nika init --with-examples

Also creates:
  .nika/context/brand.md              # Example brand guidelines
  .nika/context/persona.json          # Example persona
  .nika/agents/researcher.agent.yaml  # Example agent
  .nika/skills/seo.skill.yaml         # Example skill
  workflows/example.nika.yaml         # Example workflow
```

---

## Reference Syntax

### In Workflows

```yaml
schema: "nika/workflow@0.6"

# Auto-discover from .nika/ (no paths needed)
agents:
  researcher: researcher           # → .nika/agents/researcher.agent.yaml
  writer: writer                   # → .nika/agents/writer.agent.yaml

skills:
  seo: seo                         # → .nika/skills/seo.skill.yaml
  tdd: tdd                         # → .nika/skills/tdd.skill.yaml

# Memory auto-loads from .nika/context/
memory:
  files:
    brand: brand.md                # → .nika/context/brand.md
    persona: persona.json          # → .nika/context/persona.json

# Or explicit paths (for files outside .nika/)
memory:
  files:
    external: ./other/file.md      # Relative to workflow
```

### In Chat

```
> Hello!
  → Uses default agent from config

> /agent researcher
  → Switches to .nika/agents/researcher.agent.yaml

> /skill seo
  → Applies .nika/skills/seo.skill.yaml to current agent

> /context brand
  → Loads .nika/context/brand.md into conversation

> /memory
  → Lists available context files

> /agents
  → Lists available agents

> /skills
  → Lists available skills
```

---

## Discovery Rules

### Agent Discovery

1. Check `.nika/agents/<name>.agent.yaml`
2. Check `./agents/<name>.agent.yaml` (workflow-relative)
3. Check explicit path if provided

### Skill Discovery

1. Check `.nika/skills/<name>.skill.yaml`
2. Check `./skills/<name>.skill.yaml` (workflow-relative)
3. Check explicit path if provided

### Context Discovery

1. Check `.nika/context/<name>`
2. Check `./context/<name>` (workflow-relative)
3. Check explicit path if provided

---

## File Formats

### Agent File (`.agent.yaml`)

```yaml
# Required
name: string                    # Unique identifier
system: string                  # System prompt

# Optional
version: string                 # SemVer (default: "1.0.0")
description: string             # Human-readable description
provider: string                # LLM provider (default: from config)
model: string                   # Model name (default: from config)
mcp: string[]                   # MCP servers to access
max_turns: number               # Max agent turns (default: 10)
token_budget: number            # Token limit (default: unlimited)
temperature: number             # 0.0-2.0 (default: provider default)
stop_conditions: string[]       # Early stop triggers
extended_thinking: boolean      # Enable Claude thinking (default: false)
thinking_budget: number         # Thinking tokens (default: 4096)
depth_limit: number             # Spawn depth (default: 3)

# Inheritance
inherit: string                 # Parent agent name
system_prepend: string          # Add before parent system
system_append: string           # Add after parent system
```

### Skill File (`.skill.yaml`)

```yaml
# Required
name: string                    # Unique identifier
system_augment: string          # Added to agent's system prompt

# Optional
version: string                 # SemVer (default: "1.0.0")
description: string             # Human-readable description
requires_mcp: string[]          # MCP servers needed for this skill
stop_conditions: string[]       # Suggested stop conditions
validation: ValidationRule[]    # Output validation rules
```

### Context Files

| Extension | Loaded As | Access Pattern |
|-----------|-----------|----------------|
| `.md` | `string` | `{{memory.files.brand}}` |
| `.txt` | `string` | `{{memory.files.notes}}` |
| `.json` | `object` | `{{memory.files.persona.name}}` |
| `.yaml` | `object` | `{{memory.files.rules.tone}}` |
| `.toml` | `object` | `{{memory.files.config.key}}` |

---

## Chat Commands

| Command | Description |
|---------|-------------|
| `/agent <name>` | Switch to agent |
| `/skill <name>` | Apply skill to current agent |
| `/skill -<name>` | Remove skill |
| `/context <name>` | Load context file |
| `/agents` | List available agents |
| `/skills` | List available skills |
| `/memory` | List loaded context |
| `/session save` | Save current session |
| `/session load <id>` | Load session |
| `/session list` | List sessions |
| `/export yaml` | Export chat as workflow |
| `/export trace` | Export as NDJSON |

---

## Integration with Chat-as-DAG

When using chat with the DAG system:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  Chat View                              │  DAG Panel                        │
├─────────────────────────────────────────┼─────────────────────────────────────┤
│                                         │                                   │
│  /agent researcher                      │   ╭─────────────╮                 │
│  Agent switched to: researcher          │   │ researcher  │                 │
│                                         │   ╰──────┬──────╯                 │
│  /skill seo                             │          │                        │
│  Skill applied: seo                     │          │                        │
│                                         │          │                        │
│  /context brand                         │          │                        │
│  Context loaded: brand.md (2.3KB)       │          │                        │
│                                         │          │                        │
│  > Research QR code trends              │   ╭──────┴──────╮                 │
│                                         │   │   msg-001   │                 │
│  ╭──────────────────────────────────╮   │   │ researcher  │                 │
│  │ ⚡ msg-001          ◐ 12.3s     │   │   │ +seo        │                 │
│  │ 🧠 claude-sonnet-4  📊 2K→1.5K  │   │   ╰─────────────╯                 │
│  │ 💬 "Research QR code trends"    │   │                                   │
│  │ 📤 "## QR Code Market 2026..."  │   │   Context: brand.md              │
│  │ 🔗 context: brand.md            │   │   Agent: researcher               │
│  ╰──────────────────────────────────╯   │   Skill: seo                      │
│                                         │                                   │
│  > _                                    │   1 task | 1 layer                │
└─────────────────────────────────────────┴───────────────────────────────────┘
```

---

## Migration from v0.5

### Before (scattered files)

```
project/
├── workflows/
│   ├── pipeline.nika.yaml     # Inline agents, exec: cat for files
│   └── research.nika.yaml     # Duplicate agent definitions
├── context/                   # Not standardized
│   └── brand.md
└── .nika/
    └── config.toml            # Only config
```

### After (organized)

```
project/
├── workflows/
│   ├── pipeline.nika.yaml     # use: researcher, memory.files.brand
│   └── research.nika.yaml     # use: researcher (same agent!)
└── .nika/
    ├── config.toml
    ├── agents/
    │   └── researcher.agent.yaml
    ├── skills/
    │   └── seo.skill.yaml
    ├── context/
    │   └── brand.md
    └── sessions/
```

---

## Implementation Phases

### Phase 1: Project Structure (v0.9)
- [ ] Define `.nika/` structure
- [ ] Update `nika init` to create directories
- [ ] Add `--with-examples` flag

### Phase 2: Discovery System (v0.9)
- [ ] Implement agent discovery
- [ ] Implement skill discovery
- [ ] Implement context discovery
- [ ] Add fallback to explicit paths

### Phase 3: AST Updates (v0.9)
- [ ] Add `agents:` to Workflow
- [ ] Add `skills:` to Workflow
- [ ] Add `memory:` to Workflow
- [ ] Update schema to v0.6

### Phase 4: Chat Commands (v0.9)
- [ ] Implement `/agent` command
- [ ] Implement `/skill` command
- [ ] Implement `/context` command
- [ ] Implement list commands

### Phase 5: Session Integration (v0.9)
- [ ] Save chat sessions to `.nika/sessions/`
- [ ] Load sessions on startup
- [ ] Export chat as workflow

---

## Success Criteria

1. **`nika init` creates full structure**
2. **Agents discoverable by name** (no paths needed)
3. **Skills composable** (`skill: [seo, tdd]`)
4. **Context auto-loaded** from `.nika/context/`
5. **Chat can use same agents/skills** as workflows
6. **Sessions persist** and restore correctly

---

## References

- [Memory & Agents Design](./2026-02-24-memory-and-agents-design.md)
- [Chat as DAG Design](./2026-02-24-chat-as-workflow-dag.md)
- CrewAI: `config/agents.yaml`, `config/tasks.yaml`
- LangGraph: Checkpointers, InMemoryStore
