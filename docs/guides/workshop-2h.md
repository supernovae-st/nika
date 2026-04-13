# Workshop: Master Nika -- From Zero to Agent Orchestration

> 2-hour hands-on workshop
> Target audience: developers with YAML familiarity, some AI/API experience helpful
> Format: theory slides + 6 hands-on exercises + live coding segments
> Prerequisites: Rust toolchain installed, at least one LLM API key
> Class size: 10-30 participants

---

## Pre-Workshop Setup Email (send 48 hours before)

```
Subject: Nika Workshop Setup -- 3 Things to Install

1. Install Rust: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
2. Install Nika: cargo install nika
3. Set an API key (any one):
   export ANTHROPIC_API_KEY=sk-ant-...     (recommended)
   export OPENAI_API_KEY=sk-...
   export GROQ_API_KEY=gsk_...

Verify: nika --version && nika provider list

See you there!
```

---

## Materials Checklist

- [ ] Slide deck (theory segments)
- [ ] Exercise repository with starter files
- [ ] Solution files for all 6 exercises (do not share until after each exercise)
- [ ] Backup API keys for participants who do not have one
- [ ] Wi-Fi credentials posted visibly
- [ ] USB drive with pre-compiled Nika binary (backup for slow internet)

---

## MINUTE-BY-MINUTE AGENDA

```
0:00 - 0:10   Welcome + Setup Verification
0:10 - 0:25   Theory 1: What is Nika? (5 verbs, architecture)
0:25 - 0:45   Exercise 1: First Workflow (exec + fetch)
0:45 - 0:55   Theory 2: Data Flow (depends_on, with:, templates)
0:55 - 1:10   Exercise 2: Chaining Tasks (data flow pipeline)
1:10 - 1:15   BREAK (5 minutes)
1:15 - 1:25   Theory 3: LLM Integration (infer, structured output)
1:25 - 1:45   Exercise 3: AI Generation (infer + structured output)
1:45 - 1:55   Theory 4: Advanced Features (media, MCP, agents)
1:55 - 2:10   Exercise 4: Media Pipeline (invoke + nika: tools)
2:10 - 2:15   BREAK (5 minutes)
2:15 - 2:25   Exercise 5: Agent Loop (agent verb)
2:25 - 2:40   Exercise 6: Capstone (full production workflow)
2:40 - 2:50   Theory 5: Ecosystem + Next Steps
2:50 - 3:00   Q&A + Closing
```

*Note: scheduled for 3 hours with buffer. Actual instruction time is 2 hours. Breaks and buffer account for the rest.*

---

## 0:00 - 0:10 | WELCOME + SETUP VERIFICATION

### Slide: Welcome

[SLIDE] "Master Nika: From Zero to Agent Orchestration" with butterfly logo.

**Speaker Script:**
Welcome everyone. Today we are going from zero to building a multi-step AI workflow with agents, media processing, and structured output. By the end of this workshop, you will have built six exercises of increasing complexity, and you will understand the architecture well enough to build your own.

### Setup Check

[SCREEN] Terminal.

**Speaker Script:**
Let's verify everyone's setup. Open your terminal and run these two commands.

```bash
nika --version
# Expected: nika 0.42.0

nika provider list
# Expected: at least one provider with a green checkmark
```

**Troubleshooting table:**

| Problem | Solution |
|---------|----------|
| `command not found` | `cargo install nika` or use USB backup binary |
| No green providers | `export ANTHROPIC_API_KEY=...` (provide backup key) |
| Cargo fails to compile | Use pre-built binary from USB drive |

**Speaker Script:**
If you see a version number and at least one green provider, you are ready. If not, raise your hand and we will get you sorted. Meanwhile, create a workshop directory.

```bash
mkdir nika-workshop && cd nika-workshop
```

---

## 0:10 - 0:25 | THEORY 1: What is Nika?

### Slide: The Problem

[SLIDE] "AI workflows today: 200 lines of Python to chain two API calls."

**Speaker Script:**
Before we write any YAML, let me give you the thirty-second pitch. Every AI workflow framework today is Python-based, discovers errors at runtime, and requires learning a framework-specific API. Nika takes a different approach: five verbs in YAML, validated before execution, executed in parallel on a Rust runtime.

### Slide: Five Verbs

[SLIDE] Five-verb table:

