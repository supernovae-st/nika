# Wave 9: DX, Functional Testing, TUI Verification, New User Experience

> **Copy-paste everything below into a fresh Claude Code chat.**

---

# REVIEW: Full DX + Functional + TUI Audit — Nika v0.41.5

## Context

Nika is a semantic YAML workflow engine for AI tasks at
`/Users/thibaut/dev/supernovae/nika/tools/` (11 crates, ~320K LOC).
The codebase has been through 8 waves of bug hunting — zero production bugs
remain. Now we shift from "find bugs" to "verify everything works end-to-end".

## WHAT TO DO

Launch **5 parallel agents**, each exploring a different axis. Each agent
should READ code, RUN commands, and REPORT findings. No code changes.

---

## AGENT 1: New User Experience (DX Smoke Test)

Simulate what a new user encounters. Run each command and report what happens.

```bash
cd /tmp && mkdir nika-test-user && cd nika-test-user

# 1. First contact — does help work?
nika --help
nika --version

# 2. Init — does the wizard work?
nika init --minimal

# 3. Check — does validation work on generated files?
nika check *.nika.yaml

# 4. Provider list — does it show available providers?
nika provider list

# 5. Course — does the learning path work?
nika init --course
nika course status
nika course info 1

# 6. Showcase — does browsing work?
nika showcase list | head -20

# Cleanup
rm -rf /tmp/nika-test-user
```

For each command, report:
- Does it run without errors?
- Is the output clear and helpful for a new user?
- Are there any confusing messages, missing info, or crashes?
- Does it handle missing API keys gracefully (no panic)?

---

## AGENT 2: Workflow Validation Pipeline

Test `nika check` on various workflow patterns to verify the analyzer catches
errors correctly and produces helpful messages.

Create temporary test files and run `nika check` on them:

```yaml
# Test 1: Valid minimal workflow
schema: nika/workflow@0.12
tasks:
  - id: hello
    exec: echo "Hello World"

# Test 2: Missing schema
tasks:
  - id: hello
    exec: echo "no schema"

# Test 3: Unknown verb
schema: nika/workflow@0.12
tasks:
  - id: bad
    generate: "This verb doesn't exist"

# Test 4: Duplicate task IDs (should fail at parse now)
schema: nika/workflow@0.12
tasks:
  - id: step1
    exec: echo "first"
  - id: step1
    exec: echo "duplicate"

# Test 5: Circular dependency
schema: nika/workflow@0.12
tasks:
  - id: a
    exec: echo "a"
    depends_on: [b]
  - id: b
    exec: echo "b"
    depends_on: [a]

# Test 6: Missing model on infer
schema: nika/workflow@0.12
tasks:
  - id: gen
    infer: "Hello"

# Test 7: schema without format: json (should warn)
schema: nika/workflow@0.12
tasks:
  - id: structured
    infer: "Generate JSON"
    model: gpt-4o
    output:
      schema:
        type: object
        properties:
          name: { type: string }

# Test 8: timeout: 0 (should warn)
schema: nika/workflow@0.12
tasks:
  - id: fast
    exec: echo "too fast"
    timeout: 0

# Test 9: for_each + decompose coexistence (should warn)
schema: nika/workflow@0.12
tasks:
  - id: both
    infer: "Process {{item}}"
    model: gpt-4o
    for_each:
      items: '["a","b"]'
    decompose: "Split into subtasks"

# Test 10: Empty workflow (no tasks)
schema: nika/workflow@0.12
```

For each test, report:
- Does `nika check` produce the right error/warning?
- Is the error message helpful (points to line, suggests fix)?
- Are there false positives or missed errors?

---

## AGENT 3: TUI Startup + Rendering Verification

Read and analyze TUI startup code for correctness:

1. **Read these files carefully:**
   - `nika-tui/src/lib.rs` (entry points: run_tui, run_tui_standalone, run_tui_chat)
   - `nika-tui/src/startup.rs` (pre-flight checks)
   - `nika-tui/src/app/lifecycle.rs` (init/cleanup)
   - `nika-tui/src/app/render.rs` (frame rendering)
   - `nika-tui/src/views/studio/mod.rs` (Studio view init)
   - `nika-tui/src/views/command.rs` (Command view init)

2. **Verify these scenarios don't crash:**
   - Terminal smaller than 60x15 → should show "too small" overlay
   - No .nika.yaml files in directory → should show empty browser
   - No API keys configured → should show provider status
   - First run ever (no ~/.nika/ dir) → should create dirs
   - Ctrl+C during startup → should restore terminal

3. **Check widget bounds:**
   - Read `nika-tui/src/widgets/task_box/` — does it handle empty task names?
   - Read `nika-tui/src/widgets/dag_ascii.rs` — does it handle 0 tasks? 100+ tasks?
   - Read `nika-tui/src/widgets/header.rs` — does it truncate long workflow names?
   - Read status_bar.rs — does it handle narrow terminals?

4. **Check event processing:**
   - Read `nika-tui/src/state/event_handler.rs` — what happens with out-of-order events?
   - Does TaskCompleted before TaskStarted crash?
   - Does ForEachCompleted with 0 items crash?

Report any code paths that could panic or render garbage.

---

## AGENT 4: Exec/Fetch/Invoke Verb Wiring

Verify that the 5 verbs are correctly wired from YAML → parser → analyzer → lower → executor.

For each verb (infer, exec, fetch, invoke, agent):

1. **Trace the pipeline:**
   - Parser: `nika-core/src/ast/raw/parser.rs` — all fields parsed correctly?
   - Analyzer: `nika-core/src/ast/analyzer/analyze.rs` — all fields validated?
   - Lower: `nika-engine/src/ast/lower.rs` — all fields lowered?
   - Executor: `nika-engine/src/runtime/executor/verbs.rs` — all fields used?

2. **Check for field drops:**
   - Is there any field parsed but never used in execution?
   - Is there any field expected by executor but never parsed?
   - Are alias fields (timeout/timeout_ms, max_retries/max_attempts) consistent?

3. **Check exec specifically:**
   - Is `shell: true` correctly wiring to `sh -c` vs direct exec?
   - Is `cwd:` propagated?
   - Is `env:` propagated?
   - Is timeout enforced?

4. **Check fetch specifically:**
   - Are all 9 extract modes wired?
   - Is `response: binary` → CAS pipeline working?
   - Is `follow_redirects:` propagated?
   - Is retry config applied to fetch?

5. **Check invoke specifically:**
   - Is MCP server name resolved correctly?
   - Are params template-resolved then type-coerced?
   - Is timeout enforced per-invoke?

---

## AGENT 5: Course + Showcase Content Verification

Verify the generated course and showcase workflows are valid:

```bash
cd /tmp && mkdir nika-course-test && cd nika-course-test

# Generate course
nika init --course

# Check ALL 44 exercise files
find . -name "*.nika.yaml" -exec nika check {} \;

# List showcases
nika showcase list

# Extract a few and check them
nika showcase extract hello-world
nika check hello-world.nika.yaml

nika showcase extract media-pipeline
nika check media-pipeline.nika.yaml

nika showcase extract agent-research
nika check agent-research.nika.yaml

# Cleanup
rm -rf /tmp/nika-course-test
```

For each workflow, report:
- Does it pass `nika check`?
- If it fails, what's the error?
- Are there common patterns of failure (missing model, wrong schema version)?

---

## HOW TO WORK

1. Launch all 5 agents in parallel
2. Each agent does READ + RUN (no code changes)
3. Collect findings into a single report
4. Categorize: BROKEN (must fix), WARNING (should fix), OK (works correctly)
5. Create a prioritized fix list

## OUTPUT FORMAT

For each agent, produce:
```
## Agent N: [Name]

### BROKEN
- [ ] description (file:line if applicable)

### WARNING
- [ ] description

### OK
- [x] feature works correctly
```
