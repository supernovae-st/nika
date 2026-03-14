# AI Coding Agent Architectures & Runtimes: Technical Deep Dive

**Date:** 2026-03-14
**Author:** Claude Opus 4 (Research Agent)
**Context:** Competitive architecture analysis for Nika v0.27.0 positioning
**Scope:** 9 major systems + emerging runtimes

---

## Executive Summary

The AI coding agent landscape has evolved from simple LLM wrappers into sophisticated
runtime architectures with sandboxing, multi-step planning, persistent memory, and
multi-model orchestration. This report dissects the technical architecture of 9 major
systems and compares them to Nika's DAG workflow engine.

**Key finding:** Most coding agents converge on a single-agent-with-tools pattern
(ReAct loop) rather than multi-agent swarms. The differentiators are in context
management, sandboxing, and persistence -- not in the core agent loop itself.
Nika's YAML-first DAG approach remains architecturally unique.

---

## 1. Devin by Cognition

### Architecture

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  DEVIN ARCHITECTURE                                                             │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  User Prompt                                                                    │
│       │                                                                         │
│       ▼                                                                         │
│  ┌─────────────────┐                                                            │
│  │  Planner Agent   │  ← High-level task decomposition                          │
│  │  (Multi-step)    │                                                            │
│  └────────┬────────┘                                                            │
│           │                                                                     │
│           ▼                                                                     │
│  ┌─────────────────┐     ┌─────────────────┐                                    │
│  │  Executor Agent  │────►│  Cloud VM (EC2)  │                                   │
│  │  (ReAct Loop)    │     │  - Shell access   │                                   │
│  │                  │     │  - Browser (Playwright) │                             │
│  │                  │     │  - Code editor    │                                   │
│  │                  │◄────│  - File system    │                                   │
│  └─────────────────┘     └─────────────────┘                                    │
│           │                                                                     │
│           ▼                                                                     │
│  ┌─────────────────┐                                                            │
│  │  Knowledge Base  │  ← Session memory + playbook library                      │
│  │  (Persistent)    │                                                            │
│  └─────────────────┘                                                            │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Key Technical Details

| Aspect | Detail |
|--------|--------|
| **Orchestration** | Single agent with planner/executor phases (not multi-agent swarm) |
| **Environment** | Full cloud VM per session (firecracker/EC2 micro-VM) |
| **Tools** | Shell, browser (Playwright-based), code editor, file system |
| **Context Management** | Sliding window with summarization; maintains "knowledge base" of past actions |
| **Multi-step Planning** | Explicit plan generation before execution; plan is revisable |
| **Persistence** | Session state persists across interactions; "playbooks" for reusable patterns |
| **Multi-model** | Primarily Claude (Anthropic) backbone; model routing unclear |
| **Parallelism** | Sequential execution within a session; multiple sessions possible |
| **Memory** | Long-term memory via knowledge base; per-repo context accumulation |

### Key Innovations

1. **Full VM Environment** -- Not just shell access, but a complete development environment
   with browser, terminal, editor. The agent can browse documentation, run servers,
   inspect UIs.

2. **Playbooks** -- Reusable procedural knowledge that can be invoked across sessions.
   Similar to Nika's `include:` DAG fusion but for agent procedures rather than workflows.

3. **Asynchronous Execution** -- Users can assign tasks and return later. The agent
   continues working in the cloud VM, sending Slack/email notifications on completion.

4. **Self-Verification** -- Devin runs tests, checks compilation, verifies changes
   before presenting results. Built-in quality gates.

### Limitations

- **Closed source** -- No way to extend or customize the runtime
- **Cloud-only** -- Requires internet; no local execution option
- **Cost** -- Full VM per session is expensive ($500/mo subscription)
- **Latency** -- Cloud VM startup adds latency to new sessions
- **Context limits** -- Still bounded by LLM context windows despite summarization
- **No DAG/workflow** -- Tasks are implicitly sequential, no explicit dependency graph

### Relevance to Nika

Devin's playbook pattern maps loosely to Nika's `include:` with skill merging.
Devin's VM isolation is stronger than Nika's `exec:` sandboxing but is also
much heavier. Nika's advantage: declarative workflows are reproducible and
version-controlled; Devin's agent traces are opaque.

---

## 2. Manus AI

### Architecture

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  MANUS AI ARCHITECTURE                                                          │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  User Task                                                                      │
│       │                                                                         │
│       ▼                                                                         │
│  ┌─────────────────┐                                                            │
│  │  Planning Layer  │  ← Claude/GPT backbone for high-level decomposition       │
│  │  (Task Graph)    │                                                            │
│  └────────┬────────┘                                                            │
│           │                                                                     │
│           ▼                                                                     │
│  ┌─────────────────────────────────────────────────────────────┐                 │
│  │  Agent Orchestrator (Multi-Agent)                           │                 │
│  │  ├── Research Agent   (web browsing, data gathering)        │                 │
│  │  ├── Code Agent       (writing, editing, testing)           │                 │
│  │  ├── Analysis Agent   (data processing, visualization)      │                 │
│  │  └── Deploy Agent     (deployment, configuration)           │                 │
│  └────────────────────────┬────────────────────────────────────┘                 │
│                           │                                                     │
│                           ▼                                                     │
│  ┌─────────────────────────────────────────────────────────────┐                 │
│  │  Sandbox Environment (Docker/Cloud VM)                      │                 │
│  │  ├── Browser (headless Chrome)                              │                 │
│  │  ├── Terminal (shell access)                                │                 │
│  │  ├── Code Editor (file operations)                          │                 │
│  │  └── Persistent Storage                                     │                 │
│  └─────────────────────────────────────────────────────────────┘                 │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Key Technical Details

| Aspect | Detail |
|--------|--------|
| **Orchestration** | Multi-agent with specialized roles; central coordinator |
| **Environment** | Docker-based sandbox with persistent filesystem |
| **Tools** | Browser automation, shell, file I/O, data visualization |
| **Context Management** | Hierarchical: global context + per-agent context windows |
| **Long Tasks** | Asynchronous execution with progress streaming; can run for hours |
| **Multi-model** | Uses multiple models (Claude, GPT-4) for different agent roles |
| **Parallelism** | Multiple agents can work concurrently on subtasks |
| **Memory** | Session persistence; cross-session learning via "knowledge" |

### Key Innovations

1. **Multi-Agent Specialization** -- Different agents optimized for different task
   types (research vs coding vs deployment). This is closer to CrewAI's role pattern
   than single-agent systems.

2. **Long-Running Task Support** -- Tasks can run autonomously for hours with
   periodic status updates. The user does not need to stay connected.

3. **Browser-Based Environment** -- The entire development environment is accessible
   via browser, including real-time observation of what the agent is doing.

4. **Artifact Generation** -- Produces structured deliverables (reports, code repos,
   deployed applications) not just text responses.

### Limitations

- **Closed source** -- Proprietary architecture, no self-hosting
- **Multi-agent overhead** -- Agent coordination adds latency and token cost
- **Opaque orchestration** -- Users cannot inspect or modify the agent graph
- **No declarative format** -- Tasks described in natural language, not structured YAML
- **Limited customization** -- Cannot add custom tools or agents
- **Reproducibility** -- Same input can produce different agent paths

### Relevance to Nika