```
infer:    LLM generation (8 providers, vision, structured output)
exec:     Shell commands (blocklist, timeout)
fetch:    HTTP requests (9 extraction modes)
invoke:   MCP tool calls (schema validation, retry)
agent:    Multi-turn loops (guardrails, completion modes)
```

**Speaker Script:**
Five verbs. That is the entire vocabulary. Every Nika task uses exactly one verb. `infer:` calls an LLM. `exec:` runs a command. `fetch:` makes an HTTP request. `invoke:` calls an MCP tool. `agent:` creates a multi-turn autonomous loop.

We will use four of these five today. The fifth -- invoke with external MCP servers -- requires a running MCP server, but we will use invoke with builtin tools.

### Slide: Architecture (30 seconds)

[SLIDE] Simplified pipeline:

```
.nika.yaml --> Parse --> Analyze --> DAG --> Execute (parallel) --> Results
```

**Speaker Script:**
Nika processes your YAML through a three-phase pipeline: parse it with source spans, analyze and validate it, build a dependency graph, then execute tasks in parallel. Errors are caught at the analysis phase -- before any execution, before any API calls.

### Slide: The Simplest Workflow

[SLIDE] Four-line example:

```yaml
schema: nika/workflow@0.12

tasks:
  - id: hello
    exec:
      command: echo "Hello, Nika!"
```

**Speaker Script:**
This is the simplest possible workflow. Schema declaration, one task, the exec verb. Four meaningful lines. Let's build something more interesting.

---

## 0:25 - 0:45 | EXERCISE 1: First Workflow

### Exercise Brief

[SLIDE] "Exercise 1: System Info Collector" -- 20 minutes

**Objective:** Write a workflow that collects system information using exec and fetch.

**Requirements:**
1. Create `ex1-sysinfo.nika.yaml`
2. Task 1: Use `exec:` to get the current date
3. Task 2: Use `exec:` to get the hostname
4. Task 3: Use `exec:` to count files in the current directory
5. Task 4: Use `fetch:` to get your public IP from `https://api.ipify.org`
6. Run `nika check` to validate
7. Run `nika run` to execute

**Starter file:**

```yaml
schema: nika/workflow@0.12

tasks:
  - id: get_date
    exec:
      command: # YOUR COMMAND HERE

  # Add 3 more tasks...
```

### Live Coding (walk through first task)

[SCREEN] Write the first task together:

```yaml
  - id: get_date
    exec:
      command: date +"%Y-%m-%d %H:%M"
```

[TYPE] `nika check ex1-sysinfo.nika.yaml`
[TYPE] `nika run ex1-sysinfo.nika.yaml`

**Speaker Script:**
Write the first task with me. `id: get_date`, `exec:`, `command: date`. Let's validate and run. Now you have the pattern -- add three more tasks. You have fifteen minutes.

### Expected Solution

```yaml
schema: nika/workflow@0.12

tasks:
  - id: get_date
    exec:
      command: date +"%Y-%m-%d %H:%M"

  - id: get_hostname
    exec:
      command: hostname

  - id: count_files
    exec:
      command: ls -1 | wc -l

  - id: get_ip
    fetch:
      url: https://api.ipify.org
```

### Debrief Points

- All four tasks ran in parallel (no depends_on)
- The fetch task required no configuration beyond the URL
- `nika check` caught errors before execution
- Total execution time < 1 second

---

## 0:45 - 0:55 | THEORY 2: Data Flow

### Slide: depends_on

[SLIDE] DAG example:

```yaml
  - id: task_b
    depends_on: [task_a]    # task_b waits for task_a
```

```
[task_a] --> [task_b]
```

**Speaker Script:**
`depends_on` creates edges in the dependency graph. task_b will not start until task_a completes. Without depends_on, tasks run in parallel. With it, they run in order.

### Slide: with: Bindings

[SLIDE] Binding flow:

```yaml
  - id: summarize
    depends_on: [fetch_data]
    with: { data: $fetch_data }     # $ prefix = task reference
    exec:
      command: echo "Data: {{with.data}}"  # {{ }} = template
```

```
Three parts:
  1. depends_on: [fetch_data]  -- ordering guarantee
  2. with: { data: $fetch_data }  -- data binding ($ prefix)
  3. {{with.data}}  -- template resolution
```

**Speaker Script:**
Data flow has three parts. `depends_on` guarantees ordering. `with:` creates a named binding -- dollar sign prefix means "reference a task result." Double-brace templates resolve at runtime. All three are validated at analysis time. Misspell a task ID? NIKA-075 with the line number.

