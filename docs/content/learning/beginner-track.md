# Beginner Track -- Levels 1 through 4

> From zero to running multi-task AI workflows. No prior Nika experience required.

---

## Prerequisites

Before starting, ensure you have:

1. **Nika installed**: `brew install supernovae/tap/nika` (or from source)
2. **An LLM provider** (for Levels 4+): Set an API key for at least one provider
   ```bash
   export ANTHROPIC_API_KEY="sk-..."  # or OPENAI_API_KEY, etc.
   nika provider list                 # verify it is detected
   ```
3. **The course generated**:
   ```bash
   nika init --course
   cd nika-course/
   nika course status
   ```

---

## Level 1 -- Jailbreak

> *"They said AI was for them. You just broke out."*

### What You Will Learn

Level 1 teaches the fundamentals that every Nika workflow needs: schema declarations, task structure, and the first two verbs (`exec:` and `fetch:`). By the end, you can write, run, and validate workflows from scratch.

### Concept: The Anatomy of a Workflow

Every Nika workflow is a YAML file with three required parts:

```yaml
schema: "nika/workflow@0.12"    # 1. Schema declaration (always this)
workflow: my-first-workflow      # 2. Workflow name (human-readable)
tasks:                           # 3. Task list (the work to do)
  - id: hello
    exec: echo "Hello from Nika!"
```

That is a complete, runnable workflow. No framework, no SDK, no build step. Save it as `hello.nika.yaml` and run:

```bash
nika run hello.nika.yaml
```

**Why this matters**: Most automation tools require dozens of lines of boilerplate. Nika gets you running in 5 lines.

### Concept: The `exec:` Verb

The `exec:` verb runs shell commands. It captures stdout, stderr, and the exit code. Two forms are available:

**Shorthand** -- a single string command (runs without a shell, more secure):
```yaml
- id: list_files
  exec: "ls -la"
```

**Full form** -- an object with options:
```yaml
- id: system_info
  exec:
    command: "uname -s && whoami && date '+%Y-%m-%d'"
    shell: true        # Enable pipes and chaining
    timeout: 10        # Kill after 10 seconds
    env:               # Environment variables
      GREETING: "Nika"
    cwd: "/tmp"        # Working directory
```

The `shell: true` flag is required for pipes (`|`), chaining (`&&`), and variable expansion (`$VAR`). Without it, commands run directly, which is safer.

### Concept: The `fetch:` Verb

The `fetch:` verb makes HTTP requests. GET by default. No headers, no auth ceremony. URL in, data out.

**Shorthand**:
```yaml
- id: get_ip
  fetch: "https://httpbin.org/ip"
```

**Full form**:
```yaml
- id: post_data
  fetch:
    url: "https://httpbin.org/post"
    method: POST
    json:
      name: "Nika"
      version: "0.49"
    headers:
      Accept: "application/json"
    response: full     # Get status + headers + body
```

### Concept: Provider and Model

For tasks that use LLMs (`infer:` and `agent:`), you set a provider and model at the workflow level:

```yaml
schema: "nika/workflow@0.12"
workflow: with-llm
provider: anthropic
model: claude-sonnet-4-20250514
tasks:
  - id: think
    infer: "Explain recursion in one sentence."
```

You can override per task:
```yaml
- id: fast_task
  provider: groq
  model: llama-3.3-70b-versatile
  infer: "Quick answer: what is 2+2?"
```

### Exercise 1: Hello World

**Objective**: Write your first workflow with `infer:` in both shorthand and full form.

**What to do**: Open `01-jailbreak/01-hello-world.nika.yaml`. You will see TODO markers. Replace them with:
- A `schema:` declaration
- A `workflow:` name
- A task with `infer:` shorthand (just a string prompt)
- A task with `infer:` full form (`prompt:`, `system:`, `temperature:`, `max_tokens:`)
- A `depends_on:` between them

**Validate**: `nika check 01-jailbreak/01-hello-world.nika.yaml`
**Run**: `nika run 01-jailbreak/01-hello-world.nika.yaml`

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: hello-world
provider: anthropic
model: claude-sonnet-4-20250514

