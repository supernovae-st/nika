# Competing AI Workflow Engine Architectures

> Research date: 2026-04-05
> Purpose: Architectural inspiration for Nika (Rust, YAML, 5 verbs, 14 providers, DAG, structured output, streaming)
> Sources: Perplexity searches, GitHub repos, documentation sites
> Confidence: High (8 systems analyzed, cross-referenced)

---

## Executive Summary

Eight competing AI workflow/orchestration systems were analyzed for architectural patterns relevant to Nika. The key finding: **Nika's architecture is already ahead on several fronts** (Rust performance, 5-verb semantic model, CAS media pipeline, multi-layer structured output), but there are specific patterns worth studying for the launch polish phase.

**Top 3 patterns to consider adopting:**

1. **Dagster's asset-based model** -- thinking of workflow outputs as first-class data assets with lineage, not just task completions. Nika's artifact system is already close; the mental model shift is the value.
2. **Instructor's Mode enum for structured output** -- Nika's 5-layer defense is more robust, but Instructor's explicit mode selection per provider capability is a clean pattern for debugging/overrides.
3. **LangGraph's checkpoint-resume via state channels** -- Nika has traces, but explicit checkpoint-based resume for long-running workflows (especially agents) is the next frontier.

---

## 1. LangGraph (by LangChain)

**Repo**: `langchain-ai/langgraph` | **Language**: Python | **Stars**: ~10k

### Architecture

```
StateGraph (builder)
    |
    v
CompiledStateGraph (executable)
    |
    v
Pregel engine (execution loop)
    |
    v
Channels (state management)
```

### Provider Dispatch

LangGraph itself does NOT handle provider dispatch. It delegates entirely to LangChain's `ChatModel` abstraction. Each node in the graph receives state and can use any LangChain model. This is a **delegation pattern** -- the graph engine is model-agnostic by design.

**Nika comparison**: Nika integrates provider dispatch INTO the engine (via rig-core traits + provider enum). This is architecturally tighter -- one binary, no dependency chain. LangGraph requires LangChain + provider SDKs + langchain-community.

### DAG Execution: The Pregel Engine

Named after Google's Pregel paper for graph-parallel computation. Key concepts:

- **Channels**: Typed state slots. Each key in the TypedDict state schema is a channel. Channels have reducers (how to merge updates).
- **Supersteps**: Execution proceeds in supersteps. All nodes scheduled in a superstep run (potentially in parallel), then their channel updates are merged.
- **Conditional edges**: Functions that examine state and return the next node name. This enables dynamic routing without hardcoded DAG structure.

```python
# Reducer pattern -- how channel updates merge
class State(TypedDict):
    messages: Annotated[list, add_messages]  # reducer = append
    count: Annotated[int, operator.add]       # reducer = sum
```

**Pattern worth studying**: The **reducer concept for state channels**. In Nika, task outputs are stored as `Value` in the binding table. If two tasks write to the same downstream binding, last-write-wins. LangGraph's reducer pattern (append, merge, replace) is more explicit. However, Nika's `with:` binding system with explicit `$task_id` references avoids this ambiguity by design -- you never have two tasks writing to the same channel.

### Streaming

LangGraph streams via modes:
- `values` -- full state after each superstep
- `updates` -- only the delta
- `messages` -- LLM token-level streaming
- `debug` -- internal execution trace

**Pattern worth studying**: The **multi-mode streaming** approach. Nika's SSE streaming already sends task events, but the explicit mode selection (full state vs. delta vs. token-level) is a clean API design for `nika serve`.

### Checkpointing

`BaseCheckpointSaver` is pluggable. Every state transition persists. This enables:
- Resume after crash
- Time-travel debugging (replay from any checkpoint)
- Human-in-the-loop (pause at checkpoint, wait for input, resume)

**Pattern worth studying**: Nika has NDJSON traces for replay, but lacks explicit checkpoint-based RESUME. For agents with `max_turns: 50`, being able to resume from turn 35 after a network failure would be valuable.

### Verdict for Nika

