# Research: CNCF Serverless Workflow DSL — Data Flow & Expression Model

**Date:** 2026-03-13
**Researcher:** Claude (research only)
**Spec Version:** DSL 1.0.3
**Sources:**
- https://github.com/serverlessworkflow/specification (main repo)
- `dsl.md` — DSL concepts document
- `dsl-reference.md` — Full reference (2,867 lines)
- `ctk/features/data-flow.feature` — Conformance tests
- `examples/` — 65+ example workflows

---

## 1. Executive Summary

The CNCF Serverless Workflow DSL v1.0.3 uses a **pipeline-based data flow model** where data passes sequentially between tasks, with explicit transformation points at input, output, and export stages. All data transformations use **JQ expressions** as the default (and mandatory) runtime expression language. The spec defines a comprehensive 11-step data flow pipeline with schema validation at 5 strategic points.

**Key differences from Nika's model:**

| Aspect | Serverless Workflow | Nika |
|--------|-------------------|------|
| Data binding | Pipeline (output of previous = input of next) | Explicit `use:` block with named bindings |
| Expression language | JQ (mandatory) | Handlebars-style `{{use.alias}}` templates |
| Transformation points | 3 per task (`input.from`, `output.as`, `export.as`) | None (raw pass-through) |
| Schema validation | JSON Schema at input/output/export boundaries | JSON Schema for StructuredOutput only |
| Context | Mutable `$context` shared across all tasks | Immutable DataStore |
| Flow control | `then` directives (continue/exit/end/named) | Explicit `flows:` section with DAG edges |

---

## 2. Data Flow Pipeline (The 11 Steps)

This is the core of the spec. Every workflow execution follows these steps exactly:

### 2.1 Workflow-Level (Steps 1-2, 10-11)

```
RAW WORKFLOW INPUT
    |
    v
[1] Validate against workflow input.schema  ---> ValidationError if invalid
    |
    v
[2] Transform via workflow input.from (JQ)  ---> Sets initial $context and $input
    |
    v
--- PASS TO FIRST TASK ---
    |
    v
    ... (task pipeline, repeated per task) ...
    |
    v
--- LAST TASK OUTPUT ---
    |
    v
[10] Transform via workflow output.as (JQ) ---> Final workflow output
    |
    v
[11] Validate against workflow output.schema ---> ValidationError if invalid
```

### 2.2 Task-Level (Steps 3-9, repeated for each task)

```
RAW TASK INPUT  (= transformed workflow input for first task,
                  OR transformed output of previous task)
    |
    v
[3] Evaluate task `if` condition (JQ)  ---> Skip task if false (raw input becomes output)
    |
    v
[4] Validate against task input.schema ---> ValidationError if invalid
    |
    v
[5] Transform via task input.from (JQ) ---> Sets $input for this task
    |
    v
[6] EXECUTE TASK (call/set/for/etc.)   ---> Produces raw task output
    |
    v
[7] Transform via task output.as (JQ)  ---> Sets $output for this task
    |
    v
[8] Validate against task output.schema ---> ValidationError if invalid
    |
    v
[9] Update context via task export.as (JQ) ---> Sets $context
    |
    v
[10] Validate against task export.schema ---> ValidationError if invalid
    |
    v
--- PASS TRANSFORMED OUTPUT TO NEXT TASK ---
```

### 2.3 Key Insight: Data Flows Forward, Context Flows Parallel

```
Task1 ----output.as----> Task2 ----output.as----> Task3
  |                        |                        |
  v                        v                        v
export.as               export.as               export.as
  |                        |                        |
  v                        v                        v
$context =============== $context =============== $context
(shared mutable state accessible by all tasks via JQ)
```

The data pipeline is a **chain**: each task's transformed output becomes the next task's raw input. The `$context` is a **separate mutable accumulator** that tasks can write to and read from independently of the data chain.

---

## 3. Input/Output/Export Definitions

### 3.1 Input

```yaml
input:
  schema:              # Optional JSON Schema for validation
    format: json
    document:          # Inline schema
      type: object
      properties:
        order:
          type: object
          required: [pet]
          properties:
            pet:
              type: object
              required: [id]
              properties:
                id:
                  type: string
  from: .order.pet     # JQ expression: filter/transform raw input
```