tasks:
  - id: hello
    infer: "Say hello to the world in 3 different languages. Keep it short and fun!"

  - id: hello_detailed
    depends_on: [hello]
    infer:
      prompt: "Now say goodbye to the world in 3 different languages. Be poetic!"
      system: "You are a multilingual poet who writes with warmth and elegance."
      temperature: 0.7
      max_tokens: 150
```
</details>

**Key takeaway**: Every workflow needs `schema:`, `workflow:`, and `tasks:`. The `infer:` verb has a shorthand (string) and full form (object).

### Exercise 2: Shell Commands

**Objective**: Use the `exec:` verb in both shorthand and full form.

**What to do**: Create four tasks:
1. `list_files` -- `exec:` shorthand with `ls -la`
2. `system_info` -- `exec:` full form with `shell: true`
3. `with_timeout` -- `exec:` full form with `timeout: 5`
4. `with_env` -- `exec:` with `env:` variables

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: shell-commands

tasks:
  - id: list_files
    exec: "ls -la"

  - id: system_info
    exec:
      command: "uname -s && whoami && date '+%Y-%m-%d'"
      shell: true

  - id: with_timeout
    exec:
      command: "echo 'Processing...'"
      shell: true
      timeout: 5

  - id: with_env
    exec:
      command: "echo \"Hello $GREETING from $LOCATION\""
      shell: true
      env:
        GREETING: "Nika"
        LOCATION: "the workflow engine"
```
</details>

**Key takeaway**: `shell: true` enables pipes and chaining. `timeout:` prevents commands from hanging. `env:` injects variables safely.

### Exercise 3: HTTP Requests

**Objective**: Use `fetch:` for GET, POST, headers, and JSONPath extraction.

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: http-requests

tasks:
  - id: get_ip
    fetch:
      url: "https://httpbin.org/ip"

  - id: post_data
    fetch:
      url: "https://httpbin.org/post"
      method: POST
      json:
        name: "Nika"
        version: "0.49"

  - id: with_headers
    fetch:
      url: "https://httpbin.org/get"
      headers:
        Accept: "application/json"
      response: full

  - id: extract_origin
    fetch:
      url: "https://httpbin.org/ip"
      extract: jsonpath
      selector: "$.origin"
```
</details>

**Key takeaway**: `fetch:` supports GET, POST, custom headers, full response mode, and JSONPath extraction -- all without curl.

### Exercises 4 and 5

Exercise 4 teaches provider/model configuration at workflow and task levels. Exercise 5 combines everything into a validated workflow with `nika check`. Complete both to unlock Level 2.

### Try It Yourself

Create a workflow that:
1. Fetches your public IP with `fetch:`
2. Runs `hostname` with `exec:`
3. Logs both results (for now, just use separate tasks)

---

## Level 2 -- Hot Wire

> *"Data flows where you tell it. Not where they sell it."*

### What You Will Learn

Level 2 introduces the data flow system: `with:` blocks, template syntax, JSONPath, and environment variable bindings. This is where isolated tasks become connected pipelines.

### Concept: Wiring Tasks Together with `with:`

In Level 1, each task was isolated. The `with:` block connects them by aliasing upstream task outputs:

```yaml
tasks:
  - id: get_data
    fetch: "https://httpbin.org/json"

  - id: process
    depends_on: [get_data]
    with:
      data: $get_data          # $ prefix = "output of this task"
    exec: echo "Got: {{with.data}}"
```

Three rules:
1. The `$` prefix means "output of this task"
2. The alias name (here, `data`) is your choice
3. Templates use `{{with.alias}}` to inject the value

### Concept: Reaching Into JSON with Dot Notation

APIs return nested JSON. You access nested fields with dot notation in templates:

```yaml
with:
  response: $api_call
# In templates:
# {{with.response.slideshow.title}}     -- nested object field
# {{with.response.items[0].name}}       -- array index + field
```

### Concept: Environment Variables

Secrets belong in the environment, not in YAML files. Use `$env.VAR` to reference them:

```yaml
tasks:
  - id: auth_request
    with:
      token: $env.API_TOKEN
    fetch:
      url: "https://api.example.com/data"
      headers:
        Authorization: "Bearer {{with.token}}"
