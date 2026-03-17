# Research: Naming Conventions for In-Memory Runtime State Containers

**Date**: 2026-03-14
**Context**: Choosing a name for the DashMap-based RAM store that holds task results, context, and inputs during a single workflow execution run in Nika.
**Current name**: `DataStore` (in `src/store/datastore.rs`)

---

## 1. Workflow Engine Survey

### Comparison Table

| Framework | Language | Name | What It Stores | Lifetime | Concurrency Model |
|-----------|----------|------|----------------|----------|-------------------|
| **Temporal** | Rust SDK | `WorkflowContext` | Workflow state, timers, signals, queries | Per workflow execution | Event-sourced replay |
| **Temporal** | Rust core | `ManagedRun` / `WorkflowManager` | Activation state, command buffers | Per workflow task | Managed by worker |
| **Airflow** | Python | `XCom` ("cross-communication") | Task outputs for inter-task data passing | Per DAG run (DB-persisted) | DB-backed, not in-memory |
| **Airflow** | Python | `context` dict | Task instance metadata, execution date, params | Per task execution | Thread-local dict |
| **Prefect** | Python | `ResultStore` | Task/flow return values | Per flow run (persisted) | Serialized to storage |
| **Prefect** | Python | `FlowRunContext` / `TaskRunContext` | Runtime metadata, result factory, client | Per run | Context var (asyncio) |
| **Dagster** | Python | `StepExecutionContext` / `PlanExecutionContext` | I/O managers, resources, step outputs | Per step/plan execution | Thread-safe via resources |
| **n8n** | TypeScript | `IRunExecutionData` | `resultData.runData` (node outputs), `executionData.contextData` | Per workflow execution | Single-threaded (Node.js) |
| **Argo Workflows** | Go | `NodeStatus` (in WorkflowStatus) | Node outputs, artifacts, phase | Per workflow (K8s CRD) | K8s etcd-persisted |
| **Windmill** | Rust | `FlowStatus` + `FlowContext` | Job results, flow inputs, module status | Per flow run (DB-persisted) | PostgreSQL-backed |
| **LangGraph** | Python | `StateGraph` + `Channel` system | Graph state via typed channels (`LastValue`, `BinaryOp`) | Per graph invocation | Channel-based (Pregel model) |

### Key Observations from Workflow Engines

1. **"Context" is the dominant pattern** for "bag of data available during execution":
   - Temporal: `WorkflowContext`
   - Dagster: `StepExecutionContext`, `PlanExecutionContext`
   - Prefect: `FlowRunContext`, `TaskRunContext`
   - Airflow: `context` dict
   - Windmill: `FlowContext`

2. **"Store" appears when persistence is involved**:
   - Prefect: `ResultStore` (manages storage + retrieval to disk/cloud)
   - Dagster: `DynamicPartitionsStore` (persistent)
   - Semantic Kernel: `MemoryStore`, `VectorStore` (DB-backed)

3. **"State" appears when describing the shape of data, not the container**:
   - LangGraph: `State` is a `TypedDict` schema, not a container class
   - AutoGen: `AssistantAgentState`, `TeamState` (serializable snapshots)
   - n8n: `IRunExecutionData` (data structure, not a class with methods)

4. **"RunData" / "RunExecutionData"** is n8n's name for the full execution payload including all node outputs -- closest to what Nika's `DataStore` does.

---

## 2. Agent Framework Survey

| Framework | Language | Name | What It Stores | Lifetime |
|-----------|----------|------|----------------|----------|
| **LangGraph** | Python | `State` (TypedDict) | User-defined graph state, messages | Per graph invocation |
| **LangGraph** | Python | `BaseChannel` | Per-key state with reducer logic | Per superstep |
| **CrewAI** | Python | `ShortTermMemory` / `LongTermMemory` / `EntityMemory` | RAG embeddings, conversation history | Short-term: per task; Long-term: persistent |
| **AutoGen** | Python | `ChatCompletionContext` / `BufferedChatCompletionContext` | Message history for agents | Per agent conversation |
| **AutoGen** | Python | `BaseState` / `TeamState` | Serializable agent/team state | Per save/load cycle |
| **Semantic Kernel** | Python | `RunContext` | In-process runtime execution context | Per agent run |
| **Semantic Kernel** | Python | `MemoryStore` (deprecated) / `VectorStore` | Embeddings, semantic memory | Persistent (DB-backed) |

### Key Observations from Agent Frameworks

1. **"Memory" implies semantic/LLM memory** (embeddings, conversation history, RAG). Using `WorkingMemory` for a task result map would be misleading in the AI/agent space.

2. **"State" in agent frameworks means serializable snapshots**, not live concurrent containers.

