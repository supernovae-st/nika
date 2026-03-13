# Research Report: Binding & Data-Flow Patterns in Modern Workflow Engines

**Date**: 2026-03-13
**Author**: Claude (Opus 4) + Thibaut
**Purpose**: Competitive analysis of data binding/flow patterns across 4 workflow engines
**Status**: RESEARCH ONLY -- no implementation code

---

## Summary

Four modern workflow engines were analyzed for their binding and data-flow patterns:
Temporal.io, Windmill.dev, Flyte (Union.ai), and Hatchet. Each takes a fundamentally
different approach to passing data between tasks -- from Temporal's serialization-layer
abstraction, to Flyte's Python-native promise system, to Windmill's JavaScript expression
engine, to Hatchet's context-based parent output access. This report extracts concrete
lessons for Nika's `use:` / `{{use.alias}}` binding system.

---

## 1. Temporal.io

### Data Passing Model

Temporal passes data between Activities and Workflows through **function arguments and
return values**. There is no YAML binding syntax -- data flow is expressed as normal
TypeScript/Python/Go code.

```typescript
// Workflow orchestrates activities via typed function calls
import { proxyActivities } from '@temporalio/workflow';
import type * as activities from './activities';

const { greet, sendEmail } = proxyActivities<typeof activities>({
  startToCloseTimeout: '1 minute',
});

// Data flow: greeting is the return value of greet(), passed to sendEmail()
export async function onboardingWorkflow(name: string): Promise<void> {
  const greeting = await greet(name);          // Activity 1 output
  await sendEmail(greeting, name);             // Activity 2 input
}
```

### Serialization Architecture (PayloadConverter / DataConverter)

Temporal's type safety comes from its **three-layer serialization chain**:

```
Application Object
    --> PayloadConverter.toPayload()     (serialize to binary)
        --> PayloadCodec.encode()        (encrypt/compress)
            --> gRPC wire format
            --> PayloadCodec.decode()
        --> PayloadConverter.fromPayload()
    --> Application Object
```

**PayloadConverter**: Converts between application types and Payload (binary).
Runs inside the Workflow sandbox. Must be deterministic.

**PayloadCodec**: Transforms Payload-to-Payload (encryption, compression).
Runs outside sandbox. Can call external services.

**CompositePayloadConverter**: Chain of converters tried in order:

```typescript
// Default chain
export class DefaultPayloadConverter extends CompositePayloadConverter {
  constructor() {
    super(
      new UndefinedPayloadConverter(),
      new BinaryPayloadConverter(),
      new JsonPayloadConverter(),
    );
  }
}

// Custom: EJSON for BigInt, Date, Regex, Uint8Array
export const payloadConverter = new CompositePayloadConverter(
  new UndefinedPayloadConverter(),
  new EjsonPayloadConverter(),  // handles non-JSON-native types
);
```

### Typed Inputs/Outputs

Temporal recommends **single object parameters** for forward compatibility:

```typescript
// Recommended: object parameter
type ExampleArgs = { name: string; born: number };
export async function example({ name, born }: ExampleArgs): Promise<string> {
  return `Hello ${name}, you were born in ${born}.`;
}

// Client invocation with typed args
await client.workflow.start(example, {
  args: [{ name: 'Temporal', born: 2019 }],
  taskQueue: 'your-queue',
  workflowId: 'business-meaningful-id',
});
```

All parameters must be **serializable**. Limits: 2MB per argument, 4MB per gRPC message.

### Child Workflows (DAG composition)

```typescript
import { executeChild, startChild } from '@temporalio/workflow';

export async function parentWorkflow(...names: string[]): Promise<string> {
  // Fan-out: parallel child workflows
  const results = await Promise.all(
    names.map(name => executeChild(childWorkflow, { args: [name] }))
  );
  return results.join('\n');
}
```

### What's New in 2025: Temporal Nexus

Temporal Nexus enables **cross-namespace, cross-cluster** workflow composition.
It's their answer to microservice data boundaries -- typed RPC between
independently deployed Temporal namespaces.

### Key Takeaway for Nika

