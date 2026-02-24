# Agentic Architecture Research Report

**Date:** 2026-02-24
**Purpose:** Research current industry patterns for file-first agentic architecture
**Tools Used:** Perplexity AI (sonar-pro model)
**Pages Analyzed:** 9 comprehensive queries across industry patterns

---

## Summary

This research covers five key areas of modern agentic architecture (2024-2026):
1. **File-First Architecture** - Local files as persistent context and configuration
2. **SOUL Pattern** - Structured agent identity and behavior definition
3. **Boot Sequences** - Agent initialization and context loading patterns
4. **Memory/Skills Organization** - YAML-based capability and memory systems
5. **Chat-as-DAG** - Conversation flow as directed acyclic graphs

---

## 1. File-First Agentic Architecture

### Key Findings

Modern AI coding assistants (Claude Code, Manus, Cursor) leverage the **file system as persistent context and memory** across iterations. This pattern enables:
- Version-controlled agent configuration
- Project-specific behavior customization
- Reproducible agent state

### Manus AI Architecture (Best Documented)

Manus employs a **file-first** design in its iterative agent loop:

```
ANALYZE -> PLAN -> EXECUTE -> OBSERVE -> (repeat)
```

**Directory Structure:**
```
project/
├── .manus/
│   ├── SKILL.md          # Core config for reusable skills
│   ├── skills/           # Python/Bash scripts
│   ├── events/           # Event stream logs
│   └── plans/            # Current execution plans
```

**Key Features:**
- **SKILL.md**: Auto-generated from successful interactions, bundling scripts for precise triggering via `/SKILL_NAME` slash commands
- **File-based memory**: Tracks progress, event streams, and plans; appended per loop cycle
- **Context injection**: File contents read and injected into LLM prompts
- **Multi-agent setup**: Specialized modules (planning, knowledge retrieval) use files for state persistence

### Claude Code Pattern (Inferred)

```
project/
├── CLAUDE.md             # Project-level context and rules
├── .claude/
│   ├── settings.json     # Configuration
│   └── rules/            # Behavioral rules
```

**Features:**
- Project-root auto-load of CLAUDE.md
- Hierarchical context (global -> project -> directory)
- Rules applied based on current working directory

### Cursor IDE Pattern

```
project/
├── .cursorrules          # Project-specific AI rules
├── .cursor/
│   └── config            # Editor configuration
```

---

## 2. SOUL Pattern for Agent Definition

### Core Structure

The **SOUL pattern** structures AI agent definitions in Markdown/YAML files:

```markdown
# SOUL.md

## Identity
- Name: Atlas
- Role: Full-stack architect
- Personality: Pragmatic, witty

## Communication
- Lead with solution, then "why"
- Bullet points for steps

## Rules
- Ask clarifications on ambiguity
- Never deploy without approval

## Capabilities
- Tools: [file_edit, bash, mcp_tools]
- Workflow: Plan -> Act -> Verify

## Handoffs
- To: SecurityAgent when auth required
- To: Human when production changes
```

### Full SOUL Directory (Advanced)

```
.soul/
├── soul.json             # Metadata: {name, version, spec}
├── SOUL.md               # Core identity
├── RULES.md              # Constraints and boundaries
├── AGENTS.md             # Multi-agent triggers, handoffs
└── CAPABILITIES.md       # Tools, skills, workflows
```

### Framework Comparison

| Framework | Format | Key Sections | Strengths |
|-----------|--------|--------------|-----------|
| **OpenAI GPTs** | Markdown | Role, Rules, Tools | Simple, portable |
| **Claude** | Markdown (SOUL.md) | Identity, Communication, Rules | Project-root auto-load |
| **LangGraph** | YAML/JSON | Prompt, Tools, Edges (handoffs) | Graph workflows |
| **CrewAI** | YAML/Python | Role, Goal, Backstory, Handoffs | Crew orchestration |
| **AutoGen** | YAML/JSON | Description, System Msg, Functions | Multi-agent chats |

### Best Practices