Manus's multi-agent pattern is what Nika achieves with DAG tasks + `agent:` verb.
Each Nika task with `agent:` is conceptually a specialized agent. The difference:
Nika makes the orchestration explicit in YAML; Manus infers it from the task description.
Nika's approach is more reproducible but requires more upfront design.

---

## 3. OpenHands (formerly OpenDevin)

### Architecture

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  OPENHANDS ARCHITECTURE                                                         │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐                       │
│  │  Controller   │───►│  AgentHub    │───►│  Agent       │                       │
│  │  (Orchestr.)  │    │  (Registry)  │    │  (CodeAct)   │                       │
│  └──────┬───────┘    └──────────────┘    └──────┬───────┘                       │
│         │                                        │                              │
│         ▼                                        ▼                              │
│  ┌──────────────┐                        ┌──────────────────┐                   │
│  │  EventStream  │◄──────────────────────│  Action Space     │                   │
│  │  (Obs+Act)    │                       │  ├── CmdRunAction │                   │
│  └──────┬───────┘                        │  ├── FileEditAction│                  │
│         │                                │  ├── BrowseAction  │                  │
│         ▼                                │  ├── IPythonAction │                  │
│  ┌──────────────────────────────────┐    │  └── MessageAction │                  │
│  │  Docker Sandbox (per session)    │    └──────────────────┘                   │
│  │  ├── Bash shell                  │                                           │
│  │  ├── Python (Jupyter kernel)     │                                           │
│  │  ├── Browser (Playwright)        │                                           │
│  │  └── File system (bind mount)    │                                           │
│  └──────────────────────────────────┘                                           │
│                                                                                 │
│  Runtime Options:                                                               │
│  ├── Docker (default)                                                           │
│  ├── E2B (cloud sandbox)                                                        │
│  ├── Modal (serverless)                                                         │
│  └── Remote (SSH)                                                               │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Key Technical Details

| Aspect | Detail |
|--------|--------|
| **Orchestration** | Single agent (CodeAct) with pluggable agent implementations |
| **Environment** | Docker sandbox with Jupyter kernel, browser, shell |
| **Tools** | CmdRunAction, FileEditAction, IPythonRunCellAction, BrowseInteractiveAction |
| **Context Management** | EventStream pattern -- all observations and actions logged sequentially |
| **CodeAct** | Agent can write and execute Python code as its "action" rather than using predefined tools |
| **Multi-model** | Supports any LLM via LiteLLM (OpenAI, Anthropic, local models) |
| **Parallelism** | Single-threaded agent loop; no native parallelism |
| **Memory** | Condensed context via summarization when context window fills |
| **Benchmarks** | Strong SWE-bench results (competitive with commercial agents) |

### Key Innovations

1. **CodeAct Paradigm** -- Instead of predefined tool schemas, the agent writes
   executable Python code. This gives it unlimited expressiveness. The agent can
   import libraries, define functions, run complex scripts -- all as "actions".

   ```python
   # Agent's action (not a predefined tool):
   import subprocess
   result = subprocess.run(['git', 'diff', '--stat'], capture_output=True, text=True)
   print(result.stdout)
   ```

2. **EventStream Architecture** -- All interactions are captured as an ordered stream
   of Events (observations + actions). This is very similar to Nika's EventLog
   (NDJSON traces) but used as the primary state management mechanism.

3. **Pluggable Runtime** -- The sandbox can be Docker, E2B, Modal, or SSH.
   This decouples the agent logic from the execution environment.

4. **AgentHub** -- Registry of different agent implementations. While CodeAct is
   the default, custom agents can be registered and selected per task.

5. **Open Source** -- MIT license, active community (30k+ GitHub stars), extensive
   documentation.

### Architecture Deep Dive (from GitHub)

```
openhands/
├── controller/          # Main orchestration loop
│   ├── agent_controller.py    # ReAct loop implementation
│   └── state.py               # Session state management
├── core/
│   ├── config.py              # Configuration (LLM, sandbox, agent)
│   └── schema.py              # Action/Observation type definitions
├── agenthub/
│   ├── codeact_agent/         # Default CodeAct agent
│   ├── browsing_agent/        # Web browsing specialist
│   └── delegator_agent/       # Multi-agent delegation
├── runtime/
│   ├── docker/                # Docker sandbox runtime
│   ├── e2b/                   # E2B cloud sandbox
│   ├── modal/                 # Modal serverless
│   └── remote/                # SSH remote execution
├── events/
│   ├── event.py               # Base event type
│   ├── action/                # Action events (CmdRun, FileEdit, etc.)
│   └── observation/           # Observation events (CmdOutput, FileRead, etc.)
└── llm/
    └── llm.py                 # LiteLLM wrapper for multi-model support
```

### Limitations

- **Sequential execution** -- No parallelism within a single agent session
- **Context window pressure** -- Long sessions accumulate large event streams
- **Docker dependency** -- Default runtime requires Docker; not purely native
- **No declarative workflow** -- Everything is imperative agent actions
- **Single agent focus** -- Multi-agent delegation exists but is secondary
- **No persistent memory** -- Each session starts fresh (no cross-session learning)

### Relevance to Nika

OpenHands' EventStream is architecturally close to Nika's EventLog. The CodeAct
pattern (agent writes code as actions) could be a future Nika verb. OpenHands lacks
DAG support, declarative workflows, and MCP integration -- all Nika strengths.
However, OpenHands' sandbox isolation is more robust than Nika's current exec: security.

---

## 4. OpenAI Codex CLI

### Architecture

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  OPENAI CODEX CLI ARCHITECTURE                                                  │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  User Prompt (CLI)                                                              │
│       │                                                                         │
│       ▼                                                                         │
│  ┌─────────────────┐                                                            │
│  │  Codex Agent     │  ← o3/o4-mini backbone with Responses API                 │
│  │  (Single Agent)  │                                                            │
│  └────────┬────────┘                                                            │
│           │                                                                     │
│           ▼                                                                     │
│  ┌─────────────────────────────────────────────────────────────┐                 │
│  │  Tool Layer                                                 │                 │
│  │  ├── shell (command execution)                              │                 │
│  │  ├── file_read / file_write                                 │                 │
│  │  ├── file_edit (apply_diff format)                          │                 │
│  │  └── browser (optional)                                     │                 │
│  └────────────────────────┬────────────────────────────────────┘                 │
│                           │                                                     │
│                           ▼                                                     │
│  ┌─────────────────────────────────────────────────────────────┐                 │
│  │  Sandbox (Network-Disabled by Default)                      │                 │
│  │  ├── macOS: Apple Seatbelt (sandbox-exec)                   │                 │
│  │  ├── Linux: Docker container or Bubblewrap                  │                 │
│  │  └── Network: Disabled unless --full-auto                   │                 │
│  └─────────────────────────────────────────────────────────────┘                 │
│                                                                                 │
│  Approval Modes:                                                                │
│  ├── suggest    (read-only, no writes)                                          │
│  ├── auto-edit  (file writes OK, shell needs approval)                          │
│  └── full-auto  (everything auto-approved, network enabled)                     │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Key Technical Details

