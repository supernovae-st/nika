# Memory & External Agents — Design Document

**Date:** 2026-02-24
**Status:** Draft
**Authors:** Thibaut, Claude
**Related:** [2026-02-24-chat-as-workflow-dag.md](./2026-02-24-chat-as-workflow-dag.md)

---

## Executive Summary

Deux features manquantes identifiées lors du brainstorm "Chat as DAG":

1. **Memory System** — Charger des fichiers externes (context, brand, persona)
2. **External Agents/Skills** — Référencer des définitions d'agents ou skills réutilisables

---

## Feature 1: Memory System (`memory:`)

### Problème Actuel

Pas de chargement de fichiers externes. Workaround actuel:

```yaml
# WORKAROUND ACTUEL (moche)
tasks:
  - id: load_brand
    exec: "cat ./context/brand.md"

  - id: generate
    use:
      brand: load_brand
    infer: "Using brand: {{use.brand}}"
```

### Solution Proposée

Nouveau champ `memory:` au niveau workflow:

```yaml
schema: nika/workflow@0.6
workflow: generate-page

# NEW: Memory system (Option B - Layered)
memory:
  # Layer 1: Project files (loaded at start)
  files:
    brand: ./context/brand.md           # Markdown → string
    persona: ./context/persona.json     # JSON → object
    rules: ./context/rules.yaml         # YAML → object
    examples: ./examples/*.md           # Glob → array of strings

  # Layer 2: Session context (optional)
  session: .nika/sessions/chat-abc.json  # Load previous chat

  # Layer 3: Long-term memory (future)
  # persistent: .nika/memory/embeddings.db

tasks:
  - id: generate
    infer: |
      Brand voice: {{memory.files.brand}}
      Persona: {{memory.files.persona.name}}

      Previous conversation:
      {{memory.session.messages}}

      Generate a landing page.
```

### Memory Layers

```
┌─────────────────────────────────────────────────────────────┐
│  Layer 3: Long-term Memory (v1.0+)                          │
│  • SQLite/NDJSON avec embeddings                            │
│  • Cross-session, queryable                                 │
│  • Auto-summarization                                       │
├─────────────────────────────────────────────────────────────┤
│  Layer 2: Session Context (v0.9)                            │
│  • Chat history (.nika/sessions/*.json)                     │
│  • Workflow execution history                               │
│  • Accessible via {{memory.session}}                        │
├─────────────────────────────────────────────────────────────┤
│  Layer 1: Project Files (v0.9)                              │
│  • Static files (brand.md, persona.json)                    │
│  • Loaded at workflow start                                 │
│  • Accessible via {{memory.files.alias}}                    │
└─────────────────────────────────────────────────────────────┘
```

### Supported File Types

| Extension | Parsing | Access |
|-----------|---------|--------|
| `.md` | Raw string | `{{memory.files.brand}}` |
| `.txt` | Raw string | `{{memory.files.notes}}` |
| `.json` | JSON object | `{{memory.files.persona.name}}` |
| `.yaml` | YAML object | `{{memory.files.rules.tone}}` |
| `.toml` | TOML object | `{{memory.files.config.key}}` |
| `*.md` (glob) | Array of strings | `{{memory.files.examples}}` |

### AST Changes

```rust
// src/ast/workflow.rs
#[derive(Debug, Deserialize)]
pub struct Workflow {
    pub schema: String,
    pub provider: String,
    pub model: Option<String>,
    pub mcp: Option<FxHashMap<String, McpConfigInline>>,
    pub memory: Option<MemoryConfig>,  // NEW
    pub tasks: Vec<Arc<Task>>,
    pub flows: Vec<Flow>,
}

// src/ast/memory.rs (NEW)
#[derive(Debug, Clone, Deserialize)]
pub struct MemoryConfig {
    /// Static files to load at workflow start
    #[serde(default)]
    pub files: FxHashMap<String, FileRef>,

    /// Session context to restore
    #[serde(default)]
    pub session: Option<String>,

    /// Long-term memory database (future)
    #[serde(default)]
    pub persistent: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum FileRef {
    /// Simple path: "brand: ./context/brand.md"
    Path(String),
    /// Glob pattern: "examples: ./examples/*.md"
    Glob(String),
}
```

---

## Feature 2: External Agents & Skills

### Problème Actuel

Chaque `agent:` doit définir son prompt inline. Pas de réutilisation:

```yaml
# ACTUEL: Tout inline, pas réutilisable
tasks:
  - id: research
    agent:
      prompt: |
        You are a research agent...
        [50 lignes de prompt]
      system: |
        [30 lignes de system prompt]
      mcp: [perplexity]
      max_turns: 10
```

### Solution Proposée

Nouveau champ `agents:` au niveau workflow + référencement:

```yaml
schema: nika/workflow@0.6
workflow: research-pipeline

# NEW: Agent definitions (can be inline or external)
agents:
  researcher:
    file: ./agents/researcher.agent.yaml    # External file

  writer:
    file: ./agents/writer.agent.yaml

  reviewer:                                  # Inline definition
    system: "You are a code reviewer..."
    mcp: [filesystem]
    max_turns: 5

# NEW: Skill definitions
skills:
  tdd: ./skills/tdd.skill.yaml
  debugging: ./skills/debugging.skill.yaml

tasks:
  - id: research
    agent:
      use: researcher                        # Reference defined agent
      prompt: "Research QR code trends"      # Task-specific prompt

  - id: write
    agent:
      use: writer
      prompt: "Write article based on {{use.research}}"
      skill: tdd                             # Apply skill to agent
```

### External Agent File Format

```yaml
# ./agents/researcher.agent.yaml
name: researcher
version: 1.0.0
description: "Web research specialist"

# Agent configuration
system: |
  You are a research specialist. Your goal is to find accurate,
  up-to-date information from the web and synthesize it clearly.

  Guidelines:
  - Always cite sources
  - Prefer recent information
  - Cross-reference multiple sources

provider: claude
model: claude-sonnet-4-6

mcp:
  - perplexity
  - web-search

max_turns: 15
token_budget: 50000
temperature: 0.3

stop_conditions:
  - "RESEARCH_COMPLETE"
  - "NO_MORE_SOURCES"

# Optional: Default tools to always include
tools:
  - web_search
  - fetch_url
```

### External Skill File Format

```yaml
# ./skills/tdd.skill.yaml
name: tdd
version: 1.0.0
description: "Test-Driven Development methodology"

# Skill = system prompt augmentation
system_augment: |
  ## TDD Methodology

  You MUST follow the RED-GREEN-REFACTOR cycle:

  1. RED: Write a failing test first
  2. GREEN: Write minimal code to pass
  3. REFACTOR: Clean up while keeping tests green

  NEVER write implementation before tests.

# Optional: Required MCP servers for this skill
requires_mcp:
  - filesystem

# Optional: Suggested stop conditions
stop_conditions:
  - "ALL_TESTS_PASSING"
```

### Inheritance & Composition

```yaml
# Agents can inherit from base agents
agents:
  base-agent:
    file: ./agents/base.agent.yaml

  specialized:
    inherit: base-agent                      # Inherit base config
    system_append: |                         # Append to system prompt
      Additional specialization...
    mcp:                                     # Override/extend MCP
      - extra-server

tasks:
  - id: task1
    agent:
      use: specialized
      skill: [tdd, debugging]                # Multiple skills
      prompt: "Do the thing"
```

### AST Changes

```rust
// src/ast/workflow.rs
#[derive(Debug, Deserialize)]
pub struct Workflow {
    // ... existing fields ...
    pub agents: Option<FxHashMap<String, AgentDef>>,  // NEW
    pub skills: Option<FxHashMap<String, String>>,    // NEW (name -> path)
}

// src/ast/agent_def.rs (NEW)
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum AgentDef {
    /// External file reference
    External { file: String },
    /// Inline definition with optional inheritance
    Inline(AgentTemplate),
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentTemplate {
    pub inherit: Option<String>,
    pub system: Option<String>,
    pub system_append: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub mcp: Option<Vec<String>>,
    pub max_turns: Option<u32>,
    pub token_budget: Option<u32>,
    pub temperature: Option<f32>,
    pub stop_conditions: Option<Vec<String>>,
}

// src/ast/agent.rs (MODIFY)
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AgentParams {
    // ... existing fields ...

    /// Reference to a defined agent (NEW)
    #[serde(rename = "use")]
    pub agent_ref: Option<String>,

    /// Skills to apply to this agent (NEW)
    #[serde(default)]
    pub skill: Option<SkillRef>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum SkillRef {
    Single(String),
    Multiple(Vec<String>),
}
```

---

## Combined Example