1. **Modular sections**: Separate identity, capabilities, rules, workflows
2. **Conciseness**: Use bullets/lists, avoid prose
3. **Explicit constraints**: Always include safety rules and handoffs
4. **Versioning**: Add spec version, compatibility tags
5. **Multi-agent support**: Define roles, triggers, handoff protocols

---

## 3. Boot Sequence Patterns

### Optimal Boot Sequence for Agentic TUI

```
1. CLI invocation -> parse args/validate config (global + project)
2. System checks (permissions, Doctor, auto-updater)
3. Environment setup (cwd, configs); detect capabilities
4. Parallel load:
   - Tools/MCP: Promise.all([getTools(), getMcpClients()])
   - Memory hydration: history, embeddings
5. Context loading order:
   - System prompt (base behavior)
   - Project files (CLAUDE.md, SOUL.md)
   - User preferences
   - Memory/history
6. Model params init (tokens, handlers)
7. Render TUI/REPL
8. Ready for input
```

### Context Loading Order

| Order | Type | Source |
|-------|------|--------|
| 1 | System prompt | `getSystemPrompt()` - base agent behavior |
| 2 | Project files | CLAUDE.md, SOUL.md - project rules |
| 3 | User files | Global config, preferences |
| 4 | Memory | Conversation history (token-limited) |

### Tool/MCP Initialization

```rust
// Parallel initialization pattern
let (tools, mcp_clients) = tokio::join!(
    get_tools(),
    get_mcp_clients()
);

// Tool registration
register_tools([
    FileReadTool,
    BashTool,
    FileEditTool,
    MemoryReadTool,
    AgentTool,  // Nested agent spawning
    ArchitectTool,  // Conditional on config
]);
```

### Capability Detection

- **Available tools**: Via config flags (e.g., `enableArchitectTool`)
- **File system access**: Via `setup(cwd)`, permissions in "Doctor" checks
- **MCP servers**: Discovery and connection status
- **Model capabilities**: Token limits, streaming support

---

## 4. Context/Memory/Skills Organization

### Memory Hierarchy

| Level | Storage | TTL | Purpose |
|-------|---------|-----|---------|
| **Short-term** | Rolling buffer | Auto-expire | Recent interactions |
| **Working** | Prompt injection | Session | Current task context |
| **Long-term** | Vector store + files | Configurable | Facts, entities, episodes |

### YAML Schema for Skills

```yaml
# skills/research.yaml
skill:
  name: "research"
  description: "Deep research with citations"
  version: "1.0"

  triggers:
    - "research"
    - "find information"
    - "look up"

  tools:
    - web_search
    - scrape_url
    - summarize

  workflow:
    - step: analyze_query
      action: infer
    - step: search_web
      action: fetch
    - step: synthesize
      action: infer

  memory:
    type: episodic
    persist: true
    ttl_days: 30
```

### MEMORY.md Pattern

```
memories/
├── {user_id}/
│   ├── 2026-02-24-session-1.md    # Timestamped session
│   ├── 2026-02-24-session-2.md
│   └── index.json                  # Sidecar for fast lookup
```

**Memory File Format:**
```markdown
---
type: episodic
importance: 0.85
tags: [coding, rust, tui]
timestamp: 2026-02-24T10:30:00Z
embedding: [0.123, 0.456, ...]
---

# Session Summary

User worked on TUI improvements for Nika.
Key decisions:
- Adopted Solarized theme
- Implemented edit history with 500ms coalescing
```

### RAG Integration Pattern

```yaml
rag:
  knowledge_files:
    - path: docs/*.md
      type: static
      index_on_load: true

  runtime_memory:
    vector_store: chromadb
    embedding_model: text-embedding-3-small
    chunk_size: 512
    overlap: 50

  retrieval:
    top_k: 5
    min_score: 0.7
    rerank: true
```

---

## 5. Chat-as-DAG Implementations

### LangGraph StateGraph Pattern

LangGraph represents agent workflows as **stateful graphs**:

```python
from langgraph.graph import StateGraph, START, END

class AgentState(TypedDict):
    messages: list[Message]
    current_step: str

graph = StateGraph(AgentState)

# Add nodes (functions)
graph.add_node("analyze", analyze_node)
graph.add_node("plan", plan_node)
graph.add_node("execute", execute_node)
graph.add_node("verify", verify_node)

# Add edges (flow)
graph.add_edge(START, "analyze")
graph.add_edge("analyze", "plan")
graph.add_conditional_edges("plan", router, {
    "execute": "execute",
    "clarify": END
})
graph.add_edge("execute", "verify")
graph.add_edge("verify", END)

workflow = graph.compile()
```

### Conditional Branching

```python
def router(state: AgentState) -> str:
    if state.get("needs_clarification"):
        return "clarify"
    if state.get("needs_revision"):
        return "revise"
    return "execute"

graph.add_conditional_edges(
    "plan",
    router,
    {"clarify": END, "revise": "plan", "execute": "execute"}
)
```

### Message Flow Pattern

```python
from langgraph.graph.message import add_messages

class AgentState(TypedDict):
    messages: Annotated[list[Message], add_messages]

def agent_node(state: AgentState) -> dict:
    # LLM call appends new message
    response = llm.invoke(state["messages"])
    return {"messages": [response]}
```

### Task Decomposition Pattern

From Claude Code's approach:

```yaml
# Decomposed task structure
task:
  id: "complex-feature"
  decomposition:
    - id: step-1
      description: "Extract data from source"
      pass_criteria: "Data file exists with valid JSON"
      status: pending
    - id: step-2
      description: "Transform data"
      depends_on: [step-1]
      pass_criteria: "Transformed file matches schema"
      status: pending
    - id: step-3
      description: "Update target system"
      depends_on: [step-2]
      pass_criteria: "API returns 200 OK"
      status: pending
```

### Plan-Act-Observe Loop

```
1. PLAN: Generate/refine atomic task list
2. ACT: Execute one skill with tools/memory
3. OBSERVE: Check pass/fail criteria
4. REFINE: On fail -> debug; On pass -> next task
5. LOOP: Until goal met or manual intervention
```

---

## 6. MCP Server Patterns

### Architecture

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│  MCP Host   │────>│ MCP Client  │────>│ MCP Server  │
│ (Claude)    │     │ (per server)│     │ (tools)     │
└─────────────┘     └─────────────┘     └─────────────┘
```

### Server Lifecycle

1. **Initialization**: Client connects via transport (Stdio/HTTP)
2. **Capability negotiation**: Exchange supported features
3. **Ready state**: Handle tool calls, resources, notifications
4. **Termination**: Graceful close

### Multi-Server Pattern

```yaml
mcp:
  servers:
    novanet:
      command: node
      args: ["/path/to/novanet-mcp/dist/index.js"]
      transport: stdio

    filesystem:
      command: npx
      args: ["-y", "@modelcontextprotocol/server-filesystem"]
      transport: stdio

    remote-api:
      url: "https://api.example.com/mcp"
      transport: http
      auth:
        type: bearer
        token: $API_KEY
```

### Robust Server Patterns

| Pattern | Description |
|---------|-------------|
| **Transport Abstraction** | Support Stdio (local) and HTTP (remote) |
| **JSON-RPC Strictness** | Use 2.0 for all messages |
| **Capability Negotiation** | Declare features early, adjust dynamically |
| **Secure Auth** | OAuth/bearer tokens on HTTP |
| **Lifecycle Management** | init -> negotiate -> ready -> terminate |
| **Notification-Driven** | Push logs/updates without polling |

---

## 7. TUI Patterns for AI Agents

### Ratatui Architecture (Rust)

```rust
// Three-layer pattern
pub struct AgentApp {
    // Layer 1: State
    db: Database,
    agent_context: Vec<Message>,
    streaming_buffer: String,

    // Layer 2: UI State
    list_state: ListState,
    input_mode: InputMode,
    scroll_offset: u16,

    // Layer 3: Rendering (pure functions)
    // render_main_pane(), render_sidebar(), etc.
}

pub enum InputMode {
    Normal,      // Navigation
    Editing,     // Text input
    CommandMode, // Command palette
}
```

### Split-Pane Layout

```rust
let chunks = Layout::default()
    .direction(Direction::Vertical)
    .constraints([
        Constraint::Length(3),    // Status bar
        Constraint::Min(1),       // Main area
        Constraint::Length(3),    // Input
    ])
    .split(f.size());

