# 06 — Research Synthesis Report

> Complete findings from the Nika Evolution brainstorming session.
> 13 research agents deployed. 371 source files audited. 6 papers analyzed. 5 competitors studied.
> Revised: 2026-03-14 — Updated to align with 6-priority architecture (doc 05) and Slate deep integration (doc 07).
> Date: 2026-03-14

---

## Table of Contents

1. [Ecosystem Overview](#1-ecosystem-overview)
2. [Nika Deep Dive — Every Module](#2-nika-deep-dive)
3. [NovaNet Deep Dive](#3-novanet-deep-dive)
4. [Scientific Literature](#4-scientific-literature)
5. [Competitive Landscape](#5-competitive-landscape)
6. [Gap Analysis](#6-gap-analysis)
7. [Synergy Map](#7-synergy-map)
8. [Evolution Priorities](#8-evolution-priorities)

---

## 1. Ecosystem Overview

```
+=====================================================================================+
|                                                                                     |
|    S U P E R N O V A E   E C O S Y S T E M                                         |
|                                                                                     |
+=====================================================================================+
|                                                                                     |
|   +---------------------------+      MCP Protocol      +------------------------+   |
|   |                           | <===================>  |                        |   |
|   |     N O V A N E T         |   novanet_search        |      N I K A           |   |
|   |     (The Brain)           |   novanet_context        |      (The Body)        |   |
|   |                           |   novanet_write          |                        |   |
|   |   Knowledge Graph         |   novanet_introspect     |   Workflow Engine       |   |
|   |   Neo4j + 59 NodeClasses  |   novanet_audit          |   Rust + tokio         |   |
|   |   159 ArcClasses          |   novanet_describe       |   5 Verbs, 7 Providers |   |
|   |   200+ Locales            |   novanet_batch          |   6,157 Tests          |   |
|   |   8 MCP Tools             |   novanet_query          |   371 Files, 219K LOC  |   |
|   |                           |                          |                        |   |
|   +---------------------------+                          +------------------------+   |
|               |                                                    |                |
|               v                                                    v                |
|   +---------------------------+                          +------------------------+   |
|   |   SHARED Realm (36 nodes) |                          |   Core Registry        |   |
|   |   config / locale /       |                          |   18 KNOWN_PROVIDERS   |   |
|   |   geography / knowledge   |                          |   16+ KNOWN_MODELS     |   |
|   |                           |                          |   48 MCP_ALIASES       |   |
|   |   ORG Realm (23 nodes)    |                          |                        |   |
|   |   foundation / structure /|                          |   spn Daemon (IPC)     |   |
|   |   semantic / instruction /|                          |   OS Keychain secrets  |   |
|   |   output                  |                          |   Editor sync (4)      |   |
|   +---------------------------+                          +------------------------+   |
|                                                                                     |
|   +---------------------------------------------------------------------------------+   |
|   |                      THE GOLDEN RULE (Extended)                                 |   |
|   |   If it's about KNOWING things    -->  NovaNet                                  |   |
|   |   If it's about DOING things      -->  Nika                                     |   |
|   |   If it's about CONNECTING        -->  MCP                                      |   |
|   |   If it's about THINKING          -->  Episodes (strategy + model slots)        |   |
|   |   If it's about REMEMBERING       -->  Episodes (NovaNet persistence)           |   |
|   +---------------------------------------------------------------------------------+   |
|                                                                                     |
+=====================================================================================+
```

### Stats Snapshot

```
+--------------------------+----------------------------+
|  NIKA v0.27.0            |  NOVANET v0.20.0           |
+--------------------------+----------------------------+
|  371 files               |  59 NodeClasses            |
|  219,197 lines           |  159 ArcClasses            |
|  6,157 tests             |  8 MCP tools               |
|  5 semantic verbs        |  200+ locales              |
|  7 LLM providers         |  6 knowledge atom types    |
|  11 builtin tools        |  5 search modes            |
|  30+ transform ops       |  4 context modes           |
|  34 event types          |  CSR quality audit         |
|  4 TUI views             |  Neo4j backend             |
|  48 MCP server aliases   |  Denomination forms (6)    |
+--------------------------+----------------------------+
```

---

## 2. Nika Deep Dive

### 2.1 Module Map

```
+=====================================================================================+
|  NIKA SOURCE TREE — 371 files, 219K lines                                           |
+=====================================================================================+
|                                                                                     |
|  tools/nika/src/                                                                    |
|  |                                                                                  |
|  +-- ast/                    TWO-PHASE YAML PARSING                                 |
|  |   +-- raw/                Phase 1: YAML -> Raw AST (with spans)                  |
|  |   |   +-- parser.rs         marked_yaml, FileId, full position tracking          |
|  |   |   +-- workflow.rs       RawWorkflow (all fields Optional)                    |
|  |   |   +-- task.rs           RawTask, RawForEach, RawRetry                        |
|  |   |   +-- action.rs         RawTaskAction (5 verbs: infer/exec/fetch/invoke/agent) |
|  |   |   +-- mcp.rs            RawMcpConfig, RawMcpServer                           |
|  |   |                                                                              |
|  |   +-- analyzed/           Phase 2: Raw -> Analyzed (validated)                    |
|  |   |   +-- workflow.rs       AnalyzedWorkflow + TaskTable (O(1) lookup)            |
|  |   |   +-- task.rs           AnalyzedTask + TaskId (interned)                      |
|  |   |   +-- action.rs         AnalyzedTaskAction (typed, validated)                 |
|  |   |                                                                              |
|  |   +-- analyzer/           Validation + transformation                             |
|  |   |   +-- analyze.rs        Main analyze() function                               |
|  |   |   +-- errors.rs         NIKA-140 to NIKA-149                                  |
|  |   |   +-- feature_gate.rs   Schema version gating (@0.1 -> @0.11)                |
|  |   |                                                                              |
|  |   +-- context.rs           ContextSpec (file loading, glob, session)              |
|  |   +-- include.rs           IncludeSpec (DAG fusion, prefix, cycle detect)         |
|  |   +-- skill_def.rs         SkillDef (path + alias, merge dedup)                  |
|  |   +-- decompose.rs         DecomposeSpec (runtime DAG expansion)                 |
|  |   +-- output.rs            OutputSpec (structured output schema)                 |
|  |                                                                                  |
|  +-- core/                   ZERO-DEP REGISTRY (v0.27)                              |
|  |   +-- providers.rs          KNOWN_PROVIDERS: 6 LLM + 11 MCP + 1 Local = 18      |
|  |   +-- models.rs             KNOWN_MODELS: 16+ (text, vision, embedding)           |
|  |   +-- mcp_aliases.rs        MCP_ALIASES: 48 servers in 6 categories              |
|  |   +-- mcp_config.rs         McpConfig: 3-level (global -> project -> workflow)    |
|  |                                                                                  |
|  +-- dag/                    IMMUTABLE DAG                                          |
|  |   +-- mod.rs                Dag: FxHashMap + SmallVec, 3-color DFS cycle detect   |
|  |   +-- flow.rs               Build from AnalyzedWorkflow, implicit deps            |
|  |   +-- validate.rs           Binding validation                                    |
|  |   +-- stable.rs             petgraph StableGraph wrapper                          |
|  |                                                                                  |
|  +-- runtime/                EXECUTION ENGINE                                       |
|  |   +-- executor/             TaskExecutor: verb dispatch, provider cache            |
|  |   +-- runner.rs             Runner: layered topo-sort, JoinSet, semaphore         |
|  |   +-- spawn.rs              SpawnAgentTool: depth 3-10, MCP inheritance           |
|  |   +-- rig_agent_loop/       RigAgentLoop: rig-core v0.32, multi-turn chat         |
|  |   |   +-- types.rs          RigAgentStatus, AgentTurnMetadata                    |
|  |   |   +-- chat.rs           Chat history, continue conversation                   |
|  |   |   +-- streaming.rs      Real-time token streaming                             |
|  |   |   +-- thinking.rs       Extended thinking (Claude)                            |
|  |   |   +-- providers.rs      run_claude/openai/mistral/groq/deepseek/gemini        |
|  |   +-- output.rs             4-layer structured output (validate->retry->repair)   |
|  |                                                                                  |
|  +-- binding/                DATA FLOW                                              |
|  |   +-- entry.rs              UseEntry/WiringSpec + WithEntry/WithSpec              |
|  |   +-- resolve.rs            LazyBinding (Resolved/Pending/PendingWithEntry)       |
|  |   +-- template.rs           3-pass engine (use -> context -> inputs)              |
|  |   +-- transform.rs          30+ transform operations (map, filter, join, etc.)    |
|  |   +-- jsonpath.rs           JSONPath resolution for nested data                  |
|  |                                                                                  |
|  +-- provider/               LLM PROVIDERS                                          |
|  |   +-- rig.rs                RigProvider: 6 constructors + auto-detect             |
|  |   +-- native/               NativeRuntime: mistral.rs, GGUF, Metal/CUDA          |
|  |                                                                                  |
|  +-- mcp/                    MCP CLIENT                                             |
|  |   +-- mod.rs                McpClientPool: DashMap + OnceCell lazy connect        |
|  |   +-- error.rs              McpErrorCode: JSON-RPC -32700 to -32099              |
|  |                                                                                  |
|  +-- event/                  EVENT SOURCING                                         |
|  |   +-- log.rs                EventLog: 34 variants, broadcast channels             |
|  |   +-- trace.rs              TraceWriter: NDJSON, Arc<Mutex<BufWriter>>            |
|  |                                                                                  |
|  +-- store/                  STATE                                                  |
|  |   +-- mod.rs                DataStore: DashMap<Arc<str>, TaskResult>              |
|  |                              + RwLock<LoadedContext> + RwLock<FxHashMap> inputs    |
|  |                                                                                  |
|  +-- secrets/                CREDENTIALS (feature-gated: nika-daemon)               |
|  |   +-- mod.rs                SecretsManager: daemon IPC -> keychain -> env         |
|  |                                                                                  |
|  +-- tools/                  BUILTIN TOOLS                                          |
|  |   +-- mod.rs                BuiltinToolRouter: 11 tools                           |
|  |   +-- core_tools.rs         sleep, log, emit, assert, prompt, run (6)            |
|  |   +-- file_tools.rs         read, write, edit, glob, grep (5)                    |
|  |                                                                                  |
|  +-- tui/                    TERMINAL UI (164 files, 91.5K lines)                   |
|      +-- views/                4 views: Studio, Runner, Chat, Settings              |
|      +-- widgets/              Spinners, DAG viz, task panels, command palette       |
|      +-- edit_history.rs       Undo/redo with 500ms coalescing                      |
|      +-- session.rs            Auto-save/restore to .nika/sessions/                 |
|      +-- config.rs             .nika/config.toml                                    |
|                                                                                     |
+=====================================================================================+
```

### 2.2 The 5 Verbs

```
+=====================================================================================+
|  THE 5 SEMANTIC VERBS (ADR-001) — Nika's Action Taxonomy                            |
+=====================================================================================+
|                                                                                     |
|  +------+----------+------------------+-------------------------------------------+ |
|  | Icon | Verb     | Purpose          | Key Implementation Details                | |
|  +------+----------+------------------+-------------------------------------------+ |
|  |  ~~  | infer:   | LLM Generation   | RigProvider.infer() via rig-core v0.32    | |
|  |      |          |                  | 6 cloud providers + native (mistral.rs)   | |
|  |      |          |                  | temperature, system, max_tokens, thinking  | |
|  |      |          |                  | Shorthand: infer: "prompt here"           | |
|  +------+----------+------------------+-------------------------------------------+ |
|  |  $>  | exec:    | Shell Command    | shell: false by default (security)        | |
|  |      |          |                  | shlex parsing, command blocklist           | |
|  |      |          |                  | env: injection, working_dir               | |
|  |      |          |                  | Shorthand: exec: "npm run build"          | |
|  +------+----------+------------------+-------------------------------------------+ |
|  |  ->  | fetch:   | HTTP Request     | reqwest client, GET/POST/PUT/DELETE        | |
|  |      |          |                  | headers, json: auto-serialize, timeout     | |
|  |      |          |                  | No shorthand                              | |
|  +------+----------+------------------+-------------------------------------------+ |
|  |  <>  | invoke:  | MCP Tool Call    | McpClientPool, rmcp v0.16                 | |
|  |      |          |                  | Server auto-start, tool: + server:        | |
|  |      |          |                  | 30s timeout, JSON-RPC error codes         | |
|  +------+----------+------------------+-------------------------------------------+ |
|  |  @   | agent:   | Multi-Turn Loop  | RigAgentLoop, max_turns, depth_limit      | |
|  |      |          |                  | MCP tools + builtin tools + spawn_agent   | |
|  |      |          |                  | Extended thinking (Claude-only)            | |
|  |      |          |                  | Chat history, streaming                   | |
|  +------+----------+------------------+-------------------------------------------+ |
|                                                                                     |
|  RULE: No new verbs. Ever. New capabilities = modifiers on existing verbs.          |
|                                                                                     |
+=====================================================================================+
```

### 2.3 LLM Providers

```
+=====================================================================================+
|  7 LLM PROVIDERS — Auto-Detection Priority Order                                    |
+=====================================================================================+
|                                                                                     |
|  Priority   Provider    Env Variable          Default Model          Status          |
|  --------   --------    ---------------       ----------------       ------          |
|  1          Claude      ANTHROPIC_API_KEY     claude-sonnet-4-6     Primary         |
|  2          OpenAI      OPENAI_API_KEY        gpt-4o                Active          |
|  3          Mistral     MISTRAL_API_KEY       mistral-large-latest  Active          |
|  4          Groq        GROQ_API_KEY          llama-3.3-70b         Active          |
|  5          DeepSeek    DEEPSEEK_API_KEY      deepseek-chat         Active          |
|  6          Gemini      GEMINI_API_KEY        gemini-2.0-flash      Active          |
|  7          Native      (local GGUF file)     llama3.2:1b           Local only      |
|                                                                                     |
|  Auto-detect: RigProvider::auto() checks env vars in order 1-6.                     |
|  First found wins. Native requires explicit provider: native in YAML.               |
|                                                                                     |
|  All providers support:                                                             |
|    - Full streaming (real-time token delivery)                                      |
|    - Token tracking (input/output/total)                                            |
|    - Chat history (multi-turn conversations)                                        |
|    - Tool calling via rig-core ToolDyn trait                                        |
|                                                                                     |
|  Claude-only features:                                                              |
|    - Extended thinking (thinking_budget: 1024-65536)                                |
|    - Thinking capture in AgentTurnMetadata                                          |
|                                                                                     |
+=====================================================================================+
```

### 2.4 DAG Engine

```
+=====================================================================================+
|  IMMUTABLE DAG — Dependency Resolution & Execution                                  |
+=====================================================================================+
|                                                                                     |
|  CONSTRUCTION:                                                                      |
|  +----------+      +-----------+      +--------+      +------------------+          |
|  | YAML     | ---> | Raw AST   | ---> | Analyze| ---> | Dag (FxHashMap)  |          |
|  | workflow  |      | (spans)   |      | (valid)|      | (immutable)      |          |
|  +----------+      +-----------+      +--------+      +------------------+          |
|                                                                                     |
|  DATA STRUCTURES:                                                                   |
|  +-----------------------------------------------------------------------+          |
|  | Dag {                                                                  |          |
|  |   adjacency:    FxHashMap<Arc<str>, DepVec>    // task -> successors   |          |
|  |   predecessors: FxHashMap<Arc<str>, DepVec>    // task -> predecessors |          |
|  |   task_ids:     Vec<Arc<str>>                  // ordered task list    |          |
|  |   task_set:     FxHashSet<Arc<str>>            // O(1) existence check|          |
|  | }                                                                      |          |
|  |                                                                        |          |
|  | DepVec = SmallVec<[Arc<str>; 4]>  // 4 deps inline, heap if more      |          |
|  +-----------------------------------------------------------------------+          |
|                                                                                     |
|  CYCLE DETECTION: 3-color DFS (White/Gray/Black)                                    |
|  IMPLICIT DEPS:  use:/with: bindings auto-create flow edges                         |
|  TASK ID RULES:  snake_case only [a-z0-9_]+                                         |
|                                                                                     |
|  EXECUTION (Runner):                                                                |
|                                                                                     |
|  Layer 0: [task_a]  [task_b]          <-- parallel (JoinSet)                        |
|              |         |                                                            |
|  Layer 1: [task_c]                    <-- waits for layer 0                         |
|              |                                                                      |
|  Layer 2: [task_d]  [task_e]          <-- parallel                                  |
|              |         |                                                            |
|  Layer 3: [task_f]                    <-- final aggregation                         |
|                                                                                     |
|  Concurrency: tokio::sync::Semaphore limits parallel tasks                          |
|  Pause/Resume: AtomicBool + Notify for interactive control                          |
|  Cancellation: CancellationToken via tokio::select!                                 |
|  fail_fast: true stops all in-flight tasks on first error                           |
|                                                                                     |
+=====================================================================================+
```

### 2.5 Agent Architecture

```
+=====================================================================================+
|  AGENT HIERARCHY — The Mascots                                                      |
+=====================================================================================+
|                                                                                     |
|                         Nika (Runtime)                                               |
|                              |                                                      |
|                     +--------+--------+                                              |
|                     |                 |                                              |
|               [infer: task]    [agent: task]                                         |
|               Single shot      Multi-turn                                           |
|                                    |                                                |
|                          +---------+---------+                                       |
|                          |                   |                                       |
|                    [MCP tools]        [spawn_agent]                                  |
|                    novanet_*           (internal tool)                               |
|                    nika:read               |                                         |
|                    nika:write        [subagent]                                      |
|                    nika:grep          depth - 1                                      |
|                    etc.               inherits MCP                                   |
|                                            |                                        |
|                                      [sub-subagent]                                  |
|                                       depth - 2                                      |
|                                       (until depth_limit = 0)                        |
|                                                                                     |
|  DEPTH PROTECTION:                                                                  |
|  +------------------------------------------------------------------+               |
|  | depth_limit: 3 (default) to 10 (max)                             |               |
|  | Each spawn_agent decrements depth by 1                           |               |
|  | At depth 0: spawn_agent tool is NOT registered                   |               |
|  | Prevents infinite recursion                                       |               |
|  +------------------------------------------------------------------+               |
|                                                                                     |
|  AGENT LOOP (RigAgentLoop):                                                         |
|  +------------------------------------------------------------------+               |
|  | 1. Build tool list (MCP + builtins + spawn_agent if depth > 0)   |               |
|  | 2. Create rig AgentBuilder with tools                             |               |
|  | 3. Send prompt + history                                          |               |
|  | 4. Agent responds with text or tool_calls                         |               |
|  | 5. Execute tool calls, append results                             |               |
|  | 6. Repeat until: max_turns, stop condition, or no tool calls      |               |
|  | 7. Emit AgentTurn event per turn with metadata                    |               |
|  +------------------------------------------------------------------+               |
|                                                                                     |
|  STOP CONDITIONS:                                                                   |
|  - max_turns exceeded -> MaxTurnsExceeded                                           |
|  - CancellationToken fired -> CancelledByUser                                       |
|  - Tool error (non-recoverable) -> ToolError                                        |
|  - Agent says "DONE" or stops calling tools -> Success                              |
|                                                                                     |
+=====================================================================================+
```

### 2.6 Binding & Transform System

```
+=====================================================================================+
|  DATA FLOW — Binding, Templates, Transforms                                         |
+=====================================================================================+
|                                                                                     |
|  TWO BINDING SYSTEMS (coexist today):                                               |
|                                                                                     |
|  LEGACY (use:)                         NEW (with: v0.28)                            |
|  +---------------------------+         +---------------------------+                |
|  | use:                      |         | with:                     |                |
|  |   result: $step1          |         |   result: $step1          |                |
|  |   data: step2.output      |         |   data:                   |                |
|  |                           |         |     path: step2.output    |                |
|  | Template: {{use.result}}  |         |     transform: uppercase  |                |
|  +---------------------------+         |                           |                |
|                                        | Template: {{with.result}} |                |
|                                        +---------------------------+                |
|                                                                                     |
|  TEMPLATE ENGINE (3-pass):                                                          |
|  +------------------------------------------------------------------+               |
|  | Pass 1: Resolve {{use.xxx}} / {{with.xxx}} from DataStore        |               |
|  | Pass 2: Resolve {{context.files.xxx}} from LoadedContext          |               |
|  | Pass 3: Resolve {{inputs.xxx}} from workflow inputs               |               |
|  +------------------------------------------------------------------+               |
|                                                                                     |
|  LAZY BINDINGS:                                                                     |
|  +------------------------------------------------------------------+               |
|  | with:                                                             |               |
|  |   eager_val: $step1              # resolved immediately          |               |
|  |   lazy_val:                                                       |               |
|  |     path: future_step.result     # resolved on first access      |               |
|  |     lazy: true                                                    |               |
|  |     default: "fallback"          # if still unresolved            |               |
|  +------------------------------------------------------------------+               |
|                                                                                     |
|  30+ TRANSFORM OPERATIONS:                                                          |
|  +------------------------------------------------------------------+               |
|  | String:   uppercase, lowercase, trim, replace, split, join       |               |
|  | Array:    map, filter, first, last, length, sort, unique, flat    |               |
|  | Object:   pick, omit, merge, keys, values                        |               |
|  | Type:     to_string, to_number, to_bool, parse_json              |               |
|  | Format:   template, markdown, json_pretty                        |               |
|  | Logic:    if_empty, default, coalesce                             |               |
|  +------------------------------------------------------------------+               |
|                                                                                     |
+=====================================================================================+
```

### 2.7 Structured Output

```
+=====================================================================================+
|  4-LAYER STRUCTURED OUTPUT — JSON Schema Validation Pipeline                        |
+=====================================================================================+
|                                                                                     |
|                    LLM Response                                                     |
|                        |                                                            |
|                        v                                                            |
|  +------------------------------------------------------------------+               |
|  | LAYER 1: Extract JSON                                             |               |
|  | Try 4 strategies:                                                 |               |
|  |   1. Direct JSON parse                                            |               |
|  |   2. Extract from ```json ``` blocks                              |               |
|  |   3. Extract from ``` ``` blocks                                  |               |
|  |   4. Bracket matching (find outermost { } or [ ])                |               |
|  +------------------------------------------------------------------+               |
|                        |                                                            |
|                        v                                                            |
|  +------------------------------------------------------------------+               |
|  | LAYER 2: Validate against JSON Schema                             |               |
|  | Uses cached schema (global DashMap)                                |               |
|  | If valid -> return                                                 |               |
|  +------------------------------------------------------------------+               |
|                        |                                                            |
|                   FAILED v                                                           |
|  +------------------------------------------------------------------+               |
|  | LAYER 3: Retry with Feedback                                      |               |
|  | Send validation errors back to LLM:                               |               |
|  | "Your response failed validation: {errors}. Fix and retry."       |               |
|  | Up to 2 retry attempts                                            |               |
|  +------------------------------------------------------------------+               |
|                        |                                                            |
|                   FAILED v                                                           |
|  +------------------------------------------------------------------+               |
|  | LAYER 4: LLM Repair                                               |               |
|  | Different LLM call specifically for JSON repair:                   |               |
|  | "Fix this JSON to match the schema: {broken_json}"                |               |
|  | Last resort before failing the task                                |               |
|  +------------------------------------------------------------------+               |
|                                                                                     |
+=====================================================================================+
```

### 2.8 Event Sourcing

```
+=====================================================================================+
|  34 EVENT TYPES — Full Observability                                                |
+=====================================================================================+
|                                                                                     |
|  WORKFLOW EVENTS:                        AGENT EVENTS:                              |
|  +----------------------------+          +----------------------------+              |
|  | WorkflowStarted            |          | AgentStarted               |              |
|  | WorkflowCompleted          |          | AgentTurn (per turn)       |              |
|  | WorkflowFailed             |          |   .thinking (Claude)       |              |
|  | WorkflowCancelled          |          |   .response_text           |              |
|  +----------------------------+          |   .input/output_tokens     |              |
|                                          |   .tool_calls              |              |
|  TASK EVENTS:                            | AgentCompleted             |              |
|  +----------------------------+          | AgentSpawned               |              |
|  | TaskStarted                |          |   .parent_task_id          |              |
|  | TaskCompleted              |          |   .child_task_id           |              |
|  | TaskFailed                 |          |   .current_depth           |              |
|  | TaskCancelled (NIKA-027)   |          +----------------------------+              |
|  | TaskSkipped                |                                                     |
|  | DependencyFailed (NIKA-025)|          MCP EVENTS:                                |
|  +----------------------------+          +----------------------------+              |
|                                          | McpServerStarted           |              |
|  PROVIDER EVENTS:                        | McpServerStopped           |              |
|  +----------------------------+          | McpToolCalled              |              |
|  | ProviderSelected           |          | McpToolResult              |              |
|  | InferStarted               |          | McpError                   |              |
|  | InferCompleted             |          +----------------------------+              |
|  | InferFailed                |                                                     |
|  | InferStreaming             |          ARTIFACT EVENTS:                            |
|  +----------------------------+          +----------------------------+              |
|                                          | ArtifactWritten            |              |
|  CUSTOM EVENTS:                          | ArtifactFailed             |              |
|  +----------------------------+          +----------------------------+              |
|  | UserEvent (nika:emit)      |                                                     |
|  | LogEvent (nika:log)        |          TRACE: NDJSON file per run                 |
|  +----------------------------+          Broadcast: tokio channels for TUI           |
|                                                                                     |
+=====================================================================================+
```

### 2.9 Secrets & Daemon

```
+=====================================================================================+
|  SECRETS MANAGEMENT — Feature-Gated Daemon IPC                                      |
+=====================================================================================+
|                                                                                     |
|  WITHOUT DAEMON:                       WITH DAEMON (--features nika-daemon):        |
|  +-----------------------------+       +--------------------------------------+     |
|  |                             |       |                                      |     |
|  | Nika -> env var lookup      |       | Nika -> Unix socket -> daemon.sock   |     |
|  | MCP1 -> env var lookup      |       |                          |           |     |
|  | MCP2 -> env var lookup      |       |                     OS Keychain      |     |
|  |                             |       |                    (one popup)        |     |
|  | Problem: Multiple keychain  |       |                                      |     |
|  | popups on macOS             |       | Resolution chain:                    |     |
|  +-----------------------------+       | 1. Env var (highest priority)        |     |
|                                        | 2. Daemon IPC (Unix socket)          |     |
|                                        | 3. Not found                         |     |
|                                        +--------------------------------------+     |
|                                                                                     |
|  DAEMON SECURITY:                                                                   |
|  +------------------------------------------------------------------+               |
|  | Socket permissions:    0600 (owner-only)                          |               |
|  | Peer verification:    SO_PEERCRED / LOCAL_PEERCRED                |               |
|  | Single instance:      PID file with flock()                       |               |
|  | Memory protection:    mlock(), MADV_DONTDUMP, Zeroizing<T>        |               |
|  | Key type:             SecretString (no Debug/Display)              |               |
|  +------------------------------------------------------------------+               |
|                                                                                     |
|  BOOT SEQUENCE:                                                                     |
|  +------------------------------------------------------------------+               |
|  | 1. Load SecretsLoadResult at startup                               |               |
|  | 2. For each KNOWN_PROVIDER: resolve key via chain                  |               |
|  | 3. Inject found keys as env vars (for MCP server processes)        |               |
|  | 4. Report: loaded N keys, failed M, skipped K                     |               |
|  +------------------------------------------------------------------+               |
|                                                                                     |
+=====================================================================================+
```

### 2.10 Core Registry

```
+=====================================================================================+
|  CORE MODULE — Zero-Dependency Static Registry (v0.27)                              |
+=====================================================================================+
|                                                                                     |
|  KNOWN_PROVIDERS (18 total):                                                        |
|  +------------------------------------------------------------------+               |
|  | LLM Providers (6):                                                |               |
|  |   anthropic, openai, mistral, groq, deepseek, gemini             |               |
|  |                                                                   |               |
|  | MCP Providers (11):                                               |               |
|  |   neo4j, github, slack, perplexity, firecrawl, supadata,         |               |
|  |   brave-search, exa, tavily, jina, serper                        |               |
|  |                                                                   |               |
|  | Local Provider (1):                                               |               |
|  |   native (mistral.rs GGUF inference)                              |               |
|  +------------------------------------------------------------------+               |
|                                                                                     |
|  KNOWN_MODELS (16+):                                                                |
|  +------------------------------------------------------------------+               |
|  | Text:      llama3.2:1b, llama3.2:3b, qwen3:8b, mistral:7b       |               |
|  | Vision:    llava:7b, llava:13b                                    |               |
|  | Embedding: nomic-embed-text, mxbai-embed-large                    |               |
|  | Each with: HF repo path, size, quantization, description          |               |
|  +------------------------------------------------------------------+               |
|                                                                                     |
|  MCP_ALIASES (48 in 6 categories):                                                  |
|  +------------------------------------------------------------------+               |
|  | AI/LLM:     anthropic, openai, ollama, ...                        |               |
|  | Data:       neo4j, postgres, sqlite, ...                          |               |
|  | Search:     perplexity, brave-search, exa, ...                    |               |
|  | Dev:        github, gitlab, linear, ...                            |               |
|  | Comms:      slack, discord, ...                                    |               |
|  | Files:      filesystem, google-drive, ...                          |               |
|  +------------------------------------------------------------------+               |
|                                                                                     |
|  MCP CONFIG (3-level hierarchy):                                                    |
|  +------------------------------------------------------------------+               |
|  | 1. Global:   ~/.nika/mcp.yaml        (user defaults)              |               |
|  | 2. Project:  ./.nika/mcp.yaml        (project overrides)          |               |
|  | 3. Workflow:  mcp: block in YAML      (workflow-specific)          |               |
|  |                                                                   |               |
|  | Merge strategy: workflow > project > global                        |               |
|  +------------------------------------------------------------------+               |
|                                                                                     |
+=====================================================================================+
```

---

## 3. NovaNet Deep Dive

```
+=====================================================================================+
|  NOVANET v0.20.0 — Knowledge Graph for AI Agents                                    |
+=====================================================================================+
|                                                                                     |
|  ARCHITECTURE:                                                                      |
|  +------------------------------------------------------------------+               |
|  | Neo4j Graph Database                                              |               |
|  |   |                                                               |               |
|  |   +-- 59 NodeClasses (organized in 2 realms, 11 layers)          |               |
|  |   +-- 159 ArcClasses (organized in 5 families)                   |               |
|  |   +-- Fulltext indexes for search                                 |               |
|  |   +-- Constraint-based validation                                 |               |
|  |                                                                   |               |
|  | MCP Server (8 tools)                                              |               |
|  |   |                                                               |               |
|  |   +-- novanet_describe    Bootstrap: understand the graph         |               |
|  |   +-- novanet_introspect  Schema: classes, arcs, properties       |               |
|  |   +-- novanet_search      Find: fulltext, property, walk, hybrid  |               |
|  |   +-- novanet_context     Context: page, block, knowledge, assemble|              |
|  |   +-- novanet_write       Mutate: upsert_node, create_arc, update |               |
|  |   +-- novanet_audit       Quality: CSR metrics, coverage, orphans |               |
|  |   +-- novanet_batch       Parallel: multiple ops in one request   |               |
|  |   +-- novanet_query       Last resort: raw Cypher                 |               |
|  +------------------------------------------------------------------+               |
|                                                                                     |
|  SCHEMA:                                                                            |
|  +------------------------------------------------------------------+               |
|  |                                                                   |               |
|  | SHARED Realm (36 nodes, READ-ONLY):                               |               |
|  |   config (3):    LocaleConfig, Currency, NumberFormat             |               |
|  |   locale (5):    Locale, Script, Region, ...                      |               |
|  |   geography (7): Country, City, Timezone, ...                     |               |
|  |   knowledge (21): TermSet, Term, ExpressionSet, Expression,       |               |
|  |                   Pattern, CultureRef, Taboo, AudienceTrait, ...  |               |
|  |                                                                   |               |
|  | ORG Realm (23 nodes):                                             |               |
|  |   config (1):      OrgConfig                                      |               |
|  |   foundation (8):  Project, Brand, Audience, Competitor, ...      |               |
|  |   structure (3):   Page, Block, Section                           |               |
|  |   semantic (2):    Entity, EntityNative                           |               |
|  |   instruction (3): BlockType, BlockInstruction, PageStructure     |               |
|  |   output (6):      PageNative, BlockNative, SEOKeyword, ...       |               |
|  |                                                                   |               |
|  +------------------------------------------------------------------+               |
|                                                                                     |
|  KNOWLEDGE ATOMS (6 types):                                                         |
|  +------------------------------------------------------------------+               |
|  | Term          Technical vocabulary with definitions                |               |
|  | Expression    Idiomatic expressions per locale                     |               |
|  | Pattern       Text templates and patterns                          |               |
|  | CultureRef    Cultural references                                  |               |
|  | Taboo         Things to avoid in a locale                          |               |
|  | AudienceTrait Audience characteristics                             |               |
|  +------------------------------------------------------------------+               |
|                                                                                     |
|  ARC FAMILIES (5):                                                                  |
|  +------------------------------------------------------------------+               |
|  | ownership     HAS_ENTITY, HAS_PAGE, HAS_BLOCK, ...                |               |
|  | localization  FOR_LOCALE, HAS_NATIVE, LOCALE_OF, ...              |               |
|  | semantic      USES_ENTITY, REPRESENTS, RELATED_TO, ...            |               |
|  | generation    GENERATED_FROM, BASED_ON, ...                        |               |
|  | mining        MINED_FROM, EXTRACTED_FROM, ...                      |               |
|  +------------------------------------------------------------------+               |
|                                                                                     |
|  KEY PATTERNS:                                                                      |
|  +------------------------------------------------------------------+               |
|  | *Native:        Entity -> EntityNative (authored per locale)       |               |
|  |                 Page -> PageNative (generated per locale)          |               |
|  | Denomination:   6 forms: text/title/abbrev/mixed/base/url          |               |
|  | CSR Audit:      >= 0.95 healthy, 0.85-0.95 warning, <0.85 critical|               |
|  | Context Modes:  page (all blocks), block (single), knowledge,      |               |
|  |                 assemble (custom strategy with token budget)        |               |
|  +------------------------------------------------------------------+               |
|                                                                                     |
+=====================================================================================+
```

---

## 4. Scientific Literature

### 4.1 Paper-by-Paper Findings

```
+=====================================================================================+
|  6 PAPERS ANALYZED — Key Insights for Nika                                          |
+=====================================================================================+

+-------------------------------------------------------------------------------------+
|  PAPER 1: RLM — Recursive Language Models (MIT, 2025)                               |
+-------------------------------------------------------------------------------------+
|                                                                                     |
|  CORE IDEA:  8B model matches GPT-4 on long tasks using REPL as working memory      |
|                                                                                     |
|  HOW IT WORKS:                                                                      |
|    Problem                                                                          |
|      |                                                                              |
|      +---> Decompose into sub-problems                                              |
|      |        |                                                                     |
|      |        +---> Execute in REPL (Python/shell)                                  |
|      |        |        |                                                            |
|      |        |        +---> Store result in variable (ref semantics)               |
|      |        |                                                                     |
|      |        +---> Execute next sub-problem...                                     |
|      |                                                                              |
|      +---> Compose final output from references                                     |
|                                                                                     |
|  NIKA ALREADY HAS:                          NIKA GAPS:                              |
|  +-----------------------------+            +-----------------------------+          |
|  | DataStore = REPL vars       |            | Dynamic DAG generation      |          |
|  | $task refs = reference sem. |            | (agent can't create new     |          |
|  | spawn_agent = recursive     |            |  workflow steps at runtime) |          |
|  | decompose: = task decomp.   |            |                             |          |
|  | Coverage: ~70%              |            |                             |          |
|  +-----------------------------+            +-----------------------------+          |
|                                                                                     |
+-------------------------------------------------------------------------------------+

+-------------------------------------------------------------------------------------+
|  PAPER 2: CodeAct (ICML 2024, arXiv:2402.01030)                                    |
+-------------------------------------------------------------------------------------+
|                                                                                     |
|  CORE IDEA:  LLM writes executable code instead of choosing JSON tools.             |
|              +20% success rate vs function-calling.                                  |
|                                                                                     |
|  JSON tools (3 round-trips):          CodeAct (1 round-trip):                       |
|  search("nika") -> result             results = search("nika")                      |
|  filter(result) -> filtered           filtered = [r for r in results                |
|  format(filtered) -> final                         if r.score > 0.8]                |
|                                       print(format_table(filtered))                 |
|                                                                                     |
|  NIKA ALREADY HAS:                          NIKA GAPS:                              |
|  +-----------------------------+            +-----------------------------+          |
|  | exec: verb (shell commands) |            | No code sandbox (Python/JS)|          |
|  | 30+ transform operations    |            | Transforms are declarative  |          |
|  | Structured output retry     |            | No self-debugging loops     |          |
|  +-----------------------------+            +-----------------------------+          |
|                                                                                     |
|  VERDICT: Low priority. Nika's transform engine + MCP tools cover most cases.       |
|  A future `code:` mode for exec: could add this without a new verb.                 |
|                                                                                     |
+-------------------------------------------------------------------------------------+

+-------------------------------------------------------------------------------------+
|  PAPER 3: THREAD (IJCAI 2025, arXiv:2405.17402)                                    |
+-------------------------------------------------------------------------------------+
|                                                                                     |
|  CORE IDEA:  Hierarchical decomposition with per-task model routing.                |
|              Manager (big model) plans, Workers (small models) execute.              |
|              10-50% improvement on complex tasks.                                    |
|                                                                                     |
|                  Manager Thread (Claude Sonnet)                                      |
|                  "Plan the research"                                                |
|                         |                                                           |
|            +------------+------------+                                               |
|            |            |            |                                               |
|      Worker 1      Worker 2      Worker 3                                           |
|      (Groq/fast)   (Groq/fast)   (Groq/fast)                                       |
|      "Search A"    "Search B"    "Analyze C"                                        |
|                                                                                     |
|  NIKA ALREADY HAS:                          NIKA GAPS:                              |
|  +-----------------------------+            +-----------------------------+          |
|  | spawn_agent (depth 3-10)    |            | No per-task model routing   |          |
|  | Agent loop with max_turns   |            | No context compression      |          |
|  | Parallel execution          |            | No strategy/tactics split   |          |
|  +-----------------------------+            +-----------------------------+          |
|                                                                                     |
|  DIRECTLY INSPIRES: P2 (multi-model) + P1 (strategy/tactics)                       |
|                                                                                     |
+-------------------------------------------------------------------------------------+

+-------------------------------------------------------------------------------------+
|  PAPER 4: Context-Folding (arXiv:2510.11967)                                       |
+-------------------------------------------------------------------------------------+
|                                                                                     |
|  CORE IDEA:  Branch/fold operations compress agent trajectories.                    |
|              10x smaller active context. Quality maintained.                         |
|                                                                                     |
|  WITHOUT FOLDING:                                                                   |
|  [S1] [S2] [S3] [S4] [S5] [S6] ... [S20]  context grows linearly                  |
|                                             quality degrades ~step 20               |
|                                                                                     |
|  WITH FOLDING:                                                                      |
|  [S1] [S2] -> branch -> [sub1][sub2][sub3] -> fold -> [Summary]                    |
|  [S1] [S2] [Summary] [S7] -> ...           context stays bounded                   |
|                                                                                     |
|  NIKA ALREADY HAS:                          NIKA GAPS:                              |
|  +-----------------------------+            +-----------------------------+          |
|  | spawn_agent = branch        |            | No result compression       |          |
|  | use: {result: $child}       |            | Child trace not summarized  |          |
|  | max_turns = blunt bound     |            | No automatic folding        |          |
|  +-----------------------------+            +-----------------------------+          |
|                                                                                     |
|  DIRECTLY INSPIRES: P1 (fold: true for sub-DAG result compression)                  |
|                                                                                     |
+-------------------------------------------------------------------------------------+

+-------------------------------------------------------------------------------------+
|  PAPER 5: LLM Swarms (arXiv:2506.14496)                                            |
+-------------------------------------------------------------------------------------+
|                                                                                     |
|  CORE FINDING:                                                                      |
|    Rule-based swarms:  Fast for optimization (PSO, ant colony)                      |
|    LLM swarms:         Good for creative, open-ended tasks                          |
|    HYBRID:             Optimal = rules for structure + LLM for decisions             |
|                                                                                     |
|                                                                                     |
|    Nika IS hybrid:     DAG = rules (structure, order, parallelism)                  |
|                        LLM = decisions (infer:, agent:)                              |
|                                                                                     |
|  VERDICT: VALIDATION. Nika's architecture is already correct.                       |
|  Don't go pure-swarm. Keep structured DAG. This is Nika's strength.                 |
|                                                                                     |
+-------------------------------------------------------------------------------------+

+-------------------------------------------------------------------------------------+
|  PAPER 6: Memory-R1 (2025)                                                          |
+-------------------------------------------------------------------------------------+
|                                                                                     |
|  CORE IDEA:  RL-trained memory policies beat simple retrieval.                      |
|              Agents learn WHEN to store, retrieve, and forget.                       |
|                                                                                     |
|  RELEVANCE:  P4 episodic memory should have intelligent recall,                     |
|              not just "retrieve top-5 similar episodes".                             |
|              Future: RL-trained memory policies.                                     |
|                                                                                     |
+-------------------------------------------------------------------------------------+
```

### 4.2 Literature Synthesis

```
+=====================================================================================+
|  WHAT THE LITERATURE SAYS NIKA SHOULD DO                                            |
+=====================================================================================+
|                                                                                     |
|  HIGH IMPACT + LOW EFFORT:                                                          |
|  +------------------------------------------------------------------+               |
|  |  4-slot model routing (THREAD + Slate)         ---> P-MODEL       |               |
|  |  Runtime introspection tools (self-aware)      ---> P-INTROSPECT  |               |
|  +------------------------------------------------------------------+               |
|                                                                                     |
|  HIGH IMPACT + MEDIUM EFFORT:                                                       |
|  +------------------------------------------------------------------+               |
|  |  Episode compression at completion boundary    ---> P-EPISODE     |               |
|  |  Context budget / working memory awareness     ---> P-CONTEXT     |               |
|  |  (Old P3 ConfidenceRouter absorbed into P-EPISODE confidence)     |               |
|  +------------------------------------------------------------------+               |
|                                                                                     |
|  HIGH IMPACT + HIGH EFFORT:                                                         |
|  +------------------------------------------------------------------+               |
|  |  Strategy orchestration + dynamic dispatch     ---> P-STRATEGY    |               |
|  |  NovaNet episodic memory (cross-session)       ---> P-MEMORY      |               |
|  +------------------------------------------------------------------+               |
|                                                                                     |
|  ALREADY VALIDATED (Nika does it right):                                            |
|  +------------------------------------------------------------------+               |
|  |  Hybrid DAG+LLM architecture (Swarms paper)                       |               |
|  |  Reference semantics via DataStore (RLM)                          |               |
|  |  Recursive spawning with depth limits (THREAD)                    |               |
|  |  Structured output validation (CodeAct self-correction)           |               |
|  +------------------------------------------------------------------+               |
|                                                                                     |
+=====================================================================================+
```

---

## 5. Competitive Landscape

### 5.1 Market Map

```
+=====================================================================================+
|  AGENT RUNTIME LANDSCAPE — March 2026                                               |
+=====================================================================================+
|                                                                                     |
|                    Declarative                                                       |
|                        ^                                                            |
|                        |                                                            |
|              Dify      |     NIKA                                                   |
|              (visual)  |     (YAML + KG + multi-locale)                             |
|                        |                                                            |
|   Simple <-------------+-------------> Complex                                      |
|                        |                                                            |
|              CrewAI    |     LangGraph                                               |
|              (roles)   |     (Python stateful)                                       |
|                        |                                                            |
|                        v                                                            |
|                   Imperative                                                        |
|                                                                                     |
|  CODING AGENTS (different category):                                                |
|  +------------------------------------------------------------------+               |
|  | Claude Code    Conversational, hooks/skills, Nika's USER          |               |
|  | Codex          Cloud sandbox, PR-oriented                         |               |
|  | Devin          Full dev environment                                |               |
|  | Slate          Swarm-native, thread compression, episodic memory   |               |
|  | Cursor/Cline   IDE-embedded                                        |               |
|  +------------------------------------------------------------------+               |
|                                                                                     |
|  PROTOCOLS:                                                                         |
|  +------------------------------------------------------------------+               |
|  | MCP (Anthropic)   Agent <-> Tools    (intra-agent)                |               |
|  | A2A (Google/LF)   Agent <-> Agent    (inter-agent)                |               |
|  | ACP (various)     Agent communication (emerging)                   |               |
|  +------------------------------------------------------------------+               |
|                                                                                     |
+=====================================================================================+
```

### 5.2 Slate — The Primary Competitor (Deep Analysis)

> **See doc 03 for full technical breakdown and doc 07 for complete Slate → Nika integration strategy.**

```
+=====================================================================================+
|  SLATE by Random Labs — Deep Architecture Analysis                                  |
+=====================================================================================+
|                                                                                     |
|  CORE INSIGHT: Slate solves context window degradation (the "dumb zone")            |
|  via one-shot threads + episode compression + thread weaving.                       |
|                                                                                     |
|  8 INTERCONNECTED CONCEPTS:                                                         |
|  +------------------------------------------------------------------+               |
|  | 1. Working Memory       Never exceed usable context. Beyond it   |               |
|  |    & Dumb Zone          lies the "dumb zone" where LLM degrades. |               |
|  |                         ≠ compaction (lossy, unpredictable).      |               |
|  |                                                                   |               |
|  | 2. Threads              One-shot execution units. NOT persistent  |               |
|  |    (NOT subagents)      subagents. Execute one action → episode   |               |
|  |                         → return control to orchestrator.         |               |
|  |                                                                   |               |
|  | 3. Episodes             Compressed representation at the NATURAL  |               |
|  |    (Completion Boundary) completion boundary. The executing agent |               |
|  |                         decides what's important. ≠ compaction.   |               |
|  |                                                                   |               |
|  | 4. Thread Weaving       Orchestrator loop: dispatch → episodes →  |               |
|  |    (Adaptive Decomp.)   synthesize → dispatch next. No explicit  |               |
|  |                         plan. ≠ markdown planning (3 failure      |               |
|  |                         modes: underspec, incomplete, stale).     |               |
|  |                                                                   |               |
|  | 5. Strategy / Tactics   Strategy = open-ended planning (value     |               |
|  |    (AlphaZero Mapping)  network). Tactics = learned sequences    |               |
|  |                         (policy network). Orchestrator =          |               |
|  |                         strategist. Threads = tacticians.         |               |
|  |                                                                   |               |
|  | 6. Knowledge Overhang   Models have knowledge they can't access   |               |
|  |                         without scaffolding. Episodes provide     |               |
|  |                         scaffolding that activates latent         |               |
|  |                         knowledge. A systems problem, not         |               |
|  |                         capability.                               |               |
|  |                                                                   |               |
|  | 7. Composability        Episodes as inputs to other threads.      |               |
|  |                         Cross-model composition: different models |               |
|  |                         across threads, episodes as clean handoff.|               |
|  |                                                                   |               |
|  | 8. OS Framing           Orchestrator = kernel. Threads =          |               |
|  |                         processes. Episodes = process return      |               |
|  |                         values. Context = RAM.                    |               |
|  +------------------------------------------------------------------+               |
|                                                                                     |
|  SLATE'S 4 MODEL SLOTS:                                                             |
|  main → Primary reasoning (expensive, capable)                                      |
|  subagent → Thread execution (cheaper, faster)                                      |
|  search → Information retrieval (fast, cheap)                                       |
|  reasoning → Planning, review, critique (deep thinking)                             |
|                                                                                     |
|  CRITICAL REALIZATION:                                                              |
|  Nika's DAG IS already Slate's kernel. Tasks ARE processes. TaskResult              |
|  IS return values. EventLog IS process accounting. We don't rebuild —               |
|  we add 4 KERNEL UPGRADES: model slots, episodes, strategy mode,                   |
|  context budgeting.                                                                 |
|                                                                                     |
+=====================================================================================+
```

**Head-to-Head Comparison (revised with deep understanding):**

```
+=====================================================================================+
|  SLATE vs NIKA — COMPLETE COMPARISON                                                |
+=====================================================================================+
|                                                                                     |
|  WHERE SLATE LEADS (gaps Nika must close):                                          |
|  +-------------------------------+-----------+-----------+-----------+               |
|  | Capability                    | Slate     | Nika      | Priority  |               |
|  +-------------------------------+-----------+-----------+-----------+               |
|  | Context management            | Working   | None      | P-CONTEXT |               |
|  |                               | memory    |           |           |               |
|  | Model routing                  | 4 slots   | 1 prov.   | P-MODEL   |               |
|  | Episode compression            | Core      | None      | P-EPISODE |               |
|  | Strategy/tactics               | Native    | Flat loop | P-STRATEGY|               |
|  | Cross-session memory           | Session   | In-memory | P-MEMORY  |               |
|  | Adaptive decomposition         | Weaving   | Static    | P-STRATEGY|               |
|  +-------------------------------+-----------+-----------+-----------+               |
|                                                                                     |
|  WHERE NIKA LEADS (moat Slate cannot replicate):                                    |
|  +-------------------------------+-----------+-----------+-----------+               |
|  | Capability                    | Nika      | Slate     | Moat      |               |
|  +-------------------------------+-----------+-----------+-----------+               |
|  | Declarative workflows          | YAML DAG  | TypeScript| Strong    |               |
|  | Knowledge graph                | NovaNet   | None      | Unique    |               |
|  | Reproducibility                | NDJSON    | Non-determ| Strong    |               |
|  | Security (exec hardening)      | shell:    | Not doc'd | Strong    |               |
|  | Structured output (4-layer)    | parse →   | Not doc'd | Strong    |               |
|  |                               | validate  |           |           |               |
|  |                               | → retry   |           |           |               |
|  |                               | → repair  |           |           |               |
|  | Multi-locale (200+)            | NovaNet   | English   | Unique    |               |
|  | Observability (34 events)      | Full      | Basic     | Strong    |               |
|  | Provider independence (7)      | rig-core  | Unknown   | Strong    |               |
|  | Episode persistence            | NovaNet   | Session   | BEYOND    |               |
|  |   (future, P-MEMORY)          | (graph)   | (files)   | Slate     |               |
|  +-------------------------------+-----------+-----------+-----------+               |
|                                                                                     |
|  SCORE: Slate leads on 6 (all targeted by new 6 priorities)                         |
|         Nika leads on 9 (all existing strengths = moat)                              |
|         Nika GOES BEYOND on 1 (NovaNet episodic memory > session files)             |
|                                                                                     |
|  TAKEAWAY: Slate's advantages map 1:1 to our 6 evolution priorities.                |
|            After Wave 3, Nika has PARITY on Slate's strengths plus                  |
|            9 capabilities Slate cannot replicate.                                    |
|                                                                                     |
+=====================================================================================+
```

### 5.3 Other Competitors (Quick Comparison)

```
+=====================================================================================+
|  QUICK COMPETITIVE MATRIX                                                           |
+=====================================================================================+
|                                                                                     |
|  +-------------------+------------+-------------+---------+--------+---------+      |
|  | Feature           | Nika       | LangGraph   | CrewAI  | Codex  | Claude  |      |
|  |                   |            |             |         |        | Code    |      |
|  +-------------------+------------+-------------+---------+--------+---------+      |
|  | Language          | Rust+YAML  | Python      | Python  | Cloud  | CLI     |      |
|  | Workflow format   | YAML DAG   | Python code | Python  | None   | Conv.   |      |
|  | Performance       | Fast       | Slow        | Slow    | Cloud  | N/A     |      |
|  | Multi-provider    | 7          | Via LC      | 1-2     | OpenAI | Claude  |      |
|  | Knowledge graph   | NovaNet    | Manual      | None    | None   | None    |      |
|  | Multi-locale      | 200+       | None        | None    | None   | None    |      |
|  | Memory            | Session    | Checkpoints | 3-type  | None   | Conv.   |      |
|  | Observability     | 34 events  | LangSmith   | Basic   | PRs    | Conv.   |      |
|  | Reproducibility   | NDJSON     | Low         | Low     | PR     | Low     |      |
|  | MCP integration   | Native     | Plugin      | None    | None   | Native  |      |
|  | Version control   | YAML files | Python      | Python  | N/A    | N/A     |      |
|  +-------------------+------------+-------------+---------+--------+---------+      |
|                                                                                     |
|  POSITIONING:                                                                       |
|                                                                                     |
|  Claude Code = Nika's USER (symbiotic, not competitive)                             |
|  Codex       = Different market (PR-as-output vs workflow-as-artifact)              |
|  LangGraph   = Python flexibility vs Nika's YAML reproducibility                   |
|  CrewAI      = Role-based (intuitive) vs Verb-based (precise)                      |
|                CrewAI's 3-type memory is MORE MATURE than Nika's                    |
|                                                                                     |
|  NIKA'S MOAT (no competitor has ALL of these):                                      |
|  +------------------------------------------------------------------+               |
|  | 1. NovaNet knowledge graph (curated, not auto-generated)          |               |
|  | 2. YAML DSL (workflows as version-controlled artifacts)           |               |
|  | 3. 200+ locales (no one else attempts multi-locale at this scale) |               |
|  | 4. Rust performance + tokio concurrency                           |               |
|  | 5. 34-event observability + NDJSON traces                         |               |
|  | 6. 4-layer structured output validation                           |               |
|  +------------------------------------------------------------------+               |
|                                                                                     |
+=====================================================================================+
```

---

## 6. Gap Analysis

```
+=====================================================================================+
|  COMPREHENSIVE GAP ANALYSIS                                                         |
+=====================================================================================+
|                                                                                     |
|  +-----+----------------------------+----------+----------+----------+-------------+|
|  | Gap | Description                | Source   | Severity | Effort   | Priority    ||
|  +-----+----------------------------+----------+----------+----------+-------------+|
|  | G1  | No per-task model routing   | THREAD   | HIGH     | LOW      | P-MODEL     ||
|  |     | Single provider per workflow| Slate    |          |          |             ||
|  +-----+----------------------------+----------+----------+----------+-------------+|
|  | G2  | No context compression      | Context  | HIGH     | MEDIUM   | P-EPISODE   ||
|  |     | Full output carried forward | Folding  |          |          | + P-CONTEXT ||
|  |     |                             | Slate    |          |          |             ||
|  +-----+----------------------------+----------+----------+----------+-------------+|
|  | G3  | No episodic memory          | Slate    | HIGH     | HIGH     | P-MEMORY    ||
|  |     | In-memory session only      | CrewAI   |          |          |             ||
|  |     |                             | Memory-R1|          |          |             ||
|  +-----+----------------------------+----------+----------+----------+-------------+|
|  | G4  | No strategy/tactics pattern | THREAD   | HIGH     | HIGH     | P-STRATEGY  ||
|  |     | No adaptive decomposition   | Slate    |          |          |             ||
|  +-----+----------------------------+----------+----------+----------+-------------+|
|  | G5  | No dynamic DAG generation   | RLM      | MEDIUM   | HIGH     | P-STRATEGY  ||
|  |     | Static YAML only            | Slate    |          |          |             ||
|  +-----+----------------------------+----------+----------+----------+-------------+|
|  | G6  | No confidence-based escal.  | THREAD   | MEDIUM   | LOW      | P-EPISODE   ||
|  |     | (absorbed into episodes)    | Slate    |          |          | (confidence)||
|  +-----+----------------------------+----------+----------+----------+-------------+|
|  | G7  | No runtime introspection    | Audit    | LOW      | LOW      | P-INTROSPECT||
|  |     | Agents can't see DAG state  | RLM      |          |          |             ||
|  +-----+----------------------------+----------+----------+----------+-------------+|
|  | G8  | No code execution sandbox   | CodeAct  | LOW      | HIGH     | Future      ||
|  |     | exec: is shell-only         |          |          |          |             ||
|  +-----+----------------------------+----------+----------+----------+-------------+|
|  | G9  | No inter-agent protocol     | A2A      | LOW      | HIGH     | Future      ||
|  |     | Parent-child only (spawn)   | Swarms   |          |          |             ||
|  +-----+----------------------------+----------+----------+----------+-------------+|
|                                                                                     |
|  ARCHITECTURAL DEBT (found in audit):                                               |
|  +------------------------------------------------------------------+               |
|  | D1  Two binding systems coexist (use: + with:)                    |               |
|  | D2  DataStore has no eviction (unbounded memory growth)           |               |
|  | D3  Mixed locking (DashMap + RwLock in DataStore)                 |               |
|  | D4  Context file loading has no size limits (OOM risk)            |               |
|  | D5  Env var pollution from boot-time secret injection             |               |
|  | D6  Limited JSONPath in binding resolution                        |               |
|  +------------------------------------------------------------------+               |
|                                                                                     |
+=====================================================================================+
```

---

## 7. Synergy Map

```
+=====================================================================================+
|  NIKA x NOVANET SYNERGY OPPORTUNITIES                                               |
+=====================================================================================+
|                                                                                     |
|  BOUNDARY RULES:                                                                    |
|  +------------------------------------------------------------------+               |
|  | NIKA OWNS:                 NOVANET OWNS:                          |               |
|  | - Workflow execution       - Entity/content storage               |               |
|  | - LLM orchestration        - Locale intelligence                  |               |
|  | - DAG construction         - Graph context assembly               |               |
|  | - Agent loop/spawning      - Schema validation                    |               |
|  | - File-based context       - Content quality audit                |               |
|  | - Event sourcing           - Search/discovery                     |               |
|  | - TUI/developer UX         - Data lineage                        |               |
|  | - Transform engine         - Cross-session state                  |               |
|  | - Security model           - Denomination forms                   |               |
|  +------------------------------------------------------------------+               |
|                                                                                     |
|  SYNERGY 1: Episodic Memory (P-MEMORY + P-EPISODE)                                  |
|  +------------------------------------------------------------------+               |
|  |                                                                   |               |
|  |  Nika agent completes task                                        |               |
|  |       |                                                           |               |
|  |       +---> Compress trace into episode summary                   |               |
|  |       +---> novanet_write(class: AgentEpisode, ...)               |               |
|  |       +---> novanet_write(arc: EPISODE_OF, to: entity_key)        |               |
|  |                                                                   |               |
|  |  Nika agent starts NEW task                                       |               |
|  |       |                                                           |               |
|  |       +---> novanet_search(query: task_prompt, kinds: [Episode])  |               |
|  |       +---> Inject relevant episodes as context                   |               |
|  |       +---> Agent benefits from past experience                   |               |
|  |                                                                   |               |
|  +------------------------------------------------------------------+               |
|                                                                                     |
|  SYNERGY 2: Generation Lineage                                                      |
|  +------------------------------------------------------------------+               |
|  |                                                                   |               |
|  |  novanet_context(page, fr-FR)  --->  Nika infer: generates        |               |
|  |                                            |                      |               |
|  |                                            v                      |               |
|  |                                      novanet_write(PageNative)    |               |
|  |                                      provenance: generated_by=nika|               |
|  |                                      workflow: "generate-page"     |               |
|  |                                                                   |               |
|  +------------------------------------------------------------------+               |
|                                                                                     |
|  SYNERGY 3: Smart Model Routing via NovaNet                                         |
|  +------------------------------------------------------------------+               |
|  |                                                                   |               |
|  |  Store model performance data in NovaNet                          |               |
|  |  (new NodeClass: ModelBenchmark)                                  |               |
|  |                                                                   |               |
|  |  At routing time:                                                 |               |
|  |    novanet_search("best model for translation fr-FR")             |               |
|  |    --> returns model recommendation                               |               |
|  |    --> Nika uses that model for the task                          |               |
|  |                                                                   |               |
|  +------------------------------------------------------------------+               |
|                                                                                     |
|  SYNERGY 4: Decompose via Graph Structure                                           |
|  +------------------------------------------------------------------+               |
|  |                                                                   |               |
|  |  decompose:                                                       |               |
|  |    strategy: semantic                                             |               |
|  |    traverse: HAS_CHILD                                            |               |
|  |    source: $entity                                                |               |
|  |                                                                   |               |
|  |  NovaNet graph structure DRIVES Nika DAG expansion                |               |
|  |  (already partially implemented)                                  |               |
|  |                                                                   |               |
|  +------------------------------------------------------------------+               |
|                                                                                     |
|  DUPLICATION RISKS TO AVOID:                                                        |
|  +------------------------------------------------------------------+               |
|  | NEVER build parallel memory in Nika (use NovaNet)                 |               |
|  | NEVER add entity awareness to Nika (use novanet_context)          |               |
|  | NEVER hardcode graph schema in Nika (use novanet_introspect)      |               |
|  | NEVER build quality scoring in Nika (use novanet_audit)           |               |
|  +------------------------------------------------------------------+               |
|                                                                                     |
+=====================================================================================+
```

---

## 8. Evolution Priorities

> **Full details in doc 05 (Evolution Roadmap) and doc 07 (Slate Deep Integration Strategy).**

```
+=====================================================================================+
|  THE 6 PRIORITIES IN 3 WAVES (Slate Integration Architecture)                       |
+=====================================================================================+
|                                                                                     |
|  CORE INSIGHT: Nika's DAG IS already Slate's kernel. Tasks ARE processes.           |
|  TaskResult IS return values. EventLog IS process accounting. We add                |
|  4 KERNEL UPGRADES: model slots, episodes, strategy mode, context budgeting.        |
|  Then extend BEYOND Slate via NovaNet episodic memory + runtime introspection.      |
|                                                                                     |
|  OLD P3 (ConfidenceRouter) → ABSORBED into P-EPISODE (confidence is an              |
|  episode property; strategy LLM handles escalation naturally)                       |
|                                                                                     |
|  WAVE 1: Thread Foundation (v0.28.0, schema @0.12)                                  |
|  +=================================================================+                |
|  |                                                                  |                |
|  |  P-MODEL: 4-SLOT MODEL ARCHITECTURE                             |                |
|  |  +---------------------------------------------------------+    |                |
|  |  | Per-workflow model_slots: block (4 named slots)          |    |                |
|  |  | Per-task model_slot: reference (route to slot)           |    |                |
|  |  | Slots: main / tactical / search / reasoning              |    |                |
|  |  |                                                          |    |                |
|  |  | EXAMPLE:                                                  |    |                |
|  |  |   model_slots:                                           |    |                |
|  |  |     reasoning:                                           |    |                |
|  |  |       provider: anthropic                                |    |                |
|  |  |       model: claude-sonnet-4-6                            |    |                |
|  |  |       extended_thinking: true                            |    |                |
|  |  |     tactical:                                            |    |                |
|  |  |       provider: groq                                     |    |                |
|  |  |       model: llama-3.3-70b-versatile                     |    |                |
|  |  |   tasks:                                                  |    |                |
|  |  |     - id: plan                                            |    |                |
|  |  |       model_slot: reasoning       # expensive, deep      |    |                |
|  |  |     - id: execute                                         |    |                |
|  |  |       model_slot: tactical        # cheap, fast          |    |                |
|  |  |                                                          |    |                |
|  |  | Why per-workflow, not global: Different workflows have    |    |                |
|  |  | different cost/quality tradeoffs. Slate's global config   |    |                |
|  |  | can't express this.                                      |    |                |
|  |  |                                                          |    |                |
|  |  | Source: Slate (4 model slots) + THREAD (resource-aware)   |    |                |
|  |  | New files: ast/raw/model_slot.rs                          |    |                |
|  |  | Modified: ast/raw/workflow.rs, task.rs, provider/rig.rs   |    |                |
|  |  +---------------------------------------------------------+    |                |
|  |                                                                  |                |
|  |  P-EPISODE: EPISODE ENGINE                                      |                |
|  |  +---------------------------------------------------------+    |                |
|  |  | Compressed task result at natural completion boundary     |    |                |
|  |  | LLM-based compression (NOT lossy compaction)             |    |                |
|  |  | Episode struct: summary + key_findings + confidence      |    |                |
|  |  | Downstream tasks receive EPISODES, not raw output        |    |                |
|  |  |                                                          |    |                |
|  |  | COMPRESSION FLOW:                                        |    |                |
|  |  |   Task executes → Full output → LLM compresses →        |    |                |
|  |  |   Episode stored in DataStore                            |    |                |
|  |  |   ├── Summary (always kept)                              |    |                |
|  |  |   ├── Key findings (configurable via retain:)            |    |                |
|  |  |   ├── Confidence score (self-assessed)                   |    |                |
|  |  |   └── Raw output (debug mode only)                       |    |                |
|  |  |                                                          |    |                |
|  |  | HOW CONFIDENCE REPLACES OLD P3:                          |    |                |
|  |  |   Old: Task → Tier1 → confidence < threshold → Tier2    |    |                |
|  |  |   New: Task → Episode(confidence) → Strategy LLM SEES   |    |                |
|  |  |        low confidence → DECIDES: retry? more context?    |    |                |
|  |  |   Adaptive. Full context. No rigid router needed.        |    |                |
|  |  |                                                          |    |                |
|  |  | Source: Slate (episodes) + Context-Folding + Memory-R1   |    |                |
|  |  | New files: runtime/episode.rs, episode_compress.rs       |    |                |
|  |  | Modified: executor.rs, store/mod.rs, binding/resolve.rs  |    |                |
|  |  +---------------------------------------------------------+    |                |
|  |                                                                  |                |
|  +=================================================================+                |
|                                                                                     |
|  WAVE 2: Strategy Intelligence (v0.29.0, schema @0.13)                              |
|  +=================================================================+                |
|  |                                                                  |                |
|  |  P-STRATEGY: STRATEGY ORCHESTRATION                             |                |
|  |  +---------------------------------------------------------+    |                |
|  |  | New orchestration: strategy mode in workflow YAML         |    |                |
|  |  | Strategy LLM dispatches tactic tasks dynamically          |    |                |
|  |  | Tasks become tactic TEMPLATES (not fixed DAG)             |    |                |
|  |  | Dynamic DAG mutation at runtime                           |    |                |
|  |  |                                                          |    |                |
|  |  | ORCHESTRATION LOOP:                                      |    |                |
|  |  |   Round 1: Strategy → "dispatch: research(topic)"        |    |                |
|  |  |   Round 2: Strategy ← [research_ep]                      |    |                |
|  |  |            → "dispatch: write(hero), write(features)"    |    |                |
|  |  |   Round 3: Strategy ← [hero_ep, features_ep]             |    |                |
|  |  |            → "dispatch: review(draft)"                   |    |                |
|  |  |   Round N: Strategy → "DONE. Final output."              |    |                |
|  |  |                                                          |    |                |
|  |  | This IS Slate's thread weaving made declarative.          |    |                |
|  |  |                                                          |    |                |
|  |  | Source: Slate (thread weaving) + THREAD + RLM             |    |                |
|  |  | New files: runtime/strategy.rs, runtime/tactic.rs,        |    |                |
|  |  |   dag/dynamic.rs                                         |    |                |
|  |  | Modified: runner.rs, ast/raw/workflow.rs                  |    |                |
|  |  +---------------------------------------------------------+    |                |
|  |                                                                  |                |
|  |  P-CONTEXT: CONTEXT BUDGET MANAGEMENT                          |                |
|  |  +---------------------------------------------------------+    |                |
|  |  | Working memory awareness at runtime level                 |    |                |
|  |  | context_budget: per task in YAML (max tokens)             |    |                |
|  |  | Episode-only passing (never raw history from other tasks) |    |                |
|  |  | Strategy decides which episodes each thread receives      |    |                |
|  |  |                                                          |    |                |
|  |  | RULES:                                                   |    |                |
|  |  | 1. Each task receives: prompt + relevant episodes + ctx   |    |                |
|  |  | 2. NEVER raw history from other tasks                     |    |                |
|  |  | 3. Budget enforced by runtime (truncate/warn if exceeded) |    |                |
|  |  | 4. Token budget tracked in events for observability       |    |                |
|  |  |                                                          |    |                |
|  |  | Source: Slate (dumb zone) + Context-Folding               |    |                |
|  |  | New files: runtime/context_budget.rs                      |    |                |
|  |  | Modified: executor.rs, event/log.rs                       |    |                |
|  |  +---------------------------------------------------------+    |                |
|  |                                                                  |                |
|  +=================================================================+                |
|                                                                                     |
|  WAVE 3: Persistent Memory (v0.30.0)                                                |
|  +=================================================================+                |
|  |                                                                  |                |
|  |  P-MEMORY: NOVANET EPISODIC MEMORY                              |                |
|  |  +---------------------------------------------------------+    |                |
|  |  | Episodes persisted in NovaNet, linked to semantic entities|    |                |
|  |  | Cross-session learning + knowledge overhang activation    |    |                |
|  |  |                                                          |    |                |
|  |  | LIFECYCLE:                                                |    |                |
|  |  |   Session 1: research(qr-code) → Episode                 |    |                |
|  |  |     → novanet_write(AgentEpisode) + EPISODE_OF(Entity)   |    |                |
|  |  |                                                          |    |                |
|  |  |   Session 2: generate(qr-code)                           |    |                |
|  |  |     → novanet_search(AgentEpisode, entity=qr-code)       |    |                |
|  |  |     → Previous episodes surface as context               |    |                |
|  |  |     → Knowledge overhang ACTIVATED                       |    |                |
|  |  |                                                          |    |                |
|  |  | This GOES BEYOND Slate (session files) by using a         |    |                |
|  |  | graph-queryable, entity-linked, cross-session memory.     |    |                |
|  |  |                                                          |    |                |
|  |  | Source: Slate (episodic memory) + Memory-R1 + CrewAI      |    |                |
|  |  | NovaNet: new AgentEpisode NodeClass + 3 ArcClasses        |    |                |
|  |  | New files: runtime/episodic_memory.rs                     |    |                |
|  |  +---------------------------------------------------------+    |                |
|  |                                                                  |                |
|  |  P-INTROSPECT: RUNTIME INTROSPECTION TOOLS                     |                |
|  |  +---------------------------------------------------------+    |                |
|  |  | 6 new builtin tools for agent self-awareness              |    |                |
|  |  | nika:episodes      → list accumulated episodes            |    |                |
|  |  | nika:threads       → active/completed threads             |    |                |
|  |  | nika:strategy_state → current round, budget               |    |                |
|  |  | nika:cost          → token usage and cost report           |    |                |
|  |  | nika:dag_info      → predecessors, critical path           |    |                |
|  |  | nika:task_status   → status of specific tasks              |    |                |
|  |  |                                                          |    |                |
|  |  | Source: RLM (self-referential) + Slate (DAG awareness)    |    |                |
|  |  +---------------------------------------------------------+    |                |
|  |                                                                  |                |
|  +=================================================================+                |
|                                                                                     |
|  TOTALS:                                                                            |
|  New files: 8                                                                       |
|  Modified files: 11                                                                 |
|  Schema bumps: @0.12 (Wave 1), @0.13 (Wave 2)                                      |
|  Cross-project: NovaNet schema changes (Wave 3 only)                                |
|                                                                                     |
+=====================================================================================+
```

### Why This Order

```
+=====================================================================================+
|  DEPENDENCY GRAPH — Why This Sequence                                               |
+=====================================================================================+
|                                                                                     |
|   P-MODEL (4-slot)          P-EPISODE (compression)                                 |
|        |                         |                                                  |
|        |  (strategy needs        |  (strategy needs episodes for                    |
|        |   model slots to        |   inter-round communication)                     |
|        |   route tactics)        |                                                  |
|        |                         |                                                  |
|        +----------+--------------+                                                  |
|                   |                                                                  |
|                   v                                                                  |
|        P-STRATEGY (orchestration)                                                   |
|        P-CONTEXT (budget mgmt)     <-- context budgeting makes                      |
|                   |                     strategy mode practical                      |
|                   |                                                                  |
|                   v                                                                  |
|        P-MEMORY (NovaNet persistence)                                               |
|        P-INTROSPECT (runtime tools) <-- simple once episodes,                       |
|                                         strategy, cost tracked                      |
|                                                                                     |
|  P-MODEL first:     Low effort, high value, prerequisite for everything else.       |
|  P-EPISODE with it:  Core primitive — everything depends on compressed results.     |
|  P-STRATEGY after:   REQUIRES both model slots (routing) and episodes (comms).      |
|  P-CONTEXT with it:  Without budgets, strategy rounds accumulate unbounded context. |
|  P-MEMORY last:      Cross-project NovaNet schema changes. Needs stable episodes.   |
|  P-INTROSPECT last:  Simple once runtime state is already tracked.                  |
|                                                                                     |
+=====================================================================================+
```

### After All Priorities

```
+=====================================================================================+
|  COMPETITIVE POSITION — Before vs After                                             |
+=====================================================================================+
|                                                                                     |
|  +-------------------------------+-----------+-----------+----------+               |
|  | Capability                    | Before    | After     | vs Slate |               |
|  +-------------------------------+-----------+-----------+----------+               |
|  | 4-slot model routing           | No        | Yes       | Parity   |               |
|  | Episode compression            | No        | Yes       | Parity   |               |
|  | Strategy/tactics               | No        | Yes       | Parity   |               |
|  | Context budgeting              | No        | Yes       | Parity   |               |
|  | Cross-session memory           | No        | Yes       | BEYOND   |               |
|  |   (NovaNet entity-linked)     |           |           | (Slate   |               |
|  |                               |           |           |  = files)|               |
|  | Runtime introspection          | No        | Yes       | NIKA+    |               |
|  +-------------------------------+-----------+-----------+----------+               |
|  | Knowledge graph (NovaNet)      | Yes       | Yes       | NIKA+    |               |
|  | YAML-first workflows           | Yes       | Yes       | NIKA+    |               |
|  | 200+ locales                   | Yes       | Yes       | NIKA+    |               |
|  | Structured output (4-layer)    | Yes       | Yes       | NIKA+    |               |
|  | Event sourcing (34+ events)    | Yes       | Yes+      | NIKA+    |               |
|  | 7 LLM providers + native       | Yes       | Yes       | NIKA+    |               |
|  | Rust performance               | Yes       | Yes       | NIKA+    |               |
|  | Security (exec hardening)      | Yes       | Yes       | NIKA+    |               |
|  | Reproducibility (NDJSON)       | Yes       | Yes       | NIKA+    |               |
|  +-------------------------------+-----------+-----------+----------+               |
|                                                                                     |
|  RESULT: Parity on ALL 6 of Slate's advantages.                                    |
|          BEYOND Slate on episodic memory (NovaNet > session files).                 |
|          9 unique strengths Slate cannot replicate (moat).                           |
|                                                                                     |
+=====================================================================================+
```

---

## Research Methodology

```
+=====================================================================================+
|  HOW THIS RESEARCH WAS CONDUCTED                                                    |
+=====================================================================================+
|                                                                                     |
|  13 RESEARCH AGENTS DEPLOYED:                                                       |
|  +------------------------------------------------------------------+               |
|  | 1.  Deep-dive Nika architecture (all modules)                     |               |
|  | 2.  Research RLM, CodeAct, THREAD papers                          |               |
|  | 3.  Research Slate (Random Labs) agent architecture                |               |
|  | 4.  Research competing runtimes (LangGraph, CrewAI, etc.)         |               |
|  | 5.  Analyze Nika-NovaNet ecosystem boundaries                     |               |
|  | 6.  Research agent orchestration patterns                          |               |
|  | 7.  Audit ALL Nika features (exhaustive inventory)                |               |
|  | 8.  NovaNet MCP tools inventory                                   |               |
|  | 9.  Research agent memory architectures                            |               |
|  | 10. Research model routing in production                           |               |
|  | 11. Deep audit Nika runtime internals                              |               |
|  | 12. Deep audit Nika AST + DAG internals                            |               |
|  | 13. Research context compression techniques                        |               |
|  +------------------------------------------------------------------+               |
|                                                                                     |
|  SOURCE MATERIAL:                                                                   |
|  +------------------------------------------------------------------+               |
|  | Papers:   6 (RLM, CodeAct, THREAD, Context-Folding, Swarms, R1)  |               |
|  | Products: 5 (Slate, Claude Code, Codex, LangGraph, CrewAI)        |               |
|  | Protocols: 3 (MCP, A2A, ACP)                                      |               |
|  | Codebase: 371 files, 219K lines (every module audited)            |               |
|  | Brainstorm docs: 7 (01-features through 07-slate-integration)     |               |
|  +------------------------------------------------------------------+               |
|                                                                                     |
+=====================================================================================+
```