| Aspect | Temporal | Nika |
|--------|----------|------|
| Binding syntax | Function args/returns (code) | `use:` block + `{{use.alias}}` (YAML) |
| Type safety | Language-native (TypeScript/Python types) | JSON Schema validation (OutputSpec) |
| Serialization | PayloadConverter chain (pluggable) | serde_json (fixed) |
| Transform | Full language (inside workflow sandbox) | Template interpolation only |
| Lazy loading | N/A (activities are already deferred) | `lazy: true` on bindings |

**What Nika could learn**:
- **Composite converter chain**: Temporal's `CompositePayloadConverter` pattern --
  trying converters in order -- could inspire a pluggable binding resolver chain
  in Nika (e.g., try JSON parse, then string coercion, then error).
- **Single-object recommendation**: Temporal strongly recommends single object params
  for evolution. Nika's `use:` block already achieves this naturally.
- **Protobuf support**: Temporal supports Protobuf natively as an alternative to JSON.
  Nika could benefit from a binary serialization option for large inter-step data.

---

## 2. Windmill.dev

### Data Flow Architecture

Windmill uses **input transforms** -- JavaScript expressions that map any previous step's
output to the current step's parameters. Every flow is actually a **DAG** (not a linear
sequence), because any step can reference any prior step's output.

```
flow_input  --->  Step A  --->  Step B  --->  Step C
                    |                          ^
                    +--------------------------+
                    (Step C can reference Step A directly)
```

### Binding Syntax (Input Transforms)

Each step parameter is bound using a JavaScript expression with these globals:

| Global | Description |
|--------|-------------|
| `flow_input` | The workflow's input parameters |
| `results.{step_id}` | Output of step with given ID |
| `resource(path)` | Workspace resource at path |
| `variable(path)` | Workspace variable at path |
| `flow_env.NAME` | Flow-level environment variable |

```json
// Windmill OpenFlow JSON format (step input transform)
{
  "input_transforms": {
    "message": {
      "type": "javascript",
      "expr": "results.step_a.transformed_message.toUpperCase()"
    },
    "count": {
      "type": "static",
      "value": 42
    },
    "name": {
      "type": "javascript",
      "expr": "flow_input.user_name"
    }
  }
}
```

### JavaScript Evaluation Engine

Windmill offers **two JS engines** for evaluating expressions:

| Engine | Speed | Compatibility | Default |
|--------|-------|---------------|---------|
| **QuickJS** | 8-16x faster startup | Limited stdlib | Community Edition |
| **Deno (V8)** | Full V8 | Complete compat | Enterprise Edition |

QuickJS uses **lazy evaluation** -- `resource()` and `variable()` calls are resolved
on-demand, not pre-fetched.

### For Loops

```json
{
  "type": "forloopflow",
  "iterator": {
    "type": "javascript",
    "expr": "results.step_a.items"
  },
  "skip_failures": false,
  "parallel": true,
  "parallelism": 5
}
```

Inside loops, steps access `flow_input.iter.value` and `flow_input.iter.index`.

### Custom Flow States (Escape Hatch)

When output/input passing is insufficient, Windmill provides a **flow-level state store**:

```typescript
import * as wmill from "windmill-client@1";

export async function main(x: string) {
  await wmill.setFlowUserState("FOO", 42);
  return await wmill.getFlowUserState("FOO");
}
```

### Shared Directory (Heavy Data)

For non-JSON data (files, binary), steps share a `./shared` directory:

```
Step A writes: ./shared/large_dataset.parquet
Step B reads:  ./shared/large_dataset.parquet
```

### Workflows-as-Code Alternative

```python
from wmill import task

@task()
def heavy_compute(n: int):
    df = pd.DataFrame(np.random.randn(100, 4), columns=list('ABCD'))
    return df.sum().sum()

@task
def send_result(res: int, email: str):
    return "OK"

def main(n: int):
    results = [heavy_compute(i) for i in range(n)]
    return send_result(sum(results), "user@example.com")
```

### Key Takeaway for Nika