- **Adopt**: Multi-mode streaming concept for `nika serve` SSE
- **Study**: Checkpoint-resume for long-running agents
- **Skip**: Pregel superstep model (Nika's topological DAG sort is simpler and sufficient)
- **Skip**: Channel reducers (Nika's explicit `$task_id` bindings are cleaner)

---

## 2. Dify.ai

**Repo**: `langgenius/dify` | **Language**: Python (FastAPI) | **Stars**: ~55k

### Architecture: The Beehive Model

Dify uses a hexagonal architecture they call "Beehive" -- modular cells that collaborate but are independently deployable.

```
Frontend (Next.js)
    |
    v
API (FastAPI/Uvicorn)
    |
    +-- model_runtime/        <-- Provider abstraction
    |   +-- model_providers/
    |       +-- openai/
    |       |   +-- manifest.yaml
    |       |   +-- models/
    |       |       +-- llm/
    |       |       +-- text_embedding/
    |       +-- anthropic/
    |       +-- ... (50+ providers)
    |
    +-- workflow/              <-- Node-based execution
    |   +-- nodes/
    |       +-- llm_call.py
    |       +-- tool.py
    |       +-- condition.py
    |       +-- code.py
    |
    +-- orchestrator/          <-- DAG traversal
        +-- variable_pool.py   <-- Hierarchical state
```

### Provider Layer: Config + Class Hybrid

Each provider is a directory with:
- `manifest.yaml` -- declares models, capabilities, pricing
- `provider.py` -- credential validation
- `models/llm/llm.py` -- actual API call implementation
- `models/llm/model.yaml` -- per-model config

This is a **YAML-manifest + Python-class** hybrid. The manifest declares capabilities declaratively, the class implements them imperatively.

**Nika comparison**: Nika uses a Rust enum (`ProviderName`) + rig-core traits. Adding a provider requires code changes + recompilation. Dify's approach is more dynamic (add a folder, hot-reload). BUT Nika's approach gives compile-time guarantees and zero runtime config parsing. For a compiled binary targeting 14 providers, Nika's approach is correct. Dify's approach makes sense for a web platform where non-developers add providers via UI.

### Variable Pool: Hierarchical State Scoping

Dify's most interesting pattern. Each workflow node has access to a **variable pool** with hierarchical scoping:

```
workflow_level (inputs, env)
  +-- branch_level (branch-local state)
      +-- node_level (current node outputs)
```

Variables are referenced as `{{node_id.output_key}}`. Sound familiar? This is essentially Nika's `$task_id.path` binding system, independently evolved.

**Pattern worth studying**: Dify adds **branch-level scoping** -- when a workflow branches (conditional), each branch gets isolated variable scope. Nika's `with:` bindings are flat (all tasks share the same namespace). Adding branch-level isolation could prevent subtle bugs in complex DAGs with conditional paths.

### Workflow Node Hierarchy

```python
class BaseNode:
    def _run(self, variable_pool) -> NodeRunResult

class LLMNode(BaseNode): ...
class ToolNode(BaseNode): ...
class ConditionNode(BaseNode): ...
class CodeNode(BaseNode): ...
class IterationNode(BaseNode): ...  # for_each equivalent
```

`NodeRunResult` contains `status`, `outputs`, `metadata`, `error`. Each node class handles its own execution, validation, and error reporting.

**Nika comparison**: Nika's verb system (`infer`, `exec`, `fetch`, `invoke`, `agent`) maps 1:1 to Dify's node types but with a crucial difference: Nika's verbs are SEMANTIC (what you want) while Dify's nodes are STRUCTURAL (how to do it). Nika's approach is better for YAML authoring; Dify's is better for visual drag-and-drop.

### Verdict for Nika

- **Adopt**: Nothing directly -- Nika's architecture is already more principled
- **Study**: Branch-level variable scoping for conditional DAGs
- **Note**: Dify proves that YAML-manifest + code-class is a viable provider pattern at scale (50+ providers)
- **Skip**: Beehive hexagonal model (over-engineered for a CLI binary)

---

## 3. Prefect / Dagster

**Language**: Python | **Focus**: General workflow orchestration (not AI-specific)

### Prefect: Decorator-Based DAG Construction

```python
@task(retries=3, retry_delay_seconds=[1, 10, 100])
def extract_data(url: str) -> dict:
    return requests.get(url).json()

@task
def transform(data: dict) -> pd.DataFrame:
    return pd.DataFrame(data)

@flow
def etl_pipeline():
    data = extract_data("https://api.example.com")
    df = transform(data)  # implicit dependency via data flow
```

DAG is inferred from Python data flow (function call graph). No explicit `depends_on`. This is elegant for Python but impossible for YAML-based systems like Nika.

### Retry Pattern: The Gold Standard

Prefect's retry is the most battle-tested in the industry:

```python
@task(
    retries=3,
    retry_delay_seconds=[1, 10, 100],  # exponential backoff as explicit list
    retry_jitter_factor=0.5,            # randomized jitter
    retry_condition_fn=should_retry,    # custom retry predicate
    timeout_seconds=300,                # per-attempt timeout
)
```

**Pattern worth studying**: The **retry_condition_fn** -- a callable that receives the task, the state, and the exception, and returns bool. This is more powerful than Nika's current `retry: { max_attempts, delay_ms, backoff }` because it allows conditional retry based on error type.

Nika could add: `retry: { ... , on: [timeout, rate_limit, server_error] }` to filter which errors trigger retry.

### Dagster: Asset-Based Model

This is the most architecturally interesting pattern in the comparison.

```python
@asset
def raw_users() -> pd.DataFrame:
    return pd.read_sql("SELECT * FROM users", conn)

@asset
def enriched_users(raw_users: pd.DataFrame) -> pd.DataFrame:
    return raw_users.merge(external_data)

@asset
def user_report(enriched_users: pd.DataFrame) -> str:
    return generate_report(enriched_users)
```

Key insight: **assets are the nouns, not the verbs**. You declare WHAT data exists, not HOW to compute it. Dependencies are inferred from function signatures. The framework handles:
- Materialization (when to recompute)
- Caching (skip if input hasn't changed)
- Lineage (who depends on what)
- Partial re-execution (only recompute stale assets)

**The Op/Asset/Resource/IO Manager pattern:**

| Concept | Purpose | Nika Equivalent |
|---------|---------|-----------------|
| **Op** | Atomic computation | Task (verb) |
| **Asset** | Named data output with lineage | Artifact (partial) |
| **Resource** | Shared runtime dependency (DB conn, API key) | `with: { key: $env.API_KEY }` |
| **IO Manager** | How assets are stored/loaded | Artifact system + CAS |

**Pattern worth studying**: Dagster's **partial re-execution**. If a 10-task workflow fails at task 7, Dagster can re-run from task 7 using cached outputs of tasks 1-6. Nika's NDJSON traces contain task outputs, so this is theoretically possible but not implemented.

### Verdict for Nika

- **Adopt**: Conditional retry predicates (`on: [timeout, rate_limit]`)
- **Study**: Partial re-execution from cached task outputs (big UX win for expensive workflows)
- **Study**: Asset-based mental model for artifact documentation/examples
- **Skip**: Decorator-based DAG (incompatible with YAML-first approach)
- **Skip**: IO Manager abstraction (Nika's CAS is simpler and better for binary artifacts)

---

## 4. Rivet (by Ironclad)

**Repo**: `ironcladapp/rivet` | **Language**: TypeScript | **Stars**: ~3k

### Architecture: Visual Node Graph

Rivet represents AI workflows as visual node graphs saved as YAML. Each node has:
- `inputSchema` -- typed inputs
- `outputSchema` -- typed outputs
- `invoke(inputs)` -- execution method

```
[Prompt Node] --> [LLM Node] --> [Extract Node] --> [Output Node]
      |                               ^
      +--- [Context Node] ------------+
```

### Node Registration Pattern

```typescript
interface NodeDefinition {
  type: string;
  inputSchema: Record<string, PortType>;
  outputSchema: Record<string, PortType>;
  invoke(inputs: Record<string, unknown>): Promise<Record<string, unknown>>;
}

class NodeRegistry {
  register(def: NodeDefinition): void;
  get(type: string): NodeDefinition;
}
```

Nodes are registered at startup. External functions from host applications are injected as callable nodes during execution. This is essentially Nika's `invoke:` verb with MCP tools.

### Streaming in Node Graphs

Rivet streams LLM tokens through graph edges using async iterables. The visual debugger shows token-by-token output in real-time. Key pattern: **streaming does NOT block downstream nodes** -- partial results flow through the graph as they arrive.

**Nika comparison**: Nika's streaming is task-level (SSE events per task). Rivet's approach of streaming THROUGH the graph (not just FROM individual tasks) is more powerful but also more complex. For YAML workflows, task-level streaming is the right granularity.

### Error Propagation

Errors halt the current node and all downstream dependents. Each node gets a try-catch wrapper. Error state includes:
- Which node failed
- Input values at time of failure
- Stack trace
- Partial outputs from successful predecessors

**Nika comparison**: Nika's `NIKA-026` (dependency chain failed) serves the same purpose. Rivet's advantage is visual -- you can SEE which node failed in the graph. Nika's TUI DAG visualization could adopt this pattern.

### Verdict for Nika

- **Adopt**: Nothing directly (Rivet is visual-first, Nika is YAML-first)
- **Study**: Error visualization in DAG view (TUI enhancement)
- **Skip**: Node registration pattern (Nika's builtin tools + MCP is more powerful)
- **Note**: Rivet validates that YAML-serialized graphs work at scale (confirms Nika's approach)

---

## 5. DSPy (Stanford NLP)

**Repo**: `stanfordnlp/dspy` | **Language**: Python | **Stars**: ~20k

### The Signature Concept

DSPy's most innovative pattern. A Signature is a **typed specification of LLM behavior**:

```python
class ExtractFacts(dspy.Signature):
    """Extract key facts from a passage."""
    passage: str = dspy.InputField()
    facts: list[str] = dspy.OutputField(desc="list of key facts")
```

This is not a prompt. It's a CONTRACT. DSPy's compiler generates the actual prompt from the signature + examples + optimization passes.

**Nika comparison**: Nika's `structured:` block serves a similar purpose but at a lower level of abstraction:

```yaml
# Nika -- explicit prompt + schema validation
infer: "Extract key facts from: {{with.passage}}"
structured:
  schema:
    type: object
    properties:
      facts: { type: array, items: { type: string } }
```

```python
# DSPy -- declarative signature, prompt is auto-generated
class ExtractFacts(dspy.Signature):
    passage: str = dspy.InputField()
    facts: list[str] = dspy.OutputField()
```

DSPy is MORE abstract (no prompt writing), Nika is MORE explicit (full control over prompt). For a workflow engine, Nika's approach is correct -- users WANT prompt control. DSPy's approach is better for research/optimization.

### Module Composition

Modules are like neural network layers for LLM programs:

```python
class RAG(dspy.Module):
    def __init__(self):
        self.retrieve = dspy.Retrieve(k=3)
        self.generate = dspy.ChainOfThought("context, question -> answer")

    def forward(self, question):
        context = self.retrieve(question)
        return self.generate(context=context, question=question)
```

**Pattern worth studying**: The **forward() method pattern** -- each module has a single entry point that composes sub-modules. This is cleaner than Nika's flat task list for complex workflows. However, Nika's `include:` system with prefixed task IDs achieves similar composition.

### Optimizers (Compilers)

The killer feature. DSPy can automatically:
1. Select few-shot examples from a training set
2. Optimize prompt phrasing
3. Choose between Chain-of-Thought vs. direct answering
4. Tune temperature and other params

This is done by treating the entire program as a differentiable graph and running optimization passes.

**Nika implication**: This is outside Nika's scope (Nika is an ENGINE, not an OPTIMIZER). But Nika could expose hooks for external optimizers to tune workflow parameters.

### Multi-Provider Dispatch

DSPy uses a **global LM registry**:

```python
dspy.settings.configure(lm=dspy.OpenAI(model="gpt-4"))
# OR
lm = dspy.Anthropic(model="claude-3-sonnet")
dspy.settings.configure(lm=lm)
```

Per-module override possible. This is the simplest dispatch pattern: global default + per-call override. Same as Nika's `provider:` at workflow level + task level.

### Verdict for Nika

- **Adopt**: Nothing directly (different abstraction level)
- **Study**: Signature concept for potential `nika explain` output ("this task expects X, produces Y")
- **Study**: Module composition pattern for documentation/mental model
- **Skip**: Optimizers (out of scope for an engine)
- **Note**: DSPy validates that typed I/O contracts improve reliability -- Nika's `structured:` is the right approach

---

## 6. Instructor (by jxnl)

**Repo**: `jxnl/instructor` | **Language**: Python | **Stars**: ~8k

### The Patching Pattern

Instructor's core innovation: **wrap the provider client, don't replace it**.

```python
# Before: raw OpenAI
response = client.chat.completions.create(messages=[...])

# After: patched with Instructor
client = instructor.patch(openai.OpenAI())
response = client.chat.completions.create(
    response_model=User,      # NEW: Pydantic model
    max_retries=3,             # NEW: auto-retry
    messages=[...]
)
# response is now a validated User instance, not raw dict
```

The patch wraps `create()` to:
1. Convert Pydantic model to JSON Schema
2. Inject schema into the API call (via tool_use, json_mode, or prompt)
3. Parse response
4. Validate against Pydantic
5. On failure: retry with validation error feedback to LLM

### Mode Enum: Provider-Aware Extraction Strategy

```python
class Mode(str, Enum):
    TOOLS = "tool_call"          # Function calling API
    JSON = "json_mode"           # Native JSON mode
    MD_JSON = "markdown_json"    # JSON in markdown block
    JSON_SCHEMA = "json_schema"  # OpenAI strict schema
    GEMINI_JSON = "gemini_json"  # Gemini-specific
```

Each mode changes HOW the schema is communicated to the LLM. The `from_provider()` factory auto-selects the best mode per provider.

**Pattern worth studying**: Nika's 5-layer defense does something similar but IMPLICITLY:
- L0: Tool injection (if provider supports it) -- equivalent to `Mode.TOOLS`
- L2: Extract + validate -- handles `Mode.JSON` and `Mode.MD_JSON` cases
- L3: Retry with feedback -- same as Instructor's retry
- L4: LLM repair -- Instructor doesn't have this (Nika is ahead)

The difference: Instructor makes the mode EXPLICIT and user-selectable. Nika auto-selects. Both approaches have merits. For power users, an `extraction_mode:` field in `structured:` could be useful for debugging:

```yaml
structured:
  schema: { ... }
  mode: tools        # Force tool_use extraction (skip auto-detection)
```

### Streaming Partial Objects

Instructor can stream PARTIAL Pydantic objects as tokens arrive:

```python
for partial in client.chat.completions.create_partial(
    response_model=User,
    messages=[...]
):
    print(partial)  # User(name="Ali", age=None)  -> User(name="Alice", age=25)
```

**Pattern worth studying**: Nika's structured output waits for full completion before validation. Streaming partial structured objects would enable real-time UI updates during extraction. This is a v2 feature but architecturally interesting.

### Validation Hooks

```python
class User(BaseModel):
    name: str
    age: int

    @field_validator("age")
    def validate_age(cls, v):
        if v < 0 or v > 150:
            raise ValueError("age must be between 0 and 150")
        return v
```

Custom validators run INSIDE the retry loop. If validation fails, the error message is fed back to the LLM for repair.

**Nika comparison**: Nika uses JSON Schema validation (min/max/pattern/enum). Pydantic-style custom validators would require a scripting layer. JSON Schema `minimum`/`maximum`/`pattern` covers 90% of cases. The remaining 10% is where `exec:` + a validation step handles it.

### Verdict for Nika

- **Adopt**: Consider explicit `mode:` override in `structured:` block for debugging
- **Study**: Streaming partial structured objects for future TUI/serve enhancement
- **Note**: Nika's L4 LLM repair is ahead of Instructor (they don't have it)
- **Note**: Instructor validates that retry-with-error-feedback is the right approach
- **Skip**: Patching pattern (Python-specific, not applicable to Rust)

---

## 7. LiteLLM (by BerriAI)

**Repo**: `BerriAI/litellm` | **Language**: Python | **Stars**: ~15k

### The Grand If/Elif Dispatch

LiteLLM's dirty secret: the core dispatch is a massive `if/elif` chain based on model name prefix:

```python
def completion(model, messages, **kwargs):
    if model.startswith("gpt-") or model.startswith("o1-"):
        return openai_completion(...)
    elif model.startswith("claude-"):
        return anthropic_completion(...)
    elif model.startswith("azure/"):
        return azure_completion(...)
    # ... 100+ elif branches
```

There was a community proposal to refactor to Strategy/Registry pattern, but the if/elif persists because it's "battle-tested" and any refactor risks breaking the 100+ providers.

**Nika comparison**: Nika uses `match provider_name { ... }` in Rust, which is the same pattern but with compile-time exhaustiveness checking. The Rust compiler FORCES you to handle every variant. LiteLLM's if/elif can silently miss a provider.

### The Router Class

LiteLLM's most architecturally interesting component:

```python
router = Router(
    model_list=[
        {"model_name": "gpt-4", "litellm_params": {"model": "azure/gpt-4-east", "api_key": "..."}},
        {"model_name": "gpt-4", "litellm_params": {"model": "azure/gpt-4-west", "api_key": "..."}},
        {"model_name": "gpt-4", "litellm_params": {"model": "openai/gpt-4", "api_key": "..."}},
    ],
    routing_strategy="least-busy",    # or "simple-shuffle", "latency-based-routing"
    fallbacks=[{"gpt-4": ["gpt-3.5-turbo"]}],
    num_retries=3,
)
```

**Pattern worth studying**: The **model_list** concept -- multiple deployments of the "same" model with load balancing. For Nika, this maps to the `endpoint:` concept (multiple backends for the same provider). The routing_strategy enum is clean:

```
simple-shuffle    -- random
least-busy        -- fewest in-flight requests
latency-based     -- lowest recent latency
cost-based        -- cheapest available
```

Nika could adopt this for `nika.toml` provider configuration:

```toml
[[provider.anthropic.endpoints]]
name = "primary"
api_key = "$env.ANTHROPIC_API_KEY"
priority = 1

[[provider.anthropic.endpoints]]
name = "fallback"
api_key = "$env.ANTHROPIC_API_KEY_2"
priority = 2
```

### Streaming Unification

LiteLLM normalizes all provider streams to OpenAI-format chunks:

```python
{"choices": [{"delta": {"content": "Hello"}, "index": 0}]}
```

Every provider's streaming format is converted to this. This is the same approach as Nika (all providers emit `ProviderEvent::Token(String)` events).

### Error Normalization

LiteLLM maps provider errors to standardized codes:
- 429 -> `RateLimitError`
- 401 -> `AuthenticationError`
- 500 -> `ServiceUnavailableError`
- Timeout -> `Timeout`

**Pattern worth studying**: Nika's `NikaError` codes (NIKA-XXX) are more granular but could benefit from a higher-level error CATEGORY for retry decisions:

```rust
enum ErrorCategory {
    Retryable,      // 429, 500, timeout
    NonRetryable,   // 401, 403, schema error
    Partial,        // Some items in for_each failed
}
```

### Verdict for Nika

- **Adopt**: Error categorization for smarter retry decisions
- **Study**: Router/load-balancing pattern for multi-endpoint support
- **Study**: Fallback chains between providers (automatic failover)
- **Skip**: If/elif dispatch (Nika's enum + match is strictly better)
- **Note**: LiteLLM's supply chain compromise (March 2026) validates Nika's compiled-binary distribution model -- no PyPI supply chain risk

---

## 8. Mastra.ai

**Repo**: `mastra-ai/mastra` | **Language**: TypeScript | **Stars**: ~5k

### Architecture: TypeScript-First AI Framework

Mastra builds on Vercel AI SDK for provider abstraction and adds:
- Agent framework
- Tool system with Zod validation
- Workflow engine
- Memory management

### Tool System: Zod-Schema Pattern

```typescript
const githubTool = createTool({
    id: "get-github-repo",
    inputSchema: z.object({
        owner: z.string(),
        repo: z.string(),
    }),
    outputSchema: z.object({
        stars: z.number(),
        forks: z.number(),
    }),
    execute: async ({ context }) => {
        // ...
    },
});
```

**Pattern worth studying**: The **explicit output schema on tools**. Nika's builtin tools (nika:*) have implicit output types known to the engine. MCP tools declare their schemas via the protocol. But Mastra's approach of Zod schemas on both input AND output is more explicit and enables compile-time validation of tool chains.

### Workflow as Typed Steps

Mastra workflows are typed step chains with explicit input/output contracts at each step. Each step logs its inputs and outputs for observability.

**Nika comparison**: Nika's task outputs are dynamically typed (`Value` enum). Mastra's typed steps provide better IDE support but less flexibility. For a YAML-first engine, dynamic typing is the right call.

### Agent Evaluation

Mastra includes built-in evaluation:
- Model-graded evals (LLM judges output quality)
- Rule-based evals (regex, length, keyword)
- Statistical evals (BLEU, ROUGE)

**Pattern worth studying**: Nika's agent guardrails (`regex`, `length`, `schema`, `llm` judge) implement the same four evaluation types. Mastra adds statistical metrics which could be useful for testing workflows.

### Verdict for Nika

- **Adopt**: Nothing directly (TypeScript patterns don't transfer well to Rust)
- **Study**: Explicit output schemas on tools for documentation/validation
- **Note**: Mastra validates that Zod/JSON Schema for tool I/O is the industry standard
- **Skip**: Vercel AI SDK dependency (Nika owns its provider layer)

---

## Cross-Cutting Architectural Patterns

### Pattern 1: Provider Dispatch Approaches

| Framework | Pattern | Extensibility | Type Safety |
|-----------|---------|---------------|-------------|
| **Nika** | Rust enum + trait (rig-core) | Recompile | Compile-time exhaustive |
| LangGraph | Delegates to LangChain | Plugin | Runtime |
| Dify | YAML manifest + Python class | Hot-reload | Runtime |
| LiteLLM | If/elif chain | Code change | None |
| Instructor | Monkey-patch wrapper | Adapter | Runtime |
| DSPy | Global registry | Config | Runtime |
| Mastra | Vercel AI SDK delegation | Package | TypeScript |

**Nika is uniquely positioned**: only compiled-binary engine with compile-time provider guarantees. This is a feature, not a limitation.

### Pattern 2: Structured Output Strategies

| Framework | Approach | Layers | Cross-Provider |
|-----------|----------|--------|----------------|
| **Nika** | 5-layer defense (tool -> extract -> validate -> retry -> repair) | 5 | Yes (all 14) |
| Instructor | Mode enum + Pydantic retry | 3 | Yes (via patching) |
| DSPy | Signature compilation | 2 | Yes (via compilation) |
| OpenAI | Native json_schema (constrained decoding) | 1 | No (OpenAI only) |
| Dify | Provider-specific + fallback parsing | 2 | Yes |

**Nika has the most robust approach**. The L4 LLM repair layer is unique.

### Pattern 3: DAG Execution Models

| Framework | Model | Dynamic Routing | Parallel |
|-----------|-------|-----------------|----------|
| **Nika** | Topological sort + binding resolution | `when:` conditional | `for_each` with `concurrency:` |
| LangGraph | Pregel supersteps + conditional edges | State-based routing | Superstep parallelism |
| Dify | Node traversal + variable pool | Condition nodes | Parallel branches |
| Prefect | Implicit from Python data flow | Python conditionals | Task-level |
| Dagster | Asset dependency graph | Automatic | Op-level |

### Pattern 4: Error Handling Taxonomy

Most mature pattern across all frameworks:

```
Error detected
    |
    +-- Is it retryable? (429, 500, timeout)
    |       |
    |       +-- Retry with backoff
    |       +-- On max retries: propagate
    |
    +-- Is it a validation error? (structured output)
    |       |
    |       +-- Retry with error feedback to LLM
    |       +-- On max retries: LLM repair (Nika unique)
    |
    +-- Is it a dependency failure?
    |       |
    |       +-- Skip downstream tasks (NIKA-026)
    |
    +-- Is it fatal? (auth, config, schema)
            |
            +-- Fail fast with clear error
```

### Pattern 5: Streaming Architectures

| Framework | Token-level | Task-level | Graph-level |
|-----------|-------------|------------|-------------|
| **Nika** | Yes (SSE) | Yes (events) | No |
| LangGraph | Yes (messages mode) | Yes (updates mode) | Yes (values mode) |
| Rivet | Yes (visual) | Yes | Yes (graph edges) |
| LiteLLM | Yes (normalized chunks) | N/A | N/A |

---

## Actionable Recommendations for Nika

### Priority 1: Adopt Now (Pre-Launch)

1. **Error categorization for retry** -- Add `ErrorCategory::Retryable | NonRetryable | Partial` to improve retry intelligence. Simple enum, big impact.

2. **Conditional retry predicates** -- Extend `retry:` block with `on:` field:
   ```yaml
   retry:
     max_attempts: 3
     delay_ms: 1000
     on: [rate_limit, timeout, server_error]  # Skip retry on auth/schema errors
   ```

### Priority 2: Study for Post-Launch

3. **Partial re-execution** -- Resume workflow from failed task using cached outputs from NDJSON traces. Dagster proves this is the #1 UX improvement for expensive workflows.

4. **Multi-mode streaming for nika serve** -- Add `?stream_mode=updates|values|tokens` query parameter to SSE endpoint. LangGraph validates the pattern.

5. **Checkpoint-resume for agents** -- Persist agent state at each turn. Resume from last turn after crash/timeout. LangGraph's `BaseCheckpointSaver` is the reference.

### Priority 3: Consider for Future Versions

6. **Explicit extraction mode override** -- `structured: { mode: tools | json | prompt }` for debugging. Instructor's Mode enum is the reference.

7. **Provider fallback chains** -- `fallback: [anthropic, openai, gemini]` at workflow level. LiteLLM's Router is the reference.

8. **Branch-level variable scoping** -- Isolate variable namespaces in conditional branches. Dify's variable pool is the reference.

### Explicitly Skip

- LangChain-style abstraction layers (too many indirections)
- Plugin/registry dynamic loading (not needed for compiled binary)
- DSPy-style prompt optimization (out of scope for an engine)
- Visual node graph editor (Nika is YAML-first; studio/TUI is the visual layer)

---

## Methodology

- **Tools used**: Perplexity (sonar-pro) for web research, 12 queries
- **Systems analyzed**: 8 (LangGraph, Dify, Prefect, Dagster, Rivet, DSPy, Instructor, LiteLLM, Mastra)
- **Focus**: Architecture and code patterns, not features or marketing
- **Cross-reference**: Patterns validated across 3+ sources
- **Time period**: 2024-2026 (current state of the art)

## Sources

1. LangGraph docs + GitHub (langchain-ai/langgraph)
2. Dify.ai docs + GitHub (langgenius/dify)
3. Prefect docs (prefect.io)
4. Dagster docs (dagster.io)
5. Rivet GitHub (ironcladapp/rivet)
6. DSPy docs + GitHub (stanfordnlp/dspy)
7. Instructor docs + GitHub (jxnl/instructor)
8. LiteLLM docs + GitHub (BerriAI/litellm)
9. Mastra docs + GitHub (mastra-ai/mastra)
10. Perplexity AI cross-framework comparisons (April 2026)