### Slide: Pipe Transforms

[SLIDE] Transform examples:

```yaml
{{with.data | uppercase}}             -- HELLO WORLD
{{with.data | trim | word_count}}     -- 42
{{with.data | truncate(100)}}         -- First 100 chars...
```

```
27 transforms available: uppercase, lowercase, trim,
reverse, word_count, truncate, base64_encode, ...
```

**Speaker Script:**
Pipe transforms process data inline. Twenty-seven available. Chain them with the pipe operator. Applied at template resolution time.

---

## 0:55 - 1:10 | EXERCISE 2: Chaining Tasks

### Exercise Brief

[SLIDE] "Exercise 2: Weather Report Pipeline" -- 15 minutes

**Objective:** Build a multi-task pipeline with data flow between tasks.

**Requirements:**
1. Create `ex2-weather.nika.yaml`
2. Task 1: Fetch weather data from `https://wttr.in/Paris?format=j1`
3. Task 2: Extract the temperature using `extract: jsonpath` with `selector: "$.current_condition[0].temp_C"`
4. Task 3: Depends on tasks 1 and 2. Use `exec:` to format a report string using `with:` bindings from both tasks
5. Validate and run

**Hint:** You can bind multiple tasks: `with: { raw: $task1, temp: $task2 }`

### Expected Solution

```yaml
schema: nika/workflow@0.12

tasks:
  - id: raw_weather
    fetch:
      url: https://wttr.in/Paris?format=j1

  - id: temperature
    fetch:
      url: https://wttr.in/Paris?format=j1
      extract: jsonpath
      selector: "$.current_condition[0].temp_C"

  - id: report
    depends_on: [raw_weather, temperature]
    with: { temp: $temperature }
    exec:
      command: echo "Paris temperature is {{with.temp | trim}} degrees Celsius"
```

### Debrief Points

- `raw_weather` and `temperature` ran in parallel (independent fetches)
- `report` waited for both to complete
- `{{with.temp | trim}}` removed whitespace from the JSON value
- The DAG had two layers: Layer 0 (two fetches), Layer 1 (report)

---

## 1:10 - 1:15 | BREAK

**Speaker Script:**
Five-minute break. Stretch, refill water, ask questions. We are about to bring in the LLMs.

---

## 1:15 - 1:25 | THEORY 3: LLM Integration

### Slide: The infer: Verb

[SLIDE] infer example:

```yaml
  - id: poem
    infer:
      model: claude/claude-haiku-3-5-20241022
      prompt: "Write a haiku about Rust programming."
```

```
model: provider/model-name
  - claude/claude-sonnet-4-20250514
  - openai/gpt-4o
  - groq/llama-3.3-70b-versatile
  - native/mistral-7b (local, offline)
```

**Speaker Script:**
The `infer:` verb calls an LLM. The `model:` field uses slash syntax -- provider slash model name. Eight providers available. Change the model, keep everything else the same.

### Slide: Structured Output

[SLIDE] Output schema:

```yaml
  - id: analyze
    infer:
      model: claude/claude-haiku-3-5-20241022
      prompt: "Analyze the weather for outdoor activities."
      output:
        format: json
        schema:
          type: object
          properties:
            suitable: { type: boolean }
            activities: { type: array, items: { type: string } }
            warning: { type: string }
          required: [suitable, activities]
```

**Speaker Script:**
The `output:` block enforces structure. Define a JSON schema, and Nika guarantees the LLM output conforms. Five-layer defense: provider-native structured output, extraction, validation, retry, LLM self-repair. You define what you want. Nika ensures you get it.

### Slide: System Prompts and Parameters

[SLIDE] Additional options:

```yaml
      system: "You are a meteorologist."
      temperature: 0.3
      max_tokens: 500
```

**Speaker Script:**
System prompts, temperature, max_tokens -- all in YAML. No builder patterns. No method chains.

---

## 1:25 - 1:45 | EXERCISE 3: AI Generation

### Exercise Brief

[SLIDE] "Exercise 3: AI Weather Analyst" -- 20 minutes

**Objective:** Build an AI-powered weather analysis pipeline with structured output.

**Requirements:**
1. Create `ex3-analyst.nika.yaml`
2. Task 1: Fetch weather from `https://wttr.in/Tokyo?format=j1` with `extract: jsonpath` and `selector: "$.current_condition[0]"`
3. Task 2: Depends on task 1. Use `infer:` to analyze the weather. Include a `system:` prompt establishing the AI as a weather expert. Use `with:` to pass the weather data.
4. Task 3: Depends on task 2. Use `infer:` with structured output to generate a recommendation:
   - JSON schema with: `recommendation` (string), `confidence` (number 0-1), `activities` (array of strings)
   - All three fields required