| Aspect | Detail |
|--------|--------|
| **Orchestration** | Single agent with ReAct loop; no multi-agent |
| **Environment** | Local filesystem + sandbox; network disabled by default |
| **Tools** | shell, file_read, file_write, file_edit (apply_diff) |
| **Context Management** | Automatic repo-level context via file discovery; git-aware |
| **Multi-model** | OpenAI models only (o3, o4-mini); Responses API |
| **Parallelism** | None -- sequential tool calls |
| **Memory** | Project-level instructions via `AGENTS.md` files |
| **Sandbox** | Apple Seatbelt (macOS) or Docker/Bubblewrap (Linux) |

### Key Innovations

1. **Seatbelt Sandbox** -- Uses macOS's native `sandbox-exec` for lightweight
   process isolation. No Docker required on macOS. Network is disabled by default,
   preventing data exfiltration.

2. **apply_diff Format** -- Custom diff format for file edits that is more
   LLM-friendly than unified diff. The model generates structured edit operations
   rather than trying to produce correct unified diffs.

3. **Three Approval Modes** -- Graduated trust model from suggest (read-only)
   to full-auto (everything approved). This maps well to different use cases
   (exploration vs autonomous coding).

4. **AGENTS.md Convention** -- Project-level instructions file (similar to
   CLAUDE.md) that provides repo-specific context to the agent. Hierarchical:
   root AGENTS.md + subdirectory AGENTS.md files.

5. **Open Source** -- Released as open-source Node.js CLI tool. Community can
   extend and customize.

### Limitations

- **OpenAI models only** -- No support for Claude, Mistral, or local models
- **No workflow/DAG** -- Pure conversational agent, no structured workflow support
- **No MCP support** -- Predefined tool set, cannot connect to MCP servers
- **Sequential execution** -- No parallelism
- **Limited persistence** -- No cross-session memory beyond AGENTS.md
- **No TUI** -- Basic terminal output, no interactive UI
- **No streaming** -- Waits for complete responses before displaying

### Relevance to Nika

The AGENTS.md convention parallels Nika's context: file loading. The approval
modes map to Nika's potential permission system. The sandbox approach is lighter
than Docker but platform-specific. Nika's advantage: multi-model support, DAG
execution, MCP integration, structured YAML workflows.

---

## 5. Claude Code by Anthropic

### Architecture

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  CLAUDE CODE ARCHITECTURE                                                       │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  User Prompt (CLI / VS Code Extension)                                          │
│       │                                                                         │
│       ▼                                                                         │
│  ┌─────────────────┐                                                            │
│  │  Agent SDK       │  ← Claude Opus/Sonnet backbone                            │
│  │  (Single Agent)  │                                                            │
│  │  ├── System prompt (context-aware)                                           │
│  │  ├── Extended thinking (o1-style reasoning)                                  │
│  │  └── Tool use loop (ReAct)                                                   │
│  └────────┬────────┘                                                            │
│           │                                                                     │
│           ▼                                                                     │
│  ┌─────────────────────────────────────────────────────────────┐                 │
│  │  Tool Layer (18 builtin tools)                              │                 │
│  │  ├── Read / Write / Edit (file operations)                  │                 │
│  │  ├── Bash (shell execution)                                 │                 │
│  │  ├── Glob / Grep (file search)                              │                 │
│  │  ├── TodoWrite (task tracking)                              │                 │
│  │  ├── AskUserQuestion (interactive clarification)            │                 │
│  │  ├── MCP tools (via configured MCP servers)                 │                 │
│  │  └── Task (sub-agent delegation)                            │                 │
│  └────────────────────────┬────────────────────────────────────┘                 │
│                           │                                                     │
│                           ▼                                                     │
│  ┌─────────────────────────────────────────────────────────────┐                 │
│  │  Context System                                             │                 │
│  │  ├── CLAUDE.md (project instructions, hierarchical)         │                 │
│  │  ├── Git-aware context (diff, log, status)                  │                 │
│  │  ├── File content (on-demand reading)                       │                 │
│  │  ├── MCP server context                                     │                 │
│  │  └── Conversation history (with compaction)                 │                 │
│  └─────────────────────────────────────────────────────────────┘                 │
│                                                                                 │
│  Permission System:                                                             │
│  ├── Read-only (auto-approved)                                                  │
│  ├── Write operations (user approval or allowlist)                              │
│  ├── Bash commands (approval with patterns)                                     │
│  └── Hooks (pre/post tool execution)                                            │
│                                                                                 │
│  Sub-Agent Support:                                                             │
│  ├── Task tool (spawn sub-agent with scoped context)                            │
│  └── Parallel task execution (multiple sub-agents)                              │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Key Technical Details

| Aspect | Detail |
|--------|--------|
| **Orchestration** | Single agent with sub-agent delegation via Task tool |
| **Environment** | Local filesystem; no sandboxing by default |
| **Tools** | 18 builtin + MCP tools; Read, Write, Edit, Bash, Glob, Grep, TodoWrite, Task |
| **Context Management** | CLAUDE.md hierarchy + git awareness + on-demand file reading |
| **Multi-step** | Extended thinking for complex reasoning; TodoWrite for task tracking |
| **Multi-model** | Claude models only (Opus, Sonnet); model selection configurable |
| **Parallelism** | Sub-agent parallelism via Task tool; main agent is sequential |
| **Memory** | CLAUDE.md as persistent project memory; no cross-project memory |
| **MCP** | Full MCP client support for external tool servers |

### Key Innovations

1. **CLAUDE.md System** -- Hierarchical project instructions that persist across
   sessions. Root CLAUDE.md + nested subdirectory files. This is the closest
   thing to a project "memory" without a database.

2. **TodoWrite Pattern** -- Agent self-organizes by creating visible task lists.
   Not execution management, but transparency into agent planning. Single
   in_progress constraint forces sequential focus.

3. **Task Tool (Sub-Agents)** -- Can spawn sub-agents with scoped file access.
   Each sub-agent gets a focused context (specific files/directories) and
   returns results to the parent agent. Enables divide-and-conquer.

4. **Hooks System** -- Pre/post tool execution hooks for validation, formatting,
   and security. Similar to git hooks but for agent tool calls.

5. **Extended Thinking** -- Claude can reason step-by-step before acting,
   producing a thinking trace that is visible but separate from the final response.
   This is analogous to chain-of-thought prompting but built into the model.

6. **Permission Model** -- Granular tool permissions with allowlists, patterns,
   and approval flows. The agent asks permission for destructive operations.

### Limitations

- **Claude models only** -- No multi-model support
- **No workflow/DAG** -- Conversational agent, no structured orchestration
- **No sandboxing** -- Bash commands run in user's environment (risky in full-auto)
- **Sequential main loop** -- Only parallelism is via Task sub-agents
- **No persistent state** -- No checkpoint/resume for long-running tasks
- **Cost** -- Heavy token usage for complex tasks; extended thinking is expensive
- **No structured output** -- No JSON schema enforcement on agent outputs

### Relevance to Nika

Claude Code's CLAUDE.md is analogous to Nika's `context:` file loading.
The Task tool maps to Nika's `spawn_agent`. The TodoWrite pattern could
inform Nika's TUI progress display. The hooks system could inspire Nika's
pre/post task hooks. Nika advantages: DAG execution, multi-model, YAML
workflows, MCP-native, structured output enforcement.

---

## 6. SWE-Agent (Princeton)

