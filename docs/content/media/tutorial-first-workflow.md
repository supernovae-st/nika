# Tutorial: Your First Nika Workflow in 5 Minutes

> 15-minute tutorial video
> Target audience: developers new to Nika, familiar with YAML and APIs
> Format: screen recording with voice-over, terminal + editor split screen
> Resolution: 1920x1080 minimum, terminal font JetBrains Mono 16pt

---

## Pre-Recording Setup

- Clean terminal (no history, minimal prompt)
- VS Code with Nika LSP extension installed
- Environment variables set for at least one LLM provider (ANTHROPIC_API_KEY recommended)
- Working internet connection for fetch examples
- Empty project directory ready

---

## CHAPTER 1: Installation (0:00 - 1:30)

### Scene 1.1 -- Title Card (0:00 - 0:10)

[SCREEN] Black background. Title fades in: "Your First Nika Workflow in 5 Minutes"
[SUBTITLE] "A step-by-step tutorial for the Nika workflow engine"

**Voice-over:**
Welcome to Nika. In this tutorial, we will go from zero to running your first AI workflow. By the end, you will have written three workflows, used three of the five verbs, and seen the TUI in action. Let's go.

### Scene 1.2 -- Installing Nika (0:10 - 0:50)

[SCREEN] Terminal, clean prompt. Cursor blinking.

**Voice-over:**
First, let's install Nika. If you have Rust installed, it is one command.

[TYPE] `cargo install nika`

[SCREEN] Show cargo compilation output (speed up to 4x). Highlight the final "Installed" line.

**Voice-over:**
Nika compiles from source. This takes about two minutes on a modern machine. If you prefer a pre-built binary, check the GitHub releases page -- we provide binaries for macOS, Linux, and Windows.

[CUT TO] Installation complete.

[TYPE] `nika --version`

[SCREEN] Output: `nika 0.42.0`

**Voice-over:**
Version 0.42.0. Ten crates compiled into a single static binary. Let's verify everything is working.

### Scene 1.3 -- Verify Setup (0:50 - 1:30)

[TYPE] `nika provider list`

[SCREEN] Output showing provider status -- green checkmarks for configured providers, red X for unconfigured ones.

**Voice-over:**
`nika provider list` shows which LLM providers are available. You need at least one for the infer verb. I have Anthropic configured -- that means Claude is ready. If you see red marks, set the appropriate environment variable. For Anthropic, that is `ANTHROPIC_API_KEY`. For OpenAI, `OPENAI_API_KEY`. Nika supports eight providers -- use whichever you prefer.

[SCREEN] If needed, show: `export ANTHROPIC_API_KEY=sk-ant-...`

**Voice-over:**
If you do not have an API key yet, do not worry. The first two workflows we build will use exec and fetch -- no LLM required.

---

## CHAPTER 2: Your First Workflow -- exec: (1:30 - 4:00)

### Scene 2.1 -- Create the Project (1:30 - 2:00)

[TYPE] `mkdir nika-tutorial && cd nika-tutorial`

**Voice-over:**
Let's create a project directory. Nika does not require any project structure -- you just need a `.nika.yaml` file. But let's be organized.

[SCREEN] Switch to VS Code, open the `nika-tutorial` directory.

[TYPE] Create a new file: `hello.nika.yaml`

**Voice-over:**
Create a new file. The extension is `.nika.yaml` -- not just `.yaml`. This extension tells the LSP to activate, and it tells Nika this is a workflow file.

### Scene 2.2 -- The Schema Line (2:00 - 2:30)

[SCREEN] VS Code editor, empty file.

[TYPE] Line by line:
```yaml
schema: nika/workflow@0.12
```

[ZOOM] On the editor gutter -- show the LSP validation indicator (green checkmark or no errors).

**Voice-over:**
Every Nika workflow starts with a schema declaration. `nika/workflow@0.12` -- this is the current schema version. It is not decorative. This line activates the full validation pipeline: the three-phase AST parser will use this to check your workflow structure, the DAG validator will verify your dependencies, and the LSP will give you completions and diagnostics.

Watch the editor -- no red squiggles. The schema line is valid. You can also use `nika check` to validate without executing.

### Scene 2.3 -- Adding a Task (2:30 - 3:15)