```yaml
schema: nika/workflow@0.6
workflow: full-content-pipeline

# Memory: Load project context
memory:
  files:
    brand: ./context/brand.md
    persona: ./context/persona.json
    style_guide: ./context/style-guide.yaml
  session: .nika/sessions/previous-run.json

# Agents: Define reusable agents
agents:
  researcher:
    file: ./agents/researcher.agent.yaml
  writer:
    file: ./agents/writer.agent.yaml
  reviewer:
    file: ./agents/reviewer.agent.yaml

# Skills: Define reusable skills
skills:
  seo: ./skills/seo-optimization.skill.yaml
  brand_voice: ./skills/brand-voice.skill.yaml

# MCP servers
mcp:
  perplexity:
    command: npx
    args: [-y, "@anthropic/perplexity-mcp"]
  novanet:
    command: cargo
    args: [run, -p, novanet-mcp]

tasks:
  - id: research
    agent:
      use: researcher
      prompt: |
        Research trends for: {{memory.files.brand}}
        Focus on: SEO, competitors, audience

  - id: write
    agent:
      use: writer
      skill: [seo, brand_voice]              # Apply multiple skills
      prompt: |
        Write content based on research: {{use.research}}

        Brand guidelines:
        {{memory.files.style_guide}}

  - id: review
    agent:
      use: reviewer
      prompt: |
        Review this content: {{use.write}}
        Check against: {{memory.files.brand}}
```

---

## Implementation Phases

### Phase 1: Memory Files (v0.9)

- [ ] Add `MemoryConfig` to AST
- [ ] Implement file loading (md, json, yaml, toml)
- [ ] Implement glob patterns
- [ ] Add `{{memory.files.*}}` template resolution
- [ ] Tests for all file types

### Phase 2: Session Memory (v0.9)

- [ ] Load session files into memory
- [ ] Add `{{memory.session.*}}` template resolution
- [ ] Integration with Chat-as-DAG feature

### Phase 3: External Agents (v0.9)

- [ ] Add `AgentDef` to AST
- [ ] Implement .agent.yaml parser
- [ ] Implement `use:` resolution in AgentParams
- [ ] Implement inheritance (`inherit:`)

### Phase 4: Skills (v0.9)

- [ ] Add skills to AST
- [ ] Implement .skill.yaml parser
- [ ] Implement system prompt augmentation
- [ ] Implement `skill:` resolution in AgentParams

### Phase 5: Long-term Memory (v1.0+)

- [ ] Design persistent memory format
- [ ] Implement embedding storage
- [ ] Implement `memory.search()` query syntax

---

## Schema Version

```yaml
# v0.5 (current)
schema: nika/workflow@0.5

# v0.6 (proposed - memory + agents + skills)
schema: nika/workflow@0.6
```

---

## File Conventions

| Type | Extension | Location |
|------|-----------|----------|
| Workflow | `.nika.yaml` | `./workflows/` or root |
| Agent | `.agent.yaml` | `./agents/` |
| Skill | `.skill.yaml` | `./skills/` |
| Memory/Context | `.md`, `.json`, `.yaml` | `./context/` |
| Sessions | `.json` | `.nika/sessions/` |

---

## Open Questions

1. **Skill composition order?**
   - When multiple skills applied, which system prompt goes first?
   - Proposal: Array order = composition order

2. **Memory hot-reload?**
   - Should file changes during workflow reload memory?
   - Proposal: No, load once at start (predictable)

3. **Agent versioning?**
   - Should agents have version compatibility checks?
   - Proposal: Yes, `version:` field with semver

4. **Skill conflicts?**
   - What if two skills have conflicting instructions?
   - Proposal: Later skill wins (explicit override)

---

## Success Criteria

1. **Memory loading:** Files load at workflow start, accessible via templates
2. **Agent reuse:** Same agent definition works across multiple workflows
3. **Skill composition:** Multiple skills can augment an agent
4. **Chat integration:** Chat can use same memory/agent system
5. **Backward compatible:** v0.5 workflows still work without memory/agents

---

## References

- Chat-as-DAG Design: [2026-02-24-chat-as-workflow-dag.md](./2026-02-24-chat-as-workflow-dag.md)
- ADR-001: 5 Semantic Verbs
- ADR-002: YAML-First Workflow Definition
- `src/ast/agent.rs` — Current AgentParams
- `src/ast/workflow.rs` — Current Workflow struct