5. Validate and run

**Challenge bonus:** Add `temperature: 0.2` to the recommendation task for more consistent output.

### Expected Solution

```yaml
schema: nika/workflow@0.12

tasks:
  - id: weather
    fetch:
      url: https://wttr.in/Tokyo?format=j1
      extract: jsonpath
      selector: "$.current_condition[0]"

  - id: analysis
    depends_on: [weather]
    with: { conditions: $weather }
    infer:
      model: claude/claude-haiku-3-5-20241022
      system: You are an expert meteorologist. Analyze weather data concisely.
      prompt: |
        Current conditions in Tokyo:
        {{with.conditions}}

        Provide a brief weather analysis.

  - id: recommendation
    depends_on: [analysis]
    with: { analysis: $analysis }
    infer:
      model: claude/claude-haiku-3-5-20241022
      temperature: 0.2
      prompt: |
        Weather analysis: {{with.analysis}}

        Based on this analysis, provide an activity recommendation.
      output:
        format: json
        schema:
          type: object
          properties:
            recommendation:
              type: string
              description: One-paragraph recommendation
            confidence:
              type: number
              minimum: 0
              maximum: 1
            activities:
              type: array
              items: { type: string }
          required: [recommendation, confidence, activities]
```

### Debrief Points

- Three-task sequential pipeline: fetch -> analyze -> recommend
- Structured output guaranteed valid JSON matching the schema
- System prompts shape the LLM's behavior
- Low temperature for consistency in the final recommendation
- This pipeline cost roughly $0.002 total (two Haiku calls)

---

## 1:45 - 1:55 | THEORY 4: Advanced Features

### Slide: Media Tools

[SLIDE] Three-tier overview:

```
24 tools via invoke: nika:*

nika:import     -- Bring files into CAS (content-addressable storage)
nika:thumbnail  -- SIMD-accelerated resize
nika:convert    -- Format conversion (PNG/JPEG/WebP)
nika:metadata   -- EXIF extraction
nika:pipeline   -- Chain operations in-memory
nika:chart      -- Generate charts from JSON data
```

**Speaker Script:**
Twenty-four media tools, accessed through the invoke verb with the `nika:` prefix. They use content-addressable storage -- import a file, get a hash, operate on hashes. No path juggling, no temporary files.

### Slide: MCP Integration

[SLIDE] invoke example:

```yaml
  - id: search
    invoke:
      tool: novanet_search
      input:
        query: "weather patterns"
```

**Speaker Script:**
The invoke verb calls MCP tools. Any MCP-compatible server. Nika's builtin tools use the `nika:` prefix. External tools use their registered names. Schema validation before the call. Connection pooling and retry built in.

### Slide: The Agent Verb

[SLIDE] Agent example:

```yaml
  - id: researcher
    agent:
      model: claude/claude-sonnet-4-20250514
      goal: "Research the best hiking trails near Tokyo"
      skills:
        - tool: nika:log
          description: "Log a message"
      max_turns: 5
      completion:
        mode: natural
```

**Speaker Script:**
The agent verb creates multi-turn loops. The LLM receives a goal and available tools. It decides which tools to call and when to stop. `max_turns` is a safety limit. `completion.mode: natural` means the agent stops when it decides it has enough information. Guardrails can validate each response.

---

## 1:55 - 2:10 | EXERCISE 4: Media Pipeline

### Exercise Brief

[SLIDE] "Exercise 4: Image Analysis Pipeline" -- 15 minutes

**Objective:** Build a media pipeline that imports, processes, and analyzes an image.

**Requirements:**
1. Create `ex4-media.nika.yaml`
2. Save any JPEG or PNG image as `sample.jpg` in your directory
3. Task 1: Import the image with `invoke: nika:import`
4. Task 2: Generate a thumbnail (400x300) with `invoke: nika:thumbnail`
5. Task 3: Extract colors with `invoke: nika:dominant_color` (count: 3)
6. Task 4: Depends on tasks 2 and 3. Use `infer:` with `content:` to describe the image, mentioning the dominant colors.
7. Tasks 2 and 3 should run in parallel

### Expected Solution