3. **`RunContext`** appears in Semantic Kernel for exactly "the context of a single execution run" -- close to what Nika needs.

---

## 3. Rust Ecosystem Patterns

| Library | Name | What It Stores | Pattern |
|---------|------|----------------|---------|
| **tokio** | `Context` (scheduler) | Current task, runtime handle | Per-worker thread-local |
| **axum** | `State<S>` | User-provided app state | Per-application (shared via `Arc`) |
| **bevy** | `World` | All ECS entities, components, resources | Per-app (the universal container) |
| **bevy** | `ExecutorState` | System scheduling state | Per-schedule execution |
| **Temporal Rust SDK** | `WorkflowContext<W>` | Workflow handle, state access | Per workflow execution |
| **Windmill** | `FlowContext` | Flow inputs, flow status | Per flow execution |
| **Vector** | `TaskCoordinator<State>` | Task lifecycle state | Per validation run |

### Rust Naming Conventions

1. **`Context`** in Rust typically means "data + methods needed to do work in a scope". Tokio, axum, Temporal, and Windmill all use this pattern.

2. **`State`** in Rust typically means either:
   - A generic type parameter (`State<S>` in axum) -- user-defined
   - A phase/lifecycle marker (`ExecutorState` in bevy)
   - Raw data without behavior

3. **`Store`** in Rust is less common for in-memory containers. When it appears, it usually implies persistence or indexed lookup (key-value store semantics).

4. **`World`** (bevy) is the "contains everything" pattern -- too broad for a scoped container.

---

## 4. Analysis of Each Candidate

### `DataStore` (current name)

| Aspect | Assessment |
|--------|------------|
| **Accuracy** | Partially accurate. It stores data, but "store" implies persistence (databases, filesystems). |
| **Confusion risk** | **High**. "DataStore" in most ecosystems means a persistent storage layer (Google Cloud Datastore, Dagster stores, Prefect ResultStore). A reader might expect disk I/O or database access. |
| **Precedent** | Unusual for in-memory ephemeral containers. Most "DataStore" classes wrap databases. |
| **Verdict** | **Rename recommended**. The name suggests durability that does not exist. |

### `RunState`

| Aspect | Assessment |
|--------|------------|
| **Accuracy** | Good. "Run" scopes it to a single execution. "State" indicates mutable data. |
| **Confusion risk** | **Medium**. "RunState" in many frameworks means the lifecycle phase of a run (Pending/Running/Completed/Failed). Prefect has `FlowRunState`, Argo has `NodeStatus.Phase`. Could be confused with an enum, not a container. |
| **Precedent** | Common as an enum (run lifecycle). Less common as a container struct. |
| **Verdict** | **Ambiguous**. The name reads like "what state is the run in?" not "the state held by the run". |

### `RunContext`

| Aspect | Assessment |
|--------|------------|
| **Accuracy** | Strong. "Run" scopes it. "Context" means "data available during execution". |
| **Confusion risk** | **Low**. Aligns with Temporal `WorkflowContext`, Dagster `StepExecutionContext`, Windmill `FlowContext`, Semantic Kernel `RunContext`. |
| **Precedent** | **Very strong**. This is the dominant pattern across workflow engines. Semantic Kernel uses `RunContext` verbatim. |
| **Downside** | Nika already has `BootContext` and `LoadedContext`. "Context" is slightly overloaded. However, the `Run` prefix disambiguates clearly. |
| **Verdict** | **Strong candidate**. Industry-standard pattern. |

### `ExecutionState`

| Aspect | Assessment |
|--------|------------|
| **Accuracy** | Good. "Execution" is precise. "State" indicates data. |
| **Confusion risk** | **Medium**. Could be confused with the state *of* the execution (running/paused/done) rather than the state *in* the execution. Dagster uses `PlanExecutionContext` to avoid this ambiguity. |
| **Precedent** | Bevy uses `ExecutorState` for scheduling. n8n uses `executionData`. Not a common compound name. |
| **Verdict** | **Decent but verbose**. "Execution" is 9 chars vs "Run" at 3. |

### `WorkingMemory`

| Aspect | Assessment |
|--------|------------|
| **Accuracy** | Conceptually appealing (cognitive science: short-term working memory). |
| **Confusion risk** | **High in the AI/agent space**. CrewAI, Semantic Kernel, and AutoGen all use "Memory" to mean LLM conversation/embedding memory. In Nika's domain (AI workflows + agents), this would be deeply confusing. |
| **Precedent** | Used in cognitive architectures (ACT-R, SOAR). Not used in workflow engines. |
| **Verdict** | **Reject**. Wrong semantic domain for an AI workflow engine. |

### `TaskStore`

