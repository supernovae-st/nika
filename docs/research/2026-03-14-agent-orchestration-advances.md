# Research Report: Agent Orchestration Advances (2025-2026)

**Date**: 2026-03-14
**Researcher**: Claude (Opus 4) via Perplexity sonar-pro
**Scope**: Technical patterns, architectures, and production lessons for AI agent orchestration
**Queries**: 11 Perplexity searches across 7 topic areas

---

## Table of Contents

1. [Multi-Agent Orchestration Patterns](#1-multi-agent-orchestration-patterns)
2. [Context Management for Long-Running Agents](#2-context-management-for-long-running-agents)
3. [REPL-Augmented Reasoning](#3-repl-augmented-reasoning)
4. [Multi-Model Routing](#4-multi-model-routing)
5. [Rust-Based Agent Frameworks](#5-rust-based-agent-frameworks)
6. [Karpathy LLM OS Concept](#6-karpathy-llm-os-concept)
7. [Anthropic Agent Research + Protocol Landscape](#7-anthropic-agent-research--protocol-landscape)
8. [Production Lessons Learned](#8-production-lessons-learned)
9. [Relevance to Nika](#9-relevance-to-nika)

---

## 1. Multi-Agent Orchestration Patterns

### Supervisor-Worker (Most Proven)

Central coordinator decomposes tasks, routes to specialized workers, aggregates results.

**Frameworks implementing this pattern:**

| Framework | Approach | Strength |
|-----------|----------|----------|
| **LangGraph** | State-machine graph with conditional edges; supports cycles for iterative refinement | Persistent state, dynamic routing |
| **CrewAI** | Manager agent delegates to workers via `HierarchicalProcess()` | Clear hierarchy, easy setup |
| **AutoGen/AG2** | Conversational group chat; workers respond in turns | Collaborative reasoning, HITL |
| **OpenAI Swarm/Agents SDK** | Lightweight client-side handoffs, no persistent state | Low-latency, stateless |
| **Claude Agent SDK** | Primitives for agentic loops: gather context, act, verify, repeat | Raw capabilities, developer-first |
| **Google ADK** | Agent tree with sub-agent delegation and shared state | Multi-agent context sharing |

**Key insight**: Centralized control aids compliance and debugging but limits parallelism. Northwestern Mutual cut processing from hours to minutes with supervisor-worker.

Source: [Openlayer Multi-Agent Architecture Guide](https://www.openlayer.com/blog/post/multi-agent-system-architecture-guide), [Microsoft Azure AI Agent Patterns](https://learn.microsoft.com/en-us/azure/architecture/ai-ml/guide/ai-agent-design-patterns)

### Hierarchical Agent Systems

Stack supervisors recursively: strategist -> planner -> executor. Depth control prevents infinite recursion.

**Production pattern:**
```
def hierarchical_execute(task, depth=0, max_depth=5):
    if depth >= max_depth:
        return human_escalate(task)
    subagents = decompose(task)
    results = parallel_map(subagents, hierarchical_execute, depth+1)
    return aggregate(results)
```

**Depth control mechanisms:**
- Fixed depth limits (e.g., 3-5 levels)
- Token budget thresholds (e.g., < 200k tokens per inter-agent call)
- Cost circuit breakers
- Time-based deadlines

**Nika parallel**: This maps directly to Nika's `spawn_agent` with `depth_limit` (ADR-004). Nika already implements depth-limited hierarchical agents.

### DAG-Based Agent Orchestration

Nodes are agents/tasks, edges are dependencies. Enables parallelism, retries, fault tolerance via topological execution.

**Production approaches:**
- **Prefect**: Python decorators for agent nodes with `submit()` + `wait_for=[]`
- **Temporal**: Durable execution with agent-as-activity pattern
- **LangGraph**: Compiles to DAGs with conditional edges for dynamic routing
- Custom engines add agent-specific primitives (context passing, MCP routing)

**Key metrics**: >95% handoff success rate, exponential backoff retries, >80% utilization targets.

**Nika parallel**: Nika's core architecture IS a DAG executor. This validates the fundamental design choice.

### Actor Model for Agents

Agents as isolated actors with mailboxes for async message passing. Each processes one message at a time, spawns children, handles supervision (restart on failure).

**Benefits**: Scales horizontally, resilient (route around failures), no shared state. Used in mesh architectures for peer-to-peer without central bottleneck.

**Relevance**: A2A protocol (see section 7) essentially standardizes actor-model communication between agents across organizations.

### Swarm Intelligence for Coding

Peer-to-peer coordination with emergent behavior:
- **Parallel exploration**: Multiple code generators propose variants; consensus via weighted voting
- **Event-driven choreography**: Local meshes for tactical (debug swarms), orchestrator for strategy
- **Circuit breakers**: Prevent cascade failures in swarm

### What Patterns Win in Production

| Pattern | Best For | Metrics |
|---------|----------|---------|
| Sequential Pipeline | Step-wise (loan approval) | 45% faster resolution |
| Parallel Execution | Analysis swarms | 60% accuracy gain |
| Conditional Routing | Dynamic coding/debug | >95% handoff success |
| Event-Driven Hybrid | Real-time systems | 3x decision speed |
| Hierarchical w/ Depth | Compliance-heavy | Hours -> minutes |

**Production consensus**: Start with 2-3 agents. Measure ROI per workflow. Hybrids win -- supervisor for strategy + mesh for execution.

Source: [OnAbout Multi-Agent Orchestration](https://www.onabout.ai/p/mastering-multi-agent-orchestration-architectures-patterns-roi-benchmarks-for-2025-2026)

---

## 2. Context Management for Long-Running Agents

### Anthropic's Context Engineering Framework (September 2025)

Anthropic published a landmark blog post (September 29, 2025, ~500k views in one week) defining **context engineering** as curating the optimal token set during inference.

**Key strategies recommended by Anthropic:**

1. **Holistic context management**: Treat context as a finite resource. Start minimal, iterate based on failure modes. Organize with XML tags or Markdown headers into sections: background, instructions, tool guidance, output description.

2. **Rolling compression**: Use LLM-driven summarization to condense history. In Claude Code, retain architectural decisions, unresolved bugs, implementation details while discarding redundant tool outputs.

3. **Note-taking for milestones**: Agents write structured notes post-interaction (`{goals, progress, todos}`), then reload subsets on reset.

4. **Sub-agent architectures**: Lead agent coordinates; sub-agents get clean context windows for deep work, return distilled summaries. Reduces main agent context pollution.

5. **Memory systems**: External file-based memory for persistent knowledge across sessions. Public beta with Claude Sonnet 4.5.

Source: [Anthropic: Effective Context Engineering for AI Agents](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents)

### Token-Efficient Context Strategies

| Strategy | Token Savings | Best For | Implementation |
|----------|---------------|----------|----------------|
| **RAG** | High (external storage) | Fact retrieval, codebases | Embed chunks, retrieve top-k (cosine > 0.75), inject 1-2k tokens |
| **Summarization** | Medium (50-80% reduction) | Conversations | LLM prompt: "Compress last N turns to key facts"; chain every 10k tokens |
| **Sliding Window** | Low-Medium | Sequential chats | Keep last M tokens, summarize dropped prefix |
| **Hierarchical** | High | Long-horizon tasks | Tiered: short=last 4k verbatim, mid=session summary 2k, long=key facts 1k |
| **Sub-agent isolation** | High | Complex multi-step | Clean windows per sub-task, return 1-2k summaries |

### Episodic Memory Implementations

Hierarchical system with three tiers:
- **Short-term**: Verbatim recent turns
- **Medium-term**: Session summaries
- **Long-term**: Key facts/relationships extracted

Retrieval: Similarity search on summaries with filters (recency > 0.8), inject 500-1000 tokens of top-3 matches.

### Reference Semantics (State Without Context Stuffing)

Agents reference external state via IDs or paths, reading only on-demand:
```
# Instead of stuffing full project state into context:
Agent writes: {"goals": [...], "progress": {...}, "todos": [...]}
Agent reads: "Load todos from project_state.json"
```

This is the pattern Claude Code uses for multi-hour sessions. Write notes at milestones, read relevant subsets when resuming.

### Novel Approaches

- **MemGPT/Letta**: Virtual context windows via OS-like paging. Core context (4k tokens) + page faults to external DB. LRU eviction policy.
- **Dynamic allocation**: Adjust budgets at runtime. If query complexity > 0.7 (cheap classifier), boost history to 60%.
- **Predictive prefetch**: ML model predicts next needs from chat history, pre-loads embeddings.

Source: [GetMaxim Context Window Management](https://www.getmaxim.ai/articles/context-window-management-strategies-for-long-context-ai-agents-and-chatbots/), [Google ADK Context Engineering](https://developers.googleblog.com/architecting-efficient-context-aware-multi-agent-framework-for-production/)

---

## 3. REPL-Augmented Reasoning

### How Coding Agents Use REPLs as External Working Memory

Architecture pattern:
1. Agent generates code based on LLM reasoning
2. Sends to sandboxed REPL (Python subprocess, Jupyter kernel)
3. Captures stdout/stderr/return values
4. Feeds results back into LLM prompt for next-step planning
5. State persists via REPL namespace (global variables, imported modules)

**Key agents using this pattern:**
- **Devin**: REPL for iterative debugging (define data, query, mutate, analyze)
- **SWE-agent**: REPL for code exploration and testing
- **Claude Code**: REPL for tool-calling verification in agent loops
- **Cursor**: VS Code-like REPL for real-time edits

### Code-as-Reasoning Paradigm

Core loop: `LLM -> pseudocode -> REPL execution -> parse results -> refine reasoning`

Execution acts as a "ground truth" oracle. LLM hypothesizes, code tests empirically, corrects via feedback. This extends chain-of-thought by making steps executable, reducing error compounding.

**Key difference from chain-of-thought:**

| Aspect | Tool-Use-as-Reasoning | Chain-of-Thought |
|--------|----------------------|------------------|
| Mechanism | External tools execute/verify | Internal token generation |
| Verification | Empirical (runtime results) | Verbal (self-consistency) |
| Scalability | Parallelizable, persistent memory | Sequential, token-limited |
| Best for | Coding, data, math | Qualitative planning |

### Persistent Execution Environments

- **E2B**: Cloud-native REPL-as-a-service. Jupyter kernels in Firecracker microVMs. Agents API-call for sessions with package requirements.
- **Modal**: Serverless Python with persistent volumes. Agents deploy `@modal.function` with REPL loop.
- **Fly.io**: Edge-deployed containers with persistent volumes across regions.

**Security**: Namespace-limited syscalls, resource caps (CPU/memory), seccomp/AppArmor, no fork/exec outside sandbox.

**Nika parallel**: Nika's `exec:` verb with `shell: false` default aligns with sandboxed execution. The REPL pattern could inform a future `repl:` verb or persistent `exec:` context.

Source: [NVIDIA Reasoning AI Agents](https://blogs.nvidia.com/blog/reasoning-ai-agents-decision-making/)

---

## 4. Multi-Model Routing

### Cost-Performance Routing Strategies

**Model tiering pattern** (most common in production):
1. **Triage**: Cheap/fast model (Haiku, GPT-mini) classifies task complexity
2. **Route**: Simple tasks stay on cheap model; complex tasks escalate
3. **Execute**: Specialist model handles the task
4. **Verify**: Optional verification step with different model

**Cascading/fallback:**
```
1. Cheap model attempts task -> Success? Done. Fail? Escalate.
2. Mid-tier (Sonnet) -> Partial success? Refine. Fail? Top-tier.
3. Top-tier (Opus) -> Handle with full capability
4. Monitor cost/quality via observability
```

IDC predicts 70% of top AI enterprises will use dynamic model routing by 2028.

Source: [IDC: The Future of AI is Model Routing](https://www.idc.com/resource-center/blog/the-future-of-ai-is-model-routing/)

### Coding Model Benchmarks (March 2026)

**SWE-bench Verified scores** (resolution rate on GitHub issues):

| Model | SWE-bench Verified | SWE-bench Pro | Notes |
|-------|-------------------|---------------|-------|
| GPT-5.4 Pro | 95% | -- | Top overall |
| GPT-5.3 Codex | 95% | 57% (custom scaffolding) | Best implementation |
| Claude Opus 4.6 | 91% | -- | 1M context |
| Gemini 3.1 Pro | 91% | -- | 2M context, Deep Think |
| Claude Opus 4.5 | 80.9% | 45.9% | Strong semantics |
| Claude Sonnet 4.5 | -- | 43.6% | Good debugging |
| DeepSeek V3 | 73% | -- | Open-source leader |

**IMPORTANT**: SWE-bench Verified is considered contaminated (inflated scores). SWE-bench Pro (1,865 tasks, multi-language) is more reliable. Best model scores 46% on Pro vs 81% on Verified.

**Inferred subtask strengths** (from failure analysis):

| Subtask | Best Models | Why |
|---------|-------------|-----|
| Planning | Claude Opus 4.5/4.6 | Strong semantic understanding |
| Implementation | GPT-5.3 Codex | High multi-file edit success |
| Debugging | Claude Sonnet 4.5 | 43.6% Pro score |
| Code Review | Gemini 3 Pro | 43.3% Pro score |
| Refactoring | GPT-5.4 Pro | High multi-file changes |

**Context overflow is the #1 failure mode**: 60%+ agent time spent searching. Models with 1M+ context (Opus 4.6, GPT-5.4, Gemini 3.1) reduce overflow costs.

**Optimal routing strategy for coding pipeline:**
- Reasoning models (Opus, GPT-5.x) for planning/debugging
- Codex-specialized for implementation/refactoring
- Long-context models for search-heavy phases
- 250-turn limits, standardized tools

Source: [BenchLM Coding Benchmarks](https://benchlm.ai/coding), [Morph SWE-Bench Pro](https://www.morphllm.com/swe-bench-pro), [SmartScope LLM Coding Comparison](https://smartscope.blog/en/generative-ai/chatgpt/llm-coding-benchmark-comparison-2026/)

---

## 5. Rust-Based Agent Frameworks

### rig-core (v0.32.0)

The primary Rust crate for building LLM-powered applications:
- **Version**: 0.32.0 (50 versions total, latest Feb 2026)
- **Focus**: Ergonomics and modularity for LLM apps
- **Ecosystem**: Companion crates (rig-lancedb for vector stores, riglr-core for multi-chain orchestration)
- **Status**: Actively maintained, no security advisories
- **Nika uses**: rig-core v0.32 for all 6 cloud providers via `RigProvider`

Source: [rig-core on crates.io](https://crates.io/crates/rig-core)

### rmcp (Rust MCP SDK)

The Rust MCP SDK that Nika uses (v0.16). Low visibility in public discourse but functional. Implements JSON-RPC transport for MCP protocol. Nika's `src/mcp/` module wraps this.

### AutoAgents (Rust-Native Agent Runtime)

Announced February 2026 on Rust users forum. A Rust-native agent runtime focused on:
- Safety and modularity
- Production-grade performance
- Edge + cloud deployment (e.g., Raspberry Pi)
- Memory-safe execution without GC pauses

Source: [AutoAgents on Rust Users Forum](https://users.rust-lang.org/t/showcase-autoagents-rust-runtime-for-safe-production-ai-agents-edge-cloud/138073)

### Why Rust for Agents

| Aspect | Rust | Python |
|--------|------|--------|
| Performance | 500x faster ops, true parallelism via Rayon/Tokio | GIL bottleneck at scale |
| Safety | Compile-time checks, no GC pauses | Memory leaks, unpredictable pauses |
| Concurrency | Tokio async + Rayon parallel | AsyncIO + multiprocessing (fragile) |
| Use case | Production agents (24/7, edge/cloud) | Prototyping, not 24/7 reliability |
| Ecosystem maturity | Growing but niche | Dominant (PyTorch, LangChain, etc.) |

**Honest assessment**: Rust AI ecosystem is functional for production inference/orchestration but immature vs Python for ML training and high-level agent frameworks. The gap is closing -- rig-core at v0.32 with 50 releases shows momentum.

Source: [Red Hat: Why Agentic AI Developers Move to Rust](https://developers.redhat.com/articles/2025/09/15/why-some-agentic-ai-developers-are-moving-code-python-rust)

### Other Notable Rust Crates

- **mistral.rs**: Local inference runtime. Native Ollama support for Llama/Mistral/Gemma. Metal (macOS) and CUDA (Linux) acceleration. Used by Nika for `provider: native`.
- **riglr-core**: Multi-chain tool orchestration in the rig ecosystem.
- **DataFusion / Polars**: High-speed data processing for AI pipelines.

---

## 6. Karpathy LLM OS Concept

### 2025 Year in Review: Six Paradigm Shifts

Karpathy's December 2025 review identified key shifts relevant to agent runtimes:

1. **RLVR (Reinforcement Learning from Verifiable Rewards)**: Shifted compute from pretraining toward inference-time reasoning traces. New scaling dimension: test-time compute through extended thinking.

2. **Vertical LLM Applications** (the real insight): A distinct layer mediates between foundation models and end-user value. Cursor exemplifies this:
   - Context engineering
   - Multi-call orchestration through DAGs
   - Application-specific UI/UX
   - "Autonomy slider" for user control

3. **Software 3.0**: From hand-written code (1.0) to neural networks (2.0) to LLM-powered code generation (3.0).

### LLM OS Thread/Process Model

Karpathy's December 2025 X thread described a **new programmable abstraction layer**:
- Agents, subagents, prompts, contexts, memory
- MCP, LSP, workflows, IDE integrations
- Developers must build mental models for "stochastic, fallible, unintelligible and changing entities"

**How it maps to agent runtimes**: The LLM OS concept treats each agent invocation as a "process" with:
- Its own context window ("memory space")
- Tool access ("system calls")
- Communication channels ("IPC" via MCP/A2A)
- Lifecycle management (spawn, monitor, terminate)

**Nika parallel**: Nika's workflow execution is essentially an LLM OS scheduler. Each task is a "process" with its own context, tool access (MCP), and lifecycle. The `spawn_agent` tool creates "child processes."

Source: [Karpathy 2025 Year in Review](https://karpathy.bearblog.dev/year-in-review-2025/), [Futurum: Karpathy's Thread](https://futurumgroup.com/insights/karpathys-thread-signals-ai-driven-development-breakpoint/)

---

## 7. Anthropic Agent Research + Protocol Landscape

### Anthropic's Agent Building Philosophy

Anthropic favors **raw capabilities over visual builders**:
- Claude Agent SDK provides primitives for agentic loops: gather context -> act -> verify -> repeat
- Focus on foundational access: terminal, file system, script execution
- Philosophy: Build production-ready agents like Claude Code with composable primitives
- Contrasts with OpenAI's "polished products with visual interfaces" approach

### Context Engineering (Detailed)

From Anthropic's September 2025 blog (see section 2 for full details):
- Compression for conversational flow
- Note-taking for milestones
- Sub-agents for parallel exploration
- Let models act autonomously as capabilities improve
- Enabled 30+ hour coding sessions and multi-file refactoring

### Security Progress

Anthropic reduced prompt injection success from 23.6% to 11.2% in Claude Sonnet 4.5 through architectural improvements.

### Agent Communication Protocol Landscape (March 2026)

Three protocols now define the agent interop space:

#### MCP (Model Context Protocol) -- Anthropic

- **Purpose**: Connect models to tools, data sources, and context
- **Scope**: Intra-agent (model-to-tool communication)
- **Transport**: JSON-RPC over stdio or SSE
- **Adoption**: Widely adopted (Claude Code, Cursor, VS Code, etc.)
- **Status**: Active development, exact version not confirmed in searches

#### A2A (Agent-to-Agent Protocol) -- Google/Linux Foundation

- **Purpose**: Peer-to-peer communication between autonomous agents
- **Released**: April 2025, donated to Linux Foundation June 2025
- **Transport**: HTTPS + JSON-RPC 2.0 + Server-Sent Events (SSE)
- **Discovery**: Agent Cards at `/.well-known/agent.json` (RFC 8615)
- **Task lifecycle**: Stateful (input-required, completed, failed)
- **Security**: OAuth 2.0, API keys, or mutual TLS
- **Version**: v0.3.0+ (added gRPC July 2025, Python SDK v0.3.24 Feb 2026)
- **Backing**: 150+ organizations (AWS, Microsoft, IBM)

**Agent Card example:**
```json
{
  "capabilities": {
    "streaming": true,
    "pushNotifications": true
  },
  "skills": [{
    "id": "detect-network-incident",
    "inputModes": ["application/json", "text/plain"],
    "outputModes": ["application/json"]
  }]
}
```

#### ACP (Agent Communication Protocol)

- **Purpose**: Simpler HTTP-based agent messaging
- **Auth**: Bearer tokens, API keys (simpler than A2A's OAuth flows)
- **Status**: Less mature than A2A

#### How They Relate

```
MCP: Model <-> Tools (intra-agent)
     "What tools can I use?"

A2A: Agent <-> Agent (inter-agent)
     "What can you do? Here's a task."

ACP: Agent <-> Agent (simpler)
     "Here's a message via HTTP."
```

**They are complementary, not competing.** MCP handles the vertical (model-to-tool), A2A handles the horizontal (agent-to-agent). A production system uses both.

**Nika parallel**: Nika already implements MCP client. A2A support would enable Nika workflows to discover and delegate to external agents. This is a natural evolution.

Source: [Google A2A Announcement](https://developers.googleblog.com/en/a2a-a-new-era-of-agent-interoperability/), [Linux Foundation A2A Project](https://www.linuxfoundation.org/press/linux-foundation-launches-the-agent2agent-protocol-project-to-enable-secure-intelligent-communication-between-ai-agents), [Ruh AI Protocol Guide](https://www.ruh.ai/blogs/ai-agent-protocols-2026-complete-guide)

---

## 8. Production Lessons Learned

### What Failed / Was Overhyped

- **Fully autonomous multi-agent systems**: High abandonment rates. Gartner predicts >40% of agent-based AI initiatives fail by 2027.
- **Open-ended agents without clear workflows**: Underdelivered on vague requirements, required constant human correction.
- **Complex architectures before proving value**: Scalability issues, cost overruns under load.
- **Demo-driven development**: 2024-2025 demos proliferated but faltered in real environments lacking data integration.

### What Actually Works

- **Narrow, well-defined use cases**: Repetitive, high-volume tasks with structured inputs
- **Start with 2-3 agents, not 20**: Prove value before scaling
- **Human-in-the-loop for critical paths**: Not optional
- **Structured output (JSON mode) over free-form text**: Enables reliable parsing, tool use, workflow chaining
- **Observability at every step**: Track accuracy, task success rate, fallback frequency, cost

### Error Recovery Patterns

Production systems that work embed:
1. Verification per step with detailed action logs
2. Fallback/escalation triggers to humans for uncertainty
3. Simulation environments for pre-production testing
4. Continuous feedback loop (treat agents like employees: onboard, train, evaluate)

### Cost Management

- Budget for API/model usage spikes under load
- Track cost vs time saved per task against human baselines
- Prioritize narrow use cases to minimize overruns
- Phased rollouts with human oversight
- Fine-tuning with domain data improves efficiency over general models

### Evaluation Frameworks

- Structured test scenarios for edge cases
- Simulation environments with 100+ runs per scenario
- Metrics: accuracy, task success rate, fallback frequency, cost, human benchmark comparison
- Automated, repeatable testing (non-deterministic behavior demands this)

Source: [McKinsey: Six Lessons from Agentic AI](https://www.mckinsey.com/capabilities/quantumblack/our-insights/one-year-of-agentic-ai-six-lessons-from-the-people-doing-the-work), [Origin 137: Deploy AI Agents Production Guide](https://www.o137.ai/en/blog/deploy-ai-agents-production-practical-guide-2026)

---

## 9. Relevance to Nika

### Architecture Validation

Nika's design choices align with proven production patterns:

| Nika Feature | Industry Pattern | Validation |
|--------------|-----------------|------------|
| DAG executor | DAG-based orchestration | Core pattern in Prefect, Temporal, LangGraph |
| `spawn_agent` + `depth_limit` | Hierarchical agents with depth control | Proven in production (ADR-004 matches best practice) |
| 5 semantic verbs | Task-type specialization | Matches "narrow, well-defined" winning pattern |
| MCP client | Tool discovery protocol | MCP is the standard (Anthropic-endorsed) |
| YAML workflows | Declarative orchestration | Readable, testable, reproducible (winning over imperative) |
| Structured output | JSON Schema enforcement | "Structured > free-form" is production consensus |
| `fail_fast` + `DependencyFailed` | Circuit breakers + error propagation | Standard fault tolerance pattern |

### Gaps / Opportunities Identified

1. **Context compression**: Nika does not yet implement rolling context compression for long-running agent tasks. Anthropic's note-taking pattern could be implemented as a builtin tool or runtime feature.

2. **Multi-model routing**: Nika uses a single `provider:` per workflow. A routing layer that selects models per-task based on task type (planning=Opus, implementation=Codex, debugging=Sonnet) would match production best practice.

3. **A2A protocol support**: Nika could expose Agent Cards and accept A2A task delegations, enabling inter-agent communication beyond MCP.

4. **REPL integration**: A persistent execution environment for `exec:` tasks (keeping state across steps) would enable REPL-augmented reasoning patterns.

5. **Episodic memory**: Cross-workflow memory that persists key facts and retrieves relevant context for new runs.

6. **Cost tracking**: Token usage per task with budget limits and cost-performance routing.

### Actionable Next Steps (Priority Order)

1. **Multi-model routing per task** (high impact, moderate effort)
   - Allow `provider:` + `model:` at task level, not just workflow level
   - Implement cascading: try cheap model first, escalate on failure
   - Already partially supported via `infer: { model: "..." }`

2. **Context compression for agents** (high impact, moderate effort)
   - Implement `nika:summarize` builtin tool for agents
   - Add `context_budget` parameter to `agent:` verb
   - Auto-summarize after N turns or when approaching token limit

3. **Token cost tracking** (medium impact, low effort)
   - Extend `AgentTurnMetadata` with cost estimates
   - Add workflow-level budget limits
   - Emit `CostThresholdExceeded` event

4. **A2A Agent Card** (future, after v0.28)
   - Expose Nika workflows as Agent Cards
   - Accept A2A task delegations as a new verb or transport

---

## Sources

### Primary Sources (Perplexity citations)

1. [Anthropic: Effective Context Engineering for AI Agents](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents) -- Context management strategies
2. [Openlayer: Multi-Agent Architecture Guide](https://www.openlayer.com/blog/post/multi-agent-system-architecture-guide) -- Pattern comparison
3. [Microsoft Azure: AI Agent Design Patterns](https://learn.microsoft.com/en-us/azure/architecture/ai-ml/guide/ai-agent-design-patterns) -- Enterprise patterns
4. [OnAbout: Multi-Agent Orchestration](https://www.onabout.ai/p/mastering-multi-agent-orchestration-architectures-patterns-roi-benchmarks-for-2025-2026) -- ROI benchmarks
5. [IDC: The Future of AI is Model Routing](https://www.idc.com/resource-center/blog/the-future-of-ai-is-model-routing/) -- Routing strategies
6. [BenchLM: Coding Benchmarks 2026](https://benchlm.ai/coding) -- SWE-bench scores
7. [Morph: SWE-Bench Pro](https://www.morphllm.com/swe-bench-pro) -- Contamination analysis
8. [Google: A2A Protocol Announcement](https://developers.googleblog.com/en/a2a-a-new-era-of-agent-interoperability/) -- Protocol spec
9. [Linux Foundation: A2A Project](https://www.linuxfoundation.org/press/linux-foundation-launches-the-agent2agent-protocol-project-to-enable-secure-intelligent-communication-between-ai-agents) -- Governance
10. [Karpathy: 2025 Year in Review](https://karpathy.bearblog.dev/year-in-review-2025/) -- LLM OS paradigm
11. [Red Hat: Rust for Agentic AI](https://developers.redhat.com/articles/2025/09/15/why-some-agentic-ai-developers-are-moving-code-python-rust) -- Rust vs Python
12. [rig-core on crates.io](https://crates.io/crates/rig-core) -- v0.32.0
13. [McKinsey: Six Lessons from Agentic AI](https://www.mckinsey.com/capabilities/quantumblack/our-insights/one-year-of-agentic-ai-six-lessons-from-the-people-doing-the-work) -- Production lessons
14. [GetMaxim: Context Window Management](https://www.getmaxim.ai/articles/context-window-management-strategies-for-long-context-ai-agents-and-chatbots/) -- Token strategies
15. [Google: ADK Context Engineering](https://developers.googleblog.com/architecting-efficient-context-aware-multi-agent-framework-for-production/) -- Multi-agent context
16. [AutoAgents Rust Runtime](https://users.rust-lang.org/t/showcase-autoagents-rust-runtime-for-safe-production-ai-agents-edge-cloud/138073) -- Rust agent runtime
17. [Ruh AI: Agent Protocols 2026](https://www.ruh.ai/blogs/ai-agent-protocols-2026-complete-guide) -- Protocol comparison
18. [SmartScope: LLM Coding Comparison 2026](https://smartscope.blog/en/generative-ai/chatgpt/llm-coding-benchmark-comparison-2026/) -- Model benchmarks

### Methodology

- **Tools used**: Perplexity sonar-pro (11 queries)
- **Pages analyzed**: 60+ sources across searches
- **Time period**: 2025-01 to 2026-03
- **Total cost**: ~$0.21 (Perplexity API)

### Confidence Level

**High** for orchestration patterns, context management, and protocol landscape (multiple corroborating sources, primary docs).
**Medium** for coding benchmarks (SWE-bench contamination acknowledged, Pro scores more reliable).
**Medium** for Rust ecosystem (limited public discourse, supplemented by known Nika internals).
**Low** for specific model routing implementations in Cursor/Copilot/Claude Code (proprietary, not documented).