```yaml
schema: nika/workflow@0.12

tasks:
  - id: import
    invoke:
      tool: nika:import
      input:
        path: ./sample.jpg

  - id: thumb
    depends_on: [import]
    with: { img: $import }
    invoke:
      tool: nika:thumbnail
      input:
        hash: "{{with.img.hash}}"
        width: 400
        height: 300

  - id: colors
    depends_on: [import]
    with: { img: $import }
    invoke:
      tool: nika:dominant_color
      input:
        hash: "{{with.img.hash}}"
        count: 3

  - id: describe
    depends_on: [thumb, colors]
    with: { thumbnail: $thumb, palette: $colors }
    infer:
      model: claude/claude-sonnet-4-20250514
      content:
        - type: image
          source: "{{with.thumbnail.hash}}"
        - type: text
          text: |
            This image has dominant colors: {{with.palette}}
            Describe what you see and how the colors contribute to the mood.
```

### Debrief Points

- CAS storage: file imported once, referenced by hash everywhere
- Parallel execution: thumbnail and color extraction ran simultaneously
- Vision: Claude sees the thumbnail through the CAS hash (auto-resolved to base64)
- The DAG had three layers: import -> [thumb, colors] -> describe

---

## 2:10 - 2:15 | BREAK

**Speaker Script:**
Final break. Two more exercises and we are done. The next one introduces the agent verb.

---

## 2:15 - 2:25 | EXERCISE 5: Agent Loop

### Exercise Brief

[SLIDE] "Exercise 5: Research Agent" -- 10 minutes

**Objective:** Build a simple agent that uses tools to complete a research task.

**Requirements:**
1. Create `ex5-agent.nika.yaml`
2. Task 1: Use `agent:` to research a topic
3. Give the agent the `nika:log` tool (for recording findings)
4. Set `max_turns: 5`
5. Set `completion.mode: natural`
6. The goal should be: "List 3 benefits of writing workflow engines in Rust"

### Expected Solution

```yaml
schema: nika/workflow@0.12

tasks:
  - id: researcher
    agent:
      model: claude/claude-haiku-3-5-20241022
      goal: |
        List 3 key benefits of writing workflow engines in Rust
        instead of Python. Be specific and technical.
      skills:
        - tool: nika:log
          description: "Log a finding to the console"
      max_turns: 5
      completion:
        mode: natural
```

### Debrief Points

- The agent decided when to stop (natural completion)
- max_turns is a safety net, not a target
- The agent used nika:log to record its reasoning
- In production, agents would have access to external tools via MCP

---

## 2:25 - 2:40 | EXERCISE 6: Capstone

### Exercise Brief

[SLIDE] "Exercise 6: Capstone -- News Briefing Generator" -- 15 minutes

**Objective:** Build a production-quality workflow combining all concepts.

**Requirements:**
1. Create `ex6-capstone.nika.yaml`
2. Task 1: Fetch HackerNews top stories API (`https://hacker-news.firebaseio.com/v0/topstories.json`)
3. Task 2: Fetch a second news source (your choice -- Reddit API, RSS feed, etc.)
4. Task 3: Depends on tasks 1 and 2. Use `infer:` to generate a news briefing. Include a `system:` prompt. Use structured output with a schema containing `headline` (string), `summary` (string), and `sources` (array of strings).
5. Task 4: Depends on task 3. Use `exec:` to save the briefing with: `echo "{{with.brief}}" > briefing.txt`
6. Ensure maximum parallelism (tasks 1 and 2 should be parallel)
7. Run and verify the output file exists

### Expected Solution

```yaml
schema: nika/workflow@0.12

tasks:
  - id: hackernews
    fetch:
      url: https://hacker-news.firebaseio.com/v0/topstories.json
      extract: jsonpath
      selector: "$[0:5]"

  - id: reddit
    fetch:
      url: https://www.reddit.com/r/technology/hot.json?limit=5
      headers:
        User-Agent: "NikaWorkshop/1.0"
      extract: jsonpath
      selector: "$.data.children[*].data.title"

  - id: briefing
    depends_on: [hackernews, reddit]
    with: { hn: $hackernews, rd: $reddit }
    infer:
      model: claude/claude-haiku-3-5-20241022
      system: |
        You are a tech news editor. Write concise, insightful briefings.
      prompt: |
        HackerNews top story IDs: {{with.hn}}
        Reddit technology headlines: {{with.rd}}

        Write a morning tech news briefing.
      output:
        format: json
        schema:
          type: object
          properties:
            headline: { type: string }
            summary: { type: string }
            sources:
              type: array
              items: { type: string }
          required: [headline, summary, sources]

  - id: save
    depends_on: [briefing]
    with: { brief: $briefing }
    exec:
      command: echo '{{with.brief}}' > briefing.json
```