```

### Exercise 1: Simple Binding

**Objective**: Wire two tasks together using `with:` and templates.

Create a workflow where:
1. Task `fetch_joke` fetches `https://httpbin.org/json`
2. Task `display` depends on `fetch_joke`, binds its output as `joke`, and echoes it with `{{with.joke}}`

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: simple-binding

tasks:
  - id: fetch_joke
    fetch: "https://httpbin.org/json"

  - id: display
    depends_on: [fetch_joke]
    with:
      joke: $fetch_joke
    exec:
      command: echo "Response = {{with.joke}}"
      shell: true
```
</details>

**Key takeaway**: `with:` creates aliases. `$task_id` references outputs. `{{with.alias}}` renders them.

### Exercise 2: Nested JSON

**Objective**: Access deeply nested fields in JSON responses.

The httpbin `/json` endpoint returns:
```json
{
  "slideshow": {
    "title": "Sample Slide Show",
    "slides": [
      { "title": "Wake up to WonderWidgets!", "type": "all" }
    ]
  }
}
```

Create a workflow that extracts the slideshow title and the first slide's title using dot notation.

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: nested-json

tasks:
  - id: fetch_slides
    fetch: "https://httpbin.org/json"

  - id: extract_title
    depends_on: [fetch_slides]
    with:
      slides: $fetch_slides
    exec:
      command: |
        echo "Show: {{with.slides.slideshow.title}}"
        echo "First slide: {{with.slides.slideshow.slides[0].title}}"
      shell: true
```
</details>

**Key takeaway**: Dot notation navigates objects, bracket notation navigates arrays.

### Exercise 3: Transforms

**Objective**: Apply pipe transforms to clean and reshape data.

Use `| trim` to strip whitespace, `| uppercase` to convert case, and `| length` to count characters.

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: transforms

tasks:
  - id: get_name
    exec: "echo '  Nika Workflow Engine  '"

  - id: clean
    depends_on: [get_name]
    with:
      raw: $get_name
    exec:
      command: |
        echo "Trimmed: {{with.raw | trim}}"
        echo "Upper: {{with.raw | trim | uppercase}}"
        echo "Length: {{with.raw | trim | length}}"
      shell: true
```
</details>

**Key takeaway**: Pipe transforms (`|`) chain left to right. Each transforms the result of the previous one.

### Exercise 4: Env Bindings

**Objective**: Inject environment variables into workflows without hardcoding secrets.

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: env-bindings

tasks:
  - id: show_env
    with:
      user: $env.USER
      home: $env.HOME
    exec:
      command: |
        echo "User: {{with.user}}"
        echo "Home: {{with.home}}"
      shell: true
```
</details>

**Key takeaway**: `$env.VAR` keeps secrets out of YAML files. The environment is the source of truth for credentials.

---

## Level 3 -- Fork Bomb

> *"One task? Cute. Try a thousand."*

### What You Will Learn

Level 3 reveals the execution model: the DAG. You will learn why tasks without dependencies run in parallel, how to design dependency graphs, and how `for_each:` fans work out across lists.

### Concept: The DAG Execution Model

Every Nika workflow is compiled into a Directed Acyclic Graph (DAG). The scheduler:
1. Identifies all tasks with no unmet dependencies
2. Runs them in parallel
3. When a task completes, checks if new tasks are unblocked
4. Repeats until all tasks are done

```yaml
tasks:
  - id: fetch_users
    fetch: "https://api.example.com/users"

  - id: fetch_products
    fetch: "https://api.example.com/products"

  - id: merge
    depends_on: [fetch_users, fetch_products]
    exec: echo "Both done"
```

`fetch_users` and `fetch_products` run simultaneously because neither depends on the other. `merge` waits for both.

### Concept: The Diamond Pattern

The most powerful DAG shape -- fan out, then fan in:

```
        [start]
        /      \
   [task_a]  [task_b]
        \      /
        [merge]
```