| Aspect | Windmill | Nika |
|--------|----------|------|
| Binding syntax | JavaScript expressions (`results.step_a.field`) | `{{use.alias}}` templates |
| Transform | Full JavaScript (arbitrary computation) | String interpolation only |
| Engine | QuickJS or Deno V8 | Handlebars-like templates |
| State escape hatch | `setFlowUserState()` / `getFlowUserState()` | DataStore (similar) |
| Heavy data | Shared directory (`./shared/`) | Artifact system (`io::writer`) |

**What Nika could learn**:
- **Expression engine**: Windmill's biggest advantage is that bindings are **full
  JavaScript expressions**, not just path lookups. `results.a.items.filter(x => x > 10).length`
  is valid. Nika could benefit from a minimal expression language (e.g., jq-like or
  JSONPath) beyond `{{use.alias}}` string interpolation.
- **Static vs Dynamic bindings**: Windmill explicitly distinguishes `"type": "static"`
  from `"type": "javascript"`. Nika's templates are always dynamic. Having an explicit
  static mode could enable compile-time validation.
- **Dual engine approach**: QuickJS for simple expressions, Deno for complex ones.
  If Nika adds an expression engine, starting with a lightweight evaluator (like
  `jmespath` or `jsonpath`) and graduating to something heavier is a good pattern.
- **Flow-level env vars**: `flow_env.VARIABLE_NAME` accessible from any step.
  Nika's `context:` does something similar but with file-loading semantics.

---

## 3. Flyte (Union.ai)

### Type System Philosophy

Flyte's binding model is **Python-native with compile-time type checking**. Tasks
and workflows are decorated Python functions with **strong type annotations**.
Data binding happens through **Python's type system + Flyte's Promise mechanism**.

```python
from flytekit import task, workflow

@task
def slope(x: list[int], y: list[int]) -> float:
    sum_xy = sum([x[i] * y[i] for i in range(len(x))])
    sum_x_squared = sum([x[i] ** 2 for i in range(len(x))])
    n = len(x)
    return (n * sum_xy - sum(x) * sum(y)) / (n * sum_x_squared - sum(x) ** 2)

@task
def intercept(x: list[int], y: list[int], slope: float) -> float:
    mean_x = sum(x) / len(x)
    mean_y = sum(y) / len(y)
    return mean_y - slope * mean_x

# Workflow: data flows through function call syntax
# slope_value is a PROMISE, not a float -- resolved at execution time
@workflow
def simple_wf(x: list[int], y: list[int]) -> float:
    slope_value = slope(x=x, y=y)
    intercept_value = intercept(x=x, y=y, slope=slope_value)  # promise passed
    return intercept_value
```

### Promise-Based Data Flow

Inside a `@workflow` function, task return values are **Promises** (not actual values).
They can only be passed to other tasks -- you cannot inspect them directly:

```python
@workflow
def example_wf(x: list[int], y: list[int]) -> float:
    slope_value = slope(x=x, y=y)       # This is a Promise<float>, not float
    # print(slope_value)                 # ERROR: can't inspect a Promise
    # if slope_value > 0:               # ERROR: can't compare a Promise
    return intercept(x=x, y=y, slope=slope_value)  # OK: pass to another task
```

This forces a **clean DAG structure** -- the workflow definition IS the DAG.

### Named Outputs (NamedTuple)

```python
from typing import NamedTuple

slope_and_intercept = NamedTuple("slope_and_intercept", [
    ("slope", float),
    ("intercept", float)
])

@workflow
def wf(x: list[int], y: list[int]) -> slope_and_intercept:
    s = slope(x=x, y=y)
    i = intercept(x=x, y=y, slope=s)
    return slope_and_intercept(slope=s, intercept=i)
```

### Dataclass Types (Structured I/O)

```python
from dataclasses import dataclass
from flytekit.types.file import FlyteFile
from flytekit.types.directory import FlyteDirectory
from flytekit.types.structured import StructuredDataset

@dataclass
class FlyteTypes:
    dataframe: StructuredDataset
    file: FlyteFile
    directory: FlyteDirectory

@task
def upload_data() -> FlyteTypes:
    df = pd.DataFrame({"Name": ["Tom"], "Age": [20]})
    return FlyteTypes(
        dataframe=StructuredDataset(dataframe=df),
        file=FlyteFile("path/to/file"),
        directory=FlyteDirectory("path/to/dir"),
    )

@task
def download_data(res: FlyteTypes):
    df = res.dataframe.open(pd.DataFrame).all()  # Lazy load!
    f = open(res.file, "r")                       # Lazy download!
```