### Architecture

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  SWE-AGENT ARCHITECTURE                                                         │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  GitHub Issue / Bug Report                                                      │
│       │                                                                         │
│       ▼                                                                         │
│  ┌─────────────────┐                                                            │
│  │  SWE-Agent       │  ← LLM backbone (GPT-4, Claude, etc.)                    │
│  │  (ReAct Loop)    │                                                            │
│  └────────┬────────┘                                                            │
│           │                                                                     │
│           ▼                                                                     │
│  ┌─────────────────────────────────────────────────────────────┐                 │
│  │  Agent-Computer Interface (ACI)                             │                 │
│  │  ├── File Viewer (scrollable, contextual)                   │                 │
│  │  ├── File Editor (line-based with linting)                  │                 │
│  │  ├── Search Tools (find_file, search_dir, search_file)      │                 │
│  │  ├── Navigation (open, goto, scroll_up/down)                │                 │
│  │  └── Submit (create patch for evaluation)                   │                 │
│  └────────────────────────┬────────────────────────────────────┘                 │
│                           │                                                     │
│                           ▼                                                     │
│  ┌─────────────────────────────────────────────────────────────┐                 │
│  │  Docker Container (per issue)                               │                 │
│  │  ├── Cloned repository                                      │                 │
│  │  ├── Test environment                                       │                 │
│  │  └── Patch generation                                       │                 │
│  └─────────────────────────────────────────────────────────────┘                 │
│                                                                                 │
│  SWE-bench Evaluation:                                                          │
│  ├── SWE-bench Lite:  ~26-33% resolve rate (GPT-4/Claude)                       │
│  ├── SWE-bench Full:  ~12-18% resolve rate                                      │
│  └── SWE-bench Verified: Higher accuracy subset                                 │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Key Technical Details

| Aspect | Detail |
|--------|--------|
| **Orchestration** | Single agent with ReAct loop |
| **Environment** | Docker container per issue; isolated repo clone |
| **Tools** | Custom ACI (Agent-Computer Interface); NOT generic shell |
| **Context Management** | Sliding window file viewer; search-then-read pattern |
| **Multi-model** | Supports GPT-4, Claude, open models via configurable LLM backend |
| **Parallelism** | None within agent; batch evaluation across issues is parallel |
| **Memory** | No persistent memory; each issue starts fresh |
| **Benchmarks** | Standard SWE-bench evaluation framework |

### Key Innovations

1. **Agent-Computer Interface (ACI)** -- Custom tool interface designed specifically
   for software engineering tasks. Instead of giving the agent raw shell access,
   SWE-Agent provides structured commands (open, edit, search, scroll) that are
   optimized for LLM interaction.

   ```
   open src/parser.py          # Opens file with line numbers
   goto 150                    # Jump to line 150
   edit 155:160                # Edit lines 155-160
   search_dir "parse_token"    # Search across directory
   ```

2. **Linting Guard** -- After each edit, the agent's changes are linted.
   If linting fails, the agent is informed and must fix the issue before
   proceeding. This prevents accumulation of syntax errors.

3. **SWE-bench Standard** -- Established the benchmark that all coding agents
   are now measured against. 2,294 real GitHub issues from popular Python repos.

4. **Configurable Agent** -- The agent behavior is configured via YAML
   "config files" that define the system prompt, tools, and guardrails.
   This is a lightweight form of workflow configuration.

### Benchmark Results (as of early 2025)

| System | SWE-bench Lite | SWE-bench Full |
|--------|----------------|----------------|
| SWE-Agent + GPT-4 | ~26% | ~12% |
| SWE-Agent + Claude 3.5 | ~33% | ~18% |
| Devin | ~14% (early) → higher | N/A |
| OpenHands CodeAct | ~28% | ~15% |
| AutoCodeRover | ~22% | N/A |
| Aider | ~26% | N/A |

*Note: Benchmarks evolve rapidly; these figures are approximate as of early 2025.*

### Limitations

- **Benchmark-optimized** -- The ACI is designed for SWE-bench tasks (bug fixes);
  less suited for greenfield development or refactoring
- **No multi-file editing** -- Sequential file operations; no batch edits
- **No persistence** -- Each issue is independent; no learning across issues
- **No workflow** -- Single-task focus, no multi-step orchestration
- **Python-centric** -- SWE-bench is Python repos; ACI assumptions may not generalize
- **Research focus** -- Not designed as a production tool

### Relevance to Nika

SWE-Agent's ACI concept (domain-specific tool interface) aligns with Nika's
5 semantic verbs. Both constrain the agent's action space for better results.
SWE-Agent's configurable agent YAML is a simpler version of Nika's workflow YAML.
Nika could adopt the linting-guard pattern for exec: tasks.

---

## 7. Aider

### Architecture

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  AIDER ARCHITECTURE                                                             │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  User Prompt (CLI chat interface)                                               │
│       │                                                                         │
│       ▼                                                                         │
│  ┌─────────────────┐                                                            │
│  │  Chat Session    │  ← Conversational loop with file context                  │
│  │  (Editor Mode)   │                                                            │
│  └────────┬────────┘                                                            │
│           │                                                                     │
│           ▼                                                                     │
│  ┌─────────────────────────────────────────────────────────────┐                 │
│  │  Edit Formats                                               │                 │
│  │  ├── whole    (send entire file, get entire file back)      │                 │
│  │  ├── diff     (unified diff format)                         │                 │
│  │  ├── udiff    (universal diff)                              │                 │
│  │  └── editor-diff (search/replace blocks)                   │                 │
│  └────────────────────────┬────────────────────────────────────┘                 │
│                           │                                                     │
│                           ▼                                                     │
│  ┌─────────────────────────────────────────────────────────────┐                 │
│  │  Context Management                                         │                 │
│  │  ├── /add files (explicit file context)                     │                 │
│  │  ├── /read-only files (reference without editing)           │                 │
│  │  ├── Repo map (tree-sitter AST summary of entire repo)     │                 │
│  │  ├── Git integration (auto-commit each change)              │                 │
│  │  └── .aider* config files                                   │                 │
│  └─────────────────────────────────────────────────────────────┘                 │
│                                                                                 │
│  Multi-Model Support:                                                           │
│  ├── OpenAI (GPT-4, GPT-4o, o1, o3)                                            │
│  ├── Anthropic (Claude 3.5, Claude 3 Opus)                                      │
│  ├── Google (Gemini)                                                            │
│  ├── DeepSeek (V3, R1)                                                          │
│  ├── Ollama (local models)                                                      │
│  └── Any OpenAI-compatible API                                                  │
│                                                                                 │
│  Architect Mode:                                                                │
│  ├── Architect model (thinks about approach)                                    │
│  └── Editor model (implements changes)                                          │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Key Technical Details

| Aspect | Detail |
|--------|--------|
| **Orchestration** | Single agent; Architect mode uses two models in sequence |
| **Environment** | Local filesystem; no sandboxing |
| **Tools** | File editing only; no shell execution, no browser |
| **Context Management** | Repo map (AST-based) + explicit file context + git history |
| **Multi-file Edits** | Multiple files in single prompt; search/replace blocks |
| **Multi-model** | Excellent multi-model support; any OpenAI-compatible API |
| **Parallelism** | None |
| **Memory** | Git history as implicit memory; `.aider*` config files |
| **Git Integration** | Auto-commits each change; easy undo via git revert |