| Aspect | Assessment |
|--------|------------|
| **Accuracy** | Partially accurate -- it stores task results, but also stores context and inputs. |
| **Confusion risk** | **Medium**. "TaskStore" suggests it only stores tasks (definitions), not results. Also, "store" still implies persistence. |
| **Precedent** | Not found in surveyed frameworks. |
| **Verdict** | **Too narrow**. The container holds more than just task data (context, inputs). |

### `RuntimeStore`

| Aspect | Assessment |
|--------|------------|
| **Accuracy** | Good. "Runtime" scopes it to execution time. "Store" indicates data container. |
| **Confusion risk** | **Medium**. "Runtime" is broad -- could be confused with the tokio runtime, the Nika `runtime/` module, or the `NativeRuntime`. "Store" still implies persistence. |
| **Precedent** | Not found in surveyed frameworks. |
| **Verdict** | **Ambiguous scope**. "Runtime" does not clearly mean "one workflow run". |

---

## 5. Recommendation Matrix

```
                    Accuracy  No-Confusion  Precedent  Rust-Idiomatic  Scope-Clarity
RunContext            +++        +++           +++          +++             +++
ExecutionState        ++         ++            +            ++              ++
RunState              ++         +             +            ++              +
RuntimeStore          ++         +             -            +               -
DataStore (current)   +          -             -            +               +
TaskStore             +          +             -            +               -
WorkingMemory         +          --            -            -               --
```

### Verdict: **`RunContext`**

**Why:**

1. **Industry standard**: Temporal (`WorkflowContext`), Dagster (`StepExecutionContext`), Prefect (`FlowRunContext`), Windmill (`FlowContext`), Semantic Kernel (`RunContext`) all follow the `*Context` pattern for "data available during one execution".

2. **Precise scoping**: The `Run` prefix makes it clear this is per-execution, not per-application or per-process.

3. **Rust idiomatic**: Tokio, axum, and the Temporal Rust SDK all use `Context` for "the bag of state you need to do work in a scope".

4. **No confusion with persistence**: Unlike `*Store`, `*Context` does not imply database or filesystem backing.

5. **No confusion with lifecycle enum**: Unlike `RunState`, `RunContext` clearly means "the context of a run", not "which state is the run in".

6. **No confusion with LLM memory**: Unlike `WorkingMemory`, `RunContext` has no AI/cognitive science baggage.

7. **Matches Nika's module naming**: Nika already has `BootContext` and `LoadedContext`. A `RunContext` fits the family.

### Alternative: If `RunContext` feels too close to `BootContext`

Consider **`RunStore`** -- drops the persistence connotation slightly because `Run` anchors the meaning to "ephemeral execution scope". But `RunContext` is still the stronger choice given industry precedent.

### Migration Note

Current `DataStore` has 205 references across the codebase. A rename would be mechanical (find-replace) but not trivial. The field name on `Runner` struct could go from `datastore: DataStore` to `ctx: RunContext` or `run_ctx: RunContext`.

---

## Sources

| Source | What It Provided |
|--------|------------------|
| Temporal SDK Core (Rust) | `WorkflowContext`, `ManagedRun`, `WorkflowStateInfo` naming |
| Airflow models | `XCom` for inter-task data, `context` dict pattern |
| Prefect source | `ResultStore` (persistent), `FlowRunContext` (ephemeral) |
| Dagster source | `StepExecutionContext`, `PlanExecutionContext`, `PlanOrchestrationContext` |
| n8n workflow package | `IRunExecutionData`, `IRunData`, `contextData` |
| Argo Workflows (Go) | `DagContext` interface, `NodeStatus` for outputs |
| Windmill (Rust) | `FlowContext`, `FlowStatus` |
| LangGraph | `StateGraph`, `BaseChannel`, `State` TypedDict pattern |
| AutoGen | `BaseState`, `TeamState`, `ChatCompletionContext` |
| Semantic Kernel | `RunContext`, `MemoryStore` (deprecated), `VectorStore` |
| tokio | `Context` for per-worker state |
| axum | `State<S>` for per-application shared state |
| bevy ECS | `World`, `ExecutorState`, `SystemState` |
| Nika codebase | `DataStore` (205 references), `BootContext`, `LoadedContext` |

## Methodology

- **Tools used**: Git sparse-checkout + grep across 14 framework repositories
- **Repositories analyzed**: 14 (Temporal, Airflow, Prefect, Dagster, n8n, Argo, Windmill, LangGraph, AutoGen, Semantic Kernel, tokio, axum, bevy, Vector)
- **Language coverage**: Rust, Python, TypeScript, Go
- **Focus**: Struct/class naming for in-memory state containers scoped to a single execution

## Confidence Level

**High** -- Based on direct source code analysis of 14 major frameworks across 4 languages, with consistent patterns emerging across the workflow engine and Rust ecosystems.