### Lazy Data Loading (FlyteFile, FlyteDirectory)

Flyte's killer feature for large data: **FlyteFile** and **FlyteDirectory** are
**lazy references** that only download data when accessed:

```
Task A --> outputs FlyteFile (upload to S3)
              |
              v (only a URI is passed, not the data)
Task B --> opens FlyteFile (download from S3 on demand)
```

This is conceptually similar to Nika's `lazy: true` binding modifier.

### Dynamic Workflows (Runtime DAG)

```python
@dynamic
def count_characters(s1: str, s2: str) -> int:
    freq1 = [0] * 26
    for i in range(len(s1)):            # Loop length unknown until runtime
        index = return_index(character=s1[i])
        freq1 = update_list(freq_list=freq1, list_index=index)  # Each is a task
    return derive_count(freq1=freq1, freq2=freq2)
```

### Map Tasks (Parallel Fan-Out)

```python
from flytekit import map_task
import functools

@task
def detect_anomalies(data_point: int) -> bool:
    return data_point > threshold

@workflow
def map_workflow(data: list[int]) -> list[bool]:
    return map_task(detect_anomalies)(data_point=data)

# With partial binding for multi-input map tasks
@workflow
def multi_input_map(quantities: list[int], price: float) -> list[float]:
    partial = functools.partial(multi_input_task, price=price, shipping=7.0)
    return map_task(partial)(quantity=quantities)
```

### FlyteIDL (Protobuf Type System)

Under the hood, Flyte serializes all data through **FlyteIDL** (Interface Definition
Language) based on Protocol Buffers. The type mapping:

| Python Type | Flyte IDL Type |
|-------------|----------------|
| `int` | `Integer` |
| `float` | `Float` |
| `str` | `String` |
| `bool` | `Boolean` |
| `list[T]` | `Collection[T]` |
| `dict[K,V]` | `Map[K,V]` |
| `@dataclass` | `Struct` (JSON) |
| `FlyteFile` | `Blob` (URI + metadata) |
| `FlyteDirectory` | `Blob` (multipart) |
| `StructuredDataset` | `StructuredDataset` (schema + URI) |
| `Enum` | `String` (with validation) |

### Key Takeaway for Nika

| Aspect | Flyte | Nika |
|--------|-------|------|
| Binding syntax | Python function args (native) | `use:` block + `{{use.alias}}` |
| Type safety | Python type annotations + FlyteIDL | JSON Schema (OutputSpec) |
| Lazy loading | FlyteFile/FlyteDirectory (URI refs) | `lazy: true` modifier |
| Fan-out | `map_task()` with `functools.partial` | `for_each:` with `concurrency:` |
| Dynamic DAG | `@dynamic` decorator | `decompose:` modifier |
| Named outputs | `NamedTuple` | Task output is a single value |

**What Nika could learn**:
- **Promise pattern**: Flyte's promise-based data flow is elegant -- return values
  from tasks are not actual values but typed references. Nika's `$task_id` binding
  is conceptually similar but lacks compile-time type propagation. Adding type
  inference to the analyzer (Phase 2 AST) could catch binding mismatches early.
- **Lazy file references**: Flyte's `FlyteFile` is exactly what Nika's artifact
  system could evolve into -- a typed reference that transparently downloads on access.
  Nika's `lazy: true` is a step in this direction.
- **functools.partial for map_task**: The pattern of partially binding some inputs
  before fanning out is very clean. Nika's `for_each` could support a similar
  "bind these values statically, iterate over that one" pattern.
- **Named outputs**: Nika tasks return a single value. Supporting named outputs
  (e.g., `use: { slope: $calc.slope, intercept: $calc.intercept }`) with
  dot-notation access would be more expressive.