### Key Innovations

1. **Repo Map** -- Uses tree-sitter to build an AST-level summary of the entire
   repository. This gives the model awareness of all functions, classes, and
   imports without sending full file contents. Dramatically reduces token usage.

   ```
   src/
   ├── parser.py
   │   ├── class Parser
   │   │   ├── def parse(self, input)
   │   │   ├── def tokenize(self, text)
   │   │   └── def validate(self, ast)
   │   └── class Token
   │       └── def __init__(self, type, value)
   └── main.py
       └── def main()
   ```

2. **Edit Formats** -- Multiple edit format strategies optimized for different
   models. The "editor-diff" format uses search/replace blocks:

   ```
   <<<<<<< SEARCH
   def old_function():
       return None
   =======
   def new_function():
       return 42
   >>>>>>> REPLACE
   ```

3. **Architect Mode** -- Two-model pipeline where a "thinking" model (e.g., o1)
   designs the approach, and an "editing" model (e.g., GPT-4) implements it.
   This separates planning from execution.

4. **Git-Native Workflow** -- Every edit is auto-committed to git. Users can
   see exactly what changed, revert easily, and the git history serves as
   an implicit audit trail.

5. **Universal Model Support** -- Works with any OpenAI-compatible API,
   including local models via Ollama. Model-specific edit formats are
   auto-selected based on model capabilities.

### Limitations

- **No tool execution** -- Cannot run shell commands, tests, or servers
- **No agent loop** -- Single prompt-response cycle (not ReAct)
- **No browser** -- Cannot browse documentation or websites
- **No workflow/DAG** -- Purely conversational file editing
- **No MCP** -- No protocol-based tool integration
- **No sandboxing** -- Edits directly on filesystem (though git provides undo)
- **No TUI beyond chat** -- Simple terminal chat interface

### Relevance to Nika

Aider's repo map (tree-sitter AST) is an excellent context management strategy
that Nika could adopt for providing repo awareness to agents. The multi-model
support and architect mode directly parallel Nika's multi-provider approach.
The search/replace edit format is similar to Nika's `nika:edit` tool.
Nika's advantages: full agent loop, DAG execution, MCP tools, shell execution,
structured workflows.

---

## 8. Cursor / Windsurf

### Cursor Architecture

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  CURSOR ARCHITECTURE                                                            │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  VS Code Fork (Electron)                                                        │
│       │                                                                         │
│       ├── Tab (Autocomplete)                                                    │
│       │   └── Speculative edits + multi-line suggestions                        │
│       │                                                                         │
│       ├── Cmd+K (Inline Edit)                                                   │
│       │   └── Context-aware inline code generation                              │
│       │                                                                         │
│       ├── Chat (Sidebar Agent)                                                  │
│       │   ├── Codebase-aware Q&A                                                │
│       │   ├── Multi-file context                                                │
│       │   └── Apply edits from chat                                             │
│       │                                                                         │
│       └── Composer (Agent Mode)                                                 │
│           ├── Multi-file editing agent                                          │
│           ├── Terminal command execution                                        │
│           ├── Error detection and auto-fix                                      │
│           └── Background agent (runs autonomously)                              │
│                                                                                 │
│  Context Engine:                                                                │
│  ├── Codebase indexing (embeddings-based)                                       │
│  ├── @-mentions (@file, @folder, @web, @docs, @git)                            │
│  ├── .cursorrules (project-level instructions)                                  │
│  └── Automatic context retrieval                                                │
│                                                                                 │
│  Multi-Model:                                                                   │
│  ├── GPT-4o, GPT-4, o1, o3                                                     │
│  ├── Claude 3.5 Sonnet, Claude 3 Opus                                          │
│  ├── cursor-small (custom fine-tuned model)                                    │
│  └── Custom API endpoints                                                       │
│                                                                                 │
│  Background Agent (2025+):                                                      │
│  ├── Runs in cloud sandbox                                                      │
│  ├── Full terminal + file access                                                │
│  ├── Can run tests, build projects                                              │
│  └── Notifies user on completion                                                │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Windsurf Architecture

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  WINDSURF (CODEIUM) ARCHITECTURE                                                │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  VS Code Fork (Electron)                                                        │
│       │                                                                         │
│       ├── Autocomplete (Supercomplete)                                          │
│       │   └── Context-aware, multi-line, AST-aware                              │
│       │                                                                         │
│       ├── Cascade (Agent Mode)                                                  │
│       │   ├── Multi-step reasoning                                              │
│       │   ├── Multi-file editing                                                │
│       │   ├── Terminal integration                                              │
│       │   ├── MCP tool support                                                  │
│       │   └── Proactive suggestions                                             │
│       │                                                                         │
│       └── Flows (Background Agent - 2025+)                                      │
│           ├── Runs tasks asynchronously                                          │
│           ├── Multi-step workflows                                              │
│           ├── Can branch and merge code                                          │
│           └── Cloud-based execution                                             │
│                                                                                 │
│  Context Engine:                                                                │
│  ├── Codebase indexing (embedding + graph-based)                                │
│  ├── "Memories" (persistent context from past sessions)                         │
│  ├── .windsurfrules (project instructions)                                      │
│  └── Cascade context propagation                                                │
│                                                                                 │
│  Multi-Model:                                                                   │
│  ├── GPT-4o, Claude 3.5 Sonnet                                                 │
│  ├── Windsurf proprietary models                                                │
│  └── Custom API endpoints                                                       │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Key Technical Details (Both)

| Aspect | Cursor | Windsurf |
|--------|--------|----------|
| **Orchestration** | Single agent (Composer) | Single agent (Cascade) |
| **Environment** | VS Code fork + terminal | VS Code fork + terminal |
| **Tools** | File edit, terminal, browser (limited) | File edit, terminal, MCP |
| **Context** | Embeddings index + @mentions | Embeddings + graph + memories |
| **Multi-model** | Excellent (GPT-4, Claude, custom) | Good (GPT-4, Claude, proprietary) |
| **Background** | Background Agent (cloud sandbox) | Flows (cloud-based) |
| **MCP** | Not yet native | MCP support in Cascade |
| **Parallelism** | Sequential (main), parallel (background) | Sequential (main) |
| **Memory** | .cursorrules (per-project) | Memories (cross-session) |

### Key Innovations

1. **IDE-Native Agent** -- Both embed the agent directly in the code editor,
   giving it access to LSP (language server), debugger, terminal, and all
   IDE features. This is fundamentally different from CLI agents.

2. **Codebase Indexing** -- Both build embeddings-based indexes of the entire
   codebase for retrieval. Cursor uses vector search; Windsurf adds graph-based
   relationships between files.

3. **Speculative Edits** -- Both can suggest multi-line completions that consider
   the broader codebase context, not just the current file.

4. **Background Agents** -- Both are developing cloud-based background agents
   that can run autonomously (similar to Devin's async model but within the
   IDE ecosystem). These represent a convergence with Devin's approach.

5. **Windsurf Memories** -- Persistent cross-session context that accumulates
   knowledge about the project over time. This is more advanced than
   Cursor's .cursorrules or Claude Code's CLAUDE.md.