**Properties:**

| Property | Type | Required | Description |
|----------|------|----------|-------------|
| `schema` | Schema | no | JSON Schema to validate raw input BEFORE transformation |
| `from` | string/object | no | JQ expression to filter/transform. Defaults to identity (`.`) |

**Important:** `input.schema` validates the RAW input (before `input.from`). The `from` expression receives the already-validated data.

### 3.2 Output

```yaml
output:
  schema:
    format: json
    document:
      type: object
      properties:
        petId:
          type: string
      required: [petId]
  as:
    petId: '${ .pet.id }'    # JQ expression to transform raw task output
```

**Properties:**

| Property | Type | Required | Description |
|----------|------|----------|-------------|
| `schema` | Schema | no | JSON Schema to validate AFTER transformation |
| `as` | string/object | no | JQ expression to transform raw task output. Defaults to identity (`.`) |

**Important:** `output.schema` validates AFTER `output.as` is applied. The transformed result is what gets validated and passed to the next task.

### 3.3 Export

```yaml
export:
  schema:
    format: json
    document:
      type: object
  as: '$context + .'    # Merge task output into existing context
```

**Properties:**

| Property | Type | Required | Description |
|----------|------|----------|-------------|
| `schema` | Schema | no | JSON Schema to validate the new context |
| `as` | string/object | no | JQ expression evaluated on transformed output, produces new `$context`. Defaults to returning existing context unchanged |

**Key patterns for `export.as`:**

```yaml
# Merge output into context
as: '$context + .'

# Replace context entirely with output
as: '.'

# Extract specific field into context
as:
  homeworld: ${ .content.homeworld }

# Accumulate into array
as: '$context + { items: ($context.items + [.result]) }'
```

---

## 4. Runtime Expressions (JQ)

### 4.1 Expression Modes

The spec defines two evaluation modes via `evaluate.mode`:

| Mode | Syntax | Behavior |
|------|--------|----------|
| `strict` (default) | `${ expression }` | Expressions MUST be wrapped in `${ }`. Failure raises an error |
| `loose` | bare expression | Any value is evaluated. If evaluation fails, treated as a string literal |

```yaml
# Strict mode (default) - expressions must be wrapped
evaluate:
  mode: strict

do:
  - greet:
      set:
        greeting: ${ "Hello " + .user.name }    # Explicit expression
        literal: "This is just a string"          # Not evaluated

# Loose mode - everything is evaluated
evaluate:
  mode: loose

do:
  - greet:
      set:
        greeting: '"Hello " + .user.name'        # Evaluated as JQ
        literal: "This is just a string"          # Also evaluated (fails -> stays string)
```

### 4.2 Expression Language

JQ is the mandatory default. Runtimes MAY support alternatives via `evaluate.language`:

```yaml
evaluate:
  language: jq       # Default (mandatory support)
  mode: strict       # Default
```

### 4.3 Runtime Expression Arguments

Each expression has access to specific arguments depending on where it's used:

| Argument | Type | Description |
|----------|------|-------------|
| `$context` | map | Workflow's mutable context data |
| `$input` | any | Task's transformed input (result of `input.from`) |
| `$output` | any | Task's transformed output (result of `output.as`) |
| `$secrets` | map | Key/value map of secrets (restricted to `input.from` only) |
| `$task` | TaskDescriptor | Current task metadata (name, reference, definition, raw input/output) |
| `$workflow` | WorkflowDescriptor | Workflow metadata (id, definition, raw input, startedAt) |
| `$runtime` | RuntimeDescriptor | Runtime metadata (name, version) |
| `$authorization` | AuthorizationDescriptor | Resolved auth scheme/parameter |

### 4.4 Argument Availability Matrix

This table is CRITICAL for understanding what data is accessible where:

| Expression Location | Evaluated On | `$context` | `$input` | `$output` | `$secrets` |
|---------------------|-------------|:----------:|:--------:|:---------:|:----------:|
| Workflow `input.from` | Raw workflow input | - | - | - | YES |
| Task `if` | Raw task input | YES | - | - | YES |
| Task `input.from` | Raw task input | YES | - | - | YES |
| Task definition | Transformed task input | YES | YES | - | YES |
| Task `output.as` | Raw task output | YES | YES | - | YES |
| Task `export.as` | Transformed task output | YES | YES | YES | YES |
| Workflow `output.as` | Last task output | YES | - | - | YES |