---

## 4. Hatchet

### Data Flow Model

Hatchet uses a **context-based** data passing model. Tasks in a DAG access parent
outputs through the `context` object. The newer (2025) API simplifies this with
typed generics.

### DAG Definition (Python -- Legacy API)

```python
@hatchet.workflow(on_events=["dag:create"])
class DagWorkflow:

    @hatchet.step(timeout="5s")
    def step1(self, context: Context) -> dict[str, int]:
        return {"rando": random.randint(1, 100)}

    @hatchet.step(timeout="5s")
    def step2(self, context: Context) -> dict[str, int]:
        return {"rando": random.randint(1, 100)}

    # Explicit parent declaration
    @hatchet.step(parents=["step1", "step2"])
    def step3(self, context: Context) -> dict[str, int]:
        # Access parent outputs through context
        one = cast(dict, context.step_output("step1"))["rando"]
        two = cast(dict, context.step_output("step2"))["rando"]
        return {"sum": one + two}

    @hatchet.step(parents=["step1", "step3"])
    def step4(self, context: Context) -> dict[str, str]:
        # Can access any ancestor's output
        print(context.step_output("step1"))
        print(context.step_output("step3"))
        return {"step4": "done"}
```

### DAG Definition (TypeScript -- New Task API)

```typescript
type DagInput = { Message: string };
type DagOutput = { reverse: { Original: string; Transformed: string } };

// Workflow with typed input/output
const dag = hatchet.workflow<DagInput, DagOutput>({ name: 'simple' });

// First task: receives workflow input directly
const toLower = dag.task({
  name: 'to-lower',
  fn: (input) => ({
    TransformedMessage: input.Message.toLowerCase(),
  }),
});

// Second task: typed parent output access
dag.task({
  name: 'reverse',
  parents: [toLower],  // Typed reference to parent task
  fn: async (input, ctx) => {
    const lower = await ctx.parentOutput(toLower);  // Typed!
    return {
      Original: input.Message,
      Transformed: lower.TransformedMessage.split('').reverse().join(''),
    };
  },
});
```

### Standalone Tasks (Simple API)

```typescript
export const simple = hatchet.task({
  name: 'simple',
  retries: 3,
  fn: async (input: SimpleInput) => {
    return { TransformedMessage: input.Message.toLowerCase() };
  },
});

// Triggering
const result = await simple.run({ Message: "HELLO" });
// result.TransformedMessage === "hello"
```

### Child Workflow Spawning

```typescript
const parent = hatchet.task({
  name: 'parent',
  fn: async (input: ParentInput, ctx) => {
    const promises = [];
    for (let i = 0; i < input.N; i++) {
      promises.push(child.run({ N: i }));  // spawn child tasks
    }
    const childRes = await Promise.all(promises);
    return { Result: childRes.reduce((acc, curr) => acc + curr.Value, 0) };
  },
});
```

### Python Fanout Pattern

```python
@hatchet.step(timeout="5m")
async def spawn(self, context: Context) -> dict[str, Any]:
    results = []
    for i in range(n):
        results.append(
            (await context.aio.spawn_workflow(
                "Child",
                {"a": str(i)},
                key=f"child{i}",
            )).result()
        )
    return {"results": await asyncio.gather(*results)}
```

### Key Takeaway for Nika

| Aspect | Hatchet | Nika |
|--------|---------|------|
| Binding syntax | `context.step_output("step_id")` / `ctx.parentOutput(ref)` | `use: { alias: $step_id }` |
| Type safety | TypeScript generics on workflow/task | JSON Schema (OutputSpec) |
| DAG definition | `parents: [task_ref]` array | `flows:` section |
| Fan-out | `child.run()` + `Promise.all()` | `for_each:` + `concurrency:` |
| Child spawning | `spawn_workflow()` / `child.run()` | `spawn_agent` internal tool |

**What Nika could learn**:
- **Typed parent references**: Hatchet's `ctx.parentOutput(toLower)` is type-safe
  because `toLower` is a typed task reference, not a string. The TypeScript compiler
  knows the return type. Nika could achieve this in the analyzer by tracking output
  types through the DAG and validating `use:` references.