6. **MCP in Windsurf** -- Windsurf's Cascade agent supports MCP tools,
   allowing connection to external tool servers. This makes Windsurf the
   first IDE-integrated agent with MCP support.

### Limitations

- **IDE-locked** -- Require their specific VS Code fork; cannot run headlessly
- **Closed source** -- Proprietary agent logic, no customization
- **Subscription model** -- Monthly fees ($20-40/mo) for full features
- **No workflow format** -- Conversational only, no structured task definition
- **No DAG** -- Sequential task execution
- **Platform dependent** -- Desktop application, not server/CI compatible

### Relevance to Nika

Cursor and Windsurf represent the IDE-first approach vs Nika's workflow-first
approach. They are complementary rather than competitive. Nika workflows could
be triggered from within Cursor/Windsurf via MCP. Windsurf's MCP support
makes it a potential Nika client. The "memories" pattern from Windsurf could
inform Nika's cross-session persistence.

---

## 9. Emerging 2025-2026 Agent Runtimes

### Amazon Q Developer Agent

```
Orchestration: Single agent with specialized modes
Environment:   AWS Cloud9 / VS Code extension / CLI
Tools:         /dev (code generation), /transform (upgrades), /review, /test
Context:       AWS service integration, CodeWhisperer index
Multi-model:   Amazon proprietary models
Parallelism:   None
Memory:        Workspace-level context
```

**Innovation:** Deep AWS service integration. Can generate and deploy
infrastructure code (CDK, CloudFormation) with awareness of existing AWS resources.
The `/transform` capability can upgrade entire Java codebases (e.g., Java 8 to 17).

### Google Jules

```
Orchestration: Single agent (async)
Environment:   Cloud-based VM
Tools:         Code editing, testing, git operations
Context:       Repository-aware
Multi-model:   Gemini backbone
Parallelism:   Async task execution
Memory:        Per-task, no cross-task
```

**Innovation:** GitHub-integrated asynchronous agent. Assigns issues to Jules,
which creates PRs autonomously. Similar to Devin's async model but tightly
integrated with GitHub workflow.

### Anthropic Agent SDK / Claude Agent Framework

```
Orchestration: Single agent with tool delegation
Environment:   Host process (no sandbox)
Tools:         User-defined via MCP + builtin
Context:       Conversation history + MCP resources
Multi-model:   Claude only
Parallelism:   Via sub-agent tasks
Memory:        Session-scoped
```

**Innovation:** First-party agent SDK from a model provider. Designed to be
the reference implementation for building Claude-powered agents. The SDK
standardizes the agent loop, tool use, and context management patterns.

### Cline (VS Code Extension, Open Source)

```
Orchestration: Single agent (ReAct loop)
Environment:   VS Code extension + terminal
Tools:         File read/write/edit, terminal, browser
Context:       Active file + @mentions + codebase search
Multi-model:   Any via API key (OpenAI, Anthropic, etc.)
Parallelism:   None
Memory:        Per-session
MCP:           Full MCP client support
```

**Innovation:** Open-source VS Code agent with MCP support. Community-driven
alternative to Cursor/Windsurf. Supports any LLM provider. The MCP integration
makes it extensible with custom tool servers.

### Goose (Block/Square, Open Source)

```
Orchestration: Single agent with extension system
Environment:   Local + optional sandbox
Tools:         File operations, shell, developer tools
Context:       Session-based + .goosehints files
Multi-model:   Any via provider plugins
Parallelism:   None
Memory:        Session-scoped
MCP:           Full MCP support (extensions = MCP servers)
```

**Innovation:** MCP-native architecture where every extension is an MCP server.
This is architecturally close to Nika's MCP-first design. The extension model
is clean and composable.

### Roo Code (Open Source, VS Code)

```
Orchestration: Single agent with "modes" (Code, Architect, Ask, Debug)
Environment:   VS Code extension
Tools:         File operations, terminal, browser
Context:       Codebase indexing + mode-specific prompts
Multi-model:   Any via API key
Parallelism:   None
Memory:        Per-session + custom instructions
MCP:           Yes
```

**Innovation:** Mode-based agent with different system prompts and tool access
per mode. Architect mode plans without editing; Code mode implements; Debug mode
focuses on error resolution. Custom modes can be defined.

### Bolt.new / v0 / Lovable (Browser-Based)

```
Orchestration: Single agent for full-stack generation
Environment:   WebContainer (browser-based Node.js)
Tools:         File system, terminal, preview (all in-browser)
Context:       Conversation history + generated code
Multi-model:   Provider-specific (Anthropic for Bolt, OpenAI for v0)
Parallelism:   None
Memory:        Project-scoped
```

**Innovation:** Entire development environment runs in the browser via
WebContainers (Stackblitz technology). No Docker, no cloud VM -- the Node.js
runtime is compiled to WebAssembly and runs in the browser tab. Instant
preview of generated applications.

---

## Comparative Analysis

### Orchestration Models

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  ORCHESTRATION MODEL COMPARISON                                                 │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  Single Agent + Tools (ReAct Loop):                                             │
│  ├── Claude Code         ← Most systems converge here                           │
│  ├── Codex CLI                                                                  │
│  ├── SWE-Agent                                                                  │
│  ├── Aider                                                                      │
│  ├── Cursor Composer                                                            │
│  ├── Windsurf Cascade                                                           │
│  ├── Cline                                                                      │
│  └── Goose                                                                      │
│                                                                                 │
│  Single Agent + Sub-Agents:                                                     │
│  ├── Claude Code (Task tool)                                                    │
│  ├── Devin (planner + executor)                                                 │
│  └── OpenHands (delegator agent)                                                │
│                                                                                 │
│  Multi-Agent Specialized:                                                       │
│  ├── Manus AI (research + code + analysis + deploy agents)                      │
│  └── (rare for coding agents -- more common in general AI agents)               │
│                                                                                 │
│  DAG Workflow (Declarative):                                                    │
│  └── Nika ← UNIQUE POSITION                                                    │
│                                                                                 │
│  Conclusion: Almost all coding agents use single-agent + tools.                 │
│  Multi-agent is rare because coordination overhead > benefits for coding.       │
│  Nika's DAG approach is fundamentally different and unique.                     │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Context Management Strategies

| System | Strategy | Repo Awareness | Persistent |
|--------|----------|----------------|------------|
| **Devin** | VM state + knowledge base | Full repo access | Cross-session |
| **Manus** | Hierarchical agent contexts | Full repo access | Cross-session |
| **OpenHands** | EventStream + summarization | Docker mount | Per-session |
| **Codex CLI** | AGENTS.md + file discovery | Git-aware | AGENTS.md only |
| **Claude Code** | CLAUDE.md + on-demand read | Git-aware | CLAUDE.md only |
| **SWE-Agent** | Sliding window + search | Docker mount | None |
| **Aider** | Repo map (tree-sitter AST) | Full AST | .aider config |
| **Cursor** | Embeddings index + @mentions | Indexed | .cursorrules |
| **Windsurf** | Embeddings + graph + memories | Indexed | Memories (cross-session) |
| **Nika** | context: files + MCP + use: bindings | Configurable | EventLog traces |