**Key observations:**
- `$secrets` is available ONLY in `input.from` expressions (to prevent accidental leaking)
- `$input` is set AFTER `input.from` runs, so it's not available in `input.from` itself
- `$output` is only available in `export.as` (the last transformation step)
- The dot (`.`) in JQ always refers to the current data being evaluated

### 4.5 JQ Expression Examples from the Spec

```yaml
# Simple field access
input:
  from: .order.pet                    # Extract nested field

# Object construction
output:
  as:
    petId: '${ .pet.id }'             # Build new object from output

# String interpolation
body:
  message: "${ \"Executing task '\\($task.reference)'...\" }"

# Array manipulation
output:
  as: '$input + { availablePets: [.[] | select(.category.name == "dog")] }'

# Context merging
export:
  as: '$context + .'                  # Merge output into context

# Conditional filtering
for:
  in: '.pets | map(select(.status == "available"))'

# Accumulation
output:
  as: '.pets + [{ "id": $pet.id }]'   # Append to array

# Combining $input and current data
output:
  as: '{ ids: [ $input, .id ] }'      # Reference previous task output AND current
```

---

## 5. Schema Validation Model

### 5.1 Schema Definition

```yaml
schema:
  format: json                # Only "json" (JSON Schema) supported
  document:                   # Inline schema
    type: object
    properties:
      id:
        type: string
    required: [id]
  resource:                   # OR external schema
    endpoint: https://example.com/schema.json
```

### 5.2 Validation Points (5 total)

| # | Location | Validates | Against | On Failure |
|---|----------|-----------|---------|------------|
| 1 | Workflow input | Raw workflow input | `workflow.input.schema` | ValidationError (400) |
| 2 | Task input | Raw task input | `task.input.schema` | ValidationError (400) |
| 3 | Task output | Transformed task output | `task.output.schema` | ValidationError (400) |
| 4 | Task export | Updated context | `task.export.schema` | ValidationError (400) |
| 5 | Workflow output | Transformed workflow output | `workflow.output.schema` | ValidationError (400) |

**Validation order matters:**
- Input schemas validate BEFORE transformation (`input.from`)
- Output schemas validate AFTER transformation (`output.as`)
- Export schemas validate AFTER context update (`export.as`)

### 5.3 Standard Error Types

| Error Type URI | Status | When |
|----------------|--------|------|
| `.../errors/validation` | 400 | Schema validation failure |
| `.../errors/expression` | 400 | JQ expression evaluation failure |
| `.../errors/configuration` | 400 | Invalid config |
| `.../errors/authentication` | 401 | Auth failure |
| `.../errors/authorization` | 403 | Forbidden |
| `.../errors/timeout` | 408 | Timeout exceeded |
| `.../errors/communication` | 500 | Service call failure |
| `.../errors/runtime` | 500 | Generic runtime error |

Errors follow RFC 7807 Problem Details format:

```yaml
type: https://serverlessworkflow.io/spec/1.0.0/errors/validation
title: Invalid Input
status: 400
detail: Property 'pet.id' is required but missing
instance: /do/getPetById    # JSON Pointer to the failing task
```

---

## 6. Task Types and Their Data Behavior

### 6.1 `set` — Direct Data Assignment

Sets data without external calls. The value IS the output.

```yaml
- setShape:
    set:
      shape: circle
      size: ${ .configuration.size }
      fill: ${ .configuration.fill }
```

Can also be a pure expression:
```yaml
- initialize:
    set: ${ $workflow.input[0] }
```

### 6.2 `call` — Service Integration

Calls external services. The response IS the raw output.

```yaml
- getPet:
    call: http
    with:
      method: get
      endpoint: https://petstore.swagger.io/v2/pet/{petId}
    output:
      as: .id    # Transform: extract just the ID from the response
```

Supports: HTTP, gRPC, OpenAPI, AsyncAPI, A2A, MCP, and custom functions.

**HTTP output formats:**

| Format | Description |
|--------|-------------|
| `content` (default) | Deserialized response body |
| `response` | Full HTTP response object (status, headers, body) |
| `raw` | Base64-encoded response content |