This pattern is everywhere in real workflows: fetch from 3 APIs in parallel, then combine results.

### Concept: For Each

The `for_each:` field iterates over a list, executing the task once per item:

```yaml
- id: translate
  for_each: ["English", "French", "German"]
  concurrency: 3
  infer:
    prompt: "Translate 'Hello world' to {{with.item}}"
```

Each iteration runs as a separate instance. `concurrency:` limits how many run at once.

### Exercise 1: Parallel Diamond

**Objective**: Build a diamond-shaped DAG with fan-out and fan-in.

Create a workflow with:
1. A `start` task that echoes "Beginning"
2. Two parallel tasks (`path_a` and `path_b`) that depend on `start`
3. A `merge` task that depends on both parallel tasks

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: parallel-diamond

tasks:
  - id: start
    exec: echo "Beginning the diamond"

  - id: path_a
    depends_on: [start]
    exec: echo "Path A processing..."

  - id: path_b
    depends_on: [start]
    exec: echo "Path B processing..."

  - id: merge
    depends_on: [path_a, path_b]
    with:
      a_result: $path_a
      b_result: $path_b
    exec:
      command: |
        echo "Path A said: {{with.a_result}}"
        echo "Path B said: {{with.b_result}}"
      shell: true
```
</details>

**Key takeaway**: Tasks without mutual dependencies run in parallel automatically. `depends_on:` lists create fan-in merge points.

### Exercise 2: For Each Basic

**Objective**: Iterate over a list with `for_each:`.

Create a workflow that greets 4 different names using `for_each:`.

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: for-each-basic

tasks:
  - id: greet
    for_each: ["Alice", "Bob", "Charlie", "Diana"]
    exec:
      command: echo "Hello, {{with.item}}!"
      shell: true
```
</details>

**Key takeaway**: `for_each:` turns one task definition into multiple parallel executions. `{{with.item}}` references the current element.

### Exercise 3: For Each Concurrent

**Objective**: Iterate over a list with concurrency limits.

When processing a large list, you may not want all iterations running at once. The `concurrency:` field limits how many run simultaneously.

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: for-each-concurrent

tasks:
  - id: process
    for_each: ["https://httpbin.org/delay/1", "https://httpbin.org/delay/1", "https://httpbin.org/delay/1", "https://httpbin.org/delay/1"]
    concurrency: 2
    fetch:
      url: "{{with.item}}"
      timeout: 10
```
</details>

**Key takeaway**: `concurrency: 2` means at most 2 iterations run at once. This prevents overwhelming APIs with too many simultaneous requests.

### Exercise 4: Chained Pipeline

**Objective**: Build a multi-stage pipeline with complex dependencies.

Create a workflow with 5+ tasks forming a pipeline:
1. Two tasks run in parallel (fetch from two APIs)
2. A merge task combines both results
3. A processing task transforms the merged data
4. A final task writes the result

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: chained-pipeline

tasks:
  - id: fetch_a
    fetch: "https://httpbin.org/json"

  - id: fetch_b
    fetch: "https://httpbin.org/ip"

  - id: get_time
    exec: "date '+%H:%M:%S'"

  - id: merge
    depends_on: [fetch_a, fetch_b, get_time]
    with:
      json_data: $fetch_a
      ip_data: $fetch_b
      timestamp: $get_time
    exec:
      command: |
        echo "=== Pipeline Report ==="
        echo "Timestamp: {{with.timestamp | trim}}"
        echo "JSON source: {{with.json_data | length}} chars"
        echo "IP data: {{with.ip_data | trim}}"
      shell: true

  - id: save
    depends_on: [merge]
    with:
      report: $merge
    invoke:
      tool: "nika:write"
      params:
        file_path: ".scratch/pipeline-report.txt"
        content: "{{with.report}}"
```
</details>

**Key takeaway**: Real workflows combine parallel fetches, sequential processing, and output writing. The DAG handles orchestration automatically.

### Try It Yourself

Build a workflow that:
1. Fetches data from 3 different public APIs in parallel
2. Merges all results into a single summary task
3. Uses `with:` bindings to access each API's response