**Insight:** Aider's tree-sitter repo map is the most efficient approach for
giving models repo-level awareness without sending full file contents.
Windsurf's cross-session "memories" is the most advanced persistence.
Nika's MCP-based context is the most composable (can pull from any MCP server).

### Multi-Model Support

| System | Models | Selection Method |
|--------|--------|------------------|
| **Devin** | Claude (primary) | Fixed |
| **Manus** | Claude + GPT-4 | Per-agent role |
| **OpenHands** | Any (LiteLLM) | Configurable |
| **Codex CLI** | OpenAI only | Fixed |
| **Claude Code** | Claude only | Configurable (Opus/Sonnet) |
| **SWE-Agent** | Any (configurable) | Configurable |
| **Aider** | Excellent (any OpenAI-compatible) | Per-session + architect mode |
| **Cursor** | GPT-4, Claude, custom | Per-request |
| **Windsurf** | GPT-4, Claude, proprietary | Per-request |
| **Nika** | 6 providers + native local | Per-task in YAML |

**Insight:** Aider has the best multi-model story for CLI tools. Nika's per-task
model selection in YAML is unique -- no other system allows different models for
different steps in the same workflow.

### Parallelism Capabilities

| System | Type | Details |
|--------|------|---------|
| **Devin** | Multi-session | Multiple Devin sessions in parallel |
| **Manus** | Multi-agent | Specialized agents work concurrently |
| **OpenHands** | None | Sequential agent loop |
| **Codex CLI** | None | Sequential |
| **Claude Code** | Sub-agent | Task tool spawns parallel sub-agents |
| **SWE-Agent** | Batch | Multiple issues evaluated in parallel |
| **Aider** | None | Sequential |
| **Cursor** | Background | Background agent runs separately |
| **Windsurf** | Flows | Async background tasks |
| **Nika** | DAG + for_each | True parallel execution with concurrency control |

**Insight:** Nika has the most sophisticated parallelism model. for_each with
concurrency control and DAG-based dependency resolution is architecturally
superior to all competitors' approaches.

### Sandbox/Security

| System | Sandbox Type | Network | File System |
|--------|-------------|---------|-------------|
| **Devin** | Cloud VM | Full | Isolated |
| **Manus** | Docker/Cloud | Full | Isolated |
| **OpenHands** | Docker/E2B | Configurable | Bind mount |
| **Codex CLI** | Seatbelt/Docker | Disabled default | Local |
| **Claude Code** | None (permissions) | Full | Local |
| **SWE-Agent** | Docker | Limited | Isolated |
| **Aider** | None | Full | Local |
| **Cursor** | None (IDE sandbox) | Full | Local |
| **Windsurf** | None (IDE sandbox) | Full | Local |
| **Nika** | shell: false + blocklist | Full | Local |

**Insight:** Codex CLI's Seatbelt sandbox on macOS is the most elegant lightweight
solution. OpenHands' pluggable runtime (Docker/E2B/Modal) is the most flexible.
Nika's exec: security (shell: false, command blocklist) is minimal but effective
for YAML workflows.

### DAG / Workflow Support

| System | Workflow Format | DAG Support | Dependency Management |
|--------|----------------|-------------|----------------------|
| **Devin** | None (implicit plan) | No | Implicit via planning |
| **Manus** | None (task graph) | Internal | Internal coordinator |
| **OpenHands** | None (EventStream) | No | Sequential |
| **Codex CLI** | None | No | Sequential |
| **Claude Code** | None (TodoWrite) | No | Sequential TodoWrite |
| **SWE-Agent** | Config YAML (agent config) | No | Sequential |
| **Aider** | None | No | Sequential |
| **Cursor** | None | No | Sequential |
| **Windsurf** | Flows (emerging) | Unknown | Unknown |
| **Nika** | YAML workflow files | Full DAG | Explicit flows: section |

**Insight:** Nika is the ONLY system with explicit DAG workflow support.
This remains Nika's strongest architectural differentiator. No competitor
has moved toward declarative workflow files for agent orchestration.

---

## Architectural Patterns Summary

### The Convergent Pattern (90% of Systems)

```
User Prompt → Single Agent (ReAct Loop) → Tool Calls → Observation → Repeat
                    │
                    ├── Context: embeddings/files/git
                    ├── Tools: file edit, shell, browser
                    ├── Safety: permissions/sandbox
                    └── Output: modified files + conversation
```

Almost all coding agents converge on this pattern. The differentiators are:
1. **Which tools** (file-only vs shell+browser vs full VM)
2. **Context strategy** (repo map vs embeddings vs on-demand)
3. **Sandbox model** (none vs Docker vs cloud VM)
4. **Model flexibility** (single vs multi-model)

### The Divergent Pattern (Nika)

```
YAML Workflow → DAG Builder → Parallel Executor → Task Results
                    │               │
                    ├── Validate     ├── infer: (LLM)
                    ├── Optimize     ├── exec: (shell)
                    └── Plan         ├── fetch: (HTTP)
                                     ├── invoke: (MCP)
                                     └── agent: (ReAct loop)
```

Nika is architecturally distinct because:
1. **Declarative** -- Workflow defined in YAML, not discovered at runtime
2. **DAG-based** -- Explicit dependencies, not sequential conversation
3. **Multi-verb** -- Different execution models for different task types
4. **Reproducible** -- Same YAML produces same execution graph
5. **Composable** -- include: + context: + for_each + flows:

---

## Strategic Recommendations for Nika

### Strengths to Amplify

1. **DAG Workflow** -- No competitor offers this. Position heavily.
2. **Multi-model per task** -- Unique capability (different models for different steps).
3. **MCP-native** -- First-class MCP integration, not bolted on.
4. **Reproducibility** -- YAML workflows are version-controlled and deterministic.
5. **for_each parallelism** -- Most sophisticated parallelism model in the space.

### Gaps to Address (from competitive analysis)

| Gap | Leaders | Nika Action |
|-----|---------|-------------|
| **Repo map / AST context** | Aider (tree-sitter) | Consider tree-sitter integration for repo-aware context |
| **Cross-session memory** | Windsurf (memories), Devin (knowledge base) | Extend .nika/ with persistent knowledge store |
| **Background/async execution** | Devin, Cursor, Jules | Already have `nika jobs` (v0.27) |
| **Sandbox isolation** | Codex CLI (Seatbelt), OpenHands (Docker) | Consider Docker sandbox option for exec: |
| **IDE integration** | Cursor, Windsurf, Cline | Nika as MCP server for IDE agents |
| **Browser automation** | Devin, Manus, OpenHands | Consider fetch: + headless browser |
| **Linting guard** | SWE-Agent | Add post-exec: lint verification |
| **Approval modes** | Codex CLI, Claude Code | Add permission modes to nika run |

### Positioning Against Each Competitor

| Competitor | Nika's Advantage | Their Advantage |
|------------|-----------------|-----------------|
| **Devin** | Declarative, reproducible, open, cheaper | Full VM, browser, async, managed |
| **Manus** | Transparent orchestration, YAML, open | Multi-agent specialization, UI |
| **OpenHands** | DAG, MCP, multi-model, parallelism | CodeAct flexibility, Docker sandbox |
| **Codex CLI** | Multi-model, MCP, DAG, parallelism | Seatbelt sandbox, zero-config |
| **Claude Code** | DAG, multi-model, structured workflows | Better UX, sub-agents, hooks |
| **SWE-Agent** | Production runtime, MCP, parallelism | Benchmark-optimized, ACI design |
| **Aider** | DAG, MCP, agent loop, shell execution | Repo map, multi-model, git-native |
| **Cursor/Windsurf** | Headless, CI/CD, DAG, reproducible | IDE integration, codebase index |