[TYPE] Continue in the editor:
```yaml

tasks:
  - id: hello
    exec:
      command: echo "Hello from Nika!"
```

[ZOOM] On the `id:` field. Then on the `exec:` verb. Then on the `command:` field.

**Voice-over:**
Now we add a task. Every task needs an `id` -- a unique identifier within the workflow. Then a verb. We are using `exec:` -- the simplest verb. It runs a shell command. The `command:` field takes any shell command you would type in your terminal.

Let me point out what is NOT here. No imports. No boilerplate. No class instantiation. No callback registration. Five lines of YAML. That is a complete workflow.

### Scene 2.4 -- Running It (3:15 - 3:45)

[SCREEN] Switch to terminal (or use integrated terminal).

[TYPE] `nika check hello.nika.yaml`

[SCREEN] Output shows validation results:
```
  Workflow: hello.nika.yaml
  Schema:   nika/workflow@0.12
  Tasks:    1
  DAG:      valid (no cycles)
  Status:   OK
```

**Voice-over:**
Before running, let's validate. `nika check` runs the full analysis pipeline without executing anything. Schema validation, DAG cycle detection, binding resolution, security checks -- all verified ahead of time. Everything is green.

[TYPE] `nika run hello.nika.yaml`

[SCREEN] Output:
```
  hello  exec  "Hello from Nika!"  [2ms]
```

**Voice-over:**
And there it is. `nika run` executes the workflow. The task ran in two milliseconds. The output is the echo result. One task, one verb, done.

### Scene 2.5 -- A Multi-Task Workflow (3:45 - 4:00)

[SCREEN] Back in VS Code. Edit the file:

[TYPE] Add more tasks:
```yaml
  - id: date
    exec:
      command: date +"%Y-%m-%d"

  - id: hostname
    exec:
      command: hostname
```

**Voice-over:**
Let's add two more tasks. Date and hostname. Notice there is no `depends_on:` -- these three tasks have no dependencies between them. Watch what happens when we run.

[TYPE] `nika run hello.nika.yaml`

[SCREEN] Output shows all three tasks running:
```
  hello     exec  "Hello from Nika!"  [1ms]
  date      exec  "2026-03-23"        [2ms]
  hostname  exec  "macbook-pro"       [1ms]
```

**Voice-over:**
All three ran. And here is the key: since no task depends on another, Nika ran them in parallel. The DAG had three root nodes with no edges -- maximum parallelism, automatic. You did not configure thread pools or async decorators. The engine figured it out.

---

## CHAPTER 3: Data Flow with fetch: (4:00 - 7:30)

### Scene 3.1 -- Fetching Data (4:00 - 5:00)

[SCREEN] Create a new file: `weather.nika.yaml`

[TYPE]
```yaml
schema: nika/workflow@0.12

tasks:
  - id: get_weather
    fetch:
      url: https://wttr.in/Paris?format=j1
```

**Voice-over:**
The second verb: `fetch:`. This makes HTTP requests. We are hitting the wttr.in weather API for Paris. No headers needed for this API, but you can set method, headers, body, timeout -- everything you would expect from an HTTP client.

[TYPE] `nika run weather.nika.yaml`

[SCREEN] Output shows the raw JSON response from the weather API.

**Voice-over:**
Raw JSON. Useful, but verbose. We got the entire API response. Let's extract just what we need.

### Scene 3.2 -- Extraction Modes (5:00 - 5:45)

[SCREEN] Edit the file:

[TYPE] Add `extract:` and `selector:`:
```yaml
  - id: get_weather
    fetch:
      url: https://wttr.in/Paris?format=j1
      extract: jsonpath
      selector: "$.current_condition[0].weatherDesc[0].value"
```

[ZOOM] On the `extract: jsonpath` line.

**Voice-over:**
Nika has nine extraction modes built into the fetch verb. `jsonpath` lets you query JSON responses with JSONPath syntax. We are extracting just the weather description. Other modes include `markdown` for converting HTML to clean Markdown, `article` for Readability-style article extraction, `metadata` for OpenGraph and Twitter Cards, `links` for classified link extraction, and `feed` for RSS and Atom parsing. One field, nine modes.

[TYPE] `nika run weather.nika.yaml`

[SCREEN] Output: `"Partly cloudy"`