### 6.3 `for` — Iteration

```yaml
- checkup:
    for:
      each: pet        # Variable name for current item (default: "item")
      in: .pets         # JQ expression returning the collection
      at: index         # Variable name for index (default: "index")
    while: .vet != null  # Continue condition (JQ)
    do:
      - waitForCheckup:
          set:
            result: ${ $pet.name }
```

The `each`/`at` variables become JQ variables (`$pet`, `$index`) accessible within the loop body.

### 6.4 `fork` — Parallel Execution

```yaml
- raiseAlarm:
    fork:
      compete: false    # false = collect all results as array
      branches:
        - callNurse:
            call: http
            with:
              method: put
              endpoint: https://hospital.com/api/alert/nurses
        - callDoctor:
            call: http
            with:
              method: put
              endpoint: https://hospital.com/api/alert/doctor
```

- `compete: false` (default) — Returns array of all branch outputs, preserving declaration order
- `compete: true` — Returns only the winning (first completed) branch's output

### 6.5 `switch` — Conditional Branching

```yaml
- processOrder:
    switch:
      - case1:
          when: .orderType == "electronic"     # JQ boolean expression
          then: processElectronicOrder         # Flow directive
      - case2:
          when: .orderType == "physical"
          then: processPhysicalOrder
      - default:
          then: handleUnknownOrderType         # No `when` = default case
```

### 6.6 `try`/`catch` — Error Handling

```yaml
- trySomething:
    try:
      - riskyCall:
          call: http
          with:
            method: get
            endpoint: https://unreliable.com/api
    catch:
      errors:
        with:
          type: https://serverlessworkflow.io/spec/1.0.0/errors/communication
          status: 503
      as: error          # Variable name for the caught error (default: "error")
      when: '$error.status == 503'    # Additional condition
      retry:
        delay:
          seconds: 3
        backoff:
          exponential: {}
        limit:
          attempt:
            count: 5
      do:                # Fallback tasks (executed if retries exhausted)
        - handleError:
            set:
              fallback: true
```

### 6.7 `run` — Process Execution

```yaml
# Run a shell command
- runShell:
    run:
      shell:
        command: echo "Hello"
        arguments:
          name: ${ .user.name }

# Run a container
- runContainer:
    run:
      container:
        image: my-processor:latest
        command: /bin/process

# Run a sub-workflow
- runSubWorkflow:
    run:
      workflow:
        namespace: test
        name: register-customer
        version: '0.1.0'
        input:
          customer: .user
```

---

## 7. Flow Directives

Control what happens after a task completes:

| Directive | Description |
|-----------|-------------|
| `continue` (default) | Execute next task in declaration order |
| `exit` | Complete current scope (may end workflow if in main `do`) |
| `end` | Gracefully end entire workflow |
| `<taskName>` | Jump to the named task within the same scope |

```yaml
do:
  - taskA:
      set:
        step: a
      then: taskC          # Skip taskB, jump to taskC

  - taskB:
      set:
        step: b
      then: end            # End workflow after this

  - taskC:
      set:
        step: c
      then: taskB          # Go back to taskB
```

**Constraint:** Flow directives can ONLY redirect to tasks within the same scope (same `do` block level).

---

## 8. Reusable Components

The `use` block defines reusable definitions:

```yaml
use:
  authentications:
    myAuth:
      oauth2: { ... }

  errors:
    notFound:
      type: https://example.com/errors/not-found
      status: 404

  functions:                  # Reusable task definitions
    getAvailablePets:
      call: openapi
      with:
        document:
          endpoint: https://petstore.swagger.io/v2/swagger.json
        operationId: findByStatus
        parameters:
          status: available

  retries:
    defaultRetry:
      delay:
        seconds: 3
      limit:
        attempt:
          count: 5

  secrets:
    - my-api-key
    - my-db-password

  timeouts:
    shortTimeout:
      after:
        seconds: 30
```

Functions are invoked by name:
```yaml
do:
  - getAvailablePets:
      call: getAvailablePets    # References use.functions.getAvailablePets
```

---

## 9. Extensions (Before/After Hooks)

Extensions inject logic around task execution:

```yaml
use:
  extensions:
    - logging:
        extend: all           # Apply to all tasks (or specific task type)
        when: '$task.name != "healthCheck"'   # Optional condition
        before:               # Tasks to run BEFORE the target task
          - sendLog:
              call: http
              with:
                method: post
                endpoint: https://logs.example.com
                body:
                  message: ${ "Starting task '\($task.reference)'" }
        after:                # Tasks to run AFTER the target task
          - sendLog:
              call: http
              with:
                method: post
                endpoint: https://logs.example.com
                body:
                  message: ${ "Completed task '\($task.reference)'" }
```

---

## 10. Complete Data Flow Example (Annotated)

This example from the CTK shows the full pipeline:

```yaml
document:
  dsl: '1.0.3'
  namespace: default
  name: data-flow-demo
  version: '1.0.0'

# STEP 1: Validate raw workflow input against this schema
input:
  schema:
    format: json
    document:
      type: object
      required: [id]
      properties:
        id:
          type: integer
          minimum: 1
  # STEP 2: Transform workflow input (identity by default)
  # from: .  (implicit)

do:
  # TASK 1
  - getCharacter:
      # STEP 3: if condition (not set, so task always runs)
      # STEP 4: validate task input (not set, no validation)
      # STEP 5: transform task input (not set, identity)
      call: http                                          # STEP 6: Execute
      with:
        method: get
        endpoint: https://swapi.dev/api/people/{id}
        output: response
      # STEP 7: output.as not set, raw HTTP response is the output
      # STEP 8: output.schema not set, no validation
      export:                                             # STEP 9: Update context
        as:
          homeworld: ${ .content.homeworld }
      # STEP 10: export.schema not set, no validation

  # TASK 2
  - getHomeworld:
      # Raw input = Task 1's transformed output (the full HTTP response)
      call: http                                          # STEP 6: Execute
      with:
        method: get
        endpoint: ${ $context.homeworld }                 # Uses $context set by Task 1

# STEP 10-11: Workflow output (no output.as, so last task's output passes through)
```

**Data flow trace:**

```
Workflow Input: { id: 1 }
    |
    v (validated, no transform)
Task 1 Input: { id: 1 }
    |
    v (HTTP call)
Task 1 Raw Output: { content: { name: "Luke", homeworld: "https://..." }, status: 200, ... }
    |
    v (no output.as, passes through)
Task 1 Transformed Output: { content: { name: "Luke", homeworld: "https://..." }, status: 200, ... }
    |
    v (export.as extracts homeworld into context)
$context: { homeworld: "https://swapi.dev/api/planets/1/" }
    |
Task 2 Input: { content: { name: "Luke", homeworld: "https://..." }, status: 200, ... }
    |
    v (HTTP call using $context.homeworld)
Task 2 Raw Output: { name: "Tatooine", climate: "arid", ... }
    |
    v (no output.as)
Workflow Output: { name: "Tatooine", climate: "arid", ... }
```

---

## 11. MCP Integration (New in 1.0.3)

The spec now includes native MCP (Model Context Protocol) support:

```yaml
- publishToSlack:
    call: mcp
    with:
      method: tools/call
      parameters:
        name: conversations_add_message
        arguments:
          channel_id: 'C1234567890'
          payload: 'Hello, world!'
      transport:
        stdio:
          command: npx
          arguments: [slack-mcp-server@latest, --transport, stdio]
          environment:
            SLACK_MCP_XOXP_TOKEN: ${ $secrets.slack_token }
```

Supported MCP methods: `tools/list`, `tools/call`, `prompts/list`, `prompts/get`, `resources/list`, `resources/read`, `resources/templates/list`.

---

## 12. Catalogs and Custom Functions

Functions can be published to catalogs and referenced by version:

```yaml
use:
  catalogs:
    global:
      endpoint:
        uri: https://github.com/serverlessworkflow/catalog

do:
  - log:
      call: log:0.5.2@global    # Function name:version@catalog
      with:
        message: "Hello from catalog function"
```

---

## 13. Comparison: Nika vs Serverless Workflow Data Flow

### 13.1 Data Passing