**Challenge**: Add a `for_each:` task that iterates over 5 URLs and fetches each one with `concurrency: 2`.

### Why DAGs Matter

Consider a sequential approach:
```
Task 1 (500ms) --> Task 2 (500ms) --> Task 3 (500ms) --> Task 4 (500ms)
Total: 2000ms
```

With a DAG:
```
Task 1 (500ms) ─┐
Task 2 (500ms) ─┤──> Task 4 (500ms)
Task 3 (500ms) ─┘
Total: 1000ms (50% faster)
```

The more parallel branches you have, the bigger the speedup. This is why DAGs are strictly superior to sequential scripts for I/O-bound work like API calls.

---

## Level 4 -- Root Access

> *"Their walled gardens? Your open fields."*

### What You Will Learn

Level 4 introduces workflow parameterization: context files, imports, and inputs. These features make workflows reusable and configurable without editing YAML.

### Concept: Context Files

External files can be loaded into a task's context, allowing you to inject large prompts, data files, or schemas without bloating the YAML.

### Concept: Imports

The `imports:` block lets you share definitions across multiple workflows. Common configurations, task templates, and shared schemas can live in separate files and be imported where needed.

### Concept: Inputs

The `inputs:` block declares parameters with default values that can be overridden from the CLI:

```yaml
schema: "nika/workflow@0.12"
workflow: configurable-report

inputs:
  target_url: "https://example.com"
  depth: 3
  output_format: "markdown"

tasks:
  - id: scrape
    fetch:
      url: "{{inputs.target_url}}"
      extract: "{{inputs.output_format}}"
```

Override at runtime:
```bash
nika run configurable-report.nika.yaml --input target_url=https://other.com
```

### Exercise 1: Context Files

**Objective**: Load external content into a workflow task.

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: context-files
provider: anthropic
model: claude-sonnet-4-20250514

tasks:
  - id: load_context
    exec: "cat README.md"

  - id: summarize
    depends_on: [load_context]
    with:
      content: $load_context
    infer:
      prompt: "Summarize this document:\n{{with.content}}"
      max_tokens: 300
```
</details>

### Exercise 2: Imports

**Objective**: Share definitions across workflows using `imports:`.

### Exercise 3: Inputs

**Objective**: Create a parameterized workflow with CLI-overridable inputs.

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: parameterized

inputs:
  greeting: "Hello"
  target: "World"
  repeat: 3

tasks:
  - id: greet
    exec:
      command: |
        for i in $(seq 1 {{inputs.repeat}}); do
          echo "{{inputs.greeting}}, {{inputs.target}}! (iteration $i)"
        done
      shell: true
```

Run with custom inputs:
```bash
nika run 03-inputs.nika.yaml --input greeting=Bonjour --input target=Nika
```
</details>

**Key takeaway**: `inputs:` makes workflows reusable. Users configure behavior without editing YAML.

---

## Phase 1 Checkpoint

After completing Levels 1-4, you should be able to:

- Write a workflow from scratch with the correct schema declaration
- Use all three non-agent verbs: `exec:`, `fetch:`, `infer:`
- Wire task outputs together with `with:` bindings and templates
- Access nested JSON with dot notation and array indexing
- Build parallel DAG patterns with `depends_on:`
- Iterate over lists with `for_each:`
- Create parameterized workflows with `inputs:`
- Validate workflows before running them with `nika check`
- Apply pipe transforms for inline data transformation

### Checkpoint Project: System Health Dashboard

Build a workflow that:
1. Checks disk usage with `exec: df -h`
2. Checks memory with `exec: vm_stat` (macOS) or `exec: free -m` (Linux)
3. Pings a URL with `fetch: https://httpbin.org/ip`
4. All three checks run in parallel (no `depends_on` between them)
5. A summary task depends on all three and uses `with:` bindings to format a report
6. The workflow accepts an `inputs:` parameter for the URL to ping

```bash
nika run system-health.nika.yaml --input url=https://api.github.com
```

This project validates all Phase 1 skills in a single, practical workflow.

---

*"You broke out of the click-and-pray GUI. You write YAML now. Welcome to the other side."*