**Voice-over:**
Clean, extracted data. Just the weather description string.

### Scene 3.3 -- Chaining Tasks with depends_on and with: (5:45 - 7:00)

[SCREEN] Continue editing `weather.nika.yaml`:

[TYPE] Add a second task:
```yaml
  - id: report
    depends_on: [get_weather]
    with: { weather: $get_weather }
    exec:
      command: echo "The weather in Paris is {{with.weather}}"
```

[ZOOM] On `depends_on:` -- draw attention to the array syntax.
[ZOOM] On `with:` -- highlight the `$get_weather` reference.
[ZOOM] On `{{with.weather}}` -- highlight the template syntax.

**Voice-over:**
Now the magic of data flow. Three concepts working together. First, `depends_on:` creates a DAG edge -- the report task will not start until get_weather completes. Second, `with:` creates a named binding -- we are giving the result of `$get_weather` the alias `weather`. Dollar sign prefix means "reference a task result." Third, `{{with.weather}}` is a template that resolves at runtime. When the report task runs, the engine replaces this template with the actual weather data.

This is not string concatenation. The binding is validated at analysis time. If you misspell the task ID, Nika catches it before execution. If you reference a task that does not depend on, you get an error. The data flow is checked.

[TYPE] `nika run weather.nika.yaml`

[SCREEN] Output:
```
  get_weather  fetch  "Partly cloudy"                          [380ms]
  report       exec   "The weather in Paris is Partly cloudy"  [2ms]
```

**Voice-over:**
The fetch ran first. The result flowed into the report task through the binding. Clean, ordered, validated. This is a two-task pipeline, but the same pattern scales to fifty tasks with complex dependency graphs.

### Scene 3.4 -- Pipe Transforms (7:00 - 7:30)

[SCREEN] Edit the template in the report task:

[TYPE] Change to:
```yaml
      command: echo "WEATHER UPDATE - {{with.weather | uppercase | trim}}"
```

[ZOOM] On `| uppercase | trim`

**Voice-over:**
Pipe transforms let you process data inline. `uppercase` converts to uppercase, `trim` removes whitespace. There are twenty-seven transforms available -- `lowercase`, `reverse`, `word_count`, `truncate`, `base64_encode`, and more. Chain as many as you need with the pipe operator.

[TYPE] `nika run weather.nika.yaml`

[SCREEN] Output: `"WEATHER UPDATE - PARTLY CLOUDY"`

---

## CHAPTER 4: AI Generation with infer: (7:30 - 11:00)

### Scene 4.1 -- The Third Verb (7:30 - 8:30)

[SCREEN] Create a new file: `poet.nika.yaml`

[TYPE]
```yaml
schema: nika/workflow@0.12

tasks:
  - id: get_weather
    fetch:
      url: https://wttr.in/Paris?format=j1
      extract: jsonpath
      selector: "$.current_condition[0]"

  - id: poem
    depends_on: [get_weather]
    with: { conditions: $get_weather }
    infer:
      model: claude/claude-haiku-3-5-20241022
      prompt: |
        Current weather data for Paris:
        {{with.conditions}}

        Write a haiku about this weather.
```

[ZOOM] On `infer:` -- the third verb.
[ZOOM] On `model:` -- the provider/model syntax.

**Voice-over:**
Now we bring in the LLM. The `infer:` verb generates text using any configured provider. The `model:` field uses a slash syntax -- provider slash model name. Here we are using Claude Haiku for speed and cost efficiency. The prompt uses our template bindings to inject the weather data. Multi-line YAML strings with the pipe character give us clean prompt formatting.

[TYPE] `nika run poet.nika.yaml`

[SCREEN] Output shows:
```
  get_weather  fetch  {...weather data...}    [350ms]
  poem         infer  "Gray clouds hang low    [890ms]
                       Rain whispers to cobbles
                       Paris dreams in mist"
```

**Voice-over:**
Fetch, then infer. The weather data flowed into the prompt, Claude wrote a haiku, and we have a three-task pipeline: HTTP request, data extraction, AI generation. Total time under two seconds.

### Scene 4.2 -- Adding Structure (8:30 - 9:30)

[SCREEN] Edit `poet.nika.yaml`, add output schema:

[TYPE] Add to the poem task:
```yaml
      output:
        format: json
        schema:
          type: object
          properties:
            haiku:
              type: string
              description: "The haiku poem"
            mood:
              type: string
              enum: [serene, dramatic, playful, melancholy]
            season:
              type: string
          required: [haiku, mood, season]
```

[ZOOM] On the `output:` block.

**Voice-over:**
What if we need structured data, not just text? The `output:` block defines a JSON schema. Nika will ensure the LLM response conforms to this structure. Not by hoping -- by enforcing. Remember the five-layer defense system? Provider-native structured output, extraction, validation, retry, and LLM repair. You define the schema, Nika guarantees the shape.

[TYPE] `nika run poet.nika.yaml`

[SCREEN] Output shows structured JSON:
```json
{
  "haiku": "Gray clouds hang low\nRain whispers to cobbles\nParis dreams in mist",
  "mood": "melancholy",
  "season": "spring"
}
```

**Voice-over:**
Valid JSON. Conforming to our schema. The haiku, a mood from our enum, and a season. This output can flow into the next task as structured data -- no parsing, no regex extraction. Clean data pipelines.

### Scene 4.3 -- System Prompts and Parameters (9:30 - 10:15)

[SCREEN] Show additional infer options:

[TYPE] Add to the infer block:
```yaml
      system: You are a poetry expert specializing in weather haiku.
      temperature: 0.8
      max_tokens: 200
```

**Voice-over:**
The infer verb supports all the parameters you would expect. `system:` for system prompts. `temperature:` for creativity control. `max_tokens:` for length limits. Plus advanced options like extended thinking for Claude, vision content for multimodal inputs, and guardrails for output validation. All in YAML. No SDK calls, no builder patterns, no configuration objects.

### Scene 4.4 -- Multiple Models (10:15 - 11:00)

[SCREEN] Show a workflow with different models:

[TYPE] Show example:
```yaml
  - id: quick_summary
    infer:
      model: groq/llama-3.3-70b-versatile
      prompt: "Summarize this in one line: {{with.data}}"

  - id: deep_analysis
    infer:
      model: claude/claude-sonnet-4-20250514
      prompt: "Analyze this in depth: {{with.data}}"

  - id: local_check
    infer:
      model: native/mistral-7b
      prompt: "Verify this fact: {{with.data}}"
```

**Voice-over:**
Different tasks, different models. Fast summary on Groq. Deep analysis on Claude Sonnet. Local fact-checking on a native GGUF model running entirely on your machine. No code changes between providers. Just change the model line. Nika abstracts the provider differences through rig-core, giving you a unified interface across eight providers and hundreds of models.

---

## CHAPTER 5: The TUI (11:00 - 13:00)

### Scene 5.1 -- Launching the TUI (11:00 - 11:30)

[SCREEN] Terminal.

[TYPE] `nika ui`

[SCREEN] The TUI launches. Home view appears with file browser on the left and preview on the right.

**Voice-over:**
The Nika TUI is not a simple log viewer. It is a full terminal application with three views, forty-plus widgets, and ninety-two thousand lines of ratatui code. Let me show you around.

### Scene 5.2 -- Home View (11:30 - 12:00)

[SCREEN] Navigate the Home view.

**Demo Script:**
1. Arrow keys to browse files in the left panel
2. Select `poet.nika.yaml` -- show preview on the right
3. [ZOOM] on the preview showing task list, verb types, dependency count

**Voice-over:**
The Home view is your project browser. File tree on the left, workflow preview on the right. You can see the tasks, verbs, and dependencies at a glance. Select a workflow and press Enter to open it in the Studio.

### Scene 5.3 -- Studio View (12:00 - 12:30)

[SCREEN] Press `1` or `s` to switch to Studio view.

**Demo Script:**
1. Show the YAML editor with syntax highlighting
2. Type a few characters -- show LSP completions appearing
3. Introduce an error -- show inline diagnostic
4. Fix the error -- show diagnostic clear

**Voice-over:**
Studio view. A full YAML editor running in your terminal. Syntax highlighting via tree-sitter. LSP completions for all Nika schema elements. Inline diagnostics that appear as you type. You can write and validate workflows without leaving the terminal.

### Scene 5.4 -- Monitor View (12:30 - 13:00)

[SCREEN] Run a workflow and switch to Monitor view.