### The Unique Nika Thesis

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                                                                               ║
║   THESIS: The coding agent space has converged on conversational               ║
║   single-agent patterns (ReAct loops with tools).                             ║
║                                                                               ║
║   Nika is the ONLY system that treats AI workflows as                         ║
║   declarative, composable, reproducible DAG artifacts.                        ║
║                                                                               ║
║   This matters because:                                                       ║
║   1. Conversations are not reproducible; workflows are                        ║
║   2. Sequential agents waste time; DAGs parallelize                           ║
║   3. Natural language plans are opaque; YAML plans are auditable             ║
║   4. Tool-locked agents are brittle; MCP-native agents are composable        ║
║   5. Single-model agents are limited; per-task model selection optimizes      ║
║                                                                               ║
║   Nika is not competing with coding assistants.                               ║
║   Nika is building the workflow runtime that coding assistants should use.    ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

---

## Appendix A: Feature Matrix

| Feature | Devin | Manus | OpenHands | Codex CLI | Claude Code | SWE-Agent | Aider | Cursor | Windsurf | Nika |
|---------|:-----:|:-----:|:---------:|:---------:|:-----------:|:---------:|:-----:|:------:|:--------:|:----:|
| **Open Source** | -- | -- | MIT | MIT | Proprietary | MIT | Apache | -- | -- | AGPL |
| **YAML Workflows** | -- | -- | -- | -- | -- | Config only | -- | -- | -- | YES |
| **DAG Execution** | -- | -- | -- | -- | -- | -- | -- | -- | -- | YES |
| **MCP Client** | -- | -- | -- | -- | YES | -- | -- | -- | YES | YES |
| **Multi-Model** | -- | Mixed | LiteLLM | OpenAI | Claude | Config | Excellent | Good | Good | 6+native |
| **Parallelism** | Sessions | Multi-agent | -- | -- | Sub-agents | Batch | -- | Background | Flows | DAG+for_each |
| **Sandbox** | Cloud VM | Docker | Docker+ | Seatbelt | Permissions | Docker | -- | -- | -- | shell:false |
| **Browser** | YES | YES | YES | -- | -- | -- | -- | -- | -- | -- |
| **Background** | YES | YES | -- | -- | -- | -- | -- | YES | YES | jobs cmd |
| **Memory** | Playbooks | Knowledge | -- | AGENTS.md | CLAUDE.md | -- | .aider | .cursorrules | Memories | EventLog |
| **Repo Map** | -- | -- | -- | -- | -- | -- | tree-sitter | Embeddings | Graph+Embed | -- |
| **Git Native** | -- | -- | -- | -- | YES | -- | YES | -- | -- | -- |
| **TUI** | Web | Web | Web | -- | -- | -- | CLI Chat | IDE | IDE | 4-View TUI |
| **5 Verbs** | -- | -- | -- | -- | -- | -- | -- | -- | -- | YES |
| **for_each** | -- | -- | -- | -- | -- | -- | -- | -- | -- | YES |
| **include/compose** | -- | -- | -- | -- | -- | -- | -- | -- | -- | YES |
| **Structured Output** | -- | -- | -- | -- | -- | -- | -- | -- | -- | YES |
| **NDJSON Traces** | -- | -- | -- | -- | -- | -- | -- | -- | -- | YES |

---

## Appendix B: Architecture Pattern Classification

```
TYPE 1: Conversational Agent (chat loop with tools)
├── Claude Code, Codex CLI, Aider, Cline, Goose
└── Best for: interactive coding, pair programming

TYPE 2: Autonomous Agent (async task completion)
├── Devin, Manus AI, Google Jules, Cursor Background
└── Best for: delegated tasks, CI/CD integration

TYPE 3: Benchmark Agent (optimized for evaluation)
├── SWE-Agent, AutoCodeRover, CodeR
└── Best for: automated bug fixing, standardized tasks

TYPE 4: IDE-Integrated Agent (editor-native)
├── Cursor, Windsurf, Cline, Roo Code
└── Best for: real-time coding assistance

TYPE 5: Workflow Engine (declarative orchestration)
├── Nika (unique)
└── Best for: reproducible multi-step AI workflows, CI/CD, batch processing
```

---

## Appendix C: Technology Stack Comparison

| System | Language | Runtime | LLM Integration | Key Dependencies |
|--------|----------|---------|-----------------|------------------|
| **Devin** | Python? | Cloud VM | Anthropic API | Playwright, custom |
| **Manus** | Unknown | Cloud | Multi-API | Proprietary |
| **OpenHands** | Python | Docker/E2B | LiteLLM | Jupyter, Playwright |
| **Codex CLI** | TypeScript | Node.js | OpenAI Responses API | sandbox-exec |
| **Claude Code** | TypeScript | Node.js | Anthropic API | MCP SDK |
| **SWE-Agent** | Python | Docker | Multi-API | Custom ACI |
| **Aider** | Python | Native | Multi-API (litellm) | tree-sitter, git |
| **Cursor** | TypeScript | Electron | Multi-API | VS Code, custom models |
| **Windsurf** | TypeScript | Electron | Multi-API | VS Code, Codeium |
| **Nika** | **Rust** | **tokio** | **rig-core + mistral.rs** | **rmcp, ratatui** |

Nika is the only system built in Rust with a native async runtime (tokio).
This gives it significant performance advantages for DAG execution and parallelism.

---

## Methodology

- **Sources:** Public documentation, GitHub repositories, published papers,
  blog posts, and architectural analysis from training data (up to January 2025)
  supplemented with project context from existing research documents.
- **Systems analyzed:** 9 major systems + 6 emerging runtimes
- **Focus:** Technical architecture, not marketing claims
- **Limitations:** Some systems (Devin, Manus) are closed-source; architecture
  details are inferred from public documentation, demos, and API analysis.
  Information may not reflect changes after January 2025.

---

## Sources

1. OpenHands GitHub: https://github.com/All-Hands-AI/OpenHands
2. SWE-Agent GitHub: https://github.com/princeton-nlp/SWE-agent
3. Aider GitHub: https://github.com/paul-gauthier/aider
4. OpenAI Codex CLI: https://github.com/openai/codex
5. Goose GitHub: https://github.com/block/goose
6. Cline GitHub: https://github.com/cline/cline
7. SWE-bench: https://www.swebench.com/
8. Devin: https://devin.ai/
9. Manus AI: https://manus.im/
10. Cursor: https://cursor.com/
11. Windsurf: https://codeium.com/windsurf
12. Claude Code: https://docs.anthropic.com/en/docs/claude-code
13. Existing Nika research: `docs/research/2026-02-27-ai-workflow-competitive-analysis.md`
14. Existing Nika research: `docs/research/competitors/nika-competitive-analysis.md`
15. Existing Nika research: `docs/research/2026-02-24-claude-code-builtin-patterns.md`

---

*Research generated by Claude Opus 4 for SuperNovae Studio -- 2026-03-14*