- **Task-as-value pattern**: Hatchet treats tasks as first-class values that can be
  passed to `parents: [toLower]`. This is cleaner than Nika's string-based
  `flows: [{ source: "a", target: "b" }]`. Consider allowing typed task references.
- **Standalone vs workflow tasks**: Hatchet distinguishes standalone `hatchet.task()`
  from workflow-bound `dag.task()`. Nika treats everything as workflow tasks. A
  standalone task concept could simplify simple one-shot operations.

---

## Comparative Analysis

### Binding Syntax Comparison

```
TEMPORAL:    const result = await activity(input);
             // Native function calls. No special binding syntax.

WINDMILL:    "expr": "results.step_a.items.filter(x => x.active).length"
             // Full JavaScript expressions. Maximum flexibility.

FLYTE:       intercept_value = intercept(x=x, y=y, slope=slope_value)
             // Python function calls with Promise values. Compile-time types.

HATCHET:     const lower = await ctx.parentOutput(toLower);
             // Context method with typed task reference.

NIKA:        use:
               result: $step1
             infer: "Process: {{use.result}}"
             // YAML binding block + template interpolation.
```

### Type Safety Spectrum

```
MOST TYPED -------------------------------------------------- LEAST TYPED

  Flyte          Temporal        Hatchet (TS)     Nika          Windmill
  (Python types  (TypeScript     (generics on     (JSON Schema  (JS exprs
   + FlyteIDL     types +         workflow/task    on output,    evaluated
   protobuf)      PayloadConv)    definitions)     template      at runtime)
                                                   validation)
```

### Transform Capabilities

| Engine | Can Transform? | How? |
|--------|---------------|------|
| Temporal | Full language | Workflow code is the transform |
| Windmill | Full JavaScript | Input transform expressions |
| Flyte | Full Python | Inside `@dynamic` or `@task` |
| Hatchet | Full language | Task function body |
| **Nika** | **Template only** | **`{{use.alias}}` interpolation** |

Nika is the only engine that cannot perform inline transforms between steps.
All others have full programming language access.

### Lazy Loading Comparison

| Engine | Lazy Mechanism | Granularity |
|--------|---------------|-------------|
| Temporal | Activities are inherently lazy (scheduled) | Activity-level |
| Windmill | QuickJS lazy `resource()` evaluation | Expression-level |
| Flyte | `FlyteFile` / `FlyteDirectory` (URI reference) | File/Directory-level |
| Hatchet | N/A (results cached in context) | N/A |
| **Nika** | **`lazy: true` binding modifier** | **Binding-level** |

---

## Recommendations for Nika

### High Priority

1. **Minimal Expression Language for Bindings**
   Nika is the only engine limited to string interpolation. Every competitor supports
   at least path access with dot notation and array indexing. Consider:
   - **JSONPath** subset: `$step1.items[0].name`
   - **jq-like** filters: `$step1.items | length`
   - Or at minimum: `$step1.items.0.name` (dot-path access into nested JSON)

2. **Type Propagation in Analyzer**
   Flyte and Hatchet both do compile-time type checking of data flow. Nika's Phase 2
   analyzer could track output types (from `output:` schema) through the DAG and
   validate `use:` references at parse time instead of runtime.

3. **Named Output Fields**
   Currently Nika tasks return a single value. Supporting named outputs:
   ```yaml
   - id: calc
     infer: "Calculate slope and intercept"
     output:
       schema: { slope: number, intercept: number }

   - id: display
     use:
       s: $calc.slope         # Access named field
       i: $calc.intercept     # Access named field
   ```

### Medium Priority