**Demo Script:**
1. Show the live DAG visualization with colored nodes
2. [ZOOM] on a running task node -- show progress bar, token count, timing
3. Show the output panel with streaming LLM output
4. Show the mission control panel with overall progress

**Voice-over:**
Monitor view. This is mission control. The DAG renders live -- green for completed tasks, blue for running, gray for pending. Each node shows timing, token counts, and status. The output panel streams LLM responses in real time. The mission control panel tracks overall progress, cost, and timing. When you are running a complex workflow with twenty tasks hitting three different providers, this is where you watch it happen.

---

## CHAPTER 6: Next Steps (13:00 - 15:00)

### Scene 6.1 -- The Course (13:00 - 13:45)

[SCREEN] Terminal.

[TYPE] `nika init --course`

[SCREEN] Show the course generation output -- 12 levels, 44 exercises created.

[TYPE] `nika course status`

[SCREEN] Show the constellation progress map.

**Voice-over:**
Want to go deeper? Nika has a built-in interactive course. Twelve levels, forty-four exercises. From Jailbreak -- basic exec commands -- to SuperNovae -- full production orchestration. `nika course status` shows your progress as a constellation map. `nika course next` opens your next exercise. `nika course check` validates your solutions. `nika course hint` gives you progressive hints -- three tiers of help before showing the answer.

[TYPE] `nika course next`

[SCREEN] Show the first exercise opening.

### Scene 6.2 -- The Showcase (13:45 - 14:15)

[TYPE] `nika showcase list`

[SCREEN] Show the scrolling list of 200+ showcase workflows.

**Voice-over:**
If you learn by example, the showcase has over two hundred working workflows. `nika showcase list` shows them all, organized by category. `nika showcase extract weather-report` pulls a specific workflow into your project, ready to run.

[TYPE] `nika showcase extract weather-report`

[SCREEN] Show the file being created.

### Scene 6.3 -- The Two Missing Verbs (14:15 - 14:45)

[SCREEN] Show code examples of invoke and agent:

[TYPE] Show side-by-side:
```yaml
# invoke: -- MCP tool calls
- id: search
  invoke:
    tool: novanet_search
    input:
      query: "weather patterns"

# agent: -- multi-turn loops
- id: researcher
  agent:
    model: claude/claude-sonnet-4-20250514
    goal: "Research weather prediction methods"
    tools: [web_search, file_read]
    max_turns: 10
```

**Voice-over:**
We covered three verbs today -- exec, fetch, and infer. The remaining two are invoke for MCP tool calls and agent for multi-turn agentic loops. Invoke connects Nika to any MCP-compatible tool server. Agent creates autonomous loops where the LLM decides which tools to call and when to stop. These are covered in levels eight and ten of the course.

### Scene 6.4 -- Closing (14:45 - 15:00)

[SCREEN] Show three resources:

```
github.com/supernovae-st/nika     -- Source code
nika init --course                 -- Interactive learning
nika showcase list                 -- 200+ examples
```

**Voice-over:**
That is your first Nika workflow. We went from installation to a three-task AI pipeline in under five minutes. We used three of the five verbs. We saw data flow through typed bindings. We explored the TUI. And we barely scratched the surface -- structured output, media tools, MCP integration, agent loops, and the full course are waiting.

The source code is on GitHub. The course is built in. The showcase has hundreds of examples. Go build something.

[SCREEN] Title card: "Nika -- AI Workflows, Liberated" with GitHub URL.

---

## B-Roll Shot List

| Timestamp | Shot | Purpose |
|-----------|------|---------|
| 0:00-0:10 | Butterfly animation | Title card |
| 1:30 | Close-up of terminal | Transition to chapter 2 |
| 4:00 | Code flowing between tasks (animated) | Data flow concept |
| 7:30 | Brain icon activating | Transition to LLM chapter |
| 11:00 | TUI full-screen capture | Showcase the interface |
| 14:45 | Butterfly logo dissolve | Closing |

## Post-Production Notes

- Speed up installation compilation to 4x
- Add subtle sound effects for task completion (soft chime)
- Lower thirds for command explanations
- Caption all terminal output for accessibility
- Add chapter markers at each section break
- Background music: ambient electronic, low volume, no lyrics
- Color grade: slightly warm, high contrast for terminal visibility