### Debrief Points

- Four tasks, three layers of execution
- Two data sources fetched in parallel
- LLM synthesis with structured output
- File output for persistence
- This is a production-viable pattern for automated briefings

---

## 2:40 - 2:50 | THEORY 5: Ecosystem + Next Steps

### Slide: The Course

[SLIDE] Course overview:

```bash
nika init --course     # Generate 12-level course (44 exercises)
nika course status     # Constellation progress map
nika course next       # Open next exercise
nika course hint       # Progressive hints (3 tiers)
nika course watch      # Auto-check on file save
```

**Speaker Script:**
Today you built six workflows. The Nika course has forty-four more, organized in twelve progressive levels. If you want to go deeper, `nika init --course` generates the entire thing in your project directory. Each level builds on the previous, from basic commands to full agent orchestration.

### Slide: The Showcase

[SLIDE] Showcase commands:

```bash
nika showcase list              # 200+ working examples
nika showcase extract <name>    # Pull to your project
```

**Speaker Script:**
Over two hundred showcase workflows. Working, tested, ready to run. Browse them, extract the ones you need, customize them. They cover fetch patterns, LLM workflows, media pipelines, and infrastructure automation.

### Slide: The TUI

[SLIDE] TUI screenshots:

```
nika ui     -- Launch the terminal UI
  1/s: Studio view (YAML editor + syntax highlighting + LSP)
  2/c: Command view
  3/x: Control view
```

**Speaker Script:**
The TUI is a full terminal application. Studio view for editing with syntax highlighting and LSP completions. Monitor view for watching execution in real-time with DAG visualization. Forty-plus custom widgets. Try `nika ui` when you get home.

### Slide: Resources

[SLIDE] Next steps:

```
github.com/supernovae-st/nika     -- Source code
nika init --course                  -- 44 exercises
nika showcase list                  -- 200+ examples
nika init --minimal                 -- 5 starter workflows
```

---

## 2:50 - 3:00 | Q&A + CLOSING

**Speaker Script:**
That is the workshop. You built six workflows using four of the five verbs. You learned data flow with depends_on and with: bindings. You used structured output to guarantee LLM response formats. You processed images through the media pipeline. You created an agent loop. And you built a production-viable news briefing generator.

Questions?

### Common Workshop Questions

**Q: My fetch task returned an error.**
A: Check the URL is accessible. Some APIs require headers (User-Agent for Reddit). Try `curl <url>` first to verify.

**Q: My structured output does not match the schema.**
A: Nika retries automatically (up to layer 4). If it still fails, simplify the schema or use a more capable model.

**Q: Can I use GPT-4 instead of Claude?**
A: Yes. Change `claude/claude-haiku-3-5-20241022` to `openai/gpt-4o`. Set `OPENAI_API_KEY`.

**Q: How do I debug a failing workflow?**
A: `nika check` first. Then `nika run --trace` for NDJSON event logs. The TUI Monitor view shows live execution.

**Q: What about error handling in workflows?**
A: Use `timeout:` for time limits. `retry:` for automatic retries. `fail_fast: false` at the workflow level to continue on failure. The agent verb has `max_turns` as a safety limit.

---

## Workshop Feedback Form

```
1. Experience level with AI tools (1-5):
2. Which exercise was most valuable?
3. Which exercise was most challenging?
4. Would you use Nika in a real project? Why/why not?
5. What topic would you want in a follow-up workshop?
6. Overall rating (1-5):
```

---

## Instructor Notes

- Exercise timing is approximate. If the group is fast, spend extra time on the capstone. If slow, skip Exercise 5 (agent) and go directly to the capstone.
- Have backup API keys ready. At least two participants will not have keys set up.
- The media exercise (Exercise 4) requires a local image file. Have sample images ready on a USB drive or downloadable URL.
- Walk the room during exercises. Common issues: YAML indentation, missing depends_on, misspelled task IDs.
- The error messages are your friend. When a participant has an issue, show them the NIKA-XXX error code and how it points to the exact line.
- If internet is unreliable, have cached API responses ready as local files and use exec: cat instead of fetch.