4. **Flow-Level Variables** (like Windmill's `flow_env`)
   Currently Nika has `context: files:` for loading external data. A simpler
   `env:` or `vars:` block for constant values shared across steps would reduce
   template complexity.

5. **Typed Task References in flows:**
   Instead of string-based flow definitions:
   ```yaml
   # Current
   flows:
     - source: step_a
       target: step_b

   # Could be: implicit flows from use: block
   # (step_b uses step_a, therefore step_a -> step_b)
   ```
   Nika already does implicit flow inference from `use:` -- consider making
   `flows:` optional when `use:` provides sufficient dependency info.

6. **Pluggable Serialization** (like Temporal's CompositePayloadConverter)
   For large data passing between steps, allowing pluggable serialization
   (JSON, MessagePack, Protobuf) could improve performance.

### Low Priority

7. **Shared Directory for Heavy Data** (like Windmill)
   Nika's artifact system is more sophisticated, but a simple `./shared/` escape
   hatch for binary/file data between steps could complement it.

8. **Standalone Task Concept** (like Hatchet)
   Tasks that don't need a full workflow definition for one-shot operations.

---

## Sources

1. **Temporal.io**
   - [Core Application - TypeScript SDK](https://docs.temporal.io/develop/typescript/core-application)
   - [Converters and Encryption](https://docs.temporal.io/develop/typescript/converters-and-encryption)
   - [Child Workflows](https://docs.temporal.io/develop/typescript/child-workflows)

2. **Windmill.dev**
   - [Architecture and Data Exchange](https://www.windmill.dev/docs/flows/architecture)
   - [For Loops](https://www.windmill.dev/docs/flows/flow_loops)
   - [Flow Editor Components](https://www.windmill.dev/docs/flows/editor_components)
   - [Workflows as Code](https://www.windmill.dev/docs/core_concepts/workflows_as_code)

3. **Flyte (Union.ai)**
   - [flytesnacks/basics/workflow.py](https://github.com/flyteorg/flytesnacks/blob/master/examples/basics/basics/workflow.py)
   - [flytesnacks/data_types_and_io/dataclass.py](https://github.com/flyteorg/flytesnacks/blob/master/examples/data_types_and_io/data_types_and_io/dataclass.py)
   - [flytesnacks/data_types_and_io/file.py](https://github.com/flyteorg/flytesnacks/blob/master/examples/data_types_and_io/data_types_and_io/file.py)
   - [flytesnacks/advanced_composition/dynamic_workflow.py](https://github.com/flyteorg/flytesnacks/blob/master/examples/advanced_composition/advanced_composition/dynamic_workflow.py)
   - [flytesnacks/advanced_composition/map_task.py](https://github.com/flyteorg/flytesnacks/blob/master/examples/advanced_composition/advanced_composition/map_task.py)
   - [flytesnacks/basics/named_outputs.py](https://github.com/flyteorg/flytesnacks/blob/master/examples/basics/basics/named_outputs.py)

4. **Hatchet**
   - [Tasks Documentation](https://docs.hatchet.run/home/basics/workflows)
   - [hatchet/examples/typescript/dag/workflow.ts](https://github.com/hatchet-dev/hatchet/blob/main/examples/typescript/dag/workflow.ts)
   - [hatchet/examples/typescript/child_workflows/workflow.ts](https://github.com/hatchet-dev/hatchet/blob/main/examples/typescript/child_workflows/workflow.ts)
   - [hatchet/examples/typescript/simple/workflow.ts](https://github.com/hatchet-dev/hatchet/blob/main/examples/typescript/simple/workflow.ts)
   - [hatchet-python/examples/dag/worker.py](https://github.com/hatchet-dev/hatchet-python/blob/main/examples/dag/worker.py)
   - [hatchet-python/examples/fanout/worker.py](https://github.com/hatchet-dev/hatchet-python/blob/main/examples/fanout/worker.py)

## Methodology

- Tools used: curl (direct page scraping), GitHub raw content API
- Pages analyzed: 18+
- Documentation versions: Current as of 2026-03-13
- Note: Flyte docs migrated to Union.ai (v2), older URL patterns return 404;
  examples sourced from flytesnacks GitHub repo instead.
- Note: Hatchet restructured their docs; DAG-specific pages returned 404;
  examples sourced from GitHub repos.

## Confidence Level

**High** -- All findings are sourced from official documentation and first-party
code examples. Temporal and Windmill documentation was comprehensive. Flyte and
Hatchet required GitHub source exploration due to documentation URL changes.