let main_chunks = Layout::default()
    .direction(Direction::Horizontal)
    .constraints([
        Constraint::Percentage(70),  // Agent response
        Constraint::Percentage(30),  // Context sidebar
    ])
    .split(chunks[1]);
```

### Streaming Response Pattern

```rust
// Buffer streaming tokens
fn handle_stream_chunk(&mut self, chunk: &str) {
    self.streaming_buffer.push_str(chunk);
    // Render will pick up on next frame
}

// Frame-based rendering (immediate mode)
fn render(&self, frame: &mut Frame) {
    let content = Paragraph::new(self.streaming_buffer.as_str())
        .wrap(Wrap { trim: false })
        .scroll((self.scroll_offset, 0));
    frame.render_widget(content, main_area);
}
```

### Best Practices

1. **Architecture**: Separate database, logic, and rendering layers
2. **Performance**: Buffer streaming; redraw entire UI per frame
3. **UX**: Clear visual feedback for agent states (thinking, executing, streaming)
4. **Navigation**: Vim-style bindings, modal input handling
5. **Testing**: Mock agent responses for deterministic UI tests

---

## Recommendations for Nika

Based on this research, here are recommendations for Nika's architecture:

### 1. File-First Configuration

```
.nika/
├── config.toml           # Editor, session, provider settings
├── SOUL.md               # Agent identity (optional)
├── skills/               # Reusable skill definitions
│   ├── research.skill.yaml
│   └── code-review.skill.yaml
├── sessions/             # Persistent sessions
└── memory/               # Long-term memory store
```

### 2. Boot Sequence

```rust
async fn boot() -> Result<App> {
    // 1. Parse CLI args
    let args = parse_args()?;

    // 2. Load config
    let config = Config::load()?;

    // 3. System checks
    run_doctor()?;

    // 4. Parallel init
    let (tools, mcp_clients) = tokio::join!(
        load_tools(&config),
        start_mcp_servers(&config)
    );

    // 5. Context loading
    let context = Context::new()
        .with_system_prompt()
        .with_project_files()
        .with_memory()?;

    // 6. Initialize model
    let provider = RigProvider::auto()?;

    // 7. Ready
    Ok(App::new(config, context, provider, mcp_clients))
}
```

### 3. Chat-as-DAG Integration

Extend Nika's existing DAG model to support dynamic task decomposition:

```yaml
# Dynamic decomposition during chat
task:
  id: user-request
  agent:
    prompt: $user_message
    decompose:
      strategy: semantic
      max_depth: 3
    on_complex:
      action: plan_first
      min_steps: 3
```

### 4. Memory System

```rust
pub struct Memory {
    short_term: RingBuffer<Message>,  // Last N messages
    working: HashMap<String, Value>,  // Current task context
    long_term: VectorStore,           // Persistent embeddings
}

impl Memory {
    async fn recall(&self, query: &str, top_k: usize) -> Vec<Memory> {
        self.long_term.search(query, top_k).await
    }

    async fn persist(&mut self, memory: &Memory) {
        self.long_term.insert(memory).await;
    }
}
```

---

## Sources

1. Perplexity AI search results (sonar-pro model, 2026-02-24)
2. Manus AI architecture documentation
3. LangGraph official documentation
4. CrewAI/PraisonAI patterns
5. OpenAI GPT Builder patterns
6. Claude Code inferred patterns
7. Ratatui documentation and examples

---

## Confidence Level

**High** - Multiple corroborating sources for core patterns (file-first, SOUL, boot sequence, LangGraph DAG)

**Medium** - Some inferred patterns for Claude Code and Cursor (limited direct documentation)

**Low** - Chat-as-DAG specific implementations (limited detailed technical docs available)

---

## Further Research Suggestions

1. **Claude Code source analysis** - Deeper dive into Anthropic's implementation
2. **Manus open-source** - Review actual skill/memory implementation
3. **Production MCP patterns** - Real-world multi-server orchestration
4. **Memory scaling** - Vector store vs file-based at scale
5. **Agent evaluation** - Benchmarks for agentic TUI responsiveness