```yaml
# Serverless Workflow: Pipeline (implicit, output -> next input)
do:
  - step1:
      call: http
      with:
        endpoint: https://api.example.com/data
      output:
        as: .result                    # Transform output
  - step2:
      set:
        processed: ${ .result }        # Step 1's output is step 2's input (via `.`)

# Nika: Explicit bindings
tasks:
  - id: step1
    fetch:
      url: https://api.example.com/data
  - id: step2
    use:
      result: step1                    # Explicit reference
    infer: "Process: {{use.result}}"
```

### 13.2 Shared State

```yaml
# Serverless Workflow: Mutable $context
do:
  - step1:
      call: http
      with: { ... }
      export:
        as:
          homeworld: ${ .content.homeworld }    # Write to context
  - step2:
      call: http
      with:
        endpoint: ${ $context.homeworld }        # Read from context

# Nika: No shared mutable context (DataStore is immutable from task perspective)
# Tasks explicitly declare dependencies via use: blocks
```

### 13.3 Expression Power

```yaml
# Serverless Workflow: Full JQ power
output:
  as: '$input + { pets: [.[] | select(.category == "dog" and .age > 2)] }'

# Nika: Template interpolation only
infer: "Process these pets: {{use.pets}}"
# (No filtering, mapping, or complex transformations in-line)
```

### 13.4 Schema Validation

```yaml
# Serverless Workflow: 5 validation points per task
input:
  schema:
    format: json
    document:
      type: object
      required: [name]
output:
  schema:
    format: json
    document:
      type: string
export:
  schema:
    format: json
    document:
      type: object

# Nika: StructuredOutput on infer/agent tasks only
- id: generate
  infer:
    prompt: "Generate..."
  output:
    schema: { type: object, required: [title] }    # Enforced with retry/repair
```

---

## 14. Interesting Design Patterns

### 14.1 The "Star Wars Homeworld" Pattern (Context as Side-Channel)

The most instructive example: data flows forward via the pipeline, but intermediate results are stored in `$context` for later use by non-adjacent tasks.

### 14.2 The "Non-Object Output" Pattern

Task outputs can be any type (string, number, array), not just objects. The CTK tests this explicitly:

```yaml
- getPetById1:
    call: http
    with: { ... }
    output:
      as: .id           # Output is a number (not an object)
- getPetById2:
    call: http
    with: { ... }
    output:
      as: '{ ids: [ $input, .id ] }'    # Combine previous (number) with current
```

### 14.3 The "Export Merge" Pattern

Most common context update pattern:
```yaml
export:
  as: '$context + .'    # Merge task output into existing context
```

### 14.4 The "Conditional Skip" Pattern

Tasks with `if` that evaluate to false are skipped, and their raw input becomes their output:

```yaml
- conditionalStep:
    if: .customer.age >= 18    # Skip if underage
    call: http
    with: { ... }
```

---

## 15. Confidence Assessment

**Confidence Level: HIGH**

- Primary sources are the official spec documents from the GitHub repo
- CTK conformance tests validate the exact data flow behavior
- Examples are taken directly from the specification
- The DSL reference document is comprehensive (2,867 lines)
- The spec is at version 1.0.3, indicating stability

**Limitations:**
- Did not scrape the live website (serverlessworkflow.io) as the raw GitHub docs were more complete
- Runtime expression language alternatives to JQ are mentioned but not detailed in the spec
- The A2A and MCP call types are relatively new additions and may evolve

---

## 16. Sources

1. **dsl.md** — Main DSL concepts document (817 lines) — Data Flow section, Runtime Expressions section
2. **dsl-reference.md** — Complete reference (2,867 lines) — Input/Output/Export/Schema/Error/Retry definitions
3. **ctk/features/data-flow.feature** — Conformance tests for Input Filtering, Output Filtering, Non-object Output
4. **ctk/features/flow.feature** — Conformance tests for Implicit/Explicit Sequence Flow
5. **ctk/features/set.feature** — Conformance tests for Set task with JQ expressions
6. **examples/star-wars-homeworld.yaml** — Best example of export/context pattern
7. **examples/do-multiple.yaml** — Pipeline data flow between sequential tasks
8. **examples/for.yaml** — Loop iteration with JQ expressions
9. **examples/try-catch-retry-inline.yaml** — Error handling with retry
10. **README.md** — Project overview, ecosystem description
